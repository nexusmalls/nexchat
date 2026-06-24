// Copyright (C) Polytope Labs Ltd. (vendored on_accept / on_timeout shape)
// Copyright (C) Nexus contributors (native mint/refund adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! ISMP module callbacks for the NEX asset bridge.
//! NEX 资产桥的 ISMP 模块回调。
//!
//! Replay / idempotency note: `pallet-ismp` records request receipts and rejects
//! duplicates *before* dispatching to this module, so neither `on_accept` (inbound
//! mint) nor `on_timeout` (refund mint) can be replayed. We therefore do not keep
//! a redundant commitment set, matching the audited HFT design.
//! 重放 / 幂等说明：`pallet-ismp` 在派发到本模块**之前**记录请求回执并拒绝重复，故
//! `on_accept`（入站铸造）与 `on_timeout`（退款铸造）均不会被重放。因此我们不额外维护
//! commitment 集合，与已审计的 HFT 设计一致。

use crate::{
	error::BridgeError,
	pallet::{Chains, Event, Paused, PausedChain},
	types::{CrossChainOrderHandler, EvmToSubstrate, InboundOp, OrderIntent},
	BalanceOf, Config, Message, Pallet,
};
use alloy_sol_types::SolValue;
use codec::Decode;
use frame_support::{storage::with_storage_layer, weights::Weight};
use ismp::{
	host::StateMachine,
	module::IsmpModule,
	router::{GetResponse, PostRequest, Request},
};
use sp_core::H160;
use sp_runtime::traits::{Get, Zero};

/// Maps a [`BridgeError`] into the `anyhow::Error` that `pallet-ismp` expects.
/// 将 [`BridgeError`] 映射为 `pallet-ismp` 期望的 `anyhow::Error`。
fn into_anyhow(e: BridgeError) -> anyhow::Error {
	anyhow::anyhow!(alloc::format!("{e}"))
}

impl<T: Config> IsmpModule for Pallet<T>
where
	T::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
	BalanceOf<T>: core::str::FromStr + Into<u128>,
	<T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
{
	/// Inbound: an EVM chain burned `NEX` and Hyperbridge proved it. Mint the
	/// local NEX (within the in-flight ledger) to the beneficiary.
	/// 入站：某 EVM 链销毁了 `NEX` 且 Hyperbridge 已证明。在在途账本内向受益人铸造本地 NEX。
	fn on_accept(
		&self,
		PostRequest { body, from, source, .. }: PostRequest,
	) -> Result<Weight, anyhow::Error> {
		// Pause checks first / 先做暂停检查
		if Paused::<T>::get() || PausedChain::<T>::get(source) {
			Err(into_anyhow(BridgeError::Paused(source)))?
		}

		// Source allow-list: the registered contract for `source` must equal `from`.
		// 来源 allow-list：`source` 已注册合约必须等于 `from`。
		let cfg = Chains::<T>::get(source)
			.ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(source)))?;
		if from != cfg.contract.0.to_vec() {
			Err(into_anyhow(BridgeError::UnknownSourceContract(source)))?
		}

		// Decode the vendored Message ABI. / 解码 vendor 的 Message ABI。
		let message = Message::abi_decode(&body).map_err(|e| into_anyhow(BridgeError::DecodeError(e)))?;

		// `data` empty → plain asset transfer (Stage 2). Non-empty → an authenticated
		// operation (HB-ENT-01, Stage 3) decoded from the SCALE `InboundOp` enum. Only
		// the asset-bearing paths read `message.amount` (a withdraw carries `amount=0`).
		// `data` 为空 → 纯资产转账（Stage 2）。非空 → 经鉴权操作（HB-ENT-01，Stage 3），
		// 按 SCALE `InboundOp` 枚举解码。仅携带资产的路径读取 `message.amount`（提款 `amount=0`）。
		if message.data.is_empty() {
			let amount =
				Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals).map_err(into_anyhow)?;
			let beneficiary =
				Pallet::<T>::account_from_bytes(message.to.as_ref()).map_err(into_anyhow)?;
			Pallet::<T>::credit_inbound(&beneficiary, source, amount).map_err(into_anyhow)?;
			Pallet::<T>::deposit_event(Event::<T>::BridgedIn { beneficiary, source, amount });
			return Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(6, 3));
		}

		let op = InboundOp::decode(&mut message.data.as_ref())
			.map_err(|_| into_anyhow(BridgeError::InvalidPayload))?;
		match op {
			InboundOp::Order(intent) => {
				let amount = Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals)
					.map_err(into_anyhow)?;
				Self::on_cross_order(source, amount, intent)
			},
			InboundOp::Withdraw(req) => Self::on_withdraw(source, req),
		}
	}

	// (cross-order handling lives in the inherent impl below)

	/// The asset bridge does not issue GET requests, so it never receives responses.
	/// 资产桥不发起 GET 请求，故永不接收响应。
	fn on_response(&self, _response: GetResponse) -> Result<Weight, anyhow::Error> {
		Err(into_anyhow(BridgeError::ResponsesNotSupported))?
	}

	/// Outbound POST timed out: refund the original sender by re-minting the burned
	/// NEX and decrementing the in-flight ledger for the destination chain.
	/// 出站 POST 超时：通过重新铸造已销毁的 NEX 并对目标链递减在途账本，退款给原发送方。
	fn on_timeout(&self, request: Request) -> Result<Weight, anyhow::Error> {
		match request {
			Request::Post(PostRequest { body, to, dest, .. }) => {
				let cfg = Chains::<T>::get(dest)
					.ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(dest)))?;
				// Sanity: the timed-out request must have targeted the registered contract.
				// 完整性：超时的请求必须曾以已注册合约为目标。
				if to != cfg.contract.0.to_vec() {
					Err(into_anyhow(BridgeError::UnknownSourceContract(dest)))?
				}

				let message =
					Message::abi_decode(&body).map_err(|e| into_anyhow(BridgeError::DecodeError(e)))?;

				// Refund recipient = the original sender encoded in `message.from`.
				// 退款收款人 = `message.from` 中编码的原发送方。
				let sender_bytes = message.from.as_ref();
				let sender = Pallet::<T>::account_from_bytes(sender_bytes).map_err(|_| {
					into_anyhow(BridgeError::InvalidSenderLength(sender_bytes.len()))
				})?;
				let amount = Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals)
					.map_err(into_anyhow)?;

				Pallet::<T>::credit_inbound(&sender, dest, amount).map_err(into_anyhow)?;

				Pallet::<T>::deposit_event(Event::<T>::BridgeRefunded {
					beneficiary: sender,
					dest,
					amount,
				});
				Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(6, 3))
			},
			Request::Get(_) => Err(into_anyhow(BridgeError::UnsupportedTimeoutType))?,
		}
	}
}

impl<T: Config> Pallet<T>
where
	T::AccountId: From<[u8; 32]> + Into<[u8; 32]>,
	BalanceOf<T>: core::str::FromStr + Into<u128>,
	<T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
{
	/// Handle an inbound [`OrderIntent`] (HB-ENT-01): mint the bridged NEX to the
	/// derived buyer (within the in-flight ledger), then dispatch the digital order
	/// in a nested storage layer. On order failure the mint is **kept** (DerivedCredit)
	/// and only the order side is rolled back, satisfying the "never burned without
	/// settlement" invariant; `on_accept` still returns `Ok` so the ISMP receipt is
	/// persisted and the request is not replayed.
	/// 处理入站 [`OrderIntent`]（HB-ENT-01）：在在途账本内向派生买家铸造已桥接 NEX，再在嵌套
	/// 存储层内派发数字下单。下单失败时**保留**铸造额（DerivedCredit），仅回滚订单侧，满足
	/// “绝不已 burn 却无结算”不变量；`on_accept` 仍返回 `Ok`，以持久化 ISMP 回执、避免重放。
	fn on_cross_order(
		source: StateMachine,
		amount: BalanceOf<T>,
		intent: OrderIntent,
	) -> Result<Weight, anyhow::Error> {
		let buyer = <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(intent.buyer_evm));
		let referrer = intent
			.referrer
			.map(|e| <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(e)));

		// Mint NEX to the derived buyer (anti-inflation invariant enforced inside).
		// 向派生买家铸造 NEX（防增发不变量在内部强制）。
		Pallet::<T>::credit_inbound(&buyer, source, amount).map_err(into_anyhow)?;

		// The bridged amount is the buyer's budget and the slippage cap: the order
		// may charge at most what was bridged; any remainder stays as DerivedCredit.
		// 已桥接额即买家预算与滑点上限：下单最多扣已桥接额；余额留作 DerivedCredit。
		let res = with_storage_layer(|| {
			T::CrossOrderHandler::do_cross_order(
				buyer.clone(),
				buyer.clone(),
				intent.product_id,
				intent.quantity,
				amount,
				referrer.clone(),
			)
		});

		match res {
			Ok(order_id) => Pallet::<T>::deposit_event(Event::<T>::CrossOrderPlaced {
				order_id,
				buyer,
				source,
				product_id: intent.product_id,
			}),
			Err(error) => Pallet::<T>::deposit_event(Event::<T>::CrossOrderFailed {
				buyer,
				source,
				amount,
				error,
			}),
		}
		Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(12, 8))
	}

	/// Handle an inbound [`WithdrawRequest`](crate::types::WithdrawRequest)
	/// (HB-ENT-01 §7, G-B4): move a derived account's NEX back to the source EVM
	/// chain. Authorisation is enforced on the EVM side (the gateway checks
	/// `msg.sender == owner_evm`) and by the inbound source-contract allow-list; on
	/// Nexus we only ever debit the derived owner's own balance. Reuses the outbound
	/// core (burn `KeepAlive` + ledger + ISMP POST) inside a nested storage layer so a
	/// dispatch failure rolls back the burn cleanly.
	/// 处理入站 [`WithdrawRequest`](crate::types::WithdrawRequest)（HB-ENT-01 §7，G-B4）：
	/// 将派生账户的 NEX 提回来源 EVM 链。鉴权在 EVM 侧完成（网关校验 `msg.sender == owner_evm`）
	/// 并由入站来源合约 allow-list 保证；Nexus 侧只扣派生持有人本人余额。在嵌套存储层内复用
	/// 出站核心（`KeepAlive` 销毁 + 账本 + ISMP POST），以便派发失败时干净回滚销毁。
	fn on_withdraw(
		source: StateMachine,
		req: crate::types::WithdrawRequest,
	) -> Result<Weight, anyhow::Error> {
		let cfg = Chains::<T>::get(source)
			.ok_or_else(|| into_anyhow(BridgeError::UnregisteredChain(source)))?;

		let owner = <T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(req.owner_evm));
		let recipient = H160(req.dest_recipient);
		let amount = Pallet::<T>::decode_amount(
			alloy_primitives::U256::from(req.amount_nex),
			cfg.erc_decimals,
		)
		.map_err(into_anyhow)?;

		// Burn from the derived owner + dispatch back to `source`, atomically.
		// 从派生持有人销毁 + 派回 `source`，原子执行。
		let commitment = with_storage_layer(|| {
			Pallet::<T>::do_outbound(&owner, source, recipient, amount, BalanceOf::<T>::zero())
		})
		.map_err(|e| anyhow::anyhow!(alloc::format!("{e:?}")))?;

		Pallet::<T>::deposit_event(Event::<T>::DerivedWithdraw {
			owner,
			dest: source,
			recipient,
			amount,
			commitment,
		});
		Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(8, 6))
	}
}
