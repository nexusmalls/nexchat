//! Domain types and enums
//!
//! All types previously defined in lib.rs, now organized into this submodule.

use codec::{Decode, Encode, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;

/// Pool reward cap behavior shared across governance/commission layers
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub enum PoolRewardCapBehavior {
    Fixed,
    UnlockByTeam {
        direct_per_unlock: u32,
        team_per_unlock: u32,
        unlock_percent: u16,
    },
}

/// Pool reward per-level claim rule shared across governance/commission layers
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub struct PoolRewardLevelClaimRule {
    pub base_cap_percent: u16,
    pub cap_behavior: PoolRewardCapBehavior,
    pub baseline_direct: u32,
    pub baseline_team: u32,
}

// ============================================================================
// 实体类型枚举 (Phase 2 新增)
// ============================================================================

/// 实体类型
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum EntityType {
    /// 商户（原 Shop，默认类型）
    #[default]
    Merchant,
    /// 企业
    Enterprise,
    /// 去中心化自治组织
    DAO,
    /// 社区
    Community,
    /// 项目方
    Project,
    /// 服务提供商
    ServiceProvider,
    /// 基金
    Fund,
    /// 自定义类型
    ///
    /// **已废弃**: Custom(u8) 无注册/验证机制，所有辅助方法均回退到最宽松默认值，
    /// 相当于绕过策略的后门。请使用 7 种内置类型代替。
    #[deprecated(note = "Custom(u8) 无验证机制，所有辅助方法回退到最宽松默认值。请使用内置类型")]
    Custom(u8),
}

impl EntityType {
    /// 默认治理模式（创建实体时的建议值）
    pub fn default_governance(&self) -> GovernanceMode {
        match self {
            Self::DAO => GovernanceMode::FullDAO,
            Self::Enterprise => GovernanceMode::MultiSig,
            Self::Fund | Self::Community => GovernanceMode::Council,
            Self::Project => GovernanceMode::FullDAO,
            _ => GovernanceMode::None,
        }
    }

    /// 默认代币类型（创建实体时的建议值）
    #[allow(deprecated)]
    pub fn default_token_type(&self) -> TokenType {
        match self {
            Self::Merchant | Self::ServiceProvider => TokenType::Points,
            Self::Enterprise => TokenType::Equity,
            Self::DAO => TokenType::Governance,
            Self::Community => TokenType::Membership,
            Self::Project => TokenType::Share,
            Self::Fund => TokenType::Share,
            Self::Custom(_) => TokenType::Points,
        }
    }

    /// 是否默认需要 KYC（与治理模式无关）
    ///
    /// Enterprise/Fund/Project 因合规要求默认启用 KYC，
    /// 不论使用 FullDAO、MultiSig 或 Council 治理模式。
    pub fn requires_kyc_by_default(&self) -> bool {
        matches!(self, Self::Enterprise | Self::Fund | Self::Project)
    }

    /// 检查代币类型是否与实体类型匹配（仅为建议，不强制）
    /// 返回 true 表示推荐组合，false 表示不常见组合但仍允许
    pub fn suggests_token_type(&self, token: &TokenType) -> bool {
        match (self, token) {
            // 商户/服务商通常不发行证券类代币
            (Self::Merchant | Self::ServiceProvider, TokenType::Equity | TokenType::Bond) => false,
            // DAO 通常使用治理代币
            (Self::DAO, TokenType::Points | TokenType::Membership) => false,
            // 基金通常使用份额类代币
            (Self::Fund, TokenType::Points | TokenType::Governance) => false,
            _ => true,
        }
    }

    /// 检查治理模式是否与实体类型匹配（仅为建议，不强制）
    /// 返回 true 表示推荐组合，false 表示不常见组合但仍允许
    pub fn suggests_governance(&self, mode: &GovernanceMode) -> bool {
        !matches!(
            (self, mode),
            (Self::DAO, GovernanceMode::None)
                | (Self::Fund, GovernanceMode::FullDAO)
                | (Self::Enterprise, GovernanceMode::FullDAO)
        )
    }

    /// 默认转账限制模式
    #[allow(deprecated)]
    pub fn default_transfer_restriction(&self) -> TransferRestrictionMode {
        match self {
            Self::Merchant | Self::ServiceProvider | Self::Community => {
                TransferRestrictionMode::None
            }
            Self::Enterprise | Self::Fund => TransferRestrictionMode::Whitelist,
            Self::DAO => TransferRestrictionMode::None,
            Self::Project => TransferRestrictionMode::KycRequired,
            Self::Custom(_) => TransferRestrictionMode::None,
        }
    }
}

/// 治理模式
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum GovernanceMode {
    /// 无治理（管理员全权控制，可 lock_governance 锁定参数）
    #[default]
    None,
    /// 完全 DAO（所有决策需代币投票，可选管理员否决权）
    FullDAO,
    /// 多签治理（N-of-M 签名者共同决策，适合 Enterprise）
    MultiSig,
    /// 理事会治理（选举/任命理事会成员投票，适合 Fund/Community）
    Council,
}

// ============================================================================
// 实体相关类型
// ============================================================================

/// 实体状态（Entity 组织层状态）
///
/// **Default = Pending**：`create_entity` 内部显式设为 `Active`，
/// `reopen_entity` 使用 `Default` 即 `Pending` 进入审核流程。
/// 不要依赖 `EntityStatus::default()` 初始化新建实体。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum EntityStatus {
    /// 待审核（reopen_entity 重新开业时使用，新建 create_entity 跳过此状态直接 Active）
    #[default]
    Pending,
    /// 正常运营
    Active,
    /// 暂停运营（管理员主动）
    Suspended,
    /// 被封禁（治理处罚）
    Banned,
    /// 已关闭
    Closed,
    /// 待关闭审批（owner 申请关闭，等待治理批准）
    PendingClose,
}

impl EntityStatus {
    /// 是否正常运营
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 是否为终态（不可恢复）
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Banned | Self::Closed)
    }

    /// 是否处于待定状态
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending | Self::PendingClose)
    }
}

// ============================================================================
// Shop 有效状态（实时计算，不存储）
// ============================================================================

/// Shop 有效运营状态（由 EntityStatus + ShopOperatingStatus 实时计算）
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub enum EffectiveShopStatus {
    /// 正常营业
    Active,
    /// Shop 自身暂停（manager 主动）
    PausedBySelf,
    /// Entity 暂停导致不可运营
    PausedByEntity,
    /// Shop 资金耗尽
    FundDepleted,
    /// Shop 已关闭（自身关闭）
    Closed,
    /// Entity 已关闭/封禁，Shop 强制关闭
    ClosedByEntity,
    /// Shop 关闭中（宽限期）
    Closing,
    /// Shop 被治理层封禁
    Banned,
}

impl EffectiveShopStatus {
    /// 是否可运营（接单、上架等）
    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 是否因 Entity 导致不可用
    pub fn is_entity_caused(&self) -> bool {
        matches!(self, Self::PausedByEntity | Self::ClosedByEntity)
    }

    /// 计算有效状态
    pub fn compute(entity_status: &EntityStatus, shop_status: &ShopOperatingStatus) -> Self {
        // 1. Entity 终态优先（Banned/Closed → Shop 强制关闭）
        match entity_status {
            EntityStatus::Banned | EntityStatus::Closed => {
                return Self::ClosedByEntity;
            }
            EntityStatus::Suspended | EntityStatus::PendingClose | EntityStatus::Pending => {
                // Entity 非 Active → Shop 不可运营
                // 如果 Shop 自身已 Closed/Closing，优先显示对应状态（终态不可逆）
                if matches!(shop_status, ShopOperatingStatus::Closed) {
                    return Self::Closed;
                }
                if matches!(shop_status, ShopOperatingStatus::Closing) {
                    return Self::Closing;
                }
                if matches!(shop_status, ShopOperatingStatus::Banned) {
                    return Self::Banned;
                }
                return Self::PausedByEntity;
            }
            EntityStatus::Active => { /* 继续判断 Shop 自身状态 */ }
        }

        // 2. Entity Active → 看 Shop 自身状态
        match shop_status {
            ShopOperatingStatus::Active => Self::Active,
            ShopOperatingStatus::Paused => Self::PausedBySelf,
            ShopOperatingStatus::FundDepleted => Self::FundDepleted,
            ShopOperatingStatus::Closed => Self::Closed,
            ShopOperatingStatus::Closing => Self::Closing,
            ShopOperatingStatus::Banned => Self::Banned,
        }
    }
}

// ============================================================================
// Shop 类型枚举 (Entity-Shop 分离架构)
// ============================================================================

/// Shop 类型
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum ShopType {
    /// 线上商城（默认）
    #[default]
    OnlineStore,
    /// 实体门店
    PhysicalStore,
    /// 服务网点
    ServicePoint,
    /// 仓储/自提点
    ///
    /// **v0.9.0 废弃预告**: 初期上线低频类型，建议使用 PhysicalStore + 链下标记替代。
    Warehouse,
    /// 加盟店
    ///
    /// **v0.9.0 废弃预告**: 初期上线低频类型，建议使用 OnlineStore + 链下标记替代。
    Franchise,
    /// 快闪店/临时店
    ///
    /// **v0.9.0 废弃预告**: 初期上线低频类型，建议使用 PhysicalStore + 链下标记替代。
    Popup,
    /// 虚拟店铺（纯服务）
    Virtual,
}

impl ShopType {
    /// 是否需要地理位置
    pub fn requires_location(&self) -> bool {
        matches!(
            self,
            Self::PhysicalStore | Self::ServicePoint | Self::Warehouse | Self::Popup
        )
    }

    /// 是否支持实物商品
    pub fn supports_physical_products(&self) -> bool {
        matches!(
            self,
            Self::OnlineStore | Self::PhysicalStore | Self::Warehouse | Self::Franchise
        )
    }

    /// 是否支持服务类商品
    pub fn supports_services(&self) -> bool {
        matches!(self, Self::ServicePoint | Self::Virtual | Self::OnlineStore)
    }
}

/// Shop 状态（业务层状态）
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum ShopOperatingStatus {
    /// 营业中
    #[default]
    Active,
    /// 暂停营业
    Paused,
    /// 资金耗尽（自动暂停）
    FundDepleted,
    /// 已关闭
    Closed,
    /// 关闭中（宽限期内）
    Closing,
    /// 被治理层封禁（仅 Root 可解封）
    Banned,
}

impl ShopOperatingStatus {
    /// 是否可以进行业务操作
    pub fn is_operational(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 是否可以恢复营业
    pub fn can_resume(&self) -> bool {
        matches!(self, Self::Paused | Self::FundDepleted)
    }

    /// 是否处于关闭/关闭中状态（终态或准终态）
    pub fn is_closed_or_closing(&self) -> bool {
        matches!(self, Self::Closed | Self::Closing)
    }

    /// 是否被封禁
    pub fn is_banned(&self) -> bool {
        matches!(self, Self::Banned)
    }

    /// 是否处于不可管理状态（关闭/关闭中/封禁）
    pub fn is_terminal_or_banned(&self) -> bool {
        matches!(self, Self::Closed | Self::Closing | Self::Banned)
    }
}

// ============================================================================
// 会员注册策略（位标记）
// ============================================================================

/// 会员注册策略（位标记，可组合）
///
/// - `0b0000_0000` = 开放注册（默认）
/// - `PURCHASE_REQUIRED` = 必须先消费才能成为会员（手动注册被拒）
/// - `REFERRAL_REQUIRED` = 必须提供推荐人
/// - `APPROVAL_REQUIRED` = 需要 Entity owner 审批
/// - `KYC_REQUIRED` = 注册时需要通过 KYC 认证
/// - `KYC_UPGRADE_REQUIRED` = 等级升级时需要通过 KYC 认证
///
/// 支持组合，例如 `PURCHASE_REQUIRED | REFERRAL_REQUIRED` = 必须消费且有推荐人
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub struct MemberRegistrationPolicy(pub u8);

impl MemberRegistrationPolicy {
    /// 开放注册（无限制）
    pub const OPEN: Self = Self(0);
    /// 必须先消费（auto_register 触发）才能成为会员
    pub const PURCHASE_REQUIRED: u8 = 0b0000_0001;
    /// 必须提供推荐人
    pub const REFERRAL_REQUIRED: u8 = 0b0000_0010;
    /// 需要 Entity owner 审批（注册后进入 Pending 状态）
    pub const APPROVAL_REQUIRED: u8 = 0b0000_0100;
    /// 注册时需要通过 KYC 认证
    pub const KYC_REQUIRED: u8 = 0b0000_1000;
    /// 等级升级时需要通过 KYC 认证
    pub const KYC_UPGRADE_REQUIRED: u8 = 0b0001_0000;
    /// 所有已定义标记位的并集
    pub const ALL_VALID: u8 = Self::PURCHASE_REQUIRED
        | Self::REFERRAL_REQUIRED
        | Self::APPROVAL_REQUIRED
        | Self::KYC_REQUIRED
        | Self::KYC_UPGRADE_REQUIRED;

    /// 检查是否设置了指定标记
    pub fn contains(&self, flag: u8) -> bool {
        self.0 & flag == flag
    }

    /// 检查策略值是否仅包含已定义的位
    pub fn is_valid(&self) -> bool {
        self.0 & !Self::ALL_VALID == 0
    }

    /// 是否为开放注册（无任何限制）
    pub fn is_open(&self) -> bool {
        self.0 == 0
    }

    /// 是否要求购买
    pub fn requires_purchase(&self) -> bool {
        self.contains(Self::PURCHASE_REQUIRED)
    }

    /// 是否要求推荐人
    pub fn requires_referral(&self) -> bool {
        self.contains(Self::REFERRAL_REQUIRED)
    }

    /// 是否要求审批
    pub fn requires_approval(&self) -> bool {
        self.contains(Self::APPROVAL_REQUIRED)
    }

    /// 注册时是否要求 KYC
    pub fn requires_kyc(&self) -> bool {
        self.contains(Self::KYC_REQUIRED)
    }

    /// 升级时是否要求 KYC
    pub fn requires_kyc_for_upgrade(&self) -> bool {
        self.contains(Self::KYC_UPGRADE_REQUIRED)
    }
}

impl Default for MemberRegistrationPolicy {
    fn default() -> Self {
        Self(Self::PURCHASE_REQUIRED | Self::REFERRAL_REQUIRED)
    }
}

// ============================================================================
// 通证类型枚举 (Phase 2 新增)
// ============================================================================

/// 通证类型
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum TokenType {
    /// 积分（原默认类型，消费奖励）
    #[default]
    Points,
    /// 治理代币（投票权）
    Governance,
    /// 股权代币（分红权）
    Equity,
    /// 会员代币（会员资格）
    Membership,
    /// 份额代币（基金份额）
    Share,
    /// 债券代币（固定收益）
    Bond,
    /// 混合型（投票权 + 分红权）
    Hybrid,
}

impl TokenType {
    /// 是否具有投票权
    pub fn has_voting_power(&self) -> bool {
        matches!(self, Self::Governance | Self::Equity | Self::Hybrid)
    }

    /// 是否具有分红权
    pub fn has_dividend_rights(&self) -> bool {
        matches!(self, Self::Equity | Self::Share | Self::Hybrid)
    }

    /// 是否可转让（默认可转让）
    pub fn is_transferable_by_default(&self) -> bool {
        !matches!(self, Self::Membership)
    }

    /// 获取默认要求的 KYC 级别
    /// 返回 (持有者 KYC, 接收方 KYC)
    pub fn required_kyc_level(&self) -> (u8, u8) {
        match self {
            Self::Points => (0, 0),             // None, None
            Self::Membership => (1, 1),         // Basic, Basic
            Self::Governance => (2, 2),         // Standard, Standard
            Self::Share | Self::Bond => (2, 2), // Standard, Standard
            Self::Equity => (3, 3),             // Enhanced, Enhanced
            Self::Hybrid => (2, 2),             // Standard, Standard (默认)
        }
    }

    /// 是否为证券类型（需要严格合规）
    pub fn is_security(&self) -> bool {
        matches!(self, Self::Equity | Self::Share | Self::Bond)
    }

    /// 是否需要强制披露
    pub fn requires_disclosure(&self) -> bool {
        matches!(self, Self::Equity | Self::Share | Self::Bond)
    }

    /// 默认转账限制模式
    pub fn default_transfer_restriction(&self) -> TransferRestrictionMode {
        match self {
            Self::Points => TransferRestrictionMode::None,
            Self::Membership => TransferRestrictionMode::MembersOnly,
            Self::Governance => TransferRestrictionMode::KycRequired,
            Self::Share | Self::Bond => TransferRestrictionMode::KycRequired,
            Self::Equity => TransferRestrictionMode::Whitelist,
            Self::Hybrid => TransferRestrictionMode::None,
        }
    }
}

/// 转账限制模式
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum TransferRestrictionMode {
    /// 无限制（默认）
    #[default]
    None,
    /// 白名单模式 - 只能转给白名单地址
    Whitelist,
    /// 黑名单模式 - 禁止转给黑名单地址
    Blacklist,
    /// KYC 模式 - 接收方需满足 KYC 要求
    KycRequired,
    /// 闭环模式 - 只能在实体成员间转账
    MembersOnly,
}

impl TransferRestrictionMode {
    /// 安全转换（未知值返回 None 而非静默回退）
    pub fn try_from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::None),
            1 => Some(Self::Whitelist),
            2 => Some(Self::Blacklist),
            3 => Some(Self::KycRequired),
            4 => Some(Self::MembersOnly),
            _ => Option::None,
        }
    }
}

/// 分红配置
///
/// **设计说明：** `last_distribution` 和 `accumulated` 为运行时状态字段，
/// 理想情况下应作为独立存储项放在 `pallet-entity-token` 中。
/// 因当前已嵌入 `EntityTokenConfig` 存储结构，修改需存储迁移，暂保留。
/// 新代码应使用 `DividendState` 存储运行时状态。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub struct DividendConfig<Balance, BlockNumber> {
    /// 是否启用分红
    pub enabled: bool,
    /// 最小分红周期（区块数）
    pub min_period: BlockNumber,
    /// 上次分红时间
    ///
    /// **迁移提示**: 运行时状态不应嵌入配置结构体。
    /// 新代码请使用 `DividendState::last_distribution`，此字段保留仅为存储兼容。
    pub last_distribution: BlockNumber,
    /// 累计待分配金额
    ///
    /// **迁移提示**: 运行时状态不应嵌入配置结构体。
    /// 新代码请使用 `DividendState::accumulated`，此字段保留仅为存储兼容。
    pub accumulated: Balance,
}

/// 分红运行时状态（与 DividendConfig 配置分离）
///
/// 新模块应将配置（enabled/min_period）和状态（last_distribution/accumulated）
/// 存储在不同的 StorageMap 中，避免频繁写入完整配置。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub struct DividendState<Balance, BlockNumber> {
    /// 上次分红区块号
    pub last_distribution: BlockNumber,
    /// 累计待分配金额
    pub accumulated: Balance,
    /// 累计已分配总额
    pub total_distributed: Balance,
    /// 分红轮次计数
    pub round_count: u32,
}

// ============================================================================
// 服务/商品相关类型
// ============================================================================

/// 商品状态
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum ProductStatus {
    /// 草稿（未上架）
    #[default]
    Draft,
    /// 在售
    OnSale,
    /// 售罄
    SoldOut,
    /// 已下架
    OffShelf,
}

/// 商品可见性
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum ProductVisibility {
    /// 公开（所有人可见）
    #[default]
    Public,
    /// 仅会员可见/可购买
    MembersOnly,
    /// 等级门槛（达到指定等级才能购买）
    LevelGated(u8),
}

/// 商品类别
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum ProductCategory {
    /// 数字商品（虚拟物品）
    Digital,
    /// 实物商品
    #[default]
    Physical,
    /// 服务类
    Service,
    /// 订阅制商品（周期性付费）
    Subscription,
    /// 组合包（多个商品打包）
    Bundle,
    /// 其他
    Other,
}

// ============================================================================
// 订单相关类型
// ============================================================================

/// 订单状态
///
/// **编码兼容性**: SCALE 按声明顺序分配序号（Created=0, Paid=1, ...）。
/// 新增变体必须追加到末尾，禁止插入中间，否则已存储的状态值将解码错误。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum OrderStatus {
    /// 已创建，待支付
    #[default]
    Created,
    /// 已支付，待发货
    Paid,
    /// 已发货，待收货
    Shipped,
    /// 已完成
    Completed,
    /// 已取消（买家取消）
    Cancelled,
    /// 争议中
    Disputed,
    /// 已退款（全额）
    Refunded,
    /// 已过期（支付超时）
    Expired,
    // ---- v0.9.0 新增（追加到末尾，保持 SCALE 编码兼容） ----
    /// 处理中（数字商品/服务类：Paid 后自动流转，无需 Shipped）
    Processing,
    /// 待确认收货（买家确认窗口，超时自动 Completed）
    AwaitingConfirmation,
    /// 部分退款（从 Completed 状态发起部分退款后的终态）
    PartiallyRefunded,
}

// ============================================================================
// 会员状态枚举
// ============================================================================

/// 会员状态
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum MemberStatus {
    /// 正常活跃
    #[default]
    Active,
    /// 待审批（APPROVAL_REQUIRED 策略时）
    Pending,
    /// 暂时冻结（管理员操作）
    Frozen,
    /// 永久封禁
    Banned,
    /// 有效期已到期
    Expired,
}

impl MemberStatus {
    /// 是否可正常参与实体活动
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 是否为受限状态（不可参与活动）
    pub fn is_restricted(&self) -> bool {
        matches!(self, Self::Frozen | Self::Banned | Self::Expired)
    }

    /// 是否可恢复为活跃
    pub fn can_reactivate(&self) -> bool {
        matches!(self, Self::Frozen | Self::Expired)
    }

    /// 是否为待审批状态
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

// ============================================================================
// 争议相关类型
// ============================================================================

/// 争议状态（跨模块共享）
///
/// 由 dispute 模块设置，供 order/commission/review 等模块查询
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum DisputeStatus {
    /// 无争议
    #[default]
    None,
    /// 已提交，等待响应
    Submitted,
    /// 被投诉方已响应
    Responded,
    /// 调解中
    Mediating,
    /// 仲裁中
    Arbitrating,
    /// 已解决
    Resolved,
    /// 已撤销
    Withdrawn,
    /// 已过期
    Expired,
}

impl DisputeStatus {
    /// 是否处于活跃争议状态（未解决）
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Submitted | Self::Responded | Self::Mediating | Self::Arbitrating
        )
    }

    /// 是否已终结
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Resolved | Self::Withdrawn | Self::Expired)
    }
}

/// 争议裁决结果
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub enum DisputeResolution {
    /// 投诉方胜诉（全额退款）
    ComplainantWin,
    /// 被投诉方胜诉（全额放款）
    RespondentWin,
    /// 和解（双方协商，无具体比例）
    Settlement,
    /// 按比例裁决（仲裁员指定投诉方获得的比例）
    ///
    /// `complainant_share_bps`: 投诉方获得比例（基点，0..=10000，10000 = 100%）
    PartialSettlement { complainant_share_bps: u16 },
}

impl DisputeResolution {
    /// 校验裁决参数是否合法
    ///
    /// `PartialSettlement` 的 `complainant_share_bps` 必须在 0..=10000 范围内。
    pub fn is_valid(&self) -> bool {
        match self {
            Self::PartialSettlement {
                complainant_share_bps,
            } => *complainant_share_bps <= 10000,
            _ => true,
        }
    }

    /// 获取投诉方获赔比例（基点）
    ///
    /// - `ComplainantWin` → 10000 (100%)
    /// - `RespondentWin` → 0
    /// - `Settlement` → 5000 (50%，默认对半)
    /// - `PartialSettlement` → 指定值
    pub fn complainant_share_bps(&self) -> u16 {
        match self {
            Self::ComplainantWin => 10000,
            Self::RespondentWin => 0,
            Self::Settlement => 5000,
            Self::PartialSettlement {
                complainant_share_bps,
            } => *complainant_share_bps,
        }
    }
}

// ============================================================================
// Token Sale 相关类型
// ============================================================================

/// Token Sale 状态（跨模块共享）
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
    Default,
)]
pub enum TokenSaleStatus {
    /// 未开始
    #[default]
    NotStarted,
    /// 发售进行中
    Active,
    /// 已暂停
    Paused,
    /// 已结束
    Ended,
    /// 已取消
    Cancelled,
    /// 已完成（全部售出或手动完成）
    Completed,
}

impl TokenSaleStatus {
    /// 是否可以购买
    pub fn is_purchasable(&self) -> bool {
        matches!(self, Self::Active)
    }
}

// ============================================================================
// Admin 权限位掩码
// ============================================================================

/// Admin 细粒度权限（位掩码模式）
///
/// 每个 admin 绑定一个 `u32` 权限值，通过 `&` 运算检查是否拥有特定权限。
/// Owner 天然拥有全部权限，不受此掩码限制。
#[allow(non_snake_case)]
pub mod AdminPermission {
    /// Shop 管理（创建/更新/暂停 Shop、产品管理）
    pub const SHOP_MANAGE: u32 = 0b0000_0001;
    /// 会员等级管理（等级系统、升级规则、会员审批）
    pub const MEMBER_MANAGE: u32 = 0b0000_0010;
    /// Token 发售管理（创建/结束 tokensale）
    pub const TOKEN_MANAGE: u32 = 0b0000_0100;
    /// 广告管理（广告位注册/广告活动管理）
    pub const ADS_MANAGE: u32 = 0b0000_1000;
    /// 评论管理（开关评论系统）
    pub const REVIEW_MANAGE: u32 = 0b0001_0000;
    /// 披露/公告管理（配置披露、发布公告、内幕人员管理）
    pub const DISCLOSURE_MANAGE: u32 = 0b0010_0000;
    /// 实体管理（更新实体信息、充值资金）
    pub const ENTITY_MANAGE: u32 = 0b0100_0000;
    /// KYC 要求管理（设置实体 KYC 要求配置）
    pub const KYC_MANAGE: u32 = 0b1000_0000;
    /// 治理提案管理（创建/投票/执行提案）
    pub const GOVERNANCE_MANAGE: u32 = 0b0001_0000_0000;
    /// 订单管理（退款审批、争议处理）
    pub const ORDER_MANAGE: u32 = 0b0010_0000_0000;
    /// 佣金配置管理（返佣模式、费率设置）
    pub const COMMISSION_MANAGE: u32 = 0b0100_0000_0000;
    /// 商品管理（定价、库存、上下架，独立于 SHOP_MANAGE）
    pub const PRODUCT_MANAGE: u32 = 0b1000_0000_0000;
    /// 市场/交易管理（挂单管理、交易对配置）
    pub const MARKET_MANAGE: u32 = 0b0001_0000_0000_0000;
    /// 全部权限
    pub const ALL: u32 = 0xFFFF_FFFF;
    /// 所有已定义权限位的并集（用于校验合法性）
    pub const ALL_DEFINED: u32 = SHOP_MANAGE
        | MEMBER_MANAGE
        | TOKEN_MANAGE
        | ADS_MANAGE
        | REVIEW_MANAGE
        | DISCLOSURE_MANAGE
        | ENTITY_MANAGE
        | KYC_MANAGE
        | GOVERNANCE_MANAGE
        | ORDER_MANAGE
        | COMMISSION_MANAGE
        | PRODUCT_MANAGE
        | MARKET_MANAGE;

    /// 检查权限值是否仅包含已定义的位
    pub fn is_valid(permissions: u32) -> bool {
        permissions & !ALL_DEFINED == 0
    }
}

// ============================================================================
// 订单 Hook DTO (Phase 5.3)
// ============================================================================

/// 订单完成信息（传递给 OnOrderCompleted Hook 链）
///
/// 包含完成订单后所有副作用所需的上下文信息，
/// 避免 Hook 实现方反向查询 Order 存储。
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct OrderCompletionInfo<AccountId, Balance> {
    pub order_id: u64,
    pub entity_id: u64,
    pub shop_id: u64,
    pub product_id: u64,
    pub buyer: AccountId,
    pub seller: AccountId,
    pub payer: Option<AccountId>,
    pub quantity: u32,
    pub payment_asset: super::traits::core::PaymentAsset,
    // NEX 相关（Native 支付时有值）
    pub nex_total_amount: Balance,
    pub nex_platform_fee: Balance,
    pub nex_seller_received: Balance,
    /// 卖家账户上锁定的佣金资金（reserve），
    /// commission engine 应使用 repatriate_reserved 扣佣，完成后 unreserve 剩余。
    pub nex_reserved_for_commission: Balance,
    // Token 相关（EntityToken 支付时有值）
    pub token_payment_amount: u128,
    pub token_platform_fee: u128,
    pub token_seller_received: u128,
    /// P0-5 审计修复: Token 平台费是否实际转账成功
    /// false 时 commission hook 应传 0 作为 fee，避免"账面有承诺、实际没钱"
    pub token_platform_fee_paid: bool,
    // 会员相关
    pub referrer: Option<AccountId>,
    pub amount_usdt: u64,
    pub product_category: ProductCategory,
    // 购物余额支付（ShoppingBalance 支付时有值）
    /// 购物余额全额支付金额（NEX 等价）
    pub shopping_balance_used: Balance,
}

/// 订单取消信息（传递给 OnOrderCancelled Hook 链）
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct OrderCancellationInfo {
    pub order_id: u64,
    pub entity_id: u64,
    pub shop_id: u64,
    pub payment_asset: super::traits::core::PaymentAsset,
}

// ============================================================================
// PaymentConfig — Entity 级支付通道配置
// ============================================================================

/// Entity 级支付通道配置
///
/// 控制 Entity 下的店铺订单可使用哪些支付通道。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub struct PaymentConfig {
    /// 是否启用 NEX 原生代币支付
    pub native_enabled: bool,
    /// 是否启用 Entity Token 支付
    pub token_enabled: bool,
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            native_enabled: true,
            token_enabled: false,
        }
    }
}
