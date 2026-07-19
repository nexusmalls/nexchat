// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Pool reward rounds: P5–P7 tier pots with periodic claim (P5).
//! 沉淀轮次：P5–P7 档位份额与周期性领取（P5）。

use crate::{
	Config, CurrencyBalanceOf, CurrentPoolRound, Error, Event, LastClaimedPoolRound,
	LifetimeTradingFee, Members, Pallet, PoolRewardPaused, PoolRoundInfo, RemovedMembers,
	TierMemberCount, UnallocatedPool,
};
use frame_support::{
	ensure,
	traits::{ExistenceRequirement, Get},
};
use frame_system::pallet_prelude::BlockNumberFor;
use orml_traits::MultiCurrency;
use pallet_prediction_community_common::{
	is_pool_eligible_tier, lookup_tier_id, pool_tier_pot,
};
use sp_runtime::traits::{Saturating, Zero};

impl<T: Config> Pallet<T> {
	/// Ensure a current round exists; rotate when duration elapsed.
	/// 确保存在当期轮次；超时则开启新轮。
	pub(crate) fn ensure_current_pool_round(
	) -> Result<PoolRoundInfo<CurrencyBalanceOf<T>, BlockNumberFor<T>>, sp_runtime::DispatchError>
	{
		let now = <frame_system::Pallet<T>>::block_number();
		let duration = T::PoolRoundDuration::get();
		if let Some(info) = CurrentPoolRound::<T>::get() {
			if now < info.start_block.saturating_add(duration) {
				return Ok(info);
			}
		}
		Self::create_new_pool_round(now)
	}

	pub(crate) fn create_new_pool_round(
		now: BlockNumberFor<T>,
	) -> Result<PoolRoundInfo<CurrencyBalanceOf<T>, BlockNumberFor<T>>, sp_runtime::DispatchError>
	{
		let pool_snapshot = UnallocatedPool::<T>::get();
		let pot = pool_tier_pot(pool_snapshot);
		let p5_count = TierMemberCount::<T>::get(5);
		let p6_count = TierMemberCount::<T>::get(6);
		let p7_count = TierMemberCount::<T>::get(7);

		let p5_per = if p5_count == 0 {
			Zero::zero()
		} else {
			pot / <CurrencyBalanceOf<T>>::from(p5_count)
		};
		let p6_per = if p6_count == 0 {
			Zero::zero()
		} else {
			pot / <CurrencyBalanceOf<T>>::from(p6_count)
		};
		let p7_per = if p7_count == 0 {
			Zero::zero()
		} else {
			pot / <CurrencyBalanceOf<T>>::from(p7_count)
		};

		let prev_id = CurrentPoolRound::<T>::get().map(|r| r.round_id).unwrap_or(0);
		let round_id = prev_id.saturating_add(1);
		let info = PoolRoundInfo {
			round_id,
			start_block: now,
			pool_snapshot,
			p5_count,
			p5_per_member: p5_per,
			p5_claimed: 0,
			p6_count,
			p6_per_member: p6_per,
			p6_claimed: 0,
			p7_count,
			p7_per_member: p7_per,
			p7_claimed: 0,
		};
		CurrentPoolRound::<T>::put(info.clone());
		Self::deposit_event(Event::NewPoolRoundStarted {
			round_id,
			pool_snapshot,
			p5_count,
			p6_count,
			p7_count,
		});
		Ok(info)
	}

	/// Claim current round pool reward for a P5–P7 member.
	/// P5–P7 会员领取当期沉淀。
	pub(crate) fn do_claim_pool_reward(who: &T::AccountId) -> sp_runtime::DispatchResult {
		ensure!(!PoolRewardPaused::<T>::get(), Error::<T>::PoolRewardPaused);
		ensure!(Members::<T>::contains_key(who), Error::<T>::NotMember);
		ensure!(!RemovedMembers::<T>::get(who), Error::<T>::MemberRemoved);

		let tier = lookup_tier_id(LifetimeTradingFee::<T>::get(who));
		ensure!(is_pool_eligible_tier(tier), Error::<T>::TierNotEligible);

		let mut info = Self::ensure_current_pool_round()?;
		let last = LastClaimedPoolRound::<T>::get(who);
		ensure!(last < info.round_id, Error::<T>::AlreadyClaimedPool);

		let (per_member, claimed, count) = match tier {
			5 => (info.p5_per_member, info.p5_claimed, info.p5_count),
			6 => (info.p6_per_member, info.p6_claimed, info.p6_count),
			7 => (info.p7_per_member, info.p7_claimed, info.p7_count),
			_ => return Err(Error::<T>::TierNotEligible.into()),
		};
		ensure!(!per_member.is_zero(), Error::<T>::NothingToClaim);
		ensure!(claimed < count, Error::<T>::PoolTierExhausted);

		UnallocatedPool::<T>::try_mutate(|p| -> sp_runtime::DispatchResult {
			ensure!(*p >= per_member, Error::<T>::InsufficientPool);
			*p = p.saturating_sub(per_member);
			Ok(())
		})?;

		let vault = Self::vault_account();
		let asset = T::CommunityAsset::get();
		T::MultiCurrency::transfer(
			asset,
			&vault,
			who,
			per_member,
			ExistenceRequirement::AllowDeath,
		)
		.map_err(|_| Error::<T>::UsdxTransferFailed)?;

		match tier {
			5 => info.p5_claimed = info.p5_claimed.saturating_add(1),
			6 => info.p6_claimed = info.p6_claimed.saturating_add(1),
			7 => info.p7_claimed = info.p7_claimed.saturating_add(1),
			_ => {}
		}
		CurrentPoolRound::<T>::put(info.clone());
		LastClaimedPoolRound::<T>::insert(who, info.round_id);
		Self::deposit_event(Event::PoolRewardClaimed {
			who: who.clone(),
			round_id: info.round_id,
			tier_id: tier,
			amount: per_member,
		});
		Ok(())
	}

	/// Maintain `TierMemberCount` when lifetime trading fee changes.
	/// `lifetime_trading_fee` 变化时维护档位人数计数。
	pub(crate) fn note_tier_change(
		who: &T::AccountId,
		before: CurrencyBalanceOf<T>,
		after: CurrencyBalanceOf<T>,
	) {
		if !Members::<T>::contains_key(who) || RemovedMembers::<T>::get(who) {
			return;
		}
		let old_tier = lookup_tier_id(before);
		let new_tier = lookup_tier_id(after);
		if old_tier == new_tier {
			return;
		}
		if is_pool_eligible_tier(old_tier) {
			TierMemberCount::<T>::mutate(old_tier, |c| *c = c.saturating_sub(1));
		}
		if is_pool_eligible_tier(new_tier) {
			TierMemberCount::<T>::mutate(new_tier, |c| *c = c.saturating_add(1));
		}
	}
}
