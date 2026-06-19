# Relay Server — Rust 重写开发文档 / Rust Rewrite Plan

> **Status / 状态**：✅ DONE — the Rust `relay-rs` is the sole relay in production; the Node
> `scripts/relay-server.mjs` and its modules/tests have been removed. This doc is kept as the
> design/lineage record. / 已完成——Rust `relay-rs` 为生产唯一 relay；Node `relay-server.mjs`
> 及其模块/测试已删除。本文作为设计与溯源记录保留。
> **Original Status / 原状态**：Proposed v0.1（待评审 / pending review）
> **Date / 日期**：2026-06-13
> **Scope / 范围**：把 `nexchat/scripts/relay-server.mjs` 及其模块用 Rust 重写为单二进制服务
> **关联 / Related**：`nexchat/docs/RELAY_PERSISTENCE.md`（Layer A/B）、`pallets/chat/CHAT_SYNC_ANCHOR_ADR.md`（A+B+C 三层）、`docs/CHAT_GROUP_MLS_DESIGN.md`（MLS 群聊）
> **决策前提 / Premise**：群聊与私聊**均用 MLS**（OpenMLS，`nexchat/mls-wasm`）；relay 是协议无关的密文搬运 + MLS 控制面路由，**不引入 Signal、不改加密协议**

---

## 0. TL;DR

CN：本文规划把现有 **Node WebSocket relay** 重写为 **Rust（tokio）单二进制**。这是一次「**同协议、同 wire、同磁盘格式，仅换实现语言 + 并发模型**」的重写，**不是**重新设计 relay 语义。

三条**逐字节兼容**红线（任何一条破坏即回归 ADR 的隐患）：

| 红线 | 必须与 JS 版一致 | 否则破坏 |
|------|------------------|----------|
| **Wire 协议** | 所有 `type` / `_ctrl` / `delivery` 消息字段与语义 | 现有 TS 客户端（`src/relay/relayClient.ts` + `wsRelay.ts`）需改 |
| **磁盘格式** | `relay-state.json`（schema `v=1`）+ `relay-journal.ndjson`（`op` + `at`） | ADR §5.8 `relay-pinner` / `relay-chain-pinner` / `relay-crust-pinner` 失效；§4 备份/恢复失效 |
| **RFC 9474 验签** | RSABSSA-SHA384-PSS-Randomized 验签结果与 `@cloudflare/blindrsa-ts` 一致 | sealed-sender 投递准入误判，1:1 投递全断 |

EN: Rewrite the Node WS relay into a single Rust (tokio) binary. **Same protocol, same wire, same on-disk format** — only the implementation language and concurrency model change. Drop-in compatible with the existing TS client, the JS pinners (ADR §5.8), and ops backup/restore (ADR §4).

**MLS 不变 / MLS unchanged**：relay 现在已对 1:1（`d:` 会话）和群聊（`g:` 会话）统一承载 MLS 控制面（`kp` / `welcome` / `commit` / `mls_ready`）。本次重写**保留这一事实**，不替换为 Signal。

---

## 1. 背景与动机 / Context

### 1.1 现状

- **实现**：`nexchat/scripts/relay-server.mjs`（~700 行）+ 模块：`relay-persistence.mjs`（WAL+快照）、`relay-token-verify.mjs`（RFC 9474）、`relay-chat-mailbox.mjs`、`relay-contact-mailbox.mjs`、`relay-limits.mjs`、`relay-ss58.mjs`、`relay-admin.mjs`。
- **运行形态**：单进程 Node WebSocket 服务（默认 `:8765`），文件 WAL + 快照持久化（`$RELAY_DATA_DIR/`）。
- **职责（ADR Layer A）**：
  - **热 KV 指针**：`index_put` / `contacts_put` / `msg_archive_put`（账户 → `{cid, updated_at}`）。
  - **投递**：1:1 sealed-sender（RFC 9474 盲签准入）+ 群聊 `routeTo` 定向 + MLS 控制面扇出。
  - **离线邮箱**：chat（密文 store-and-forward，180d TTL）、contact（请求/确认）、group-invite、MLS 控制。
  - **inbox 注册册** + spent 反重放（RFC 9474）。
  - `admin_stats`、限频、消息体上限。
- **配套（不属本次重写，但依赖磁盘格式）**：`relay-pinner` / `relay-chain-pinner` / `relay-crust-pinner`（消费持久化流，ADR §5.8）、`relay-backup-*.sh` / `relay-restore-*.sh`（整库备份，ADR §4）、`relay-sync-audit.mjs`。

### 1.2 为什么用 Rust 重写

| 动机 | 说明 |
|------|------|
| **与主仓语言统一** | 主链、`grouprobot`、`mls-wasm` 均 Rust；relay 是少数 Node 残留 |
| **消除验签双实现漂移** | RFC 9474 现在 JS（relay）与未来链上/其它组件各写一份，是真实风险点；Rust 版可与链共享测试向量 |
| **并发与资源** | tokio 多线程 + 显式背压；可把 fsync 移出投递热路径（ADR §14.2「不在扇出关键路径跑重 fsync」） |
| **单二进制部署** | 无 `node_modules`，systemd 直起，镜像更小 |
| **演进基线** | ADR §14.2 的 hot/cold 拆分、共享 KV（Redis/Postgres）演进，Rust 起步更顺 |

### 1.3 非目标 / Non-Goals

- ❌ **不引入 Signal**。1:1 与群聊都用 MLS；relay 仅搬运密文 + 路由 MLS 控制消息。
- ❌ **不改 wire 协议**（必须 drop-in；客户端零改动）。
- ✅ **三个 pinner（hot / chain / crust）纳入 Rust 化**——作为 relay 主体之后的并行 track（P7–P9，详见 §6、§12）；它们消费 relay 磁盘格式，故必须在 relay 持久化兼容（P1）落地后进行。
- ❌ **不重写 backup / restore / audit 等 shell 脚本**（`relay-backup-*.sh` / `relay-restore-*.sh` / `relay-sync-audit.mjs`）——属运维胶水，保持磁盘格式兼容即可继续用。
- ❌ **不引入 Redis/Postgres**（ADR §14.2 的 P2/P3 演进，后续单独立项）。
- ❌ **不承担 Layer B/C 职责**（备份属运维脚本，EISA 属 pallet + 客户端；relay 只做 Layer A）。

---

## 2. 兼容性合同（重写的验收基线）/ Compatibility Contract

> 这是本次重写的**外观面冻结**。Rust 版必须复刻下列全部行为；任何偏差都需在评审中显式记录为「有意变更」。

### 2.1 Wire 协议消息表（必须全部支持）

来源：`relay-server.mjs::handleMessage` + 客户端 `relayClient.ts::ControlMsg`。

| 方向 | `type` / 标记 | 鉴权 | 行为 |
|------|---------------|------|------|
| C→S | `register {id}` | 无 | 绑定 endpoint id |
| C→S | `register_account {id, account}` | 无（写时校验） | 绑定 SS58→endpoint；flush MLS + chat 邮箱 |
| C→S | `index_put {account,cid,updated_at}` | `register_account` 同账户 | LWW 写 + journal fsync + `index_ack` |
| C→S | `index_fetch {account,request_id}` | 无 | `index_reply` |
| C→S | `contacts_put` / `contacts_fetch` | 同上 | 同 index |
| C→S | `msg_archive_put` / `msg_archive_fetch` | 同上 | 同 index |
| C→S | `inbox_register {account,inbox_id,epoch,ipk_n,ipk_e,revoked_tags}` | 同账户 | 单调 epoch；bump 清旧 inbox spent；`inbox_ack` / `inbox_reject(stale_epoch)` |
| C→S | `inbox_lookup {account,request_id}` | 无 | `inbox_reply`（含 `online`、`revoked_tags`） |
| C→S | `contact_fetch` / `contact_consume` | fetch 无 / consume 无 | TTL prune + 列举 / 删除 |
| C→S | `group_invite_fetch` / `group_invite_consume` | 无 | 同上 |
| C→S | `chat_fetch {account}` | 无 | 非破坏性列举 `chat_reply.frames` |
| C→S | `chat_consume {account,dedup_keys}` | 同账户 | 删除（运维/单设备；客户端默认不开）+ `chat_ack` |
| C→S | `admin_stats {request_id,admin_secret}` | localhost 或 secret | `admin_stats_reply.stats` |
| C→S | `_ctrl: contact_req {toAddr,reqId,...}` | 无 | 存 contact 邮箱 + 即时投递 |
| C→S | `_ctrl: contact_ack {toAddr,reqId,...}` | 无 | 同上 |
| C→S | `_ctrl: group_invite {toAddr,inviteId,...}` | 无 | 存 group-invite 邮箱 + 投递 |
| C→S | `_ctrl: kp/welcome/commit/mls_ready (convId d:)` | 无 | MLS 控制邮箱（按 `mlsControlRecipient`）+ 投递 |
| C→S | `_ctrl: token_req/token_sig {toAddr}` | 无 | 直投 toAddr |
| C→S | `_ctrl: hello/kp/commit (convId g:)` | 无 | 群控制面广播给所有在线账户 |
| C→S | data frame（含 `delivery` 或裸 MLS） | `delivery` 走 RFC 9474 验签 | 解析收件人 → 存 chat 邮箱 + 扇出 |
| S→C | `*_ack` / `*_reply` / `*_reject` / `auth_reject` / `admin_reject` | — | 对应回执 |

**收件人解析（`resolveFrameRecipients`）**：`delivery.inboxId`→inbox owner；`routeTo[]`→各 SS58；`convId="d:A:B"`→A、B；`convId="d:A"`→A + 发送者。规范化一律 SS58 **prefix-42**（与 Node relay `relay-ss58.mjs` `RPC_SS58 = 42` 一致；这是 relay 内部键，**非**链前缀）。⚠️ **历史坑**：早期文档误写"273"，Rust 初版照抄导致与磁盘 42 键不匹配、账户分裂——relay 内部键**必须 42**；链/前端的 273 是另一回事（见下）。

**delivery 验签失败回退**：`delete msg.delivery` 后按裸 MLS 帧投递（补 `senderRef = 发送者账户`）——必须保留此回退。

#### 2.1.1 有意追加的多设备扩展（双实现同步）/ Intentional multi-device extensions (both impls)

> 为支撑 `pallets/chat/CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md` 路线 B 的「同账户多设备」前置，relay 增加
> 三处**严格加性、opt-in、默认休眠**的 wire 扩展。**两份实现（Node `relay-server.mjs` 与 `relay-rs`）
> 同步落地**，故仍满足逐字节 parity 红线（无旧客户端发这些字段 → 现网行为不变）。差分测试须把它们当
> **共同基线**而非分歧。EN: additive, opt-in, dormant-by-default; implemented in BOTH relays.

| 扩展 | 触发 | 行为 | 默认（无字段）|
|------|------|------|---------------|
| `echoSelf: true`（data frame）| 客户端显式置位 | chat 邮箱留存**也含发送方账户**（其离线兄弟设备重连补齐）| 排除发送方账户（单设备语义）|
| `convId = "s:<account>"`（`_ctrl`，`t ∈ {kp,welcome,commit,new_device_state}`）| 子群控制帧 | 经 MLS 控制邮箱扇出到该账户**全部设备**（链下、账户内；复用 7d TTL + register_account flush）| 不匹配 → 不路由 |
| `msgId`（`_ctrl` MLS 帧）| 帧带唯一 id | MLS 去重键改用 `t:convId:msgId`（防同 `(t,convId)` 的多条子群 Commit 折叠）| 沿用 `t:convId:identity:toAddr` |
| `mls_vault_put` / `mls_vault_fetch`（指针）| 客户端显式发送 | 第 4 个云同步指针槽 `mlsVaultPointers`（路线 A MLS 托管 vault 指针，设计 §4/§13）；写者鉴权、按 `updated_at` 单调、落 journal、进快照——与 index/contacts/msg_archive 合同一致 | 无字段 → 槽位为空（旧客户端不发） |

落点：Node `relay-server.mjs`（`mlsControlRecipient` / `mlsDedupKey` / `_ctrl` 分发 / frame 存储 `exceptAccount`；
`mls_vault_put`/`mls_vault_fetch` + `mlsVaultPointers` Map + `relay-persistence.mjs` 快照/journal/stats）；
Rust `relay-rs`（`server/{protocol.rs,mailbox.rs}` 同名逻辑 + `PtrKind::MlsVault`；`core/{persistence.rs,types.rs,
journal.rs,desired.rs}` 的 `mls_vault_pointers` 字段 + `MlsVaultPut` op + `mlsVaultPointers` slot；含单测）。
⚠️ 子群回显类扩展（echoSelf / `s:` / msgId）**客户端尚未启用**（路线 B 受 Gate-B 阻断，见设计文档 §13.7）；
`mls_vault` 指针通道**客户端已接线**（`src/relay/mlsVaultPointer.ts` + `mlsVaultSync` + coordinator 第 4 槽），但由
`VITE_MLS_VAULT_ENABLED` 默认关闭，故现网保持休眠。两实现仍逐字节 parity。

> ✅ **wire 多 leaf 控制面 parity 已补齐（2026-06，P6 切换不再被此阻断）/ Wire multi-leaf control-plane parity LANDED**：
> 1:1 Wire 多 leaf（`CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC`）控制面四项现已**两份实现对齐**：① **闸二**（`d:` 的
> `commit` 带 `commit_epoch` → `(conv,epoch)` CAS + `commit_reject{epoch_stale}` + 启动 `reseed_commit_slots`）；
> ② **闸一**意图路由（`commit_intent` / `commit_result` 及 `presence` 经账户自通道 `s:<account>` 扇出，驱动 CD 选举）；
> ③ **对端代 Add** `peer_add_req` 的接收方路由（`d:` 投会话另一方）+ **认证发送者账户盖章**（`_senderAccount`，未认证即丢弃，
> 是 relay-trustless 校验的根）；④ **`mls_backlog_req`** 按需重投某 conv 已存胜出 Commit（确定性追平，仅本人可拉）。
> `relay-rs` 落点：`server/src/protocol.rs` 的 `mls_control_recipient`（`s:` 增 `presence/commit_intent/commit_result`、`d:` 增
> `peer_add_req` 路由）、`mls_dedup_key`（四类按 device/req_id 去重槽）、`_ctrl` 分发（`s:` 类型扩列 + `peer_add_req` 盖章分支）、
> 顶层 `mls_backlog_req` handler；含 3 条新单测（`subgroup_control_routes_gate1_intent_and_presence` /
> `peer_add_req_routes_to_the_other_conv_party` / `mls_dedup_key_for_multileaf_control_types`），`cargo test` 12/12 绿。
> **Production / 生产**：`deploy/systemd/nexchat-relay.service` runs the Rust `relay-rs` binary; wire multi-device paths are E2E-tested against relay-rs.
> EN: all four wire control-plane pieces now in BOTH relays; P6 cutover no longer blocked.
>
> ✅ **P5 差分测试已落地（端到端打 Rust 二进制）/ P5 differential coverage LANDED**：`scripts/relay-rs-wire.differential.test.mjs`
> （`npm run test:relay:rs-wire`）在 `before` 钩子里 `cargo build -p relay-server` 后 **spawn `relay-rs/target/debug/relay-server`**，
> 用与 `relay-server-auth.test.mjs` 相同的真实 WS 客户端 flow 对 **Rust relay** 跑通五条用例：① `peer_add_req` 路由 + `_senderAccount`
> 盖章、② 未认证 `peer_add_req` 丢弃、③ `mls_backlog_req` 重投已存 Commit、④ `mls_backlog_req` 仅本人鉴权、⑤ 闸一 `commit_intent`
> 经 `s:<account>` 扇出到兄弟设备（5/5 绿）。EN: same client scripts that hit the Node relay now also hit the Rust binary end-to-end
> over real WS — the wire multi-leaf flows are confirmed byte-parity before the P6 cutover.

### 2.2 磁盘格式（`relay-persistence.mjs`）

- **快照** `relay-state.json`：`{ v:1, saved_at, indexPointers, contactsPointers, msgArchivePointers, mlsVaultPointers, inboxesByAccount, spentByInbox(obj→array), contactMailbox(reqs/acks), groupInviteMailbox, mlsMailbox, chatMailbox }`。`.tmp` 写入 → fsync → rename → fsync → 复制 `.bak` → fsync dir。（`mlsVaultPointers` 经 `default`/`?? {}` 兼容旧快照。）
- **日志** `relay-journal.ndjson`：每行 `{op, ...payload, at}`；`op ∈ {index_put, contacts_put, msg_archive_put, mls_vault_put, inbox_register, spent_add, spent_clear}`。**append 后立即 fsync**。
- **启动**：加载 `relay-state.json`（损坏回退 `.bak`）→ 重放 `at > saved_at` 的 journal → `pruneOrphanSpent`。
- **快照时机**：debounce 300ms + SIGTERM/SIGINT flush；快照后**清空 journal**（truncate + fsync）。
- ⚠️ **pinner 约束（ADR §5.8）**：`relay-pinner` 采用「journal `at` 偏移 + `relay-state.json` 全量对账」双轨消费，免疫 journal 截断。Rust 版**必须保留** `saved_at` 与每条 journal 的 `at` 字段语义，截断行为不变。

### 2.3 RFC 9474 验签（`relay-token-verify.mjs`）

- 套件：`RSABSSA.SHA384.PSS.Randomized()`（`@cloudflare/blindrsa-ts`）。
- 公钥：JWK `{kty:RSA, n:ipk_n, e:ipk_e, alg:PS384}`。
- `verifyDeliveryFrame` 校验序列（**顺序与短路必须一致**）：字段齐全 → inbox 存在 → `epoch == ib.epoch` → `ct ∉ revoked_tags` → `t ∉ spent` → 有 `p` → 验签 `verify(pk, s, p)` → `addSpentToken`（受 `RELAY_SPENT_CAP` 上限）→ journal `spent_add`。
- token message 编码（`buildTokenMessage`）：`t(32) ‖ ct(32) ‖ epoch(u32, big-endian)` = 68B（供共享测试向量用）。

### 2.4 环境变量（保持同名同默认）

`RELAY_PORT=8765`、`RELAY_MAX_MSG_BYTES=262144`、`RELAY_RATE_LIMIT=120`、`RELAY_ADMIN_SECRET`、`RELAY_DATA_DIR=./data`、`RELAY_SPENT_CAP=50000`、`RELAY_CHAT_MAILBOX_TTL_MS=15552000000`、`RELAY_CHAT_MAILBOX_MAX_FRAMES=5000`、`RELAY_CHAT_MAILBOX_MAX_BYTES=268435456`、`RELAY_DEBUG`。

### 2.5 TTL / 上限常量（保持一致）

| 邮箱 | TTL | 其它 |
|------|-----|------|
| chat | 180d（`RELAY_CHAT_MAILBOX_TTL_MS`） | 每账户 5000 帧 / 256MiB，LRU 裁剪 |
| contact | 30d | — |
| group-invite | 7d | — |
| MLS control | 7d | — |
| 限频 | 滑动 1min 窗口 / 连接，默认 120 | — |

---

## 3. 目标架构 / Target Architecture

### 3.1 形态选择：独立 workspace crate

仿 `nexchat/mls-wasm`、`grouprobot`——**独立 workspace**，**不并入**主 substrate workspace（避免污染节点构建、保持快速独立编译）。workspace 含**一个共享 lib（`relay-core`）+ 一个 server bin + 一个 pinners crate（lib 公共层 + 三个 bin）**：

```
nexchat/relay-rs/                # ✅ 已实现
├── Cargo.toml            # [workspace] members = [core, server, pinners]
├── core/                 # relay-core lib：被 server 与三个 pinner 共享（纯逻辑，无网络 IO）
│   └── src/
│       ├── persistence.rs    # 快照/journal 反序列化类型 + load/replay（server 写、pinner 读）
│       ├── journal.rs        # journal op 应用
│       ├── desired.rs        # collect_desired_pointers（双轨 LWW，§12.1）— 纯函数
│       ├── planner.rs        # plan_pin_ops（代次轮转）+ plan_chain_pin_requests（only-additive）
│       ├── statefile.rs      # pinner state 文件原子读写（.tmp→rename）
│       ├── types.rs          # 磁盘/wire serde 类型（Pointer / Snapshot / ...）
│       └── ss58.rs           # 共享 SS58 normalize（prefix-42，与 Node relay 一致；解码兼容 1/2 字节前缀输入）
├── server/               # bin: relay-server（main/config/protocol/state/mailbox/token）
├── pinners/              # relay-pinners crate：lib 公共层 + 三个 bin
│   ├── Cargo.toml            # subxt/subxt-signer 走可选 `chain` feature（默认不引入）
│   └── src/
│       ├── lib.rs            # 公共层：read_desired（双轨）/ env 辅助 / shutdown 信号
│       ├── ipfs.rs           # IPFS HTTP 客户端（reqwest：files/stat 体积、pin add/rm）
│       └── bin/
│           ├── pinner-hot.rs     # relay-pinner（§12.2 热层）：plan_pin_ops + IPFS pin/unpin
│           ├── pinner-chain.rs   # relay-chain-pinner（§12.3 持久层）：subxt dynamic 发 extrinsic（需 `chain` feature）
│           └── pinner-crust.rs   # relay-crust-pinner（§12.4 灾备底）：W3Auth PSA /pins 下单
└── tests/（计划中）
    ├── golden_persistence.rs   # 合成 data/ → 重快照 → 语义等价（§11.2）
    ├── rfc9474_vectors.rs      # 与 blindrsa-ts 共享向量（§11.1）
    └── conformance.rs          # 对照 JS relay 的 wire 差分测试
```

> **实现取舍（与早期 §6 草案的偏差）**：① 三个 pinner 合入**单个 `relay-pinners` crate**（lib 公共层 + 三 bin），而非三个独立 crate——三 bin 仍各自独立编译/部署（systemd 各管各），但共享 `read_desired`/IPFS 客户端，避免重复代码（回应开放问题 #5）。② IPFS 客户端 `ipfs.rs` 落在 **pinners crate**（属网络 IO）而非 `relay-core`（保持 core 无 IO）。③ pin 决策纯逻辑（`collect_desired_pointers` / `plan_pin_ops` / `plan_chain_pin_requests` / state 文件读写）已在 `relay-core`，三 bin 直接复用并有单测。

### 3.2 关键依赖 / Crates

| 用途 | crate | 备注 |
|------|-------|------|
| 异步运行时 | `tokio`（rt-multi-thread, net, signal, sync, time, fs） | — |
| WebSocket | `tokio-tungstenite` + `futures-util` | 文本帧 |
| 序列化 | `serde` + `serde_json` | 字段名与 JS 完全对齐（`#[serde(rename)]` 按需） |
| **RFC 9474** | **`blind-rsa-signatures`**（jedisct1） | 与 `@cloudflare/blindrsa-ts` 同源作者；支持 SHA-384/PSS/Randomized；**必须用共享向量验证** |
| RSA 公钥构造 | `rsa` / `num-bigint`（按 `blind-rsa-signatures` API） | 从 JWK `n`/`e`（base64url）构造 PublicKey |
| SS58 | `blake2` + `bs58`（轻量自实现 prefix-42，与 Node relay 一致；解码兼容 1/2 字节前缀输入） | 或 `sp-core`（重，不推荐进独立 workspace） |
| base64 | `base64` | token 的 `s`/`p` 与 ipk 的 `n`/`e` 解码 |
| 日志/指标 | `tracing` + `tracing-subscriber`（+ 后续 `prometheus`） | — |
| **HTTP（pinner）** | `reqwest`（rustls） | IPFS `/api/v0/...` + W3Auth PSA `/pins`；带超时/AbortController 等价 |
| **链客户端（chain pinner）** | `subxt` + `subxt-signer`（sr25519） | 发 `StorageService::request_pin_for_subject`；用 **dynamic extrinsic**（运行时拉取 metadata，无编译期 codegen） |

> **依赖分层**：`reqwest` 三个 pinner 用（在 `relay-pinners` crate）；`subxt`/`subxt-signer` **仅 `pinner-chain`** 用，置于 `relay-pinners` 的可选 `chain` feature 后——默认 `cargo build` **不引入** subxt，relay 主体（server bin）与 hot/crust bin 均保持最小编译面。构建 chain bin：`cargo build -p relay-pinners --features chain --bin pinner-chain`。
>
> **subxt dynamic 而非 codegen**：`request_pin_for_subject(subject_id: u64, cid: Vec<u8>, size_bytes: u64, tier: Option<PinTier>)` 通过 `subxt::dynamic::tx` 构造（`tier = Some(Standard)`），metadata 在连接时从节点拉取——**无需把 runtime metadata 纳入 CI 再生成**（回应开放问题 #6）。代价：调用名/参数类型错配在**运行期**而非编译期暴露，故 chain bin 需 dev-node 集成绿灯作为门禁。

> **SS58 实现注意**：`@polkadot/util-crypto` 的 SS58 = `base58(prefix_byte ‖ pubkey ‖ blake2b-512("SS58PRE"‖data)[..2])`。需用**固定测试向量**钉死与 JS `normalizeAccount` 完全一致（含非法输入原样返回的回退）。

### 3.3 并发模型 / Concurrency

JS 是单线程事件循环；Rust 用 tokio 多线程，必须显式管理共享态。

```
                ┌──────────────── tokio tasks (per WS connection) ────────────────┐
   WS accept ──▶│  read loop → parse → handle_message(state, persist_tx, ...)      │
                └──────────────────────────┬──────────────────────────────────────┘
                                           │ lock
                              ┌────────────▼─────────────┐
                              │   RelayState（共享态）     │  Mutex / RwLock
                              │  指针/inbox/邮箱/endpoints  │
                              └────────────┬─────────────┘
                                           │ persist_tx (mpsc)
                              ┌────────────▼─────────────┐
                              │   Persistence task         │  独占 data/ 文件句柄
                              │  journal append+fsync       │  → 关键写完成回 ack
                              │  debounce 300ms snapshot    │
                              └───────────────────────────┘
```

**设计决策**：

1. **单一状态锁起步**：`Arc<Mutex<RelayState>>`（或 `tokio::sync::RwLock`）。JS 单线程语义最易用一把锁复刻，先正确再优化。后续按 ADR §14.2 分片 / 拆 delivery vs sync KV。
2. **持久化独立 task**：journal 写入经 `mpsc` 投给专用 writer task，**把 fsync 移出投递热路径**（相对 JS 的内联 fsync 是改进，ADR §14.2）。
3. **耐久性合同保留**：`*_put` / `inbox_register` / `spent_add` 这些「ack 即落盘」的操作，handler 必须 **await journal fsync 完成回执后再发 `*_ack`**（与 JS 内联 fsync-then-ack 等价，不能因异步化弱化耐久承诺）。低价值/可重建态（邮箱、MLS 控制）走 `markDirty` debounce 即可。
4. **优雅停机**：`tokio::signal` 收 SIGTERM/SIGINT → flush 快照（保证 ADR §4 备份一致性）→ 退出。

---

## 4. 模块映射 / JS → Rust Mapping

| JS 文件 | Rust 模块 | 重点/坑 |
|---------|-----------|---------|
| `relay-server.mjs` | `server.rs` + `routing.rs` | 分发顺序、`_ctrl` 分支、delivery 回退、`resolveFrameRecipients` 逐条对齐 |
| `relay-persistence.mjs` | `persistence.rs` | schema `v=1`、journal `op`/`at`、`.bak`、截断、`pruneOrphanSpent`、`addSpentToken` 上限 |
| `relay-token-verify.mjs` | `token.rs` | Blind RSA 套件参数、JWK→PublicKey、verify(pk,s,p) |
| `relay-chat-mailbox.mjs` | `mailbox/chat.rs` | `chatFrameDedupKey`、TTL prune、LRU `enforceChatCap`、`chatRowToWire`（剥离 `stored_at`/`bytes`） |
| `relay-contact-mailbox.mjs` | `mailbox/contact.rs` | reqs/acks 双 Map、30d / 7d TTL |
| `relay-limits.mjs` | `limits.rs` | 滑动 1min 窗口；JS 用 `WeakMap<ws>`，Rust 挂连接上下文 |
| `relay-ss58.mjs` | `ss58.rs` | **prefix-42**（与 `relay-ss58.mjs` `RPC_SS58 = 42` 一致；relay 内部键，非链前缀）；非法输入原样返回 |
| `relay-admin.mjs` | `admin.rs` | secret 匹配 或 远端 IP ∈ {127.0.0.1, ::1, ::ffff:127.0.0.1} |

---

## 5. ADR 对齐 / Alignment with A+B+C

### 5.1 三层并行回顾（ADR §0）/ Three Parallel Layers

NexChat 云同步采用 **三层并行**，职责不重叠（源自 `pallets/chat/CHAT_SYNC_ANCHOR_ADR.md` §0）：

| 层 | 名称 | 职责 |
|----|------|------|
| **A** | Relay 热 KV | 日常低延迟 `*_put` / 投递 / spent |
| **B** | 运维备份 | `RELAY_DATA_DIR` 加密 tarball → IPFS pin（换机**全员、含 spent**） |
| **C** | EISA | `anchor_id → AES-GCM(SyncManifest)` 上链；`anchor_id` 凭助记词可重算；Relay 空库时**数据层登录自愈**（边界见 ADR §6.5，blob 可用性前置见 ADR §5.8） |

```text
         ┌──────── IPFS 加密 blob（不变）────────┐
         │  conv-index / contacts / msg-archive   │
         └───────────────────┬────────────────────┘
                             │ cid in SyncManifest
     ┌───────────────────────┼───────────────────────┐
     ▼                       ▼                       ▼
 Layer A              Layer B                 Layer C
 Relay KV             Ops backup              EISA (chain)
 account→{cid,ts}     data/ tarball           anchor_id→ciphertext
 热路径 + spent        GPG + IPFS pin          K_sync 加密 SyncManifest
   ▲
   └── 本次 Rust 重写的唯一对象（A）；B/C 经磁盘格式兼容零改动
```

### 5.2 本次重写在三层中的定位 / Where This Rewrite Sits

| ADR 层 | 与本次重写关系 | 约束 |
|--------|----------------|------|
| **Layer A**（Relay 热 KV） | **= 本次重写对象** | 行为/wire/磁盘逐字节兼容；语义零变更 |
| **Layer B**（运维整库备份，ADR §4） | **不改**；脚本继续作用于 `data/` | Rust 版**必须**保留 SIGTERM flush + 磁盘格式，否则备份/恢复不一致 |
| **Layer C / §5.8**（EISA 三层 pin 的 pinner 守护进程） | **本计划 Rust 化**（P7–P9，§12）；过渡期与 JS 版双向兼容 | 消费 relay `data/`；保留 `saved_at` + journal `at` 语义 + 「快照后截断 journal」以匹配双轨消费；pinner 自身 state 文件双向兼容 |
| **Layer C / EISA pallet + 客户端** | **不改**（`pallet-chat-sync` + `syncAnchor.ts`） | 与 relay 重写无耦合 |

**结论**：Rust relay 主体**只做 Layer A**；**pinner 守护进程（Layer C 的 §5.8 三层 pin 执行体）一并 Rust 化**，但仍是独立进程、独立职责，**不得并入 relay 进程**——三层职责不重叠是 ADR 的核心不变量。B 的 shell 脚本与 EISA pallet/客户端保持现状，靠磁盘格式兼容继续工作。

> 若未来要把 pinner 也 Rust 化，属独立后续项；当前以「格式兼容、JS pinner 不动」为最小爆炸半径。

---

## 6. 分阶段计划 / Phased Plan

| 阶段 | 内容 | 交付 / 验收 |
|------|------|-------------|
| **P0 脚手架** | 独立 workspace；`config.rs`（§2.4）；tokio + tungstenite accept；`register` / `register_account`；`protocol.rs` 类型骨架；`tracing` | 能接 WS、能注册、能裸帧投递；`cargo build` |
| **P1 持久化** | `persistence.rs` 逐字节兼容（load 真实 `relay-state.json` + journal 重放 + 快照 + `.bak` + 截断）；KV `*_put`/`*_fetch`；fsync-then-ack 合同 | **golden 测试**：加载现网 `data/` 快照 → 重快照 → 语义等价；JS 写、Rust 读互通 |
| **P2 RFC 9474** | `token.rs`（`blind-rsa-signatures`）；`inbox_register`/`inbox_lookup`；spent + cap；`verifyDeliveryFrame` 全序列 | **共享向量测试**：与 `blindrsa-ts` 同 `(n,e,s,p)` 验签结果一致；epoch/revoked/spent 短路一致 |
| **P3 邮箱与投递** | 四类邮箱 + flush（register_account 时）+ fetch/consume；`resolveFrameRecipients` + 扇出 + delivery 回退；`_ctrl` 全分支（含 `g:` 广播） | 离线补发、1:1 sealed-sender、群 `routeTo`、MLS 控制面端到端 |
| **P4 运维面** | `admin_stats`（含 chat 邮箱指标）；限频；`MAX_MSG_BYTES`；SIGTERM flush | `admin_stats_reply` 字段与 JS 一致；停机后备份可恢复 |
| **P5 一致性验证** | 差分测试：同一组客户端脚本分别打 JS / Rust relay；跑现有 `scripts/*relay*.test.mjs`、`src/integration.test.ts`。✅ **wire 多 leaf 差分已落地**：`scripts/relay-rs-wire.differential.test.mjs`（`npm run test:relay:rs-wire`）spawn Rust 二进制，对 `peer_add_req`/`mls_backlog_req`/闸一 `commit_intent` 跑通 5/5 | 两实现行为等价；现有测试对 Rust relay 全绿 |
| **P6 部署切换** | systemd unit（仿 `deploy/systemd/nexchat-relay.service`）；按 ADR §4.3 切机 SOP（先 backup 后切）；灰度 + 回滚预案。✅ **wire 多 leaf 控制面前置已满足**：`relay-rs` 已补齐闸一 `commit_intent`/`commit_result`/`presence` 路由、`peer_add_req` + `_senderAccount` 盖章、`mls_backlog_req`（见 §2.1.1 已补齐框）；P5 差分测试已端到端覆盖 wire 多设备 flow（`test:relay:rs-wire` 5/5 绿） | 生产切换；pinner / backup / audit 零改动继续工作；wire 多设备链路在 Rust relay 上端到端可用 |
| **P7 pinner-hot** ✅ | `relay-pinner` Rust 化（双轨消费 + 代次轮转 + 异机 IPFS pin/unpin + state 文件双向兼容）；详见 §12 | ✅ 已实现，`plan_pin_ops` 单测绿；state 文件 serde 与 JS 同形（`{v,slots,pinned}`）；**待补**：与 JS 版同输入产同 pin/unpin 决策的共享向量 + IPFS 集成 |
| **P8 pinner-chain** ✅ | `relay-chain-pinner` Rust 化（subxt **dynamic** 发 `StorageService::request_pin_for_subject` + 运营者 sr25519 + only-additive；`chain` feature） | ✅ 已实现并编译（`--features chain`）；`plan_chain_pin_requests` 单测绿；**待补**：dev-node 集成绿（dynamic 调用名/类型运行期校验）、隐私红线（仅运营者账户）测试 |
| **P9 pinner-crust** ✅ | `relay-crust-pinner` Rust 化（W3Auth PSA `/pins` 下单 + 每日节奏 + only-additive） | ✅ 已实现，与 chain 同 planner；**待补**：节奏红线（每日/仅变化 CID）+ PSA 集成测试 |
| **P10 演进（可选）** | 指标 Prometheus；hot/cold 拆分（ADR §14.2）；后续共享 KV | 不在本次重写硬性范围 |

> **P7–P9 依赖 P1**（持久化兼容）但**不依赖** P6（relay 切换）——pinner 只读 `data/`，可与 relay 主体并行开发；上线顺序建议 hot → chain → crust（对齐 ADR §5.8 的 RPO/独立性优先级）。

---

## 7. 测试策略 / Testing

1. **单元测试**：每模块（dedup key、TTL prune、LRU、SS58 向量、限频窗口、spent cap、LWW）。
2. **Golden 持久化测试**（P1 关键）：用真实 `relay-ops/` / `data/` 快照作 fixture，验证 Rust `load → snapshot` 后 JS 仍可解析、且语义等价（指针/spent/邮箱计数一致）。
3. **RFC 9474 共享向量**（P2 关键）：固定 `(ipk_n, ipk_e, t, ct, epoch, s, p)` 向量集，JS（`blindrsa-ts`）与 Rust（`blind-rsa-signatures`）双向通过；含**应拒绝**的负向量。
4. **SS58 向量**：与 `@polkadot/util-crypto` 对照（合法多前缀输入 + 非法回退）。
5. **差分 / 一致性测试**（P5 关键）：一个测试客户端跑全套场景（注册、KV、inbox、sealed-sender 投递、群路由、MLS 控制、离线补发、admin），分别对 JS 与 Rust relay 断言相同输出。
6. **复用现有测试**：`scripts/relay-server-auth.test.mjs`、`relay-chat-mailbox*.test.mjs`、`relay-persistence.test.mjs`、`src/integration.test.ts` 等指向 Rust relay 跑一遍。
7. **负载**：镜像 `relay-chat-mailbox-load.test.mjs`，验证 LRU/字节上限与内存表现。

---

## 8. 风险与缓解 / Risks

| 风险 | 影响 | 缓解 |
|------|------|------|
| **Blind RSA 跨实现不一致** | 投递准入误判，1:1 全断 | P2 共享向量为**硬门禁**；选同源 `blind-rsa-signatures`；保留「验签失败回退裸 MLS 帧」 |
| **磁盘格式漂移** | pinner / 备份失效（击穿 ADR §5.8/§4） | Golden 测试 + 用 serde struct 锁定 schema；`saved_at`/`at`/截断语义不动 |
| **SS58 规范化差异** | 收件人解析错乱、账户分裂 | 固定向量；非法输入原样返回的回退必须复刻 |
| **多线程竞态** | 偶发投递/计数错误 | 起步单锁复刻单线程语义；持久化串行化于 writer task；压测 + loom（可选）验证 |
| **耐久性弱化**（异步 fsync） | 崩溃丢已 ack 指针 | `*_put`/`inbox`/`spent` 严格 fsync-then-ack；不可降级 |
| **TS 客户端隐含依赖未覆盖** | 个别字段/时序回归 | 以 `relayClient.ts`/`wsRelay.ts` 为合同源；P5 差分测试兜底 |
| **JSON 数字/精度**（`updated_at` ms） | LWW 比较错误 | 用 `u64`/`i64`；与 JS `Date.now()` 量级对齐；向量测试 |

---

## 9. 切换与回滚 / Cutover & Rollback

1. **切换前**：按 ADR §4.3——最后一次 `relay-backup-to-ipfs.sh`；`relay-sync-audit` 验收。
2. **灰度**：新 Rust relay 指向**同一** `RELAY_DATA_DIR`（或其副本）启动；用差分测试客户端旁路验证；先切只读 `*_fetch` 流量观察。
3. **切换**：`SIGTERM` 停 JS relay（触发 flush）→ 起 Rust relay（load 同一 `data/`）→ 切 DNS / `VITE_RELAY_WS`。
4. **回滚**：Rust relay `SIGTERM`（flush，格式兼容）→ 起回 JS relay（load 同 `data/`）。因双向格式兼容，**回滚零数据迁移**。
5. **pinner / backup / audit**：全程不停、不改（消费同一 `data/`）。

---

## 10. 未决问题 / Open Questions

1. **SS58 依赖**：轻量自实现（`blake2`+`bs58`）vs `sp-core`？倾向轻量（独立 workspace 不引重依赖），需向量背书。**待评审**。
2. **状态锁粒度**：单 `Mutex` 起步是否够 P6 生产量级？还是 P5 即引入「delivery vs sync KV」分片（ADR §14.2 P2）？**建议先单锁 + 指标驱动再拆**。
3. **持久化异步化的耐久边界**：哪些 op 必须 fsync-then-ack（已定：三类指针 + inbox + spent），其余 debounce——评审确认清单。
4. **是否顺带补 JS 版缺口**：如 `delivery.p` 与 `(t,ct,epoch)` 一致性校验（现 JS 未重建校验）——**默认不改**（保持行为等价），如要补属有意变更需单列。
5. ~~**pinner Rust 化的部署形态**~~ ✅ **已定**：三个 **独立 bin**（`pinner-hot`/`pinner-chain`/`pinner-crust`），但同住 `relay-pinners` crate（共享 lib 公共层），systemd 各自管理、与现状一一对应。
6. ~~**subxt 元数据来源**~~ ✅ **已定**：chain pinner 用 **subxt dynamic extrinsic**（连接时从节点拉 metadata），**不做编译期 codegen**——故无 CI metadata 再生成负担；代价是调用名/类型错配延后到运行期，由 dev-node 集成测试兜底。

---

## 11. 高风险模块攻坚详案 / High-Risk Module Deep-Dive

> 全文最高风险的两块——**P2 RFC 9474 验签** 与 **P1 持久化格式**——单独立详案先钉死。原则：**用共享/合成测试向量把跨实现行为锁死**，再写实现。两套向量都由现有 JS（权威实现）生成并提交入库，Rust 与 JS 双向校验。

### 11.1 P2 — RFC 9474 共享测试向量方案 / Shared Verification Vectors

#### 11.1.1 核心认知（决定方案形态）

1. **签名是概率性的，验证是确定性的。** RSABSSA Randomized 套件的 `prepare`（前置 32B 随机）、`blind`、PSS salt 都引入随机性——**无法**用「生成同一签名再比字节」做跨实现对拍。但 `verify(pk, sig, msg) → bool` 是确定的。
2. **relay 只做 verify。** 故共享向量**只针对验证**，与 relay 职责完全吻合——不需要在 Rust 侧复刻盲签发流程。
3. **relay 对 `p` 整体验签、不重建消息。** 客户端送来的 `p`（DeliveryAdmission.p）= `随机(32) ‖ t(32) ‖ ct(32) ‖ epoch(u32 大端)` = 100B（`tokenMessage.ts::buildTokenMessage` + Randomized `prepare` 前置随机）。relay `verifyDeliveryToken` 调 `suite.verify(pk, s, p)`，**不**校验 `p[32..64]==t`、`p[64..96]==ct`。Rust 必须复刻这一**整体验签语义**（不重建）。

#### 11.1.2 三级向量

| 级 | 来源 | 目的 | 期望 |
|----|------|------|------|
| **L1 RFC 官方向量** | RFC 9474 Appendix（RSABSSA-SHA384-PSS-**Randomized** 变体） | 把两端**同时**钉到 RFC 标准（而非只是互相对齐） | `verify == true` |
| **L2 应用布局正向量** | JS（`@cloudflare/blindrsa-ts`）用**固定 inbox 密钥对** + relay 真实消息布局（`p=rand32‖t‖ct‖epochBE`）生成一次、提交 | 覆盖 relay 真实 wire 形态（3072-bit n、`e=AQAB`、`p` 100B、base64url 的 `n`/`e`/`s`/`p`） | `verify == true` |
| **L3 负向量** | 在 L2 基础上篡改 | 防「永远返回 true」类实现错误 | `verify == false` |

**L3 负向量清单（最少）**：① `s` 翻 1 bit；② 换一个不同 `n`（错公钥）；③ `p` 截断/补长；④ 用 epoch=0 的 `p` 配 epoch=1 的 `s`（绑定失败）；⑤ `e` 改为非 `AQAB`。

**保留现有 JS 行为的向量（行为等价红线）**：构造一条 `p` 内嵌的 `t/ct` 与帧 `delivery.t/ct` **不一致** 但签名对 `p` 有效的向量，标记 `expect: true, note: "passes_today"`——Rust 必须同样返回 `true`（与 §10 开放问题 4 一致：是否补 `p` 一致性校验是**独立有意变更**，不在本重写默认范围）。

#### 11.1.3 Rust crate 绑定要点（`blind-rsa-signatures`）

- **哈希/盐**：套件 = SHA-384 + MGF1-SHA-384 + PSS `salt_len = 48`（= hLen）。`blind-rsa-signatures` 默认即 SHA-384；`Options` 显式设 `salt_len(48)`，`deterministic` 仅影响签名、对 verify 无关（PSS verify 从签名恢复 salt，但需 salt 长度匹配）。
- **公钥构造**：`ipk_n` / `ipk_e` 是 JWK 的 base64**url**（无填充）。Rust：base64url-decode → `BigUint` → `RsaPublicKey`（`e` 通常 `AQAB`=65537）。
- **32B 随机前缀的传参方式（必须用 L2 向量二选一锁定）**：
  - 方案 (a)：`pk.verify(&s, msg_randomizer = Some(p[0..32]), msg = p[32..100], &opts)`
  - 方案 (b)：`pk.verify(&s, msg_randomizer = None, msg = p[0..100], &opts)`（把整段 `p` 当消息哈希）
  - blindrsa-ts 对整段 `preparedMsg` 做 PSS-encode，故**预期 (b) 与 JS 一致**；但**以 L2 向量实测为准**，谁通过用谁，写死在 `token.rs` 注释里。
- **`s` 长度**：3072-bit n → 签名 384B；base64 解码后长度校验。

#### 11.1.4 产物与目录

```
nexchat/scripts/gen-rfc9474-vectors.mjs        # 生成器（用 blindrsa-ts + relay 布局）
nexchat/scripts/rfc9474-vectors.guard.test.mjs # JS 侧守卫：同一向量过 relay-token-verify.mjs
nexchat/relay-rs/tests/vectors/rfc9474/
  l1_rfc_official.json          # 从 RFC 9474 附录誊录
  l2_app_layout_positive.json   # JS 生成（固定密钥/固定 t,ct,epoch）
  l3_negative.json              # 篡改向量
nexchat/relay-rs/tests/rfc9474_vectors.rs       # Rust 侧消费向量
```

**向量 JSON 字段**（每条）：`{ ipk_n, ipk_e, t, ct, epoch, s, p, expect: bool, note? }`（全 base64url 或 hex，固定一种编码并在文件头声明）。

#### 11.1.5 双向校验（防生成器漂移）

1. **Rust 消费**：`cargo test -p relay-rs rfc9474` 加载三文件，对每条跑 `token::verify_delivery` 断言 `expect`。
2. **JS 守卫**：`node --test scripts/rfc9474-vectors.guard.test.mjs` 加载**同一**文件，过 `relay-token-verify.mjs::verifyDeliveryToken` 断言同样 `expect`——保证向量对**两端**都成立，杜绝「向量本身写错只迁就一端」。
3. **L1 必须双绿**：RFC 官方向量在 JS 与 Rust 同时 `true`，是「两端都符合 RFC」的硬证据。

#### 11.1.6 验收

- [ ] L1/L2/L3 三文件在 Rust 与 JS 守卫双绿。
- [ ] (a)/(b) 传参方式由 L2 实测锁定并注释。
- [ ] `passes_today` 向量在 Rust 返回 `true`（行为等价）。
- [ ] 负向量全部 `false`（含错公钥、篡改 `s`、epoch 错配）。

---

### 11.2 P1 — 持久化 Golden Fixtures / Persistence Golden Tests

#### 11.2.1 前提

- `relay-ops/data/` **现为空**（无生产快照）→ fixtures **全部合成、提交入库、不含密钥**（用占位 SS58 / base64 占位密文）。
- fixtures 由**权威实现**（现有 `relay-persistence.mjs`）以**冻结时钟**生成，再提交——天然正确；格式有意变更时重生成 = 一次**可评审的 diff**，即「防格式漂移」的护栏。

#### 11.2.2 三向 golden 测试

| 方向 | 含义 | 断言 |
|------|------|------|
| **JS 写 → Rust 读**（load 一致性） | Rust `persistence::load_into(fixture)` 复现内存态 | `stats()` 全字段计数一致 + 抽样字段值一致 |
| **Rust 写 → JS 读**（write 一致性） | Rust `snapshot()` 输出能被 `relay-persistence.mjs::applySnapshot` 解析 | JS load 后 `stats()` 与 Rust 内存态一致 |
| **Round-trip 幂等** | `load → snapshot → load` | 两次内存态完全相等 |

> Rust 写→JS 读这一向**最关键**：它保证 Rust relay 落盘后，JS 的 pinner / backup / restore（ADR §5.8 / §4）仍能解析——这是 drop-in 兼容的命门。

#### 11.2.3 必须钉死的字节级规则

- schema `v: 1`（精确）。
- `spentByInbox` 序列化为 `{ inboxId: [token...] }`（Set→数组，**非**对象）。
- `contactMailbox[acct] = { reqs: {id: row}, acks: {id: row} }`（嵌套双 Map）。
- `groupInviteMailbox` / `mlsMailbox` / `chatMailbox` = `{ acct: { key: row } }`。
- 指针仅 `{cid, updated_at}`（多余字段丢弃）。
- journal 行 = 原 op 负载 + 追加 `at`；ndjson 每行换行结尾；容忍尾部空行；损坏行计数跳过。
- **紧凑 JSON**（JS `JSON.stringify(x,null,0)` 无空白）→ Rust `serde_json::to_string`（紧凑）。
- **数字而非字符串**：`updated_at` / `epoch` / `bytes` / `stored_at` / `expiresAt` 用 `u64`（ms 时间戳 ~1.7e12，JS `Number` 安全位内）。
- **冻结时间戳**：生成器注入固定 `saved_at` 与各 journal `at`（不用真实 `Date.now()`），使 fixtures 确定、CI 稳定。
- `addSpentToken` **上限**：replay 超 `RELAY_SPENT_CAP` 部分跳过（fixture 用小 cap 触发）。
- `pruneOrphanSpent`：spent 指向未注册 inbox → load 时丢弃（专门 fixture）。
- **单调 LWW**：journal 较旧 `updated_at` 不覆盖较新（镜像现有 JS 测试）。
- **字段顺序**：JS 解析与 JS pinner/backup 均**与顺序无关**，故「语义等价」是**必过线**；字节级 diff 一致为**加分项**（建议 serde struct 字段顺序对齐 `snapshot()` 的 `v, saved_at, indexPointers, contactsPointers, msgArchivePointers, inboxesByAccount, spentByInbox, contactMailbox, groupInviteMailbox, mlsMailbox, chatMailbox` 以获得干净 diff）。

#### 11.2.4 Fixtures 目录

```
nexchat/scripts/gen-relay-fixtures.mjs              # 用 relay-persistence.mjs + 冻结时钟生成
nexchat/scripts/relay-fixtures.guard.test.mjs       # JS 守卫：load 同一 fixtures → 断言 expected
nexchat/relay-rs/tests/fixtures/persistence/
  full_state/relay-state.json        # 每类字段都非空（含 4 类邮箱、spent、3 指针、inbox）
  full_state/expected.json           # 期望 stats + 抽样断言
  bak_fallback/relay-state.json      # 损坏主快照（"{not json"）
  bak_fallback/relay-state.json.bak  # 有效备份 → load 应回退成功
  journal_replay/relay-state.json    # 快照 saved_at = T0
  journal_replay/relay-journal.ndjson# at>T0 应用 + at<=T0 跳过 + 1 行损坏跳过
  orphan_spent/relay-state.json      # spent 指向未注册 inbox → load 后应消失
  spent_cap/relay-state.json         # spent 超小 cap → 截断
nexchat/relay-rs/tests/golden_persistence.rs        # Rust 侧三向测试
```

`expected.json` 形如：`{ stats: {indexPointers, contactsPointers, msgArchivePointers, inboxes, spentInboxes, spentTokens, contactMailboxes, groupInviteMailboxes, mlsMailboxes, chatMailboxes, chatFrames}, spot: [{path, value}...] }`。

#### 11.2.5 CI wiring

- Rust：`cargo test -p relay-rs --test golden_persistence`（三向 + 边界 fixtures）。
- JS 守卫：`node --test scripts/relay-fixtures.guard.test.mjs`——用 `relay-persistence.mjs` load 同一 fixtures，断言同 `expected.json`，保证 fixtures 对两端都权威。
- 复用：把现有 `relay-persistence.test.mjs` 的 6 个场景（journal 存活、快照清 journal、bak 回退、单调 LWW、spent 存活、spent_clear、chatMailbox 存活）**逐条**在 Rust 侧重写为 fixture 驱动测试，确保语义对等。

#### 11.2.6 验收

- [ ] 三向（JS↔Rust）对 `full_state` 全绿。
- [ ] `bak_fallback` / `journal_replay`（含跳过 `at<=T0` 与损坏行）/ `orphan_spent` / `spent_cap` 边界全绿。
- [ ] Rust 落盘文件被 `relay-persistence.mjs` 成功 load 且 `stats()` 一致（命门）。
- [ ] `relay-persistence.test.mjs` 6 场景的 Rust 对等测试全绿。

---

## 12. Pinner Rust 化详案 / Pinner Rewrite Deep-Dive

> 把 ADR §5.8 的三层 pin 守护进程（`relay-pinner` 热层 / `relay-chain-pinner` 持久层 / `relay-crust-pinner` 灾备底）从 JS 迁到 Rust。三者**共享**：双轨消费（snapshot+journal）、IPFS 体积校验、state 文件记账、跳过超限/失败 + 重启重试。差异在 **pin 后端**（远程 IPFS / 链上 extrinsic / W3Auth PSA）。

### 12.1 共享内核（`relay-core`）/ Shared Core

复刻这些**纯逻辑**（现有 JS 已抽为纯函数 + 单测，是天然的共享向量来源）：

- **`collect_desired_pointers(snapshot, journal) → Map<slotKey, {cid, updated_at}>`**：从 `relay-state.json` 的 `indexPointers/contactsPointers/msgArchivePointers` + journal 的 `index_put/contacts_put/msg_archive_put` 推导期望集合，**按 slot LWW（`updated_at >=` 保留）**。`slotKey = "{kind}/{account}"`，kind ∈ `{index, contacts, archive}`。**每 tick 重新全量推导**（不 tail），免疫 journal 截断（§5.8 双轨）。
- **state 文件原子读写**：`.tmp` 写入 → `rename`（与 JS 一致）。
- **IPFS 体积**：`POST /api/v0/files/stat?arg=/ipfs/{cid}` → `CumulativeSize ?? Size`；超 `MAX_BLOB_BYTES`（默认 10MB）跳过。

> `relay-core` 同时被 server bin 复用（persistence 反序列化类型一份，避免 server 与 pinner 各写一套解析而漂移）。

### 12.2 热层 `relay-pinner`（P7）

- **planner（纯）** `plan_pin_ops(desired, prev_state, keep_generations=2)`：每 slot 维护代次数组（新在前），CID 变化则 `unshift`、否则更新首代，裁到 `keep_generations`；**从 relay 状态消失的 slot 保留其代次**（relay 清库**不得**级联 unpin——热层存在的意义）。`toPin = wanted − had`，`toUnpin = had − wanted`。
- **后端**：远程 IPFS `POST /api/v0/pin/add?arg={cid}&recursive=true` / `pin/rm?arg={cid}`。
- **记账红线**：state.pinned **只记实际 pin 成功的 CID**；想要但失败的不入 `pinned`，下个 tick 自动重试（与 JS `pinnedOk` 过滤一致）。
- **部署红线（ADR §5.8）**：`PINNER_IPFS_API` 必须指向**异机** IPFS（与 relay 主机不同宿主），否则三层冗余形同虚设。
- **state 文件** `relay-pinner-state.json`：`{v:1, slots:{key:[{cid,updated_at}...]}, pinned:[...]}`——**双向兼容**（JS↔Rust 互读，支持灰度/回滚）。

### 12.3 持久层 `relay-chain-pinner`（P8，最高新风险）

- **planner（纯）** `plan_chain_pin_requests(desired, prev_state)`：**only-additive**——对当前被引用、且 `requested` 未记录的 CID 入 `toRequest`；CID **永不**从 `requested` 移除（pin 生命周期归链上计费，relay 清库不取消链上 pin）。`maxPerTick=50`。
- **链调用**：`subxt` 发 `storageService.request_pin_for_subject(subject_id: u64, cid: Vec<u8>, size_bytes: u64, tier: Option<PinTier>)`，`tier = Some(Standard)`。**`cid` 参数 = CID 字符串的 UTF-8 字节**（对应 JS `utf8Hex(cid)` = `0x`+hex(utf8)），**不是** multibase 解码——必须一致，否则链上存的是不同字节。
- **`AlreadyPinned` = 成功**：其它路径已先 pin，对本进程视为成功落账（记入 `requested`）。
- **隐私红线（ADR §5.8，硬不变量）**：签名者**必须**是运营者 sr25519（`CHAIN_PINNER_OPERATOR_SURI`），**绝不可**用户账户——否则把被否决的「`AccountId` → 明文 CID」经计费路径写回链上。Rust 侧：唯一凭据是 operator suri，代码中**无任何**用户密钥入口；加单测断言签名账户 == 运营者。
- **subxt 元数据/编码对拍（关键风险）**：`request_pin_for_subject` 的 call index + SCALE 编码必须与 polkadot-js 一致。缓解：① subxt 类型从**同一 runtime metadata** codegen；② 集成测试对 dev-node 提交并断言 `ExtrinsicSuccess` + pin 落账；③ 单测比对编码后 call 字节 vs polkadot-js `api.tx.storageService.requestPinForSubject(...).method.toHex()`（committed 向量）。
- **state 文件** `relay-chain-pinner-state.json`：`{v:1, requested:{cid:{at,size}}}`——双向兼容。
- **可注入提交器**：保留 JS 的 `opts.submit` 等价（trait 对象 / 泛型），测试用 mock submitter，不连真链。

### 12.4 灾备底 `relay-crust-pinner`（P9）

- **复用** 持久层同一 `plan_chain_pin_requests`（only-additive，每 CID 只下一次单），`maxPerTick=200`。
- **后端**：自托管 W3Auth Pinning Service（标准 IPFS Remote Pinning API）`POST {endpoint}/pins`，`Authorization: Bearer {token}`，body `{cid, name:"nexchat-sync-{cid[:12]}"}`，取 `requestid/requestId`。
- **节奏红线（ADR §5.8）**：**每日**扫描、**仅对变化 CID** 下单——禁止按 `*_put` 或锚频率逐次下单（百万级订单/天不可接受）。`CRUST_PINNER_INTERVAL_MS=86400000`。
- **隐私**：PSA token 属**运营者 seed**；Crust 链公开 operator→CID（无主加密 blob，可接受泄漏，已在客户端隐私文案披露）。**绝不**用用户密钥。
- **state 文件** `relay-crust-pinner-state.json`：`{v:1, requested:{cid:{at,size,requestId}}}`——双向兼容。

### 12.5 测试策略 / Testing

| 层 | 方法 |
|----|------|
| **纯 planner 向量** | `collect_desired_pointers` / `plan_pin_ops` / `plan_chain_pin_requests` 用 JS 生成的「输入(snapshot+journal+prevState) → 期望(toPin/toUnpin/nextState 或 toRequest)」向量；Rust 消费 + JS 守卫双绿（同 §11 套路）。直接迁移现有 `relay-pinner.test.mjs` / `relay-chain-pinner.test.mjs` / `relay-crust-pinner.test.mjs` 场景 |
| **state 文件互兼容** | JS 写→Rust 读、Rust 写→JS 读，三种 state 文件各一组（灰度/回滚命门） |
| **IPFS 客户端** | mock HTTP（`files/stat` 各种 size、`pin/add`、`pin/rm`）；超限跳过 + 失败重试断言 |
| **chain extrinsic（最重）** | ① call 编码对拍 polkadot-js committed 向量；② dev-node 集成：operator 提交 → `ExtrinsicSuccess` + 链上 pin 记录；③ `AlreadyPinned` 视为成功 |
| **crust 下单** | mock PSA `/pins`；only-additive（同 CID 不重下）+ 每日节奏断言 |
| **隐私红线** | 单测断言 chain/crust 凭据仅来自 operator env，无用户密钥路径 |

### 12.6 切换与回滚 / Cutover

- pinner 与 relay **独立切换**：因 state 文件 + 消费的 `data/` 双向兼容，可单独把某个 pinner 从 JS 切到 Rust（停 JS unit → 起 Rust unit，读同一 state 文件续跑），失败原样切回。
- systemd：仿 `deploy/systemd/nexchat-relay.service` 为三个 Rust pinner 各加 unit（与现状 JS 进程一一对应）。
- 上线顺序：**hot → chain → crust**（ADR §5.8 RPO/独立性优先级）。

### 12.7 验收 / Acceptance

- [ ] 三个纯 planner 的 JS↔Rust 向量双绿；三种 state 文件双向互读。
- [ ] 热层：异机 IPFS pin/unpin 正确；relay 清库不级联 unpin；失败 CID 下 tick 重试。
- [ ] 持久层：dev-node 集成绿；call 编码对拍一致；`AlreadyPinned` 成功；签名账户 == 运营者（隐私红线单测）。
- [ ] 灾备底：PSA only-additive + 每日节奏；运营者 token，无用户密钥。
- [ ] `relay-sync-audit`（仍 JS）对 Rust pinner 维护的 pin 集合三项检查（≥2 网关取回 / Cluster 状态 / Crust 订单）通过。

---

## 13. 附录：与 MLS 协议的关系澄清 / MLS Clarification

- **relay 不理解 MLS 密码学**：它只识别 `convId` 前缀（`d:` 1:1 / `g:` 群）与控制消息标记（`kp`/`welcome`/`commit`/`mls_ready`/`hello`）用于**路由**，载荷一律不透明 base64（真实 OpenMLS 字节，加密在 `nexchat/mls-wasm` 完成）。
- **群聊与私聊统一 MLS**：本次重写**不**改变这一点，也**不**引入 Signal。若未来评估 1:1 改协议，那是客户端 + relay 控制面路由的独立决策，**不属本重写范围**。
- **与 `docs/CHAT_GROUP_MLS_DESIGN.md` 的关系**：该文档的 §13「方案 A：节点内置 libp2p 中继」是更远期的投递层演进；当前 relay（本文对象）是其前身/并行的 store-and-forward 实现，二者通过 wire 兼容性解耦演进。

---

*End of Plan / 文档结束*
