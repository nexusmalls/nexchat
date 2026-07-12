// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use alloc::fmt::Debug;
use frame_support::pallet_prelude::Weight;
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::Zero;
use zeitgeist_primitives::{traits::FutarchyOracle, types::BlockNumber};

#[cfg(feature = "fuzzing")]
use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};

#[derive(
    Clone, Debug, Decode, DecodeWithMemTracking, Encode, Eq, MaxEncodedLen, PartialEq, TypeInfo,
)]
pub struct MockOracle {
    weight: Weight,
    value: bool,
}

impl Default for MockOracle {
    fn default() -> Self {
        Self {
            weight: Default::default(),
            value: true,
        }
    }
}

impl MockOracle {
    pub fn new(weight: Weight, value: bool) -> Self {
        Self { weight, value }
    }
}

impl FutarchyOracle for MockOracle {
    type BlockNumber = BlockNumber;

    fn evaluate(&self) -> (Weight, bool) {
        (self.weight, self.value)
    }

    fn update(&mut self, _: Self::BlockNumber) -> Weight {
        Zero::zero()
    }
}

#[cfg(feature = "fuzzing")]
impl<'a> Arbitrary<'a> for MockOracle {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        let weight = Weight::from_parts(u64::arbitrary(u)?, u64::arbitrary(u)?);
        Ok(Self::new(weight, bool::arbitrary(u)?))
    }
}
