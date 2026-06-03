# Pallet Chat Common

聊天系统共享模块，提供所有聊天子模块共用的类型和工具。

## 概述

本模块是聊天系统的基础模块，不包含任何存储或业务逻辑，仅提供：

- **共享类型**：MessageType、MessageStatus、EncryptionMode、ChatUserId
- **共享Trait**：ChatPermissionCheck、FriendshipCheck、ChatUserIdProvider
- **工具函数**：CID验证、频率限制

## 模块结构

```
common/
├── src/
│   ├── lib.rs          # 模块入口
│   ├── types.rs        # 共享类型定义
│   ├── traits.rs       # 共享trait定义
│   ├── validation.rs   # CID验证工具
│   └── rate_limit.rs   # 频率限制工具
├── Cargo.toml
└── README.md
```

## 共享类型

### MessageType

统一的消息类型枚举：

```rust
pub enum MessageType {
    Text,    // 文本消息
    Image,   // 图片消息
    File,    // 文件消息
    Voice,   // 语音消息
    Video,   // 视频消息
    System,  // 系统消息
    AI,      // AI生成消息
}
```

### EncryptionMode

加密模式枚举（用于群聊）：

```rust
pub enum EncryptionMode {
    Military,    // 军用级（量子抗性）
    Business,    // 商用级（标准加密）
    Selective,   // 选择性
    Transparent, // 透明公开
}
```

### ChatUserId

11位数字的用户ID类型，用于隐私保护：

```rust
pub type ChatUserId = u64;
// 范围：10,000,000,000 - 99,999,999,999
```

## 共享Trait

### ChatPermissionCheck

```rust
pub trait ChatPermissionCheck<AccountId> {
    fn can_send_message(sender: &AccountId, receiver: &AccountId) -> bool;
    fn is_blocked(blocker: &AccountId, blocked: &AccountId) -> bool;
}
```

### ChatUserIdProvider

```rust
pub trait ChatUserIdProvider<AccountId> {
    fn get_chat_user_id(account: &AccountId) -> Option<ChatUserId>;
    fn get_account(chat_user_id: ChatUserId) -> Option<AccountId>;
}
```

## 工具函数

### CID验证

> **审计 C**：`is_cid_encrypted` / `validate_encrypted_cid` 是**可绕过的长度/前缀启发式**，
> **不能**当作内容已加密的证明，也不得用作链上加密闸门。`pallet-chat-core` 已移除该启发式，
> 仅做非空 + 长度 sanity；加密由客户端 MLS E2EE 保证。下列“加密”相关函数当前未被在用 pallet
> 调用，保留仅作格式探测。

```rust
// 【启发式，非证明】长度/前缀猜测 CID 是否“像”加密内容；勿用作加密闸门
pub fn is_cid_encrypted(cid: &[u8]) -> bool;

// 验证CID格式（非空 + 长度 + CIDv0/v1 形态）
pub fn validate_cid(cid: &[u8], max_len: usize) -> CidValidationResult;

// 【启发式，非证明】在 validate_cid 基础上额外要求“看起来已加密”；勿用作加密闸门
pub fn validate_encrypted_cid(cid: &[u8], max_len: usize) -> Result<(), &'static str>;
```

### 频率限制

```rust
// 检查并更新频率限制
pub fn check_and_update_rate_limit<BlockNumber>(
    state: &mut RateLimitState<BlockNumber>,
    current_block: BlockNumber,
    window: BlockNumber,
    max_count: u32,
) -> RateLimitResult;

// 计算剩余配额
pub fn remaining_quota<BlockNumber>(...) -> u32;
```

## 使用示例

```rust
use pallet_chat_common::{
    MessageType, EncryptionMode, ChatUserId,
    is_cid_encrypted, validate_encrypted_cid,
    check_and_update_rate_limit, RateLimitState,
};

// 使用消息类型
let msg_type = MessageType::from_u8(1); // Image

// 验证CID
let cid = b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
if !is_cid_encrypted(cid) {
    // 要求加密
}

// 频率限制
let mut state = RateLimitState::new();
if check_and_update_rate_limit(&mut state, 100, 10, 5).is_allowed() {
    // 允许发送
}
```

## 依赖

本模块仅依赖 Substrate 核心库，不依赖任何 pallet。

```toml
[dependencies]
codec = { package = "parity-scale-codec" }
scale-info = { ... }
frame-support = { ... }
sp-std = { ... }
sp-runtime = { ... }
```
