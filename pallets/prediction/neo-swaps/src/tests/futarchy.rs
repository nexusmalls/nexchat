// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::types::{DecisionMarketOracle, DecisionMarketOracleScoreboard};
use zeitgeist_primitives::traits::FutarchyOracle;

fn decision_oracle(
    positive_price: BalanceOf<Runtime>,
    negative_price: BalanceOf<Runtime>,
) -> DecisionMarketOracle<Runtime> {
    let market_id = create_market_and_deploy_pool(
        ALICE,
        BASE_ASSET,
        MarketType::Categorical(2),
        _10,
        vec![positive_price, negative_price],
        CENT,
    );
    let assets = Pools::<Runtime>::get(market_id).unwrap().assets();
    let scoreboard = DecisionMarketOracleScoreboard::new(1, 1, _1_10, _1_10);
    DecisionMarketOracle::new(market_id, assets[0], assets[1], scoreboard)
}

#[test]
fn decision_market_oracle_accepts_positive_threshold() {
    ExtBuilder::default().build().execute_with(|| {
        let mut oracle = decision_oracle(_3_4, _1_4);
        oracle.update(1);
        assert!(oracle.evaluate().1);
    });
}

#[test]
fn decision_market_oracle_rejects_negative_threshold() {
    ExtBuilder::default().build().execute_with(|| {
        let mut oracle = decision_oracle(_1_4, _3_4);
        oracle.update(1);
        assert!(!oracle.evaluate().1);
    });
}

#[test]
fn decision_market_scoreboard_requires_victory_margin() {
    ExtBuilder::default().build().execute_with(|| {
        let mut scoreboard = DecisionMarketOracleScoreboard::<Runtime>::new(5, 2, _1_10, _1_10);
        scoreboard.update(4, _3_4, _1_4);
        assert!(!scoreboard.evaluate());
        scoreboard.update(5, _3_4, _1_4);
        assert!(!scoreboard.evaluate());
        scoreboard.update(6, _3_4, _1_4);
        assert!(scoreboard.evaluate());
    });
}
