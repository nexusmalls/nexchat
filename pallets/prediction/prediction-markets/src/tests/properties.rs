//! Property tests for Phase 3 market invariants.
//! Phase 3 市场不变量的 property 测试。

use super::*;
use zeitgeist_primitives::types::{Asset, MarketCreation, ScoringRule};

#[test]
fn complete_set_buy_sell_roundtrip_conserves_native_collateral() {
    ExtBuilder::default().build().execute_with(|| {
        simple_create_categorical_market(
            Asset::Ztg,
            MarketCreation::Permissionless,
            0..2,
            ScoringRule::AmmCdaHybrid,
        );

        let market_id = 0;
        let amount = CENT;
        let bob_before = AssetManager::free_balance(Asset::Ztg, &BOB);
        let market_account = PredictionMarkets::market_account(market_id);

        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            amount,
        ));

        let market = MarketCommons::market(&market_id).unwrap();
        for asset in market.outcome_assets().iter() {
            assert_eq!(Tokens::free_balance(*asset, &BOB), amount);
        }
        assert_eq!(
            AssetManager::free_balance(Asset::Ztg, &market_account),
            amount
        );

        assert_ok!(PredictionMarkets::sell_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            amount,
        ));

        assert_eq!(AssetManager::free_balance(Asset::Ztg, &BOB), bob_before);
        assert_eq!(AssetManager::free_balance(Asset::Ztg, &market_account), 0);
        for asset in market.outcome_assets().iter() {
            assert_eq!(Tokens::free_balance(*asset, &BOB), 0);
        }
    });
}

#[test]
fn complete_set_buy_sell_roundtrip_conserves_foreign_collateral() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        simple_create_categorical_market(
            base_asset,
            MarketCreation::Permissionless,
            0..2,
            ScoringRule::AmmCdaHybrid,
        );

        let market_id = 0;
        let amount = CENT;
        let bob_before = AssetManager::free_balance(base_asset, &BOB);
        let market_account = PredictionMarkets::market_account(market_id);

        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            amount,
        ));
        assert_eq!(
            AssetManager::free_balance(base_asset, &market_account),
            amount
        );

        assert_ok!(PredictionMarkets::sell_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            amount,
        ));

        assert_eq!(AssetManager::free_balance(base_asset, &BOB), bob_before);
        assert_eq!(AssetManager::free_balance(base_asset, &market_account), 0);
    });
}
