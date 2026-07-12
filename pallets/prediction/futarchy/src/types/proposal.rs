// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{BoundedCallOf, Config, OracleOf};
use frame_support::{CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use frame_system::pallet_prelude::BlockNumberFor;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[cfg(feature = "fuzzing")]
use {
    arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured},
    frame_support::traits::Bounded,
    sp_core::H256,
    sp_runtime::traits::Hash,
};

/// A scheduled call and the oracle that decides whether it may execute.
/// 待调度调用及决定其是否可执行的预言机。
#[derive(
    CloneNoBound,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEqNoBound,
    RuntimeDebugNoBound,
    TypeInfo,
)]
#[scale_info(skip_type_params(S, T))]
pub struct Proposal<T: Config> {
    /// Execution block. / 执行区块。
    pub when: BlockNumberFor<T>,
    /// Proposed bounded runtime call. / 提议的有界 Runtime 调用。
    pub call: BoundedCallOf<T>,
    /// Oracle deciding enactment. / 决定是否执行的预言机。
    pub oracle: OracleOf<T>,
}

#[cfg(feature = "fuzzing")]
impl<'a, T> Arbitrary<'a> for Proposal<T>
where
    OracleOf<T>: Arbitrary<'a>,
    T: Config,
{
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        let when = u32::arbitrary(u)?.into();
        let raw: [u8; 32] = Arbitrary::arbitrary(u)?;
        let hash = H256(raw);
        let frame_system_hash = <T as frame_system::Config>::Hashing::hash_of(&hash);
        let len = u32::arbitrary(u)?;
        let call = Bounded::Lookup {
            hash: frame_system_hash,
            len,
        };
        let oracle = Arbitrary::arbitrary(u)?;
        Ok(Proposal { when, call, oracle })
    }
}
