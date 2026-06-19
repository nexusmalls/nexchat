# Pallet Chat Group

基于 MLS（RFC 9420）的端到端加密群聊 pallet。已在 runtime 注册为 `ChatGroup`。

**消息密文走链下（MLS + 节点中继），不上链。** 链上只承载群的 MLS 状态锚、握手（Commit）
日志、成员与角色、审计锚点，以及治理 / 防滥用元数据。设计文档见
[`CHAT_GROUP_WIREIFY_DESIGN.md`](../../CHAT_GROUP_WIREIFY_DESIGN.md)。

> ⚠️ 本文档随代码维护。早期「四种加密模式（Military/Business/Selective/Transparent）」
> 为 MLS 收敛前 legacy 概念，已废弃；统一由 MLS `cipher_suite: u16` 表达。

## 1. 定位与边界

| 层 | 职责 |
| --- | --- |
| **链上（DS + AS）** | KeyPackage 发布、Commit 全局定序、Welcome 短期信箱、成员表、群元数据、可选 digest 锚 |
| **链下** | 群消息密文（MLS application message + relay）；链**不**解密、**不**存储明文 |
| **1:1 不变量** | **禁止恰好 2 人的链上群**（`TwoMemberGroupForbidden`）；合法人数为 1（仅创建者）或 3+；1:1 走链下 |

统一会话视图中群聊行由 `ChatViewApi` 聚合；群 `unread`/`last_active` 恒为 0，客户端须与链下
MLS 合并，见 `pallets/chat/README.md` Merge Spec。

## 2. 加密 / Cipher suite

群创建时指定 MLS `cipher_suite: u16`（RFC 9420 套件 ID）。链上仅记录套件 ID 与 MLS 状态摘要
（`tree_hash` / `confirmed_transcript_hash` / `group_info_cid`），**不持有任何密钥或明文**。

## 3. 隐私：群成员公开（审计 P3，固有权衡）

多人群的成员关系（`GroupMembers` / `UserGroups`）与角色**以明文存于链上**——DS+AS 的固有属性：
链必须知道成员集合才能为 Commit 全局定序、强制 `MaxGroupMembers`、路由 Welcome、防分叉。
由「1:1 不建链上群」不变量收口；**消息内容**仍链下 E2EE。

- 被动拉入防护（审计 U3）：公开/私群 Add 均要求被加者 `KeyPackageCount > 0`（opt-in + MLS 必需）。
- 私群另需 `approve_join` 后的 `JoinApprovals` 记录，由后续 Add `commit` 消费。

## 4. Extrinsic 一览

| call_index | extrinsic | 说明 |
| --- | --- | --- |
| 0 | `publish_key_package` | 发布 KeyPackage（预留 `KeyPackageDeposit` 押金） |
| 1 | `revoke_key_package` | 吊销 KeyPackage（退还押金） |
| 2 | `create_group` | 建群（预留 `GroupDeposit`；冷却 + 每用户群上限） |
| 3 | `commit` | MLS Commit：成员变更 + epoch 推进；权重随 delta 规模计费 |
| 4 | `claim_welcome` | 领取并删除 Welcome（**先**经 Runtime API 读取，再 claim） |
| 5 | `disband_group` | 群主解散（有界拆除，审计 B4） |
| 6 | `anchor_message_digest` | 可选强审计 digest 锚（无 CID/明文） |
| 7 | `request_join` | 私群入群申请（公开群无需） |
| 8 | `cancel_join_request` | 撤回申请 |
| 9 | `approve_join` | 批准申请（由后续 Add `commit` 消费） |
| 10 | `transfer_ownership` | 转让群主（重绑 `ChatHook` 场景授权） |
| 11 | `set_admin` | 设/撤管理员 |
| 12 | `set_group_profile` | 群名 / 头像 CID / 公告 |
| 13 | `set_group_nickname` | 自己的群名片 |
| 14 | `ban_member` | 封禁（不自动移出 MLS，阻止回流） |
| 15 | `unban_member` | 解封 |
| 16 | `set_member_mute` | 单人禁言至区块 / 解除 |
| 17 | `set_group_mute_all` | 全员禁言开关 |
| 18 | `force_disband_group` | 治理强制解散 |
| 19 | `set_group_frozen` | 治理冻结 / 解冻 |

> 群消息**无** `send_group_message` extrinsic——人类群聊全链下。

### P0 链上约束（commit）

- `welcomes` 与 `member_delta.added` **双射一致**（每名新成员恰一条非空 Welcome）。
- `expected_epoch` 闸门防分叉；并发 commit 落败报 `EpochStale`。
- 校验**全部通过后才** `note_mls_action`——失败 commit **不**消耗限频配额。
- 群主转让时 `ChatHook` 将成员↔群主场景授权从旧群主迁移到新群主。

## 5. 群角色与权限矩阵

`Owner` / `Admin` / `Member`（MLS 之上的应用层角色）。校验：群主项 `g.admin == who`；
群主/管理员项 `ensure_owner_or_admin`；成员项校验在群。

| 能力 | extrinsic | Owner | Admin | Member |
|---|---|:--:|:--:|:--:|
| 成员变更（加人/踢人） | `commit` | ✓ | ✓ | 仅自己退群 |
| 批准入群申请 | `approve_join` | ✓ | ✓ | ✗ |
| 设置群资料 | `set_group_profile` | ✓ | ✓ | ✗ |
| 封禁 / 解封 | `ban_member` / `unban_member` | ✓ | ✓ | ✗ |
| 单人禁言 / 解除 | `set_member_mute` | ✓ | ✓ | ✗ |
| 全员禁言 | `set_group_mute_all` | ✓ | ✓ | ✗ |
| 消息 digest 锚定 | `anchor_message_digest` | ✓ | ✓ | ✗ |
| 设/撤管理员 | `set_admin` | ✓ | ✗ | ✗ |
| 转让群主 | `transfer_ownership` | ✓ | ✗ | ✗ |
| 解散群 | `disband_group` | ✓ | ✗ | ✗ |
| 自己的群名片 | `set_group_nickname` | ✓ | ✓ | ✓ |
| 申请入群 / 取消 | `request_join` / `cancel_join_request` | — | — | ✓（非成员）|
| 发布 / 撤销 KeyPackage | `publish_key_package` / `revoke_key_package` | ✓ | ✓ | ✓ |

**易误解边界：**

- **封禁 ≠ 移出**：`ban_member` 阻止申请/被加入，不移出现有 MLS 成员。
- **禁言为链下执行**：链上写状态，客户端/relay 读取 `is_member_muted` / `GroupMutedAll` 执行。
- **群主受保护**：不能被封禁、禁言或移除；退群前须 `transfer_ownership`。
- **`muted` 语义**：本 pallet 禁言 = **不能发言**；与私聊 DND（不收提醒）不同——`ChatViewApi` 按 `kind` 分支。

## 6. 防滥用与治理

| 机制 | 说明 |
| --- | --- |
| **建群押金** | `GroupDeposit`（runtime：100_000 NEX），解散退还 |
| **KeyPackage 押金** | `KeyPackageDeposit`（runtime：0.1 NEX），吊销退还 |
| **建群冷却** | `GroupCreationCooldown`（runtime：10 分钟） |
| **MLS 写入限频** | `commit` / `anchor_message_digest` → `MlsActionRate`（runtime：30 次/分钟/账户） |
| **入群申请限频** | `request_join` → `JoinRequestRate`（runtime：20 次/小时/账户，跨群合计） |
| **平台禁言** | `PlatformMuteCheck` → `ChatPermission::is_account_muted`；写入路径报 `SenderPlatformMuted` |
| **治理冻结** | `set_group_frozen`：拒绝 `commit` / `anchor` / `request_join`（`GroupFrozen`） |
| **治理强拆** | `force_disband_group`：同群主解散清理流程 |
| **有界拆除（B4）** | `do_disband` 单次每前缀最多 `MAX_DISBAND_ITEMS_PER_CALL=128`；大群重复调用；进行中自动冻结 |

## 7. Runtime API 与 RPC

`runtime_api::ChatGroupApi`（只读、免费）：

| 方法 | 说明 |
| --- | --- |
| `pending_welcome(group_id, who)` | 待领 Welcome 字节——**在 `claim_welcome` 之前**读取 |
| `handshake_at_epoch(group_id, epoch)` | 指定 epoch 的 Commit 字节（离线补齐） |
| `group_mls_snapshot(group_id)` | MLS 锚点快照（含 `frozen`） |
| `group_exists` / `is_group_frozen` | 群存在性 / 冻结状态 |

Node JSON-RPC（`node/src/chat_rpc.rs`）：`chat_pendingWelcome`、`chat_handshakeAtEpoch`、
`chat_groupMlsSnapshot`、`chat_isGroupFrozen`。

## 8. 存储结构

**MLS 与成员**

- `KeyPackages` / `NextKeyPackageId` / `KeyPackageCount`
- `GroupMls` / `NextGroupId`
- `HandshakeLog` / `WelcomeMailbox`
- `GroupMembers` / `UserGroups` / `LastGroupCreation`

**入群与审计**

- `JoinRequests` / `PendingJoinCount` / `JoinApprovals`
- `MessageDigestAnchor`

**治理 / 防滥用 / 押金**

- `GroupDepositOf` / `MlsActionRate` / `JoinRequestRate` / `GroupFrozen`

**展示与管理**

- `GroupProfiles` / `GroupNicknames` / `Banned` / `MemberMutedUntil` / `GroupMutedAll`

## 9. 配置（`Config`）— runtime 当前值

| 项 | runtime 值 |
| --- | --- |
| `GroupDeposit` | 100_000 NEX |
| `KeyPackageDeposit` | 0.1 NEX |
| `MaxGroupMembers` | 500 |
| `MaxGroupsPerUser` | 500 |
| `MaxPendingJoins` | 256 |
| `MaxKeyPackagesPerUser` | 16 |
| `GroupCreationCooldown` | 10 分钟 |
| `MlsActionWindow` / `MaxMlsActionsPerWindow` | 1 分钟 / 30 |
| `JoinRequestWindow` / `MaxJoinRequestsPerWindow` | 60 分钟 / 20 |
| `ChatHook` | `GroupChatAuthorizer`（成员↔群主场景授权，O(1)） |
| `PlatformMuteCheck` | `GroupPlatformMuteCheck` |
| `GovernanceOrigin` | Root 或技术委员会多数 |

字节上限：`MaxKeyPackageLen=4096`、`MaxHandshakeLen=16384`、`MaxWelcomeLen=8192`、
`MaxCidLen=96`、`MaxGroupNameLen=128`、`MaxGroupAnnouncementLen=2048`、`MaxGroupNicknameLen=64`。

## 10. 依赖关系

```
pallet-chat-common  ←── pallet-chat-group（rate_limit + deposit 薄封装）
         ↑
runtime ChatHook ──→ pallet-chat-permission（场景授权，成员↔群主）
runtime PlatformMuteCheck ──→ pallet-chat-permission（平台禁言）
```

群模块**不**直接依赖 `pallet-chat-permission` crate；通过 `ChatHook` / `PlatformMuteChecker`
trait 在 runtime 接线。`MemberRole` 在本 crate 定义；稳定 `u8` 角色标签见
`pallet-chat-common::runtime_api::role`（供 `ChatViewApi` DTO）。

## 11. 权重与基准

- 全 extrinsic 有 `WeightInfo` + `benchmarking.rs`；权重在 dev 链实测（`src/weights.rs`）。
- `commit(a, r)` 按增/删成员数线性计费。
- `disband_group` / `force_disband_group` 按 `MAX_DISBAND_ITEMS_PER_CALL` 预算公式计量
  （基准仅种子小群，未覆盖最坏拆除）。主网前应在参考硬件重跑 `runtime-benchmarks`。

## 12. 上线审计摘要（2026-06-19）

| 维度 | 结论 |
| --- | --- |
| **链下消息** | ✅ 无群消息上链 extrinsic；密文离链 MLS + relay |
| **1:1 不变量** | ✅ `TwoMemberGroupForbidden`；单测覆盖 add/leave 路径 |
| **防分叉** | ✅ `expected_epoch` + `EpochStale` |
| **U3 opt-in** | ✅ Add 要求 `KeyPackageCount > 0`（公开/私群） |
| **Welcome 一致性** | ✅ `WelcomeMismatch` 双射校验 |
| **B4 有界拆除** | ✅ `MAX_DISBAND_ITEMS_PER_CALL=128` + 拆除期冻结 |
| **限频** | ✅ MLS 写入 + 入群申请双轨；失败 commit 不烧配额 |
| **平台合规** | ✅ 治理冻结/强拆 + 平台禁言闸门 |
| **权限镜像** | ✅ `ChatHook` 接线 `GroupChatAuthorizer`；转让重绑 |
| **Runtime 接线** | ✅ `ChatGroup` + `ChatGroupApi` + node RPC |
| **单测** | ✅ 60 项通过（`cargo test -p pallet-chat-group`） |
| **缺口（非阻塞）** | ⚪ 主网前重跑 benchmark；⚪ 禁言/封禁依赖客户端/relay 执行（设计既定） |

**总评：达到上线标准。** MLS DS+AS 锚、隐私不变量、防滥用与治理闸门均已落地并有测试；
群成员明文与链下禁言执行为已文档化的固有权衡。
