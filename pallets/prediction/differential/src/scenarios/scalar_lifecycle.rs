//! Scalar market buy/report/resolve lifecycle without trading pools.
//! 无交易池的 scalar 市场买入/报告/决议生命周期。

use frame_support::assert_ok;
use sp_arithmetic::Perbill;
use zeitgeist_primitives::{
    constants::mock::CENT,
    types::{
        Asset, BlockNumber, MarketCreation, MarketDisputeMechanism, MarketPeriod, MarketStatus,
        MarketType, MultiHash, OutcomeReport, ScoringRule,
    },
};
use zrml_market_commons::MarketCommonsPalletApi;
use zrml_prediction_markets::mock::{
    run_blocks, run_to_block, ExtBuilder, PredictionMarkets, Runtime, RuntimeOrigin, ALICE, BOB,
    CHARLIE,
};

use super::capture::{finalize_market_snapshot, initial_native_balance, native_asset};
use crate::snapshot::ScenarioSnapshot;

pub const NAME: &str = "scalar_lifecycle_native";

pub fn scalar_lifecycle_native() -> ScenarioSnapshot {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::Ztg;
        let end = 2u32;
        assert_ok!(PredictionMarkets::create_market(
            RuntimeOrigin::signed(ALICE),
            base_asset,
            Perbill::zero(),
            BOB,
            MarketPeriod::Block(0..end),
            deadlines(),
            metadata(2),
            MarketCreation::Permissionless,
            MarketType::Scalar(100..=200),
            Some(MarketDisputeMechanism::Court),
            ScoringRule::AmmCdaHybrid,
        ));
        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(CHARLIE),
            0,
            CENT,
        ));

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        run_to_block(end + market.deadlines.grace_period + 1);
        assert_ok!(PredictionMarkets::report(
            RuntimeOrigin::signed(BOB),
            0,
            OutcomeReport::Scalar(150),
        ));
        run_blocks(market.deadlines.dispute_duration);

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.resolved_outcome, Some(OutcomeReport::Scalar(150)));

        let snapshot = finalize_market_snapshot(ScenarioSnapshot::new(NAME), 0, base_asset);
        assert_eq!(
            snapshot
                .balances
                .get(&(CHARLIE, native_asset()))
                .map(|b| b.free),
            Some(initial_native_balance() - CENT),
        );
        snapshot
    })
}

fn deadlines() -> zeitgeist_primitives::types::Deadlines<BlockNumber> {
    zeitgeist_primitives::types::Deadlines {
        grace_period: 1,
        oracle_duration: <Runtime as zrml_prediction_markets::Config>::MinOracleDuration::get(),
        dispute_duration: <Runtime as zrml_prediction_markets::Config>::MinDisputeDuration::get(),
    }
}

fn metadata(byte: u8) -> MultiHash {
    let mut metadata = [byte; 50];
    metadata[0] = 0x15;
    metadata[1] = 0x30;
    MultiHash::Sha3_384(metadata)
}
