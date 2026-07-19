// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Oracle-based futarchy proposal evaluation and scheduling.
//! 基于预言机的未来政治提案评估与调度。

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod benchmarking;
mod dispatchable_impls;
pub mod mock;
mod pallet_impls;
mod proposal_storage;
mod tests;
pub mod traits;
pub mod types;
pub mod weights;

pub use pallet::*;

#[frame_support::pallet]
mod pallet {
    use crate::{traits::ProposalStorage, types::Proposal, weights::WeightInfoZeitgeist};
    use alloc::fmt::Debug;
    use core::marker::PhantomData;
    use frame_support::{
        pallet_prelude::{IsType, StorageMap, StorageValue, StorageVersion, ValueQuery, Weight},
        traits::{schedule::v3::Anon as ScheduleAnon, Bounded, EnsureOrigin, Hooks, OriginTrait},
        transactional, Blake2_128Concat, BoundedVec,
    };
    use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
    use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
    use scale_info::TypeInfo;
    use sp_runtime::{traits::Get, DispatchResult, SaturatedConversion};
    use zeitgeist_primitives::traits::FutarchyOracle;

    #[cfg(feature = "runtime-benchmarks")]
    use zeitgeist_primitives::traits::FutarchyBenchmarkHelper;

    /// Futarchy pallet configuration.
    /// 未来政治 pallet 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Creates benchmark oracle fixtures.
        /// 创建基准测试预言机夹具。
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: FutarchyBenchmarkHelper<Self::Oracle>;

        /// Maximum number of proposals in flight.
        /// 同时处理中提案的最大数量。
        type MaxProposals: Get<u32>;

        /// Minimum proposal evaluation duration.
        /// 提案评估的最短持续区块数。
        type MinDuration: Get<BlockNumberFor<Self>>;

        /// Oracle used to evaluate proposals.
        /// 用于评估提案的预言机。
        type Oracle: FutarchyOracle<BlockNumber = BlockNumberFor<Self>>
            + Clone
            + Debug
            + Decode
            + DecodeWithMemTracking
            + Encode
            + Eq
            + MaxEncodedLen
            + PartialEq
            + TypeInfo;

        /// Runtime event type.
        /// Runtime 事件类型。
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Anonymous FRAME v3 scheduler boundary used to execute approved calls.
        /// 用于执行已批准调用的 FRAME v3 匿名调度器边界。
        type Scheduler: ScheduleAnon<
            BlockNumberFor<Self>,
            CallOf<Self>,
            PalletsOriginOf<Self>,
            Hasher = <Self as frame_system::Config>::Hashing,
        >;

        /// Origin allowed to submit governance proposals.
        /// 允许提交治理提案的 origin。
        type SubmitOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Weight implementation.
        /// 权重实现。
        type WeightInfo: WeightInfoZeitgeist;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    pub(crate) type CallOf<T> = <T as frame_system::Config>::RuntimeCall;
    pub(crate) type BoundedCallOf<T> = Bounded<CallOf<T>, <T as frame_system::Config>::Hashing>;
    pub(crate) type OracleOf<T> = <T as Config>::Oracle;
    pub(crate) type PalletsOriginOf<T> =
        <<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin;
    pub(crate) type ProposalsOf<T> = BoundedVec<Proposal<T>, <T as Config>::MaxProposals>;

    pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

    #[pallet::storage]
    pub type Proposals<T: Config> =
        StorageMap<_, Blake2_128Concat, BlockNumberFor<T>, ProposalsOf<T>, ValueQuery>;

    #[pallet::storage]
    pub type ProposalCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A proposal was submitted. / 已提交提案。
        Submitted {
            duration: BlockNumberFor<T>,
            proposal: Proposal<T>,
        },
        /// The oracle rejected a proposal. / 预言机拒绝了提案。
        Rejected { proposal: Proposal<T> },
        /// A proposal was scheduled. / 提案已进入调度。
        Scheduled { proposal: Proposal<T> },
        /// The scheduler failed unexpectedly. / 调度器意外失败。
        UnexpectedSchedulerError,
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The proposal cache is full. / 提案缓存已满。
        CacheFull,
        /// Duration is below `MinDuration`. / 持续时间低于 `MinDuration`。
        DurationTooShort,
        /// Proposal storage was internally inconsistent. / 提案存储内部状态不一致。
        UnexpectedStorageFailure,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Submit a proposal for oracle evaluation after `duration` blocks.
        /// 提交一个在 `duration` 个区块后由预言机评估的提案。
        #[pallet::call_index(0)]
        #[transactional]
        #[pallet::weight(T::WeightInfo::submit_proposal())]
        pub fn submit_proposal(
            origin: OriginFor<T>,
            duration: BlockNumberFor<T>,
            proposal: Proposal<T>,
        ) -> DispatchResult {
            T::SubmitOrigin::ensure_origin(origin)?;
            Self::do_submit_proposal(duration, proposal)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(now: BlockNumberFor<T>) -> Weight {
            let mut total_weight = Weight::zero();

            // Update all oracles.
            let mutate_all_result =
                <Pallet<T> as ProposalStorage<T>>::mutate_all(|p| p.oracle.update(now));
            if let Ok(block_to_weights) = mutate_all_result {
                // We did one storage read per vector cached. Shouldn't saturate, but technically
                // might.
                let reads: u64 = block_to_weights.len().saturated_into();
                total_weight = total_weight.saturating_add(T::DbWeight::get().reads(reads));
                for weights in block_to_weights.values() {
                    for &weight in weights {
                        total_weight = total_weight.saturating_add(weight);
                    }
                }
            } else {
                // Unreachable!
                return total_weight;
            }

            let proposals = if let Ok(proposals) = <Pallet<T> as ProposalStorage<T>>::take(now) {
                total_weight = total_weight
                    .saturating_add(T::WeightInfo::take_proposals(proposals.len() as u32));
                proposals
            } else {
                // assumes the worst case scenario
                total_weight = total_weight
                    .saturating_add(T::WeightInfo::take_proposals(T::MaxProposals::get()));
                return total_weight;
            };

            for proposal in proposals.into_iter() {
                total_weight = total_weight.saturating_add(Self::maybe_schedule_proposal(proposal));
            }
            total_weight
        }
    }
}
