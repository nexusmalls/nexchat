//! Compliance and governance query traits
//!
//! DisclosureProvider (+ Read/Write split), KycProvider, GovernanceProvider.

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

use super::super::types::*;

// ============================================================================
// 披露接口
// ============================================================================

/// 披露级别（跨模块共享）
///
/// 由 pallet-entity-disclosure 设置，供 token/market 等模块查询
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
    PartialOrd,
    Ord,
)]
pub enum DisclosureLevel {
    /// 基础披露（年度简报）
    #[default]
    Basic,
    /// 标准披露（季度报告）
    Standard,
    /// 增强披露（月度报告 + 重大事件）
    Enhanced,
    /// 完全披露（实时 + 详细财务）
    Full,
}

/// 披露查询接口
///
/// 供 token/market 等模块在交易前检查黑窗口期和内幕人员限制，
/// 无需直接依赖 pallet-entity-disclosure。
pub trait DisclosureProvider<AccountId> {
    /// 检查实体是否处于黑窗口期
    fn is_in_blackout(entity_id: u64) -> bool;

    /// 检查账户是否是内幕人员
    fn is_insider(entity_id: u64, account: &AccountId) -> bool;

    /// 检查内幕人员是否可以交易
    ///
    /// 非内幕人员始终返回 true；内幕人员在黑窗口期内且启用控制时返回 false
    fn can_insider_trade(entity_id: u64, account: &AccountId) -> bool;

    /// 获取实体的披露级别
    fn get_disclosure_level(entity_id: u64) -> DisclosureLevel;

    /// 检查披露是否逾期
    fn is_disclosure_overdue(entity_id: u64) -> bool;

    /// F7: 获取实体违规次数
    fn get_violation_count(_entity_id: u64) -> u32 {
        0
    }

    /// F7: 获取内幕人员角色（返回 InsiderRole 的 u8 表示）
    ///
    /// 0=Owner, 1=Admin, 2=Auditor, 3=Advisor, 4=MajorHolder
    fn get_insider_role(_entity_id: u64, _account: &AccountId) -> Option<u8> {
        None
    }

    /// F7: 检查实体是否已配置披露
    fn is_disclosure_configured(_entity_id: u64) -> bool {
        false
    }

    /// F6/F7: 检查实体是否被标记为高风险（违规超阈值）
    fn is_high_risk(_entity_id: u64) -> bool {
        false
    }

    // ==================== F10: 治理写入接口 ====================

    /// F10: 治理提案配置披露级别
    fn governance_configure_disclosure(
        _entity_id: u64,
        _level: DisclosureLevel,
        _insider_trading_control: bool,
        _blackout_period_after: u64,
    ) -> sp_runtime::DispatchResult {
        Err(sp_runtime::DispatchError::Other("not implemented"))
    }

    /// F10: 治理提案重置违规记录
    fn governance_reset_violations(_entity_id: u64) -> sp_runtime::DispatchResult {
        Err(sp_runtime::DispatchError::Other("not implemented"))
    }

    // ==================== v0.6: 大股东自动注册 ====================

    /// 将账户注册为大股东内幕人员（供 token 模块在持仓超过阈值时调用）
    fn register_major_holder(_entity_id: u64, _account: &AccountId) -> sp_runtime::DispatchResult {
        Ok(())
    }

    /// 注销大股东内幕人员身份（供 token 模块在持仓低于阈值时调用）
    fn deregister_major_holder(
        _entity_id: u64,
        _account: &AccountId,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }

    // ==================== v0.6: 渐进式处罚 ====================

    /// 获取实体当前处罚级别 (0=None, 1=Warning, 2=Restricted, 3=Suspended, 4=Delisted)
    fn get_penalty_level(_entity_id: u64) -> u8 {
        0
    }

    /// 检查实体是否受到活跃处罚（Restricted 及以上）
    fn is_penalty_active(_entity_id: u64) -> bool {
        false
    }

    // ==================== R10: 治理提案链上执行接口 ====================

    /// 设置处罚级别（治理提案执行）
    fn governance_set_penalty_level(_entity_id: u64, _level: u8) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

/// 空披露提供者（测试用或未启用披露时）
pub struct NullDisclosureProvider;

impl<AccountId> DisclosureProvider<AccountId> for NullDisclosureProvider {
    fn is_in_blackout(_entity_id: u64) -> bool {
        false
    }
    fn is_insider(_entity_id: u64, _account: &AccountId) -> bool {
        false
    }
    fn can_insider_trade(_entity_id: u64, _account: &AccountId) -> bool {
        true
    }
    fn get_disclosure_level(_entity_id: u64) -> DisclosureLevel {
        DisclosureLevel::Basic
    }
    fn is_disclosure_overdue(_entity_id: u64) -> bool {
        false
    }
    fn get_violation_count(_entity_id: u64) -> u32 {
        0
    }
    fn get_insider_role(_entity_id: u64, _account: &AccountId) -> Option<u8> {
        None
    }
    fn is_disclosure_configured(_entity_id: u64) -> bool {
        false
    }
    fn is_high_risk(_entity_id: u64) -> bool {
        false
    }
    fn get_penalty_level(_entity_id: u64) -> u8 {
        0
    }
    fn is_penalty_active(_entity_id: u64) -> bool {
        false
    }
}

// ============================================================================
// 披露接口职责拆分（DisclosureProvider 的精简替代）
// ============================================================================

/// 披露只读查询接口（DisclosureProvider 读取子集）
///
/// 新模块应优先使用此 trait，仅关注只读查询，无需 mock 写入方法。
pub trait DisclosureReadProvider<AccountId> {
    fn is_in_blackout(entity_id: u64) -> bool;
    fn is_insider(entity_id: u64, account: &AccountId) -> bool;
    fn can_insider_trade(entity_id: u64, account: &AccountId) -> bool;
    fn get_disclosure_level(entity_id: u64) -> DisclosureLevel;
    fn is_disclosure_overdue(entity_id: u64) -> bool;
    fn get_violation_count(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }
    fn get_insider_role(entity_id: u64, account: &AccountId) -> Option<u8> {
        let _ = (entity_id, account);
        None
    }
    fn is_disclosure_configured(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
    fn is_high_risk(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
    fn get_penalty_level(entity_id: u64) -> u8 {
        let _ = entity_id;
        0
    }
    fn is_penalty_active(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
}

/// 披露治理写入接口（DisclosureProvider 写入子集）
///
/// 仅供 governance 模块使用，其他模块无需依赖写入方法。
pub trait DisclosureWriteProvider<AccountId> {
    fn governance_configure_disclosure(
        entity_id: u64,
        level: DisclosureLevel,
        insider_trading_control: bool,
        blackout_period_after: u64,
    ) -> sp_runtime::DispatchResult;
    fn governance_reset_violations(entity_id: u64) -> sp_runtime::DispatchResult;
    fn register_major_holder(entity_id: u64, account: &AccountId) -> sp_runtime::DispatchResult;
    fn deregister_major_holder(entity_id: u64, account: &AccountId) -> sp_runtime::DispatchResult;
    fn governance_set_penalty_level(entity_id: u64, level: u8) -> sp_runtime::DispatchResult;
}

/// 空只读披露提供者 — `NullDisclosureProvider` 的类型别名
///
/// 通过 blanket impl，`NullDisclosureProvider` 自动实现 `DisclosureReadProvider`
/// 和 `DisclosureWriteProvider`，无需单独定义独立类型。
pub type NullDisclosureReadProvider = NullDisclosureProvider;

/// 空写入披露提供者 — `NullDisclosureProvider` 的类型别名（无操作模式）
///
/// 写入方法来自 `DisclosureProvider` 的默认实现：
/// - `register_major_holder` / `deregister_major_holder` → `Ok(())`
/// - `governance_configure_disclosure` / `governance_reset_violations` → `Err("not implemented")`
///   （表示未完成 override，而非功能关闭）
pub type NullDisclosureWriteProvider = NullDisclosureProvider;

// ---- 桥接: DisclosureProvider 自动实现 DisclosureReadProvider / WriteProvider ----

impl<AccountId, T: DisclosureProvider<AccountId>> DisclosureReadProvider<AccountId> for T {
    fn is_in_blackout(entity_id: u64) -> bool {
        <T as DisclosureProvider<AccountId>>::is_in_blackout(entity_id)
    }
    fn is_insider(entity_id: u64, account: &AccountId) -> bool {
        <T as DisclosureProvider<AccountId>>::is_insider(entity_id, account)
    }
    fn can_insider_trade(entity_id: u64, account: &AccountId) -> bool {
        <T as DisclosureProvider<AccountId>>::can_insider_trade(entity_id, account)
    }
    fn get_disclosure_level(entity_id: u64) -> DisclosureLevel {
        <T as DisclosureProvider<AccountId>>::get_disclosure_level(entity_id)
    }
    fn is_disclosure_overdue(entity_id: u64) -> bool {
        <T as DisclosureProvider<AccountId>>::is_disclosure_overdue(entity_id)
    }
    fn get_violation_count(entity_id: u64) -> u32 {
        <T as DisclosureProvider<AccountId>>::get_violation_count(entity_id)
    }
    fn get_insider_role(entity_id: u64, account: &AccountId) -> Option<u8> {
        <T as DisclosureProvider<AccountId>>::get_insider_role(entity_id, account)
    }
    fn is_disclosure_configured(entity_id: u64) -> bool {
        <T as DisclosureProvider<AccountId>>::is_disclosure_configured(entity_id)
    }
    fn is_high_risk(entity_id: u64) -> bool {
        <T as DisclosureProvider<AccountId>>::is_high_risk(entity_id)
    }
    fn get_penalty_level(entity_id: u64) -> u8 {
        <T as DisclosureProvider<AccountId>>::get_penalty_level(entity_id)
    }
    fn is_penalty_active(entity_id: u64) -> bool {
        <T as DisclosureProvider<AccountId>>::is_penalty_active(entity_id)
    }
}

impl<AccountId, T: DisclosureProvider<AccountId>> DisclosureWriteProvider<AccountId> for T {
    fn governance_configure_disclosure(
        entity_id: u64,
        level: DisclosureLevel,
        insider_trading_control: bool,
        blackout_period_after: u64,
    ) -> sp_runtime::DispatchResult {
        <T as DisclosureProvider<AccountId>>::governance_configure_disclosure(
            entity_id,
            level,
            insider_trading_control,
            blackout_period_after,
        )
    }
    fn governance_reset_violations(entity_id: u64) -> sp_runtime::DispatchResult {
        <T as DisclosureProvider<AccountId>>::governance_reset_violations(entity_id)
    }
    fn register_major_holder(entity_id: u64, account: &AccountId) -> sp_runtime::DispatchResult {
        <T as DisclosureProvider<AccountId>>::register_major_holder(entity_id, account)
    }
    fn deregister_major_holder(entity_id: u64, account: &AccountId) -> sp_runtime::DispatchResult {
        <T as DisclosureProvider<AccountId>>::deregister_major_holder(entity_id, account)
    }
    fn governance_set_penalty_level(entity_id: u64, level: u8) -> sp_runtime::DispatchResult {
        <T as DisclosureProvider<AccountId>>::governance_set_penalty_level(entity_id, level)
    }
}

// ============================================================================
// KYC 查询接口
// ============================================================================

/// KYC 查询接口
///
/// 供其他模块查询用户 KYC 状态，无需直接依赖 pallet-entity-kyc。
pub trait KycProvider<AccountId> {
    /// 获取用户在指定实体下的 KYC 级别（0 = 未认证）
    fn kyc_level(entity_id: u64, account: &AccountId) -> u8;

    /// 用户是否已通过 KYC 认证（level >= 1）
    fn is_kyc_approved(entity_id: u64, account: &AccountId) -> bool {
        Self::kyc_level(entity_id, account) >= 1
    }

    /// 用户是否满足指定 KYC 级别要求
    fn meets_kyc_requirement(entity_id: u64, account: &AccountId, required_level: u8) -> bool {
        Self::kyc_level(entity_id, account) >= required_level
    }

    // ==================== #9 补充: 过期与参与检查 ====================

    /// KYC 认证是否已过期
    fn is_kyc_expired(entity_id: u64, account: &AccountId) -> bool {
        let _ = (entity_id, account);
        false
    }

    /// 用户是否可以参与实体活动（综合 KYC 状态 + 宽限期 + 封禁状态）
    fn can_participate(entity_id: u64, account: &AccountId) -> bool {
        Self::is_kyc_approved(entity_id, account)
    }

    /// 获取 KYC 过期时间（区块号，0 = 永不过期或无记录）
    fn kyc_expires_at(entity_id: u64, account: &AccountId) -> u64 {
        let _ = (entity_id, account);
        0
    }
}

/// 空 KYC 提供者（测试用或未启用 KYC 时）
pub struct NullKycProvider;

impl<AccountId> KycProvider<AccountId> for NullKycProvider {
    fn kyc_level(_entity_id: u64, _account: &AccountId) -> u8 {
        0
    }
}

// ============================================================================
// 治理查询接口
// ============================================================================

/// 治理查询接口
///
/// 供其他模块查询实体治理状态，无需直接依赖 pallet-entity-governance。
pub trait GovernanceProvider {
    /// 获取实体治理模式
    fn governance_mode(entity_id: u64) -> GovernanceMode;

    /// 实体是否有活跃提案
    fn has_active_proposals(entity_id: u64) -> bool;

    /// 实体治理是否被锁定（例如重大变更期间）
    fn is_governance_locked(entity_id: u64) -> bool;

    // ==================== #10 补充: 治理查询扩展 ====================

    /// 获取活跃提案数量
    fn active_proposal_count(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }

    /// 检查实体治理是否已初始化
    fn is_governance_initialized(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }

    /// 获取实体治理配置中的执行延迟（区块数）
    fn execution_delay(entity_id: u64) -> u32 {
        let _ = entity_id;
        0
    }

    /// 获取通过阈值（百分比 0-100）
    fn pass_threshold(entity_id: u64) -> u8 {
        let _ = entity_id;
        0
    }

    /// 实体治理是否被暂停
    fn is_governance_paused(entity_id: u64) -> bool {
        let _ = entity_id;
        false
    }
}

/// 空治理提供者（测试用或未启用治理时）
pub struct NullGovernanceProvider;

impl GovernanceProvider for NullGovernanceProvider {
    fn governance_mode(_entity_id: u64) -> GovernanceMode {
        GovernanceMode::None
    }
    fn has_active_proposals(_entity_id: u64) -> bool {
        false
    }
    fn is_governance_locked(_entity_id: u64) -> bool {
        false
    }
}
