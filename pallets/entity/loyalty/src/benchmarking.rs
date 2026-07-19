//! Benchmarks for pallet-entity-loyalty
//!
//! Covers all 11 extrinsics (call_index 0..=10).
//! External trait dependencies (EntityProvider, ShopProvider, etc.) are bypassed
//! by writing directly into storage to construct preconditions.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::pallet::{
    BalanceOf, Config, PointsCleanupCursor, PointsConfig, ShopPointsBalances, ShopPointsConfigs,
    ShopPointsExpiresAt, ShopPointsMaxSupply, ShopPointsTotalSupply, ShopPointsTtl,
};
use crate::CleanupPhase;
use frame_benchmarking::v2::*;
use frame_support::{traits::Currency, BoundedVec};
use frame_system::RawOrigin;
use sp_runtime::traits::{Bounded, Saturating};

const SHOP_ID: u64 = 1;

/// Fund an account with a large balance.
fn fund_account<T: Config>(account: &T::AccountId) {
    let amount = BalanceOf::<T>::max_value() / 4u32.into();
    let _ = T::Currency::deposit_creating(account, amount);
}

/// Create a funded account by name+index.
fn funded_account<T: Config>(name: &'static str, index: u32) -> T::AccountId {
    let account: T::AccountId = frame_benchmarking::account(name, index, 0);
    fund_account::<T>(&account);
    account
}

/// Set up mock Entity/Shop state so extrinsic guards pass (test only).
/// In real runtime benchmarks the providers point to live pallets.
fn setup_mock_state<T: Config>(_owner: &T::AccountId) {
    #[cfg(test)]
    {
        use codec::Encode;
        let bytes = _owner.encode();
        let id = if bytes.len() >= 8 {
            u64::from_le_bytes(bytes[..8].try_into().unwrap())
        } else {
            0u64
        };
        crate::mock::register_benchmark_shop(SHOP_ID, 1u64, id);
    }
}

/// Seed a PointsConfig for the given shop_id (bypasses extrinsic guards).
fn seed_points_config<T: Config>(shop_id: u64) {
    let name: BoundedVec<u8, T::MaxPointsNameLength> = b"BenchPts".to_vec().try_into().unwrap();
    let symbol: BoundedVec<u8, T::MaxPointsSymbolLength> = b"BP".to_vec().try_into().unwrap();
    let config = PointsConfig {
        name,
        symbol,
        reward_rate: 500,
        exchange_rate: 1000,
        transferable: true,
    };
    ShopPointsConfigs::<T>::insert(shop_id, config);
}

/// Seed points balance for an account.
fn seed_points_balance<T: Config>(shop_id: u64, account: &T::AccountId, amount: BalanceOf<T>) {
    ShopPointsBalances::<T>::insert(shop_id, account, amount);
    ShopPointsTotalSupply::<T>::mutate(shop_id, |s| *s = s.saturating_add(amount));
}

/// Seed multiple points holders (for clear_prefix benchmarks).
fn seed_points_users<T: Config>(shop_id: u64, count: u32) {
    for i in 0..count {
        let account: T::AccountId = frame_benchmarking::account("pts_user", i, 0);
        let amount: BalanceOf<T> = 100u32.into();
        ShopPointsBalances::<T>::insert(shop_id, &account, amount);
        ShopPointsTotalSupply::<T>::mutate(shop_id, |s| *s = s.saturating_add(amount));
        let now = frame_system::Pallet::<T>::block_number();
        ShopPointsExpiresAt::<T>::insert(shop_id, &account, now.saturating_add(1000u32.into()));
    }
}

#[benchmarks]
mod benches {
    use super::*;

    // ==================== call_index(0): enable_points ====================
    #[benchmark]
    fn enable_points() {
        let caller = funded_account::<T>("owner", 0);
        setup_mock_state::<T>(&caller);

        let name: BoundedVec<u8, T::MaxPointsNameLength> = b"BenchPts".to_vec().try_into().unwrap();
        let symbol: BoundedVec<u8, T::MaxPointsSymbolLength> = b"BP".to_vec().try_into().unwrap();

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            SHOP_ID,
            name,
            symbol,
            500u16,
            1000u16,
            true,
        );

        assert!(ShopPointsConfigs::<T>::contains_key(SHOP_ID));
    }

    // ==================== call_index(1): disable_points ====================
    #[benchmark]
    fn disable_points() {
        let caller = funded_account::<T>("owner", 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);
        seed_points_users::<T>(SHOP_ID, 50);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID);

        assert!(!ShopPointsConfigs::<T>::contains_key(SHOP_ID));
    }

    // ==================== call_index(2): update_points_config ====================
    #[benchmark]
    fn update_points_config() {
        let caller = funded_account::<T>("owner", 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            SHOP_ID,
            Some(800u16),
            Some(2000u16),
            Some(false),
        );
    }

    // ==================== call_index(3): transfer_points ====================
    #[benchmark]
    fn transfer_points() {
        let from = funded_account::<T>("from", 0);
        let to: T::AccountId = frame_benchmarking::account("to", 0, 0);
        setup_mock_state::<T>(&from);
        seed_points_config::<T>(SHOP_ID);
        seed_points_balance::<T>(SHOP_ID, &from, 10_000u32.into());

        #[extrinsic_call]
        _(RawOrigin::Signed(from), SHOP_ID, to, 5_000u32.into());
    }

    // ==================== call_index(4): manager_issue_points ====================
    #[benchmark]
    fn manager_issue_points() {
        let caller = funded_account::<T>("owner", 0);
        let to: T::AccountId = frame_benchmarking::account("recipient", 0, 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID, to, 5_000u32.into());
    }

    // ==================== call_index(5): manager_burn_points ====================
    #[benchmark]
    fn manager_burn_points() {
        let caller = funded_account::<T>("owner", 0);
        let target: T::AccountId = frame_benchmarking::account("target", 0, 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);
        seed_points_balance::<T>(SHOP_ID, &target, 10_000u32.into());

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID, target, 5_000u32.into());
    }

    // ==================== call_index(6): redeem_points ====================
    #[benchmark]
    fn redeem_points() {
        let user = funded_account::<T>("user", 0);
        setup_mock_state::<T>(&user);
        seed_points_config::<T>(SHOP_ID);
        seed_points_balance::<T>(SHOP_ID, &user, 10_000u32.into());

        // Fund the shop account so the redeem payout succeeds.
        // In mock, shop_account(shop_id) returns 0 — fund that account.
        #[cfg(test)]
        {
            let zero_account: T::AccountId = {
                use codec::Decode;
                T::AccountId::decode(&mut &0u64.to_le_bytes()[..]).unwrap()
            };
            fund_account::<T>(&zero_account);
        }

        #[extrinsic_call]
        _(RawOrigin::Signed(user), SHOP_ID, 1_000u32.into());
    }

    // ==================== call_index(7): set_points_ttl ====================
    #[benchmark]
    fn set_points_ttl() {
        let caller = funded_account::<T>("owner", 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID, 100u32.into());

        assert_eq!(ShopPointsTtl::<T>::get(SHOP_ID), 100u32.into());
    }

    // ==================== call_index(8): expire_points ====================
    #[benchmark]
    fn expire_points() {
        let caller = funded_account::<T>("caller", 0);
        let target: T::AccountId = frame_benchmarking::account("target", 0, 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);
        seed_points_balance::<T>(SHOP_ID, &target, 10_000u32.into());

        // Set expiry to the past so the points are expired.
        let now = frame_system::Pallet::<T>::block_number();
        ShopPointsExpiresAt::<T>::insert(SHOP_ID, &target, now.saturating_sub(1u32.into()));

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID, target);
    }

    // ==================== call_index(9): set_points_max_supply ====================
    #[benchmark]
    fn set_points_max_supply() {
        let caller = funded_account::<T>("owner", 0);
        setup_mock_state::<T>(&caller);
        seed_points_config::<T>(SHOP_ID);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID, 1_000_000u32.into());

        assert_eq!(ShopPointsMaxSupply::<T>::get(SHOP_ID), 1_000_000u32.into());
    }

    // ==================== call_index(10): continue_cleanup ====================
    #[benchmark]
    fn continue_cleanup() {
        let caller = funded_account::<T>("caller", 0);
        // Simulate an incomplete cleanup with cursor set.
        PointsCleanupCursor::<T>::insert(SHOP_ID, CleanupPhase::ClearingBalances);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), SHOP_ID);

        // Cursor should be removed after cleanup completes (no actual data to clean).
        assert!(!PointsCleanupCursor::<T>::contains_key(SHOP_ID));
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
