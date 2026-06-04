//! Benchmarking for pallet-nex-market.
//!
//! Uses `frame_benchmarking::v2` macro style.
//! All extrinsics are benchmarked with realistic storage setup.
//!
//! 注意：unsigned extrinsics (submit_ocw_result, auto_confirm_payment, submit_underpaid_update,
//! finalize_underpaid) 通过直接写入存储来模拟前置状态，避免依赖 OCW。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{
    traits::{Currency, Get, ReservableCurrency},
    BoundedVec,
};
use pallet_trading_common::DepositCalculator;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use sp_runtime::traits::{Bounded, Saturating, Zero};
use sp_runtime::SaturatedConversion;

/// 创建一个用于 benchmark 的 TxHashVec（包含一条 dummy hash）
fn bench_tx_hashes() -> TxHashVec {
    let hash: TxHash = b"bench_tx_hash_0123456789abcdef"
        .to_vec()
        .try_into()
        .unwrap_or_default();
    BoundedVec::try_from(alloc::vec![hash]).unwrap_or_default()
}

/// 创建一个有足够余额的账户
fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
    let account: T::AccountId = frame_benchmarking::account(name, index, 0);
    let amount = BalanceOf::<T>::max_value() / 8u32.into();
    let _ = T::Currency::deposit_creating(&account, amount);
    account
}

/// 有效的 TRON 地址（34 字节 Base58）
fn valid_tron_address() -> Vec<u8> {
    b"T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb".to_vec()
}

/// 另一个有效的 TRON 地址
fn valid_tron_address_2() -> Vec<u8> {
    b"TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t".to_vec()
}

/// 标准测试 NEX 数量（10 × 最小值）
fn standard_nex<T: Config>() -> BalanceOf<T> {
    T::MinOrderNexAmount::get().saturating_mul(10u32.into())
}

/// 标准 USDT 价格（0.5 USDT/NEX = 500_000）
const STANDARD_PRICE: u64 = 500_000;

/// 设置初始价格（TWAP 冷启动必需）
fn setup_initial_price<T: Config>() {
    PriceProtectionStore::<T>::put(PriceProtectionConfig {
        enabled: false,
        max_price_deviation: 5000,
        circuit_breaker_threshold: 5000,
        min_trades_for_twap: 100,
        circuit_breaker_active: false,
        circuit_breaker_until: 0,
        initial_price: Some(STANDARD_PRICE),
    });
    LastTradePrice::<T>::put(STANDARD_PRICE);
}

/// 计算买家保证金（复制 pallet 内部逻辑，因为 calculate_buyer_deposit 是 private）
fn calc_buyer_deposit<T: Config>(usdt_amount: u64) -> BalanceOf<T> {
    let rate = T::BuyerDepositRate::get();
    let usdt_deposit = (usdt_amount as u64)
        .saturating_mul(rate as u64)
        .saturating_div(10_000u64);
    let min_deposit =
        T::DepositCalculator::calculate_deposit(T::MinBuyerDepositUsd::get(), Zero::zero());
    let calculated = T::DepositCalculator::calculate_deposit(usdt_deposit, min_deposit);
    if calculated > min_deposit {
        calculated
    } else {
        min_deposit
    }
}

/// 创建一个 Open 状态的卖单，返回 (seller, order_id)
fn seed_sell_order<T: Config>(seller_index: u32) -> (T::AccountId, u64) {
    let seller = funded_account::<T>("seller", seller_index);
    let order_id = NextOrderId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();
    let ttl: BlockNumberFor<T> = T::DefaultOrderTTL::get().into();
    let nex = standard_nex::<T>();
    let tron: TronAddress = valid_tron_address().try_into().unwrap();

    let order = Order::<T> {
        order_id,
        maker: seller.clone(),
        side: OrderSide::Sell,
        nex_amount: nex,
        filled_amount: Zero::zero(),
        usdt_price: STANDARD_PRICE,
        tron_address: Some(tron),
        status: OrderStatus::Open,
        created_at: now,
        expires_at: now.saturating_add(ttl),
        buyer_deposit: Zero::zero(),
        deposit_waived: false,
        min_fill_amount: Zero::zero(),
    };

    // 锁定 NEX
    let _ = T::Currency::reserve(&seller, nex);

    Orders::<T>::insert(order_id, order);
    let _ = SellOrders::<T>::try_mutate(|ids| ids.try_push(order_id));
    let _ = UserOrders::<T>::try_mutate(&seller, |ids| ids.try_push(order_id));
    NextOrderId::<T>::put(order_id.saturating_add(1));

    (seller, order_id)
}

/// 创建一个 Open 状态的买单，返回 (buyer, order_id)
fn seed_buy_order<T: Config>(buyer_index: u32) -> (T::AccountId, u64) {
    let buyer = funded_account::<T>("buyer", buyer_index);
    let order_id = NextOrderId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();
    let ttl: BlockNumberFor<T> = T::DefaultOrderTTL::get().into();
    let nex = standard_nex::<T>();
    let tron: TronAddress = valid_tron_address_2().try_into().unwrap();

    let nex_u128: u128 = nex.saturated_into();
    let usdt_total = (nex_u128 * STANDARD_PRICE as u128 / 1_000_000_000_000u128) as u64;
    let deposit = calc_buyer_deposit::<T>(usdt_total);
    if !deposit.is_zero() {
        let _ = T::Currency::reserve(&buyer, deposit);
    }

    let order = Order::<T> {
        order_id,
        maker: buyer.clone(),
        side: OrderSide::Buy,
        nex_amount: nex,
        filled_amount: Zero::zero(),
        usdt_price: STANDARD_PRICE,
        tron_address: Some(tron),
        status: OrderStatus::Open,
        created_at: now,
        expires_at: now.saturating_add(ttl),
        buyer_deposit: deposit,
        deposit_waived: false,
        min_fill_amount: Zero::zero(),
    };

    Orders::<T>::insert(order_id, order);
    let _ = BuyOrders::<T>::try_mutate(|ids| ids.try_push(order_id));
    let _ = UserOrders::<T>::try_mutate(&buyer, |ids| ids.try_push(order_id));
    NextOrderId::<T>::put(order_id.saturating_add(1));

    (buyer, order_id)
}

/// 创建一个 AwaitingPayment 状态的 UsdtTrade，返回 (seller, buyer, trade_id)
fn seed_trade_awaiting_payment<T: Config>() -> (T::AccountId, T::AccountId, u64) {
    let (seller, order_id) = seed_sell_order::<T>(0);
    let buyer = funded_account::<T>("buyer", 0);
    let trade_id = NextUsdtTradeId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();
    let timeout: BlockNumberFor<T> = T::UsdtTimeout::get().into();
    let nex = standard_nex::<T>();
    let nex_u128: u128 = nex.saturated_into();
    let usdt_amount = (nex_u128 * STANDARD_PRICE as u128 / 1_000_000_000_000u128) as u64;

    let deposit = calc_buyer_deposit::<T>(usdt_amount);
    if !deposit.is_zero() {
        let _ = T::Currency::reserve(&buyer, deposit);
    }

    let seller_tron: TronAddress = valid_tron_address().try_into().unwrap();
    let buyer_tron: TronAddress = valid_tron_address_2().try_into().unwrap();

    let trade = UsdtTrade::<T> {
        trade_id,
        order_id,
        seller: seller.clone(),
        buyer: buyer.clone(),
        nex_amount: nex,
        usdt_amount,
        seller_tron_address: seller_tron,
        buyer_tron_address: Some(buyer_tron),
        status: UsdtTradeStatus::AwaitingPayment,
        created_at: now,
        timeout_at: now.saturating_add(timeout),
        buyer_deposit: deposit,
        deposit_status: if deposit.is_zero() {
            BuyerDepositStatus::None
        } else {
            BuyerDepositStatus::Locked
        },
        first_verified_at: None,
        first_actual_amount: None,
        underpaid_deadline: None,
        completed_at: None,
        payment_confirmed: false,
        cumulative_penalty: Zero::zero(),
    };

    UsdtTrades::<T>::insert(trade_id, trade);
    let _ = PendingUsdtTrades::<T>::try_mutate(|ids| ids.try_push(trade_id));
    let _ = AwaitingPaymentTrades::<T>::try_mutate(|ids| ids.try_push(trade_id));
    let _ = UserTrades::<T>::try_mutate(&seller, |ids| ids.try_push(trade_id));
    let _ = UserTrades::<T>::try_mutate(&buyer, |ids| ids.try_push(trade_id));
    let _ = OrderTrades::<T>::try_mutate(order_id, |ids| ids.try_push(trade_id));
    NextUsdtTradeId::<T>::put(trade_id.saturating_add(1));

    // 更新订单为 Filled
    Orders::<T>::mutate(order_id, |o| {
        if let Some(order) = o {
            order.filled_amount = order.nex_amount;
            order.status = OrderStatus::Filled;
        }
    });

    (seller, buyer, trade_id)
}

/// 创建一个 AwaitingVerification 状态的 UsdtTrade
fn seed_trade_awaiting_verification<T: Config>() -> (T::AccountId, T::AccountId, u64, u64) {
    let (seller, buyer, trade_id) = seed_trade_awaiting_payment::<T>();
    let trade = UsdtTrades::<T>::get(trade_id).unwrap();
    let usdt_amount = trade.usdt_amount;

    UsdtTrades::<T>::mutate(trade_id, |t| {
        if let Some(trade) = t {
            trade.status = UsdtTradeStatus::AwaitingVerification;
            trade.payment_confirmed = true;
        }
    });

    // 从 AwaitingPayment 队列移除
    AwaitingPaymentTrades::<T>::mutate(|ids| ids.retain(|&id| id != trade_id));

    (seller, buyer, trade_id, usdt_amount)
}

/// 创建一个 UnderpaidPending 状态的 UsdtTrade
fn seed_trade_underpaid<T: Config>() -> (T::AccountId, T::AccountId, u64, u64) {
    let (seller, buyer, trade_id, usdt_amount) = seed_trade_awaiting_verification::<T>();
    let now = frame_system::Pallet::<T>::block_number();
    let grace: BlockNumberFor<T> = T::UnderpaidGracePeriod::get().into();
    let actual_80 = usdt_amount * 80 / 100;

    UsdtTrades::<T>::mutate(trade_id, |t| {
        if let Some(trade) = t {
            trade.status = UsdtTradeStatus::UnderpaidPending;
            trade.first_verified_at = Some(now);
            trade.first_actual_amount = Some(actual_80);
            trade.underpaid_deadline = Some(now.saturating_add(grace));
        }
    });

    // 从 PendingUsdtTrades 移除，加入 PendingUnderpaidTrades
    PendingUsdtTrades::<T>::mutate(|ids| ids.retain(|&id| id != trade_id));
    let _ = PendingUnderpaidTrades::<T>::try_mutate(|ids| ids.try_push(trade_id));

    // 写入 OCW 验证结果
    OcwVerificationResults::<T>::insert(
        trade_id,
        (PaymentVerificationResult::Underpaid, actual_80),
    );

    (seller, buyer, trade_id, usdt_amount)
}

/// 创建一个 Refunded + payment_confirmed 的交易（可争议）
fn seed_trade_refunded_disputable<T: Config>() -> (T::AccountId, T::AccountId, u64) {
    let (seller, buyer, trade_id, _) = seed_trade_awaiting_verification::<T>();
    let now = frame_system::Pallet::<T>::block_number();

    UsdtTrades::<T>::mutate(trade_id, |t| {
        if let Some(trade) = t {
            trade.status = UsdtTradeStatus::Refunded;
            trade.payment_confirmed = true;
            trade.completed_at = Some(now);
        }
    });

    PendingUsdtTrades::<T>::mutate(|ids| ids.retain(|&id| id != trade_id));

    (seller, buyer, trade_id)
}

#[benchmarks]
mod benches {
    use super::*;

    // ==================== call_index(0): place_sell_order ====================
    #[benchmark]
    fn place_sell_order() {
        setup_initial_price::<T>();
        let caller = funded_account::<T>("caller", 0);
        let nex = standard_nex::<T>();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            nex,
            STANDARD_PRICE,
            valid_tron_address(),
            None,
        );

        assert!(Orders::<T>::contains_key(0));
    }

    // ==================== call_index(1): place_buy_order ====================
    #[benchmark]
    fn place_buy_order() {
        setup_initial_price::<T>();
        let caller = funded_account::<T>("caller", 0);
        let nex = standard_nex::<T>();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            nex,
            STANDARD_PRICE,
            valid_tron_address_2(),
        );

        assert!(Orders::<T>::contains_key(0));
    }

    // ==================== call_index(2): cancel_order ====================
    #[benchmark]
    fn cancel_order() {
        setup_initial_price::<T>();
        let (seller, order_id) = seed_sell_order::<T>(0);

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), order_id);

        let order = Orders::<T>::get(order_id).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    // ==================== call_index(3): reserve_sell_order ====================
    #[benchmark]
    fn reserve_sell_order() {
        setup_initial_price::<T>();
        let (_seller, order_id) = seed_sell_order::<T>(0);
        let buyer = funded_account::<T>("buyer", 0);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(buyer),
            order_id,
            None,
            valid_tron_address_2(),
        );

        assert!(UsdtTrades::<T>::contains_key(0));
    }

    // ==================== call_index(4): accept_buy_order ====================
    #[benchmark]
    fn accept_buy_order() {
        setup_initial_price::<T>();
        let (_buyer, order_id) = seed_buy_order::<T>(0);
        let seller = funded_account::<T>("seller", 0);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(seller),
            order_id,
            None,
            valid_tron_address(),
        );

        assert!(UsdtTrades::<T>::contains_key(0));
    }

    // ==================== call_index(5): confirm_payment ====================
    #[benchmark]
    fn confirm_payment() {
        setup_initial_price::<T>();
        let (_seller, buyer, trade_id) = seed_trade_awaiting_payment::<T>();

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), trade_id);

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::AwaitingVerification);
    }

    // ==================== call_index(6): process_timeout ====================
    #[benchmark]
    fn process_timeout() {
        setup_initial_price::<T>();
        let (_seller, buyer, trade_id) = seed_trade_awaiting_payment::<T>();
        let trade = UsdtTrades::<T>::get(trade_id).unwrap();

        // 推进到超时后
        frame_system::Pallet::<T>::set_block_number(trade.timeout_at.saturating_add(1u32.into()));

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), trade_id);

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
    }

    // ==================== call_index(7): submit_ocw_result ====================
    #[benchmark]
    fn submit_ocw_result() {
        setup_initial_price::<T>();
        let (_seller, _buyer, trade_id, usdt_amount) = seed_trade_awaiting_verification::<T>();
        let tx_hashes = bench_tx_hashes();

        #[extrinsic_call]
        _(RawOrigin::None, trade_id, usdt_amount, tx_hashes);

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    }

    // ==================== call_index(8): claim_verification_reward ====================
    #[benchmark]
    fn claim_reward() {
        setup_initial_price::<T>();
        // 确保 RewardSource 有余额
        let reward_source = T::RewardSource::get();
        let amount = BalanceOf::<T>::max_value() / 8u32.into();
        let _ = T::Currency::deposit_creating(&reward_source, amount);

        let (_seller, _buyer, trade_id, usdt_amount) = seed_trade_awaiting_verification::<T>();
        // 记录 OCW 验证结果（trade 保持 AwaitingVerification 状态）
        OcwVerificationResults::<T>::insert(
            trade_id,
            (PaymentVerificationResult::Exact, usdt_amount),
        );

        let claimer = funded_account::<T>("claimer", 0);

        #[extrinsic_call]
        claim_verification_reward(RawOrigin::Signed(claimer), trade_id);
    }

    // ==================== call_index(9): configure_price_protection ====================
    #[benchmark]
    fn configure_price_protection() {
        #[extrinsic_call]
        _(RawOrigin::Root, true, 2000, 5000, 100);
    }

    // ==================== call_index(10): set_initial_price ====================
    #[benchmark]
    fn set_initial_price() {
        #[extrinsic_call]
        _(RawOrigin::Root, STANDARD_PRICE);
    }

    // ==================== call_index(11): lift_circuit_breaker ====================
    #[benchmark]
    fn lift_circuit_breaker() {
        // 激活熔断，设置到期区块为 0（已过期，可解除）
        PriceProtectionStore::<T>::put(PriceProtectionConfig {
            enabled: true,
            max_price_deviation: 2000,
            circuit_breaker_threshold: 5000,
            min_trades_for_twap: 100,
            circuit_breaker_active: true,
            circuit_breaker_until: 0,
            initial_price: Some(STANDARD_PRICE),
        });

        #[extrinsic_call]
        _(RawOrigin::Root);
    }

    // ==================== call_index(13): fund_seed_account ====================
    #[benchmark]
    fn fund_seed_account() {
        // fund_seed_account 需要 MarketAdminOrigin（Root），从国库转账到种子账户
        let treasury = T::TreasuryAccount::get();
        let fund_amount = BalanceOf::<T>::max_value() / 8u32.into();
        let _ = T::Currency::deposit_creating(&treasury, fund_amount);
        let amount: BalanceOf<T> = T::MinOrderNexAmount::get().saturating_mul(100u32.into());

        #[extrinsic_call]
        _(RawOrigin::Root, amount);
    }

    // ==================== call_index(14): seed_liquidity ====================
    #[benchmark]
    fn seed_liquidity() {
        setup_initial_price::<T>();
        let seed_account = T::SeedLiquidityAccount::get();
        let amount = BalanceOf::<T>::max_value() / 8u32.into();
        let _ = T::Currency::deposit_creating(&seed_account, amount);

        #[extrinsic_call]
        _(RawOrigin::Root, 5, None);
    }

    // ==================== call_index(15): auto_confirm_payment ====================
    #[benchmark]
    fn auto_confirm_payment() {
        setup_initial_price::<T>();
        let (_seller, _buyer, trade_id) = seed_trade_awaiting_payment::<T>();
        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        let tx_hashes = bench_tx_hashes();

        #[extrinsic_call]
        _(RawOrigin::None, trade_id, trade.usdt_amount, tx_hashes);
    }

    // ==================== call_index(16): submit_underpaid_update ====================
    #[benchmark]
    fn submit_underpaid_update() {
        setup_initial_price::<T>();
        let (_seller, _buyer, trade_id, usdt_amount) = seed_trade_underpaid::<T>();
        let new_amount = usdt_amount * 90 / 100;
        let new_tx_hashes = bench_tx_hashes();

        #[extrinsic_call]
        _(RawOrigin::None, trade_id, new_amount, new_tx_hashes);
    }

    // ==================== call_index(17): finalize_underpaid ====================
    #[benchmark]
    fn finalize_underpaid() {
        setup_initial_price::<T>();
        let (_seller, buyer, trade_id, _usdt_amount) = seed_trade_underpaid::<T>();

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        let deadline = trade.underpaid_deadline.unwrap();
        frame_system::Pallet::<T>::set_block_number(deadline.saturating_add(1u32.into()));

        // finalize_underpaid 需要 signed origin
        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), trade_id);
    }

    // ==================== call_index(18): force_pause_market ====================
    #[benchmark]
    fn force_pause_market() {
        #[extrinsic_call]
        _(RawOrigin::Root);

        assert!(MarketPausedStore::<T>::get());
    }

    // ==================== call_index(19): force_resume_market ====================
    #[benchmark]
    fn force_resume_market() {
        MarketPausedStore::<T>::put(true);

        #[extrinsic_call]
        _(RawOrigin::Root);

        assert!(!MarketPausedStore::<T>::get());
    }

    // ==================== call_index(20): force_settle_trade ====================
    #[benchmark]
    fn force_settle_trade() {
        setup_initial_price::<T>();
        let (_seller, _buyer, trade_id, usdt_amount) = seed_trade_awaiting_verification::<T>();

        #[extrinsic_call]
        _(
            RawOrigin::Root,
            trade_id,
            usdt_amount,
            DisputeResolution::ReleaseToBuyer,
        );

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    }

    // ==================== call_index(21): force_cancel_trade ====================
    #[benchmark]
    fn force_cancel_trade() {
        setup_initial_price::<T>();
        let (_seller, _buyer, trade_id) = seed_trade_awaiting_payment::<T>();

        #[extrinsic_call]
        _(RawOrigin::Root, trade_id);

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Refunded);
    }

    // ==================== call_index(22): dispute_trade ====================
    #[benchmark]
    fn dispute_trade() {
        setup_initial_price::<T>();
        let (_seller, buyer, trade_id) = seed_trade_refunded_disputable::<T>();
        let evidence = b"QmBenchmarkEvidenceCid1234567890".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), trade_id, evidence);

        assert!(TradeDisputeStore::<T>::contains_key(trade_id));
    }

    // ==================== call_index(23): resolve_dispute ====================
    #[benchmark]
    fn resolve_dispute() {
        setup_initial_price::<T>();
        let (_seller, buyer, trade_id) = seed_trade_refunded_disputable::<T>();

        // 确保国库有余额用于补偿
        let treasury = T::TreasuryAccount::get();
        let amount = BalanceOf::<T>::max_value() / 8u32.into();
        let _ = T::Currency::deposit_creating(&treasury, amount);

        // 创建争议
        let now = frame_system::Pallet::<T>::block_number();
        let evidence: BoundedVec<u8, sp_core::ConstU32<128>> =
            b"QmBenchEvidence".to_vec().try_into().unwrap();
        TradeDisputeStore::<T>::insert(
            trade_id,
            TradeDispute::<T> {
                trade_id,
                initiator: buyer.clone(),
                status: DisputeStatus::Open,
                created_at: now,
                evidence_cid: evidence,
                counter_evidence_cid: None,
                counter_party: None,
            },
        );

        #[extrinsic_call]
        _(RawOrigin::Root, trade_id, DisputeResolution::ReleaseToBuyer);
    }

    // ==================== call_index(24): set_trading_fee ====================
    #[benchmark]
    fn set_trading_fee() {
        #[extrinsic_call]
        _(RawOrigin::Root, 100); // 1%
    }

    // ==================== call_index(25): update_order_price ====================
    #[benchmark]
    fn update_order_price() {
        setup_initial_price::<T>();
        let (seller, order_id) = seed_sell_order::<T>(0);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(seller),
            order_id,
            STANDARD_PRICE + 100_000,
        );

        let order = Orders::<T>::get(order_id).unwrap();
        assert_eq!(order.usdt_price, STANDARD_PRICE + 100_000);
    }

    // ==================== call_index(26): update_deposit_exchange_rate [deprecated] ====================

    // ==================== call_index(27): seller_confirm_received ====================
    #[benchmark]
    fn seller_confirm_received() {
        setup_initial_price::<T>();
        let (seller, _buyer, trade_id, _) = seed_trade_awaiting_verification::<T>();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), trade_id);

        let trade = UsdtTrades::<T>::get(trade_id).unwrap();
        assert_eq!(trade.status, UsdtTradeStatus::Completed);
    }

    // ==================== call_index(28): ban_user ====================
    #[benchmark]
    fn ban_user() {
        let target = funded_account::<T>("target", 0);

        #[extrinsic_call]
        _(RawOrigin::Root, target.clone());

        assert!(BannedAccounts::<T>::get(&target));
    }

    // ==================== call_index(29): unban_user ====================
    #[benchmark]
    fn unban_user() {
        let target = funded_account::<T>("target", 0);
        BannedAccounts::<T>::insert(&target, true);

        #[extrinsic_call]
        _(RawOrigin::Root, target.clone());

        assert!(!BannedAccounts::<T>::get(&target));
    }

    // ==================== call_index(30): submit_counter_evidence ====================
    #[benchmark]
    fn submit_counter_evidence() {
        setup_initial_price::<T>();
        let (seller, buyer, trade_id) = seed_trade_refunded_disputable::<T>();

        let now = frame_system::Pallet::<T>::block_number();
        let evidence: BoundedVec<u8, sp_core::ConstU32<128>> =
            b"QmBuyerEvidence".to_vec().try_into().unwrap();
        TradeDisputeStore::<T>::insert(
            trade_id,
            TradeDispute::<T> {
                trade_id,
                initiator: buyer.clone(),
                status: DisputeStatus::Open,
                created_at: now,
                evidence_cid: evidence,
                counter_evidence_cid: None,
                counter_party: None,
            },
        );

        let counter_evidence = b"QmSellerCounterEvidence12345".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), trade_id, counter_evidence);
    }

    // ==================== call_index(31): update_order_amount ====================
    #[benchmark]
    fn update_order_amount() {
        setup_initial_price::<T>();
        let (seller, order_id) = seed_sell_order::<T>(0);
        let new_amount = standard_nex::<T>().saturating_mul(2u32.into());

        let extra = new_amount.saturating_sub(standard_nex::<T>());
        let _ = T::Currency::deposit_creating(&seller, extra);

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), order_id, new_amount);

        let order = Orders::<T>::get(order_id).unwrap();
        assert_eq!(order.nex_amount, new_amount);
    }

    // ==================== call_index(32): batch_force_settle ====================
    #[benchmark]
    fn batch_force_settle() {
        setup_initial_price::<T>();
        let mut trade_ids_vec = Vec::new();
        for i in 0..5u32 {
            let seller = funded_account::<T>("bseller", i);
            let buyer = funded_account::<T>("bbuyer", i);
            let trade_id = NextUsdtTradeId::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();
            let timeout: BlockNumberFor<T> = T::UsdtTimeout::get().into();
            let nex = standard_nex::<T>();
            let nex_u128: u128 = nex.saturated_into();
            let usdt_amount = (nex_u128 * STANDARD_PRICE as u128 / 1_000_000_000_000u128) as u64;

            let _ = T::Currency::reserve(&seller, nex);

            let seller_tron: TronAddress = valid_tron_address().try_into().unwrap();
            let buyer_tron: TronAddress = valid_tron_address_2().try_into().unwrap();

            let trade = UsdtTrade::<T> {
                trade_id,
                order_id: 0,
                seller: seller.clone(),
                buyer: buyer.clone(),
                nex_amount: nex,
                usdt_amount,
                seller_tron_address: seller_tron,
                buyer_tron_address: Some(buyer_tron),
                status: UsdtTradeStatus::AwaitingVerification,
                created_at: now,
                timeout_at: now.saturating_add(timeout),
                buyer_deposit: Zero::zero(),
                deposit_status: BuyerDepositStatus::None,
                first_verified_at: None,
                first_actual_amount: None,
                underpaid_deadline: None,
                completed_at: None,
                payment_confirmed: true,
                cumulative_penalty: Zero::zero(),
            };

            UsdtTrades::<T>::insert(trade_id, trade);
            trade_ids_vec.push(trade_id);
            NextUsdtTradeId::<T>::put(trade_id.saturating_add(1));
        }

        let trade_ids: BoundedVec<u64, sp_core::ConstU32<20>> = trade_ids_vec.try_into().unwrap();

        #[extrinsic_call]
        _(
            RawOrigin::Root,
            trade_ids,
            STANDARD_PRICE as u64 * 5,
            DisputeResolution::ReleaseToBuyer,
        );
    }

    // ==================== call_index(33): batch_force_cancel ====================
    #[benchmark]
    fn batch_force_cancel() {
        setup_initial_price::<T>();
        let mut trade_ids_vec = Vec::new();
        for i in 0..5u32 {
            let seller = funded_account::<T>("cseller", i);
            let buyer = funded_account::<T>("cbuyer", i);
            let trade_id = NextUsdtTradeId::<T>::get();
            let now = frame_system::Pallet::<T>::block_number();
            let timeout: BlockNumberFor<T> = T::UsdtTimeout::get().into();
            let nex = standard_nex::<T>();
            let nex_u128: u128 = nex.saturated_into();
            let usdt_amount = (nex_u128 * STANDARD_PRICE as u128 / 1_000_000_000_000u128) as u64;

            let _ = T::Currency::reserve(&seller, nex);

            let seller_tron: TronAddress = valid_tron_address().try_into().unwrap();
            let buyer_tron: TronAddress = valid_tron_address_2().try_into().unwrap();

            let trade = UsdtTrade::<T> {
                trade_id,
                order_id: 0,
                seller: seller.clone(),
                buyer: buyer.clone(),
                nex_amount: nex,
                usdt_amount,
                seller_tron_address: seller_tron,
                buyer_tron_address: Some(buyer_tron),
                status: UsdtTradeStatus::AwaitingPayment,
                created_at: now,
                timeout_at: now.saturating_add(timeout),
                buyer_deposit: Zero::zero(),
                deposit_status: BuyerDepositStatus::None,
                first_verified_at: None,
                first_actual_amount: None,
                underpaid_deadline: None,
                completed_at: None,
                payment_confirmed: false,
                cumulative_penalty: Zero::zero(),
            };

            UsdtTrades::<T>::insert(trade_id, trade);
            trade_ids_vec.push(trade_id);
            NextUsdtTradeId::<T>::put(trade_id.saturating_add(1));
        }

        let trade_ids: BoundedVec<u64, sp_core::ConstU32<20>> = trade_ids_vec.try_into().unwrap();

        #[extrinsic_call]
        _(RawOrigin::Root, trade_ids);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
