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

/// EN: Establish a bidirectional friendship between `a` and `b` via the
/// request → accept handshake (benchmark setup helper).
/// CN: 经「申请 → 同意」握手在 `a`、`b` 之间建立双向好友（基准 setup 辅助）。
fn befriend<T: Config>(a: &T::AccountId, b: &T::AccountId) {
    ChatPermission::<T>::request_friend(RawOrigin::Signed(a.clone()).into(), b.clone(), None)
        .expect("request_friend in setup");
    ChatPermission::<T>::accept_friend(RawOrigin::Signed(b.clone()).into(), a.clone())
        .expect("accept_friend in setup");
}

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

    #[benchmark]
    fn block_user() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        #[extrinsic_call]
        block_user(RawOrigin::Signed(caller.clone()), target.clone());
        assert!(PrivacySettingsOf::<T>::get(&caller).block_list.contains(&target));
    }

    #[benchmark]
    fn unblock_user() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        ChatPermission::<T>::block_user(RawOrigin::Signed(caller.clone()).into(), target.clone())
            .expect("block");
        #[extrinsic_call]
        unblock_user(RawOrigin::Signed(caller.clone()), target.clone());
        assert!(!PrivacySettingsOf::<T>::get(&caller).block_list.contains(&target));
    }

    #[benchmark]
    fn add_to_whitelist() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        #[extrinsic_call]
        add_to_whitelist(RawOrigin::Signed(caller.clone()), target.clone());
        assert!(PrivacySettingsOf::<T>::get(&caller).whitelist.contains(&target));
    }

    #[benchmark]
    fn remove_from_whitelist() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        ChatPermission::<T>::add_to_whitelist(
            RawOrigin::Signed(caller.clone()).into(),
            target.clone(),
        )
        .expect("add");
        #[extrinsic_call]
        remove_from_whitelist(RawOrigin::Signed(caller.clone()), target.clone());
        assert!(!PrivacySettingsOf::<T>::get(&caller).whitelist.contains(&target));
    }

    #[benchmark]
    fn request_friend() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        let msg = vec![b'x'; T::MaxFriendRequestMsgLen::get() as usize];
        #[extrinsic_call]
        request_friend(RawOrigin::Signed(caller.clone()), target.clone(), Some(msg));
        assert!(FriendRequests::<T>::get(&target, &caller).is_some());
    }

    #[benchmark]
    fn accept_friend() {
        let caller: T::AccountId = whitelisted_caller();
        let requester: T::AccountId = account("requester", 0, 0);
        ChatPermission::<T>::request_friend(
            RawOrigin::Signed(requester.clone()).into(),
            caller.clone(),
            None,
        )
        .expect("request");
        #[extrinsic_call]
        accept_friend(RawOrigin::Signed(caller.clone()), requester.clone());
        assert!(Friendships::<T>::get(&caller, &requester).is_some());
    }

    #[benchmark]
    fn reject_friend() {
        let caller: T::AccountId = whitelisted_caller();
        let requester: T::AccountId = account("requester", 0, 0);
        ChatPermission::<T>::request_friend(
            RawOrigin::Signed(requester.clone()).into(),
            caller.clone(),
            None,
        )
        .expect("request");
        #[extrinsic_call]
        reject_friend(RawOrigin::Signed(caller.clone()), requester.clone());
        assert!(FriendRequests::<T>::get(&caller, &requester).is_none());
    }

    #[benchmark]
    fn cancel_friend_request() {
        let caller: T::AccountId = whitelisted_caller();
        let target: T::AccountId = account("target", 0, 0);
        ChatPermission::<T>::request_friend(
            RawOrigin::Signed(caller.clone()).into(),
            target.clone(),
            None,
        )
        .expect("request");
        #[extrinsic_call]
        cancel_friend_request(RawOrigin::Signed(caller.clone()), target.clone());
        assert!(FriendRequests::<T>::get(&target, &caller).is_none());
    }

    #[benchmark]
    fn remove_friend() {
        let caller: T::AccountId = whitelisted_caller();
        let friend: T::AccountId = account("friend", 0, 0);
        befriend::<T>(&caller, &friend);
        #[extrinsic_call]
        remove_friend(RawOrigin::Signed(caller.clone()), friend.clone());
        assert!(Friendships::<T>::get(&caller, &friend).is_none());
    }

    #[benchmark]
    fn set_friend_meta() {
        let caller: T::AccountId = whitelisted_caller();
        let friend: T::AccountId = account("friend", 0, 0);
        befriend::<T>(&caller, &friend);
        let remark = vec![b'r'; T::MaxFriendRemarkLen::get() as usize];
        let group = vec![b'g'; T::MaxFriendGroupLen::get() as usize];
        #[extrinsic_call]
        set_friend_meta(
            RawOrigin::Signed(caller.clone()),
            friend.clone(),
            Some(remark),
            Some(group),
        );
        assert!(FriendRemark::<T>::contains_key(&caller, &friend));
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
