//! Live foreign-collateral conservation tests for Parimutuel.
//! Parimutuel 的实时外部抵押守恒测试。

use crate::{mock::*, utils::market_mock, Config};
use frame_support::assert_ok;
use orml_traits::MultiCurrency;
use prediction_mock_runtime::USDX_ASSET_ID;
use zeitgeist_primitives::types::{Asset, MarketStatus, OutcomeReport};
use zrml_market_commons::Markets;

fn insert_foreign_market(market_id: u128) {
    let mut market = market_mock::<Runtime>(MARKET_CREATOR);
    market.base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
    Markets::<Runtime>::insert(market_id, market);
}

fn assert_foreign_collateral_conserved(market_id: u128, expected_issuance: u128) {
    let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
    let tracked_mirror = [ALICE, BOB, CHARLIE, MARKET_CREATOR]
        .into_iter()
        .map(|account| AssetManager::free_balance(base_asset, &account))
        .sum::<u128>()
        + AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id));
    assert_eq!(tracked_mirror, expected_issuance);
    assert_eq!(
        PredictionCollateral::mirror_issuance(USDX_ASSET_ID),
        expected_issuance
    );
    assert_eq!(
        PredictionCollateral::escrow_balance(USDX_ASSET_ID),
        expected_issuance
    );
    assert!(PredictionCollateral::is_mirror_consistent(USDX_ASSET_ID));
}

#[test]
fn foreign_collateral_winner_payout_and_fees_are_conservative() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        insert_foreign_market(market_id);
        assert!(PredictionCollateral::is_deposit_allowed(USDX_ASSET_ID));
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let winner_asset = Asset::ParimutuelShare(market_id, 0);
        let loser_asset = Asset::ParimutuelShare(market_id, 1);
        let winner_amount = 20 * <Runtime as Config>::MinBetSize::get();
        let loser_amount = 10 * <Runtime as Config>::MinBetSize::get();

        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            winner_asset,
            winner_amount,
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            loser_asset,
            loser_amount,
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(0));
        });
        assert_ok!(Parimutuel::claim_rewards(
            RuntimeOrigin::signed(ALICE),
            market_id,
        ));

        let total_fees =
            calculate_fee::<Runtime>(winner_amount) + calculate_fee::<Runtime>(loser_amount);
        let payoff = winner_amount + loser_amount - total_fees;
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        assert_eq!(
            AssetManager::free_balance(base_asset, &ALICE),
            INITIAL_FOREIGN_BALANCE - winner_amount + payoff
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &BOB),
            INITIAL_FOREIGN_BALANCE - loser_amount
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &MARKET_CREATOR),
            INITIAL_FOREIGN_BALANCE + total_fees
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id)),
            0
        );
        assert_foreign_collateral_conserved(market_id, issuance_before);
    });
}

#[test]
fn foreign_collateral_no_winner_refunds_and_fees_are_conservative() {
    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        insert_foreign_market(market_id);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let alice_asset = Asset::ParimutuelShare(market_id, 0);
        let bob_asset = Asset::ParimutuelShare(market_id, 1);
        let alice_amount = 20 * <Runtime as Config>::MinBetSize::get();
        let bob_amount = 10 * <Runtime as Config>::MinBetSize::get();

        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            alice_asset,
            alice_amount,
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            bob_asset,
            bob_amount,
        ));
        Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(2));
        });
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(ALICE),
            alice_asset,
        ));
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(BOB),
            bob_asset,
        ));

        let alice_fee = calculate_fee::<Runtime>(alice_amount);
        let bob_fee = calculate_fee::<Runtime>(bob_amount);
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        assert_eq!(
            AssetManager::free_balance(base_asset, &ALICE),
            INITIAL_FOREIGN_BALANCE - alice_fee
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &BOB),
            INITIAL_FOREIGN_BALANCE - bob_fee
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &MARKET_CREATOR),
            INITIAL_FOREIGN_BALANCE + alice_fee + bob_fee
        );
        assert_eq!(
            AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id)),
            0
        );
        assert_foreign_collateral_conserved(market_id, issuance_before);
    });
}
