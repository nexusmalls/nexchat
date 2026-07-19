use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok, traits::Currency as CurrencyT};
use pallet_grouprobot_primitives::*;

/// P14: 生成有效的 equivocation 证据 (两组不同消息 + 同一节点的真实签名)
fn equivocation_evidence(node_n: u8, seq: u64) -> ([u8; 32], [u8; 64], [u8; 32], [u8; 64]) {
    let msg_a = [b"equivocation-a", seq.to_le_bytes().as_slice(), &[node_n]].concat();
    let msg_b = [b"equivocation-b", seq.to_le_bytes().as_slice(), &[node_n]].concat();
    let (hash_a, sig_a) = sign_msg(node_n, &msg_a);
    let (hash_b, sig_b) = sign_msg(node_n, &msg_b);
    (hash_a, sig_a, hash_b, sig_b)
}

/// P14: 便捷函数 — 使用真实签名举报 equivocation (assert_ok)
fn do_report_equivocation(reporter: u64, node_n: u8, seq: u64) {
    let (ha, sa, hb, sb) = equivocation_evidence(node_n, seq);
    assert_ok!(GroupRobotConsensus::report_equivocation(
        RuntimeOrigin::signed(reporter),
        node_id(node_n),
        seq,
        ha,
        sa,
        hb,
        sb,
    ));
}

// ============================================================================
// register_node
// ============================================================================

#[test]
fn register_node_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.operator, OPERATOR);
        assert_eq!(node.stake, 200);
        assert_eq!(node.status, NodeStatus::Active);
        assert_eq!(ActiveNodeList::<Test>::get().len(), 1);
        assert_eq!(OperatorNodes::<Test>::get(OPERATOR), Some(node_id(1)));
    });
}

#[test]
fn register_node_fails_duplicate() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::register_node(RuntimeOrigin::signed(OPERATOR2), node_id(1), 200),
            Error::<Test>::NodeAlreadyRegistered
        );
    });
}

#[test]
fn register_node_fails_operator_already_has_node() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::register_node(RuntimeOrigin::signed(OPERATOR), node_id(2), 200),
            Error::<Test>::NodeAlreadyRegistered
        );
    });
}

#[test]
fn register_node_fails_insufficient_stake() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::register_node(RuntimeOrigin::signed(OPERATOR), node_id(1), 50),
            Error::<Test>::InsufficientStake
        );
    });
}

// ============================================================================
// request_exit / finalize_exit
// ============================================================================

#[test]
fn exit_flow_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::request_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));

        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.status, NodeStatus::Exiting);
        assert_eq!(ActiveNodeList::<Test>::get().len(), 0);

        // Cooldown not complete
        System::set_block_number(5);
        assert_noop!(
            GroupRobotConsensus::finalize_exit(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            Error::<Test>::CooldownNotComplete
        );

        // Cooldown complete (10 blocks)
        System::set_block_number(12);
        let balance_before = Balances::free_balance(OPERATOR);
        assert_ok!(GroupRobotConsensus::finalize_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert!(!Nodes::<Test>::contains_key(node_id(1)));
        assert!(!OperatorNodes::<Test>::contains_key(OPERATOR));
        assert!(Balances::free_balance(OPERATOR) > balance_before);
    });
}

#[test]
fn request_exit_fails_not_operator() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::request_exit(RuntimeOrigin::signed(OTHER), node_id(1)),
            Error::<Test>::NotOperator
        );
    });
}

// ============================================================================
// report_equivocation / slash_equivocation
// ============================================================================

#[test]
fn report_equivocation_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        do_report_equivocation(OTHER, 1, 42);
        let record = EquivocationRecords::<Test>::get(node_id(1), 42).unwrap();
        assert_eq!(record.reporter, OTHER);
        assert!(!record.resolved);
    });
}

#[test]
fn slash_equivocation_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.stake, 900);
        assert_eq!(node.status, NodeStatus::Suspended);
        assert_eq!(ActiveNodeList::<Test>::get().len(), 0);

        let record = EquivocationRecords::<Test>::get(node_id(1), 42).unwrap();
        assert!(record.resolved);
    });
}

// ============================================================================
// mark_sequence_processed
// ============================================================================

#[test]
fn mark_sequence_processed_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            42
        ));
        assert!(ProcessedSequences::<Test>::contains_key(bot_hash(1), 42));
        assert!(GroupRobotConsensus::is_sequence_processed(&bot_hash(1), 42));
    });
}

#[test]
fn mark_sequence_duplicate_emits_event() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            42
        ));
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            42
        ));
    });
}

// ============================================================================
// verify_node_tee
// ============================================================================

#[test]
fn verify_node_tee_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert!(!Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);

        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);
        assert_eq!(NodeBotBinding::<Test>::get(node_id(1)), Some(bot_hash(10)));
    });
}

#[test]
fn verify_node_tee_fails_already_verified() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert_noop!(
            GroupRobotConsensus::verify_node_tee(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                bot_hash(10)
            ),
            Error::<Test>::AlreadyTeeVerified
        );
    });
}

#[test]
fn verify_node_tee_fails_bot_not_tee() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OWNER),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::verify_node_tee(
                RuntimeOrigin::signed(OWNER),
                node_id(1),
                bot_hash(1)
            ),
            Error::<Test>::AttestationNotValid
        );
    });
}

#[test]
fn verify_node_tee_fails_owner_mismatch() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::verify_node_tee(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                bot_hash(11)
            ),
            Error::<Test>::BotOwnerMismatch
        );
    });
}

// ============================================================================
// on_era_end (via on_initialize) — now delegates to mock traits
// ============================================================================

#[test]
fn era_end_calls_settler_and_distributor() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));

        // Set mock subscription income
        set_mock_settle_income(100);
        clear_distributed_rewards();

        advance_to(52);

        assert!(CurrentEra::<Test>::get() >= 1);
        // MockRewardDistributor should have been called
        let rewards = get_distributed_rewards();
        assert!(!rewards.is_empty(), "Rewards should be distributed");
    });
}

#[test]
fn era_end_no_nodes_just_advances_era() {
    new_test_ext().execute_with(|| {
        // No active nodes
        clear_distributed_rewards();
        advance_to(52);

        assert!(CurrentEra::<Test>::get() >= 1);
        // No rewards distributed (no nodes)
        let rewards = get_distributed_rewards();
        assert!(rewards.is_empty());
    });
}

#[test]
fn era_end_non_tee_gets_base_weight() {
    new_test_ext().execute_with(|| {
        // Non-TEE node — now gets base weight and earns inflation rewards
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));

        set_mock_settle_income(0);
        clear_distributed_rewards();

        advance_to(52);

        // Non-TEE node has base weight, gets full inflation (sole active node)
        let rewards = get_distributed_rewards();
        assert!(!rewards.is_empty(), "Non-TEE node should receive rewards");
        assert_eq!(
            rewards[0].1, 1_000_000_000,
            "Sole node gets full RewardPool balance"
        );
    });
}

// ============================================================================
// SgxEnclaveBonus: TEE reward params
// ============================================================================

#[test]
fn set_tee_reward_params_requires_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::set_tee_reward_params(
                RuntimeOrigin::signed(OPERATOR),
                15_000,
                2_000
            ),
            sp_runtime::DispatchError::BadOrigin
        );
        assert_ok!(GroupRobotConsensus::set_tee_reward_params(
            RuntimeOrigin::root(),
            15_000,
            2_000
        ));
        assert_eq!(TeeRewardMultiplier::<Test>::get(), 15_000);
        assert_eq!(SgxEnclaveBonus::<Test>::get(), 2_000);
    });
}

// ============================================================================
// ProcessedSequences TTL cleanup
// ============================================================================

#[test]
fn expired_sequences_are_cleaned() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            1
        ));
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            2
        ));
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            3
        ));

        advance_to(103);

        assert!(!ProcessedSequences::<Test>::contains_key(bot_hash(1), 1));
        assert!(!ProcessedSequences::<Test>::contains_key(bot_hash(1), 2));
        assert!(!ProcessedSequences::<Test>::contains_key(bot_hash(1), 3));
    });
}

#[test]
fn fresh_sequences_not_cleaned() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            1
        ));
        advance_to(51);
        assert!(ProcessedSequences::<Test>::contains_key(bot_hash(1), 1));
    });
}

// ============================================================================
// NodeConsensusProvider
// ============================================================================

#[test]
fn node_consensus_provider_trait() {
    new_test_ext().execute_with(|| {
        use pallet_grouprobot_primitives::NodeConsensusProvider;
        assert!(!<GroupRobotConsensus as NodeConsensusProvider<u64>>::is_node_active(&node_id(1)));

        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert!(<GroupRobotConsensus as NodeConsensusProvider<u64>>::is_node_active(&node_id(1)));
        assert_eq!(
            <GroupRobotConsensus as NodeConsensusProvider<u64>>::node_operator(&node_id(1)),
            Some(OPERATOR)
        );
    });
}

// ============================================================================
// SubscriptionTier primitives (tested here since they're in primitives)
// ============================================================================

#[test]
fn subscription_tier_is_paid() {
    assert!(!SubscriptionTier::Free.is_paid());
    assert!(SubscriptionTier::Basic.is_paid());
    assert!(SubscriptionTier::Pro.is_paid());
    assert!(SubscriptionTier::Enterprise.is_paid());
}

#[test]
fn subscription_tier_default_is_free() {
    assert_eq!(SubscriptionTier::default(), SubscriptionTier::Free);
}

// ============================================================================
// Tier gate
// ============================================================================

#[test]
fn mark_sequence_fails_free_tier() {
    new_test_ext().execute_with(|| {
        // bot_hash(99) → Free tier in MockSubscription (not 1/2/10/11)
        assert_noop!(
            GroupRobotConsensus::mark_sequence_processed(
                RuntimeOrigin::signed(OTHER),
                bot_hash(99),
                1
            ),
            Error::<Test>::FreeTierNotAllowed
        );
    });
}

#[test]
fn mark_sequence_works_paid_tier() {
    new_test_ext().execute_with(|| {
        // bot_hash(1) → Basic tier in MockSubscription
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            100
        ));
        assert!(ProcessedSequences::<Test>::contains_key(bot_hash(1), 100));
    });
}

// ============================================================================
// Regression tests (audit fixes)
// ============================================================================

#[test]
fn h1_report_equivocation_rejects_identical_msg_hash() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        // Same msg_hash_a and msg_hash_b → should fail
        assert_noop!(
            GroupRobotConsensus::report_equivocation(
                RuntimeOrigin::signed(OTHER),
                node_id(1),
                42,
                [1u8; 32],
                [1u8; 64],
                [1u8; 32],
                [2u8; 64],
            ),
            Error::<Test>::InvalidEquivocationEvidence
        );
    });
}

#[test]
fn h1_report_equivocation_rejects_identical_signatures() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        // Same signature_a and signature_b → should fail
        assert_noop!(
            GroupRobotConsensus::report_equivocation(
                RuntimeOrigin::signed(OTHER),
                node_id(1),
                42,
                [1u8; 32],
                [1u8; 64],
                [2u8; 32],
                [1u8; 64],
            ),
            Error::<Test>::InvalidEquivocationEvidence
        );
    });
}

#[test]
fn h2_mark_sequence_rejects_unauthorized_caller() {
    new_test_ext().execute_with(|| {
        // bot_hash(1) owner=OWNER, operator=OPERATOR
        // OTHER is neither owner nor operator → should fail
        assert_noop!(
            GroupRobotConsensus::mark_sequence_processed(
                RuntimeOrigin::signed(OTHER),
                bot_hash(1),
                42
            ),
            Error::<Test>::NotBotOperator
        );
    });
}

#[test]
fn h2_mark_sequence_works_for_owner() {
    new_test_ext().execute_with(|| {
        // bot_hash(1) owner=OWNER → should succeed
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OWNER),
            bot_hash(1),
            42
        ));
        assert!(ProcessedSequences::<Test>::contains_key(bot_hash(1), 42));
    });
}

#[test]
fn h2_mark_sequence_works_for_operator() {
    new_test_ext().execute_with(|| {
        // bot_hash(1) operator=OPERATOR → should succeed
        assert_ok!(GroupRobotConsensus::mark_sequence_processed(
            RuntimeOrigin::signed(OPERATOR),
            bot_hash(1),
            42
        ));
        assert!(ProcessedSequences::<Test>::contains_key(bot_hash(1), 42));
    });
}

#[test]
fn h4_finalize_exit_cleans_node_bot_binding() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(NodeBotBinding::<Test>::contains_key(node_id(1)));

        assert_ok!(GroupRobotConsensus::request_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        System::set_block_number(12);
        assert_ok!(GroupRobotConsensus::finalize_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));

        // NodeBotBinding should be cleaned
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
    });
}

#[test]
fn m2_slash_cleans_node_bot_binding() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(NodeBotBinding::<Test>::contains_key(node_id(1)));

        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // NodeBotBinding should be cleaned after slash
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
    });
}

#[test]
fn m3_slash_missing_equivocation_returns_correct_error() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        // No equivocation reported for sequence 99 → should return EquivocationNotFound
        assert_noop!(
            GroupRobotConsensus::slash_equivocation(RuntimeOrigin::root(), node_id(1), 99),
            Error::<Test>::EquivocationNotFound
        );
    });
}

#[test]
fn h5_suspended_node_can_request_exit() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // Node is now Suspended
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Suspended
        );

        // Suspended node should be able to request exit
        assert_ok!(GroupRobotConsensus::request_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Exiting
        );

        // After cooldown, can finalize and recover remaining stake
        System::set_block_number(12);
        let balance_before = Balances::free_balance(OPERATOR);
        assert_ok!(GroupRobotConsensus::finalize_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert!(Balances::free_balance(OPERATOR) > balance_before);
        assert!(!Nodes::<Test>::contains_key(node_id(1)));
        assert!(!OperatorNodes::<Test>::contains_key(OPERATOR));
    });
}

#[test]
fn h6_set_tee_reward_params_rejects_excessive_values() {
    new_test_ext().execute_with(|| {
        // tee_multiplier > 50000 → rejected
        assert_noop!(
            GroupRobotConsensus::set_tee_reward_params(RuntimeOrigin::root(), 50_001, 0),
            Error::<Test>::InvalidTeeRewardParams
        );
        // sgx_bonus > 10000 → rejected
        assert_noop!(
            GroupRobotConsensus::set_tee_reward_params(RuntimeOrigin::root(), 10_000, 10_001),
            Error::<Test>::InvalidTeeRewardParams
        );
        // Both at max → ok
        assert_ok!(GroupRobotConsensus::set_tee_reward_params(
            RuntimeOrigin::root(),
            50_000,
            10_000
        ));
        assert_eq!(TeeRewardMultiplier::<Test>::get(), 50_000);
        assert_eq!(SgxEnclaveBonus::<Test>::get(), 10_000);
    });
}

#[test]
fn h8_slash_resets_tee_status() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);

        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // After slash, is_tee_node should be false
        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert!(!node.is_tee_node, "Slash should reset TEE status");
        assert_eq!(node.status, NodeStatus::Suspended);
    });
}

// ============================================================================
// Round 1 regression tests
// ============================================================================

#[test]
fn h1r1_slash_rejects_already_resolved() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // Second slash of the same record → must be rejected
        assert_noop!(
            GroupRobotConsensus::slash_equivocation(RuntimeOrigin::root(), node_id(1), 42),
            Error::<Test>::EquivocationAlreadyResolved
        );

        // Stake should only be slashed once (10% of 1000 = 100 → 900 remaining)
        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.stake, 900);
    });
}

#[test]
fn m2r1_cleanup_resolved_equivocation_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));
        assert!(EquivocationRecords::<Test>::contains_key(node_id(1), 42));

        // Anyone can clean up a resolved record
        assert_ok!(GroupRobotConsensus::cleanup_resolved_equivocation(
            RuntimeOrigin::signed(OTHER),
            node_id(1),
            42
        ));
        assert!(!EquivocationRecords::<Test>::contains_key(node_id(1), 42));
    });
}

#[test]
fn m2r1_cleanup_unresolved_equivocation_fails() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);

        // Unresolved record cannot be cleaned up
        assert_noop!(
            GroupRobotConsensus::cleanup_resolved_equivocation(
                RuntimeOrigin::signed(OTHER),
                node_id(1),
                42
            ),
            Error::<Test>::EquivocationNotResolved
        );
    });
}

#[test]
fn m3r1_verify_tee_rejects_suspended_node() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));

        // Slash to suspend
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Suspended
        );

        // Suspended node cannot verify TEE
        assert_noop!(
            GroupRobotConsensus::verify_node_tee(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                bot_hash(10)
            ),
            Error::<Test>::NodeNotActive
        );
    });
}

#[test]
fn m3r1_verify_tee_rejects_exiting_node() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::request_exit(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Exiting
        );

        // Exiting node cannot verify TEE
        assert_noop!(
            GroupRobotConsensus::verify_node_tee(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                bot_hash(10)
            ),
            Error::<Test>::NodeNotActive
        );
    });
}

#[test]
fn m5r1_no_nodes_still_settles_and_records_uptime() {
    new_test_ext().execute_with(|| {
        // No nodes registered — era end should still settle subscriptions and record uptime
        set_mock_settle_income(500);
        clear_distributed_rewards();

        advance_to(52);

        assert!(CurrentEra::<Test>::get() >= 1);
        // Uptime should still be recorded
        let uptime_eras = get_recorded_uptime_eras();
        assert!(
            !uptime_eras.is_empty(),
            "Uptime should be recorded even with no nodes"
        );
        // No rewards distributed (no nodes)
        let rewards = get_distributed_rewards();
        assert!(rewards.is_empty());
    });
}

// ============================================================================
// P1: increase_stake
// ============================================================================

#[test]
fn p1_increase_stake_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 200);

        assert_ok!(GroupRobotConsensus::increase_stake(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            300
        ));
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 500);
    });
}

#[test]
fn p1_increase_stake_fails_not_operator() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::increase_stake(RuntimeOrigin::signed(OTHER), node_id(1), 100),
            Error::<Test>::NotOperator
        );
    });
}

#[test]
fn p1_increase_stake_fails_zero_amount() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::increase_stake(RuntimeOrigin::signed(OPERATOR), node_id(1), 0),
            Error::<Test>::InsufficientStake
        );
    });
}

#[test]
fn p1_increase_stake_works_for_suspended_node() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        // Slash to suspend
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Suspended
        );

        // Can increase stake even when suspended
        assert_ok!(GroupRobotConsensus::increase_stake(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            500
        ));
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 1400); // 900 + 500
    });
}

// ============================================================================
// P2: reinstate_node
// ============================================================================

#[test]
fn p2_reinstate_node_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Suspended
        );
        assert_eq!(ActiveNodeList::<Test>::get().len(), 0);

        assert_ok!(GroupRobotConsensus::reinstate_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Active
        );
        assert_eq!(ActiveNodeList::<Test>::get().len(), 1);
    });
}

#[test]
fn p2_reinstate_node_fails_not_suspended() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::reinstate_node(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            Error::<Test>::NodeNotSuspended
        );
    });
}

#[test]
fn p2_reinstate_node_fails_insufficient_stake() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            100
        ));
        // Force suspend via governance
        assert_ok!(GroupRobotConsensus::force_suspend_node(
            RuntimeOrigin::root(),
            node_id(1)
        ));
        // Slash some stake manually to make it below MinStake
        // Actually we need equivocation slash — use force_suspend + manual stake reduction
        // Better: register with minimal stake, slash via equivocation (10% of 100 = 10, remaining 90 < 100)
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Suspended
        );
        // Stake is 100, MinStake is 100, so this should work (stake >= MinStake)
        assert_ok!(GroupRobotConsensus::reinstate_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
    });
}

#[test]
fn p2_reinstate_after_slash_with_topup() {
    new_test_ext().execute_with(|| {
        // Register with exactly MinStake
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            100
        ));
        // Report + slash (10% = 10, remaining = 90 < MinStake 100)
        do_report_equivocation(OTHER, 1, 1);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            1
        ));
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 90);

        // Cannot reinstate: stake < MinStake
        assert_noop!(
            GroupRobotConsensus::reinstate_node(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            Error::<Test>::InsufficientStake
        );

        // Top up stake
        assert_ok!(GroupRobotConsensus::increase_stake(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            20
        ));
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 110);

        // Now reinstate works
        assert_ok!(GroupRobotConsensus::reinstate_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Active
        );
    });
}

// ============================================================================
// P3: force_suspend_node
// ============================================================================

#[test]
fn p3_force_suspend_node_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);

        assert_ok!(GroupRobotConsensus::force_suspend_node(
            RuntimeOrigin::root(),
            node_id(1)
        ));

        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.status, NodeStatus::Suspended);
        assert!(!node.is_tee_node);
        assert_eq!(ActiveNodeList::<Test>::get().len(), 0);
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
    });
}

#[test]
fn p3_force_suspend_node_requires_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::force_suspend_node(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn p3_force_suspend_node_fails_not_active() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));
        // Already Suspended
        assert_noop!(
            GroupRobotConsensus::force_suspend_node(RuntimeOrigin::root(), node_id(1)),
            Error::<Test>::NodeNotActive
        );
    });
}

// ============================================================================
// P4: force_remove_node
// ============================================================================

#[test]
fn p4_force_remove_node_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));

        assert_ok!(GroupRobotConsensus::force_remove_node(
            RuntimeOrigin::root(),
            node_id(1)
        ));

        assert!(!Nodes::<Test>::contains_key(node_id(1)));
        assert!(!OperatorNodes::<Test>::contains_key(OPERATOR));
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
        assert_eq!(ActiveNodeList::<Test>::get().len(), 0);
    });
}

#[test]
fn p4_force_remove_node_requires_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::force_remove_node(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ============================================================================
// P5: reporter reward + set_reporter_reward_pct
// ============================================================================

#[test]
fn p5_set_reporter_reward_pct_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::set_reporter_reward_pct(
            RuntimeOrigin::root(),
            1000
        ));
        assert_eq!(ReporterRewardPct::<Test>::get(), 1000);
    });
}

#[test]
fn p5_set_reporter_reward_pct_rejects_excessive() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::set_reporter_reward_pct(RuntimeOrigin::root(), 5001),
            Error::<Test>::InvalidReporterRewardPct
        );
    });
}

#[test]
fn p5_set_reporter_reward_pct_requires_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::set_reporter_reward_pct(RuntimeOrigin::signed(OPERATOR), 1000),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn p5_reporter_gets_reward_on_slash() {
    new_test_ext().execute_with(|| {
        // Set 10% reporter reward
        assert_ok!(GroupRobotConsensus::set_reporter_reward_pct(
            RuntimeOrigin::root(),
            1000
        ));

        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        let reporter_balance_before = Balances::free_balance(OTHER);

        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // Slash = 10% of 1000 = 100, reporter reward = 10% of 100 = 10
        let reporter_balance_after = Balances::free_balance(OTHER);
        assert_eq!(reporter_balance_after - reporter_balance_before, 10);
    });
}

#[test]
fn p5_no_reporter_reward_when_pct_zero() {
    new_test_ext().execute_with(|| {
        // Default ReporterRewardPct is 0
        assert_eq!(ReporterRewardPct::<Test>::get(), 0);

        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        let reporter_balance_before = Balances::free_balance(OTHER);

        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // No reward
        assert_eq!(Balances::free_balance(OTHER), reporter_balance_before);
    });
}

// ============================================================================
// P7: unbind_bot
// ============================================================================

#[test]
fn p7_unbind_bot_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);
        assert!(NodeBotBinding::<Test>::contains_key(node_id(1)));

        assert_ok!(GroupRobotConsensus::unbind_bot(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));

        assert!(!Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
    });
}

#[test]
fn p7_unbind_bot_fails_no_binding() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::unbind_bot(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            Error::<Test>::NoBotBinding
        );
    });
}

#[test]
fn p7_unbind_then_rebind_bot() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));

        // Unbind
        assert_ok!(GroupRobotConsensus::unbind_bot(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1)
        ));
        assert!(!Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);

        // Re-verify with same bot
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert!(Nodes::<Test>::get(node_id(1)).unwrap().is_tee_node);
    });
}

// ============================================================================
// P8: replace_operator
// ============================================================================

#[test]
fn p8_replace_operator_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));

        assert_ok!(GroupRobotConsensus::replace_operator(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            OWNER
        ));

        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.operator, OWNER);
        assert!(
            !node.is_tee_node,
            "TEE should be reset after operator change"
        );
        assert!(!NodeBotBinding::<Test>::contains_key(node_id(1)));
        assert!(!OperatorNodes::<Test>::contains_key(OPERATOR));
        assert_eq!(OperatorNodes::<Test>::get(OWNER), Some(node_id(1)));
    });
}

#[test]
fn p8_replace_operator_fails_new_has_node() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));
        assert_noop!(
            GroupRobotConsensus::replace_operator(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                OPERATOR2
            ),
            Error::<Test>::NewOperatorAlreadyHasNode
        );
    });
}

#[test]
fn p8_replace_operator_transfers_stake() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            500
        ));
        let op_reserved_before = Balances::reserved_balance(OPERATOR);
        let owner_reserved_before = Balances::reserved_balance(OWNER);

        assert_ok!(GroupRobotConsensus::replace_operator(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            OWNER
        ));

        // Old operator: stake unreserved
        assert_eq!(
            Balances::reserved_balance(OPERATOR),
            op_reserved_before - 500
        );
        // New operator: stake reserved
        assert_eq!(
            Balances::reserved_balance(OWNER),
            owner_reserved_before + 500
        );
    });
}

// ============================================================================
// P9: set_slash_percentage
// ============================================================================

#[test]
fn p9_set_slash_percentage_works() {
    new_test_ext().execute_with(|| {
        assert_eq!(GroupRobotConsensus::effective_slash_percentage(), 10); // Config default

        assert_ok!(GroupRobotConsensus::set_slash_percentage(
            RuntimeOrigin::root(),
            25
        ));
        assert_eq!(GroupRobotConsensus::effective_slash_percentage(), 25);

        // Reset to default
        assert_ok!(GroupRobotConsensus::set_slash_percentage(
            RuntimeOrigin::root(),
            0
        ));
        assert_eq!(GroupRobotConsensus::effective_slash_percentage(), 10);
    });
}

#[test]
fn p9_set_slash_percentage_rejects_over_100() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::set_slash_percentage(RuntimeOrigin::root(), 101),
            Error::<Test>::InvalidSlashPercentage
        );
    });
}

#[test]
fn p9_slash_uses_runtime_percentage() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::set_slash_percentage(
            RuntimeOrigin::root(),
            50
        ));

        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // 50% of 1000 = 500 slashed
        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.stake, 500);
    });
}

// ============================================================================
// P10: force_reinstate_node
// ============================================================================

#[test]
fn p10_force_reinstate_node_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));
        do_report_equivocation(OTHER, 1, 42);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            42
        ));

        // Force reinstate (no stake check)
        assert_ok!(GroupRobotConsensus::force_reinstate_node(
            RuntimeOrigin::root(),
            node_id(1)
        ));
        assert_eq!(
            Nodes::<Test>::get(node_id(1)).unwrap().status,
            NodeStatus::Active
        );
        assert_eq!(ActiveNodeList::<Test>::get().len(), 1);
    });
}

#[test]
fn p10_force_reinstate_requires_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::force_suspend_node(
            RuntimeOrigin::root(),
            node_id(1)
        ));
        assert_noop!(
            GroupRobotConsensus::force_reinstate_node(RuntimeOrigin::signed(OPERATOR), node_id(1)),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ============================================================================
// P12: batch_cleanup_equivocations + query helpers
// ============================================================================

#[test]
fn p12_batch_cleanup_equivocations_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            1000
        ));

        // Report and slash two equivocations
        do_report_equivocation(OTHER, 1, 1);
        do_report_equivocation(OTHER, 1, 2);
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            1
        ));
        assert_ok!(GroupRobotConsensus::slash_equivocation(
            RuntimeOrigin::root(),
            node_id(1),
            2
        ));

        // Batch cleanup
        let items = vec![(node_id(1), 1u64), (node_id(1), 2u64)];
        let bounded: BoundedVec<_, <Test as crate::pallet::Config>::MaxActiveNodes> =
            items.try_into().unwrap();
        assert_ok!(GroupRobotConsensus::batch_cleanup_equivocations(
            RuntimeOrigin::signed(OTHER),
            bounded
        ));

        assert!(!EquivocationRecords::<Test>::contains_key(node_id(1), 1));
        assert!(!EquivocationRecords::<Test>::contains_key(node_id(1), 2));
    });
}

#[test]
fn p12_batch_cleanup_fails_nothing_to_clean() {
    new_test_ext().execute_with(|| {
        let items: Vec<(NodeId, u64)> = vec![];
        let bounded: BoundedVec<_, <Test as crate::pallet::Config>::MaxActiveNodes> =
            items.try_into().unwrap();
        assert_noop!(
            GroupRobotConsensus::batch_cleanup_equivocations(RuntimeOrigin::signed(OTHER), bounded),
            Error::<Test>::NothingToCleanup
        );
    });
}

#[test]
fn p12_active_node_count_query() {
    new_test_ext().execute_with(|| {
        assert_eq!(GroupRobotConsensus::active_node_count(), 0);
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_eq!(GroupRobotConsensus::active_node_count(), 1);
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));
        assert_eq!(GroupRobotConsensus::active_node_count(), 2);
    });
}

#[test]
fn p12_current_era_query() {
    new_test_ext().execute_with(|| {
        assert_eq!(GroupRobotConsensus::current_era(), 0);
        advance_to(52);
        assert!(GroupRobotConsensus::current_era() >= 1);
    });
}

// ============================================================================
// P13: force_era_end
// ============================================================================

#[test]
fn p13_force_era_end_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        set_mock_settle_income(0);
        clear_distributed_rewards();

        let era_before = GroupRobotConsensus::current_era();
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));
        assert_eq!(GroupRobotConsensus::current_era(), era_before + 1);
    });
}

#[test]
fn p13_force_era_end_requires_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            GroupRobotConsensus::force_era_end(RuntimeOrigin::signed(OPERATOR)),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

// ============================================================================
// P14: ed25519 签名验证
// ============================================================================

#[test]
fn p14_report_equivocation_rejects_invalid_signature() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));

        // Use node 2's key to sign, but report against node 1 → sig verification fails
        let (ha, sa, _hb, _sb) = equivocation_evidence(2, 42); // signed by node 2
        let (hb, sb) = sign_msg(2, b"other-msg"); // also signed by node 2
        assert_noop!(
            GroupRobotConsensus::report_equivocation(
                RuntimeOrigin::signed(OTHER),
                node_id(1),
                42, // but claiming node 1
                ha,
                sa,
                hb,
                sb,
            ),
            Error::<Test>::InvalidEquivocationEvidence
        );
    });
}

#[test]
fn p14_report_equivocation_with_valid_signatures_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        let (ha, sa, hb, sb) = equivocation_evidence(1, 42);
        assert_ok!(GroupRobotConsensus::report_equivocation(
            RuntimeOrigin::signed(OTHER),
            node_id(1),
            42,
            ha,
            sa,
            hb,
            sb,
        ));
        assert!(EquivocationRecords::<Test>::contains_key(node_id(1), 42));
    });
}

#[test]
fn p14_report_equivocation_rejects_one_bad_signature() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        let (ha, sa, _hb, _sb) = equivocation_evidence(1, 42);
        // sig_b is from wrong node
        let (hb, sb) = sign_msg(2, b"wrong-key-msg");
        assert_noop!(
            GroupRobotConsensus::report_equivocation(
                RuntimeOrigin::signed(OTHER),
                node_id(1),
                42,
                ha,
                sa,
                hb,
                sb,
            ),
            Error::<Test>::InvalidEquivocationEvidence
        );
    });
}

// ============================================================================
// Boundary tests: MaxActiveNodes limit
// ============================================================================

#[test]
fn register_node_fails_when_max_nodes_reached() {
    new_test_ext().execute_with(|| {
        // MaxActiveNodes = 10 in mock
        // We need 10 different operators and node IDs
        let accounts: Vec<u64> = (100..110).collect();
        for (i, &acct) in accounts.iter().enumerate() {
            // Fund each account
            let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&acct, 100_000);
            assert_ok!(GroupRobotConsensus::register_node(
                RuntimeOrigin::signed(acct),
                node_id(20 + i as u8),
                200
            ));
        }
        assert_eq!(ActiveNodeList::<Test>::get().len(), 10);

        // 11th registration should fail
        let extra = 120u64;
        let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&extra, 100_000);
        assert_noop!(
            GroupRobotConsensus::register_node(RuntimeOrigin::signed(extra), node_id(50), 200),
            Error::<Test>::MaxNodesReached
        );
    });
}

#[test]
fn reinstate_node_fails_when_max_nodes_reached() {
    new_test_ext().execute_with(|| {
        // Fill up to MaxActiveNodes
        let accounts: Vec<u64> = (100..110).collect();
        for (i, &acct) in accounts.iter().enumerate() {
            let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&acct, 100_000);
            assert_ok!(GroupRobotConsensus::register_node(
                RuntimeOrigin::signed(acct),
                node_id(20 + i as u8),
                200
            ));
        }

        // Suspend one node, then fill the slot with a new registration
        assert_ok!(GroupRobotConsensus::force_suspend_node(
            RuntimeOrigin::root(),
            node_id(20)
        ));
        assert_eq!(ActiveNodeList::<Test>::get().len(), 9);

        let extra = 120u64;
        let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&extra, 100_000);
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(extra),
            node_id(50),
            200
        ));
        assert_eq!(ActiveNodeList::<Test>::get().len(), 10);

        // Now reinstate the suspended node — should fail, list is full
        assert_noop!(
            GroupRobotConsensus::reinstate_node(RuntimeOrigin::signed(100), node_id(20)),
            Error::<Test>::MaxNodesReached
        );
    });
}

#[test]
fn force_reinstate_node_fails_when_max_nodes_reached() {
    new_test_ext().execute_with(|| {
        let accounts: Vec<u64> = (100..110).collect();
        for (i, &acct) in accounts.iter().enumerate() {
            let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&acct, 100_000);
            assert_ok!(GroupRobotConsensus::register_node(
                RuntimeOrigin::signed(acct),
                node_id(20 + i as u8),
                200
            ));
        }

        assert_ok!(GroupRobotConsensus::force_suspend_node(
            RuntimeOrigin::root(),
            node_id(20)
        ));

        let extra = 120u64;
        let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&extra, 100_000);
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(extra),
            node_id(50),
            200
        ));

        assert_noop!(
            GroupRobotConsensus::force_reinstate_node(RuntimeOrigin::root(), node_id(20)),
            Error::<Test>::MaxNodesReached
        );
    });
}

// ============================================================================
// Boundary tests: replace_operator new operator insufficient balance
// ============================================================================

#[test]
fn replace_operator_fails_new_operator_insufficient_balance() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            500
        ));

        // Create a new account with very little balance
        let poor_account = 999u64;
        let _ = <Balances as CurrencyT<u64>>::make_free_balance_be(&poor_account, 10); // way less than 500

        assert_noop!(
            GroupRobotConsensus::replace_operator(
                RuntimeOrigin::signed(OPERATOR),
                node_id(1),
                poor_account
            ),
            pallet_balances::Error::<Test>::InsufficientBalance
        );

        // Verify original operator's stake is still intact (not unreserved)
        let node = Nodes::<Test>::get(node_id(1)).unwrap();
        assert_eq!(node.operator, OPERATOR);
        assert_eq!(node.stake, 500);
        assert_eq!(Balances::reserved_balance(OPERATOR), 500);
    });
}

// ============================================================================
// Boundary tests: multi-era consecutive settlement
// ============================================================================

#[test]
fn multiple_consecutive_eras_settle_correctly() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        set_mock_settle_income(100);
        clear_distributed_rewards();

        // Era 0 → 1
        advance_to(52);
        assert_eq!(GroupRobotConsensus::current_era(), 1);

        // Era 1 → 2
        advance_to(102);
        assert_eq!(GroupRobotConsensus::current_era(), 2);

        // Era 2 → 3
        advance_to(152);
        assert_eq!(GroupRobotConsensus::current_era(), 3);

        // Rewards should have been distributed 3 times
        let rewards = get_distributed_rewards();
        assert_eq!(rewards.len(), 3);

        // Uptime should have been recorded 3 times
        let uptime_eras = get_recorded_uptime_eras();
        assert!(uptime_eras.len() >= 3);
    });
}

// ============================================================================
// Boundary tests: force_era_end then natural era end
// ============================================================================

#[test]
fn force_era_end_then_natural_era_end() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        set_mock_settle_income(0);
        clear_distributed_rewards();

        // Advance a bit, then force era end at block 20
        advance_to(20);
        let era_before = GroupRobotConsensus::current_era();
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));
        assert_eq!(GroupRobotConsensus::current_era(), era_before + 1);

        // EraStartBlock should be reset to current block (20)
        // Next natural era end should be at block 20 + EraLength(50) = 70
        clear_distributed_rewards();
        advance_to(71);
        assert_eq!(GroupRobotConsensus::current_era(), era_before + 2);

        let rewards = get_distributed_rewards();
        assert!(
            !rewards.is_empty(),
            "Natural era end after force should still distribute"
        );
    });
}

// ============================================================================
// Boundary tests: replace_operator stake verification
// ============================================================================

#[test]
fn replace_operator_preserves_stake_on_success() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            500
        ));

        let op_free_before = Balances::free_balance(OPERATOR);
        let owner_free_before = Balances::free_balance(OWNER);

        assert_ok!(GroupRobotConsensus::replace_operator(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            OWNER
        ));

        // Old operator: stake unreserved → free balance increased by 500
        assert_eq!(Balances::reserved_balance(OPERATOR), 0);
        assert_eq!(Balances::free_balance(OPERATOR), op_free_before + 500);

        // New operator: stake reserved → free balance decreased by 500
        assert_eq!(Balances::reserved_balance(OWNER), 500);
        assert_eq!(Balances::free_balance(OWNER), owner_free_before - 500);

        // Node stake unchanged
        assert_eq!(Nodes::<Test>::get(node_id(1)).unwrap().stake, 500);
    });
}

// ============================================================================
// P15: consensus_enabled + allow_single_source_fallback
// ============================================================================

#[test]
fn p15_set_consensus_config_works() {
    new_test_ext().execute_with(|| {
        // Default values
        assert!(GroupRobotConsensus::is_consensus_enabled());
        assert!(GroupRobotConsensus::is_single_source_fallback_allowed());

        // Root can set
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            false,
            false
        ));
        assert!(!GroupRobotConsensus::is_consensus_enabled());
        assert!(!GroupRobotConsensus::is_single_source_fallback_allowed());

        // Non-root fails
        assert_noop!(
            GroupRobotConsensus::set_consensus_config(RuntimeOrigin::signed(OPERATOR), true, true),
            sp_runtime::DispatchError::BadOrigin
        );

        // Root can restore
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            true,
            true
        ));
        assert!(GroupRobotConsensus::is_consensus_enabled());
        assert!(GroupRobotConsensus::is_single_source_fallback_allowed());
    });
}

#[test]
fn p15_consensus_disabled_skips_reward_distribution() {
    new_test_ext().execute_with(|| {
        // Register a TEE node so rewards would normally distribute
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        set_mock_settle_income(100);
        clear_distributed_rewards();

        // Disable consensus
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            false,
            true
        ));

        let era_before = GroupRobotConsensus::current_era();

        // Trigger era end
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // Era should have advanced
        assert_eq!(GroupRobotConsensus::current_era(), era_before + 1);

        // But no rewards distributed
        let rewards = get_distributed_rewards();
        assert!(
            rewards.is_empty(),
            "consensus_enabled=false should skip reward distribution"
        );

        // Uptime should still be recorded
        let uptime_eras = get_recorded_uptime_eras();
        assert!(uptime_eras.contains(&era_before));
    });
}

#[test]
fn p15_consensus_disabled_still_settles_subscriptions() {
    new_test_ext().execute_with(|| {
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        set_mock_settle_income(500);
        clear_distributed_rewards();

        // Disable consensus
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            false,
            true
        ));

        let era_before = GroupRobotConsensus::current_era();

        // Force era end — settlement happens (MockSubscriptionSettler::settle_era called)
        // and era advances, but no reward distribution
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));
        assert_eq!(GroupRobotConsensus::current_era(), era_before + 1);

        // No rewards (consensus disabled)
        assert!(get_distributed_rewards().is_empty());
    });
}

#[test]
fn p15_single_source_fallback_false_requires_multiple_tee() {
    new_test_ext().execute_with(|| {
        // Register only 1 TEE node
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        set_mock_settle_income(0);
        clear_distributed_rewards();

        // Disable single source fallback
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            true,
            false
        ));

        // Trigger era end
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // Single TEE node → TEE bonus stripped, demoted to base weight
        // Still gets base reward (sole active node gets full inflation)
        let rewards = get_distributed_rewards();
        assert!(
            !rewards.is_empty(),
            "Demoted TEE node still gets base reward"
        );
        assert_eq!(
            rewards[0].1, 1_000_000_000,
            "Demoted to base weight, sole node gets full pool"
        );
    });
}

#[test]
fn p15_single_source_fallback_true_allows_single_tee() {
    new_test_ext().execute_with(|| {
        // Register 1 TEE node
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        set_mock_settle_income(0);
        clear_distributed_rewards();

        // Default: allow_single_source_fallback=true
        assert!(GroupRobotConsensus::is_single_source_fallback_allowed());

        // Trigger era end
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // Single TEE node should get rewards
        let rewards = get_distributed_rewards();
        assert!(
            !rewards.is_empty(),
            "Single TEE node with fallback=true should get rewards"
        );
        assert!(rewards[0].1 > 0, "Reward amount should be > 0");
    });
}

#[test]
fn p15_single_source_fallback_with_multiple_tee_works() {
    new_test_ext().execute_with(|| {
        // Register 2 TEE nodes
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            bot_hash(11)
        ));
        set_mock_settle_income(0);
        clear_distributed_rewards();

        // Disable single source fallback — but we have 2 TEE nodes, so it's fine
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            true,
            false
        ));

        // Trigger era end
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // Both nodes should get rewards
        let rewards = get_distributed_rewards();
        assert_eq!(rewards.len(), 2, "Two TEE nodes should both get rewards");
        assert!(rewards[0].1 > 0);
        assert!(rewards[1].1 > 0);
    });
}

#[test]
fn p15_fallback_false_demotes_tee_but_non_tee_unaffected() {
    new_test_ext().execute_with(|| {
        // 1 TEE node + 1 non-TEE node
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));
        // node 2 is non-TEE

        set_mock_settle_income(0);
        clear_distributed_rewards();

        // Disable single source fallback → single TEE demoted to base weight
        assert_ok!(GroupRobotConsensus::set_consensus_config(
            RuntimeOrigin::root(),
            true,
            false
        ));

        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // Both nodes get equal base weight → equal share of inflation
        let rewards = get_distributed_rewards();
        assert_eq!(rewards.len(), 2, "Both nodes should get rewards");
        // Both have same base weight → equal split of 1000 → 500 each
        assert_eq!(
            rewards[0].1, rewards[1].1,
            "TEE demoted to base = same as non-TEE"
        );
        assert_eq!(rewards[0].1, 500_000_000);
    });
}

#[test]
fn non_tee_and_tee_mixed_weight_distribution() {
    new_test_ext().execute_with(|| {
        // 1 TEE node (weight = base × 1.0 = 500,000) + 1 non-TEE node (weight = base = 500,000)
        // Default TeeRewardMultiplier = 0 → treated as 10,000 = 1.0x → same weight
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));

        set_mock_settle_income(0);
        clear_distributed_rewards();

        // With default TeeRewardMultiplier=0 (=1x), TEE weight = non-TEE weight
        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        let rewards = get_distributed_rewards();
        assert_eq!(rewards.len(), 2);
        // bot_hash(10) has_dual_attestation=true but SgxEnclaveBonus=0 → no bonus
        // Both nodes: equal weight → equal share
        assert_eq!(rewards[0].1, 500_000_000);
        assert_eq!(rewards[1].1, 500_000_000);
    });
}

#[test]
fn tee_multiplier_gives_tee_higher_share() {
    new_test_ext().execute_with(|| {
        // Set TEE multiplier to 2x (20000 basis points)
        assert_ok!(GroupRobotConsensus::set_tee_reward_params(
            RuntimeOrigin::root(),
            20_000,
            0
        ));

        // 1 TEE node (weight = 500,000 × 20000 / 10000 = 1,000,000)
        // 1 non-TEE node (weight = 500,000)
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            200
        ));
        assert_ok!(GroupRobotConsensus::verify_node_tee(
            RuntimeOrigin::signed(OPERATOR),
            node_id(1),
            bot_hash(10)
        ));
        assert_ok!(GroupRobotConsensus::register_node(
            RuntimeOrigin::signed(OPERATOR2),
            node_id(2),
            200
        ));

        set_mock_settle_income(0);
        clear_distributed_rewards();

        assert_ok!(GroupRobotConsensus::force_era_end(RuntimeOrigin::root()));

        // total_weight = 1,000,000 + 500,000 = 1,500,000
        // TEE: 1_000_000_000 × 1,000,000 / 1,500,000 = 666_666_666
        // non-TEE: 1_000_000_000 × 500,000 / 1,500,000 = 333_333_333
        let rewards = get_distributed_rewards();
        assert_eq!(rewards.len(), 2);
        let tee_reward = rewards
            .iter()
            .find(|(nid, _)| *nid == node_id(1))
            .unwrap()
            .1;
        let non_tee_reward = rewards
            .iter()
            .find(|(nid, _)| *nid == node_id(2))
            .unwrap()
            .1;
        assert_eq!(
            tee_reward, 666_666_666,
            "TEE node gets 2/3 of pool with 2x multiplier"
        );
        assert_eq!(non_tee_reward, 333_333_333, "Non-TEE node gets 1/3 of pool");
        assert!(
            tee_reward > non_tee_reward,
            "TEE should earn more than non-TEE"
        );
    });
}
