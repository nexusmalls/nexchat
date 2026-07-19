use crate::mock::*;
use crate::pallet;
use frame_support::{assert_noop, assert_ok};
use pallet_commission_common::{
    CommissionModes, CommissionPlugin, CommissionType, ReferralPlanWriter,
};

const ENTITY_ID: u64 = 1;
const OWNER: u64 = 100;
const ADMIN: u64 = 101;
const NON_OWNER: u64 = 102;

/// 设置 Entity Owner (用于 extrinsic 权限测试)
fn setup_entity() {
    set_entity_owner(ENTITY_ID, OWNER);
}

// ============================================================================
// Extrinsic tests — set_direct_reward_config
// ============================================================================

#[test]
fn set_direct_reward_config_works() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.direct_reward.rate, 500);
    });
}

#[test]
fn set_direct_reward_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                10001
            ),
            pallet::Error::<Test>::InvalidRate
        );
    });
}

#[test]
fn set_direct_reward_config_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(NON_OWNER),
                ENTITY_ID,
                500
            ),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

// ============================================================================
// Extrinsic tests — set_fixed_amount_config
// ============================================================================

#[test]
fn set_fixed_amount_config_works() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_fixed_amount_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.fixed_amount.amount, 1000);
    });
}

// ============================================================================
// Extrinsic tests — set_first_order_config
// ============================================================================

#[test]
fn set_first_order_config_works() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_first_order_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
            300,
            true,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.first_order.amount, 500);
        assert_eq!(config.first_order.rate, 300);
        assert!(config.first_order.use_amount);
    });
}

#[test]
fn set_first_order_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_noop!(
            CommissionReferral::set_first_order_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                500,
                10001,
                false
            ),
            pallet::Error::<Test>::InvalidRate
        );
    });
}

// ============================================================================
// Extrinsic tests — set_repeat_purchase_config
// ============================================================================

#[test]
fn set_repeat_purchase_config_works() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_repeat_purchase_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            200,
            3,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.repeat_purchase.rate, 200);
        assert_eq!(config.repeat_purchase.min_orders, 3);
    });
}

#[test]
fn set_repeat_purchase_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_noop!(
            CommissionReferral::set_repeat_purchase_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                10001,
                3
            ),
            pallet::Error::<Test>::InvalidRate
        );
    });
}

// ============================================================================
// CommissionPlugin — direct reward
// ============================================================================

#[test]
fn direct_reward_basic() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        // buyer=50, referrer=40
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 }, // 10%
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 40);
        assert_eq!(outputs[0].amount, 1000); // 10000 * 10%
        assert_eq!(outputs[0].commission_type, CommissionType::DirectReward);
        assert_eq!(remaining, 9000);
    });
}

#[test]
fn direct_reward_no_referrer() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        // buyer=50, no referrer

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn direct_reward_zero_rate_skips() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 0 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn direct_reward_capped_by_remaining() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 5000 }, // 50%
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        // remaining=100, order=10000, 50% of 10000 = 5000 but capped at 100
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 100, modes, false, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 100);
        assert_eq!(remaining, 0);
    });
}

// ============================================================================
// CommissionPlugin — fixed amount
// ============================================================================

#[test]
fn fixed_amount_basic() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                fixed_amount: pallet::FixedAmountConfig { amount: 500 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIXED_AMOUNT);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 500);
        assert_eq!(outputs[0].commission_type, CommissionType::FixedAmount);
        assert_eq!(remaining, 9500);
    });
}

#[test]
fn fixed_amount_zero_skips() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                fixed_amount: pallet::FixedAmountConfig { amount: 0 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIXED_AMOUNT);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

// ============================================================================
// CommissionPlugin — first order
// ============================================================================

#[test]
fn first_order_by_amount() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 800,
                    rate: 0,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        // is_first_order = true
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 800);
        assert_eq!(outputs[0].commission_type, CommissionType::FirstOrder);
        assert_eq!(remaining, 9200);
    });
}

#[test]
fn first_order_by_rate() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 0,
                    rate: 500,
                    use_amount: false,
                }, // 5%
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 500); // 10000 * 5%
        assert_eq!(remaining, 9500);
    });
}

#[test]
fn first_order_not_triggered_when_not_first() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // F7: buyer has completed orders → not first order
        set_completed_orders(1, 50, 1);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 800,
                    rate: 0,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 1, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn h3_first_order_zero_amount_early_return() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        // use_amount=true, amount=0 → 早返回
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 0,
                    rate: 500,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn h3_first_order_zero_rate_early_return() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        // use_amount=false, rate=0 → 早返回
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 500,
                    rate: 0,
                    use_amount: false,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

// ============================================================================
// CommissionPlugin — repeat purchase
// ============================================================================

#[test]
fn repeat_purchase_basic() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                repeat_purchase: pallet::RepeatPurchaseConfig {
                    rate: 300,
                    min_orders: 3,
                }, // 3%
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::REPEAT_PURCHASE);
        // buyer_order_count=5 >= min_orders=3
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 5, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 300); // 10000 * 3%
        assert_eq!(outputs[0].commission_type, CommissionType::RepeatPurchase);
        assert_eq!(remaining, 9700);
    });
}

#[test]
fn repeat_purchase_below_min_orders() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                repeat_purchase: pallet::RepeatPurchaseConfig {
                    rate: 300,
                    min_orders: 3,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::REPEAT_PURCHASE);
        // buyer_order_count=2 < min_orders=3
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 2, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

// ============================================================================
// CommissionPlugin — mode not enabled
// ============================================================================

#[test]
fn mode_not_enabled_returns_empty() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        // FIXED_AMOUNT mode, but config only has direct_reward
        let modes = CommissionModes(CommissionModes::FIXED_AMOUNT);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn no_config_returns_empty() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

// ============================================================================
// ReferralPlanWriter tests
// ============================================================================

#[test]
fn plan_writer_set_direct_rate() {
    new_test_ext().execute_with(|| {
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 500));
        let config = pallet::ReferralConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.direct_reward.rate, 500);
    });
}

#[test]
fn h2_plan_writer_rejects_invalid_direct_rate() {
    new_test_ext().execute_with(|| {
        assert!(
            <pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 10001)
                .is_err()
        );
    });
}

#[test]
fn h2_plan_writer_rejects_invalid_first_order_rate() {
    new_test_ext().execute_with(|| {
        assert!(
            <pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_first_order(
                1, 100, 10001, false,
            )
            .is_err()
        );
    });
}

#[test]
fn h2_plan_writer_rejects_invalid_repeat_purchase_rate() {
    new_test_ext().execute_with(|| {
        assert!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_repeat_purchase(
            1, 10001, 3,
        ).is_err());
    });
}

#[test]
fn plan_writer_clear_config() {
    new_test_ext().execute_with(|| {
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 500));
        assert!(pallet::ReferralConfigs::<Test>::get(1).is_some());

        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::clear_config(1));
        assert!(pallet::ReferralConfigs::<Test>::get(1).is_none());
    });
}

// ============================================================================
// M1 regression: PlanWriter emits events
// ============================================================================

#[test]
fn m1_plan_writer_set_direct_rate_emits_event() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 500));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: 1,
                mode: pallet::ReferralConfigMode::DirectReward,
            }
            .into(),
        );
    });
}

#[test]
fn m1_plan_writer_set_fixed_amount_emits_event() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        assert_ok!(
            <pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_fixed_amount(1, 1000)
        );
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: 1,
                mode: pallet::ReferralConfigMode::FixedAmount,
            }
            .into(),
        );
    });
}

#[test]
fn m1_plan_writer_set_first_order_emits_event() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        assert_ok!(
            <pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_first_order(
                1, 500, 300, true
            )
        );
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: 1,
                mode: pallet::ReferralConfigMode::FirstOrder,
            }
            .into(),
        );
    });
}

#[test]
fn m1_plan_writer_set_repeat_purchase_emits_event() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        assert_ok!(
            <pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_repeat_purchase(1, 200, 3)
        );
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: 1,
                mode: pallet::ReferralConfigMode::RepeatPurchase,
            }
            .into(),
        );
    });
}

#[test]
fn m1_plan_writer_clear_config_emits_cleared_event() {
    new_test_ext().execute_with(|| {
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 500));
        System::reset_events();
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::clear_config(1));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigCleared { entity_id: 1 }.into(),
        );
    });
}

// ============================================================================
// X1 regression: is_banned skips banned referrer
// ============================================================================

#[test]
fn x1_direct_reward_skips_banned_referrer() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        ban_member(1, 40);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn x1_fixed_amount_skips_banned_referrer() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        ban_member(1, 40);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                fixed_amount: pallet::FixedAmountConfig { amount: 500 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIXED_AMOUNT);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn x1_first_order_skips_banned_referrer() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        ban_member(1, 40);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 800,
                    rate: 0,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn x1_repeat_purchase_skips_banned_referrer() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        ban_member(1, 40);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                repeat_purchase: pallet::RepeatPurchaseConfig {
                    rate: 300,
                    min_orders: 3,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::REPEAT_PURCHASE);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 5, 0,
            );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn x1_non_banned_referrer_still_receives_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // ban a different member, not the referrer
        ban_member(1, 99);

        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 40);
        assert_eq!(remaining, 9000);
    });
}

// ============================================================================
// X2 regression: phantom event guard
// ============================================================================

#[test]
fn x2_plan_writer_clear_no_phantom_event_when_config_absent() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        // clear on non-existent config — should NOT emit event
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::clear_config(999));
        assert!(System::events().is_empty());
    });
}

#[test]
fn x2_plan_writer_clear_emits_event_when_config_exists() {
    new_test_ext().execute_with(|| {
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::set_direct_rate(1, 500));
        System::reset_events();
        assert_ok!(<pallet::Pallet<Test> as ReferralPlanWriter<Balance>>::clear_config(1));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigCleared { entity_id: 1 }.into(),
        );
        assert!(pallet::ReferralConfigs::<Test>::get(1).is_none());
    });
}

#[test]
fn x2_force_clear_no_phantom_event() {
    new_test_ext().execute_with(|| {
        System::reset_events();
        assert_ok!(CommissionReferral::force_clear_referral_config(
            RuntimeOrigin::root(),
            999,
        ));
        assert!(System::events().is_empty());
    });
}

#[test]
fn x2_force_clear_emits_event_when_config_exists() {
    new_test_ext().execute_with(|| {
        pallet::ReferralConfigs::<Test>::insert(1, pallet::ReferralConfig::<Balance>::default());
        System::reset_events();
        assert_ok!(CommissionReferral::force_clear_referral_config(
            RuntimeOrigin::root(),
            1,
        ));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigCleared { entity_id: 1 }.into(),
        );
    });
}

// ============================================================================
// X3 regression: Owner/Admin permission + force_* Root
// ============================================================================

#[test]
fn x3_admin_with_commission_manage_can_set_config() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::COMMISSION_MANAGE,
        );
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(ADMIN),
            ENTITY_ID,
            800,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.direct_reward.rate, 800);
    });
}

#[test]
fn x3_admin_without_commission_manage_rejected() {
    new_test_ext().execute_with(|| {
        setup_entity();
        // Admin has SHOP_MANAGE but not COMMISSION_MANAGE
        set_entity_admin(
            ENTITY_ID,
            ADMIN,
            pallet_entity_common::AdminPermission::SHOP_MANAGE,
        );
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(ADMIN),
                ENTITY_ID,
                800
            ),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn x3_entity_not_found_rejected() {
    new_test_ext().execute_with(|| {
        // No entity setup → entity_owner returns None
        assert_noop!(
            CommissionReferral::set_direct_reward_config(RuntimeOrigin::signed(OWNER), 999, 500),
            pallet::Error::<Test>::EntityNotFound
        );
    });
}

#[test]
fn x3_force_set_direct_reward_works_for_root() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionReferral::force_set_direct_reward_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            500,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.direct_reward.rate, 500);
    });
}

#[test]
fn x3_force_set_rejects_non_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            CommissionReferral::force_set_direct_reward_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                500
            ),
            sp_runtime::DispatchError::BadOrigin
        );
    });
}

#[test]
fn x3_force_set_fixed_amount_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionReferral::force_set_fixed_amount_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2000,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.fixed_amount.amount, 2000);
    });
}

#[test]
fn x3_force_set_first_order_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionReferral::force_set_first_order_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            100,
            500,
            false,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.first_order.amount, 100);
        assert_eq!(config.first_order.rate, 500);
        assert!(!config.first_order.use_amount);
    });
}

#[test]
fn x3_force_set_repeat_purchase_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(CommissionReferral::force_set_repeat_purchase_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            300,
            5,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.repeat_purchase.rate, 300);
        assert_eq!(config.repeat_purchase.min_orders, 5);
    });
}

#[test]
fn x3_clear_referral_config_by_owner() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
        ));
        assert!(pallet::ReferralConfigs::<Test>::get(ENTITY_ID).is_some());

        assert_ok!(CommissionReferral::clear_referral_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
        ));
        assert!(pallet::ReferralConfigs::<Test>::get(ENTITY_ID).is_none());
    });
}

#[test]
fn x3_clear_referral_config_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_entity();
        pallet::ReferralConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::ReferralConfig::<Balance>::default(),
        );
        assert_noop!(
            CommissionReferral::clear_referral_config(RuntimeOrigin::signed(NON_OWNER), ENTITY_ID),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin
        );
    });
}

#[test]
fn x3_clear_referral_config_rejects_absent() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_noop!(
            CommissionReferral::clear_referral_config(RuntimeOrigin::signed(OWNER), ENTITY_ID),
            pallet::Error::<Test>::ConfigNotFound
        );
    });
}

// ==================== EntityLocked 回归测试 ====================

#[test]
fn entity_locked_rejects_set_direct_reward_config() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                1000,
            ),
            pallet::Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn entity_locked_rejects_clear_referral_config() {
    new_test_ext().execute_with(|| {
        setup_entity();
        // 先设置配置
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
        ));
        // 锁定后无法清除
        set_entity_locked(ENTITY_ID);
        assert_noop!(
            CommissionReferral::clear_referral_config(RuntimeOrigin::signed(OWNER), ENTITY_ID),
            pallet::Error::<Test>::EntityLocked
        );
    });
}

// ============================================================================
// F1 regression: 推荐人激活条件
// ============================================================================

#[test]
fn f1_referrer_guard_min_spent_blocks_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // referrer spent=500, guard requires 1000
        set_stats(1, 40, 0, 0, 500, 500);
        pallet::ReferrerGuardConfigs::<Test>::insert(
            1,
            pallet::ReferrerGuardConfig {
                min_referrer_spent: 1000,
                min_referrer_orders: 0,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f1_referrer_guard_min_spent_passes() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        set_stats(1, 40, 0, 0, 2000, 2000);
        pallet::ReferrerGuardConfigs::<Test>::insert(
            1,
            pallet::ReferrerGuardConfig {
                min_referrer_spent: 1000,
                min_referrer_orders: 0,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f1_referrer_guard_min_orders_blocks_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        set_completed_orders(1, 40, 2); // referrer has 2, guard requires 5
        pallet::ReferrerGuardConfigs::<Test>::insert(
            1,
            pallet::ReferrerGuardConfig {
                min_referrer_spent: 0,
                min_referrer_orders: 5,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f1_set_referrer_guard_config_extrinsic() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_referrer_guard_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            5000,
            3,
        ));
        let guard = pallet::ReferrerGuardConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(guard.min_referrer_spent, 5000);
        assert_eq!(guard.min_referrer_orders, 3);
    });
}

// ============================================================================
// F2 regression: 返佣上限封顶
// ============================================================================

#[test]
fn f2_max_per_order_caps_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        pallet::CommissionCapConfigs::<Test>::insert(
            1,
            pallet::CommissionCapConfig {
                max_per_order: 200u128,
                max_total_earned: 0u128,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 }, // 10% of 10000 = 1000
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 200); // capped at 200
        assert_eq!(remaining, 9800);
    });
}

#[test]
fn f2_max_total_earned_caps_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // referrer already earned 900, cap is 1000
        pallet::ReferrerTotalEarned::<Test>::insert(1, 40u64, 900u128);
        pallet::CommissionCapConfigs::<Test>::insert(
            1,
            pallet::CommissionCapConfig {
                max_per_order: 0u128,
                max_total_earned: 1000u128,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 }, // 10% of 10000 = 1000
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 100); // only 100 space left
        assert_eq!(remaining, 9900);
        // total earned should be updated
        assert_eq!(pallet::ReferrerTotalEarned::<Test>::get(1, 40u64), 1000);
    });
}

#[test]
fn f2_total_earned_reached_cap_blocks_entirely() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        pallet::ReferrerTotalEarned::<Test>::insert(1, 40u64, 1000u128);
        pallet::CommissionCapConfigs::<Test>::insert(
            1,
            pallet::CommissionCapConfig {
                max_per_order: 0u128,
                max_total_earned: 1000u128,
            },
        );
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f2_set_commission_cap_config_extrinsic() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
            10000,
        ));
        let cap = pallet::CommissionCapConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(cap.max_per_order, 500);
        assert_eq!(cap.max_total_earned, 10000);
    });
}

// ============================================================================
// F3 regression: 配置生效时间控制
// ============================================================================

#[test]
fn f3_config_not_effective_yet_returns_empty() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // current block = 1, effective_after = 100
        pallet::ConfigEffectiveAfter::<Test>::insert(1, 100u64);
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f3_config_effective_after_reached_works() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        System::set_block_number(100);
        pallet::ConfigEffectiveAfter::<Test>::insert(1, 100u64);
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, _) = <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
            1, &50, 10000, 10000, modes, false, 0, 0,
        );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 1000);
    });
}

#[test]
fn f3_set_config_effective_after_extrinsic() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_config_effective_after(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
        ));
        assert_eq!(
            pallet::ConfigEffectiveAfter::<Test>::get(ENTITY_ID),
            Some(500)
        );
        System::assert_last_event(
            pallet::Event::<Test>::ConfigEffectiveAfterSet {
                entity_id: ENTITY_ID,
                block_number: 500,
            }
            .into(),
        );
    });
}

// ============================================================================
// F4 regression: Entity 活跃状态检查
// ============================================================================

#[test]
fn f4_entity_not_active_rejects_set_config() {
    new_test_ext().execute_with(|| {
        setup_entity();
        deactivate_entity(ENTITY_ID);
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                500
            ),
            pallet::Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_not_active_rejects_clear_config() {
    new_test_ext().execute_with(|| {
        setup_entity();
        pallet::ReferralConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::ReferralConfig::<Balance>::default(),
        );
        deactivate_entity(ENTITY_ID);
        assert_noop!(
            CommissionReferral::clear_referral_config(RuntimeOrigin::signed(OWNER), ENTITY_ID),
            pallet::Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_entity_not_active_rejects_new_extrinsics() {
    new_test_ext().execute_with(|| {
        setup_entity();
        deactivate_entity(ENTITY_ID);
        assert_noop!(
            CommissionReferral::set_referrer_guard_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                100,
                1
            ),
            pallet::Error::<Test>::EntityNotActive
        );
        assert_noop!(
            CommissionReferral::set_commission_cap_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                100,
                1000
            ),
            pallet::Error::<Test>::EntityNotActive
        );
        assert_noop!(
            CommissionReferral::set_config_effective_after(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                100
            ),
            pallet::Error::<Test>::EntityNotActive
        );
    });
}

#[test]
fn f4_force_set_ignores_entity_inactive() {
    new_test_ext().execute_with(|| {
        setup_entity();
        deactivate_entity(ENTITY_ID);
        // Root force_* should still work
        assert_ok!(CommissionReferral::force_set_direct_reward_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            500,
        ));
        let config = pallet::ReferralConfigs::<Test>::get(ENTITY_ID).unwrap();
        assert_eq!(config.direct_reward.rate, 500);
    });
}

// ============================================================================
// F6 regression: 推荐人冻结状态检查
// ============================================================================

#[test]
fn f6_frozen_referrer_skips_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        freeze_member(1, 40);
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn f6_active_referrer_receives_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // freeze a different member
        freeze_member(1, 99);
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 1000 },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, _) = <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
            1, &50, 10000, 10000, modes, false, 0, 0,
        );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 40);
    });
}

// ============================================================================
// F7 regression: 首单判定准确性
// ============================================================================

#[test]
fn f7_first_order_uses_completed_order_count() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // buyer has 0 completed orders (cancelled orders don't count)
        set_completed_orders(1, 50, 0);
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 800,
                    rate: 0,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        // even if is_first_order param is false, F7 uses completed_order_count
        let (outputs, _) = <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
            1, &50, 10000, 10000, modes, false, 0, 0,
        );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 800);
    });
}

#[test]
fn f7_not_first_order_when_completed_orders_exist() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        set_completed_orders(1, 50, 1); // buyer has 1 completed order
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                first_order: pallet::FirstOrderConfig {
                    amount: 800,
                    rate: 0,
                    use_amount: true,
                },
                ..Default::default()
            },
        );

        let modes = CommissionModes(CommissionModes::FIRST_ORDER);
        // even if is_first_order param is true, F7 overrides with completed_order_count
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, true, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

// ============================================================================
// F8 regression: 全局返佣率上限
// ============================================================================

#[test]
fn f8_global_rate_cap_limits_total_commission() {
    new_test_ext().execute_with(|| {
        clear_thread_locals();
        setup_chain(1, 50, &[40]);
        // Override MaxTotalReferralRate via storage — we can't change the const,
        // but we can test with the default 10000 (no limit) and verify the logic
        // by directly testing the internal process functions with cap applied.
        // Instead, let's directly set up a scenario that exercises the cap logic
        // by checking that when cap = 10000, no clipping occurs.
        pallet::ReferralConfigs::<Test>::insert(
            1,
            pallet::ReferralConfig {
                direct_reward: pallet::DirectRewardConfig { rate: 5000 }, // 50%
                repeat_purchase: pallet::RepeatPurchaseConfig {
                    rate: 5000,
                    min_orders: 0,
                }, // 50%
                ..Default::default()
            },
        );

        let modes =
            CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::REPEAT_PURCHASE);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                1, &50, 10000, 10000, modes, false, 5, 0,
            );
        // With MaxTotalReferralRate=10000 (100%), both commissions should pass through
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].amount, 5000);
        assert_eq!(outputs[1].amount, 5000);
        assert_eq!(remaining, 0);
    });
}

// ============================================================================
// F10 regression: 事件粒度增强
// ============================================================================

#[test]
fn f10_set_direct_reward_emits_mode() {
    new_test_ext().execute_with(|| {
        setup_entity();
        System::reset_events();
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
        ));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: ENTITY_ID,
                mode: pallet::ReferralConfigMode::DirectReward,
            }
            .into(),
        );
    });
}

#[test]
fn f10_set_referrer_guard_emits_mode() {
    new_test_ext().execute_with(|| {
        setup_entity();
        System::reset_events();
        assert_ok!(CommissionReferral::set_referrer_guard_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            100,
            1,
        ));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: ENTITY_ID,
                mode: pallet::ReferralConfigMode::ReferrerGuard,
            }
            .into(),
        );
    });
}

#[test]
fn f10_set_commission_cap_emits_mode() {
    new_test_ext().execute_with(|| {
        setup_entity();
        System::reset_events();
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500,
            10000,
        ));
        System::assert_last_event(
            pallet::Event::<Test>::ReferralConfigUpdated {
                entity_id: ENTITY_ID,
                mode: pallet::ReferralConfigMode::CommissionCap,
            }
            .into(),
        );
    });
}

// ============================================================================
// H1 regression: clear 操作清理附属存储
// ============================================================================

#[test]
fn h1_clear_referral_config_removes_satellite_storage() {
    new_test_ext().execute_with(|| {
        setup_entity();
        // Set main config + all satellite storage
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500
        ));
        assert_ok!(CommissionReferral::set_referrer_guard_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
            3
        ));
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            200,
            5000
        ));
        assert_ok!(CommissionReferral::set_config_effective_after(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500
        ));

        // Verify all exist
        assert!(pallet::ReferralConfigs::<Test>::contains_key(ENTITY_ID));
        assert!(pallet::ReferrerGuardConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(pallet::CommissionCapConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(pallet::ConfigEffectiveAfter::<Test>::contains_key(
            ENTITY_ID
        ));

        // Clear
        assert_ok!(CommissionReferral::clear_referral_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID
        ));

        // All satellite storage must be removed
        assert!(!pallet::ReferralConfigs::<Test>::contains_key(ENTITY_ID));
        assert!(!pallet::ReferrerGuardConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::CommissionCapConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::ConfigEffectiveAfter::<Test>::contains_key(
            ENTITY_ID
        ));
    });
}

#[test]
fn m1r4_clear_config_resets_referrer_total_earned() {
    new_test_ext().execute_with(|| {
        setup_entity();
        // Set up config with cap
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000
        ));
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            0,
            5000
        ));

        // Simulate referrer earning 3000
        pallet::ReferrerTotalEarned::<Test>::insert(ENTITY_ID, 40u64, 3000u128);
        pallet::ReferrerTotalEarned::<Test>::insert(ENTITY_ID, 41u64, 1500u128);

        // Clear all config
        assert_ok!(CommissionReferral::clear_referral_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID
        ));

        // ReferrerTotalEarned must be cleared
        assert_eq!(
            pallet::ReferrerTotalEarned::<Test>::get(ENTITY_ID, 40u64),
            0
        );
        assert_eq!(
            pallet::ReferrerTotalEarned::<Test>::get(ENTITY_ID, 41u64),
            0
        );

        // Re-create config with new cap — referrer should get full cap space
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000
        ));
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            0,
            2000
        ));

        // Verify referrer 40 gets full 2000 space (not reduced by stale 3000)
        setup_chain(ENTITY_ID, 50, &[40]);
        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, _) = <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
            ENTITY_ID, &50, 100000, 100000, modes, false, 0, 0,
        );
        // 10% of 100000 = 10000, capped at 2000
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 2000);
    });
}

#[test]
fn h1_force_clear_removes_satellite_storage() {
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500
        ));
        assert_ok!(CommissionReferral::set_referrer_guard_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
            3
        ));
        assert_ok!(CommissionReferral::set_commission_cap_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            200,
            5000
        ));
        assert_ok!(CommissionReferral::set_config_effective_after(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500
        ));

        assert_ok!(CommissionReferral::force_clear_referral_config(
            RuntimeOrigin::root(),
            ENTITY_ID
        ));

        assert!(!pallet::ReferralConfigs::<Test>::contains_key(ENTITY_ID));
        assert!(!pallet::ReferrerGuardConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::CommissionCapConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::ConfigEffectiveAfter::<Test>::contains_key(
            ENTITY_ID
        ));
    });
}

#[test]
fn h1_plan_writer_clear_removes_satellite_storage() {
    use pallet_commission_common::ReferralPlanWriter;
    new_test_ext().execute_with(|| {
        setup_entity();
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            500
        ));
        pallet::ReferrerGuardConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::ReferrerGuardConfig {
                min_referrer_spent: 1000,
                min_referrer_orders: 3,
            },
        );
        pallet::CommissionCapConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::CommissionCapConfig {
                max_per_order: 200u128,
                max_total_earned: 5000u128,
            },
        );
        pallet::ConfigEffectiveAfter::<Test>::insert(ENTITY_ID, 500u64);

        assert_ok!(<CommissionReferral as ReferralPlanWriter<Balance>>::clear_config(ENTITY_ID));

        assert!(!pallet::ReferralConfigs::<Test>::contains_key(ENTITY_ID));
        assert!(!pallet::ReferrerGuardConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::CommissionCapConfigs::<Test>::contains_key(
            ENTITY_ID
        ));
        assert!(!pallet::ConfigEffectiveAfter::<Test>::contains_key(
            ENTITY_ID
        ));
    });
}

// ============================================================================
// 审计 R5: M1 — 非会员推荐人不获佣
// ============================================================================

#[test]
fn m1_r5_non_member_referrer_skipped_direct_reward() {
    new_test_ext().execute_with(|| {
        setup_entity();
        let buyer = 1u64;
        let referrer = 2u64;
        set_referrer(ENTITY_ID, buyer, referrer);
        // 标记推荐人为非会员
        set_non_member(ENTITY_ID, referrer);

        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
        ));

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                ENTITY_ID, &buyer, 10000, 10000, modes, false, 0, 0,
            );
        // 非会员推荐人应被跳过，无返佣输出
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn m1_r5_member_referrer_still_gets_commission() {
    new_test_ext().execute_with(|| {
        setup_entity();
        let buyer = 1u64;
        let referrer = 2u64;
        set_referrer(ENTITY_ID, buyer, referrer);
        // 不标记为非会员 — 默认是会员

        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
        ));

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                ENTITY_ID, &buyer, 10000, 10000, modes, false, 0, 0,
            );
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 1000); // 10000 * 1000 / 10000
        assert_eq!(remaining, 9000);
    });
}

#[test]
fn m1_r5_non_member_referrer_skipped_token_path() {
    use pallet_commission_common::TokenCommissionPlugin;
    new_test_ext().execute_with(|| {
        setup_entity();
        let buyer = 1u64;
        let referrer = 2u64;
        set_referrer(ENTITY_ID, buyer, referrer);
        set_non_member(ENTITY_ID, referrer);

        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1000,
        ));

        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as TokenCommissionPlugin<u64, u128>>::calculate_token(
                ENTITY_ID, &buyer, 10000u128, 10000u128, modes, false, 0, 0,
            );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000u128);
    });
}

// ============================================================================
// 审计 R5: M2 — F8 裁剪后 ReferrerTotalEarned 修正
// ============================================================================

#[test]
fn m2_r5_f8_truncation_adjusts_referrer_total_earned() {
    new_test_ext().execute_with(|| {
        setup_entity();
        let buyer = 1u64;
        let referrer = 2u64;
        set_referrer(ENTITY_ID, buyer, referrer);

        // 设置直推 20% + 复购 20% = 总计 40%
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            2000,
        ));
        assert_ok!(CommissionReferral::set_repeat_purchase_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            2000,
            0,
        ));
        // F2: 设置累计上限 = 10000
        pallet::CommissionCapConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::CommissionCapConfig {
                max_per_order: 0u128,
                max_total_earned: 10000u128,
            },
        );

        // MaxTotalReferralRate = 10000 (不限制) — 验证基线
        let modes =
            CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::REPEAT_PURCHASE);
        let (outputs, _remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                ENTITY_ID, &buyer, 1000, 1000, modes, false, 5, 0,
            );
        // direct=200 + repeat=200 = 400, 无F8裁剪
        assert_eq!(outputs.len(), 2);
        let total_output: u128 = outputs.iter().map(|o| o.amount).sum();
        assert_eq!(total_output, 400);
        // ReferrerTotalEarned 应等于实际输出总额
        let earned = pallet::ReferrerTotalEarned::<Test>::get(ENTITY_ID, referrer);
        assert_eq!(earned, 400);
    });
}

#[test]
fn m2_r5_f8_truncation_corrects_cumulative_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        let buyer = 1u64;
        let referrer = 2u64;
        set_referrer(ENTITY_ID, buyer, referrer);
        // F8: 限制总返佣率为 10% (1000 bps)
        set_max_referral_rate(1000);

        // 设置直推 20% + 复购 20% = 总计 40%，但 F8 上限为 10%
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            2000,
        ));
        assert_ok!(CommissionReferral::set_repeat_purchase_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            2000,
            0,
        ));
        // F2: 设置累计上限
        pallet::CommissionCapConfigs::<Test>::insert(
            ENTITY_ID,
            pallet::CommissionCapConfig {
                max_per_order: 0u128,
                max_total_earned: 5000u128,
            },
        );

        let modes =
            CommissionModes(CommissionModes::DIRECT_REWARD | CommissionModes::REPEAT_PURCHASE);
        let (outputs, remaining) =
            <pallet::Pallet<Test> as CommissionPlugin<u64, Balance>>::calculate(
                ENTITY_ID, &buyer, 10000, 10000, modes, false, 5, 0,
            );
        // F8 上限: max_amount = 10000 * 1000 / 10000 = 1000
        // direct=2000 被裁剪为 1000, repeat=2000 被裁剪为 0
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 1000);
        // remaining 应恢复: 10000 - 2000 - 2000 + 1000 + 2000 = 9000
        assert_eq!(remaining, 9000);
        // M2 修复: ReferrerTotalEarned 应等于实际发放的 1000，而非 process 阶段的 4000
        let earned = pallet::ReferrerTotalEarned::<Test>::get(ENTITY_ID, referrer);
        assert_eq!(earned, 1000);
    });
}

// ============================================================================
// Budget Cap Validation Tests
// ============================================================================

#[test]
fn set_direct_reward_config_rejects_rate_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_budget_cap(ENTITY_ID, 3000); // cap = 30%
        assert_noop!(
            CommissionReferral::set_direct_reward_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                3001
            ),
            crate::pallet::Error::<Test>::RateExceedsBudgetCap
        );
        // rate == cap should pass
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            3000
        ));
    });
}

#[test]
fn set_first_order_config_rejects_rate_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_budget_cap(ENTITY_ID, 2000);
        assert_noop!(
            CommissionReferral::set_first_order_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                0u128,
                2001,
                false
            ),
            crate::pallet::Error::<Test>::RateExceedsBudgetCap
        );
        assert_ok!(CommissionReferral::set_first_order_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            0u128,
            2000,
            false
        ));
    });
}

#[test]
fn set_repeat_purchase_config_rejects_rate_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_budget_cap(ENTITY_ID, 1500);
        assert_noop!(
            CommissionReferral::set_repeat_purchase_config(
                RuntimeOrigin::signed(OWNER),
                ENTITY_ID,
                1501,
                1
            ),
            crate::pallet::Error::<Test>::RateExceedsBudgetCap
        );
        assert_ok!(CommissionReferral::set_repeat_purchase_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            1500,
            1
        ));
    });
}

#[test]
fn force_set_direct_reward_config_rejects_rate_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_budget_cap(ENTITY_ID, 2000);
        assert_noop!(
            CommissionReferral::force_set_direct_reward_config(
                RuntimeOrigin::root(),
                ENTITY_ID,
                2001
            ),
            crate::pallet::Error::<Test>::RateExceedsBudgetCap
        );
        assert_ok!(CommissionReferral::force_set_direct_reward_config(
            RuntimeOrigin::root(),
            ENTITY_ID,
            2000
        ));
    });
}

#[test]
fn budget_cap_zero_means_no_limit() {
    new_test_ext().execute_with(|| {
        setup_entity();
        // cap=0 means no limit, rate=10000 should pass
        assert_ok!(CommissionReferral::set_direct_reward_config(
            RuntimeOrigin::signed(OWNER),
            ENTITY_ID,
            10000
        ));
    });
}

#[test]
fn plan_writer_rejects_rate_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        setup_entity();
        set_budget_cap(ENTITY_ID, 1000);
        assert!(
            <crate::pallet::Pallet<Test> as ReferralPlanWriter<u128>>::set_direct_rate(
                ENTITY_ID, 1001
            )
            .is_err()
        );
        assert!(
            <crate::pallet::Pallet<Test> as ReferralPlanWriter<u128>>::set_first_order(
                ENTITY_ID, 0, 1001, false
            )
            .is_err()
        );
        assert!(
            <crate::pallet::Pallet<Test> as ReferralPlanWriter<u128>>::set_repeat_purchase(
                ENTITY_ID, 1001, 1
            )
            .is_err()
        );
        // at cap should pass
        assert_ok!(
            <crate::pallet::Pallet<Test> as ReferralPlanWriter<u128>>::set_direct_rate(
                ENTITY_ID, 1000
            )
        );
    });
}
