//! 聊天权限系统类型定义
//!
//! 本模块定义了聊天权限系统所需的所有核心类型，包括：
//! - 场景类型 (SceneType)
//! - 场景标识 (SceneId)
//! - 场景授权 (SceneAuthorization)
//! - 聊天权限级别 (ChatPermissionLevel)
//! - 用户隐私设置 (PrivacySettings)
//! - 权限检查结果 (PermissionResult)

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

/// 场景类型枚举
///
/// 定义了系统支持的各种聊天场景类型，业务模块通过场景类型
/// 来区分不同的聊天授权来源。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum SceneType {
    /// 做市商场景：用户可咨询做市商
    /// 当用户与做市商建立交易关系时自动授权
    MarketMaker,

    /// 订单场景：订单买卖双方
    /// 当订单创建时自动授权买卖双方聊天
    Order,

    /// 纪念馆场景：访客可联系管理员
    /// 当用户访问或操作纪念馆时授权
    Memorial,

    /// 群聊场景：群成员之间的聊天
    /// 群聊成员自动获得相互聊天权限
    Group,

    /// 自定义场景：用于扩展新的业务场景
    /// 使用最多32字节的标识符来区分不同的自定义场景
    Custom(BoundedVec<u8, ConstU32<32>>),

    /// 1:1 direct chat scene — LEGACY / TEST-ONLY.
    /// 直聊场景——**遗留 / 仅测试**。
    ///
    /// EN: This variant predates the final privacy model and is NOT granted by any
    /// production caller. 1:1 DMs are now off-chain only: they MUST NOT create a
    /// 2-member on-chain group (see `pallet-chat-group`'s `TwoMemberGroupForbidden`
    /// invariant), and the "may DM me" right is a receiver-signed off-chain
    /// capability token (see `CapabilityEpoch`), NOT an on-chain `Direct` scene
    /// authorization. The variant is kept only to preserve SCALE indices and is
    /// referenced solely by unit tests / the RPC string mapping.
    /// CN: 本变体早于最终隐私模型，**生产无任何调用方授予**。1:1 私聊现仅走链下：
    /// 禁止建成 2 人链上群（见 `pallet-chat-group` 的 `TwoMemberGroupForbidden`
    /// 不变量），「允许向我私聊」由接收方签名的链下能力令牌承载（见 `CapabilityEpoch`），
    /// 而**非**链上 `Direct` 场景授权。保留该变体仅为维持 SCALE 索引，目前仅单测 /
    /// RPC 字符串映射引用。
    ///
    /// NOTE: appended at the end of the enum on purpose to preserve SCALE
    /// variant indices of pre-existing variants. / 注意：刻意追加在末尾，
    /// 以保持既有变体的 SCALE 编码索引不变。
    Direct,
}

impl Default for SceneType {
    fn default() -> Self {
        SceneType::Order
    }
}

/// 场景标识符枚举
///
/// 用于唯一标识某个具体的业务场景实例，如订单ID、纪念馆ID等。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
pub enum SceneId {
    /// 无特定 ID（如 MarketMaker 场景不需要具体ID）
    #[default]
    None,

    /// 数字 ID（订单号、纪念馆ID、群聊ID等）
    Numeric(u64),

    /// Hash ID（用于更复杂的标识需求）
    Hash([u8; 32]),
}

/// 场景授权结构体
///
/// 记录两个用户之间某个场景的聊天授权信息。
/// 包含授权来源、时间、有效期和额外元数据。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub struct SceneAuthorization<BlockNumber> {
    /// 场景类型
    pub scene_type: SceneType,

    /// 场景标识（如订单ID、纪念馆ID）
    pub scene_id: SceneId,

    /// 授权来源 pallet 标识（8字节）
    /// 用于标识是哪个业务模块发起的授权
    pub source_pallet: [u8; 8],

    /// 授权时间（区块号）
    pub granted_at: BlockNumber,

    /// 过期时间（None 表示永不过期）
    pub expires_at: Option<BlockNumber>,

    /// 额外元数据（最大 128 字节）。
    ///
    /// # 隐私警告（审计 P2）/ Privacy warning (audit P2)
    /// EN: This blob is stored **on-chain in clear**. Do NOT put sensitive
    /// plaintext here (order amounts, memorial/person names, free-text notes):
    /// it would widen the relationship leak. Pass empty, or an **opaque**
    /// reference only (e.g. an encrypted IPFS CID the parties can resolve). All
    /// current production callers pass empty metadata.
    /// CN: 该字段**以明文存于链上**。请勿放敏感明文（订单金额、纪念馆/人名、自由文本备注），
    /// 否则会扩大关系泄漏面。应传空，或仅传**不透明**引用（如双方可解析的加密 IPFS CID）。
    /// 目前所有生产调用方均传空 metadata。
    pub metadata: BoundedVec<u8, ConstU32<128>>,
}

/// 聊天权限级别枚举
///
/// 定义用户的基础聊天权限策略，决定陌生人能否发起聊天。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
pub enum ChatPermissionLevel {
    /// 开放：任何人可发起聊天
    Open,

    /// EN: Contacts only (default). The on-chain friend graph was removed; the
    /// social "is a contact" check is enforced off-chain via capability tokens.
    /// On-chain, a stranger without a scene authorization or whitelist entry is
    /// denied. CN: 仅联系人（默认）。链上好友图谱已删除，「是否联系人」由链下能力
    /// 令牌强制；链上对无场景授权且不在白名单的陌生人一律拒绝。
    #[default]
    FriendsOnly,

    /// EN: Whitelist. DEPRECATED semantics: the on-chain whitelist was removed
    /// for privacy (audit P1), so this level now behaves like `FriendsOnly` —
    /// the "allowed contact" decision is made off-chain via capability tokens.
    /// Kept as a variant only to preserve SCALE indices. CN: 白名单。**语义已弃用**：
    /// 链上白名单已为隐私移除（审计 P1），此级别现等同 `FriendsOnly`——「是否放行的
    /// 联系人」改由链下能力令牌判定。保留变体仅为维持 SCALE 编码索引不变。
    Whitelist,

    /// 关闭：不接受任何消息
    Closed,
}

/// 用户隐私设置结构体
///
/// EN: Stores a user's chat permission policy: the base permission level and the
/// set of rejected scene types. The on-chain **block list / whitelist were
/// removed for privacy** (audit P1): an on-chain plaintext (or even hashed) list
/// of who you blocked / allowed is enumerable and leaks the very communication
/// relationships the design hides. Blocking and the "may DM me" right now live
/// entirely off-chain as receiver-signed capability tokens, revoked via
/// [`crate::CapabilityEpoch`] (`bump_capability_epoch`) and per-contact inbox tag
/// revocation (`pallet-chat-inbox::revoke_tag`).
/// CN: 存储用户的聊天权限策略：基础权限级别与被拒场景类型集合。链上**黑名单 / 白名单
/// 已为隐私移除**（审计 P1）：链上明文（乃至哈希）的「拉黑 / 放行」名单可被枚举，会
/// 泄露本设计要隐藏的通信关系。拉黑与「允许向我私聊」的权利现完全以链下、由接收方签名
/// 的能力令牌承载，经 [`crate::CapabilityEpoch`]（`bump_capability_epoch`）与每联系人
/// 信箱标签撤销（`pallet-chat-inbox::revoke_tag`）实现。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    DebugNoBound,
    TypeInfo,
    MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct PrivacySettings<T: crate::Config> {
    /// 聊天权限级别
    pub permission_level: ChatPermissionLevel,

    /// 拒绝的场景类型（空表示接受所有场景）
    /// 用户可以选择拒绝某些类型的场景授权聊天
    pub rejected_scene_types: BoundedVec<SceneType, ConstU32<10>>,

    /// 最后更新区块号
    pub updated_at: frame_system::pallet_prelude::BlockNumberFor<T>,
}

impl<T: crate::Config> Default for PrivacySettings<T> {
    fn default() -> Self {
        Self {
            permission_level: ChatPermissionLevel::default(),
            rejected_scene_types: BoundedVec::default(),
            updated_at: Default::default(),
        }
    }
}

/// 权限检查结果枚举
///
/// 表示聊天权限检查的结果，包括允许和各种拒绝原因。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum PermissionResult {
    /// 允许（开放模式）
    Allowed,

    /// 允许（有场景授权）
    /// 包含有效的场景类型列表
    AllowedByScene(sp_std::vec::Vec<SceneType>),

    /// EN: Denied: receiver requires a contact relationship (off-chain capability
    /// token). Returned for `FriendsOnly` / `Whitelist` levels when there is no
    /// valid scene authorization. CN: 拒绝：接收方要求联系人关系（链下能力令牌）。
    /// 在 `FriendsOnly` / `Whitelist` 级别且无有效场景授权时返回。
    ///
    /// NOTE (audit cleanup): the former `DeniedBlocked` / `DeniedNotInWhitelist`
    /// variants were removed — they were unreachable after the on-chain
    /// blocklist/whitelist were dropped (audit P1). Blocking is off-chain now, so
    /// `check_permission` never produced them. `PermissionResult` is a transient
    /// runtime-API result (not stored), so removing dead variants is safe.
    /// 注（审计清理）：原 `DeniedBlocked` / `DeniedNotInWhitelist` 变体已删除——
    /// 链上黑/白名单移除后（审计 P1）它们不可达，`check_permission` 从不返回；本类型
    /// 仅为 runtime API 瞬时返回值（不入存储），删除死变体安全。
    DeniedRequiresFriend,

    /// 拒绝：对方已关闭聊天
    DeniedClosed,

    /// EN: Denied because the *sender* is platform-muted by governance.
    /// CN: 拒绝：发送方被治理平台级禁言。
    DeniedSenderMuted,
}

impl PermissionResult {
    /// 检查是否允许聊天
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            PermissionResult::Allowed | PermissionResult::AllowedByScene(_)
        )
    }
}

/// 场景授权详情（用于 Runtime API 返回）
///
/// 简化的场景授权信息，用于前端查询。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct SceneAuthorizationInfo {
    /// 场景类型
    pub scene_type: SceneType,

    /// 场景标识
    pub scene_id: SceneId,

    /// 是否已过期
    pub is_expired: bool,

    /// 过期时间（区块号）
    pub expires_at: Option<u64>,

    /// 元数据（字节数组）
    pub metadata: sp_std::vec::Vec<u8>,
}

/// EN: Platform-level mute status of an account, set by governance. A muted
/// account is denied as a chat *sender* (`check_permission` → `DeniedSenderMuted`)
/// until the mute is lifted or (for `Until`) expires.
/// CN: 由治理设置的账户平台级禁言状态。被禁言账户作为聊天**发送方**会被拒绝
/// （`check_permission` → `DeniedSenderMuted`），直到解除或（`Until`）到期。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum MuteStatus<BlockNumber> {
    /// EN: Muted indefinitely (until governance unmutes). CN: 无限期禁言（直至治理解除）。
    Forever,
    /// EN: Muted until the given block (exclusive). CN: 禁言至给定区块（不含）。
    Until(BlockNumber),
}

/// EN: Subject of a user report (for compliance / evidence). Only references
/// (ids / hashes) live on-chain — no plaintext, consistent with the chat
/// module's off-chain content model. CN: 用户举报的对象（合规 / 存证）。链上仅存
/// 引用（id / 哈希），无明文，与聊天模块的链下内容模型一致。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub enum ReportTarget<T: crate::Config> {
    /// EN: Report an account. CN: 举报某账户。
    Account(T::AccountId),
    /// EN: Report a group by id. CN: 举报某群（按 id）。
    Group(u64),
    /// EN: Report a specific message by its on-chain digest/hash. CN: 举报某条消息（按链上 digest/哈希）。
    Message([u8; 32]),
}

/// EN: An on-chain report record kept for governance review. The reason is an
/// IPFS CID (evidence stored off-chain); records are removed by governance via
/// `resolve_report`. CN: 供治理审阅的链上举报记录。理由为 IPFS CID（证据存链下）；
/// 记录由治理经 `resolve_report` 移除。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    DebugNoBound,
    TypeInfo,
    MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct ReportRecord<T: crate::Config> {
    /// EN: Account that filed the report. CN: 举报发起账户。
    pub reporter: T::AccountId,
    /// EN: What is being reported. CN: 被举报对象。
    pub target: ReportTarget<T>,
    /// EN: IPFS CID of the evidence bundle. CN: 证据包的 IPFS CID。
    pub reason_cid: BoundedVec<u8, T::MaxReportCidLen>,
    /// EN: Block at which the report was filed. CN: 举报发起区块。
    pub filed_at: frame_system::pallet_prelude::BlockNumberFor<T>,
}

/// 隐私设置摘要（用于 Runtime API 返回）
///
/// 简化的用户隐私设置信息，用于前端查询。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct PrivacySettingsSummary {
    /// 权限级别
    pub permission_level: ChatPermissionLevel,

    /// 拒绝的场景类型列表
    pub rejected_scene_types: sp_std::vec::Vec<SceneType>,
}
