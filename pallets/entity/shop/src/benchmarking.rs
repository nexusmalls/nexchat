//! Benchmarks for pallet-entity-shop
//!
//! Covers the current shop lifecycle extrinsics (the points sub-system was
//! moved out of this pallet, so its former benchmarks are gone). Shop state is
//! seeded by writing storage directly to bypass external trait dependencies
//! (EntityProvider / StoragePin / ProductProvider …).

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::{
    traits::{Currency, Get},
    BoundedVec,
};
use frame_system::RawOrigin;
use pallet::*;
use pallet_entity_common::{ShopOperatingStatus, ShopType};
use sp_runtime::traits::{Bounded, Saturating, Zero};

/// 确保账户有足够余额
fn fund_account<T: Config>(account: &T::AccountId) {
    let amount = BalanceOf::<T>::max_value() / 4u32.into();
    let _ = T::Currency::deposit_creating(account, amount);
}

/// 创建有足够余额的账户
fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
    let account: T::AccountId = frame_benchmarking::account(name, index, 0);
    fund_account::<T>(&account);
    account
}

/// 构造 benchmark CID
fn bench_cid<T: Config>() -> BoundedVec<u8, T::MaxCidLength> {
    let cid = b"QmBenchCid12345678901234567890123456789012345678".to_vec();
    cid.try_into().expect("cid fits MaxCidLength")
}

/// 构造 benchmark Shop 名称
fn bench_name<T: Config>() -> BoundedVec<u8, T::MaxShopNameLength> {
    b"Benchmark Shop".to_vec().try_into().expect("name fits")
}

/// 直接写入存储种子一个 Shop，绕过外部依赖
fn seed_shop<T: Config>(shop_id: u64, entity_id: u64, status: ShopOperatingStatus) {
    let now = frame_system::Pallet::<T>::block_number();
    let name: BoundedVec<u8, T::MaxShopNameLength> = b"Bench Shop".to_vec().try_into().unwrap();
    let cid: BoundedVec<u8, T::MaxCidLength> = bench_cid::<T>();

    let shop = Shop {
        id: shop_id,
        entity_id,
        name,
        logo_cid: Some(cid.clone()),
        description_cid: Some(cid.clone()),
        shop_type: ShopType::OnlineStore,
        status,
        managers: BoundedVec::default(),
        location: None,
        address_cid: None,
        business_hours_cid: Some(cid.clone()),
        policies_cid: Some(cid),
        created_at: now,
        product_count: 0,
        total_sales: Zero::zero(),
        total_orders: 0,
        rating: 0,
        rating_total: 0,
        rating_count: 0,
    };

    Shops::<T>::insert(shop_id, shop);
    ShopEntity::<T>::insert(shop_id, entity_id);
    if NextShopId::<T>::get() <= shop_id {
        NextShopId::<T>::put(shop_id.saturating_add(1));
    }
}

/// 种子 Shop 并设为 Active + 有运营资金
fn seed_active_shop<T: Config>(shop_id: u64, entity_id: u64) {
    seed_shop::<T>(shop_id, entity_id, ShopOperatingStatus::Active);
    // 给 shop 账户充值运营资金
    let shop_account = Pallet::<T>::shop_account_id(shop_id);
    let amount: BalanceOf<T> = 100_000u32.into();
    let _ = T::Currency::deposit_creating(&shop_account, amount);
}

/// 种子一个待处理的 Shop 转让请求
fn seed_pending_transfer<T: Config>(shop_id: u64, from_entity_id: u64, to_entity_id: u64) {
    let now = frame_system::Pallet::<T>::block_number();
    PendingTransfers::<T>::insert(
        shop_id,
        PendingShopTransfer {
            from_entity_id,
            to_entity_id,
            keep_managers: true,
            requested_at: now,
        },
    );
}

#[benchmarks]
mod benches {
    use super::*;

    // ==================== call_index(0): create_shop ====================
    #[benchmark]
    fn create_shop() {
        let owner = funded_account::<T>("owner", 0);
        NextShopId::<T>::put(1u64);

        let name = bench_name::<T>();

        // 计算足够的初始资金（覆盖 MinInitialFundUsdt 等值 NEX + MinOperatingBalance）
        let initial_fund: BalanceOf<T> = BalanceOf::<T>::max_value() / 8u32.into();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(owner),
            1u64, // entity_id
            name,
            ShopType::OnlineStore,
            initial_fund,
        );

        assert!(Shops::<T>::contains_key(1u64));
    }

    // ==================== call_index(1): update_shop ====================
    #[benchmark]
    fn update_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        let new_name: BoundedVec<u8, T::MaxShopNameLength> =
            b"Updated Shop".to_vec().try_into().unwrap();
        let new_cid = Some(Some(bench_cid::<T>()));

        #[extrinsic_call]
        _(
            RawOrigin::Signed(owner),
            1u64,
            Some(new_name),
            new_cid.clone(), // logo_cid
            new_cid.clone(), // description_cid
            new_cid.clone(), // business_hours_cid
            new_cid,         // policies_cid
        );
    }

    // ==================== call_index(2): add_manager ====================
    #[benchmark]
    fn add_manager() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);
        let manager: T::AccountId = frame_benchmarking::account("manager", 0, 0);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, manager.clone());

        let shop = Shops::<T>::get(1u64).unwrap();
        assert!(shop.managers.contains(&manager));
    }

    // ==================== call_index(3): remove_manager ====================
    #[benchmark]
    fn remove_manager() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);
        let manager: T::AccountId = frame_benchmarking::account("manager", 0, 0);

        // 先添加 manager
        Shops::<T>::mutate(1u64, |maybe| {
            if let Some(shop) = maybe {
                let _ = shop.managers.try_push(manager.clone());
            }
        });

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, manager.clone());

        let shop = Shops::<T>::get(1u64).unwrap();
        assert!(!shop.managers.contains(&manager));
    }

    // ==================== call_index(4): fund_operating ====================
    #[benchmark]
    fn fund_operating() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, 5000u32.into());
    }

    // ==================== call_index(5): pause_shop ====================
    #[benchmark]
    fn pause_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Paused);
    }

    // ==================== call_index(6): resume_shop ====================
    #[benchmark]
    fn resume_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_shop::<T>(1, 1, ShopOperatingStatus::Paused);
        // 确保有足够运营资金
        let shop_account = Pallet::<T>::shop_account_id(1u64);
        let _ = T::Currency::deposit_creating(&shop_account, 100_000u32.into());

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Active);
    }

    // ==================== call_index(7): set_location ====================
    #[benchmark]
    fn set_location() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        let addr_cid = Some(Some(bench_cid::<T>()));

        #[extrinsic_call]
        _(
            RawOrigin::Signed(owner),
            1u64,
            Some((121_473_000i64, 31_230_000i64)), // 上海坐标
            addr_cid,
        );
    }

    // ==================== call_index(9): close_shop ====================
    #[benchmark]
    fn close_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Closing);
    }

    // ==================== call_index(13): withdraw_operating_fund ====================
    #[benchmark]
    fn withdraw_operating_fund() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        // 确保 shop 有足够余额（超过 MinOperatingBalance）
        let shop_account = Pallet::<T>::shop_account_id(1u64);
        let _ = T::Currency::deposit_creating(&shop_account, 500_000u32.into());

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, 1000u32.into());
    }

    // ==================== call_index(15): finalize_close_shop ====================
    #[benchmark]
    fn finalize_close_shop() {
        let caller = funded_account::<T>("caller", 0);
        seed_shop::<T>(1, 1, ShopOperatingStatus::Closing);

        // 设置关闭时间为足够早
        let now = frame_system::Pallet::<T>::block_number();
        let grace = T::ShopClosingGracePeriod::get();
        let closing_at = now.saturating_sub(grace.saturating_add(1u32.into()));
        ShopClosingAt::<T>::insert(1u64, closing_at);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Closed);
    }

    // ==================== call_index(19): request_transfer_shop (weight: transfer_shop) ====================
    #[benchmark]
    fn transfer_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        request_transfer_shop(RawOrigin::Signed(owner), 1u64, 2u64);

        assert!(PendingTransfers::<T>::contains_key(1u64));
    }

    // ==================== call_index(33): accept_transfer_shop ====================
    #[benchmark]
    fn accept_transfer_shop() {
        let to_owner = funded_account::<T>("to_owner", 0);
        seed_active_shop::<T>(1, 1);
        seed_pending_transfer::<T>(1, 1, 2);

        #[extrinsic_call]
        _(RawOrigin::Signed(to_owner), 1u64, true);
    }

    // ==================== call_index(34): cancel_transfer_shop ====================
    #[benchmark]
    fn cancel_transfer_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);
        seed_pending_transfer::<T>(1, 1, 2);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64);

        assert!(!PendingTransfers::<T>::contains_key(1u64));
    }

    // ==================== call_index(35): allocate_from_treasury ====================
    #[benchmark]
    fn allocate_from_treasury() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, 5000u32.into());
    }

    // ==================== call_index(20): set_primary_shop ====================
    #[benchmark]
    fn set_primary_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);
        seed_active_shop::<T>(2, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, 2u64);
    }

    // ==================== call_index(21): force_pause_shop ====================
    #[benchmark]
    fn force_pause_shop() {
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Paused);
    }

    // ==================== call_index(24): force_close_shop ====================
    #[benchmark]
    fn force_close_shop() {
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Closed);
    }

    // ==================== call_index(27): set_shop_type ====================
    #[benchmark]
    fn set_shop_type() {
        let owner = funded_account::<T>("owner", 0);
        seed_active_shop::<T>(1, 1);

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64, ShopType::PhysicalStore);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.shop_type, ShopType::PhysicalStore);
    }

    // ==================== call_index(28): cancel_close_shop ====================
    #[benchmark]
    fn cancel_close_shop() {
        let owner = funded_account::<T>("owner", 0);
        seed_shop::<T>(1, 1, ShopOperatingStatus::Closing);
        ShopClosingAt::<T>::insert(1u64, frame_system::Pallet::<T>::block_number());

        // 确保有运营资金（恢复为 Active）
        let shop_account = Pallet::<T>::shop_account_id(1u64);
        let _ = T::Currency::deposit_creating(&shop_account, 100_000u32.into());

        #[extrinsic_call]
        _(RawOrigin::Signed(owner), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Active);
    }

    // ==================== call_index(30): resign_manager ====================
    #[benchmark]
    fn resign_manager() {
        let manager = funded_account::<T>("manager", 0);
        seed_active_shop::<T>(1, 1);

        // 添加 manager 到列表
        Shops::<T>::mutate(1u64, |maybe| {
            if let Some(shop) = maybe {
                let _ = shop.managers.try_push(manager.clone());
            }
        });

        #[extrinsic_call]
        _(RawOrigin::Signed(manager.clone()), 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert!(!shop.managers.contains(&manager));
    }

    // ==================== call_index(31): ban_shop ====================
    #[benchmark]
    fn ban_shop() {
        seed_active_shop::<T>(1, 1);

        let reason: BoundedVec<u8, T::MaxCidLength> =
            b"Violation of terms".to_vec().try_into().unwrap();

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64, reason);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Banned);
    }

    // ==================== call_index(32): unban_shop ====================
    #[benchmark]
    fn unban_shop() {
        seed_shop::<T>(1, 1, ShopOperatingStatus::Banned);
        ShopStatusBeforeBan::<T>::insert(1u64, ShopOperatingStatus::Active);
        ShopBanReason::<T>::insert(
            1u64,
            BoundedVec::<u8, T::MaxCidLength>::try_from(b"reason".to_vec()).unwrap(),
        );

        #[extrinsic_call]
        _(RawOrigin::Root, 1u64);

        let shop = Shops::<T>::get(1u64).unwrap();
        assert_eq!(shop.status, ShopOperatingStatus::Active);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
