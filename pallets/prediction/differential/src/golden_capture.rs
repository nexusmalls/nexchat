//! Temporary helper to print scenario snapshots for golden pinning.
//! 用于固定 golden 的临时快照打印 helper。

use crate::scenarios::scenario_catalog;

#[test]
#[ignore = "manual golden capture helper"]
fn print_scenario_snapshots_for_golden_pinning() {
    for entry in scenario_catalog() {
        let snapshot = (entry.run)();
        println!("{entry:?}", entry = snapshot.name);
        println!("{snapshot:#?}");
    }
}
