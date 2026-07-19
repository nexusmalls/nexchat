// Copyright 2024-2025 Forecasting Technologies LTD.
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

use crate::mock::runtime::{
    Assets, PredictionCollateral, PredictionControl, Runtime, RuntimeOrigin, System,
};
use frame_support::{assert_ok, traits::fungibles::Mutate};
use pallet_prediction_control::PredictionMode;
use prediction_mock_runtime::USDX_ASSET_ID;
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;
use zeitgeist_primitives::{
    constants::BASE,
    types::{AccountIdTest, Balance},
};

pub const INITIAL_FOREIGN_BALANCE: Balance = 1_000 * BASE;
pub const USDX_MIN_BALANCE: Balance = 1;

pub struct ExtBuilder;

impl ExtBuilder {
    pub fn build() -> TestExternalities {
        let mut t = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();

        // See the logs in tests when using `RUST_LOG=debug cargo test -- --nocapture`
        let _ = env_logger::builder().is_test(true).try_init();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: vec![],
            dev_accounts: None,
        }
        .assimilate_storage(&mut t)
        .unwrap();

        let mut test_ext: sp_io::TestExternalities = t.into();

        test_ext.execute_with(|| {
            System::set_block_number(1);
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                99,
                true,
                USDX_MIN_BALANCE,
            ));
            for account in 0..10 {
                assert_ok!(<Assets as Mutate<AccountIdTest>>::mint_into(
                    USDX_ASSET_ID,
                    &account,
                    INITIAL_FOREIGN_BALANCE + USDX_MIN_BALANCE,
                ));
            }
            assert_ok!(PredictionControl::set_prediction_mode(
                RuntimeOrigin::root(),
                PredictionMode::Full,
            ));
            assert_ok!(PredictionCollateral::set_asset_whitelisted(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                true,
            ));
            for account in 0..10 {
                assert_ok!(PredictionCollateral::deposit(
                    RuntimeOrigin::signed(account),
                    USDX_ASSET_ID,
                    INITIAL_FOREIGN_BALANCE,
                ));
            }
            System::reset_events();
        });

        test_ext
    }
}
