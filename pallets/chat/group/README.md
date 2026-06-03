# Pallet Chat Group

智能群聊系统 - 支持四种加密模式

## 概述

本模块提供群聊功能：

- **群组管理**：创建、解散群组
- **成员管理**：加入、离开、踢出成员
- **消息发送**：群组消息广播
- **四种加密模式**：Military/Business/Selective/Transparent

## 加密模式

| 模式 | 描述 | 适用场景 |
|------|------|----------|
| Military | 军用级量子抗性加密 | 高度机密群组 |
| Business | 商用级AES-256加密 | 普通私密群组（默认）|
| Selective | 选择性加密 | 部分消息需加密 |
| Transparent | 透明公开 | 公开群组 |

## 核心功能

### 群组创建

```rust
// 创建群组
Chat::create_group(
    origin,
    name,           // 群组名称
    description,    // 描述（可选）
    encryption_mode,// 加密模式
    is_public,      // 是否公开
)?;
```

### 消息发送

```rust
// 发送群组消息
Chat::send_group_message(
    origin,
    group_id,       // 群组ID
    content,        // 消息内容（CID）
    message_type,   // 消息类型
)?;
```

## 防滥用与治理（P2）

- **写入型 MLS 操作限频**：`commit` / `anchor_message_digest` 按账户窗口限频
  （`MlsActionWindow` / `MaxMlsActionsPerWindow`，复用 `pallet-chat-common::rate_limit`），
  约束 `HandshakeLog` / `MessageDigestAnchor` 增长；超限报 `RateLimited`。
- **治理冻结 / 强制解散**：`GovernanceOrigin`（Root / 治理）可：
  - `set_group_frozen(group_id, frozen)`：冻结群拒绝 `commit` / `anchor_message_digest` /
    `request_join`（报 `GroupFrozen`），元数据仍可读，便于客户端展示"已冻结"。
  - `force_disband_group(group_id)`：无视群内归属强制解散并退还押金（同 `disband_group` 清理）。

## 存储结构

- `Groups`: 群组信息
- `GroupMembers`: 群组成员
- `UserGroups`: 用户的群组列表
- `GroupMessages`: 群组消息
- `NextMessageId`: 消息ID计数器

## 依赖

- `pallet-chat-common`: 共享类型（MessageType, EncryptionMode等）
- `pallet-chat-permission`: 权限检查
- `stardust-media-common`: 媒体验证
