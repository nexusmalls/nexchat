use crate::{mock::*, pallet::*, types::*};
use frame_support::{assert_noop, assert_ok, BoundedVec};
use frame_system::RawOrigin;

const DOMAIN: [u8; 8] = *b"otc_ord_";

fn cid(s: &[u8]) -> BoundedVec<u8, <Test as Config>::MaxCidLen> {
    BoundedVec::truncate_from(s.to_vec())
}

// ==================== dispute (call_index 0, 2) - deprecated ====================

#[test]
fn dispute_deprecated() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Arbitration::dispute(RuntimeOrigin::signed(1), DOMAIN, 1, vec![cid(b"ev")],),
            Error::<Test>::Deprecated
        );
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

#[test]
fn dispute_with_evidence_id_deprecated() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Arbitration::dispute_with_evidence_id(RuntimeOrigin::signed(1), DOMAIN, 1, 42,),
            Error::<Test>::Deprecated
        );
    });
}

// ==================== dispute_with_two_way_deposit (call_index 4) ====================

#[test]
fn dispute_with_two_way_deposit_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_some());
        let evidence_ids = EvidenceIds::<Test>::get(DOMAIN, 1);
        assert_eq!(evidence_ids.to_vec(), vec![42]);
        assert!(TwoWayDeposits::<Test>::get(DOMAIN, 1).is_some());
    });
}

#[test]
fn dispute_with_two_way_deposit_rejects_bad_evidence() {
    new_test_ext().execute_with(|| {
        set_evidence_exists(false);
        assert_noop!(
            Arbitration::dispute_with_two_way_deposit(RuntimeOrigin::signed(1), DOMAIN, 1, 42,),
            Error::<Test>::EvidenceNotFound
        );
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

#[test]
fn dispute_self_dispute_rejected() {
    new_test_ext().execute_with(|| {
        set_counterparty(Some(1));
        assert_noop!(
            Arbitration::dispute_with_two_way_deposit(RuntimeOrigin::signed(1), DOMAIN, 1, 42,),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn notifier_fires_on_dispute_open_and_arbitrate() {
    new_test_ext().execute_with(|| {
        // 发起方=1，被诉方=2（mock counterparty）。
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_eq!(
            notify_log(),
            vec![(2, b"dispute:opened:otc_ord_:1".to_vec())]
        );

        assert_ok!(Arbitration::arbitrate(
            RawOrigin::Root.into(),
            DOMAIN,
            1,
            0,
            None,
        ));
        assert_eq!(
            notify_log(),
            vec![
                (2, b"dispute:opened:otc_ord_:1".to_vec()),
                (1, b"dispute:arbitrated:otc_ord_:1:0".to_vec()),
                (2, b"dispute:arbitrated:otc_ord_:1:0".to_vec()),
            ]
        );
    });
}

// ==================== arbitrate (call_index 1) ====================

#[test]
fn arbitrate_release_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::arbitrate(
            RawOrigin::Root.into(),
            DOMAIN,
            1,
            0,
            None,
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
        let stats = ArbitrationStats::<Test>::get();
        assert_eq!(stats.total_disputes, 1);
        assert_eq!(stats.release_count, 1);
    });
}

#[test]
fn arbitrate_refund_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::arbitrate(
            RawOrigin::Root.into(),
            DOMAIN,
            1,
            1,
            None,
        ));
        let stats = ArbitrationStats::<Test>::get();
        assert_eq!(stats.refund_count, 1);
    });
}

#[test]
fn arbitrate_partial_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::arbitrate(
            RawOrigin::Root.into(),
            DOMAIN,
            1,
            2,
            Some(5000),
        ));
        let stats = ArbitrationStats::<Test>::get();
        assert_eq!(stats.partial_count, 1);
    });
}

#[test]
fn arbitrate_not_disputed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Arbitration::arbitrate(RawOrigin::Root.into(), DOMAIN, 1, 0, None),
            Error::<Test>::NotDisputed
        );
    });
}

#[test]
fn arbitrate_requires_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert!(Arbitration::arbitrate(RuntimeOrigin::signed(1), DOMAIN, 1, 0, None,).is_err());
    });
}

#[test]
fn arbitrate_accepts_governance_account() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::arbitrate(
            RuntimeOrigin::signed(99),
            DOMAIN,
            1,
            0,
            None,
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

#[test]
fn arbitrate_rejects_non_governance_signed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_noop!(
            Arbitration::arbitrate(RuntimeOrigin::signed(1), DOMAIN, 1, 0, None,),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ==================== append_evidence_id (call_index 3) ====================

#[test]
fn append_evidence_id_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::append_evidence_id(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            100,
        ));
        let evidence_ids = EvidenceIds::<Test>::get(DOMAIN, 1);
        assert_eq!(evidence_ids.to_vec(), vec![42, 100]);
    });
}

#[test]
fn append_evidence_id_not_disputed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Arbitration::append_evidence_id(RuntimeOrigin::signed(1), DOMAIN, 1, 100,),
            Error::<Test>::NotDisputed
        );
    });
}

// ==================== file_complaint (call_index 10) ====================

#[test]
fn file_complaint_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            Some(500),
        ));
        let complaint = Complaints::<Test>::get(0).expect("complaint should exist");
        assert_eq!(complaint.complainant, 1);
        assert_eq!(complaint.respondent, 2);
        assert_eq!(complaint.status, ComplaintStatus::Submitted);
        assert!(ComplaintDeposits::<Test>::get(0).is_some());
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 1);
        let stats = DomainStats::<Test>::get(DOMAIN);
        assert_eq!(stats.total_complaints, 1);
        assert_eq!(notify_log(), vec![(2, b"complaint:filed:0".to_vec())]);
    });
}

#[test]
fn file_complaint_invalid_type_for_domain() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                1,
                ComplaintType::AdsReceiptDispute,
                cid(b"details"),
                None,
            ),
            Error::<Test>::InvalidComplaintType
        );
    });
}

#[test]
fn file_complaint_self_complaint_rejected() {
    new_test_ext().execute_with(|| {
        set_counterparty(Some(1));
        assert_noop!(
            Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                1,
                ComplaintType::OtcSellerNotDeliver,
                cid(b"details"),
                None,
            ),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn file_complaint_cooldown_enforced() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::settle_complaint(
            RuntimeOrigin::signed(1),
            0,
            cid(b"settlement"),
        ));

        // Cooldown is active: can't file again immediately
        assert_noop!(
            Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                1,
                ComplaintType::OtcSellerNotDeliver,
                cid(b"details2"),
                None,
            ),
            Error::<Test>::CooldownActive
        );

        // Advance past cooldown (ResponseDeadline=100)
        System::set_block_number(200);
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details3"),
            None,
        ));
    });
}

#[test]
fn file_complaint_rate_limit() {
    new_test_ext().execute_with(|| {
        // MaxActivePerUser = 50, file 50 complaints on different objects
        for i in 1..=50u64 {
            set_counterparty(Some(2));
            assert_ok!(Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                i,
                ComplaintType::OtcSellerNotDeliver,
                cid(b"details"),
                None,
            ));
        }
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 50);

        // 51st should fail
        assert_noop!(
            Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                51,
                ComplaintType::OtcSellerNotDeliver,
                cid(b"details"),
                None,
            ),
            Error::<Test>::TooManyActiveComplaints
        );
    });
}

// ==================== respond_to_complaint (call_index 11) ====================

#[test]
fn respond_to_complaint_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Responded);
        assert!(complaint.response_cid.is_some());
    });
}

#[test]
fn respond_to_complaint_not_respondent() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_noop!(
            Arbitration::respond_to_complaint(RuntimeOrigin::signed(3), 0, cid(b"response"),),
            Error::<Test>::NotAuthorized
        );
    });
}

#[test]
fn respond_to_complaint_past_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        System::set_block_number(200);
        assert_noop!(
            Arbitration::respond_to_complaint(RuntimeOrigin::signed(2), 0, cid(b"response"),),
            Error::<Test>::ResponseDeadlinePassed
        );
    });
}

// ==================== withdraw_complaint (call_index 12) ====================

#[test]
fn withdraw_complaint_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 1);

        assert_ok!(Arbitration::withdraw_complaint(RuntimeOrigin::signed(1), 0));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Withdrawn);
        assert!(ComplaintDeposits::<Test>::get(0).is_none());
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
    });
}

#[test]
fn withdraw_complaint_not_complainant() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_noop!(
            Arbitration::withdraw_complaint(RuntimeOrigin::signed(2), 0),
            Error::<Test>::NotAuthorized
        );
    });
}

// ==================== settle_complaint (call_index 13) ====================

#[test]
fn settle_complaint_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::settle_complaint(
            RuntimeOrigin::signed(1),
            0,
            cid(b"settlement"),
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedSettlement);
        assert!(complaint.settlement_cid.is_some());
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
    });
}

#[test]
fn respondent_cannot_settle() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_noop!(
            Arbitration::settle_complaint(RuntimeOrigin::signed(2), 0, cid(b"settlement"),),
            Error::<Test>::NotAuthorized
        );
    });
}

// ==================== escalate_to_arbitration (call_index 14) ====================

#[test]
fn escalate_to_arbitration_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0,
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Arbitrating);
        assert!(PendingArbitrationComplaints::<Test>::get(0).is_some());
    });
}

#[test]
fn escalate_wrong_status() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        // Submitted -> Arbitrating is now allowed, but only after response deadline
        assert_noop!(
            Arbitration::escalate_to_arbitration(RuntimeOrigin::signed(1), 0),
            Error::<Test>::ResponseDeadlineNotReached
        );
    });
}

// ==================== resolve_complaint (call_index 15) ====================

#[test]
fn resolve_complaint_complainant_wins() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"reason"),
            None,
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedComplainantWin);
        assert!(ComplaintDeposits::<Test>::get(0).is_none());
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
        assert_eq!(
            notify_log(),
            vec![
                (2, b"complaint:filed:0".to_vec()),
                (1, b"complaint:resolved:0:0".to_vec()),
                (2, b"complaint:resolved:0:0".to_vec()),
            ]
        );
    });
}

#[test]
fn resolve_complaint_respondent_wins_slashes_deposit() {
    new_test_ext().execute_with(|| {
        let balance_before = Balances::free_balance(1);
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        let balance_after_filing = Balances::free_balance(1);
        let deposit = balance_before - balance_after_filing;
        assert!(deposit > 0);

        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            1,
            cid(b"reason"),
            None,
        ));
        let balance_final = Balances::free_balance(1);
        assert!(balance_final < balance_before);
    });
}

// ==================== expire_old_complaints ====================

#[test]
fn expire_old_complaints_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        System::set_block_number(200);
        let expired = Arbitration::expire_old_complaints(10);
        assert_eq!(expired, 1);
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Expired);
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
    });
}

#[test]
fn stale_responded_complaint_expires() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        System::set_block_number(600);
        let expired = Arbitration::expire_old_complaints(10);
        assert_eq!(expired, 1);
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Expired);
    });
}

#[test]
fn stale_arbitrating_complaint_expires() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));

        System::set_block_number(400);
        assert_eq!(Arbitration::expire_old_complaints(10), 0);

        System::set_block_number(600);
        assert_eq!(Arbitration::expire_old_complaints(10), 1);
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Expired);
    });
}

// ==================== auto-escalation ====================

#[test]
fn auto_escalate_stale_responded() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));

        // Not yet stale
        System::set_block_number(100);
        assert_eq!(Arbitration::auto_escalate_stale_complaints(10), 0);

        // Past AutoEscalateBlocks=200 from updated_at
        System::set_block_number(250);
        assert_eq!(Arbitration::auto_escalate_stale_complaints(10), 1);
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Arbitrating);
        assert!(PendingArbitrationComplaints::<Test>::get(0).is_some());
    });
}

// ==================== archive_old_complaints ====================

#[test]
fn archive_old_complaints_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::withdraw_complaint(RuntimeOrigin::signed(1), 0));

        // Advance past both archive delay (50) and appeal window (50)
        System::set_block_number(100);
        let archived = Arbitration::archive_old_complaints(10);
        assert_eq!(archived, 1);
        assert!(Complaints::<Test>::get(0).is_none());
        assert!(ArchivedComplaints::<Test>::get(0).is_some());
    });
}

// ==================== archive cleanup: ComplaintEvidenceCids + ComplaintCooldown ====================

#[test]
fn archive_cleans_evidence_cids_and_cooldown() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::supplement_complaint_evidence(
            RuntimeOrigin::signed(1),
            0,
            cid(b"extra"),
        ));
        assert_eq!(ComplaintEvidenceCids::<Test>::get(0).len(), 1);

        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::settle_complaint(
            RuntimeOrigin::signed(1),
            0,
            cid(b"settlement"),
        ));

        // Cooldown should be set
        assert!(ComplaintCooldown::<Test>::get(DOMAIN, 1).is_some());

        // Advance past archive delay and appeal window
        System::set_block_number(200);
        let archived = Arbitration::archive_old_complaints(10);
        assert_eq!(archived, 1);
        assert!(Complaints::<Test>::get(0).is_none());
        assert!(ArchivedComplaints::<Test>::get(0).is_some());

        // Storage cleaned up
        assert_eq!(ComplaintEvidenceCids::<Test>::get(0).len(), 0);
        assert!(ComplaintCooldown::<Test>::get(DOMAIN, 1).is_none());
    });
}

// ==================== ComplaintType domain matching ====================

#[test]
fn complaint_type_domain_mapping() {
    assert_eq!(ComplaintType::EntityOrderNotDeliver.domain(), *b"entorder");
    assert_eq!(ComplaintType::OtcSellerNotDeliver.domain(), *b"otc_ord_");
    assert_eq!(ComplaintType::MakerCreditDefault.domain(), *b"maker___");
    assert_eq!(ComplaintType::AdsReceiptDispute.domain(), *b"ads_____");
    assert_eq!(ComplaintType::TokenSaleNotDeliver.domain(), *b"toknsale");
    assert_eq!(ComplaintType::Other.domain(), *b"other___");
}

#[test]
fn complaint_status_is_resolved() {
    assert!(ComplaintStatus::ResolvedComplainantWin.is_resolved());
    assert!(ComplaintStatus::ResolvedRespondentWin.is_resolved());
    assert!(ComplaintStatus::ResolvedSettlement.is_resolved());
    assert!(ComplaintStatus::Withdrawn.is_resolved());
    assert!(ComplaintStatus::Expired.is_resolved());
    assert!(!ComplaintStatus::Submitted.is_resolved());
    assert!(!ComplaintStatus::Responded.is_resolved());
    assert!(!ComplaintStatus::Arbitrating.is_resolved());
    assert!(!ComplaintStatus::Appealed.is_resolved());
}

// ==================== Evidence on-chain tracking ====================

#[test]
fn supplement_evidence_stores_on_chain() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::supplement_complaint_evidence(
            RuntimeOrigin::signed(1),
            0,
            cid(b"extra_evidence"),
        ));
        let cids = ComplaintEvidenceCids::<Test>::get(0);
        assert_eq!(cids.len(), 1);
        assert_eq!(cids[0].to_vec(), b"extra_evidence");

        // Respondent can supplement after responding
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::supplement_response_evidence(
            RuntimeOrigin::signed(2),
            0,
            cid(b"counter_evidence"),
        ));
        let cids = ComplaintEvidenceCids::<Test>::get(0);
        assert_eq!(cids.len(), 2);
    });
}

// ==================== Appeal mechanism ====================

#[test]
fn appeal_by_losing_respondent() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"reason"),
            None,
        ));

        // Respondent lost (decision=0 = complainant wins) -> can appeal
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedComplainantWin);

        assert_ok!(Arbitration::appeal(
            RuntimeOrigin::signed(2),
            0,
            cid(b"appeal_reason"),
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Appealed);
        assert!(complaint.appeal_cid.is_some());
        assert!(PendingArbitrationComplaints::<Test>::get(0).is_some());
    });
}

#[test]
fn appeal_by_winning_party_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"reason"),
            None,
        ));

        // Complainant won -> cannot appeal
        assert_noop!(
            Arbitration::appeal(RuntimeOrigin::signed(1), 0, cid(b"appeal")),
            Error::<Test>::CannotAppeal
        );
    });
}

#[test]
fn appeal_window_expired() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"reason"),
            None,
        ));

        // Advance past appeal window (AppealWindowBlocks=50)
        System::set_block_number(200);
        assert_noop!(
            Arbitration::appeal(RuntimeOrigin::signed(2), 0, cid(b"appeal")),
            Error::<Test>::AppealWindowClosed
        );
    });
}

#[test]
fn resolve_appeal_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"reason"),
            None,
        ));

        // After resolve_complaint, ActiveComplaintCount should be 0
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);

        assert_ok!(Arbitration::appeal(
            RuntimeOrigin::signed(2),
            0,
            cid(b"appeal")
        ));

        // Appeal re-increments ActiveComplaintCount
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 1);

        // Verify appellant is recorded
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.appellant, Some(2));

        assert_ok!(Arbitration::resolve_appeal(
            RawOrigin::Root.into(),
            0,
            1,
            cid(b"appeal_reason"),
        ));

        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedRespondentWin);

        // ActiveComplaintCount decremented again after appeal resolution
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);

        // DomainStats updated by resolve_appeal
        let stats = DomainStats::<Test>::get(DOMAIN);
        assert!(stats.resolved_count >= 2);
        assert!(stats.respondent_wins >= 1);
    });
}

#[test]
fn resolve_appeal_complainant_as_appellant() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        // Respondent wins -> complainant is the loser
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            1,
            cid(b"reason"),
            None,
        ));

        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedRespondentWin);

        // Complainant (loser) appeals
        assert_ok!(Arbitration::appeal(
            RuntimeOrigin::signed(1),
            0,
            cid(b"appeal")
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.appellant, Some(1));

        // Appeal overturns: complainant wins
        assert_ok!(Arbitration::resolve_appeal(
            RawOrigin::Root.into(),
            0,
            0,
            cid(b"appeal_reason"),
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::ResolvedComplainantWin);
    });
}

// ==================== Penalty rate tests ====================

#[test]
fn domain_penalty_rate_overrides() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::set_domain_penalty_rate(
            RawOrigin::Root.into(),
            DOMAIN,
            Some(8000),
        ));
        let balance_before = Balances::free_balance(1);
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        let deposit = balance_before - Balances::free_balance(1);
        assert!(deposit > 0);

        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));

        let respondent_before = Balances::free_balance(2);
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            1,
            cid(b"reason"),
            None,
        ));
        let slashed_to_respondent = Balances::free_balance(2) - respondent_before;
        let expected_slash = sp_runtime::Permill::from_parts(8000u32 * 100).mul_floor(deposit);
        assert_eq!(slashed_to_respondent, expected_slash);
    });
}

#[test]
fn per_type_penalty_rate_used() {
    new_test_ext().execute_with(|| {
        let balance_before = Balances::free_balance(1);
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcTradeFraud,
            cid(b"details"),
            None,
        ));
        let deposit = balance_before - Balances::free_balance(1);

        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        let respondent_before = Balances::free_balance(2);
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            1,
            cid(b"reason"),
            None,
        ));
        let slashed_to_respondent = Balances::free_balance(2) - respondent_before;
        let expected_slash = sp_runtime::Permill::from_parts(8000u32 * 100).mul_floor(deposit);
        assert_eq!(slashed_to_respondent, expected_slash);
    });
}

// ==================== Partial ruling stats ====================

#[test]
fn partial_ruling_counts_as_complainant_win() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        assert_ok!(Arbitration::resolve_complaint(
            RawOrigin::Root.into(),
            0,
            2,
            cid(b"reason"),
            Some(5000),
        ));

        let stats = DomainStats::<Test>::get(DOMAIN);
        assert_eq!(stats.resolved_count, 1);
        assert_eq!(stats.complainant_wins, 1);
        assert_eq!(stats.settlements, 0);
    });
}

// ==================== Module pause ====================

#[test]
fn paused_blocks_operations() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::set_paused(RawOrigin::Root.into(), true));
        assert_noop!(
            Arbitration::file_complaint(
                RuntimeOrigin::signed(1),
                DOMAIN,
                1,
                ComplaintType::OtcSellerNotDeliver,
                cid(b"details"),
                None,
            ),
            Error::<Test>::ModulePaused
        );
    });
}

// ==================== Force close ====================

#[test]
fn force_close_complaint_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 1);

        assert_ok!(Arbitration::force_close_complaint(
            RawOrigin::Root.into(),
            0
        ));
        let complaint = Complaints::<Test>::get(0).unwrap();
        assert_eq!(complaint.status, ComplaintStatus::Withdrawn);
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
    });
}

// ==================== Dismiss complaint ====================

#[test]
fn dismiss_complaint_slashes_and_cleans() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 1);

        assert_ok!(Arbitration::dismiss_complaint(RawOrigin::Root.into(), 0));
        assert_eq!(ActiveComplaintCount::<Test>::get(1), 0);
        let stats = DomainStats::<Test>::get(DOMAIN);
        assert_eq!(stats.respondent_wins, 1);
    });
}

// ==================== respond_to_dispute ====================

#[test]
fn respond_to_dispute_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        let record = TwoWayDeposits::<Test>::get(DOMAIN, 1).unwrap();
        assert!(!record.has_responded);

        assert_ok!(Arbitration::respond_to_dispute(
            RuntimeOrigin::signed(2),
            DOMAIN,
            1,
            43,
        ));
        let record = TwoWayDeposits::<Test>::get(DOMAIN, 1).unwrap();
        assert!(record.has_responded);
        assert!(record.respondent_deposit.is_some());
        assert_eq!(EvidenceIds::<Test>::get(DOMAIN, 1).len(), 2);
    });
}

#[test]
fn respond_to_dispute_wrong_respondent() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_noop!(
            Arbitration::respond_to_dispute(RuntimeOrigin::signed(3), DOMAIN, 1, 43),
            Error::<Test>::NotDisputed
        );
    });
}

#[test]
fn respond_to_dispute_after_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        System::set_block_number(200);
        assert_noop!(
            Arbitration::respond_to_dispute(RuntimeOrigin::signed(2), DOMAIN, 1, 43),
            Error::<Test>::ResponseDeadlinePassed
        );
    });
}

// ==================== settle_dispute ====================

#[test]
fn settle_dispute_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::respond_to_dispute(
            RuntimeOrigin::signed(2),
            DOMAIN,
            1,
            43,
        ));
        assert_ok!(Arbitration::settle_dispute(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
        assert!(TwoWayDeposits::<Test>::get(DOMAIN, 1).is_none());
    });
}

#[test]
fn settle_dispute_requires_response() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_noop!(
            Arbitration::settle_dispute(RuntimeOrigin::signed(1), DOMAIN, 1),
            Error::<Test>::SettlementNotConfirmed
        );
    });
}

// ==================== dismiss_dispute ====================

#[test]
fn dismiss_dispute_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::dismiss_dispute(
            RawOrigin::Root.into(),
            DOMAIN,
            1
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

// ==================== request_default_judgment ====================

#[test]
fn request_default_judgment_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        System::set_block_number(200);
        assert_ok!(Arbitration::request_default_judgment(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

#[test]
fn request_default_judgment_before_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_noop!(
            Arbitration::request_default_judgment(RuntimeOrigin::signed(1), DOMAIN, 1),
            Error::<Test>::ResponseDeadlineNotReached
        );
    });
}

// ==================== force_close_dispute ====================

#[test]
fn force_close_dispute_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::respond_to_dispute(
            RuntimeOrigin::signed(2),
            DOMAIN,
            1,
            43,
        ));
        assert_ok!(Arbitration::force_close_dispute(
            RawOrigin::Root.into(),
            DOMAIN,
            1
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
        assert!(TwoWayDeposits::<Test>::get(DOMAIN, 1).is_none());
    });
}

// ==================== start_mediation ====================

#[test]
fn start_mediation_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_ok!(Arbitration::respond_to_complaint(
            RuntimeOrigin::signed(2),
            0,
            cid(b"response"),
        ));
        assert_ok!(Arbitration::start_mediation(RawOrigin::Root.into(), 0));
        let c = Complaints::<Test>::get(0).unwrap();
        assert_eq!(c.status, ComplaintStatus::Mediating);
    });
}

#[test]
fn start_mediation_invalid_state() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_noop!(
            Arbitration::start_mediation(RawOrigin::Root.into(), 0),
            Error::<Test>::InvalidState
        );
    });
}

// ==================== arbitrate invalid decision_code ====================

#[test]
fn arbitrate_invalid_decision_code_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_noop!(
            Arbitration::arbitrate(RawOrigin::Root.into(), DOMAIN, 1, 5, None),
            Error::<Test>::InvalidDecisionCode
        );
    });
}

#[test]
fn arbitrate_partial_without_bps_uses_default() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::dispute_with_two_way_deposit(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            42,
        ));
        assert_ok!(Arbitration::arbitrate(
            RawOrigin::Root.into(),
            DOMAIN,
            1,
            2,
            None
        ));
        assert!(Disputed::<Test>::get(DOMAIN, 1).is_none());
    });
}

// ==================== Submitted -> Arbitrating escalation ====================

#[test]
fn escalate_submitted_after_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        System::set_block_number(200);
        assert_ok!(Arbitration::escalate_to_arbitration(
            RuntimeOrigin::signed(1),
            0
        ));
        let c = Complaints::<Test>::get(0).unwrap();
        assert_eq!(c.status, ComplaintStatus::Arbitrating);
    });
}

#[test]
fn escalate_submitted_before_deadline_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(Arbitration::file_complaint(
            RuntimeOrigin::signed(1),
            DOMAIN,
            1,
            ComplaintType::OtcSellerNotDeliver,
            cid(b"details"),
            None,
        ));
        assert_noop!(
            Arbitration::escalate_to_arbitration(RuntimeOrigin::signed(1), 0),
            Error::<Test>::ResponseDeadlineNotReached
        );
    });
}
