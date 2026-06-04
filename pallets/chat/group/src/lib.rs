#![cfg_attr(not(feature = "std"), no_std)]

//! # pallet-chat-group — MLS (RFC 9420) group chat anchor
//! # pallet-chat-group — MLS（RFC 9420）群聊锚定
//!
//! EN: The chain acts ONLY as the MLS Delivery Service (DS) + Authentication
//! Service (AS): it provides global ordering for **identity (KeyPackages)** and
//! **membership changes (Commits / epoch advance)**, and a short-lived mailbox
//! for **Welcome** messages. All cryptography (TreeKEM, HPKE, encrypt/decrypt)
//! runs in the client (OpenMLS). The chain stores only opaque blobs and never
//! decrypts. Group **message ciphertext does NOT touch the chain** — it is
//! delivered off-chain via node broadcast (see design doc §13). An optional,
//! off-by-default message-digest anchor exists only for strong-audit scenarios.
//!
//! CN: 本链只承担 MLS 的投递服务（DS）+ 认证服务（AS）：为「身份（KeyPackage）」
//! 与「成员变更（Commit / epoch 推进）」提供全局定序，并为「Welcome」提供短期邮箱。
//! 全部密码学（TreeKEM、HPKE、加解密）在客户端（OpenMLS）完成；链只存不透明字节、
//! 绝不解密。群**消息密文不走链**——经节点广播离链投递（见设计文档 §13）。仅在强审计
//! 场景下，才启用默认关闭的消息 digest 锚。
//!
//! Core anti-fork mechanism / 核心防分叉机制：`commit(expected_epoch)` —— see §7.
//!
//! ## 1:1 私聊不建链上群 / 1:1 DMs MUST NOT create on-chain groups
//!
//! EN: Privacy invariant (C-plan finalized): one-to-one direct messages MUST NOT
//! be modeled as a 2-member on-chain group here. Creating an on-chain group
//! publishes membership (`who ↔ whom`) and would re-expose the communication
//! relationship this design removes. 1:1 DMs use the off-chain path only: a
//! receiver-signed chat capability token (gated by `pallet-chat-permission`'s
//! `CapabilityEpoch`) + a pairwise MLS session delivered via relay; no
//! `create_group` / membership row is written on-chain. On-chain groups are for
//! genuine multi-party (3+) rooms. CN: 隐私不变量（C 方案定稿）：一对一私聊**禁止**
//! 在此建成 2 人链上群。建链上群会公开成员关系（谁↔谁），重新暴露本设计意在隐藏的
//! 通信关系。1:1 私聊仅走链下：由接收方签名的聊天能力令牌（受 `pallet-chat-permission`
//! 的 `CapabilityEpoch` 约束）+ 经 relay 投递的成对 MLS 会话；链上不写 `create_group`/
//! 成员记录。链上群仅用于真正的多人（3+）房间。
//!
//! ## 群成员公开性 / Group membership is public (audit P3, inherent trade-off)
//!
//! EN: For genuine multi-party groups, membership (`GroupMembers`, `UserGroups`)
//! and roles are stored on-chain in clear. This is **inherent** to the DS+AS
//! role: the chain must know the member set to globally order Commits, enforce
//! `MaxGroupMembers`, route Welcomes, and prevent forks. It is an **accepted**
//! trade-off, scoped by the 1:1 invariant above (the most sensitive case — who
//! DMs whom — never becomes an on-chain group). What stays private even for
//! groups: message *content* (off-chain MLS E2EE; ciphertext never touches the
//! chain). Product guidance: model a cohort as an on-chain group only when its
//! membership being publicly visible is acceptable; for relationship-sensitive
//! cohorts, prefer the off-chain pairwise path. Hiding multi-party membership
//! itself would require a different primitive (e.g. anonymous-credential groups)
//! and is out of scope for the MLS DS+AS anchor.
//! CN: 对真正的多人群，成员关系（`GroupMembers`、`UserGroups`）与角色以明文存于链上。
//! 这是 DS+AS 角色的**固有**属性：链必须知道成员集合，才能为 Commit 全局定序、强制
//! `MaxGroupMembers`、路由 Welcome 并防分叉。这是**可接受**的权衡，并由上文 1:1 不变量
//! 收口（最敏感的「谁私聊谁」永不成为链上群）。即便是群，仍保持私密的是：消息**内容**
//! （链下 MLS 端到端加密，密文不触链）。产品建议：仅当群成员公开可见可接受时才用链上群；
//! 关系敏感的群体优先走链下成对路径。隐藏多人成员关系本身需另一种原语（如匿名凭证群），
//! 不在 MLS DS+AS 锚的范围内。

pub use pallet::*;
pub use weights::WeightInfo;

pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::DecodeWithMemTracking;
use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ReservableCurrency},
};
use frame_system::pallet_prelude::*;
use pallet_chat_common::rate_limit::{check_and_update_rate_limit, RateLimitResult, RateLimitState};
use sp_runtime::traits::{Saturating, UniqueSaturatedInto, Zero};
use sp_std::vec::Vec;

/// EN: Balance type of the configured reservable currency.
/// CN: 所配置可预留货币的余额类型。
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// EN: Group identifier (monotonically increasing; un-guessability is enforced
/// by the permission layer, not by ID entropy).
/// CN: 群 ID（自增；防猜测靠权限层，不靠 ID 熵）。
pub type GroupId = u64;

/// EN: Per-account KeyPackage identifier.
/// CN: 账户内 KeyPackage 标识。
pub type KeyPackageId = u64;

/// EN: Max storage items removed per `disband` call, per prefix. Bounds the
/// extrinsic's worst-case weight against unbounded per-group prefixes
/// (`HandshakeLog` / `MessageDigestAnchor` / `Banned` …); a large group needs a
/// few repeated `disband` calls (audit B4). CN: 单次 `disband` 每个前缀最多移除的
/// 存储项数，用于约束最坏权重以对抗无上界的群前缀（`HandshakeLog` /
/// `MessageDigestAnchor` / `Banned` 等）；大群需重复调用几次（审计 B4）。
pub const MAX_DISBAND_ITEMS_PER_CALL: u32 = 128;

/// EN: Application-level member role on top of MLS (MLS itself is flat).
/// CN: MLS 之上的应用层成员角色（MLS 本身无角色）。
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

/// EN: Member view stored on chain (no key material).
/// CN: 链上成员视图（不含任何密钥材料）。
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct GroupMember {
    pub role: MemberRole,
    /// EN: epoch at which the member joined / CN: 入群时的 epoch
    pub joined_epoch: u64,
    /// EN: unix-ish block number when joined / CN: 入群区块号
    pub joined_at: u32,
}

/// EN: On-chain MLS state anchor — contains NO secrets.
/// CN: 群的 MLS 状态锚点——不含任何机密。
#[derive(CloneNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, DebugNoBound)]
#[scale_info(skip_type_params(T))]
pub struct MlsGroupState<T: Config> {
    /// EN: monotonic epoch, prevents fork/replay / CN: 单调 epoch，防分叉/重放
    pub epoch: u64,
    /// EN: current ratchet-tree hash (client verifies) / CN: 当前棘轮树哈希
    pub tree_hash: [u8; 32],
    /// EN: MLS confirmed transcript hash anchor / CN: MLS transcript 锚点
    pub confirmed_transcript_hash: [u8; 32],
    /// EN: IPFS CID of external GroupInfo (audit/external-commit)
    /// CN: 外部 GroupInfo 的 IPFS CID（审计/外部 commit 用）
    pub group_info_cid: BoundedVec<u8, T::MaxCidLen>,
    /// EN: app-level owner / CN: 应用层群主
    pub admin: T::AccountId,
    pub member_count: u32,
    /// EN: ciphersuite tag (chain does not negotiate) / CN: 套件标识（链不协商）
    pub cipher_suite: u16,
    /// EN: public group (anyone may be added) vs private (admin approval)
    /// CN: 公开群（可直接 Add）/ 私群（管理员审批）
    pub is_public: bool,
}

/// EN: App-layer group display profile (name / avatar / announcement). This is
/// NOT part of MLS and carries no key material; it is plaintext metadata that
/// every member needs a consistent view of, so it is anchored on chain. The
/// avatar is stored as an IPFS CID; name and announcement are raw bytes.
/// CN: 应用层群展示资料（群名 / 头像 / 公告）。**不属于 MLS**、不含密钥材料，是
/// 所有成员都需要一致视图的明文元数据，故上链锚定。头像存 IPFS CID，群名与公告
/// 存原始字节。
#[derive(CloneNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, DebugNoBound)]
#[scale_info(skip_type_params(T))]
pub struct GroupProfile<T: Config> {
    /// EN: Display name / CN: 群名称
    pub name: BoundedVec<u8, T::MaxGroupNameLen>,
    /// EN: Avatar IPFS CID (empty = unset) / CN: 群头像 IPFS CID（空表示未设置）
    pub avatar_cid: BoundedVec<u8, T::MaxCidLen>,
    /// EN: Announcement / pinned notice / CN: 群公告
    pub announcement: BoundedVec<u8, T::MaxGroupAnnouncementLen>,
}

impl<T: Config> Default for GroupProfile<T> {
    fn default() -> Self {
        Self {
            name: BoundedVec::default(),
            avatar_cid: BoundedVec::default(),
            announcement: BoundedVec::default(),
        }
    }
}

/// EN: Membership delta carried by a `commit`, kept consistent with the MLS
/// Commit body. The chain only maintains the app-level member table from it.
/// CN: `commit` 携带的成员增减，与 MLS Commit 内容语义一致；链据此维护应用层成员表。
#[derive(
    CloneNoBound,
    PartialEqNoBound,
    EqNoBound,
    Encode,
    Decode,
    DecodeWithMemTracking,
    TypeInfo,
    MaxEncodedLen,
    DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct MemberDelta<T: Config> {
    pub added: BoundedVec<T::AccountId, T::MaxGroupMembers>,
    pub removed: BoundedVec<T::AccountId, T::MaxGroupMembers>,
}

impl<T: Config> Default for MemberDelta<T> {
    fn default() -> Self {
        Self { added: BoundedVec::default(), removed: BoundedVec::default() }
    }
}

/// EN: Decoupled hook mirroring group membership into an external
/// authorization layer (e.g. `chat-permission` scene authorization for optional
/// 1:1 DM rights). Kept O(1) per event by relating each member to a single
/// counterparty (the group owner) rather than the full O(N²) mesh. No-op by
/// default so the pallet stays standalone.
///
/// CN: 解耦钩子：把群成员关系镜像到外部授权层（如 `chat-permission` 的场景授权，
/// 用于可选的 1:1 私聊权限）。为保持每事件 O(1)，仅将成员与单一对手方（群主）关联，
/// 而非 O(N²) 全网格。默认空实现，使本 pallet 可独立运行。
pub trait GroupChatHook<AccountId> {
    /// EN: A member joined; `counterparty` is the group owner.
    /// CN: 成员加入；`counterparty` 为群主。
    fn on_member_added(group_id: GroupId, member: &AccountId, counterparty: &AccountId);
    /// EN: A member left/was removed; `counterparty` is the group owner.
    /// CN: 成员离开/被移除；`counterparty` 为群主。
    fn on_member_removed(group_id: GroupId, member: &AccountId, counterparty: &AccountId);
}

impl<AccountId> GroupChatHook<AccountId> for () {
    fn on_member_added(_: GroupId, _: &AccountId, _: &AccountId) {}
    fn on_member_removed(_: GroupId, _: &AccountId, _: &AccountId) {}
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// EN: Runtime event / CN: 运行时事件
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// EN: Reservable currency used for anti-spam deposits.
        /// CN: 用于防滥用押金的可预留货币。
        type Currency: ReservableCurrency<Self::AccountId>;

        /// EN: Deposit reserved on group creation, returned on disband.
        /// CN: 建群预留押金，解散时退还。
        #[pallet::constant]
        type GroupDeposit: Get<BalanceOf<Self>>;

        /// EN: Deposit reserved per published KeyPackage, returned on revoke.
        /// CN: 每个已发布 KeyPackage 的预留押金，吊销时退还。
        #[pallet::constant]
        type KeyPackageDeposit: Get<BalanceOf<Self>>;

        /// EN: Max pending join requests per group (anti-spam).
        /// CN: 单群最大待批入群申请数（防滥用）。
        #[pallet::constant]
        type MaxPendingJoins: Get<u32>;

        /// EN: Hook to mirror membership into an external authorization layer
        /// (e.g. chat-permission). Defaults to no-op `()`.
        /// CN: 把成员关系镜像到外部授权层（如 chat-permission）的钩子，默认空实现 `()`。
        type ChatHook: GroupChatHook<Self::AccountId>;

        /// EN: Max members per group / CN: 单群最大成员数
        #[pallet::constant]
        type MaxGroupMembers: Get<u32>;

        /// EN: Max groups a single account can belong to / CN: 单账户最大群数
        #[pallet::constant]
        type MaxGroupsPerUser: Get<u32>;

        /// EN: Max KeyPackage blob length / CN: KeyPackage 字节上限
        #[pallet::constant]
        type MaxKeyPackageLen: Get<u32>;

        /// EN: Max handshake (Commit) blob length / CN: 握手（Commit）字节上限
        #[pallet::constant]
        type MaxHandshakeLen: Get<u32>;

        /// EN: Max Welcome blob length / CN: Welcome 字节上限
        #[pallet::constant]
        type MaxWelcomeLen: Get<u32>;

        /// EN: Max GroupInfo CID length / CN: GroupInfo CID 上限
        #[pallet::constant]
        type MaxCidLen: Get<u32>;

        /// EN: Max group display-name length / CN: 群名称字节上限
        #[pallet::constant]
        type MaxGroupNameLen: Get<u32>;

        /// EN: Max group announcement length / CN: 群公告字节上限
        #[pallet::constant]
        type MaxGroupAnnouncementLen: Get<u32>;

        /// EN: Max per-member in-group nickname length / CN: 群内昵称（群名片）字节上限
        #[pallet::constant]
        type MaxGroupNicknameLen: Get<u32>;

        /// EN: Max KeyPackages a single account may keep published.
        /// CN: 单账户可挂载的 KeyPackage 上限。
        #[pallet::constant]
        type MaxKeyPackagesPerUser: Get<u32>;

        /// EN: Cooldown (in blocks) between group creations per account.
        /// CN: 单账户两次建群之间的冷却（区块数）。
        #[pallet::constant]
        type GroupCreationCooldown: Get<BlockNumberFor<Self>>;

        /// EN: Rate-limit window (in blocks) for write-heavy MLS actions
        /// (`commit` / `anchor_message_digest`), per account. Together with
        /// `MaxMlsActionsPerWindow` this caps how often one account can advance
        /// epochs / anchor digests, bounding `HandshakeLog` / `MessageDigestAnchor`
        /// growth (anti-spam). CN: 写入型 MLS 操作（`commit` / `anchor_message_digest`）
        /// 的限频窗口（区块数，按账户）。与 `MaxMlsActionsPerWindow` 一起限制单账户推进
        /// epoch / 锚定 digest 的频率，约束 `HandshakeLog` / `MessageDigestAnchor` 增长（防滥用）。
        #[pallet::constant]
        type MlsActionWindow: Get<BlockNumberFor<Self>>;

        /// EN: Max write-heavy MLS actions (`commit` + `anchor_message_digest`)
        /// per account within `MlsActionWindow`. CN: 单账户在 `MlsActionWindow`
        /// 窗口内允许的写入型 MLS 操作（`commit` + `anchor_message_digest`）上限。
        #[pallet::constant]
        type MaxMlsActionsPerWindow: Get<u32>;

        /// EN: Privileged origin (Root / governance) allowed to force-disband or
        /// freeze a group for platform compliance. Distinct from the in-group
        /// owner/admin authority. CN: 平台合规用的特权来源（Root / 治理），可强制解散
        /// 或冻结群；区别于群内群主/管理员权限。
        type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// EN: Weight info / CN: 权重信息
        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    // ---------------------------------------------------------------------
    // Storage / 存储
    // ---------------------------------------------------------------------

    /// EN: Published MLS KeyPackages (one-shot pre-keys). Consumed on Add.
    /// CN: 成员发布的 MLS KeyPackage（一次性预共享公钥包），被 Add 消费即删。
    #[pallet::storage]
    pub type KeyPackages<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Twox64Concat,
        KeyPackageId,
        BoundedVec<u8, T::MaxKeyPackageLen>,
    >;

    /// EN: Next KeyPackage id per account / CN: 每账户下一个 KeyPackage 自增 id
    #[pallet::storage]
    pub type NextKeyPackageId<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, KeyPackageId, ValueQuery>;

    /// EN: Per-account count of live KeyPackages / CN: 每账户在册 KeyPackage 计数
    #[pallet::storage]
    pub type KeyPackageCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// EN: Group MLS state anchor / CN: 群 MLS 状态锚点
    #[pallet::storage]
    pub type GroupMls<T: Config> = StorageMap<_, Blake2_128Concat, GroupId, MlsGroupState<T>>;

    /// EN: Next group id (auto-increment) / CN: 下一个群 ID（自增）
    #[pallet::storage]
    pub type NextGroupId<T: Config> = StorageValue<_, GroupId, ValueQuery>;

    /// EN: Handshake log: one merged Commit blob per epoch (epoch advances by 1
    /// per commit). Lets offline members catch up; opaque to the chain.
    /// CN: 握手日志：每个 epoch 恰好一条合并后的 Commit 字节（每次 commit epoch +1）。
    /// 供离线成员补齐；对链不透明。
    #[pallet::storage]
    pub type HandshakeLog<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Twox64Concat,
        u64, // epoch
        BoundedVec<u8, T::MaxHandshakeLen>,
    >;

    /// EN: Welcome mailbox for newly added members; deleted on claim.
    /// CN: 新成员 Welcome 邮箱；领取即删。
    #[pallet::storage]
    pub type WelcomeMailbox<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u8, T::MaxWelcomeLen>,
    >;

    /// EN: Application-level member table (no key material).
    /// CN: 应用层成员表（不含密钥材料）。
    #[pallet::storage]
    pub type GroupMembers<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        GroupMember,
    >;

    /// EN: User's group list (kept in sync on every join/leave/disband to avoid
    /// ghost entries). / CN: 用户群列表（每次进/退/解散同步维护，杜绝幽灵群）。
    #[pallet::storage]
    pub type UserGroups<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<GroupId, T::MaxGroupsPerUser>,
        ValueQuery,
    >;

    /// EN: Last block at which an account created a group (cooldown).
    /// CN: 账户上次建群区块（冷却用）。
    #[pallet::storage]
    pub type LastGroupCreation<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, ValueQuery>;

    /// EN: Per-account windowed rate-limit state for write-heavy MLS actions
    /// (`commit` / `anchor_message_digest`). Self-resetting once the window
    /// elapses; not group-scoped, so it throttles a spammer across all groups.
    /// CN: 写入型 MLS 操作（`commit` / `anchor_message_digest`）的按账户窗口限频状态。
    /// 窗口过期自动重置；非按群维度，故可跨群限制同一滥用者。
    #[pallet::storage]
    pub type MlsActionRate<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, RateLimitState<BlockNumberFor<T>>, ValueQuery>;

    /// EN: Groups frozen by governance for compliance. A frozen group blocks
    /// `commit` / `anchor_message_digest` / `request_join` until unfrozen (its
    /// metadata stays readable). CN: 被治理冻结的群（合规）。冻结期间禁止
    /// `commit` / `anchor_message_digest` / `request_join`，解冻前元数据仍可读。
    #[pallet::storage]
    pub type GroupFrozen<T: Config> = StorageMap<_, Blake2_128Concat, GroupId, (), OptionQuery>;

    /// EN: (depositor, amount) reserved on group creation; refunded on disband.
    /// CN: 建群时预留的（押金人，金额）；解散时退还。
    #[pallet::storage]
    pub type GroupDepositOf<T: Config> =
        StorageMap<_, Blake2_128Concat, GroupId, (T::AccountId, BalanceOf<T>)>;

    /// EN: Pending join requests for private groups (block when requested).
    /// CN: 私群待批入群申请（记录申请区块）。
    #[pallet::storage]
    pub type JoinRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
    >;

    /// EN: Count of pending join requests per group (bounds `JoinRequests`).
    /// CN: 单群待批申请计数（约束 `JoinRequests`）。
    #[pallet::storage]
    pub type PendingJoinCount<T: Config> =
        StorageMap<_, Blake2_128Concat, GroupId, u32, ValueQuery>;

    /// EN: Admin-approved join authorizations; consumed by the Add `commit`.
    /// CN: 管理员批准的入群授权；由 Add `commit` 消费。
    #[pallet::storage]
    pub type JoinApprovals<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
    >;

    /// EN: OPTIONAL, off by default. Strong-audit message-batch digest anchor.
    /// Stores only a digest — never CID/plaintext/per-message data.
    /// CN: 【可选，默认关闭】强审计场景的消息批次 digest 锚；只存指纹，
    /// 绝不含 CID/明文/逐条消息。
    #[pallet::storage]
    pub type MessageDigestAnchor<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Twox64Concat,
        u64, // batch_seq
        ([u8; 32], u64, BlockNumberFor<T>),
    >;

    /// EN: App-layer group display profile (name / avatar / announcement).
    /// CN: 应用层群展示资料（群名 / 头像 / 公告）。
    #[pallet::storage]
    pub type GroupProfiles<T: Config> = StorageMap<_, Blake2_128Concat, GroupId, GroupProfile<T>>;

    /// EN: Per-member in-group nickname (group business card). Empty/absent = use
    /// the member's global profile name. / CN: 群内昵称（群名片），缺省时回退全局昵称。
    #[pallet::storage]
    pub type GroupNicknames<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u8, T::MaxGroupNicknameLen>,
    >;

    /// EN: Group ban list — banned accounts cannot request to join nor be added
    /// via `commit`. This is enforced on chain (unlike mute, which is advisory).
    /// Value = block at which the ban was set. CN: 群封禁名单——被封禁账户既不能
    /// 申请入群也不能经 `commit` 被加入；该约束**链上强制**（与仅作策略的禁言不同）。
    /// 值为封禁区块号。
    #[pallet::storage]
    pub type Banned<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
    >;

    /// EN: Per-member mute expiry. A member is muted while `block_number < value`.
    /// Mute is an APP-LAYER POLICY: since message ciphertext never touches the
    /// chain, enforcement is done by clients / relay nodes reading this state;
    /// the chain only provides the single source of truth + events.
    /// CN: 成员禁言到期区块。当前区块 < 该值即处于禁言。禁言是**应用层策略**：
    /// 消息密文不上链，故由客户端 / 中继节点读取此状态执行；链仅提供单一事实来源与事件。
    #[pallet::storage]
    pub type MemberMutedUntil<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        GroupId,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
    >;

    /// EN: Group-wide mute-all flag (only owner/admin may speak). APP-LAYER POLICY,
    /// enforced off-chain like per-member mute. / CN: 全员禁言开关（仅群主/管理员可发言）。
    /// 与单人禁言一样属应用层策略，由链下执行。
    #[pallet::storage]
    pub type GroupMutedAll<T: Config> = StorageMap<_, Blake2_128Concat, GroupId, bool, ValueQuery>;

    // ---------------------------------------------------------------------
    // Events / 事件
    // ---------------------------------------------------------------------

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// EN: KeyPackage published / CN: KeyPackage 已发布
        KeyPackagePublished { who: T::AccountId, id: KeyPackageId },
        /// EN: KeyPackage revoked / CN: KeyPackage 已吊销
        KeyPackageRevoked { who: T::AccountId, id: KeyPackageId },
        /// EN: Group created (epoch 0) / CN: 群已创建（epoch 0）
        GroupCreated { group_id: GroupId, creator: T::AccountId, epoch: u64 },
        /// EN: Membership commit applied, epoch advanced / CN: 成员 Commit 已应用，epoch 推进
        Committed { group_id: GroupId, epoch: u64, committer: T::AccountId },
        /// EN: Member added by a commit / CN: Commit 加入成员
        MemberJoined { group_id: GroupId, member: T::AccountId, epoch: u64 },
        /// EN: Member removed by a commit / CN: Commit 移除成员
        MemberRemoved { group_id: GroupId, member: T::AccountId, epoch: u64 },
        /// EN: Welcome claimed / CN: Welcome 已领取
        WelcomeClaimed { group_id: GroupId, who: T::AccountId },
        /// EN: Group disbanded / CN: 群已解散
        GroupDisbanded { group_id: GroupId },
        /// EN: A disband call made bounded progress but the group is not yet fully
        /// torn down (large group); call disband again to continue (audit B4).
        /// CN: 一次解散调用按预算推进，但群尚未完全拆除（大群）；再次调用以继续（审计 B4）。
        GroupDisbandProgress { group_id: GroupId },
        /// EN: Message-batch digest anchored (optional audit) / CN: 消息批次 digest 已锚（可选审计）
        MessageDigestAnchored { group_id: GroupId, batch_seq: u64, epoch: u64 },
        /// EN: Join requested (private group) / CN: 已申请入群（私群）
        JoinRequested { group_id: GroupId, who: T::AccountId },
        /// EN: Join request cancelled by applicant / CN: 申请人撤回入群申请
        JoinRequestCancelled { group_id: GroupId, who: T::AccountId },
        /// EN: Join approved by owner/admin (awaits Add commit) / CN: 群主/管理员已批准（待 Add commit）
        JoinApproved { group_id: GroupId, who: T::AccountId, by: T::AccountId },
        /// EN: Ownership transferred / CN: 群主已转让
        OwnershipTransferred { group_id: GroupId, from: T::AccountId, to: T::AccountId },
        /// EN: Admin role set/unset / CN: 管理员角色设/撤
        AdminSet { group_id: GroupId, who: T::AccountId, on: bool },
        /// EN: Group display profile updated / CN: 群展示资料已更新
        GroupProfileUpdated { group_id: GroupId, by: T::AccountId },
        /// EN: A member set their in-group nickname / CN: 成员设置了群内昵称
        MemberNicknameSet { group_id: GroupId, who: T::AccountId },
        /// EN: A member was banned (owner/admin) / CN: 成员已被封禁（群主/管理员）
        MemberBanned { group_id: GroupId, who: T::AccountId, by: T::AccountId },
        /// EN: A member was unbanned / CN: 成员已被解封
        MemberUnbanned { group_id: GroupId, who: T::AccountId, by: T::AccountId },
        /// EN: A member was muted until `until` block / CN: 成员被禁言至 `until` 区块
        MemberMuted { group_id: GroupId, who: T::AccountId, until: BlockNumberFor<T>, by: T::AccountId },
        /// EN: A member was unmuted / CN: 成员已被解除禁言
        MemberUnmuted { group_id: GroupId, who: T::AccountId, by: T::AccountId },
        /// EN: Group-wide mute-all toggled / CN: 全员禁言开关已切换
        GroupMuteAllSet { group_id: GroupId, on: bool, by: T::AccountId },
        /// EN: A group was force-disbanded by governance / CN: 群被治理强制解散
        GroupForceDisbanded { group_id: GroupId },
        /// EN: A group's frozen flag was set/cleared by governance / CN: 群冻结标记被治理设/撤
        GroupFrozenSet { group_id: GroupId, frozen: bool },
    }

    // ---------------------------------------------------------------------
    // Errors / 错误
    // ---------------------------------------------------------------------

    #[pallet::error]
    pub enum Error<T> {
        /// 群不存在 / Group not found
        GroupNotFound,
        /// 不是群成员 / Not a member
        NotMember,
        /// 已是群成员 / Already a member
        AlreadyMember,
        /// 群已满 / Group full
        GroupFull,
        /// 用户群数量超限 / User group limit exceeded
        UserGroupLimitExceeded,
        /// epoch 过期（并发 commit 落败），需重试 / Stale epoch, retry
        EpochStale,
        /// 无权执行（角色/策略不允许）/ Not authorized
        NotAuthorized,
        /// 非群主 / Not the group owner
        NotGroupOwner,
        /// KeyPackage 不存在 / KeyPackage not found
        KeyPackageNotFound,
        /// KeyPackage 数量超限 / Too many KeyPackages
        TooManyKeyPackages,
        /// Welcome 不存在 / Welcome not found
        WelcomeNotFound,
        /// 成员增减不合法（重复/空/越界）/ Bad member delta
        BadMemberDelta,
        /// 字节超过上限 / Blob exceeds bound
        TooLong,
        /// 群 ID 溢出 / Group id overflow
        GroupIdOverflow,
        /// 建群冷却中 / Group creation cooldown
        CreationCooldown,
        /// 公开群无需申请/审批（直接 commit Add）/ Public group needs no request/approval
        PublicGroupNoApproval,
        /// 已有待批申请 / A join request already exists
        AlreadyRequested,
        /// 入群申请不存在 / Join request not found
        JoinRequestNotFound,
        /// 待批申请过多 / Too many pending join requests
        TooManyPendingJoins,
        /// 私群加入未获批准 / Add to private group not approved
        NotApproved,
        /// EN: Addee has not opted into being added to a public group (no published
        /// KeyPackage). CN: 被加成员未选择可被加入公开群（无已发布 KeyPackage）。
        AddeeNotJoinable,
        /// 目标不是群成员 / Target is not a member
        TargetNotMember,
        /// 群主退群前须先转让 / Owner must transfer ownership before leaving
        MustTransferFirst,
        /// 不能对自己执行该操作 / Cannot target self
        CannotTargetSelf,
        /// 目标已被封禁 / Target is already banned
        AlreadyBanned,
        /// 目标未被封禁 / Target is not banned
        NotBanned,
        /// 被封禁账户不可入群（申请 / 被加入）/ Banned account cannot join or be added
        Banned,
        /// 禁言到期时间必须在未来 / Mute expiry must be in the future
        InvalidMuteExpiry,
        /// MLS 写入操作触发限频（commit/anchor 过于频繁）/ MLS write action rate-limited
        RateLimited,
        /// 群已被治理冻结，禁止写入型操作 / Group is frozen by governance
        GroupFrozen,
    }

    // ---------------------------------------------------------------------
    // Calls / 调用接口
    // ---------------------------------------------------------------------

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// EN: Publish a KeyPackage (pre-shared public key bundle) so others can
        /// Add you offline. CN: 发布 KeyPackage，供他人离线 Add 入群。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::publish_key_package())]
        pub fn publish_key_package(origin: OriginFor<T>, kp_bytes: Vec<u8>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let blob: BoundedVec<u8, T::MaxKeyPackageLen> =
                kp_bytes.try_into().map_err(|_| Error::<T>::TooLong)?;

            let count = KeyPackageCount::<T>::get(&who);
            ensure!(count < T::MaxKeyPackagesPerUser::get(), Error::<T>::TooManyKeyPackages);

            // 预留押金防滥用 / reserve anti-spam deposit
            T::Currency::reserve(&who, T::KeyPackageDeposit::get())?;

            let id = NextKeyPackageId::<T>::get(&who);
            KeyPackages::<T>::insert(&who, id, blob);
            NextKeyPackageId::<T>::insert(&who, id.saturating_add(1));
            KeyPackageCount::<T>::insert(&who, count.saturating_add(1));

            Self::deposit_event(Event::KeyPackagePublished { who, id });
            Ok(())
        }

        /// EN: Revoke/rotate a published KeyPackage (compromised/expired).
        /// CN: 吊销/轮换已发布的 KeyPackage（被攻破或过期）。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::revoke_key_package())]
        pub fn revoke_key_package(origin: OriginFor<T>, id: KeyPackageId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(KeyPackages::<T>::contains_key(&who, id), Error::<T>::KeyPackageNotFound);
            KeyPackages::<T>::remove(&who, id);
            KeyPackageCount::<T>::mutate(&who, |c| *c = c.saturating_sub(1));
            // 退还押金 / refund deposit
            T::Currency::unreserve(&who, T::KeyPackageDeposit::get());
            Self::deposit_event(Event::KeyPackageRevoked { who, id });
            Ok(())
        }

        /// EN: Create a group. Client initialises the MLS group locally (epoch 0,
        /// creator only); the chain anchors the initial state.
        /// CN: 创建群。客户端本地初始化 MLS group（epoch 0，仅创建者），链锚定初始状态。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::create_group())]
        pub fn create_group(
            origin: OriginFor<T>,
            init_group_info_cid: Vec<u8>,
            cipher_suite: u16,
            is_public: bool,
            tree_hash: [u8; 32],
            transcript_hash: [u8; 32],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 建群冷却 / creation cooldown
            let now = frame_system::Pallet::<T>::block_number();
            let last = LastGroupCreation::<T>::get(&who);
            if !last.is_zero() {
                ensure!(
                    now.saturating_sub(last) >= T::GroupCreationCooldown::get(),
                    Error::<T>::CreationCooldown
                );
            }

            // 用户群数量上限 / per-user group limit
            ensure!(
                (UserGroups::<T>::get(&who).len() as u32) < T::MaxGroupsPerUser::get(),
                Error::<T>::UserGroupLimitExceeded
            );

            let cid: BoundedVec<u8, T::MaxCidLen> =
                init_group_info_cid.try_into().map_err(|_| Error::<T>::TooLong)?;

            // 预留建群押金 / reserve group creation deposit
            let deposit = T::GroupDeposit::get();
            T::Currency::reserve(&who, deposit)?;

            let group_id = NextGroupId::<T>::get();
            let next = group_id.checked_add(1).ok_or(Error::<T>::GroupIdOverflow)?;
            NextGroupId::<T>::put(next);
            GroupDepositOf::<T>::insert(group_id, (who.clone(), deposit));

            let state = MlsGroupState::<T> {
                epoch: 0,
                tree_hash,
                confirmed_transcript_hash: transcript_hash,
                group_info_cid: cid,
                admin: who.clone(),
                member_count: 1,
                cipher_suite,
                is_public,
            };
            GroupMls::<T>::insert(group_id, &state);

            let joined_at = Self::block_u32(now);
            GroupMembers::<T>::insert(
                group_id,
                &who,
                GroupMember { role: MemberRole::Owner, joined_epoch: 0, joined_at },
            );
            Self::user_groups_add(&who, group_id)?;
            LastGroupCreation::<T>::insert(&who, now);

            Self::deposit_event(Event::GroupCreated { group_id, creator: who, epoch: 0 });
            Ok(())
        }

        /// EN: Submit a membership change (MLS Commit): apply Add/Remove and
        /// advance epoch. The `expected_epoch` gate serialises concurrent commits
        /// using the block's total order (the unique committer arbitration).
        /// CN: 提交成员变更（MLS Commit）：应用 Add/Remove 并推进 epoch。
        /// `expected_epoch` 闸门借区块全序仲裁并发 commit（唯一 committer）。
        #[pallet::call_index(3)]
        // EN: Weight scales with the membership delta size: `commit` does O(added)
        // + O(removed) storage work, so charge per added/removed member instead of a
        // fixed under-estimate. CN: 权重随成员增减规模线性增长：`commit` 的存储开销为
        // O(added)+O(removed)，故按增/删成员数计费，而非固定低估。
        #[pallet::weight(T::WeightInfo::commit(
            member_delta.added.len() as u32,
            member_delta.removed.len() as u32,
        ))]
        #[allow(clippy::too_many_arguments)]
        pub fn commit(
            origin: OriginFor<T>,
            group_id: GroupId,
            expected_epoch: u64,
            commit_bytes: Vec<u8>,
            new_tree_hash: [u8; 32],
            new_transcript_hash: [u8; 32],
            new_group_info_cid: Vec<u8>,
            welcomes: Vec<(T::AccountId, Vec<u8>)>,
            member_delta: MemberDelta<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // 治理冻结闸门 / governance freeze gate
            ensure!(!GroupFrozen::<T>::contains_key(group_id), Error::<T>::GroupFrozen);
            // 写入型 MLS 操作限频（防滥用）/ rate-limit write-heavy MLS action
            Self::note_mls_action(&who)?;

            let mut g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            // ★ 防分叉闸门 / anti-fork gate
            ensure!(g.epoch == expected_epoch, Error::<T>::EpochStale);
            // committer 必须是成员 / committer must be a member
            let committer = GroupMembers::<T>::get(group_id, &who).ok_or(Error::<T>::NotMember)?;

            // 自助退群：仅移除自己（群主须先转让）
            // self-leave: removing only oneself (owner must transfer first)
            let removing_only_self = member_delta.added.is_empty()
                && member_delta.removed.len() == 1
                && member_delta.removed[0] == who;
            // 是否触及他人 / whether others are affected
            let changes_others = !member_delta.added.is_empty()
                || member_delta.removed.iter().any(|a| *a != who);
            if removing_only_self {
                ensure!(who != g.admin, Error::<T>::MustTransferFirst);
            } else if changes_others {
                ensure!(
                    matches!(committer.role, MemberRole::Owner | MemberRole::Admin),
                    Error::<T>::NotAuthorized
                );
            }

            let new_epoch = g.epoch.saturating_add(1);
            let joined_at = Self::block_u32(frame_system::Pallet::<T>::block_number());

            // 校验 delta 合法性 / validate delta before mutating
            for acct in member_delta.added.iter() {
                ensure!(!GroupMembers::<T>::contains_key(group_id, acct), Error::<T>::AlreadyMember);
                // 封禁名单链上强制：被封禁账户不可被加入 / banned accounts cannot be added
                ensure!(!Banned::<T>::contains_key(group_id, acct), Error::<T>::Banned);
                if g.is_public {
                    // 公开群（审计 U3）：被加成员必须已发布至少一个 KeyPackage。
                    // 这既是「同意被加入」的链上信号（成员主动发布、可随时吊销退出），也符合 MLS
                    // ——没有对方 KeyPackage 本就无法 Add。杜绝群主/管理员把任意人拉进公开群。
                    // Public group (audit U3): the addee must have published a KeyPackage.
                    // This is both the on-chain opt-in/consent signal (the member chose to
                    // publish, and can revoke to opt out) and MLS-correct (you cannot Add
                    // without their KeyPackage), preventing owners/admins from pulling
                    // arbitrary accounts into a public group.
                    ensure!(
                        KeyPackageCount::<T>::get(acct) > 0,
                        Error::<T>::AddeeNotJoinable
                    );
                } else {
                    // 私群：被加成员必须已获管理员批准 / private group: addee must be approved
                    ensure!(
                        JoinApprovals::<T>::contains_key(group_id, acct),
                        Error::<T>::NotApproved
                    );
                }
            }
            for acct in member_delta.removed.iter() {
                ensure!(GroupMembers::<T>::contains_key(group_id, acct), Error::<T>::NotMember);
                // 群主不可被移除（须先转让，P1）/ owner cannot be removed (transfer first, P1)
                ensure!(*acct != g.admin, Error::<T>::BadMemberDelta);
            }
            let added = member_delta.added.len() as u32;
            let removed = member_delta.removed.len() as u32;
            let new_count = g
                .member_count
                .saturating_add(added)
                .saturating_sub(removed);
            ensure!(new_count <= T::MaxGroupMembers::get(), Error::<T>::GroupFull);

            // 应用成员表 + UserGroups 同步（杜绝幽灵群）
            // apply member table + UserGroups sync (no ghost entries)
            for acct in member_delta.added.iter() {
                GroupMembers::<T>::insert(
                    group_id,
                    acct,
                    GroupMember { role: MemberRole::Member, joined_epoch: new_epoch, joined_at },
                );
                Self::user_groups_add(acct, group_id)?;
                // 消费入群申请/批准状态 / consume join request & approval
                Self::clear_join_state(group_id, acct);
                // 镜像到外部授权层（成员↔群主）/ mirror to external auth (member↔owner)
                T::ChatHook::on_member_added(group_id, acct, &g.admin);
                Self::deposit_event(Event::MemberJoined {
                    group_id,
                    member: acct.clone(),
                    epoch: new_epoch,
                });
            }
            for acct in member_delta.removed.iter() {
                GroupMembers::<T>::remove(group_id, acct);
                WelcomeMailbox::<T>::remove(group_id, acct);
                // 清理离群成员的群内昵称与禁言状态（封禁名单保留以防回流）。
                // Clear the leaver's nickname & mute; keep the ban entry to prevent rejoin.
                GroupNicknames::<T>::remove(group_id, acct);
                MemberMutedUntil::<T>::remove(group_id, acct);
                Self::user_groups_remove(acct, group_id);
                T::ChatHook::on_member_removed(group_id, acct, &g.admin);
                Self::deposit_event(Event::MemberRemoved {
                    group_id,
                    member: acct.clone(),
                    epoch: new_epoch,
                });
            }

            // 推进 epoch + 锚点 / advance epoch + anchors
            g.epoch = new_epoch;
            g.tree_hash = new_tree_hash;
            g.confirmed_transcript_hash = new_transcript_hash;
            g.group_info_cid =
                new_group_info_cid.try_into().map_err(|_| Error::<T>::TooLong)?;
            g.member_count = new_count;
            GroupMls::<T>::insert(group_id, &g);

            // 落 Commit 到本 epoch 日志 / append commit to this epoch's log
            let commit_blob: BoundedVec<u8, T::MaxHandshakeLen> =
                commit_bytes.try_into().map_err(|_| Error::<T>::TooLong)?;
            HandshakeLog::<T>::insert(group_id, new_epoch, commit_blob);

            // 投递 Welcome / deliver Welcome to added members
            for (acct, w) in welcomes {
                let wb: BoundedVec<u8, T::MaxWelcomeLen> =
                    w.try_into().map_err(|_| Error::<T>::TooLong)?;
                WelcomeMailbox::<T>::insert(group_id, acct, wb);
            }

            Self::deposit_event(Event::Committed { group_id, epoch: new_epoch, committer: who });
            Ok(())
        }

        /// EN: Claim (and delete) your Welcome message to join a group.
        /// CN: 领取（并删除）自己的 Welcome 以入群。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::claim_welcome())]
        pub fn claim_welcome(origin: OriginFor<T>, group_id: GroupId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                WelcomeMailbox::<T>::contains_key(group_id, &who),
                Error::<T>::WelcomeNotFound
            );
            WelcomeMailbox::<T>::remove(group_id, &who);
            Self::deposit_event(Event::WelcomeClaimed { group_id, who });
            Ok(())
        }

        /// EN: Disband a group (owner only). Cleans all group storage and syncs
        /// every member's UserGroups. CN: 解散群（仅群主），清理全部群存储并同步成员 UserGroups。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::disband_group())]
        pub fn disband_group(origin: OriginFor<T>, group_id: GroupId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            ensure!(g.admin == who, Error::<T>::NotGroupOwner);
            // 有界拆除（审计 B4）：大群可能需重复调用，未清完会发 GroupDisbandProgress。
            // Bounded teardown (audit B4): large groups may need repeated calls.
            let _ = Self::do_disband(group_id);
            Ok(())
        }

        /// EN: OPTIONAL strong-audit anchor. Records a digest of a batch of
        /// message ciphertexts — no CID/plaintext. Owner/Admin only.
        /// CN: 【可选】强审计锚：记录一批消息密文的 digest（无 CID/明文），仅群主/管理员。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::anchor_message_digest())]
        pub fn anchor_message_digest(
            origin: OriginFor<T>,
            group_id: GroupId,
            batch_seq: u64,
            digest: [u8; 32],
            epoch: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // 治理冻结闸门 / governance freeze gate
            ensure!(!GroupFrozen::<T>::contains_key(group_id), Error::<T>::GroupFrozen);
            // 写入型 MLS 操作限频（防滥用）/ rate-limit write-heavy MLS action
            Self::note_mls_action(&who)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            let m = GroupMembers::<T>::get(group_id, &who).ok_or(Error::<T>::NotMember)?;
            ensure!(
                matches!(m.role, MemberRole::Owner | MemberRole::Admin),
                Error::<T>::NotAuthorized
            );
            let now = frame_system::Pallet::<T>::block_number();
            MessageDigestAnchor::<T>::insert(group_id, batch_seq, (digest, epoch, now));
            Self::deposit_event(Event::MessageDigestAnchored { group_id, batch_seq, epoch });
            Ok(())
        }

        /// EN: Request to join a private group (awaits admin approval). Public
        /// groups need no request — clients are Added directly via `commit`.
        /// CN: 申请加入私群（待管理员批准）。公开群无需申请——客户端经 `commit` 直接 Add。
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::request_join())]
        pub fn request_join(origin: OriginFor<T>, group_id: GroupId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // 治理冻结闸门：冻结群不接受新入群申请 / frozen groups reject new join requests
            ensure!(!GroupFrozen::<T>::contains_key(group_id), Error::<T>::GroupFrozen);
            let g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            ensure!(!g.is_public, Error::<T>::PublicGroupNoApproval);
            ensure!(!GroupMembers::<T>::contains_key(group_id, &who), Error::<T>::AlreadyMember);
            ensure!(!Banned::<T>::contains_key(group_id, &who), Error::<T>::Banned);
            ensure!(!JoinRequests::<T>::contains_key(group_id, &who), Error::<T>::AlreadyRequested);

            let cnt = PendingJoinCount::<T>::get(group_id);
            ensure!(cnt < T::MaxPendingJoins::get(), Error::<T>::TooManyPendingJoins);

            let now = frame_system::Pallet::<T>::block_number();
            JoinRequests::<T>::insert(group_id, &who, now);
            PendingJoinCount::<T>::insert(group_id, cnt.saturating_add(1));
            Self::deposit_event(Event::JoinRequested { group_id, who });
            Ok(())
        }

        /// EN: Withdraw your own pending join request.
        /// CN: 撤回自己的待批入群申请。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::cancel_join_request())]
        pub fn cancel_join_request(origin: OriginFor<T>, group_id: GroupId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                JoinRequests::<T>::contains_key(group_id, &who),
                Error::<T>::JoinRequestNotFound
            );
            Self::clear_join_state(group_id, &who);
            Self::deposit_event(Event::JoinRequestCancelled { group_id, who });
            Ok(())
        }

        /// EN: Approve a pending join (owner/admin). The approval is consumed by
        /// the subsequent Add `commit`; the chain records authorization only.
        /// CN: 批准待批入群（群主/管理员）。批准由随后的 Add `commit` 消费，链只记录授权。
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::approve_join())]
        pub fn approve_join(
            origin: OriginFor<T>,
            group_id: GroupId,
            who: T::AccountId,
        ) -> DispatchResult {
            let by = ensure_signed(origin)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            let approver = GroupMembers::<T>::get(group_id, &by).ok_or(Error::<T>::NotMember)?;
            ensure!(
                matches!(approver.role, MemberRole::Owner | MemberRole::Admin),
                Error::<T>::NotAuthorized
            );
            ensure!(
                JoinRequests::<T>::contains_key(group_id, &who),
                Error::<T>::JoinRequestNotFound
            );
            ensure!(!GroupMembers::<T>::contains_key(group_id, &who), Error::<T>::AlreadyMember);
            ensure!(!Banned::<T>::contains_key(group_id, &who), Error::<T>::Banned);

            let now = frame_system::Pallet::<T>::block_number();
            JoinApprovals::<T>::insert(group_id, &who, now);
            Self::deposit_event(Event::JoinApproved { group_id, who, by });
            Ok(())
        }

        /// EN: Transfer group ownership (owner only). Pure app-layer role swap;
        /// the MLS tree is unaffected. CN: 转让群主（仅群主）。纯应用层角色互换，MLS 树不变。
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::transfer_ownership())]
        pub fn transfer_ownership(
            origin: OriginFor<T>,
            group_id: GroupId,
            to: T::AccountId,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let mut g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            ensure!(g.admin == who, Error::<T>::NotGroupOwner);
            ensure!(to != who, Error::<T>::CannotTargetSelf);

            let mut target = GroupMembers::<T>::get(group_id, &to).ok_or(Error::<T>::TargetNotMember)?;
            let mut old = GroupMembers::<T>::get(group_id, &who).ok_or(Error::<T>::NotMember)?;
            old.role = MemberRole::Admin;
            target.role = MemberRole::Owner;
            GroupMembers::<T>::insert(group_id, &who, old);
            GroupMembers::<T>::insert(group_id, &to, target);
            g.admin = to.clone();
            GroupMls::<T>::insert(group_id, &g);

            Self::deposit_event(Event::OwnershipTransferred { group_id, from: who, to });
            Ok(())
        }

        /// EN: Set/unset admin role for a member (owner only). Pure app-layer.
        /// CN: 设/撤某成员的管理员角色（仅群主）。纯应用层。
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::set_admin())]
        pub fn set_admin(
            origin: OriginFor<T>,
            group_id: GroupId,
            who: T::AccountId,
            on: bool,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            ensure!(g.admin == owner, Error::<T>::NotGroupOwner);
            ensure!(who != owner, Error::<T>::CannotTargetSelf);

            let mut m = GroupMembers::<T>::get(group_id, &who).ok_or(Error::<T>::TargetNotMember)?;
            // 群主角色不可经此修改 / owner role cannot be changed here
            ensure!(m.role != MemberRole::Owner, Error::<T>::BadMemberDelta);
            m.role = if on { MemberRole::Admin } else { MemberRole::Member };
            GroupMembers::<T>::insert(group_id, &who, m);

            Self::deposit_event(Event::AdminSet { group_id, who, on });
            Ok(())
        }

        /// EN: Set the group display profile (name / avatar CID / announcement),
        /// owner/admin only. Each field is optional; `None` leaves it unchanged.
        /// Pure app-layer metadata — the MLS state is untouched.
        /// CN: 设置群展示资料（群名 / 头像 CID / 公告），仅群主/管理员。各字段可选，
        /// `None` 表示保持不变。纯应用层元数据，不触碰 MLS 状态。
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::set_group_profile())]
        pub fn set_group_profile(
            origin: OriginFor<T>,
            group_id: GroupId,
            name: Option<Vec<u8>>,
            avatar_cid: Option<Vec<u8>>,
            announcement: Option<Vec<u8>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(group_id, &who)?;

            let mut profile = GroupProfiles::<T>::get(group_id).unwrap_or_default();
            if let Some(name) = name {
                profile.name = name.try_into().map_err(|_| Error::<T>::TooLong)?;
            }
            if let Some(cid) = avatar_cid {
                profile.avatar_cid = cid.try_into().map_err(|_| Error::<T>::TooLong)?;
            }
            if let Some(ann) = announcement {
                profile.announcement = ann.try_into().map_err(|_| Error::<T>::TooLong)?;
            }
            GroupProfiles::<T>::insert(group_id, profile);

            Self::deposit_event(Event::GroupProfileUpdated { group_id, by: who });
            Ok(())
        }

        /// EN: Set (or clear with `None`) your own in-group nickname. Caller must
        /// be a member. CN: 设置（或以 `None` 清除）自己的群内昵称（群名片），
        /// 调用者须为群成员。
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::set_group_nickname())]
        pub fn set_group_nickname(
            origin: OriginFor<T>,
            group_id: GroupId,
            nickname: Option<Vec<u8>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(GroupMembers::<T>::contains_key(group_id, &who), Error::<T>::NotMember);

            match nickname {
                Some(nick) => {
                    let bounded: BoundedVec<u8, T::MaxGroupNicknameLen> =
                        nick.try_into().map_err(|_| Error::<T>::TooLong)?;
                    GroupNicknames::<T>::insert(group_id, &who, bounded);
                }
                None => {
                    GroupNicknames::<T>::remove(group_id, &who);
                }
            }

            Self::deposit_event(Event::MemberNicknameSet { group_id, who });
            Ok(())
        }

        /// EN: Ban an account from the group (owner/admin). A ban is enforced on
        /// chain: the account can neither request to join nor be added via
        /// `commit`. Banning does NOT remove a current member from MLS — the
        /// admin must still issue a `commit` removal; the ban only prevents
        /// rejoin. Cannot target self or the owner.
        /// CN: 封禁某账户（群主/管理员）。封禁**链上强制**：该账户既不能申请入群也不能
        /// 经 `commit` 被加入。封禁**不会**把现有成员移出 MLS——管理员仍需发起 `commit`
        /// 移除；封禁仅阻止其回流。不能针对自己或群主。
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::ban_member())]
        pub fn ban_member(
            origin: OriginFor<T>,
            group_id: GroupId,
            who: T::AccountId,
        ) -> DispatchResult {
            let by = ensure_signed(origin)?;
            let g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            Self::ensure_owner_or_admin(group_id, &by)?;
            ensure!(who != by, Error::<T>::CannotTargetSelf);
            ensure!(who != g.admin, Error::<T>::NotAuthorized);
            ensure!(!Banned::<T>::contains_key(group_id, &who), Error::<T>::AlreadyBanned);

            let now = frame_system::Pallet::<T>::block_number();
            Banned::<T>::insert(group_id, &who, now);
            // 消费可能存在的入群申请/批准，避免封禁后仍残留待批状态。
            // Consume any pending join request/approval so no stale state survives the ban.
            Self::clear_join_state(group_id, &who);
            Self::deposit_event(Event::MemberBanned { group_id, who, by });
            Ok(())
        }

        /// EN: Lift a ban (owner/admin). / CN: 解除封禁（群主/管理员）。
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::unban_member())]
        pub fn unban_member(
            origin: OriginFor<T>,
            group_id: GroupId,
            who: T::AccountId,
        ) -> DispatchResult {
            let by = ensure_signed(origin)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            Self::ensure_owner_or_admin(group_id, &by)?;
            ensure!(Banned::<T>::contains_key(group_id, &who), Error::<T>::NotBanned);

            Banned::<T>::remove(group_id, &who);
            Self::deposit_event(Event::MemberUnbanned { group_id, who, by });
            Ok(())
        }

        /// EN: Mute a member until `until` (block), or unmute with `None`
        /// (owner/admin). APP-LAYER POLICY: since messages are off-chain, clients
        /// / relay nodes enforce it from this state. Target must be a member and
        /// not the owner; cannot target self.
        /// CN: 将某成员禁言至 `until`（区块），或以 `None` 解除禁言（群主/管理员）。
        /// **应用层策略**：消息离链，由客户端 / 中继节点据此状态执行。目标须为成员且
        /// 非群主，不能针对自己。
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::set_member_mute())]
        pub fn set_member_mute(
            origin: OriginFor<T>,
            group_id: GroupId,
            who: T::AccountId,
            until: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let by = ensure_signed(origin)?;
            let g = GroupMls::<T>::get(group_id).ok_or(Error::<T>::GroupNotFound)?;
            Self::ensure_owner_or_admin(group_id, &by)?;
            ensure!(who != by, Error::<T>::CannotTargetSelf);
            ensure!(who != g.admin, Error::<T>::NotAuthorized);
            ensure!(GroupMembers::<T>::contains_key(group_id, &who), Error::<T>::TargetNotMember);

            match until {
                Some(until) => {
                    let now = frame_system::Pallet::<T>::block_number();
                    ensure!(until > now, Error::<T>::InvalidMuteExpiry);
                    MemberMutedUntil::<T>::insert(group_id, &who, until);
                    Self::deposit_event(Event::MemberMuted { group_id, who, until, by });
                }
                None => {
                    MemberMutedUntil::<T>::remove(group_id, &who);
                    Self::deposit_event(Event::MemberUnmuted { group_id, who, by });
                }
            }
            Ok(())
        }

        /// EN: Toggle group-wide mute-all (owner/admin). APP-LAYER POLICY enforced
        /// off-chain. / CN: 切换全员禁言（群主/管理员）。应用层策略，链下执行。
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::set_group_mute_all())]
        pub fn set_group_mute_all(
            origin: OriginFor<T>,
            group_id: GroupId,
            on: bool,
        ) -> DispatchResult {
            let by = ensure_signed(origin)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            Self::ensure_owner_or_admin(group_id, &by)?;

            if on {
                GroupMutedAll::<T>::insert(group_id, true);
            } else {
                GroupMutedAll::<T>::remove(group_id);
            }
            Self::deposit_event(Event::GroupMuteAllSet { group_id, on, by });
            Ok(())
        }

        /// EN: Governance force-disbands a group for platform compliance,
        /// regardless of in-group ownership. Tears down all storage and refunds
        /// the creation deposit (same teardown as owner `disband_group`).
        /// CN: 治理出于平台合规强制解散群，无视群内归属；清理全部存储并退还建群押金
        /// （与群主 `disband_group` 同样的清理流程）。
        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::force_disband_group())]
        pub fn force_disband_group(origin: OriginFor<T>, group_id: GroupId) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            // 有界拆除（审计 B4）：仅在完全拆除时发终态事件；未清完发 GroupDisbandProgress，
            // 治理重复调用直至完成（群在拆除期间已被冻结，无法继续增长）。
            // Bounded teardown (audit B4): emit the terminal event only on full
            // teardown; otherwise GroupDisbandProgress is emitted and governance
            // repeats (the group is frozen during teardown, so it cannot grow).
            if Self::do_disband(group_id) {
                Self::deposit_event(Event::GroupForceDisbanded { group_id });
            }
            Ok(())
        }

        /// EN: Governance freezes (or unfreezes) a group. While frozen, the group
        /// rejects `commit` / `anchor_message_digest` / `request_join`; existing
        /// metadata stays readable so clients can show a "frozen" state.
        /// CN: 治理冻结（或解冻）群。冻结期间拒绝 `commit` / `anchor_message_digest` /
        /// `request_join`；已有元数据仍可读，客户端可展示"已冻结"状态。
        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::set_group_frozen())]
        pub fn set_group_frozen(
            origin: OriginFor<T>,
            group_id: GroupId,
            frozen: bool,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(GroupMls::<T>::contains_key(group_id), Error::<T>::GroupNotFound);
            if frozen {
                GroupFrozen::<T>::insert(group_id, ());
            } else {
                GroupFrozen::<T>::remove(group_id);
            }
            Self::deposit_event(Event::GroupFrozenSet { group_id, frozen });
            Ok(())
        }
    }

    // ---------------------------------------------------------------------
    // Internal helpers / 内部函数
    // ---------------------------------------------------------------------

    impl<T: Config> Pallet<T> {
        /// EN: Account a write-heavy MLS action (`commit` / `anchor`) against the
        /// per-account windowed rate limit; errors with `RateLimited` when the
        /// window quota is exhausted. CN: 把一次写入型 MLS 操作（`commit` / `anchor`）
        /// 计入按账户窗口限频；配额耗尽时报 `RateLimited`。
        fn note_mls_action(who: &T::AccountId) -> DispatchResult {
            let now = frame_system::Pallet::<T>::block_number();
            MlsActionRate::<T>::try_mutate(who, |state| -> DispatchResult {
                let res = check_and_update_rate_limit(
                    state,
                    now,
                    T::MlsActionWindow::get(),
                    T::MaxMlsActionsPerWindow::get(),
                );
                ensure!(res == RateLimitResult::Allowed, Error::<T>::RateLimited);
                Ok(())
            })
        }

        /// EN: Ensure `who` is an Owner or Admin of `group_id`.
        /// CN: 校验 `who` 是 `group_id` 的群主或管理员。
        fn ensure_owner_or_admin(group_id: GroupId, who: &T::AccountId) -> DispatchResult {
            let m = GroupMembers::<T>::get(group_id, who).ok_or(Error::<T>::NotMember)?;
            ensure!(
                matches!(m.role, MemberRole::Owner | MemberRole::Admin),
                Error::<T>::NotAuthorized
            );
            Ok(())
        }

        /// EN: Whether `who` is currently muted in `group_id` (per-member mute not
        /// yet expired, OR group-wide mute-all is on and `who` is a plain Member).
        /// Read-only helper for clients / relay nodes; the chain does not gate
        /// off-chain message delivery itself.
        /// CN: `who` 当前在 `group_id` 是否被禁言（单人禁言未到期，或全员禁言开启且
        /// `who` 为普通成员）。供客户端 / 中继节点只读查询；链本身不拦截离链消息投递。
        pub fn is_member_muted(group_id: GroupId, who: &T::AccountId) -> bool {
            let now = frame_system::Pallet::<T>::block_number();
            if let Some(until) = MemberMutedUntil::<T>::get(group_id, who) {
                if now < until {
                    return true;
                }
            }
            if GroupMutedAll::<T>::get(group_id) {
                // 全员禁言下，群主/管理员仍可发言 / owner & admins exempt under mute-all
                if let Some(m) = GroupMembers::<T>::get(group_id, who) {
                    return matches!(m.role, MemberRole::Member);
                }
            }
            false
        }

        /// EN: Group ids `who` belongs to. Read-only view helper for the unified
        /// conversation Runtime API. CN: `who` 所属的群 id 列表；供统一会话
        /// Runtime API 的只读视图使用。
        pub fn user_group_ids(who: &T::AccountId) -> Vec<GroupId> {
            UserGroups::<T>::get(who).into_inner()
        }

        /// EN: Display profile (name / avatar / announcement) of a group, if any.
        /// CN: 群展示资料（群名 / 头像 / 公告），不存在则 None。
        pub fn group_profile(group_id: GroupId) -> Option<GroupProfile<T>> {
            GroupProfiles::<T>::get(group_id)
        }

        /// EN: `who`'s role tag in `group_id` as a stable u8 (see
        /// `pallet_chat_common::runtime_api::role`): 0=Owner,1=Admin,2=Member,
        /// 255=non-member. CN: `who` 在群中的角色（稳定 u8）：0 群主、1 管理员、
        /// 2 普通成员、255 非成员。
        pub fn member_role_tag(group_id: GroupId, who: &T::AccountId) -> u8 {
            match GroupMembers::<T>::get(group_id, who).map(|m| m.role) {
                Some(MemberRole::Owner) => 0,
                Some(MemberRole::Admin) => 1,
                Some(MemberRole::Member) => 2,
                None => 255,
            }
        }

        /// EN: Current member count of a group (0 if unknown).
        /// CN: 群当前成员数（未知为 0）。
        pub fn group_member_count(group_id: GroupId) -> u32 {
            GroupMls::<T>::get(group_id).map(|g| g.member_count).unwrap_or(0)
        }

        /// EN: Append a group to a user's list, guarding the per-user bound.
        /// CN: 把群加入用户列表，校验单用户上限。
        fn user_groups_add(who: &T::AccountId, group_id: GroupId) -> DispatchResult {
            UserGroups::<T>::try_mutate(who, |groups| {
                if groups.contains(&group_id) {
                    return Ok(());
                }
                groups.try_push(group_id).map_err(|_| Error::<T>::UserGroupLimitExceeded.into())
            })
        }

        /// EN: Remove a group from a user's list (idempotent).
        /// CN: 从用户列表移除群（幂等）。
        fn user_groups_remove(who: &T::AccountId, group_id: GroupId) {
            UserGroups::<T>::mutate(who, |groups| groups.retain(|g| *g != group_id));
        }

        /// EN: Tear down a group's storage in **bounded** work per call and sync
        /// members' lists. Returns `true` once the group is fully removed.
        ///
        /// # DoS（审计 B4）/ DoS (audit B4)
        /// `HandshakeLog` / `MessageDigestAnchor` / `Banned` 等前缀随群生命周期无上界增长，
        /// 旧实现用 `clear_prefix(u32::MAX)` 配固定权重会被严重低估。改为单次最多处理
        /// `MAX_DISBAND_ITEMS_PER_CALL` 项：进入即冻结群（阻止 `commit` 等在拆除期间继续
        /// 写入），按预算分批清理，全部清空后才移除 `GroupMls` 并退押金；未清完则发
        /// `GroupDisbandProgress` 事件，调用方重复调用直至返回 `true`。小群（含全部单测）
        /// 一次即完成。The legacy `clear_prefix(u32::MAX)` under a fixed weight was
        /// under-charged because these prefixes grow without bound. We now process at
        /// most `MAX_DISBAND_ITEMS_PER_CALL` items per call: freeze the group on entry
        /// (so `commit` cannot grow storage mid-teardown), clear within budget, and
        /// only finalize (remove `GroupMls`, refund deposit) once everything is empty;
        /// otherwise emit `GroupDisbandProgress` and let the caller repeat. Small groups
        /// (including every unit test) finish in a single call.
        fn do_disband(group_id: GroupId) -> bool {
            let budget = crate::MAX_DISBAND_ITEMS_PER_CALL;
            let admin = GroupMls::<T>::get(group_id).map(|g| g.admin);

            // 冻结群，阻止拆除期间的写入型操作继续增长存储（幂等）。
            // Freeze to block write-heavy ops from growing storage mid-teardown.
            GroupFrozen::<T>::insert(group_id, ());

            // 1. 本次最多处理 `budget` 个成员：每个成员只处理一次（处理即移除其行，
            //    故重复调用不会重复触发 hook）。Process up to `budget` members; each is
            //    handled exactly once (its row is removed), so repeats never double-fire.
            let members: Vec<T::AccountId> = GroupMembers::<T>::iter_key_prefix(group_id)
                .take(budget as usize)
                .collect();
            for acct in members.iter() {
                Self::user_groups_remove(acct, group_id);
                // 镜像撤销（群主与自身的对授权无意义，跳过）/ mirror revoke (skip owner-self)
                if let Some(ref a) = admin {
                    if acct != a {
                        T::ChatHook::on_member_removed(group_id, acct, a);
                    }
                }
                GroupMembers::<T>::remove(group_id, acct);
            }

            // 2. 有界清理其余（可能很大的）前缀。/ Bounded-clear the other prefixes.
            let c_hand = HandshakeLog::<T>::clear_prefix(group_id, budget, None);
            let c_welc = WelcomeMailbox::<T>::clear_prefix(group_id, budget, None);
            let c_dig = MessageDigestAnchor::<T>::clear_prefix(group_id, budget, None);
            let c_jreq = JoinRequests::<T>::clear_prefix(group_id, budget, None);
            let c_japp = JoinApprovals::<T>::clear_prefix(group_id, budget, None);
            let c_nick = GroupNicknames::<T>::clear_prefix(group_id, budget, None);
            let c_ban = Banned::<T>::clear_prefix(group_id, budget, None);
            let c_mute = MemberMutedUntil::<T>::clear_prefix(group_id, budget, None);

            // 3. 仅当无成员残留且每个前缀都已清空时才算完成。
            //    Done only when no members remain and every prefix is fully cleared.
            let members_left = GroupMembers::<T>::iter_key_prefix(group_id).next().is_some();
            let more = members_left
                || c_hand.maybe_cursor.is_some()
                || c_welc.maybe_cursor.is_some()
                || c_dig.maybe_cursor.is_some()
                || c_jreq.maybe_cursor.is_some()
                || c_japp.maybe_cursor.is_some()
                || c_nick.maybe_cursor.is_some()
                || c_ban.maybe_cursor.is_some()
                || c_mute.maybe_cursor.is_some();
            if more {
                Self::deposit_event(Event::GroupDisbandProgress { group_id });
                return false;
            }

            // 4. 收尾：清理单值存储、退押金、移除群根、发终态事件。
            //    Finalize: clear singletons, refund deposit, remove root, emit done.
            GroupProfiles::<T>::remove(group_id);
            GroupMutedAll::<T>::remove(group_id);
            PendingJoinCount::<T>::remove(group_id);
            GroupFrozen::<T>::remove(group_id);
            // 退还建群押金 / refund group creation deposit
            if let Some((depositor, amount)) = GroupDepositOf::<T>::take(group_id) {
                T::Currency::unreserve(&depositor, amount);
            }
            GroupMls::<T>::remove(group_id);
            Self::deposit_event(Event::GroupDisbanded { group_id });
            true
        }

        /// EN: Clear a member's pending join request + approval (consumed on Add).
        /// CN: 清理某成员的待批申请与批准状态（Add 时消费）。
        fn clear_join_state(group_id: GroupId, who: &T::AccountId) {
            JoinApprovals::<T>::remove(group_id, who);
            if JoinRequests::<T>::take(group_id, who).is_some() {
                PendingJoinCount::<T>::mutate(group_id, |c| *c = c.saturating_sub(1));
            }
        }

        /// EN: Saturating conversion of block number to u32 for timestamps.
        /// CN: 区块号到 u32 的饱和转换（用于时间戳）。
        fn block_u32(n: BlockNumberFor<T>) -> u32 {
            n.unique_saturated_into()
        }
    }
}
