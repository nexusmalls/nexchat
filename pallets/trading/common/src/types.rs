//! # 公共类型定义
//!
//! 本模块定义 Trading 相关的公共类型，供多个 pallet 共享。
//!
//! ## 版本历史
//! - v0.1.0 (2026-01-18): 初始版本，从 OTC/Swap/Maker 模块提取

use frame_support::{pallet_prelude::ConstU32, BoundedVec};
use core::fmt::Debug;

/// 函数级详细中文注释：TRON 地址类型（固定 34 字节）
///
/// ## 说明
/// - TRC20 地址以 'T' 开头，长度固定为 34 字符
/// - 用于 OTC 订单收款地址和 Swap 兑换地址
///
/// ## 使用者
/// - `pallet-nex-market`: 做市商收款地址 / 用户 USDT 接收地址
pub type TronAddress = BoundedVec<u8, ConstU32<34>>;

/// 函数级详细中文注释：时间戳类型（Unix 秒）
///
/// ## 说明
/// - 用于 OTC 订单的时间字段
/// - 精度为秒（非毫秒）
pub type MomentOf = u64;

/// 函数级详细中文注释：IPFS CID 类型（最大 64 字节）
///
/// ## 说明
/// - 用于存储 IPFS 内容标识符
/// - 如做市商的公开/私密资料
pub type Cid = BoundedVec<u8, ConstU32<64>>;

/// 函数级详细中文注释：交易哈希类型（最大 128 字节）
///
/// ## 说明
/// - 用于存储 TRON TRC20 交易哈希
/// - Swap 模块中使用
pub type TxHash = BoundedVec<u8, ConstU32<128>>;

/// TRON 交易哈希类型（64 字节 hex）
pub type TronTxHash = BoundedVec<u8, ConstU32<64>>;

// ==================== USDT 交易共享类型 ====================

/// USDT 交易状态（entity-market / nex-market 共享）
#[derive(
    codec::Encode,
    codec::Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    scale_info::TypeInfo,
    frame_support::pallet_prelude::MaxEncodedLen,
    Debug,
)]
pub enum UsdtTradeStatus {
    /// 等待买家支付 USDT
    AwaitingPayment, // 0
    /// 等待 OCW 验证
    AwaitingVerification, // 1
    /// 少付等待补付（补付窗口内）
    UnderpaidPending, // 2 — 保持 nex-market 现有编码
    /// 已完成
    Completed, // 3
    /// 已退款（超时）
    Refunded, // 4
    /// 争议中（新增，尾部追加不影响现有编码）
    Disputed, // 5
    /// 已取消（新增，尾部追加不影响现有编码）
    Cancelled, // 6
}

/// 买家保证金状态（entity-market / nex-market 共享）
#[derive(
    codec::Encode,
    codec::Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    scale_info::TypeInfo,
    frame_support::pallet_prelude::MaxEncodedLen,
    Debug,
    Default,
)]
pub enum BuyerDepositStatus {
    /// 无保证金
    #[default]
    None,
    /// 已锁定
    Locked,
    /// 已退还（交易完成）
    Released,
    /// 已没收（超时/违约）
    Forfeited,
    /// 部分没收（少付场景）
    PartiallyForfeited,
}

/// 付款金额验证结果（多档判定，entity-market / nex-market 共享）
///
/// | 实际金额        | 结果              |
/// |-----------------|-------------------|
/// | ≥ 100.5%        | Overpaid          |
/// | 99.5% ~ 100.5%  | Exact             |
/// | 50% ~ 99.5%     | Underpaid         |
/// | < 50%           | SeverelyUnderpaid |
/// | = 0             | Invalid           |
#[derive(
    codec::Encode,
    codec::Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    scale_info::TypeInfo,
    frame_support::pallet_prelude::MaxEncodedLen,
    Debug,
)]
pub enum PaymentVerificationResult {
    /// 验证通过（≥99.5%）
    Exact,
    /// 多付（≥100.5%）
    Overpaid,
    /// 少付（50%-99.5%）→ 按比例处理
    Underpaid,
    /// 严重少付（<50%）
    SeverelyUnderpaid,
    /// 无效（0 或交易失败）
    Invalid,
}

/// 计算付款金额验证结果（多档判定）
///
/// 由 entity-market 和 nex-market 共享，避免重复实现。
pub fn calculate_payment_verification_result(
    expected_amount: u64,
    actual_amount: u64,
) -> PaymentVerificationResult {
    if actual_amount == 0 {
        return PaymentVerificationResult::Invalid;
    }
    if expected_amount == 0 {
        return PaymentVerificationResult::Invalid;
    }
    let ratio = compute_payment_ratio_bps(expected_amount, actual_amount);
    match ratio {
        r if r >= 10050 => PaymentVerificationResult::Overpaid,
        r if r >= 9950 => PaymentVerificationResult::Exact,
        r if r >= 5000 => PaymentVerificationResult::Underpaid,
        _ => PaymentVerificationResult::SeverelyUnderpaid,
    }
}

/// 计算付款比例（basis points, 10000 = 100%）
///
/// 返回 u32 避免 u16 截断（付款超 6.55 倍时 u16 会回绕）。
/// 下游 pallet 应统一使用此函数，不要自行 `as u16`。
pub fn compute_payment_ratio_bps(expected_amount: u64, actual_amount: u64) -> u32 {
    if expected_amount == 0 {
        return 0;
    }
    let ratio_u128 = (actual_amount as u128)
        .saturating_mul(10000)
        .saturating_div(expected_amount as u128);
    // u32::MAX = 4_294_967_295，足以表示 ~429496 倍付款
    ratio_u128.min(u32::MAX as u128) as u32
}

/// 保证金没收梯度计算（entity-market / nex-market 共享）
///
/// | 付款比例     | 没收比例 |
/// |-------------|---------|
/// | ≥ 99.5%     | 0%      |
/// | 95% - 99.5% | 20%     |
/// | 80% - 95%   | 50%     |
/// | < 80%       | 100%    |
pub fn calculate_deposit_forfeit_rate(payment_ratio: u32) -> u16 {
    match payment_ratio {
        r if r >= 9950 => 0,
        r if r >= 9500 => 2000,
        r if r >= 8000 => 5000,
        _ => 10000,
    }
}

// ===== 单元测试 =====

#[cfg(test)]
mod tests {
    use super::*;

    // ---- H1 回归测试: compute_payment_ratio_bps ----

    #[test]
    fn h1_ratio_normal_exact_payment() {
        // 100% 付款 → 10000 bps
        assert_eq!(compute_payment_ratio_bps(1_000_000, 1_000_000), 10000);
    }

    #[test]
    fn h1_ratio_50_percent() {
        assert_eq!(compute_payment_ratio_bps(1_000_000, 500_000), 5000);
    }

    #[test]
    fn h1_ratio_overpaid_7x_no_truncation() {
        // 修复前: 7x = 70000 bps, as u16 → 70000 % 65536 = 4464 → SeverelyUnderpaid
        // 修复后: 70000 (u32) → Overpaid
        let ratio = compute_payment_ratio_bps(1_000_000, 7_000_000);
        assert_eq!(ratio, 70000);
        assert!(ratio > 10050); // Overpaid 阈值
    }

    #[test]
    fn h1_ratio_overpaid_100x_no_truncation() {
        // 100x 付款 → 1_000_000 bps
        let ratio = compute_payment_ratio_bps(1_000_000, 100_000_000);
        assert_eq!(ratio, 1_000_000);
    }

    #[test]
    fn h1_ratio_expected_zero_returns_zero() {
        assert_eq!(compute_payment_ratio_bps(0, 1_000_000), 0);
    }

    #[test]
    fn h1_ratio_both_zero() {
        assert_eq!(compute_payment_ratio_bps(0, 0), 0);
    }

    #[test]
    fn h1_ratio_max_u64_no_overflow() {
        // u64::MAX * 10000 会溢出 u128? 不会: u64::MAX * 10000 < u128::MAX
        let ratio = compute_payment_ratio_bps(1, u64::MAX);
        assert!(ratio == u32::MAX); // 被 min(u32::MAX) 限制
    }

    // ---- H1 回归测试: calculate_payment_verification_result ----

    #[test]
    fn h1_verification_exact_payment() {
        assert_eq!(
            calculate_payment_verification_result(1_000_000, 1_000_000),
            PaymentVerificationResult::Exact,
        );
    }

    #[test]
    fn h1_verification_overpaid_7x_not_severely_underpaid() {
        // 核心回归: 修复前会返回 SeverelyUnderpaid
        assert_eq!(
            calculate_payment_verification_result(1_000_000, 7_000_000),
            PaymentVerificationResult::Overpaid,
        );
    }

    #[test]
    fn h1_verification_overpaid_boundary() {
        // 100.5% → Overpaid
        assert_eq!(
            calculate_payment_verification_result(10000, 10050),
            PaymentVerificationResult::Overpaid,
        );
        // 100.49% → Exact
        assert_eq!(
            calculate_payment_verification_result(10000, 10049),
            PaymentVerificationResult::Exact,
        );
    }

    #[test]
    fn h1_verification_underpaid_boundary() {
        // 99.5% → Exact
        assert_eq!(
            calculate_payment_verification_result(10000, 9950),
            PaymentVerificationResult::Exact,
        );
        // 99.49% → Underpaid
        assert_eq!(
            calculate_payment_verification_result(10000, 9949),
            PaymentVerificationResult::Underpaid,
        );
    }

    #[test]
    fn h1_verification_severely_underpaid_boundary() {
        // 50% → Underpaid
        assert_eq!(
            calculate_payment_verification_result(10000, 5000),
            PaymentVerificationResult::Underpaid,
        );
        // 49.99% → SeverelyUnderpaid
        assert_eq!(
            calculate_payment_verification_result(10000, 4999),
            PaymentVerificationResult::SeverelyUnderpaid,
        );
    }

    // ---- M2 回归测试: expected_amount == 0 ----

    #[test]
    fn m2_expected_zero_returns_invalid() {
        // 修复前返回 Overpaid，修复后返回 Invalid
        assert_eq!(
            calculate_payment_verification_result(0, 1_000_000),
            PaymentVerificationResult::Invalid,
        );
    }

    #[test]
    fn m2_both_zero_returns_invalid() {
        assert_eq!(
            calculate_payment_verification_result(0, 0),
            PaymentVerificationResult::Invalid,
        );
    }

    #[test]
    fn m2_actual_zero_returns_invalid() {
        assert_eq!(
            calculate_payment_verification_result(1_000_000, 0),
            PaymentVerificationResult::Invalid,
        );
    }

    // ---- forfeit_rate 回归测试 ----

    #[test]
    fn forfeit_rate_exact_no_forfeit() {
        assert_eq!(calculate_deposit_forfeit_rate(10000), 0);
        assert_eq!(calculate_deposit_forfeit_rate(9950), 0);
    }

    #[test]
    fn forfeit_rate_slight_underpaid() {
        assert_eq!(calculate_deposit_forfeit_rate(9949), 2000);
        assert_eq!(calculate_deposit_forfeit_rate(9500), 2000);
    }

    #[test]
    fn forfeit_rate_moderate_underpaid() {
        assert_eq!(calculate_deposit_forfeit_rate(9499), 5000);
        assert_eq!(calculate_deposit_forfeit_rate(8000), 5000);
    }

    #[test]
    fn forfeit_rate_severe_underpaid() {
        assert_eq!(calculate_deposit_forfeit_rate(7999), 10000);
        assert_eq!(calculate_deposit_forfeit_rate(0), 10000);
    }

    #[test]
    fn forfeit_rate_accepts_u32_large_values() {
        // 超付 70000 bps (7x) → 不没收
        assert_eq!(calculate_deposit_forfeit_rate(70000), 0);
        // u32::MAX → 不没收
        assert_eq!(calculate_deposit_forfeit_rate(u32::MAX), 0);
    }
}
