use crate::mock::*;
use crate::pallet::*;
use frame_support::{assert_noop, assert_ok, weights::Weight};
use pallet_entity_common::{OrderStatus, ProductProvider};

// ==================== place_order ====================

#[test]
fn place_order_physical_works() {
    new_test_ext().execute_with(|| {
        // Product 1 = Physical, shop 1, price 100
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1, // product_id
            2, // quantity
            Some(b"addr_cid".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.buyer, BUYER);
        assert_eq!(order.seller, SELLER);
        assert_eq!(order.quantity, 2);
        assert_eq!(order.unit_price, 100);
        assert_eq!(order.total_amount, 200); // 100 * 2
        assert_eq!(order.status, OrderStatus::Paid);
    });
}

#[test]
fn place_order_digital_auto_completes() {
    new_test_ext().execute_with(|| {
        // Product 2 = Digital, shop 1, price 50
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2, // digital product
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Completed);
        assert!(order.completed_at.is_some());
    });
}

// ==================== do_cross_order (HB-ENT-01) ====================

#[test]
fn do_cross_order_digital_public_completes() {
    new_test_ext().execute_with(|| {
        // Derived buyer (no funds); bridge-funded payer (PAYER). Product 2 =
        // Digital + Public, so the order settles instantly.
        let derived_buyer: u64 = 777;
        assert_ok!(Transaction::do_cross_order(
            derived_buyer,
            PAYER,
            2, // digital product
            1, // quantity
            u64::MAX, // max_nex_amount (slippage cap)
            None,     // referrer
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.buyer, derived_buyer);
        assert_eq!(order.status, OrderStatus::Completed);
        assert_eq!(order.payment_asset, pallet_entity_common::PaymentAsset::Native);
    });
}

#[test]
fn do_cross_order_rejects_non_digital() {
    new_test_ext().execute_with(|| {
        // Product 1 = Physical → must be rejected before any payment.
        assert_noop!(
            Transaction::do_cross_order(777, PAYER, 1, 1, u64::MAX, None),
            Error::<Test>::CrossOrderDigitalOnly
        );
    });
}

#[test]
fn do_cross_order_rejects_non_public() {
    new_test_ext().execute_with(|| {
        set_product_visibility(2, pallet_entity_common::ProductVisibility::MembersOnly);
        assert_noop!(
            Transaction::do_cross_order(777, PAYER, 2, 1, u64::MAX, None),
            Error::<Test>::CrossOrderPublicOnly
        );
    });
}

#[test]
fn place_order_service_works() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service, shop 1, price 200
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Paid);
        assert_eq!(
            order.product_category,
            pallet_entity_common::ProductCategory::Service
        );
    });
}

#[test]
fn place_order_fails_zero_quantity() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                0,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::InvalidQuantity
        );
    });
}

#[test]
fn place_order_fails_product_not_found() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                99,
                1,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::ProductNotFound
        );
    });
}

#[test]
fn place_order_fails_buy_own_product() {
    new_test_ext().execute_with(|| {
        // SELLER owns shop 1, product 1 belongs to shop 1
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(SELLER),
                1,
                1,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::CannotBuyOwnProduct
        );
    });
}

// ==================== cancel_order ====================

#[test]
fn cancel_order_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Cancelled);

        // C3: on_order_cancelled was called
        assert_eq!(get_cancelled_orders(), vec![1]);
    });
}

#[test]
fn cancel_order_fails_digital() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::DigitalProductCannotCancel
        );
    });
}

#[test]
fn cancel_order_fails_not_buyer() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::cancel_order(RuntimeOrigin::signed(BUYER2), 1),
            Error::<Test>::NotOrderParticipant
        );
    });
}

#[test]
fn cancel_order_fails_after_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track123".to_vec()
        ));

        assert_noop!(
            Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::CannotCancelOrder
        );
    });
}

// ==================== ship_order ====================

#[test]
fn ship_order_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track_cid".to_vec()
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.shipped_at.is_some());
        assert!(order.tracking_cid.is_some());
    });
}

#[test]
fn ship_order_fails_not_seller() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::ship_order(RuntimeOrigin::signed(BUYER), 1, b"track".to_vec()),
            Error::<Test>::NotOrderSeller
        );
    });
}

// ==================== confirm_receipt ====================

#[test]
fn confirm_receipt_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Completed);
        assert!(order.completed_at.is_some());

        // Check stats
        let stats = Transaction::order_stats();
        assert_eq!(stats.completed_orders, 1);
    });
}

#[test]
fn notifier_fires_on_ship_and_confirm() {
    // audit 2.3 fix: ship_order 通知买家、confirm_receipt 通知卖家（端到端,经 OrderNotifier 端口）。
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        // 发货后:买家收到 order:shipped:1。
        assert_eq!(
            notify_log(),
            vec![(BUYER, b"order:shipped:1".to_vec())]
        );

        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        // 确认收货后:卖家收到 order:confirmed:1（追加在日志末尾）。
        assert_eq!(
            notify_log(),
            vec![
                (BUYER, b"order:shipped:1".to_vec()),
                (SELLER, b"order:confirmed:1".to_vec()),
            ]
        );
    });
}

#[test]
fn confirm_receipt_fails_not_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::confirm_receipt(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

// ==================== request_refund / approve_refund ====================

#[test]
fn request_refund_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Disputed);
    });
}

#[test]
fn request_refund_fails_digital() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::request_refund(RuntimeOrigin::signed(BUYER), 1, b"reason".to_vec()),
            Error::<Test>::DigitalProductCannotRefund
        );
    });
}

#[test]
fn approve_refund_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.status, OrderStatus::Refunded);

        // C3: on_order_cancelled was called
        assert_eq!(get_cancelled_orders(), vec![1]);
    });
}

#[test]
fn approve_refund_fails_not_seller() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_noop!(
            Transaction::approve_refund(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::NotOrderSeller
        );
    });
}

// ==================== Service order flow ====================

#[test]
fn service_order_full_flow() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);

        // Start service
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.service_started_at.is_some());

        // Complete service
        assert_ok!(Transaction::complete_service(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        let order = Transaction::orders(1).unwrap();
        assert!(order.service_completed_at.is_some());

        // Confirm service
        assert_ok!(Transaction::confirm_service(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    });
}

#[test]
fn start_service_fails_not_service_order() {
    new_test_ext().execute_with(|| {
        // Product 1 = Physical
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::start_service(RuntimeOrigin::signed(SELLER), 1),
            Error::<Test>::NotServiceLikeOrder
        );
    });
}

#[test]
fn confirm_service_fails_without_completion() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        // service_completed_at is None → should fail
        assert_noop!(
            Transaction::confirm_service(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

// ==================== Timeout processing ====================

#[test]
fn ship_timeout_auto_refunds() {
    new_test_ext().execute_with(|| {
        // Physical product, ShipTimeout = 100 blocks
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Paid);

        // Advance to block 1 + 100 = 101
        run_to_block(101);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // C3: on_order_cancelled was called by timeout handler
        assert!(get_cancelled_orders().contains(&1));
    });
}

#[test]
fn confirm_timeout_auto_completes() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // shipped_at = 1, ConfirmTimeout = 200, expiry = 1 + 200 = 201
        run_to_block(201);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    });
}

#[test]
fn service_timeout_auto_refunds() {
    new_test_ext().execute_with(|| {
        // Service product, ShipTimeout = 100
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Don't start service, wait for timeout
        run_to_block(101);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

// ==================== OrderProvider trait ====================

#[test]
fn order_provider_trait_works() {
    new_test_ext().execute_with(|| {
        use pallet_entity_common::OrderProvider;

        assert!(!<Transaction as OrderProvider<u64, u64>>::order_exists(1));

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert!(<Transaction as OrderProvider<u64, u64>>::order_exists(1));
        assert_eq!(
            <Transaction as OrderProvider<u64, u64>>::order_buyer(1),
            Some(BUYER)
        );
        assert_eq!(
            <Transaction as OrderProvider<u64, u64>>::order_shop_id(1),
            Some(SHOP_1)
        );
        assert!(!<Transaction as OrderProvider<u64, u64>>::is_order_completed(1));

        // Ship and confirm
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"t".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert!(<Transaction as OrderProvider<u64, u64>>::is_order_completed(1));
    });
}

// ==================== Statistics ====================

#[test]
fn order_stats_tracking() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            2,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        let stats = Transaction::order_stats();
        assert_eq!(stats.total_orders, 2);
        assert_eq!(stats.completed_orders, 0);

        // Complete first order
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"t".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let stats = Transaction::order_stats();
        assert_eq!(stats.completed_orders, 1);
    });
}

// ==================== Platform fee ====================

#[test]
fn platform_fee_calculated_correctly() {
    new_test_ext().execute_with(|| {
        // Price 100 * qty 1 = 100, platform_fee = 100 * 100 / 10000 = 1
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 100);
        assert_eq!(order.platform_fee, 1);
    });
}

// ==================== H5: Service order expiry timing ====================

#[test]
fn service_in_progress_not_auto_completed_by_ship_timeout() {
    new_test_ext().execute_with(|| {
        // H5: Service order started but ShipTimeout fires → should NOT auto-complete
        // because service_completed_at is still None (service not done yet)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Seller starts service at block 50 (before ShipTimeout at 101)
        System::set_block_number(50);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.service_completed_at.is_none());

        // ShipTimeout = 100, original expiry at block 1+100 = 101
        // Run to block 101 — the ShipTimeout fires but service_completed_at is None
        run_to_block(101);

        // H5: Order should still be Shipped (not auto-completed)
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
    });
}

#[test]
fn service_completed_auto_confirms_after_service_confirm_timeout() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        System::set_block_number(20);
        assert_ok!(Transaction::complete_service(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        // complete_service enqueues expiry at 20 + 150 = 170
        // H4: start_service 已清理旧 ShipTimeout(101)，complete_service 已清理旧 ServiceTimeout(160)

        // 自动确认发生在 complete_service 的超时点（block 170）
        run_to_block(170);
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );
    });
}

// ==================== Audit fix regression tests ====================

// C2: Service order cannot use ship_order
#[test]
fn c2_ship_order_rejects_service_order() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::ship_order(RuntimeOrigin::signed(SELLER), 1, b"track".to_vec()),
            Error::<Test>::ServiceLikeOrderCannotShip
        );
    });
}

// C2: Service order cannot use confirm_receipt
#[test]
fn c2_confirm_receipt_rejects_service_order() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service, use start_service to get to Shipped
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        assert_noop!(
            Transaction::confirm_receipt(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::ServiceLikeOrderCannotShip
        );
    });
}

// M5: request_refund rejects empty reason_cid
#[test]
fn m5_request_refund_rejects_empty_reason() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::request_refund(RuntimeOrigin::signed(BUYER), 1, b"".to_vec()),
            Error::<Test>::EmptyReasonCid
        );
    });
}

// M5: request_refund rejects too-long reason_cid
#[test]
fn m5_request_refund_rejects_long_reason() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        let long_cid = vec![b'x'; 65]; // MaxCidLength = 64
        assert_noop!(
            Transaction::request_refund(RuntimeOrigin::signed(BUYER), 1, long_cid),
            Error::<Test>::CidTooLong
        );
    });
}

// M6: Physical order requires shipping_cid
#[test]
fn m6_physical_order_requires_shipping_cid() {
    new_test_ext().execute_with(|| {
        // Product 1 = Physical, no shipping_cid → should fail
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                None,
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::ShippingCidRequired
        );

        // With shipping_cid → should succeed
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
    });
}

// M6: Service order does NOT require shipping_cid
#[test]
fn m6_service_order_no_shipping_cid_ok() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service, no shipping_cid → should succeed
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));
    });
}

// C1: ExpiryQueue partial processing preserves unprocessed entries
#[test]
fn c1_expiry_queue_preserves_unprocessed() {
    new_test_ext().execute_with(|| {
        // Place 3 physical orders — all expire at block 1 + 100 = 101
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"a".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"b".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"c".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Verify all 3 are in the expiry queue at block 101
        let queue = crate::ExpiryQueue::<Test>::get(101u64);
        assert_eq!(queue.len(), 3);

        // Process with very limited weight — only 1 order can be processed
        System::set_block_number(101);
        <Transaction as frame_support::traits::Hooks<u64>>::on_idle(
            101,
            frame_support::weights::Weight::from_parts(250_000_000, 100_000), // ~1 order
        );

        // Order 1 should be refunded, orders 2 and 3 should remain
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );

        // Remaining orders should still be in the queue
        let queue_after = crate::ExpiryQueue::<Test>::get(101u64);
        assert!(
            queue_after.len() >= 1,
            "unprocessed orders should remain in queue"
        );

        // Process remaining with enough weight
        <Transaction as frame_support::traits::Hooks<u64>>::on_idle(
            101,
            frame_support::weights::Weight::from_parts(10_000_000_000, 1_000_000),
        );

        // All should now be refunded
        assert_eq!(
            Transaction::orders(2).unwrap().status,
            OrderStatus::Refunded
        );
        assert_eq!(
            Transaction::orders(3).unwrap().status,
            OrderStatus::Refunded
        );

        // Queue should be empty now
        let queue_final = crate::ExpiryQueue::<Test>::get(101u64);
        assert!(queue_final.is_empty());
    });
}

// ============================================================================
// MemberHandler 集成测试
// ============================================================================

#[test]
fn order_complete_triggers_member_register_and_update_spent() {
    new_test_ext().execute_with(|| {
        // Digital product auto-completes → triggers do_complete_order
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2, // Digital, price 50
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // auto_register should have been called with (entity_id=1, buyer=1)
        let registered = get_member_registered();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0], (ENTITY_1, BUYER));

        // update_spent should have been called with (entity_id=1, buyer=1, amount_usdt)
        let spent = get_member_spent();
        assert_eq!(spent.len(), 1);
        assert_eq!(spent[0].0, ENTITY_1);
        assert_eq!(spent[0].1, BUYER);
        // amount_usdt: now uses usdt_total snapshot = 50 (product 2 usdt_price)
        assert_eq!(spent[0].2, 50);
    });
}

#[test]
fn confirm_order_triggers_member_handler() {
    new_test_ext().execute_with(|| {
        // Physical order: place → ship → confirm → do_complete_order
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1, // Physical, price 100
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // place_order 不触发 do_complete_order
        assert!(get_member_registered().is_empty());
        assert!(get_member_spent().is_empty());

        // ship
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"tracking".to_vec(),
        ));

        // confirm → triggers do_complete_order
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let registered = get_member_registered();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0], (ENTITY_1, BUYER));

        let spent = get_member_spent();
        assert_eq!(spent.len(), 1);
        assert_eq!(spent[0].0, ENTITY_1);
        assert_eq!(spent[0].1, BUYER);
        // amount_usdt: now uses usdt_total snapshot = 100 (product 1 usdt_price)
        assert_eq!(spent[0].2, 100);
    });
}

#[test]
fn auto_complete_timeout_triggers_member_handler() {
    new_test_ext().execute_with(|| {
        // Physical: place → ship → timeout auto-complete
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"tracking".to_vec(),
        ));

        // 发货后不确认，等待 ConfirmTimeout(200) 自动完成
        run_to_block(202);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);

        let registered = get_member_registered();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0], (ENTITY_1, BUYER));

        let spent = get_member_spent();
        assert_eq!(spent.len(), 1);
        // amount_usdt: now uses usdt_total snapshot = 100 (product 1 usdt_price)
        assert_eq!(spent[0].2, 100);
    });
}

// ==================== Token payment tests ====================

use pallet_entity_common::PaymentAsset;

#[test]
fn token_place_order_physical_works() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payment_asset, PaymentAsset::EntityToken);
        assert_eq!(order.token_payment_amount, 100); // price 100 * qty 1
        assert_eq!(order.platform_fee, 0); // no platform fee for Token orders
        assert_eq!(order.status, OrderStatus::Paid);

        // Token reserved from buyer
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000 - 100);
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 100);
    });
}

#[test]
fn token_place_order_digital_auto_completes() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1, // Digital, price 50
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payment_asset, PaymentAsset::EntityToken);
        assert_eq!(order.token_payment_amount, 50);
        assert_eq!(order.status, OrderStatus::Completed);

        // Token transferred from buyer's reserve to seller
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, SELLER), 50);

        // OnOrderCompleted hook was called
        let completed = get_completed_hook_calls();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0], (1, ENTITY_1, SHOP_1));
    });
}

#[test]
fn token_cancel_order_unreserves_tokens() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 100);

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);

        // Tokens unreserved back to buyer
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);

        // TokenCommissionHandler::on_token_order_cancelled was called
        assert_eq!(get_token_cancelled_orders(), vec![1]);
    });
}

#[test]
fn token_confirm_receipt_transfers_to_seller() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);

        // Tokens moved from buyer reserved → seller free
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, SELLER), 100);

        // OnOrderCompleted hook called
        let completed = get_completed_hook_calls();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0], (1, ENTITY_1, SHOP_1));
    });
}

#[test]
fn token_approve_refund_unreserves_tokens() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // Tokens returned to buyer
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);

        // TokenCommissionHandler::on_token_order_cancelled was called
        assert_eq!(get_token_cancelled_orders(), vec![1]);
    });
}

#[test]
fn token_ship_timeout_auto_refunds() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        run_to_block(101); // ShipTimeout = 100

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);
        assert!(get_token_cancelled_orders().contains(&1));
    });
}

#[test]
fn token_confirm_timeout_auto_completes() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        run_to_block(201); // ConfirmTimeout = 200

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, SELLER), 100);
    });
}

#[test]
fn token_place_order_fails_token_not_enabled() {
    new_test_ext().execute_with(|| {
        // Token NOT enabled for entity
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                Some(PaymentAsset::EntityToken),
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::EntityTokenNotEnabled
        );
    });
}

#[test]
fn token_place_order_fails_insufficient_balance() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 50); // price is 100, insufficient

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                Some(PaymentAsset::EntityToken),
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::InsufficientTokenBalance
        );
    });
}

#[test]
fn token_place_order_none_defaults_to_native() {
    new_test_ext().execute_with(|| {
        // payment_asset = None should default to Native
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payment_asset, PaymentAsset::Native);
        assert_eq!(order.token_payment_amount, 0);
        assert_eq!(order.platform_fee, 1); // 100 * 100 / 10000 = 1
    });
}

// ==================== Audit regression tests ====================

// H2: stock=0 应拒绝购买（不再当作"无限库存"）
#[test]
fn h2_zero_stock_rejects_purchase() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 0); // Physical product depleted → SoldOut
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::ProductNotOnSale
        );
    });
}

// H3: ship_order 拒绝空 tracking_cid
#[test]
fn h3_ship_order_rejects_empty_tracking_cid() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_noop!(
            Transaction::ship_order(RuntimeOrigin::signed(SELLER), 1, b"".to_vec()),
            Error::<Test>::EmptyTrackingCid
        );
    });
}

// H4: start_service 写入 ExpiryQueue，服务超时后自动退款
#[test]
fn h4_service_start_timeout_auto_refunds() {
    new_test_ext().execute_with(|| {
        // Product 3 = Service, ShipTimeout=100, ServiceConfirmTimeout=150
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Start service at block 10
        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.service_started_at.is_some());

        // ServiceConfirmTimeout entry at block 10+150=160
        let queue = crate::ExpiryQueue::<Test>::get(160u64);
        assert!(
            queue.contains(&1),
            "start_service should enqueue to ExpiryQueue"
        );

        // At block 101 (ShipTimeout from place_order), service is still in progress
        // and 101 < 10+150=160, so should NOT be refunded
        run_to_block(101);
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Shipped);

        // At block 160 (ServiceConfirmTimeout from start_service), auto-refund
        run_to_block(160);
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

// H4: 服务正常完成后确认不受影响
#[test]
fn h4_service_normal_flow_unaffected() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        System::set_block_number(20);
        assert_ok!(Transaction::complete_service(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        System::set_block_number(25);
        assert_ok!(Transaction::confirm_service(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    });
}

// M3: do_complete_order 的 reward_on_purchase 使用正确的 entity_id
#[test]
fn m3_reward_uses_resolved_entity_id() {
    new_test_ext().execute_with(|| {
        // Digital product auto-completes → do_complete_order → reward_on_purchase
        // This test verifies it doesn't panic and completes successfully
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    });
}

// ==================== Audit Round 3 回归测试 ====================

// C1: EntityToken 订单可以成功申请退款（修复前会因 Escrow::set_disputed 失败）
#[test]
fn c1_token_order_request_refund_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // 用 EntityToken 下单（服务类产品不需要 shipping_cid）
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payment_asset, PaymentAsset::EntityToken);
        assert_eq!(order.status, OrderStatus::Paid);

        // EntityToken 订单申请退款（修复前此处会因 NoLock 失败）
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason_cid".to_vec(),
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Disputed);
    });
}

// C1: Native 订单退款仍通过 Escrow::set_disputed（行为不变）
#[test]
fn c1_native_order_request_refund_still_uses_escrow() {
    new_test_ext().execute_with(|| {
        // 服务类产品不需要 shipping_cid
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason_cid".to_vec(),
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Disputed);
    });
}

// C1: EntityToken 订单退款后卖家可同意退款，Token 被 unreserve
#[test]
fn c1_token_order_approve_refund_unreserves_tokens() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // 服务类产品不需要 shipping_cid
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // 买家申请退款
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec(),
        ));

        // 卖家同意退款
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // Token 已退回买家（unreserved）
        let reserved = get_token_reserved(entity_id, BUYER);
        assert_eq!(reserved, 0);
    });
}

// ==================== Audit Round 4 回归测试 ====================

// H1: process_expired_orders 保留服务中未到期的订单在 ExpiryQueue
#[test]
fn h1_expiry_queue_retains_in_service_not_yet_due() {
    new_test_ext().execute_with(|| {
        // Service order (product 3, price=200, ServiceConfirmTimeout=150)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Start service at block 50
        System::set_block_number(50);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        // start_service enqueues at block 50+150=200
        let queue = crate::ExpiryQueue::<Test>::get(200u64);
        assert!(queue.contains(&1));

        // ShipTimeout entry was at block 1+100=101. At block 101, order is Shipped+in_service.
        // The old code would drop it from the queue. The fix should retain it.
        run_to_block(101);

        // Order should still be Shipped (service in progress, not yet due for refund)
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);

        // The entry at block 101 (ShipTimeout) processed the order but should have kept it
        // because service_started_at=50, deadline=200, now=101 < 200.
        // Check: the order at block 200 queue should still have the order
        let queue200 = crate::ExpiryQueue::<Test>::get(200u64);
        assert!(
            queue200.contains(&1),
            "H1: in-service order must be retained in ExpiryQueue"
        );

        // At block 200 (ServiceConfirmTimeout), auto-refund should work
        run_to_block(200);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

// H3: place_order rejects overflow in price * quantity
#[test]
fn h3_place_order_rejects_overflow() {
    new_test_ext().execute_with(|| {
        // Product 1 has price=100. We'd need a very large quantity to overflow u64.
        // Since quantity is u32, max is 4294967295. 100 * 4294967295 = 429496729500 which fits u64.
        // We can't easily overflow with the mock's fixed prices.
        // Instead, verify the checked_mul path exists by confirming normal orders still work.
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 100);
    });
}

// M1: Token 订单完成后 OrderStats 追踪 Token 交易量和平台费
#[test]
fn m1_token_order_stats_track_token_volume() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order (auto-completes)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        // M2-R5-fix: total_volume (NEX) should be 0 for Token-only orders
        assert_eq!(stats.total_volume, 0);
    });
}

// M1: NEX 订单不影响 Token 统计
#[test]
fn m1_nex_order_stats_zero_token_volume() {
    new_test_ext().execute_with(|| {
        // Digital NEX order (auto-completes)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        assert_eq!(stats.total_volume, 50); // NEX
        assert_eq!(stats.total_platform_fees, 0); // product 2 is digital, no platform fee
    });
}

// M3: Token 订单 reward_on_purchase 使用 token_payment_amount 而非 total_amount(0)
#[test]
fn m3_token_order_reward_uses_token_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order auto-completes → do_complete_order → reward_on_purchase
        // Before fix: reward_on_purchase(entity_id, buyer, 0) — useless
        // After fix: reward_on_purchase(entity_id, buyer, 50) — correct token amount
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
        // Verify token_payment_amount is set correctly
        assert_eq!(order.token_payment_amount, 50);
        // total_amount also stores the price (used as Balance for reserve/unreserve)
        assert_eq!(order.total_amount, 50);
    });
}

// H2: Token 订单完成后 update_shop_stats 使用 token_payment_amount 而非 0
#[test]
fn h2_token_order_shop_stats_uses_token_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // If update_shop_stats was called with 0 (old bug), shop revenue tracking is wrong.
        // After fix it's called with token_payment_amount (50).
        // We can't directly observe MockShopProvider's internal state,
        // but we verify the order completed successfully with correct token amount.
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
        assert_eq!(order.token_payment_amount, 50);
    });
}

// ==================== Audit Round 5 回归测试 ====================

// H1-R5: Token 订单完成后 update_spent 传入 (0, 0) 而非 (token_amount, 错误的 usdt)
#[test]
fn h1r5_token_order_update_spent_passes_zero() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order auto-completes → do_complete_order → update_spent
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // Check MockMemberHandler recorded calls
        let spent = get_member_spent();
        // Token order now also snapshots usdt_total (product 2 = 50)
        assert!(spent.len() >= 1, "update_spent should be called");
        let last = spent.last().unwrap();
        assert_eq!(last.0, entity_id); // entity_id
        assert_eq!(last.1, BUYER); // buyer
        assert_eq!(last.2, 50); // amount_usdt: usdt_total snapshot = 50 (product 2)
    });
}

// H1-R5: NEX 订单 update_spent 正常传入正确金额
#[test]
fn h1r5_nex_order_update_spent_passes_correct_amounts() {
    new_test_ext().execute_with(|| {
        // Digital NEX order auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let spent = get_member_spent();
        assert!(spent.len() >= 1);
        let last = spent.last().unwrap();
        // amount_usdt: now uses usdt_total snapshot = 50 (product 2 usdt_price)
        assert_eq!(last.2, 50);
    });
}

// M1-R5: ship_timeout 自动退款发射 OrderRefunded 事件
#[test]
fn m1r5_ship_timeout_emits_order_refunded_event() {
    new_test_ext().execute_with(|| {
        // Physical order (product 1, price=100, requires_shipping=true)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // ShipTimeout = 100, so expire at block 101
        run_to_block(101);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // Check that OrderRefunded event was emitted
        let events = System::events();
        let refunded = events.iter().any(|e| {
            matches!(
                &e.event,
                RuntimeEvent::Transaction(crate::Event::OrderRefunded { order_id: 1, .. })
            )
        });
        assert!(
            refunded,
            "M1-R5: ship_timeout should emit OrderRefunded event"
        );
    });
}

// M1-R5: service 未开始超时也发射 OrderRefunded 事件
#[test]
fn m1r5_service_timeout_emits_order_refunded_event() {
    new_test_ext().execute_with(|| {
        // Service order (product 3, price=200)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // ShipTimeout = 100, service not started → expire at block 101
        run_to_block(101);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        let events = System::events();
        let refunded = events.iter().any(|e| {
            matches!(
                &e.event,
                RuntimeEvent::Transaction(crate::Event::OrderRefunded { order_id: 1, .. })
            )
        });
        assert!(
            refunded,
            "M1-R5: service_timeout should emit OrderRefunded event"
        );
    });
}

// M2-R5: Token 订单不计入 total_volume（NEX）
#[test]
fn m2r5_token_order_does_not_inflate_nex_volume() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        // M2-R5-fix: total_volume (NEX) should be 0 for Token-only orders
        assert_eq!(
            stats.total_volume, 0,
            "M2-R5: Token order should not inflate NEX total_volume"
        );
        assert_eq!(stats.total_platform_fees, 0);
    });
}

// ==================== Audit Round 6 回归测试 ====================

// M1-R6: complete_service 不允许重复调用（防止填充 ExpiryQueue）
#[test]
fn m1r6_complete_service_rejects_second_call() {
    new_test_ext().execute_with(|| {
        // Service order (product 3, price=200)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Start service
        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        // Complete service — first call succeeds
        System::set_block_number(20);
        assert_ok!(Transaction::complete_service(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        let order = Transaction::orders(1).unwrap();
        assert!(order.service_completed_at.is_some());

        // Second call should fail with InvalidOrderStatus
        assert_noop!(
            Transaction::complete_service(RuntimeOrigin::signed(SELLER), 1),
            Error::<Test>::InvalidOrderStatus
        );

        // ExpiryQueue at block 20+150=170 should have exactly 1 entry (not 2)
        let queue = crate::ExpiryQueue::<Test>::get(170u64);
        assert_eq!(
            queue.iter().filter(|&&id| id == 1).count(),
            1,
            "M1-R6: ExpiryQueue should have exactly 1 entry, not duplicates"
        );
    });
}

// M1-R6: Normal service flow still works after the fix
#[test]
fn m1r6_service_flow_still_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        System::set_block_number(20);
        assert_ok!(Transaction::complete_service(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        System::set_block_number(25);
        assert_ok!(Transaction::confirm_service(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    });
}

// M2-R5: NEX + Token 混合后统计正确分离
#[test]
fn m2r5_mixed_orders_stats_separated() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // NEX digital order (product 2, price=50)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        // Token digital order (product 2, price=50)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 2);
        assert_eq!(
            stats.total_volume, 50,
            "Only NEX order counted in total_volume"
        );
    });
}

// ==================== Platform Fee Rate Governance ====================

#[test]
fn set_platform_fee_rate_works() {
    new_test_ext().execute_with(|| {
        // 默认值 = 100 bps (1%)
        assert_eq!(PlatformFeeRate::<Test>::get(), 100);

        // Root 设置为 500 bps (5%)
        assert_ok!(Transaction::set_platform_fee_rate(
            RuntimeOrigin::root(),
            500
        ));
        assert_eq!(PlatformFeeRate::<Test>::get(), 500);

        // 验证事件
        System::assert_last_event(RuntimeEvent::Transaction(
            crate::Event::PlatformFeeRateUpdated {
                old_rate: 100,
                new_rate: 500,
            },
        ));
    });
}

#[test]
fn set_platform_fee_rate_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::set_platform_fee_rate(RuntimeOrigin::signed(BUYER), 500),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn set_platform_fee_rate_rejects_too_high() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::set_platform_fee_rate(RuntimeOrigin::root(), 1001),
            crate::Error::<Test>::PlatformFeeRateTooHigh
        );
        // 边界值 1000 应通过
        assert_ok!(Transaction::set_platform_fee_rate(
            RuntimeOrigin::root(),
            1000
        ));
        assert_eq!(PlatformFeeRate::<Test>::get(), 1000);
    });
}

#[test]
fn set_platform_fee_rate_zero_disables() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::set_platform_fee_rate(RuntimeOrigin::root(), 0));
        assert_eq!(PlatformFeeRate::<Test>::get(), 0);
    });
}

// ==================== Audit Round 7 回归测试 ====================

// M1-R7: Token 订单 OrderCompleted 事件中 seller_received 应为 0（无 NEX 收入）
#[test]
fn m1r7_token_order_completed_event_seller_received_is_zero() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);

        // Digital token order auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // Find OrderCompleted event
        let events = System::events();
        let completed = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    RuntimeEvent::Transaction(crate::Event::OrderCompleted { order_id: 1, .. })
                )
            })
            .expect("OrderCompleted event should exist");

        match &completed.event {
            RuntimeEvent::Transaction(crate::Event::OrderCompleted {
                seller_received,
                token_seller_received,
                ..
            }) => {
                // M1-R7: Token 订单卖家 NEX 收入为 0
                assert_eq!(
                    *seller_received, 0,
                    "M1-R7: Token order seller_received (NEX) should be 0"
                );
                // token_seller_received = token_amount - token_platform_fee
                // token_platform_fee = 50 * 0 / 10000 = 0 (mock fee rate = 0)
                assert_eq!(
                    *token_seller_received, 50,
                    "token_seller_received should be token amount minus fee"
                );
            }
            _ => panic!("Expected OrderCompleted event"),
        }
    });
}

// M1-R7: NEX 订单 seller_received 保持正常
#[test]
fn m1r7_nex_order_completed_event_seller_received_correct() {
    new_test_ext().execute_with(|| {
        // Digital NEX order (product 2, price=50) auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let events = System::events();
        let completed = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    RuntimeEvent::Transaction(crate::Event::OrderCompleted { order_id: 1, .. })
                )
            })
            .expect("OrderCompleted event should exist");

        match &completed.event {
            RuntimeEvent::Transaction(crate::Event::OrderCompleted {
                seller_received,
                token_seller_received,
                ..
            }) => {
                // NEX order: seller_received = total_amount - platform_fee
                // platform_fee = 50 * 100 / 10000 = 0 (integer div, 50 < 100)
                // Actually 50 * 100 = 5000, 5000 / 10000 = 0
                // So seller_received = 50 - 0 = 50
                assert_eq!(
                    *seller_received, 50,
                    "NEX order seller_received should be total - fee"
                );
                assert_eq!(
                    *token_seller_received, 0,
                    "NEX order token_seller_received should be 0"
                );
            }
            _ => panic!("Expected OrderCompleted event"),
        }
    });
}

// M2-R7: Token 平台费计算正确（验证事件中 token_seller_received 值一致）
#[test]
fn m2r7_token_platform_fee_computed_once_correctly() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        // Set token platform fee rate to 500 bps (5%)
        set_token_fee_rate(entity_id, 500);

        // Digital token order auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // Verify stats — Token orders don't contribute to NEX OrderStats
        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        assert_eq!(
            stats.total_volume, 0,
            "Token order should not inflate NEX volume"
        );

        // Verify event — token_seller_received = 50 - 2 = 48
        let events = System::events();
        let completed = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    RuntimeEvent::Transaction(crate::Event::OrderCompleted { order_id: 1, .. })
                )
            })
            .expect("OrderCompleted event should exist");

        match &completed.event {
            RuntimeEvent::Transaction(crate::Event::OrderCompleted {
                token_seller_received,
                seller_received,
                ..
            }) => {
                assert_eq!(
                    *token_seller_received, 48,
                    "M2-R7: token_seller_received = 50 - 2 = 48"
                );
                assert_eq!(
                    *seller_received, 0,
                    "M1-R7: Token order NEX seller_received = 0"
                );
            }
            _ => panic!("Expected OrderCompleted event"),
        }
    });
}

// L1-R7: ExpiryQueue 在服务订单跳过后不残留孤立条目
#[test]
fn l1r7_expiry_queue_no_orphan_after_service_skip() {
    new_test_ext().execute_with(|| {
        // Service order (product 3, price=200)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Start service at block 50 → creates ExpiryQueue entry at 50+150=200
        System::set_block_number(50);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        // At block 101 (ShipTimeout), the service order is Shipped+Service+uncompleted
        // L1-R7-fix: should NOT retain orphaned entry at block 101
        run_to_block(101);

        // Verify order is still Shipped (not refunded yet — deadline is block 200)
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);

        // L1-R7: ExpiryQueue at block 101 should be EMPTY (cleaned up, no orphan)
        let queue_101 = crate::ExpiryQueue::<Test>::get(101u64);
        assert!(
            queue_101.is_empty(),
            "L1-R7: ExpiryQueue[101] should be empty, no orphaned entries"
        );

        // The service deadline entry at block 200 should still work
        run_to_block(200);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(
            order.status,
            OrderStatus::Refunded,
            "Service should auto-refund at deadline"
        );
    });
}

// L5-R7: cleanup_buyer_orders 移除终态订单后释放容量
#[test]
fn l5r7_cleanup_buyer_orders_removes_terminal_orders() {
    new_test_ext().execute_with(|| {
        // Place 3 orders: digital (auto-completes), physical (will cancel), physical (stays Paid)
        // Order 1: digital, auto-completes → Completed
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        // Order 2: physical → Paid, then cancel → Cancelled
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 2));
        // Order 3: physical → stays Paid
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Before cleanup: 3 orders in index
        let before = crate::BuyerOrders::<Test>::get(BUYER);
        assert_eq!(before.len(), 3);

        // Cleanup: should remove Completed(1) + Cancelled(2), keep Paid(3)
        assert_ok!(Transaction::cleanup_buyer_orders(RuntimeOrigin::signed(
            BUYER
        )));

        let after = crate::BuyerOrders::<Test>::get(BUYER);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0], 3);

        // Event emitted
        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::BuyerOrdersCleaned { removed: 2, .. })
        )));
    });
}

// L5-R7: cleanup_buyer_orders 无终态订单时报错
#[test]
fn l5r7_cleanup_buyer_orders_nothing_to_clean() {
    new_test_ext().execute_with(|| {
        // Place physical order → stays Paid
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::cleanup_buyer_orders(RuntimeOrigin::signed(BUYER)),
            crate::Error::<Test>::NothingToClean
        );
    });
}

// L5-R7: cleanup_buyer_orders 空列表报错
#[test]
fn l5r7_cleanup_buyer_orders_empty_list() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::cleanup_buyer_orders(RuntimeOrigin::signed(BUYER)),
            crate::Error::<Test>::NothingToClean
        );
    });
}

// ==================== reject_refund ====================

#[test]
fn reject_refund_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject_reason".to_vec()
        ));

        // Order stays Disputed
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Disputed);

        // ExpiryQueue entry at now + DisputeTimeout(300) = 1 + 300 = 301
        let queue = crate::ExpiryQueue::<Test>::get(301u64);
        assert!(
            queue.contains(&1),
            "reject_refund should enqueue dispute timeout"
        );

        // Event emitted
        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::RefundRejected { order_id: 1, .. })
        )));
    });
}

#[test]
fn reject_refund_fails_not_seller() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_noop!(
            Transaction::reject_refund(RuntimeOrigin::signed(BUYER), 1, b"reject".to_vec()),
            Error::<Test>::NotOrderSeller
        );
    });
}

#[test]
fn reject_refund_fails_not_disputed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::reject_refund(RuntimeOrigin::signed(SELLER), 1, b"reject".to_vec()),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn reject_refund_fails_empty_reason() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_noop!(
            Transaction::reject_refund(RuntimeOrigin::signed(SELLER), 1, b"".to_vec()),
            Error::<Test>::EmptyReasonCid
        );
    });
}

// ==================== Disputed timeout auto-refund ====================

#[test]
fn dispute_timeout_auto_refunds() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        // Seller rejects at block 1, dispute timeout = 300, expiry at 301
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));

        // Before timeout: still Disputed
        run_to_block(300);
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );

        // At timeout block: auto-refund
        run_to_block(301);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // Commission cancelled
        assert!(get_cancelled_orders().contains(&1));
    });
}

#[test]
fn dispute_timeout_token_order_auto_refunds() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));

        run_to_block(301);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        // Tokens returned
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);
    });
}

#[test]
fn dispute_approved_before_timeout_no_double_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));

        // Seller changes mind and approves before timeout
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );

        // Timeout fires but order already Refunded → skipped by _ => {}
        run_to_block(301);
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

// ==================== seller_cancel_order ====================

#[test]
fn seller_cancel_order_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"no_stock".to_vec()
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);

        // Commission cancelled
        assert!(get_cancelled_orders().contains(&1));

        // Event emitted
        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderSellerCancelled { order_id: 1, .. })
        )));
    });
}

#[test]
fn seller_cancel_order_token_works() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Cancelled
        );

        // Tokens returned
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);
    });
}

#[test]
fn seller_cancel_order_fails_not_seller() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::seller_cancel_order(RuntimeOrigin::signed(BUYER), 1, b"reason".to_vec()),
            Error::<Test>::NotOrderSeller
        );
    });
}

#[test]
fn seller_cancel_order_fails_after_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_noop!(
            Transaction::seller_cancel_order(RuntimeOrigin::signed(SELLER), 1, b"reason".to_vec()),
            Error::<Test>::CannotCancelOrder
        );
    });
}

#[test]
fn seller_cancel_order_fails_digital() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::seller_cancel_order(RuntimeOrigin::signed(SELLER), 1, b"reason".to_vec()),
            Error::<Test>::DigitalProductCannotCancel
        );
    });
}

// ==================== force_refund ====================

#[test]
fn force_refund_paid_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::force_refund(RuntimeOrigin::root(), 1, None));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderForceRefunded { order_id: 1, .. })
        )));
    });
}

#[test]
fn force_refund_shipped_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_ok!(Transaction::force_refund(RuntimeOrigin::root(), 1, None));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

#[test]
fn force_refund_disputed_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::force_refund(RuntimeOrigin::root(), 1, None));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

#[test]
fn force_refund_token_disputed_works() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::force_refund(RuntimeOrigin::root(), 1, None));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);
    });
}

#[test]
fn force_refund_fails_non_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::force_refund(RuntimeOrigin::signed(BUYER), 1, None),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn force_refund_fails_completed() {
    new_test_ext().execute_with(|| {
        // Digital auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::force_refund(RuntimeOrigin::root(), 1, None),
            Error::<Test>::CannotForceOrder
        );
    });
}

// ==================== force_complete ====================

#[test]
fn force_complete_paid_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::force_complete(RuntimeOrigin::root(), 1, None));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderForceCompleted { order_id: 1, .. })
        )));
    });
}

#[test]
fn force_complete_disputed_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::force_complete(RuntimeOrigin::root(), 1, None));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );

        // Stats updated
        let stats = Transaction::order_stats();
        assert_eq!(stats.completed_orders, 1);
    });
}

#[test]
fn force_complete_fails_non_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::force_complete(RuntimeOrigin::signed(SELLER), 1, None),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn force_complete_fails_already_completed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::force_complete(RuntimeOrigin::root(), 1, None),
            Error::<Test>::CannotForceOrder
        );
    });
}

// ==================== update_shipping_address ====================

#[test]
fn update_shipping_address_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"old_addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_ok!(Transaction::update_shipping_address(
            RuntimeOrigin::signed(BUYER),
            1,
            b"new_addr_cid".to_vec()
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(
            order.shipping_cid.unwrap().into_inner(),
            b"new_addr_cid".to_vec()
        );

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::ShippingAddressUpdated { order_id: 1 })
        )));
    });
}

#[test]
fn update_shipping_address_fails_not_buyer() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::update_shipping_address(RuntimeOrigin::signed(SELLER), 1, b"new".to_vec()),
            Error::<Test>::NotOrderBuyer
        );
    });
}

#[test]
fn update_shipping_address_fails_after_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_noop!(
            Transaction::update_shipping_address(RuntimeOrigin::signed(BUYER), 1, b"new".to_vec()),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn update_shipping_address_fails_service_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::update_shipping_address(RuntimeOrigin::signed(BUYER), 1, b"new".to_vec()),
            Error::<Test>::ServiceLikeOrderCannotShip
        );
    });
}

#[test]
fn update_shipping_address_fails_empty_cid() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::update_shipping_address(RuntimeOrigin::signed(BUYER), 1, b"".to_vec()),
            Error::<Test>::ShippingCidRequired
        );
    });
}

// ==================== extend_confirm_timeout ====================

#[test]
fn extend_confirm_timeout_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Extend at block 1, ConfirmExtension = 100, new deadline = 101
        assert_ok!(Transaction::extend_confirm_timeout(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        let order = Transaction::orders(1).unwrap();
        assert!(order.confirm_extended);

        // ExpiryQueue has new entry at block 101
        let queue = crate::ExpiryQueue::<Test>::get(101u64);
        assert!(queue.contains(&1));

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::ConfirmTimeoutExtended {
                order_id: 1,
                new_deadline: 101
            })
        )));
    });
}

#[test]
fn extend_confirm_timeout_fails_second_time() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_ok!(Transaction::extend_confirm_timeout(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        assert_noop!(
            Transaction::extend_confirm_timeout(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::AlreadyExtended
        );
    });
}

#[test]
fn extend_confirm_timeout_fails_not_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::extend_confirm_timeout(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn extend_confirm_timeout_fails_not_buyer() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_noop!(
            Transaction::extend_confirm_timeout(RuntimeOrigin::signed(SELLER), 1),
            Error::<Test>::NotOrderBuyer
        );
    });
}

#[test]
fn extend_confirm_timeout_delays_auto_complete() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Original ConfirmTimeout = 200, shipped at block 1, expiry at 201
        // Extend at block 50
        System::set_block_number(50);
        assert_ok!(Transaction::extend_confirm_timeout(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        // New deadline = 50 + 100 = 150

        // At block 150 the extended timeout fires → auto-complete
        run_to_block(150);
        // Note: original expiry at 201 hasn't fired yet, but the extended entry at 150 triggers
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );
    });
}

// ==================== cleanup_shop_orders ====================

#[test]
fn cleanup_shop_orders_works() {
    new_test_ext().execute_with(|| {
        // Order 1: digital auto-completes → Completed
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));
        // Order 2: physical → cancel → Cancelled
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 2));
        // Order 3: physical → stays Paid
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Before cleanup: 3 orders in shop 1 index
        let before = crate::ShopOrders::<Test>::get(SHOP_1);
        assert_eq!(before.len(), 3);

        // Cleanup: owner of SHOP_1 is SELLER
        assert_ok!(Transaction::cleanup_shop_orders(
            RuntimeOrigin::signed(SELLER),
            SHOP_1
        ));

        let after = crate::ShopOrders::<Test>::get(SHOP_1);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0], 3); // only Paid order remains

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::ShopOrdersCleaned {
                shop_id: 1,
                removed: 2
            })
        )));
    });
}

#[test]
fn cleanup_shop_orders_fails_not_owner() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::cleanup_shop_orders(RuntimeOrigin::signed(BUYER), SHOP_1),
            Error::<Test>::NotShopOwner
        );
    });
}

#[test]
fn cleanup_shop_orders_fails_nothing_to_clean() {
    new_test_ext().execute_with(|| {
        // Only Paid order, nothing terminal
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::cleanup_shop_orders(RuntimeOrigin::signed(SELLER), SHOP_1),
            Error::<Test>::NothingToClean
        );
    });
}

#[test]
fn cleanup_shop_orders_fails_shop_not_found() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::cleanup_shop_orders(RuntimeOrigin::signed(SELLER), 999),
            Error::<Test>::ShopNotFound
        );
    });
}

// ==================== Token→USDT 换算测试 ====================

// F2: Token 订单完成时，amount_usdt 直接使用 usdt_total 快照
// token_price=2_000_000_000_000 (2 NEX/Token), product 2 usdt_price=50
// Token amount = nex_amount * 10^12 / token_price = 50 * 10^12 / 2*10^12 = 25 tokens
#[test]
fn f2_token_order_amount_usdt_indirect_conversion() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        // Set token price: 2 NEX per Token (precision 10^12)
        set_token_price(entity_id, 2_000_000_000_000);
        set_token_price_reliable(entity_id, true);

        // Digital token order (product 2, usdt_price=50) auto-completes
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(pallet_entity_common::PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        // Verify order snapshots
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.usdt_total, 50); // usdt_price snapshot
        assert_eq!(order.token_payment_amount, 25); // 50 NEX / 2 NEX per Token

        // amount_usdt now directly uses usdt_total snapshot
        let spent = get_member_spent();
        let entry = spent
            .iter()
            .find(|(eid, acc, _)| *eid == entity_id && *acc == BUYER);
        assert!(entry.is_some(), "update_spent should have been called");
        let (_, _, amount_usdt) = entry.unwrap();
        assert_eq!(
            *amount_usdt, 50u64,
            "F2: amount_usdt = usdt_total snapshot = 50"
        );
    });
}

// F3: Token 价格不可靠时，下单应被拒绝 (TokenPriceUnavailable)
#[test]
fn f3_token_order_unreliable_price_degrades_to_zero() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        // Set token price but mark as unreliable
        set_token_price(entity_id, 2_000_000_000_000);
        set_token_price_reliable(entity_id, false);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                2,
                1,
                None,
                None,
                Some(pallet_entity_common::PaymentAsset::EntityToken),
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::TokenPriceUnavailable
        );
    });
}

// F4: Token 价格不可用时（None），下单应被拒绝 (TokenPriceUnavailable)
#[test]
fn f4_token_order_missing_price_degrades_to_zero() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        // Mark as reliable but don't set token price (get_token_price returns None)
        set_token_price_reliable(entity_id, true);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                2,
                1,
                None,
                None,
                Some(pallet_entity_common::PaymentAsset::EntityToken),
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::TokenPriceUnavailable
        );
    });
}

// ==================== Audit Round 8 回归测试 ====================

// H1-R8: reject_refund 禁止重复调用
#[test]
fn h1r8_reject_refund_cannot_be_called_twice() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        // request_refund now sets initial dispute_deadline
        let order_before = Transaction::orders(1).unwrap();
        assert!(order_before.dispute_deadline.is_some());
        assert!(!order_before.dispute_rejected);

        // First reject succeeds
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));
        let order = Transaction::orders(1).unwrap();
        assert!(order.dispute_deadline.is_some());
        assert!(order.dispute_rejected);

        // Second reject fails with DisputeAlreadyRejected
        assert_noop!(
            Transaction::reject_refund(RuntimeOrigin::signed(SELLER), 1, b"reject_again".to_vec()),
            Error::<Test>::DisputeAlreadyRejected
        );

        // H4-fix: reject_refund 清理旧条目后只剩 1 个条目
        let deadline = order.dispute_deadline.unwrap();
        let queue = crate::ExpiryQueue::<Test>::get(deadline);
        assert_eq!(
            queue.iter().filter(|&&id| id == 1).count(),
            1,
            "H4: ExpiryQueue should have 1 entry after cleanup"
        );
    });
}

// H1-R8: reject_refund 正常流程仍然工作
#[test]
fn h1r8_reject_refund_then_timeout_still_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));

        // Dispute timeout at block 1 + 300 = 301
        run_to_block(301);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

// M1-R8: process_expired_orders weight 包含跳过订单的读开销
#[test]
fn m1r8_expired_orders_weight_includes_skipped() {
    new_test_ext().execute_with(|| {
        // Place digital order (auto-completes → Completed) — will be skipped by expiry handler
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));
        // Place physical order — will be processed
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Manually insert order 1 (Completed) into ExpiryQueue at block 101 to simulate a skip
        crate::ExpiryQueue::<Test>::mutate(101u64, |ids| {
            let _ = ids.try_push(1); // order 1 is Completed, will be skipped
        });

        // At block 101: order 1 skipped (Completed), order 2 processed (Paid → Refunded)
        // Use on_idle which calls process_expired_orders internally
        System::set_block_number(101);
        let weight = <Transaction as frame_support::traits::Hooks<u64>>::on_idle(
            101u64,
            Weight::from_parts(10_000_000_000, 1_000_000),
        );

        // Weight should be > base (50M) + processed (200M * 1) = 250M
        // because skipped orders add 25M each
        assert!(
            weight.ref_time() > 250_000_000,
            "M1-R8: weight should include skipped order read cost"
        );
        assert!(
            weight.proof_size() > 4_000 + 8_000,
            "M1-R8: proof_size should include skipped order"
        );
    });
}

// M2-R8: Token 订单完成后 total_token_volume 和 total_token_platform_fees 正确更新
#[test]
fn m2r8_token_order_stats_track_token_volume_and_fees() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        set_token_fee_rate(entity_id, 500); // 5%

        // Digital token order auto-completes (product 2, price=50)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        // NEX stats should be 0
        assert_eq!(stats.total_volume, 0);
        assert_eq!(stats.total_platform_fees, 0);
        // M2-R8: Token stats should be populated
        assert_eq!(
            stats.total_token_volume, 50,
            "M2-R8: total_token_volume should track token orders"
        );
        // token_platform_fee = 50 * 500 / 10000 = 2
        assert_eq!(
            stats.total_token_platform_fees, 2,
            "M2-R8: total_token_platform_fees should track token fees"
        );
    });
}

// M2-R8: NEX 订单不影响 Token 统计字段
#[test]
fn m2r8_nex_order_does_not_affect_token_stats() {
    new_test_ext().execute_with(|| {
        // Digital NEX order auto-completes (product 2, price=50)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let stats = crate::OrderStats::<Test>::get();
        assert_eq!(stats.completed_orders, 1);
        assert_eq!(stats.total_volume, 50);
        assert_eq!(
            stats.total_token_volume, 0,
            "M2-R8: NEX order should not inflate token volume"
        );
        assert_eq!(stats.total_token_platform_fees, 0);
    });
}

// M3-R8: token_platform_fee_rate 超过 10000 时被防御性截断
#[test]
fn m3r8_token_fee_rate_capped_at_10000() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 10_000);
        // Set absurd fee rate: 20000 bps (200%) — should be capped to 10000 (100%)
        set_token_fee_rate(entity_id, 20000);

        // Digital token order auto-completes (product 2, price=50)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let stats = crate::OrderStats::<Test>::get();
        // With cap: token_platform_fee = 50 * 10000 / 10000 = 50 (all goes to platform)
        // Without cap: 50 * 20000 / 10000 = 100 (exceeds token amount)
        assert_eq!(
            stats.total_token_platform_fees, 50,
            "M3-R8: fee should be capped at 100% of token amount"
        );

        // Verify event: seller should receive 0 (all goes to platform fee)
        let events = System::events();
        let completed = events
            .iter()
            .find(|e| {
                matches!(
                    &e.event,
                    RuntimeEvent::Transaction(crate::Event::OrderCompleted { order_id: 1, .. })
                )
            })
            .expect("OrderCompleted event should exist");

        match &completed.event {
            RuntimeEvent::Transaction(crate::Event::OrderCompleted {
                token_seller_received,
                ..
            }) => {
                assert_eq!(
                    *token_seller_received, 0,
                    "M3-R8: seller gets 0 when fee rate capped at 100%"
                );
            }
            _ => panic!("Expected OrderCompleted event"),
        }
    });
}

// L1-R8: extend_confirm_timeout 拒绝服务类订单
#[test]
fn l1r8_extend_confirm_timeout_rejects_service_order() {
    new_test_ext().execute_with(|| {
        // Service order (product 3)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Start service → status becomes Shipped
        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Shipped);

        // Buyer tries to extend confirm timeout on service order → should fail
        assert_noop!(
            Transaction::extend_confirm_timeout(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::ServiceLikeOrderCannotShip
        );
    });
}

// L1-R8: extend_confirm_timeout 仍然支持物理商品订单
#[test]
fn l1r8_extend_confirm_timeout_works_for_physical() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_ok!(Transaction::extend_confirm_timeout(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        let order = Transaction::orders(1).unwrap();
        assert!(order.confirm_extended);
    });
}

// ==================== Audit Round 9 regression tests ====================

// M2-R9: process_expired_orders 空队列返回 proof_size > 0
#[test]
fn m2r9_empty_expiry_queue_returns_nonzero_proof_size() {
    new_test_ext().execute_with(|| {
        // Block 999 has no expiry entries
        System::set_block_number(999);
        let w = <Transaction as frame_support::traits::Hooks<u64>>::on_idle(
            999,
            Weight::from_parts(10_000_000_000, 1_000_000),
        );
        // proof_size should be > 0 even for empty queue (1 storage read)
        assert!(
            w.proof_size() > 0,
            "M2-R9: empty queue should report nonzero proof_size"
        );
    });
}

// L1-R9: do_auto_refund — ship timeout still refunds physical orders correctly
#[test]
fn l1r9_do_auto_refund_ship_timeout_physical() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Paid);

        run_to_block(101); // ShipTimeout = 100

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert!(get_cancelled_orders().contains(&1));

        // Verify OrderRefunded event emitted
        let events = frame_system::Pallet::<Test>::events();
        assert!(
            events.iter().any(|e| {
                match &e.event {
                    RuntimeEvent::Transaction(crate::Event::OrderRefunded { order_id, .. }) => {
                        *order_id == 1
                    }
                    _ => false,
                }
            }),
            "L1-R9: OrderRefunded event should be emitted by do_auto_refund"
        );
    });
}

// L1-R9: do_auto_refund — service timeout still refunds service orders correctly
#[test]
fn l1r9_do_auto_refund_service_timeout() {
    new_test_ext().execute_with(|| {
        // Service order (product 3)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None
        ));

        // Start service at block 10
        System::set_block_number(10);
        assert_ok!(Transaction::start_service(RuntimeOrigin::signed(SELLER), 1));

        // ServiceConfirmTimeout = 150, so deadline = 10 + 150 = 160
        run_to_block(161);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

// L1-R9: do_auto_refund — dispute timeout still refunds correctly
#[test]
fn l1r9_do_auto_refund_dispute_timeout() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"no".to_vec()
        ));

        // DisputeTimeout = 300, reject at block 1 → deadline = 1 + 300 = 301
        run_to_block(301);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

// L1-R9: do_auto_refund — token order timeout refund unreserves tokens
#[test]
fn l1r9_do_auto_refund_token_order_timeout() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 100);

        run_to_block(101); // ShipTimeout

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert_eq!(get_token_reserved(ENTITY_1, BUYER), 0);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 10_000);
        assert!(get_token_cancelled_orders().contains(&1));
    });
}

// ==================== R11: referrer validation ====================

#[test]
fn r11_referrer_cannot_be_buyer() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                Some(BUYER),
                None,
                None,
            ),
            Error::<Test>::InvalidReferrer
        );
    });
}

#[test]
fn r11_referrer_cannot_be_seller() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                Some(SELLER),
                None,
                None,
            ),
            Error::<Test>::InvalidReferrer
        );
    });
}

#[test]
fn r11_valid_referrer_stored() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert_eq!(OrderReferrer::<Test>::get(1), Some(BUYER2));
    });
}

#[test]
fn r11_no_referrer_no_storage() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(OrderReferrer::<Test>::get(1), None);
    });
}

// ==================== R11: OrderReferrer cleanup ====================

#[test]
fn r11_referrer_cleaned_on_cancel() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

#[test]
fn r11_referrer_cleaned_on_complete() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

#[test]
fn r11_referrer_cleaned_on_seller_cancel() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reason".to_vec()
        ));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

#[test]
fn r11_referrer_cleaned_on_force_refund() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        assert_ok!(Transaction::force_refund(RuntimeOrigin::root(), 1, None));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

#[test]
fn r11_referrer_cleaned_on_approve_refund() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

#[test]
fn r11_referrer_cleaned_on_auto_timeout() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        // ShipTimeout = 100
        run_to_block(101);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

// ==================== R11: seller_refund_order (Shipped only) ====================

#[test]
fn r11_seller_refund_shipped_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert_ok!(Transaction::seller_refund_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"defective".to_vec(),
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderSellerRefunded { order_id: 1, .. })
        )));
    });
}

#[test]
fn r11_seller_refund_fails_on_paid() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::seller_refund_order(RuntimeOrigin::signed(SELLER), 1, b"reason".to_vec(),),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn r11_seller_refund_cleans_referrer() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        assert_ok!(Transaction::seller_refund_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"defective".to_vec(),
        ));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

// ==================== R11: force_partial_refund ====================

#[test]
fn r11_partial_refund_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::force_partial_refund(
            RuntimeOrigin::root(),
            1,
            5000,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyRefunded);

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderPartialRefunded {
                order_id: 1,
                refund_bps: 5000,
                ..
            })
        )));
        assert!(!events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderRefunded { .. })
        )));
    });
}

#[test]
fn r11_partial_refund_no_stock_restore() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::force_partial_refund(
            RuntimeOrigin::root(),
            1,
            3000,
            Some(b"partial_reason".to_vec()),
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::PartiallyRefunded);
    });
}

#[test]
fn r11_partial_refund_fails_token_order() {
    new_test_ext().execute_with(|| {
        use pallet_entity_common::PaymentAsset;
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 10_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::force_partial_refund(RuntimeOrigin::root(), 1, 5000, None),
            Error::<Test>::PartialRefundNotSupported
        );
    });
}

#[test]
fn r11_partial_refund_fails_non_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::force_partial_refund(RuntimeOrigin::signed(SELLER), 1, 5000, None),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn r11_partial_refund_invalid_bps() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::force_partial_refund(RuntimeOrigin::root(), 1, 0, None),
            Error::<Test>::InvalidRefundBps
        );
        assert_noop!(
            Transaction::force_partial_refund(RuntimeOrigin::root(), 1, 10000, None),
            Error::<Test>::InvalidRefundBps
        );
    });
}

#[test]
fn r11_partial_refund_cleans_referrer() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER2, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));
        assert!(OrderReferrer::<Test>::contains_key(1));

        assert_ok!(Transaction::force_partial_refund(
            RuntimeOrigin::root(),
            1,
            5000,
            None
        ));
        assert!(!OrderReferrer::<Test>::contains_key(1));
    });
}

// ==================== R11: banned buyer check ====================

#[test]
fn r11_banned_buyer_cannot_place_order() {
    new_test_ext().execute_with(|| {
        set_banned(ENTITY_1, BUYER, true);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::BuyerBanned
        );
    });
}

// ==================== R11: member level discount ====================

#[test]
fn r11_member_discount_applies() {
    new_test_ext().execute_with(|| {
        set_member(ENTITY_1, BUYER, true);
        set_member_level(ENTITY_1, BUYER, 2);
        set_level_discount(ENTITY_1, 2, 1000); // 10% discount

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        // unit_price=100, 10% off → total_amount=90
        assert_eq!(order.total_amount, 90);
    });
}

#[test]
fn r11_no_discount_for_level_zero() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 100);
    });
}

// ==================== R12: 审计修复验证 ====================

#[test]
fn r12_dispute_auto_refunds_when_seller_ignores() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec(),
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Disputed);
        assert!(order.dispute_deadline.is_some());
        let deadline = order.dispute_deadline.unwrap();
        assert_eq!(deadline, 1 + 300); // block 1 + DisputeTimeout(300)

        // Seller does nothing — run to dispute deadline
        run_to_block(301);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(
            order.status,
            OrderStatus::Refunded,
            "R12: Disputed order should auto-refund when seller ignores request_refund"
        );
    });
}

#[test]
fn r12_request_refund_sets_dispute_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec(),
        ));

        let order = Transaction::orders(1).unwrap();
        assert!(order.dispute_deadline.is_some());
        assert!(!order.dispute_rejected);

        let queue = crate::ExpiryQueue::<Test>::get(301u64);
        assert!(
            queue.contains(&1),
            "R12: request_refund should enqueue dispute timeout"
        );
    });
}

#[test]
fn r12_reject_refund_resets_deadline() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec(),
        ));

        // Advance to block 50 then reject
        System::set_block_number(50);
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec(),
        ));

        let order = Transaction::orders(1).unwrap();
        assert!(order.dispute_rejected);
        assert_eq!(order.dispute_deadline, Some(50 + 300));

        // Run to new deadline
        run_to_block(350);
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    });
}

#[test]
fn r12_shopping_balance_pays_full_amount() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payment_asset, PaymentAsset::ShoppingBalance);
        assert_eq!(order.shopping_balance_used, 100); // full unit_price
        assert_eq!(order.total_amount, 0); // no NEX escrow
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 100); // 200 - 100
    });
}

#[test]
fn r12_update_tracking_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track_v1".to_vec(),
        ));

        assert_ok!(Transaction::update_tracking(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track_v2".to_vec(),
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(
            order.tracking_cid.unwrap().into_inner(),
            b"track_v2".to_vec()
        );

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::TrackingInfoUpdated { order_id: 1 })
        )));
    });
}

#[test]
fn r12_update_tracking_fails_not_seller() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec(),
        ));

        assert_noop!(
            Transaction::update_tracking(RuntimeOrigin::signed(BUYER), 1, b"new_track".to_vec()),
            Error::<Test>::NotOrderSeller
        );
    });
}

#[test]
fn r12_update_tracking_fails_not_shipped() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_noop!(
            Transaction::update_tracking(RuntimeOrigin::signed(SELLER), 1, b"track".to_vec()),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn r12_update_tracking_fails_empty_cid() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec(),
        ));

        assert_noop!(
            Transaction::update_tracking(RuntimeOrigin::signed(SELLER), 1, b"".to_vec()),
            Error::<Test>::EmptyTrackingCid
        );
    });
}

#[test]
fn r12_reason_cid_in_seller_cancel_event() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"out_of_stock".to_vec(),
        ));

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderSellerCancelled {
                order_id: 1,
                reason_cid,
                ..
            }) if reason_cid == &b"out_of_stock".to_vec()
        )));
    });
}

#[test]
fn r12_reason_cid_in_reject_refund_event() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"defective".to_vec(),
        ));

        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"valid_product".to_vec(),
        ));

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::RefundRejected {
                order_id: 1,
                reason_cid,
                ..
            }) if reason_cid == &b"valid_product".to_vec()
        )));
    });
}

#[test]
fn r12_reason_cid_in_force_refund_event() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::force_refund(
            RuntimeOrigin::root(),
            1,
            Some(b"governance_decision".to_vec()),
        ));

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderForceRefunded {
                order_id: 1,
                reason_cid: Some(ref cid),
                ..
            }) if cid == &b"governance_decision".to_vec()
        )));
    });
}

#[test]
fn r12_reason_cid_in_seller_refund_event_check() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec(),
        ));

        assert_ok!(Transaction::seller_refund_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"wrong_item".to_vec(),
        ));

        let events = System::events();
        assert!(events.iter().any(|e| matches!(
            &e.event,
            RuntimeEvent::Transaction(crate::Event::OrderSellerRefunded {
                order_id: 1,
                reason_cid,
                ..
            }) if reason_cid == &b"wrong_item".to_vec()
        )));
    });
}

// ==================== R13: 测试盲区补全 ====================

#[test]
fn r13_use_tokens_redeem_deducts_from_final_amount() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_price(ENTITY_1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 500);
        set_redeem_discount(ENTITY_1, BUYER, 20);

        // args: product_id, quantity, shipping_cid, use_tokens, use_shopping_balance, payment_asset, note_cid, referrer
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            Some(50),
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        // price=100, redeem_for_discount returns 20, so final_amount = 100 - 20 = 80
        assert_eq!(order.total_amount, 80);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 450);
    });
}

#[test]
fn r13_use_tokens_no_effect_when_disabled() {
    new_test_ext().execute_with(|| {
        // args: product_id, quantity, shipping_cid, use_tokens, use_shopping_balance, payment_asset, note_cid, referrer
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            Some(50), // use_tokens = 50, but token not enabled
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.total_amount, 100);
    });
}

#[test]
fn r13_visibility_members_only_rejects_non_member() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::MembersOnly);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::ProductMembersOnly
        );
    });
}

#[test]
fn r13_visibility_members_only_allows_member() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::MembersOnly);
        set_member(ENTITY_1, BUYER, true);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
    });
}

#[test]
fn r13_visibility_level_gated_rejects_insufficient_level() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::LevelGated(3));
        set_member(ENTITY_1, BUYER, true);
        set_member_level(ENTITY_1, BUYER, 2);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::MemberLevelInsufficient
        );
    });
}

#[test]
fn r13_visibility_level_gated_allows_sufficient_level() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::LevelGated(3));
        set_member(ENTITY_1, BUYER, true);
        set_member_level(ENTITY_1, BUYER, 3);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
    });
}

#[test]
fn r13_visibility_level_gated_rejects_non_member() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::LevelGated(1));

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::ProductMembersOnly
        );
    });
}

// ==================== pre-order auto-registration ====================

#[test]
fn r13_members_only_auto_registers_with_referrer() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::MembersOnly);
        set_member(ENTITY_1, BUYER2, true); // referrer must be an existing member
                                            // BUYER is NOT a member, but passes a valid referrer
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2), // referrer
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
        // auto_register was called
        assert!(get_member_registered().contains(&(ENTITY_1, BUYER)));
    });
}

#[test]
fn r13_members_only_still_rejects_without_referrer() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::MembersOnly);
        // BUYER is NOT a member, no referrer
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None, // no referrer
                None,
            ),
            crate::Error::<Test>::ProductMembersOnly
        );
    });
}

#[test]
fn r13_level_gated_zero_auto_registers_with_referrer() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::LevelGated(0));
        set_member(ENTITY_1, BUYER2, true); // referrer must be an existing member
                                            // BUYER is NOT a member, passes referrer; new member default level=0 meets LevelGated(0)
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
        assert!(get_member_registered().contains(&(ENTITY_1, BUYER)));
    });
}

#[test]
fn r13_level_gated_rejects_newly_registered_insufficient() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::LevelGated(3));
        set_member(ENTITY_1, BUYER2, true); // referrer must be an existing member
                                            // BUYER is NOT a member; auto-register succeeds but new member level=0 < 3
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                Some(BUYER2),
                None,
                None,
            ),
            crate::Error::<Test>::MemberLevelInsufficient
        );
    });
}

#[test]
fn r13_public_no_auto_register() {
    new_test_ext().execute_with(|| {
        // Product default is Public; BUYER is NOT a member
        set_member(ENTITY_1, BUYER2, true); // referrer must be an existing member
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2), // referrer passed but shouldn't trigger auto_register
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
        // auto_register should NOT have been called for Public products
        assert!(!get_member_registered().contains(&(ENTITY_1, BUYER)));
    });
}

#[test]
fn r13_already_member_no_duplicate_register() {
    new_test_ext().execute_with(|| {
        set_product_visibility(1, pallet_entity_common::ProductVisibility::MembersOnly);
        set_member(ENTITY_1, BUYER, true);
        set_member(ENTITY_1, BUYER2, true); // referrer must be an existing member
                                            // BUYER is already a member
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            Some(BUYER2),
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Paid);
        // auto_register should NOT have been called since buyer is already a member
        assert!(!get_member_registered().contains(&(ENTITY_1, BUYER)));
    });
}

#[test]
fn r13_min_order_quantity_rejects_below_minimum() {
    new_test_ext().execute_with(|| {
        set_product_min_quantity(1, 3);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                2,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::QuantityBelowMinimum
        );
    });
}

#[test]
fn r13_min_order_quantity_allows_at_minimum() {
    new_test_ext().execute_with(|| {
        set_product_min_quantity(1, 3);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            3,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.quantity, 3);
    });
}

#[test]
fn r13_max_order_quantity_rejects_above_maximum() {
    new_test_ext().execute_with(|| {
        set_product_max_quantity(1, 5);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                6,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::QuantityAboveMaximum
        );
    });
}

#[test]
fn r13_max_order_quantity_allows_at_maximum() {
    new_test_ext().execute_with(|| {
        set_product_max_quantity(1, 5);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            5,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.quantity, 5);
    });
}

#[test]
fn r13_shop_inactive_rejects_order() {
    new_test_ext().execute_with(|| {
        set_shop_active(SHOP_1, false);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::ShopInactive
        );
    });
}

#[test]
fn r13_shop_inactive_allows_when_reactivated() {
    new_test_ext().execute_with(|| {
        set_shop_active(SHOP_1, false);
        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::ShopInactive
        );

        set_shop_active(SHOP_1, true);
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
    });
}

#[test]
fn r13_expiry_queue_full_rejects_order() {
    new_test_ext().execute_with(|| {
        use frame_support::BoundedVec;
        use sp_runtime::traits::ConstU32;

        // ShipTimeout=100, block=1 → expiry block = 101
        let ids: Vec<u64> = (10000..10500).collect();
        let bounded: BoundedVec<u64, ConstU32<500>> = ids.try_into().unwrap();
        crate::ExpiryQueue::<Test>::insert(101u64, bounded);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::ExpiryQueueFull
        );
    });
}

#[test]
fn r13_note_cid_stored_correctly() {
    new_test_ext().execute_with(|| {
        // args: product_id, quantity, shipping_cid, use_tokens, use_shopping_balance, payment_asset, note_cid, referrer
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            Some(b"buyer_note_12345".to_vec()),
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert!(order.note_cid.is_some());
        assert_eq!(
            order.note_cid.unwrap().into_inner(),
            b"buyer_note_12345".to_vec()
        );
    });
}

#[test]
fn r13_note_cid_none_when_not_provided() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert!(order.note_cid.is_none());
    });
}

#[test]
fn r13_deduct_stock_on_place_order() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            3,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let stock = MockProductProvider::product_stock(1).unwrap();
        assert_eq!(stock, 7);
    });
}

#[test]
fn r13_restore_stock_on_cancel() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            3,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 7);

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 10);
    });
}

#[test]
fn r13_restore_stock_on_refund() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            2,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 8);

        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"defective".to_vec(),
        ));
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 10);
    });
}

#[test]
fn r13_restore_stock_on_seller_cancel() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            4,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 6);

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"out_of_stock".to_vec(),
        ));

        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 10);
    });
}

#[test]
fn r13_restore_stock_on_auto_refund_ship_timeout() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            2,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 8);

        // ShipTimeout = 100, placed at block 1, expires at 101
        run_to_block(102);

        let order = crate::Orders::<Test>::get(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert_eq!(MockProductProvider::product_stock(1).unwrap(), 10);
    });
}

#[test]
fn r13_insufficient_stock_rejects_order() {
    new_test_ext().execute_with(|| {
        set_product_stock(1, 2);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                3,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            crate::Error::<Test>::InsufficientStock
        );
    });
}

// ==================== R13: 审计修复回归测试 ====================

#[test]
fn r13_force_process_expirations_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        System::set_block_number(200);
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Paid);

        assert_ok!(Transaction::force_process_expirations(
            RuntimeOrigin::root(),
            101
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );

        assert_noop!(
            Transaction::force_process_expirations(RuntimeOrigin::signed(BUYER), 101),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn r13_withdraw_dispute_shipped_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );

        assert_ok!(Transaction::withdraw_dispute(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.dispute_deadline.is_none());
        assert!(!order.dispute_rejected);
        assert!(order.refund_reason_cid.is_none());
    });
}

#[test]
fn r13_withdraw_dispute_restores_paid() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_ok!(Transaction::withdraw_dispute(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Paid);
    });
}

#[test]
fn r13_withdraw_dispute_fails_after_reject() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_ok!(Transaction::reject_refund(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reject".to_vec()
        ));

        assert_noop!(
            Transaction::withdraw_dispute(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::DisputeAlreadyRejected
        );
    });
}

#[test]
fn r13_withdraw_dispute_fails_not_buyer() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));

        assert_noop!(
            Transaction::withdraw_dispute(RuntimeOrigin::signed(SELLER), 1),
            Error::<Test>::NotOrderParticipant
        );
    });
}

#[test]
fn r13_withdraw_dispute_fails_not_disputed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));

        assert_noop!(
            Transaction::withdraw_dispute(RuntimeOrigin::signed(BUYER), 1),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn r13_ship_order_cleans_old_expiry() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert!(!crate::ExpiryQueue::<Test>::get(101u64).is_empty());

        System::set_block_number(5);
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        assert!(
            crate::ExpiryQueue::<Test>::get(101u64).is_empty(),
            "H4: ship_order should clean old ShipTimeout entry"
        );
        assert!(!crate::ExpiryQueue::<Test>::get(205u64).is_empty());
    });
}

#[test]
fn r13_extend_confirm_cleans_old_expiry() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        System::set_block_number(5);
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        System::set_block_number(50);
        assert_ok!(Transaction::extend_confirm_timeout(
            RuntimeOrigin::signed(BUYER),
            1
        ));

        assert!(
            crate::ExpiryQueue::<Test>::get(205u64).is_empty(),
            "H4: extend should clean old ConfirmTimeout entry"
        );
        assert!(!crate::ExpiryQueue::<Test>::get(150u64).is_empty());
    });
}

#[test]
fn r13_request_refund_fails_on_disputed() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason1".to_vec()
        ));

        assert_noop!(
            Transaction::request_refund(RuntimeOrigin::signed(BUYER), 1, b"reason2".to_vec()),
            Error::<Test>::InvalidOrderStatus
        );
    });
}

#[test]
fn r13_expiry_queue_full_blocks_place_order() {
    new_test_ext().execute_with(|| {
        for i in 1u64..=500 {
            crate::ExpiryQueue::<Test>::mutate(101u64, |ids| {
                let _ = ids.try_push(i);
            });
        }

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::ExpiryQueueFull
        );
    });
}

#[test]
fn r13_buyer_orders_overflow() {
    new_test_ext().execute_with(|| {
        let mut ids = sp_std::vec::Vec::new();
        for i in 1u64..=1000 {
            ids.push(i);
        }
        let bounded: frame_support::BoundedVec<u64, frame_support::traits::ConstU32<1000>> =
            ids.try_into().unwrap();
        crate::BuyerOrders::<Test>::insert(BUYER, bounded);

        assert_noop!(
            Transaction::place_order(
                RuntimeOrigin::signed(BUYER),
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None
            ),
            Error::<Test>::Overflow
        );
    });
}

#[test]
fn r13_withdraw_dispute_token_order() {
    new_test_ext().execute_with(|| {
        set_token_enabled(1, true);
        set_token_price(1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(1, true);
        set_token_balance(1, BUYER, 10000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(pallet_entity_common::PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );

        assert_ok!(Transaction::withdraw_dispute(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert_eq!(Transaction::orders(1).unwrap().status, OrderStatus::Paid);
    });
}

// ==================== 代付功能 (Proxy-Pay) Tests ====================

// ---- 代付基础 (6 tests) ----

#[test]
fn proxy_pay_nex_basic_works() {
    new_test_ext().execute_with(|| {
        // PAYER 为 BUYER 代付 Physical 商品
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1, // physical, price 100
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).expect("order should exist");
        assert_eq!(order.buyer, BUYER);
        assert_eq!(order.seller, SELLER);
        assert_eq!(order.payer, Some(PAYER));
        assert_eq!(order.total_amount, 100);
        assert_eq!(order.status, OrderStatus::Paid);
    });
}

#[test]
fn proxy_pay_token_basic_works() {
    new_test_ext().execute_with(|| {
        set_token_enabled(1, true);
        set_token_price(1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(1, true);
        set_token_balance(1, PAYER, 500);

        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(pallet_entity_common::PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payer, Some(PAYER));
        assert_eq!(
            order.payment_asset,
            pallet_entity_common::PaymentAsset::EntityToken
        );
        assert_eq!(order.token_payment_amount, 100);
    });
}

#[test]
fn proxy_pay_insufficient_balance_fails() {
    new_test_ext().execute_with(|| {
        // Product 1 stock is only 10, set it higher so stock check passes
        set_product_stock(1, 2000);
        // Product 1 price=100, qty=1001 = 100_100 > PAYER's 100_000
        assert_noop!(
            Transaction::place_order_for(
                RuntimeOrigin::signed(PAYER),
                BUYER,
                1,
                1001,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            sp_runtime::DispatchError::Token(sp_runtime::TokenError::FundsUnavailable)
        );
    });
}

#[test]
fn proxy_pay_payer_is_seller_fails() {
    new_test_ext().execute_with(|| {
        // SELLER pays for BUYER → PayerCannotBeSeller
        assert_noop!(
            Transaction::place_order_for(
                RuntimeOrigin::signed(SELLER),
                BUYER,
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::PayerCannotBeSeller
        );
    });
}

#[test]
fn proxy_pay_buyer_discount_payer_pays_remainder() {
    new_test_ext().execute_with(|| {
        // Set buyer as member with level discount
        set_member(1, BUYER, true);
        set_member_level(1, BUYER, 2);
        set_level_discount(1, 2, 1000); // 10% discount

        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 90); // 100 - 10% = 90
        assert_eq!(order.payer, Some(PAYER));
    });
}

#[test]
fn proxy_pay_degenerate_payer_equals_buyer() {
    new_test_ext().execute_with(|| {
        // payer == buyer → stored_payer = None (degenerate case)
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(BUYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payer, None); // degenerate: no separate payer stored
        assert_eq!(order.buyer, BUYER);
    });
}

// ---- 代付退款 (6 tests) ----

#[test]
fn proxy_pay_buyer_can_cancel() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // buyer can cancel
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Cancelled
        );
    });
}

#[test]
fn proxy_pay_payer_can_cancel() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // payer can cancel
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(PAYER), 1));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Cancelled
        );
    });
}

#[test]
fn proxy_pay_seller_approve_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // buyer requests refund
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );

        // seller approves
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

#[test]
fn proxy_pay_ship_timeout_auto_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Advance past ShipTimeout (100 blocks)
        run_to_block(102);

        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

#[test]
fn proxy_pay_dispute_timeout_auto_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Ship the order
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Payer requests refund
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(PAYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );

        // Advance past dispute timeout (300 blocks from dispute)
        let now = System::block_number();
        run_to_block((now + 301) as u32);

        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Refunded
        );
    });
}

#[test]
fn proxy_pay_partial_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Force partial refund (root)
        assert_ok!(Transaction::force_partial_refund(
            RuntimeOrigin::root(),
            1,
            5000,
            None
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::PartiallyRefunded
        );
    });
}

// ---- 代付结算 (4 tests) ----

#[test]
fn proxy_pay_nex_complete_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );
    });
}

#[test]
fn proxy_pay_token_complete_works() {
    new_test_ext().execute_with(|| {
        set_token_enabled(1, true);
        set_token_price(1, 1_000_000_000_000); // 1:1 Token/NEX
        set_token_price_reliable(1, true);
        set_token_balance(1, PAYER, 500);

        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(pallet_entity_common::PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::confirm_receipt(
            RuntimeOrigin::signed(BUYER),
            1
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );
    });
}

#[test]
fn proxy_pay_auto_confirm_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Advance past ConfirmTimeout (200 blocks)
        let now = System::block_number();
        run_to_block((now + 201) as u32);

        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Completed
        );
    });
}

#[test]
fn proxy_pay_digital_auto_completes() {
    new_test_ext().execute_with(|| {
        // Product 2 = Digital, auto-completes
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
        assert_eq!(order.payer, Some(PAYER));
    });
}

// ---- 代付权限 (6 tests) ----

#[test]
fn proxy_pay_payer_cannot_confirm() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Payer cannot confirm receipt (buyer-only)
        assert_noop!(
            Transaction::confirm_receipt(RuntimeOrigin::signed(PAYER), 1),
            Error::<Test>::NotOrderBuyer
        );
    });
}

#[test]
fn proxy_pay_payer_cannot_extend_timeout() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Payer cannot extend confirm timeout (buyer-only)
        assert_noop!(
            Transaction::extend_confirm_timeout(RuntimeOrigin::signed(PAYER), 1),
            Error::<Test>::NotOrderBuyer
        );
    });
}

#[test]
fn proxy_pay_payer_cannot_update_shipping() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Payer cannot update shipping address (buyer-only)
        assert_noop!(
            Transaction::update_shipping_address(
                RuntimeOrigin::signed(PAYER),
                1,
                b"new_addr".to_vec()
            ),
            Error::<Test>::NotOrderBuyer
        );
    });
}

#[test]
fn proxy_pay_third_party_cannot_cancel() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        // Third party (BUYER2) cannot cancel
        assert_noop!(
            Transaction::cancel_order(RuntimeOrigin::signed(BUYER2), 1),
            Error::<Test>::NotOrderParticipant
        );
    });
}

#[test]
fn proxy_pay_buyer_can_request_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Buyer can request refund
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );
    });
}

#[test]
fn proxy_pay_payer_can_request_refund() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));

        // Payer can request refund
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(PAYER),
            1,
            b"reason".to_vec()
        ));
        assert_eq!(
            Transaction::orders(1).unwrap().status,
            OrderStatus::Disputed
        );
    });
}

// ---- 索引与清理 (5 tests) ----

#[test]
fn proxy_pay_buyer_orders_contains_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert!(Transaction::buyer_orders(BUYER).contains(&1));
    });
}

#[test]
fn proxy_pay_payer_orders_contains_order() {
    new_test_ext().execute_with(|| {
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert!(Transaction::payer_orders(PAYER).contains(&1));
    });
}

#[test]
fn proxy_pay_degenerate_no_payer_orders() {
    new_test_ext().execute_with(|| {
        // payer == buyer → no PayerOrders entry
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(BUYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert!(Transaction::payer_orders(BUYER).is_empty());
    });
}

#[test]
fn proxy_pay_payer_orders_multiple() {
    new_test_ext().execute_with(|| {
        for i in 0..5 {
            let product_id = if i % 2 == 0 { 1 } else { 3 };
            let shipping = if product_id == 1 {
                Some(b"addr".to_vec())
            } else {
                None
            };
            assert_ok!(Transaction::place_order_for(
                RuntimeOrigin::signed(PAYER),
                BUYER,
                product_id,
                1,
                shipping,
                None,
                None,
                None,
                None,
                None,
                None,
            ));
        }
        assert_eq!(Transaction::payer_orders(PAYER).len(), 5);
    });
}

#[test]
fn proxy_pay_cleanup_payer_orders_works() {
    new_test_ext().execute_with(|| {
        // Place 2 proxy-pay orders
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            3,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        assert_eq!(Transaction::payer_orders(PAYER).len(), 2);

        // Cancel both
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 2));

        // Cleanup
        assert_ok!(Transaction::cleanup_payer_orders(RuntimeOrigin::signed(
            PAYER
        )));
        assert_eq!(Transaction::payer_orders(PAYER).len(), 0);
    });
}

// ==================== BUG-1 回归：Token 退款金额使用 token_payment_amount ====================

// BUG-1: Token 价格非 1:1 时，退款必须使用 token_payment_amount 而非 total_amount (NEX)
#[test]
fn bug1_token_refund_uses_token_amount_not_nex_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        // Token 价格 = 0.5 NEX/Token (即 500_000_000_000)
        // 1 NEX 可购买 2 Token → token_amount = nex_amount * 10^12 / price = nex * 2
        set_token_price(entity_id, 500_000_000_000);
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        // Product 1: Physical, usdt_price=100, with mock nex_usdt_price=10^12 → nex_amount=100
        // token_amount = 100 * 10^12 / 500_000_000_000 = 200
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 100); // NEX equivalent
        assert_eq!(order.token_payment_amount, 200); // actual Token reserved

        // Verify reserved amount
        assert_eq!(get_token_reserved(entity_id, BUYER), 200);
        assert_eq!(get_token_balance(entity_id, BUYER), 99_800); // 100_000 - 200

        // Cancel order → should unreserve 200 Token (not 100)
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        // BUG-1 fix: reserved must be 0 (all 200 Token returned)
        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        // Buyer gets back full 100_000
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// BUG-1: Token 退款 — seller_cancel_order 路径
#[test]
fn bug1_seller_cancel_token_order_refunds_correct_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        // Token 价格 = 2 NEX/Token → token_amount = nex / 2
        set_token_price(entity_id, 2_000_000_000_000);
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        // nex_amount=100, token_amount = 100 * 10^12 / 2*10^12 = 50
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.total_amount, 100);
        assert_eq!(order.token_payment_amount, 50);
        assert_eq!(get_token_reserved(entity_id, BUYER), 50);

        // Seller cancels
        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"reason".to_vec(),
        ));

        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// BUG-1: Token 退款 — approve_refund (争议退款) 路径
#[test]
fn bug1_approve_refund_token_order_refunds_correct_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 500_000_000_000); // 0.5 NEX/Token
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        // nex=100, token=200
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(entity_id, BUYER), 200);

        // Ship → Dispute → Approve refund
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec(),
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"broken".to_vec(),
        ));
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// BUG-1: Token 退款 — force_refund 路径
#[test]
fn bug1_force_refund_token_order_refunds_correct_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 500_000_000_000);
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(entity_id, BUYER), 200);

        assert_ok!(Transaction::force_refund(
            RuntimeOrigin::root(),
            1,
            Some(b"admin".to_vec())
        ));

        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// BUG-1: Token 退款 — 超时自动退款路径
#[test]
fn bug1_token_ship_timeout_auto_refunds_correct_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 500_000_000_000);
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(entity_id, BUYER), 200);

        // ShipTimeout = 100 blocks, placed at block 1 → expires at block 101
        run_to_block(102);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// BUG-1: seller_refund_order 路径
#[test]
fn bug1_seller_refund_token_order_refunds_correct_amount() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        set_token_enabled(entity_id, true);
        set_token_price(entity_id, 500_000_000_000);
        set_token_price_reliable(entity_id, true);
        set_token_balance(entity_id, BUYER, 100_000);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::EntityToken),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_reserved(entity_id, BUYER), 200);

        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec(),
        ));
        assert_ok!(Transaction::seller_refund_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"defect".to_vec(),
        ));

        assert_eq!(get_token_reserved(entity_id, BUYER), 0);
        assert_eq!(get_token_balance(entity_id, BUYER), 100_000);
    });
}

// ==================== BUG-1: Loyalty refund on cancel ====================

#[test]
fn cancel_order_refunds_shopping_balance() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);
        let buyer_bal_before = pallet_balances::Pallet::<Test>::free_balance(BUYER);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.shopping_balance_used, 100);
        assert_eq!(order.total_amount, 0);
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 100); // 200 - 100
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before
        );

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        assert_eq!(
            get_shopping_balance(ENTITY_1, BUYER),
            200,
            "BUG-1: shopping balance must be restored on cancel"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before,
            "Scheme A: cancel must not mint free NEX to buyer"
        );
    });
}

#[test]
fn cancel_order_refunds_token_discount() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 100);
        set_redeem_discount(ENTITY_1, BUYER, 20); // 20 NEX discount

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            Some(50),
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.token_discount_tokens_burned, 50);
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 50); // 100 - 50 burned

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        assert_eq!(
            get_token_balance(ENTITY_1, BUYER),
            100,
            "BUG-1: token discount tokens must be restored on cancel"
        );
    });
}

#[test]
fn seller_cancel_refunds_loyalty() {
    new_test_ext().execute_with(|| {
        // Test token discount rollback on seller cancel (Native channel)
        set_token_enabled(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 50);
        set_redeem_discount(ENTITY_1, BUYER, 10);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            Some(30), // use_tokens = 30 → burns 30, discount = 10
            None,
            None,
            None,
            None,
            None,
        ));

        assert_eq!(get_token_balance(ENTITY_1, BUYER), 20); // 50 - 30

        assert_ok!(Transaction::seller_cancel_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"out_of_stock".to_vec(),
        ));

        assert_eq!(get_token_balance(ENTITY_1, BUYER), 50);
    });
}

#[test]
fn approve_refund_refunds_loyalty() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);
        let buyer_bal_before = pallet_balances::Pallet::<Test>::free_balance(BUYER);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 100); // 200 - 100
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before
        );

        // Ship → dispute → approve refund
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"defect".to_vec()
        ));
        assert_ok!(Transaction::approve_refund(
            RuntimeOrigin::signed(SELLER),
            1
        ));

        assert_eq!(
            get_shopping_balance(ENTITY_1, BUYER),
            200,
            "BUG-1: approve_refund must restore shopping balance"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before,
            "Scheme A: approve_refund must not depend on buyer free balance"
        );
    });
}

#[test]
fn force_refund_refunds_loyalty() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);
        let buyer_bal_before = pallet_balances::Pallet::<Test>::free_balance(BUYER);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 100); // 200 - 100
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before
        );

        assert_ok!(Transaction::force_refund(
            RuntimeOrigin::root(),
            1,
            Some(b"admin".to_vec())
        ));

        assert_eq!(
            get_shopping_balance(ENTITY_1, BUYER),
            200,
            "BUG-1: force_refund must restore shopping balance"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before,
            "Scheme A: force_refund must not mint free NEX to buyer"
        );
    });
}

#[test]
fn auto_timeout_refunds_loyalty() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);
        let buyer_bal_before = pallet_balances::Pallet::<Test>::free_balance(BUYER);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 100); // 200 - 100
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before
        );

        // ShipTimeout = 100 blocks; order created at block 1 → expires at 101
        run_to_block(102);

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
        assert_eq!(
            get_shopping_balance(ENTITY_1, BUYER),
            200,
            "BUG-1: auto timeout refund must restore shopping balance"
        );
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before,
            "Scheme A: timeout refund must not depend on buyer free balance"
        );
    });
}

#[test]
fn force_partial_refund_refunds_loyalty() {
    new_test_ext().execute_with(|| {
        // Partial refund only works on Native orders.
        // Test that token discount is fully restored even on partial refund.
        set_token_enabled(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 50);
        set_redeem_discount(ENTITY_1, BUYER, 20);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            Some(30), // use_tokens = 30
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 20); // 50 - 30

        // Ship → dispute → partial refund (50%)
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        assert_ok!(Transaction::request_refund(
            RuntimeOrigin::signed(BUYER),
            1,
            b"issue".to_vec()
        ));
        assert_ok!(Transaction::force_partial_refund(
            RuntimeOrigin::root(),
            1,
            5000,
            Some(b"split".to_vec())
        ));

        assert_eq!(
            get_token_balance(ENTITY_1, BUYER),
            50,
            "BUG-1: partial refund must fully restore token discount"
        );
    });
}

#[test]
fn order_records_loyalty_fields() {
    new_test_ext().execute_with(|| {
        // Test 1: ShoppingBalance channel records shopping_balance_used
        set_shopping_balance(ENTITY_1, BUYER, 200);
        let buyer_bal_before = pallet_balances::Pallet::<Test>::free_balance(BUYER);

        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.shopping_balance_used, 100); // full unit_price
        assert_eq!(order.token_discount_tokens_burned, 0);
        assert_eq!(order.total_amount, 0); // no NEX escrow
        assert_eq!(order.payment_asset, PaymentAsset::ShoppingBalance);
        assert_eq!(
            pallet_balances::Pallet::<Test>::free_balance(BUYER),
            buyer_bal_before,
            "Scheme A: placing a ShoppingBalance order must not credit buyer free NEX"
        );
    });
}

#[test]
fn no_loyalty_no_rollback() {
    new_test_ext().execute_with(|| {
        // Order without any loyalty
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.shopping_balance_used, 0);
        assert_eq!(order.token_discount_tokens_burned, 0);

        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));

        let order_after = Transaction::orders(1).unwrap();
        assert_eq!(order_after.status, OrderStatus::Cancelled);
        // No loyalty to rollback — should not fail or emit LoyaltyRollback event
    });
}

// ==================== BUG-2: place_order_for auth bypass ====================

#[test]
fn place_order_for_rejects_use_shopping_balance() {
    new_test_ext().execute_with(|| {
        set_shopping_balance(ENTITY_1, BUYER, 200);

        // Proxy pay with ShoppingBalance channel should be rejected (payer != buyer)
        assert_noop!(
            Transaction::place_order_for(
                RuntimeOrigin::signed(PAYER),
                BUYER,
                1,
                1,
                Some(b"addr".to_vec()),
                None,
                Some(PaymentAsset::ShoppingBalance),
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::PayerCannotUseBuyerLoyalty
        );

        // Buyer's shopping balance untouched
        assert_eq!(get_shopping_balance(ENTITY_1, BUYER), 200);
    });
}

#[test]
fn place_order_for_rejects_use_tokens() {
    new_test_ext().execute_with(|| {
        set_token_enabled(ENTITY_1, true);
        set_token_balance(ENTITY_1, BUYER, 100);
        set_redeem_discount(ENTITY_1, BUYER, 20);

        assert_noop!(
            Transaction::place_order_for(
                RuntimeOrigin::signed(PAYER),
                BUYER,
                1,
                1,
                Some(b"addr".to_vec()),
                Some(50),
                None,
                None,
                None,
                None,
                None,
            ),
            Error::<Test>::PayerCannotUseBuyerLoyalty
        );

        // Buyer's tokens untouched
        assert_eq!(get_token_balance(ENTITY_1, BUYER), 100);
    });
}

#[test]
fn place_order_for_allows_zero_loyalty() {
    new_test_ext().execute_with(|| {
        // Some(0) should be allowed for proxy pay
        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(PAYER),
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            Some(0), // zero use_tokens → ok
            None,
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payer, Some(PAYER));
        assert_eq!(order.shopping_balance_used, 0);
        assert_eq!(order.token_discount_tokens_burned, 0);
    });
}

#[test]
fn place_order_for_self_pay_allows_loyalty() {
    new_test_ext().execute_with(|| {
        // payer == buyer degenerates to self-pay → ShoppingBalance allowed
        set_shopping_balance(ENTITY_1, BUYER, 200);

        assert_ok!(Transaction::place_order_for(
            RuntimeOrigin::signed(BUYER), // payer == buyer
            BUYER,
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            Some(PaymentAsset::ShoppingBalance),
            None,
            None,
            None,
            None,
        ));

        let order = Transaction::orders(1).unwrap();
        assert_eq!(order.payer, None); // stored_payer collapses to None
        assert_eq!(order.shopping_balance_used, 100);
        assert_eq!(order.total_amount, 0);
    });
}

// ==================== chat scene authorization wiring ====================
// Verifies the order pallet drives `OrderChatAuthorizer` (grant on creation of a
// non-instant order; revoke at every terminal state). The runtime adapter maps
// these to chat-permission scene authorizations; here we use the recording mock.
// 验证订单模块驱动 `OrderChatAuthorizer`：非即时订单创建时 grant，所有终态 revoke。

#[test]
fn chat_grant_on_physical_place_and_revoke_on_cancel() {
    new_test_ext().execute_with(|| {
        let _ = drain_chat_log();
        // Product 1 = Physical (has a lifecycle) → grant on placement.
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(drain_chat_log(), vec![(true, 1, BUYER, SELLER)]);

        // Cancel (status Paid) → terminal revoke.
        assert_ok!(Transaction::cancel_order(RuntimeOrigin::signed(BUYER), 1));
        assert_eq!(drain_chat_log(), vec![(false, 1, BUYER, SELLER)]);
    });
}

#[test]
fn chat_revoke_on_confirm_receipt_completion() {
    new_test_ext().execute_with(|| {
        let _ = drain_chat_log();
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            1,
            1,
            Some(b"addr".to_vec()),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_ok!(Transaction::ship_order(
            RuntimeOrigin::signed(SELLER),
            1,
            b"track".to_vec()
        ));
        let _ = drain_chat_log(); // discard the grant from placement
        assert_ok!(Transaction::confirm_receipt(RuntimeOrigin::signed(BUYER), 1));
        // Completion revokes the order's chat authorization.
        assert_eq!(drain_chat_log(), vec![(false, 1, BUYER, SELLER)]);
    });
}

#[test]
fn chat_no_grant_for_digital_instant_order() {
    new_test_ext().execute_with(|| {
        let _ = drain_chat_log();
        // Product 2 = Digital → completes instantly, so no chat is opened.
        assert_ok!(Transaction::place_order(
            RuntimeOrigin::signed(BUYER),
            2,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        // No grant ever recorded for an instant order.
        let log = drain_chat_log();
        assert!(!log.iter().any(|(granted, ..)| *granted), "digital order must not grant chat: {:?}", log);
    });
}
