// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Phase 2 non-production weights for prediction control.
//! 预测控制 Phase 2 非生产权重。
//!
//! These conservative placeholders preserve the runtime-facing interface only.
//! Phase 7 must regenerate and review weights before production wiring.
//! 这些保守占位值仅用于稳定 runtime 接口。正式接线前必须在 Phase 7 重新生成并审核权重。

use core::marker::PhantomData;
use frame_support::{
    traits::Get,
    weights::{constants::RocksDbWeight, Weight},
};

/// Weight functions required by prediction-control governance calls.
/// Prediction-control 治理调用所需的权重函数。
pub trait WeightInfo {
    fn set_prediction_mode() -> Weight;
    fn set_module_enabled() -> Weight;
}

/// Phase 2 database-weight adapter; not benchmark-generated.
/// Phase 2 数据库权重 adapter；并非 benchmark 生成。
pub struct Phase2Weight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for Phase2Weight<T> {
    fn set_prediction_mode() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(T::DbWeight::get().reads_writes(1, 1))
    }

    fn set_module_enabled() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }
}

impl WeightInfo for () {
    fn set_prediction_mode() -> Weight {
        Weight::from_parts(10_000_000, 1_500)
            .saturating_add(RocksDbWeight::get().reads_writes(1, 1))
    }

    fn set_module_enabled() -> Weight {
        Weight::from_parts(10_000_000, 1_500).saturating_add(RocksDbWeight::get().writes(1))
    }
}
