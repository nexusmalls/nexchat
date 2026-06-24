// Copyright (C) Polytope Labs Ltd. (vendored ABI / precision / send-burn core)
// Copyright (C) Nexus contributors (native burn/mint adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! # NEX Asset Bridge (`pallet-bridge-ismp`)
//!
//! Self-built ISMP asset bridge for the **native NEX** token, implementing
//! HB-ASSET-01 under decision **D3=(c)**: vendor the audited core of Polytope
//! Labs' `pallet-hyper-fungible-token` (`send` burn branch, `on_accept`,
//! `on_timeout`, the `Message` ABI, and the precision functions) and adapt it to
//! a **burn/mint** model for the native currency, with all guardrails and the
//! in-flight ledger inlined. Unlike the upstream custody model, native NEX is
//! **really burned** (`Currency::withdraw` → `TotalIssuance↓`) on the way out and
//! **really minted** (`Currency::deposit_creating` → `TotalIssuance↑`) on the way
//! back. There is no local liquidity pool, no `NativeFungibleAdapter`, no fork,
//! and no dependency on the deprecated `pallet-token-gateway` or the unpublished
//! HFT crate.
//!
//! # NEX 资产跨链桥（`pallet-bridge-ismp`）
//!
//! 针对**原生 NEX** 的自建 ISMP 资产桥，按决策 **D3=(c)** 实现 HB-ASSET-01：vendor
//! Polytope Labs `pallet-hyper-fungible-token` 已审计的核心（`send` 的 burn 分支、
//! `on_accept`、`on_timeout`、`Message` ABI 与精度函数），并改造为原生币的
//! **burn/mint** 模型，护栏与在途账本全部内联。与上游托管模型不同，原生 NEX 出站时
//! **真销毁**（`Currency::withdraw` → `TotalIssuance↓`），回桥时**真铸造**
//!（`Currency::deposit_creating` → `TotalIssuance↑`）。无本地资金池、无
//! `NativeFungibleAdapter`、无 fork，也不依赖已 deprecated 的 `pallet-token-gateway`
//! 或未发布的 HFT crate。
//!
//! See `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md` and `docs/HYPERBRIDGE_INTEGRATION.md`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod error;
pub mod impls;
pub mod module;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use types::Message;
pub use weights::WeightInfo;

use frame_support::PalletId;
use pallet_ismp::ModuleId;

/// The well-known ISMP module id for this bridge. EVM `NEX` contracts must set
/// this as the destination (`to`) when sending messages to Nexus, and the
/// runtime's `IsmpRouter` routes this id to this pallet.
/// 本桥的 ISMP 模块 id。EVM `NEX` 合约向 Nexus 发送消息时必须以此为目标（`to`），
/// 运行时 `IsmpRouter` 据此把消息路由到本 pallet。
pub const PALLET_ID: ModuleId = ModuleId::Pallet(PalletId(*b"nexbridg"));

/// Convenience: the byte form of [`PALLET_ID`] used in ISMP request `from`/`to`.
/// 便捷函数：[`PALLET_ID`] 的字节形式，用于 ISMP 请求的 `from`/`to`。
pub fn module_id_bytes() -> alloc::vec::Vec<u8> {
	PALLET_ID.to_bytes()
}

/// The native-currency balance type used throughout this pallet.
/// 本 pallet 通用的原生币余额类型。
pub type BalanceOf<T> = <<T as Config>::NativeCurrency as frame_support::traits::Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::types::{BridgeLimits, ChainConfig, EvmToSubstrate};
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, WithdrawReasons},
	};
	use frame_system::pallet_prelude::*;
	use ismp::{
		dispatcher::{DispatchPost, DispatchRequest, FeeMetadata, IsmpDispatcher},
		host::StateMachine,
	};
	use sp_core::{H160, H256, U256};
	use sp_runtime::traits::{CheckedAdd, Saturating, Zero};

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Configuration trait for the NEX asset bridge.
	/// NEX 资产桥的配置 trait。
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_ismp::Config {
		/// The aggregated runtime event type.
		/// 聚合的运行时事件类型。
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// ISMP request dispatcher (wired to `pallet-hyperbridge`, which charges the
		/// per-byte protocol fee then commits the request to `pallet-ismp`).
		/// ISMP 请求派发器（接 `pallet-hyperbridge`，先收取按字节协议费再把请求提交给
		/// `pallet-ismp`）。
		type Dispatcher: IsmpDispatcher<
			Account = Self::AccountId,
			Balance = <Self as pallet_ismp::Config>::Balance,
		>;

		/// The native currency (NEX). Bridged out via real burn, bridged back via
		/// real mint — `TotalIssuance` moves with cross-chain supply.
		/// 原生币（NEX）。出站真销毁、回桥真铸造——`TotalIssuance` 随跨链供给变动。
		type NativeCurrency: Currency<Self::AccountId>;

		/// EVM `H160` → Substrate `AccountId` mapping. Unused by the pure asset
		/// bridge; reserved for HB-ENT-01 (Stage 3).
		/// EVM `H160` → Substrate `AccountId` 映射。纯资产桥用不到；为 HB-ENT-01（Stage 3）预留。
		type EvmToSubstrate: EvmToSubstrate<Self>;

		/// Decimals of the native currency (NEX = 12).
		/// 原生币精度（NEX = 12）。
		#[pallet::constant]
		type NativeDecimals: Get<u8>;

		/// Minimum bridge amount. Must be `>= 10^(erc-local)` so inbound conversion
		/// never truncates to zero (see HB-ASSET-01 §3A.5 / G-A1-1).
		/// 最小桥接额。必须 `>= 10^(erc-local)`，以保证入站换算不会被截断为 0
		///（见 HB-ASSET-01 §3A.5 / G-A1-1）。
		#[pallet::constant]
		type MinBridgeAmount: Get<BalanceOf<Self>>;

		/// Length (in blocks) of the rolling daily-limit window.
		/// 滚动单日限额窗口的长度（区块数）。
		#[pallet::constant]
		type DailyLimitWindow: Get<BlockNumberFor<Self>>;

		/// ISMP request timeout, in seconds (0 = no timeout).
		/// ISMP 请求超时（秒，0 = 不超时）。
		#[pallet::constant]
		type RequestTimeout: Get<u64>;

		/// Privileged origin for governance ops (pause / limits / chain registry).
		/// 治理操作（暂停 / 限额 / 链注册）的特权来源。
		type BridgeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information.
		/// 权重信息。
		type WeightInfo: WeightInfo;
	}

	/// Cumulative NEX bridged out and still in flight; decremented on bridge-back
	/// and timeout refund. Mints can never exceed this (anti-inflation invariant).
	/// 已桥出且在途的 NEX 累计量；回桥与超时退款时递减。铸造永不超过此值（防增发不变量）。
	#[pallet::storage]
	pub type BridgedOut<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Per-destination in-flight amount; `Σ == BridgedOut` (checked by `try_state`).
	/// 按目标链分桶的在途量；`Σ == BridgedOut`（由 `try_state` 校验）。
	#[pallet::storage]
	pub type BridgedOutByChain<T: Config> =
		StorageMap<_, Blake2_128Concat, StateMachine, BalanceOf<T>, ValueQuery>;

	/// Global pause switch. 全局暂停开关。
	#[pallet::storage]
	pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// Per-chain pause switch. 分链暂停开关。
	#[pallet::storage]
	pub type PausedChain<T: Config> =
		StorageMap<_, Blake2_128Concat, StateMachine, bool, ValueQuery>;

	/// Rolling daily-out accumulator: `(window_start_block, amount_in_window)`.
	/// 滚动单日桥出累加器：`(窗口起始区块, 窗口内累计量)`。
	#[pallet::storage]
	pub type DailyOut<T: Config> =
		StorageValue<_, (BlockNumberFor<T>, BalanceOf<T>), ValueQuery>;

	/// Per-tx / per-day outbound limits (governance-adjustable).
	/// 出站单笔 / 单日限额（治理可调）。
	#[pallet::storage]
	pub type Limits<T: Config> = StorageValue<_, BridgeLimits<BalanceOf<T>>, ValueQuery>;

	/// Registered EVM chains: `StateMachine → ChainConfig{ contract, erc_decimals }`.
	/// Serves as the outbound target and the inbound source allow-list.
	/// 已注册 EVM 链：`StateMachine → ChainConfig{ contract, erc_decimals }`。同时作为
	/// 出站目标与入站来源 allow-list。
	#[pallet::storage]
	pub type Chains<T: Config> =
		StorageMap<_, Blake2_128Concat, StateMachine, ChainConfig, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// NEX was burned and an outbound ISMP POST dispatched.
		/// NEX 已销毁，并派发了出站 ISMP POST。
		BridgedOut {
			sender: T::AccountId,
			recipient: H160,
			dest: StateMachine,
			amount: BalanceOf<T>,
			commitment: H256,
		},
		/// NEX was minted to a beneficiary from a verified inbound message.
		/// 经已验证的入站消息，向受益人铸造了 NEX。
		BridgedIn { beneficiary: T::AccountId, source: StateMachine, amount: BalanceOf<T> },
		/// An outbound transfer timed out and the sender was refunded (re-minted).
		/// 出站转账超时，已向发送方退款（重新铸造）。
		BridgeRefunded { beneficiary: T::AccountId, dest: StateMachine, amount: BalanceOf<T> },
		/// A chain was registered / updated. 链已注册 / 更新。
		ChainRegistered { chain: StateMachine, contract: H160, erc_decimals: u8 },
		/// A chain was deregistered. 链已注销。
		ChainDeregistered { chain: StateMachine },
		/// Pause state changed (global if `chain` is `None`). 暂停状态变化（`chain` 为 `None` 即全局）。
		PausedSet { chain: Option<StateMachine>, paused: bool },
		/// Limits changed. 限额已变更。
		LimitsChanged { per_tx: BalanceOf<T>, daily: BalanceOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The bridge (or this destination lane) is paused. 桥（或该目标通道）已暂停。
		BridgePaused,
		/// The destination chain is not registered. 目标链未注册。
		ChainNotRegistered,
		/// The state machine is not an EVM chain. 状态机不是 EVM 链。
		NotEvmChain,
		/// Configured ERC decimals are below the native decimals. 配置的 ERC 精度低于原生精度。
		ErcDecimalsBelowLocal,
		/// Amount is below `MinBridgeAmount`. 金额低于 `MinBridgeAmount`。
		AmountBelowMin,
		/// Amount exceeds the per-tx limit. 金额超过单笔限额。
		PerTxLimitExceeded,
		/// Amount would exceed the daily limit. 金额将超过单日限额。
		DailyLimitExceeded,
		/// Insufficient spendable (free, ED-preserving) balance. 可用（free、保全 ED）余额不足。
		InsufficientFreeBalance,
		/// A ledger invariant would be violated. 将违反账本不变量。
		InvariantViolation,
		/// Arithmetic overflow. 算术溢出。
		Overflow,
		/// The ISMP dispatcher rejected the request. ISMP 派发器拒绝了该请求。
		DispatchFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<[u8; 32]>,
		BalanceOf<T>: Into<u128>,
		<T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
	{
		/// Bridge native NEX out to a registered EVM chain.
		///
		/// Guardrails run **before** any burn: not paused, `>= MinBridgeAmount`,
		/// `<= PerTxMax`, daily window not exceeded, and the spend respects locks +
		/// ED (`KeepAlive`). NEX is then really burned (`TotalIssuance↓`), the
		/// in-flight ledger is bumped, and an ISMP POST carrying the vendored
		/// [`Message`] ABI is dispatched. The whole call is transactional: if the
		/// dispatch fails, the burn and ledger writes are rolled back.
		///
		/// 桥出原生 NEX 到已注册的 EVM 链。
		///
		/// 护栏在任何销毁**之前**执行：未暂停、`>= MinBridgeAmount`、`<= PerTxMax`、
		/// 未超单日窗口，且消费尊重 locks + ED（`KeepAlive`）。随后真销毁 NEX
		///（`TotalIssuance↓`）、递增在途账本、派发携带 vendor [`Message`] ABI 的
		/// ISMP POST。整笔调用事务化：派发失败则销毁与账本写入回滚。
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::bridge_out())]
		pub fn bridge_out(
			origin: OriginFor<T>,
			dest: StateMachine,
			recipient: H160,
			amount: BalanceOf<T>,
			relayer_fee: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// --- Guardrails (before burn) / 护栏（销毁前） ---
			ensure!(!Paused::<T>::get(), Error::<T>::BridgePaused);
			ensure!(!PausedChain::<T>::get(dest), Error::<T>::BridgePaused);
			let cfg = Chains::<T>::get(dest).ok_or(Error::<T>::ChainNotRegistered)?;
			ensure!(amount >= T::MinBridgeAmount::get(), Error::<T>::AmountBelowMin);

			let limits = Limits::<T>::get();
			ensure!(amount <= limits.per_tx, Error::<T>::PerTxLimitExceeded);

			let now = frame_system::Pallet::<T>::block_number();
			let (window_start, used) = DailyOut::<T>::get();
			let (window_start, used) =
				if now.saturating_sub(window_start) >= T::DailyLimitWindow::get() {
					(now, Zero::zero())
				} else {
					(window_start, used)
				};
			let new_used = used.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
			ensure!(new_used <= limits.daily, Error::<T>::DailyLimitExceeded);

			// --- Burn (real, respects locks + ED) / 销毁（真实，尊重 locks + ED） ---
			let negative = T::NativeCurrency::withdraw(
				&who,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::KeepAlive,
			)
			.map_err(|_| Error::<T>::InsufficientFreeBalance)?;
			// Dropping the NegativeImbalance reduces TotalIssuance (the burn).
			// 丢弃 NegativeImbalance 会减少 TotalIssuance（即销毁）。
			drop(negative);

			// --- Ledger / 账本 ---
			BridgedOut::<T>::try_mutate(|b| -> DispatchResult {
				*b = b.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
				Ok(())
			})?;
			BridgedOutByChain::<T>::try_mutate(dest, |b| -> DispatchResult {
				*b = b.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
				Ok(())
			})?;
			DailyOut::<T>::put((window_start, new_used));

			// --- Encode + dispatch ISMP POST / 编码并派发 ISMP POST ---
			let sender: [u8; 32] = who.clone().into();
			let erc20_amount =
				impls::convert_to_erc20(amount.into(), cfg.erc_decimals, T::NativeDecimals::get());
			let message = Message {
				from: sender.to_vec().into(),
				to: recipient.0.to_vec().into(),
				amount: alloy_primitives::U256::from_be_bytes(erc20_amount.to_big_endian()),
				data: Default::default(),
			};

			let post = DispatchPost {
				dest,
				from: module_id_bytes(),
				to: cfg.contract.0.to_vec(),
				timeout: T::RequestTimeout::get(),
				body: {
					use alloy_sol_types::SolValue;
					Message::abi_encode(&message)
				},
			};
			let fee = <<T as pallet_ismp::Config>::Balance as From<BalanceOf<T>>>::from(
				relayer_fee,
			);
			let metadata = FeeMetadata { payer: who.clone(), fee };
			let commitment = <T as Config>::Dispatcher::default()
				.dispatch_request(DispatchRequest::Post(post), metadata)
				.map_err(|_| Error::<T>::DispatchFailed)?;

			Self::deposit_event(Event::<T>::BridgedOut {
				sender: who,
				recipient,
				dest,
				amount,
				commitment,
			});
			Ok(())
		}

		/// Governance: pause / resume (globally if `chain` is `None`).
		/// 治理：暂停 / 恢复（`chain` 为 `None` 即全局）。
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_paused())]
		pub fn set_paused(
			origin: OriginFor<T>,
			chain: Option<StateMachine>,
			paused: bool,
		) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			match chain {
				Some(c) => PausedChain::<T>::insert(c, paused),
				None => Paused::<T>::put(paused),
			}
			Self::deposit_event(Event::<T>::PausedSet { chain, paused });
			Ok(())
		}

		/// Governance: set per-tx and daily outbound limits.
		/// 治理：设置单笔与单日出站限额。
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_limits())]
		pub fn set_limits(
			origin: OriginFor<T>,
			per_tx: BalanceOf<T>,
			daily: BalanceOf<T>,
		) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			Limits::<T>::put(BridgeLimits { per_tx, daily });
			Self::deposit_event(Event::<T>::LimitsChanged { per_tx, daily });
			Ok(())
		}

		/// Governance: register or update an EVM chain (contract + ERC decimals).
		/// 治理：注册或更新一条 EVM 链（合约 + ERC 精度）。
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::register_chain())]
		pub fn register_chain(
			origin: OriginFor<T>,
			chain: StateMachine,
			contract: H160,
			erc_decimals: u8,
		) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			ensure!(chain.is_evm(), Error::<T>::NotEvmChain);
			ensure!(
				erc_decimals >= T::NativeDecimals::get(),
				Error::<T>::ErcDecimalsBelowLocal
			);
			Chains::<T>::insert(chain, ChainConfig { contract, erc_decimals });
			Self::deposit_event(Event::<T>::ChainRegistered { chain, contract, erc_decimals });
			Ok(())
		}

		/// Governance: deregister an EVM chain. Inbound/outbound for it then fail.
		/// 治理：注销一条 EVM 链。其入站/出站随后失败。
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::deregister_chain())]
		pub fn deregister_chain(origin: OriginFor<T>, chain: StateMachine) -> DispatchResult {
			T::BridgeOrigin::ensure_origin(origin)?;
			Chains::<T>::remove(chain);
			Self::deposit_event(Event::<T>::ChainDeregistered { chain });
			Ok(())
		}
	}

	impl<T> Default for Pallet<T> {
		fn default() -> Self {
			Self(PhantomData)
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::check_ledger_invariant().map_err(Into::into)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Invariant: `Σ BridgedOutByChain == BridgedOut`.
		/// 不变量：`Σ BridgedOutByChain == BridgedOut`。
		pub fn check_ledger_invariant() -> Result<(), &'static str> {
			let mut sum: BalanceOf<T> = Zero::zero();
			for (_chain, amount) in BridgedOutByChain::<T>::iter() {
				sum = sum.checked_add(&amount).ok_or("BridgedOutByChain sum overflow")?;
			}
			if sum != BridgedOut::<T>::get() {
				return Err("Σ BridgedOutByChain != BridgedOut");
			}
			Ok(())
		}

		/// Decodes a 20-byte (EVM) or 32-byte (Substrate) address into an `AccountId`.
		/// 将 20 字节（EVM）或 32 字节（Substrate）地址解码为 `AccountId`。
		pub(crate) fn account_from_bytes(
			bytes: &[u8],
		) -> Result<T::AccountId, crate::error::BridgeError>
		where
			T::AccountId: From<[u8; 32]>,
		{
			let mut buf = [0u8; 32];
			match bytes.len() {
				32 => buf.copy_from_slice(bytes),
				20 => buf[12..].copy_from_slice(bytes),
				other => return Err(crate::error::BridgeError::InvalidRecipientLength(other)),
			}
			Ok(buf.into())
		}

		/// Inbound credit: mint `amount` NEX to `beneficiary` and decrement the
		/// in-flight ledger for `chain`. Enforces `amount <= BridgedOut` and
		/// `amount <= BridgedOutByChain[chain]` (anti-inflation invariant). All
		/// checks precede the (infallible) mint, so no partial state is possible.
		/// 入站入账：向 `beneficiary` 铸造 `amount` NEX，并对 `chain` 递减在途账本。
		/// 强制 `amount <= BridgedOut` 与 `amount <= BridgedOutByChain[chain]`（防增发
		/// 不变量）。所有校验先于（不可失败的）铸造，故不会留下部分状态。
		pub(crate) fn credit_inbound(
			beneficiary: &T::AccountId,
			chain: StateMachine,
			amount: BalanceOf<T>,
		) -> Result<(), crate::error::BridgeError> {
			let total = BridgedOut::<T>::get();
			let per_chain = BridgedOutByChain::<T>::get(chain);
			if amount > total || amount > per_chain {
				return Err(crate::error::BridgeError::InvariantViolation);
			}
			BridgedOut::<T>::put(total.saturating_sub(amount));
			BridgedOutByChain::<T>::insert(chain, per_chain.saturating_sub(amount));
			// Dropping the PositiveImbalance increases TotalIssuance (the mint).
			// 丢弃 PositiveImbalance 会增加 TotalIssuance（即铸造）。
			drop(T::NativeCurrency::deposit_creating(beneficiary, amount));
			Ok(())
		}

		/// Converts an ERC-20 `U256` amount (big-endian alloy value) into local NEX,
		/// using the per-chain ERC decimals. Enforces `>= MinBridgeAmount`.
		/// 用逐链 ERC 精度，将 ERC-20 `U256` 金额（big-endian alloy 值）换算为本地 NEX。
		/// 强制 `>= MinBridgeAmount`。
		pub(crate) fn decode_amount(
			erc_amount: alloy_primitives::U256,
			erc_decimals: u8,
		) -> Result<BalanceOf<T>, crate::error::BridgeError>
		where
			BalanceOf<T>: core::str::FromStr,
		{
			let amount = impls::convert_to_balance::<BalanceOf<T>>(
				U256::from_big_endian(&erc_amount.to_be_bytes::<32>()),
				erc_decimals,
				T::NativeDecimals::get(),
			)
			.map_err(|_| {
				crate::error::BridgeError::InvalidAmountConversion(alloc::format!(
					"amount conversion failed"
				))
			})?;
			if amount < T::MinBridgeAmount::get() {
				return Err(crate::error::BridgeError::AmountBelowMin);
			}
			Ok(amount)
		}
	}
}
