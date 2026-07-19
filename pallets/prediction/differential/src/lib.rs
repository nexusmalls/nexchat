//! Behavioral differential baseline harness for the Nexus prediction port.
//! Nexus 预测市场移植的行为差分基线测试框架。
//!
//! Scenarios execute on the isolated prediction-markets mock runtime and compare
//! normalized business snapshots against upstream-derived goldens. This crate
//! does not wire prediction pallets into the production runtime.
//! 场景在隔离的 prediction-markets mock runtime 上执行，并将归一化业务快照与
//! 上游导出的 golden 对比；不在生产 runtime 中接线 prediction pallet。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod compare;
pub mod normalize;
pub mod snapshot;

#[cfg(test)]
mod golden_capture;
#[cfg(test)]
mod goldens;
#[cfg(test)]
mod scenarios;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trading_goldens;
#[cfg(test)]
mod trading_scenarios;
