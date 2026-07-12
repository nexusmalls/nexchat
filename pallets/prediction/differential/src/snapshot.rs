//! Normalized scenario snapshots compared by the differential harness.
//! 差分框架用于对比的归一化场景快照。

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use zeitgeist_primitives::types::MarketStatus;

use crate::normalize::NormalizedAsset;

/// Ledger balance tracked after Nexus/ upstream normalization.
/// 经 Nexus/上游归一化后跟踪的账本余额。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBalance {
    pub free: u128,
    pub reserved: u128,
}

/// Business-level snapshot captured at the end of a scripted scenario.
/// 脚本化场景结束时捕获的业务层快照。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioSnapshot {
    pub name: &'static str,
    pub market_status: MarketStatus,
    pub resolved_outcome: Option<String>,
    pub latest_market_id: u128,
    pub balances: BTreeMap<(u128, NormalizedAsset), AccountBalance>,
    pub creation_bond_settled: bool,
    pub oracle_bond_settled: bool,
    pub outsider_bond_settled: bool,
    pub dispute_bond_settled: bool,
}

impl ScenarioSnapshot {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            market_status: MarketStatus::Proposed,
            resolved_outcome: None,
            latest_market_id: 0,
            balances: BTreeMap::new(),
            creation_bond_settled: false,
            oracle_bond_settled: false,
            outsider_bond_settled: false,
            dispute_bond_settled: false,
        }
    }

    pub fn track_balance(
        &mut self,
        account: u128,
        asset: NormalizedAsset,
        free: u128,
        reserved: u128,
    ) {
        self.balances
            .insert((account, asset), AccountBalance { free, reserved });
    }

    pub fn with_outcome(mut self, outcome: impl Into<String>) -> Self {
        self.resolved_outcome = Some(outcome.into());
        self
    }
}
