#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod authorized_pallet_api;
mod benchmarks;
pub mod migrations;
mod mock;
mod mock_storage;
mod tests;
pub mod weights;

pub use authorized_pallet_api::AuthorizedPalletApi;
pub use pallet::*;

#[frame_support::pallet]
mod pallet {
    use crate::{weights::WeightInfoZeitgeist, AuthorizedPalletApi};
    use alloc::vec::Vec;
    use core::marker::PhantomData;
    use frame_support::{
        dispatch::DispatchResultWithPostInfo,
        ensure,
        pallet_prelude::{ConstU32, EnsureOrigin, OptionQuery, StorageMap, Weight},
        traits::{Currency, Get, Hooks, StorageVersion},
        PalletId, Twox64Concat,
    };
    use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
    use sp_runtime::{traits::Saturating, DispatchError, DispatchResult};
    use zeitgeist_primitives::{
        traits::{DisputeApi, DisputeMaxWeightApi, DisputeResolutionApi},
        types::{
            AuthorityReport, GlobalDisputeItem, Market, MarketDisputeMechanism, MarketStatus,
            OutcomeReport, ResultWithWeightInfo,
        },
    };
    use zrml_market_commons::MarketCommonsPalletApi;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(3);
    pub(crate) type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub(crate) type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::NegativeImbalance;
    pub(crate) type MarketIdOf<T> =
        <<T as Config>::MarketCommons as MarketCommonsPalletApi>::MarketId;
    pub(crate) type MomentOf<T> = <<T as Config>::MarketCommons as MarketCommonsPalletApi>::Moment;
    pub type CacheSize = ConstU32<64>;
    pub(crate) type MarketOf<T> = Market<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        BlockNumberFor<T>,
        MomentOf<T>,
        MarketIdOf<T>,
    >;

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Submit or replace an authorized outcome for a disputed market.
        /// 为争议市场提交或替换授权结果。
        #[pallet::call_index(0)]
        #[pallet::weight(
            T::WeightInfo::authorize_market_outcome_first_report(CacheSize::get()).max(
                T::WeightInfo::authorize_market_outcome_existing_report(),
            )
        )]
        #[frame_support::transactional]
        pub fn authorize_market_outcome(
            origin: OriginFor<T>,
            market_id: MarketIdOf<T>,
            outcome: OutcomeReport,
        ) -> DispatchResultWithPostInfo {
            T::AuthorizedDisputeResolutionOrigin::ensure_origin(origin)?;
            let market = T::MarketCommons::market(&market_id)?;
            ensure!(
                market.status == MarketStatus::Disputed,
                Error::<T>::MarketIsNotDisputed
            );
            ensure!(
                market.matches_outcome_report(&outcome),
                Error::<T>::OutcomeMismatch
            );
            Self::ensure_dispute_mechanism(&market)?;

            let now = frame_system::Pallet::<T>::block_number();
            let previous = AuthorizedOutcomeReports::<T>::get(market_id);
            let (report, ids_len) = match &previous {
                Some(report) => (
                    AuthorityReport {
                        resolve_at: report.resolve_at,
                        outcome: outcome.clone(),
                    },
                    0,
                ),
                None => {
                    let resolve_at = now.saturating_add(T::CorrectionPeriod::get());
                    let ids_len = T::DisputeResolution::add_auto_resolve(&market_id, resolve_at)?;
                    (
                        AuthorityReport {
                            resolve_at,
                            outcome: outcome.clone(),
                        },
                        ids_len,
                    )
                }
            };
            AuthorizedOutcomeReports::<T>::insert(market_id, report);
            Self::deposit_event(Event::AuthorityReported { market_id, outcome });
            let weight = if previous.is_none() {
                T::WeightInfo::authorize_market_outcome_first_report(ids_len)
            } else {
                T::WeightInfo::authorize_market_outcome_existing_report()
            };
            Ok(Some(weight).into())
        }
    }

    /// Runtime configuration for authorized dispute resolution.
    /// 授权争议解决的 runtime 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Native currency. / 原生货币。
        type Currency: Currency<Self::AccountId>;
        /// Outcome correction period. / 结果修正周期。
        #[pallet::constant]
        type CorrectionPeriod: Get<BlockNumberFor<Self>>;
        /// Prediction-market scheduling bridge. / 预测市场调度桥。
        type DisputeResolution: DisputeResolutionApi<
            AccountId = Self::AccountId,
            BlockNumber = BlockNumberFor<Self>,
            MarketId = MarketIdOf<Self>,
            Moment = MomentOf<Self>,
        >;
        /// Shared market storage API. / 共享市场存储 API。
        type MarketCommons: MarketCommonsPalletApi<
            AccountId = Self::AccountId,
            BlockNumber = BlockNumberFor<Self>,
            Balance = BalanceOf<Self>,
        >;
        /// Origin allowed to authorize outcomes. / 允许授权结果的 origin。
        type AuthorizedDisputeResolutionOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// Pallet sovereign identifier. / Pallet 主权账户标识。
        #[pallet::constant]
        type PalletId: Get<PalletId>;
        /// Benchmark-generated weights. / 基准测试生成的权重。
        type WeightInfo: WeightInfoZeitgeist;
    }

    #[pallet::error]
    pub enum Error<T> {
        MarketDoesNotHaveDisputeMechanismAuthorized,
        MarketIsNotDisputed,
        OutcomeMismatch,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// The authority reported an outcome. / 授权机构报告了结果。
        AuthorityReported {
            market_id: MarketIdOf<T>,
            outcome: OutcomeReport,
        },
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    impl<T: Config> Pallet<T> {
        fn ensure_dispute_mechanism(market: &MarketOf<T>) -> DispatchResult {
            ensure!(
                market.dispute_mechanism == Some(MarketDisputeMechanism::Authorized),
                Error::<T>::MarketDoesNotHaveDisputeMechanismAuthorized
            );
            Ok(())
        }
    }

    impl<T: Config> DisputeMaxWeightApi for Pallet<T> {
        fn on_dispute_max_weight() -> Weight {
            T::WeightInfo::on_dispute_weight()
        }
        fn on_resolution_max_weight() -> Weight {
            T::WeightInfo::on_resolution_weight()
        }
        fn exchange_max_weight() -> Weight {
            T::WeightInfo::exchange_weight()
        }
        fn get_auto_resolve_max_weight() -> Weight {
            T::WeightInfo::get_auto_resolve_weight()
        }
        fn has_failed_max_weight() -> Weight {
            T::WeightInfo::has_failed_weight()
        }
        fn on_global_dispute_max_weight() -> Weight {
            T::WeightInfo::on_global_dispute_weight()
        }
        fn clear_max_weight() -> Weight {
            T::WeightInfo::clear_weight()
        }
    }

    impl<T: Config> DisputeApi for Pallet<T> {
        type AccountId = T::AccountId;
        type Balance = BalanceOf<T>;
        type NegativeImbalance = NegativeImbalanceOf<T>;
        type BlockNumber = BlockNumberFor<T>;
        type MarketId = MarketIdOf<T>;
        type Moment = MomentOf<T>;
        type Origin = T::RuntimeOrigin;

        fn on_dispute(
            _: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> Result<ResultWithWeightInfo<()>, DispatchError> {
            Self::ensure_dispute_mechanism(market)?;
            Ok(ResultWithWeightInfo {
                result: (),
                weight: T::WeightInfo::on_dispute_weight(),
            })
        }

        fn on_resolution(
            market_id: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> Result<ResultWithWeightInfo<Option<OutcomeReport>>, DispatchError> {
            Self::ensure_dispute_mechanism(market)?;
            let report = AuthorizedOutcomeReports::<T>::take(market_id);
            Ok(ResultWithWeightInfo {
                result: report.map(|r| r.outcome),
                weight: T::WeightInfo::on_resolution_weight(),
            })
        }

        fn exchange(
            _: &Self::MarketId,
            market: &MarketOf<T>,
            _: &OutcomeReport,
            overall_imbalance: NegativeImbalanceOf<T>,
        ) -> Result<ResultWithWeightInfo<NegativeImbalanceOf<T>>, DispatchError> {
            Self::ensure_dispute_mechanism(market)?;
            Ok(ResultWithWeightInfo {
                result: overall_imbalance,
                weight: T::WeightInfo::exchange_weight(),
            })
        }

        fn get_auto_resolve(
            market_id: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> ResultWithWeightInfo<Option<Self::BlockNumber>> {
            let result = if market.dispute_mechanism == Some(MarketDisputeMechanism::Authorized) {
                AuthorizedOutcomeReports::<T>::get(market_id).map(|report| report.resolve_at)
            } else {
                None
            };
            ResultWithWeightInfo {
                result,
                weight: T::WeightInfo::get_auto_resolve_weight(),
            }
        }

        fn has_failed(
            _: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> Result<ResultWithWeightInfo<bool>, DispatchError> {
            Self::ensure_dispute_mechanism(market)?;
            Ok(ResultWithWeightInfo {
                result: false,
                weight: T::WeightInfo::has_failed_weight(),
            })
        }

        fn on_global_dispute(
            _: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> Result<
            ResultWithWeightInfo<Vec<GlobalDisputeItem<Self::AccountId, Self::Balance>>>,
            DispatchError,
        > {
            Self::ensure_dispute_mechanism(market)?;
            Ok(ResultWithWeightInfo {
                result: Vec::new(),
                weight: T::WeightInfo::on_global_dispute_weight(),
            })
        }

        fn clear(
            market_id: &Self::MarketId,
            market: &MarketOf<T>,
        ) -> Result<ResultWithWeightInfo<()>, DispatchError> {
            Self::ensure_dispute_mechanism(market)?;
            AuthorizedOutcomeReports::<T>::remove(market_id);
            Ok(ResultWithWeightInfo {
                result: (),
                weight: T::WeightInfo::clear_weight(),
            })
        }
    }

    impl<T: Config> AuthorizedPalletApi for Pallet<T> {}

    /// Authorized report by market id. / 按市场 id 存储的授权报告。
    #[pallet::storage]
    #[pallet::getter(fn outcomes)]
    pub type AuthorizedOutcomeReports<T: Config> =
        StorageMap<_, Twox64Concat, MarketIdOf<T>, AuthorityReport<BlockNumberFor<T>>, OptionQuery>;
}

#[cfg(any(feature = "runtime-benchmarks", test))]
fn market_mock<T: Config>() -> MarketOf<T> {
    use frame_support::traits::Get;
    use sp_runtime::{traits::AccountIdConversion, Perbill};
    use zeitgeist_primitives::types::{
        Asset, Deadlines, Market, MarketBonds, MarketCreation, MarketDisputeMechanism,
        MarketPeriod, MarketStatus, MarketType, ScoringRule,
    };
    Market {
        base_asset: Asset::Ztg,
        market_id: Default::default(),
        creation: MarketCreation::Permissionless,
        creator_fee: Perbill::zero(),
        creator: T::PalletId::get().into_account_truncating(),
        market_type: MarketType::Scalar(0..=100),
        dispute_mechanism: Some(MarketDisputeMechanism::Authorized),
        metadata: Default::default(),
        oracle: T::PalletId::get().into_account_truncating(),
        period: MarketPeriod::Block(Default::default()),
        deadlines: Deadlines {
            grace_period: 1_u32.into(),
            oracle_duration: 1_u32.into(),
            dispute_duration: 1_u32.into(),
        },
        report: None,
        resolved_outcome: None,
        scoring_rule: ScoringRule::AmmCdaHybrid,
        status: MarketStatus::Disputed,
        bonds: MarketBonds::default(),
        early_close: None,
    }
}
