// Copyright 2024-2025 Forecasting Technologies LTD.
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

//! Read-only runtime API types for prediction market outcome assets.
//! 预测市场 outcome 资产的只读 runtime API 类型。

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Codec, MaxEncodedLen};
use zeitgeist_primitives::types::Asset;

sp_api::decl_runtime_apis! {
    /// Exposes deterministic prediction-market asset identifiers to clients.
    /// 向客户端公开确定性的预测市场资产标识。
    pub trait PredictionMarketsApi<MarketId, Hash> where
        MarketId: Codec + MaxEncodedLen,
        Hash: Codec,
    {
        /// Returns the asset identifier for one market outcome.
        /// 返回指定市场 outcome 的资产标识。
        fn market_outcome_share_id(market_id: MarketId, outcome: u16) -> Asset<MarketId>;
    }
}
