// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Phase 2 non-production weights for prediction collateral.
//! 预测抵押镜像 Phase 2 非生产权重。
//!
//! Phase 7 must replace these estimates with Nexus benchmark output.
//! Phase 7 必须使用 Nexus benchmark 结果替换这些估算值。

use core::marker::PhantomData;
use frame_support::{
    traits::Get,
    weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions required by prediction-collateral calls.
/// Prediction-collateral 调用所需的权重函数。
pub trait WeightInfo {
    fn deposit() -> Weight;
    fn withdraw() -> Weight;
    fn set_asset_whitelisted() -> Weight;
    fn set_asset_deposit_paused() -> Weight;
    fn set_global_deposit_paused() -> Weight;
}

/// Phase 2 database-weight adapter; not benchmark-generated.
/// Phase 2 数据库权重适配器；并非 benchmark 生成。
pub struct Phase2Weight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for Phase2Weight<T> {
    fn deposit() -> Weight {
        Weight::from_parts(80_000_000, 8_000).saturating_add(T::DbWeight::get().reads_writes(10, 4))
    }

    fn withdraw() -> Weight {
        Weight::from_parts(80_000_000, 8_000).saturating_add(T::DbWeight::get().reads_writes(7, 4))
    }

    fn set_asset_whitelisted() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_asset_deposit_paused() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }

    fn set_global_deposit_paused() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }
}

impl WeightInfo for () {
    fn deposit() -> Weight {
        Weight::from_parts(80_000_000, 8_000)
            .saturating_add(RocksDbWeight::get().reads_writes(10, 4))
    }

    fn withdraw() -> Weight {
        Weight::from_parts(80_000_000, 8_000)
            .saturating_add(RocksDbWeight::get().reads_writes(7, 4))
    }

    fn set_asset_whitelisted() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(RocksDbWeight::get().writes(1))
    }

    fn set_asset_deposit_paused() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(RocksDbWeight::get().writes(1))
    }

    fn set_global_deposit_paused() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(RocksDbWeight::get().writes(1))
    }
}
