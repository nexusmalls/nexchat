//! Weight definitions for `pallet-task-bounty`.
//! `pallet-task-bounty` 的权重定义。
//!
//! Phase 1 ships placeholder constant weights; replace with benchmarked values
//! (`benchmarking.rs`) before mainnet, mirroring escrow/arbitration style.
//! Phase 1 使用占位常量权重；上主网前用基准实测值替换（见 `benchmarking.rs`），
//! 风格对齐 escrow/arbitration。

use frame_support::{traits::Get, weights::Weight};

/// Weight functions needed for the pallet. / 模块所需权重函数。
pub trait WeightInfo {
    fn create_bounty() -> Weight;
    fn submit() -> Weight;
    fn deliver() -> Weight;
    fn accept() -> Weight;
    fn withdraw_submission() -> Weight;
    fn cancel_bounty() -> Weight;
    fn open_dispute() -> Weight;
    fn expire_bounty() -> Weight;
    fn set_meta() -> Weight;
}

/// Placeholder weights (reads/writes only). / 占位权重（仅读写计）。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

macro_rules! placeholder {
    ($name:ident, $reads:expr, $writes:expr) => {
        fn $name() -> Weight {
            Weight::from_parts(20_000_000, 0)
                .saturating_add(T::DbWeight::get().reads($reads))
                .saturating_add(T::DbWeight::get().writes($writes))
        }
    };
}

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    placeholder!(create_bounty, 2, 3);
    placeholder!(submit, 2, 3);
    placeholder!(deliver, 2, 1);
    placeholder!(accept, 4, 4);
    placeholder!(withdraw_submission, 2, 2);
    placeholder!(cancel_bounty, 2, 2);
    placeholder!(open_dispute, 3, 2);
    placeholder!(expire_bounty, 3, 3);
    placeholder!(set_meta, 1, 1);
}

impl WeightInfo for () {
    fn create_bounty() -> Weight {
        Weight::from_parts(20_000_000, 0)
    }
    fn submit() -> Weight {
        Weight::from_parts(20_000_000, 0)
    }
    fn deliver() -> Weight {
        Weight::from_parts(10_000_000, 0)
    }
    fn accept() -> Weight {
        Weight::from_parts(30_000_000, 0)
    }
    fn withdraw_submission() -> Weight {
        Weight::from_parts(15_000_000, 0)
    }
    fn cancel_bounty() -> Weight {
        Weight::from_parts(15_000_000, 0)
    }
    fn open_dispute() -> Weight {
        Weight::from_parts(15_000_000, 0)
    }
    fn expire_bounty() -> Weight {
        Weight::from_parts(20_000_000, 0)
    }
    fn set_meta() -> Weight {
        Weight::from_parts(15_000_000, 0)
    }
}
