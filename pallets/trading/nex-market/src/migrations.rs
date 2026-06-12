//! Storage migrations for pallet-nex-market.
//! pallet-nex-market 的存储迁移。

use crate::pallet::*;
use frame_support::{pallet_prelude::*, weights::Weight};

/// v2 → v3: `TwapAccumulator` layout change.
///
/// - Adds `first_trade_block` (history coverage anchor for the
///   `initial_price` → TWAP reference-price switch).
/// - Replaces the single rolling snapshot per period with dual checkpoints
///   (`prev` / `curr`) so each TWAP window actually covers its named period.
///
/// v2 → v3：`TwapAccumulator` 结构变更。
///
/// - 新增 `first_trade_block`（参考价从 `initial_price` 切换到 TWAP 所依据的
///   历史覆盖时长起点）。
/// - 每周期的单个滚动快照改为双 checkpoint（`prev` / `curr`），使 TWAP 窗口
///   真实覆盖其命名周期。
pub mod v3 {
    use super::*;

    /// v2 on-chain layout of `TwapAccumulator` (decode-only).
    /// v2 链上 `TwapAccumulator` 布局（仅用于解码）。
    #[derive(Encode, Decode)]
    pub struct OldTwapAccumulator {
        pub current_cumulative: u128,
        pub current_block: u32,
        pub last_price: u64,
        pub trade_count: u64,
        pub hour_snapshot: PriceSnapshot,
        pub day_snapshot: PriceSnapshot,
        pub week_snapshot: PriceSnapshot,
        pub last_hour_update: u32,
        pub last_day_update: u32,
        pub last_week_update: u32,
    }

    pub fn migrate<T: Config>() -> Weight {
        let result = TwapAccumulatorStore::<T>::translate::<OldTwapAccumulator, _>(|maybe_old| {
            let old = maybe_old?;

            // Best-effort estimate of the first trade block: the oldest block
            // we still know about. This underestimates the real coverage,
            // which is the conservative direction. `max(1)` keeps the
            // "0 = no trades yet" sentinel meaningful. Note: genesis /
            // set_initial_price seed trade_count = 1 without a real trade, so
            // a freshly seeded market may switch to TWAP one hour after its
            // estimated anchor — acceptable since the accumulator then already
            // tracks a meaningful price.
            //
            // 首笔成交区块的尽力估计：取当前仍可知的最旧区块号，只会低估真实
            // 覆盖时长（方向保守）。`max(1)` 保证不与"0 = 尚无成交"哨兵值
            // 冲突。注意：genesis / set_initial_price 会预置 trade_count = 1
            // 而无真实成交，因此刚预置的市场可能在估计锚点 1 小时后即切换到
            // TWAP——此时累积器已跟踪有效价格，可接受。
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
        });

        match result {
            Ok(_) => log::info!(
                target: "pallet-nex-market",
                "v3 migration: TwapAccumulatorStore translated",
            ),
            Err(_) => {
                // Undecodable leftover would be unreadable garbage under the
                // new layout — remove it; the accumulator re-initializes on
                // the next trade.
                // 无法解码的旧值在新布局下是不可读的脏数据——直接移除，
                // 下一笔成交会重新初始化累积器。
                TwapAccumulatorStore::<T>::kill();
                log::warn!(
                    target: "pallet-nex-market",
                    "v3 migration: TwapAccumulatorStore failed to decode as v2 layout, value removed",
                );
            }
        }

        T::DbWeight::get().reads_writes(1, 1)
    }
}
