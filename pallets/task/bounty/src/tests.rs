use crate as pallet_task_bounty;
use crate::{
    mock::*, ArbitrationOutcome, BountyKind, BountyState, ContactVisibility, ReviewMode,
    SubmissionState,
};
use frame_support::{assert_noop, assert_ok};
use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
use sp_runtime::traits::Hash;

const FIRST_ID: u64 = 1u64 << 60;

fn escrow_amount(id: u64) -> Balance {
    <Escrow as EscrowTrait<AccountId, Balance>>::amount_of(id)
}

fn create_single(poster: AccountId, reward: Balance) -> u64 {
    assert_ok!(TaskBounty::create_bounty(
        RuntimeOrigin::signed(poster),
        BountyKind::Single,
        reward,
        1,
        0,
        None,
        None,
    ));
    FIRST_ID
}

#[test]
fn notifier_fires_on_submit_accept_and_dispute_settle() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        assert_eq!(
            notify_log(),
            vec![(1, format_notice(b"bounty:submitted", id, &[0]))]
        );

        run_to(4);
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
        assert_eq!(
            notify_log(),
            vec![
                (1, format_notice(b"bounty:submitted", id, &[0])),
                (2, format_notice(b"bounty:accepted", id, &[0])),
                (1, format_notice(b"bounty:completed", id, &[])),
            ]
        );

        // 新一轮：争议 + 仲裁结案（`create_single` 恒返回 FIRST_ID，故第二次单独 create）。
        assert_ok!(TaskBounty::create_bounty(
            RuntimeOrigin::signed(1),
            BountyKind::Single,
            500,
            1,
            0,
            None,
            None,
        ));
        let id2 = pallet_task_bounty::NextBountyId::<Test>::get().saturating_sub(1);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id2, Some(1), None));
        run_to(4);
        assert_ok!(TaskBounty::open_dispute(RuntimeOrigin::signed(2), id2, 0));
        assert_eq!(notify_log().last().map(|(a, _)| *a), Some(1u64));
        assert_eq!(
            notify_log().last().unwrap().1,
            format_notice(b"bounty:disputed", id2, &[0])
        );

        assert_ok!(<Escrow as EscrowTrait<AccountId, Balance>>::set_resolved(id2));
        assert_ok!(<Escrow as EscrowTrait<AccountId, Balance>>::release_all(id2, &2));
        assert_ok!(TaskBounty::settle_from_arbitration(id2, ArbitrationOutcome::Release));

        let settled = format_notice(b"bounty:dispute_settled", id2, &[0]);
        assert!(notify_log().iter().any(|(a, n)| *a == 1 && *n == settled));
        assert!(notify_log().iter().any(|(a, n)| *a == 2 && *n == settled));
    });
}

fn format_notice(kind: &[u8], bounty_id: u64, parts: &[u64]) -> Vec<u8> {
    let mut v = kind.to_vec();
    v.push(b':');
    v.extend_from_slice(&u64_ascii(bounty_id));
    for p in parts {
        v.push(b':');
        v.extend_from_slice(&u64_ascii(*p));
    }
    v
}

fn u64_ascii(mut n: u64) -> Vec<u8> {
    if n == 0 {
        return vec![b'0'];
    }
    let mut buf = Vec::new();
    while n > 0 {
        buf.push(b'0' + (n % 10) as u8);
        n /= 10;
    }
    buf.reverse();
    buf
}

#[test]
fn single_flow_works() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        // poster locked reward + 5% fee = 1050
        assert_eq!(escrow_amount(id), 1050);
        assert_eq!(Balances::free_balance(1), 10_000_000 - 1050);

        // two solvers submit with deliverable
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(3), id, Some(22), None));
        // 10% stake reserved
        assert_eq!(Balances::reserved_balance(2), 100);
        assert_eq!(Balances::reserved_balance(3), 100);

        run_to(4); // pass MinOpenWindow (created=1, window=3)
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));

        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.state, BountyState::Completed);
        assert_eq!(b.winner, Some(2));
        // winner paid, stakes released
        assert_eq!(Balances::free_balance(2), 10_000_000 + 1000);
        assert_eq!(Balances::reserved_balance(2), 0);
        assert_eq!(Balances::reserved_balance(3), 0);
        // fee to collector
        assert_eq!(Balances::free_balance(999), 50);
        // escrow drained
        assert_eq!(escrow_amount(id), 0);
    });
}

#[test]
fn quota_flow_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(TaskBounty::create_bounty(
            RuntimeOrigin::signed(1),
            BountyKind::Quota,
            100,
            3,
            0,
            None,
            None,
        ));
        let id = FIRST_ID;
        // locked = 100*3 + 5% of 300 = 315
        assert_eq!(escrow_amount(id), 315);

        for s in [2u64, 3, 4] {
            assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(s), id, Some(s), None));
        }
        run_to(4);
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 1));
        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.filled, 2);
        assert_eq!(b.state, BountyState::Open);

        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 2));
        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.filled, 3);
        assert_eq!(b.state, BountyState::Completed);

        // each solver received 100
        for s in [2u64, 3, 4] {
            assert_eq!(Balances::free_balance(s), 10_000_000 + 100);
            assert_eq!(Balances::reserved_balance(s), 0);
        }
        // fee collector got all 15, escrow drained
        assert_eq!(Balances::free_balance(999), 15);
        assert_eq!(escrow_amount(id), 0);
    });
}

#[test]
fn quota_one_per_solver() {
    new_test_ext().execute_with(|| {
        assert_ok!(TaskBounty::create_bounty(
            RuntimeOrigin::signed(1),
            BountyKind::Quota,
            100,
            3,
            0,
            None,
            None,
        ));
        let id = FIRST_ID;
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(2), None));
        run_to(4);
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
        assert_noop!(
            TaskBounty::accept(RuntimeOrigin::signed(1), id, 1),
            pallet_task_bounty::Error::<Test>::AlreadyRewarded
        );
    });
}

#[test]
fn bad_slots_and_quota_cap() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            TaskBounty::create_bounty(RuntimeOrigin::signed(1), BountyKind::Single, 1000, 2, 0, None, None),
            pallet_task_bounty::Error::<Test>::BadSlots
        );
        assert_noop!(
            TaskBounty::create_bounty(RuntimeOrigin::signed(1), BountyKind::Quota, 1000, 1, 0, None, None),
            pallet_task_bounty::Error::<Test>::BadSlots
        );
        assert_noop!(
            TaskBounty::create_bounty(
                RuntimeOrigin::signed(1),
                BountyKind::Quota,
                2_000_000,
                2,
                0,
                None,
                None
            ),
            pallet_task_bounty::Error::<Test>::QuotaUnitTooHigh
        );
    });
}

#[test]
fn reward_too_low_and_self_submit() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            TaskBounty::create_bounty(RuntimeOrigin::signed(1), BountyKind::Single, 10, 1, 0, None, None),
            pallet_task_bounty::Error::<Test>::RewardTooLow
        );
        let id = create_single(1, 1000);
        assert_noop!(
            TaskBounty::submit(RuntimeOrigin::signed(1), id, Some(1), None),
            pallet_task_bounty::Error::<Test>::SelfSubmit
        );
    });
}

#[test]
fn unsupported_review_mode_rejected() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            TaskBounty::create_bounty(
                RuntimeOrigin::signed(1),
                BountyKind::Single,
                1000,
                1,
                0,
                Some(ReviewMode::AutoOnReveal),
                None
            ),
            pallet_task_bounty::Error::<Test>::ReviewModeUnsupported
        );
    });
}

#[test]
fn commit_reveal_flow() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        let evidence: u64 = 77;
        let salt: [u8; 32] = [9u8; 32];
        let commit = <Test as frame_system::Config>::Hashing::hash_of(&(evidence, salt, &2u64));
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, None, Some(commit)));

        // not deliverable yet
        run_to(4);
        assert_noop!(
            TaskBounty::accept(RuntimeOrigin::signed(1), id, 0),
            pallet_task_bounty::Error::<Test>::NoDeliverable
        );

        // wrong salt fails
        assert_noop!(
            TaskBounty::deliver(RuntimeOrigin::signed(2), id, 0, evidence, Some([1u8; 32])),
            pallet_task_bounty::Error::<Test>::CommitMismatch
        );
        // correct reveal
        assert_ok!(TaskBounty::deliver(RuntimeOrigin::signed(2), id, 0, evidence, Some(salt)));
        let sub = pallet_task_bounty::Submissions::<Test>::get(id, 0).unwrap();
        assert_eq!(sub.state, SubmissionState::Delivered);
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
    });
}

#[test]
fn missing_deliverable_rejected() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_noop!(
            TaskBounty::submit(RuntimeOrigin::signed(2), id, None, None),
            pallet_task_bounty::Error::<Test>::MissingDeliverable
        );
    });
}

#[test]
fn open_window_enforced() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        // still at block 1, window is 3
        assert_noop!(
            TaskBounty::accept(RuntimeOrigin::signed(1), id, 0),
            pallet_task_bounty::Error::<Test>::OpenWindowNotElapsed
        );
    });
}

#[test]
fn cancel_refunds_when_idle() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::cancel_bounty(RuntimeOrigin::signed(1), id));
        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.state, BountyState::Cancelled);
        assert_eq!(Balances::free_balance(1), 10_000_000);
        assert_eq!(escrow_amount(id), 0);
    });
}

#[test]
fn cancel_blocked_by_active_submission() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        assert_noop!(
            TaskBounty::cancel_bounty(RuntimeOrigin::signed(1), id),
            pallet_task_bounty::Error::<Test>::HasActiveSubmissions
        );
    });
}

#[test]
fn expire_refunds_and_unreserves() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        assert_eq!(Balances::reserved_balance(2), 100);
        assert_noop!(
            TaskBounty::expire_bounty(RuntimeOrigin::signed(5), id),
            pallet_task_bounty::Error::<Test>::NotYetExpired
        );
        run_to(101); // deadline = created(1) + 100
        assert_ok!(TaskBounty::expire_bounty(RuntimeOrigin::signed(5), id));
        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.state, BountyState::Refunded);
        assert_eq!(Balances::free_balance(1), 10_000_000);
        assert_eq!(Balances::reserved_balance(2), 0);
        assert_eq!(escrow_amount(id), 0);
    });
}

#[test]
fn withdraw_unreserves() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        assert_eq!(Balances::reserved_balance(2), 100);
        assert_ok!(TaskBounty::withdraw_submission(RuntimeOrigin::signed(2), id, 0));
        assert_eq!(Balances::reserved_balance(2), 0);
        let sub = pallet_task_bounty::Submissions::<Test>::get(id, 0).unwrap();
        assert_eq!(sub.state, SubmissionState::Withdrawn);
    });
}

#[test]
fn dispute_then_settle_release() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        run_to(4);
        assert_ok!(TaskBounty::open_dispute(RuntimeOrigin::signed(2), id, 0));
        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.state, BountyState::Disputed);
        assert_eq!(b.contested, Some(2));

        // simulate the runtime arbitration router: settle escrow then notify pallet
        assert_ok!(<Escrow as EscrowTrait<AccountId, Balance>>::set_resolved(id));
        assert_ok!(<Escrow as EscrowTrait<AccountId, Balance>>::release_all(id, &2));
        assert_ok!(TaskBounty::settle_from_arbitration(id, ArbitrationOutcome::Release));

        let b = pallet_task_bounty::Bounties::<Test>::get(id).unwrap();
        assert_eq!(b.state, BountyState::Completed);
        // contested solver stake unreserved by settle
        assert_eq!(Balances::reserved_balance(2), 0);
        let stats = pallet_task_bounty::UserStats::<Test>::get(1);
        assert_eq!(stats.poster_dispute_lost, 1);
    });
}

#[test]
fn reputation_writeback_counters() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_eq!(pallet_task_bounty::UserStats::<Test>::get(1).poster_published, 1);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None));
        assert_eq!(pallet_task_bounty::UserStats::<Test>::get(2).solver_submitted, 1);
        run_to(4);
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
        let ps = pallet_task_bounty::UserStats::<Test>::get(1);
        let ss = pallet_task_bounty::UserStats::<Test>::get(2);
        assert_eq!(ps.poster_completed, 1);
        assert_eq!(ss.solver_accepted, 1);
        // derived reputation moved above neutral for both. / 双方派生分高于中性。
        assert!(
            <TaskBounty as pallet_task_bounty::BountyReputationInspect<AccountId>>::reputation_of(
                &1,
                pallet_task_bounty::BountyReputationRole::Poster
            ) > 5000
        );
        assert!(
            <TaskBounty as pallet_task_bounty::BountyReputationInspect<AccountId>>::reputation_of(
                &2,
                pallet_task_bounty::BountyReputationRole::Solver
            ) > 5000
        );
    });
}

#[test]
fn submit_blocked_by_low_solver_reputation() {
    new_test_ext().execute_with(|| {
        // Tank solver 2's reputation directly (e.g. prior dispute loss). / 直接压低 solver 声誉。
        pallet_task_bounty::UserStats::<Test>::insert(
            2,
            pallet_task_bounty::BountyUserStats { solver_dispute_lost: 2, ..Default::default() },
        );
        let id = create_single(1, 1000);
        assert_noop!(
            TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(1), None),
            pallet_task_bounty::Error::<Test>::SolverReputationTooLow
        );
        // a fresh solver (neutral 5000) is unaffected. / 新人不受影响。
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(3), id, Some(1), None));
    });
}

#[test]
fn create_gated_by_poster_reputation_only_above_threshold() {
    new_test_ext().execute_with(|| {
        // Poster 1 has lost disputes → reputation 1000 (5000 - 4000). / 一次败诉 → 1000。
        pallet_task_bounty::UserStats::<Test>::insert(
            1,
            pallet_task_bounty::BountyUserStats { poster_dispute_lost: 1, ..Default::default() },
        );
        // Small bounty (< threshold 5000) is NOT gated. / 小额（<阈值）不校验。
        assert_ok!(TaskBounty::create_bounty(
            RuntimeOrigin::signed(1),
            BountyKind::Single,
            1000,
            1,
            0,
            None,
            None,
        ));
        // Large bounty (>= threshold) requires rep >= 1000; here rep == 1000 passes,
        // push one more loss to drop below. / 再降一档使其低于门槛。
        pallet_task_bounty::UserStats::<Test>::insert(
            1,
            pallet_task_bounty::BountyUserStats { poster_dispute_lost: 2, ..Default::default() },
        );
        assert_noop!(
            TaskBounty::create_bounty(
                RuntimeOrigin::signed(1),
                BountyKind::Single,
                6000,
                1,
                0,
                None,
                None,
            ),
            pallet_task_bounty::Error::<Test>::PosterReputationTooLow
        );
    });
}

#[test]
fn id_namespace_isolated_from_low_range() {
    new_test_ext().execute_with(|| {
        // a low-range escrow id (mimicking an order) must not collide
        assert_ok!(<Escrow as EscrowTrait<AccountId, Balance>>::lock_from(&5, 7u64, 500));
        let id = create_single(1, 1000);
        assert!(id >= (1u64 << 60));
        assert_eq!(escrow_amount(7), 500);
        assert_eq!(escrow_amount(id), 1050);
    });
}

/// Create a Single bounty under an explicit category. / 以指定类目创建 Single 悬赏。
fn create_with_category(poster: AccountId, reward: Balance, category: u16) -> u64 {
    assert_ok!(TaskBounty::create_bounty(
        RuntimeOrigin::signed(poster),
        BountyKind::Single,
        reward,
        1,
        category,
        None,
        None,
    ));
    FIRST_ID
}

#[test]
fn set_meta_works_and_is_stored() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::set_meta(
            RuntimeOrigin::signed(1),
            id,
            5,
            None,
            None,
            None,
            ContactVisibility::AfterAccept,
        ));
        let meta = pallet_task_bounty::BountyMeta::<Test>::get(id).unwrap();
        assert_eq!(meta.coop_profile_ref, 5);
        assert_eq!(meta.contact_visibility, ContactVisibility::AfterAccept);
        // updates are allowed while Open. / Open 期间可更新。
        assert_ok!(TaskBounty::set_meta(
            RuntimeOrigin::signed(1),
            id,
            6,
            None,
            Some(110_000),
            None,
            ContactVisibility::Hidden,
        ));
        let meta = pallet_task_bounty::BountyMeta::<Test>::get(id).unwrap();
        assert_eq!(meta.coop_profile_ref, 6);
        assert_eq!(meta.region, Some(110_000));
    });
}

#[test]
fn set_meta_rejects_unowned_coop_ref_and_non_poster() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        // ref 0 is not poster-owned per MockEvidence. / 0 视为非自有证据。
        assert_noop!(
            TaskBounty::set_meta(
                RuntimeOrigin::signed(1),
                id,
                0,
                None,
                None,
                None,
                ContactVisibility::AfterAccept,
            ),
            pallet_task_bounty::Error::<Test>::BadCoopProfileRef
        );
        // non-poster cannot set meta. / 非发布方不可设置。
        assert_noop!(
            TaskBounty::set_meta(
                RuntimeOrigin::signed(2),
                id,
                5,
                None,
                None,
                None,
                ContactVisibility::AfterAccept,
            ),
            pallet_task_bounty::Error::<Test>::NotPoster
        );
    });
}

#[test]
fn set_meta_region_required_for_ground_category() {
    new_test_ext().execute_with(|| {
        // category 1 == GroundPromoCategory in mock. / mock 中类目 1 即地推类目。
        let id = create_with_category(1, 1000, 1);
        assert_noop!(
            TaskBounty::set_meta(
                RuntimeOrigin::signed(1),
                id,
                5,
                None,
                None,
                None,
                ContactVisibility::AfterAccept,
            ),
            pallet_task_bounty::Error::<Test>::RegionRequired
        );
        // with a region it succeeds. / 带地区即可。
        assert_ok!(TaskBounty::set_meta(
            RuntimeOrigin::signed(1),
            id,
            5,
            None,
            Some(440_300),
            None,
            ContactVisibility::AfterAccept,
        ));
    });
}

#[test]
fn set_meta_rejected_after_terminal_state() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::cancel_bounty(RuntimeOrigin::signed(1), id));
        assert_noop!(
            TaskBounty::set_meta(
                RuntimeOrigin::signed(1),
                id,
                5,
                None,
                None,
                None,
                ContactVisibility::AfterAccept,
            ),
            pallet_task_bounty::Error::<Test>::NotOpen
        );
    });
}

// ---------------------- Chat wiring (§5.3) ----------------------

#[test]
fn chat_granted_on_submit_when_on_submit_visibility() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::set_meta(
            RuntimeOrigin::signed(1),
            id,
            5,
            None,
            None,
            None,
            ContactVisibility::OnSubmit,
        ));
        let _ = chat_log_take();
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        // OnSubmit → 提交即授予 poster↔solver 双向聊天。
        assert_eq!(chat_log_take(), vec![(true, id, 1, 2)]);
    });
}

#[test]
fn chat_not_granted_on_submit_for_default_after_accept() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        let _ = chat_log_take();
        // No meta → defaults to AfterAccept; submit must not grant chat. / 默认 AfterAccept，提交不授权。
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        assert_eq!(chat_log_take(), vec![]);
    });
}

#[test]
fn chat_granted_on_accept_and_losers_revoked() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(3), id, Some(22), None));
        run_to(4);
        let _ = chat_log_take();
        assert_ok!(TaskBounty::accept(RuntimeOrigin::signed(1), id, 0));
        // Winner(2) granted; loser(3) revoked. / 中标者授予，落选撤销。
        assert_eq!(chat_log_take(), vec![(true, id, 1, 2), (false, id, 1, 3)]);
    });
}

#[test]
fn chat_revoked_on_withdraw() {
    new_test_ext().execute_with(|| {
        let id = create_single(1, 1000);
        assert_ok!(TaskBounty::set_meta(
            RuntimeOrigin::signed(1),
            id,
            5,
            None,
            None,
            None,
            ContactVisibility::OnSubmit,
        ));
        assert_ok!(TaskBounty::submit(RuntimeOrigin::signed(2), id, Some(11), None));
        let _ = chat_log_take();
        assert_ok!(TaskBounty::withdraw_submission(RuntimeOrigin::signed(2), id, 0));
        assert_eq!(chat_log_take(), vec![(false, id, 1, 2)]);
    });
}
