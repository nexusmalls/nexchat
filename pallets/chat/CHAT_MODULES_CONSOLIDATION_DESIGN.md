# 聊天模块统一整合设计方案

> **创建日期**: 2025-12-28
> **状态**: 历史记录（整合已完成）
>
> **⚠️ 现状提示（2026-06-03）**：本文档为整合期的历史设计稿，其中描述的 `ai`
> 子模块（与逝者数字代理对话）已被移除，旧版顶层 `pallet-chat` 亦已删除。
> 当前实际生效的聊天模块以 `README.md` 为准：`common` / `permission` / `core` /
> `group` 四个，前三者 + group 已接入 runtime。

---

## 一、现状分析

### 1.1 当前聊天相关模块分布

| 模块 | 路径 | 功能描述 |
|------|------|----------|
| **chat** | `pallets/chat/` | 基础私聊功能（链上元数据 + IPFS内容） |
| **ai-chat** | `pallets/ai-chat/` | AI对话集成层（与逝者数字代理对话） |
| **chat-permission** | `pallets/chat-permission/` | 聊天权限系统（多场景权限控制） |
| **smart-group-chat** | `pallets/smart-group-chat/` | 智能群聊系统（四种加密模式） |

### 1.2 模块依赖关系

```text
                    ┌─────────────────────┐
                    │   chat-permission   │
                    │   (权限控制层)       │
                    └──────────┬──────────┘
                               │
           ┌───────────────────┼───────────────────┐
           │                   │                   │
           ▼                   ▼                   ▼
    ┌─────────────┐    ┌─────────────┐    ┌─────────────────┐
    │    chat     │    │   ai-chat   │    │ smart-group-chat│
    │  (基础私聊)  │    │  (AI对话)   │    │   (智能群聊)     │
    └──────┬──────┘    └──────┬──────┘    └────────┬────────┘
           │                  │                    │
           └──────────────────┼────────────────────┘
                              │
                              ▼
                    ┌─────────────────────┐
                    │   stardust-ipfs     │
                    │   (内容存储层)       │
                    └─────────────────────┘
```

### 1.3 各模块核心功能

#### chat (基础私聊)
- 一对一私聊消息
- 会话管理
- 已读/未读状态
- 软删除机制
- 黑名单系统
- 频率限制

#### ai-chat (AI对话)
- 与逝者数字代理对话
- 依赖 pallet-deceased 和 pallet-deceased-ai
- AI训练数据集成

#### chat-permission (权限控制)
- 基于场景的权限控制
- 多场景共存
- OTC订单聊天权限

#### smart-group-chat (智能群聊)
- 四种加密模式（军用/商用/选择性/公开）
- 乐观UI更新
- AI智能决策
- 分层存储

---

## 二、整合方案

### 2.1 推荐方案：统一文件夹 + 子模块结构

将所有聊天相关模块整合到 `pallets/chat/` 文件夹下，采用类似 `pallets/divination/` 的组织方式。

#### 目标目录结构

```text
pallets/chat/
├── README.md                    # 聊天系统总览文档
├── CHAT_MODULES_CONSOLIDATION_DESIGN.md  # 本设计文档
│
├── core/                        # 核心私聊模块 (原 chat)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── types.rs
│       ├── weights.rs
│       └── tests.rs
│
├── ai/                          # AI对话模块 (原 ai-chat)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── types.rs
│       └── tests.rs
│
├── permission/                  # 权限控制模块 (原 chat-permission)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── types.rs
│       └── tests.rs
│
├── group/                       # 智能群聊模块 (原 smart-group-chat)
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
│       ├── lib.rs
│       ├── types.rs
│       ├── encryption.rs        # 加密模式实现
│       ├── ai_decision.rs       # AI决策引擎
│       ├── optimistic.rs        # 乐观UI更新
│       └── tests.rs
│
└── common/                      # 共享类型和工具 (新增)
    ├── Cargo.toml
    ├── README.md
    └── src/
        ├── lib.rs
        ├── types.rs             # 共享类型定义
        ├── traits.rs            # 共享trait定义
        └── utils.rs             # 共享工具函数
```

### 2.2 模块重命名映射

| 原模块名 | 新模块名 | 新路径 | Crate名称 |
|---------|---------|--------|-----------|
| pallet-chat | pallet-chat-core | pallets/chat/core | pallet-chat-core |
| pallet-ai-chat | pallet-chat-ai | pallets/chat/ai | pallet-chat-ai |
| pallet-chat-permission | pallet-chat-permission | pallets/chat/permission | pallet-chat-permission |
| pallet-smart-group-chat | pallet-chat-group | pallets/chat/group | pallet-chat-group |
| (新增) | pallet-chat-common | pallets/chat/common | pallet-chat-common |

### 2.3 共享模块 (common) 设计

提取各模块共用的类型和功能：

```rust
// pallets/chat/common/src/types.rs

/// 消息类型枚举（所有聊天模块共用）
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum MessageType {
    Text,    // 文本消息
    Image,   // 图片消息
    File,    // 文件消息
    Voice,   // 语音消息
    Video,   // 视频消息
    System,  // 系统消息
    AI,      // AI生成消息
}

/// 加密模式枚举（群聊和私聊共用）
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum EncryptionMode {
    Military,   // 军用级加密
    Business,   // 商用级加密
    Selective,  // 选择性加密
    Transparent,// 透明公开
}

/// 消息状态枚举
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum MessageStatus {
    Pending,    // 待发送（乐观UI）
    Sent,       // 已发送
    Delivered,  // 已送达
    Read,       // 已读
    Failed,     // 发送失败
    Deleted,    // 已删除
}

/// CID验证工具
pub fn is_cid_encrypted(cid: &[u8]) -> bool {
    // 标准CIDv0检测
    if cid.len() == 46 && cid.starts_with(b"Qm") {
        return false; // 未加密
    }
    // 加密后的CID通常>50字节
    cid.len() > 50
}
```

```rust
// pallets/chat/common/src/traits.rs

/// 聊天权限检查trait
pub trait ChatPermissionCheck<AccountId> {
    /// 检查用户是否可以发送消息给目标
    fn can_send_message(sender: &AccountId, receiver: &AccountId) -> bool;
    
    /// 检查用户是否可以加入群组
    fn can_join_group(user: &AccountId, group_id: u64) -> bool;
    
    /// 检查用户是否被拉黑
    fn is_blocked(blocker: &AccountId, blocked: &AccountId) -> bool;
}

/// IPFS内容存储trait
pub trait IpfsContentStore {
    /// 验证CID格式
    fn validate_cid(cid: &[u8]) -> bool;
    
    /// 检查CID是否已加密
    fn is_encrypted(cid: &[u8]) -> bool;
}

/// 频率限制trait
pub trait RateLimiter<AccountId, BlockNumber> {
    /// 检查是否超过频率限制
    fn check_rate_limit(sender: &AccountId) -> Result<(), &'static str>;
    
    /// 记录发送行为
    fn record_send(sender: &AccountId, block: BlockNumber);
}
```

---

## 三、迁移步骤

### 3.1 Phase 1: 创建目录结构 (无破坏性)

1. 创建 `pallets/chat/common/` 目录和基础文件
2. 提取共享类型到 common 模块
3. 更新 workspace Cargo.toml

### 3.2 Phase 2: 迁移核心模块

1. 将 `pallets/chat/` 内容移动到 `pallets/chat/core/`
2. 更新 Cargo.toml 中的 crate 名称
3. 更新 runtime 依赖引用

### 3.3 Phase 3: 迁移其他模块

1. 移动 `pallets/ai-chat/` → `pallets/chat/ai/`
2. 移动 `pallets/chat-permission/` → `pallets/chat/permission/`
3. 移动 `pallets/smart-group-chat/` → `pallets/chat/group/`
4. 更新所有 Cargo.toml 和依赖

### 3.4 Phase 4: 清理和验证

1. 删除旧目录
2. 更新 workspace 成员列表
3. 运行完整测试套件
4. 更新文档

---

## 四、Cargo.toml 配置示例

### 4.1 Workspace Cargo.toml 更新

```toml
[workspace]
members = [
    # ... 其他模块
    
    # 聊天系统模块（统一目录）
    "pallets/chat/common",
    "pallets/chat/core",
    "pallets/chat/ai",
    "pallets/chat/permission",
    "pallets/chat/group",
]
```

### 4.2 common 模块 Cargo.toml

```toml
[package]
name = "pallet-chat-common"
version = "0.1.0"
description = "聊天系统共享类型和工具"
authors = ["Stardust Team"]
edition = "2021"
license = "Apache-2.0"

[dependencies]
codec = { package = "parity-scale-codec", version = "3.6.1", default-features = false, features = ["derive"] }
scale-info = { version = "2.11.0", default-features = false, features = ["derive"] }
frame-support = { git = "https://github.com/paritytech/polkadot-sdk.git", branch = "stable2506", default-features = false }
sp-std = { git = "https://github.com/paritytech/polkadot-sdk.git", branch = "stable2506", default-features = false }

[features]
default = ["std"]
std = [
    "codec/std",
    "scale-info/std",
    "frame-support/std",
    "sp-std/std",
]
```

### 4.3 core 模块 Cargo.toml

```toml
[package]
name = "pallet-chat-core"
version = "0.1.0"
description = "去中心化私聊功能模块"
authors = ["Stardust Team"]
edition = "2021"
license = "Apache-2.0"

[dependencies]
# ... 原有依赖

# 本地依赖
pallet-chat-common = { path = "../common", default-features = false }

[features]
default = ["std"]
std = [
    # ... 原有 std features
    "pallet-chat-common/std",
]
```

### 4.4 group 模块 Cargo.toml

```toml
[package]
name = "pallet-chat-group"
version = "1.0.0"
description = "智能群聊系统"
authors = ["Stardust Team"]
edition = "2021"
license = "Apache-2.0"

[dependencies]
# ... 原有依赖

# 本地依赖
pallet-chat-common = { path = "../common", default-features = false }
pallet-chat-core = { path = "../core", default-features = false }
pallet-stardust-ipfs = { path = "../../stardust-ipfs", default-features = false }

[features]
default = ["std"]
std = [
    # ... 原有 std features
    "pallet-chat-common/std",
    "pallet-chat-core/std",
]
```

---

## 五、Runtime 配置更新

### 5.1 runtime/Cargo.toml

```toml
[dependencies]
# 聊天系统模块
pallet-chat-common = { path = "../pallets/chat/common", default-features = false }
pallet-chat-core = { path = "../pallets/chat/core", default-features = false }
pallet-chat-ai = { path = "../pallets/chat/ai", default-features = false }
pallet-chat-permission = { path = "../pallets/chat/permission", default-features = false }
pallet-chat-group = { path = "../pallets/chat/group", default-features = false }

[features]
std = [
    # ...
    "pallet-chat-common/std",
    "pallet-chat-core/std",
    "pallet-chat-ai/std",
    "pallet-chat-permission/std",
    "pallet-chat-group/std",
]
```

### 5.2 runtime/src/lib.rs

```rust
// 聊天系统配置
impl pallet_chat_core::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_chat_core::weights::SubstrateWeight<Runtime>;
    type MaxCidLen = ConstU32<100>;
    type MaxSessionsPerUser = ConstU32<100>;
    type MaxMessagesPerSession = ConstU32<1000>;
    type RateLimitWindow = ConstU32<100>;
    type MaxMessagesPerWindow = ConstU32<10>;
    type MessageExpirationTime = ConstU32<2592000>;
}

impl pallet_chat_ai::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    // ... AI聊天配置
}

impl pallet_chat_permission::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    // ... 权限配置
}

impl pallet_chat_group::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Randomness = RandomnessCollectiveFlip;
    type TimeProvider = Timestamp;
    // ... 群聊配置
}

construct_runtime!(
    pub enum Runtime {
        // ... 其他模块
        
        // 聊天系统
        ChatCore: pallet_chat_core,
        ChatAi: pallet_chat_ai,
        ChatPermission: pallet_chat_permission,
        ChatGroup: pallet_chat_group,
    }
);
```

---

## 六、优势分析

### 6.1 整合后的优势

| 方面 | 整合前 | 整合后 |
|------|--------|--------|
| **代码组织** | 分散在4个独立目录 | 统一在 chat/ 目录下 |
| **类型复用** | 各模块重复定义 | 共享 common 模块 |
| **依赖管理** | 路径引用混乱 | 清晰的相对路径 |
| **文档维护** | 分散的README | 统一的文档结构 |
| **测试集成** | 独立测试 | 可共享测试工具 |
| **版本管理** | 独立版本 | 统一版本策略 |

### 6.2 与 divination 模块对比

```text
pallets/divination/          pallets/chat/
├── common/                  ├── common/
├── bazi/                    ├── core/
├── qimen/                   ├── ai/
├── liuyao/                  ├── permission/
├── ziwei/                   ├── group/
├── ...                      └── ...
```

采用相同的组织模式，保持项目一致性。

---

## 七、风险评估

### 7.1 潜在风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 路径变更导致编译失败 | 高 | 分阶段迁移，每步验证 |
| Runtime配置错误 | 高 | 完整测试覆盖 |
| 依赖循环 | 中 | 仔细设计 common 模块 |
| 文档过时 | 低 | 同步更新文档 |

### 7.2 回滚策略

- 保留原目录结构的 git 历史
- 使用 feature branch 进行迁移
- 迁移完成前不删除原目录

---

## 八、时间估算

| 阶段 | 工作内容 | 预计时间 |
|------|----------|----------|
| Phase 1 | 创建 common 模块 | 2小时 |
| Phase 2 | 迁移 core 模块 | 2小时 |
| Phase 3 | 迁移其他3个模块 | 4小时 |
| Phase 4 | 测试和文档更新 | 2小时 |
| **总计** | | **10小时** |

---

## 九、决策点

### 需要确认的问题

1. **是否保留原 crate 名称？**
   - 选项A: 保留原名（如 pallet-chat, pallet-ai-chat）
   - 选项B: 统一命名（如 pallet-chat-core, pallet-chat-ai）
   - **建议**: 选项B，更清晰的命名空间

2. **common 模块是否作为独立 crate？**
   - 选项A: 独立 crate，可被外部引用
   - 选项B: 仅作为内部模块
   - **建议**: 选项A，便于未来扩展

3. **是否需要向后兼容？**
   - 如果需要，可以在原位置保留 re-export
   - **建议**: 不保留，一次性迁移

---

*文档结束*
