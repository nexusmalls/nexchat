// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate as zrml_futarchy;
use crate::{
    mock::types::{MockOracle, MockScheduler},
    weights::WeightInfo,
};
use frame_support::{
    construct_runtime, derive_impl, ord_parameter_types, parameter_types,
    traits::{EitherOfDiverse, Everything},
};
use frame_system::{mocking::MockBlockU32, EnsureRoot, EnsureSignedBy};
use sp_runtime::traits::IdentityLookup;
use zeitgeist_primitives::{
    constants::mock::{BlockHashCount, ExistentialDeposit, MaxLocks, MaxReserves},
    types::{AccountIdTest, Balance, BlockNumber, Hash},
};

#[cfg(feature = "runtime-benchmarks")]
use crate::mock::types::MockBenchmarkHelper;

parameter_types! {
    pub const MaxProposals: u32 = 16;
    pub const MinDuration: BlockNumber = 10;
}

ord_parameter_types! {
    pub const TechnicalCommittee: AccountIdTest = 42;
}

construct_runtime! {
    pub enum Runtime {
        System: frame_system,
        Balances: pallet_balances,
        Futarchy: zrml_futarchy,
    }
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type AccountData = pallet_balances::AccountData<Balance>;
    type AccountId = AccountIdTest;
    type BaseCallFilter = Everything;
    type Block = MockBlockU32<Runtime>;
    type BlockHashCount = BlockHashCount;
    type Hash = Hash;
    type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ExistentialDeposit;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
}

impl zrml_futarchy::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = MockBenchmarkHelper;
    type MaxProposals = MaxProposals;
    type MinDuration = MinDuration;
    type Oracle = MockOracle;
    type RuntimeEvent = RuntimeEvent;
    type Scheduler = MockScheduler;
    type SubmitOrigin = EitherOfDiverse<
        EnsureRoot<AccountIdTest>,
        EnsureSignedBy<TechnicalCommittee, AccountIdTest>,
    >;
    type WeightInfo = WeightInfo<Runtime>;
}
