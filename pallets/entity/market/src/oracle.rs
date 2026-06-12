//! TWAP 价格预言机：累积器更新、TWAP 计算、价格偏离检查、熔断机制

use crate::pallet::*;
use frame_support::pallet_prelude::*;
use sp_runtime::traits::{Saturating, Zero};
use sp_runtime::SaturatedConversion;

impl<T: Config> Pallet<T> {
    // ==================== Phase 5: TWAP 价格预言机内部函数 ====================

    /// Update the TWAP accumulator (called on every trade).
    /// P1 security fix: abnormal price filtering to resist manipulation.
    ///
    /// 更新 TWAP 累积器（每次成交时调用）。
    /// P1 安全修复: 添加异常价格过滤，防止价格操纵。
    fn update_twap_accumulator(entity_id: u64, trade_price: BalanceOf<T>) {
        let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();

        TwapAccumulators::<T>::mutate(entity_id, |maybe_acc| {
            let acc = maybe_acc.get_or_insert_with(|| {
                let genesis_snapshot = PriceSnapshot {
                    cumulative_price: 0,
                    block_number: current_block,
                };
                TwapAccumulator {
                    current_cumulative: 0,
                    current_block,
                    last_price: trade_price,
                    trade_count: 0,
                    first_trade_block: 0,
                    hour_prev: genesis_snapshot.clone(),
                    hour_curr: genesis_snapshot.clone(),
                    day_prev: genesis_snapshot.clone(),
                    day_curr: genesis_snapshot.clone(),
                    week_prev: genesis_snapshot.clone(),
                    week_curr: genesis_snapshot,
                }
            });

            // P1: 异常价格过滤 - 如果价格偏离上次价格超过 100%，使用加权平均
            let filtered_price = if acc.trade_count > 0 && !acc.last_price.is_zero() {
                let last_price_u128: u128 = acc.last_price.into();
                let trade_price_u128: u128 = trade_price.into();
                let max_deviation = last_price_u128; // 100% 偏离

                let deviation = if trade_price_u128 > last_price_u128 {
                    trade_price_u128.saturating_sub(last_price_u128)
                } else {
                    last_price_u128.saturating_sub(trade_price_u128)
                };

                if deviation > max_deviation {
                    // 异常价格: 限制价格变动幅度为上次价格的 50%
                    // 如果新价格过高，使用 last_price * 1.5
                    // 如果新价格过低，使用 last_price * 0.5
                    if trade_price_u128 > last_price_u128 {
                        // 价格上涨过快，限制为 +50%
                        acc.last_price.saturating_mul(3u32.into()) / 2u32.into()
                    } else {
                        // 价格下跌过快，限制为 -50%
                        acc.last_price / 2u32.into()
                    }
                } else {
                    trade_price
                }
            } else {
                trade_price
            };

            // 计算自上次更新以来经过的区块数
            let blocks_elapsed = current_block.saturating_sub(acc.current_block);

            // 更新累积价格: cumulative += last_price × blocks_elapsed
            if blocks_elapsed > 0 {
                let price_u128: u128 = acc.last_price.into();
                acc.current_cumulative = acc
                    .current_cumulative
                    .saturating_add(price_u128.saturating_mul(blocks_elapsed as u128));
            }

            // 更新当前状态（使用过滤后的价格）
            acc.current_block = current_block;
            acc.last_price = filtered_price;
            acc.trade_count = acc.trade_count.saturating_add(1);

            // Record the block of the first real trade (history coverage
            // anchor). `max(1)` keeps the "0 = no trades yet" sentinel
            // meaningful even for block-0 trades (test environments).
            // 记录首笔真实成交的区块（价格历史覆盖时长的起点）。`max(1)`
            // 保证区块 0 的成交（测试环境）不与"0 = 尚无成交"哨兵值冲突。
            if acc.first_trade_block == 0 {
                acc.first_trade_block = current_block.max(1);
            }

            // Rotate per-period checkpoints: once `curr` is at least one full
            // period old, it becomes `prev` and a fresh `curr` is taken. This
            // keeps `prev` between 1x and 2x the period old in active markets,
            // so each TWAP window actually covers its named period.
            //
            // 轮换各周期 checkpoint：当 `curr` 距今超过一个完整周期时，将其
            // 转为 `prev` 并取新的 `curr`。活跃市场中 `prev` 的年龄始终在
            // 1~2 个周期之间，确保 TWAP 窗口真实覆盖其命名周期。
            let new_snapshot = PriceSnapshot {
                cumulative_price: acc.current_cumulative,
                block_number: current_block,
            };
            let rotations: [(&mut PriceSnapshot, &mut PriceSnapshot, u32); 3] = [
                (&mut acc.hour_prev, &mut acc.hour_curr, T::BlocksPerHour::get()),
                (&mut acc.day_prev, &mut acc.day_curr, T::BlocksPerDay::get()),
                (&mut acc.week_prev, &mut acc.week_curr, T::BlocksPerWeek::get()),
            ];
            for (prev, curr, period) in rotations {
                if current_block.saturating_sub(curr.block_number) >= period {
                    *prev = curr.clone();
                    *curr = new_snapshot.clone();
                }
            }
        });
    }

    /// Calculate the TWAP for the given period.
    ///
    /// Picks the checkpoint closest to (but at least) one period old when one
    /// exists; otherwise falls back to the oldest available checkpoint (early
    /// market life, best effort with a shorter window).
    ///
    /// 计算指定周期的 TWAP。
    ///
    /// 优先选取"距今至少一个周期、且最接近一个周期"的 checkpoint；若两个
    /// checkpoint 都不足一个周期（市场早期），退化为使用最旧的 checkpoint
    /// （窗口较短的尽力估计）。
    pub fn calculate_twap(entity_id: u64, period: TwapPeriod) -> Option<BalanceOf<T>> {
        let acc = TwapAccumulators::<T>::get(entity_id)?;
        let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();

        let (prev, curr, period_blocks) = match period {
            TwapPeriod::OneHour => (&acc.hour_prev, &acc.hour_curr, T::BlocksPerHour::get()),
            TwapPeriod::OneDay => (&acc.day_prev, &acc.day_curr, T::BlocksPerDay::get()),
            TwapPeriod::OneWeek => (&acc.week_prev, &acc.week_curr, T::BlocksPerWeek::get()),
        };

        // `curr` is always newer than or equal to `prev`. If even `curr` is a
        // full period old (sparse trading), it gives the window closest to the
        // named period; otherwise use `prev` (1x~2x period in active markets).
        // `curr` 始终不早于 `prev`。若 `curr` 已满一个周期（成交稀疏），它的
        // 窗口最接近命名周期；否则使用 `prev`（活跃市场中为 1~2 个周期）。
        let snapshot = if current_block.saturating_sub(curr.block_number) >= period_blocks {
            curr
        } else {
            prev
        };

        // 计算当前累积价格（包含自上次更新以来的部分）
        let blocks_since_update = current_block.saturating_sub(acc.current_block);
        let price_u128: u128 = acc.last_price.into();
        let current_cumulative = acc
            .current_cumulative
            .saturating_add(price_u128.saturating_mul(blocks_since_update as u128));

        // 计算区块差
        let block_diff = current_block.saturating_sub(snapshot.block_number);
        if block_diff == 0 {
            return Some(acc.last_price);
        }

        // 计算累积价格差
        let cumulative_diff = current_cumulative.saturating_sub(snapshot.cumulative_price);

        // TWAP = 累积价格差 / 区块差
        let twap_u128 = cumulative_diff / (block_diff as u128);

        Some(twap_u128.into())
    }

    /// Check whether a price deviates too far from the reference price.
    ///
    /// Reference price priority:
    /// 1. If TWAP data is sufficient (see `is_twap_data_sufficient`), use the 1h TWAP.
    /// 2. Otherwise fall back to the owner-set `initial_price`.
    /// 3. If neither exists, skip the check.
    ///
    /// 检查价格是否偏离参考价格过大。
    ///
    /// 参考价格优先级：
    /// 1. TWAP 数据充足（见 `is_twap_data_sufficient`）时使用 1小时 TWAP
    /// 2. 否则回退到实体所有者设定的初始价格 `initial_price`
    /// 3. 都没有则跳过检查
    pub fn check_price_deviation(entity_id: u64, price: BalanceOf<T>) -> Result<(), Error<T>> {
        // 获取价格保护配置
        let config = PriceProtection::<T>::get(entity_id).unwrap_or_default();

        // 如果未启用价格保护，直接通过
        if !config.enabled {
            return Ok(());
        }

        // 检查熔断状态
        let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
        if config.circuit_breaker_active {
            if current_block < config.circuit_breaker_until {
                return Err(Error::<T>::MarketCircuitBreakerActive);
            }
            // 审计修复 S2-R11: 到期后自动清理存储
            PriceProtection::<T>::mutate(entity_id, |maybe_config| {
                if let Some(c) = maybe_config {
                    c.circuit_breaker_active = false;
                    c.circuit_breaker_until = 0;
                }
            });
            Self::deposit_event(Event::CircuitBreakerLifted { entity_id });
        }

        // 获取参考价格
        let reference_price: Option<BalanceOf<T>> = {
            // 获取 TWAP 累积器
            let acc = TwapAccumulators::<T>::get(entity_id);

            match acc {
                Some(ref a) if Self::is_twap_data_sufficient(a, current_block, &config) => {
                    // 三周期 TWAP 数据充足，使用 1小时 TWAP
                    Self::calculate_twap(entity_id, TwapPeriod::OneHour)
                }
                _ => {
                    // TWAP 数据不足，使用实体所有者设定的初始价格
                    config.initial_price
                }
            }
        };

        // 如果没有参考价格，跳过检查
        let ref_price = match reference_price {
            Some(p) => p,
            None => return Ok(()),
        };

        // 计算偏离度 (基点)
        let price_u128: u128 = price.into();
        let ref_price_u128: u128 = ref_price.into();

        if ref_price_u128 == 0 {
            return Ok(());
        }

        let deviation_bps = if price_u128 > ref_price_u128 {
            ((price_u128 - ref_price_u128) * 10000 / ref_price_u128).min(u16::MAX as u128) as u16
        } else {
            ((ref_price_u128 - price_u128) * 10000 / ref_price_u128).min(u16::MAX as u128) as u16
        };

        // 检查是否超过最大偏离
        if deviation_bps > config.max_price_deviation {
            return Err(Error::<T>::PriceDeviationTooHigh);
        }

        Ok(())
    }

    /// Check whether TWAP data is sufficient to serve as the reference price.
    ///
    /// Conditions:
    /// 1. `trade_count >= min_trades_for_twap`
    /// 2. The market has accumulated at least one hour of real trading history
    ///    (`current_block - first_trade_block >= BlocksPerHour`), since the
    ///    reference price used by `check_price_deviation` is the 1h TWAP.
    ///
    /// Note: history coverage is measured from the first real trade, NOT from
    /// the snapshot age. Snapshot checkpoints are rotated on every trade, so
    /// their age shrinks as the market gets more active — using it here would
    /// make the switch from `initial_price` to TWAP unreachable (the bug fixed
    /// in v2.5.0).
    ///
    /// 检查 TWAP 数据是否充足，可作为参考价格使用。
    ///
    /// 条件：
    /// 1. 成交量 >= min_trades_for_twap
    /// 2. 市场已积累至少 1 小时的真实成交历史
    ///    （当前区块 - first_trade_block >= BlocksPerHour），因为
    ///    `check_price_deviation` 使用的参考价是 1小时 TWAP。
    ///
    /// 注意：历史覆盖时长从首笔真实成交起算，而非快照年龄。快照 checkpoint
    /// 随每次成交滚动刷新，市场越活跃年龄越小——若用快照年龄判断，会导致
    /// 参考价永远无法从 `initial_price` 切换到 TWAP（v2.5.0 修复的 bug）。
    fn is_twap_data_sufficient(
        acc: &TwapAccumulator<BalanceOf<T>>,
        current_block: u32,
        config: &PriceProtectionConfig<BalanceOf<T>>,
    ) -> bool {
        // 检查成交量
        if acc.trade_count < config.min_trades_for_twap {
            return false;
        }

        // 检查真实成交历史覆盖时长（first_trade_block == 0 表示尚无真实成交）
        if acc.first_trade_block == 0 {
            return false;
        }
        current_block.saturating_sub(acc.first_trade_block) >= T::BlocksPerHour::get()
    }

    /// 检查并触发熔断机制
    fn check_circuit_breaker(entity_id: u64, current_price: BalanceOf<T>) {
        let config = match PriceProtection::<T>::get(entity_id) {
            Some(c) => c,
            None => return,
        };

        if !config.enabled {
            return;
        }

        // 使用 7天 TWAP 判断熔断
        let twap_7d = match Self::calculate_twap(entity_id, TwapPeriod::OneWeek) {
            Some(t) => t,
            None => return,
        };

        let price_u128: u128 = current_price.into();
        let twap_u128: u128 = twap_7d.into();

        if twap_u128 == 0 {
            return;
        }

        let deviation_bps = if price_u128 > twap_u128 {
            ((price_u128 - twap_u128) * 10000 / twap_u128).min(u16::MAX as u128) as u16
        } else {
            ((twap_u128 - price_u128) * 10000 / twap_u128).min(u16::MAX as u128) as u16
        };

        // 如果偏离超过熔断阈值，触发熔断
        if deviation_bps > config.circuit_breaker_threshold {
            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
            let until_block = current_block.saturating_add(T::CircuitBreakerDuration::get());

            PriceProtection::<T>::mutate(entity_id, |maybe_config| {
                if let Some(c) = maybe_config {
                    c.circuit_breaker_active = true;
                    c.circuit_breaker_until = until_block;
                }
            });

            Self::deposit_event(Event::CircuitBreakerTriggered {
                entity_id,
                current_price,
                twap_7d,
                deviation_bps,
                until_block,
            });
        }
    }

    /// 在成交后更新 TWAP 并检查熔断
    pub(crate) fn on_trade_completed(entity_id: u64, trade_price: BalanceOf<T>) {
        // 更新 TWAP 累积器
        Self::update_twap_accumulator(entity_id, trade_price);

        // 更新最新成交价
        Self::update_last_trade_price(entity_id, trade_price);

        // L1: 发出 TwapUpdated 事件
        let twap_1h = Self::calculate_twap(entity_id, TwapPeriod::OneHour);
        let twap_24h = Self::calculate_twap(entity_id, TwapPeriod::OneDay);
        let twap_7d = Self::calculate_twap(entity_id, TwapPeriod::OneWeek);
        Self::deposit_event(Event::TwapUpdated {
            entity_id,
            new_price: trade_price,
            twap_1h,
            twap_24h,
            twap_7d,
        });

        // 检查熔断
        Self::check_circuit_breaker(entity_id, trade_price);
    }
}
