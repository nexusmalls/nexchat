//! Live foreign-collateral routing and rollback invariants.
//! 实时外部抵押路由与回滚不变量。

use super::*;
use orml_traits::MultiReservableCurrency;
use prediction_mock_runtime::USDX_ASSET_ID;
use zeitgeist_primitives::types::{Asset, Balance};

fn assert_usdx_mirror_unchanged(expected_issuance: Balance) {
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
fn foreign_collateral_buy_falls_back_to_orderbook_on_amm_soft_failure() {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::ForeignAsset(USDX_ASSET_ID);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let min_spot_price = CENT / 2;
        let market_id = create_market_and_deploy_pool(
            ALICE,
            base_asset,
            MarketType::Categorical(2),
            _10,
            vec![min_spot_price, _1 - min_spot_price],
            CENT,
        );
        let asset = Asset::CategoricalOutcome(market_id, 0);

        assert_ok!(HybridRouter::buy(
            RuntimeOrigin::signed(ALICE),
            market_id,
            2,
            asset,
            _1_100,
            _3_4,
            vec![],
            Strategy::LimitOrder,
        ));

        assert_eq!(
            Orders::<Runtime>::get(0),
            Some(Order {
                market_id,
                maker: ALICE,
                maker_asset: base_asset,
                maker_amount: _1_100,
                taker_asset: asset,
                taker_amount: 133333334,
            })
        );
        assert_eq!(AssetManager::reserved_balance(base_asset, &ALICE), _1_100);
        assert_usdx_mirror_unchanged(issuance_before);
    });
}

#[test]
fn foreign_collateral_price_limit_failure_rolls_back_all_venues() {
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
        let asset = Asset::CategoricalOutcome(market_id, 0);
        assert_ok!(AssetManager::deposit(asset, &CHARLIE, _1));
        assert_ok!(Orderbook::place_order(
            RuntimeOrigin::signed(CHARLIE),
            market_id,
            asset,
            _1,
            base_asset,
            _2,
        ));
        let alice_base_before = AssetManager::free_balance(base_asset, &ALICE);
        let alice_outcome_before = AssetManager::free_balance(asset, &ALICE);
        let order_before = Orders::<Runtime>::get(0);

        assert_noop!(
            HybridRouter::buy(
                RuntimeOrigin::signed(ALICE),
                market_id,
                2,
                asset,
                _2,
                _3_4,
                vec![0],
                Strategy::LimitOrder,
            ),
            Error::<Runtime>::OrderPriceAboveMaxPrice
        );

        assert_eq!(
            AssetManager::free_balance(base_asset, &ALICE),
            alice_base_before
        );
        assert_eq!(
            AssetManager::free_balance(asset, &ALICE),
            alice_outcome_before
        );
        assert_eq!(Orders::<Runtime>::get(0), order_before);
        assert_usdx_mirror_unchanged(issuance_before);
    });
}
