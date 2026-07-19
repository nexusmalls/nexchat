// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for `pallet-bridge-ismp` (HB-ASSET-01 test matrix).

use crate::{
    impls::convert_to_erc20,
    mock::*,
    module_id_bytes,
    pallet::{
        BridgedOut, BridgedOutByChain, Chains, Error, Event, PayoutRefunds, PendingWithdraws,
    },
    types::{InboundOp, OrderIntent, WithdrawRequest},
    Message,
};
use alloy_sol_types::SolValue;
use codec::Encode;
use frame_support::BoundedVec;
use frame_support::{assert_noop, assert_ok, traits::Get};
use ismp::{
    module::IsmpModule,
    router::{PostRequest, Request},
};
use sp_core::{H160, H256};

const ERC_DECIMALS: u8 = 18;
const FUND: Balance = 1_000_000_000_000_000; // 10^15
const ONE_NEX: Balance = 1_000_000_000_000; // 10^12 (1 NEX, 12 decimals)

fn contract() -> H160 {
    H160::repeat_byte(0xCC)
}

/// Distinct NEX contract address on the Polygon lane (so multi-lane tests can
/// tell the two `from` contracts apart). 与 BSC lane 不同的 NEX 合约地址，
/// 便于多 lane 测试区分两条 lane 的 `from` 合约。
fn polygon_contract() -> H160 {
    H160::repeat_byte(0xDD)
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

/// Registers only the Polygon lane (limits shared globally with BSC setup).
/// 仅注册 Polygon lane（限额与 BSC 共享全局配置）。
fn setup_polygon() {
    assert_ok!(Bridge::register_chain(
        RuntimeOrigin::root(),
        polygon(),
        polygon_contract(),
        ERC_DECIMALS
    ));
    assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, FUND));
}

/// Registers both BSC and Polygon lanes with distinct contracts.
/// 同时注册 BSC + Polygon 两条 lane，合约地址不同。
fn setup_both_chains() {
    assert_ok!(Bridge::register_chain(
        RuntimeOrigin::root(),
        bsc(),
        contract(),
        ERC_DECIMALS
    ));
    assert_ok!(Bridge::register_chain(
        RuntimeOrigin::root(),
        polygon(),
        polygon_contract(),
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
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::ChainNotRegistered
        );
    });
}

#[test]
fn bridge_out_rejects_below_min() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                999_999,
                0
            ),
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
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::BridgePaused
        );
        // per-chain pause too
        assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), None, false));
        assert_ok!(Bridge::set_paused(RuntimeOrigin::root(), Some(bsc()), true));
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::BridgePaused
        );
    });
}

#[test]
fn bridge_out_enforces_per_tx_limit() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        assert_ok!(Bridge::register_chain(
            RuntimeOrigin::root(),
            bsc(),
            contract(),
            ERC_DECIMALS
        ));
        assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), ONE_NEX, FUND));
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX + 1,
                0
            ),
            Error::<Test>::PerTxLimitExceeded
        );
    });
}

#[test]
fn bridge_out_enforces_daily_limit() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        assert_ok!(Bridge::register_chain(
            RuntimeOrigin::root(),
            bsc(),
            contract(),
            ERC_DECIMALS
        ));
        // per_tx large, daily small (= 1.5 NEX)
        assert_ok!(Bridge::set_limits(
            RuntimeOrigin::root(),
            FUND,
            ONE_NEX + ONE_NEX / 2
        ));
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::DailyLimitExceeded
        );
    });
}

#[test]
fn bridge_out_daily_window_resets() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        assert_ok!(Bridge::register_chain(
            RuntimeOrigin::root(),
            bsc(),
            contract(),
            ERC_DECIMALS
        ));
        assert_ok!(Bridge::set_limits(RuntimeOrigin::root(), FUND, ONE_NEX));
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        // advance past the 100-block window
        System::set_block_number(200);
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_eq!(BridgedOut::<Test>::get(), 2 * ONE_NEX);
    });
}

#[test]
fn bridge_out_rejects_insufficient_balance() {
    new_test_ext(vec![(acc(1), ONE_NEX)]).execute_with(|| {
        setup_chain();
        // KeepAlive: cannot spend the whole balance (ED must remain)
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                bsc(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::InsufficientFreeBalance
        );
    });
}

// ----------------------------- register_chain validation -----------------------------

#[test]
fn register_chain_rejects_non_evm() {
    new_test_ext(vec![]).execute_with(|| {
        assert_noop!(
            Bridge::register_chain(
                RuntimeOrigin::root(),
                HostStateMachine::get(), // Substrate, not EVM
                contract(),
                ERC_DECIMALS
            ),
            Error::<Test>::NotEvmChain
        );
    });
}

#[test]
fn register_chain_rejects_erc_decimals_below_native() {
    new_test_ext(vec![]).execute_with(|| {
        // NativeDecimals = 12 in the mock; 11 is below the local floor.
        assert_noop!(
            Bridge::register_chain(RuntimeOrigin::root(), bsc(), contract(), 11),
            Error::<Test>::ErcDecimalsBelowLocal
        );
    });
}

#[test]
fn register_chain_rejects_erc_decimals_above_gap() {
    new_test_ext(vec![]).execute_with(|| {
        // native (12) + MAX_ERC_NATIVE_DECIMAL_GAP (38) = 50 is the highest accepted;
        // 51 would overflow the u128 `10^(gap)` scale factor and must be rejected.
        let native = NativeDecimals::get();
        let max_ok = native + crate::MAX_ERC_NATIVE_DECIMAL_GAP;
        assert_ok!(Bridge::register_chain(
            RuntimeOrigin::root(),
            bsc(),
            contract(),
            max_ok
        ));
        assert_noop!(
            Bridge::register_chain(RuntimeOrigin::root(), bsc(), contract(), max_ok + 1),
            Error::<Test>::ErcDecimalsTooHigh
        );
    });
}

// ----------------------------- Inbound -----------------------------

#[test]
fn on_accept_mints_within_ledger() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        // First bridge out so there is in-flight supply to mint back.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
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
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
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
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
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
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
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
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));

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
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
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

// --------------------------- Cross-order (HB-ENT-01) ---------------------------

/// Builds an inbound [`Message`] whose `data` carries a SCALE [`OrderIntent`].
fn order_body(buyer_evm: H160, local_amount: Balance, product_id: u64) -> Vec<u8> {
    let erc = convert_to_erc20(local_amount, ERC_DECIMALS, 12);
    let intent = OrderIntent {
        schema_version: 1,
        buyer_evm: buyer_evm.0,
        product_id,
        quantity: 1,
        referrer: None,
    };
    let msg = Message {
        from: [0u8; 32].to_vec().into(),
        to: [0u8; 32].to_vec().into(),
        amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
        data: InboundOp::Order(intent).encode().into(),
    };
    Message::abi_encode(&msg)
}

/// The buyer account derived from a 20-byte EVM address by the mock `EvmToSubstrate`
/// (`()` zero-pad). The runtime wires a blake2 derivation instead.
fn derived(buyer_evm: H160) -> AccountId {
    let mut b = [0u8; 32];
    b[12..].copy_from_slice(&buyer_evm.0);
    AccountId::new(b)
}

fn has_event(pred: impl Fn(&Event<Test>) -> bool) -> bool {
    System::events().iter().any(|r| match &r.event {
        RuntimeEvent::Bridge(e) => pred(e),
        _ => false,
    })
}

#[test]
fn on_accept_cross_order_success_mints_and_dispatches() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        // Bridge out first so there is in-flight supply to mint back.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        set_order_result(Ok(99));

        let buyer_evm = H160::repeat_byte(0xB0);
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            order_body(buyer_evm, ONE_NEX, 2),
        );
        assert_ok!(Bridge::default().on_accept(post));

        // NEX minted to the derived buyer; ledger decremented.
        assert_eq!(Balances::free_balance(derived(buyer_evm)), ONE_NEX);
        assert_eq!(BridgedOut::<Test>::get(), 0);
        assert_ok!(Bridge::check_ledger_invariant());
        assert!(has_event(|e| matches!(
            e,
            Event::CrossOrderPlaced {
                order_id: 99,
                product_id: 2,
                ..
            }
        )));
    });
}

#[test]
fn on_accept_cross_order_failure_credits_derived_buyer() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Order dispatch fails → mint must be KEPT as DerivedCredit, on_accept still Ok.
        set_order_result(Err(sp_runtime::DispatchError::Other("boom")));

        let buyer_evm = H160::repeat_byte(0xB1);
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            order_body(buyer_evm, ONE_NEX, 2),
        );
        assert_ok!(Bridge::default().on_accept(post));

        assert_eq!(Balances::free_balance(derived(buyer_evm)), ONE_NEX); // credited
        assert_eq!(BridgedOut::<Test>::get(), 0);
        assert_ok!(Bridge::check_ledger_invariant());
        assert!(has_event(
            |e| matches!(e, Event::CrossOrderFailed { amount, .. } if *amount == ONE_NEX)
        ));
    });
}

#[test]
fn on_accept_cross_order_rejects_unsupported_schema_version() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));

        // schema_version != PAYLOAD_SCHEMA_VERSION (1) must be rejected before any mint.
        let erc = convert_to_erc20(ONE_NEX, ERC_DECIMALS, 12);
        let intent = OrderIntent {
            schema_version: 2,
            buyer_evm: H160::repeat_byte(0xB9).0,
            product_id: 2,
            quantity: 1,
            referrer: None,
        };
        let msg = Message {
            from: [0u8; 32].to_vec().into(),
            to: [0u8; 32].to_vec().into(),
            amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
            data: InboundOp::Order(intent).encode().into(),
        };
        let post = post_to_bridge(bsc(), contract().0.to_vec(), Message::abi_encode(&msg));
        assert!(Bridge::default().on_accept(post).is_err());
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX); // unchanged, nothing minted
    });
}

#[test]
fn on_accept_cross_order_rejects_over_ledger() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Minting more than the in-flight ledger must fail before any dispatch.
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            order_body(H160::repeat_byte(0xB2), 2 * ONE_NEX, 2),
        );
        assert!(Bridge::default().on_accept(post).is_err());
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX); // unchanged
    });
}

// --------------------------- Derived withdraw (HB-ENT-01 §7) ---------------------------

/// Builds an inbound [`Message`] whose `data` carries a SCALE [`WithdrawRequest`].
fn withdraw_body(owner_evm: H160, local_amount: Balance, dest_recipient: H160) -> Vec<u8> {
    // EVM precision = local * 10^(18-12); compute directly to avoid U256 juggling.
    let req = WithdrawRequest {
        schema_version: 1,
        owner_evm: owner_evm.0,
        amount_nex: local_amount * 1_000_000,
        dest_recipient: dest_recipient.0,
    };
    let msg = Message {
        from: [0u8; 32].to_vec().into(),
        to: [0u8; 32].to_vec().into(),
        amount: alloy_primitives::U256::ZERO,
        data: InboundOp::Withdraw(req).encode().into(),
    };
    Message::abi_encode(&msg)
}

#[test]
fn on_accept_withdraw_queues_then_executes() {
    let owner_evm = H160::repeat_byte(0xD0);
    new_test_ext(vec![(derived(owner_evm), FUND)]).execute_with(|| {
        setup_chain();
        let issuance = Balances::total_issuance();

        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            withdraw_body(owner_evm, ONE_NEX, recipient()),
        );
        assert_ok!(Bridge::default().on_accept(post));

        // Phase 1: queued only — no funds touched, no ledger movement (H2).
        assert_eq!(Balances::free_balance(derived(owner_evm)), FUND);
        assert_eq!(Balances::total_issuance(), issuance);
        assert_eq!(BridgedOut::<Test>::get(), 0);
        assert!(PendingWithdraws::<Test>::get(0).is_some());
        assert!(has_event(
            |e| matches!(e, Event::WithdrawQueued { amount, .. } if *amount == ONE_NEX)
        ));

        // Veto window not elapsed yet (WithdrawDelay = 10, now = 1).
        assert_noop!(
            Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 0),
            Error::<Test>::WithdrawNotDue
        );

        // Phase 2: after the window anyone may execute → real burn + dispatch.
        System::set_block_number(11);
        assert_ok!(Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 0));

        assert_eq!(Balances::free_balance(derived(owner_evm)), FUND - ONE_NEX);
        assert_eq!(Balances::total_issuance(), issuance - ONE_NEX); // really burned
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX);
        assert_eq!(BridgedOutByChain::<Test>::get(bsc()), ONE_NEX);
        assert!(PendingWithdraws::<Test>::get(0).is_none());
        assert_ok!(Bridge::check_ledger_invariant());
        assert!(has_event(
            |e| matches!(e, Event::DerivedWithdraw { amount, .. } if *amount == ONE_NEX)
        ));
    });
}

#[test]
fn cancel_withdraw_drops_entry_without_touching_funds() {
    let owner_evm = H160::repeat_byte(0xD2);
    new_test_ext(vec![(derived(owner_evm), FUND)]).execute_with(|| {
        setup_chain();
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            withdraw_body(owner_evm, ONE_NEX, recipient()),
        );
        assert_ok!(Bridge::default().on_accept(post));
        assert!(PendingWithdraws::<Test>::get(0).is_some());

        // Guardian veto removes the entry; no funds ever moved.
        assert_ok!(Bridge::cancel_withdraw(RuntimeOrigin::root(), 0));
        assert!(PendingWithdraws::<Test>::get(0).is_none());
        assert_eq!(Balances::free_balance(derived(owner_evm)), FUND);
        assert!(has_event(
            |e| matches!(e, Event::WithdrawCancelled { id } if *id == 0)
        ));

        // A cancelled entry can no longer be executed.
        System::set_block_number(11);
        assert_noop!(
            Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 0),
            Error::<Test>::WithdrawNotFound
        );
    });
}

#[test]
fn cancel_withdraw_requires_guardian() {
    let owner_evm = H160::repeat_byte(0xD3);
    new_test_ext(vec![(derived(owner_evm), FUND)]).execute_with(|| {
        setup_chain();
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            withdraw_body(owner_evm, ONE_NEX, recipient()),
        );
        assert_ok!(Bridge::default().on_accept(post));
        // A non-guardian signer cannot veto.
        assert!(Bridge::cancel_withdraw(RuntimeOrigin::signed(acc(1)), 0).is_err());
        assert!(PendingWithdraws::<Test>::get(0).is_some());
    });
}

#[test]
fn execute_withdraw_rejects_unknown_id() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_noop!(
            Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 99),
            Error::<Test>::WithdrawNotFound
        );
    });
}

#[test]
fn execute_withdraw_insufficient_balance_retains_entry() {
    let owner_evm = H160::repeat_byte(0xD1);
    // Fund exactly ONE_NEX: KeepAlive cannot spend the whole balance (ED must remain).
    new_test_ext(vec![(derived(owner_evm), ONE_NEX)]).execute_with(|| {
        setup_chain();
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            withdraw_body(owner_evm, ONE_NEX, recipient()),
        );
        // Queue succeeds (no funds touched yet).
        assert_ok!(Bridge::default().on_accept(post));
        // Execution fails (KeepAlive) and rolls back; the entry stays queued for retry.
        System::set_block_number(11);
        assert!(Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 0).is_err());
        assert_eq!(Balances::free_balance(derived(owner_evm)), ONE_NEX); // unchanged
        assert_eq!(BridgedOut::<Test>::get(), 0); // nothing booked
        assert!(PendingWithdraws::<Test>::get(0).is_some()); // retained
    });
}

#[test]
fn plain_inbound_to_evm_address_uses_evm_derivation() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        // Seed in-flight supply so the inbound mint is allowed.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));

        // Plain transfer to a bare 20-byte EVM address.
        let evm = H160::repeat_byte(0x33);
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            inbound_body(&acc(2), evm, ONE_NEX),
        );
        assert_ok!(Bridge::default().on_accept(post));

        // The beneficiary is the `EvmToSubstrate`-derived account (the same mapping the
        // withdraw path debits), NOT some ad-hoc encoding — so the funds are reachable.
        assert_eq!(Balances::free_balance(derived(evm)), ONE_NEX);
    });
}

#[test]
fn plain_inbound_then_withdraw_resolve_to_same_account() {
    let evm = H160::repeat_byte(0x44);
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        // Bridge out enough in-flight supply, then mint 2 NEX to the EVM-derived account.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            2 * ONE_NEX,
            0
        ));
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            inbound_body(&acc(2), evm, 2 * ONE_NEX),
        );
        assert_ok!(Bridge::default().on_accept(post));
        assert_eq!(Balances::free_balance(derived(evm)), 2 * ONE_NEX);

        // A withdraw for that EVM identity, once executed, debits the very account the
        // transfer credited (KeepAlive leaves 1 NEX behind), proving the two
        // derivations are unified.
        let post = post_to_bridge(
            bsc(),
            contract().0.to_vec(),
            withdraw_body(evm, ONE_NEX, recipient()),
        );
        assert_ok!(Bridge::default().on_accept(post));
        System::set_block_number(11);
        assert_ok!(Bridge::execute_withdraw(RuntimeOrigin::signed(acc(1)), 0));
        assert_eq!(Balances::free_balance(derived(evm)), ONE_NEX);
        assert!(has_event(
            |e| matches!(e, Event::DerivedWithdraw { amount, .. } if *amount == ONE_NEX)
        ));
    });
}

#[test]
fn on_accept_rejects_invalid_payload() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Non-empty but undecodable `data` → InvalidPayload, nothing minted.
        let erc = convert_to_erc20(ONE_NEX, ERC_DECIMALS, 12);
        let msg = Message {
            from: [0u8; 32].to_vec().into(),
            to: [0u8; 32].to_vec().into(),
            amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
            data: vec![0xFFu8; 3].into(),
        };
        let post = post_to_bridge(bsc(), contract().0.to_vec(), Message::abi_encode(&msg));
        assert!(Bridge::default().on_accept(post).is_err());
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX); // unchanged
    });
}

// --------------------- Tracked payout refund (HB-WD-01 mechanism 2) ---------------------

/// Reconstructs the canonical timed-out POST for `local_amount` to `recipient()`
/// (sender = `acc(1)`), as `on_timeout` would receive it.
fn timed_out_post(local_amount: Balance) -> Request {
    Request::Post(PostRequest {
        source: HostStateMachine::get(),
        dest: bsc(),
        nonce: 0,
        from: module_id_bytes(),
        to: contract().0.to_vec(),
        timeout_timestamp: 0,
        body: inbound_body(&acc(1), recipient(), local_amount),
    })
}

#[test]
fn do_outbound_tracked_records_refund_meta() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        let meta: BoundedVec<u8, _> = vec![9, 8, 7, 6].try_into().unwrap();
        let commitment =
            Bridge::do_outbound_tracked(&acc(1), bsc(), recipient(), ONE_NEX, 0, meta.clone())
                .unwrap();
        // NEX burned (same as plain do_outbound) and meta recorded at the commitment,
        // tagged with the dispatch block (1, set by new_test_ext).
        assert_eq!(Balances::free_balance(acc(1)), FUND - ONE_NEX);
        assert_eq!(PayoutRefunds::<Test>::get(commitment), Some((1, meta)));
    });
}

#[test]
fn on_timeout_invokes_refund_handler_and_consumes_entry() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        // Seed in-flight supply so the refund re-mint is allowed.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));

        // Simulate a tracked payout: record meta at the request's canonical commitment.
        let req = timed_out_post(ONE_NEX);
        let commitment = ismp::messaging::hash_request::<pallet_ismp::Pallet<Test>>(&req);
        let meta: BoundedVec<u8, _> = vec![1, 2, 3, 4].try_into().unwrap();
        PayoutRefunds::<Test>::insert(commitment, (1u64, meta));

        assert_ok!(Bridge::default().on_timeout(req));

        // Sender refunded, handler invoked with the exact meta, entry consumed.
        assert_eq!(Balances::free_balance(acc(1)), FUND);
        assert_eq!(refund_calls(), vec![vec![1, 2, 3, 4]]);
        assert!(PayoutRefunds::<Test>::get(commitment).is_none());
        assert!(has_event(|e| matches!(
            e,
            Event::PayoutRefundNotified { handled: true, .. }
        )));
    });
}

#[test]
fn on_timeout_handler_failure_still_refunds_and_reports_unhandled() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        set_refund_fail(true); // business-side compensation fails

        let req = timed_out_post(ONE_NEX);
        let commitment = ismp::messaging::hash_request::<pallet_ismp::Pallet<Test>>(&req);
        let meta: BoundedVec<u8, _> = vec![5, 5].try_into().unwrap();
        PayoutRefunds::<Test>::insert(commitment, (1u64, meta));

        assert_ok!(Bridge::default().on_timeout(req));

        // Re-mint to the sender always stands; the entry is consumed; handled=false.
        assert_eq!(Balances::free_balance(acc(1)), FUND);
        assert_eq!(refund_calls(), vec![vec![5, 5]]);
        assert!(PayoutRefunds::<Test>::get(commitment).is_none());
        assert!(has_event(|e| matches!(
            e,
            Event::PayoutRefundNotified { handled: false, .. }
        )));
    });
}

#[test]
fn on_timeout_without_tracked_meta_skips_handler() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_chain();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Plain bridge_out leaves no PayoutRefunds entry → handler not called.
        assert_ok!(Bridge::default().on_timeout(timed_out_post(ONE_NEX)));
        assert_eq!(Balances::free_balance(acc(1)), FUND);
        assert!(refund_calls().is_empty());
    });
}

#[test]
fn prune_payout_refunds_removes_only_stale_entries() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        // PayoutRefundTtl = 100 in the mock.
        let meta: BoundedVec<u8, _> = vec![7].try_into().unwrap();

        // An entry dispatched at block 1 (will be stale once now - 1 >= 100).
        let c_old = H256::repeat_byte(0x01);
        PayoutRefunds::<Test>::insert(c_old, (1u64, meta.clone()));

        // Advance well past the TTL, then add a fresh entry.
        System::set_block_number(150);
        let c_new = H256::repeat_byte(0x02);
        PayoutRefunds::<Test>::insert(c_new, (150u64, meta.clone()));

        assert_ok!(Bridge::prune_payout_refunds(
            RuntimeOrigin::signed(acc(1)),
            10
        ));

        assert!(PayoutRefunds::<Test>::get(c_old).is_none()); // stale → removed
        assert!(PayoutRefunds::<Test>::get(c_new).is_some()); // fresh → kept
        assert!(has_event(|e| matches!(
            e,
            Event::PayoutRefundsPruned { removed: 1 }
        )));
    });
}

#[test]
fn prune_payout_refunds_respects_limit() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        let meta: BoundedVec<u8, _> = vec![7].try_into().unwrap();
        for i in 0..5u8 {
            PayoutRefunds::<Test>::insert(H256::repeat_byte(i), (1u64, meta.clone()));
        }
        System::set_block_number(200); // all 5 are stale

        // limit caps how many entries are examined/removed per call.
        assert_ok!(Bridge::prune_payout_refunds(
            RuntimeOrigin::signed(acc(1)),
            2
        ));
        assert_eq!(PayoutRefunds::<Test>::iter().count(), 3);

        assert_ok!(Bridge::prune_payout_refunds(
            RuntimeOrigin::signed(acc(1)),
            10
        ));
        assert_eq!(PayoutRefunds::<Test>::iter().count(), 0);
    });
}

// ===========================================================================
// Polygon lane (Evm(137)) — verifies the burn/mint bridge is EVM-chain-agnostic
// and that multiple lanes coexist with independent ledgers.
// Polygon lane（Evm(137)）— 验证 burn/mint 桥对 EVM 链通用，多 lane 账本独立共存。
// ===========================================================================

#[test]
fn polygon_register_chain_succeeds() {
    new_test_ext(vec![]).execute_with(|| {
        assert_ok!(Bridge::register_chain(
            RuntimeOrigin::root(),
            polygon(),
            polygon_contract(),
            ERC_DECIMALS
        ));
        assert!(Chains::<Test>::get(polygon()).is_some());
        assert!(has_event(|e| matches!(
            e,
            Event::ChainRegistered { chain, .. } if *chain == polygon()
        )));
    });
}

#[test]
fn polygon_bridge_out_burns_and_books() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_polygon();
        let issuance_before = Balances::total_issuance();

        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            ONE_NEX,
            0
        ));

        assert_eq!(Balances::free_balance(acc(1)), FUND - ONE_NEX);
        assert_eq!(Balances::total_issuance(), issuance_before - ONE_NEX);
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX);
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), ONE_NEX);
        // BSC lane untouched.
        assert_eq!(BridgedOutByChain::<Test>::get(bsc()), 0);
        assert_ok!(Bridge::check_ledger_invariant());
    });
}

#[test]
fn polygon_round_trip_restores_total_issuance() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_polygon();
        let issuance_before = Balances::total_issuance();

        // Outbound: burn 1 NEX to Polygon.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_eq!(Balances::total_issuance(), issuance_before - ONE_NEX);

        // Inbound: mint back from Polygon to a 20-byte EVM recipient.
        let evm = H160::repeat_byte(0x22);
        let body = inbound_body(&acc(2), evm, ONE_NEX);
        let post = post_to_bridge(polygon(), polygon_contract().0.to_vec(), body);
        assert_ok!(Bridge::default().on_accept(post));

        assert_eq!(Balances::total_issuance(), issuance_before);
        assert_eq!(BridgedOut::<Test>::get(), 0);
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), 0);
        assert_ok!(Bridge::check_ledger_invariant());
    });
}

#[test]
fn polygon_inbound_exceeds_bridged_out_rejected() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_polygon();
        // Only 1 NEX bridged out to Polygon.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Forged inbound claiming 2 NEX from Polygon → anti-inflation reject.
        let body = inbound_body(&acc(2), recipient(), 2 * ONE_NEX);
        let post = post_to_bridge(polygon(), polygon_contract().0.to_vec(), body);
        assert!(Bridge::default().on_accept(post).is_err());
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), ONE_NEX); // unchanged
    });
}

#[test]
fn polygon_inbound_rejects_unknown_source_contract() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_polygon();
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            ONE_NEX,
            0
        ));
        // `from` is the BSC contract, not the registered Polygon contract.
        let body = inbound_body(&acc(2), recipient(), ONE_NEX);
        let post = post_to_bridge(polygon(), contract().0.to_vec(), body);
        assert!(Bridge::default().on_accept(post).is_err());
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), ONE_NEX); // unchanged
    });
}

#[test]
fn polygon_pause_lane_blocks_outbound() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_polygon();
        assert_ok!(Bridge::set_paused(
            RuntimeOrigin::root(),
            Some(polygon()),
            true
        ));
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                polygon(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::BridgePaused
        );
    });
}

#[test]
fn polygon_pause_does_not_affect_bsc() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_both_chains();
        // Pause only the Polygon lane.
        assert_ok!(Bridge::set_paused(
            RuntimeOrigin::root(),
            Some(polygon()),
            true
        ));

        // Polygon outbound is blocked...
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                polygon(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::BridgePaused
        );
        // ...while BSC stays fully operational.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_eq!(BridgedOutByChain::<Test>::get(bsc()), ONE_NEX);
    });
}

#[test]
fn multi_lane_independent_ledgers() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_both_chains();
        let issuance_before = Balances::total_issuance();

        // Bridge 1 NEX to BSC and 2 NEX to Polygon.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            2 * ONE_NEX,
            0
        ));

        // Per-chain ledgers are independent...
        assert_eq!(BridgedOutByChain::<Test>::get(bsc()), ONE_NEX);
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), 2 * ONE_NEX);
        // ...and the global total aggregates both.
        assert_eq!(BridgedOut::<Test>::get(), 3 * ONE_NEX);
        assert_eq!(Balances::total_issuance(), issuance_before - 3 * ONE_NEX);
        assert_ok!(Bridge::check_ledger_invariant());

        // Redeem back from Polygon only: BSC ledger must be untouched.
        let evm = H160::repeat_byte(0x55);
        let body = inbound_body(&acc(2), evm, 2 * ONE_NEX);
        let post = post_to_bridge(polygon(), polygon_contract().0.to_vec(), body);
        assert_ok!(Bridge::default().on_accept(post));
        assert_eq!(BridgedOutByChain::<Test>::get(bsc()), ONE_NEX); // unchanged
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), 0); // redeemed
        assert_eq!(BridgedOut::<Test>::get(), ONE_NEX); // only BSC remains in-flight
        assert_ok!(Bridge::check_ledger_invariant());
    });
}

#[test]
fn polygon_precision_scaling_matches_bsc() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_both_chains();
        // Same local amount bridged to each lane → identical ERC amounts on both.
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            polygon(),
            recipient(),
            ONE_NEX,
            0
        ));
        // Inbound dust-truncation behaves identically on the Polygon lane.
        let evm = H160::repeat_byte(0x66);
        let erc = convert_to_erc20(ONE_NEX, ERC_DECIMALS, 12) + sp_core::U256::from(1u8);
        let sender_bytes: [u8; 32] = acc(2).into();
        let msg = Message {
            from: sender_bytes.to_vec().into(),
            to: evm.0.to_vec().into(),
            amount: alloy_primitives::U256::from_be_bytes(erc.to_big_endian()),
            data: Default::default(),
        };
        let post = post_to_bridge(
            polygon(),
            polygon_contract().0.to_vec(),
            Message::abi_encode(&msg),
        );
        assert_ok!(Bridge::default().on_accept(post));
        // Exactly ONE_NEX minted (dust dropped), Polygon ledger cleared.
        assert_eq!(BridgedOutByChain::<Test>::get(polygon()), 0);
        assert_ok!(Bridge::check_ledger_invariant());
    });
}

#[test]
fn polygon_deregister_disables_lane() {
    new_test_ext(vec![(acc(1), FUND)]).execute_with(|| {
        setup_both_chains();
        // Deregister Polygon → outbound to Polygon fails, BSC unaffected.
        assert_ok!(Bridge::deregister_chain(RuntimeOrigin::root(), polygon()));
        assert_noop!(
            Bridge::bridge_out(
                RuntimeOrigin::signed(acc(1)),
                polygon(),
                recipient(),
                ONE_NEX,
                0
            ),
            Error::<Test>::ChainNotRegistered
        );
        assert_ok!(Bridge::bridge_out(
            RuntimeOrigin::signed(acc(1)),
            bsc(),
            recipient(),
            ONE_NEX,
            0
        ));
    });
}
