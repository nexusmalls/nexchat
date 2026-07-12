// Copyright 2023-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.

use crate::{mock::*, utils::*, *};
use core::ops::RangeInclusive;
use frame_support::{assert_noop, assert_ok};
use orml_traits::MultiCurrency;
use test_case::test_case;
use zeitgeist_primitives::types::{Asset, MarketStatus, MarketType, ScoringRule};
use zrml_market_commons::{Error as MError, Markets};

#[test]
fn buy_emits_event() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Active;
        Markets::<Runtime>::insert(market_id, market);
        let asset = Asset::ParimutuelShare(market_id, 0u16);
        let amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(RuntimeOrigin::signed(ALICE), asset, amount));
        let amount_minus_fees = 99_000_000_000;
        let fees = 1_000_000_000;
        assert_eq!(amount, amount_minus_fees + fees);
        System::assert_last_event(
            Event::OutcomeBought {
                market_id,
                buyer: ALICE,
                asset,
                amount_minus_fees,
                fees,
            }
            .into(),
        );
    });
}

#[test]
fn buy_balances_change_correctly() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = MarketStatus::Active;
        Markets::<Runtime>::insert(market_id, market.clone());
        let base_asset = market.base_asset;
        let alice_before = AssetManager::free_balance(base_asset, &ALICE);
        let creator_before = AssetManager::free_balance(base_asset, &market.creator);
        let pot_before =
            AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id));
        let asset = Asset::ParimutuelShare(market_id, 0u16);
        let amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(RuntimeOrigin::signed(ALICE), asset, amount));
        let amount_minus_fees = 99_000_000_000;
        let fees = 1_000_000_000;
        assert_eq!(amount, amount_minus_fees + fees);
        assert_eq!(
            AssetManager::free_balance(base_asset, &ALICE),
            alice_before - amount
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id))
                - pot_before,
            amount_minus_fees
        );
        assert_eq!(AssetManager::free_balance(asset, &ALICE), amount_minus_fees);
        assert_eq!(
            AssetManager::free_balance(base_asset, &market.creator) - creator_before,
            fees
        );
    });
}

#[test]
fn buy_fails_if_asset_not_parimutuel_share() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let market = market_mock::<Runtime>(MARKET_CREATOR);
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::CategoricalOutcome(market_id, 0u16),
                <Runtime as Config>::MinBetSize::get()
            ),
            Error::<Runtime>::NotParimutuelOutcome
        );
    });
}

#[test_case(ScoringRule::AmmCdaHybrid; "amm_cda_hybrid")]
fn buy_fails_if_invalid_scoring_rule(scoring_rule: ScoringRule) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.scoring_rule = scoring_rule;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16),
                <Runtime as Config>::MinBetSize::get()
            ),
            Error::<Runtime>::InvalidScoringRule
        );
    });
}

#[test_case(MarketStatus::Proposed; "proposed")]
#[test_case(MarketStatus::Closed; "closed")]
#[test_case(MarketStatus::Reported; "reported")]
#[test_case(MarketStatus::Disputed; "disputed")]
fn buy_fails_if_market_status_is_not_active(status: MarketStatus) {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        market.status = status;
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16),
                <Runtime as Config>::MinBetSize::get()
            ),
            Error::<Runtime>::MarketIsNotActive
        );
    });
}

#[test]
fn buy_fails_if_market_type_is_scalar() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let mut market = market_mock::<Runtime>(MARKET_CREATOR);
        let range: RangeInclusive<u128> = 0..=100;
        market.market_type = MarketType::Scalar(range);
        Markets::<Runtime>::insert(market_id, market);
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16),
                2 * <Runtime as Config>::MinBetSize::get()
            ),
            Error::<Runtime>::NotCategorical
        );
    });
}

#[test]
fn buy_fails_if_insufficient_balance() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let market = market_mock::<Runtime>(MARKET_CREATOR);
        let base_asset = market.base_asset;
        Markets::<Runtime>::insert(market_id, market);
        let free = AssetManager::free_balance(base_asset, &ALICE);
        AssetManager::slash(base_asset, &ALICE, free);
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16),
                <Runtime as Config>::MinBetSize::get()
            ),
            Error::<Runtime>::InsufficientBalance
        );
    });
}

#[test]
fn buy_fails_if_below_minimum_bet_size() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        Markets::<Runtime>::insert(market_id, market_mock::<Runtime>(MARKET_CREATOR));
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(market_id, 0u16),
                <Runtime as Config>::MinBetSize::get() - 1
            ),
            Error::<Runtime>::AmountBelowMinimumBetSize
        );
    });
}

#[test]
fn buy_fails_if_market_does_not_exist() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            Parimutuel::buy(
                RuntimeOrigin::signed(ALICE),
                Asset::ParimutuelShare(0, 0u16),
                <Runtime as Config>::MinBetSize::get()
            ),
            MError::<Runtime>::MarketDoesNotExist
        );
    });
}
