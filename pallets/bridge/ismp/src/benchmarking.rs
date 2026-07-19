// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Benchmarking for `pallet-bridge-ismp` (frame_benchmarking::v2 style).
//!
//! `bridge_out` is exercised end-to-end (guardrails → real burn → ledger →
//! ISMP dispatch); governance extrinsics are benchmarked in isolation. The
//! `impl_benchmark_test_suite!` runs them against the mock (whose `Dispatcher`
//! returns a deterministic commitment) so the bodies stay valid under
//! `--features runtime-benchmarks`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::types::{BridgeLimits, ChainConfig};
use frame_benchmarking::v2::*;
use frame_support::traits::{Currency, Get};
use frame_system::RawOrigin;
use ismp::host::StateMachine;
use sp_core::H160;
use sp_runtime::traits::{Bounded, Saturating, Zero};

fn evm_chain() -> StateMachine {
    StateMachine::Evm(56)
}

#[benchmarks(
	where
		T::AccountId: Into<[u8; 32]>,
		BalanceOf<T>: Into<u128>,
		<T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
)]
mod benches {
    use super::*;

    #[benchmark]
    fn bridge_out() {
        let caller: T::AccountId = whitelisted_caller();
        let funding = BalanceOf::<T>::max_value() / 2u32.into();
        let _ = T::NativeCurrency::deposit_creating(&caller, funding);

        Chains::<T>::insert(
            evm_chain(),
            ChainConfig {
                contract: H160::repeat_byte(0xCC),
                erc_decimals: 18,
            },
        );
        let cap = BalanceOf::<T>::max_value() / 4u32.into();
        Limits::<T>::put(BridgeLimits {
            per_tx: cap,
            daily: cap,
        });

        let amount = T::MinBridgeAmount::get();
        let recipient = H160::repeat_byte(0xEE);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            evm_chain(),
            recipient,
            amount,
            Zero::zero(),
        );

        assert_eq!(BridgedOut::<T>::get(), amount);
        assert_eq!(BridgedOutByChain::<T>::get(evm_chain()), amount);
    }

    #[benchmark]
    fn set_paused() {
        #[extrinsic_call]
        _(RawOrigin::Root, Some(evm_chain()), true);

        assert!(PausedChain::<T>::get(evm_chain()));
    }

    #[benchmark]
    fn set_limits() {
        let cap = BalanceOf::<T>::max_value() / 4u32.into();

        #[extrinsic_call]
        _(RawOrigin::Root, cap, cap);

        assert_eq!(Limits::<T>::get().per_tx, cap);
    }

    #[benchmark]
    fn register_chain() {
        #[extrinsic_call]
        _(RawOrigin::Root, evm_chain(), H160::repeat_byte(0xCC), 18u8);

        assert!(Chains::<T>::get(evm_chain()).is_some());
    }

    #[benchmark]
    fn deregister_chain() {
        Chains::<T>::insert(
            evm_chain(),
            ChainConfig {
                contract: H160::repeat_byte(0xCC),
                erc_decimals: 18,
            },
        );

        #[extrinsic_call]
        _(RawOrigin::Root, evm_chain());

        assert!(Chains::<T>::get(evm_chain()).is_none());
    }

    #[benchmark]
    fn prune_payout_refunds(n: Linear<0, 100>) {
        use frame_support::BoundedVec;
        let caller: T::AccountId = whitelisted_caller();
        let meta: BoundedVec<u8, T::MaxPayoutMeta> = BoundedVec::default();
        // Seed `n` entries at block 0; advancing past the TTL makes them all stale.
        for i in 0..n {
            let mut commitment_bytes = [0u8; 32];
            commitment_bytes[24..].copy_from_slice(&(i as u64).to_be_bytes());
            let commitment = sp_core::H256::from(commitment_bytes);
            crate::pallet::PayoutRefunds::<T>::insert(
                commitment,
                (
                    frame_system::pallet_prelude::BlockNumberFor::<T>::zero(),
                    meta.clone(),
                ),
            );
        }
        frame_system::Pallet::<T>::set_block_number(
            T::PayoutRefundTtl::get().saturating_add(1u32.into()),
        );

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), n);

        assert_eq!(crate::pallet::PayoutRefunds::<T>::iter().count(), 0);
    }

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::new_test_ext(alloc::vec::Vec::new()),
        crate::mock::Test
    );
}
