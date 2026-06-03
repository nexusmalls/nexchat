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
//! # 链上 / 链下边界（重要）
//! EN: Private (1:1) conversations keep their unread count, last-active block,
//! pin and mute (DND) flags ON CHAIN, so those fields are authoritative here.
//! Group messages are delivered off-chain (MLS + node relay; ciphertext never
//! touches the chain), so a group's `unread` and `last_active` CANNOT be known
//! on chain — they are reported as `0` and MUST be merged by the client from
//! its local/off-chain state. Only group metadata (name / avatar / role /
//! member count / admin mute) is authoritative on chain.
//!
//! CN: 私聊（1:1）的未读数、最后活跃区块、置顶与免打扰均在链上，故此处权威；
//! 群聊消息走链下（MLS + 节点中继，密文不触链），因此群的 `unread` 与
//! `last_active` 链上无从得知——一律返回 `0`，需由客户端用本地/链下状态合并。
//! 链上权威的仅为群元数据（群名 / 头像 / 角色 / 成员数 / 管理员禁言）。

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
    /// EN: Direct → session.last_active block; Group → 0 (recency is off-chain).
    /// CN: 私聊 → 会话最后活跃区块；群聊 → 0（活跃度在链下）。
    pub last_active: BlockNumber,
    /// EN: Direct → on-chain unread count; Group → 0 (messages are off-chain).
    /// CN: 私聊 → 链上未读数；群聊 → 0（消息在链下）。
    pub unread: u32,
    /// EN: Caller pinned this conversation (direct only; group pin is client-side).
    /// CN: 调用者是否置顶（仅私聊；群置顶为客户端侧）。
    pub pinned: bool,
    /// EN: Direct → caller's DND flag; Group → muted by admin (`is_member_muted`).
    /// CN: 私聊 → 调用者免打扰；群聊 → 是否被管理员禁言。
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

sp_api::decl_runtime_apis! {
    /// EN: Unified chat view API. CN: 统一聊天视图 API。
    pub trait ChatViewApi<AccountId, Hash, BlockNumber>
    where
        AccountId: Codec,
        Hash: Codec,
        BlockNumber: Codec,
    {
        /// EN: List all conversations (private + group) for `who`, with direct
        /// sessions pre-sorted (pinned first, then last-active desc) as
        /// returned by `pallet-chat-core`, followed by groups. Group recency /
        /// unread are NOT on chain — clients must merge their off-chain state.
        /// CN: 列出 `who` 的全部会话（私聊 + 群聊）。私聊部分按 core 的排序
        /// （置顶优先，再按最后活跃倒序）在前，群聊在后。群的活跃度/未读不在链上，
        /// 需客户端用链下状态合并。
        fn list_conversations(
            who: AccountId,
        ) -> Vec<ConversationSummary<AccountId, Hash, BlockNumber>>;

        /// EN: Total unread across all private sessions (on-chain authoritative).
        /// Group unread is excluded by design (off-chain).
        /// CN: 全部私聊会话的未读总数（链上权威）。群聊未读按设计不计入（链下）。
        fn total_direct_unread(who: AccountId) -> u32;
    }
}
