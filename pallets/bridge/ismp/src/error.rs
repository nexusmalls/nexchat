// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Error types for the bridge's ISMP module callbacks (`on_accept` / `on_timeout`).
//! These are returned as `anyhow::Error` to `pallet-ismp` and do not mutate the
//! ledger (all ledger writes happen only on the success path).
//! 桥接 ISMP 模块回调（`on_accept` / `on_timeout`）的错误类型。以 `anyhow::Error`
//! 返回给 `pallet-ismp`，且不改动账本（账本写入仅发生在成功路径）。

extern crate alloc;

use ismp::host::StateMachine;

/// Errors raised while processing an inbound ISMP message for the NEX bridge.
/// 处理 NEX 桥入站 ISMP 消息时可能产生的错误。
#[derive(thiserror::Error, Debug)]
pub enum BridgeError {
	/// The source state machine is not a registered EVM chain.
	/// 来源状态机不是已注册的 EVM 链。
	#[error("Source chain {0:?} is not registered")]
	UnregisteredChain(StateMachine),
	/// The `from` module id does not match the registered source contract (allow-list).
	/// `from` 模块 id 与已注册来源合约不符（allow-list）。
	#[error("Unknown source contract on {0:?}")]
	UnknownSourceContract(StateMachine),
	/// The bridge (or this chain lane) is paused.
	/// 桥（或该链通道）已暂停。
	#[error("Bridge is paused for {0:?}")]
	Paused(StateMachine),
	/// Failed to ABI-decode the [`Message`](crate::Message) body.
	/// 无法对 [`Message`](crate::Message) 消息体做 ABI 解码。
	#[error("Failed to decode message: {0}")]
	DecodeError(alloy_sol_types::Error),
	/// Recipient byte length is neither 20 (EVM) nor 32 (Substrate).
	/// 收款人字节长度既非 20（EVM）也非 32（Substrate）。
	#[error("Invalid recipient length: {0}")]
	InvalidRecipientLength(usize),
	/// Sender byte length on a timeout refund is neither 20 nor 32.
	/// 超时退款时发送方字节长度既非 20 也非 32。
	#[error("Invalid sender length: {0}")]
	InvalidSenderLength(usize),
	/// Decimals/precision conversion failed or overflowed.
	/// 精度换算失败或溢出。
	#[error("Invalid amount conversion: {0}")]
	InvalidAmountConversion(alloc::string::String),
	/// Converted amount is below the configured minimum.
	/// 换算后金额低于配置的最小值。
	#[error("Amount below minimum bridge amount")]
	AmountBelowMin,
	/// Inbound mint / refund would exceed the in-flight `BridgedOut` ledger
	/// (would mint NEX that was never burned). Hard invariant guard.
	/// 入站铸造 / 退款将超过在途 `BridgedOut` 账本（会铸出从未销毁的 NEX）。硬不变量护栏。
	#[error("Inbound amount exceeds in-flight bridged-out ledger")]
	InvariantViolation,
	/// GET responses are not supported by the asset bridge.
	/// 资产桥不支持 GET 响应。
	#[error("Responses are not supported")]
	ResponsesNotSupported,
	/// GET-request timeouts are not supported (the bridge only sends POSTs).
	/// 不支持 GET 请求超时（桥仅发送 POST）。
	#[error("Unsupported timeout type")]
	UnsupportedTimeoutType,
	/// The non-empty `Message.data` failed to SCALE-decode into an `InboundOp`
	/// (HB-ENT-01). 非空 `Message.data` 无法 SCALE 解码为 `InboundOp`（HB-ENT-01）。
	#[error("Invalid inbound operation payload")]
	InvalidPayload,
}
