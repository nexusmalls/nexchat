//! Snapshot comparison helpers with actionable mismatch output.
//! 带可操作差异输出的快照比较 helper。

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::snapshot::ScenarioSnapshot;

/// Compare two normalized snapshots and return human-readable mismatches.
/// 比较两个归一化快照并返回可读差异列表。
pub fn diff_snapshots(actual: &ScenarioSnapshot, expected: &ScenarioSnapshot) -> Vec<String> {
    let mut mismatches = Vec::new();

    if actual.name != expected.name {
        mismatches.push(format!(
            "scenario name: actual={:?}, expected={:?}",
            actual.name, expected.name
        ));
    }
    if actual.market_status != expected.market_status {
        mismatches.push(format!(
            "market status: actual={:?}, expected={:?}",
            actual.market_status, expected.market_status
        ));
    }
    if actual.resolved_outcome != expected.resolved_outcome {
        mismatches.push(format!(
            "resolved outcome: actual={:?}, expected={:?}",
            actual.resolved_outcome, expected.resolved_outcome
        ));
    }
    if actual.latest_market_id != expected.latest_market_id {
        mismatches.push(format!(
            "latest market id: actual={}, expected={}",
            actual.latest_market_id, expected.latest_market_id
        ));
    }
    if actual.creation_bond_settled != expected.creation_bond_settled {
        mismatches.push(format!(
            "creation bond settled: actual={}, expected={}",
            actual.creation_bond_settled, expected.creation_bond_settled
        ));
    }
    if actual.oracle_bond_settled != expected.oracle_bond_settled {
        mismatches.push(format!(
            "oracle bond settled: actual={}, expected={}",
            actual.oracle_bond_settled, expected.oracle_bond_settled
        ));
    }
    if actual.outsider_bond_settled != expected.outsider_bond_settled {
        mismatches.push(format!(
            "outsider bond settled: actual={}, expected={}",
            actual.outsider_bond_settled, expected.outsider_bond_settled
        ));
    }
    if actual.dispute_bond_settled != expected.dispute_bond_settled {
        mismatches.push(format!(
            "dispute bond settled: actual={}, expected={}",
            actual.dispute_bond_settled, expected.dispute_bond_settled
        ));
    }

    for (key, expected_balance) in &expected.balances {
        match actual.balances.get(key) {
            Some(actual_balance) if actual_balance == expected_balance => {}
            Some(actual_balance) => mismatches.push(format!(
                "balance {:?}: actual={actual_balance:?}, expected={expected_balance:?}",
                key
            )),
            None => mismatches.push(format!(
                "balance {:?}: missing actual entry, expected={expected_balance:?}",
                key
            )),
        }
    }

    for key in actual.balances.keys() {
        if !expected.balances.contains_key(key) {
            mismatches.push(format!(
                "balance {:?}: unexpected actual entry {:?}",
                key,
                actual.balances.get(key)
            ));
        }
    }

    mismatches
}

/// Assert that `actual` matches `expected`, panicking with a diff on failure.
/// 断言 `actual` 与 `expected` 一致，失败时输出差异。
pub fn assert_snapshot_eq(actual: &ScenarioSnapshot, expected: &ScenarioSnapshot) {
    let mismatches = diff_snapshots(actual, expected);
    if !mismatches.is_empty() {
        panic!(
            "differential baseline mismatch for {:?}:\n{}",
            actual.name,
            mismatches.join("\n")
        );
    }
}
