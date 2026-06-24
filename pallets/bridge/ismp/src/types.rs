// Copyright (C) Polytope Labs Ltd. (vendored ABI / precision logic)
// Copyright (C) Nexus contributors (burn/mint adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for `pallet-bridge-ismp`.
//! `pallet-bridge-ismp` 的类型定义。
//!
//! The [`Message`] ABI and the [`EvmToSubstrate`] trait are **vendored** from
//! Polytope Labs' `pallet-hyper-fungible-token` (commit `6931d9f6`, crate line
//! `2512.0.0`) so that the on-chain encoding byte-matches the EVM-side ERC-6160
//! `NEX` contract (D3=(c), see `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md` §13.3).
//! [`Message`] ABI 与 [`EvmToSubstrate`] trait 自 Polytope Labs 的
//! `pallet-hyper-fungible-token`（commit `6931d9f6`，crate 线 `2512.0.0`）**vendor**
//! 而来，以保证链上编码与 EVM 侧 ERC-6160 `NEX` 合约逐字节对齐（D3=(c)）。

use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_core::{H160, H256};

// ABI-compatible message matching the Solidity HyperFungibleToken `Message` struct:
// `struct Message { bytes from; bytes to; uint256 amount; bytes data; }`.
// VENDORED verbatim from `pallet-hyper-fungible-token@6931d9f6` so encoding is
// byte-identical to the EVM contract.
// 与 Solidity HyperFungibleToken `Message` 结构对齐的 ABI 兼容消息。逐字节 vendor 自
// `pallet-hyper-fungible-token@6931d9f6`，确保与 EVM 合约编码一致。
alloy_sol_macro::sol! {
	#![sol(all_derives)]
	struct Message {
		bytes from;
		bytes to;
		uint256 amount;
		bytes data;
	}
}

/// Per-EVM-chain bridge configuration for native NEX.
/// 针对原生 NEX 的逐 EVM 链桥接配置。
///
/// `contract` is both the outbound destination (the `to` module id of the ISMP
/// POST) and the inbound source allow-list entry (`on_accept` requires
/// `from == contract`). `erc_decimals` is the ERC-20 precision on that chain;
/// it must be `>= NativeDecimals` so the 18→12 down-scaling never widens dust.
/// `contract` 同时作为出站目标（ISMP POST 的 `to` 模块 id）与入站来源 allow-list
///（`on_accept` 要求 `from == contract`）。`erc_decimals` 为该链 ERC-20 精度，必须
/// `>= NativeDecimals`，以保证 18→12 缩小换算不会放大 dust。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct ChainConfig {
	/// The ERC-6160 `NEX` token contract address on the EVM chain.
	/// EVM 链上的 ERC-6160 `NEX` 代币合约地址。
	pub contract: H160,
	/// ERC-20 decimals on the EVM chain (must be `>= NativeDecimals`).
	/// EVM 链上的 ERC-20 精度（必须 `>= NativeDecimals`）。
	pub erc_decimals: u8,
}

/// Per-tx / per-day outbound limits (governance-adjustable via `set_limits`).
/// 出站单笔 / 单日限额（经 `set_limits` 治理调整）。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug, MaxEncodedLen)]
pub struct BridgeLimits<Balance> {
	/// Maximum amount per single `bridge_out`. Single-tx cap.
	/// 单次 `bridge_out` 的上限。
	pub per_tx: Balance,
	/// Maximum cumulative amount bridged out within one `DailyLimitWindow`.
	/// 单个 `DailyLimitWindow` 内累计桥出上限。
	pub daily: Balance,
}

impl<Balance: Default> Default for BridgeLimits<Balance> {
	fn default() -> Self {
		Self { per_tx: Balance::default(), daily: Balance::default() }
	}
}

/// Converts an EVM `H160` address into a Substrate `AccountId`.
/// 将 EVM `H160` 地址转换为 Substrate `AccountId`。
///
/// VENDORED from HFT. Unused by the pure NEX asset bridge (Stage 2); reserved for
/// HB-ENT-01 cross-chain ordering (Stage 3), which derives accounts from EVM keys.
/// 自 HFT vendor。纯 NEX 资产桥（Stage 2）用不到；为 HB-ENT-01 跨链下单（Stage 3）
/// 从 EVM 公钥派生账户预留。
pub trait EvmToSubstrate<T: frame_system::Config> {
	fn convert(addr: H160) -> T::AccountId;
}

/// Default: zero-pad the 20-byte EVM address into a 32-byte `AccountId`.
/// 默认实现：将 20 字节 EVM 地址零填充为 32 字节 `AccountId`。
impl<T: frame_system::Config> EvmToSubstrate<T> for ()
where
	<T as frame_system::Config>::AccountId: From<[u8; 32]>,
{
	fn convert(addr: H160) -> <T as frame_system::Config>::AccountId {
		let mut account = [0u8; 32];
		account[12..].copy_from_slice(&addr.0);
		account.into()
	}
}

/// Re-export for event payloads.
/// 供事件负载复用。
pub type Commitment = H256;
