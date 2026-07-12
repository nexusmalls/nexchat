//! Shared mock runtime for Nexus prediction subsystem unit tests.
//! Nexus 预测子系统单元测试共享 mock runtime。
//!
//! Provides a minimal FRAME 45 runtime with ORML tokens/currencies, `pallet-assets`
//! collateral plumbing, and an explicit foreign-collateral whitelist policy.
//! 提供包含 ORML tokens/currencies、`pallet-assets` 抵押 plumbing
//! 以及显式外部抵押白名单策略的最小 FRAME 45 runtime。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod base_asset;
mod runtime;

pub use base_asset::{MockBaseAssetPolicy, USDX_ASSET_ID};
pub use runtime::{
    AccountIdOf, Assets, Balances, Block, ExtBuilder, PredictionCurrencies, PredictionTokens,
    Runtime, RuntimeEvent, RuntimeOrigin, System, Timestamp, ALICE, BASE, CENT, INITIAL_BALANCE,
    SUDO,
};
pub use zeitgeist_primitives::types::{Amount, Balance, Moment};
