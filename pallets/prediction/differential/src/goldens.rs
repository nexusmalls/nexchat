//! Upstream-derived golden snapshots for the differential baseline.
//! 差分基线的上游导出 golden 快照。
//!
//! Values pinned from Nexus scenarios verified against upstream commit
//! `39ad8d60aa2f7af0a465d58c5e87dcc509602df5`.
//! 数值来自已与上游 commit `39ad8d60` 对拍确认的 Nexus 场景。

use alloc::collections::BTreeMap;

use zeitgeist_primitives::types::MarketStatus;

use crate::{
    normalize::NormalizedAsset,
    snapshot::{AccountBalance, ScenarioSnapshot},
};

const INITIAL: u128 = 10_000_000_000_000;
const CENT: u128 = 100_000_000;
const ORACLE_BOND: u128 = 50 * CENT;

/// Build the pinned golden set.
/// 构建固定的 golden 集合。
pub fn goldens() -> Vec<ScenarioSnapshot> {
    vec![
        permissionless_resolve_native_golden(),
        authorized_dispute_native_golden(),
        court_global_dispute_native_golden(),
        trusted_market_native_golden(),
        scalar_lifecycle_native_golden(),
    ]
}

fn balance(free: u128) -> AccountBalance {
    AccountBalance { free, reserved: 0 }
}

fn permissionless_resolve_native_golden() -> ScenarioSnapshot {
    let mut balances = BTreeMap::new();
    balances.insert((0, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((1, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((2, NormalizedAsset::Native), balance(INITIAL - CENT));

    ScenarioSnapshot {
        name: "permissionless_resolve_native",
        market_status: MarketStatus::Resolved,
        resolved_outcome: Some("categorical:1".into()),
        latest_market_id: 0,
        balances,
        checkpoints: BTreeMap::new(),
        creation_bond_settled: true,
        oracle_bond_settled: true,
        outsider_bond_settled: false,
        dispute_bond_settled: false,
    }
}

fn authorized_dispute_native_golden() -> ScenarioSnapshot {
    let mut balances = BTreeMap::new();
    balances.insert((0, NormalizedAsset::Native), balance(9_997_500_000_000));
    balances.insert((1, NormalizedAsset::Native), balance(INITIAL));
    balances.insert(
        (2, NormalizedAsset::Native),
        balance(INITIAL + ORACLE_BOND / 2),
    );

    ScenarioSnapshot {
        name: "authorized_dispute_native",
        market_status: MarketStatus::Resolved,
        resolved_outcome: Some("categorical:1".into()),
        latest_market_id: 0,
        balances,
        checkpoints: BTreeMap::new(),
        creation_bond_settled: true,
        oracle_bond_settled: true,
        outsider_bond_settled: false,
        dispute_bond_settled: true,
    }
}

fn court_global_dispute_native_golden() -> ScenarioSnapshot {
    // Nexus intentionally retains CourtInfo until final outcome-dependent appeal
    // settlement; fixed upstream deletes it during escalation and cannot complete
    // this final-resolution snapshot.
    // Nexus 有意保留 CourtInfo 到依赖最终结果的 appeal 结算完成；固定上游会在
    // 升级时删除该记录，因此无法完成此最终决议快照。
    let mut balances = BTreeMap::new();
    balances.insert((0, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((1, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((2, NormalizedAsset::Native), balance(9_989_100_000_000));

    let checkpoints = [
        "appeal_free_restored",
        "appeal_reserve_released",
        "court_market_mapping_removed",
        "court_record_removed",
        "court_reverse_mapping_removed",
        "global_dispute_inactive",
        "resolution_queue_empty",
    ]
    .into_iter()
    .map(|name| (name.into(), true))
    .collect();

    ScenarioSnapshot {
        name: "court_global_dispute_native",
        market_status: MarketStatus::Resolved,
        resolved_outcome: Some("categorical:0".into()),
        latest_market_id: 0,
        balances,
        checkpoints,
        creation_bond_settled: true,
        oracle_bond_settled: true,
        outsider_bond_settled: false,
        dispute_bond_settled: true,
    }
}

fn trusted_market_native_golden() -> ScenarioSnapshot {
    let mut balances = BTreeMap::new();
    balances.insert((0, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((1, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((2, NormalizedAsset::Native), balance(INITIAL));

    ScenarioSnapshot {
        name: "trusted_market_native",
        market_status: MarketStatus::Resolved,
        resolved_outcome: Some("categorical:1".into()),
        latest_market_id: 0,
        balances,
        checkpoints: BTreeMap::new(),
        creation_bond_settled: true,
        oracle_bond_settled: true,
        outsider_bond_settled: false,
        dispute_bond_settled: false,
    }
}

fn scalar_lifecycle_native_golden() -> ScenarioSnapshot {
    let mut balances = BTreeMap::new();
    balances.insert((0, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((1, NormalizedAsset::Native), balance(INITIAL));
    balances.insert((2, NormalizedAsset::Native), balance(INITIAL - CENT));

    ScenarioSnapshot {
        name: "scalar_lifecycle_native",
        market_status: MarketStatus::Resolved,
        resolved_outcome: Some("scalar:150".into()),
        latest_market_id: 0,
        balances,
        checkpoints: BTreeMap::new(),
        creation_bond_settled: true,
        oracle_bond_settled: true,
        outsider_bond_settled: false,
        dispute_bond_settled: false,
    }
}
