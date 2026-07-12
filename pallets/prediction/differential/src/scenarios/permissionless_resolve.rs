//! Permissionless categorical market that auto-resolves after the dispute window.
//! 争议窗口结束后自动决议的 permissionless categorical 市场。

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

pub const NAME: &str = "permissionless_resolve_native";

pub fn permissionless_resolve_native() -> ScenarioSnapshot {
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
            MarketType::Categorical(
                <Runtime as zrml_prediction_markets::Config>::MinCategories::get()
            ),
            Some(MarketDisputeMechanism::Authorized),
            ScoringRule::AmmCdaHybrid,
        ));
        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(CHARLIE),
            0,
            CENT,
        ));

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        let report_at = end + market.deadlines.grace_period + 1;
        run_to_block(report_at);
        assert_ok!(PredictionMarkets::report(
            RuntimeOrigin::signed(BOB),
            0,
            OutcomeReport::Categorical(1),
        ));
        run_blocks(market.deadlines.dispute_duration);

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.resolved_outcome, Some(OutcomeReport::Categorical(1)));

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
