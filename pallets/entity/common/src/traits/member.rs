//! Member, Commission handler, and Fund protection traits
//!
//! MemberProvider (+ Query/Write split with blanket impls),
//! CommissionFundGuard, CommissionCloseGuard, CommissionFundBreakdown,
//! EntityTreasuryPort, ShopFundPort, FundProtectionPort, FundProtectionQueryPort,
//! OrderCommissionHandler, TokenOrderCommissionHandler, ShoppingBalanceProvider.

extern crate alloc;

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

// ============================================================================
// 佣金资金保护接口
// ============================================================================

/// 佣金资金保护接口
///
/// 供 Registry / Shop 等模块在从 Entity 账户转出资金时查询已承诺的佣金资金
/// （pending + shopping + pool + refund），防止资金流出侵占用户佣金。
pub trait CommissionFundGuard {
    /// 获取 entity 已承诺的佣金资金总额（pending + shopping + pool + refund）
    fn protected_funds(entity_id: u64) -> u128;
}

/// 空 CommissionFundGuard 实现（无佣金系统时使用）
impl CommissionFundGuard for () {
    fn protected_funds(_: u64) -> u128 {
        0
    }
}

/// Entity 关闭前置佣金检查接口
///
/// CRITICAL-2 审计修复: 供 Registry 在 Entity 关闭前检查是否存在未结清的
/// 佣金资金（pending commissions + shopping balances + pending refunds），
/// 防止关闭后会员资金成为孤儿。
pub trait CommissionCloseGuard {
    /// Entity 是否存在未结清的佣金资金
    ///
    /// 检查项：
    /// - ShopPendingTotal / TokenPendingTotal（待提佣金）
    /// - ShoppingTotal / TokenShoppingTotal（购物余额）
    /// - PendingRefundTotal（待重试退款）
    fn has_uncommitted_funds(entity_id: u64) -> bool;
}

/// 空 CommissionCloseGuard 实现（无佣金系统时使用）
impl CommissionCloseGuard for () {
    fn has_uncommitted_funds(_: u64) -> bool {
        false
    }
}

// ============================================================================
// 佣金资金明细查询（供 Runtime API 返回 protected 分项）
// ============================================================================

/// 佣金资金明细查询接口
///
/// `CommissionFundGuard` 只返回总计，此接口返回分项明细，
/// 供 Registry Runtime API (`get_entity_funds`) 向前端展示各类已承诺资金。
pub trait CommissionFundBreakdown {
    /// 返回 (pending_commission, shopping_balance, unallocated_pool, pending_refund)
    ///
    /// - `pending_commission`: 会员待提取佣金（ShopPendingTotal）
    /// - `shopping_balance`: 复购产生的购物余额（ShopShoppingTotal）
    /// - `unallocated_pool`: 未分配佣金沉淀池（UnallocatedPool）
    /// - `pending_refund`: 退款失败待重试（PendingRefundTotal）
    fn protected_funds_breakdown(entity_id: u64) -> (u128, u128, u128, u128);
}

/// 空 CommissionFundBreakdown 实现
impl CommissionFundBreakdown for () {
    fn protected_funds_breakdown(_: u64) -> (u128, u128, u128, u128) {
        (0, 0, 0, 0)
    }
}

// ============================================================================
// 资金保护配置查询（供 Runtime API 聚合展示）
// ============================================================================

/// 资金保护配置查询接口
///
/// 供 Registry 模块的 `get_entity_funds` Runtime API 聚合展示
/// 治理模块设置的资金保护规则和当日支出追踪。
pub trait FundProtectionQueryPort {
    /// 查询 Entity 资金保护配置及当日已支出
    ///
    /// 返回 `Some((min_treasury_threshold, max_single_spend, max_daily_spend, daily_spent))`
    /// 或 `None`（未配置）。所有金额为 u128（NEX 精度）。
    fn fund_protection_status(entity_id: u64) -> Option<(u128, u128, u128, u128)>;

    /// 查询 Entity 的 min_treasury_threshold（u128 NEX 精度）
    /// 返回 0 表示未配置或无下限。
    fn min_treasury_threshold(entity_id: u64) -> u128 {
        Self::fund_protection_status(entity_id)
            .map(|(min, _, _, _)| min)
            .unwrap_or(0)
    }
}

/// 空 FundProtectionQueryPort 实现
impl FundProtectionQueryPort for () {
    fn fund_protection_status(_: u64) -> Option<(u128, u128, u128, u128)> {
        None
    }
}

// ============================================================================
// Phase 1 新增: 资金规则统一 Port
// ============================================================================

/// Entity 资金库查询接口
///
/// 供 loyalty/commission 等模块查询 Entity 级资金状态，
/// 用于资金保护和阈值判断。
pub trait EntityTreasuryPort {
    /// 获取 Entity 派生账户的可用余额（NEX）
    fn treasury_balance(entity_id: u64) -> u128;

    /// 检查 Entity 资金是否充足（高于安全阈值）
    fn is_treasury_sufficient(entity_id: u64, required: u128) -> bool {
        Self::treasury_balance(entity_id) >= required
    }
}

/// 空 EntityTreasuryPort 实现
impl EntityTreasuryPort for () {
    fn treasury_balance(_: u64) -> u128 {
        0
    }
}

/// Shop 资金查询接口
///
/// 供 product/loyalty 等模块查询 Shop 运营资金状态。
pub trait ShopFundPort {
    /// 获取 Shop 运营资金余额
    fn operating_balance(shop_id: u64) -> u128;

    /// 扣减 Shop 运营资金
    fn deduct_operating_fund(shop_id: u64, amount: u128) -> Result<(), DispatchError>;
}

/// 空 ShopFundPort 实现
impl ShopFundPort for () {
    fn operating_balance(_: u64) -> u128 {
        0
    }
    fn deduct_operating_fund(_: u64, _: u128) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 资金保护查询接口
///
/// 供需要保护已承诺资金的模块实现（commission/loyalty），
/// 防止 Shop 运营扣费侵占用户已承诺的佣金/购物余额。
pub trait FundProtectionPort {
    /// 获取已承诺不可动用的资金总额
    fn protected_funds(entity_id: u64) -> u128;
}

/// 空 FundProtectionPort 实现
impl FundProtectionPort for () {
    fn protected_funds(_: u64) -> u128 {
        0
    }
}

/// 订单佣金处理接口
///
/// 供 Transaction 模块在订单完成时触发佣金计算，
/// 无需直接依赖 commission 模块。
pub trait OrderCommissionHandler<AccountId, Balance> {
    /// 订单完成时处理佣金
    fn on_order_completed(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &AccountId,
        order_amount: Balance,
        platform_fee: Balance,
    ) -> Result<(), DispatchError>;

    /// 订单取消/退款时撤销佣金
    fn on_order_cancelled(order_id: u64) -> Result<(), DispatchError>;
}

/// 空佣金处理（无佣金系统时使用）
impl<AccountId, Balance> OrderCommissionHandler<AccountId, Balance> for () {
    fn on_order_completed(
        _: u64,
        _: u64,
        _: u64,
        _: &AccountId,
        _: Balance,
        _: Balance,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn on_order_cancelled(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// TokenOrderCommissionHandler — Token 订单佣金处理接口
// ============================================================================

/// Token 订单佣金处理接口
///
/// 供 Order 模块在 Entity Token 订单完成时触发 Token 佣金计算，
/// 无需直接依赖 commission 模块。使用 u128 避免泛型膨胀。
pub trait TokenOrderCommissionHandler<AccountId> {
    /// Token 订单完成时处理 Token 佣金（双源：token_platform_fee 为 Pool A 资金）
    fn on_token_order_completed(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &AccountId,
        token_amount: u128,
        token_platform_fee: u128,
    ) -> Result<(), DispatchError>;

    /// Token 订单取消时撤销 Token 佣金
    fn on_token_order_cancelled(order_id: u64) -> Result<(), DispatchError>;

    /// 获取 Entity 级 Token 平台费率（bps，供 entity-order 计算拆分）
    fn token_platform_fee_rate(entity_id: u64) -> u16;

    /// 获取 Entity 派生账户（Token 平台费转入目标）
    ///
    /// **已废弃**: 与 `EntityProvider::entity_account()` 功能重复。
    /// 新代码应通过 `EntityProvider` 获取 Entity 派生账户。
    fn entity_account(entity_id: u64) -> AccountId;
}

/// 空 Token 佣金处理（无 Token 佣金系统时使用）
impl<AccountId: Default> TokenOrderCommissionHandler<AccountId> for () {
    fn on_token_order_completed(
        _: u64,
        _: u64,
        _: u64,
        _: &AccountId,
        _: u128,
        _: u128,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn on_token_order_cancelled(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn token_platform_fee_rate(_: u64) -> u16 {
        0
    }
    fn entity_account(_: u64) -> AccountId {
        AccountId::default()
    }
}

// ============================================================================
// 购物余额接口
// ============================================================================

/// 购物余额提供者（供 Transaction 模块在下单时抵扣购物余额）
///
/// `consume_shopping_balance` 会：
/// 1. 扣减会员购物余额记账（MemberShoppingBalance / ShopShoppingTotal）
/// 2. 将等额 NEX 从 Entity 账户转入会员钱包（会员随后通过 Escrow 锁定）
pub trait ShoppingBalanceProvider<AccountId, Balance> {
    /// 查询会员在指定实体的购物余额
    fn shopping_balance(entity_id: u64, account: &AccountId) -> Balance;
    /// 消费购物余额：扣减记账 + 将 NEX 从 Entity 账户转入会员钱包
    fn consume_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;
}

/// 空购物余额提供者（无佣金系统时使用）
impl<AccountId, Balance: Default> ShoppingBalanceProvider<AccountId, Balance> for () {
    fn shopping_balance(_: u64, _: &AccountId) -> Balance {
        Balance::default()
    }
    fn consume_shopping_balance(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// 会员服务接口（统一定义）
// ============================================================================

/// 会员等级信息（无泛型，适合跨模块 trait 返回）
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MemberLevelInfo {
    /// 等级 ID
    pub level_id: u8,
    /// 等级名称（UTF-8 字节）
    pub name: sp_std::vec::Vec<u8>,
    /// 升级阈值（USDT 累计消费，精度 10^6）
    pub threshold: u128,
    /// 折扣率（基点）
    pub discount_rate: u16,
    /// 返佣加成（基点）
    pub commission_bonus: u16,
}

/// Member spend stats with explicit semantics.
/// 显式语义的会员消费统计。
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MemberSpendStats {
    /// 累计总消费（USDT 精度 10^6）
    pub total_spent: u128,
    /// 升级/奖励有效消费（USDT 精度 10^6）
    pub upgrade_eligible_spent: u128,
}

/// Member aggregate stats with named fields.
/// 使用命名字段的会员聚合统计。
#[derive(Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MemberStats {
    /// 直推人数
    pub direct_referrals: u32,
    /// 团队人数
    pub team_size: u32,
    /// 消费统计
    pub spend: MemberSpendStats,
}

/// 会员服务接口（供返佣、治理、订单等模块统一调用）
///
/// 由 `pallet-entity-member` 实现，通过 runtime 桥接到各消费方。
/// 合并了原 `pallet-entity-member::MemberProvider` 和 `pallet-commission-common::MemberProvider`
/// 两个重复定义，消除运行时手动桥接的冗余。
pub trait MemberProvider<AccountId> {
    // ==================== 只读查询 ====================

    /// 检查是否为实体会员
    fn is_member(entity_id: u64, account: &AccountId) -> bool;

    /// 获取推荐人
    fn get_referrer(entity_id: u64, account: &AccountId) -> Option<AccountId>;

    /// 获取自定义等级 ID
    fn custom_level_id(entity_id: u64, account: &AccountId) -> u8;

    /// 获取有效等级（考虑过期）
    fn get_effective_level(entity_id: u64, account: &AccountId) -> u8 {
        Self::custom_level_id(entity_id, account)
    }

    /// 获取等级折扣率
    fn get_level_discount(entity_id: u64, level_id: u8) -> u16 {
        let _ = (entity_id, level_id);
        0
    }

    /// 获取等级返佣加成
    fn get_level_commission_bonus(entity_id: u64, level_id: u8) -> u16;

    /// 检查实体是否使用自定义等级
    fn uses_custom_levels(entity_id: u64) -> bool;

    /// 获取会员统计信息（命名字段，避免 tuple 语义歧义）
    fn get_member_stats(entity_id: u64, account: &AccountId) -> MemberStats;

    /// 查询 Entity 的会员总数
    fn member_count(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }

    /// 查询会员是否被封禁
    fn is_banned(entity_id: u64, account: &AccountId) -> bool {
        let _ = (entity_id, account);
        false
    }

    /// 查询会员是否已激活（如首次消费达标）
    ///
    /// "激活"与"未封禁"是两个独立概念：
    /// - 新注册但未消费的会员 → is_banned=false 但 is_activated=false
    /// - 未激活会员不应获得佣金
    fn is_activated(entity_id: u64, account: &AccountId) -> bool {
        let _ = (entity_id, account);
        true // 默认实现: 向后兼容，所有会员视为已激活
    }

    /// F6: 查询会员是否处于活跃状态（非冻结/非封禁/非过期）
    fn is_member_active(entity_id: u64, account: &AccountId) -> bool {
        // 默认实现: 非 banned 即为 active（向后兼容）
        !Self::is_banned(entity_id, account)
    }

    /// F5: 获取推荐关系建立时间（区块号，0 = 未知/不支持）
    fn referral_registered_at(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }

    /// 查询 Entity 是否启用了 REFERRAL_REQUIRED 策略
    ///
    /// 启用后：已注册但无推荐人的会员不能下单，也不能被他人绑定为推荐人。
    fn requires_referral(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }

    /// F7: 获取已完成的成功订单数（排除取消/退款的订单）
    fn completed_order_count(entity_id: u64, account: &AccountId) -> u32 {
        let _ = (entity_id, account);
        0
    }

    /// 查询会员最后活跃时间（区块号，0 = 未知/非会员）
    fn last_active_at(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }

    /// 获取会员当前有效等级的完整信息
    fn member_level(entity_id: u64, account: &AccountId) -> Option<MemberLevelInfo> {
        let _ = (entity_id, account);
        None
    }

    /// 查询自定义等级数量
    fn custom_level_count(entity_id: u64) -> u8 {
        let _ = entity_id;
        0
    }

    /// 检查指定 level_id 是否存在于 Entity 的等级系统中
    ///
    /// 默认实现基于 custom_level_count（等级 ID 为 1..=count 的连续序列）。
    /// 等级系统未启用或 level_id=0 时返回 false。
    fn level_id_exists(entity_id: u64, level_id: u8) -> bool {
        if level_id == 0 {
            return false;
        }
        Self::uses_custom_levels(entity_id) && level_id <= Self::custom_level_count(entity_id)
    }

    /// 查询指定等级的会员数量
    fn member_count_by_level(entity_id: u64, level_id: u8) -> u32 {
        let _ = (entity_id, level_id);
        0
    }

    /// 查询会员总消费（USDT 精度 10^6）
    fn get_member_total_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }

    /// 查询会员升级/奖励有效消费（USDT 精度 10^6）
    fn get_member_upgrade_eligible_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }

    // ==================== #5 补充: 溢出推荐人查询 ====================

    /// 获取真实推荐人（溢出安置时记录的原始推荐人）
    ///
    /// 溢出场景下 `get_referrer` 返回的是实际安置节点，
    /// `get_introduced_by` 返回原始推荐人。无溢出时返回 None。
    fn get_introduced_by(entity_id: u64, account: &AccountId) -> Option<AccountId> {
        let _ = (entity_id, account);
        None
    }

    /// 获取直推会员账户列表
    fn get_direct_referral_accounts(
        entity_id: u64,
        account: &AccountId,
    ) -> alloc::vec::Vec<AccountId> {
        let _ = (entity_id, account);
        alloc::vec::Vec::new()
    }

    // ==================== 会员注册/更新 ====================

    /// 自动注册会员（首次下单时）
    fn auto_register(
        entity_id: u64,
        account: &AccountId,
        referrer: Option<AccountId>,
    ) -> Result<(), DispatchError>;

    /// 更新消费金额（USDT 精度 10^6）
    fn update_spent(
        entity_id: u64,
        account: &AccountId,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, account, amount_usdt);
        Ok(())
    }

    /// 检查订单完成时的升级规则
    fn check_order_upgrade_rules(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, buyer, product_id, amount_usdt);
        Ok(())
    }

    // ==================== 治理写入 ====================

    /// 启用/禁用自定义等级系统
    fn set_custom_levels_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }

    /// 设置升级模式
    fn set_upgrade_mode(entity_id: u64, mode: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, mode);
        Ok(())
    }

    /// 添加自定义等级
    fn add_custom_level(
        entity_id: u64,
        level_id: u8,
        name: &[u8],
        threshold: u128,
        discount_rate: u16,
        commission_bonus: u16,
    ) -> Result<(), DispatchError> {
        let _ = (
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        );
        Ok(())
    }

    /// 更新自定义等级
    fn update_custom_level(
        entity_id: u64,
        level_id: u8,
        name: Option<&[u8]>,
        threshold: Option<u128>,
        discount_rate: Option<u16>,
        commission_bonus: Option<u16>,
    ) -> Result<(), DispatchError> {
        let _ = (
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        );
        Ok(())
    }

    /// 删除自定义等级
    fn remove_custom_level(entity_id: u64, level_id: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, level_id);
        Ok(())
    }

    /// G1: 设置注册策略（治理调用）
    fn set_registration_policy(entity_id: u64, policy_bits: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, policy_bits);
        Ok(())
    }

    /// G1: 设置升级规则系统开关（治理调用）
    fn set_upgrade_rule_system_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }

    // ==================== #6 补充: 治理封禁/移除接口 ====================

    /// 封禁会员（治理调用，禁止参与实体活动）
    fn ban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }

    /// 解除会员封禁（治理调用）
    fn unban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }

    /// 移除会员（治理调用，从实体会员列表中删除）
    fn remove_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }
}

/// 空会员服务提供者（测试用或未启用会员系统时）
pub struct NullMemberProvider;

impl<AccountId> MemberProvider<AccountId> for NullMemberProvider {
    fn is_member(_: u64, _: &AccountId) -> bool {
        false
    }
    fn get_referrer(_: u64, _: &AccountId) -> Option<AccountId> {
        None
    }
    fn custom_level_id(_: u64, _: &AccountId) -> u8 {
        0
    }
    fn get_effective_level(_: u64, _: &AccountId) -> u8 {
        0
    }
    fn get_level_discount(_: u64, _: u8) -> u16 {
        0
    }
    fn get_level_commission_bonus(_: u64, _: u8) -> u16 {
        0
    }
    fn uses_custom_levels(_: u64) -> bool {
        false
    }
    fn get_member_stats(_: u64, _: &AccountId) -> MemberStats {
        MemberStats {
            direct_referrals: 0,
            team_size: 0,
            spend: MemberSpendStats {
                total_spent: 0,
                upgrade_eligible_spent: 0,
            },
        }
    }
    fn auto_register(_: u64, _: &AccountId, _: Option<AccountId>) -> Result<(), DispatchError> {
        Ok(())
    }
    fn update_spent(_: u64, _: &AccountId, _: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn check_order_upgrade_rules(
        _: u64,
        _: &AccountId,
        _: u64,
        _: u64,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_custom_levels_enabled(_: u64, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_upgrade_mode(_: u64, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn add_custom_level(
        _: u64,
        _: u8,
        _: &[u8],
        _: u128,
        _: u16,
        _: u16,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn update_custom_level(
        _: u64,
        _: u8,
        _: Option<&[u8]>,
        _: Option<u128>,
        _: Option<u16>,
        _: Option<u16>,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn remove_custom_level(_: u64, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_registration_policy(_: u64, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_upgrade_rule_system_enabled(_: u64, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// 会员接口职责拆分（MemberProvider 的精简替代）
// ============================================================================

/// 会员只读查询接口（MemberProvider 读取子集）
///
/// 新模块仅需读取会员信息时，应优先使用此 trait。
pub trait MemberQueryProvider<AccountId> {
    fn is_member(entity_id: u64, account: &AccountId) -> bool;
    fn get_referrer(entity_id: u64, account: &AccountId) -> Option<AccountId>;
    fn custom_level_id(entity_id: u64, account: &AccountId) -> u8;
    fn get_effective_level(entity_id: u64, account: &AccountId) -> u8 {
        Self::custom_level_id(entity_id, account)
    }
    fn get_level_discount(entity_id: u64, level_id: u8) -> u16 {
        let _ = (entity_id, level_id);
        0
    }
    fn get_level_commission_bonus(entity_id: u64, level_id: u8) -> u16;
    fn uses_custom_levels(entity_id: u64) -> bool;
    fn get_member_stats(entity_id: u64, account: &AccountId) -> MemberStats;
    fn member_count(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }
    fn is_banned(entity_id: u64, account: &AccountId) -> bool {
        let _ = (entity_id, account);
        false
    }
    fn is_activated(entity_id: u64, account: &AccountId) -> bool {
        let _ = (entity_id, account);
        true
    }
    fn is_member_active(entity_id: u64, account: &AccountId) -> bool {
        !Self::is_banned(entity_id, account)
    }
    fn referral_registered_at(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }
    fn completed_order_count(entity_id: u64, account: &AccountId) -> u32 {
        let _ = (entity_id, account);
        0
    }
    fn last_active_at(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }
    fn member_level(entity_id: u64, account: &AccountId) -> Option<MemberLevelInfo> {
        let _ = (entity_id, account);
        None
    }
    fn custom_level_count(entity_id: u64) -> u8 {
        let _ = entity_id;
        0
    }
    fn level_id_exists(entity_id: u64, level_id: u8) -> bool {
        if level_id == 0 {
            return false;
        }
        Self::uses_custom_levels(entity_id) && level_id <= Self::custom_level_count(entity_id)
    }
    fn member_count_by_level(entity_id: u64, level_id: u8) -> u32 {
        let _ = (entity_id, level_id);
        0
    }
    fn get_member_total_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }
    fn get_member_upgrade_eligible_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }
    fn get_introduced_by(entity_id: u64, account: &AccountId) -> Option<AccountId> {
        let _ = (entity_id, account);
        None
    }
    fn get_direct_referral_accounts(
        entity_id: u64,
        account: &AccountId,
    ) -> alloc::vec::Vec<AccountId> {
        let _ = (entity_id, account);
        alloc::vec::Vec::new()
    }
}

/// 会员写入接口（MemberProvider 写入子集）
///
/// 仅供 order/governance 模块使用。
pub trait MemberWriteProvider<AccountId> {
    fn auto_register(
        entity_id: u64,
        account: &AccountId,
        referrer: Option<AccountId>,
    ) -> Result<(), DispatchError>;
    fn update_spent(
        entity_id: u64,
        account: &AccountId,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, account, amount_usdt);
        Ok(())
    }
    fn check_order_upgrade_rules(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, buyer, product_id, amount_usdt);
        Ok(())
    }
    fn set_custom_levels_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }
    fn set_upgrade_mode(entity_id: u64, mode: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, mode);
        Ok(())
    }
    fn add_custom_level(
        entity_id: u64,
        level_id: u8,
        name: &[u8],
        threshold: u128,
        discount_rate: u16,
        commission_bonus: u16,
    ) -> Result<(), DispatchError> {
        let _ = (
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        );
        Ok(())
    }
    fn update_custom_level(
        entity_id: u64,
        level_id: u8,
        name: Option<&[u8]>,
        threshold: Option<u128>,
        discount_rate: Option<u16>,
        commission_bonus: Option<u16>,
    ) -> Result<(), DispatchError> {
        let _ = (
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        );
        Ok(())
    }
    fn remove_custom_level(entity_id: u64, level_id: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, level_id);
        Ok(())
    }
    fn set_registration_policy(entity_id: u64, policy_bits: u8) -> Result<(), DispatchError> {
        let _ = (entity_id, policy_bits);
        Ok(())
    }
    fn set_upgrade_rule_system_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }
    fn ban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }
    fn unban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }
    fn remove_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        let _ = (entity_id, account);
        Ok(())
    }
}

/// 空会员查询提供者 — `NullMemberProvider` 的类型别名
///
/// 通过 blanket impl，`NullMemberProvider` 自动实现 `MemberQueryProvider`。
pub type NullMemberQueryProvider = NullMemberProvider;

/// 空会员写入提供者 — `NullMemberProvider` 的类型别名
///
/// 通过 blanket impl，`NullMemberProvider` 自动实现 `MemberWriteProvider`。
pub type NullMemberWriteProvider = NullMemberProvider;

// ---- 桥接: MemberProvider 自动实现 MemberQueryProvider / MemberWriteProvider ----

impl<AccountId, T: MemberProvider<AccountId>> MemberQueryProvider<AccountId> for T {
    fn is_member(entity_id: u64, account: &AccountId) -> bool {
        <T as MemberProvider<AccountId>>::is_member(entity_id, account)
    }
    fn get_referrer(entity_id: u64, account: &AccountId) -> Option<AccountId> {
        <T as MemberProvider<AccountId>>::get_referrer(entity_id, account)
    }
    fn custom_level_id(entity_id: u64, account: &AccountId) -> u8 {
        <T as MemberProvider<AccountId>>::custom_level_id(entity_id, account)
    }
    fn get_effective_level(entity_id: u64, account: &AccountId) -> u8 {
        <T as MemberProvider<AccountId>>::get_effective_level(entity_id, account)
    }
    fn get_level_discount(entity_id: u64, level_id: u8) -> u16 {
        <T as MemberProvider<AccountId>>::get_level_discount(entity_id, level_id)
    }
    fn get_level_commission_bonus(entity_id: u64, level_id: u8) -> u16 {
        <T as MemberProvider<AccountId>>::get_level_commission_bonus(entity_id, level_id)
    }
    fn uses_custom_levels(entity_id: u64) -> bool {
        <T as MemberProvider<AccountId>>::uses_custom_levels(entity_id)
    }
    fn get_member_stats(entity_id: u64, account: &AccountId) -> MemberStats {
        <T as MemberProvider<AccountId>>::get_member_stats(entity_id, account)
    }
    fn member_count(entity_id: u64) -> u32 {
        <T as MemberProvider<AccountId>>::member_count(entity_id)
    }
    fn is_banned(entity_id: u64, account: &AccountId) -> bool {
        <T as MemberProvider<AccountId>>::is_banned(entity_id, account)
    }
    fn is_activated(entity_id: u64, account: &AccountId) -> bool {
        <T as MemberProvider<AccountId>>::is_activated(entity_id, account)
    }
    fn is_member_active(entity_id: u64, account: &AccountId) -> bool {
        <T as MemberProvider<AccountId>>::is_member_active(entity_id, account)
    }
    fn referral_registered_at(entity_id: u64, account: &AccountId) -> u64 {
        <T as MemberProvider<AccountId>>::referral_registered_at(entity_id, account)
    }
    fn completed_order_count(entity_id: u64, account: &AccountId) -> u32 {
        <T as MemberProvider<AccountId>>::completed_order_count(entity_id, account)
    }
    fn last_active_at(entity_id: u64, account: &AccountId) -> u64 {
        <T as MemberProvider<AccountId>>::last_active_at(entity_id, account)
    }
    fn member_level(entity_id: u64, account: &AccountId) -> Option<MemberLevelInfo> {
        <T as MemberProvider<AccountId>>::member_level(entity_id, account)
    }
    fn custom_level_count(entity_id: u64) -> u8 {
        <T as MemberProvider<AccountId>>::custom_level_count(entity_id)
    }
    fn level_id_exists(entity_id: u64, level_id: u8) -> bool {
        <T as MemberProvider<AccountId>>::level_id_exists(entity_id, level_id)
    }
    fn member_count_by_level(entity_id: u64, level_id: u8) -> u32 {
        <T as MemberProvider<AccountId>>::member_count_by_level(entity_id, level_id)
    }
    fn get_member_total_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        <T as MemberProvider<AccountId>>::get_member_total_spent_usdt(entity_id, account)
    }
    fn get_member_upgrade_eligible_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        <T as MemberProvider<AccountId>>::get_member_upgrade_eligible_spent_usdt(entity_id, account)
    }
    fn get_introduced_by(entity_id: u64, account: &AccountId) -> Option<AccountId> {
        <T as MemberProvider<AccountId>>::get_introduced_by(entity_id, account)
    }
    fn get_direct_referral_accounts(
        entity_id: u64,
        account: &AccountId,
    ) -> alloc::vec::Vec<AccountId> {
        <T as MemberProvider<AccountId>>::get_direct_referral_accounts(entity_id, account)
    }
}

impl<AccountId, T: MemberProvider<AccountId>> MemberWriteProvider<AccountId> for T {
    fn auto_register(
        entity_id: u64,
        account: &AccountId,
        referrer: Option<AccountId>,
    ) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::auto_register(entity_id, account, referrer)
    }
    fn update_spent(
        entity_id: u64,
        account: &AccountId,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::update_spent(entity_id, account, amount_usdt)
    }
    fn check_order_upgrade_rules(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
        amount_usdt: u64,
    ) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::check_order_upgrade_rules(
            entity_id,
            buyer,
            product_id,
            amount_usdt,
        )
    }
    fn set_custom_levels_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::set_custom_levels_enabled(entity_id, enabled)
    }
    fn set_upgrade_mode(entity_id: u64, mode: u8) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::set_upgrade_mode(entity_id, mode)
    }
    fn add_custom_level(
        entity_id: u64,
        level_id: u8,
        name: &[u8],
        threshold: u128,
        discount_rate: u16,
        commission_bonus: u16,
    ) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::add_custom_level(
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        )
    }
    fn update_custom_level(
        entity_id: u64,
        level_id: u8,
        name: Option<&[u8]>,
        threshold: Option<u128>,
        discount_rate: Option<u16>,
        commission_bonus: Option<u16>,
    ) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::update_custom_level(
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        )
    }
    fn remove_custom_level(entity_id: u64, level_id: u8) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::remove_custom_level(entity_id, level_id)
    }
    fn set_registration_policy(entity_id: u64, policy_bits: u8) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::set_registration_policy(entity_id, policy_bits)
    }
    fn set_upgrade_rule_system_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::set_upgrade_rule_system_enabled(entity_id, enabled)
    }
    fn ban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::ban_member(entity_id, account)
    }
    fn unban_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::unban_member(entity_id, account)
    }
    fn remove_member(entity_id: u64, account: &AccountId) -> Result<(), DispatchError> {
        <T as MemberProvider<AccountId>>::remove_member(entity_id, account)
    }
}
