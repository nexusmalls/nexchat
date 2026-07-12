//! Trusted market reported and resolved without a dispute mechanism.
//! 无争议机制、直接报告并决议的 trusted 市场。

use frame_support::assert_ok;
use sp_arithmetic::Perbill;
use sp_runtime::traits::Zero;
use zeitgeist_primitives::{
    constants::mock::BASE,
    types::{
        Asset, BlockNumber, MarketCreation, MarketPeriod, MarketStatus, MarketType, MultiHash,
        OutcomeReport, ScoringRule,
    },
};
use zrml_market_commons::MarketCommonsPalletApi;
use zrml_prediction_markets::mock::{
    run_to_block, ExtBuilder, PredictionMarkets, Runtime, RuntimeOrigin, ALICE, BOB, FRED,
};

use super::capture::finalize_market_snapshot;
use crate::snapshot::ScenarioSnapshot;

pub const NAME: &str = "trusted_market_native";

pub fn trusted_market_native() -> ScenarioSnapshot {
    ExtBuilder::default().build().execute_with(|| {
        let base_asset = Asset::Ztg;
        let end = 3u32;
        assert_ok!(PredictionMarkets::create_market(
            RuntimeOrigin::signed(ALICE),
            base_asset,
            Perbill::zero(),
            BOB,
            MarketPeriod::Block(0..end),
            deadlines(),
            metadata(0x99),
            MarketCreation::Permissionless,
            MarketType::Categorical(3),
            None,
            ScoringRule::AmmCdaHybrid,
        ));
        assert_ok!(PredictionMarkets::buy_complete_set(
            RuntimeOrigin::signed(FRED),
            0,
            BASE,
        ));
        run_to_block(end);
        assert_ok!(PredictionMarkets::report(
            RuntimeOrigin::signed(BOB),
            0,
            OutcomeReport::Categorical(1),
        ));
        assert_ok!(PredictionMarkets::redeem_shares(
            RuntimeOrigin::signed(FRED),
            0,
        ));

        let market = zrml_market_commons::Pallet::<Runtime>::market(&0).unwrap();
        assert_eq!(market.status, MarketStatus::Resolved);

        finalize_market_snapshot(ScenarioSnapshot::new(NAME), 0, base_asset)
    })
}

fn deadlines() -> zeitgeist_primitives::types::Deadlines<BlockNumber> {
    zeitgeist_primitives::types::Deadlines {
        grace_period: 0,
        oracle_duration: <Runtime as zrml_prediction_markets::Config>::MinOracleDuration::get(),
        dispute_duration: Zero::zero(),
    }
}

fn metadata(byte: u8) -> MultiHash {
    let mut metadata = [byte; 50];
    metadata[0] = 0x15;
    metadata[1] = 0x30;
    MultiHash::Sha3_384(metadata)
}
