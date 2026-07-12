//! Phase 0 no-std dependency smoke checks for the prediction subsystem.
//! 预测市场子系统 Phase 0 的 no-std 依赖冒烟检查。

#![cfg_attr(not(feature = "std"), no_std)]

use orml_traits::NamedMultiReservableCurrency;
use zeitgeist_primitives::types::Asset;

/// Prediction asset type used to prove a single ORML/FRAME type graph.
/// 用于证明 ORML/FRAME 单一类型图的预测资产类型。
pub type PredictionAsset = Asset<u128>;

/// Compile-time contract required from the final prediction asset manager.
/// 最终预测资产管理器必须满足的编译期约束。
pub fn assert_asset_manager<AccountId, Manager>()
where
    Manager: NamedMultiReservableCurrency<
        AccountId,
        CurrencyId = PredictionAsset,
        Balance = u128,
        ReserveIdentifier = [u8; 8],
    >,
{
}

/// Links market commons into the same no-std dependency graph.
/// 将市场公共 pallet 纳入同一 no-std 依赖图。
pub fn assert_market_commons<T: zrml_market_commons::Config>() {}

#[cfg(test)]
mod tests;
