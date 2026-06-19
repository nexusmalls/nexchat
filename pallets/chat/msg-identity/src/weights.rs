//! Weight definitions for `pallet-msg-identity`.
//! `pallet-msg-identity` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`),
//! generated on a dev chain (steps=50, repeat=20). Re-run on reference hardware
//! before mainnet.
//! CN: 由节点基准框架（见 `benchmarking.rs`）在 dev 链实测（steps=50, repeat=20）。
//! 上主网前应在基准硬件重跑。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-msg-identity`.
/// CN: `pallet-msg-identity` 所需的权重函数。
pub trait WeightInfo {
    fn register_device() -> Weight;
    fn set_signed_prekey() -> Weight;
    fn set_opk_root() -> Weight;
    fn bump_prekey_epoch() -> Weight;
    fn unregister_device() -> Weight;
    fn set_stack_caps() -> Weight;
    fn force_unregister_device() -> Weight;
}

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn register_device() -> Weight {
        Weight::from_parts(89_092_000, 3665)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn set_signed_prekey() -> Weight {
        Weight::from_parts(50_842_000, 3665)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_opk_root() -> Weight {
        Weight::from_parts(55_626_000, 3665)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn bump_prekey_epoch() -> Weight {
        Weight::from_parts(45_938_000, 3665)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn unregister_device() -> Weight {
        Weight::from_parts(105_353_000, 3665)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(4))
    }
    fn set_stack_caps() -> Weight {
        Weight::from_parts(29_010_000, 0)
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn force_unregister_device() -> Weight {
        Weight::from_parts(109_866_000, 3665)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(5))
    }
}

impl WeightInfo for () {
    fn register_device() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_signed_prekey() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn set_opk_root() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn bump_prekey_epoch() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn unregister_device() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_stack_caps() -> Weight { Weight::from_parts(12_000_000, 0) }
    fn force_unregister_device() -> Weight { Weight::from_parts(20_000_000, 0) }
}
