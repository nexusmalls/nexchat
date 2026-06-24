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
	Config, Message, Pallet,
};
use alloy_sol_types::SolValue;
use frame_support::weights::Weight;
use ismp::{
	module::IsmpModule,
	router::{GetResponse, PostRequest, Request},
};
use sp_runtime::traits::Get;

/// Maps a [`BridgeError`] into the `anyhow::Error` that `pallet-ismp` expects.
/// 将 [`BridgeError`] 映射为 `pallet-ismp` 期望的 `anyhow::Error`。
fn into_anyhow(e: BridgeError) -> anyhow::Error {
	anyhow::anyhow!(alloc::format!("{e}"))
}

impl<T: Config> IsmpModule for Pallet<T>
where
	T::AccountId: From<[u8; 32]>,
	crate::BalanceOf<T>: core::str::FromStr,
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

		let beneficiary =
			Pallet::<T>::account_from_bytes(message.to.as_ref()).map_err(into_anyhow)?;
		let amount =
			Pallet::<T>::decode_amount(message.amount, cfg.erc_decimals).map_err(into_anyhow)?;

		// NOTE: `message.data` (the BodyWithCall calldata path) is intentionally
		// ignored in Stage 2 (pure asset bridge). HB-ENT-01 (Stage 3) adds the
		// authenticated cross-order dispatch on top of this mint.
		// 注：`message.data`（BodyWithCall calldata 路径）在 Stage 2（纯资产桥）刻意忽略。
		// HB-ENT-01（Stage 3）在此铸造之上加入经鉴权的跨链下单派发。

		Pallet::<T>::credit_inbound(&beneficiary, source, amount).map_err(into_anyhow)?;

		Pallet::<T>::deposit_event(Event::<T>::BridgedIn {
			beneficiary,
			source,
			amount,
		});
		Ok(<T as frame_system::Config>::DbWeight::get().reads_writes(6, 3))
	}

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
