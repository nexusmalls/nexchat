//! Benchmarks for `pallet-chat-inbox`.
//! `pallet-chat-inbox` 的基准测试。
//!
//! Run via the node benchmark harness to generate real `WeightInfo`.
//! 通过节点基准框架运行以生成真实 `WeightInfo`。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::types::{ContactTag, InboxId};
use crate::Pallet as ChatInbox;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;

/// EN: Fund `who` well above the inbox deposit so reserve succeeds.
/// CN: 给 `who` 充值远超信箱押金，使预留成功。
fn fund<T: Config>(who: &T::AccountId) {
    let amount = T::InboxDeposit::get().saturating_mul(1_000u32.into());
    let _ = T::Currency::deposit_creating(who, amount);
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_inbox() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let id: InboxId = [7u8; 32];
        #[extrinsic_call]
        register_inbox(RawOrigin::Signed(caller.clone()), id);
        assert!(Inboxes::<T>::contains_key(id));
    }

    #[benchmark]
    fn bump_epoch() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let id: InboxId = [7u8; 32];
        ChatInbox::<T>::register_inbox(RawOrigin::Signed(caller.clone()).into(), id)
            .expect("register");
        #[extrinsic_call]
        bump_epoch(RawOrigin::Signed(caller.clone()), id);
        assert_eq!(ChatInbox::<T>::inbox_epoch(id), Some(1));
    }

    #[benchmark]
    fn revoke_tag() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let id: InboxId = [7u8; 32];
        let tag: ContactTag = [9u8; 32];
        ChatInbox::<T>::register_inbox(RawOrigin::Signed(caller.clone()).into(), id)
            .expect("register");
        #[extrinsic_call]
        revoke_tag(RawOrigin::Signed(caller.clone()), id, tag);
        assert!(ChatInbox::<T>::is_tag_revoked(id, tag));
    }

    #[benchmark]
    fn deregister_inbox() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let id: InboxId = [7u8; 32];
        ChatInbox::<T>::register_inbox(RawOrigin::Signed(caller.clone()).into(), id)
            .expect("register");
        #[extrinsic_call]
        deregister_inbox(RawOrigin::Signed(caller.clone()), id);
        assert!(!Inboxes::<T>::contains_key(id));
    }

    impl_benchmark_test_suite!(ChatInbox, crate::mock::new_test_ext(), crate::mock::Test);
}
