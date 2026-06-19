# Relay Persistence / Relay 持久化

## Problem / 问题

NexChat cloud sync stores **encrypted blobs on IPFS** and **pointers (account → CID)** on the Relay.
Before persistence, Relay kept pointers in memory only — any restart wiped `contacts_put`, `index_put`, and `msg_archive_put`, breaking reinstall recovery.

NexChat 云同步把**加密数据放在 IPFS**，**指针（账户 → CID）放在 Relay**。
此前 Relay 指针仅存内存，重启即丢，重装后无法恢复通讯录与聊天记录。

## Design / 方案

**WAL (write-ahead log) + atomic snapshot + backup**

```
Client                    Relay                         Disk
  │ contacts_put ───────► │ append journal (fsync) ───► relay-journal.ndjson
  │                       │ debounced snapshot ───────► relay-state.json
  │                       │                           relay-state.json.bak
  │ contacts_fetch ◄──────│ load: snapshot + replay journal
```

### Durability tiers / 持久化分级

| Data | Strategy | Rationale |
|------|----------|-----------|
| `index_put`, `contacts_put`, `msg_archive_put` | **Journal fsync on every write** | Cloud-sync pointers; must survive restart |
| `inbox_register` | Journal fsync | Needed for RFC 9474 delivery after restart |
| `spent_add`, `spent_clear` | **Journal fsync** | RFC 9474 anti-replay must survive crash |
| Contact mailbox, MLS control | Debounced snapshot (300ms) + SIGTERM flush | Ephemeral / lower volume |
| **Chat frame mailbox** | Debounced snapshot (300ms) + SIGTERM flush | Offline 1:1/group ciphertext store-and-forward (180d TTL) |

### Security (P0 + P1) / 安全

| Control | Behavior |
|---------|----------|
| Pointer/inbox writes | Require `register_account` with matching SS58 |
| `inbox_register` | Monotonic `epoch`; bump clears `spent` for old inbox |
| Chat frames | Targeted delivery (1:1 / `routeTo` / delivery inbox owner) |
| `admin_stats` | Localhost only, or `admin_secret` matching `RELAY_ADMIN_SECRET` |
| Message size | Max `RELAY_MAX_MSG_BYTES` (default 256KB) |
| Rate limit | `RELAY_RATE_LIMIT` msgs/min per connection (default 120) |
| Spent cap | `RELAY_SPENT_CAP` per inbox (default 50k); overflow rejects token |

### P2 improvements / P2 改进

| Item | Behavior |
|------|----------|
| Fetch-path prune | `contact_fetch` / `group_invite_fetch` only persist when TTL rows removed |
| `inboxById` index | O(1) delivery verify lookup |
| `inbox_lookup` | Returns `revoked_tags` |
| `relay-sync-audit` | Normalizes SS58 to prefix 42 before fetch |
| Journal replay | Logs corrupt line count; journal truncate fsync on snapshot |
| Shared helpers | `relay-ss58.mjs` (audit CLI); relay itself is Rust `relay-rs` (`core`/`server`) |

### Startup / 启动流程

1. Load `relay-state.json` (fallback to `.bak` if corrupt)
2. Replay journal entries with `at > snapshot.saved_at`
3. Prune orphan `spentByInbox` keys (inbox no longer registered)
4. Serve WebSocket

## Configuration / 配置

```bash
# systemd Environment=
RELAY_DATA_DIR=/opt/nexchat-relay/data
RELAY_ADMIN_SECRET=...          # optional; required for admin_stats from non-localhost
RELAY_MAX_MSG_BYTES=262144      # optional
RELAY_RATE_LIMIT=120            # optional, per-connection per minute
RELAY_SPENT_CAP=50000           # optional
# Offline chat frame mailbox (store-and-forward):
RELAY_CHAT_MAILBOX_TTL_MS=15552000000   # optional, default 180 days
RELAY_CHAT_MAILBOX_MAX_FRAMES=5000      # optional, per-account frame cap (LRU trim)
RELAY_CHAT_MAILBOX_MAX_BYTES=268435456  # optional, 256 MiB per-account byte cap
```

Files:

- `$RELAY_DATA_DIR/relay-state.json` — full state snapshot
- `$RELAY_DATA_DIR/relay-state.json.bak` — previous snapshot
- `$RELAY_DATA_DIR/relay-journal.ndjson` — append-only WAL

## Operations / 运维

### Health check

From localhost (or with secret):

```json
{ "type": "admin_stats", "request_id": "1", "admin_secret": "..." }
```

Reply `stats` includes chat mailbox metrics:

| Field | Meaning |
|-------|---------|
| `chatMailboxes` | Accounts with pending offline frames |
| `chatFrames` | Total stored ciphertext frames |
| `chatMailboxBytes` | Sum of stored wire bytes (approx) |
| `chatMaxFramesPerAccount` | Largest per-account mailbox |

CLI helper:

```bash
RELAY_WS=ws://127.0.0.1:8765 RELAY_ADMIN_SECRET=... npm run relay:admin-stats
```

### Offline chat mailbox / 离线聊天邮箱

| Wire op | Auth | Purpose |
|---------|------|---------|
| `chat_fetch` | `register_account` + `account_sig` when `RELAY_STRICT_AUTH=1` | List pending frames (`chat_reply.frames`) |
| `chat_consume` | `register_account` + sig (strict) | Delete processed `dedup_keys` (ops / single-device only) |

WAL: each new offline frame is journaled as `chat_store { account, dedup_key, row }` (fsync before memory apply). Snapshot debounce (300ms) still applies for full-state flush.

Delivery paths for clients:

1. `register_account` → relay `flushChatMailbox` (realtime WS)
2. `chat_fetch` on unlock / visibility poll (weak-network fallback)
3. Client `InboundDedup` + localStore `env.id` dedup (no duplicate UI)

**Do not enable client `chat_consume` by default** — multi-device accounts would lose mail on the first device that consumes. Rely on TTL + LRU caps unless explicitly single-device.

Monitor growth via `admin_stats.chatFrames` and `chatMailboxBytes`; alert if `chatMaxFramesPerAccount` approaches `RELAY_CHAT_MAILBOX_MAX_FRAMES`.

### Audit user sync state

```bash
node scripts/relay-sync-audit.mjs <SS58>...
```

### IPFS encrypted backup / IPFS 加密备份

One-time setup (dev or copy paths to `/opt/nexchat-relay`):

```bash
./scripts/relay-ops-init.sh
RELAY_DATA_DIR=relay-ops/data npm run relay:server   # terminal 1
npm run relay:backup                                  # terminal 2
```

Production systemd (edit `User`, paths, then):

```bash
sudo cp deploy/systemd/nexchat-relay*.service deploy/systemd/nexchat-relay-backup.timer /etc/systemd/system/
sudo cp scripts/relay-backup.env.example /opt/nexchat-relay/relay-backup.env
# set GPG_PASSPHRASE_FILE, RELAY_STOP_CMD, RELAY_START_CMD
sudo systemctl enable --now nexchat-relay nexchat-relay-backup.timer
```

Restore on a new host:

```bash
source /opt/nexchat-relay/relay-backup.env
./scripts/relay-restore-from-ipfs.sh    # uses latest-backup.json CID
# or: ./scripts/relay-restore-from-ipfs.sh bafy...
```

Scripts: `relay-backup-to-ipfs.sh`, `relay-backup-run.sh`, `relay-restore-from-ipfs.sh`, `relay-ops-init.sh`.

**Off-host manifest (ADR §4.2)** / **清单异地存放（ADR §4.2）**: if the relay host dies, the
latest backup CID must survive elsewhere. Set `BACKUP_OFFSITE_CMD` (receives the manifest path
as `$1`) — e.g. `BACKUP_OFFSITE_CMD='scp "$1" backup@offsite:/srv/relay/latest-backup.json'`.
The backup logs a warning when unset. / 主机失联时最新备份 CID 必须在他处可得；配置
`BACKUP_OFFSITE_CMD`（`$1` 为清单路径），未配置时备份脚本会告警。

### Hot-tier multi-location pinning / 热层多点 pin（ADR §5.8, `relay-pinner`）

The hot-tier pinner (`relay-rs` bin `pinner-hot`) watches the relay's plaintext pointer set
(`relay-state.json` snapshot + `relay-journal.ndjson` replay — dual-track, immune to journal
truncation) and pins the referenced CIDs onto a **remote** IPFS node/cluster, so sync blobs
survive the relay host and its co-located IPFS daemon dying together. Keeps 2 generations per
slot; only unreferenced CIDs are unpinned; a wiped relay state never cascades into unpinning.
`pinner-hot` 消费 relay 明文指针集合（快照 + journal 重放双轨，免疫 journal 截断），把
CID pin 到**异机** IPFS 节点/集群——relay 主机与其同机 IPFS 一起挂掉时 sync blob 仍可取回。
每槽位保留 2 代；仅 unpin 无引用 CID；relay 清库不会级联 unpin。

```bash
# MUST point at an IPFS node on a different host than the relay (§5.8 deployment redline)
RELAY_DATA_DIR=/opt/nexchat-relay/data \
PINNER_IPFS_API=http://10.0.0.2:5001 \
npm run relay:pinner
# optional: PINNER_INTERVAL_MS=30000 PINNER_MAX_BLOB_BYTES=10485760 \
#           PINNER_KEEP_GENERATIONS=2 PINNER_UNPIN=1 PINNER_STATE_FILE=...
```

### Persistent-tier on-chain pinning / 持久层链上 pin（ADR §5.8, `relay-chain-pinner`）

The persistent-tier pinner (`relay-rs` bin `pinner-chain`, built with `--features chain`)
periodically scans the same plaintext pointer set and requests a `pallet-storage-service`
**Standard** pin (3 replicas + 24h probes) for every newly referenced CID, signed by the
**operator account** (§5.8 privacy redline: never a user account — that would write the rejected
`AccountId → plaintext CID` mapping back on-chain). Only-additive: a relay wipe never cancels
on-chain pins; pin lifecycle is owned by chain billing.
`pinner-chain` 周期扫描同一明文指针集合，对新引用的 CID 以**运营者账户**发起
`pallet-storage-service` **Standard** pin（3 副本 + 24h 巡检）（§5.8 隐私红线：绝不可用用户
账户——否则把被否决的「AccountId → 明文 CID」映射写回链上）。只增不减：relay 清库不会取消
链上 pin；pin 生命周期由链上计费管理。

```bash
RELAY_DATA_DIR=/opt/nexchat-relay/data \
CHAIN_PINNER_WS=ws://chain-host:9944 \
CHAIN_PINNER_OPERATOR_SURI='//Operator' \
CHAIN_PINNER_SUBJECT_ID=1 \
CHAIN_PINNER_IPFS_API=http://10.0.0.2:5001 \
npm run relay:chain-pinner
# optional: CHAIN_PINNER_INTERVAL_MS=1800000 (30min ≈ anchor cadence, §5.8)
#           CHAIN_PINNER_MAX_BLOB_BYTES=10485760 CHAIN_PINNER_MAX_PER_TICK=50
```

### Disaster-tier Crust ordering / 灾备底 Crust 下单（ADR §5.8, `relay-crust-pinner`）

The disaster-tier pinner (`relay-rs` bin `pinner-crust`) places **daily** storage orders for
newly referenced pointer CIDs through a self-hosted W3Auth Pinning Service (standard IPFS Remote
Pinning API → Crust). Cadence redline: changed CIDs on the daily snapshot only — never per
`*_put`. The PSA token belongs to the **operator seed**; the Crust chain publicly maps
operator → CID for ownerless encrypted blobs (accepted, disclosed in the client privacy note).
`pinner-crust` 经自托管 W3Auth Pinning Service（标准 IPFS Remote Pinning API →
Crust）对新引用的指针 CID **每日**下存储单。节奏红线：仅每日快照中变化的 CID——禁止按
`*_put` 逐次下单。PSA token 属**运营者 seed**；Crust 链上公开 operator → CID（无主加密
blob，可接受泄漏，已写入客户端隐私文案）。

```bash
RELAY_DATA_DIR=/opt/nexchat-relay/data \
CRUST_PIN_ENDPOINT=http://10.0.0.3:3000/psa \
CRUST_PIN_TOKEN=... \
CRUST_PINNER_IPFS_API=http://10.0.0.2:5001 \
npm run relay:crust-pinner
# optional: CRUST_PINNER_INTERVAL_MS=86400000 CRUST_PINNER_MAX_BLOB_BYTES=10485760
```

Ops runbook / 运维要点（§5.8）:

- Crust storage orders expire after ~6 months — enable the W3Auth prepaid pool for
  auto-renewal and **alert on CRU balance**. / Crust 存储单约 6 个月过期——启用 W3Auth
  预付池自动续期并对 **CRU 余额告警**。
- `relay-sync-audit` should verify: each pointer CID retrievable from ≥2 independent
  gateways, cluster pin status, and Crust order status (PSA state + sampled `ipfs.io`
  retrieval). / `relay-sync-audit` 应检查：每个指针 CID 可从 ≥2 个独立网关取回、Cluster
  pin 状态、Crust 订单状态（PSA 状态库 + 抽样 `ipfs.io` 取回）。

Chain + client encrypted anchor (EISA, Layer C): see `pallets/chat/CHAT_SYNC_ANCHOR_ADR.md`.

## Tests / 测试

```bash
npm run test:relay        # Rust relay-rs unit/integration tests (cargo test)
# or
cd relay-rs && cargo test
```

## Why not Redis/Postgres? / 为何不用 Redis/Postgres

Current Relay is a single-process Rust WebSocket server (`relay-rs`) on one VPS.
File-backed WAL + snapshot gives **zero extra infra**, **fsync-level durability**, and **simple ops** (copy `data/` to backup).

当前 Relay 是单机 Rust 进程（`relay-rs`）；文件 WAL + 快照无需额外组件，指针 fsync 落盘，运维只需备份 `data/` 目录。
