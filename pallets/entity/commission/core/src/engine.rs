//! 佣金计算引擎子模块
//!
//! 从 lib.rs 提取的佣金计算/记账/取消相关函数：
//! - process_commission — NEX 佣金调度引擎
//! - credit_commission — NEX 佣金记账
//! - process_token_commission — Token 佣金调度引擎
//! - credit_token_commission — Token 佣金记账
//! - process_shopping_commission — 购物余额佣金调度引擎
//! - credit_shopping_commission — 购物余额佣金记账
//! - cancel_commission — 取消 NEX 佣金
//! - do_cancel_token_commission — 取消 Token 佣金
//! - cancel_shopping_commission — 取消购物余额佣金
//! - do_settle_order_records — 结算订单佣金记录

use crate::pallet::*;
use frame_support::pallet_prelude::*;
use frame_support::traits::{Currency, ExistenceRequirement, ReservableCurrency};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_commission_common::{
    CommissionModes, CommissionPlugin, CommissionRecord, CommissionStatus, CommissionType,
    EntityReferrerProvider, FundingSource, LevelDiffQueryProvider, MultiLevelQueryProvider,
    PluginStatsRollback, PoolFundingCallback, SingleLineQueryProvider, TeamQueryProvider,
    TokenCommissionPlugin, TokenCommissionRecord, TokenTransferProvider,
};
use pallet_entity_common::{
    EntityProvider, FundProtectionQueryPort, LoyaltyReadPort, LoyaltyTokenReadPort, ShopProvider,
};
use sp_runtime::traits::{SaturatedConversion, Saturating, Zero};

impl<T: Config> Pallet<T> {
    /// BUG-1 修复: 将订单的所有 Pending 佣金记录结算为 Withdrawn
    ///
    /// 由订单模块在订单完结（确认收货/超时完成）时通过
    /// CommissionProvider::settle_order_commission 调用。
    /// 标记后记录可被 archive_order_records 归档释放存储。
    pub(crate) fn do_settle_order_records(order_id: u64) -> DispatchResult {
        OrderCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                if record.status == CommissionStatus::Pending {
                    record.status = CommissionStatus::Settled;
                }
            }
        });
        OrderTokenCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                if record.status == CommissionStatus::Pending {
                    record.status = CommissionStatus::Settled;
                }
            }
        });
        Self::deposit_event(Event::OrderRecordsSettled { order_id });
        Ok(())
    }

    /// 调度引擎：处理订单返佣（双来源架构）
    ///
    /// 订单来自 shop_id，佣金记账在 entity_id 级。
    /// 双来源并行：
    /// - 池 A（平台费池）：platform_fee × ReferrerShareBps → 招商推荐人奖金（EntityReferral）
    /// - 池 B（卖家池）：order_amount × effective_rate → 会员返佣（5 个插件）
    ///   seller_transferable.min() 保证不超过卖家实际可转余额
    ///
    /// available_pool = order_amount - platform_fee（订单级精确值），
    /// 用于限制 Phase 1.5 奖池入账上限，防止 seller 被多扣平台费等额资金。
    pub(crate) fn process_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        order_amount: BalanceOf<T>,
        available_pool: BalanceOf<T>,
        platform_fee: BalanceOf<T>,
        product_id: u64,
        seller_reserved: BalanceOf<T>,
    ) -> DispatchResult {
        // F17: 全局紧急暂停检查 — soft check + unreserve
        if GlobalCommissionPaused::<T>::get() {
            if !seller_reserved.is_zero() {
                if let Some(seller) = T::ShopProvider::shop_owner(shop_id) {
                    T::Currency::unreserve(&seller, seller_reserved);
                }
            }
            return Ok(());
        }
        // P0-2 审计修复: 幂等保护，防止同一订单重复处理佣金
        if OrderCommissionProcessed::<T>::get(order_id) {
            if !seller_reserved.is_zero() {
                if let Some(seller) = T::ShopProvider::shop_owner(shop_id) {
                    T::Currency::unreserve(&seller, seller_reserved);
                }
            }
            return Ok(());
        }

        let platform_account = T::PlatformAccount::get();

        // ── 平台费无条件转国库（无论佣金是否配置，保障平台收入） ──
        // 全局固定规则：referrer 拿 ReferrerShareBps%，剩余进国库
        let config = CommissionConfigs::<T>::get(entity_id).filter(|c| c.enabled);

        // 计算推荐人奖金占比（有 referrer 时才预留）
        // 全局治理: 推荐链深度超过阈值的 Entity，免除推荐人招商提成
        // Entity 级: Owner/Admin 可主动关闭推荐人佣金
        let global_referrer_bps = T::ReferrerShareBps::get();
        let exempt_threshold = ReferrerExemptThreshold::<T>::get();
        let is_exempt = exempt_threshold > 0 && {
            let max_depth = T::MultiLevelQuery::tier_count(entity_id)
                .max(T::SingleLineQuery::chain_depth(entity_id))
                .max(T::LevelDiffQuery::chain_depth(entity_id))
                .max(T::TeamQuery::chain_depth(entity_id));
            max_depth > exempt_threshold
        };
        let entity_disabled = ReferrerPayoutDisabled::<T>::get(entity_id);
        let has_referrer = !is_exempt
            && !entity_disabled
            && global_referrer_bps > 0
            && T::EntityReferrerProvider::entity_referrer(entity_id).is_some();
        let referrer_quota = if has_referrer {
            platform_fee.saturating_mul(global_referrer_bps.into()) / 10000u32.into()
        } else {
            BalanceOf::<T>::zero()
        };
        let treasury_portion = platform_fee.saturating_sub(referrer_quota);

        if !treasury_portion.is_zero() {
            let treasury_account = T::TreasuryAccount::get();
            let platform_balance = T::Currency::free_balance(&platform_account);
            let min_balance = T::Currency::minimum_balance();
            let platform_transferable = platform_balance.saturating_sub(min_balance);
            // 为推荐人预留额度：先扣除 referrer_quota，剩余才给国库
            let treasury_cap = platform_transferable.saturating_sub(referrer_quota);
            let actual_treasury = treasury_portion.min(treasury_cap);
            if !actual_treasury.is_zero() {
                T::Currency::transfer(
                    &platform_account,
                    &treasury_account,
                    actual_treasury,
                    ExistenceRequirement::KeepAlive,
                )?;
                OrderTreasuryTransfer::<T>::insert(order_id, actual_treasury);
                Self::deposit_event(Event::PlatformFeeToTreasury {
                    order_id,
                    amount: actual_treasury,
                });
            }
        }

        // 未配置佣金或未启用 → 平台费已入库，释放锁定后返回
        let config = match config {
            Some(c) => c,
            None => {
                if !seller_reserved.is_zero() {
                    if let Some(seller) = T::ShopProvider::shop_owner(shop_id) {
                        T::Currency::unreserve(&seller, seller_reserved);
                    }
                }
                return Ok(());
            }
        };
        let seller = T::ShopProvider::shop_owner(shop_id).ok_or(Error::<T>::ShopNotFound)?;
        let entity_account = T::EntityProvider::entity_account(entity_id);
        let now = <frame_system::Pallet<T>>::block_number();
        let buyer_stats = MemberCommissionStats::<T>::get(entity_id, buyer);
        let is_first_order = buyer_stats.order_count == 0;
        let enabled_modes = config.enabled_modes;

        // P0-9 审计修复: 先递增 order_count，再传给插件
        // 原代码在所有插件执行完才 +1，导致 RepeatPurchase 的 min_orders 判断偏差
        // （第 N 笔订单看到的 order_count 是 N-1）
        let current_order_count = buyer_stats.order_count.saturating_add(1);
        MemberCommissionStats::<T>::mutate(entity_id, buyer, |stats| {
            stats.order_count = current_order_count;
        });

        let mut total_from_platform = BalanceOf::<T>::zero();
        let mut total_from_seller = BalanceOf::<T>::zero();

        // ── 池 A：招商推荐人奖金（从平台费扣除，比例由全局常量控制） ──
        // L1-R5 审计修复: 复用上方已计算的 referrer_quota 和 has_referrer，避免重复存储读取和计算
        if has_referrer {
            if let Some(referrer) = T::EntityReferrerProvider::entity_referrer(entity_id) {
                // KeepAlive 要求转账后余额 >= ED
                let platform_balance = T::Currency::free_balance(&platform_account);
                let min_balance = T::Currency::minimum_balance();
                let transferable = platform_balance.saturating_sub(min_balance);
                let referrer_amount = referrer_quota.min(transferable);
                if !referrer_amount.is_zero() {
                    if Self::credit_commission(
                        entity_id,
                        shop_id,
                        order_id,
                        buyer,
                        &referrer,
                        referrer_amount,
                        CommissionType::EntityReferral,
                        0,
                        now,
                    )? {
                        total_from_platform = referrer_amount;
                    }
                }
            }
        }

        // ── 池 B：会员返佣（从卖家货款扣除） ──
        // 费率优先级: 产品覆盖 > 店铺覆盖 > Entity 默认
        // 基数为 order_amount（与 plugin_budget_cap 同维度），
        // seller_reserved 精确限制可用预算（不受其他模块 reserve 干扰）。
        // 防御性截断: 确保 effective_rate ≤ budget_ceiling（10000 - platform_fee_rate）
        let budget_ceiling = Self::commission_budget_ceiling();
        let effective_rate = ProductCommissionRate::<T>::get(product_id)
            .or_else(|| ShopCommissionRate::<T>::get(shop_id))
            .unwrap_or(config.max_commission_rate)
            .min(budget_ceiling);
        let max_commission = order_amount.saturating_mul(effective_rate.into()) / 10000u32.into();
        // Reserve 模式: 使用传入的 reserved 金额作为可用预算
        let mut remaining = max_commission.min(seller_reserved);
        // Owner 奖励直接到账金额（从 seller reserved 中 unreserve+transfer 给 Owner）
        let mut owner_reward_amount = BalanceOf::<T>::zero();

        if !remaining.is_zero() {
            let initial_remaining = remaining;
            // M-2 审计修复: 跟踪记录容量是否耗尽
            let mut records_full = false;
            let mut dropped_outputs: u32 = 0;
            let mut dropped_amount = BalanceOf::<T>::zero();

            // ── Owner 奖励（从 Pool B 预算中优先扣除，直接转入 Owner 个人账户） ──
            // 不创建 CommissionRecord，不追踪 OrderOwnerReward。
            // 订单状态机保证：cancel 只在 Paid/Shipped 触发，此时 process_commission
            // 尚未执行，Owner 奖励不存在，无需回收。
            if !records_full
                && enabled_modes.contains(CommissionModes::OWNER_REWARD)
                && config.owner_reward_rate > 0
            {
                let owner_amount =
                    order_amount.saturating_mul(config.owner_reward_rate.into()) / 10000u32.into();
                let owner_amount = owner_amount.min(remaining);
                if !owner_amount.is_zero() {
                    if let Some(owner) = T::EntityProvider::entity_owner(entity_id) {
                        // seller 资金此时在 reserved，先 unreserve 再 transfer 到 owner
                        T::Currency::unreserve(&seller, owner_amount);
                        // P1-FIX + P2-FIX: 转账失败时回流到 UnallocatedPool
                        if T::Currency::transfer(
                            &seller,
                            &owner,
                            owner_amount,
                            ExistenceRequirement::KeepAlive,
                        )
                        .is_ok()
                        {
                            remaining = remaining.saturating_sub(owner_amount);
                            owner_reward_amount = owner_amount;
                            Self::deposit_event(Event::OwnerRewardPaid {
                                entity_id,
                                order_id,
                                to: owner,
                                amount: owner_amount,
                            });
                        } else {
                            // 转账失败，re-reserve 并回流到未分配池
                            let _ = T::Currency::reserve(&seller, owner_amount);
                            UnallocatedPool::<T>::mutate(entity_id, |pool| {
                                *pool = pool.saturating_add(owner_amount);
                            });
                            dropped_amount = dropped_amount.saturating_add(owner_amount);
                        }
                    } else {
                        // Owner 不存在，回流到未分配池（资金保持在 seller reserved）
                        UnallocatedPool::<T>::mutate(entity_id, |pool| {
                            *pool = pool.saturating_add(owner_amount);
                        });
                        dropped_amount = dropped_amount.saturating_add(owner_amount);
                    }
                }
            }

            // cap 与 max_commission_rate 同一量纲（bps of order_amount），
            // plugin_budget = min(remaining, order_amount × cap / 10000)
            // remaining.min() 保证不超过实际可分配余额。
            let cap_base = order_amount;

            // M-2 审计修复: 内联宏——处理单个插件的 outputs 循环，遇到 RecordsFull 停止并累计 dropped
            macro_rules! credit_outputs {
                ($outputs:expr) => {
                    if !records_full {
                        for output in $outputs {
                            if Self::credit_commission(
                                entity_id,
                                shop_id,
                                order_id,
                                buyer,
                                &output.beneficiary,
                                output.amount,
                                output.commission_type,
                                output.level,
                                now,
                            )? {
                                // 成功
                            } else {
                                // 容量耗尽 — 当前 output 及后续全部丢弃
                                records_full = true;
                                dropped_outputs += 1;
                                dropped_amount = dropped_amount.saturating_add(output.amount);
                            }
                        }
                    } else {
                        // 已满，直接累计所有 outputs 到 dropped
                        for output in $outputs {
                            dropped_outputs += 1;
                            dropped_amount = dropped_amount.saturating_add(output.amount);
                        }
                    }
                };
            }

            // 1. Referral Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.referral_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::ReferralPlugin::calculate(
                    entity_id,
                    buyer,
                    order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                // P1-2 审计修复: 插件不变量校验
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_outputs!(outputs);
            }

            // 2. MultiLevel Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.multi_level_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::MultiLevelPlugin::calculate(
                    entity_id,
                    buyer,
                    order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_outputs!(outputs);
            }

            // 3. LevelDiff Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.level_diff_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::LevelDiffPlugin::calculate(
                    entity_id,
                    buyer,
                    order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_outputs!(outputs);
            }

            // 4. SingleLine Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.single_line_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::SingleLinePlugin::calculate(
                    entity_id,
                    buyer,
                    order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_outputs!(outputs);
            }

            // 5. Team Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.team_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::TeamPlugin::calculate(
                    entity_id,
                    buyer,
                    order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_outputs!(outputs);
            }

            // M-2 审计修复: 容量耗尽时，dropped_amount 退回 remaining，
            // 后续 Phase 1.5 会将其导入沉淀池（而非丢失）
            if records_full {
                remaining = remaining.saturating_add(dropped_amount);
                Self::deposit_event(Event::CommissionRecordsCapacityExhausted {
                    entity_id,
                    order_id,
                    dropped_outputs,
                    dropped_amount,
                });
            }

            total_from_seller = initial_remaining
                .saturating_sub(remaining)
                .saturating_sub(owner_reward_amount);
        }

        // ── Phase 1.5：未分配佣金 → 沉淀资金池 ──
        // seller 本单实际所得 = available_pool（order_amount - platform_fee），
        // 扣除已发佣金后，剩余才是本单可入池的最大金额。
        // 防止 remaining（基于 order_amount 计算）超出 seller 本单所得，侵蚀 seller 自有资金。
        let max_pool_from_order = available_pool
            .saturating_sub(total_from_seller)
            .saturating_sub(owner_reward_amount);
        let remaining = remaining.min(max_pool_from_order);
        let mut pool_funded = BalanceOf::<T>::zero();
        if enabled_modes.contains(CommissionModes::POOL_REWARD) && !remaining.is_zero() {
            // Reserve 模式: 从 reserved 中划转，而非 free balance
            // owner_reward_amount 已从 reserved 中 unreserve 并转给 owner，需一并扣除
            let still_reserved = seller_reserved
                .saturating_sub(total_from_seller)
                .saturating_sub(owner_reward_amount);
            let actual_pool = remaining.min(still_reserved);
            if !actual_pool.is_zero() {
                if T::Currency::total_balance(&entity_account).is_zero() {
                    T::Currency::unreserve(&seller, actual_pool);
                    T::Currency::transfer(
                        &seller,
                        &entity_account,
                        actual_pool,
                        ExistenceRequirement::KeepAlive,
                    )?;
                } else {
                    T::Currency::repatriate_reserved(
                        &seller,
                        &entity_account,
                        actual_pool,
                        frame_support::traits::BalanceStatus::Free,
                    )?;
                }
                UnallocatedPool::<T>::mutate(entity_id, |pool| {
                    *pool = pool.saturating_add(actual_pool);
                });
                T::PoolFundingCallback::on_pool_funded(
                    entity_id,
                    FundingSource::OrderCommissionRemainder,
                    actual_pool.saturated_into(),
                    0,
                    order_id,
                );
                OrderUnallocated::<T>::insert(order_id, (entity_id, shop_id, actual_pool));
                pool_funded = actual_pool;
                Self::deposit_event(Event::UnallocatedCommissionPooled {
                    entity_id,
                    order_id,
                    amount: actual_pool,
                });
            }
        }

        // P0-9: order_count 已在插件执行前递增（见上方），此处不再重复

        // total_distributed 仅统计从外部转入的佣金（不含池内循环）
        let total_distributed = total_from_platform.saturating_add(total_from_seller);

        // 更新 Entity 统计
        ShopCommissionTotals::<T>::mutate(entity_id, |(total, orders)| {
            *total = total.saturating_add(total_distributed);
            *orders = orders.saturating_add(1);
        });

        // 将佣金资金转入 Entity 账户（双来源分别转；池资金已在 entity_account 中）
        if !total_from_platform.is_zero() {
            T::Currency::transfer(
                &platform_account,
                &entity_account,
                total_from_platform,
                ExistenceRequirement::KeepAlive,
            )?;
        }

        // Reserve 模式: Pool B 使用 repatriate_reserved（seller.reserved → entity.free）
        // 若 entity_account 尚未存在（total_balance==0），repatriate_reserved 会失败（DeadAccount），
        // 此时先 unreserve 再用普通 transfer 创建账户。
        if !total_from_seller.is_zero() {
            if T::Currency::total_balance(&entity_account).is_zero() {
                T::Currency::unreserve(&seller, total_from_seller);
                T::Currency::transfer(
                    &seller,
                    &entity_account,
                    total_from_seller,
                    ExistenceRequirement::KeepAlive,
                )?;
            } else {
                T::Currency::repatriate_reserved(
                    &seller,
                    &entity_account,
                    total_from_seller,
                    frame_support::traits::BalanceStatus::Free,
                )?;
            }
        }

        if !total_distributed.is_zero() || !pool_funded.is_zero() {
            Self::deposit_event(Event::CommissionFundsTransferred {
                entity_id,
                shop_id,
                amount: total_distributed.saturating_add(pool_funded),
            });
        }

        // P0-2 审计修复: 标记订单 NEX 佣金已处理
        OrderCommissionProcessed::<T>::insert(order_id, true);

        // ── Reserve 模式: 释放 seller 上本单未使用的锁定 ──
        // owner_reward_amount 已从 reserved 中 unreserve 并转给 owner，需一并扣除
        let total_deducted = total_from_seller
            .saturating_add(pool_funded)
            .saturating_add(owner_reward_amount);
        let unused_reserved = seller_reserved.saturating_sub(total_deducted);
        if !unused_reserved.is_zero() {
            T::Currency::unreserve(&seller, unused_reserved);
        }

        Ok(())
    }

    /// 记录并发放返佣（Entity 级记账）
    ///
    /// M-2 审计修复: 返回 Ok(true) 表示成功，Ok(false) 表示记录已满（容量耗尽），
    /// 调用方应停止后续 output 的记账并将剩余金额导入沉淀池。
    pub(crate) fn credit_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        beneficiary: &T::AccountId,
        amount: BalanceOf<T>,
        commission_type: CommissionType,
        level: u16,
        now: BlockNumberFor<T>,
    ) -> Result<bool, DispatchError> {
        let record = CommissionRecord {
            entity_id,
            shop_id,
            order_id,
            buyer: buyer.clone(),
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
            status: CommissionStatus::Pending,
            created_at: now,
        };

        // M-2 审计修复: 容量耗尽时返回 Ok(false) 而非 Err，防止整笔订单佣金回滚
        let pushed = OrderCommissionRecords::<T>::try_mutate(
            order_id,
            |records| -> Result<bool, DispatchError> {
                match records.try_push(record) {
                    Ok(()) => Ok(true),
                    Err(_) => Ok(false),
                }
            },
        )?;

        if !pushed {
            return Ok(false);
        }

        MemberCommissionStats::<T>::mutate(entity_id, beneficiary, |stats| {
            stats.total_earned = stats.total_earned.saturating_add(amount);
            stats.pending = stats.pending.saturating_add(amount);
        });

        // 更新最后入账时间（用于冻结期检查）
        MemberLastCredited::<T>::insert(entity_id, beneficiary, now);

        ShopPendingTotal::<T>::mutate(entity_id, |total| {
            *total = total.saturating_add(amount);
        });

        // 推荐链类型佣金：维护 ReferrerEarnedByBuyer 索引
        if matches!(
            commission_type,
            CommissionType::DirectReward
                | CommissionType::FirstOrder
                | CommissionType::RepeatPurchase
                | CommissionType::FixedAmount
        ) {
            ReferrerEarnedByBuyer::<T>::mutate((entity_id, beneficiary, buyer), |earned| {
                *earned = earned.saturating_add(amount);
            });
        }

        // F19: 记录会员佣金关联订单 ID（去重，满则丢弃最旧）
        MemberCommissionOrderIds::<T>::mutate(entity_id, beneficiary, |ids| {
            if !ids.contains(&order_id) {
                if ids.try_push(order_id).is_err() {
                    // 满了 → 移除最旧的，腾出空间
                    if !ids.is_empty() {
                        ids.remove(0);
                    }
                    let _ = ids.try_push(order_id);
                }
            }
        });

        Self::deposit_event(Event::CommissionDistributed {
            entity_id,
            order_id,
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
        });

        Ok(true)
    }

    // ====================================================================
    // Token 多资产管线
    // ====================================================================

    /// Token 调度引擎：处理 Token 订单返佣（双源架构）
    ///
    /// 池 A（Token 平台费池）：token_platform_fee → 招商推荐人 Token 奖金 + Entity 留存
    /// 池 B（Token 佣金池）：token_order_amount × effective_rate → 5 插件 → 沉淀池
    ///   available_token.min() 保证不超过 Entity 可用 Token 余额
    ///
    /// token_available_pool = token_order_amount - token_platform_fee（订单级精确值），
    /// 用于限制 Token 沉淀池入账上限，与 NEX 管线 available_pool 对称。
    pub(crate) fn process_token_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        token_order_amount: TokenBalanceOf<T>,
        token_available_pool: TokenBalanceOf<T>,
        token_platform_fee: TokenBalanceOf<T>,
        product_id: u64,
    ) -> DispatchResult {
        // F17: 全局紧急暂停检查
        ensure!(
            !GlobalCommissionPaused::<T>::get(),
            Error::<T>::GlobalCommissionPaused
        );
        // P0-2 审计修复: Token 幂等保护，防止同一订单重复处理
        ensure!(
            !OrderTokenCommissionProcessed::<T>::get(order_id),
            Error::<T>::OrderAlreadyProcessed
        );

        // M2-R5 审计修复: 先 sweep 再检查配置，未配置时优雅返回（与 NEX 版 process_commission 对称）
        Self::sweep_token_free_balance(entity_id, token_platform_fee);

        let config = match CommissionConfigs::<T>::get(entity_id).filter(|c| c.enabled) {
            Some(c) => c,
            None => return Ok(()),
        };

        let enabled_modes = config.enabled_modes;
        let entity_account = T::EntityProvider::entity_account(entity_id);
        let now = <frame_system::Pallet<T>>::block_number();
        let buyer_stats = MemberTokenCommissionStats::<T>::get(entity_id, buyer);
        let is_first_order = buyer_stats.order_count == 0;

        // P0-9 审计修复: Token 管线同样先递增 order_count 再传给插件
        let current_token_order_count = buyer_stats.order_count.saturating_add(1);
        MemberTokenCommissionStats::<T>::mutate(entity_id, buyer, |stats| {
            stats.order_count = current_token_order_count;
        });

        // ── 池 A：Token 招商推荐人奖金（从 Token 平台费中分配） ──
        // 全局治理: 推荐链深度超过阈值的 Entity，免除推荐人招商提成
        // Entity 级: Owner/Admin 可主动关闭推荐人佣金
        let mut pool_a_distributed = TokenBalanceOf::<T>::zero();
        let referrer_share_bps = T::ReferrerShareBps::get();
        let exempt_threshold = ReferrerExemptThreshold::<T>::get();
        let token_is_exempt = exempt_threshold > 0 && {
            let max_depth = T::MultiLevelQuery::tier_count(entity_id)
                .max(T::SingleLineQuery::chain_depth(entity_id))
                .max(T::LevelDiffQuery::chain_depth(entity_id))
                .max(T::TeamQuery::chain_depth(entity_id));
            max_depth > exempt_threshold
        };
        let token_entity_disabled = ReferrerPayoutDisabled::<T>::get(entity_id);
        if !token_is_exempt
            && !token_entity_disabled
            && referrer_share_bps > 0
            && !token_platform_fee.is_zero()
        {
            if let Some(referrer) = T::EntityReferrerProvider::entity_referrer(entity_id) {
                let referrer_quota =
                    token_platform_fee.saturating_mul(referrer_share_bps.into()) / 10000u32.into();
                if !referrer_quota.is_zero() {
                    if Self::credit_token_commission(
                        entity_id,
                        order_id,
                        buyer,
                        &referrer,
                        referrer_quota,
                        CommissionType::EntityReferral,
                        0,
                        now,
                    )? {
                        pool_a_distributed = referrer_quota;
                    }
                }
            }
        }
        // 池 A 剩余部分计入沉淀池（不留为 FREE_BALANCE）
        let pool_a_retention = token_platform_fee.saturating_sub(pool_a_distributed);
        if !pool_a_retention.is_zero() {
            UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                *pool = pool.saturating_add(pool_a_retention);
            });
            T::PoolFundingCallback::on_pool_funded(
                entity_id,
                FundingSource::TokenPlatformFeeRetention,
                0,
                pool_a_retention.saturated_into(),
                order_id,
            );
            // M2-R6 审计修复: 记录 Pool A 留存，供 cancel 时回退
            OrderTokenPlatformRetention::<T>::insert(order_id, (entity_id, pool_a_retention));
            Self::deposit_event(Event::TokenUnallocatedPooled {
                entity_id,
                order_id,
                amount: pool_a_retention,
            });
        }

        // ── 池 B：会员 Token 返佣（从 entity_account Token 余额中分配） ──
        // 费率优先级: 产品覆盖 > 店铺覆盖 > Entity 默认
        // 基数为 token_order_amount（与 plugin_budget_cap 同维度），
        // available_token.min() 保证不超过 Entity 可用 Token 余额。
        // 防御性截断: 确保 effective_rate ≤ budget_ceiling（10000 - platform_fee_rate）
        let token_budget_ceiling = Self::commission_budget_ceiling();
        let effective_rate = ProductCommissionRate::<T>::get(product_id)
            .or_else(|| ShopCommissionRate::<T>::get(shop_id))
            .unwrap_or(config.max_commission_rate)
            .min(token_budget_ceiling);
        let max_commission =
            token_order_amount.saturating_mul(effective_rate.into()) / 10000u32.into();

        let entity_token_balance =
            T::TokenTransferProvider::token_balance_of(entity_id, &entity_account);
        // H1 审计修复: Token 佣金预算必须扣除已承诺的 Token 额度
        // （包括待提现佣金、购物余额、沉淀池）避免跨订单重复承诺
        // NEX 管线无此问题——转账即时发生，seller 余额自然递减；
        // Token 管线是纯记账模式，entity_token_balance 不变，需手动扣除。
        let committed = TokenPendingTotal::<T>::get(entity_id)
            .saturating_add(T::LoyaltyToken::token_shopping_total(entity_id))
            .saturating_add(UnallocatedTokenPool::<T>::get(entity_id));
        let available_token = entity_token_balance.saturating_sub(committed);
        let mut remaining = max_commission.min(available_token);
        let initial_remaining = remaining;
        // Owner Token 奖励直接到账金额（从 entity_account 转给 Owner）
        let mut owner_token_reward_amount = TokenBalanceOf::<T>::zero();

        if !remaining.is_zero() {
            // M-2 审计修复: Token 管线同样跟踪记录容量
            let mut token_records_full = false;
            let mut token_dropped_outputs: u32 = 0;
            let mut token_dropped_amount = TokenBalanceOf::<T>::zero();

            // ── Owner 奖励（从 Token Pool B 预算中优先扣除，直接转入 Owner 个人账户） ──
            // 不创建 TokenCommissionRecord，不追踪 OrderTokenOwnerReward。
            // 与 NEX 对称：订单状态机保证 cancel 时 Owner 奖励尚未发放，无需回收。
            if !token_records_full
                && enabled_modes.contains(CommissionModes::OWNER_REWARD)
                && config.owner_reward_rate > 0
            {
                let owner_amount = token_order_amount
                    .saturating_mul(config.owner_reward_rate.into())
                    / 10000u32.into();
                let owner_amount = owner_amount.min(remaining);
                if !owner_amount.is_zero() {
                    if let Some(owner) = T::EntityProvider::entity_owner(entity_id) {
                        // Token: entity_account → owner
                        if T::TokenTransferProvider::token_transfer(
                            entity_id,
                            &entity_account,
                            &owner,
                            owner_amount,
                        )
                        .is_ok()
                        {
                            remaining = remaining.saturating_sub(owner_amount);
                            // 扣减 EntityTokenAccountedBalance（资金已离开 entity）
                            EntityTokenAccountedBalance::<T>::mutate(entity_id, |b| {
                                *b = Some(b.unwrap_or_default().saturating_sub(owner_amount));
                            });
                            owner_token_reward_amount = owner_amount;
                            Self::deposit_event(Event::TokenOwnerRewardPaid {
                                entity_id,
                                order_id,
                                to: owner,
                                amount: owner_amount,
                            });
                        } else {
                            // 转账失败，回流到未分配池
                            UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                                *pool = pool.saturating_add(owner_amount);
                            });
                            token_dropped_amount =
                                token_dropped_amount.saturating_add(owner_amount);
                        }
                    } else {
                        // Owner 不存在，回流到未分配池
                        UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                            *pool = pool.saturating_add(owner_amount);
                        });
                        token_dropped_amount = token_dropped_amount.saturating_add(owner_amount);
                    }
                }
            }

            // cap 与 max_commission_rate 同一量纲（bps of token_order_amount），
            // remaining.min() 保证不超过实际可分配余额。
            let cap_base = token_order_amount;

            // M-2 审计修复: Token 管线 credit_outputs 宏
            macro_rules! credit_token_outputs {
                ($outputs:expr) => {
                    if !token_records_full {
                        for output in $outputs {
                            if Self::credit_token_commission(
                                entity_id,
                                order_id,
                                buyer,
                                &output.beneficiary,
                                output.amount,
                                output.commission_type,
                                output.level,
                                now,
                            )? {
                                // 成功
                            } else {
                                token_records_full = true;
                                token_dropped_outputs += 1;
                                token_dropped_amount =
                                    token_dropped_amount.saturating_add(output.amount);
                            }
                        }
                    } else {
                        for output in $outputs {
                            token_dropped_outputs += 1;
                            token_dropped_amount =
                                token_dropped_amount.saturating_add(output.amount);
                        }
                    }
                };
            }

            // 1. Token Referral Plugin
            if !token_records_full {
                let plugin_budget =
                    Self::capped_token_budget(config.plugin_caps.referral_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::TokenReferralPlugin::calculate_token(
                    entity_id,
                    buyer,
                    token_order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_token_order_count,
                    order_id,
                );
                // P1-2 审计修复: Token 插件不变量校验
                let outputs_sum = outputs.iter().fold(TokenBalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_token_outputs!(outputs);
            }

            // 2. Token MultiLevel Plugin
            if !token_records_full {
                let plugin_budget = Self::capped_token_budget(
                    config.plugin_caps.multi_level_cap,
                    cap_base,
                    remaining,
                );
                let (outputs, new_remaining) = T::TokenMultiLevelPlugin::calculate_token(
                    entity_id,
                    buyer,
                    token_order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_token_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(TokenBalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_token_outputs!(outputs);
            }

            // 3. Token LevelDiff Plugin
            if !token_records_full {
                let plugin_budget = Self::capped_token_budget(
                    config.plugin_caps.level_diff_cap,
                    cap_base,
                    remaining,
                );
                let (outputs, new_remaining) = T::TokenLevelDiffPlugin::calculate_token(
                    entity_id,
                    buyer,
                    token_order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_token_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(TokenBalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_token_outputs!(outputs);
            }

            // 4. Token SingleLine Plugin
            if !token_records_full {
                let plugin_budget = Self::capped_token_budget(
                    config.plugin_caps.single_line_cap,
                    cap_base,
                    remaining,
                );
                let (outputs, new_remaining) = T::TokenSingleLinePlugin::calculate_token(
                    entity_id,
                    buyer,
                    token_order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_token_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(TokenBalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_token_outputs!(outputs);
            }

            // 5. Token Team Plugin
            if !token_records_full {
                let plugin_budget =
                    Self::capped_token_budget(config.plugin_caps.team_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::TokenTeamPlugin::calculate_token(
                    entity_id,
                    buyer,
                    token_order_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_token_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(TokenBalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_token_outputs!(outputs);
            }

            // M-2 审计修复: Token 容量耗尽时，dropped_amount 退回 remaining → 沉淀池
            if token_records_full {
                remaining = remaining.saturating_add(token_dropped_amount);
                Self::deposit_event(Event::TokenCommissionRecordsCapacityExhausted {
                    entity_id,
                    order_id,
                    dropped_outputs: token_dropped_outputs,
                    dropped_amount: token_dropped_amount,
                });
            }
        }

        // 剩余 Token → 沉淀池
        // seller 本单 Token 实际所得 = token_available_pool（token_order_amount - token_platform_fee），
        // 扣除已发佣金后，剩余才是本单可入池的最大金额，与 NEX 管线对称。
        // owner_token_reward_amount 已直接转给 Owner，需从已分配中排除
        let token_distributed = initial_remaining
            .saturating_sub(remaining)
            .saturating_sub(owner_token_reward_amount);
        let max_token_pool_from_order = token_available_pool
            .saturating_sub(token_distributed)
            .saturating_sub(owner_token_reward_amount);
        let remaining = remaining.min(max_token_pool_from_order);
        if enabled_modes.contains(CommissionModes::POOL_REWARD) && !remaining.is_zero() {
            UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                *pool = pool.saturating_add(remaining);
            });
            T::PoolFundingCallback::on_pool_funded(
                entity_id,
                FundingSource::TokenCommissionRemainder,
                0,
                remaining.saturated_into(),
                order_id,
            );
            OrderTokenUnallocated::<T>::insert(order_id, (entity_id, shop_id, remaining));
            Self::deposit_event(Event::TokenUnallocatedPooled {
                entity_id,
                order_id,
                amount: remaining,
            });
        }

        // P0-9: Token order_count 已在插件执行前递增（见上方），此处不再重复

        // P0-2 审计修复: 标记订单 Token 佣金已处理
        OrderTokenCommissionProcessed::<T>::insert(order_id, true);

        Ok(())
    }

    /// Token 佣金记账（纯记账，不转账——Token 在 entity_account 中托管直到提现）
    ///
    /// M-2 审计修复: 返回 Ok(true) 表示成功，Ok(false) 表示记录已满。
    pub(crate) fn credit_token_commission(
        entity_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        beneficiary: &T::AccountId,
        amount: TokenBalanceOf<T>,
        commission_type: CommissionType,
        level: u16,
        now: BlockNumberFor<T>,
    ) -> Result<bool, DispatchError> {
        let record = TokenCommissionRecord {
            entity_id,
            order_id,
            buyer: buyer.clone(),
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
            status: CommissionStatus::Pending,
            created_at: now,
        };

        // M-2 审计修复: 容量耗尽时返回 Ok(false) 而非 Err
        let pushed = OrderTokenCommissionRecords::<T>::try_mutate(
            order_id,
            |records| -> Result<bool, DispatchError> {
                match records.try_push(record) {
                    Ok(()) => Ok(true),
                    Err(_) => Ok(false),
                }
            },
        )?;

        if !pushed {
            return Ok(false);
        }

        MemberTokenCommissionStats::<T>::mutate(entity_id, beneficiary, |stats| {
            stats.total_earned = stats.total_earned.saturating_add(amount);
            stats.pending = stats.pending.saturating_add(amount);
        });

        TokenPendingTotal::<T>::mutate(entity_id, |total| {
            *total = total.saturating_add(amount);
        });

        // P3 审计修复: Token 入账更新独立冻结时间（与 NEX 的 MemberLastCredited 解耦）
        MemberTokenLastCredited::<T>::insert(entity_id, beneficiary, now);

        // F19: 记录会员 Token 佣金关联订单 ID（去重，满则丢弃最旧）
        MemberTokenCommissionOrderIds::<T>::mutate(entity_id, beneficiary, |ids| {
            if !ids.contains(&order_id) {
                if ids.try_push(order_id).is_err() {
                    if !ids.is_empty() {
                        ids.remove(0);
                    }
                    let _ = ids.try_push(order_id);
                }
            }
        });

        Self::deposit_event(Event::TokenCommissionDistributed {
            entity_id,
            order_id,
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
        });

        Ok(true)
    }

    /// 取消订单返佣（双来源架构）
    ///
    /// 按 CommissionType 决定退款目标：
    /// - `EntityReferral`: Entity 账户 → 平台账户
    /// - 其余: Entity 账户 → 卖家 (shop_owner)
    ///
    /// H2 审计修复: 先尝试转账，成功后再取消记录和更新统计，
    /// 防止转账失败但记录已被标记为 Cancelled 导致资金丢失。
    pub(crate) fn cancel_commission(order_id: u64) -> DispatchResult {
        let records = OrderCommissionRecords::<T>::get(order_id);
        let platform_account = T::PlatformAccount::get();

        // BUG-3 修复: 可取消状态包括 Pending 和 Settled（同块结算后记录已为 Settled）
        let is_cancellable = |status: &CommissionStatus| {
            matches!(
                status,
                CommissionStatus::Pending | CommissionStatus::Settled
            )
        };

        // 第一步：按 (entity_id, shop_id, is_platform) 分组汇总待退还金额
        // is_platform = true → EntityReferral（退平台），false → 会员返佣（退卖家）
        // PoolReward 记录不参与转账退款（资金回池）
        let mut refund_groups: alloc::vec::Vec<(u64, u64, bool, BalanceOf<T>)> =
            alloc::vec::Vec::new();
        let mut pool_return_groups: alloc::vec::Vec<(u64, BalanceOf<T>)> = alloc::vec::Vec::new();

        for record in records.iter() {
            if is_cancellable(&record.status) {
                if record.commission_type == CommissionType::PoolReward {
                    if let Some(entry) = pool_return_groups
                        .iter_mut()
                        .find(|(e, _)| *e == record.entity_id)
                    {
                        entry.1 = entry.1.saturating_add(record.amount);
                    } else {
                        pool_return_groups.push((record.entity_id, record.amount));
                    }
                } else {
                    let is_platform = record.commission_type == CommissionType::EntityReferral;
                    if let Some(entry) = refund_groups.iter_mut().find(|(e, s, p, _)| {
                        *e == record.entity_id && *s == record.shop_id && *p == is_platform
                    }) {
                        entry.3 = entry.3.saturating_add(record.amount);
                    } else {
                        refund_groups.push((
                            record.entity_id,
                            record.shop_id,
                            is_platform,
                            record.amount,
                        ));
                    }
                }
            }
        }

        // 第二步：尝试转账退款
        let mut refund_failed_groups: alloc::vec::Vec<(u64, u64, bool, BalanceOf<T>)> =
            alloc::vec::Vec::new();
        let mut refund_succeeded_groups: alloc::vec::Vec<(u64, u64, bool, BalanceOf<T>)> =
            alloc::vec::Vec::new();

        for (entity_id, shop_id, is_platform, refund_amount) in refund_groups.iter() {
            if refund_amount.is_zero() {
                continue;
            }
            let entity_account = T::EntityProvider::entity_account(*entity_id);

            let refund_target = if *is_platform {
                platform_account.clone()
            } else {
                match T::ShopProvider::shop_owner(*shop_id) {
                    Some(seller) => seller,
                    None => {
                        Self::deposit_event(Event::CommissionRefundFailed {
                            entity_id: *entity_id,
                            shop_id: *shop_id,
                            amount: *refund_amount,
                        });
                        refund_failed_groups.push((
                            *entity_id,
                            *shop_id,
                            *is_platform,
                            *refund_amount,
                        ));
                        continue;
                    }
                }
            };

            if T::Currency::transfer(
                &entity_account,
                &refund_target,
                *refund_amount,
                ExistenceRequirement::KeepAlive,
            )
            .is_err()
            {
                Self::deposit_event(Event::CommissionRefundFailed {
                    entity_id: *entity_id,
                    shop_id: *shop_id,
                    amount: *refund_amount,
                });
                refund_failed_groups.push((*entity_id, *shop_id, *is_platform, *refund_amount));
            } else {
                refund_succeeded_groups.push((*entity_id, *shop_id, *is_platform, *refund_amount));
            }
        }

        // H-2 审计修复: 构建退款失败的 (entity_id, shop_id, is_platform) 查询集
        // 用于第三步中按记录精确判断是否应扣减统计
        let is_refund_failed = |eid: u64, sid: u64, is_plat: bool| -> bool {
            refund_failed_groups
                .iter()
                .any(|(e, s, p, _)| *e == eid && *s == sid && *p == is_plat)
        };

        // 第三步：H-2 审计修复 — 只有退款成功的记录才扣减统计，退款失败的保持原状
        // PoolReward 记录无需转账，直接回池并取消
        for (entity_id, return_amount) in pool_return_groups.iter() {
            if !return_amount.is_zero() {
                UnallocatedPool::<T>::mutate(entity_id, |pool| {
                    *pool = pool.saturating_add(*return_amount);
                });
                T::PoolFundingCallback::on_pool_funded(
                    *entity_id,
                    FundingSource::CancelReturn,
                    (*return_amount).saturated_into(),
                    0,
                    order_id,
                );
            }
        }

        // AUDIT-FIX: 收集成功取消的记录，用于插件统计回滚 + order_count 条件回滚
        let mut any_cancelled = false;
        let mut cancelled_outputs: alloc::vec::Vec<(T::AccountId, u128, CommissionType, u16)> =
            alloc::vec::Vec::new();

        OrderCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                if is_cancellable(&record.status) {
                    if record.commission_type == CommissionType::PoolReward {
                        // PoolReward: 无转账，直接回池，统计必须扣减
                        MemberCommissionStats::<T>::mutate(
                            record.entity_id,
                            &record.beneficiary,
                            |stats| {
                                stats.pending = stats.pending.saturating_sub(record.amount);
                                stats.total_earned =
                                    stats.total_earned.saturating_sub(record.amount);
                            },
                        );
                        ShopPendingTotal::<T>::mutate(record.entity_id, |total| {
                            *total = total.saturating_sub(record.amount);
                        });
                        record.status = CommissionStatus::Cancelled;
                        any_cancelled = true;
                        // PoolReward 不需要插件统计回滚（无插件参与）
                    } else {
                        let is_platform = record.commission_type == CommissionType::EntityReferral;
                        if is_refund_failed(record.entity_id, record.shop_id, is_platform) {
                            // H-2 审计修复: 退款失败 — 不扣减统计，不改状态
                            // 资金仍在 entity_account 中，stats 保持原值，
                            // 直到 Root retry_pending_refund 成功后再扣减
                        } else {
                            // 退款成功 — 正常扣减统计并标记 Cancelled
                            MemberCommissionStats::<T>::mutate(
                                record.entity_id,
                                &record.beneficiary,
                                |stats| {
                                    stats.pending = stats.pending.saturating_sub(record.amount);
                                    stats.total_earned =
                                        stats.total_earned.saturating_sub(record.amount);
                                },
                            );
                            ShopPendingTotal::<T>::mutate(record.entity_id, |total| {
                                *total = total.saturating_sub(record.amount);
                            });
                            // 推荐链类型：同步扣减 ReferrerEarnedByBuyer
                            if matches!(
                                record.commission_type,
                                CommissionType::DirectReward
                                    | CommissionType::FirstOrder
                                    | CommissionType::RepeatPurchase
                                    | CommissionType::FixedAmount
                            ) {
                                ReferrerEarnedByBuyer::<T>::mutate(
                                    (record.entity_id, &record.beneficiary, &record.buyer),
                                    |earned| {
                                        *earned = earned.saturating_sub(record.amount);
                                    },
                                );
                            }
                            // AUDIT-FIX(BUG-1): 收集成功取消的记录，用于插件统计回滚
                            let amount_u128: u128 = record.amount.try_into().unwrap_or(u128::MAX);
                            cancelled_outputs.push((
                                record.beneficiary.clone(),
                                amount_u128,
                                record.commission_type,
                                record.level,
                            ));
                            record.status = CommissionStatus::Cancelled;
                            any_cancelled = true;
                        }
                    }
                }
            }
        });

        // H-2 审计修复: 退款失败的分组记入 PendingRefunds 存储，供 Root 重试
        if !refund_failed_groups.is_empty() {
            let mut total_failed = BalanceOf::<T>::zero();
            PendingRefunds::<T>::mutate(order_id, |pending| {
                for (eid, sid, is_plat, amount) in refund_failed_groups.iter() {
                    total_failed = total_failed.saturating_add(*amount);
                    let _ = pending.try_push((*eid, *sid, *is_plat, *amount));
                }
            });
            // 按 entity_id 累加 PendingRefundTotal
            for (eid, _, _, amount) in refund_failed_groups.iter() {
                PendingRefundTotal::<T>::mutate(eid, |total| {
                    *total = total.saturating_add(*amount);
                });
            }
            Self::deposit_event(Event::RefundPendingCreated {
                entity_id: refund_failed_groups.first().map(|g| g.0).unwrap_or(0),
                order_id,
                amount: total_failed,
            });
        }

        // 第四步：退还国库部分（Treasury → PlatformAccount）
        let treasury_refund = OrderTreasuryTransfer::<T>::get(order_id);
        if !treasury_refund.is_zero() {
            let treasury_account = T::TreasuryAccount::get();
            if T::Currency::transfer(
                &treasury_account,
                &platform_account,
                treasury_refund,
                ExistenceRequirement::AllowDeath,
            )
            .is_ok()
            {
                OrderTreasuryTransfer::<T>::remove(order_id);
                Self::deposit_event(Event::TreasuryRefund {
                    order_id,
                    amount: treasury_refund,
                });
            } else {
                // 国库余额不足时记录事件，保留 Storage 供后续重试
                Self::deposit_event(Event::CommissionRefundFailed {
                    entity_id: 0,
                    shop_id: 0,
                    amount: treasury_refund,
                });
            }
        }

        // 第五步：退还本订单沉淀池贡献（entity_account → seller）
        let (unalloc_entity_id, unalloc_shop_id, unalloc_amount) =
            OrderUnallocated::<T>::get(order_id);
        if !unalloc_amount.is_zero() {
            let entity_account = T::EntityProvider::entity_account(unalloc_entity_id);
            if let Some(seller) = T::ShopProvider::shop_owner(unalloc_shop_id) {
                if T::Currency::transfer(
                    &entity_account,
                    &seller,
                    unalloc_amount,
                    ExistenceRequirement::KeepAlive,
                )
                .is_ok()
                {
                    UnallocatedPool::<T>::mutate(unalloc_entity_id, |pool| {
                        *pool = pool.saturating_sub(unalloc_amount);
                    });
                    OrderUnallocated::<T>::remove(order_id);
                    Self::deposit_event(Event::UnallocatedPoolRefunded {
                        entity_id: unalloc_entity_id,
                        order_id,
                        amount: unalloc_amount,
                    });
                } else {
                    // E-2 审计修复: 退款失败时发出事件，便于运维追踪
                    Self::deposit_event(Event::UnallocatedPoolRefundFailed {
                        entity_id: unalloc_entity_id,
                        order_id,
                        amount: unalloc_amount,
                    });
                }
            }
        }

        // Owner reward: no clawback needed — the order state machine guarantees
        // cancel_commission runs only before process_commission (Paid/Shipped/Disputed),
        // so Owner rewards have never been paid at this point.
        // Owner 奖励无需回收——订单状态机保证 cancel_commission 仅在
        // process_commission 之前调用（Paid/Shipped/Disputed），此时 Owner 奖励尚未发放。

        // CC-M1: 汇总退款结果
        let total_refund_groups = refund_groups.len() as u32;
        let failed = refund_failed_groups.len() as u32;
        let succeeded = total_refund_groups.saturating_sub(failed);
        Self::deposit_event(Event::CommissionCancelled {
            order_id,
            refund_succeeded: succeeded,
            refund_failed: failed,
        });

        // P0-3 审计修复: 取消订单时回滚 buyer 的 order_count，防止复购计数被取消订单污染
        // 不变量: 同一 order_id 的所有 NEX 佣金记录必定属于同一 entity_id + 同一 buyer，
        // 因为 process_commission 以 (entity_id, buyer) 为入口，单次调用只产出一个组合。
        // AUDIT-FIX(漏洞-4): 仅在至少有一条记录成功取消时才回滚 order_count
        if any_cancelled {
            if let Some(first) = records.first() {
                MemberCommissionStats::<T>::mutate(first.entity_id, &first.buyer, |stats| {
                    stats.order_count = stats.order_count.saturating_sub(1);
                });
            }
        }

        // AUDIT-FIX(BUG-1): 回滚插件内部统计（MultiLevel 等）
        if !cancelled_outputs.is_empty() {
            if let Some(first) = records.first() {
                let entity_id = first.entity_id;
                T::MultiLevelStatsRollback::rollback_stats(entity_id, &cancelled_outputs, true);
            }
        }

        // M3 审计修复: 复用 do_cancel_token_commission 消除代码重复（原第六、七步）
        Self::do_cancel_token_commission(order_id)?;

        // P0-2 审计修复: 取消订单时清除幂等标记（允许后续重新处理，如有需要）
        OrderCommissionProcessed::<T>::remove(order_id);
        OrderTokenCommissionProcessed::<T>::remove(order_id);

        Ok(())
    }

    /// Token 佣金独立取消（供 TokenCommissionProvider::cancel_token_commission 调用）
    pub(crate) fn do_cancel_token_commission(order_id: u64) -> DispatchResult {
        let token_records = OrderTokenCommissionRecords::<T>::get(order_id);
        let mut token_cancelled: u32 = 0;
        // AUDIT-FIX(BUG-1): 收集 Token 取消记录用于插件统计回滚
        let mut token_cancelled_outputs: alloc::vec::Vec<(
            T::AccountId,
            u128,
            CommissionType,
            u16,
        )> = alloc::vec::Vec::new();
        OrderTokenCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                // BUG-3 修复: 同时处理 Pending 和 Settled 状态的 Token 记录
                if matches!(
                    record.status,
                    CommissionStatus::Pending | CommissionStatus::Settled
                ) {
                    MemberTokenCommissionStats::<T>::mutate(
                        record.entity_id,
                        &record.beneficiary,
                        |stats| {
                            stats.pending = stats.pending.saturating_sub(record.amount);
                            stats.total_earned = stats.total_earned.saturating_sub(record.amount);
                        },
                    );
                    TokenPendingTotal::<T>::mutate(record.entity_id, |total| {
                        *total = total.saturating_sub(record.amount);
                    });
                    // AUDIT-FIX(BUG-1): 收集取消的 Token 记录
                    let amount_u128: u128 = record.amount.into();
                    token_cancelled_outputs.push((
                        record.beneficiary.clone(),
                        amount_u128,
                        record.commission_type,
                        record.level,
                    ));
                    record.status = CommissionStatus::Cancelled;
                    token_cancelled = token_cancelled.saturating_add(1);
                }
            }
        });

        // BUG-B 审计修复: Token 取消时回滚 buyer 的 Token order_count（与 NEX cancel_commission 对称）
        // 不变量: 同 NEX 侧，同一 order_id 的 Token 佣金记录共享同一 (entity_id, buyer)。
        // 当 cancel_commission 从 NEX 侧调用时，NEX order_count 由上游回滚，
        // 此处负责 Token 侧独立回滚，确保 cancel_token_commission 被单独调用时也能正确回滚。
        // 仅在实际取消了记录时才回滚（token_cancelled > 0 表明有 Pending/Settled 记录被取消）。
        if token_cancelled > 0 {
            if let Some(first) = token_records.first() {
                MemberTokenCommissionStats::<T>::mutate(first.entity_id, &first.buyer, |stats| {
                    stats.order_count = stats.order_count.saturating_sub(1);
                });
            }
        }

        // AUDIT-FIX(BUG-1): Token 管线回滚插件内部统计（count_order=false，与 Token calculate 对称）
        if !token_cancelled_outputs.is_empty() {
            if let Some(first) = token_records.first() {
                T::MultiLevelStatsRollback::rollback_stats(
                    first.entity_id,
                    &token_cancelled_outputs,
                    false,
                );
            }
        }

        // H2 审计修复: 退还 Token 沉淀池 — 仅在转账成功时扣减池余额
        let (te_id, ts_id, t_amount) = OrderTokenUnallocated::<T>::get(order_id);
        if !t_amount.is_zero() {
            let mut refund_ok = false;
            let entity_account = T::EntityProvider::entity_account(te_id);
            if let Some(seller) = T::ShopProvider::shop_owner(ts_id) {
                if T::TokenTransferProvider::token_transfer(
                    te_id,
                    &entity_account,
                    &seller,
                    t_amount,
                )
                .is_ok()
                {
                    refund_ok = true;
                }
            }
            if refund_ok {
                UnallocatedTokenPool::<T>::mutate(te_id, |pool| {
                    *pool = pool.saturating_sub(t_amount);
                });
                EntityTokenAccountedBalance::<T>::mutate(te_id, |b| {
                    *b = Some(b.unwrap_or_default().saturating_sub(t_amount));
                });
                OrderTokenUnallocated::<T>::remove(order_id);
                Self::deposit_event(Event::TokenUnallocatedPoolRefunded {
                    entity_id: te_id,
                    order_id,
                    amount: t_amount,
                });
            } else {
                // HIGH-1 审计修复: Token 沉淀池退款失败时记入待重试队列（与 NEX PendingRefunds 对称）
                PendingTokenRefundTotal::<T>::mutate(te_id, |total| {
                    *total = total.saturating_add(t_amount);
                });
                PendingTokenRefunds::<T>::mutate(order_id, |pending| {
                    let _ = pending.try_push((te_id, ts_id, t_amount));
                });
                Self::deposit_event(Event::TokenUnallocatedPoolRefundFailed {
                    entity_id: te_id,
                    order_id,
                    amount: t_amount,
                });
            }
        }

        // Token Owner reward: no clawback needed — same state machine guarantee as NEX.
        // cancel_token_commission runs only before process_token_commission.
        // Token Owner 奖励无需回收——与 NEX 相同的状态机保证。
        // cancel_token_commission 仅在 process_token_commission 之前调用。

        // M2-R6 审计修复: 回退 Pool A 留存（token_platform_fee 中未分配给 referrer 的部分）
        // 与 NEX cancel 退还 OrderTreasuryTransfer 对称
        let (retention_entity_id, retention_amount) =
            OrderTokenPlatformRetention::<T>::get(order_id);
        if !retention_amount.is_zero() {
            UnallocatedTokenPool::<T>::mutate(retention_entity_id, |pool| {
                *pool = pool.saturating_sub(retention_amount);
            });
            OrderTokenPlatformRetention::<T>::remove(order_id);
        }

        if token_cancelled > 0 {
            Self::deposit_event(Event::TokenCommissionCancelled {
                order_id,
                cancelled_count: token_cancelled,
            });
        }

        Ok(())
    }

    /// 购物余额分佣结算（Pending → Settled）
    pub(crate) fn do_settle_order_shopping_records(order_id: u64) -> DispatchResult {
        OrderShoppingCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                if record.status == CommissionStatus::Pending {
                    record.status = CommissionStatus::Settled;
                }
            }
        });
        Self::deposit_event(Event::OrderRecordsSettled { order_id });
        Ok(())
    }

    /// 购物余额支付的分佣处理
    ///
    /// 与 process_commission / process_token_commission 平行，但：
    /// - 无平台费（购物余额不收 platform fee）
    /// - 无池 A（无招商推荐人奖金）
    /// - 资金来源是 Entity 国库（不是 seller reserve）
    /// - 无需链上转账（资金始终在 Entity 账户内）
    pub(crate) fn process_shopping_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        shopping_amount: BalanceOf<T>,
        product_id: u64,
    ) -> DispatchResult {
        // Phase 0: 安全检查
        if GlobalCommissionPaused::<T>::get() {
            return Ok(());
        }
        if OrderShoppingCommissionProcessed::<T>::get(order_id) {
            return Ok(());
        }

        // Phase 1: 配置检查
        if shopping_amount.is_zero() {
            return Ok(());
        }
        let config = match CommissionConfigs::<T>::get(entity_id).filter(|c| c.enabled) {
            Some(c) => c,
            None => return Ok(()),
        };

        let entity_account = T::EntityProvider::entity_account(entity_id);
        let now = <frame_system::Pallet<T>>::block_number();
        // Use dedicated shopping stats to avoid cross-pipeline order_count pollution with NEX.
        // 使用独立的购物余额统计，防止与 NEX 管线的 order_count 相互污染。
        let buyer_stats = MemberShoppingCommissionStats::<T>::get(entity_id, buyer);
        let is_first_order = buyer_stats.order_count == 0;
        let enabled_modes = config.enabled_modes;

        let current_order_count = buyer_stats.order_count.saturating_add(1);
        MemberShoppingCommissionStats::<T>::mutate(entity_id, buyer, |stats| {
            stats.order_count = current_order_count;
        });

        // Phase 2: Entity 偿付能力检查
        let entity_balance = T::Currency::free_balance(&entity_account);
        let min_balance = T::Currency::minimum_balance();
        let existing_obligations = ShopPendingTotal::<T>::get(entity_id)
            .saturating_add(T::Loyalty::shopping_total(entity_id))
            .saturating_add(UnallocatedPool::<T>::get(entity_id))
            .saturating_add(PendingRefundTotal::<T>::get(entity_id));
        let min_threshold: BalanceOf<T> =
            T::FundProtectionQuery::min_treasury_threshold(entity_id).saturated_into();
        let available_for_commission = entity_balance
            .saturating_sub(existing_obligations)
            .saturating_sub(min_balance)
            .saturating_sub(min_threshold);

        // Phase 3: 计算分佣（仅 Pool B，无 Pool A）
        let budget_ceiling = Self::commission_budget_ceiling();
        let effective_rate = ProductCommissionRate::<T>::get(product_id)
            .or_else(|| ShopCommissionRate::<T>::get(shop_id))
            .unwrap_or(config.max_commission_rate)
            .min(budget_ceiling);
        let max_commission =
            shopping_amount.saturating_mul(effective_rate.into()) / 10000u32.into();
        let mut remaining = max_commission.min(available_for_commission);

        let mut total_distributed = BalanceOf::<T>::zero();

        if !remaining.is_zero() {
            let initial_remaining = remaining;
            let mut records_full = false;
            let mut dropped_outputs: u32 = 0;
            let mut dropped_amount = BalanceOf::<T>::zero();

            // NOTE: Shopping balance pipeline does NOT pay Owner rewards.
            // Owner rewards are exclusive to the NEX and Token pipelines where
            // real funds flow through escrow. Shopping balance is Entity-internal
            // accounting — paying Owner from entity_account would drain the
            // treasury with no offsetting income on refund.
            // 注意：购物余额管线不发放 Owner 奖励。Owner 奖励仅限 NEX 和 Token
            // 管线（有真实资金经过托管）。购物余额是 Entity 内部记账，若从
            // entity_account 转账给 Owner，退款时无法回收，会持续消耗资金池。

            let cap_base = shopping_amount;

            // 内联宏——处理单个插件的 outputs 循环
            macro_rules! credit_shopping_outputs {
                ($outputs:expr) => {
                    if !records_full {
                        for output in $outputs {
                            if Self::credit_shopping_commission(
                                entity_id,
                                shop_id,
                                order_id,
                                buyer,
                                &output.beneficiary,
                                output.amount,
                                output.commission_type,
                                output.level,
                                now,
                            )? {
                                // 成功
                            } else {
                                records_full = true;
                                dropped_outputs += 1;
                                dropped_amount = dropped_amount.saturating_add(output.amount);
                            }
                        }
                    } else {
                        for output in $outputs {
                            dropped_outputs += 1;
                            dropped_amount = dropped_amount.saturating_add(output.amount);
                        }
                    }
                };
            }

            // 1. Referral Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.referral_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::ReferralPlugin::calculate(
                    entity_id,
                    buyer,
                    shopping_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_shopping_outputs!(outputs);
            }

            // 2. MultiLevel Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.multi_level_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::MultiLevelPlugin::calculate(
                    entity_id,
                    buyer,
                    shopping_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_shopping_outputs!(outputs);
            }

            // 3. LevelDiff Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.level_diff_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::LevelDiffPlugin::calculate(
                    entity_id,
                    buyer,
                    shopping_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_shopping_outputs!(outputs);
            }

            // 4. SingleLine Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.single_line_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::SingleLinePlugin::calculate(
                    entity_id,
                    buyer,
                    shopping_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_shopping_outputs!(outputs);
            }

            // 5. Team Plugin
            if !records_full {
                let plugin_budget =
                    Self::capped_budget(config.plugin_caps.team_cap, cap_base, remaining);
                let (outputs, new_remaining) = T::TeamPlugin::calculate(
                    entity_id,
                    buyer,
                    shopping_amount,
                    plugin_budget,
                    enabled_modes,
                    is_first_order,
                    current_order_count,
                    order_id,
                );
                let outputs_sum = outputs.iter().fold(BalanceOf::<T>::zero(), |acc, o| {
                    acc.saturating_add(o.amount)
                });
                ensure!(
                    outputs_sum.saturating_add(new_remaining) == plugin_budget,
                    Error::<T>::PluginOutputInvariantViolation
                );
                remaining = remaining.saturating_sub(plugin_budget.saturating_sub(new_remaining));
                credit_shopping_outputs!(outputs);
            }

            if records_full {
                remaining = remaining.saturating_add(dropped_amount);
                Self::deposit_event(Event::CommissionRecordsCapacityExhausted {
                    entity_id,
                    order_id,
                    dropped_outputs,
                    dropped_amount,
                });
            }

            total_distributed = initial_remaining.saturating_sub(remaining);
        }

        // Phase 4: 沉淀池（资金已在 Entity 账户，纯记账）
        if enabled_modes.contains(CommissionModes::POOL_REWARD) && !remaining.is_zero() {
            UnallocatedPool::<T>::mutate(entity_id, |pool| {
                *pool = pool.saturating_add(remaining);
            });
            T::PoolFundingCallback::on_pool_funded(
                entity_id,
                FundingSource::OrderCommissionRemainder,
                remaining.saturated_into(),
                0,
                order_id,
            );
            OrderShoppingUnallocated::<T>::insert(order_id, (entity_id, shop_id, remaining));
            Self::deposit_event(Event::UnallocatedCommissionPooled {
                entity_id,
                order_id,
                amount: remaining,
            });
        }

        // Phase 5: 标记已处理
        OrderShoppingCommissionProcessed::<T>::insert(order_id, true);

        // Phase 6: 统计
        ShopCommissionTotals::<T>::mutate(entity_id, |(total, orders)| {
            *total = total.saturating_add(total_distributed);
            *orders = orders.saturating_add(1);
        });

        Ok(())
    }

    /// 购物余额分佣记账（与 credit_commission 平行，存储在 OrderShoppingCommissionRecords）
    pub(crate) fn credit_shopping_commission(
        entity_id: u64,
        shop_id: u64,
        order_id: u64,
        buyer: &T::AccountId,
        beneficiary: &T::AccountId,
        amount: BalanceOf<T>,
        commission_type: CommissionType,
        level: u16,
        now: BlockNumberFor<T>,
    ) -> Result<bool, DispatchError> {
        let record = CommissionRecord {
            entity_id,
            shop_id,
            order_id,
            buyer: buyer.clone(),
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
            status: CommissionStatus::Pending,
            created_at: now,
        };

        let pushed = OrderShoppingCommissionRecords::<T>::try_mutate(
            order_id,
            |records| -> Result<bool, DispatchError> {
                match records.try_push(record) {
                    Ok(()) => Ok(true),
                    Err(_) => Ok(false),
                }
            },
        )?;

        if !pushed {
            return Ok(false);
        }

        // 共享 MemberCommissionStats / ShopPendingTotal（与 NEX 佣金统一 pending）
        MemberCommissionStats::<T>::mutate(entity_id, beneficiary, |stats| {
            stats.total_earned = stats.total_earned.saturating_add(amount);
            stats.pending = stats.pending.saturating_add(amount);
        });

        MemberLastCredited::<T>::insert(entity_id, beneficiary, now);

        ShopPendingTotal::<T>::mutate(entity_id, |total| {
            *total = total.saturating_add(amount);
        });

        if matches!(
            commission_type,
            CommissionType::DirectReward
                | CommissionType::FirstOrder
                | CommissionType::RepeatPurchase
                | CommissionType::FixedAmount
        ) {
            ReferrerEarnedByBuyer::<T>::mutate((entity_id, beneficiary, buyer), |earned| {
                *earned = earned.saturating_add(amount);
            });
        }

        MemberCommissionOrderIds::<T>::mutate(entity_id, beneficiary, |ids| {
            if !ids.contains(&order_id) {
                if ids.try_push(order_id).is_err() {
                    if !ids.is_empty() {
                        ids.remove(0);
                    }
                    let _ = ids.try_push(order_id);
                }
            }
        });

        Self::deposit_event(Event::CommissionDistributed {
            entity_id,
            order_id,
            beneficiary: beneficiary.clone(),
            amount,
            commission_type,
            level,
        });

        Ok(true)
    }

    /// 取消购物余额分佣（纯记账回滚，无链上转账）
    ///
    /// 与 cancel_commission 的关键差异：
    /// - 无需链上转账（佣金本来就在 Entity 账户，"退回"只是统计回滚）
    /// - 无 PendingRefunds 机制（因为没有可能失败的转账）
    /// - 沉淀池回滚直接扣减（不需要转账）
    pub(crate) fn cancel_shopping_commission(order_id: u64) -> DispatchResult {
        let records = OrderShoppingCommissionRecords::<T>::get(order_id);

        let is_cancellable = |status: &CommissionStatus| {
            matches!(
                status,
                CommissionStatus::Pending | CommissionStatus::Settled
            )
        };

        let mut any_cancelled = false;
        let mut cancelled_outputs: alloc::vec::Vec<(T::AccountId, u128, CommissionType, u16)> =
            alloc::vec::Vec::new();

        // 回滚统计（纯记账，无需转账）
        OrderShoppingCommissionRecords::<T>::mutate(order_id, |records| {
            for record in records.iter_mut() {
                if is_cancellable(&record.status) {
                    MemberCommissionStats::<T>::mutate(
                        record.entity_id,
                        &record.beneficiary,
                        |stats| {
                            stats.pending = stats.pending.saturating_sub(record.amount);
                            stats.total_earned = stats.total_earned.saturating_sub(record.amount);
                        },
                    );
                    ShopPendingTotal::<T>::mutate(record.entity_id, |total| {
                        *total = total.saturating_sub(record.amount);
                    });
                    if matches!(
                        record.commission_type,
                        CommissionType::DirectReward
                            | CommissionType::FirstOrder
                            | CommissionType::RepeatPurchase
                            | CommissionType::FixedAmount
                    ) {
                        ReferrerEarnedByBuyer::<T>::mutate(
                            (record.entity_id, &record.beneficiary, &record.buyer),
                            |earned| {
                                *earned = earned.saturating_sub(record.amount);
                            },
                        );
                    }
                    let amount_u128: u128 = record.amount.try_into().unwrap_or(u128::MAX);
                    cancelled_outputs.push((
                        record.beneficiary.clone(),
                        amount_u128,
                        record.commission_type,
                        record.level,
                    ));
                    record.status = CommissionStatus::Cancelled;
                    any_cancelled = true;
                }
            }
        });

        // 退回沉淀池（纯记账）
        if let Some((unalloc_entity_id, _unalloc_shop_id, unalloc_amount)) =
            OrderShoppingUnallocated::<T>::get(order_id)
        {
            if !unalloc_amount.is_zero() {
                UnallocatedPool::<T>::mutate(unalloc_entity_id, |pool| {
                    *pool = pool.saturating_sub(unalloc_amount);
                });
                OrderShoppingUnallocated::<T>::remove(order_id);
            }
        }

        // No Owner reward to claw back — shopping balance pipeline does not pay Owner rewards.
        // 无需回收 Owner 奖励——购物余额管线不发放 Owner 奖励。

        // Roll back buyer order_count in the dedicated shopping stats table.
        // 从独立的购物余额统计回滚 order_count（与 process_shopping_commission 对称）。
        if any_cancelled {
            if let Some(first) = records.first() {
                MemberShoppingCommissionStats::<T>::mutate(
                    first.entity_id,
                    &first.buyer,
                    |stats| {
                        stats.order_count = stats.order_count.saturating_sub(1);
                    },
                );
            }
        }

        // 回滚插件内部统计
        if !cancelled_outputs.is_empty() {
            if let Some(first) = records.first() {
                T::MultiLevelStatsRollback::rollback_stats(
                    first.entity_id,
                    &cancelled_outputs,
                    true,
                );
            }
        }

        // Cleanup
        OrderShoppingCommissionProcessed::<T>::remove(order_id);

        let cancelled_count = cancelled_outputs.len() as u32;
        if cancelled_count > 0 {
            Self::deposit_event(Event::CommissionCancelled {
                order_id,
                refund_succeeded: cancelled_count,
                refund_failed: 0,
            });
        }

        Ok(())
    }

    /// 计算插件预算（NEX）：cap = 0 表示不启用（预算为 0），
    /// cap > 0 时取 min(remaining, order_amount × cap / 10000)。
    ///
    /// cap 与 max_commission_rate 同一量纲（bps of order_amount），
    /// Owner 可将总费率按需分配给各插件。
    /// remaining.min() 保证不超过实际可分配余额。
    #[inline]
    pub(crate) fn capped_budget(
        cap: u16,
        cap_base: BalanceOf<T>,
        remaining: BalanceOf<T>,
    ) -> BalanceOf<T> {
        if cap == 0 {
            BalanceOf::<T>::zero()
        } else {
            let cap_amount = cap_base.saturating_mul(cap.into()) / 10000u32.into();
            remaining.min(cap_amount)
        }
    }

    /// 计算插件预算（Token）：cap = 0 表示不启用（预算为 0），
    /// cap > 0 时取 min(remaining, token_order_amount × cap / 10000)。
    ///
    /// cap 与 max_commission_rate 同一量纲（bps of token_order_amount），
    /// remaining.min() 保证不超过实际可分配余额。
    #[inline]
    pub(crate) fn capped_token_budget(
        cap: u16,
        cap_base: TokenBalanceOf<T>,
        remaining: TokenBalanceOf<T>,
    ) -> TokenBalanceOf<T> {
        if cap == 0 {
            TokenBalanceOf::<T>::zero()
        } else {
            let cap_amount = cap_base.saturating_mul(cap.into()) / 10000u32.into();
            remaining.min(cap_amount)
        }
    }
}
