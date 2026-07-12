// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::mock::runtime::{Balances, Futarchy, System};
use frame_support::traits::Hooks;
use zeitgeist_primitives::types::BlockNumber;

pub fn run_to_block(to: BlockNumber) {
    while System::block_number() < to {
        let now = System::block_number();
        Futarchy::on_finalize(now);
        Balances::on_finalize(now);
        System::on_finalize(now);
        let next = now + 1;
        System::set_block_number(next);
        System::on_initialize(next);
        Balances::on_initialize(next);
        Futarchy::on_initialize(next);
    }
}
