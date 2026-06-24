//! # Commission Common Types
//!
//! Shared types and traits for the commission plugin system.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use pallet_entity_common::PoolRewardLevelClaimRule;
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

// ============================================================================
// 返佣模式位标志
// ============================================================================

/// 返佣模式位标志（可多选）
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
pub struct CommissionModes(pub u16);

impl Default for CommissionModes {
    fn default() -> Self {
        // 默认启用：单线（上线+下线）、多级分佣、奖金池
        Self(
            Self::SINGLE_LINE_UPLINE
                | Self::SINGLE_LINE_DOWNLINE
                | Self::MULTI_LEVEL
                | Self::POOL_REWARD,
        )
    }
}

impl CommissionModes {
    pub const NONE: u16 = 0b0000_0000;
    pub const DIRECT_REWARD: u16 = 0b0000_0001;
    pub const MULTI_LEVEL: u16 = 0b0000_0010;
    pub const TEAM_PERFORMANCE: u16 = 0b0000_0100;
    pub const LEVEL_DIFF: u16 = 0b0000_1000;
    pub const FIXED_AMOUNT: u16 = 0b0001_0000;
    pub const FIRST_ORDER: u16 = 0b0010_0000;
    pub const REPEAT_PURCHASE: u16 = 0b0100_0000;
    pub const SINGLE_LINE_UPLINE: u16 = 0b1000_0000;
    pub const SINGLE_LINE_DOWNLINE: u16 = 0b1_0000_0000;
    pub const POOL_REWARD: u16 = 0b10_0000_0000;
    pub const OWNER_REWARD: u16 = 0b100_0000_0000;

    /// 所有已定义模式位的并集（单一事实来源）
    pub const ALL_VALID: u16 = Self::DIRECT_REWARD
        | Self::MULTI_LEVEL
        | Self::TEAM_PERFORMANCE
        | Self::LEVEL_DIFF
        | Self::FIXED_AMOUNT
        | Self::FIRST_ORDER
        | Self::REPEAT_PURCHASE
        | Self::SINGLE_LINE_UPLINE
        | Self::SINGLE_LINE_DOWNLINE
        | Self::POOL_REWARD
        | Self::OWNER_REWARD;

    /// 检查是否仅包含已定义的模式位（无未知高位）
    pub fn is_valid(&self) -> bool {
        self.0 & !Self::ALL_VALID == 0
    }

    /// P1-1 审计修复: 检查是否包含 flag 中的**全部**位
    pub fn contains(&self, flag: u16) -> bool {
        self.0 & flag == flag
    }

    /// P1-1 审计修复: 检查是否与 flag 有**任意**位交集
    pub fn intersects(&self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

// ============================================================================
// 返佣类型 / 状态
// ============================================================================

/// 返佣类型
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
pub enum CommissionType {
    DirectReward,
    MultiLevel,
    TeamPerformance,
    LevelDiff,
    FixedAmount,
    FirstOrder,
    RepeatPurchase,
    SingleLineUpline,
    SingleLineDownline,
    EntityReferral,
    PoolReward,
    OwnerReward,
}

/// 返佣状态
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
    Default,
)]
pub enum CommissionStatus {
    #[default]
    Pending,
    /// [已废弃] 保留以维持 SCALE 编码索引稳定，生产代码从未使用此状态
    Distributed,
    /// P1-3 审计修复: 原 Withdrawn，实际含义为"订单已结算，可归档"（由 settle_order_commission 设置）
    Settled,
    Cancelled,
}

// ============================================================================
// 返佣记录
// ============================================================================

/// 返佣记录
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
pub struct CommissionRecord<AccountId, Balance, BlockNumber> {
    pub entity_id: u64,
    pub shop_id: u64,
    pub order_id: u64,
    pub buyer: AccountId,
    pub beneficiary: AccountId,
    pub amount: Balance,
    pub commission_type: CommissionType,
    pub level: u16,
    pub status: CommissionStatus,
    pub created_at: BlockNumber,
}

/// 会员返佣统计
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
    Default,
)]
pub struct MemberCommissionStatsData<Balance> {
    pub total_earned: Balance,
    pub pending: Balance,
    pub withdrawn: Balance,
    pub repurchased: Balance,
    pub order_count: u32,
}

// ============================================================================
// 提现配置
// ============================================================================

/// 分级提现配置
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
pub struct WithdrawalTierConfig {
    pub withdrawal_rate: u16,
    pub repurchase_rate: u16,
}

impl Default for WithdrawalTierConfig {
    fn default() -> Self {
        Self {
            withdrawal_rate: 10000,
            repurchase_rate: 0,
        }
    }
}

impl WithdrawalTierConfig {
    /// 校验 withdrawal_rate + repurchase_rate == 10000
    pub fn is_valid(&self) -> bool {
        self.withdrawal_rate.saturating_add(self.repurchase_rate) == 10000
    }
}

/// 提现模式
///
/// 决定佣金提现时复购比率的确定方式。
/// 无论选择哪种模式，Governance 设定的全局最低复购比率始终生效。
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
    Default,
)]
pub enum WithdrawalMode {
    /// 全额提现：不强制复购（Governance 底线仍生效）
    FullWithdrawal,
    /// 固定比率：所有会员统一复购比率
    FixedRate { repurchase_rate: u16 },
    /// 按等级自动决定：通过 default_tier + level_overrides 查表
    #[default]
    LevelBased,
    /// 会员自选：会员提现时指定复购比率，不低于 min_repurchase_rate
    MemberChoice { min_repurchase_rate: u16 },
}

// ============================================================================
// 强制复购配置
// ============================================================================

/// 强制复购配置
///
/// 由 Entity Owner/Admin 通过 `set_repurchase_config` 设置。
/// 当 `enforced == true` 时，购物余额（折算 USDT）达到 `min_package_usdt` 后
/// 会发出 `RepurchaseReady` 事件，或（`auto_order == true` 时）直接链上创建复购订单。
///
/// `shopping_balance_ttl_blocks > 0` 时，任何人可在 TTL 到期后调用
/// `expire_shopping_balance` 强制触发自动下单或没收余额。
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
pub struct RepurchaseConfig {
    /// 最低复购套餐金额（USDT，精度 10^6）
    /// 购物余额（折算 USDT）达到此值时，标记为可复购状态
    pub min_package_usdt: u64,
    /// 是否启用强制复购（true = 购物余额只能用于购买，不能用于升级/转出）
    pub enforced: bool,
    /// 是否启用链上自动下单（opt-in，false = 只发 RepurchaseReady 事件）
    pub auto_order: bool,
    /// 自动复购套餐 product_id（auto_order=true 时必须 > 0）
    pub default_product_id: u64,
    /// 购物余额 TTL（区块数，0 = 永不过期）
    /// 从最后一次 credit 起超过此区块数仍未消费，可被 expire_shopping_balance 触发处理
    pub shopping_balance_ttl_blocks: u32,
    /// 购物余额超过此 USDT 阈值时阻止领奖（USDT，精度 10^6，0 = 不限制）
    ///
    /// 启用后（> 0），会员的购物余额折算 USDT 超过此值时不可提现，
    /// 必须先消费购物余额至阈值以下才能领取下一笔佣金，
    /// 形成"领奖 → 复购 → 消费 → 再领奖"的闭环。
    /// 与 TTL 没收互补：TTL 是惩罚性的（过期没收），本字段是引导性的（不消费就不能继续拿新奖）。
    /// 建议设为最低商品价格以下的残余容忍值（如 5 USDT = 5_000_000）。
    pub max_shopping_balance_usdt: u64,
}

// ============================================================================
// 插件输出
// ============================================================================

/// 单条返佣输出（插件计算结果）
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CommissionOutput<AccountId, Balance> {
    pub beneficiary: AccountId,
    pub amount: Balance,
    pub commission_type: CommissionType,
    pub level: u16,
}

// ============================================================================
// CommissionPlugin Trait
// ============================================================================

/// 返佣插件接口
///
/// 每个返佣模式实现此 trait，由 core 调度引擎调用。
/// `calculate` 接收订单上下文和剩余可分配额度，返回返佣输出列表和更新后的剩余额度。
pub trait CommissionPlugin<AccountId, Balance> {
    /// 计算返佣
    ///
    /// # 参数
    /// - `entity_id`: 实体 ID（用于插件配置查询和 MemberProvider 查询推荐链）
    /// - `buyer`: 买家账户
    /// - `order_amount`: 订单金额
    /// - `remaining`: 剩余可分配额度
    /// - `enabled_modes`: 启用的返佣模式位标志
    /// - `is_first_order`: 是否首单
    /// - `buyer_order_count`: 买家订单数
    /// - `order_id`: 订单 ID（用于分佣历史记录关联）
    ///
    /// # 返回
    /// `(outputs, new_remaining)` — 返佣输出列表和剩余额度
    fn calculate(
        entity_id: u64,
        buyer: &AccountId,
        order_amount: Balance,
        remaining: Balance,
        enabled_modes: CommissionModes,
        is_first_order: bool,
        buyer_order_count: u32,
        order_id: u64,
    ) -> (Vec<CommissionOutput<AccountId, Balance>>, Balance);
}

/// 空插件实现（占位）
impl<AccountId, Balance> CommissionPlugin<AccountId, Balance> for () {
    fn calculate(
        _entity_id: u64,
        _buyer: &AccountId,
        _order_amount: Balance,
        remaining: Balance,
        _enabled_modes: CommissionModes,
        _is_first_order: bool,
        _buyer_order_count: u32,
        _order_id: u64,
    ) -> (Vec<CommissionOutput<AccountId, Balance>>, Balance) {
        (Vec::new(), remaining)
    }
}

// ============================================================================
// CommissionProvider Trait（供外部模块调用）
// ============================================================================

/// 返佣服务接口（所有方法统一使用 entity_id）
pub trait CommissionProvider<AccountId, Balance> {
    fn process_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &AccountId,
        order_amount: Balance,
        available_pool: Balance,
        platform_fee: Balance,
        product_id: u64,
        seller_reserved: Balance,
    ) -> Result<(), DispatchError>;

    fn cancel_commission(order_id: u64) -> Result<(), DispatchError>;

    fn pending_commission(entity_id: u64, account: &AccountId) -> Balance;

    fn set_commission_modes(entity_id: u64, modes: u16) -> Result<(), DispatchError>;

    fn set_direct_reward_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;

    fn set_level_diff_config(entity_id: u64, level_rates: Vec<u16>) -> Result<(), DispatchError>;

    fn set_fixed_amount(entity_id: u64, amount: Balance) -> Result<(), DispatchError>;

    fn set_first_order_config(
        entity_id: u64,
        amount: Balance,
        rate: u16,
        use_amount: bool,
    ) -> Result<(), DispatchError>;

    fn set_repeat_purchase_config(
        entity_id: u64,
        rate: u16,
        min_orders: u32,
    ) -> Result<(), DispatchError>;

    fn set_withdrawal_config_by_governance(
        entity_id: u64,
        enabled: bool,
    ) -> Result<(), DispatchError>;

    fn shopping_balance(entity_id: u64, account: &AccountId) -> Balance;

    /// 使用购物余额（由订单模块调用）
    fn use_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: Balance,
    ) -> Result<(), DispatchError>;

    /// 设置全局最低复购比例（由 Governance 调用，万分比）
    fn set_min_repurchase_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;

    /// 设置 Owner 收益比例（基点，从 Pool B 预算中优先扣除）
    fn set_owner_reward_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;

    /// 订单完结时结算佣金记录（Pending → Withdrawn）
    ///
    /// 由订单模块在订单完结（确认收货/超时完成）时调用，
    /// 标记佣金记录为已结算，使其可以被 archive_order_records 归档。
    fn settle_order_commission(order_id: u64) -> Result<(), DispatchError>;

    // ==================== 购物余额分佣通道 ====================

    /// 购物余额支付的分佣处理（资金来源为 Entity 国库，无平台费/无池 A）
    fn process_shopping_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &AccountId,
        shopping_amount: Balance,
        product_id: u64,
    ) -> Result<(), DispatchError>;

    /// 取消购物余额分佣（纯记账回滚，无链上转账）
    fn cancel_shopping_commission(order_id: u64) -> Result<(), DispatchError>;

    /// 购物余额分佣结算（Pending → Settled）
    fn settle_order_shopping_commission(order_id: u64) -> Result<(), DispatchError>;

    // ==================== R10: 治理提案链上执行接口 ====================

    /// 设置最大返佣比率（治理提案执行）
    fn governance_set_commission_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError> {
        let _ = (entity_id, rate);
        Ok(())
    }

    /// 返佣总开关（治理提案执行）
    fn governance_toggle_commission(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }
}

/// 空 CommissionProvider 实现
pub struct NullCommissionProvider;

impl<AccountId, Balance: Default> CommissionProvider<AccountId, Balance>
    for NullCommissionProvider
{
    fn process_commission(
        _: u64,
        _: u64,
        _: u64,
        _: &AccountId,
        _: Balance,
        _: Balance,
        _: Balance,
        _: u64,
        _: Balance,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn cancel_commission(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn pending_commission(_: u64, _: &AccountId) -> Balance {
        Balance::default()
    }
    fn set_commission_modes(_: u64, _: u16) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_direct_reward_rate(_: u64, _: u16) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_level_diff_config(_: u64, _: Vec<u16>) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_fixed_amount(_: u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_first_order_config(_: u64, _: Balance, _: u16, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_repeat_purchase_config(_: u64, _: u16, _: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_withdrawal_config_by_governance(_: u64, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
    fn shopping_balance(_: u64, _: &AccountId) -> Balance {
        Balance::default()
    }
    fn use_shopping_balance(_: u64, _: &AccountId, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_min_repurchase_rate(_: u64, _: u16) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_owner_reward_rate(_: u64, _: u16) -> Result<(), DispatchError> {
        Ok(())
    }
    fn settle_order_commission(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn process_shopping_commission(
        _: u64,
        _: u64,
        _: u64,
        _: &AccountId,
        _: Balance,
        _: u64,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn cancel_shopping_commission(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn settle_order_shopping_commission(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// MemberProvider Trait — 从 pallet-entity-common 统一导出
// ============================================================================

/// 从 `pallet-entity-common` 统一导出，消除重复定义。
pub use pallet_entity_common::{MemberProvider, NullMemberProvider};

// ============================================================================
// EntityReferrerProvider — 招商推荐人查询接口
// ============================================================================

/// 招商推荐人查询接口（供 commission-core 查询 Entity 级推荐人）
pub trait EntityReferrerProvider<AccountId> {
    /// 获取 Entity 的招商推荐人
    fn entity_referrer(entity_id: u64) -> Option<AccountId>;

    /// 获取推荐人绑定区块号（用于保护期计算）
    /// 默认返回 None（不支持保护期）
    fn referrer_bound_at(entity_id: u64) -> Option<u64> {
        let _ = entity_id;
        None
    }
}

/// 空 EntityReferrerProvider 实现
impl<AccountId> EntityReferrerProvider<AccountId> for () {
    fn entity_referrer(_entity_id: u64) -> Option<AccountId> {
        None
    }
}

// ============================================================================
// PlanWriter Traits — 插件写入接口
// ============================================================================

/// 推荐链插件写入接口（由 commission-referral 实现）
pub trait ReferralPlanWriter<Balance> {
    /// 设置直推奖励比例
    fn set_direct_rate(entity_id: u64, rate: u16) -> Result<(), DispatchError>;
    /// 设置固定金额奖励
    fn set_fixed_amount(entity_id: u64, amount: Balance) -> Result<(), DispatchError>;
    /// 设置首单奖励
    fn set_first_order(
        entity_id: u64,
        amount: Balance,
        rate: u16,
        use_amount: bool,
    ) -> Result<(), DispatchError>;
    /// 设置复购奖励
    fn set_repeat_purchase(entity_id: u64, rate: u16, min_orders: u32)
        -> Result<(), DispatchError>;
    /// 设置推荐人资格门槛
    fn set_referrer_guard(
        entity_id: u64,
        min_referrer_spent: u128,
        min_referrer_orders: u32,
    ) -> Result<(), DispatchError>;
    /// 设置返佣上限
    fn set_commission_cap(
        entity_id: u64,
        max_per_order: Balance,
        max_total_earned: Balance,
    ) -> Result<(), DispatchError>;
    /// 清除全部推荐链配置
    fn clear_config(entity_id: u64) -> Result<(), DispatchError>;
}

/// 空 ReferralPlanWriter 实现
impl<Balance> ReferralPlanWriter<Balance> for () {
    fn set_direct_rate(_: u64, _: u16) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_fixed_amount(_: u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_first_order(_: u64, _: Balance, _: u16, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_repeat_purchase(_: u64, _: u16, _: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_referrer_guard(_: u64, _: u128, _: u32) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_commission_cap(_: u64, _: Balance, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 多级分销插件写入接口（由 commission-multi-level 实现）
pub trait MultiLevelPlanWriter {
    /// 设置多级分销（每级比例列表，激活条件全为 0）
    fn set_multi_level(entity_id: u64, level_rates: Vec<u16>) -> Result<(), DispatchError>;
    /// F7: 设置多级分销（含完整激活条件）
    ///
    /// tiers: Vec<(rate, required_directs, required_team_size, required_spent, required_level_id)>
    fn set_multi_level_full(
        entity_id: u64,
        tiers: Vec<(u16, u32, u32, u128, u8)>,
    ) -> Result<(), DispatchError>;
    /// 清除多级分销配置
    fn clear_multi_level_config(entity_id: u64) -> Result<(), DispatchError>;
    /// 暂停多级分销（治理调用）
    fn governance_pause(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(())
    }
    /// 恢复多级分销（治理调用）
    fn governance_resume(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(())
    }
}

/// 空 MultiLevelPlanWriter 实现
impl MultiLevelPlanWriter for () {
    fn set_multi_level(_: u64, _: Vec<u16>) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_multi_level_full(
        _: u64,
        _: Vec<(u16, u32, u32, u128, u8)>,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_multi_level_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 等级极差插件写入接口（由 commission-level-diff 实现）
pub trait LevelDiffPlanWriter {
    /// 设置自定义等级极差比例（level_rates: 每个自定义等级对应的 bps）
    fn set_level_rates(
        entity_id: u64,
        level_rates: Vec<u16>,
        max_depth: u8,
    ) -> Result<(), DispatchError>;
    /// 清除等级极差配置
    fn clear_config(entity_id: u64) -> Result<(), DispatchError>;
}

/// 空 LevelDiffPlanWriter 实现
impl LevelDiffPlanWriter for () {
    fn set_level_rates(_: u64, _: Vec<u16>, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 团队业绩插件写入接口（由 commission-team 实现）
pub trait TeamPlanWriter<Balance> {
    /// 设置团队业绩阶梯配置
    ///
    /// tiers: Vec<(sales_threshold_u128, min_team_size, rate_bps)>
    /// threshold_mode: 0=Nex, 1=Usdt
    fn set_team_config(
        entity_id: u64,
        tiers: Vec<(u128, u32, u16)>,
        max_depth: u8,
        allow_stacking: bool,
        threshold_mode: u8,
    ) -> Result<(), DispatchError>;
    /// 清除团队业绩配置
    fn clear_config(entity_id: u64) -> Result<(), DispatchError>;
    /// 暂停团队业绩返佣（治理调用）
    fn governance_pause(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(())
    }
    /// 恢复团队业绩返佣（治理调用）
    fn governance_resume(entity_id: u64) -> Result<(), DispatchError> {
        let _ = entity_id;
        Ok(())
    }
}

/// 空 TeamPlanWriter 实现
impl<Balance> TeamPlanWriter<Balance> for () {
    fn set_team_config(
        _: u64,
        _: Vec<(u128, u32, u16)>,
        _: u8,
        _: bool,
        _: u8,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 单线收益插件写入接口（由 commission-single-line 实现）
pub trait SingleLinePlanWriter {
    /// 设置单线收益配置
    ///
    /// rates: (upline_rate_bps, downline_rate_bps), max 1000 each
    /// base_levels: (base_upline, base_downline)
    /// max_levels: (max_upline, max_downline)
    /// level_increment_threshold: u128 encoded threshold
    /// reach_mode: 0=Bidirectional, 1=BuyerOnly, 2=BeneficiaryOnly
    #[allow(clippy::too_many_arguments)]
    fn set_single_line_config(
        entity_id: u64,
        upline_rate: u16,
        downline_rate: u16,
        base_upline_levels: u8,
        base_downline_levels: u8,
        level_increment_threshold: u128,
        max_upline_levels: u8,
        max_downline_levels: u8,
        reach_mode: u8,
    ) -> Result<(), DispatchError>;
    /// 清除单线收益配置
    fn clear_config(entity_id: u64) -> Result<(), DispatchError>;
    /// 设置按等级自定义层数覆盖
    fn set_level_based_levels(
        entity_id: u64,
        level_id: u8,
        upline_levels: u8,
        downline_levels: u8,
    ) -> Result<(), DispatchError>;
    /// 清除指定等级的层数覆盖
    fn clear_level_overrides(entity_id: u64, level_id: u8) -> Result<(), DispatchError>;
}

/// 空 SingleLinePlanWriter 实现
impl SingleLinePlanWriter for () {
    fn set_single_line_config(
        _: u64,
        _: u16,
        _: u16,
        _: u8,
        _: u8,
        _: u128,
        _: u8,
        _: u8,
        _: u8,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_level_based_levels(_: u64, _: u8, _: u8, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_level_overrides(_: u64, _: u8) -> Result<(), DispatchError> {
        Ok(())
    }
}

/// 沉淀池奖励插件写入接口（由 commission-pool-reward 实现）
///
/// v2: 周期性等额分配模型——level_rules 使用完整规则，round_duration 为区块数
pub trait PoolRewardPlanWriter {
    /// 设置沉淀池奖励配置
    fn set_pool_reward_config(
        entity_id: u64,
        level_rules: Vec<(u8, PoolRewardLevelClaimRule)>,
        round_duration: u32,
    ) -> Result<(), DispatchError>;
    /// 清除沉淀池奖励配置
    fn clear_config(entity_id: u64) -> Result<(), DispatchError>;
    /// 设置 Entity Token 池奖励是否启用（默认 no-op）
    fn set_token_pool_enabled(entity_id: u64, enabled: bool) -> Result<(), DispatchError> {
        let _ = (entity_id, enabled);
        Ok(())
    }
}

/// 空 PoolRewardPlanWriter 实现
impl PoolRewardPlanWriter for () {
    fn set_pool_reward_config(
        _: u64,
        _: Vec<(u8, PoolRewardLevelClaimRule)>,
        _: u32,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn clear_config(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn set_token_pool_enabled(_: u64, _: bool) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// PoolBalanceProvider — 沉淀池余额读写接口
// ============================================================================

/// 沉淀池余额读写接口（由 commission-core 实现，供 pool-reward 访问）
pub trait PoolBalanceProvider<Balance> {
    /// 查询沉淀池余额
    fn pool_balance(entity_id: u64) -> Balance;
    /// 从沉淀池扣减指定金额
    fn deduct_pool(entity_id: u64, amount: Balance) -> Result<(), DispatchError>;
}

/// 空 PoolBalanceProvider 实现
impl<Balance: Default> PoolBalanceProvider<Balance> for () {
    fn pool_balance(_: u64) -> Balance {
        Balance::default()
    }
    fn deduct_pool(_: u64, _: Balance) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// Token 多资产扩展（方案 A: 全插件管线多资产化）
// ============================================================================

/// Token 佣金插件接口（与 CommissionPlugin 对称，Balance → TokenBalance）
///
/// 每个返佣插件额外实现此 trait，由 core 的 Token 调度管线调用。
/// 签名与 `CommissionPlugin` 完全一致，仅类型语义不同。
pub trait TokenCommissionPlugin<AccountId, TokenBalance> {
    fn calculate_token(
        entity_id: u64,
        buyer: &AccountId,
        order_amount: TokenBalance,
        remaining: TokenBalance,
        enabled_modes: CommissionModes,
        is_first_order: bool,
        buyer_order_count: u32,
        order_id: u64,
    ) -> (Vec<CommissionOutput<AccountId, TokenBalance>>, TokenBalance);
}

/// 空 TokenCommissionPlugin 实现
impl<AccountId, TokenBalance> TokenCommissionPlugin<AccountId, TokenBalance> for () {
    fn calculate_token(
        _: u64,
        _: &AccountId,
        _: TokenBalance,
        remaining: TokenBalance,
        _: CommissionModes,
        _: bool,
        _: u32,
        _: u64,
    ) -> (Vec<CommissionOutput<AccountId, TokenBalance>>, TokenBalance) {
        (Vec::new(), remaining)
    }
}

// ============================================================================
// PluginStatsRollback Trait — 插件内部统计回滚（订单取消时由 core 调用）
// ============================================================================

/// 插件统计回滚接口
///
/// 插件在 `calculate` 时可能更新自身统计（如 MemberMultiLevelStats）。
/// 当订单取消时，core 引擎通过此 trait 通知插件回滚已记录的统计。
///
/// `rollback_stats` 接收被取消的佣金记录列表，插件根据 `commission_type`
/// 过滤属于自己的记录并回滚相应统计。
///
/// `count_order`: 与 calculate 对称，NEX 管道传 true，Token 管道传 false。
pub trait PluginStatsRollback<AccountId> {
    fn rollback_stats(
        entity_id: u64,
        cancelled_outputs: &[(AccountId, u128, CommissionType, u16)],
        count_order: bool,
    );
}

/// 空 PluginStatsRollback 实现
impl<AccountId> PluginStatsRollback<AccountId> for () {
    fn rollback_stats(_: u64, _: &[(AccountId, u128, CommissionType, u16)], _: bool) {}
}

/// Token 佣金记录（与 CommissionRecord 对称，无 shop_id —— Token 佣金不区分 Shop）
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
pub struct TokenCommissionRecord<AccountId, TokenBalance, BlockNumber> {
    pub entity_id: u64,
    pub order_id: u64,
    pub buyer: AccountId,
    pub beneficiary: AccountId,
    pub amount: TokenBalance,
    pub commission_type: CommissionType,
    pub level: u16,
    pub status: CommissionStatus,
    pub created_at: BlockNumber,
}

/// Token 佣金统计（含复购分流统计）
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
    Default,
)]
pub struct MemberTokenCommissionStatsData<TokenBalance> {
    pub total_earned: TokenBalance,
    pub pending: TokenBalance,
    pub withdrawn: TokenBalance,
    pub repurchased: TokenBalance,
    pub order_count: u32,
}

// ============================================================================
// TokenTransferProvider — Token 转账接口（entity_id 级）
// ============================================================================

/// Token 转账接口（entity_id 级，简化 fungibles 接口）
///
/// 由 runtime 实现（委托 EntityTokenProvider 或 pallet-assets）。
/// commission-core 通过此 trait 执行 Token 提现和余额查询。
pub trait TokenTransferProvider<AccountId, TokenBalance> {
    /// 获取指定账户在某 Entity 下的可用 Token 余额
    fn token_balance_of(entity_id: u64, who: &AccountId) -> TokenBalance;

    /// Token 转账: from → to（entity_id 级）
    fn token_transfer(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: TokenBalance,
    ) -> Result<(), DispatchError>;
}

/// 空 TokenTransferProvider 实现
impl<AccountId, TokenBalance: Default> TokenTransferProvider<AccountId, TokenBalance> for () {
    fn token_balance_of(_: u64, _: &AccountId) -> TokenBalance {
        TokenBalance::default()
    }
    fn token_transfer(
        _: u64,
        _: &AccountId,
        _: &AccountId,
        _: TokenBalance,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// TokenPoolBalanceProvider — Token 沉淀池余额读写接口
// ============================================================================

/// Token 沉淀池余额读写接口（由 commission-core 实现，供 pool-reward 访问）
pub trait TokenPoolBalanceProvider<TokenBalance> {
    fn token_pool_balance(entity_id: u64) -> TokenBalance;
    fn deduct_token_pool(entity_id: u64, amount: TokenBalance) -> Result<(), DispatchError>;
}

/// 空 TokenPoolBalanceProvider 实现
impl<TokenBalance: Default> TokenPoolBalanceProvider<TokenBalance> for () {
    fn token_pool_balance(_: u64) -> TokenBalance {
        TokenBalance::default()
    }
    fn deduct_token_pool(_: u64, _: TokenBalance) -> Result<(), DispatchError> {
        Ok(())
    }
}

// ============================================================================
// TokenCommissionProvider — Token 佣金服务接口（供 transaction 模块调用）
// ============================================================================

/// Token 佣金服务接口（全插件 Token 管线对外入口）
pub trait TokenCommissionProvider<AccountId, TokenBalance> {
    fn process_token_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &AccountId,
        token_order_amount: TokenBalance,
        token_available_pool: TokenBalance,
        token_platform_fee: TokenBalance,
        product_id: u64,
    ) -> Result<(), DispatchError>;

    fn cancel_token_commission(order_id: u64) -> Result<(), DispatchError>;

    fn pending_token_commission(entity_id: u64, account: &AccountId) -> TokenBalance;

    /// 获取 Entity 级 Token 平台费率（bps，0 = 不收费）
    fn token_platform_fee_rate(entity_id: u64) -> u16;
}

// ============================================================================
// ParticipationGuard — Entity 参与权守卫（KYC / 合规检查）
// ============================================================================

/// Entity 参与权守卫（KYC / 合规检查接口）
///
/// 在 `withdraw_commission`、`claim_pool_reward`、`do_consume_shopping_balance` 中调用，
/// 确保 target 账户满足 Entity 的参与要求（如 mandatory KYC）。
/// 默认空实现允许所有操作（适用于未配置 KYC 的 Entity）。
pub trait ParticipationGuard<AccountId> {
    fn can_participate(entity_id: u64, account: &AccountId) -> bool;
}

/// 默认空实现（无 KYC 系统时使用，所有账户均允许）
impl<AccountId> ParticipationGuard<AccountId> for () {
    fn can_participate(_entity_id: u64, _account: &AccountId) -> bool {
        true
    }
}

// ============================================================================
// CrossChainPayout — 原生 NEX 跨链派发端口 (HB-WD-01)
// CrossChainPayout — cross-chain native-NEX payout port (HB-WD-01)
// ============================================================================

/// Cross-chain native-NEX payout port (HB-WD-01).
/// 原生 NEX 跨链派发端口（HB-WD-01）。
///
/// `pallet-commission-core` calls this to bridge the *withdrawal* part of a
/// promoter's NEX commission out to an EVM chain, where the promoter receives
/// native NEX (and self-swaps to a stablecoin). The runtime wires it to the
/// bridge's outbound core (`pallet-bridge-ismp::do_outbound`: burn NEX → ISMP
/// POST), keeping commission-core decoupled from the bridge crate. This is the
/// business→bridge counterpart of the bridge→business
/// `CrossChainOrderHandler` used by HB-ENT-01.
/// `pallet-commission-core` 经此把推广员 NEX 佣金的「withdrawal」部分桥到某 EVM 链，
/// 推广员在该链收到原生 NEX（自行换稳定币）。runtime 将其对接桥的出站核心
///（`pallet-bridge-ismp::do_outbound`：burn NEX → ISMP POST），使 commission-core
/// 不直接依赖桥 crate。这是 HB-ENT-01 中 `CrossChainOrderHandler`（桥→业务）的对偶
///（业务→桥）。
///
/// Amounts are passed in local NEX precision (`Balance`); ERC20 precision
/// conversion (12→18) happens inside the bridge. On success returns the ISMP
/// request commitment (32 bytes). Must be invoked inside a transactional layer
/// (extrinsics auto-wrap) so a returned error rolls back the burn and the
/// pending decrement together.
/// 金额以本地 NEX 精度（`Balance`）传入；ERC20 精度换算（12→18）在桥内发生。
/// 成功返回 ISMP 请求 commitment（32 字节）。须在事务层内调用（extrinsic 自动包裹），
/// 以便返回错误时一并回滚 burn 与 pending 扣减。
pub trait CrossChainPayout<AccountId, Balance> {
    /// Bridge `amount` NEX out of `from`, minting native NEX to `recipient` on
    /// the EVM chain identified by `evm_chain_id`.
    /// 从 `from` 桥出 `amount` NEX，在 `evm_chain_id` 标识的 EVM 链向 `recipient`
    /// 铸造原生 NEX。
    fn payout_native(
        from: &AccountId,
        evm_chain_id: u32,
        recipient: [u8; 20],
        amount: Balance,
    ) -> Result<[u8; 32], sp_runtime::DispatchError>;
}

/// Default `()`: cross-chain payout disabled — every attempt fails (and, because
/// the caller runs in a transactional layer, leaves pending untouched).
/// 默认 `()`：未接线即关闭跨链提现——所有尝试失败（且因调用方在事务层内，pending 不动）。
impl<AccountId, Balance> CrossChainPayout<AccountId, Balance> for () {
    fn payout_native(
        _from: &AccountId,
        _evm_chain_id: u32,
        _recipient: [u8; 20],
        _amount: Balance,
    ) -> Result<[u8; 32], sp_runtime::DispatchError> {
        Err(sp_runtime::DispatchError::Other(
            "cross-chain payout not configured",
        ))
    }
}

/// 空 TokenCommissionProvider 实现
pub struct NullTokenCommissionProvider;

impl<AccountId, TokenBalance: Default> TokenCommissionProvider<AccountId, TokenBalance>
    for NullTokenCommissionProvider
{
    fn process_token_commission(
        _: u64,
        _: u64,
        _: u64,
        _: &AccountId,
        _: TokenBalance,
        _: TokenBalance,
        _: TokenBalance,
        _: u64,
    ) -> Result<(), DispatchError> {
        Ok(())
    }
    fn cancel_token_commission(_: u64) -> Result<(), DispatchError> {
        Ok(())
    }
    fn pending_token_commission(_: u64, _: &AccountId) -> TokenBalance {
        TokenBalance::default()
    }
    fn token_platform_fee_rate(_: u64) -> u16 {
        0
    }
}

// ============================================================================
// QueryProvider Traits — 子模块查询接口（供 Runtime API 聚合层使用）
// ============================================================================

/// 多级分销查询接口
pub trait MultiLevelQueryProvider<AccountId> {
    /// 激活进度（每层级的当前值 vs 要求值）
    fn activation_progress(entity_id: u64, account: &AccountId) -> Vec<MultiLevelActivationInfo>;
    /// 是否暂停
    fn is_paused(entity_id: u64) -> bool;
    /// 会员多级佣金统计
    fn member_stats(entity_id: u64, account: &AccountId) -> Option<MultiLevelMemberStats>;
    /// 查询 Entity 多级分销配置的层数（无配置返回 0）
    fn tier_count(entity_id: u64) -> u16 {
        let _ = entity_id;
        0
    }
}

/// 激活进度（Runtime API 可编解码版本，不依赖 Config）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct MultiLevelActivationInfo {
    pub level: u16,
    pub activated: bool,
    pub directs_current: u32,
    pub directs_required: u32,
    pub team_current: u32,
    pub team_required: u32,
    pub spent_current: u128,
    pub spent_required: u128,
    /// BUG-2 修复: 会员当前自定义等级 ID
    pub level_id_current: u8,
    /// BUG-2 修复: 该层级要求的最低等级 ID（0 = 无要求）
    pub level_id_required: u8,
}

/// 多级佣金会员统计
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug, Default)]
pub struct MultiLevelMemberStats {
    pub total_earned: u128,
    pub total_orders: u32,
    pub last_commission_block: u32,
}

/// 空 MultiLevelQueryProvider 实现
impl<AccountId> MultiLevelQueryProvider<AccountId> for () {
    fn activation_progress(_: u64, _: &AccountId) -> Vec<MultiLevelActivationInfo> {
        Vec::new()
    }
    fn is_paused(_: u64) -> bool {
        false
    }
    fn member_stats(_: u64, _: &AccountId) -> Option<MultiLevelMemberStats> {
        None
    }
}

/// 团队业绩查询接口
pub trait TeamQueryProvider<AccountId, Balance> {
    /// 查询会员匹配的阶梯档位
    /// 返回 (tier_index, rate_bps, next_threshold, next_min_team_size)
    fn matched_tier(entity_id: u64, account: &AccountId) -> Option<TeamTierInfo<Balance>>;
    /// 查询团队业绩模块状态 (config_exists, is_enabled)
    fn status(entity_id: u64) -> (bool, bool);
    /// 查询团队业绩配置的最大遍历深度（无配置返回 0）
    fn chain_depth(entity_id: u64) -> u16 {
        let _ = entity_id;
        0
    }
}

/// 团队阶梯快照
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct TeamTierInfo<Balance> {
    pub tier_index: u8,
    pub rate: u16,
    pub next_threshold: Option<Balance>,
    pub next_min_team_size: Option<u32>,
}

/// 空 TeamQueryProvider 实现
impl<AccountId, Balance> TeamQueryProvider<AccountId, Balance> for () {
    fn matched_tier(_: u64, _: &AccountId) -> Option<TeamTierInfo<Balance>> {
        None
    }
    fn status(_: u64) -> (bool, bool) {
        (false, false)
    }
}

/// 单线收益查询接口
pub trait SingleLineQueryProvider<AccountId> {
    /// 全局排位
    fn position(entity_id: u64, account: &AccountId) -> Option<u32>;
    /// 有效搜索层数 (upline_levels, downline_levels)
    fn effective_levels(entity_id: u64, account: &AccountId) -> Option<(u8, u8)>;
    /// 单线是否启用
    fn is_enabled(entity_id: u64) -> bool;
    /// 排队总长度
    fn queue_length(entity_id: u64) -> u32;
    /// 查询单线配置的最大链深度（取 upline/downline 较大值，无配置返回 0）
    fn chain_depth(entity_id: u64) -> u16 {
        let _ = entity_id;
        0
    }
}

/// 空 SingleLineQueryProvider 实现
impl<AccountId> SingleLineQueryProvider<AccountId> for () {
    fn position(_: u64, _: &AccountId) -> Option<u32> {
        None
    }
    fn effective_levels(_: u64, _: &AccountId) -> Option<(u8, u8)> {
        None
    }
    fn is_enabled(_: u64) -> bool {
        false
    }
    fn queue_length(_: u64) -> u32 {
        0
    }
}

/// 沉淀池奖励查询接口
pub trait PoolRewardQueryProvider<AccountId, Balance, TokenBalance> {
    /// 可领取金额 (nex, token)
    fn claimable(entity_id: u64, account: &AccountId) -> (Balance, TokenBalance);
    /// 是否暂停
    fn is_paused(entity_id: u64) -> bool;
    /// 当前轮次 ID
    fn current_round_id(entity_id: u64) -> u64;
}

/// 空 PoolRewardQueryProvider 实现
impl<AccountId, Balance: Default, TokenBalance: Default>
    PoolRewardQueryProvider<AccountId, Balance, TokenBalance> for ()
{
    fn claimable(_: u64, _: &AccountId) -> (Balance, TokenBalance) {
        (Balance::default(), TokenBalance::default())
    }
    fn is_paused(_: u64) -> bool {
        false
    }
    fn current_round_id(_: u64) -> u64 {
        0
    }
}

/// 推荐链返佣查询接口
pub trait ReferralQueryProvider<AccountId, Balance> {
    /// 推荐人累计获佣
    fn referrer_total_earned(entity_id: u64, account: &AccountId) -> Balance;
    /// 返佣上限配置 (max_per_order, max_total_earned)；None 表示未配置
    fn cap_config(entity_id: u64) -> Option<(Balance, Balance)>;
}

/// 空 ReferralQueryProvider 实现
impl<AccountId, Balance: Default> ReferralQueryProvider<AccountId, Balance> for () {
    fn referrer_total_earned(_: u64, _: &AccountId) -> Balance {
        Balance::default()
    }
    fn cap_config(_: u64) -> Option<(Balance, Balance)> {
        None
    }
}

// ============================================================================
// PoolFundingCallback — 沉淀池资金来源记录回调
// ============================================================================

/// 沉淀池资金来源类型
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
pub enum FundingSource {
    /// NEX 订单佣金剩余 → 沉淀池
    OrderCommissionRemainder,
    /// Token 平台费留存 → Token 沉淀池
    TokenPlatformFeeRetention,
    /// Token 佣金剩余 → Token 沉淀池
    TokenCommissionRemainder,
    /// NEX 取消订单退回 → 沉淀池（池奖励类型的佣金退回）
    CancelReturn,
}

/// 沉淀池资金来源记录回调（由 commission-core 调用，pool-reward 实现）
///
/// 在每次 UnallocatedPool / UnallocatedTokenPool 增减时通知 pool-reward 记录来源。
pub trait PoolFundingCallback {
    /// 记录一笔资金入池
    ///
    /// - `entity_id`: 实体 ID
    /// - `source`: 资金来源类型
    /// - `nex_amount`: NEX 入池金额（Token 类来源传 0）
    /// - `token_amount`: Token 入池金额（NEX 类来源传 0）
    /// - `order_id`: 关联订单 ID
    fn on_pool_funded(
        entity_id: u64,
        source: FundingSource,
        nex_amount: u128,
        token_amount: u128,
        order_id: u64,
    );
}

/// 空 PoolFundingCallback 实现
impl PoolFundingCallback for () {
    fn on_pool_funded(_: u64, _: FundingSource, _: u128, _: u128, _: u64) {}
}

// ============================================================================
// LevelDiffQueryProvider — 级差插件链深度查询
// ============================================================================

/// 级差插件查询接口（链深度）
pub trait LevelDiffQueryProvider {
    /// 查询级差配置的最大遍历深度（无配置返回 0）
    fn chain_depth(entity_id: u64) -> u16 {
        let _ = entity_id;
        0
    }
}

/// 空 LevelDiffQueryProvider 实现
impl LevelDiffQueryProvider for () {}

// ============================================================================
// PluginBudgetCapProvider — 插件预算上限查询接口
// ============================================================================

/// 插件预算上限查询接口（由 commission-core 实现，供各插件读取自身的 budget cap）
pub trait PluginBudgetCapProvider {
    /// 查询多级分销插件的预算上限（bps，0 = 无上限）
    fn multi_level_cap(entity_id: u64) -> u16;
    /// 查询推荐返佣插件的预算上限（bps，0 = 无上限）
    fn referral_cap(entity_id: u64) -> u16;
    /// 查询等级极差插件的预算上限（bps，0 = 无上限）
    fn level_diff_cap(entity_id: u64) -> u16;
    /// 查询单线排队插件的预算上限（bps，0 = 无上限）
    fn single_line_cap(entity_id: u64) -> u16;
    /// 查询团队业绩插件的预算上限（bps，0 = 无上限）
    fn team_cap(entity_id: u64) -> u16;
}

/// 空实现（无上限）
impl PluginBudgetCapProvider for () {
    fn multi_level_cap(_: u64) -> u16 {
        0
    }
    fn referral_cap(_: u64) -> u16 {
        0
    }
    fn level_diff_cap(_: u64) -> u16 {
        0
    }
    fn single_line_cap(_: u64) -> u16 {
        0
    }
    fn team_cap(_: u64) -> u16 {
        0
    }
}

// ============================================================================
// NEX ↔ USDT 辅助函数
// ============================================================================

/// NEX 金额转 USDT（精度 10^6）
///
/// 公式：usdt = (nex_amount × nex_usdt_rate) / 10^12
/// - nex_amount: NEX 精度 10^12
/// - nex_usdt_rate: PricingProvider 返回的价格（USDT per NEX × 10^6）
/// - 返回值: USDT 精度 10^6
///
/// 示例：1 NEX = 0.5 USDT → nex_usdt_rate = 500_000
///       nex_to_usdt(1_000_000_000_000, 500_000) = 500_000 (= 0.5 USDT)
pub fn nex_to_usdt(nex_amount: u128, nex_usdt_rate: u64) -> u64 {
    (nex_amount.saturating_mul(nex_usdt_rate as u128) / 1_000_000_000_000u128) as u64
}

/// USDT 金额转 NEX（精度 10^12）
///
/// 公式：nex = (usdt_amount × 10^12) / nex_usdt_rate
/// - usdt_amount: USDT 精度 10^6
/// - nex_usdt_rate: PricingProvider 返回的价格
/// - 返回值: NEX 精度 10^12
///
/// 示例：0.5 USDT → 1 NEX（当 rate=500_000 时）
///       usdt_to_nex(500_000, 500_000) = 1_000_000_000_000
pub fn usdt_to_nex(usdt_amount: u64, nex_usdt_rate: u64) -> u128 {
    if nex_usdt_rate == 0 {
        return 0;
    }
    (usdt_amount as u128).saturating_mul(1_000_000_000_000u128) / (nex_usdt_rate as u128)
}

/// 购物余额（NEX）转 USDT — nex_to_usdt 的语义别名
///
/// 购物余额以 NEX 精度 10^12 存储，折算为 USDT 用于复购门槛比较。
#[inline]
pub fn shopping_bal_to_usdt(shopping_balance: u128, nex_usdt_rate: u64) -> u64 {
    nex_to_usdt(shopping_balance, nex_usdt_rate)
}

/// Token 购物余额转 USDT（精度 10^6）
///
/// Token 购物余额以 Token 精度 10^12 存储，折算为 USDT 用于阈值比较。
/// 公式与 nex_to_usdt 相同：usdt = (token_amount × token_usdt_rate) / 10^12
#[inline]
pub fn token_shopping_bal_to_usdt(token_balance: u128, token_usdt_rate: u64) -> u64 {
    (token_balance.saturating_mul(token_usdt_rate as u128) / 1_000_000_000_000u128) as u64
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CommissionModes ----

    #[test]
    fn modes_default_enables_three_plugins() {
        let m = CommissionModes::default();
        assert!(m.contains(CommissionModes::SINGLE_LINE_UPLINE));
        assert!(m.contains(CommissionModes::SINGLE_LINE_DOWNLINE));
        assert!(m.contains(CommissionModes::MULTI_LEVEL));
        assert!(m.contains(CommissionModes::POOL_REWARD));
        // 其余模式默认关闭
        assert!(!m.contains(CommissionModes::DIRECT_REWARD));
        assert!(!m.contains(CommissionModes::TEAM_PERFORMANCE));
        assert!(!m.contains(CommissionModes::LEVEL_DIFF));
        assert!(!m.contains(CommissionModes::OWNER_REWARD));
        assert!(m.is_valid());
    }

    #[test]
    fn modes_single_flag() {
        let m = CommissionModes(CommissionModes::DIRECT_REWARD);
        assert!(m.contains(CommissionModes::DIRECT_REWARD));
        assert!(!m.contains(CommissionModes::MULTI_LEVEL));
        assert!(m.is_valid());
    }

    #[test]
    fn modes_combined_flags() {
        let m = CommissionModes(
            CommissionModes::DIRECT_REWARD
                | CommissionModes::POOL_REWARD
                | CommissionModes::OWNER_REWARD,
        );
        assert!(m.contains(CommissionModes::DIRECT_REWARD));
        assert!(m.contains(CommissionModes::POOL_REWARD));
        assert!(m.contains(CommissionModes::OWNER_REWARD));
        assert!(!m.contains(CommissionModes::TEAM_PERFORMANCE));
        assert!(m.is_valid());
    }

    #[test]
    fn modes_all_valid_bits() {
        let m = CommissionModes(CommissionModes::ALL_VALID);
        assert!(m.is_valid());
        // 11 个模式位全部包含
        assert!(m.contains(CommissionModes::DIRECT_REWARD));
        assert!(m.contains(CommissionModes::MULTI_LEVEL));
        assert!(m.contains(CommissionModes::TEAM_PERFORMANCE));
        assert!(m.contains(CommissionModes::LEVEL_DIFF));
        assert!(m.contains(CommissionModes::FIXED_AMOUNT));
        assert!(m.contains(CommissionModes::FIRST_ORDER));
        assert!(m.contains(CommissionModes::REPEAT_PURCHASE));
        assert!(m.contains(CommissionModes::SINGLE_LINE_UPLINE));
        assert!(m.contains(CommissionModes::SINGLE_LINE_DOWNLINE));
        assert!(m.contains(CommissionModes::POOL_REWARD));
        assert!(m.contains(CommissionModes::OWNER_REWARD));
    }

    #[test]
    fn modes_invalid_high_bit() {
        // 设置一个未定义的高位
        let m = CommissionModes(0b1000_0000_0000);
        assert!(!m.is_valid());
    }

    #[test]
    fn modes_mixed_valid_and_invalid() {
        let m = CommissionModes(CommissionModes::DIRECT_REWARD | 0b1000_0000_0000);
        assert!(!m.is_valid());
    }

    #[test]
    fn modes_insert_and_remove() {
        let mut m = CommissionModes(CommissionModes::NONE);
        assert!(!m.contains(CommissionModes::MULTI_LEVEL));

        m.insert(CommissionModes::MULTI_LEVEL);
        assert!(m.contains(CommissionModes::MULTI_LEVEL));

        m.insert(CommissionModes::FIRST_ORDER);
        assert!(m.contains(CommissionModes::FIRST_ORDER));
        assert!(m.contains(CommissionModes::MULTI_LEVEL));

        m.remove(CommissionModes::MULTI_LEVEL);
        assert!(!m.contains(CommissionModes::MULTI_LEVEL));
        assert!(m.contains(CommissionModes::FIRST_ORDER));
        assert!(m.is_valid());
    }

    // ---- WithdrawalTierConfig ----

    #[test]
    fn withdrawal_tier_default_is_valid() {
        let tier = WithdrawalTierConfig::default();
        assert_eq!(tier.withdrawal_rate, 10000);
        assert_eq!(tier.repurchase_rate, 0);
        assert!(tier.is_valid());
    }

    #[test]
    fn withdrawal_tier_valid_split() {
        let tier = WithdrawalTierConfig {
            withdrawal_rate: 7000,
            repurchase_rate: 3000,
        };
        assert!(tier.is_valid());
    }

    #[test]
    fn withdrawal_tier_invalid_sum() {
        let tier = WithdrawalTierConfig {
            withdrawal_rate: 5000,
            repurchase_rate: 4000,
        };
        assert!(!tier.is_valid());
    }

    #[test]
    fn withdrawal_tier_overflow_saturates() {
        // u16::MAX + u16::MAX 会 saturate 到 u16::MAX，不等于 10000
        let tier = WithdrawalTierConfig {
            withdrawal_rate: u16::MAX,
            repurchase_rate: u16::MAX,
        };
        assert!(!tier.is_valid());
    }

    // ---- CommissionStatus default ----

    #[test]
    fn commission_status_default_is_pending() {
        assert_eq!(CommissionStatus::default(), CommissionStatus::Pending);
    }

    // ---- WithdrawalMode default ----

    #[test]
    fn withdrawal_mode_default_is_level_based() {
        assert_eq!(WithdrawalMode::default(), WithdrawalMode::LevelBased);
    }

    // ---- 空实现 ----

    #[test]
    fn null_commission_plugin_returns_empty() {
        let (outputs, remaining) = <() as CommissionPlugin<u64, u128>>::calculate(
            1,
            &42,
            1000,
            500,
            CommissionModes::default(),
            false,
            0,
            0,
        );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 500);
    }

    #[test]
    fn null_token_commission_plugin_returns_empty() {
        let (outputs, remaining) = <() as TokenCommissionPlugin<u64, u128>>::calculate_token(
            1,
            &42,
            1000,
            500,
            CommissionModes::default(),
            false,
            0,
            0,
        );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 500);
    }

    #[test]
    fn null_commission_provider_noop() {
        type P = NullCommissionProvider;
        assert!(<P as CommissionProvider<u64, u128>>::process_commission(
            1, 1, 1, &42, 100, 50, 10, 1, 0
        )
        .is_ok());
        assert!(<P as CommissionProvider<u64, u128>>::cancel_commission(1).is_ok());
        assert_eq!(
            <P as CommissionProvider<u64, u128>>::pending_commission(1, &42),
            0u128
        );
        assert_eq!(
            <P as CommissionProvider<u64, u128>>::shopping_balance(1, &42),
            0u128
        );
        assert!(<P as CommissionProvider<u64, u128>>::settle_order_commission(1).is_ok());
    }

    #[test]
    fn null_entity_referrer_provider() {
        assert_eq!(
            <() as EntityReferrerProvider<u64>>::entity_referrer(1),
            None
        );
    }

    #[test]
    fn null_participation_guard_allows_all() {
        assert!(<() as ParticipationGuard<u64>>::can_participate(1, &42));
    }

    #[test]
    fn null_pool_balance_provider() {
        assert_eq!(<() as PoolBalanceProvider<u128>>::pool_balance(1), 0);
        assert!(<() as PoolBalanceProvider<u128>>::deduct_pool(1, 100).is_ok());
    }

    #[test]
    fn null_token_pool_balance_provider() {
        assert_eq!(
            <() as TokenPoolBalanceProvider<u128>>::token_pool_balance(1),
            0
        );
        assert!(<() as TokenPoolBalanceProvider<u128>>::deduct_token_pool(1, 100).is_ok());
    }

    #[test]
    fn null_token_transfer_provider() {
        assert_eq!(
            <() as TokenTransferProvider<u64, u128>>::token_balance_of(1, &42),
            0
        );
        assert!(<() as TokenTransferProvider<u64, u128>>::token_transfer(1, &42, &43, 100).is_ok());
    }

    #[test]
    fn null_pool_funding_callback() {
        <() as PoolFundingCallback>::on_pool_funded(
            1,
            FundingSource::OrderCommissionRemainder,
            100,
            0,
            42,
        );
        // should not panic
    }

    #[test]
    fn null_token_commission_provider() {
        assert!(<NullTokenCommissionProvider as TokenCommissionProvider<u64, u128>>::process_token_commission(1, 1, 1, &42, 100, 50, 10, 1).is_ok());
        assert!(<NullTokenCommissionProvider as TokenCommissionProvider<u64, u128>>::cancel_token_commission(1).is_ok());
        assert_eq!(<NullTokenCommissionProvider as TokenCommissionProvider<u64, u128>>::pending_token_commission(1, &42), 0u128);
        assert_eq!(<NullTokenCommissionProvider as TokenCommissionProvider<u64, u128>>::token_platform_fee_rate(1), 0);
    }
}
