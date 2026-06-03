# Stardust 聊天系统模块

统一的聊天系统模块集合，提供完整的即时通讯（私聊 + MLS 群聊 + 权限）功能。

## 模块概览

```
pallets/chat/
├── common/       # 共享类型和工具库              [已接入]
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
                    │   common    │  ← 共享类型（无pallet依赖）
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
       ┌────────────┐ ┌─────────┐ ┌─────────────┐
       │ permission │ │  core   │ │    group    │
       └────────────┘ └─────────┘ └─────────────┘
```

## 各模块功能

### common - 共享类型库

提供聊天系统的基础类型和工具：

- **MessageType**: 消息类型（文本/图片/文件/语音/视频/系统/AI）
- **EncryptionMode**: 加密模式（Military/Business/Selective/Transparent）
- **ChatUserId**: 11位数字聊天ID（10000000000-99999999999）
- **Traits**: ChatPermissionCheck, FriendshipCheck, ChatUserIdProvider
- **Validation**: CID格式验证、加密CID验证
- **RateLimit**: 防刷机制

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
```

在 `std` feature 中添加：

```toml
"pallet-chat-common/std",
"pallet-chat-permission/std",
"pallet-chat-core/std",
"pallet-chat-group/std",
```

## Runtime API：统一会话视图

`pallet-chat-common::runtime_api::ChatViewApi` 把**私聊（core）+ 群聊（group）**聚合为
单一会话列表，供前端"消息"页直接渲染。该 trait 定义在 `common`（core / group 互不依赖），
聚合逻辑在 `runtime/src/apis.rs` 的 `impl_runtime_apis!` 中实现（那里可同时访问
`ChatCore` 与 `ChatGroup`）。前端经 `api.call.chatViewApi.*` 免费调用，无需 gas。

- `list_conversations(who) -> Vec<ConversationSummary>`：私聊在前（core 已按"置顶优先 +
  最后活跃倒序"排序），群聊在后。
- `total_direct_unread(who) -> u32`：全部私聊会话的未读总数（链上权威）。

> **链上 / 链下边界（重要）**
> 私聊（1:1）的未读数、最后活跃区块、置顶、免打扰均在链上，故 `ConversationSummary`
> 对应字段权威可信。**群聊消息走链下（MLS + 节点中继，密文不触链）**，因此群的 `unread`
> 与 `last_active` 链上无从得知，一律返回 `0`，需由客户端用本地/链下状态合并排序；链上
> 权威的仅为群元数据（群名 / 头像 / 角色 / 成员数 / 管理员禁言）。群级置顶/免打扰目前为
> 客户端侧能力，未上链。

### 自定义 JSON-RPC（node 端）

除经 `state_call` / polkadot-js `api.call.*` 调用外，node 还把上述两个 Runtime API
封装为具名 JSON-RPC 方法（`node/src/chat_rpc.rs`，在 `create_full` 中挂载），便于非
polkadot-js 客户端用 JSON 友好类型直接调用。所有方法只读且免费。

| 方法 | 说明 |
| --- | --- |
| `chat_listConversations(who, at?)` | 统一会话列表（私聊 + 群聊） |
| `chat_totalDirectUnread(who, at?)` | 私聊未读总数 |
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

1. **低耦合**: common模块不依赖任何pallet，各子模块通过traits交互
2. **类型统一**: 所有消息类型、加密模式在common中定义
3. **权限集中**: 统一的权限检查通过permission模块
4. **可扩展**: 新增聊天场景只需扩展SceneType枚举

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
- 2026-06-03: 大文件处理（边界决策）：聊天大文件（图片/视频/语音/附件）**本体不上链、
  不进 MLS payload**——每文件独立对称密钥加密 + 分块 + manifest 存 IPFS，MLS 仅传引用
  （`cid + file_key + 元数据`），持久化交给 `pallet-storage-service` 的 Tier 化多副本 Pin
  或链下托管。已落规范：`CHAT_LARGE_FILE_SPEC.md`（文件信封 / 分块 manifest / 缩略图 /
  计费分级 / 换机恢复衔接 / 链上不做清单）。
