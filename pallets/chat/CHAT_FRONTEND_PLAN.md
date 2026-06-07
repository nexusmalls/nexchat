# Chat 前端开发方案 / Chat Frontend Development Plan

> 状态：方案 / 待评审（**前端 + relay 职责，链上不新增 extrinsic / storage**）
> 适用范围：基于 `pallets/chat/{common,permission,core,group,inbox}` 现有链上接口的 IM 客户端
> 关联：
> - `pallets/chat/README.md`（链上 / 链下边界 + 客户端 Merge Spec）
> - `CHAT_GROUP_CLIENT_INTEGRATION.md`（群客户端调用时序、错误映射）
> - `CHAT_OFFCHAIN_DELIVERY_DESIGN.md`（盲化一次性投递令牌）
> - `CHAT_P2_SESSION_ANCHOR_DESIGN.md`（跨设备会话发现）
> - `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`（reply / mention / reaction / forward / 阅后即焚信封）
> - `CHAT_LARGE_FILE_SPEC.md`（大文件加密分块 / manifest / Pin）
> - `CHAT_DEVICE_RETENTION_DESIGN.md`（设备端保留与清理）
> - `common/src/runtime_api.rs`、`group/src/runtime_api.rs`、`inbox/src/runtime_api.rs`、
>   `permission/src/runtime_api.rs`、`node/src/chat_rpc.rs`（只读 RPC）

---

## 0. 一句话结论 / TL;DR

CN: 这不是"链上聊天"前端，而是一个**端到端加密（MLS / RFC 9420）的 IM 重客户端**——链只承担
很薄的一层：身份/成员变更定序（DS+AS）、System 通知、投递信箱注册表、权限/隐私。**人类消息
（文本/图片/语音/文件，私聊与群聊）全部链下**（MLS + relay + IPFS，密文不触链）。因此前端必须
自己实现 MLS 引擎、relay 投递、本地加密存储，并按 README 的 **Merge Spec** 把"链上切片 ⊕ 链下
状态"合并成真实会话列表。`chat_listConversations` 绝不能直接当首页。
**选定技术栈：Rust 共享核心（openmls + subxt + 加密 SQLite）+ Tauri 2.0 外壳 + React/TS UI**
——所有密码学/密钥/签名收进 Rust 核心，UI 为无密钥哑层（详见 §2）。

EN: This is NOT an "on-chain chat" frontend; it is an **end-to-end encrypted (MLS / RFC 9420) IM
client**. The chain is a thin layer only: identity/membership ordering (DS+AS), System notifications,
the delivery-inbox registry, and permission/privacy. **All human messages (text/image/voice/file,
both 1:1 and group) are off-chain** (MLS + relay + IPFS; ciphertext never touches the chain). The
frontend therefore owns the MLS engine, relay delivery, and local encrypted storage, and MUST merge
the on-chain slice with off-chain state per the README **Merge Spec**. `chat_listConversations` is
never the home page by itself.
**Chosen stack: a shared Rust core (openmls + subxt + encrypted SQLite) + a Tauri 2.0 shell +
a React/TS UI** — all crypto/keys/signing live in the Rust core; the UI is a key-free dumb layer
(see §2).

---

## 1. 链上 / 链下职责（决定整个前端架构）

| 角色 | 链上（本仓库 pallets） | 前端 + relay（仓库外，开发主体） |
|---|---|---|
| 人类消息（文本/图片/语音/文件） | **完全不上链** | MLS 加解密 + relay 投递 + IPFS |
| 身份 / 群成员变更定序 | KeyPackage、`commit`/epoch、Welcome 信箱、群元数据 | 构造 MLS Commit/Welcome、本地 TreeKEM 状态 |
| System 通知（订单/争议/治理） | 唯一上链消息，经 `send_message` | 渲染为"平台通知"卡片 |
| 权限 / 隐私 / 平台禁言 | `permission` pallet | 调用前检查、UI 门控 |
| 投递准入令牌 | `inbox` 注册表（epoch + 标签撤销） | 盲签发/兑付（RFC 9474）、relay 验证 |
| 会话列表 / 未读 / 活跃度 / 置顶偏好 | 仅 System 切片 | **客户端权威**（Merge Spec + 加密会话索引 blob） |

> 不变量：**1:1 私聊不建链上群**；不为私聊新增链上会话锚点（隐私，见 P2 设计）。前端不得为
> "方便"反向请求链上加接口。

---

## 2. 技术栈（已选定）/ Tech stack (decided)

**决策驱动因素**：`openmls` 是 Rust crate，且 MLS 私钥 / KeyVault / 签名都是高敏感路径。
因此最优解是把所有敏感逻辑收进一个 **Rust 共享核心**，UI 做成"无密钥哑层"。

**选定方案：Rust 共享核心 + Tauri 2.0 + React/TS**

| 层 | 选型 | 理由 |
|---|---|---|
| 共享核心 `chat-client-core`（Rust crate） | `openmls` + `subxt` + `rusqlite`/SQLCipher + `aes-gcm` + relay 客户端 + KeyVault | 一份 Rust 核心，密码学/密钥/签名都不出 Rust |
| 外壳 | **Tauri 2.0** | 同一 Rust 核心覆盖桌面（Win/macOS/Linux）+ 移动（iOS/Android），webview 经 command/event 调用核心 |
| UI | React + TypeScript + Vite + Tailwind；TanStack Query（链上只读缓存）+ Zustand（本地状态） | 复用最成熟前端生态；UI 不碰密钥、不做加密 |
| 链交互 | **`subxt`（在 Rust 核心里）**，发交易 + `chat_*` RPC | KeyVault/MLS 私钥已在 Rust，签名与只读都留核心，私钥不跨进 JS |
| MLS 引擎 | `openmls` **原生编译**（非 WASM），封成 `MlsEngine` | 性能最好、参考实现 |
| 本地存储 | SQLite（按 `CHAT_DEVICE_RETENTION_DESIGN.md` §7）+ SQLCipher / 派生密钥加密 | |
| 媒体/大文件 | IPFS 客户端（kubo/helia）+ AES-256-GCM 分块 | |

**否决的备选**：
- **纯 Web + openmls WASM**：无安全密钥存储、无像样本地加密 DB、WASM 密码学性能/侧信道顾虑、
  无法后台收消息。仅适合"只读网页伴侣端"，不作主端。
- **Flutter**：MLS 仍需 FFI 调同一 Rust 核心，而 Dart 侧 Substrate 生态薄弱（无 polkadot-js
  级库）。Tauri 既享 Rust 核心又保留 Web/polkadot 生态。
- **React Native**：同样要 FFI 到 Rust，但桌面端弱；Tauri 2 在"密码学重 + 桌面+移动统一"
  场景整合度更好。

**需预先正视的取舍**：
1. **Tauri 移动端较新**（2.0 GA 不久），个别插件成熟度有风险——移动端排期预留缓冲。
2. **移动端后台收消息**（尤其 iOS）需配合原生推送 + relay 唤醒，是 RelayClient 在移动端的
   专门工作项。

---

## 3. 前端分层架构 / Layered architecture

边界：UI（React/TS，无密钥）↔ Tauri command/event ↔ Rust 共享核心 `chat-client-core`
（密码学/密钥/签名/存储全在此）。

```
╔══════════ React / TS（webview，无密钥哑层）══════════╗
║  UI 层：会话列表 / 聊天窗 / 群管理 / 设置             ║
╚══════════════════════╤══════════════════════════════╝
                       │  Tauri command / event（IPC）
╔══════════════════════╪═══ Rust 核心 chat-client-core ═══╗
║  ConversationStore（Merge 引擎）                       ║  ← 核心：链上切片 ⊕ 链下状态
║ ┌──────────────┬──────────────┬───────────────┐       ║
║ │ ChainClient  │  MlsEngine   │  RelayClient   │       ║
║ │ (subxt:tx +  │ (openmls)    │ (投递/兑付/补投) │       ║
║ │  chat_* RPC) │              │                │       ║
║ ├──────────────┴──────────────┴───────────────┤       ║
║ │  LocalStore (SQLite+SQLCipher) + MediaStore   │       ║
║ │            (IPFS + AES-GCM 加密)              │       ║
║ ├───────────────────────────────────────────────┤     ║
║ │ KeyVault（主密钥派生：会话索引/inbox/MLS 私钥）│     ║
║ └───────────────────────────────────────────────┘     ║
╚════════════════════════════════════════════════════════╝
```

> 边界原则：**任何密钥、明文消息、签名都不跨过 Tauri IPC 进入 webview**。UI 只收到渲染所需的
> 脱敏视图模型（会话卡片、已解密文本、状态），并通过 command 触发核心动作。

1. **ChainClient**（subxt）—— 封装所有链交互（写交易 + 免费只读 RPC，见 §4）。
2. **MlsEngine** —— 封装 openmls：KeyPackage、建群、Commit/Welcome、application message 加解密、
   epoch 补齐。**所有密码学只在这里**。
3. **RelayClient** —— 链下投递：盲签令牌兑付/请求、sealed-sender 封装、按 `t` 去重、离线补投、
   ephemeral TTL。
4. **ConversationStore（Merge 引擎）** —— 前端心脏，严格实现 README 的 Merge Spec（见 §6）。
5. **LocalStore / MediaStore** —— 本地消息时间线、会话索引、媒体冷热分层。
6. **KeyVault** —— 账户主密钥派生子密钥：`K_index = KDF(master, "chat/conv-index/v1")`、inbox
   一次性 controller 钥、MLS 私钥保管。

---

## 3.1 `chat-client-core` 模块清单与 Tauri 接口草案 / Core crate layout & Tauri API

### 3.1.1 crate 模块布局 / Module layout

```
chat-client-core/
├── src/
│   ├── lib.rs                # 对外门面 ChatCore：聚合各子系统，暴露给 tauri 适配层
│   ├── error.rs              # CoreError（统一错误；映射链上 DispatchError / MLS / relay / io）
│   ├── types.rs              # 脱敏视图模型（ConversationVM/MessageVM/...）+ 入参 DTO
│   ├── keyvault/             # 主密钥解锁、子密钥派生、私钥安全存储（OS keychain / 加密文件）
│   │   ├── mod.rs
│   │   └── derive.rs         # KDF 路径：conv-index / inbox-controller / mls-store
│   ├── chain/                # ChainClient（subxt）
│   │   ├── mod.rs
│   │   ├── tx.rs             # 全部 extrinsic 构造 + 签名 + 提交 + 事件等待（见 §4.1）
│   │   ├── query.rs          # chat_* 只读 RPC / runtime API（见 §4.2）
│   │   └── metadata.rs       # subxt 生成的运行时元数据绑定
│   ├── mls/                  # MlsEngine（openmls 封装）—— 所有密码学唯一入口
│   │   ├── mod.rs
│   │   ├── identity.rs       # KeyPackage 生成/池/轮换
│   │   ├── group.rs          # 建群、Commit/Welcome 处理、epoch 补齐、成员变更
│   │   ├── session.rs        # 1:1 成对会话
│   │   ├── message.rs        # application message 加解密 + P3/大文件信封编解码
│   │   └── store.rs          # openmls 持久化后端（落 LocalStore，加密）
│   ├── relay/                # RelayClient
│   │   ├── mod.rs
│   │   ├── token.rs          # 盲签令牌：请求/兑付/按 t 去重（Phase 4 前可打桩）
│   │   ├── delivery.rs       # sealed-sender 投递 + 离线补投拉取
│   │   └── ephemeral.rs      # TTL / 阅后即焚
│   ├── store/                # LocalStore + MediaStore
│   │   ├── mod.rs
│   │   ├── schema.rs         # SQLite/SQLCipher 表（见 §5）
│   │   ├── timeline.rs       # 消息时间线读写
│   │   ├── conv_index.rs     # 加密会话索引 blob（IPFS）读写 + 多设备合并（LWW→CRDT）
│   │   └── media.rs          # IPFS 分块/manifest/缩略图 + 冷热分层 + LRU 淘汰
│   └── merge/                # ConversationStore（Merge 引擎，前端心脏）
│       ├── mod.rs
│       └── spec.rs           # README Merge Spec 纯函数 + 宪法测试
└── Cargo.toml                # openmls, subxt, rusqlite(+SQLCipher), aes-gcm, ...
```

> 依赖方向：`merge → {chain, mls, relay, store}`；`mls/relay/store → keyvault`；
> `chain` 独立。`error`/`types` 被全局引用。**密码学只在 `mls/`，密钥只在 `keyvault/`**。

### 3.1.2 对 webview 暴露的 Tauri command / event

命令（webview → 核心，`invoke`）；返回值一律是**脱敏视图模型**，不含密钥/密文。

| command | 入参 | 返回 | 说明 |
|---|---|---|---|
| `unlock` | `passphrase` | `AccountVM` | 解锁主密钥，初始化各子系统 |
| `register_account` | `nickname?` | `AccountVM` | `register_chat_user` + 首发 KeyPackage + 注册 inbox |
| `list_conversations` | — | `ConversationVM[]` | **已 Merge** 的真实会话列表（非链上切片） |
| `open_conversation` | `conv_id, page` | `MessageVM[]` | 拉取/解密某会话时间线分页 |
| `send_message` | `conv_id, body, opts?` | `client_msg_id` | MLS 加密 + relay 投递；`opts` 含 reply/ephemeral 等 |
| `send_media` | `conv_id, file_path, opts?` | `client_msg_id` | 分块加密 + IPFS + 信封（见 §7-E） |
| `react` / `recall_local` / `forward` | `conv_id, target_msg_id, ...` | `()` | P3 链下交互信封 |
| `mark_read` | `conv_id, up_to_msg_id` | `()` | 本地未读推进（+ 必要时链上 System `mark_*`） |
| `create_group` | `name, members[], opts` | `group_id` | 建群 + 首次 commit（一次 ≥2 人，见 §7-B） |
| `group_action` | `group_id, action`（add/remove/transfer/ban/mute/profile/disband…） | `()` | 收口所有群治理 extrinsic + 必要的 MLS commit |
| `join_group` / `claim_welcome_flow` | `group_id` | `()` | 私群申请 / **先读后删** Welcome 入群（见 §7-A） |
| `set_pref` | `conv_id, {pinned,muted,archived}` | `()` | 私聊偏好（链上 + 本地）/ 群本地偏好 |
| `block_contact` | `peer / inbox_tag` | `()` | `revoke_tag` + 必要时 `bump_capability_epoch` |
| `report` | `target, reason_cid` | `report_id` | 举报 |
| `recover_on_new_device` | — | `ConversationVM[]` | 取并解密会话索引 blob + inbox 推导兜底（§7-G） |

事件（核心 → webview，`emit`）：

| event | 载荷 | 触发 |
|---|---|---|
| `conv:updated` | `ConversationVM` | 新消息/未读/活跃度/偏好变化 → 列表增量刷新 |
| `msg:new` | `MessageVM` | 当前会话新消息（已解密脱敏） |
| `msg:status` | `{client_msg_id, state}` | pending→sent→acked / 投递失败 |
| `group:epoch` | `{group_id, epoch, frozen}` | epoch 推进 / 冻结态变化 → 刷新群头/只读态 |
| `key:relock` | — | 主密钥超时上锁 → UI 回登录态 |
| `error` | `{scope, code, action_hint}` | 映射群集成文档 §9 错误→动作表 |

> 约定：**所有耗时/网络/密码学操作在核心异步执行**，webview 通过 command 触发、event 接收结果；
> 视图模型字段命名与 §5 数据模型、§6 Merge 输出保持一致，避免 UI 侧再做语义判断（尤其
> `muted` 必须由核心按 `kind` 解析为 `dnd` / `admin_muted` 两个不同字段，UI 不得共用图标）。

### 3.1.3 视图模型定义 / View models（核心 → webview，已脱敏）

> 原则：视图模型**只含渲染所需的脱敏字段**，无密钥、无密文、无 SCALE 原始 DTO。
> `muted` 的歧义在核心消解：`dnd`（私聊免打扰，能发言）与 `admin_muted`（群禁言，不能发言）
> 是**两个独立布尔字段**，UI 据此用不同图标。

```rust
/// EN: One row of the MERGED conversation list (NOT the on-chain slice).
/// CN: 已 Merge 的会话列表中的一行（非链上切片）。
pub struct ConversationVM {
    pub conv_id: String,              // 统一主键：direct=peer 派生 id；group=group_id
    pub kind: ConvKind,              // Direct | Group
    pub title: String,               // 私聊=对端昵称；群=群名
    pub avatar_cid: Option<String>,  // 头像 CID（私聊可空）
    pub peer: Option<String>,        // 私聊对端账户（脱敏后的展示 id）
    pub group_id: Option<u64>,
    pub last_message_preview: Option<String>, // 已解密的末条摘要（脱敏，可空）
    pub recency: i64,                // 排序键 = max(链上折算, 链下最后消息时间)
    pub unread: u32,                 // 真实未读 = 链下 MLS 未读 (+可选 System)
    pub pinned: bool,                // 私聊链上 OR 本地置顶偏好
    pub dnd: bool,                   // 免打扰（私聊链上 DND 或本地偏好；群=本地偏好）
    pub admin_muted: bool,           // 仅群：被管理员禁言（不能发言）；私聊恒 false
    pub archived: bool,
    pub frozen: bool,                // 仅群：治理冻结 → UI 只读态
    pub member_count: u32,           // 仅群
    pub my_role: GroupRole,          // Owner|Admin|Member|NA(私聊)
    pub presence: ConvPresence,      // OnChainOnly | OffChainOnly | Both（来源诊断/调试用）
}

pub enum ConvKind { Direct, Group }
pub enum GroupRole { Owner, Admin, Member, NA }
pub enum ConvPresence { OnChainOnly, OffChainOnly, Both }

/// EN: One message in a conversation timeline (decrypted, sanitized).
/// CN: 会话时间线中的一条消息（已解密、脱敏）。
pub struct MessageVM {
    pub client_msg_id: String,       // 客户端生成 id（引用/去重/状态用）
    pub conv_id: String,
    pub sender_ref: String,          // 展示用发送方引用（自己/对端/群成员）
    pub is_outgoing: bool,
    pub sent_at: i64,
    pub content: MessageContent,     // Text | Media | System | Reaction | ...
    pub reply_to: Option<String>,    // P3 引用回复
    pub mentions: Vec<String>,       // 群 @ 提及（展示引用）
    pub ephemeral_burn_at: Option<i64>, // 阅后即焚到期
    pub starred: bool,
    pub status: MsgStatus,           // Pending | Sent | Acked | Failed | Recalled
    pub source: MsgSource,           // OffChainMls | OnChainSystem
}

pub enum MessageContent {
    Text { text: String },
    Media {                          // 见 §7-E / 大文件规范信封
        mime: String, name: Option<String>, size: u64,
        thumb_ready: bool,           // 缩略图是否已就绪
        body_ready: bool,            // 本体是否已下载解密（冷层=false）
        duration_ms: Option<u64>,
    },
    System { kind: String },         // 订单/争议/治理通知
    Reaction { target: String, emoji: String },
}
pub enum MsgStatus { Pending, Sent, Acked, Failed, Recalled }
pub enum MsgSource { OffChainMls, OnChainSystem }

/// EN: Account/session summary returned by unlock/register.
pub struct AccountVM {
    pub account: String,
    pub nickname: Option<String>,
    pub chat_user_id: Option<u64>,   // core 分配的 11 位 id
    pub key_packages_available: u32, // 在册 KeyPackage 数（不足应补发）
    pub inbox_registered: bool,
    pub platform_muted: bool,        // 被治理平台级禁言（发送方将被拒）
}
```

### 3.1.4 `merge/spec.rs` 宪法测试 / Merge constitution tests

把 Merge Spec（§6）实现为**纯函数**，输入链上切片 + 本地状态，输出 `Vec<ConversationVM>`，
便于全覆盖单测。签名建议：

```rust
pub struct OnChainRow { /* 来自 chat_listConversations 的一行 */ }
pub struct LocalConv  { /* 本地 MLS/index/偏好：last_active, unread, pinned, dnd, ... */ }

/// 纯函数：无 I/O，可完全单测。
pub fn merge_conversations(
    on_chain: &[OnChainRow],
    local: &[LocalConv],
    now_block_to_time: impl Fn(u32) -> i64, // 链上 block → 近似时间
) -> Vec<ConversationVM>;
```

必须覆盖的"宪法"用例（每条都是历史上踩过的坑）：

| # | 用例 | 期望 |
|---|---|---|
| T1 | 纯链下私聊（链上无行，仅本地 MLS） | **出现在列表**，`presence=OffChainOnly`，`recency=本地` |
| T2 | 仅链上 System 私聊（无人类消息） | 作为"平台通知"卡片保留，`presence=OnChainOnly` |
| T3 | 同对端 System + 人类 MLS 两条来源 | 按对端**合并为一张卡片**（同一 `conv_id`），不重复 |
| T4 | 群 `last_active=0`（链上恒 0） | 用本地 `last_active` 排序；**不**沉底 |
| T5 | 私聊 `kind=direct` 的 `muted` | 解析为 `dnd=true, admin_muted=false` |
| T6 | 群 `kind=group` 的 `muted` | 解析为 `admin_muted=true`（dnd 由本地偏好独立决定） |
| T7 | App 角标 | = Σ `unread`（含链下），**≠** `total_direct_unread` |
| T8 | 置顶组排序 | `pinned`（链上私聊 OR 本地）组在前，组内按 `recency` 倒序 |
| T9 | 群 `frozen=true` | `ConversationVM.frozen=true`，UI 禁用成员变更入口 |
| T10 | 链上私聊 `pinned/archived` 与本地偏好冲突 | 按 §6 字段权威性表取权威方（私聊置顶/归档链上权威） |

> 这 10 条作为前端 CI 的**必过门槛**；任何 Merge 改动都先跑它们，杜绝"把链上切片当完整列表"
> 一类回归。

---

## 4. 链上接口清单 / On-chain surface to integrate

### 4.1 写（extrinsic）

**`chat-core`（私聊 / 资料 / 会话偏好）**
- 资料/隐私：`register_chat_user` / `update_chat_profile` / `set_user_status` /
  `update_privacy_settings`
- 会话偏好：`set_session_pinned` / `set_session_muted`（DND）/ `archive_session`
- System 通道：`mark_as_read` / `mark_batch_as_read` / `mark_session_as_read` /
  `delete_message` / `recall_message`
- ⚠️ `send_message` 已收窄为**仅 System**；前端**不要**用它发人类消息（返回 `HumanMessagesOffChain`）。

**`chat-group`（MLS DS/AS）**
- 身份：`publish_key_package` / `revoke_key_package`
- 生命周期：`create_group` / `disband_group` / `transfer_ownership` / `set_admin`
- 成员变更：`commit`（加/退/移）/ `request_join` / `cancel_join_request` / `approve_join` /
  `claim_welcome`
- 治理/资料：`set_group_profile` / `set_group_nickname` / `ban_member` / `unban_member` /
  `set_member_mute` / `set_group_mute_all` / `anchor_message_digest`

**`chat-permission`（权限 / 隐私 / 举报）**
- `set_permission_level` / `set_rejected_scene_types` / `update_privacy_settings`
- `bump_capability_epoch`（链下能力令牌"拉黑/整批失效"核弹）
- `report` / `resolve_report`（举报对象/理由为 IPFS CID，链上无明文）

**`chat-inbox`（投递信箱）**
- `register_inbox` / `bump_epoch` / `revoke_tag` / `unrevoke_tag` / `deregister_inbox` /
  `transfer_controller`

### 4.2 读（免费 `chat_*` RPC / `api.call.*`）

| 方法 | 用途 |
|---|---|
| `chat_listConversations(who, at?)` | 链上会话切片（私聊+群聊）；**非完整列表**，需 Merge |
| `chat_totalDirectUnread(who, at?)` | 链上 System 通道未读总数；**非** App 全局角标 |
| `chat_pendingWelcome(groupId, who, at?)` | 待领 Welcome（**先读后 claim**） |
| `chat_handshakeAtEpoch(groupId, epoch, at?)` | 指定 epoch 的 Commit 字节（离线补齐） |
| `chat_groupMlsSnapshot(groupId, at?)` | 群 MLS 锚点快照（epoch/hash/cid/count/cipher/public/frozen） |
| `chat_isGroupFrozen(groupId, at?)` | 群是否冻结（治理或拆除中） |
| `chat_checkPermission(sender, receiver, at?)` | 聊天权限检查 |
| `chat_getActiveScenes(user1, user2, at?)` | 两用户间场景授权 |
| `chat_capabilityEpoch(who, at?)` | 账户能力撤销纪元（令牌新鲜度比对） |
| `chat_isAccountMuted(who, at?)` | 账户是否被治理平台级禁言 |
| `chat_privacySummary(who, at?)` | 隐私设置摘要 |
| `chat_inboxEpoch(inboxId, at?)` / `chat_isTagRevoked(inboxId, tag, at?)` / `chat_inboxExists(inboxId, at?)` | 投递信箱撤销态（relay 校验面） |

---

## 5. 数据模型（前端本地，参考设备保留草案）

```sql
conversation(
  conv_id, kind/*direct|group*/, pinned BOOL,
  retention_days INT NULL, max_messages INT NULL, media_retention_days INT NULL
)

message(
  conv_id, msg_id/*client id*/, sent_at, sender_ref,
  type, body_cache NULL/*可空=已卸载*/, content_cid,
  reply_to NULL, ephemeral_burn_at NULL,
  starred BOOL, sync_state/*pending|sent|acked*/, last_access_at
)

media_blob(content_cid, conv_id, bytes_path NULL, thumb_path, size_bytes, last_access_at)
```

加密会话索引 blob（跨设备恢复，存 IPFS，`K_index` 加密；指针不上链）schema 见
`CHAT_P2_SESSION_ANCHOR_DESIGN.md` §2.1。

MLS payload 统一信封（reply/mention/reaction/forward/ephemeral/大文件 body）见
`CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md` §3 与 `CHAT_LARGE_FILE_SPEC.md` §3。

---

## 6. Merge 引擎（最易踩坑，务必照此实现）

`ConversationStore` 必须严格执行 README 的 Merge Spec，硬性约束：

1. **会话集合 = 链上切片 ∪ 链下来源**。链下来源 = 加密会话索引 blob（首选）∪ inbox 投递推导
   （兜底）∪ 本地 MLS 库。纯链下私聊**链上没有行**（隐私有意为之），以链下为准补入。
2. **`muted` 按 `kind` 分支渲染**：
   - `kind=direct` → 我自己的免打扰（DND），收不到提醒但**能发言**。
   - `kind=group` → 管理员**禁言**，我**不能发言**。
   - **绝不能共用一个 🔕 图标**。
3. **`unread` / `total_direct_unread` 只反映 System 通道**，**不能**直接做 App 角标。真实角标 =
   Σ 各会话合并后链下 MLS 未读。
4. **群在链上切片末尾且 `last_active=0`**，必须用链下 `last_active` 跨类型重排。
5. **排序键** `recency = max(链上 last_active 折算, 链下最后消息时间)`；置顶组在前。

> 建议把这套规则写成纯函数（输入：链上切片 + 本地状态；输出：渲染用会话列表）+ 完整单测，
> 作为前端的"宪法测试"。

---

## 7. 关键流程时序 / Critical flows

**A. 新成员入群 —— 先读后删（最易出 bug，见群集成文档 §5）**
```
chat_pendingWelcome(group_id, who)   // 只读，先取回
→ MlsEngine 本地处理 Welcome、建会话
→ chat_handshakeAtEpoch 逐 epoch 补齐
→ claim_welcome(group_id)            // 最后才删信箱
```
❌ 反模式：先 `claim_welcome` 再读 → Welcome 永久丢失。

**B. 建群 + 首次扩群**：建群后**第一次 commit 必须一次加 ≥2 人**（1→3），否则
`TwoMemberGroupForbidden`；被加人须先 `publish_key_package`，否则 `AddeeNotJoinable`；
`welcomes` 与 `member_delta.added` **一一对应**。

**C. 并发 commit 仲裁**：带 `expected_epoch`，收到 `EpochStale` → 重取 `chat_groupMlsSnapshot`
+ `HandshakeLog` 重建 → 重算重试。

**D. 发普通消息**：MlsEngine 加密 → RelayClient 用盲签令牌兑付 + sealed-sender → relay 投递。
**全程零链上交易**。

**E. 发大文件**（大文件规范 §9）：随机 `file_key` → 分块 AES-GCM → IPFS → 组 manifest →
缩略图 → MLS 信封只带 `{root_cid, file_key, thumb_cid, size, ...}`。

**F. 错误→动作映射**：实现群集成文档 §9 整张表（`EpochStale`/`WelcomeMismatch`/`GroupFrozen`/
`MustTransferFirst`/`Banned`/`RateLimited`…）。

**G. 换设备恢复**（P2 设计）：解锁主密钥 → 取并解密会话索引 blob → 还原列表与偏好 → inbox
投递推导兜底。链上零参与。

---

## 8. 分阶段路线图 / Phased roadmap

**Phase 0｜地基（2-3 周）**
- ChainClient 封装全部 `chat_*` RPC + extrinsic 类型；钱包/账户接入；KeyVault 主密钥派生。
- LocalStore schema。产出：能登录、读链上会话切片、更新资料/隐私。

**Phase 1｜私聊 MVP（4-6 周）**
- MlsEngine 接入 openmls，1:1 成对会话；RelayClient（盲签令牌可先打桩）。
- ConversationStore Merge 引擎 + 宪法测试。产出：两端加密文本收发，列表正确合并。

**Phase 2｜群聊（4-6 周）**
- 建群/入群（先读后删时序）/成员变更/epoch 补齐/群资料/角色/禁言/封禁/治理冻结只读态。
  产出：完整 MLS 群聊。

**Phase 3｜富媒体 + P3 交互（3-4 周）**
- 大文件分块/manifest/缩略图/IPFS Pin 分级；MLS 信封 reply/mention/reaction/forward/ephemeral
  （全部链下，按 P3 §3 信封）。

**Phase 4｜隐私 + 投递硬化（评审后）**
- RFC 9474 Blind RSA 盲签令牌全链路 + per-contact 标签 + epoch 撤销（**需先过密码学评审**，
  见投递设计 §13）；加密会话索引 blob 多设备合并（LWW → CRDT）；设备保留/清理、阅后即焚执行。

**Phase 5｜合规 + 打磨**
- 举报（`report`，理由 CID）、平台禁言态渲染、System 通知卡片、消息撤回 UI。

---

## 9. 主要风险与注意点 / Risks

1. **MLS 密码学正确性**：用成熟实现（openmls）；多端签发需门限或受控同步——**不要自造**。
2. **链上/链下边界误用**：最常见 bug 是把 `list_conversations` 当完整列表、把 System 未读当全局
   角标——Merge 引擎要挡住。
3. **盲签投递令牌**：属密码学协议，**先评审后落地**（投递设计标注"待评审"）；Phase 1 可先打桩。
4. **不可恢复是特性**：MLS 前向保密 + 阅后即焚下，部分历史**设计上就不该找回**，UI 要明确提示
   而非误导用户"还能找回"。
5. **隐私不变量**：1:1 不建链上群、不为私聊加链上锚点——前端不得反向请求链上开口子。

---

## 10. 交付边界 / Delivery boundary

- **链上侧（本仓库）**：本方案**不引入新 extrinsic / storage**；维持现有收敛方向。前端只消费
  §4 列出的既有接口。
- **链下侧（前端 + relay）**：MlsEngine / RelayClient / ConversationStore / LocalStore / KeyVault
  及全部 UI 按本方案与所引用设计文档实现。
- 如实现过程中确需链上原语，回到 `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md` §6 评审流程，不在本方案
  默认范围内开口子。
