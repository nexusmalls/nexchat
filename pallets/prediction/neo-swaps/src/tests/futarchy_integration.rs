// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::types::{DecisionMarketOracle, DecisionMarketOracleScoreboard};
use frame_support::traits::{Get, Hooks, StorePreimage};
use pallet_prediction_control::{GlobalMode, PredictionMode};
use zeitgeist_primitives::types::BlockNumber;
use zrml_futarchy::types::Proposal;

fn run_futarchy_and_scheduler_to(target: BlockNumber) {
    while System::block_number() < target {
        let block = System::block_number() + 1;
        System::set_block_number(block);
        let _ = Futarchy::on_initialize(block);
        let _ = Scheduler::on_initialize(block);
    }
}

fn proposal(
    positive_price: BalanceOf<Runtime>,
    negative_price: BalanceOf<Runtime>,
) -> (Proposal<Runtime>, BlockNumber) {
    let market_id = create_market_and_deploy_pool(
        ALICE,
        BASE_ASSET,
        MarketType::Categorical(2),
        _10,
        vec![positive_price, negative_price],
        CENT,
    );
    let assets = Pools::<Runtime>::get(market_id).unwrap().assets();
    let duration = <Runtime as zrml_futarchy::Config>::MinDuration::get();
    let when = System::block_number() + duration + 2;
    let scoreboard = DecisionMarketOracleScoreboard::new(System::block_number(), 2, _1_10, _1_10);
    let oracle = DecisionMarketOracle::new(market_id, assets[0], assets[1], scoreboard);
    let call =
        RuntimeCall::PredictionControl(pallet_prediction_control::Call::set_prediction_mode {
            new: PredictionMode::Trading,
        });
    let call = Preimage::bound(call).unwrap();
    (Proposal { when, call, oracle }, duration)
}

#[test]
fn positive_decision_market_schedules_and_executes_real_scheduler_call() {
    ExtBuilder::default().build().execute_with(|| {
        let (proposal, duration) = proposal(_9_10, _1_10);
        let when = proposal.when;
        assert_ok!(Futarchy::submit_proposal(
            RuntimeOrigin::root(),
            duration,
            proposal,
        ));

        run_futarchy_and_scheduler_to(when);

        assert_eq!(GlobalMode::<Runtime>::get(), PredictionMode::Trading);
        System::assert_has_event(
            pallet_scheduler::Event::<Runtime>::Dispatched {
                task: (when, 0),
                id: None,
                result: Ok(()),
            }
            .into(),
        );
    });
}

#[test]
fn negative_decision_market_rejects_before_real_scheduler() {
    ExtBuilder::default().build().execute_with(|| {
        let (proposal, duration) = proposal(_1_10, _9_10);
        let evaluation_at = System::block_number() + duration;
        let when = proposal.when;
        let mode_before = GlobalMode::<Runtime>::get();
        assert_ok!(Futarchy::submit_proposal(
            RuntimeOrigin::root(),
            duration,
            proposal,
        ));

        run_futarchy_and_scheduler_to(evaluation_at);

        assert_eq!(GlobalMode::<Runtime>::get(), mode_before);
        assert!(pallet_scheduler::Agenda::<Runtime>::get(when).is_empty());
        assert!(System::events().iter().any(|record| matches!(
            record.event,
            RuntimeEvent::Futarchy(zrml_futarchy::Event::Rejected { .. })
        )));
    });
}
