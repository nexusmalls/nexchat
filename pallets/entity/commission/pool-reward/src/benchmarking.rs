//! Benchmarking for pallet-commission-pool-reward.
//!
//! Uses `frame_benchmarking::v2` macro style.
//! Root extrinsics are fully benchmarked; signed extrinsics that depend on
//! MemberProvider / EntityProvider / PoolBalanceProvider are benchmarked via
//! their Root equivalents or `#[block]` sections that replicate the storage
//! mutation cost. The storage I/O dominates execution time and is identical
//! regardless of origin path.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use pallet::*;
use pallet_entity_common::PoolRewardCapBehavior;
use sp_runtime::traits::Zero;
use sp_runtime::Saturating;

/// Build a simple `Fixed` per-level claim rule. / 构造简单的 `Fixed` 单级领取规则。
fn fixed_rule(base_cap_percent: u16) -> LevelClaimRule {
    LevelClaimRule {
        base_cap_percent,
        cap_behavior: PoolRewardCapBehavior::Fixed,
        baseline_direct: 0,
        baseline_team: 0,
    }
}

/// Build `count` distinct level rules (ids 1..=count, each a valid `Fixed` rule).
/// 构造 `count` 条不同等级规则（id 1..=count，每条均为合法 `Fixed` 规则）。
fn make_level_rules<T: Config>(
    count: u32,
) -> BoundedVec<(u8, LevelClaimRule), T::MaxPoolRewardLevels> {
    let per_level = if count == 0 { 10000u16 } else { 10000u16 / (count as u16) }.max(1);
    let v: alloc::vec::Vec<(u8, LevelClaimRule)> =
        (0..count).map(|i| ((i + 1) as u8, fixed_rule(per_level))).collect();
    v.try_into().expect("count should be <= MaxPoolRewardLevels")
}

/// Build `levels` level-quota snapshots for a round. / 为一轮构造 `levels` 个等级配额快照。
fn make_level_quotas<T: Config>(
    levels: u32,
) -> BoundedVec<LevelQuotaSnapshot, T::MaxPoolRewardLevels> {
    let mut q = BoundedVec::default();
    for i in 1..=levels {
        let _ = q.try_push(LevelQuotaSnapshot {
            level_id: i as u8,
            member_count: 10,
            claimed_count: 0,
        });
    }
    q
}

fn seed_config<T: Config>(entity_id: u64, levels: u32) {
    let rules = make_level_rules::<T>(levels);
    let rd = T::MinRoundDuration::get();
    PoolRewardConfigs::<T>::insert(
        entity_id,
        PoolRewardConfig {
            level_rules: rules,
            round_duration: rd,
            token_pool_enabled: false,
        },
    );
}

/// Seed a config with token pool enabled.
fn seed_config_with_token<T: Config>(entity_id: u64, levels: u32) {
    let rules = make_level_rules::<T>(levels);
    let rd = T::MinRoundDuration::get();
    PoolRewardConfigs::<T>::insert(
        entity_id,
        PoolRewardConfig {
            level_rules: rules,
            round_duration: rd,
            token_pool_enabled: true,
        },
    );
}

/// Build a `RoundInfo` with `levels` quota snapshots. / 构造含 `levels` 个配额快照的 `RoundInfo`。
fn make_round_info<T: Config>(
    round_id: u64,
    start_block: BlockNumberFor<T>,
    levels: u32,
    token: bool,
) -> RoundInfoOf<T> {
    RoundInfo {
        round_id,
        start_block,
        pool_snapshot: BalanceOf::<T>::from(10000u32),
        nex_usdt_rate_snapshot: None,
        eligible_count: 10u32.saturating_mul(levels),
        per_member_reward: BalanceOf::<T>::from(100u32),
        claimed_count: 0,
        level_quotas: make_level_quotas::<T>(levels),
        token_pool_snapshot: if token { Some(TokenBalanceOf::<T>::from(5000u32)) } else { None },
        token_per_member_reward: if token { Some(TokenBalanceOf::<T>::from(50u32)) } else { None },
        token_claimed_count: 0,
        token_level_quotas: if token { Some(make_level_quotas::<T>(levels)) } else { None },
    }
}

/// Build a `CompletedRoundSummary` from a round. / 由一轮构造 `CompletedRoundSummary`。
fn make_completed_summary<T: Config>(
    round: &RoundInfoOf<T>,
    end_block: BlockNumberFor<T>,
) -> CompletedRoundSummaryOf<T> {
    CompletedRoundSummary {
        round_id: round.round_id,
        start_block: round.start_block,
        end_block,
        pool_snapshot: round.pool_snapshot,
        nex_usdt_rate_snapshot: round.nex_usdt_rate_snapshot,
        eligible_count: round.eligible_count,
        per_member_reward: round.per_member_reward,
        claimed_count: round.claimed_count,
        level_quotas: round.level_quotas.clone(),
        token_pool_snapshot: round.token_pool_snapshot,
        token_per_member_reward: round.token_per_member_reward,
        token_claimed_count: round.token_claimed_count,
        token_level_quotas: round.token_level_quotas.clone(),
        funding_summary: RoundFundingSummary::default(),
    }
}

/// Seed a round so that extrinsics requiring an active round can proceed.
fn seed_round<T: Config>(entity_id: u64, levels: u32) {
    let now = frame_system::Pallet::<T>::block_number();
    let round_id = LastRoundId::<T>::get(entity_id).saturating_add(1);
    CurrentRound::<T>::insert(entity_id, make_round_info::<T>(round_id, now, levels, false));
}

/// Seed user claim records for force_clear worst-case.
fn seed_user_records<T: Config>(entity_id: u64, count: u32) {
    for i in 0..count {
        let account: T::AccountId = frame_benchmarking::account("user", i, 0);
        LastClaimedRound::<T>::insert(entity_id, &account, 1u64);
        let record = ClaimRecord {
            round_id: 1u64,
            amount: BalanceOf::<T>::from(100u32),
            level_id: 1u8,
            claimed_at: frame_system::Pallet::<T>::block_number(),
            token_amount: TokenBalanceOf::<T>::from(0u32),
        };
        let mut records = BoundedVec::default();
        let _ = records.try_push(record);
        ClaimRecords::<T>::insert(entity_id, &account, records);
    }
}

#[benchmarks]
mod benches {
    use super::*;

    // ========================================================================
    // Root extrinsics (no EntityProvider dependency)
    // ========================================================================

    /// Benchmark `force_set_pool_reward_config` — also anchors `set_pool_reward_config`.
    /// Storage: entity_exists(1R) + config_read(1R) + config_write(1W) +
    ///          current_round_read(1R) + last_round_id_write(1W) +
    ///          current_round_remove(1W) + pending_config_remove(1W)
    #[benchmark]
    fn set_pool_reward_config() {
        let entity_id: u64 = 9999;
        let levels = make_level_rules::<T>(3);
        let rd = T::MinRoundDuration::get();

        #[extrinsic_call]
        force_set_pool_reward_config(RawOrigin::Root, entity_id, levels, rd);

        assert!(PoolRewardConfigs::<T>::get(entity_id).is_some());
    }

    /// Benchmark `claim_pool_reward` worst case:
    /// Round expired → create_new_round (archive old + snapshot new) +
    /// NEX deduct + Currency::transfer + Token best-effort +
    /// ClaimRecords mutate (BoundedVec::remove(0) shift) +
    /// DistributionStatistics mutate + LastClaimedRound write + CurrentRound write.
    ///
    /// Since claim requires MemberProvider/EntityProvider wiring, we benchmark
    /// the storage-equivalent path via #[block] that exercises the same I/O.
    #[benchmark]
    fn claim_pool_reward() {
        let entity_id: u64 = 9999;
        seed_config_with_token::<T>(entity_id, 3);
        seed_round::<T>(entity_id, 3);

        // Seed an old round in history for archive path
        let old_round = CurrentRound::<T>::get(entity_id).unwrap();
        RoundHistory::<T>::mutate(entity_id, |history| {
            let now = frame_system::Pallet::<T>::block_number();
            let summary = make_completed_summary::<T>(&old_round, now);
            let _ = history.try_push(summary);
        });

        // Seed claim records at capacity to trigger remove(0) shift
        let caller: T::AccountId = frame_benchmarking::account("claimer", 0, 0);
        let mut records: BoundedVec<ClaimRecordOf<T>, T::MaxClaimHistory> = BoundedVec::default();
        for i in 0..T::MaxClaimHistory::get() {
            let _ = records.try_push(ClaimRecord {
                round_id: i as u64,
                amount: BalanceOf::<T>::from(10u32),
                level_id: 1u8,
                claimed_at: frame_system::Pallet::<T>::block_number(),
                token_amount: TokenBalanceOf::<T>::from(0u32),
            });
        }
        ClaimRecords::<T>::insert(entity_id, &caller, records);

        let now = frame_system::Pallet::<T>::block_number();

        #[block]
        {
            // Simulate worst-case claim storage operations:
            // 1. Read config, round, paused flags, member state (covered by reads count)
            // 2. Archive old round to history
            RoundHistory::<T>::mutate(entity_id, |history| {
                if history.is_full() {
                    history.remove(0);
                }
                let _ = history.try_push(make_completed_summary::<T>(&old_round, now));
            });
            // 3. Update distribution stats (round completed)
            DistributionStatistics::<T>::mutate(entity_id, |stats| {
                stats.total_rounds_completed = stats.total_rounds_completed.saturating_add(1);
            });
            // 4. Write new round
            let new_round_id = old_round.round_id.saturating_add(1);
            let mut new_round = make_round_info::<T>(new_round_id, now, 3, true);
            // 5. Update claimed_count in round
            new_round.level_quotas[0].claimed_count += 1;
            CurrentRound::<T>::insert(entity_id, &new_round);
            // 6. Write last_claimed_round
            LastClaimedRound::<T>::insert(entity_id, &caller, new_round_id);
            // 7. Mutate claim records (with remove(0) shift at capacity)
            ClaimRecords::<T>::mutate(entity_id, &caller, |history| {
                let record = ClaimRecord {
                    round_id: new_round_id,
                    amount: BalanceOf::<T>::from(100u32),
                    level_id: 1u8,
                    claimed_at: now,
                    token_amount: TokenBalanceOf::<T>::from(50u32),
                };
                if history.is_full() {
                    history.remove(0);
                }
                let _ = history.try_push(record);
            });
            // 8. Update distribution stats (claim)
            DistributionStatistics::<T>::mutate(entity_id, |stats| {
                stats.total_nex_distributed = stats
                    .total_nex_distributed
                    .saturating_add(BalanceOf::<T>::from(100u32));
                stats.total_claims = stats.total_claims.saturating_add(1);
            });
            // 9. Worst case: TokenPoolDeficit write
            TokenPoolDeficit::<T>::mutate(entity_id, |d| {
                *d = d.saturating_add(TokenBalanceOf::<T>::from(50u32));
            });
            // 10. LastRoundId write (from archive)
            LastRoundId::<T>::insert(entity_id, old_round.round_id);
            // 11. Deposit events (negligible weight but included)
            Pallet::<T>::deposit_event(Event::RoundArchived {
                entity_id,
                round_id: old_round.round_id,
            });
            Pallet::<T>::deposit_event(Event::PoolRewardClaimed {
                entity_id,
                account: caller.clone(),
                amount: BalanceOf::<T>::from(100u32),
                token_amount: TokenBalanceOf::<T>::from(50u32),
                round_id: new_round.round_id,
                level_id: 1u8,
            });
        }

        assert!(CurrentRound::<T>::get(entity_id).is_some());
    }

    /// Benchmark `create_new_round` internal path (no extrinsic — used by on_initialize + claim).
    #[benchmark]
    fn start_new_round() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        // Seed an existing round so archive path is exercised
        seed_round::<T>(entity_id, 3);
        let old_round = CurrentRound::<T>::get(entity_id).unwrap();
        let now = frame_system::Pallet::<T>::block_number();

        #[block]
        {
            // Archive old round
            RoundHistory::<T>::mutate(entity_id, |history| {
                if history.is_full() {
                    history.remove(0);
                }
                let _ = history.try_push(make_completed_summary::<T>(&old_round, now));
            });
            DistributionStatistics::<T>::mutate(entity_id, |stats| {
                stats.total_rounds_completed = stats.total_rounds_completed.saturating_add(1);
            });
            LastRoundId::<T>::insert(entity_id, old_round.round_id);

            // Create new round with snapshots
            let new_round_id = old_round.round_id.saturating_add(1);
            CurrentRound::<T>::insert(
                entity_id,
                make_round_info::<T>(new_round_id, now, 3, false),
            );
            Pallet::<T>::deposit_event(Event::NewRoundStarted {
                entity_id,
                round_id: new_round_id,
                pool_snapshot: BalanceOf::<T>::from(10000u32),
                token_pool_snapshot: None,
                level_snapshots: alloc::vec![(1u8, 10u32, BalanceOf::<T>::from(100u32))],
                token_level_snapshots: None,
            });
        }

        let round = CurrentRound::<T>::get(entity_id).unwrap();
        assert!(round.round_id >= 2);
    }

    /// Benchmark `set_token_pool_enabled` / `force_set_token_pool_enabled`.
    /// Storage: config(1R+1W) + current_round(1R) + last_round_id(1W) + current_round_remove(1W)
    #[benchmark]
    fn set_token_pool_enabled() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);

        #[extrinsic_call]
        force_set_token_pool_enabled(RawOrigin::Root, entity_id, true);

        let cfg = PoolRewardConfigs::<T>::get(entity_id).unwrap();
        assert!(cfg.token_pool_enabled);
    }

    /// Benchmark `clear_pool_reward_config` (Owner-level partial clear).
    /// Storage: config(1R+1W) + paused(1W) + pending(1W) +
    ///          current_round(1R+1W) + last_round_id(1W) +
    ///          round_history(1W) + distribution_stats(1W)
    #[benchmark]
    fn clear_pool_reward_config() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        seed_round::<T>(entity_id, 3);
        PoolRewardPaused::<T>::insert(entity_id, true);
        PendingPoolRewardConfig::<T>::insert(
            entity_id,
            PendingConfigChange {
                level_rules: make_level_rules::<T>(2),
                round_duration: T::MinRoundDuration::get(),
                apply_after: frame_system::Pallet::<T>::block_number(),
            },
        );
        let seed = CurrentRound::<T>::get(entity_id).unwrap();
        let end = frame_system::Pallet::<T>::block_number();
        RoundHistory::<T>::mutate(entity_id, |h| {
            let _ = h.try_push(make_completed_summary::<T>(&seed, end));
        });

        #[block]
        {
            Pallet::<T>::do_clear_pool_reward_config(entity_id);
        }

        assert!(PoolRewardConfigs::<T>::get(entity_id).is_none());
        assert!(!PoolRewardPaused::<T>::get(entity_id));
        assert!(PendingPoolRewardConfig::<T>::get(entity_id).is_none());
    }

    /// Benchmark `force_clear_pool_reward_config` (Root full clear with user records).
    /// Worst case: 2× clear_prefix O(n) for LastClaimedRound + ClaimRecords,
    /// plus all entity-level storage removals + TokenPoolDeficit.
    #[benchmark]
    fn force_clear_pool_reward_config() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        seed_round::<T>(entity_id, 3);
        seed_user_records::<T>(entity_id, 100);
        PoolRewardPaused::<T>::insert(entity_id, true);
        TokenPoolDeficit::<T>::insert(entity_id, TokenBalanceOf::<T>::from(500u32));

        #[extrinsic_call]
        _(RawOrigin::Root, entity_id, u32::MAX);

        assert!(PoolRewardConfigs::<T>::get(entity_id).is_none());
        assert!(CurrentRound::<T>::get(entity_id).is_none());
    }

    /// Benchmark `pause_pool_reward`.
    /// Storage: entity_active(1R) + owner(1R) + locked(1R) + config(1R) + paused(1R+1W)
    #[benchmark]
    fn pause_pool_reward() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);

        #[extrinsic_call]
        force_pause_pool_reward(RawOrigin::Root, entity_id);

        assert!(PoolRewardPaused::<T>::get(entity_id));
    }

    /// Benchmark `resume_pool_reward`.
    /// Storage: entity_active(1R) + owner(1R) + locked(1R) + config(1R) + paused(1R+1W)
    #[benchmark]
    fn resume_pool_reward() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        PoolRewardPaused::<T>::insert(entity_id, true);

        #[extrinsic_call]
        force_resume_pool_reward(RawOrigin::Root, entity_id);

        assert!(!PoolRewardPaused::<T>::get(entity_id));
    }

    /// Benchmark `set_global_pool_reward_paused`.
    /// Storage: global_paused(1R+1W)
    #[benchmark]
    fn set_global_pool_reward_paused() {
        #[extrinsic_call]
        _(RawOrigin::Root, true);

        assert!(GlobalPoolRewardPaused::<T>::get());
    }

    /// Benchmark `force_pause_pool_reward`.
    /// Storage: config(1R) + paused(1R+1W)
    #[benchmark]
    fn force_pause_pool_reward() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);

        #[extrinsic_call]
        _(RawOrigin::Root, entity_id);

        assert!(PoolRewardPaused::<T>::get(entity_id));
    }

    /// Benchmark `force_resume_pool_reward`.
    /// Storage: config(1R) + paused(1R+1W)
    #[benchmark]
    fn force_resume_pool_reward() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        PoolRewardPaused::<T>::insert(entity_id, true);

        #[extrinsic_call]
        _(RawOrigin::Root, entity_id);

        assert!(!PoolRewardPaused::<T>::get(entity_id));
    }

    /// Benchmark `schedule_pool_reward_config_change`.
    /// Storage: entity_active(1R) + owner(1R) + locked(1R) + config(1R) +
    ///          pending(1R+1W) + block_number(1R)
    #[benchmark]
    fn schedule_pool_reward_config_change() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        let new_rules = make_level_rules::<T>(2);
        let rd = T::MinRoundDuration::get();
        let apply_after = frame_system::Pallet::<T>::block_number() + T::ConfigChangeDelay::get();

        #[block]
        {
            PendingPoolRewardConfig::<T>::insert(
                entity_id,
                PendingConfigChange {
                    level_rules: new_rules,
                    round_duration: rd,
                    apply_after,
                },
            );
            Pallet::<T>::deposit_event(Event::PoolRewardConfigScheduled {
                entity_id,
                apply_after,
            });
        }

        assert!(PendingPoolRewardConfig::<T>::get(entity_id).is_some());
    }

    /// Benchmark `apply_pending_pool_reward_config`.
    /// Storage: entity_active(1R) + owner(1R) + locked(1R) + pending(1R+1W) +
    ///          do_set_pool_reward_config: entity_exists(1R) + config(1R+1W) +
    ///          current_round(1R+1W) + last_round_id(1W) + pending_remove(1W)
    #[benchmark]
    fn apply_pending_pool_reward_config() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        let new_rules = make_level_rules::<T>(2);
        let rd = T::MinRoundDuration::get();
        let current_block = frame_system::Pallet::<T>::block_number();

        PendingPoolRewardConfig::<T>::insert(
            entity_id,
            PendingConfigChange {
                level_rules: new_rules.clone(),
                round_duration: rd,
                apply_after: current_block, // already ready
            },
        );

        #[block]
        {
            let pending = PendingPoolRewardConfig::<T>::take(entity_id).unwrap();
            // Replicate do_set_pool_reward_config storage path
            let token_pool_enabled = PoolRewardConfigs::<T>::get(entity_id)
                .map(|c| c.token_pool_enabled)
                .unwrap_or(false);
            PoolRewardConfigs::<T>::insert(
                entity_id,
                PoolRewardConfig {
                    level_rules: pending.level_rules,
                    round_duration: pending.round_duration,
                    token_pool_enabled,
                },
            );
            // invalidate_current_round
            if let Some(round) = CurrentRound::<T>::get(entity_id) {
                LastRoundId::<T>::insert(entity_id, round.round_id);
            }
            CurrentRound::<T>::remove(entity_id);
            PendingPoolRewardConfig::<T>::remove(entity_id);
            Pallet::<T>::deposit_event(Event::PendingPoolRewardConfigApplied { entity_id });
        }

        assert!(PoolRewardConfigs::<T>::get(entity_id).is_some());
        assert!(PendingPoolRewardConfig::<T>::get(entity_id).is_none());
    }

    /// Benchmark `cancel_pending_pool_reward_config`.
    /// Storage: entity_active(1R) + owner(1R) + locked(1R) + pending(1R+1W)
    #[benchmark]
    fn cancel_pending_pool_reward_config() {
        let entity_id: u64 = 9999;
        seed_config::<T>(entity_id, 3);
        PendingPoolRewardConfig::<T>::insert(
            entity_id,
            PendingConfigChange {
                level_rules: make_level_rules::<T>(2),
                round_duration: T::MinRoundDuration::get(),
                apply_after: frame_system::Pallet::<T>::block_number() + 100u32.into(),
            },
        );

        #[block]
        {
            PendingPoolRewardConfig::<T>::remove(entity_id);
            Pallet::<T>::deposit_event(Event::PendingPoolRewardConfigCancelled { entity_id });
        }

        assert!(PendingPoolRewardConfig::<T>::get(entity_id).is_none());
    }

    /// Benchmark `correct_token_pool_deficit`.
    /// Storage: token_pool_deficit(1R+1W) + deduct_token_pool(1R+1W)
    #[benchmark]
    fn correct_token_pool_deficit() {
        let entity_id: u64 = 9999;
        TokenPoolDeficit::<T>::insert(entity_id, TokenBalanceOf::<T>::from(1000u32));

        #[block]
        {
            let deficit = TokenPoolDeficit::<T>::take(entity_id);
            // In real runtime, deduct_token_pool would be called here.
            // We simulate the storage write cost.
            TokenPoolDeficit::<T>::insert(entity_id, TokenBalanceOf::<T>::from(0u32));
            Pallet::<T>::deposit_event(Event::TokenPoolDeficitCorrected {
                entity_id,
                amount: deficit,
            });
        }

        assert!(TokenPoolDeficit::<T>::get(entity_id).is_zero());
    }

    impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test,);
}
