use crate as pallet_entity_member;
use alloc::collections::BTreeSet;
use core::cell::RefCell;
use frame_support::{
    derive_impl,
    traits::{ConstU32, ConstU64},
};
use sp_runtime::BuildStorage;

extern crate alloc;

type Block = frame_system::mocking::MockBlock<Test>;

// Test accounts
pub const OWNER: u64 = 1;
pub const ADMIN: u64 = 2;
pub const ALICE: u64 = 10;
pub const BOB: u64 = 11;
pub const CHARLIE: u64 = 12;
pub const DAVE: u64 = 13;

// Test IDs
pub const ENTITY_1: u64 = 100;
pub const SHOP_1: u64 = 1000;
pub const SHOP_2: u64 = 1001;
pub const INVALID_SHOP: u64 = 9999;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        MemberPallet: pallet_entity_member,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = u128;
}

// ============================================================================
// Mock EntityProvider
// ============================================================================

pub struct MockEntityProvider;

impl pallet_entity_common::EntityProvider<u64> for MockEntityProvider {
    fn entity_exists(entity_id: u64) -> bool {
        entity_id == ENTITY_1
    }

    fn is_entity_active(entity_id: u64) -> bool {
        entity_id == ENTITY_1
    }

    fn entity_status(entity_id: u64) -> Option<pallet_entity_common::EntityStatus> {
        if entity_id == ENTITY_1 {
            Some(pallet_entity_common::EntityStatus::Active)
        } else {
            None
        }
    }

    fn entity_owner(entity_id: u64) -> Option<u64> {
        if entity_id == ENTITY_1 {
            Some(OWNER)
        } else {
            None
        }
    }

    fn entity_account(_entity_id: u64) -> u64 {
        99
    }

    fn update_entity_stats(
        _entity_id: u64,
        _sales_amount: u128,
        _order_count: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }

    fn is_entity_admin(entity_id: u64, account: &u64, _required_permission: u32) -> bool {
        entity_id == ENTITY_1 && *account == ADMIN
    }
    fn is_entity_locked(entity_id: u64) -> bool {
        ENTITY_LOCKED.with(|l| l.borrow().contains(&entity_id))
    }
}

// ============================================================================
// Mock ShopProvider
// ============================================================================

pub struct MockShopProvider;

impl pallet_entity_common::ShopProvider<u64> for MockShopProvider {
    fn shop_exists(shop_id: u64) -> bool {
        shop_id == SHOP_1 || shop_id == SHOP_2
    }

    fn is_shop_active(shop_id: u64) -> bool {
        shop_id == SHOP_1 || shop_id == SHOP_2
    }

    fn shop_entity_id(shop_id: u64) -> Option<u64> {
        match shop_id {
            SHOP_1 | SHOP_2 => Some(ENTITY_1),
            _ => None,
        }
    }

    fn shop_owner(shop_id: u64) -> Option<u64> {
        match shop_id {
            SHOP_1 | SHOP_2 => Some(OWNER),
            _ => None,
        }
    }

    fn shop_account(_shop_id: u64) -> u64 {
        98
    }

    fn shop_type(_shop_id: u64) -> Option<pallet_entity_common::ShopType> {
        Some(pallet_entity_common::ShopType::OnlineStore)
    }

    fn is_shop_manager(_shop_id: u64, _account: &u64) -> bool {
        false
    }

    fn update_shop_stats(
        _shop_id: u64,
        _sales_amount: u128,
        _order_count: u32,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }

    fn update_shop_rating(_shop_id: u64, _rating: u8) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }

    fn deduct_operating_fund(
        _shop_id: u64,
        _amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }

    fn operating_balance(_shop_id: u64) -> u128 {
        0
    }
}

// ============================================================================
// Mock KycChecker
// ============================================================================

thread_local! {
    static KYC_PASSED: RefCell<BTreeSet<(u64, u64)>> = RefCell::new(BTreeSet::new());
    static ENTITY_LOCKED: RefCell<BTreeSet<u64>> = RefCell::new(BTreeSet::new());
}

pub fn set_entity_locked(entity_id: u64) {
    ENTITY_LOCKED.with(|l| l.borrow_mut().insert(entity_id));
}

pub fn set_kyc_passed(entity_id: u64, account: u64) {
    KYC_PASSED.with(|k| k.borrow_mut().insert((entity_id, account)));
}

pub struct MockKycChecker;

impl crate::KycChecker<u64> for MockKycChecker {
    fn is_kyc_passed(entity_id: u64, account: &u64) -> bool {
        KYC_PASSED.with(|k| k.borrow().contains(&(entity_id, *account)))
    }
}

// ============================================================================
// Pallet Config
// ============================================================================

impl pallet_entity_member::Config for Test {
    type EntityProvider = MockEntityProvider;
    type ShopProvider = MockShopProvider;
    type MaxDirectReferrals = ConstU32<50>;
    type MaxCustomLevels = ConstU32<10>;
    type MaxUpgradeRules = ConstU32<10>;
    type MaxUpgradeHistory = ConstU32<50>;
    type PendingMemberExpiry = ConstU64<100>; // 100 blocks for testing
    type KycChecker = MockKycChecker;
    type OnMemberRemoved = ();
    type OnMemberLevelChanged = ();
    type OnMemberTeamChanged = ();
    type OnLevelRemoved = ();
    type WeightInfo = ();
}

// ============================================================================
// Test Helpers
// ============================================================================

pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        System::set_block_number(1);
        // 测试环境默认使用 OPEN 策略，各测试按需覆盖
        pallet_entity_member::EntityMemberPolicy::<Test>::insert(
            ENTITY_1,
            pallet_entity_common::MemberRegistrationPolicy::OPEN,
        );
    });
    ext
}
