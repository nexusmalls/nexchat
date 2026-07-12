// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg(all(feature = "mock", test))]

mod submit_proposal;

use crate::{
    mock::{
        ext_builder::ExtBuilder,
        runtime::{Futarchy, Runtime, RuntimeOrigin, System},
        types::{MockOracle, MockScheduler},
        utility,
    },
    types::Proposal,
    Config, Error, Event, Proposals, ProposalsOf,
};
use frame_support::{
    assert_noop, assert_ok,
    dispatch::RawOrigin,
    traits::{schedule::DispatchTime, Bounded},
};
use sp_runtime::DispatchError;

pub(crate) struct Account {
    id: <Runtime as frame_system::Config>::AccountId,
}

impl Account {
    pub(crate) fn new(id: <Runtime as frame_system::Config>::AccountId) -> Self {
        Self { id }
    }

    pub(crate) fn signed(&self) -> RuntimeOrigin {
        RuntimeOrigin::signed(self.id)
    }
}
