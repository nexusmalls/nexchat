// Copyright 2023-2025 Forecasting Technologies LTD.
// Copyright 2021-2022 Zeitgeist PM LLC.
//
// This file is part of Zeitgeist.
//
// Zeitgeist is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at
// your option) any later version.
//
// Zeitgeist is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Zeitgeist. If not, see <https://www.gnu.org/licenses/>.

#![cfg(test)]

use crate::{self as zrml_styx};
use frame_support::{
    construct_runtime, derive_impl, ord_parameter_types, parameter_types, traits::EitherOfDiverse,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use sp_runtime::BuildStorage;
use zeitgeist_primitives::{
    constants::mock::{BlockHashCount, ExistentialDeposit, MaxLocks, MaxReserves},
    types::Balance,
};

pub type AccountIdTest = u64;
pub type Block = frame_system::mocking::MockBlockU32<Runtime>;
pub const ALICE: AccountIdTest = 0;
pub const BOB: AccountIdTest = 1;
pub const CHARLIE: AccountIdTest = 2;
pub const SUDO: AccountIdTest = 1337;
pub const NEX: Balance = 1_000_000_000_000;

ord_parameter_types! {
    pub const Sudo: AccountIdTest = SUDO;
}

parameter_types! {
    pub const DefaultBurnAmount: Balance = 200 * NEX;
}

construct_runtime!(
    pub enum Runtime {
        Balances: pallet_balances,
        Styx: zrml_styx,
        System: frame_system,
    }
);

impl crate::Config for Runtime {
    type Currency = Balances;
    type DefaultBurnAmount = DefaultBurnAmount;
    type SetBurnAmountOrigin =
        EitherOfDiverse<EnsureRoot<AccountIdTest>, EnsureSignedBy<Sudo, AccountIdTest>>;
    type WeightInfo = zrml_styx::weights::WeightInfo<Runtime>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type AccountData = pallet_balances::AccountData<Balance>;
    type AccountId = AccountIdTest;
    type Block = Block;
    type BlockHashCount = BlockHashCount;
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

pub struct ExtBuilder {
    balances: Vec<(AccountIdTest, Balance)>,
}

impl Default for ExtBuilder {
    fn default() -> Self {
        Self {
            balances: vec![
                (ALICE, 1_000 * NEX),
                (BOB, 1_000 * NEX),
                (CHARLIE, 1_000 * NEX),
            ],
        }
    }
}

impl ExtBuilder {
    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .expect("frame-system genesis builds");

        // See logs with `RUST_LOG=debug cargo test -- --nocapture`.
        let _ = env_logger::builder().is_test(true).try_init();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balances,
            dev_accounts: None,
        }
        .assimilate_storage(&mut storage)
        .expect("balances genesis assimilates");

        storage.into()
    }
}
