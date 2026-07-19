// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! USDX PSM data types.
//! USDX PSM 数据类型。

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::RuntimeDebug;

/// Basis-point denominator used by all PSM policies.
/// PSM 策略统一使用的基点分母。
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Mint and redeem policy for one receipt lane.
/// 单条收据资产通道的铸造与赎回策略。
#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct CollateralPolicy {
    /// Receipt value recognized during mint, in basis points.
    /// 铸造时认可的收据价值，单位为基点。
    pub mint_factor_bps: u16,
    /// Forgone USDX issuance retained as receipt surplus, in basis points.
    /// 通过少铸 USDX 留作收据盈余的费用，单位为基点。
    pub mint_fee_bps: u16,
    /// Receipt retained by the PSM during redemption, in basis points.
    /// 赎回时由 PSM 留存的收据费用，单位为基点。
    pub redeem_fee_bps: u16,
}

/// Per-lane amount, rolling-window, and debt limits.
/// 单通道金额、滚动窗口与债务限制。
#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct LaneLimits<BlockNumber> {
    pub min_amount: u128,
    pub max_per_tx: u128,
    pub max_per_window: u128,
    pub window_blocks: BlockNumber,
    pub debt_ceiling: u128,
}

/// Registered receipt lane configuration.
/// 已注册收据资产通道配置。
#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct LaneConfig {
    pub descriptor_hash: H256,
    pub activation_evidence_hash: H256,
    pub enabled: bool,
}

/// Governance-verified evidence binding one receipt lane to its EVM collateral.
/// 经治理验证、用于绑定收据通道与 EVM 抵押资产的证据。
#[derive(
    Clone,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct LaneActivationEvidence {
    pub wrapper_contract: [u8; 20],
    pub underlying_contract: [u8; 20],
    pub owner_contract: [u8; 20],
    pub host_contract: [u8; 20],
    pub dispatcher_contract: [u8; 20],
    pub is_weth: bool,
    pub hft_bytecode_hash: H256,
    pub controller_bytecode_hash: H256,
    pub config_block: u64,
    pub config_block_hash: H256,
    pub nexus_peer_hash: H256,
    pub proof_bundle_hash: H256,
}

/// Usage accumulated in one fixed block window.
/// 固定区块窗口内累计的使用量。
#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Default,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub struct WindowUsage<BlockNumber> {
    pub window_start: BlockNumber,
    pub used_amount: u128,
}

/// Direction of a lane rate-limit window.
/// 通道限速窗口的方向。
#[derive(
    Clone,
    Copy,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    RuntimeDebug,
    TypeInfo,
)]
pub enum WindowDirection {
    Mint,
    Redeem,
}
