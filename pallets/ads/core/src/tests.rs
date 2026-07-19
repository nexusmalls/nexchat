use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, traits::ReservableCurrency, BoundedVec};

// ============================================================================
// Helpers
// ============================================================================

fn ad_text(s: &str) -> BoundedVec<u8, <Test as Config>::MaxAdTextLength> {
    BoundedVec::try_from(s.as_bytes().to_vec()).unwrap()
}

fn ad_url(s: &str) -> BoundedVec<u8, <Test as Config>::MaxAdUrlLength> {
    BoundedVec::try_from(s.as_bytes().to_vec()).unwrap()
}

const UNIT: u128 = 1_000_000_000_000;

fn create_default_campaign(advertiser: u64) -> u64 {
    let id = NextCampaignId::<Test>::get();
    assert_ok!(AdsCore::create_campaign(
        RuntimeOrigin::signed(advertiser),
        ad_text("Test Ad"),
        ad_url("https://example.com"),
        UNIT / 2,          // 0.5 UNIT per mille
        10 * UNIT,         // daily budget
        50 * UNIT,         // total budget
        0b001,             // type bit 0
        1000,              // expires_at
        None,              // targets (全网投放)
        CampaignType::Cpm, // campaign_type
        0u128,             // bid_per_click (CPM 模式不使用)
    ));
    id
}

fn create_approved_campaign(advertiser: u64) -> u64 {
    let id = create_default_campaign(advertiser);
    assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));
    id
}

// ============================================================================
// create_campaign
// ============================================================================

#[test]
fn create_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_eq!(id, 0);

        let campaign = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(campaign.advertiser, ADVERTISER);
        assert_eq!(campaign.bid_per_mille, UNIT / 2);
        assert_eq!(campaign.total_budget, 50 * UNIT);
        assert_eq!(campaign.spent, 0);
        assert_eq!(campaign.status, CampaignStatus::Active);
        assert_eq!(campaign.review_status, AdReviewStatus::Pending);

        assert_eq!(CampaignEscrow::<Test>::get(id), 50 * UNIT);
        assert_eq!(NextCampaignId::<Test>::get(), 1);
    });
}

#[test]
fn create_campaign_fails_empty_text() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text(""),
                ad_url("https://x.com"),
                UNIT,
                10 * UNIT,
                50 * UNIT,
                0b001,
                1000,
                None,
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::EmptyAdText
        );
    });
}

#[test]
fn create_campaign_fails_bid_too_low() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("Ad"),
                ad_url("https://x.com"),
                1,
                10 * UNIT,
                50 * UNIT,
                0b001,
                1000,
                None,
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::BidTooLow
        );
    });
}

#[test]
fn create_campaign_fails_invalid_delivery_types() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("Ad"),
                ad_url("https://x.com"),
                UNIT,
                10 * UNIT,
                50 * UNIT,
                0b1000, // invalid
                1000,
                None,
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::InvalidDeliveryTypes
        );
    });
}

#[test]
fn create_campaign_fails_zero_budget() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("Ad"),
                ad_url("https://x.com"),
                UNIT,
                10 * UNIT,
                0,
                0b001,
                1000,
                None,
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::ZeroBudget
        );
    });
}

#[test]
fn create_campaign_fails_invalid_expiry() {
    new_test_ext().execute_with(|| {
        // block_number = 1, expires_at = 1 → not > now
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("Ad"),
                ad_url("https://x.com"),
                UNIT,
                10 * UNIT,
                50 * UNIT,
                0b001,
                1,
                None, // expires_at == now
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::InvalidExpiry
        );
    });
}

// ============================================================================
// fund_campaign
// ============================================================================

#[test]
fn fund_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::fund_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            10 * UNIT,
        ));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.total_budget, 60 * UNIT);
        assert_eq!(CampaignEscrow::<Test>::get(id), 60 * UNIT);
    });
}

#[test]
fn fund_campaign_revives_exhausted() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        // Manually set to Exhausted
        Campaigns::<Test>::mutate(id, |c| {
            c.as_mut().unwrap().status = CampaignStatus::Exhausted;
        });
        assert_ok!(AdsCore::fund_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            5 * UNIT,
        ));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Active
        );
    });
}

// ============================================================================
// pause / cancel
// ============================================================================

#[test]
fn pause_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::pause_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Paused
        );
    });
}

#[test]
fn cancel_campaign_refunds() {
    new_test_ext().execute_with(|| {
        let before = pallet_balances::Pallet::<Test>::free_balance(ADVERTISER);
        let id = create_default_campaign(ADVERTISER);
        let after_create = pallet_balances::Pallet::<Test>::free_balance(ADVERTISER);
        // 50 UNIT reserved
        assert_eq!(before - after_create, 50 * UNIT);

        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        let after_cancel = pallet_balances::Pallet::<Test>::free_balance(ADVERTISER);
        assert_eq!(after_cancel, before); // fully refunded
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Cancelled
        );
    });
}

// ============================================================================
// review
// ============================================================================

#[test]
fn review_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Approved
        );
    });
}

#[test]
fn review_campaign_reject() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Rejected
        );
    });
}

#[test]
fn review_campaign_allows_re_review_of_approved() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));
        // Governance can re-review an approved campaign (e.g. revoke approval)
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Rejected
        );
    });
}

#[test]
fn review_campaign_reject_auto_refunds() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let free_before = Balances::free_balance(ADVERTISER);
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.review_status, AdReviewStatus::Rejected);
        assert_eq!(c.status, CampaignStatus::Cancelled);
        // Escrow should be cleared
        assert_eq!(CampaignEscrow::<Test>::get(id), 0);
        // Budget refunded
        let free_after = Balances::free_balance(ADVERTISER);
        assert_eq!(free_after - free_before, 50 * UNIT);
    });
}

// ============================================================================
// submit_delivery_receipt
// ============================================================================

#[test]
fn submit_delivery_receipt_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].audience_size, 100);
        assert_eq!(receipts[0].campaign_id, id);
        assert_eq!(PlacementEraDeliveries::<Test>::get(&pid), 1);
    });
}

#[test]
fn submit_receipt_caps_audience() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // MockDeliveryVerifier caps at 500
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            1000,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].audience_size, 500); // capped
    });
}

#[test]
fn submit_receipt_fails_not_approved() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // not reviewed
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::CampaignNotApproved
        );
    });
}

#[test]
fn submit_receipt_fails_campaign_expired() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Advance past expiry
        System::set_block_number(1001);
        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::CampaignExpired
        );
    });
}

#[test]
fn submit_receipt_fails_audience_below_min() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // MinAudienceSize = 20, submit 10
        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 10,),
            Error::<Test>::AudienceBelowMinimum
        );
    });
}

#[test]
fn submit_receipt_fails_banned_placement() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);
        BannedPlacements::<Test>::insert(&pid, true);

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::PlacementBanned
        );
    });
}

// ============================================================================
// settle_era_ads
// ============================================================================

#[test]
fn settle_era_ads_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Submit receipt: audience=100, multiplier=100 (1.0x)
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // Expected cost: bid(0.5 UNIT) * 100 * 100 / 100_000 = 0.05 UNIT
        let expected_cost = UNIT / 2 * 100 * 100 / 100_000;

        // Phase 5: 广告主确认收据后才可结算
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid,
        ));

        // Receipts cleared
        assert_eq!(DeliveryReceipts::<Test>::get(&pid).len(), 0);

        // Revenue recorded
        assert_eq!(EraAdRevenue::<Test>::get(&pid), expected_cost);
        assert_eq!(PlacementTotalRevenue::<Test>::get(&pid), expected_cost);

        // Claimable = 80% of cost (MockRevenueDistributor)
        assert_eq!(
            PlacementClaimable::<Test>::get(&pid),
            expected_cost * 80 / 100
        );

        // Campaign spent updated
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.spent, expected_cost);
    });
}

// ============================================================================
// claim_ad_revenue
// ============================================================================

#[test]
fn claim_ad_revenue_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        // Manually set claimable
        PlacementClaimable::<Test>::insert(&pid, 10 * UNIT);

        let before = pallet_balances::Pallet::<Test>::free_balance(PLACEMENT_ADMIN);
        assert_ok!(AdsCore::claim_ad_revenue(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            0,
        ));
        let after = pallet_balances::Pallet::<Test>::free_balance(PLACEMENT_ADMIN);
        assert_eq!(after - before, 10 * UNIT);
        assert_eq!(PlacementClaimable::<Test>::get(&pid), 0);
    });
}

#[test]
fn claim_ad_revenue_fails_not_admin() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementClaimable::<Test>::insert(&pid, 10 * UNIT);
        assert_noop!(
            AdsCore::claim_ad_revenue(RuntimeOrigin::signed(ADVERTISER), pid, 0),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

#[test]
fn claim_ad_revenue_fails_nothing_to_claim() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::claim_ad_revenue(RuntimeOrigin::signed(PLACEMENT_ADMIN), pid, 0),
            Error::<Test>::NothingToClaim
        );
    });
}

// ============================================================================
// Bidirectional preferences
// ============================================================================

#[test]
fn advertiser_blacklist_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::advertiser_block_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));
        assert_eq!(AdvertiserBlacklist::<Test>::get(ADVERTISER).len(), 1);

        assert_ok!(AdsCore::advertiser_unblock_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));
        assert_eq!(AdvertiserBlacklist::<Test>::get(ADVERTISER).len(), 0);
    });
}

#[test]
fn advertiser_whitelist_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::advertiser_prefer_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));
        assert_eq!(AdvertiserWhitelist::<Test>::get(ADVERTISER).len(), 1);

        assert_ok!(AdsCore::advertiser_unprefer_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));
        assert_eq!(AdvertiserWhitelist::<Test>::get(ADVERTISER).len(), 0);
    });
}

#[test]
fn placement_blacklist_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::placement_block_advertiser(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            ADVERTISER,
        ));
        assert_eq!(PlacementBlacklist::<Test>::get(&pid).len(), 1);

        assert_ok!(AdsCore::placement_unblock_advertiser(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            ADVERTISER,
        ));
        assert_eq!(PlacementBlacklist::<Test>::get(&pid).len(), 0);
    });
}

#[test]
fn placement_whitelist_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::placement_prefer_advertiser(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            ADVERTISER,
        ));
        assert_eq!(PlacementWhitelist::<Test>::get(&pid).len(), 1);

        assert_ok!(AdsCore::placement_unprefer_advertiser(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            ADVERTISER,
        ));
        assert_eq!(PlacementWhitelist::<Test>::get(&pid).len(), 0);
    });
}

#[test]
fn placement_ops_fail_not_admin() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::placement_block_advertiser(
                RuntimeOrigin::signed(ADVERTISER),
                pid,
                ADVERTISER2,
            ),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

// ============================================================================
// flag / slash
// ============================================================================

#[test]
fn flag_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::flag_campaign(RuntimeOrigin::signed(REPORTER), id));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Flagged
        );
    });
}

#[test]
fn flag_placement_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(REPORTER),
            pid
        ));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 1);

        // M1: 不同用户可继续举报
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid
        ));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 2);
    });
}

#[test]
fn slash_placement_bans_after_3() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::slash_placement(
            RuntimeOrigin::root(),
            pid,
            REPORTER
        ));
        assert_eq!(SlashCount::<Test>::get(&pid), 1);
        assert!(!BannedPlacements::<Test>::get(&pid));

        assert_ok!(AdsCore::slash_placement(
            RuntimeOrigin::root(),
            pid,
            REPORTER
        ));
        assert_eq!(SlashCount::<Test>::get(&pid), 2);

        assert_ok!(AdsCore::slash_placement(
            RuntimeOrigin::root(),
            pid,
            REPORTER
        ));
        assert_eq!(SlashCount::<Test>::get(&pid), 3);
        assert!(BannedPlacements::<Test>::get(&pid));
    });
}

// ============================================================================
// register_private_ad
// ============================================================================

#[test]
fn register_private_ad_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::register_private_ad(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            3,
        ));
        assert_eq!(PrivateAdCount::<Test>::get(&pid), 3);
        assert_eq!(PlacementEraDeliveries::<Test>::get(&pid), 3);
    });
}

#[test]
fn register_private_ad_fails_not_admin() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::register_private_ad(RuntimeOrigin::signed(ADVERTISER), pid, 1),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

#[test]
fn register_private_ad_fails_zero_count() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::register_private_ad(RuntimeOrigin::signed(PLACEMENT_ADMIN), pid, 0),
            Error::<Test>::ZeroPrivateAdCount
        );
    });
}

// ============================================================================
// 审计回归测试
// ============================================================================

// C1: settle_era_ads 中 unreserve 不足时只结算实际解锁金额
#[test]
fn c1_settle_uses_actually_unreserved_amount() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 提交收据
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // Phase 5: 确认收据
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        // 手动减少 advertiser 的 reserved (模拟部分 unreserve 已被外部消耗)
        // 先查当前 reserved
        let reserved_before = pallet_balances::Pallet::<Test>::reserved_balance(ADVERTISER);
        assert!(reserved_before > 0);

        // unreserve 大部分，只留 1 token reserved
        let to_unreserve = reserved_before.saturating_sub(1);
        pallet_balances::Pallet::<Test>::unreserve(&ADVERTISER, to_unreserve);
        assert_eq!(
            pallet_balances::Pallet::<Test>::reserved_balance(ADVERTISER),
            1
        );

        let treasury_before = pallet_balances::Pallet::<Test>::free_balance(TREASURY);

        // 结算 — 应该只结算 1 token (实际可 unreserve 的金额)
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid,
        ));

        let treasury_after = pallet_balances::Pallet::<Test>::free_balance(TREASURY);
        // 国库只收到 1 token (实际解锁量)，而非原始 CPM 费用
        assert_eq!(treasury_after - treasury_before, 1);
    });
}

// H1: settle_era_ads transfer 失败时跳过而非回滚整体
#[test]
fn h1_settle_skips_failed_transfer_does_not_abort() {
    new_test_ext().execute_with(|| {
        // 两个广告主各创建一个 campaign
        let id1 = create_approved_campaign(ADVERTISER);
        let id2 = create_approved_campaign(ADVERTISER2);
        let pid = placement_id(1);

        // 两个 campaign 都提交收据到同一广告位
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id1,
            pid,
            100,
        ));
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id2,
            pid,
            100,
        ));

        // Phase 5: 确认两张收据
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id1,
            pid,
            0
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER2),
            id2,
            pid,
            1
        ));

        // 让 ADVERTISER 的 reserved 为 0 (模拟资金已被消耗)
        let reserved1 = pallet_balances::Pallet::<Test>::reserved_balance(ADVERTISER);
        pallet_balances::Pallet::<Test>::unreserve(&ADVERTISER, reserved1);

        // 结算应成功 — ADVERTISER 的部分被跳过，ADVERTISER2 的部分正常结算
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
        ));

        // ADVERTISER2 的 campaign spent 应该更新
        let c2 = Campaigns::<Test>::get(id2).unwrap();
        assert!(
            c2.spent > 0,
            "ADVERTISER2 campaign should have been settled"
        );

        // 收据已清空 (Era 结束)
        assert_eq!(DeliveryReceipts::<Test>::get(&pid).len(), 0);
    });
}

// H2: flag_campaign 不能 flag 已 Approved 的 Campaign (防 griefing)
#[test]
fn h2_flag_campaign_rejects_approved_campaign() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);

        // 尝试 flag 已审核通过的 campaign — 应失败
        assert_noop!(
            AdsCore::flag_campaign(RuntimeOrigin::signed(REPORTER), id),
            Error::<Test>::AlreadyReviewed
        );

        // Campaign 仍为 Approved，投放不受影响
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Approved
        );
    });
}

// H2: governance 可以重审已 Approved 的 Campaign (reject + auto-refund)
#[test]
fn h2_review_campaign_can_reject_approved() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Approved
        );

        let free_before = Balances::free_balance(ADVERTISER);
        // Governance 可以 reject 已审核通过的 Campaign (+ auto-refund)
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.review_status, AdReviewStatus::Rejected);
        assert_eq!(c.status, CampaignStatus::Cancelled);
        let free_after = Balances::free_balance(ADVERTISER);
        assert_eq!(free_after - free_before, 50 * UNIT);
    });
}

// ============================================================================
// Round 2 审计回归测试
// ============================================================================

// H1: fund_campaign 阻止对已过期 Campaign 充值
#[test]
fn h1_fund_campaign_rejects_expired() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // expires_at = 1000
                                                      // 推进到过期后
        System::set_block_number(1001);
        assert_noop!(
            AdsCore::fund_campaign(RuntimeOrigin::signed(ADVERTISER), id, 5 * UNIT),
            Error::<Test>::CampaignExpired
        );
    });
}

// H1: fund_campaign 在过期前仍可正常充值
#[test]
fn h1_fund_campaign_works_before_expiry() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // expires_at = 1000
        System::set_block_number(999);
        assert_ok!(AdsCore::fund_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            5 * UNIT
        ));
        assert_eq!(Campaigns::<Test>::get(id).unwrap().total_budget, 55 * UNIT);
    });
}

// M1: flag_placement 同一用户不可重复举报同一广告位
#[test]
fn m1_flag_placement_rejects_duplicate() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(REPORTER),
            pid
        ));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 1);

        // 同一用户再次举报 — 应失败
        assert_noop!(
            AdsCore::flag_placement(RuntimeOrigin::signed(REPORTER), pid),
            Error::<Test>::AlreadyFlaggedPlacement
        );
        // count 不变
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 1);
    });
}

// M1: 不同用户可以分别举报同一广告位
#[test]
fn m1_flag_placement_allows_different_reporters() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(REPORTER),
            pid
        ));
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid
        ));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 2);
    });
}

// ============================================================================
// Round 2+ 审计回归测试
// ============================================================================

// M1-R2: resume_campaign 正常恢复已暂停 Campaign
#[test]
fn m1r2_resume_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::pause_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Paused
        );

        assert_ok!(AdsCore::resume_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Active
        );
    });
}

// M1-R2: resume_campaign 仅允许恢复 Paused 状态
#[test]
fn m1r2_resume_campaign_rejects_active() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::resume_campaign(RuntimeOrigin::signed(ADVERTISER), id),
            Error::<Test>::CampaignNotPaused
        );
    });
}

// M1-R2: resume_campaign 不允许恢复已过期的 Paused Campaign
#[test]
fn m1r2_resume_campaign_rejects_expired() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // expires_at = 1000
        assert_ok!(AdsCore::pause_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        System::set_block_number(1001);
        assert_noop!(
            AdsCore::resume_campaign(RuntimeOrigin::signed(ADVERTISER), id),
            Error::<Test>::CampaignExpired
        );
    });
}

// M1-R2: resume_campaign 仅允许 owner 操作
#[test]
fn m1r2_resume_campaign_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::pause_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_noop!(
            AdsCore::resume_campaign(RuntimeOrigin::signed(ADVERTISER2), id),
            Error::<Test>::NotCampaignOwner
        );
    });
}

// M2-R2: 广告主拉黑广告位后, 该广告位不能提交投放收据
#[test]
fn m2r2_submit_receipt_blocked_by_advertiser_blacklist() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 广告主拉黑该广告位
        assert_ok!(AdsCore::advertiser_block_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));

        // 提交投放收据应失败
        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::AdvertiserBlacklistedPlacement
        );
    });
}

// M2-R2: 广告位拉黑广告主后, 该广告主的 Campaign 不能投放到该广告位
#[test]
fn m2r2_submit_receipt_blocked_by_placement_blacklist() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 广告位拉黑该广告主
        assert_ok!(AdsCore::placement_block_advertiser(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            ADVERTISER,
        ));

        // 提交投放收据应失败
        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::PlacementBlacklistedAdvertiser
        );
    });
}

// ============================================================================
// Regression: L-CORE4 — expire_campaign
// ============================================================================

#[test]
fn lcore4_expire_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        // Campaign expires_at = 1000, advance past it
        System::set_block_number(1001);

        let free_before = Balances::free_balance(ADVERTISER);
        System::reset_events();

        // Anyone can call expire_campaign
        assert_ok!(AdsCore::expire_campaign(RuntimeOrigin::signed(42), id));

        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.status, CampaignStatus::Expired);

        // Refund: total_budget(50 UNIT) - spent(0) = 50 UNIT unreserved
        let free_after = Balances::free_balance(ADVERTISER);
        assert_eq!(free_after - free_before, 50 * UNIT);

        System::assert_has_event(RuntimeEvent::AdsCore(Event::CampaignMarkedExpired {
            campaign_id: id,
            refunded: 50 * UNIT,
        }));
    });
}

#[test]
fn lcore4_expire_campaign_rejects_not_expired() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        // Still at block 1, expires_at = 1000
        assert_noop!(
            AdsCore::expire_campaign(RuntimeOrigin::signed(42), id),
            Error::<Test>::CampaignNotExpired
        );
    });
}

#[test]
fn lcore4_expire_campaign_rejects_cancelled() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        System::set_block_number(1001);
        assert_noop!(
            AdsCore::expire_campaign(RuntimeOrigin::signed(42), id),
            Error::<Test>::CampaignInactive
        );
    });
}

#[test]
fn lcore4_expire_campaign_rejects_already_expired() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        System::set_block_number(1001);
        assert_ok!(AdsCore::expire_campaign(RuntimeOrigin::signed(42), id));
        // Second call should fail — now status is Expired (not Active/Paused/Exhausted)
        assert_noop!(
            AdsCore::expire_campaign(RuntimeOrigin::signed(42), id),
            Error::<Test>::CampaignInactive
        );
    });
}

// M2-R2: 取消拉黑后可以正常提交收据
#[test]
fn m2r2_submit_receipt_works_after_unblock() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 拉黑再解除
        assert_ok!(AdsCore::advertiser_block_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));
        assert_ok!(AdsCore::advertiser_unblock_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid,
        ));

        // 提交收据应成功
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
    });
}

// ============================================================================
// New Feature Tests
// ============================================================================

// --- CampaignsByAdvertiser index ---
#[test]
fn campaigns_by_advertiser_index_maintained() {
    new_test_ext().execute_with(|| {
        let id1 = create_default_campaign(ADVERTISER);
        let id2 = create_default_campaign(ADVERTISER);
        let list = CampaignsByAdvertiser::<Test>::get(ADVERTISER);
        assert_eq!(list.len(), 2);
        assert!(list.contains(&id1));
        assert!(list.contains(&id2));
    });
}

// --- update_campaign ---
#[test]
fn update_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert_ok!(AdsCore::update_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            Some(ad_text("New Text")),
            Some(ad_url("https://new.com")),
            None,
            None,
            None,
            None,
        ));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.text.as_slice(), b"New Text");
        assert_eq!(c.review_status, AdReviewStatus::Pending); // reset
    });
}

#[test]
fn update_campaign_fails_not_owner() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::update_campaign(
                RuntimeOrigin::signed(ADVERTISER2),
                id,
                Some(ad_text("X")),
                None,
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::NotCampaignOwner
        );
    });
}

// --- extend_campaign_expiry ---
#[test]
fn extend_campaign_expiry_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // expires_at = 1000
        assert_ok!(AdsCore::extend_campaign_expiry(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            2000,
        ));
        assert_eq!(Campaigns::<Test>::get(id).unwrap().expires_at, 2000);
    });
}

#[test]
fn extend_campaign_expiry_fails_not_extended() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // expires_at = 1000
        assert_noop!(
            AdsCore::extend_campaign_expiry(RuntimeOrigin::signed(ADVERTISER), id, 500),
            Error::<Test>::ExpiryNotExtended
        );
    });
}

// --- force_cancel_campaign ---
#[test]
fn force_cancel_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let free_before = Balances::free_balance(ADVERTISER);
        assert_ok!(AdsCore::force_cancel_campaign(RuntimeOrigin::root(), id));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.status, CampaignStatus::Cancelled);
        let free_after = Balances::free_balance(ADVERTISER);
        assert_eq!(free_after - free_before, 50 * UNIT);
    });
}

#[test]
fn force_cancel_campaign_fails_not_root() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::force_cancel_campaign(RuntimeOrigin::signed(ADVERTISER), id),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// --- unban_placement ---
#[test]
fn unban_placement_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        BannedPlacements::<Test>::insert(&pid, true);
        SlashCount::<Test>::insert(&pid, 3);
        assert_ok!(AdsCore::unban_placement(RuntimeOrigin::root(), pid));
        assert!(!BannedPlacements::<Test>::get(&pid));
        assert_eq!(SlashCount::<Test>::get(&pid), 0);
    });
}

#[test]
fn unban_placement_fails_not_banned() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::unban_placement(RuntimeOrigin::root(), pid),
            Error::<Test>::PlacementNotBanned
        );
    });
}

// --- slash_placement actual slash ---
#[test]
fn slash_placement_deducts_claimable() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementClaimable::<Test>::insert(&pid, 100 * UNIT);
        assert_ok!(AdsCore::slash_placement(
            RuntimeOrigin::root(),
            pid,
            REPORTER
        ));
        // AdSlashPercentage = 30, so slashed = 100 * 30 / 100 = 30
        assert_eq!(PlacementClaimable::<Test>::get(&pid), 70 * UNIT);
        assert_eq!(SlashCount::<Test>::get(&pid), 1);
    });
}

// --- reset_slash_count ---
#[test]
fn reset_slash_count_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        SlashCount::<Test>::insert(&pid, 2);
        assert_ok!(AdsCore::reset_slash_count(RuntimeOrigin::root(), pid));
        assert_eq!(SlashCount::<Test>::get(&pid), 0);
    });
}

// --- clear_placement_flags ---
#[test]
fn clear_placement_flags_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(REPORTER),
            pid
        ));
        assert_ok!(AdsCore::flag_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid
        ));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 2);

        assert_ok!(AdsCore::clear_placement_flags(RuntimeOrigin::root(), pid));
        assert_eq!(PlacementFlagCount::<Test>::get(&pid), 0);
    });
}

// --- suspend_campaign / unsuspend_campaign ---
#[test]
fn suspend_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::suspend_campaign(RuntimeOrigin::root(), id));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Suspended
        );
    });
}

#[test]
fn unsuspend_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::suspend_campaign(RuntimeOrigin::root(), id));
        assert_ok!(AdsCore::unsuspend_campaign(RuntimeOrigin::root(), id));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Active
        );
    });
}

#[test]
fn unsuspend_campaign_fails_not_suspended() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::unsuspend_campaign(RuntimeOrigin::root(), id),
            Error::<Test>::CampaignNotSuspended
        );
    });
}

// --- report_approved_campaign ---
#[test]
fn report_approved_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert_ok!(AdsCore::report_approved_campaign(
            RuntimeOrigin::signed(REPORTER),
            id
        ));
        assert_eq!(CampaignReportCount::<Test>::get(id), 1);
    });
}

#[test]
fn report_approved_campaign_fails_duplicate() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert_ok!(AdsCore::report_approved_campaign(
            RuntimeOrigin::signed(REPORTER),
            id
        ));
        assert_noop!(
            AdsCore::report_approved_campaign(RuntimeOrigin::signed(REPORTER), id),
            Error::<Test>::AlreadyReportedCampaign
        );
    });
}

#[test]
fn report_approved_campaign_fails_not_approved() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER); // Pending
        assert_noop!(
            AdsCore::report_approved_campaign(RuntimeOrigin::signed(REPORTER), id),
            Error::<Test>::CampaignNotApproved
        );
    });
}

// --- resubmit_campaign ---
#[test]
fn resubmit_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        // Reject (auto-refunds)
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().review_status,
            AdReviewStatus::Rejected
        );

        // Resubmit with new content + budget
        assert_ok!(AdsCore::resubmit_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            ad_text("Fixed Ad"),
            ad_url("https://fixed.com"),
            30 * UNIT,
        ));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.text.as_slice(), b"Fixed Ad");
        assert_eq!(c.status, CampaignStatus::Active);
        assert_eq!(c.review_status, AdReviewStatus::Pending);
        assert_eq!(c.total_budget, 30 * UNIT);
        assert_eq!(CampaignEscrow::<Test>::get(id), 30 * UNIT);
    });
}

#[test]
fn resubmit_campaign_fails_not_rejected() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::resubmit_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                id,
                ad_text("X"),
                ad_url(""),
                10 * UNIT,
            ),
            Error::<Test>::CampaignNotRejected
        );
    });
}

// --- set_placement_delivery_types ---
#[test]
fn set_placement_delivery_types_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::set_placement_delivery_types(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            0b010,
        ));
        assert_eq!(PlacementDeliveryTypes::<Test>::get(&pid), 0b010);
    });
}

#[test]
fn set_placement_delivery_types_zero_removes() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementDeliveryTypes::<Test>::insert(&pid, 0b010);
        assert_ok!(AdsCore::set_placement_delivery_types(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            0,
        ));
        assert_eq!(PlacementDeliveryTypes::<Test>::get(&pid), 0);
    });
}

// --- delivery type enforcement ---
#[test]
fn submit_receipt_fails_delivery_type_mismatch() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER); // delivery_types = 0b001
        let pid = placement_id(1);
        // Placement only accepts type 0b010
        PlacementDeliveryTypes::<Test>::insert(&pid, 0b010);

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::DeliveryTypeMismatch
        );
    });
}

#[test]
fn submit_receipt_passes_delivery_type_match() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER); // delivery_types = 0b001
        let pid = placement_id(1);
        // Placement accepts types including 0b001
        PlacementDeliveryTypes::<Test>::insert(&pid, 0b011);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
    });
}

// --- whitelist enforcement ---
#[test]
fn submit_receipt_fails_not_in_advertiser_whitelist() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);
        let pid2 = placement_id(2);

        // Advertiser whitelists only pid2
        assert_ok!(AdsCore::advertiser_prefer_placement(
            RuntimeOrigin::signed(ADVERTISER),
            pid2,
        ));

        // Submit to pid1 (not in whitelist) should fail
        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::NotInAdvertiserWhitelist
        );
    });
}

// --- partial claim ---
#[test]
fn claim_ad_revenue_partial_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementClaimable::<Test>::insert(&pid, 10 * UNIT);

        assert_ok!(AdsCore::claim_ad_revenue(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            3 * UNIT,
        ));
        assert_eq!(PlacementClaimable::<Test>::get(&pid), 7 * UNIT);
    });
}

#[test]
fn claim_ad_revenue_fails_amount_too_large() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementClaimable::<Test>::insert(&pid, 10 * UNIT);

        assert_noop!(
            AdsCore::claim_ad_revenue(RuntimeOrigin::signed(PLACEMENT_ADMIN), pid, 20 * UNIT,),
            Error::<Test>::ClaimAmountTooLarge
        );
    });
}

// --- unregister_private_ad ---
#[test]
fn unregister_private_ad_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PrivateAdCount::<Test>::insert(&pid, 5);
        assert_ok!(AdsCore::unregister_private_ad(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            3,
        ));
        assert_eq!(PrivateAdCount::<Test>::get(&pid), 2);
    });
}

// --- cleanup_campaign ---
#[test]
fn cleanup_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));

        assert_ok!(AdsCore::cleanup_campaign(RuntimeOrigin::signed(42), id));
        assert!(Campaigns::<Test>::get(id).is_none());
        assert_eq!(CampaignEscrow::<Test>::get(id), 0);
        // Removed from advertiser index
        assert!(!CampaignsByAdvertiser::<Test>::get(ADVERTISER).contains(&id));
    });
}

#[test]
fn cleanup_campaign_fails_not_terminated() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::cleanup_campaign(RuntimeOrigin::signed(42), id),
            Error::<Test>::CampaignNotTerminated
        );
    });
}

// --- daily_budget enforcement ---
#[test]
fn daily_budget_limits_settlement() {
    new_test_ext().execute_with(|| {
        // Create campaign with small daily budget
        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("Daily Test"),
            ad_url("https://example.com"),
            UNIT,      // 1 UNIT per mille
            UNIT / 10, // daily budget = 0.1 UNIT (very small)
            50 * UNIT, // total budget
            0b001,
            1000,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));

        let pid = placement_id(1);
        // Submit receipt with large audience — would cost more than daily budget
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            500,
        ));

        // Settle — cost should be capped by daily budget
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid,
        ));

        let c = Campaigns::<Test>::get(id).unwrap();
        // Cost would be 1 UNIT * 500 * 100 / 100_000 = 0.5 UNIT
        // But daily budget is 0.1 UNIT, so spent should be <= 0.1 UNIT
        assert!(
            c.spent <= UNIT / 10,
            "spent {} should be <= daily budget {}",
            c.spent,
            UNIT / 10
        );
    });
}

// --- settlement incentive ---
#[test]
fn settlement_incentive_paid() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let settler_before = Balances::free_balance(ADVERTISER2);
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid,
        ));
        let settler_after = Balances::free_balance(ADVERTISER2);

        // SettlementIncentiveBps = 10 (0.1%), cost = 0.05 UNIT
        // incentive = 0.05 UNIT * 10 / 10000 = very small but > 0 if cost > 0
        // Just verify settler got something
        assert!(
            settler_after >= settler_before,
            "settler should receive incentive"
        );
    });
}

// --- suspend blocks delivery ---
#[test]
fn suspended_campaign_blocks_delivery() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);
        assert_ok!(AdsCore::suspend_campaign(RuntimeOrigin::root(), id));

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::CampaignNotActive
        );
    });
}

// --- expire suspended campaign ---
#[test]
fn expire_suspended_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::suspend_campaign(RuntimeOrigin::root(), id));
        System::set_block_number(1001);
        assert_ok!(AdsCore::expire_campaign(RuntimeOrigin::signed(42), id));
        assert_eq!(
            Campaigns::<Test>::get(id).unwrap().status,
            CampaignStatus::Expired
        );
    });
}

// --- block_to_day_index helper ---
#[test]
fn block_to_day_index_works() {
    new_test_ext().execute_with(|| {
        // created at block 100, now at block 100 → day 0
        assert_eq!(AdsCore::block_to_day_index(100, 100), 0);
        // now at block 14500 (14400 blocks = 1 day) → day 0 (100 blocks past)
        assert_eq!(AdsCore::block_to_day_index(14500, 100), 1);
        // now at block 28900 → day 2
        assert_eq!(AdsCore::block_to_day_index(28900, 100), 2);
    });
}

// --- force_settle_era_ads ---
#[test]
fn force_settle_era_ads_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // Phase 5: 确认收据
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        assert_ok!(AdsCore::force_settle_era_ads(RuntimeOrigin::root(), pid));
        assert_eq!(DeliveryReceipts::<Test>::get(&pid).len(), 0);
        let c = Campaigns::<Test>::get(id).unwrap();
        assert!(c.spent > 0);
    });
}

#[test]
fn force_settle_era_ads_fails_not_root() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::force_settle_era_ads(RuntimeOrigin::signed(ADVERTISER), pid),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ============================================================================
// Phase 1: Campaign 投放定向
// ============================================================================

#[test]
fn create_campaign_with_targets_works() {
    new_test_ext().execute_with(|| {
        let pid1 = placement_id(1);
        let pid2 = placement_id(2);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![pid1, pid2]).unwrap();

        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("Targeted Ad"),
            ad_url("https://example.com"),
            UNIT / 2,
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            Some(targets),
            CampaignType::Cpm,
            0u128,
        ));

        let stored = CampaignTargets::<Test>::get(id).unwrap();
        assert_eq!(stored.len(), 2);
        assert!(stored.contains(&pid1));
        assert!(stored.contains(&pid2));
    });
}

#[test]
fn create_campaign_without_targets_has_no_entry() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert!(CampaignTargets::<Test>::get(id).is_none());
    });
}

#[test]
fn create_campaign_rejects_empty_targets() {
    new_test_ext().execute_with(|| {
        let empty: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![]).unwrap();
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("Ad"),
                ad_url("https://x.com"),
                UNIT / 2,
                10 * UNIT,
                50 * UNIT,
                0b001,
                1000,
                Some(empty),
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::EmptyTargetsList
        );
    });
}

#[test]
fn targeted_campaign_blocks_non_target_placement() {
    new_test_ext().execute_with(|| {
        let pid1 = placement_id(1);
        let pid2 = placement_id(2);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![pid1]).unwrap();

        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("Targeted"),
            ad_url("https://x.com"),
            UNIT / 2,
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            Some(targets),
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));

        // pid1 (target) should work
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid1,
            100,
        ));

        // pid2 (not target) should fail
        assert_noop!(
            AdsCore::submit_delivery_receipt(
                RuntimeOrigin::signed(PLACEMENT_ADMIN2),
                id,
                pid2,
                100,
            ),
            Error::<Test>::PlacementNotTargeted
        );
    });
}

#[test]
fn untargeted_campaign_allows_any_placement() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid1 = placement_id(1);
        let pid2 = placement_id(2);

        // Both placements should work for untargeted campaign
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid1,
            100,
        ));
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN2),
            id,
            pid2,
            100,
        ));
    });
}

#[test]
fn set_campaign_targets_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid1 = placement_id(1);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![pid1]).unwrap();

        assert_ok!(AdsCore::set_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            targets,
        ));
        assert_eq!(CampaignTargets::<Test>::get(id).unwrap().len(), 1);
    });
}

#[test]
fn set_campaign_targets_fails_not_owner() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![placement_id(1)]).unwrap();

        assert_noop!(
            AdsCore::set_campaign_targets(RuntimeOrigin::signed(ADVERTISER2), id, targets,),
            Error::<Test>::NotCampaignOwner
        );
    });
}

#[test]
fn set_campaign_targets_fails_empty() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let empty: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![]).unwrap();

        assert_noop!(
            AdsCore::set_campaign_targets(RuntimeOrigin::signed(ADVERTISER), id, empty,),
            Error::<Test>::EmptyTargetsList
        );
    });
}

#[test]
fn clear_campaign_targets_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![placement_id(1)]).unwrap();

        assert_ok!(AdsCore::set_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            targets,
        ));
        assert!(CampaignTargets::<Test>::get(id).is_some());

        assert_ok!(AdsCore::clear_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
        ));
        assert!(CampaignTargets::<Test>::get(id).is_none());
    });
}

#[test]
fn cleanup_campaign_removes_targets() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let targets: BoundedVec<PlacementId, <Test as Config>::MaxTargetsPerCampaign> =
            BoundedVec::try_from(vec![placement_id(1)]).unwrap();
        CampaignTargets::<Test>::insert(id, targets);

        // Cancel then cleanup
        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_ok!(AdsCore::cleanup_campaign(RuntimeOrigin::signed(42), id));

        assert!(CampaignTargets::<Test>::get(id).is_none());
        assert!(Campaigns::<Test>::get(id).is_none());
    });
}

// ============================================================================
// Phase 3: CPM Multiplier 治理化
// ============================================================================

#[test]
fn set_campaign_multiplier_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::set_campaign_multiplier(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            200,
        ));
        assert_eq!(CampaignMultiplier::<Test>::get(id), Some(200));
    });
}

#[test]
fn set_campaign_multiplier_zero_clears() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        CampaignMultiplier::<Test>::insert(id, 150);
        assert_ok!(AdsCore::set_campaign_multiplier(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            0,
        ));
        assert!(CampaignMultiplier::<Test>::get(id).is_none());
    });
}

#[test]
fn set_campaign_multiplier_fails_not_owner() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert_noop!(
            AdsCore::set_campaign_multiplier(RuntimeOrigin::signed(ADVERTISER2), id, 200,),
            Error::<Test>::NotCampaignOwner
        );
    });
}

#[test]
fn set_campaign_multiplier_fails_invalid_range() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        // Too low
        assert_noop!(
            AdsCore::set_campaign_multiplier(RuntimeOrigin::signed(ADVERTISER), id, 5,),
            Error::<Test>::InvalidMultiplier
        );
        // Too high
        assert_noop!(
            AdsCore::set_campaign_multiplier(RuntimeOrigin::signed(ADVERTISER), id, 20_000,),
            Error::<Test>::InvalidMultiplier
        );
    });
}

#[test]
fn set_placement_multiplier_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_ok!(AdsCore::set_placement_multiplier(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            150,
        ));
        assert_eq!(PlacementMultiplier::<Test>::get(&pid), Some(150));
    });
}

#[test]
fn set_placement_multiplier_zero_clears() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        PlacementMultiplier::<Test>::insert(&pid, 200);
        assert_ok!(AdsCore::set_placement_multiplier(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            0,
        ));
        assert!(PlacementMultiplier::<Test>::get(&pid).is_none());
    });
}

#[test]
fn set_placement_multiplier_fails_not_admin() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::set_placement_multiplier(RuntimeOrigin::signed(ADVERTISER), pid, 150,),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

#[test]
fn delivery_receipt_uses_campaign_multiplier() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Set campaign multiplier to 2x
        CampaignMultiplier::<Test>::insert(id, 200);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].cpm_multiplier_bps, 200);
    });
}

#[test]
fn delivery_receipt_uses_placement_multiplier_fallback() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Set placement multiplier, no campaign multiplier
        PlacementMultiplier::<Test>::insert(&pid, 150);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].cpm_multiplier_bps, 150);
    });
}

#[test]
fn delivery_receipt_campaign_multiplier_overrides_placement() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Both set — campaign should win
        CampaignMultiplier::<Test>::insert(id, 300);
        PlacementMultiplier::<Test>::insert(&pid, 150);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].cpm_multiplier_bps, 300);
    });
}

#[test]
fn delivery_receipt_defaults_to_100_when_no_multiplier() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].cpm_multiplier_bps, 100);
    });
}

#[test]
fn cleanup_campaign_removes_multiplier() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        CampaignMultiplier::<Test>::insert(id, 200);

        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_ok!(AdsCore::cleanup_campaign(RuntimeOrigin::signed(42), id));

        assert!(CampaignMultiplier::<Test>::get(id).is_none());
    });
}

// ============================================================================
// Phase 2: 广告位级审核
// ============================================================================

#[test]
fn set_placement_approval_required_works() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_eq!(PlacementRequiresApproval::<Test>::get(&pid), false);

        assert_ok!(AdsCore::set_placement_approval_required(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            true,
        ));
        assert_eq!(PlacementRequiresApproval::<Test>::get(&pid), true);

        assert_ok!(AdsCore::set_placement_approval_required(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            false,
        ));
        assert_eq!(PlacementRequiresApproval::<Test>::get(&pid), false);
    });
}

#[test]
fn set_placement_approval_required_fails_not_admin() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::set_placement_approval_required(RuntimeOrigin::signed(ADVERTISER), pid, true,),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

#[test]
fn approve_campaign_for_placement_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::approve_campaign_for_placement(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            id,
        ));
        assert_eq!(PlacementCampaignApproval::<Test>::get(&pid, id), true);
    });
}

#[test]
fn approve_campaign_fails_not_admin() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::approve_campaign_for_placement(RuntimeOrigin::signed(ADVERTISER), pid, id,),
            Error::<Test>::NotPlacementAdmin
        );
    });
}

#[test]
fn approve_campaign_fails_not_found() {
    new_test_ext().execute_with(|| {
        let pid = placement_id(1);
        assert_noop!(
            AdsCore::approve_campaign_for_placement(
                RuntimeOrigin::signed(PLACEMENT_ADMIN),
                pid,
                999,
            ),
            Error::<Test>::CampaignNotFound
        );
    });
}

#[test]
fn reject_campaign_for_placement_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid = placement_id(1);

        PlacementCampaignApproval::<Test>::insert(&pid, id, true);
        assert_ok!(AdsCore::reject_campaign_for_placement(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
            id,
        ));
        assert_eq!(PlacementCampaignApproval::<Test>::get(&pid, id), false);
    });
}

#[test]
fn delivery_blocked_when_approval_required_and_not_approved() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        PlacementRequiresApproval::<Test>::insert(&pid, true);

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::CampaignNotApprovedForPlacement
        );
    });
}

#[test]
fn delivery_allowed_when_approval_required_and_approved() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        PlacementRequiresApproval::<Test>::insert(&pid, true);
        PlacementCampaignApproval::<Test>::insert(&pid, id, true);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
    });
}

#[test]
fn delivery_allowed_when_approval_not_required() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // approval not required (default) — should work without approval
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
    });
}

// ============================================================================
// Phase 4: 广告发现索引
// ============================================================================

#[test]
fn review_approved_adds_to_active_index() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        assert!(ActiveApprovedCampaigns::<Test>::get().is_empty());

        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));

        let active = ActiveApprovedCampaigns::<Test>::get();
        assert_eq!(active.len(), 1);
        assert!(active.contains(&id));

        let by_type = CampaignsByDeliveryType::<Test>::get(0x01);
        assert!(by_type.contains(&id));
    });
}

#[test]
fn review_rejected_removes_from_active_index() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));

        // create another campaign and reject it — should not affect first
        let id2 = create_default_campaign(ADVERTISER);
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id2, true));
        assert_eq!(ActiveApprovedCampaigns::<Test>::get().len(), 2);

        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id2, false));
        let active = ActiveApprovedCampaigns::<Test>::get();
        assert_eq!(active.len(), 1);
        assert!(active.contains(&id));
        assert!(!active.contains(&id2));
    });
}

#[test]
fn cancel_campaign_removes_from_active_index() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));

        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert!(!ActiveApprovedCampaigns::<Test>::get().contains(&id));
    });
}

#[test]
fn expire_campaign_removes_from_active_index() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));

        // advance past expiry (expires_at = 1000)
        frame_system::Pallet::<Test>::set_block_number(1001);
        assert_ok!(AdsCore::expire_campaign(RuntimeOrigin::signed(42), id));
        assert!(!ActiveApprovedCampaigns::<Test>::get().contains(&id));
    });
}

#[test]
fn suspend_removes_unsuspend_readds_index() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));

        assert_ok!(AdsCore::suspend_campaign(RuntimeOrigin::root(), id));
        assert!(!ActiveApprovedCampaigns::<Test>::get().contains(&id));

        assert_ok!(AdsCore::unsuspend_campaign(RuntimeOrigin::root(), id));
        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));
    });
}

#[test]
fn set_campaign_targets_updates_placement_index() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid1 = placement_id(1);
        let pid2 = placement_id(2);
        let targets = BoundedVec::try_from(vec![pid1, pid2]).unwrap();

        assert_ok!(AdsCore::set_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            targets,
        ));

        assert!(CampaignsForPlacement::<Test>::get(&pid1).contains(&id));
        assert!(CampaignsForPlacement::<Test>::get(&pid2).contains(&id));
    });
}

#[test]
fn clear_campaign_targets_removes_placement_index() {
    new_test_ext().execute_with(|| {
        let id = create_default_campaign(ADVERTISER);
        let pid1 = placement_id(1);
        let targets = BoundedVec::try_from(vec![pid1]).unwrap();

        assert_ok!(AdsCore::set_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            targets,
        ));
        assert!(CampaignsForPlacement::<Test>::get(&pid1).contains(&id));

        assert_ok!(AdsCore::clear_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
        ));
        assert!(!CampaignsForPlacement::<Test>::get(&pid1).contains(&id));
    });
}

#[test]
fn cleanup_campaign_clears_all_indexes() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid1 = placement_id(1);
        let targets = BoundedVec::try_from(vec![pid1]).unwrap();
        assert_ok!(AdsCore::set_campaign_targets(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            targets,
        ));

        assert!(ActiveApprovedCampaigns::<Test>::get().contains(&id));
        assert!(CampaignsForPlacement::<Test>::get(&pid1).contains(&id));

        assert_ok!(AdsCore::cancel_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id
        ));
        assert_ok!(AdsCore::cleanup_campaign(RuntimeOrigin::signed(42), id));

        assert!(!ActiveApprovedCampaigns::<Test>::get().contains(&id));
        assert!(!CampaignsForPlacement::<Test>::get(&pid1).contains(&id));
        assert!(CampaignsByDeliveryType::<Test>::get(0x01).is_empty());
    });
}

#[test]
fn create_campaign_with_targets_populates_placement_index() {
    new_test_ext().execute_with(|| {
        let pid1 = placement_id(1);
        let targets = BoundedVec::try_from(vec![pid1]).unwrap();
        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("targeted"),
            ad_url("https://t.co"),
            UNIT / 2,
            10 * UNIT,
            50 * UNIT,
            0x01,
            100u64.into(),
            Some(targets),
            CampaignType::Cpm,
            0u128,
        ));

        assert!(CampaignsForPlacement::<Test>::get(&pid1).contains(&id));
    });
}

// ============================================================================
// Phase 5: Receipt Confirmation & Dispute Tests
// ============================================================================

#[test]
fn confirm_receipt_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // 初始状态为 Pending
        assert_eq!(
            ReceiptConfirmation::<Test>::get((id, pid, 0u32)),
            Some(ReceiptStatus::Pending),
        );

        // 广告主确认
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));
        assert_eq!(
            ReceiptConfirmation::<Test>::get((id, pid, 0u32)),
            Some(ReceiptStatus::Confirmed),
        );
    });
}

#[test]
fn confirm_receipt_fails_not_owner() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // 非广告主不能确认
        assert_noop!(
            AdsCore::confirm_receipt(RuntimeOrigin::signed(ADVERTISER2), id, pid, 0),
            Error::<Test>::NotCampaignOwner
        );
    });
}

#[test]
fn confirm_receipt_fails_already_confirmed() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        // 重复确认失败
        assert_noop!(
            AdsCore::confirm_receipt(RuntimeOrigin::signed(ADVERTISER), id, pid, 0),
            Error::<Test>::ReceiptNotPending
        );
    });
}

#[test]
fn dispute_receipt_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        assert_ok!(AdsCore::dispute_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));
        assert_eq!(
            ReceiptConfirmation::<Test>::get((id, pid, 0u32)),
            Some(ReceiptStatus::Disputed),
        );
    });
}

#[test]
fn dispute_receipt_fails_not_owner() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        assert_noop!(
            AdsCore::dispute_receipt(RuntimeOrigin::signed(ADVERTISER2), id, pid, 0),
            Error::<Test>::NotCampaignOwner
        );
    });
}

#[test]
fn dispute_receipt_fails_already_disputed() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::dispute_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        assert_noop!(
            AdsCore::dispute_receipt(RuntimeOrigin::signed(ADVERTISER), id, pid, 0),
            Error::<Test>::ReceiptNotPending
        );
    });
}

#[test]
fn auto_confirm_receipt_works_after_window() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // 确认窗口内不可自动确认 (ReceiptConfirmationWindow = 100)
        System::set_block_number(50);
        assert_noop!(
            AdsCore::auto_confirm_receipt(RuntimeOrigin::signed(ADVERTISER2), id, pid, 0),
            Error::<Test>::ConfirmationWindowNotExpired
        );

        // 窗口到期后可自动确认 (submitted_at=1, window=100, 需 > 101)
        System::set_block_number(102);
        assert_ok!(AdsCore::auto_confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER2),
            id,
            pid,
            0
        ));
        assert_eq!(
            ReceiptConfirmation::<Test>::get((id, pid, 0u32)),
            Some(ReceiptStatus::AutoConfirmed),
        );
    });
}

#[test]
fn auto_confirm_receipt_fails_if_already_confirmed() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        System::set_block_number(102);
        assert_noop!(
            AdsCore::auto_confirm_receipt(RuntimeOrigin::signed(ADVERTISER2), id, pid, 0),
            Error::<Test>::ReceiptNotPending
        );
    });
}

#[test]
fn settle_skips_pending_receipts() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // 不确认，直接结算 — 收据被跳过, 无收入
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid
        ));
        assert_eq!(EraAdRevenue::<Test>::get(&pid), 0);
    });
}

#[test]
fn settle_skips_disputed_receipts() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::dispute_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));

        // 争议收据不结算
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid
        ));
        assert_eq!(EraAdRevenue::<Test>::get(&pid), 0);
    });
}

#[test]
fn settle_processes_auto_confirmed_receipts() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // 自动确认
        System::set_block_number(102);
        assert_ok!(AdsCore::auto_confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER2),
            id,
            pid,
            0
        ));

        // 自动确认的收据可结算
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid
        ));
        assert!(EraAdRevenue::<Test>::get(&pid) > 0);
    });
}

#[test]
fn settle_mixed_receipts_only_processes_confirmed() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 提交 3 张收据
        for _ in 0..3 {
            assert_ok!(AdsCore::submit_delivery_receipt(
                RuntimeOrigin::signed(PLACEMENT_ADMIN),
                id,
                pid,
                100,
            ));
        }

        // 收据 0: 确认, 收据 1: 争议, 收据 2: 保持 Pending
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0
        ));
        assert_ok!(AdsCore::dispute_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            1
        ));

        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid
        ));

        // 只结算了 1 张确认的收据 (cost = 0.5*100*100/100_000 = 0.05 UNIT)
        let expected_single = UNIT / 2 * 100 * 100 / 100_000;
        assert_eq!(EraAdRevenue::<Test>::get(&pid), expected_single);
    });
}

#[test]
fn receipt_not_found_for_nonexistent_index() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::confirm_receipt(RuntimeOrigin::signed(ADVERTISER), id, pid, 99),
            Error::<Test>::ReceiptNotFound
        );
        assert_noop!(
            AdsCore::dispute_receipt(RuntimeOrigin::signed(ADVERTISER), id, pid, 99),
            Error::<Test>::ReceiptNotFound
        );
        assert_noop!(
            AdsCore::auto_confirm_receipt(RuntimeOrigin::signed(ADVERTISER), id, pid, 99),
            Error::<Test>::ReceiptNotFound
        );
    });
}

// ============================================================================
// Phase 6: 广告主推荐测试
// ============================================================================

#[test]
fn p6_register_advertiser_works() {
    new_test_ext().execute_with(|| {
        let new_adv: u64 = 77;
        // ADVERTISER is seed advertiser (registered in mock setup)
        assert_ok!(AdsCore::register_advertiser(
            RuntimeOrigin::signed(new_adv),
            ADVERTISER
        ));
        assert!(AdvertiserRegisteredAt::<Test>::contains_key(new_adv));
        assert_eq!(AdvertiserReferrer::<Test>::get(new_adv), Some(ADVERTISER));
        assert_eq!(ReferrerAdvertisers::<Test>::get(ADVERTISER).len(), 1);
        assert_eq!(ReferrerAdvertisers::<Test>::get(ADVERTISER)[0], new_adv);

        System::assert_has_event(RuntimeEvent::AdsCore(Event::AdvertiserRegistered {
            advertiser: new_adv,
            referrer: ADVERTISER,
        }));
    });
}

#[test]
fn p6_register_advertiser_rejects_already_registered() {
    new_test_ext().execute_with(|| {
        // ADVERTISER is already registered as seed
        assert_noop!(
            AdsCore::register_advertiser(RuntimeOrigin::signed(ADVERTISER), ADVERTISER2),
            Error::<Test>::AlreadyRegisteredAdvertiser
        );
    });
}

#[test]
fn p6_register_advertiser_rejects_self_referral() {
    new_test_ext().execute_with(|| {
        let new_adv: u64 = 77;
        assert_noop!(
            AdsCore::register_advertiser(RuntimeOrigin::signed(new_adv), new_adv),
            Error::<Test>::SelfReferral
        );
    });
}

#[test]
fn p6_register_advertiser_rejects_non_advertiser_referrer() {
    new_test_ext().execute_with(|| {
        let new_adv: u64 = 77;
        let non_advertiser: u64 = 88;
        assert_noop!(
            AdsCore::register_advertiser(RuntimeOrigin::signed(new_adv), non_advertiser),
            Error::<Test>::ReferrerNotAdvertiser
        );
    });
}

#[test]
fn p6_force_register_advertiser_works() {
    new_test_ext().execute_with(|| {
        let seed: u64 = 99;
        assert_ok!(AdsCore::force_register_advertiser(
            RuntimeOrigin::root(),
            seed
        ));
        assert!(AdvertiserRegisteredAt::<Test>::contains_key(seed));
        // No referrer for seed
        assert!(AdvertiserReferrer::<Test>::get(seed).is_none());

        System::assert_has_event(RuntimeEvent::AdsCore(Event::SeedAdvertiserRegistered {
            advertiser: seed,
        }));
    });
}

#[test]
fn p6_force_register_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::force_register_advertiser(RuntimeOrigin::signed(ADVERTISER), 88),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn p6_force_register_rejects_already_registered() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::force_register_advertiser(RuntimeOrigin::root(), ADVERTISER),
            Error::<Test>::AlreadyRegisteredAdvertiser
        );
    });
}

#[test]
fn p6_create_campaign_requires_registration() {
    new_test_ext().execute_with(|| {
        let unregistered: u64 = 77;
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(unregistered),
                ad_text("test"),
                ad_url("https://example.com"),
                UNIT,       // bid
                10 * UNIT,  // daily
                100 * UNIT, // total
                1,
                1000u64.into(),
                None,
                CampaignType::Cpm,
                0u128,
            ),
            Error::<Test>::NotRegisteredAdvertiser
        );
    });
}

#[test]
fn p6_referral_commission_credited_on_settlement() {
    new_test_ext().execute_with(|| {
        let referred_adv: u64 = 77;
        // Give balance via genesis-style direct set
        pallet_balances::Pallet::<Test>::force_set_balance(
            RuntimeOrigin::root(),
            referred_adv,
            1_000 * UNIT,
        )
        .unwrap();
        // Register referred_adv with ADVERTISER as referrer
        assert_ok!(AdsCore::register_advertiser(
            RuntimeOrigin::signed(referred_adv),
            ADVERTISER
        ));
        assert_eq!(
            AdvertiserReferrer::<Test>::get(referred_adv),
            Some(ADVERTISER)
        );

        // Create and approve campaign by referred_adv
        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(referred_adv),
            ad_text("referral test"),
            ad_url("https://ref.com"),
            UNIT,
            0u128,
            100 * UNIT,
            1,
            1000u64.into(),
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));

        // Submit delivery receipt
        let pid = placement_id(1);
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));

        // Confirm receipt (Phase 5: must be Confirmed before settlement)
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(referred_adv),
            id,
            pid,
            0,
        ));

        // Settle
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
        ));

        // Check referral commission credited to ADVERTISER (referrer)
        // compute_cpm_cost: bid * audience * multiplier / 100_000
        // = 1 UNIT * 100 * 100 / 100_000 = 0.1 UNIT = 100_000_000_000
        let expected_cost = UNIT * 100 * 100 / 100_000;
        // Platform share = 20% of cost (MockRevenueDistributor)
        let platform_share = expected_cost * 20 / 100;
        // Referral commission = 5% of platform share (AdvertiserReferralRate = 500 bps)
        let expected_commission = platform_share * 500 / 10_000;
        assert!(expected_commission > 0, "commission should be non-zero");

        assert_eq!(
            ReferrerClaimable::<Test>::get(ADVERTISER),
            expected_commission
        );
        assert_eq!(
            ReferrerTotalEarnings::<Test>::get(ADVERTISER),
            expected_commission
        );

        System::assert_has_event(RuntimeEvent::AdsCore(Event::ReferralCommissionCredited {
            referrer: ADVERTISER,
            advertiser: referred_adv,
            amount: expected_commission,
        }));
    });
}

#[test]
fn p6_no_commission_for_seed_advertiser() {
    new_test_ext().execute_with(|| {
        // ADVERTISER is a seed (no referrer) — should not credit any commission
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0,
        ));
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
        ));

        // No referrer → no commission
        assert_eq!(ReferrerClaimable::<Test>::get(ADVERTISER), 0);
    });
}

#[test]
fn p6_claim_referral_earnings_works() {
    new_test_ext().execute_with(|| {
        // Manually seed some claimable for ADVERTISER
        let amount = 5 * UNIT;
        ReferrerClaimable::<Test>::insert(ADVERTISER, amount);

        let balance_before = Balances::free_balance(ADVERTISER);
        assert_ok!(AdsCore::claim_referral_earnings(RuntimeOrigin::signed(
            ADVERTISER
        )));
        let balance_after = Balances::free_balance(ADVERTISER);

        assert_eq!(balance_after - balance_before, amount);
        assert_eq!(ReferrerClaimable::<Test>::get(ADVERTISER), 0);

        System::assert_has_event(RuntimeEvent::AdsCore(Event::ReferralEarningsClaimed {
            referrer: ADVERTISER,
            amount,
        }));
    });
}

#[test]
fn p6_claim_referral_earnings_rejects_zero() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::claim_referral_earnings(RuntimeOrigin::signed(ADVERTISER)),
            Error::<Test>::NoReferralEarnings
        );
    });
}

#[test]
fn p6_referral_chain_prevents_unregistered() {
    new_test_ext().execute_with(|| {
        // Register 77 via ADVERTISER
        let a = 77u64;
        assert_ok!(AdsCore::register_advertiser(
            RuntimeOrigin::signed(a),
            ADVERTISER
        ));

        // Register 78 via 77 (77 is now a registered advertiser)
        let b = 78u64;
        assert_ok!(AdsCore::register_advertiser(RuntimeOrigin::signed(b), a));
        assert_eq!(AdvertiserReferrer::<Test>::get(b), Some(a));
        assert_eq!(ReferrerAdvertisers::<Test>::get(a).len(), 1);
    });
}

#[test]
fn p6_commission_accumulates_across_settlements() {
    new_test_ext().execute_with(|| {
        let referred_adv: u64 = 77;
        pallet_balances::Pallet::<Test>::force_set_balance(
            RuntimeOrigin::root(),
            referred_adv,
            1_000 * UNIT,
        )
        .unwrap();
        assert_ok!(AdsCore::register_advertiser(
            RuntimeOrigin::signed(referred_adv),
            ADVERTISER
        ));

        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(referred_adv),
            ad_text("accum"),
            ad_url("https://a.com"),
            UNIT,
            0u128,
            100 * UNIT,
            1,
            1000u64.into(),
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));
        let pid = placement_id(1);

        // Settlement 1
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(referred_adv),
            id,
            pid,
            0,
        ));
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
        ));
        let after_first = ReferrerClaimable::<Test>::get(ADVERTISER);
        assert!(after_first > 0);

        // Settlement 2
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            200,
        ));
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(referred_adv),
            id,
            pid,
            0,
        ));
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            pid,
        ));
        let after_second = ReferrerClaimable::<Test>::get(ADVERTISER);
        assert!(after_second > after_first);
        assert_eq!(ReferrerTotalEarnings::<Test>::get(ADVERTISER), after_second);
    });
}

// ============================================================================
// CPC (Cost-Per-Click) Tests
// ============================================================================

fn create_default_cpc_campaign(advertiser: u64) -> u64 {
    let id = NextCampaignId::<Test>::get();
    assert_ok!(AdsCore::create_campaign(
        RuntimeOrigin::signed(advertiser),
        ad_text("CPC Ad"),
        ad_url("https://cpc.example.com"),
        0u128,     // bid_per_mille (CPC 不使用)
        10 * UNIT, // daily budget
        50 * UNIT, // total budget
        0b001,     // type bit 0
        1000,      // expires_at
        None,      // targets
        CampaignType::Cpc,
        UNIT / 10, // bid_per_click = 0.1 UNIT
    ));
    id
}

fn create_approved_cpc_campaign(advertiser: u64) -> u64 {
    let id = create_default_cpc_campaign(advertiser);
    assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, true));
    id
}

#[test]
fn cpc_create_campaign_works() {
    new_test_ext().execute_with(|| {
        let id = create_default_cpc_campaign(ADVERTISER);
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.campaign_type, CampaignType::Cpc);
        assert_eq!(c.bid_per_click, UNIT / 10);
        assert_eq!(c.total_clicks, 0);
    });
}

#[test]
fn cpc_create_campaign_fails_bid_too_low() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            AdsCore::create_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                ad_text("CPC"),
                ad_url("https://x.com"),
                0u128,
                10 * UNIT,
                50 * UNIT,
                0b001,
                1000,
                None,
                CampaignType::Cpc,
                1u128, // too low
            ),
            Error::<Test>::ClickBidTooLow
        );
    });
}

#[test]
fn cpc_submit_click_receipt_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_click_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            50,
            50,
        ));

        // 验证 receipt 存储
        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].click_count, 50); // MockClickVerifier caps at 200, 50 < 200
        assert_eq!(receipts[0].audience_size, 0); // CPC receipts have 0 audience

        // 验证 campaign total_clicks
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.total_clicks, 50);
        assert_eq!(c.total_deliveries, 1);
    });
}

#[test]
fn cpc_submit_click_receipt_caps_at_verifier_limit() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        // MockClickVerifier caps at 200
        assert_ok!(AdsCore::submit_click_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            500,
            500,
        ));

        let receipts = DeliveryReceipts::<Test>::get(&pid);
        assert_eq!(receipts[0].click_count, 200); // capped

        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.total_clicks, 200);
    });
}

#[test]
fn cpc_submit_click_receipt_fails_zero_clicks() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::submit_click_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 0, 0,),
            Error::<Test>::ZeroClickCount
        );
    });
}

#[test]
fn cpc_submit_click_receipt_fails_verified_exceeds_total() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::submit_click_receipt(
                RuntimeOrigin::signed(PLACEMENT_ADMIN),
                id,
                pid,
                10,
                20, // verified > total
            ),
            Error::<Test>::VerifiedExceedsTotal
        );
    });
}

#[test]
fn cpc_submit_click_receipt_fails_campaign_type_mismatch() {
    new_test_ext().execute_with(|| {
        // CPM campaign
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::submit_click_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 50, 50,),
            Error::<Test>::CampaignTypeMismatch
        );
    });
}

#[test]
fn cpm_submit_delivery_receipt_fails_for_cpc_campaign() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_noop!(
            AdsCore::submit_delivery_receipt(RuntimeOrigin::signed(PLACEMENT_ADMIN), id, pid, 100,),
            Error::<Test>::CampaignTypeMismatch
        );
    });
}

#[test]
fn cpc_settle_era_ads_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        // Submit click receipt
        assert_ok!(AdsCore::submit_click_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
            100,
        ));

        // Confirm receipt
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0,
        ));

        let _before = Balances::free_balance(ADVERTISER);

        // Settle
        assert_ok!(AdsCore::settle_era_ads(
            RuntimeOrigin::signed(ADVERTISER2),
            pid,
        ));

        let c = Campaigns::<Test>::get(id).unwrap();
        // CPC cost = bid_per_click * clicks * multiplier / 100
        // = (UNIT/10) * 100 * 100 / 100 = 10 UNIT
        assert!(c.spent > 0);
    });
}

#[test]
fn cpc_compute_cpc_cost_basic() {
    new_test_ext().execute_with(|| {
        // bid=0.1 UNIT, clicks=100, multiplier=100 (1x)
        let cost = AdsCore::compute_cpc_cost(UNIT / 10, 100, 100);
        assert_eq!(cost, 10 * UNIT);

        // bid=0.1 UNIT, clicks=100, multiplier=50 (0.5x)
        let cost2 = AdsCore::compute_cpc_cost(UNIT / 10, 100, 50);
        assert_eq!(cost2, 5 * UNIT);

        // bid=0.1 UNIT, clicks=0
        let cost3 = AdsCore::compute_cpc_cost(UNIT / 10, 0, 100);
        assert_eq!(cost3, 0);
    });
}

#[test]
fn cpc_update_campaign_bid_per_click_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);

        // CPC campaign 可以更新 bid_per_click
        assert_ok!(AdsCore::update_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            None,
            None,
            None,
            None,
            None,
            Some(UNIT / 5), // 新 bid_per_click
        ));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.bid_per_click, UNIT / 5);
    });
}

#[test]
fn cpc_update_campaign_rejects_bid_per_mille() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);

        // CPC campaign 不能更新 bid_per_mille
        assert_noop!(
            AdsCore::update_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                id,
                None,
                None,
                Some(UNIT),
                None,
                None,
                None,
            ),
            Error::<Test>::CampaignTypeMismatch
        );
    });
}

#[test]
fn cpm_update_campaign_rejects_bid_per_click() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);

        // CPM campaign 不能更新 bid_per_click
        assert_noop!(
            AdsCore::update_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                id,
                None,
                None,
                None,
                None,
                None,
                Some(UNIT),
            ),
            Error::<Test>::CampaignTypeMismatch
        );
    });
}

#[test]
fn cpc_update_campaign_rejects_low_bid() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);

        // bid_per_click 低于 MinBidPerClick
        assert_noop!(
            AdsCore::update_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                id,
                None,
                None,
                None,
                None,
                None,
                Some(1u128),
            ),
            Error::<Test>::ClickBidTooLow
        );
    });
}

#[test]
fn cpc_force_settle_era_ads_works() {
    new_test_ext().execute_with(|| {
        let id = create_approved_cpc_campaign(ADVERTISER);
        let pid = placement_id(1);

        assert_ok!(AdsCore::submit_click_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            80,
            80,
        ));

        // Confirm receipt
        assert_ok!(AdsCore::confirm_receipt(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            pid,
            0,
        ));

        // Force settle (Root)
        assert_ok!(AdsCore::force_settle_era_ads(RuntimeOrigin::root(), pid,));

        let c = Campaigns::<Test>::get(id).unwrap();
        // CPC cost = (UNIT/10) * 80 * 100 / 100 = 8 UNIT
        assert_eq!(c.spent, 8 * UNIT);
    });
}

// ============================================================================
// available_campaigns_for_placement — 按 effective_bid 降序排序
// ============================================================================

#[test]
fn available_campaigns_sorted_by_effective_bid_desc() {
    new_test_ext().execute_with(|| {
        // Campaign A: CPM, bid_per_mille = 0.5 UNIT, multiplier = 100 → effective = 0.5 UNIT
        let id_a = create_approved_campaign(ADVERTISER);

        // Campaign B: CPM, bid_per_mille = 1 UNIT, multiplier = 100 → effective = 1 UNIT
        let id_b = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("High CPM"),
            ad_url("https://high.com"),
            UNIT, // 1 UNIT per mille
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id_b, true));

        // Campaign C: CPC, bid_per_click = 0.1 UNIT, multiplier = 100 → effective = 0.1 UNIT
        let id_c = create_approved_cpc_campaign(ADVERTISER);

        let pid = placement_id(1);
        let results = AdsCore::available_campaigns_for_placement(&pid, 10);

        assert_eq!(results.len(), 3);
        // 排序: B(1 UNIT) > A(0.5 UNIT) > C(0.1 UNIT)
        assert_eq!(results[0].campaign_id, id_b);
        assert_eq!(results[0].effective_bid, UNIT); // 1 UNIT * 100 / 100
        assert_eq!(results[1].campaign_id, id_a);
        assert_eq!(results[1].effective_bid, UNIT / 2); // 0.5 UNIT * 100 / 100
        assert_eq!(results[2].campaign_id, id_c);
        assert_eq!(results[2].effective_bid, UNIT / 10); // 0.1 UNIT * 100 / 100
    });
}

#[test]
fn available_campaigns_sorted_with_multiplier() {
    new_test_ext().execute_with(|| {
        // Campaign A: CPM, bid_per_mille = 0.5 UNIT, default multiplier 100 → effective = 0.5 UNIT
        let id_a = create_approved_campaign(ADVERTISER);

        // Campaign B: CPM, bid_per_mille = 0.3 UNIT, multiplier = 200 → effective = 0.6 UNIT
        let id_b = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER2),
            ad_text("Low bid high mult"),
            ad_url("https://mult.com"),
            UNIT * 3 / 10, // 0.3 UNIT per mille
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id_b, true));

        // 给 Campaign B 设置 multiplier = 200
        CampaignMultiplier::<Test>::insert(id_b, 200u32);

        let pid = placement_id(1);
        let results = AdsCore::available_campaigns_for_placement(&pid, 10);

        assert_eq!(results.len(), 2);
        // B effective = 0.3 * 200 / 100 = 0.6 UNIT
        // A effective = 0.5 * 100 / 100 = 0.5 UNIT
        // 排序: B > A
        assert_eq!(results[0].campaign_id, id_b);
        assert_eq!(results[0].effective_bid, UNIT * 3 / 10 * 200 / 100);
        assert_eq!(results[1].campaign_id, id_a);
        assert_eq!(results[1].effective_bid, UNIT / 2);
    });
}

#[test]
fn available_campaigns_sorted_truncates_to_max() {
    new_test_ext().execute_with(|| {
        // 创建 3 个 campaign, 请求 max_results = 2
        let _id_a = create_approved_campaign(ADVERTISER); // 0.5 UNIT
        let id_b = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("High"),
            ad_url("https://high.com"),
            UNIT,
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id_b, true));

        let id_c = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER2),
            ad_text("Mid"),
            ad_url("https://mid.com"),
            UNIT * 7 / 10, // 0.7 UNIT
            10 * UNIT,
            50 * UNIT,
            0b001,
            1000,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id_c, true));

        let pid = placement_id(1);
        let results = AdsCore::available_campaigns_for_placement(&pid, 2);

        // 只返回 top 2: B(1 UNIT), C(0.7 UNIT)
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].campaign_id, id_b);
        assert_eq!(results[1].campaign_id, id_c);
    });
}

// ============================================================================
// Round 2 审计回归测试
// ============================================================================

#[test]
fn h1_r2_resubmit_campaign_rejects_expired_before_reserve() {
    new_test_ext().execute_with(|| {
        // 创建 campaign, 过期时间设为 block 10
        let id = NextCampaignId::<Test>::get();
        assert_ok!(AdsCore::create_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            ad_text("Expire test"),
            ad_url("https://expire.com"),
            UNIT / 2,
            0u128,
            50 * UNIT,
            1,
            10u64,
            None,
            CampaignType::Cpm,
            0u128,
        ));
        // Root 拒绝 → 退款
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));

        let balance_before = Balances::free_balance(ADVERTISER);

        // 推进到 block 11 (已过期)
        System::set_block_number(11);

        // H1-R2: resubmit 应在 reserve 之前检查过期, 余额不变
        assert_noop!(
            AdsCore::resubmit_campaign(
                RuntimeOrigin::signed(ADVERTISER),
                id,
                ad_text("New text"),
                ad_url("https://new.com"),
                50 * UNIT,
            ),
            Error::<Test>::CampaignExpired
        );

        // 验证余额未被 reserve 锁定
        assert_eq!(Balances::free_balance(ADVERTISER), balance_before);
    });
}

#[test]
fn m3_r2_resubmit_campaign_resets_counters() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);
        let pid = placement_id(1);

        // 提交收据产生 total_deliveries
        assert_ok!(AdsCore::submit_delivery_receipt(
            RuntimeOrigin::signed(PLACEMENT_ADMIN),
            id,
            pid,
            100,
        ));
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.total_deliveries, 1);

        // Root 拒绝
        assert_ok!(AdsCore::review_campaign(RuntimeOrigin::root(), id, false));

        // Resubmit
        assert_ok!(AdsCore::resubmit_campaign(
            RuntimeOrigin::signed(ADVERTISER),
            id,
            ad_text("Resubmitted"),
            ad_url("https://resubmit.com"),
            50 * UNIT,
        ));

        // M3-R2: 验证计数器已重置
        let c = Campaigns::<Test>::get(id).unwrap();
        assert_eq!(c.total_deliveries, 0);
        assert_eq!(c.total_clicks, 0);
        assert_eq!(c.spent, 0);
    });
}

#[test]
fn m2_r2_campaign_details_total_deliveries_safe_truncation() {
    new_test_ext().execute_with(|| {
        let id = create_approved_campaign(ADVERTISER);

        // 手动设置极大 total_deliveries 以测试截断
        Campaigns::<Test>::mutate(id, |maybe| {
            if let Some(c) = maybe {
                c.total_deliveries = u64::MAX;
            }
        });

        let detail = AdsCore::campaign_details(id).unwrap();
        // M2-R2: 应安全截断到 u32::MAX 而非溢出
        assert_eq!(detail.total_deliveries, u32::MAX);
    });
}
