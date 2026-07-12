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

//! Read-only runtime API for weighted swap pools.
//! 加权兑换池的只读 runtime API。

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Codec, MaxEncodedLen};
use sp_runtime::traits::{MaybeDisplay, MaybeFromStr};
use zeitgeist_primitives::types::{Asset, SerdeWrapper};

sp_api::decl_runtime_apis! {
    /// Exposes swap-pool identifiers, accounts, and spot prices to host clients.
    /// 向宿主客户端公开兑换池标识、账户和现货价格。
    pub trait SwapsApi<PoolId, AccountId, Balance, MarketId> where
        PoolId: Codec,
        AccountId: Codec,
        Balance: Codec + MaybeDisplay + MaybeFromStr + MaxEncodedLen,
        MarketId: Codec + MaxEncodedLen,
    {
        /// Returns the pool-share asset identifier.
        /// 返回池份额资产标识。
        fn pool_shares_id(pool_id: PoolId) -> Asset<SerdeWrapper<MarketId>>;

        /// Returns the deterministic account that owns a pool's liquidity.
        /// 返回持有池流动性的确定性账户。
        fn pool_account_id(pool_id: &PoolId) -> AccountId;

        /// Returns the current spot price for an asset pair.
        /// 返回资产对的当前现货价格。
        fn get_spot_price(
            pool_id: &PoolId,
            asset_in: &Asset<MarketId>,
            asset_out: &Asset<MarketId>,
            with_fees: bool,
        ) -> SerdeWrapper<Balance>;
    }
}
