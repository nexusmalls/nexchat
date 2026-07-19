// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! FRAME v2 benchmarks for the bounded Nexus HFT fork.
//! Nexus 有界 HFT fork 的 FRAME v2 基准测试。

use super::*;
use crate::types::{ChainConfig, MaxCallDataLen, MaxChainsPerCall};
use frame_benchmarking::v2::*;
use frame_support::{
    traits::{
        fungibles::{Inspect, Mutate},
        Currency, Get,
    },
    BoundedBTreeMap, BoundedVec,
};
use ismp::host::StateMachine;
use sp_core::H160;
use sp_runtime::traits::Zero;

fn chain(index: u32) -> StateMachine {
    StateMachine::Evm(10_000 + index)
}

fn config(index: u32, decimals: u8) -> ChainConfig {
    let mut bytes = [0u8; 20];
    bytes[16..].copy_from_slice(&index.saturating_add(1).to_be_bytes());
    ChainConfig {
        token_contract: H160(bytes),
        decimals,
    }
}

#[benchmarks(
    where
        T::AccountId: From<[u8; 32]>,
        [u8; 32]: From<T::AccountId>,
        AssetId<T>: From<u32>,
        <<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance: From<u128>,
        u128: From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance>,
        <T as pallet_ismp::Config>::Balance:
            From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance>,
        <<T as Config>::Assets as Inspect<T::AccountId>>::Balance:
            From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance> + From<u128>,
)]
mod benches {
    use super::*;

    #[benchmark]
    fn send() {
        let caller: T::AccountId = whitelisted_caller();
        let asset_id: AssetId<T> = 900_001u32.into();
        let destination = chain(0);
        T::BenchmarkHelper::create_asset(asset_id.clone(), caller.clone());
        T::Assets::mint_into(asset_id.clone(), &caller, 1_000u128.into())
            .expect("benchmark asset must accept minting");
        T::NativeCurrency::make_free_balance_be(&caller, 1_000_000_000_000u128.into());
        NativeAssets::<T>::insert(asset_id.clone(), false);
        TokenContracts::<T>::insert(
            destination,
            asset_id.clone(),
            config(0, 6).token_contract.0.to_vec(),
        );
        Precisions::<T>::insert(asset_id.clone(), destination, 6);
        let call_data: BoundedVec<u8, MaxCallDataLen> =
            BoundedVec::try_from(vec![0xCD; <MaxCallDataLen as Get<u32>>::get() as usize])
                .expect("benchmark payload is exactly bounded");
        let params = SendParams {
            asset_id: asset_id.clone(),
            destination,
            recipient: BoundedVec::truncate_from(vec![0xEF; 32]),
            amount: 1_000u128.into(),
            timeout: 300,
            relayer_fee: 1_000_000_000u128.into(),
            call_data: Some(call_data),
        };

        #[extrinsic_call]
        _(frame_system::RawOrigin::Signed(caller.clone()), params);

        assert_eq!(T::Assets::balance(asset_id, &caller), Zero::zero());
    }

    #[benchmark]
    fn register_token(c: Linear<1, 16>) {
        let caller: T::AccountId = whitelisted_caller();
        let asset_id: AssetId<T> = 900_002u32.into();
        T::BenchmarkHelper::create_asset(asset_id.clone(), caller);
        let mut chains: BoundedBTreeMap<_, _, MaxChainsPerCall> = BoundedBTreeMap::new();
        for i in 0..c {
            chains
                .try_insert(chain(i), config(i, 6))
                .expect("c is bounded");
        }
        let registration = TokenRegistration {
            local_id: asset_id.clone(),
            native: false,
            chains,
        };

        #[extrinsic_call]
        _(frame_system::RawOrigin::Root, registration);

        assert!(NativeAssets::<T>::contains_key(asset_id.clone()));
        assert!(TokenContracts::<T>::contains_key(chain(0), asset_id));
    }

    #[benchmark]
    fn update_token(a: Linear<0, 16>, r: Linear<0, 16>) {
        let caller: T::AccountId = whitelisted_caller();
        let asset_id: AssetId<T> = 900_003u32.into();
        T::BenchmarkHelper::create_asset(asset_id.clone(), caller);
        NativeAssets::<T>::insert(asset_id.clone(), false);

        let mut add_chains: BoundedBTreeMap<_, _, MaxChainsPerCall> = BoundedBTreeMap::new();
        for i in 0..a {
            let state_machine = chain(i);
            let old = config(i, 6);
            let contract = old.token_contract.0.to_vec();
            TokenContracts::<T>::insert(state_machine, asset_id.clone(), contract.clone());
            ContractToAsset::<T>::insert(state_machine, contract, asset_id.clone());
            Precisions::<T>::insert(asset_id.clone(), state_machine, 6);
            add_chains
                .try_insert(state_machine, config(i.saturating_add(1_000), 6))
                .expect("add is bounded");
        }

        let mut remove_chains: BoundedVec<_, MaxChainsPerCall> = BoundedVec::new();
        for i in 0..r {
            let state_machine = chain(i.saturating_add(100));
            let old = config(i.saturating_add(100), 6);
            let contract = old.token_contract.0.to_vec();
            TokenContracts::<T>::insert(state_machine, asset_id.clone(), contract.clone());
            ContractToAsset::<T>::insert(state_machine, contract, asset_id.clone());
            Precisions::<T>::insert(asset_id.clone(), state_machine, 6);
            remove_chains
                .try_push(state_machine)
                .expect("remove is bounded");
        }
        let update = TokenUpdate {
            asset_id: asset_id.clone(),
            add_chains,
            remove_chains,
        };

        #[extrinsic_call]
        _(frame_system::RawOrigin::Root, update);

        if a > 0 {
            assert!(TokenContracts::<T>::contains_key(
                chain(0),
                asset_id.clone()
            ));
        }
        if r > 0 {
            assert!(!TokenContracts::<T>::contains_key(chain(100), asset_id));
        }
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
