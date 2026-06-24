// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Weight abstraction for `pallet-bridge-ismp`.
//! `pallet-bridge-ismp` 的权重抽象。
//!
//! Placeholder weights (constant DB reads/writes). Replace with benchmarked
//! values before mainnet (see HB-ASSET-01 DoD).
//! 占位权重（常量 DB 读写）。主网前需替换为基准测试值（见 HB-ASSET-01 DoD）。

use frame_support::weights::Weight;

pub trait WeightInfo {
	fn bridge_out() -> Weight;
	fn set_paused() -> Weight;
	fn set_limits() -> Weight;
	fn register_chain() -> Weight;
	fn deregister_chain() -> Weight;
}

impl WeightInfo for () {
	fn bridge_out() -> Weight {
		Weight::from_parts(50_000_000, 0)
	}
	fn set_paused() -> Weight {
		Weight::from_parts(10_000_000, 0)
	}
	fn set_limits() -> Weight {
		Weight::from_parts(10_000_000, 0)
	}
	fn register_chain() -> Weight {
		Weight::from_parts(20_000_000, 0)
	}
	fn deregister_chain() -> Weight {
		Weight::from_parts(20_000_000, 0)
	}
}
