// Copyright 2022-2025 Forecasting Technologies LTD.
// Copyright 2021-2022 Zeitgeist PM LLC.
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

#[cfg(feature = "runtime-benchmarks")]
use crate::traits::ZeitgeistAssetEnumerator;
use crate::{
    traits::PoolSharesId,
    types::{CategoryIndex, CombinatorialId, PoolId},
};
use parity_scale_codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

/// The `Asset` enum represents all types of assets available in the Zeitgeist
/// system.
/// `Asset` 枚举表示 Zeitgeist 系统中可用的全部资产类型。
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Default,
    Deserialize,
    Eq,
    Encode,
    MaxEncodedLen,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    TypeInfo,
)]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub enum Asset<MarketId> {
    CategoricalOutcome(MarketId, CategoryIndex),
    ScalarOutcome(MarketId, ScalarPosition),
    CombinatorialOutcomeLegacy, // Here to avoid having to migrate all holdings on the chain.
    PoolShare(PoolId),
    #[default]
    Ztg,
    /// Nexus foreign collateral id (`pallet-assets` `AssetId = u64`).
    /// Nexus 外部抵押资产 id（`pallet-assets` 的 `AssetId = u64`）。
    ForeignAsset(u64),
    ParimutuelShare(MarketId, CategoryIndex),
    CombinatorialToken(CombinatorialId),
}

#[cfg(feature = "runtime-benchmarks")]
impl<MarketId: MaxEncodedLen> ZeitgeistAssetEnumerator<MarketId> for Asset<MarketId> {
    fn create_asset_id(t: MarketId) -> Self {
        Asset::CategoricalOutcome(t, 0)
    }
}

impl<MarketId: MaxEncodedLen> PoolSharesId<PoolId> for Asset<MarketId> {
    fn pool_shares_id(pool_id: PoolId) -> Self {
        Self::PoolShare(pool_id)
    }
}

/// In a scalar market, users can either choose a `Long` position,
/// meaning that they think the outcome will be closer to the upper bound
/// or a `Short` position meaning that they think the outcome will be closer
/// to the lower bound.
/// 在标量市场中，用户可选择 `Long`（结果更接近上界）或 `Short`
///（结果更接近下界）头寸。
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Deserialize,
    Eq,
    Encode,
    MaxEncodedLen,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    TypeInfo,
)]
#[serde(rename_all = "camelCase")]
pub enum ScalarPosition {
    Long,
    Short,
}

/// Map an upstream Zeitgeist `ForeignAsset(u32)` id to the Nexus semantic `u64` value.
/// 将上游 Zeitgeist `ForeignAsset(u32)` id 映射为 Nexus 语义 `u64` 值。
///
/// Upstream ids always zero-extend; Nexus ids above `u32::MAX` are native-only.
/// 上游 id 始终零扩展；超过 `u32::MAX` 的 Nexus id 仅在本链使用。
#[inline]
pub fn foreign_asset_from_upstream_id(upstream_id: u32) -> u64 {
    upstream_id as u64
}

/// Build the Nexus `Asset::ForeignAsset` variant from an upstream `u32` fixture id.
/// 根据上游 `u32` fixture id 构造 Nexus `Asset::ForeignAsset` 变体。
#[inline]
pub fn foreign_asset_from_upstream<MarketId>(upstream_id: u32) -> Asset<MarketId> {
    Asset::ForeignAsset(foreign_asset_from_upstream_id(upstream_id))
}
