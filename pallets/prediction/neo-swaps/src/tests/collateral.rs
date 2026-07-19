//! Foreign-collateral conservation tests for Phase 4 trading.
//! Phase 4 交易模块的外部抵押守恒测试。

use super::*;
use prediction_mock_runtime::USDX_ASSET_ID;
use zeitgeist_primitives::types::Balance;

fn assert_usdx_mirror_unchanged(expected_issuance: Balance) {
    assert!(PredictionCollateral::is_mirror_consistent(USDX_ASSET_ID));
    assert_eq!(
        PredictionCollateral::mirror_issuance(USDX_ASSET_ID),
        expected_issuance
    );
    assert_eq!(
        PredictionCollateral::escrow_balance(USDX_ASSET_ID),
        expected_issuance
    );
}

#[test]
fn buy_preserves_foreign_collateral_mirror() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market_and_deploy_pool(
            ALICE,
            base_asset,
            MarketType::Categorical(2),
            _10,
            vec![_1_2, _1_2],
            CENT,
        );
        let asset_out = Pools::<Runtime>::get(market_id).unwrap().assets()[0];

        assert_ok!(NeoSwaps::buy(
            RuntimeOrigin::signed(BOB),
            market_id,
            2,
            asset_out,
            _1,
            0,
        ));

        assert_usdx_mirror_unchanged(issuance_before);
    });
}

#[test]
fn sell_preserves_foreign_collateral_mirror() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market_and_deploy_pool(
            ALICE,
            base_asset,
            MarketType::Scalar(0..=1),
            _10,
            vec![_1_4, _3_4],
            CENT,
        );
        let asset_in = Pools::<Runtime>::get(market_id).unwrap().assets()[1];
        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            _10,
        ));

        assert_ok!(NeoSwaps::sell(
            RuntimeOrigin::signed(BOB),
            market_id,
            2,
            asset_in,
            _10,
            0,
        ));

        assert_usdx_mirror_unchanged(issuance_before);
    });
}

#[test]
fn join_exit_roundtrip_preserves_foreign_collateral_mirror() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market_and_deploy_pool(
            ALICE,
            base_asset,
            MarketType::Scalar(0..=1),
            _10,
            vec![_1_6, _5_6 + 1],
            CENT,
        );
        let pool_shares = _4;
        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(BOB),
            market_id,
            pool_shares + CENT,
        ));
        assert_ok!(NeoSwaps::join(
            RuntimeOrigin::signed(BOB),
            market_id,
            pool_shares,
            vec![u128::MAX; 2],
        ));
        assert_ok!(NeoSwaps::exit(
            RuntimeOrigin::signed(BOB),
            market_id,
            pool_shares,
            vec![0; 2],
        ));

        assert_usdx_mirror_unchanged(issuance_before);
    });
}

#[test]
fn fee_accrual_and_withdrawal_preserve_foreign_collateral_mirror() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market_and_deploy_pool(
            ALICE,
            base_asset,
            MarketType::Categorical(2),
            _10,
            vec![_1_2, _1_2],
            CENT,
        );
        let asset_out = Pools::<Runtime>::get(market_id).unwrap().assets()[0];
        assert_ok!(NeoSwaps::buy(
            RuntimeOrigin::signed(CHARLIE),
            market_id,
            2,
            asset_out,
            _10,
            0,
        ));
        let alice_before = AssetManager::free_balance(base_asset, &ALICE);

        assert_ok!(NeoSwaps::withdraw_fees(
            RuntimeOrigin::signed(ALICE),
            market_id,
        ));

        assert!(AssetManager::free_balance(base_asset, &ALICE) > alice_before);
        assert_usdx_mirror_unchanged(issuance_before);
    });
}
