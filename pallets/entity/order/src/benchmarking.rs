//! Benchmarks for pallet-entity-order
//!
//! 全部 23 个 extrinsics 均有 benchmark。
//! 由于 order pallet 依赖大量外部 trait（ShopProvider / ProductProvider / Escrow 等），
//! benchmark 通过直接写入存储来构造前置状态，绕过外部 pallet 依赖。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{
    traits::{Currency, Get},
    BoundedVec,
};
use frame_system::RawOrigin;
use pallet::*;
use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
use pallet_entity_common::{OrderStatus, PaymentAsset, ProductCategory, ShopProvider};
use sp_runtime::traits::{Bounded, Saturating, Zero};

/// 确保账户有足够余额
fn fund_account<T: Config>(account: &T::AccountId) {
    let amount = BalanceOf::<T>::max_value() / 4u32.into();
    let _ = T::Currency::deposit_creating(account, amount);
}

/// 种子订单：直接写入 Orders 存储，绕过 place_order 的全部外部依赖
fn seed_order<T: Config>(
    order_id: u64,
    buyer: &T::AccountId,
    seller: &T::AccountId,
    status: OrderStatus,
    category: ProductCategory,
    payment_asset: PaymentAsset,
) {
    let now = frame_system::Pallet::<T>::block_number();
    let amount: BalanceOf<T> = 1_000u32.into();
    let fee: BalanceOf<T> = 10u32.into();

    let cid = b"QmBenchCid12345678901234567890123456789012345678".to_vec();
    let bounded_cid: BoundedVec<u8, T::MaxCidLength> = cid.try_into().expect("cid fits");

    let shipped_at = if matches!(status, OrderStatus::Shipped | OrderStatus::Disputed) {
        Some(now)
    } else {
        None
    };
    let service_started_at = if category == ProductCategory::Service && shipped_at.is_some() {
        Some(now)
    } else {
        None
    };
    let token_payment = if payment_asset == PaymentAsset::EntityToken {
        1000u128
    } else {
        0u128
    };

    let order = Order {
        id: order_id,
        entity_id: 1u64,
        shop_id: 1u64,
        product_id: 1u64,
        buyer: buyer.clone(),
        seller: seller.clone(),
        quantity: 1u32,
        unit_price: amount,
        total_amount: amount,
        platform_fee: fee,
        usdt_total: 0,
        nex_usdt_rate: 0,
        token_nex_rate: 0,
        product_category: category,
        shipping_cid: Some(bounded_cid.clone()),
        tracking_cid: if shipped_at.is_some() {
            Some(bounded_cid.clone())
        } else {
            None
        },
        status,
        created_at: now,
        shipped_at,
        completed_at: None,
        service_started_at,
        service_completed_at: None,
        payment_asset,
        token_payment_amount: token_payment,
        confirm_extended: false,
        dispute_rejected: false,
        dispute_deadline: None,
        note_cid: None,
        refund_reason_cid: None,
        payer: None,
        shopping_balance_used: Zero::zero(),
        token_discount_tokens_burned: 0u128,
    };

    Orders::<T>::insert(order_id, order);
    if NextOrderId::<T>::get() <= order_id {
        NextOrderId::<T>::put(order_id.saturating_add(1));
    }
    let _ = BuyerOrders::<T>::try_mutate(buyer, |ids| ids.try_push(order_id));
    let _ = ShopOrders::<T>::try_mutate(1u64, |ids| ids.try_push(order_id));

    if payment_asset == PaymentAsset::Native {
        let _ = <T::Escrow as EscrowTrait<T::AccountId, BalanceOf<T>>>::lock_from(
            buyer, order_id, amount,
        );
    }
}

/// 种子 Disputed 订单（含 dispute_deadline）
fn seed_disputed_order<T: Config>(
    order_id: u64,
    buyer: &T::AccountId,
    seller: &T::AccountId,
    category: ProductCategory,
    rejected: bool,
) {
    seed_order::<T>(
        order_id,
        buyer,
        seller,
        OrderStatus::Disputed,
        category,
        PaymentAsset::Native,
    );

    let now = frame_system::Pallet::<T>::block_number();
    let deadline = now.saturating_add(T::DisputeTimeout::get());

    Orders::<T>::mutate(order_id, |maybe| {
        if let Some(o) = maybe {
            o.dispute_deadline = Some(deadline);
            o.dispute_rejected = rejected;
            o.refund_reason_cid = Some(b"QmReason".to_vec().try_into().unwrap_or_default());
        }
    });
    let _ = ExpiryQueue::<T>::try_mutate(deadline, |ids| ids.try_push(order_id));
}

/// 种子终态订单用于 cleanup benchmark
fn seed_terminal_orders<T: Config>(buyer: &T::AccountId, shop_id: u64, count: u32) {
    for i in 0..count {
        let oid = 10000u64 + i as u64;
        let now = frame_system::Pallet::<T>::block_number();
        let amount: BalanceOf<T> = 100u32.into();
        let order = Order {
            id: oid,
            entity_id: 1u64,
            shop_id,
            product_id: 1u64,
            buyer: buyer.clone(),
            seller: buyer.clone(),
            quantity: 1u32,
            unit_price: amount,
            total_amount: amount,
            platform_fee: Zero::zero(),
            usdt_total: 0,
            nex_usdt_rate: 0,
            token_nex_rate: 0,
            payer: None,
            product_category: ProductCategory::Physical,
            shipping_cid: None,
            tracking_cid: None,
            status: OrderStatus::Completed,
            created_at: now,
            shipped_at: None,
            completed_at: Some(now),
            service_started_at: None,
            service_completed_at: None,
            payment_asset: PaymentAsset::Native,
            token_payment_amount: 0u128,
            confirm_extended: false,
            dispute_rejected: false,
            dispute_deadline: None,
            note_cid: None,
            refund_reason_cid: None,
            shopping_balance_used: Zero::zero(),
            token_discount_tokens_burned: 0u128,
        };
        Orders::<T>::insert(oid, order);
        let _ = BuyerOrders::<T>::try_mutate(buyer, |ids| ids.try_push(oid));
        let _ = ShopOrders::<T>::try_mutate(shop_id, |ids| ids.try_push(oid));
    }
}

/// 创建有足够余额的账户
fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
    let account: T::AccountId = frame_benchmarking::account(name, index, 0);
    fund_account::<T>(&account);
    account
}

#[benchmarks]
mod benches {
    use super::*;

    // ==================== call_index(0): place_order ====================
    #[benchmark]
    fn place_order() {
        let buyer = funded_account::<T>("buyer", 0);
        NextOrderId::<T>::put(1u64);

        let cid = b"QmShippingAddr1234567890123456789012345678901234".to_vec();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(buyer),
            1u64,      // product_id
            2u32,      // quantity
            Some(cid), // shipping_cid
            None,      // use_tokens
            None,      // payment_asset
            None,      // note_cid
            None,      // referrer
            None,      // max_nex_amount
            None,      // max_token_amount
        );

        assert!(Orders::<T>::contains_key(1u64));
    }

    // ==================== call_index(1): cancel_order ====================
    #[benchmark]
    fn cancel_order() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    // ==================== call_index(2): ship_order ====================
    #[benchmark]
    fn ship_order() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let now = frame_system::Pallet::<T>::block_number();
        let ship_expiry = now.saturating_add(T::ShipTimeout::get());
        let _ = ExpiryQueue::<T>::try_mutate(ship_expiry, |ids| ids.try_push(1u64));

        let tracking = b"QmTracking12345678901234567890123456789012345678".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64, tracking);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
    }

    // ==================== call_index(3): confirm_receipt ====================
    #[benchmark]
    fn confirm_receipt() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    }

    // ==================== call_index(4): request_refund ====================
    #[benchmark]
    fn request_refund() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmRefundReason123456789012345678901234567890123".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64, reason);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Disputed);
    }

    // ==================== call_index(5): approve_refund ====================
    #[benchmark]
    fn approve_refund() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_disputed_order::<T>(1, &buyer, &seller, ProductCategory::Physical, false);

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    }

    // ==================== call_index(6): start_service ====================
    #[benchmark]
    fn start_service() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Service,
            PaymentAsset::Native,
        );

        let now = frame_system::Pallet::<T>::block_number();
        let ship_expiry = now.saturating_add(T::ShipTimeout::get());
        let _ = ExpiryQueue::<T>::try_mutate(ship_expiry, |ids| ids.try_push(1u64));

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Shipped);
        assert!(order.service_started_at.is_some());
    }

    // ==================== call_index(7): complete_service ====================
    #[benchmark]
    fn complete_service() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Service,
            PaymentAsset::Native,
        );

        let now = frame_system::Pallet::<T>::block_number();
        let svc_expiry = now.saturating_add(T::ServiceConfirmTimeout::get());
        let _ = ExpiryQueue::<T>::try_mutate(svc_expiry, |ids| ids.try_push(1u64));

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert!(order.service_completed_at.is_some());
    }

    // ==================== call_index(8): confirm_service ====================
    #[benchmark]
    fn confirm_service() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Service,
            PaymentAsset::Native,
        );

        Orders::<T>::mutate(1u64, |maybe| {
            if let Some(o) = maybe {
                o.service_completed_at = Some(frame_system::Pallet::<T>::block_number());
            }
        });

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    }

    // ==================== call_index(9): set_platform_fee_rate ====================
    #[benchmark]
    fn set_platform_fee_rate() {
        #[extrinsic_call]
        _(RawOrigin::Root, 500u16);

        assert_eq!(PlatformFeeRate::<T>::get(), 500u16);
    }

    // ==================== call_index(10): cleanup_buyer_orders ====================
    #[benchmark]
    fn cleanup_buyer_orders() {
        let buyer = funded_account::<T>("buyer", 0);
        seed_terminal_orders::<T>(&buyer, 1u64, 20);

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer));
    }

    // ==================== call_index(11): reject_refund ====================
    #[benchmark]
    fn reject_refund() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_disputed_order::<T>(1, &buyer, &seller, ProductCategory::Physical, false);

        let reason = b"QmRejectReason123456789012345678901234567890123".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64, reason);

        let order = Orders::<T>::get(1u64).unwrap();
        assert!(order.dispute_rejected);
    }

    // ==================== call_index(12): seller_cancel_order ====================
    #[benchmark]
    fn seller_cancel_order() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmSellerCancelReason12345678901234567890123456".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64, reason);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Cancelled);
    }

    // ==================== call_index(13): force_refund ====================
    #[benchmark]
    fn force_refund() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmForceRefundReason1234567890123456789012345678".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64, Some(reason));

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    }

    // ==================== call_index(14): force_complete ====================
    #[benchmark]
    fn force_complete() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmForceCompleteReason123456789012345678901234567".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64, Some(reason));

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Completed);
    }

    // ==================== call_index(15): update_shipping_address ====================
    #[benchmark]
    fn update_shipping_address() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Paid,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let new_cid = b"QmNewShippingAddr12345678901234567890123456789012".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64, new_cid);
    }

    // ==================== call_index(16): extend_confirm_timeout ====================
    #[benchmark]
    fn extend_confirm_timeout() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let now = frame_system::Pallet::<T>::block_number();
        let confirm_expiry = now.saturating_add(T::ConfirmTimeout::get());
        let _ = ExpiryQueue::<T>::try_mutate(confirm_expiry, |ids| ids.try_push(1u64));

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert!(order.confirm_extended);
    }

    // ==================== call_index(17): cleanup_shop_orders ====================
    #[benchmark]
    fn cleanup_shop_orders() {
        // shop_owner(1) 在 mock 中返回 SELLER，需要用 shop_owner 的实际返回值
        let shop_id = 1u64;
        let owner = T::ShopProvider::shop_owner(shop_id).expect("shop must exist");
        fund_account::<T>(&owner);
        seed_terminal_orders::<T>(&owner, shop_id, 20);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), shop_id);
    }

    // ==================== call_index(18): update_tracking ====================
    #[benchmark]
    fn update_tracking() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let new_tracking = b"QmNewTracking12345678901234567890123456789012345".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64, new_tracking);
    }

    // ==================== call_index(19): seller_refund_order ====================
    #[benchmark]
    fn seller_refund_order() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmSellerRefundReason12345678901234567890123456".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Signed(seller), 1u64, reason);

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    }

    // ==================== call_index(20): force_partial_refund ====================
    #[benchmark]
    fn force_partial_refund() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_order::<T>(
            1,
            &buyer,
            &seller,
            OrderStatus::Shipped,
            ProductCategory::Physical,
            PaymentAsset::Native,
        );

        let reason = b"QmPartialRefundReason1234567890123456789012345".to_vec();

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64, 5000u16, Some(reason));

        let order = Orders::<T>::get(1u64).unwrap();
        assert_eq!(order.status, OrderStatus::Refunded);
    }

    // ==================== call_index(21): withdraw_dispute ====================
    #[benchmark]
    fn withdraw_dispute() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);
        seed_disputed_order::<T>(1, &buyer, &seller, ProductCategory::Physical, false);

        #[extrinsic_call]
        _(RawOrigin::Signed(buyer), 1u64);

        let order = Orders::<T>::get(1u64).unwrap();
        assert!(matches!(
            order.status,
            OrderStatus::Paid | OrderStatus::Shipped
        ));
    }

    // ==================== call_index(22): force_process_expirations ====================
    #[benchmark]
    fn force_process_expirations() {
        let buyer = funded_account::<T>("buyer", 0);
        let seller = funded_account::<T>("seller", 1);

        let now = frame_system::Pallet::<T>::block_number();
        let target_block = now.saturating_add(1u32.into());

        for i in 0..5u64 {
            let oid = 100 + i;
            seed_order::<T>(
                oid,
                &buyer,
                &seller,
                OrderStatus::Paid,
                ProductCategory::Physical,
                PaymentAsset::Native,
            );
            let _ = ExpiryQueue::<T>::try_mutate(target_block, |ids| ids.try_push(oid));
        }

        frame_system::Pallet::<T>::set_block_number(target_block);

        #[extrinsic_call]
        _(RawOrigin::Root, target_block);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
