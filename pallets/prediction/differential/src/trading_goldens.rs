//! Upstream-derived Phase 4 trading goldens.
//! 上游导出的 Phase 4 交易 golden。

use crate::{
    snapshot::TradingSnapshot,
    trading_scenarios::{
        trading_snapshots, COMBINATORIAL_ROUNDTRIP, ORDERBOOK_PARTIAL_FILL, PARIMUTUEL_NO_WINNER,
    },
};

fn expected_snapshots() -> Vec<TradingSnapshot> {
    let mut orderbook = TradingSnapshot::new(ORDERBOOK_PARTIAL_FILL);
    orderbook.value("order.maker_amount", 250_000_000_000);
    orderbook.value("order.taker_amount", 50_000_000_000);
    orderbook.value("maker.reserved_base", 250_000_000_000);
    orderbook.value("taker.free_base", 1_247_500_000_000);
    orderbook.value("fee_recipient.free_base", 2_500_000_000);
    orderbook.checkpoint("order_remains", true);

    let mut parimutuel = TradingSnapshot::new(PARIMUTUEL_NO_WINNER);
    parimutuel.value("alice.free_base", 9_998_000_000_000);
    parimutuel.value("bob.free_base", 9_999_000_000_000);
    parimutuel.value("fee_recipient.free_base", 3_000_000_000);
    parimutuel.value("pot.free_base", 0);
    parimutuel.checkpoint("shares_burned", true);

    let mut combinatorial = TradingSnapshot::new(COMBINATORIAL_ROUNDTRIP);
    combinatorial.value("account.free_base", 100_000_000_000);
    combinatorial.value("pallet.free_base", 0);
    combinatorial.value("first_position.issuance", 0);
    combinatorial.checkpoint("roundtrip_restored", true);

    vec![orderbook, parimutuel, combinatorial]
}

#[test]
fn phase4_trading_scenarios_match_upstream_goldens() {
    assert_eq!(trading_snapshots(), expected_snapshots());
}

#[test]
fn phase4_trading_scenario_names_are_unique() {
    let snapshots = trading_snapshots();
    let names = snapshots
        .iter()
        .map(|snapshot| snapshot.name)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names.len(), snapshots.len());
}
