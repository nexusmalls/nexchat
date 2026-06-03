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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
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

    /// 1:1 direct chat scene: two users may message each other directly.
    /// 直聊场景：两名用户可直接 1:1 通信。
    ///
    /// Granted when a 1:1 conversation is established (treated as a 2-member
    /// MLS group under the chat-core × MLS convergence). This is the
    /// single-source authorization for direct messaging, replacing chat-core's
    /// former self-managed blacklist / stranger checks.
    /// 当建立 1:1 会话（收敛方案中视为 2 人 MLS 群）时授予。作为直聊的
    /// 单一授权来源，取代 chat-core 旧版自管的黑名单 / 陌生人校验。
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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen, Default)]
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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
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

    /// 额外元数据（如订单金额、纪念馆名称等，用于前端显示）
    /// 最大128字节
    pub metadata: BoundedVec<u8, ConstU32<128>>,
}

/// 聊天权限级别枚举
///
/// 定义用户的基础聊天权限策略，决定陌生人能否发起聊天。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen, Default)]
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

    /// 白名单：仅白名单用户可发起聊天
    Whitelist,

    /// 关闭：不接受任何消息
    Closed,
}

/// 用户隐私设置结构体
///
/// 存储用户的聊天权限配置，包括权限级别、黑白名单和拒绝的场景类型。
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, DebugNoBound, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct PrivacySettings<T: crate::Config> {
    /// 聊天权限级别
    pub permission_level: ChatPermissionLevel,

    /// 黑名单：被屏蔽的用户列表
    pub block_list: BoundedVec<T::AccountId, T::MaxBlockListSize>,

    /// 白名单：允许聊天的用户列表（仅在 Whitelist 模式下生效）
    pub whitelist: BoundedVec<T::AccountId, T::MaxWhitelistSize>,

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
            block_list: BoundedVec::default(),
            whitelist: BoundedVec::default(),
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

    /// 拒绝：已被屏蔽
    DeniedBlocked,

    /// 拒绝：需要好友关系
    DeniedRequiresFriend,

    /// 拒绝：不在白名单
    DeniedNotInWhitelist,

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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, DebugNoBound)]
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
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, DebugNoBound, TypeInfo, MaxEncodedLen)]
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

    /// 黑名单数量
    pub block_list_count: u32,

    /// 白名单数量
    pub whitelist_count: u32,

    /// 拒绝的场景类型列表
    pub rejected_scene_types: sp_std::vec::Vec<SceneType>,
}
