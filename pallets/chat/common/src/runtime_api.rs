//! Runtime API: unified conversation view across private chat and group chat.
//! 统一会话视图 Runtime API：聚合私聊（`pallet-chat-core`）与群聊
//! （`pallet-chat-group`）为单一会话列表，供前端"消息"页直接渲染。
//!
//! EN: This API lives in `common` (which neither `core` nor `group` depend on
//! the other) precisely because the unified view spans BOTH pallets. The
//! aggregation logic is implemented in the runtime's `impl_runtime_apis!`,
//! where both `ChatCore` and `ChatGroup` are visible.
//!
//! CN: 该 API 定义在 `common`，因为统一视图需要同时跨越 `core` 与 `group` 两个
//! pallet；真正的聚合逻辑在 runtime 的 `impl_runtime_apis!` 中实现（那里能同时
//! 访问 `ChatCore` 与 `ChatGroup`）。
//!
//! # 链上 / 链下边界（重要 — 客户端必读）
//! EN: This API returns an ON-CHAIN SLICE only; it is NOT a complete message
//! list. Human chat (Text/Image/File/Voice — both 1:1 and group) is delivered
//! off-chain (MLS + relay-rs; ciphertext never touches the chain). The ONLY
//! on-chain messages are `System` notifications (order / dispute / governance),
//! and a direct session/`Session` row exists ONLY because a `System` message
//! was sent between the pair (see `pallet-chat-core::send_message`).
//! Concretely:
//! - Direct `unread` / `last_active`: count the **System-notification channel
//!   only**, NOT the pair's human chat. They are authoritative for System but
//!   are NOT the user's real conversation recency / unread.
//! - A pair that only ever chatted off-chain has NO direct row here at all.
//! - Group `unread` / `last_active`: always `0` (messages are off-chain).
//! - Authoritative on chain: direct pin/mute(DND)/archive flags, and group
//!   metadata (name / avatar / role / member count / admin mute).
//! Clients MUST merge this slice with their local/off-chain MLS state to render
//! the real "Messages" page (sorting, unread badges, conversation presence).
//! See the client Merge Spec in `pallets/chat/README.md`.
//!
//! CN: 本 API 仅返回**链上切片**，**不是完整消息列表**。人类聊天（Text/Image/File/
//! Voice，无论私聊还是群聊）走链下（MLS + 节点中继，密文不触链）。链上**唯一**的消息
//! 是 `System` 通知（订单/争议/治理）；一条私聊 `Session` 之所以存在，**仅**因为该对
//! 用户间发过 `System` 消息（见 `pallet-chat-core::send_message`）。具体：
//! - 私聊 `unread` / `last_active`：只统计 **System 通知通道**，**不含**该对用户的
//!   人类聊天；对 System 权威，但**不是**用户真实会话的活跃度/未读。
//! - 只在链下聊过天的用户对，这里**根本没有**对应私聊行。
//! - 群聊 `unread` / `last_active`：恒为 `0`（消息在链下）。
//! - 链上权威：私聊的置顶/免打扰(DND)/归档，以及群元数据（群名/头像/角色/成员数/
//!   管理员禁言）。
//! 客户端**必须**把本切片与本地/链下 MLS 状态合并，才能渲染真实「消息」页（排序、
//! 未读角标、会话是否存在）。合并规则见 `pallets/chat/README.md` 的客户端 Merge Spec。

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

/// EN: Conversation discriminator. CN: 会话类型判别。
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, Debug)]
pub enum ConversationKind {
    /// EN: 1:1 private session (`pallet-chat-core`). CN: 一对一私聊会话。
    Direct,
    /// EN: MLS group (`pallet-chat-group`). CN: MLS 群聊。
    Group,
}

/// EN: Group role tag mirrored as a stable `u8` so this DTO does not depend on
/// `pallet-chat-group` types. 0=Owner, 1=Admin, 2=Member, 255=N/A (direct or
/// non-member). CN: 群角色以稳定 `u8` 表达，避免本 DTO 依赖群聊 pallet 类型。
/// 0=群主，1=管理员，2=普通成员，255=不适用（私聊或非成员）。
pub mod role {
    pub const OWNER: u8 = 0;
    pub const ADMIN: u8 = 1;
    pub const MEMBER: u8 = 2;
    pub const NONE: u8 = 255;
}

/// EN: One row in the unified conversation list. Fields are populated according
/// to `kind`; cross-kind fields use neutral defaults (see per-field notes).
/// CN: 统一会话列表中的一行。字段按 `kind` 填充，跨类型字段取中性默认值。
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct ConversationSummary<AccountId, Hash, BlockNumber> {
    /// EN: Direct or Group. CN: 私聊或群聊。
    pub kind: ConversationKind,
    /// EN: Direct → Some(session hash); Group → None.
    /// CN: 私聊 → Some(会话哈希)；群聊 → None。
    pub direct_id: Option<Hash>,
    /// EN: Group → Some(group id); Direct → None.
    /// CN: 群聊 → Some(群 id)；私聊 → None。
    pub group_id: Option<u64>,
    /// EN: Direct → the other participant; Group → None.
    /// CN: 私聊 → 对端账户；群聊 → None。
    pub peer: Option<AccountId>,
    /// EN: Group display name (empty for direct).
    /// CN: 群名称（私聊为空）。
    pub name: Vec<u8>,
    /// EN: Group avatar IPFS CID (empty for direct / unset).
    /// CN: 群头像 IPFS CID（私聊或未设置为空）。
    pub avatar_cid: Vec<u8>,
    /// EN: Direct → last-active block of the **System-notification** session
    /// (NOT human chat); Group → 0 (recency is off-chain). Merge with off-chain
    /// MLS recency for real ordering.
    /// CN: 私聊 → **System 通知**会话的最后活跃区块（**非**人类聊天）；群聊 → 0
    /// （活跃度在链下）。真实排序需与链下 MLS 活跃度合并。
    pub last_active: BlockNumber,
    /// EN: Direct → unread count of the **System-notification** channel only
    /// (NOT human chat); Group → 0 (messages are off-chain). App total unread
    /// MUST add the client's off-chain MLS unread.
    /// CN: 私聊 → 仅 **System 通知**通道的未读数（**非**人类聊天）；群聊 → 0
    /// （消息在链下）。App 总未读**必须**叠加客户端链下 MLS 未读。
    pub unread: u32,
    /// EN: Caller pinned this conversation (direct only; group pin is client-side).
    /// CN: 调用者是否置顶（仅私聊；群置顶为客户端侧）。
    pub pinned: bool,
    /// EN: SEMANTICS DIFFER BY `kind` — the client MUST branch on `kind`, never
    /// render a single "muted" icon for both. Direct → caller's own DND flag
    /// (you won't be notified). Group → admin-imposed mute (`is_member_muted`):
    /// you CANNOT send, regardless of your notification preference.
    /// CN: 语义**按 `kind` 不同**——客户端**必须**按 `kind` 分支，切勿对两者渲染同一个
    /// 「静音」图标。私聊 → 调用者自己的免打扰(DND)（你收不到提醒）；群聊 → 管理员强制
    /// 禁言（`is_member_muted`）：无论你的提醒偏好如何，你都**不能发言**。
    pub muted: bool,
    /// EN: Direct → archived flag; Group → false.
    /// CN: 私聊 → 是否归档；群聊 → false。
    pub archived: bool,
    /// EN: Group member count (0 for direct).
    /// CN: 群成员数（私聊为 0）。
    pub member_count: u32,
    /// EN: Group role tag (see `role`); 255 for direct / non-member.
    /// CN: 群角色标记（见 `role`）；私聊或非成员为 255。
    pub group_role: u8,
}

/// EN: Maximum rows returned by `list_conversations` (direct + group combined).
/// Protects RPC/runtime from unbounded reads on heavy accounts.
/// CN: `list_conversations` 返回的最大行数（私聊 + 群合计），防止大账户无界读取。
pub const MAX_CONVERSATIONS_API: usize = 512;

sp_api::decl_runtime_apis! {
    /// EN: Unified chat view API. CN: 统一聊天视图 API。
    pub trait ChatViewApi<AccountId, Hash, BlockNumber>
    where
        AccountId: Codec,
        Hash: Codec,
        BlockNumber: Codec,
    {
        /// EN: List the ON-CHAIN conversation slice for `who` — direct rows
        /// (which exist only because of `System` messages) pre-sorted by core
        /// (pinned first, then last-active desc), followed by groups sorted by
        /// `group_id` ascending (a deterministic, pageable baseline; real
        /// recency is off-chain) with `last_active`/`unread` = 0. This is NOT a
        /// complete message list: human chat (1:1 and group) lives off-chain.
        /// Clients MUST merge with local/off-chain MLS state for the real list
        /// and cross-kind sorting (see Merge Spec in `pallets/chat/README.md`).
        /// CN: 列出 `who` 的**链上会话切片**——私聊行（仅因 `System` 消息而存在）按 core
        /// 排序（置顶优先，再按最后活跃倒序）在前，群聊按 `group_id` **升序**在后（确定、
        /// 可分页的基线；真实活跃度在链下）且 `last_active`/`unread` 为 0。这**不是**完整
        /// 消息列表：人类聊天（私聊与群聊）在链下。客户端**必须**与本地/链下 MLS 状态合并，
        /// 才能得到真实列表与跨类型排序（见 `pallets/chat/README.md` 的 Merge Spec）。
        fn list_conversations(
            who: AccountId,
        ) -> Vec<ConversationSummary<AccountId, Hash, BlockNumber>>;

        /// EN: Total unread of the on-chain **System-notification** channel
        /// across all direct sessions. This is NOT the app's global unread
        /// badge — it excludes all off-chain human chat (1:1 and group). The
        /// client MUST add its off-chain MLS unread for the real total.
        /// CN: 全部私聊会话上、链上 **System 通知**通道的未读总数。这**不是** App 全局
        /// 未读角标——它**不含**任何链下人类聊天（私聊与群聊）。真实总数需客户端叠加
        /// 链下 MLS 未读。
        fn total_direct_unread(who: AccountId) -> u32;
    }
}
