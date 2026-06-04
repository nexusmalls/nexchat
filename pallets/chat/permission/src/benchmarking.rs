//! Benchmarks for `pallet-chat-permission`.
//! `pallet-chat-permission` 的基准测试。
//!
//! Run via the node benchmark harness to generate real `WeightInfo`.
//! 通过节点基准框架运行以生成真实 `WeightInfo`。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as ChatPermission;
use frame_benchmarking::v2::*;
use frame_support::traits::{ConstU32, EnsureOrigin, Get};
use frame_support::BoundedVec;
use frame_system::RawOrigin;
use sp_std::vec;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn set_permission_level() {
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        set_permission_level(RawOrigin::Signed(caller.clone()), ChatPermissionLevel::Open);
        assert_eq!(
            PrivacySettingsOf::<T>::get(&caller).permission_level,
            ChatPermissionLevel::Open
        );
    }

    #[benchmark]
    fn set_rejected_scene_types() {
        let caller: T::AccountId = whitelisted_caller();
        let types: BoundedVec<SceneType, ConstU32<10>> =
            vec![SceneType::Order, SceneType::Group].try_into().expect("within bound");
        #[extrinsic_call]
        set_rejected_scene_types(RawOrigin::Signed(caller.clone()), types);
        assert_eq!(
            PrivacySettingsOf::<T>::get(&caller).rejected_scene_types.len(),
            2
        );
    }

    // NOTE / 注意（审计 P1）：`block_user` / `unblock_user` / `add_to_whitelist` /
    // `remove_from_whitelist` 基准已随对应 extrinsic 移除（链上黑/白名单去明文）。
    // Benches removed along with the extrinsics (on-chain block/whitelist dropped).

    #[benchmark]
    fn bump_capability_epoch() {
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        bump_capability_epoch(RawOrigin::Signed(caller.clone()));
        assert_eq!(CapabilityEpoch::<T>::get(&caller), 1);
    }

    #[benchmark]
    fn force_mute_account() -> Result<(), BenchmarkError> {
        let target: T::AccountId = account("target", 0, 0);
        let origin = T::GovernanceOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Weightless)?;
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, target.clone(), None);
        assert!(MutedAccounts::<T>::contains_key(&target));
        Ok(())
    }

    #[benchmark]
    fn force_unmute_account() -> Result<(), BenchmarkError> {
        let target: T::AccountId = account("target", 0, 0);
        let origin = T::GovernanceOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Weightless)?;
        ChatPermission::<T>::force_mute_account(origin.clone(), target.clone(), None)
            .expect("mute");
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, target.clone());
        assert!(!MutedAccounts::<T>::contains_key(&target));
        Ok(())
    }

    #[benchmark]
    fn report() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        let cid = vec![b'c'; T::MaxReportCidLen::get() as usize];
        #[extrinsic_call]
        report(RawOrigin::Signed(caller.clone()), ReportTarget::Account(target), cid);
        assert!(Reports::<T>::contains_key(0));
    }

    #[benchmark]
    fn resolve_report() -> Result<(), BenchmarkError> {
        let reporter: T::AccountId = account("reporter", 0, 0);
        let target: T::AccountId = account("target", 0, 0);
        let cid = vec![b'c'; T::MaxReportCidLen::get() as usize];
        ChatPermission::<T>::report(
            RawOrigin::Signed(reporter).into(),
            ReportTarget::Account(target),
            cid,
        )
        .expect("report");
        let origin = T::GovernanceOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Weightless)?;
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, 0, true);
        assert!(!Reports::<T>::contains_key(0));
        Ok(())
    }

    impl_benchmark_test_suite!(ChatPermission, crate::mock::new_test_ext(), crate::mock::Test);
}
