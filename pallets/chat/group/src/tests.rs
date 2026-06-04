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
                vec![],
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
        // 先把 ALICE 加进来（普通成员）/ add ALICE as a plain member
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        // ALICE（Member）尝试加 BOB → 无权 / ALICE (Member) tries to add BOB → not authorized
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(ALICE),
                gid,
                1,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![(BOB, vec![1])],
                delta(vec![BOB], vec![]),
            ),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn commit_removes_member_and_syncs_user_groups() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert!(UserGroups::<Test>::get(ALICE).contains(&gid));

        // 移除 ALICE / remove ALICE
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            1,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
        // 无幽灵群：成员表与 UserGroups 都已清 / no ghost entries
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().member_count, 1);
    });
}

#[test]
fn admin_cannot_remove_owner() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        // 加 ALICE 并升为管理员 / add ALICE, promote to admin
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        // 管理员尝试移除群主 → 拒绝 / admin tries to remove the owner → rejected
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(ALICE),
                gid,
                1,
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
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![2, 3, 4, 5, 6, 7, 8], vec![]),
        ));
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
                vec![],
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
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1, 2, 3])],
            delta(vec![ALICE], vec![]),
        ));
        assert_ok!(ChatGroup::claim_welcome(RuntimeOrigin::signed(ALICE), gid));
        assert!(!WelcomeMailbox::<Test>::contains_key(gid, ALICE));
        // 重复领取失败 / second claim fails
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
        let gid = create(OWNER); // 公开群 / public group
        let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
        // 账户 15 在候选池之外，未发布 KeyPackage → 拒绝。
        // Account 15 is outside the seeded pool and has no KeyPackage → rejected.
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                epoch,
                vec![],
                [0u8; 32],
                [0u8; 32],
                b"cid".to_vec(),
                vec![(15u64, vec![1])],
                delta(vec![15u64], vec![]),
            ),
            Error::<Test>::AddeeNotJoinable
        );

        // 账户 15 发布 KeyPackage（同意）后即可被加入。
        // Once account 15 publishes a KeyPackage (consent), the add succeeds.
        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(15u64), vec![15]));
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            epoch,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(15u64, vec![1])],
            delta(vec![15u64], vec![]),
        ));
        assert!(GroupMembers::<Test>::contains_key(gid, 15u64));
    });
}

#[test]
fn disband_cleans_everything() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1]), (BOB, vec![2])],
            delta(vec![ALICE, BOB], vec![]),
        ));
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
                vec![(ALICE, vec![1])],
                delta(vec![ALICE], vec![]),
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
        // ALICE 申请 / request
        assert_ok!(ChatGroup::request_join(RuntimeOrigin::signed(ALICE), gid));
        assert!(JoinRequests::<Test>::contains_key(gid, ALICE));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 1);
        System::assert_has_event(Event::JoinRequested { group_id: gid, who: ALICE }.into());

        // OWNER 批准 / approve
        assert_ok!(ChatGroup::approve_join(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert!(JoinApprovals::<Test>::contains_key(gid, ALICE));

        // Add commit 消费申请/批准 / commit consumes request & approval
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert!(GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!JoinRequests::<Test>::contains_key(gid, ALICE));
        assert!(!JoinApprovals::<Test>::contains_key(gid, ALICE));
        assert_eq!(PendingJoinCount::<Test>::get(gid), 0);
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
                vec![(ALICE, vec![1])],
                delta(vec![ALICE], vec![]),
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
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert_ok!(ChatGroup::transfer_ownership(RuntimeOrigin::signed(OWNER), gid, ALICE));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().admin, ALICE);
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Owner);
        assert_eq!(GroupMembers::<Test>::get(gid, OWNER).unwrap().role, MemberRole::Admin);
        System::assert_has_event(
            Event::OwnershipTransferred { group_id: gid, from: OWNER, to: ALICE }.into(),
        );
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
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        // 提升 ALICE 为管理员 / promote
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Admin);
        // 撤销 / demote
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, false));
        assert_eq!(GroupMembers::<Test>::get(gid, ALICE).unwrap().role, MemberRole::Member);
        // 非群主无权 / non-owner forbidden
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
        // 加 ALICE 并提升为管理员 / add ALICE, promote
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert_ok!(ChatGroup::set_admin(RuntimeOrigin::signed(OWNER), gid, ALICE, true));
        // ALICE（Admin）现在可加 BOB / ALICE (Admin) may now add BOB
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            1,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(BOB, vec![1])],
            delta(vec![BOB], vec![]),
        ));
        assert!(GroupMembers::<Test>::contains_key(gid, BOB));
    });
}

// ===================== P1: 自助退群 / self-leave =====================

#[test]
fn member_self_leave_works() {
    new_test_ext().execute_with(|| {
        let gid = create(OWNER);
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        // ALICE 自己退群（自助 Remove commit）/ ALICE leaves via self-remove commit
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            1,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
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
// C3：1:1 = 2 人群锚（复用 chat-group，无需新增链上代码）
// C3: 1:1 modeled as a 2-member group (reuses chat-group, no new chain code).
//
// 收敛方案把 1:1 私聊视为「成员数 = 2 的 MLS 群」（XMTP/libxmtp 工业做法）。
// 链侧无需任何新 extrinsic：发起方建群并 Add 对端即得到一个 2 人会话锚，
// 享有与群聊同源的 epoch 单调 / 防分叉 / Welcome 投递 / opaque 往返。
// 以下用例证明该链路成立，作为 C3 链侧交付（opaque 往返 + epoch 单调 + 授权镜像）。
// ============================================================================

/// 1:1 完整生命周期：建群 → Add 对端（含 Welcome）→ 领取 → 对端退出。
/// Full 1:1 lifecycle: create → Add peer (with Welcome) → claim → peer leaves.
#[test]
fn one_to_one_two_member_group_lifecycle() {
    new_test_ext().execute_with(|| {
        let _ = drain_hook_events();

        // 对端先发布 KeyPackage，供发起方离线 Add。
        assert_ok!(ChatGroup::publish_key_package(RuntimeOrigin::signed(ALICE), vec![0xAA, 0xBB]));

        // 发起方建一个 2 人会话锚（公开群即可直接 Add，无需审批闸门）。
        let gid = create_with(OWNER, true);
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().member_count, 1);

        // Add 对端：opaque commit_bytes + tree/transcript 锚 + Welcome 信封。
        let commit_bytes = vec![0x11, 0x22, 0x33, 0x44];
        let welcome = vec![0x55, 0x66, 0x77];
        let tree = [0xABu8; 32];
        let transcript = [0xCDu8; 32];
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            commit_bytes.clone(),
            tree,
            transcript,
            b"cid-1to1".to_vec(),
            vec![(ALICE, welcome.clone())],
            delta(vec![ALICE], vec![]),
        ));

        // 这是个 2 人群：epoch 推进到 1，成员数 = 2。
        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.epoch, 1);
        assert_eq!(state.member_count, 2);
        assert!(GroupMembers::<Test>::contains_key(gid, OWNER));
        assert!(GroupMembers::<Test>::contains_key(gid, ALICE));

        // Welcome 已投递，领取后即删。
        assert!(WelcomeMailbox::<Test>::contains_key(gid, ALICE));
        assert_ok!(ChatGroup::claim_welcome(RuntimeOrigin::signed(ALICE), gid));
        assert!(!WelcomeMailbox::<Test>::contains_key(gid, ALICE));

        // 对端自助退出 → 回到 1 人，epoch 单调推进到 2。
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            1,
            vec![0x99],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.epoch, 2);
        assert_eq!(state.member_count, 1);
        assert!(!GroupMembers::<Test>::contains_key(gid, ALICE));
        assert!(!UserGroups::<Test>::get(ALICE).contains(&gid));
    });
}

/// opaque 往返：链原样存储/返回 commit/welcome/tree/transcript，绝不改写或解释。
/// Opaque round-trip: chain stores and returns blobs verbatim, never mutating them.
#[test]
fn one_to_one_opaque_blobs_round_trip() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, true);
        let commit_bytes = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let welcome = vec![250, 240, 230, 220];
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
            vec![(ALICE, welcome.clone())],
            delta(vec![ALICE], vec![]),
        ));

        // commit 字节按 epoch 落入 HandshakeLog，原样可取。
        let logged = crate::HandshakeLog::<Test>::get(gid, 1).unwrap();
        assert_eq!(logged.to_vec(), commit_bytes);
        // Welcome 字节原样存储。
        let stored_welcome = WelcomeMailbox::<Test>::get(gid, ALICE).unwrap();
        assert_eq!(stored_welcome.to_vec(), welcome);
        // tree_hash / transcript 锚原样保存。
        let state = GroupMls::<Test>::get(gid).unwrap();
        assert_eq!(state.tree_hash, tree);
        assert_eq!(state.confirmed_transcript_hash, transcript);
    });
}

/// epoch 单调：每次成员变更 commit epoch 恰好 +1；重放旧 expected_epoch 被拒。
/// Epoch monotonicity: each membership commit advances epoch by exactly 1; replaying
/// a stale `expected_epoch` is rejected.
#[test]
fn one_to_one_epoch_strictly_monotonic() {
    new_test_ext().execute_with(|| {
        let gid = create_with(OWNER, true);
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().epoch, 0);

        // Add 对端 → epoch 1。
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![1],
            [1u8; 32],
            [1u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().epoch, 1);

        // 重放 expected_epoch=0（已过期）→ EpochStale，防分叉/重放。
        assert_noop!(
            ChatGroup::commit(
                RuntimeOrigin::signed(OWNER),
                gid,
                0,
                vec![2],
                [2u8; 32],
                [2u8; 32],
                b"cid".to_vec(),
                vec![],
                delta(vec![], vec![ALICE]),
            ),
            Error::<Test>::EpochStale
        );

        // 用正确 expected_epoch=1 → epoch 2（移除对端）。
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            1,
            vec![3],
            [3u8; 32],
            [3u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
        assert_eq!(GroupMls::<Test>::get(gid).unwrap().epoch, 2);
    });
}

/// 1:1 授权镜像：2 人群的成员增减经 ChatHook 镜像到外部授权层（成员↔群主），
/// runtime 中即 chat-permission 场景授权，使这对用户获得 1:1 私聊权限。
/// 1:1 authorization mirroring: membership changes of a 2-member group flow through
/// ChatHook to the external authorization layer (member↔owner), which the runtime
/// maps to chat-permission scene authorization, granting the pair direct-message rights.
#[test]
fn one_to_one_membership_mirrors_to_chat_hook() {
    new_test_ext().execute_with(|| {
        let _ = drain_hook_events();
        let gid = create_with(OWNER, true);

        // Add 对端：应镜像一次 on_member_added(gid, ALICE, OWNER)。
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(OWNER),
            gid,
            0,
            vec![1],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![(ALICE, vec![1])],
            delta(vec![ALICE], vec![]),
        ));
        let events = drain_hook_events();
        assert_eq!(events, vec![(true, gid, ALICE, OWNER)]);

        // 对端退出：应镜像一次 on_member_removed(gid, ALICE, OWNER)。
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            1,
            vec![2],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
        let events = drain_hook_events();
        assert_eq!(events, vec![(false, gid, ALICE, OWNER)]);
    });
}

// ============================================================================
// P1 阶段A：群展示资料 / 群内昵称 / 封禁 / 禁言
// P1 phase A: group profile / in-group nickname / ban / mute
// ============================================================================

/// 加入一名普通成员的便捷函数（OWNER 在 epoch 0 add）。
/// Helper: add a plain member via the owner's epoch-0 commit.
fn add_member(gid: u64, who: u64) {
    let epoch = GroupMls::<Test>::get(gid).unwrap().epoch;
    assert_ok!(ChatGroup::commit(
        RuntimeOrigin::signed(OWNER),
        gid,
        epoch,
        vec![],
        [0u8; 32],
        [0u8; 32],
        b"cid".to_vec(),
        vec![(who, vec![1])],
        delta(vec![who], vec![]),
    ));
}

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
        add_member(gid, ALICE);
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
        add_member(gid, ALICE);
        assert_ok!(ChatGroup::set_group_nickname(
            RuntimeOrigin::signed(ALICE),
            gid,
            Some(b"Ali".to_vec()),
        ));
        assert_eq!(GroupNicknames::<Test>::get(gid, ALICE).unwrap().to_vec(), b"Ali".to_vec());
        System::assert_has_event(Event::MemberNicknameSet { group_id: gid, who: ALICE }.into());

        // 清除 / clear with None
        assert_ok!(ChatGroup::set_group_nickname(RuntimeOrigin::signed(ALICE), gid, None));
        assert!(!GroupNicknames::<Test>::contains_key(gid, ALICE));

        // 重设后离群应自动清理 / leaving clears it again
        assert_ok!(ChatGroup::set_group_nickname(RuntimeOrigin::signed(ALICE), gid, Some(b"Ali".to_vec())));
        assert_ok!(ChatGroup::commit(
            RuntimeOrigin::signed(ALICE),
            gid,
            GroupMls::<Test>::get(gid).unwrap().epoch,
            vec![],
            [0u8; 32],
            [0u8; 32],
            b"cid".to_vec(),
            vec![],
            delta(vec![], vec![ALICE]),
        ));
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
                vec![(ALICE, vec![1])],
                delta(vec![ALICE], vec![]),
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
        add_member(gid, ALICE);
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
        add_member(gid, ALICE);
        // 禁言到区块 100 / mute until block 100
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
        add_member(gid, ALICE);
        // 过期时间必须在未来 / expiry must be in the future
        assert_noop!(
            ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, ALICE, Some(1)),
            Error::<Test>::InvalidMuteExpiry
        );
        // 不能禁言非成员 / cannot mute a non-member
        assert_noop!(
            ChatGroup::set_member_mute(RuntimeOrigin::signed(OWNER), gid, CAROL, Some(100)),
            Error::<Test>::TargetNotMember
        );
        // 不能禁言群主 / cannot mute the owner
        add_member(gid, BOB);
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
        add_member(gid, ALICE); // plain member
        add_member(gid, BOB);
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
        add_member(gid, ALICE);
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
