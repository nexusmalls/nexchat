//! # Commission Pool Reward Plugin (pallet-commission-pool-reward)
//!
//! 沉淀池奖励插件：周期性等额分配模型（Periodic Equal-Share Claim）。
//!
//! ## 核心逻辑
//!
//! 当 `POOL_REWARD` 模式启用后，未分配佣金自动沉淀入 Entity 级沉淀池（Phase 1.5，由 core 管理）。
//! 每隔 `round_duration` 区块为一轮。首个 claim 触发新轮快照，记录池余额和各等级会员数。
//! 用户在轮次窗口内签名调用 `claim_pool_reward` 领取属于自己等级的份额。
//! 未领取的金额留在池中，下一轮自然包含。
//!
//! ## Entity Owner 不可提取
//!
//! 沉淀池资金完全由算法驱动分配，Entity Owner 无法直接提取。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use pallet_entity_common::PricingProvider as NexPricingProvider;
use sp_runtime::traits::SaturatedConversion;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod runtime_api;
pub mod weights;
pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, Get},
    };
    use frame_system::pallet_prelude::*;
    use pallet_commission_common::{
        MemberProvider, ParticipationGuard, PoolBalanceProvider, TokenPoolBalanceProvider,
        TokenTransferProvider as TokenTransferProviderT,
    };
    use pallet_entity_common::{
        AdminPermission, EntityProvider, PoolRewardCapBehavior, PoolRewardLevelClaimRule,
    };
    use sp_runtime::traits::{Saturating, UniqueSaturatedInto, Zero};

    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    pub type TokenBalanceOf<T> = <T as Config>::TokenBalance;

    /// F12: 池奖励领取回调（将 claim 记录写入 commission-core 的统一佣金体系）
    pub trait PoolRewardClaimCallback<AccountId, Balance, TokenBalance> {
        fn on_pool_reward_claimed(
            entity_id: u64,
            account: &AccountId,
            nex_amount: Balance,
            token_amount: TokenBalance,
            round_id: u64,
            level_id: u8,
        );
    }

    impl<AccountId, Balance, TokenBalance> PoolRewardClaimCallback<AccountId, Balance, TokenBalance>
        for ()
    {
        fn on_pool_reward_claimed(
            _: u64,
            _: &AccountId,
            _: Balance,
            _: TokenBalance,
            _: u64,
            _: u8,
        ) {
        }
    }

    // ========================================================================
    // Data structs
    // ========================================================================

    pub type CapBehavior = PoolRewardCapBehavior;

    pub type LevelClaimRule = PoolRewardLevelClaimRule;

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
    pub struct PoolRewardConfig<MaxLevels: Get<u32>, BlockNumber> {
        pub level_rules: BoundedVec<(u8, LevelClaimRule), MaxLevels>,
        pub round_duration: BlockNumber,
        pub token_pool_enabled: bool,
    }

    pub type PoolRewardConfigOf<T> =
        PoolRewardConfig<<T as Config>::MaxPoolRewardLevels, BlockNumberFor<T>>;

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
    pub struct LevelQuotaSnapshot {
        pub level_id: u8,
        pub member_count: u32,
        pub claimed_count: u32,
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
    #[scale_info(skip_type_params(MaxLevels))]
    pub struct RoundInfo<MaxLevels: Get<u32>, Balance, TokenBalance, BlockNumber> {
        pub round_id: u64,
        pub start_block: BlockNumber,
        pub pool_snapshot: Balance,
        pub nex_usdt_rate_snapshot: Option<u64>,
        pub eligible_count: u32,
        pub per_member_reward: Balance,
        pub claimed_count: u32,
        pub level_quotas: BoundedVec<LevelQuotaSnapshot, MaxLevels>,
        pub token_pool_snapshot: Option<TokenBalance>,
        pub token_per_member_reward: Option<TokenBalance>,
        pub token_claimed_count: u32,
        pub token_level_quotas: Option<BoundedVec<LevelQuotaSnapshot, MaxLevels>>,
    }

    pub type RoundInfoOf<T> = RoundInfo<
        <T as Config>::MaxPoolRewardLevels,
        BalanceOf<T>,
        TokenBalanceOf<T>,
        BlockNumberFor<T>,
    >;

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
    pub struct ClaimRecord<Balance, TokenBalance, BlockNumber> {
        pub round_id: u64,
        pub amount: Balance,
        pub level_id: u8,
        pub claimed_at: BlockNumber,
        pub token_amount: TokenBalance,
    }

    pub type ClaimRecordOf<T> = ClaimRecord<BalanceOf<T>, TokenBalanceOf<T>, BlockNumberFor<T>>;

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
    pub struct CompletedRoundSummary<MaxLevels: Get<u32>, Balance, TokenBalance, BlockNumber> {
        pub round_id: u64,
        pub start_block: BlockNumber,
        pub end_block: BlockNumber,
        pub pool_snapshot: Balance,
        pub nex_usdt_rate_snapshot: Option<u64>,
        pub eligible_count: u32,
        pub per_member_reward: Balance,
        pub claimed_count: u32,
        pub level_quotas: BoundedVec<LevelQuotaSnapshot, MaxLevels>,
        pub token_pool_snapshot: Option<TokenBalance>,
        pub token_per_member_reward: Option<TokenBalance>,
        pub token_claimed_count: u32,
        pub token_level_quotas: Option<BoundedVec<LevelQuotaSnapshot, MaxLevels>>,
        /// 本轮资金来源汇总
        pub funding_summary: RoundFundingSummary,
    }

    pub type CompletedRoundSummaryOf<T> = CompletedRoundSummary<
        <T as Config>::MaxPoolRewardLevels,
        BalanceOf<T>,
        TokenBalanceOf<T>,
        BlockNumberFor<T>,
    >;

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
    pub struct DistributionStats<Balance: Default, TokenBalance: Default> {
        pub total_nex_distributed: Balance,
        pub total_token_distributed: TokenBalance,
        pub total_rounds_completed: u64,
        pub total_claims: u64,
    }

    pub type DistributionStatsOf<T> = DistributionStats<BalanceOf<T>, TokenBalanceOf<T>>;

    /// 沉淀池单笔资金来源记录
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
    pub struct PoolFundingRecord {
        pub source: pallet_commission_common::FundingSource,
        pub nex_amount: u128,
        pub token_amount: u128,
        pub order_id: u64,
        pub block_number: u32,
    }

    /// 单轮资金来源汇总
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
    pub struct RoundFundingSummary {
        /// NEX 订单佣金剩余累计
        pub nex_commission_remainder: u128,
        /// Token 平台费留存累计
        pub token_platform_fee_retention: u128,
        /// Token 佣金剩余累计
        pub token_commission_remainder: u128,
        /// NEX 取消退回累计
        pub nex_cancel_return: u128,
        /// 总入账笔数
        pub total_funding_count: u32,
    }

    /// 待生效的配置变更
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
    pub struct PendingConfigChange<MaxLevels: Get<u32>, BlockNumber> {
        pub level_rules: BoundedVec<(u8, LevelClaimRule), MaxLevels>,
        pub round_duration: BlockNumber,
        pub apply_after: BlockNumber,
    }

    pub type PendingConfigChangeOf<T> =
        PendingConfigChange<<T as Config>::MaxPoolRewardLevels, BlockNumberFor<T>>;

    // ========================================================================
    // Pallet Config
    // ========================================================================

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        type Currency: Currency<Self::AccountId>;
        type MemberProvider: MemberProvider<Self::AccountId>;
        type EntityProvider: EntityProvider<Self::AccountId>;
        type PoolBalanceProvider: PoolBalanceProvider<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxPoolRewardLevels: Get<u32>;

        /// 每用户领取记录滚动窗口大小。
        /// 生产环境建议 >= 20，过小会导致用户丢失较早的领取证据。
        #[pallet::constant]
        type MaxClaimHistory: Get<u32>;

        type TokenBalance: codec::FullCodec
            + codec::MaxEncodedLen
            + TypeInfo
            + Copy
            + Default
            + core::fmt::Debug
            + sp_runtime::traits::AtLeast32BitUnsigned
            + From<u32>
            + Into<u128>;

        type TokenPoolBalanceProvider: TokenPoolBalanceProvider<TokenBalanceOf<Self>>;
        type TokenTransferProvider: TokenTransferProviderT<Self::AccountId, TokenBalanceOf<Self>>;
        type ExchangeRateProvider: NexPricingProvider;
        type ParticipationGuard: ParticipationGuard<Self::AccountId>;
        type WeightInfo: WeightInfo;

        #[pallet::constant]
        type MinRoundDuration: Get<BlockNumberFor<Self>>;

        #[pallet::constant]
        type MaxRoundHistory: Get<u32>;

        type ClaimCallback: PoolRewardClaimCallback<
            Self::AccountId,
            BalanceOf<Self>,
            TokenBalanceOf<Self>,
        >;

        /// 配置变更延迟（区块数）— 计划配置变更生效前的最小等待时间
        #[pallet::constant]
        type ConfigChangeDelay: Get<BlockNumberFor<Self>>;

        /// 活跃实体列表上限（on_initialize 扫描用）
        #[pallet::constant]
        type MaxActivePoolRewardEntities: Get<u32>;

        /// 每个区块最多自动轮转的实体数
        #[pallet::constant]
        type MaxAutoRotatePerBlock: Get<u32>;

        /// 每个实体的资金来源明细记录上限（FIFO）。
        /// 高频 Entity 建议 >= 50，过小会导致跨轮明细快速被淘汰。
        #[pallet::constant]
        type MaxFundingRecords: Get<u32>;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// 自动轮转：扫描活跃实体，过期则创建新轮次
        fn on_initialize(now: BlockNumberFor<T>) -> Weight {
            let current = Pallet::<T>::on_chain_storage_version();
            let db = T::DbWeight::get();
            let migration_weight = if current < 4 {
                let mut migrated = 0u64;
                for (entity_id, account, claimed_nex) in MemberCumulativeClaimedNex::<T>::iter() {
                    if MemberCumulativeClaimed::<T>::contains_key(entity_id, &account) {
                        continue;
                    }
                    let claimed_usdt = Pallet::<T>::convert_nex_to_usdt(
                        claimed_nex,
                        T::ExchangeRateProvider::get_nex_usdt_price(),
                    );
                    MemberCumulativeClaimed::<T>::insert(entity_id, &account, claimed_usdt);
                    migrated = migrated.saturating_add(1);
                }
                StorageVersion::new(4).put::<Pallet<T>>();
                db.reads_writes(migrated.saturating_add(2), migrated.saturating_add(1))
            } else {
                Weight::zero()
            };

            // 1 read: GlobalPoolRewardPaused
            let mut weight = migration_weight.saturating_add(db.reads(1));
            if GlobalPoolRewardPaused::<T>::get() {
                return weight;
            }

            // 1 read: ActivePoolRewardEntities + 1 read: RotationCursor
            weight = weight.saturating_add(db.reads(2));
            let active = ActivePoolRewardEntities::<T>::get();
            let len = active.len() as u32;
            if len == 0 {
                return weight;
            }

            let max_check = T::MaxAutoRotatePerBlock::get().min(len);
            let mut cursor = RotationCursor::<T>::get();
            if cursor >= len {
                cursor = 0;
            }

            // 收集需要移除的不活跃实体（循环结束后统一处理，避免游标计算错乱）
            let mut to_remove = alloc::vec::Vec::new();

            for i in 0..max_check {
                let idx = ((cursor + i) % len) as usize;
                let entity_id = active[idx];

                // 每个实体检查: PoolRewardPaused + PoolRewardConfigs + CurrentRound + entity_locked + entity_active + pool_balance
                weight = weight.saturating_add(db.reads(6));

                if PoolRewardPaused::<T>::get(entity_id) {
                    continue;
                }
                if !T::EntityProvider::is_entity_active(entity_id) {
                    to_remove.push(entity_id);
                    continue;
                }
                if T::EntityProvider::is_entity_locked(entity_id) {
                    continue;
                }

                let config = match PoolRewardConfigs::<T>::get(entity_id) {
                    Some(c) => c,
                    None => continue,
                };

                // 空池跳过：避免产生无意义的零奖励轮次
                let pool_balance = T::PoolBalanceProvider::pool_balance(entity_id);
                if pool_balance.is_zero() {
                    continue;
                }

                // 有当前轮次 → 检查是否过期；无轮次 → 自动创建首轮
                let should_rotate = match CurrentRound::<T>::get(entity_id) {
                    Some(r) => {
                        let end_block = r.start_block.saturating_add(config.round_duration);
                        now >= end_block
                    }
                    None => true, // 首轮自动创建
                };

                if !should_rotate {
                    continue;
                }

                // 创建新轮次: ~6 reads + ~4 writes
                weight = weight.saturating_add(db.reads_writes(6, 4));
                match Self::create_new_round(entity_id, &config, now) {
                    Ok(new_round) => {
                        Self::deposit_event(Event::RoundAutoRotated {
                            entity_id,
                            round_id: new_round.round_id,
                        });
                    }
                    Err(_) => {
                        frame_support::defensive!("pool-reward: auto-rotate failed for entity");
                    }
                }
            }

            // 统一移除不活跃实体
            if !to_remove.is_empty() {
                ActivePoolRewardEntities::<T>::mutate(|list| {
                    list.retain(|id| !to_remove.contains(id));
                });
                weight = weight.saturating_add(db.reads_writes(1, 1));
            }

            // 按移除后的实际长度推进游标
            let actual_len = ActivePoolRewardEntities::<T>::decode_len().unwrap_or(0) as u32;
            if actual_len == 0 {
                RotationCursor::<T>::kill();
            } else {
                let new_cursor = (cursor + max_check) % actual_len;
                RotationCursor::<T>::put(new_cursor);
            }
            weight = weight.saturating_add(db.reads_writes(1, 1));

            weight
        }

        fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let base_weight = T::DbWeight::get().reads_writes(1, 1);
            let mut consumed = Weight::zero();
            let mut processed = 0u32;
            const MAX_PER_BLOCK: u32 = 5;

            let mut iter = TokenPoolDeficit::<T>::iter();
            while processed < MAX_PER_BLOCK {
                if consumed
                    .saturating_add(base_weight)
                    .any_gt(remaining_weight)
                {
                    break;
                }
                let Some((entity_id, deficit)) = iter.next() else {
                    break;
                };
                if deficit.is_zero() {
                    TokenPoolDeficit::<T>::remove(entity_id);
                    consumed = consumed.saturating_add(base_weight);
                    processed += 1;
                    continue;
                }
                if T::TokenPoolBalanceProvider::deduct_token_pool(entity_id, deficit).is_ok() {
                    TokenPoolDeficit::<T>::remove(entity_id);
                    Self::deposit_event(Event::TokenPoolDeficitCorrected {
                        entity_id,
                        amount: deficit,
                    });
                }
                consumed = consumed.saturating_add(base_weight);
                processed += 1;
            }
            consumed
        }

        #[cfg(feature = "std")]
        fn integrity_test() {
            assert!(
                T::MaxPoolRewardLevels::get() >= 1,
                "MaxPoolRewardLevels must be >= 1"
            );
            assert!(
                T::MaxClaimHistory::get() >= 1,
                "MaxClaimHistory must be >= 1"
            );
            assert!(
                T::MinRoundDuration::get() > BlockNumberFor::<T>::zero(),
                "MinRoundDuration must be > 0"
            );
            assert!(
                T::MaxRoundHistory::get() >= 1,
                "MaxRoundHistory must be >= 1"
            );
            assert!(
                T::ConfigChangeDelay::get() > BlockNumberFor::<T>::zero(),
                "ConfigChangeDelay must be > 0"
            );
            assert!(
                T::MaxActivePoolRewardEntities::get() >= 1,
                "MaxActivePoolRewardEntities must be >= 1"
            );
            assert!(
                T::MaxAutoRotatePerBlock::get() >= 1,
                "MaxAutoRotatePerBlock must be >= 1"
            );
            assert!(
                T::MaxFundingRecords::get() >= 1,
                "MaxFundingRecords must be >= 1"
            );
        }
    }

    // ========================================================================
    // Storage
    // ========================================================================

    #[pallet::storage]
    #[pallet::getter(fn pool_reward_config)]
    pub type PoolRewardConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PoolRewardConfigOf<T>>;

    #[pallet::storage]
    #[pallet::getter(fn current_round)]
    pub type CurrentRound<T: Config> = StorageMap<_, Blake2_128Concat, u64, RoundInfoOf<T>>;

    #[pallet::storage]
    pub type LastRoundId<T: Config> = StorageMap<_, Blake2_128Concat, u64, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn last_claimed_round)]
    pub type LastClaimedRound<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u64, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn claim_records)]
    pub type ClaimRecords<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<ClaimRecordOf<T>, T::MaxClaimHistory>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn pool_reward_paused)]
    pub type PoolRewardPaused<T: Config> = StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn global_pool_reward_paused)]
    pub type GlobalPoolRewardPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn round_history)]
    pub type RoundHistory<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<CompletedRoundSummaryOf<T>, T::MaxRoundHistory>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn distribution_stats)]
    pub type DistributionStatistics<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, DistributionStatsOf<T>, ValueQuery>;

    #[pallet::storage]
    pub type MemberCumulativeClaimedNex<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        BalanceOf<T>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type MemberCumulativeClaimed<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Blake2_128Concat,
        T::AccountId,
        u128,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type CappedMemberCount<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u64, Blake2_128Concat, u8, u32, ValueQuery>;

    /// 待生效的配置变更
    #[pallet::storage]
    #[pallet::getter(fn pending_pool_reward_config)]
    pub type PendingPoolRewardConfig<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, PendingConfigChangeOf<T>>;

    /// Token 池账本差额：回滚失败时累计的已转出但未扣减的 token 数量
    #[pallet::storage]
    #[pallet::getter(fn token_pool_deficit)]
    pub type TokenPoolDeficit<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, TokenBalanceOf<T>, ValueQuery>;

    /// 活跃实体列表（已配置且未暂停，on_initialize 自动轮转扫描用）
    #[pallet::storage]
    pub type ActivePoolRewardEntities<T: Config> =
        StorageValue<_, BoundedVec<u64, T::MaxActivePoolRewardEntities>, ValueQuery>;

    /// 轮询游标（round-robin 公平扫描）
    #[pallet::storage]
    pub type RotationCursor<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// 当前轮次资金来源汇总累加器（每轮开始时重置）
    #[pallet::storage]
    pub type CurrentRoundFunding<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, RoundFundingSummary, ValueQuery>;

    /// 资金来源明细记录（FIFO，跨轮持续，全局视角）
    #[pallet::storage]
    pub type PoolFundingRecords<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<PoolFundingRecord, T::MaxFundingRecords>,
        ValueQuery,
    >;

    // ========================================================================
    // Events / Errors
    // ========================================================================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoolRewardConfigUpdated {
            entity_id: u64,
        },
        /// R1: 合并原 NewRoundStarted + NewRoundDetails 为单一事件
        NewRoundStarted {
            entity_id: u64,
            round_id: u64,
            pool_snapshot: BalanceOf<T>,
            token_pool_snapshot: Option<TokenBalanceOf<T>>,
            level_snapshots: alloc::vec::Vec<(u8, u32, BalanceOf<T>)>,
            token_level_snapshots: Option<alloc::vec::Vec<(u8, u32, TokenBalanceOf<T>)>>,
        },
        PoolRewardClaimed {
            entity_id: u64,
            account: T::AccountId,
            amount: BalanceOf<T>,
            token_amount: TokenBalanceOf<T>,
            round_id: u64,
            level_id: u8,
        },
        MemberCapReached {
            entity_id: u64,
            account: T::AccountId,
            level_id: u8,
            cumulative_claimed_usdt: u128,
            cap_usdt: u128,
        },
        TokenPoolEnabledUpdated {
            entity_id: u64,
            enabled: bool,
        },
        TokenTransferRollbackFailed {
            entity_id: u64,
            account: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// P2-12: Token 初始转账失败（区分于回滚失败）
        TokenClaimTransferFailed {
            entity_id: u64,
            account: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        /// V-2: Token 池扣减失败但转账已成功回滚（用户未收到 Token，无资金损失）
        TokenClaimDeductPoolFailed {
            entity_id: u64,
            account: T::AccountId,
            amount: TokenBalanceOf<T>,
        },
        PoolRewardConfigCleared {
            entity_id: u64,
        },
        /// R3: 去除 Event 后缀
        PoolRewardPaused {
            entity_id: u64,
        },
        PoolRewardResumed {
            entity_id: u64,
        },
        GlobalPoolRewardPaused,
        GlobalPoolRewardResumed,
        RoundArchived {
            entity_id: u64,
            round_id: u64,
        },
        /// P0: 配置变更已计划
        PoolRewardConfigScheduled {
            entity_id: u64,
            apply_after: BlockNumberFor<T>,
        },
        PendingPoolRewardConfigApplied {
            entity_id: u64,
        },
        PendingPoolRewardConfigCancelled {
            entity_id: u64,
        },
        /// Token 池差额已被 Root 修正
        TokenPoolDeficitCorrected {
            entity_id: u64,
            amount: TokenBalanceOf<T>,
        },
        /// force_clear 用户记录未完全清理，需再次调用
        ClearIncomplete {
            entity_id: u64,
            remaining: u32,
        },
        /// on_initialize 自动轮转成功
        RoundAutoRotated {
            entity_id: u64,
            round_id: u64,
        },
        /// 沉淀池收到资金
        PoolFunded {
            entity_id: u64,
            source: pallet_commission_common::FundingSource,
            nex_amount: u128,
            token_amount: u128,
            order_id: u64,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        InvalidRatio,
        RatioSumMismatch,
        DuplicateLevelId,
        InvalidRoundDuration,
        NotMember,
        LevelNotConfigured,
        LevelNotEligible,
        AlreadyClaimed,
        LevelQuotaExhausted,
        NothingToClaim,
        InsufficientPool,
        ConfigNotFound,
        EntityNotActive,
        LevelNotInSnapshot,
        RoundIdOverflow,
        ParticipationRequirementNotMet,
        NotAuthorized,
        EntityLocked,
        PoolRewardIsPaused,
        PoolRewardNotPaused,
        RoundDurationTooShort,
        GlobalPaused,
        GlobalNotPaused,
        /// 已存在待生效的配置变更
        PendingConfigExists,
        /// 无待生效的配置变更
        NoPendingConfig,
        /// 配置变更延迟未到期
        ConfigChangeDelayNotMet,
        /// 累计领取已达上限
        CumulativeCapReached,
        /// Token 池无差额可修正
        NoDeficit,
        /// 活跃实体列表已满
        ActiveEntitiesFull,
        /// 缺少 NEX/USDT 价格
        PriceUnavailable,
        /// NEX/USDT 价格不可靠
        PriceUnreliable,
    }

    // ========================================================================
    // Extrinsics (17 个)
    // ========================================================================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 设置沉淀池奖励配置（Entity Owner / Admin(COMMISSION_MANAGE)）
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_pool_reward_config())]
        pub fn set_pool_reward_config(
            origin: OriginFor<T>,
            entity_id: u64,
            level_rules: BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels>,
            round_duration: BlockNumberFor<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            Self::do_set_pool_reward_config(entity_id, level_rules, round_duration)
        }

        /// 用户领取沉淀池奖励（NEX + Token 双池统一入口）
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::claim_pool_reward())]
        pub fn claim_pool_reward(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                !GlobalPoolRewardPaused::<T>::get(),
                Error::<T>::GlobalPaused
            );
            ensure!(
                !PoolRewardPaused::<T>::get(entity_id),
                Error::<T>::PoolRewardIsPaused
            );

            ensure!(
                T::MemberProvider::is_member(entity_id, &who),
                Error::<T>::NotMember
            );
            ensure!(
                !T::MemberProvider::is_banned(entity_id, &who),
                Error::<T>::NotMember
            );
            ensure!(
                T::MemberProvider::is_member_active(entity_id, &who),
                Error::<T>::NotMember
            );
            ensure!(
                T::ParticipationGuard::can_participate(entity_id, &who),
                Error::<T>::ParticipationRequirementNotMet
            );

            let config =
                PoolRewardConfigs::<T>::get(entity_id).ok_or(Error::<T>::ConfigNotFound)?;

            let user_level = T::MemberProvider::custom_level_id(entity_id, &who);
            let (effective_level, level_rule) = Self::get_exact_level_rule(&config, user_level)
                .ok_or(Error::<T>::LevelNotEligible)?;

            let cumulative_claimed_usdt = MemberCumulativeClaimed::<T>::get(entity_id, &who);
            let now = <frame_system::Pallet<T>>::block_number();
            let mut round = Self::ensure_current_round(entity_id, &config, now)?;
            let member_cap_usdt =
                Self::calculate_member_cap(entity_id, &who, effective_level, level_rule);
            ensure!(
                cumulative_claimed_usdt < member_cap_usdt,
                Error::<T>::CumulativeCapReached
            );

            let last_round = LastClaimedRound::<T>::get(entity_id, &who);
            ensure!(last_round < round.round_id, Error::<T>::AlreadyClaimed);

            let nex_snap_idx = round
                .level_quotas
                .iter()
                .position(|s| s.level_id == effective_level)
                .ok_or(Error::<T>::LevelNotInSnapshot)?;

            let reward = {
                let snapshot = &round.level_quotas[nex_snap_idx];
                // V-1 修复: fallback 用户也必须受配额限制，防止侵占其他等级的池资金
                ensure!(
                    snapshot.claimed_count < snapshot.member_count,
                    Error::<T>::LevelQuotaExhausted
                );
                let r = round.per_member_reward;
                ensure!(!r.is_zero(), Error::<T>::NothingToClaim);
                r
            };

            // Use the round's snapshotted rate if available; fall back to the current live rate
            // so that rounds created before the rate-snapshot feature was added remain claimable.
            // 优先使用轮次快照汇率；若快照缺失则 fallback 到当前实时汇率，
            // 确保在引入汇率快照功能前创建的轮次仍可正常领取。
            let rate_for_cap = round
                .nex_usdt_rate_snapshot
                .map(Ok)
                .unwrap_or_else(|| Self::get_reliable_nex_usdt_rate())?;

            let actual_reward = reward.min(Self::convert_usdt_to_nex(
                member_cap_usdt.saturating_sub(cumulative_claimed_usdt),
                rate_for_cap,
            ));
            ensure!(!actual_reward.is_zero(), Error::<T>::CumulativeCapReached);

            let pool = T::PoolBalanceProvider::pool_balance(entity_id);
            ensure!(pool >= actual_reward, Error::<T>::InsufficientPool);
            T::PoolBalanceProvider::deduct_pool(entity_id, actual_reward)?;
            let entity_account = T::EntityProvider::entity_account(entity_id);
            T::Currency::transfer(
                &entity_account,
                &who,
                actual_reward,
                ExistenceRequirement::KeepAlive,
            )?;

            // Token 采用 transfer-first 顺序：best-effort 路径不走 `?`，
            // 不会触发 Substrate 事务回滚，且无 add_back 接口回滚记账扣减
            let mut token_reward = TokenBalanceOf::<T>::default();
            if let Some(ref mut token_quotas) = round.token_level_quotas {
                if let Some(token_snap) = token_quotas
                    .iter_mut()
                    .find(|s| s.level_id == effective_level)
                {
                    // V-1 修复: fallback 用户也受 Token 配额限制
                    let token_quota_ok = token_snap.claimed_count < token_snap.member_count;
                    if token_quota_ok {
                        let tr = round.token_per_member_reward.unwrap_or_default();
                        if !tr.is_zero() {
                            let token_pool =
                                T::TokenPoolBalanceProvider::token_pool_balance(entity_id);
                            if token_pool >= tr {
                                match T::TokenTransferProvider::token_transfer(
                                    entity_id,
                                    &entity_account,
                                    &who,
                                    tr,
                                ) {
                                    Ok(()) => {
                                        if T::TokenPoolBalanceProvider::deduct_token_pool(
                                            entity_id, tr,
                                        )
                                        .is_ok()
                                        {
                                            token_snap.claimed_count =
                                                token_snap.claimed_count.saturating_add(1);
                                            round.token_claimed_count =
                                                round.token_claimed_count.saturating_add(1);
                                            token_reward = tr;
                                        } else {
                                            // P1-2 修复: 扣减失败 → 尝试回滚转账
                                            if T::TokenTransferProvider::token_transfer(
                                                entity_id,
                                                &who,
                                                &entity_account,
                                                tr,
                                            )
                                            .is_err()
                                            {
                                                // 回滚也失败 → Token 已转出但账本未扣减
                                                // 必须记录分配以保持 claimed_count 一致
                                                token_snap.claimed_count =
                                                    token_snap.claimed_count.saturating_add(1);
                                                round.token_claimed_count =
                                                    round.token_claimed_count.saturating_add(1);
                                                token_reward = tr;
                                                TokenPoolDeficit::<T>::mutate(entity_id, |d| {
                                                    *d = d.saturating_add(tr);
                                                });
                                                Self::deposit_event(
                                                    Event::TokenTransferRollbackFailed {
                                                        entity_id,
                                                        account: who.clone(),
                                                        amount: tr,
                                                    },
                                                );
                                            } else {
                                                // V-2 修复: 回滚成功 → 通知外部系统
                                                Self::deposit_event(
                                                    Event::TokenClaimDeductPoolFailed {
                                                        entity_id,
                                                        account: who.clone(),
                                                        amount: tr,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        // P2-12: Token 转账失败事件（区分于回滚失败）
                                        Self::deposit_event(Event::TokenClaimTransferFailed {
                                            entity_id,
                                            account: who.clone(),
                                            amount: tr,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let round_id = round.round_id;
            let claim_rate = round
                .nex_usdt_rate_snapshot
                .ok_or(Error::<T>::PriceUnavailable)?;
            let claimed_usdt_delta = Self::convert_nex_to_usdt(actual_reward, claim_rate);
            round.level_quotas[nex_snap_idx].claimed_count = round.level_quotas[nex_snap_idx]
                .claimed_count
                .saturating_add(1);
            round.claimed_count = round.claimed_count.saturating_add(1);
            CurrentRound::<T>::insert(entity_id, round);
            LastClaimedRound::<T>::insert(entity_id, &who, round_id);

            MemberCumulativeClaimed::<T>::mutate(entity_id, &who, |claimed| {
                *claimed = claimed.saturating_add(claimed_usdt_delta);
            });
            MemberCumulativeClaimedNex::<T>::mutate(entity_id, &who, |claimed| {
                *claimed = claimed.saturating_add(actual_reward);
            });

            let new_cumulative = MemberCumulativeClaimed::<T>::get(entity_id, &who);
            if cumulative_claimed_usdt < member_cap_usdt && new_cumulative >= member_cap_usdt {
                CappedMemberCount::<T>::mutate(entity_id, effective_level, |count| {
                    *count = count.saturating_add(1);
                });
                Self::deposit_event(Event::MemberCapReached {
                    entity_id,
                    account: who.clone(),
                    level_id: effective_level,
                    cumulative_claimed_usdt: new_cumulative,
                    cap_usdt: member_cap_usdt,
                });
            }

            ClaimRecords::<T>::mutate(entity_id, &who, |history| {
                let record = ClaimRecord {
                    round_id,
                    amount: actual_reward,
                    level_id: effective_level,
                    claimed_at: now,
                    token_amount: token_reward,
                };
                if history.is_full() {
                    history.remove(0);
                }
                let _ = history.try_push(record);
            });

            DistributionStatistics::<T>::mutate(entity_id, |stats| {
                stats.total_nex_distributed =
                    stats.total_nex_distributed.saturating_add(actual_reward);
                stats.total_token_distributed =
                    stats.total_token_distributed.saturating_add(token_reward);
                stats.total_claims = stats.total_claims.saturating_add(1);
            });

            T::ClaimCallback::on_pool_reward_claimed(
                entity_id,
                &who,
                actual_reward,
                token_reward,
                round_id,
                effective_level,
            );

            Self::deposit_event(Event::PoolRewardClaimed {
                entity_id,
                account: who,
                amount: actual_reward,
                token_amount: token_reward,
                round_id,
                level_id: effective_level,
            });

            Ok(())
        }

        /// 启用/禁用 Entity Token 池分配（Entity Owner / Admin）
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_token_pool_enabled())]
        pub fn set_token_pool_enabled(
            origin: OriginFor<T>,
            entity_id: u64,
            enabled: bool,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            Self::do_set_token_pool_enabled(entity_id, enabled)
        }

        /// [Root] 强制设置沉淀池奖励配置
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_pool_reward_config())]
        pub fn force_set_pool_reward_config(
            origin: OriginFor<T>,
            entity_id: u64,
            level_rules: BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels>,
            round_duration: BlockNumberFor<T>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_set_pool_reward_config(entity_id, level_rules, round_duration)
        }

        /// [Root] 强制启用/禁用 Token 池分配
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::set_token_pool_enabled())]
        pub fn force_set_token_pool_enabled(
            origin: OriginFor<T>,
            entity_id: u64,
            enabled: bool,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_set_token_pool_enabled(entity_id, enabled)
        }

        /// 清除沉淀池奖励配置（Owner/Admin，部分清理）
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::clear_pool_reward_config())]
        pub fn clear_pool_reward_config(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            Self::do_clear_pool_reward_config(entity_id);
            Ok(())
        }

        /// [Root] 强制清除 — 完整清理全部存储（含用户记录）
        /// `max_users`: 每次最多清理的用户记录数（控制单次权重）。
        /// 如果用户记录未完全清理，发出 `ClearIncomplete` 事件，需再次调用。
        /// 可在 config 已删除的情况下继续调用以完成用户记录清理。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::force_clear_pool_reward_config())]
        pub fn force_clear_pool_reward_config(
            origin: OriginFor<T>,
            entity_id: u64,
            max_users: u32,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_full_clear_pool_reward(entity_id, max_users);
            Ok(())
        }

        /// 暂停沉淀池奖励分配（Entity Owner / Admin）
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::pause_pool_reward())]
        pub fn pause_pool_reward(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            ensure!(
                !PoolRewardPaused::<T>::get(entity_id),
                Error::<T>::PoolRewardIsPaused
            );

            PoolRewardPaused::<T>::insert(entity_id, true);
            Self::remove_from_active_entities(entity_id);
            Self::deposit_event(Event::PoolRewardPaused { entity_id });
            Ok(())
        }

        /// 恢复沉淀池奖励分配（Entity Owner / Admin）
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::resume_pool_reward())]
        pub fn resume_pool_reward(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            ensure!(
                PoolRewardPaused::<T>::get(entity_id),
                Error::<T>::PoolRewardNotPaused
            );

            PoolRewardPaused::<T>::remove(entity_id);
            let _ = Self::add_to_active_entities(entity_id);
            Self::deposit_event(Event::PoolRewardResumed { entity_id });
            Ok(())
        }

        /// 全局紧急暂停/恢复（Root only）
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::set_global_pool_reward_paused())]
        pub fn set_global_pool_reward_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            ensure_root(origin)?;
            if paused {
                ensure!(
                    !GlobalPoolRewardPaused::<T>::get(),
                    Error::<T>::GlobalPaused
                );
                GlobalPoolRewardPaused::<T>::put(true);
                Self::deposit_event(Event::GlobalPoolRewardPaused);
            } else {
                ensure!(
                    GlobalPoolRewardPaused::<T>::get(),
                    Error::<T>::GlobalNotPaused
                );
                GlobalPoolRewardPaused::<T>::kill();
                Self::deposit_event(Event::GlobalPoolRewardResumed);
            }
            Ok(())
        }

        /// P0-2: [Root] 强制暂停 per-entity 池奖励（绕过 EntityLocked）
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::force_pause_pool_reward())]
        pub fn force_pause_pool_reward(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            ensure!(
                !PoolRewardPaused::<T>::get(entity_id),
                Error::<T>::PoolRewardIsPaused
            );

            PoolRewardPaused::<T>::insert(entity_id, true);
            Self::remove_from_active_entities(entity_id);
            Self::deposit_event(Event::PoolRewardPaused { entity_id });
            Ok(())
        }

        /// P0-2: [Root] 强制恢复 per-entity 池奖励（绕过 EntityLocked）
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::force_resume_pool_reward())]
        pub fn force_resume_pool_reward(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            ensure!(
                PoolRewardPaused::<T>::get(entity_id),
                Error::<T>::PoolRewardNotPaused
            );

            PoolRewardPaused::<T>::remove(entity_id);
            let _ = Self::add_to_active_entities(entity_id);
            Self::deposit_event(Event::PoolRewardResumed { entity_id });
            Ok(())
        }

        /// P0-1: 计划配置变更（Owner/Admin）— 延迟 ConfigChangeDelay 区块后生效
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::schedule_pool_reward_config_change())]
        pub fn schedule_pool_reward_config_change(
            origin: OriginFor<T>,
            entity_id: u64,
            level_rules: BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels>,
            round_duration: BlockNumberFor<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PoolRewardConfigs::<T>::contains_key(entity_id),
                Error::<T>::ConfigNotFound
            );
            ensure!(
                !PendingPoolRewardConfig::<T>::contains_key(entity_id),
                Error::<T>::PendingConfigExists
            );

            ensure!(
                round_duration > BlockNumberFor::<T>::zero(),
                Error::<T>::InvalidRoundDuration
            );
            ensure!(
                round_duration >= T::MinRoundDuration::get(),
                Error::<T>::RoundDurationTooShort
            );
            Self::validate_level_rules(&level_rules)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let apply_after = now.saturating_add(T::ConfigChangeDelay::get());

            PendingPoolRewardConfig::<T>::insert(
                entity_id,
                PendingConfigChange {
                    level_rules,
                    round_duration,
                    apply_after,
                },
            );

            Self::deposit_event(Event::PoolRewardConfigScheduled {
                entity_id,
                apply_after,
            });
            Ok(())
        }

        /// P0-1: 应用待生效的配置变更
        /// P2-7 修复: 限制为 Owner/Admin（防止任意用户提前失效活跃轮次）
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::apply_pending_pool_reward_config())]
        pub fn apply_pending_pool_reward_config(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );

            let pending =
                PendingPoolRewardConfig::<T>::get(entity_id).ok_or(Error::<T>::NoPendingConfig)?;

            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(
                now >= pending.apply_after,
                Error::<T>::ConfigChangeDelayNotMet
            );

            Self::do_set_pool_reward_config(
                entity_id,
                pending.level_rules,
                pending.round_duration,
            )?;
            Self::deposit_event(Event::PendingPoolRewardConfigApplied { entity_id });
            Ok(())
        }

        /// P0-1: 取消待生效的配置变更（Owner/Admin）
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::cancel_pending_pool_reward_config())]
        pub fn cancel_pending_pool_reward_config(
            origin: OriginFor<T>,
            entity_id: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_owner_or_admin(&who, entity_id)?;
            ensure!(
                !T::EntityProvider::is_entity_locked(entity_id),
                Error::<T>::EntityLocked
            );
            ensure!(
                PendingPoolRewardConfig::<T>::contains_key(entity_id),
                Error::<T>::NoPendingConfig
            );

            PendingPoolRewardConfig::<T>::remove(entity_id);
            Self::deposit_event(Event::PendingPoolRewardConfigCancelled { entity_id });
            Ok(())
        }

        /// [Root] 修正 Token 池账本差额（回滚失败导致的已转出未扣减部分）
        /// 同时从 Token 池余额中扣减对应金额，使链上余额与实际一致。
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::correct_token_pool_deficit())]
        pub fn correct_token_pool_deficit(origin: OriginFor<T>, entity_id: u64) -> DispatchResult {
            ensure_root(origin)?;
            let deficit = TokenPoolDeficit::<T>::take(entity_id);
            ensure!(!deficit.is_zero(), Error::<T>::NoDeficit);
            if let Err(e) = T::TokenPoolBalanceProvider::deduct_token_pool(entity_id, deficit) {
                frame_support::defensive!(
                    "pool-reward: deduct_token_pool failed during deficit correction"
                );
                TokenPoolDeficit::<T>::insert(entity_id, deficit);
                return Err(e);
            }
            Self::deposit_event(Event::TokenPoolDeficitCorrected {
                entity_id,
                amount: deficit,
            });
            Ok(())
        }
    }

    // ========================================================================
    // Internal logic
    // ========================================================================

    impl<T: Config> Pallet<T> {
        fn ensure_owner_or_admin(who: &T::AccountId, entity_id: u64) -> DispatchResult {
            ensure!(
                T::EntityProvider::is_entity_active(entity_id),
                Error::<T>::EntityNotActive
            );
            let owner =
                T::EntityProvider::entity_owner(entity_id).ok_or(Error::<T>::EntityNotActive)?;
            ensure!(
                *who == owner
                    || T::EntityProvider::is_entity_admin(
                        entity_id,
                        who,
                        AdminPermission::COMMISSION_MANAGE
                    ),
                Error::<T>::NotAuthorized
            );
            Ok(())
        }

        pub(crate) fn invalidate_current_round(entity_id: u64) {
            if let Some(round) = CurrentRound::<T>::get(entity_id) {
                LastRoundId::<T>::insert(entity_id, round.round_id);
            }
            CurrentRound::<T>::remove(entity_id);
        }

        pub(crate) fn validate_level_rules(rules: &[(u8, LevelClaimRule)]) -> DispatchResult {
            ensure!(!rules.is_empty(), Error::<T>::InvalidRatio);
            let mut seen_ids = alloc::collections::BTreeSet::new();
            for (level_id, rule) in rules.iter() {
                ensure!(seen_ids.insert(*level_id), Error::<T>::DuplicateLevelId);
                ensure!(
                    rule.base_cap_percent > 0 && rule.base_cap_percent <= 10000,
                    Error::<T>::InvalidRatio
                );
                if let CapBehavior::UnlockByTeam {
                    direct_per_unlock,
                    team_per_unlock,
                    unlock_percent,
                } = rule.cap_behavior
                {
                    ensure!(
                        direct_per_unlock > 0 && team_per_unlock > 0,
                        Error::<T>::InvalidRatio
                    );
                    ensure!(
                        unlock_percent > 0 && unlock_percent <= 10000,
                        Error::<T>::InvalidRatio
                    );
                }
            }
            Ok(())
        }

        pub(crate) fn calculate_member_cap(
            entity_id: u64,
            account: &T::AccountId,
            _level_id: u8,
            rule: &LevelClaimRule,
        ) -> u128 {
            let stats = T::MemberProvider::get_member_stats(entity_id, account);
            let cap_basis_spent_usdt = stats.spend.upgrade_eligible_spent;
            let (_quota_usdt, base_cap_usdt, unlock_amount_usdt, unlock_count) =
                Self::compute_cap_values_usdt(
                    cap_basis_spent_usdt,
                    rule,
                    stats.direct_referrals,
                    stats.team_size,
                );
            base_cap_usdt.saturating_add(unlock_amount_usdt.saturating_mul(unlock_count.into()))
        }

        fn build_cap_behavior_info(rule: &LevelClaimRule) -> crate::runtime_api::CapBehaviorInfo {
            match rule.cap_behavior {
                CapBehavior::Fixed => crate::runtime_api::CapBehaviorInfo::Fixed,
                CapBehavior::UnlockByTeam {
                    direct_per_unlock,
                    team_per_unlock,
                    unlock_percent,
                } => crate::runtime_api::CapBehaviorInfo::UnlockByTeam {
                    direct_per_unlock,
                    team_per_unlock,
                    unlock_percent,
                    baseline_direct: rule.baseline_direct,
                    baseline_team: rule.baseline_team,
                },
            }
        }

        fn get_reliable_nex_usdt_rate() -> Result<u64, DispatchError> {
            let rate = T::ExchangeRateProvider::get_nex_usdt_price();
            ensure!(rate > 0, Error::<T>::PriceUnavailable);
            ensure!(
                !T::ExchangeRateProvider::is_price_stale(),
                Error::<T>::PriceUnreliable
            );
            Ok(rate)
        }

        fn convert_usdt_to_nex(total_spent_usdt: u128, nex_usdt_rate: u64) -> BalanceOf<T> {
            if nex_usdt_rate == 0 {
                return BalanceOf::<T>::zero();
            }
            let quota_nex_u128 =
                total_spent_usdt.saturating_mul(1_000_000_000_000u128) / (nex_usdt_rate as u128);
            quota_nex_u128.unique_saturated_into()
        }

        fn convert_nex_to_usdt(amount_nex: BalanceOf<T>, nex_usdt_rate: u64) -> u128 {
            if nex_usdt_rate == 0 {
                return 0;
            }
            let amount_nex_u128: u128 = amount_nex.saturated_into();
            amount_nex_u128.saturating_mul(nex_usdt_rate as u128) / 1_000_000_000_000u128
        }

        pub(crate) fn compute_cap_values_usdt(
            total_spent_usdt: u128,
            rule: &LevelClaimRule,
            direct_count: u32,
            team_count: u32,
        ) -> (u128, u128, u128, u32) {
            let base_cap_usdt =
                total_spent_usdt.saturating_mul(rule.base_cap_percent.into()) / 10000u128;
            match rule.cap_behavior {
                CapBehavior::Fixed => (total_spent_usdt, base_cap_usdt, 0, 0),
                CapBehavior::UnlockByTeam {
                    direct_per_unlock,
                    team_per_unlock,
                    unlock_percent,
                } => {
                    let excess_direct = direct_count.saturating_sub(rule.baseline_direct);
                    let excess_team = team_count.saturating_sub(rule.baseline_team);
                    let unlock_count = if excess_direct == 0 || excess_team == 0 {
                        0u32
                    } else {
                        (excess_direct / direct_per_unlock).min(excess_team / team_per_unlock)
                    };
                    let unlock_amount_usdt =
                        total_spent_usdt.saturating_mul(unlock_percent.into()) / 10000u128;
                    (
                        total_spent_usdt,
                        base_cap_usdt,
                        unlock_amount_usdt,
                        unlock_count,
                    )
                }
            }
        }

        fn resolve_cap_rate_for_entity(entity_id: u64) -> Option<u64> {
            CurrentRound::<T>::get(entity_id)
                .and_then(|round| round.nex_usdt_rate_snapshot)
                .or_else(|| {
                    let rate = T::ExchangeRateProvider::get_nex_usdt_price();
                    if rate > 0 {
                        Some(rate)
                    } else {
                        None
                    }
                })
        }

        pub(crate) fn get_exact_level_rule<'a>(
            config: &'a PoolRewardConfigOf<T>,
            user_level: u8,
        ) -> Option<(u8, &'a LevelClaimRule)> {
            config
                .level_rules
                .iter()
                .find(|(id, _)| *id == user_level)
                .map(|(id, rule)| (*id, rule))
        }

        fn build_level_rule_summary(
            level_id: u8,
            rule: &LevelClaimRule,
        ) -> crate::runtime_api::LevelRuleSummaryInfo {
            crate::runtime_api::LevelRuleSummaryInfo {
                level_id,
                base_cap_percent: rule.base_cap_percent,
                cap_behavior: Self::build_cap_behavior_info(rule),
            }
        }

        fn build_admin_level_rules(
            entity_id: u64,
            config: &PoolRewardConfigOf<T>,
        ) -> alloc::vec::Vec<crate::runtime_api::AdminLevelRuleInfo> {
            let level_counts = Self::build_strict_level_counts(entity_id, config);
            config
                .level_rules
                .iter()
                .map(|(level_id, rule)| {
                    let member_count = level_counts
                        .iter()
                        .find(|(id, _)| id == level_id)
                        .map(|(_, count)| *count)
                        .unwrap_or(0);
                    crate::runtime_api::AdminLevelRuleInfo {
                        level_id: *level_id,
                        base_cap_percent: rule.base_cap_percent,
                        cap_behavior: Self::build_cap_behavior_info(rule),
                        member_count,
                        capped_member_count: CappedMemberCount::<T>::get(entity_id, *level_id),
                    }
                })
                .collect()
        }

        fn compute_member_cap_info(
            entity_id: u64,
            who: &T::AccountId,
            rule: &LevelClaimRule,
        ) -> (
            crate::runtime_api::MemberStatsInfo,
            crate::runtime_api::MemberCapInfo<BalanceOf<T>>,
        ) {
            let stats = T::MemberProvider::get_member_stats(entity_id, who);
            let cap_basis_spent_usdt = stats.spend.upgrade_eligible_spent;
            let cumulative_claimed_usdt = MemberCumulativeClaimed::<T>::get(entity_id, who);
            let nex_usdt_rate = Self::resolve_cap_rate_for_entity(entity_id);
            let (quota_usdt, base_cap_usdt, unlock_amount_per_step_usdt, unlock_count) =
                Self::compute_cap_values_usdt(
                    cap_basis_spent_usdt,
                    rule,
                    stats.direct_referrals,
                    stats.team_size,
                );
            let current_cap_usdt = base_cap_usdt
                .saturating_add(unlock_amount_per_step_usdt.saturating_mul(unlock_count.into()));
            let quota_nex = nex_usdt_rate
                .map(|rate| Self::convert_usdt_to_nex(quota_usdt, rate))
                .unwrap_or_else(BalanceOf::<T>::zero);

            let (unlock_percent, next_direct_gap, next_team_gap, next_unlock_increase_usdt) =
                match rule.cap_behavior {
                    CapBehavior::Fixed => (None, None, None, None),
                    CapBehavior::UnlockByTeam {
                        direct_per_unlock,
                        team_per_unlock,
                        unlock_percent,
                    } => {
                        let excess_direct =
                            stats.direct_referrals.saturating_sub(rule.baseline_direct);
                        let excess_team = stats.team_size.saturating_sub(rule.baseline_team);
                        let next_unlock = unlock_count.saturating_add(1);
                        let target_direct = direct_per_unlock.saturating_mul(next_unlock);
                        let target_team = team_per_unlock.saturating_mul(next_unlock);
                        (
                            Some(unlock_percent),
                            Some(target_direct.saturating_sub(excess_direct)),
                            Some(target_team.saturating_sub(excess_team)),
                            Some(unlock_amount_per_step_usdt),
                        )
                    }
                };

            (
                crate::runtime_api::MemberStatsInfo {
                    direct_count: stats.direct_referrals,
                    team_count: stats.team_size,
                    total_spent: stats.spend.total_spent,
                    upgrade_eligible_spent: stats.spend.upgrade_eligible_spent,
                    cap_basis_spent_usdt,
                },
                crate::runtime_api::MemberCapInfo {
                    cumulative_claimed_usdt,
                    current_cap_usdt,
                    remaining_cap_usdt: current_cap_usdt.saturating_sub(cumulative_claimed_usdt),
                    is_capped: cumulative_claimed_usdt >= current_cap_usdt,
                    quota_nex_before_cap: quota_nex,
                    rate_snapshot_used: nex_usdt_rate,
                    base_cap_percent: rule.base_cap_percent,
                    base_cap_usdt,
                    unlock_count,
                    unlock_percent,
                    unlock_amount_per_step_usdt: match rule.cap_behavior {
                        CapBehavior::Fixed => None,
                        CapBehavior::UnlockByTeam { .. } => Some(unlock_amount_per_step_usdt),
                    },
                    next_direct_gap,
                    next_team_gap,
                    next_unlock_increase_usdt,
                },
            )
        }

        /// 查询某会员当前是否已达累计上限
        pub fn is_member_capped(entity_id: u64, account: &T::AccountId) -> bool {
            let Some(config) = PoolRewardConfigs::<T>::get(entity_id) else {
                return false;
            };
            let user_level = T::MemberProvider::custom_level_id(entity_id, account);
            let Some((eff, rule)) = Self::get_exact_level_rule(&config, user_level) else {
                return false;
            };
            let cumulative = MemberCumulativeClaimed::<T>::get(entity_id, account);
            cumulative >= Self::calculate_member_cap(entity_id, account, eff, rule)
        }

        fn build_strict_level_counts(
            entity_id: u64,
            config: &PoolRewardConfigOf<T>,
        ) -> alloc::vec::Vec<(u8, u32)> {
            config
                .level_rules
                .iter()
                .map(|(level_id, _)| {
                    (
                        *level_id,
                        T::MemberProvider::member_count_by_level(entity_id, *level_id),
                    )
                })
                .collect()
        }

        // ================================================================
        // R2: do_* 共享内部逻辑，消除 normal/force 代码重复
        // ================================================================

        /// 设置配置的共享逻辑（校验 + 写入 + 失效轮次 + 清除待生效变更）
        pub(crate) fn do_set_pool_reward_config(
            entity_id: u64,
            level_rules: BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels>,
            round_duration: BlockNumberFor<T>,
        ) -> DispatchResult {
            ensure!(
                T::EntityProvider::entity_exists(entity_id),
                Error::<T>::EntityNotActive
            );
            ensure!(
                round_duration > BlockNumberFor::<T>::zero(),
                Error::<T>::InvalidRoundDuration
            );
            ensure!(
                round_duration >= T::MinRoundDuration::get(),
                Error::<T>::RoundDurationTooShort
            );
            Self::validate_level_rules(&level_rules)?;

            let token_pool_enabled = PoolRewardConfigs::<T>::get(entity_id)
                .map(|c| c.token_pool_enabled)
                .unwrap_or(false);

            PoolRewardConfigs::<T>::insert(
                entity_id,
                PoolRewardConfig {
                    level_rules,
                    round_duration,
                    token_pool_enabled,
                },
            );

            Self::invalidate_current_round(entity_id);
            PendingPoolRewardConfig::<T>::remove(entity_id);
            Self::deposit_event(Event::PoolRewardConfigUpdated { entity_id });

            // 活跃队列维护：未暂停则加入（best-effort，满了不阻断配置设置）
            if !PoolRewardPaused::<T>::get(entity_id) {
                let _ = Self::add_to_active_entities(entity_id);
            }

            Ok(())
        }

        /// Token 开关共享逻辑
        pub(crate) fn do_set_token_pool_enabled(entity_id: u64, enabled: bool) -> DispatchResult {
            let mut changed = false;
            PoolRewardConfigs::<T>::try_mutate(entity_id, |maybe| -> DispatchResult {
                let config = maybe.as_mut().ok_or(Error::<T>::ConfigNotFound)?;
                if config.token_pool_enabled != enabled {
                    config.token_pool_enabled = enabled;
                    changed = true;
                }
                Ok(())
            })?;
            if changed {
                Self::invalidate_current_round(entity_id);
                Self::deposit_event(Event::TokenPoolEnabledUpdated { entity_id, enabled });
            }
            Ok(())
        }

        /// 部分清理（Owner 级别：不清理用户级记录）
        /// P2-6 修复: 同时清理 RoundHistory 和 DistributionStatistics（避免存储泄漏）
        pub(crate) fn do_clear_pool_reward_config(entity_id: u64) {
            PoolRewardConfigs::<T>::remove(entity_id);
            PoolRewardPaused::<T>::remove(entity_id);
            PendingPoolRewardConfig::<T>::remove(entity_id);
            Self::invalidate_current_round(entity_id);
            RoundHistory::<T>::remove(entity_id);
            DistributionStatistics::<T>::remove(entity_id);
            TokenPoolDeficit::<T>::remove(entity_id);
            CurrentRoundFunding::<T>::remove(entity_id);
            PoolFundingRecords::<T>::remove(entity_id);
            Self::remove_from_active_entities(entity_id);
            Self::deposit_event(Event::PoolRewardConfigCleared { entity_id });
        }

        /// 完整清理（Root / PlanWriter 级别：含全部用户记录）
        /// `max_users`: 每次 clear_prefix 的上限。如果未完全清理，发出 ClearIncomplete。
        /// 可重复调用直到用户记录完全清理（entity 级存储首次调用即清除，后续调用幂等）。
        pub(crate) fn do_full_clear_pool_reward(entity_id: u64, max_users: u32) {
            // Entity 级存储：幂等删除（首次删除实际值，后续调用删除不存在的 key = no-op）
            PoolRewardConfigs::<T>::remove(entity_id);
            CurrentRound::<T>::remove(entity_id);
            LastRoundId::<T>::remove(entity_id);
            PoolRewardPaused::<T>::remove(entity_id);
            PendingPoolRewardConfig::<T>::remove(entity_id);
            RoundHistory::<T>::remove(entity_id);
            DistributionStatistics::<T>::remove(entity_id);
            TokenPoolDeficit::<T>::remove(entity_id);
            CurrentRoundFunding::<T>::remove(entity_id);
            PoolFundingRecords::<T>::remove(entity_id);
            Self::remove_from_active_entities(entity_id);

            // 用户级存储：分批 clear_prefix
            let limit = max_users.max(1);
            let r1 = LastClaimedRound::<T>::clear_prefix(entity_id, limit, None);
            let r2 = ClaimRecords::<T>::clear_prefix(entity_id, limit, None);

            let has_remaining = r1.maybe_cursor.is_some() || r2.maybe_cursor.is_some();
            if has_remaining {
                let remaining = r1.loops.saturating_add(r2.loops);
                Self::deposit_event(Event::ClearIncomplete {
                    entity_id,
                    remaining,
                });
            }
            Self::deposit_event(Event::PoolRewardConfigCleared { entity_id });
        }

        // ================================================================
        // 活跃实体队列维护
        // ================================================================

        /// 去重插入活跃实体列表（满则返回 ActiveEntitiesFull）
        fn add_to_active_entities(entity_id: u64) -> DispatchResult {
            ActivePoolRewardEntities::<T>::try_mutate(|active| {
                if active.iter().any(|&id| id == entity_id) {
                    return Ok(()); // 已存在，跳过
                }
                active
                    .try_push(entity_id)
                    .map_err(|_| Error::<T>::ActiveEntitiesFull)?;
                Ok(())
            })
        }

        /// 移除活跃实体 + 游标越界修正
        fn remove_from_active_entities(entity_id: u64) {
            ActivePoolRewardEntities::<T>::mutate(|active| {
                active.retain(|&id| id != entity_id);
                let len = active.len() as u32;
                if len == 0 {
                    RotationCursor::<T>::kill();
                } else {
                    RotationCursor::<T>::mutate(|cursor| {
                        if *cursor >= len {
                            *cursor = 0;
                        }
                    });
                }
            });
        }

        fn build_level_quotas(
            level_counts: &[(u8, u32)],
        ) -> BoundedVec<LevelQuotaSnapshot, T::MaxPoolRewardLevels> {
            let mut snapshots = BoundedVec::default();
            for &(level_id, count) in level_counts.iter() {
                if snapshots
                    .try_push(LevelQuotaSnapshot {
                        level_id,
                        member_count: count,
                        claimed_count: 0,
                    })
                    .is_err()
                {
                    frame_support::defensive!(
                        "pool-reward: snapshot overflow in build_level_quotas"
                    );
                }
            }
            snapshots
        }

        fn ensure_current_round(
            entity_id: u64,
            config: &PoolRewardConfigOf<T>,
            now: BlockNumberFor<T>,
        ) -> Result<RoundInfoOf<T>, DispatchError> {
            if let Some(round) = CurrentRound::<T>::get(entity_id) {
                let end_block = round.start_block.saturating_add(config.round_duration);
                if now < end_block {
                    return Ok(round);
                }
            }
            Self::create_new_round(entity_id, config, now)
        }

        pub(crate) fn create_new_round(
            entity_id: u64,
            config: &PoolRewardConfigOf<T>,
            now: BlockNumberFor<T>,
        ) -> Result<RoundInfoOf<T>, DispatchError> {
            let nex_usdt_rate_snapshot = Some(Self::get_reliable_nex_usdt_rate()?);
            let pool_balance = T::PoolBalanceProvider::pool_balance(entity_id);
            let old_round = CurrentRound::<T>::get(entity_id);
            let old_round_id = old_round
                .as_ref()
                .map(|r| r.round_id)
                .unwrap_or_else(|| LastRoundId::<T>::get(entity_id));

            frame_support::ensure!(old_round_id < u64::MAX, Error::<T>::RoundIdOverflow);

            if let Some(ref old) = old_round {
                let computed_end = old.start_block.saturating_add(config.round_duration);
                // 取出当前轮次的资金来源汇总，嵌入归档摘要
                let funding_summary = CurrentRoundFunding::<T>::take(entity_id);
                let summary = CompletedRoundSummary {
                    round_id: old.round_id,
                    start_block: old.start_block,
                    end_block: computed_end,
                    pool_snapshot: old.pool_snapshot,
                    nex_usdt_rate_snapshot: old.nex_usdt_rate_snapshot,
                    eligible_count: old.eligible_count,
                    per_member_reward: old.per_member_reward,
                    claimed_count: old.claimed_count,
                    level_quotas: old.level_quotas.clone(),
                    token_pool_snapshot: old.token_pool_snapshot,
                    token_per_member_reward: old.token_per_member_reward,
                    token_claimed_count: old.token_claimed_count,
                    token_level_quotas: old.token_level_quotas.clone(),
                    funding_summary,
                };
                RoundHistory::<T>::mutate(entity_id, |history| {
                    if history.is_full() {
                        history.remove(0);
                    }
                    let _ = history.try_push(summary);
                });
                Self::deposit_event(Event::RoundArchived {
                    entity_id,
                    round_id: old.round_id,
                });
                DistributionStatistics::<T>::mutate(entity_id, |stats| {
                    stats.total_rounds_completed = stats.total_rounds_completed.saturating_add(1);
                });
            }

            // V-1 修复: 使用包含 fallback 用户的等级计数
            let level_counts = Self::build_strict_level_counts(entity_id, config);
            let eligible_count: u32 = level_counts.iter().map(|(_, count)| *count).sum();
            let per_member_reward = if eligible_count == 0 || pool_balance.is_zero() {
                BalanceOf::<T>::zero()
            } else {
                pool_balance / eligible_count.into()
            };
            let level_quotas = Self::build_level_quotas(&level_counts);

            let (token_pool_snapshot, token_per_member_reward, token_level_quotas) =
                if config.token_pool_enabled {
                    let token_balance = T::TokenPoolBalanceProvider::token_pool_balance(entity_id);
                    let token_per_member = if eligible_count == 0 || token_balance.is_zero() {
                        TokenBalanceOf::<T>::default()
                    } else {
                        token_balance / eligible_count.into()
                    };
                    (
                        Some(token_balance),
                        Some(token_per_member),
                        Some(Self::build_level_quotas(&level_counts)),
                    )
                } else {
                    (None, None, None)
                };

            let new_round = RoundInfo {
                round_id: old_round_id.saturating_add(1),
                start_block: now,
                pool_snapshot: pool_balance,
                nex_usdt_rate_snapshot,
                eligible_count,
                per_member_reward,
                claimed_count: 0,
                level_quotas,
                token_pool_snapshot,
                token_per_member_reward,
                token_claimed_count: 0,
                token_level_quotas,
            };

            CurrentRound::<T>::insert(entity_id, &new_round);

            // R1: 单一合并事件（原 NewRoundStarted + NewRoundDetails）
            let level_details: alloc::vec::Vec<(u8, u32, BalanceOf<T>)> = new_round
                .level_quotas
                .iter()
                .map(|s| (s.level_id, s.member_count, new_round.per_member_reward))
                .collect();
            let token_level_details = new_round.token_level_quotas.as_ref().map(|snaps| {
                snaps
                    .iter()
                    .map(|s| {
                        (
                            s.level_id,
                            s.member_count,
                            new_round.token_per_member_reward.unwrap_or_default(),
                        )
                    })
                    .collect()
            });
            Self::deposit_event(Event::NewRoundStarted {
                entity_id,
                round_id: new_round.round_id,
                pool_snapshot: pool_balance,
                token_pool_snapshot,
                level_snapshots: level_details,
                token_level_snapshots: token_level_details,
            });

            Ok(new_round)
        }

        /// 可领取金额预查询（只读）
        pub fn get_claimable(
            entity_id: u64,
            who: &T::AccountId,
        ) -> (BalanceOf<T>, TokenBalanceOf<T>) {
            let zero_nex = BalanceOf::<T>::zero();
            let zero_token = TokenBalanceOf::<T>::default();

            if GlobalPoolRewardPaused::<T>::get() || PoolRewardPaused::<T>::get(entity_id) {
                return (zero_nex, zero_token);
            }
            if !T::EntityProvider::is_entity_active(entity_id) {
                return (zero_nex, zero_token);
            }
            if !T::MemberProvider::is_member(entity_id, who) {
                return (zero_nex, zero_token);
            }
            if T::MemberProvider::is_banned(entity_id, who)
                || !T::MemberProvider::is_member_active(entity_id, who)
            {
                return (zero_nex, zero_token);
            }
            if !T::ParticipationGuard::can_participate(entity_id, who) {
                return (zero_nex, zero_token);
            }

            let config = match PoolRewardConfigs::<T>::get(entity_id) {
                Some(c) => c,
                None => return (zero_nex, zero_token),
            };

            let user_level = T::MemberProvider::custom_level_id(entity_id, who);
            let Some((effective_level, _level_rule)) =
                Self::get_exact_level_rule(&config, user_level)
            else {
                return (zero_nex, zero_token);
            };

            let now = <frame_system::Pallet<T>>::block_number();
            let round = if let Some(r) = CurrentRound::<T>::get(entity_id) {
                let end_block = r.start_block.saturating_add(config.round_duration);
                if now < end_block {
                    r
                } else {
                    return Self::simulate_claimable(entity_id, &config, effective_level);
                }
            } else {
                return Self::simulate_claimable(entity_id, &config, effective_level);
            };

            let last_round = LastClaimedRound::<T>::get(entity_id, who);
            if last_round >= round.round_id {
                return (zero_nex, zero_token);
            }

            let nex_reward = round
                .level_quotas
                .iter()
                .find(|s| s.level_id == effective_level)
                .and_then(|s| {
                    let quota_ok = s.claimed_count < s.member_count;
                    if quota_ok && !round.per_member_reward.is_zero() {
                        let pool = T::PoolBalanceProvider::pool_balance(entity_id);
                        let cumulative = MemberCumulativeClaimed::<T>::get(entity_id, who);
                        let cap = Self::calculate_member_cap(
                            entity_id,
                            who,
                            effective_level,
                            _level_rule,
                        );
                        // Mirror the extrinsic fallback: prefer snapshot rate, else live rate.
                        // 与 extrinsic 保持一致：优先快照汇率，缺失时使用实时汇率。
                        let rate = round.nex_usdt_rate_snapshot.or_else(|| {
                            let live = T::ExchangeRateProvider::get_nex_usdt_price();
                            if live > 0 {
                                Some(live)
                            } else {
                                None
                            }
                        })?;
                        let clipped = round.per_member_reward.min(Self::convert_usdt_to_nex(
                            cap.saturating_sub(cumulative),
                            rate,
                        ));
                        if pool >= clipped && !clipped.is_zero() {
                            Some(clipped)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or(zero_nex);

            let token_reward = round
                .token_level_quotas
                .as_ref()
                .and_then(|snaps| snaps.iter().find(|s| s.level_id == effective_level))
                .and_then(|s| {
                    let quota_ok = s.claimed_count < s.member_count;
                    let tr = round.token_per_member_reward.unwrap_or_default();
                    if quota_ok && !tr.is_zero() {
                        let token_pool = T::TokenPoolBalanceProvider::token_pool_balance(entity_id);
                        if token_pool >= tr {
                            Some(tr)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or(zero_token);

            (nex_reward, token_reward)
        }

        /// Safe per-member reward calculation with overflow protection.
        /// Shared by `build_level_snapshots` and `simulate_claimable`.

        fn simulate_claimable(
            entity_id: u64,
            config: &PoolRewardConfigOf<T>,
            _effective_level: u8,
        ) -> (BalanceOf<T>, TokenBalanceOf<T>) {
            let zero_nex = BalanceOf::<T>::zero();
            let zero_token = TokenBalanceOf::<T>::default();

            let pool_balance = T::PoolBalanceProvider::pool_balance(entity_id);
            if pool_balance.is_zero() {
                return (zero_nex, zero_token);
            }

            // V-1 修复: 使用包含 fallback 用户的等级计数
            let level_counts = Self::build_strict_level_counts(entity_id, config);
            let total_members: u32 = level_counts.iter().map(|(_, count)| *count).sum();

            let nex_reward = if total_members == 0 {
                zero_nex
            } else {
                pool_balance / total_members.into()
            };

            let token_reward = if config.token_pool_enabled {
                let token_balance = T::TokenPoolBalanceProvider::token_pool_balance(entity_id);
                if token_balance.is_zero() || total_members == 0 {
                    zero_token
                } else {
                    token_balance / total_members.into()
                }
            } else {
                zero_token
            };

            (nex_reward, token_reward)
        }

        /// 轮次领取进度查询
        pub fn get_round_statistics(
            entity_id: u64,
        ) -> Option<alloc::vec::Vec<(u8, u32, u32, BalanceOf<T>)>> {
            CurrentRound::<T>::get(entity_id).map(|round| {
                round
                    .level_quotas
                    .iter()
                    .map(|s| {
                        (
                            s.level_id,
                            s.member_count,
                            s.claimed_count,
                            round.per_member_reward,
                        )
                    })
                    .collect()
            })
        }

        // ================================================================
        // Runtime API helpers
        // ================================================================

        fn block_to_u64(b: BlockNumberFor<T>) -> u64 {
            sp_runtime::traits::UniqueSaturatedInto::<u64>::unique_saturated_into(b)
        }

        fn build_level_progress<B: Copy>(
            quotas: &[LevelQuotaSnapshot],
            per_member_reward: B,
            config: &PoolRewardConfigOf<T>,
        ) -> alloc::vec::Vec<crate::runtime_api::LevelProgressInfo<B>> {
            quotas
                .iter()
                .map(|s| {
                    let ratio_bps = config
                        .level_rules
                        .iter()
                        .find(|(id, _)| *id == s.level_id)
                        .map(|(_, rule)| rule.base_cap_percent)
                        .unwrap_or(0);
                    crate::runtime_api::LevelProgressInfo {
                        level_id: s.level_id,
                        ratio_bps,
                        member_count: s.member_count,
                        claimed_count: s.claimed_count,
                        per_member_reward,
                    }
                })
                .collect()
        }

        fn build_round_detail(
            round: &RoundInfoOf<T>,
            config: &PoolRewardConfigOf<T>,
        ) -> crate::runtime_api::RoundDetailInfo<BalanceOf<T>, TokenBalanceOf<T>> {
            let end_block = round.start_block.saturating_add(config.round_duration);
            crate::runtime_api::RoundDetailInfo {
                round_id: round.round_id,
                start_block: Self::block_to_u64(round.start_block),
                end_block: Self::block_to_u64(end_block),
                pool_snapshot: round.pool_snapshot,
                nex_usdt_rate_snapshot: round.nex_usdt_rate_snapshot,
                eligible_count: round.eligible_count,
                per_member_reward: round.per_member_reward,
                claimed_count: round.claimed_count,
                token_pool_snapshot: round.token_pool_snapshot,
                token_per_member_reward: round.token_per_member_reward,
                token_claimed_count: round.token_claimed_count,
                level_snapshots: Self::build_level_progress(
                    &round.level_quotas,
                    round.per_member_reward,
                    config,
                ),
                token_level_snapshots: round.token_level_quotas.as_ref().map(|ts| {
                    Self::build_level_progress(
                        ts,
                        round.token_per_member_reward.unwrap_or_default(),
                        config,
                    )
                }),
            }
        }

        fn build_level_rule_summaries(
            config: &PoolRewardConfigOf<T>,
        ) -> alloc::vec::Vec<crate::runtime_api::LevelRuleSummaryInfo> {
            config
                .level_rules
                .iter()
                .map(|(level_id, rule)| Self::build_level_rule_summary(*level_id, rule))
                .collect()
        }

        /// Runtime API: 会员沉淀池详情
        pub fn get_pool_reward_member_view(
            entity_id: u64,
            who: &T::AccountId,
        ) -> Option<crate::runtime_api::PoolRewardMemberView<BalanceOf<T>, TokenBalanceOf<T>>>
        {
            let config = PoolRewardConfigs::<T>::get(entity_id)?;

            if !T::MemberProvider::is_member(entity_id, who) {
                return None;
            }

            let user_level = T::MemberProvider::custom_level_id(entity_id, who);
            let effective_level = Self::get_exact_level_rule(&config, user_level)
                .map(|(level, _)| level)
                .unwrap_or(user_level);
            let level_rule =
                Self::get_exact_level_rule(&config, user_level).map(|(_, rule)| rule)?;

            let (claimable_nex, claimable_token) = Self::get_claimable(entity_id, who);
            let (member_stats, cap_info) =
                Self::compute_member_cap_info(entity_id, who, level_rule);
            let level_rule_details = Self::build_level_rule_summaries(&config);

            let is_paused =
                GlobalPoolRewardPaused::<T>::get() || PoolRewardPaused::<T>::get(entity_id);

            let last_claimed = LastClaimedRound::<T>::get(entity_id, who);

            let now = <frame_system::Pallet<T>>::block_number();
            let round = CurrentRound::<T>::get(entity_id);
            let current_round_id = round
                .as_ref()
                .map(|r| r.round_id)
                .unwrap_or_else(|| LastRoundId::<T>::get(entity_id));

            let round_expired = round
                .as_ref()
                .map(|r| now >= r.start_block.saturating_add(config.round_duration))
                .unwrap_or(true);

            let already_claimed = if round_expired {
                false
            } else {
                last_claimed >= current_round_id && current_round_id > 0
            };

            let (
                round_start_block,
                round_end_block,
                pool_snapshot,
                token_pool_snapshot,
                level_progress,
                token_level_progress,
            ) = match round {
                Some(ref r) => {
                    let end = r.start_block.saturating_add(config.round_duration);
                    (
                        Self::block_to_u64(r.start_block),
                        Self::block_to_u64(end),
                        r.pool_snapshot,
                        r.token_pool_snapshot,
                        Self::build_level_progress(&r.level_quotas, r.per_member_reward, &config),
                        r.token_level_quotas.as_ref().map(|ts| {
                            Self::build_level_progress(
                                ts,
                                r.token_per_member_reward.unwrap_or_default(),
                                &config,
                            )
                        }),
                    )
                }
                None => (
                    0u64,
                    0u64,
                    BalanceOf::<T>::zero(),
                    None,
                    alloc::vec::Vec::new(),
                    None,
                ),
            };

            let claim_history: alloc::vec::Vec<
                crate::runtime_api::ClaimRecordInfo<BalanceOf<T>, TokenBalanceOf<T>>,
            > = ClaimRecords::<T>::get(entity_id, who)
                .iter()
                .map(|r| crate::runtime_api::ClaimRecordInfo {
                    round_id: r.round_id,
                    amount: r.amount,
                    token_amount: r.token_amount,
                    level_id: r.level_id,
                    claimed_at: Self::block_to_u64(r.claimed_at),
                })
                .collect();

            Some(crate::runtime_api::PoolRewardMemberView {
                round_duration: Self::block_to_u64(config.round_duration),
                token_pool_enabled: config.token_pool_enabled,
                level_rules: config
                    .level_rules
                    .iter()
                    .map(|(id, rule)| (*id, rule.base_cap_percent))
                    .collect(),
                level_rule_details,
                current_round_id,
                round_start_block,
                round_end_block,
                pool_snapshot,
                token_pool_snapshot,
                effective_level,
                claimable_nex,
                claimable_token,
                already_claimed,
                round_expired,
                last_claimed_round: last_claimed,
                member_stats,
                cap_info,
                level_progress,
                token_level_progress,
                claim_history,
                is_paused,
                has_pending_config: PendingPoolRewardConfig::<T>::contains_key(entity_id),
            })
        }

        /// Runtime API: 管理者沉淀池总览
        pub fn get_pool_reward_admin_view(
            entity_id: u64,
        ) -> Option<crate::runtime_api::PoolRewardAdminView<BalanceOf<T>, TokenBalanceOf<T>>>
        {
            let config = PoolRewardConfigs::<T>::get(entity_id)?;
            let level_rule_details = Self::build_admin_level_rules(entity_id, &config);

            let current_round =
                CurrentRound::<T>::get(entity_id).map(|r| Self::build_round_detail(&r, &config));

            let stats = DistributionStatistics::<T>::get(entity_id);

            let round_history: alloc::vec::Vec<
                crate::runtime_api::CompletedRoundInfo<BalanceOf<T>, TokenBalanceOf<T>>,
            > = RoundHistory::<T>::get(entity_id)
                .iter()
                .map(|r| crate::runtime_api::CompletedRoundInfo {
                    round_id: r.round_id,
                    start_block: Self::block_to_u64(r.start_block),
                    end_block: Self::block_to_u64(r.end_block),
                    pool_snapshot: r.pool_snapshot,
                    nex_usdt_rate_snapshot: r.nex_usdt_rate_snapshot,
                    eligible_count: r.eligible_count,
                    per_member_reward: r.per_member_reward,
                    claimed_count: r.claimed_count,
                    token_pool_snapshot: r.token_pool_snapshot,
                    token_per_member_reward: r.token_per_member_reward,
                    token_claimed_count: r.token_claimed_count,
                    level_snapshots: Self::build_level_progress(
                        &r.level_quotas,
                        r.per_member_reward,
                        &config,
                    ),
                    token_level_snapshots: r.token_level_quotas.as_ref().map(|ts| {
                        Self::build_level_progress(
                            ts,
                            r.token_per_member_reward.unwrap_or_default(),
                            &config,
                        )
                    }),
                    funding_summary: crate::runtime_api::FundingSummaryInfo {
                        nex_commission_remainder: r.funding_summary.nex_commission_remainder,
                        token_platform_fee_retention: r
                            .funding_summary
                            .token_platform_fee_retention,
                        token_commission_remainder: r.funding_summary.token_commission_remainder,
                        nex_cancel_return: r.funding_summary.nex_cancel_return,
                        total_funding_count: r.funding_summary.total_funding_count,
                    },
                })
                .collect();

            let pending_config = PendingPoolRewardConfig::<T>::get(entity_id).map(|p| {
                crate::runtime_api::PendingConfigInfo {
                    level_rules: p
                        .level_rules
                        .iter()
                        .map(|(id, rule)| (*id, rule.base_cap_percent))
                        .collect(),
                    level_rule_details: p
                        .level_rules
                        .iter()
                        .map(|(level_id, rule)| Self::build_level_rule_summary(*level_id, rule))
                        .collect(),
                    round_duration: Self::block_to_u64(p.round_duration),
                    apply_after: Self::block_to_u64(p.apply_after),
                }
            });

            let current_pool_balance = T::PoolBalanceProvider::pool_balance(entity_id);
            let current_token_pool_balance =
                T::TokenPoolBalanceProvider::token_pool_balance(entity_id);

            Some(crate::runtime_api::PoolRewardAdminView {
                level_rules: config
                    .level_rules
                    .iter()
                    .map(|(id, rule)| (*id, rule.base_cap_percent))
                    .collect(),
                level_rule_details,
                round_duration: Self::block_to_u64(config.round_duration),
                token_pool_enabled: config.token_pool_enabled,
                current_round,
                total_nex_distributed: stats.total_nex_distributed,
                total_token_distributed: stats.total_token_distributed,
                total_rounds_completed: stats.total_rounds_completed,
                total_claims: stats.total_claims,
                round_history,
                pending_config,
                is_paused: PoolRewardPaused::<T>::get(entity_id),
                is_global_paused: GlobalPoolRewardPaused::<T>::get(),
                current_pool_balance,
                current_token_pool_balance,
                token_pool_deficit: TokenPoolDeficit::<T>::get(entity_id),
            })
        }
    }
}

// ============================================================================
// PoolRewardPlanWriter implementation
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::PoolRewardPlanWriter for pallet::Pallet<T> {
    fn set_pool_reward_config(
        entity_id: u64,
        level_rules: alloc::vec::Vec<(u8, pallet_entity_common::PoolRewardLevelClaimRule)>,
        round_duration: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        let bounded: frame_support::BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels> =
            level_rules
                .try_into()
                .map_err(|_| sp_runtime::DispatchError::Other("TooManyLevels"))?;

        let rd: frame_system::pallet_prelude::BlockNumberFor<T> = round_duration.into();
        pallet::Pallet::<T>::do_set_pool_reward_config(entity_id, bounded, rd)
    }

    fn clear_config(entity_id: u64) -> Result<(), sp_runtime::DispatchError> {
        pallet::Pallet::<T>::do_full_clear_pool_reward(entity_id, u32::MAX);
        Ok(())
    }

    fn set_token_pool_enabled(
        entity_id: u64,
        enabled: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet::Pallet::<T>::do_set_token_pool_enabled(entity_id, enabled)
    }
}

// ============================================================================
// PoolRewardQueryProvider 实现
// ============================================================================

impl<T: pallet::Config>
    pallet_commission_common::PoolRewardQueryProvider<T::AccountId, BalanceOf<T>, TokenBalanceOf<T>>
    for pallet::Pallet<T>
{
    fn claimable(entity_id: u64, account: &T::AccountId) -> (BalanceOf<T>, TokenBalanceOf<T>) {
        Self::get_claimable(entity_id, account)
    }

    fn is_paused(entity_id: u64) -> bool {
        pallet::GlobalPoolRewardPaused::<T>::get() || pallet::PoolRewardPaused::<T>::get(entity_id)
    }

    fn current_round_id(entity_id: u64) -> u64 {
        pallet::CurrentRound::<T>::get(entity_id)
            .map(|r| r.round_id)
            .unwrap_or_else(|| pallet::LastRoundId::<T>::get(entity_id))
    }
}

// ============================================================================
// OnMemberLevelChanged / OnMemberTeamChanged — CappedMemberCount 维护
// ============================================================================

// ============================================================================
// PoolFundingCallback 实现
// ============================================================================

impl<T: pallet::Config> pallet_commission_common::PoolFundingCallback for pallet::Pallet<T> {
    fn on_pool_funded(
        entity_id: u64,
        source: pallet_commission_common::FundingSource,
        nex_amount: u128,
        token_amount: u128,
        order_id: u64,
    ) {
        use pallet_commission_common::FundingSource;

        // 1. 更新当前轮次资金来源汇总
        pallet::CurrentRoundFunding::<T>::mutate(entity_id, |summary| {
            match source {
                FundingSource::OrderCommissionRemainder => {
                    summary.nex_commission_remainder =
                        summary.nex_commission_remainder.saturating_add(nex_amount);
                }
                FundingSource::TokenPlatformFeeRetention => {
                    summary.token_platform_fee_retention = summary
                        .token_platform_fee_retention
                        .saturating_add(token_amount);
                }
                FundingSource::TokenCommissionRemainder => {
                    summary.token_commission_remainder = summary
                        .token_commission_remainder
                        .saturating_add(token_amount);
                }
                FundingSource::CancelReturn => {
                    summary.nex_cancel_return =
                        summary.nex_cancel_return.saturating_add(nex_amount);
                }
            }
            summary.total_funding_count = summary.total_funding_count.saturating_add(1);
        });

        // 2. FIFO 明细记录
        let block_number: u32 = <frame_system::Pallet<T>>::block_number()
            .try_into()
            .unwrap_or(0u32);
        let record = pallet::PoolFundingRecord {
            source,
            nex_amount,
            token_amount,
            order_id,
            block_number,
        };
        pallet::PoolFundingRecords::<T>::mutate(entity_id, |records| {
            if records.is_full() {
                records.remove(0);
            }
            let _ = records.try_push(record);
        });

        // 3. 发出事件
        pallet::Pallet::<T>::deposit_event(pallet::Event::PoolFunded {
            entity_id,
            source,
            nex_amount,
            token_amount,
            order_id,
        });
    }
}

// ============================================================================
// P2-14: OnMemberRemoved 回调实现
// ============================================================================

impl<T: pallet::Config> pallet_entity_common::OnMemberRemoved<T::AccountId> for pallet::Pallet<T> {
    fn on_member_removed(entity_id: u64, account: &T::AccountId) {
        pallet::LastClaimedRound::<T>::remove(entity_id, account);
        pallet::ClaimRecords::<T>::remove(entity_id, account);
    }
}

impl<T: pallet::Config> pallet_entity_common::OnMemberLevelChanged<T::AccountId>
    for pallet::Pallet<T>
{
    fn on_level_changed(_entity_id: u64, _account: &T::AccountId, _old_level: u8, _new_level: u8) {
        // CappedMemberCount 采用单调递增语义：仅在 claim 时 +1，
        // 等级变更不重算，保留历史统计含义。
    }
}

impl<T: pallet::Config> pallet_entity_common::OnMemberTeamChanged<T::AccountId>
    for pallet::Pallet<T>
{
    fn on_team_changed(
        _entity_id: u64,
        _account: &T::AccountId,
        _old_direct: u32,
        _new_direct: u32,
        _old_team: u32,
        _new_team: u32,
    ) {
        // CappedMemberCount 采用单调递增语义：仅在 claim 时 +1，
        // 团队变更不重算，保留历史统计含义。
    }
}

#[cfg(test)]
mod tests;
