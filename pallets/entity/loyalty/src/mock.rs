//! Mock runtime for pallet-entity-loyalty tests

use crate as pallet_entity_loyalty;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU32, Currency},
};
use sp_runtime::{traits::IdentityLookup, BuildStorage, DispatchError};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

type Balance = u128;

// ==================== Thread-local 状态 ====================

thread_local! {
    // Entity 相关
    static ENTITY_OWNERS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
    static ENTITY_ACTIVE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static ENTITY_LOCKED: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static ENTITY_SHOPS: RefCell<HashMap<u64, Vec<u64>>> = RefCell::new(HashMap::new());

    // Shop 相关
    static SHOP_ENTITY: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
    static SHOP_OWNERS: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());
    static SHOP_MANAGERS: RefCell<HashSet<(u64, u64)>> = RefCell::new(HashSet::new());
    static SHOP_STATUS: RefCell<HashMap<u64, pallet_entity_common::ShopOperatingStatus>>
        = RefCell::new(HashMap::new());

    // Token 相关
    static TOKEN_ENABLED: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
    static TOKEN_BALANCES: RefCell<HashMap<(u64, u64), Balance>> = RefCell::new(HashMap::new());
    static REFUND_CALLS: RefCell<Vec<(u64, u64, Balance)>> = RefCell::new(Vec::new());

    // KYC
    static KYC_BLOCKED: RefCell<HashSet<(u64, u64)>> = RefCell::new(HashSet::new());
}

fn clear_thread_locals() {
    ENTITY_OWNERS.with(|m| m.borrow_mut().clear());
    ENTITY_ACTIVE.with(|s| s.borrow_mut().clear());
    ENTITY_LOCKED.with(|s| s.borrow_mut().clear());
    ENTITY_SHOPS.with(|m| m.borrow_mut().clear());
    SHOP_ENTITY.with(|m| m.borrow_mut().clear());
    SHOP_OWNERS.with(|m| m.borrow_mut().clear());
    SHOP_MANAGERS.with(|s| s.borrow_mut().clear());
    SHOP_STATUS.with(|m| m.borrow_mut().clear());
    TOKEN_ENABLED.with(|s| s.borrow_mut().clear());
    TOKEN_BALANCES.with(|m| m.borrow_mut().clear());
    REFUND_CALLS.with(|v| v.borrow_mut().clear());
    KYC_BLOCKED.with(|s| s.borrow_mut().clear());
}

// ==================== 常量 ====================

pub const ENTITY_ID: u64 = 1;
pub const SHOP_ID: u64 = 100;
pub const OWNER: u64 = 10;
pub const BUYER: u64 = 20;
pub const ENTITY_ACCOUNT: u64 = 9001; // entity_id + 9000

// ==================== construct_runtime ====================

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Loyalty: pallet_entity_loyalty,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = frame_system::mocking::MockBlock<Test>;
    type AccountData = pallet_balances::AccountData<Balance>;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type Balance = Balance;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
}

// ==================== Mock Providers ====================

pub struct MockEntityProvider;
impl pallet_entity_common::EntityProvider<u64> for MockEntityProvider {
    fn entity_exists(entity_id: u64) -> bool {
        ENTITY_OWNERS.with(|m| m.borrow().contains_key(&entity_id))
    }
    fn entity_owner(entity_id: u64) -> Option<u64> {
        ENTITY_OWNERS.with(|m| m.borrow().get(&entity_id).copied())
    }
    fn is_entity_active(entity_id: u64) -> bool {
        ENTITY_ACTIVE.with(|s| s.borrow().contains(&entity_id))
    }
    fn is_entity_locked(entity_id: u64) -> bool {
        ENTITY_LOCKED.with(|s| s.borrow().contains(&entity_id))
    }
    fn entity_account(entity_id: u64) -> u64 {
        entity_id + 9000
    }
    fn entity_shops(entity_id: u64) -> sp_std::vec::Vec<u64> {
        ENTITY_SHOPS.with(|m| m.borrow().get(&entity_id).cloned().unwrap_or_default())
    }
    fn entity_status(_entity_id: u64) -> Option<pallet_entity_common::EntityStatus> {
        Some(pallet_entity_common::EntityStatus::Active)
    }
    fn update_entity_stats(_: u64, _: u128, _: u32) -> Result<(), DispatchError> {
        Ok(())
    }
}

pub struct MockShopProvider;
impl pallet_entity_common::ShopProvider<u64> for MockShopProvider {
    fn shop_exists(shop_id: u64) -> bool {
        SHOP_ENTITY.with(|m| m.borrow().contains_key(&shop_id))
    }
    fn shop_entity_id(shop_id: u64) -> Option<u64> {
        SHOP_ENTITY.with(|m| m.borrow().get(&shop_id).copied())
    }
    fn shop_owner(shop_id: u64) -> Option<u64> {
        SHOP_OWNERS.with(|m| m.borrow().get(&shop_id).copied())
    }
    fn is_shop_manager(shop_id: u64, who: &u64) -> bool {
        let is_owner = SHOP_OWNERS.with(|m| m.borrow().get(&shop_id).copied() == Some(*who));
        let is_mgr = SHOP_MANAGERS.with(|s| s.borrow().contains(&(shop_id, *who)));
        is_owner || is_mgr
    }
    fn shop_own_status(shop_id: u64) -> Option<pallet_entity_common::ShopOperatingStatus> {
        SHOP_STATUS.with(|m| m.borrow().get(&shop_id).cloned())
    }
    fn is_shop_active(_shop_id: u64) -> bool {
        true
    }
    fn shop_account(_shop_id: u64) -> u64 {
        0
    }
    fn shop_type(_shop_id: u64) -> Option<pallet_entity_common::ShopType> {
        None
    }
    fn update_shop_stats(_: u64, _: u128, _: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn update_shop_rating(_: u64, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn deduct_operating_fund(_: u64, _: u128) -> Result<(), DispatchError> {
        Ok(())
    }
    fn operating_balance(_: u64) -> u128 {
        0
    }
}

pub struct MockTokenProvider;
impl pallet_entity_common::EntityTokenProvider<u64, Balance> for MockTokenProvider {
    fn is_token_enabled(entity_id: u64) -> bool {
        TOKEN_ENABLED.with(|s| s.borrow().contains(&entity_id))
    }
    fn token_balance(entity_id: u64, holder: &u64) -> Balance {
        TOKEN_BALANCES.with(|m| m.borrow().get(&(entity_id, *holder)).copied().unwrap_or(0))
    }
    fn reward_on_purchase(_: u64, _: &u64, _: Balance) -> Result<Balance, DispatchError> {
        Ok(0)
    }
    fn redeem_for_discount(_: u64, _: &u64, _: Balance) -> Result<Balance, DispatchError> {
        Ok(0)
    }
    fn transfer(_: u64, _: &u64, _: &u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn reserve(_: u64, _: &u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn unreserve(_: u64, _: &u64, _: Balance) -> Balance {
        0
    }
    fn repatriate_reserved(_: u64, _: &u64, _: &u64, _: Balance) -> Result<Balance, DispatchError> {
        Ok(0)
    }
    fn get_token_type(_: u64) -> pallet_entity_common::TokenType {
        pallet_entity_common::TokenType::default()
    }
    fn total_supply(_: u64) -> Balance {
        0
    }
    fn governance_burn(_: u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }

    fn refund_discount_tokens(
        entity_id: u64,
        buyer: &u64,
        tokens: Balance,
    ) -> Result<(), DispatchError> {
        REFUND_CALLS.with(|v| v.borrow_mut().push((entity_id, *buyer, tokens)));
        // 模拟 mint 回 token
        TOKEN_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let bal = map.entry((entity_id, *buyer)).or_insert(0);
            *bal += tokens;
        });
        Ok(())
    }
}

pub struct MockParticipationGuard;
impl pallet_commission_common::ParticipationGuard<u64> for MockParticipationGuard {
    fn can_participate(entity_id: u64, account: &u64) -> bool {
        !KYC_BLOCKED.with(|s| s.borrow().contains(&(entity_id, *account)))
    }
}

pub struct MockTokenTransferProvider;
impl pallet_commission_common::TokenTransferProvider<u64, u128> for MockTokenTransferProvider {
    fn token_balance_of(_: u64, _: &u64) -> u128 {
        0
    }
    fn token_transfer(_: u64, _: &u64, _: &u64, _: u128) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ==================== Pallet Config ====================

impl pallet_entity_loyalty::Config for Test {
    type Currency = Balances;
    type ShopProvider = MockShopProvider;
    type EntityProvider = MockEntityProvider;
    type TokenProvider = MockTokenProvider;
    type ParticipationGuard = MockParticipationGuard;
    type TokenBalance = u128;
    type TokenTransferProvider = MockTokenTransferProvider;
    type MaxPointsNameLength = ConstU32<32>;
    type MaxPointsSymbolLength = ConstU32<8>;
    type WeightInfo = ();
}

// ==================== Helpers ====================

pub fn fund(account: u64, amount: Balance) {
    let _ = <pallet_balances::Pallet<Test> as Currency<u64>>::deposit_creating(&account, amount);
}

pub fn balance_of(account: u64) -> Balance {
    <pallet_balances::Pallet<Test> as Currency<u64>>::free_balance(&account)
}

#[allow(dead_code)]
pub fn get_refund_calls() -> Vec<(u64, u64, Balance)> {
    REFUND_CALLS.with(|v| v.borrow().clone())
}

fn setup_default() {
    // Entity 1, 拥有者 OWNER, 活跃
    ENTITY_OWNERS.with(|m| m.borrow_mut().insert(ENTITY_ID, OWNER));
    ENTITY_ACTIVE.with(|s| {
        s.borrow_mut().insert(ENTITY_ID);
    });
    ENTITY_SHOPS.with(|m| m.borrow_mut().insert(ENTITY_ID, vec![SHOP_ID]));

    // Shop 100, 属于 Entity 1, 拥有者 OWNER
    SHOP_ENTITY.with(|m| m.borrow_mut().insert(SHOP_ID, ENTITY_ID));
    SHOP_OWNERS.with(|m| m.borrow_mut().insert(SHOP_ID, OWNER));
    SHOP_STATUS.with(|m| {
        m.borrow_mut()
            .insert(SHOP_ID, pallet_entity_common::ShopOperatingStatus::Active)
    });
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    clear_thread_locals();
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        System::set_block_number(1);
        setup_default();
        // 预充值：OWNER（Entity 账户用 ENTITY_ACCOUNT）、BUYER
        fund(OWNER, 100_000);
        fund(BUYER, 100_000);
        fund(ENTITY_ACCOUNT, 100_000);
    });
    ext
}

/// Register an account as shop manager for benchmarking.
/// Sets up entity + shop + manager state so extrinsic guards pass.
pub fn register_benchmark_shop(shop_id: u64, entity_id: u64, manager: u64) {
    ENTITY_OWNERS.with(|m| m.borrow_mut().insert(entity_id, manager));
    ENTITY_ACTIVE.with(|s| {
        s.borrow_mut().insert(entity_id);
    });
    ENTITY_SHOPS.with(|m| m.borrow_mut().insert(entity_id, vec![shop_id]));
    SHOP_ENTITY.with(|m| m.borrow_mut().insert(shop_id, entity_id));
    SHOP_OWNERS.with(|m| m.borrow_mut().insert(shop_id, manager));
    SHOP_STATUS.with(|m| {
        m.borrow_mut()
            .insert(shop_id, pallet_entity_common::ShopOperatingStatus::Active)
    });
}
