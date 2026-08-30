//! Incentive, query, and auxiliary provider traits
//!
//! LoyaltyReadPort, LoyaltyWritePort, DisputeQueryProvider, TokenSaleProvider,
//! VestingSchedule/Provider, DividendProvider, EmergencyProvider,
//! ReviewProvider, MarketProvider.

use super::super::types::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

// ============================================================================
// #2 争议查询接口（跨模块）
// ============================================================================

/// 争议查询接口
///
/// 供 order/commission/review 等模块查询争议状态，
/// 无需直接依赖 pallet-dispute-arbitration。
pub trait DisputeQueryProvider<AccountId> {
    /// 获取订单的争议状态
    fn order_dispute_status(order_id: u64) -> DisputeStatus;

    /// 获取争议的裁决结果（仅已解决的争议）
    fn dispute_resolution(dispute_id: u64) -> Option<DisputeResolution>;

    /// 查询账户在指定域下的活跃争议数量
    fn active_dispute_count(domain: u8, account: &AccountId) -> u32;

    /// 检查订单是否有活跃争议
    fn has_active_dispute(order_id: u64) -> bool {
        Self::order_dispute_status(order_id).is_active()
    }

    /// 获取争议 ID（通过订单 ID 查找）
    fn dispute_id_by_order(order_id: u64) -> Option<u64> {
        let _ = order_id;
        None
    }

    /// 获取争议涉及金额
    fn dispute_amount(dispute_id: u64) -> Option<u128> {
        let _ = dispute_id;
        None
    }

    /// 检查指定 Entity 是否存在活跃争议
    fn has_active_disputes_for_entity(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
}

/// 空争议查询提供者（测试用或未启用争议系统时）
pub struct NullDisputeQueryProvider;

impl<AccountId> DisputeQueryProvider<AccountId> for NullDisputeQueryProvider {
    fn order_dispute_status(_order_id: u64) -> DisputeStatus {
        DisputeStatus::None
    }
    fn dispute_resolution(_dispute_id: u64) -> Option<DisputeResolution> {
        None
    }
    fn active_dispute_count(_domain: u8, _account: &AccountId) -> u32 {
        0
    }
}

// ============================================================================
// #8 Token Sale 查询接口
// ============================================================================

/// Token Sale 查询接口
///
/// 供 entity/governance/frontend 等模块查询 Token Sale 状态，
/// 无需直接依赖 pallet-entity-tokensale。
pub trait TokenSaleProvider<Balance> {
    /// 获取实体当前活跃的发售轮次 ID
    fn active_sale_round(entity_id: u64) -> Option<u64>;

    /// 获取发售轮次状态
    fn sale_round_status(round_id: u64) -> Option<TokenSaleStatus>;

    /// 获取轮次已售数量
    fn sold_amount(round_id: u64) -> Option<Balance>;

    /// 获取轮次剩余数量
    fn remaining_amount(round_id: u64) -> Option<Balance>;

    /// 获取轮次参与人数
    fn participants_count(round_id: u64) -> Option<u32>;

    /// 检查实体是否有活跃的发售
    fn has_active_sale(entity_id: u64) -> bool {
        Self::active_sale_round(entity_id).is_some()
    }

    /// 获取轮次总供应量
    fn sale_total_supply(round_id: u64) -> Option<Balance> {
        let _ = round_id;
        None
    }

    /// 获取轮次所属实体 ID
    fn sale_entity_id(round_id: u64) -> Option<u64> {
        let _ = round_id;
        None
    }
}

/// 空 Token Sale 提供者（测试用或未启用 Token Sale 时）
pub struct NullTokenSaleProvider;

impl<Balance> TokenSaleProvider<Balance> for NullTokenSaleProvider {
    fn active_sale_round(_entity_id: u64) -> Option<u64> {
        None
    }
    fn sale_round_status(_round_id: u64) -> Option<TokenSaleStatus> {
        None
    }
    fn sold_amount(_round_id: u64) -> Option<Balance> {
        None
    }
    fn remaining_amount(_round_id: u64) -> Option<Balance> {
        None
    }
    fn participants_count(_round_id: u64) -> Option<u32> {
        None
    }
}

// ============================================================================
// P8: 锁仓/归属 (Vesting) 接口
// ============================================================================

/// 锁仓/归属计划
///
/// 定义代币的线性释放规则：悬崖期 + 线性释放期。
/// 用于 Token Sale 锁仓、团队分配、投资者保护等场景。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub struct VestingSchedule {
    /// 锁仓总量
    pub total: u128,
    /// 已释放量
    pub released: u128,
    /// 开始区块
    pub start_block: u64,
    /// 悬崖期（区块数，悬崖期内不释放）
    pub cliff_blocks: u64,
    /// 线性释放期（区块数，悬崖期后线性释放）
    pub vesting_blocks: u64,
}

impl VestingSchedule {
    /// 计算在指定区块时可释放的数量（尚未领取的部分）
    pub fn releasable_at(&self, current_block: u64) -> u128 {
        let cliff_end = self.start_block.saturating_add(self.cliff_blocks);
        if current_block < cliff_end {
            return 0;
        }
        let elapsed = current_block.saturating_sub(cliff_end);
        let total_vested = if self.vesting_blocks == 0 || elapsed >= self.vesting_blocks {
            self.total
        } else {
            self.total.saturating_mul(elapsed as u128) / (self.vesting_blocks as u128)
        };
        total_vested.saturating_sub(self.released)
    }

    /// 是否已完全释放
    pub fn is_fully_released(&self) -> bool {
        self.released >= self.total
    }
}

/// 锁仓/归属查询接口
///
/// 供 token sale / governance / frontend 等模块查询和操作锁仓计划，
/// 无需直接依赖 vesting 实现模块。
pub trait VestingProvider<AccountId> {
    /// 获取账户在指定实体下的锁仓余额
    fn vesting_balance(entity_id: u64, account: &AccountId) -> u128;

    /// 获取当前可释放的余额
    fn releasable_balance(entity_id: u64, account: &AccountId) -> u128;

    /// 释放已到期的锁仓代币，返回实际释放数量
    fn release(entity_id: u64, account: &AccountId) -> Result<u128, DispatchError>;

    /// 获取锁仓计划详情
    fn vesting_schedule(entity_id: u64, account: &AccountId) -> Option<VestingSchedule> {
        let _ = (entity_id, account);
        None
    }

    /// 检查账户是否有活跃锁仓
    fn has_vesting(entity_id: u64, account: &AccountId) -> bool {
        Self::vesting_balance(entity_id, account) > 0
    }
}

/// 空锁仓提供者（测试用或未启用锁仓时）
pub struct NullVestingProvider;

impl<AccountId> VestingProvider<AccountId> for NullVestingProvider {
    fn vesting_balance(_: u64, _: &AccountId) -> u128 {
        0
    }
    fn releasable_balance(_: u64, _: &AccountId) -> u128 {
        0
    }
    fn release(_: u64, _: &AccountId) -> Result<u128, DispatchError> {
        Ok(0)
    }
}

// ============================================================================
// P9: 分红查询接口
// ============================================================================

/// 分红查询接口
///
/// 供 governance / frontend 等模块查询和领取分红，
/// 无需直接依赖 token 模块的分红实现。
pub trait DividendProvider<AccountId, Balance: Default> {
    /// 查询待领取分红
    fn pending_dividend(entity_id: u64, account: &AccountId) -> Balance;

    /// 领取分红
    fn claim_dividend(entity_id: u64, account: &AccountId) -> Result<Balance, DispatchError>;

    /// 检查分红是否已激活
    fn is_dividend_active(entity_id: u64) -> bool;

    /// 获取下次分红时间（区块号，None = 未配置或未激活）
    fn next_distribution_at(entity_id: u64) -> Option<u64> {
        let _ = entity_id;
        None
    }

    /// 获取累计已分红总额
    fn total_distributed(entity_id: u64) -> Balance {
        let _ = entity_id;
        Default::default()
    }
}

/// 空分红提供者（测试用或未启用分红时）
pub struct NullDividendProvider;

impl<AccountId, Balance: Default> DividendProvider<AccountId, Balance> for NullDividendProvider {
    fn pending_dividend(_: u64, _: &AccountId) -> Balance {
        Default::default()
    }
    fn claim_dividend(_: u64, _: &AccountId) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn is_dividend_active(_: u64) -> bool {
        false
    }
}

// ============================================================================
// P12: 紧急暂停接口
// ============================================================================

/// 紧急暂停接口
///
/// 全局紧急暂停机制，用于发现严重漏洞或遭受攻击时一键暂停核心操作。
/// 由 Root 调用，影响所有交易、订单、Token 操作。
pub trait EmergencyProvider {
    /// 检查系统是否处于紧急暂停状态
    fn is_emergency_paused() -> bool;

    /// 检查指定模块是否被暂停（模块 ID 由各 pallet 自定义）
    ///
    /// 默认行为：跟随全局暂停状态
    fn is_module_paused(module_id: u8) -> bool {
        let _ = module_id;
        Self::is_emergency_paused()
    }

    /// 暂停系统（仅 Root）
    fn pause_system() -> Result<(), DispatchError> {
        Err(DispatchError::Other("not implemented"))
    }

    /// 恢复系统（仅 Root）
    fn resume_system() -> Result<(), DispatchError> {
        Err(DispatchError::Other("not implemented"))
    }
}

/// 空紧急暂停提供者（测试用，系统永不暂停）
pub struct NullEmergencyProvider;

impl EmergencyProvider for NullEmergencyProvider {
    fn is_emergency_paused() -> bool {
        false
    }
}

// ============================================================================
// 评价查询接口
// ============================================================================

/// 评价查询接口
///
/// 供 shop/product/order/governance 等模块查询评价信息，
/// 无需直接依赖 pallet-entity-review。
pub trait ReviewProvider<AccountId> {
    /// 获取 Shop 平均评分（0-100，0 = 无评分）
    fn shop_average_rating(shop_id: u64) -> u8;

    /// 获取 Shop 评价总数
    fn shop_review_count(shop_id: u64) -> u32;

    /// 获取 Product 平均评分（0-100，0 = 无评分）
    fn product_average_rating(product_id: u64) -> u8;

    /// 获取 Product 评价总数
    fn product_review_count(product_id: u64) -> u32;

    /// 检查用户是否已评价某订单
    fn has_reviewed_order(order_id: u64, reviewer: &AccountId) -> bool;

    /// 检查 Entity 是否启用评价系统
    fn is_review_enabled(entity_id: u64) -> bool {
        let _ = entity_id;
        true
    }

    /// 获取用户在某 Entity 下的总评价数
    fn user_review_count(entity_id: u64, reviewer: &AccountId) -> u32 {
        let _ = (entity_id, reviewer);
        0
    }
}

/// 空评价提供者（测试用或未启用评价系统时）
pub struct NullReviewProvider;

impl<AccountId> ReviewProvider<AccountId> for NullReviewProvider {
    fn shop_average_rating(_: u64) -> u8 {
        0
    }
    fn shop_review_count(_: u64) -> u32 {
        0
    }
    fn product_average_rating(_: u64) -> u8 {
        0
    }
    fn product_review_count(_: u64) -> u32 {
        0
    }
    fn has_reviewed_order(_: u64, _: &AccountId) -> bool {
        false
    }
}

// ============================================================================
// 市场查询接口
// ============================================================================

/// 市场/交易查询接口
///
/// 供 token/governance/frontend 等模块查询 Entity Token 二级市场信息，
/// 无需直接依赖 pallet-entity-market。
pub trait MarketProvider<AccountId, Balance> {
    /// 检查 Entity Token 是否有活跃的交易对
    fn has_active_market(entity_id: u64) -> bool;

    /// 获取 Entity Token 近期交易量（原生代币单位）
    ///
    /// "24h" 基于区块数估算（假设 6s/block ≈ 14400 blocks），
    /// 实现方应使用滑动窗口或最近 N 个区块的交易量统计。
    fn trading_volume_24h(entity_id: u64) -> Balance;

    /// 获取当前最佳买价（最高买单价格）
    fn best_bid(entity_id: u64) -> Option<Balance>;

    /// 获取当前最佳卖价（最低卖单价格）
    fn best_ask(entity_id: u64) -> Option<Balance>;

    /// 获取某用户的活跃挂单数量
    fn user_active_order_count(entity_id: u64, account: &AccountId) -> u32 {
        let _ = (entity_id, account);
        0
    }

    /// 市场是否被暂停交易
    fn is_market_paused(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
}

/// 空市场提供者（测试用或未启用市场时）
pub struct NullMarketProvider;

impl<AccountId, Balance: Default> MarketProvider<AccountId, Balance> for NullMarketProvider {
    fn has_active_market(_: u64) -> bool {
        false
    }
    fn trading_volume_24h(_: u64) -> Balance {
        Default::default()
    }
    fn best_bid(_: u64) -> Option<Balance> {
        None
    }
    fn best_ask(_: u64) -> Option<Balance> {
        None
    }
}

// ============================================================================
// Phase 1 新增: Loyalty 激励系统接口
// ============================================================================

/// 激励系统只读查询接口
///
/// 供 order/governance/frontend 等模块查询积分余额、购物余额和 Token 状态。
/// 由 `pallet-entity-loyalty`（Phase 2 新建）实现。
pub trait LoyaltyReadPort<AccountId, Balance> {
    /// 查询 entity 是否启用了 Token（决定是否允许 Token 折扣/奖励）
    fn is_token_enabled(entity_id: u64) -> bool;

    /// 查询用户在某 entity 下的 Token 折扣可用余额
    fn token_discount_balance(entity_id: u64, who: &AccountId) -> Balance;

    /// 查询用户在某 entity 下的购物余额（NEX 计价）
    fn shopping_balance(entity_id: u64, who: &AccountId) -> Balance;

    /// 查询 Entity 级购物余额总额（用于 solvency check）
    fn shopping_total(entity_id: u64) -> Balance;
}

/// 激励系统写入接口
///
/// 供 order 模块下单时抵扣折扣/购物余额、完成时发放奖励，
/// 供 commission 模块结算后写入购物余额。
pub trait LoyaltyWritePort<AccountId, Balance>: LoyaltyReadPort<AccountId, Balance> {
    /// Token 折扣抵扣（下单时从 buyer 的 token 中扣减，返回实际折扣金额）
    fn redeem_for_discount(
        entity_id: u64,
        who: &AccountId,
        tokens: Balance,
    ) -> Result<Balance, DispatchError>;

    /// 消费购物余额（下单时从 buyer 的购物余额中扣减）
    fn consume_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;

    /// 购物奖励发放（订单完成时 mint token 给 buyer）
    fn reward_on_purchase(
        entity_id: u64,
        who: &AccountId,
        purchase_amount: Balance,
    ) -> Result<Balance, DispatchError>;

    /// 写入购物余额（commission 结算后调用，将应得购物余额记入 buyer 账户）
    fn credit_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;

    /// 回滚购物余额（订单取消/退款时，NEX 从 buyer → entity，记账余额恢复）
    fn rollback_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;

    /// 回滚 Token 折扣（订单取消/退款时，将已扣减的 Token 退还 buyer）
    fn rollback_token_discount(
        entity_id: u64,
        who: &AccountId,
        tokens: Balance,
    ) -> Result<(), DispatchError>;

    /// HIGH-2 审计修复: 清零 Entity 所有会员的 NEX 购物余额（Entity 关闭前使用）
    ///
    /// 将 ShopShoppingTotal 清零，并 drain 所有 MemberShoppingBalance prefix。
    /// 资金保留在 entity_account 中（不退还会员），由 do_finalize_close 统一处理。
    fn forfeit_all_shopping_balances(entity_id: u64);

    /// 没收单个用户的购物余额（TTL 到期时使用）
    ///
    /// 仅清零 MemberShoppingBalance 记账并同步 ShopShoppingTotal，
    /// Entity 账户中对应的 NEX 自然解锁为 Entity 可用资金，无需额外转账。
    fn forfeit_shopping_balance(entity_id: u64, who: &AccountId) -> Result<(), DispatchError>;
}

// ============================================================================
// Phase 5.1B: Token 购物余额 Port（独立于 NEX 购物余额）
// ============================================================================

/// Token 购物余额只读查询接口
///
/// 供 commission-core 查询 Token 购物余额总额（solvency check）和个人余额（dashboard）。
/// 由 `pallet-entity-loyalty` 实现。
pub trait LoyaltyTokenReadPort<AccountId, TokenBalance> {
    /// 查询用户在某 entity 下的 Token 购物余额
    fn token_shopping_balance(entity_id: u64, who: &AccountId) -> TokenBalance;

    /// 查询 Entity 级 Token 购物余额总额（用于 solvency check）
    fn token_shopping_total(entity_id: u64) -> TokenBalance;
}

/// Token 购物余额写入接口
///
/// 供 commission-core 结算后写入 Token 购物余额、提现时消费 Token 购物余额。
/// 由 `pallet-entity-loyalty` 实现。
pub trait LoyaltyTokenWritePort<AccountId, TokenBalance>:
    LoyaltyTokenReadPort<AccountId, TokenBalance>
{
    /// 写入 Token 购物余额（commission 结算后调用，记入目标账户）
    fn credit_token_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: TokenBalance,
    ) -> Result<(), DispatchError>;

    /// 消费 Token 购物余额（记账 + Token 从 Entity 账户转入会员钱包）
    fn consume_token_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: TokenBalance,
    ) -> Result<(), DispatchError>;

    /// HIGH-2 审计修复: 清零 Entity 所有会员的 Token 购物余额（Entity 关闭前使用）
    fn forfeit_all_token_shopping_balances(entity_id: u64);
}

/// 空 Loyalty 提供者（测试用或 loyalty 模块未上线时）
pub struct NullLoyaltyProvider;

impl<AccountId, Balance: Default> LoyaltyReadPort<AccountId, Balance> for NullLoyaltyProvider {
    fn is_token_enabled(_entity_id: u64) -> bool {
        false
    }
    fn token_discount_balance(_: u64, _: &AccountId) -> Balance {
        Default::default()
    }
    fn shopping_balance(_: u64, _: &AccountId) -> Balance {
        Default::default()
    }
    fn shopping_total(_: u64) -> Balance {
        Default::default()
    }
}

impl<AccountId, Balance: Default> LoyaltyWritePort<AccountId, Balance> for NullLoyaltyProvider {
    fn redeem_for_discount(_: u64, _: &AccountId, _: Balance) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn consume_shopping_balance(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn reward_on_purchase(_: u64, _: &AccountId, _: Balance) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn credit_shopping_balance(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn rollback_shopping_balance(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn rollback_token_discount(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn forfeit_all_shopping_balances(_: u64) {}
    fn forfeit_shopping_balance(_: u64, _: &AccountId) -> Result<(), DispatchError> {
        Ok(())
    }
}

impl<AccountId, TokenBalance: Default> LoyaltyTokenReadPort<AccountId, TokenBalance>
    for NullLoyaltyProvider
{
    fn token_shopping_balance(_: u64, _: &AccountId) -> TokenBalance {
        Default::default()
    }
    fn token_shopping_total(_: u64) -> TokenBalance {
        Default::default()
    }
}

impl<AccountId, TokenBalance: Default> LoyaltyTokenWritePort<AccountId, TokenBalance>
    for NullLoyaltyProvider
{
    fn credit_token_shopping_balance(
        _: u64,
        _: &AccountId,
        _: TokenBalance,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn consume_token_shopping_balance(
        _: u64,
        _: &AccountId,
        _: TokenBalance,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn forfeit_all_token_shopping_balances(_: u64) {}
}
