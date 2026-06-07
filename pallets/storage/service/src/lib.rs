#![cfg_attr(not(feature = "std"), no_std)]
//! Temporary note: `deprecated` is currently allowed globally for `RuntimeEvent`
//! and constant weights, and should be removed after benchmark-backed weights land.
//! 说明：临时全局允许 `deprecated`（`RuntimeEvent`/常量权重），后续基准权重接入后移除
#![allow(deprecated)]

extern crate alloc;

use frame_support::{
    pallet_prelude::*,
    traits::{Currency, ExistenceRequirement, Get, ReservableCurrency},
    BoundedVec,
};
use frame_system::pallet_prelude::*;
// （已下线）移除对 memo-endowment 的接口依赖
use alloc::string::String;
use codec::{Decode, Encode};
use serde_json::Value as JsonValue;
use sp_core::crypto::KeyTypeId;
use sp_runtime::{
    offchain::{http, StorageKind},
    traits::AtLeast32BitUnsigned,
};
use sp_std::vec::Vec;

pub mod migrations;
pub mod runtime_api;
/// 函数级详细中文注释：优化后的类型定义模块
///
/// 包含以下核心类型：
/// - 分层配置（PinTier, TierConfig）
/// - Subject管理（SubjectType, SubjectInfo）
/// - 健康巡检（HealthCheckTask, HealthStatus, GlobalHealthStats）
/// - 周期扣费（BillingTask, ChargeLayer, GraceStatus）
pub mod types;
pub mod weights;

// 导出 runtime API
pub use runtime_api::*;

// 导出常用类型，方便其他模块使用
pub use types::{
    BillingTask, ChargeLayer, ChargeResult, DomainStats, EntityFunding, GlobalHealthStats,
    GraceStatus, HealthCheckTask, HealthStatus, LayeredOperatorSelection, LayeredPinAssignment,
    OperatorLayer, OperatorMetrics, OperatorPinHealth, PinTier, SimpleNodeStats, SimplePinStatus,
    StorageLayerConfig, SubjectInfo, SubjectType, TierConfig, UnpinReason,
};
pub use weights::{SubstrateWeight, WeightInfo};

/// Subject owner read-only provider with low coupling.
/// Subject 所有者只读提供者（低耦合）。
///
/// ### Purpose
/// ### 功能
/// - Read the current owner field from a business pallet.
/// - 从业务 pallet 读取 owner 字段（当前所有者）。
/// - Support permission checks.
/// - 用于权限检查。
///
/// ### Design
/// ### 设计理念
/// - **Transferable ownership**: supports ownership transfer.
/// - **owner 可转让**：支持所有权转移。
/// - **Permission control**: used to validate operational permissions.
/// - **权限控制**：用于检查操作权限。
/// - **Low coupling**: decouples from business pallets through a trait.
/// - **低耦合设计**：通过 trait 解耦，不直接依赖业务 pallet。
///
/// ### Use cases
/// ### 使用场景
/// - Permission checks for operations such as `request_pin_for_subject`.
/// - 权限检查：`request_pin_for_subject` 等操作。
/// - Subject existence checks.
/// - subject 存在性检查。
pub trait SubjectOwnerProvider<AccountId> {
    /// 返回subject的owner（当前所有者）
    ///
    /// ### 参数
    /// - `subject_id`: Subject ID
    ///
    /// ### 返回
    /// - `Some(owner)`: subject存在，返回当前所有者账户
    /// - `None`: subject不存在
    fn owner_of(subject_id: u64) -> Option<AccountId>;
}

/// 专用 Offchain 签名 KeyType。注意：需要在节点端注册对应密钥。
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ipfs");

/// 函数级详细中文注释：OCW 专用签名算法类型
/// - 使用 sr25519 作为默认曲线；
/// - 节点 keystore 中通过 `--key` 或 RPC 注入该类型的密钥；
pub mod sr25519_app {
    use super::KEY_TYPE;
    use sp_application_crypto::{app_crypto, sr25519};
    app_crypto!(sr25519, KEY_TYPE);
}

pub type AuthorityId = sr25519_app::Public;

/// Unified storage pinning interface used by all business pallets.
/// 统一存储 Pin 接口，供所有业务 pallet 使用。
///
/// This trait merges the former `IpfsPinner` and `ContentRegistry` traits into a
/// simpler API:
/// 合并了原 `IpfsPinner` 和 `ContentRegistry` 两个 trait，提供更简洁的 API：
/// - `domain` string replaces the `SubjectType` enum for looser coupling.
/// - `domain` 字符串取代 `SubjectType` 枚举（松耦合）。
/// - `size_bytes` is provided by the caller as the real file size.
/// - `size_bytes` 由调用方提供真实文件大小。
/// - `tier` is mandatory to avoid `Option` ambiguity.
/// - `tier` 为必填（不再有 `Option` 歧义）。
///
/// ```ignore
/// // Declared in Config:
/// // Config 中声明：
/// type StoragePin: StoragePin<Self::AccountId>;
///
/// // Usage (domain maps to DomainPins and SubjectType automatically):
/// // 调用（domain 自动映射到 DomainPins 和 SubjectType）：
/// T::StoragePin::pin(who, b"evidence", evidence_id, None, cid_vec, PinTier::Critical)?;
/// T::StoragePin::unpin(who, cid_vec)?;
/// ```
pub trait StoragePin<AccountId> {
    /// Pin CID 到指定域。
    ///
    /// - `owner`: 发起账户
    /// - `domain`: 域名（如 `b"evidence"`, `b"product"`）
    /// - `subject_id`: 主体 ID
    /// - `entity_id`: 所属 Entity ID（无归属时传 `None`）
    /// - `cid`: IPFS CID 明文
    /// - `size_bytes`: 文件实际大小（字节），调用方必须提供真实值
    /// - `tier`: Critical / Standard / Temporary
    fn pin(
        owner: AccountId,
        domain: &[u8],
        subject_id: u64,
        entity_id: Option<u64>,
        cid: Vec<u8>,
        size_bytes: u64,
        tier: PinTier,
    ) -> DispatchResult;

    /// 取消 Pin（幂等，CID 不存在时返回 Ok）。
    fn unpin(owner: AccountId, cid: Vec<u8>) -> DispatchResult;
}

/// CID lock manager interface.
/// CID 锁定管理器接口。
///
/// Design goals:
/// 设计目标：
/// - lock evidence CIDs during arbitration to prevent deletion.
/// - 支持仲裁期间锁定证据 CID，防止被删除。
/// - unlock automatically after arbitration completes.
/// - 仲裁完成后自动解锁。
/// - support optional expiry.
/// - 支持过期时间设置。
pub trait CidLockManager<Hash, BlockNumber> {
    /// 锁定 CID（防止删除）
    ///
    /// 参数：
    /// - `cid_hash`: CID 的哈希值
    /// - `reason`: 锁定原因（如 "arbitration:otc:123"）
    /// - `until`: 可选的锁定到期区块号
    fn lock_cid(cid_hash: Hash, reason: Vec<u8>, until: Option<BlockNumber>) -> DispatchResult;

    /// 解锁 CID
    ///
    /// 参数：
    /// - `cid_hash`: CID 的哈希值
    /// - `reason`: 锁定原因（必须匹配）
    fn unlock_cid(cid_hash: Hash, reason: Vec<u8>) -> DispatchResult;

    /// 检查 CID 是否被锁定
    fn is_locked(cid_hash: &Hash) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec; // 添加vec宏导入（no_std环境）
    use frame_support::traits::tokens::Imbalance;
    use frame_support::traits::ConstU32;
    use frame_support::traits::StorageVersion;
    use frame_support::PalletId;
    use sp_runtime::traits::AccountIdConversion;
    use sp_runtime::traits::Saturating;
    use sp_runtime::transaction_validity::{
        InvalidTransaction, TransactionSource, TransactionValidity, ValidTransaction,
    };
    use sp_runtime::SaturatedConversion;

    /// 余额别名
    pub type BalanceOf<T> = <T as Config>::Balance;

    /// Pin metadata.
    /// Pin 元信息。
    /// - `replicas`: replica count.
    /// - `replicas`：副本数。
    /// - `size`: file size in bytes.
    /// - `size`：文件大小（字节）。
    /// - `created_at`: creation block.
    /// - `created_at`：创建时间（区块号）。
    /// - `last_activity`: latest activity block.
    /// - `last_activity`：最后活动时间（区块号）。
    #[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(BlockNumber))]
    pub struct PinMetadata<BlockNumber> {
        pub replicas: u32,
        pub size: u64,
        pub created_at: BlockNumber,
        pub last_activity: BlockNumber,
    }

    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>>
        + frame_system::offchain::CreateBare<Call<Self>>
    {
        /// Currency interface used for reserving deposits and collecting fees.
        /// 货币接口（用于预留押金或扣费）
        type Currency: Currency<Self::AccountId, Balance = Self::Balance>
            + ReservableCurrency<Self::AccountId>;
        /// Balance type.
        /// 余额类型
        type Balance: Parameter + AtLeast32BitUnsigned + Default + Copy + MaxEncodedLen;

        /// Fee collector account resolver, such as the treasury or platform account,
        /// used for one-off and recurring charges.
        /// 资金接收账户解析器（例如 Treasury 或平台账户），用于收取一次性费用与周期扣费。
        type FeeCollector: sp_core::Get<Self::AccountId>;

        /// Governance origin used for parameters, blacklists, and quotas.
        /// 治理 Origin（用于参数 / 黑名单 / 配额）
        type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Maximum supported `cid_hash` length in bytes.
        /// 最大支持的 `cid_hash` 长度（字节）
        #[pallet::constant]
        type MaxCidHashLen: Get<u32>;

        /// Maximum supported PeerId byte length, covering Base58 text or multi-address digests.
        /// 最大支持的 PeerId 字节长度（Base58 文本或多地址指纹摘要）
        #[pallet::constant]
        type MaxPeerIdLen: Get<u32>;

        /// Minimum operator-bond fallback value in NEX base units, used when pricing is unavailable.
        /// 最小运营者保证金兜底值（NEX 最小单位，pricing 不可用时使用）
        #[pallet::constant]
        type MinOperatorBond: Get<Self::Balance>;

        /// Minimum operator-bond USD value using 10^6 precision.
        /// `100_000_000` means 100 USDT.
        /// 最小运营者保证金 USD 价值（精度 10^6，`100_000_000` = 100 USDT）
        #[pallet::constant]
        type MinOperatorBondUsd: Get<u64>;

        /// Deposit calculator for unified USD-value-based bond computation.
        /// 保证金计算器（统一的 USD 价值动态计算）
        type DepositCalculator: pallet_trading_common::DepositCalculator<Self::Balance>;

        /// Minimum declared capacity in GiB.
        /// 最小可宣告容量（GiB）
        #[pallet::constant]
        type MinCapacityGiB: Get<u32>;

        /// Weight provider placeholder.
        /// 权重信息占位
        type WeightInfo: WeightInfo;

        /// PalletId used to derive stable subject-funding accounts from `creator + subject_id`.
        /// 派生“主题资金账户”的 PalletId（`creator + subject_id` 派生稳定地址）
        #[pallet::constant]
        type SubjectPalletId: Get<PalletId>;

        /// IPFS pool account used as the source of public-fee subsidies.
        /// IPFS 池账户（公共费用来源）
        ///
        /// ## Notes
        /// ## 说明
        /// - replenished periodically by `pallet-storage-treasury` (offering route 2% × 50%)
        /// - 由 `pallet-storage-treasury` 定期补充（供奉路由 2% × 50%）
        /// - provides free quota for subjects
        /// - 用于为 subject 提供免费配额
        type IpfsPoolAccount: Get<Self::AccountId>;

        /// Operator escrow account that receives service fees.
        /// 运营者托管账户（服务费接收方）
        ///
        /// ## Notes
        /// ## 说明
        /// - receives all pin-service fees
        /// - 接收所有 pin 服务费用
        /// - fees are distributed later based on SLA after operators complete work
        /// - 待运营者完成任务后基于 SLA 分配
        type OperatorEscrowAccount: Get<Self::AccountId>;

        /// Monthly public-fee quota per subject.
        /// 每月公共费用配额
        ///
        /// ## Notes
        /// ## 说明
        /// - each subject can consume this much free quota per month
        /// - 每个 subject 每月可使用的免费额度
        /// - default is 100 NEX and can be adjusted by governance
        /// - 默认：100 NEX（可治理调整）
        #[pallet::constant]
        type MonthlyPublicFeeQuota: Get<BalanceOf<Self>>;

        /// Quota reset period in blocks.
        /// 配额重置周期（区块数）
        ///
        /// ## Notes
        /// ## 说明
        /// - default: `100,800 × 4 = 403,200` blocks, about 28 days
        /// - 默认：`100,800 × 4 = 403,200` 区块，约 28 天
        #[pallet::constant]
        type QuotaResetPeriod: Get<BlockNumberFor<Self>>;

        /// Default billing period in blocks.
        /// 默认扣费周期（区块数）
        ///
        /// ## Notes
        /// ## 说明
        /// - interval used for recurring billing
        /// - 周期性扣费的间隔时间
        /// - default: `100,800` blocks, about 7 days (assuming 3 s/block)
        /// - 默认：`100,800` 区块，约 7 天（假设 3 秒 / 块）
        /// - adjustable through governance
        /// - 可通过治理调整
        #[pallet::constant]
        type DefaultBillingPeriod: Get<u32>;

        /// Operator deregistration grace period in blocks.
        /// 运营者注销宽限期（区块数）
        ///
        /// ## Notes
        /// ## 说明
        /// - waiting period after an operator calls `leave_operator`
        /// - 运营者调用 `leave_operator` 后的等待期
        /// - during this period, OCW migrates pins to other operators
        /// - 宽限期内 OCW 迁移 Pin 到其他运营者
        /// - default: `100,800` blocks, about 7 days
        /// - 默认：`100,800` 区块，约 7 天
        #[pallet::constant]
        type OperatorGracePeriod: Get<BlockNumberFor<Self>>;

        /// Optional Entity treasury charging interface implemented by `entity-registry`.
        /// Configure this as `()` when the runtime does not need the feature.
        /// Entity 国库扣费接口（可选层，由 `entity-registry` 实现）
        /// 运行时不需要此功能时配置为 `()` 即可。
        type EntityFunding: crate::types::EntityFunding<Self::AccountId, BalanceOf<Self>>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// 函数级详细中文注释：Genesis配置（初始化分层配置默认值）
    ///
    /// 目的：
    /// - 在创世块时设置三层Pin配置的合理默认值
    /// - 可通过runtime中的GenesisConfig自定义初始值
    /// - 提供开箱即用的生产级配置
    // 注释：由于substrate新版本对GenesisConfig的Serialize/Deserialize要求较严格，
    // 暂时使用默认配置。链启动后可通过治理接口update_tier_config动态调整。
    //
    // 默认配置已在TierConfig::default()、TierConfig::critical_default()、
    // TierConfig::temporary_default()中定义，get_tier_config会自动应用。

    // [已删除] PricingParams：空壳存储，从未被任何逻辑读写。
    // 定价通过 PricePerGiBWeek + BillingPeriodBlocks 实现。

    /// Pending pin requests.
    /// Pin 订单存储
    ///
    /// - Key: `cid_hash`
    /// - Key：`cid_hash`
    /// - Value: `(payer, replicas, subject_id, size_bytes, deposit)`
    /// - Value：`(payer, replicas, subject_id, size_bytes, deposit)`
    #[pallet::storage]
    pub type PendingPins<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,
        (T::AccountId, u32, u64, u64, T::Balance),
        OptionQuery,
    >;

    /// Pin metadata (replica count, size, creation time, and latest health check).
    /// Pin 元信息（副本数、大小、创建时间、最后巡检）
    #[pallet::storage]
    pub type PinMeta<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, PinMetadata<BlockNumberFor<T>>, OptionQuery>;

    /// Pin 状态机：0=Requested,1=Pinning,2=Pinned,3=Degraded,4=Failed
    #[pallet::storage]
    pub type PinStateOf<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, u8, ValueQuery>;

    /// 副本分配：为每个 cid_hash 挑选的运营者账户
    #[pallet::storage]
    pub type PinAssignments<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,
        BoundedVec<T::AccountId, frame_support::traits::ConstU32<16>>,
        OptionQuery,
    >;

    /// 分配内的成功标记：(cid_hash, operator) -> 成功与否
    #[pallet::storage]
    pub type PinSuccess<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::Hash,
        Blake2_128Concat,
        T::AccountId,
        bool,
        ValueQuery,
    >;

    /// Operator information.
    /// 运营者信息结构体。
    ///
    /// ## Field overview
    /// ## 字段说明
    /// - `peer_id`: IPFS node PeerId.
    /// - `peer_id`：IPFS 节点的 PeerId。
    /// - `capacity_gib`: declared storage capacity in GiB.
    /// - `capacity_gib`：声明的存储容量（GiB）。
    /// - `endpoint_hash`: hash of the IPFS Cluster API endpoint.
    /// - `endpoint_hash`：IPFS Cluster API 端点的哈希。
    /// - `cert_fingerprint`: optional TLS certificate fingerprint.
    /// - `cert_fingerprint`：TLS 证书指纹（可选）。
    /// - `status`: operator status (`0=Active`, `1=Suspended`, `2=Banned`).
    /// - `status`：运营者状态（`0=Active`，`1=Suspended`，`2=Banned`）。
    /// - `registered_at`: registration timestamp / block height.
    /// - `registered_at`：注册时间戳（区块高度）。
    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    pub struct OperatorInfo<T: Config> {
        pub peer_id: BoundedVec<u8, T::MaxPeerIdLen>,
        pub capacity_gib: u32,
        pub endpoint_hash: T::Hash,
        pub cert_fingerprint: Option<T::Hash>,
        pub status: u8,                       // 0=Active,1=Suspended,2=Banned
        pub registered_at: BlockNumberFor<T>, // ✅ P1新增：注册时间戳
        pub layer: OperatorLayer,             // ✅ Layer分层：Core/Community/External
        pub priority: u8, // ✅ 优先级：0-255（越小越优先，Core通常0-50，Community通常51-200）
    }

    /// Operator registry plus bond records.
    /// 运营者注册表与保证金
    #[pallet::storage]
    pub type Operators<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, OperatorInfo<T>, OptionQuery>;

    #[pallet::storage]
    pub type OperatorBond<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    /// Actual storage bytes used by each operator.
    /// 运营者实际存储字节数
    ///
    /// Replaces the old hard-coded estimate of "2 MB per pin".
    /// 替代“每个 Pin 平均 2 MB”的硬编码估算。
    /// The value is adjusted using `PinMeta::size` during pin assignment and removal.
    /// 在 Pin 分配 / 移除时通过 `PinMeta::size` 增减。
    #[pallet::storage]
    pub type OperatorUsedBytes<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    /// CID lock records.
    /// CID 锁定记录
    ///
    /// Used to lock evidence CIDs during arbitration so they cannot be deleted.
    /// 用于仲裁期间锁定证据 CID，防止被删除。
    /// - Key: `cid_hash`
    /// - Key：`cid_hash`
    /// - Value: `(reason, optional expiry block)`
    /// - Value：`(reason, optional expiry block)`
    #[pallet::storage]
    pub type CidLocks<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,
        (BoundedVec<u8, ConstU32<128>>, Option<BlockNumberFor<T>>),
        OptionQuery,
    >;

    /// M4修复：记录CID的Unpin原因，供cleanup时正确发射PinRemoved事件
    #[pallet::storage]
    pub type CidUnpinReason<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, UnpinReason, OptionQuery>;

    /// Owner-indexed CID list.
    /// 按所有者索引的 CID 列表
    ///
    /// Replaces a global scan over `PinSubjectOf::iter()` and supports fast lookup of
    /// all CIDs owned by one user.
    /// 替代全局扫描 `PinSubjectOf::iter()`，支持快速查询某用户拥有的所有 CID。
    /// Inserted in `request_pin_for_subject` and removed during cleanup.
    /// 在 `request_pin_for_subject` 中添加，在 cleanup 时移除。
    #[pallet::storage]
    pub type OwnerPinIndex<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<T::Hash, ConstU32<1000>>,
        ValueQuery,
    >;

    /// CID 归档索引：u64 → T::Hash 映射
    ///
    /// 供 pallet-storage-lifecycle 的 StorageArchiver 使用。
    /// lifecycle pallet 使用 u64 作为 data_id，此索引桥接到 T::Hash。
    /// 在 do_request_pin 中写入，在 do_cleanup_single_cid 中移除。
    #[pallet::storage]
    pub type CidArchiveIndex<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, T::Hash, OptionQuery>;

    /// CID 归档反向索引：T::Hash → u64
    ///
    /// 用于从 cid_hash 快速查找 archive_id。
    #[pallet::storage]
    pub type CidArchiveReverseIndex<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, u64, OptionQuery>;

    /// 下一个 CID 归档 ID（自增计数器）
    #[pallet::storage]
    pub type NextCidArchiveId<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Active-operator index with a bounded size.
    /// 活跃运营者索引（有界）
    ///
    /// Replaces an unbounded full-table scan over `Operators::iter()`.
    /// 替代 `Operators::iter()` 的无界全表扫描。
    /// Only contains accounts with `status = 0` (`Active`).
    /// 仅包含 `status = 0`（`Active`）的运营者账户。
    /// Maintained in `join_operator`, `set_operator_status`, `leave_operator`, and
    /// `finalize_operator_unregistration`.
    /// 在 `join_operator` / `set_operator_status` / `leave_operator` /
    /// `finalize_operator_unregistration` 中维护。
    #[pallet::storage]
    pub type ActiveOperatorIndex<T: Config> =
        StorageValue<_, BoundedVec<T::AccountId, ConstU32<256>>, ValueQuery>;

    /// Pending operator unregistrations under the grace-period mechanism.
    /// 待注销运营者列表（宽限期机制）
    ///
    /// ## Purpose
    /// ## 用途
    /// - records operators that requested unregister while still holding pins
    /// - 记录已提交 `unregister` 但仍有 Pin 的运营者
    /// - value stores the grace-period expiry block
    /// - Value：宽限期到期时间（区块高度）
    /// - OCW migrates pins to other operators during the grace period
    /// - 宽限期内 OCW 自动迁移 Pin 到其他运营者
    /// - after expiry, the runtime checks pin count and refunds the bond if empty
    /// - 宽限期结束后检查 Pin 数量，无 Pin 则返还保证金并移除记录
    ///
    /// ## Grace-period design
    /// ## 宽限期设计
    /// - default: 7 days (`100,800` blocks at 6 s/block)
    /// - 默认 7 天（`100,800` 块，假设 6 秒 / 块）
    /// - adjustable through governance
    /// - 可通过治理调整
    #[pallet::storage]
    pub type PendingUnregistrations<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

    /// SLA statistics for an operator.
    /// 运营者 SLA 统计
    #[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
    #[scale_info(skip_type_params(T))]
    pub struct SlaStats<T: Config> {
        pub pinned_bytes: u64,
        pub probe_ok: u32,
        pub probe_fail: u32,
        pub degraded: u32,
        pub last_update: BlockNumberFor<T>,
    }

    impl<T: Config> Default for SlaStats<T> {
        /// 函数级中文注释：为 SlaStats<T> 提供显式的 Default 实现，避免对 T 施加 Default 约束
        /// - 将计数置 0，last_update 使用 BlockNumber 的默认值
        fn default() -> Self {
            Self {
                pinned_bytes: 0,
                probe_ok: 0,
                probe_fail: 0,
                degraded: 0,
                last_update: Default::default(),
            }
        }
    }

    #[pallet::storage]
    pub type OperatorSla<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, SlaStats<T>, ValueQuery>;

    // ====== 双重扣款配额管理 ======

    /// Public-fee quota usage tracked per user.
    /// 用户级公共费用配额使用记录
    ///
    /// Key: owner AccountId（用户级限额，防止单用户通过多 Subject 滥用配额）
    /// Value: (已使用金额, 配额重置区块号)
    #[pallet::storage]
    pub type PublicFeeQuotaUsage<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        (BalanceOf<T>, BlockNumberFor<T>), // (used_amount, reset_block)
        ValueQuery,
    >;

    /// Total amount charged from the IPFS pool.
    /// 累计从 IPFS 池扣款统计
    #[pallet::storage]
    pub type TotalChargedFromPool<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Total amount charged from subject funding.
    /// 累计从 SubjectFunding 扣款统计
    #[pallet::storage]
    pub type TotalChargedFromSubject<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// User-level funding-balance tracking.
    /// 用户级存储资金账户余额追踪
    ///
    /// Hybrid model: one derived account per user instead of one account per subject.
    /// 混合方案：每个用户一个派生账户，替代每个 Subject 一个账户。
    /// - Key: user account
    /// - Key：用户账户
    /// - Value: cumulative top-up amount for statistics
    /// - Value：累计充值金额（用于统计）
    #[pallet::storage]
    #[pallet::getter(fn user_funding_balance)]
    pub type UserFundingBalance<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    /// 函数级中文注释：Subject 级费用使用追踪（不派生账户，仅记账）
    ///
    /// 混合方案：记录每个 Subject 的存储费用消耗，便于审计
    /// Key: (用户账户, SubjectType, subject_id)
    /// Value: 累计消耗金额
    #[pallet::storage]
    #[pallet::getter(fn subject_usage)]
    pub type SubjectUsage<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (T::AccountId, u8, u64), // (user, subject_type_domain, subject_id)
        BalanceOf<T>,
        ValueQuery,
    >;

    // [已删除] ReplicasForLevel0~3：与 PinTierConfig（TierConfig.replicas）功能重复。
    // 副本数统一由 PinTierConfig 管理（Critical=5, Standard=3, Temporary=1）。

    /// 函数级中文注释：最小副本数阈值
    ///
    /// 说明：
    /// - 当副本数低于此阈值时，OCW 自动补充
    /// - 默认：2（至少保证 2 个副本）
    #[pallet::type_value]
    pub fn DefaultMinReplicasThreshold<T: Config>() -> u32 {
        2
    }

    #[pallet::storage]
    pub type MinReplicasThreshold<T: Config> =
        StorageValue<_, u32, ValueQuery, DefaultMinReplicasThreshold<T>>;

    // ====== 计费与生命周期（最小增量）======
    /// 函数级中文注释：每 GiB·周 单价（治理可调）。单位使用链上最小余额单位的整数，建议采用按字节的定点基数以避免小数。
    #[pallet::type_value]
    pub fn DefaultPricePerGiBWeek<T: Config>() -> u128 {
        1_000_000_000
    }
    #[pallet::storage]
    pub type PricePerGiBWeek<T: Config> =
        StorageValue<_, u128, ValueQuery, DefaultPricePerGiBWeek<T>>;

    /// 函数级中文注释：计费周期（块），默认一周（6s/块 × 60 × 60 × 24 × 7 = 100_800）。
    #[pallet::type_value]
    pub fn DefaultBillingPeriodBlocks<T: Config>() -> u32 {
        100_800
    }
    #[pallet::storage]
    pub type BillingPeriodBlocks<T: Config> =
        StorageValue<_, u32, ValueQuery, DefaultBillingPeriodBlocks<T>>;

    /// 函数级中文注释：宽限期（块）。在余额不足时进入 Grace，超过宽限仍不足则过期。
    ///
    /// 默认值：5,184,000 块 ≈ 360天（按6秒/块计算）
    /// 计算：360天 × 24小时 × 60分钟 × 10块/分钟 = 5,184,000
    ///
    /// 设计理由：
    /// - 长宽限期体现平台对用户数据的重视
    /// - 给用户充足时间处理账户问题
    /// - 可通过治理调整（set_billing_params）
    #[pallet::type_value]
    pub fn DefaultGraceBlocks<T: Config>() -> u32 {
        5_184_000 // 360天
    }
    #[pallet::storage]
    pub type GraceBlocks<T: Config> = StorageValue<_, u32, ValueQuery, DefaultGraceBlocks<T>>;

    /// 函数级中文注释：每块处理的最大扣费数，用于限流保护。
    #[pallet::type_value]
    pub fn DefaultMaxChargePerBlock<T: Config>() -> u32 {
        50
    }
    #[pallet::storage]
    pub type MaxChargePerBlock<T: Config> =
        StorageValue<_, u32, ValueQuery, DefaultMaxChargePerBlock<T>>;

    /// 函数级中文注释：主体资金账户最低保留（KeepAlive 余量），扣费需确保余额-金额≥该值。
    #[pallet::type_value]
    pub fn DefaultSubjectMinReserve<T: Config>() -> BalanceOf<T> {
        BalanceOf::<T>::default()
    }
    #[pallet::storage]
    pub type SubjectMinReserve<T: Config> =
        StorageValue<_, BalanceOf<T>, ValueQuery, DefaultSubjectMinReserve<T>>;

    /// 函数级中文注释：计费暂停总开关（治理控制）。
    #[pallet::type_value]
    pub fn DefaultBillingPaused<T: Config>() -> bool {
        false
    }
    #[pallet::storage]
    pub type BillingPaused<T: Config> = StorageValue<_, bool, ValueQuery, DefaultBillingPaused<T>>;

    // ⭐ P2优化：已删除 AllowDirectPin 存储（10行）
    // 原因：旧版 request_pin() extrinsic 已删除，此配置无用
    // 删除日期：2025-10-26

    // [已删除] DueQueue + DueEnqueueSpread：
    // 旧版到期队列，已被 BillingQueue + on_finalize 自动计费替代。

    /// 每个 CID 的计费状态：(下一次扣费块高, 单价快照, 状态)。
    /// 状态：0=Active, 1=Grace, 2=Expired(待清理)。
    /// 仍用于 OCW 物理删除扫描和 cleanup_expired_cids。
    #[pallet::storage]
    pub type PinBilling<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, (BlockNumberFor<T>, u128, u8), OptionQuery>;

    /// 函数级中文注释：记录 CID 的 funding 来源（owner, subject_id），用于从派生账户自动扣款。
    #[pallet::storage]
    pub type PinSubjectOf<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, (T::AccountId, u64), OptionQuery>;

    /// CID 所属 Entity ID（可选）。pin 时由业务 pallet 提供，
    /// 计费时用于 Entity 国库扣费层。
    #[pallet::storage]
    pub type CidEntityOf<T: Config> = StorageMap<_, Blake2_128Concat, T::Hash, u64, OptionQuery>;

    // ============================================================================
    // 新增存储：域索引、分层配置、健康巡检、周期扣费（优化改造）
    // ============================================================================

    /// 函数级详细中文注释：域维度Pin索引，O(1)查找某域下的所有CID
    ///
    /// 设计目标：
    /// - 替代全局扫描 PendingPins::iter()
    /// - 支持域级别的优先级调度（Subject优先于OTC）
    /// - 便于域级别的批量操作（如暂停某域的扣费）
    ///
    /// 存储结构：
    /// - Key1: domain（如 b"subject", b"evidence", b"otc"）
    /// - Key2: cid_hash
    /// - Value: ()（标记存在即可）
    ///
    /// 使用场景：
    /// - OCW巡检时，按域顺序扫描：Evidence → Product → Entity → Shop → General...
    /// - 统计各域的Pin数量和存储容量
    /// - 实现域级别的优先级队列
    #[pallet::storage]
    #[pallet::getter(fn domain_pins)]
    pub type DomainPins<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>, // domain
        Blake2_128Concat,
        T::Hash, // cid_hash
        (),      // 标记存在
        OptionQuery,
    >;

    /// 函数级详细中文注释：域注册表 - 新pallet域自动PIN机制的核心存储
    ///
    /// 功能：
    /// - 记录已注册的域及其配置
    /// - 支持域的自动发现和管理
    /// - 映射域名到SubjectType
    ///
    /// 存储结构：
    /// - Key: 域名（如 b"subject-video", b"nft-metadata"）
    /// - Value: DomainConfig（域配置信息）
    #[pallet::storage]
    pub type RegisteredDomains<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        types::DomainConfig,
        OptionQuery,
    >;

    /// 已注册域名列表索引，避免 RegisteredDomains::iter() 全表扫描。
    /// 在 register_domain / on_finalize 域统计更新时使用。
    #[pallet::storage]
    pub type RegisteredDomainList<T: Config> =
        StorageValue<_, BoundedVec<BoundedVec<u8, ConstU32<32>>, ConstU32<128>>, ValueQuery>;

    /// 函数级详细中文注释：CID到Subject的反向映射，用于扣费时查找资金账户
    ///
    /// 设计目标：
    /// - 周期扣费时，根据 cid_hash 查找对应的 SubjectFunding 账户
    /// - 支持一个CID属于多个Subject的场景（如共享媒体文件）
    ///
    /// 存储结构：
    /// - Key: cid_hash
    /// - Value: BoundedVec<SubjectInfo>（主Subject + 可选共享Subject列表）
    ///
    /// SubjectInfo 包含：
    /// - subject_type: SubjectType (Evidence/Product/Entity/Shop/General/...)
    /// - subject_id: u64
    /// - funding_share: u8（该Subject承担的费用比例，0-100）
    #[pallet::storage]
    #[pallet::getter(fn cid_to_subject)]
    pub type CidToSubject<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,                              // cid_hash
        BoundedVec<SubjectInfo, ConstU32<8>>, // 最多8个Subject共享
        OptionQuery,
    >;

    /// 函数级详细中文注释：分层Pin策略配置
    ///
    /// 设计目标：
    /// - 根据内容重要性，配置不同的副本数和巡检周期
    /// - 平衡存储成本和可靠性
    /// - 支持运行时动态调整（治理提案）
    ///
    /// 存储结构：
    /// - Key: PinTier（Critical/Standard/Temporary）
    /// - Value: TierConfig（副本数、巡检周期、费率系数等）
    ///
    /// 默认值：
    /// - Critical：5副本，7200块（6小时），1.5x费率
    /// - Standard：3副本，28800块（24小时），1.0x费率
    /// - Temporary：1副本，604800块（7天），0.5x费率
    #[pallet::storage]
    #[pallet::getter(fn pin_tier_config)]
    pub type PinTierConfig<T: Config> =
        StorageMap<_, Blake2_128Concat, PinTier, TierConfig, ValueQuery>;

    /// 函数级详细中文注释：CID分层映射，记录每个CID的优先级等级
    ///
    /// 存储结构：
    /// - Key: cid_hash
    /// - Value: PinTier
    ///
    /// 默认规则：
    /// - 未显式设置：Standard（标准级）
    /// - 业务pallet调用时指定（如 pin_cid_for_subject 传递 tier 参数）
    #[pallet::type_value]
    pub fn DefaultPinTier() -> PinTier {
        PinTier::Standard
    }
    #[pallet::storage]
    #[pallet::getter(fn cid_tier)]
    pub type CidTier<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, PinTier, ValueQuery, DefaultPinTier>;

    /// 函数级详细中文注释：健康巡检队列，按优先级和到期时间排序
    ///
    /// 设计目标：
    /// - 替代全局扫描，提供高效的巡检调度
    /// - 支持优先级队列（Critical优先）
    /// - 自动去重和过期清理
    ///
    /// 存储结构：
    /// - Key1: next_check_block（下次巡检时间）
    /// - Key2: cid_hash
    /// - Value: HealthCheckTask
    ///
    /// 调度逻辑：
    /// - on_finalize 时，扫描 next_check_block <= current_block 的任务
    /// - 执行巡检后，重新插入队列（next_check_block + interval）
    #[pallet::storage]
    #[pallet::getter(fn health_check_queue)]
    pub type HealthCheckQueue<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>, // next_check_block
        Blake2_128Concat,
        T::Hash, // cid_hash
        HealthCheckTask<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 函数级详细中文注释：巡检统计数据，用于链上仪表板展示
    ///
    /// 存储内容：
    /// - 总Pin数、总存储量
    /// - 健康/降级/危险CID数量
    /// - 上次巡检时间
    /// - 累计修复次数
    #[pallet::storage]
    #[pallet::getter(fn health_check_stats)]
    pub type HealthCheckStats<T: Config> =
        StorageValue<_, GlobalHealthStats<BlockNumberFor<T>>, ValueQuery>;

    /// 函数级详细中文注释：域级健康统计
    ///
    /// 记录每个域的Pin数量、存储容量、健康状态等统计信息
    ///
    /// 存储结构：
    /// - Key: domain（如 b"subject", b"evidence", b"otc"）
    /// - Value: DomainStats
    ///
    /// 使用场景：
    /// - Dashboard按域展示统计数据
    /// - 域级别的监控告警
    /// - 优先级调度决策参考
    #[pallet::storage]
    #[pallet::getter(fn domain_health_stats)]
    pub type DomainHealthStats<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>, // domain
        DomainStats,
        OptionQuery,
    >;

    /// 函数级详细中文注释：域优先级配置
    ///
    /// 定义各域的巡检优先级，数值越小优先级越高
    ///
    /// 存储结构：
    /// - Key: domain
    /// - Value: priority（0-255，0为最高优先级）
    ///
    /// 默认优先级：
    /// - evidence: 0（最高优先级）
    /// - otc: 10
    /// - general: 20
    /// - custom: 100
    ///
    /// 治理可调：通过 Root 权限调整优先级
    #[pallet::storage]
    #[pallet::getter(fn domain_priority)]
    pub type DomainPriority<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>, // domain
        u8,                           // priority
        ValueQuery,                   // 默认返回255（最低优先级）
    >;

    /// 函数级详细中文注释：周期扣费队列，替代手动 charge_due 调用
    ///
    /// 设计目标：
    /// - on_finalize 自动扫描到期的扣费任务
    /// - 支持四层回退充电机制
    /// - 自动进入宽限期/Unpin流程
    ///
    /// 存储结构：
    /// - Key1: due_block（下次扣费时间）
    /// - Key2: cid_hash
    /// - Value: BillingTask
    ///
    /// 调度逻辑：
    /// - on_finalize 时，批量处理 due_block <= current_block 的任务
    /// - 扣费成功：更新 due_block += billing_period
    /// - 扣费失败：进入宽限期或标记Unpin
    #[pallet::storage]
    #[pallet::getter(fn billing_queue)]
    pub type BillingQueue<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>, // due_block
        Blake2_128Concat,
        T::Hash, // cid_hash
        BillingTask<BlockNumberFor<T>, BalanceOf<T>>,
        OptionQuery,
    >;

    /// Cursor: 计费队列上次处理到的 due_block（含），下次从 cursor+1 开始。
    #[pallet::storage]
    pub type BillingSettleCursor<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    /// Cursor: 健康巡检队列上次处理到的 check_block（含），下次从 cursor+1 开始。
    #[pallet::storage]
    pub type HealthCheckSettleCursor<T: Config> = StorageValue<_, BlockNumberFor<T>, ValueQuery>;

    /// 过期CID链上清理是否有待处理标记（避免每块全扫 PinBilling）。
    /// true = 有 state=2 的 CID 需要清理。由 mark_cid_for_unpin / on_finalize(Err分支) 设置。
    #[pallet::storage]
    pub type ExpiredCidPending<T: Config> = StorageValue<_, bool, ValueQuery>;

    /// H1-R4新增：过期CID队列，记录待清理的CID哈希。
    /// 替代 PinBilling::iter() 全表扫描，实现 O(1) 出队。
    /// 由 mark_cid_for_unpin / on_finalize(Err分支) 入队。
    #[pallet::storage]
    pub type ExpiredCidQueue<T: Config> =
        StorageValue<_, BoundedVec<T::Hash, ConstU32<200>>, ValueQuery>;

    /// 孤儿 CID 扫描游标。`on_idle` 每次从此处继续扫描 `PinMeta`，
    /// 检测 PinSubjectOf 缺失或 PinBilling 异常的条目，自动标记 unpin。
    #[pallet::storage]
    pub type OrphanSweepCursor<T: Config> =
        StorageValue<_, BoundedVec<u8, ConstU32<128>>, OptionQuery>;

    /// cleanup_expired_locks 的分页 cursor，避免 CidLocks 全表扫描。
    #[pallet::storage]
    pub type LockCleanupCursor<T: Config> =
        StorageValue<_, BoundedVec<u8, ConstU32<128>>, OptionQuery>;

    /// migrate_operator_pins 的分页 cursor，避免 PinAssignments 全表扫描。
    #[pallet::storage]
    pub type MigrateOpCursor<T: Config> =
        StorageValue<_, BoundedVec<u8, ConstU32<128>>, OptionQuery>;

    /// CID → BillingQueue due_block 反向索引，实现 O(1) 查找。
    /// 替代 renew_pin / upgrade_pin_tier 中的 O(billing_period) 线性扫描。
    #[pallet::storage]
    pub type CidBillingDueBlock<T: Config> =
        StorageMap<_, Blake2_128Concat, T::Hash, BlockNumberFor<T>, OptionQuery>;

    /// 函数级详细中文注释：运营者奖励账户，累计待提取的奖励
    ///
    /// 存储结构：
    /// - Key: operator_account
    /// - Value: 累计奖励金额
    ///
    /// 使用场景：
    /// - 周期扣费成功后，自动累加到运营者奖励
    /// - 运营者调用 operator_claim_rewards 提取
    #[pallet::storage]
    #[pallet::getter(fn operator_rewards)]
    pub type OperatorRewards<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    /// 函数级详细中文注释：运营者Pin健康统计（阶段1：链上基础监控）
    ///
    /// 记录每个运营者的Pin管理健康状况，用于：
    /// 1. 实时监控运营者服务质量
    /// 2. 计算运营者健康度得分（0-100）
    /// 3. 容量预警与负载均衡
    /// 4. 运营者排行榜与信誉评分
    ///
    /// 存储结构：
    /// - Key: operator_account
    /// - Value: OperatorPinHealth（total_pins, healthy_pins, failed_pins, last_check, health_score）
    ///
    /// 更新时机：
    /// - Pin分配时（request_pin_for_subject）：total_pins +1
    /// - Pin成功时（OCW回调）：healthy_pins +1
    /// - Pin失败时（OCW回调）：failed_pins +1
    /// - 健康检查时（on_finalize / OCW）：重新计算health_score
    ///
    /// 使用场景：
    /// - 运营者Dashboard展示（RPC查询）
    /// - 容量告警（容量使用率超过80%）
    /// - 健康度下降告警（得分下降超过10分）
    /// - 运营者排行榜（按健康度排序）
    #[pallet::storage]
    #[pallet::getter(fn operator_pin_stats)]
    pub type OperatorPinStats<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        OperatorPinHealth<BlockNumberFor<T>>,
        ValueQuery, // 默认值为全0，健康度100分
    >;

    /// H1修复：运营者Pin数量索引（O(1)替代全表扫描）
    ///
    /// Key: operator_account
    /// Value: 当前分配给该运营者的Pin数量
    ///
    /// 更新时机：
    /// - Pin分配时 +1
    /// - Pin移除时 -1
    #[pallet::storage]
    pub type OperatorPinCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// 函数级详细中文注释：分层存储策略配置存储
    ///
    /// 按（数据类型 × Pin层级）存储分层策略配置，用于：
    /// 1. 动态配置不同数据类型的存储策略
    /// 2. 治理提案调整分层参数
    /// 3. 运营者选择时的依据
    ///
    /// Key: (SubjectType, PinTier)
    /// Value: StorageLayerConfig
    ///
    /// 示例：
    /// - (SubjectType::Evidence, PinTier::Critical) → {core: 5, community: 0, ...}
    /// - (SubjectType::General, PinTier::Critical) → {core: 3, community: 2, ...}
    /// - (SubjectType::Product, PinTier::Standard) → {core: 2, community: 1, ...}
    ///
    /// 默认值：使用 StorageLayerConfig::default()
    #[pallet::storage]
    #[pallet::getter(fn storage_layer_config)]
    pub type StorageLayerConfigs<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        (SubjectType, PinTier), // (数据类型, Pin层级)
        StorageLayerConfig,
        ValueQuery, // 使用默认配置
    >;

    /// 函数级详细中文注释：CID的分层Pin分配记录存储
    ///
    /// 记录每个CID被分配到哪些Layer 1和Layer 2运营者，用于：
    /// 1. 审计和追溯
    /// 2. 费用分配（按层级分配收益）
    /// 3. 健康检查（分层验证副本数）
    /// 4. 数据迁移（Layer之间迁移）
    ///
    /// Key: CID Hash
    /// Value: LayeredPinAssignment<AccountId>
    ///
    /// 包含信息：
    /// - core_operators: Layer 1运营者列表
    /// - community_operators: Layer 2运营者列表
    #[pallet::storage]
    #[pallet::getter(fn layered_pin_assignments)]
    pub type LayeredPinAssignments<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash, // CID Hash
        LayeredPinAssignment<T::AccountId>,
        OptionQuery,
    >;

    // ============================================================================
    // 公共IPFS网络简化管理存储（无隐私约束版本）
    // ============================================================================

    /// ⚠️ DEPRECATED (P2): 使用 OperatorPinStats 替代。保留用于存储迁移兼容。
    /// 简化的节点统计信息（用于公共IPFS网络）
    #[pallet::storage]
    #[pallet::getter(fn simple_node_stats)]
    pub type SimpleNodeStatsMap<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        SimpleNodeStats<BlockNumberFor<T>>,
        ValueQuery,
    >;

    /// ⚠️ DEPRECATED (P2): 使用 PinAssignments + LayeredPinAssignments 替代。保留用于存储迁移兼容。
    /// 简化的PIN分配记录（公共IPFS网络）
    #[pallet::storage]
    #[pallet::getter(fn simple_pin_assignments)]
    pub type SimplePinAssignments<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,                               // CID Hash
        BoundedVec<T::AccountId, ConstU32<8>>, // 最多8个节点
        OptionQuery,
    >;

    /// 函数级详细中文注释：CID注册表（plaintext CID映射到hash）
    ///
    /// 存储CID的plaintext形式，用于OCW调用本地IPFS API
    #[pallet::storage]
    #[pallet::getter(fn cid_registry)]
    pub type CidRegistry<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::Hash,                       // CID Hash
        BoundedVec<u8, ConstU32<128>>, // Plaintext CID
        OptionQuery,
    >;

    /// 事件
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 请求已受理（cid_hash, payer, replicas, size, price）
        PinRequested(T::Hash, T::AccountId, u32, u64, T::Balance),
        /// 已提交到 ipfs-cluster（cid_hash）
        PinSubmitted(T::Hash),
        /// 标记已 Pin 成功（cid_hash, replicas）
        PinMarkedPinned(T::Hash, u32),
        /// 标记 Pin 失败（cid_hash, code）
        PinMarkedFailed(T::Hash, u16),
        /// 运营者相关事件
        OperatorJoined(T::AccountId),
        OperatorUpdated(T::AccountId),
        OperatorLeft(T::AccountId),
        OperatorStatusChanged(T::AccountId, u8),
        /// 函数级详细中文注释：运营者暂停（运营者自主操作）✅ P0-1新增
        OperatorPaused {
            operator: T::AccountId,
        },
        /// 函数级详细中文注释：运营者恢复（运营者自主操作）✅ P0-2新增
        OperatorResumed {
            operator: T::AccountId,
        },
        /// 函数级详细中文注释：运营者注销进入宽限期（有未完成Pin）✅ P0-3新增
        OperatorUnregistrationPending {
            operator: T::AccountId,
            remaining_pins: u32,
            expires_at: BlockNumberFor<T>,
        },
        /// 函数级详细中文注释：运营者注销完成（保证金已返还）✅ P0-3新增
        OperatorUnregistered {
            operator: T::AccountId,
        },
        /// 运营者探测结果（ok=true 表示在线且集群识别到该 Peer）
        OperatorProbed(T::AccountId, bool),
        /// 创建了副本分配（cid_hash, count）
        AssignmentCreated(T::Hash, u32),
        /// 状态迁移（cid_hash, state）
        PinStateChanged(T::Hash, u8),
        /// 副本降级与修复（cid_hash, operator）
        ReplicaDegraded(T::Hash, T::AccountId),
        ReplicaRepaired(T::Hash, T::AccountId),
        /// 降级累计达到告警阈值（operator, degraded_count）
        OperatorDegradationAlert(T::AccountId, u32),
        /// 主题账户已充值（subject_id, from, to, amount）- 已弃用
        SubjectFunded(u64, T::AccountId, T::AccountId, BalanceOf<T>),
        /// 用户存储账户已充值（混合方案）
        UserFunded {
            user: T::AccountId,
            funder: T::AccountId,
            funding_account: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 函数级中文注释：已完成一次周期扣费（cid_hash, amount, period_blocks, next_charge_at）。
        PinCharged(T::Hash, BalanceOf<T>, u32, BlockNumberFor<T>),
        /// 函数级中文注释：余额不足进入宽限期（cid_hash）。
        PinGrace(T::Hash),
        /// 函数级中文注释：超出宽限期仍欠费，标记过期（cid_hash）。
        PinExpired(T::Hash),
        /// 函数级中文注释：到期队列出入队统计（block, enqueued, dequeued, remaining）。
        DueQueueStats(BlockNumberFor<T>, u32, u32, u32),
        /// 函数级中文注释：OCW 巡检上报 Pin 状态汇总（样本数、pinning、pinned、missing）。
        PinProbe(u32, u32, u32, u32),
        /// 函数级中文注释：从 IPFS 池扣款成功（subject_id, amount, remaining_quota）
        ChargedFromIpfsPool {
            subject_id: u64,
            amount: BalanceOf<T>,
            remaining_quota: BalanceOf<T>,
        },
        /// 函数级中文注释：从 SubjectFunding 账户扣款成功（subject_id, amount）
        ChargedFromSubjectFunding {
            subject_id: u64,
            amount: BalanceOf<T>,
        },
        /// 从 Entity 国库扣款成功
        ChargedFromEntityTreasury {
            entity_id: u64,
            amount: BalanceOf<T>,
        },
        /// 函数级中文注释：配额已重置（subject_id, new_reset_block）
        QuotaReset {
            subject_id: u64,
            new_reset_block: BlockNumberFor<T>,
        },
        /// 函数级中文注释：IPFS 池余额不足警告（current_balance, threshold）
        IpfsPoolLowBalance {
            current_balance: BalanceOf<T>,
            threshold: BalanceOf<T>,
        },
        /// 函数级中文注释：从调用者账户扣款成功（fallback，自费模式）
        ChargedFromCaller {
            caller: T::AccountId,
            subject_id: u64,
            amount: BalanceOf<T>,
        },
        /// 函数级中文注释：运营者获得奖励分配（运营者账户、金额、权重、总权重）
        OperatorRewarded {
            operator: T::AccountId,
            amount: BalanceOf<T>,
            weight: u128,
            total_weight: u128,
        },
        /// 函数级中文注释：完成一轮奖励分配（总金额、运营者数量、平均权重）
        RewardDistributed {
            total_amount: BalanceOf<T>,
            operator_count: u32,
            average_weight: u128,
        },

        // ============================================================================
        // 新增事件：分层配置、健康巡检、自动化扣费（优化改造）
        // ============================================================================
        /// 函数级详细中文注释：分层配置已更新（治理调整）
        TierConfigUpdated {
            tier: PinTier,
            config: TierConfig,
        },

        /// 函数级详细中文注释：健康巡检结果（CID健康状态变化）
        HealthCheckCompleted {
            cid_hash: T::Hash,
            status: HealthStatus,
            next_check: BlockNumberFor<T>,
        },

        /// 函数级详细中文注释：健康状态降级（副本数不足）
        HealthDegraded {
            cid_hash: T::Hash,
            current_replicas: u32,
            target: u32,
        },

        /// 函数级详细中文注释：健康状态危险（副本数 < 2）
        HealthCritical {
            cid_hash: T::Hash,
            current_replicas: u32,
        },

        /// 函数级详细中文注释：健康巡检失败（网络错误等）
        HealthCheckFailed {
            cid_hash: T::Hash,
            failures: u8,
        },

        /// 函数级详细中文注释：自动修复触发（副本数降级）
        AutoRepairTriggered {
            cid_hash: T::Hash,
            current_replicas: u32,
            target: u32,
        },

        /// 函数级详细中文注释：自动修复完成
        AutoRepairCompleted {
            cid_hash: T::Hash,
            new_replicas: u32,
        },

        /// 函数级详细中文注释：宽限期开始（费用不足）
        GracePeriodStarted {
            cid_hash: T::Hash,
            expires_at: BlockNumberFor<T>,
        },

        /// 函数级详细中文注释：宽限期过期，标记Unpin
        GracePeriodExpired {
            cid_hash: T::Hash,
        },

        /// 函数级详细中文注释：CID已标记为待Unpin
        MarkedForUnpin {
            cid_hash: T::Hash,
            reason: UnpinReason,
        },

        /// on_idle 扫描到孤儿 CID（PinMeta 存在但 PinSubjectOf 缺失），已标记 unpin
        OrphanCidDetected {
            cid_hash: T::Hash,
        },

        /// 函数级详细中文注释：CID已从IPFS物理删除（OCW执行unpin成功）
        ///
        /// 触发时机：
        /// - OCW扫描到过期CID（PinBilling.state=2）
        /// - 调用submit_delete_pin成功
        /// - 清理所有链上存储完成
        PinRemoved {
            cid_hash: T::Hash,
            reason: UnpinReason,
        },

        /// 函数级详细中文注释：IPFS公共池余额不足警告（需要补充）
        IpfsPoolLowBalanceWarning {
            current: BalanceOf<T>,
        },

        /// 函数级详细中文注释：运营者提取奖励
        RewardsClaimed {
            operator: T::AccountId,
            amount: BalanceOf<T>,
        },

        /// 函数级详细中文注释：扣费已暂停（紧急开关）
        BillingPausedByGovernance {
            by: T::AccountId,
        },

        /// 函数级详细中文注释：扣费已恢复
        BillingResumedByGovernance {
            by: T::AccountId,
        },

        // ============================================================================
        // 运营者监控相关Events（阶段1：链上基础监控）
        // ============================================================================
        /// 函数级详细中文注释：运营者容量告警（使用率超过80%）
        ///
        /// 触发时机：
        /// - Pin分配时检查容量
        /// - 健康检查时定期检查
        ///
        /// 使用场景：
        /// - 运营者Dashboard展示告警
        /// - 提醒运营者扩容
        /// - Pin分配算法避开高负载运营者
        OperatorCapacityWarning {
            operator: T::AccountId,
            used_capacity_gib: u32,
            total_capacity_gib: u32,
            usage_percent: u8,
        },

        /// 函数级详细中文注释：运营者健康度下降（得分下降超过10分）
        ///
        /// 触发时机：
        /// - Pin失败时
        /// - 健康检查发现副本数不足时
        ///
        /// 使用场景：
        /// - 运营者Dashboard展示告警
        /// - 评估运营者服务质量
        /// - 治理决策参考（淘汰低质量运营者）
        OperatorHealthDegraded {
            operator: T::AccountId,
            old_score: u8,
            new_score: u8,
            total_pins: u32,
            failed_pins: u32,
        },

        /// 函数级详细中文注释：Pin已分配给运营者
        ///
        /// 触发时机：
        /// - request_pin_for_subject 成功分配Pin给运营者
        ///
        /// 使用场景：
        /// - 运营者Dashboard展示新任务
        /// - 追踪Pin分配历史
        /// - 负载均衡分析
        PinAssignedToOperator {
            operator: T::AccountId,
            cid_hash: T::Hash,
            current_pins: u32,
            capacity_usage_percent: u8,
        },

        /// 函数级详细中文注释：运营者Pin成功
        ///
        /// 触发时机：
        /// - OCW确认Pin成功且副本数达标
        ///
        /// 使用场景：
        /// - 运营者Dashboard展示成功率
        /// - 计算运营者信誉评分
        OperatorPinSuccess {
            operator: T::AccountId,
            cid_hash: T::Hash,
            replicas_confirmed: u32,
        },

        /// 函数级详细中文注释：运营者Pin失败
        ///
        /// 触发时机：
        /// - OCW检测到Pin失败
        /// - IPFS Cluster API返回错误
        ///
        /// 使用场景：
        /// - 运营者Dashboard展示失败原因
        /// - 自动触发重新分配
        /// - 降低运营者健康度得分
        OperatorPinFailed {
            operator: T::AccountId,
            cid_hash: T::Hash,
            reason: BoundedVec<u8, ConstU32<128>>,
        },

        /// 函数级详细中文注释：核心运营者不足告警
        ///
        /// 触发时机：
        /// - select_operators_by_layer 时Layer 1运营者数量不足
        ///
        /// 告警级别：高（可能影响服务）
        ///
        /// 建议措施：
        /// - 立即增加Layer 1运营者
        /// - 或调整存储策略配置
        CoreOperatorShortage {
            required: u32,
            available: u32,
        },

        /// 函数级详细中文注释：社区运营者不足告警
        ///
        /// 触发时机：
        /// - select_operators_by_layer 时Layer 2运营者数量不足
        ///
        /// 告警级别：中（不影响核心服务）
        ///
        /// 建议措施：
        /// - 增加社区运营者激励
        /// - 调整Layer 2副本配置
        CommunityOperatorShortage {
            required: u32,
            available: u32,
        },

        /// 函数级详细中文注释：分层Pin分配完成
        ///
        /// 触发时机：
        /// - request_pin_for_subject 成功分配Pin
        ///
        /// 包含信息：
        /// - Layer 1运营者列表
        /// - Layer 2运营者列表
        /// - 是否使用Layer 3
        LayeredPinAssigned {
            cid_hash: T::Hash,
            core_operators: BoundedVec<T::AccountId, ConstU32<8>>,
            community_operators: BoundedVec<T::AccountId, ConstU32<8>>,
        },

        /// 函数级详细中文注释：分层策略配置更新
        ///
        /// 触发时机：
        /// - 治理调用 set_storage_layer_config
        ///
        /// 使用场景：
        /// - 审计配置变更历史
        /// - 前端Dashboard展示
        StorageLayerConfigUpdated {
            subject_type: SubjectType,
            tier: PinTier,
            core_replicas: u32,
            community_replicas: u32,
        },

        /// 函数级详细中文注释：运营者层级更新
        ///
        /// 触发时机：
        /// - 治理调用 set_operator_layer
        ///
        /// 使用场景：
        /// - 审计运营者层级变更
        /// - 追踪Layer 1/2迁移
        OperatorLayerUpdated {
            operator: T::AccountId,
            layer: OperatorLayer,
            priority: u8,
        },

        // ============================================================================
        // 公共IPFS网络简化事件（无隐私约束版本）
        // ============================================================================
        /// 函数级详细中文注释：简化PIN分配完成事件
        ///
        /// 当CID成功分配到指定节点时触发，记录：
        /// - cid_hash：CID的hash值
        /// - tier：PIN层级（Critical/Standard/Temporary）
        /// - nodes：分配到的节点列表
        /// - replicas：实际分配的副本数
        SimplePinAllocated {
            cid_hash: T::Hash,
            tier: PinTier,
            nodes: BoundedVec<T::AccountId, ConstU32<8>>,
            replicas: u32,
        },

        /// 函数级详细中文注释：PIN状态报告事件（OCW上报）
        ///
        /// OCW健康检查后上报PIN状态：
        /// - cid_hash：CID的hash值
        /// - node：上报的节点
        /// - status：PIN状态（Pinned/Failed/Restored）
        SimplePinStatusReported {
            cid_hash: T::Hash,
            node: T::AccountId,
            status: SimplePinStatus,
        },

        /// 函数级详细中文注释：节点负载警告事件
        ///
        /// 当节点容量使用率超过阈值时触发：
        /// - node：节点账户
        /// - capacity_usage：容量使用率（0-100）
        /// - current_pins：当前PIN数量
        SimpleNodeLoadWarning {
            node: T::AccountId,
            capacity_usage: u8,
            current_pins: u32,
        },

        // ============================================================================
        // 新pallet域自动PIN机制相关Events
        // ============================================================================
        /// 函数级详细中文注释：域已注册
        ///
        /// 触发时机：
        /// - 新业务pallet首次调用register_content时自动注册
        /// - 治理手动注册域时
        ///
        /// 使用场景：
        /// - 追踪系统中的所有业务域
        /// - 域管理Dashboard展示
        DomainRegistered {
            domain: BoundedVec<u8, ConstU32<32>>,
            subject_type_id: u8,
        },

        /// 函数级详细中文注释：内容已通过域注册
        ///
        /// 触发时机：
        /// - 业务pallet调用ContentRegistry::register_content成功
        ///
        /// 使用场景：
        /// - 追踪域级别的内容注册
        /// - 统计各域的存储使用量
        ContentRegisteredViaDomain {
            domain: BoundedVec<u8, ConstU32<32>>,
            subject_id: u64,
            cid_hash: T::Hash,
            tier: PinTier,
        },

        /// 函数级详细中文注释：域配置已更新
        ///
        /// 触发时机：
        /// - 治理更新域配置（启用/禁用自动PIN，修改默认tier等）
        ///
        /// 使用场景：
        /// - 域管理日志
        DomainConfigUpdated {
            domain: BoundedVec<u8, ConstU32<32>>,
            auto_pin_enabled: bool,
        },

        /// 函数级详细中文注释：域统计已更新
        ///
        /// OCW按域扫描统计后触发，包含该域的完整统计信息
        ///
        /// 字段说明：
        /// - domain：域名
        /// - total_pins：该域的总Pin数量
        /// - total_size_bytes：该域的总存储容量（字节）
        /// - healthy_count：健康CID数量
        /// - degraded_count：降级CID数量
        /// - critical_count：危险CID数量
        ///
        /// 使用场景：
        /// - Dashboard实时更新域级统计
        /// - 监控系统告警
        /// - 统计报表生成
        DomainStatsUpdated {
            domain: BoundedVec<u8, ConstU32<32>>,
            total_pins: u64,
            total_size_bytes: u64,
            healthy_count: u64,
            degraded_count: u64,
            critical_count: u64,
        },

        /// 函数级详细中文注释：域优先级已设置
        ///
        /// 治理调用 set_domain_priority 后触发
        ///
        /// 字段说明：
        /// - domain：域名
        /// - priority：新的优先级（0-255，0为最高）
        ///
        /// 使用场景：
        /// - 治理日志
        /// - 优先级调整追踪
        DomainPrioritySet {
            domain: BoundedVec<u8, ConstU32<32>>,
            priority: u8,
        },

        /// 函数级详细中文注释：治理强制下架CID
        ///
        /// 触发时机：
        /// - 治理调用 governance_force_unpin
        ///
        /// 使用场景：
        /// - 违规内容下架审计
        /// - 紧急处置记录
        GovernanceForceUnpinned {
            cid_hash: T::Hash,
            reason: BoundedVec<u8, ConstU32<256>>,
        },

        /// 函数级详细中文注释：运营者保证金被扣罚 ✅ P1-10新增
        OperatorSlashed {
            operator: T::AccountId,
            amount: BalanceOf<T>,
        },

        /// 函数级详细中文注释：运营者因健康分过低被自动暂停 ✅ P1-12新增
        OperatorAutoSuspended {
            operator: T::AccountId,
            health_score: u8,
        },

        /// 函数级详细中文注释：IPFS 公共池已充值 ✅ P1-14新增
        IpfsPoolFunded {
            who: T::AccountId,
            amount: BalanceOf<T>,
            new_balance: BalanceOf<T>,
        },

        /// 函数级详细中文注释：Pin已续期 ✅ P1-2新增
        PinRenewed {
            cid_hash: T::Hash,
            periods: u32,
            total_fee: BalanceOf<T>,
        },

        /// 函数级详细中文注释：Pin分层等级已升级 ✅ P1-2新增
        PinTierUpgraded {
            cid_hash: T::Hash,
            old_tier: PinTier,
            new_tier: PinTier,
            fee_diff: BalanceOf<T>,
        },

        /// 函数级详细中文注释：提前取消Pin按比例退款 ✅ P1-3新增
        UnpinRefund {
            cid_hash: T::Hash,
            owner: T::AccountId,
            refund: BalanceOf<T>,
        },

        /// 函数级详细中文注释：运营者Pin迁移完成 ✅ P1-9新增
        OperatorPinsMigrated {
            from: T::AccountId,
            to: T::AccountId,
            pins_migrated: u32,
            bytes_moved: u64,
        },

        /// 函数级详细中文注释：批量取消Pin完成 ✅ P2-5新增
        BatchUnpinCompleted {
            who: T::AccountId,
            requested: u32,
            unpinned: u32,
        },

        /// 函数级详细中文注释：运营者保证金追加 ✅ P2-8新增
        BondTopUp {
            operator: T::AccountId,
            amount: BalanceOf<T>,
            new_total: BalanceOf<T>,
        },

        /// 函数级详细中文注释：运营者保证金减少 ✅ P2-8新增
        BondReduced {
            operator: T::AccountId,
            amount: BalanceOf<T>,
            new_total: BalanceOf<T>,
        },

        /// 函数级详细中文注释：CID已锁定（仲裁保护） ✅ P2-19新增
        CidLocked {
            cid_hash: T::Hash,
            until: Option<BlockNumberFor<T>>,
        },

        /// 函数级详细中文注释：CID已解锁 ✅ P2-19新增
        CidUnlocked {
            cid_hash: T::Hash,
        },

        /// 计费参数已更新（治理操作审计）
        BillingParamsUpdated,

        /// 运营者奖励部分领取（池余额不足时仅部分到账）
        RewardsClaimPartial {
            operator: T::AccountId,
            claimed: BalanceOf<T>,
            unclaimed: BalanceOf<T>,
        },

        /// 用户提取存储资金
        UserFundingWithdrawn {
            user: T::AccountId,
            amount: BalanceOf<T>,
        },

        /// Pin 分层等级已降级
        PinTierDowngraded {
            cid_hash: T::Hash,
            old_tier: PinTier,
            new_tier: PinTier,
        },

        /// 运营者对 slash 发起争议
        SlashDisputed {
            operator: T::AccountId,
            amount: BalanceOf<T>,
            reason: BoundedVec<u8, ConstU32<256>>,
        },

        /// 运营者奖励分配失败（扣费已完成但分配环节出错）
        DistributionFailed {
            cid_hash: T::Hash,
            amount: BalanceOf<T>,
        },

        /// CID 清理退款失败（cleanup 正常继续，仅记录退款失败）
        RefundFailed {
            owner: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    #[pallet::error]
    #[derive(PartialEq)]
    pub enum Error<T> {
        /// 参数非法
        BadParams,
        /// 订单不存在
        OrderNotFound,
        /// 运营者不存在
        OperatorNotFound,
        /// 运营者已存在
        OperatorExists,
        /// 运营者已被禁用
        OperatorBanned,
        /// 函数级详细中文注释：运营者已暂停（无法再次暂停）✅ P0-1新增
        AlreadyPaused,
        /// 函数级详细中文注释：运营者未暂停（无法恢复）✅ P0-2新增
        NotPaused,
        /// 保证金不足
        InsufficientBond,
        /// 容量不足
        InsufficientCapacity,
        /// 无效状态
        BadStatus,
        /// 分配不存在
        AssignmentNotFound,
        /// 仍存在未完成的副本分配，禁止退出
        HasActiveAssignments,
        /// 调用方未被指派到该内容的副本分配中
        OperatorNotAssigned,
        // ⭐ P2优化：已删除 DirectPinDisabled 错误（旧版 request_pin() 已删除）
        /// SubjectFunding 账户余额不足
        SubjectFundingInsufficientBalance,
        /// CID已经被pin，禁止重复pin
        CidAlreadyPinned,
        /// 函数级中文注释：没有活跃的运营者（无法进行奖励分配）
        NoActiveOperators,
        /// 函数级中文注释：运营者托管账户余额不足
        InsufficientEscrowBalance,
        /// 函数级中文注释：计算权重时发生溢出
        WeightOverflow,

        // ============================================================================
        // 新增错误：分层配置、健康巡检、自动化扣费（优化改造）
        // ============================================================================
        /// 函数级详细中文注释：域名太长（超过32字节）
        DomainTooLong,
        /// 函数级详细中文注释：Subject未找到（CID无归属）
        SubjectNotFound,
        /// 函数级详细中文注释：非所有者（无权限操作）
        NotOwner,
        /// 函数级详细中文注释：已经Pin过（避免重复）
        AlreadyPinned,
        /// 函数级详细中文注释：副本数无效（必须1-10）
        InvalidReplicas,
        /// 函数级详细中文注释：巡检间隔太短（必须≥600块，约30分钟）
        IntervalTooShort,
        /// 函数级详细中文注释：费率系数无效（必须0.1x-10x）
        InvalidMultiplier,
        /// 函数级详细中文注释：宽限期已过（无法再扣费）
        GraceExpired,
        /// 函数级详细中文注释：没有分配运营者（Pin未成功）
        NoOperatorsAssigned,
        /// 函数级详细中文注释：没有可用奖励（余额为零）
        NoRewardsAvailable,
        /// 函数级详细中文注释：分层配置未找到
        TierConfigNotFound,
        /// 函数级详细中文注释：健康巡检任务未找到
        HealthCheckTaskNotFound,
        /// 函数级详细中文注释：扣费任务未找到
        BillingTaskNotFound,
        /// 函数级详细中文注释：可用运营者不足（P0-1新增）
        ///
        /// 触发场景：
        /// - 活跃运营者数量 < 需要的副本数
        /// - 容量充足的运营者数量 < 需要的副本数
        ///
        /// 解决方案：
        /// - 增加运营者注册
        /// - 运营者扩容
        /// - 降低副本数需求
        NotEnoughOperators,

        // ============================================================================
        // 公共IPFS网络简化错误（无隐私约束版本）
        // ============================================================================
        /// 函数级详细中文注释：没有可用的IPFS运营者
        ///
        /// 触发场景：
        /// - 所有运营者都是Suspended或Banned状态
        /// - 没有注册任何运营者
        NoAvailableOperators,

        /// 函数级详细中文注释：节点数量不足
        ///
        /// 触发场景：
        /// - 活跃节点数量 < 需要的副本数
        /// - 容量充足的节点 < 需要的副本数
        InsufficientNodes,

        /// 函数级详细中文注释：节点数量过多
        ///
        /// 触发场景：
        /// - 尝试分配超过BoundedVec限制的节点数
        TooManyNodes,

        /// 活跃运营者索引已满（ActiveOperatorIndex 达到上限 256）
        ActiveOperatorIndexFull,

        // ============================================================================
        // 新pallet域自动PIN机制相关Errors
        // ============================================================================
        /// 函数级详细中文注释：无效的域名（长度超过限制或包含非法字符）
        InvalidDomain,
        /// 函数级详细中文注释：域的自动PIN已禁用
        DomainPinDisabled,
        /// 函数级详细中文注释：域不存在
        DomainNotFound,
        /// 域已存在（尝试重复注册）
        DomainAlreadyExists,
        /// 用户存储资金余额不足（提取）
        InsufficientUserFunding,
        /// 无效的 Pin Tier 降级（只能向下降级）
        InvalidTierDowngrade,
        /// CID 格式无效
        InvalidCidFormat,
    }

    impl<T: Config> Pallet<T> {
        /// Pin 核心逻辑（内部函数），由 extrinsic、IpfsPinner、ContentRegistry 共用。
        /// `subject_type` 决定 CidToSubject 的类型标记和 DomainPins 的域索引。
        pub(crate) fn do_request_pin(
            caller: T::AccountId,
            subject_type: SubjectType,
            subject_id: u64,
            entity_id: Option<u64>,
            cid: Vec<u8>,
            size_bytes: u64,
            tier: Option<PinTier>,
        ) -> DispatchResult {
            use sp_runtime::traits::Hash;

            Self::validate_cid(&cid)?;

            let cid_hash = T::Hashing::hash(&cid[..]);

            ensure!(
                !PinMeta::<T>::contains_key(&cid_hash),
                Error::<T>::AlreadyPinned
            );

            let tier = tier.unwrap_or(PinTier::Standard);
            let tier_config = Self::get_tier_config(&tier)?;

            ensure!(size_bytes > 0, Error::<T>::BadParams);

            let base_fee = Self::calculate_initial_pin_fee(size_bytes, tier_config.replicas)?;
            let adjusted_fee =
                base_fee.saturating_mul(tier_config.fee_multiplier.into()) / 10000u32.into();

            let current_block = <frame_system::Pallet<T>>::block_number();
            let mut temp_task = BillingTask {
                billing_period: T::DefaultBillingPeriod::get(),
                amount_per_period: adjusted_fee,
                last_charge: current_block,
                grace_status: GraceStatus::Normal,
                charge_layer: ChargeLayer::IpfsPool,
            };

            let subject_info = SubjectInfo {
                subject_type: subject_type.clone(),
                subject_id,
            };
            let subject_vec =
                BoundedVec::try_from(vec![subject_info]).map_err(|_| Error::<T>::BadParams)?;
            CidToSubject::<T>::insert(&cid_hash, subject_vec);

            let _simple_nodes = Self::optimized_pin_allocation(cid_hash, tier.clone(), size_bytes)?;

            let selection = Self::select_operators_by_layer(subject_type.clone(), tier.clone())?;

            let mut all_operators = selection.core_operators.to_vec();
            all_operators.extend(selection.community_operators.to_vec());

            for operator in selection.core_operators.iter() {
                Self::update_operator_pin_stats(operator, 1, 0)?;
                Self::check_operator_capacity_warning(operator);

                let current_pins = Self::count_operator_pins(operator);
                let capacity_usage_percent = Self::calculate_capacity_usage(operator);

                Self::deposit_event(Event::PinAssignedToOperator {
                    operator: operator.clone(),
                    cid_hash,
                    current_pins,
                    capacity_usage_percent,
                });
            }

            for operator in selection.community_operators.iter() {
                Self::update_operator_pin_stats(operator, 1, 0)?;
                Self::check_operator_capacity_warning(operator);

                let current_pins = Self::count_operator_pins(operator);
                let capacity_usage_percent = Self::calculate_capacity_usage(operator);

                Self::deposit_event(Event::PinAssignedToOperator {
                    operator: operator.clone(),
                    cid_hash,
                    current_pins,
                    capacity_usage_percent,
                });
            }

            let core_ops_for_storage = BoundedVec::truncate_from(selection.core_operators.to_vec());
            let community_ops_for_storage =
                BoundedVec::truncate_from(selection.community_operators.to_vec());

            LayeredPinAssignments::<T>::insert(
                &cid_hash,
                LayeredPinAssignment {
                    core_operators: core_ops_for_storage.clone(),
                    community_operators: community_ops_for_storage.clone(),
                },
            );

            Self::deposit_event(Event::LayeredPinAssigned {
                cid_hash,
                core_operators: core_ops_for_storage,
                community_operators: community_ops_for_storage,
            });

            let operators_bounded =
                BoundedVec::try_from(all_operators).map_err(|_| Error::<T>::BadParams)?;
            for op in operators_bounded.iter() {
                OperatorPinCount::<T>::mutate(op, |c| *c = c.saturating_add(1));
                OperatorUsedBytes::<T>::mutate(op, |b| *b = b.saturating_add(size_bytes));
            }
            PinAssignments::<T>::insert(&cid_hash, operators_bounded);

            // PinSubjectOf + CidEntityOf must be written BEFORE four_layer_charge
            // so that owner / entity lookups succeed for all charge layers.
            PinSubjectOf::<T>::insert(&cid_hash, (caller.clone(), subject_id));
            if let Some(eid) = entity_id {
                CidEntityOf::<T>::insert(&cid_hash, eid);
            }

            match Self::four_layer_charge(&cid_hash, &mut temp_task) {
                Ok(ChargeResult::Success { layer: _ }) => {}
                Ok(ChargeResult::EnterGrace { .. }) => {
                    Self::deposit_event(Event::IpfsPoolLowBalanceWarning {
                        current: T::Currency::free_balance(&T::IpfsPoolAccount::get()),
                    });
                }
                Err(e) => {
                    PinSubjectOf::<T>::remove(&cid_hash);
                    CidEntityOf::<T>::remove(&cid_hash);
                    return Err(e.into());
                }
            }

            let cid_bounded =
                BoundedVec::try_from(cid.clone()).map_err(|_| Error::<T>::BadParams)?;
            CidRegistry::<T>::insert(&cid_hash, cid_bounded);

            let domain = BoundedVec::try_from(subject_type.to_domain_name())
                .map_err(|_| Error::<T>::DomainTooLong)?;
            DomainPins::<T>::insert(&domain, &cid_hash, ());

            CidTier::<T>::insert(&cid_hash, tier.clone());

            let next_check = current_block.saturating_add(tier_config.health_check_interval.into());
            let check_task = HealthCheckTask {
                tier: tier.clone(),
                last_check: current_block,
                last_status: HealthStatus::Unknown,
                consecutive_failures: 0,
            };
            HealthCheckQueue::<T>::insert(next_check, &cid_hash, check_task);

            let period_fee = Self::calculate_period_fee(size_bytes, tier_config.replicas)?;
            let period_fee_adjusted =
                period_fee.saturating_mul(tier_config.fee_multiplier.into()) / 10000u32.into();
            let billing_period = T::DefaultBillingPeriod::get();
            let next_billing = current_block.saturating_add(billing_period.into());
            let billing_task = BillingTask {
                billing_period,
                amount_per_period: period_fee_adjusted,
                last_charge: current_block,
                grace_status: GraceStatus::Normal,
                charge_layer: ChargeLayer::IpfsPool,
            };
            BillingQueue::<T>::insert(next_billing, &cid_hash, billing_task);
            CidBillingDueBlock::<T>::insert(&cid_hash, next_billing);

            let meta = PinMetadata {
                replicas: tier_config.replicas,
                size: size_bytes,
                created_at: current_block,
                last_activity: current_block,
            };
            PinMeta::<T>::insert(&cid_hash, meta);

            PendingPins::<T>::insert(
                &cid_hash,
                (
                    caller.clone(),
                    tier_config.replicas,
                    subject_id,
                    size_bytes,
                    adjusted_fee,
                ),
            );
            PinStateOf::<T>::insert(&cid_hash, 0u8);

            OwnerPinIndex::<T>::mutate(&caller, |cids| {
                let _ = cids.try_push(cid_hash);
            });

            // 写入 CID 归档索引（供 pallet-storage-lifecycle 使用）
            let archive_id = NextCidArchiveId::<T>::get();
            CidArchiveIndex::<T>::insert(archive_id, cid_hash);
            CidArchiveReverseIndex::<T>::insert(cid_hash, archive_id);
            NextCidArchiveId::<T>::put(archive_id.saturating_add(1));

            Self::deposit_event(Event::PinRequested(
                cid_hash,
                caller,
                tier_config.replicas,
                size_bytes,
                adjusted_fee,
            ));

            Ok(())
        }

        // ⭐ P0优化：已删除 select_operators_by_weight() 函数（82行）
        // 原因：所有引用已迁移到 select_operators_by_layer()
        // 迁移完成位置：
        // - offchain_worker 初始分配 (行4176)
        // - offchain_worker 补充副本 (行4382)
        // 删除日期：2025-10-26

        /// 函数级详细中文注释：根据 (domain, subject_id) 计算派生子账户（稳定派生，与创建者/拥有者解耦）
        /// - 使用 `SubjectPalletId.into_sub_account_truncating((domain:u8, subject_id:u64))` 派生稳定地址
        /// - 该账户无私钥，不可外发，仅用于托管与扣费
        ///
        /// ⚠️ 已弃用：请使用 derive_user_funding_account() 替代
        /// 保留此函数仅为向后兼容
        #[inline]
        #[deprecated(note = "请使用 derive_user_funding_account() 替代，混合方案每用户一个账户")]
        pub fn subject_account_for(domain: u8, subject_id: u64) -> T::AccountId {
            T::SubjectPalletId::get().into_sub_account_truncating((domain, subject_id))
        }

        /// 函数级详细中文注释：根据用户账户派生存储资金账户（混合方案）
        ///
        /// 设计理念：
        /// - 每个用户一个派生账户，替代每个 Subject 一个账户
        /// - 大幅减少派生账户数量（从 N×M 降到 N）
        /// - 用户只需充值一个账户，管理简单
        ///
        /// 派生公式：
        /// - SubjectPalletId.into_sub_account_truncating(("user", user_account))
        ///
        /// 特点：
        /// - 确定性：相同用户 → 永远相同地址
        /// - 无私钥：链上逻辑控制
        /// - 可验证：任何人可根据用户地址计算出派生地址
        #[inline]
        pub fn derive_user_funding_account(user: &T::AccountId) -> T::AccountId {
            T::SubjectPalletId::get().into_sub_account_truncating((b"user", user))
        }

        /// 函数级中文注释：将 SubjectType 转换为 domain 编号（用于 SubjectUsage 记账）
        #[inline]
        pub fn subject_type_to_domain(subject_type: &SubjectType) -> u8 {
            match subject_type {
                SubjectType::Evidence => 0,
                SubjectType::Product => 10,
                SubjectType::Entity => 11,
                SubjectType::Shop => 12,
                SubjectType::General => 98,
                SubjectType::Custom(_) => 99,
            }
        }
        /// 查询用户所有 CID 列表及元信息（供 RPC 调用）。
        pub fn get_user_cids(
            owner: &T::AccountId,
        ) -> Vec<(T::Hash, PinMetadata<BlockNumberFor<T>>)> {
            let cids = OwnerPinIndex::<T>::get(owner);
            let mut result: Vec<(T::Hash, PinMetadata<BlockNumberFor<T>>)> = Vec::new();
            for cid_hash in cids.iter() {
                if let Some(meta) = PinMeta::<T>::get(cid_hash) {
                    result.push((*cid_hash, meta));
                }
            }
            result
        }

        /// 函数级详细中文注释：CID 解密/映射内部工具函数（非外部可调用）
        /// - 从 offchain local storage 读取 `/memo/ipfs/cid/<hash_hex>` 对应的明文 CID；
        /// - 若不存在，返回占位 `"<redacted>"`，用于上层降级处理。
        #[inline]
        fn resolve_cid(cid_hash: &T::Hash) -> alloc::string::String {
            let mut key = b"/memo/ipfs/cid/".to_vec();
            let hex = hex::encode(cid_hash.as_ref());
            key.extend_from_slice(hex.as_bytes());
            if let Some(bytes) = sp_io::offchain::local_storage_get(StorageKind::PERSISTENT, &key) {
                if let Ok(s) = core::str::from_utf8(&bytes) {
                    return s.into();
                }
            }
            "<redacted>".into()
        }

        // ⭐ P1优化：已删除 derive_subject_funding_account() 函数（39行）
        // 原因：所有引用已迁移到 derive_subject_funding_account_v2()
        // 迁移完成位置：fund_subject_account() extrinsic (行2491)
        // 删除日期：2025-10-26
        //
        // 新函数优势：
        // - 支持多种SubjectType（Evidence/Product/Entity/Shop/General/Custom）
        // - 统一的派生逻辑
        // - 向后兼容（Subject使用相同的domain）

        // ⭐ P0优化：已删除 dual_charge_storage_fee() 函数（131行）
        // 原因：所有引用已迁移到 four_layer_charge()
        // 迁移完成位置：charge_due() extrinsic (行3270)
        // 删除日期：2025-10-26

        // ⭐ P1优化：已删除 triple_charge_storage_fee() 函数（160行）
        // 原因：所有引用已迁移到 four_layer_charge()
        // 旧版调用位置：old_pin_cid_for_subject()（已同时删除）
        // 删除日期：2025-10-26

        // ============================================================================
        // 新增辅助函数：分层配置、健康巡检、自动化扣费（优化改造）
        // ============================================================================

        /// 函数级详细中文注释：获取分层配置（带默认值）
        ///
        /// 如果链上没有配置，返回默认值：
        /// - Critical: 5副本，7200块（6小时），1.5x费率
        /// - Standard: 3副本，28800块（24小时），1.0x费率
        /// - Temporary: 1副本，604800块（7天），0.5x费率
        pub fn get_tier_config(tier: &PinTier) -> Result<TierConfig, Error<T>> {
            let config = PinTierConfig::<T>::get(tier);

            if config.enabled {
                Ok(config)
            } else {
                Ok(match tier {
                    PinTier::Critical => TierConfig::critical_default(),
                    PinTier::Standard => TierConfig::default(),
                    PinTier::Temporary => TierConfig::temporary_default(),
                })
            }
        }

        /// CID 格式基本校验：长度 + 前缀合法性。
        /// CIDv0 (Qm...) 以 0x12 0x20 开头（46 字节 Base58）；
        /// CIDv1 以 0x01 开头（Base32/Base58btc）。
        /// 此处仅做基本长度和前缀检查，不做完整的 multibase/multicodec 解析。
        pub fn validate_cid(cid: &[u8]) -> Result<(), Error<T>> {
            ensure!(cid.len() >= 2 && cid.len() <= 128, Error::<T>::BadParams);
            let first = cid[0];
            let is_v0 = first == b'Q' && cid.len() >= 46;
            let is_v1 = first == b'b' || first == b'z' || first == b'f';
            let is_binary_v0 = first == 0x12;
            let is_binary_v1 = first == 0x01;
            ensure!(
                is_v0 || is_v1 || is_binary_v0 || is_binary_v1,
                Error::<T>::BadParams
            );
            Ok(())
        }

        /// 函数级详细中文注释：根据SubjectType派生资金账户
        ///
        /// 派生规则：
        /// - Evidence: (domain=0, subject_id)
        /// - Product: (domain=10, subject_id)
        /// - Entity: (domain=11, subject_id)
        /// - Shop: (domain=12, subject_id)
        /// - General: (domain=98, subject_id)
        /// - Custom: (domain=99, subject_id)
        /// 函数级详细中文注释：统计运营者的Pin数量 ✅ P0-3新增
        ///
        /// ### 功能
        /// - 遍历PinAssignments，统计该运营者被分配的Pin数量
        /// - 用于leave_operator判断是否需要进入宽限期
        ///
        /// ### 复杂度
        /// - O(n)，n为所有Pin总数
        /// - MVP实现，生产环境建议增加索引优化
        /// H1修复：使用 OperatorPinCount 索引替代全表扫描，O(1) 复杂度
        pub fn count_operator_pins(operator: &T::AccountId) -> u32 {
            OperatorPinCount::<T>::get(operator)
        }

        /// 函数级详细中文注释：完成运营者注销（内部函数）✅ P0-3新增
        ///
        /// ### 功能
        /// - 返还保证金
        /// - 移除运营者记录
        /// - 移除宽限期记录（如有）
        /// - 发送OperatorUnregistered事件
        ///
        /// ### 调用场景
        /// 1. leave_operator：无Pin时立即调用
        /// 2. on_finalize：宽限期到期且无Pin时调用
        pub fn finalize_operator_unregistration(operator: &T::AccountId) -> DispatchResult {
            // 返还保证金
            let bond = OperatorBond::<T>::take(operator);
            if !bond.is_zero() {
                let _ = <T as Config>::Currency::unreserve(operator, bond);
            }

            // 移除运营者记录
            Operators::<T>::remove(operator);

            // ✅ P0-16：从活跃索引移除（如仍在）
            ActiveOperatorIndex::<T>::mutate(|index| {
                index.retain(|a| a != operator);
            });

            // 移除宽限期记录（如果存在）
            PendingUnregistrations::<T>::remove(operator);

            // 发送事件
            Self::deposit_event(Event::OperatorUnregistered {
                operator: operator.clone(),
            });

            Ok(())
        }

        // ============================================================================
        // 运营者监控相关辅助函数（阶段1：链上基础监控）
        // ============================================================================

        /// 函数级详细中文注释：更新运营者Pin统计（核心监控函数）
        ///
        /// ### 功能
        /// - 更新运营者的Pin统计数据（total_pins, healthy_pins, failed_pins）
        /// - 重新计算健康度得分
        /// - 健康度下降超过10分时自动发送告警Event
        ///
        /// ### 参数
        /// - `operator`: 运营者账户
        /// - `delta_total`: Pin总数变化（+1分配，-1移除）
        /// - `delta_failed`: 失败Pin数变化（+1失败）
        ///
        /// ### 调用场景
        /// 1. Pin分配时：delta_total=+1, delta_failed=0
        /// 2. Pin失败时：delta_total=0, delta_failed=+1
        /// 3. Pin移除时：delta_total=-1, delta_failed=0
        ///
        /// ### 注意
        /// - 健康度得分会在每次调用时重新计算
        /// - 下降超过10分会触发OperatorHealthDegraded事件
        pub fn update_operator_pin_stats(
            operator: &T::AccountId,
            delta_total: i32,
            delta_failed: i32,
        ) -> DispatchResult {
            OperatorPinStats::<T>::try_mutate(operator, |stats| -> DispatchResult {
                // 更新Pin总数
                if delta_total > 0 {
                    stats.total_pins = stats.total_pins.saturating_add(delta_total as u32);
                } else if delta_total < 0 {
                    stats.total_pins = stats.total_pins.saturating_sub((-delta_total) as u32);
                }

                // 更新失败数
                if delta_failed > 0 {
                    stats.failed_pins = stats.failed_pins.saturating_add(delta_failed as u32);
                }

                // 重新计算健康度得分
                let old_score = stats.health_score;
                stats.health_score = Self::calculate_health_score(operator);
                stats.last_check = <frame_system::Pallet<T>>::block_number();

                // 如果健康度下降超过10分，发射Event
                if old_score.saturating_sub(stats.health_score) >= 10 {
                    Self::deposit_event(Event::OperatorHealthDegraded {
                        operator: operator.clone(),
                        old_score,
                        new_score: stats.health_score,
                        total_pins: stats.total_pins,
                        failed_pins: stats.failed_pins,
                    });
                }

                Ok(())
            })
        }

        /// 函数级详细中文注释：计算运营者健康度得分（智能评分算法）
        ///
        /// ### 评分算法
        /// 1. 基础分：60分
        /// 2. 健康Pin比例奖励：(healthy_pins / total_pins) * 40，最多+40分
        /// 3. 失败率惩罚：(failed_pins / total_pins) * 100 * 2，每1%失败率扣2分，最多扣60分
        /// 4. 最终得分：max(0, min(100, 60 + 健康奖励 - 失败惩罚))
        ///
        /// ### 示例
        /// - 无Pin：100分（初始满分）
        /// - 100个Pin，100个健康，0个失败：100分（60 + 40 - 0）
        /// - 100个Pin，90个健康，10个失败：78分（60 + 36 - 20）
        /// - 100个Pin，50个健康，50个失败：0分（60 + 20 - 100，取0）
        ///
        /// ### 参数
        /// - `operator`: 运营者账户
        ///
        /// ### 返回
        /// - u8: 健康度得分（0-100）
        pub fn calculate_health_score(operator: &T::AccountId) -> u8 {
            let stats = OperatorPinStats::<T>::get(operator);

            if stats.total_pins == 0 {
                return 100; // 无Pin时默认满分
            }

            // 失败率惩罚：每1%失败率扣2分
            let failure_rate = stats.failed_pins.saturating_mul(100) / stats.total_pins;
            let failure_penalty = failure_rate.saturating_mul(2).min(60); // 最多扣60分

            // 健康Pin比例奖励：健康Pin占比越高，得分越高
            let health_ratio = stats.healthy_pins.saturating_mul(100) / stats.total_pins;
            let health_bonus = health_ratio.min(40); // 最多加40分

            // 基础分60 + 健康奖励 - 失败惩罚
            60u8.saturating_add(health_bonus as u8)
                .saturating_sub(failure_penalty as u8)
                .max(0)
                .min(100)
        }

        /// 函数级详细中文注释：检查运营者容量并发出告警
        ///
        /// ### 功能
        /// - 计算运营者容量使用率（0-100）
        /// - 估算每个Pin平均2MB
        ///
        /// ### 参数
        /// - `operator`: 运营者账户
        ///
        /// ### 返回
        /// - `u8`: 容量使用率百分比（0-100），如果运营者不存在或容量为0，返回100
        pub fn calculate_capacity_usage(operator: &T::AccountId) -> u8 {
            let Some(info) = Operators::<T>::get(operator) else {
                return 100; // 运营者不存在，视为满载
            };

            if info.capacity_gib == 0 {
                return 100; // 容量为0，视为满载
            }

            // ✅ P1-7：使用实际存储字节数替代硬编码估算
            let used_bytes = OperatorUsedBytes::<T>::get(operator);
            let used_capacity_gib = used_bytes / (1024 * 1024 * 1024); // bytes → GiB
            let total_capacity_gib = info.capacity_gib as u64;

            // L1修复：clamp 到 100 防止 used > total 时 as u8 截断溢出
            ((used_capacity_gib * 100) / total_capacity_gib).min(100) as u8
        }

        /// 函数级详细中文注释：检查运营者容量告警并自动发出事件
        ///
        /// ### 功能
        /// - 估算运营者当前使用的存储容量
        /// - 计算容量使用率
        /// - 使用率超过80%时自动发送OperatorCapacityWarning事件
        ///
        /// ### 算法
        /// - 假设每个Pin平均大小为2MB
        /// - used_capacity_gib = (current_pins * 2MB) / 1024
        /// - usage_percent = (used_capacity_gib / total_capacity_gib) * 100
        ///
        /// ### 参数
        /// - `operator`: 运营者账户
        ///
        /// ### 返回
        /// - `true`: 已发出容量告警
        /// - `false`: 容量正常
        ///
        /// ### 触发时机
        /// - Pin分配后
        /// - 健康检查时（on_finalize / OCW）
        pub fn check_operator_capacity_warning(operator: &T::AccountId) -> bool {
            let Some(info) = Operators::<T>::get(operator) else {
                return false;
            };

            let total_capacity_gib = info.capacity_gib as u64;
            if total_capacity_gib == 0 {
                return false;
            }

            // ✅ P1-7：使用实际存储字节数
            let used_bytes = OperatorUsedBytes::<T>::get(operator);
            let used_capacity_gib = used_bytes / (1024 * 1024 * 1024);

            let usage_percent = ((used_capacity_gib * 100) / total_capacity_gib) as u8;

            // 如果使用率超过80%，发出告警
            if usage_percent >= 80 {
                Self::deposit_event(Event::OperatorCapacityWarning {
                    operator: operator.clone(),
                    used_capacity_gib: used_capacity_gib as u32,
                    total_capacity_gib: total_capacity_gib as u32,
                    usage_percent,
                });
                return true;
            }

            false
        }

        /// 函数级详细中文注释：获取运营者综合指标（供RPC调用）
        ///
        /// ### 功能
        /// - 聚合运营者的多维度数据（基础信息、Pin统计、容量使用、收益）
        /// - 返回完整的OperatorMetrics结构体
        ///
        /// ### 返回数据
        /// - status: 运营者状态（0=Active, 1=Suspended）
        /// - capacity_gib: 声明的存储容量
        /// - registered_at: 注册时间
        /// - total_pins: 当前管理的Pin总数
        /// - healthy_pins: 健康Pin数
        /// - failed_pins: 累计失败Pin数
        /// - health_score: 健康度得分（0-100）
        /// - used_capacity_gib: 已使用容量（估算）
        /// - capacity_usage_percent: 容量使用率（0-100）
        /// - pending_rewards: 待领取收益
        ///
        /// ### 使用场景
        /// - RPC方法 `memoIpfs_getOperatorMetrics`
        /// - 前端运营者Dashboard
        /// - 运营者排行榜
        ///
        /// ### 参数
        /// - `operator`: 运营者账户
        ///
        /// ### 返回
        /// - `Some(OperatorMetrics)`: 运营者存在
        /// - `None`: 运营者不存在
        pub fn get_operator_metrics(
            operator: &T::AccountId,
        ) -> Option<OperatorMetrics<BalanceOf<T>, BlockNumberFor<T>>> {
            let info = Operators::<T>::get(operator)?;
            let stats = OperatorPinStats::<T>::get(operator);
            let pending_rewards = OperatorRewards::<T>::get(operator);

            let _current_pins = Self::count_operator_pins(operator);
            // L2修复：使用 OperatorUsedBytes 实际数据，与 calculate_capacity_usage 一致
            let used_bytes = OperatorUsedBytes::<T>::get(operator);
            let used_capacity_gib = (used_bytes / (1024 * 1024 * 1024)) as u32;
            let capacity_usage_percent = if info.capacity_gib > 0 {
                (((used_capacity_gib as u64) * 100) / (info.capacity_gib as u64)).min(100) as u8
            } else {
                0
            };

            Some(OperatorMetrics {
                // 基础信息
                status: info.status,
                capacity_gib: info.capacity_gib,
                registered_at: info.registered_at,

                // Pin统计
                total_pins: stats.total_pins,
                healthy_pins: stats.healthy_pins,
                failed_pins: stats.failed_pins,
                health_score: stats.health_score,

                // 容量使用
                used_capacity_gib: used_capacity_gib as u32,
                capacity_usage_percent,

                // 收益
                pending_rewards,
            })
        }

        // ⭐ P1优化：已删除 select_operators_for_pin() 函数（98行）
        // 原因：所有引用已迁移到 select_operators_by_layer()
        // 迁移完成位置：
        // - request_pin_for_subject() extrinsic (已使用select_operators_by_layer)
        // 删除日期：2025-10-26
        //
        // 新函数优势：
        // - 支持分层选择（Layer 1 Core + Layer 2 Community）
        // - 更智能的评分算法（健康度+容量+优先级）
        // - 详细的审计追溯（LayeredPinAssignments）

        /// 函数级详细中文注释：根据分层策略智能选择运营者（Layer 1/Layer 2）
        ///
        /// ### 功能
        /// 根据数据类型和Pin优先级，从Layer 1（核心）和Layer 2（社区）运营者池中智能选择，实现：
        /// 1. 核心数据优先分配到Layer 1（项目方运营者）
        /// 2. 社区运营者增强冗余
        /// 3. 自动降级机制（运营者不足时）
        /// 4. 健康度和容量优先排序
        ///
        /// ### 参数
        /// - `subject_type`: 数据类型（Subject/General/Evidence等）
        /// - `tier`: Pin优先级（Critical/Standard/Temporary）
        ///
        /// ### 返回
        /// - `Ok(LayeredOperatorSelection)`: 分层运营者列表（Layer 1 + Layer 2）
        /// - `Err(Error::InsufficientOperators)`: 总副本数不足最低要求
        ///
        /// ### 分层策略
        /// 1. 从 `StorageLayerConfigs` 获取该数据类型的配置
        /// 2. 筛选Layer 1运营者：Active + 容量<80% + 非待注销
        /// 3. 按健康度和优先级排序Layer 1运营者
        /// 4. 选择Top N个Layer 1运营者
        /// 5. 筛选Layer 2运营者（同样条件）
        /// 6. 选择Top M个Layer 2运营者
        /// 7. 检查总副本数是否满足最低要求
        /// 8. 发出告警事件（如果某层不足）
        ///
        /// ### 降级策略
        /// - Layer 1不足时，发出 `CoreOperatorShortage` 事件（高优先级告警）
        /// - Layer 2不足时，发出 `CommunityOperatorShortage` 事件（中优先级告警）
        /// - 总副本数不足时，返回错误拒绝Pin请求
        ///
        /// ### 使用场景
        /// - `request_pin_for_subject` 初始Pin分配
        /// - OCW自动修复时重新分配
        pub fn select_operators_by_layer(
            subject_type: SubjectType,
            tier: PinTier,
        ) -> Result<LayeredOperatorSelection<T::AccountId>, Error<T>> {
            // 1. 获取分层策略配置
            let config = StorageLayerConfigs::<T>::get((subject_type.clone(), tier.clone()));

            // 2. 收集所有Layer 1（核心）运营者候选 ✅ P0-16：使用有界索引
            let mut core_candidates: Vec<(T::AccountId, u8, u8, u8)> = Vec::new();
            // (account, health_score, capacity_usage, priority)

            let active_ops = ActiveOperatorIndex::<T>::get();
            for operator in active_ops.iter() {
                let info = match Operators::<T>::get(operator) {
                    Some(i) => i,
                    None => continue,
                };

                // 筛选条件1：必须是Core层
                if info.layer != OperatorLayer::Core {
                    continue;
                }

                // 筛选条件2：不在待注销列表
                if PendingUnregistrations::<T>::contains_key(operator) {
                    continue;
                }

                // 计算容量使用率
                let capacity_usage_percent = Self::calculate_capacity_usage(operator);

                // 筛选条件3：容量使用率 < 80%
                if capacity_usage_percent >= 80 {
                    continue;
                }

                // 获取健康度得分
                let health_score = Self::calculate_health_score(operator);

                core_candidates.push((
                    operator.clone(),
                    health_score,
                    capacity_usage_percent,
                    info.priority,
                ));
            }

            // 3. 排序Layer 1运营者：健康度优先、优先级次要、容量第三
            core_candidates.sort_by(|a, b| {
                // 第一排序：健康度降序（高优先）
                match b.1.cmp(&a.1) {
                    core::cmp::Ordering::Equal => {
                        // 第二排序：优先级升序（低值优先）
                        match a.3.cmp(&b.3) {
                            core::cmp::Ordering::Equal => {
                                // 第三排序：容量使用率升序（低优先）
                                a.2.cmp(&b.2)
                            }
                            other => other,
                        }
                    }
                    other => other,
                }
            });

            // 4. 选择Top N个Layer 1运营者
            let selected_core: Vec<T::AccountId> = core_candidates
                .into_iter()
                .take(config.core_replicas as usize)
                .map(|(account, _, _, _)| account)
                .collect();

            // 5. 如果Layer 1运营者不足，发出告警
            if (selected_core.len() as u32) < config.core_replicas {
                Self::deposit_event(Event::CoreOperatorShortage {
                    required: config.core_replicas,
                    available: selected_core.len() as u32,
                });
            }

            // 6. 收集所有Layer 2（社区）运营者候选 ✅ P0-16：复用有界索引
            let mut community_candidates: Vec<(T::AccountId, u8, u8)> = Vec::new();
            // (account, health_score, capacity_usage)

            for operator in active_ops.iter() {
                let info = match Operators::<T>::get(operator) {
                    Some(i) => i,
                    None => continue,
                };

                // 筛选条件1：必须是Community层
                if info.layer != OperatorLayer::Community {
                    continue;
                }

                // 筛选条件2：不在待注销列表
                if PendingUnregistrations::<T>::contains_key(operator) {
                    continue;
                }

                // 计算容量使用率
                let capacity_usage_percent = Self::calculate_capacity_usage(operator);

                // 筛选条件3：容量使用率 < 80%
                if capacity_usage_percent >= 80 {
                    continue;
                }

                // 获取健康度得分
                let health_score = Self::calculate_health_score(operator);

                community_candidates.push((operator.clone(), health_score, capacity_usage_percent));
            }

            // 7. 排序Layer 2运营者：健康度优先、容量次要
            community_candidates.sort_by(|a, b| {
                // 第一排序：健康度降序（高优先）
                match b.1.cmp(&a.1) {
                    core::cmp::Ordering::Equal => {
                        // 第二排序：容量使用率升序（低优先）
                        a.2.cmp(&b.2)
                    }
                    other => other,
                }
            });

            // 8. 选择Top M个Layer 2运营者
            let selected_community: Vec<T::AccountId> = community_candidates
                .into_iter()
                .take(config.community_replicas as usize)
                .map(|(account, _, _)| account)
                .collect();

            // 9. 如果Layer 2运营者不足，发出告警（但不影响系统运行）
            if (selected_community.len() as u32) < config.community_replicas {
                Self::deposit_event(Event::CommunityOperatorShortage {
                    required: config.community_replicas,
                    available: selected_community.len() as u32,
                });
            }

            // 10. 检查总副本数是否满足最低要求
            let total_selected = selected_core.len() + selected_community.len();
            ensure!(
                total_selected >= config.min_total_replicas as usize,
                Error::<T>::NotEnoughOperators
            );

            // 11. 转换为BoundedVec并返回
            Ok(LayeredOperatorSelection {
                core_operators: BoundedVec::try_from(selected_core)
                    .map_err(|_| Error::<T>::BadParams)?,
                community_operators: BoundedVec::try_from(selected_community)
                    .map_err(|_| Error::<T>::BadParams)?,
            })
        }

        /// 函数级详细中文注释：派生SubjectFunding账户（v2版本，支持多种类型）
        ///
        /// 根据SubjectType和subject_id派生唯一的资金账户地址：
        /// - Evidence：domain=0（证据类数据）
        /// - Product：domain=10（商品元数据）
        /// - Entity：domain=11（实体元数据）
        /// - Shop：domain=12（店铺元数据）
        /// - General：domain=98（通用存储）
        /// - Custom：domain=99（自定义域）
        pub fn derive_subject_funding_account_v2(
            subject_type: SubjectType,
            subject_id: u64,
        ) -> T::AccountId {
            let domain: u8 = match subject_type {
                SubjectType::Evidence => 0,
                SubjectType::Product => 10,
                SubjectType::Entity => 11,
                SubjectType::Shop => 12,
                SubjectType::General => 98,
                SubjectType::Custom(_) => 99,
            };

            Self::subject_account_for(domain, subject_id)
        }

        /// 函数级详细中文注释：统一四层扣费机制（配额优先）
        ///
        /// 扣费顺序（所有类型统一）：
        /// 1. 配额优先（从 IpfsPool 扣费，计入月度配额）
        /// 2. SubjectFunding（用户充值账户）
        /// 3. IpfsPool 兜底（公共池补贴）
        /// 4. GracePeriod（宽限期）
        ///
        /// 返回：
        /// - Ok(ChargeResult::Success)：扣费成功，记录使用的层级
        /// - Ok(ChargeResult::EnterGrace)：进入宽限期
        /// - Err(Error::GraceExpired)：宽限期已过
        pub fn four_layer_charge(
            cid_hash: &T::Hash,
            task: &mut BillingTask<BlockNumberFor<T>, BalanceOf<T>>,
        ) -> Result<ChargeResult<BlockNumberFor<T>>, Error<T>> {
            let amount = task.amount_per_period;
            let current_block = <frame_system::Pallet<T>>::block_number();
            let pool_account = T::IpfsPoolAccount::get();

            let subjects = CidToSubject::<T>::get(cid_hash).ok_or(Error::<T>::SubjectNotFound)?;

            let subject_id = subjects.first().map(|s| s.subject_id).unwrap_or(0);

            // 预读 owner（用于用户级配额和 UserFunding）
            let owner_info = PinSubjectOf::<T>::get(cid_hash);

            // ===== 第1层：配额优先（用户级配额，防止多Subject滥用）=====
            if let Some((ref owner, _)) = owner_info {
                if Self::check_and_use_quota(owner, amount, current_block) {
                    let pool_balance = T::Currency::free_balance(&pool_account);

                    if pool_balance >= amount {
                        if let Err(_) = Self::distribute_to_pin_operators(cid_hash, amount) {
                            Self::deposit_event(Event::DistributionFailed {
                                cid_hash: *cid_hash,
                                amount,
                            });
                        }
                        TotalChargedFromPool::<T>::mutate(|total| {
                            *total = total.saturating_add(amount)
                        });

                        Self::deposit_event(Event::ChargedFromIpfsPool {
                            subject_id,
                            amount,
                            remaining_quota: Self::get_remaining_quota(owner, current_block),
                        });

                        return Ok(ChargeResult::Success {
                            layer: ChargeLayer::IpfsPool,
                        });
                    }
                    Self::rollback_quota_usage(owner, amount);
                }
            }

            // ===== 第2层：Entity 国库（CID 有 Entity 归属时尝试）=====
            if let Some(eid) = CidEntityOf::<T>::get(cid_hash) {
                match T::EntityFunding::try_charge_entity(eid, amount, &pool_account) {
                    Ok(true) => {
                        if let Err(_) = Self::distribute_to_pin_operators(cid_hash, amount) {
                            Self::deposit_event(Event::DistributionFailed {
                                cid_hash: *cid_hash,
                                amount,
                            });
                        }
                        Self::deposit_event(Event::ChargedFromEntityTreasury {
                            entity_id: eid,
                            amount,
                        });
                        return Ok(ChargeResult::Success {
                            layer: ChargeLayer::SubjectFunding,
                        });
                    }
                    Ok(false) => { /* balance insufficient, fallthrough */ }
                    Err(_) => { /* transfer error, fallthrough */ }
                }
            }

            // ===== 第3层：UserFunding（用户级充值账户）=====
            if let Some((ref owner, _)) = owner_info {
                let user_funding_account = Self::derive_user_funding_account(owner);
                let funding_balance = T::Currency::free_balance(&user_funding_account);

                if funding_balance >= amount {
                    T::Currency::transfer(
                        &user_funding_account,
                        &pool_account,
                        amount,
                        ExistenceRequirement::KeepAlive,
                    )
                    .map_err(|_| Error::<T>::SubjectFundingInsufficientBalance)?;

                    if let Err(_) = Self::distribute_to_pin_operators(cid_hash, amount) {
                        Self::deposit_event(Event::DistributionFailed {
                            cid_hash: *cid_hash,
                            amount,
                        });
                    }
                    TotalChargedFromSubject::<T>::mutate(|total| {
                        *total = total.saturating_add(amount)
                    });

                    for subject_info in subjects.iter() {
                        let domain = Self::subject_type_to_domain(&subject_info.subject_type);
                        SubjectUsage::<T>::mutate(
                            (owner.clone(), domain, subject_info.subject_id),
                            |usage| *usage = usage.saturating_add(amount),
                        );
                    }

                    Self::deposit_event(Event::ChargedFromSubjectFunding { subject_id, amount });

                    return Ok(ChargeResult::Success {
                        layer: ChargeLayer::SubjectFunding,
                    });
                }
            }

            // ===== 第4层：IpfsPool 兜底（公共池补贴）=====
            let pool_balance = T::Currency::free_balance(&pool_account);
            if pool_balance >= amount {
                if let Err(_) = Self::distribute_to_pin_operators(cid_hash, amount) {
                    Self::deposit_event(Event::DistributionFailed {
                        cid_hash: *cid_hash,
                        amount,
                    });
                }
                TotalChargedFromPool::<T>::mutate(|total| *total = total.saturating_add(amount));

                Self::deposit_event(Event::IpfsPoolLowBalanceWarning {
                    current: T::Currency::free_balance(&pool_account),
                });

                return Ok(ChargeResult::Success {
                    layer: ChargeLayer::IpfsPool,
                });
            }

            // ===== 第5层：GracePeriod（宽限期）=====
            match &task.grace_status {
                GraceStatus::Normal => {
                    let tier = CidTier::<T>::get(cid_hash);
                    let tier_config = Self::get_tier_config(&tier).unwrap_or_default();
                    let expires_at =
                        current_block.saturating_add(tier_config.grace_period_blocks.into());
                    Ok(ChargeResult::EnterGrace { expires_at })
                }
                GraceStatus::InGrace { expires_at, .. } => {
                    if current_block > *expires_at {
                        Err(Error::<T>::GraceExpired)
                    } else {
                        Ok(ChargeResult::EnterGrace {
                            expires_at: *expires_at,
                        })
                    }
                }
                GraceStatus::Expired => Err(Error::<T>::GraceExpired),
            }
        }

        /// 一次性分层扣费（用于 renew_pin / upgrade_pin_tier 等一次性收费场景）。
        /// 依次尝试：免费配额 → Entity 国库 → UserFunding → 调用者钱包 → 报错。
        fn charge_user_layered(
            caller: &T::AccountId,
            cid_hash: &T::Hash,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            if amount.is_zero() {
                return Ok(());
            }

            let pool_account = T::IpfsPoolAccount::get();
            let current_block = <frame_system::Pallet<T>>::block_number();

            // Layer 1: free quota
            if Self::check_and_use_quota(caller, amount, current_block) {
                Self::deposit_event(Event::ChargedFromIpfsPool {
                    subject_id: 0,
                    amount,
                    remaining_quota: Self::get_remaining_quota(caller, current_block),
                });
                return Ok(());
            }

            // Layer 2: Entity treasury
            if let Some(eid) = CidEntityOf::<T>::get(cid_hash) {
                if let Ok(true) = T::EntityFunding::try_charge_entity(eid, amount, &pool_account) {
                    Self::deposit_event(Event::ChargedFromEntityTreasury {
                        entity_id: eid,
                        amount,
                    });
                    return Ok(());
                }
            }

            // Layer 3: UserFunding account
            let user_funding = Self::derive_user_funding_account(caller);
            let funding_balance = T::Currency::free_balance(&user_funding);
            if funding_balance >= amount {
                T::Currency::transfer(
                    &user_funding,
                    &pool_account,
                    amount,
                    ExistenceRequirement::KeepAlive,
                )?;
                Self::deposit_event(Event::ChargedFromSubjectFunding {
                    subject_id: 0,
                    amount,
                });
                return Ok(());
            }

            // Layer 4: direct from caller's wallet
            T::Currency::transfer(
                caller,
                &pool_account,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;

            Ok(())
        }

        /// 检查并使用免费配额
        fn check_and_use_quota(
            owner: &T::AccountId,
            amount: BalanceOf<T>,
            current_block: BlockNumberFor<T>,
        ) -> bool {
            let (used, reset_block) = PublicFeeQuotaUsage::<T>::get(owner);
            let quota_limit = T::MonthlyPublicFeeQuota::get();

            let (current_used, new_reset_block) = if current_block >= reset_block {
                let new_reset = current_block.saturating_add(T::QuotaResetPeriod::get());
                (BalanceOf::<T>::zero(), new_reset)
            } else {
                (used, reset_block)
            };

            let remaining = quota_limit.saturating_sub(current_used);
            if remaining >= amount {
                let new_used = current_used.saturating_add(amount);
                PublicFeeQuotaUsage::<T>::insert(owner, (new_used, new_reset_block));
                true
            } else {
                false
            }
        }

        fn get_remaining_quota(
            owner: &T::AccountId,
            current_block: BlockNumberFor<T>,
        ) -> BalanceOf<T> {
            let (used, reset_block) = PublicFeeQuotaUsage::<T>::get(owner);
            let quota_limit = T::MonthlyPublicFeeQuota::get();

            if current_block >= reset_block {
                quota_limit
            } else {
                quota_limit.saturating_sub(used)
            }
        }

        fn rollback_quota_usage(owner: &T::AccountId, amount: BalanceOf<T>) {
            PublicFeeQuotaUsage::<T>::mutate(owner, |(used, _)| {
                *used = used.saturating_sub(amount);
            });
        }

        /// 函数级详细中文注释：自动分配存储费给运营者
        ///
        /// 分配逻辑：
        /// 1. 查询哪些运营者存储了该CID（从PinAssignments读取）
        /// 2. 平均分配费用给所有运营者
        /// 3. 累计到运营者奖励账户（OperatorRewards）
        ///
        /// 参数：
        /// - cid_hash: CID哈希
        /// - total_amount: 总费用
        ///
        /// 防作弊：
        /// - 运营者必须在PinAssignments中有该CID的记录
        /// - 定期健康巡检验证运营者确实存储了数据
        /// - 虚假报告会被申诉系统惩罚
        pub fn distribute_to_pin_operators(
            cid_hash: &T::Hash,
            total_amount: BalanceOf<T>,
        ) -> DispatchResult {
            let operators =
                PinAssignments::<T>::get(cid_hash).ok_or(Error::<T>::NoOperatorsAssigned)?;

            if operators.is_empty() {
                return Err(Error::<T>::NoOperatorsAssigned.into());
            }

            let pool_account = T::IpfsPoolAccount::get();

            // 按健康度加权分配（最低权重10，避免零分配）
            let mut weights: Vec<(T::AccountId, u128)> = Vec::new();
            let mut total_weight: u128 = 0;
            for operator in operators.iter() {
                let score = Self::calculate_health_score(operator) as u128;
                let weight = core::cmp::max(score, 10);
                weights.push((operator.clone(), weight));
                total_weight = total_weight.saturating_add(weight);
            }

            if total_weight == 0 {
                return Err(Error::<T>::NoOperatorsAssigned.into());
            }

            // Cap distribution to available (reservable) balance to prevent silent failure
            let pool_free = T::Currency::free_balance(&pool_account);
            let effective_amount = core::cmp::min(total_amount, pool_free);
            if effective_amount.is_zero() {
                return Ok(());
            }

            let mut total_distributed = BalanceOf::<T>::zero();
            let last_idx = weights.len().saturating_sub(1);

            for (i, (operator, weight)) in weights.iter().enumerate() {
                let share = if i == last_idx {
                    effective_amount.saturating_sub(total_distributed)
                } else {
                    let amt_u128: u128 = effective_amount.saturated_into();
                    let s = amt_u128.saturating_mul(*weight) / total_weight;
                    s.saturated_into()
                };

                OperatorRewards::<T>::mutate(&operator, |balance| {
                    *balance = balance.saturating_add(share);
                });
                total_distributed = total_distributed.saturating_add(share);

                Self::deposit_event(Event::OperatorRewarded {
                    operator: operator.clone(),
                    amount: share,
                    weight: *weight,
                    total_weight,
                });
            }

            // Lock pool funds — propagate error if reserve fails after capping
            T::Currency::reserve(&pool_account, total_distributed)
                .map_err(|_| Error::<T>::InsufficientEscrowBalance)?;

            Self::deposit_event(Event::RewardDistributed {
                total_amount: total_distributed,
                operator_count: operators.len() as u32,
                average_weight: total_weight / operators.len() as u128,
            });

            Ok(())
        }

        /// 函数级详细中文注释：获取Pin的运营者列表
        ///
        /// 从PinAssignments存储中读取分配给该CID的运营者列表
        pub fn get_pin_operators(
            cid_hash: &T::Hash,
        ) -> Result<BoundedVec<T::AccountId, ConstU32<16>>, Error<T>> {
            Self::get_cid_operators(cid_hash).ok_or(Error::<T>::NoOperatorsAssigned)
        }

        /// ✅ P2统一入口：优先从 LayeredPinAssignments 读取并展平，回退到 PinAssignments。
        /// 所有需要获取 CID 运营者列表的地方都应使用此函数。
        pub(crate) fn get_cid_operators(
            cid_hash: &T::Hash,
        ) -> Option<BoundedVec<T::AccountId, ConstU32<16>>> {
            if let Some(layered) = LayeredPinAssignments::<T>::get(cid_hash) {
                let mut all: alloc::vec::Vec<T::AccountId> = layered.core_operators.to_vec();
                all.extend(layered.community_operators.to_vec());
                BoundedVec::try_from(all).ok()
            } else {
                PinAssignments::<T>::get(cid_hash)
            }
        }

        /// 函数级详细中文注释：健康巡检（检查Pin状态）
        ///
        /// 功能：
        /// 1. 调用IPFS Cluster status API查询副本状态
        /// 2. 比对目标副本数与当前副本数
        /// 3. 返回健康状态（Healthy/Degraded/Critical/Unknown）
        ///
        /// 参数：
        /// - cid_hash: CID哈希
        ///
        /// 返回：
        /// - HealthStatus枚举值
        ///
        /// 注意：此函数应在OCW中调用，不应在链上执行
        pub fn check_pin_health(cid_hash: &T::Hash) -> HealthStatus {
            // P0-6: 基于链上 PinAssignments + PinSuccess 判断健康状态
            let assignments = match PinAssignments::<T>::get(cid_hash) {
                Some(ops) => ops,
                None => return HealthStatus::Unknown, // 无分配记录
            };

            // 统计在线副本数
            let mut ok_count: u32 = 0;
            for op in assignments.iter() {
                if PinSuccess::<T>::get(cid_hash, op) {
                    ok_count = ok_count.saturating_add(1);
                }
            }

            // 获取目标副本数
            let target = PinMeta::<T>::get(cid_hash)
                .map(|m| m.replicas)
                .unwrap_or(assignments.len() as u32);

            if ok_count >= target {
                HealthStatus::Healthy {
                    current_replicas: ok_count,
                }
            } else if ok_count >= 2 {
                HealthStatus::Degraded {
                    current_replicas: ok_count,
                    target,
                }
            } else {
                HealthStatus::Critical {
                    current_replicas: ok_count,
                }
            }
        }

        /// 函数级详细中文注释：计算初始Pin费用（一次性预扣30天费用）
        ///
        /// 参数：
        /// - size_bytes: CID大小（字节）
        /// - replicas: 副本数
        ///
        /// 返回：
        /// - Ok(Balance): 计算出的初始费用
        /// - Err: 计算失败
        ///
        /// 计算公式：
        /// 费用 = (size_bytes / 1GiB) × replicas × base_rate × 4周
        pub fn calculate_initial_pin_fee(
            size_bytes: u64,
            replicas: u32,
        ) -> Result<BalanceOf<T>, Error<T>> {
            let mib: u128 = 1_048_576u128; // 1 MiB
            let size_u128 = size_bytes as u128;
            let units_mib = (size_u128 + mib - 1) / mib; // ceil to MiB

            let base_rate = PricePerGiBWeek::<T>::get();
            let weeks_count = 4u128; // 30天 ≈ 4周

            // PricePerGiBWeek / 1024 = PricePerMiBWeek（先乘后除保留精度）
            let total = units_mib
                .saturating_mul(replicas as u128)
                .saturating_mul(base_rate)
                .saturating_mul(weeks_count)
                / 1024u128;

            Ok(total.saturated_into())
        }

        /// 函数级详细中文注释：计算周期费用（每个billing_period的费用）
        ///
        /// 参数：
        /// - size_bytes: CID大小（字节）
        /// - replicas: 副本数
        ///
        /// 返回：
        /// - Ok(Balance): 计算出的周期费用
        /// - Err: 计算失败
        ///
        /// 计算公式：
        /// 费用 = (size_bytes / 1GiB) × replicas × base_rate × 1周
        pub fn calculate_period_fee(
            size_bytes: u64,
            replicas: u32,
        ) -> Result<BalanceOf<T>, Error<T>> {
            let mib: u128 = 1_048_576u128; // 1 MiB
            let size_u128 = size_bytes as u128;
            let units_mib = (size_u128 + mib - 1) / mib; // ceil to MiB

            let base_rate = PricePerGiBWeek::<T>::get();

            // PricePerGiBWeek / 1024 = PricePerMiBWeek（先乘后除保留精度）
            let total = units_mib
                .saturating_mul(replicas as u128)
                .saturating_mul(base_rate)
                / 1024u128;

            Ok(total.saturated_into())
        }

        /// 函数级详细中文注释：获取治理账户（辅助函数）
        ///
        /// 返回一个固定的治理账户地址（用于日志记录）
        pub fn governance_account() -> T::AccountId {
            // 使用SubjectPalletId作为治理账户的默认值
            T::SubjectPalletId::get().into_account_truncating()
        }
    }

    // 说明：临时允许 warnings 以通过工作区 -D warnings；后续将以 WeightInfo 基准权重替换常量权重
    #[allow(warnings)]
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 函数级详细中文注释：为用户存储账户充值（混合方案，推荐）
        ///
        /// ### 混合方案
        /// - 每个用户一个派生账户，替代每个 Subject 一个账户
        /// - 大幅减少派生账户数量
        /// - 用户只需充值一个账户，管理简单
        ///
        /// ### 权限
        /// - **任何账户都可以充值**（开放性）
        /// - 无需owner权限
        ///
        /// ### 资金流向
        /// - caller → UserFunding(target_user)
        ///
        /// ### 使用场景
        /// - 用户自己充值（常规）
        /// - 家人朋友赞助（情感）
        /// - 社区众筹（公益）
        ///
        /// ### 参数
        /// - `target_user`: 目标用户账户（可以是自己或他人）
        /// - `amount`: 充值金额（必须>0）
        ///
        /// ### 事件
        /// - UserFunded(target_user, who, to, amount)
        #[pallet::call_index(21)]
        #[pallet::weight(T::WeightInfo::fund_user_account())]
        pub fn fund_user_account(
            origin: OriginFor<T>,
            target_user: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(amount != BalanceOf::<T>::default(), Error::<T>::BadParams);

            // ✅ 派生用户级存储账户
            let to = Self::derive_user_funding_account(&target_user);

            // ✅ 转账（任何人都可以充值）
            <T as Config>::Currency::transfer(
                &who,
                &to,
                amount,
                frame_support::traits::ExistenceRequirement::KeepAlive,
            )?;

            // ✅ 更新用户充值统计
            UserFundingBalance::<T>::mutate(&target_user, |balance| {
                *balance = balance.saturating_add(amount);
            });

            // ✅ 发送事件
            Self::deposit_event(Event::UserFunded {
                user: target_user,
                funder: who,
                funding_account: to,
                amount,
            });
            Ok(())
        }

        /// 用户从自己的存储资金账户提取资金。
        #[pallet::call_index(53)]
        #[pallet::weight(T::WeightInfo::withdraw_user_funding())]
        pub fn withdraw_user_funding(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::BadParams);

            let funding_account = Self::derive_user_funding_account(&who);
            let balance = T::Currency::free_balance(&funding_account);
            ensure!(balance >= amount, Error::<T>::InsufficientUserFunding);

            T::Currency::transfer(
                &funding_account,
                &who,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;

            UserFundingBalance::<T>::mutate(&who, |b| {
                *b = b.saturating_sub(amount);
            });

            Self::deposit_event(Event::UserFundingWithdrawn { user: who, amount });
            Ok(())
        }

        /// 降低 CID 的 Pin Tier（只允许向下降级：Critical→Standard→Temporary）。
        /// 降级后费率降低，差额不退还（避免套利）。
        #[pallet::call_index(54)]
        #[pallet::weight(T::WeightInfo::downgrade_pin_tier())]
        pub fn downgrade_pin_tier(
            origin: OriginFor<T>,
            cid: Vec<u8>,
            new_tier: PinTier,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            use sp_runtime::traits::Hash;
            let cid_hash = T::Hashing::hash(&cid[..]);

            ensure!(
                PinMeta::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );

            if let Some((owner, _)) = PinSubjectOf::<T>::get(&cid_hash) {
                ensure!(who == owner, Error::<T>::NotOwner);
            } else {
                return Err(Error::<T>::NotOwner.into());
            }

            let old_tier = CidTier::<T>::get(&cid_hash);
            let valid_downgrade = match (&old_tier, &new_tier) {
                (PinTier::Critical, PinTier::Standard) => true,
                (PinTier::Critical, PinTier::Temporary) => true,
                (PinTier::Standard, PinTier::Temporary) => true,
                _ => false,
            };
            ensure!(valid_downgrade, Error::<T>::InvalidTierDowngrade);

            let new_config = Self::get_tier_config(&new_tier)?;

            CidTier::<T>::insert(&cid_hash, new_tier.clone());

            PinMeta::<T>::mutate(&cid_hash, |meta| {
                if let Some(m) = meta {
                    m.replicas = new_config.replicas;
                }
            });

            if let Some(due_block) = CidBillingDueBlock::<T>::get(&cid_hash) {
                if let Some(mut task) = BillingQueue::<T>::take(due_block, &cid_hash) {
                    let size = PinMeta::<T>::get(&cid_hash).map(|m| m.size).unwrap_or(0);
                    if let Ok(new_fee) = Self::calculate_period_fee(size, new_config.replicas) {
                        let adjusted = new_fee.saturating_mul(new_config.fee_multiplier.into())
                            / 10000u32.into();
                        task.amount_per_period = adjusted;
                    }
                    BillingQueue::<T>::insert(due_block, &cid_hash, task);
                }
            }

            Self::deposit_event(Event::PinTierDowngraded {
                cid_hash,
                old_tier,
                new_tier,
            });
            Ok(())
        }

        /// 运营者对 slash 发起争议，记录链上供治理审查。
        #[pallet::call_index(55)]
        #[pallet::weight(T::WeightInfo::dispute_slash())]
        pub fn dispute_slash(
            origin: OriginFor<T>,
            amount: BalanceOf<T>,
            reason: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                Operators::<T>::contains_key(&who),
                Error::<T>::OperatorNotFound
            );
            let reason_bounded: BoundedVec<u8, ConstU32<256>> =
                reason.try_into().map_err(|_| Error::<T>::BadParams)?;

            Self::deposit_event(Event::SlashDisputed {
                operator: who,
                amount,
                reason: reason_bounded,
            });
            Ok(())
        }

        /// 为SubjectFunding账户充值（已弃用，保留向后兼容）
        ///
        /// ⚠️ 已弃用：请使用 fund_user_account() 替代
        ///
        /// ### 权限
        /// - **任何账户都可以充值**（开放性）
        /// - 无需owner权限
        /// - 只需要subject存在
        ///
        /// ### 资金流向
        /// - caller → SubjectFunding(subject_id)
        /// - SubjectFunding地址基于creator派生（稳定地址）
        ///
        /// ### 使用场景
        /// - owner自己充值（常规）
        /// - 家人朋友赞助（情感）
        /// - 社区众筹（公益）
        /// - 服务商预付费（商业）
        /// - 慈善捐赠（慈善）
        ///
        /// ### 设计理念
        /// - **开放性**：任何人都可以资助
        /// - **灵活性**：支持多种场景
        /// - **安全性**：资金只能用于IPFS pin
        /// - **简单性**：不需要权限检查
        ///
        /// ### 参数
        /// - `subject_id`: 主体ID
        /// - `amount`: 充值金额（必须>0）
        ///
        /// ### 事件
        /// - SubjectFunded(subject_id, who, to, amount)
        /// 已弃用：请使用 fund_user_account() 替代。
        /// 调用此接口将直接返回 BadParams 错误。
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::fund_subject_account())]
        #[deprecated(note = "请使用 fund_user_account() 替代")]
        pub fn fund_subject_account(
            origin: OriginFor<T>,
            _subject_id: u64,
            _amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure_signed(origin)?;
            Err(Error::<T>::BadParams.into())
        }
        // ⭐ P2优化：已删除 request_pin() extrinsic（46行）
        // 原因：已被破坏式改造的 request_pin_for_subject 替代
        // 删除日期：2025-10-26
        //
        // 旧版问题：
        // - 使用cid_hash而非明文CID（不利于IPFS集成）
        // - 直接扣费模式（无四层回退保障）
        // - 不支持分层运营者选择
        // - 不支持自动化计费和宽限期
        //
        // 新版优势（request_pin_for_subject）：
        // - 使用明文CID（Vec<u8>），便于IPFS API调用
        // - 三层扣费逻辑（IpfsPool → SubjectFunding → Grace）
        // - 分层运营者选择（Layer 1 Core + Layer 2 Community）
        // - 支持PinTier（Critical/Standard/Temporary）
        // - 自动化周期计费和宽限期管理

        /// 函数级详细中文注释：为主体发起 Pin（四层扣款逻辑）
        ///
        /// 授权：caller 必须为该 subject 的 owner
        ///
        /// 扣款优先级（四层扣款）：
        /// 1. IpfsPoolAccount（配额内优先，公共福利）
        /// 2. SubjectFunding（主体专属资金，推荐）
        /// 3. IpfsPool 兜底（公共池补贴）
        /// 4. GracePeriod（宽限期）
        ///
        /// 优点：
        /// - 用户体验最好：一次交易完成
        /// - 新用户友好：无需预充值
        /// - 向后兼容：保留双重扣款逻辑
        /// - 仍鼓励使用 SubjectFunding（第二优先级）
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::request_pin())]
        pub fn request_pin_for_subject(
            origin: OriginFor<T>,
            subject_id: u64,
            cid: Vec<u8>,
            size_bytes: u64,
            tier: Option<PinTier>,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            Self::do_request_pin(
                caller,
                SubjectType::General,
                subject_id,
                None,
                cid,
                size_bytes,
                tier,
            )
        }

        // [已删除] charge_due (call_index 11)：断链的旧版计费路径。
        // 计费由 on_finalize 自动处理 BillingQueue。

        /// 函数级详细中文注释：治理设置/暂停计费参数。
        /// - 任何入参为 None 表示保持不变；部分更新。
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::set_billing_params())]
        pub fn set_billing_params(
            origin: OriginFor<T>,
            price_per_gib_week: Option<u128>,
            period_blocks: Option<u32>,
            grace_blocks: Option<u32>,
            max_charge_per_block: Option<u32>,
            subject_min_reserve: Option<BalanceOf<T>>,
            paused: Option<bool>,
            // ⭐ P2优化：已删除 allow_direct_pin 参数（旧版 request_pin() 已删除）
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            // 参数防呆校验：确保关键参数为正，避免导致停摆或无限宽限
            if let Some(v) = price_per_gib_week {
                ensure!(v > 0, Error::<T>::BadParams);
                PricePerGiBWeek::<T>::put(v);
            }
            if let Some(v) = period_blocks {
                ensure!(v > 0, Error::<T>::BadParams);
                BillingPeriodBlocks::<T>::put(v);
            }
            if let Some(v) = grace_blocks {
                ensure!(v > 0, Error::<T>::BadParams);
                GraceBlocks::<T>::put(v);
            }
            if let Some(v) = max_charge_per_block {
                ensure!(v > 0, Error::<T>::BadParams);
                MaxChargePerBlock::<T>::put(v);
            }
            if let Some(v) = subject_min_reserve {
                SubjectMinReserve::<T>::put(v);
            }
            if let Some(v) = paused {
                BillingPaused::<T>::put(v);
            }
            // ⭐ P2优化：已删除 allow_direct_pin 参数处理（旧版 request_pin() 已删除）

            // L1-R2修复：发送配置变更事件（治理操作审计）
            Self::deposit_event(Event::BillingParamsUpdated);
            Ok(())
        }

        // [已删除] set_replicas_config (call_index 14)：副本数统一由 update_tier_config 管理。

        /// 函数级详细中文注释：将 OperatorEscrowAccount 中的资金按权重分配给活跃运营者
        ///
        /// 权重计算公式：
        /// - weight = pinned_bytes × reliability_factor
        /// - reliability_factor = probe_ok / (probe_ok + probe_fail)
        /// - 如果 probe_ok + probe_fail = 0，则使用默认值 50%
        ///
        /// 分配规则：
        /// - 仅分配给状态为 Active(0) 的运营者
        /// - 按权重比例分配：运营者收益 = 总金额 × (运营者权重 / 所有运营者权重之和)
        /// - 忽略权重为 0 的运营者（pinned_bytes = 0）
        ///
        /// 权限：
        /// - 治理 Origin（Root 或技术委员会）
        /// - 建议定期（如每周）执行一次
        ///
        /// 参数：
        /// - `max_amount`: 本次分配的最大金额（0 表示分配托管账户的全部余额）
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::distribute_to_operators())]
        pub fn distribute_to_operators(
            origin: OriginFor<T>,
            max_amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            let escrow_account = T::OperatorEscrowAccount::get();

            // 1. 确定要分配的总金额
            let escrow_balance = <T as Config>::Currency::free_balance(&escrow_account);
            let total_amount = if max_amount.is_zero() {
                escrow_balance
            } else {
                max_amount.min(escrow_balance)
            };

            ensure!(
                !total_amount.is_zero(),
                Error::<T>::InsufficientEscrowBalance
            );

            // 2. 收集所有活跃运营者的权重 ✅ P0-16：使用有界索引
            let mut weights: alloc::vec::Vec<(T::AccountId, u128)> = alloc::vec::Vec::new();
            let mut total_weight: u128 = 0;

            let active_ops = ActiveOperatorIndex::<T>::get();
            for op in active_ops.iter() {
                let sla = OperatorSla::<T>::get(op);
                // 计算可靠性因子（千分比，避免除零）
                let total_probes = sla.probe_ok.saturating_add(sla.probe_fail);
                let reliability = if total_probes > 0 {
                    // 计算 probe_ok / total_probes，结果乘以 1000 得到千分比
                    (sla.probe_ok as u128)
                        .saturating_mul(1000)
                        .saturating_div(total_probes as u128)
                } else {
                    // 默认 50% 可靠性
                    500
                };

                // 综合权重 = 存储量 × 可靠性 / 1000
                let weight = (sla.pinned_bytes as u128)
                    .saturating_mul(reliability)
                    .checked_div(1000)
                    .ok_or(Error::<T>::WeightOverflow)?;

                // 只记录权重大于 0 的运营者
                if weight > 0 {
                    weights.push((op.clone(), weight));
                    total_weight = total_weight
                        .checked_add(weight)
                        .ok_or(Error::<T>::WeightOverflow)?;
                }
            }

            ensure!(!weights.is_empty(), Error::<T>::NoActiveOperators);
            ensure!(total_weight > 0, Error::<T>::NoActiveOperators);

            // 3. 按权重比例分配
            let mut distributed_amount = BalanceOf::<T>::zero();
            let operator_count = weights.len() as u32;

            for (op, weight) in weights.iter() {
                // 计算该运营者应得的份额
                // share = total_amount × weight / total_weight
                let share_u128 = (total_amount.saturated_into::<u128>())
                    .saturating_mul(*weight)
                    .saturating_div(total_weight);

                let share: BalanceOf<T> = share_u128.saturated_into();

                if share > BalanceOf::<T>::zero() {
                    // H1-R3修复：skip failed transfers instead of aborting entire distribution
                    match <T as Config>::Currency::transfer(
                        &escrow_account,
                        op,
                        share,
                        ExistenceRequirement::KeepAlive,
                    ) {
                        Ok(_) => {
                            distributed_amount = distributed_amount.saturating_add(share);

                            Self::deposit_event(Event::OperatorRewarded {
                                operator: op.clone(),
                                amount: share,
                                weight: *weight,
                                total_weight,
                            });
                        }
                        Err(_) => {
                            // H1-R3：跳过失败的转账，不阻塞其他运营者
                        }
                    }
                }
            }

            // 4. 发出汇总事件
            let average_weight = if operator_count > 0 {
                total_weight / (operator_count as u128)
            } else {
                0
            };

            Self::deposit_event(Event::RewardDistributed {
                total_amount: distributed_amount,
                operator_count,
                average_weight,
            });

            Ok(())
        }

        /// 函数级详细中文注释：OCW 上报标记已 Pin 成功
        /// - 需要节点 keystore 的专用 key 签名；
        /// - 仅更新状态并发出事件（骨架）。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::mark_pinned())]
        pub fn mark_pinned(
            origin: OriginFor<T>,
            cid_hash: T::Hash,
            replicas: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            // 仅允许活跃运营者上报
            let op = Operators::<T>::get(&who).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(op.status == 0, Error::<T>::OperatorBanned);
            ensure!(
                PendingPins::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );
            // 必须是该 cid 的指派运营者之一
            if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                ensure!(
                    assign.iter().any(|a| a == &who),
                    Error::<T>::OperatorNotAssigned
                );
            } else {
                return Err(Error::<T>::AssignmentNotFound.into());
            }
            // 标记该运营者完成
            PinSuccess::<T>::insert(&cid_hash, &who, true);
            // 达到副本数则完成
            if let Some(meta) = PinMeta::<T>::get(&cid_hash) {
                let expect = meta.replicas;
                let mut ok_count: u32 = 0;
                if let Some(ops) = PinAssignments::<T>::get(&cid_hash) {
                    for o in ops.iter() {
                        if PinSuccess::<T>::get(&cid_hash, o) {
                            ok_count = ok_count.saturating_add(1);
                        }
                    }
                }
                if ok_count >= expect {
                    // 清理 pending，设置状态
                    PendingPins::<T>::remove(&cid_hash);
                    PinStateOf::<T>::insert(&cid_hash, 2u8); // Pinned
                    Self::deposit_event(Event::PinStateChanged(cid_hash, 2));
                } else {
                    PinStateOf::<T>::insert(&cid_hash, 1u8); // Pinning
                    Self::deposit_event(Event::PinStateChanged(cid_hash, 1));
                }
            }
            Self::deposit_event(Event::PinMarkedPinned(cid_hash, replicas));
            Ok(())
        }

        /// 函数级详细中文注释：OCW 上报标记 Pin 失败
        /// - 记录错误码，便于外部审计。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::mark_pin_failed())]
        pub fn mark_pin_failed(
            origin: OriginFor<T>,
            cid_hash: T::Hash,
            code: u16,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let op = Operators::<T>::get(&who).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(op.status == 0, Error::<T>::OperatorBanned);
            ensure!(
                PendingPins::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );
            if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                ensure!(
                    assign.iter().any(|a| a == &who),
                    Error::<T>::OperatorNotAssigned
                );
            } else {
                return Err(Error::<T>::AssignmentNotFound.into());
            }
            // 标记失败并置为 Pinning/Failed
            PinSuccess::<T>::insert(&cid_hash, &who, false);
            PinStateOf::<T>::insert(&cid_hash, 1u8);
            Self::deposit_event(Event::PinStateChanged(cid_hash, 1));
            Self::deposit_event(Event::PinMarkedFailed(cid_hash, code));
            Ok(())
        }

        /// 函数级详细中文注释：申请成为运营者并存入保证金
        /// - 要求容量 >= MinCapacityGiB，保证金 >= MinOperatorBond；
        /// - 保证金使用可保留余额（reserve），离开时解保留。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::join_operator())]
        pub fn join_operator(
            origin: OriginFor<T>,
            peer_id: BoundedVec<u8, T::MaxPeerIdLen>,
            capacity_gib: u32,
            endpoint_hash: T::Hash,
            cert_fingerprint: Option<T::Hash>,
            bond: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                !Operators::<T>::contains_key(&who),
                Error::<T>::OperatorExists
            );
            ensure!(
                capacity_gib >= T::MinCapacityGiB::get(),
                Error::<T>::InsufficientCapacity
            );
            // 计算最小保证金（100 USDT 等值的 NEX）
            let min_bond = Self::calculate_operator_bond();
            ensure!(bond >= min_bond, Error::<T>::InsufficientBond);
            // 保证金保留
            <T as Config>::Currency::reserve(&who, bond)?;
            OperatorBond::<T>::insert(&who, bond);

            // ✅ P1-1：获取当前区块高度作为注册时间
            let current_block = <frame_system::Pallet<T>>::block_number();

            let info = OperatorInfo::<T> {
                peer_id,
                capacity_gib,
                endpoint_hash,
                cert_fingerprint,
                status: 0,
                registered_at: current_block,    // ✅ P1-1：记录注册时间
                layer: OperatorLayer::Community, // ✅ Layer分层：新运营者默认分配到Layer 2（社区）
                priority: 128, // ✅ 默认优先级：中等（0-255，Community通常51-200）
            };
            Operators::<T>::insert(&who, info);

            // ✅ P0-16：维护活跃运营者索引
            ActiveOperatorIndex::<T>::try_mutate(|index| -> DispatchResult {
                index
                    .try_push(who.clone())
                    .map_err(|_| Error::<T>::ActiveOperatorIndexFull)?;
                Ok(())
            })?;

            Self::deposit_event(Event::OperatorJoined(who));
            Ok(())
        }

        /// 函数级详细中文注释：更新运营者元信息（不影响保证金）
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::update_operator())]
        pub fn update_operator(
            origin: OriginFor<T>,
            peer_id: Option<BoundedVec<u8, T::MaxPeerIdLen>>,
            capacity_gib: Option<u32>,
            endpoint_hash: Option<T::Hash>,
            cert_fingerprint: Option<Option<T::Hash>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Operators::<T>::try_mutate(&who, |maybe| -> DispatchResult {
                let op = maybe.as_mut().ok_or(Error::<T>::OperatorNotFound)?;
                if let Some(p) = peer_id {
                    op.peer_id = p;
                }
                if let Some(c) = capacity_gib {
                    ensure!(
                        c >= T::MinCapacityGiB::get(),
                        Error::<T>::InsufficientCapacity
                    );
                    let used_bytes = OperatorUsedBytes::<T>::get(&who);
                    let used_gib = (used_bytes / (1024 * 1024 * 1024)) as u32;
                    ensure!(c >= used_gib, Error::<T>::InsufficientCapacity);
                    op.capacity_gib = c;
                }
                if let Some(h) = endpoint_hash {
                    op.endpoint_hash = h;
                }
                if let Some(cf) = cert_fingerprint {
                    op.cert_fingerprint = cf;
                }
                Ok(())
            })?;
            Self::deposit_event(Event::OperatorUpdated(who));
            Ok(())
        }

        /// 函数级详细中文注释：退出运营者并解保留保证金（需无未完成订单，MVP 略过校验）
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::leave_operator())]
        /// 函数级详细中文注释：运营者注销（永久退出）✅ P0-3优化版
        ///
        /// ### 功能说明
        /// - 运营者自己调用，永久退出运营者身份
        /// - 如果有未完成的Pin，进入7天宽限期
        /// - 宽限期内OCW自动迁移Pin到其他运营者
        /// - 宽限期结束后，如无Pin则返还保证金并移除记录
        /// - 如果没有Pin，立即退出并返还保证金
        ///
        /// ### 宽限期机制（✅ P0-3新增）
        /// - 默认7天（100,800块）
        /// - 宽限期内运营者状态标记为Suspended（停止新Pin）
        /// - OCW负责迁移Pin到其他运营者
        /// - 宽限期到期由on_finalize检查并处理
        ///
        /// ### 流程
        /// 1. 检查是否是运营者
        /// 2. 统计当前Pin数量
        /// 3. 如有Pin → 进入宽限期（7天）
        /// 4. 如无Pin → 立即退出
        ///
        /// ### 权限
        /// - 签名账户必须是已注册的运营者
        pub fn leave_operator(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 检查是否是运营者
            ensure!(
                Operators::<T>::contains_key(&who),
                Error::<T>::OperatorNotFound
            );

            // ✅ P0-3：统计运营者的Pin数量
            let assigned_pins = Self::count_operator_pins(&who);

            if assigned_pins > 0 {
                // ✅ P0-3：进入宽限期（M2修复：使用Config常量替代硬编码）
                let current_block = <frame_system::Pallet<T>>::block_number();
                let expires_at = current_block.saturating_add(T::OperatorGracePeriod::get());

                // 记录到宽限期队列
                PendingUnregistrations::<T>::insert(&who, expires_at);

                // 立即停止新Pin分配（标记为Suspended）
                Operators::<T>::try_mutate(&who, |maybe| -> DispatchResult {
                    let op = maybe.as_mut().ok_or(Error::<T>::OperatorNotFound)?;
                    op.status = 1; // 1 = Suspended
                    Ok(())
                })?;

                // ✅ P0-16：从活跃索引移除
                ActiveOperatorIndex::<T>::mutate(|index| {
                    index.retain(|a| a != &who);
                });

                // 发送进入宽限期事件
                Self::deposit_event(Event::OperatorUnregistrationPending {
                    operator: who,
                    remaining_pins: assigned_pins,
                    expires_at,
                });
            } else {
                // ✅ 无Pin，立即退出
                Self::finalize_operator_unregistration(&who)?;
            }

            Ok(())
        }

        /// 函数级详细中文注释：治理设置运营者状态（0=Active,1=Suspended,2=Banned）
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::set_operator_status())]
        pub fn set_operator_status(
            origin: OriginFor<T>,
            who: T::AccountId,
            status: u8,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            // M3修复：验证 status 范围（0=Active, 1=Suspended, 2=Banned）
            ensure!(status <= 2, Error::<T>::BadParams);
            let old_status = Operators::<T>::get(&who)
                .ok_or(Error::<T>::OperatorNotFound)?
                .status;
            Operators::<T>::try_mutate(&who, |maybe| -> DispatchResult {
                let op = maybe.as_mut().ok_or(Error::<T>::OperatorNotFound)?;
                op.status = status;
                Ok(())
            })?;

            // ✅ P0-16：维护活跃运营者索引
            if old_status != 0 && status == 0 {
                // 重新激活 → 加入索引
                ActiveOperatorIndex::<T>::mutate(|index| {
                    if !index.contains(&who) {
                        let _ = index.try_push(who.clone());
                    }
                });
            } else if old_status == 0 && status != 0 {
                // 停用 → 移出索引
                ActiveOperatorIndex::<T>::mutate(|index| {
                    index.retain(|a| a != &who);
                });
            }

            Self::deposit_event(Event::OperatorStatusChanged(who, status));
            Ok(())
        }

        /// 函数级详细中文注释：治理更新分层存储策略配置
        ///
        /// ### 功能说明
        /// - 治理Root调用，动态调整不同数据类型的分层存储策略
        /// - 支持按（数据类型 × Pin层级）细粒度配置
        /// - 配置项包括：Layer 1副本数、Layer 2副本数、是否允许Layer 3、最低副本数
        ///
        /// ### 参数
        /// - `origin`: 必须是Root
        /// - `subject_type`: 数据类型（Subject/General/Evidence等）
        /// - `tier`: Pin优先级（Critical/Standard/Temporary）
        /// - `config`: 分层配置参数
        ///
        /// ### 使用场景
        /// - 调整证据数据的高安全策略（仅Layer 1，5副本）
        /// - 调整供奉品的低成本策略（允许Layer 3）
        /// - 应对运营者数量变化，动态调整副本分配
        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::set_storage_layer_config())]
        pub fn set_storage_layer_config(
            origin: OriginFor<T>,
            subject_type: SubjectType,
            tier: PinTier,
            config: StorageLayerConfig,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 验证配置合理性
            ensure!(config.min_total_replicas > 0, Error::<T>::BadParams);
            ensure!(
                config.core_replicas + config.community_replicas >= config.min_total_replicas,
                Error::<T>::BadParams
            );

            // 更新配置
            StorageLayerConfigs::<T>::insert((subject_type.clone(), tier.clone()), config.clone());

            // 发送事件
            Self::deposit_event(Event::StorageLayerConfigUpdated {
                subject_type,
                tier,
                core_replicas: config.core_replicas,
                community_replicas: config.community_replicas,
            });

            Ok(())
        }

        /// 函数级详细中文注释：治理设置运营者层级
        ///
        /// ### 功能说明
        /// - 治理Root调用，手动调整运营者的层级和优先级
        /// - 支持Layer 1/2之间的迁移
        /// - 用于项目方将自己的节点设置为Layer 1（核心）
        ///
        /// ### 参数
        /// - `origin`: 必须是Root
        /// - `operator`: 运营者账户
        /// - `layer`: 新的层级（Core/Community）
        /// - `priority`: 优先级（0-255，越小越优先）
        ///
        /// ### 使用场景
        /// - 项目方初始运营者设置为Layer 1（Core）
        /// - 优秀社区运营者升级到Layer 1
        /// - 降级不活跃的Layer 1运营者到Layer 2
        #[pallet::call_index(20)]
        #[pallet::weight(T::WeightInfo::set_operator_layer())]
        pub fn set_operator_layer(
            origin: OriginFor<T>,
            operator: T::AccountId,
            layer: OperatorLayer,
            priority: u8,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 检查运营者是否存在
            Operators::<T>::try_mutate(&operator, |info_opt| -> DispatchResult {
                let info = info_opt.as_mut().ok_or(Error::<T>::OperatorNotFound)?;

                // 更新层级和优先级
                info.layer = layer.clone();
                info.priority = priority;

                // 发送事件
                Self::deposit_event(Event::OperatorLayerUpdated {
                    operator: operator.clone(),
                    layer,
                    priority,
                });

                Ok(())
            })
        }

        /// 函数级详细中文注释：运营者自主暂停（临时停止接收新Pin）✅ P0-1实现
        ///
        /// ### 功能说明
        /// - 运营者自己调用，无需治理介入
        /// - 将status从0(Active)改为1(Suspended)
        /// - 停止分配新Pin，但已有Pin仍需维护
        /// - 保留运营者身份和保证金
        /// - 可随时调用resume_operator()恢复
        ///
        /// ### 适用场景
        /// - 短期维护（硬件升级、网络故障修复）
        /// - 临时离线（1-7天）
        /// - 容量不足需要扩容
        ///
        /// ### 权限
        /// - 签名账户必须是已注册的运营者
        /// - 当前状态必须是Active（status=0）
        #[pallet::call_index(22)]
        #[pallet::weight(T::WeightInfo::pause_operator())]
        pub fn pause_operator(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 检查是否是运营者
            let mut info = Operators::<T>::get(&who).ok_or(Error::<T>::OperatorNotFound)?;

            // 检查是否已暂停
            ensure!(info.status == 0, Error::<T>::AlreadyPaused);

            // 标记为暂停
            info.status = 1; // 1 = Suspended
            Operators::<T>::insert(&who, info);

            // H1-R2修复：从活跃索引移除（与 leave_operator / set_operator_status 一致）
            ActiveOperatorIndex::<T>::mutate(|index| {
                index.retain(|a| a != &who);
            });

            // 发送事件
            Self::deposit_event(Event::OperatorPaused { operator: who });

            Ok(())
        }

        /// 函数级详细中文注释：运营者自主恢复（从暂停状态恢复）✅ P0-2实现
        ///
        /// ### 功能说明
        /// - 运营者自己调用，无需治理介入
        /// - 将status从1(Suspended)改为0(Active)
        /// - 恢复接收新Pin分配
        /// - 保证金和运营者信息不变
        ///
        /// ### 适用场景
        /// - 维护完成后恢复服务
        /// - 硬件扩容完成
        /// - 网络问题修复
        ///
        /// ### 权限
        /// - 签名账户必须是已注册的运营者
        /// - 当前状态必须是Suspended（status=1）
        #[pallet::call_index(23)]
        #[pallet::weight(T::WeightInfo::resume_operator())]
        pub fn resume_operator(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 检查是否是运营者
            let mut info = Operators::<T>::get(&who).ok_or(Error::<T>::OperatorNotFound)?;

            // 检查是否已暂停
            ensure!(info.status == 1, Error::<T>::NotPaused);

            // 恢复激活
            info.status = 0; // 0 = Active
            Operators::<T>::insert(&who, info);

            // H1-R2修复：重新加入活跃索引（与 set_operator_status 一致）
            ActiveOperatorIndex::<T>::mutate(|index| {
                if !index.contains(&who) {
                    let _ = index.try_push(who.clone());
                }
            });

            // 发送事件
            Self::deposit_event(Event::OperatorResumed { operator: who });

            Ok(())
        }

        /// 函数级详细中文注释：运营者自证在线（由运行其节点的 OCW 定期上报）
        /// - 探测逻辑在 OCW：若 /peers 含有自身 peer_id → ok=true，否则 false。
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::report_probe())]
        pub fn report_probe(origin: OriginFor<T>, ok: bool) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let op = Operators::<T>::get(&who).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(op.status == 0, Error::<T>::BadStatus);
            OperatorSla::<T>::mutate(&who, |s| {
                if ok {
                    s.probe_ok = s.probe_ok.saturating_add(1);
                } else {
                    s.probe_fail = s.probe_fail.saturating_add(1);
                }
                s.last_update = <frame_system::Pallet<T>>::block_number();
            });
            Self::deposit_event(Event::OperatorProbed(who, ok));
            Ok(())
        }

        /// 函数级详细中文注释：治理扣罚运营者的保证金（阶梯惩罚使用）。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::slash_operator())]
        pub fn slash_operator(
            origin: OriginFor<T>,
            who: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                Operators::<T>::contains_key(&who),
                Error::<T>::OperatorNotFound
            );
            let (slashed, _remaining) = <T as Config>::Currency::slash_reserved(&who, amount);
            // 记录剩余 bond（slash_reserved 返回负不平衡，使用 peek 获取相应余额值再进行安全减法）
            let old = OperatorBond::<T>::get(&who);
            let slashed_amount = slashed.peek();
            let new = old.saturating_sub(slashed_amount);
            OperatorBond::<T>::insert(&who, new);

            // ✅ P1-10：发送扣罚事件
            Self::deposit_event(Event::OperatorSlashed {
                operator: who,
                amount: slashed_amount,
            });

            Ok(())
        }

        // ============================================================================
        // 新增治理接口：分层配置、扣费控制、运营者奖励（优化改造）
        // ============================================================================

        /// 函数级详细中文注释：治理更新分层配置
        ///
        /// 功能：
        /// - 允许治理提案动态调整分层配置参数
        /// - 支持调整副本数、巡检周期、费率系数、宽限期
        ///
        /// 参数：
        /// - tier: 分层等级（Critical/Standard/Temporary）
        /// - config: 新的配置参数
        ///
        /// 权限：
        /// - 治理Origin（Root或技术委员会）
        ///
        /// 验证：
        /// - 副本数：1-10
        /// - 巡检间隔：≥600块（约30分钟）
        /// - 费率系数：1000-100000（0.1x-10x）
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::update_tier_config())]
        pub fn update_tier_config(
            origin: OriginFor<T>,
            tier: PinTier,
            config: TierConfig,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 验证配置合理性
            ensure!(
                config.replicas > 0 && config.replicas <= 10,
                Error::<T>::InvalidReplicas
            );
            ensure!(
                config.health_check_interval >= 600, // 至少30分钟
                Error::<T>::IntervalTooShort
            );
            ensure!(
                config.fee_multiplier >= 1000 && config.fee_multiplier <= 100000, // 0.1x ~ 10x
                Error::<T>::InvalidMultiplier
            );

            // 更新配置
            PinTierConfig::<T>::insert(&tier, config.clone());

            // 发送事件
            Self::deposit_event(Event::TierConfigUpdated { tier, config });

            Ok(())
        }

        /// 函数级详细中文注释：运营者提取奖励
        ///
        /// 功能：
        /// - 运营者提取累计的存储费奖励
        /// - 从IpfsPoolAccount转账到运营者账户
        /// - 清零奖励记录
        ///
        /// 权限：
        /// - 签名账户（运营者本人）
        ///
        /// 检查：
        /// - 必须有可用奖励（余额 > 0）
        /// - IpfsPoolAccount余额充足
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::operator_claim_rewards())]
        pub fn operator_claim_rewards(origin: OriginFor<T>) -> DispatchResult {
            let operator = ensure_signed(origin)?;

            let reward = OperatorRewards::<T>::get(&operator);
            ensure!(!reward.is_zero(), Error::<T>::NoRewardsAvailable);

            let pool_account = T::IpfsPoolAccount::get();

            // 从 Pool 的 reserved 余额直接转入运营者账户（原子操作，防止竞态）
            let remaining = T::Currency::unreserve(&pool_account, reward);
            let actual_reward = reward.saturating_sub(remaining);

            if !actual_reward.is_zero() {
                T::Currency::transfer(
                    &pool_account,
                    &operator,
                    actual_reward,
                    ExistenceRequirement::KeepAlive,
                )?;
            }

            let unclaimed = reward.saturating_sub(actual_reward);
            if unclaimed.is_zero() {
                OperatorRewards::<T>::remove(&operator);
            } else {
                OperatorRewards::<T>::insert(&operator, unclaimed);
            }

            Self::deposit_event(Event::RewardsClaimed {
                operator: operator.clone(),
                amount: actual_reward,
            });

            if !unclaimed.is_zero() {
                Self::deposit_event(Event::RewardsClaimPartial {
                    operator,
                    claimed: actual_reward,
                    unclaimed,
                });
            }

            Ok(())
        }

        /// 函数级详细中文注释：紧急暂停自动扣费（应急开关）
        ///
        /// 使用场景：
        /// - 发现扣费逻辑漏洞，需要暂停保护用户资金
        /// - IPFS集群故障，暂停扣费直到恢复
        /// - 链上治理投票期间，暂停重大变更
        ///
        /// 功能：
        /// - 设置BillingPaused标志为true
        /// - on_finalize将跳过扣费逻辑
        ///
        /// 权限：
        /// - 治理Origin（Root或技术委员会）
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::emergency_pause_billing())]
        pub fn emergency_pause_billing(origin: OriginFor<T>) -> DispatchResult {
            // M5修复：不再丢弃实际调用者，但GovernanceOrigin可能返回()
            let _ = T::GovernanceOrigin::ensure_origin(origin)?;

            BillingPaused::<T>::put(true);

            Self::deposit_event(Event::BillingPausedByGovernance {
                by: Self::governance_account(),
            });

            Ok(())
        }

        /// 函数级详细中文注释：恢复自动扣费
        ///
        /// 功能：
        /// - 设置BillingPaused标志为false
        /// - on_finalize恢复扣费逻辑
        ///
        /// 权限：
        /// - 治理Origin（Root或技术委员会）
        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::resume_billing())]
        pub fn resume_billing(origin: OriginFor<T>) -> DispatchResult {
            let _ = T::GovernanceOrigin::ensure_origin(origin)?;

            BillingPaused::<T>::put(false);

            Self::deposit_event(Event::BillingResumedByGovernance {
                by: Self::governance_account(),
            });

            Ok(())
        }

        // ============================================================================
        // 新pallet域自动PIN机制相关Extrinsics
        // ============================================================================

        /// 函数级详细中文注释：治理手动注册域
        ///
        /// 功能：
        /// - 手动注册新业务域
        /// - 配置域的SubjectType映射
        /// - 设置域的默认Pin等级
        ///
        /// 权限：
        /// - 治理Origin（Root或技术委员会）
        ///
        /// 使用场景：
        /// - 预注册域，避免自动创建时的不确定性
        /// - 修改域的默认配置
        /// - 禁用某些域的自动PIN
        #[pallet::call_index(25)]
        #[pallet::weight(T::WeightInfo::register_domain())]
        pub fn register_domain(
            origin: OriginFor<T>,
            domain: Vec<u8>,
            subject_type_id: u8,
            default_tier: types::PinTier,
            auto_pin_enabled: bool,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 1. 转换域名为BoundedVec
            let bounded_domain: BoundedVec<u8, ConstU32<32>> =
                domain.try_into().map_err(|_| Error::<T>::InvalidDomain)?;

            // 2. 检查域是否已存在
            ensure!(
                !RegisteredDomains::<T>::contains_key(&bounded_domain),
                Error::<T>::DomainAlreadyExists
            );

            let config = types::DomainConfig {
                auto_pin_enabled,
                default_tier,
                subject_type_id,
                created_at: {
                    use sp_runtime::SaturatedConversion;
                    frame_system::Pallet::<T>::block_number().saturated_into()
                },
            };

            RegisteredDomains::<T>::insert(&bounded_domain, &config);

            RegisteredDomainList::<T>::mutate(|list| {
                if !list.contains(&bounded_domain) {
                    let _ = list.try_push(bounded_domain.clone());
                }
            });

            Self::deposit_event(Event::DomainRegistered {
                domain: bounded_domain,
                subject_type_id,
            });

            Ok(())
        }

        /// 函数级详细中文注释：治理更新域配置
        ///
        /// 功能：
        /// - 修改域的自动PIN开关
        /// - 修改域的默认等级
        /// - 重新映射SubjectType
        ///
        /// 权限：
        /// - 治理Origin（Root或技术委员会）
        #[pallet::call_index(26)]
        #[pallet::weight(T::WeightInfo::update_domain_config())]
        pub fn update_domain_config(
            origin: OriginFor<T>,
            domain: Vec<u8>,
            auto_pin_enabled: Option<bool>,
            default_tier: Option<types::PinTier>,
            subject_type_id: Option<u8>,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 1. 转换域名为BoundedVec
            let bounded_domain: BoundedVec<u8, ConstU32<32>> =
                domain.try_into().map_err(|_| Error::<T>::InvalidDomain)?;

            // 2. 更新配置
            RegisteredDomains::<T>::try_mutate(&bounded_domain, |maybe_config| {
                let config = maybe_config.as_mut().ok_or(Error::<T>::DomainNotFound)?;

                if let Some(enabled) = auto_pin_enabled {
                    config.auto_pin_enabled = enabled;
                }
                if let Some(tier) = default_tier {
                    config.default_tier = tier;
                }
                if let Some(type_id) = subject_type_id {
                    config.subject_type_id = type_id;
                }

                Ok::<(), Error<T>>(())
            })?;

            // 3. 发送事件（M1修复：使用更新后的实际值，而非 unwrap_or(true)）
            let updated_config =
                RegisteredDomains::<T>::get(&bounded_domain).ok_or(Error::<T>::DomainNotFound)?;
            Self::deposit_event(Event::DomainConfigUpdated {
                domain: bounded_domain,
                auto_pin_enabled: updated_config.auto_pin_enabled,
            });

            Ok(())
        }

        /// 函数级详细中文注释：用户请求取消固定CID
        ///
        /// ### 功能
        /// - 用户主动取消固定自己的CID
        /// - 标记CID为待删除状态
        /// - OCW将在后续区块执行物理删除
        /// - 停止后续扣费
        ///
        /// ### 参数
        /// - `cid`：要取消固定的IPFS CID（明文）
        ///
        /// ### 权限
        /// - 签名账户必须是CID的所有者（Pin时的caller）
        ///
        /// ### 行为
        /// 1. 计算CID哈希
        /// 2. 验证调用者是CID所有者
        /// 3. 标记为待删除状态（state=2）
        /// 4. 发送 MarkedForUnpin 事件
        /// 5. OCW后续执行物理删除
        #[pallet::call_index(32)]
        #[pallet::weight(T::WeightInfo::request_unpin())]
        pub fn request_unpin(origin: OriginFor<T>, cid: Vec<u8>) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            use sp_runtime::traits::Hash;

            // 1. 计算CID哈希
            let cid_hash = T::Hashing::hash(&cid[..]);

            // 2. 检查CID是否存在
            let meta = PinMeta::<T>::get(&cid_hash).ok_or(Error::<T>::OrderNotFound)?;

            // 3. 权限验证：检查调用者是否为CID所有者
            let (owner, _subject_id) =
                PinSubjectOf::<T>::get(&cid_hash).ok_or(Error::<T>::NotOwner)?;
            ensure!(caller == owner, Error::<T>::NotOwner);

            // ✅ P1-3 + M1-R3：按比例退款（提取为辅助函数，batch_unpin 共用）
            Self::try_refund_unpin(&cid_hash, &owner);

            // 4. 标记为待删除并同步停止后续计费
            Self::mark_cid_for_unpin(&cid_hash, UnpinReason::ManualRequest);

            Ok(())
        }

        /// 函数级详细中文注释：治理强制下架CID
        ///
        /// ### 功能
        /// - 治理可强制取消任意CID的固定，无需所有者授权
        /// - 用于违规内容下架、紧急处置等场景
        ///
        /// ### 参数
        /// - `cid`：要强制取消固定的IPFS CID（明文）
        /// - `reason`：下架原因（用于审计日志）
        ///
        /// ### 权限
        /// - GovernanceOrigin（Root或技术委员会）
        ///
        /// ### 行为
        /// 1. 计算CID哈希
        /// 2. 验证CID存在
        /// 3. 标记为待删除（GovernanceForceUnpin原因）
        /// 4. 发送 GovernanceForceUnpinned 事件
        #[pallet::call_index(33)]
        #[pallet::weight(T::WeightInfo::governance_force_unpin())]
        pub fn governance_force_unpin(
            origin: OriginFor<T>,
            cid: Vec<u8>,
            reason: Vec<u8>,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            use sp_runtime::traits::Hash;
            let cid_hash = T::Hashing::hash(&cid[..]);

            // 验证CID存在
            ensure!(
                PinMeta::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );

            // M1-R2修复：先验证输入参数，再执行副作用
            let reason_bounded: BoundedVec<u8, ConstU32<256>> =
                reason.try_into().map_err(|_| Error::<T>::BadParams)?;

            // 标记为待删除
            Self::mark_cid_for_unpin(&cid_hash, UnpinReason::GovernanceDecision);

            Self::deposit_event(Event::GovernanceForceUnpinned {
                cid_hash,
                reason: reason_bounded,
            });

            Ok(())
        }

        /// 函数级详细中文注释：手动清理过期CID存储
        ///
        /// ### 功能
        /// - 任何人可调用，清理已标记为过期（state=2）的CID关联存储
        /// - 补充 on_finalize 自动清理（每块限5个），允许批量加速清理
        ///
        /// ### 参数
        /// - `limit`：本次清理的最大CID数量（≤50）
        ///
        /// ### 行为
        /// 1. 扫描 PinBilling 中 state=2 的记录
        /// 2. 清理所有关联存储（PinMeta, PinAssignments, PinSuccess 等）
        /// 3. 回减 OperatorPinCount
        /// 4. 发送 PinRemoved 事件
        #[pallet::call_index(34)]
        #[pallet::weight(T::WeightInfo::cleanup_expired_cids(*limit))]
        pub fn cleanup_expired_cids(origin: OriginFor<T>, limit: u32) -> DispatchResult {
            ensure_signed(origin)?;

            let limit = limit.min(50);
            ensure!(limit > 0, Error::<T>::BadParams);

            let mut cleaned = 0u32;
            let mut to_clean: alloc::vec::Vec<T::Hash> = alloc::vec::Vec::new();

            // H1-R4优化：从 ExpiredCidQueue 出队，替代 PinBilling::iter() 全表扫描
            ExpiredCidQueue::<T>::mutate(|queue| {
                while cleaned < limit && !queue.is_empty() {
                    to_clean.push(queue.remove(0));
                    cleaned += 1;
                }
            });

            for cid_hash in to_clean.iter() {
                Self::do_cleanup_single_cid(cid_hash);
            }

            // H1-R4：队列为空时重置标志
            if ExpiredCidQueue::<T>::get().is_empty() {
                ExpiredCidPending::<T>::put(false);
            }

            Ok(())
        }

        /// 函数级详细中文注释：设置域优先级
        ///
        /// ### 功能
        /// - 设置域的巡检优先级
        /// - 优先级范围：0-255（0为最高优先级）
        /// - 影响OCW按域扫描的顺序
        ///
        /// ### 参数
        /// - `domain`：域名（如 b"evidence", b"otc", b"general"）
        /// - `priority`：优先级（0-255）
        ///
        /// ### 默认优先级
        /// - evidence: 0（最高）
        /// - otc: 10
        /// - general: 20
        /// - custom: 100
        /// - 其他：255（默认）
        ///
        /// ### 权限
        /// - Root权限
        ///
        /// ### 使用场景
        /// - 调整域的巡检优先级
        /// - 确保关键域优先处理
        #[pallet::call_index(27)]
        #[pallet::weight(T::WeightInfo::set_domain_priority())]
        pub fn set_domain_priority(
            origin: OriginFor<T>,
            domain: Vec<u8>,
            priority: u8,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            // 1. 转换域名为BoundedVec
            let bounded_domain: BoundedVec<u8, ConstU32<32>> =
                domain.try_into().map_err(|_| Error::<T>::InvalidDomain)?;

            // 2. 设置优先级
            DomainPriority::<T>::insert(&bounded_domain, priority);

            // 3. 发送事件
            Self::deposit_event(Event::DomainPrioritySet {
                domain: bounded_domain,
                priority,
            });

            Ok(())
        }

        // ================================================================
        // OCW Unsigned Extrinsics（P1/P2: 通过 ValidateUnsigned 验证）
        // ================================================================

        /// OCW 上报 Pin 成功（unsigned，替代直写 PinStateOf）
        #[pallet::call_index(40)]
        #[pallet::weight(T::WeightInfo::ocw_mark_pinned())]
        pub fn ocw_mark_pinned(
            origin: OriginFor<T>,
            operator: T::AccountId,
            cid_hash: T::Hash,
            replicas: u32,
        ) -> DispatchResult {
            ensure_none(origin)?;
            // 验证运营者有效
            let op = Operators::<T>::get(&operator).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(op.status == 0, Error::<T>::OperatorBanned);
            ensure!(
                PendingPins::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );
            // 验证运营者被分配
            if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                ensure!(
                    assign.iter().any(|a| a == &operator),
                    Error::<T>::OperatorNotAssigned
                );
            } else {
                return Err(Error::<T>::AssignmentNotFound.into());
            }
            // 标记该运营者完成
            PinSuccess::<T>::insert(&cid_hash, &operator, true);
            // 达到副本数则完成
            if let Some(meta) = PinMeta::<T>::get(&cid_hash) {
                let expect = meta.replicas;
                let mut ok_count: u32 = 0;
                if let Some(ops) = PinAssignments::<T>::get(&cid_hash) {
                    for o in ops.iter() {
                        if PinSuccess::<T>::get(&cid_hash, o) {
                            ok_count = ok_count.saturating_add(1);
                        }
                    }
                }
                if ok_count >= expect {
                    PendingPins::<T>::remove(&cid_hash);
                    PinStateOf::<T>::insert(&cid_hash, 2u8); // Pinned
                    Self::deposit_event(Event::PinStateChanged(cid_hash, 2));
                } else {
                    PinStateOf::<T>::insert(&cid_hash, 1u8); // Pinning
                    Self::deposit_event(Event::PinStateChanged(cid_hash, 1));
                }
            }
            Self::deposit_event(Event::PinMarkedPinned(cid_hash, replicas));
            Ok(())
        }

        /// OCW 上报 Pin 失败（unsigned）
        #[pallet::call_index(41)]
        #[pallet::weight(T::WeightInfo::ocw_mark_pin_failed())]
        pub fn ocw_mark_pin_failed(
            origin: OriginFor<T>,
            operator: T::AccountId,
            cid_hash: T::Hash,
            code: u16,
        ) -> DispatchResult {
            ensure_none(origin)?;
            let op = Operators::<T>::get(&operator).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(op.status == 0, Error::<T>::OperatorBanned);
            ensure!(
                PendingPins::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );
            if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                ensure!(
                    assign.iter().any(|a| a == &operator),
                    Error::<T>::OperatorNotAssigned
                );
            } else {
                return Err(Error::<T>::AssignmentNotFound.into());
            }
            PinSuccess::<T>::insert(&cid_hash, &operator, false);
            Self::deposit_event(Event::PinMarkedFailed(cid_hash, code));
            Ok(())
        }

        /// OCW 提交分层Pin分配（unsigned，替代直写 LayeredPinAssignments/PinAssignments）
        #[pallet::call_index(42)]
        #[pallet::weight(T::WeightInfo::ocw_submit_assignments())]
        pub fn ocw_submit_assignments(
            origin: OriginFor<T>,
            cid_hash: T::Hash,
            core_operators: Vec<T::AccountId>,
            community_operators: Vec<T::AccountId>,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(
                PendingPins::<T>::contains_key(&cid_hash),
                Error::<T>::OrderNotFound
            );
            // 防止重复分配
            ensure!(
                LayeredPinAssignments::<T>::get(&cid_hash).is_none(),
                Error::<T>::AssignmentNotFound // 已有分配则拒绝
            );
            // 验证所有运营者有效
            for op in core_operators.iter().chain(community_operators.iter()) {
                ensure!(
                    Operators::<T>::contains_key(op),
                    Error::<T>::OperatorNotFound
                );
            }

            let core_ops = BoundedVec::truncate_from(core_operators.clone());
            let community_ops = BoundedVec::truncate_from(community_operators.clone());

            LayeredPinAssignments::<T>::insert(
                &cid_hash,
                LayeredPinAssignment {
                    core_operators: core_ops.clone(),
                    community_operators: community_ops.clone(),
                },
            );

            let cid_size = PinMeta::<T>::get(&cid_hash).map(|m| m.size).unwrap_or(0);
            let mut all_ops: alloc::vec::Vec<T::AccountId> = core_ops.to_vec();
            all_ops.extend(community_ops.to_vec());
            for op in all_ops.iter() {
                OperatorPinCount::<T>::mutate(op, |c| *c = c.saturating_add(1));
                OperatorUsedBytes::<T>::mutate(op, |b| *b = b.saturating_add(cid_size));
            }
            if let Ok(operators_bounded) = BoundedVec::try_from(all_ops) {
                PinAssignments::<T>::insert(&cid_hash, operators_bounded);
            }

            Self::deposit_event(Event::LayeredPinAssigned {
                cid_hash,
                core_operators: core_ops,
                community_operators: community_ops,
            });
            Ok(())
        }

        /// OCW 上报健康巡检结果（unsigned，替代直写 PinSuccess/OperatorPinStats）
        #[pallet::call_index(43)]
        #[pallet::weight(T::WeightInfo::ocw_report_health())]
        pub fn ocw_report_health(
            origin: OriginFor<T>,
            cid_hash: T::Hash,
            operator: T::AccountId,
            is_pinned: bool,
        ) -> DispatchResult {
            ensure_none(origin)?;
            ensure!(
                Operators::<T>::contains_key(&operator),
                Error::<T>::OperatorNotFound
            );

            // H2-R3修复：验证运营者已被分配到该CID（与 mark_pinned/mark_pin_failed 一致）
            if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                ensure!(
                    assign.iter().any(|a| a == &operator),
                    Error::<T>::OperatorNotAssigned
                );
            } else {
                return Err(Error::<T>::AssignmentNotFound.into());
            }

            let was_success = PinSuccess::<T>::get(&cid_hash, &operator);

            if is_pinned && !was_success {
                // 恢复：offline → online
                PinSuccess::<T>::insert(&cid_hash, &operator, true);
                let _ = OperatorPinStats::<T>::try_mutate(&operator, |stats| {
                    stats.healthy_pins = stats.healthy_pins.saturating_add(1);
                    stats.last_check = <frame_system::Pallet<T>>::block_number();
                    stats.health_score = Self::calculate_health_score(&operator);
                    Ok::<(), ()>(())
                });
                Self::deposit_event(Event::ReplicaRepaired(cid_hash, operator));
            } else if !is_pinned && was_success {
                // 降级：online → offline
                PinSuccess::<T>::insert(&cid_hash, &operator, false);
                let _ = OperatorPinStats::<T>::try_mutate(&operator, |stats| {
                    stats.healthy_pins = stats.healthy_pins.saturating_sub(1);
                    stats.failed_pins = stats.failed_pins.saturating_add(1);
                    let old_score = stats.health_score;
                    stats.last_check = <frame_system::Pallet<T>>::block_number();
                    stats.health_score = Self::calculate_health_score(&operator);
                    if old_score.saturating_sub(stats.health_score) >= 10 {
                        Self::deposit_event(Event::OperatorHealthDegraded {
                            operator: operator.clone(),
                            old_score,
                            new_score: stats.health_score,
                            total_pins: stats.total_pins,
                            failed_pins: stats.failed_pins,
                        });
                    }
                    // ✅ P1-12：健康分低于阈值（30）自动暂停运营者
                    if stats.health_score < 30 {
                        if let Some(mut info) = Operators::<T>::get(&operator) {
                            if info.status == 0 {
                                // 仅Active→Suspended
                                info.status = 1; // Suspended
                                Operators::<T>::insert(&operator, info);
                                // 从活跃索引移除
                                ActiveOperatorIndex::<T>::mutate(|idx| {
                                    idx.retain(|a| a != &operator);
                                });
                                Self::deposit_event(Event::OperatorAutoSuspended {
                                    operator: operator.clone(),
                                    health_score: stats.health_score,
                                });
                            }
                        }
                    }
                    Ok::<(), ()>(())
                });
                OperatorSla::<T>::mutate(&operator, |s| {
                    s.degraded = s.degraded.saturating_add(1);
                });
                Self::deposit_event(Event::ReplicaDegraded(cid_hash, operator));
            }
            // 状态未变则忽略（幂等）
            Ok(())
        }

        /// 函数级详细中文注释：续期 Pin（延长计费周期） ✅ P1-2新增
        ///
        /// 功能：
        /// - 为已存在的CID预付若干周期的续期费用
        /// - 延长 BillingQueue 中的下次扣费时间
        /// - 更新 PinMeta::last_activity
        ///
        /// 参数：
        /// - cid_hash: CID哈希
        /// - periods: 续期周期数（1-52）
        #[pallet::call_index(45)]
        #[pallet::weight(T::WeightInfo::renew_pin())]
        pub fn renew_pin(origin: OriginFor<T>, cid_hash: T::Hash, periods: u32) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            ensure!(periods > 0 && periods <= 52, Error::<T>::BadParams);

            // 验证CID存在
            let meta = PinMeta::<T>::get(&cid_hash).ok_or(Error::<T>::OrderNotFound)?;

            // M3-R4修复：验证调用者是CID所有者（使用 NotOwner 错误）
            let (owner, _subject_id) =
                PinSubjectOf::<T>::get(&cid_hash).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(caller == owner, Error::<T>::NotOwner);

            // 计算续期费用
            let tier = CidTier::<T>::get(&cid_hash);
            let tier_config = Self::get_tier_config(&tier)?;
            let period_fee = Self::calculate_period_fee(meta.size, tier_config.replicas)?;
            let period_fee_adjusted =
                period_fee.saturating_mul(tier_config.fee_multiplier.into()) / 10000u32.into();
            let total_fee = period_fee_adjusted.saturating_mul(periods.into());

            Self::charge_user_layered(&caller, &cid_hash, total_fee)?;

            let current_block = <frame_system::Pallet<T>>::block_number();
            let billing_period = BillingPeriodBlocks::<T>::get();
            let extension: BlockNumberFor<T> = (billing_period.saturating_mul(periods)).into();

            // O(1) 查找：通过 CidBillingDueBlock 反向索引定位 BillingQueue 条目
            let mut found = false;
            if let Some(old_due) = CidBillingDueBlock::<T>::get(&cid_hash) {
                if let Some(task) = BillingQueue::<T>::get(old_due, &cid_hash) {
                    BillingQueue::<T>::remove(old_due, &cid_hash);
                    let new_due = old_due.saturating_add(extension);
                    BillingQueue::<T>::insert(new_due, &cid_hash, task);
                    CidBillingDueBlock::<T>::insert(&cid_hash, new_due);
                    found = true;
                }
            }

            if !found {
                if let Some((next_due, unit_price, state)) = PinBilling::<T>::get(&cid_hash) {
                    if state != 2u8 {
                        let new_due = next_due.saturating_add(extension);
                        PinBilling::<T>::insert(&cid_hash, (new_due, unit_price, state));
                    }
                }
            }

            // 更新 PinMeta
            PinMeta::<T>::mutate(&cid_hash, |m| {
                if let Some(meta) = m {
                    meta.last_activity = current_block;
                }
            });

            Self::deposit_event(Event::PinRenewed {
                cid_hash,
                periods,
                total_fee,
            });

            Ok(())
        }

        /// 函数级详细中文注释：升级 Pin 的分层等级 ✅ P1-2新增
        ///
        /// 功能：
        /// - 将CID从当前tier升级到更高tier（如 Standard → Critical）
        /// - 升级后调整副本数和巡检频率
        /// - 收取tier差价费用
        ///
        /// 参数：
        /// - cid_hash: CID哈希
        /// - new_tier: 新的分层等级
        #[pallet::call_index(46)]
        #[pallet::weight(T::WeightInfo::upgrade_pin_tier())]
        pub fn upgrade_pin_tier(
            origin: OriginFor<T>,
            cid_hash: T::Hash,
            new_tier: PinTier,
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            // 验证CID存在
            let meta = PinMeta::<T>::get(&cid_hash).ok_or(Error::<T>::OrderNotFound)?;

            // M3-R4修复：验证调用者是CID所有者（使用 NotOwner 错误）
            let (owner, _subject_id) =
                PinSubjectOf::<T>::get(&cid_hash).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(caller == owner, Error::<T>::NotOwner);

            let old_tier = CidTier::<T>::get(&cid_hash);
            let old_config = Self::get_tier_config(&old_tier)?;
            let new_config = Self::get_tier_config(&new_tier)?;

            // 验证是升级（新tier的fee_multiplier应更高）
            ensure!(
                new_config.fee_multiplier > old_config.fee_multiplier,
                Error::<T>::BadParams
            );

            // 计算差价（一个周期的差价）
            let old_fee = Self::calculate_period_fee(meta.size, old_config.replicas)?
                .saturating_mul(old_config.fee_multiplier.into())
                / 10000u32.into();
            let new_fee = Self::calculate_period_fee(meta.size, new_config.replicas)?
                .saturating_mul(new_config.fee_multiplier.into())
                / 10000u32.into();
            let diff = new_fee.saturating_sub(old_fee);

            Self::charge_user_layered(&caller, &cid_hash, diff)?;

            // 更新 tier
            CidTier::<T>::insert(&cid_hash, new_tier.clone());

            let current_block = <frame_system::Pallet<T>>::block_number();

            // O(1) 更新 BillingQueue 中的 amount_per_period 为新费率
            let mut billing_updated = false;
            if let Some(due_block) = CidBillingDueBlock::<T>::get(&cid_hash) {
                if let Some(mut task) = BillingQueue::<T>::get(due_block, &cid_hash) {
                    task.amount_per_period = new_fee;
                    BillingQueue::<T>::insert(due_block, &cid_hash, task);
                    billing_updated = true;
                }
            }
            // 兼容旧的 PinBilling 路径
            if !billing_updated {
                if let Some((next_due, _old_unit_price, state)) = PinBilling::<T>::get(&cid_hash) {
                    if state != 2u8 {
                        // 更新单价为新费率（PinBilling 存储 u128）
                        let new_unit_price: u128 = new_fee.saturated_into();
                        PinBilling::<T>::insert(&cid_hash, (next_due, new_unit_price, state));
                    }
                }
            }

            // 更新副本数（如果新tier需要更多副本）
            if new_config.replicas > old_config.replicas {
                PinMeta::<T>::mutate(&cid_hash, |m| {
                    if let Some(meta) = m {
                        meta.replicas = new_config.replicas;
                        meta.last_activity = current_block;
                    }
                });
                // 触发自动修复补充副本
                let current = PinAssignments::<T>::get(&cid_hash)
                    .map(|a| a.len() as u32)
                    .unwrap_or(0);
                if current < new_config.replicas {
                    Self::try_auto_repair(&cid_hash, current, new_config.replicas);
                }
            }

            Self::deposit_event(Event::PinTierUpgraded {
                cid_hash,
                old_tier,
                new_tier,
                fee_diff: diff,
            });

            Ok(())
        }

        /// 函数级详细中文注释：运营者追加保证金 ✅ P2-8新增
        ///
        /// 允许运营者追加保证金以提高信誉或恢复被slash后的保证金水平。
        #[pallet::call_index(49)]
        #[pallet::weight(T::WeightInfo::top_up_bond())]
        pub fn top_up_bond(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::BadParams);
            ensure!(
                Operators::<T>::contains_key(&who),
                Error::<T>::OperatorNotFound
            );

            <T as Config>::Currency::reserve(&who, amount)?;
            OperatorBond::<T>::mutate(&who, |bond| {
                *bond = bond.saturating_add(amount);
            });

            let new_total = OperatorBond::<T>::get(&who);
            Self::deposit_event(Event::BondTopUp {
                operator: who,
                amount,
                new_total,
            });

            Ok(())
        }

        /// 函数级详细中文注释：运营者减少保证金 ✅ P2-8新增
        ///
        /// 允许运营者取回多余保证金，但不能低于最低保证金要求。
        #[pallet::call_index(50)]
        #[pallet::weight(T::WeightInfo::reduce_bond())]
        pub fn reduce_bond(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::BadParams);
            ensure!(
                Operators::<T>::contains_key(&who),
                Error::<T>::OperatorNotFound
            );

            let current_bond = OperatorBond::<T>::get(&who);
            let min_bond = Self::calculate_operator_bond();
            let new_bond = current_bond.saturating_sub(amount);
            ensure!(new_bond >= min_bond, Error::<T>::InsufficientBond);

            // L1-R3修复：检查 unreserve 返回的 deficit，只减去实际解锁的金额
            let deficit = <T as Config>::Currency::unreserve(&who, amount);
            let actually_unreserved = amount.saturating_sub(deficit);
            let new_bond = current_bond.saturating_sub(actually_unreserved);
            OperatorBond::<T>::insert(&who, new_bond);

            Self::deposit_event(Event::BondReduced {
                operator: who,
                amount: actually_unreserved,
                new_total: new_bond,
            });

            Ok(())
        }

        /// 函数级详细中文注释：批量取消Pin ✅ P2-5新增
        ///
        /// 功能：
        /// - 允许所有者一次取消多个CID的固定
        /// - 每个CID独立处理，失败的跳过不影响其他
        /// - 最多一次处理20个CID
        #[pallet::call_index(48)]
        #[pallet::weight(T::WeightInfo::batch_unpin())]
        pub fn batch_unpin(origin: OriginFor<T>, cids: Vec<Vec<u8>>) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            ensure!(!cids.is_empty() && cids.len() <= 20, Error::<T>::BadParams);

            use sp_runtime::traits::Hash;

            let mut unpinned = 0u32;
            for cid in cids.iter() {
                let cid_hash = T::Hashing::hash(&cid[..]);

                // 验证存在且为调用者所有
                if !PinMeta::<T>::contains_key(&cid_hash) {
                    continue;
                }
                if let Some((owner, _)) = PinSubjectOf::<T>::get(&cid_hash) {
                    if owner != caller {
                        continue;
                    }
                } else {
                    continue;
                }

                // M1-R3修复：batch_unpin 也执行退款逻辑（与 request_unpin 一致）
                Self::try_refund_unpin(&cid_hash, &caller);
                Self::mark_cid_for_unpin(&cid_hash, UnpinReason::ManualRequest);
                unpinned += 1;
            }

            Self::deposit_event(Event::BatchUnpinCompleted {
                who: caller,
                requested: cids.len() as u32,
                unpinned,
            });

            Ok(())
        }

        /// L2-R3修复：清理过期的 CidLocks 条目
        ///
        /// 任何人可调用，每次最多清理 max_count 个过期锁。
        /// 覆盖 CID 仍存活但锁已过期的场景（CID 被删除时由 cleanup 路径自动清理）。
        #[pallet::call_index(51)]
        #[pallet::weight(T::WeightInfo::cleanup_expired_locks())]
        pub fn cleanup_expired_locks(origin: OriginFor<T>, max_count: u32) -> DispatchResult {
            ensure_signed(origin)?;
            ensure!(max_count > 0 && max_count <= 20, Error::<T>::BadParams);

            let now = <frame_system::Pallet<T>>::block_number();
            let cursor = LockCleanupCursor::<T>::get();
            let mut iter = match cursor {
                Some(ref raw) => CidLocks::<T>::iter_from(raw.to_vec()),
                None => CidLocks::<T>::iter(),
            };

            let mut cleaned = 0u32;
            let mut scanned = 0u32;
            let scan_limit = max_count.saturating_mul(5);
            let mut to_remove: alloc::vec::Vec<T::Hash> = alloc::vec::Vec::new();
            let mut last_raw_key: Option<alloc::vec::Vec<u8>> = None;

            while let Some((cid_hash, (_reason, until))) = iter.next() {
                scanned += 1;
                last_raw_key = Some(CidLocks::<T>::hashed_key_for(&cid_hash));
                if let Some(expiry) = until {
                    if now > expiry {
                        to_remove.push(cid_hash);
                        cleaned += 1;
                    }
                }
                if cleaned >= max_count || scanned >= scan_limit {
                    break;
                }
            }

            let exhausted = scanned < scan_limit && cleaned < max_count;

            for cid_hash in to_remove.iter() {
                CidLocks::<T>::remove(cid_hash);
                Self::deposit_event(Event::CidUnlocked {
                    cid_hash: *cid_hash,
                });
            }

            if exhausted {
                LockCleanupCursor::<T>::kill();
            } else if let Some(key) = last_raw_key {
                if let Ok(bounded) = BoundedVec::try_from(key) {
                    LockCleanupCursor::<T>::put(bounded);
                }
            }

            Ok(())
        }

        /// 函数级详细中文注释：运营者迁移 — 将所有Pin分配从一个运营者转移到另一个 ✅ P1-9新增
        ///
        /// 功能：
        /// - 治理将 `from` 运营者的所有Pin分配迁移到 `to` 运营者
        /// - 更新 PinAssignments、OperatorPinCount、OperatorUsedBytes
        /// - 用于运营者退出交接或故障替换
        ///
        /// 限制：
        /// - 每次最多迁移 max_pins 个CID（避免区块超重）
        /// - `to` 运营者必须为Active状态
        #[pallet::call_index(47)]
        #[pallet::weight(T::WeightInfo::migrate_operator_pins())]
        pub fn migrate_operator_pins(
            origin: OriginFor<T>,
            from: T::AccountId,
            to: T::AccountId,
            max_pins: u32,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;

            ensure!(from != to, Error::<T>::BadParams);
            let max_pins = max_pins.min(100);
            ensure!(max_pins > 0, Error::<T>::BadParams);

            // 验证目标运营者存在且Active
            let to_info = Operators::<T>::get(&to).ok_or(Error::<T>::OperatorNotFound)?;
            ensure!(to_info.status == 0, Error::<T>::OperatorBanned);

            let cursor = MigrateOpCursor::<T>::get();
            let mut iter = match cursor {
                Some(ref raw) => PinAssignments::<T>::iter_from(raw.to_vec()),
                None => PinAssignments::<T>::iter(),
            };

            let mut migrated = 0u32;
            let mut scanned = 0u32;
            let scan_limit = max_pins.saturating_mul(10);
            let mut cids_to_migrate: alloc::vec::Vec<T::Hash> = alloc::vec::Vec::new();
            let mut last_raw_key: Option<alloc::vec::Vec<u8>> = None;

            while let Some((cid_hash, assignments)) = iter.next() {
                scanned += 1;
                last_raw_key = Some(PinAssignments::<T>::hashed_key_for(&cid_hash));
                if assignments.iter().any(|a| a == &from) {
                    cids_to_migrate.push(cid_hash);
                    migrated += 1;
                }
                if migrated >= max_pins || scanned >= scan_limit {
                    break;
                }
            }

            let exhausted = scanned < scan_limit && migrated < max_pins;
            if exhausted {
                MigrateOpCursor::<T>::kill();
            } else if let Some(key) = last_raw_key {
                if let Ok(bounded) = BoundedVec::try_from(key) {
                    MigrateOpCursor::<T>::put(bounded);
                }
            }

            let mut total_bytes_moved: u64 = 0;

            for cid_hash in cids_to_migrate.iter() {
                if let Some(mut assignments) = PinAssignments::<T>::get(cid_hash) {
                    // 替换 from → to（如果to不在列表中）
                    if !assignments.iter().any(|a| a == &to) {
                        if let Some(pos) = assignments.iter().position(|a| a == &from) {
                            assignments[pos] = to.clone();
                            PinAssignments::<T>::insert(cid_hash, assignments);

                            let cid_size = PinMeta::<T>::get(cid_hash).map(|m| m.size).unwrap_or(0);
                            total_bytes_moved = total_bytes_moved.saturating_add(cid_size);

                            // 更新统计
                            OperatorPinCount::<T>::mutate(&from, |c| *c = c.saturating_sub(1));
                            OperatorPinCount::<T>::mutate(&to, |c| *c = c.saturating_add(1));
                            OperatorUsedBytes::<T>::mutate(&from, |b| {
                                *b = b.saturating_sub(cid_size)
                            });
                            OperatorUsedBytes::<T>::mutate(&to, |b| {
                                *b = b.saturating_add(cid_size)
                            });
                        }
                    } else {
                        // to已在列表中，直接移除from
                        let mut v = assignments.to_vec();
                        v.retain(|a| a != &from);
                        if let Ok(new_assignments) = BoundedVec::try_from(v) {
                            PinAssignments::<T>::insert(cid_hash, new_assignments);
                            OperatorPinCount::<T>::mutate(&from, |c| *c = c.saturating_sub(1));
                            let cid_size = PinMeta::<T>::get(cid_hash).map(|m| m.size).unwrap_or(0);
                            OperatorUsedBytes::<T>::mutate(&from, |b| {
                                *b = b.saturating_sub(cid_size)
                            });
                        }
                    }
                }
            }

            Self::deposit_event(Event::OperatorPinsMigrated {
                from,
                to,
                pins_migrated: migrated,
                bytes_moved: total_bytes_moved,
            });

            Ok(())
        }

        /// 函数级详细中文注释：向 IPFS 公共池充值 ✅ P1-14新增
        ///
        /// 任何签名账户都可以向 IpfsPoolAccount 转账充值。
        /// 如果充值后余额仍低于阈值（100 NEX），发出余额预警事件。
        #[pallet::call_index(44)]
        #[pallet::weight(T::WeightInfo::fund_ipfs_pool())]
        pub fn fund_ipfs_pool(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::BadParams);

            let pool = T::IpfsPoolAccount::get();
            <T as Config>::Currency::transfer(
                &who,
                &pool,
                amount,
                frame_support::traits::ExistenceRequirement::KeepAlive,
            )?;

            let new_balance = <T as Config>::Currency::free_balance(&pool);

            Self::deposit_event(Event::IpfsPoolFunded {
                who,
                amount,
                new_balance,
            });

            // 余额预警：低于 100 NEX (100 * 10^12 = 100_000_000_000_000)
            let threshold: BalanceOf<T> = 100_000_000_000_000u128.saturated_into();
            if new_balance < threshold {
                Self::deposit_event(Event::IpfsPoolLowBalance {
                    current_balance: new_balance,
                    threshold,
                });
            }

            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// 函数级详细中文注释：Offchain Worker 入口
        /// - 周期性扫描 `PendingPins`，对每个 `cid_hash` 调用 ipfs-cluster API 进行 Pin；
        /// - 成功则提交 `mark_pinned`，失败则提交 `mark_pin_failed`；
        /// - HTTP 令牌与集群端点从本地 offchain storage 读取，避免上链泄露。
        fn offchain_worker(n: BlockNumberFor<T>) {
            // 读取本地配置（示例键）："/memo/ipfs/cluster_endpoint" 与 "/memo/ipfs/token"
            let endpoint: alloc::string::String = sp_io::offchain::local_storage_get(
                StorageKind::PERSISTENT,
                b"/memo/ipfs/cluster_endpoint",
            )
            .and_then(|v| core::str::from_utf8(&v).ok().map(|s| s.to_string()))
            .unwrap_or_else(|| alloc::string::String::from("http://127.0.0.1:9094"));
            let token: Option<alloc::string::String> =
                sp_io::offchain::local_storage_get(StorageKind::PERSISTENT, b"/memo/ipfs/token")
                    .and_then(|v| core::str::from_utf8(&v).ok().map(|s| s.to_string()));

            // 分配与 Pin：遍历 PendingPins，若无分配则创建；否则尝试 POST /pins 携带 allocations
            if let Some((cid_hash, (_payer, _replicas, _subject_id, _size, _price))) =
                <PendingPins<T>>::iter().next()
            {
                // ⭐ P2: 通过 unsigned extrinsic 提交分配（替代 OCW 直写）
                // 防重复：检查是否在最近 5 个区块内已提交过
                if LayeredPinAssignments::<T>::get(&cid_hash).is_none()
                    && !Self::ocw_recently_submitted(
                        b"ocw-assign",
                        &cid_hash.encode(),
                        n,
                        5u32.into(),
                    )
                {
                    let tier = CidTier::<T>::get(&cid_hash);
                    let subject_type = SubjectType::Custom(Default::default());

                    if let Ok(selection) =
                        Self::select_operators_by_layer(subject_type, tier.clone())
                    {
                        let core_ops = selection.core_operators.to_vec();
                        let community_ops = selection.community_operators.to_vec();

                        // 提交 unsigned extrinsic（通过共识持久化）
                        let call = Call::<T>::ocw_submit_assignments {
                            cid_hash,
                            core_operators: core_ops,
                            community_operators: community_ops,
                        };
                        let xt = T::create_bare(call.into());
                        if frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_ok() {
                            Self::ocw_set_last_submitted(b"ocw-assign", &cid_hash.encode(), n);
                        }
                    }
                }
                // 发起 Pin 请求
                let _ = Self::submit_pin_request(&endpoint, &token, cid_hash);
                // ⭐ P2: 通过 unsigned extrinsic 更新 Pin 状态（替代直写 PinStateOf）
                // 防重复：检查是否在最近 5 个区块内已提交过
                if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                    if let Some(first_op) = assign.first() {
                        if !Self::ocw_recently_submitted(
                            b"ocw-pin",
                            &cid_hash.encode(),
                            n,
                            5u32.into(),
                        ) {
                            let call = Call::<T>::ocw_mark_pinned {
                                operator: first_op.clone(),
                                cid_hash,
                                replicas: 1,
                            };
                            let xt = T::create_bare(call.into());
                            if frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_ok() {
                                Self::ocw_set_last_submitted(b"ocw-pin", &cid_hash.encode(), n);
                            }
                        }
                    }
                }
            }

            // 探测自身是否在线（运营者必须运行集群节点）：读取 /peers 并查找自身 peer_id
            // 探测自身是否在线：简化为本地统计，避免依赖 CreateSignedTransaction
            let _ = Self::http_get_bytes(&endpoint, &token, "/peers");

            // 巡检：针对已 Pinned/Pinning 的对象，GET /pins/{cid} 矫正副本；若缺少则再 Pin；并统计上报
            // 注意：演示中未持有明文 CID，这里仅示意调用；生产需有 CID 解密/映射。
            // 逻辑：遍历 PinStateOf in {1=Pinning,2=Pinned}，若 assignments 存在，检查成功标记数；不足副本则再次发起 submit_pin_request。
            let mut sample: u32 = 0;
            let mut pinning: u32 = 0;
            let mut pinned: u32 = 0;
            let mut missing: u32 = 0;
            for (cid_hash, state) in PinStateOf::<T>::iter() {
                if state == 1u8 || state == 2u8 {
                    sample = sample.saturating_add(1);
                    if let Some(assign) = PinAssignments::<T>::get(&cid_hash) {
                        let expect = PinMeta::<T>::get(&cid_hash)
                            .map(|m| m.replicas)
                            .unwrap_or(assign.len() as u32);
                        let mut ok_count: u32 = 0;
                        for o in assign.iter() {
                            if PinSuccess::<T>::get(&cid_hash, o) {
                                ok_count = ok_count.saturating_add(1);
                            }
                        }
                        if ok_count < expect {
                            // 副本不足，需要补充
                            let shortage = expect.saturating_sub(ok_count);

                            // 解析 /pins/{cid}，对比分配并触发降级/修复事件
                            let cid_str = Self::resolve_cid(&cid_hash);
                            // 直接 GET /pins/{cid} 获取状态（Plan B 替换 submit_get_pin_status_collect）
                            if let Some(body) = Self::http_get_bytes(
                                &endpoint,
                                &token,
                                &alloc::format!("/pins/{}", cid_str),
                            ) {
                                let mut online_peers: Vec<Vec<u8>> = Vec::new();
                                if let Ok(json) = serde_json::from_slice::<JsonValue>(&body) {
                                    // 兼容两类结构：{peer_map:{"peerid":{status:"pinned"|...}}} 或 {allocations:["peerid",...]}
                                    if let Some(map) =
                                        json.get("peer_map").and_then(|v| v.as_object())
                                    {
                                        for (pid, st) in map.iter() {
                                            if st.get("status").and_then(|s| s.as_str())
                                                == Some("pinned")
                                            {
                                                online_peers.push(pid.as_bytes().to_vec());
                                            }
                                        }
                                    } else if let Some(arr) =
                                        json.get("allocations").and_then(|v| v.as_array())
                                    {
                                        for v in arr.iter() {
                                            if let Some(s) = v.as_str() {
                                                online_peers.push(s.as_bytes().to_vec());
                                            }
                                        }
                                    }
                                }
                                // ⭐ P2: 通过 unsigned extrinsic 上报健康状态（替代直写）
                                for op_acc in assign.iter() {
                                    if let Some(info) = Operators::<T>::get(op_acc) {
                                        let present = online_peers
                                            .iter()
                                            .any(|p| p.as_slice() == info.peer_id.as_slice());
                                        let success = PinSuccess::<T>::get(&cid_hash, op_acc);
                                        // 仅在状态变化时提交（幂等，避免无意义交易）
                                        // 防重复：检查是否在最近 3 个区块内已提交过
                                        let health_key_data = (cid_hash, op_acc).encode();
                                        if present != success
                                            && !Self::ocw_recently_submitted(
                                                b"ocw-health",
                                                &health_key_data,
                                                n,
                                                3u32.into(),
                                            )
                                        {
                                            let call = Call::<T>::ocw_report_health {
                                                cid_hash,
                                                operator: op_acc.clone(),
                                                is_pinned: present,
                                            };
                                            let xt = T::create_bare(call.into());
                                            if frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_ok() {
                                                Self::ocw_set_last_submitted(b"ocw-health", &health_key_data, n);
                                            }
                                        }
                                    }
                                }

                                // ⭐ P0优化：自动补充副本使用分层选择算法
                                if shortage > 0 {
                                    // 获取CID的Tier（ValueQuery自动返回默认值）和SubjectType
                                    let tier = CidTier::<T>::get(&cid_hash);
                                    let subject_type = SubjectType::Custom(Default::default());

                                    // 使用分层选择算法获取新运营者
                                    if let Ok(selection) =
                                        Self::select_operators_by_layer(subject_type, tier)
                                    {
                                        // 合并Layer 1和Layer 2运营者
                                        let mut new_candidates = selection.core_operators.to_vec();
                                        new_candidates
                                            .extend(selection.community_operators.to_vec());

                                        // 获取当前已分配的运营者（用于去重）
                                        let current_operators: alloc::vec::Vec<T::AccountId> =
                                            assign.iter().cloned().collect();

                                        // 过滤出新的运营者（未在当前分配列表中）
                                        let mut updated_assign = assign.clone();
                                        let mut added_count = 0u32;

                                        for new_op in new_candidates.iter() {
                                            if !current_operators.contains(new_op)
                                                && added_count < shortage
                                            {
                                                if updated_assign.try_push(new_op.clone()).is_ok() {
                                                    added_count += 1;
                                                }
                                            }
                                        }

                                        // ⭐ P2: 通过 unsigned extrinsic 提交补充分配
                                        if added_count > 0 {
                                            let _new_core: Vec<T::AccountId> = new_candidates
                                                .iter()
                                                .filter(|op| !current_operators.contains(op))
                                                .take(added_count as usize)
                                                .cloned()
                                                .collect();
                                            // 注意：ocw_submit_assignments 要求无现有分配
                                            // 副本补充走不同路径，这里直接提交 mark_pinned 触发重新评估
                                        }
                                    }
                                }
                            }
                            // 再 Pin（带退避）
                            let _ = Self::submit_pin_request(&endpoint, &token, cid_hash);
                            // ⭐ P2: 通过 unsigned extrinsic 触发状态重评估
                            // 防重复：复用 ocw-pin 锁（同一 cid_hash 不重复提交）
                            if let Some(first_op) = assign.first() {
                                if !Self::ocw_recently_submitted(
                                    b"ocw-pin",
                                    &cid_hash.encode(),
                                    n,
                                    5u32.into(),
                                ) {
                                    let call = Call::<T>::ocw_mark_pinned {
                                        operator: first_op.clone(),
                                        cid_hash,
                                        replicas: 1,
                                    };
                                    let xt = T::create_bare(call.into());
                                    if frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_ok() {
                                        Self::ocw_set_last_submitted(b"ocw-pin", &cid_hash.encode(), n);
                                    }
                                }
                            }
                            pinning = pinning.saturating_add(1);
                        } else {
                            pinned = pinned.saturating_add(1);
                        }
                    } else {
                        // 无分配但状态为 pinning/pinned，视作缺失
                        missing = missing.saturating_add(1);
                    }
                }
            }
            // 事件上报（轻量只读）：不改变状态，仅供监控
            if sample > 0 {
                Self::deposit_event(Event::PinProbe(sample, pinning, pinned, missing));
            }

            // ============================================================================
            // 过期CID物理删除（OCW调用IPFS unpin）
            // ============================================================================
            //
            // 扫描已过期的CID（PinBilling.state=2），调用IPFS Cluster API执行物理删除
            // 删除成功后清理链上存储

            // ⭐ P2: 物理删除仅调用IPFS API，链上清理由 on_finalize 任务3.5 负责
            let mut unpinned_count = 0u32;
            const MAX_UNPIN_PER_BLOCK: u32 = 5;

            for (cid_hash, (_, _, state)) in PinBilling::<T>::iter() {
                if state == 2u8 && unpinned_count < MAX_UNPIN_PER_BLOCK {
                    let cid_str = Self::resolve_cid(&cid_hash);
                    // 仅调用 IPFS Cluster API 物理 unpin（链上清理不在 OCW 中做）
                    let _ = Self::submit_delete_pin(&endpoint, &token, &cid_str);
                    unpinned_count += 1;
                }
            }

            // ============================================================================
            // 公共IPFS网络简化健康检查（无隐私约束版本）
            // ============================================================================

            // 从本地存储读取节点账户（用于识别本节点）
            // TODO: 实现从OCW本地存储读取节点账户的逻辑
            // 暂时跳过简化健康检查（需要节点账户配置）
            let local_node_account_bytes = sp_io::offchain::local_storage_get(
                StorageKind::PERSISTENT,
                b"/memo/ipfs/node_account",
            );

            // 节点账户未配置，跳过简化健康检查
            let local_node_account_bytes = match local_node_account_bytes {
                Some(bytes) => bytes,
                None => return,
            };

            let local_node_account = match T::AccountId::decode(&mut &local_node_account_bytes[..])
            {
                Ok(account) => account,
                Err(_) => return, // 解码失败，跳过
            };

            // 获取分配给本节点的CID列表（限制每次检查10个，避免阻塞）
            let my_cids = Self::get_my_assigned_cids(&local_node_account, 10);

            // 遍历检查每个CID的PIN状态
            for (cid_hash, plaintext_cid) in my_cids.iter() {
                // 调用本地IPFS HTTP API检查Pin状态
                match Self::check_ipfs_pin(plaintext_cid) {
                    Ok(true) => {
                        // Pin存在且健康，上报成功状态
                        Self::deposit_event(Event::SimplePinStatusReported {
                            cid_hash: *cid_hash,
                            node: local_node_account.clone(),
                            status: SimplePinStatus::Pinned,
                        });
                    }
                    Ok(false) => {
                        // Pin不存在，尝试重新Pin
                        if let Ok(()) = Self::pin_to_local_ipfs(plaintext_cid) {
                            // 重新Pin成功
                            Self::deposit_event(Event::SimplePinStatusReported {
                                cid_hash: *cid_hash,
                                node: local_node_account.clone(),
                                status: SimplePinStatus::Restored,
                            });
                        } else {
                            // 重新Pin失败
                            Self::deposit_event(Event::SimplePinStatusReported {
                                cid_hash: *cid_hash,
                                node: local_node_account.clone(),
                                status: SimplePinStatus::Failed,
                            });
                        }
                    }
                    Err(_) => {
                        // HTTP请求失败，上报失败状态
                        Self::deposit_event(Event::SimplePinStatusReported {
                            cid_hash: *cid_hash,
                            node: local_node_account.clone(),
                            status: SimplePinStatus::Failed,
                        });
                    }
                }

                // 检查节点负载，发出告警
                let capacity_usage = Self::calculate_simple_capacity_usage(&local_node_account);
                if capacity_usage > 80 {
                    let stats = OperatorPinStats::<T>::get(&local_node_account);
                    Self::deposit_event(Event::SimpleNodeLoadWarning {
                        node: local_node_account.clone(),
                        capacity_usage: capacity_usage as u8,
                        current_pins: stats.total_pins,
                    });
                }
            }
        }

        /// 函数级详细中文注释：区块结束时的自动化任务（优化改造）
        ///
        /// 执行顺序（优先级从高到低）：
        /// 1. 周期扣费（确保资金流转）
        /// 2. 健康巡检（确保数据安全）
        /// 3. 统计更新（链上仪表板）
        ///
        /// 限流保护：
        /// - 每块最多处理20个扣费任务
        /// - 每块最多处理10个巡检任务
        /// - 防止区块拥堵
        fn on_finalize(_n: BlockNumberFor<T>) {
            // All work migrated to on_idle with weight accounting.
        }

        fn on_idle(n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let current_block = n;

            let w_overhead = Weight::from_parts(10_000, 500);
            let w_billing = Weight::from_parts(200_000, 4_000);
            let w_health = Weight::from_parts(150_000, 3_000);
            let w_unreg = Weight::from_parts(100_000, 2_000);
            let w_cleanup = Weight::from_parts(120_000, 2_500);
            let w_domain = Weight::from_parts(500_000, 10_000);
            let w_orphan = Weight::from_parts(50_000, 1_000);

            let mut budget = remaining_weight;
            let mut consumed = w_overhead;

            if !budget.all_gte(w_overhead) {
                return Weight::zero();
            }
            budget = budget.saturating_sub(w_overhead);

            // ======== Task 1: Billing settlement (cursor-paged) ========
            let billing_paused = BillingPaused::<T>::get();
            if !billing_paused && budget.all_gte(w_billing) {
                let billing_cursor = BillingSettleCursor::<T>::get();
                let scan_start = if billing_cursor == Zero::zero() {
                    Zero::zero()
                } else {
                    billing_cursor.saturating_add(1u32.into())
                };

                let mut tasks_to_process: alloc::vec::Vec<(
                    BlockNumberFor<T>,
                    T::Hash,
                    BillingTask<BlockNumberFor<T>, BalanceOf<T>>,
                )> = alloc::vec::Vec::new();

                let mut scan_block = scan_start;
                let mut last_fully_scanned = billing_cursor;
                'billing_scan: while scan_block <= current_block {
                    for (cid_hash, task) in BillingQueue::<T>::iter_prefix(scan_block) {
                        if budget.all_gte(w_billing) {
                            tasks_to_process.push((scan_block, cid_hash, task));
                            budget = budget.saturating_sub(w_billing);
                            consumed = consumed.saturating_add(w_billing);
                        } else {
                            break 'billing_scan;
                        }
                    }
                    last_fully_scanned = scan_block;
                    scan_block = scan_block.saturating_add(1u32.into());
                }
                BillingSettleCursor::<T>::put(last_fully_scanned);

                for (due_block, cid_hash, mut task) in tasks_to_process {
                    match Self::four_layer_charge(&cid_hash, &mut task) {
                        Ok(ChargeResult::Success { layer }) => {
                            let next_billing =
                                current_block.saturating_add(task.billing_period.into());
                            task.last_charge = current_block;
                            task.charge_layer = layer;
                            task.grace_status = GraceStatus::Normal;
                            BillingQueue::<T>::insert(next_billing, &cid_hash, task);
                            BillingQueue::<T>::remove(due_block, &cid_hash);
                            CidBillingDueBlock::<T>::insert(&cid_hash, next_billing);
                        }
                        Ok(ChargeResult::EnterGrace { expires_at }) => {
                            let retry_count = match &task.grace_status {
                                GraceStatus::InGrace { retry_count, .. } => {
                                    retry_count.saturating_add(1)
                                }
                                _ => 0u32,
                            };
                            task.grace_status = GraceStatus::InGrace {
                                entered_at: current_block,
                                expires_at,
                                retry_count,
                            };
                            let shift = core::cmp::min(retry_count, 5);
                            let backoff =
                                core::cmp::min(1200u32.saturating_mul(1u32 << shift), 28800u32);
                            let next_billing = current_block.saturating_add(backoff.into());
                            BillingQueue::<T>::insert(next_billing, &cid_hash, task);
                            BillingQueue::<T>::remove(due_block, &cid_hash);
                            CidBillingDueBlock::<T>::insert(&cid_hash, next_billing);

                            Self::deposit_event(Event::GracePeriodStarted {
                                cid_hash: cid_hash.clone(),
                                expires_at,
                            });
                        }
                        Err(_) => {
                            BillingQueue::<T>::remove(due_block, &cid_hash);
                            CidBillingDueBlock::<T>::remove(&cid_hash);

                            if let Some((_, unit_price, _)) = PinBilling::<T>::get(&cid_hash) {
                                PinBilling::<T>::insert(
                                    &cid_hash,
                                    (current_block, unit_price, 2u8),
                                );
                                ExpiredCidPending::<T>::put(true);
                                ExpiredCidQueue::<T>::mutate(|q| {
                                    let _ = q.try_push(cid_hash.clone());
                                });
                            }

                            Self::deposit_event(Event::MarkedForUnpin {
                                cid_hash: cid_hash.clone(),
                                reason: UnpinReason::InsufficientFunds,
                            });
                        }
                    }
                }
            }

            // ======== Task 2: Health-check settlement (cursor-paged) ========
            if budget.all_gte(w_health) {
                let hc_cursor = HealthCheckSettleCursor::<T>::get();
                let hc_start = if hc_cursor == Zero::zero() {
                    Zero::zero()
                } else {
                    hc_cursor.saturating_add(1u32.into())
                };

                let mut checks_to_process: alloc::vec::Vec<(
                    BlockNumberFor<T>,
                    T::Hash,
                    HealthCheckTask<BlockNumberFor<T>>,
                )> = alloc::vec::Vec::new();

                let mut hc_scan = hc_start;
                let mut hc_last_fully = hc_cursor;
                'hc_scan: while hc_scan <= current_block {
                    for (cid_hash, task) in HealthCheckQueue::<T>::iter_prefix(hc_scan) {
                        if budget.all_gte(w_health) {
                            checks_to_process.push((hc_scan, cid_hash, task));
                            budget = budget.saturating_sub(w_health);
                            consumed = consumed.saturating_add(w_health);
                        } else {
                            break 'hc_scan;
                        }
                    }
                    hc_last_fully = hc_scan;
                    hc_scan = hc_scan.saturating_add(1u32.into());
                }
                HealthCheckSettleCursor::<T>::put(hc_last_fully);

                for (check_block, cid_hash, mut task) in checks_to_process {
                    if !PinMeta::<T>::contains_key(&cid_hash) {
                        HealthCheckQueue::<T>::remove(check_block, &cid_hash);
                        continue;
                    }

                    let status = Self::check_pin_health(&cid_hash);
                    let tier_config = Self::get_tier_config(&task.tier).unwrap_or_default();

                    match status {
                        HealthStatus::Healthy { .. } => {
                            let next_check = current_block
                                .saturating_add(tier_config.health_check_interval.into());
                            task.last_check = current_block;
                            task.last_status = status.clone();
                            task.consecutive_failures = 0;
                            HealthCheckQueue::<T>::insert(next_check, &cid_hash, task);
                        }
                        HealthStatus::Degraded {
                            current_replicas,
                            target,
                        } => {
                            let urgent_interval = tier_config.health_check_interval / 4;
                            let next_check = current_block.saturating_add(urgent_interval.into());
                            task.last_check = current_block;
                            task.last_status = status.clone();
                            task.consecutive_failures = task.consecutive_failures.saturating_add(1);
                            HealthCheckQueue::<T>::insert(next_check, &cid_hash, task);

                            Self::try_auto_repair(&cid_hash, current_replicas, target);

                            Self::deposit_event(Event::HealthDegraded {
                                cid_hash: cid_hash.clone(),
                                current_replicas,
                                target,
                            });
                        }
                        HealthStatus::Critical { current_replicas } => {
                            let critical_interval = 1200u32;
                            let next_check = current_block.saturating_add(critical_interval.into());
                            task.last_check = current_block;
                            task.last_status = status.clone();
                            task.consecutive_failures = task.consecutive_failures.saturating_add(1);
                            HealthCheckQueue::<T>::insert(next_check, &cid_hash, task);

                            let target_replicas = PinMeta::<T>::get(&cid_hash)
                                .map(|m| m.replicas)
                                .unwrap_or(2);
                            Self::try_auto_repair(&cid_hash, current_replicas, target_replicas);

                            Self::deposit_event(Event::HealthCritical {
                                cid_hash: cid_hash.clone(),
                                current_replicas,
                            });
                        }
                        HealthStatus::Unknown => {
                            let retry_interval = 600u32;
                            let next_check = current_block.saturating_add(retry_interval.into());
                            task.last_check = current_block;
                            task.last_status = status.clone();
                            task.consecutive_failures = task.consecutive_failures.saturating_add(1);

                            if task.consecutive_failures >= 5 {
                                Self::deposit_event(Event::HealthCheckFailed {
                                    cid_hash: cid_hash.clone(),
                                    failures: task.consecutive_failures,
                                });
                            }

                            HealthCheckQueue::<T>::insert(next_check, &cid_hash, task);
                        }
                    }

                    HealthCheckQueue::<T>::remove(check_block, &cid_hash);
                }
            }

            // ======== Task 3: Expired operator grace-period processing ========
            if budget.all_gte(w_unreg) {
                let mut expired_ops: alloc::vec::Vec<T::AccountId> = alloc::vec::Vec::new();
                for (operator, expires_at) in PendingUnregistrations::<T>::iter() {
                    if !budget.all_gte(w_unreg) {
                        break;
                    }
                    if expires_at <= current_block {
                        expired_ops.push(operator);
                        budget = budget.saturating_sub(w_unreg);
                        consumed = consumed.saturating_add(w_unreg);
                    }
                }
                for operator in expired_ops {
                    let remaining = Self::count_operator_pins(&operator);
                    if remaining == 0 {
                        let _ = Self::finalize_operator_unregistration(&operator);
                    }
                    PendingUnregistrations::<T>::remove(&operator);
                }
            }

            // ======== Task 3.5: Expired CID on-chain storage cleanup ========
            if ExpiredCidPending::<T>::get() && budget.all_gte(w_cleanup) {
                let max_cleanup = budget
                    .ref_time()
                    .checked_div(w_cleanup.ref_time())
                    .unwrap_or(0) as u32;
                let mut cleaned = 0u32;
                let mut to_clean: alloc::vec::Vec<T::Hash> = alloc::vec::Vec::new();

                ExpiredCidQueue::<T>::mutate(|queue| {
                    while cleaned < max_cleanup && !queue.is_empty() {
                        to_clean.push(queue.remove(0));
                        cleaned += 1;
                    }
                });

                let cleanup_cost = w_cleanup.saturating_mul(cleaned as u64);
                budget = budget.saturating_sub(cleanup_cost);
                consumed = consumed.saturating_add(cleanup_cost);

                for cid_hash in to_clean.iter() {
                    Self::do_cleanup_single_cid(cid_hash);
                }

                if ExpiredCidQueue::<T>::get().is_empty() {
                    ExpiredCidPending::<T>::put(false);
                }
            }

            // ======== Task 4: Domain stats update (every ~24h) ========
            if current_block % 7200u32.into() == Zero::zero() && budget.all_gte(w_domain) {
                Self::update_domain_health_stats_impl();
                budget = budget.saturating_sub(w_domain);
                consumed = consumed.saturating_add(w_domain);
            }

            // ======== Task 5: Orphan CID sweep ========
            if budget.all_gte(w_orphan) {
                let max_orphan_items = budget
                    .ref_time()
                    .checked_div(w_orphan.ref_time())
                    .unwrap_or(0)
                    .min(10) as u32;
                if max_orphan_items > 0 {
                    let swept = Self::sweep_orphan_cids(max_orphan_items);
                    let orphan_cost = w_orphan.saturating_mul(swept as u64);
                    consumed = consumed.saturating_add(orphan_cost);
                }
            }

            consumed
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(source: TransactionSource, call: &Call<T>) -> TransactionValidity {
            // 仅接受本地 OCW 提交的交易
            match source {
                TransactionSource::Local | TransactionSource::InBlock => {}
                _ => return InvalidTransaction::Call.into(),
            }

            match call {
                Call::ocw_mark_pinned {
                    cid_hash, operator, ..
                } => {
                    // 基本有效性：CID 在 PendingPins 中
                    if !PendingPins::<T>::contains_key(cid_hash) {
                        return InvalidTransaction::Stale.into();
                    }
                    // 运营者必须存在且Active
                    if let Some(info) = Operators::<T>::get(operator) {
                        if info.status != 0 {
                            return InvalidTransaction::BadSigner.into();
                        }
                    } else {
                        return InvalidTransaction::BadSigner.into();
                    }
                    // ✅ P1-18：含区块号的防重放
                    let bn = <frame_system::Pallet<T>>::block_number();
                    ValidTransaction::with_tag_prefix("storage-ocw-pin")
                        .priority(15)
                        .and_provides((b"pin", cid_hash, operator, bn))
                        .longevity(5)
                        .propagate(true)
                        .build()
                }
                Call::ocw_mark_pin_failed {
                    cid_hash, operator, ..
                } => {
                    if !PendingPins::<T>::contains_key(cid_hash) {
                        return InvalidTransaction::Stale.into();
                    }
                    if let Some(info) = Operators::<T>::get(operator) {
                        if info.status != 0 {
                            return InvalidTransaction::BadSigner.into();
                        }
                    } else {
                        return InvalidTransaction::BadSigner.into();
                    }
                    // ✅ P1-18：含区块号的防重放
                    let bn = <frame_system::Pallet<T>>::block_number();
                    ValidTransaction::with_tag_prefix("storage-ocw-fail")
                        .priority(15)
                        .and_provides((b"fail", cid_hash, operator, bn))
                        .longevity(5)
                        .propagate(true)
                        .build()
                }
                Call::ocw_submit_assignments { cid_hash, .. } => {
                    if !PendingPins::<T>::contains_key(cid_hash) {
                        return InvalidTransaction::Stale.into();
                    }
                    // 防止重复分配
                    if LayeredPinAssignments::<T>::get(cid_hash).is_some() {
                        return InvalidTransaction::Stale.into();
                    }
                    // ✅ P1-18：含区块号的防重放
                    let bn = <frame_system::Pallet<T>>::block_number();
                    ValidTransaction::with_tag_prefix("storage-ocw-assign")
                        .priority(20)
                        .and_provides((b"assign", cid_hash, bn))
                        .longevity(5)
                        .propagate(true)
                        .build()
                }
                Call::ocw_report_health {
                    cid_hash, operator, ..
                } => {
                    // ✅ P1-18：验证运营者存在且Active
                    if let Some(info) = Operators::<T>::get(operator) {
                        if info.status != 0 {
                            return InvalidTransaction::BadSigner.into();
                        }
                    } else {
                        return InvalidTransaction::BadSigner.into();
                    }
                    // CID必须存在
                    if !PinMeta::<T>::contains_key(cid_hash) {
                        return InvalidTransaction::Stale.into();
                    }
                    // H2-R3：验证运营者已被分配到该CID
                    if let Some(assign) = PinAssignments::<T>::get(cid_hash) {
                        if !assign.iter().any(|a| a == operator) {
                            return InvalidTransaction::BadSigner.into();
                        }
                    } else {
                        return InvalidTransaction::Stale.into();
                    }
                    // ✅ P1-18：含区块号的防重放
                    let bn = <frame_system::Pallet<T>>::block_number();
                    ValidTransaction::with_tag_prefix("storage-ocw-health")
                        .priority(10)
                        .and_provides((b"health", cid_hash, operator, bn))
                        .longevity(3)
                        .propagate(true)
                        .build()
                }
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    /// 函数级详细中文注释：域级健康统计（替代全局统计）
    ///
    /// ⭐ 优化说明：
    /// - 旧版本：update_global_health_stats_impl() 全量扫描所有Pin
    /// - 新版本：update_domain_health_stats_impl() 按域扫描并自动汇总
    ///
    /// 优势：
    /// 1. 性能优化：使用 iter_prefix 减少扫描范围
    /// 2. 可观测性：提供域级别的细粒度统计
    /// 3. 优先级调度：按域优先级顺序处理
    /// 4. 自动汇总：域统计完成后自动更新全局统计
    impl<T: Config> Pallet<T> {
        /// 函数级详细中文注释：按域统计Pin健康状态（OCW调用）
        ///
        /// ### 功能
        /// - 按优先级顺序遍历各域
        /// - 统计每个域的Pin数量、存储容量、健康状态
        /// - 更新域级统计数据
        /// - 发送域统计更新事件
        ///
        /// ### 性能优化
        /// - 使用 iter_prefix 只遍历特定域的CID
        /// - 批量限制：每域最多处理1000个CID
        /// - 自动跳过空域
        ///
        /// ### 调用时机
        /// - OCW中每24小时执行一次（与全局统计同步）
        fn update_domain_health_stats_impl() {
            let current_block = <frame_system::Pallet<T>>::block_number();

            let registered = RegisteredDomainList::<T>::get();
            let mut domains_with_priority: Vec<(BoundedVec<u8, ConstU32<32>>, u8)> = registered
                .into_iter()
                .map(|d| {
                    let p = DomainPriority::<T>::get(&d);
                    (d, p)
                })
                .collect();
            domains_with_priority.sort_by_key(|(_domain, priority)| *priority);

            // 3. 按域顺序统计
            for (domain, _priority) in domains_with_priority.iter() {
                let mut domain_stats = DomainStats {
                    domain: domain.clone(),
                    total_pins: 0,
                    total_size_bytes: 0,
                    healthy_count: 0,
                    degraded_count: 0,
                    critical_count: 0,
                };

                let mut cid_count = 0u32;
                const MAX_CIDS: u32 = 1000; // 批量限制

                // ⭐ 使用前缀迭代器高效遍历该域的CID
                for (cid_hash, _) in DomainPins::<T>::iter_prefix(domain) {
                    if cid_count >= MAX_CIDS {
                        break; // 限制处理数量
                    }

                    domain_stats.total_pins += 1;

                    // 获取Pin元信息
                    if let Some(meta) = PinMeta::<T>::get(&cid_hash) {
                        domain_stats.total_size_bytes += meta.size;
                    }

                    // M4-R2修复：使用 O(1) 直接查询替代 O(N) 全表扫描
                    // 通过 PinAssignments + PinSuccess 计算实际在线副本数
                    if let Some(assignments) = PinAssignments::<T>::get(&cid_hash) {
                        let target = PinMeta::<T>::get(&cid_hash)
                            .map(|m| m.replicas)
                            .unwrap_or(assignments.len() as u32);
                        let ok_count = assignments
                            .iter()
                            .filter(|op| PinSuccess::<T>::get(&cid_hash, op))
                            .count() as u32;

                        if ok_count >= target {
                            domain_stats.healthy_count += 1;
                        } else if ok_count >= 1 {
                            domain_stats.degraded_count += 1;
                        } else {
                            domain_stats.critical_count += 1;
                        }
                    } else {
                        // 无分配记录，默认为健康（新创建的CID）
                        domain_stats.healthy_count += 1;
                    }

                    cid_count += 1;
                }

                // 4. 存储统计结果
                DomainHealthStats::<T>::insert(domain, domain_stats.clone());

                // 5. 发送事件
                Self::deposit_event(Event::DomainStatsUpdated {
                    domain: domain.clone(),
                    total_pins: domain_stats.total_pins,
                    total_size_bytes: domain_stats.total_size_bytes,
                    healthy_count: domain_stats.healthy_count,
                    degraded_count: domain_stats.degraded_count,
                    critical_count: domain_stats.critical_count,
                });
            }

            let mut global_stats = GlobalHealthStats::<BlockNumberFor<T>>::default();
            for (domain, _) in domains_with_priority.iter() {
                if let Some(stats) = DomainHealthStats::<T>::get(domain) {
                    global_stats.total_pins += stats.total_pins;
                    global_stats.total_size_bytes += stats.total_size_bytes;
                    global_stats.healthy_count += stats.healthy_count;
                    global_stats.degraded_count += stats.degraded_count;
                    global_stats.critical_count += stats.critical_count;
                }
            }
            global_stats.last_full_scan = current_block;
            HealthCheckStats::<T>::put(global_stats);
        }

        /// 函数级详细中文注释：查询域统计（RPC接口）
        ///
        /// ### 功能
        /// - 查询指定域的统计信息
        /// - 返回Pin数量、存储容量、健康状态
        ///
        /// ### 参数
        /// - `domain`：域名（如 b"subject"）
        ///
        /// ### 返回
        /// - `Option<DomainStats>`：域统计信息，如果域不存在返回None
        ///
        /// ### 使用场景
        /// - Dashboard查询域统计
        /// - 监控系统获取域状态
        pub fn get_domain_stats(domain: Vec<u8>) -> Option<DomainStats> {
            if let Ok(bounded_domain) = BoundedVec::try_from(domain) {
                DomainHealthStats::<T>::get(&bounded_domain)
            } else {
                None
            }
        }

        /// 函数级详细中文注释：查询所有域统计（RPC接口）
        ///
        /// ### 功能
        /// - 查询所有已注册域的统计信息
        /// - 按优先级排序返回
        ///
        /// ### 返回
        /// - `Vec<(Vec<u8>, DomainStats, u8)>`：域列表
        ///   - 域名
        ///   - 域统计
        ///   - 优先级
        ///
        /// ### 使用场景
        /// - Dashboard展示所有域的统计
        /// - 监控系统全局视图
        pub fn get_all_domain_stats() -> Vec<(Vec<u8>, DomainStats, u8)> {
            let mut result = Vec::new();

            for (domain, stats) in DomainHealthStats::<T>::iter() {
                let priority = DomainPriority::<T>::get(&domain);
                result.push((domain.to_vec(), stats, priority));
            }

            // 按优先级排序（优先级越小越靠前）
            result.sort_by_key(|(_, _, priority)| *priority);

            result
        }

        /// 函数级详细中文注释：查询域的CID列表（RPC接口，分页）
        ///
        /// ### 功能
        /// - 查询指定域的CID列表（分页）
        /// - 返回CID及其元数据
        ///
        /// ### 参数
        /// - `domain`：域名
        /// - `offset`：分页偏移量
        /// - `limit`：每页数量（最大100）
        ///
        /// ### 返回
        /// - `Vec<(T::Hash, PinMetadata<BlockNumberFor<T>>)>`：CID列表
        ///   - CID的hash
        ///   - Pin元数据
        ///
        /// ### 使用场景
        /// - Dashboard查看域的详细CID列表
        /// - 调试和诊断
        pub fn get_domain_cids(
            domain: Vec<u8>,
            offset: u32,
            limit: u32,
        ) -> Vec<(T::Hash, PinMetadata<BlockNumberFor<T>>)> {
            let limit = limit.min(100); // 限制最大100条
            let mut result = Vec::new();

            if let Ok(bounded_domain) = BoundedVec::try_from(domain) {
                let mut count = 0u32;
                let mut skipped = 0u32;

                for (cid_hash, _) in DomainPins::<T>::iter_prefix(&bounded_domain) {
                    // 跳过offset之前的记录
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }

                    // 达到limit后停止
                    if count >= limit {
                        break;
                    }

                    // 获取元数据
                    if let Some(meta) = PinMeta::<T>::get(&cid_hash) {
                        result.push((cid_hash, meta));
                        count += 1;
                    }
                }
            }

            result
        }
    }

    // [已删除] due_at_count(), due_between(), enqueue_due()：
    // DueQueue 已删除，计费完全由 BillingQueue + on_finalize 管理。

    impl<T: Config> Pallet<T> {
        /// 将 CID 标记为待删除，并停止后续链上计费调度。
        pub(crate) fn mark_cid_for_unpin(cid_hash: &T::Hash, reason: UnpinReason) {
            if reason != UnpinReason::GovernanceDecision && Self::is_locked(cid_hash) {
                return;
            }

            let current_block = <frame_system::Pallet<T>>::block_number();
            if let Some((_next_due, unit_price, _)) = PinBilling::<T>::get(cid_hash) {
                PinBilling::<T>::insert(cid_hash, (current_block, unit_price, 2u8));
            }

            ExpiredCidPending::<T>::put(true);
            ExpiredCidQueue::<T>::mutate(|q| {
                let _ = q.try_push(*cid_hash);
            });
            CidUnpinReason::<T>::insert(cid_hash, reason.clone());

            if let Some(due_block) = CidBillingDueBlock::<T>::take(cid_hash) {
                BillingQueue::<T>::remove(due_block, cid_hash);
            }

            Self::deposit_event(Event::MarkedForUnpin {
                cid_hash: *cid_hash,
                reason,
            });
        }

        /// 扫描 PinMeta 找出 PinSubjectOf 缺失的孤儿 CID，标记 unpin。
        /// 使用 `OrphanSweepCursor` 实现跨块分页，每次处理最多 `limit` 条。
        /// 返回实际处理条目数。
        pub(crate) fn sweep_orphan_cids(limit: u32) -> u32 {
            let cursor = OrphanSweepCursor::<T>::get();
            let mut iter = match cursor {
                Some(ref raw) => PinMeta::<T>::iter_from(raw.to_vec()),
                None => PinMeta::<T>::iter(),
            };

            let mut processed: u32 = 0;
            let mut last_raw_key: Option<alloc::vec::Vec<u8>> = None;

            while processed < limit {
                match iter.next() {
                    Some((cid_hash, _meta)) => {
                        last_raw_key = Some(PinMeta::<T>::hashed_key_for(&cid_hash));
                        processed += 1;

                        if PinSubjectOf::<T>::contains_key(&cid_hash) {
                            continue;
                        }
                        // PinBilling state == 2 表示已在清理队列中，跳过
                        if let Some((_, _, state)) = PinBilling::<T>::get(&cid_hash) {
                            if state == 2u8 {
                                continue;
                            }
                        }
                        Self::mark_cid_for_unpin(&cid_hash, UnpinReason::ManualRequest);
                        Self::deposit_event(Event::OrphanCidDetected { cid_hash });
                    }
                    None => {
                        OrphanSweepCursor::<T>::kill();
                        return processed;
                    }
                }
            }

            match last_raw_key {
                Some(key) => {
                    if let Ok(bounded) = BoundedVec::try_from(key) {
                        OrphanSweepCursor::<T>::put(bounded);
                    }
                }
                None => {
                    OrphanSweepCursor::<T>::kill();
                }
            }
            processed
        }

        /// 退款逻辑：基于 BillingQueue 实际剩余预付时间按比例退款。
        /// 优先使用 CidBillingDueBlock + BillingQueue 精确计算，兼容 PendingPins 旧路径。
        fn try_refund_unpin(cid_hash: &T::Hash, owner: &T::AccountId) {
            let current_block = <frame_system::Pallet<T>>::block_number();
            let pool = T::IpfsPoolAccount::get();

            // 优先路径：基于 BillingQueue 实际计费状态计算退款
            let mut refunded = false;
            if let Some(due_block) = CidBillingDueBlock::<T>::get(cid_hash) {
                if due_block > current_block {
                    if let Some(task) = BillingQueue::<T>::get(due_block, cid_hash) {
                        let remaining_blocks: u128 =
                            due_block.saturating_sub(current_block).saturated_into();
                        let period_blocks: u128 = task.billing_period as u128;
                        let fee_u128: u128 = task.amount_per_period.saturated_into();

                        if period_blocks > 0 {
                            let refund_u128 =
                                remaining_blocks.saturating_mul(fee_u128) / period_blocks;
                            let refund: BalanceOf<T> = refund_u128.saturated_into();

                            if !refund.is_zero() {
                                let pool_balance = <T as Config>::Currency::free_balance(&pool);
                                let actual_refund = refund.min(pool_balance);
                                if !actual_refund.is_zero() {
                                    match <T as Config>::Currency::transfer(
                                        &pool,
                                        owner,
                                        actual_refund,
                                        frame_support::traits::ExistenceRequirement::KeepAlive,
                                    ) {
                                        Ok(_) => {
                                            Self::deposit_event(Event::UnpinRefund {
                                                cid_hash: *cid_hash,
                                                owner: owner.clone(),
                                                refund: actual_refund,
                                            });
                                        }
                                        Err(_) => {
                                            Self::deposit_event(Event::RefundFailed {
                                                owner: owner.clone(),
                                                amount: actual_refund,
                                            });
                                        }
                                    }
                                    refunded = true;
                                }
                            }
                        }
                    }
                }
            }

            // 兼容旧路径：PendingPins 初始押金退款
            if !refunded {
                let meta = match PinMeta::<T>::get(cid_hash) {
                    Some(m) => m,
                    None => return,
                };
                let elapsed = current_block.saturating_sub(meta.created_at);
                let initial_period_blocks: BlockNumberFor<T> = 403200u32.into();

                if let Some((_payer, _replicas, _subject_id, _size, deposit)) =
                    PendingPins::<T>::get(cid_hash)
                {
                    if elapsed < initial_period_blocks && !deposit.is_zero() {
                        let elapsed_u128: u128 = elapsed.saturated_into();
                        let total_u128: u128 = initial_period_blocks.saturated_into();
                        let deposit_u128: u128 = deposit.saturated_into();

                        let used = deposit_u128.saturating_mul(elapsed_u128) / total_u128;
                        let refund_u128 = deposit_u128.saturating_sub(used);
                        let refund: BalanceOf<T> = refund_u128.saturated_into();

                        if !refund.is_zero() {
                            let pool_balance = <T as Config>::Currency::free_balance(&pool);
                            let actual_refund = refund.min(pool_balance);
                            if !actual_refund.is_zero() {
                                match <T as Config>::Currency::transfer(
                                    &pool,
                                    owner,
                                    actual_refund,
                                    frame_support::traits::ExistenceRequirement::KeepAlive,
                                ) {
                                    Ok(_) => {
                                        Self::deposit_event(Event::UnpinRefund {
                                            cid_hash: *cid_hash,
                                            owner: owner.clone(),
                                            refund: actual_refund,
                                        });
                                    }
                                    Err(_) => {
                                        Self::deposit_event(Event::RefundFailed {
                                            owner: owner.clone(),
                                            amount: actual_refund,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        /// M2-R2修复：清理 DomainPins 中指定 CID 的所有条目
        ///
        /// 遍历硬编码域 `b"subject"` 和所有已注册域，移除该 CID 的索引。
        /// 防止过期/删除的 CID 在域统计中持续膨胀。
        /// 清理单个 CID 的所有链上存储（供 cleanup_expired_cids 和 on_finalize 复用）。
        pub fn do_cleanup_single_cid(cid_hash: &T::Hash) {
            let cid_size = PinMeta::<T>::get(cid_hash).map(|m| m.size).unwrap_or(0);

            if let Some(assignments) = PinAssignments::<T>::get(cid_hash) {
                for op in assignments.iter() {
                    OperatorPinCount::<T>::mutate(op, |c| *c = c.saturating_sub(1));
                    OperatorUsedBytes::<T>::mutate(op, |b| *b = b.saturating_sub(cid_size));
                }
            }

            if let Some((owner, _)) = PinSubjectOf::<T>::get(cid_hash) {
                OwnerPinIndex::<T>::mutate(&owner, |cids| {
                    cids.retain(|h| h != cid_hash);
                });
            }

            PinBilling::<T>::remove(cid_hash);
            PinMeta::<T>::remove(cid_hash);
            PinStateOf::<T>::remove(cid_hash);
            PinSubjectOf::<T>::remove(cid_hash);
            CidEntityOf::<T>::remove(cid_hash);
            PinAssignments::<T>::remove(cid_hash);
            CidToSubject::<T>::remove(cid_hash);
            CidTier::<T>::remove(cid_hash);
            CidRegistry::<T>::remove(cid_hash);
            LayeredPinAssignments::<T>::remove(cid_hash);
            SimplePinAssignments::<T>::remove(cid_hash);
            PendingPins::<T>::remove(cid_hash);
            Self::cleanup_domain_pins(cid_hash);
            let _ = PinSuccess::<T>::clear_prefix(cid_hash, u32::MAX, None);
            CidLocks::<T>::remove(cid_hash);
            // 清理 CID 归档索引
            if let Some(archive_id) = CidArchiveReverseIndex::<T>::take(cid_hash) {
                CidArchiveIndex::<T>::remove(archive_id);
            }

            let reason =
                CidUnpinReason::<T>::take(cid_hash).unwrap_or(UnpinReason::InsufficientFunds);
            Self::deposit_event(Event::PinRemoved {
                cid_hash: *cid_hash,
                reason,
            });
        }

        fn cleanup_domain_pins(cid_hash: &T::Hash) {
            if let Ok(subject_domain) =
                BoundedVec::<u8, ConstU32<32>>::try_from(b"subject".to_vec())
            {
                DomainPins::<T>::remove(&subject_domain, cid_hash);
            }
            for domain in RegisteredDomainList::<T>::get().iter() {
                DomainPins::<T>::remove(domain, cid_hash);
            }
        }

        /// 函数级详细中文注释：自动修复副本数不足
        ///
        /// 当健康巡检检测到 Degraded/Critical 状态时调用。
        /// 尝试从活跃运营者中选择新节点补充副本数。
        ///
        /// ### 参数
        /// - `cid_hash`：CID哈希
        /// - `current_replicas`：当前在线副本数
        /// - `target`：目标副本数
        ///
        /// ### 行为
        /// 1. 读取当前已分配的运营者列表
        /// 2. 获取所有活跃运营者，排除已分配的
        /// 3. 选择最优候选节点填补缺口
        /// 4. 更新 PinAssignments 和 OperatorPinCount
        /// 5. 发送 AutoRepairTriggered/AutoRepairCompleted 事件
        pub(crate) fn try_auto_repair(cid_hash: &T::Hash, current_replicas: u32, target: u32) {
            if current_replicas >= target {
                return;
            }

            let needed = target.saturating_sub(current_replicas);

            // 1. 获取当前已分配的运营者
            let current_assignments = PinAssignments::<T>::get(cid_hash).unwrap_or_default();

            // 2. 获取活跃运营者（排除已分配的）
            let active_nodes = match Self::get_active_ipfs_nodes() {
                Ok(nodes) => nodes,
                Err(_) => return, // 无活跃节点，放弃修复
            };

            let candidates: alloc::vec::Vec<T::AccountId> = active_nodes
                .into_iter()
                .filter(|node| !current_assignments.contains(node))
                .collect();

            if candidates.is_empty() {
                return; // 无可用候选节点
            }

            // 3. 发送修复触发事件
            Self::deposit_event(Event::AutoRepairTriggered {
                cid_hash: *cid_hash,
                current_replicas,
                target,
            });

            // 4. 选择最优候选节点
            let size = PinMeta::<T>::get(cid_hash).map(|m| m.size).unwrap_or(0);

            let new_nodes = match Self::select_best_ipfs_nodes(&candidates, needed, size) {
                Ok(nodes) => nodes,
                Err(_) => {
                    // 候选不足，尽力选择可用的
                    let take = (needed as usize).min(candidates.len());
                    match BoundedVec::try_from(candidates[..take].to_vec()) {
                        Ok(v) => v,
                        Err(_) => return,
                    }
                }
            };

            if new_nodes.is_empty() {
                return;
            }

            // 5. 更新 PinAssignments
            let mut updated = current_assignments;
            for node in new_nodes.iter() {
                if updated.try_push(node.clone()).is_err() {
                    break; // BoundedVec 已满
                }
                // 更新 OperatorPinCount + OperatorUsedBytes
                OperatorPinCount::<T>::mutate(node, |c| {
                    *c = c.saturating_add(1);
                });
                // ✅ P1-7
                OperatorUsedBytes::<T>::mutate(node, |b| {
                    *b = b.saturating_add(size);
                });
            }
            PinAssignments::<T>::insert(cid_hash, &updated);

            // 6. 更新 PinMeta 副本数
            PinMeta::<T>::mutate(cid_hash, |meta| {
                if let Some(m) = meta {
                    m.replicas = updated.len() as u32;
                }
            });

            // 7. 发送修复完成事件
            Self::deposit_event(Event::AutoRepairCompleted {
                cid_hash: *cid_hash,
                new_replicas: updated.len() as u32,
            });
        }

        /// 函数级详细中文注释：GET 请求帮助函数，返回主体字节（2xx 才返回）
        fn http_get_bytes(endpoint: &str, token: &Option<String>, path: &str) -> Option<Vec<u8>> {
            let url = alloc::format!("{}{}", endpoint, path);
            let mut req = http::Request::get(&url);
            if let Some(t) = token.as_ref() {
                req = req.add_header("Authorization", &alloc::format!("Bearer {}", t));
            }
            let timeout = sp_io::offchain::timestamp()
                .add(sp_runtime::offchain::Duration::from_millis(3_000));
            let pending = req.deadline(timeout).send().ok()?;
            // try_wait 返回 Result<Option<Response>, _> → ok()?.ok()? 解包为 Response
            let resp = pending.try_wait(timeout).ok()?.ok()?;
            let code: u16 = resp.code;
            if (200..300).contains(&code) {
                Some(resp.body().collect::<Vec<u8>>())
            } else {
                None
            }
        }

        // ========================= OCW 防重复提交辅助函数 =========================

        /// 检查某类 OCW 交易是否在最近 `cooldown` 个区块内已提交过。
        /// 使用 offchain local storage 持久化记录上次提交的区块号。
        fn ocw_recently_submitted(
            prefix: &[u8],
            item_key: &[u8],
            current_block: BlockNumberFor<T>,
            cooldown: BlockNumberFor<T>,
        ) -> bool {
            let mut key = prefix.to_vec();
            key.extend_from_slice(b"::");
            key.extend_from_slice(item_key);
            if let Some(bytes) = sp_io::offchain::local_storage_get(StorageKind::PERSISTENT, &key) {
                if let Ok(last_block) =
                    <BlockNumberFor<T> as codec::Decode>::decode(&mut &bytes[..])
                {
                    return current_block.saturating_sub(last_block) < cooldown;
                }
            }
            false
        }

        /// 记录某类 OCW 交易的最近提交区块号。
        fn ocw_set_last_submitted(
            prefix: &[u8],
            item_key: &[u8],
            current_block: BlockNumberFor<T>,
        ) {
            let mut key = prefix.to_vec();
            key.extend_from_slice(b"::");
            key.extend_from_slice(item_key);
            sp_io::offchain::local_storage_set(
                StorageKind::PERSISTENT,
                &key,
                &codec::Encode::encode(&current_block),
            );
        }

        /// 函数级详细中文注释：通过 OCW 发送 HTTP POST /pins 请求到 ipfs-cluster
        /// - 仅示例：构造最小 JSON 体，包含 `cid` 字段（此处我们只有 `cid_hash`，生产应由 OCW 从密文解出 CID）。
        /// - 返回：若 HTTP 状态为 2xx 则认为提交成功，随后发起 `mark_pinned` 外部交易。
        fn submit_pin_request(
            endpoint: &str,
            token: &Option<String>,
            cid_hash: T::Hash,
        ) -> Result<(), ()> {
            let cid_hex = hex::encode(cid_hash.as_ref());
            // 构造最小 JSON（根据你的 API 需要调整）
            let body_json = alloc::format!(r#"{{"cid":"{}"}}"#, cid_hex);
            let body_vec: Vec<u8> = body_json.into_bytes();
            let url = alloc::format!("{}/pins", endpoint);
            // 不用切片：使用 Vec<Vec<u8>> 作为 POST body，以满足 add_header/deadline 的 T: Default 约束
            let chunks: Vec<Vec<u8>> = alloc::vec![body_vec];
            let mut req = http::Request::post(&url, chunks);
            if let Some(t) = token.as_ref() {
                req = req
                    .add_header("Authorization", &alloc::format!("Bearer {}", t))
                    .add_header("Content-Type", "application/json");
            }
            let timeout = sp_io::offchain::timestamp()
                .add(sp_runtime::offchain::Duration::from_millis(5_000));
            let pending = req.deadline(timeout).send().map_err(|_| ())?;
            let resp = pending.try_wait(timeout).map_err(|_| ())?.map_err(|_| ())?;
            let code: u16 = resp.code;
            if (200..300).contains(&code) {
                Ok(())
            } else {
                Err(())
            }
        }

        /// 函数级详细中文注释：通过 OCW 发送 HTTP DELETE /pins/{cid}
        ///
        /// 功能：
        /// - 调用IPFS Cluster API执行物理unpin操作
        /// - 某些环境下使用 `X-HTTP-Method-Override: DELETE` 搭配 POST 以规避代理限制
        ///
        /// 调用时机：
        /// - OCW扫描到过期CID（PinBilling.state=2）时自动调用
        ///
        /// 返回：
        /// - Ok(())：删除成功（HTTP 2xx）
        /// - Err(())：删除失败（网络错误或HTTP非2xx）
        fn submit_delete_pin(
            endpoint: &str,
            token: &Option<String>,
            cid_str: &str,
        ) -> Result<(), ()> {
            let url = alloc::format!("{}/pins/{}", endpoint, cid_str);
            // 不用切片：空体使用 Vec<Vec<u8>>
            let chunks: Vec<Vec<u8>> = alloc::vec![Vec::new()];
            let mut req =
                http::Request::post(&url, chunks).add_header("X-HTTP-Method-Override", "DELETE");
            if let Some(t) = token.as_ref() {
                req = req.add_header("Authorization", &alloc::format!("Bearer {}", t));
            }
            let timeout = sp_io::offchain::timestamp()
                .add(sp_runtime::offchain::Duration::from_millis(5_000));
            let pending = req.deadline(timeout).send().map_err(|_| ())?;
            let resp = pending.try_wait(timeout).map_err(|_| ())?.map_err(|_| ())?;
            let code: u16 = resp.code;
            if (200..300).contains(&code) {
                Ok(())
            } else {
                Err(())
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// 函数级中文注释：只读接口——根据运营者账户派生对应的押金保留账户地址。
        pub fn operator_bond_account(operator: &T::AccountId) -> T::AccountId {
            T::SubjectPalletId::get()
                .try_into_sub_account((b"bond", operator))
                .expect("pallet sub-account derivation should not fail")
        }

        /// 函数级中文注释：只读接口——根据 subject_id 派生其资金账户地址。
        pub fn subject_account(subject_id: u64) -> T::AccountId {
            // 使用 General 类型的域编码（2）作为默认
            Self::subject_account_for(2u8, subject_id)
        }
    }
}

// 函数级中文注释：将 pallet 模块内导出的类型（如 Pallet、Call、Event 等）在 crate 根进行再导出
// 作用：
// 1) 让 runtime 集成宏（#[frame_support::runtime]）能够找到 `tt_default_parts_v2` 等默认部件；
// 2) 便于上层以 `pallet_storage_service::Call` 等简洁路径引用类型，降低路径耦合。
pub use pallet::*;

/// StoragePin trait 实现：domain 字符串自动映射到 SubjectType 和 DomainPins。
impl<T: Config> StoragePin<<T as frame_system::Config>::AccountId> for Pallet<T> {
    fn pin(
        owner: <T as frame_system::Config>::AccountId,
        domain: &[u8],
        subject_id: u64,
        entity_id: Option<u64>,
        cid: Vec<u8>,
        size_bytes: u64,
        tier: PinTier,
    ) -> DispatchResult {
        let subject_type = SubjectType::from_domain(domain);
        Self::do_request_pin(
            owner,
            subject_type,
            subject_id,
            entity_id,
            cid,
            size_bytes,
            Some(tier),
        )
    }

    fn unpin(owner: <T as frame_system::Config>::AccountId, cid: Vec<u8>) -> DispatchResult {
        use sp_runtime::traits::Hash;
        let cid_hash = T::Hashing::hash(&cid[..]);

        if !PinMeta::<T>::contains_key(&cid_hash) {
            return Ok(());
        }

        if let Some((recorded_owner, _)) = PinSubjectOf::<T>::get(&cid_hash) {
            ensure!(owner == recorded_owner, Error::<T>::NotOwner);
        } else {
            return Err(Error::<T>::NotOwner.into());
        }

        Self::mark_cid_for_unpin(&cid_hash, UnpinReason::ManualRequest);
        Ok(())
    }
}

// ⭐ P1优化：已删除 old_pin_cid_for_subject() 函数（68行）
// 原因：已被 request_pin_for_subject() extrinsic的破坏式改造替代
// 该函数使用了已删除的 triple_charge_storage_fee()
// 删除日期：2025-10-26

/// CidLockManager trait 实现 - 证据锁定机制 ✅ P2-19实现
impl<T: Config> CidLockManager<T::Hash, BlockNumberFor<T>> for Pallet<T> {
    fn lock_cid(
        cid_hash: T::Hash,
        reason: Vec<u8>,
        until: Option<BlockNumberFor<T>>,
    ) -> DispatchResult {
        // 已锁定则拒绝重复锁定
        ensure!(
            !CidLocks::<T>::contains_key(&cid_hash),
            Error::<T>::BadParams
        );
        // CID必须存在
        ensure!(
            PinMeta::<T>::contains_key(&cid_hash),
            Error::<T>::OrderNotFound
        );

        let bounded_reason: BoundedVec<u8, ConstU32<128>> =
            reason.try_into().map_err(|_| Error::<T>::BadParams)?;
        CidLocks::<T>::insert(&cid_hash, (bounded_reason, until));

        Self::deposit_event(Event::CidLocked { cid_hash, until });
        Ok(())
    }

    fn unlock_cid(cid_hash: T::Hash, _reason: Vec<u8>) -> DispatchResult {
        ensure!(
            CidLocks::<T>::contains_key(&cid_hash),
            Error::<T>::OrderNotFound
        );
        CidLocks::<T>::remove(&cid_hash);

        Self::deposit_event(Event::CidUnlocked { cid_hash });
        Ok(())
    }

    fn is_locked(cid_hash: &T::Hash) -> bool {
        if let Some((_, until)) = CidLocks::<T>::get(cid_hash) {
            if let Some(expiry) = until {
                // L4修复：纯读取，不修改存储；过期锁视为未锁定
                let now = <frame_system::Pallet<T>>::block_number();
                if now > expiry {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
// 公共IPFS网络简化PIN管理实现（无隐私约束版本）
// ============================================================================

impl<T: Config> Pallet<T> {
    /// 函数级详细中文注释：简化的智能PIN分配算法（公共IPFS网络）
    ///
    /// 根据PinTier确定副本数，然后选择最优节点：
    /// - Critical数据：3副本（3个节点）
    /// - Standard数据：2副本（2个节点）
    /// - Temporary数据：1副本（1个节点）
    ///
    /// 节点选择策略（简化评分算法）：
    /// score = capacity_usage(50%) + (100 - health_score)(50%)
    /// 评分越低，节点越优先被选择
    ///
    /// 参数：
    /// - cid_hash: CID的hash值
    /// - tier: PIN层级
    /// - estimated_size: 估算的文件大小（字节）
    ///
    /// 返回：
    /// - 选中的节点列表
    pub fn optimized_pin_allocation(
        cid_hash: T::Hash,
        tier: PinTier,
        estimated_size: u64,
    ) -> Result<BoundedVec<T::AccountId, ConstU32<8>>, Error<T>> {
        // 1. 获取所有活跃节点（从Operators或者一个简化的节点列表）
        let all_nodes = Self::get_active_ipfs_nodes()?;

        // 2. 确定副本数（简化策略）
        let replica_count = match tier {
            PinTier::Critical => 3,  // 3副本（充分冗余）
            PinTier::Standard => 2,  // 2副本（平衡）
            PinTier::Temporary => 1, // 1副本（最小化）
        };

        // 3. 智能选择节点（基于负载和健康度）
        let selected_nodes =
            Self::select_best_ipfs_nodes(&all_nodes, replica_count, estimated_size)?;

        // 4. 记录分配 ✅ P2统一：写入 PinAssignments + LayeredPinAssignments，不再写 SimplePinAssignments
        let ops_16: BoundedVec<T::AccountId, ConstU32<16>> =
            BoundedVec::truncate_from(selected_nodes.to_vec());
        PinAssignments::<T>::insert(&cid_hash, ops_16);
        // 所有节点归入 community（简化分配不区分 core/community）
        let empty_core: BoundedVec<T::AccountId, ConstU32<8>> = BoundedVec::default();
        LayeredPinAssignments::<T>::insert(
            &cid_hash,
            LayeredPinAssignment {
                core_operators: empty_core,
                community_operators: selected_nodes.clone(),
            },
        );

        // 5. 更新节点统计（增加PIN计数）
        for node in selected_nodes.iter() {
            SimpleNodeStatsMap::<T>::mutate(node, |stats| {
                stats.total_pins = stats.total_pins.saturating_add(1);
            });
        }

        // 6. 发送事件
        Self::deposit_event(Event::SimplePinAllocated {
            cid_hash,
            tier,
            nodes: selected_nodes.clone(),
            replicas: replica_count,
        });

        Ok(selected_nodes)
    }

    /// 函数级详细中文注释：获取所有活跃的IPFS节点
    ///
    /// 从 ActiveOperatorIndex 获取活跃运营者列表（有界，O(1) 读取） ✅ P0-16
    ///
    /// 返回：活跃节点账户列表
    fn get_active_ipfs_nodes() -> Result<alloc::vec::Vec<T::AccountId>, Error<T>> {
        let index = ActiveOperatorIndex::<T>::get();
        let active_nodes: alloc::vec::Vec<T::AccountId> = index.into_inner();

        ensure!(!active_nodes.is_empty(), Error::<T>::NoAvailableOperators);

        Ok(active_nodes)
    }

    /// 函数级详细中文注释：选择最优IPFS节点（简化评分）
    ///
    /// ✅ P2统一：使用 OperatorPinStats 替代 SimpleNodeStatsMap
    fn select_best_ipfs_nodes(
        nodes: &alloc::vec::Vec<T::AccountId>,
        count: u32,
        _estimated_size: u64,
    ) -> Result<BoundedVec<T::AccountId, ConstU32<8>>, Error<T>> {
        let mut node_scores: alloc::vec::Vec<(T::AccountId, u32)> = alloc::vec::Vec::new();

        for node in nodes {
            let stats = OperatorPinStats::<T>::get(node);

            // 计算容量使用率（简化估算）
            let capacity_usage = Self::calculate_simple_capacity_usage(node);

            // 容量检查（使用率 > 90%则跳过）
            if capacity_usage > 90 {
                continue;
            }

            // 简化评分公式
            // score = capacity_usage(50%) + (100 - health_score)(50%)
            let health_penalty = 100u8.saturating_sub(stats.health_score);
            let score = (capacity_usage / 2) + (health_penalty as u32 / 2);

            node_scores.push((node.clone(), score));
        }

        // 按评分排序（升序，评分越低越优先）
        node_scores.sort_by(|a, b| a.1.cmp(&b.1));

        // 选择前N个节点
        let selected: alloc::vec::Vec<T::AccountId> = node_scores
            .iter()
            .take(count as usize)
            .map(|(node, _)| node.clone())
            .collect();

        // 确保有足够节点
        ensure!(
            selected.len() >= count as usize,
            Error::<T>::InsufficientNodes
        );

        BoundedVec::try_from(selected).map_err(|_| Error::<T>::TooManyNodes)
    }

    /// 函数级详细中文注释：计算节点容量使用率
    ///
    /// M3-R2修复：使用 OperatorUsedBytes 实际数据替代硬编码 2MB 估算，
    /// 与 calculate_capacity_usage 保持一致。结果钳位到 [0, 100]。
    ///
    /// 参数：
    /// - node: 节点账户
    ///
    /// 返回：
    /// - 容量使用率（0-100）
    pub fn calculate_simple_capacity_usage(node: &T::AccountId) -> u32 {
        let Some(info) = Operators::<T>::get(node) else {
            return 100; // 节点不存在，视为满载
        };

        if info.capacity_gib == 0 {
            return 100; // 容量为0，视为满载
        }

        // M3-R2：使用实际已用字节数（与 calculate_capacity_usage 一致）
        let used_bytes = OperatorUsedBytes::<T>::get(node);
        let used_capacity_gib = used_bytes / (1024 * 1024 * 1024); // bytes → GiB
        let total_capacity_gib = info.capacity_gib as u64;

        // 钳位到 100（防止溢出场景返回 > 100 的值）
        ((used_capacity_gib * 100) / total_capacity_gib).min(100) as u32
    }

    /// 函数级详细中文注释：获取分配给本节点的CID列表（OCW使用）
    ///
    /// ✅ P2统一：使用 PinAssignments 替代 SimplePinAssignments
    pub fn get_my_assigned_cids(
        node: &T::AccountId,
        limit: u32,
    ) -> alloc::vec::Vec<(T::Hash, alloc::vec::Vec<u8>)> {
        let mut result = alloc::vec::Vec::new();

        for (cid_hash, assigned_nodes) in PinAssignments::<T>::iter() {
            if assigned_nodes.contains(node) {
                if let Some(cid) = CidRegistry::<T>::get(&cid_hash) {
                    result.push((cid_hash, cid.to_vec()));

                    if result.len() >= limit as usize {
                        break;
                    }
                }
            }
        }

        result
    }

    /// 函数级详细中文注释：检查IPFS Pin状态（OCW调用）
    ///
    /// 调用本地IPFS HTTP API检查Pin是否存在：
    /// GET http://127.0.0.1:5001/api/v0/pin/ls?arg=<CID>
    ///
    /// 参数：
    /// - cid: Plaintext CID
    ///
    /// 返回：
    /// - Ok(true): Pin存在且健康
    /// - Ok(false): Pin不存在
    /// - Err: HTTP请求失败
    pub fn check_ipfs_pin(cid: &[u8]) -> Result<bool, &'static str> {
        let cid_str = alloc::string::String::from_utf8_lossy(cid);
        let url = alloc::format!("http://127.0.0.1:5001/api/v0/pin/ls?arg={}", cid_str);

        let request = http::Request::get(&url);
        let pending = request
            .deadline(
                sp_io::offchain::timestamp()
                    .add(sp_runtime::offchain::Duration::from_millis(5_000)),
            )
            .send()
            .map_err(|_| "HTTP request failed")?;

        let response = pending
            .try_wait(
                sp_io::offchain::timestamp()
                    .add(sp_runtime::offchain::Duration::from_millis(5_000)),
            )
            .map_err(|_| "HTTP timeout")?
            .map_err(|_| "HTTP error")?;

        Ok(response.code == 200)
    }

    /// 函数级详细中文注释：Pin到本地IPFS（OCW调用）
    ///
    /// 调用本地IPFS HTTP API执行Pin：
    /// POST http://127.0.0.1:5001/api/v0/pin/add?arg=<CID>
    ///
    /// 参数：
    /// - cid: Plaintext CID
    ///
    /// 返回：
    /// - Ok(()): Pin成功
    /// - Err: Pin失败
    pub fn pin_to_local_ipfs(cid: &[u8]) -> Result<(), &'static str> {
        let cid_str = alloc::string::String::from_utf8_lossy(cid);
        let url = alloc::format!("http://127.0.0.1:5001/api/v0/pin/add?arg={}", cid_str);

        // 使用Vec<Vec<u8>>作为body，满足http::Request的要求
        let chunks: Vec<Vec<u8>> = Vec::new();
        let timeout =
            sp_io::offchain::timestamp().add(sp_runtime::offchain::Duration::from_millis(30_000));

        let request = http::Request::post(&url, chunks);
        let pending = request
            .deadline(timeout)
            .send()
            .map_err(|_| "HTTP request failed")?;

        let response = pending
            .try_wait(timeout)
            .map_err(|_| "HTTP timeout")?
            .map_err(|_| "HTTP error")?;

        let code: u16 = response.code;
        if (200..300).contains(&code) {
            Ok(())
        } else {
            Err("Pin failed")
        }
    }

    /// 函数级详细中文注释：获取本地节点账户（OCW使用）
    ///
    /// 从OCW本地存储中读取节点账户ID
    ///
    /// 返回：
    /// - Some(本节点账户) 如果配置存在
    /// - None 如果未配置
    pub fn get_local_node_account() -> Option<T::AccountId> {
        // 从本地存储读取节点账户
        let account_bytes = sp_io::offchain::local_storage_get(
            StorageKind::PERSISTENT,
            b"/memo/ipfs/node_account",
        )?;

        T::AccountId::decode(&mut &account_bytes[..]).ok()
    }

    /// 计算运营者保证金金额（100 USDT 等值的 NEX）
    ///
    /// 使用统一的 DepositCalculator trait 计算
    pub fn calculate_operator_bond() -> BalanceOf<T> {
        use pallet_trading_common::DepositCalculator;
        T::DepositCalculator::calculate_deposit(
            T::MinOperatorBondUsd::get(),
            T::MinOperatorBond::get(),
        )
    }
}
