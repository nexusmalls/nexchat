// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Mock runtime for prediction-control tests.
//! Prediction-control 测试使用的 mock runtime。

use crate as pallet_prediction_control;
use frame_support::{construct_runtime, derive_impl};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

pub type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
    pub enum Test {
        System: frame_system,
        PredictionControl: pallet_prediction_control,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
}

impl pallet_prediction_control::Config for Test {
    type UpdateOrigin = frame_system::EnsureRoot<AccountId>;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system genesis builds");
    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
