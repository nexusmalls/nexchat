// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for the Hyper Fungible Token pallet.
//! Hyper Fungible Token pallet 的类型定义。

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use frame_support::traits::fungibles;
use ismp::host::StateMachine;
use polkadot_sdk::*;
use sp_core::{ConstU32, H160};

use crate::Config;

/// Maximum outbound callback payload accepted by the HFT call.
/// HFT 出站调用允许的最大 callback payload。
pub type MaxCallDataLen = ConstU32<4096>;

/// Maximum chain entries accepted by one registry call.
/// 单次 registry 调用允许的最大链条目数。
pub type MaxChainsPerCall = ConstU32<16>;

/// Local asset ID type alias.
/// 本地资产 ID 类型别名。
pub type AssetId<T> =
    <<T as Config>::Assets as fungibles::Inspect<<T as frame_system::Config>::AccountId>>::AssetId;

// ABI-compatible with Solidity:
// struct Message { bytes from; bytes to; uint256 amount; bytes data; }
alloy_sol_macro::sol! {
    #![sol(all_derives)]
    struct Message {
        bytes from;
        bytes to;
        uint256 amount;
        bytes data;
    }
}

/// Parameters for initiating a cross-chain token transfer.
/// 发起跨链 token 转移的参数。
#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct SendParams<AssetId, Balance> {
    pub asset_id: AssetId,
    pub destination: StateMachine,
    pub recipient: BoundedVec<u8, ConstU32<32>>,
    pub amount: Balance,
    pub timeout: u64,
    pub relayer_fee: Balance,
    pub call_data: Option<BoundedVec<u8, MaxCallDataLen>>,
}

/// Per-chain configuration for a registered token.
/// 已注册 token 的逐链配置。
#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct ChainConfig {
    pub token_contract: H160,
    pub decimals: u8,
}

/// Registration parameters for a new token.
/// 新 token 的注册参数。
#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct TokenRegistration<AssetId> {
    pub local_id: AssetId,
    pub native: bool,
    pub chains: BoundedBTreeMap<StateMachine, ChainConfig, MaxChainsPerCall>,
}

/// Parameters for updating an existing token.
/// 更新已有 token 的参数。
#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct TokenUpdate<AssetId> {
    pub asset_id: AssetId,
    pub add_chains: BoundedBTreeMap<StateMachine, ChainConfig, MaxChainsPerCall>,
    pub remove_chains: BoundedVec<StateMachine, MaxChainsPerCall>,
}

/// SCALE-encoded calldata for a destination runtime call.
/// 目标 runtime call 的 SCALE 编码 calldata。
#[derive(Debug, Clone, Encode, Decode, scale_info::TypeInfo, PartialEq, Eq)]
pub struct SubstrateCalldata {
    pub signature: Option<Vec<u8>>,
    pub runtime_call: Vec<u8>,
}

/// Converts an EVM address to a Substrate account.
/// 将 EVM 地址转换为 Substrate 账户。
pub trait EvmToSubstrate<T: frame_system::Config> {
    fn convert(addr: H160) -> T::AccountId;
}

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
