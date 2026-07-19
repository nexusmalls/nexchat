// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Shared types for Prediction community fee-commission (USDX ledger + NEX bond).
//! Prediction 社群手续费分佣共享类型（USDX 分账 + NEX 押金）。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{traits::AtLeast32BitUnsigned, RuntimeDebug};

/// Basis points denominator (100% = 10_000).
/// 万分比分母（100% = 10_000）。
pub const BPS_DENOM: u32 = 10_000;

/// Protocol total trade fee: 0.03 (300 bps).
/// 协议总交易费率：0.03（300 bps）。
pub const PROTOCOL_TRADE_FEE_BPS: u32 = 300;

/// Protocol top bar (creator + treasury): 0.01 (100 bps).
/// 协议顶栏（盘创建者 + 国库）：0.01（100 bps）。
pub const PROTOCOL_TOP_BPS: u32 = 100;

/// Protocol community commission side: 0.02 (200 bps).
/// 协议社群侧：0.02（200 bps）。
pub const PROTOCOL_COMMISSION_BPS: u32 = 200;

/// Deposit / trade-ticket split: single-line 50%.
/// 充值 / 成交票切分：公排 50%。
pub const SINGLE_LINE_BPS: u32 = 5_000;

/// Deposit / trade-ticket split: multi-level 47%.
/// 充值 / 成交票切分：助力 47%。
pub const MULTI_LEVEL_BPS: u32 = 4_700;

/// Deposit / trade-ticket split: community operator 3%.
/// 充值 / 成交票切分：社区运营账户 3%。
pub const COMMUNITY_OPERATOR_BPS: u32 = 300;

/// Registered community operator status.
/// 已登记社区运营账户状态。
#[derive(
	Clone,
	Copy,
	Decode,
	DecodeWithMemTracking,
	Encode,
	Eq,
	MaxEncodedLen,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
)]
pub enum CommunityStatus {
	Active,
	Unbonding,
	Suspended,
	Closed,
}

impl Default for CommunityStatus {
	fn default() -> Self {
		Self::Active
	}
}

/// Deposit or trade commission ticket awaiting SL/ML settlement (P3/P4).
/// 待公排/助力结算的充值或成交分佣票（P3/P4）。
#[derive(
	Clone,
	Decode,
	DecodeWithMemTracking,
	Encode,
	Eq,
	MaxEncodedLen,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
)]
pub struct CommissionTicket<AccountId, Balance> {
	pub who: AccountId,
	pub single_line: Balance,
	pub multi_level: Balance,
	/// MultiLevel (referral help) settled. / 动态助力已结算。
	pub ml_settled: bool,
	/// SingleLine settled (P4). / 公排已结算（P4）。
	pub sl_settled: bool,
}

/// PPT relative weights for 15 MultiLevel layers (sum = 50).
/// PPT 动态助力 15 层相对权重（之和 = 50）。
pub const ML_LEVEL_WEIGHTS: [u16; 15] =
	[6, 12, 2, 2, 2, 2, 4, 2, 2, 4, 2, 2, 4, 2, 2];

/// Sum of [`ML_LEVEL_WEIGHTS`].
/// [`ML_LEVEL_WEIGHTS`] 之和。
pub const ML_WEIGHT_SUM: u32 = 50;

/// Max MultiLevel depth (15 layers).
/// 动态助力最大深度（15 层）。
pub const ML_MAX_LEVELS: u8 = 15;

/// P1 activation threshold on `lifetime_trading_fee` (USDX units).
/// P1 激活阈值（`lifetime_trading_fee`，USDX）。
pub const TIER_P1_THRESHOLD: u32 = 50;

/// Lifetime-trading-fee thresholds for P1..=P7.
/// P1..=P7 的 `lifetime_trading_fee` 阈值。
pub const TIER_THRESHOLDS: [u32; 7] = [50, 100, 200, 300, 500, 2_000, 50_000];

/// Lookup package tier id from lifetime community-side trading fee.
/// Returns `0` if below P1, otherwise `1..=7`.
/// 按累计社群侧交易费查档位：低于 P1 返回 `0`，否则 `1..=7`。
pub fn lookup_tier_id<Balance>(lifetime_trading_fee: Balance) -> u8
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	let mut tier = 0u8;
	for (i, threshold) in TIER_THRESHOLDS.iter().enumerate() {
		if lifetime_trading_fee >= Balance::from(*threshold) {
			tier = (i as u8).saturating_add(1);
		} else {
			break;
		}
	}
	tier
}

/// Max MultiLevel layers a tier may claim (`0` if inactive).
/// 某档位可领取的助力层数（未激活为 `0`）。
pub fn max_help_levels(tier_id: u8) -> u8 {
	match tier_id {
		1..=4 => 6,
		5 => 9,
		6 => 12,
		7 => 15,
		_ => 0,
	}
}

/// Whether `lifetime_trading_fee` has reached P1 activation.
/// `lifetime_trading_fee` 是否已达 P1 激活。
pub fn is_activated<Balance>(lifetime_trading_fee: Balance) -> bool
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	lifetime_trading_fee >= Balance::from(TIER_P1_THRESHOLD)
}

/// Normalized MultiLevel payout for layer index `0..15` from `ml_budget`.
/// 将 `ml_budget` 按归一化权重分配到层索引 `0..15`。
pub fn ml_layer_share<Balance>(ml_budget: Balance, level_idx: usize) -> Balance
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	if level_idx >= ML_LEVEL_WEIGHTS.len() || ml_budget == Balance::from(0u8) {
		return Balance::from(0u8);
	}
	let w = Balance::from(ML_LEVEL_WEIGHTS[level_idx] as u32);
	ml_budget.saturating_mul(w) / Balance::from(ML_WEIGHT_SUM)
}

/// SingleLine tier row: `(base_up, base_down, downline_direct_gate)`.
/// 公排档位行：`(上线层数, 下线层数, 下线直推门槛)`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlTierLevels {
	pub base_up: u8,
	pub base_down: u8,
	/// Directs required before downline layers open (D2). `0` = always open.
	/// 开放下线层数所需直推数（D2）；`0` 表示无门槛。
	pub downline_direct_gate: u32,
}

/// P1..=P7 SingleLine level table from PPT (§3.2 / §7.2).
/// PPT 公排层数表（§3.2 / §7.2），对应 P1..=P7。
pub const SL_TIER_LEVELS: [SlTierLevels; 7] = [
	SlTierLevels { base_up: 20, base_down: 30, downline_direct_gate: 0 },
	SlTierLevels { base_up: 24, base_down: 36, downline_direct_gate: 0 },
	SlTierLevels { base_up: 28, base_down: 42, downline_direct_gate: 0 },
	SlTierLevels { base_up: 32, base_down: 48, downline_direct_gate: 0 },
	SlTierLevels { base_up: 40, base_down: 60, downline_direct_gate: 6 },
	SlTierLevels { base_up: 40, base_down: 60, downline_direct_gate: 9 },
	SlTierLevels { base_up: 40, base_down: 60, downline_direct_gate: 12 },
];

/// Resolve effective up/down layer counts for a payer (extra levels deferred = 0).
/// D2: if directs < gate, `effective_down = 0`.
/// 解析 payer 的有效上下线层数（额外层暂为 0）；直推不足门槛时下线为 0（D2）。
pub fn sl_effective_levels(tier_id: u8, direct_count: u32) -> (u8, u8) {
	if tier_id == 0 || tier_id as usize > SL_TIER_LEVELS.len() {
		return (0, 0);
	}
	let row = SL_TIER_LEVELS[(tier_id as usize).saturating_sub(1)];
	let effective_down = if direct_count < row.downline_direct_gate {
		0
	} else {
		row.base_down
	};
	(row.base_up, effective_down)
}

/// Split `sl_budget` into up/down halves and per-layer equal shares (§7.1).
/// Returns `(up_budget, down_budget, per_up, per_down)`.
/// When `effective_* == 0`, that side's `per_*` is 0 (whole side remains for pool).
/// 将 `sl_budget` 对半切开并做等额层份额（§7.1）。
/// `effective_* == 0` 时该侧 `per_*` 为 0（整侧进沉淀）。
pub fn sl_equal_split<Balance>(
	sl_budget: Balance,
	effective_up: u8,
	effective_down: u8,
) -> (Balance, Balance, Balance, Balance)
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	let up_budget = sl_budget / Balance::from(2u8);
	let down_budget = sl_budget.saturating_sub(up_budget);
	let per_up = if effective_up == 0 {
		Balance::from(0u8)
	} else {
		up_budget / Balance::from(effective_up as u32)
	};
	let per_down = if effective_down == 0 {
		Balance::from(0u8)
	} else {
		down_budget / Balance::from(effective_down as u32)
	};
	(up_budget, down_budget, per_up, per_down)
}

/// Cash bps for withdraw by tier (P1–P7); remainder is reinvest.
/// 按档位提现现金万分比（P1–P7）；其余复投。
pub fn withdraw_cash_bps(tier_id: u8) -> u32 {
	match tier_id {
		1..=4 => 4_000, // 40%
		5 => 5_000,     // 50%
		6 => 6_000,     // 60%
		7 => 7_000,     // 70%
		_ => 4_000,     // below P1: use P1 split
	}
}

/// Split withdraw amount into `(cash, reinvest)` by package tier (§10.2).
/// 按套餐档位将提现金额拆为 `(现金, 复投)`（§10.2）。
pub fn withdraw_split_by_tier<Balance>(tier_id: u8, amount: Balance) -> (Balance, Balance)
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	let cash = split_bps(amount, withdraw_cash_bps(tier_id));
	let reinvest = amount.saturating_sub(cash);
	(cash, reinvest)
}

/// Whether tier may claim pool rewards (P5–P7 only).
/// 是否可领取沉淀（仅 P5–P7）。
pub fn is_pool_eligible_tier(tier_id: u8) -> bool {
	matches!(tier_id, 5..=7)
}

/// Equal tier-pot share of a pool round for one of P5/P6/P7 (`pool / 3`).
/// Spec labels these 40%/40%/40%; equal thirds conserve the round budget.
/// 沉淀轮次中 P5/P6/P7 单档份额（`pool / 3`）；规格写 40%×3，等分以守恒。
pub fn pool_tier_pot<Balance>(pool_round: Balance) -> Balance
where
	Balance: AtLeast32BitUnsigned + Copy,
{
	pool_round / Balance::from(3u8)
}

/// Split a balance by bps with dust going to `remainder_sink` (caller merges into pool).
/// 按 bps 切分金额；尘埃由调用方并入沉淀池。
pub fn split_bps<Balance>(amount: Balance, bps: u32) -> Balance
where
	Balance: sp_runtime::traits::AtLeast32BitUnsigned + Copy,
{
	amount.saturating_mul(Balance::from(bps)) / Balance::from(BPS_DENOM)
}

/// Split deposit/ticket amount into SL / ML / operator shares (50/47/3).
/// Dust from rounding is assigned to `single_line` so the three parts sum to `amount`.
/// 将充值/票金额切为公排/助力/运营（50/47/3）；舍入尘埃并入公排以保证三者之和等于本金。
pub fn split_commission_budget<Balance>(amount: Balance) -> (Balance, Balance, Balance)
where
	Balance: sp_runtime::traits::AtLeast32BitUnsigned + Copy,
{
	let sl = split_bps(amount, SINGLE_LINE_BPS);
	let ml = split_bps(amount, MULTI_LEVEL_BPS);
	let op = split_bps(amount, COMMUNITY_OPERATOR_BPS);
	let used = sl.saturating_add(ml).saturating_add(op);
	let dust = amount.saturating_sub(used);
	(sl.saturating_add(dust), ml, op)
}

/// Split protocol top bar (0.01 of notional) into creator cut and treasury cut.
/// 将协议顶栏（notional 的 0.01）切为盘创建者与国库。
pub fn split_top_bar<Balance>(
	notional: Balance,
	creator_fee: sp_runtime::Perbill,
) -> (Balance, Balance)
where
	Balance: sp_runtime::traits::AtLeast32BitUnsigned + Copy,
{
	let top = split_bps(notional, PROTOCOL_TOP_BPS);
	let creator_cut = creator_fee.mul_floor(notional).min(top);
	let treasury_cut = top.saturating_sub(creator_cut);
	(creator_cut, treasury_cut)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::Perbill;

	#[test]
	fn split_commission_sums_to_amount() {
		for amount in [0u128, 1, 50, 100, 10_000, 999_999] {
			let (sl, ml, op) = split_commission_budget(amount);
			assert_eq!(sl + ml + op, amount);
		}
	}

	#[test]
	fn split_top_respects_bucket() {
		let notional = 1000u128;
		let (c, t) = split_top_bar(notional, Perbill::from_percent(1));
		assert_eq!(c + t, 10);
		assert_eq!(c, 10);
		let (c2, t2) = split_top_bar(notional, Perbill::from_perthousand(5));
		assert_eq!(c2 + t2, 10);
		assert_eq!(c2, 5);
		assert_eq!(t2, 5);
	}

	#[test]
	fn ml_weights_sum_and_normalize() {
		assert_eq!(ML_LEVEL_WEIGHTS.iter().map(|w| *w as u32).sum::<u32>(), ML_WEIGHT_SUM);
		let budget = 4_700u128;
		let mut sum = 0u128;
		for i in 0..15 {
			sum += ml_layer_share(budget, i);
		}
		// Integer division may leave dust < layer count.
		assert!(sum <= budget);
		assert_eq!(budget - sum, budget % ML_WEIGHT_SUM as u128);
		assert_eq!(lookup_tier_id(0u128), 0);
		assert_eq!(lookup_tier_id(50u128), 1);
		assert_eq!(lookup_tier_id(500u128), 5);
		assert_eq!(max_help_levels(1), 6);
		assert_eq!(max_help_levels(7), 15);
	}

	#[test]
	fn sl_levels_and_equal_split() {
		assert_eq!(sl_effective_levels(1, 0), (20, 30));
		assert_eq!(sl_effective_levels(5, 5), (40, 0)); // D2 gate
		assert_eq!(sl_effective_levels(5, 6), (40, 60));
		let (up, down, per_up, per_down) = sl_equal_split(100u128, 20, 30);
		assert_eq!(up + down, 100);
		assert_eq!(per_up, 2); // 50/20
		assert_eq!(per_down, 1); // 50/30 → 1
		let (_, _, pu0, _) = sl_equal_split(100u128, 0, 30);
		assert_eq!(pu0, 0);
	}

	#[test]
	fn withdraw_and_pool_helpers() {
		let (c, r) = withdraw_split_by_tier(5u8, 100u128);
		assert_eq!(c, 50);
		assert_eq!(r, 50);
		let (c1, r1) = withdraw_split_by_tier(1u8, 100u128);
		assert_eq!(c1, 40);
		assert_eq!(r1, 60);
		assert!(is_pool_eligible_tier(5));
		assert!(!is_pool_eligible_tier(4));
		assert_eq!(pool_tier_pot(300u128), 100);
	}
}
