//! Weight definitions for `pallet-chat-permission`.
//! `pallet-chat-permission` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`).
//! Generated on a dev chain (steps=50, repeat=20); re-run on reference hardware
//! before mainnet. CN: 由节点基准框架（见 `benchmarking.rs`）实测得到的权重，
//! 在 dev 链上生成（steps=50, repeat=20）；上主网前应在基准硬件上重跑。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-chat-permission`.
/// CN: `pallet-chat-permission` 所需的权重函数。
pub trait WeightInfo {
    fn set_permission_level() -> Weight;
    fn set_rejected_scene_types() -> Weight;
    fn bump_capability_epoch() -> Weight;
    fn force_mute_account() -> Weight;
    fn force_unmute_account() -> Weight;
    fn report() -> Weight;
    fn resolve_report() -> Weight;
}

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Storage: PrivacySettingsOf (r:1 w:1).
    fn set_permission_level() -> Weight {
        Weight::from_parts(32_590_000, 3859)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: PrivacySettingsOf (r:1 w:1).
    fn set_rejected_scene_types() -> Weight {
        Weight::from_parts(35_060_000, 3859)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: CapabilityEpoch (r:1 w:1).
    fn bump_capability_epoch() -> Weight {
        Weight::from_parts(35_767_000, 3517)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: MutedAccounts (r:0 w:1).
    fn force_mute_account() -> Weight {
        Weight::from_parts(25_062_000, 0)
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: MutedAccounts (r:0 w:1).
    fn force_unmute_account() -> Weight {
        Weight::from_parts(24_242_000, 0)
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: LastReportAt, OpenReportCount, NextReportId (r:3 w:4 incl. Reports).
    fn report() -> Weight {
        Weight::from_parts(48_139_000, 3517)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(4))
    }
    /// Storage: Reports, OpenReportCount (r:2 w:2).
    fn resolve_report() -> Weight {
        Weight::from_parts(47_557_000, 3680)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
}

impl WeightInfo for () {
    fn set_permission_level() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn set_rejected_scene_types() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn bump_capability_epoch() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn force_mute_account() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn force_unmute_account() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn report() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn resolve_report() -> Weight { Weight::from_parts(15_000_000, 0) }
}
