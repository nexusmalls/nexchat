# pallet-chat-core

私聊核心 pallet：链上仅保存 **System 通知** 与会话级元数据；人类聊天（Text/Image/File/Voice）
全链下（MLS 端到端加密 + relay）。本模块是 `pallets/chat/` 模块集中的 `core`，已在 runtime
注册为 `ChatCore`。

> ⚠️ 本文档随代码维护。历史上曾描述的链上黑名单、`is_cid_encrypted` 加密判断、
> 任意用户 `send_message`、链上好友图谱等**均已移除**，请勿参考旧版本说明。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上** | `Session`、消息元数据 `MessageMeta`（CID、收发方、时间、已读/删除/撤回）、未读计数、会话 DND/置顶/归档、`ChatUserId` 注册表与资料 |
| **链下** | 人类消息内容与投递（MLS + relay）；「谁与谁聊、聊什么」不触链（隐私：隐藏通信关系） |
| **链上消息类型** | **仅 `MessageType::System`**（订单/争议/治理等平台通知） |

人类类型枚举（`Text`/`Image`/`File`/`Voice`）仅为 **SCALE 索引兼容** 保留；
`send_message` 传入人类类型一律返回 `HumanMessagesOffChain`。

**与统一会话视图的关系：** 私聊行出现在 `ChatViewApi::list_conversations` **仅当**
该对用户间发过 System 消息；纯链下私聊无链上行。客户端须与链下 MLS 状态合并，见
`pallets/chat/README.md` 的 Merge Spec。

## 2. System 消息发送（三条路径）

### 2.1 Extrinsic：`send_message`（治理 / Root）

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `send_message(receiver, content_cid, msg_type_code, session_id)` | **唯一** extrinsic System 入口；仅 `msg_type_code == 4`（System），其余 `HumanMessagesOffChain` |

- **来源门控（审计 B2）**：`SystemMessageOrigin` → 生产为 `EnsureRootWithSuccess<AccountId, ChatSystemMessenger>`（仅治理/Root）；`sender` 记为 PalletId `chat/sys` 派生系统账户。
- **权限**：System **绕过** `ChatPermission::can_send_message`（平台通知须无视接收方隐私级别送达）；纵深防御保留在内部 `do_send` 的非 System 分支（当前对外不可达）。
- **CID**：仅非空 + 长度上限（`InvalidCid` / `CidTooLong`）；**不**校验加密（审计 C，MLS E2EE 在客户端）。
- **会话注入防护**：传入 `session_id` 时校验收发双方均为参与者（`NotSessionParticipant`）。

> 原 `send_system_message`（call_index 16）已删除（审计 2.1）；索引 16 留空不复用。

### 2.2 Trait：`SystemNotifier::notify`（业务 pallet 内部）

```rust
pub trait SystemNotifier<AccountId> {
    fn notify(receiver: &AccountId, notice: Vec<u8>) -> DispatchResult;
}
```

- **不是 extrinsic**；仅 runtime 显式接线的业务 pallet 可调用（编译期门控，审计 B2 加强）。
- `sender` = `Config::SystemAccount`（与 `ChatSystemMessenger` 同一派生账户）。
- `notice` 为**不透明客户端本地化描述符**（模板码 + 参数），存入 `content_cid`；**不要求**是真 IPFS CID。
- 落入 `system_account ↔ receiver` 平台通知会话，**不**重建 buyer↔seller 等业务关系（隐私友好）。

### 2.3 尽力桥接：`notify_system_best_effort`

Runtime 适配器对业务 pallet 暴露的包装：失败时发 `SystemNotifyFailed` 事件、**不**回滚调用方状态
（如 `pallet-entity-order` 订单通知）。内部仍调用 `SystemNotifier::notify`。

## 3. 会话与消息状态

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 1 | `mark_as_read(msg_id)` | 接收方标记单条已读，幂等，递减未读 |
| 3 | `mark_batch_as_read(message_ids)` | 批量标记；列表上限 `MAX_BATCH_READ=512`；权重随长度计费 |
| 4 | `mark_session_as_read(session_id)` | 有界扫描 `MAX_SESSION_READ_SCAN=512`；`SessionReadCursor` 按 msg_id **升序**推进，重复调用可扫完全会话（审计 B3） |
| 2 | `delete_message(msg_id)` | 单边软删除；接收方删未读消息时同步抵消未读（审计 B1） |
| 17 | `recall_message(msg_id)` | 发送方在 `MessageRecallWindow` 内撤回，双方隐藏；撤回未读同步抵消 |
| 5 | `archive_session(session_id)` | 归档 |
| 18 | `set_session_muted(session_id, muted)` | 会话免打扰 DND（仅调用者提醒偏好，不限制对方发送） |
| 19 | `set_session_pinned(session_id, pinned)` | 会话置顶（`list_sessions` 置顶优先） |

会话 ID = 参与者地址排序后哈希，每对用户确定且唯一。

**`muted` 语义提醒：** 本 pallet 的 muted 是 **DND（不收提醒）**；群聊 `ChatViewApi` 中的
`muted` 是管理员禁言——客户端必须按 `kind` 分支渲染。

## 4. ChatUserId 与资料

11 位数字 ID（`10_000_000_000`–`99_999_999_999`），`Randomness` + `NextChatUserId` nonce 生成，
碰撞重试；账户 ↔ ID 双向 O(1) 映射。

| call_index | extrinsic |
| --- | --- |
| 12 | `register_chat_user(nickname?)` |
| 13 | `update_chat_profile(nickname?, avatar_cid?, signature?)` |
| 14 | `set_user_status(status_code)` |
| 15 | `update_privacy_settings(show_online?, show_last_active?)` |

- 发送 System 消息时自动 `get_or_create_chat_user_id`（双方）。
- `ProfileDisplaySettings`（原 `PrivacySettings`，审计 2.8）：**纯 UI 展示偏好**；通信权限唯一来源为 `pallet-chat-permission::permission_level`。

## 5. 运维：过期消息清理

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 8 | `cleanup_old_messages(limit)` | **仅 Root**；`LastCleanupCursor` 增量扫描；`limit` 1–1000；仅删「已过期且收发双方均软删除」的消息（审计 G） |

## 6. 主要存储

| 存储 | 说明 |
| --- | --- |
| `Messages` / `NextMessageId` / `LastCleanupCursor` | 消息元数据、ID 计数、GC 游标 |
| `Sessions` / `UserSessions` / `SessionMessages` | 会话与索引 |
| `SessionReadCursor` | `mark_session_as_read` 增量扫描游标（per user × session） |
| `UnreadCount` | `(account, session_id) → u32` |
| `SessionMuted` / `SessionPinned` | 每用户、每会话 DND / 置顶 |
| `AccountToChatUserId` / `ChatUserIdToAccount` / `ChatUserProfiles` / `UsedChatUserIds` / `NextChatUserId` | ChatUserId 注册表 |

已移除：`MessageRateLimit`、链上黑名单（拉黑走 `permission::bump_capability_epoch` +
`inbox::revoke_tag`）。

## 7. 配置（`Config`）

| 关联类型 / 常量 | 说明 |
| --- | --- |
| `WeightInfo` | 见 `src/weights.rs`（节点 benchmark 实测） |
| `MaxCidLen` | runtime：`96` |
| `MessageExpirationTime` | runtime：`180 * DAYS` |
| `MessageRecallWindow` | runtime：`2 * MINUTES` |
| `Randomness` / `UnixTime` | ChatUserId 生成与时间戳 |
| `MaxNicknameLength` / `MaxSignatureLength` | runtime：`64` / `256` |
| `ChatPermission` | `pallet-chat-permission::ChatPermissionChecker` |
| `SystemMessageOrigin` | System extrinsic 特权来源 |
| `SystemAccount` | 程序化通知 `sender`（= `ChatSystemMessenger`） |

## 8. 依赖关系

```
pallet-chat-permission  ←── pallet-chat-core
         ↑
    runtime ChatPermission 端口
```

不依赖 `pallet-chat-common`（审计 P1 已移除 dead dep）。统一会话视图由 `common::ChatViewApi`
定义、runtime 聚合。

## 9. 查询 / Runtime API

只读公共函数：

- 消息：`get_message` / `list_messages_by_session`（分页）
- 会话：`get_session` / `list_sessions`（置顶优先 + 最后活跃倒序）/ `get_session_id`
- 状态：`get_unread_count` / `is_session_muted` / `is_session_pinned`
- ChatUserId：`get_account_by_chat_user_id` / `get_chat_user_id_by_account` / `get_chat_user_profile`

统一私聊+群聊列表：`pallet-chat-common::runtime_api::ChatViewApi`（runtime 实现，node `chat_*` RPC）。

## 10. 主要事件

`MessageSent` / `MessageRead` / `MessageDeleted` / `MessageRecalled` / `SessionCreated` /
`SessionMarkedAsRead` / `SessionArchived` / `SessionMuteSet` / `SessionPinSet` /
`OldMessagesCleanedUp` / `SystemNotifyFailed` / `ChatUserCreated` / `ChatUserProfileUpdated` /
`ChatUserStatusChanged` / `PrivacySettingsUpdated`

> `MessageSentWithChatId` 已合并入 `MessageSent`（审计 2.4）；`content_cid` 不入事件（在 `Messages` 存储）。

## 11. 已知限制

- **权重**：已由节点 benchmark 生成；主网前应在参考硬件重跑 `runtime-benchmarks`。
- **`mark_session_as_read`**：单次最多 512 条；超大会话需客户端重复调用。
- **`list_sessions` / `get_unread_count(None)`**：按用户前缀迭代，重度用户 RPC 侧需自行限流（Runtime `list_conversations` 已 cap 512 行）。

## 12. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **链下收敛** | ✅ 人类消息拒绝上链；`HumanMessagesOffChain` 有单测 |
| **安全 B2** | ✅ System 仅 Root/治理 extrinsic；`SystemNotifier` 非 extrinsic、业务 pallet 编译期接线 |
| **安全 B3** | ✅ `mark_session_as_read` 有界扫描 + `SessionReadCursor` 升序推进 |
| **安全 B1** | ✅ 接收方删除未读消息抵消角标 |
| **权限单一来源** | ✅ `ChatPermission` 端口；黑名单/好友图谱已下链 |
| **CID 姿态（审计 C）** | ✅ 仅非空+长度；不伪造「加密校验」 |
| **GC（审计 G）** | ✅ `cleanup_old_messages` 游标化、权重按 `limit` 计量 |
| **Runtime 接线** | ✅ `ChatCore` + `ChatSystemMessenger` + 订单等业务 `notify_system_best_effort` |
| **权重 / 基准** | ✅ 全 extrinsic 有 `WeightInfo` + `benchmarking.rs`；2026-06-19 dev 链重跑 |
| **单测** | ✅ 70 项通过（`cargo test -p pallet-chat-core`） |
| **缺口（非阻塞）** | ⚪ 主网前在参考硬件重跑 benchmark（命令见 [`pallets/chat/README.md`](../README.md#基准测试与权重--benchmarks--weights)） |

**总评：达到上线标准。** 核心隐私边界（人类消息链下、System 受信来源、会话注入防护、
有界已读扫描）已落地并有测试覆盖；剩余项为文档收尾与主网前权重复测惯例。
