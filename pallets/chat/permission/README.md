# Pallet Chat Permission

聊天权限系统模块 - 基于场景的多场景共存权限控制

## 概述

本模块提供聊天权限管理功能：

- **场景授权**：业务模块可为用户授予基于场景的聊天权限
- **权限级别**：Open/FriendsOnly/Whitelist/Closed（`Whitelist` 已等同 `FriendsOnly`，见隐私章节）
- **聊天能力撤销纪元**：`CapabilityEpoch` —— 链下能力令牌的链上撤销锚点

> ⚠️ **黑白名单已下链（审计 P1）**：链上 `block_list` / `whitelist` 明文存储及
> `block_user`/`unblock_user`/`add_to_whitelist`/`remove_from_whitelist` extrinsic
> 与对应事件**已整体删除**。理由：链上明文（乃至哈希）的拉黑 / 放行名单可被枚举，会
> 泄露本设计要隐藏的通信关系。拉黑 / 放行改由**链下、接收方签名的能力令牌**承载，撤销以
> `bump_capability_epoch`（账户级）+ `pallet-chat-inbox::revoke_tag`（每联系人定向）实现。
> `Whitelist` 级别现等同 `FriendsOnly`。call_index 2/3/6/7 留空不复用。

## 隐私：好友图谱已移出链上（C 方案定稿）

> **重大变更**：链上双向好友图谱（`Friendships` + 好友申请 + 备注/分组 + 对应 extrinsic /
> RPC / runtime API）**已整体删除**，使「谁与谁建立联系」不再在链上公开可见。联系人与
> 「允许私聊我」的权利改由**链下、接收方签名的聊天能力令牌**承载（加密通讯录保险库），
> 详见 `../CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`。
>
> 链上仅保留每账户的 **`CapabilityEpoch`**（单调递增计数器）作为撤销锚点：账户经
> `bump_capability_epoch()` 递增纪元，即可使其此前签发的所有能力令牌失效（删除联系人、
> 更换设备、疑似泄露时使用）。链下 relay/客户端用 `CapabilityEpoch[签发者]` 校验令牌新鲜度。
>
> 旧版 `add_friend`/`request_friend`/`accept_friend`/`reject_friend`/
> `cancel_friend_request`/`remove_friend`/`set_friend_meta`（call_index 4/5/8/9/10/11/12）
> 及其存储/事件/查询全部移除；索引留空，不复用。

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

1. **平台级禁言**（最高优先级拒绝）
2. **场景授权检查**
3. **隐私设置检查**：`Open` 放行；`FriendsOnly` / `Whitelist` 一律 `DeniedRequiresFriend`
   （真正的「联系人」闸门由链下能力令牌强制）；`Closed` 拒绝所有。

> 注：审计 P1 后**不再有链上黑名单检查**——拉黑由链下能力令牌 / 信箱标签撤销执行。

## 平台合规（治理）

- **平台级禁言**：`GovernanceOrigin`（Root / 技术委员会多数）经 `force_mute_account(who, until?)`
  / `force_unmute_account(who)` 设/撤账户禁言（`MutedAccounts`）。被禁言账户作为**发送方**在
  `check_permission` 最高优先级被拒（`DeniedSenderMuted`），并经 `can_send_message` 联动私聊门控。
- **举报 / 存证**：任意账户 `report(target, reason_cid)` 就账户 / 群 / 消息发起举报，理由为
  IPFS CID（链上无明文），按举报人 `ReportCooldown` 冷却、全局 `MaxOpenReports` 上限约束
  `Reports` 增长；治理 `resolve_report(id, upheld)` 关闭并移除。

## 存储结构

- `PrivacySettingsOf`: 用户隐私设置
- `CapabilityEpoch`: 每账户聊天能力撤销纪元（`account -> u32`，链下能力令牌的撤销锚点）
- `SceneAuthorizations`: 场景授权（排序后的用户对）
- `MutedAccounts` / `Reports` 等：平台合规

## 事件

- `PrivacySettingsUpdated`
- `CapabilityEpochBumped`
- `SceneAuthorizationGranted` / `SceneAuthorizationRevoked`
- `AccountMuted` / `AccountUnmuted` / `ReportFiled` / `ReportResolved`

## 查询（Runtime API / 辅助方法）

- `check_chat_permission(sender, receiver)`：权限判定结果
- `capability_epoch(who)`：某账户当前的能力撤销纪元（链下校验令牌新鲜度）
- `get_active_scenes(user1, user2)` / `get_privacy_settings_summary(user)`

## 依赖

- 仅 Substrate 核心库（`frame-support` / `frame-system` / `sp-*`）。
  审计 P1 后**不再依赖 `pallet-chat-common`**（原为声明但零 import 的 dead dep）。
