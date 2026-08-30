//! Shared helpers for Ads / GroupRobot / Prediction retirement migrations.
//! Ads / GroupRobot / Prediction 退役迁移的共用辅助。
//!
//! Refunds must either fully succeed or abort the upgrade (panic), so
//! `RemovePallet` cannot run after a partial refund. try-runtime estimates the
//! whole package against 80% of max block weight; over-budget upgrades must be
//! split across spec versions (refunds first, prefix wipe later).
//! 退款必须整笔成功，否则中止升级（panic），避免 `RemovePallet` 在部分退款后执行。
//! try-runtime 按最大区块重量的 80% 估算整包；超预算时必须拆 spec（先退款，后清前缀）。

use frame_support::{
    traits::{Currency, ExistenceRequirement, ReservableCurrency},
    weights::{constants::RocksDbWeight, Weight},
};
use sp_runtime::traits::Zero;
#[cfg(any(test, feature = "try-runtime"))]
use sp_runtime::Perbill;

#[cfg(any(test, feature = "try-runtime"))]
use crate::configs::RuntimeBlockWeights;
use crate::{AccountId, Balance, Balances, EXISTENTIAL_DEPOSIT};
use sp_io::{hashing::twox_128, storage};

/// Fraction of max block weight the whole retirement package may consume.
/// 整包退役迁移允许占用的最大区块重量比例。
#[cfg(any(test, feature = "try-runtime"))]
pub const RETIREMENT_WEIGHT_BUDGET_PERCENT: u32 = 80;

/// 80% of `RuntimeBlockWeights::max_block`.
/// `RuntimeBlockWeights::max_block` 的 80%。
#[cfg(any(test, feature = "try-runtime"))]
pub fn retirement_weight_budget() -> Weight {
    Perbill::from_percent(RETIREMENT_WEIGHT_BUDGET_PERCENT) * RuntimeBlockWeights::get().max_block
}

/// Fails try-runtime / pre-checks when the estimated package exceeds the budget.
/// 估算重量超过预算时令 try-runtime / 预检查失败。
#[cfg(any(test, feature = "try-runtime"))]
pub fn ensure_weight_fits(estimated: Weight) -> Result<(), &'static str> {
    let budget = retirement_weight_budget();
    if estimated.ref_time() > budget.ref_time() || estimated.proof_size() > budget.proof_size() {
        Err(
            "retirement weight exceeds 80% of max block; split refunds and RemovePallet across spec versions",
        )
    } else {
        Ok(())
    }
}

/// Twox128 pallet prefix (16 bytes).
/// Pallet 的 Twox128 前缀（16 字节）。
pub fn pallet_prefix(name: &str) -> [u8; 16] {
    twox_128(name.as_bytes())
}

/// On-chain FRAME `StorageVersion` key for a retired pallet name.
/// 已退役 pallet 名称对应的链上 FRAME `StorageVersion` 键。
pub fn storage_version_key(pallet_name: &str) -> [u8; 32] {
    frame_support::storage::storage_prefix(pallet_name.as_bytes(), b":__STORAGE_VERSION__:")
}

/// Counts keys under a raw prefix via `next_key`.
/// 通过 `next_key` 统计某原始前缀下的键数量。
pub fn count_keys_with_prefix(prefix: &[u8]) -> u64 {
    let mut count = 0u64;
    let mut key = prefix.to_vec();
    while let Some(next) = storage::next_key(&key) {
        if !next.starts_with(prefix) {
            break;
        }
        count = count.saturating_add(1);
        key = next;
    }
    count
}

/// Counts keys under a pallet prefix that are not the FRAME storage-version key.
/// 统计 pallet 前缀下、非 FRAME storage-version 的键数量。
pub fn count_non_version_keys(pallet_name: &str) -> u64 {
    let prefix = pallet_prefix(pallet_name);
    let version = storage_version_key(pallet_name);
    let mut count = 0u64;
    let mut key = prefix.to_vec();
    while let Some(next) = storage::next_key(&key) {
        if !next.starts_with(&prefix) {
            break;
        }
        if next.as_slice() != version.as_slice() {
            count = count.saturating_add(1);
        }
        key = next;
    }
    count
}

/// `RemovePallet`-style weight for deleting `keys` entries.
/// 按删除 `keys` 条估算的 `RemovePallet` 重量。
#[cfg(any(test, feature = "try-runtime"))]
pub fn wipe_weight_for_keys(keys: u64) -> Weight {
    RocksDbWeight::get().reads_writes(keys.saturating_add(1), keys)
}

/// Counts every leftover key under retired Ads / GroupRobot / Prediction prefixes.
/// 统计 Ads / GroupRobot / Prediction 退役前缀下的剩余键。
#[cfg(any(test, feature = "try-runtime"))]
pub fn count_all_retired_keys() -> u64 {
    super::retire_ads::pallet_names()
        .iter()
        .chain(super::retire_grouprobot::pallet_names().iter())
        .chain(super::retire_prediction::pallet_names().iter())
        .map(|name| count_keys_with_prefix(&pallet_prefix(name)))
        .fold(0u64, |acc, n| acc.saturating_add(n))
}

/// Estimated weight of refunds + idle check + prefix wipe.
/// 退款 + 空闲检查 + 前缀清除的估算重量。
#[cfg(any(test, feature = "try-runtime"))]
pub fn estimate_full_retirement_weight() -> Weight {
    super::retire_ads::estimated_refund_weight()
        .saturating_add(super::retire_grouprobot::estimated_refund_weight())
        .saturating_add(super::retire_prediction::estimated_idle_check_weight())
        .saturating_add(wipe_weight_for_keys(count_all_retired_keys()))
}

/// Unreserve `amount`, failing if any remainder stays reserved.
/// 解押 `amount`；若仍有剩余则失败。
pub fn unreserve_exact(who: &AccountId, amount: Balance) -> Result<Weight, &'static str> {
    if amount.is_zero() {
        return Ok(Weight::zero());
    }
    ensure_reserved(who, amount)?;
    let leftover = <Balances as ReservableCurrency<AccountId>>::unreserve(who, amount);
    if !leftover.is_zero() {
        return Err("unreserve left a remainder; reserved balance was insufficient");
    }
    Ok(RocksDbWeight::get().reads_writes(1, 1))
}

/// Transfer with `KeepAlive` so treasury / reward-pool accounts are not killed.
/// 使用 `KeepAlive` 转账，避免国库 / 奖励池账户被杀死。
pub fn transfer_keep_alive(
    from: &AccountId,
    to: &AccountId,
    amount: Balance,
) -> Result<Weight, &'static str> {
    if amount.is_zero() {
        return Ok(Weight::zero());
    }
    <Balances as Currency<AccountId>>::transfer(from, to, amount, ExistenceRequirement::KeepAlive)
        .map_err(|_| "KeepAlive transfer failed (insolvent source or would kill source)")?;
    Ok(RocksDbWeight::get().reads_writes(2, 2))
}

/// Source must retain at least `EXISTENTIAL_DEPOSIT` after paying `amount`.
/// 付款后源账户至少仍须保留 `EXISTENTIAL_DEPOSIT`。
pub fn ensure_keep_alive_source(who: &AccountId, amount: Balance) -> Result<(), &'static str> {
    if amount.is_zero() {
        return Ok(());
    }
    let free = <Balances as Currency<AccountId>>::free_balance(who);
    if free < amount.saturating_add(EXISTENTIAL_DEPOSIT) {
        Err("source cannot KeepAlive after paying retirement claimables")
    } else {
        Ok(())
    }
}

/// Account must already have at least `amount` reserved.
/// 账户已锁定余额必须至少为 `amount`。
pub fn ensure_reserved(who: &AccountId, amount: Balance) -> Result<(), &'static str> {
    if amount.is_zero() {
        return Ok(());
    }
    if <Balances as ReservableCurrency<AccountId>>::reserved_balance(who) < amount {
        Err("account reserved balance is below the retirement unreserve amount")
    } else {
        Ok(())
    }
}

/// Abort the runtime upgrade so later `RemovePallet` steps cannot run.
/// 中止 runtime 升级，使后续 `RemovePallet` 无法执行。
pub fn panic_refund(module: &str, err: &str) -> ! {
    log::error!(
        target: "runtime::retire",
        "{module} retirement refund failed: {err}"
    );
    panic!(
        "{module} retirement refund failed: {err}; refusing to mark retired so RemovePallet cannot run"
    );
}

/// Abort when a wipe is attempted before refunds / idle checks completed.
/// 在退款 / 空闲检查完成前尝试清存储时中止。
pub fn panic_blocked_wipe(module: &str) -> ! {
    log::error!(
        target: "runtime::retire",
        "{module} RemovePallet blocked: refund/idle assertion did not complete"
    );
    panic!("{module} RemovePallet blocked: refund/idle assertion did not complete");
}

/// Fails the upgrade (try-runtime and live) when prefix wipe exceeds the block budget.
/// 前缀清除超过区块预算时令升级失败（try-runtime 与主网）。
///
/// Live `on_runtime_upgrade` only re-checks the estimate; operators must still run
/// try-runtime on a mainnet snapshot before `setCode`.
/// 主网 `on_runtime_upgrade` 只复查估算；`setCode` 前仍须对主网快照跑 try-runtime。
pub struct AssertRetiredWipeFitsBlock;

impl frame_support::traits::OnRuntimeUpgrade for AssertRetiredWipeFitsBlock {
    fn on_runtime_upgrade() -> Weight {
        // Live counting would double-walk every retired key before RemovePallet.
        // The budget gate lives in try-runtime `pre_upgrade` (and in
        // `RetireAdsFunds::pre_upgrade` for the full package).
        // 主网再扫一遍会在 RemovePallet 前把退役键走两遍。预算闸门放在
        // try-runtime `pre_upgrade`（以及 `RetireAdsFunds::pre_upgrade` 的整包估算）。
        RocksDbWeight::get().reads(1)
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
        let estimated = estimate_full_retirement_weight();
        log::info!(
            target: "runtime::retire",
            "retirement package estimated weight ref_time={} proof_size={} keys={}",
            estimated.ref_time(),
            estimated.proof_size(),
            count_all_retired_keys()
        );
        ensure_weight_fits(estimated).map_err(sp_runtime::DispatchError::Other)?;
        Ok(alloc::vec::Vec::new())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXISTENTIAL_DEPOSIT;
    use codec::Encode;
    use frame_support::traits::Currency as CurrencyT;

    fn account(b: u8) -> AccountId {
        AccountId::new([b; 32])
    }

    #[test]
    fn unreserve_exact_rejects_a_shortfall() {
        sp_io::TestExternalities::default().execute_with(|| {
            let who = account(1);
            let _ = <Balances as CurrencyT<AccountId>>::deposit_creating(&who, EXISTENTIAL_DEPOSIT * 4);
            assert_eq!(
                <Balances as ReservableCurrency<AccountId>>::reserve(&who, EXISTENTIAL_DEPOSIT),
                Ok(())
            );
            assert!(unreserve_exact(&who, EXISTENTIAL_DEPOSIT * 2).is_err());
            assert_eq!(
                <Balances as ReservableCurrency<AccountId>>::reserved_balance(&who),
                EXISTENTIAL_DEPOSIT
            );
        });
    }

    #[test]
    fn empty_state_fits_retirement_weight_budget() {
        sp_io::TestExternalities::default().execute_with(|| {
            assert!(ensure_weight_fits(estimate_full_retirement_weight()).is_ok());
            assert_eq!(count_all_retired_keys(), 0);
        });
    }

    #[test]
    fn count_non_version_keys_ignores_storage_version() {
        sp_io::TestExternalities::default().execute_with(|| {
            let pallet = "PredictionTokens";
            sp_io::storage::set(&storage_version_key(pallet), &1u16.encode());
            assert_eq!(count_non_version_keys(pallet), 0);

            let mut extra = pallet_prefix(pallet).to_vec();
            extra.extend_from_slice(&[7u8; 16]);
            sp_io::storage::set(&extra, &[1]);
            assert_eq!(count_non_version_keys(pallet), 1);
        });
    }
}
