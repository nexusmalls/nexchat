// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Prediction-market base-asset admission boundary.
//! 预测市场基础资产准入边界。

/// Decides whether a foreign asset may back a prediction market.
/// 决定某个外部资产是否可作为预测市场抵押资产。
///
/// Native collateral is admitted by the consuming pallet. Implementations must
/// explicitly whitelist foreign asset identifiers; outcome and pool assets are
/// rejected before this policy is consulted.
/// 原生抵押品由消费该 trait 的 pallet 准入。实现必须显式列出允许的外部资产 ID；
/// outcome 与 pool 资产会在调用本策略前被拒绝。
pub trait PredictionBaseAssetPolicy<AssetId> {
    /// Returns whether `asset_id` is approved as foreign collateral.
    /// 返回 `asset_id` 是否已获准作为外部抵押品。
    fn is_allowed(asset_id: AssetId) -> bool;
}
