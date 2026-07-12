// Copyright 2022-2025 Forecasting Technologies LTD.
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

use frame_support::pallet_prelude::{
    Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, TypeInfo,
};
use sp_runtime::traits::Saturating;
use zeitgeist_primitives::types::OutcomeReport;

/// The original voting outcome owner information.
///
/// 投票结果的原始所有权信息。
#[derive(
    Debug, TypeInfo, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, Clone, PartialEq, Eq,
)]
pub enum Possession<AccountId, Balance, OwnerInfo> {
    /// The outcome is owned by a single account.
    /// This happens due to the call to `add_vote_outcome`.
    ///
    /// 结果由单个账户所有；该状态由 `add_vote_outcome` 调用产生。
    Paid { owner: AccountId, fee: Balance },
    /// The outcome is owned by multiple accounts.
    /// When a global dispute is triggered, these are the owners of the initially added outcomes.
    ///
    /// 结果由多个账户共有；启动全局争议时，这些账户是初始结果的所有者。
    Shared { owners: OwnerInfo },
}

impl<AccountId, Balance, OwnerInfo> Possession<AccountId, Balance, OwnerInfo> {
    /// Returns the shared owners, or `None` for paid possession.
    ///
    /// 返回共有所有者；若为付费单一所有权则返回 `None`。
    pub fn get_shared_owners(self) -> Option<OwnerInfo> {
        match self {
            Possession::Shared { owners } => Some(owners),
            _ => None,
        }
    }
}

/// The information about a voting outcome of a global dispute.
///
/// 全局争议中某个投票结果的信息。
#[derive(
    Debug, TypeInfo, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, Clone, PartialEq, Eq,
)]
pub struct OutcomeInfo<AccountId, Balance, OwnerInfo> {
    /// The current sum of all locks on this outcome.
    ///
    /// 当前锁定在该结果上的金额总和。
    pub outcome_sum: Balance,
    /// The information about the owner(s) and optionally additional fee.
    ///
    /// 所有者信息及可选的附加费用。
    pub possession: Possession<AccountId, Balance, OwnerInfo>,
}

/// The general information about the global dispute.
///
/// 全局争议的通用信息。
#[derive(
    Debug, TypeInfo, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, Clone, PartialEq, Eq,
)]
pub struct GlobalDisputeInfo<AccountId, Balance, OwnerInfo, BlockNumber> {
    /// The outcome which is in the lead.
    ///
    /// 当前领先的结果。
    pub winner_outcome: OutcomeReport,
    /// The information about the winning outcome.
    ///
    /// 获胜结果的信息。
    pub outcome_info: OutcomeInfo<AccountId, Balance, OwnerInfo>,
    /// The current status of the global dispute.
    ///
    /// 全局争议的当前状态。
    pub status: GdStatus<BlockNumber>,
}

impl<AccountId, Balance: Saturating, OwnerInfo: Default, BlockNumber: Default>
    GlobalDisputeInfo<AccountId, Balance, OwnerInfo, BlockNumber>
{
    /// Creates dispute information with active periods initialized to defaults.
    ///
    /// 创建争议信息，并将活动期限初始化为默认值。
    pub fn new(
        outcome: OutcomeReport,
        possession: Possession<AccountId, Balance, OwnerInfo>,
        vote_sum: Balance,
    ) -> Self {
        let outcome_info = OutcomeInfo {
            outcome_sum: vote_sum,
            possession,
        };
        // `add_outcome_end` and `vote_end` gets set in `start_global_dispute`
        let status = GdStatus::Active {
            add_outcome_end: Default::default(),
            vote_end: Default::default(),
        };
        GlobalDisputeInfo {
            winner_outcome: outcome,
            status,
            outcome_info,
        }
    }

    /// Replaces the leading outcome and its accumulated vote amount.
    ///
    /// 替换领先结果及其累计投票金额。
    pub fn update_winner(&mut self, outcome: OutcomeReport, vote_sum: Balance) {
        self.winner_outcome = outcome;
        self.outcome_info.outcome_sum = vote_sum;
    }
}

/// The current status of the global dispute.
///
/// 全局争议的当前状态。
#[derive(
    TypeInfo, Debug, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen, Clone, PartialEq, Eq,
)]
pub enum GdStatus<BlockNumber> {
    /// The global dispute is in progress.
    /// The block number `add_outcome_end`, when the addition of new outcomes is over.
    /// The block number `vote_end`, when the global dispute voting period is over.
    ///
    /// 全局争议进行中；`add_outcome_end` 是停止新增结果的区块，
    /// `vote_end` 是投票期结束的区块。
    Active {
        add_outcome_end: BlockNumber,
        vote_end: BlockNumber,
    },
    /// The global dispute is finished.
    ///
    /// 全局争议已完成。
    Finished,
    /// The global dispute is destroyed.
    ///
    /// 全局争议已销毁。
    Destroyed,
}

/// An initial vote outcome item with the outcome owner and the initial vote amount.
///
/// 包含结果所有者和初始投票金额的初始结果项。
pub struct InitialItem<AccountId, Balance> {
    /// The outcome which is added as initial global dispute vote possibility.
    ///
    /// 作为全局争议初始投票选项加入的结果。
    pub outcome: OutcomeReport,
    /// The owner of the outcome. This account is rewarded in case the outcome is the winning one.
    ///
    /// 结果所有者；该结果获胜时此账户获得奖励。
    pub owner: AccountId,
    /// The vote amount at the start of the global dispute.
    ///
    /// 全局争议启动时的投票金额。
    pub amount: Balance,
}
