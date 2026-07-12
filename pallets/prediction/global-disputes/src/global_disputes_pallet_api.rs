// Copyright 2022-2023, 2025 Forecasting Technologies LTD.
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

extern crate alloc;

use crate::types::InitialItem;
use sp_runtime::DispatchError;
use zeitgeist_primitives::types::OutcomeReport;

/// Initiates and resolves global disputes.
///
/// 启动并解决全局争议。
pub trait GlobalDisputesPalletApi<MarketId, AccountId, Balance, BlockNumber> {
    /// Returns the `AddOutcomePeriod` parameter.
    ///
    /// 返回 `AddOutcomePeriod` 参数。
    fn get_add_outcome_period() -> BlockNumber;

    /// Returns the `GdVotingPeriod` parameter.
    ///
    /// 返回 `GdVotingPeriod` 参数。
    fn get_vote_period() -> BlockNumber;

    /// Start a global dispute.
    ///
    /// 启动全局争议。
    ///
    /// # Arguments
    /// - `market_id` - The id of the market.
    /// - `initial_items` - The initial vote options (outcome, owner, amount)
    ///   to add to the global dispute. One initial item consists of the vote outcome,
    ///   the owner of the outcome who is rewarded in case of a win,
    ///   and the initial vote amount for this outcome.
    ///   It is required to add at least two unique outcomes.
    ///   In case of a duplicated outcome, the owner and amount is added to the pre-existing outcome.
    /// - `market_id`：市场标识。
    /// - `initial_items`：初始投票选项；每项包含结果、结果所有者和初始投票金额。
    ///   至少需要两个不同结果；重复结果的所有者和金额会合并到已有结果。
    fn start_global_dispute(
        market_id: &MarketId,
        initial_items: &[InitialItem<AccountId, Balance>],
    ) -> Result<u32, DispatchError>;

    /// Determine the winner of a global dispute.
    ///
    /// 确定全局争议的获胜结果。
    ///
    /// # Arguments
    /// - `market_id` - The id of the market.
    ///
    /// # Returns
    ///
    /// Returns the winning outcome.
    ///
    /// 返回获胜结果。
    fn determine_voting_winner(market_id: &MarketId) -> Option<OutcomeReport>;

    /// Checks whether a global dispute exists for the specified market.
    ///
    /// 检查指定市场是否存在全局争议。
    fn does_exist(market_id: &MarketId) -> bool;

    /// Check if global dispute is active.
    /// This call is useful to check if a global dispute is ready for a destruction.
    ///
    /// 检查全局争议是否处于活动状态；该结果也可用于判断争议是否可销毁。
    ///
    /// # Arguments
    /// - `market_id` - The id of the market.
    fn is_active(market_id: &MarketId) -> bool;

    /// Destroy a global dispute and allow to return all funds of the participants.
    ///
    /// 销毁全局争议，并允许返还参与者的全部资金。
    ///
    /// # Arguments
    /// - `market_id` - The id of the market.
    fn destroy_global_dispute(market_id: &MarketId) -> Result<(), DispatchError>;
}
