# Pallet Chat Permission

聊天权限系统模块 - 基于场景的多场景共存权限控制

## 概述

本模块提供聊天权限管理功能：

- **场景授权**：业务模块可为用户授予基于场景的聊天权限
- **好友关系（通讯录后端）**：用户间经「申请 → 同意」双方握手建立双向好友关系
- **黑白名单**：用户可屏蔽特定用户或设置白名单
- **权限级别**：Open/FriendsOnly/Whitelist/Closed

## 核心功能

### 场景授权

业务模块通过 `SceneAuthorizationManager` trait 管理场景授权：

```rust
T::ChatPermission::grant_bidirectional_scene_authorization(
    *b"otc_ordr",
    &buyer,
    &seller,
    SceneType::Order,
    SceneId::Numeric(order_id),
    Some(30 * 24 * 60 * 10), // 30天
    "订单#123".as_bytes().to_vec(),
)?;
```

### 权限检查优先级

1. **黑名单检查**（最高优先级拒绝）
2. **好友关系检查**
3. **场景授权检查**
4. **隐私设置检查**

### 好友关系 / 通讯录（申请 → 同意握手）

好友图谱是消息权限闸门（`FriendsOnly`）的输入，因此**关系的建立必须经双方同意**，
不能由任意一方单方面写入。流程：

```text
ALICE ──request_friend(BOB, msg?)─▶ FriendRequests[BOB][ALICE] (+可选附言)  (待 BOB 处理)
BOB   ──accept_friend(ALICE)──────▶ Friendships 双向建立 + 申请/附言清理
      ──reject_friend(ALICE)──────▶ 申请/附言清理，不建立关系
ALICE ──cancel_friend_request(BOB)─▶ 撤回自己的待处理申请（连带附言）
ALICE ──set_friend_meta(BOB, 备注?, 分组?)─▶ 设置/清除对 BOB 的私有备注/分组（须已是好友）
```

- **双向申请快捷路径**：若 A 申请 B 时，B 此前已申请过 A，则立即成为好友（免二次确认）。
- **好友申请附言（验证消息）**：`request_friend` 可携带可选附言（`MaxFriendRequestMsgLen` 字节内），
  存于 `FriendRequestMsg`，供 B 在收件箱查看；同意/拒绝/撤回/快捷路径时随申请一并清理。
- **拒绝条件**：自己、已是好友、重复申请、被对方拉黑、对方 `Closed`、对方收件申请达上限。
- **`remove_friend`** 仍是单方面操作（任何一方都可解除关系），并清理双方的备注/分组。

> **安全修复（单向授权缺口）**：旧版 `add_friend` 允许任意账户单方面即建立双向好友，
> 从而**绕过对方的 `FriendsOnly`/隐私闸门**直接获得发送权限。已移除该入口（`call_index(4)` 留空），
> 好友关系仅能经上述握手建立。

> **通讯录元数据（备注 / 分组）**：自 P2 起，好友**备注名**与**单标签分组**上链，存于
> `FriendRemark` / `FriendGroupTag`（`(owner, friend) -> bytes`）。设计取舍：仅**好友之间**
> 可设、**owner 私有且单向**（B 看不到 A 给 B 的备注）、长度有界（`MaxFriendRemarkLen` /
> `MaxFriendGroupLen`）、解除好友即清理；以此在「跨端同步通讯录」与「隐私 / 状态膨胀」之间取平衡。
> 星标等更高频的纯展示偏好仍建议放客户端/链下。

## 平台合规（治理）

- **平台级禁言**：`GovernanceOrigin`（Root / 技术委员会多数）经 `force_mute_account(who, until?)`
  / `force_unmute_account(who)` 设/撤账户禁言（`MutedAccounts`）。被禁言账户作为**发送方**在
  `check_permission` 最高优先级被拒（`DeniedSenderMuted`），并经 `can_send_message` 联动私聊门控。
- **举报 / 存证**：任意账户 `report(target, reason_cid)` 就账户 / 群 / 消息发起举报，理由为
  IPFS CID（链上无明文），按举报人 `ReportCooldown` 冷却、全局 `MaxOpenReports` 上限约束
  `Reports` 增长；治理 `resolve_report(id, upheld)` 关闭并移除。

## 存储结构

- `PrivacySettingsOf`: 用户隐私设置
- `Friendships`: 好友关系（双向存储）
- `FriendRequests`: 待处理好友申请，键为 (接收方, 发起方)，使收件申请可前缀扫描
- `IncomingFriendRequestCount`: 单账户待处理收件申请计数（防刷上限 `MaxFriendRequests`）
- `SceneAuthorizations`: 场景授权（排序后的用户对）

## 事件

- `PrivacySettingsUpdated`
- `UserBlocked` / `UserUnblocked`
- `FriendshipCreated` / `FriendshipRemoved`
- `FriendRequestSent` / `FriendRequestAccepted` / `FriendRequestRejected` / `FriendRequestCancelled`
- `SceneAuthorizationGranted` / `SceneAuthorizationRevoked`

## 查询（Runtime API / 辅助方法）

- `is_friend(user1, user2)`
- `list_friends(who)`：列出某用户所有好友（通讯录列表）
- `list_incoming_friend_requests(who)`：列出待处理收件好友申请发起方

## 依赖

- `pallet-chat-common`: 共享类型和工具
