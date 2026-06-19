# Pallet Chat Common

聊天子系统的轻量共享库。**不是 pallet**：无 storage、无 extrinsic、无权重、无 runtime 实例。

链下收敛后（人类消息 Text/Image/File/Voice，无论私聊还是群聊，均走链下 MLS + relay；
链上仅存 `System` 通知），本 crate 仅保留**真正跨 pallet / runtime 共享**的四块：

| 模块 | 内容 | 使用方 |
| --- | --- | --- |
| `rate_limit` | 滑动窗口反垃圾计数 + 块高冷却 `min_blocks_elapsed` | `group`（MLS 写入限频、建群冷却）、`permission`（举报冷却）、`sync`（发布间隔） |
| `deposit` | `reserve_deposit` / `unreserve_deposit` 薄封装 | `group`、`inbox`、`msg-identity`、`sync`（注册/建群押金门控） |
| `epoch` | `bump_u32_epoch` / `next_u32_epoch`（饱和 +1） | `permission`（`CapabilityEpoch`）、`inbox`（信箱 epoch）、`msg-identity`（预密钥 epoch） |
| `runtime_api` | 统一会话视图 `ChatViewApi` + `ConversationSummary` | `runtime/src/apis.rs` 聚合、`node/src/chat_rpc.rs` 封装为 `chat_*` RPC |

> 各 pallet 的 epoch **键正交**（账户能力 / inbox / 设备预密钥不复用同一 storage），
> `epoch` 模块只共享算术，不合并布局或事件。

## 1. `rate_limit`

滑动窗口限频：窗口（区块数）+ 窗口内最大次数。状态 `RateLimitState { last_time, count }`
可上链存储（实现 `MaxEncodedLen`）。

```rust
use pallet_chat_common::rate_limit::{check_and_update_rate_limit, RateLimitResult, RateLimitState};

let mut state = RateLimitState::<BlockNumber>::new();
let res = check_and_update_rate_limit(&mut state, now, window, max_count);
ensure!(res == RateLimitResult::Allowed, Error::<T>::RateLimited);
```

辅助函数：

- `check_rate_limit` — 只读检查，不更新状态
- `reset_rate_limit` — 重置状态
- `remaining_quota` — 窗口内剩余配额
- `min_blocks_elapsed(last, now, min_interval)` — 块高冷却（建群冷却、举报冷却、sync 发布间隔）

单测覆盖：`check_and_update_rate_limit`、`remaining_quota`、`min_blocks_elapsed`。

## 2. `deposit`

经 `frame_support::traits::ReservableCurrency` 预留/退还反垃圾押金，避免各 pallet 重复
`Currency::reserve` / `unreserve` 样板代码。

```rust
use pallet_chat_common::{reserve_deposit, unreserve_deposit};

reserve_deposit::<T::Currency, _, _>(&who, amount)?;
// … 业务逻辑 …
unreserve_deposit::<T::Currency, _, _>(&who, amount);
```

`unreserve_deposit` 忽略 leftover（与 Substrate `unreserve` 惯例一致）。

## 3. `epoch`

u32 撤销/发布纪元的饱和递增，供链下令牌新鲜度比对（各 pallet 自行维护 storage 键）：

```rust
use pallet_chat_common::{bump_u32_epoch, next_u32_epoch};

let new_epoch = CapabilityEpoch::<T>::mutate(&who, bump_u32_epoch);
let preview = next_u32_epoch(rec.epoch); // 只读预览，不改 storage
```

单测覆盖：正常递增与 `u32::MAX` 饱和。

## 4. `runtime_api`：统一会话视图

`ChatViewApi` 把私聊（`pallet-chat-core`）+ 群聊（`pallet-chat-group`）聚合为单一会话
列表。trait 定义在 `common`（core / group 互不依赖），聚合逻辑在 runtime 的
`impl_runtime_apis!` 落地（那里能同时访问 `ChatCore` 与 `ChatGroup`）。

```rust
fn list_conversations(who) -> Vec<ConversationSummary<AccountId, Hash, BlockNumber>>;
fn total_direct_unread(who) -> u32;
```

`ConversationSummary` 字段按 `kind`（`Direct` / `Group`）填充；群角色以稳定 `u8` 常量
（`role::OWNER` / `ADMIN` / `MEMBER` / `NONE`）表达，避免 DTO 依赖 `pallet-chat-group` 类型。

> **⚠️ 链上切片 ≠ 完整消息列表（客户端必读）**
> 本 API 仅返回**链上切片**。人类聊天（私聊与群聊）在链下；链上唯一消息是 `System` 通知。
> 因此私聊 `unread` / `last_active` 只反映 **System 通道**（纯链下聊过的用户对**没有**私聊行），
> 群聊 `unread` / `last_active` 恒为 `0`，`muted` 在私聊（免打扰 DND）与群聊（管理员禁言/不能
> 发言）下语义不同。渲染真实「消息」页**必须**与本地/链下 MLS 状态合并——详见
> `pallets/chat/README.md` 的「链上 / 链下边界」与「客户端 Merge Spec」。

Node 封装（只读、免费）：`chat_listConversations`、`chat_totalDirectUnread`（见
`node/src/chat_rpc.rs`）。

## 5. 依赖

| 依赖 | 用途 |
| --- | --- |
| `codec` / `scale-info` | `RateLimitState`、`ConversationSummary` 编码 |
| `sp-std` / `sp-runtime` | 无 std 构建、饱和算术 |
| `sp-api` | `decl_runtime_apis!` |
| `frame-support` | `deposit` 的 `ReservableCurrency` |

不依赖任何 chat pallet；`core` / `permission` **不**反向依赖本 crate（审计 P1 已移除 dead dep）。

## 6. 设计变更说明（审计 P1：类型收敛）

历史版本曾把 common 设计为「聊天统一类型库」，提供 `MessageType` / `MessageStatus` /
`EncryptionMode` / `ChatUserId` 与 `ChatPermissionCheck` 等跨 pallet trait，以及 CID
「加密」启发式。这些在链下收敛后**无任何调用方**，且 `MessageType` 的判别值与
`pallet-chat-core` 链上枚举发散，是潜在编码地雷，故**已整体删除**。当前唯一事实来源：

- 链上权威 `MessageType` → `pallet-chat-core`（仅 `System` 真正上链）。
- 权限端口 → `pallet-chat-permission::ChatPermissionChecker`。
- `ChatUserId` → `pallet-chat-core` 内定义与注册表。
- 消息分类 / 加密 → 客户端 / 链下职责，由 MLS 端到端加密保证（链不做 CID 加密校验）。

## 7. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **职责边界** | ✅ 仅共享工具 + Runtime API 定义；无 pallet 实例、无链上状态 |
| **类型收敛（P1）** | ✅ 发散 `MessageType` / trait 端口已删；单一事实来源在 core / permission |
| **调用方接线** | ✅ `group` / `permission` / `inbox` / `msg-identity` / `sync` + runtime + node RPC |
| **链上/链下契约** | ✅ `runtime_api` 与 chat 父 README Merge Spec 一致；`muted` 语义按 `kind` 分支已文档化 |
| **单测** | ✅ `rate_limit` + `epoch` 有单元测试（`cargo test -p pallet-chat-common`） |
| **编译告警** | ✅ 无 unused import / warning（`epoch` 依赖 `saturating_add` 内建，无需显式 trait import） |
| **安全面** | ✅ 纯算术/薄封装；限频与冷却逻辑有界、饱和；押金走标准 `ReservableCurrency` |
| **缺口（非阻塞）** | ⚪ `deposit` 无独立单测（逻辑为单行委托，风险低）；⚪ 无 benchmark（库 crate，不适用） |

**总评：达到上线标准。** 本 crate 为无状态共享库，链上行为由调用方 pallet 承担；
审计遗留的 P1 类型收敛已完成，Runtime API 契约与客户端 Merge Spec 已对齐。
