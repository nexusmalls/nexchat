// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Helper implementations for the Hyper Fungible Token pallet.
//! Hyper Fungible Token pallet 的辅助实现。

use alloc::string::ToString;
use polkadot_sdk::*;
use sp_core::U256;

use crate::{Config, Pallet, PALLET_ID};

impl<T: Config> Pallet<T> {
    /// Returns the pallet's custodial account for native assets.
    /// 返回 pallet 用于托管原生资产的账户。
    pub fn pallet_account() -> T::AccountId {
        use frame_support::PalletId;
        use sp_runtime::traits::AccountIdConversion;
        PalletId(*b"hft__acc").into_account_truncating()
    }

    pub fn is_module(id: &[u8]) -> bool {
        id == PALLET_ID.to_bytes()
    }
}

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

pub fn convert_to_erc20(value: u128, erc_decimals: u8, local_decimals: u8) -> U256 {
    U256::from(value) * U256::from(10u128.pow(erc_decimals.saturating_sub(local_decimals) as u32))
}
