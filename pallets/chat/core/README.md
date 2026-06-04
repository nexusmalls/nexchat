# pallet-chat-core

私聊核心模块：链上仅保存**会话元数据**与会话级状态；消息内容与人类聊天走链下
（MLS 端到端加密 + relay）。本模块是 `pallets/chat/` 模块集中的 `core`。

> ⚠️ 本文档随代码维护。历史上曾描述的链上黑名单、`is_cid_encrypted` 加密判断、
> 任意用户 `send_message`、链上好友图谱等**均已移除**，请勿参考旧版本说明。

## 1. 定位与边界

- **链上**：会话（`Session`）、消息元数据（`MessageMeta`，含 IPFS CID、收发方、时间、
  已读/删除/撤回标记）、未读计数、会话免打扰/置顶、ChatUserId 注册表与资料。
- **链下**：人类聊天消息（Text/Image/File/Voice）的内容与投递，由 MLS + relay 承载，
  「谁与谁聊、聊什么」不触链（隐私目标：隐藏通信关系）。
- **链上消息仅 `System` 一种**：用于订单状态、仲裁通知等**低频系统事件**。

## 2. 消息发送（仅 System 上链）

两个入口，均把消息类型固定为 `System` 落库：

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `send_message(receiver, content_cid, msg_type_code, session_id)` | 通用入口，**已收窄**：仅接受 `msg_type_code == 4`（System），其余返回 `HumanMessagesOffChain` |
| 16 | `send_system_message(receiver, content_cid, session_id)` | 推荐入口，强制 `System` 类型 |

### 受信来源与权限模型（重要）

两个入口都经 `SystemMessageOrigin: EnsureOrigin<_, Success = AccountId>` 鉴权：
- 生产 runtime 配置为 `EnsureRootWithSuccess<AccountId, ChatSystemMessenger>`——
  **仅治理 / Root** 可发，`sender` 记为 PalletId 派生的系统账户（防止任意用户伪造系统通知，审计 B2）。
- 单测 mock 用 `EnsureSigned` 以保留既有用例语义。

System 消息是平台通知，必须无视接收方隐私级别送达，且受信来源不受反垃圾限频约束，
因此 **System 消息绕过 `ChatPermission::can_send_message` 权限闸门与频率限制**——
受信边界由 `SystemMessageOrigin` 在入口处强制。权限闸门与限频仅对（当前不存在的）
非 System 路径生效，作为未来扩展的预留。

### CID 校验

仅做格式 sanity：**非空**（否则 `InvalidCid`）+ **不超长**（否则 `CidTooLong`）。
链**不**判断 CID 是否加密——加密由客户端 MLS E2EE 保证，链只存不透明 CID（审计 C）。

### 会话注入防护

传入 `session_id` 时校验收发双方都是该会话参与者（否则 `NotSessionParticipant`），
防止把消息注入无关的第三方会话。

## 3. 会话与消息状态

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 1 | `mark_as_read(msg_id)` | 接收方标记单条已读，幂等，递减未读 |
| 3 | `mark_batch_as_read(message_ids)` | 批量标记，权重随列表长度计费 |
| 4 | `mark_session_as_read(session_id)` | 按会话标记；**单次最多扫描 `MAX_SESSION_READ_SCAN=512` 条**，按本次实际标记数递减未读，超量时客户端重复调用（审计 B3） |
| 2 | `delete_message(msg_id)` | 单边软删除：发送/接收方各自隐藏，互不影响；接收方删除「仍计未读」的消息时同步抵消未读，避免角标卡住 |
| 17 | `recall_message(msg_id)` | **发送方**在 `MessageRecallWindow` 内撤回，**双方隐藏**（客户端显示「消息已撤回」占位）；撤回未读消息同步抵消未读 |
| 5 | `archive_session(session_id)` | 归档（前端可隐藏） |
| 18 | `set_session_muted(session_id, muted)` | 会话免打扰（仅影响调用者提醒，不限制对方发送） |
| 19 | `set_session_pinned(session_id, pinned)` | 会话置顶（`list_sessions` 中置顶优先） |

会话 ID 由参与者地址排序后哈希生成，确定且每对用户唯一。

## 4. ChatUserId 与资料

11 位数字 ID（`10_000_000_000`–`99_999_999_999`），多源随机 + 全局 nonce 生成，
碰撞重试；账户 ↔ ID 双向映射，O(1) 生成（不再全表扫描）。

| call_index | extrinsic |
| --- | --- |
| 12 | `register_chat_user(nickname?)` |
| 13 | `update_chat_profile(nickname?, avatar_cid?, signature?)` |
| 14 | `set_user_status(status_code)` |
| 15 | `update_privacy_settings(allow_stranger?, show_online?, show_last_active?)` |

> `PrivacySettings.allow_stranger_messages` 已弃用为纯展示标志；通信权限的唯一判定
> 来源是 `pallet-chat-permission` 的 `permission_level`。

## 5. 运维：过期消息清理

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 8 | `cleanup_old_messages(limit)` | **仅 Root/治理**（`ensure_root`）。从 `LastCleanupCursor` 游标处增量扫描至多 `limit`（1–1000）条，仅移除「已过期且收发双方都软删除」的消息；权重按 `limit` 据实计量（审计 G） |

## 6. 主要存储

- `Messages: u64 -> MessageMeta`、`NextMessageId`、`LastCleanupCursor`
- `Sessions: Hash -> Session`、`UserSessions(account, session) -> ()`、
  `SessionMessages(session, msg_id) -> ()`
- `UnreadCount((account, session)) -> u32`
- `SessionMuted` / `SessionPinned`（每用户、每会话）
- `MessageRateLimit`（非 System 路径限频状态）
- `AccountToChatUserId` / `ChatUserIdToAccount` / `ChatUserProfiles` / `UsedChatUserIds` / `NextChatUserId`

> 黑名单存储与 `block_user`/`unblock_user`（原 call_index 6/7）已移除，索引留空不复用；
> 拉黑改由链下能力令牌撤销（`pallet-chat-permission::bump_capability_epoch`）/ 信箱定向
> 标签撤销（`pallet-chat-inbox::revoke_tag`）。

## 7. 配置（`Config`）

`RuntimeEvent`、`WeightInfo`、`MaxCidLen`、`RateLimitWindow`、`MaxMessagesPerWindow`、
`MessageExpirationTime`、`MessageRecallWindow`、`Randomness`、`UnixTime`、
`MaxNicknameLength`、`MaxSignatureLength`、`ChatPermission`（权限端口）、
`SystemMessageOrigin`（System 通道特权来源）。

> 历史死配置 `MaxSessionsPerUser` / `MaxMessagesPerSession` 已移除（审计 L）。

## 8. 查询 / Runtime API

只读公共函数：`get_message` / `list_messages_by_session`（分页）/ `get_session` /
`list_sessions`（置顶优先、其余按最后活跃倒序）/ `get_unread_count` /
`is_session_muted` / `is_session_pinned`，以及 ChatUserId 正反查询与资料读取。

统一会话视图（私聊 + 群聊聚合）由 `pallet-chat-common::runtime_api::ChatViewApi`
定义、在 runtime `impl_runtime_apis!` 落地，并由 node 端封装为 `chat_*` JSON-RPC。

## 9. 已知限制

- **权重为占位值**，尚未用节点 benchmark 实测；上主网前需替换为实测 `WeightInfo`。
- `list_sessions` / `get_unread_count(None)` 等只读接口按用户前缀全量迭代，属链下/RPC
  开销（非 extrinsic），重度用户需依赖 RPC 侧自身上限。
