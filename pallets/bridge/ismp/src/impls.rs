// Copyright (C) Polytope Labs Ltd. (vendored precision logic)
// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Precision conversion helpers, VENDORED verbatim from
//! `pallet-hyper-fungible-token@6931d9f6` (PR #907 *token decimal conversion
//! guards*). Kept byte-identical so EVM↔Nexus amount conversion is symmetric.
//! 精度换算辅助函数，逐字节 vendor 自 `pallet-hyper-fungible-token@6931d9f6`
//!（PR #907 *token decimal conversion guards*）。保持一致以使 EVM↔Nexus 金额换算对称。

use alloc::string::ToString;
use sp_core::U256;

/// Converts an ERC-20 `U256` amount to a local balance.
/// 将 ERC-20 `U256` 金额转换为本地余额。
///
/// Divides by `10^(erc_decimals - local_decimals)` (integer division → **dust is
/// truncated**, e.g. `1 wei → 0`). Requires `erc_decimals >= local_decimals`.
/// 除以 `10^(erc_decimals - local_decimals)`（整数除 → **截断 dust**，如 `1 wei → 0`）。
/// 要求 `erc_decimals >= local_decimals`。
pub fn convert_to_balance<B: core::str::FromStr>(
    value: U256,
    erc_decimals: u8,
    local_decimals: u8,
) -> Result<B, B::Err> {
    let dec_str = (value
        / U256::from(10u128.pow(erc_decimals.saturating_sub(local_decimals) as u32)))
    .to_string();
    dec_str.parse::<B>()
}

/// Converts a local `u128` balance to an ERC-20 `U256` amount.
/// 将本地 `u128` 余额转换为 ERC-20 `U256` 金额。
///
/// Multiplies by `10^(erc_decimals - local_decimals)`; **no dust** on the way out.
/// 乘以 `10^(erc_decimals - local_decimals)`；出站**无 dust**。
pub fn convert_to_erc20(value: u128, erc_decimals: u8, local_decimals: u8) -> U256 {
    U256::from(value) * U256::from(10u128.pow(erc_decimals.saturating_sub(local_decimals) as u32))
}
