// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

use crate::{
	mock::{
		new_test_ext, usdx_free, CommunityCore, RuntimeOrigin, System, ALICE, BOB, CHARLIE, DAVE,
		NEX, TREASURY, USDX,
	},
	CommissionTickets, DirectCount, Error, FeeAllowance, LifetimeTradingFee, MemberPending,
	OperatorPending, RegisteredCommunities, SingleLineIndex, TierMemberCount, UnallocatedPool,
};
use frame_support::{assert_noop, assert_ok};
use orml_traits::MultiCurrency;
use pallet_prediction_community_common::{
	ml_layer_share, pool_tier_pot, sl_equal_split, split_commission_budget, withdraw_split_by_tier,
};
use sp_runtime::{BoundedVec, Perbill};

fn set_lifetime(who: u64, amount: u128) {
	let before = LifetimeTradingFee::<crate::mock::Test>::get(who);
	LifetimeTradingFee::<crate::mock::Test>::insert(who, amount);
	CommunityCore::note_tier_change(&who, before, amount);
}

fn fund_vault(amount: u128) {
	let vault = CommunityCore::vault_account();
	assert_ok!(<orml_tokens::Pallet<crate::mock::Test> as MultiCurrency<u64>>::deposit(
		USDX, &vault, amount
	));
}

#[test]
fn register_community_locks_bond() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register_community(RuntimeOrigin::signed(ALICE)));
		assert!(RegisteredCommunities::<crate::mock::Test>::contains_key(ALICE));
		assert_noop!(
			CommunityCore::register_community(RuntimeOrigin::signed(ALICE)),
			Error::<crate::mock::Test>::AlreadyRegistered
		);
	});
}

#[test]
fn deposit_splits_and_increases_allowance() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register_community(RuntimeOrigin::signed(BOB)));
		assert_ok!(CommunityCore::bind_home_community(RuntimeOrigin::signed(ALICE), BOB));
		let amount = 50u128;
		let alice_before = usdx_free(ALICE);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		assert_eq!(usdx_free(ALICE), alice_before - amount);
		assert_eq!(FeeAllowance::<crate::mock::Test>::get(ALICE), amount);
		assert_eq!(LifetimeTradingFee::<crate::mock::Test>::get(ALICE), 0);
		let (_sl, _ml, op) = split_commission_budget(amount);
		assert_eq!(OperatorPending::<crate::mock::Test>::get(BOB), op);
	});
}

#[test]
fn deposit_unbound_operator_share_to_treasury() {
	new_test_ext().execute_with(|| {
		let amount = 100u128;
		let (_sl, _ml, op) = split_commission_budget(amount);
		let treasury_before = usdx_free(TREASURY);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		assert_eq!(usdx_free(TREASURY), treasury_before + op);
	});
}

#[test]
fn trade_path_a_uses_allowance() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register_community(RuntimeOrigin::signed(BOB)));
		assert_ok!(CommunityCore::bind_home_community(RuntimeOrigin::signed(ALICE), BOB));
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			50
		));
		let notional = 1000u128;
		let creator_fee = Perbill::from_perthousand(5); // 0.5%
		let alice_before = usdx_free(ALICE);
		let creator_before = usdx_free(CHARLIE);
		let treasury_before = usdx_free(TREASURY);

		System::set_extrinsic_index(1);
		let taken = CommunityCore::apply_usdx_trade_fee(&ALICE, notional, &CHARLIE, creator_fee);
		assert_eq!(taken, 10); // top bar
		assert_eq!(usdx_free(ALICE), alice_before - 10);
		assert_eq!(usdx_free(CHARLIE), creator_before + 5);
		assert_eq!(usdx_free(TREASURY), treasury_before + 5);
		assert_eq!(FeeAllowance::<crate::mock::Test>::get(ALICE), 30);
		assert_eq!(LifetimeTradingFee::<crate::mock::Test>::get(ALICE), 20);

		// D19: second call same extrinsic returns 0
		let taken2 = CommunityCore::apply_usdx_trade_fee(&ALICE, notional, &CHARLIE, creator_fee);
		assert_eq!(taken2, 0);
		assert_eq!(FeeAllowance::<crate::mock::Test>::get(ALICE), 30);
	});
}

#[test]
fn trade_path_b_without_allowance() {
	new_test_ext().execute_with(|| {
		let notional = 1000u128;
		let creator_fee = Perbill::from_perthousand(5);
		let alice_before = usdx_free(ALICE);
		System::set_extrinsic_index(2);
		let taken = CommunityCore::apply_usdx_trade_fee(&ALICE, notional, &CHARLIE, creator_fee);
		assert_eq!(taken, 30);
		assert_eq!(usdx_free(ALICE), alice_before - 30);
		assert_eq!(LifetimeTradingFee::<crate::mock::Test>::get(ALICE), 20);
		assert_eq!(FeeAllowance::<crate::mock::Test>::get(ALICE), 0);
	});
}

#[test]
fn insufficient_bond_fails_register() {
	new_test_ext().execute_with(|| {
		// Drain charlie's NEX below bond.
		use frame_support::traits::Currency;
		let _ = <pallet_balances::Pallet<crate::mock::Test> as Currency<u64>>::make_free_balance_be(
			&CHARLIE,
			100 * NEX,
		);
		assert_noop!(
			CommunityCore::register_community(RuntimeOrigin::signed(CHARLIE)),
			Error::<crate::mock::Test>::InsufficientBond
		);
	});
}

#[test]
fn register_member_binds_referrer() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), Some(BOB)));
		assert_noop!(
			CommunityCore::register(RuntimeOrigin::signed(ALICE), Some(BOB)),
			Error::<crate::mock::Test>::AlreadyMember
		);
		assert_noop!(
			CommunityCore::register(RuntimeOrigin::signed(CHARLIE), Some(DAVE)),
			Error::<crate::mock::Test>::InvalidReferrer
		);
	});
}

#[test]
fn settle_multi_level_credits_activated_upline() {
	new_test_ext().execute_with(|| {
		// Chain: Alice → Bob → Charlie (Charlie is L2 for Alice's ticket).
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(CHARLIE), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), Some(CHARLIE)));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), Some(BOB)));

		// Activate Bob + Charlie (lifetime ≥ P1).
		LifetimeTradingFee::<crate::mock::Test>::insert(BOB, 50u128);
		LifetimeTradingFee::<crate::mock::Test>::insert(CHARLIE, 50u128);

		assert_ok!(CommunityCore::register_community(RuntimeOrigin::signed(DAVE)));
		assert_ok!(CommunityCore::bind_home_community(RuntimeOrigin::signed(ALICE), DAVE));

		let amount = 100u128;
		let (_sl, ml, _op) = split_commission_budget(amount);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		let ticket_id = 0u64;
		let pool_before = UnallocatedPool::<crate::mock::Test>::get();

		let ids = BoundedVec::<u64, crate::mock::MaxSettleBatch>::try_from(vec![ticket_id]).unwrap();
		assert_ok!(CommunityCore::settle_multi_level(RuntimeOrigin::signed(ALICE), ids));

		let l1 = ml_layer_share(ml, 0);
		let l2 = ml_layer_share(ml, 1);
		assert_eq!(MemberPending::<crate::mock::Test>::get(BOB), l1);
		assert_eq!(MemberPending::<crate::mock::Test>::get(CHARLIE), l2);
		assert_eq!(
			UnallocatedPool::<crate::mock::Test>::get(),
			pool_before - l1 - l2
		);
		assert!(CommissionTickets::<crate::mock::Test>::get(ticket_id)
			.unwrap()
			.ml_settled);
		assert!(!CommissionTickets::<crate::mock::Test>::get(ticket_id)
			.unwrap()
			.sl_settled);
	});
}

#[test]
fn settle_multi_level_skips_inactive_upline() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), Some(BOB)));
		// Bob not activated.

		let amount = 100u128;
		let (_sl, ml, _op) = split_commission_budget(amount);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		let pool_before = UnallocatedPool::<crate::mock::Test>::get();
		let ids = BoundedVec::<u64, crate::mock::MaxSettleBatch>::try_from(vec![0u64]).unwrap();
		assert_ok!(CommunityCore::settle_multi_level(RuntimeOrigin::signed(ALICE), ids));

		assert_eq!(MemberPending::<crate::mock::Test>::get(BOB), 0);
		// Entire ML budget remains in unallocated pool.
		assert_eq!(UnallocatedPool::<crate::mock::Test>::get(), pool_before);
		let _ = ml;
	});
}

#[test]
fn first_p1_activation_increments_referrer_directs() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), Some(BOB)));
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			50
		));
		assert_eq!(DirectCount::<crate::mock::Test>::get(BOB), 0);

		System::set_extrinsic_index(1);
		// Path A: commission_fee = 20 per notional 1000; need ≥50 → multiple trades.
		let creator_fee = Perbill::from_perthousand(5);
		let _ = CommunityCore::apply_usdx_trade_fee(&ALICE, 1000, &CHARLIE, creator_fee);
		assert_eq!(LifetimeTradingFee::<crate::mock::Test>::get(ALICE), 20);
		assert_eq!(DirectCount::<crate::mock::Test>::get(BOB), 0);

		System::set_extrinsic_index(2);
		let _ = CommunityCore::apply_usdx_trade_fee(&ALICE, 1000, &CHARLIE, creator_fee);
		System::set_extrinsic_index(3);
		let _ = CommunityCore::apply_usdx_trade_fee(&ALICE, 500, &CHARLIE, creator_fee);
		// 20+20+10 = 50 → P1
		assert_eq!(LifetimeTradingFee::<crate::mock::Test>::get(ALICE), 50);
		assert_eq!(DirectCount::<crate::mock::Test>::get(BOB), 1);
		assert!(SingleLineIndex::<crate::mock::Test>::contains_key(ALICE));
	});
}

#[test]
fn settle_single_line_equal_split_up_and_down() {
	new_test_ext().execute_with(|| {
		// Chain order: Bob (0) → Alice (1) → Charlie (2)
		for who in [BOB, ALICE, CHARLIE] {
			assert_ok!(CommunityCore::register(RuntimeOrigin::signed(who), None));
			LifetimeTradingFee::<crate::mock::Test>::insert(who, 50u128);
			assert_ok!(CommunityCore::add_to_single_line(&who));
		}
		assert_eq!(SingleLineIndex::<crate::mock::Test>::get(BOB), Some(0));
		assert_eq!(SingleLineIndex::<crate::mock::Test>::get(ALICE), Some(1));
		assert_eq!(SingleLineIndex::<crate::mock::Test>::get(CHARLIE), Some(2));

		let amount = 100u128;
		let (sl, _ml, _op) = split_commission_budget(amount);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		let pool_before = UnallocatedPool::<crate::mock::Test>::get();
		let (_up, _down, per_up, per_down) = sl_equal_split(sl, 20, 30);

		let ids = BoundedVec::<u64, crate::mock::MaxSettleBatch>::try_from(vec![0u64]).unwrap();
		assert_ok!(CommunityCore::settle_single_line(RuntimeOrigin::signed(ALICE), ids));

		assert_eq!(MemberPending::<crate::mock::Test>::get(BOB), per_up);
		assert_eq!(MemberPending::<crate::mock::Test>::get(CHARLIE), per_down);
		assert_eq!(
			UnallocatedPool::<crate::mock::Test>::get(),
			pool_before - per_up - per_down
		);
		assert!(CommissionTickets::<crate::mock::Test>::get(0).unwrap().sl_settled);
	});
}

#[test]
fn settle_single_line_d2_blocks_downline_without_directs() {
	new_test_ext().execute_with(|| {
		for who in [BOB, ALICE, CHARLIE] {
			assert_ok!(CommunityCore::register(RuntimeOrigin::signed(who), None));
			// P5 threshold = 500
			LifetimeTradingFee::<crate::mock::Test>::insert(who, 500u128);
			assert_ok!(CommunityCore::add_to_single_line(&who));
		}
		// Alice P5 but DirectCount = 0 → effective_down = 0
		assert_eq!(DirectCount::<crate::mock::Test>::get(ALICE), 0);

		let amount = 100u128;
		let (sl, _ml, _op) = split_commission_budget(amount);
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		let (_up, down, per_up, _per_down) = sl_equal_split(sl, 40, 0);

		let ids = BoundedVec::<u64, crate::mock::MaxSettleBatch>::try_from(vec![0u64]).unwrap();
		assert_ok!(CommunityCore::settle_single_line(RuntimeOrigin::signed(ALICE), ids));

		assert_eq!(MemberPending::<crate::mock::Test>::get(BOB), per_up);
		assert_eq!(MemberPending::<crate::mock::Test>::get(CHARLIE), 0);
		// Whole down half stayed in pool (not credited).
		let _ = down;
	});
}

#[test]
fn settle_single_line_skips_inactive_neighbor() {
	new_test_ext().execute_with(|| {
		for who in [BOB, ALICE, CHARLIE] {
			assert_ok!(CommunityCore::register(RuntimeOrigin::signed(who), None));
			assert_ok!(CommunityCore::add_to_single_line(&who));
		}
		// Only Alice activated; Bob/Charlie inactive → skipped, no credits.
		LifetimeTradingFee::<crate::mock::Test>::insert(ALICE, 50u128);

		let amount = 100u128;
		assert_ok!(CommunityCore::deposit_fee_allowance(
			RuntimeOrigin::signed(ALICE),
			amount
		));
		let pool_before = UnallocatedPool::<crate::mock::Test>::get();
		let ids = BoundedVec::<u64, crate::mock::MaxSettleBatch>::try_from(vec![0u64]).unwrap();
		assert_ok!(CommunityCore::settle_single_line(RuntimeOrigin::signed(ALICE), ids));

		assert_eq!(MemberPending::<crate::mock::Test>::get(BOB), 0);
		assert_eq!(MemberPending::<crate::mock::Test>::get(CHARLIE), 0);
		assert_eq!(UnallocatedPool::<crate::mock::Test>::get(), pool_before);
	});
}

#[test]
fn withdraw_commission_splits_cash_and_reinvest() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), None));
		set_lifetime(ALICE, 500); // P5 → 50/50
		fund_vault(1_000);
		MemberPending::<crate::mock::Test>::insert(ALICE, 100);
		let allowance_before = FeeAllowance::<crate::mock::Test>::get(ALICE);
		let alice_usdx = usdx_free(ALICE);
		let (cash, reinvest) = withdraw_split_by_tier(5u8, 100u128);

		assert_ok!(CommunityCore::withdraw_commission(
			RuntimeOrigin::signed(ALICE),
			100
		));
		assert_eq!(MemberPending::<crate::mock::Test>::get(ALICE), 0);
		assert_eq!(usdx_free(ALICE), alice_usdx + cash);
		assert_eq!(
			FeeAllowance::<crate::mock::Test>::get(ALICE),
			allowance_before + reinvest
		);
		// Reinvest enqueued a new ticket (id 0).
		assert!(CommissionTickets::<crate::mock::Test>::contains_key(0));
	});
}

#[test]
fn claim_pool_reward_p5_only_and_once_per_round() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), None));
		set_lifetime(ALICE, 500); // P5
		set_lifetime(BOB, 50); // P1 — not eligible
		assert_eq!(TierMemberCount::<crate::mock::Test>::get(5), 1);

		fund_vault(1_000);
		UnallocatedPool::<crate::mock::Test>::put(300);
		let pot = pool_tier_pot(300u128); // 100
		let alice_before = usdx_free(ALICE);

		assert_noop!(
			CommunityCore::claim_pool_reward(RuntimeOrigin::signed(BOB)),
			Error::<crate::mock::Test>::TierNotEligible
		);
		assert_ok!(CommunityCore::claim_pool_reward(RuntimeOrigin::signed(ALICE)));
		assert_eq!(usdx_free(ALICE), alice_before + pot);
		assert_eq!(UnallocatedPool::<crate::mock::Test>::get(), 300 - pot);

		assert_noop!(
			CommunityCore::claim_pool_reward(RuntimeOrigin::signed(ALICE)),
			Error::<crate::mock::Test>::AlreadyClaimedPool
		);
	});
}

#[test]
fn claim_pool_reward_three_tiers_equal_pots() {
	new_test_ext().execute_with(|| {
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(ALICE), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(BOB), None));
		assert_ok!(CommunityCore::register(RuntimeOrigin::signed(CHARLIE), None));
		set_lifetime(ALICE, 500); // P5
		set_lifetime(BOB, 2_000); // P6
		set_lifetime(CHARLIE, 50_000); // P7
		assert_eq!(TierMemberCount::<crate::mock::Test>::get(5), 1);
		assert_eq!(TierMemberCount::<crate::mock::Test>::get(6), 1);
		assert_eq!(TierMemberCount::<crate::mock::Test>::get(7), 1);

		fund_vault(1_000);
		UnallocatedPool::<crate::mock::Test>::put(300);
		let pot = pool_tier_pot(300u128);

		assert_ok!(CommunityCore::claim_pool_reward(RuntimeOrigin::signed(ALICE)));
		assert_ok!(CommunityCore::claim_pool_reward(RuntimeOrigin::signed(BOB)));
		assert_ok!(CommunityCore::claim_pool_reward(RuntimeOrigin::signed(CHARLIE)));
		assert_eq!(UnallocatedPool::<crate::mock::Test>::get(), 0);
		assert_eq!(usdx_free(ALICE) >= pot, true);
		assert_eq!(usdx_free(BOB) >= pot, true);
		assert_eq!(usdx_free(CHARLIE) >= pot, true);
	});
}
