// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0

//! # Hyper Fungible Token Pallet
//!
//! Cross-chain fungible-token transfers using the official Hyperbridge wire
//! protocol. This Nexus fork preserves the upstream ABI and storage layout
//! while making outbound dispatch and registry governance atomic.
//!
//! # Hyper Fungible Token Pallet
//!
//! 使用官方 Hyperbridge wire protocol 执行跨链同质化 token 转移。本 Nexus fork
//! 保留上游 ABI 与 storage layout，仅为出站 dispatch 和 registry 治理增加原子性。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod error;
pub mod impls;
pub mod module;
pub mod types;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use weights::WeightInfo;

use crate::impls::convert_to_erc20;
use alloy_sol_types::SolValue;
use frame_support::{
    traits::fungibles::{self, Inspect, Mutate},
    PalletId,
};
use pallet_ismp::ModuleId;
use polkadot_sdk::*;
use primitive_types::H256;
use types::{AssetId, EvmToSubstrate, Message, SendParams, TokenRegistration, TokenUpdate};

use alloc::{collections::BTreeSet, vec, vec::Vec};

pub use pallet::*;

/// Official well-known HFT module ID.
/// 官方 HFT well-known module ID。
pub const PALLET_ID: ModuleId = ModuleId::Pallet(PalletId(*b"pall_hft"));

const ETHEREUM_MESSAGE_PREFIX: &str = "\x19Ethereum Signed Message:\n";

/// Creates and configures the benchmark asset without weakening production origins.
/// 在不削弱生产 origin 的前提下创建并配置 benchmark 资产。
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<T: Config> {
    fn create_asset(asset_id: AssetId<T>, owner: T::AccountId);
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        pallet_prelude::*,
        traits::{
            tokens::{Fortitude, Precision, Preservation},
            Currency, ExistenceRequirement,
        },
    };
    use frame_system::pallet_prelude::*;
    use ismp::{
        dispatcher::{DispatchPost, DispatchRequest, FeeMetadata, IsmpDispatcher},
        host::StateMachine,
    };

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: polkadot_sdk::frame_system::Config + pallet_ismp::Config {
        /// ISMP dispatcher used for outbound requests.
        /// 用于出站请求的 ISMP dispatcher。
        type Dispatcher: IsmpDispatcher<Account = Self::AccountId, Balance = Self::Balance>;

        /// Native currency backend.
        /// 原生货币 backend。
        type NativeCurrency: Currency<Self::AccountId>;

        /// Origin authorized to manage token registries.
        /// 被授权管理 token registry 的 origin。
        type CreateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Fungible asset backend.
        /// 同质化资产 backend。
        type Assets: fungibles::Mutate<Self::AccountId>
            + fungibles::metadata::Inspect<Self::AccountId>;

        type NativeAssetId: Get<AssetId<Self>>;

        #[pallet::constant]
        type Decimals: Get<u8>;

        type EvmToSubstrate: EvmToSubstrate<Self>;

        type WeightInfo: WeightInfo;

        /// Asset setup used only by runtime benchmarks.
        /// 仅供 runtime benchmark 使用的资产初始化器。
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: BenchmarkHelper<Self>;
    }

    #[pallet::storage]
    pub type TokenContracts<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        StateMachine,
        Blake2_128Concat,
        AssetId<T>,
        Vec<u8>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type ContractToAsset<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        StateMachine,
        Blake2_128Concat,
        Vec<u8>,
        AssetId<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type NativeAssets<T: Config> =
        StorageMap<_, Blake2_128Concat, AssetId<T>, bool, ValueQuery>;

    #[pallet::storage]
    pub type Precisions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        AssetId<T>,
        Blake2_128Concat,
        StateMachine,
        u8,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        TokenSent {
            from: T::AccountId,
            to: BoundedVec<u8, sp_core::ConstU32<32>>,
            amount: <T::NativeCurrency as Currency<T::AccountId>>::Balance,
            dest: StateMachine,
            commitment: H256,
        },
        TokenReceived {
            beneficiary: T::AccountId,
            amount: <<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance,
            source: StateMachine,
        },
        TokenRefunded {
            beneficiary: T::AccountId,
            amount: <<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance,
            dest: StateMachine,
        },
        TokenRegistered {
            asset_id: AssetId<T>,
            native: bool,
            chains: Vec<StateMachine>,
        },
        TokenUpdated {
            asset_id: AssetId<T>,
            added: Vec<StateMachine>,
            removed: Vec<StateMachine>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        UnregisteredAsset,
        TokenContractNotFound,
        PalletAddressNotFound,
        DecimalsNotFound,
        AssetTransferError,
        DispatchError,
        NonEvmPeerChain,
        ErcDecimalsBelowLocal,
        AssetDoesNotExist,
        TokenAlreadyRegistered,
        EmptyRegistration,
        ZeroContractAddress,
        ContractAlreadyInUse,
        RegistryInconsistent,
        DuplicateChainUpdate,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        <T as frame_system::Config>::AccountId: From<[u8; 32]>,
        u128: From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance>,
        <T as pallet_ismp::Config>::Balance:
            From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance>,
        <<T as Config>::Assets as fungibles::Inspect<T::AccountId>>::Balance:
            From<<<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance>,
        <<T as Config>::Assets as fungibles::Inspect<T::AccountId>>::Balance: From<u128>,
        [u8; 32]: From<<T as frame_system::Config>::AccountId>,
    {
        /// Atomically locks/burns assets and dispatches an HFT request.
        /// 原子地锁定/销毁资产并派发 HFT 请求。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::send())]
        #[frame_support::transactional]
        pub fn send(
            origin: OriginFor<T>,
            params: SendParams<
                AssetId<T>,
                <<T as Config>::NativeCurrency as Currency<T::AccountId>>::Balance,
            >,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let dispatcher = <T as Config>::Dispatcher::default();
            let token_contract =
                TokenContracts::<T>::get(params.destination, params.asset_id.clone())
                    .ok_or(Error::<T>::TokenContractNotFound)?;
            let erc_decimals = Precisions::<T>::get(params.asset_id.clone(), params.destination)
                .ok_or(Error::<T>::DecimalsNotFound)?;

            let decimals = if params.asset_id == T::NativeAssetId::get() {
                <T as Config>::NativeCurrency::transfer(
                    &who,
                    &Self::pallet_account(),
                    params.amount,
                    ExistenceRequirement::AllowDeath,
                )?;
                T::Decimals::get()
            } else {
                if NativeAssets::<T>::get(params.asset_id.clone()) {
                    <T as Config>::Assets::transfer(
                        params.asset_id.clone(),
                        &who,
                        &Self::pallet_account(),
                        params.amount.into(),
                        Preservation::Expendable,
                    )?;
                } else {
                    <T as Config>::Assets::burn_from(
                        params.asset_id.clone(),
                        &who,
                        params.amount.into(),
                        Preservation::Expendable,
                        Precision::Exact,
                        Fortitude::Polite,
                    )?;
                }
                <T::Assets as fungibles::metadata::Inspect<T::AccountId>>::decimals(
                    params.asset_id.clone(),
                )
            };

            let sender: [u8; 32] = who.clone().into();
            let amount: u128 = params.amount.into();
            let token_message = Message {
                from: sender.to_vec().into(),
                to: params.recipient.to_vec().into(),
                amount: alloy_primitives::U256::from_be_bytes(
                    convert_to_erc20(amount, erc_decimals, decimals).to_big_endian(),
                ),
                data: params.call_data.unwrap_or_default().into_inner().into(),
            };
            let dispatch_post = DispatchPost {
                dest: params.destination,
                from: PALLET_ID.to_bytes(),
                to: token_contract,
                timeout: params.timeout,
                body: Message::abi_encode(&token_message),
            };
            let metadata = FeeMetadata {
                payer: who.clone(),
                fee: params.relayer_fee.into(),
            };
            let commitment = dispatcher
                .dispatch_request(DispatchRequest::Post(dispatch_post), metadata)
                .map_err(|_| Error::<T>::DispatchError)?;

            Self::deposit_event(Event::<T>::TokenSent {
                from: who,
                to: params.recipient,
                dest: params.destination,
                amount: params.amount,
                commitment,
            });
            Ok(())
        }

        /// Atomically registers a fully validated token configuration.
        /// 原子注册经过完整校验的 token 配置。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::register_token(registration.chains.len() as u32))]
        pub fn register_token(
            origin: OriginFor<T>,
            registration: TokenRegistration<AssetId<T>>,
        ) -> DispatchResult {
            T::CreateOrigin::ensure_origin(origin)?;
            Self::do_register_token(registration)
        }

        /// Atomically updates a fully validated token configuration.
        /// 原子更新经过完整校验的 token 配置。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::update_token(
			update.add_chains.len() as u32,
			update.remove_chains.len() as u32,
		))]
        pub fn update_token(
            origin: OriginFor<T>,
            update: TokenUpdate<AssetId<T>>,
        ) -> DispatchResult {
            T::CreateOrigin::ensure_origin(origin)?;
            Self::do_update_token(update)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            Self::check_registry_invariants().map_err(Into::into)
        }
    }

    impl<T: Config> Pallet<T> {
        /// Registers a token after the caller has enforced the governance origin.
        /// 在调用方已校验治理 origin 后注册 token。
        #[frame_support::transactional]
        pub(crate) fn do_register_token(
            registration: TokenRegistration<AssetId<T>>,
        ) -> DispatchResult {
            ensure!(
                !registration.chains.is_empty(),
                Error::<T>::EmptyRegistration
            );
            ensure!(
                !NativeAssets::<T>::contains_key(registration.local_id.clone()),
                Error::<T>::TokenAlreadyRegistered
            );
            let local_decimals = Self::local_decimals(&registration.local_id)?;

            for (chain, config) in &registration.chains {
                Self::validate_chain_config(
                    &registration.local_id,
                    *chain,
                    config,
                    local_decimals,
                )?;
            }

            let chains: Vec<StateMachine> = registration.chains.keys().copied().collect();
            NativeAssets::<T>::insert(registration.local_id.clone(), registration.native);
            for (chain, config) in registration.chains {
                Self::write_chain_config(&registration.local_id, chain, config);
            }
            Self::deposit_event(Event::<T>::TokenRegistered {
                asset_id: registration.local_id,
                native: registration.native,
                chains,
            });
            Ok(())
        }

        /// Updates a token after the caller has enforced the governance origin.
        /// 在调用方已校验治理 origin 后更新 token。
        #[frame_support::transactional]
        pub(crate) fn do_update_token(update: TokenUpdate<AssetId<T>>) -> DispatchResult {
            ensure!(
                NativeAssets::<T>::contains_key(update.asset_id.clone()),
                Error::<T>::UnregisteredAsset
            );
            let remove_set: BTreeSet<StateMachine> = update.remove_chains.iter().copied().collect();
            ensure!(
                remove_set.len() == update.remove_chains.len(),
                Error::<T>::DuplicateChainUpdate
            );
            ensure!(
                update
                    .add_chains
                    .keys()
                    .all(|chain| !remove_set.contains(chain)),
                Error::<T>::DuplicateChainUpdate
            );

            let local_decimals = Self::local_decimals(&update.asset_id)?;
            for (chain, config) in &update.add_chains {
                Self::validate_existing_forward(&update.asset_id, *chain)?;
                Self::validate_chain_config(&update.asset_id, *chain, config, local_decimals)?;
            }
            for chain in &update.remove_chains {
                ensure!(chain.is_evm(), Error::<T>::NonEvmPeerChain);
                Self::validate_existing_forward(&update.asset_id, *chain)?;
            }

            let added: Vec<StateMachine> = update.add_chains.keys().copied().collect();
            let removed = update.remove_chains.clone();
            for (chain, config) in update.add_chains {
                Self::remove_chain_config(&update.asset_id, chain)?;
                Self::write_chain_config(&update.asset_id, chain, config);
            }
            for chain in update.remove_chains {
                Self::remove_chain_config(&update.asset_id, chain)?;
            }
            Self::deposit_event(Event::<T>::TokenUpdated {
                asset_id: update.asset_id,
                added,
                removed: removed.into_inner(),
            });
            Ok(())
        }

        /// Checks the one-to-one HFT registry and precision invariants.
        /// 检查 HFT registry 一一对应关系与精度不变量。
        pub fn check_registry_invariants() -> Result<(), &'static str> {
            for (chain, asset_id, contract) in TokenContracts::<T>::iter() {
                if !chain.is_evm() || contract.len() != 20 || contract.iter().all(|byte| *byte == 0)
                {
                    return Err("HFT forward registry contains an invalid EVM contract");
                }
                if ContractToAsset::<T>::get(chain, contract.clone()) != Some(asset_id.clone()) {
                    return Err("HFT forward registry has no matching reverse entry");
                }
                if !Precisions::<T>::contains_key(asset_id, chain) {
                    return Err("HFT forward registry has no precision entry");
                }
            }
            for (chain, contract, asset_id) in ContractToAsset::<T>::iter() {
                if TokenContracts::<T>::get(chain, asset_id) != Some(contract) {
                    return Err("HFT reverse registry has no matching forward entry");
                }
            }
            for (asset_id, chain, _) in Precisions::<T>::iter() {
                if !TokenContracts::<T>::contains_key(chain, asset_id) {
                    return Err("HFT precision has no matching forward entry");
                }
            }
            Ok(())
        }

        fn local_decimals(asset_id: &AssetId<T>) -> Result<u8, DispatchError> {
            if *asset_id == T::NativeAssetId::get() {
                return Ok(T::Decimals::get());
            }
            ensure!(
                <T::Assets as Inspect<T::AccountId>>::asset_exists(asset_id.clone()),
                Error::<T>::AssetDoesNotExist
            );
            Ok(
                <T::Assets as fungibles::metadata::Inspect<T::AccountId>>::decimals(
                    asset_id.clone(),
                ),
            )
        }

        fn validate_chain_config(
            asset_id: &AssetId<T>,
            chain: StateMachine,
            config: &crate::types::ChainConfig,
            local_decimals: u8,
        ) -> DispatchResult {
            ensure!(chain.is_evm(), Error::<T>::NonEvmPeerChain);
            ensure!(
                config.token_contract != sp_core::H160::zero(),
                Error::<T>::ZeroContractAddress
            );
            ensure!(
                config.decimals >= local_decimals,
                Error::<T>::ErcDecimalsBelowLocal
            );
            let contract = config.token_contract.0.to_vec();
            if let Some(existing) = ContractToAsset::<T>::get(chain, contract) {
                ensure!(existing == *asset_id, Error::<T>::ContractAlreadyInUse);
            }
            Ok(())
        }

        fn validate_existing_forward(asset_id: &AssetId<T>, chain: StateMachine) -> DispatchResult {
            if let Some(contract) = TokenContracts::<T>::get(chain, asset_id.clone()) {
                ensure!(
                    ContractToAsset::<T>::get(chain, contract) == Some(asset_id.clone()),
                    Error::<T>::RegistryInconsistent
                );
            }
            Ok(())
        }

        fn write_chain_config(
            asset_id: &AssetId<T>,
            chain: StateMachine,
            config: crate::types::ChainConfig,
        ) {
            let contract = config.token_contract.0.to_vec();
            TokenContracts::<T>::insert(chain, asset_id.clone(), contract.clone());
            ContractToAsset::<T>::insert(chain, contract, asset_id.clone());
            Precisions::<T>::insert(asset_id.clone(), chain, config.decimals);
        }

        fn remove_chain_config(asset_id: &AssetId<T>, chain: StateMachine) -> DispatchResult {
            if let Some(contract) = TokenContracts::<T>::get(chain, asset_id.clone()) {
                ensure!(
                    ContractToAsset::<T>::get(chain, contract.clone()) == Some(asset_id.clone()),
                    Error::<T>::RegistryInconsistent
                );
                ContractToAsset::<T>::remove(chain, contract);
            }
            TokenContracts::<T>::remove(chain, asset_id.clone());
            Precisions::<T>::remove(asset_id.clone(), chain);
            Ok(())
        }
    }

    impl<T> Default for Pallet<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }
}
