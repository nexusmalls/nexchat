// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Runtime benchmarks for the collateral mirror boundary.
//! 抵押镜像边界的 runtime benchmark。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::fungibles::{Create, Mutate};
use frame_system::RawOrigin;
use pallet_prediction_control::PredictionMode;

const BENCHMARK_ASSET_ID: u64 = 900_001;
const BENCHMARK_AMOUNT: u128 = 1_000_000;

fn prepare_asset<T: Config>(caller: &T::AccountId) {
    T::Assets::create(BENCHMARK_ASSET_ID, caller.clone(), true, 1)
        .expect("benchmark asset creation must succeed");
    T::Assets::mint_into(
        BENCHMARK_ASSET_ID,
        caller,
        BENCHMARK_AMOUNT.saturating_mul(2),
    )
    .expect("benchmark asset mint must succeed");
}

fn enable_deposits<T: Config + pallet_prediction_control::Config>() {
    pallet_prediction_control::GlobalMode::<T>::put(PredictionMode::Full);
    WhitelistedAssets::<T>::insert(BENCHMARK_ASSET_ID, true);
}

#[benchmarks(
    where
        T: pallet_prediction_control::Config,
)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn deposit() {
        let caller: T::AccountId = whitelisted_caller();
        prepare_asset::<T>(&caller);
        enable_deposits::<T>();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            BENCHMARK_ASSET_ID,
            BENCHMARK_AMOUNT,
        );

        assert_eq!(
            Pallet::<T>::mirror_issuance(BENCHMARK_ASSET_ID),
            BENCHMARK_AMOUNT
        );
        assert!(Pallet::<T>::is_mirror_consistent(BENCHMARK_ASSET_ID));
    }

    #[benchmark]
    fn withdraw() {
        let caller: T::AccountId = whitelisted_caller();
        prepare_asset::<T>(&caller);
        enable_deposits::<T>();
        Pallet::<T>::deposit(
            RawOrigin::Signed(caller.clone()).into(),
            BENCHMARK_ASSET_ID,
            BENCHMARK_AMOUNT,
        )
        .expect("benchmark mirror deposit must succeed");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            BENCHMARK_ASSET_ID,
            BENCHMARK_AMOUNT,
        );

        assert_eq!(Pallet::<T>::mirror_issuance(BENCHMARK_ASSET_ID), 0);
        assert!(Pallet::<T>::is_mirror_consistent(BENCHMARK_ASSET_ID));
    }

    #[benchmark]
    fn set_asset_whitelisted() {
        let caller: T::AccountId = whitelisted_caller();
        prepare_asset::<T>(&caller);

        #[extrinsic_call]
        _(RawOrigin::Root, BENCHMARK_ASSET_ID, true);

        assert_eq!(WhitelistedAssets::<T>::get(BENCHMARK_ASSET_ID), Some(true));
    }

    #[benchmark]
    fn set_asset_deposit_paused() {
        #[extrinsic_call]
        _(RawOrigin::Root, BENCHMARK_ASSET_ID, true);

        assert!(AssetDepositPaused::<T>::get(BENCHMARK_ASSET_ID));
    }

    #[benchmark]
    fn set_global_deposit_paused() {
        #[extrinsic_call]
        _(RawOrigin::Root, true);

        assert!(GlobalDepositPaused::<T>::get());
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
