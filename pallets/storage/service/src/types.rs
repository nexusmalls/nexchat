/// 函数级详细中文注释：pallet-nexus-ipfs 类型定义模块
///
/// 本模块包含优化后的IPFS Pin管理系统的所有类型定义：
/// - 分层配置（PinTier, TierConfig）
/// - 主题信息（SubjectType, SubjectInfo）
/// - 健康巡检（HealthCheckTask, HealthStatus）
/// - 周期扣费（BillingTask, ChargeLayer）
/// - 统计数据（GlobalHealthStats）
extern crate alloc;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, BoundedVec};
use scale_info::TypeInfo;
use Debug;

// ============================================================================
// Subject 相关类型（CID归属管理）
// ============================================================================

/// 函数级详细中文注释：Subject类型枚举
///
/// 定义CID所属的业务域类型，用于：
/// 1. SubjectFunding账户派生（每个域独立的资金账户）
/// 2. 域级别的Pin优先级调度
/// 3. 费用统计和分析
/// 4. 分层存储策略配置（不同域不同副本数）
///
/// 类型说明：
/// - Evidence：证据类数据（法律文件、证明材料等）- 最高优先级
/// - Product：商品元数据（名称、图片、详情 CID）
/// - Entity：实体元数据（logo、描述、联系方式）
/// - Shop：店铺元数据（logo、描述、地址、营业时间、政策）
/// - General：通用存储（默认类型）
/// - Custom：自定义域（预留扩展）
///
/// 域ID映射（用于SubjectFunding账户派生）：
/// - Evidence = 0
/// - Product = 10
/// - Entity = 11
/// - Shop = 12
/// - General = 98
/// - Custom = 99
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum SubjectType {
    /// 证据类数据（最高优先级，Critical级别，永久保存）
    Evidence,
    /// 商品元数据（名称、图片、详情 CID）- 商品在售期间持久保存
    Product,
    /// 实体元数据（logo、描述、联系方式）- 实体存续期间持久保存
    Entity,
    /// 店铺元数据（logo、描述、地址、营业时间、政策）- 店铺存续期间持久保存
    Shop,
    /// 通用存储（默认类型）
    General,
    /// 自定义域（预留扩展）
    Custom(BoundedVec<u8, ConstU32<32>>),
}

impl SubjectType {
    /// SubjectType → 域名字节串（用于 DomainPins 索引）。
    pub fn to_domain_name(&self) -> Vec<u8> {
        match self {
            Self::Evidence => b"evidence".to_vec(),
            Self::Product => b"product".to_vec(),
            Self::Entity => b"entity".to_vec(),
            Self::Shop => b"shop".to_vec(),
            Self::General => b"general".to_vec(),
            Self::Custom(name) => name.to_vec(),
        }
    }

    /// 域名字节串 → SubjectType（已知域映射到枚举，未知域映射到 Custom）。
    pub fn from_domain(domain: &[u8]) -> Self {
        match domain {
            b"evidence" => Self::Evidence,
            b"product" => Self::Product,
            b"entity" => Self::Entity,
            b"shop" => Self::Shop,
            b"general" => Self::General,
            other => Self::Custom(BoundedVec::try_from(other.to_vec()).unwrap_or_default()),
        }
    }
}

impl Default for SubjectType {
    fn default() -> Self {
        Self::General
    }
}

/// Entity 国库扣费接口。
///
/// 由 entity-registry 实现，storage-service 在计费时通过此 trait
/// 尝试从 Entity 国库代付存储费，失败则 fallthrough 到下一层。
pub trait EntityFunding<AccountId, Balance> {
    /// 尝试从 entity_id 对应的国库转 `amount` 到 `dest`。
    ///
    /// - `Ok(true)` — 扣费成功
    /// - `Ok(false)` — Entity 不存在 / 非活跃 / 余额不足，调用方应 fallthrough
    /// - `Err(_)` — 转账本身出错（不应 fallthrough）
    fn try_charge_entity(
        entity_id: u64,
        amount: Balance,
        dest: &AccountId,
    ) -> Result<bool, sp_runtime::DispatchError>;
}

/// Noop 实现：不涉及 Entity 的场景直接 fallthrough。
impl<A, B> EntityFunding<A, B> for () {
    fn try_charge_entity(_: u64, _: B, _: &A) -> Result<bool, sp_runtime::DispatchError> {
        Ok(false)
    }
}

/// Subject信息结构体
///
/// 记录CID归属的详细信息。
///
/// 字段说明：
/// - subject_type：Subject类型（Evidence/Product/Entity/Shop/General等）
/// - subject_id：Subject ID
///
/// 使用场景：
/// - CidToSubject存储的Value
/// - 周期扣费时查找资金账户
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct SubjectInfo {
    /// Subject类型
    pub subject_type: SubjectType,
    /// Subject ID
    pub subject_id: u64,
}

/// 函数级详细中文注释：域配置结构体 - 新pallet域自动PIN机制
///
/// 功能：
/// - 记录域的基本信息和配置
/// - 控制域的自动PIN行为
/// - 映射域到SubjectType
///
/// 用途：
/// - 新业务pallet注册自己的域
/// - 治理管理域的配置
/// - 自动化内容发现和PIN
///
/// 示例：
/// ```text
/// DomainConfig {
///     auto_pin_enabled: true,        // 启用自动PIN
///     default_tier: PinTier::Standard, // 默认标准等级
///     subject_type_id: 10,           // 自定义类型ID
///     owner_pallet: b"pallet-dispute-evidence",  // 所属pallet
///     created_at: 12345,             // 注册时间
/// }
/// ```
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct DomainConfig {
    /// 域是否启用自动PIN
    pub auto_pin_enabled: bool,

    /// 默认Pin等级
    pub default_tier: PinTier,

    /// 域的SubjectType映射ID
    /// - 内置类型: 0=Evidence, 10=Product, 11=Entity, 12=Shop, 98=General
    /// - 自定义类型: 10-255（由治理分配）
    pub subject_type_id: u8,

    /// 域注册时的区块号
    pub created_at: u32,
}

impl Default for DomainConfig {
    fn default() -> Self {
        Self {
            auto_pin_enabled: true,
            default_tier: PinTier::Standard,
            subject_type_id: 99,
            created_at: 0,
        }
    }
}

// ============================================================================
// Pin 分层配置相关类型
// ============================================================================

/// 函数级详细中文注释：Pin分层等级枚举
///
/// 根据内容重要性，定义不同的Pin策略等级：
///
/// 等级说明：
/// - Critical（关键级）：
///   * 副本数：5个
///   * 巡检周期：6小时
///   * 费率系数：1.5x
///   * 适用场景：证据类数据、重要文件
///
/// - Standard（标准级）：
///   * 副本数：3个
///   * 巡检周期：24小时
///   * 费率系数：1.0x（基准）
///   * 适用场景：一般业务数据（默认）
///
/// - Temporary（临时级）：
///   * 副本数：1个
///   * 巡检周期：7天
///   * 费率系数：0.5x
///   * 适用场景：OTC聊天记录、临时媒体
///
/// 设计理念：
/// - 平衡存储成本与数据可靠性
/// - 关键数据高成本高可靠性
/// - 临时数据低成本低可靠性
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum PinTier {
    /// 关键级：5副本，6小时巡检
    Critical,
    /// 标准级：3副本，24小时巡检（默认）
    Standard,
    /// 临时级：1副本，7天巡检
    Temporary,
}

impl Default for PinTier {
    fn default() -> Self {
        Self::Standard
    }
}

/// 函数级详细中文注释：分层配置参数结构体
///
/// 定义每个Pin等级的具体参数，支持运行时动态调整（治理提案）。
///
/// 字段说明：
/// - replicas：副本数（1-10）
/// - health_check_interval：巡检周期（区块数）
/// - fee_multiplier：存储费率系数（基数10000，如10000=1.0x, 15000=1.5x）
/// - grace_period_blocks：宽限期（区块数）
/// - enabled：是否启用该等级
///
/// 默认值：
/// - Critical：5副本，7200块（6小时），1.5x费率，7天宽限期
/// - Standard：3副本，28800块（24小时），1.0x费率，7天宽限期
/// - Temporary：1副本，604800块（7天），0.5x费率，3天宽限期
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct TierConfig {
    /// 副本数（1-10）
    pub replicas: u32,
    /// 巡检周期（区块数）
    pub health_check_interval: u32,
    /// 存储费率系数（相对于基准费率的倍数，基数10000）
    /// 例如：10000=1.0x, 15000=1.5x, 5000=0.5x, 100000=10.0x
    /// 注意：使用u32以支持更大的费率系数（最大429万倍）
    pub fee_multiplier: u32,
    /// 宽限期（区块数）
    pub grace_period_blocks: u32,
    /// 是否启用该等级
    pub enabled: bool,
}

impl Default for TierConfig {
    /// 默认配置（Standard级别）
    fn default() -> Self {
        Self {
            replicas: 3,
            health_check_interval: 28800, // 24小时（假设3秒/块）
            fee_multiplier: 10000,        // 1.0x
            grace_period_blocks: 201600,  // 7天
            enabled: true,
        }
    }
}

impl TierConfig {
    /// 获取Critical级别的默认配置
    pub fn critical_default() -> Self {
        Self {
            replicas: 5,
            health_check_interval: 7200, // 6小时
            fee_multiplier: 15000,       // 1.5x
            grace_period_blocks: 201600, // 7天
            enabled: true,
        }
    }

    /// 获取Temporary级别的默认配置
    pub fn temporary_default() -> Self {
        Self {
            replicas: 1,
            health_check_interval: 604800, // 7天
            fee_multiplier: 5000,          // 0.5x
            grace_period_blocks: 86400,    // 3天
            enabled: true,
        }
    }
}

// ============================================================================
// 健康巡检相关类型
// ============================================================================

/// 函数级详细中文注释：健康巡检任务结构体
///
/// 记录每个CID的巡检状态和历史，用于：
/// 1. 自动调度巡检任务
/// 2. 追踪连续失败次数
/// 3. 动态调整巡检频率
///
/// 字段说明：
/// - tier：CID分层等级
/// - last_check：上次巡检时间（区块号）
/// - last_status：上次巡检结果
/// - consecutive_failures：连续失败次数（≥5次发送告警）
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub struct HealthCheckTask<BlockNumber> {
    /// CID分层等级
    pub tier: PinTier,
    /// 上次巡检时间
    pub last_check: BlockNumber,
    /// 上次巡检结果
    pub last_status: HealthStatus,
    /// 连续失败次数
    pub consecutive_failures: u8,
}

/// 函数级详细中文注释：健康状态枚举
///
/// 定义CID的健康状态，用于：
/// 1. 判断是否需要自动修复
/// 2. 调整巡检频率（降级/危险状态缩短间隔）
/// 3. 链上仪表板展示
///
/// 状态说明：
/// - Healthy：健康，副本数 >= 目标副本数
/// - Degraded：降级，副本数 < 目标但 >= 2（可用但冗余不足）
/// - Critical：危险，副本数 < 2（数据安全风险）
/// - Unknown：未知，巡检失败（网络错误等）
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum HealthStatus {
    /// 健康：副本数 >= 目标副本数
    Healthy { current_replicas: u32 },
    /// 降级：副本数 < 目标副本数，但 >= 最低阈值（2）
    Degraded { current_replicas: u32, target: u32 },
    /// 危险：副本数 < 2
    Critical { current_replicas: u32 },
    /// 未知：巡检失败（网络错误等）
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// 函数级详细中文注释：全局健康统计数据结构体
///
/// 记录全网IPFS存储的健康状态，用于：
/// 1. 链上Dashboard展示
/// 2. 治理决策参考
/// 3. 费率调整依据
///
/// 字段说明：
/// - total_pins：总Pin数量
/// - total_size_bytes：总存储量（字节）
/// - healthy_count：健康CID数量
/// - degraded_count：降级CID数量
/// - critical_count：危险CID数量
/// - last_full_scan：上次完整扫描时间
/// - total_repairs：累计修复次数
#[derive(
    Clone,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub struct GlobalHealthStats<BlockNumber> {
    /// 总Pin数量
    pub total_pins: u64,
    /// 总存储量（字节）
    pub total_size_bytes: u64,
    /// 健康CID数量
    pub healthy_count: u64,
    /// 降级CID数量
    pub degraded_count: u64,
    /// 危险CID数量
    pub critical_count: u64,
    /// 上次完整扫描时间
    pub last_full_scan: BlockNumber,
    /// 累计修复次数
    pub total_repairs: u64,
}

/// 函数级详细中文注释：域级别健康统计
///
/// 记录单个域的Pin数量、存储容量、健康状态等统计信息，用于：
/// 1. 域级别的监控和告警
/// 2. Dashboard按域展示统计数据
/// 3. 域级别的优先级调度决策
///
/// 字段说明：
/// - domain：域名（如 b"evidence", b"otc"）
/// - total_pins：该域的总Pin数量
/// - total_size_bytes：该域的总存储量（字节）
/// - healthy_count：健康CID数量
/// - degraded_count：降级CID数量
/// - critical_count：危险CID数量
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct DomainStats {
    /// 域名
    pub domain: BoundedVec<u8, ConstU32<32>>,
    /// 总Pin数量
    pub total_pins: u64,
    /// 总存储量（字节）
    pub total_size_bytes: u64,
    /// 健康CID数量
    pub healthy_count: u64,
    /// 降级CID数量
    pub degraded_count: u64,
    /// 危险CID数量
    pub critical_count: u64,
}

// ============================================================================
// 周期扣费相关类型
// ============================================================================

/// 函数级详细中文注释：扣费任务结构体
///
/// 记录每个CID的扣费状态和历史，用于：
/// 1. 自动调度周期扣费
/// 2. 追踪宽限期状态
/// 3. 记录扣费层级（调试用）
///
/// 字段说明：
/// - billing_period：扣费周期（区块数，如30天）
/// - amount_per_period：每周期费用
/// - last_charge：上次扣费时间
/// - grace_status：宽限期状态
/// - charge_layer：当前使用的扣费层级
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber, Balance))]
pub struct BillingTask<BlockNumber, Balance> {
    /// 扣费周期（区块数）
    pub billing_period: u32,
    /// 每周期费用
    pub amount_per_period: Balance,
    /// 上次扣费时间
    pub last_charge: BlockNumber,
    /// 宽限期状态
    pub grace_status: GraceStatus<BlockNumber>,
    /// 扣费层级（记录当前使用哪层资金）
    pub charge_layer: ChargeLayer,
}

/// 函数级详细中文注释：宽限期状态枚举
///
/// 定义CID的宽限期状态，用于：
/// 1. 判断是否需要Unpin
/// 2. 发送用户通知
/// 3. 前端显示倒计时
///
/// 状态说明：
/// - Normal：正常状态，扣费正常
/// - InGrace：宽限期中，记录进入时间和截止时间
/// - Expired：宽限期已过期，待Unpin
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub enum GraceStatus<BlockNumber> {
    /// 正常状态
    Normal,
    /// 宽限期中（记录进入时间、截止时间和重试次数）
    InGrace {
        entered_at: BlockNumber,
        expires_at: BlockNumber,
        retry_count: u32,
    },
    /// 宽限期已过期，待Unpin
    Expired,
}

impl<BlockNumber> Default for GraceStatus<BlockNumber> {
    fn default() -> Self {
        Self::Normal
    }
}

/// 函数级详细中文注释：充电层级枚举（三层回退机制）
///
/// 定义扣费的优先级顺序（三层机制）：
/// 1. IpfsPool：系统公共池（第一顺序）
/// 2. SubjectFunding：用户充值账户（第二顺序）
/// 3. GracePeriod：宽限期（不扣费，等待充值）
///
/// 设计理念：
/// - 优先从公共池扣费，确保运营者及时获得收益
/// - 用户账户作为第二层备份，补充公共池
/// - 宽限期保护用户，避免因短期余额不足导致数据丢失
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum ChargeLayer {
    /// 第1层：IpfsPoolAccount公共池（系统缓冲）
    IpfsPool,
    /// 第2层：SubjectFunding账户（用户充值）
    SubjectFunding,
    /// 第3层：宽限期（不扣费，等待充值）
    GracePeriod,
}

impl Default for ChargeLayer {
    fn default() -> Self {
        Self::IpfsPool
    }
}

/// 函数级详细中文注释：充电结果枚举
///
/// 定义三层回退充电的结果，用于：
/// 1. 判断是否需要进入宽限期
/// 2. 更新BillingTask状态
/// 3. 发送事件通知
///
/// 结果说明：
/// - Success：扣费成功，记录使用的层级
/// - EnterGrace：进入宽限期，记录过期时间
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
#[scale_info(skip_type_params(BlockNumber))]
pub enum ChargeResult<BlockNumber> {
    /// 扣费成功
    Success { layer: ChargeLayer },
    /// 进入宽限期
    EnterGrace { expires_at: BlockNumber },
}

// [已删除] ChargeStrategy 枚举：所有路径统一使用 four_layer_charge()，
// 不存在 QuotaFirst/UserFirst 分支选择，此枚举为死代码。

/// 函数级详细中文注释：Unpin原因枚举
///
/// 定义CID被Unpin的原因，用于：
/// 1. 审计和统计
/// 2. 用户通知
/// 3. 争议处理
///
/// 原因说明：
/// - InsufficientFunds：费用不足（宽限期已过）
/// - ManualRequest：用户手动请求Unpin
/// - GovernanceDecision：治理决定（违规内容等）
/// - OperatorOffline：运营者长期离线
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum UnpinReason {
    /// 费用不足（宽限期已过）
    InsufficientFunds,
    /// 用户手动请求Unpin
    ManualRequest,
    /// 治理决定（违规内容等）
    GovernanceDecision,
    /// 运营者长期离线
    OperatorOffline,
}

// ============================================================================
// 运营者分层架构相关类型（Layer 1/Layer 2）
// ============================================================================

/// 运营者层级枚举
///
/// - Core（Layer 1）：项目方运行，存储100%数据，最高优先级
/// - Community（Layer 2）：社区成员运行，选择性存储，通过链上奖励获利
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum OperatorLayer {
    /// Layer 1：核心运营者（项目方）
    Core,
    /// Layer 2：社区运营者
    Community,
}

impl Default for OperatorLayer {
    fn default() -> Self {
        Self::Core // 默认为核心层
    }
}

/// 分层存储策略配置
///
/// 定义不同数据类型和 PinTier 的分层副本分配策略。
/// 降级策略：可用运营者不足时优先满足 Layer 1，Layer 1 不足时从 Layer 2 补充，
/// 总副本数 < min_total_replicas 时拒绝 Pin 请求。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageLayerConfig {
    /// Layer 1核心运营者副本数（0-10）
    pub core_replicas: u32,
    /// Layer 2社区运营者副本数（0-10）
    pub community_replicas: u32,
    /// 最低总副本数（降级阈值，1-10）
    pub min_total_replicas: u32,
}

impl Default for StorageLayerConfig {
    /// 默认配置（标准数据）
    fn default() -> Self {
        Self {
            core_replicas: 2,
            community_replicas: 1,
            min_total_replicas: 1,
        }
    }
}

impl StorageLayerConfig {
    /// 获取证据数据的默认配置（最高安全）
    /// 适用于：Evidence - 法律证据、证明材料
    pub fn evidence_default() -> Self {
        Self {
            core_replicas: 5,
            community_replicas: 0,
            min_total_replicas: 3,
        }
    }

    /// 获取通用数据的默认配置（标准安全）
    /// 适用于：General - 通用存储
    pub fn general_default() -> Self {
        Self {
            core_replicas: 2,
            community_replicas: 1,
            min_total_replicas: 1,
        }
    }

    /// 获取临时数据的默认配置（低成本）
    /// 适用于：临时文件、缓存数据
    pub fn temporary_default() -> Self {
        Self {
            core_replicas: 1,
            community_replicas: 0,
            min_total_replicas: 1,
        }
    }

    /// 根据 SubjectType 获取推荐的存储配置
    pub fn for_subject_type(subject_type: &SubjectType) -> Self {
        match subject_type {
            SubjectType::Evidence => Self::evidence_default(),
            SubjectType::Product
            | SubjectType::Entity
            | SubjectType::Shop
            | SubjectType::General
            | SubjectType::Custom(_) => Self::general_default(),
        }
    }
}

/// 函数级详细中文注释：分层运营者选择结果结构体
///
/// 记录分层运营者选择算法的结果，包含：
/// 1. Layer 1（核心）运营者列表
/// 2. Layer 2（社区）运营者列表
///
/// 字段说明：
/// - core_operators：Layer 1运营者账户列表（最多16个）
/// - community_operators：Layer 2运营者账户列表（最多16个）
///
/// 选择逻辑：
/// 1. 从Layer 1池中按健康度排序，选择Top N
/// 2. 从Layer 2池中按健康度和容量使用率排序，选择Top M
/// 3. 如果某层运营者不足，发出告警事件
/// 4. 总副本数必须 >= min_total_replicas
///
/// 使用场景：
/// - `select_operators_by_layer()` 函数的返回值
/// - `request_pin_for_subject()` 中的运营者分配
/// - `LayeredPinAssignments` 存储的数据源
#[derive(Clone, Encode, Decode, TypeInfo)]
pub struct LayeredOperatorSelection<AccountId> {
    /// Layer 1核心运营者列表
    pub core_operators: BoundedVec<AccountId, ConstU32<16>>,
    /// Layer 2社区运营者列表
    pub community_operators: BoundedVec<AccountId, ConstU32<16>>,
}

/// CID 的分层 Pin 分配记录，包含 Layer 1（core）和 Layer 2（community）运营者列表。
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct LayeredPinAssignment<AccountId> {
    /// Layer 1运营者列表
    pub core_operators: BoundedVec<AccountId, ConstU32<8>>,
    /// Layer 2运营者列表
    pub community_operators: BoundedVec<AccountId, ConstU32<8>>,
}

// ============================================================================
// 运营者监控相关类型（阶段1：链上基础监控）
// ============================================================================

/// 函数级详细中文注释：运营者Pin健康统计结构体
///
/// 记录每个运营者的Pin管理健康状况，用于：
/// 1. 实时监控运营者服务质量
/// 2. 计算运营者健康度得分
/// 3. 容量预警与负载均衡
/// 4. 运营者排行榜与信誉评分
///
/// 字段说明：
/// - total_pins：当前管理的Pin总数
/// - healthy_pins：健康Pin数（副本数达标）
/// - failed_pins：累计失败Pin数（历史累计）
/// - last_check：上次统计更新时间
/// - health_score：健康度得分（0-100，动态计算）
///
/// 健康度评分算法：
/// - 基础分：60分
/// - 健康Pin比例奖励：(healthy_pins / total_pins) * 40，最多+40分
/// - 失败率惩罚：(failed_pins / total_pins) * 100 * 2，每1%失败率扣2分，最多扣60分
/// - 最终得分：max(0, min(100, 60 + 健康奖励 - 失败惩罚))
///
/// 使用场景：
/// - 在Pin分配时更新（`request_pin_for_subject`）
/// - 在OCW健康检查时更新（`check_pin_health_via_ocw`）
/// - 在运营者Dashboard展示（RPC查询）
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub struct OperatorPinHealth<BlockNumber> {
    /// 当前管理的Pin总数
    pub total_pins: u32,
    /// 健康Pin数（副本数达标）
    pub healthy_pins: u32,
    /// 累计失败Pin数
    pub failed_pins: u32,
    /// 上次统计更新时间
    pub last_check: BlockNumber,
    /// 健康度得分（0-100）
    pub health_score: u8,
}

impl<BlockNumber: Default> Default for OperatorPinHealth<BlockNumber> {
    fn default() -> Self {
        Self {
            total_pins: 0,
            healthy_pins: 0,
            failed_pins: 0,
            last_check: Default::default(),
            health_score: 100, // 初始满分
        }
    }
}

/// 函数级详细中文注释：运营者综合指标结构体（供RPC返回）
///
/// 聚合运营者的多维度指标，用于：
/// 1. RPC接口返回完整的运营者数据
/// 2. 前端Dashboard展示
/// 3. 运营者排行榜
///
/// 字段说明：
/// - status：运营者状态（0=Active, 1=Suspended）
/// - capacity_gib：声明的存储容量（GiB）
/// - registered_at：注册时间（区块号）
/// - total_pins：当前管理的Pin总数
/// - healthy_pins：健康Pin数
/// - failed_pins：累计失败Pin数
/// - health_score：健康度得分（0-100）
/// - used_capacity_gib：已使用容量（GiB，估算值）
/// - capacity_usage_percent：容量使用率（0-100）
/// - pending_rewards：待领取收益
///
/// 使用场景：
/// - RPC方法 `memoIpfs_getOperatorMetrics`
/// - 前端运营者Dashboard
#[derive(Clone, Encode, Decode, TypeInfo, DecodeWithMemTracking, MaxEncodedLen)]
pub struct OperatorMetrics<Balance: MaxEncodedLen, BlockNumber: MaxEncodedLen> {
    /// 运营者状态（0=Active, 1=Suspended）
    pub status: u8,
    /// 声明的存储容量（GiB）
    pub capacity_gib: u32,
    /// 注册时间
    pub registered_at: BlockNumber,
    /// 当前管理的Pin总数
    pub total_pins: u32,
    /// 健康Pin数
    pub healthy_pins: u32,
    /// 累计失败Pin数
    pub failed_pins: u32,
    /// 健康度得分（0-100）
    pub health_score: u8,
    /// 已使用容量（GiB，估算值）
    pub used_capacity_gib: u32,
    /// 容量使用率（0-100）
    pub capacity_usage_percent: u8,
    /// 待领取收益
    pub pending_rewards: Balance,
}

// ============================================================================
// 公共IPFS网络节点管理类型（简化版，无隐私约束）
// ============================================================================

/// 函数级详细中文注释：节点统计信息（简化版，用于公共IPFS网络）
///
/// 记录每个Substrate节点的PIN统计和健康状态：
/// - total_pins：该节点当前Pin的CID总数
/// - capacity_gib：节点存储容量（GB）
/// - health_score：健康评分（0-100，越高越好）
/// - last_check：最后一次健康检查的区块号
///
/// 用途：
/// 1. 智能PIN分配：根据容量和健康度选择最优节点
/// 2. 负载均衡：避免单个节点过载
/// 3. 监控告警：健康度低于阈值时告警
///
/// 评分算法（简化）：
/// score = capacity_usage(50%) + (100 - health_score)(50%)
/// 评分越低，节点越优先被选择
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(BlockNumber))]
pub struct SimpleNodeStats<BlockNumber> {
    /// 该节点当前Pin的CID总数
    pub total_pins: u32,
    /// 节点存储容量（GB）
    pub capacity_gib: u32,
    /// 健康评分（0-100，越高越好）
    pub health_score: u8,
    /// 最后一次健康检查的区块号
    pub last_check: BlockNumber,
}

impl<BlockNumber: Default> Default for SimpleNodeStats<BlockNumber> {
    fn default() -> Self {
        Self {
            total_pins: 0,
            capacity_gib: 0,
            health_score: 100, // 初始满分
            last_check: Default::default(),
        }
    }
}

/// 函数级详细中文注释：PIN状态枚举（简化版）
///
/// 记录PIN的当前状态：
/// - Pending：等待OCW处理
/// - Pinned：已成功Pin
/// - Failed：Pin失败
/// - Restored：丢失后已修复
#[derive(
    Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen,
)]
pub enum SimplePinStatus {
    /// 等待OCW处理
    Pending,
    /// 已成功Pin
    Pinned,
    /// Pin失败
    Failed,
    /// 丢失后已修复
    Restored,
}

impl Default for SimplePinStatus {
    fn default() -> Self {
        Self::Pending
    }
}
