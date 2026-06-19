//! Unit tests for `pallet-chat-group` (P0 chain skeleton).
//! `pallet-chat-group`（P0 链骨架）单元测试。

use crate::{
    mock::*, Banned, Error, Event, GroupMembers, GroupMls, GroupMutedAll, GroupNicknames,
    GroupProfiles, JoinApprovals, JoinRequests, MemberDelta, MemberMutedUntil, MemberRole,
    PendingJoinCount, UserGroups, WelcomeMailbox,
};
use frame_support::{assert_noop, assert_ok, traits::ReservableCurrency, BoundedVec};

const OWNER: u64 = 1;
const ALICE: u64 = 2;
const BOB: u64 = 3;
const CAROL: u64 = 4;

fn delta(added: Vec<u64>, removed: Vec<u64>) -> MemberDelta<Test> {
    MemberDelta {
        added: BoundedVec::try_from(added).unwrap(),
        removed: BoundedVec::try_from(removed).unwrap(),
    }
}

/// Welcome 向量，与 `added` 一一对应（P0：commit 强制匹配）。
fn welcomes(members: &[u64]) -> Vec<(u64, Vec<u8>)> {
    members.iter().map(|&m| (m, vec![m as u8, 1])).collect()
}

/// 提交成员变更；自动构造与 `added` 匹配的 welcomes。
fn commit_at(gid: u64, who: u64, epoch: u64, added: Vec<u64>, removed: Vec<u64>) {
    assert_ok!(ChatGroup::commit(
        RuntimeOrigin::signed(who),
        gid,
        epoch,
        vec![],
        [0u8; 32],
        [0u8; 32],
        b"cid".to_vec(),
        welcomes(&added),
        delta(added, removed),
    ));
}

/// 将群从 1 人（群主）扩到 3 人（+ALICE+BOB），满足「禁止 2 人群」不变量。
fn seed_group(gid: u64) {
    let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
    commit_at(gid, OWNER, epoch, vec![ALICE, BOB], vec![]);
}

/// 在已有 ≥3 人的群里再加一名成员。
fn add_member(gid: u64, who: u64) {
    let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
    commit_at(gid, OWNER, epoch, vec![who], vec![]);
}

fn create_with(owner: u64, is_public: bool) -> u64 {
    let gid = crate::NextGroupId::<Test>::get();
    assert_ok!(ChatGroup::create_group(
        RuntimeOrigin::signed(owner),
        b"cid-init".to_vec(),
        1, // cipher_suite
        is_public,
        [1u8; 32],
        [2u8; 32],
    ));
    // 审计 U3：公开群加人要求被加成员已发布 KeyPackage（同意被加入 + MLS 必需）。
    // 测试夹具为候选成员池预置一个 KeyPackage（幂等），使既有"加人"用例无需逐处改写；
    // 池外账户（如 15）仍可用于验证 AddeeNotJoinable 拒绝路径。
    // Audit U3: adding to a public group requires the addee to have published a
    // KeyPackage (opt-in consent + MLS necessity). The fixture seeds one KeyPackage
    // (idempotently) for the candidate-member pool so existing "add" cases need no
    // per-site edits; accounts outside the pool (e.g. 15) still exercise the
    // AddeeNotJoinable rejection path.
    for who in 2..=12u64 {
        if crate::KeyPackageCount::<Test>::get(who) == 0 {
            assert_ok!(ChatGroup::publish_key_package(
                RuntimeOrigin::signed(who),
                vec![who as u8]
            ));
        }
    }
    gid
}

fn create(owner: u64) -> u64 {
    create_with(owner, true)
}

fn reserved(who: u64) -> u128 {
    <Balances as ReservableCurrency<u64>>::reserved_balance(&who)
}

#[test]
fn publish_and_revoke_key_package() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(ALICE), vec![1, 2, 3]));
        assert_eq!(crate::KeyPackageCount::<Test>::get(ALICE), 1);
        assert!(crate::KeyPackages::<Test>::contains_key(ALICE, 0));
        System::assert_has_event(Event::KeyPackagePublished { who: ALICE, id: 0 }.into());

        assert_ok!(ChatGroup::revoke_key_package(RuntimeOrigin::signed(ALICE), 0));
        assert_eq!(crate::KeyPackageCount::<Test>::get(ALICE), 0);
        assert!(!crate::KeyPackages::<Test>::contains_key(ALICE, 0));
    });
}

#[test]
fn revoke_missing_key_package_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ChatGroup::revoke_key_package(RuntimeOrigin::signed(ALICE), 7),
            Error::<Test>::KeyPackageNotFound
        );
    });
}

#[test]
fn key_package_limit_enforced() {
    new_test_ext().execute_with(|| {
        for _ in 0..4 {
            assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(ALICE), vec![0]));
        }
        assert_noop!(
            ChatGroup::publish_key_package(RuntimeOrigin::signed(ALICE), vec![0]),
            Error::<Test>::TooManyKeyPackages
        );
    });
}

#[test]
fn create_group_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.epoch, 0);
        assert_eq!(state.admin, OWNER);
        assert_eq!(state.member_count, 1);
        let m = GroupMembers::<Test>::get(gid, OWNER).unwrap();
        assert_eq!(m.role, MemberRole::Owner);
        assert_eq!(UserGroups::<Test>::get(OWNER).to_vec(), vec![gid]);
        System::assert_has_event(Event::GroupCreated { group_id: gid, creator: OWNER, epoch: 0 }.into());
    });
}

#[test]
fn create_group_cooldown_enforced() {
    new_test_ext().execute_with(|| {
        let _ = create(OWNER);
        // 冷却期内再次建群被拒 / second create within cooldown rejected
        assert_noop!(
            ChatGroup::create_group(
                RuntimeOrigin::signed(OWNER),
                b"cid".to_vec(),
                1,
                true,
                [0u8; 32],
                [0u8; 32],
            ),
            Error::<Test>::CreationCooldown
        );
        // 过了冷却期后允许 / allowed after cooldown
        run_to_block(12);
        assert_ok!(ChatGroup::create_group(
            RuntimeOrigin::signed(OWNER),
            b"cid".to_vec(),
            1,
            true,
            [0u8; 32],
            [0u8; 32],
        ));
    });
}

#[test]
fn commit_adds_members_and_advances_epoch() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0, // expected_epoch
            vec![9, 9, 9],
            [3u8; 32],
            [4u8; 32],
            b"cid-1".to_vec(),
            vec![(ALICE, vec![1, 1]), (BOB, vec![2, 2])],
            delta(vec![ALICE, BOB], vec![]),
        ));

        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.epoch, 1);
        assert_eq!(state.member_count, 3);
        // 成员表 + UserGroups 同步 / member table + UserGroups synced
        assert!(GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(GroupMembers::<Test>::contains_key(gid, BOB));
        assert!(UserGroups::<Test>::get(ALICE).contains(&gid));
        assert!(UserGroups::<Test>::get(BOB).contains(&gid));
        // Welcome 投递 / welcomes delivered
        assert!(WelcomeMailbox::<Test>::contains_key(gid, ALICE));
        assert!(WelcomeMailbox::<Test>::contains_key(gid, BOB));
        // Commit 落入本 epoch 日志 / commit logged at new epoch
        assert!(crate::HandshakeLog::<Test>::contains_key(gid, 1));
        System::assert_has_event(Event::Committed { group_id: gid, epoch: 1, committer: OWNER }.into());
    });
}

#[test]
fn commit_with_stale_epoch_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // 当前 epoch 为 0，传 1 应被拒 / chain epoch is 0, expected 1 rejected
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                1,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE]),
                delta(vec![ALICE], vec![]),
            ),
            Error::<Test>::EpochStale
        );
    });
}

#[test]
fn non_admin_cannot_change_others() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        // ALICE（Member）尝试加 CAROL → 无权
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(ALICE),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[CAROL]),
                delta(vec![CAROL], vec![]),
            ),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn commit_removes_member_and_syncs_user_groups() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        add_member(gid, CAROL);
        assert!(UserGroups::<Test>::get(ALICE).contains(&gid));

        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        commit_at(gid, OWNER, epoch, vec![], vec![ALICE]);
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().member_count, 3);
    });
}

#[test]
fn admin_cannot_remove_owner() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(ALICE),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![OWNER]),
            ),
            Error::<Test>::BadMemberDelta
        );
    });
}

#[test]
fn group_full_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // MaxGroupMembers = 8；owner + 7 = 8 ok / fill to bound
        // 使用候选池内账户（已具 KeyPackage），以触达 GroupFull 而非 U3 闸门。
        // Use in-pool accounts (already have KeyPackages) so we hit GroupFull, not U3.
        commit_at(gid, OWNER, 0, vec![2, 3, 4, 5, 6, 7, 8], vec![]);
        // 第 9 个成员越界 / 9th member exceeds bound
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                1,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[9]),
                delta(vec![9], vec![]),
            ),
            Error::<Test>::GroupFull
        );
    });
}

#[test]
fn claim_welcome_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::claim_welcome(RuntimeOrigin::signed(ALICE), gid));
        assert!(!WelcomeMailbox::<Test>::contains_key(gid, ALICE));
        assert_noop!(
            ChatGroup::claim_welcome(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::WelcomeNotFound
        );
    });
}

/// 审计 U3：公开群不能把未发布 KeyPackage（未同意）的账户拉进来。
/// Audit U3: a public group cannot add an account that never opted in
/// (no published KeyPackage).
#[test]
fn public_add_requires_addee_keypackage() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[15u64, 16u64]),
                delta(vec![15u64, 16u64], vec![]),
            ),
            Error::<Test>::AddeeNotJoinable
        );

        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(15u64), vec![15]));
        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(16u64), vec![16]));
        commit_at(gid, OWNER, epoch, vec![15u64, 16u64], vec![]);
        assert!(GroupMembers::<Test>::contains_key(gid, 15u64));
        assert!(GroupMembers::<Test>::contains_key(gid, 16u64));
    });
}

#[test]
fn disband_cleans_everything() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        // 非群主不能解散 / non-owner cannot disband
        assert_noop!(
            ChatGroup::disband_group(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::NotGroupOwner
        );

        assert_ok!(ChatGroup::disband_group(RuntimeOrigin::signed(OWNER), gid));
        assert!(!GroupMls::<Test>::contains_key(gid));
        assert!(!GroupMembers::<Test>::contains_key(gid, OWNER));
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        // 所有成员 UserGroups 同步清理 / every member's UserGroups synced
        assert!(!UserGroups::<Test>::get(OWNER).contains(&gid));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
        assert!(!UserGroups::<Test>::get(BOB).contains(&gid));
        System::assert_has_event(Event::GroupDisbanded { group_id: gid }.into());
    });
}

#[test]
fn anchor_message_digest_optional_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::anchor_message_digest(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            [7u8; 32],
            0,
        ));
        assert!(crate::MessageDigestAnchor::<Test>::contains_key(gid, 0));
        // 非成员不能锚 / non-member cannot anchor
        assert_noop!(
            ChatGroup::anchor_message_digest(RuntimeOrigin::signed(CAROL), gid, 1, [0u8; 32], 0),
            Error::<Test>::NotMember
        );
    });
}

#[test]
fn mls_action_rate_limit_blocks_excess_anchors() {
    use frame_support::traits::Get;
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let max = <<Test as crate::Config>::MaxMlsActionsPerWindow as Get<u32>>::get();
        // 窗口内允许 max 次写入型 MLS 操作 / first `max` actions allowed in-window
        for seq in 0..max as u64 {
            assert_ok!(ChatGroup::anchor_message_digest(
                RuntimeOrigin::signed(OWNER),
                gid,
                seq,
                [1u8; 32],
                0,
            ));
        }
        // 第 max+1 次被限频 / the (max+1)-th is rate-limited
        assert_noop!(
            ChatGroup::anchor_message_digest(RuntimeOrigin::signed(OWNER), gid, max as u64, [1u8; 32], 0),
            Error::<Test>::RateLimited
        );
    });
}

#[test]
fn governance_freeze_blocks_writes_and_unfreeze_restores() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // 治理冻结 / governance freeze (Root)
        assert_ok!(ChatGroup::set_group_frozen(RuntimeOrigin::root(), gid, true));

        // 冻结期间禁止 anchor / commit / request_join
        assert_noop!(
            ChatGroup::anchor_message_digest(RuntimeOrigin::signed(OWNER), gid, 0, [1u8; 32], 0),
            Error::<Test>::GroupFrozen
        );
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE, BOB]),
                delta(vec![ALICE, BOB], vec![]),
            ),
            Error::<Test>::GroupFrozen
        );

        // 解冻后恢复 / unfreeze restores
        assert_ok!(ChatGroup::set_group_frozen(RuntimeOrigin::root(), gid, false));
        assert_ok!(ChatGroup::anchor_message_digest(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            [1u8; 32],
            0,
        ));
    });
}

#[test]
fn governance_force_disband_works_and_requires_governance() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // 非治理来源不可强制解散 / non-governance signed origin is rejected
        assert_noop!(
            ChatGroup::force_disband_group(RuntimeOrigin::signed(OWNER), gid),
            sp_runtime::DispatchError::BadOrigin
        );
        // Root 强制解散 / Root force-disbands
        assert_ok!(ChatGroup::force_disband_group(RuntimeOrigin::root(), gid));
        assert!(GroupMls::<Test>::get(gid).is_none());
        assert!(!UserGroups::<Test>::get(OWNER).contains(&gid));
    });
}

#[test]
fn mls_action_rate_limit_resets_after_window() {
    use frame_support::traits::Get;
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let max = <<Test as crate::Config>::MaxMlsActionsPerWindow as Get<u32>>::get();
        let window = <<Test as crate::Config>::MlsActionWindow as Get<u64>>::get();
        for seq in 0..max as u64 {
            assert_ok!(ChatGroup::anchor_message_digest(
                RuntimeOrigin::signed(OWNER),
                gid,
                seq,
                [1u8; 32],
                0,
            ));
        }
        // 推进超过窗口后配额恢复 / quota recovers once the window elapses
        run_to_block(window + 2);
        assert_ok!(ChatGroup::anchor_message_digest(
            RuntimeOrigin::signed(OWNER),
            gid,
            max as u64,
            [1u8; 32],
            0,
        ));
    });
}

// ===================== P1: 押金 / deposits =====================

#[test]
fn create_group_reserves_and_disband_refunds_deposit() {
    new_test_ext().execute_with(|| {
        assert_eq!(reserved(OWNER), 0);
        let gid = create(OWNER);
        assert_eq!(reserved(OWNER), 100); // GroupDeposit
        assert_ok!(ChatGroup::disband_group(RuntimeOrigin::signed(OWNER), gid));
        assert_eq!(reserved(OWNER), 0);
    });
}

#[test]
fn key_package_deposit_reserved_and_refunded() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(ALICE), vec![1]));
        assert_eq!(reserved(ALICE), 10); // KeyPackageDeposit
        assert_ok!(ChatGroup::revoke_key_package(RuntimeOrigin::signed(ALICE), 0));
        assert_eq!(reserved(ALICE), 0);
    });
}

// ===================== P1: 私群入群流程 / private join flow =====================

#[test]
fn private_group_join_flow_works() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(BOB), gid));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 2);

        assert_ok!(ChatGroup::approve_join(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert_ok!(ChatGroup::approve_join(RuntimeOrigin::signed(OWNER), gid, BOB));

        commit_at(gid, OWNER, 0, vec![ALICE, BOB], vec![]);
        assert!(GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(GroupMembers::<Test>::contains_key(gid, BOB));
        assert!(!JoinRequests::<Test>::contains_key(gid, ALICE));
        assert!(!JoinApprovals::<Test>::contains_key(gid, ALICE));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 0);
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().member_count, 3);
    });
}

#[test]
fn request_join_public_group_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, true);
        assert_noop!(
            ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::PublicGroupNoApproval
        );
    });
}

#[test]
fn private_add_without_approval_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                0,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE, BOB]),
                delta(vec![ALICE, BOB], vec![]),
            ),
            Error::<Test>::NotApproved
        );
    });
}

#[test]
fn cancel_join_request_works() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        assert_ok!(ChatGroup::cancel_join_request(RuntimeOrigin::signed(ALICE), gid));
        assert!(!JoinRequests::<Test>::contains_key(gid, ALICE));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 0);
        assert_noop!(
            ChatGroup::cancel_join_request(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::JoinRequestNotFound
        );
    });
}

#[test]
fn approve_join_requires_admin() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        // BOB 非成员，无权批准 / BOB not a member
        assert_noop!(
            ChatGroup::approve_join(RuntimeOrigin::signed(BOB), gid, ALICE),
            Error::<Test>::NotMember
        );
    });
}

// ===================== P1: 治理 / governance =====================

#[test]
fn transfer_ownership_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::transfer_ownership(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().admin, ALICE);
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Owner);
        assert_eq!(GroupMembers::<Test>::get(gid, OWNER).unwrap().role, MemberRole::Admin);
    });
}

#[test]
fn transfer_ownership_rebinds_chat_hook() {
    new_test_ext().execute_with(|| {
        let _ = drain_hook_events();
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::transfer_ownership(RuntimeOrigin::signed(OWNER), gid, ALICE));
        let events = drain_hook_events();
        // BOB：revoke(BOB, OWNER) + grant(BOB, ALICE)；OWNER(现管理员)：revoke + grant
        assert!(events.contains(&(false, gid, BOB, OWNER)));
        assert!(events.contains(&(true, gid, BOB, ALICE)));
        assert!(events.contains(&(true, gid, OWNER, ALICE)));
        assert!(!events.iter().any(|e| e.2 == ALICE && e.3 == ALICE));
    });
}

#[test]
fn transfer_to_non_member_fails() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_noop!(
            ChatGroup::transfer_ownership(RuntimeOrigin::signed(OWNER), gid, BOB),
            Error::<Test>::TargetNotMember
        );
    });
}

#[test]
fn set_admin_works_and_guards() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Admin);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, false));
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Member);
        assert_noop!(
            ChatGroup::set_admin(RuntimeOrigin::signed(ALICE), gid, OWNER, true),
            Error::<Test>::NotGroupOwner
        );
    });
}

#[test]
fn admin_can_add_member_after_promotion() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        commit_at(gid, ALICE, epoch, vec![CAROL], vec![]);
        assert!(GroupMembers::<Test>::contains_key(gid, CAROL));
    });
}

// ===================== P1: 自助退群 / self-leave =====================

#[test]
fn member_self_leave_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        add_member(gid, CAROL);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        commit_at(gid, ALICE, epoch, vec![], vec![ALICE]);
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().member_count, 3);
    });
}

#[test]
fn owner_cannot_self_leave() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                0,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![OWNER]),
            ),
            Error::<Test>::MustTransferFirst
        );
    });
}

// ============================================================================
// P0：隐私不变量 + welcome/delta 一致性
// P0: privacy invariant + welcome/delta consistency
// ============================================================================

#[test]
fn two_member_group_forbidden_on_single_add() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE]),
                delta(vec![ALICE], vec![]),
            ),
            Error::<Test>::TwoMemberGroupForbidden
        );
    });
}

#[test]
fn two_member_group_forbidden_on_leave_from_three() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![ALICE]),
            ),
            Error::<Test>::TwoMemberGroupForbidden
        );
    });
}

#[test]
fn welcome_mismatch_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        // 增员但 welcomes 缺少 BOB
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE]),
                delta(vec![ALICE, BOB], vec![]),
            ),
            Error::<Test>::WelcomeMismatch
        );
        // 无增员但携带 welcome
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE]),
                delta(vec![], vec![]),
            ),
            Error::<Test>::WelcomeMismatch
        );
    });
}

#[test]
fn opaque_blobs_round_trip_on_seed_commit() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let commit_bytes = vec![1, 2, 3, 4, 5];
        let tree = [0x7Eu8; 32];
        let transcript = [0x3Cu8; 32];
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            commit_bytes.clone(),
            tree,
            transcript,
            b"cid".to_vec(),
            welcomes(&[ALICE, BOB]),
            delta(vec![ALICE, BOB], vec![]),
        ));
        let logged = crate::HandshakeLog::<Test>::get(gid, 1).unwrap();
        assert_eq!(logged.to_vec(), commit_bytes);
        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.tree_hash, tree);
        assert_eq!(state.confirmed_transcript_hash, transcript);
    });
}

// ============================================================================
// P1 阶段A：群展示资料 / 群内昵称 / 封禁 / 禁言
// P1 phase A: group profile / in-group nickname / ban / mute
// ============================================================================

#[test]
fn group_profile_set_by_admin_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::set_group_profile(
            RuntimeOrigin::signed(OWNER),
            gid,
            Some(b"My Group".to_vec()),
            Some(b"QmAvatarCid".to_vec()),
            Some(b"Welcome all".to_vec()),
        ));
        let p = GroupProfiles::<Test>::get(gid).unwrap();
        assert_eq!(p.name.to_vec(), b"My Group".to_vec());
        assert_eq!(p.avatar_cid.to_vec(), b"QmAvatarCid".to_vec());
        assert_eq!(p.announcement.to_vec(), b"Welcome all".to_vec());
        System::assert_has_event(Event::GroupProfileUpdated { group_id: gid, by: OWNER }.into());

        // 部分更新：仅改公告，其余保持 / partial update keeps other fields
        assert_ok!(ChatGroup::set_group_profile(
            RuntimeOrigin::signed(OWNER),
            gid,
            None,
            None,
            Some(b"New notice".to_vec()),
        ));
        let p = GroupProfiles::<Test>::get(gid).unwrap();
        assert_eq!(p.name.to_vec(), b"My Group".to_vec());
        assert_eq!(p.announcement.to_vec(), b"New notice".to_vec());
    });
}

#[test]
fn group_profile_non_admin_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_noop!(
            ChatGroup::set_group_profile(RuntimeOrigin::signed(ALICE), gid, Some(b"x".to_vec()), None, None),
            Error::<Test>::NotAuthorized
        );
        // 非成员 → NotMember / non-member
        assert_noop!(
            ChatGroup::set_group_profile(RuntimeOrigin::signed(CAROL), gid, Some(b"x".to_vec()), None, None),
            Error::<Test>::NotMember
        );
    });
}

#[test]
fn group_profile_too_long_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // MaxGroupNameLen = 64 in mock / 群名上限
        let long = vec![b'a'; 65];
        assert_noop!(
            ChatGroup::set_group_profile(RuntimeOrigin::signed(OWNER), gid, Some(long), None, None),
            Error::<Test>::TooLong
        );
    });
}

#[test]
fn group_nickname_set_and_cleared_on_leave() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        add_member(gid, CAROL);
        assert_ok!(ChatGroup::set_group_nickname(
            RuntimeOrigin::signed(ALICE),
            gid,
            Some(b"Ali".to_vec()),
        ));
        assert_eq!(GroupNicknames::<Test>::get(gid, ALICE).unwrap().to_vec(), b"Ali".to_vec());
        System::assert_has_event(Event::MemberNicknameSet { group_id: gid, who: ALICE }.into());

        assert_ok!(ChatGroup::set_group_nickname(RuntimeOrigin::signed(ALICE), gid, None));
        assert!(!GroupNicknames::<Test>::contains_key(gid, ALICE));

        assert_ok!(ChatGroup::set_group_nickname(RuntimeOrigin::signed(ALICE), gid, Some(b"Ali".to_vec())));
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        commit_at(gid, ALICE, epoch, vec![], vec![ALICE]);
        assert!(!GroupNicknames::<Test>::contains_key(gid, ALICE));
    });
}

#[test]
fn group_nickname_requires_membership() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_noop!(
            ChatGroup::set_group_nickname(RuntimeOrigin::signed(CAROL), gid, Some(b"x".to_vec())),
            Error::<Test>::NotMember
        );
    });
}

#[test]
fn ban_blocks_request_join_and_add() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false); // private
        assert_ok!(ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert!(Banned::<Test>::contains_key(gid, ALICE));
        System::assert_has_event(Event::MemberBanned { group_id: gid, who: ALICE, by: OWNER }.into());

        // 被封禁者不能申请入群 / banned cannot request join
        assert_noop!(
            ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::Banned
        );

        // 即便强行 commit add 也被拒 / even a direct add commit is rejected
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                0,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE, BOB]),
                delta(vec![ALICE, BOB], vec![]),
            ),
            Error::<Test>::Banned
        );
    });
}

#[test]
fn ban_then_unban_allows_rejoin() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert_ok!(ChatGroup::unban_member(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert!(!Banned::<Test>::contains_key(gid, ALICE));
        System::assert_has_event(Event::MemberUnbanned { group_id: gid, who: ALICE, by: OWNER }.into());
        // 解封后可申请 / can request after unban
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
    });
}

#[test]
fn ban_guards_self_owner_and_duplicates() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        // 管理员不能封自己 / admin cannot ban self
        assert_noop!(
            ChatGroup::ban_member(RuntimeOrigin::signed(ALICE), gid, ALICE),
            Error::<Test>::CannotTargetSelf
        );
        // 不能封群主 / cannot ban the owner
        assert_noop!(
            ChatGroup::ban_member(RuntimeOrigin::signed(ALICE), gid, OWNER),
            Error::<Test>::NotAuthorized
        );
        // 重复封禁 / duplicate ban
        assert_ok!(ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, BOB));
        assert_noop!(
            ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, BOB),
            Error::<Test>::AlreadyBanned
        );
        // 解封不存在的封禁 / unban a non-banned account
        assert_noop!(
            ChatGroup::unban_member(RuntimeOrigin::signed(OWNER), gid, CAROL),
            Error::<Test>::NotBanned
        );
    });
}

#[test]
fn ban_consumes_pending_join_state() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 1);
        assert_ok!(ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, ALICE));
        // 封禁顺带清理待批申请 / ban clears the pending request
        assert!(!JoinRequests::<Test>::contains_key(gid, ALICE));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 0);
    });
}

#[test]
fn mute_member_and_query() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, ALICE, Some(100)));
        assert_eq!(MemberMutedUntil::<Test>::get(gid, ALICE).unwrap(), 100);
        assert!(ChatGroup::is_member_muted(gid, &ALICE));
        System::assert_has_event(Event::MemberMuted { group_id: gid, who: ALICE, until: 100, by: OWNER }.into());

        // 到期后解除 / expires after the block
        run_to_block(100);
        assert!(!ChatGroup::is_member_muted(gid, &ALICE));

        // 显式解除 / explicit unmute
        assert_ok!(ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, ALICE, None));
        assert!(!MemberMutedUntil::<Test>::contains_key(gid, ALICE));
        System::assert_has_event(Event::MemberUnmuted { group_id: gid, who: ALICE, by: OWNER }.into());
    });
}

#[test]
fn mute_guards() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_noop!(
            ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, ALICE, Some(1)),
            Error::<Test>::InvalidMuteExpiry
        );
        assert_noop!(
            ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, CAROL, Some(100)),
            Error::<Test>::TargetNotMember
        );
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, BOB, true));
        assert_noop!(
            ChatGroup::set_member_mute(RuntimeOrigin::signed(BOB), gid, OWNER, Some(100)),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn mute_all_exempts_admins() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, BOB, true));

        assert_ok!(ChatGroup::set_group_mute_all(RuntimeOrigin::signed(OWNER), gid, true));
        assert!(GroupMutedAll::<Test>::get(gid));
        System::assert_has_event(Event::GroupMuteAllSet { group_id: gid, on: true, by: OWNER }.into());

        // 全员禁言下：普通成员被禁，群主/管理员豁免。
        assert!(ChatGroup::is_member_muted(gid, &ALICE));
        assert!(!ChatGroup::is_member_muted(gid, &OWNER));
        assert!(!ChatGroup::is_member_muted(gid, &BOB));

        // 关闭后恢复 / turn off
        assert_ok!(ChatGroup::set_group_mute_all(RuntimeOrigin::signed(OWNER), gid, false));
        assert!(!GroupMutedAll::<Test>::get(gid));
        assert!(!ChatGroup::is_member_muted(gid, &ALICE));
    });
}

#[test]
fn disband_clears_profile_ban_mute_nickname() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid);
        assert_ok!(ChatGroup::set_group_profile(RuntimeOrigin::signed(OWNER), gid, Some(b"G".to_vec()), None, None));
        assert_ok!(ChatGroup::set_group_nickname(RuntimeOrigin::signed(ALICE), gid, Some(b"Ali".to_vec())));
        assert_ok!(ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, ALICE, Some(100)));
        assert_ok!(ChatGroup::set_group_mute_all(RuntimeOrigin::signed(OWNER), gid, true));
        assert_ok!(ChatGroup::ban_member(RuntimeOrigin::signed(OWNER), gid, BOB));

        assert_ok!(ChatGroup::disband_group(RuntimeOrigin::signed(OWNER), gid));

        assert!(!GroupProfiles::<Test>::contains_key(gid));
        assert!(!GroupNicknames::<Test>::contains_key(gid, ALICE));
        assert!(!MemberMutedUntil::<Test>::contains_key(gid, ALICE));
        assert!(!GroupMutedAll::<Test>::get(gid));
        assert!(!Banned::<Test>::contains_key(gid, BOB));
    });
}

// EN: Multi-device prerequisite (Track B / CHAT_MULTIDEVICE_MLS_SYNC_DESIGN §9.2 Phase 0).
// A `commit` is signed by the AccountId; the `committer must be a member` gate checks
// AccountId membership. For the same-account multi-device case the account is already a
// member, so an empty-delta commit (the on-chain projection of an MLS external-commit /
// rekey performed by a new device) MUST pass: epoch advances, HandshakeLog gets an entry,
// member set is unchanged, and none of NotMember / NotAuthorized / TwoMemberGroupForbidden
// fire. This proves the chain does NOT block multi-device self-rekey — no chain change is
// needed for it (Phase 3 External Commit is only for a true cross-account non-member).
// CN: 多设备前置（路线 B / 设计文档 §9.2 Phase 0）。`commit` 由 AccountId 签名，
// 「committer 必须是成员」闸门校验的是 AccountId 成员资格。同账户多设备场景账户本就是成员，
// 故一条空增删 commit（新设备做 MLS external-commit/rekey 的链上投影）必须放行：epoch 推进、
// HandshakeLog 落条、成员集不变，且不触发 NotMember/NotAuthorized/TwoMemberGroupForbidden。
// 这证明链上不阻挡多设备自助 rekey——该场景无需链改（Phase 3 External Commit 仅为真·跨账户非成员）。
#[test]
fn same_account_empty_delta_commit_rekey_is_allowed() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid); // epoch 1, members: OWNER + ALICE + BOB (count 3)

        let before = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(before.epoch, 1);
        assert_eq!(before.member_count, 3);

        // 普通成员 ALICE 以「新设备」身份提交空增删 commit（rekey / external-commit 投影）。
        // A regular member ALICE submits an empty-delta commit as a "new device" (rekey).
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            1, // expected_epoch == current
            vec![7, 7, 7], // opaque commit bytes
            [9u8; 32],
            [9u8; 32],
            b"cid-rekey".to_vec(),
            vec![],            // no welcomes: no added members
            delta(vec![], vec![]),
        ));

        let after = GroupMls::<Test>::get(gid).unwrap();
        // epoch 推进；成员数不变 / epoch advanced; membership unchanged
        assert_eq!(after.epoch, 2);
        assert_eq!(after.member_count, 3);
        // 新 epoch 的 Commit 落入 HandshakeLog，供其它设备补齐 / logged for catch-up
        assert!(crate::HandshakeLog::<Test>::contains_key(gid, 2));
        // ALICE 仍是成员，未被误判 / still a member, not misjudged
        assert!(GroupMembers::<Test>::contains_key(gid, ALICE));
        System::assert_has_event(
            Event::Committed { group_id: gid, epoch: 2, committer: ALICE }.into(),
        );
    });
}

// EN: Boundary of the above: a true cross-account non-member (never joined) is still
// rejected with NotMember even with an empty delta. This is the ONLY gap that a future
// External Commit channel (Phase 3) would need to open; it is NOT the multi-device case.
// CN: 上一条的边界：真·跨账户非成员（从未入群）即便空增删也仍被 NotMember 拒绝。
// 这是未来 External Commit 通道（Phase 3）唯一需要放开的缺口，且**不是**多设备场景。
#[test]
fn true_non_member_commit_still_rejected() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        seed_group(gid); // CAROL 从未入群 / CAROL never joined
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(CAROL),
                gid,
                1,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![]),
            ),
            Error::<Test>::NotMember
        );
    });
}

#[test]
fn platform_muted_committer_is_rejected() {
    use crate::mock::{clear_platform_mutes, mute_platform};
    new_test_ext().execute_with(|| {
        clear_platform_mutes();
        let gid = create(OWNER);
        seed_group(gid);
        mute_platform(ALICE);
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(ALICE),
                gid,
                1,
                vec![1, 2, 3],
                [1u8; 32],
                [2u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![]),
            ),
            Error::<Test>::SenderPlatformMuted
        );
    });
}

#[test]
fn approve_join_rejected_when_group_frozen() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, false);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        assert_ok!(ChatGroup::set_group_frozen(RuntimeOrigin::root(), gid, true));
        assert_noop!(
            ChatGroup::approve_join(RuntimeOrigin::signed(OWNER), gid, ALICE),
            Error::<Test>::GroupFrozen
        );
    });
}

#[test]
fn member_delta_rejects_duplicate_accounts() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                welcomes(&[ALICE, ALICE]),
                delta(vec![ALICE, ALICE], vec![]),
            ),
            Error::<Test>::BadMemberDelta
        );
    });
}

#[test]
fn anchor_rejects_stale_epoch() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_noop!(
            ChatGroup::anchor_message_digest(
                RuntimeOrigin::signed(OWNER),
                gid,
                0,
                [1u8; 32],
                99,
            ),
            Error::<Test>::EpochStale
        );
    });
}

#[test]
fn failed_commit_does_not_consume_mls_rate_limit() {
    use frame_support::traits::Get;
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        let max = <<Test as crate::Config>::MaxMlsActionsPerWindow as Get<u32>>::get();
        for seq in 0..max.saturating_sub(1) {
            assert_ok!(ChatGroup::anchor_message_digest(
                RuntimeOrigin::signed(OWNER),
                gid,
                seq as u64,
                [1u8; 32],
                0,
            ));
        }
        let stale_epoch = GroupMls::<Test>::get(gid).unwrap().epoch.saturating_add(1);
        for _ in 0..5 {
            assert_noop!(
                ChatGroup::commit(
                    RuntimeOrigin::signed(OWNER),
                    gid,
                    stale_epoch,
                    vec![],
                    [0u8; 32],
                    [0u8; 32],
                    b"cid".to_vec(),
                    vec![],
                    delta(vec![], vec![]),
                ),
                Error::<Test>::EpochStale
            );
        }
        assert_ok!(ChatGroup::anchor_message_digest(
            RuntimeOrigin::signed(OWNER),
            gid,
            max as u64,
            [1u8; 32],
            0,
        ));
    });
}

#[test]
fn request_join_rate_limit_blocks_excess_requests() {
    use crate::mock::run_to_block;
    use frame_support::traits::Get;
    new_test_ext().execute_with(|| {
        let max = <<Test as crate::Config>::MaxJoinRequestsPerWindow as Get<u32>>::get();
        let window = <<Test as crate::Config>::JoinRequestWindow as Get<u64>>::get();
        let owners = [OWNER, BOB, CAROL, 5u64, 6u64];
        for owner in owners.iter().take(max as usize) {
            let gid = create_with(*owner, false);
            assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        }
        let gid = create_with(7, false);
        assert_noop!(
            ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid),
            Error::<Test>::RateLimited
        );
        run_to_block(1 + window);
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
    });
}
