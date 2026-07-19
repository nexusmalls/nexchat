use crate::mock::*;
use crate::pallet::*;
use frame_support::{assert_noop, assert_ok, traits::Hooks, weights::Weight, BoundedVec};

// ==================== NEX 通道：挂单 ====================

#[test]
fn place_sell_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.maker, ALICE);
        assert_eq!(order.side, OrderSide::Sell);
        assert_eq!(order.token_amount, 1000u128);
        assert_eq!(order.price, 100u128);
        assert_eq!(order.status, OrderStatus::Open);
        // H1: Token 应已被锁定
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), 10_000_000 - 1000);
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 1000);
    });
}

#[test]
fn place_sell_order_fails_market_not_enabled() {
    ExtBuilder::build().execute_with(|| {
        // M6: 未配置市场 → MarketNotEnabled
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::MarketNotEnabled
        );
    });
}

#[test]
fn place_sell_order_fails_zero_price() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 0),
            Error::<Test>::ZeroPrice
        );
    });
}

#[test]
fn place_sell_order_fails_zero_amount() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 0, 100),
            Error::<Test>::AmountTooSmall
        );
    });
}

#[test]
fn place_sell_order_fails_insufficient_token() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // ALICE has 10_000_000, try to sell 20_000_000
        assert_noop!(
            EntityMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                20_000_000,
                100
            ),
            Error::<Test>::InsufficientTokenBalance
        );
    });
}

#[test]
fn place_buy_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // price=100, amount=1000 → total NEX = 100_000
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.maker, ALICE);
        assert_eq!(order.side, OrderSide::Buy);
        // NEX should be reserved
        assert_eq!(Balances::reserved_balance(ALICE), 100_000);
    });
}

#[test]
fn place_buy_order_fails_insufficient_nex() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // price=1_000_000_000, amount=1000 → overflow or insufficient
        assert_noop!(
            EntityMarket::place_buy_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                1000,
                1_000_000_000_000
            ),
            Error::<Test>::InsufficientBalance
        );
    });
}

// ==================== NEX 通道：吃单 ====================

#[test]
fn take_sell_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 1000 Token @ 100 NEX
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let bob_nex_before = Balances::free_balance(BOB);

        // BOB 吃单
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // H2: BOB 应该收到 Token
        assert_eq!(get_token_balance(ENTITY_ID, BOB), 10_000_000 + 1000);
        // ALICE 的 reserved Token 应减少
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);

        // BOB 支付 NEX: 全额转给 maker = 100_000
        let total_paid = bob_nex_before - Balances::free_balance(BOB);
        assert_eq!(total_paid, 100_000);

        // 订单应该是 Filled
        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.status, OrderStatus::Filled);

        // M2: UserOrders 应已清除
        assert!(UserOrders::<Test>::get(ALICE).is_empty());
    });
}

#[test]
fn take_buy_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂买单: 1000 Token @ 100 NEX
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let bob_token_before = get_token_balance(ENTITY_ID, BOB);

        // BOB 吃买单 (卖 Token)
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // H3: ALICE 应该收到 Token
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), 10_000_000 + 1000);
        // BOB 的 Token 应减少
        assert_eq!(get_token_balance(ENTITY_ID, BOB), bob_token_before - 1000);
        // BOB 应收到 NEX (扣除手续费)
        // ALICE 的 reserved NEX 应已释放
        assert_eq!(Balances::reserved_balance(ALICE), 0);

        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.status, OrderStatus::Filled);
    });
}

#[test]
fn take_order_partial_fill() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 2000 Token @ 100 NEX
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            2000,
            100
        ));

        // BOB 只吃 1000
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(1000)
        ));

        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.status, OrderStatus::PartiallyFilled);
        assert_eq!(order.filled_amount, 1000u128);

        // UserOrders 应还保留（未完全成交）
        assert!(!UserOrders::<Test>::get(ALICE).is_empty());
    });
}

#[test]
fn take_order_fails_own_order() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(ALICE), 0, None),
            Error::<Test>::CannotTakeOwnOrder
        );
    });
}

// ==================== NEX 通道：取消订单 ====================

#[test]
fn cancel_sell_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // H4: Token 已锁定
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 1000);

        assert_ok!(EntityMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));

        // H4: Token 应退还
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), 10_000_000);

        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.status, OrderStatus::Cancelled);

        // M2: UserOrders 应已清除
        assert!(UserOrders::<Test>::get(ALICE).is_empty());
    });
}

#[test]
fn cancel_buy_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_eq!(Balances::reserved_balance(ALICE), 100_000);

        assert_ok!(EntityMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));

        assert_eq!(Balances::reserved_balance(ALICE), 0);
        let order = Orders::<Test>::get(0).expect("order exists");
        assert_eq!(order.status, OrderStatus::Cancelled);
    });
}

#[test]
fn cancel_order_fails_not_owner() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_noop!(
            EntityMarket::cancel_order(RuntimeOrigin::signed(BOB), 0),
            Error::<Test>::NotOrderOwner
        );
    });
}

// ==================== 市场配置 ====================

#[test]
fn configure_market_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            10,
            500,
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).expect("config exists");
        assert!(config.nex_enabled);
    });
}

#[test]
fn configure_market_fails_not_owner() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::configure_market(RuntimeOrigin::signed(ALICE), ENTITY_ID, true, 1, 1000,),
            Error::<Test>::NotEntityOwner
        );
    });
}

// ==================== 价格保护配置 ====================

#[test]
fn configure_price_protection_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            2000,
            500,
            5000,
            100,
        ));
        let config = PriceProtection::<Test>::get(ENTITY_ID).expect("config exists");
        assert!(config.enabled);
        assert_eq!(config.max_price_deviation, 2000);
    });
}

#[test]
fn configure_price_protection_fails_invalid_bps() {
    ExtBuilder::build().execute_with(|| {
        // M4: bps > 10000 should fail
        assert_noop!(
            EntityMarket::configure_price_protection(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                10001,
                500,
                5000,
                100,
            ),
            Error::<Test>::InvalidBasisPoints
        );
        assert_noop!(
            EntityMarket::configure_price_protection(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                2000,
                10001,
                5000,
                100,
            ),
            Error::<Test>::InvalidBasisPoints
        );
        assert_noop!(
            EntityMarket::configure_price_protection(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                2000,
                500,
                10001,
                100,
            ),
            Error::<Test>::InvalidBasisPoints
        );
    });
}

// ==================== 熔断 ====================

#[test]
fn lift_circuit_breaker_fails_not_active() {
    ExtBuilder::build().execute_with(|| {
        // M3: 熔断未激活时调用应失败（审计修复 H3: 使用正确的错误类型）
        assert_noop!(
            EntityMarket::lift_circuit_breaker(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::CircuitBreakerNotActive
        );
    });
}

// ==================== 初始价格 ====================

#[test]
fn set_initial_price_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            500
        ));
        let config = PriceProtection::<Test>::get(ENTITY_ID).expect("config exists");
        assert_eq!(config.initial_price, Some(500u128));
    });
}

// ==================== 市价单 ====================

#[test]
fn market_buy_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 2000 Token @ 100 NEX
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            2000,
            100
        ));

        // BOB 市价买 1000 Token, max_cost = 200_000
        assert_ok!(EntityMarket::market_buy(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            200_000
        ));

        // H5: BOB 应收到 Token
        assert_eq!(get_token_balance(ENTITY_ID, BOB), 10_000_000 + 1000);
    });
}

#[test]
fn market_buy_fails_no_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(BOB), ENTITY_ID, 1000, 200_000),
            Error::<Test>::NoOrdersAvailable
        );
    });
}

#[test]
fn market_sell_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂买单: 2000 Token @ 100 NEX
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            2000,
            100
        ));

        let bob_nex_before = Balances::free_balance(BOB);

        // BOB 市价卖 1000 Token, min_receive = 50_000
        assert_ok!(EntityMarket::market_sell(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            50_000
        ));

        // H5: BOB Token 应减少
        assert_eq!(get_token_balance(ENTITY_ID, BOB), 10_000_000 - 1000);
        // BOB 应收到 NEX
        assert!(Balances::free_balance(BOB) > bob_nex_before);
    });
}

// ==================== M6: 未配置市场默认不启用 ====================

#[test]
fn market_not_enabled_by_default() {
    ExtBuilder::build().execute_with(|| {
        // 不调用 configure_market → 默认 nex_enabled=false
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::MarketNotEnabled
        );
    });
}

// ==================== 查询接口 ====================

#[test]
fn get_order_book_depth_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            80
        ));

        let depth = EntityMarket::get_order_book_depth(ENTITY_ID, 10);
        assert!(!depth.asks.is_empty());
        assert!(!depth.bids.is_empty());
        assert_eq!(depth.best_ask, Some(100u128));
        assert_eq!(depth.best_bid, Some(80u128));
    });
}

#[test]
fn get_market_summary_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let summary = EntityMarket::get_market_summary(ENTITY_ID);
        assert_eq!(summary.best_ask, Some(100u128));
        assert_eq!(summary.total_ask_amount, 1000u128);
    });
}

// ==================== TWAP 和统计 ====================

#[test]
fn trade_updates_stats_and_twap() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        let stats = MarketStatsStorage::<Test>::get(ENTITY_ID);
        assert_eq!(stats.total_trades, 1);
        assert!(stats.total_volume_nex > 0);

        // TWAP 累积器应该更新
        let twap = TwapAccumulators::<Test>::get(ENTITY_ID);
        assert!(twap.is_some());
    });
}

// ==================== 审计回归测试 (H1-H6) ====================

#[test]
fn h3_market_sell_slippage_enforced_on_partial_fill() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂买单: 500 Token @ 100 NEX (仅能提供 50000 NEX)
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100
        ));

        // ALICE 想卖 1000 Token, 要求最少收到 80000 NEX
        // 只有 500 Token 能成交, 收到 50000 NEX
        // 50000 < 80000 → 应失败
        assert_noop!(
            EntityMarket::market_sell(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 80000),
            Error::<Test>::SlippageExceeded
        );
    });
}

#[test]
fn h4_place_order_fails_entity_not_active() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 标记实体为非活跃 (模拟 Banned/Closed)
        set_entity_active(ENTITY_ID, false);

        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::EntityNotActive
        );
        assert_noop!(
            EntityMarket::place_buy_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn h6_configure_market_rejects_short_ttl() {
    ExtBuilder::build().execute_with(|| {
        // order_ttl = 5 < 10 → 应失败
        assert_noop!(
            EntityMarket::configure_market(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                1,
                5,
            ),
            Error::<Test>::OrderTtlTooShort
        );
    });
}

// ==================== EntityTokenPriceProvider 测试 ====================

#[test]
fn token_price_returns_none_when_no_data() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        assert_eq!(EntityMarket::get_token_price(ENTITY_ID), None);
        assert_eq!(EntityMarket::get_token_price_usdt(ENTITY_ID), None);
        assert_eq!(EntityMarket::token_price_confidence(ENTITY_ID), 0);
        assert!(EntityMarket::is_token_price_stale(ENTITY_ID, 100));
        assert!(!EntityMarket::is_token_price_reliable(ENTITY_ID));
    });
}

#[test]
fn token_price_returns_initial_price() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // 设置初始价格: 100 NEX per Token
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));

        // H1: initial_price 不再写入 LastTradePrice，价格通过 TWAP 累积器 fallback 获取
        assert_eq!(EntityMarket::get_token_price(ENTITY_ID), Some(100u128));
        // LastTradePrice 未被写入
        assert_eq!(LastTradePrice::<Test>::get(ENTITY_ID), None);
        // 置信度 = 80 (TWAP 累积器存在，calculate_twap 在 block_diff=0 时返回 last_price)
        let confidence = EntityMarket::token_price_confidence(ENTITY_ID);
        assert_eq!(confidence, 80);
        assert!(EntityMarket::is_token_price_reliable(ENTITY_ID));
    });
}

#[test]
fn token_price_usdt_indirect_conversion() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 价格 50_000_000_000 = 0.05 NEX/Token (精度 10^12)
        // BOB balance = 100B, cost = 50B → affordable
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1,
            50_000_000_000u128
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 间接换算: 0.05 NEX/Token × 1 USDT/NEX = 0.05 USDT (精度 10^6 → 50_000)
        let usdt_price = EntityMarket::get_token_price_usdt(ENTITY_ID);
        assert_eq!(usdt_price, Some(50_000));
    });
}

#[test]
fn token_price_usdt_sub_precision_returns_zero() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 极小价格: 100 raw units → 100 * 1_000_000 / 10^12 = 0
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        let usdt_price = EntityMarket::get_token_price_usdt(ENTITY_ID);
        assert_eq!(usdt_price, Some(0));
    });
}

#[test]
fn token_price_staleness_detection() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 当前区块 = 1, TWAP 累积器 current_block = 1
        // max_age = 100 → 不过时
        assert!(!EntityMarket::is_token_price_stale(ENTITY_ID, 100));

        // 推进区块到 200
        System::set_block_number(200);
        // max_age = 100 → 200 - 1 = 199 > 100 → 过时
        assert!(EntityMarket::is_token_price_stale(ENTITY_ID, 100));
        // max_age = 300 → 199 < 300 → 不过时
        assert!(!EntityMarket::is_token_price_stale(ENTITY_ID, 300));
    });
}

#[test]
fn token_price_confidence_levels() {
    use pallet_entity_common::EntityTokenPriceProvider;
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 仅 initial_price: TWAP 累积器已初始化，calculate_twap 在 block_diff=0 时返回 last_price
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        let c1 = EntityMarket::token_price_confidence(ENTITY_ID);
        // H1: 不再设置 LastTradePrice, 但 has_twap=true (block_diff=0 fallback) → 80
        assert_eq!(c1, 80);

        // 有交易后: 置信度应更高
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
        let c2 = EntityMarket::token_price_confidence(ENTITY_ID);
        assert!(c2 >= c1);
    });
}

// ==================== Round 3 审计回归测试 ====================

/// C1: deviation_bps u16 截断不应绕过价格保护
#[test]
fn c1_extreme_deviation_not_bypassed_by_u16_wrap() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 启用价格保护，设置初始参考价格和较小的 max_deviation
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true, // enabled
            1000, // max_deviation = 10%
            500,  // max_slippage
            5000, // circuit_breaker_threshold
            0,    // min_trades_for_twap = 0 → 使用 initial_price
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));

        // 正常偏离 5% (price=105) → 应通过
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            105
        ));

        // 极端偏离 price = 100_000 (99900% 偏离) → 修复前 as u16 会 wrap
        // 99900 * 10000 / 100 = 9_990_000 bps → 超过 u16::MAX(65535)
        // 修复前: wrap 到 (9_990_000 % 65536) = 某个小值 → 绕过
        // 修复后: .min(u16::MAX) = 65535 > 1000 → PriceDeviationTooHigh
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 100, 100_000),
            Error::<Test>::PriceDeviationTooHigh
        );
    });
}

/// H2: 过期订单不可吃单
#[test]
fn h2_take_order_rejects_expired_order() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: TTL = 1000
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        let order = Orders::<Test>::get(0).unwrap();

        // 推进到过期之后（但 on_idle 未运行，订单仍为 Open）
        System::set_block_number(order.expires_at + 1);

        // BOB 尝试吃单 → 应失败
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, None),
            Error::<Test>::OrderClosed
        );
    });
}

/// H3: 过期订单不出现在 best prices 和 market orders
#[test]
fn h3_expired_orders_excluded_from_best_prices() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // BOB 挂买单
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            80
        ));

        // 验证 best prices 有效
        let (ask, bid) = EntityMarket::get_best_prices(ENTITY_ID);
        assert_eq!(ask, Some(100u128));
        assert_eq!(bid, Some(80u128));

        // 推进到订单过期后（但不运行 on_idle）
        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // best prices 应为 None（过期单被过滤）
        let (ask2, bid2) = EntityMarket::get_best_prices(ENTITY_ID);
        assert_eq!(ask2, None);
        assert_eq!(bid2, None);

        // 市价买单应失败（无有效卖单）
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(CHARLIE), ENTITY_ID, 100, 200_000),
            Error::<Test>::NoOrdersAvailable
        );
    });
}

// ==================== EntityLocked 回归测试 ====================

#[test]
fn entity_locked_rejects_configure_market() {
    ExtBuilder::build().execute_with(|| {
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            EntityMarket::configure_market(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                1,
                1000,
            ),
            Error::<Test>::EntityLocked
        );
    });
}

// ==================== 新 extrinsics 测试 ====================

// --- force_cancel_order ---

#[test]
fn force_cancel_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // Root 强制取消
        assert_ok!(EntityMarket::force_cancel_order(RuntimeOrigin::root(), 0));

        let order = Orders::<Test>::get(0).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
        // Token 退还
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
    });
}

#[test]
fn force_cancel_order_fails_not_root() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        assert_noop!(
            EntityMarket::force_cancel_order(RuntimeOrigin::signed(ALICE), 0),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// --- pause_market / resume_market ---

#[test]
fn pause_and_resume_market_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 暂停
        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.paused);

        // 暂停后不能下单
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::MarketPaused
        );

        // 恢复
        assert_ok!(EntityMarket::resume_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.paused);

        // 恢复后可以下单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
    });
}

#[test]
fn pause_market_fails_not_owner() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::pause_market(RuntimeOrigin::signed(ALICE), ENTITY_ID),
            Error::<Test>::NotEntityOwner
        );
    });
}

// --- global_market_pause ---

#[test]
fn global_market_pause_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // Root 全局暂停
        assert_ok!(EntityMarket::global_market_pause(
            RuntimeOrigin::root(),
            true
        ));
        assert!(GlobalMarketPaused::<Test>::get());

        // 全局暂停后不能下单
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::GlobalMarketPausedError
        );

        // 解除全局暂停
        assert_ok!(EntityMarket::global_market_pause(
            RuntimeOrigin::root(),
            false
        ));
        assert!(!GlobalMarketPaused::<Test>::get());

        // 可以下单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
    });
}

#[test]
fn global_market_pause_fails_not_root() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::global_market_pause(RuntimeOrigin::signed(ALICE), true),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// --- batch_cancel_orders ---

#[test]
fn batch_cancel_orders_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            200
        ));

        assert_ok!(EntityMarket::batch_cancel_orders(
            RuntimeOrigin::signed(ALICE),
            BoundedVec::try_from(vec![0, 1]).unwrap()
        ));

        assert_eq!(
            Orders::<Test>::get(0).unwrap().status,
            OrderStatus::Cancelled
        );
        assert_eq!(
            Orders::<Test>::get(1).unwrap().status,
            OrderStatus::Cancelled
        );
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
    });
}

// --- cleanup_expired_orders ---

#[test]
fn cleanup_expired_orders_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let order = Orders::<Test>::get(0).unwrap();
        // 推进到过期后
        System::set_block_number(order.expires_at + 1);

        assert_ok!(EntityMarket::cleanup_expired_orders(
            RuntimeOrigin::signed(CHARLIE),
            ENTITY_ID,
            10
        ));

        let order = Orders::<Test>::get(0).unwrap();
        assert_eq!(order.status, OrderStatus::Expired);

        // Token 应已退还给 ALICE
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
    });
}

// --- modify_order ---

#[test]
fn modify_order_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            2000,
            100
        ));

        // 修改价格和减少数量
        assert_ok!(EntityMarket::modify_order(
            RuntimeOrigin::signed(ALICE),
            0,
            150,
            1000
        ));

        let order = Orders::<Test>::get(0).unwrap();
        assert_eq!(order.token_amount, 1000u128);
        assert_eq!(order.price, 150u128);
    });
}

#[test]
fn modify_order_fails_increase_amount() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        assert_noop!(
            EntityMarket::modify_order(RuntimeOrigin::signed(ALICE), 0, 100, 2000),
            Error::<Test>::ModifyAmountExceedsOriginal
        );
    });
}

#[test]
fn modify_order_fails_not_owner() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        assert_noop!(
            EntityMarket::modify_order(RuntimeOrigin::signed(BOB), 0, 100, 500),
            Error::<Test>::NotOrderOwner
        );
    });
}

// ==================== 审计回归测试 (Round 2) ====================

/// H1: take_order 在市场暂停时应失败
#[test]
fn h1_take_order_rejects_paused_market() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 暂停市场
        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // BOB 尝试吃单 → 应失败
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, None),
            Error::<Test>::MarketPaused
        );

        // 恢复后可吃单
        assert_ok!(EntityMarket::resume_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
    });
}

/// H1: take_order 在实体非活跃时应失败
#[test]
fn h1_take_order_rejects_inactive_entity() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 标记实体为非活跃
        set_entity_active(ENTITY_ID, false);

        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, None),
            Error::<Test>::EntityNotActive
        );
    });
}

/// H3: lift_circuit_breaker 未激活时返回 CircuitBreakerNotActive
#[test]
fn h3_lift_circuit_breaker_correct_error_when_not_active() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::lift_circuit_breaker(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::CircuitBreakerNotActive
        );
    });
}

/// 无手续费交易验证
#[test]
fn no_fee_trade_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let bob_before = Balances::free_balance(BOB);
        let alice_before = Balances::free_balance(ALICE);

        // BOB 吃单
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 无手续费: BOB 支付 100_000, ALICE 收到 100_000（无扣除）
        let bob_paid = bob_before - Balances::free_balance(BOB);
        let alice_received = Balances::free_balance(ALICE) - alice_before;
        assert_eq!(bob_paid, 100_000);
        assert_eq!(alice_received, 100_000);
    });
}

/// M2: force_cancel_order 后 best_prices 应更新
#[test]
fn m2_force_cancel_updates_best_prices() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        // Root 强制取消
        assert_ok!(EntityMarket::force_cancel_order(RuntimeOrigin::root(), 0));

        // best_ask 应清除
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), None);
    });
}

/// M2: batch_cancel_orders 后 best_prices 应更新
#[test]
fn m2_batch_cancel_updates_best_prices() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            200
        ));
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        // 取消最优卖单
        assert_ok!(EntityMarket::batch_cancel_orders(
            RuntimeOrigin::signed(ALICE),
            BoundedVec::try_from(vec![0]).unwrap()
        ));

        // best_ask 应更新为次优 200
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(200u128));
    });
}

/// M2: cleanup_expired_orders 后 best_prices 应更新
#[test]
fn m2_cleanup_expired_updates_best_prices() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        assert_ok!(EntityMarket::cleanup_expired_orders(
            RuntimeOrigin::signed(CHARLIE),
            ENTITY_ID,
            10
        ));

        // best_ask 应清除
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), None);
    });
}

// --- nex_enabled 配置测试 ---

#[test]
fn market_paused_field_persists() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            1,
            1000,
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.paused); // 默认 false
        assert!(config.nex_enabled);
    });
}

// ==================== P1: 交易历史测试 ====================

#[test]
fn p1_trade_history_recorded_on_take_order() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Alice 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // Bob 吃单
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        // 检查成交记录
        let record = TradeRecords::<Test>::get(0).expect("trade record exists");
        assert_eq!(record.order_id, 0);
        assert_eq!(record.entity_id, ENTITY_ID);
        assert_eq!(record.maker, ALICE);
        assert_eq!(record.taker, BOB);
        assert_eq!(record.token_amount, 500);
        assert_eq!(record.price, 100);
        // 检查用户交易历史索引
        let alice_history = UserTradeHistory::<Test>::get(ALICE);
        assert!(alice_history.contains(&0));
        let bob_history = UserTradeHistory::<Test>::get(BOB);
        assert!(bob_history.contains(&0));
        // 检查实体交易历史索引
        let entity_history = EntityTradeHistory::<Test>::get(ENTITY_ID);
        assert!(entity_history.contains(&0));
    });
}

#[test]
fn p1_trade_history_recorded_on_cross_match() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Alice 挂卖单 price=100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // Bob 挂买单 price=100 (会自动撮合)
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100
        ));
        // 检查成交记录存在
        assert!(TradeRecords::<Test>::get(0).is_some());
    });
}

#[test]
fn p1_trade_history_recorded_on_market_buy() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::market_buy(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100_000
        ));
        assert!(TradeRecords::<Test>::get(0).is_some());
    });
}

// ==================== P2: 订单历史测试 ====================

#[test]
fn p2_order_history_on_fill() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        // Alice's order should be in history (fully filled)
        let alice_history = UserOrderHistory::<Test>::get(ALICE);
        assert!(alice_history.contains(&0));
    });
}

#[test]
fn p2_order_history_on_cancel() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::cancel_order(RuntimeOrigin::signed(ALICE), 0));
        let alice_history = UserOrderHistory::<Test>::get(ALICE);
        assert!(alice_history.contains(&0));
    });
}

// ==================== P3: 日统计测试 ====================

#[test]
fn p3_daily_stats_updated_on_trade() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        let stats = EntityDailyStats::<Test>::get(ENTITY_ID);
        assert_eq!(stats.open_price, 100);
        assert_eq!(stats.high_price, 100);
        assert_eq!(stats.low_price, 100);
        assert_eq!(stats.close_price, 100);
        assert!(stats.volume_nex > 0);
        assert_eq!(stats.trade_count, 1);
    });
}

#[test]
fn p3_daily_stats_tracks_ohlc() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Trade at price 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        // Trade at price 200
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            200
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            1,
            Some(500)
        ));
        // Trade at price 50
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            50
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            2,
            Some(500)
        ));

        let stats = EntityDailyStats::<Test>::get(ENTITY_ID);
        assert_eq!(stats.open_price, 100);
        assert_eq!(stats.high_price, 200);
        assert_eq!(stats.low_price, 50);
        assert_eq!(stats.close_price, 50);
        assert_eq!(stats.trade_count, 3);
    });
}

// ==================== P4: KYC 门槛测试 ====================

#[test]
fn p4_set_kyc_requirement_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::set_kyc_requirement(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            2
        ));
        assert_eq!(MarketKycRequirement::<Test>::get(ENTITY_ID), 2);
    });
}

#[test]
fn p4_set_kyc_requirement_rejects_non_owner() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::set_kyc_requirement(RuntimeOrigin::signed(ALICE), ENTITY_ID, 2),
            Error::<Test>::NotEntityOwner
        );
    });
}

#[test]
fn p4_kyc_blocks_insufficient_level() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Set KYC requirement to level 2
        assert_ok!(EntityMarket::set_kyc_requirement(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            2
        ));
        // Alice has no KYC (level 0)
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::InsufficientKycLevel
        );
    });
}

#[test]
fn p4_kyc_allows_sufficient_level() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::set_kyc_requirement(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            2
        ));
        // Give Alice KYC level 3
        set_kyc_level(ENTITY_ID, ALICE, 3);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
    });
}

#[test]
fn p4_kyc_blocks_take_order() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Alice places order before KYC requirement
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // Set KYC requirement
        assert_ok!(EntityMarket::set_kyc_requirement(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            1
        ));
        // Bob has no KYC, can't take
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, Some(500)),
            Error::<Test>::InsufficientKycLevel
        );
    });
}

// ==================== P6: 关闭市场测试 ====================

#[test]
fn p6_close_market_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Place some orders
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            50
        ));
        // Close market
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_eq!(
            MarketStatusStorage::<Test>::get(ENTITY_ID),
            MarketStatus::Closed
        );
        // All orders should be cancelled
        assert_eq!(EntitySellOrders::<Test>::get(ENTITY_ID).len(), 0);
        assert_eq!(EntityBuyOrders::<Test>::get(ENTITY_ID).len(), 0);
    });
}

#[test]
fn p6_close_market_rejects_non_owner() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::close_market(RuntimeOrigin::signed(ALICE), ENTITY_ID),
            Error::<Test>::NotEntityOwner
        );
    });
}

#[test]
fn p6_closed_market_blocks_new_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::MarketAlreadyClosed
        );
    });
}

#[test]
fn p6_close_market_cannot_close_twice() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_noop!(
            EntityMarket::close_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::MarketAlreadyClosed
        );
    });
}

// ==================== P7: 取消全部订单测试 ====================

#[test]
fn p7_cancel_all_entity_orders_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Alice places multiple orders
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            200
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            300,
            10
        ));
        assert_ok!(EntityMarket::cancel_all_entity_orders(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID
        ));
        // All orders cancelled
        let user_orders = UserOrders::<Test>::get(ALICE);
        let entity_orders: Vec<_> = user_orders
            .iter()
            .filter_map(|&id| Orders::<Test>::get(id))
            .filter(|o| o.entity_id == ENTITY_ID)
            .collect();
        assert_eq!(entity_orders.len(), 0);
        // Order history should have entries
        let history = UserOrderHistory::<Test>::get(ALICE);
        assert_eq!(history.len(), 3);
    });
}

// ==================== P8: 治理配置测试 ====================

#[test]
fn p8_governance_configure_market_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::governance_configure_market(
            RuntimeOrigin::root(),
            ENTITY_ID,
            true,
            10,
            5000,
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(config.nex_enabled);
        assert_eq!(config.min_order_amount, 10);
        assert_eq!(config.order_ttl, 5000);
    });
}

#[test]
fn p8_governance_configure_rejects_signed() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::governance_configure_market(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                true,
                10,
                5000,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ==================== P10: Root 强制关闭测试 ====================

#[test]
fn p10_force_close_market_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::force_close_market(
            RuntimeOrigin::root(),
            ENTITY_ID
        ));
        assert_eq!(
            MarketStatusStorage::<Test>::get(ENTITY_ID),
            MarketStatus::Closed
        );
        assert_eq!(EntitySellOrders::<Test>::get(ENTITY_ID).len(), 0);
    });
}

#[test]
fn p10_force_close_rejects_signed() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_noop!(
            EntityMarket::force_close_market(RuntimeOrigin::signed(ALICE), ENTITY_ID),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ==================== P11: 全局统计测试 ====================

#[test]
fn p11_global_stats_updated_on_trade() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        let stats = GlobalStats::<Test>::get();
        assert!(stats.total_trades > 0);
        assert!(stats.total_volume_nex > 0);
    });
}

// ==================== P12: Maker in OrderFilled 测试 ====================

#[test]
fn p12_order_filled_event_includes_maker() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));
        // Check the OrderFilled event includes maker=ALICE
        let events = frame_system::Pallet::<Test>::events();
        let found = events.iter().any(|e| match &e.event {
            RuntimeEvent::EntityMarket(Event::OrderFilled { maker, taker, .. }) => {
                *maker == ALICE && *taker == BOB
            }
            _ => false,
        });
        assert!(found, "OrderFilled event with maker field not found");
    });
}

// ==================== P13: 分页查询测试 ====================

#[test]
fn p13_pagination_trade_history() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Create 3 trades
        for _ in 0..3 {
            assert_ok!(EntityMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                100,
                100
            ));
        }
        let order_count = NextOrderId::<Test>::get();
        for i in 0..order_count {
            let _ = EntityMarket::take_order(RuntimeOrigin::signed(BOB), i, Some(100));
        }
        // Page 0, size 2
        let page0 = EntityMarket::get_user_trade_history(&ALICE, 0, 2);
        assert_eq!(page0.len(), 2);
        // Page 1, size 2
        let page1 = EntityMarket::get_user_trade_history(&ALICE, 1, 2);
        assert_eq!(page1.len(), 1);
    });
}

#[test]
fn p13_pagination_order_history() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Create and cancel 3 orders
        for _ in 0..3 {
            assert_ok!(EntityMarket::place_sell_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                100,
                100
            ));
        }
        for i in 0..3u64 {
            assert_ok!(EntityMarket::cancel_order(RuntimeOrigin::signed(ALICE), i));
        }
        let page0 = EntityMarket::get_user_order_history(&ALICE, 0, 2);
        assert_eq!(page0.len(), 2);
        let page1 = EntityMarket::get_user_order_history(&ALICE, 1, 2);
        assert_eq!(page1.len(), 1);
    });
}

// ==================== P15: IOC/FOK/PostOnly 测试 ====================

#[test]
fn p15_ioc_order_partial_fill_and_cancel_rest() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Sell side: Alice has 300 tokens at price 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            300,
            100
        ));
        // Bob places IOC buy for 500 at price 100 → only 300 filled, rest cancelled
        assert_ok!(EntityMarket::place_ioc_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            500,
            100
        ));
        // Bob should not have any open orders
        let bob_orders: Vec<_> = UserOrders::<Test>::get(BOB)
            .iter()
            .filter_map(|&id| Orders::<Test>::get(id))
            .filter(|o| o.status == OrderStatus::Open)
            .collect();
        assert_eq!(bob_orders.len(), 0);
    });
}

#[test]
fn p15_fok_order_fails_when_not_fillable() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // Only 300 tokens available
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            300,
            100
        ));
        // FOK for 500 → should fail
        assert_noop!(
            EntityMarket::place_fok_order(
                RuntimeOrigin::signed(BOB),
                ENTITY_ID,
                OrderSide::Buy,
                500,
                100
            ),
            Error::<Test>::FokNotFullyFillable
        );
    });
}

#[test]
fn p15_fok_order_succeeds_when_fillable() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // FOK for 500 → should succeed
        assert_ok!(EntityMarket::place_fok_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            500,
            100
        ));
    });
}

#[test]
fn p15_post_only_order_rejects_when_would_match() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        // Post-only buy at 100 would match the sell at 100
        assert_noop!(
            EntityMarket::place_post_only_order(
                RuntimeOrigin::signed(BOB),
                ENTITY_ID,
                OrderSide::Buy,
                500,
                100
            ),
            Error::<Test>::PostOnlyWouldMatch
        );
    });
}

#[test]
fn p15_post_only_order_succeeds_when_no_match() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            200
        ));
        // Post-only buy at 99 won't match the sell at 200
        assert_ok!(EntityMarket::place_post_only_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            500,
            99
        ));
        // Order should be on the book
        let order = Orders::<Test>::get(1).expect("post-only order exists");
        assert_eq!(order.order_type, OrderType::PostOnly);
        assert_eq!(order.status, OrderStatus::Open);
    });
}

// ==================== P6: 关闭市场退还资产测试 ====================

#[test]
fn p6_close_market_refunds_assets() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        let alice_tokens_before = get_token_balance(ENTITY_ID, ALICE);
        let bob_balance_before = Balances::free_balance(BOB);

        // Alice sells, Bob buys
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            300,
            50
        ));

        // Close market
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // Alice's tokens should be restored
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), alice_tokens_before);
        // Bob's balance should be restored
        assert_eq!(Balances::free_balance(BOB), bob_balance_before);
    });
}

// ==================== P9: configure_market closed market 测试 ====================

#[test]
fn p9_configure_market_rejects_closed_market() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_noop!(
            EntityMarket::configure_market(
                RuntimeOrigin::signed(ENTITY_OWNER),
                ENTITY_ID,
                true,
                1,
                1000,
            ),
            Error::<Test>::MarketAlreadyClosed
        );
    });
}

// ==================== H1: set_initial_price 一次性限制回归测试 ====================

/// H1: set_initial_price 不写 LastTradePrice
#[test]
fn h1_set_initial_price_does_not_write_last_trade_price() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            500
        ));
        // LastTradePrice 应为 None — 初始价格仅存入 PriceProtection
        assert_eq!(LastTradePrice::<Test>::get(ENTITY_ID), None);
        // initial_price 应正确存储
        let config = PriceProtection::<Test>::get(ENTITY_ID).expect("config exists");
        assert_eq!(config.initial_price, Some(500u128));
    });
}

/// H1: 无真实成交时可重新设置初始价格
#[test]
fn h1_set_initial_price_can_update_before_trades() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            500
        ));
        // 再次设置不同价格 — 无真实成交，应成功
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            800
        ));
        let config = PriceProtection::<Test>::get(ENTITY_ID).expect("config exists");
        assert_eq!(config.initial_price, Some(800u128));
    });
}

/// H1: 有真实成交后禁止设置初始价格
#[test]
fn h1_set_initial_price_rejected_after_real_trade() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // 先设初始价格
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        // 产生真实成交
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
        // 尝试重设初始价格 → 应失败
        assert_noop!(
            EntityMarket::set_initial_price(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID, 200),
            Error::<Test>::InitialPriceAlreadySet
        );
        // 原始初始价格不受影响
        let config = PriceProtection::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.initial_price, Some(100u128));
    });
}

/// H1: 从未设初始价格、直接成交后再设 → 应失败
#[test]
fn h1_set_initial_price_rejected_when_never_set_but_traded() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // 无初始价格，直接成交（update_twap_accumulator 自动创建累积器）
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
        // trade_count >= 1 → 禁止设初始价格
        assert_noop!(
            EntityMarket::set_initial_price(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID, 100),
            Error::<Test>::InitialPriceAlreadySet
        );
    });
}

/// L1: re-set 时 TWAP 累积器 last_price 和 current_block 正确更新
#[test]
fn l1_re_set_initial_price_updates_twap_accumulator() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            500
        ));
        let acc1 = TwapAccumulators::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(acc1.last_price, 500u128);
        assert_eq!(acc1.trade_count, 0);
        let block1 = acc1.current_block;

        // 推进区块后重设
        System::set_block_number(10);
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            800
        ));
        let acc2 = TwapAccumulators::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(acc2.last_price, 800u128);
        assert_eq!(acc2.current_block, 10);
        // 旧价格的累积量应已写入: 500 * (10 - block1) 区块
        let expected_cumulative = 500u128 * (10u32.saturating_sub(block1) as u128);
        assert_eq!(acc2.current_cumulative, expected_cumulative);
    });
}

// ==================== Round 5 审计回归测试 ====================

/// H1-R5: FOK 订单应受价格偏离保护限制
#[test]
fn h1_r5_fok_order_respects_price_deviation() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 启用价格保护，设置小偏离阈值
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            1000,
            500,
            5000,
            0,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));

        // 正常价格挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            105
        ));

        // FOK 买单价格极端偏离 → 应被价格偏离检查拒绝
        assert_noop!(
            EntityMarket::place_fok_order(
                RuntimeOrigin::signed(BOB),
                ENTITY_ID,
                OrderSide::Buy,
                100,
                100_000
            ),
            Error::<Test>::PriceDeviationTooHigh
        );
    });
}

// ==================== 价格锚点强制检查测试 ====================

/// 无价格锚点时所有交易入口应被拒绝
#[test]
fn trading_blocked_without_price_anchor() {
    ExtBuilder::build().execute_with(|| {
        // 仅开市场，不设 initial_price
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            1,
            1000,
        ));

        // 限价卖单 → InitialPriceNotSet
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::InitialPriceNotSet
        );

        // 限价买单 → InitialPriceNotSet
        assert_noop!(
            EntityMarket::place_buy_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100),
            Error::<Test>::InitialPriceNotSet
        );

        // 市价买单 → InitialPriceNotSet
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 999999),
            Error::<Test>::InitialPriceNotSet
        );

        // 市价卖单 → InitialPriceNotSet
        assert_noop!(
            EntityMarket::market_sell(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 0),
            Error::<Test>::InitialPriceNotSet
        );

        // IOC → InitialPriceNotSet
        assert_noop!(
            EntityMarket::place_ioc_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Buy,
                1000,
                100
            ),
            Error::<Test>::InitialPriceNotSet
        );

        // FOK → InitialPriceNotSet
        assert_noop!(
            EntityMarket::place_fok_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Sell,
                1000,
                100
            ),
            Error::<Test>::InitialPriceNotSet
        );

        // PostOnly → InitialPriceNotSet
        assert_noop!(
            EntityMarket::place_post_only_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Sell,
                1000,
                100
            ),
            Error::<Test>::InitialPriceNotSet
        );

        // 设置价格锚点后应能交易
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100
        ));
    });
}

/// 有真实成交后即使无 initial_price 也满足价格锚点（LastTradePrice 优先）
#[test]
fn last_trade_price_satisfies_price_anchor() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 创造一笔成交
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 清除 PriceProtection（模拟无 initial_price）
        PriceProtection::<Test>::remove(ENTITY_ID);

        // 有 LastTradePrice → 仍可交易
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
    });
}

/// M1-R5: batch_cancel_orders 后订单应出现在用户历史中
#[test]
fn m1_r5_batch_cancel_adds_to_order_history() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            200
        ));

        assert_ok!(EntityMarket::batch_cancel_orders(
            RuntimeOrigin::signed(ALICE),
            BoundedVec::try_from(vec![0, 1]).unwrap()
        ));

        let history = UserOrderHistory::<Test>::get(ALICE);
        assert!(
            history.contains(&0),
            "batch cancelled order 0 should be in history"
        );
        assert!(
            history.contains(&1),
            "batch cancelled order 1 should be in history"
        );
    });
}

/// M2-R5: force_cancel_order 后订单应出现在用户历史中
#[test]
fn m2_r5_force_cancel_adds_to_order_history() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        assert_ok!(EntityMarket::force_cancel_order(RuntimeOrigin::root(), 0));

        let history = UserOrderHistory::<Test>::get(ALICE);
        assert!(
            history.contains(&0),
            "force cancelled order should be in history"
        );
    });
}

/// M3-R5: cleanup_expired_orders 后订单应出现在用户历史中
#[test]
fn m3_r5_cleanup_expired_adds_to_order_history() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        assert_ok!(EntityMarket::cleanup_expired_orders(
            RuntimeOrigin::signed(CHARLIE),
            ENTITY_ID,
            10
        ));

        let history = UserOrderHistory::<Test>::get(ALICE);
        assert!(
            history.contains(&0),
            "expired order should be in history after cleanup"
        );
    });
}

/// M5-R5: get_market_summary 应排除过期订单的数量
#[test]
fn m5_r5_market_summary_excludes_expired_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 未过期时 total_ask_amount = 1000
        let summary = EntityMarket::get_market_summary(ENTITY_ID);
        assert_eq!(summary.total_ask_amount, 1000u128);

        // 推进到订单过期后（不运行 cleanup）
        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 过期后 total_ask_amount 应为 0
        let summary2 = EntityMarket::get_market_summary(ENTITY_ID);
        assert_eq!(summary2.total_ask_amount, 0u128);
    });
}

/// L1-R5: close_market 在实体锁定时应失败
#[test]
fn l1_r5_close_market_rejects_entity_locked() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        set_entity_locked(ENTITY_ID);

        assert_noop!(
            EntityMarket::close_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::EntityLocked
        );
    });
}

/// L2-R5: pause_market 在实体锁定时应失败
#[test]
fn l2_r5_pause_market_rejects_entity_locked() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        set_entity_locked(ENTITY_ID);

        assert_noop!(
            EntityMarket::pause_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::EntityLocked
        );
    });
}

/// L2-R5: resume_market 在实体锁定时应失败
#[test]
fn l2_r5_resume_market_rejects_entity_locked() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // 先暂停
        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        // 然后锁定实体
        set_entity_locked(ENTITY_ID);

        assert_noop!(
            EntityMarket::resume_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::EntityLocked
        );
    });
}

// ==================== Round 6 审计回归测试 ====================

/// H1-R6: FOK 买单在价格改善时应退还多余 NEX
#[test]
fn h1_r6_fok_buy_refunds_excess_nex_on_price_improvement() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 1000 Token @ 80 NEX (低于买方限价)
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            80
        ));

        let bob_before = Balances::free_balance(BOB);

        // BOB 发 FOK 买单: 1000 Token @ 100 NEX (限价 100, 实际成交 80)
        assert_ok!(EntityMarket::place_fok_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            1000,
            100
        ));

        // BOB 实际支付应为 80 * 1000 = 80,000 NEX (而非 100 * 1000 = 100,000)
        let bob_after = Balances::free_balance(BOB);
        let bob_paid = bob_before - bob_after;
        assert_eq!(
            bob_paid, 80_000,
            "FOK buy should only pay at actual fill price, excess refunded"
        );

        // BOB 不应有任何残留锁定
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

/// H1-R6: FOK 不应将自己的订单计入可成交量
#[test]
fn h1_r6_fok_excludes_self_orders_from_fillable() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 500 Token @ 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));

        // BOB 也挂卖单: 500 Token @ 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100
        ));

        // BOB 发 FOK 买单: 1000 Token @ 100
        // 可用量只有 ALICE 的 500 (BOB 自己的 500 被排除)
        // 500 < 1000 → 应失败
        assert_noop!(
            EntityMarket::place_fok_order(
                RuntimeOrigin::signed(BOB),
                ENTITY_ID,
                OrderSide::Buy,
                1000,
                100
            ),
            Error::<Test>::FokNotFullyFillable
        );

        // BOB 不应有任何残留锁定的 NEX
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

/// H2-R6: IOC 买单在价格改善时应退还多余 NEX
#[test]
fn h2_r6_ioc_buy_refunds_excess_nex_on_price_improvement() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单: 500 Token @ 80 (低于买方限价)
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            80
        ));

        let bob_before = Balances::free_balance(BOB);

        // BOB 发 IOC 买单: 1000 Token @ 100 (只能成交 500, 且以 80 成交)
        assert_ok!(EntityMarket::place_ioc_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            1000,
            100
        ));

        // BOB 实际支付 = 500 * 80 = 40,000 NEX
        // 修复前: 会退还 remaining(500) * 100 = 50,000, 但 crossed(500) * (100-80) = 10,000 卡住
        let bob_after = Balances::free_balance(BOB);
        let bob_paid = bob_before - bob_after;
        assert_eq!(bob_paid, 40_000, "IOC buy should only pay actual fill cost");

        // 不应有残留锁定
        assert_eq!(Balances::reserved_balance(BOB), 0);
    });
}

/// M1-R6: on_idle 过期清理后应更新 BestAsk/BestBid 缓存
#[test]
fn m1_r6_on_idle_updates_best_prices_after_cleanup() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单 @ 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        // 推进到过期后
        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 运行 on_idle
        let weight = EntityMarket::on_idle(
            System::block_number(),
            // 审计修复 L1-R9: on_idle 现在同时检查 proof_size，需提供足够的 proof_size
            Weight::from_parts(1_000_000, 100_000),
        );
        assert!(weight.ref_time() > 0);

        // BestAsk 应已更新为 None（订单已过期清理）
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), None);
    });
}

/// M2-R6: modify_order 应拒绝将数量减少到低于 min_order_amount
#[test]
fn m2_r6_modify_order_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        // 配置 min_order_amount = 10
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            10,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        // ALICE 挂卖单: 100 Token @ 100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            100
        ));

        // 修改数量到 5 (低于 min_order_amount=10) → 应失败
        assert_noop!(
            EntityMarket::modify_order(RuntimeOrigin::signed(ALICE), 0, 100, 5),
            Error::<Test>::OrderAmountBelowMinimum
        );

        // 修改到 10 (等于 min_order_amount) → 应成功
        assert_ok!(EntityMarket::modify_order(
            RuntimeOrigin::signed(ALICE),
            0,
            100,
            10
        ));
    });
}

/// L1-R6: get_sell_orders/get_buy_orders 应过滤过期订单
#[test]
fn l1_r6_query_functions_filter_expired_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            50
        ));

        // 未过期时应有订单
        assert_eq!(EntityMarket::get_sell_orders(ENTITY_ID).len(), 1);
        assert_eq!(EntityMarket::get_buy_orders(ENTITY_ID).len(), 1);

        // 推进到过期后（不运行 cleanup）
        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 过期后查询应返回空
        assert_eq!(EntityMarket::get_sell_orders(ENTITY_ID).len(), 0);
        assert_eq!(EntityMarket::get_buy_orders(ENTITY_ID).len(), 0);
    });
}

// ==================== Round 7 审计回归测试 ====================

/// H1-R7: market_buy 跳过自己的卖单（防止洗盘）
#[test]
fn h1_r7_market_buy_skips_self_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        let alice_nex_before = Balances::free_balance(ALICE);
        let alice_token_before = get_token_balance(ENTITY_ID, ALICE);

        // ALICE 尝试用市价买单吃自己的卖单 — 跳过自己的订单，filled=0 触发 AmountTooSmall
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 100_000),
            Error::<Test>::AmountTooSmall
        );

        // 余额不变
        assert_eq!(Balances::free_balance(ALICE), alice_nex_before);
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), alice_token_before);
    });
}

/// H1-R7: market_sell 跳过自己的买单（防止洗盘）
#[test]
fn h1_r7_market_sell_skips_self_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂买单
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // ALICE 尝试用市价卖单吃自己的买单 — 跳过自己的订单，filled=0 触发 AmountTooSmall
        assert_noop!(
            EntityMarket::market_sell(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 0),
            Error::<Test>::AmountTooSmall
        );
    });
}

/// H1-R7: market_buy 只跳过自己的，其他人的订单正常成交
#[test]
fn h1_r7_market_buy_fills_others_skips_self() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂卖单 @100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        // BOB 也挂卖单 @100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            500,
            100
        ));

        // ALICE 市价买 500 — 应只吃 BOB 的单，跳过自己的
        assert_ok!(EntityMarket::market_buy(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100_000
        ));

        // ALICE 的卖单仍在（未被吃）
        let alice_order = Orders::<Test>::get(0).unwrap();
        assert_eq!(alice_order.filled_amount, 0);

        // BOB 的卖单被完全吃掉
        let bob_order = Orders::<Test>::get(1).unwrap();
        assert_eq!(bob_order.filled_amount, 500);
    });
}

/// H2-R7: market_buy 在熔断期间被拒绝
#[test]
fn h2_r7_market_buy_rejected_during_circuit_breaker() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        // 手动激活熔断器
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 1000,
                initial_price: Some(100),
            },
        );

        // 市价买单应被熔断器拒绝
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(ALICE), ENTITY_ID, 500, 100_000),
            Error::<Test>::MarketCircuitBreakerActive
        );
    });
}

/// H2-R7: market_sell 在熔断期间被拒绝
#[test]
fn h2_r7_market_sell_rejected_during_circuit_breaker() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂买单
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        // 手动激活熔断器
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 1000,
                initial_price: Some(100),
            },
        );

        // 市价卖单应被熔断器拒绝
        assert_noop!(
            EntityMarket::market_sell(RuntimeOrigin::signed(ALICE), ENTITY_ID, 500, 0),
            Error::<Test>::MarketCircuitBreakerActive
        );
    });
}

/// H2-R7: take_order 在熔断期间被拒绝
#[test]
fn h2_r7_take_order_rejected_during_circuit_breaker() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        // 手动激活熔断器
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 1000,
                initial_price: Some(100),
            },
        );

        // take_order 应被熔断器拒绝
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(ALICE), 0, Some(500)),
            Error::<Test>::MarketCircuitBreakerActive
        );
    });
}

/// H2-R7: 熔断到期后 market_buy 恢复正常
#[test]
fn h2_r7_market_buy_works_after_circuit_breaker_expires() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        // 激活熔断器，until_block = 50
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 50,
                initial_price: Some(100),
            },
        );

        // 推进到熔断到期后
        System::set_block_number(50);

        // 市价买单应成功
        assert_ok!(EntityMarket::market_buy(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100_000
        ));
    });
}

/// 无手续费: do_cross_match 全额转账，entity_owner 不收取任何费用
#[test]
fn no_fee_cross_match_full_amount_to_maker() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // BOB 挂卖单 @100, 数量 1000
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        let owner_before = Balances::free_balance(ENTITY_OWNER);
        let bob_before = Balances::free_balance(BOB);

        // ALICE 挂买单 @100, 数量 1000 — 自动撮合
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // entity_owner 不应收到任何费用
        let owner_after = Balances::free_balance(ENTITY_OWNER);
        assert_eq!(
            owner_after, owner_before,
            "entity_owner should receive no fee"
        );

        // BOB 应收到全额 100_000 NEX
        let bob_after = Balances::free_balance(BOB);
        assert_eq!(
            bob_after - bob_before,
            100_000,
            "maker receives full amount"
        );
    });
}

/// 无手续费: take_order Buy 分支 - 全额转给 taker
#[test]
fn no_fee_take_order_buy_full_amount() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // ALICE 挂买单 @100, 数量 500
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));

        let owner_before = Balances::free_balance(ENTITY_OWNER);
        let bob_before = Balances::free_balance(BOB);
        let alice_reserved_before = Balances::reserved_balance(ALICE);

        // BOB 吃 ALICE 的买单
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            Some(500)
        ));

        // entity_owner 不应收到任何费用
        let owner_after = Balances::free_balance(ENTITY_OWNER);
        assert_eq!(
            owner_after, owner_before,
            "entity_owner should receive no fee"
        );

        // BOB 应收到全额 50_000 NEX
        let bob_after = Balances::free_balance(BOB);
        assert_eq!(bob_after - bob_before, 50_000, "taker receives full amount");

        // ALICE 的 reserved 应减少 total_next (全部用于支付)
        let alice_reserved_after = Balances::reserved_balance(ALICE);
        assert_eq!(alice_reserved_before - alice_reserved_after, 50_000);
    });
}

// ==================== Round 8 回归测试 ====================

/// M1-R8: on_idle 使用游标扫描 — 老订单（ID 较小）也能被清理
#[test]
fn m1_r8_on_idle_cursor_cleans_old_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 在 block 1 创建订单（TTL=1000，expires_at=1001）
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            50
        ));
        let order_id_0 = 0u64;

        // 确认订单存在
        assert!(EntityMarket::orders(order_id_0).is_some());
        assert_eq!(
            EntityMarket::orders(order_id_0).unwrap().status,
            OrderStatus::Open
        );

        // 推进到过期后
        System::set_block_number(1100);

        // 游标初始为 0，on_idle 应该从 0 开始扫描并清理
        let weight = EntityMarket::on_idle(1100u64, Weight::from_parts(1_000_000, 100_000));
        assert!(weight.ref_time() > 0);
        // M2-R8: 权重包含 proof_size
        assert!(weight.proof_size() > 0);

        // 订单应被标记为 Expired
        let order = EntityMarket::orders(order_id_0).unwrap();
        assert_eq!(order.status, OrderStatus::Expired);

        // 仅 1 个订单，scan_end(1) >= next_id(1)，游标归零
        assert_eq!(crate::OnIdleCursor::<Test>::get(), 0);
    });
}

/// M1-R8: on_idle 游标到达末尾后归零重新扫描
#[test]
fn m1_r8_on_idle_cursor_wraps_around() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 创建一个订单，NextOrderId = 1
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            50
        ));

        // 手动设置游标到一个很大的值（超过 NextOrderId）
        crate::OnIdleCursor::<Test>::put(9999);

        // on_idle 应检测到 cursor >= next_id，从 0 开始
        System::set_block_number(1100);
        EntityMarket::on_idle(1100u64, Weight::from_parts(1_000_000, 100_000));

        // 游标应该归零后重新扫描（scan_end = min(0+200, 1) = 1）
        // 由于 scan_end(1) >= next_id(1)，游标再次归零
        assert_eq!(crate::OnIdleCursor::<Test>::get(), 0);

        // 订单应被清理
        let order = EntityMarket::orders(0).unwrap();
        assert_eq!(order.status, OrderStatus::Expired);
    });
}

/// L1-R8: resume_market 在市场未暂停时应拒绝
#[test]
fn l1_r8_resume_market_rejects_when_not_paused() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 市场已启用但未暂停，尝试 resume 应失败
        assert_noop!(
            EntityMarket::resume_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::MarketNotPaused
        );

        // 先暂停
        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // 恢复应成功
        assert_ok!(EntityMarket::resume_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // 再次 resume 应失败（已恢复）
        assert_noop!(
            EntityMarket::resume_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::MarketNotPaused
        );
    });
}

/// L1-R8: pause_market 双重暂停被拒绝（已有保护，回归验证）
#[test]
fn l1_r8_pause_market_rejects_double_pause() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));
        assert_noop!(
            EntityMarket::pause_market(RuntimeOrigin::signed(ENTITY_OWNER), ENTITY_ID),
            Error::<Test>::MarketPaused
        );
    });
}

// ==================== Round 9 审计回归测试 ====================

/// M1-R9: on_idle 返回的权重应反映实际扫描开销（不仅是清理数）
#[test]
fn m1_r9_on_idle_returns_accurate_weight() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        // 放置 3 个订单（未过期）
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            200
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            300
        ));

        // on_idle 扫描 3 个活跃订单（无过期），消耗 base_weight * 3
        let weight = EntityMarket::on_idle(
            System::block_number(),
            Weight::from_parts(10_000_000, 100_000),
        );
        // 扫描了 3 个 ID，每个消耗 base_weight(5000, 64)
        assert!(
            weight.ref_time() >= 15_000,
            "should account for all scanned IDs, got {}",
            weight.ref_time()
        );
        assert!(
            weight.proof_size() >= 192,
            "should account for proof_size per scan, got {}",
            weight.proof_size()
        );
    });
}

/// M1-R9: on_idle 清理过期订单时权重应包含 per_order_weight
#[test]
fn m1_r9_on_idle_weight_includes_cleanup_cost() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            100
        ));
        // 推进到过期
        System::set_block_number(20000);

        let weight = EntityMarket::on_idle(
            System::block_number(),
            Weight::from_parts(10_000_000, 100_000),
        );
        // 扫描 1 个 ID + 清理 1 个: base_weight(5000) + per_order_weight(30000) = 35000
        assert!(
            weight.ref_time() >= 35_000,
            "should include per_order cleanup cost, got {}",
            weight.ref_time()
        );
        assert!(
            weight.proof_size() >= 576,
            "should include proof_size for cleanup, got {}",
            weight.proof_size()
        );
    });
}

/// M2-R9: modify_order 在市场暂停时应被拒绝
#[test]
fn m2_r9_modify_order_rejected_during_market_pause() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 暂停市场
        assert_ok!(EntityMarket::pause_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // 修改订单应失败（市场暂停）
        assert_noop!(
            EntityMarket::modify_order(RuntimeOrigin::signed(ALICE), 0, 90, 500),
            Error::<Test>::MarketPaused
        );
    });
}

/// M2-R9: modify_order 在市场关闭后应被拒绝
#[test]
fn m2_r9_modify_order_rejected_after_market_close() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 关闭市场（会取消所有订单）
        assert_ok!(EntityMarket::close_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID
        ));

        // 修改订单应失败（订单已被取消 + 市场关闭）
        assert_noop!(
            EntityMarket::modify_order(RuntimeOrigin::signed(ALICE), 0, 90, 500),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

/// M3-R9: batch_cancel_orders 使用 BoundedVec 限制长度
#[test]
fn m3_r9_batch_cancel_bounded_vec_limit() {
    ExtBuilder::build().execute_with(|| {
        // 尝试创建超过 50 个元素的 BoundedVec 应失败
        let too_many: Vec<u64> = (0..51).collect();
        assert!(
            BoundedVec::<u64, frame_support::traits::ConstU32<50>>::try_from(too_many).is_err(),
            "BoundedVec should reject > 50 elements"
        );
    });
}

/// L1-R9: on_idle 应在 proof_size 不足时提前退出
#[test]
fn l1_r9_on_idle_respects_proof_size_limit() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            100
        ));
        // 推进到过期
        System::set_block_number(20000);

        // 提供充足的 ref_time 但极少的 proof_size
        let weight = EntityMarket::on_idle(
            System::block_number(),
            Weight::from_parts(10_000_000, 100), // proof_size=100 < per_order(512)
        );
        // 不应清理任何订单（proof_size 不足）
        let order = Orders::<Test>::get(0).unwrap();
        assert_eq!(
            order.status,
            OrderStatus::Open,
            "order should NOT be cleaned due to proof_size limit"
        );
        assert!(
            weight.proof_size() <= 100,
            "consumed proof_size should be minimal"
        );
    });
}

// ==================== Round 10 审计修复回归测试 ====================

/// H1-R10: IOC 订单应受 min_order_amount 限制
#[test]
fn h1_r10_ioc_order_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        // 配置 min_order_amount = 100
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            100,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        // IOC 订单数量 50 < 100 → 应失败
        assert_noop!(
            EntityMarket::place_ioc_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Buy,
                50,
                100
            ),
            Error::<Test>::OrderAmountBelowMinimum
        );

        // IOC 订单数量 100 = min → 应成功（预锁定后因无对手盘，部分退还）
        assert_ok!(EntityMarket::place_ioc_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            OrderSide::Buy,
            100,
            100
        ));
    });
}

/// H1-R10: FOK 订单应受 min_order_amount 限制
#[test]
fn h1_r10_fok_order_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            100,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        assert_noop!(
            EntityMarket::place_fok_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Sell,
                50,
                100
            ),
            Error::<Test>::OrderAmountBelowMinimum
        );
    });
}

/// H1-R10: PostOnly 订单应受 min_order_amount 限制
#[test]
fn h1_r10_post_only_order_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            100,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        assert_noop!(
            EntityMarket::place_post_only_order(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                OrderSide::Sell,
                50,
                100
            ),
            Error::<Test>::OrderAmountBelowMinimum
        );

        // 数量 >= min → 应成功
        assert_ok!(EntityMarket::place_post_only_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            OrderSide::Sell,
            100,
            100
        ));
    });
}

/// H2-R10: configure_market 禁用 nex_enabled 时自动取消所有订单
#[test]
fn h2_r10_disable_market_cancels_all_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        let alice_tokens_before = get_token_balance(ENTITY_ID, ALICE);
        let bob_balance_before = Balances::free_balance(BOB);

        // Alice 挂卖单，Bob 挂买单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            300,
            50
        ));

        // 验证资产已锁定
        assert!(get_token_reserved(ENTITY_ID, ALICE) > 0);
        assert!(Balances::reserved_balance(BOB) > 0);

        // 禁用市场
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            1,
            1000,
        ));

        // 所有订单应被取消，资产应退还
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), alice_tokens_before);
        assert_eq!(Balances::free_balance(BOB), bob_balance_before);

        // 订单簿应为空
        assert_eq!(EntitySellOrders::<Test>::get(ENTITY_ID).len(), 0);
        assert_eq!(EntityBuyOrders::<Test>::get(ENTITY_ID).len(), 0);
    });
}

/// H2-R10: 首次配置 nex_enabled=false 不应触发取消（was_enabled=false）
#[test]
fn h2_r10_first_configure_disabled_no_cancel() {
    ExtBuilder::build().execute_with(|| {
        // 首次配置为 disabled — was_enabled=false, nex_enabled=false → 不触发
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            1,
            1000,
        ));
        let config = MarketConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert!(!config.nex_enabled);
    });
}

/// H3-R10: governance_configure_price_protection 正常工作
#[test]
fn h3_r10_governance_configure_price_protection_works() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::governance_configure_price_protection(
            RuntimeOrigin::root(),
            ENTITY_ID,
            true,
            3000,
            800,
            6000,
            50,
        ));

        let config = PriceProtection::<Test>::get(ENTITY_ID).expect("config exists");
        assert!(config.enabled);
        assert_eq!(config.max_price_deviation, 3000);
        assert_eq!(config.max_slippage, 800);
        assert_eq!(config.circuit_breaker_threshold, 6000);
        assert_eq!(config.min_trades_for_twap, 50);
    });
}

/// H3-R10: governance_configure_price_protection 拒绝非 Root
#[test]
fn h3_r10_governance_configure_price_protection_rejects_signed() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::governance_configure_price_protection(
                RuntimeOrigin::signed(ALICE),
                ENTITY_ID,
                true,
                3000,
                800,
                6000,
                50,
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

/// H3-R10: governance_configure_price_protection 验证基点参数
#[test]
fn h3_r10_governance_configure_price_protection_rejects_invalid_bps() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::governance_configure_price_protection(
                RuntimeOrigin::root(),
                ENTITY_ID,
                true,
                10001,
                800,
                6000,
                50,
            ),
            Error::<Test>::InvalidBasisPoints
        );
        assert_noop!(
            EntityMarket::governance_configure_price_protection(
                RuntimeOrigin::root(),
                ENTITY_ID,
                true,
                3000,
                10001,
                6000,
                50,
            ),
            Error::<Test>::InvalidBasisPoints
        );
        assert_noop!(
            EntityMarket::governance_configure_price_protection(
                RuntimeOrigin::root(),
                ENTITY_ID,
                true,
                3000,
                800,
                10001,
                50,
            ),
            Error::<Test>::InvalidBasisPoints
        );
    });
}

/// H4-R10: force_lift_circuit_breaker 正常工作
#[test]
fn h4_r10_force_lift_circuit_breaker_works() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // 先挂卖单（熔断前）
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 然后手动激活熔断器
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 99999,
                initial_price: Some(100),
            },
        );

        // 熔断期间 take_order 应失败
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, None),
            Error::<Test>::MarketCircuitBreakerActive
        );

        // Root 强制解除熔断
        assert_ok!(EntityMarket::force_lift_circuit_breaker(
            RuntimeOrigin::root(),
            ENTITY_ID
        ));

        // 熔断解除后 take_order 应成功
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
    });
}

/// H4-R10: force_lift_circuit_breaker 拒绝非 Root
#[test]
fn h4_r10_force_lift_circuit_breaker_rejects_signed() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::force_lift_circuit_breaker(RuntimeOrigin::signed(ALICE), ENTITY_ID),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

/// H4-R10: force_lift_circuit_breaker 未激活时应失败
#[test]
fn h4_r10_force_lift_circuit_breaker_rejects_not_active() {
    ExtBuilder::build().execute_with(|| {
        assert_noop!(
            EntityMarket::force_lift_circuit_breaker(RuntimeOrigin::root(), ENTITY_ID),
            Error::<Test>::CircuitBreakerNotActive
        );
    });
}

/// C2-R10: on_idle 清理过期订单时应发出 ExpiredOrdersAutoCleaned 事件
#[test]
fn c2_r10_on_idle_emits_event_on_cleanup() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            100
        ));
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            200
        ));

        // 推进到过期后
        System::set_block_number(20000);

        // 清除之前的事件
        System::reset_events();

        // 运行 on_idle
        EntityMarket::on_idle(20000u64, Weight::from_parts(10_000_000, 100_000));

        // 应有 ExpiredOrdersAutoCleaned 事件
        let events = frame_system::Pallet::<Test>::events();
        let found = events.iter().any(|e| {
            matches!(&e.event, RuntimeEvent::EntityMarket(
                crate::pallet::Event::ExpiredOrdersAutoCleaned { count }
            ) if *count == 2)
        });
        assert!(
            found,
            "ExpiredOrdersAutoCleaned event with count=2 not found"
        );
    });
}

/// R2-R10: PostOnly 应使用动态计算过滤过期订单
#[test]
fn r2_r10_post_only_uses_dynamic_best_price() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        // Alice 挂卖单 @100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        // Bob 尝试 PostOnly 买单 @100 — 应因会立即撮合被拒绝
        assert_noop!(
            EntityMarket::place_post_only_order(
                RuntimeOrigin::signed(BOB),
                ENTITY_ID,
                OrderSide::Buy,
                500,
                100
            ),
            Error::<Test>::PostOnlyWouldMatch
        );

        // 推进到卖单过期后（但不运行 on_idle，BestAsk 缓存仍为 100）
        let order = Orders::<Test>::get(0).unwrap();
        System::set_block_number(order.expires_at + 1);

        // 缓存仍为旧值
        assert_eq!(BestAsk::<Test>::get(ENTITY_ID), Some(100u128));

        // 修复后: PostOnly 使用动态计算，过期单被过滤 → 应成功
        assert_ok!(EntityMarket::place_post_only_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            OrderSide::Buy,
            500,
            100
        ));
    });
}

// ==================== Round 11 审计修复回归测试 ====================

/// S1-R11: governance_configure_market 禁用时自动取消订单（与 configure_market 一致）
#[test]
fn s1_r11_governance_disable_market_cancels_orders() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        let alice_tokens_before = get_token_balance(ENTITY_ID, ALICE);
        let bob_balance_before = Balances::free_balance(BOB);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            500,
            100
        ));
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            300,
            50
        ));

        assert!(get_token_reserved(ENTITY_ID, ALICE) > 0);
        assert!(Balances::reserved_balance(BOB) > 0);

        // Root 禁用市场
        assert_ok!(EntityMarket::governance_configure_market(
            RuntimeOrigin::root(),
            ENTITY_ID,
            false,
            1,
            1000,
        ));

        // 资产应全部退还
        assert_eq!(get_token_reserved(ENTITY_ID, ALICE), 0);
        assert_eq!(Balances::reserved_balance(BOB), 0);
        assert_eq!(get_token_balance(ENTITY_ID, ALICE), alice_tokens_before);
        assert_eq!(Balances::free_balance(BOB), bob_balance_before);
    });
}

/// S2-R11: 熔断器到期后自动清理存储（通过 ensure_circuit_breaker_inactive 触发）
#[test]
fn s2_r11_circuit_breaker_auto_cleanup_on_expiry() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID);

        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // 手动激活熔断器，到期区块 = 500
        PriceProtection::<Test>::insert(
            ENTITY_ID,
            PriceProtectionConfig {
                enabled: true,
                max_price_deviation: 2000,
                max_slippage: 500,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: true,
                circuit_breaker_until: 500,
                initial_price: Some(100),
            },
        );

        // 区块 100: 熔断中
        System::set_block_number(100);
        assert_noop!(
            EntityMarket::take_order(RuntimeOrigin::signed(BOB), 0, None),
            Error::<Test>::MarketCircuitBreakerActive
        );

        // 区块 500: 熔断到期，交易应成功
        System::set_block_number(500);
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 存储应自动清理
        let config = PriceProtection::<Test>::get(ENTITY_ID).unwrap();
        assert!(
            !config.circuit_breaker_active,
            "circuit_breaker_active should be false after auto-cleanup"
        );
        assert_eq!(
            config.circuit_breaker_until, 0,
            "circuit_breaker_until should be 0 after auto-cleanup"
        );
    });
}

/// S3-R11: market_buy 受 min_order_amount 限制
#[test]
fn s3_r11_market_buy_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            100,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        // 先挂卖单
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));

        // market_buy 50 < min_order_amount(100) → 应失败
        assert_noop!(
            EntityMarket::market_buy(RuntimeOrigin::signed(BOB), ENTITY_ID, 50, 999999),
            Error::<Test>::OrderAmountBelowMinimum
        );

        // market_buy 100 >= min → 应成功
        assert_ok!(EntityMarket::market_buy(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            100,
            999999
        ));
    });
}

/// S3-R11: market_sell 受 min_order_amount 限制
#[test]
fn s3_r11_market_sell_rejects_below_min_amount() {
    ExtBuilder::build().execute_with(|| {
        assert_ok!(EntityMarket::configure_market(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true,
            100,
            1000,
        ));
        assert_ok!(EntityMarket::set_initial_price(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            100
        ));
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            false,
            2000,
            500,
            5000,
            100
        ));

        // 先挂买单
        assert_ok!(EntityMarket::place_buy_order(
            RuntimeOrigin::signed(BOB),
            ENTITY_ID,
            1000,
            100
        ));

        // market_sell 50 < min_order_amount(100) → 应失败
        assert_noop!(
            EntityMarket::market_sell(RuntimeOrigin::signed(ALICE), ENTITY_ID, 50, 0),
            Error::<Test>::OrderAmountBelowMinimum
        );

        // market_sell 100 >= min → 应成功
        assert_ok!(EntityMarket::market_sell(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            100,
            0
        ));
    });
}

// ==================== v2.5.0: 参考价 initial_price → TWAP 切换修复 ====================

/// 回归测试: 成交量与历史覆盖达标后，价格偏离检查的参考价应从
/// initial_price 切换到 1h TWAP（修复前该切换不可达）
#[test]
fn reference_price_switches_from_initial_price_to_twap() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID); // initial_price = 100
        assert_ok!(EntityMarket::configure_price_protection(
            RuntimeOrigin::signed(ENTITY_OWNER),
            ENTITY_ID,
            true, // enabled
            2000, // max_deviation = 20%
            500,
            5000,
            1, // min_trades_for_twap = 1
        ));

        // 区块 1: 一笔真实成交 @110（偏离 initial 100 仅 10%，通过）
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            110
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));
        let acc = TwapAccumulators::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(acc.trade_count, 1);
        assert_eq!(acc.first_trade_block, 1);

        // 区块 2: 历史覆盖不足 1h → 参考价仍为 initial_price(100)
        // 挂单 @125 偏离 25% > 20% → 拒绝
        System::set_block_number(2);
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 125),
            Error::<Test>::PriceDeviationTooHigh
        );

        // 区块 602: 覆盖 601 区块 >= BlocksPerHour(600) 且成交数达标
        // → 参考价切换为 1h TWAP(≈110)
        // 挂单 @125 偏离 TWAP 仅 ~13.6% < 20% → 通过（修复前会一直按 100 拒绝）
        System::set_block_number(602);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            125
        ));

        // 偏离 TWAP 超限仍应拒绝: @135 偏离 110 约 22.7% > 20%
        assert_noop!(
            EntityMarket::place_sell_order(RuntimeOrigin::signed(ALICE), ENTITY_ID, 1000, 135),
            Error::<Test>::PriceDeviationTooHigh
        );
    });
}

/// v2.5.0: 1h TWAP 应基于约一个周期前的 checkpoint 计算，
/// 近期价格跳变不应立刻主导 TWAP（双 checkpoint 轮换）
#[test]
fn twap_uses_period_old_checkpoint() {
    ExtBuilder::build().execute_with(|| {
        configure_market_enabled(ENTITY_ID); // 价格保护已禁用，价格不受限

        // 区块 1: 成交 @100
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            0,
            None
        ));

        // 区块 700: 成交 @100 → 触发 1h checkpoint 轮换
        // (hour_curr 距今 699 >= 600)
        System::set_block_number(700);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            100
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            1,
            None
        ));
        let acc = TwapAccumulators::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(acc.hour_prev.block_number, 1);
        assert_eq!(acc.hour_curr.block_number, 700);

        // 区块 750: 价格跳变到 200 成交
        System::set_block_number(750);
        assert_ok!(EntityMarket::place_sell_order(
            RuntimeOrigin::signed(ALICE),
            ENTITY_ID,
            1000,
            200
        ));
        assert_ok!(EntityMarket::take_order(
            RuntimeOrigin::signed(BOB),
            2,
            None
        ));

        // 区块 760: 1h TWAP 使用 hour_prev(区块 1) 作为基准
        // 累积 = 100×699 + 100×50 + 200×10 = 76900, 窗口 = 759
        // TWAP = 76900 / 759 = 101 — 反映过去 1 小时的 ~100，而非跳变后的 200
        System::set_block_number(760);
        assert_eq!(
            EntityMarket::calculate_twap(ENTITY_ID, TwapPeriod::OneHour),
            Some(101u128)
        );
    });
}

/// v1 迁移: 旧版 TwapAccumulator 应正确转换为双 checkpoint 结构
#[test]
fn v1_migration_translates_old_accumulator() {
    use crate::migrations::v1::{migrate, OldTwapAccumulator};
    use codec::Encode;

    ExtBuilder::build().execute_with(|| {
        // 有成交的旧累积器
        let old_traded = OldTwapAccumulator::<u128> {
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
        // 仅 set_initial_price、无成交的旧累积器
        let old_untraded = OldTwapAccumulator::<u128> {
            current_cumulative: 0,
            current_block: 5,
            last_price: 100,
            trade_count: 0,
            hour_snapshot: PriceSnapshot {
                cumulative_price: 0,
                block_number: 5,
            },
            day_snapshot: PriceSnapshot {
                cumulative_price: 0,
                block_number: 5,
            },
            week_snapshot: PriceSnapshot {
                cumulative_price: 0,
                block_number: 5,
            },
            last_hour_update: 5,
            last_day_update: 5,
            last_week_update: 5,
        };
        frame_support::storage::unhashed::put_raw(
            &TwapAccumulators::<Test>::hashed_key_for(ENTITY_ID),
            &old_traded.encode(),
        );
        frame_support::storage::unhashed::put_raw(
            &TwapAccumulators::<Test>::hashed_key_for(ENTITY_ID_2),
            &old_untraded.encode(),
        );

        migrate::<Test>();

        let acc = TwapAccumulators::<Test>::get(ENTITY_ID).unwrap();
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

        // 无成交 → first_trade_block 保持 0 哨兵值
        let acc2 = TwapAccumulators::<Test>::get(ENTITY_ID_2).unwrap();
        assert_eq!(acc2.first_trade_block, 0);
        assert_eq!(acc2.trade_count, 0);
    });
}
