// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

#![no_main]

use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};
use frame_system::pallet_prelude::{BlockNumberFor, OriginFor};
use libfuzzer_sys::fuzz_target;
use zrml_futarchy::{
    mock::{
        ext_builder::ExtBuilder,
        runtime::{Futarchy, Runtime, RuntimeOrigin},
    },
    types::Proposal,
};

#[derive(Debug)]
struct SubmitProposalParams {
    origin: OriginFor<Runtime>,
    duration: BlockNumberFor<Runtime>,
    proposal: Proposal<Runtime>,
}

impl<'a> Arbitrary<'a> for SubmitProposalParams {
    fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
        Ok(Self {
            origin: RuntimeOrigin::signed(u128::arbitrary(u)?),
            duration: Arbitrary::arbitrary(u)?,
            proposal: Arbitrary::arbitrary(u)?,
        })
    }
}

fuzz_target!(|params: SubmitProposalParams| {
    let mut ext = ExtBuilder::build();
    ext.execute_with(|| {
        let _ = Futarchy::submit_proposal(params.origin, params.duration, params.proposal);
    });
    let _ = ext.commit_all();
});
