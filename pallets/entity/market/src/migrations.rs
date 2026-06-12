//! Storage migrations for pallet-entity-market.
//! pallet-entity-market 的存储迁移。

use crate::pallet::*;
use frame_support::{pallet_prelude::*, weights::Weight};

/// v0 → v1: `TwapAccumulator` layout change.
///
/// - Adds `first_trade_block` (history coverage anchor for the
///   `initial_price` → TWAP reference-price switch).
/// - Replaces the single rolling snapshot per period with dual checkpoints
///   (`prev` / `curr`) so each TWAP window actually covers its named period.
///
/// v0 → v1：`TwapAccumulator` 结构变更。
///
/// - 新增 `first_trade_block`（参考价从 `initial_price` 切换到 TWAP 所依据的
///   历史覆盖时长起点）。
/// - 每周期的单个滚动快照改为双 checkpoint（`prev` / `curr`），使 TWAP 窗口
///   真实覆盖其命名周期。
pub mod v1 {
    use super::*;

    /// v0 on-chain layout of `TwapAccumulator` (decode-only).
    /// v0 链上 `TwapAccumulator` 布局（仅用于解码）。
    #[derive(Encode, Decode)]
    pub struct OldTwapAccumulator<Balance> {
        pub current_cumulative: u128,
        pub current_block: u32,
        pub last_price: Balance,
        pub trade_count: u64,
        pub hour_snapshot: PriceSnapshot,
        pub day_snapshot: PriceSnapshot,
        pub week_snapshot: PriceSnapshot,
        pub last_hour_update: u32,
        pub last_day_update: u32,
        pub last_week_update: u32,
    }

    pub fn migrate<T: Config>() -> Weight {
        let mut count: u64 = 0;

        TwapAccumulators::<T>::translate::<OldTwapAccumulator<BalanceOf<T>>, _>(
            |_entity_id, old| {
                count = count.saturating_add(1);

                // Best-effort estimate of the first trade block: the oldest
                // block we still know about. This underestimates the real
                // coverage (the actual first trade happened at or before the
                // accumulator's creation), which is the conservative direction.
                // `max(1)` keeps the "0 = no trades yet" sentinel meaningful.
                //
                // 首笔成交区块的尽力估计：取当前仍可知的最旧区块号。该估计只会
                // 低估真实覆盖时长（实际首笔成交不晚于累积器创建），方向保守。
                // `max(1)` 保证不与"0 = 尚无成交"哨兵值冲突。
                let first_trade_block = if old.trade_count > 0 {
                    old.hour_snapshot
                        .block_number
                        .min(old.day_snapshot.block_number)
                        .min(old.week_snapshot.block_number)
                        .min(old.current_block)
                        .max(1)
                } else {
                    0
                };

                Some(TwapAccumulator {
                    current_cumulative: old.current_cumulative,
                    current_block: old.current_block,
                    last_price: old.last_price,
                    trade_count: old.trade_count,
                    first_trade_block,
                    hour_prev: old.hour_snapshot.clone(),
                    hour_curr: old.hour_snapshot,
                    day_prev: old.day_snapshot.clone(),
                    day_curr: old.day_snapshot,
                    week_prev: old.week_snapshot.clone(),
                    week_curr: old.week_snapshot,
                })
            },
        );

        log::info!(
            target: "pallet-entity-market",
            "v1 migration: translated {} TwapAccumulator entries",
            count,
        );

        T::DbWeight::get().reads_writes(count, count)
    }
}
