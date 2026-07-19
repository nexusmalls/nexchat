// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Weight abstraction for `pallet-bridge-ismp`.
//! `pallet-bridge-ismp` 的权重抽象。
//!
//! `SubstrateWeight` holds conservative DB-weight-based estimates; replace with
//! benchmarked values before mainnet (see HB-ASSET-01 DoD). Run:
//! `SubstrateWeight` 为保守的、基于 DB 权重的估计；主网前需用基准测试值替换
//!（见 HB-ASSET-01 DoD）。运行：
//! ```bash
//! cargo run --release --features runtime-benchmarks -- benchmark pallet \
//!   --chain dev --pallet pallet_bridge_ismp --extrinsic '*' \
//!   --steps 50 --repeat 20 \
//!   --output pallets/bridge/ismp/src/weights.rs
//! ```

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
    fn bridge_out() -> Weight;
    fn set_paused() -> Weight;
    fn set_limits() -> Weight;
    fn register_chain() -> Weight;
    fn deregister_chain() -> Weight;
    fn prune_payout_refunds(limit: u32) -> Weight;
    fn execute_withdraw() -> Weight;
    fn cancel_withdraw() -> Weight;
}

/// Conservative default weights (estimated; replace via benchmarking).
/// 保守的默认权重（估计值；通过基准测试替换）。
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Reads: Paused, PausedChain, Chains, Limits, DailyOut, account; writes:
    /// burn, BridgedOut, BridgedOutByChain, DailyOut, dispatch, event.
    /// 读：Paused、PausedChain、Chains、Limits、DailyOut、账户；写：销毁、BridgedOut、
    /// BridgedOutByChain、DailyOut、派发、事件。
    fn bridge_out() -> Weight {
        Weight::from_parts(80_000_000, 8_000)
            .saturating_add(T::DbWeight::get().reads(6))
            .saturating_add(T::DbWeight::get().writes(5))
    }
    fn set_paused() -> Weight {
        Weight::from_parts(12_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_limits() -> Weight {
        Weight::from_parts(12_000_000, 1_500).saturating_add(T::DbWeight::get().writes(1))
    }
    fn register_chain() -> Weight {
        Weight::from_parts(18_000_000, 2_000).saturating_add(T::DbWeight::get().writes(1))
    }
    fn deregister_chain() -> Weight {
        Weight::from_parts(18_000_000, 2_000).saturating_add(T::DbWeight::get().writes(1))
    }
    /// Visits up to `limit` entries (one read each) and removes at most `limit`
    /// (one write each). 最多检查 `limit` 个条目（各一次读）并删除至多 `limit` 个（各一次写）。
    fn prune_payout_refunds(limit: u32) -> Weight {
        Weight::from_parts(10_000_000, 0)
            .saturating_add(Weight::from_parts(2_000_000, 0).saturating_mul(limit.into()))
            .saturating_add(T::DbWeight::get().reads(limit.into()))
            .saturating_add(T::DbWeight::get().writes(limit.into()))
    }
    /// Loads the pending entry then runs the full outbound core (== `bridge_out`),
    /// plus removing the entry. 读取排队条目后执行完整出站核心（== `bridge_out`），再删除条目。
    fn execute_withdraw() -> Weight {
        Self::bridge_out()
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Takes (read + remove) one pending entry and emits an event.
    /// 取出（读 + 删）一个排队条目并发出事件。
    fn cancel_withdraw() -> Weight {
        Weight::from_parts(12_000_000, 1_500)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
}

/// Zero weights for unit tests.
/// 单元测试用零权重。
impl WeightInfo for () {
    fn bridge_out() -> Weight {
        Weight::zero()
    }
    fn set_paused() -> Weight {
        Weight::zero()
    }
    fn set_limits() -> Weight {
        Weight::zero()
    }
    fn register_chain() -> Weight {
        Weight::zero()
    }
    fn deregister_chain() -> Weight {
        Weight::zero()
    }
    fn prune_payout_refunds(_limit: u32) -> Weight {
        Weight::zero()
    }
    fn execute_withdraw() -> Weight {
        Weight::zero()
    }
    fn cancel_withdraw() -> Weight {
        Weight::zero()
    }
}
