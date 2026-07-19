//! Benchmarks for `pallet-msg-identity`.
//! `pallet-msg-identity` 的基准测试。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::types::{X25519Pub, STACK_DR};
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;

/// EN: Fund `who` so it can cover the device deposit. CN: 给 `who` 充值以覆盖设备押金。
fn fund<T: Config>(who: &T::AccountId) {
    let unit = T::DeviceDeposit::get();
    let mut bal = unit;
    let mut i = 0u32;
    while i < 1_000 {
        bal = bal.saturating_add(unit);
        i = i.saturating_add(1);
    }
    let _ = T::Currency::make_free_balance_be(who, bal);
}

/// EN: Deterministic IK + its self-certifying device id. CN: 确定性 IK + 自证设备 id。
fn ik_dev(seed: u8) -> (X25519Pub, DeviceId) {
    let ik = [seed; 32];
    (ik, sp_io::hashing::blake2_128(&ik))
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_device() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let (ik, dev) = ik_dev(1);
        #[extrinsic_call]
        register_device(RawOrigin::Signed(caller.clone()), dev, ik, [0u8; 64]);
        assert!(DeviceIdentities::<T>::contains_key(&caller, dev));
    }

    #[benchmark]
    fn set_signed_prekey() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let (ik, dev) = ik_dev(2);
        Pallet::<T>::register_device(RawOrigin::Signed(caller.clone()).into(), dev, ik, [0u8; 64])
            .unwrap();
        let (spk, _) = ik_dev(3);
        #[extrinsic_call]
        set_signed_prekey(
            RawOrigin::Signed(caller.clone()),
            dev,
            spk,
            [0u8; 64],
            0u32.into(),
        );
        assert!(DeviceSignedPreKeys::<T>::contains_key(&caller, dev));
    }

    #[benchmark]
    fn set_opk_root() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let (ik, dev) = ik_dev(4);
        Pallet::<T>::register_device(RawOrigin::Signed(caller.clone()).into(), dev, ik, [0u8; 64])
            .unwrap();
        #[extrinsic_call]
        set_opk_root(RawOrigin::Signed(caller.clone()), dev, [1u8; 32], 100);
        assert!(DeviceOpkRoots::<T>::contains_key(&caller, dev));
    }

    #[benchmark]
    fn bump_prekey_epoch() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let (ik, dev) = ik_dev(5);
        Pallet::<T>::register_device(RawOrigin::Signed(caller.clone()).into(), dev, ik, [0u8; 64])
            .unwrap();
        #[extrinsic_call]
        bump_prekey_epoch(RawOrigin::Signed(caller.clone()), dev);
        assert_eq!(
            Pallet::<T>::device_ik(&caller, dev).map(|(_, e)| e),
            Some(1)
        );
    }

    #[benchmark]
    fn unregister_device() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let (ik, dev) = ik_dev(6);
        Pallet::<T>::register_device(RawOrigin::Signed(caller.clone()).into(), dev, ik, [0u8; 64])
            .unwrap();
        #[extrinsic_call]
        unregister_device(RawOrigin::Signed(caller.clone()), dev);
        assert!(!DeviceIdentities::<T>::contains_key(&caller, dev));
    }

    #[benchmark]
    fn set_stack_caps() {
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        set_stack_caps(RawOrigin::Signed(caller.clone()), STACK_DR, 1);
        assert_eq!(Pallet::<T>::stack_caps(&caller), Some((STACK_DR, 1)));
    }

    #[benchmark]
    fn force_unregister_device() {
        let owner: T::AccountId = account("owner", 0, 0);
        fund::<T>(&owner);
        let (ik, dev) = ik_dev(7);
        Pallet::<T>::register_device(RawOrigin::Signed(owner.clone()).into(), dev, ik, [0u8; 64])
            .unwrap();
        #[extrinsic_call]
        force_unregister_device(RawOrigin::Root, owner.clone(), dev);
        assert!(!DeviceIdentities::<T>::contains_key(&owner, dev));
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
