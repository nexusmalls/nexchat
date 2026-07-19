use crate::{mock::*, pallet::*};
use frame_support::{
    assert_noop, assert_ok,
    traits::ConstU32,
    traits::{Currency, Hooks},
    weights::Weight,
    BoundedVec,
};
use sp_runtime::traits::{ValidateUnsigned, Zero};
use sp_runtime::transaction_validity::TransactionSource;

fn tron_address() -> Vec<u8> {
    b"T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb".to_vec()
}

/// Helper: create a TxHashVec containing a single hash
fn make_tx_hashes(hash: &[u8]) -> TxHashVec {
    BoundedVec::try_from(vec![BoundedVec::try_from(hash.to_vec()).unwrap()]).unwrap()
}

/// Helper: create an empty TxHashVec
fn empty_tx_hashes() -> TxHashVec {
    BoundedVec::try_from(vec![]).unwrap()
}

/// Convenience wrapper: call submit_ocw_result
fn call_submit_ocw_result(
    trade_id: u64,
    actual_amount: u64,
    tx_hashes: TxHashVec,
) -> frame_support::dispatch::DispatchResult {
    NexMarket::submit_ocw_result(RuntimeOrigin::none(), trade_id, actual_amount, tx_hashes)
}

/// Convenience wrapper: call auto_confirm_payment
fn call_auto_confirm_payment(
    trade_id: u64,
    actual_amount: u64,
    tx_hashes: TxHashVec,
) -> frame_support::dispatch::DispatchResult {
    NexMarket::auto_confirm_payment(RuntimeOrigin::none(), trade_id, actual_amount, tx_hashes)
}

/// Convenience wrapper: call submit_underpaid_update
fn call_submit_underpaid_update(
    trade_id: u64,
    new_actual_amount: u64,
    new_tx_hashes: TxHashVec,
) -> frame_support::dispatch::DispatchResult {
    NexMarket::submit_underpaid_update(
        RuntimeOrigin::none(),
        trade_id,
        new_actual_amount,
        new_tx_hashes,
    )
}

fn buyer_tron() -> Vec<u8> {
    b"TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_vec()
}

/// 设置初始价格（seed_liquidity 需要基准价格）
fn setup_seed_price() {
    assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));
}

#[test]
fn set_initial_price_accepts_governance_account() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::set_initial_price(
            RuntimeOrigin::signed(99),
            500_000
        ));
        assert_eq!(NexMarket::last_trade_price(), Some(500_000));
    });
}

#[test]
fn set_initial_price_rejects_non_governance_signed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::set_initial_price(RuntimeOrigin::signed(ALICE), 500_000),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn configure_price_protection_accepts_governance_account() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::signed(99),
            true,
            1000,
            2000,
            3,
        ));
        assert!(NexMarket::price_protection().unwrap().enabled);
    });
}

#[test]
fn configure_price_protection_rejects_non_governance_signed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::configure_price_protection(
                RuntimeOrigin::signed(ALICE),
                true,
                1000,
                2000,
                3,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ==================== 卖单测试 ====================

#[test]
fn place_sell_order_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64; // 0.5 USDT/NEX

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.side, OrderSide::Sell);
        assert_eq!(order.nex_amount, nex);
        assert_eq!(order.usdt_price, price);
        assert_eq!(order.status, OrderStatus::Open);

        // NEX 已锁定
        assert_eq!(Balances::reserved_balance(ALICE), nex);
    });
}

#[test]
fn place_sell_order_insufficient_balance() {
    new_test_ext().execute_with(|| {
        // 使用低于 MaxOrderNexAmount(500 NEX) 但高于余额的金额
        let too_much = 500_000_000_000_000u128; // 500 NEX, Alice only has 1000
                                                // 先下一半订单锁定 500 NEX
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            500_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        // 再下 500 NEX → 余额不足
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                too_much,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::InsufficientBalance
        );
    });
}

#[test]
fn place_sell_order_zero_price() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                0,
                tron_address(),
                None,
            ),
            Error::<Test>::ZeroPrice
        );
    });
}

#[test]
fn place_sell_order_invalid_tron_address() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                500_000,
                b"short".to_vec(),
                None,
            ),
            Error::<Test>::InvalidTronAddress
        );
    });
}

// ==================== 买单测试 ====================

#[test]
fn place_buy_order_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            price,
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.nex_amount, nex);
        // 买单预锁定保证金
        assert!(!order.buyer_deposit.is_zero());
        assert_eq!(Balances::reserved_balance(BOB), order.buyer_deposit);
        assert_eq!(
            Balances::free_balance(BOB),
            bob_before - order.buyer_deposit
        );
    });
}

#[test]
fn place_buy_order_insufficient_deposit() {
    new_test_ext().execute_with(|| {
        // DAVE 余额为 0（mock 中未分配），下任意买单保证金不足
        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(DAVE),
                100_000_000_000_000,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::InsufficientDepositBalance
        );
    });
}

// ==================== 取消订单测试 ====================

#[test]
fn cancel_sell_order_returns_nex() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let initial = Balances::free_balance(ALICE);

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_eq!(Balances::free_balance(ALICE), initial - nex);

        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));
        assert_eq!(Balances::free_balance(ALICE), initial);

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    });
}

#[test]
fn cancel_order_not_owner() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::cancel_order(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::NotOrderOwner
        );
    });
}

#[test]
fn cancel_buy_order_works() {
    new_test_ext().execute_with(|| {
        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            100_000_000_000_000,
            500_000,
            buyer_tron(),
        ));
        // 保证金已锁定
        assert!(Balances::reserved_balance(BOB) > 0);

        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(BOB), 0));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
        // 保证金已退还
        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(Balances::free_balance(BOB), bob_before);
    });
}

// ==================== reserve_sell_order 测试 ====================

#[test]
fn reserve_sell_order_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64; // 0.5 USDT/NEX

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // Bob 预锁定（吃卖单）
        let _bob_initial = Balances::free_balance(BOB);
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // 检查 trade
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.seller, ALICE);
        assert_eq!(trade.buyer, BOB);
        assert_eq!(trade.nex_amount, nex);
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingPayment);
        assert!(!trade.buyer_deposit.is_zero());
        assert!(trade.buyer_tron_address.is_some());

        // Bob 保证金已锁定
        assert!(Balances::reserved_balance(BOB) > 0);

        // 订单已更新
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.filled_amount, nex);
        assert_eq!(order.status, OrderStatus::Filled);
    });
}

#[test]
fn reserve_sell_order_partial() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));

        // Bob 只吃一半
        let half = 50_000_000_000_000u128;
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(half),
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.filled_amount, half);
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
    });
}

#[test]
fn reserve_sell_order_cannot_take_own() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(ALICE), 0, None, buyer_tron()),
            Error::<Test>::CannotTakeOwnOrder
        );
    });
}

// ==================== accept_buy_order 测试 ====================

#[test]
fn accept_buy_order_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // Bob 挂买单（保证金已预锁定）
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            500_000,
            buyer_tron(),
        ));
        let deposit_after_place = Balances::reserved_balance(BOB);
        assert!(deposit_after_place > 0);

        // Alice 接受（卖 NEX）— 不应题外增加 Bob 的 reserved
        assert_ok!(NexMarket::accept_buy_order(
            RuntimeOrigin::signed(ALICE),
            0,
            None,
            tron_address(),
        ));
        assert_eq!(Balances::reserved_balance(ALICE), nex);
        // Bob 的 reserved 不变（保证金已在 place_buy_order 时预锁）
        assert_eq!(Balances::reserved_balance(BOB), deposit_after_place);

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.seller, ALICE);
        assert_eq!(trade.buyer, BOB);
        // 交易的保证金 == 订单预锁的保证金（全额成交）
        assert_eq!(trade.buyer_deposit, deposit_after_place);
    });
}

// ==================== confirm_payment 测试 ====================

#[test]
fn confirm_payment_works() {
    new_test_ext().execute_with(|| {
        // 创建卖单 + 预锁定
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // Bob 确认支付（卖单流程，buyer_tron 已在 reserve 时提供，可传 None）
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingVerification);
        assert!(trade.buyer_tron_address.is_some());

        // 已加入待验证队列
        let pending = NexMarket::pending_usdt_trades();
        assert!(pending.contains(&0));
    });
}

#[test]
fn confirm_payment_not_buyer() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_noop!(
            NexMarket::confirm_payment(RuntimeOrigin::signed(ALICE), 0),
            Error::<Test>::NotTradeParticipant
        );
    });
}

// ==================== OCW + claim_reward 测试 ====================

#[test]
fn full_trade_flow_exact_payment() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64; // 0.5 USDT/NEX

        // 1. Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // 2. Bob 预锁定
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let usdt_amount = trade.usdt_amount;

        // 3. Bob 确认支付
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));

        // 4. OCW 提交结果（精确付款）→ R1: 自动结算
        let bob_before = Balances::free_balance(BOB);
        assert_ok!(call_submit_ocw_result(
            0,
            usdt_amount,
            make_tx_hashes(b"tx_full_trade_exact")
        ));

        // R1: submit_ocw_result 已自动结算，OCW 结果已清理
        assert!(NexMarket::ocw_verification_results(0).is_none());

        let bob_after = Balances::free_balance(BOB);
        assert!(bob_after > bob_before);

        // 保证金已退还（Bob 的 reserved 为 0）
        assert_eq!(Balances::reserved_balance(BOB), 0);

        // Trade 已完成
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Released);
    });
}

#[test]
fn underpaid_enters_pending_then_finalize() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));

        // 少付 70% → 进入 UnderpaidPending
        let actual = expected * 70 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual,
            make_tx_hashes(b"tx_underpaid_70pct")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);
        assert!(trade.underpaid_deadline.is_some());
        assert!(NexMarket::pending_underpaid_trades().contains(&0));

        // 补付窗口未到期 → finalize 失败
        assert_noop!(
            NexMarket::finalize_underpaid(RuntimeOrigin::signed(CHARLIE), 0),
            Error::<Test>::UnderpaidGraceNotExpired
        );

        // 推进到窗口到期后
        let deadline: u64 = trade.underpaid_deadline.unwrap().into();
        System::set_block_number(deadline + 1);

        let alice_before = Balances::free_balance(ALICE);
        let bob_before = Balances::free_balance(BOB);

        assert_ok!(NexMarket::finalize_underpaid(
            RuntimeOrigin::signed(CHARLIE),
            0,
        ));

        // Bob 获得约 70% 的 NEX
        let bob_nex_gained = Balances::free_balance(BOB) - bob_before;
        let expected_nex = nex * 70 / 100;
        assert!(bob_nex_gained <= expected_nex + 1_000_000_000);

        // Alice 退还了约 30% 的 NEX
        let alice_gained = Balances::free_balance(ALICE) - alice_before;
        assert!(alice_gained > 0);

        // 保证金被没收（70% ratio → 50% forfeit rate）
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Forfeited);
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

#[test]
fn severely_underpaid_auto_process() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));

        // 严重少付 10% → R1: 自动处理
        let actual = expected * 10 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual,
            make_tx_hashes(b"tx_severe_underpaid")
        ));

        // R1: 自动结算后 OCW 结果已清理
        assert!(NexMarket::ocw_verification_results(0).is_none());

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Forfeited);
    });
}

#[test]
fn underpaid_topup_upgrades_to_exact() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 少付 80% → UnderpaidPending
        let actual_80 = expected * 80 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_80,
            make_tx_hashes(b"tx_topup_80pct")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);

        // 补付窗口内 OCW 检测到买家补齐了 → R1: 自动结算
        assert_ok!(call_submit_underpaid_update(
            0,
            expected,
            make_tx_hashes(b"tx_topup_full")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Released);
        assert!(!NexMarket::pending_underpaid_trades().contains(&0));
    });
}

#[test]
fn underpaid_update_rejects_decrease() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let actual_80 = expected * 80 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_80,
            make_tx_hashes(b"tx_reject_decrease")
        ));

        // 尝试提交更低的金额 → validate_unsigned 拒绝 (non-increasing)
        let call_lower = crate::Call::<Test>::submit_underpaid_update {
            trade_id: 0,
            new_actual_amount: actual_80 - 1,
            new_tx_hashes: make_tx_hashes(b"tx_decrease_attempt"),
        };
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::Local,
            &call_lower,
        )
        .is_err());

        let (_, stored_amount) = NexMarket::ocw_verification_results(0).unwrap();
        assert_eq!(stored_amount, actual_80); // 金额未变
    });
}

#[test]
fn graduated_deposit_forfeit_light_underpay() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 少付 97%（轻微少付 → forfeit 20%）
        let actual_97 = expected * 97 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_97,
            make_tx_hashes(b"tx_light_underpay")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);

        let deadline: u64 = trade.underpaid_deadline.unwrap().into();
        System::set_block_number(deadline + 1);

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::finalize_underpaid(
            RuntimeOrigin::signed(CHARLIE),
            0
        ));

        // 97% ratio → forfeit rate 20% → 买家应该退还 80% 保证金
        // 退还的保证金 = deposit * 80% (unreserved 回 free balance)
        // Bob 还获得 97% 的 NEX
        let bob_after = Balances::free_balance(BOB);
        // Bob 净收入 = 97% NEX + 80% 保证金退还
        assert!(bob_after > bob_before);

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        // 97% payment → forfeit rate 20% (graduated) → PartiallyForfeited
        assert_eq!(trade.deposit_status, BuyerDepositStatus::PartiallyForfeited);
    });
}

#[test]
fn finalize_underpaid_full_topup_in_window() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 少付 60% → UnderpaidPending
        let actual_60 = expected * 60 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_60,
            make_tx_hashes(b"tx_topup_60pct")
        ));
        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::UnderpaidPending
        );

        // R1: 窗口内补齐到 100% → 自动结算
        assert_ok!(call_submit_underpaid_update(
            0,
            expected,
            make_tx_hashes(b"tx_topup_full_window")
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Released);
    });
}

#[test]
fn submit_underpaid_update_rejects_wrong_status() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 还在 AwaitingVerification，不是 UnderpaidPending

        let hashes = empty_tx_hashes();
        assert_noop!(
            NexMarket::submit_underpaid_update(RuntimeOrigin::none(), 0, 50_000_000, hashes),
            Error::<Test>::NotUnderpaidPending
        );
    });
}

#[test]
fn process_timeout_handles_underpaid_pending() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 少付 90% → UnderpaidPending
        let actual_90 = expected * 90 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_90,
            make_tx_hashes(b"tx_timeout_90pct")
        ));
        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::UnderpaidPending
        );

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deadline: u64 = trade.underpaid_deadline.unwrap().into();

        // 补付窗口未到期 → process_timeout 也失败
        assert_noop!(
            NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::UnderpaidGraceNotExpired
        );

        // 推进到窗口到期 → process_timeout 可以终裁
        System::set_block_number(deadline + 1);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert!(!NexMarket::pending_underpaid_trades().contains(&0));
    });
}

#[test]
fn auto_confirm_underpaid_routes_to_pending() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        // auto_confirm with 80% → 应该进入 UnderpaidPending（而非 AwaitingVerification）
        let actual_80 = expected * 80 / 100;
        assert_ok!(call_auto_confirm_payment(
            0,
            actual_80,
            make_tx_hashes(b"tx_autoconfirm_80pct")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);
        assert!(trade.underpaid_deadline.is_some());
        assert!(NexMarket::pending_underpaid_trades().contains(&0));
        // 不在 PendingUsdtTrades
        assert!(!NexMarket::pending_usdt_trades().contains(&0));
        // AwaitingPaymentTrades 已清理
        assert!(!NexMarket::awaiting_payment_trades().contains(&0));
    });
}

#[test]
fn auto_confirm_exact_routes_to_verification() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;

        // R1: auto_confirm exact → 直接结算
        assert_ok!(call_auto_confirm_payment(
            0,
            expected,
            make_tx_hashes(b"tx_autoconfirm_exact")
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

// ==================== 超时测试 ====================

#[test]
fn process_timeout_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // 模拟超时
        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        let timeout_block: u64 = trade.timeout_at.into();
        System::set_block_number(timeout_block + 1);

        let alice_before = Balances::free_balance(ALICE);

        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0,));

        // NEX 退还给 Alice + 50% 保证金补偿给卖家
        let half_deposit = deposit / 2;
        let seller_share = deposit - half_deposit; // 向上取整给 seller
        assert_eq!(
            Balances::free_balance(ALICE) - alice_before,
            nex + seller_share
        );
        assert_eq!(Balances::reserved_balance(ALICE), 0);

        // 订单回滚
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Open);
        assert_eq!(order.filled_amount, 0);

        // Trade 已退款
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Forfeited);
    });
}

#[test]
fn process_timeout_before_expiry_fails() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        // 不推进区块 → 超时检查失败
        assert_noop!(
            NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::InvalidTradeStatus
        );
    });
}

#[test]
fn process_timeout_awaiting_verification_grace_period_blocks() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        // 买家确认付款 → AwaitingVerification
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingVerification);
        let timeout_block: u64 = trade.timeout_at.into();

        // 超过 timeout 但在宽限期内 → StillInGracePeriod
        System::set_block_number(timeout_block + 1);
        assert_noop!(
            NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::StillInGracePeriod
        );

        // 超过 timeout + grace_period → 允许超时
        // grace = 600, timeout_block + 600 + 1
        System::set_block_number(timeout_block + 601);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
    });
}

#[test]
fn process_timeout_awaiting_verification_settles_if_ocw_result_exists() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let _trade = NexMarket::usdt_trades(0).unwrap();
        let bob_before = Balances::free_balance(BOB);
        assert_ok!(call_submit_ocw_result(
            0,
            50_000_000,
            make_tx_hashes(b"tx_timeout_settle")
        ));

        // 交易已在 submit_ocw_result 中自动完成
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Released);

        // BOB 收到 NEX
        assert!(Balances::free_balance(BOB) > bob_before);
    });
}

#[test]
fn process_timeout_awaiting_payment_no_grace_period() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        // 不调用 confirm_payment → 仍在 AwaitingPayment

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingPayment);
        let timeout_block: u64 = trade.timeout_at.into();

        // 超过 timeout 即可，不需要宽限期
        System::set_block_number(timeout_block + 1);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
    });
}

// ==================== 自动检测付款测试 ====================

#[test]
fn auto_confirm_payment_works() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // Trade 在 AwaitingPayment，且在跟踪队列中
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingPayment);
        assert!(NexMarket::awaiting_payment_trades().contains(&0));

        // OCW 检测到 USDT 已到账 → sidecar 调用 auto_confirm_payment
        assert_ok!(call_auto_confirm_payment(
            0,
            50_000_000,
            make_tx_hashes(b"tx_autoconfirm_works")
        ));

        // R1: auto_confirm_payment Exact → 直接结算
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert!(!NexMarket::awaiting_payment_trades().contains(&0));
    });
}

#[test]
fn auto_confirm_payment_rejects_non_awaiting_payment() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        // 买家手动确认 → 变为 AwaitingVerification
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // auto_confirm_payment 应失败（已不是 AwaitingPayment）
        assert_noop!(
            NexMarket::auto_confirm_payment(
                RuntimeOrigin::none(),
                0,
                50_000_000,
                make_tx_hashes(b"tx_reject_status")
            ),
            Error::<Test>::InvalidTradeStatus
        );
    });
}

#[test]
fn auto_confirm_payment_rejects_signed_origin() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // signed origin 应失败（只允许 unsigned）
        assert_noop!(
            NexMarket::auto_confirm_payment(
                RuntimeOrigin::signed(CHARLIE),
                0,
                50_000_000,
                make_tx_hashes(b"tx_reject_signed")
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn awaiting_payment_trades_tracking() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // 创建交易 → 加入跟踪队列
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert!(NexMarket::awaiting_payment_trades().contains(&0));

        // confirm_payment → 从跟踪队列移除
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert!(!NexMarket::awaiting_payment_trades().contains(&0));
    });
}

#[test]
fn awaiting_payment_trades_cleaned_on_timeout() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert!(NexMarket::awaiting_payment_trades().contains(&0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let timeout_block: u64 = trade.timeout_at.into();
        System::set_block_number(timeout_block + 1);

        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));
        assert!(!NexMarket::awaiting_payment_trades().contains(&0));
    });
}

// ==================== 价格保护测试 ====================

#[test]
fn set_initial_price_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000,));

        let config = NexMarket::price_protection().unwrap();
        assert_eq!(config.initial_price, Some(500_000));
        assert_eq!(NexMarket::last_trade_price(), Some(500_000));
    });
}

#[test]
fn configure_price_protection_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            true,
            3000,
            6000,
            50,
        ));

        let config = NexMarket::price_protection().unwrap();
        assert_eq!(config.max_price_deviation, 3000);
        assert_eq!(config.circuit_breaker_threshold, 6000);
        assert_eq!(config.min_trades_for_twap, 50);
    });
}

#[test]
fn price_deviation_blocks_extreme_orders() {
    new_test_ext().execute_with(|| {
        // 设置初始价格 0.5 USDT/NEX
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));

        // 正常范围内的价格可以挂单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            550_000,
            tron_address(),
            None, // +10%
        ));

        // 偏离超过 20% 默认阈值 → 被阻止
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                10_000_000_000_000,
                1_000_000,
                tron_address(),
                None, // +100%
            ),
            Error::<Test>::PriceDeviationTooHigh
        );
    });
}

// ==================== 市场统计测试 ====================

#[test]
fn market_stats_updated() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            50_000_000_000_000,
            500_000,
            buyer_tron(),
        ));

        let stats = NexMarket::market_stats();
        assert_eq!(stats.total_orders, 2);
    });
}

// ==================== 最优价格测试 ====================

#[test]
fn best_prices_updated() {
    new_test_ext().execute_with(|| {
        // Alice 卖 100 NEX @ 0.6
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            600_000,
            tron_address(),
            None,
        ));
        // Alice 卖 100 NEX @ 0.5
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        // Bob 买 100 NEX @ 0.4
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            100_000_000_000_000,
            400_000,
            buyer_tron(),
        ));
        // Bob 买 100 NEX @ 0.45
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            100_000_000_000_000,
            450_000,
            buyer_tron(),
        ));

        assert_eq!(NexMarket::best_ask(), Some(500_000)); // 最低卖价
        assert_eq!(NexMarket::best_bid(), Some(450_000)); // 最高买价
    });
}

// ==================== 多档判定测试 ====================

#[test]
fn payment_verification_result_categories() {
    new_test_ext().execute_with(|| {
        // 通过 OCW 提交结果间接测试多档判定
        // 精确付款
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));

        // R1: Exact → 自动结算
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_verify_categories")
        ));
        // R1: 自动结算后 OCW 结果已清理
        assert!(NexMarket::ocw_verification_results(0).is_none());
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

// ==================== seed_liquidity 测试 ====================

#[test]
fn seed_liquidity_works() {
    new_test_ext().execute_with(|| {
        setup_seed_price(); // InitialPrice=500_000, premium=20% → seed_price=600_000

        let seed_account: u64 = 96;
        let seed_before = Balances::free_balance(seed_account);

        // 挂 2 笔固定 10 USDT 订单（默认金额，usdt_override = None）
        // nex_per_order = 10_000_000 × 10^12 / 600_000 = 16_666_666_666_666
        assert_ok!(NexMarket::seed_liquidity(RuntimeOrigin::root(), 2, None,));

        let expected_nex_per_order: u128 = 10_000_000u128 * 1_000_000_000_000 / 600_000;
        let total_locked = expected_nex_per_order * 2;

        // 种子账户余额减少
        assert_eq!(
            Balances::free_balance(seed_account),
            seed_before - total_locked
        );

        // 最优卖价 = seed_price
        assert_eq!(NexMarket::best_ask(), Some(600_000));

        // 订单检查
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.usdt_price, 600_000);
        assert_eq!(order.nex_amount, expected_nex_per_order);
        assert!(order.deposit_waived);
    });
}

#[test]
fn seed_liquidity_usdt_override() {
    new_test_ext().execute_with(|| {
        setup_seed_price(); // seed_price = 600_000

        let seed_account: u64 = 96;
        let seed_before = Balances::free_balance(seed_account);

        // usdt_override = 20 USDT (20_000_000)
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            1,
            Some(20_000_000),
        ));

        // nex = 20_000_000 × 10^12 / 600_000 = 33_333_333_333_333
        let expected_nex: u128 = 20_000_000u128 * 1_000_000_000_000 / 600_000;
        assert_eq!(
            Balances::free_balance(seed_account),
            seed_before - expected_nex
        );

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, expected_nex);
    });
}

#[test]
fn seed_liquidity_rejects_no_price() {
    new_test_ext().execute_with(|| {
        // 不设置 InitialPrice → NoPriceReference
        assert_noop!(
            NexMarket::seed_liquidity(RuntimeOrigin::root(), 1, None,),
            Error::<Test>::NoPriceReference
        );
    });
}

// ==================== fund_seed_account 测试 ====================

#[test]
fn fund_seed_account_works() {
    new_test_ext().execute_with(|| {
        let seed_account: u64 = 96;
        let treasury: u64 = 99;
        let seed_before = Balances::free_balance(seed_account);
        let treasury_before = Balances::free_balance(treasury);

        let amount = 200_000_000_000_000u128; // 200 NEX
        assert_ok!(NexMarket::fund_seed_account(RuntimeOrigin::root(), amount));

        assert_eq!(Balances::free_balance(seed_account), seed_before + amount);
        assert_eq!(Balances::free_balance(treasury), treasury_before - amount);
    });
}

#[test]
fn fund_seed_account_rejects_zero() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::fund_seed_account(RuntimeOrigin::root(), 0),
            Error::<Test>::AmountTooSmall
        );
    });
}

#[test]
fn fund_seed_account_limited_only_by_treasury_balance() {
    new_test_ext().execute_with(|| {
        // Treasury = 1,000 NEX，无累计上限
        assert_ok!(NexMarket::fund_seed_account(
            RuntimeOrigin::root(),
            999_000_000_000_000u128
        ));
        assert_eq!(
            Balances::free_balance(96u64),
            500_000_000_000_000 + 999_000_000_000_000
        );
    });
}

// ==================== 免保证金首单测试 ====================

#[test]
fn waived_deposit_reserve_works_for_new_buyer() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // seed 一笔免保证金卖单 (100 NEX = 60 USDT @ seed_price 0.6)
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            1,
            Some(60_000_000),
        ));

        // DAVE (零余额) 可以吃免保证金卖单
        // 注意: DAVE 没有 NEX，但免保证金不需要锁定
        // 但 DAVE 需要存在于链上 (existential deposit)
        // 用 BOB 来测试免保证金（BOB 有余额但不应被扣保证金）
        let bob_reserved_before = Balances::reserved_balance(BOB);
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000u128),
            buyer_tron(),
        ));

        // BOB 无保证金被锁定
        assert_eq!(Balances::reserved_balance(BOB), bob_reserved_before);

        // trade 已创建
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.buyer_deposit, 0);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::None);

        // ActiveWaivedTrades 已记录
        assert_eq!(ActiveWaivedTrades::<Test>::get(BOB), 1);
    });
}

#[test]
fn waived_deposit_l2_one_active_limit() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // seed 2 笔免保证金卖单 (每笔 50 NEX = 30 USDT)
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            2,
            Some(30_000_000),
        ));

        // BOB 吃第一笔
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // BOB 尝试吃第二笔 → 被信用系统并发限制阻止（新用户限 1 笔活跃交易）
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 1, None, buyer_tron()),
            Error::<Test>::TradeLimitExceeded
        );
    });
}

#[test]
fn waived_deposit_l2_amount_cap() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // seed 一笔大额免保证金卖单 (200 NEX = 120 USDT)
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            1,
            Some(120_000_000),
        ));

        // BOB 尝试吃 150 NEX → 超过 MaxFirstOrderAmount (100 NEX)
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                0,
                Some(150_000_000_000_000u128),
                buyer_tron(),
            ),
            Error::<Test>::FirstOrderAmountTooLarge
        );

        // BOB 吃 100 NEX（上限）→ 成功
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(100_000_000_000_000u128),
            buyer_tron(),
        ));
    });
}

#[test]
fn waived_deposit_l2_short_timeout() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            1,
            Some(30_000_000),
        ));

        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        // 免保证金超时 = FirstOrderTimeout = 600 blocks (1h)
        // 标准超时 = UsdtTimeout = 7200 blocks (12h)
        let expected_timeout: u64 = 600;
        assert_eq!(trade.timeout_at, expected_timeout);
    });
}

#[test]
fn waived_deposit_l3_completed_buyer_blocked() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // seed 2 笔 (每笔 50 NEX = 30 USDT)
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            2,
            Some(30_000_000),
        ));

        // BOB 吃第一笔
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // BOB 完成交易流程
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));
        let trade = NexMarket::usdt_trades(0).unwrap();
        let tx_h = make_tx_hashes(b"tx_waived_complete");
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            trade.usdt_amount,
            tx_h.clone(),
        ));

        // L3: BOB 已标记为 CompletedBuyer
        assert!(CompletedBuyers::<Test>::get(BOB));
        // ActiveWaivedTrades 已清零
        assert_eq!(ActiveWaivedTrades::<Test>::get(BOB), 0);

        // BOB 尝试再次吃免保证金卖单 → 被 L3 阻止
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 1, None, buyer_tron()),
            Error::<Test>::BuyerAlreadyCompleted
        );
    });
}

#[test]
fn waived_deposit_timeout_cleans_counter() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        assert_ok!(NexMarket::seed_liquidity(
            RuntimeOrigin::root(),
            2,
            Some(30_000_000),
        ));

        // BOB 吃第一笔
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_eq!(ActiveWaivedTrades::<Test>::get(BOB), 1);

        // 超时
        let trade = NexMarket::usdt_trades(0).unwrap();
        let timeout_block: u64 = trade.timeout_at.into();
        System::set_block_number(timeout_block + 1);

        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0,));

        // 超时后计数器归零，BOB 未被标记为 CompletedBuyer
        assert_eq!(ActiveWaivedTrades::<Test>::get(BOB), 0);
        assert!(!CompletedBuyers::<Test>::get(BOB));

        // 信用系统：超时后 BOB 信用分 = 450（500 - 50），低于 500 无法再交易
        let profile = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(profile.credit_score, 450);
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 1, None, buyer_tron()),
            Error::<Test>::BuyerSuspended
        );
    });
}

// cumulative_seed_usdt_sold 已移除（R2: 可通过事件日志链下计算）

#[test]
fn on_idle_advances_twap_snapshots() {
    new_test_ext().execute_with(|| {
        use frame_support::traits::Hooks;

        // 设置初始价格并完成一笔交易来初始化 TWAP
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_twap_snapshots")
        ));

        // 交易完成后，记录当前 TWAP 累积器状态
        let acc_before = NexMarket::twap_accumulator().unwrap();
        let cum_before = acc_before.current_cumulative;

        // 前进 700 区块（> hour_interval=100, > bph=600）但不做任何交易
        let new_block = acc_before.current_block as u64 + 700;
        System::set_block_number(new_block);

        // 调用 on_idle
        NexMarket::on_idle(new_block, Weight::from_parts(1_000_000_000, 1_000_000));

        // 检查累积器已推进
        let acc_after = NexMarket::twap_accumulator().unwrap();
        assert_eq!(acc_after.current_block, new_block as u32);
        assert!(acc_after.current_cumulative > cum_before);

        // 1h checkpoint 应已轮换（700 >= bph=600）: 旧 curr → prev, 新 curr 在当前块
        assert_eq!(acc_after.hour_prev, acc_before.hour_curr);
        assert_eq!(acc_after.hour_curr.block_number, new_block as u32);

        // 24h checkpoint 不应轮换（700 < bpd=14400）
        assert_eq!(acc_after.day_curr, acc_before.day_curr);

        // 再次 on_idle（同一区块）→ 累积器 current_block 不变
        let cum_mid = acc_after.current_cumulative;
        NexMarket::on_idle(new_block, Weight::from_parts(1_000_000_000, 1_000_000));
        let acc_same = NexMarket::twap_accumulator().unwrap();
        assert_eq!(acc_same.current_cumulative, cum_mid); // 没有变化
    });
}

// ==================== v3: 参考价 initial_price → TWAP 切换修复 ====================

/// 回归测试: 成交量与历史覆盖达标后，价格偏离检查的参考价应从
/// initial_price 切换到 1h TWAP（修复前该切换不可达）
#[test]
fn reference_price_switches_from_initial_price_to_twap() {
    new_test_ext().execute_with(|| {
        // min_trades_for_twap = 1, max_deviation = 20%
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            true,
            2000,
            5000,
            1
        ));
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));

        // 一笔真实成交 @550_000（偏离 initial 500_000 仅 10%，通过）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            550_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0,));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_twap_switch")
        ));

        let acc = NexMarket::twap_accumulator().unwrap();
        assert_eq!(acc.last_price, 550_000);
        let first_trade_block = acc.first_trade_block;
        assert!(first_trade_block > 0);

        // 历史覆盖不足 1h → 参考价仍为 initial_price(500_000)
        // 630_000 偏离 26% > 20% → 拒绝
        assert!(matches!(
            NexMarket::check_price_deviation(630_000),
            Err(Error::<Test>::PriceDeviationTooHigh)
        ));

        // 覆盖 >= BlocksPerHour(600) → 参考价切换为 1h TWAP(≈550_000)
        // 630_000 偏离 TWAP 仅 ~14.5% < 20% → 通过（修复前会一直按 500_000 拒绝）
        System::set_block_number(first_trade_block as u64 + 601);
        assert!(NexMarket::check_price_deviation(630_000).is_ok());

        // 偏离 TWAP 超限仍应拒绝: 680_000 偏离 550_000 约 23.6% > 20%
        assert!(matches!(
            NexMarket::check_price_deviation(680_000),
            Err(Error::<Test>::PriceDeviationTooHigh)
        ));
    });
}

/// v3 迁移: 旧版 TwapAccumulator 应正确转换为双 checkpoint 结构
#[test]
fn v3_migration_translates_old_accumulator() {
    use crate::migrations::v3::{migrate, OldTwapAccumulator};
    use codec::Encode;

    new_test_ext().execute_with(|| {
        let old = OldTwapAccumulator {
            current_cumulative: 5000,
            current_block: 900,
            last_price: 120,
            trade_count: 7,
            hour_snapshot: PriceSnapshot {
                cumulative_price: 4000,
                block_number: 850,
            },
            day_snapshot: PriceSnapshot {
                cumulative_price: 3000,
                block_number: 800,
            },
            week_snapshot: PriceSnapshot {
                cumulative_price: 1000,
                block_number: 100,
            },
            last_hour_update: 850,
            last_day_update: 800,
            last_week_update: 100,
        };
        frame_support::storage::unhashed::put_raw(
            &TwapAccumulatorStore::<Test>::hashed_key(),
            &old.encode(),
        );

        migrate::<Test>();

        let acc = NexMarket::twap_accumulator().unwrap();
        assert_eq!(acc.current_cumulative, 5000);
        assert_eq!(acc.current_block, 900);
        assert_eq!(acc.last_price, 120);
        assert_eq!(acc.trade_count, 7);
        // first_trade_block = 各已知区块号的最小值 = week_snapshot(100)
        assert_eq!(acc.first_trade_block, 100);
        // 旧快照同时作为 prev 与 curr
        assert_eq!(acc.hour_prev.block_number, 850);
        assert_eq!(acc.hour_curr.cumulative_price, 4000);
        assert_eq!(acc.day_curr.block_number, 800);
        assert_eq!(acc.week_curr.block_number, 100);
    });
}

#[test]
fn on_idle_noop_without_twap_data() {
    new_test_ext().execute_with(|| {
        use frame_support::traits::Hooks;

        // 没有任何交易，TWAP 累积器不存在
        System::set_block_number(100);
        NexMarket::on_idle(100, Weight::from_parts(1_000_000_000, 1_000_000));
        // TWAP 仍不存在
        assert!(NexMarket::twap_accumulator().is_none());
    });
}

// ==================== 审计修复回归测试 ====================

#[test]
fn m2_reward_paid_event_tracks_success() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_m2_reward_event")
        ));

        // R1: submit_ocw_result 自动结算，不再需要 claim，验证交易已完成
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

#[test]
fn m2_reward_paid_false_when_source_empty() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_m2_source_empty")
        ));

        // R1: 自动结算，验证交易已完成
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

#[test]
fn l1_query_filters_expired_orders() {
    new_test_ext().execute_with(|| {
        // 创建卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        // 创建买单
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            100_000_000_000_000,
            400_000,
            buyer_tron(),
        ));

        // 当前应该有 1 卖单 + 1 买单
        assert_eq!(NexMarket::get_sell_order_list().len(), 1);
        assert_eq!(NexMarket::get_buy_order_list().len(), 1);

        // 推进到订单过期后 (DefaultOrderTTL=14400)
        System::set_block_number(14401);

        // 查询应返回空列表（过期订单被过滤）
        assert_eq!(NexMarket::get_sell_order_list().len(), 0);
        assert_eq!(NexMarket::get_buy_order_list().len(), 0);
    });
}

#[test]
fn h2_weight_values_are_realistic() {
    // 验证权重值在合理范围 (10M ~ 500M ref_time)
    use crate::weights::WeightInfo;
    let weights: Vec<Weight> = vec![
        <() as WeightInfo>::place_sell_order(),
        <() as WeightInfo>::place_buy_order(),
        <() as WeightInfo>::cancel_order(),
        <() as WeightInfo>::reserve_sell_order(),
        <() as WeightInfo>::accept_buy_order(),
        <() as WeightInfo>::confirm_payment(),
        <() as WeightInfo>::process_timeout(),
        <() as WeightInfo>::submit_ocw_result(),
        <() as WeightInfo>::claim_reward(),
        <() as WeightInfo>::configure_price_protection(),
        <() as WeightInfo>::set_initial_price(),
        <() as WeightInfo>::lift_circuit_breaker(),
        <() as WeightInfo>::fund_seed_account(),
        <() as WeightInfo>::seed_liquidity(),
        <() as WeightInfo>::auto_confirm_payment(),
        <() as WeightInfo>::submit_underpaid_update(),
        <() as WeightInfo>::finalize_underpaid(),
    ];

    for (i, w) in weights.iter().enumerate() {
        let ref_time = w.ref_time();
        assert!(
            ref_time >= 10_000_000,
            "Weight {} ref_time too low: {}",
            i,
            ref_time
        );
        assert!(
            ref_time <= 500_000_000,
            "Weight {} ref_time too high: {}",
            i,
            ref_time
        );
        let proof_size = w.proof_size();
        assert!(
            proof_size >= 1_000,
            "Weight {} proof_size too low: {}",
            i,
            proof_size
        );
        assert!(
            proof_size <= 100_000,
            "Weight {} proof_size too high: {}",
            i,
            proof_size
        );
    }
}

// ==================== C3: tx_hash 防重放测试 ====================

/// 辅助：创建完整交易流程到 AwaitingVerification 状态
fn setup_trade_awaiting_verification() -> (u64, u64) {
    let nex = 100_000_000_000_000u128;
    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(ALICE),
        nex,
        500_000,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(BOB),
        0,
        None,
        buyer_tron(),
    ));
    let trade = NexMarket::usdt_trades(0).unwrap();
    assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
    (0, trade.usdt_amount)
}

#[test]
fn c3_submit_ocw_result_records_tx_hash() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        let tx_hash: TxHash = b"abc123def456".to_vec().try_into().unwrap();

        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            trade_id,
            usdt_amount,
            make_tx_hashes(b"abc123def456"),
        ));

        // tx_hash 已记录，映射到 (trade_id, block_number)
        assert!(NexMarket::used_tx_hashes(&tx_hash).is_some());
        assert_eq!(NexMarket::used_tx_hashes(&tx_hash).unwrap().0, trade_id);
    });
}

#[test]
fn c3_submit_ocw_result_rejects_replay() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // 第一笔交易：正常提交
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            trade.usdt_amount,
            make_tx_hashes(b"replay_hash_001"),
        ));

        // 第二笔交易：尝试用同一 tx_hash
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        let trade2 = NexMarket::usdt_trades(1).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 1));

        // 重放攻击 → 被 C3 防重放拒绝
        assert_noop!(
            NexMarket::submit_ocw_result(
                RuntimeOrigin::none(),
                1,
                trade2.usdt_amount,
                make_tx_hashes(b"replay_hash_001"),
            ),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

#[test]
fn b1_submit_ocw_result_empty_tx_hashes_rejected() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        // B1: 空 tx_hashes 应被拒绝（TxHashRequired）
        assert_noop!(
            NexMarket::submit_ocw_result(
                RuntimeOrigin::none(),
                trade_id,
                usdt_amount,
                empty_tx_hashes(),
            ),
            Error::<Test>::TxHashRequired
        );
    });
}

#[test]
fn c3_different_tx_hash_accepted() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let hash1: TxHash = b"tx_hash_aaa".to_vec().try_into().unwrap();
        let hash2: TxHash = b"tx_hash_bbb".to_vec().try_into().unwrap();

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let t1 = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            t1.usdt_amount,
            make_tx_hashes(b"tx_hash_aaa"),
        ));

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        let t2 = NexMarket::usdt_trades(1).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 1));

        // 不同 tx_hash → 正常通过
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            1,
            t2.usdt_amount,
            make_tx_hashes(b"tx_hash_bbb"),
        ));

        assert_eq!(NexMarket::used_tx_hashes(&hash1).unwrap().0, 0);
        assert_eq!(NexMarket::used_tx_hashes(&hash2).unwrap().0, 1);
    });
}

#[test]
fn c3_auto_confirm_payment_rejects_replay() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // 第一笔：auto_confirm 消耗 tx_hash
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::auto_confirm_payment(
            RuntimeOrigin::none(),
            0,
            trade.usdt_amount,
            make_tx_hashes(b"auto_confirm_replay")
        ));

        // 第二笔：尝试重放同一 tx_hash → 失败
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        assert_noop!(
            NexMarket::auto_confirm_payment(
                RuntimeOrigin::none(),
                1,
                trade.usdt_amount,
                make_tx_hashes(b"auto_confirm_replay")
            ),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

#[test]
fn c3_cross_extrinsic_replay_blocked() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // auto_confirm 消耗 tx_hash
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::auto_confirm_payment(
            RuntimeOrigin::none(),
            0,
            trade.usdt_amount,
            make_tx_hashes(b"cross_extrinsic_hash")
        ));

        // submit_ocw_result 尝试用同一 tx_hash → 也被拒绝
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 1));

        assert_noop!(
            NexMarket::submit_ocw_result(
                RuntimeOrigin::none(),
                1,
                trade.usdt_amount,
                make_tx_hashes(b"cross_extrinsic_hash"),
            ),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

// ==================== C4+M3: validate_unsigned 安全加固测试 ====================

#[test]
fn c4_submit_ocw_result_rejects_excessive_amount() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        // M-7: 推进区块以满足最小交易年龄检查
        System::set_block_number(System::block_number() + 11);

        // 超过 3 倍金额上限 → validate_unsigned 拒绝 (Custom(14)) (M-8: 10x→3x)
        let excessive = usdt_amount * 3 + 1;
        let call = crate::Call::<Test>::submit_ocw_result {
            trade_id,
            actual_amount: excessive,
            tx_hashes: make_tx_hashes(b"tx_validate_excessive"),
        };
        let result =
            <NexMarket as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call);
        assert!(result.is_err());

        // 恰好 3 倍 → 应该通过 validate_unsigned (M-8: 10x→3x)
        let at_cap = usdt_amount * 3;
        let at_cap_hashes = make_tx_hashes(b"tx_validate_at_cap");
        let call_ok = crate::Call::<Test>::submit_ocw_result {
            trade_id,
            actual_amount: at_cap,
            tx_hashes: at_cap_hashes,
        };
        let result_ok =
            <NexMarket as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call_ok);
        assert!(result_ok.is_ok());
    });
}

#[test]
fn c4_auto_confirm_rejects_excessive_amount() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();

        let excessive = trade.usdt_amount * 10 + 1;
        let call = crate::Call::<Test>::auto_confirm_payment {
            trade_id: 0,
            actual_amount: excessive,
            tx_hashes: make_tx_hashes(b"tx_auto_excessive"),
        };
        let result =
            <NexMarket as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call);
        assert!(result.is_err());
    });
}

#[test]
fn c4_underpaid_update_rejects_excessive_amount() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 少付 → UnderpaidPending
        let actual_80 = expected * 80 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_80,
            make_tx_hashes(b"tx_c4_underpaid_ex")
        ));

        // 超过 3x → validate_unsigned 拒绝 (M-8: 10x→3x)
        let excessive = expected * 3 + 1;
        let call = crate::Call::<Test>::submit_underpaid_update {
            trade_id: 0,
            new_actual_amount: excessive,
            new_tx_hashes: empty_tx_hashes(),
        };
        let result =
            <NexMarket as ValidateUnsigned>::validate_unsigned(TransactionSource::Local, &call);
        assert!(result.is_err());
    });
}

#[test]
fn c4_underpaid_update_rejects_non_increasing_amount() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let actual_80 = expected * 80 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_80,
            make_tx_hashes(b"tx_c4_non_incr")
        ));

        // 同金额 → validate_unsigned 拒绝 (Custom(33))
        let call_same = crate::Call::<Test>::submit_underpaid_update {
            trade_id: 0,
            new_actual_amount: actual_80,
            new_tx_hashes: empty_tx_hashes(),
        };
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::Local,
            &call_same,
        )
        .is_err());

        // 更低金额 → 同样拒绝
        let call_lower = crate::Call::<Test>::submit_underpaid_update {
            trade_id: 0,
            new_actual_amount: actual_80 - 1,
            new_tx_hashes: empty_tx_hashes(),
        };
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::Local,
            &call_lower,
        )
        .is_err());

        // 更高金额 → 通过
        let higher_hashes = make_tx_hashes(b"tx_c4_higher");
        let call_higher = crate::Call::<Test>::submit_underpaid_update {
            trade_id: 0,
            new_actual_amount: actual_80 + 1,
            new_tx_hashes: higher_hashes,
        };
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::Local,
            &call_higher,
        )
        .is_ok());
    });
}

#[test]
fn m3_validate_unsigned_rejects_external_source() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        // M-7: 推进区块以满足最小交易年龄检查
        System::set_block_number(System::block_number() + 11);

        // submit_ocw_result from External → rejected
        let ext_hashes = make_tx_hashes(b"tx_external_source");
        let call1 = crate::Call::<Test>::submit_ocw_result {
            trade_id,
            actual_amount: usdt_amount,
            tx_hashes: ext_hashes,
        };
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::External,
            &call1,
        )
        .is_err());

        // Same call from Local → accepted
        assert!(<NexMarket as ValidateUnsigned>::validate_unsigned(
            TransactionSource::Local,
            &call1,
        )
        .is_ok());
    });
}

// ==================== H3: 过期订单 GC 测试 ====================

#[test]
fn h3_on_idle_gc_cleans_expired_sell_orders() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::sell_orders().len(), 1);
        let alice_reserved = Balances::reserved_balance(ALICE);
        assert!(alice_reserved > 0);

        // 推进到订单过期后
        let order = NexMarket::orders(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 执行 on_idle → 应清理过期卖单
        NexMarket::on_idle(order.expires_at + 1, Weight::from_parts(u64::MAX, u64::MAX));

        // 验证：卖单已从索引移除
        assert_eq!(NexMarket::sell_orders().len(), 0);
        // 验证：订单状态标记为 Expired
        let expired_order = NexMarket::orders(0).unwrap();
        assert_eq!(expired_order.status, OrderStatus::Expired);
        // 验证：锁定资产已退还
        assert_eq!(Balances::reserved_balance(ALICE), 0);
        // 验证：用户订单索引已清理
        assert_eq!(NexMarket::user_orders(ALICE).len(), 0);
    });
}

#[test]
fn h3_on_idle_gc_cleans_expired_buy_orders() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            price,
            buyer_tron(),
        ));
        assert_eq!(NexMarket::buy_orders().len(), 1);
        let bob_reserved = Balances::reserved_balance(BOB);
        assert!(bob_reserved > 0);

        let order = NexMarket::orders(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        NexMarket::on_idle(order.expires_at + 1, Weight::from_parts(u64::MAX, u64::MAX));

        assert_eq!(NexMarket::buy_orders().len(), 0);
        let expired_order = NexMarket::orders(0).unwrap();
        assert_eq!(expired_order.status, OrderStatus::Expired);
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

#[test]
fn h3_on_idle_gc_respects_max_per_block() {
    new_test_ext().execute_with(|| {
        let nex = 10_000_000_000_000u128; // 10 NEX each
        let price = 500_000u64;

        // 创建 15 个卖单（MaxExpiredOrdersPerBlock=10）
        for _ in 0..15 {
            assert_ok!(NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                nex,
                price,
                tron_address(),
                None,
            ));
        }
        assert_eq!(NexMarket::sell_orders().len(), 15);

        let order = NexMarket::orders(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 第一次 on_idle：最多清理 10 个
        NexMarket::on_idle(order.expires_at + 1, Weight::from_parts(u64::MAX, u64::MAX));
        assert_eq!(NexMarket::sell_orders().len(), 5);

        // 第二次 on_idle：清理剩余 5 个
        NexMarket::on_idle(order.expires_at + 2, Weight::from_parts(u64::MAX, u64::MAX));
        assert_eq!(NexMarket::sell_orders().len(), 0);
    });
}

// ==================== H4: 增量 best prices 测试 ====================

#[test]
fn h4_best_prices_update_incrementally_on_new_order() {
    new_test_ext().execute_with(|| {
        let nex = 10_000_000_000_000u128;

        // 第一个卖单 → 设置 best ask
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(500_000));

        // 更低价卖单 → best ask 应更新
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            400_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(400_000));

        // 更高价卖单 → best ask 不变
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            600_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(400_000));
    });
}

#[test]
fn h4_best_prices_rescan_on_best_order_cancel() {
    new_test_ext().execute_with(|| {
        let nex = 10_000_000_000_000u128;

        // 挂两个卖单: 400_000 和 500_000
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            400_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(400_000));

        // 取消最优卖单 → 应重扫找到 500_000
        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));
        assert_eq!(NexMarket::best_ask(), Some(500_000));

        // 取消最后一个 → best ask 应清空
        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(ALICE), 1));
        assert_eq!(NexMarket::best_ask(), None);
    });
}

#[test]
fn h4_cancel_non_best_order_no_rescan() {
    new_test_ext().execute_with(|| {
        let nex = 10_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            400_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(400_000));

        // 取消非最优卖单 → best ask 不应变
        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(ALICE), 1));
        assert_eq!(NexMarket::best_ask(), Some(400_000));
    });
}

// 🆕 L1修复: 添加缺失的 #[test] 注解
#[test]
fn normal_sell_order_still_requires_deposit() {
    new_test_ext().execute_with(|| {
        // Alice 正常挂卖单（非 seed_liquidity）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert!(!order.deposit_waived);

        // BOB 吃这个正常卖单 → 照常扣保证金
        let bob_reserved_before = Balances::reserved_balance(BOB);
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // 保证金已锁定
        assert!(Balances::reserved_balance(BOB) > bob_reserved_before);
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert!(!trade.buyer_deposit.is_zero());
    });
}

// ==================== Phase 6: M6/M4/M7/M2 回归测试 ====================

#[test]
fn m6_order_id_overflow_rejected() {
    new_test_ext().execute_with(|| {
        // 将 NextOrderId 设为 u64::MAX
        NextOrderId::<Test>::put(u64::MAX);

        // 尝试挂卖单 → 应返回 ArithmeticOverflow
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                1_000_000,
                tron_address(),
                None,
            ),
            Error::<Test>::ArithmeticOverflow,
        );
    });
}

#[test]
fn m6_trade_id_overflow_rejected() {
    new_test_ext().execute_with(|| {
        // 先正常挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            1_000_000,
            tron_address(),
            None,
        ));

        // 将 NextUsdtTradeId 设为 u64::MAX
        NextUsdtTradeId::<Test>::put(u64::MAX);

        // 尝试预锁定 → 应返回 ArithmeticOverflow（do_create_usdt_trade_ex 内部）
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 0, None, buyer_tron(),),
            Error::<Test>::ArithmeticOverflow,
        );
    });
}

#[test]
fn m4_extreme_deviation_saturates_instead_of_truncating() {
    new_test_ext().execute_with(|| {
        // 设置初始价格 = 1 USDT
        assert_ok!(NexMarket::set_initial_price(
            RuntimeOrigin::root(),
            1_000_000
        ));

        // 启用价格保护，max_deviation = 50% (5000 bps)
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            true,
            5000,
            5000,
            0,
        ));

        // 挂一个价格 = 100 USDT (10000% 偏离) 的卖单
        // 如果 u16 截断: 100_000_000 bps → 100_000_000 % 65536 = 34464 bps < 65535
        // 但 saturating: min(100_000_000, 65535) = 65535 > 5000 → PriceDeviationTooHigh
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                100_000_000,
                tron_address(),
                None,
            ),
            Error::<Test>::PriceDeviationTooHigh,
        );
    });
}

#[test]
fn m7_tx_hash_gc_cleans_expired_entries() {
    new_test_ext().execute_with(|| {
        // 在区块 1 插入一条 tx_hash
        System::set_block_number(1);
        let hash: TxHash = b"gc_test_hash_001".to_vec().try_into().unwrap();
        UsedTxHashes::<Test>::insert(&hash, (42u64, 1u64));

        // 在 TTL 之前 → on_idle 不清理
        System::set_block_number(100);
        NexMarket::on_idle(100u64, Weight::from_parts(u64::MAX, u64::MAX));
        assert!(UsedTxHashes::<Test>::contains_key(&hash));

        // 跳到 TTL 之后 (TxHashTtlBlocks = 100800)
        System::set_block_number(100802);

        NexMarket::on_idle(100802u64, Weight::from_parts(u64::MAX, u64::MAX));

        // tx_hash 应已被清理
        assert!(!UsedTxHashes::<Test>::contains_key(&hash));
    });
}

// ==================== P0 #6: 紧急暂停市场 ====================

#[test]
fn force_pause_market_blocks_trading() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // 暂停市场
        assert_ok!(NexMarket::force_pause_market(RuntimeOrigin::root()));
        assert!(NexMarket::market_paused());

        // 挂单被拒绝
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                nex,
                price,
                tron_address(),
                None
            ),
            Error::<Test>::MarketIsPaused,
        );

        // 恢复后可以挂单
        assert_ok!(NexMarket::force_resume_market(RuntimeOrigin::root()));
        assert!(!NexMarket::market_paused());
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
    });
}

#[test]
fn force_pause_blocks_buy_order() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        assert_ok!(NexMarket::force_pause_market(RuntimeOrigin::root()));

        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                100_000_000_000_000u128,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::MarketIsPaused,
        );
    });
}

#[test]
fn force_pause_blocks_reserve_sell_order() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // 先挂单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // 暂停后无法吃单
        assert_ok!(NexMarket::force_pause_market(RuntimeOrigin::root()));
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 0, None, buyer_tron(),),
            Error::<Test>::MarketIsPaused,
        );
    });
}

// ==================== P0 #7: 管理员强制结算/取消交易 ====================

/// 辅助函数：创建一个 AwaitingPayment 状态的交易
fn setup_awaiting_payment_trade() -> u64 {
    setup_seed_price();
    let nex = 100_000_000_000_000u128;
    let price = 500_000u64;

    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(ALICE),
        nex,
        price,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(BOB),
        0,
        None,
        buyer_tron(),
    ));
    0 // trade_id
}

/// W6 helper: 创建一个买家已确认付款后超时退款的交易（payment_confirmed=true, Refunded）
fn setup_confirmed_then_refunded_trade() -> u64 {
    let trade_id = setup_awaiting_payment_trade();
    assert_ok!(NexMarket::confirm_payment(
        RuntimeOrigin::signed(BOB),
        trade_id
    ));
    // AwaitingVerification timeout needs: now > timeout_at + grace_period
    // timeout_at = 1 + 7200 = 7201, grace = 600 → need block > 7801
    System::set_block_number(8000);
    assert_ok!(NexMarket::process_timeout(
        RuntimeOrigin::signed(BOB),
        trade_id
    ));
    let trade = NexMarket::usdt_trades(trade_id).unwrap();
    assert_eq!(trade.status, UsdtTradeStatus::Refunded);
    assert!(trade.payment_confirmed);
    trade_id
}

#[test]
fn force_cancel_trade_refunds_both_parties() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();

        let alice_reserved_before = Balances::reserved_balance(ALICE);
        let bob_reserved_before = Balances::reserved_balance(BOB);

        assert_ok!(NexMarket::force_cancel_trade(
            RuntimeOrigin::root(),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);

        // NEX 退还给卖家
        assert!(Balances::reserved_balance(ALICE) < alice_reserved_before);
        // 保证金退还给买家
        assert!(Balances::reserved_balance(BOB) < bob_reserved_before);
    });
}

#[test]
fn force_cancel_trade_rejects_completed() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();

        // 强制取消
        assert_ok!(NexMarket::force_cancel_trade(
            RuntimeOrigin::root(),
            trade_id
        ));

        // 不能重复取消
        assert_noop!(
            NexMarket::force_cancel_trade(RuntimeOrigin::root(), trade_id),
            Error::<Test>::InvalidTradeStatus,
        );
    });
}

#[test]
fn force_settle_trade_release_to_buyer() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();

        // confirm_payment 进入 AwaitingVerification
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        let buyer_free_before = Balances::free_balance(BOB);

        assert_ok!(NexMarket::force_settle_trade(
            RuntimeOrigin::root(),
            trade_id,
            50_000_000,
            DisputeResolution::ReleaseToBuyer,
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        // 买家收到 NEX
        assert!(Balances::free_balance(BOB) > buyer_free_before);
    });
}

#[test]
fn force_settle_trade_refund_to_seller() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        let alice_reserved_before = Balances::reserved_balance(ALICE);

        assert_ok!(NexMarket::force_settle_trade(
            RuntimeOrigin::root(),
            trade_id,
            0,
            DisputeResolution::RefundToSeller,
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
        // NEX 退还给卖家
        assert!(Balances::reserved_balance(ALICE) < alice_reserved_before);
    });
}

// ==================== P0 #1: 争议仲裁 ====================

#[test]
fn dispute_trade_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        let evidence = b"QmTestCid12345".to_vec();
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            evidence.clone(),
        ));

        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::Open);
        assert_eq!(dispute.initiator, BOB);
    });
}

#[test]
fn dispute_trade_rejects_non_participant() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(CHARLIE), trade_id, b"QmTest".to_vec(),),
            Error::<Test>::NotTradeParticipant,
        );
    });
}

#[test]
fn dispute_trade_rejects_non_refunded() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(BOB), trade_id, b"QmTest".to_vec(),),
            Error::<Test>::TradeNotDisputable,
        );
    });
}

#[test]
fn dispute_trade_rejects_duplicate() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(ALICE), trade_id, b"QmTest2".to_vec(),),
            Error::<Test>::TradeAlreadyDisputed,
        );
    });
}

#[test]
fn resolve_dispute_release_to_buyer() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        let buyer_free_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            trade_id,
            DisputeResolution::ReleaseToBuyer,
        ));

        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::ResolvedForBuyer);
        assert!(Balances::free_balance(BOB) > buyer_free_before);
    });
}

#[test]
fn resolve_dispute_refund_to_seller() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            trade_id,
            DisputeResolution::RefundToSeller,
        ));

        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::ResolvedForSeller);
    });
}

#[test]
fn resolve_dispute_rejects_already_closed() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));
        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            trade_id,
            DisputeResolution::RefundToSeller,
        ));

        assert_noop!(
            NexMarket::resolve_dispute(
                RuntimeOrigin::root(),
                trade_id,
                DisputeResolution::ReleaseToBuyer,
            ),
            Error::<Test>::DisputeAlreadyClosed,
        );
    });
}

// ==================== P1 #9: 手续费 ====================

#[test]
fn set_trading_fee_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(NexMarket::trading_fee_bps(), 0);
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100)); // 1%
        assert_eq!(NexMarket::trading_fee_bps(), 100);
    });
}

#[test]
fn set_trading_fee_rejects_over_max() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::set_trading_fee(RuntimeOrigin::root(), 1001),
            Error::<Test>::FeeTooHigh,
        );
    });
}

// ==================== P1 #4: 最低交易金额 ====================

#[test]
fn place_sell_order_rejects_below_minimum() {
    new_test_ext().execute_with(|| {
        // MinOrderNexAmount = 1 NEX = 1_000_000_000_000
        let too_small = 999_999_999_999u128;
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                too_small,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::OrderAmountBelowMinimum,
        );
    });
}

#[test]
fn place_buy_order_rejects_below_minimum() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let too_small = 999_999_999_999u128;
        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                too_small,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::OrderAmountBelowMinimum,
        );
    });
}

// ==================== P2 #3: 订单修改 ====================

#[test]
fn update_order_price_works() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        let new_price = 600_000u64;
        assert_ok!(NexMarket::update_order_price(
            RuntimeOrigin::signed(ALICE),
            0,
            new_price,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.usdt_price, new_price);
    });
}

#[test]
fn update_order_price_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        assert_noop!(
            NexMarket::update_order_price(RuntimeOrigin::signed(BOB), 0, 600_000),
            Error::<Test>::NotOrderOwner,
        );
    });
}

#[test]
fn update_order_price_rejects_when_paused() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::force_pause_market(RuntimeOrigin::root()));

        assert_noop!(
            NexMarket::update_order_price(RuntimeOrigin::signed(ALICE), 0, 600_000),
            Error::<Test>::MarketIsPaused,
        );
    });
}

// ==================== P1 #8: 保证金动态汇率（已废弃，改为实时市场价格） ====================

#[test]
fn update_deposit_exchange_rate_deprecated() {
    new_test_ext().execute_with(|| {
        // 该 extrinsic 已废弃，调用应返回错误
        assert_noop!(
            NexMarket::update_deposit_exchange_rate(RuntimeOrigin::root(), 2_000_000),
            sp_runtime::DispatchError::Other("deprecated")
        );
    });
}

// ==================== P1 #2/#12: 交易历史+索引 ====================

#[test]
fn user_trades_indexed_on_create() {
    new_test_ext().execute_with(|| {
        let _trade_id = setup_awaiting_payment_trade();

        // 卖家和买家都应该有交易记录
        let alice_trades = NexMarket::user_trades(ALICE);
        assert_eq!(alice_trades.len(), 1);
        assert_eq!(alice_trades[0], 0);

        let bob_trades = NexMarket::user_trades(BOB);
        assert_eq!(bob_trades.len(), 1);
        assert_eq!(bob_trades[0], 0);
    });
}

#[test]
fn order_trades_indexed_on_create() {
    new_test_ext().execute_with(|| {
        let _trade_id = setup_awaiting_payment_trade();

        let trades = NexMarket::order_trades(0u64);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0], 0);
    });
}

#[test]
fn get_user_trade_list_returns_trades() {
    new_test_ext().execute_with(|| {
        let _trade_id = setup_awaiting_payment_trade();

        let trades = NexMarket::get_user_trade_list(&BOB);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade_id, 0);
    });
}

#[test]
fn get_trades_by_order_works() {
    new_test_ext().execute_with(|| {
        let _trade_id = setup_awaiting_payment_trade();

        let trades = NexMarket::get_trades_by_order(0);
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].trade_id, 0);
    });
}

#[test]
fn get_active_trades_filters_correctly() {
    new_test_ext().execute_with(|| {
        let _trade_id = setup_awaiting_payment_trade();

        // AwaitingPayment → 活跃
        let active = NexMarket::get_active_trades(&BOB);
        assert_eq!(active.len(), 1);

        // 超时后 → 不再活跃
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));
        let active = NexMarket::get_active_trades(&BOB);
        assert_eq!(active.len(), 0);
    });
}

// ==================== 审计回归测试 ====================

// ---- C1: resolve_dispute 传播国库转账失败 ----

#[test]
fn c1_resolve_dispute_treasury_underfunded_partial_compensation() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        // 掏空国库余额（Treasury=99）
        let treasury_balance = Balances::free_balance(99u64);
        let _ = <Balances as Currency<u64>>::transfer(
            &99u64,
            &10u64,
            treasury_balance - 1,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        );

        let bob_before = Balances::free_balance(BOB);

        // C3: 国库余额不足时仍可裁决（尽力补偿，不阻断）
        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            trade_id,
            DisputeResolution::ReleaseToBuyer,
        ));

        // 争议已解决
        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert_eq!(dispute.status, DisputeStatus::ResolvedForBuyer);

        // 国库余额 ≤ existential deposit → 无补偿
        // （余额只剩 1，减去 minimum_balance 后可用为 0）
        assert_eq!(Balances::free_balance(BOB), bob_before);
    });
}

// ---- H1: update_order_price 买单保证金重算 ----

#[test]
fn h1_update_buy_order_price_adjusts_deposit_upward() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 禁用价格保护以允许极端价差
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            false,
            0,
            0,
            0,
        ));
        // 10 NEX，低价 10 USDT/NEX → usdt=100 → deposit=10 NEX (min)
        let nex = 10_000_000_000_000u128;
        let low_price = 10_000_000u64;

        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            low_price,
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        let old_deposit = order.buyer_deposit;
        let bob_reserved_before = Balances::reserved_balance(BOB);

        // 提高价格到 100 USDT/NEX → usdt=1000 → deposit=100 NEX > 10 NEX
        let high_price = 100_000_000u64;
        assert_ok!(NexMarket::update_order_price(
            RuntimeOrigin::signed(BOB),
            0,
            high_price,
        ));

        let order_after = NexMarket::orders(0).unwrap();
        assert!(
            order_after.buyer_deposit > old_deposit,
            "deposit should increase: {} > {}",
            order_after.buyer_deposit,
            old_deposit
        );
        assert!(
            Balances::reserved_balance(BOB) > bob_reserved_before,
            "reserved should increase"
        );
    });
}

#[test]
fn h1_update_buy_order_price_adjusts_deposit_downward() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 禁用价格保护
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            false,
            0,
            0,
            0,
        ));
        // 10 NEX，高价 100 USDT/NEX → usdt=1000 → deposit=100 NEX
        let nex = 10_000_000_000_000u128;
        let high_price = 100_000_000u64;

        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            high_price,
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        let old_deposit = order.buyer_deposit;
        let bob_reserved_before = Balances::reserved_balance(BOB);

        // 降低价格到 10 USDT/NEX → usdt=100 → deposit=10 NEX < 100 NEX
        let low_price = 10_000_000u64;
        assert_ok!(NexMarket::update_order_price(
            RuntimeOrigin::signed(BOB),
            0,
            low_price,
        ));

        let order_after = NexMarket::orders(0).unwrap();
        assert!(
            order_after.buyer_deposit < old_deposit,
            "deposit should decrease: {} < {}",
            order_after.buyer_deposit,
            old_deposit
        );
        assert!(
            Balances::reserved_balance(BOB) < bob_reserved_before,
            "reserved should decrease"
        );
    });
}

#[test]
fn h1_update_sell_order_price_no_deposit_change() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 禁用价格保护
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            false,
            0,
            0,
            0,
        ));
        let nex = 10_000_000_000_000u128;
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        let alice_reserved = Balances::reserved_balance(ALICE);

        // 卖单价格变更不影响 reserved（卖家锁的是 NEX，与价格无关）
        assert_ok!(NexMarket::update_order_price(
            RuntimeOrigin::signed(ALICE),
            0,
            100_000_000,
        ));

        assert_eq!(Balances::reserved_balance(ALICE), alice_reserved);
    });
}

// ---- H2: rollback 跳过已过期订单 ----

#[test]
fn h2_rollback_skips_expired_order() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // Bob 吃单（全量）→ 订单变 Filled，从订单簿移除
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);

        // 推进到订单过期之后
        System::set_block_number(order.expires_at.saturating_add(1u64.into()));

        // 超时交易 → rollback_order_filled_amount
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        // 订单应标记为 Expired，不应在订单簿中
        let order_after = NexMarket::orders(0).unwrap();
        assert_eq!(order_after.status, OrderStatus::Expired);

        // 确认不在卖单簿中
        let sell_orders = NexMarket::sell_orders();
        assert!(!sell_orders.contains(&0));
    });
}

// ---- M1: 最低吃单量检查 ----

#[test]
fn m1_reserve_sell_order_rejects_micro_fill() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // 尝试吃 0.5 NEX（< MinOrderNexAmount = 1 NEX）
        let micro_amount = 500_000_000_000u128;
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                0,
                Some(micro_amount),
                buyer_tron(),
            ),
            Error::<Test>::OrderAmountBelowMinimum,
        );
    });
}

#[test]
fn m1_accept_buy_order_rejects_micro_fill() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            price,
            buyer_tron(),
        ));

        // 尝试吃 0.5 NEX
        let micro_amount = 500_000_000_000u128;
        assert_noop!(
            NexMarket::accept_buy_order(
                RuntimeOrigin::signed(ALICE),
                0,
                Some(micro_amount),
                tron_address(),
            ),
            Error::<Test>::OrderAmountBelowMinimum,
        );
    });
}

#[test]
fn m1_reserve_sell_order_allows_tail_fill_below_minimum() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 挂一个刚好超过 1 NEX 的卖单
        let nex = 1_500_000_000_000u128; // 1.5 NEX
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // 先吃 1 NEX → 剩余 0.5 NEX（低于最低限额）
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(1_000_000_000_000u128),
            buyer_tron(),
        ));

        // 超时第一笔交易让 Bob 可以再吃单
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        // 现在剩余 0.5 NEX < MinOrderNexAmount，尾单应该被允许
        // 注意：需要使用不同买家或确保 BOB 可以再次吃单
        // 使用 DAVE 作为新买家
        // DAVE 需要余额
        let _ = <Balances as Currency<u64>>::transfer(
            &BOB,
            &DAVE,
            100_000_000_000_000u128,
            frame_support::traits::ExistenceRequirement::KeepAlive,
        );
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(DAVE),
            0,
            None,
            buyer_tron(),
        ));
    });
}

// ==================== Round 2 审计回归测试 ====================

// ---- H1-R2: rollback 不覆写 Cancelled 订单状态 ----

#[test]
fn h1r2_rollback_preserves_cancelled_order_status() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // Bob 部分吃单（50 NEX）→ 订单变 PartiallyFilled
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000u128),
            buyer_tron(),
        ));
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyFilled);

        // Alice 取消订单 → 状态变 Cancelled
        assert_ok!(NexMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);

        // Bob 的交易超时 → rollback_order_filled_amount
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        // 订单状态应保持 Cancelled（不应被覆写为 Open）
        let order_after = NexMarket::orders(0).unwrap();
        assert_eq!(
            order_after.status,
            OrderStatus::Cancelled,
            "Cancelled order should NOT be overwritten to Open by rollback"
        );
        // filled_amount 应已回退
        assert!(order_after.filled_amount < 50_000_000_000_000u128);

        // 确认不在卖单簿中
        let sell_orders = NexMarket::sell_orders();
        assert!(!sell_orders.contains(&0));
    });
}

#[test]
fn h1r2_rollback_preserves_expired_order_status_non_filled() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // Bob 部分吃单
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000u128),
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        let expires_at = order.expires_at;

        // 推进到订单过期后 → on_idle GC 会标记为 Expired
        // 同时 on_idle 保证金逾期罚金会自动关闭交易（全额扣罚后自动 Refunded）
        System::set_block_number(expires_at + 1);
        NexMarket::on_idle(
            expires_at + 1,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        let order_after_gc = NexMarket::orders(0).unwrap();
        assert_eq!(order_after_gc.status, OrderStatus::Expired);

        // 交易已被 on_idle 逾期罚金自动关闭 → Refunded
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Forfeited);

        // 订单状态应保持 Expired（rollback 保护）
        let order_final = NexMarket::orders(0).unwrap();
        assert_eq!(
            order_final.status,
            OrderStatus::Expired,
            "Expired order should NOT be overwritten by rollback"
        );
    });
}

// ---- M1-R2: process_full_payment 手续费实际收取量 ----

#[test]
fn m1r2_fee_actually_charged_equals_nex_deducted() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 设置 1% 手续费
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100));

        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // Bob 吃单
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // Bob confirm_payment
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let bob_free_before = Balances::free_balance(BOB);
        let treasury_free_before = Balances::free_balance(99u64);

        // 模拟 OCW 提交验证结果（Exact）
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            50_000_000,
            make_tx_hashes(b"tx_fee_charged"),
        ));

        let bob_free_after = Balances::free_balance(BOB);
        let treasury_free_after = Balances::free_balance(99u64);

        // 1% of 100 NEX = 1 NEX = 1_000_000_000_000
        let expected_fee = 1_000_000_000_000u128;
        let treasury_received = treasury_free_after - treasury_free_before;
        let bob_received = bob_free_after - bob_free_before;

        assert_eq!(
            treasury_received, expected_fee,
            "Treasury should receive exactly 1% fee"
        );
        // Bob should get 99 NEX (100 - 1 fee) + deposit refund
        // Bob's deposit is also unreserved, so check NEX received >= 99 NEX
        assert!(
            bob_received >= nex - expected_fee,
            "Buyer should receive at least nex_amount - fee"
        );
    });
}

// ---- M2-R2: rollback 后 BestAsk/BestBid 刷新 ----

#[test]
fn m2r2_rollback_refreshes_best_ask() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64; // 0.5 USDT/NEX

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::best_ask(), Some(price));

        // Bob 全量吃单 → 订单变 Filled，从卖单簿移除
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Filled);

        // BestAsk 应该已被清除（无其他卖单）
        // 注意：remove_from_order_book 后 update_best_price_on_remove 可能已刷新
        // 但由于没有其他卖单，BestAsk 应为 None 或保持旧值

        // 超时交易 → rollback → 订单重新入簿
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        // 订单重新入簿，BestAsk 应该被刷新为 price
        let order_after = NexMarket::orders(0).unwrap();
        assert!(
            order_after.status == OrderStatus::Open || order_after.status == OrderStatus::Expired,
            "Order should be Open (if not expired) or Expired"
        );

        // 如果订单未过期，BestAsk 应刷新
        if order_after.status == OrderStatus::Open {
            assert_eq!(
                NexMarket::best_ask(),
                Some(price),
                "BestAsk should be refreshed after rollback re-adds order"
            );
        }
    });
}

// ---- L1-R2: get_order_depth 不含过期订单 ----

#[test]
fn l1r2_order_depth_excludes_expired_orders() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let price = 500_000u64;

        // Alice 挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));

        // 深度图应包含该卖单
        let (asks, _bids) = NexMarket::get_order_depth();
        assert_eq!(asks.len(), 1);

        // 推进到过期后
        let order = NexMarket::orders(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 深度图不应包含过期订单
        let (asks_after, _) = NexMarket::get_order_depth();
        assert_eq!(
            asks_after.len(),
            0,
            "Expired orders should not appear in order depth"
        );
    });
}

// ==================== 审计新增功能测试 ====================

// ---- seller_confirm_received ----

#[test]
fn seller_confirm_received_awaiting_payment() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        // W1: AwaitingPayment 状态不再允许卖家确认收款（防社工攻击）
        assert_noop!(
            NexMarket::seller_confirm_received(RuntimeOrigin::signed(ALICE), trade_id),
            Error::<Test>::InvalidTradeStatus
        );
    });
}

#[test]
fn seller_confirm_received_awaiting_verification() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

#[test]
fn seller_confirm_received_rejects_non_seller() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_noop!(
            NexMarket::seller_confirm_received(RuntimeOrigin::signed(BOB), trade_id),
            Error::<Test>::NotTradeParticipant
        );
    });
}

#[test]
fn seller_confirm_received_rejects_completed() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        // 先让买家确认付款（进入 AwaitingVerification）
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));
        // 卖家确认收款 → Completed
        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        assert_noop!(
            NexMarket::seller_confirm_received(RuntimeOrigin::signed(ALICE), trade_id),
            Error::<Test>::InvalidTradeStatus
        );
    });
}

// ---- ban_user / unban_user ----

#[test]
fn ban_user_blocks_place_sell_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));
        assert!(NexMarket::is_banned(ALICE));

        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::UserIsBanned
        );
    });
}

#[test]
fn ban_user_blocks_place_buy_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), BOB));

        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                100_000_000_000_000,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::UserIsBanned
        );
    });
}

#[test]
fn ban_user_blocks_reserve_sell_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), BOB));

        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 0, None, buyer_tron()),
            Error::<Test>::UserIsBanned
        );
    });
}

#[test]
fn ban_user_blocks_confirm_payment() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), BOB));

        assert_noop!(
            NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), trade_id),
            Error::<Test>::UserIsBanned
        );
    });
}

#[test]
fn unban_user_restores_access() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));
        assert!(NexMarket::is_banned(ALICE));

        assert_ok!(NexMarket::unban_user(RuntimeOrigin::root(), ALICE));
        assert!(!NexMarket::is_banned(ALICE));

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
    });
}

#[test]
fn ban_user_rejects_non_admin() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::ban_user(RuntimeOrigin::signed(ALICE), BOB),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ---- submit_counter_evidence ----

#[test]
fn submit_counter_evidence_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmBuyerEvidence".to_vec(),
        ));

        // 卖家提交反驳
        assert_ok!(NexMarket::submit_counter_evidence(
            RuntimeOrigin::signed(ALICE),
            trade_id,
            b"QmSellerRebuttal".to_vec(),
        ));

        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert!(dispute.counter_evidence_cid.is_some());
        assert_eq!(dispute.counter_party, Some(ALICE));
    });
}

#[test]
fn submit_counter_evidence_rejects_initiator() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        assert_noop!(
            NexMarket::submit_counter_evidence(
                RuntimeOrigin::signed(BOB),
                trade_id,
                b"QmSelf".to_vec(),
            ),
            Error::<Test>::NotTradeParticipant
        );
    });
}

#[test]
fn submit_counter_evidence_rejects_non_participant() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        assert_noop!(
            NexMarket::submit_counter_evidence(
                RuntimeOrigin::signed(CHARLIE),
                trade_id,
                b"QmOther".to_vec(),
            ),
            Error::<Test>::NotTradeParticipant
        );
    });
}

// ---- update_order_amount ----

#[test]
fn update_order_amount_sell_increase() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        let reserved_before = Balances::reserved_balance(ALICE);

        let new_amount = 200_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(ALICE),
            0,
            new_amount
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount);
        assert_eq!(
            Balances::reserved_balance(ALICE),
            reserved_before + 100_000_000_000_000
        );
    });
}

#[test]
fn update_order_amount_sell_decrease() {
    new_test_ext().execute_with(|| {
        let nex = 200_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        let reserved_before = Balances::reserved_balance(ALICE);

        let new_amount = 100_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(ALICE),
            0,
            new_amount
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount);
        assert_eq!(
            Balances::reserved_balance(ALICE),
            reserved_before - 100_000_000_000_000
        );
    });
}

#[test]
fn update_order_amount_buy_recalculates_deposit() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 用 100 NEX 创建买单，然后减小到 50 NEX
        // MockDepositCalculator: 1 NEX=1 USDT, rate=10%
        // 100 NEX @ 0.5 USDT/NEX → usdt=50 USDT → deposit=5 NEX
        // 50 NEX @ 0.5 USDT/NEX → usdt=25 USDT → deposit=2.5 NEX
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            500_000,
            buyer_tron(),
        ));
        let reserved_before = Balances::reserved_balance(BOB);

        // 减小到 50 NEX（deposit 从 5 NEX 降到 2.5 NEX）
        let new_amount = 50_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(BOB),
            0,
            new_amount
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount);
        // deposit 减小，reserved 减少
        assert!(Balances::reserved_balance(BOB) < reserved_before);

        // 增大到 200 NEX
        let new_amount_2 = 200_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(BOB),
            0,
            new_amount_2
        ));
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount_2);
    });
}

#[test]
fn update_order_amount_rejects_below_filled() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));

        // 部分成交
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000),
            buyer_tron(),
        ));
        // 完成交易释放 filled_amount
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_reject_below_filled")
        ));

        // 尝试减少到低于已成交量
        assert_noop!(
            NexMarket::update_order_amount(RuntimeOrigin::signed(ALICE), 0, 10_000_000_000_000),
            Error::<Test>::AmountBelowFilledAmount
        );
    });
}

#[test]
fn update_order_amount_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::update_order_amount(RuntimeOrigin::signed(BOB), 0, 200_000_000_000_000),
            Error::<Test>::NotOrderOwner
        );
    });
}

#[test]
fn update_order_amount_rejects_active_trades() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 创建大卖单，然后部分成交（保持 PartiallyFilled + 活跃交易）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            200_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000),
            buyer_tron(),
        ));
        // 订单是 PartiallyFilled，交易在 AwaitingPayment
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyFilled);

        assert_noop!(
            NexMarket::update_order_amount(RuntimeOrigin::signed(ALICE), 0, 300_000_000_000_000),
            Error::<Test>::OrderHasActiveTrades
        );
    });
}

// ---- batch_force_settle / batch_force_cancel ----

#[test]
fn batch_force_cancel_works() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 创建 2 笔交易（不同买家，避免信用系统并发限制）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000),
            buyer_tron(),
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(CHARLIE),
            0,
            None,
            buyer_tron(),
        ));

        let ids: BoundedVec<u64, ConstU32<20>> = vec![0, 1].try_into().unwrap();
        assert_ok!(NexMarket::batch_force_cancel(RuntimeOrigin::root(), ids));

        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::Refunded
        );
        assert_eq!(
            NexMarket::usdt_trades(1).unwrap().status,
            UsdtTradeStatus::Refunded
        );
    });
}

#[test]
fn batch_force_settle_works() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let ids: BoundedVec<u64, ConstU32<20>> = vec![0].try_into().unwrap();
        assert_ok!(NexMarket::batch_force_settle(
            RuntimeOrigin::root(),
            ids,
            50_000_000,
            DisputeResolution::ReleaseToBuyer,
        ));

        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::Completed
        );
    });
}

#[test]
fn batch_force_cancel_partial_failure() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        // trade_id 0 有效, trade_id 99 不存在
        let ids: BoundedVec<u64, ConstU32<20>> = vec![0, 99].try_into().unwrap();
        assert_ok!(NexMarket::batch_force_cancel(RuntimeOrigin::root(), ids));

        // trade 0 已取消, 事件记录 1 成功 1 失败
        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::Refunded
        );
    });
}

#[test]
fn batch_rejects_non_admin() {
    new_test_ext().execute_with(|| {
        let ids: BoundedVec<u64, ConstU32<20>> = vec![0].try_into().unwrap();
        assert_noop!(
            NexMarket::batch_force_cancel(RuntimeOrigin::signed(ALICE), ids),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ---- min_fill_amount ----

#[test]
fn min_fill_amount_blocks_small_fill() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let min_fill = 50_000_000_000_000u128; // 50 NEX

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            Some(min_fill),
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.min_fill_amount, min_fill);

        // 尝试吃单 10 NEX < 50 NEX 最低限额 → 拒绝
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                0,
                Some(10_000_000_000_000),
                buyer_tron(),
            ),
            Error::<Test>::BelowMinFillAmount
        );
    });
}

#[test]
fn min_fill_amount_allows_at_minimum() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let min_fill = 50_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            Some(min_fill),
        ));

        // 吃单 50 NEX = 最低限额 → 成功
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(min_fill),
            buyer_tron(),
        ));
    });
}

#[test]
fn min_fill_amount_tail_fill_exempt() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 60_000_000_000_000u128; // 60 NEX
        let min_fill = 50_000_000_000_000u128; // 50 NEX

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            Some(min_fill),
        ));

        // 先吃 50 NEX
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(min_fill),
            buyer_tron(),
        ));
        // 完成交易
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_tail_fill_exempt")
        ));

        // 剩余 10 NEX < min_fill(50 NEX) → 尾单豁免
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
    });
}

#[test]
fn min_fill_amount_zero_means_no_restriction() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert!(order.min_fill_amount.is_zero());

        // 小额吃单可以
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(1_000_000_000_000),
            buyer_tron(),
        ));
    });
}

// ---- MaxOrderNexAmount ----

#[test]
fn max_order_nex_amount_rejects_sell() {
    new_test_ext().execute_with(|| {
        // MaxOrderNexAmount = 500 NEX in mock
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                501_000_000_000_000,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::OrderAmountTooLarge
        );
    });
}

#[test]
fn max_order_nex_amount_rejects_buy() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                501_000_000_000_000,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::OrderAmountTooLarge
        );
    });
}

#[test]
fn max_order_nex_amount_allows_at_limit() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            500_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
    });
}

// ---- DisputeWindowBlocks ----

#[test]
fn dispute_window_blocks_rejects_expired() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        // W5: 窗口锚定 completed_at（block 8000），+ DisputeWindowBlocks(100800) = 108800
        let anchor: u64 = trade.completed_at.unwrap().into();
        System::set_block_number(anchor + 100801);

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(BOB), trade_id, b"QmTest".to_vec(),),
            Error::<Test>::DisputeWindowExpired
        );
    });
}

#[test]
fn dispute_window_blocks_allows_within_window() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        // W5: 窗口锚定 completed_at
        let anchor: u64 = trade.completed_at.unwrap().into();
        System::set_block_number(anchor + 100800); // 恰好在窗口边界

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));
    });
}

// ---- process_timeout 调用者限制 ----

#[test]
fn process_timeout_rejects_non_participant() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        System::set_block_number(10000);

        assert_noop!(
            NexMarket::process_timeout(RuntimeOrigin::signed(DAVE), trade_id),
            Error::<Test>::NotParticipantOrAdmin
        );
    });
}

#[test]
fn process_timeout_allows_admin() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        System::set_block_number(10000);

        // Root 是 MarketAdminOrigin
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::root(), trade_id));
    });
}

#[test]
fn process_timeout_allows_buyer() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        System::set_block_number(10000);

        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));
    });
}

#[test]
fn process_timeout_allows_seller() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        System::set_block_number(10000);

        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));
    });
}

// ---- resolve_dispute C3: Completed 不双重支付 ----

#[test]
fn resolve_dispute_completed_trade_no_double_payment() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 创建并完成交易
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(call_submit_ocw_result(
            0,
            trade.usdt_amount,
            make_tx_hashes(b"tx_no_double_pay")
        ));

        // Completed 状态
        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);

        let bob_after_trade = Balances::free_balance(BOB);
        let treasury_before = Balances::free_balance(99u64);

        // 发起争议（Completed 现在可争议）
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            0,
            b"QmTest".to_vec(),
        ));

        // 裁决为 ReleaseToBuyer
        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            0,
            DisputeResolution::ReleaseToBuyer,
        ));

        // C3: Completed 交易不再从国库补偿（买家已拿到 NEX）
        assert_eq!(Balances::free_balance(BOB), bob_after_trade);
        assert_eq!(Balances::free_balance(99u64), treasury_before);

        let dispute = NexMarket::trade_disputes(0).unwrap();
        assert_eq!(dispute.status, DisputeStatus::ResolvedForBuyer);
    });
}

// ---- resolve_dispute A3: Refunded 交易部分补偿 ----

#[test]
fn resolve_dispute_refunded_trade_full_compensation() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::resolve_dispute(
            RuntimeOrigin::root(),
            trade_id,
            DisputeResolution::ReleaseToBuyer,
        ));

        // Refunded 交易 → 从国库补偿 NEX 给买家
        assert!(Balances::free_balance(BOB) > bob_before);
    });
}

// ===================== WARNING 修复验证测试 =====================

// ---- W1: seller_confirm_received 仅允许 AwaitingVerification / UnderpaidPending ----

#[test]
fn w1_seller_confirm_awaiting_verification_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingVerification);

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert!(Balances::free_balance(BOB) > bob_before);
    });
}

#[test]
fn w1_seller_confirm_awaiting_payment_rejected() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_noop!(
            NexMarket::seller_confirm_received(RuntimeOrigin::signed(ALICE), trade_id),
            Error::<Test>::InvalidTradeStatus
        );
    });
}

// ---- W2: ban_user 取消已有挂单 ----

#[test]
fn w2_ban_user_cancels_open_orders() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert!(!NexMarket::sell_orders().is_empty());

        let alice_reserved = Balances::reserved_balance(ALICE);
        assert!(alice_reserved > 0);

        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));

        assert!(NexMarket::sell_orders().is_empty());
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
        assert_eq!(Balances::reserved_balance(ALICE), 0);
    });
}

#[test]
fn w2_ban_user_skips_orders_with_active_trades() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 200_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        // 创建一个交易（部分成交），使 order 有活跃 trade
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(100_000_000_000_000u128),
            buyer_tron(),
        ));

        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));

        // 订单仍存在（有活跃交易不能取消）
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
    });
}

#[test]
fn w2_ban_user_cancels_buy_order_refunds_deposit() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        // ALICE 创建卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        // BOB 创建买单
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            500_000,
            buyer_tron(),
        ));

        let bob_reserved = Balances::reserved_balance(BOB);
        assert!(bob_reserved > 0);

        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), BOB));

        // 买单已取消，保证金已退还
        let order = NexMarket::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

// ---- W3: submit_counter_evidence 不可覆盖 ----

#[test]
fn w3_counter_evidence_immutable() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmBuyerEvidence".to_vec(),
        ));
        assert_ok!(NexMarket::submit_counter_evidence(
            RuntimeOrigin::signed(ALICE),
            trade_id,
            b"QmFirstRebuttal".to_vec(),
        ));

        // 二次提交应被拒绝
        assert_noop!(
            NexMarket::submit_counter_evidence(
                RuntimeOrigin::signed(ALICE),
                trade_id,
                b"QmSecondRebuttal".to_vec(),
            ),
            Error::<Test>::CounterEvidenceAlreadySubmitted
        );

        let dispute = NexMarket::trade_disputes(trade_id).unwrap();
        assert_eq!(
            dispute.counter_evidence_cid.unwrap().to_vec(),
            b"QmFirstRebuttal".to_vec()
        );
    });
}

// ---- W5: 争议窗口锚定 completed_at ----

#[test]
fn w5_completed_at_set_on_full_payment() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        System::set_block_number(50);
        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        assert_eq!(trade.completed_at, Some(50));
    });
}

#[test]
fn w5_completed_at_set_on_refund() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.completed_at, Some(8000));
    });
}

#[test]
fn w5_dispute_window_uses_completed_at_not_timeout_at() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        let completed: u64 = trade.completed_at.unwrap().into();
        let timeout: u64 = trade.timeout_at.into();

        // completed_at (8000) > timeout_at (7201)
        assert!(completed > timeout);

        // 在 timeout_at + window 之后但 completed_at + window 之前 → 仍可争议
        System::set_block_number(completed + 100800);
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));
    });
}

// ---- W6: Refunded 交易需 payment_confirmed ----

#[test]
fn w6_refunded_without_payment_not_disputable() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        // 直接超时（未 confirm_payment） → Refunded 但 payment_confirmed=false
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
        assert!(!trade.payment_confirmed);

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(BOB), trade_id, b"QmTest".to_vec(),),
            Error::<Test>::PaymentNotConfirmed
        );
    });
}

#[test]
fn w6_refunded_with_payment_is_disputable() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_confirmed_then_refunded_trade();

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert!(trade.payment_confirmed);

        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));
    });
}

#[test]
fn w6_completed_trade_disputable_without_payment_confirmed_check() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));
        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);

        // Completed 交易不受 payment_confirmed 限制
        assert_ok!(NexMarket::dispute_trade(
            RuntimeOrigin::signed(BOB),
            trade_id,
            b"QmTest".to_vec(),
        ));
    });
}

// ---- W8: submit_ocw_result 结算失败时保留 OCW 结果 ----

#[test]
fn w8_ocw_result_stored_before_settlement() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        // usdt_amount = 100 NEX * 0.5 USDT = 50_000_000 usdt units
        // 70% = 35_000_000 → Underpaid（5000~9950 bps）→ 进入 UnderpaidPending
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            trade_id,
            35_000_000,
            make_tx_hashes(b"tx_w8_underpaid"),
        ));

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);

        // W8: Underpaid 结果保留在存储中，供补付/手动结算使用
        assert!(NexMarket::ocw_verification_results(trade_id).is_some());
    });
}

#[test]
fn w8_ocw_result_cleaned_on_exact_settlement() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_awaiting_payment_trade();
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            trade_id,
            500_000,
            make_tx_hashes(b"tx_w8_exact_clean"),
        ));

        // Exact 结算成功 → OCW 结果已清理
        let result = NexMarket::ocw_verification_results(trade_id);
        assert!(result.is_none());

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

// ==================== 补充测试：熔断机制 ====================

#[test]
fn circuit_breaker_triggers_on_extreme_deviation() {
    new_test_ext().execute_with(|| {
        // 设置初始价格并启用价格保护
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            true,
            2000,
            3000,
            0,
        ));

        // 完成一笔正常交易来初始化 TWAP
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
        assert_ok!(call_submit_ocw_result(
            0,
            500_000,
            make_tx_hashes(b"tx_circuit_breaker")
        ));

        // 尝试挂一个极端偏离的卖单（价格 = 2 USDT，偏离 300%）
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                10_000_000_000_000,
                2_000_000,
                tron_address(),
                None,
            ),
            Error::<Test>::PriceDeviationTooHigh,
        );
    });
}

#[test]
fn lift_circuit_breaker_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::set_initial_price(RuntimeOrigin::root(), 500_000));
        assert_ok!(NexMarket::configure_price_protection(
            RuntimeOrigin::root(),
            true,
            2000,
            3000,
            0,
        ));

        // 手动触发熔断
        PriceProtectionStore::<Test>::mutate(|maybe| {
            if let Some(config) = maybe {
                config.circuit_breaker_active = true;
                config.circuit_breaker_until = 100;
            }
        });

        // 市场应被熔断暂停
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                10_000_000_000_000,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::MarketCircuitBreakerActive,
        );

        // 熔断未到期时不能解除
        assert_noop!(
            NexMarket::lift_circuit_breaker(RuntimeOrigin::root()),
            Error::<Test>::CircuitBreakerNotExpired,
        );

        // 推进到熔断到期后
        System::set_block_number(100);

        // 管理员解除熔断
        assert_ok!(NexMarket::lift_circuit_breaker(RuntimeOrigin::root()));

        // 现在可以挂单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
    });
}

// ==================== 补充测试：ban_user / unban_user ====================

#[test]
fn ban_user_blocks_trading() {
    new_test_ext().execute_with(|| {
        // 封禁 ALICE
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));
        assert!(BannedAccounts::<Test>::get(ALICE));

        // ALICE 不能挂单
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::UserIsBanned,
        );

        // ALICE 不能挂买单
        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(ALICE),
                100_000_000_000_000,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::UserIsBanned,
        );
    });
}

#[test]
fn ban_user_cancels_existing_orders() {
    new_test_ext().execute_with(|| {
        // ALICE 先挂一个卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_eq!(NexMarket::sell_orders().len(), 1);

        // 封禁 ALICE → 应自动取消其挂单
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));

        // 卖单应被取消
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
        assert_eq!(NexMarket::sell_orders().len(), 0);
    });
}

#[test]
fn unban_user_restores_trading() {
    new_test_ext().execute_with(|| {
        assert_ok!(NexMarket::ban_user(RuntimeOrigin::root(), ALICE));
        assert_ok!(NexMarket::unban_user(RuntimeOrigin::root(), ALICE));
        assert!(!BannedAccounts::<Test>::get(ALICE));

        // ALICE 可以再次挂单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
    });
}

// ==================== 补充测试：seller_confirm_received ====================

#[test]
fn seller_confirm_received_completes_trade_full_flow() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let bob_free_before = Balances::free_balance(BOB);

        // 卖家手动确认收款
        assert_ok!(NexMarket::seller_confirm_received(
            RuntimeOrigin::signed(ALICE),
            0,
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
        // 买家收到 NEX
        assert!(Balances::free_balance(BOB) > bob_free_before);
    });
}

// ==================== 补充测试：update_order_amount ====================

#[test]
fn update_order_amount_increases() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));

        let alice_reserved_before = Balances::reserved_balance(ALICE);

        // 增加到 200 NEX
        let new_amount = 200_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(ALICE),
            0,
            new_amount,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount);
        // 额外锁定了 100 NEX
        assert!(Balances::reserved_balance(ALICE) > alice_reserved_before);
    });
}

#[test]
fn update_order_amount_decreases() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 200_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));

        let alice_reserved_before = Balances::reserved_balance(ALICE);

        // 减少到 100 NEX
        let new_amount = 100_000_000_000_000u128;
        assert_ok!(NexMarket::update_order_amount(
            RuntimeOrigin::signed(ALICE),
            0,
            new_amount,
        ));

        let order = NexMarket::orders(0).unwrap();
        assert_eq!(order.nex_amount, new_amount);
        // 释放了 100 NEX
        assert!(Balances::reserved_balance(ALICE) < alice_reserved_before);
    });
}

// ==================== 补充测试：batch_force_settle / batch_force_cancel ====================

#[test]
fn batch_force_settle_rejects_over_limit() {
    new_test_ext().execute_with(|| {
        // BoundedVec<u64, ConstU32<20>> 最多 20 个元素
        // 尝试创建超过 20 个的 BoundedVec → try_into 会失败
        let trade_ids: Vec<u64> = (0..21).collect();
        let result: Result<BoundedVec<u64, ConstU32<20>>, _> = trade_ids.try_into();
        assert!(
            result.is_err(),
            "BoundedVec should reject more than 20 elements"
        );
    });
}

// ==================== 补充测试：订单簿容量上限 ====================

#[test]
fn sell_order_book_full_rejected() {
    new_test_ext().execute_with(|| {
        // MaxSellOrders = 1000，直接填满
        let mut ids = Vec::new();
        for i in 0..1000u64 {
            ids.push(i);
        }
        let bounded: BoundedVec<u64, <Test as crate::Config>::MaxSellOrders> =
            ids.try_into().unwrap();
        SellOrders::<Test>::put(bounded);

        // 再挂一个卖单 → 应被拒绝
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                1_000_000_000_000,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::OrderBookFull,
        );
    });
}

#[test]
fn buy_order_book_full_rejected() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let mut ids = Vec::new();
        for i in 0..1000u64 {
            ids.push(i);
        }
        let bounded: BoundedVec<u64, <Test as crate::Config>::MaxBuyOrders> =
            ids.try_into().unwrap();
        BuyOrders::<Test>::put(bounded);

        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                1_000_000_000_000,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::OrderBookFull,
        );
    });
}

// ==================== 补充测试：队列满自动暂停 ====================

#[test]
fn queue_overflow_auto_pauses_market() {
    new_test_ext().execute_with(|| {
        // MaxPendingTrades = 100, QueueFullThresholdBps = 8000 (80%)
        // 80% of 100 = 80 → 填入 80 个 pending trades
        let mut ids = Vec::new();
        for i in 0..80u64 {
            ids.push(i);
        }
        let bounded: BoundedVec<u64, <Test as crate::Config>::MaxPendingTrades> =
            ids.try_into().unwrap();
        PendingUsdtTrades::<Test>::put(bounded);

        // 创建一笔真实交易来触发 confirm_payment 内部的队列检查
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        // confirm_payment 内部会调用 check_queue_overflow_and_pause
        // trade_id 是 0（第一笔创建的交易）
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 市场应被自动暂停
        assert!(NexMarket::market_paused());
    });
}

// ==================== 补充测试：争议窗口过期 ====================

#[test]
fn dispute_window_expired_rejected() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 超时退款
        System::set_block_number(8000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);

        // 推进到争议窗口之后 (DisputeWindowBlocks = 100800)
        let completed_at = trade.completed_at.unwrap_or(trade.timeout_at);
        System::set_block_number(completed_at + 100801);

        assert_noop!(
            NexMarket::dispute_trade(RuntimeOrigin::signed(BOB), 0, b"QmLateEvidence".to_vec(),),
            Error::<Test>::DisputeWindowExpired,
        );
    });
}

// ==================== 补充测试：min_fill_amount ====================

#[test]
fn min_fill_amount_enforced_on_reserve() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        let min_fill = 50_000_000_000_000u128; // 50 NEX 最低吃单量

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            Some(min_fill),
        ));

        // 尝试吃 10 NEX（低于 min_fill_amount 50 NEX）
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                0,
                Some(10_000_000_000_000u128),
                buyer_tron(),
            ),
            Error::<Test>::BelowMinFillAmount,
        );

        // 吃 50 NEX（等于 min_fill_amount）→ 成功
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(50_000_000_000_000u128),
            buyer_tron(),
        ));
    });
}

// ==================== 补充测试：MaxOrderNexAmount ====================

#[test]
fn place_sell_order_rejects_above_maximum() {
    new_test_ext().execute_with(|| {
        // MaxOrderNexAmount = 500 NEX in test config
        let too_large = 501_000_000_000_000u128;
        assert_noop!(
            NexMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                too_large,
                500_000,
                tron_address(),
                None,
            ),
            Error::<Test>::OrderAmountTooLarge,
        );
    });
}

#[test]
fn place_buy_order_rejects_above_maximum() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let too_large = 501_000_000_000_000u128;
        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                too_large,
                500_000,
                buyer_tron(),
            ),
            Error::<Test>::OrderAmountTooLarge,
        );
    });
}

// ==================== B1: Multi tx_hash proof model tests ====================

#[test]
fn b1_multi_hash_all_registered() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        // 提交包含 3 个 tx_hash 的结果
        let h1: TxHash = b"hash_one_aabbcc".to_vec().try_into().unwrap();
        let h2: TxHash = b"hash_two_ddeeff".to_vec().try_into().unwrap();
        let h3: TxHash = b"hash_three_0011".to_vec().try_into().unwrap();
        let tx_hashes: TxHashVec =
            BoundedVec::try_from(vec![h1.clone(), h2.clone(), h3.clone()]).unwrap();

        assert_ok!(call_submit_ocw_result(trade_id, usdt_amount, tx_hashes));

        // 所有 3 个 hash 都应被记录
        assert!(NexMarket::used_tx_hashes(&h1).is_some());
        assert!(NexMarket::used_tx_hashes(&h2).is_some());
        assert!(NexMarket::used_tx_hashes(&h3).is_some());

        // 每个 hash 都映射到同一个 trade_id
        assert_eq!(NexMarket::used_tx_hashes(&h1).unwrap().0, trade_id);
        assert_eq!(NexMarket::used_tx_hashes(&h2).unwrap().0, trade_id);
        assert_eq!(NexMarket::used_tx_hashes(&h3).unwrap().0, trade_id);
    });
}

#[test]
fn b1_any_hash_replay_blocks_all() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        // 第一笔交易
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade0 = NexMarket::usdt_trades(0).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 提交第一笔交易的结果（使用 hash_alpha 和 hash_beta）
        let shared_hash: TxHash = b"hash_alpha_shared".to_vec().try_into().unwrap();
        let unique_hash: TxHash = b"hash_beta_unique".to_vec().try_into().unwrap();
        let tx_hashes_1: TxHashVec =
            BoundedVec::try_from(vec![shared_hash.clone(), unique_hash.clone()]).unwrap();
        assert_ok!(call_submit_ocw_result(0, trade0.usdt_amount, tx_hashes_1));

        // 第二笔交易
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        let trade1 = NexMarket::usdt_trades(1).unwrap();
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 1));

        // 尝试在第二笔交易中重用 hash_alpha → 应被拒绝
        let new_hash: TxHash = b"hash_gamma_new00".to_vec().try_into().unwrap();
        let tx_hashes_2: TxHashVec = BoundedVec::try_from(vec![new_hash, shared_hash]).unwrap();
        assert_noop!(
            call_submit_ocw_result(1, trade1.usdt_amount, tx_hashes_2),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

#[test]
fn b1_empty_tx_hashes_rejected() {
    new_test_ext().execute_with(|| {
        let (trade_id, usdt_amount) = setup_trade_awaiting_verification();

        // 空 tx_hashes → TxHashRequired
        assert_noop!(
            call_submit_ocw_result(trade_id, usdt_amount, empty_tx_hashes()),
            Error::<Test>::TxHashRequired
        );
    });
}

// ==================== B2: Underpaid update registers new tx_hashes ====================

#[test]
fn b2_underpaid_update_registers_hashes() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 初始少付 70%（进入 UnderpaidPending）
        let actual_70 = expected * 70 / 100;
        let initial_hash: TxHash = b"b2_initial_hash0".to_vec().try_into().unwrap();
        assert_ok!(call_submit_ocw_result(
            0,
            actual_70,
            BoundedVec::try_from(vec![initial_hash.clone()]).unwrap()
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);
        assert!(NexMarket::used_tx_hashes(&initial_hash).is_some());

        // 补付到 90%（提交新的 tx_hash）
        let actual_90 = expected * 90 / 100;
        let topup_hash: TxHash = b"b2_topup_hash_01".to_vec().try_into().unwrap();
        assert_ok!(call_submit_underpaid_update(
            0,
            actual_90,
            BoundedVec::try_from(vec![topup_hash.clone()]).unwrap(),
        ));

        // 新 hash 已被记录
        assert!(NexMarket::used_tx_hashes(&topup_hash).is_some());
        assert_eq!(NexMarket::used_tx_hashes(&topup_hash).unwrap().0, 0);

        // 尝试在同一交易中重复使用已有 hash → 被拒
        assert_noop!(
            call_submit_underpaid_update(
                0,
                actual_90 + 1,
                BoundedVec::try_from(vec![initial_hash]).unwrap(),
            ),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

#[test]
fn b2_underpaid_update_full_topup_settles() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();
        let expected = trade.usdt_amount;
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // 初始少付 80%
        let actual_80 = expected * 80 / 100;
        assert_ok!(call_submit_ocw_result(
            0,
            actual_80,
            BoundedVec::try_from(vec![b"b2_settle_hash_1".to_vec().try_into().unwrap(),]).unwrap(),
        ));
        assert_eq!(
            NexMarket::usdt_trades(0).unwrap().status,
            UsdtTradeStatus::UnderpaidPending
        );

        // 补付到 100% → 应直接结算
        assert_ok!(call_submit_underpaid_update(
            0,
            expected,
            BoundedVec::try_from(vec![b"b2_settle_hash_2".to_vec().try_into().unwrap(),]).unwrap(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    });
}

// ==================== P1: forfeit_buyer_deposit 全额没收测试 ====================

#[test]
fn p1_timeout_awaiting_payment_forfeits_full_deposit() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        assert!(!deposit.is_zero());

        let treasury_before = Balances::free_balance(99u64);
        let seller_before = Balances::free_balance(ALICE);

        // AwaitingPayment 超时 — 买家完全未付款
        System::set_block_number(trade.timeout_at + 1);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(ALICE), 0));

        let trade = NexMarket::usdt_trades(0).unwrap();
        // P1: 完全违约场景始终 100% 没收
        assert_eq!(trade.deposit_status, BuyerDepositStatus::Forfeited);

        // 保证金 50/50 分配：国库 + 卖家
        let half_treasury = deposit / 2;
        let half_seller = deposit - half_treasury;
        let treasury_after = Balances::free_balance(99u64);
        assert_eq!(treasury_after - treasury_before, half_treasury);

        // 卖家收到 NEX 退还 + 保证金补偿
        let seller_after = Balances::free_balance(ALICE);
        assert_eq!(seller_after - seller_before, nex + half_seller);

        // 买家 reserved 保证金应归零
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

// ==================== P3: 买单多次部分成交保证金 dust 修复测试 ====================

#[test]
fn p3_buy_order_multi_fill_no_deposit_dust() {
    new_test_ext().execute_with(|| {
        // 3 NEX 买单 — 三次各 fill 1 NEX，验证最后一笔拿走全部剩余保证金
        let nex = 3_000_000_000_000u128; // 3 NEX

        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            nex,
            500_000,
            buyer_tron(),
        ));

        let order = NexMarket::orders(0).unwrap();
        let total_deposit = order.buyer_deposit;
        assert!(!total_deposit.is_zero());

        let fill = 1_000_000_000_000u128; // 1 NEX

        // Fill 1
        assert_ok!(NexMarket::accept_buy_order(
            RuntimeOrigin::signed(ALICE),
            0,
            Some(fill),
            tron_address(),
        ));
        let t0 = NexMarket::usdt_trades(0).unwrap();
        let d0 = t0.buyer_deposit;

        // Fill 2
        assert_ok!(NexMarket::accept_buy_order(
            RuntimeOrigin::signed(ALICE),
            0,
            Some(fill),
            tron_address(),
        ));
        let t1 = NexMarket::usdt_trades(1).unwrap();
        let d1 = t1.buyer_deposit;

        // Fill 3 (最后一笔 — fill_amount == available)
        assert_ok!(NexMarket::accept_buy_order(
            RuntimeOrigin::signed(ALICE),
            0,
            Some(fill),
            tron_address(),
        ));
        let t2 = NexMarket::usdt_trades(2).unwrap();
        let d2 = t2.buyer_deposit;

        // 核心断言：三笔交易的保证金总和 == 原始保证金（无 dust 残留）
        assert_eq!(d0 + d1 + d2, total_deposit, "deposit dust leaked");

        // 订单上不应残留保证金
        let order = NexMarket::orders(0).unwrap();
        assert_eq!(
            order.buyer_deposit, 0,
            "order deposit should be zero after full fill"
        );
        assert_eq!(order.status, OrderStatus::Filled);
    });
}

// ==================== 买家信用系统测试 ====================

/// Helper: 执行一笔完整交易（卖单 → 吃单 → 确认付款 → OCW 精确付款 → 完成）
/// 返回 (order_id, trade_id)
fn do_complete_trade(seller: u64, buyer: u64, nex: u128, price: u64) -> (u64, u64) {
    let order_id = NexMarket::next_order_id();
    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(seller),
        nex,
        price,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(buyer),
        order_id,
        None,
        buyer_tron(),
    ));
    let trade_id = NexMarket::next_usdt_trade_id().saturating_sub(1);
    let trade = NexMarket::usdt_trades(trade_id).unwrap();
    assert_ok!(NexMarket::confirm_payment(
        RuntimeOrigin::signed(buyer),
        trade_id
    ));
    let tx_hash_str = format!("tx_credit_test_{}", trade_id);
    assert_ok!(call_submit_ocw_result(
        trade_id,
        trade.usdt_amount,
        make_tx_hashes(tx_hash_str.as_bytes())
    ));
    (order_id, trade_id)
}

/// Helper: 执行一笔超时交易（卖单 → 吃单 → 等待超时 → process_timeout）
fn do_timeout_trade(seller: u64, buyer: u64, nex: u128, price: u64) -> u64 {
    let order_id = NexMarket::next_order_id();
    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(seller),
        nex,
        price,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(buyer),
        order_id,
        None,
        buyer_tron(),
    ));
    let trade_id = NexMarket::next_usdt_trade_id().saturating_sub(1);
    let trade = NexMarket::usdt_trades(trade_id).unwrap();
    let timeout_block: u64 = trade.timeout_at.into();
    System::set_block_number(timeout_block + 1);
    assert_ok!(NexMarket::process_timeout(
        RuntimeOrigin::signed(seller),
        trade_id
    ));
    trade_id
}

#[test]
fn credit_initial_profile_created_on_completion() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 新用户无 profile → 完成一笔交易后 profile 被创建 score=550（500+50）
        assert!(NexMarket::buyer_credit_profiles(BOB).is_none());

        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);

        let profile = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(profile.credit_score, 550); // 500 + 50（前 3 单）
        assert_eq!(profile.completed_orders, 1);
        assert_eq!(profile.consecutive_good_orders, 1);
        assert_eq!(profile.total_violations, 0);
        assert_eq!(profile.is_suspended, false);
    });
}

#[test]
fn credit_concurrent_limit_new_user() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 新用户（0 单）最多 1 个活跃交易 → 第 2 笔被拒 TradeLimitExceeded
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            50_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            50_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));

        // 第一笔成功
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_eq!(NexMarket::active_buyer_trades(BOB), 1);

        // 第二笔被并发限制阻止
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 1, None, buyer_tron()),
            Error::<Test>::TradeLimitExceeded
        );
    });
}

#[test]
fn credit_deposit_discount_high_score() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 手动设置 BOB 信用分 = 900 → 保证金折扣 50%
        use crate::credit::BuyerCreditProfile;
        let profile = BuyerCreditProfile {
            credit_score: 900,
            completed_orders: 50,
            ..Default::default()
        };
        BuyerCreditProfiles::<Test>::insert(BOB, profile);

        // 挂一笔卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            100_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));

        let bob_before = Balances::free_balance(BOB);
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let bob_after = Balances::free_balance(BOB);

        // 计算预期保证金（含折扣）
        // 100 NEX × 0.5 USDT/NEX = 50 USDT → 保证金 10% = 5 USDT
        // MockDepositCalculator: 5_000_000 usd → 5_000_000_000_000 NEX (5 NEX)
        // MinBuyerDepositUsd=1 USDT → DepositCalculator → 1 NEX (< 5 NEX)
        // max(5, 1) = 5 NEX = base → 折扣 50% → 2.5 NEX
        let expected_deposit = 2_500_000_000_000u128;
        let reserved = Balances::reserved_balance(BOB);
        assert_eq!(
            reserved, expected_deposit,
            "900 score should get 50% discount"
        );
        assert_eq!(bob_before - bob_after, expected_deposit);
    });
}

#[test]
fn credit_timeout_penalty_and_exponential_increase() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成一笔交易先提升信用分到 550（可以承受多次扣分）
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);

        let p1 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p1.credit_score, 550);

        // 第一次超时 → -50
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p2 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p2.credit_score, 500); // 550 - 50
        assert_eq!(p2.consecutive_violations, 1);

        // 完成一笔恢复（completed_orders=2, 仍在前 3 单内 → +50）→ 使下次超时不触发暂停
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p3 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p3.credit_score, 550); // 500 + 50
        assert_eq!(p3.consecutive_violations, 0);

        // 再次超时 → -50（连续重置了）
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p4 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p4.credit_score, 500); // 550 - 50
    });
}

#[test]
fn credit_three_consecutive_violations_suspension() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 先完成多笔交易提升信用分（需要能承受 3 次扣分: -50, -100, -200）
        // 完成 4 笔: 500 + 50*3 + 10 = 660
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p.credit_score, 660); // 500 + 50 + 50 + 50 + 10

        // 3 次连续超时 → 暂停
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -50 → 610
        let p1 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p1.credit_score, 610);
        assert_eq!(p1.consecutive_violations, 1);

        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -100 → 510
        let p2 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p2.credit_score, 510);
        assert_eq!(p2.consecutive_violations, 2);

        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -200 → 310, 3连续 → suspended
        let p3 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p3.credit_score, 310);
        assert_eq!(p3.is_suspended, true);
        assert!(p3.suspension_until.is_some());
        assert_eq!(p3.consecutive_violations, 1); // P7: 暂停后保留 1（而非归零）

        // 无法交易
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                NexMarket::next_order_id() - 1,
                None,
                buyer_tron()
            ),
            Error::<Test>::BuyerSuspended
        );
    });
}

#[test]
fn credit_zero_score_permanent_ban() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 设置 BOB 信用分足以创建交易
        use crate::credit::BuyerCreditProfile;
        let profile = BuyerCreditProfile {
            credit_score: 500,
            completed_orders: 10,
            consecutive_good_orders: 0,
            consecutive_violations: 0,
            ..Default::default()
        };
        BuyerCreditProfiles::<Test>::insert(BOB, profile);

        // 先创建一笔交易（BOB 信用分 500 可以通过检查）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade_id = NexMarket::next_usdt_trade_id().saturating_sub(1);
        let trade = NexMarket::usdt_trades(trade_id).unwrap();

        // 在超时之前，手动把信用分设为 30
        let mut p2 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        p2.credit_score = 30;
        p2.consecutive_violations = 0;
        BuyerCreditProfiles::<Test>::insert(BOB, p2);
        BannedAccounts::<Test>::remove(BOB);

        // 推进到超时后
        let timeout_block: u64 = trade.timeout_at.into();
        System::set_block_number(timeout_block + 1);

        // 超时 → -50 → saturating_sub → 0 → 永久 ban
        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));
        let p3 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p3.credit_score, 0);
        assert!(
            BannedAccounts::<Test>::get(BOB),
            "score=0 should trigger permanent ban"
        );

        // 被 ban 后连 ensure_not_banned 都过不了
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                NexMarket::next_order_id() - 1,
                None,
                buyer_tron()
            ),
            Error::<Test>::UserIsBanned
        );
    });
}

#[test]
fn credit_natural_recovery_after_30_days() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成交易后超时，信用分下降
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        // score = 550
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        // score = 500
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p.credit_score, 500);

        // 推进 30 天 + 1 block（BlocksPerDay = 14400, 30 天 = 432000）
        let current = System::block_number();
        System::set_block_number(current + 432001);

        // 下次交易时 lazy 恢复 +10
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            NexMarket::next_order_id() - 1,
            None,
            buyer_tron(),
        ));

        let p2 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p2.credit_score, 500); // score=500 不触发自然恢复（上限 500，需完成交易继续提升）
    });
}

#[test]
fn credit_force_cancel_no_penalty() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成一笔先建立 profile
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let before_profile = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(before_profile.credit_score, 550);

        // 创建一笔交易
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            NexMarket::next_order_id() - 1,
            None,
            buyer_tron(),
        ));
        let trade_id = NexMarket::next_usdt_trade_id() - 1;
        assert_eq!(NexMarket::active_buyer_trades(BOB), 1);

        // Admin 强制取消
        let ids: BoundedVec<u64, ConstU32<20>> = vec![trade_id].try_into().unwrap();
        assert_ok!(NexMarket::batch_force_cancel(RuntimeOrigin::root(), ids));

        // 信用分不变，活跃计数减少
        let after_profile = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(
            after_profile.credit_score, 550,
            "force_cancel should not affect credit"
        );
        assert_eq!(after_profile.total_violations, 0);
        assert_eq!(NexMarket::active_buyer_trades(BOB), 0);
    });
}

#[test]
fn credit_force_settle_refund_penalizes_buyer() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成一笔先建立 profile
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let before = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(before.credit_score, 550);

        // 创建交易并进入 AwaitingVerification
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            NexMarket::next_order_id() - 1,
            None,
            buyer_tron(),
        ));
        let trade_id = NexMarket::next_usdt_trade_id() - 1;
        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        // Admin force_settle RefundToSeller（判定买家违约）
        let ids: BoundedVec<u64, ConstU32<20>> = vec![trade_id].try_into().unwrap();
        assert_ok!(NexMarket::batch_force_settle(
            RuntimeOrigin::root(),
            ids,
            0,
            DisputeResolution::RefundToSeller,
        ));

        // 信用分应下降
        let after = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(after.credit_score, 500); // 550 - 50
        assert_eq!(after.total_violations, 1);
        assert_eq!(NexMarket::active_buyer_trades(BOB), 0);
    });
}

#[test]
fn credit_suspension_lifted_after_expiry() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 手动设置暂停状态（score >= 500 确保可恢复）
        use crate::credit::BuyerCreditProfile;
        let seven_days = 14400u64 * 7; // BlocksPerDay * 7
        let profile = BuyerCreditProfile {
            credit_score: 500,
            completed_orders: 10,
            is_suspended: true,
            suspension_until: Some(100u64 + seven_days),
            ..Default::default()
        };
        BuyerCreditProfiles::<Test>::insert(BOB, profile);
        System::set_block_number(100);

        // 暂停期间无法交易
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_noop!(
            NexMarket::reserve_sell_order(RuntimeOrigin::signed(BOB), 0, None, buyer_tron()),
            Error::<Test>::BuyerSuspended
        );

        // 推进到暂停期满 + 1
        System::set_block_number(100 + seven_days + 1);

        // 需要创建新的卖单（旧的已过期, expires_at = 100 + 14400 = 14500 < 100901）
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));

        // 暂停自动解除
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            1,
            None,
            buyer_tron(),
        ));
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p.is_suspended, false);
        assert!(p.suspension_until.is_none());
    });
}

#[test]
fn credit_first_three_orders_fast_learning() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 前 3 笔完成各 +50（而非 +10）
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        assert_eq!(
            NexMarket::buyer_credit_profiles(BOB).unwrap().credit_score,
            550
        ); // +50

        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        assert_eq!(
            NexMarket::buyer_credit_profiles(BOB).unwrap().credit_score,
            600
        ); // +50

        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        assert_eq!(
            NexMarket::buyer_credit_profiles(BOB).unwrap().credit_score,
            650
        ); // +50

        // 第 4 笔只 +10
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        assert_eq!(
            NexMarket::buyer_credit_profiles(BOB).unwrap().credit_score,
            660
        ); // +10
    });
}

#[test]
fn credit_consecutive_10_bonus() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 连续完成 10 笔 → 第 10 笔额外 +5
        // 前 3 笔: 500 + 50*3 = 650
        // 第 4-9 笔: 650 + 10*6 = 710
        // 第 10 笔: 710 + 10 + 5(bonus) = 725
        for _ in 0..10 {
            do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        }
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p.credit_score, 725); // 500 + 50*3 + 10*6 + 10 + 5
        assert_eq!(p.consecutive_good_orders, 10);
        assert_eq!(p.completed_orders, 10);
    });
}

#[test]
fn credit_underpaid_lighter_penalty() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成一笔先建立 profile
        do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p.credit_score, 550);

        // 创建交易并少付
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            NexMarket::next_order_id() - 1,
            None,
            buyer_tron(),
        ));
        let trade_id = NexMarket::next_usdt_trade_id() - 1;
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        let expected = trade.usdt_amount;

        assert_ok!(NexMarket::confirm_payment(
            RuntimeOrigin::signed(BOB),
            trade_id
        ));

        // 少付 70%
        let actual = expected * 70 / 100;
        assert_ok!(call_submit_ocw_result(
            trade_id,
            actual,
            make_tx_hashes(b"tx_underpaid_credit")
        ));

        // 进入 UnderpaidPending，等待补付窗口到期
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::UnderpaidPending);
        let deadline: u64 = trade.underpaid_deadline.unwrap().into();
        System::set_block_number(deadline + 1);

        // 触发终裁
        assert_ok!(NexMarket::process_timeout(
            RuntimeOrigin::signed(ALICE),
            trade_id
        ));

        // 少付扣分 -30（轻于超时的 -50）
        let p2 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p2.credit_score, 520); // 550 - 30
        assert_eq!(p2.total_violations, 1);
    });
}

// ==================== Oracle 不可用阻止交易测试 ====================

#[test]
fn oracle_unavailable_blocks_place_buy_order() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        ORACLE_AVAILABLE.with(|c| c.set(false));

        assert_noop!(
            NexMarket::place_buy_order(
                RuntimeOrigin::signed(BOB),
                10_000_000_000_000, // 10 NEX
                1_000_000,          // 1 USDT/NEX
                buyer_tron(),
            ),
            Error::<Test>::OracleUnavailable
        );

        // 恢复 Oracle 后正常下单
        ORACLE_AVAILABLE.with(|c| c.set(true));
        assert_ok!(NexMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            10_000_000_000_000,
            1_000_000,
            buyer_tron(),
        ));
    });
}

#[test]
fn oracle_unavailable_blocks_reserve_sell_order() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        // Oracle 可用时正常挂卖单
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            10_000_000_000_000,
            1_000_000,
            tron_address(),
            None,
        ));
        let order_id = NexMarket::next_order_id() - 1;

        // 关闭 Oracle → 买家吃单失败
        ORACLE_AVAILABLE.with(|c| c.set(false));
        assert_noop!(
            NexMarket::reserve_sell_order(
                RuntimeOrigin::signed(BOB),
                order_id,
                None,
                buyer_tron(),
            ),
            Error::<Test>::OracleUnavailable
        );

        // 恢复 Oracle → 正常吃单
        ORACLE_AVAILABLE.with(|c| c.set(true));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            order_id,
            None,
            buyer_tron(),
        ));
    });
}

// ==================== 保证金逾期罚金测试 ====================

/// 宽限期（2h = 1200 blocks）内 on_idle 不扣罚金
#[test]
fn deposit_penalty_no_charge_within_grace_period() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        assert!(!deposit.is_zero());

        let bob_reserved_before = Balances::reserved_balance(BOB);

        // 推进到宽限期内（1200 blocks = 2h 边界）
        System::set_block_number(trade.created_at + 1200);
        NexMarket::on_idle(
            trade.created_at + 1200,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        // 不应有罚金扣除
        let trade_after = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade_after.cumulative_penalty, 0);
        assert_eq!(trade_after.status, UsdtTradeStatus::AwaitingPayment);
        assert_eq!(Balances::reserved_balance(BOB), bob_reserved_before);
    });
}

/// 宽限期后第一个整小时（3h = 1800 blocks）扣 5%
#[test]
fn deposit_penalty_first_hour_after_grace() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        let treasury_before = Balances::free_balance(99u64);
        let alice_before = Balances::free_balance(ALICE);

        // 推进到 3h（宽限期 2h + 1h），hourly_ticks = 1, rate = 5%
        System::set_block_number(trade.created_at + 1800);
        NexMarket::on_idle(
            trade.created_at + 1800,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        let trade_after = NexMarket::usdt_trades(0).unwrap();
        let expected_penalty = deposit * 500 / 10000; // 5%
        assert_eq!(trade_after.cumulative_penalty, expected_penalty);
        assert_eq!(trade_after.status, UsdtTradeStatus::AwaitingPayment);

        // 50/50 分配
        let half_treasury = expected_penalty / 2;
        let half_seller = expected_penalty - half_treasury;
        assert_eq!(
            Balances::free_balance(99u64) - treasury_before,
            half_treasury
        );
        assert_eq!(Balances::free_balance(ALICE) - alice_before, half_seller);
    });
}

/// 多次 on_idle 累积扣罚，第二次只扣增量
#[test]
fn deposit_penalty_incremental_charges() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;

        // 第一次: 3h, 5%
        System::set_block_number(trade.created_at + 1800);
        NexMarket::on_idle(
            trade.created_at + 1800,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );
        let after1 = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(after1.cumulative_penalty, deposit * 500 / 10000);

        // 第二次: 4h, 累计 10%
        System::set_block_number(trade.created_at + 2400);
        NexMarket::on_idle(
            trade.created_at + 2400,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );
        let after2 = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(after2.cumulative_penalty, deposit * 1000 / 10000);

        // 第三次: 5h, 累计 15%
        System::set_block_number(trade.created_at + 3000);
        NexMarket::on_idle(
            trade.created_at + 3000,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );
        let after3 = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(after3.cumulative_penalty, deposit * 1500 / 10000);
    });
}

/// 保证金全部扣完 → 交易自动关闭
#[test]
fn deposit_penalty_auto_close_when_fully_consumed() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        assert!(!deposit.is_zero());

        let alice_reserved_before = Balances::reserved_balance(ALICE);

        // 推进到 22h（宽限期 2h + 20h），hourly_ticks = 20, rate = min(20*500, 10000) = 10000 = 100%
        System::set_block_number(trade.created_at + 13200);
        NexMarket::on_idle(
            trade.created_at + 13200,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        // 交易自动关闭
        let trade_after = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade_after.status, UsdtTradeStatus::Refunded);
        assert_eq!(trade_after.deposit_status, BuyerDepositStatus::Forfeited);
        assert_eq!(trade_after.cumulative_penalty, deposit);

        // 卖家 NEX 已退还
        assert_eq!(
            Balances::reserved_balance(ALICE),
            alice_reserved_before - nex
        );

        // 买家保证金全部扣完
        assert_eq!(Balances::reserved_balance(BOB), 0);

        // 订单回滚
        let order = NexMarket::orders(0).unwrap();
        assert!(order.status == OrderStatus::Open || order.filled_amount == 0);

        // 从 AwaitingPaymentTrades 队列中清除
        assert!(!NexMarket::awaiting_payment_trades().contains(&0));
    });
}

/// 买家部分罚金后 confirm_payment → 剩余保证金仍锁定
#[test]
fn deposit_penalty_then_confirm_payment() {
    new_test_ext().execute_with(|| {
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;

        // 3h 后扣 5%
        System::set_block_number(trade.created_at + 1800);
        NexMarket::on_idle(
            trade.created_at + 1800,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );
        let trade_after = NexMarket::usdt_trades(0).unwrap();
        let penalized = trade_after.cumulative_penalty;
        assert!(penalized > 0);
        assert_eq!(trade_after.status, UsdtTradeStatus::AwaitingPayment);

        // 买家 confirm_payment
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let trade_confirmed = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(
            trade_confirmed.status,
            UsdtTradeStatus::AwaitingVerification
        );
        assert_eq!(trade_confirmed.cumulative_penalty, penalized);
        // 剩余保证金仍在 reserved
        let remaining = deposit - penalized;
        // Bob reserved = remaining deposit (部分已被 repatriate 转走)
        assert!(Balances::reserved_balance(BOB) >= remaining);
    });
}

/// 买家部分罚金后完成交易 → 只退还剩余保证金
#[test]
fn deposit_penalty_partial_then_full_payment_releases_remaining() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        let usdt_amount = trade.usdt_amount;

        // 3h 后扣 5%
        System::set_block_number(trade.created_at + 1800);
        NexMarket::on_idle(
            trade.created_at + 1800,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );
        let penalized = NexMarket::usdt_trades(0).unwrap().cumulative_penalty;
        assert!(penalized > 0);

        // 买家 confirm_payment → AwaitingVerification
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // OCW 验证通过（Exact）
        let bob_before = Balances::free_balance(BOB);
        assert_ok!(call_submit_ocw_result(
            0,
            usdt_amount,
            make_tx_hashes(b"hash_penalty_test")
        ));

        // 交易完成
        let trade_final = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade_final.status, UsdtTradeStatus::Completed);

        // 保证金状态：部分罚没
        assert_eq!(
            trade_final.deposit_status,
            BuyerDepositStatus::PartiallyForfeited
        );

        // 买家收到 NEX + 退还的剩余保证金
        let _remaining_deposit = deposit - penalized;
        let bob_after = Balances::free_balance(BOB);
        // bob should get nex + remaining_deposit back
        assert!(bob_after > bob_before);
        // reserved should be zero
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

/// 免保证金交易不受罚金影响
#[test]
fn deposit_penalty_waived_deposit_not_affected() {
    new_test_ext().execute_with(|| {
        setup_seed_price();

        // seed_liquidity 创建免保证金卖单
        assert_ok!(NexMarket::seed_liquidity(RuntimeOrigin::root(), 1, None));

        let order = NexMarket::orders(0).unwrap();
        assert!(order.deposit_waived);

        // Dave 吃免保证金单
        // 先给 Dave 一些余额
        let _ = Balances::deposit_creating(&DAVE, 100_000_000_000_000);
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(DAVE),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        assert!(trade.buyer_deposit.is_zero());

        // 3h 后 on_idle → 不应有罚金
        System::set_block_number(trade.created_at + 1800);
        NexMarket::on_idle(
            trade.created_at + 1800,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        let trade_after = NexMarket::usdt_trades(0).unwrap();
        assert_eq!(trade_after.cumulative_penalty, 0);
        assert_eq!(trade_after.status, UsdtTradeStatus::AwaitingPayment);
    });
}

// ==================== Indexer 测试 ====================

/// 辅助：注册 Indexer（默认端点 URL）
fn register_indexer_helper(who: u64) {
    assert_ok!(NexMarket::register_indexer(
        RuntimeOrigin::signed(who),
        b"https://indexer.example.com".to_vec(),
    ));
}

/// 辅助：创建一笔 AwaitingVerification 状态的交易（返回 trade_id）
fn setup_indexer_trade_awaiting_verification() -> u64 {
    setup_seed_price();
    let nex = 100_000_000_000_000u128;
    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(ALICE),
        nex,
        500_000,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(BOB),
        0,
        None,
        buyer_tron(),
    ));
    assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));
    let trade = NexMarket::usdt_trades(0).unwrap();
    assert_eq!(trade.status, UsdtTradeStatus::AwaitingVerification);
    0 // trade_id
}

/// 辅助：创建一笔 AwaitingPayment 状态的交易（返回 trade_id）
fn setup_indexer_trade_awaiting_payment() -> u64 {
    setup_seed_price();
    let nex = 100_000_000_000_000u128;
    assert_ok!(NexMarket::place_sell_order(
        RuntimeOrigin::signed(ALICE),
        nex,
        500_000,
        tron_address(),
        None,
    ));
    assert_ok!(NexMarket::reserve_sell_order(
        RuntimeOrigin::signed(BOB),
        0,
        None,
        buyer_tron(),
    ));
    let trade = NexMarket::usdt_trades(0).unwrap();
    assert_eq!(trade.status, UsdtTradeStatus::AwaitingPayment);
    0 // trade_id
}

#[test]
fn indexer_register_works() {
    new_test_ext().execute_with(|| {
        let stake_before = Balances::free_balance(INDEXER);
        register_indexer_helper(INDEXER);

        // 检查 IndexerSet
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.stake, 100_000_000_000_000); // 100 NEX
        assert_eq!(info.accelerated_count, 0);
        assert_eq!(info.error_count, 0);
        assert_eq!(info.pending_hint_count, 0);
        assert!(!info.suspended);

        // 检查质押扣除
        assert_eq!(Balances::reserved_balance(INDEXER), 100_000_000_000_000,);
        assert_eq!(
            Balances::free_balance(INDEXER),
            stake_before - 100_000_000_000_000,
        );

        // IndexerCount
        assert_eq!(NexMarket::indexer_count(), 1);
    });
}

#[test]
fn indexer_register_duplicate_fails() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);
        assert_noop!(
            NexMarket::register_indexer(
                RuntimeOrigin::signed(INDEXER),
                b"https://dup.example.com".to_vec(),
            ),
            Error::<Test>::IndexerAlreadyRegistered
        );
    });
}

#[test]
fn indexer_register_max_reached() {
    new_test_ext().execute_with(|| {
        // MaxIndexers = 10 in mock
        for i in 10u64..20 {
            let _ = Balances::deposit_creating(&i, 500_000_000_000_000);
            assert_ok!(NexMarket::register_indexer(
                RuntimeOrigin::signed(i),
                b"https://indexer.example.com".to_vec(),
            ));
        }
        assert_eq!(NexMarket::indexer_count(), 10);

        let _ = Balances::deposit_creating(&20, 500_000_000_000_000);
        assert_noop!(
            NexMarket::register_indexer(
                RuntimeOrigin::signed(20),
                b"https://overflow.example.com".to_vec(),
            ),
            Error::<Test>::MaxIndexersReached
        );
    });
}

#[test]
fn indexer_register_insufficient_stake() {
    new_test_ext().execute_with(|| {
        // DAVE has no balance
        assert_noop!(
            NexMarket::register_indexer(
                RuntimeOrigin::signed(DAVE),
                b"https://indexer.example.com".to_vec(),
            ),
            Error::<Test>::InsufficientIndexerStake
        );
    });
}

#[test]
fn indexer_deregister_works() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);

        let free_before = Balances::free_balance(INDEXER);
        assert_ok!(NexMarket::deregister_indexer(RuntimeOrigin::signed(
            INDEXER
        )));

        // 质押退还
        assert_eq!(Balances::reserved_balance(INDEXER), 0);
        assert_eq!(
            Balances::free_balance(INDEXER),
            free_before + 100_000_000_000_000
        );

        // 已移除
        assert!(NexMarket::indexer_set(INDEXER).is_none());
        assert_eq!(NexMarket::indexer_count(), 0);
    });
}

#[test]
fn indexer_deregister_not_found() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::deregister_indexer(RuntimeOrigin::signed(INDEXER)),
            Error::<Test>::IndexerNotFound
        );
    });
}

#[test]
fn indexer_deregister_with_pending_hints_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"abc123def456".to_vec(),
            50_000_000,
        ));

        // 不能退出（有 pending hint）
        assert_noop!(
            NexMarket::deregister_indexer(RuntimeOrigin::signed(INDEXER)),
            Error::<Test>::IndexerHasPendingHints
        );
    });
}

#[test]
fn indexer_hint_submit_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_abc123".to_vec(),
            50_000_000,
        ));

        // 检查 hint 存储
        let hint = NexMarket::indexer_hints(trade_id).unwrap();
        assert_eq!(hint.indexer, INDEXER);
        assert_eq!(hint.reported_amount, 50_000_000);
        assert_eq!(hint.status, IndexerHintStatus::Pending);

        // Indexer pending_hint_count 递增
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.pending_hint_count, 1);
    });
}

#[test]
fn indexer_hint_duplicate_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_abc123".to_vec(),
            50_000_000,
        ));

        // 另一个 Indexer 也尝试提交
        let _ = Balances::deposit_creating(&10u64, 500_000_000_000_000);
        assert_ok!(NexMarket::register_indexer(
            RuntimeOrigin::signed(10),
            b"https://indexer2.example.com".to_vec(),
        ));

        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(10),
                trade_id,
                b"txhash_xyz789".to_vec(),
                50_000_000,
            ),
            Error::<Test>::HintAlreadyExists
        );
    });
}

#[test]
fn indexer_hint_tx_hash_reuse_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 先标记一个 tx_hash 为已使用
        let hash_bytes: TxHash = b"used_hash_123".to_vec().try_into().unwrap();
        let now = System::block_number();
        UsedTxHashes::<Test>::insert(&hash_bytes, (99u64, now));

        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(INDEXER),
                trade_id,
                b"used_hash_123".to_vec(),
                50_000_000,
            ),
            Error::<Test>::TxHashAlreadyUsed
        );
    });
}

#[test]
fn indexer_hint_amount_insane_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        let insane_amount = trade.usdt_amount * 11; // > 10x

        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(INDEXER),
                trade_id,
                b"txhash_insane".to_vec(),
                insane_amount,
            ),
            Error::<Test>::HintAmountInsane
        );
    });
}

#[test]
fn indexer_hint_non_indexer_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();

        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(CHARLIE),
                trade_id,
                b"txhash_abc".to_vec(),
                50_000_000,
            ),
            Error::<Test>::IndexerNotFound
        );
    });
}

#[test]
fn indexer_hint_suspended_fails() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 手动 suspend
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.suspended = true;
            }
        });

        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(INDEXER),
                trade_id,
                b"txhash_abc".to_vec(),
                50_000_000,
            ),
            Error::<Test>::IndexerIsSuspended
        );
    });
}

#[test]
fn indexer_hint_wrong_trade_status_fails() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        // 不创建交易，直接测试

        register_indexer_helper(INDEXER);
        // trade 0 不存在
        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(INDEXER),
                999,
                b"txhash_abc".to_vec(),
                50_000_000,
            ),
            Error::<Test>::UsdtTradeNotFound
        );
    });
}

#[test]
fn indexer_mismatch_report_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_mismatch".to_vec(),
            50_000_000,
        ));

        // OCW 报告 mismatch
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        // hint 已移除（槽位释放）
        assert!(NexMarket::indexer_hints(trade_id).is_none());

        // Indexer error_count 递增
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.error_count, 1);
        assert_eq!(info.pending_hint_count, 0);
        assert!(!info.suspended);
    });
}

#[test]
fn indexer_mismatch_auto_suspend() {
    new_test_ext().execute_with(|| {
        // MaxIndexerErrors = 3 in mock
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 手动设置 error_count 接近阈值
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.error_count = 2; // 差一次就暂停
            }
        });

        // 提交 hint + mismatch → 应触发暂停
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_final_mis".to_vec(),
            50_000_000,
        ));
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        // 3 次 mismatch → 自动暂停
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.error_count, 3);
        assert!(info.suspended);
    });
}

#[test]
fn indexer_mismatch_releases_slot_for_resubmit() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // Indexer 1 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_bad".to_vec(),
            50_000_000,
        ));

        // Mismatch → 移除 hint
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        // Indexer 2 可以重新提交 hint
        let _ = Balances::deposit_creating(&10u64, 500_000_000_000_000);
        assert_ok!(NexMarket::register_indexer(
            RuntimeOrigin::signed(10),
            b"https://indexer2.example.com".to_vec(),
        ));

        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(10),
            trade_id,
            b"txhash_good".to_vec(),
            50_000_000,
        ));

        let hint = NexMarket::indexer_hints(trade_id).unwrap();
        assert_eq!(hint.indexer, 10);
    });
}

#[test]
fn indexer_force_remove_works() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);

        let reserved_before = Balances::reserved_balance(INDEXER);
        assert!(reserved_before > 0);

        // Root 强制移除
        assert_ok!(NexMarket::force_remove_indexer(
            RuntimeOrigin::root(),
            INDEXER,
        ));

        // 已移除
        assert!(NexMarket::indexer_set(INDEXER).is_none());
        assert_eq!(NexMarket::indexer_count(), 0);
        assert_eq!(Balances::reserved_balance(INDEXER), 0);
    });
}

#[test]
fn indexer_force_remove_not_found() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::force_remove_indexer(RuntimeOrigin::root(), INDEXER),
            Error::<Test>::IndexerNotFound
        );
    });
}

#[test]
fn indexer_hint_reward_on_completion() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_reward".to_vec(),
            50_000_000,
        ));

        let indexer_balance_before = Balances::free_balance(INDEXER);
        let treasury_before = Balances::free_balance(99u64); // Treasury

        // OCW 验证通过（Exact）
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_ok!(call_submit_ocw_result(
            trade_id,
            trade.usdt_amount,
            make_tx_hashes(b"txhash_reward_ocw")
        ));

        // P9: hint 已在交易完成后清理
        assert!(NexMarket::indexer_hints(trade_id).is_none());

        // Indexer 统计更新
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.accelerated_count, 1);
        assert_eq!(info.pending_hint_count, 0);

        // 奖励从 Indexer 奖池发放（0.01 NEX = 10_000_000_000）
        let expected_reward = 10_000_000_000u128;
        assert_eq!(
            Balances::free_balance(INDEXER),
            indexer_balance_before + expected_reward,
        );
        // 国库余额不应减少（奖励从奖池出，非国库）
        assert!(
            Balances::free_balance(99u64) >= treasury_before,
            "Treasury should not decrease (reward from pool, not treasury)"
        );
    });
}

#[test]
fn indexer_reward_pool_depleted_emits_event() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1); // 必须 >= 1 才能记录事件
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER), trade_id,
            b"txhash_deplete".to_vec(), 50_000_000,
        ));

        // 掏空 Indexer 奖池（RewardSource = 97，奖励从奖池发放）
        let pool_balance = Balances::free_balance(97u64);
        let _ = Balances::deposit_creating(&88u64, 1);
        let _ = <Balances as Currency<u64>>::transfer(
            &97u64, &88u64, pool_balance, frame_support::traits::ExistenceRequirement::AllowDeath,
        );
        assert_eq!(Balances::free_balance(97u64), 0);

        // OCW 验证通过 → 结算成功，但奖池余额不够发奖励
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_ok!(call_submit_ocw_result(trade_id, trade.usdt_amount, make_tx_hashes(b"txhash_deplete_ocw")));

        // 交易仍正常完成
        let trade_final = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade_final.status, UsdtTradeStatus::Completed);

        // P9: hint 已在交易完成后清理（统计正常更新）
        assert!(NexMarket::indexer_hints(trade_id).is_none());

        // 检查 IndexerHintVerified 事件中 reward = 0
        let events = System::events();
        let verified_event = events.iter().find(|e| {
            matches!(
                e.event,
                RuntimeEvent::NexMarket(Event::IndexerHintVerified { reward, .. }) if reward == 0u128
            )
        });
        assert!(verified_event.is_some(), "Should emit IndexerHintVerified with reward=0");

        // 检查 IndexerRewardPoolDepleted 告警事件
        let depleted_event = events.iter().find(|e| {
            matches!(
                e.event,
                RuntimeEvent::NexMarket(Event::IndexerRewardPoolDepleted { .. })
            )
        });
        assert!(depleted_event.is_some(), "Should emit IndexerRewardPoolDepleted warning");
    });
}

#[test]
fn indexer_hint_on_awaiting_payment_trade() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_payment();
        register_indexer_helper(INDEXER);

        // 提交 hint（AwaitingPayment 也可以）
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_await_pay".to_vec(),
            50_000_000,
        ));

        let hint = NexMarket::indexer_hints(trade_id).unwrap();
        assert_eq!(hint.status, IndexerHintStatus::Pending);
    });
}

#[test]
fn validate_unsigned_report_indexer_mismatch() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_vu".to_vec(),
            50_000_000,
        ));

        // ValidateUnsigned 应接受
        let call = crate::pallet::Call::<Test>::report_indexer_mismatch { trade_id };
        let result = NexMarket::validate_unsigned(TransactionSource::Local, &call);
        assert!(result.is_ok());
    });
}

#[test]
fn validate_unsigned_report_mismatch_no_hint() {
    new_test_ext().execute_with(|| {
        // 没有 hint → 拒绝
        let call = crate::pallet::Call::<Test>::report_indexer_mismatch { trade_id: 999 };
        let result = NexMarket::validate_unsigned(TransactionSource::Local, &call);
        assert!(result.is_err());
    });
}

#[test]
fn indexer_count_tracks_correctly() {
    new_test_ext().execute_with(|| {
        assert_eq!(NexMarket::indexer_count(), 0);

        register_indexer_helper(INDEXER);
        assert_eq!(NexMarket::indexer_count(), 1);

        let _ = Balances::deposit_creating(&10u64, 500_000_000_000_000);
        assert_ok!(NexMarket::register_indexer(
            RuntimeOrigin::signed(10),
            b"https://idx2.example.com".to_vec(),
        ));
        assert_eq!(NexMarket::indexer_count(), 2);

        assert_ok!(NexMarket::deregister_indexer(RuntimeOrigin::signed(
            INDEXER
        )));
        assert_eq!(NexMarket::indexer_count(), 1);

        assert_ok!(NexMarket::force_remove_indexer(RuntimeOrigin::root(), 10));
        assert_eq!(NexMarket::indexer_count(), 0);
    });
}

// ==================== Indexer 奖池分成测试 ====================

#[test]
fn set_indexer_pool_share_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(NexMarket::indexer_pool_share_bps(), 0);
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            500
        )); // 5%
        assert_eq!(NexMarket::indexer_pool_share_bps(), 500);

        // 最大 10000 (100%)
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            10000
        ));
        assert_eq!(NexMarket::indexer_pool_share_bps(), 10000);
    });
}

#[test]
fn set_indexer_pool_share_rejects_over_max() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::set_indexer_pool_share(RuntimeOrigin::root(), 10001),
            Error::<Test>::FeeTooHigh,
        );
    });
}

#[test]
fn trading_fee_split_to_pool_and_treasury() {
    new_test_ext().execute_with(|| {
        setup_seed_price();

        // 设置 1% 手续费 + 50% 分给奖池
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100));
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            5000
        ));

        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let treasury_before = Balances::free_balance(99u64);
        let pool_before = Balances::free_balance(97u64);

        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            50_000_000,
            make_tx_hashes(b"tx_pool_split"),
        ));

        let treasury_after = Balances::free_balance(99u64);
        let pool_after = Balances::free_balance(97u64);

        // 1% of 100 NEX = 1 NEX = 1_000_000_000_000
        // 50% to pool = 500_000_000_000, 50% to treasury = 500_000_000_000
        let expected_total_fee = 1_000_000_000_000u128;
        let expected_pool = expected_total_fee / 2;
        let expected_treasury = expected_total_fee - expected_pool;

        assert_eq!(
            treasury_after - treasury_before,
            expected_treasury,
            "Treasury should receive 50% of fee"
        );
        assert_eq!(
            pool_after - pool_before,
            expected_pool,
            "Reward pool should receive 50% of fee"
        );
    });
}

#[test]
fn indexer_reward_paid_from_pool_not_treasury() {
    new_test_ext().execute_with(|| {
        setup_seed_price();

        // 设置手续费 1% + 100% 分给奖池
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100));
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            10000
        ));

        register_indexer_helper(INDEXER);
        let trade_id = setup_indexer_trade_awaiting_verification();

        // Indexer 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"tx_pool_reward".to_vec(),
            50_000_000,
        ));

        let pool_before = Balances::free_balance(97u64);
        let treasury_before = Balances::free_balance(99u64);
        let indexer_before = Balances::free_balance(INDEXER);

        // OCW 验证成功
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            trade_id,
            50_000_000,
            make_tx_hashes(b"tx_pool_reward"),
        ));

        let pool_after = Balances::free_balance(97u64);
        let treasury_after = Balances::free_balance(99u64);
        let indexer_after = Balances::free_balance(INDEXER);

        // 奖励 = IndexerHintReward = 0.01 NEX = 10_000_000_000
        let reward = 10_000_000_000u128;

        // 奖池余额减少（手续费进来，奖励出去）
        // 手续费 1% of 100 NEX = 1 NEX，全部进奖池
        let fee_into_pool = 1_000_000_000_000u128;
        // 奖池净变化 = +fee_into_pool - reward
        assert_eq!(
            pool_after - pool_before,
            fee_into_pool - reward,
            "Pool should gain fee minus reward"
        );

        // 国库不应收到任何手续费（100% 分给奖池）
        assert_eq!(
            treasury_after, treasury_before,
            "Treasury should not receive any fee when pool share is 100%"
        );

        // Indexer 收到奖励
        assert!(
            indexer_after > indexer_before,
            "Indexer should receive reward from pool"
        );
    });
}

#[test]
fn zero_pool_share_all_fee_goes_to_treasury() {
    new_test_ext().execute_with(|| {
        setup_seed_price();

        // 设置 1% 手续费 + 0% 分给奖池（默认）
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100));
        // IndexerPoolShareBps 默认为 0

        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        let treasury_before = Balances::free_balance(99u64);
        let pool_before = Balances::free_balance(97u64);

        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            50_000_000,
            make_tx_hashes(b"tx_zero_share"),
        ));

        let treasury_after = Balances::free_balance(99u64);
        let pool_after = Balances::free_balance(97u64);

        // 全部进国库
        assert_eq!(treasury_after - treasury_before, 1_000_000_000_000u128);
        assert_eq!(
            pool_after, pool_before,
            "Pool should not change with 0% share"
        );
    });
}

#[test]
fn underpaid_path_also_splits_fee_and_rewards_indexer() {
    new_test_ext().execute_with(|| {
        setup_seed_price();

        // 设置 2% 手续费 + 50% 分给奖池
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 200));
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            5000
        ));

        register_indexer_helper(INDEXER);

        let nex = 100_000_000_000_000u128; // 100 NEX
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        assert_ok!(NexMarket::confirm_payment(RuntimeOrigin::signed(BOB), 0));

        // Indexer 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            0,
            b"tx_underpaid_split".to_vec(),
            10_000_000,
        ));

        let indexer_before = Balances::free_balance(INDEXER);

        // OCW 报告严重少付 (20% 付款 → SeverelyUnderpaid)
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            0,
            10_000_000,
            make_tx_hashes(b"tx_underpaid_split"),
        ));

        let indexer_after = Balances::free_balance(INDEXER);

        // BUG-1 修复验证：Indexer 在少付场景也应获得奖励
        let reward = 10_000_000_000u128; // 0.01 NEX
        assert_eq!(
            indexer_after - indexer_before,
            reward,
            "Indexer should be rewarded even in underpaid scenario"
        );

        // P9: Indexer hint 已在交易终态后清理
        assert!(NexMarket::indexer_hints(0).is_none());

        // pending_hint_count 应该归零
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(
            info.pending_hint_count, 0,
            "pending_hint_count should be 0 after settlement"
        );
        assert_eq!(info.accelerated_count, 1);
    });
}

#[test]
fn pool_depleted_emits_event_does_not_block_settlement() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        setup_seed_price();

        // 不设置奖池分成（pool_share = 0），使结算时手续费全进国库
        assert_ok!(NexMarket::set_trading_fee(RuntimeOrigin::root(), 100));

        register_indexer_helper(INDEXER);
        let trade_id = setup_indexer_trade_awaiting_verification();

        // Indexer 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"tx_depleted".to_vec(),
            50_000_000,
        ));

        // 掏空 Indexer 奖池（RewardSource = 97）
        let pool_balance = Balances::free_balance(97u64);
        let _ = Balances::deposit_creating(&88u64, 1);
        let _ = <Balances as Currency<u64>>::transfer(
            &97u64,
            &88u64,
            pool_balance,
            frame_support::traits::ExistenceRequirement::AllowDeath,
        );
        assert_eq!(Balances::free_balance(97u64), 0);

        // OCW 验证成功 — 交易应正常结算，奖励发放失败但不阻断
        assert_ok!(NexMarket::submit_ocw_result(
            RuntimeOrigin::none(),
            trade_id,
            50_000_000,
            make_tx_hashes(b"tx_depleted"),
        ));

        // 交易成功完成
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);

        // 检查 IndexerRewardPoolDepleted 事件
        let events = System::events();
        let depleted_event = events.iter().find(|e| {
            matches!(
                e.event,
                RuntimeEvent::NexMarket(Event::IndexerRewardPoolDepleted { .. })
            )
        });
        assert!(
            depleted_event.is_some(),
            "Should emit IndexerRewardPoolDepleted event"
        );
    });
}

// ==================== 保证金没收三方分配测试 ====================

#[test]
fn deposit_forfeit_three_way_split_on_timeout() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        setup_seed_price();

        // 设置奖池分成 20%（平台 50% 中的 20% = 总保证金的 10% 给奖池）
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            2000
        ));

        let nex = 100_000_000_000_000u128; // 100 NEX
        let price = 500_000u64;

        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        assert!(!deposit.is_zero());

        let seller_before = Balances::free_balance(ALICE);
        let treasury_before = Balances::free_balance(99u64);
        let pool_before = Balances::free_balance(97u64);

        // 超时
        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let seller_after = Balances::free_balance(ALICE);
        let treasury_after = Balances::free_balance(99u64);
        let pool_after = Balances::free_balance(97u64);

        // 保证金分配: 50% seller, 50% platform (80% treasury + 20% pool)
        let deposit_u128: u128 = deposit;
        let seller_half = deposit_u128 / 2;
        let platform_half = deposit_u128 - seller_half;
        let expected_pool = platform_half * 2000 / 10000; // 20% of platform
        let expected_treasury = platform_half - expected_pool;

        // 卖家还拿回 unreserve 的 NEX，所以用增量判断
        let seller_gain = seller_after - seller_before;
        // seller 得到: nex_amount unreserve (100 NEX) + deposit 50%
        assert_eq!(
            seller_gain,
            nex + seller_half,
            "Seller should get NEX back + 50% deposit"
        );

        assert_eq!(
            treasury_after - treasury_before,
            expected_treasury,
            "Treasury should get 80% of platform half"
        );
        assert_eq!(
            pool_after - pool_before,
            expected_pool,
            "Pool should get 20% of platform half"
        );
    });
}

#[test]
fn deposit_forfeit_zero_pool_share_backward_compat() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        setup_seed_price();

        // 奖池分成 = 0（默认），所有平台份额归国库
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;

        let treasury_before = Balances::free_balance(99u64);
        let pool_before = Balances::free_balance(97u64);

        System::set_block_number(10000);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(BOB), 0));

        let treasury_after = Balances::free_balance(99u64);
        let pool_after = Balances::free_balance(97u64);

        // 平台份额 100% 归国库
        let deposit_u128: u128 = deposit;
        let platform_half = deposit_u128 - deposit_u128 / 2;
        assert_eq!(treasury_after - treasury_before, platform_half);
        assert_eq!(
            pool_after, pool_before,
            "Pool should not change with 0% share"
        );
    });
}

#[test]
fn on_idle_penalty_splits_to_pool() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        setup_seed_price();

        // 奖池分成 50%
        assert_ok!(NexMarket::set_indexer_pool_share(
            RuntimeOrigin::root(),
            5000
        ));

        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));

        let trade = NexMarket::usdt_trades(0).unwrap();
        let deposit = trade.buyer_deposit;
        assert!(!deposit.is_zero());

        let pool_before = Balances::free_balance(97u64);
        let treasury_before = Balances::free_balance(99u64);
        let seller_before = Balances::free_balance(ALICE);

        // 宽限期 1200 blocks + 1h = 600 blocks = 1800
        // DepositPenaltyRatePerHour = 500 bps = 5%
        System::set_block_number(1 + 1200 + 600);
        NexMarket::on_idle(
            1u64 + 1200 + 600,
            Weight::from_parts(1_000_000_000_000, 100_000),
        );

        let pool_after = Balances::free_balance(97u64);
        let treasury_after = Balances::free_balance(99u64);
        let seller_after = Balances::free_balance(ALICE);

        // 1 小时罚金: deposit × 5% = deposit × 500 / 10000
        let deposit_u128: u128 = deposit;
        let penalty = deposit_u128 * 500 / 10000;
        let seller_half = penalty / 2;
        let platform_half = penalty - seller_half;
        let expected_pool = platform_half * 5000 / 10000; // 50% of platform
        let expected_treasury = platform_half - expected_pool;

        assert_eq!(
            seller_after - seller_before,
            seller_half,
            "Seller gets 50% of penalty"
        );
        assert_eq!(
            treasury_after - treasury_before,
            expected_treasury,
            "Treasury gets 50% of platform half"
        );
        assert_eq!(
            pool_after - pool_before,
            expected_pool,
            "Pool gets 50% of platform half"
        );
    });
}

// ==================== P1: force_remove_indexer 清理孤儿 hints 测试 ====================

#[test]
fn force_remove_indexer_cleans_pending_hints() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint → pending_hint_count = 1
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_orphan".to_vec(),
            50_000_000,
        ));
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info.pending_hint_count, 1);

        // hint 存在
        assert!(NexMarket::indexer_hints(trade_id).is_some());

        // 强制移除 → 应清理 hint
        assert_ok!(NexMarket::force_remove_indexer(
            RuntimeOrigin::root(),
            INDEXER
        ));

        // Indexer 已移除
        assert!(NexMarket::indexer_set(INDEXER).is_none());
        assert_eq!(NexMarket::indexer_count(), 0);

        // hint 已清理（槽位释放）
        assert!(NexMarket::indexer_hints(trade_id).is_none());

        // 质押已退还
        assert_eq!(Balances::reserved_balance(INDEXER), 0);
    });
}

#[test]
fn force_remove_indexer_slot_released_allows_new_hint() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // INDEXER 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_old".to_vec(),
            50_000_000,
        ));

        // 强制移除 INDEXER（清理 hint）
        assert_ok!(NexMarket::force_remove_indexer(
            RuntimeOrigin::root(),
            INDEXER
        ));

        // 注册新 Indexer 并提交 hint → 槽位已释放，应成功
        let _ = Balances::deposit_creating(&10u64, 500_000_000_000_000);
        assert_ok!(NexMarket::register_indexer(
            RuntimeOrigin::signed(10),
            b"https://indexer2.example.com".to_vec(),
        ));
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(10),
            trade_id,
            b"txhash_new".to_vec(),
            50_000_000,
        ));

        let hint = NexMarket::indexer_hints(trade_id).unwrap();
        assert_eq!(hint.indexer, 10);
    });
}

#[test]
fn force_remove_indexer_no_hints_no_cleanup() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);

        // 没有 pending hints 时强制移除 → 正常
        assert_ok!(NexMarket::force_remove_indexer(
            RuntimeOrigin::root(),
            INDEXER
        ));
        assert!(NexMarket::indexer_set(INDEXER).is_none());
        assert_eq!(Balances::reserved_balance(INDEXER), 0);
    });
}

// ==================== P2: reinstate_indexer 测试 ====================

#[test]
fn reinstate_indexer_works() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);

        // 手动设置为 suspended
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.suspended = true;
                info.error_count = 5;
            }
        });

        let info_before = NexMarket::indexer_set(INDEXER).unwrap();
        assert!(info_before.suspended);
        assert_eq!(info_before.error_count, 5);

        // 管理员恢复
        assert_ok!(NexMarket::reinstate_indexer(RuntimeOrigin::root(), INDEXER));

        let info_after = NexMarket::indexer_set(INDEXER).unwrap();
        assert!(!info_after.suspended);
        assert_eq!(info_after.error_count, 0);
        // accelerated_count 不变
        assert_eq!(info_after.accelerated_count, info_before.accelerated_count);
    });
}

#[test]
fn reinstate_indexer_not_suspended_fails() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);

        // 未暂停的 Indexer → 恢复失败
        assert_noop!(
            NexMarket::reinstate_indexer(RuntimeOrigin::root(), INDEXER),
            Error::<Test>::IndexerNotSuspended
        );
    });
}

#[test]
fn reinstate_indexer_not_found_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            NexMarket::reinstate_indexer(RuntimeOrigin::root(), INDEXER),
            Error::<Test>::IndexerNotFound
        );
    });
}

#[test]
fn reinstate_indexer_non_admin_fails() {
    new_test_ext().execute_with(|| {
        register_indexer_helper(INDEXER);
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.suspended = true;
            }
        });

        // 普通用户不能恢复
        assert_noop!(
            NexMarket::reinstate_indexer(RuntimeOrigin::signed(BOB), INDEXER),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn reinstate_then_submit_hint_works() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 暂停 Indexer
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.suspended = true;
                info.error_count = 3;
            }
        });

        // 暂停后不能提交 hint
        assert_noop!(
            NexMarket::submit_indexer_hint(
                RuntimeOrigin::signed(INDEXER),
                trade_id,
                b"txhash_no".to_vec(),
                50_000_000,
            ),
            Error::<Test>::IndexerIsSuspended
        );

        // 恢复后可以提交 hint
        assert_ok!(NexMarket::reinstate_indexer(RuntimeOrigin::root(), INDEXER));
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_yes".to_vec(),
            50_000_000,
        ));

        let hint = NexMarket::indexer_hints(trade_id).unwrap();
        assert_eq!(hint.indexer, INDEXER);
    });
}

// ==================== P3: Indexer 暂停时 slash 质押测试 ====================

#[test]
fn indexer_mismatch_auto_suspend_slashes_stake() {
    new_test_ext().execute_with(|| {
        // MaxIndexerErrors = 3, IndexerSlashRateBps = 3000 (30%)
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        let stake = 100_000_000_000_000u128; // 100 NEX
        let treasury_before = Balances::free_balance(99u64);

        // 预设 error_count = 2（差一次暂停）
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.error_count = 2;
            }
        });

        // 提交 hint + mismatch → 第 3 次错误 → 暂停 + slash
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_slash".to_vec(),
            50_000_000,
        ));
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        // 确认暂停
        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert!(info.suspended);
        assert_eq!(info.error_count, 3);

        // Slash 30%: 100 NEX × 30% = 30 NEX
        let expected_slash = stake * 3000 / 10000;
        let expected_remaining_stake = stake - expected_slash;
        assert_eq!(info.stake, expected_remaining_stake);

        // 国库收到 slash 金额
        assert_eq!(
            Balances::free_balance(99u64) - treasury_before,
            expected_slash,
        );

        // Indexer reserved 减少了 slash 部分
        assert_eq!(
            Balances::reserved_balance(INDEXER),
            expected_remaining_stake
        );
    });
}

#[test]
fn indexer_mismatch_below_threshold_no_slash() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        let treasury_before = Balances::free_balance(99u64);

        // error_count = 0，第一次 mismatch → error_count=1，不暂停
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_no_slash".to_vec(),
            50_000_000,
        ));
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert!(!info.suspended);
        assert_eq!(info.error_count, 1);

        // 质押不变
        assert_eq!(info.stake, 100_000_000_000_000);
        assert_eq!(Balances::reserved_balance(INDEXER), 100_000_000_000_000);

        // 国库不变
        assert_eq!(Balances::free_balance(99u64), treasury_before);
    });
}

#[test]
fn suspended_indexer_can_deregister_with_reduced_stake() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        let stake = 100_000_000_000_000u128; // 100 NEX
        let slash_amount = stake * 3000 / 10000; // 30%
        let remaining_stake = stake - slash_amount;

        // 触发暂停 + slash
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.error_count = 2;
            }
        });

        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_dereg".to_vec(),
            50_000_000,
        ));
        assert_ok!(NexMarket::report_indexer_mismatch(
            RuntimeOrigin::none(),
            trade_id,
        ));

        let info = NexMarket::indexer_set(INDEXER).unwrap();
        assert!(info.suspended);
        assert_eq!(info.stake, remaining_stake);

        // slash 后 reserved = remaining_stake
        assert_eq!(Balances::reserved_balance(INDEXER), remaining_stake);
        let free_before_deregister = Balances::free_balance(INDEXER);

        // hint 已被 mismatch 清除，pending_hint_count = 0 → 可以 deregister
        assert_ok!(NexMarket::deregister_indexer(RuntimeOrigin::signed(
            INDEXER
        )));

        // 退还的是 slash 后的质押
        assert_eq!(Balances::reserved_balance(INDEXER), 0);
        assert_eq!(
            Balances::free_balance(INDEXER),
            free_before_deregister + remaining_stake,
            "Should get back remaining stake after slash"
        );
    });
}

// ==================== P5: InvalidEndpointUrl 测试 ====================

#[test]
fn register_indexer_url_too_long_fails_with_correct_error() {
    new_test_ext().execute_with(|| {
        // 超过 256 字节的 URL
        let long_url = vec![b'x'; 300];
        assert_noop!(
            NexMarket::register_indexer(RuntimeOrigin::signed(INDEXER), long_url),
            Error::<Test>::InvalidEndpointUrl
        );
    });
}

// ==================== P7: 暂停后 consecutive_violations 保留测试 ====================

#[test]
fn credit_suspension_retains_consecutive_violations() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        // 完成多笔交易提升信用分（需承受 3+2 次扣分）
        for _ in 0..6 {
            do_complete_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        }
        let p = NexMarket::buyer_credit_profiles(BOB).unwrap();
        // 500 + 50*3 + 10*3 = 680
        assert_eq!(p.credit_score, 680);

        // 3 次连续超时 → 暂停，consecutive_violations = 1（P7）
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -50 → 630
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -100 → 530
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000); // -200 → 330, suspended
        let p1 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert!(p1.is_suspended);
        assert_eq!(p1.consecutive_violations, 1); // P7: 保留 1

        // 模拟解除暂停后再犯 — 直接修改 profile 模拟恢复
        let mut p2 = p1.clone();
        p2.is_suspended = false;
        p2.suspension_until = None;
        p2.credit_score = 600; // 手动恢复信用分以允许交易
                               // consecutive_violations 保留为 1
        BuyerCreditProfiles::<Test>::insert(BOB, p2);

        // 第 1 次再犯 → cv = 2
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p3 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert_eq!(p3.consecutive_violations, 2);
        assert!(!p3.is_suspended);

        // 第 2 次再犯 → cv = 3 → 再次暂停（比原来少 1 次机会）
        do_timeout_trade(ALICE, BOB, 10_000_000_000_000, 500_000);
        let p4 = NexMarket::buyer_credit_profiles(BOB).unwrap();
        assert!(
            p4.is_suspended,
            "Should be suspended again after 2 more violations (1+2=3)"
        );
        assert_eq!(p4.consecutive_violations, 1); // 再次保留 1
    });
}

// ==================== P9: IndexerHints GC 测试 ====================

#[test]
fn hint_cleaned_after_trade_completion() {
    new_test_ext().execute_with(|| {
        let trade_id = setup_indexer_trade_awaiting_verification();
        register_indexer_helper(INDEXER);

        // 提交 hint
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            trade_id,
            b"txhash_gc".to_vec(),
            50_000_000,
        ));
        assert!(NexMarket::indexer_hints(trade_id).is_some());

        // OCW 验证通过 → 交易完成
        let trade = NexMarket::usdt_trades(trade_id).unwrap();
        assert_ok!(call_submit_ocw_result(
            trade_id,
            trade.usdt_amount,
            make_tx_hashes(b"txhash_gc_ocw")
        ));

        // hint 已被清理
        assert!(NexMarket::indexer_hints(trade_id).is_none());
    });
}

#[test]
fn hint_cleaned_after_trade_timeout() {
    new_test_ext().execute_with(|| {
        setup_seed_price();
        let nex = 100_000_000_000_000u128;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            500_000,
            tron_address(),
            None,
        ));
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0,
            None,
            buyer_tron(),
        ));
        let trade = NexMarket::usdt_trades(0).unwrap();

        register_indexer_helper(INDEXER);
        assert_ok!(NexMarket::submit_indexer_hint(
            RuntimeOrigin::signed(INDEXER),
            0,
            b"txhash_timeout_gc".to_vec(),
            50_000_000,
        ));
        assert!(NexMarket::indexer_hints(0).is_some());

        // Indexer pending_hint_count = 1
        let info_before = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info_before.pending_hint_count, 1);

        // 超时
        let timeout_block: u64 = trade.timeout_at.into();
        System::set_block_number(timeout_block + 1);
        assert_ok!(NexMarket::process_timeout(RuntimeOrigin::signed(ALICE), 0));

        // hint 已被清理
        assert!(NexMarket::indexer_hints(0).is_none());
        // pending_hint_count 已递减
        let info_after = NexMarket::indexer_set(INDEXER).unwrap();
        assert_eq!(info_after.pending_hint_count, 0);
    });
}

// ==================== P10: IndexerNetworkSummary 命名测试 ====================

#[test]
fn indexer_network_summary_counts_correct() {
    new_test_ext().execute_with(|| {
        // 注册 2 个 Indexer
        register_indexer_helper(INDEXER);
        let _ = Balances::deposit_creating(&10u64, 500_000_000_000_000);
        assert_ok!(NexMarket::register_indexer(
            RuntimeOrigin::signed(10),
            b"https://idx2.example.com".to_vec(),
        ));

        // 暂停其中一个
        IndexerSet::<Test>::mutate(INDEXER, |maybe| {
            if let Some(ref mut info) = maybe {
                info.suspended = true;
            }
        });

        let summary = NexMarket::api_get_indexer_network_summary();
        assert_eq!(summary.total_count, 2);
        assert_eq!(summary.active_count, 1); // total - suspended
        assert_eq!(summary.suspended_count, 1);
    });
}

// ==================== H-1 审计修复: TTL GC 安全防护 ====================

#[test]
fn h1_gc_uses_max_of_ttl_and_min_ttl() {
    // MinTxHashTtlBlocks = 86400, TxHashTtlBlocks = 100800
    // effective TTL = max(100800, 86400) = 100800
    // 此测试验证当 entry 已超过 MinTxHashTtlBlocks 但未超过 TxHashTtlBlocks 时不被清理
    new_test_ext().execute_with(|| {
        let hash: TxHash =
            BoundedVec::try_from(b"h1_replay_guard_test_hash_abc123456".to_vec()).unwrap();
        UsedTxHashes::<Test>::insert(&hash, (42u64, 1u64));

        // 跳到 MinTxHashTtlBlocks (86400) 之后但 TxHashTtlBlocks (100800) 之前
        System::set_block_number(90000);
        NexMarket::on_idle(90000u64, Weight::from_parts(u64::MAX, u64::MAX));

        // 不应被清理（effective TTL = 100800）
        assert!(
            UsedTxHashes::<Test>::contains_key(&hash),
            "H-1: tx_hash 不应在 effective TTL 到期前被 GC"
        );

        // 跳到 effective TTL 之后
        System::set_block_number(100802);
        NexMarket::on_idle(100802u64, Weight::from_parts(u64::MAX, u64::MAX));

        // 此时应被清理
        assert!(
            !UsedTxHashes::<Test>::contains_key(&hash),
            "tx_hash 应在 effective TTL 到期后被 GC"
        );
    });
}

// ==================== M-4 审计修复: admin_reset_credit + unban_user 信用重置 ====================

#[test]
fn m4_admin_reset_credit_works() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        // 模拟买家信用分归零
        let profile = BuyerCreditProfile {
            credit_score: 0,
            completed_orders: 15,
            total_violations: 10,
            consecutive_good_orders: 0,
            consecutive_violations: 3,
            is_suspended: true,
            suspension_until: Some(999999u64),
            last_violation_at: Some(100u64),
        };
        BuyerCreditProfiles::<Test>::insert(BOB, &profile);

        // 非管理员不能重置
        assert_noop!(
            NexMarket::admin_reset_credit(RuntimeOrigin::signed(ALICE), BOB),
            sp_runtime::DispatchError::BadOrigin,
        );

        // 管理员重置
        assert_ok!(NexMarket::admin_reset_credit(RuntimeOrigin::root(), BOB));

        // 检查重置结果
        let new_profile = BuyerCreditProfiles::<Test>::get(BOB).unwrap();
        assert_eq!(new_profile.credit_score, 500);
        assert_eq!(new_profile.completed_orders, 15); // 保留历史
        assert_eq!(new_profile.total_violations, 10); // 保留历史
        assert_eq!(new_profile.consecutive_violations, 0);
        assert_eq!(new_profile.consecutive_good_orders, 0);
        assert!(!new_profile.is_suspended);
        assert!(new_profile.suspension_until.is_none());
        assert!(new_profile.last_violation_at.is_none());

        // CreditReset 事件
        System::assert_has_event(RuntimeEvent::NexMarket(Event::CreditReset {
            account: BOB,
            old_score: 0,
            new_score: 500,
        }));
    });
}

#[test]
fn m4_unban_user_resets_credit() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);

        // 模拟信用分归零导致的自动封禁
        let profile = BuyerCreditProfile {
            credit_score: 0,
            completed_orders: 5,
            total_violations: 8,
            consecutive_good_orders: 0,
            consecutive_violations: 1,
            is_suspended: false,
            suspension_until: None,
            last_violation_at: Some(50u64),
        };
        BuyerCreditProfiles::<Test>::insert(BOB, &profile);
        BannedAccounts::<Test>::insert(BOB, true);

        // 解封前: 被 ban
        assert!(NexMarket::is_banned(BOB));

        // 管理员解封
        assert_ok!(NexMarket::unban_user(RuntimeOrigin::root(), BOB));

        // 解封后: ban 已移除
        assert!(!NexMarket::is_banned(BOB));

        // 信用已重置到 500（修复前: 仍为 0，ensure_not_suspended 会阻止交易）
        let new_profile = BuyerCreditProfiles::<Test>::get(BOB).unwrap();
        assert_eq!(new_profile.credit_score, 500);
        assert!(!new_profile.is_suspended);

        // 验证用户现在可以正常交易（不被 ensure_not_suspended 阻止）
        System::set_block_number(1);
        let nex = 10_000_000_000_000u128;
        let price = 500_000u64;
        assert_ok!(NexMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            nex,
            price,
            tron_address(),
            None,
        ));
        // 解封后的 BOB 应可正常吃单（不再被 BuyerSuspended 阻止）
        assert_ok!(NexMarket::reserve_sell_order(
            RuntimeOrigin::signed(BOB),
            0, // order_id (从 0 开始)
            None,
            buyer_tron(),
        ));
    });
}

#[test]
fn m4_admin_reset_credit_for_new_user_is_noop_safe() {
    new_test_ext().execute_with(|| {
        // 对没有 credit profile 的用户调用 reset — 不应 panic
        assert_ok!(NexMarket::admin_reset_credit(
            RuntimeOrigin::root(),
            CHARLIE
        ));

        // 创建了默认 profile（score=500）
        let profile = BuyerCreditProfiles::<Test>::get(CHARLIE).unwrap();
        assert_eq!(profile.credit_score, 500);
    });
}
