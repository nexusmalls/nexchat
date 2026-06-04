# Stardust 聊天系统模块

统一的聊天系统模块集合，提供完整的即时通讯（私聊 + MLS 群聊 + 权限）功能。

## 模块概览

```
pallets/chat/
├── common/       # 轻量共享构件（rate_limit + ChatViewApi）[已接入]
├── permission/   # 权限系统（场景授权+黑白名单）   [已接入 runtime]
├── core/         # 核心私聊模块                  [已接入 runtime]
└── group/        # MLS 群聊模块（RFC 9420 锚定）  [已接入 runtime]
```

> **模块状态说明**
> - `common` / `permission` / `core` / `group` 已在根 `Cargo.toml` 的 workspace
>   members 中，并在 `runtime` 注册（`ChatPermission` / `ChatCore` / `ChatGroup`）。
> - 合并前的旧版顶层 `pallet-chat`（`pallets/chat/src/`）与未接入 runtime 的 AI 对话
>   子模块（原 `pallets/chat/ai`）已随收口删除；私聊功能统一由 `core/` 提供，
>   AI 对话能力如需重建可基于 `pallet-deceased` / `pallet-deceased-ai` 另行规划。

## 模块依赖关系

```
       ┌─────────────┐
       │   common    │  rate_limit（被 group 使用）+ runtime_api::ChatViewApi（被 runtime 使用）
       └──────┬──────┘
              │ rate_limit
              ▼
       ┌─────────────┐      ┌────────────┐ ┌─────────┐
       │    group    │      │ permission │ │  core   │  ← 不再依赖 common（审计 P1）
       └─────────────┘      └─────┬──────┘ └────┬────┘
                                  └─────────────┘
                            core 依赖 permission（ChatPermissionChecker）
```

> 注：`core` / `permission` 已移除对 `common` 的依赖（原为声明但零 import 的 dead dep）。
> `common::runtime_api` 由 `runtime/src/apis.rs` 聚合实现、`node/src/chat_rpc.rs` 封装。

## 各模块功能

### common - 轻量共享构件

链下收敛后仅保留**真正跨 pallet / runtime 共享**的两块（详见 `common/README.md`）：

- **RateLimit**（`rate_limit`）：窗口化反垃圾计数器，`pallet-chat-group` 用于约束写入型
  MLS 操作。
- **ChatViewApi**（`runtime_api`）：统一会话视图 Runtime API（私聊 + 群聊聚合），在 runtime
  落地、node 封装为 `chat_*` RPC。

> 审计 P1（类型收敛）：历史的 `MessageType` / `MessageStatus` / `EncryptionMode` /
> `ChatUserId` 与 `ChatPermissionCheck` 等跨 pallet trait、CID「加密」启发式**已删除**
> （零调用方 + `MessageType` 判别值发散是编码地雷）。链上权威 `MessageType` 在
> `pallet-chat-core`；权限走 `pallet-chat-permission::ChatPermissionChecker`。

### permission - 权限系统

场景化权限控制：

- **SceneType**: 支持多种场景（私聊/群聊/AI对话/逝者纪念等）
- **PermissionLevel**: 权限级别（Block/ReadOnly/Normal/Premium/Admin）
- **白名单/黑名单**: 灵活的访问控制
- **临时授权**: 带过期时间的访问令牌

### core - 核心私聊

基础的一对一聊天功能：

- **ChatUserId分配**: 唯一11位数字ID
- **消息发送**: 支持多种消息类型
- **消息状态**: 已发送/已送达/已读/已撤回
- **权限集成**: 基于permission模块的访问控制

### group - 智能群聊

四种加密模式的群组聊天：

| 模式 | 描述 | 适用场景 |
|------|------|----------|
| Military | 量子抗性加密 | 高度机密群组 |
| Business | AES-256加密 | 普通私密群组（默认）|
| Selective | 选择性加密 | 部分消息需加密 |
| Transparent | 透明公开 | 公开群组 |

功能：
- 群组创建/解散
- 成员管理（加入/离开/踢出）
- 群组消息广播
- 管理员权限管理

## Runtime 配置

在 `runtime/Cargo.toml` 中添加依赖（`ai` 未接入，故不在此列）：

```toml
pallet-chat-common = { path = "../pallets/chat/common", default-features = false }
pallet-chat-permission = { path = "../pallets/chat/permission", default-features = false }
pallet-chat-core = { path = "../pallets/chat/core", default-features = false }
pallet-chat-group = { path = "../pallets/chat/group", default-features = false }
pallet-chat-inbox = { path = "../pallets/chat/inbox", default-features = false }
```

在 `std` feature 中添加：

```toml
"pallet-chat-common/std",
"pallet-chat-permission/std",
"pallet-chat-core/std",
"pallet-chat-group/std",
"pallet-chat-inbox/std",
```

## Runtime API：统一会话视图

`pallet-chat-common::runtime_api::ChatViewApi` 把**私聊（core）+ 群聊（group）**聚合为
单一会话列表，供前端"消息"页直接渲染。该 trait 定义在 `common`（core / group 互不依赖），
聚合逻辑在 `runtime/src/apis.rs` 的 `impl_runtime_apis!` 中实现（那里可同时访问
`ChatCore` 与 `ChatGroup`）。前端经 `api.call.chatViewApi.*` 免费调用，无需 gas。

- `list_conversations(who) -> Vec<ConversationSummary>`：**链上切片**，私聊在前（core 已按
  "置顶优先 + 最后活跃倒序"排序），群聊在后并按 `group_id` **升序**（确定、可分页基线；真实
  活跃度在链下）。**不是**完整消息列表，见下方边界与 Merge Spec。

> 链下私聊为何不在列表、以及换设备如何恢复会话列表：见
> [`CHAT_P2_SESSION_ANCHOR_DESIGN.md`](./CHAT_P2_SESSION_ANCHOR_DESIGN.md)——结论是
> **否决链上会话锚点**（会泄漏通信关系），改用链下加密会话索引 blob + inbox 推导。
- `total_direct_unread(who) -> u32`：链上 **System 通知**通道的未读总数，**不是** App 全局未读。

> **⚠️ 链上 / 链下边界（重要 — 客户端必读）**
>
> 本 API 返回的是**链上切片**，不能直接当成完整 IM 首页。原因：人类聊天（Text/Image/
> File/Voice，无论私聊还是群聊）全部走链下（MLS + relay，密文不触链）；链上唯一的消息是
> `System` 通知（订单/争议/治理，经 `send_system_message`）。因此：
>
> | 字段 | 私聊（Direct） | 群聊（Group） |
> | --- | --- | --- |
> | 是否出现在列表 | **仅当**该对用户间发过 `System` 消息才有行；纯链下私聊**无行** | 用户所在的群都在 |
> | `last_active` | **System 通知**会话的最后活跃区块（**非**人类聊天） | 恒 `0`（链下） |
> | `unread` | 仅 **System 通知**通道未读（**非**人类聊天） | 恒 `0`（链下） |
> | `pinned` | 链上权威（`set_session_pinned`） | 恒 `false`（群置顶为客户端能力，未上链） |
> | `muted` | 调用者自己的免打扰 **DND**（收不到提醒） | 管理员**禁言**（`is_member_muted`，你**不能发言**）|
> | `archived` | 链上权威 | 恒 `false` |
> | 群元数据（名/头像/角色/成员数）| N/A | 链上权威 |
>
> **关键提醒：**
> 1. `muted` 在两种 `kind` 下语义完全不同（DND vs 禁言），客户端**必须**按 `kind` 分支渲染，
>    不要共用一个「🔕 静音」图标。
> 2. `total_direct_unread` 与 `unread` 都只反映 System 通道，**不能**直接做 App 角标。
> 3. 群聊在列表末尾且 `last_active=0`，无活跃度排序，客户端**必须**跨类型重排。
>
> 渲染真实「消息」页必须执行下方 **客户端 Merge Spec**。

### 客户端 Merge Spec（链上切片 ⊕ 链下 MLS 状态）

前端/客户端拿到 `list_conversations` 后，需与本地链下状态（MLS 会话库 + relay 投递记录 +
本地偏好）合并。约定如下：

**1. 会话主键（去重/合并键）**
- 私聊：以**对端身份**为主键（`peer` AccountId，或客户端侧的成对 MLS session id）。
  注意：同一对用户的「链上 System 会话」与「链下人类 MLS 会话」是两条独立来源，
  客户端应按对端**合并为同一张会话卡片**（System 通知与人类消息混排在该卡片时间线内，或
  按产品需要分区展示）。
- 群聊：以 `group_id` 为主键。

**2. 会话集合（presence）= 链上 ∪ 链下**
- 起始集合 = 本地会话来源 ∪ 链上返回的行。本地会话来源 = **加密会话索引 blob**（首选）∪
  **inbox 投递推导**（兜底）∪ 本地 MLS 会话库——详见
  [`CHAT_P2_SESSION_ANCHOR_DESIGN.md`](./CHAT_P2_SESSION_ANCHOR_DESIGN.md)。
- 仅链下存在的私聊（无 System 消息）：链上无行（**有意为之**，链上私聊锚点已否决以隐藏通信
  关系），**以链下为准**补入。
- 仅链上存在的私聊（只有 System 通知）：作为「平台通知」卡片保留。

**3. 排序键 `recency`**
- 私聊：`recency = max(链上 last_active 折算时间, 链下最后一条消息时间)`。
- 群聊：`recency = 链下最后一条消息时间`（链上恒 0，不可用）。
- 置顶优先：`pinned = 链上 pinned(私聊) OR 本地置顶偏好(私聊/群)`，置顶组排在最前，
  组内再按 `recency` 倒序。

**4. 未读 `unread`**
- 私聊：`unread = 链下 MLS 未读 + (可选)链上 System 未读`（是否计入 System 由产品决定）。
- 群聊：`unread = 链下 MLS 未读`（链上恒 0）。
- App 全局角标 = Σ 各会话合并后的 `unread`，**不要**直接用 `total_direct_unread`。

**5. 静音 / 免打扰**
- 私聊提醒抑制：`链上 muted(DND) OR 本地 DND 偏好`。
- 群：`链上 muted` 表示**被管理员禁言（不能发言）**，与「我不想收提醒」是两件事；
  群的「免打扰」是本地偏好，需单独存储与渲染。

**6. 字段权威性速查**
- 链上权威：私聊 `pinned/muted(DND)/archived`、群 `name/avatar/role/member_count/muted(禁言)`。
- 链下权威：所有人类消息的内容、时间、未读、会话存在性、群活跃度、群置顶/免打扰偏好。

### 自定义 JSON-RPC（node 端）

除经 `state_call` / polkadot-js `api.call.*` 调用外，node 还把上述两个 Runtime API
封装为具名 JSON-RPC 方法（`node/src/chat_rpc.rs`，在 `create_full` 中挂载），便于非
polkadot-js 客户端用 JSON 友好类型直接调用。所有方法只读且免费。

| 方法 | 说明 |
| --- | --- |
| `chat_listConversations(who, at?)` | 链上会话切片（私聊 + 群聊）；非完整列表，需客户端 Merge（见上） |
| `chat_totalDirectUnread(who, at?)` | 链上 System 通道未读总数；**非** App 全局角标 |
| `chat_checkPermission(sender, receiver, at?)` | 聊天权限检查 |
| `chat_getActiveScenes(user1, user2, at?)` | 两用户间有效场景授权 |
| `chat_isFriend(user1, user2, at?)` | 是否好友 |
| `chat_listFriends(who, at?)` | 好友列表 |
| `chat_listIncomingFriendRequests(who, at?)` | 待处理的好友申请发起方 |
| `chat_listIncomingFriendRequestsDetailed(who, at?)` | 待处理好友申请（含附言/验证消息） |
| `chat_friendMeta(owner, friend, at?)` | 某账户对某好友的私有备注/分组 |
| `chat_isAccountMuted(who, at?)` | 账户是否被治理平台级禁言 |
| `chat_privacySummary(who, at?)` | 隐私设置摘要 |

> pallet 的 DTO 仅有 SCALE 编码（无 serde）；为保持 pallet 纯净，node 端定义本地
> serde 响应类型并从 Runtime API DTO 转换（如群名/头像/元数据以 UTF-8 有损转字符串，
> `SceneType` / `PermissionResult` 等映射为稳定字符串标签）。`at` 省略时取最佳区块。

## 设计原则

1. **低耦合**: `common` 不依赖任何 pallet；只承载真正共享的 `rate_limit` 与 `runtime_api`。
2. **单一事实来源**: 链上 `MessageType` 在 `core`；权限在 `permission`；不在 `common` 维护
   并行的类型/trait（审计 P1 已删除发散定义）。
3. **权限集中**: 统一的权限检查通过 `permission`（`ChatPermissionChecker`）。
4. **可扩展**: 新增聊天场景只需扩展 `SceneType` 枚举。

## 迁移历史

- 2025-12-29: 统一整合以下模块到 `pallets/chat/` 目录：
  - `pallets/chat` → `pallets/chat/core`
  - `pallets/chat-permission` → `pallets/chat/permission`
  - `pallets/smart-group-chat` → `pallets/chat/group`
  - `pallets/ai-chat` → `pallets/chat/ai`
  - 新建 `pallets/chat/common` 共享类型库
- 2026-06-03: P0 收口：
  - 删除合并前残留的顶层 `pallet-chat`（`pallets/chat/src/`），其私聊功能早已由
    `core/` 取代。
  - 移除未接入 runtime 的 AI 对话子模块（原 `pallets/chat/ai`，依赖
    `pallet-deceased` / `pallet-deceased-ai`）。如需 AI 对话能力，后续另行规划。
- 2026-06-03: P1 功能补齐：
  - `group`：群展示资料（群名/头像/公告）、群内昵称、封禁名单（链上强制）、禁言
    （单人 + 全员，应用层策略）。
  - `core`：消息撤回（限时、双向隐藏、未读回退）、会话级免打扰 + 置顶。
- 2026-06-03: P2 起步：新增统一会话视图 Runtime API（`pallet-chat-common::runtime_api::
  ChatViewApi`，聚合 core + group），并在 runtime `impl_runtime_apis!` 落地。
- 2026-06-03: P2 续：
  - 接线既有但未挂载的 `ChatPermissionApi`（权限检查 / 场景授权 / 好友 / 隐私摘要）到
    runtime `impl_runtime_apis!`。
  - 为 P1 新增 extrinsic 补 benchmark：`group`（`set_group_profile` / `set_group_nickname`
    / `ban_member` / `unban_member` / `set_member_mute` / `set_group_mute_all`）；`core`
    首次引入 benchmark 基础设施，覆盖 `recall_message` / `set_session_muted` /
    `set_session_pinned`。两者均已加入 runtime 基准清单（`benchmarks.rs`）。
  - node 端把 `ChatViewApi` / `ChatPermissionApi` 封装为自定义 JSON-RPC
    （`node/src/chat_rpc.rs`，`chat_*` 方法，见上表）。
- 2026-06-03: P2 社交完善 + 平台合规：
  - #9 防滥用：`group` 的写入型 MLS 操作（`commit` / `anchor_message_digest`）按账户
    窗口限频（`MlsActionWindow` / `MaxMlsActionsPerWindow`，复用
    `common::rate_limit`），约束 `HandshakeLog` / 锚点增长。
  - #7 社交完善（`permission`）：好友申请可带附言（验证消息，随申请生命周期清理）；
    好友**备注 + 分组**（私有、单向，解除好友时双向清理）；新增 extrinsic
    `set_friend_meta`；Runtime API/RPC 扩展（`*_detailed` / `friendMeta`）。
  - #8 平台合规：
    - `group` 治理闸门：`GovernanceOrigin` 可 `force_disband_group` /
      `set_group_frozen`；冻结群拒绝 `commit` / `anchor` / `request_join`，元数据仍可读。
    - `permission` 平台级禁言：治理 `force_mute_account` / `force_unmute_account`，
      被禁言账户作为**发送方**在 `check_permission` 直接拒（`DeniedSenderMuted`，
      经 `can_send_message` 联动私聊门控）；Runtime API/RPC `isAccountMuted`。
    - `permission` 举报/存证：`report`（账户/群/消息为对象，理由为 IPFS CID，链上无
      明文）按举报人冷却 + 全局 `MaxOpenReports` 上限；治理 `resolve_report` 关闭并移除。
- 2026-06-03: P2 续（基准/权重）：为本轮新增 extrinsic 补 benchmark + `WeightInfo`：
  - `group`：`force_disband_group` / `set_group_frozen`（治理来源基准，复用 `GovernanceOrigin`
    的 `try_successful_origin`）。
  - `permission`：首次引入 benchmark 基础设施（`weights.rs` + `benchmarking.rs` +
    `Config::WeightInfo`），覆盖全部 extrinsic（隐私/黑白名单/好友握手/`set_friend_meta`/
    平台禁言/举报）；已加入 runtime 基准清单（`benchmarks.rs`）。
  - 注：当前权重为占位值；生成实测权重需先修复与本模块无关的 `pallet-commission-pool-reward`
    等 pallet 的 `runtime-benchmarks` 编译问题（既有技术债，不在 chat 范围内）。
- 2026-06-03: P3 进阶能力（边界决策）：经评估，审计 P3 列出的「引用回复 / @提及 /
  reaction / 转发 / 阅后即焚 / 音视频信令」在本架构（人类消息迁出链，§13；群消息全链下）
  下**几乎全部属于链下职责**，链上**不新增 extrinsic / storage**。已落一份链下方案与
  链上边界设计：`CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`（MLS payload 信封约定、客户端/relay
  执行分层、链上明确不做清单、可选未来挂钩）。
- 2026-06-03: 设备端保留与清理策略草案：`CHAT_DEVICE_RETENTION_DESIGN.md`（客户端职责，
  链上不参与）。明确"链上 180 天元数据软过期 ≠ 设备保留"，定义本地数据模型、按时间/条数/容量
  的保留维度、热冷分层 + LRU 淘汰、按 CID 从 IPFS/relay 的恢复路径，以及与 MLS 前向保密/阅后
  即焚的安全交互。
- 2026-06-03: 大文件处理（边界决策）：聊天大文件（图片/视频/语音/附件）**本体不上链、
  不进 MLS payload**——每文件独立对称密钥加密 + 分块 + manifest 存 IPFS，MLS 仅传引用
  （`cid + file_key + 元数据`），持久化交给 `pallet-storage-service` 的 Tier 化多副本 Pin
  或链下托管。已落规范：`CHAT_LARGE_FILE_SPEC.md`（文件信封 / 分块 manifest / 缩略图 /
  计费分级 / 换机恢复衔接 / 链上不做清单）。
- 2026-06-03: 链上好友图谱**整体删除**，改链下能力令牌（隐私：隐藏好友列表 + 通信关系）。
  `pallet-chat-permission` 删 `Friendships`/好友申请/备注分组及对应 extrinsic/RPC/API，
  新增 `CapabilityEpoch` 撤销锚 + `bump_capability_epoch`；`pallet-chat-core` `send_message`
  收窄为仅 `System`（人类消息全链下，返回 `HumanMessagesOffChain`）；`pallet-chat-group`
  文档化「1:1 不建链上群」不变量。
- 2026-06-03: 链下投递准入规范：`CHAT_OFFCHAIN_DELIVERY_DESIGN.md`（**盲化一次性投递令牌**）。
  基线 RFC 9474 Blind RSA：接收方盲签令牌、relay 公钥离线验签 + per-inbox spent set；
  **per-contact 标签**定向撤销 + epoch 整批撤销；一次性务实降级为「Bob 侧精确一次 + relay 侧
  至多 k 份」；含 inbox 注册表字段、relay 验证伪码、匿名性/陷阱分析、BBS+ 升级路径。
- 2026-06-03: 新增 **`pallet-chat-inbox`**（`pallets/chat/inbox`，runtime index 78）——上文规范的
  链上锚点 v1。`inbox_id → {controller, epoch(inbox 维度), revoked_tags, deposit}`；IPK 下链
  （`inbox_id = H(IPK)`，链不做 RSA）；epoch **不复用** 账户级 `CapabilityEpoch`（避免 relay 把
  inbox 链回账户）；v1 用签名 controller + 押金反垃圾（不可关联性靠一次性 controller）。暴露
  `ChatInboxApi` + RPC `chat_inboxEpoch / chat_isTagRevoked / chat_inboxExists`。relay 程序、客户端
  盲签/兑付、RFC 9474 实现不在本仓，属链下组件。
- 2026-06-03: 复审三项修复（B1/B2/P1）：
  - **B1（未读漂移）**：`pallet-chat-core::delete_message` 接收方删除「仍计未读」的消息时同步
    抵消 `UnreadCount`（幂等：仅未读 + 未撤回 + 此前未删除时抵消），修复角标永久 +1。
  - **B2（System 来源限制）**：`send_message` / `send_system_message` 改用新配置
    `SystemMessageOrigin: EnsureOrigin<…, Success = AccountId>` 取代 `ensure_signed`，杜绝任意用户
    伪造 `System` 系统通知。runtime 收敛为 `EnsureRootWithSuccess<AccountId, ChatSystemMessenger>`
    （治理签发，sender = PalletId 派生系统账户）；mock 用 `EnsureSigned` 保持单测语义。
  - **P1（黑/白名单去明文，隐私）**：`pallet-chat-permission` **删除链上 `block_list` / `whitelist`**
    明文存储及 `block_user`/`unblock_user`/`add_to_whitelist`/`remove_from_whitelist`（call_index
    2/3/6/7 留空）+ 对应事件 / 错误 / 权重 / 基准；`check_permission` 去除黑/白名单分支，`Whitelist`
    级别等同 `FriendsOnly`；摘要去除 `block_list_count`/`whitelist_count`。拉黑 / 放行统一走链下
    能力令牌（`bump_capability_epoch`）+ 信箱标签撤销（`pallet-chat-inbox::revoke_tag`）。
- 2026-06-03: 复审收尾（B3/B4/U2/U3 复核 + P2/P3 边界文档化）：
  - **B3（已落地复核）**：`mark_session_as_read` 已为有界扫描（单次 `MAX_SESSION_READ_SCAN=512`，
    按本次实际标记数递减未读，分批安全）；`mark_batch_as_read` 权重随 `message_ids.len()` 计费。
  - **B4（已落地复核）**：`do_disband` 已改有界拆除（单次 `MAX_DISBAND_ITEMS_PER_CALL`，进入即
    `GroupFrozen` 冻结、cursor 判定完成、未完成发 `GroupDisbandProgress`、全清后才退押金移除群根）。
  - **U3（已落地复核）**：`commit` 公开群要求被加成员**已发布 KeyPackage**（链上同意信号 + MLS 正确性），
    私群需管理员批准，杜绝被动拉入。
  - **U2（决策文档化）**：场景授权**有意**覆盖 `Closed`——存在场景授权即双方处于活跃交易上下文
    （订单/争议/做市），对方必须能就该业务联系；接收方经 `rejected_scene_types` 按场景控制。非泄漏。
  - **P2（固有权衡 + 防呆）**：场景授权对 `(user1,user2)+scene` 镜像的是来源 pallet 本就公开的业务关系
    （悬赏 poster/solver、群成员均在链上）；`SceneAuthorization.metadata` 明文上链，已加隐私警告并约束
    调用方仅传空/不透明引用（现生产调用方均传空）。
  - **P3（固有权衡）**：多人群成员（`GroupMembers`/`UserGroups`）明文上链是 DS+AS 角色固有属性，由
    「1:1 不建链上群」不变量收口；消息内容仍链下 E2EE。已在 group lib 头与 README 文档化。
- 2026-06-03: 复审第二批修复（B3/B4/U2/U3）：
  - **B3（权重/DoS）**：`pallet-chat-core::mark_session_as_read` 原对会话消息「无界迭代 + 固定权重
    100」，会被低估。改为单次最多扫描 `MAX_SESSION_READ_SCAN`（512）条、权重按该上限计；未读按
    「本次实际标记数」递减（分批收敛、不再误清零），会话过大时客户端可重复调用。
  - **B4（权重/DoS）**：`pallet-chat-group::do_disband` 原用 `clear_prefix(u32::MAX)` 配固定权重清理
    随群生命周期无界增长的 `HandshakeLog` / `MessageDigestAnchor` / `Banned` 等前缀。改为**有界拆除**：
    进入即冻结群（阻止 `commit` 等拆除期间继续写入），单次每前缀最多移除 `MAX_DISBAND_ITEMS_PER_CALL`
    （128）项，全部清空才移除 `GroupMls` 并退押金；未清完发 `GroupDisbandProgress`，调用方重复
    `disband_group` / `force_disband_group` 直至完成。权重按预算计量。小群（含全部单测）一次完成。
  - **U2（语义澄清，非行为变更）**：明确 `check_permission` 中**场景授权有意覆盖 `Closed`**——存在订单 /
    争议 / 做市等活跃交易上下文时对方必须可就业务联系到接收方；接收方仍可用 `rejected_scene_types`
    按场景拒绝（拒绝后正常套用 `Closed`/`FriendsOnly`）。已在 doc 注释中写明（EN+CN）。
  - **U3（滥用向量）**：`pallet-chat-group::commit` 向**公开群**加人时，要求被加成员已发布至少一个
    KeyPackage（`KeyPackageCount > 0`，新增 `Error::AddeeNotJoinable`）——既是「同意被加入」的链上信号
    （成员主动发布、可吊销退出），也符合 MLS（无对方 KeyPackage 本就无法 Add）。私群仍走 request/approve
    同意流程，行为不变。
- 2026-06-03: 待定隐私决策（P2/P3）——见 `permission` / `group` 模块说明与本批讨论，涉及
  `SceneAuthorizations` 明文配对 与 `GroupMembers`/`UserGroups` 明文成员关系，属架构级取舍，待定方案后再改。
- 2026-06-04: 审计 P2（会话列表体验，隐私优先）：
  - **Item 1（否决链上锚点）**：明确**不**为链下私聊新增链上会话锚点（`touch_session` 等）——
    会泄漏「谁↔谁」通信关系，违反「1:1 不建链上群」不变量。换设备/重装的会话恢复改走**链下**：
    加密会话索引 blob（首选）+ inbox 投递推导（兜底）。决策与方案见
    `CHAT_P2_SESSION_ANCHOR_DESIGN.md`。链上零新增。
  - **Item 2（群排序确定性）**：`runtime/src/apis.rs` 的 `list_conversations` 群聊部分改为按
    `group_id` 升序输出，给客户端一个确定、可分页的基线（真实活跃度仍由客户端用链下
    `last_active` 跨类型重排）。仅改 runtime 聚合层，无 pallet / storage / weight 改动。
- 2026-06-04: `common` 审计 P1（类型收敛 + 死代码清理，无链上行为变更）：
  - **删除** `common` 中零调用方的 `types.rs`（`MessageType`/`MessageStatus`/`EncryptionMode`/
    重复 `ChatUserId`）、`traits.rs`（`ChatPermissionCheck`/`FriendshipCheck`/`ChatUserIdProvider`/
    `IpfsContentValidator`/`GroupMemberCheck`/`RateLimitCheck`）、`validation.rs`（可绕过的 CID
    「加密」启发式 + 误导示例）。`common` 现仅余 `rate_limit` + `runtime_api`。
  - 消除 `MessageType` 编码地雷：链上权威 `MessageType` 唯一来源为 `pallet-chat-core`（未改其
    链上枚举，无存储迁移）；common 不再维护并行定义。
  - **移除** `pallet-chat-core` / `pallet-chat-permission` 对 `pallet-chat-common` 的 dead 依赖
    （deps + `std` feature；二者源码本就零 import）。清理 `common/Cargo.toml` 未用的
    `frame-support` 与 dev-deps。
  - 同步重写 `common/README.md`、更新 chat/permission/group README 中对 common 的描述与依赖图。
- 2026-06-04: `common` 审计 P0（文档/契约修正，无链上行为变更）：纠正 `ChatViewApi` 的
  「链上 / 链下边界」误述——私聊 `unread`/`last_active` 实际只反映 **System 通知通道**（人类
  私聊全链下，纯链下聊天不产生链上私聊行），`total_direct_unread` **非** App 全局角标；明确
  `muted` 在 `kind=direct`（免打扰 DND）与 `kind=group`（管理员禁言/不能发言）下语义不同，
  客户端必须按 `kind` 分支；新增**客户端 Merge Spec**（会话主键/presence/排序/未读/静音/
  字段权威性）。同步修订 `runtime_api.rs`、`node/src/chat_rpc.rs` 文档注释（EN+CN）。
