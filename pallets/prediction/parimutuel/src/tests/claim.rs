// Copyright 2023-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.

use crate::{mock::*, utils::*, *};
use core::ops::RangeInclusive;
use frame_support::{assert_noop, assert_ok};
use orml_traits::MultiCurrency;
use sp_runtime::Percent;
use test_case::test_case;
use zeitgeist_primitives::types::{Asset, MarketStatus, MarketType, OutcomeReport, ScoringRule};
use zrml_market_commons::{Error as MError, Markets};

#[test]
fn claim_rewards_emits_event() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let winner_asset = Asset::ParimutuelShare(market_id, 0u16);
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            winner_asset,
            20 * <Runtime as Config>::MinBetSize::get()
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            Asset::ParimutuelShare(market_id, 1u16),
            10 * <Runtime as Config>::MinBetSize::get()
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        });
        assert_ok!(Parimutuel::claim_rewards(
            RuntimeOrigin::signed(ALICE),
            market_id
        ));
        System::assert_last_event(
            Event::RewardsClaimed {
                market_id,
                asset: winner_asset,
                withdrawn_asset_balance: 198_000_000_000,
                base_asset_payoff: 297_000_000_000,
                sender: ALICE,
            }
            .into(),
        );
    });
}

#[test]
fn claim_rewards_categorical_changes_balances_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let winner_asset = Asset::ParimutuelShare(market_id, 0u16);
        let winner_amount_0 = 20 * <Runtime as Config>::MinBetSize::get();
        let winner_amount_1 = 30 * <Runtime as Config>::MinBetSize::get();
        let loser_amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            winner_asset,
            winner_amount_0
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(CHARLIE),
            winner_asset,
            winner_amount_1
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            Asset::ParimutuelShare(market_id, 1u16),
            loser_amount
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        });
        let market = Markets::<Runtime>::get(market_id).unwrap();
        let actual_payoff = 594_000_000_000;
        let total = winner_amount_0 + winner_amount_1 + loser_amount;
        assert_eq!(actual_payoff, total - Percent::from_percent(1) * total);
        let alice_payoff = 237_600_000_000;
        let charlie_payoff = 356_400_000_000;
        assert_eq!(Percent::from_percent(40) * actual_payoff, alice_payoff);
        assert_eq!(Percent::from_percent(60) * actual_payoff, charlie_payoff);
        assert_eq!(alice_payoff + charlie_payoff, actual_payoff);
        let alice_shares_before = AssetManager::free_balance(winner_asset, &ALICE);
        assert_eq!(
            alice_shares_before,
            winner_amount_0 - Percent::from_percent(1) * winner_amount_0
        );
        let alice_base_before = AssetManager::free_balance(market.base_asset, &ALICE);
        let pot_before =
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id));
        assert_eq!(pot_before, actual_payoff);
        assert_ok!(Parimutuel::claim_rewards(
            RuntimeOrigin::signed(ALICE),
            market_id
        ));
        assert_eq!(
            alice_shares_before - AssetManager::free_balance(winner_asset, &ALICE),
            alice_shares_before
        );
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &ALICE) - alice_base_before,
            alice_payoff
        );
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id)),
            charlie_payoff
        );
        let charlie_shares_before = AssetManager::free_balance(winner_asset, &CHARLIE);
        assert_eq!(
            charlie_shares_before,
            winner_amount_1 - Percent::from_percent(1) * winner_amount_1
        );
        let charlie_base_before = AssetManager::free_balance(market.base_asset, &CHARLIE);
        assert_ok!(Parimutuel::claim_rewards(
            RuntimeOrigin::signed(CHARLIE),
            market_id
        ));
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &CHARLIE) - charlie_base_before,
            charlie_payoff
        );
        assert_eq!(
            charlie_shares_before - AssetManager::free_balance(winner_asset, &CHARLIE),
            charlie_shares_before
        );
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id)),
            0
        );
        assert_eq!(AssetManager::free_balance(winner_asset, &ALICE), 0);
        assert_eq!(AssetManager::free_balance(winner_asset, &CHARLIE), 0);
    });
}

#[test]
fn claim_rewards_fails_if_market_type_is_scalar() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        let range: RangeInclusive<u128> = 0..=100;
        market.market_type = MarketType::Scalar(range);
        market.resolved_outcome = Some(OutcomeReport::Scalar(50));
        market.status = MarketStatus::Resolved;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::NotCategorical
        );
    });
}

#[test_case(MarketStatus::Active; "active")]
#[test_case(MarketStatus::Proposed; "proposed")]
#[test_case(MarketStatus::Closed; "closed")]
#[test_case(MarketStatus::Reported; "reported")]
#[test_case(MarketStatus::Disputed; "disputed")]
fn claim_rewards_fails_if_not_resolved(status: MarketStatus) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = status;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::MarketIsNotResolvedYet
        );
    });
}

#[test_case(ScoringRule::AmmCdaHybrid)]
fn claim_rewards_fails_if_scoring_rule_not_parimutuel(scoring_rule: ScoringRule) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        market.scoring_rule = scoring_rule;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::InvalidScoringRule
        );
    });
}

#[test]
fn claim_rewards_fails_if_no_resolved_outcome() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Resolved;
        market.resolved_outcome = None;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::NoResolvedOutcome
        );
    });
}

#[test]
fn claim_rewards_fails_if_market_does_not_exist() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), 0),
            MError::<Runtime>::MarketDoesNotExist
        );
    });
}

#[test]
fn claim_rewards_categorical_fails_if_no_winner() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            Asset::ParimutuelShare(market_id, 0u16),
            20 * <Runtime as Config>::MinBetSize::get()
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            Asset::ParimutuelShare(market_id, 1u16),
            10 * <Runtime as Config>::MinBetSize::get()
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(6u16));
        });
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::NoRewardShareOutstanding
        );
    });
}

#[test]
fn claim_rewards_categorical_fails_if_no_winning_shares() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            Asset::ParimutuelShare(market_id, 0u16),
            20 * <Runtime as Config>::MinBetSize::get()
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            Asset::ParimutuelShare(market_id, 1u16),
            10 * <Runtime as Config>::MinBetSize::get()
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(0u16));
        });
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(BOB), market_id),
            Error::<Runtime>::NoWinningShares
        );
    });
}

#[test]
fn claim_refunds_works() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        let alice_asset = Asset::ParimutuelShare(market_id, 0u16);
        let bob_asset = Asset::ParimutuelShare(market_id, 1u16);
        let alice_amount = 20 * <Runtime as Config>::MinBetSize::get();
        let bob_amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            alice_asset,
            alice_amount
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            bob_asset,
            bob_amount
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(2u16));
        });
        assert_noop!(
            Parimutuel::claim_rewards(RuntimeOrigin::signed(ALICE), market_id),
            Error::<Runtime>::NoRewardShareOutstanding
        );
        let market = Markets::<Runtime>::get(market_id).unwrap();
        let alice_refund = alice_amount - Percent::from_percent(1) * alice_amount;
        let bob_refund = bob_amount - Percent::from_percent(1) * bob_amount;
        let alice_before = AssetManager::free_balance(market.base_asset, &ALICE);
        let bob_before = AssetManager::free_balance(market.base_asset, &BOB);
        let pot_before =
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id));
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(ALICE),
            alice_asset
        ));
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &ALICE) - alice_before,
            alice_refund
        );
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id)),
            pot_before - alice_refund
        );
        assert_eq!(pot_before - alice_refund, bob_refund);
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(BOB),
            bob_asset
        ));
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &BOB) - bob_before,
            bob_refund
        );
        assert_eq!(
            AssetManager::free_balance(market.base_asset, &Parimutuel::pot_account(market_id)),
            0
        );
    });
}
