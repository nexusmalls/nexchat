//! 提现计算子模块
//!
//! 从 lib.rs 提取的提现/复购/奖励分配相关函数：
//! - calc_withdrawal_split — NEX 提现分配计算
//! - calc_token_withdrawal_split — Token 提现分配计算
//! - ensure_owner_or_admin — 权限校验（Owner 或 Admin）
//! - ensure_entity_owner — 权限校验（仅 Owner）
//! - sweep_token_free_balance — Token 外部转入归集
//! - validate_plugin_caps_for_modes — H-1 插件预算 cap 校验
//! - commission_budget_ceiling — 佣金预算上限（10000 - platform_fee_rate）

use crate::pallet::*;
use frame_support::pallet_prelude::*;
use pallet_commission_common::{
    CommissionModes, MemberProvider, TokenTransferProvider, WithdrawalMode,
};
use pallet_entity_common::{
    AdminPermission, AutoRepurchasePort as _, EntityProvider, FeeConfigProvider as _,
    LoyaltyReadPort as _, PricingProvider as _,
};
use sp_runtime::traits::{Saturating, Zero};
use sp_runtime::SaturatedConversion;

impl<T: Config> Pallet<T> {
    /// HB-WD-01 mechanism 2: compensate a promoter after a cross-chain commission
    /// payout timed out. Called by the runtime's `PayoutRefundHandler` (wired to
    /// the bridge's `on_timeout`), it decodes `(entity_id, who, amount)` — the
    /// bridged `withdrawal` part recorded at dispatch — and restores it to the
    /// promoter's `pending` (and removes it from `withdrawn`), so the promoter can
    /// retry. The NEX itself is re-minted by the bridge back to the Entity account
    /// (mechanism 1), so this only repairs the off-chain-claimable ledger; the
    /// repurchase/bonus parts (which never left Nexus) are untouched.
    /// HB-WD-01 机制 2：跨链佣金派发超时后补偿推广员。由 runtime 的 `PayoutRefundHandler`
    ///（接桥 `on_timeout`）调用，解码 `(entity_id, who, amount)`——派发时记录的已桥出
    /// `withdrawal` 部分——并恢复到推广员 `pending`（并从 `withdrawn` 扣回），使其可重试。
    /// NEX 本身由桥铸回 Entity 账户（机制 1），故本函数仅修复可领记账；repurchase/bonus
    ///（从未离开 Nexus）不受影响。
    pub fn on_payout_timeout(meta: &[u8]) -> DispatchResult {
        let (entity_id, who, amount): (u64, T::AccountId, BalanceOf<T>) =
            Decode::decode(&mut &meta[..]).map_err(|_| Error::<T>::InvalidPayoutRefundMeta)?;
        MemberCommissionStats::<T>::mutate(entity_id, &who, |stats| {
            stats.pending = stats.pending.saturating_add(amount);
            stats.withdrawn = stats.withdrawn.saturating_sub(amount);
        });
        ShopPendingTotal::<T>::mutate(entity_id, |t| *t = t.saturating_add(amount));
        Self::deposit_event(Event::CommissionPayoutRefunded {
            entity_id,
            account: who,
            amount,
        });
        Ok(())
    }

    /// F1: 验证 Entity Owner 或 Admin(COMMISSION_MANAGE) 权限
    pub(crate) fn ensure_owner_or_admin(entity_id: u64, who: &T::AccountId) -> DispatchResult {
        let owner = T::EntityProvider::entity_owner(entity_id).ok_or(Error::<T>::EntityNotFound)?;
        if *who == owner {
            return Ok(());
        }
        ensure!(
            T::EntityProvider::is_entity_admin(entity_id, who, AdminPermission::COMMISSION_MANAGE),
            Error::<T>::NotEntityOwnerOrAdmin
        );
        Ok(())
    }

    /// 验证 Entity Owner（仅 Owner，不含 Admin — 用于资金提取等敏感操作）
    pub(crate) fn ensure_entity_owner(
        entity_id: u64,
        who: &T::AccountId,
    ) -> Result<(), DispatchError> {
        let owner = T::EntityProvider::entity_owner(entity_id).ok_or(Error::<T>::EntityNotFound)?;
        ensure!(*who == owner, Error::<T>::NotEntityOwner);
        Ok(())
    }

    /// 佣金预算上限 = 10000 - 平台费率
    ///
    /// 保证 max_commission_rate + platform_fee_rate ≤ 10000，
    /// 即卖家需支付的佣金 + 平台费不超过订单金额。
    pub(crate) fn commission_budget_ceiling() -> u16 {
        let platform_rate = T::PlatformFeeRate::platform_fee_rate();
        10000u16.saturating_sub(platform_rate)
    }

    /// 检测并归集外部直接转入 entity_account 的 Token 到沉淀池。
    ///
    /// `incoming`: 本次已知的合法入账金额（如 token_platform_fee），
    /// 在调用前已到达 entity_account，不应被视为外部转入。
    ///
    /// 原理：EntityTokenAccountedBalance 记录 entity_account 通过已知渠道应有的余额，
    /// actual_balance - (accounted + incoming) > 0 即为外部转入。
    ///
    /// RISK-2 审计修复: 首次调用时 EntityTokenAccountedBalance = None，
    /// 初始化为 actual - incoming（当前实际余额减去已知入账），
    /// 这意味着 Entity 启用佣金前的既有 Token 余额被视为已记账的合法资金，
    /// 不会被误吸入沉淀池。后续外部转入才会被归集。
    pub(crate) fn sweep_token_free_balance(entity_id: u64, incoming: TokenBalanceOf<T>) {
        let entity_account = T::EntityProvider::entity_account(entity_id);
        let actual = T::TokenTransferProvider::token_balance_of(entity_id, &entity_account);
        let accounted = EntityTokenAccountedBalance::<T>::get(entity_id)
            .unwrap_or_else(|| actual.saturating_sub(incoming));
        let expected = accounted.saturating_add(incoming);
        let external = actual.saturating_sub(expected);
        if !external.is_zero() {
            UnallocatedTokenPool::<T>::mutate(entity_id, |pool| {
                *pool = pool.saturating_add(external);
            });
            Self::deposit_event(Event::TokenUnallocatedPooled {
                entity_id,
                order_id: 0,
                amount: external,
            });
        }
        // 快照当前实际余额（含 incoming + external）
        EntityTokenAccountedBalance::<T>::insert(entity_id, actual);
    }

    /// H-1 审计修复: 验证插件预算 cap 与启用模式的一致性
    ///
    /// 规则：当 enabled_modes 中启用了 ≥2 个插件组时，
    /// 每个已启用插件组的 cap 必须 > 0（禁止 cap=0 全穿透）。
    ///
    /// 5 个插件组与 modes 映射：
    /// - Referral: DIRECT_REWARD | FIRST_ORDER | REPEAT_PURCHASE | FIXED_AMOUNT
    /// - MultiLevel: MULTI_LEVEL
    /// - LevelDiff: LEVEL_DIFF
    /// - SingleLine: SINGLE_LINE_UPLINE | SINGLE_LINE_DOWNLINE
    /// - Team: TEAM_PERFORMANCE
    ///
    /// 仅 1 个插件组时不要求设置 cap（单插件独占全部预算是合理的）。
    pub(crate) fn validate_plugin_caps_for_modes(
        modes: &CommissionModes,
        caps: &PluginBudgetCaps,
    ) -> DispatchResult {
        let referral_modes = CommissionModes::DIRECT_REWARD
            | CommissionModes::FIRST_ORDER
            | CommissionModes::REPEAT_PURCHASE
            | CommissionModes::FIXED_AMOUNT;

        let referral_on = modes.intersects(referral_modes);
        let multi_level_on = modes.contains(CommissionModes::MULTI_LEVEL);
        let level_diff_on = modes.contains(CommissionModes::LEVEL_DIFF);
        let single_line_on = modes.intersects(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let team_on = modes.contains(CommissionModes::TEAM_PERFORMANCE);

        // 任何插件启用都必须有对应的 cap > 0，否则引擎会分配 0 预算给该插件
        if referral_on {
            ensure!(
                caps.referral_cap > 0,
                Error::<T>::PluginCapRequiredForMultiPlugin
            );
        }
        if multi_level_on {
            ensure!(
                caps.multi_level_cap > 0,
                Error::<T>::PluginCapRequiredForMultiPlugin
            );
        }
        if level_diff_on {
            ensure!(
                caps.level_diff_cap > 0,
                Error::<T>::PluginCapRequiredForMultiPlugin
            );
        }
        if single_line_on {
            ensure!(
                caps.single_line_cap > 0,
                Error::<T>::PluginCapRequiredForMultiPlugin
            );
        }
        if team_on {
            ensure!(
                caps.team_cap > 0,
                Error::<T>::PluginCapRequiredForMultiPlugin
            );
        }

        Ok(())
    }

    /// 计算提现/复购/奖励分配（Entity 级，四种模式）
    ///
    /// 三层约束模型：
    /// ```text
    /// Governance 底线（强制）
    ///     ↓ max()
    /// Entity 模式设定（FullWithdrawal / FixedRate / LevelBased / MemberChoice）
    ///     ↓ max()
    /// 会员选择（MemberChoice 模式下的 requested_rate）
    ///     ↓
    /// 最终复购比率
    /// ```
    ///
    /// 自愿多复购奖励：超出强制最低线的部分 × voluntary_bonus_rate 额外计入购物余额
    pub(crate) fn calc_withdrawal_split(
        entity_id: u64,
        who: &T::AccountId,
        total_amount: BalanceOf<T>,
        requested_repurchase_rate: Option<u16>,
    ) -> WithdrawalSplit<BalanceOf<T>> {
        let zero = BalanceOf::<T>::zero();
        let config = WithdrawalConfigs::<T>::get(entity_id);

        // Step 1: 根据模式确定 Entity 层面的复购比率
        // - mandatory_base_rate: 模式强制最低线（不含 governance）
        // - mode_final_rate: 模式最终值（MemberChoice 允许高于 mandatory_base_rate）
        let (mandatory_base_rate, mode_final_rate, voluntary_bonus_rate) = match config {
            Some(ref config) if config.enabled => match &config.mode {
                WithdrawalMode::FullWithdrawal => (0u16, 0u16, config.voluntary_bonus_rate),
                WithdrawalMode::FixedRate { repurchase_rate } => (
                    *repurchase_rate,
                    *repurchase_rate,
                    config.voluntary_bonus_rate,
                ),
                WithdrawalMode::LevelBased => {
                    let level_id = T::MemberProvider::custom_level_id(entity_id, who);
                    let tier = config
                        .level_overrides
                        .iter()
                        .find(|(id, _)| *id == level_id)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(config.default_tier.clone());
                    (
                        tier.repurchase_rate,
                        tier.repurchase_rate,
                        config.voluntary_bonus_rate,
                    )
                }
                WithdrawalMode::MemberChoice {
                    min_repurchase_rate,
                } => {
                    let requested = requested_repurchase_rate
                        .unwrap_or(*min_repurchase_rate)
                        .min(10000);
                    let mode_rate = requested.max(*min_repurchase_rate);
                    (*min_repurchase_rate, mode_rate, config.voluntary_bonus_rate)
                }
            },
            _ => (0u16, 0u16, 0u16),
        };

        // Step 2: Governance 底线兜底
        let gov_min_rate = GlobalMinRepurchaseRate::<T>::get(entity_id);
        let mandatory_min_rate = mandatory_base_rate.max(gov_min_rate).min(10000);
        let final_repurchase_rate = mode_final_rate.max(gov_min_rate).min(10000);

        // Step 4: 计算金额
        let final_withdrawal_rate = 10000u16.saturating_sub(final_repurchase_rate);
        let withdrawal =
            total_amount.saturating_mul(final_withdrawal_rate.into()) / 10000u32.into();
        let repurchase = total_amount.saturating_sub(withdrawal);

        // Step 5: 计算自愿多复购奖励
        // 超出强制最低线的部分 × voluntary_bonus_rate
        let bonus = if voluntary_bonus_rate > 0 && final_repurchase_rate > mandatory_min_rate {
            let mandatory_repurchase =
                total_amount.saturating_mul(mandatory_min_rate.into()) / 10000u32.into();
            let voluntary_extra = repurchase.saturating_sub(mandatory_repurchase);
            voluntary_extra.saturating_mul(voluntary_bonus_rate.into()) / 10000u32.into()
        } else {
            zero
        };

        WithdrawalSplit {
            withdrawal,
            repurchase,
            bonus,
        }
    }

    /// Token 提现分配计算（与 NEX calc_withdrawal_split 对称，使用 Token 独立配置）
    pub(crate) fn calc_token_withdrawal_split(
        entity_id: u64,
        who: &T::AccountId,
        total_amount: TokenBalanceOf<T>,
        requested_repurchase_rate: Option<u16>,
    ) -> WithdrawalSplit<TokenBalanceOf<T>> {
        let zero = TokenBalanceOf::<T>::zero();
        let config = TokenWithdrawalConfigs::<T>::get(entity_id);

        let (mandatory_base_rate, mode_final_rate, voluntary_bonus_rate) = match config {
            Some(ref config) if config.enabled => match &config.mode {
                WithdrawalMode::FullWithdrawal => (0u16, 0u16, config.voluntary_bonus_rate),
                WithdrawalMode::FixedRate { repurchase_rate } => (
                    *repurchase_rate,
                    *repurchase_rate,
                    config.voluntary_bonus_rate,
                ),
                WithdrawalMode::LevelBased => {
                    let level_id = T::MemberProvider::custom_level_id(entity_id, who);
                    let tier = config
                        .level_overrides
                        .iter()
                        .find(|(id, _)| *id == level_id)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(config.default_tier.clone());
                    (
                        tier.repurchase_rate,
                        tier.repurchase_rate,
                        config.voluntary_bonus_rate,
                    )
                }
                WithdrawalMode::MemberChoice {
                    min_repurchase_rate,
                } => {
                    let requested = requested_repurchase_rate
                        .unwrap_or(*min_repurchase_rate)
                        .min(10000);
                    let mode_rate = requested.max(*min_repurchase_rate);
                    (*min_repurchase_rate, mode_rate, config.voluntary_bonus_rate)
                }
            },
            _ => (0u16, 0u16, 0u16),
        };

        // Governance 底线兜底（Token 独立配置）
        let gov_min_rate = GlobalMinTokenRepurchaseRate::<T>::get(entity_id);
        let mandatory_min_rate = mandatory_base_rate.max(gov_min_rate).min(10000);
        let final_repurchase_rate = mode_final_rate.max(gov_min_rate).min(10000);

        // 计算金额
        let final_withdrawal_rate = 10000u16.saturating_sub(final_repurchase_rate);
        let withdrawal =
            total_amount.saturating_mul(final_withdrawal_rate.into()) / 10000u32.into();
        let repurchase = total_amount.saturating_sub(withdrawal);

        // 自愿多复购奖励
        let bonus = if voluntary_bonus_rate > 0 && final_repurchase_rate > mandatory_min_rate {
            let mandatory_repurchase =
                total_amount.saturating_mul(mandatory_min_rate.into()) / 10000u32.into();
            let voluntary_extra = repurchase.saturating_sub(mandatory_repurchase);
            voluntary_extra.saturating_mul(voluntary_bonus_rate.into()) / 10000u32.into()
        } else {
            zero
        };

        WithdrawalSplit {
            withdrawal,
            repurchase,
            bonus,
        }
    }

    /// 检查购物余额是否达到复购门槛，达标时：
    /// 1. 记录 last_credited 时间戳（TTL 起点）
    /// 2. 若 auto_order=true，尝试链上自动下单 → 发 AutoRepurchaseCreated
    /// 3. 降级：发 RepurchaseReady 事件，由前端/服务监听后提交复购订单
    ///
    /// 仅在 `withdraw_commission` 中 `credit_shopping_balance` 之后调用。
    pub(crate) fn check_repurchase_ready(entity_id: u64, who: &T::AccountId) {
        let Some(config) = RepurchaseConfigs::<T>::get(entity_id) else {
            return;
        };
        if !config.enforced {
            return;
        }

        let shopping_bal = T::Loyalty::shopping_balance(entity_id, who);
        let nex_usdt_rate = T::PricingProvider::get_nex_usdt_price();
        let bal_nex: u128 = shopping_bal.saturated_into();
        let bal_usdt = pallet_commission_common::shopping_bal_to_usdt(bal_nex, nex_usdt_rate);

        if bal_usdt < config.min_package_usdt {
            return;
        }

        // 记录 last_credited（TTL 起点，仅在余额达到门槛时更新）
        let now = <frame_system::Pallet<T>>::block_number();
        MemberShoppingBalanceLastCredited::<T>::insert(entity_id, who, now);

        // 尝试链上自动下单
        if config.auto_order && config.default_product_id > 0 {
            match T::AutoRepurchase::try_place_repurchase_order(
                entity_id,
                who,
                config.default_product_id,
            ) {
                Ok(order_id) => {
                    // C-1 fix: 自动下单成功，购物余额已被消费，清除 TTL 起点
                    // C-1 fix: auto-order succeeded, shopping balance consumed — clear TTL marker
                    MemberShoppingBalanceLastCredited::<T>::remove(entity_id, who);
                    Self::deposit_event(Event::AutoRepurchaseCreated {
                        entity_id,
                        account: who.clone(),
                        order_id,
                        product_id: config.default_product_id,
                    });
                    return;
                }
                Err(_) => { /* 降级为发事件 */ }
            }
        }

        // 发 RepurchaseReady 事件（前端/服务监听后手动提交复购订单）
        Self::deposit_event(Event::RepurchaseReady {
            entity_id,
            account: who.clone(),
            shopping_balance: shopping_bal,
            usdt_equivalent: bal_usdt,
            min_package_usdt: config.min_package_usdt,
        });
    }
}
