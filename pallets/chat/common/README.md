# Pallet Chat Common

聊天系统的轻量共享构件。**不是 pallet**：无 storage、无 extrinsic、无权重。

链下收敛后（人类消息 Text/Image/File/Voice，无论私聊还是群聊，均走链下 MLS + relay；
链上仅存 `System` 通知），本 crate 仅保留**真正跨 pallet / runtime 共享**的两块：

| 模块 | 内容 | 使用方 |
| --- | --- | --- |
| `rate_limit` | 窗口化反垃圾计数器 | `pallet-chat-group`（约束写入型 MLS 操作） |
| `runtime_api` | 统一会话视图 `ChatViewApi` + `ConversationSummary` | `runtime/src/apis.rs` 聚合、`node/src/chat_rpc.rs` 封装为 `chat_*` RPC |

## 1. `rate_limit`

滑动窗口限频：窗口（区块数）+ 窗口内最大次数。状态 `RateLimitState { last_time, count }`
可上链存储（实现 `MaxEncodedLen`）。

```rust
use pallet_chat_common::rate_limit::{check_and_update_rate_limit, RateLimitResult, RateLimitState};

let mut state = RateLimitState::<BlockNumber>::new();
let res = check_and_update_rate_limit(&mut state, now, window, max_count);
ensure!(res == RateLimitResult::Allowed, Error::<T>::RateLimited);
```

辅助函数：`check_rate_limit`（只读不更新）、`reset_rate_limit`、`remaining_quota`。

## 2. `runtime_api`：统一会话视图

`ChatViewApi` 把私聊（`pallet-chat-core`）+ 群聊（`pallet-chat-group`）聚合为单一会话
列表。trait 定义在 `common`（core / group 互不依赖），聚合逻辑在 runtime 的
`impl_runtime_apis!` 落地（那里能同时访问 `ChatCore` 与 `ChatGroup`）。

```rust
fn list_conversations(who) -> Vec<ConversationSummary<AccountId, Hash, BlockNumber>>;
fn total_direct_unread(who) -> u32;
```

> **⚠️ 链上切片 ≠ 完整消息列表（客户端必读）**
> 本 API 仅返回**链上切片**。人类聊天（私聊与群聊）在链下；链上唯一消息是 `System` 通知。
> 因此私聊 `unread` / `last_active` 只反映 **System 通道**（纯链下聊过的用户对**没有**私聊行），
> 群聊 `unread` / `last_active` 恒为 `0`，`muted` 在私聊（免打扰 DND）与群聊（管理员禁言/不能
> 发言）下语义不同。渲染真实「消息」页**必须**与本地/链下 MLS 状态合并——详见
> `pallets/chat/README.md` 的「链上 / 链下边界」与「客户端 Merge Spec」。

## 3. 依赖

仅依赖 Substrate 核心库（`codec` / `scale-info` / `sp-std` / `sp-runtime` / `sp-api`），
不依赖任何 pallet。

## 4. 设计变更说明（审计 P1：类型收敛）

历史版本曾把 common 设计为「聊天统一类型库」，提供 `MessageType` / `MessageStatus` /
`EncryptionMode` / `ChatUserId` 与 `ChatPermissionCheck` 等跨 pallet trait，以及 CID
「加密」启发式。这些在链下收敛后**无任何调用方**，且 `MessageType` 的判别值与
`pallet-chat-core` 链上枚举发散，是潜在编码地雷，故**已整体删除**。当前唯一事实来源：

- 链上权威 `MessageType` → `pallet-chat-core`（仅 `System` 真正上链）。
- 权限端口 → `pallet-chat-permission::ChatPermissionChecker`。
- `ChatUserId` → `pallet-chat-core` 内定义与注册表。
- 消息分类 / 加密 → 客户端 / 链下职责，由 MLS 端到端加密保证（链不做 CID 加密校验）。
