//! Live foreign-collateral conservation tests for combinatorial operations.
//! 组合代币操作的实时外部抵押守恒测试。

use super::*;
use crate::mock::ext_builder::INITIAL_FOREIGN_BALANCE;

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
fn foreign_collateral_split_merge_roundtrip_is_conservative() {
    ExtBuilder::build().execute_with(|| {
        let alice = Account::new(0);
        let collateral = Asset::ForeignAsset(USDX_ASSET_ID);
        let collateral_before = alice.free_balance(collateral);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market(collateral, MarketType::Categorical(2));
        let partition = vec![vec![B1, B0], vec![B0, B1]];
        let amount = _10;

        assert_ok!(CombinatorialTokens::split_position(
            alice.signed(),
            None,
            market_id,
            partition.clone(),
            amount,
            Fuel::new(16, false),
        ));
        assert_eq!(alice.free_balance(collateral), collateral_before - amount);
        assert_usdx_mirror_unchanged(issuance_before);

        assert_ok!(CombinatorialTokens::merge_position(
            alice.signed(),
            None,
            market_id,
            partition,
            amount,
            Fuel::new(16, false),
        ));
        assert_eq!(alice.free_balance(collateral), collateral_before);
        assert_eq!(collateral_before, INITIAL_FOREIGN_BALANCE);
        assert_usdx_mirror_unchanged(issuance_before);
    });
}

#[test]
fn foreign_collateral_redeem_is_conservative() {
    ExtBuilder::build().execute_with(|| {
        let alice = Account::new(0);
        let collateral = Asset::ForeignAsset(USDX_ASSET_ID);
        let collateral_before = alice.free_balance(collateral);
        let issuance_before = PredictionCollateral::mirror_issuance(USDX_ASSET_ID);
        let market_id = create_market(collateral, MarketType::Categorical(2));
        let winning_index_set = vec![B1, B0];
        let partition = vec![winning_index_set.clone(), vec![B0, B1]];
        let amount = _10;
        let winning_position = Pallet::<Runtime>::position_from_parent_collection(
            None,
            market_id,
            winning_index_set.clone(),
            Fuel::new(16, false),
        )
        .unwrap();

        assert_ok!(CombinatorialTokens::split_position(
            alice.signed(),
            None,
            market_id,
            partition,
            amount,
            Fuel::new(16, false),
        ));
        MockPayout::set_return_value(Some(vec![_1, 0]));
        assert_ok!(CombinatorialTokens::redeem_position(
            alice.signed(),
            None,
            market_id,
            winning_index_set,
            Fuel::new(16, false),
        ));

        assert_eq!(alice.free_balance(winning_position), 0);
        assert_eq!(alice.free_balance(collateral), collateral_before);
        assert_usdx_mirror_unchanged(issuance_before);
    });
}
