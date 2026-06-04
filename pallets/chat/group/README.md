# Pallet Chat Group

基于 MLS（RFC 9420）的端到端加密群聊。

## 概述

**消息密文走链下（MLS + 节点中继），不上链。** 链上只承载群的 MLS 状态、握手（commit）
日志、成员与角色、审计锚点，以及治理 / 防滥用元数据：

- **群生命周期**：建群（押金 + 冷却）、转让群主、解散
- **成员管理**：KeyPackage 发布、`commit` 加入/退出/移除、入群申请—批准、封禁（链上强制）
- **应用层治理**：群主 / 管理员角色、群展示资料、群名片、禁言（单人 / 全员）
- **审计**：消息摘要锚定（只锚 hash，不锚明文）

## 加密 / Cipher suite

群创建时指定 MLS `cipher_suite: u16`（RFC 9420 命名的密码学套件 ID），决定签名 / HPKE / AEAD
组合。链上仅记录套件 ID 与 MLS 状态摘要（`tree_hash` / `confirmed_transcript_hash` /
`group_info_cid`），**不持有任何密钥或明文**。

> 早期"四种加密模式（Military/Business/Selective/Transparent）"为 MLS 收敛前的 legacy 概念，
> 群模块已不再使用，统一由 MLS cipher suite 表达。

## 隐私：群成员公开（审计 P3，固有权衡）

多人群的成员关系（`GroupMembers` / `UserGroups`）与角色**以明文存于链上**。这是 DS+AS 角色的
**固有**属性：链必须知道成员集合才能为 Commit 全局定序、强制 `MaxGroupMembers`、路由 Welcome、
防分叉。属**可接受**权衡，并由「1:1 私聊不建链上群」不变量收口（最敏感的「谁私聊谁」永不成为
链上群）。即便是群，消息**内容**仍私密（链下 MLS 端到端加密，密文不触链）。

- 产品建议：仅当成员公开可见可接受时才用链上群；关系敏感群体优先走链下成对路径。
- 真要隐藏多人成员关系需另一种原语（如匿名凭证群），不在本锚范围内。
- 被动拉入防护（审计 U3）：公开群只能加入**已发布 KeyPackage** 的账户（即用户主动的「同意被加入」
  信号，可吊销退出）；私群需先获管理员批准。

## 核心功能

### 建群

```rust
Chat::create_group(
    origin,
    init_group_info_cid, // 初始 GroupInfo 的 IPFS CID
    cipher_suite,        // MLS 密码学套件 ID (u16)
    is_public,           // 是否公开群
    tree_hash,           // 初始 ratchet tree 哈希 [u8; 32]
    transcript_hash,     // 初始 confirmed transcript 哈希 [u8; 32]
)?;
```

发起者成为 `Owner`，预留建群押金（`GroupDeposit`），受建群冷却（`GroupCreationCooldown`）
与每用户群上限（`MaxGroupsPerUser`）约束。

### 消息（链下）

群消息**不经任何链上 extrinsic**——不存在 `send_group_message`。客户端把消息封装为 MLS
application message 加密后由节点中继投递。如需审计留痕，群主/管理员可用
`anchor_message_digest` 锚定密文摘要（只锚 hash，不锚明文）。

### 成员变更（commit）

成员加入 / 退出 / 移除通过 `commit` 提交 MLS Commit 并推进 epoch；`expected_epoch` 闸门借
区块全序仲裁并发 commit（唯一 committer）。新成员经 `WelcomeMailbox` 领取 Welcome 后入群。

## 群角色与权限矩阵

群成员分三种应用层角色（MLS 协议本身是扁平的，角色是其上的应用层叠加）：
`Owner`（群主）/ `Admin`（管理员）/ `Member`（普通成员）。`create_group` 的发起者成为 `Owner`。

> 前端可据此矩阵对按钮做按角色置灰。校验入口：群主项校验 `g.admin == who`；
> 群主/管理员项走 `ensure_owner_or_admin`；成员项仅校验在群。

| 能力 | extrinsic | Owner | Admin | Member |
|---|---|:--:|:--:|:--:|
| 成员变更（加人/踢人） | `commit` | ✓ | ✓ | 仅自己退群 |
| 批准入群申请 | `approve_join` | ✓ | ✓ | ✗ |
| 设置群资料（群名/头像/公告） | `set_group_profile` | ✓ | ✓ | ✗ |
| 封禁 / 解封（链上强制） | `ban_member` / `unban_member` | ✓ | ✓ | ✗ |
| 单人禁言 / 解除 | `set_member_mute` | ✓ | ✓ | ✗ |
| 全员禁言开关 | `set_group_mute_all` | ✓ | ✓ | ✗ |
| 消息摘要锚定（审计） | `anchor_message_digest` | ✓ | ✓ | ✗ |
| 设/撤管理员 | `set_admin` | ✓ | ✗ | ✗ |
| 转让群主 | `transfer_ownership` | ✓ | ✗ | ✗ |
| 解散群 | `disband_group` | ✓ | ✗ | ✗ |
| 设置自己的群名片 | `set_group_nickname` | ✓ | ✓ | ✓ |
| 申请入群 / 取消申请 | `request_join` / `cancel_join_request` | — | — | ✓（非成员）|
| 发布 / 撤销 KeyPackage | `publish_key_package` / `revoke_key_package` | ✓ | ✓ | ✓ |

### 重要边界（易误解）

- **封禁 ≠ 移出**：`ban_member` 只阻止"申请入群 / 被 `commit` 加入"，**不会**自动把现有成员
  移出 MLS——管理员仍需另发一次 `commit` 移除该成员；封禁仅阻止其回流。
- **禁言为链下执行**：单人/全员禁言把状态写上链，但消息本身离链（MLS），实际"禁言"由
  客户端 / 中继节点读取该状态执行。
- **群主受保护**：群主不能被封禁、禁言或移除；群主退群前必须先 `transfer_ownership`。
- **不能针对自己**：封禁 / 禁言均禁止 target 自己。

## 防滥用与治理（P2）

- **写入型 MLS 操作限频**：`commit` / `anchor_message_digest` 按账户窗口限频
  （`MlsActionWindow` / `MaxMlsActionsPerWindow`，复用 `pallet-chat-common::rate_limit`），
  约束 `HandshakeLog` / `MessageDigestAnchor` 增长；超限报 `RateLimited`。
- **治理冻结 / 强制解散**：`GovernanceOrigin`（Root / 治理）可：
  - `set_group_frozen(group_id, frozen)`：冻结群拒绝 `commit` / `anchor_message_digest` /
    `request_join`（报 `GroupFrozen`），元数据仍可读，便于客户端展示"已冻结"。
  - `force_disband_group(group_id)`：无视群内归属强制解散并退还押金（同 `disband_group` 清理）。

## 存储结构

**MLS 与成员**

- `KeyPackages` / `NextKeyPackageId` / `KeyPackageCount`：成员发布的 MLS KeyPackage 及计数
- `GroupMls`：群 MLS 状态（`epoch` / `tree_hash` / `confirmed_transcript_hash` /
  `group_info_cid` / `admin` / `member_count` / `cipher_suite` / `is_public`）
- `NextGroupId`：群 ID 自增计数
- `HandshakeLog`：握手（commit）日志
- `WelcomeMailbox`：Welcome 信箱（新成员领取）
- `GroupMembers`：成员（角色 / 加入 epoch / 加入区块）
- `UserGroups`：用户所属群列表
- `LastGroupCreation`：建群冷却时间戳

**入群与审计**

- `JoinRequests` / `PendingJoinCount`：入群申请及计数
- `JoinApprovals`：入群批准（由后续 `commit` 消费）
- `MessageDigestAnchor`：消息摘要审计锚点

**治理 / 防滥用 / 押金**

- `GroupDepositOf`：建群押金（解散时退还）
- `MlsActionRate`：写入型 MLS 操作限频状态
- `GroupFrozen`：治理冻结标记

**展示资料与管理**

- `GroupProfiles`：群展示资料（群名 / 头像 CID / 公告）
- `GroupNicknames`：群内昵称（群名片）
- `Banned`：封禁名单（链上强制）
- `MemberMutedUntil`：单人禁言截止区块
- `GroupMutedAll`：全员禁言开关

## 依赖

- `pallet-chat-common`：仅用其 `rate_limit`（窗口化限频；`MemberRole` 在本 crate 内定义，
  角色标签的稳定 `u8` 约定见 `pallet-chat-common::runtime_api::role`）
- `frame-support` 的 `Currency`：建群押金的预留 / 退还（mock/测试用 `pallet-balances`）
- `frame-benchmarking`（可选）：`runtime-benchmarks` 下启用

> 群模块**不**直接依赖 `pallet-chat-permission`；权限/好友判定属私聊（core）域，群以
> MLS 成员关系 + 应用层角色自洽。
