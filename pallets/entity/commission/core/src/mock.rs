use crate as pallet_commission_core;
use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_runtime::BuildStorage;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub type Balance = u128;
#[allow(dead_code)]
pub type AccountId = u64;

pub const PLATFORM: u64 = 99;
pub const SELLER: u64 = 10;
pub const BUYER: u64 = 50;
pub const REFERRER: u64 = 88;

pub const TREASURY: u64 = 98;
pub const ENTITY_ID: u64 = 1;
pub const SHOP_ID: u64 = 100;
pub const PRODUCT_ID: u64 = 1000;
pub const GIFT_TARGET: u64 = 77;
pub const ADMIN: u64 = 55;

// ============================================================================
// Thread-local mock state
// ============================================================================

thread_local! {
    static SHOP_ENTITY: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    static ENTITY_OWNERS: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    static SHOP_OWNERS: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    static REFERRERS: RefCell<BTreeMap<(u64, u64), u64>> = RefCell::new(BTreeMap::new());
    static ENTITY_REFERRERS: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    static KYC_BLOCKED: RefCell<std::collections::BTreeSet<(u64, u64)>> = RefCell::new(std::collections::BTreeSet::new());
    /// Mock Token balances: (entity_id, account) → balance
    static TOKEN_BALANCES: RefCell<BTreeMap<(u64, u64), u128>> = RefCell::new(BTreeMap::new());
    /// Mock 非会员集合: (entity_id, account) → 如果存在则 is_member 返回 false
    static NON_MEMBERS: RefCell<std::collections::BTreeSet<(u64, u64)>> = RefCell::new(std::collections::BTreeSet::new());
    /// F1: Mock Admin 权限: (entity_id, account) → permission bitmask
    static ENTITY_ADMINS: RefCell<BTreeMap<(u64, u64), u32>> = RefCell::new(BTreeMap::new());
    static ENTITY_LOCKED: RefCell<std::collections::BTreeSet<u64>> = RefCell::new(std::collections::BTreeSet::new());
    static ENTITY_INACTIVE: RefCell<std::collections::BTreeSet<u64>> = RefCell::new(std::collections::BTreeSet::new());
    /// R8: Mock governance mode per entity (None=0, FullDAO=1)
    static GOVERNANCE_MODES: RefCell<BTreeMap<u64, u8>> = RefCell::new(BTreeMap::new());
    /// Mock NEX 购物余额: (entity_id, account) → balance
    static LOYALTY_SHOPPING_BALANCES: RefCell<BTreeMap<(u64, u64), u128>> = RefCell::new(BTreeMap::new());
    /// Mock NEX 购物余额总额: entity_id → balance
    static LOYALTY_SHOPPING_TOTALS: RefCell<BTreeMap<u64, u128>> = RefCell::new(BTreeMap::new());
    /// Mock Token 购物余额: (entity_id, account) → balance
    static LOYALTY_TOKEN_SHOPPING_BALANCES: RefCell<BTreeMap<(u64, u64), u128>> = RefCell::new(BTreeMap::new());
    /// Mock Token 购物余额总额: entity_id → balance
    static LOYALTY_TOKEN_SHOPPING_TOTALS: RefCell<BTreeMap<u64, u128>> = RefCell::new(BTreeMap::new());
    /// Mock 自定义等级数量: entity_id → count
    static CUSTOM_LEVEL_COUNTS: RefCell<BTreeMap<u64, u8>> = RefCell::new(BTreeMap::new());
    /// Mock 多级分销层数: entity_id → tier_count
    static MOCK_TIER_COUNTS: RefCell<BTreeMap<u64, u16>> = RefCell::new(BTreeMap::new());
    /// Mock 单线深度: entity_id → chain_depth
    static MOCK_SINGLE_LINE_DEPTHS: RefCell<BTreeMap<u64, u16>> = RefCell::new(BTreeMap::new());
    /// Mock 级差深度: entity_id → chain_depth
    static MOCK_LEVEL_DIFF_DEPTHS: RefCell<BTreeMap<u64, u16>> = RefCell::new(BTreeMap::new());
    /// Mock 团队深度: entity_id → chain_depth
    static MOCK_TEAM_DEPTHS: RefCell<BTreeMap<u64, u16>> = RefCell::new(BTreeMap::new());
    /// Mock 推荐人绑定区块号: entity_id → block_number
    static REFERRER_BOUND_AT: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    /// Mock NEX 平台费率 (bps)，默认 100 = 1%
    static MOCK_PLATFORM_FEE_RATE: RefCell<u16> = RefCell::new(100);
    /// Mock min_treasury_threshold: entity_id → threshold
    static MOCK_MIN_TREASURY_THRESHOLD: RefCell<BTreeMap<u64, u128>> = RefCell::new(BTreeMap::new());
    /// Mock NEX/USDT 价格（精度 10^6，默认 1_000_000 = 1 USDT/NEX）
    static MOCK_NEX_USDT_RATE: RefCell<u64> = RefCell::new(1_000_000);
    /// Mock Token/USDT 价格: entity_id → Option<u64>（精度 10^6，None = 价格不可用）
    static MOCK_TOKEN_USDT_PRICE: RefCell<BTreeMap<u64, u64>> = RefCell::new(BTreeMap::new());
    /// HB-WD-01 Mock 跨链派发模式: 0=成功(真 burn), 1=桥拒绝, 2=未接线
    static PAYOUT_MODE: RefCell<u8> = RefCell::new(0);
    /// HB-WD-01 Mock 跨链派发调用记录: (from, evm_chain_id, recipient, amount)
    static PAYOUT_CALLS: RefCell<Vec<(u64, u32, [u8; 20], u128)>> = RefCell::new(Vec::new());
}

pub fn setup_default() {
    SHOP_ENTITY.with(|m| m.borrow_mut().insert(SHOP_ID, ENTITY_ID));
    ENTITY_OWNERS.with(|m| m.borrow_mut().insert(ENTITY_ID, SELLER));
    SHOP_OWNERS.with(|m| m.borrow_mut().insert(SHOP_ID, SELLER));
}

pub fn set_entity_owner(entity_id: u64, owner: u64) {
    ENTITY_OWNERS.with(|m| m.borrow_mut().insert(entity_id, owner));
}

pub fn clear_entity_owner(entity_id: u64) {
    ENTITY_OWNERS.with(|m| m.borrow_mut().remove(&entity_id));
}

pub fn set_shop_entity(shop_id: u64, entity_id: u64) {
    SHOP_ENTITY.with(|m| m.borrow_mut().insert(shop_id, entity_id));
}

pub fn set_shop_owner(shop_id: u64, owner: u64) {
    SHOP_OWNERS.with(|m| m.borrow_mut().insert(shop_id, owner));
}

pub fn set_entity_referrer(entity_id: u64, referrer: u64) {
    ENTITY_REFERRERS.with(|m| m.borrow_mut().insert(entity_id, referrer));
}

pub fn set_referrer(entity_id: u64, account: u64, referrer: u64) {
    REFERRERS.with(|m| m.borrow_mut().insert((entity_id, account), referrer));
}

pub fn set_entity_locked(entity_id: u64) {
    ENTITY_LOCKED.with(|m| m.borrow_mut().insert(entity_id));
}

pub fn set_entity_inactive(entity_id: u64) {
    ENTITY_INACTIVE.with(|m| m.borrow_mut().insert(entity_id));
}

/// R8: 设置 Mock governance mode (0=None, 1=FullDAO)
pub fn set_governance_mode(entity_id: u64, mode: u8) {
    GOVERNANCE_MODES.with(|m| m.borrow_mut().insert(entity_id, mode));
}

pub fn clear_thread_locals() {
    SHOP_ENTITY.with(|m| m.borrow_mut().clear());
    ENTITY_OWNERS.with(|m| m.borrow_mut().clear());
    SHOP_OWNERS.with(|m| m.borrow_mut().clear());
    REFERRERS.with(|m| m.borrow_mut().clear());
    ENTITY_REFERRERS.with(|m| m.borrow_mut().clear());
    KYC_BLOCKED.with(|m| m.borrow_mut().clear());
    TOKEN_BALANCES.with(|m| m.borrow_mut().clear());
    NON_MEMBERS.with(|m| m.borrow_mut().clear());
    ENTITY_ADMINS.with(|m| m.borrow_mut().clear());
    ENTITY_LOCKED.with(|m| m.borrow_mut().clear());
    ENTITY_INACTIVE.with(|m| m.borrow_mut().clear());
    GOVERNANCE_MODES.with(|m| m.borrow_mut().clear());
    LOYALTY_SHOPPING_BALANCES.with(|m| m.borrow_mut().clear());
    LOYALTY_SHOPPING_TOTALS.with(|m| m.borrow_mut().clear());
    LOYALTY_TOKEN_SHOPPING_BALANCES.with(|m| m.borrow_mut().clear());
    LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| m.borrow_mut().clear());
    CUSTOM_LEVEL_COUNTS.with(|m| m.borrow_mut().clear());
    MOCK_TIER_COUNTS.with(|m| m.borrow_mut().clear());
    MOCK_SINGLE_LINE_DEPTHS.with(|m| m.borrow_mut().clear());
    MOCK_LEVEL_DIFF_DEPTHS.with(|m| m.borrow_mut().clear());
    MOCK_TEAM_DEPTHS.with(|m| m.borrow_mut().clear());
    REFERRER_BOUND_AT.with(|m| m.borrow_mut().clear());
    MOCK_PLATFORM_FEE_RATE.with(|r| *r.borrow_mut() = 100);
    MOCK_MIN_TREASURY_THRESHOLD.with(|m| m.borrow_mut().clear());
    PAYOUT_MODE.with(|r| *r.borrow_mut() = 0);
    PAYOUT_CALLS.with(|c| c.borrow_mut().clear());
}

/// HB-WD-01: 设置 Mock 跨链派发模式（0=成功并真 burn，1=桥拒绝，2=未接线）
pub fn set_payout_mode(mode: u8) {
    PAYOUT_MODE.with(|r| *r.borrow_mut() = mode);
}

/// HB-WD-01: 读取 Mock 跨链派发调用记录
pub fn payout_calls() -> Vec<(u64, u32, [u8; 20], u128)> {
    PAYOUT_CALLS.with(|c| c.borrow().clone())
}

/// 设置 Mock NEX 平台费率 (bps)
pub fn set_platform_fee_rate(rate: u16) {
    MOCK_PLATFORM_FEE_RATE.with(|r| *r.borrow_mut() = rate);
}

/// 设置 Mock min_treasury_threshold
pub fn set_min_treasury_threshold(entity_id: u64, threshold: u128) {
    MOCK_MIN_TREASURY_THRESHOLD.with(|m| m.borrow_mut().insert(entity_id, threshold));
}

#[allow(dead_code)]
pub fn set_nex_usdt_rate(rate: u64) {
    MOCK_NEX_USDT_RATE.with(|r| *r.borrow_mut() = rate);
}

/// 设置 Mock Token 余额
pub fn set_token_balance(entity_id: u64, account: u64, balance: u128) {
    TOKEN_BALANCES.with(|m| m.borrow_mut().insert((entity_id, account), balance));
}

/// 读取 Mock Token 余额
pub fn get_token_balance(entity_id: u64, account: u64) -> u128 {
    TOKEN_BALANCES.with(|m| m.borrow().get(&(entity_id, account)).copied().unwrap_or(0))
}

/// 设置 Mock 自定义等级数量（level_id 有效范围为 1..=count）
pub fn set_custom_level_count(entity_id: u64, count: u8) {
    CUSTOM_LEVEL_COUNTS.with(|m| m.borrow_mut().insert(entity_id, count));
}

// ============================================================================
// Mock Providers
// ============================================================================

pub struct MockShopProvider;

impl pallet_entity_common::ShopProvider<u64> for MockShopProvider {
    fn shop_exists(shop_id: u64) -> bool {
        SHOP_ENTITY.with(|m| m.borrow().contains_key(&shop_id))
    }
    fn is_shop_active(_: u64) -> bool {
        true
    }
    fn shop_entity_id(shop_id: u64) -> Option<u64> {
        SHOP_ENTITY.with(|m| m.borrow().get(&shop_id).copied())
    }
    fn shop_owner(shop_id: u64) -> Option<u64> {
        SHOP_OWNERS.with(|m| m.borrow().get(&shop_id).copied())
    }
    fn shop_account(_: u64) -> u64 {
        0
    }
    fn shop_type(_: u64) -> Option<pallet_entity_common::ShopType> {
        None
    }
    fn is_shop_manager(_: u64, _: &u64) -> bool {
        false
    }
    fn update_shop_stats(_: u64, _: u128, _: u32) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn update_shop_rating(_: u64, _: u8) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn deduct_operating_fund(_: u64, _: u128) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn operating_balance(_: u64) -> u128 {
        0
    }
}

pub struct MockEntityProvider;

impl pallet_entity_common::EntityProvider<u64> for MockEntityProvider {
    fn entity_exists(entity_id: u64) -> bool {
        ENTITY_OWNERS.with(|m| m.borrow().contains_key(&entity_id))
    }
    fn is_entity_active(entity_id: u64) -> bool {
        !ENTITY_INACTIVE.with(|m| m.borrow().contains(&entity_id))
    }
    fn entity_status(_: u64) -> Option<pallet_entity_common::EntityStatus> {
        None
    }
    fn entity_owner(entity_id: u64) -> Option<u64> {
        ENTITY_OWNERS.with(|m| m.borrow().get(&entity_id).copied())
    }
    fn entity_account(entity_id: u64) -> u64 {
        entity_id + 9000
    }
    fn update_entity_stats(_: u64, _: u128, _: u32) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn is_entity_admin(entity_id: u64, account: &u64, required_permission: u32) -> bool {
        ENTITY_ADMINS.with(|m| {
            m.borrow()
                .get(&(entity_id, *account))
                .map(|perms| perms & required_permission == required_permission)
                .unwrap_or(false)
        })
    }
    fn is_entity_locked(entity_id: u64) -> bool {
        ENTITY_LOCKED.with(|m| m.borrow().contains(&entity_id))
    }
}

pub struct MockMemberProvider;

impl pallet_commission_common::MemberProvider<u64> for MockMemberProvider {
    fn is_member(entity_id: u64, account: &u64) -> bool {
        !NON_MEMBERS.with(|m| m.borrow().contains(&(entity_id, *account)))
    }
    fn get_referrer(entity_id: u64, account: &u64) -> Option<u64> {
        REFERRERS.with(|r| r.borrow().get(&(entity_id, *account)).copied())
    }
    fn get_member_stats(_: u64, _: &u64) -> pallet_entity_common::MemberStats {
        pallet_entity_common::MemberStats {
            direct_referrals: 0,
            team_size: 0,
            spend: pallet_entity_common::MemberSpendStats {
                total_spent: 0,
                upgrade_eligible_spent: 0,
            },
        }
    }
    fn uses_custom_levels(entity_id: u64) -> bool {
        CUSTOM_LEVEL_COUNTS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0)) > 0
    }
    fn custom_level_id(_: u64, _: &u64) -> u8 {
        0
    }
    fn get_level_commission_bonus(_: u64, _: u8) -> u16 {
        0
    }
    fn custom_level_count(entity_id: u64) -> u8 {
        CUSTOM_LEVEL_COUNTS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
    fn auto_register(
        entity_id: u64,
        account: &u64,
        referrer: Option<u64>,
    ) -> Result<(), sp_runtime::DispatchError> {
        // 注册后从非会员集合移除
        NON_MEMBERS.with(|m| m.borrow_mut().remove(&(entity_id, *account)));
        if let Some(r) = referrer {
            REFERRERS.with(|m| m.borrow_mut().insert((entity_id, *account), r));
        }
        Ok(())
    }
    fn set_custom_levels_enabled(_: u64, _: bool) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn set_upgrade_mode(_: u64, _: u8) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn add_custom_level(
        _: u64,
        _: u8,
        _: &[u8],
        _: u128,
        _: u16,
        _: u16,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn update_custom_level(
        _: u64,
        _: u8,
        _: Option<&[u8]>,
        _: Option<u128>,
        _: Option<u16>,
        _: Option<u16>,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn remove_custom_level(_: u64, _: u8) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
}

// R8: Mock GovernanceProvider
pub struct MockGovernanceProvider;

impl pallet_entity_common::GovernanceProvider for MockGovernanceProvider {
    fn governance_mode(entity_id: u64) -> pallet_entity_common::GovernanceMode {
        GOVERNANCE_MODES.with(|m| match m.borrow().get(&entity_id).copied().unwrap_or(0) {
            1 => pallet_entity_common::GovernanceMode::FullDAO,
            _ => pallet_entity_common::GovernanceMode::None,
        })
    }
    fn has_active_proposals(_entity_id: u64) -> bool {
        false
    }
    fn is_governance_locked(entity_id: u64) -> bool {
        ENTITY_LOCKED.with(|m| m.borrow().contains(&entity_id))
    }
    fn is_governance_paused(_entity_id: u64) -> bool {
        false
    }
}

pub struct MockEntityReferrerProvider;

impl pallet_commission_common::EntityReferrerProvider<u64> for MockEntityReferrerProvider {
    fn entity_referrer(entity_id: u64) -> Option<u64> {
        ENTITY_REFERRERS.with(|m| m.borrow().get(&entity_id).copied())
    }
    fn referrer_bound_at(entity_id: u64) -> Option<u64> {
        REFERRER_BOUND_AT.with(|m| m.borrow().get(&entity_id).copied())
    }
}

pub struct MockTokenTransferProvider;

impl pallet_commission_common::TokenTransferProvider<u64, u128> for MockTokenTransferProvider {
    fn token_balance_of(entity_id: u64, who: &u64) -> u128 {
        TOKEN_BALANCES.with(|m| m.borrow().get(&(entity_id, *who)).copied().unwrap_or(0))
    }

    fn token_transfer(
        entity_id: u64,
        from: &u64,
        to: &u64,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        TOKEN_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let from_balance = map.get(&(entity_id, *from)).copied().unwrap_or(0);
            if from_balance < amount {
                return Err(sp_runtime::DispatchError::Other("InsufficientTokenBalance"));
            }
            map.insert((entity_id, *from), from_balance - amount);
            let to_balance = map.get(&(entity_id, *to)).copied().unwrap_or(0);
            map.insert((entity_id, *to), to_balance + amount);
            Ok(())
        })
    }
}

// ============================================================================
// Mock LoyaltyProvider (NEX 购物余额)
// ============================================================================

pub struct MockLoyaltyProvider;

/// 设置 Mock 购物余额
pub fn set_loyalty_shopping_balance(entity_id: u64, account: u64, balance: u128) {
    LOYALTY_SHOPPING_BALANCES.with(|m| m.borrow_mut().insert((entity_id, account), balance));
}

/// 读取 Mock 购物余额
pub fn get_loyalty_shopping_balance(entity_id: u64, account: u64) -> u128 {
    LOYALTY_SHOPPING_BALANCES.with(|m| m.borrow().get(&(entity_id, account)).copied().unwrap_or(0))
}

/// 设置 Mock 购物余额总额
pub fn set_loyalty_shopping_total(entity_id: u64, total: u128) {
    LOYALTY_SHOPPING_TOTALS.with(|m| m.borrow_mut().insert(entity_id, total));
}

/// 读取 Mock 购物余额总额
pub fn get_loyalty_shopping_total(entity_id: u64) -> u128 {
    LOYALTY_SHOPPING_TOTALS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
}

impl pallet_entity_common::LoyaltyReadPort<u64, u128> for MockLoyaltyProvider {
    fn is_token_enabled(_entity_id: u64) -> bool {
        false
    }
    fn token_discount_balance(_: u64, _: &u64) -> u128 {
        0
    }
    fn shopping_balance(entity_id: u64, who: &u64) -> u128 {
        LOYALTY_SHOPPING_BALANCES.with(|m| m.borrow().get(&(entity_id, *who)).copied().unwrap_or(0))
    }
    fn shopping_total(entity_id: u64) -> u128 {
        LOYALTY_SHOPPING_TOTALS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

impl pallet_entity_common::LoyaltyWritePort<u64, u128> for MockLoyaltyProvider {
    fn redeem_for_discount(_: u64, _: &u64, _: u128) -> Result<u128, sp_runtime::DispatchError> {
        Ok(0)
    }
    fn consume_shopping_balance(
        entity_id: u64,
        account: &u64,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        // KYC 检查
        if KYC_BLOCKED.with(|s| s.borrow().contains(&(entity_id, *account))) {
            return Err(sp_runtime::DispatchError::Other(
                "ParticipationRequirementNotMet",
            ));
        }
        LOYALTY_SHOPPING_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let balance = map.get(&(entity_id, *account)).copied().unwrap_or(0);
            if balance < amount {
                return Err(sp_runtime::DispatchError::Other(
                    "InsufficientShoppingBalance",
                ));
            }
            map.insert((entity_id, *account), balance - amount);
            Ok(())
        })?;
        LOYALTY_SHOPPING_TOTALS.with(|m| {
            let mut map = m.borrow_mut();
            let total = map.get(&entity_id).copied().unwrap_or(0);
            map.insert(entity_id, total.saturating_sub(amount));
        });
        // NEX 转账: entity_account → account
        let entity_account = entity_id + 9000;
        let entity_balance = <pallet_balances::Pallet<Test> as frame_support::traits::Currency<
            u64,
        >>::free_balance(&entity_account);
        if entity_balance >= amount {
            let _ =
                <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::transfer(
                    &entity_account,
                    account,
                    amount,
                    frame_support::traits::ExistenceRequirement::KeepAlive,
                );
        }
        Ok(())
    }
    fn reward_on_purchase(_: u64, _: &u64, _: u128) -> Result<u128, sp_runtime::DispatchError> {
        Ok(0)
    }
    fn credit_shopping_balance(
        entity_id: u64,
        who: &u64,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        if amount == 0 {
            return Ok(());
        }
        LOYALTY_SHOPPING_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let balance = map.get(&(entity_id, *who)).copied().unwrap_or(0);
            map.insert((entity_id, *who), balance + amount);
        });
        LOYALTY_SHOPPING_TOTALS.with(|m| {
            let mut map = m.borrow_mut();
            let total = map.get(&entity_id).copied().unwrap_or(0);
            map.insert(entity_id, total + amount);
        });
        Ok(())
    }
    fn rollback_shopping_balance(
        _: u64,
        _: &u64,
        _: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn rollback_token_discount(_: u64, _: &u64, _: u128) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
    fn forfeit_all_shopping_balances(entity_id: u64) {
        LOYALTY_SHOPPING_BALANCES.with(|m| {
            m.borrow_mut().retain(|&(eid, _), _| eid != entity_id);
        });
        LOYALTY_SHOPPING_TOTALS.with(|m| {
            m.borrow_mut().remove(&entity_id);
        });
    }
    fn forfeit_shopping_balance(
        entity_id: u64,
        who: &u64,
    ) -> Result<(), sp_runtime::DispatchError> {
        LOYALTY_SHOPPING_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let bal = map.remove(&(entity_id, *who)).unwrap_or(0);
            if bal > 0 {
                LOYALTY_SHOPPING_TOTALS.with(|t| {
                    t.borrow_mut()
                        .entry(entity_id)
                        .and_modify(|v| *v = v.saturating_sub(bal));
                });
            }
        });
        Ok(())
    }
}

/// 设置 Mock Token 购物余额
pub fn set_loyalty_token_shopping_balance(entity_id: u64, account: u64, balance: u128) {
    LOYALTY_TOKEN_SHOPPING_BALANCES.with(|m| m.borrow_mut().insert((entity_id, account), balance));
}

/// 读取 Mock Token 购物余额
pub fn get_loyalty_token_shopping_balance(entity_id: u64, account: u64) -> u128 {
    LOYALTY_TOKEN_SHOPPING_BALANCES
        .with(|m| m.borrow().get(&(entity_id, account)).copied().unwrap_or(0))
}

/// 设置 Mock Token 购物余额总额
pub fn set_loyalty_token_shopping_total(entity_id: u64, total: u128) {
    LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| m.borrow_mut().insert(entity_id, total));
}

/// 读取 Mock Token 购物余额总额
pub fn get_loyalty_token_shopping_total(entity_id: u64) -> u128 {
    LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
}

impl pallet_entity_common::LoyaltyTokenReadPort<u64, u128> for MockLoyaltyProvider {
    fn token_shopping_balance(entity_id: u64, who: &u64) -> u128 {
        LOYALTY_TOKEN_SHOPPING_BALANCES
            .with(|m| m.borrow().get(&(entity_id, *who)).copied().unwrap_or(0))
    }
    fn token_shopping_total(entity_id: u64) -> u128 {
        LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

impl pallet_entity_common::LoyaltyTokenWritePort<u64, u128> for MockLoyaltyProvider {
    fn credit_token_shopping_balance(
        entity_id: u64,
        who: &u64,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        if amount == 0 {
            return Ok(());
        }
        LOYALTY_TOKEN_SHOPPING_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let balance = map.get(&(entity_id, *who)).copied().unwrap_or(0);
            map.insert((entity_id, *who), balance + amount);
        });
        LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| {
            let mut map = m.borrow_mut();
            let total = map.get(&entity_id).copied().unwrap_or(0);
            map.insert(entity_id, total + amount);
        });
        Ok(())
    }
    fn consume_token_shopping_balance(
        entity_id: u64,
        account: &u64,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        // KYC 检查
        if KYC_BLOCKED.with(|s| s.borrow().contains(&(entity_id, *account))) {
            return Err(sp_runtime::DispatchError::Other(
                "ParticipationRequirementNotMet",
            ));
        }
        LOYALTY_TOKEN_SHOPPING_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let balance = map.get(&(entity_id, *account)).copied().unwrap_or(0);
            if balance < amount {
                return Err(sp_runtime::DispatchError::Other(
                    "InsufficientShoppingBalance",
                ));
            }
            map.insert((entity_id, *account), balance - amount);
            Ok(())
        })?;
        LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| {
            let mut map = m.borrow_mut();
            let total = map.get(&entity_id).copied().unwrap_or(0);
            map.insert(entity_id, total.saturating_sub(amount));
        });
        // Token 转账: entity_account → account
        let entity_account = entity_id + 9000;
        TOKEN_BALANCES.with(|m| {
            let mut map = m.borrow_mut();
            let from_balance = map.get(&(entity_id, entity_account)).copied().unwrap_or(0);
            if from_balance >= amount {
                map.insert((entity_id, entity_account), from_balance - amount);
                let to_balance = map.get(&(entity_id, *account)).copied().unwrap_or(0);
                map.insert((entity_id, *account), to_balance + amount);
            }
        });
        Ok(())
    }
    fn forfeit_all_token_shopping_balances(entity_id: u64) {
        LOYALTY_TOKEN_SHOPPING_BALANCES.with(|m| {
            m.borrow_mut().retain(|&(eid, _), _| eid != entity_id);
        });
        LOYALTY_TOKEN_SHOPPING_TOTALS.with(|m| {
            m.borrow_mut().remove(&entity_id);
        });
    }
}

// ============================================================================
// Mock Runtime
// ============================================================================

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        CommissionCore: pallet_commission_core,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = frame_system::mocking::MockBlock<Test>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
}

parameter_types! {
    pub const PlatformAccount: u64 = PLATFORM;
    pub const TreasuryAccount: u64 = TREASURY;
    pub const ReferrerShareBps: u16 = 5000; // 50% of platform fee
    pub const ReferrerProtectionPeriod: u64 = 1000; // 1000 blocks protection period
}

impl pallet_commission_core::Config for Test {
    type Currency = Balances;
    type WeightInfo = ();
    type ShopProvider = MockShopProvider;
    type EntityProvider = MockEntityProvider;
    type GovernanceProvider = MockGovernanceProvider;
    type MemberProvider = MockMemberProvider;
    type ReferralPlugin = ();
    type MultiLevelPlugin = ();
    type MultiLevelStatsRollback = ();
    type LevelDiffPlugin = ();
    type SingleLinePlugin = ();
    type TeamPlugin = ();
    type EntityReferrerProvider = MockEntityReferrerProvider;
    type ReferralWriter = ();
    type MultiLevelWriter = ();
    type LevelDiffWriter = ();
    type TeamWriter = ();
    type PoolRewardWriter = ();
    type PlatformAccount = PlatformAccount;
    type TreasuryAccount = TreasuryAccount;
    type ReferrerShareBps = ReferrerShareBps;
    type MaxCommissionRecordsPerOrder = ConstU32<200>;
    type MaxCustomLevels = ConstU32<10>;
    type ParticipationGuard = MockParticipationGuard;
    type ReferrerProtectionPeriod = ReferrerProtectionPeriod;
    type TokenBalance = u128;
    type TokenReferralPlugin = ();
    type TokenMultiLevelPlugin = ();
    type TokenLevelDiffPlugin = ();
    type TokenSingleLinePlugin = ();
    type TokenTeamPlugin = ();
    type TokenTransferProvider = MockTokenTransferProvider;
    type MaxWithdrawalRecords = ConstU32<50>;
    type MaxMemberOrderIds = ConstU32<100>;
    type MultiLevelQuery = MockMultiLevelQuery;
    type TeamQuery = MockTeamQuery;
    type SingleLineQuery = MockSingleLineQuery;
    type LevelDiffQuery = MockLevelDiffQuery;
    type PoolRewardQuery = ();
    type ReferralQuery = ();
    type Loyalty = MockLoyaltyProvider;
    type LoyaltyToken = MockLoyaltyProvider;
    type PlatformFeeRate = MockFeeConfigProvider;
    type PoolFundingCallback = ();
    type FundProtectionQuery = MockFundProtectionQuery;
    type PricingProvider = MockPricingProvider;
    type TokenPriceProvider = MockTokenPriceProvider;
    type AutoRepurchase = pallet_entity_common::NullAutoRepurchasePort;
    type MinShoppingBalanceTtlBlocks = ConstU32<0>; // 测试中不强制最小 TTL
    type CrossChainPayout = MockCrossChainPayout;
}

// ============================================================================
// Mock ParticipationGuard (H3)
// ============================================================================

pub struct MockParticipationGuard;

impl crate::ParticipationGuard<u64> for MockParticipationGuard {
    fn can_participate(entity_id: u64, account: &u64) -> bool {
        !KYC_BLOCKED.with(|s| s.borrow().contains(&(entity_id, *account)))
    }
}

/// 标记 (entity_id, account) 为不满足参与要求（模拟 mandatory KYC 拒绝）
pub fn block_participation(entity_id: u64, account: u64) {
    KYC_BLOCKED.with(|s| s.borrow_mut().insert((entity_id, account)));
}

/// 解除参与限制
pub fn unblock_participation(entity_id: u64, account: u64) {
    KYC_BLOCKED.with(|s| s.borrow_mut().remove(&(entity_id, account)));
}

// ============================================================================
// Mock CrossChainPayout (HB-WD-01)
// ============================================================================

/// HB-WD-01 Mock 跨链派发端口。
/// - mode 0：成功——真从 `from` burn `amount`（模拟桥 `do_outbound` 的 burn），记录调用，
///   返回固定 commitment `[7u8; 32]`。
/// - mode 1：返回通用错误（模拟桥护栏拒绝）→ extrinsic 映射为 `CrossChainPayoutFailed`。
/// - mode 2：返回与 `()` 默认实现一致的 "cross-chain payout not configured" 错误
///   → extrinsic 映射为 `CrossChainPayoutNotConfigured`。
pub struct MockCrossChainPayout;

impl crate::CrossChainPayout<u64, u128> for MockCrossChainPayout {
    fn payout_native(
        from: &u64,
        evm_chain_id: u32,
        recipient: [u8; 20],
        amount: u128,
        _refund_ctx: &[u8],
    ) -> Result<[u8; 32], sp_runtime::DispatchError> {
        match PAYOUT_MODE.with(|r| *r.borrow()) {
            1 => return Err(sp_runtime::DispatchError::Other("bridge rejected")),
            2 => {
                return Err(sp_runtime::DispatchError::Other(
                    "cross-chain payout not configured",
                ))
            }
            _ => {}
        }
        // 模拟桥的真 burn：从 from 销毁 amount（KeepAlive），TotalIssuance↓
        let negative =
            <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::withdraw(
                from,
                amount,
                frame_support::traits::WithdrawReasons::TRANSFER,
                frame_support::traits::ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| sp_runtime::DispatchError::Other("payout burn failed"))?;
        drop(negative);
        PAYOUT_CALLS.with(|c| {
            c.borrow_mut()
                .push((*from, evm_chain_id, recipient, amount))
        });
        Ok([7u8; 32])
    }
}

// ============================================================================
// Mock FeeConfigProvider (平台费率查询)
// ============================================================================

pub struct MockFeeConfigProvider;

impl pallet_entity_common::FeeConfigProvider for MockFeeConfigProvider {
    fn platform_fee_rate() -> u16 {
        MOCK_PLATFORM_FEE_RATE.with(|r| *r.borrow())
    }
}

pub struct MockFundProtectionQuery;

impl pallet_entity_common::FundProtectionQueryPort for MockFundProtectionQuery {
    fn fund_protection_status(_entity_id: u64) -> Option<(u128, u128, u128, u128)> {
        None
    }

    fn min_treasury_threshold(entity_id: u64) -> u128 {
        MOCK_MIN_TREASURY_THRESHOLD.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

// ============================================================================
// MockPricingProvider
// ============================================================================

pub struct MockPricingProvider;

impl pallet_entity_common::PricingProvider for MockPricingProvider {
    fn get_nex_usdt_price() -> u64 {
        MOCK_NEX_USDT_RATE.with(|r| *r.borrow())
    }
}

// ============================================================================
// MockTokenPriceProvider
// ============================================================================

pub struct MockTokenPriceProvider;

/// 设置 Mock Token USDT 价格（精度 10^6）
pub fn set_token_usdt_price(entity_id: u64, price: u64) {
    MOCK_TOKEN_USDT_PRICE.with(|m| m.borrow_mut().insert(entity_id, price));
}

impl pallet_entity_common::EntityTokenPriceProvider for MockTokenPriceProvider {
    type Balance = u128;
    fn get_token_price(_entity_id: u64) -> Option<u128> {
        None
    }
    fn get_token_price_usdt(entity_id: u64) -> Option<u64> {
        MOCK_TOKEN_USDT_PRICE.with(|m| m.borrow().get(&entity_id).copied())
    }
    fn token_price_confidence(_entity_id: u64) -> u8 {
        80
    }
    fn is_token_price_stale(_entity_id: u64, _max_age_blocks: u32) -> bool {
        false
    }
}

// ============================================================================
// Mock MultiLevelQuery (推荐人免除阈值测试)
// ============================================================================

pub struct MockMultiLevelQuery;

impl pallet_commission_common::MultiLevelQueryProvider<u64> for MockMultiLevelQuery {
    fn activation_progress(
        _: u64,
        _: &u64,
    ) -> Vec<pallet_commission_common::MultiLevelActivationInfo> {
        Vec::new()
    }
    fn is_paused(_: u64) -> bool {
        false
    }
    fn member_stats(_: u64, _: &u64) -> Option<pallet_commission_common::MultiLevelMemberStats> {
        None
    }
    fn tier_count(entity_id: u64) -> u16 {
        MOCK_TIER_COUNTS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

pub fn set_mock_tier_count(entity_id: u64, count: u16) {
    MOCK_TIER_COUNTS.with(|m| m.borrow_mut().insert(entity_id, count));
}

pub fn set_referrer_bound_at(entity_id: u64, block: u64) {
    REFERRER_BOUND_AT.with(|m| m.borrow_mut().insert(entity_id, block));
}

// ============================================================================
// Mock SingleLineQuery (推荐链深度测试)
// ============================================================================

pub struct MockSingleLineQuery;

impl pallet_commission_common::SingleLineQueryProvider<u64> for MockSingleLineQuery {
    fn position(_: u64, _: &u64) -> Option<u32> {
        None
    }
    fn effective_levels(_: u64, _: &u64) -> Option<(u8, u8)> {
        None
    }
    fn is_enabled(_: u64) -> bool {
        false
    }
    fn queue_length(_: u64) -> u32 {
        0
    }
    fn chain_depth(entity_id: u64) -> u16 {
        MOCK_SINGLE_LINE_DEPTHS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

pub fn set_mock_single_line_depth(entity_id: u64, depth: u16) {
    MOCK_SINGLE_LINE_DEPTHS.with(|m| m.borrow_mut().insert(entity_id, depth));
}

// ============================================================================
// Mock LevelDiffQuery (推荐链深度测试)
// ============================================================================

pub struct MockLevelDiffQuery;

impl pallet_commission_common::LevelDiffQueryProvider for MockLevelDiffQuery {
    fn chain_depth(entity_id: u64) -> u16 {
        MOCK_LEVEL_DIFF_DEPTHS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

pub fn set_mock_level_diff_depth(entity_id: u64, depth: u16) {
    MOCK_LEVEL_DIFF_DEPTHS.with(|m| m.borrow_mut().insert(entity_id, depth));
}

// ============================================================================
// Mock TeamQuery (推荐链深度测试)
// ============================================================================

pub struct MockTeamQuery;

impl pallet_commission_common::TeamQueryProvider<u64, u128> for MockTeamQuery {
    fn matched_tier(_: u64, _: &u64) -> Option<pallet_commission_common::TeamTierInfo<u128>> {
        None
    }
    fn status(_: u64) -> (bool, bool) {
        (false, false)
    }
    fn chain_depth(entity_id: u64) -> u16 {
        MOCK_TEAM_DEPTHS.with(|m| m.borrow().get(&entity_id).copied().unwrap_or(0))
    }
}

pub fn set_mock_team_depth(entity_id: u64, depth: u16) {
    MOCK_TEAM_DEPTHS.with(|m| m.borrow_mut().insert(entity_id, depth));
}

/// F1: 设置 Mock Admin 权限
pub fn set_entity_admin(entity_id: u64, account: u64, permissions: u32) {
    ENTITY_ADMINS.with(|m| m.borrow_mut().insert((entity_id, account), permissions));
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
    });
    ext
}

/// 给账户注资
pub fn fund(account: u64, amount: Balance) {
    let _ =
        <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::deposit_creating(
            &account, amount,
        );
}

/// Entity 派生账户
pub fn entity_account(entity_id: u64) -> u64 {
    entity_id + 9000
}

/// Reserve 模式测试辅助: 在 seller 账户上锁定指定金额（模拟 order 侧 reserve）
/// 返回实际锁定金额（受 ED 限制可能小于请求值）
pub fn reserve_seller(account: u64, amount: Balance) -> Balance {
    use frame_support::traits::ReservableCurrency;
    let free =
        <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::free_balance(
            &account,
        );
    let min_balance =
        <pallet_balances::Pallet<Test> as frame_support::traits::Currency<u64>>::minimum_balance();
    let max_reservable = free.saturating_sub(min_balance);
    let target = amount.min(max_reservable);
    if target > 0 {
        <pallet_balances::Pallet<Test> as ReservableCurrency<u64>>::reserve(&account, target)
            .expect("reserve should succeed in test");
    }
    target
}
