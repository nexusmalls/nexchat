//! Scripted lifecycle scenarios for the differential baseline.
//! 差分基线的脚本化生命周期场景。

mod authorized_dispute;
mod capture;
mod court_global_dispute;
mod permissionless_resolve;
mod scalar_lifecycle;
mod trusted_market;

pub use authorized_dispute::authorized_dispute_native;
pub use court_global_dispute::court_global_dispute_native;
pub use permissionless_resolve::permissionless_resolve_native;
pub use scalar_lifecycle::scalar_lifecycle_native;
pub use trusted_market::trusted_market_native;

use crate::snapshot::ScenarioSnapshot;

pub const UPSTREAM_BASELINE_COMMIT: &str = "39ad8d60aa2f7af0a465d58c5e87dcc509602df5";

/// Catalog entry executed by the differential harness.
/// 差分框架执行的场景目录项。
pub struct ScenarioEntry {
    pub name: &'static str,
    pub run: fn() -> ScenarioSnapshot,
}

/// All scenarios pinned by the Phase 3 differential baseline.
/// Phase 3 差分基线固定的全部场景。
pub fn scenario_catalog() -> &'static [ScenarioEntry] {
    &[
        ScenarioEntry {
            name: permissionless_resolve::NAME,
            run: permissionless_resolve_native,
        },
        ScenarioEntry {
            name: authorized_dispute::NAME,
            run: authorized_dispute_native,
        },
        ScenarioEntry {
            name: court_global_dispute::NAME,
            run: court_global_dispute_native,
        },
        ScenarioEntry {
            name: trusted_market::NAME,
            run: trusted_market_native,
        },
        ScenarioEntry {
            name: scalar_lifecycle::NAME,
            run: scalar_lifecycle_native,
        },
    ]
}
