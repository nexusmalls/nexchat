# relay-rs 前端对接方案 / Frontend Integration Guide

> **适用对象**：前端 / 客户端 / 运维探测脚本。  
> **事实源（implementation source of truth）**：
> - 服务端 wire 分发：`nexchat/relay-rs/server/src/protocol.rs`
> - 快照 schema：`nexchat/relay-rs/core/src/types.rs`（`SCHEMA_V = 1`）
> - 前端封装：`nexchat/src/relay/*`（`wsRelay.ts`、`relayOneShot.ts`、各 `*Pointer.ts`、邮箱模块）
>
> 生产唯一 relay 为 Rust **`relay-rs`**（systemd：`nexchat-relay.service` → `/opt/nexchat-relay/relay-server-rs`）。旧 Node `relay-server.mjs` 已退役，本文不再以其为基线。

---

## 1. 传输与连接 / Transport

- **协议**：WebSocket，应用层通常一条**长连接**（实时 IM / MLS 控制）+ 若干**一次性连接**（指针 put/fetch、邮箱 consume）。
- **帧格式**：JSON 文本帧（`Message::Text`）。不推荐二进制帧。
- **地址**：`ws://<host>:<port>`，默认 `8765`（`RELAY_PORT`）。前端：`VITE_RELAY_WS`，例 `wss://nexusmall.net/nexchat-relay/`。
- **单条上限**：默认 `RELAY_MAX_MSG_BYTES = 256 KiB`，超限静默丢弃。`WebSocketRelay.send` 发送前会自检并抛错。
- **限频**：默认 `RELAY_RATE_LIMIT = 120`/分钟。带 `dedupKey` 的数据帧被限流时回 `frame_reject`（§3.7）。
- **重连**：断线后指数退避重连，并**重新** `register` + `register_account`。

### 连接后必做：注册

```json
{ "type": "register", "id": "<endpointId>" }
{ "type": "register_account", "id": "<endpointId>", "account": "<SS58>", "account_sig?": "<base64>" }
```

| 字段 | 说明 |
|------|------|
| `id` | 端点 id（UUID）。回声抑制（`_from`）、`register_account` 必须匹配同一 `id`。 |
| `account` | SS58；标记在线并 **flush** 该账户离线 MLS 控制 + 聊天帧。 |
| `account_sig` | 可选 sr25519 签名（`registerAccountAuth.ts`）。**`RELAY_STRICT_AUTH=1` 时必填**。 |

同一 `account` 可多 `id` 在线；消息扇出至所有端点（排除 `_from`）。

> **SS58**：relay 内部存储键规范化为 **prefix 42**；链/前端展示用 **273**（`canonicalAddress()`）。客户端发任意前缀均可，回包原样回显。

---

## 2. 鉴权模型 / Auth

| 门禁 | 条件 | 适用操作 |
|------|------|----------|
| **无** | — | `register`、`inbox_lookup`、大部分 `_ctrl`（`peer_add_req` 除外见 §3.9） |
| **Reader** | 默认开放；`RELAY_STRICT_AUTH=1` 时等同 Writer | `*_fetch`、`chat_fetch`、`contact_fetch`、`group_invite_fetch` |
| **Writer** | 已 `register_account` 且 `conn.account === 目标 account` | 所有 `*_put`、`inbox_register`、`*_consume`、`mls_backlog_req` |
| **Admin** | loopback 或 `admin_secret === RELAY_ADMIN_SECRET` | `admin_stats` |

失败返回：`{ "type": "auth_reject", "op": "<操作名>", "account?": "...", "reason?": "missing_sig|invalid_sig" }`。

**前端约定**：
- 长连接：`WebSocketRelay` 在 `connect()` 时 `register` + `registerAccountWire()`（含可选 `account_sig`）。
- 写/消费：`relayOneShotSend()` / `relayOneShotFetch()` **先注册再发业务消息**（见 `relayOneShot.ts`）。
- 无 ack 的 consume（`contact_consume`、`group_invite_consume`）使用 `relayOneShotSend(..., { noReply: true })`。

---

## 3. 消息总览 / Message Catalog

方向：C→S 客户端发，S→C 服务端回。

### 3.1 会话

| type | 字段 | 响应 |
|------|------|------|
| `register` | `id` | 无 |
| `register_account` | `id`, `account`, `account_sig?` | 无（触发 MLS/chat flush） |

### 3.2 云同步指针（6 槽，LWW `{cid, updated_at}`）

| 槽位 | put | fetch | ack | reject | reply |
|------|-----|-------|-----|--------|-------|
| 会话索引 | `index_put` | `index_fetch` | `index_ack` | `index_reject` | `index_reply` |
| 联系人库 | `contacts_put` | `contacts_fetch` | `contacts_ack` | `contacts_reject` | `contacts_reply` |
| 消息归档 | `msg_archive_put` | `msg_archive_fetch` | `msg_archive_ack` | `msg_archive_reject` | `msg_archive_reply` |
| MLS escrow vault | `mls_vault_put` | `mls_vault_fetch` | `mls_vault_ack` | `mls_vault_reject` | `mls_vault_reply` |
| 发送权 handoff | `handoff_put` | `handoff_fetch` | `handoff_ack` | `handoff_reject` | `handoff_reply` |
| PIN 签名钥备份 | `mls_signing_put` | `mls_signing_fetch` | `mls_signing_ack` | `mls_signing_reject` | `mls_signing_reply` |

- **put 请求**：`account`, `cid`, `updated_at`（>0）。**Writer**。
- **fetch 请求**：`account`, `request_id?`。**Reader**。
- **reject**：`reason: "stale_updated_at"`, `updated_at`（当前值）。
- 前端模块：`indexPointer.ts`、`contactsPointer.ts`、`msgArchivePointer.ts`、`mlsVaultPointer.ts`、`handoffPointer.ts`、`mlsSigningPointer.ts`。

### 3.3 RFC 9474 Inbox

| type | 字段 | 响应 | 鉴权 |
|------|------|------|------|
| `inbox_register` | `account`, `inbox_id`, `epoch?`, `ipk_n?`, `ipk_e?`, `revoked_tags?[]` | `inbox_ack` / `inbox_reject` | Writer |
| `inbox_lookup` | `account`, `request_id?` | `inbox_reply`（含 `online`） | 无 |

`epoch` 回退 → `inbox_reject{reason:"stale_epoch", epoch}`；epoch 升高或 `inbox_id` 变更清空 spent。

### 3.4 联系人邮箱（TTL 30 天）

| type | 字段 | 响应 | 鉴权 |
|------|------|------|------|
| `contact_fetch` | `account`, `request_id?` | `contact_reply{reqs[], acks[]}` | Reader |
| `contact_consume` | `account`, `req_ids[]`, `ack_ids[]` | **无** | **Writer** |

行 = §3.9 `contact_req`/`contact_ack` + `toAddr`/`_ctrl`/`stored_at`。  
前端：`contactRequestInbox.ts`；处理完后 **必须** `consumeContactInbox`（经 `relayOneShotSend`）。

### 3.5 群邀请邮箱（TTL 7 天）

| type | 字段 | 响应 | 鉴权 |
|------|------|------|------|
| `group_invite_fetch` | `account`, `request_id?` | `group_invite_reply{invites[]}` | Reader |
| `group_invite_consume` | `account`, `invite_ids[]` | **无** | **Writer** |

前端：`groupInviteInbox.ts`。

### 3.6 聊天邮箱（TTL 180 天，5000 帧 / 256 MB / 账户）

| type | 字段 | 响应 | 鉴权 |
|------|------|------|------|
| `chat_fetch` | `account`, `request_id?` | `chat_reply{frames[]}` | Reader |
| `chat_consume` | `account`, `dedup_keys[]` | `chat_ack` | Writer |

- `chat_fetch` **非破坏性**（多设备各自拉取）；删除靠 `chat_consume`。
- 前端：`chatMailbox.ts`（fetch 用 `relayOneShotFetch`；consume 带 `account` 字段）。

### 3.7 MLS backlog 拉取（Wire Gate-2）

| type | 字段 | 响应 | 鉴权 |
|------|------|------|------|
| `mls_backlog_req` | `account`, `convId` | **无包装**；匹配 `convId` 的 MLS 控制行原样重投 | Writer |

用于 CAS 落败方追平 epoch，无需等 reconnect flush。前端：`WebSocketRelay.requestMlsBacklog()`。

### 3.8 数据帧（密文）

无 `type`、无 `_ctrl`：

```jsonc
{
  "convId": "d:<a>:<b> | g:<groupId>",
  "ciphertextB64": "<必填>",
  "senderRef?": "<展示用；sealed-sender 可省略>",
  "dedupKey?": "<建议 client 生成>",
  "expiresAt?": 1718000000000,
  "routeTo?": ["<SS58>", "..."],   // 群聊路由
  "echoSelf?": true,                // Track B：也存入发送方邮箱（多设备离线补齐）
  "delivery?": { /* RFC 9474 */ },
  "_from": "<endpointId>"
}
```

**收件人**（并集）：`delivery.inboxId` 所属账户、`routeTo[]`、`convId` 解析出的 `d:` 双方。  
**`delivery` 验签失败**：默认剥掉 `delivery` 继续投递；`RELAY_STRICT_AUTH=1` 时回 `frame_reject{reason:"delivery_rejected"}`。

**`frame_reject`**（限流 / strict delivery）：`reason`, `dedupKey`, `convId?`。

### 3.9 控制面（`_ctrl: true`，字段 `t`）

统一：`{ "_ctrl": true, "t": "...", "_from": "...", ... }`。类型定义见 `relayClient.ts` → `ControlMsg`。

#### 社交 / 令牌（无 convId 或任意）

| `t` | 存储 | 路由 |
|-----|------|------|
| `contact_req` / `contact_ack` | contact 邮箱 | `toAddr` |
| `group_invite` | group_invite 邮箱 | `toAddr` |
| `token_req` / `token_sig` | 不入库 | `toAddr` 实时 |

#### 1:1 MLS（`convId` 以 `d:` 开头）

| `t` | 存储 | 路由 | 备注 |
|-----|------|------|------|
| `kp` | MLS 邮箱 | owner（有 `identity` 时） | |
| `welcome` | MLS 邮箱 | `toAddr` | |
| `commit` | MLS 邮箱 | member | 带 `commit_epoch` → Gate-2 CAS；落败 → `commit_reject` |
| `mls_ready` | MLS 邮箱 | owner | |
| `peer_add_req` | MLS 邮箱 | **对端** | 需 Writer；relay 补 `_senderAccount` |

`commit` Gate-2 字段：`commit_epoch`, `msgId?`。无 `commit_epoch` 的 commit **legacy 透传**（与旧 2-leaf 兼容）。

**`commit_reject`（S→C，非 `_ctrl`）**：

```jsonc
{ "type": "commit_reject", "reason": "epoch_stale", "convId": "...", "current_epoch": 1, "msgId?": "..." }
```

#### 账户自通道（`convId = s:<account>`，Wire / Track A）

| `t` | 存储 | 路由 |
|-----|------|------|
| `kp`, `welcome`, `commit`, `new_device_state` | MLS 邮箱 | 该账户所有在线端点 |

`new_device_state`（Track B 预留，现行 Wire 不发）：`{ t, from, convId: "s:<account>", state: "<b64 opaque MLS export>", msgId? }`。
| `presence`, `commit_intent`, `commit_result` | MLS 邮箱 | 同上 |
| `device_join_request`, `device_join_offer`, `device_join_kp` | MLS 邮箱 | 同上 |
| `handoff-request`, `handoff-grant` | MLS 邮箱 | 同上 |

> `mls_ready` **不**在 `s:` 通道路由（仅 `d:`）。

#### 群 legacy 广播（`convId` 以 `g:` 开头）

| `t` | 存储 | 路由 |
|-----|------|------|
| `hello`, `kp`, `commit` | **不入库** | 广播所有在线账户 |
| `welcome` | MLS 邮箱 | `toAddr`（加入设备） |
| `peer_add_req` | — | 广播（需认证 + `_senderAccount`） |

Track A 群 MLS 主路径在链上；上表为 legacy / Wire 群辅助。

### 3.10 运维

| type | 鉴权 | 响应 |
|------|------|------|
| `admin_stats` | Admin | `admin_stats_reply` / `admin_reject` |

脚本：`scripts/relay-admin-stats.mjs`。

---

## 4. 推送 vs 拉取

| 内容 | 在线 | 离线补发 | TTL |
|------|------|----------|-----|
| 聊天密文 | onMessage | flush + `chat_fetch` | 180d |
| 1:1 MLS（`d:`） | onControl | flush + `mls_backlog_req` | 7d |
| 账户自通道（`s:`） | onControl | flush（MLS 邮箱） | 7d |
| 群 MLS（`g:` legacy） | 广播 | 无 | — |
| 好友 / 群邀请 | onControl | 轮询 fetch | 30d / 7d |
| 云指针 | — | `*_fetch` | 持久快照 |

---

## 5. 典型流程

**上线**：`register` → `register_account` →（Wire 开启时）`probeRelayWireCapabilities` → flush 收 MLS/chat → `contact_fetch` / `group_invite_fetch` / 六槽指针对账。

**Wire 1:1 多 leaf**：`peer_add_req` 或 `device_join_*` on `s:<account>` → Gate-1 `commit_intent`/`commit_result` → Gate-2 `commit`+`commit_epoch` → 落败 `mls_backlog_req` + `commit_reject` 追平。

**Track A handoff**：只读设备 `handoff-request` on `s:<account>` → 旧主 `handoff-grant`；指针槽 `handoff_*` / `mls_vault_*` / `mls_signing_*`。

**加好友**：`contact_req` → 对端 `contact_fetch` → 处理 → `contact_consume`（authenticated）。

---

## 6. 前端模块对照

| 能力 | 模块 |
|------|------|
| 长连接 / 重连 / 控制面 | `wsRelay.ts`, `relayClient.ts`, `multiplexRelay.ts` |
| 一次性 RPC | `relayOneShot.ts` |
| 六槽指针 | `*Pointer.ts`, `offchainSyncCoordinator.ts` |
| 邮箱 | `chatMailbox.ts`, `contactRequestInbox.ts`, `groupInviteInbox.ts` |
| Inbox / 盲签 | `delivery/inboxManager.ts`, `delivery/tokenWallet.ts` |
| Wire 能力探测 | `relayWireCapabilities.ts`（连接后一次性） |
| 账户绑定签名 | `registerAccountAuth.ts` |

---

## 7. 错误回执汇总

| type | 触发 |
|------|------|
| `auth_reject` | Writer/Reader 门禁失败 |
| `inbox_reject` | `stale_epoch` |
| `index_reject` 等 | `stale_updated_at` |
| `commit_reject` | Gate-2 CAS 落败 |
| `frame_reject` | `rate_limited` 或 strict `delivery_rejected` |
| `admin_reject` | `forbidden` |
| 静默丢弃 | 非 JSON、超大帧、无 dedupKey 的限流丢弃 |

`*_ack` **不回显** `request_id`；按 `type`+`account` 关联。

**前端恢复（`src/relay/relayErrors.ts` + 调用方）**：
- 指针 `*_reject{stale_updated_at}` → `publishCloudPointer` 拉远端指针写本地。
- `inbox_reject{stale_epoch}` → `InboxManager` 采纳服务端 `epoch` 并重试一次注册。
- `auth_reject` → `relayOneShotSend` 抛错；`relayOneShotFetch` 返回 `null`。
- `frame_reject` → `appStore` 用 `frameRejectHint(reason)` 标失败（含 `delivery_rejected`）。

---

## 8. 环境变量速查

| 项 | 默认 | 变量 |
|----|------|------|
| 端口 | 8765 | `RELAY_PORT` |
| 单帧上限 | 256 KiB | `RELAY_MAX_MSG_BYTES` |
| 限频 | 120/min | `RELAY_RATE_LIMIT` |
| 严格鉴权 | off | `RELAY_STRICT_AUTH` |
| 聊天邮箱 TTL | 180d | `RELAY_CHAT_MAILBOX_TTL_MS` |
| 聊天帧/字节上限 | 5000 / 256MB | `RELAY_CHAT_MAILBOX_MAX_*` |
| spent 上限/inbox | 50000 | `RELAY_SPENT_CAP` |
| MLS 邮箱上限 | 2000 帧 / 64MB | `RELAY_MLS_MAILBOX_MAX_*` |
| Admin | loopback only | `RELAY_ADMIN_SECRET` |

---

## 9. 生产验收 / Production probe

对照 `.env.production` 的能力矩阵（Wire、Track A 指针、邮箱、inbox）：

```bash
cd scripts && npm run test:nexchat:relay-prod
```

探测项：`mls_backlog_req`、`peer_add_req`、`commit_epoch_cas`、`device_join`、`handoff_control`、`mls_signing_pointer`、`mls_vault_pointer`、三槽基础指针、`chat_fetch`、`contact_fetch`、`inbox_lookup`。

本地 Wire 矩阵 + live e2e：见 `scripts/docs/NEXUS_TEST_PLAN.md`（`test:nexchat:relay-wire`、`nexchat` 的 `e2e:wire-relay`）。

---

## 10. 最小 TS 示例（摘录）

生产请直接用 `src/relay/*` 封装，勿复制裸 WebSocket 逻辑（consume 必须带 `register_account`）。

```ts
import { registerAccountWire } from "@/relay/registerAccountAuth";
import { relayOneShotSend, relayOneShotFetch } from "@/relay/relayOneShot";

// Authenticated consume (contact mailbox)
await relayOneShotSend(account, {
  type: "contact_consume",
  account,
  req_ids: ["r1"],
  ack_ids: [],
}, { noReply: true });

// Authenticated chat fetch
await relayOneShotFetch(account, { type: "chat_fetch" }, (m, requestId) =>
  m.type === "chat_reply" && m.request_id === requestId ? m.frames : undefined,
);
```

长连接注册：

```ts
ws.send(JSON.stringify({ type: "register", id: endpointId }));
ws.send(JSON.stringify(registerAccountWire(endpointId, account)));
```
