//! # Commission Core (pallet-commission-core)
//!
//! The core commission orchestration engine.
//! 返佣系统核心调度引擎。
//! Responsibilities include:
//! 负责：
//! - global commission configuration (enabled modes, sources, caps)
//! - 全局返佣配置（启用模式、来源、上限）
//! - commission accounting (`credit_commission`) and cancellation (`cancel_commission`)
//! - 返佣记账（`credit_commission`）与取消（`cancel_commission`）
//! - withdrawal flows (tiered withdrawals + shopping balance)
//! - 提现系统（分级提现 + 购物余额）
//! - solvency protection (`ShopPendingTotal + Loyalty::shopping_total`)
//! - 偿付安全（`ShopPendingTotal + Loyalty::shopping_total`）
//! - plugin dispatch (`ReferralPlugin` / `LevelDiffPlugin` / `SingleLinePlugin` / `TeamPlugin`)
//! - 调度各插件（`ReferralPlugin` / `LevelDiffPlugin` / `SingleLinePlugin` / `TeamPlugin`）

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use pallet_commission_common::{
    CommissionModes, CommissionOutput, CommissionPlugin, CommissionProvider, CommissionRecord,
    CommissionStatus, CommissionType, CrossChainPayout, EntityReferrerProvider, LevelDiffPlanWriter,
    LevelDiffQueryProvider, MemberCommissionStatsData, MemberProvider,
    MemberTokenCommissionStatsData, MultiLevelPlanWriter, MultiLevelQueryProvider,
    ParticipationGuard, PoolRewardPlanWriter, PoolRewardQueryProvider, ReferralPlanWriter,
    ReferralQueryProvider, SingleLineQueryProvider, TeamPlanWriter, TeamQueryProvider,
    TokenCommissionPlugin, TokenCommissionRecord, TokenTransferProvider as TokenTransferProviderT,
    WithdrawalMode, WithdrawalTierConfig,
};
use pallet_entity_common::{
    LoyaltyReadPort as _, LoyaltyTokenReadPort as _, LoyaltyTokenWritePort as _,
};
use sp_runtime::traits::{Saturating, Zero};
use sp_runtime::SaturatedConversion;

pub mod runtime_api;
pub mod weights;
pub use weights::WeightInfo;

mod engine;
mod settlement;
mod withdraw;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        traits::{ConstU16, Currency, ExistenceRequirement, Get},
    };
    use frame_system::pallet_prelude::*;
    use pallet_commission_common::{
        LevelDiffPlanWriter, MultiLevelPlanWriter, ReferralPlanWriter, TeamPlanWriter,
    };
    use pallet_entity_common::{
        AutoRepurchasePort, EntityProvider, EntityTokenPriceProvider, FundProtectionQueryPort,
        GovernanceMode, GovernanceProvider, LoyaltyReadPort, LoyaltyWritePort, PricingProvider,
        ShopProvider,
    };
    use sp_runtime::traits::{Saturating, Zero};
    use sp_runtime::SaturatedConversion;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    pub type CommissionRecordOf<T> =
        CommissionRecord<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

    pub type MemberCommissionStatsOf<T> = MemberCommissionStatsData<BalanceOf<T>>;

    pub type TokenBalanceOf<T> = <T as Config>::TokenBalance;

    pub type TokenCommissionRecordOf<T> = TokenCommissionRecord<
        <T as frame_system::Config>::AccountId,
        TokenBalanceOf<T>,
        BlockNumberFor<T>,
    >;

    pub type MemberTokenCommissionStatsOf<T> = MemberTokenCommissionStatsData<TokenBalanceOf<T>>;

    /// Per-plugin budget caps in basis points, measured against the order amount and
    /// using the same scale as `max_commission_rate`.
    /// 插件独立预算上限（bps，相对于订单金额，与 `max_commission_rate` 同一量纲）
    ///
    /// Each cap limits how much of the order amount a plugin may consume.
    /// 每个 cap 值表示该插件最多使用订单金额的 `cap / 10000` 进行分配。
    /// The owner may set `max_rate = 3000` and then distribute shares across plugins
    /// as needed (for example referral 1000 + multi_level 1500 + team 500).
    /// Owner 可先设 `max_rate = 3000`，再将费率按需分配给各插件
    /// （如 referral 1000 + multi_level 1500 + team 500）。
    /// `0` means unlimited for that plugin and preserves backward compatibility.
    /// `0` = 不限制（该插件可使用全部 remaining），向后兼容。
    /// Each cap must be `<= max_commission_rate` and is validated on write.
    /// 每个 cap 必须 `<= max_commission_rate`（设置时校验）。
    /// Runtime formula: `plugin_budget = min(remaining, order_amount × cap / 10000)`.
    /// 运行时：`plugin_budget = min(remaining, order_amount × cap / 10000)`。
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
    pub struct PluginBudgetCaps {
        /// 推荐返佣上限（DirectReward/FirstOrder/RepeatPurchase/FixedAmount）
        pub referral_cap: u16,
        /// 多级分销上限
        pub multi_level_cap: u16,
        /// 级差奖上限
        pub level_diff_cap: u16,
        /// 单线排队上限
        pub single_line_cap: u16,
        /// 团队业绩上限
        pub team_cap: u16,
    }

    impl Default for PluginBudgetCaps {
        fn default() -> Self {
            Self {
                referral_cap: 0,
                multi_level_cap: 0,
                level_diff_cap: 0,
                single_line_cap: 0,
                team_cap: 0,
            }
        }
    }

    /// Global commission switch configuration for a single entity.
    /// 全局返佣开关配置（per-entity）。
    ///
    /// Member commissions are deducted from seller proceeds under `max_commission_rate`.
    /// 会员返佣从卖家货款中扣除（`max_commission_rate`）。
    /// Referrer incentives are controlled by the global `ReferrerShareBps` constant.
    /// 招商奖金比例由全局常量 `ReferrerShareBps` 控制。
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
    pub struct CoreCommissionConfig {
        /// 启用的返佣模式（位标志）
        pub enabled_modes: CommissionModes,
        /// 会员返佣上限比例（基点，10000 = 100%）
        /// 佣金从卖家货款中扣除，Entity Owner 可设置
        pub max_commission_rate: u16,
        /// 是否全局启用
        pub enabled: bool,
        /// 提现冻结期（区块数，0 = 无冻结）
        pub withdrawal_cooldown: u32,
        /// Owner 收益比例（基点 of order_amount，从 Pool B 预算中优先扣除）
        /// 0 = 不启用，500 = 订单金额的 5%（与 plugin_cap 同维度）
        pub owner_reward_rate: u16,
        /// Token 提现冻结期（区块数，0 = 使用 withdrawal_cooldown）
        /// F3: Token/NEX 独立冻结期，与 P3 审计修复（独立时间追踪）配套
        pub token_withdrawal_cooldown: u32,
        /// 插件独立预算上限（bps，与 max_commission_rate 同一量纲，0 = 不限制）
        pub plugin_caps: PluginBudgetCaps,
    }

    impl Default for CoreCommissionConfig {
        fn default() -> Self {
            Self {
                enabled_modes: CommissionModes::default(),
                max_commission_rate: 10000,
                enabled: false,
                withdrawal_cooldown: 0,
                owner_reward_rate: 0,
                token_withdrawal_cooldown: 0,
                plugin_caps: PluginBudgetCaps::default(),
            }
        }
    }

    /// Entity withdrawal configuration covering four modes plus voluntary repurchase rewards.
    /// 实体提现配置（四种模式 + 自愿复购奖励）。
    ///
    /// Modes:
    /// 模式：
    /// - `FullWithdrawal`: no forced repurchase, while governance floors still apply.
    /// - `FullWithdrawal`：不强制复购，Governance 底线仍生效。
    /// - `FixedRate`: all members use the same repurchase ratio.
    /// - `FixedRate`：所有会员统一复购比率。
    /// - `LevelBased`: lookup by `level_id` through `default_tier` / `level_overrides`.
    /// - `LevelBased`：按 `level_id` 查 `default_tier` / `level_overrides`。
    /// - `MemberChoice`: members choose their ratio, but not below `min_repurchase_rate`.
    /// - `MemberChoice`：会员提现时自选比率，不低于 `min_repurchase_rate`。
    ///
    /// `voluntary_bonus_rate` adds extra shopping balance on the repurchase amount above
    /// the mandatory minimum.
    /// `voluntary_bonus_rate`：对超出强制最低线的复购部分按 bonus_rate 额外计入购物余额。
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
    #[scale_info(skip_type_params(MaxLevels))]
    pub struct EntityWithdrawalConfig<MaxLevels: Get<u32>> {
        /// 提现模式
        pub mode: WithdrawalMode,
        /// LevelBased 模式下的默认提现/复购比例
        pub default_tier: WithdrawalTierConfig,
        /// LevelBased 模式下按 level_id 覆写的配置
        pub level_overrides: BoundedVec<(u8, WithdrawalTierConfig), MaxLevels>,
        /// 自愿多复购奖励加成（万分比，如 500 = 5%）
        pub voluntary_bonus_rate: u16,
        pub enabled: bool,
    }

    impl<MaxLevels: Get<u32>> Default for EntityWithdrawalConfig<MaxLevels> {
        fn default() -> Self {
            Self {
                mode: WithdrawalMode::default(),
                default_tier: WithdrawalTierConfig::default(),
                level_overrides: BoundedVec::default(),
                voluntary_bonus_rate: 0,
                enabled: false,
            }
        }
    }

    pub type EntityWithdrawalConfigOf<T> = EntityWithdrawalConfig<<T as Config>::MaxCustomLevels>;

    /// 提现分配结果（内部使用）
    pub(crate) struct WithdrawalSplit<Balance> {
        pub withdrawal: Balance,
        pub repurchase: Balance,
        pub bonus: Balance,
    }

    // ========================================================================
    // Config
    // ========================================================================

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type Currency: Currency<Self::AccountId>
            + frame_support::traits::ReservableCurrency<Self::AccountId>;

        /// Weight provider.
        /// 权重信息
        type WeightInfo: crate::WeightInfo;

        /// Shop lookup interface.
        /// Shop 查询接口
        type ShopProvider: ShopProvider<Self::AccountId>;

        /// Entity lookup interface.
        /// Entity 查询接口
        type EntityProvider: EntityProvider<Self::AccountId>;

        /// Governance lookup interface used for the R8 monotonic-decrease exemption in
        /// `locked + None` mode.
        /// 治理查询接口（R8：用于 `locked + None` 模式的单调递减豁免）
        type GovernanceProvider: GovernanceProvider;

        /// Member lookup interface.
        /// 会员查询接口
        type MemberProvider: MemberProvider<Self::AccountId>;

        /// Referral-chain commission plugin.
        /// 推荐链返佣插件
        type ReferralPlugin: CommissionPlugin<Self::AccountId, BalanceOf<Self>>;

        /// Multi-level commission plugin.
        /// 多级分销返佣插件
        type MultiLevelPlugin: CommissionPlugin<Self::AccountId, BalanceOf<Self>>;

        /// Level-difference commission plugin.
        /// 等级极差返佣插件
        type LevelDiffPlugin: CommissionPlugin<Self::AccountId, BalanceOf<Self>>;

        /// Single-line commission plugin.
        /// 单线收益插件
        type SingleLinePlugin: CommissionPlugin<Self::AccountId, BalanceOf<Self>>;

        /// Team-performance plugin (reserved for future use).
        /// 团队业绩插件（预留）
        type TeamPlugin: CommissionPlugin<Self::AccountId, BalanceOf<Self>>;

        /// Entity referrer lookup interface.
        /// 招商推荐人查询接口
        type EntityReferrerProvider: EntityReferrerProvider<Self::AccountId>;

        /// 推荐链方案写入器
        type ReferralWriter: ReferralPlanWriter<BalanceOf<Self>>;

        /// 多级分销方案写入器
        type MultiLevelWriter: MultiLevelPlanWriter;

        /// 等级极差方案写入器
        type LevelDiffWriter: LevelDiffPlanWriter;

        /// 团队业绩方案写入器
        type TeamWriter: TeamPlanWriter<BalanceOf<Self>>;

        /// 沉淀池奖励方案写入器
        type PoolRewardWriter: PoolRewardPlanWriter;

        /// 平台账户（用于招商奖金从平台费中扣除）
        type PlatformAccount: Get<Self::AccountId>;

        /// 国库账户（接收平台费中推荐人奖金以外的部分）
        type TreasuryAccount: Get<Self::AccountId>;

        /// 招商推荐人分佣比例（基点，5000 = 平台费的50%）
        /// 全局固定规则：平台费 = 1%，referrer 50% + 国库 50%
        #[pallet::constant]
        type ReferrerShareBps: Get<u16>;

        /// 最大返佣记录数（每订单）
        #[pallet::constant]
        type MaxCommissionRecordsPerOrder: Get<u32>;

        /// 最大自定义等级数
        #[pallet::constant]
        type MaxCustomLevels: Get<u32>;

        /// 推荐人佣金保护期（区块数）
        /// 推荐人绑定后在此期间内，Entity Owner/Admin 不可关闭推荐人佣金
        /// 约 2 年 ≈ 10_512_000 blocks（6 秒出块）
        #[pallet::constant]
        type ReferrerProtectionPeriod: Get<u64>;

        /// Entity 参与权守卫（KYC / 合规检查）
        /// 默认使用 `()` 允许所有操作（无 KYC 要求）
        type ParticipationGuard: crate::ParticipationGuard<Self::AccountId>;

        // ====================================================================
        // Token 多资产扩展（方案 A）
        // ====================================================================

        /// Entity Token 余额类型
        type TokenBalance: codec::FullCodec
            + codec::MaxEncodedLen
            + TypeInfo
            + Copy
            + Default
            + core::fmt::Debug
            + sp_runtime::traits::AtLeast32BitUnsigned
            + From<u32>
            + Into<u128>;

        /// Token 版推荐链插件
        type TokenReferralPlugin: TokenCommissionPlugin<Self::AccountId, TokenBalanceOf<Self>>;

        /// Token 版多级分销插件
        type TokenMultiLevelPlugin: TokenCommissionPlugin<Self::AccountId, TokenBalanceOf<Self>>;

        /// Token 版等级极差插件
        type TokenLevelDiffPlugin: TokenCommissionPlugin<Self::AccountId, TokenBalanceOf<Self>>;

        /// Token 版单线收益插件
        type TokenSingleLinePlugin: TokenCommissionPlugin<Self::AccountId, TokenBalanceOf<Self>>;

        /// Token 版团队业绩插件
        type TokenTeamPlugin: TokenCommissionPlugin<Self::AccountId, TokenBalanceOf<Self>>;

        /// Token 转账接口（entity_id 级）
        type TokenTransferProvider: TokenTransferProviderT<Self::AccountId, TokenBalanceOf<Self>>;

        /// F6: 会员提现记录上限（每个 (entity_id, account) 最多保留的提现记录数）
        #[pallet::constant]
        type MaxWithdrawalRecords: Get<u32>;

        /// F7: 会员佣金关联订单 ID 上限（每个 (entity_id, account) 最多保留的 order_id 数）
        #[pallet::constant]
        type MaxMemberOrderIds: Get<u32>;

        // ====================================================================
        // 子模块查询接口（供 Runtime API 聚合）
        // ====================================================================

        /// 多级分销查询
        type MultiLevelQuery: pallet_commission_common::MultiLevelQueryProvider<Self::AccountId>;

        /// 多级分销统计回滚（订单取消时回滚 MemberMultiLevelStats / EntityMultiLevelStats）
        type MultiLevelStatsRollback: pallet_commission_common::PluginStatsRollback<Self::AccountId>;
        /// 团队业绩查询
        type TeamQuery: pallet_commission_common::TeamQueryProvider<
            Self::AccountId,
            BalanceOf<Self>,
        >;
        /// 单线收益查询
        type SingleLineQuery: pallet_commission_common::SingleLineQueryProvider<Self::AccountId>;
        /// 级差查询
        type LevelDiffQuery: pallet_commission_common::LevelDiffQueryProvider;
        /// 沉淀池奖励查询
        type PoolRewardQuery: pallet_commission_common::PoolRewardQueryProvider<
            Self::AccountId,
            BalanceOf<Self>,
            TokenBalanceOf<Self>,
        >;
        /// 推荐链返佣查询
        type ReferralQuery: pallet_commission_common::ReferralQueryProvider<
            Self::AccountId,
            BalanceOf<Self>,
        >;

        /// Loyalty 模块接口（NEX 购物余额读写）
        type Loyalty: pallet_entity_common::LoyaltyWritePort<Self::AccountId, BalanceOf<Self>>;

        /// Loyalty Token 模块接口（Token 购物余额读写）
        type LoyaltyToken: pallet_entity_common::LoyaltyTokenWritePort<
            Self::AccountId,
            TokenBalanceOf<Self>,
        >;

        /// NEX 平台费率查询（用于佣金预算上限约束：max_commission_rate ≤ 10000 - platform_fee_rate）
        type PlatformFeeRate: pallet_entity_common::FeeConfigProvider;

        /// 沉淀池资金来源记录回调（由 pool-reward 实现，在每次入池时通知记录来源）
        type PoolFundingCallback: pallet_commission_common::PoolFundingCallback;

        /// 资金保护配置查询（用于 withdraw_entity_funds 检查 min_treasury_threshold）
        type FundProtectionQuery: pallet_entity_common::FundProtectionQueryPort;

        /// NEX/USDT 定价接口（用于 check_repurchase_ready 中购物余额折算 USDT）
        type PricingProvider: pallet_entity_common::PricingProvider;

        /// Entity Token / USDT 定价接口（用于 Token 购物余额折算 USDT）
        type TokenPriceProvider: pallet_entity_common::EntityTokenPriceProvider;

        /// 自动复购订单创建接口（由 order pallet 实现，注入 runtime bridge）
        /// 失败时降级为发 RepurchaseReady 事件，不阻塞提现流程
        type AutoRepurchase: pallet_entity_common::AutoRepurchasePort<Self::AccountId>;

        /// 购物余额 TTL 最小值（区块数）
        ///
        /// 防止 Entity Owner 设置过短的 TTL，导致 `expire_shopping_balance`
        /// 被频繁调用、浪费链上资源。
        /// 建议值：100_800 ≈ 7 天（6 秒出块）
        /// 设为 0 则不强制最小值（测试用）。
        #[pallet::constant]
        type MinShoppingBalanceTtlBlocks: Get<u32>;

        /// Cross-chain native-NEX payout port (HB-WD-01). Wired by the runtime to
        /// the bridge's outbound core; defaults to `()` (cross-chain withdrawal
        /// disabled — `withdraw_commission_to_evm` fails without burning or
        /// touching pending).
        /// 原生 NEX 跨链派发端口（HB-WD-01）。由 runtime 对接桥的出站核心；默认 `()`
        ///（关闭跨链提现——`withdraw_commission_to_evm` 失败且不 burn、不动 pending）。
        type CrossChainPayout: crate::CrossChainPayout<Self::AccountId, BalanceOf<Self>>;
    }

    /// 提现记录
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
    pub struct WithdrawalRecord<Balance, BlockNumber> {
        /// 提现总额（withdrawal + repurchase + bonus）
        pub total_amount: Balance,
        /// 到手金额
        pub withdrawn: Balance,
        /// 复购金额
        pub repurchased: Balance,
        /// 自愿多复购奖励
        pub bonus: Balance,
        /// 提现区块号
        pub block_number: BlockNumber,
    }

    pub type WithdrawalRecordOf<T> = WithdrawalRecord<BalanceOf<T>, BlockNumberFor<T>>;
    pub type TokenWithdrawalRecordOf<T> = WithdrawalRecord<TokenBalanceOf<T>, BlockNumberFor<T>>;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ========================================================================
    // Storage
    // ========================================================================

    /// Entity 返佣核心配置 entity_id -> CoreCommissionConfig
    #[pallet::storage]
    #[pallet::getter(fn commission_config)]
    pub type CommissionConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, CoreCommissionConfig>;

    /// 会员返佣统计 (entity_id, account) -> MemberCommissionStatsData
    #[pallet::storage]
    #[pallet::getter(fn member_commission_stats)]
    pub type MemberCommissionStats<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        MemberCommissionStatsOf<T>,
        ValueQuery,
    >;

    /// Shopping-balance commission stats per member (entity_id, account) -> MemberCommissionStatsData
    ///
    /// Separate from NEX stats to prevent cross-pipeline order_count pollution.
    /// 购物余额佣金独立统计，与 NEX 统计隔离，防止 order_count 跨管线污染。
    #[pallet::storage]
    pub type MemberShoppingCommissionStats<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        MemberCommissionStatsOf<T>,
        ValueQuery,
    >;

    /// 订单返佣记录 order_id -> Vec<CommissionRecord>
    #[pallet::storage]
    #[pallet::getter(fn order_commission_records)]
    pub type OrderCommissionRecords<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<CommissionRecordOf<T>, T::MaxCommissionRecordsPerOrder>,
        ValueQuery,
    >;

    /// Entity 返佣统计 entity_id -> (total_distributed, total_orders)
    #[pallet::storage]
    #[pallet::getter(fn entity_commission_totals)]
    pub type ShopCommissionTotals<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (BalanceOf<T>, u64), ValueQuery>;

    /// Entity 待提取佣金总额 entity_id -> Balance
    #[pallet::storage]
    #[pallet::getter(fn entity_pending_total)]
    pub type ShopPendingTotal<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BalanceOf<T>, ValueQuery>;

    /// 提现配置 entity_id -> EntityWithdrawalConfig
    #[pallet::storage]
    #[pallet::getter(fn withdrawal_config)]
    pub type WithdrawalConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, EntityWithdrawalConfigOf<T>>;

    /// 会员最后入账区块 (entity_id, account) -> BlockNumber（用于冻结期检查）
    #[pallet::storage]
    pub type MemberLastCredited<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
        ValueQuery,
    >;

    /// 全局最低复购比例 entity_id -> u16（万分比，由 Governance 设定）
    /// 提现时实际复购比例 = max(entity 分层配置, 此底线)
    #[pallet::storage]
    #[pallet::getter(fn global_min_repurchase_rate)]
    pub type GlobalMinRepurchaseRate<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u16, ValueQuery>;

    /// 订单平台费转国库金额 order_id -> Balance（用于 cancel_commission 退款）
    #[pallet::storage]
    pub type OrderTreasuryTransfer<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BalanceOf<T>, ValueQuery>;

    /// 未分配佣金沉淀资金池 entity_id -> Balance
    #[pallet::storage]
    #[pallet::getter(fn unallocated_pool)]
    pub type UnallocatedPool<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BalanceOf<T>, ValueQuery>;

    /// 订单未分配佣金记录 order_id -> (entity_id, shop_id, Balance)
    /// 用于 cancel_commission 时退还未分配部分给卖家
    #[pallet::storage]
    pub type OrderUnallocated<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (u64, u64, BalanceOf<T>), ValueQuery>;

    // ========================================================================
    // H-2 审计修复: 退款失败待重试记录
    // ========================================================================

    /// 退款失败待重试总额 entity_id → Balance
    ///
    /// 取消订单时转账退款失败的佣金金额累计。
    /// 这些资金仍在 entity_account 中，对应的个人 stats(pending/total_earned) 不扣减，
    /// 直到 Root 通过 retry_pending_refund 成功退回后才清零。
    /// 偿付检查中须计入此值，防止被其他提现消耗。
    #[pallet::storage]
    pub type PendingRefundTotal<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BalanceOf<T>, ValueQuery>;

    /// 退款失败待重试明细 order_id → Vec<(entity_id, shop_id, is_platform, amount)>
    ///
    /// 记录每笔订单中转账退款失败的分组，供 Root retry_pending_refund 精准重试。
    /// 成功重试后删除对应条目并扣减 PendingRefundTotal。
    #[pallet::storage]
    pub type PendingRefunds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<(u64, u64, bool, BalanceOf<T>), ConstU32<50>>,
        ValueQuery,
    >;

    // ========================================================================
    // Token Storage（方案 A: 全插件多资产化）
    // ========================================================================

    /// Token 佣金统计 (entity_id, account) → MemberTokenCommissionStatsData
    #[pallet::storage]
    pub type MemberTokenCommissionStats<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        MemberTokenCommissionStatsOf<T>,
        ValueQuery,
    >;

    /// Token 佣金记录 order_id → Vec<TokenCommissionRecord>
    #[pallet::storage]
    pub type OrderTokenCommissionRecords<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<TokenCommissionRecordOf<T>, T::MaxCommissionRecordsPerOrder>,
        ValueQuery,
    >;

    /// Token 待提取总额 entity_id → TokenBalance
    #[pallet::storage]
    pub type TokenPendingTotal<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, TokenBalanceOf<T>, ValueQuery>;

    /// Token 未分配沉淀池 entity_id → TokenBalance
    #[pallet::storage]
    pub type UnallocatedTokenPool<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, TokenBalanceOf<T>, ValueQuery>;

    /// Token 订单沉淀池记录 order_id → (entity_id, shop_id, TokenBalance)
    #[pallet::storage]
    pub type OrderTokenUnallocated<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (u64, u64, TokenBalanceOf<T>), ValueQuery>;

    /// M2-R6 审计修复: Token 订单平台费留存 order_id → (entity_id, TokenBalance)
    /// 记录 process_token_commission 中 Pool A 留存（platform_fee - referrer），
    /// 供 cancel 时从 UnallocatedTokenPool 回退
    #[pallet::storage]
    pub type OrderTokenPlatformRetention<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (u64, TokenBalanceOf<T>), ValueQuery>;

    /// HIGH-1 审计修复: Token 退款失败待重试总额 entity_id → TokenBalance
    ///
    /// Token 取消时沉淀池退款失败的金额累计（与 NEX PendingRefundTotal 对称）。
    /// 这些 Token 仍在 entity_account 中，偿付检查须计入此值。
    #[pallet::storage]
    pub type PendingTokenRefundTotal<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, TokenBalanceOf<T>, ValueQuery>;

    /// HIGH-1 审计修复: Token 退款失败待重试明细 order_id → Vec<(entity_id, shop_id, amount)>
    ///
    /// 记录每笔订单中 Token 沉淀池转账退款失败的条目，供 Root retry。
    #[pallet::storage]
    pub type PendingTokenRefunds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<(u64, u64, TokenBalanceOf<T>), ConstU32<50>>,
        ValueQuery,
    >;

    /// Token 订单平台费率（基点，100 = 1%）
    /// 可通过 set_token_platform_fee_rate 治理调整，0 = 关闭 Token 平台费
    #[pallet::storage]
    pub type TokenPlatformFeeRate<T> =
        StorageValue<_, u16, ValueQuery, DefaultTokenPlatformFeeRate>;

    /// Token 平台费率默认值（100 bps = 1%）
    #[pallet::type_value]
    pub fn DefaultTokenPlatformFeeRate() -> u16 {
        100
    }

    /// Token 提现配置 entity_id → EntityWithdrawalConfig（与 NEX 对称，独立配置）
    #[pallet::storage]
    #[pallet::getter(fn token_withdrawal_config)]
    pub type TokenWithdrawalConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, EntityWithdrawalConfigOf<T>>;

    /// Token 会员最后入账区块 (entity_id, account) → BlockNumber（用于 Token 独立冻结期检查）
    /// P3 审计修复: Token 冻结期与 NEX 完全解耦，各自独立管理
    #[pallet::storage]
    pub type MemberTokenLastCredited<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
        ValueQuery,
    >;

    /// Token 全局最低复购比例 entity_id → u16（万分比，由 Governance 设定）
    #[pallet::storage]
    #[pallet::getter(fn global_min_token_repurchase_rate)]
    pub type GlobalMinTokenRepurchaseRate<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u16, ValueQuery>;

    /// entity_account Token 已记账余额（用于检测外部转入）
    /// 跟踪 entity_account 通过已知渠道（平台费入账、佣金提现、购物消费、退款）
    /// 应有的 Token 余额。actual_balance - accounted = 外部转入金额。
    #[pallet::storage]
    pub type EntityTokenAccountedBalance<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, TokenBalanceOf<T>>;

    /// F15: 全局佣金率上限 entity_id → u16（万分比，由 Root 设定）
    /// Entity Owner 的 max_commission_rate 不得超过此值。0 = 无限制（默认）
    #[pallet::storage]
    #[pallet::getter(fn global_max_commission_rate)]
    pub type GlobalMaxCommissionRate<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u16, ValueQuery>;

    /// F16: 全局 Token 佣金率上限 entity_id → u16（万分比，由 Root 设定）
    /// 与 NEX GlobalMaxCommissionRate 对称
    #[pallet::storage]
    #[pallet::getter(fn global_max_token_commission_rate)]
    pub type GlobalMaxTokenCommissionRate<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u16, ValueQuery>;

    /// F17: 全局佣金紧急暂停开关（true = 暂停所有 Entity 的佣金处理和提现）
    #[pallet::storage]
    #[pallet::getter(fn global_commission_paused)]
    pub type GlobalCommissionPaused<T> = StorageValue<_, bool, ValueQuery>;

    /// 全局治理：多级分销层数超过此阈值的 Entity，免除推荐人招商提成
    /// 平台费全部归国库，防止深层分销团队成员拉队伍另立门户
    /// 默认值 5：多级分销超过 5 层即自动免除推荐人提成
    /// 设为 0 可禁用此规则
    #[pallet::storage]
    #[pallet::getter(fn referrer_exempt_threshold)]
    pub type ReferrerExemptThreshold<T: Config> = StorageValue<_, u16, ValueQuery, ConstU16<5>>;

    /// Entity 级推荐人佣金关闭开关 entity_id → bool
    /// Entity Owner/Admin 可关闭本 Entity 推荐人的平台费分成（Pool A）
    /// true = 推荐人不获得 EntityReferral 佣金，平台费 100% 归国库
    #[pallet::storage]
    #[pallet::getter(fn referrer_payout_disabled)]
    pub type ReferrerPayoutDisabled<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// F18: Entity 级提现暂停开关 entity_id → bool
    /// 与 WithdrawalConfig.enabled 独立，轻量级暂停/恢复
    #[pallet::storage]
    #[pallet::getter(fn withdrawal_paused)]
    pub type WithdrawalPaused<T: Config> = StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// F19: 会员佣金关联订单 ID 索引 (entity_id, account) → BoundedVec<order_id>
    #[pallet::storage]
    pub type MemberCommissionOrderIds<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u64, T::MaxMemberOrderIds>,
        ValueQuery,
    >;

    /// F19: Token 版会员佣金关联订单 ID 索引
    #[pallet::storage]
    pub type MemberTokenCommissionOrderIds<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u64, T::MaxMemberOrderIds>,
        ValueQuery,
    >;

    /// 推荐人从每个直推买家获得的佣金累计 (entity_id, referrer, buyer) → Balance
    ///
    /// 仅追踪推荐链类型（DirectReward/FirstOrder/RepeatPurchase/FixedAmount）的佣金。
    /// 在 credit_commission 中同步写入，cancel_commission 中同步扣减。
    #[pallet::storage]
    pub type ReferrerEarnedByBuyer<T: Config> = StorageNMap<
        _,
        (
            NMapKey<Blake2_128Concat, u64>,          // entity_id
            NMapKey<Blake2_128Concat, T::AccountId>, // referrer
            NMapKey<Blake2_128Concat, T::AccountId>, // buyer
        ),
        BalanceOf<T>,
        ValueQuery,
    >;

    /// F20: 会员 NEX 提现历史 (entity_id, account) → BoundedVec<WithdrawalRecord>
    #[pallet::storage]
    pub type MemberWithdrawalHistory<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<WithdrawalRecordOf<T>, T::MaxWithdrawalRecords>,
        ValueQuery,
    >;

    /// F20: 会员 Token 提现历史
    #[pallet::storage]
    pub type MemberTokenWithdrawalHistory<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<TokenWithdrawalRecordOf<T>, T::MaxWithdrawalRecords>,
        ValueQuery,
    >;

    /// MISSING-3: 会员 NEX 最后提现区块 (entity_id, account) → BlockNumber
    /// 与 MemberLastCredited（入账时间）独立，用于提现频率限制
    #[pallet::storage]
    pub type MemberLastWithdrawn<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
        ValueQuery,
    >;

    /// MISSING-3: 会员 Token 最后提现区块
    #[pallet::storage]
    pub type MemberTokenLastWithdrawn<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
        ValueQuery,
    >;

    /// MISSING-3: Entity 最小提现间隔（区块数，0 = 无限制）
    /// 与 withdrawal_cooldown（基于入账时间）独立，本参数基于上次提现时间
    #[pallet::storage]
    #[pallet::getter(fn min_withdrawal_interval)]
    pub type MinWithdrawalInterval<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u32, ValueQuery>;

    /// P0-2 审计修复: 订单佣金幂等保护（order_id → bool），防止同一订单重复处理
    #[pallet::storage]
    pub type OrderCommissionProcessed<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// P0-2 审计修复: Token 订单佣金幂等保护
    #[pallet::storage]
    pub type OrderTokenCommissionProcessed<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// 店铺级佣金率覆盖 shop_id → u16（基点，10000 = 100%）
    /// None = 继承 Entity 级 max_commission_rate
    /// 优先级: ProductCommissionRate > ShopCommissionRate > CoreCommissionConfig.max_commission_rate
    #[pallet::storage]
    pub type ShopCommissionRate<T: Config> = StorageMap<_, Blake2_128Concat, u64, u16>;

    /// 产品级佣金率覆盖 product_id → u16（基点，10000 = 100%）
    /// None = 继承 Shop 级或 Entity 级
    /// 优先级: ProductCommissionRate > ShopCommissionRate > CoreCommissionConfig.max_commission_rate
    #[pallet::storage]
    pub type ProductCommissionRate<T: Config> = StorageMap<_, Blake2_128Concat, u64, u16>;

    // ── 购物余额分佣通道 Storage ──

    /// 购物余额分佣记录（与 OrderCommissionRecords / OrderTokenCommissionRecords 平行）
    #[pallet::storage]
    pub type OrderShoppingCommissionRecords<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<
            CommissionRecord<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
            T::MaxCommissionRecordsPerOrder,
        >,
        ValueQuery,
    >;

    /// 购物余额分佣幂等标记
    #[pallet::storage]
    pub type OrderShoppingCommissionProcessed<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// 购物余额分佣 - 沉淀池追踪（取消退款用）
    #[pallet::storage]
    pub type OrderShoppingUnallocated<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        (u64, u64, BalanceOf<T>), // (entity_id, shop_id, amount)
    >;

    /// 强制复购配置 entity_id → RepurchaseConfig
    #[pallet::storage]
    pub type RepurchaseConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, pallet_commission_common::RepurchaseConfig>;

    /// 购物余额最后 credit 时间 (entity_id, account) → BlockNumber
    /// 用于 TTL 过期判断：仅在 check_repurchase_ready 中（withdraw_commission 后）写入
    #[pallet::storage]
    pub type MemberShoppingBalanceLastCredited<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
    >;

    // ========================================================================
    // Events
    // ========================================================================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        CommissionConfigUpdated {
            entity_id: u64,
        },
        CommissionModesUpdated {
            entity_id: u64,
            modes: CommissionModes,
        },
        CommissionDistributed {
            entity_id: u64,
            order_id: u64,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            commission_type: CommissionType,
            level: u16,
        },
        /// [已废弃] 保留以维持事件索引稳定，新代码使用 TieredWithdrawal
        CommissionWithdrawn {
            entity_id: u64,
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// CC-M1 审计修复: 增加成功/失败计数，便于链上追踪部分退款
        CommissionCancelled {
            order_id: u64,
            refund_succeeded: u32,
            refund_failed: u32,
        },
        /// [已移除] init_commission_plan 过度设计，前端改用 utility.batch 组合分步 extrinsics。
        /// 保留占位以维持事件索引稳定。
        CommissionPlanRemoved {
            entity_id: u64,
        },
        WithdrawalCooldownNotMet {
            entity_id: u64,
            account: T::AccountId,
            earliest_block: BlockNumberFor<T>,
        },
        TieredWithdrawal {
            entity_id: u64,
            account: T::AccountId,
            withdrawn_amount: BalanceOf<T>,
            repurchase_amount: BalanceOf<T>,
            bonus_amount: BalanceOf<T>,
        },
        /// HB-WD-01: the `withdrawal` part of a tiered withdrawal was bridged out
        /// to an EVM chain as native NEX (repurchase + bonus still credited to the
        /// on-chain shopping balance). Carries the ISMP request commitment.
        /// HB-WD-01：分级提现的 `withdrawal` 部分作为原生 NEX 桥出到某 EVM 链
        ///（repurchase + bonus 仍计入链上购物余额）。携带 ISMP 请求 commitment。
        CommissionBridgedOut {
            entity_id: u64,
            account: T::AccountId,
            evm_chain_id: u32,
            recipient: [u8; 20],
            withdrawn_amount: BalanceOf<T>,
            repurchase_amount: BalanceOf<T>,
            bonus_amount: BalanceOf<T>,
            commitment: [u8; 32],
        },
        WithdrawalConfigUpdated {
            entity_id: u64,
        },
        ShoppingBalanceUsed {
            entity_id: u64,
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// 佣金资金从 Shop 转入 Entity 账户
        CommissionFundsTransferred {
            entity_id: u64,
            shop_id: u64,
            amount: BalanceOf<T>,
        },
        /// 平台费剩余部分转入国库
        PlatformFeeToTreasury {
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// 国库退款（订单取消时平台费退回平台账户）
        TreasuryRefund {
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// 佣金退款失败（Entity 账户余额不足，需人工干预）
        CommissionRefundFailed {
            entity_id: u64,
            shop_id: u64,
            amount: BalanceOf<T>,
        },
        /// 未分配佣金转入沉淀资金池
        UnallocatedCommissionPooled {
            entity_id: u64,
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// 沉淀池奖励发放（Phase 2）
        PoolRewardDistributed {
            entity_id: u64,
            order_id: u64,
            total_distributed: BalanceOf<T>,
        },
        /// 沉淀池退还卖家（订单取消）
        UnallocatedPoolRefunded {
            entity_id: u64,
            order_id: u64,
            amount: BalanceOf<T>,
        },

        // Token 多资产事件
        /// Token 佣金分发
        TokenCommissionDistributed {
            entity_id: u64,
            order_id: u64,
            beneficiary: T::AccountId,
            amount: TokenBalanceOf<T>,
            commission_type: CommissionType,
            level: u16,
        },
        /// [已废弃] 保留以维持事件索引稳定，新代码使用 TokenTieredWithdrawal
        TokenCommissionWithdrawn {
            entity_id: u64,
            account: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// Token 沉淀池入账
        TokenUnallocatedPooled {
            entity_id: u64,
            order_id: u64,
            amount: TokenBalanceOf<T>,
        },
        /// Token 沉淀池退还
        TokenUnallocatedPoolRefunded {
            entity_id: u64,
            order_id: u64,
            amount: TokenBalanceOf<T>,
        },
        /// M1 审计修复: Governance 设置 Token 全局最低复购比例
        GlobalMinTokenRepurchaseRateSet {
            entity_id: u64,
            rate: u16,
        },
        /// L5 审计修复: Governance 设置 NEX 全局最低复购比例
        GlobalMinRepurchaseRateSet {
            entity_id: u64,
            rate: u16,
        },
        /// Token 佣金取消
        TokenCommissionCancelled {
            order_id: u64,
            cancelled_count: u32,
        },
        /// Token 分层提现（含复购分流 + 赠与）
        TokenTieredWithdrawal {
            entity_id: u64,
            account: T::AccountId,
            withdrawn_amount: TokenBalanceOf<T>,
            repurchase_amount: TokenBalanceOf<T>,
            bonus_amount: TokenBalanceOf<T>,
        },
        /// Token 提现配置已更新
        TokenWithdrawalConfigUpdated {
            entity_id: u64,
        },
        /// Token 购物余额已使用
        TokenShoppingBalanceUsed {
            entity_id: u64,
            account: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// Entity Owner 提取 entity_account NEX 自由余额
        EntityFundsWithdrawn {
            entity_id: u64,
            to: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Entity Owner 提取 entity_account Token 自由余额
        EntityTokenFundsWithdrawn {
            entity_id: u64,
            to: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// Token 平台费率已更新
        TokenPlatformFeeRateUpdated {
            old_rate: u16,
            new_rate: u16,
        },
        /// F14: Root 紧急禁用 Entity 佣金
        CommissionForceDisabled {
            entity_id: u64,
        },
        /// F15: 全局佣金率上限已更新
        GlobalMaxCommissionRateSet {
            entity_id: u64,
            rate: u16,
        },
        /// F2: 提现冻结期已更新
        WithdrawalCooldownUpdated {
            entity_id: u64,
            nex_cooldown: u32,
            token_cooldown: u32,
        },
        /// F4: 佣金配置已清除
        CommissionConfigCleared {
            entity_id: u64,
        },
        /// F4: 提现配置已清除
        WithdrawalConfigCleared {
            entity_id: u64,
        },
        /// F4: Token 提现配置已清除
        TokenWithdrawalConfigCleared {
            entity_id: u64,
        },
        /// F16: 全局 Token 佣金率上限已更新
        GlobalMaxTokenCommissionRateSet {
            entity_id: u64,
            rate: u16,
        },
        /// F17: 全局佣金紧急暂停/恢复
        GlobalCommissionPauseToggled {
            paused: bool,
        },
        /// 推荐人免除阈值已更新（多级分销层数超过阈值的 Entity 推荐人不获得招商提成）
        ReferrerExemptThresholdChanged {
            old_threshold: u16,
            new_threshold: u16,
        },
        /// Entity 级推荐人佣金开关已切换
        ReferrerPayoutToggled {
            entity_id: u64,
            disabled: bool,
        },
        /// F18: Entity 级提现暂停/恢复
        WithdrawalPauseToggled {
            entity_id: u64,
            paused: bool,
        },
        /// F21: 订单佣金记录已归档清理
        OrderRecordsArchived {
            order_id: u64,
        },
        /// BUG-1 修复: 订单佣金记录已结算（Pending → Withdrawn）
        OrderRecordsSettled {
            order_id: u64,
        },
        /// BUG-3 修复: Root 重新启用 Entity 佣金
        CommissionForceEnabled {
            entity_id: u64,
        },
        /// MISSING-3: 最小提现间隔已更新
        MinWithdrawalIntervalUpdated {
            entity_id: u64,
            interval: u32,
        },
        /// P0-1 审计修复: 治理提案设置佣金率
        GovernanceCommissionRateSet {
            entity_id: u64,
            rate: u16,
        },
        /// P0-1 审计修复: 治理提案切换佣金开关
        GovernanceCommissionToggled {
            entity_id: u64,
            enabled: bool,
        },
        /// E-2 审计修复: NEX 沉淀池退款失败（entity_account 余额不足）
        UnallocatedPoolRefundFailed {
            entity_id: u64,
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// E-2 审计修复: Token 沉淀池退款失败
        TokenUnallocatedPoolRefundFailed {
            entity_id: u64,
            order_id: u64,
            amount: TokenBalanceOf<T>,
        },
        /// 店铺级佣金率覆盖已设置（None = 清除覆盖）
        ShopCommissionRateSet {
            entity_id: u64,
            shop_id: u64,
            rate: Option<u16>,
        },
        /// 产品级佣金率覆盖已设置（None = 清除覆盖）
        ProductCommissionRateSet {
            entity_id: u64,
            product_id: u64,
            rate: Option<u16>,
        },
        /// 插件独立预算上限已更新
        PluginBudgetCapsUpdated {
            entity_id: u64,
            caps: PluginBudgetCaps,
        },
        /// H-2 审计修复: 退款失败记入待重试队列（资金仍在 entity_account，stats 未扣减）
        RefundPendingCreated {
            entity_id: u64,
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// H-2 审计修复: 待重试退款成功处理
        RefundPendingResolved {
            entity_id: u64,
            order_id: u64,
            amount: BalanceOf<T>,
        },
        /// HIGH-1 审计修复: Token 待重试退款成功处理
        TokenRefundPendingResolved {
            order_id: u64,
            amount: TokenBalanceOf<T>,
        },
        /// HIGH-2 审计修复: Entity 所有未结清佣金/购物余额已强制清零
        EntityBalancesForfeited {
            entity_id: u64,
            nex_pending_forfeited: BalanceOf<T>,
            token_pending_forfeited: TokenBalanceOf<T>,
            nex_refund_forfeited: BalanceOf<T>,
            token_refund_forfeited: TokenBalanceOf<T>,
        },
        /// M-2 审计修复: 佣金记录容量耗尽，剩余佣金转入沉淀池
        CommissionRecordsCapacityExhausted {
            entity_id: u64,
            order_id: u64,
            dropped_outputs: u32,
            dropped_amount: BalanceOf<T>,
        },
        /// M-2 审计修复: Token 佣金记录容量耗尽
        TokenCommissionRecordsCapacityExhausted {
            entity_id: u64,
            order_id: u64,
            dropped_outputs: u32,
            dropped_amount: TokenBalanceOf<T>,
        },
        /// LOW-1 审计修复: NEX 提现历史已满，丢弃最旧记录
        WithdrawalHistoryTruncated {
            entity_id: u64,
            who: T::AccountId,
        },
        /// LOW-1 审计修复: Token 提现历史已满，丢弃最旧记录
        TokenWithdrawalHistoryTruncated {
            entity_id: u64,
            who: T::AccountId,
        },
        /// Owner 奖励直接到账（NEX）
        OwnerRewardPaid {
            entity_id: u64,
            order_id: u64,
            to: T::AccountId,
            amount: BalanceOf<T>,
        },
        /// Owner 奖励直接到账（Token）
        TokenOwnerRewardPaid {
            entity_id: u64,
            order_id: u64,
            to: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// 购物余额达到复购门槛，前端监听此事件自动提交复购订单
        RepurchaseReady {
            entity_id: u64,
            account: T::AccountId,
            shopping_balance: BalanceOf<T>,
            usdt_equivalent: u64,
            min_package_usdt: u64,
        },
        /// 强制复购配置已更新
        RepurchaseConfigUpdated {
            entity_id: u64,
        },
        /// 链上自动创建复购订单成功
        AutoRepurchaseCreated {
            entity_id: u64,
            account: T::AccountId,
            order_id: u64,
            product_id: u64,
        },
        /// 购物余额因超过 TTL 未使用而被没收（归 Entity 账户）
        ShoppingBalanceExpired {
            entity_id: u64,
            account: T::AccountId,
            amount: BalanceOf<T>,
        },
    }

    // ========================================================================
    // Errors
    // ========================================================================

    #[pallet::error]
    pub enum Error<T> {
        ShopNotFound,
        EntityNotFound,
        NotShopOwner,
        NotEntityOwner,
        CommissionNotConfigured,
        InsufficientCommission,
        InvalidCommissionRate,
        RecordsFull,
        Overflow,
        WithdrawalConfigNotEnabled,
        InvalidWithdrawalConfig,
        InsufficientShoppingBalance,
        /// 提现冻结期未满足
        WithdrawalCooldownNotMet,
        /// 提现金额不能为 0
        ZeroWithdrawalAmount,
        /// LevelBased 配置中 level_id 重复
        DuplicateLevelId,
        /// LevelBased 配置中 level_id 在 Entity 等级系统中不存在
        LevelIdNotFound,
        /// [已废弃] 保留以维持 Error 索引稳定
        MemberNotActivated,
        /// H3: 账户不满足 Entity 参与要求，无法提取购物余额
        ParticipationRequirementNotMet,
        /// 购物余额仅可用于购物（下单抵扣），不可直接提取为 NEX
        ShoppingBalanceWithdrawalDisabled,
        /// 沉淀资金池余额不足
        InsufficientUnallocatedPool,
        /// Token 佣金余额不足
        InsufficientTokenCommission,
        /// Token 转账失败
        TokenTransferFailed,
        /// Entity 账户 NEX 自由余额不足（提取后不能低于锁定总额）
        InsufficientEntityFunds,
        /// Entity 账户 Token 自由余额不足
        InsufficientEntityTokenFunds,
        /// 沉淀池资金非零，不可关闭 POOL_REWARD（须先让会员领取至清零）
        PoolNotEmpty,
        /// init_commission_plan 已禁用，请使用 utility.batch 组合分步 extrinsics
        CommissionPlanDisabled,
        /// Token 平台费率超过上限（最大 1000 bps = 10%）
        TokenPlatformFeeRateTooHigh,
        /// 实体已被全局锁定，所有配置操作不可用
        EntityLocked,
        /// R8: 锁定状态下仅允许降低（无币实体单调递减豁免）
        LockedOnlyDecreaseAllowed,
        /// F1: 调用者既不是 Entity Owner 也不是拥有 COMMISSION_MANAGE 权限的 Admin
        NotEntityOwnerOrAdmin,
        /// F15: Entity Owner 设置的 max_commission_rate 超过全局上限
        CommissionRateExceedsGlobalMax,
        /// F4: 佣金配置不存在，无法清除
        ConfigNotFound,
        /// F16: Entity Owner 设置的 Token max_commission_rate 超过全局 Token 上限
        TokenCommissionRateExceedsGlobalMax,
        /// F17: 全局佣金紧急暂停中，所有佣金操作不可用
        GlobalCommissionPaused,
        /// F18: Entity 级提现已暂停
        WithdrawalPausedByOwner,
        /// F4+: Entity 未处于活跃状态（配置类操作需要 Entity active）
        EntityNotActive,
        /// F21: 订单记录不存在或已归档
        OrderRecordsNotFound,
        /// F21: 订单佣金记录中存在未完结的记录（Pending/Distributed），不可归档
        OrderRecordsNotFinalized,
        /// MISSING-3: 提现间隔未满足（距上次提现间隔不足 MinWithdrawalInterval）
        WithdrawalIntervalNotMet,
        /// P0-2 审计修复: 同一订单不可重复处理佣金
        OrderAlreadyProcessed,
        /// P1-2 审计修复: 插件返回的 outputs 总额 + new_remaining ≠ old_remaining
        PluginOutputInvariantViolation,
        /// Shop 不属于该 Entity
        ShopNotInEntity,
        /// Product 不属于该 Entity 下的 Shop
        ProductNotInEntity,
        /// 插件预算上限值超过 10000
        InvalidPluginCap,
        /// 插件预算上限超过当前 max_commission_rate
        PluginCapExceedsCommissionRate,
        /// 推荐人佣金保护期内，不可关闭推荐人佣金
        ReferrerProtectionPeriodActive,
        /// H-2 审计修复: 该订单没有待重试的退款记录
        NoPendingRefund,
        /// H-1 审计修复: 启用 ≥2 个插件组时，已启用插件的 cap 必须 > 0
        PluginCapRequiredForMultiPlugin,
        /// 佣金率超出预算上限（max_commission_rate + platform_fee_rate > 10000）
        CommissionRateExceedsBudget,
        /// 强制复购配置无效（enforced=true 时 min_package_usdt 必须 > 0）
        InvalidRepurchaseConfig,
        /// auto_order=true 时未设置 default_product_id（必须 > 0）
        AutoOrderProductNotSet,
        /// auto_order product does not belong to this entity.
        /// 自动复购商品不属于本 Entity。
        AutoOrderProductNotInEntity,
        /// 购物余额尚未过期（TTL 未到或未设置 TTL）
        ShoppingBalanceNotExpired,
        /// 购物余额为零，无需触发过期
        ZeroShoppingBalance,
        /// 未设置强制复购配置，无法触发过期
        RepurchaseConfigNotSet,
        /// 购物余额 TTL 低于链上最小值（MinShoppingBalanceTtlBlocks）
        TtlTooShort,
        /// 购物余额（折算 USDT）超过阈值，不可领奖（需先消费购物余额）
        ShoppingBalanceExceedsThreshold,
        /// Token 购物余额（折算 USDT）超过阈值，不可领奖（需先消费 Token 购物余额）
        TokenShoppingBalanceExceedsThreshold,
        /// USDT 金额精度错误（非零值必须 >= 1_000_000，即 >= 1 USDT，防止漏乘 10^6）
        UsdtAmountTooSmall,
        /// HB-WD-01: cross-chain payout port not configured (`CrossChainPayout = ()`).
        /// HB-WD-01：跨链派发端口未接线（`CrossChainPayout = ()`）。
        CrossChainPayoutNotConfigured,
        /// HB-WD-01: the bridge rejected the payout (paused / per-tx or daily limit /
        /// chain not registered / below min / ED). pending is left untouched.
        /// HB-WD-01：桥拒绝派发（暂停 / 单笔或单日限额 / 未注册链 / 低于最小额 / ED）。
        /// pending 不动。
        CrossChainPayoutFailed,
        /// HB-WD-01: the bridged withdrawal part is zero (full repurchase), nothing
        /// to bridge — use `withdraw_commission` instead.
        /// HB-WD-01：桥出的 withdrawal 部分为 0（全额复购），无可桥出——请改用
        /// `withdraw_commission`。
        ZeroBridgeWithdrawal,
        /// HB-WD-01: EVM recipient address must not be the zero address.
        /// HB-WD-01：EVM 收款地址不能为零地址。
        InvalidEvmRecipient,
    }

    // ========================================================================
    // Extrinsics
    // ========================================================================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 设置启用的返佣模式（Entity 级）
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_commission_modes())]
        pub fn set_commission_modes(
            origin: OriginFor<T>,
            entity_id: u64,
            modes: CommissionModes,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(modes.is_valid(), Error::<T>::InvalidCommissionRate);

            // H-1 审计修复: 多插件组时要求已启用插件的 cap > 0
            let current_caps = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.plugin_caps)
                .unwrap_or_default();
            Self::validate_plugin_caps_for_modes(&modes, &current_caps)?;

            let old_has_pool = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.enabled_modes.contains(CommissionModes::POOL_REWARD))
                .unwrap_or(false);
            let new_has_pool = modes.contains(CommissionModes::POOL_REWARD);

            // 沉淀池非空时不允许关闭 POOL_REWARD
            if old_has_pool && !new_has_pool {
                ensure!(
                    UnallocatedPool::<T>::get(entity_id).is_zero()
                        && UnallocatedTokenPool::<T>::get(entity_id).is_zero(),
                    Error::<T>::PoolNotEmpty
                );
            }

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.enabled_modes = modes;
            });

            Self::deposit_event(Event::CommissionModesUpdated { entity_id, modes });
            Ok(())
        }

        /// 设置会员返佣上限（Entity 级，从卖家货款扣除）
        ///
        /// 仅 Entity Owner 可调用
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_commission_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            max_rate: u16,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(max_rate <= 10000, Error::<T>::InvalidCommissionRate);
            // 佣金预算上限约束: max_commission_rate + platform_fee_rate ≤ 10000
            ensure!(
                max_rate <= Self::commission_budget_ceiling(),
                Error::<T>::CommissionRateExceedsBudget
            );

            // F15: 全局佣金率上限校验
            let global_max = GlobalMaxCommissionRate::<T>::get(entity_id);
            if global_max > 0 {
                ensure!(
                    max_rate <= global_max,
                    Error::<T>::CommissionRateExceedsGlobalMax
                );
            }

            // BUG-2 修复: Token 全局佣金率上限校验（NEX/Token 共用 max_commission_rate）
            let token_global_max = GlobalMaxTokenCommissionRate::<T>::get(entity_id);
            if token_global_max > 0 {
                ensure!(
                    max_rate <= token_global_max,
                    Error::<T>::TokenCommissionRateExceedsGlobalMax
                );
            }

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.max_commission_rate = max_rate;
            });

            Self::deposit_event(Event::CommissionConfigUpdated { entity_id });
            Ok(())
        }

        /// 启用/禁用返佣（Entity 级）
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::enable_commission())]
        pub fn enable_commission(
            origin: OriginFor<T>,
            entity_id: u64,
            enabled: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // H-1 审计修复: 启用时校验 plugin_caps 与 enabled_modes 一致性
            if enabled {
                let effective = CommissionConfigs::<T>::get(entity_id).unwrap_or_default();
                Self::validate_plugin_caps_for_modes(
                    &effective.enabled_modes,
                    &effective.plugin_caps,
                )?;
            }

            // 沉淀池非空时不允许关闭 POOL_REWARD（禁用 commission 等效于关闭 POOL_REWARD）
            let old_pool_on = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.enabled && c.enabled_modes.contains(CommissionModes::POOL_REWARD))
                .unwrap_or(false);

            if !enabled && old_pool_on {
                ensure!(
                    UnallocatedPool::<T>::get(entity_id).is_zero()
                        && UnallocatedTokenPool::<T>::get(entity_id).is_zero(),
                    Error::<T>::PoolNotEmpty
                );
            }

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.enabled = enabled;
            });

            Self::deposit_event(Event::CommissionConfigUpdated { entity_id });
            Ok(())
        }

        /// 提取返佣（四种提现模式 + 自愿复购奖励，Entity 级佣金池）
        ///
        /// - `entity_id`: Entity ID（佣金统一在 Entity 级记账）
        /// - `amount`: 提现金额（None = 全部 pending）
        /// - `requested_repurchase_rate`: 会员请求的复购比率（万分比，MemberChoice 模式下使用）
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::withdraw_commission())]
        pub fn withdraw_commission(
            origin: OriginFor<T>,
            entity_id: u64,
            amount: Option<BalanceOf<T>>,
            requested_repurchase_rate: Option<u16>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // CRITICAL-1 审计修复: Entity 非 Active 状态下禁止提现
            // 防止 PendingClose/Closed/Suspended/Banned 状态下资金流出
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // F17: 全局紧急暂停检查
            ensure!(
                !GlobalCommissionPaused::<T>::get(),
                Error::<T>::GlobalCommissionPaused
            );
            // F18: Entity 级提现暂停检查
            ensure!(
                !WithdrawalPaused::<T>::get(entity_id),
                Error::<T>::WithdrawalPausedByOwner
            );

            // H1 审计修复: NEX 提现也需检查参与权（与 Token 提现一致）
            ensure!(
                T::ParticipationGuard::can_participate(entity_id, &who),
                Error::<T>::ParticipationRequirementNotMet
            );

            MemberCommissionStats::<T>::try_mutate(entity_id, &who, |stats| -> DispatchResult {
                let total_amount = amount.unwrap_or(stats.pending);
                ensure!(
                    stats.pending >= total_amount,
                    Error::<T>::InsufficientCommission
                );
                ensure!(!total_amount.is_zero(), Error::<T>::ZeroWithdrawalAmount);

                // P0-8 审计修复: 先做全部校验（cooldown/interval/config），再做可能产生副作用的注册
                // 原顺序：先 auto_register → 后 cooldown 检查，导致失败提现可能白创建会员

                // H1 审计修复: 提现前检查 WithdrawalConfig 是否启用
                // RISK-3 审计说明: 未配置 WithdrawalConfig 时允许全额提现（默认行为），
                // 但 Governance 的 GlobalMinRepurchaseRate 底线仍强制生效——
                // 如需强制复购，运维应确保 Governance 设置了 GlobalMinRepurchaseRate。
                let withdrawal_config = WithdrawalConfigs::<T>::get(entity_id);
                if let Some(ref wc) = withdrawal_config {
                    ensure!(wc.enabled, Error::<T>::WithdrawalConfigNotEnabled);
                }

                // 冻结期检查
                if let Some(config) = CommissionConfigs::<T>::get(entity_id) {
                    if config.withdrawal_cooldown > 0 {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let last_credited = MemberLastCredited::<T>::get(entity_id, &who);
                        let cooldown: BlockNumberFor<T> = config.withdrawal_cooldown.into();
                        let earliest = last_credited.saturating_add(cooldown);
                        ensure!(now >= earliest, Error::<T>::WithdrawalCooldownNotMet);
                    }
                }

                // MISSING-3: 提现频率限制检查（基于上次提现时间，与 cooldown 独立）
                let min_interval = MinWithdrawalInterval::<T>::get(entity_id);
                if min_interval > 0 {
                    let now = <frame_system::Pallet<T>>::block_number();
                    let last_withdrawn = MemberLastWithdrawn::<T>::get(entity_id, &who);
                    if !last_withdrawn.is_zero() {
                        let interval: BlockNumberFor<T> = min_interval.into();
                        ensure!(
                            now >= last_withdrawn.saturating_add(interval),
                            Error::<T>::WithdrawalIntervalNotMet
                        );
                    }
                }

                // 购物余额超出 USDT 阈值时阻止领奖
                if let Some(ref rc) = RepurchaseConfigs::<T>::get(entity_id) {
                    if rc.max_shopping_balance_usdt > 0 {
                        let shopping_bal: u128 =
                            T::Loyalty::shopping_balance(entity_id, &who).saturated_into();
                        let nex_usdt_rate = T::PricingProvider::get_nex_usdt_price();
                        let bal_usdt = pallet_commission_common::shopping_bal_to_usdt(
                            shopping_bal,
                            nex_usdt_rate,
                        );
                        ensure!(
                            bal_usdt <= rc.max_shopping_balance_usdt,
                            Error::<T>::ShoppingBalanceExceedsThreshold
                        );
                    }
                }

                // 计算提现/复购/奖励分配
                let split = Self::calc_withdrawal_split(
                    entity_id,
                    &who,
                    total_amount,
                    requested_repurchase_rate,
                );

                // C1 审计修复: 偿付安全检查必须计入 repurchase+bonus 对购物余额总额的增量
                // 提现后状态: pending -= total_amount, shopping += repurchase + bonus, entity -= withdrawal
                // 需要: entity_balance - withdrawal >= (old_pending - total_amount) + (old_shopping + repurchase + bonus)
                let entity_account = T::EntityProvider::entity_account(entity_id);
                let entity_balance = T::Currency::free_balance(&entity_account);
                let remaining_pending =
                    ShopPendingTotal::<T>::get(entity_id).saturating_sub(total_amount);
                let total_to_shopping = split.repurchase.saturating_add(split.bonus);
                let new_shopping_total =
                    T::Loyalty::shopping_total(entity_id).saturating_add(total_to_shopping);
                let unallocated = UnallocatedPool::<T>::get(entity_id);
                // H-2 审计修复: 偿付检查须计入待重试退款额，防止被其他提现消耗
                let pending_refund = PendingRefundTotal::<T>::get(entity_id);
                let required_reserve = remaining_pending
                    .saturating_add(new_shopping_total)
                    .saturating_add(unallocated)
                    .saturating_add(pending_refund);
                // P0-FIX: 添加 min_balance 和 min_threshold 保护
                let min_balance = T::Currency::minimum_balance();
                let min_threshold: BalanceOf<T> =
                    T::FundProtectionQuery::min_treasury_threshold(entity_id).saturated_into();
                let required_balance = split
                    .withdrawal
                    .saturating_add(required_reserve)
                    .saturating_add(min_balance)
                    .saturating_add(min_threshold);
                // M2 审计修复: 使用专用错误码区分"pending 不足"和"Entity 偿付能力不足"
                ensure!(
                    entity_balance >= required_balance,
                    Error::<T>::InsufficientEntityFunds
                );

                // 从 Entity 账户转账提现部分到用户钱包
                if !split.withdrawal.is_zero() {
                    T::Currency::transfer(
                        &entity_account,
                        &who,
                        split.withdrawal,
                        ExistenceRequirement::KeepAlive,
                    )?;
                }

                // 复购部分 + 奖励 转入账户的购物余额（通过 Loyalty 模块）
                if !total_to_shopping.is_zero() {
                    T::Loyalty::credit_shopping_balance(entity_id, &who, total_to_shopping)?;
                }

                // 检查购物余额是否达到复购门槛
                Self::check_repurchase_ready(entity_id, &who);

                // 统计记在出资人名下
                stats.pending = stats.pending.saturating_sub(total_amount);
                stats.withdrawn = stats.withdrawn.saturating_add(split.withdrawal);
                // H3 审计修复: repurchased 应包含 bonus（两者均进入购物余额）
                stats.repurchased = stats
                    .repurchased
                    .saturating_add(split.repurchase)
                    .saturating_add(split.bonus);

                // 释放 pending 锁定
                ShopPendingTotal::<T>::mutate(entity_id, |total| {
                    *total = total.saturating_sub(total_amount);
                });

                // MISSING-3: 记录最后提现时间
                let now = <frame_system::Pallet<T>>::block_number();
                MemberLastWithdrawn::<T>::insert(entity_id, &who, now);

                // F20: 记录 NEX 提现历史（满则丢弃最旧）
                let mut truncated = false;
                MemberWithdrawalHistory::<T>::mutate(entity_id, &who, |history| {
                    let record = WithdrawalRecord {
                        total_amount,
                        withdrawn: split.withdrawal,
                        repurchased: split.repurchase,
                        bonus: split.bonus,
                        block_number: now,
                    };
                    if history.try_push(record.clone()).is_err() {
                        if !history.is_empty() {
                            history.remove(0);
                        }
                        let _ = history.try_push(record);
                        truncated = true;
                    }
                });
                if truncated {
                    Self::deposit_event(Event::WithdrawalHistoryTruncated {
                        entity_id,
                        who: who.clone(),
                    });
                }

                // 发出事件
                Self::deposit_event(Event::TieredWithdrawal {
                    entity_id,
                    account: who.clone(),
                    withdrawn_amount: split.withdrawal,
                    repurchase_amount: split.repurchase,
                    bonus_amount: split.bonus,
                });

                Ok(())
            })
        }

        /// Cross-chain commission withdrawal (HB-WD-01): bridge the `withdrawal`
        /// part of a tiered withdrawal out to an EVM chain as native NEX, while the
        /// `repurchase` + `bonus` parts stay on Nexus (credited to the shopping
        /// balance, exactly as `withdraw_commission`). Reuses every local-withdrawal
        /// guardrail (Entity Active / global+entity pause / cooldown / interval /
        /// participation / solvency) and `calc_withdrawal_split`; only the
        /// disposition of the withdrawal part changes (bridge burn + ISMP POST via
        /// `CrossChainPayout` instead of a local transfer). On dispatch failure the
        /// whole extrinsic rolls back (pending untouched); on bridge timeout the
        /// bridge mints the NEX back to the Entity account (replenishing the solvency
        /// pool). The promoter receives native NEX on the EVM chain and self-swaps to
        /// a stablecoin.
        /// 跨链佣金提现（HB-WD-01）：把分级提现的 `withdrawal` 部分作为原生 NEX 桥出到某
        /// EVM 链，而 `repurchase` + `bonus` 仍留 Nexus（计入购物余额，与
        /// `withdraw_commission` 完全一致）。复用本地提现的全部护栏（Entity Active /
        /// 全局+entity 暂停 / cooldown / 频率 / 参与权 / 偿付）与 `calc_withdrawal_split`；
        /// 仅改变 withdrawal 部分的去向（经 `CrossChainPayout` 桥 burn + ISMP POST，而非本地
        /// 转账）。派发失败时整笔回滚（pending 不动）；桥超时则由桥把 NEX 铸回 Entity 账户
        ///（回补偿付池）。推广员在 EVM 链收到原生 NEX，自行换稳定币。
        ///
        /// - `entity_id`: Entity ID
        /// - `amount`: 提现金额（None = 全部 pending）
        /// - `requested_repurchase_rate`: 会员请求的复购比率（万分比，MemberChoice 模式下使用）
        /// - `evm_chain_id`: 目标 EVM 链 id（治理须已 `register_chain`）
        /// - `recipient`: 推广员 EVM 收款地址（20 字节，不可为零地址）
        #[pallet::call_index(40)]
        #[pallet::weight(T::WeightInfo::withdraw_commission())]
        pub fn withdraw_commission_to_evm(
            origin: OriginFor<T>,
            entity_id: u64,
            amount: Option<BalanceOf<T>>,
            requested_repurchase_rate: Option<u16>,
            evm_chain_id: u32,
            recipient: [u8; 20],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(recipient != [0u8; 20], Error::<T>::InvalidEvmRecipient);

            // 与 withdraw_commission 一致的前置护栏
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !GlobalCommissionPaused::<T>::get(),
                Error::<T>::GlobalCommissionPaused
            );
            ensure!(
                !WithdrawalPaused::<T>::get(entity_id),
                Error::<T>::WithdrawalPausedByOwner
            );
            ensure!(
                T::ParticipationGuard::can_participate(entity_id, &who),
                Error::<T>::ParticipationRequirementNotMet
            );

            MemberCommissionStats::<T>::try_mutate(entity_id, &who, |stats| -> DispatchResult {
                let total_amount = amount.unwrap_or(stats.pending);
                ensure!(
                    stats.pending >= total_amount,
                    Error::<T>::InsufficientCommission
                );
                ensure!(!total_amount.is_zero(), Error::<T>::ZeroWithdrawalAmount);

                let withdrawal_config = WithdrawalConfigs::<T>::get(entity_id);
                if let Some(ref wc) = withdrawal_config {
                    ensure!(wc.enabled, Error::<T>::WithdrawalConfigNotEnabled);
                }

                // 冻结期检查
                if let Some(config) = CommissionConfigs::<T>::get(entity_id) {
                    if config.withdrawal_cooldown > 0 {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let last_credited = MemberLastCredited::<T>::get(entity_id, &who);
                        let cooldown: BlockNumberFor<T> = config.withdrawal_cooldown.into();
                        let earliest = last_credited.saturating_add(cooldown);
                        ensure!(now >= earliest, Error::<T>::WithdrawalCooldownNotMet);
                    }
                }

                // 提现频率限制
                let min_interval = MinWithdrawalInterval::<T>::get(entity_id);
                if min_interval > 0 {
                    let now = <frame_system::Pallet<T>>::block_number();
                    let last_withdrawn = MemberLastWithdrawn::<T>::get(entity_id, &who);
                    if !last_withdrawn.is_zero() {
                        let interval: BlockNumberFor<T> = min_interval.into();
                        ensure!(
                            now >= last_withdrawn.saturating_add(interval),
                            Error::<T>::WithdrawalIntervalNotMet
                        );
                    }
                }

                // 购物余额超出 USDT 阈值时阻止领奖
                if let Some(ref rc) = RepurchaseConfigs::<T>::get(entity_id) {
                    if rc.max_shopping_balance_usdt > 0 {
                        let shopping_bal: u128 =
                            T::Loyalty::shopping_balance(entity_id, &who).saturated_into();
                        let nex_usdt_rate = T::PricingProvider::get_nex_usdt_price();
                        let bal_usdt = pallet_commission_common::shopping_bal_to_usdt(
                            shopping_bal,
                            nex_usdt_rate,
                        );
                        ensure!(
                            bal_usdt <= rc.max_shopping_balance_usdt,
                            Error::<T>::ShoppingBalanceExceedsThreshold
                        );
                    }
                }

                // 计算提现/复购/奖励分配（与本地提现完全一致）
                let split = Self::calc_withdrawal_split(
                    entity_id,
                    &who,
                    total_amount,
                    requested_repurchase_rate,
                );

                // 跨链提现必须有可桥出的 withdrawal 部分（全额复购时改用本地 withdraw_commission）
                ensure!(
                    !split.withdrawal.is_zero(),
                    Error::<T>::ZeroBridgeWithdrawal
                );

                // 偿付安全检查（与 withdraw_commission 同：跨链 burn 与本地转账对 entity_balance
                // 的影响相同，均为减少 withdrawal）
                let entity_account = T::EntityProvider::entity_account(entity_id);
                let entity_balance = T::Currency::free_balance(&entity_account);
                let remaining_pending =
                    ShopPendingTotal::<T>::get(entity_id).saturating_sub(total_amount);
                let total_to_shopping = split.repurchase.saturating_add(split.bonus);
                let new_shopping_total =
                    T::Loyalty::shopping_total(entity_id).saturating_add(total_to_shopping);
                let unallocated = UnallocatedPool::<T>::get(entity_id);
                let pending_refund = PendingRefundTotal::<T>::get(entity_id);
                let required_reserve = remaining_pending
                    .saturating_add(new_shopping_total)
                    .saturating_add(unallocated)
                    .saturating_add(pending_refund);
                let min_balance = T::Currency::minimum_balance();
                let min_threshold: BalanceOf<T> =
                    T::FundProtectionQuery::min_treasury_threshold(entity_id).saturated_into();
                let required_balance = split
                    .withdrawal
                    .saturating_add(required_reserve)
                    .saturating_add(min_balance)
                    .saturating_add(min_threshold);
                ensure!(
                    entity_balance >= required_balance,
                    Error::<T>::InsufficientEntityFunds
                );

                // withdrawal 部分：经 CrossChainPayout 从 entity_account burn 并派发 ISMP POST。
                // 失败时返回错误 → extrinsic 事务层回滚（burn / pending / 购物余额全部回滚）。
                let commitment = <T::CrossChainPayout as crate::CrossChainPayout<
                    T::AccountId,
                    BalanceOf<T>,
                >>::payout_native(
                    &entity_account, evm_chain_id, recipient, split.withdrawal
                )
                .map_err(|e| match e {
                    DispatchError::Other("cross-chain payout not configured") => {
                        Error::<T>::CrossChainPayoutNotConfigured
                    }
                    _ => Error::<T>::CrossChainPayoutFailed,
                })?;

                // repurchase + bonus 仍留 Nexus（计入购物余额，与本地提现一致）
                if !total_to_shopping.is_zero() {
                    T::Loyalty::credit_shopping_balance(entity_id, &who, total_to_shopping)?;
                }

                // 检查购物余额是否达到复购门槛
                Self::check_repurchase_ready(entity_id, &who);

                // 统计记在出资人名下
                stats.pending = stats.pending.saturating_sub(total_amount);
                stats.withdrawn = stats.withdrawn.saturating_add(split.withdrawal);
                stats.repurchased = stats
                    .repurchased
                    .saturating_add(split.repurchase)
                    .saturating_add(split.bonus);

                // 释放 pending 锁定
                ShopPendingTotal::<T>::mutate(entity_id, |total| {
                    *total = total.saturating_sub(total_amount);
                });

                // 记录最后提现时间
                let now = <frame_system::Pallet<T>>::block_number();
                MemberLastWithdrawn::<T>::insert(entity_id, &who, now);

                // 记录提现历史（满则丢弃最旧）
                let mut truncated = false;
                MemberWithdrawalHistory::<T>::mutate(entity_id, &who, |history| {
                    let record = WithdrawalRecord {
                        total_amount,
                        withdrawn: split.withdrawal,
                        repurchased: split.repurchase,
                        bonus: split.bonus,
                        block_number: now,
                    };
                    if history.try_push(record.clone()).is_err() {
                        if !history.is_empty() {
                            history.remove(0);
                        }
                        let _ = history.try_push(record);
                        truncated = true;
                    }
                });
                if truncated {
                    Self::deposit_event(Event::WithdrawalHistoryTruncated {
                        entity_id,
                        who: who.clone(),
                    });
                }

                Self::deposit_event(Event::CommissionBridgedOut {
                    entity_id,
                    account: who.clone(),
                    evm_chain_id,
                    recipient,
                    withdrawn_amount: split.withdrawal,
                    repurchase_amount: split.repurchase,
                    bonus_amount: split.bonus,
                    commitment,
                });

                Ok(())
            })
        }

        /// 设置提现配置（Entity 级，四种模式 + 自愿复购奖励）
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_withdrawal_config())]
        pub fn set_withdrawal_config(
            origin: OriginFor<T>,
            entity_id: u64,
            mode: WithdrawalMode,
            default_tier: WithdrawalTierConfig,
            level_overrides: BoundedVec<(u8, WithdrawalTierConfig), T::MaxCustomLevels>,
            voluntary_bonus_rate: u16,
            enabled: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // 校验模式参数
            match &mode {
                WithdrawalMode::FixedRate { repurchase_rate } => {
                    ensure!(
                        *repurchase_rate <= 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                }
                WithdrawalMode::MemberChoice {
                    min_repurchase_rate,
                } => {
                    ensure!(
                        *min_repurchase_rate <= 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                }
                _ => {}
            }

            // 校验 LevelBased 配置（即使非 LevelBased 模式也允许预配置）
            ensure!(
                default_tier
                    .withdrawal_rate
                    .saturating_add(default_tier.repurchase_rate)
                    == 10000,
                Error::<T>::InvalidWithdrawalConfig
            );
            {
                let mut seen_ids = alloc::collections::BTreeSet::new();
                for (level_id, tier) in level_overrides.iter() {
                    ensure!(seen_ids.insert(*level_id), Error::<T>::DuplicateLevelId);
                    ensure!(
                        tier.withdrawal_rate.saturating_add(tier.repurchase_rate) == 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                    // 校验 level_id 在 Entity 等级系统中存在（LevelBased 模式强制，其他模式跳过）
                    if mode == WithdrawalMode::LevelBased {
                        ensure!(
                            T::MemberProvider::level_id_exists(entity_id, *level_id),
                            Error::<T>::LevelIdNotFound
                        );
                    }
                }
            }

            // 校验 bonus rate
            ensure!(
                voluntary_bonus_rate <= 10000,
                Error::<T>::InvalidWithdrawalConfig
            );

            WithdrawalConfigs::<T>::insert(
                entity_id,
                EntityWithdrawalConfig {
                    mode,
                    default_tier,
                    level_overrides,
                    voluntary_bonus_rate,
                    enabled,
                },
            );

            Self::deposit_event(Event::WithdrawalConfigUpdated { entity_id });
            Ok(())
        }

        /// [已禁用] init_commission_plan 过度设计，前端改用 utility.batch 组合：
        /// set_commission_modes + set_commission_rate + enable_commission + 各插件配置 extrinsics。
        ///
        /// 保留 call_index(6) 以维持链上 call index 稳定。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::init_commission_plan())]
        pub fn init_commission_plan(origin: OriginFor<T>, _entity_id: u64) -> DispatchResult {
            ensure_signed(origin)?;
            Err(Error::<T>::CommissionPlanDisabled.into())
        }

        /// [已禁用] 购物余额仅可用于购物（place_order 下单抵扣），不可直接提取为 NEX。
        ///
        /// 保留 call_index(5) 以维持链上 call index 稳定。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::use_shopping_balance())]
        pub fn use_shopping_balance(
            origin: OriginFor<T>,
            _entity_id: u64,
            _amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure_signed(origin)?;
            Err(Error::<T>::ShoppingBalanceWithdrawalDisabled.into())
        }

        /// 设置 Token 提现配置（Entity 级，四种模式 + 自愿复购奖励）
        ///
        /// 与 NEX set_withdrawal_config 完全对称，配置独立存储在 TokenWithdrawalConfigs
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::set_token_withdrawal_config())]
        pub fn set_token_withdrawal_config(
            origin: OriginFor<T>,
            entity_id: u64,
            mode: WithdrawalMode,
            default_tier: WithdrawalTierConfig,
            level_overrides: BoundedVec<(u8, WithdrawalTierConfig), T::MaxCustomLevels>,
            voluntary_bonus_rate: u16,
            enabled: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // 校验模式参数
            match &mode {
                WithdrawalMode::FixedRate { repurchase_rate } => {
                    ensure!(
                        *repurchase_rate <= 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                }
                WithdrawalMode::MemberChoice {
                    min_repurchase_rate,
                } => {
                    ensure!(
                        *min_repurchase_rate <= 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                }
                _ => {}
            }

            // H2 审计修复: 与 NEX set_withdrawal_config 一致的 tier 和验证
            ensure!(
                default_tier
                    .withdrawal_rate
                    .saturating_add(default_tier.repurchase_rate)
                    == 10000,
                Error::<T>::InvalidWithdrawalConfig
            );

            // M3 审计修复: 使用 BTreeSet O(n log n) 替代 O(n²) 重复检查
            {
                let mut seen_ids = alloc::collections::BTreeSet::new();
                for (level_id, tier) in level_overrides.iter() {
                    ensure!(seen_ids.insert(*level_id), Error::<T>::DuplicateLevelId);
                    ensure!(
                        tier.withdrawal_rate.saturating_add(tier.repurchase_rate) == 10000,
                        Error::<T>::InvalidWithdrawalConfig
                    );
                    // 校验 level_id 在 Entity 等级系统中存在（LevelBased 模式强制，其他模式跳过）
                    if mode == WithdrawalMode::LevelBased {
                        ensure!(
                            T::MemberProvider::level_id_exists(entity_id, *level_id),
                            Error::<T>::LevelIdNotFound
                        );
                    }
                }
            }

            // 校验 bonus rate
            ensure!(
                voluntary_bonus_rate <= 10000,
                Error::<T>::InvalidWithdrawalConfig
            );

            TokenWithdrawalConfigs::<T>::insert(
                entity_id,
                EntityWithdrawalConfig {
                    mode,
                    default_tier,
                    level_overrides,
                    voluntary_bonus_rate,
                    enabled,
                },
            );

            Self::deposit_event(Event::TokenWithdrawalConfigUpdated { entity_id });
            Ok(())
        }

        /// 设置 Token 全局最低复购比例（Root，Governance 底线）
        ///
        /// Token 提现时实际复购比例 = max(entity 分层配置, 此底线)
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::set_global_min_token_repurchase_rate())]
        pub fn set_global_min_token_repurchase_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            rate: u16,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(rate <= 10000, Error::<T>::InvalidCommissionRate);
            GlobalMinTokenRepurchaseRate::<T>::insert(entity_id, rate);
            Self::deposit_event(Event::GlobalMinTokenRepurchaseRateSet { entity_id, rate });
            Ok(())
        }

        /// Token 佣金提现（四种提现模式 + 复购分流 + 自愿复购奖励）
        ///
        /// - `entity_id`: Entity ID
        /// - `amount`: 提现金额（None = 全部 pending）
        /// - `requested_repurchase_rate`: 会员请求的复购比率（万分比，MemberChoice 模式下使用）
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::withdraw_token_commission())]
        pub fn withdraw_token_commission(
            origin: OriginFor<T>,
            entity_id: u64,
            amount: Option<TokenBalanceOf<T>>,
            requested_repurchase_rate: Option<u16>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // CRITICAL-1 审计修复: Entity 非 Active 状态下禁止提现
            // 防止 PendingClose/Closed/Suspended/Banned 状态下资金流出
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // F17: 全局紧急暂停检查
            ensure!(
                !GlobalCommissionPaused::<T>::get(),
                Error::<T>::GlobalCommissionPaused
            );
            // F18: Entity 级提现暂停检查
            ensure!(
                !WithdrawalPaused::<T>::get(entity_id),
                Error::<T>::WithdrawalPausedByOwner
            );

            // H1 审计修复: Token 提现也需检查参与权（与 NEX 提现一致）
            ensure!(
                T::ParticipationGuard::can_participate(entity_id, &who),
                Error::<T>::ParticipationRequirementNotMet
            );

            MemberTokenCommissionStats::<T>::try_mutate(
                entity_id,
                &who,
                |stats| -> DispatchResult {
                    let total_amount = amount.unwrap_or(stats.pending);
                    ensure!(
                        stats.pending >= total_amount,
                        Error::<T>::InsufficientTokenCommission
                    );
                    ensure!(!total_amount.is_zero(), Error::<T>::ZeroWithdrawalAmount);

                    // P0-8 审计修复: 先做全部校验（config/cooldown/interval），再做可能产生副作用的赠与注册
                    // 与 NEX withdraw_commission 保持一致，防止失败提现白创建会员

                    // Token 提现配置启用检查
                    // RISK-3 审计说明: 与 NEX 对称，未配置时允许全额提现，
                    // Governance 的 GlobalMinTokenRepurchaseRate 底线仍强制生效。
                    let token_wc = TokenWithdrawalConfigs::<T>::get(entity_id);
                    if let Some(ref wc) = token_wc {
                        ensure!(wc.enabled, Error::<T>::WithdrawalConfigNotEnabled);
                    }

                    // P3 审计修复: Token 冻结期使用独立的 MemberTokenLastCredited
                    // F3: 使用独立的 token_withdrawal_cooldown（0 = 回退到 withdrawal_cooldown）
                    if let Some(config) = CommissionConfigs::<T>::get(entity_id) {
                        let effective_cooldown = if config.token_withdrawal_cooldown > 0 {
                            config.token_withdrawal_cooldown
                        } else {
                            config.withdrawal_cooldown
                        };
                        if effective_cooldown > 0 {
                            let now = <frame_system::Pallet<T>>::block_number();
                            let last = MemberTokenLastCredited::<T>::get(entity_id, &who);
                            let cooldown: BlockNumberFor<T> = effective_cooldown.into();
                            ensure!(
                                now >= last.saturating_add(cooldown),
                                Error::<T>::WithdrawalCooldownNotMet
                            );
                        }
                    }

                    // MISSING-3: Token 提现频率限制检查
                    let min_interval = MinWithdrawalInterval::<T>::get(entity_id);
                    if min_interval > 0 {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let last_withdrawn = MemberTokenLastWithdrawn::<T>::get(entity_id, &who);
                        if !last_withdrawn.is_zero() {
                            let interval: BlockNumberFor<T> = min_interval.into();
                            ensure!(
                                now >= last_withdrawn.saturating_add(interval),
                                Error::<T>::WithdrawalIntervalNotMet
                            );
                        }
                    }

                    // Token 购物余额超出 USDT 阈值时阻止领奖
                    if let Some(ref rc) = RepurchaseConfigs::<T>::get(entity_id) {
                        if rc.max_shopping_balance_usdt > 0 {
                            let token_shopping_bal: u128 =
                                T::LoyaltyToken::token_shopping_balance(entity_id, &who)
                                    .saturated_into();
                            if !token_shopping_bal.is_zero() {
                                // Token 价格不可用时保守阻止（迫使 Entity 配置有效价格）
                                let token_usdt_rate =
                                    T::TokenPriceProvider::get_token_price_usdt(entity_id)
                                        .unwrap_or(u64::MAX);
                                let bal_usdt = pallet_commission_common::token_shopping_bal_to_usdt(
                                    token_shopping_bal,
                                    token_usdt_rate,
                                );
                                ensure!(
                                    bal_usdt <= rc.max_shopping_balance_usdt,
                                    Error::<T>::TokenShoppingBalanceExceedsThreshold
                                );
                            }
                        }
                    }

                    // 计算 Token 提现/复购/奖励分配
                    let split = Self::calc_token_withdrawal_split(
                        entity_id,
                        &who,
                        total_amount,
                        requested_repurchase_rate,
                    );

                    // Token 偿付安全检查: entity_token_balance >= withdrawal + pending_remaining + shopping_new + unallocated_pool
                    let entity_account = T::EntityProvider::entity_account(entity_id);
                    let entity_token_balance =
                        T::TokenTransferProvider::token_balance_of(entity_id, &entity_account);
                    let remaining_pending =
                        TokenPendingTotal::<T>::get(entity_id).saturating_sub(total_amount);
                    let total_to_shopping = split.repurchase.saturating_add(split.bonus);
                    let new_shopping_total = T::LoyaltyToken::token_shopping_total(entity_id)
                        .saturating_add(total_to_shopping);
                    let unallocated = UnallocatedTokenPool::<T>::get(entity_id);
                    // HIGH-1 审计修复: Token 偿付检查须计入待重试退款额（与 NEX PendingRefundTotal 对称）
                    let pending_token_refund = PendingTokenRefundTotal::<T>::get(entity_id);
                    let required_reserve = remaining_pending
                        .saturating_add(new_shopping_total)
                        .saturating_add(unallocated)
                        .saturating_add(pending_token_refund);
                    // M2 审计修复: 使用专用错误码区分"pending 不足"和"Entity Token 偿付能力不足"
                    ensure!(
                        entity_token_balance >= split.withdrawal.saturating_add(required_reserve),
                        Error::<T>::InsufficientEntityTokenFunds
                    );

                    // Token 转账: entity_account → who（提现部分）
                    if !split.withdrawal.is_zero() {
                        T::TokenTransferProvider::token_transfer(
                            entity_id,
                            &entity_account,
                            &who,
                            split.withdrawal,
                        )
                        .map_err(|_| Error::<T>::TokenTransferFailed)?;
                        EntityTokenAccountedBalance::<T>::mutate(entity_id, |b| {
                            *b = Some(b.unwrap_or_default().saturating_sub(split.withdrawal));
                        });
                    }

                    // 复购部分 + 奖励 → 账户的 Token 购物余额
                    if !total_to_shopping.is_zero() {
                        T::LoyaltyToken::credit_token_shopping_balance(
                            entity_id,
                            &who,
                            total_to_shopping,
                        )?;
                    }

                    // 统计记在出资人名下
                    stats.pending = stats.pending.saturating_sub(total_amount);
                    stats.withdrawn = stats.withdrawn.saturating_add(split.withdrawal);
                    stats.repurchased = stats
                        .repurchased
                        .saturating_add(split.repurchase)
                        .saturating_add(split.bonus);

                    // 释放 pending 锁定
                    TokenPendingTotal::<T>::mutate(entity_id, |total| {
                        *total = total.saturating_sub(total_amount);
                    });

                    // MISSING-3: 记录最后 Token 提现时间
                    let now = <frame_system::Pallet<T>>::block_number();
                    MemberTokenLastWithdrawn::<T>::insert(entity_id, &who, now);

                    // F20: 记录 Token 提现历史（满则丢弃最旧）
                    let mut truncated = false;
                    MemberTokenWithdrawalHistory::<T>::mutate(entity_id, &who, |history| {
                        let record = WithdrawalRecord {
                            total_amount,
                            withdrawn: split.withdrawal,
                            repurchased: split.repurchase,
                            bonus: split.bonus,
                            block_number: now,
                        };
                        if history.try_push(record.clone()).is_err() {
                            if !history.is_empty() {
                                history.remove(0);
                            }
                            let _ = history.try_push(record);
                            truncated = true;
                        }
                    });
                    if truncated {
                        Self::deposit_event(Event::TokenWithdrawalHistoryTruncated {
                            entity_id,
                            who: who.clone(),
                        });
                    }

                    Self::deposit_event(Event::TokenTieredWithdrawal {
                        entity_id,
                        account: who.clone(),
                        withdrawn_amount: split.withdrawal,
                        repurchase_amount: split.repurchase,
                        bonus_amount: split.bonus,
                    });

                    Ok(())
                },
            )
        }

        /// Entity Owner 提取 entity_account 中的可用 NEX 自由余额
        ///
        /// 提取后 entity_balance 必须 ≥ PendingTotal + ShoppingTotal + UnallocatedPool + PendingRefundTotal + min_balance
        /// 沉淀池（UnallocatedPool）始终受保护，不可被提取
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::withdraw_entity_funds())]
        pub fn withdraw_entity_funds(
            origin: OriginFor<T>,
            entity_id: u64,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_entity_owner(entity_id, &who)?;
            // MEDIUM-1 审计修复: Closed/Banned Entity 不允许 Owner 提取资金
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(!amount.is_zero(), Error::<T>::ZeroWithdrawalAmount);

            let entity_account = T::EntityProvider::entity_account(entity_id);
            let entity_balance = T::Currency::free_balance(&entity_account);

            // 沉淀池始终受保护（不可被 Owner 提取）
            let reserved = ShopPendingTotal::<T>::get(entity_id)
                .saturating_add(T::Loyalty::shopping_total(entity_id))
                .saturating_add(UnallocatedPool::<T>::get(entity_id))
                .saturating_add(PendingRefundTotal::<T>::get(entity_id));
            let min_balance = T::Currency::minimum_balance();
            let available = entity_balance
                .saturating_sub(reserved)
                .saturating_sub(min_balance);
            ensure!(amount <= available, Error::<T>::InsufficientEntityFunds);

            // min_treasury_threshold 保护：提取后金库余额不得低于治理阈值
            let min_threshold: BalanceOf<T> =
                T::FundProtectionQuery::min_treasury_threshold(entity_id).saturated_into();
            if !min_threshold.is_zero() {
                let post_withdraw = entity_balance.saturating_sub(amount);
                ensure!(
                    post_withdraw >= min_threshold,
                    Error::<T>::InsufficientEntityFunds
                );
            }

            T::Currency::transfer(
                &entity_account,
                &who,
                amount,
                ExistenceRequirement::KeepAlive,
            )?;

            Self::deposit_event(Event::EntityFundsWithdrawn {
                entity_id,
                to: who,
                amount,
            });
            Ok(())
        }

        /// Entity Owner 提取 entity_account 中的可用 Token 自由余额
        ///
        /// 提取后 entity_token_balance 必须 ≥ TokenPendingTotal + TokenShoppingTotal + UnallocatedTokenPool + PendingTokenRefundTotal
        /// 沉淀池（UnallocatedTokenPool）始终受保护，不可被提取
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::withdraw_entity_token_funds())]
        pub fn withdraw_entity_token_funds(
            origin: OriginFor<T>,
            entity_id: u64,
            amount: TokenBalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_entity_owner(entity_id, &who)?;
            // MEDIUM-1 审计修复: Closed/Banned Entity 不允许 Owner 提取 Token 资金
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(!amount.is_zero(), Error::<T>::ZeroWithdrawalAmount);

            // 检测外部直接转入的 Token 并归入沉淀池（incoming=0，无已知入账）
            Self::sweep_token_free_balance(entity_id, TokenBalanceOf::<T>::zero());

            let entity_account = T::EntityProvider::entity_account(entity_id);
            let entity_token_balance =
                T::TokenTransferProvider::token_balance_of(entity_id, &entity_account);

            // 沉淀池始终受保护（不可被 Owner 提取）
            let reserved = TokenPendingTotal::<T>::get(entity_id)
                .saturating_add(T::LoyaltyToken::token_shopping_total(entity_id))
                .saturating_add(UnallocatedTokenPool::<T>::get(entity_id))
                .saturating_add(PendingTokenRefundTotal::<T>::get(entity_id));
            let available = entity_token_balance.saturating_sub(reserved);
            ensure!(
                amount <= available,
                Error::<T>::InsufficientEntityTokenFunds
            );

            T::TokenTransferProvider::token_transfer(entity_id, &entity_account, &who, amount)
                .map_err(|_| Error::<T>::TokenTransferFailed)?;
            EntityTokenAccountedBalance::<T>::mutate(entity_id, |b| {
                *b = Some(b.unwrap_or_default().saturating_sub(amount));
            });

            Self::deposit_event(Event::EntityTokenFundsWithdrawn {
                entity_id,
                to: who,
                amount,
            });
            Ok(())
        }

        /// 设置 Owner 收益比例（Entity 级，从 Pool B 佣金预算中优先扣除）
        ///
        /// 仅 Entity Owner 可调用。rate 为基点，0 = 不启用，上限 5000（50%）
        /// 需同时启用 OWNER_REWARD 模式位才会实际生效
        ///
        /// R8: 无币实体（None 模式）锁定后，仍允许单调递减（只减不增）。
        /// FullDAO 锁定的实体需通过 DAO 提案修改。
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::set_owner_reward_rate())]
        pub fn set_owner_reward_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            rate: u16,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;

            // R8: 锁定状态下仅 None 模式允许单调递减
            if T::EntityProvider::is_entity_locked(entity_id) {
                let mode = T::GovernanceProvider::governance_mode(entity_id);
                ensure!(mode == GovernanceMode::None, Error::<T>::EntityLocked);
                let current = CommissionConfigs::<T>::get(entity_id)
                    .map(|c| c.owner_reward_rate)
                    .unwrap_or(0);
                ensure!(rate < current, Error::<T>::LockedOnlyDecreaseAllowed);
            }

            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(rate <= 5000, Error::<T>::InvalidCommissionRate);

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.owner_reward_rate = rate;
            });

            Self::deposit_event(Event::CommissionConfigUpdated { entity_id });
            Ok(())
        }

        /// 设置 Token 平台费率（Root / 治理）
        ///
        /// rate 为基点，0 = 关闭 Token 平台费，上限 1000 bps（10%）
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::set_token_platform_fee_rate())]
        pub fn set_token_platform_fee_rate(origin: OriginFor<T>, new_rate: u16) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(new_rate <= 1000, Error::<T>::TokenPlatformFeeRateTooHigh);
            let old_rate = TokenPlatformFeeRate::<T>::get();
            TokenPlatformFeeRate::<T>::put(new_rate);
            Self::deposit_event(Event::TokenPlatformFeeRateUpdated { old_rate, new_rate });
            Ok(())
        }

        /// F13: 设置 NEX 全局最低复购比例（Root，Governance 底线）
        ///
        /// 与 Token 版 set_global_min_token_repurchase_rate (call_index 11) 对称
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::set_global_min_repurchase_rate())]
        pub fn set_global_min_repurchase_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            rate: u16,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(rate <= 10000, Error::<T>::InvalidCommissionRate);
            GlobalMinRepurchaseRate::<T>::insert(entity_id, rate);
            Self::deposit_event(Event::GlobalMinRepurchaseRateSet { entity_id, rate });
            Ok(())
        }

        /// F2: 设置提现冻结期（Entity 级，NEX 和 Token 独立配置）
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::set_withdrawal_cooldown())]
        pub fn set_withdrawal_cooldown(
            origin: OriginFor<T>,
            entity_id: u64,
            nex_cooldown: u32,
            token_cooldown: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.withdrawal_cooldown = nex_cooldown;
                config.token_withdrawal_cooldown = token_cooldown;
            });

            Self::deposit_event(Event::WithdrawalCooldownUpdated {
                entity_id,
                nex_cooldown,
                token_cooldown,
            });
            Ok(())
        }

        /// F14: Root 紧急禁用 Entity 佣金（不可逆，需 Root 重新启用）
        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::force_disable_entity_commission())]
        pub fn force_disable_entity_commission(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // Root 豁免 PoolNotEmpty 检查（紧急处置），但沉淀池资金仍受 protected_funds 保护，
            // 会员仍可通过 pool-reward 领取。
            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.enabled = false;
            });

            Self::deposit_event(Event::CommissionForceDisabled { entity_id });
            Ok(())
        }

        /// F15: 设置全局佣金率上限（Root）
        ///
        /// Entity Owner 的 max_commission_rate 不得超过此值。0 = 无限制。
        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::set_global_max_commission_rate())]
        pub fn set_global_max_commission_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            rate: u16,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(rate <= 10000, Error::<T>::InvalidCommissionRate);
            // Root 设置 GlobalMax 也受预算上限约束（0 = 无限制，跳过检查）
            if rate > 0 {
                ensure!(
                    rate <= Self::commission_budget_ceiling(),
                    Error::<T>::CommissionRateExceedsBudget
                );
            }
            GlobalMaxCommissionRate::<T>::insert(entity_id, rate);
            Self::deposit_event(Event::GlobalMaxCommissionRateSet { entity_id, rate });
            Ok(())
        }

        /// F4: 清除佣金配置（恢复默认值）
        #[pallet::call_index(20)]
        #[pallet::weight(T::WeightInfo::clear_commission_config())]
        pub fn clear_commission_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                CommissionConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );

            // 沉淀池非空时不允许清除配置（等效于关闭 POOL_REWARD）
            let old_pool_on = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.enabled && c.enabled_modes.contains(CommissionModes::POOL_REWARD))
                .unwrap_or(false);

            if old_pool_on {
                ensure!(
                    UnallocatedPool::<T>::get(entity_id).is_zero()
                        && UnallocatedTokenPool::<T>::get(entity_id).is_zero(),
                    Error::<T>::PoolNotEmpty
                );
            }

            CommissionConfigs::<T>::remove(entity_id);

            Self::deposit_event(Event::CommissionConfigCleared { entity_id });
            Ok(())
        }

        /// F4: 清除 NEX 提现配置
        #[pallet::call_index(21)]
        #[pallet::weight(T::WeightInfo::clear_withdrawal_config())]
        pub fn clear_withdrawal_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                WithdrawalConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );

            WithdrawalConfigs::<T>::remove(entity_id);
            Self::deposit_event(Event::WithdrawalConfigCleared { entity_id });
            Ok(())
        }

        /// F4: 清除 Token 提现配置
        #[pallet::call_index(22)]
        #[pallet::weight(T::WeightInfo::clear_token_withdrawal_config())]
        pub fn clear_token_withdrawal_config(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                TokenWithdrawalConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );

            TokenWithdrawalConfigs::<T>::remove(entity_id);
            Self::deposit_event(Event::TokenWithdrawalConfigCleared { entity_id });
            Ok(())
        }

        /// F16: 设置全局 Token 佣金率上限（Root）
        ///
        /// Entity Owner 的 Token max_commission_rate 不得超过此值。0 = 无限制。
        #[pallet::call_index(23)]
        #[pallet::weight(T::WeightInfo::set_global_max_token_commission_rate())]
        pub fn set_global_max_token_commission_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            rate: u16,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(rate <= 10000, Error::<T>::InvalidCommissionRate);
            if rate > 0 {
                ensure!(
                    rate <= Self::commission_budget_ceiling(),
                    Error::<T>::CommissionRateExceedsBudget
                );
            }
            GlobalMaxTokenCommissionRate::<T>::insert(entity_id, rate);
            Self::deposit_event(Event::GlobalMaxTokenCommissionRateSet { entity_id, rate });
            Ok(())
        }

        /// F17: 全局佣金紧急暂停/恢复（Root）
        ///
        /// 暂停后所有 Entity 的 process_commission、withdraw_commission 均被阻止
        #[pallet::call_index(24)]
        #[pallet::weight(T::WeightInfo::force_global_pause())]
        pub fn force_global_pause(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            ensure_root(origin)?;
            GlobalCommissionPaused::<T>::put(paused);
            Self::deposit_event(Event::GlobalCommissionPauseToggled { paused });
            Ok(())
        }

        /// F18: Entity 级提现暂停/恢复（Entity Owner/Admin）
        ///
        /// 轻量级开关，不影响 WithdrawalConfig 本身
        #[pallet::call_index(25)]
        #[pallet::weight(T::WeightInfo::pause_withdrawals())]
        pub fn pause_withdrawals(
            origin: OriginFor<T>,
            entity_id: u64,
            paused: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );

            WithdrawalPaused::<T>::insert(entity_id, paused);
            Self::deposit_event(Event::WithdrawalPauseToggled { entity_id, paused });
            Ok(())
        }

        /// F21: 归档已完结订单的佣金记录（释放链上存储）
        ///
        /// 仅允许 Entity Owner/Admin 调用。订单所有 NEX 和 Token 佣金记录
        /// 状态必须为 Withdrawn 或 Cancelled 才可归档。
        #[pallet::call_index(26)]
        #[pallet::weight(T::WeightInfo::archive_order_records())]
        pub fn archive_order_records(
            origin: OriginFor<T>,
            entity_id: u64,
            order_id: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;

            // 检查 NEX 记录是否可归档
            let nex_records = OrderCommissionRecords::<T>::get(order_id);
            let token_records = OrderTokenCommissionRecords::<T>::get(order_id);

            // 至少有一种记录存在
            ensure!(
                !nex_records.is_empty() || !token_records.is_empty(),
                Error::<T>::OrderRecordsNotFound
            );

            // M1-R5 审计修复: 验证订单记录确属该 entity_id，防止跨实体越权归档
            for record in nex_records.iter() {
                ensure!(
                    record.entity_id == entity_id,
                    Error::<T>::OrderRecordsNotFound
                );
            }
            for record in token_records.iter() {
                ensure!(
                    record.entity_id == entity_id,
                    Error::<T>::OrderRecordsNotFound
                );
            }

            // 所有 NEX 记录必须已完结
            for record in nex_records.iter() {
                ensure!(
                    record.status == CommissionStatus::Settled
                        || record.status == CommissionStatus::Cancelled,
                    Error::<T>::OrderRecordsNotFinalized
                );
            }

            // 所有 Token 记录必须已完结
            for record in token_records.iter() {
                ensure!(
                    record.status == CommissionStatus::Settled
                        || record.status == CommissionStatus::Cancelled,
                    Error::<T>::OrderRecordsNotFinalized
                );
            }

            // 清理所有关联存储
            OrderCommissionRecords::<T>::remove(order_id);
            OrderTokenCommissionRecords::<T>::remove(order_id);
            OrderTreasuryTransfer::<T>::remove(order_id);
            OrderUnallocated::<T>::remove(order_id);
            OrderTokenUnallocated::<T>::remove(order_id);
            OrderTokenPlatformRetention::<T>::remove(order_id);

            Self::deposit_event(Event::OrderRecordsArchived { order_id });
            Ok(())
        }

        /// BUG-3 修复: Root 重新启用 Entity 佣金（与 force_disable_entity_commission 对称）
        ///
        /// force_disable 将 enabled 设为 false 后，Entity Owner 无法通过
        /// enable_commission 恢复（该 extrinsic 要求 Owner/Admin 权限，但 force_disable
        /// 的场景往往需要 Root 介入解除）。本 extrinsic 提供 Root 级恢复路径。
        #[pallet::call_index(27)]
        #[pallet::weight(T::WeightInfo::force_enable_entity_commission())]
        pub fn force_enable_entity_commission(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                T::EntityProvider::entity_exists(entity_id),
                Error::<T>::EntityNotFound
            );

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.enabled = true;
            });

            Self::deposit_event(Event::CommissionForceEnabled { entity_id });
            Ok(())
        }

        /// MISSING-2 修复: Root 重试失败的订单退款
        ///
        /// 当 cancel_commission 中 Entity 账户余额不足导致部分退款失败时，
        /// 失败的记录仍保持 Pending 状态。本 extrinsic 允许 Root 在 Entity
        /// 账户补充资金后重新触发取消流程。
        ///
        /// cancel_commission 本身是幂等的（仅处理 Pending 记录），可安全重复调用。
        #[pallet::call_index(28)]
        #[pallet::weight(T::WeightInfo::retry_cancel_commission())]
        pub fn retry_cancel_commission(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            Self::cancel_commission(order_id)
        }

        /// MISSING-3: 设置最小提现间隔（Entity 级，区块数）
        ///
        /// 限制会员连续提现频率。0 = 无限制（默认）。
        /// 与 withdrawal_cooldown（基于入账时间）独立，本参数基于上次提现时间。
        #[pallet::call_index(29)]
        #[pallet::weight(T::WeightInfo::set_min_withdrawal_interval())]
        pub fn set_min_withdrawal_interval(
            origin: OriginFor<T>,
            entity_id: u64,
            interval: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            MinWithdrawalInterval::<T>::insert(entity_id, interval);
            Self::deposit_event(Event::MinWithdrawalIntervalUpdated {
                entity_id,
                interval,
            });
            Ok(())
        }

        /// 设置店铺级佣金率覆盖（Entity Owner/Admin）
        ///
        /// 优先级: ProductCommissionRate > ShopCommissionRate > CoreCommissionConfig.max_commission_rate
        /// rate = None 清除覆盖（恢复继承 Entity 默认）
        #[pallet::call_index(30)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_shop_commission_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            shop_id: u64,
            rate: Option<u16>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // 校验 Shop 属于该 Entity
            let shop_entity = T::ShopProvider::shop_entity_id(shop_id);
            ensure!(shop_entity == Some(entity_id), Error::<T>::ShopNotInEntity);

            if let Some(r) = rate {
                ensure!(r <= 10000, Error::<T>::InvalidCommissionRate);
                ensure!(
                    r <= Self::commission_budget_ceiling(),
                    Error::<T>::CommissionRateExceedsBudget
                );
                // GlobalMax 校验（与 set_commission_rate 一致）
                let global_max = GlobalMaxCommissionRate::<T>::get(entity_id);
                if global_max > 0 {
                    ensure!(r <= global_max, Error::<T>::CommissionRateExceedsGlobalMax);
                }
                let token_global_max = GlobalMaxTokenCommissionRate::<T>::get(entity_id);
                if token_global_max > 0 {
                    ensure!(
                        r <= token_global_max,
                        Error::<T>::TokenCommissionRateExceedsGlobalMax
                    );
                }
                ShopCommissionRate::<T>::insert(shop_id, r);
            } else {
                ShopCommissionRate::<T>::remove(shop_id);
            }

            Self::deposit_event(Event::ShopCommissionRateSet {
                entity_id,
                shop_id,
                rate,
            });
            Ok(())
        }

        /// 设置产品级佣金率覆盖（Entity Owner/Admin）
        ///
        /// 优先级: ProductCommissionRate > ShopCommissionRate > CoreCommissionConfig.max_commission_rate
        /// rate = None 清除覆盖（恢复继承 Shop 或 Entity 默认）
        #[pallet::call_index(31)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_product_commission_rate(
            origin: OriginFor<T>,
            entity_id: u64,
            shop_id: u64,
            product_id: u64,
            rate: Option<u16>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // 校验 Shop 属于该 Entity（产品归属由前端保证，此处验证 Entity 权限链）
            let shop_entity = T::ShopProvider::shop_entity_id(shop_id);
            ensure!(
                shop_entity == Some(entity_id),
                Error::<T>::ProductNotInEntity
            );

            if let Some(r) = rate {
                ensure!(r <= 10000, Error::<T>::InvalidCommissionRate);
                ensure!(
                    r <= Self::commission_budget_ceiling(),
                    Error::<T>::CommissionRateExceedsBudget
                );
                let global_max = GlobalMaxCommissionRate::<T>::get(entity_id);
                if global_max > 0 {
                    ensure!(r <= global_max, Error::<T>::CommissionRateExceedsGlobalMax);
                }
                let token_global_max = GlobalMaxTokenCommissionRate::<T>::get(entity_id);
                if token_global_max > 0 {
                    ensure!(
                        r <= token_global_max,
                        Error::<T>::TokenCommissionRateExceedsGlobalMax
                    );
                }
                ProductCommissionRate::<T>::insert(product_id, r);
            } else {
                ProductCommissionRate::<T>::remove(product_id);
            }

            Self::deposit_event(Event::ProductCommissionRateSet {
                entity_id,
                product_id,
                rate,
            });
            Ok(())
        }

        /// 设置插件独立预算上限（Entity Owner/Admin）
        ///
        /// cap 与 max_commission_rate 同一量纲（bps of order_amount），
        /// 每个 cap 值表示该插件最多使用订单金额的 cap/10000 进行分配。
        /// 0 = 不限制（当前行为，向后兼容）
        /// 每个 cap ≤ max_commission_rate（设置时校验）
        /// 运行时：plugin_budget = min(remaining, order_amount × cap / 10000)
        #[pallet::call_index(32)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_plugin_budget_caps(
            origin: OriginFor<T>,
            entity_id: u64,
            caps: PluginBudgetCaps,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // Layer 1: 绝对上限（不能超过预算上限 = 10000 - platform_fee_rate）
            let ceiling = Self::commission_budget_ceiling();
            ensure!(caps.referral_cap <= ceiling, Error::<T>::InvalidPluginCap);
            ensure!(
                caps.multi_level_cap <= ceiling,
                Error::<T>::InvalidPluginCap
            );
            ensure!(caps.level_diff_cap <= ceiling, Error::<T>::InvalidPluginCap);
            ensure!(
                caps.single_line_cap <= ceiling,
                Error::<T>::InvalidPluginCap
            );
            ensure!(caps.team_cap <= ceiling, Error::<T>::InvalidPluginCap);

            // Layer 2: 相对上限（不能超过当前 max_commission_rate）
            let max_rate = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.max_commission_rate)
                .unwrap_or(10000);
            ensure!(
                caps.referral_cap <= max_rate,
                Error::<T>::PluginCapExceedsCommissionRate
            );
            ensure!(
                caps.multi_level_cap <= max_rate,
                Error::<T>::PluginCapExceedsCommissionRate
            );
            ensure!(
                caps.level_diff_cap <= max_rate,
                Error::<T>::PluginCapExceedsCommissionRate
            );
            ensure!(
                caps.single_line_cap <= max_rate,
                Error::<T>::PluginCapExceedsCommissionRate
            );
            ensure!(
                caps.team_cap <= max_rate,
                Error::<T>::PluginCapExceedsCommissionRate
            );

            // H-1 审计修复: 多插件组时要求已启用插件的 cap > 0
            let current_modes = CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.enabled_modes)
                .unwrap_or_default();
            Self::validate_plugin_caps_for_modes(&current_modes, &caps)?;

            CommissionConfigs::<T>::mutate(entity_id, |maybe| {
                let config = maybe.get_or_insert_with(CoreCommissionConfig::default);
                config.plugin_caps = caps.clone();
            });

            Self::deposit_event(Event::PluginBudgetCapsUpdated { entity_id, caps });
            Ok(())
        }

        /// [Root] 设置推荐人免除阈值
        ///
        /// 多级分销配置层数超过此阈值的 Entity，推荐人不获得招商提成。
        /// threshold = 0 表示不启用此规则。
        #[pallet::call_index(33)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_referrer_exempt_threshold(
            origin: OriginFor<T>,
            threshold: u16,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let old = ReferrerExemptThreshold::<T>::get();
            ReferrerExemptThreshold::<T>::put(threshold);
            Self::deposit_event(Event::ReferrerExemptThresholdChanged {
                old_threshold: old,
                new_threshold: threshold,
            });
            Ok(())
        }

        /// [Entity Owner/Admin] 关闭/开启本 Entity 推荐人的平台费分成
        ///
        /// disabled = true 时，该 Entity 的推荐人不再获得 EntityReferral 佣金，
        /// 平台费 100% 归国库。不影响 Pool B（会员返佣）。
        #[pallet::call_index(34)]
        #[pallet::weight(T::WeightInfo::set_referrer_payout_disabled())]
        pub fn set_referrer_payout_disabled(
            origin: OriginFor<T>,
            entity_id: u64,
            disabled: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // 关闭推荐人佣金时，检查保护期
            if disabled {
                let protection = T::ReferrerProtectionPeriod::get();
                if protection > 0 {
                    if let Some(bound_at) = T::EntityReferrerProvider::referrer_bound_at(entity_id)
                    {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let now_u64: u64 = now.try_into().unwrap_or(u64::MAX);
                        ensure!(
                            now_u64.saturating_sub(bound_at) >= protection,
                            Error::<T>::ReferrerProtectionPeriodActive
                        );
                    }
                }
            }

            ReferrerPayoutDisabled::<T>::insert(entity_id, disabled);
            Self::deposit_event(Event::ReferrerPayoutToggled {
                entity_id,
                disabled,
            });
            Ok(())
        }

        /// [Root] H-2 审计修复: 重试订单的待退款
        ///
        /// 当 cancel_commission 中转账退款失败时，资金留在 entity_account 中，
        /// stats 未扣减，待退款记录存入 PendingRefunds。
        /// Root 可在修复问题（如给 entity_account 补充资金）后调用此方法重试。
        /// 成功后扣减 stats 并标记对应佣金记录为 Cancelled。
        #[pallet::call_index(35)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn retry_pending_refund(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            let pending_entries = PendingRefunds::<T>::get(order_id);
            ensure!(!pending_entries.is_empty(), Error::<T>::NoPendingRefund);

            let platform_account = T::PlatformAccount::get();
            let mut all_succeeded = true;
            let mut resolved_total = BalanceOf::<T>::zero();
            let mut still_pending: alloc::vec::Vec<(u64, u64, bool, BalanceOf<T>)> =
                alloc::vec::Vec::new();

            for (entity_id, shop_id, is_platform, amount) in pending_entries.iter() {
                if amount.is_zero() {
                    continue;
                }
                let entity_account = T::EntityProvider::entity_account(*entity_id);
                let refund_target = if *is_platform {
                    platform_account.clone()
                } else {
                    match T::ShopProvider::shop_owner(*shop_id) {
                        Some(seller) => seller,
                        None => {
                            still_pending.push((*entity_id, *shop_id, *is_platform, *amount));
                            all_succeeded = false;
                            continue;
                        }
                    }
                };

                if T::Currency::transfer(
                    &entity_account,
                    &refund_target,
                    *amount,
                    ExistenceRequirement::KeepAlive,
                )
                .is_ok()
                {
                    // 转账成功 → 扣减 PendingRefundTotal + 扣减对应记录的 stats
                    PendingRefundTotal::<T>::mutate(entity_id, |total| {
                        *total = total.saturating_sub(*amount);
                    });
                    resolved_total = resolved_total.saturating_add(*amount);
                } else {
                    still_pending.push((*entity_id, *shop_id, *is_platform, *amount));
                    all_succeeded = false;
                }
            }

            // 退款成功后，扣减对应订单佣金记录的 stats 并标记 Cancelled
            if !resolved_total.is_zero() {
                let is_still_pending = |eid: u64, sid: u64, is_plat: bool| -> bool {
                    still_pending
                        .iter()
                        .any(|(e, s, p, _)| *e == eid && *s == sid && *p == is_plat)
                };
                OrderCommissionRecords::<T>::mutate(order_id, |records| {
                    for record in records.iter_mut() {
                        // 只处理尚未取消的记录（退款失败时保留了原状态）
                        if matches!(
                            record.status,
                            CommissionStatus::Pending | CommissionStatus::Settled
                        ) {
                            let is_platform =
                                record.commission_type == CommissionType::EntityReferral;
                            if record.commission_type != CommissionType::PoolReward
                                && !is_still_pending(record.entity_id, record.shop_id, is_platform)
                            {
                                MemberCommissionStats::<T>::mutate(
                                    record.entity_id,
                                    &record.beneficiary,
                                    |stats| {
                                        stats.pending = stats.pending.saturating_sub(record.amount);
                                        stats.total_earned =
                                            stats.total_earned.saturating_sub(record.amount);
                                    },
                                );
                                ShopPendingTotal::<T>::mutate(record.entity_id, |total| {
                                    *total = total.saturating_sub(record.amount);
                                });
                                if matches!(
                                    record.commission_type,
                                    CommissionType::DirectReward
                                        | CommissionType::FirstOrder
                                        | CommissionType::RepeatPurchase
                                        | CommissionType::FixedAmount
                                ) {
                                    ReferrerEarnedByBuyer::<T>::mutate(
                                        (record.entity_id, &record.beneficiary, &record.buyer),
                                        |earned| {
                                            *earned = earned.saturating_sub(record.amount);
                                        },
                                    );
                                }
                                record.status = CommissionStatus::Cancelled;
                            }
                        }
                    }
                });

                Self::deposit_event(Event::RefundPendingResolved {
                    entity_id: pending_entries.first().map(|e| e.0).unwrap_or(0),
                    order_id,
                    amount: resolved_total,
                });
            }

            // 更新或清除 PendingRefunds
            if all_succeeded {
                PendingRefunds::<T>::remove(order_id);
            } else {
                PendingRefunds::<T>::insert(
                    order_id,
                    BoundedVec::<_, ConstU32<50>>::try_from(still_pending).unwrap_or_default(),
                );
            }

            Ok(())
        }

        /// [Root] HIGH-1 审计修复: 重试订单的 Token 待退款（与 retry_pending_refund 对称）
        ///
        /// 当 do_cancel_token_commission 中 Token 沉淀池退款失败时，
        /// Token 留在 entity_account 的 UnallocatedTokenPool 中，
        /// 待退款记录存入 PendingTokenRefunds。
        /// Root 可在修复问题后调用此方法重试退还给卖家。
        #[pallet::call_index(36)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn retry_pending_token_refund(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            let pending_entries = PendingTokenRefunds::<T>::get(order_id);
            ensure!(!pending_entries.is_empty(), Error::<T>::NoPendingRefund);

            let mut all_succeeded = true;
            let mut resolved_total = TokenBalanceOf::<T>::zero();
            let mut still_pending: alloc::vec::Vec<(u64, u64, TokenBalanceOf<T>)> =
                alloc::vec::Vec::new();

            for (entity_id, shop_id, amount) in pending_entries.iter() {
                if amount.is_zero() {
                    continue;
                }
                let entity_account = T::EntityProvider::entity_account(*entity_id);
                let seller = match T::ShopProvider::shop_owner(*shop_id) {
                    Some(s) => s,
                    None => {
                        still_pending.push((*entity_id, *shop_id, *amount));
                        all_succeeded = false;
                        continue;
                    }
                };

                if T::TokenTransferProvider::token_transfer(
                    *entity_id,
                    &entity_account,
                    &seller,
                    *amount,
                )
                .is_ok()
                {
                    // 转账成功 → 扣减 UnallocatedTokenPool + PendingTokenRefundTotal
                    UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                        *pool = pool.saturating_sub(*amount);
                    });
                    EntityTokenAccountedBalance::<T>::mutate(entity_id, |b| {
                        *b = Some(b.unwrap_or_default().saturating_sub(*amount));
                    });
                    PendingTokenRefundTotal::<T>::mutate(entity_id, |total| {
                        *total = total.saturating_sub(*amount);
                    });
                    resolved_total = resolved_total.saturating_add(*amount);
                } else {
                    still_pending.push((*entity_id, *shop_id, *amount));
                    all_succeeded = false;
                }
            }

            if !resolved_total.is_zero() {
                // 清理 OrderTokenUnallocated（原始退款源）
                OrderTokenUnallocated::<T>::remove(order_id);

                Self::deposit_event(Event::TokenRefundPendingResolved {
                    order_id,
                    amount: resolved_total,
                });
            }

            if all_succeeded {
                PendingTokenRefunds::<T>::remove(order_id);
            } else {
                PendingTokenRefunds::<T>::insert(
                    order_id,
                    BoundedVec::<_, ConstU32<50>>::try_from(still_pending).unwrap_or_default(),
                );
            }

            Ok(())
        }

        /// [Root] HIGH-2 审计修复: 强制清零 Entity 所有未结清佣金/购物余额
        ///
        /// 当 Entity 想要关闭但 `ensure_no_active_dependencies` 因未结清资金被阻止时，
        /// Root 可调用此方法清零所有待提佣金、购物余额、待重试退款。
        /// 清零后资金保留在 entity_account 中，由关闭流程返还给 Owner。
        ///
        /// 注意: 此操作不可逆，会导致会员损失未提取的佣金和购物余额。
        /// 应仅在确认无法通过正常渠道解决时使用。
        #[pallet::call_index(37)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn force_forfeit_entity_balances(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            ensure_root(origin)?;

            // 清零 NEX pending commissions
            let nex_pending = ShopPendingTotal::<T>::take(entity_id);
            // 清零 Token pending commissions
            let token_pending = TokenPendingTotal::<T>::take(entity_id);
            // 清零 NEX pending refunds
            let nex_refund = PendingRefundTotal::<T>::take(entity_id);
            // 清零 Token pending refunds
            let token_refund = PendingTokenRefundTotal::<T>::take(entity_id);

            // 清零 NEX + Token 购物余额（通过 Loyalty 模块 drain prefix）
            T::Loyalty::forfeit_all_shopping_balances(entity_id);
            T::LoyaltyToken::forfeit_all_token_shopping_balances(entity_id);

            Self::deposit_event(Event::EntityBalancesForfeited {
                entity_id,
                nex_pending_forfeited: nex_pending,
                token_pending_forfeited: token_pending,
                nex_refund_forfeited: nex_refund,
                token_refund_forfeited: token_refund,
            });

            Ok(())
        }

        /// 设置强制复购配置（Entity Owner / Admin）
        ///
        /// 当 `enforced == true` 时，购物余额达到 `min_package_usdt` 后
        /// 提现入账会发出 `RepurchaseReady` 事件。
        #[pallet::call_index(38)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn set_repurchase_config(
            origin: OriginFor<T>,
            entity_id: u64,
            config: pallet_commission_common::RepurchaseConfig,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );

            // USDT 精度防御：非零值必须 >= 1_000_000（1 USDT），防止漏乘 10^6
            if config.min_package_usdt > 0 && config.min_package_usdt < 1_000_000 {
                return Err(Error::<T>::UsdtAmountTooSmall.into());
            }
            if config.max_shopping_balance_usdt > 0 && config.max_shopping_balance_usdt < 1_000_000
            {
                return Err(Error::<T>::UsdtAmountTooSmall.into());
            }

            // enforced=true 时 min_package_usdt 必须 > 0
            if config.enforced {
                ensure!(
                    config.min_package_usdt > 0,
                    Error::<T>::InvalidRepurchaseConfig
                );
            }
            // auto_order=true 时 default_product_id 必须 > 0 且归属本 Entity
            if config.auto_order {
                ensure!(
                    config.default_product_id > 0,
                    Error::<T>::AutoOrderProductNotSet
                );
                T::AutoRepurchase::validate_repurchase_product(
                    entity_id,
                    config.default_product_id,
                )
                .map_err(|_| Error::<T>::AutoOrderProductNotInEntity)?;
            }
            // TTL 非零时必须满足最小值（防止过短 TTL 浪费链上资源）
            if config.shopping_balance_ttl_blocks > 0 {
                let min_ttl = T::MinShoppingBalanceTtlBlocks::get();
                if min_ttl > 0 {
                    ensure!(
                        config.shopping_balance_ttl_blocks >= min_ttl,
                        Error::<T>::TtlTooShort
                    );
                }
            }

            RepurchaseConfigs::<T>::insert(entity_id, config);
            Self::deposit_event(Event::RepurchaseConfigUpdated { entity_id });

            Ok(())
        }

        /// 触发不活跃用户购物余额过期（任何签名者可调用）
        ///
        /// 条件：
        /// - `RepurchaseConfig.shopping_balance_ttl_blocks > 0`
        /// - 当前区块 >= last_credited_block + ttl_blocks
        ///
        /// 处理顺序：
        /// 1. 若 `auto_order=true` 且 `default_product_id > 0`，尝试链上自动下单
        ///    - 成功：发 `AutoRepurchaseCreated`，余额被订单消耗
        ///    - 失败：降级为步骤 2
        /// 2. 没收余额（清零记账），NEX 自然归还 Entity 账户，发 `ShoppingBalanceExpired`
        #[pallet::call_index(39)]
        #[pallet::weight(T::WeightInfo::set_commission_rate())]
        pub fn expire_shopping_balance(
            origin: OriginFor<T>,
            entity_id: u64,
            account: T::AccountId,
        ) -> DispatchResult {
            ensure_signed(origin)?;

            let config =
                RepurchaseConfigs::<T>::get(entity_id).ok_or(Error::<T>::RepurchaseConfigNotSet)?;

            ensure!(
                config.shopping_balance_ttl_blocks > 0,
                Error::<T>::ShoppingBalanceNotExpired
            );

            let last_credited = MemberShoppingBalanceLastCredited::<T>::get(entity_id, &account)
                .ok_or(Error::<T>::ShoppingBalanceNotExpired)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let ttl: BlockNumberFor<T> = config.shopping_balance_ttl_blocks.into();
            ensure!(
                now >= last_credited.saturating_add(ttl),
                Error::<T>::ShoppingBalanceNotExpired
            );

            let balance = T::Loyalty::shopping_balance(entity_id, &account);
            ensure!(!balance.is_zero(), Error::<T>::ZeroShoppingBalance);

            // 1. 尝试自动下单（消耗购物余额进入订单）
            if config.auto_order && config.default_product_id > 0 {
                if let Ok(order_id) = T::AutoRepurchase::try_place_repurchase_order(
                    entity_id,
                    &account,
                    config.default_product_id,
                ) {
                    MemberShoppingBalanceLastCredited::<T>::remove(entity_id, &account);
                    Self::deposit_event(Event::AutoRepurchaseCreated {
                        entity_id,
                        account,
                        order_id,
                        product_id: config.default_product_id,
                    });
                    return Ok(());
                }
            }

            // 2. 自动下单失败或未配置 → 没收余额归 Entity
            // 仅清零记账（MemberShoppingBalance + ShopShoppingTotal），
            // Entity 账户中对应的 NEX 自然解锁为 Entity 可用资金
            T::Loyalty::forfeit_shopping_balance(entity_id, &account)?;
            MemberShoppingBalanceLastCredited::<T>::remove(entity_id, &account);

            Self::deposit_event(Event::ShoppingBalanceExpired {
                entity_id,
                account,
                amount: balance,
            });

            Ok(())
        }
    }

    // ========================================================================
    // Internal functions — extracted to engine.rs, withdraw.rs, settlement.rs
    // ========================================================================

    // ========================================================================
    // Hooks (#11: Storage 版本与迁移钩子)
    // ========================================================================

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = StorageVersion::get::<Pallet<T>>();
            if on_chain < 1 {
                log::info!(
                    target: "pallet-commission-core",
                    "🔄 Migrating from v{:?} to v1 (no-op data migration, new storage items have defaults)",
                    on_chain,
                );
                StorageVersion::new(1).put::<Pallet<T>>();
                Weight::from_parts(10_000_000, 1_000)
            } else {
                Weight::zero()
            }
        }

        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            use frame_support::ensure;

            // INV-1: 每个 entity 的 MemberCommissionStats.pending 之和 = ShopPendingTotal
            // INV-2: [已迁移至 Loyalty 模块] NEX 购物余额一致性不再由 commission 检查
            // INV-3: Token 侧对称不变量
            //
            // 注意: 遍历 DoubleMap 在 try-runtime 中是可接受的（仅在升级检查时运行）

            // 收集所有 entity_id（从 ShopPendingTotal 和 Token Storage 的 key）
            let mut entity_ids = alloc::collections::BTreeSet::new();
            for (eid, _) in ShopPendingTotal::<T>::iter() {
                entity_ids.insert(eid);
            }
            for (eid, _) in TokenPendingTotal::<T>::iter() {
                entity_ids.insert(eid);
            }

            for entity_id in &entity_ids {
                // INV-1: NEX pending 一致性
                let stored_pending = ShopPendingTotal::<T>::get(entity_id);
                let mut sum_pending = BalanceOf::<T>::zero();
                for (_, stats) in MemberCommissionStats::<T>::iter_prefix(entity_id) {
                    sum_pending = sum_pending.saturating_add(stats.pending);
                }
                ensure!(
                    sum_pending == stored_pending,
                    sp_runtime::TryRuntimeError::Other(
                        "INV-1: sum(MemberCommissionStats.pending) != ShopPendingTotal"
                    )
                );

                // INV-2: [已迁移] NEX shopping 一致性由 Loyalty 模块负责

                // INV-3: Token pending 一致性
                let stored_token_pending = TokenPendingTotal::<T>::get(entity_id);
                let mut sum_token_pending = TokenBalanceOf::<T>::zero();
                for (_, stats) in MemberTokenCommissionStats::<T>::iter_prefix(entity_id) {
                    sum_token_pending = sum_token_pending.saturating_add(stats.pending);
                }
                ensure!(
                    sum_token_pending == stored_token_pending,
                    sp_runtime::TryRuntimeError::Other(
                        "INV-3: sum(MemberTokenCommissionStats.pending) != TokenPendingTotal"
                    )
                );

                // INV-4: [已迁移] Token shopping 一致性由 Loyalty 模块负责
            }

            // INV-5: GlobalCommissionPaused 布尔一致性（总是 valid）
            let _ = GlobalCommissionPaused::<T>::get();

            // INV-6: 所有 WithdrawalConfig 的 tier 比率之和 = 10000
            for (_, wc) in WithdrawalConfigs::<T>::iter() {
                ensure!(
                    wc.default_tier
                        .withdrawal_rate
                        .saturating_add(wc.default_tier.repurchase_rate)
                        == 10000,
                    sp_runtime::TryRuntimeError::Other(
                        "INV-6: WithdrawalConfig default_tier rates don't sum to 10000"
                    )
                );
                for (_, tier) in wc.level_overrides.iter() {
                    ensure!(
                        tier.withdrawal_rate.saturating_add(tier.repurchase_rate) == 10000,
                        sp_runtime::TryRuntimeError::Other(
                            "INV-6: WithdrawalConfig level_override rates don't sum to 10000"
                        )
                    );
                }
            }
            for (_, wc) in TokenWithdrawalConfigs::<T>::iter() {
                ensure!(
                    wc.default_tier
                        .withdrawal_rate
                        .saturating_add(wc.default_tier.repurchase_rate)
                        == 10000,
                    sp_runtime::TryRuntimeError::Other(
                        "INV-6: TokenWithdrawalConfig default_tier rates don't sum to 10000"
                    )
                );
                for (_, tier) in wc.level_overrides.iter() {
                    ensure!(
                        tier.withdrawal_rate.saturating_add(tier.repurchase_rate) == 10000,
                        sp_runtime::TryRuntimeError::Other(
                            "INV-6: TokenWithdrawalConfig level_override rates don't sum to 10000"
                        )
                    );
                }
            }

            // INV-7: CommissionConfig 的 max_commission_rate ≤ 10000
            for (_, config) in CommissionConfigs::<T>::iter() {
                ensure!(
                    config.max_commission_rate <= 10000,
                    sp_runtime::TryRuntimeError::Other("INV-7: max_commission_rate > 10000")
                );
                ensure!(
                    config.owner_reward_rate <= 5000,
                    sp_runtime::TryRuntimeError::Other("INV-7: owner_reward_rate > 5000")
                );
                // INV-7b: plugin_caps 各值 ≤ 10000
                ensure!(
                    config.plugin_caps.referral_cap <= 10000
                        && config.plugin_caps.multi_level_cap <= 10000
                        && config.plugin_caps.level_diff_cap <= 10000
                        && config.plugin_caps.single_line_cap <= 10000
                        && config.plugin_caps.team_cap <= 10000,
                    sp_runtime::TryRuntimeError::Other("INV-7b: plugin_caps value > 10000")
                );
            }

            // INV-8: ShopCommissionRate ≤ 10000
            for (_, rate) in ShopCommissionRate::<T>::iter() {
                ensure!(
                    rate <= 10000,
                    sp_runtime::TryRuntimeError::Other("INV-8: ShopCommissionRate > 10000")
                );
            }

            // INV-9: ProductCommissionRate ≤ 10000
            for (_, rate) in ProductCommissionRate::<T>::iter() {
                ensure!(
                    rate <= 10000,
                    sp_runtime::TryRuntimeError::Other("INV-9: ProductCommissionRate > 10000")
                );
            }

            Ok(())
        }

        fn integrity_test() {
            // #12: Config 常量合理性检查
            assert!(
                T::ReferrerShareBps::get() <= 10000,
                "ReferrerShareBps must be <= 10000 (100%)"
            );
            assert!(
                T::MaxCommissionRecordsPerOrder::get() > 0,
                "MaxCommissionRecordsPerOrder must be > 0"
            );
            assert!(T::MaxCustomLevels::get() > 0, "MaxCustomLevels must be > 0");
            assert!(
                T::MaxWithdrawalRecords::get() > 0,
                "MaxWithdrawalRecords must be > 0"
            );
            assert!(
                T::MaxMemberOrderIds::get() > 0,
                "MaxMemberOrderIds must be > 0"
            );
        }
    }
}

// ============================================================================
// CommissionProvider impl
// ============================================================================

/// CommissionFundGuard: 返回 Entity 账户中已承诺给会员的佣金资金总额
///
/// protected = ShopPendingTotal + ShoppingTotal + UnallocatedPool + PendingRefundTotal
///
/// Registry/Shop/Loyalty 等模块在从 Entity 账户转出资金时须扣除此值，
/// 确保不侵占会员待提佣金、购物余额、沉淀池和待重试退款。
impl<T: pallet::Config> pallet_entity_common::CommissionFundGuard for pallet::Pallet<T> {
    fn protected_funds(entity_id: u64) -> u128 {
        use sp_runtime::SaturatedConversion;
        let pending: u128 = ShopPendingTotal::<T>::get(entity_id).saturated_into();
        let shopping: u128 = T::Loyalty::shopping_total(entity_id).saturated_into();
        let unallocated: u128 = UnallocatedPool::<T>::get(entity_id).saturated_into();
        let refund: u128 = PendingRefundTotal::<T>::get(entity_id).saturated_into();
        pending
            .saturating_add(shopping)
            .saturating_add(unallocated)
            .saturating_add(refund)
    }
}

/// CommissionFundBreakdown: 返回 protected 资金的分项明细
///
/// 供 Registry Runtime API (`get_entity_funds`) 向前端展示各类已承诺资金。
impl<T: pallet::Config> pallet_entity_common::CommissionFundBreakdown for pallet::Pallet<T> {
    fn protected_funds_breakdown(entity_id: u64) -> (u128, u128, u128, u128) {
        use sp_runtime::SaturatedConversion;
        let pending: u128 = ShopPendingTotal::<T>::get(entity_id).saturated_into();
        let shopping: u128 = T::Loyalty::shopping_total(entity_id).saturated_into();
        let unallocated: u128 = UnallocatedPool::<T>::get(entity_id).saturated_into();
        let refund: u128 = PendingRefundTotal::<T>::get(entity_id).saturated_into();
        (pending, shopping, unallocated, refund)
    }
}

/// CRITICAL-2 审计修复: CommissionCloseGuard — Entity 关闭前检查未结清佣金资金
///
/// 检查 pending commissions (NEX + Token)、shopping balances (NEX + Token)、
/// pending refunds，任一非零即返回 true 阻止关闭。
impl<T: pallet::Config> pallet_entity_common::CommissionCloseGuard for pallet::Pallet<T> {
    fn has_uncommitted_funds(entity_id: u64) -> bool {
        use sp_runtime::traits::Zero;
        // NEX pending commissions
        if !ShopPendingTotal::<T>::get(entity_id).is_zero() {
            return true;
        }
        // Token pending commissions
        if !TokenPendingTotal::<T>::get(entity_id).is_zero() {
            return true;
        }
        // NEX shopping balance total
        if !T::Loyalty::shopping_total(entity_id).is_zero() {
            return true;
        }
        // Token shopping balance total
        if !T::LoyaltyToken::token_shopping_total(entity_id).is_zero() {
            return true;
        }
        // Pending refunds awaiting retry (NEX)
        if !PendingRefundTotal::<T>::get(entity_id).is_zero() {
            return true;
        }
        // Pending Token refunds awaiting retry
        if !PendingTokenRefundTotal::<T>::get(entity_id).is_zero() {
            return true;
        }
        false
    }
}

/// CommissionProvider impl: 统一使用 entity_id，无需 shop_id 解析
// ============================================================================
// PluginBudgetCapProvider — 插件预算上限查询（供各插件校验配置）
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::PluginBudgetCapProvider for pallet::Pallet<T> {
    fn multi_level_cap(entity_id: u64) -> u16 {
        pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps.multi_level_cap)
            .unwrap_or(0)
    }
    fn referral_cap(entity_id: u64) -> u16 {
        pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps.referral_cap)
            .unwrap_or(0)
    }
    fn level_diff_cap(entity_id: u64) -> u16 {
        pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps.level_diff_cap)
            .unwrap_or(0)
    }
    fn single_line_cap(entity_id: u64) -> u16 {
        pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps.single_line_cap)
            .unwrap_or(0)
    }
    fn team_cap(entity_id: u64) -> u16 {
        pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps.team_cap)
            .unwrap_or(0)
    }
}

impl<T: pallet::Config> CommissionProvider<T::AccountId, pallet::BalanceOf<T>>
    for pallet::Pallet<T>
{
    fn process_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        order_amount: pallet::BalanceOf<T>,
        available_pool: pallet::BalanceOf<T>,
        platform_fee: pallet::BalanceOf<T>,
        product_id: u64,
        seller_reserved: pallet::BalanceOf<T>,
    ) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::process_commission(
            entity_id,
            shop_id,
            order_id,
            buyer,
            order_amount,
            available_pool,
            platform_fee,
            product_id,
            seller_reserved,
        )
    }

    fn cancel_commission(order_id: u64) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::cancel_commission(order_id)
    }

    fn pending_commission(entity_id: u64, account: &T::AccountId) -> pallet::BalanceOf<T> {
        pallet::MemberCommissionStats::<T>::get(entity_id, account).pending
    }

    fn set_commission_modes(entity_id: u64, modes: u16) -> sp_runtime::DispatchResult {
        // M4 审计修复: 使用 CommissionModes::is_valid()（单一事实来源）替代手动构造的掩码，
        // 与 extrinsic 版本保持一致，防止新增模式位时遗漏更新
        let modes_flags = CommissionModes(modes);
        frame_support::ensure!(
            modes_flags.is_valid(),
            sp_runtime::DispatchError::Other("InvalidModes")
        );

        // H-1 审计修复: 多插件组时要求已启用插件的 cap > 0
        let current_caps = pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.plugin_caps)
            .unwrap_or_default();
        pallet::Pallet::<T>::validate_plugin_caps_for_modes(&modes_flags, &current_caps)?;

        // H4 审计修复: 跟踪 POOL_REWARD 开关变化（与 extrinsic 版本一致）
        let old_has_pool = pallet::CommissionConfigs::<T>::get(entity_id)
            .map(|c| c.enabled_modes.contains(CommissionModes::POOL_REWARD))
            .unwrap_or(false);
        let new_has_pool = CommissionModes(modes).contains(CommissionModes::POOL_REWARD);

        // 沉淀池非空时不允许关闭 POOL_REWARD
        if old_has_pool && !new_has_pool {
            frame_support::ensure!(
                pallet::UnallocatedPool::<T>::get(entity_id).is_zero()
                    && pallet::UnallocatedTokenPool::<T>::get(entity_id).is_zero(),
                sp_runtime::DispatchError::Other("PoolNotEmpty")
            );
        }

        pallet::CommissionConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::CoreCommissionConfig::default);
            config.enabled_modes = CommissionModes(modes);
        });

        Ok(())
    }

    fn set_direct_reward_rate(entity_id: u64, rate: u16) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 10000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        <T as pallet::Config>::ReferralWriter::set_direct_rate(entity_id, rate)
    }

    fn set_level_diff_config(
        entity_id: u64,
        level_rates: alloc::vec::Vec<u16>,
    ) -> sp_runtime::DispatchResult {
        for &rate in level_rates.iter() {
            frame_support::ensure!(
                rate <= 10000,
                sp_runtime::DispatchError::Other("InvalidRate")
            );
        }
        let depth = level_rates.len() as u8;
        <T as pallet::Config>::LevelDiffWriter::set_level_rates(entity_id, level_rates, depth)
    }

    fn set_fixed_amount(
        entity_id: u64,
        amount: pallet::BalanceOf<T>,
    ) -> sp_runtime::DispatchResult {
        <T as pallet::Config>::ReferralWriter::set_fixed_amount(entity_id, amount)
    }

    fn set_first_order_config(
        entity_id: u64,
        amount: pallet::BalanceOf<T>,
        rate: u16,
        use_amount: bool,
    ) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 10000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        <T as pallet::Config>::ReferralWriter::set_first_order(entity_id, amount, rate, use_amount)
    }

    fn set_repeat_purchase_config(
        entity_id: u64,
        rate: u16,
        min_orders: u32,
    ) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 10000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        <T as pallet::Config>::ReferralWriter::set_repeat_purchase(entity_id, rate, min_orders)
    }

    fn set_withdrawal_config_by_governance(
        entity_id: u64,
        enabled: bool,
    ) -> sp_runtime::DispatchResult {
        pallet::WithdrawalConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::EntityWithdrawalConfig::default);
            config.enabled = enabled;
        });
        Ok(())
    }

    fn shopping_balance(entity_id: u64, account: &T::AccountId) -> pallet::BalanceOf<T> {
        T::Loyalty::shopping_balance(entity_id, account)
    }

    fn use_shopping_balance(
        entity_id: u64,
        account: &T::AccountId,
        amount: pallet::BalanceOf<T>,
    ) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::do_use_shopping_balance(entity_id, account, amount)
    }

    fn set_min_repurchase_rate(entity_id: u64, rate: u16) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 10000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        pallet::GlobalMinRepurchaseRate::<T>::insert(entity_id, rate);
        // L5 审计修复: 发射事件，使 NEX 治理底线变更可审计
        pallet::Pallet::<T>::deposit_event(pallet::Event::GlobalMinRepurchaseRateSet {
            entity_id,
            rate,
        });
        Ok(())
    }

    fn set_owner_reward_rate(entity_id: u64, rate: u16) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 5000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        pallet::CommissionConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::CoreCommissionConfig::default);
            config.owner_reward_rate = rate;
        });
        Ok(())
    }

    fn settle_order_commission(order_id: u64) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::do_settle_order_records(order_id)
    }

    fn process_shopping_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        shopping_amount: BalanceOf<T>,
        product_id: u64,
    ) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::process_shopping_commission(
            entity_id,
            shop_id,
            order_id,
            buyer,
            shopping_amount,
            product_id,
        )
    }

    fn cancel_shopping_commission(order_id: u64) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::cancel_shopping_commission(order_id)
    }

    fn settle_order_shopping_commission(order_id: u64) -> sp_runtime::DispatchResult {
        pallet::Pallet::<T>::do_settle_order_shopping_records(order_id)
    }

    // P0-1 审计修复: 治理提案真实执行（不再 no-op）
    fn governance_set_commission_rate(entity_id: u64, rate: u16) -> sp_runtime::DispatchResult {
        frame_support::ensure!(
            rate <= 10000,
            sp_runtime::DispatchError::Other("InvalidRate")
        );
        let ceiling = pallet::Pallet::<T>::commission_budget_ceiling();
        frame_support::ensure!(
            rate <= ceiling,
            sp_runtime::DispatchError::Other("ExceedsBudget")
        );
        let global_max = pallet::GlobalMaxCommissionRate::<T>::get(entity_id);
        if global_max > 0 {
            frame_support::ensure!(
                rate <= global_max,
                sp_runtime::DispatchError::Other("ExceedsGlobalMax")
            );
        }
        pallet::CommissionConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::CoreCommissionConfig::default);
            config.max_commission_rate = rate;
        });
        pallet::Pallet::<T>::deposit_event(pallet::Event::GovernanceCommissionRateSet {
            entity_id,
            rate,
        });
        Ok(())
    }

    fn governance_toggle_commission(entity_id: u64, enabled: bool) -> sp_runtime::DispatchResult {
        // 治理禁用时，沉淀池非空则阻止（与 Owner 路径一致）
        if !enabled {
            let old_pool_on = pallet::CommissionConfigs::<T>::get(entity_id)
                .map(|c| c.enabled && c.enabled_modes.contains(CommissionModes::POOL_REWARD))
                .unwrap_or(false);
            if old_pool_on {
                frame_support::ensure!(
                    pallet::UnallocatedPool::<T>::get(entity_id).is_zero()
                        && pallet::UnallocatedTokenPool::<T>::get(entity_id).is_zero(),
                    sp_runtime::DispatchError::Other("PoolNotEmpty")
                );
            }
        }
        pallet::CommissionConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::CoreCommissionConfig::default);
            config.enabled = enabled;
        });
        pallet::Pallet::<T>::deposit_event(pallet::Event::GovernanceCommissionToggled {
            entity_id,
            enabled,
        });
        Ok(())
    }
}

// ============================================================================
// PoolBalanceProvider 实现（供 pool-reward v2 访问沉淀池余额）
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::PoolBalanceProvider<pallet::BalanceOf<T>>
    for pallet::Pallet<T>
{
    fn pool_balance(entity_id: u64) -> pallet::BalanceOf<T> {
        pallet::UnallocatedPool::<T>::get(entity_id)
    }

    fn deduct_pool(
        entity_id: u64,
        amount: pallet::BalanceOf<T>,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::UnallocatedPool::<T>::try_mutate(entity_id, |pool| {
            frame_support::ensure!(
                *pool >= amount,
                sp_runtime::DispatchError::Other("InsufficientPool")
            );
            *pool = sp_runtime::Saturating::saturating_sub(*pool, amount);
            Ok(())
        })
    }
}

// ============================================================================
// TokenPoolBalanceProvider 实现（供 pool-reward 访问 Token 沉淀池）
// ============================================================================

impl<T: pallet::Config>
    pallet_commission_common::TokenPoolBalanceProvider<pallet::TokenBalanceOf<T>>
    for pallet::Pallet<T>
{
    fn token_pool_balance(entity_id: u64) -> pallet::TokenBalanceOf<T> {
        pallet::UnallocatedTokenPool::<T>::get(entity_id)
    }

    fn deduct_token_pool(
        entity_id: u64,
        amount: pallet::TokenBalanceOf<T>,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::UnallocatedTokenPool::<T>::try_mutate(
            entity_id,
            |pool| -> Result<(), sp_runtime::DispatchError> {
                frame_support::ensure!(
                    *pool >= amount,
                    sp_runtime::DispatchError::Other("InsufficientTokenPool")
                );
                *pool = sp_runtime::Saturating::saturating_sub(*pool, amount);
                Ok(())
            },
        )?;
        // B-2 修复: Token 池扣减时同步更新 EntityTokenAccountedBalance，
        // 防止 sweep_token_free_balance 因 accounted 偏高而无法检测外部转入
        pallet::EntityTokenAccountedBalance::<T>::mutate(entity_id, |b| {
            *b = Some(b.unwrap_or_default().saturating_sub(amount));
        });
        Ok(())
    }
}

// ============================================================================
// TokenCommissionProvider 实现（供 transaction 模块调用 Token 佣金管线）
// ============================================================================

impl<T: pallet::Config>
    pallet_commission_common::TokenCommissionProvider<T::AccountId, pallet::TokenBalanceOf<T>>
    for pallet::Pallet<T>
{
    fn process_token_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        token_order_amount: pallet::TokenBalanceOf<T>,
        token_available_pool: pallet::TokenBalanceOf<T>,
        token_platform_fee: pallet::TokenBalanceOf<T>,
        product_id: u64,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::Pallet::<T>::process_token_commission(
            entity_id,
            shop_id,
            order_id,
            buyer,
            token_order_amount,
            token_available_pool,
            token_platform_fee,
            product_id,
        )
    }

    fn cancel_token_commission(order_id: u64) -> Result<(), sp_runtime::DispatchError> {
        pallet::Pallet::<T>::do_cancel_token_commission(order_id)
    }

    fn pending_token_commission(
        entity_id: u64,
        account: &T::AccountId,
    ) -> pallet::TokenBalanceOf<T> {
        pallet::MemberTokenCommissionStats::<T>::get(entity_id, account).pending
    }

    fn token_platform_fee_rate(_entity_id: u64) -> u16 {
        pallet::TokenPlatformFeeRate::<T>::get()
    }
}

// ============================================================================
// Runtime API 聚合查询实现
// ============================================================================

impl<T: pallet::Config> pallet::Pallet<T> {
    pub fn get_member_commission_dashboard(
        entity_id: u64,
        account: &T::AccountId,
    ) -> Option<
        runtime_api::MemberCommissionDashboard<pallet::BalanceOf<T>, pallet::TokenBalanceOf<T>>,
    > {
        use pallet_commission_common::*;

        let nex_raw = pallet::MemberCommissionStats::<T>::get(entity_id, account);
        let token_raw = pallet::MemberTokenCommissionStats::<T>::get(entity_id, account);

        let has_data = nex_raw.order_count > 0
            || token_raw.order_count > 0
            || !nex_raw.total_earned.is_zero()
            || !token_raw.total_earned.is_zero()
            || T::MemberProvider::is_member(entity_id, account);

        if !has_data {
            return None;
        }

        let nex_stats = runtime_api::NexCommissionStats {
            total_earned: nex_raw.total_earned,
            pending: nex_raw.pending,
            withdrawn: nex_raw.withdrawn,
            repurchased: nex_raw.repurchased,
            order_count: nex_raw.order_count,
        };

        let token_stats = runtime_api::TokenCommissionStats {
            total_earned: token_raw.total_earned,
            pending: token_raw.pending,
            withdrawn: token_raw.withdrawn,
            repurchased: token_raw.repurchased,
            order_count: token_raw.order_count,
        };

        let multi_level_progress = T::MultiLevelQuery::activation_progress(entity_id, account);
        let multi_level_stats = T::MultiLevelQuery::member_stats(entity_id, account);

        let team_tier = T::TeamQuery::matched_tier(entity_id, account);

        let sl_position = T::SingleLineQuery::position(entity_id, account);
        let sl_levels = T::SingleLineQuery::effective_levels(entity_id, account);
        let single_line = runtime_api::SingleLineSnapshot {
            position: sl_position,
            upline_levels: sl_levels.map(|(u, _)| u),
            downline_levels: sl_levels.map(|(_, d)| d),
            is_enabled: T::SingleLineQuery::is_enabled(entity_id),
            queue_length: T::SingleLineQuery::queue_length(entity_id),
        };

        let (claimable_nex, claimable_token) = T::PoolRewardQuery::claimable(entity_id, account);
        let pool_reward = runtime_api::PoolRewardSnapshot {
            claimable_nex,
            claimable_token,
            is_paused: T::PoolRewardQuery::is_paused(entity_id),
            current_round_id: T::PoolRewardQuery::current_round_id(entity_id),
        };

        let referral_earned = T::ReferralQuery::referrer_total_earned(entity_id, account);
        let cap = T::ReferralQuery::cap_config(entity_id);
        let referral = runtime_api::ReferralSnapshot {
            total_earned: referral_earned,
            cap_max_per_order: cap.map(|(per, _)| per),
            cap_max_total: cap.map(|(_, total)| total),
        };

        Some(runtime_api::MemberCommissionDashboard {
            nex_stats,
            token_stats,
            nex_shopping_balance: T::Loyalty::shopping_balance(entity_id, account),
            token_shopping_balance: T::LoyaltyToken::token_shopping_balance(entity_id, account),
            multi_level_progress,
            multi_level_stats,
            team_tier,
            single_line,
            pool_reward,
            referral,
        })
    }

    pub fn get_direct_referral_info(
        entity_id: u64,
        account: &T::AccountId,
    ) -> runtime_api::DirectReferralInfo<pallet::BalanceOf<T>> {
        let referral_earned = T::ReferralQuery::referrer_total_earned(entity_id, account);
        let cap = T::ReferralQuery::cap_config(entity_id);

        let cap_remaining = cap.and_then(|(_, max_total)| {
            if max_total.is_zero() {
                None
            } else {
                Some(max_total.saturating_sub(referral_earned))
            }
        });

        runtime_api::DirectReferralInfo {
            referral_total_earned: referral_earned,
            cap_max_per_order: cap
                .and_then(|(per, _)| if per.is_zero() { None } else { Some(per) }),
            cap_max_total: cap
                .and_then(|(_, total)| if total.is_zero() { None } else { Some(total) }),
            cap_remaining,
        }
    }

    pub fn get_team_performance_info(
        entity_id: u64,
        account: &T::AccountId,
    ) -> runtime_api::TeamPerformanceInfo<pallet::BalanceOf<T>> {
        let (config_exists, is_enabled) = T::TeamQuery::status(entity_id);
        let current_tier = T::TeamQuery::matched_tier(entity_id, account);
        let stats = T::MemberProvider::get_member_stats(entity_id, account);

        runtime_api::TeamPerformanceInfo {
            team_size: stats.team_size,
            direct_referrals: stats.direct_referrals,
            total_spent: stats.spend.total_spent,
            current_tier,
            is_enabled,
            config_exists,
        }
    }

    pub fn get_direct_referral_details(
        entity_id: u64,
        account: &T::AccountId,
    ) -> runtime_api::DirectReferralDetails<T::AccountId, pallet::BalanceOf<T>> {
        let referral_accounts = T::MemberProvider::get_direct_referral_accounts(entity_id, account);
        let total_count = referral_accounts.len() as u32;
        let mut total_commission_earned = pallet::BalanceOf::<T>::zero();
        let mut referrals = alloc::vec::Vec::with_capacity(referral_accounts.len());

        for ref_account in referral_accounts {
            let stats = T::MemberProvider::get_member_stats(entity_id, &ref_account);
            let level_id = T::MemberProvider::get_effective_level(entity_id, &ref_account);
            let order_count = T::MemberProvider::completed_order_count(entity_id, &ref_account);
            let joined_at = T::MemberProvider::referral_registered_at(entity_id, &ref_account);
            let last_active_at = T::MemberProvider::last_active_at(entity_id, &ref_account);
            let is_active = T::MemberProvider::is_member_active(entity_id, &ref_account)
                && T::MemberProvider::is_activated(entity_id, &ref_account);

            let contributed = ReferrerEarnedByBuyer::<T>::get((entity_id, account, &ref_account));
            total_commission_earned = total_commission_earned.saturating_add(contributed);

            referrals.push(runtime_api::DirectReferralMember {
                account: ref_account,
                level_id,
                total_spent: stats.spend.total_spent as u64,
                order_count,
                joined_at,
                last_active_at,
                is_active,
                team_size: stats.team_size,
                direct_referrals: stats.direct_referrals,
                commission_contributed: contributed,
            });
        }

        let cap = T::ReferralQuery::cap_config(entity_id);
        let cap_max_total = cap.and_then(
            |(_, total)| {
                if total.is_zero() {
                    None
                } else {
                    Some(total)
                }
            },
        );
        let referral_earned = T::ReferralQuery::referrer_total_earned(entity_id, account);
        let cap_remaining = cap_max_total.map(|max| max.saturating_sub(referral_earned));

        runtime_api::DirectReferralDetails {
            referrals,
            total_count,
            total_commission_earned,
            cap_max_total,
            cap_remaining,
        }
    }

    pub fn get_entity_commission_overview(
        entity_id: u64,
    ) -> runtime_api::EntityCommissionOverview<pallet::BalanceOf<T>, pallet::TokenBalanceOf<T>>
    {
        let config = pallet::CommissionConfigs::<T>::get(entity_id).unwrap_or_default();

        let (team_exists, team_enabled) = T::TeamQuery::status(entity_id);

        runtime_api::EntityCommissionOverview {
            enabled_modes: config.enabled_modes.0,
            max_commission_rate: config.max_commission_rate,
            is_enabled: config.enabled,
            pending_total_nex: pallet::ShopPendingTotal::<T>::get(entity_id),
            pending_total_token: pallet::TokenPendingTotal::<T>::get(entity_id),
            unallocated_pool_nex: pallet::UnallocatedPool::<T>::get(entity_id),
            unallocated_pool_token: pallet::UnallocatedTokenPool::<T>::get(entity_id),
            shopping_total_nex: T::Loyalty::shopping_total(entity_id),
            shopping_total_token: T::LoyaltyToken::token_shopping_total(entity_id),
            multi_level_paused: T::MultiLevelQuery::is_paused(entity_id),
            single_line_enabled: T::SingleLineQuery::is_enabled(entity_id),
            team_status: (team_exists, team_enabled),
            pool_reward_paused: T::PoolRewardQuery::is_paused(entity_id),
            withdrawal_paused: pallet::WithdrawalPaused::<T>::get(entity_id),
        }
    }

    pub fn get_member_withdrawal_records(
        entity_id: u64,
        account: &T::AccountId,
    ) -> alloc::vec::Vec<runtime_api::WithdrawalRecordView<pallet::BalanceOf<T>>> {
        let records = pallet::MemberWithdrawalHistory::<T>::get(entity_id, account);
        records
            .into_iter()
            .map(|r| runtime_api::WithdrawalRecordView {
                total_amount: r.total_amount,
                withdrawn: r.withdrawn,
                repurchased: r.repurchased,
                bonus: r.bonus,
                block_number: r.block_number.try_into().unwrap_or(0u64),
            })
            .collect()
    }
}

// ============================================================================
// CommissionGovernancePort 实现
// ============================================================================

impl<T: pallet::Config> pallet_entity_common::CommissionGovernancePort<pallet::BalanceOf<T>>
    for pallet::Pallet<T>
{
    fn governance_set_withdrawal_cooldown(
        entity_id: u64,
        nex_cooldown: u32,
        token_cooldown: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::CommissionConfigs::<T>::mutate(entity_id, |maybe| {
            let config = maybe.get_or_insert_with(pallet::CoreCommissionConfig::default);
            config.withdrawal_cooldown = nex_cooldown;
            config.token_withdrawal_cooldown = token_cooldown;
        });
        Ok(())
    }

    fn governance_set_token_withdrawal(
        entity_id: u64,
        enabled: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        // Token 提现启用/禁用通过 WithdrawalConfig 控制
        if let Some(mut wc) = pallet::TokenWithdrawalConfigs::<T>::get(entity_id) {
            wc.enabled = enabled;
            pallet::TokenWithdrawalConfigs::<T>::insert(entity_id, wc);
        }
        Ok(())
    }

    fn governance_set_withdrawal_pause(
        entity_id: u64,
        paused: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::WithdrawalPaused::<T>::insert(entity_id, paused);
        pallet::Pallet::<T>::deposit_event(pallet::Event::WithdrawalPauseToggled {
            entity_id,
            paused,
        });
        Ok(())
    }

    fn governance_set_referrer_guard(
        entity_id: u64,
        min_referrer_spent: pallet::BalanceOf<T>,
        min_referrer_orders: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        T::ReferralWriter::set_referrer_guard(
            entity_id,
            min_referrer_spent.saturated_into::<u128>(),
            min_referrer_orders,
        )
    }

    fn governance_set_commission_cap(
        entity_id: u64,
        max_per_order: pallet::BalanceOf<T>,
        max_total_earned: pallet::BalanceOf<T>,
    ) -> Result<(), sp_runtime::DispatchError> {
        T::ReferralWriter::set_commission_cap(entity_id, max_per_order, max_total_earned)
    }

    fn governance_pause_multi_level(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        T::MultiLevelWriter::governance_pause(entity_id)
    }

    fn governance_resume_multi_level(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        T::MultiLevelWriter::governance_resume(entity_id)
    }

    fn governance_pause_team_performance(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        T::TeamWriter::governance_pause(entity_id)
    }

    fn governance_resume_team_performance(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        T::TeamWriter::governance_resume(entity_id)
    }

    fn governance_set_pool_reward_config(
        entity_id: u64,
        level_rules: alloc::vec::Vec<(u8, pallet_entity_common::PoolRewardLevelClaimRule)>,
        round_duration: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        T::PoolRewardWriter::set_pool_reward_config(entity_id, level_rules, round_duration)
    }

    fn governance_clear_pool_reward_config(
        entity_id: u64,
    ) -> Result<(), sp_runtime::DispatchError> {
        T::PoolRewardWriter::clear_config(entity_id)
    }

    fn governance_set_pool_reward_token_enabled(
        entity_id: u64,
        enabled: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        T::PoolRewardWriter::set_token_pool_enabled(entity_id, enabled)
    }

    fn governance_set_referrer_exempt_threshold(
        threshold: u16,
    ) -> Result<(), sp_runtime::DispatchError> {
        let old = pallet::ReferrerExemptThreshold::<T>::get();
        pallet::ReferrerExemptThreshold::<T>::put(threshold);
        Pallet::<T>::deposit_event(pallet::Event::ReferrerExemptThresholdChanged {
            old_threshold: old,
            new_threshold: threshold,
        });
        Ok(())
    }
}
