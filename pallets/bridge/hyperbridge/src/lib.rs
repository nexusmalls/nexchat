// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Pallet Hyperbridge (vendored)
//!
//! **Vendor note / 来源说明**: this is a vendored copy of Polytope Labs'
//! `pallet-hyperbridge` (crate `2512.0.0`, repo `polytope-labs/hyperbridge`),
//! adapted to compile against the only available `ismp 2512.1.0` (the matching
//! `ismp 2512.0.0` was yanked and `2512.1.0` is a breaking change). Adaptations vs
//! upstream `2512.0.0` are limited to the ISMP 2512.1.0 API delta — see `README.md`.
//!
//! **来源说明**：本 crate 是 Polytope Labs `pallet-hyperbridge`（crate `2512.0.0`，
//! 仓库 `polytope-labs/hyperbridge`）的 vendor 拷贝，已改造以对唯一可用的
//! `ismp 2512.1.0` 编译（对应的 `ismp 2512.0.0` 已被 yank，`2512.1.0` 为破坏性变更）。
//! 相对上游 `2512.0.0` 的改动仅限 ISMP 2512.1.0 的 API 差异——详见 `README.md`。
//!
//! Pallet hyperbridge mediates the connection between hyperbridge and substrate-based chains. This
//! pallet provides:
//!
//!  - An [`IsmpDispatcher`] implementation which collects hyperbridge's protocol fees and commits
//!    the reciepts for these fees to child storage. Hyperbridge will only accept messages that have
//!    been paid for using this module.
//!  - An [`IsmpModule`] which recieves and processes requests from hyperbridge. These requests are
//!    dispatched by hyperbridge governance and may adjust fees or request payouts for both relayers
//!    and protocol revenue.
//!
//! This pallet contains no calls and dispatches no requests. Substrate based chains should use this
//! to dispatch requests that should be processed by hyperbridge.
//!
//! ## Usage
//!
//! This module must be configured as an [`IsmpModule`] in your
//! [`IsmpRouter`](ismp::router::IsmpRouter) implementation so that it may receive important
//! messages from hyperbridge such as paramter updates or relayer fee withdrawals.
//!
//! ```rust,ignore
//! use ismp::module::IsmpModule;
//! use ismp::router::IsmpRouter;
//!
//! #[derive(Default)]
//! struct ModuleRouter;
//!
//! impl IsmpRouter for ModuleRouter {
//!     fn module_for_id(&self, id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
//!         return match id.as_slice() {
//!             pallet_hyperbridge::PALLET_HYPERBRIDGE_ID => Ok(Box::new(pallet_hyperbridge::Pallet::<Runtime>::default())),
//!             _ => Err(Error::ModuleNotFound(id)),
//!         };
//!     }
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::{collections::BTreeMap, format};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
    sp_runtime::traits::AccountIdConversion,
    traits::{fungible::Mutate, tokens::Preservation, Get},
    weights::Weight,
};
use ismp::{
    dispatcher::{DispatchRequest, FeeMetadata, IsmpDispatcher},
    host::StateMachine,
    module::IsmpModule,
    router::PostRequest,
};
pub use pallet::*;
use pallet_ismp::RELAYER_FEE_ACCOUNT;
use primitive_types::H256;

pub mod child_trie;

/// Host params for substrate based chains
#[derive(
    Debug,
    Clone,
    Encode,
    Decode,
    DecodeWithMemTracking,
    scale_info::TypeInfo,
    PartialEq,
    Eq,
    Default,
)]
pub struct SubstrateHostParams<B> {
    /// The default per byte fee
    pub default_per_byte_fee: B,
    /// Per byte fee configured for specific chains
    pub per_byte_fees: BTreeMap<StateMachine, B>,
    /// Asset registration fee
    pub asset_registration_fee: B,
}

/// Parameters that govern the working operations of this module. Versioned for ease of migration.
#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub enum VersionedHostParams<Balance> {
    /// The per-byte fee that hyperbridge charges for outgoing requests and responses.
    V1(SubstrateHostParams<Balance>),
}

impl<Balance: Default> Default for VersionedHostParams<Balance> {
    fn default() -> Self {
        VersionedHostParams::V1(Default::default())
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, PalletId};

    /// [`IsmpModule`] module identifier for incoming requests from hyperbridge
    pub const PALLET_HYPERBRIDGE_ID: &'static [u8] = b"HYPR-FEE";

    /// [`PalletId`] where protocol fees will be collected
    pub const PALLET_HYPERBRIDGE: PalletId = PalletId(*b"HYPR-FEE");

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_ismp::Config {
        /// The underlying [`IsmpHost`] implementation
        type IsmpHost: IsmpDispatcher<Account = Self::AccountId, Balance = Self::Balance> + Default;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    /// The host parameters of the pallet-hyperbridge.
    #[pallet::storage]
    #[pallet::getter(fn host_params)]
    pub type HostParams<T> =
        StorageValue<_, VersionedHostParams<<T as pallet_ismp::Config>::Balance>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Hyperbridge governance has now updated it's host params on this chain.
        HostParamsUpdated {
            /// The old host params
            old: VersionedHostParams<<T as pallet_ismp::Config>::Balance>,
            /// The new host params
            new: VersionedHostParams<<T as pallet_ismp::Config>::Balance>,
        },
        /// A relayer has withdrawn some fees
        RelayerFeeWithdrawn {
            /// The amount that was withdrawn
            amount: <T as pallet_ismp::Config>::Balance,
            /// The withdrawal beneficiary
            account: T::AccountId,
        },
    }

    // Errors encountered by pallet-hyperbridge
    #[pallet::error]
    pub enum Error<T> {}

    // Hack for implementing the [`Default`] bound needed for
    // [`IsmpDispatcher`](ismp::dispatcher::IsmpDispatcher) and
    // [`IsmpModule`](ismp::module::IsmpModule)
    impl<T> Default for Pallet<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }
}

/// [`IsmpDispatcher`] implementation for dispatching requests to the hyperbridge coprocessor.
/// Charges the hyperbridge protocol fee on a per-byte basis.
///
/// **NOTE** Hyperbridge WILL NOT accept requests that were not dispatched through this
/// implementation.
impl<T> IsmpDispatcher for Pallet<T>
where
    T: Config,
    T::Balance: Into<u128> + From<u128>,
{
    type Account = T::AccountId;
    type Balance = T::Balance;

    fn dispatch_request(
        &self,
        request: DispatchRequest,
        fee: FeeMetadata<Self::Account, Self::Balance>,
    ) -> Result<H256, anyhow::Error> {
        let fees = match request {
            DispatchRequest::Post(ref post) => {
                let VersionedHostParams::V1(params) = Self::host_params();
                let per_byte_fee: u128 = (*params
                    .per_byte_fees
                    .get(&post.dest)
                    .unwrap_or(&params.default_per_byte_fee))
                .into();
                // minimum fee is 32 bytes
                let fees = if post.body.len() < 32 {
                    per_byte_fee * 32u128
                } else {
                    per_byte_fee * post.body.len() as u128
                };

                // collect protocol fees
                if fees != 0 {
                    T::Currency::transfer(
                        &fee.payer,
                        &RELAYER_FEE_ACCOUNT.into_account_truncating(),
                        fees.into(),
                        Preservation::Expendable,
                    )
                    .map_err(|err| {
                        ismp::Error::Custom(format!("Error withdrawing request fees: {err:?}"))
                    })?;
                }

                fees
            }
            DispatchRequest::Get(_) => Default::default(),
        };

        let host = <T as Config>::IsmpHost::default();
        let commitment = host.dispatch_request(request, fee)?;

        // commit the fee collected to child-trie
        child_trie::RequestPayments::insert(commitment, fees);

        Ok(commitment)
    }

    // NOTE (Nexus vendor): upstream `2512.0.0` also implemented `dispatch_response` here, but
    // `ismp 2512.1.0` removed `IsmpDispatcher::dispatch_response` (and the `PostResponse` type).
    // Response-fee collection is therefore no longer part of this trait and has been dropped.
    // NOTE（Nexus vendor）：上游 `2512.0.0` 这里还实现了 `dispatch_response`，但 `ismp 2512.1.0`
    // 已移除 `IsmpDispatcher::dispatch_response`（及 `PostResponse` 类型），故响应费用收取不再属于
    // 该 trait，已删除。
}

/// A request to withdraw some funds. Could either be for protocol revenue or relayer fees.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct WithdrawalRequest<Account, Amount> {
    /// The amount to be withdrawn
    pub amount: Amount,
    /// The withdrawal beneficiary
    pub account: Account,
}

/// Cross-chain messages to this module. This module will only accept messages from the hyperbridge
/// chain. Assumed to be configured in [`pallet_ismp::Config`]
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum Message<Account, Balance> {
    /// Set some new host params
    #[codec(index = 0)]
    UpdateHostParams(VersionedHostParams<Balance>),
    /// Withdraw the fees owed to a relayer
    #[codec(index = 2)]
    WithdrawRelayerFees(WithdrawalRequest<Account, Balance>),
}

impl<T> IsmpModule for Pallet<T>
where
    T: Config,
    T::Balance: Into<u128> + From<u128>,
{
    fn on_accept(&self, request: PostRequest) -> Result<Weight, anyhow::Error> {
        // this of course assumes that hyperbridge is configured as the coprocessor.
        let source = request.source;
        if Some(source) != T::Coprocessor::get() {
            Err(ismp::Error::Custom(format!(
                "Invalid request source: {source}"
            )))?
        }

        let message =
            Message::<T::AccountId, T::Balance>::decode(&mut &request.body[..]).map_err(|err| {
                ismp::Error::Custom(format!("Failed to decode per-byte fee: {err:?}"))
            })?;

        let weight = match message {
            Message::UpdateHostParams(new) => {
                let old = HostParams::<T>::get();
                HostParams::<T>::put(new.clone());
                Self::deposit_event(Event::<T>::HostParamsUpdated { old, new });
                T::DbWeight::get().reads_writes(0, 0)
            }
            Message::WithdrawRelayerFees(WithdrawalRequest { account, amount }) => {
                T::Currency::transfer(
                    &RELAYER_FEE_ACCOUNT.into_account_truncating(),
                    &account,
                    amount,
                    Preservation::Expendable,
                )
                .map_err(|err| {
                    ismp::Error::Custom(format!("Error withdrawing protocol fees: {err:?}"))
                })?;

                Self::deposit_event(Event::<T>::RelayerFeeWithdrawn { account, amount });
                T::DbWeight::get().reads_writes(0, 0)
            }
        };

        Ok(weight)
    }

    // NOTE (Nexus vendor): this module handles neither responses nor timeouts. Upstream `2512.0.0`
    // overrode `on_response`/`on_timeout` to return `CannotHandleMessage`; in `ismp 2512.1.0` those
    // trait methods carry that exact default (and their argument types changed from
    // `Response`/`Timeout` to `GetResponse`/`Request`), so the overrides are dropped.
    // NOTE（Nexus vendor）：本模块既不处理响应也不处理超时。上游 `2512.0.0` 覆写
    // `on_response`/`on_timeout` 返回 `CannotHandleMessage`；在 `ismp 2512.1.0` 中这两个 trait 方法
    // 的默认实现正是如此（且入参类型由 `Response`/`Timeout` 改为 `GetResponse`/`Request`），故删除覆写。
}
