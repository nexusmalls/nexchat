//! Benchmarks for `pallet-task-bounty`.
//! `pallet-task-bounty` 的基准测试。
//!
//! Run via the node benchmark harness to generate real `WeightInfo`.
//! 通过节点基准框架运行以生成真实 `WeightInfo`。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as TaskBounty;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::{Hash, One, Saturating, Zero};

/// Build a comfortably large balance from `MinReward` without `From<u32>` bounds.
/// 在不依赖 `From<u32>` 约束的前提下，用 `MinReward` 累加出足够大的余额。
fn big_balance<T: Config>() -> BalanceOf<T> {
    let unit = T::MinReward::get();
    let mut v = BalanceOf::<T>::zero();
    let mut i = 0u32;
    while i < 10_000 {
        v = v.saturating_add(unit);
        i = i.saturating_add(1);
    }
    v
}

fn fund<T: Config>(who: &T::AccountId) {
    let _ = T::Currency::make_free_balance_be(who, big_balance::<T>());
}

/// Create a bounty owned by `poster`, returning its id. / 由 `poster` 创建悬赏并返回 id。
fn setup_bounty<T: Config>(poster: &T::AccountId, kind: BountyKind, slots: u32) -> u64 {
    fund::<T>(poster);
    let reward = T::MinReward::get();
    TaskBounty::<T>::create_bounty(
        RawOrigin::Signed(poster.clone()).into(),
        kind,
        reward,
        slots,
        0,
        None,
        None,
    )
    .expect("create_bounty in benchmark setup");
    T::EscrowIdOffset::get()
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn create_bounty() {
        let poster: T::AccountId = whitelisted_caller();
        fund::<T>(&poster);
        let reward = T::MinReward::get();
        #[extrinsic_call]
        create_bounty(RawOrigin::Signed(poster), BountyKind::Single, reward, 1, 0, None, None);

        assert!(Bounties::<T>::contains_key(T::EscrowIdOffset::get()));
    }

    #[benchmark]
    fn submit() {
        let poster: T::AccountId = account("poster", 0, 0);
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let solver: T::AccountId = whitelisted_caller();
        fund::<T>(&solver);

        #[extrinsic_call]
        submit(RawOrigin::Signed(solver), id, Some(1u64), None);

        assert!(Submissions::<T>::contains_key(id, 0));
    }

    #[benchmark]
    fn deliver() {
        let poster: T::AccountId = account("poster", 0, 0);
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let solver: T::AccountId = whitelisted_caller();
        fund::<T>(&solver);
        let salt: [u8; 32] = [7u8; 32];
        let evidence: u64 = 9;
        let commit = T::Hashing::hash_of(&(evidence, salt, &solver));
        TaskBounty::<T>::submit(RawOrigin::Signed(solver.clone()).into(), id, None, Some(commit))
            .expect("submit with commit");

        #[extrinsic_call]
        deliver(RawOrigin::Signed(solver), id, 0, evidence, Some(salt));

        let sub = Submissions::<T>::get(id, 0).unwrap();
        assert_eq!(sub.state, SubmissionState::Delivered);
    }

    #[benchmark]
    fn accept() {
        let poster: T::AccountId = whitelisted_caller();
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let solver: T::AccountId = account("solver", 0, 0);
        fund::<T>(&solver);
        TaskBounty::<T>::submit(RawOrigin::Signed(solver).into(), id, Some(1u64), None)
            .expect("submit deliverable");
        // pass MinOpenWindow (created at block 0). / 越过开放期。
        let bn = T::MinOpenWindow::get().saturating_add(One::one());
        frame_system::Pallet::<T>::set_block_number(bn);

        #[extrinsic_call]
        accept(RawOrigin::Signed(poster), id, 0);

        assert_eq!(Bounties::<T>::get(id).unwrap().state, BountyState::Completed);
    }

    #[benchmark]
    fn withdraw_submission() {
        let poster: T::AccountId = account("poster", 0, 0);
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let solver: T::AccountId = whitelisted_caller();
        fund::<T>(&solver);
        TaskBounty::<T>::submit(RawOrigin::Signed(solver.clone()).into(), id, Some(1u64), None)
            .expect("submit");

        #[extrinsic_call]
        withdraw_submission(RawOrigin::Signed(solver), id, 0);

        let sub = Submissions::<T>::get(id, 0).unwrap();
        assert_eq!(sub.state, SubmissionState::Withdrawn);
    }

    #[benchmark]
    fn cancel_bounty() {
        let poster: T::AccountId = whitelisted_caller();
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);

        #[extrinsic_call]
        cancel_bounty(RawOrigin::Signed(poster), id);

        assert_eq!(Bounties::<T>::get(id).unwrap().state, BountyState::Cancelled);
    }

    #[benchmark]
    fn open_dispute() {
        let poster: T::AccountId = whitelisted_caller();
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let solver: T::AccountId = account("solver", 0, 0);
        fund::<T>(&solver);
        TaskBounty::<T>::submit(RawOrigin::Signed(solver).into(), id, Some(1u64), None)
            .expect("submit deliverable");

        #[extrinsic_call]
        open_dispute(RawOrigin::Signed(poster), id, 0);

        assert_eq!(Bounties::<T>::get(id).unwrap().state, BountyState::Disputed);
    }

    #[benchmark]
    fn expire_bounty() {
        let poster: T::AccountId = account("poster", 0, 0);
        let id = setup_bounty::<T>(&poster, BountyKind::Single, 1);
        let caller: T::AccountId = whitelisted_caller();
        let deadline = Bounties::<T>::get(id).unwrap().deadline;
        frame_system::Pallet::<T>::set_block_number(deadline);

        #[extrinsic_call]
        expire_bounty(RawOrigin::Signed(caller), id);

        assert_eq!(Bounties::<T>::get(id).unwrap().state, BountyState::Refunded);
    }

    impl_benchmark_test_suite!(TaskBounty, crate::mock::new_test_ext(), crate::mock::Test);
}
