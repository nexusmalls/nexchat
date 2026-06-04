//! Benchmarking for pallet-entity-governance
//!
//! 全部 16 个 extrinsics 均有 benchmark。
//! 由于 governance pallet 依赖大量外部 trait（EntityProvider / TokenProvider / ShopProvider 等），
//! benchmark 通过直接写入存储来构造前置状态，绕过外部 pallet 依赖。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_support::BoundedVec;
use frame_system::pallet_prelude::BlockNumberFor;
use frame_system::RawOrigin;
use pallet::*;
use pallet_entity_common::GovernanceMode;
use sp_runtime::traits::{Saturating, Zero};

const ENTITY_ID: u64 = 1;

// ==================== Helper 函数 ====================

/// 在 test 环境下设置 mock 状态（benchmark 在 runtime 中运行时由 runtime 提供真实 provider）
fn setup_entity_for_bench<T: Config>(_entity_id: u64, _owner: &T::AccountId) {
    #[cfg(test)]
    {
        use codec::Encode;
        let bytes = _owner.encode();
        let _id = if bytes.len() >= 8 {
            u64::from_le_bytes(bytes[..8].try_into().unwrap())
        } else {
            0u64
        };
        // 在 test 环境下设置 token 和 entity 状态
        crate::mock::set_token_enabled(_entity_id, true);
        crate::mock::set_token_balance(_entity_id, _id, 200_000);
        crate::mock::set_product_price(1, 500);
        crate::mock::set_product_stock(1, 100);
    }
}

/// 构造 FullDAO 治理配置并写入存储
fn seed_fulldao_config<T: Config>(entity_id: u64) {
    let config = GovernanceConfig::<BlockNumberFor<T>> {
        mode: GovernanceMode::FullDAO,
        voting_period: T::VotingPeriod::get(),
        execution_delay: T::ExecutionDelay::get(),
        quorum_threshold: T::QuorumThreshold::get(),
        pass_threshold: T::PassThreshold::get(),
        proposal_threshold: 0,
        admin_veto_enabled: true,
    };
    GovernanceConfigs::<T>::insert(entity_id, config);
}

/// 构造一个 Voting 状态的提案并写入存储
fn seed_proposal<T: Config>(entity_id: u64, proposer: &T::AccountId) -> ProposalId {
    let proposal_id = NextProposalId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();
    let voting_end = now.saturating_add(T::VotingPeriod::get());

    let title: BoundedVec<u8, T::MaxTitleLength> = b"Benchmark Proposal"
        .to_vec()
        .try_into()
        .expect("title fits");

    let proposal = Proposal {
        id: proposal_id,
        entity_id,
        proposer: proposer.clone(),
        proposal_type: ProposalType::General {
            title_cid: BoundedVec::truncate_from(b"bench_action".to_vec()),
            content_cid: BoundedVec::truncate_from(b"bench_content".to_vec()),
        },
        title,
        description_cid: None,
        status: ProposalStatus::Voting,
        created_at: now,
        voting_start: now,
        voting_end,
        execution_time: None,
        yes_votes: Zero::zero(),
        no_votes: Zero::zero(),
        abstain_votes: Zero::zero(),
        voter_count: 0,
        snapshot_quorum: T::QuorumThreshold::get(),
        snapshot_pass: T::PassThreshold::get(),
        snapshot_execution_delay: T::ExecutionDelay::get(),
        snapshot_total_supply: 1_000_000u128.into(),
    };

    Proposals::<T>::insert(proposal_id, &proposal);
    EntityProposals::<T>::mutate(entity_id, |list| {
        let _ = list.try_push(proposal_id);
    });
    NextProposalId::<T>::put(proposal_id.saturating_add(1));

    proposal_id
}

/// 构造一个已通过（Passed）且可执行的提案
fn seed_passed_proposal<T: Config>(entity_id: u64, proposer: &T::AccountId) -> ProposalId {
    let proposal_id = NextProposalId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();
    let voting_end = now.saturating_sub(1u32.into());

    let title: BoundedVec<u8, T::MaxTitleLength> =
        b"Benchmark Passed".to_vec().try_into().expect("title fits");

    let proposal = Proposal {
        id: proposal_id,
        entity_id,
        proposer: proposer.clone(),
        proposal_type: ProposalType::General {
            title_cid: BoundedVec::truncate_from(b"bench_action".to_vec()),
            content_cid: BoundedVec::truncate_from(b"bench_content".to_vec()),
        },
        title,
        description_cid: None,
        status: ProposalStatus::Passed,
        created_at: now.saturating_sub(200u32.into()),
        voting_start: now.saturating_sub(200u32.into()),
        voting_end,
        execution_time: Some(now),
        yes_votes: 500_000u128.into(),
        no_votes: 100_000u128.into(),
        abstain_votes: Zero::zero(),
        voter_count: 5,
        snapshot_quorum: T::QuorumThreshold::get(),
        snapshot_pass: T::PassThreshold::get(),
        snapshot_execution_delay: T::ExecutionDelay::get(),
        snapshot_total_supply: 1_000_000u128.into(),
    };

    Proposals::<T>::insert(proposal_id, &proposal);
    EntityProposals::<T>::mutate(entity_id, |list| {
        let _ = list.try_push(proposal_id);
    });
    NextProposalId::<T>::put(proposal_id.saturating_add(1));

    proposal_id
}

/// 构造一个已执行（Executed）的终态提案，用于 cleanup benchmark
fn seed_executed_proposal<T: Config>(
    entity_id: u64,
    proposer: &T::AccountId,
    voter_count: u32,
) -> ProposalId {
    let proposal_id = NextProposalId::<T>::get();
    let now = frame_system::Pallet::<T>::block_number();

    let title: BoundedVec<u8, T::MaxTitleLength> = b"Benchmark Executed"
        .to_vec()
        .try_into()
        .expect("title fits");

    let proposal = Proposal {
        id: proposal_id,
        entity_id,
        proposer: proposer.clone(),
        proposal_type: ProposalType::General {
            title_cid: BoundedVec::truncate_from(b"bench_action".to_vec()),
            content_cid: BoundedVec::truncate_from(b"bench_content".to_vec()),
        },
        title,
        description_cid: None,
        status: ProposalStatus::Executed,
        created_at: now.saturating_sub(300u32.into()),
        voting_start: now.saturating_sub(300u32.into()),
        voting_end: now.saturating_sub(200u32.into()),
        execution_time: Some(now.saturating_sub(100u32.into())),
        yes_votes: 500_000u128.into(),
        no_votes: Zero::zero(),
        abstain_votes: Zero::zero(),
        voter_count,
        snapshot_quorum: T::QuorumThreshold::get(),
        snapshot_pass: T::PassThreshold::get(),
        snapshot_execution_delay: T::ExecutionDelay::get(),
        snapshot_total_supply: 1_000_000u128.into(),
    };

    Proposals::<T>::insert(proposal_id, &proposal);
    NextProposalId::<T>::put(proposal_id.saturating_add(1));

    // 种子 VoteRecords 和 VotingPowerSnapshot 用于 cleanup 测试
    for i in 0..voter_count {
        let voter: T::AccountId = frame_benchmarking::account("voter", i, 0);
        let record = VoteRecord {
            voter: voter.clone(),
            vote: VoteType::Yes,
            weight: 1_000u128.into(),
            voted_at: now.saturating_sub(250u32.into()),
        };
        VoteRecords::<T>::insert(proposal_id, &voter, record);
        VotingPowerSnapshot::<T>::insert(proposal_id, &voter, BalanceOf::<T>::from(1_000u128));
    }

    proposal_id
}

/// 为提案种子投票记录（用于 finalize/cancel/veto 的 worst-case 解锁代价）
fn seed_voters_for_proposal<T: Config>(proposal_id: ProposalId, entity_id: u64, count: u32) {
    let now = frame_system::Pallet::<T>::block_number();
    for i in 0..count {
        let voter: T::AccountId = frame_benchmarking::account("voter", i, 0);
        let weight: BalanceOf<T> = 1_000u128.into();
        let record = VoteRecord {
            voter: voter.clone(),
            vote: VoteType::Yes,
            weight,
            voted_at: now,
        };
        VoteRecords::<T>::insert(proposal_id, &voter, record);
        VotingPowerSnapshot::<T>::insert(proposal_id, &voter, weight);
        VoterTokenLocks::<T>::insert(proposal_id, &voter, ());
        GovernanceLockCount::<T>::insert(entity_id, &voter, 1u32);
        GovernanceLockAmount::<T>::insert(entity_id, &voter, weight);
    }

    // 更新提案的投票计数和票数
    Proposals::<T>::mutate(proposal_id, |maybe| {
        if let Some(p) = maybe {
            p.voter_count = count;
            p.yes_votes = (count as u128 * 1_000u128).into();
        }
    });
}

/// 为委托 benchmark 种子委托者
fn seed_delegators<T: Config>(entity_id: u64, delegate: &T::AccountId, count: u32) {
    let mut delegators = BoundedVec::<T::AccountId, T::MaxDelegatorsPerDelegate>::default();
    for i in 0..count {
        let delegator: T::AccountId = frame_benchmarking::account("delegator", i, 0);
        VoteDelegation::<T>::insert(entity_id, &delegator, delegate);
        let _ = delegators.try_push(delegator.clone());

        // 在 test 环境下设置委托者代币余额
        #[cfg(test)]
        {
            use codec::Encode;
            let bytes = delegator.encode();
            let id = if bytes.len() >= 8 {
                u64::from_le_bytes(bytes[..8].try_into().unwrap())
            } else {
                0u64
            };
            crate::mock::set_token_balance(entity_id, id, 10_000);
        }
    }
    DelegatedVoters::<T>::insert(entity_id, delegate, delegators);
}

#[benchmarks]
mod benches {
    use super::*;
    use sp_runtime::traits::Zero;

    // ==================== call_index(0): create_proposal ====================
    #[benchmark]
    fn create_proposal() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let title = b"Benchmark proposal title".to_vec();
        let proposal_type = ProposalType::<T::AccountId, BalanceOf<T>>::General {
            title_cid: BoundedVec::truncate_from(b"QmBenchAction".to_vec()),
            content_cid: BoundedVec::truncate_from(b"QmBenchContent".to_vec()),
        };

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            ENTITY_ID,
            proposal_type,
            title,
            None,
        );

        assert!(Proposals::<T>::contains_key(0));
    }

    // ==================== call_index(1): vote ====================
    // d = 委托者数量（worst case: MaxDelegatorsPerDelegate）
    #[benchmark]
    fn vote(d: Linear<0, 10>) {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposal_id = seed_proposal::<T>(ENTITY_ID, &caller);

        // 种子委托者
        if d > 0 {
            seed_delegators::<T>(ENTITY_ID, &caller, d);
        }

        // 设置 FirstHoldTime 以启用时间加权
        let now = frame_system::Pallet::<T>::block_number();
        FirstHoldTime::<T>::insert(ENTITY_ID, &caller, now.saturating_sub(500u32.into()));

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id, VoteType::Yes);

        assert!(VoteRecords::<T>::contains_key(
            proposal_id,
            &frame_benchmarking::whitelisted_caller::<T::AccountId>()
        ));
    }

    // ==================== call_index(2): finalize_voting ====================
    #[benchmark]
    fn finalize_voting() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposal_id = seed_proposal::<T>(ENTITY_ID, &caller);

        // 种子投票者（worst case: 解锁代币）
        seed_voters_for_proposal::<T>(proposal_id, ENTITY_ID, 100);

        // 推进到投票期结束后
        let voting_end = Proposals::<T>::get(proposal_id).unwrap().voting_end;
        frame_system::Pallet::<T>::set_block_number(voting_end.saturating_add(1u32.into()));

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id);

        let proposal = Proposals::<T>::get(proposal_id).unwrap();
        assert!(
            proposal.status == ProposalStatus::Passed || proposal.status == ProposalStatus::Failed
        );
    }

    // ==================== call_index(3): execute_proposal ====================
    #[benchmark]
    fn execute_proposal() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposal_id = seed_passed_proposal::<T>(ENTITY_ID, &caller);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id);

        let proposal = Proposals::<T>::get(proposal_id).unwrap();
        assert!(
            proposal.status == ProposalStatus::Executed
                || proposal.status == ProposalStatus::ExecutionFailed
        );
    }

    // ==================== call_index(4): cancel_proposal ====================
    #[benchmark]
    fn cancel_proposal() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposal_id = seed_proposal::<T>(ENTITY_ID, &caller);
        // 种子投票者（worst case: 解锁代币）
        seed_voters_for_proposal::<T>(proposal_id, ENTITY_ID, 100);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id);

        let proposal = Proposals::<T>::get(proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Cancelled);
    }

    // ==================== call_index(5): configure_governance ====================
    #[benchmark]
    fn configure_governance() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        // 不预设 FullDAO，从 None 开始配置
        GovernanceConfigs::<T>::remove(ENTITY_ID);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller),
            ENTITY_ID,
            GovernanceMode::FullDAO,
            Some(T::VotingPeriod::get()),
            Some(T::ExecutionDelay::get()),
            Some(10u8),   // quorum
            Some(50u8),   // pass
            Some(100u16), // proposal_threshold
            Some(true),   // admin_veto_enabled
        );

        assert!(GovernanceConfigs::<T>::contains_key(ENTITY_ID));
    }

    // ==================== call_index(10): lock_governance ====================
    #[benchmark]
    fn lock_governance() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), ENTITY_ID);

        assert!(GovernanceLocked::<T>::get(ENTITY_ID));
    }

    // ==================== call_index(11): cleanup_proposal ====================
    #[benchmark]
    fn cleanup_proposal() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);

        // 种子一个已执行的提案，带 500 个投票记录（worst case for clear_prefix）
        let proposal_id = seed_executed_proposal::<T>(ENTITY_ID, &caller, 500);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id);

        // 提案应被清理（或部分清理）
    }

    // ==================== call_index(12): delegate_vote ====================
    #[benchmark]
    fn delegate_vote() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let delegate: T::AccountId = frame_benchmarking::account("delegate", 0, 0);

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            ENTITY_ID,
            delegate.clone(),
        );

        assert!(VoteDelegation::<T>::contains_key(ENTITY_ID, &caller));
    }

    // ==================== call_index(13): undelegate_vote ====================
    #[benchmark]
    fn undelegate_vote() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let delegate: T::AccountId = frame_benchmarking::account("delegate", 0, 0);
        // 预设委托关系
        VoteDelegation::<T>::insert(ENTITY_ID, &caller, &delegate);
        DelegatedVoters::<T>::mutate(ENTITY_ID, &delegate, |voters| {
            let _ = voters.try_push(caller.clone());
        });

        #[extrinsic_call]
        _(RawOrigin::Signed(caller.clone()), ENTITY_ID);

        assert!(!VoteDelegation::<T>::contains_key(ENTITY_ID, &caller));
    }

    // ==================== call_index(9): veto_proposal ====================
    #[benchmark]
    fn veto_proposal() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposer: T::AccountId = frame_benchmarking::account("proposer", 0, 0);
        let proposal_id = seed_proposal::<T>(ENTITY_ID, &proposer);
        // 种子投票者（worst case: 解锁代币）
        seed_voters_for_proposal::<T>(proposal_id, ENTITY_ID, 100);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id);

        let proposal = Proposals::<T>::get(proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Cancelled);
    }

    // ==================== call_index(14): change_vote ====================
    #[benchmark]
    fn change_vote() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposal_id = seed_proposal::<T>(ENTITY_ID, &caller);

        // 预设投票记录
        let now = frame_system::Pallet::<T>::block_number();
        let weight: BalanceOf<T> = 10_000u128.into();
        let record = VoteRecord {
            voter: caller.clone(),
            vote: VoteType::Yes,
            weight,
            voted_at: now,
        };
        VoteRecords::<T>::insert(proposal_id, &caller, record);
        Proposals::<T>::mutate(proposal_id, |maybe| {
            if let Some(p) = maybe {
                p.yes_votes = weight;
                p.voter_count = 1;
            }
        });

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), proposal_id, VoteType::No);
    }

    // ==================== call_index(15): pause_governance ====================
    #[benchmark]
    fn pause_governance() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        // 确保未暂停
        GovernancePaused::<T>::remove(ENTITY_ID);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), ENTITY_ID);

        assert!(GovernancePaused::<T>::get(ENTITY_ID));
    }

    // ==================== call_index(16): resume_governance ====================
    #[benchmark]
    fn resume_governance() {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        // 预设暂停状态
        GovernancePaused::<T>::insert(ENTITY_ID, true);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), ENTITY_ID);

        assert!(!GovernancePaused::<T>::get(ENTITY_ID));
    }

    // ==================== call_index(17): batch_cancel_proposals ====================
    // p = 活跃提案数, v = 总投票者数
    #[benchmark]
    fn batch_cancel_proposals(p: Linear<1, 10>, v: Linear<0, 50>) {
        let caller: T::AccountId = frame_benchmarking::whitelisted_caller();
        setup_entity_for_bench::<T>(ENTITY_ID, &caller);
        seed_fulldao_config::<T>(ENTITY_ID);

        let proposer: T::AccountId = frame_benchmarking::account("proposer", 0, 0);
        let voters_per_proposal = if p > 0 { v / p } else { 0 };

        for _ in 0..p {
            let pid = seed_proposal::<T>(ENTITY_ID, &proposer);
            if voters_per_proposal > 0 {
                seed_voters_for_proposal::<T>(pid, ENTITY_ID, voters_per_proposal);
            }
        }

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), ENTITY_ID);

        assert!(EntityProposals::<T>::get(ENTITY_ID).is_empty());
    }

    // ==================== call_index(18): force_unlock_governance ====================
    #[benchmark]
    fn force_unlock_governance() {
        setup_entity_for_bench::<T>(
            ENTITY_ID,
            &frame_benchmarking::whitelisted_caller::<T::AccountId>(),
        );
        seed_fulldao_config::<T>(ENTITY_ID);

        // 预设锁定 + 暂停状态（worst case: 两个都要解除）
        GovernanceLocked::<T>::insert(ENTITY_ID, true);
        GovernancePaused::<T>::insert(ENTITY_ID, true);

        #[extrinsic_call]
        _(RawOrigin::Root, ENTITY_ID);

        assert!(!GovernanceLocked::<T>::get(ENTITY_ID));
        assert!(!GovernancePaused::<T>::get(ENTITY_ID));
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::ExtBuilder::build(), crate::mock::Test,);
}
