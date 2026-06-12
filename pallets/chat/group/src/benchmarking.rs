//! Benchmarks for `pallet-chat-group`.
//! `pallet-chat-group` 的基准测试。
//!
//! Run via the node benchmark harness to generate real `WeightInfo`.
//! 通过节点基准框架运行以生成真实 `WeightInfo`。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as ChatGroup;
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, EnsureOrigin, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::Saturating;
use sp_std::vec;
use sp_std::vec::Vec;

/// Fund an account well above all deposits. / 给账户充值远超各项押金。
fn fund<T: Config>(who: &T::AccountId) {
    let unit = T::GroupDeposit::get();
    let mut bal = unit;
    let mut i = 0u32;
    while i < 1_000 {
        bal = bal.saturating_add(unit);
        i = i.saturating_add(1);
    }
    let _ = T::Currency::make_free_balance_be(who, bal);
}

/// Create a group owned by `owner`, returning its id. / 由 `owner` 建群并返回 id。
fn new_group<T: Config>(owner: &T::AccountId, is_public: bool) -> GroupId {
    fund::<T>(owner);
    let gid = NextGroupId::<T>::get();
    ChatGroup::<T>::create_group(
        RawOrigin::Signed(owner.clone()).into(),
        b"cid".to_vec(),
        1,
        is_public,
        [0u8; 32],
        [0u8; 32],
    )
    .expect("create_group in benchmark setup");
    gid
}

/// Build a single-member add delta. / 构造单成员 Add delta。
fn add_delta<T: Config>(member: &T::AccountId) -> MemberDelta<T> {
    MemberDelta {
        added: [member.clone()].to_vec().try_into().expect("1 <= bound"),
        removed: Default::default(),
    }
}

/// Welcome list for one added member. / 单成员的 Welcome 列表。
fn welcomes_for<T: Config>(member: &T::AccountId) -> Vec<(T::AccountId, Vec<u8>)> {
    let mut w = Vec::new();
    w.push((member.clone(), [1u8; 8].to_vec()));
    w
}

/// EN: Expand a freshly created group (1 member) to 3 members in one commit,
/// satisfying the on-chain `TwoMemberGroupForbidden` invariant.
/// CN: 将新建群（1 人）一次性扩至 3 人，满足链上 `TwoMemberGroupForbidden` 不变量。
fn seed_group_to_three<T: Config>(owner: &T::AccountId, gid: GroupId) {
    if GroupMls::<T>::get(gid).map(|g| g.member_count).unwrap_or(0) >= 3 {
        return;
    }
    let m1: T::AccountId = account("seed", 0, 0);
    let m2: T::AccountId = account("seed", 1, 0);
    make_joinable::<T>(&m1);
    make_joinable::<T>(&m2);
    MlsActionRate::<T>::remove(owner);
    let epoch = GroupMls::<T>::get(gid).expect("group exists").epoch;
    let welcomes = vec![
        (m1.clone(), [1u8; 8].to_vec()),
        (m2.clone(), [1u8; 8].to_vec()),
    ];
    let delta = MemberDelta::<T> {
        added: [m1, m2].to_vec().try_into().expect("2 <= bound"),
        removed: Default::default(),
    };
    ChatGroup::<T>::commit(
        RawOrigin::Signed(owner.clone()).into(),
        gid,
        epoch,
        [1u8; 16].to_vec(),
        [1u8; 32],
        [2u8; 32],
        b"cid-seed".to_vec(),
        welcomes,
        delta,
    )
    .expect("seed group to three");
}

/// EN: Make `member` addable to a public group by funding it and publishing a
/// KeyPackage (audit U3 opt-in gate). Idempotent. CN: 为被加成员充值并发布 KeyPackage，
/// 使其可被加入公开群（审计 U3 的同意闸门）。幂等。
fn make_joinable<T: Config>(member: &T::AccountId) {
    fund::<T>(member);
    if KeyPackageCount::<T>::get(member) == 0 {
        ChatGroup::<T>::publish_key_package(
            RawOrigin::Signed(member.clone()).into(),
            [7u8; 32].to_vec(),
        )
        .expect("publish keypackage for addee");
    }
}

/// Add `member` to `gid` via an owner `commit`. / 由群主 `commit` 把 `member` 加入群。
///
/// EN: Reads the group's current epoch (each commit advances it) and clears the
/// owner's MLS rate-limit state so repeated setup commits don't hit the
/// per-window cap. CN: 读取群当前 epoch（每次 commit 都会推进），并清除群主的
/// MLS 限频状态，避免基准 setup 中的多次 commit 触发窗口限频。
fn add_member<T: Config>(owner: &T::AccountId, gid: GroupId, member: &T::AccountId) {
    seed_group_to_three::<T>(owner, gid);
    make_joinable::<T>(member);
    MlsActionRate::<T>::remove(owner);
    let epoch = GroupMls::<T>::get(gid).expect("group exists").epoch;
    ChatGroup::<T>::commit(
        RawOrigin::Signed(owner.clone()).into(),
        gid,
        epoch,
        [1u8; 16].to_vec(),
        [1u8; 32],
        [2u8; 32],
        b"cid2".to_vec(),
        welcomes_for::<T>(member),
        add_delta::<T>(member),
    )
    .expect("commit add member");
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn publish_key_package() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        #[extrinsic_call]
        publish_key_package(RawOrigin::Signed(caller.clone()), [1u8; 32].to_vec());
        assert!(KeyPackages::<T>::contains_key(&caller, 0));
    }

    #[benchmark]
    fn revoke_key_package() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        ChatGroup::<T>::publish_key_package(
            RawOrigin::Signed(caller.clone()).into(),
            [1u8; 32].to_vec(),
        )
        .expect("publish");
        #[extrinsic_call]
        revoke_key_package(RawOrigin::Signed(caller.clone()), 0);
        assert!(!KeyPackages::<T>::contains_key(&caller, 0));
    }

    #[benchmark]
    fn create_group() {
        let caller: T::AccountId = whitelisted_caller();
        fund::<T>(&caller);
        let gid = NextGroupId::<T>::get();
        #[extrinsic_call]
        create_group(RawOrigin::Signed(caller), b"cid".to_vec(), 1, true, [0u8; 32], [0u8; 32]);
        assert!(GroupMls::<T>::contains_key(gid));
    }

    /// EN: `commit` weight scales with the membership delta. Bench across `a` added
    /// and `r` removed members so the generated formula captures both slopes. Ranges
    /// stay within the mock `MaxGroupMembers` (owner + members ≤ bound).
    /// CN: `commit` 权重随成员增减规模变化。对 `a` 个新增、`r` 个移除成员做基准，使生成的
    /// 权重公式同时覆盖两个斜率。取值范围受 mock `MaxGroupMembers` 约束（群主 + 成员 ≤ 上限）。
    #[benchmark]
    fn commit(a: Linear<0, 4>, r: Linear<0, 3>) {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        seed_group_to_three::<T>(&owner, gid);

        // 预先加入将被本次 commit 移除的成员 / pre-add members the bench commit removes
        let mut to_remove: Vec<T::AccountId> = Vec::new();
        for i in 0..r {
            let m: T::AccountId = account("rem", i, 0);
            add_member::<T>(&owner, gid, &m);
            to_remove.push(m);
        }

        // 准备将被加入的成员（已可加入、尚非成员）/ prepare addable, not-yet-member accounts
        let mut to_add: Vec<T::AccountId> = Vec::new();
        let mut welcomes: Vec<(T::AccountId, Vec<u8>)> = Vec::new();
        for i in 0..a {
            let m: T::AccountId = account("add", i, 0);
            make_joinable::<T>(&m);
            welcomes.push((m.clone(), [1u8; 8].to_vec()));
            to_add.push(m);
        }

        let epoch = GroupMls::<T>::get(gid).expect("group exists").epoch;
        let delta = MemberDelta::<T> {
            added: to_add.clone().try_into().expect("a <= bound"),
            removed: to_remove.clone().try_into().expect("r <= bound"),
        };

        // 清除 setup 累积的限频状态，使被测 commit 在新窗口内执行 / clear rate-limit
        // state accumulated during setup so the measured commit runs in a fresh window.
        MlsActionRate::<T>::remove(&owner);

        #[extrinsic_call]
        commit(
            RawOrigin::Signed(owner),
            gid,
            epoch,
            [1u8; 16].to_vec(),
            [1u8; 32],
            [2u8; 32],
            b"cid2".to_vec(),
            welcomes,
            delta,
        );

        for m in to_add.iter() {
            assert!(GroupMembers::<T>::contains_key(gid, m));
        }
        for m in to_remove.iter() {
            assert!(!GroupMembers::<T>::contains_key(gid, m));
        }
    }

    #[benchmark]
    fn claim_welcome() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let member: T::AccountId = account("member", 0, 0);
        let member2: T::AccountId = account("member", 1, 0);
        make_joinable::<T>(&member);
        make_joinable::<T>(&member2);
        ChatGroup::<T>::commit(
            RawOrigin::Signed(owner).into(),
            gid,
            0,
            [1u8; 16].to_vec(),
            [1u8; 32],
            [2u8; 32],
            b"cid2".to_vec(),
            vec![
                (member.clone(), [1u8; 8].to_vec()),
                (member2.clone(), [2u8; 8].to_vec()),
            ],
            MemberDelta::<T> {
                added: [member.clone(), member2]
                    .to_vec()
                    .try_into()
                    .expect("2 <= bound"),
                removed: Default::default(),
            },
        )
        .expect("commit add");
        #[extrinsic_call]
        claim_welcome(RawOrigin::Signed(member.clone()), gid);
        assert!(!WelcomeMailbox::<T>::contains_key(gid, &member));
    }

    #[benchmark]
    fn disband_group() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        #[extrinsic_call]
        disband_group(RawOrigin::Signed(owner), gid);
        assert!(!GroupMls::<T>::contains_key(gid));
    }

    #[benchmark]
    fn anchor_message_digest() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        #[extrinsic_call]
        anchor_message_digest(RawOrigin::Signed(owner), gid, 0, [7u8; 32], 0);
        assert!(MessageDigestAnchor::<T>::contains_key(gid, 0));
    }

    #[benchmark]
    fn request_join() {
        let owner: T::AccountId = account("owner", 0, 0);
        let gid = new_group::<T>(&owner, false);
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        request_join(RawOrigin::Signed(caller.clone()), gid);
        assert!(JoinRequests::<T>::contains_key(gid, &caller));
    }

    #[benchmark]
    fn cancel_join_request() {
        let owner: T::AccountId = account("owner", 0, 0);
        let gid = new_group::<T>(&owner, false);
        let caller: T::AccountId = whitelisted_caller();
        ChatGroup::<T>::request_join(RawOrigin::Signed(caller.clone()).into(), gid).expect("request");
        #[extrinsic_call]
        cancel_join_request(RawOrigin::Signed(caller.clone()), gid);
        assert!(!JoinRequests::<T>::contains_key(gid, &caller));
    }

    #[benchmark]
    fn approve_join() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, false);
        let applicant: T::AccountId = account("applicant", 0, 0);
        ChatGroup::<T>::request_join(RawOrigin::Signed(applicant.clone()).into(), gid)
            .expect("request");
        #[extrinsic_call]
        approve_join(RawOrigin::Signed(owner), gid, applicant.clone());
        assert!(JoinApprovals::<T>::contains_key(gid, &applicant));
    }

    #[benchmark]
    fn transfer_ownership() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let member: T::AccountId = account("member", 0, 0);
        add_member::<T>(&owner, gid, &member);
        #[extrinsic_call]
        transfer_ownership(RawOrigin::Signed(owner), gid, member.clone());
        assert_eq!(GroupMls::<T>::get(gid).unwrap().admin, member);
    }

    #[benchmark]
    fn set_admin() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let member: T::AccountId = account("member", 0, 0);
        add_member::<T>(&owner, gid, &member);
        #[extrinsic_call]
        set_admin(RawOrigin::Signed(owner), gid, member.clone(), true);
        assert_eq!(GroupMembers::<T>::get(gid, &member).unwrap().role, MemberRole::Admin);
    }

    #[benchmark]
    fn set_group_profile() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let mut name = Vec::new();
        name.resize(T::MaxGroupNameLen::get() as usize, b'x');
        let mut ann = Vec::new();
        ann.resize(T::MaxGroupAnnouncementLen::get() as usize, b'y');
        let mut cid = Vec::new();
        cid.resize(T::MaxCidLen::get() as usize, b'z');
        #[extrinsic_call]
        set_group_profile(RawOrigin::Signed(owner), gid, Some(name), Some(cid), Some(ann));
        assert!(GroupProfiles::<T>::contains_key(gid));
    }

    #[benchmark]
    fn set_group_nickname() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let mut nick = Vec::new();
        nick.resize(T::MaxGroupNicknameLen::get() as usize, b'n');
        #[extrinsic_call]
        set_group_nickname(RawOrigin::Signed(owner.clone()), gid, Some(nick));
        assert!(GroupNicknames::<T>::contains_key(gid, &owner));
    }

    #[benchmark]
    fn ban_member() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let target: T::AccountId = account("target", 0, 0);
        #[extrinsic_call]
        ban_member(RawOrigin::Signed(owner), gid, target.clone());
        assert!(Banned::<T>::contains_key(gid, &target));
    }

    #[benchmark]
    fn unban_member() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let target: T::AccountId = account("target", 0, 0);
        ChatGroup::<T>::ban_member(RawOrigin::Signed(owner.clone()).into(), gid, target.clone())
            .expect("ban");
        #[extrinsic_call]
        unban_member(RawOrigin::Signed(owner), gid, target.clone());
        assert!(!Banned::<T>::contains_key(gid, &target));
    }

    #[benchmark]
    fn set_member_mute() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        let member: T::AccountId = account("member", 0, 0);
        add_member::<T>(&owner, gid, &member);
        // 到期时间须严格大于当前块；取足够大的未来值以稳健通过校验。
        // Expiry must be strictly after `now`; use a comfortably future value.
        let until = frame_system::Pallet::<T>::block_number().saturating_add(1_000u32.into());
        #[extrinsic_call]
        set_member_mute(RawOrigin::Signed(owner), gid, member.clone(), Some(until));
        assert!(MemberMutedUntil::<T>::contains_key(gid, &member));
    }

    #[benchmark]
    fn set_group_mute_all() {
        let owner: T::AccountId = whitelisted_caller();
        let gid = new_group::<T>(&owner, true);
        #[extrinsic_call]
        set_group_mute_all(RawOrigin::Signed(owner), gid, true);
        assert!(GroupMutedAll::<T>::get(gid));
    }

    #[benchmark]
    fn force_disband_group() -> Result<(), BenchmarkError> {
        let owner: T::AccountId = account("owner", 0, 0);
        let gid = new_group::<T>(&owner, true);
        let origin = T::GovernanceOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Weightless)?;
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, gid);
        assert!(!GroupMls::<T>::contains_key(gid));
        Ok(())
    }

    #[benchmark]
    fn set_group_frozen() -> Result<(), BenchmarkError> {
        let owner: T::AccountId = account("owner", 0, 0);
        let gid = new_group::<T>(&owner, true);
        let origin = T::GovernanceOrigin::try_successful_origin()
            .map_err(|_| BenchmarkError::Weightless)?;
        #[extrinsic_call]
        _(origin as T::RuntimeOrigin, gid, true);
        assert!(GroupFrozen::<T>::contains_key(gid));
        Ok(())
    }

    impl_benchmark_test_suite!(ChatGroup, crate::mock::new_test_ext(), crate::mock::Test);
}
