//! # 频率限制工具
//!
//! 本模块提供消息发送频率限制功能，用于防止垃圾消息

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::Saturating;

/// 函数级详细中文注释：频率限制状态
///
/// 记录用户在时间窗口内的发送行为
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, MaxEncodedLen, Default, Debug)]
pub struct RateLimitState<BlockNumber: Default> {
    /// 上次发送时间（区块号）
    pub last_time: BlockNumber,
    /// 时间窗口内发送次数
    pub count: u32,
}

impl<BlockNumber: Default> RateLimitState<BlockNumber> {
    /// 函数级中文注释：创建新的频率限制状态
    pub fn new() -> Self {
        Self::default()
    }
}

/// 函数级详细中文注释：频率限制检查结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitResult {
    /// 允许操作
    Allowed,
    /// 超过频率限制
    Exceeded,
}

impl RateLimitResult {
    /// 函数级中文注释：检查是否允许
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }
}

/// 函数级详细中文注释：检查并更新频率限制
///
/// # 参数
/// - `state`: 当前状态（会被修改）
/// - `current_block`: 当前区块号
/// - `window`: 时间窗口（区块数）
/// - `max_count`: 窗口内最大允许次数
///
/// # 返回
/// - `RateLimitResult::Allowed`: 通过检查，状态已更新
/// - `RateLimitResult::Exceeded`: 超过限制，状态未更新
pub fn check_and_update_rate_limit<BlockNumber>(
    state: &mut RateLimitState<BlockNumber>,
    current_block: BlockNumber,
    window: BlockNumber,
    max_count: u32,
) -> RateLimitResult
where
    BlockNumber: Copy + Saturating + PartialOrd + Default,
{
    let elapsed = current_block.saturating_sub(state.last_time);

    if elapsed > window {
        // 超出窗口，重置计数
        state.last_time = current_block;
        state.count = 1;
        RateLimitResult::Allowed
    } else {
        // 在窗口内，检查计数
        if state.count >= max_count {
            RateLimitResult::Exceeded
        } else {
            state.count = state.count.saturating_add(1);
            RateLimitResult::Allowed
        }
    }
}

/// 函数级详细中文注释：仅检查频率限制（不更新状态）
///
/// # 参数
/// - `state`: 当前状态
/// - `current_block`: 当前区块号
/// - `window`: 时间窗口（区块数）
/// - `max_count`: 窗口内最大允许次数
///
/// # 返回
/// - `true`: 未超过限制
/// - `false`: 已超过限制
pub fn check_rate_limit<BlockNumber>(
    state: &RateLimitState<BlockNumber>,
    current_block: BlockNumber,
    window: BlockNumber,
    max_count: u32,
) -> bool
where
    BlockNumber: Copy + Saturating + PartialOrd + Default,
{
    let elapsed = current_block.saturating_sub(state.last_time);

    if elapsed > window {
        // 超出窗口，允许
        true
    } else {
        // 在窗口内，检查计数
        state.count < max_count
    }
}

/// 函数级详细中文注释：重置频率限制状态
///
/// # 参数
/// - `state`: 要重置的状态
pub fn reset_rate_limit<BlockNumber: Default>(state: &mut RateLimitState<BlockNumber>) {
    *state = RateLimitState::default();
}

/// 函数级详细中文注释：计算剩余可用次数
///
/// # 参数
/// - `state`: 当前状态
/// - `current_block`: 当前区块号
/// - `window`: 时间窗口（区块数）
/// - `max_count`: 窗口内最大允许次数
///
/// # 返回
/// - 剩余可用次数
pub fn remaining_quota<BlockNumber>(
    state: &RateLimitState<BlockNumber>,
    current_block: BlockNumber,
    window: BlockNumber,
    max_count: u32,
) -> u32
where
    BlockNumber: Copy + Saturating + PartialOrd + Default,
{
    let elapsed = current_block.saturating_sub(state.last_time);

    if elapsed > window {
        // 超出窗口，完全重置
        max_count
    } else {
        // 在窗口内
        max_count.saturating_sub(state.count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_and_update_rate_limit() {
        let mut state: RateLimitState<u32> = RateLimitState::new();
        let window = 100u32;
        let max_count = 3u32;

        // 第一次发送
        assert_eq!(
            check_and_update_rate_limit(&mut state, 10, window, max_count),
            RateLimitResult::Allowed
        );
        assert_eq!(state.count, 1);

        // 第二次发送
        assert_eq!(
            check_and_update_rate_limit(&mut state, 20, window, max_count),
            RateLimitResult::Allowed
        );
        assert_eq!(state.count, 2);

        // 第三次发送
        assert_eq!(
            check_and_update_rate_limit(&mut state, 30, window, max_count),
            RateLimitResult::Allowed
        );
        assert_eq!(state.count, 3);

        // 第四次发送（应该被拒绝）
        assert_eq!(
            check_and_update_rate_limit(&mut state, 40, window, max_count),
            RateLimitResult::Exceeded
        );
        assert_eq!(state.count, 3); // 计数不变

        // 窗口过期后
        assert_eq!(
            check_and_update_rate_limit(&mut state, 150, window, max_count),
            RateLimitResult::Allowed
        );
        assert_eq!(state.count, 1); // 重置
    }

    #[test]
    fn test_remaining_quota() {
        let state: RateLimitState<u32> = RateLimitState {
            last_time: 10,
            count: 2,
        };

        // 在窗口内
        assert_eq!(remaining_quota(&state, 50, 100, 5), 3);

        // 窗口过期
        assert_eq!(remaining_quota(&state, 200, 100, 5), 5);
    }
}
