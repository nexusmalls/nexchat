//! Court escalation through GlobalDisputes to final automatic resolution.
//! Court 升级至 GlobalDisputes 并完成最终自动决议的场景。

use frame_support::assert_ok;
use orml_traits::MultiCurrency;
use sp_arithmetic::Perbill;
use sp_runtime::traits::{BlakeTwo256, Hash, Zero};
use zeitgeist_primitives::{
    constants::mock::BASE,
    types::{
        Asset, BlockNumber, MarketCreation, MarketDisputeMechanism, MarketId, MarketPeriod,
        MarketStatus, MarketType, MultiHash, OutcomeReport, ScoringRule,
    },
};
use zrml_court::types::VoteItem;
use zrml_global_disputes::GlobalDisputesPalletApi;
use zrml_market_commons::MarketCommonsPalletApi;
use zrml_prediction_markets::{
    mock::{
        run_to_block, AssetManager, Balances, Court, ExtBuilder, GlobalDisputes, MarketCommons,
        PredictionMarkets, Runtime, RuntimeOrigin, ALICE, BOB, CHARLIE,
    },
    MarketIdsPerDisputeBlock,
};

use super::capture::finalize_market_snapshot;
use crate::snapshot::ScenarioSnapshot;

pub const NAME: &str = "court_global_dispute_native";

pub fn court_global_dispute_native() -> ScenarioSnapshot {
    ExtBuilder::default().build().execute_with(|| {
        seed_court();

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
            Some(MarketDisputeMechanism::Court),
            ScoringRule::AmmCdaHybrid,
        ));

        let market_id = 0;
        let market = MarketCommons::market(&market_id).expect("market exists");
        run_to_block(end + market.deadlines.grace_period + 1);
        assert_ok!(PredictionMarkets::report(
            RuntimeOrigin::signed(BOB),
            market_id,
            OutcomeReport::Categorical(0),
        ));
        assert_ok!(PredictionMarkets::dispute(
            RuntimeOrigin::signed(CHARLIE),
            market_id,
        ));

        let bob_free_before_appeals = Balances::free_balance(BOB);
        for _ in 0..<Runtime as zrml_court::Config>::MaxAppeals::get() {
            simulate_appeal_cycle(market_id);
            assert_ok!(Court::appeal(RuntimeOrigin::signed(BOB), market_id));
        }

        let court_id =
            zrml_court::MarketIdToCourtId::<Runtime>::get(market_id).expect("court mapping exists");
        assert!(Balances::reserved_balance(BOB) > 0);
        assert_ok!(PredictionMarkets::start_global_dispute(
            RuntimeOrigin::signed(BOB),
            market_id,
        ));

        let now = frame_system::Pallet::<Runtime>::block_number();
        let add_outcome_end = now + GlobalDisputes::get_add_outcome_period();
        let vote_end = add_outcome_end + GlobalDisputes::get_vote_period();
        run_to_block(add_outcome_end + 1);
        assert_ok!(GlobalDisputes::vote_on_outcome(
            RuntimeOrigin::signed(BOB),
            market_id,
            OutcomeReport::Categorical(0),
            <Runtime as zrml_global_disputes::Config>::MinOutcomeVoteAmount::get(),
        ));
        run_to_block(vote_end);

        let market = MarketCommons::market(&market_id).expect("resolved market exists");
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.resolved_outcome, Some(OutcomeReport::Categorical(0)));

        let mut snapshot =
            finalize_market_snapshot(ScenarioSnapshot::new(NAME), market_id, base_asset);
        snapshot.checkpoint(
            "global_dispute_inactive",
            !GlobalDisputes::is_active(&market_id),
        );
        snapshot.checkpoint(
            "resolution_queue_empty",
            MarketIdsPerDisputeBlock::<Runtime>::get(vote_end).is_empty(),
        );
        snapshot.checkpoint(
            "court_market_mapping_removed",
            zrml_court::MarketIdToCourtId::<Runtime>::get(market_id).is_none(),
        );
        snapshot.checkpoint(
            "court_reverse_mapping_removed",
            zrml_court::CourtIdToMarketId::<Runtime>::get(court_id).is_none(),
        );
        snapshot.checkpoint(
            "court_record_removed",
            zrml_court::Courts::<Runtime>::get(court_id).is_none(),
        );
        snapshot.checkpoint(
            "appeal_reserve_released",
            Balances::reserved_balance(BOB).is_zero(),
        );
        snapshot.checkpoint(
            "appeal_free_restored",
            Balances::free_balance(BOB) == bob_free_before_appeals,
        );
        snapshot
    })
}

fn seed_court() {
    let jurors = 1000..(1000 + <Runtime as zrml_court::Config>::MaxSelectedDraws::get() as u128);
    for juror in jurors {
        let amount = <Runtime as zrml_court::Config>::MinJurorStake::get() + juror;
        assert_ok!(AssetManager::deposit(Asset::Ztg, &juror, amount + BASE,));
        assert_ok!(Court::join_court(RuntimeOrigin::signed(juror), amount,));
    }
}

fn simulate_appeal_cycle(market_id: MarketId) {
    let court = zrml_court::Courts::<Runtime>::get(market_id).expect("court exists");
    run_to_block(court.round_ends.pre_vote + 1);

    let salt = <Runtime as frame_system::Config>::Hash::default();
    let vote_item = VoteItem::Outcome(OutcomeReport::Categorical(1));
    let draws = zrml_court::SelectedDraws::<Runtime>::get(market_id);
    for draw in &draws {
        let commitment = BlakeTwo256::hash_of(&(draw.court_participant, vote_item.clone(), salt));
        assert_ok!(Court::vote(
            RuntimeOrigin::signed(draw.court_participant),
            market_id,
            commitment,
        ));
    }

    run_to_block(court.round_ends.vote + 1);
    for draw in draws {
        assert_ok!(Court::reveal_vote(
            RuntimeOrigin::signed(draw.court_participant),
            market_id,
            vote_item.clone(),
            salt,
        ));
    }

    let resolve_at = court.round_ends.appeal;
    assert_eq!(
        MarketIdsPerDisputeBlock::<Runtime>::get(resolve_at),
        vec![market_id]
    );
    run_to_block(resolve_at - 1);
    assert_eq!(
        MarketCommons::market(&market_id)
            .expect("market exists")
            .status,
        MarketStatus::Disputed
    );
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
