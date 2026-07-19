// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! MultiLevel (referral help) settlement for commission tickets (P3).
//! 分佣票动态助力（推荐链）结算（P3）。

use crate::{Config, CurrencyBalanceOf, LifetimeTradingFee, Members, Pallet};
use alloc::collections::BTreeSet;
use pallet_prediction_community_common::{
	is_activated, lookup_tier_id, max_help_levels, ml_layer_share, ML_LEVEL_WEIGHTS, ML_MAX_LEVELS,
};
use sp_runtime::traits::{Saturating, Zero};

/// One MultiLevel credit produced while settling a ticket.
/// 结算一张票时产生的一笔助力入账。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlCredit<AccountId, Balance> {
	pub beneficiary: AccountId,
	pub amount: Balance,
	pub level: u8,
}

impl<T: Config> Pallet<T>
where
	T::AccountId: Ord,
{
	/// Walk `payer → referrer → …` up to 15 layers; normalize PPT weights onto `ml_budget`.
	/// Skipped layers leave budget in `remaining` (caller keeps it in the unallocated pool).
	/// 沿 `payer → referrer → …` 最多 15 层；将 PPT 权重归一化到 `ml_budget`。
	/// 跳过层的预算留在 `remaining`（调用方保留在未分配沉淀中）。
	pub(crate) fn process_multi_level(
		payer: &T::AccountId,
		ml_budget: CurrencyBalanceOf<T>,
	) -> (alloc::vec::Vec<MlCredit<T::AccountId, CurrencyBalanceOf<T>>>, CurrencyBalanceOf<T>) {
		if ml_budget.is_zero() {
			return (alloc::vec::Vec::new(), Zero::zero());
		}

		let mut remaining = ml_budget;
		let mut credits = alloc::vec::Vec::new();
		let mut visited = BTreeSet::new();
		visited.insert(payer.clone());

		let mut current = Members::<T>::get(payer).and_then(|m| m.referrer);

		for (level_idx, _weight) in ML_LEVEL_WEIGHTS.iter().enumerate() {
			let level = (level_idx as u8).saturating_add(1);
			if level > ML_MAX_LEVELS {
				break;
			}
			if remaining.is_zero() {
				break;
			}

			let Some(referrer) = current else {
				break;
			};
			if visited.contains(&referrer) {
				break;
			}
			visited.insert(referrer.clone());

			let next = Members::<T>::get(&referrer).and_then(|m| m.referrer);

			// Must be a registered member.
			if !Members::<T>::contains_key(&referrer) {
				current = next;
				continue;
			}

			let life = LifetimeTradingFee::<T>::get(&referrer);
			if !is_activated(life) {
				current = next;
				continue;
			}

			let tier = lookup_tier_id(life);
			if max_help_levels(tier) < level {
				// Consumes chain hop; unpaid share stays in remaining → pool.
				current = next;
				continue;
			}

			let commission = ml_layer_share(ml_budget, level_idx);
			let actual = commission.min(remaining);
			if actual.is_zero() {
				current = next;
				continue;
			}

			remaining = remaining.saturating_sub(actual);
			credits.push(MlCredit {
				beneficiary: referrer.clone(),
				amount: actual,
				level,
			});
			current = next;
		}

		(credits, remaining)
	}
}
