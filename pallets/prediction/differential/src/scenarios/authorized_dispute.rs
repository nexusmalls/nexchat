//! Authorized dispute path with correction-period auto-resolution.
//! 经修正期自动决议的 Authorized 争议路径。

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
    run_blocks, run_to_block, Authorized, ExtBuilder, PredictionMarkets, Runtime, RuntimeOrigin,
    ALICE, BOB, CHARLIE,
};

use super::capture::finalize_market_snapshot;
use crate::snapshot::ScenarioSnapshot;

pub const NAME: &str = "authorized_dispute_native";

pub fn authorized_dispute_native() -> ScenarioSnapshot {
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
        run_to_block(end + market.deadlines.grace_period + 1);
        assert_ok!(PredictionMarkets::report(
            RuntimeOrigin::signed(BOB),
            0,
            OutcomeReport::Categorical(0),
        ));
        run_to_block(end + market.deadlines.grace_period + 2);
        assert_ok!(PredictionMarkets::dispute(
            RuntimeOrigin::signed(CHARLIE),
            0,
        ));
        assert_ok!(Authorized::authorize_market_outcome(
            RuntimeOrigin::signed(ALICE),
            0,
            OutcomeReport::Categorical(0),
        ));
        assert_ok!(Authorized::authorize_market_outcome(
            RuntimeOrigin::signed(ALICE),
            0,
            OutcomeReport::Categorical(1),
        ));
        run_blocks(<Runtime as zrml_authorized::Config>::CorrectionPeriod::get());
        assert_ok!(PredictionMarkets::redeem_shares(
            RuntimeOrigin::signed(CHARLIE),
            0,
        ));

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        assert_eq!(market.status, MarketStatus::Resolved);

        let snapshot = finalize_market_snapshot(ScenarioSnapshot::new(NAME), 0, base_asset);
        assert!(snapshot.oracle_bond_settled);
        assert!(snapshot.creation_bond_settled);
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
