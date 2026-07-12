// Copyright 2022-2025 Forecasting Technologies LTD.
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

use crate as zrml_market_commons;
use frame_support::{construct_runtime, derive_impl};
use frame_system::mocking::MockBlock;
use prediction_mock_runtime::{MockBaseAssetPolicy, USDX_ASSET_ID};
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use zeitgeist_primitives::traits::PredictionBaseAssetPolicy;
use zeitgeist_primitives::{
    constants::mock::{BlockHashCount, ExistentialDeposit, MaxLocks, MaxReserves, MinimumPeriod},
    types::{Balance, Hash, MarketId, Moment},
};

/// Mock account id for FRAME 45 unit tests.
pub type AccountIdTest = u64;

pub type Block = MockBlock<Runtime>;

construct_runtime!(
    pub enum Runtime {
        Balances: pallet_balances,
        MarketCommons: zrml_market_commons,
        System: frame_system,
        Timestamp: pallet_timestamp,
    }
);

impl crate::Config for Runtime {
    type Balance = Balance;
    type MarketId = MarketId;
    type Timestamp = Timestamp;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type AccountId = AccountIdTest;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
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

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {
    type Moment = Moment;
    type MinimumPeriod = MinimumPeriod;
}

#[derive(Default)]
pub struct ExtBuilder {}

impl ExtBuilder {
    pub fn build(self) -> sp_io::TestExternalities {
        let mut t = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();

        let _ = env_logger::builder().is_test(true).try_init();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: vec![],
            dev_accounts: None,
        }
        .assimilate_storage(&mut t)
        .unwrap();

        let _policy: bool = MockBaseAssetPolicy::is_allowed(USDX_ASSET_ID);

        t.into()
    }
}

// Keep Hash available for future mock extensions copied from upstream.
const _: () = {
    let _ = core::mem::size_of::<Hash>();
};
