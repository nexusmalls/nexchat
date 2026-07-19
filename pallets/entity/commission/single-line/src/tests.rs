use crate::mock::*;
use crate::pallet;
use crate::pallet::ReachMode;
use frame_support::{assert_noop, assert_ok};
use pallet_commission_common::{
    CommissionModes, CommissionPlugin, CommissionType, TokenCommissionPlugin,
};
use pallet_entity_common::AdminPermission;

type SingleLine = pallet::Pallet<Test>;

const OWNER: u64 = 100;
const ADMIN: u64 = 101;
const NOBODY: u64 = 999;

// ============================================================================
// Helper: 构建单链 [1, 2, 3, 4, 5]
// ============================================================================

fn setup_single_line(entity_id: u64, accounts: &[u64]) {
    for acc in accounts {
        assert_ok!(SingleLine::add_to_single_line(entity_id, acc));
    }
}

fn setup_entity(entity_id: u64) {
    set_entity_owner(entity_id, OWNER);
    set_entity_admin(entity_id, ADMIN, AdminPermission::COMMISSION_MANAGE);
    // 默认配置 10 个自定义等级，覆盖大多数测试中的 level_id
    set_custom_level_count(entity_id, 10);
}

fn setup_config(entity_id: u64) {
    setup_entity(entity_id);
    assert_ok!(SingleLine::set_single_line_config(
        RuntimeOrigin::signed(OWNER),
        entity_id,
        100,  // upline_rate = 1%
        100,  // downline_rate = 1%
        10,   // base_upline_levels
        15,   // base_downline_levels
        1000, // level_increment_threshold
        150,  // max_upline_levels
        200,  // max_downline_levels
        ReachMode::Bidirectional,
    ));
}

// ============================================================================
// set_single_line_config 测试
// ============================================================================

#[test]
fn set_config_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 100);
        assert_eq!(config.downline_rate, 100);
        assert_eq!(config.base_upline_levels, 10);
        assert_eq!(config.base_downline_levels, 15);
        assert_eq!(config.level_increment_threshold, 1000);
        assert_eq!(config.max_upline_levels, 150);
        assert_eq!(config.max_downline_levels, 200);
    });
}

#[test]
fn set_config_by_admin_works() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(ADMIN),
            1,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_some());
    });
}

#[test]
fn set_config_rejects_no_entity() {
    new_test_ext().execute_with(|| {
        // entity not registered
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(NOBODY),
                1,
                100,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::EntityNotFound,
        );
    });
}

#[test]
fn set_config_rejects_non_owner_non_admin() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(NOBODY),
                1,
                100,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

#[test]
fn set_config_rejects_invalid_upline_rate() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                1001,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::InvalidRate,
        );
    });
}

#[test]
fn set_config_rejects_invalid_downline_rate() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                1001,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::InvalidRate,
        );
    });
}

#[test]
fn set_config_boundary_rate_1000_ok() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        // rate=1000 is the per-level max; total must also pass RatesTooHigh check
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            1000,
            1000,
            10,
            15,
            1000,
            50,
            50,
            ReachMode::Bidirectional,
        ));
    });
}

#[test]
fn set_config_emits_event() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigUpdated {
                entity_id: 1,
                upline_rate: 100,
                downline_rate: 100,
                base_upline_levels: 10,
                base_downline_levels: 15,
                max_upline_levels: 150,
                max_downline_levels: 200,
                reach_mode: ReachMode::Bidirectional,
            }
            .into(),
        );
    });
}

// ============================================================================
// add_to_single_line 测试
// ============================================================================

#[test]
fn add_to_single_line_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::add_to_single_line(1, &10));
        assert_ok!(SingleLine::add_to_single_line(1, &20));
        assert_ok!(SingleLine::add_to_single_line(1, &30));

        assert_eq!(pallet::SingleLineIndex::<Test>::get(1, 10), Some(0));
        assert_eq!(pallet::SingleLineIndex::<Test>::get(1, 20), Some(1));
        assert_eq!(pallet::SingleLineIndex::<Test>::get(1, 30), Some(2));

        let seg = pallet::SingleLineSegments::<Test>::get(1, 0u32);
        assert_eq!(seg.len(), 3);
        assert_eq!(seg[0], 10);
        assert_eq!(seg[1], 20);
        assert_eq!(seg[2], 30);
    });
}

#[test]
fn add_to_single_line_idempotent() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::add_to_single_line(1, &10));
        assert_ok!(SingleLine::add_to_single_line(1, &10));
        assert_eq!(SingleLine::single_line_length(1), 1);
    });
}

#[test]
fn add_to_single_line_cross_entity_isolation() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::add_to_single_line(1, &10));
        assert_ok!(SingleLine::add_to_single_line(2, &10));

        assert_eq!(pallet::SingleLineIndex::<Test>::get(1, 10), Some(0));
        assert_eq!(pallet::SingleLineIndex::<Test>::get(2, 10), Some(0));

        assert_eq!(SingleLine::single_line_length(1), 1);
        assert_eq!(SingleLine::single_line_length(2), 1);
    });
}

// ============================================================================
// calc_extra_levels 测试
// ============================================================================

#[test]
fn calc_extra_levels_zero_threshold() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::calc_extra_levels(0u128, 5000), 0);
    });
}

#[test]
fn calc_extra_levels_basic() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::calc_extra_levels(1000u128, 0), 0);
        assert_eq!(SingleLine::calc_extra_levels(1000u128, 999), 0);
        assert_eq!(SingleLine::calc_extra_levels(1000u128, 1000), 1);
        assert_eq!(SingleLine::calc_extra_levels(1000u128, 5500), 5);
    });
}

#[test]
fn calc_extra_levels_capped_at_255() {
    new_test_ext().execute_with(|| {
        // H4 审计修复: 大值不溢出 u8
        assert_eq!(SingleLine::calc_extra_levels(1u128, 1000), 255);
    });
}

// ============================================================================
// process_upline 测试
// ============================================================================

#[test]
fn process_upline_basic() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 单链: [10, 20, 30, 40, 50]
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();

        // buyer=50 (index=4), upline_rate=100 (1%), base_upline_levels=10
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 向上 4 层: 40,30,20,10，每层 100000 * 100 / 10000 = 1000
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].beneficiary, 40);
        assert_eq!(outputs[0].amount, 1000);
        assert_eq!(outputs[0].commission_type, CommissionType::SingleLineUpline);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 30);
        assert_eq!(outputs[1].amount, 1000);
        assert_eq!(outputs[2].beneficiary, 20);
        assert_eq!(outputs[2].amount, 1000);
        assert_eq!(outputs[3].beneficiary, 10);
        assert_eq!(outputs[3].amount, 1000);
        assert_eq!(remaining, 100_000 - 4000);
    });
}

#[test]
fn process_upline_buyer_at_index_0_no_output() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 10000;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=10 是第一个人 (index=0)，没有上线
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_upline(
            entity_id,
            &10,
            10000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn process_upline_buyer_not_in_line_no_output() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 10000;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=99 不在单链中
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &99, &config);
        SingleLine::process_upline(
            entity_id,
            &99,
            10000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert!(outputs.is_empty());
    });
}

#[test]
fn process_upline_capped_by_remaining() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        // remaining 只有 1500，每层 1000
        let mut remaining: u128 = 1500;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 40: 1000, remaining=500
        // 30: min(1000, 500)=500, remaining=0
        // 后续 actual=0 → 不输出
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].amount, 1000);
        assert_eq!(outputs[1].amount, 500);
        assert_eq!(remaining, 0);
    });
}

#[test]
fn process_upline_beneficiary_only_ignores_buyer_max_upline_window() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            4,
            4,
            ReachMode::BeneficiaryOnly,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        set_member_stats(entity_id, 1, 5000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &6, &config);
        SingleLine::process_upline(
            entity_id,
            &6,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 5);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 1);
        assert_eq!(outputs[1].level, 5);
    });
}

#[test]
fn process_upline_beneficiary_only_depends_only_on_beneficiary_reverse_reach() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            5,
            5,
            1000,
            5,
            5,
            ReachMode::BeneficiaryOnly,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        set_member_stats(entity_id, 2, 10_000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &6, &config);
        SingleLine::process_upline(
            entity_id,
            &6,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 5);
        assert_eq!(outputs[0].beneficiary, 5);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[4].beneficiary, 1);
        assert_eq!(outputs[4].level, 5);
        assert_eq!(remaining, 95_000);
    });
}

#[test]
fn process_downline_beneficiary_only_ignores_buyer_max_downline_window() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            4,
            4,
            ReachMode::BeneficiaryOnly,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        set_member_stats(entity_id, 6, 5000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 6);
        assert_eq!(outputs[1].level, 5);
    });
}

#[test]
fn process_downline_beneficiary_only_depends_only_on_beneficiary_reverse_reach() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            5,
            5,
            1000,
            5,
            5,
            ReachMode::BeneficiaryOnly,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        set_member_stats(entity_id, 5, 10_000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 5);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[4].beneficiary, 6);
        assert_eq!(outputs[4].level, 5);
        assert_eq!(remaining, 95_000);
    });
}

#[test]
fn process_downline_basic() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 单链: [10, 20, 30, 40, 50]
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();

        // buyer=20 (index=1), downline_rate=100 (1%), base_downline_levels=15
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &20, &config);
        SingleLine::process_downline(
            entity_id,
            &20,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 向下 3 层: 30,40,50，每层 1000
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 30);
        assert_eq!(
            outputs[0].commission_type,
            CommissionType::SingleLineDownline
        );
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 40);
        assert_eq!(outputs[2].beneficiary, 50);
        assert_eq!(remaining, 100_000 - 3000);
    });
}

#[test]
fn process_downline_zero_rate_no_output() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            0,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 10000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_downline(
            entity_id,
            &10,
            10000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert!(outputs.is_empty());
    });
}

#[test]
fn process_downline_last_in_line_no_output() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 10000;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=30 是最后一个人，没有下线
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &30, &config);
        SingleLine::process_downline(
            entity_id,
            &30,
            10000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert!(outputs.is_empty());
    });
}

#[test]
fn process_downline_capped_by_remaining() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 2500;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=10 (index=0), 下线: 20,30,40,50 每层 1000
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_downline(
            entity_id,
            &10,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 20: 1000, remaining=1500
        // 30: 1000, remaining=500
        // 40: min(1000,500)=500, remaining=0
        // 50: actual=0 → 不输出
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].amount, 1000);
        assert_eq!(outputs[1].amount, 1000);
        assert_eq!(outputs[2].amount, 500);
        assert_eq!(remaining, 0);
    });
}

#[test]
fn process_downline_dynamic_levels_with_extra() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base_downline_levels=1, threshold=500
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            1,
            500,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        // buyer=1 (index=0), earned=1500 → extra=3, effective=min(1+3,200)=4
        set_member_stats(entity_id, 1, 1500);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 下线有 4 个人 (index 1..4)，effective=4 → 全部覆盖
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[3].beneficiary, 5);
    });
}

// ============================================================================
// CommissionPlugin trait 测试
// ============================================================================

#[test]
fn plugin_no_config_returns_remaining() {
    new_test_ext().execute_with(|| {
        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &10, 10000, 10000, modes, false, 1, 0,
        );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn plugin_mode_not_enabled_returns_remaining() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // 只启用 DIRECT_REWARD，不含单线模式
        let modes = CommissionModes(CommissionModes::DIRECT_REWARD);
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &10, 10000, 10000, modes, false, 1, 0,
        );
        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn plugin_upline_only() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 1, 0,
        );

        // 上线: 20(level=1), 10(level=2), 每层 1000
        assert_eq!(outputs.len(), 2);
        assert!(outputs
            .iter()
            .all(|o| o.commission_type == CommissionType::SingleLineUpline));
        assert_eq!(remaining, 100_000 - 2000);
    });
}

#[test]
fn plugin_downline_only() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_DOWNLINE);
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, false, 1, 0,
        );

        // 下线: 20(level=1), 30(level=2), 每层 1000
        assert_eq!(outputs.len(), 2);
        assert!(outputs
            .iter()
            .all(|o| o.commission_type == CommissionType::SingleLineDownline));
        assert_eq!(remaining, 100_000 - 2000);
    });
}

#[test]
fn plugin_both_upline_and_downline() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 1, 0,
        );

        // 上线: 20,10 → 2 outputs
        // 下线: 40,50 → 2 outputs
        // 每层 1000，共 4000
        let upline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .count();
        let downline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineDownline)
            .count();
        assert_eq!(upline_count, 2);
        assert_eq!(downline_count, 2);
        assert_eq!(remaining, 100_000 - 4000);
    });
}

#[test]
fn plugin_first_order_adds_to_single_line() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 先建链 [20, 30]，让新买家 10 首单时有上线可分佣
        setup_single_line(entity_id, &[20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        // is_first_order=true → 应先加入单链再计算分佣
        let (outputs, _remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, true, 1, 0,
        );

        // buyer 10 应加入链尾 (index=2)
        assert_eq!(pallet::SingleLineIndex::<Test>::get(entity_id, 10), Some(2));
        assert_eq!(SingleLine::single_line_length(entity_id), 3);
        // 首单也应产生 upline 分佣（给上线 30, 20）
        let upline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .count();
        assert!(
            upline_count > 0,
            "first order should produce upline commission"
        );
    });
}

#[test]
fn plugin_not_first_order_still_adds_if_not_in_line() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        // F5 修复后: is_first_order 不再控制加入，未在链中的用户每次消费都自动尝试加入
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, false, 1, 0,
        );

        assert_eq!(pallet::SingleLineIndex::<Test>::get(entity_id, 10), Some(0));
    });
}

// ============================================================================
// TokenCommissionPlugin trait 测试
// ============================================================================

#[test]
fn token_plugin_upline_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, remaining) =
            <SingleLine as TokenCommissionPlugin<u64, u128>>::calculate_token(
                entity_id,
                &30,
                100_000u128,
                100_000u128,
                modes,
                false,
                1,
                0,
            );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[0].amount, 1000u128);
        assert_eq!(outputs[1].beneficiary, 10);
        assert_eq!(remaining, 100_000 - 2000);
    });
}

#[test]
fn token_plugin_downline_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_DOWNLINE);
        let (outputs, remaining) =
            <SingleLine as TokenCommissionPlugin<u64, u128>>::calculate_token(
                entity_id,
                &10,
                100_000u128,
                100_000u128,
                modes,
                false,
                1,
                0,
            );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[1].beneficiary, 30);
        assert_eq!(remaining, 100_000 - 2000);
    });
}

#[test]
fn token_plugin_first_order_adds_to_single_line() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 先建链 [10, 20]，让新买家 99 首单时有上线可分佣
        setup_single_line(entity_id, &[10, 20]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, _remaining) =
            <SingleLine as TokenCommissionPlugin<u64, u128>>::calculate_token(
                entity_id, &99, 10000u128, 10000u128, modes, true, 1, 0,
            );

        // buyer 99 应加入链尾 (index=2)
        assert_eq!(pallet::SingleLineIndex::<Test>::get(entity_id, 99), Some(2));
        assert_eq!(SingleLine::single_line_length(entity_id), 3);
        // 首单也应产生 upline 分佣
        let upline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .count();
        assert!(
            upline_count > 0,
            "token first order should produce upline commission"
        );
    });
}

// ============================================================================
// 边界 / 集成测试
// ============================================================================

#[test]
fn upline_levels_capped_by_max_upline_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // M2 审计修复后 base 不能超过 max，改用直接写入存储测试运行时钳位行为
        pallet::SingleLineConfigs::<Test>::insert(
            entity_id,
            pallet::SingleLineConfig {
                upline_rate: 100,
                downline_rate: 100,
                base_upline_levels: 10,
                base_downline_levels: 15,
                level_increment_threshold: 1000u128,
                max_upline_levels: 2,
                max_downline_levels: 200,
                reach_mode: ReachMode::Bidirectional,
            },
        );
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=5 (index=4), base=10 but max=2 → clamped to 2
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 4); // level=1
        assert_eq!(outputs[1].beneficiary, 3); // level=2
    });
}

#[test]
fn downline_levels_capped_by_max_downline_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // M2 审计修复后 base 不能超过 max，改用直接写入存储测试运行时钳位行为
        pallet::SingleLineConfigs::<Test>::insert(
            entity_id,
            pallet::SingleLineConfig {
                upline_rate: 100,
                downline_rate: 100,
                base_upline_levels: 10,
                base_downline_levels: 15,
                level_increment_threshold: 1000u128,
                max_upline_levels: 150,
                max_downline_levels: 1,
                reach_mode: ReachMode::Bidirectional,
            },
        );
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        // buyer=1 (index=0), base=15 but max=1 → clamped to 1
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 2);
    });
}

#[test]
fn commission_based_on_order_amount_not_remaining() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        // order_amount=100000, remaining=50000
        let mut remaining: u128 = 50_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &20, &config);
        SingleLine::process_upline(
            entity_id,
            &20,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // commission = 100000 * 100 / 10000 = 1000 (基于 order_amount)
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].amount, 1000);
    });
}

#[test]
fn single_line_auto_extends_on_full_segment() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        // MaxSingleLineLength=200，填满第一段
        for i in 0..200u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 1);

        // 第 201 个自动创建新段
        assert_ok!(SingleLine::add_to_single_line(entity_id, &500));
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);
        assert_eq!(SingleLine::single_line_length(entity_id), 201);
        assert_eq!(SingleLine::user_position(entity_id, &500), Some(200));

        // 通过 plugin 路径也能成功加入
        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &201, 10000, 10000, modes, true, 1, 0,
        );
        assert_eq!(SingleLine::user_position(entity_id, &201), Some(201));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::NewSegmentCreated {
                entity_id,
                segment_id: 1,
            }
            .into(),
        );
    });
}

#[test]
fn default_config_values() {
    new_test_ext().execute_with(|| {
        let config = pallet::SingleLineConfig::<u128>::default();
        assert_eq!(config.upline_rate, 0);
        assert_eq!(config.downline_rate, 0);
        assert_eq!(config.base_upline_levels, 0);
        assert_eq!(config.base_downline_levels, 0);
        assert_eq!(config.level_increment_threshold, 0);
        assert_eq!(config.max_upline_levels, 0);
        assert_eq!(config.max_downline_levels, 0);
    });
}

// ============================================================================
// 按会员等级自定义层数测试
// ============================================================================

#[test]
fn set_level_based_levels_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            10,
            12,
        ));
        let overrides = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).unwrap();
        assert_eq!(overrides.upline_levels, 10);
        assert_eq!(overrides.downline_levels, 12);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsUpdated {
                entity_id: 1,
                level_id: 2,
            }
            .into(),
        );
    });
}

#[test]
fn set_level_based_levels_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(NOBODY), 1, 0, 20, 25),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

#[test]
fn set_level_based_levels_rejects_both_zero() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 0, 0, 0),
            pallet::Error::<Test>::InvalidLevels,
        );
    });
}

#[test]
fn remove_level_based_levels_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            3,
            8,
            10,
        ));
        assert_ok!(SingleLine::remove_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            3,
        ));
        assert_eq!(
            pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 3),
            None
        );
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsRemoved {
                entity_id: 1,
                level_id: 3,
            }
            .into(),
        );
    });
}

#[test]
fn remove_nonexistent_level_no_event() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        // M3 修复: 不存在的覆盖不应发出事件
        assert_ok!(SingleLine::remove_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            99,
        ));
        // 只有 setup_entity 不产生 pallet 事件
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| matches!(e.event, RuntimeEvent::CommissionSingleLine(_)))
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

#[test]
fn level_override_affects_upline_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base_upline_levels=2, max=150
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            2,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 自定义等级 1 上线层数=5
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            1,
            5,
            2,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7]);

        // buyer=7 (index=6) 无等级 → 使用 base=2
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &7, &config);
        SingleLine::process_upline(
            entity_id,
            &7,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 2); // base=2

        // buyer=7 设为自定义等级 1 → 使用 override=5
        set_custom_level(entity_id, 7, 1);
        remaining = 100_000;
        outputs.clear();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &7, &config);
        SingleLine::process_upline(
            entity_id,
            &7,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 5); // override=5
        assert_eq!(outputs[0].beneficiary, 6);
        assert_eq!(outputs[4].beneficiary, 2);
    });
}

#[test]
fn level_override_affects_downline_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base_downline_levels=1, max=200
        // Use BuyerOnly to isolate level-override behavior from bidirectional reach
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            1,
            1000,
            150,
            200,
            ReachMode::BuyerOnly,
        ));
        // 自定义等级 2 下线层数=4
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            2,
            2,
            4,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        // buyer=1 (index=0) 无等级 → base=1
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 1); // base=1

        // buyer=1 设为自定义等级 2 → override=4
        set_custom_level(entity_id, 1, 2);
        remaining = 100_000;
        outputs.clear();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);
        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 4); // override=4
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[3].beneficiary, 5);
    });
}

#[test]
fn level_override_fallback_when_no_override_for_level() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base=2(config), threshold=1000, max=150
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            2,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 自定义等级 1 override upline=3
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            1,
            3,
            2,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7]);

        // buyer=7 自定义等级 3（无 override）→ 回退到 base=2
        set_custom_level(entity_id, 7, 3);
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &7, &config);
        SingleLine::process_upline(
            entity_id,
            &7,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 2); // fallback base=2
    });
}

#[test]
fn level_override_still_capped_by_max() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // max_upline_levels=3
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            2,
            1000,
            3,
            200,
            ReachMode::Bidirectional,
        ));
        // P1-4: override upline=10 超过 max=3 → 应被拒绝
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), entity_id, 0, 10, 2,),
            pallet::Error::<Test>::LevelOverrideExceedsMax
        );
        // override=3 等于 max=3 → 允许
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            0,
            3,
            2,
        ));

        // 运行时 min() 仍然保证 clamp（直接写入存储绕过校验时）
        pallet::SingleLineCustomLevelOverrides::<Test>::insert(
            entity_id,
            1,
            pallet::LevelBasedLevels {
                upline_levels: 10,
                downline_levels: 2,
            },
        );
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7]);
        set_custom_level(entity_id, 7, 1);
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &7, &config);
        SingleLine::process_upline(
            entity_id,
            &7,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        // override=10 但 max=3 → 运行时 clamp 到 3 层
        assert_eq!(outputs.len(), 3);
    });
}

#[test]
fn level_override_combined_with_extra_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base=2(config), threshold=1000, max=150
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            2,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 自定义等级 1 override upline=3
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            1,
            3,
            2,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7, 8]);

        // buyer=8 (index=7), 自定义等级 1 → override=3, earned=2000 → extra=2
        // effective = min(3+2, 150) = 5
        set_custom_level(entity_id, 8, 1);
        set_member_stats(entity_id, 8, 2000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &8, &config);
        SingleLine::process_upline(
            entity_id,
            &8,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 5); // override(3) + extra(2) = 5
        assert_eq!(outputs[0].beneficiary, 7);
        assert_eq!(outputs[4].beneficiary, 3);
    });
}

#[test]
fn plugin_with_level_override_integration() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // base=1, max=150/200
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 自定义等级 2 → upline=3, downline=4
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            2,
            3,
            4,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        // buyer=5 (index=4), 自定义等级 2
        set_custom_level(entity_id, 5, 2);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &5, 100_000, 100_000, modes, false, 1, 0,
        );

        // 上线: 3层 (4,3,2)
        // 下线: 4层 (6,7,8,9) — 有5个下线但只取4层
        let upline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .count();
        let downline_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineDownline)
            .count();
        assert_eq!(upline_count, 3);
        assert_eq!(downline_count, 4);
        // 每层 1000, 共 7000
        assert_eq!(remaining, 100_000 - 7000);
    });
}

/// 测试不同自定义等级的覆盖互不干扰
#[test]
fn different_custom_level_overrides_are_isolated() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 自定义等级 1 → upline=6
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            1,
            6,
            1,
        ));
        // 自定义等级 2 → upline=3
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            2,
            3,
            1,
        ));
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7, 8]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();

        // buyer=8 自定义等级 1 → 应查 CustomOverrides 得 6
        set_custom_level(entity_id, 8, 1);
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &8, &config);
        assert_eq!(base_up, 6);

        // 切换为自定义等级 2 → 应查 CustomOverrides 得 3
        set_custom_level(entity_id, 8, 2);
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &8, &config);
        assert_eq!(base_up, 3);
    });
}

// ============================================================================
// 深度审计回归测试
// ============================================================================

#[test]
fn m1_deep_add_to_single_line_emits_event() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        // 加入单链应发射 AddedToSingleLine 事件
        System::reset_events();
        assert_ok!(SingleLine::add_to_single_line(entity_id, &10));
        assert_ok!(SingleLine::add_to_single_line(entity_id, &20));

        let events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter_map(|e| {
                if let RuntimeEvent::CommissionSingleLine(inner) = e.event {
                    Some(inner)
                } else {
                    None
                }
            })
            .collect();

        // 第一次加入创建新段 → NewSegmentCreated + AddedToSingleLine
        // 第二次加入同段 → AddedToSingleLine
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0],
            pallet::Event::NewSegmentCreated {
                entity_id,
                segment_id: 0
            }
        );
        assert_eq!(
            events[1],
            pallet::Event::AddedToSingleLine {
                entity_id,
                account: 10,
                index: 0
            }
        );
        assert_eq!(
            events[2],
            pallet::Event::AddedToSingleLine {
                entity_id,
                account: 20,
                index: 1
            }
        );

        // 重复加入不发射事件
        System::reset_events();
        assert_ok!(SingleLine::add_to_single_line(entity_id, &10));
        let events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter_map(|e| {
                if let RuntimeEvent::CommissionSingleLine(inner) = e.event {
                    Some(inner)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(events.len(), 0);
    });
}

#[test]
fn m2_deep_set_config_rejects_base_upline_exceeds_max() {
    new_test_ext().execute_with(|| {
        // base_upline_levels=20 > max_upline_levels=10 → 应拒绝
        setup_entity(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                100,
                20,
                5,
                1000,
                10,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::BaseLevelsExceedMax
        );
    });
}

#[test]
fn m2_deep_set_config_rejects_base_downline_exceeds_max() {
    new_test_ext().execute_with(|| {
        // base_downline_levels=30 > max_downline_levels=5 → 应拒绝
        setup_entity(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                100,
                5,
                30,
                1000,
                150,
                5,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::BaseLevelsExceedMax
        );
    });
}

#[test]
fn m1_r3_shared_line_upline_downline_consistent() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 单链: [1, 2, 3, 4, 5]
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        // M1-R3: upline + downline 加载段数据
        let (base_up, base_down) = SingleLine::effective_base_levels(entity_id, &3, &config);

        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();

        // buyer=3 (index=2): upline=[2,1], downline=[4,5]
        SingleLine::process_upline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        SingleLine::process_downline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        let up_out: alloc::vec::Vec<_> = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .collect();
        let dn_out: alloc::vec::Vec<_> = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineDownline)
            .collect();

        assert_eq!(up_out.len(), 2); // 2,1
        assert_eq!(up_out[0].beneficiary, 2);
        assert_eq!(up_out[1].beneficiary, 1);
        assert_eq!(dn_out.len(), 2); // 4,5
        assert_eq!(dn_out[0].beneficiary, 4);
        assert_eq!(dn_out[1].beneficiary, 5);
        // 每层 1000, 共 4 层 = 4000
        assert_eq!(remaining, 100_000 - 4000);

        // 验证与 CommissionPlugin::calculate 结果一致
        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (plugin_outputs, plugin_remaining) =
            <SingleLine as CommissionPlugin<u64, u128>>::calculate(
                entity_id, &3, 100_000, 100_000, modes, false, 1, 0,
            );
        assert_eq!(plugin_outputs.len(), outputs.len());
        assert_eq!(plugin_remaining, remaining);
    });
}

#[test]
fn m2_deep_set_config_base_equals_max_ok() {
    new_test_ext().execute_with(|| {
        // base == max 应该成功
        setup_entity(1);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            10,
            15,
            1000,
            10,
            15,
            ReachMode::Bidirectional,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1u64).unwrap();
        assert_eq!(config.base_upline_levels, 10);
        assert_eq!(config.max_upline_levels, 10);
        assert_eq!(config.base_downline_levels, 15);
        assert_eq!(config.max_downline_levels, 15);
    });
}

// ============================================================================
// force_set_single_line_config 测试
// ============================================================================

#[test]
fn force_set_config_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::force_set_single_line_config(
            RuntimeOrigin::root(),
            1,
            200,
            300,
            5,
            10,
            500,
            50,
            100,
            ReachMode::Bidirectional,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 200);
        assert_eq!(config.downline_rate, 300);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigUpdated {
                entity_id: 1,
                upline_rate: 200,
                downline_rate: 300,
                base_upline_levels: 5,
                base_downline_levels: 10,
                max_upline_levels: 50,
                max_downline_levels: 100,
                reach_mode: ReachMode::Bidirectional,
            }
            .into(),
        );
    });
}

#[test]
fn force_set_config_rejects_signed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SingleLine::force_set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            frame_support::error::BadOrigin,
        );
    });
}

#[test]
fn force_set_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SingleLine::force_set_single_line_config(
                RuntimeOrigin::root(),
                1,
                1001,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::InvalidRate,
        );
    });
}

// ============================================================================
// force_clear_single_line_config 测试
// ============================================================================

#[test]
fn force_clear_config_works() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::force_set_single_line_config(
            RuntimeOrigin::root(),
            1,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_some());

        assert_ok!(SingleLine::force_clear_single_line_config(
            RuntimeOrigin::root(),
            1,
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigCleared { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn force_clear_nonexistent_is_noop() {
    new_test_ext().execute_with(|| {
        // 不存在配置时 force_clear 不报错也不发事件
        assert_ok!(SingleLine::force_clear_single_line_config(
            RuntimeOrigin::root(),
            99,
        ));
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| matches!(e.event, RuntimeEvent::CommissionSingleLine(_)))
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

#[test]
fn force_clear_rejects_signed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SingleLine::force_clear_single_line_config(RuntimeOrigin::signed(OWNER), 1,),
            frame_support::error::BadOrigin,
        );
    });
}

// ============================================================================
// clear_single_line_config 测试（Owner/Admin）
// ============================================================================

#[test]
fn clear_config_by_owner_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_some());

        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigCleared { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn clear_config_by_admin_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(ADMIN),
            1,
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
    });
}

#[test]
fn clear_config_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::clear_single_line_config(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

#[test]
fn clear_config_rejects_not_found() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        // 未设置配置
        assert_noop!(
            SingleLine::clear_single_line_config(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::ConfigNotFound,
        );
    });
}

// ============================================================================
// update_single_line_params 测试
// ============================================================================

#[test]
fn update_params_upline_rate_only() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            Some(500),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 500);
        assert_eq!(config.downline_rate, 100); // unchanged
    });
}

#[test]
fn update_params_downline_rate_only() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // 100*150 + 200*200 = 55_000 <= 100_000
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            None,
            Some(200),
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 100); // unchanged
        assert_eq!(config.downline_rate, 200);
    });
}

#[test]
fn update_params_threshold_only() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            None,
            None,
            Some(9999),
            None,
            None,
            None,
            None,
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.level_increment_threshold, 9999);
        assert_eq!(config.upline_rate, 100); // unchanged
    });
}

#[test]
fn update_params_multiple() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            Some(200),
            Some(300),
            Some(5000),
            None,
            None,
            None,
            None,
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 200);
        assert_eq!(config.downline_rate, 300);
        assert_eq!(config.level_increment_threshold, 5000);
    });
}

#[test]
fn update_params_rejects_nothing_to_update() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::NothingToUpdate,
        );
    });
}

#[test]
fn update_params_rejects_config_not_found() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                Some(100),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::ConfigNotFound,
        );
    });
}

#[test]
fn update_params_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                Some(1001),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::InvalidRate,
        );
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                None,
                Some(1001),
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::InvalidRate,
        );
    });
}

#[test]
fn update_params_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(NOBODY),
                1,
                Some(100),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

#[test]
fn update_params_emits_event() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        System::reset_events();
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            Some(200),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigUpdated {
                entity_id: 1,
                upline_rate: 200,
                downline_rate: 100,
                base_upline_levels: 10,
                base_downline_levels: 15,
                max_upline_levels: 150,
                max_downline_levels: 200,
                reach_mode: ReachMode::Bidirectional,
            }
            .into(),
        );
    });
}

// ============================================================================
// is_banned 受益人检查测试
// ============================================================================

#[test]
fn banned_upline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 单链: [10, 20, 30, 40, 50]
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // ban 40 (index=3)
        set_banned(entity_id, 40);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 上线: 40(banned→skip), 30, 20, 10 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 30); // 40 was skipped
        assert_eq!(outputs[1].beneficiary, 20);
        assert_eq!(outputs[2].beneficiary, 10);
    });
}

#[test]
fn banned_downline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // ban 30 (index=2)
        set_banned(entity_id, 30);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_downline(
            entity_id,
            &10,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 下线: 20, 30(banned→skip), 40, 50 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[1].beneficiary, 40); // 30 was skipped
        assert_eq!(outputs[2].beneficiary, 50);
    });
}

#[test]
fn banned_via_plugin_integration() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        set_banned(entity_id, 20);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, _remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 1, 0,
        );

        // 上线: 20(banned), 10 → only 10
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 10);
    });
}

// ============================================================================
// SingleLinePlanWriter 测试
// ============================================================================

#[test]
fn plan_writer_set_config_works() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;

        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 200, 300, 5, 10, 2000, 50, 100, 0u8,
            )
        );
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 200);
        assert_eq!(config.downline_rate, 300);
        assert_eq!(config.base_upline_levels, 5);
        assert_eq!(config.base_downline_levels, 10);
        assert_eq!(config.level_increment_threshold, 2000);
        assert_eq!(config.max_upline_levels, 50);
        assert_eq!(config.max_downline_levels, 100);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigUpdated {
                entity_id: 1,
                upline_rate: 200,
                downline_rate: 300,
                base_upline_levels: 5,
                base_downline_levels: 10,
                max_upline_levels: 50,
                max_downline_levels: 100,
                reach_mode: ReachMode::Bidirectional,
            }
            .into(),
        );
    });
}

#[test]
fn plan_writer_set_config_rejects_invalid_rate() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;

        assert!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 1001, 100, 5, 10, 2000, 50, 100, 0u8,
            )
            .is_err()
        );
    });
}

#[test]
fn plan_writer_set_config_rejects_base_exceeds_max() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;

        assert!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 20, 10, 2000, 10, 100, 0u8,
            )
            .is_err()
        );
    });
}

#[test]
fn plan_writer_clear_config_works() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;

        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 10, 2000, 50, 100, 0u8,
            )
        );
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_some());

        assert_ok!(<SingleLine as SingleLinePlanWriter>::clear_config(1));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineConfigCleared { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn plan_writer_clear_nonexistent_is_noop() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;

        assert_ok!(<SingleLine as SingleLinePlanWriter>::clear_config(99));
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| matches!(e.event, RuntimeEvent::CommissionSingleLine(_)))
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

// ==================== EntityLocked 回归测试 ====================

#[test]
fn entity_locked_rejects_set_single_line_config() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_locked(1);
        assert_noop!(
            CommissionSingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::EntityLocked
        );
    });
}

#[test]
fn entity_locked_rejects_clear_single_line_config() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        // 先设置配置
        assert_ok!(CommissionSingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        // 锁定后无法清除
        set_entity_locked(1);
        assert_noop!(
            CommissionSingleLine::clear_single_line_config(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityLocked
        );
    });
}

// ============================================================================
// F1: Entity 活跃状态检查测试
// ============================================================================

#[test]
fn f1_set_config_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                100,
                100,
                10,
                15,
                1000,
                150,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn f1_clear_config_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::clear_single_line_config(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn f1_update_params_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                Some(200),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn f1_set_level_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1, 5, 5),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn f1_remove_level_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::remove_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn f1_do_calculate_skips_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        setup_single_line(1, &[10, 20, 30]);
        set_entity_inactive(1);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &30, 100_000, 100_000, modes, false, 1, 0,
        );
        assert_eq!(outputs.len(), 0);
        assert_eq!(remaining, 100_000);
    });
}

// ============================================================================
// F2: 单线收益暂停/恢复测试
// ============================================================================

#[test]
fn f2_pause_single_line_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert!(SingleLine::is_single_line_enabled(1));

        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert!(!SingleLine::is_single_line_enabled(1));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLinePaused { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn f2_resume_single_line_works() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));

        assert_ok!(SingleLine::resume_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert!(SingleLine::is_single_line_enabled(1));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineResumed { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn f2_pause_rejects_already_paused() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert_noop!(
            SingleLine::pause_single_line(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::SingleLineIsPaused,
        );
    });
}

#[test]
fn f2_resume_rejects_not_paused() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::resume_single_line(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::SingleLineNotPaused,
        );
    });
}

#[test]
fn f2_paused_skips_commission_calculation() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        setup_single_line(1, &[10, 20, 30]);
        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &30, 100_000, 100_000, modes, false, 1, 0,
        );
        assert_eq!(outputs.len(), 0);
        assert_eq!(remaining, 100_000);
    });
}

#[test]
fn f2_resumed_restores_commission_calculation() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        setup_single_line(1, &[10, 20, 30]);
        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert_ok!(SingleLine::resume_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, _remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &30, 100_000, 100_000, modes, false, 1, 0,
        );
        assert!(outputs.len() > 0);
    });
}

#[test]
fn f2_pause_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::pause_single_line(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

// ============================================================================
// F3: update_params 支持 level 参数测试
// ============================================================================

#[test]
fn f3_update_base_upline_levels() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            None,
            None,
            None,
            Some(20),
            None,
            None,
            None,
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.base_upline_levels, 20);
        assert_eq!(config.base_downline_levels, 15);
    });
}

#[test]
fn f3_update_max_levels() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            None,
            None,
            None,
            None,
            None,
            Some(200),
            Some(250),
            None,
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.max_upline_levels, 200);
        assert_eq!(config.max_downline_levels, 250);
    });
}

#[test]
fn f3_update_rejects_base_exceeds_max() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                None,
                None,
                None,
                Some(200),
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::BaseLevelsExceedMax,
        );
    });
}

#[test]
fn f3_update_rejects_max_below_base() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                None,
                None,
                None,
                None,
                None,
                Some(5),
                None,
                None,
            ),
            pallet::Error::<Test>::BaseLevelsExceedMax,
        );
    });
}

// ============================================================================
// F4: 查询辅助函数测试
// ============================================================================

#[test]
fn f4_single_line_length() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::single_line_length(1), 0);
        setup_single_line(1, &[10, 20, 30]);
        assert_eq!(SingleLine::single_line_length(1), 3);
    });
}

#[test]
fn f4_single_line_remaining_capacity() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::single_line_remaining_capacity(1), 200);
        setup_single_line(1, &[10, 20, 30]);
        assert_eq!(SingleLine::single_line_remaining_capacity(1), 197);
    });
}

#[test]
fn f4_user_position() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::user_position(1, &10), None);
        setup_single_line(1, &[10, 20, 30]);
        assert_eq!(SingleLine::user_position(1, &10), Some(0));
        assert_eq!(SingleLine::user_position(1, &20), Some(1));
        assert_eq!(SingleLine::user_position(1, &30), Some(2));
        assert_eq!(SingleLine::user_position(1, &99), None);
    });
}

#[test]
fn f4_user_effective_levels() {
    new_test_ext().execute_with(|| {
        assert_eq!(SingleLine::user_effective_levels(1, &10), None);
        setup_config(1);
        assert_eq!(SingleLine::user_effective_levels(1, &10), Some((10, 15)));
        set_member_stats(1, 10, 5000);
        assert_eq!(SingleLine::user_effective_levels(1, &10), Some((15, 20)));
    });
}

// ============================================================================
// F5: 单链满后返回错误测试
// ============================================================================

#[test]
fn f5_add_to_single_line_auto_extends_past_segment() {
    new_test_ext().execute_with(|| {
        for i in 0..200u64 {
            assert_ok!(SingleLine::add_to_single_line(1, &i));
        }
        assert_eq!(SingleLine::single_line_length(1), 200);
        // 段满后自动扩展，不再报错
        assert_ok!(SingleLine::add_to_single_line(1, &200));
        assert_eq!(SingleLine::single_line_length(1), 201);
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(1), 2);
    });
}

// ============================================================================
// F7: force_reset_single_line 测试
// ============================================================================

#[test]
fn f7_force_reset_single_line_works() {
    new_test_ext().execute_with(|| {
        setup_single_line(1, &[10, 20, 30, 40, 50]);
        assert_eq!(SingleLine::single_line_length(1), 5);

        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            1,
            u32::MAX
        ));
        assert_eq!(SingleLine::single_line_length(1), 0);
        assert_eq!(SingleLine::user_position(1, &10), None);
        assert_eq!(SingleLine::user_position(1, &50), None);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineReset {
                entity_id: 1,
                removed_count: 5,
            }
            .into(),
        );
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineResetCompleted { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn f7_force_reset_empty_is_noop() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            99,
            u32::MAX
        ));
        // empty chain → only SingleLineResetCompleted, no SingleLineReset
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineResetCompleted { entity_id: 99 }.into(),
        );
    });
}

#[test]
fn f7_force_reset_rejects_signed() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SingleLine::force_reset_single_line(RuntimeOrigin::signed(OWNER), 1, u32::MAX),
            frame_support::error::BadOrigin,
        );
    });
}

#[test]
fn f7_force_reset_allows_re_add() {
    new_test_ext().execute_with(|| {
        setup_single_line(1, &[10, 20, 30]);
        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            1,
            u32::MAX
        ));
        assert_ok!(SingleLine::add_to_single_line(1, &10));
        assert_eq!(SingleLine::user_position(1, &10), Some(0));
        assert_eq!(SingleLine::single_line_length(1), 1);
    });
}

// ============================================================================
// F10: PlanWriter set/clear level_based_levels 测试
// ============================================================================

#[test]
fn f10_plan_writer_set_level_based_levels() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 10, 2000, 50, 100, 0u8,
            )
        );
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 3, 8, 12));
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 3).unwrap();
        assert_eq!(o.upline_levels, 8);
        assert_eq!(o.downline_levels, 12);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsUpdated {
                entity_id: 1,
                level_id: 3,
            }
            .into(),
        );
    });
}

#[test]
fn f10_plan_writer_set_level_rejects_zero() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        assert!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 3, 0, 0).is_err());
    });
}

#[test]
fn f10_plan_writer_clear_level_overrides() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 10, 2000, 50, 100, 0u8,
            )
        );
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 3, 8, 12));
        assert_ok!(<SingleLine as SingleLinePlanWriter>::clear_level_overrides(
            1, 3
        ));
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 3).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsRemoved {
                entity_id: 1,
                level_id: 3,
            }
            .into(),
        );
    });
}

#[test]
fn f10_plan_writer_clear_nonexistent_is_noop() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        assert_ok!(<SingleLine as SingleLinePlanWriter>::clear_level_overrides(
            1, 99
        ));
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| matches!(e.event, RuntimeEvent::CommissionSingleLine(_)))
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

// ============================================================================
// F12: clear_config 级联清理 LevelOverrides 测试
// ============================================================================

#[test]
fn f12_clear_config_cascades_level_overrides() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            5,
            5
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            8,
            8
        ));

        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).is_none());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).is_none());
    });
}

#[test]
fn f12_force_clear_config_cascades_level_overrides() {
    new_test_ext().execute_with(|| {
        assert_ok!(SingleLine::force_set_single_line_config(
            RuntimeOrigin::root(),
            1,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        pallet::SingleLineCustomLevelOverrides::<Test>::insert(
            1,
            5,
            pallet::LevelBasedLevels {
                upline_levels: 3,
                downline_levels: 3,
            },
        );

        assert_ok!(SingleLine::force_clear_single_line_config(
            RuntimeOrigin::root(),
            1
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 5).is_none());
    });
}

#[test]
fn f12_plan_writer_clear_config_cascades() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 10, 2000, 50, 100, 0u8,
            )
        );
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 5, 5));

        assert_ok!(<SingleLine as SingleLinePlanWriter>::clear_config(1));
        assert!(pallet::SingleLineConfigs::<Test>::get(1).is_none());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).is_none());
    });
}

#[test]
fn f12_clear_config_does_not_affect_other_entity() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        set_entity_owner(2, OWNER);
        set_entity_admin(2, ADMIN, AdminPermission::COMMISSION_MANAGE);
        set_custom_level_count(2, 10);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            2,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            5,
            5
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            2,
            1,
            8,
            8
        ));

        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert!(pallet::SingleLineConfigs::<Test>::get(2).is_some());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(2, 1).is_some());
    });
}

// ============================================================================
// F5: 单链满后自动重试 + 手动加入 测试
// ============================================================================

#[test]
fn f5_auto_extend_join_on_full_segment() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        // 填满第一段（MaxSingleLineLength=200）
        for i in 0..200u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }

        // 用户 500 首单时段满 → 自动创建新段，加入成功
        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &500, 10000, 10000, modes, true, 1, 0,
        );
        assert_eq!(SingleLine::user_position(entity_id, &500), Some(200));
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::AddedToSingleLine {
                entity_id,
                account: 500,
                index: 200,
            }
            .into(),
        );
    });
}

#[test]
fn f5_already_joined_user_no_duplicate_attempt() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        // 用户 10 首单加入
        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 10000, 10000, modes, true, 1, 0,
        );
        assert_eq!(SingleLine::user_position(entity_id, &10), Some(0));

        // 第二笔订单 → 用户已在链中，不会重复添加
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 10000, 10000, modes, false, 2, 0,
        );
        // 链长度仍为 1（无重复）
        assert_eq!(SingleLine::single_line_length(entity_id), 1);
    });
}

// ============================================================================
// R4 审计回归测试
// ============================================================================

#[test]
fn m1_r4_clear_config_emits_all_level_overrides_cleared() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            5,
            5
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            8,
            8
        ));
        System::reset_events();

        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::AllLevelOverridesCleared { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn m1_r4_clear_config_no_overrides_no_event() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        System::reset_events();

        assert_ok!(SingleLine::clear_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        // AllLevelOverridesCleared should NOT be emitted when no overrides exist
        let override_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.event,
                    RuntimeEvent::CommissionSingleLine(
                        pallet::Event::AllLevelOverridesCleared { .. }
                    )
                )
            })
            .collect();
        assert_eq!(override_events.len(), 0);
    });
}

#[test]
fn m3_r4_entity_locked_rejects_pause() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        set_entity_locked(1);
        assert_noop!(
            SingleLine::pause_single_line(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

#[test]
fn m3_r4_entity_locked_rejects_resume() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::pause_single_line(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        set_entity_locked(1);
        assert_noop!(
            SingleLine::resume_single_line(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

#[test]
fn m3_r4_entity_locked_rejects_update_params() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        set_entity_locked(1);
        assert_noop!(
            SingleLine::update_single_line_params(
                RuntimeOrigin::signed(OWNER),
                1,
                Some(200),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

#[test]
fn m3_r4_entity_locked_rejects_set_level() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_locked(1);
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1, 5, 5),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

#[test]
fn m3_r4_entity_locked_rejects_remove_level() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        set_entity_locked(1);
        assert_noop!(
            SingleLine::remove_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

#[test]
fn l4_r4_unactivated_upline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // unactivate 40 (index=3)
        set_unactivated(entity_id, 40);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 上线: 40(unactivated→skip), 30, 20, 10 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 30);
        assert_eq!(outputs[1].beneficiary, 20);
        assert_eq!(outputs[2].beneficiary, 10);
    });
}

#[test]
fn l4_r4_unactivated_downline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // unactivate 30 (index=2)
        set_unactivated(entity_id, 30);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_downline(
            entity_id,
            &10,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 下线: 20, 30(unactivated→skip), 40, 50 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[1].beneficiary, 40);
        assert_eq!(outputs[2].beneficiary, 50);
    });
}

#[test]
fn m1r5_frozen_upline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // freeze 40 (index=3)
        set_member_frozen(entity_id, 40);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 上线: 40(frozen→skip), 30, 20, 10 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 30);
        assert_eq!(outputs[1].beneficiary, 20);
        assert_eq!(outputs[2].beneficiary, 10);
    });
}

#[test]
fn m1r5_frozen_downline_skipped() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // freeze 30 (index=2)
        set_member_frozen(entity_id, 30);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        SingleLine::process_downline(
            entity_id,
            &10,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 下线: 20, 30(frozen→skip), 40, 50 → 3 outputs
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[1].beneficiary, 40);
        assert_eq!(outputs[2].beneficiary, 50);
    });
}

#[test]
fn m1r5_frozen_via_plugin_integration() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        set_member_frozen(entity_id, 20);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let (outputs, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 1, 0,
        );

        // 上线: 20(frozen→skip), 10 → only 10
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 10);
    });
}

#[test]
fn m1r5_frozen_token_plugin_integration() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        set_member_frozen(entity_id, 20);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_DOWNLINE);
        let (outputs, _) = <SingleLine as TokenCommissionPlugin<u64, u128>>::calculate_token(
            entity_id,
            &10,
            100_000u128,
            100_000u128,
            modes,
            false,
            1,
            0,
        );

        // 下线: 20(frozen→skip), 30 → only 30
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 30);
    });
}

#[test]
fn l4_r4_process_upline_zero_rate_no_output() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            0,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 10000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &30, &config);
        SingleLine::process_upline(
            entity_id,
            &30,
            10000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert!(outputs.is_empty());
        assert_eq!(remaining, 10000);
    });
}

#[test]
fn l4_r4_cross_segment_upline_traversal() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        // base_upline_levels=5, max=150
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            5,
            5,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));

        // 填满第一段(200) + 第二段前3个
        for i in 0..203u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);

        // buyer=202 (index=202, segment 1), upline=201,200,199,198,197 (跨段边界)
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &202, &config);
        SingleLine::process_upline(
            entity_id,
            &202,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 5 层: 201(seg1), 200(seg1), 199(seg0), 198(seg0), 197(seg0) — 跨段
        assert_eq!(outputs.len(), 5);
        assert_eq!(outputs[0].beneficiary, 201); // level=1
        assert_eq!(outputs[1].beneficiary, 200); // level=2, seg boundary
        assert_eq!(outputs[2].beneficiary, 199); // level=3, seg 0
        assert_eq!(outputs[3].beneficiary, 198); // level=4
        assert_eq!(outputs[4].beneficiary, 197); // level=5
    });
}

#[test]
fn l4_r4_cross_segment_downline_traversal() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        // base_downline_levels=5, max=200
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            5,
            5,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));

        // 填满第一段(200) + 第二段前5个
        for i in 0..205u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);

        // buyer=198 (index=198, segment 0), downline=199,200,201,202,203 (跨段边界)
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &198, &config);
        SingleLine::process_downline(
            entity_id,
            &198,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 5 层: 199(seg0), 200(seg1), 201(seg1), 202(seg1), 203(seg1) — 跨段
        assert_eq!(outputs.len(), 5);
        assert_eq!(outputs[0].beneficiary, 199); // level=1, seg 0
        assert_eq!(outputs[1].beneficiary, 200); // level=2, seg boundary
        assert_eq!(outputs[2].beneficiary, 201); // level=3, seg 1
        assert_eq!(outputs[3].beneficiary, 202); // level=4
        assert_eq!(outputs[4].beneficiary, 203); // level=5
    });
}

#[test]
fn l4_r4_force_reset_multi_segment() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;

        // Fill 2.5 segments (500 accounts, 200 per segment → 3 segments: 200+200+100)
        for i in 0..500u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 3);
        assert_eq!(SingleLine::single_line_length(entity_id), 500);

        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            u32::MAX
        ));
        assert_eq!(SingleLine::single_line_length(entity_id), 0);
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 0);
        assert_eq!(SingleLine::user_position(entity_id, &0), None);
        assert_eq!(SingleLine::user_position(entity_id, &200), None);
        assert_eq!(SingleLine::user_position(entity_id, &499), None);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineReset {
                entity_id,
                removed_count: 500,
            }
            .into(),
        );
    });
}

// ============================================================================
// P0-1: WeightInfo integration
// ============================================================================

#[test]
fn weight_info_unit_impl_works() {
    use crate::weights::WeightInfo;
    let w = <() as WeightInfo>::set_single_line_config();
    assert!(w.ref_time() > 0);
    let w = <() as WeightInfo>::force_reset_single_line(10);
    assert!(w.ref_time() > 0);
}

// ============================================================================
// P0-2: Batched force_reset
// ============================================================================

#[test]
fn batched_reset_partial_then_complete() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        for i in 0..500u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 3);

        // Reset 1 segment at a time
        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            1
        ));
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);
        assert!(SingleLine::single_line_length(entity_id) > 0);

        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            1
        ));
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 1);

        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            1
        ));
        assert_eq!(SingleLine::single_line_length(entity_id), 0);

        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::SingleLineResetCompleted { entity_id }.into(),
        );
    });
}

// ============================================================================
// P0-3: force_remove / force_restore
// ============================================================================

#[test]
fn force_remove_from_single_line_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        assert_ok!(SingleLine::force_remove_from_single_line(
            RuntimeOrigin::root(),
            entity_id,
            30,
        ));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::MemberRemovedFromSingleLine {
                entity_id,
                account: 30,
            }
            .into(),
        );

        // Removed member is skipped in commission calculation
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        SingleLine::process_upline(
            entity_id,
            &50,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        // 30 is skipped → only 40, 20, 10 = 3 outputs
        assert_eq!(outputs.len(), 3);
        assert!(outputs.iter().all(|o| o.beneficiary != 30));
    });
}

#[test]
fn force_remove_rejects_non_root() {
    new_test_ext().execute_with(|| {
        setup_single_line(1, &[10, 20]);
        assert_noop!(
            SingleLine::force_remove_from_single_line(RuntimeOrigin::signed(OWNER), 1, 10),
            frame_support::error::BadOrigin,
        );
    });
}

#[test]
fn force_remove_rejects_non_member() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            SingleLine::force_remove_from_single_line(RuntimeOrigin::root(), 1, 999),
            pallet::Error::<Test>::MemberNotInSingleLine,
        );
    });
}

#[test]
fn force_restore_to_single_line_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        assert_ok!(SingleLine::force_remove_from_single_line(
            RuntimeOrigin::root(),
            entity_id,
            20
        ));
        assert_ok!(SingleLine::force_restore_to_single_line(
            RuntimeOrigin::root(),
            entity_id,
            20
        ));
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::MemberRestoredToSingleLine {
                entity_id,
                account: 20,
            }
            .into(),
        );

        // Restored member receives commissions again
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = alloc::vec::Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &30, &config);
        SingleLine::process_upline(
            entity_id,
            &30,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 20);
        assert_eq!(outputs[1].beneficiary, 10);
    });
}

// ============================================================================
// P0-4: Config change delay
// ============================================================================

#[test]
fn schedule_config_change_works() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        let pending = pallet::PendingConfigChanges::<Test>::get(1).unwrap();
        assert_eq!(pending.upline_rate, 50);
        assert_eq!(pending.apply_after, 11); // block 1 + delay 10
        assert_eq!(pending.reach_mode, ReachMode::Bidirectional);
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::ConfigChangeScheduled {
                entity_id: 1,
                apply_after: 11,
            }
            .into(),
        );
    });
}

#[test]
fn reach_mode_default_is_beneficiary_only() {
    assert_eq!(ReachMode::default(), ReachMode::BeneficiaryOnly);
}

#[test]
fn schedule_rejects_duplicate() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        assert_noop!(
            SingleLine::schedule_config_change(
                RuntimeOrigin::signed(OWNER),
                1,
                60,
                60,
                5,
                10,
                1000,
                20,
                30,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::PendingConfigAlreadyExists,
        );
    });
}

#[test]
fn apply_pending_config_too_early() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        // block 1, apply_after = 11
        assert_noop!(
            SingleLine::apply_pending_config(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::PendingConfigNotReady,
        );
    });
}

#[test]
fn apply_pending_config_after_delay() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        // Advance to block 11
        System::set_block_number(11);
        assert_ok!(SingleLine::apply_pending_config(
            RuntimeOrigin::signed(NOBODY),
            1
        ));
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.upline_rate, 50);
        assert_eq!(config.downline_rate, 50);
        assert!(pallet::PendingConfigChanges::<Test>::get(1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::PendingConfigApplied { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn cancel_pending_config_works() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        assert_ok!(SingleLine::cancel_pending_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));
        assert!(pallet::PendingConfigChanges::<Test>::get(1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::PendingConfigCancelled { entity_id: 1 }.into(),
        );
    });
}

#[test]
fn cancel_pending_rejects_non_owner() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        assert_noop!(
            SingleLine::cancel_pending_config(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::NotEntityOwnerOrAdmin,
        );
    });
}

// ============================================================================
// P1-1: preview_single_line_commission
// ============================================================================

#[test]
fn preview_commission_works() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let outputs = SingleLine::preview_single_line_commission(entity_id, &30, 100_000);
        let up_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .count();
        let dn_count = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineDownline)
            .count();
        assert_eq!(up_count, 2); // 20, 10
        assert_eq!(dn_count, 2); // 40, 50
    });
}

#[test]
fn preview_no_config_returns_empty() {
    new_test_ext().execute_with(|| {
        let outputs = SingleLine::preview_single_line_commission(1, &10, 100_000);
        assert!(outputs.is_empty());
    });
}

// ============================================================================
// P1-2: Config change audit log
// ============================================================================

#[test]
fn config_change_log_recorded_on_set() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_eq!(pallet::ConfigChangeLogCount::<Test>::get(1), 1);
        let log = pallet::ConfigChangeLogs::<Test>::get(1, 0).unwrap();
        assert_eq!(log.upline_rate, 100);
        assert_eq!(log.block_number, 1);
    });
}

#[test]
fn config_change_log_increments_on_update() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            Some(200),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ));
        assert_eq!(pallet::ConfigChangeLogCount::<Test>::get(1), 2);
        let log = pallet::ConfigChangeLogs::<Test>::get(1, 1).unwrap();
        assert_eq!(log.upline_rate, 200);
    });
}

// ============================================================================
// P1-3: Entity stats
// ============================================================================

#[test]
fn entity_stats_updated_on_commission() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &20, 100_000, 100_000, modes, false, 1, 0,
        );

        let stats = pallet::EntitySingleLineStats::<Test>::get(entity_id);
        assert_eq!(stats.total_orders, 1);
        assert_eq!(stats.total_upline_payouts, 1); // 10
        assert_eq!(stats.total_downline_payouts, 1); // 30
    });
}

// ============================================================================
// P1-4: LevelOverrideExceedsMax
// ============================================================================

#[test]
fn set_level_rejects_exceeds_max() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            5,
            5,
            1000,
            10,
            15,
            ReachMode::Bidirectional,
        ));
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1, 11, 5),
            pallet::Error::<Test>::LevelOverrideExceedsMax,
        );
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 1, 5, 16),
            pallet::Error::<Test>::LevelOverrideExceedsMax,
        );
        // exact max is allowed
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            10,
            15
        ));
    });
}

// ============================================================================
// 4.1: RatesTooHigh validation
// ============================================================================

#[test]
fn validate_config_rejects_rates_too_high() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        // MaxTotalRateBps = 100_000 in mock
        // 1000 * 200 + 1000 * 200 = 400_000 > 100_000
        assert_noop!(
            SingleLine::set_single_line_config(
                RuntimeOrigin::signed(OWNER),
                1,
                1000,
                1000,
                100,
                100,
                0,
                200,
                200,
                ReachMode::Bidirectional,
            ),
            pallet::Error::<Test>::RatesTooHigh,
        );
        // 100 * 150 + 100 * 200 = 35_000 <= 100_000 → ok
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
    });
}

// ============================================================================
// 4.2: MaxSegmentCount
// ============================================================================

#[test]
fn max_segment_count_enforced() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // MaxSingleLineLength=200, MaxSegmentCount=1000
        // Directly set segment count to 1000 (segments 0-999, all full)
        pallet::SingleLineSegmentCount::<Test>::insert(entity_id, 1000);
        let mut seg = frame_support::BoundedVec::<u64, MaxSingleLineLength>::default();
        for i in 0..200u64 {
            seg.try_push(i + 199800).unwrap();
        }
        pallet::SingleLineSegments::<Test>::insert(entity_id, 999u32, seg);

        // new_seg_id = 1000 which is NOT < MaxSegmentCount(1000) → error
        assert_noop!(
            SingleLine::add_to_single_line(entity_id, &888888),
            pallet::Error::<Test>::MaxSegmentCountReached,
        );
    });
}

// ============================================================================
// 4.3: add_to_single_line is pub(crate) — compile-time guarantee, no runtime test needed
// ============================================================================

// ============================================================================
// R4: buyer_in_chain pre-read optimization — correctness preserved
// ============================================================================

#[test]
fn r4_buyer_not_in_chain_auto_added() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, false, 1, 0,
        );
        assert_eq!(SingleLine::user_position(entity_id, &10), Some(0));
    });
}

// ============================================================================
// R5: PlanWriter reuses validate_config
// ============================================================================

#[test]
fn r5_plan_writer_rejects_rates_too_high() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        assert!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 1000, 1000, 100, 100, 0, 200, 200, 0u8,
            )
            .is_err()
        );
    });
}

// ============================================================================
// Batched reset: partial progress preserved
// ============================================================================

#[test]
fn batched_reset_preserves_earlier_segments() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        // 350 members → 2 segments (200 + 150)
        for i in 0..350u64 {
            assert_ok!(SingleLine::add_to_single_line(entity_id, &i));
        }
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 2);

        // Reset only last segment (150 members)
        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            1
        ));
        assert_eq!(pallet::SingleLineSegmentCount::<Test>::get(entity_id), 1);
        // First segment members still exist
        assert_eq!(SingleLine::user_position(entity_id, &0), Some(0));
        assert_eq!(SingleLine::user_position(entity_id, &199), Some(199));
        // Last segment members removed
        assert_eq!(SingleLine::user_position(entity_id, &200), None);
        assert_eq!(SingleLine::user_position(entity_id, &349), None);
    });
}

// ============================================================================
// level_id 存在性校验测试
// ============================================================================

#[test]
fn set_level_based_levels_rejects_nonexistent_level_id() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // 实体只有 3 个等级 (level_id 1..=3)
        set_custom_level_count(1, 3);
        // level_id=4 不存在，应被拒绝
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 4, 5, 5),
            pallet::Error::<Test>::LevelIdNotFound,
        );
        // level_id=3 存在，应通过
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            3,
            5,
            5,
        ));
    });
}

#[test]
fn set_level_based_levels_allows_level_zero() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // 即使没有自定义等级，level_id=0（默认等级）也始终有效
        set_custom_level_count(1, 0);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            0,
            5,
            5,
        ));
    });
}

#[test]
fn plan_writer_set_level_rejects_nonexistent_level_id() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 2);
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 5, 0, 50, 50, 0u8,
            )
        );
        // level_id=3 不存在
        assert!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 3, 5, 5).is_err());
        // level_id=2 存在
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 2, 5, 5));
    });
}

// ============================================================================
// OnLevelRemoved 自动清理测试
// ============================================================================

#[test]
fn on_level_removed_cleans_up_override() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            8,
            5,
        ));
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).is_some());

        // 模拟等级删除回调
        <SingleLine as pallet_entity_common::OnLevelRemoved>::on_level_removed(1, 2);

        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsRemoved {
                entity_id: 1,
                level_id: 2,
            }
            .into(),
        );
    });
}

#[test]
fn on_level_removed_noop_when_no_override() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // level_id=5 没有 override，调用不应 panic
        <SingleLine as pallet_entity_common::OnLevelRemoved>::on_level_removed(1, 5);
        // 无 LevelBasedLevelsRemoved 事件
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.event,
                    RuntimeEvent::CommissionSingleLine(
                        pallet::Event::LevelBasedLevelsRemoved { .. }
                    )
                )
            })
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

#[test]
fn on_level_removed_only_affects_target_level() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            5,
            5,
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            8,
            8,
        ));

        // 删除 level_id=2
        <SingleLine as pallet_entity_common::OnLevelRemoved>::on_level_removed(1, 2);

        // level_id=1 的 override 不受影响
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).is_some());
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).is_none());
    });
}

// ============================================================================
// BUG-1: extrinsic set_level_based_levels 无 config 时拒绝
// ============================================================================

#[test]
fn set_level_based_levels_rejects_no_config() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_noop!(
            SingleLine::set_level_based_levels(RuntimeOrigin::signed(OWNER), 1, 0, 5, 5),
            pallet::Error::<Test>::ConfigNotFound,
        );
    });
}

// ============================================================================
// BUG-2: PlanWriter set_level_based_levels 无 config / 超 max 拒绝
// ============================================================================

#[test]
fn plan_writer_set_level_rejects_no_config() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        // 无 config → ConfigNotFound
        assert!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 5, 5).is_err());
    });
}

#[test]
fn plan_writer_set_level_rejects_exceeds_max() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        // max_upline=10, max_downline=15
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 5, 0, 10, 15, 0u8,
            )
        );
        // upline=11 超过 max_upline=10
        assert!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 11, 5).is_err());
        // downline=16 超过 max_downline=15
        assert!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 5, 16).is_err());
        // 等于 max → 允许
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 10, 15));
    });
}

// ============================================================================
// BUG-3: config 降低 max_levels 后 clamp 已有覆盖
// ============================================================================

#[test]
fn clamp_overrides_on_set_config_lower_max() {
    new_test_ext().execute_with(|| {
        setup_config(1); // max_upline=150, max_downline=200
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            100,
            150
        ));
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            2,
            5,
            5
        ));

        // 用新 config 降低 max → clamp
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            3,
            3,
            1000,
            10,
            10,
            ReachMode::Bidirectional,
        ));
        // level 1: 100→10, 150→10
        let o1 = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o1.upline_levels, 10);
        assert_eq!(o1.downline_levels, 10);
        // level 2: 5/5 不变（都 <= 10）
        let o2 = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 2).unwrap();
        assert_eq!(o2.upline_levels, 5);
        assert_eq!(o2.downline_levels, 5);
    });
}

#[test]
fn clamp_overrides_on_update_params_lower_max() {
    new_test_ext().execute_with(|| {
        setup_config(1); // max=150/200, base=10/15
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            80,
            80
        ));

        // 降低 max_upline_levels=5（同时降低 base 以满足 base<=max）
        assert_ok!(SingleLine::update_single_line_params(
            RuntimeOrigin::signed(OWNER),
            1,
            None,
            None,
            None,
            Some(3),
            None,
            Some(5),
            None,
            None,
        ));
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 5); // clamped
        assert_eq!(o.downline_levels, 80); // 不变
    });
}

#[test]
fn clamp_overrides_on_force_set_config() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            50,
            50
        ));

        assert_ok!(SingleLine::force_set_single_line_config(
            RuntimeOrigin::root(),
            1,
            100,
            100,
            3,
            3,
            0,
            8,
            8,
            ReachMode::Bidirectional,
        ));
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 8);
        assert_eq!(o.downline_levels, 8);
    });
}

#[test]
fn clamp_overrides_on_apply_pending_config() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            50,
            50
        ));

        // schedule 降低 max 到 6/6
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            3,
            3,
            0,
            6,
            6,
            ReachMode::Bidirectional,
        ));
        System::set_block_number(100);
        assert_ok!(SingleLine::apply_pending_config(
            RuntimeOrigin::signed(OWNER),
            1
        ));

        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 6);
        assert_eq!(o.downline_levels, 6);
    });
}

#[test]
fn clamp_overrides_on_governance_set_config() {
    new_test_ext().execute_with(|| {
        use pallet_entity_common::SingleLineGovernancePort;
        setup_config(1);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            50,
            50
        ));

        assert_ok!(
            <SingleLine as SingleLineGovernancePort>::governance_set_single_line_config(
                1, 100, 100, 3, 3, 7, 7, 0u8,
            )
        );
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 7);
        assert_eq!(o.downline_levels, 7);
    });
}

#[test]
fn clamp_overrides_on_plan_writer_set_config() {
    new_test_ext().execute_with(|| {
        use pallet_commission_common::SingleLinePlanWriter;
        set_custom_level_count(1, 10);
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 5, 0, 50, 50, 0u8,
            )
        );
        assert_ok!(<SingleLine as SingleLinePlanWriter>::set_level_based_levels(1, 1, 30, 30));

        // 降低 max 到 9/9
        assert_ok!(
            <SingleLine as SingleLinePlanWriter>::set_single_line_config(
                1, 100, 100, 5, 5, 0, 9, 9, 0u8,
            )
        );
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 9);
        assert_eq!(o.downline_levels, 9);
    });
}

#[test]
fn clamp_removes_override_when_both_become_zero() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // upline_only override: upline=50, downline=0 不行（both_zero 校验）
        // 直接写入存储模拟
        pallet::SingleLineCustomLevelOverrides::<Test>::insert(
            1,
            1,
            pallet::LevelBasedLevels {
                upline_levels: 3,
                downline_levels: 0,
            },
        );

        // 降低 max_upline=0 → clamp(3,0) → (0,0) → 删除
        assert_ok!(SingleLine::force_set_single_line_config(
            RuntimeOrigin::root(),
            1,
            100,
            100,
            0,
            0,
            0,
            0,
            200,
            ReachMode::Bidirectional,
        ));
        assert!(pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).is_none());
        frame_system::Pallet::<Test>::assert_has_event(
            pallet::Event::<Test>::LevelBasedLevelsRemoved {
                entity_id: 1,
                level_id: 1,
            }
            .into(),
        );
    });
}

#[test]
fn clamp_noop_when_no_overrides_exist() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        // 无覆盖，降低 max 不应 panic 或 emit 额外事件
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            3,
            3,
            1000,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        let pallet_events: alloc::vec::Vec<_> = System::events()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.event,
                    RuntimeEvent::CommissionSingleLine(
                        pallet::Event::LevelBasedLevelsUpdated { .. }
                            | pallet::Event::LevelBasedLevelsRemoved { .. }
                    )
                )
            })
            .collect();
        assert_eq!(pallet_events.len(), 0);
    });
}

#[test]
fn clamp_no_change_when_max_increases() {
    new_test_ext().execute_with(|| {
        setup_config(1); // max=150/200
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            1,
            1,
            10,
            10
        ));

        // 提高 max → 覆盖值不变
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            1,
            100,
            100,
            3,
            3,
            1000,
            200,
            200,
            ReachMode::Bidirectional,
        ));
        let o = pallet::SingleLineCustomLevelOverrides::<Test>::get(1, 1).unwrap();
        assert_eq!(o.upline_levels, 10);
        assert_eq!(o.downline_levels, 10);
    });
}

// ============================================================================
// BUG-4: governance_set_single_line_config 保留已有 threshold
// ============================================================================

#[test]
fn governance_set_config_preserves_existing_threshold() {
    new_test_ext().execute_with(|| {
        use pallet_entity_common::SingleLineGovernancePort;
        setup_config(1); // threshold=1000
        let before = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(before.level_increment_threshold, 1000);

        // governance 修改费率和层级，不应丢失 threshold
        assert_ok!(
            <SingleLine as SingleLineGovernancePort>::governance_set_single_line_config(
                1, 200, 200, 5, 5, 50, 50, 0u8,
            )
        );
        let after = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(after.upline_rate, 200);
        assert_eq!(after.level_increment_threshold, 1000); // 保留
    });
}

#[test]
fn governance_set_config_uses_zero_when_no_existing_config() {
    new_test_ext().execute_with(|| {
        use pallet_entity_common::SingleLineGovernancePort;
        // 无已有 config → threshold 默认 zero
        assert_ok!(
            <SingleLine as SingleLineGovernancePort>::governance_set_single_line_config(
                1, 100, 100, 3, 3, 10, 10, 0u8,
            )
        );
        let config = pallet::SingleLineConfigs::<Test>::get(1).unwrap();
        assert_eq!(config.level_increment_threshold, 0);
    });
}

// ============================================================================
// P0: apply_pending_config rejects inactive/locked entity
// ============================================================================

#[test]
fn apply_pending_config_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        System::set_block_number(11);
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::apply_pending_config(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn apply_pending_config_rejects_locked_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        System::set_block_number(11);
        set_entity_locked(1);
        assert_noop!(
            SingleLine::apply_pending_config(RuntimeOrigin::signed(NOBODY), 1),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

// ============================================================================
// P1: cancel_pending_config rejects inactive/locked entity
// ============================================================================

#[test]
fn cancel_pending_config_rejects_inactive_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        set_entity_inactive(1);
        assert_noop!(
            SingleLine::cancel_pending_config(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityNotActive,
        );
    });
}

#[test]
fn cancel_pending_config_rejects_locked_entity() {
    new_test_ext().execute_with(|| {
        setup_entity(1);
        assert_ok!(SingleLine::schedule_config_change(
            RuntimeOrigin::signed(OWNER),
            1,
            50,
            50,
            5,
            10,
            1000,
            20,
            30,
            ReachMode::Bidirectional,
        ));
        set_entity_locked(1);
        assert_noop!(
            SingleLine::cancel_pending_config(RuntimeOrigin::signed(OWNER), 1),
            pallet::Error::<Test>::EntityLocked,
        );
    });
}

// ============================================================================
// 分佣历史记录 (Payout History) 测试
// ============================================================================

#[test]
fn payout_history_basic_upline_records() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 单链: [10, 20, 30, 40, 50]
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // buyer=50 下单，上线和下线各分佣
        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (outputs, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &50, 100_000, 100_000, modes, false, 0, 0,
        );
        // 50 在 index=4, upline_rate=100 (1%), base_upline=10 → 上线: 40,30,20,10 各 1000
        assert!(outputs.len() >= 4);

        // 检查 account=40 的 payout history
        let payouts_40 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 40u64);
        assert_eq!(payouts_40.len(), 1);
        assert_eq!(payouts_40[0].buyer, 50);
        assert_eq!(payouts_40[0].order_id, 0);
        assert_eq!(payouts_40[0].amount, 1000);
        assert_eq!(payouts_40[0].direction, pallet::PayoutDirection::Upline);
        assert_eq!(payouts_40[0].level_distance, 1);
        assert_eq!(payouts_40[0].block_number, 1);

        // 检查 account=10 的 payout history (level_distance = 4)
        let payouts_10 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 10u64);
        assert_eq!(payouts_10.len(), 1);
        assert_eq!(payouts_10[0].buyer, 50);
        assert_eq!(payouts_10[0].level_distance, 4);
        assert_eq!(payouts_10[0].direction, pallet::PayoutDirection::Upline);
    });
}

#[test]
fn payout_history_basic_downline_records() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // buyer=10 下单，无上线(index=0)，有下线 20,30,40,50
        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (_outputs, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, false, 0, 0,
        );

        // 检查 account=20 的 downline payout
        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert_eq!(payouts_20.len(), 1);
        assert_eq!(payouts_20[0].buyer, 10);
        assert_eq!(payouts_20[0].amount, 1000);
        assert_eq!(payouts_20[0].direction, pallet::PayoutDirection::Downline);
        assert_eq!(payouts_20[0].level_distance, 1);

        // account=50 的 downline payout (level_distance = 4)
        let payouts_50 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 50u64);
        assert_eq!(payouts_50.len(), 1);
        assert_eq!(payouts_50[0].level_distance, 4);
        assert_eq!(payouts_50[0].direction, pallet::PayoutDirection::Downline);
    });
}

#[test]
fn payout_history_summary_accumulates() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );

        // 第一笔: buyer=30 下单
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        // 第二笔: buyer=30 再次下单
        System::set_block_number(5);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 200_000, 200_000, modes, false, 1, 0,
        );

        // account=20 作为上线收到 30 的两笔分佣
        let stats_20 = pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64);
        assert_eq!(stats_20.total_earned_as_upline, 1000 + 2000); // 100000*1% + 200000*1%
        assert_eq!(stats_20.total_earned_as_downline, 0);
        assert_eq!(stats_20.total_payout_count, 2);
        assert_eq!(stats_20.last_payout_block, 5);

        // account=20 应有 2 条 payout 记录
        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert_eq!(payouts_20.len(), 2);
        assert_eq!(payouts_20[0].amount, 1000);
        assert_eq!(payouts_20[1].amount, 2000);
    });
}

#[test]
fn payout_history_both_directions_accumulate() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );

        // buyer=10 下单 → account=20 作为 downline 收佣
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &10, 100_000, 100_000, modes, false, 0, 0,
        );

        // buyer=30 下单 → account=20 作为 upline 收佣
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        let stats_20 = pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64);
        assert_eq!(stats_20.total_earned_as_upline, 1000);
        assert_eq!(stats_20.total_earned_as_downline, 1000);
        assert_eq!(stats_20.total_payout_count, 2);

        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert_eq!(payouts_20.len(), 2);
        assert_eq!(payouts_20[0].direction, pallet::PayoutDirection::Downline);
        assert_eq!(payouts_20[1].direction, pallet::PayoutDirection::Upline);
    });
}

#[test]
fn payout_history_fifo_eviction() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 只需两个成员：buyer + recipient
        setup_single_line(entity_id, &[10, 20]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);

        // 发 55 笔订单，buyer=20 → account=10 作为 upline 收佣
        // MaxPayoutRecords = 50，所以前 5 笔会被淘汰
        for i in 0..55u64 {
            System::set_block_number(i + 1);
            let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
                entity_id,
                &20,
                10_000 * (i + 1) as u128,
                10_000_000,
                modes,
                false,
                i as u32,
                0,
            );
        }

        let payouts_10 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 10u64);
        assert_eq!(payouts_10.len(), 50); // 被限制到 MaxPayoutRecords

        // 最旧的应该是第 6 笔 (i=5, block=6, amount = 10000*6*1% = 600)
        assert_eq!(payouts_10[0].block_number, 6);
        assert_eq!(payouts_10[0].amount, 600);

        // 最新的应该是第 55 笔 (i=54, block=55, amount = 10000*55*1% = 5500)
        assert_eq!(payouts_10[49].block_number, 55);
        assert_eq!(payouts_10[49].amount, 5500);

        // 汇总不受淘汰影响，应包含所有 55 笔
        let stats_10 = pallet::MemberSingleLineStats::<Test>::get(entity_id, 10u64);
        assert_eq!(stats_10.total_payout_count, 55);
        // 总佣金 = sum(10000*(i+1)*1/100 for i in 0..55) = 100 * sum(1..=55) = 100 * 1540 = 154000
        assert_eq!(stats_10.total_earned_as_upline, 154_000);
        assert_eq!(stats_10.last_payout_block, 55);
    });
}

#[test]
fn single_line_member_position_info_returns_neighbors_and_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40]);

        let info = SingleLine::single_line_member_position_info(entity_id, &30).unwrap();
        assert_eq!(info.position, 2);
        assert_eq!(info.queue_length, 4);
        assert_eq!(info.upline_levels, 10);
        assert_eq!(info.downline_levels, 15);
        assert_eq!(info.previous_account, Some(20));
        assert_eq!(info.next_account, Some(40));
    });
}

#[test]
fn single_line_member_view_returns_summary_and_payouts() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        let view = SingleLine::single_line_member_view(entity_id, &20).unwrap();
        assert!(view.is_enabled);
        assert_eq!(view.position_info.as_ref().unwrap().position, 1);
        assert_eq!(view.summary.total_earned_as_upline, 1000);
        assert_eq!(view.summary.total_earned_as_downline, 0);
        assert_eq!(view.summary.total_payout_count, 1);
        assert_eq!(view.recent_payouts.len(), 1);
        assert_eq!(view.recent_payouts[0].order_id, 0);
        assert_eq!(view.recent_payouts[0].buyer, 30);
        assert_eq!(view.recent_payouts[0].amount, 1000);
        assert_eq!(view.recent_payouts[0].direction, 0);
    });
}

#[test]
fn single_line_overview_returns_queue_and_stats() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        let overview = SingleLine::single_line_overview(entity_id);
        assert!(overview.is_enabled);
        assert_eq!(overview.queue_length, 3);
        assert_eq!(overview.segment_count, 1);
        assert_eq!(overview.stats.total_orders, 1);
        assert_eq!(overview.stats.total_upline_payouts, 2);
        assert_eq!(overview.stats.total_downline_payouts, 0);
    });
}

#[test]
fn single_line_member_queries_return_none_for_non_member_without_history() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        assert!(SingleLine::single_line_member_position_info(entity_id, &99).is_none());
        assert!(SingleLine::single_line_member_view(entity_id, &99).is_none());
        assert!(SingleLine::single_line_member_payouts(entity_id, &99).is_empty());
    });
}

#[test]
fn payout_history_cross_entity_isolation() {
    new_test_ext().execute_with(|| {
        setup_config(1);
        setup_config(2);
        setup_single_line(1, &[10, 20]);
        setup_single_line(2, &[10, 20]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);

        // entity 1: buyer=20 → account=10 gets 1000
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            1, &20, 100_000, 100_000, modes, false, 0, 0,
        );

        // entity 2: buyer=20 → account=10 gets 2000
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            2, &20, 200_000, 200_000, modes, false, 0, 0,
        );

        let payouts_e1 = pallet::MemberSingleLinePayouts::<Test>::get(1, 10u64);
        let payouts_e2 = pallet::MemberSingleLinePayouts::<Test>::get(2, 10u64);
        assert_eq!(payouts_e1.len(), 1);
        assert_eq!(payouts_e2.len(), 1);
        assert_eq!(payouts_e1[0].amount, 1000);
        assert_eq!(payouts_e2[0].amount, 2000);

        let stats_e1 = pallet::MemberSingleLineStats::<Test>::get(1, 10u64);
        let stats_e2 = pallet::MemberSingleLineStats::<Test>::get(2, 10u64);
        assert_eq!(stats_e1.total_earned_as_upline, 1000);
        assert_eq!(stats_e2.total_earned_as_upline, 2000);
    });
}

#[test]
fn payout_history_token_mode_does_not_record() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );

        // Token mode 分佣不应写历史
        let (outputs, _) = <SingleLine as TokenCommissionPlugin<u64, u128>>::calculate_token(
            entity_id,
            &30,
            100_000u128,
            100_000u128,
            modes,
            false,
            0,
            0,
        );
        assert!(!outputs.is_empty()); // Token outputs 确实产出

        // 但是 payout history 应该为空
        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert!(payouts_20.is_empty());

        let stats_20 = pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64);
        assert_eq!(stats_20.total_payout_count, 0);
    });
}

#[test]
fn payout_history_reset_clears_records() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        // 确认记录存在
        assert!(!pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64).is_empty());
        assert_ne!(
            pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64).total_payout_count,
            0
        );

        // 重置
        assert_ok!(SingleLine::force_reset_single_line(
            RuntimeOrigin::root(),
            entity_id,
            1000
        ));

        // 记录应被清除
        assert!(pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64).is_empty());
        assert_eq!(
            pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64).total_payout_count,
            0
        );
    });
}

#[test]
fn payout_history_no_records_when_zero_commission() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        // upline_rate = 0, downline_rate = 0
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            0,
            0,
            10,
            15,
            1000,
            150,
            200,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );
        let (outputs, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );
        assert!(outputs.is_empty());

        // 不应有任何记录
        assert!(pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64).is_empty());
        assert_eq!(
            pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64).total_payout_count,
            0
        );
    });
}

#[test]
fn payout_history_skipped_members_no_records() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30]);

        // ban account=20
        set_banned(entity_id, 20);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &30, 100_000, 100_000, modes, false, 0, 0,
        );

        // account=20 被跳过，不应有记录
        assert!(pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64).is_empty());
        assert_eq!(
            pallet::MemberSingleLineStats::<Test>::get(entity_id, 20u64).total_payout_count,
            0
        );

        // account=10 仍应收到记录 (level_distance=2, 因为 20 被跳过但遍历仍继续)
        let payouts_10 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 10u64);
        assert_eq!(payouts_10.len(), 1);
        assert_eq!(payouts_10[0].level_distance, 2);
    });
}

#[test]
fn payout_history_summary_default_when_no_records() {
    new_test_ext().execute_with(|| {
        let summary = pallet::MemberSingleLineStats::<Test>::get(1, 999u64);
        assert_eq!(summary.total_earned_as_upline, 0);
        assert_eq!(summary.total_earned_as_downline, 0);
        assert_eq!(summary.total_payout_count, 0);
        assert_eq!(summary.last_payout_block, 0);

        let payouts = pallet::MemberSingleLinePayouts::<Test>::get(1, 999u64);
        assert!(payouts.is_empty());
    });
}

#[test]
fn payout_history_block_number_tracks_correctly() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);

        System::set_block_number(100);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &20, 100_000, 100_000, modes, false, 0, 0,
        );

        System::set_block_number(200);
        let _ = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &20, 100_000, 100_000, modes, false, 1, 0,
        );

        let payouts_10 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 10u64);
        assert_eq!(payouts_10[0].block_number, 100);
        assert_eq!(payouts_10[1].block_number, 200);

        let stats_10 = pallet::MemberSingleLineStats::<Test>::get(entity_id, 10u64);
        assert_eq!(stats_10.last_payout_block, 200);
    });
}

#[test]
fn payout_history_new_buyer_auto_joins_and_records() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        setup_single_line(entity_id, &[10, 20]);

        let modes = CommissionModes(
            CommissionModes::SINGLE_LINE_UPLINE | CommissionModes::SINGLE_LINE_DOWNLINE,
        );

        // buyer=99 不在链中，首单时先加入链(idx=2)再计算
        // upline: 20(level=1), 10(level=2) 收到分佣; downline: 99在链尾无下线
        let (outputs, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &99, 100_000, 100_000, modes, false, 0, 0,
        );
        // 首单即分佣：上线 20, 10 各收到 1000
        assert_eq!(outputs.len(), 2);

        // 99 现在已加入链中
        assert!(pallet::SingleLineIndex::<Test>::contains_key(entity_id, 99));

        // account=20 作为 upline (level=1) 在首单就有 payout 记录
        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert_eq!(payouts_20.len(), 1);
        assert_eq!(payouts_20[0].buyer, 99);
        assert_eq!(payouts_20[0].level_distance, 1);

        // 第二笔: buyer=99 再次下单，此时 index=2，upline 有 10,20
        let (outputs2, _) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &99, 100_000, 100_000, modes, false, 1, 0,
        );
        assert!(!outputs2.is_empty());

        // account=20 现在有 2 次 payout 记录
        let payouts_20 = pallet::MemberSingleLinePayouts::<Test>::get(entity_id, 20u64);
        assert_eq!(payouts_20.len(), 2);
    });
}

// ============================================================================
// Budget Cap Validation Tests
// ============================================================================

#[test]
fn set_single_line_config_rejects_max_payout_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        set_entity_owner(1, 100);
        set_budget_cap(1, 3000); // cap = 30%
                                 // upline_rate=500 * max_upline_levels=4 + downline_rate=500 * max_downline_levels=3 = 2000+1500 = 3500 > 3000
        assert_noop!(
            CommissionSingleLine::set_single_line_config(
                RuntimeOrigin::signed(100),
                1,
                500,
                500,
                2,
                2,
                0u128,
                4,
                3,
                ReachMode::Bidirectional,
            ),
            crate::pallet::Error::<Test>::MaxPayoutExceedsBudgetCap
        );
        // 500*3 + 500*3 = 3000 == cap should pass
        assert_ok!(CommissionSingleLine::set_single_line_config(
            RuntimeOrigin::signed(100),
            1,
            500,
            500,
            2,
            2,
            0u128,
            3,
            3,
            ReachMode::Bidirectional,
        ));
    });
}

#[test]
fn update_single_line_params_rejects_max_payout_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        set_entity_owner(1, 100);
        // set initial config without cap
        assert_ok!(CommissionSingleLine::set_single_line_config(
            RuntimeOrigin::signed(100),
            1,
            500,
            500,
            2,
            2,
            0u128,
            3,
            3,
            ReachMode::Bidirectional,
        ));
        // now set cap
        set_budget_cap(1, 2000);
        // try to increase max_upline_levels to 3 → 500*3 + 500*3 = 3000 > 2000
        assert_noop!(
            CommissionSingleLine::update_single_line_params(
                RuntimeOrigin::signed(100),
                1,
                None,
                None,
                None,
                None,
                None,
                Some(3),
                None,
                None,
            ),
            crate::pallet::Error::<Test>::MaxPayoutExceedsBudgetCap
        );
    });
}

#[test]
fn schedule_config_change_rejects_max_payout_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        set_entity_owner(1, 100);
        set_budget_cap(1, 2000);
        // 500*3 + 500*3 = 3000 > 2000
        assert_noop!(
            CommissionSingleLine::schedule_config_change(
                RuntimeOrigin::signed(100),
                1,
                500,
                500,
                2,
                2,
                0u128,
                3,
                3,
                ReachMode::Bidirectional,
            ),
            crate::pallet::Error::<Test>::MaxPayoutExceedsBudgetCap
        );
        // 500*2 + 500*2 = 2000 == cap should pass
        assert_ok!(CommissionSingleLine::schedule_config_change(
            RuntimeOrigin::signed(100),
            1,
            500,
            500,
            2,
            2,
            0u128,
            2,
            2,
            ReachMode::Bidirectional,
        ));
    });
}

#[test]
fn force_set_single_line_config_rejects_max_payout_exceeding_budget_cap() {
    new_test_ext().execute_with(|| {
        set_entity_owner(1, 100);
        set_budget_cap(1, 2000);
        assert_noop!(
            CommissionSingleLine::force_set_single_line_config(
                RuntimeOrigin::root(),
                1,
                500,
                500,
                2,
                2,
                0u128,
                3,
                3,
                ReachMode::Bidirectional,
            ),
            crate::pallet::Error::<Test>::MaxPayoutExceedsBudgetCap
        );
    });
}

#[test]
fn budget_cap_zero_means_no_limit_single_line() {
    new_test_ext().execute_with(|| {
        set_entity_owner(1, 100);
        // cap=0 default, high rates should pass
        assert_ok!(CommissionSingleLine::set_single_line_config(
            RuntimeOrigin::signed(100),
            1,
            1000,
            1000,
            5,
            5,
            0u128,
            10,
            10,
            ReachMode::Bidirectional,
        ));
    });
}

// ============================================================================
// 首单即分佣测试
// ============================================================================

#[test]
fn first_order_buyer_gets_upline_commission() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 建链 [A=1, B=2, C=3]
        setup_single_line(entity_id, &[1, 2, 3]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_UPLINE);
        // 新买家 D=4 首单，应先加入链再计算分佣
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &4, 100_000, 100_000, modes, true, 1, 0,
        );

        // D 应在链尾 index=3
        assert_eq!(pallet::SingleLineIndex::<Test>::get(entity_id, 4), Some(3));
        assert_eq!(SingleLine::single_line_length(entity_id), 4);

        // 上线分佣：config base_upline_levels=10, upline_rate=1%=100/10000
        // D(idx=3) → C(idx=2, level=1), B(idx=1, level=2), A(idx=0, level=3)
        // 每层 100_000 * 100 / 10000 = 1000
        let upline_outputs: Vec<_> = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineUpline)
            .collect();
        assert_eq!(upline_outputs.len(), 3);
        assert_eq!(upline_outputs[0].beneficiary, 3); // C, level=1
        assert_eq!(upline_outputs[0].amount, 1000u128);
        assert_eq!(upline_outputs[1].beneficiary, 2); // B, level=2
        assert_eq!(upline_outputs[1].amount, 1000u128);
        assert_eq!(upline_outputs[2].beneficiary, 1); // A, level=3
        assert_eq!(upline_outputs[2].amount, 1000u128);
        assert_eq!(remaining, 100_000 - 3000);
    });
}

#[test]
fn first_order_buyer_no_downline_commission() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_config(entity_id);
        // 建链 [A=1, B=2, C=3]
        setup_single_line(entity_id, &[1, 2, 3]);

        let modes = CommissionModes(CommissionModes::SINGLE_LINE_DOWNLINE);
        // 新买家 D=4 首单，加入链尾后无下线
        let (outputs, remaining) = <SingleLine as CommissionPlugin<u64, u128>>::calculate(
            entity_id, &4, 100_000, 100_000, modes, true, 1, 0,
        );

        // D 应在链尾 index=3
        assert_eq!(pallet::SingleLineIndex::<Test>::get(entity_id, 4), Some(3));
        // 无下线，不应产生 downline 分佣
        let downline_outputs: Vec<_> = outputs
            .iter()
            .filter(|o| o.commission_type == CommissionType::SingleLineDownline)
            .collect();
        assert_eq!(downline_outputs.len(), 0);
        assert_eq!(remaining, 100_000);
    });
}

// ============================================================================
// 双向覆盖 (Bidirectional Reach) 测试
// ============================================================================

/// Helper: 小 max 配置，便于测试双向覆盖
fn setup_bidirectional_config(entity_id: u64) {
    setup_entity(entity_id);
    assert_ok!(SingleLine::set_single_line_config(
        RuntimeOrigin::signed(OWNER),
        entity_id,
        100, // upline_rate = 1%
        100, // downline_rate = 1%
        2,   // base_upline_levels
        2,   // base_downline_levels
        0,   // level_increment_threshold (no extra levels by default)
        10,  // max_upline_levels
        10,  // max_downline_levels
        ReachMode::Bidirectional,
    ));
}

/// 上线方向双向覆盖：高等级上方受益人通过自身下线层数覆盖低等级买家。
///
/// 队列: [A(Lv6), B, C, D, E, F, G(Lv1)]
/// 买家 G(Lv1, base_up=2) 购买 → 上线方向遍历
/// - F (距离1, ≤ buyer_up=2) → 正常覆盖
/// - E (距离2, ≤ buyer_up=2) → 正常覆盖
/// - D (距离3, > buyer_up=2) → D 无覆盖, D.down=2, 3>2 → 跳过
/// - C (距离4, > buyer_up=2) → C 无覆盖, C.down=2, 4>2 → 跳过
/// - B (距离5, > buyer_up=2) → B 无覆盖, B.down=2, 5>2 → 跳过
/// - A (距离6, > buyer_up=2) → A.Lv6 覆盖 down=8, 6≤8 → 覆盖!
#[test]
fn bidirectional_upline_beneficiary_reaches_down() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_bidirectional_config(entity_id);
        // A=10(#0) B=20(#1) C=30(#2) D=40(#3) E=50(#4) F=60(#5) G=70(#6)
        setup_single_line(entity_id, &[10, 20, 30, 40, 50, 60, 70]);

        // A(10) 等级 6，覆盖 down=8
        set_custom_level(entity_id, 10, 6);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            6,
            2,
            8,
        ));

        // G(70) 等级 1，使用基础 base_up=2
        set_custom_level(entity_id, 70, 1);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &70, &config);
        assert_eq!(base_up, 2); // buyer G 使用基础值

        SingleLine::process_upline(
            entity_id,
            &70,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 预期: F(60) L1, E(50) L2 (buyer reach), A(10) L6 (A's down reach)
        // B(20)=L5, C(30)=L4, D(40)=L3 被跳过 (双方都覆盖不到)
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 60); // L1
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 50); // L2
        assert_eq!(outputs[1].level, 2);
        assert_eq!(outputs[2].beneficiary, 10); // L6 (A's downline reach)
        assert_eq!(outputs[2].level, 6);
        // 每层 1_000_000 × 1% = 10_000, 共 3 层
        assert_eq!(remaining, 1_000_000 - 3 * 10_000);
    });
}

/// 下线方向双向覆盖：高等级下方受益人通过自身上线层数覆盖低等级买家。
///
/// 队列: [A(Lv1), B, C, D, E, F, G(Lv6)]
/// 买家 A(Lv1, base_down=2) 购买 → 下线方向遍历
/// - B (距离1, ≤ buyer_down=2) → 正常
/// - C (距离2, ≤ buyer_down=2) → 正常
/// - D (距离3, > buyer_down=2) → D.up=2, 3>2 → 跳过
/// - E (距离4, > buyer_down=2) → E.up=2, 4>2 → 跳过
/// - F (距离5, > buyer_down=2) → F.up=2, 5>2 → 跳过
/// - G (距离6, > buyer_down=2) → G.Lv6 覆盖 up=8, 6≤8 → 覆盖!
#[test]
fn bidirectional_downline_beneficiary_reaches_up() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_bidirectional_config(entity_id);
        setup_single_line(entity_id, &[10, 20, 30, 40, 50, 60, 70]);

        // G(70) 等级 6，覆盖 up=8
        set_custom_level(entity_id, 70, 6);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            6,
            8,
            2,
        ));

        // A(10) 等级 1，使用基础 base_down=2
        set_custom_level(entity_id, 10, 1);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        assert_eq!(base_down, 2);

        SingleLine::process_downline(
            entity_id,
            &10,
            1_000_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 预期: B(20) L1, C(30) L2 (buyer reach), G(70) L6 (G's upline reach)
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 20); // L1
        assert_eq!(outputs[1].beneficiary, 30); // L2
        assert_eq!(outputs[2].beneficiary, 70); // L6
        assert_eq!(outputs[2].level, 6);
        assert_eq!(remaining, 1_000_000 - 3 * 10_000);
    });
}

/// 双方都够不到时跳过。
///
/// 队列: [A, B, C, D, E], 全部 Lv0, base_up=1, base_down=1, max=5
/// 买家 E(#4) 购买 → 上线方向
/// - D(#3) L1 → ≤ buyer_up=1 → 正常
/// - C(#2) L2 → > buyer_up=1, C.down=1, 2>1 → 跳过
/// - B(#1) L3 → > buyer_up=1, B.down=1, 3>1 → 跳过
/// - A(#0) L4 → > buyer_up=1, A.down=1, 4>1 → 跳过
#[test]
fn bidirectional_neither_side_reaches() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            0,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        assert_eq!(base_up, 1);

        SingleLine::process_upline(
            entity_id,
            &50,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 只有 D(40) 在 buyer reach 范围内
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 40);
        assert_eq!(outputs[0].level, 1);
    });
}

/// 受益人通过 extra_levels (佣金收入) 达到覆盖。
///
/// 队列: [A, B, C, D], base_up=1, base_down=1, threshold=1000, max=5
/// A 累计佣金收入 2000 → extra=2, A.effective_down = 1+2 = 3
/// 买家 D(#3), buyer_up=1
/// - C(#2) L1 → ≤ buyer_up=1 → 正常
/// - B(#1) L2 → > buyer_up=1, B.down=1+0=1, 2>1 → 跳过
/// - A(#0) L3 → > buyer_up=1, A.down=1+2=3, 3≤3 → 覆盖!
#[test]
fn bidirectional_with_extra_levels_on_beneficiary() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40]);

        // A(10) 累计佣金 2000 → extra = 2000/1000 = 2
        set_member_stats(entity_id, 10, 2000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &40, &config);
        assert_eq!(base_up, 1);

        SingleLine::process_upline(
            entity_id,
            &40,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // C(30) L1 (buyer reach), A(10) L3 (A's down=1+extra=2=3)
        // B(20) L2 被跳过 (down=1+0=1, 2>1)
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 30); // L1
        assert_eq!(outputs[1].beneficiary, 10); // L3
        assert_eq!(outputs[1].level, 3);
    });
}

/// banned 成员跳过，不触发额外查询，但不阻断后续成员。
///
/// 队列: [A(Lv6,down=4), B(banned), C, D]
/// 买家 D(#3), buyer_up=1
/// - C(#2) L1 → buyer reach → 正常
/// - B(#1) L2 → is_member_skipped → continue (不查等级)
/// - A(#0) L3 → A.down=4, 3≤4 → 覆盖!
#[test]
fn bidirectional_skipped_member_no_extra_lookup() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            0,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40]);

        // A(10) Lv6 覆盖 down=4
        set_custom_level(entity_id, 10, 6);
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            6,
            1,
            4,
        ));

        // B(20) 被 ban
        set_banned(entity_id, 20);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &40, &config);

        SingleLine::process_upline(
            entity_id,
            &40,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // C(30) L1 (buyer reach), B(20) skipped, A(10) L3 (bidirectional)
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 30); // L1
        assert_eq!(outputs[1].beneficiary, 10); // L3
    });
}

/// buyer 消费增层在 Bidirectional 下扩展条件 A 的范围。
///
/// 队列: [A, B, C, D, E], base_up=1, threshold=1000, max=5
/// buyer=E(#4), spending=2000 → extra=2 → buyer_effective_up=1+2=3
/// - D(#3) L1 → ≤3 → 条件 A 直接通过
/// - C(#2) L2 → ≤3 → 条件 A 直接通过
/// - B(#1) L3 → ≤3 → 条件 A 直接通过 (因 extra_levels 扩展)
/// - A(#0) L4 → >3, A.down=1, 4>1 → 跳过
#[test]
fn bidirectional_buyer_extra_levels_extend_condition_a() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40, 50]);

        // buyer=50 spending=2000 → extra=2 → effective_up=1+2=3
        set_member_stats(entity_id, 50, 2000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &50, &config);
        assert_eq!(base_up, 1);

        SingleLine::process_upline(
            entity_id,
            &50,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // D(40) L1, C(30) L2, B(20) L3 — all within buyer_effective=3
        // A(10) L4 — out of buyer reach, A.down=1, 4>1 → skip
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 40);
        assert_eq!(outputs[1].beneficiary, 30);
        assert_eq!(outputs[2].beneficiary, 20);
    });
}

/// buyer + 受益人同时有消费增层的复合场景。
///
/// 队列: [A, B, C, D, E, F], base_up=1, base_down=1, threshold=1000, max=10
/// buyer=F(#5), spending=1000 → extra=1 → buyer_effective_up=2
/// - E(#4) L1 → ≤2 条件 A 通过
/// - D(#3) L2 → ≤2 条件 A 通过
/// - C(#2) L3 → >2, C spending=0, C.down=1, 3>1 → 跳过
/// - B(#1) L4 → >2, B spending=3000 → extra=3, B.down=1+3=4, 4≤4 → 条件 B 通过!
/// - A(#0) L5 → >2, A spending=0, A.down=1, 5>1 → 跳过
#[test]
fn bidirectional_both_buyer_and_beneficiary_extra_levels() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            10,
            10,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40, 50, 60]);

        // buyer=60 spending=1000 → extra=1 → buyer_effective_up=2
        set_member_stats(entity_id, 60, 1000);
        // B=20 spending=3000 → extra=3 → B.effective_down=1+3=4
        set_member_stats(entity_id, 20, 3000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &60, &config);

        SingleLine::process_upline(
            entity_id,
            &60,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // E(50) L1 (A), D(40) L2 (A), B(20) L4 (B's reverse reach=4)
        // C(30) L3 skipped, A(10) L5 skipped
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 50); // L1 — buyer reach
        assert_eq!(outputs[1].beneficiary, 40); // L2 — buyer reach
        assert_eq!(outputs[2].beneficiary, 20); // L4 — beneficiary reverse reach
        assert_eq!(outputs[2].level, 4);
    });
}

/// 精确边界: i == buyer_effective == beneficiary_reverse。
///
/// 两个条件恰好在边界都满足 (<=)，确认不 off-by-one。
///
/// 队列: [A, B, C, D], base_up=2, base_down=2, threshold=0, max=5
/// buyer=D(#3), buyer_effective_up=2
/// - C(#2) L1 → ≤2 条件 A
/// - B(#1) L2 → ≤2 条件 A (边界 i==buyer_effective)
/// - A(#0) L3 → >2, 查 A: A.down=2, 3>2 → 跳过
///
/// 现在给 A spending使其 A.down=3 (base=2+extra=1): i=3 == A.effective_down=3 → 条件 B 通过
#[test]
fn bidirectional_exact_boundary_both_conditions() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            2,
            2,
            1000,
            5,
            5,
            ReachMode::Bidirectional,
        ));
        setup_single_line(entity_id, &[10, 20, 30, 40]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();

        // Part 1: buyer=40(#3), buyer_effective_up=2
        // A(10) at L3 → >2, A.down=2, 3>2 → skip
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &40, &config);
        assert_eq!(base_up, 2);

        SingleLine::process_upline(
            entity_id,
            &40,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 2); // C(30) L1, B(20) L2 — buyer boundary exact
        assert_eq!(outputs[0].beneficiary, 30);
        assert_eq!(outputs[0].level, 1);
        assert_eq!(outputs[1].beneficiary, 20);
        assert_eq!(outputs[1].level, 2); // i==buyer_effective → still included

        // Part 2: give A spending=1000 → extra=1 → A.effective_down=2+1=3
        // A at L3: i=3 == A.effective_down=3 → condition B passes (<=)
        set_member_stats(entity_id, 10, 1000);

        remaining = 1_000_000;
        outputs.clear();
        SingleLine::process_upline(
            entity_id,
            &40,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 3); // C(30) L1, B(20) L2, A(10) L3
        assert_eq!(outputs[2].beneficiary, 10);
        assert_eq!(outputs[2].level, 3); // i==ben_effective → still included
    });
}

/// 下线方向的 extra_levels + override 混合场景。
///
/// 队列: [A, B, C, D, E, F, G], buyer=A(#0)
/// base_down=1, threshold=1000, max_down=10
/// buyer=A spending=1000 → extra=1 → buyer_effective_down=2
/// - B(#1) L1 → ≤2 条件 A
/// - C(#2) L2 → ≤2 条件 A (边界)
/// - D(#3) L3 → >2, D spending=0, D.up=1, 3>1 → skip
/// - E(#4) L4 → >2, E level_override up=6 → 4≤6 → 条件 B
/// - F(#5) L5 → >2, F spending=5000 → extra=5, F.up=1+5=6, 5≤6 → 条件 B
/// - G(#6) L6 → >2, G spending=0, G.up=1, 6>1 → skip
#[test]
fn bidirectional_downline_mixed_extra_and_override() {
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            1,
            1,
            1000,
            10,
            10,
            ReachMode::Bidirectional,
        ));

        // E(50) level override: level 4 → up=6
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            4,
            6,
            1,
        ));

        setup_single_line(entity_id, &[10, 20, 30, 40, 50, 60, 70]);

        // buyer=A(10) spending=1000 → extra=1 → buyer_effective_down=2
        set_member_stats(entity_id, 10, 1000);
        // E(50) custom level=4 → override up=6
        set_custom_level(entity_id, 50, 4);
        // F(60) spending=5000 → extra=5 → F.effective_up=1+5=6
        set_member_stats(entity_id, 60, 5000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &10, &config);
        assert_eq!(base_down, 1);

        SingleLine::process_downline(
            entity_id,
            &10,
            1_000_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // B(20) L1 (A), C(30) L2 (A), E(50) L4 (B override), F(60) L5 (B extra)
        // D(40) L3 skip, G(70) L6 skip
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].beneficiary, 20); // L1 buyer reach
        assert_eq!(outputs[1].beneficiary, 30); // L2 buyer reach boundary
        assert_eq!(outputs[2].beneficiary, 50); // L4 override reverse
        assert_eq!(outputs[2].level, 4);
        assert_eq!(outputs[3].beneficiary, 60); // L5 extra reverse
        assert_eq!(outputs[3].level, 5);
    });
}

// ============================================================================
// BuyerOnly reach mode tests
// ============================================================================

/// Helper: setup config with BuyerOnly mode.
fn setup_buyer_only_config(
    entity_id: u64,
    base_up: u8,
    base_down: u8,
    max_up: u8,
    max_down: u8,
    threshold: u128,
) {
    setup_entity(entity_id);
    assert_ok!(SingleLine::set_single_line_config(
        RuntimeOrigin::signed(OWNER),
        entity_id,
        100, // upline_rate 1%
        100, // downline_rate 1%
        base_up,
        base_down,
        threshold,
        max_up,
        max_down,
        ReachMode::BuyerOnly,
    ));
}

#[test]
fn buyer_only_upline_basic() {
    // Chain: [1, 2, 3, 4, 5], buyer=5 (index=4)
    // base_up=3, buyer_effective_up=3
    // L1→4, L2→3, L3→2 awarded; L4→1 out of range
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 3, 3, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 4); // L1
        assert_eq!(outputs[1].beneficiary, 3); // L2
        assert_eq!(outputs[2].beneficiary, 2); // L3
    });
}

#[test]
fn buyer_only_downline_basic() {
    // Chain: [1, 2, 3, 4, 5], buyer=1 (index=0)
    // base_down=3, buyer_effective_down=3
    // L1→2, L2→3, L3→4 awarded; L4→5 out of range
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 3, 3, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);

        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 2); // L1
        assert_eq!(outputs[1].beneficiary, 3); // L2
        assert_eq!(outputs[2].beneficiary, 4); // L3
    });
}

#[test]
fn buyer_only_zero_reach_no_commission() {
    // base_up=0, base_down=0, threshold=0 → extra=0 → buyer_effective=0
    // Loop range 1..=0 → no iteration
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 0, 0, 10, 10, 0);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();

        SingleLine::process_upline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            0,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 0);

        SingleLine::process_downline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            0,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 0);
    });
}

#[test]
fn buyer_only_extra_levels_from_spending() {
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_up=1, threshold=1000 → buyer no spending → effective=1
    // Then give buyer spending=2000 → extra=2 → effective=3
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 1, 1, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();

        // Before: no spending → effective_up=1
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 1); // only account 4

        // Give buyer spending=2000 → extra=2 → effective=3
        set_member_stats(entity_id, 5, 2000);

        remaining = 100_000;
        outputs.clear();
        // base_up unchanged (from override/config), extra calculated inside process_upline
        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 3); // 4(L1), 3(L2), 2(L3)
        assert_eq!(outputs[0].beneficiary, 4);
        assert_eq!(outputs[1].beneficiary, 3);
        assert_eq!(outputs[2].beneficiary, 2);
    });
}

#[test]
fn buyer_only_level_override_extends_reach() {
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_up=1, max=10. Give buyer level 2 override up=5.
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 1, 1, 10, 10, 1000);

        // Set level 2 override: upline=5, downline=1
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            2,
            5, // upline_levels
            1, // downline_levels
        ));

        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        // Without override: base_up=1 → only 1 upline
        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        assert_eq!(base_up, 1); // no custom level yet

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 1);

        // Now set buyer=5 to custom level 2 → override up=5
        set_custom_level(entity_id, 5, 2);
        remaining = 100_000;
        outputs.clear();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        assert_eq!(base_up, 5);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        // 4 accounts above buyer (4,3,2,1), all within reach=5
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].beneficiary, 4);
        assert_eq!(outputs[3].beneficiary, 1);
    });
}

#[test]
fn buyer_only_beneficiary_levels_irrelevant() {
    // BuyerOnly: beneficiary's own levels should NOT affect commission.
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_up=1 → buyer can only reach 1 layer.
    // Give all beneficiaries massive spending and level overrides.
    // They should still only get L1 (account 4).
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 1, 1, 10, 10, 1000);

        // Level 3 override: downline=10 (huge reverse reach)
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            3,
            10, // upline
            10, // downline
        ));

        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        // Give all beneficiaries level 3 and huge spending
        for acc in [1, 2, 3, 4] {
            set_custom_level(entity_id, acc, 3);
            set_member_stats(entity_id, acc, 100_000);
        }

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // Despite all beneficiaries having huge reach, buyer can only see 1 layer
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 4);
    });
}

#[test]
fn buyer_only_skipped_members_consume_layers() {
    // Chain: [1, 2, 3, 4, 5], buyer=5, base_up=3
    // Ban account 4 (L1), freeze account 3 (L2)
    // L1→4 skipped, L2→3 skipped, L3→2 awarded. Account 1 (L4) out of range.
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 3, 3, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        set_banned(entity_id, 4);
        set_member_frozen(entity_id, 3);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 4(L1) banned skip, 3(L2) frozen skip, 2(L3) ok. 1(L4) out of range.
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[0].level, 3); // level 3, not level 1
    });
}

#[test]
fn buyer_only_effective_clamped_by_max() {
    // base_up=5, extra from spending=10, but max_upline=8 → effective=8 not 15
    // Chain: [1..=12], buyer=12 (index=11)
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 5, 5, 8, 8, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);

        // buyer=12 spending=10000 → extra=10 → base(5)+extra(10)=15, clamped to max(8)
        set_member_stats(entity_id, 12, 10_000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &12, &config);

        SingleLine::process_upline(
            entity_id,
            &12,
            1_000_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // Effective=min(5+10, 8)=8. Accounts 11,10,9,8,7,6,5,4 → 8 outputs
        assert_eq!(outputs.len(), 8);
        assert_eq!(outputs[0].beneficiary, 11); // L1
        assert_eq!(outputs[7].beneficiary, 4); // L8
    });
}

#[test]
fn buyer_only_downline_with_fewer_members_than_reach() {
    // Chain: [1, 2, 3], buyer=1 (index=0)
    // base_down=10, but only 2 members below → only 2 outputs
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 10, 10, 20, 20, 1000);
        setup_single_line(entity_id, &[1, 2, 3]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);

        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[1].beneficiary, 3);
    });
}

#[test]
fn buyer_only_buyer_at_head_no_upline() {
    // buyer=1 (index=0) → process_upline returns immediately
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 10, 10, 20, 20, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &1, &config);

        SingleLine::process_upline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 0);
    });
}

#[test]
fn buyer_only_buyer_at_tail_no_downline() {
    // buyer=5 (index=4, last) → process_downline returns immediately
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_buyer_only_config(entity_id, 10, 10, 20, 20, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_downline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 0);
    });
}

// ============================================================================
// BeneficiaryOnly reach mode tests
// ============================================================================

/// Helper: setup config with BeneficiaryOnly mode.
/// upline_rate=100 (1%), downline_rate=100 (1%),
/// base_upline_levels / base_downline_levels / max customizable.
fn setup_beneficiary_only_config(
    entity_id: u64,
    base_up: u8,
    base_down: u8,
    max_up: u8,
    max_down: u8,
    threshold: u128,
) {
    setup_entity(entity_id);
    assert_ok!(SingleLine::set_single_line_config(
        RuntimeOrigin::signed(OWNER),
        entity_id,
        100, // upline_rate 1%
        100, // downline_rate 1%
        base_up,
        base_down,
        threshold,
        max_up,
        max_down,
        ReachMode::BeneficiaryOnly,
    ));
}

#[test]
fn beneficiary_only_upline_basic() {
    // Chain: [1, 2, 3, 4, 5], buyer=5 (index=4)
    // base_down=3 → beneficiary's downline reach determines commission.
    // Beneficiary 4 (L1): down_reach=3, 1<=3 → awarded
    // Beneficiary 3 (L2): down_reach=3, 2<=3 → awarded
    // Beneficiary 2 (L3): down_reach=3, 3<=3 → awarded
    // Beneficiary 1 (L4): down_reach=3, 4>3  → skipped
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 3, 3, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 4); // L1
        assert_eq!(outputs[1].beneficiary, 3); // L2
        assert_eq!(outputs[2].beneficiary, 2); // L3
                                               // Account 1 (L4) is out of range — not included
    });
}

#[test]
fn beneficiary_only_downline_basic() {
    // Chain: [1, 2, 3, 4, 5], buyer=1 (index=0)
    // base_up=3 → beneficiary's upline reach determines commission.
    // Beneficiary 2 (L1): up_reach=3, 1<=3 → awarded
    // Beneficiary 3 (L2): up_reach=3, 2<=3 → awarded
    // Beneficiary 4 (L3): up_reach=3, 3<=3 → awarded
    // Beneficiary 5 (L4): up_reach=3, 4>3  → skipped
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 3, 3, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);

        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].beneficiary, 2); // L1
        assert_eq!(outputs[1].beneficiary, 3); // L2
        assert_eq!(outputs[2].beneficiary, 4); // L3
    });
}

#[test]
fn beneficiary_only_out_of_range_gets_nothing() {
    // Chain: [1, 2, 3], buyer=3 (index=2)
    // base_down=1 → each beneficiary can only reach 1 level down.
    // Beneficiary 2 (L1): down_reach=1, 1<=1 → awarded
    // Beneficiary 1 (L2): down_reach=1, 2>1  → skipped
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 1, 1, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &3, &config);

        SingleLine::process_upline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 2);
    });
}

#[test]
fn beneficiary_only_all_zero_reach_no_commission() {
    // Chain: [1, 2, 3], buyer=3
    // base_down=0, threshold=0 (no extra) → all beneficiaries have 0 downline reach.
    // Nobody gets commission.
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_entity(entity_id);
        assert_ok!(SingleLine::set_single_line_config(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            100,
            100,
            0, // base_upline_levels
            0, // base_downline_levels
            0, // threshold=0 → calc_extra_levels returns 0
            10,
            10,
            ReachMode::BeneficiaryOnly,
        ));
        setup_single_line(entity_id, &[1, 2, 3]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();

        SingleLine::process_upline(
            entity_id,
            &3,
            100_000,
            &mut remaining,
            &config,
            0,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 0);

        SingleLine::process_downline(
            entity_id,
            &1,
            100_000,
            &mut remaining,
            &config,
            0,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 0);
    });
}

#[test]
fn beneficiary_only_extra_levels_from_spending() {
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_down=1, threshold=1000, max_down=10.
    // Beneficiary 4: no spending → down_reach=1, L1<=1 → awarded
    // Beneficiary 3: no spending → down_reach=1, L2>1  → skipped
    // Now give beneficiary 3 total_earned=2000 → extra=2 → down_reach=3
    // Beneficiary 3: down_reach=3, L2<=3 → awarded
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 1, 1, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();

        // Before: beneficiary 3 has no spending
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        assert_eq!(outputs.len(), 1); // only account 4

        // Give beneficiary 3 some spending → extra=2
        set_member_stats(entity_id, 3, 2000);

        remaining = 100_000;
        outputs.clear();
        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );
        // Now: 4 (L1, reach=1, ok), 3 (L2, reach=1+2=3, ok), 2 (L3, reach=1, skip), 1 (L4, reach=1, skip)
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 4);
        assert_eq!(outputs[1].beneficiary, 3);
    });
}

#[test]
fn beneficiary_only_level_override_extends_reach() {
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_down=1, max_down=10.
    // Give account 2 custom level=2, set level 2 override downline_levels=5.
    // Beneficiary 2 (L3): base=1, but override→5, down_reach=5, 3<=5 → awarded
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 1, 1, 10, 10, 1000);

        // Set level 2 override: upline=1, downline=5
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            2, // level_id
            1, // upline_levels
            5, // downline_levels
        ));

        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        // Give account 2 custom level = 2
        set_custom_level(entity_id, 2, 2);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 4 (L1): base down=1, 1<=1 ok
        // 3 (L2): base down=1, 2>1 skip
        // 2 (L3): override down=5, 3<=5 ok
        // 1 (L4): base down=1, 4>1 skip
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 4);
        assert_eq!(outputs[1].beneficiary, 2);
    });
}

#[test]
fn beneficiary_only_buyer_level_is_irrelevant() {
    // BeneficiaryOnly: buyer's own levels should NOT affect commission.
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_up=1, base_down=1, max=10.
    // Give buyer (5) level override: upline=10, downline=10.
    // In BeneficiaryOnly, buyer's huge reach should be ignored.
    // Only beneficiary's reverse reach determines commission.
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 1, 1, 10, 10, 1000);

        // Set level 3 override: upline=10, downline=10
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            3,  // level_id
            10, // upline_levels
            10, // downline_levels
        ));

        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        // Give buyer (5) custom level = 3 → override up=10, down=10
        set_custom_level(entity_id, 5, 3);
        // Give buyer massive spending too
        set_member_stats(entity_id, 5, 100_000);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);
        // base_up = 10 (from override), but this is the BUYER's reach — irrelevant in BeneficiaryOnly

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // Despite buyer having reach=10, each beneficiary only has base_down=1:
        // 4 (L1): down=1, 1<=1 ok
        // 3 (L2): down=1, 2>1 skip
        // 2 (L3): down=1, 3>1 skip
        // 1 (L4): down=1, 4>1 skip
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].beneficiary, 4);
    });
}

#[test]
fn beneficiary_only_downline_with_mixed_reach() {
    // Chain: [1, 2, 3, 4, 5, 6], buyer=1 (index=0)
    // base_up=2, max_up=10, threshold=1000.
    // BeneficiaryOnly: each beneficiary's upline reach matters.
    // 2 (L1): up=2, 1<=2 ok
    // 3 (L2): up=2, 2<=2 ok
    // 4 (L3): up=2, 3>2 skip — but give account 4 spending=1000 → extra=1 → up=3, 3<=3 ok
    // 5 (L4): up=2, 4>2 skip
    // 6 (L5): up=2, 5>2 skip — but give level override up=6 → 5<=6 ok
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 2, 2, 10, 10, 1000);

        // Set level 5 override: upline=6, downline=1
        assert_ok!(SingleLine::set_level_based_levels(
            RuntimeOrigin::signed(OWNER),
            entity_id,
            5, // level_id
            6, // upline_levels
            1, // downline_levels
        ));

        setup_single_line(entity_id, &[1, 2, 3, 4, 5, 6]);

        // Account 4: spending=1000 → extra=1
        set_member_stats(entity_id, 4, 1000);
        // Account 6: custom level=5 → override up=6
        set_custom_level(entity_id, 6, 5);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 1_000_000;
        let mut outputs = Vec::new();
        let (_, base_down) = SingleLine::effective_base_levels(entity_id, &1, &config);

        SingleLine::process_downline(
            entity_id,
            &1,
            1_000_000,
            &mut remaining,
            &config,
            base_down,
            &mut outputs,
        );

        // 2 (L1): up=2, ok
        // 3 (L2): up=2, ok
        // 4 (L3): up=2+1=3, ok
        // 5 (L4): up=2, skip
        // 6 (L5): override up=6, ok
        assert_eq!(outputs.len(), 4);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[1].beneficiary, 3);
        assert_eq!(outputs[2].beneficiary, 4);
        assert_eq!(outputs[3].beneficiary, 6);
    });
}

#[test]
fn beneficiary_only_skipped_members_not_counted() {
    // Banned/frozen beneficiaries should be skipped even if they have reach.
    // Chain: [1, 2, 3, 4, 5], buyer=5
    // base_down=5 → all beneficiaries have enough reach.
    // Ban account 3, freeze account 4.
    // Only 2 and 1 should get commission (account 4 at L1 skipped, 3 at L2 skipped).
    new_test_ext().execute_with(|| {
        let entity_id = 1u64;
        setup_beneficiary_only_config(entity_id, 5, 5, 10, 10, 1000);
        setup_single_line(entity_id, &[1, 2, 3, 4, 5]);

        set_banned(entity_id, 3);
        set_member_frozen(entity_id, 4);

        let config = pallet::SingleLineConfigs::<Test>::get(entity_id).unwrap();
        let mut remaining: u128 = 100_000;
        let mut outputs = Vec::new();
        let (base_up, _) = SingleLine::effective_base_levels(entity_id, &5, &config);

        SingleLine::process_upline(
            entity_id,
            &5,
            100_000,
            &mut remaining,
            &config,
            base_up,
            &mut outputs,
        );

        // 4 (L1) frozen → skip, 3 (L2) banned → skip, 2 (L3) ok, 1 (L4) ok
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].beneficiary, 2);
        assert_eq!(outputs[1].beneficiary, 1);
    });
}
