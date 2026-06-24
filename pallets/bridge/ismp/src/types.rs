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

/// Cross-chain order intent (HB-ENT-01), carried SCALE-encoded inside the
/// vendored [`Message`]'s `data` field. `schema_version` allows forward-compatible
/// evolution; `buyer_evm` / `referrer` are EVM `H160`s that are derived into local
/// `AccountId`s via [`EvmToSubstrate`]. `nonce` complements the ISMP commitment as
/// an application-level replay guard.
/// 跨链下单意图（HB-ENT-01），以 SCALE 编码置于 vendor [`Message`] 的 `data` 字段内。
/// `schema_version` 支持向前兼容演进；`buyer_evm` / `referrer` 为 EVM `H160`，经
/// [`EvmToSubstrate`] 派生为本地 `AccountId`；`nonce` 配合 ISMP commitment 作应用层防重放。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct OrderIntent {
	/// Payload schema version (for forward-compatible evolution). 负载版本号。
	pub schema_version: u8,
	/// Buyer's EVM address; derived into the local buyer account. 买家 EVM 地址（派生本地账户）。
	pub buyer_evm: [u8; 20],
	/// Target product id. 目标商品 id。
	pub product_id: u64,
	/// Order quantity. 下单数量。
	pub quantity: u32,
	/// NEX burned on the source EVM chain (EVM precision). 源 EVM 链销毁的 NEX（EVM 精度）。
	pub amount_nex: u128,
	/// Slippage cap on the NEX charged (EVM precision). NEX 扣费滑点上限（EVM 精度）。
	pub max_nex_amount: u128,
	/// Optional referrer EVM address (derived). 可选推荐人 EVM 地址（派生）。
	pub referrer: Option<[u8; 20]>,
	/// Application-level replay nonce. 应用层防重放 nonce。
	pub nonce: u64,
}

/// Withdraw a derived account's NEX back to an EVM chain (HB-ENT-01 §7, G-B4).
/// Authorisation is performed on the EVM side (the gateway checks
/// `msg.sender == owner_evm`); on Nexus we trust the registered source contract
/// (same allow-list as every inbound message) and move only the derived owner's funds.
/// 将派生账户的 NEX 提回某 EVM 链（HB-ENT-01 §7，G-B4）。鉴权在 EVM 侧完成（网关校验
/// `msg.sender == owner_evm`）；Nexus 侧信任已注册来源合约（与所有入站消息同一 allow-list），
/// 且只动用该派生账户本人的资金。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct WithdrawRequest {
	/// Payload schema version. 负载版本号。
	pub schema_version: u8,
	/// Derived owner's EVM address (whose Nexus account is debited). 派生持有人 EVM 地址（其 Nexus 账户被扣款）。
	pub owner_evm: [u8; 20],
	/// Amount to withdraw (EVM precision). 提款金额（EVM 精度）。
	pub amount_nex: u128,
	/// EVM recipient of the bridged-back NEX. 提回 NEX 的 EVM 收款人。
	pub dest_recipient: [u8; 20],
	/// Application-level replay nonce. 应用层防重放 nonce。
	pub nonce: u64,
}

/// Discriminated operation carried in [`Message`]`.data`. An empty `data` means a
/// plain asset transfer (Stage 2); a non-empty `data` SCALE-decodes to this enum.
/// [`Message`]`.data` 携带的判别式操作。空 `data` 表示纯资产转账（Stage 2）；非空 `data`
/// 按本枚举 SCALE 解码。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub enum InboundOp {
	/// Cross-chain digital order (HB-ENT-01 Stage 3b). 跨链数字商品下单。
	Order(OrderIntent),
	/// Withdraw a derived account's NEX back to an EVM chain (HB-ENT-01 §7, Stage 3c).
	/// 将派生账户的 NEX 提回某 EVM 链（HB-ENT-01 §7，Stage 3c）。
	Withdraw(WithdrawRequest),
}

/// Bridge → business handler for authenticated cross-chain digital orders.
/// Implemented by the runtime against `pallet-entity-order::do_cross_order`,
/// keeping the low-level bridge decoupled from the high-level order pallet.
/// Returns the created order id on success.
/// 桥 → 业务的跨链数字下单处理器（经鉴权）。由 runtime 对接
/// `pallet-entity-order::do_cross_order` 实现，使底层桥与上层订单 pallet 解耦；成功返回订单 id。
pub trait CrossChainOrderHandler<AccountId, Balance> {
	fn do_cross_order(
		buyer: AccountId,
		payer: AccountId,
		product_id: u64,
		quantity: u32,
		max_nex_amount: Balance,
		referrer: Option<AccountId>,
	) -> Result<u64, sp_runtime::DispatchError>;
}

/// Default: no handler configured — every cross-order fails (and is credited).
/// 默认：未配置处理器——所有跨链下单失败（并入账）。
impl<AccountId, Balance> CrossChainOrderHandler<AccountId, Balance> for () {
	fn do_cross_order(
		_buyer: AccountId,
		_payer: AccountId,
		_product_id: u64,
		_quantity: u32,
		_max_nex_amount: Balance,
		_referrer: Option<AccountId>,
	) -> Result<u64, sp_runtime::DispatchError> {
		Err(sp_runtime::DispatchError::Other("cross-order handler not configured"))
	}
}
