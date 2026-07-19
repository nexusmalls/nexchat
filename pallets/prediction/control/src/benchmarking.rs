// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Runtime benchmarks for prediction governance controls.
//! 预测治理控制的 runtime benchmark。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn set_prediction_mode() {
        let new = PredictionMode::Full;

        #[extrinsic_call]
        _(RawOrigin::Root, new);

        assert_eq!(GlobalMode::<T>::get(), new);
    }

    #[benchmark]
    fn set_module_enabled() {
        let module = PredictionModule::PredictionMarkets;

        #[extrinsic_call]
        _(RawOrigin::Root, module, true);

        assert!(ModuleEnabled::<T>::get(module));
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
