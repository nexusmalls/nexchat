//! # Commission Single-Line Plugin (pallet-commission-single-line)
//!
//! 单线收益插件：基于全局消费注册顺序的上下线收益。
//! - 上线收益 (SingleLineUpline)
//! - 下线收益 (SingleLineDownline)
//! - 层数随消费额动态增长

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use pallet_entity_common::EntityProvider;

pub use pallet::*;
pub mod runtime_api;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::weights::WeightInfo;
    use alloc::vec::Vec;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, Get},
    };
    use frame_system::pallet_prelude::*;
    use pallet_commission_common::{
        CommissionOutput, CommissionType, MemberCommissionStatsData, MemberProvider,
        PluginBudgetCapProvider,
    };
    use pallet_entity_common::{AdminPermission, EntityProvider};
    use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
    use sp_runtime::Saturating;

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // ========================================================================
    // Config structs
    // ========================================================================

    /// Commission reach mode for single-line distribution.
    /// 公排佣金覆盖模式。
    ///
    /// Pre-mainnet destructive simplification: `BeneficiaryOnly` is the canonical default.
    /// 主网上线前的破坏式简化：`BeneficiaryOnly` 是唯一认可的默认值。
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
    pub enum ReachMode {
        /// Either buyer's reach or beneficiary's reverse reach is sufficient (OR logic).
        /// 双向覆盖：买家或受益人任一方能覆盖即分佣。
        Bidirectional,
        /// Only the buyer's level-based reach determines commission eligibility.
        /// 仅买家覆盖：只有买家的等级层数决定谁能拿佣金。
        BuyerOnly,
        /// Only the beneficiary's reverse reach determines commission eligibility.
        /// 仅受益人覆盖：只有受益人的反向层数决定谁能拿佣金。
        #[default]
        BeneficiaryOnly,
    }

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
    pub struct SingleLineConfig<Balance> {
        pub upline_rate: u16,
        pub downline_rate: u16,
        pub base_upline_levels: u8,
        pub base_downline_levels: u8,
        pub level_increment_threshold: Balance,
        pub max_upline_levels: u8,
        pub max_downline_levels: u8,
        pub reach_mode: ReachMode,
    }

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
    pub struct LevelBasedLevels {
        pub upline_levels: u8,
        pub downline_levels: u8,
    }

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
    pub struct PendingConfigChange<Balance, BlockNumber> {
        pub upline_rate: u16,
        pub downline_rate: u16,
        pub base_upline_levels: u8,
        pub base_downline_levels: u8,
        pub level_increment_threshold: Balance,
        pub max_upline_levels: u8,
        pub max_downline_levels: u8,
        pub apply_after: BlockNumber,
        pub reach_mode: ReachMode,
    }

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
    pub struct ConfigChangeLogEntry<Balance, BlockNumber> {
        pub block_number: BlockNumber,
        pub upline_rate: u16,
        pub downline_rate: u16,
        pub base_upline_levels: u8,
        pub base_downline_levels: u8,
        pub max_upline_levels: u8,
        pub max_downline_levels: u8,
        pub level_increment_threshold: Balance,
        pub reach_mode: ReachMode,
    }

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
    pub struct EntitySingleLineStatsData {
        pub total_orders: u32,
        pub total_upline_payouts: u32,
        pub total_downline_payouts: u32,
    }

    // ========================================================================
    // Payout history structs
    // ========================================================================

    /// 分佣方向
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
    pub enum PayoutDirection {
        /// 我是买家的上线，买家下单时我作为上线获得佣金
        Upline,
        /// 我是买家的下线，买家下单时我作为下线获得佣金
        Downline,
    }

    /// 单条分佣记录
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
    pub struct SingleLinePayoutRecord<AccountId, Balance> {
        /// 触发分佣的订单 ID
        pub order_id: u64,
        /// 触发分佣的订单买家
        pub buyer: AccountId,
        /// 分佣金额 (NEX)
        pub amount: Balance,
        /// 方向: 作为上线获得 or 作为下线获得
        pub direction: PayoutDirection,
        /// 与买家的层级距离 (1 = 直接邻位, 2 = 隔一位 ...)
        pub level_distance: u16,
        /// 发生区块
        pub block_number: u64,
    }

    /// 会员级汇总统计
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
    pub struct MemberSingleLineSummary<Balance: Default> {
        /// 作为上线累计获得
        pub total_earned_as_upline: Balance,
        /// 作为下线累计获得
        pub total_earned_as_downline: Balance,
        /// 总分佣笔数
        pub total_payout_count: u32,
        /// 最后一次分佣区块
        pub last_payout_block: u64,
    }

    // ========================================================================
    // Pallet Config
    // ========================================================================

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type Currency: Currency<Self::AccountId>;

        type StatsProvider: SingleLineStatsProvider<Self::AccountId, BalanceOf<Self>>;
        type MemberLevelProvider: SingleLineMemberLevelProvider<Self::AccountId>;
        type EntityProvider: EntityProvider<Self::AccountId>;
        type MemberProvider: pallet_commission_common::MemberProvider<Self::AccountId>;
        /// 插件预算上限查询
        type BudgetCapProvider: PluginBudgetCapProvider;
        type WeightInfo: WeightInfo;

        #[pallet::constant]
        type MaxSingleLineLength: Get<u32>;

        #[pallet::constant]
        type ConfigChangeDelay: Get<BlockNumberFor<Self>>;

        #[pallet::constant]
        type MaxSegmentCount: Get<u32>;

        /// upline_rate × max_upline_levels + downline_rate × max_downline_levels 的上限
        #[pallet::constant]
        type MaxTotalRateBps: Get<u32>;

        /// 每实体配置变更日志最大条数（循环覆写）
        #[pallet::constant]
        type MaxConfigChangeLogs: Get<u32>;

        /// 每会员最多保留的分佣历史记录条数 (FIFO 环形缓冲)
        #[pallet::constant]
        type MaxPayoutRecords: Get<u32>;
    }

    pub trait SingleLineStatsProvider<AccountId, Balance: Default> {
        fn get_member_stats(
            entity_id: u64,
            account: &AccountId,
        ) -> MemberCommissionStatsData<Balance>;
    }

    impl<AccountId, Balance: Default> SingleLineStatsProvider<AccountId, Balance> for () {
        fn get_member_stats(_: u64, _: &AccountId) -> MemberCommissionStatsData<Balance> {
            MemberCommissionStatsData::default()
        }
    }

    pub trait SingleLineMemberLevelProvider<AccountId> {
        fn custom_level_id(entity_id: u64, account: &AccountId) -> u8;
        /// 查询实体的自定义等级数量（用于 level_id 存在性校验）
        fn custom_level_count(entity_id: u64) -> u8;
    }

    impl<AccountId> SingleLineMemberLevelProvider<AccountId> for () {
        fn custom_level_id(_: u64, _: &AccountId) -> u8 {
            0
        }
        fn custom_level_count(_: u64) -> u8 {
            0
        }
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "std")]
        fn integrity_test() {
            assert!(
                T::MaxSingleLineLength::get() > 0,
                "MaxSingleLineLength must be > 0"
            );
            assert!(T::MaxSegmentCount::get() > 0, "MaxSegmentCount must be > 0");
        }
    }

    // ========================================================================
    // Storage
    // ========================================================================

    #[pallet::storage]
    #[pallet::getter(fn single_line_config)]
    pub type SingleLineConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, SingleLineConfig<BalanceOf<T>>>;

    #[pallet::storage]
    pub type SingleLineSegments<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u32,
        BoundedVec<T::AccountId, T::MaxSingleLineLength>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type SingleLineSegmentCount<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u32, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn single_line_index)]
    pub type SingleLineIndex<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u64, Blake2_128Concat, T::AccountId, u32>;

    #[pallet::storage]
    #[pallet::getter(fn custom_level_overrides)]
    pub type SingleLineCustomLevelOverrides<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u64, Blake2_128Concat, u8, LevelBasedLevels>;

    #[pallet::storage]
    pub type SingleLineEnabled<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery, EnabledDefault>;

    #[pallet::type_value]
    pub fn EnabledDefault() -> bool {
        true
    }

    #[pallet::storage]
    pub type PendingConfigChanges<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PendingConfigChange<BalanceOf<T>, BlockNumberFor<T>>>;

    #[pallet::storage]
    pub type RemovedMembers<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        bool,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type ConfigChangeLogs<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        u32,
        ConfigChangeLogEntry<BalanceOf<T>, BlockNumberFor<T>>,
    >;

    #[pallet::storage]
    pub type ConfigChangeLogCount<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, u32, ValueQuery>;

    #[pallet::storage]
    pub type EntitySingleLineStats<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, EntitySingleLineStatsData, ValueQuery>;

    /// 会员的排位分佣记录 (FIFO, 最多保留最近 MaxPayoutRecords 条)
    #[pallet::storage]
    pub type MemberSingleLinePayouts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<SingleLinePayoutRecord<T::AccountId, BalanceOf<T>>, T::MaxPayoutRecords>,
        ValueQuery,
    >;

    /// 会员级别汇总统计
    #[pallet::storage]
    pub type MemberSingleLineStats<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        MemberSingleLineSummary<BalanceOf<T>>,
        ValueQuery,
    >;

    // ========================================================================
    // Events / Errors
    // ========================================================================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SingleLineConfigUpdated {
            entity_id: u64,
            upline_rate: u16,
            downline_rate: u16,
            base_upline_levels: u8,
            base_downline_levels: u8,
            max_upline_levels: u8,
            max_downline_levels: u8,
            reach_mode: ReachMode,
        },
        SingleLineConfigCleared {
            entity_id: u64,
        },
        AddedToSingleLine {
            entity_id: u64,
            account: T::AccountId,
            index: u32,
        },
        SingleLineJoinFailed {
            entity_id: u64,
            account: T::AccountId,
        },
        LevelBasedLevelsUpdated {
            entity_id: u64,
            level_id: u8,
        },
        LevelBasedLevelsRemoved {
            entity_id: u64,
            level_id: u8,
        },
        SingleLinePaused {
            entity_id: u64,
        },
        SingleLineResumed {
            entity_id: u64,
        },
        SingleLineReset {
            entity_id: u64,
            removed_count: u32,
        },
        SingleLineResetCompleted {
            entity_id: u64,
        },
        NewSegmentCreated {
            entity_id: u64,
            segment_id: u32,
        },
        AllLevelOverridesCleared {
            entity_id: u64,
        },
        ConfigChangeScheduled {
            entity_id: u64,
            apply_after: BlockNumberFor<T>,
        },
        PendingConfigApplied {
            entity_id: u64,
        },
        PendingConfigCancelled {
            entity_id: u64,
        },
        MemberRemovedFromSingleLine {
            entity_id: u64,
            account: T::AccountId,
        },
        MemberRestoredToSingleLine {
            entity_id: u64,
            account: T::AccountId,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidRate,
        InvalidLevels,
        BaseLevelsExceedMax,
        EntityNotFound,
        NotEntityOwnerOrAdmin,
        ConfigNotFound,
        NothingToUpdate,
        EntityLocked,
        EntityNotActive,
        SingleLineIsPaused,
        SingleLineNotPaused,
        PendingConfigAlreadyExists,
        PendingConfigNotFound,
        PendingConfigNotReady,
        MemberNotInSingleLine,
        MaxSegmentCountReached,
        RatesTooHigh,
        LevelOverrideExceedsMax,
        /// level_id 不存在于实体等级系统中
        LevelIdNotFound,
        /// 最大费率总和超过插件预算上限
        MaxPayoutExceedsBudgetCap,
    }

    // ========================================================================
    // Extrinsics
    // ========================================================================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_single_line_config())]
        pub fn set_single_line_config(
            origin: OriginFor<T>,
            entity_id: u64,
            upline_rate: u16,
            downline_rate: u16,
            base_upline_levels: u8,
            base_downline_levels: u8,
            level_increment_threshold: BalanceOf<T>,
            max_upline_levels: u8,
            max_downline_levels: u8,
            reach_mode: ReachMode,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            Self::do_set_config(
                entity_id,
                SingleLineConfig {
                    upline_rate,
                    downline_rate,
                    base_upline_levels,
                    base_downline_levels,
                    level_increment_threshold,
                    max_upline_levels,
                    max_downline_levels,
                    reach_mode,
                },
            )
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::clear_single_line_config())]
        pub fn clear_single_line_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                SingleLineConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            Self::do_clear_config(entity_id);
            Ok(())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::update_single_line_params())]
        pub fn update_single_line_params(
            origin: OriginFor<T>,
            entity_id: u64,
            upline_rate: Option<u16>,
            downline_rate: Option<u16>,
            level_increment_threshold: Option<BalanceOf<T>>,
            base_upline_levels: Option<u8>,
            base_downline_levels: Option<u8>,
            max_upline_levels: Option<u8>,
            max_downline_levels: Option<u8>,
            reach_mode: Option<ReachMode>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                upline_rate.is_some()
                    || downline_rate.is_some()
                    || level_increment_threshold.is_some()
                    || base_upline_levels.is_some()
                    || base_downline_levels.is_some()
                    || max_upline_levels.is_some()
                    || max_downline_levels.is_some()
                    || reach_mode.is_some(),
                Error::<T>::NothingToUpdate
            );

            let config = SingleLineConfigs::<T>::try_mutate(
                entity_id,
                |maybe| -> Result<SingleLineConfig<BalanceOf<T>>, DispatchError> {
                    let c = maybe.as_mut().ok_or(Error::<T>::ConfigNotFound)?;
                    if let Some(r) = upline_rate {
                        c.upline_rate = r;
                    }
                    if let Some(r) = downline_rate {
                        c.downline_rate = r;
                    }
                    if let Some(t) = level_increment_threshold {
                        c.level_increment_threshold = t;
                    }
                    if let Some(v) = base_upline_levels {
                        c.base_upline_levels = v;
                    }
                    if let Some(v) = base_downline_levels {
                        c.base_downline_levels = v;
                    }
                    if let Some(v) = max_upline_levels {
                        c.max_upline_levels = v;
                    }
                    if let Some(v) = max_downline_levels {
                        c.max_downline_levels = v;
                    }
                    if let Some(v) = reach_mode {
                        c.reach_mode = v;
                    }
                    Self::validate_config(
                        c.upline_rate,
                        c.downline_rate,
                        c.base_upline_levels,
                        c.base_downline_levels,
                        c.max_upline_levels,
                        c.max_downline_levels,
                    )?;
                    Self::ensure_within_budget_cap(
                        entity_id,
                        c.upline_rate,
                        c.downline_rate,
                        c.max_upline_levels,
                        c.max_downline_levels,
                    )?;
                    Ok(c.clone())
                },
            )?;

            Self::do_clamp_level_overrides(
                entity_id,
                config.max_upline_levels,
                config.max_downline_levels,
            );
            Self::record_change_log(entity_id, &config);
            Self::deposit_event(Event::SingleLineConfigUpdated {
                entity_id,
                upline_rate: config.upline_rate,
                downline_rate: config.downline_rate,
                base_upline_levels: config.base_upline_levels,
                base_downline_levels: config.base_downline_levels,
                max_upline_levels: config.max_upline_levels,
                max_downline_levels: config.max_downline_levels,
                reach_mode: config.reach_mode,
            });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_level_based_levels())]
        pub fn set_level_based_levels(
            origin: OriginFor<T>,
            entity_id: u64,
            level_id: u8,
            upline_levels: u8,
            downline_levels: u8,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            Self::do_set_level_override(entity_id, level_id, upline_levels, downline_levels)
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::remove_level_based_levels())]
        pub fn remove_level_based_levels(
            origin: OriginFor<T>,
            entity_id: u64,
            level_id: u8,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            if SingleLineCustomLevelOverrides::<T>::contains_key(entity_id, level_id) {
                SingleLineCustomLevelOverrides::<T>::remove(entity_id, level_id);
                Self::deposit_event(Event::LevelBasedLevelsRemoved {
                    entity_id,
                    level_id,
                });
            }
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::force_set_single_line_config())]
        pub fn force_set_single_line_config(
            origin: OriginFor<T>,
            entity_id: u64,
            upline_rate: u16,
            downline_rate: u16,
            base_upline_levels: u8,
            base_downline_levels: u8,
            level_increment_threshold: BalanceOf<T>,
            max_upline_levels: u8,
            max_downline_levels: u8,
            reach_mode: ReachMode,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_set_config(
                entity_id,
                SingleLineConfig {
                    upline_rate,
                    downline_rate,
                    base_upline_levels,
                    base_downline_levels,
                    level_increment_threshold,
                    max_upline_levels,
                    max_downline_levels,
                    reach_mode,
                },
            )
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::force_clear_single_line_config())]
        pub fn force_clear_single_line_config(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_clear_config(entity_id);
            Ok(())
        }

        /// 分批重置单链数据（从末尾段开始，每次最多处理 `limit` 个段）
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::force_reset_single_line(*limit))]
        pub fn force_reset_single_line(
            origin: OriginFor<T>,
            entity_id: u64,
            limit: u32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            let (removed, done) = Self::do_reset_single_line_batched(entity_id, limit);
            if removed > 0 {
                Self::deposit_event(Event::SingleLineReset {
                    entity_id,
                    removed_count: removed,
                });
            }
            if done {
                Self::deposit_event(Event::SingleLineResetCompleted { entity_id });
            }
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::pause_single_line())]
        pub fn pause_single_line(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                SingleLineEnabled::<T>::get(entity_id),
                Error::<T>::SingleLineIsPaused
            );

            SingleLineEnabled::<T>::insert(entity_id, false);
            Self::deposit_event(Event::SingleLinePaused { entity_id });
            Ok(())
        }

        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::resume_single_line())]
        pub fn resume_single_line(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                !SingleLineEnabled::<T>::get(entity_id),
                Error::<T>::SingleLineNotPaused
            );

            SingleLineEnabled::<T>::insert(entity_id, true);
            Self::deposit_event(Event::SingleLineResumed { entity_id });
            Ok(())
        }

        /// 调度配置变更（延迟 ConfigChangeDelay 个区块后才可 apply）
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::schedule_config_change())]
        pub fn schedule_config_change(
            origin: OriginFor<T>,
            entity_id: u64,
            upline_rate: u16,
            downline_rate: u16,
            base_upline_levels: u8,
            base_downline_levels: u8,
            level_increment_threshold: BalanceOf<T>,
            max_upline_levels: u8,
            max_downline_levels: u8,
            reach_mode: ReachMode,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                !PendingConfigChanges::<T>::contains_key(entity_id),
                Error::<T>::PendingConfigAlreadyExists
            );
            Self::validate_config(
                upline_rate,
                downline_rate,
                base_upline_levels,
                base_downline_levels,
                max_upline_levels,
                max_downline_levels,
            )?;
            Self::ensure_within_budget_cap(
                entity_id,
                upline_rate,
                downline_rate,
                max_upline_levels,
                max_downline_levels,
            )?;

            let apply_after = <frame_system::Pallet<T>>::block_number()
                .saturating_add(T::ConfigChangeDelay::get());

            PendingConfigChanges::<T>::insert(
                entity_id,
                PendingConfigChange {
                    upline_rate,
                    downline_rate,
                    base_upline_levels,
                    base_downline_levels,
                    level_increment_threshold,
                    max_upline_levels,
                    max_downline_levels,
                    apply_after,
                    reach_mode,
                },
            );
            Self::deposit_event(Event::ConfigChangeScheduled {
                entity_id,
                apply_after,
            });
            Ok(())
        }

        /// 应用待生效配置（任何人可调用，需过延迟期）
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::apply_pending_config())]
        pub fn apply_pending_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            ensure_signed(origin)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            let pending = PendingConfigChanges::<T>::get(entity_id)
                .ok_or(Error::<T>::PendingConfigNotFound)?;
            ensure!(
                <frame_system::Pallet<T>>::block_number() >= pending.apply_after,
                Error::<T>::PendingConfigNotReady
            );

            let config = SingleLineConfig {
                upline_rate: pending.upline_rate,
                downline_rate: pending.downline_rate,
                base_upline_levels: pending.base_upline_levels,
                base_downline_levels: pending.base_downline_levels,
                level_increment_threshold: pending.level_increment_threshold,
                max_upline_levels: pending.max_upline_levels,
                max_downline_levels: pending.max_downline_levels,
                reach_mode: pending.reach_mode,
            };
            PendingConfigChanges::<T>::remove(entity_id);
            // do_set_config handles: insert + clamp + log + ConfigUpdated event
            Self::do_set_config(entity_id, config)?;
            Self::deposit_event(Event::PendingConfigApplied { entity_id });
            Ok(())
        }

        /// 取消待生效配置
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::cancel_pending_config())]
        pub fn cancel_pending_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(entity_id, &who)?;
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PendingConfigChanges::<T>::contains_key(entity_id),
                Error::<T>::PendingConfigNotFound
            );

            PendingConfigChanges::<T>::remove(entity_id);
            Self::deposit_event(Event::PendingConfigCancelled { entity_id });
            Ok(())
        }

        /// [Root] 逻辑移除成员（遍历时跳过，不改变链结构）
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::force_remove_from_single_line())]
        pub fn force_remove_from_single_line(
            origin: OriginFor<T>,
            entity_id: u64,
            account: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                SingleLineIndex::<T>::contains_key(entity_id, &account),
                Error::<T>::MemberNotInSingleLine
            );
            RemovedMembers::<T>::insert(entity_id, &account, true);
            Self::deposit_event(Event::MemberRemovedFromSingleLine { entity_id, account });
            Ok(())
        }

        /// [Root] 恢复已逻辑移除的成员
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::force_remove_from_single_line())]
        pub fn force_restore_to_single_line(
            origin: OriginFor<T>,
            entity_id: u64,
            account: T::AccountId,
        ) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                RemovedMembers::<T>::get(entity_id, &account),
                Error::<T>::MemberNotInSingleLine
            );
            RemovedMembers::<T>::remove(entity_id, &account);
            Self::deposit_event(Event::MemberRestoredToSingleLine { entity_id, account });
            Ok(())
        }
    }

    // ========================================================================
    // Internal functions
    // ========================================================================

    impl<T: Config> Pallet<T> {
        fn ensure_owner_or_admin(entity_id: u64, who: &T::AccountId) -> DispatchResult {
            let owner =
                T::EntityProvider::entity_owner(entity_id).ok_or(Error::<T>::EntityNotFound)?;
            if *who == owner {
                return Ok(());
            }
            ensure!(
                T::EntityProvider::is_entity_admin(
                    entity_id,
                    who,
                    AdminPermission::COMMISSION_MANAGE
                ),
                Error::<T>::NotEntityOwnerOrAdmin
            );
            Ok(())
        }

        pub(crate) fn validate_config(
            upline_rate: u16,
            downline_rate: u16,
            base_upline_levels: u8,
            base_downline_levels: u8,
            max_upline_levels: u8,
            max_downline_levels: u8,
        ) -> DispatchResult {
            ensure!(
                upline_rate <= 1000 && downline_rate <= 1000,
                Error::<T>::InvalidRate
            );
            ensure!(
                base_upline_levels <= max_upline_levels,
                Error::<T>::BaseLevelsExceedMax
            );
            ensure!(
                base_downline_levels <= max_downline_levels,
                Error::<T>::BaseLevelsExceedMax
            );
            let max_payout = (upline_rate as u32)
                .saturating_mul(max_upline_levels as u32)
                .saturating_add((downline_rate as u32).saturating_mul(max_downline_levels as u32));
            ensure!(
                max_payout <= T::MaxTotalRateBps::get(),
                Error::<T>::RatesTooHigh
            );
            Ok(())
        }

        /// 校验最大费率总和不超过插件预算上限（cap=0 表示无上限，跳过校验）
        fn ensure_within_budget_cap(
            entity_id: u64,
            upline_rate: u16,
            downline_rate: u16,
            max_upline_levels: u8,
            max_downline_levels: u8,
        ) -> DispatchResult {
            let cap = T::BudgetCapProvider::single_line_cap(entity_id);
            if cap > 0 {
                let max_payout = (upline_rate as u32)
                    .saturating_mul(max_upline_levels as u32)
                    .saturating_add(
                        (downline_rate as u32).saturating_mul(max_downline_levels as u32),
                    );
                ensure!(
                    max_payout <= cap as u32,
                    Error::<T>::MaxPayoutExceedsBudgetCap
                );
            }
            Ok(())
        }

        /// 核心写入逻辑：validate → cap check → insert → clamp → log → event
        pub(crate) fn do_set_config(
            entity_id: u64,
            config: SingleLineConfig<BalanceOf<T>>,
        ) -> DispatchResult {
            Self::validate_config(
                config.upline_rate,
                config.downline_rate,
                config.base_upline_levels,
                config.base_downline_levels,
                config.max_upline_levels,
                config.max_downline_levels,
            )?;
            Self::ensure_within_budget_cap(
                entity_id,
                config.upline_rate,
                config.downline_rate,
                config.max_upline_levels,
                config.max_downline_levels,
            )?;
            SingleLineConfigs::<T>::insert(entity_id, &config);
            Self::do_clamp_level_overrides(
                entity_id,
                config.max_upline_levels,
                config.max_downline_levels,
            );
            Self::record_change_log(entity_id, &config);
            Self::deposit_event(Event::SingleLineConfigUpdated {
                entity_id,
                upline_rate: config.upline_rate,
                downline_rate: config.downline_rate,
                base_upline_levels: config.base_upline_levels,
                base_downline_levels: config.base_downline_levels,
                max_upline_levels: config.max_upline_levels,
                max_downline_levels: config.max_downline_levels,
                reach_mode: config.reach_mode,
            });
            Ok(())
        }

        /// 核心清除逻辑：remove config → clear level overrides → event
        pub(crate) fn do_clear_config(entity_id: u64) {
            if SingleLineConfigs::<T>::contains_key(entity_id) {
                SingleLineConfigs::<T>::remove(entity_id);
                Self::do_clear_all_level_overrides(entity_id);
                Self::deposit_event(Event::SingleLineConfigCleared { entity_id });
            }
        }

        /// 核心层数覆盖写入：校验 level_id + config max → insert → event
        pub(crate) fn do_set_level_override(
            entity_id: u64,
            level_id: u8,
            upline_levels: u8,
            downline_levels: u8,
        ) -> DispatchResult {
            ensure!(
                upline_levels > 0 || downline_levels > 0,
                Error::<T>::InvalidLevels
            );
            if level_id > 0 {
                let count = T::MemberLevelProvider::custom_level_count(entity_id);
                ensure!(level_id <= count, Error::<T>::LevelIdNotFound);
            }
            let config =
                SingleLineConfigs::<T>::get(entity_id).ok_or(Error::<T>::ConfigNotFound)?;
            ensure!(
                upline_levels <= config.max_upline_levels
                    && downline_levels <= config.max_downline_levels,
                Error::<T>::LevelOverrideExceedsMax
            );
            SingleLineCustomLevelOverrides::<T>::insert(
                entity_id,
                level_id,
                LevelBasedLevels {
                    upline_levels,
                    downline_levels,
                },
            );
            Self::deposit_event(Event::LevelBasedLevelsUpdated {
                entity_id,
                level_id,
            });
            Ok(())
        }

        pub(crate) fn effective_base_levels(
            entity_id: u64,
            who: &T::AccountId,
            config: &SingleLineConfig<BalanceOf<T>>,
        ) -> (u8, u8) {
            let level_id = T::MemberLevelProvider::custom_level_id(entity_id, who);
            if let Some(o) = SingleLineCustomLevelOverrides::<T>::get(entity_id, level_id) {
                return (o.upline_levels, o.downline_levels);
            }
            (config.base_upline_levels, config.base_downline_levels)
        }

        pub(crate) fn add_to_single_line(entity_id: u64, account: &T::AccountId) -> DispatchResult {
            if SingleLineIndex::<T>::contains_key(entity_id, account) {
                return Ok(());
            }

            let seg_size = T::MaxSingleLineLength::get();
            let seg_count = SingleLineSegmentCount::<T>::get(entity_id);

            if seg_count > 0 {
                let last_seg_id = seg_count - 1;
                let mut seg = SingleLineSegments::<T>::get(entity_id, last_seg_id);
                if (seg.len() as u32) < seg_size {
                    let global_index = last_seg_id * seg_size + seg.len() as u32;
                    seg.try_push(account.clone())
                        .map_err(|_| DispatchError::Other("SegmentPushFailed"))?;
                    SingleLineSegments::<T>::insert(entity_id, last_seg_id, seg);
                    SingleLineIndex::<T>::insert(entity_id, account, global_index);
                    Self::deposit_event(Event::AddedToSingleLine {
                        entity_id,
                        account: account.clone(),
                        index: global_index,
                    });
                    return Ok(());
                }
            }

            let new_seg_id = seg_count;
            ensure!(
                new_seg_id < T::MaxSegmentCount::get(),
                Error::<T>::MaxSegmentCountReached
            );
            let global_index = new_seg_id * seg_size;
            let mut new_seg = BoundedVec::<T::AccountId, T::MaxSingleLineLength>::default();
            new_seg
                .try_push(account.clone())
                .map_err(|_| DispatchError::Other("SegmentPushFailed"))?;
            SingleLineSegments::<T>::insert(entity_id, new_seg_id, new_seg);
            SingleLineSegmentCount::<T>::insert(entity_id, new_seg_id + 1);
            SingleLineIndex::<T>::insert(entity_id, account, global_index);

            Self::deposit_event(Event::NewSegmentCreated {
                entity_id,
                segment_id: new_seg_id,
            });
            Self::deposit_event(Event::AddedToSingleLine {
                entity_id,
                account: account.clone(),
                index: global_index,
            });
            Ok(())
        }

        pub(crate) fn calc_extra_levels(threshold: BalanceOf<T>, total_earned: BalanceOf<T>) -> u8 {
            if threshold.is_zero() {
                return 0;
            }
            let threshold_u128: u128 = sp_runtime::SaturatedConversion::saturated_into(threshold);
            let earned_u128: u128 = sp_runtime::SaturatedConversion::saturated_into(total_earned);
            (earned_u128 / threshold_u128).min(255) as u8
        }

        fn is_member_skipped(entity_id: u64, account: &T::AccountId) -> bool {
            T::MemberProvider::is_banned(entity_id, account)
                || !T::MemberProvider::is_activated(entity_id, account)
                || !T::MemberProvider::is_member_active(entity_id, account)
                || RemovedMembers::<T>::get(entity_id, account)
        }

        pub fn process_upline<B>(
            entity_id: u64,
            buyer: &T::AccountId,
            order_amount: B,
            remaining: &mut B,
            config: &SingleLineConfig<BalanceOf<T>>,
            base_up: u8,
            outputs: &mut Vec<CommissionOutput<T::AccountId, B>>,
        ) where
            B: AtLeast32BitUnsigned + Copy,
        {
            if config.upline_rate == 0 {
                return;
            }
            let buyer_index = match SingleLineIndex::<T>::get(entity_id, buyer) {
                Some(idx) => idx,
                None => return,
            };
            if buyer_index == 0 {
                return;
            }

            let seg_size = T::MaxSingleLineLength::get();
            let buyer_stats = T::StatsProvider::get_member_stats(entity_id, buyer);
            let extra_levels =
                Self::calc_extra_levels(config.level_increment_threshold, buyer_stats.total_earned);
            let buyer_effective_up = base_up
                .saturating_add(extra_levels)
                .min(config.max_upline_levels) as u32;

            // Loop range depends on reach_mode:
            // BuyerOnly: only buyer's effective levels (early termination)
            // Bidirectional: capped by configured buyer-side window
            // BeneficiaryOnly: scan the full available upline chain toward root
            let loop_max = match config.reach_mode {
                ReachMode::BuyerOnly => buyer_effective_up,
                ReachMode::Bidirectional => config.max_upline_levels as u32,
                ReachMode::BeneficiaryOnly => buyer_index,
            };

            let mut cur_seg_id = buyer_index / seg_size;
            let mut cur_seg = SingleLineSegments::<T>::get(entity_id, cur_seg_id);

            for i in 1..=loop_max {
                if buyer_index < i {
                    break;
                }
                let target_index = buyer_index - i;
                let seg_id = target_index / seg_size;
                if seg_id != cur_seg_id {
                    cur_seg = SingleLineSegments::<T>::get(entity_id, seg_id);
                    cur_seg_id = seg_id;
                }
                let local_pos = (target_index % seg_size) as usize;
                let upline = match cur_seg.get(local_pos) {
                    Some(a) => a,
                    None => break,
                };

                if Self::is_member_skipped(entity_id, upline) {
                    continue;
                }

                // Reach check based on mode
                // 根据覆盖模式判定是否分佣
                match config.reach_mode {
                    ReachMode::BuyerOnly => {
                        // Already bounded by loop_max = buyer_effective_up, always in range.
                    }
                    ReachMode::BeneficiaryOnly => {
                        // Only beneficiary's reverse (downline) reach matters.
                        // 仅受益人的下线层数决定是否分佣。
                        let (_, ben_base_down) =
                            Self::effective_base_levels(entity_id, upline, config);
                        let ben_stats = T::StatsProvider::get_member_stats(entity_id, upline);
                        let ben_extra = Self::calc_extra_levels(
                            config.level_increment_threshold,
                            ben_stats.total_earned,
                        );
                        let ben_effective_down = ben_base_down.saturating_add(ben_extra) as u32;
                        if i > ben_effective_down {
                            continue;
                        }
                    }
                    ReachMode::Bidirectional => {
                        // Buyer covers → award; else check beneficiary reverse.
                        // 双向覆盖：买家能覆盖则直接分佣，否则检查受益人反向。
                        if i > buyer_effective_up {
                            let (_, ben_base_down) =
                                Self::effective_base_levels(entity_id, upline, config);
                            let ben_stats = T::StatsProvider::get_member_stats(entity_id, upline);
                            let ben_extra = Self::calc_extra_levels(
                                config.level_increment_threshold,
                                ben_stats.total_earned,
                            );
                            let ben_effective_down = ben_base_down
                                .saturating_add(ben_extra)
                                .min(config.max_downline_levels)
                                as u32;
                            if i > ben_effective_down {
                                continue;
                            }
                        }
                    }
                }

                let commission = order_amount.saturating_mul(B::from(config.upline_rate as u32))
                    / B::from(10000u32);
                let actual = commission.min(*remaining);

                if !actual.is_zero() {
                    *remaining = remaining.saturating_sub(actual);
                    outputs.push(CommissionOutput {
                        beneficiary: upline.clone(),
                        amount: actual,
                        commission_type: CommissionType::SingleLineUpline,
                        level: i as u16,
                    });
                }
            }
        }

        pub fn process_downline<B>(
            entity_id: u64,
            buyer: &T::AccountId,
            order_amount: B,
            remaining: &mut B,
            config: &SingleLineConfig<BalanceOf<T>>,
            base_down: u8,
            outputs: &mut Vec<CommissionOutput<T::AccountId, B>>,
        ) where
            B: AtLeast32BitUnsigned + Copy,
        {
            if config.downline_rate == 0 {
                return;
            }
            let buyer_index = match SingleLineIndex::<T>::get(entity_id, buyer) {
                Some(idx) => idx,
                None => return,
            };
            let total_len = Self::single_line_length(entity_id);
            if buyer_index >= total_len.saturating_sub(1) {
                return;
            }

            let seg_size = T::MaxSingleLineLength::get();
            let buyer_stats = T::StatsProvider::get_member_stats(entity_id, buyer);
            let extra_levels =
                Self::calc_extra_levels(config.level_increment_threshold, buyer_stats.total_earned);
            let buyer_effective_down = base_down
                .saturating_add(extra_levels)
                .min(config.max_downline_levels) as u32;

            // Loop range depends on reach_mode
            let loop_max = match config.reach_mode {
                ReachMode::BuyerOnly => buyer_effective_down,
                ReachMode::Bidirectional => config.max_downline_levels as u32,
                ReachMode::BeneficiaryOnly => total_len.saturating_sub(buyer_index + 1),
            };

            let mut cur_seg_id = buyer_index / seg_size;
            let mut cur_seg = SingleLineSegments::<T>::get(entity_id, cur_seg_id);

            for i in 1..=loop_max {
                let target_index = buyer_index.saturating_add(i);
                if target_index >= total_len {
                    break;
                }
                let seg_id = target_index / seg_size;
                if seg_id != cur_seg_id {
                    cur_seg = SingleLineSegments::<T>::get(entity_id, seg_id);
                    cur_seg_id = seg_id;
                }
                let local_pos = (target_index % seg_size) as usize;
                let downline = match cur_seg.get(local_pos) {
                    Some(a) => a,
                    None => break,
                };

                if Self::is_member_skipped(entity_id, downline) {
                    continue;
                }

                // Reach check based on mode
                // 根据覆盖模式判定是否分佣
                match config.reach_mode {
                    ReachMode::BuyerOnly => {
                        // Already bounded by loop_max = buyer_effective_down, always in range.
                    }
                    ReachMode::BeneficiaryOnly => {
                        // Only beneficiary's reverse (upline) reach matters.
                        // 仅受益人的上线层数决定是否分佣。
                        let (ben_base_up, _) =
                            Self::effective_base_levels(entity_id, downline, config);
                        let ben_stats = T::StatsProvider::get_member_stats(entity_id, downline);
                        let ben_extra = Self::calc_extra_levels(
                            config.level_increment_threshold,
                            ben_stats.total_earned,
                        );
                        let ben_effective_up = ben_base_up.saturating_add(ben_extra) as u32;
                        if i > ben_effective_up {
                            continue;
                        }
                    }
                    ReachMode::Bidirectional => {
                        // Buyer covers → award; else check beneficiary reverse.
                        // 双向覆盖：买家能覆盖则直接分佣，否则检查受益人反向。
                        if i > buyer_effective_down {
                            let (ben_base_up, _) =
                                Self::effective_base_levels(entity_id, downline, config);
                            let ben_stats = T::StatsProvider::get_member_stats(entity_id, downline);
                            let ben_extra = Self::calc_extra_levels(
                                config.level_increment_threshold,
                                ben_stats.total_earned,
                            );
                            let ben_effective_up = ben_base_up
                                .saturating_add(ben_extra)
                                .min(config.max_upline_levels)
                                as u32;
                            if i > ben_effective_up {
                                continue;
                            }
                        }
                    }
                }

                let commission = order_amount.saturating_mul(B::from(config.downline_rate as u32))
                    / B::from(10000u32);
                let actual = commission.min(*remaining);

                if !actual.is_zero() {
                    *remaining = remaining.saturating_sub(actual);
                    outputs.push(CommissionOutput {
                        beneficiary: downline.clone(),
                        amount: actual,
                        commission_type: CommissionType::SingleLineDownline,
                        level: i as u16,
                    });
                }
            }
        }

        pub(crate) fn do_reset_single_line_batched(
            entity_id: u64,
            max_segments: u32,
        ) -> (u32, bool) {
            let seg_count = SingleLineSegmentCount::<T>::get(entity_id);
            if seg_count == 0 {
                return (0, true);
            }

            let to_process = max_segments.min(seg_count);
            let mut total_removed = 0u32;

            for i in 0..to_process {
                let seg_id = seg_count - 1 - i;
                let seg = SingleLineSegments::<T>::take(entity_id, seg_id);
                for account in seg.iter() {
                    SingleLineIndex::<T>::remove(entity_id, account);
                }
                total_removed = total_removed.saturating_add(seg.len() as u32);
            }

            let remaining_segs = seg_count - to_process;
            if remaining_segs == 0 {
                SingleLineSegmentCount::<T>::remove(entity_id);
                // B7 修复: 全部段清理完毕后，清除 RemovedMembers 脏数据
                // 防止重置后重新入链的成员被 is_member_skipped 错误跳过
                let _ = RemovedMembers::<T>::clear_prefix(entity_id, u32::MAX, None);
                // 清除分佣历史和汇总
                let _ = MemberSingleLinePayouts::<T>::clear_prefix(entity_id, u32::MAX, None);
                let _ = MemberSingleLineStats::<T>::clear_prefix(entity_id, u32::MAX, None);
                (total_removed, true)
            } else {
                SingleLineSegmentCount::<T>::insert(entity_id, remaining_segs);
                (total_removed, false)
            }
        }

        pub(crate) fn do_clear_all_level_overrides(entity_id: u64) {
            let has_overrides = SingleLineCustomLevelOverrides::<T>::iter_prefix(entity_id)
                .next()
                .is_some();
            let _ = SingleLineCustomLevelOverrides::<T>::clear_prefix(entity_id, u32::MAX, None);
            if has_overrides {
                Self::deposit_event(Event::AllLevelOverridesCleared { entity_id });
            }
        }

        /// config 的 max_levels 降低后，clamp 所有已有覆盖至新上限。
        /// clamp 后 (0,0) 的条目直接删除。
        pub(crate) fn do_clamp_level_overrides(entity_id: u64, max_up: u8, max_down: u8) {
            let overrides: alloc::vec::Vec<_> =
                SingleLineCustomLevelOverrides::<T>::iter_prefix(entity_id).collect();
            for (level_id, lvl) in overrides {
                if lvl.upline_levels <= max_up && lvl.downline_levels <= max_down {
                    continue;
                }
                let clamped_up = lvl.upline_levels.min(max_up);
                let clamped_down = lvl.downline_levels.min(max_down);
                if clamped_up == 0 && clamped_down == 0 {
                    SingleLineCustomLevelOverrides::<T>::remove(entity_id, level_id);
                    Self::deposit_event(Event::LevelBasedLevelsRemoved {
                        entity_id,
                        level_id,
                    });
                } else {
                    SingleLineCustomLevelOverrides::<T>::insert(
                        entity_id,
                        level_id,
                        LevelBasedLevels {
                            upline_levels: clamped_up,
                            downline_levels: clamped_down,
                        },
                    );
                    Self::deposit_event(Event::LevelBasedLevelsUpdated {
                        entity_id,
                        level_id,
                    });
                }
            }
        }

        pub(crate) fn record_change_log(entity_id: u64, config: &SingleLineConfig<BalanceOf<T>>) {
            let max = T::MaxConfigChangeLogs::get();
            if max == 0 {
                return;
            }
            let count = ConfigChangeLogCount::<T>::get(entity_id);
            let slot = count % max;
            ConfigChangeLogs::<T>::insert(
                entity_id,
                slot,
                ConfigChangeLogEntry {
                    block_number: <frame_system::Pallet<T>>::block_number(),
                    upline_rate: config.upline_rate,
                    downline_rate: config.downline_rate,
                    base_upline_levels: config.base_upline_levels,
                    base_downline_levels: config.base_downline_levels,
                    max_upline_levels: config.max_upline_levels,
                    max_downline_levels: config.max_downline_levels,
                    level_increment_threshold: config.level_increment_threshold,
                    reach_mode: config.reach_mode,
                },
            );
            ConfigChangeLogCount::<T>::insert(entity_id, count.saturating_add(1));
        }

        /// 记录分佣到会员历史 (FIFO + 汇总)
        pub(crate) fn record_payout(
            entity_id: u64,
            recipient: &T::AccountId,
            buyer: &T::AccountId,
            order_id: u64,
            amount: BalanceOf<T>,
            direction: PayoutDirection,
            level_distance: u16,
        ) {
            let block_number: u64 = <frame_system::Pallet<T>>::block_number()
                .try_into()
                .unwrap_or(0u64);

            let record = SingleLinePayoutRecord {
                order_id,
                buyer: buyer.clone(),
                amount,
                direction: direction.clone(),
                level_distance,
                block_number,
            };

            // 1. 追加到 BoundedVec (FIFO: 满了就移除最旧一条)
            MemberSingleLinePayouts::<T>::mutate(entity_id, recipient, |records| {
                if records.is_full() {
                    records.remove(0);
                }
                records.try_push(record).ok();
            });

            // 2. 更新汇总
            MemberSingleLineStats::<T>::mutate(entity_id, recipient, |summary| {
                match direction {
                    PayoutDirection::Upline => {
                        summary.total_earned_as_upline =
                            summary.total_earned_as_upline.saturating_add(amount);
                    }
                    PayoutDirection::Downline => {
                        summary.total_earned_as_downline =
                            summary.total_earned_as_downline.saturating_add(amount);
                    }
                }
                summary.total_payout_count = summary.total_payout_count.saturating_add(1);
                summary.last_payout_block = block_number;
            });
        }

        // ====================================================================
        // Query helpers
        // ====================================================================

        pub fn single_line_length(entity_id: u64) -> u32 {
            let seg_size = T::MaxSingleLineLength::get();
            let seg_count = SingleLineSegmentCount::<T>::get(entity_id);
            if seg_count == 0 {
                return 0;
            }
            let last_seg = SingleLineSegments::<T>::get(entity_id, seg_count - 1);
            (seg_count - 1) * seg_size + last_seg.len() as u32
        }

        pub fn single_line_remaining_capacity(entity_id: u64) -> u32 {
            let seg_size = T::MaxSingleLineLength::get();
            let seg_count = SingleLineSegmentCount::<T>::get(entity_id);
            if seg_count == 0 {
                return seg_size;
            }
            let last_seg = SingleLineSegments::<T>::get(entity_id, seg_count - 1);
            seg_size.saturating_sub(last_seg.len() as u32)
        }

        pub fn user_position(entity_id: u64, account: &T::AccountId) -> Option<u32> {
            SingleLineIndex::<T>::get(entity_id, account)
        }

        pub fn user_effective_levels(entity_id: u64, account: &T::AccountId) -> Option<(u8, u8)> {
            let config = SingleLineConfigs::<T>::get(entity_id)?;
            let (base_up, base_down) = Self::effective_base_levels(entity_id, account, &config);
            let stats = T::StatsProvider::get_member_stats(entity_id, account);
            let extra =
                Self::calc_extra_levels(config.level_increment_threshold, stats.total_earned);
            Some((
                base_up.saturating_add(extra).min(config.max_upline_levels),
                base_down
                    .saturating_add(extra)
                    .min(config.max_downline_levels),
            ))
        }

        pub fn is_single_line_enabled(entity_id: u64) -> bool {
            SingleLineEnabled::<T>::get(entity_id)
        }

        fn account_at_position(entity_id: u64, index: u32) -> Option<T::AccountId> {
            let seg_size = T::MaxSingleLineLength::get();
            let seg_count = SingleLineSegmentCount::<T>::get(entity_id);
            if seg_count == 0 {
                return None;
            }
            let seg_id = index / seg_size;
            if seg_id >= seg_count {
                return None;
            }
            let seg = SingleLineSegments::<T>::get(entity_id, seg_id);
            let local_pos = (index % seg_size) as usize;
            seg.get(local_pos).cloned()
        }

        pub fn single_line_member_position_info(
            entity_id: u64,
            account: &T::AccountId,
        ) -> Option<runtime_api::SingleLineMemberPositionInfo<T::AccountId>> {
            let position = Self::user_position(entity_id, account)?;
            let queue_length = Self::single_line_length(entity_id);
            let (upline_levels, downline_levels) = Self::user_effective_levels(entity_id, account)?;
            let previous_account = if position > 0 {
                Self::account_at_position(entity_id, position - 1)
            } else {
                None
            };
            let next_account = if position + 1 < queue_length {
                Self::account_at_position(entity_id, position + 1)
            } else {
                None
            };

            Some(runtime_api::SingleLineMemberPositionInfo {
                position,
                queue_length,
                upline_levels,
                downline_levels,
                previous_account,
                next_account,
            })
        }

        pub fn single_line_member_payouts(
            entity_id: u64,
            account: &T::AccountId,
        ) -> Vec<runtime_api::SingleLinePayoutRecordView<T::AccountId, BalanceOf<T>>> {
            MemberSingleLinePayouts::<T>::get(entity_id, account)
                .into_iter()
                .map(|record| runtime_api::SingleLinePayoutRecordView {
                    order_id: record.order_id,
                    buyer: record.buyer,
                    amount: record.amount,
                    direction: match record.direction {
                        PayoutDirection::Upline => 0,
                        PayoutDirection::Downline => 1,
                    },
                    level_distance: record.level_distance,
                    block_number: record.block_number,
                })
                .collect()
        }

        pub fn single_line_member_view(
            entity_id: u64,
            account: &T::AccountId,
        ) -> Option<runtime_api::SingleLineMemberView<T::AccountId, BalanceOf<T>>> {
            let position_info = Self::single_line_member_position_info(entity_id, account);
            let summary = MemberSingleLineStats::<T>::get(entity_id, account);
            let recent_payouts = Self::single_line_member_payouts(entity_id, account);
            let has_data = position_info.is_some()
                || summary.total_payout_count > 0
                || !recent_payouts.is_empty();

            if !has_data {
                return None;
            }

            Some(runtime_api::SingleLineMemberView {
                position_info,
                is_enabled: Self::is_single_line_enabled(entity_id),
                summary: runtime_api::SingleLineMemberSummaryView {
                    total_earned_as_upline: summary.total_earned_as_upline,
                    total_earned_as_downline: summary.total_earned_as_downline,
                    total_payout_count: summary.total_payout_count,
                    last_payout_block: summary.last_payout_block,
                },
                recent_payouts,
            })
        }

        pub fn single_line_overview(entity_id: u64) -> runtime_api::SingleLineOverview {
            let stats = EntitySingleLineStats::<T>::get(entity_id);
            runtime_api::SingleLineOverview {
                is_enabled: Self::is_single_line_enabled(entity_id),
                queue_length: Self::single_line_length(entity_id),
                remaining_capacity_in_tail_segment: Self::single_line_remaining_capacity(entity_id),
                segment_count: SingleLineSegmentCount::<T>::get(entity_id),
                stats: runtime_api::SingleLineEntityStatsView {
                    total_orders: stats.total_orders,
                    total_upline_payouts: stats.total_upline_payouts,
                    total_downline_payouts: stats.total_downline_payouts,
                },
            }
        }

        pub fn preview_single_line_commission(
            entity_id: u64,
            buyer: &T::AccountId,
            order_amount: BalanceOf<T>,
        ) -> Vec<CommissionOutput<T::AccountId, BalanceOf<T>>> {
            let config = match SingleLineConfigs::<T>::get(entity_id) {
                Some(c) => c,
                None => return Vec::new(),
            };
            let mut remaining = order_amount;
            let mut outputs = Vec::new();
            let (base_up, base_down) = Self::effective_base_levels(entity_id, buyer, &config);
            Self::process_upline(
                entity_id,
                buyer,
                order_amount,
                &mut remaining,
                &config,
                base_up,
                &mut outputs,
            );
            Self::process_downline(
                entity_id,
                buyer,
                order_amount,
                &mut remaining,
                &config,
                base_down,
                &mut outputs,
            );
            outputs
        }
    }
}

// ============================================================================
// Unified calculate logic
// ============================================================================

impl<T: pallet::Config> pallet::Pallet<T> {
    fn do_calculate<B>(
        entity_id: u64,
        buyer: &T::AccountId,
        order_amount: B,
        remaining: B,
        enabled_modes: pallet_commission_common::CommissionModes,
        _is_first_order: bool,
    ) -> (
        alloc::vec::Vec<pallet_commission_common::CommissionOutput<T::AccountId, B>>,
        B,
    )
    where
        B: sp_runtime::traits::AtLeast32BitUnsigned + Copy,
    {
        use pallet_commission_common::CommissionModes;

        if !T::EntityProvider::is_entity_active(entity_id) {
            return (alloc::vec::Vec::new(), remaining);
        }
        if !pallet::SingleLineEnabled::<T>::get(entity_id) {
            return (alloc::vec::Vec::new(), remaining);
        }
        let config = match pallet::SingleLineConfigs::<T>::get(entity_id) {
            Some(c) => c,
            None => return (alloc::vec::Vec::new(), remaining),
        };

        let has_upline = enabled_modes.contains(CommissionModes::SINGLE_LINE_UPLINE);
        let has_downline = enabled_modes.contains(CommissionModes::SINGLE_LINE_DOWNLINE);
        if !has_upline && !has_downline {
            return (alloc::vec::Vec::new(), remaining);
        }

        let buyer_in_chain = pallet::SingleLineIndex::<T>::contains_key(entity_id, buyer);
        let mut remaining = remaining;
        let mut outputs = alloc::vec::Vec::new();

        // 首单时先加入公排链，使 buyer 获得链上位置后再计算佣金
        if !buyer_in_chain {
            if Self::add_to_single_line(entity_id, buyer).is_err() {
                Self::deposit_event(pallet::Event::SingleLineJoinFailed {
                    entity_id,
                    account: buyer.clone(),
                });
            }
        }

        let (base_up, base_down) = Self::effective_base_levels(entity_id, buyer, &config);

        if has_upline {
            Self::process_upline(
                entity_id,
                buyer,
                order_amount,
                &mut remaining,
                &config,
                base_up,
                &mut outputs,
            );
        }
        if has_downline {
            Self::process_downline(
                entity_id,
                buyer,
                order_amount,
                &mut remaining,
                &config,
                base_down,
                &mut outputs,
            );
        }

        if !outputs.is_empty() {
            pallet::EntitySingleLineStats::<T>::mutate(entity_id, |stats| {
                stats.total_orders = stats.total_orders.saturating_add(1);
                for o in &outputs {
                    match o.commission_type {
                        pallet_commission_common::CommissionType::SingleLineUpline => {
                            stats.total_upline_payouts =
                                stats.total_upline_payouts.saturating_add(1);
                        }
                        pallet_commission_common::CommissionType::SingleLineDownline => {
                            stats.total_downline_payouts =
                                stats.total_downline_payouts.saturating_add(1);
                        }
                        _ => {}
                    }
                }
            });
        }

        (outputs, remaining)
    }
}

// ============================================================================
// CommissionPlugin (NEX)
// ============================================================================

impl<T: pallet::Config>
    pallet_commission_common::CommissionPlugin<T::AccountId, pallet::BalanceOf<T>>
    for pallet::Pallet<T>
{
    fn calculate(
        entity_id: u64,
        buyer: &T::AccountId,
        order_amount: pallet::BalanceOf<T>,
        remaining: pallet::BalanceOf<T>,
        enabled_modes: pallet_commission_common::CommissionModes,
        is_first_order: bool,
        _buyer_order_count: u32,
        order_id: u64,
    ) -> (
        alloc::vec::Vec<
            pallet_commission_common::CommissionOutput<T::AccountId, pallet::BalanceOf<T>>,
        >,
        pallet::BalanceOf<T>,
    ) {
        let (outputs, rem) = Self::do_calculate(
            entity_id,
            buyer,
            order_amount,
            remaining,
            enabled_modes,
            is_first_order,
        );

        // 记录每笔分佣到会员历史 (仅 NEX)
        for o in &outputs {
            let direction = match o.commission_type {
                pallet_commission_common::CommissionType::SingleLineUpline => {
                    pallet::PayoutDirection::Upline
                }
                pallet_commission_common::CommissionType::SingleLineDownline => {
                    pallet::PayoutDirection::Downline
                }
                _ => continue,
            };
            Self::record_payout(
                entity_id,
                &o.beneficiary,
                buyer,
                order_id,
                o.amount,
                direction,
                o.level,
            );
        }

        (outputs, rem)
    }
}

// ============================================================================
// TokenCommissionPlugin
// ============================================================================

impl<T: pallet::Config, TB> pallet_commission_common::TokenCommissionPlugin<T::AccountId, TB>
    for pallet::Pallet<T>
where
    TB: sp_runtime::traits::AtLeast32BitUnsigned + Copy + Default + core::fmt::Debug,
{
    fn calculate_token(
        entity_id: u64,
        buyer: &T::AccountId,
        order_amount: TB,
        remaining: TB,
        enabled_modes: pallet_commission_common::CommissionModes,
        is_first_order: bool,
        _buyer_order_count: u32,
        _order_id: u64,
    ) -> (
        alloc::vec::Vec<pallet_commission_common::CommissionOutput<T::AccountId, TB>>,
        TB,
    ) {
        Self::do_calculate(
            entity_id,
            buyer,
            order_amount,
            remaining,
            enabled_modes,
            is_first_order,
        )
    }
}

// ============================================================================
// SingleLinePlanWriter (delegates to internal helpers)
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::SingleLinePlanWriter for pallet::Pallet<T> {
    fn set_single_line_config(
        entity_id: u64,
        upline_rate: u16,
        downline_rate: u16,
        base_upline_levels: u8,
        base_downline_levels: u8,
        level_increment_threshold: u128,
        max_upline_levels: u8,
        max_downline_levels: u8,
        reach_mode: u8,
    ) -> Result<(), sp_runtime::DispatchError> {
        let threshold: pallet::BalanceOf<T> =
            sp_runtime::SaturatedConversion::saturated_into(level_increment_threshold);
        let mode = match reach_mode {
            1 => pallet::ReachMode::BuyerOnly,
            2 => pallet::ReachMode::BeneficiaryOnly,
            _ => pallet::ReachMode::Bidirectional,
        };
        Self::do_set_config(
            entity_id,
            pallet::SingleLineConfig {
                upline_rate,
                downline_rate,
                base_upline_levels,
                base_downline_levels,
                level_increment_threshold: threshold,
                max_upline_levels,
                max_downline_levels,
                reach_mode: mode,
            },
        )
    }

    fn clear_config(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        Self::do_clear_config(entity_id);
        Ok(())
    }

    fn set_level_based_levels(
        entity_id: u64,
        level_id: u8,
        upline_levels: u8,
        downline_levels: u8,
    ) -> Result<(), sp_runtime::DispatchError> {
        Self::do_set_level_override(entity_id, level_id, upline_levels, downline_levels)
    }

    fn clear_level_overrides(
        entity_id: u64,
        level_id: u8,
    ) -> Result<(), sp_runtime::DispatchError> {
        if pallet::SingleLineCustomLevelOverrides::<T>::contains_key(entity_id, level_id) {
            pallet::SingleLineCustomLevelOverrides::<T>::remove(entity_id, level_id);
            pallet::Pallet::<T>::deposit_event(pallet::Event::LevelBasedLevelsRemoved {
                entity_id,
                level_id,
            });
        }
        Ok(())
    }
}

// ============================================================================
// SingleLineQueryProvider 实现
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::SingleLineQueryProvider<T::AccountId>
    for pallet::Pallet<T>
{
    fn position(entity_id: u64, account: &T::AccountId) -> Option<u32> {
        Self::user_position(entity_id, account)
    }

    fn effective_levels(entity_id: u64, account: &T::AccountId) -> Option<(u8, u8)> {
        Self::user_effective_levels(entity_id, account)
    }

    fn is_enabled(entity_id: u64) -> bool {
        Self::is_single_line_enabled(entity_id)
    }

    fn queue_length(entity_id: u64) -> u32 {
        Self::single_line_length(entity_id)
    }

    fn chain_depth(entity_id: u64) -> u16 {
        pallet::SingleLineConfigs::<T>::get(entity_id)
            .map(|c| c.max_upline_levels.max(c.max_downline_levels) as u16)
            .unwrap_or(0)
    }
}

// ============================================================================
// SingleLineGovernancePort 实现
// ============================================================================

impl<T: pallet::Config> pallet_entity_common::SingleLineGovernancePort for pallet::Pallet<T> {
    fn governance_set_single_line_config(
        entity_id: u64,
        upline_rate: u16,
        downline_rate: u16,
        base_upline_levels: u8,
        base_downline_levels: u8,
        max_upline_levels: u8,
        max_downline_levels: u8,
        reach_mode: u8,
    ) -> Result<(), sp_runtime::DispatchError> {
        // 保留已有 config 的 level_increment_threshold（governance 接口不控制此字段）
        let existing = pallet::SingleLineConfigs::<T>::get(entity_id);
        let existing_threshold = existing
            .as_ref()
            .map(|c| c.level_increment_threshold)
            .unwrap_or_else(sp_runtime::traits::Zero::zero);
        let mode = match reach_mode {
            1 => pallet::ReachMode::BuyerOnly,
            2 => pallet::ReachMode::BeneficiaryOnly,
            _ => pallet::ReachMode::Bidirectional,
        };

        Self::do_set_config(
            entity_id,
            pallet::SingleLineConfig {
                upline_rate,
                downline_rate,
                base_upline_levels,
                base_downline_levels,
                level_increment_threshold: existing_threshold,
                max_upline_levels,
                max_downline_levels,
                reach_mode: mode,
            },
        )
    }

    fn governance_pause_single_line(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        if !pallet::SingleLineEnabled::<T>::get(entity_id) {
            return Ok(()); // 已暂停，幂等操作
        }
        pallet::SingleLineEnabled::<T>::insert(entity_id, false);
        Self::deposit_event(pallet::Event::SingleLinePaused { entity_id });
        Ok(())
    }

    fn governance_resume_single_line(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        if pallet::SingleLineEnabled::<T>::get(entity_id) {
            return Ok(()); // 已启用，幂等操作
        }
        pallet::SingleLineEnabled::<T>::insert(entity_id, true);
        Self::deposit_event(pallet::Event::SingleLineResumed { entity_id });
        Ok(())
    }
}

// ============================================================================
// OnLevelRemoved — 等级删除时清理对应的层数覆盖
// ============================================================================

impl<T: pallet::Config> pallet_entity_common::OnLevelRemoved for pallet::Pallet<T> {
    fn on_level_removed(entity_id: u64, level_id: u8) {
        if pallet::SingleLineCustomLevelOverrides::<T>::contains_key(entity_id, level_id) {
            pallet::SingleLineCustomLevelOverrides::<T>::remove(entity_id, level_id);
            Self::deposit_event(pallet::Event::LevelBasedLevelsRemoved {
                entity_id,
                level_id,
            });
        }
    }
}
