// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::mock::runtime::{Runtime, System};
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;

#[derive(Default)]
pub struct ExtBuilder;

impl ExtBuilder {
    pub fn build() -> TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();
        let _ = env_logger::builder().is_test(true).try_init();
        pallet_balances::GenesisConfig::<Runtime> {
            balances: vec![],
            dev_accounts: None,
        }
        .assimilate_storage(&mut storage)
        .unwrap();
        let mut ext: TestExternalities = storage.into();
        ext.execute_with(|| System::set_block_number(1));
        ext
    }
}
