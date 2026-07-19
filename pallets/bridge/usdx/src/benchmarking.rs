// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Benchmarking for `pallet-usdx`.
//! `pallet-usdx` 基准测试。

use super::*;
use crate::types::{CollateralPolicy, LaneConfig, LaneLimits, BPS_DENOMINATOR};
use frame_benchmarking::v2::*;
use frame_support::traits::{
    fungibles::{Inspect, Mutate},
    Get,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};

const BENCH_RECEIPT_ID: u64 = 900_001;
const BENCH_AMOUNT: u128 = 10_000;

fn bench_policy() -> CollateralPolicy {
    CollateralPolicy {
        mint_factor_bps: BPS_DENOMINATOR,
        mint_fee_bps: 0,
        redeem_fee_bps: 0,
    }
}

fn bench_limits<T: Config>() -> LaneLimits<BlockNumberFor<T>> {
    LaneLimits {
        min_amount: 1,
        max_per_tx: 1_000_000,
        max_per_window: 2_000_000,
        window_blocks: 100u32.into(),
        debt_ceiling: 10_000_000,
    }
}

fn seed_lane<T: Config>(enabled: bool) {
    T::BenchmarkHelper::prepare();
    let descriptor_hash = T::ReceiptValidator::descriptor_hash(BENCH_RECEIPT_ID)
        .expect("benchmark receipt must be registered");
    let evidence = T::BenchmarkHelper::evidence(BENCH_RECEIPT_ID);
    assert!(T::ReceiptValidator::validate_evidence(
        BENCH_RECEIPT_ID,
        descriptor_hash,
        &evidence
    ));
    CollateralConfigs::<T>::insert(
        BENCH_RECEIPT_ID,
        LaneConfig {
            descriptor_hash,
            activation_evidence_hash: Pallet::<T>::evidence_hash(&evidence),
            enabled,
        },
    );
    LaneEvidence::<T>::insert(BENCH_RECEIPT_ID, evidence);
    CollateralPolicies::<T>::insert(BENCH_RECEIPT_ID, bench_policy());
    CollateralLimits::<T>::insert(BENCH_RECEIPT_ID, bench_limits::<T>());
    GlobalUsdxDebtCeiling::<T>::put(10_000_000);
}

#[benchmarks]
mod benches {
    use super::*;

    #[benchmark]
    fn mint() {
        let caller: T::AccountId = whitelisted_caller();
        seed_lane::<T>(true);
        T::Assets::mint_into(BENCH_RECEIPT_ID, &caller, BENCH_AMOUNT)
            .expect("benchmark receipt asset must exist");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            BENCH_RECEIPT_ID,
            BENCH_AMOUNT,
            BENCH_AMOUNT,
        );

        assert_eq!(
            T::Assets::balance(T::UsdxAssetId::get(), &caller),
            BENCH_AMOUNT
        );
    }

    #[benchmark]
    fn redeem() {
        let caller: T::AccountId = whitelisted_caller();
        seed_lane::<T>(true);
        T::Assets::mint_into(BENCH_RECEIPT_ID, &caller, BENCH_AMOUNT)
            .expect("benchmark receipt asset must exist");
        Pallet::<T>::mint(
            RawOrigin::Signed(caller.clone()).into(),
            BENCH_RECEIPT_ID,
            BENCH_AMOUNT,
            BENCH_AMOUNT,
        )
        .expect("benchmark mint setup must succeed");

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            BENCH_RECEIPT_ID,
            BENCH_AMOUNT,
            BENCH_AMOUNT,
        );

        assert_eq!(T::Assets::balance(T::UsdxAssetId::get(), &caller), 0);
    }

    #[benchmark]
    fn register_collateral() {
        T::BenchmarkHelper::prepare();
        let evidence = T::BenchmarkHelper::evidence(BENCH_RECEIPT_ID);

        #[extrinsic_call]
        _(
            RawOrigin::Root,
            BENCH_RECEIPT_ID,
            evidence,
            bench_policy(),
            bench_limits::<T>(),
        );

        assert!(CollateralConfigs::<T>::contains_key(BENCH_RECEIPT_ID));
    }

    #[benchmark]
    fn set_enabled() {
        seed_lane::<T>(false);

        #[extrinsic_call]
        _(RawOrigin::Root, BENCH_RECEIPT_ID, true);

        assert!(
            CollateralConfigs::<T>::get(BENCH_RECEIPT_ID)
                .expect("seeded lane exists")
                .enabled
        );
    }

    #[benchmark]
    fn set_global_paused() {
        #[extrinsic_call]
        _(RawOrigin::Root, true);

        assert!(GlobalPaused::<T>::get());
    }

    #[benchmark]
    fn set_collateral_paused() {
        seed_lane::<T>(false);

        #[extrinsic_call]
        _(RawOrigin::Root, BENCH_RECEIPT_ID, true);

        assert!(CollateralPaused::<T>::get(BENCH_RECEIPT_ID));
    }

    #[benchmark]
    fn set_limits() {
        seed_lane::<T>(false);
        let limits = bench_limits::<T>();

        #[extrinsic_call]
        _(RawOrigin::Root, BENCH_RECEIPT_ID, limits);

        assert!(MintWindow::<T>::contains_key(BENCH_RECEIPT_ID));
        assert!(RedeemWindow::<T>::contains_key(BENCH_RECEIPT_ID));
    }

    #[benchmark]
    fn set_policy() {
        seed_lane::<T>(false);
        let policy = CollateralPolicy {
            mint_factor_bps: 9_900,
            mint_fee_bps: 10,
            redeem_fee_bps: 20,
        };

        #[extrinsic_call]
        _(RawOrigin::Root, BENCH_RECEIPT_ID, policy);

        assert_eq!(CollateralPolicies::<T>::get(BENCH_RECEIPT_ID), Some(policy));
    }

    #[benchmark]
    fn set_global_debt_ceiling() {
        #[extrinsic_call]
        _(RawOrigin::Root, 20_000_000);

        assert_eq!(GlobalUsdxDebtCeiling::<T>::get(), 20_000_000);
    }

    #[benchmark]
    fn update_collateral() {
        seed_lane::<T>(false);
        let mut evidence = T::BenchmarkHelper::evidence(BENCH_RECEIPT_ID);
        evidence.config_block = 2;
        evidence.proof_bundle_hash = H256::repeat_byte(0xBB);

        #[extrinsic_call]
        _(RawOrigin::Root, BENCH_RECEIPT_ID, evidence.clone());

        assert_eq!(LaneEvidence::<T>::get(BENCH_RECEIPT_ID), Some(evidence));
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
