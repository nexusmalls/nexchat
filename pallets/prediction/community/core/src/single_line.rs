// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! SingleLine (global public queue) settlement for commission tickets (P4).
//! 分佣票公排（全局消费链）结算（P4）。

use crate::{
	Config, CurrencyBalanceOf, DirectCount, LifetimeTradingFee, Pallet, RemovedMembers,
	SingleLineIndex, SingleLineSegmentCount, SingleLineSegments,
};
use frame_support::{ensure, traits::Get, BoundedVec};
use pallet_prediction_community_common::{
	is_activated, lookup_tier_id, sl_effective_levels, sl_equal_split,
};
use sp_runtime::{
	traits::{Saturating, Zero},
	DispatchError, DispatchResult,
};

/// One SingleLine credit produced while settling a ticket.
/// 结算一张票时产生的一笔公排入账。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlCredit<AccountId, Balance> {
	pub beneficiary: AccountId,
	pub amount: Balance,
	pub level: u8,
	pub is_upline: bool,
}

impl<T: Config> Pallet<T> {
	/// Append `account` to the global SingleLine chain (idempotent).
	/// 将 `account` 追加到全局公排链（幂等）。
	pub(crate) fn add_to_single_line(account: &T::AccountId) -> DispatchResult {
		if SingleLineIndex::<T>::contains_key(account) {
			return Ok(());
		}

		let seg_size = T::MaxSingleLineLength::get();
		let seg_count = SingleLineSegmentCount::<T>::get();

		if seg_count > 0 {
			let last_seg_id = seg_count.saturating_sub(1);
			let mut seg = SingleLineSegments::<T>::get(last_seg_id);
			if (seg.len() as u32) < seg_size {
				let global_index = last_seg_id
					.saturating_mul(seg_size)
					.saturating_add(seg.len() as u32);
				seg.try_push(account.clone())
					.map_err(|_| DispatchError::Other("SegmentPushFailed"))?;
				SingleLineSegments::<T>::insert(last_seg_id, seg);
				SingleLineIndex::<T>::insert(account, global_index);
				return Ok(());
			}
		}

		let new_seg_id = seg_count;
		ensure!(
			new_seg_id < T::MaxSegmentCount::get(),
			crate::Error::<T>::MaxSegmentCountReached
		);
		let global_index = new_seg_id.saturating_mul(seg_size);
		let mut new_seg = BoundedVec::<T::AccountId, T::MaxSingleLineLength>::default();
		new_seg
			.try_push(account.clone())
			.map_err(|_| DispatchError::Other("SegmentPushFailed"))?;
		SingleLineSegments::<T>::insert(new_seg_id, new_seg);
		SingleLineSegmentCount::<T>::put(new_seg_id.saturating_add(1));
		SingleLineIndex::<T>::insert(account, global_index);
		Ok(())
	}

	pub(crate) fn single_line_length() -> u32 {
		let seg_count = SingleLineSegmentCount::<T>::get();
		if seg_count == 0 {
			return 0;
		}
		let seg_size = T::MaxSingleLineLength::get();
		let last = SingleLineSegments::<T>::get(seg_count.saturating_sub(1));
		seg_count
			.saturating_sub(1)
			.saturating_mul(seg_size)
			.saturating_add(last.len() as u32)
	}

	fn is_sl_skipped(account: &T::AccountId) -> bool {
		RemovedMembers::<T>::get(account)
			|| !is_activated(LifetimeTradingFee::<T>::get(account))
	}

	/// Walk upline/downline with equal-split budgets; unpaid slots stay in `remaining`.
	/// 上下线等额均分遍历；未发放层份额留在 `remaining`。
	pub(crate) fn process_single_line(
		payer: &T::AccountId,
		sl_budget: CurrencyBalanceOf<T>,
	) -> (alloc::vec::Vec<SlCredit<T::AccountId, CurrencyBalanceOf<T>>>, CurrencyBalanceOf<T>) {
		if sl_budget.is_zero() {
			return (alloc::vec::Vec::new(), Zero::zero());
		}

		let life = LifetimeTradingFee::<T>::get(payer);
		let tier = lookup_tier_id(life);
		let directs = DirectCount::<T>::get(payer);
		let (eff_up, eff_down) = sl_effective_levels(tier, directs);

		// Ensure activated payers sit on the global chain before walking.
		if is_activated(life) {
			let _ = Self::add_to_single_line(payer);
		}

		let (up_budget, down_budget, per_up, per_down) =
			sl_equal_split(sl_budget, eff_up, eff_down);

		let mut credits = alloc::vec::Vec::new();
		let mut remaining = CurrencyBalanceOf::<T>::zero();

		if eff_up == 0 {
			remaining = remaining.saturating_add(up_budget);
		} else {
			remaining = remaining.saturating_add(
				Self::walk_upline(payer, eff_up, per_up, up_budget, &mut credits),
			);
		}

		if eff_down == 0 {
			remaining = remaining.saturating_add(down_budget);
		} else {
			remaining = remaining.saturating_add(
				Self::walk_downline(payer, eff_down, per_down, down_budget, &mut credits),
			);
		}

		(credits, remaining)
	}

	fn walk_upline(
		payer: &T::AccountId,
		eff_up: u8,
		per_up: CurrencyBalanceOf<T>,
		up_budget: CurrencyBalanceOf<T>,
		credits: &mut alloc::vec::Vec<SlCredit<T::AccountId, CurrencyBalanceOf<T>>>,
	) -> CurrencyBalanceOf<T> {
		let Some(buyer_index) = SingleLineIndex::<T>::get(payer) else {
			return up_budget;
		};
		if buyer_index == 0 || per_up.is_zero() {
			return up_budget;
		}

		let seg_size = T::MaxSingleLineLength::get();
		let loop_max = eff_up as u32;
		let mut side_remaining = up_budget;
		let mut cur_seg_id = buyer_index / seg_size;
		let mut cur_seg = SingleLineSegments::<T>::get(cur_seg_id);

		for i in 1..=loop_max {
			if buyer_index < i || side_remaining.is_zero() {
				break;
			}
			let target_index = buyer_index - i;
			let seg_id = target_index / seg_size;
			if seg_id != cur_seg_id {
				cur_seg = SingleLineSegments::<T>::get(seg_id);
				cur_seg_id = seg_id;
			}
			let local_pos = (target_index % seg_size) as usize;
			let Some(upline) = cur_seg.get(local_pos) else {
				break;
			};

			if Self::is_sl_skipped(upline) {
				// Consume layer; share stays in side_remaining → pool.
				continue;
			}

			let actual = per_up.min(side_remaining);
			if actual.is_zero() {
				continue;
			}
			side_remaining = side_remaining.saturating_sub(actual);
			credits.push(SlCredit {
				beneficiary: upline.clone(),
				amount: actual,
				level: i as u8,
				is_upline: true,
			});
		}

		side_remaining
	}

	fn walk_downline(
		payer: &T::AccountId,
		eff_down: u8,
		per_down: CurrencyBalanceOf<T>,
		down_budget: CurrencyBalanceOf<T>,
		credits: &mut alloc::vec::Vec<SlCredit<T::AccountId, CurrencyBalanceOf<T>>>,
	) -> CurrencyBalanceOf<T> {
		let Some(buyer_index) = SingleLineIndex::<T>::get(payer) else {
			return down_budget;
		};
		let total_len = Self::single_line_length();
		if buyer_index >= total_len.saturating_sub(1) || per_down.is_zero() {
			return down_budget;
		}

		let seg_size = T::MaxSingleLineLength::get();
		let loop_max = eff_down as u32;
		let mut side_remaining = down_budget;
		let mut cur_seg_id = buyer_index / seg_size;
		let mut cur_seg = SingleLineSegments::<T>::get(cur_seg_id);

		for i in 1..=loop_max {
			if side_remaining.is_zero() {
				break;
			}
			let target_index = buyer_index.saturating_add(i);
			if target_index >= total_len {
				break;
			}
			let seg_id = target_index / seg_size;
			if seg_id != cur_seg_id {
				cur_seg = SingleLineSegments::<T>::get(seg_id);
				cur_seg_id = seg_id;
			}
			let local_pos = (target_index % seg_size) as usize;
			let Some(downline) = cur_seg.get(local_pos) else {
				break;
			};

			if Self::is_sl_skipped(downline) {
				continue;
			}

			let actual = per_down.min(side_remaining);
			if actual.is_zero() {
				continue;
			}
			side_remaining = side_remaining.saturating_sub(actual);
			credits.push(SlCredit {
				beneficiary: downline.clone(),
				amount: actual,
				level: i as u8,
				is_upline: false,
			});
		}

		side_remaining
	}
}
