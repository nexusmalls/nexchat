// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for `pallet-bridge-ismp` (HB-ASSET-01 test matrix).

use crate::{
	impls::convert_to_erc20,
	mock::*,
	pallet::{BridgedOut, BridgedOutByChain, Error},
	module_id_bytes, Message,
};
use alloy_sol_types::SolValue;
use frame_support::{assert_noop, assert_ok};
use ismp::{
	module::IsmpModule,
	router::{PostRequest, Request},
};
use sp_core::H160;

const ERC_DECIMALS: u8 = 18;
const FUND: Balance = 1_000_000_000_000_000; // 10^15
const ONE_NEX: Balance = 1_000_000_000_000; // 10^12 (1 NEX, 12 decimals)

fn contract() -> H160 {
	H160::repeat_byte(0xCC)
}

fn recipient() -> H160 {
	H160::repeat_byte(0xEE)
}

fn setup_chain() {
	assert_ok!(Bridge::register_chain(
		RuntimeOrigin::root(),
		bsc(),
		contract(),
		ERC_DECIMALS
	));
	assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, FUND));
}

/// Builds an ABI-encoded inbound [`Message`] body carrying `local_amount` NEX.
fn inbound_body(sender: &AccountId, to: H160, local_amount: Balance) -> Vec<u8> {
	let sender_bytes: [u8; 32] = sender.clone().into();
	let erc = convert_to_erc20(local_amount, ERC_DECIMALS, 12);
	let msg = Message {
		from: sender_bytes.to_vec().into(),
		to: to.0.to_vec().into(),
		amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
		data: Default::default(),
	};
	Message::abi_encode(&msg)
}

fn post_to_bridge(source: ismp::host::StateMachine, from: Vec<u8>, body: Vec<u8>) -> PostRequest {
	PostRequest {
		source,
		dest: HostStateMachine::get(),
		nonce: 0,
		from,
		to: module_id_bytes(),
		timeout_timestamp: 0,
		body,
	}
}

// ----------------------------- Outbound -----------------------------

#[test]
fn bridge_out_burns_and_books() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		let issuance_before = Balances::total_issuance();

		assert_ok!(Bridge::bridge_out(
			RuntimeOrigin::signed(acc(1)),
			bsc(),
			recipient(),
			ONE_NEX,
			0
		));

		assert_eq!(Balances::free_balance(acc(1)), FUND - ONE_NEX);
		assert_eq!(Balances::total_issuance(), issuance_before - ONE_NEX);
		assert_eq!(BridgedOut::<Test>::get(), ONE_NEX);
		assert_eq!(BridgedOutByChain::<Test>::get(bsc()), ONE_NEX);
		assert_ok!(Bridge::check_ledger_invariant());
	});
}

#[test]
fn bridge_out_rejects_unregistered_chain() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, FUND));
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0),
			Error::<Test>::ChainNotRegistered
		);
	});
}

#[test]
fn bridge_out_rejects_below_min() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), 999_999, 0),
			Error::<Test>::AmountBelowMin
		);
	});
}

#[test]
fn bridge_out_rejects_when_paused() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), None, true));
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0),
			Error::<Test>::BridgePaused
		);
		// per-chain pause too
		assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), None, false));
		assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), Some(bsc()), true));
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0),
			Error::<Test>::BridgePaused
		);
	});
}

#[test]
fn bridge_out_enforces_per_tx_limit() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		assert_ok!(Bridge::register_chain(RuntimeOrigin::root(), bsc(), contract(), ERC_DECIMALS));
		assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), ONE_NEX, FUND));
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX + 1, 0),
			Error::<Test>::PerTxLimitExceeded
		);
	});
}

#[test]
fn bridge_out_enforces_daily_limit() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		assert_ok!(Bridge::register_chain(RuntimeOrigin::root(), bsc(), contract(), ERC_DECIMALS));
		// per_tx large, daily small (= 1.5 NEX)
		assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, ONE_NEX + ONE_NEX / 2));
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0),
			Error::<Test>::DailyLimitExceeded
		);
	});
}

#[test]
fn bridge_out_daily_window_resets() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		assert_ok!(Bridge::register_chain(RuntimeOrigin::root(), bsc(), contract(), ERC_DECIMALS));
		assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, ONE_NEX));
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		// advance past the 100-block window
		System::set_block_number(200);
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		assert_eq!(BridgedOut::<Test>::get(), 2 * ONE_NEX);
	});
}

#[test]
fn bridge_out_rejects_insufficient_balance() {
	new_test_ext(vec![(acc(1), ONE_NEX)]).execute_with(|| {
		setup_chain();
		// KeepAlive: cannot spend the whole balance (ED must remain)
		assert_noop!(
			Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0),
			Error::<Test>::InsufficientFreeBalance
		);
	});
}

// ----------------------------- Inbound -----------------------------

#[test]
fn on_accept_mints_within_ledger() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		// First bridge out so there is in-flight supply to mint back.
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		let issuance = Balances::total_issuance();

		let body = inbound_body(&acc(2), H160::repeat_byte(0x22), ONE_NEX);
		let post = post_to_bridge(bsc(), contract().0.to_vec(), body);
		assert_ok!(Bridge::default().on_accept(post));

		// beneficiary derived from the 20-byte recipient (zero-padded)
		let mut bytes = [0u8; 32];
		bytes[12..].copy_from_slice(&H160::repeat_byte(0x22).0);
		let beneficiary = AccountId::new(bytes);
		assert_eq!(Balances::free_balance(beneficiary), ONE_NEX);
		assert_eq!(Balances::total_issuance(), issuance + ONE_NEX);
		assert_eq!(BridgedOut::<Test>::get(), 0);
		assert_eq!(BridgedOutByChain::<Test>::get(bsc()), 0);
		assert_ok!(Bridge::check_ledger_invariant());
	});
}

#[test]
fn on_accept_rejects_over_ledger() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		// Try to mint more than was bridged out → invariant violation.
		let body = inbound_body(&acc(2), recipient(), 2 * ONE_NEX);
		let post = post_to_bridge(bsc(), contract().0.to_vec(), body);
		assert!(Bridge::default().on_accept(post).is_err());
		assert_eq!(BridgedOut::<Test>::get(), ONE_NEX); // unchanged
	});
}

#[test]
fn on_accept_rejects_unknown_source_contract() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		let body = inbound_body(&acc(2), recipient(), ONE_NEX);
		// wrong `from` (not the registered contract)
		let post = post_to_bridge(bsc(), H160::repeat_byte(0x11).0.to_vec(), body);
		assert!(Bridge::default().on_accept(post).is_err());
	});
}

#[test]
fn on_accept_rejects_when_paused() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), Some(bsc()), true));
		let body = inbound_body(&acc(2), recipient(), ONE_NEX);
		let post = post_to_bridge(bsc(), contract().0.to_vec(), body);
		assert!(Bridge::default().on_accept(post).is_err());
	});
}

#[test]
fn on_accept_truncates_dust() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));

		// Build an ERC amount equal to ONE_NEX plus 1 wei of dust; the inbound
		// integer division (18→12) must truncate the dust back to exactly ONE_NEX.
		let erc = convert_to_erc20(ONE_NEX, ERC_DECIMALS, 12) + sp_core::U256::from(1u8);
		let sender_bytes: [u8; 32] = acc(2).into();
		let msg = Message {
			from: sender_bytes.to_vec().into(),
			to: recipient().0.to_vec().into(),
			amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
			data: Default::default(),
		};
		let post = post_to_bridge(bsc(), contract().0.to_vec(), Message::abi_encode(&msg));
		assert_ok!(Bridge::default().on_accept(post));
		assert_eq!(BridgedOut::<Test>::get(), 0); // exactly ONE_NEX minted, dust dropped
	});
}

// ----------------------------- Timeout -----------------------------

#[test]
fn on_timeout_refunds_sender() {
	new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
		setup_chain();
		assert_ok!(Bridge::bridge_out(RuntimeOrigin::signed(acc(1)), bsc(), recipient(), ONE_NEX, 0));
		assert_eq!(Balances::free_balance(acc(1)), FUND - ONE_NEX);

		// Reconstruct the outbound POST that timed out (sender encoded in `from`).
		let body = inbound_body(&acc(1), recipient(), ONE_NEX);
		let timed_out = Request::Post(PostRequest {
			source: HostStateMachine::get(),
			dest: bsc(),
			nonce: 0,
			from: module_id_bytes(),
			to: contract().0.to_vec(),
			timeout_timestamp: 0,
			body,
		});
		assert_ok!(Bridge::default().on_timeout(timed_out));

		assert_eq!(Balances::free_balance(acc(1)), FUND); // fully refunded
		assert_eq!(BridgedOut::<Test>::get(), 0);
		assert_ok!(Bridge::check_ledger_invariant());
	});
}
