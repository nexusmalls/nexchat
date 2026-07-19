// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Placeholder weights for prediction community core (Phase 7 will regenerate).
//! Prediction community core 占位权重（Phase 7 再生成）。

use frame_support::weights::Weight;

pub trait WeightInfo {
	fn register_community() -> Weight;
	fn bind_home_community() -> Weight;
	fn deposit_fee_allowance() -> Weight;
	fn withdraw_operator_pending() -> Weight;
	fn unbond_community() -> Weight;
	fn register() -> Weight;
	fn settle_multi_level(n: u32) -> Weight;
	fn settle_single_line(n: u32) -> Weight;
	fn withdraw_commission() -> Weight;
	fn claim_pool_reward() -> Weight;
	fn set_pool_reward_paused() -> Weight;
}

impl WeightInfo for () {
	fn register_community() -> Weight {
		Weight::from_parts(50_000_000, 0)
	}
	fn bind_home_community() -> Weight {
		Weight::from_parts(40_000_000, 0)
	}
	fn deposit_fee_allowance() -> Weight {
		Weight::from_parts(80_000_000, 0)
	}
	fn withdraw_operator_pending() -> Weight {
		Weight::from_parts(50_000_000, 0)
	}
	fn unbond_community() -> Weight {
		Weight::from_parts(60_000_000, 0)
	}
	fn register() -> Weight {
		Weight::from_parts(40_000_000, 0)
	}
	fn settle_multi_level(n: u32) -> Weight {
		Weight::from_parts(30_000_000u64.saturating_add(20_000_000u64.saturating_mul(n as u64)), 0)
	}
	fn settle_single_line(n: u32) -> Weight {
		Weight::from_parts(40_000_000u64.saturating_add(25_000_000u64.saturating_mul(n as u64)), 0)
	}
	fn withdraw_commission() -> Weight {
		Weight::from_parts(90_000_000, 0)
	}
	fn claim_pool_reward() -> Weight {
		Weight::from_parts(80_000_000, 0)
	}
	fn set_pool_reward_paused() -> Weight {
		Weight::from_parts(20_000_000, 0)
	}
}
