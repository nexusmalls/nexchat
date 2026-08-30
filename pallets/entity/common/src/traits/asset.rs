//! Asset-related traits: Token, Pricing, Fee
//!
//! EntityTokenProvider, AssetLedgerPort (with blanket impl from EntityTokenProvider),
//! PricingProvider, EntityTokenPriceProvider, FeeConfigProvider.

use super::super::types::*;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

// ============================================================================
// 实体代币接口
// ============================================================================

/// 实体代币接口
///
/// 供 order 模块调用，实现购物返积分和积分抵扣
pub trait EntityTokenProvider<AccountId, Balance: Default> {
    /// 检查实体是否启用代币
    fn is_token_enabled(entity_id: u64) -> bool;

    /// 获取用户代币余额
    fn token_balance(entity_id: u64, holder: &AccountId) -> Balance;

    /// 购物奖励（订单完成时调用）
    fn reward_on_purchase(
        entity_id: u64,
        buyer: &AccountId,
        purchase_amount: Balance,
    ) -> Result<Balance, DispatchError>;

    /// 代币兑换折扣（下单时调用）
    fn redeem_for_discount(
        entity_id: u64,
        buyer: &AccountId,
        tokens: Balance,
    ) -> Result<Balance, DispatchError>;

    /// 转移代币（P2P 交易市场使用）
    fn transfer(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;

    /// 锁定代币（挂单时使用）
    fn reserve(entity_id: u64, who: &AccountId, amount: Balance) -> Result<(), DispatchError>;

    /// 解锁代币（取消订单时使用）
    fn unreserve(entity_id: u64, who: &AccountId, amount: Balance) -> Balance;

    /// 从锁定中转移（成交时使用）
    fn repatriate_reserved(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
    ) -> Result<Balance, DispatchError>;

    /// Phase 8: 获取代币类型
    fn get_token_type(entity_id: u64) -> TokenType;

    /// Phase 8: 获取代币总供应量
    fn total_supply(entity_id: u64) -> Balance;

    /// H4: 治理提案销毁代币（从 entity 派生账户销毁）
    fn governance_burn(entity_id: u64, amount: Balance) -> Result<(), DispatchError>;

    // ==================== #11 补充: 元数据查询 ====================

    /// 获取代币名称（UTF-8 字节）
    fn token_name(entity_id: u64) -> sp_std::vec::Vec<u8> {
        let _ = entity_id;
        sp_std::vec::Vec::new()
    }

    /// 获取代币符号（UTF-8 字节）
    fn token_symbol(entity_id: u64) -> sp_std::vec::Vec<u8> {
        let _ = entity_id;
        sp_std::vec::Vec::new()
    }

    /// 获取代币精度
    fn token_decimals(entity_id: u64) -> u8 {
        let _ = entity_id;
        0
    }

    /// 代币是否可自由转让（检查 TransferRestrictionMode）
    fn is_token_transferable(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }

    /// 获取代币持有人数量
    fn token_holder_count(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }

    /// 获取可用余额（总余额 - 锁仓 - 预留）
    fn available_balance(entity_id: u64, holder: &AccountId) -> Balance {
        let _ = (entity_id, holder);
        Default::default()
    }

    // ==================== R10: 治理提案链上执行接口 ====================

    /// 设置代币最大供应量（治理提案执行）
    fn governance_set_max_supply(
        entity_id: u64,
        new_max_supply: Balance,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, new_max_supply);
        Err(DispatchError::Other("not implemented"))
    }

    /// 设置代币类型（治理提案执行）
    fn governance_set_token_type(entity_id: u64, new_type: TokenType) -> Result<(), DispatchError> {
        let _ = (entity_id, new_type);
        Err(DispatchError::Other("not implemented"))
    }

    /// 设置转账限制模式（治理提案执行）
    fn governance_set_transfer_restriction(
        entity_id: u64,
        restriction: u8,
        min_receiver_kyc: u8,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, restriction, min_receiver_kyc);
        Err(DispatchError::Other("not implemented"))
    }

    /// 退还已扣减的 Token 折扣（订单取消/退款时，将 Token 返还 buyer）
    fn refund_discount_tokens(
        entity_id: u64,
        buyer: &AccountId,
        tokens: Balance,
    ) -> Result<(), DispatchError> {
        let _ = (entity_id, buyer, tokens);
        Err(DispatchError::Other("not implemented"))
    }
}

/// 空实体代币提供者（测试用或未启用代币时）
pub struct NullEntityTokenProvider;

impl<AccountId, Balance: Default> EntityTokenProvider<AccountId, Balance>
    for NullEntityTokenProvider
{
    fn is_token_enabled(_entity_id: u64) -> bool {
        false
    }
    fn token_balance(_entity_id: u64, _holder: &AccountId) -> Balance {
        Default::default()
    }
    fn reward_on_purchase(_: u64, _: &AccountId, _: Balance) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn redeem_for_discount(_: u64, _: &AccountId, _: Balance) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn transfer(_: u64, _: &AccountId, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn reserve(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn unreserve(_: u64, _: &AccountId, _: Balance) -> Balance {
        Default::default()
    }
    fn repatriate_reserved(
        _: u64,
        _: &AccountId,
        _: &AccountId,
        _: Balance,
    ) -> Result<Balance, DispatchError> {
        Ok(Default::default())
    }
    fn get_token_type(_entity_id: u64) -> TokenType {
        TokenType::default()
    }
    fn total_supply(_entity_id: u64) -> Balance {
        Default::default()
    }
    fn governance_burn(_: u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn refund_discount_tokens(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn available_balance(_: u64, _: &AccountId) -> Balance {
        Default::default()
    }
}

// ============================================================================
// Phase 3.3: AssetLedgerPort — 从 EntityTokenProvider 拆出的资产账本接口
// ============================================================================

/// Entity Token 资产账本接口（细粒度 Port）
///
/// 仅包含资产余额查询和 reserve/unreserve/repatriate 等账本操作，
/// 供 order 模块管理 Token 支付的资金锁定与结算。
///
/// 与 `LoyaltyWritePort` 的区别：
/// - `AssetLedgerPort` — 资产语义：以 `fund_account()` (payer) 调用
/// - `LoyaltyWritePort` — 激励语义：始终以 `&buyer` 调用
pub trait AssetLedgerPort<AccountId, Balance> {
    /// 检查 entity 是否启用了 Token
    fn is_token_enabled(entity_id: u64) -> bool;

    /// 查询用户 Token 余额（free balance）
    fn token_balance(entity_id: u64, holder: &AccountId) -> Balance;

    /// 锁定 Token（下单时从 payer/buyer 的 free 转入 reserved）
    fn reserve(entity_id: u64, who: &AccountId, amount: Balance) -> Result<(), DispatchError>;

    /// 解锁 Token（取消/退款时从 reserved 转回 free）
    fn unreserve(entity_id: u64, who: &AccountId, amount: Balance) -> Balance;

    /// 从 reserved 转移给另一账户（结算时 payer → seller/platform）
    fn repatriate_reserved(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
    ) -> Result<Balance, DispatchError>;
}

/// 空 AssetLedgerPort — 直接使用 `NullEntityTokenProvider`（通过 blanket impl 自动满足）
pub type NullAssetLedgerPort = NullEntityTokenProvider;

/// Blanket impl: 任何实现了 EntityTokenProvider 的类型自动满足 AssetLedgerPort
///
/// 这确保 pallet-entity-token（实现 EntityTokenProvider）无需额外改动即可用作 AssetLedgerPort。
impl<T, AccountId, Balance: Default> AssetLedgerPort<AccountId, Balance> for T
where
    T: EntityTokenProvider<AccountId, Balance>,
{
    fn is_token_enabled(entity_id: u64) -> bool {
        <T as EntityTokenProvider<AccountId, Balance>>::is_token_enabled(entity_id)
    }

    fn token_balance(entity_id: u64, holder: &AccountId) -> Balance {
        <T as EntityTokenProvider<AccountId, Balance>>::token_balance(entity_id, holder)
    }

    fn reserve(entity_id: u64, who: &AccountId, amount: Balance) -> Result<(), DispatchError> {
        <T as EntityTokenProvider<AccountId, Balance>>::reserve(entity_id, who, amount)
    }

    fn unreserve(entity_id: u64, who: &AccountId, amount: Balance) -> Balance {
        <T as EntityTokenProvider<AccountId, Balance>>::unreserve(entity_id, who, amount)
    }

    fn repatriate_reserved(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: Balance,
    ) -> Result<Balance, DispatchError> {
        <T as EntityTokenProvider<AccountId, Balance>>::repatriate_reserved(
            entity_id, from, to, amount,
        )
    }
}

// ============================================================================
// 定价接口
// ============================================================================

/// NEX/USDT 价格查询接口
///
/// 供 shop 模块计算 USDT 等值的 NEX 押金
pub trait PricingProvider {
    /// 获取 NEX/USDT 加权平均价格
    ///
    /// # 返回
    /// - `u64`: 价格（精度 10^6，即 1,000,000 = 1 USDT/NEX）
    /// - 返回 0 表示价格不可用
    fn get_nex_usdt_price() -> u64;

    /// 价格数据是否过时
    ///
    /// # 说明
    /// 若市场长期无交易，价格可能严重偏离真实值。
    /// 消费方应在使用价格前检查此标志，过时时使用兜底值。
    ///
    /// # 默认实现
    /// 返回 `false`（向后兼容，不影响现有模块）
    fn is_price_stale() -> bool {
        false
    }
}

/// 空定价提供者（测试用）
pub struct NullPricingProvider;

impl PricingProvider for NullPricingProvider {
    fn get_nex_usdt_price() -> u64 {
        // 默认价格：0.000001 USDT/NEX（精度 10^6 = 1）
        1
    }
    fn is_price_stale() -> bool {
        false
    }
}

// ============================================================================
// 实体代币价格查询接口
// ============================================================================

/// 价格可靠性等级（简化的置信度判断）
///
/// 替代 `token_price_confidence() -> u8` 的数值型判断，
/// 下游消费方只需匹配枚举即可决策，无需记忆置信度区间。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub enum PriceReliability {
    /// 价格可靠（TWAP 可用 + 足够交易量）
    Reliable,
    /// 价格低可信（仅 initial_price 或低交易量）
    Low,
    /// 价格不可用或过时
    Unavailable,
}

/// 实体代币当前价格查询接口
///
/// 供需要获取 Entity Token 价格的模块使用（佣金换算、分红定价、前端展示等）。
///
/// ## 价格单位
/// - `get_token_price`: NEX per Token（精度 10^12，链上原生代币单位）
/// - `get_token_price_usdt`: USDT per Token（精度 10^6，通过 NEX 价格间接换算）
///
/// ## 注意
/// Entity Token 价格由 entity owner 可影响（set_initial_price + 低流动性自买自卖），
/// **不应用于安全关键的押金/保证金计算**，仅适用于展示和非关键换算。
pub trait EntityTokenPriceProvider {
    type Balance;

    /// 获取代币当前价格（NEX per Token, 精度 10^12）
    ///
    /// 优先级：1h TWAP → LastTradePrice → initial_price
    /// 返回 `None` 表示无任何价格数据
    fn get_token_price(entity_id: u64) -> Option<Self::Balance>;

    /// 获取代币 USDT 计价（精度 10^6）
    ///
    /// 通过 token_nex_price × nex_usdt_rate / 10^12 间接换算
    /// 返回 `None` 表示价格不可用（Token 或 NEX/USDT 价格缺失）
    fn get_token_price_usdt(entity_id: u64) -> Option<u64>;

    /// 价格置信度 (0-100)
    ///
    /// 基于数据来源、交易量和新鲜度综合评估
    fn token_price_confidence(entity_id: u64) -> u8;

    /// 价格数据是否过时（超过 max_age_blocks 个区块未更新）
    fn is_token_price_stale(entity_id: u64, max_age_blocks: u32) -> bool;

    /// 价格是否可信赖（置信度 >= 阈值）
    ///
    /// 默认阈值 30
    fn is_token_price_reliable(entity_id: u64) -> bool {
        Self::token_price_confidence(entity_id) >= 30
    }

    /// 获取简化的价格可靠性等级
    ///
    /// 基于 confidence 数值自动映射：>=60 Reliable, >=30 Low, <30 Unavailable。
    /// 下游代码应优先使用此方法，避免硬编码置信度数值。
    fn price_reliability(entity_id: u64) -> PriceReliability {
        let c = Self::token_price_confidence(entity_id);
        if c >= 60 {
            PriceReliability::Reliable
        } else if c >= 30 {
            PriceReliability::Low
        } else {
            PriceReliability::Unavailable
        }
    }
}

/// EntityTokenPriceProvider 的空实现（无市场时使用）
impl EntityTokenPriceProvider for () {
    type Balance = u128;
    fn get_token_price(_entity_id: u64) -> Option<u128> {
        None
    }
    fn get_token_price_usdt(_entity_id: u64) -> Option<u64> {
        None
    }
    fn token_price_confidence(_entity_id: u64) -> u8 {
        0
    }
    fn is_token_price_stale(_entity_id: u64, _max_age_blocks: u32) -> bool {
        true
    }
}

// ============================================================================
// P7: 手续费配置查询接口
// ============================================================================

/// 手续费配置查询接口
///
/// 统一跨模块手续费查询，避免费率逻辑碎片化。
/// 费率单位: 基点 (bps)，100 = 1%。
pub trait FeeConfigProvider {
    /// 获取全局 NEX 平台费率（bps）
    fn platform_fee_rate() -> u16;

    /// 获取 Entity 级平台费率覆盖（None = 使用全局默认）
    fn entity_fee_override(entity_id: u64) -> Option<u16> {
        let _ = entity_id;
        None
    }

    /// 获取 Entity Token 交易费率（bps）
    fn token_fee_rate(entity_id: u64) -> u16 {
        let _ = entity_id;
        0
    }

    /// 获取 Entity 有效费率（优先 entity_fee_override，回退 platform_fee_rate）
    fn effective_fee_rate(entity_id: u64) -> u16 {
        Self::entity_fee_override(entity_id).unwrap_or_else(Self::platform_fee_rate)
    }
}

/// 空手续费配置提供者（测试用）
pub struct NullFeeConfigProvider;

impl FeeConfigProvider for NullFeeConfigProvider {
    fn platform_fee_rate() -> u16 {
        100
    }
}

// ============================================================================
// Phase 5.3: TokenFeeConfigPort — Token 平台费率 + Entity 账户查询
// ============================================================================

/// Token 平台费率 + Entity 账户查询（从 TokenCommissionHandler 剥离的纯查询接口）
///
/// 供 Order 模块在 do_complete_order 中计算 Token 平台费拆分，
/// 不再依赖完整的 TokenOrderCommissionHandler。
pub trait TokenFeeConfigPort<AccountId> {
    /// 获取 Entity 级 Token 平台费率（bps）
    fn token_platform_fee_rate(entity_id: u64) -> u16;
    /// 获取 Entity 派生账户（Token 平台费转入目标）
    fn entity_account(entity_id: u64) -> AccountId;
}

impl<AccountId: Default> TokenFeeConfigPort<AccountId> for () {
    fn token_platform_fee_rate(_: u64) -> u16 {
        0
    }
    fn entity_account(_: u64) -> AccountId {
        AccountId::default()
    }
}
