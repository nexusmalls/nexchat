// Copyright 2023-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.

use crate::{mock::*, utils::*, *};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::Percent;
use test_case::test_case;
use zeitgeist_primitives::types::{Asset, MarketStatus, OutcomeReport, ScoringRule};
use zrml_market_commons::Markets;

#[test]
fn refund_fails_if_not_parimutuel_outcome() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_refunds(
                RuntimeOrigin::signed(ALICE),
                Asset::CategoricalOutcome(market_id, 0u16)
            ),
            Error::<Runtime>::NotParimutuelOutcome
        );
    });
}

#[test_case(MarketStatus::Active; "active")]
#[test_case(MarketStatus::Proposed; "proposed")]
#[test_case(MarketStatus::Closed; "closed")]
#[test_case(MarketStatus::Reported; "reported")]
#[test_case(MarketStatus::Disputed; "disputed")]
fn refund_fails_if_market_not_resolved(status: MarketStatus) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = status;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_refunds(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16)
            ),
            Error::<Runtime>::MarketIsNotResolvedYet
        );
    });
}

#[test_case(ScoringRule::AmmCdaHybrid)]
fn refund_fails_if_invalid_scoring_rule(scoring_rule: ScoringRule) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        market.scoring_rule = scoring_rule;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_refunds(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16)
            ),
            Error::<Runtime>::InvalidScoringRule
        );
    });
}

#[test]
fn refund_fails_if_invalid_outcome_asset() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_refunds(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 20u16)
            ),
            Error::<Runtime>::InvalidOutcomeAsset
        );
    });
}

#[test]
fn refund_fails_if_no_resolved_outcome() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = None;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_refunds(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16)
            ),
            Error::<Runtime>::NoResolvedOutcome
        );
    });
}

#[test]
fn refund_fails_if_refund_not_allowed() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let asset = Asset::ParimutuelShare(market_id, 0u16);
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            asset,
            10 * <Runtime as Config>::MinBetSize::get()
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        });
        assert_noop!(
            Parimutuel::claim_refunds(RuntimeOrigin::signed(ALICE), asset),
            Error::<Runtime>::RefundNotAllowed
        );
    });
}

#[test]
fn refund_fails_if_refundable_balance_is_zero() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let asset = Asset::ParimutuelShare(market_id, 0u16);
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            asset,
            2 * <Runtime as Config>::MinBetSize::get()
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(1u16));
        });
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(ALICE),
            asset
        ));
        assert_noop!(
            Parimutuel::claim_refunds(RuntimeOrigin::signed(ALICE), asset),
            Error::<Runtime>::RefundableBalanceIsZero
        );
    });
}

#[test]
fn refund_emits_event() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let asset = Asset::ParimutuelShare(market_id, 0u16);
        let amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(RuntimeOrigin::signed(ALICE), asset, amount));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(1u16));
        });
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(ALICE),
            asset
        ));
        System::assert_last_event(
            Event::BalanceRefunded {
                market_id,
                asset,
                refunded_balance: amount - Percent::from_percent(1) * amount,
                sender: ALICE,
            }
            .into(),
        );
    });
}
