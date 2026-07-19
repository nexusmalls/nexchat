// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for `pallet-usdx`.
//! `pallet-usdx` 单元测试。

use crate::{
    mock::*,
    pallet::{AdminUsdxDebt, CollateralUsdxDebt, Error, GlobalUsdxDebtCeiling, TotalUsdxDebt},
    types::{CollateralPolicy, LaneActivationEvidence, LaneLimits},
};
use frame_support::{
    assert_noop, assert_ok,
    traits::fungibles::{Inspect, Mutate},
};

#[test]
fn unit_adapters_are_fail_closed() {
    assert_eq!(
        <() as crate::ReceiptValidator>::descriptor_hash(900_001),
        None
    );
    assert!(!<() as crate::ReceiptValidator>::validate_evidence(
        900_001,
        sp_core::H256::zero(),
        &evidence(),
    ));
    assert!(
        !<() as crate::ProtocolAssetInspector<AccountId>>::validate_usdx(USDX_ASSET_ID, &ADMIN,)
    );
    assert!(
        !<() as crate::ProtocolAssetInspector<AccountId>>::validate_receipt(POLYGON_RECEIPT_ID,)
    );
}

fn policy() -> CollateralPolicy {
    CollateralPolicy {
        mint_factor_bps: 10_000,
        mint_fee_bps: 100,
        redeem_fee_bps: 100,
    }
}

fn limits() -> LaneLimits<u64> {
    LaneLimits {
        min_amount: 100,
        max_per_tx: 100_000,
        max_per_window: 150_000,
        window_blocks: 10,
        debt_ceiling: 500_000,
    }
}

fn evidence() -> LaneActivationEvidence {
    LaneActivationEvidence {
        wrapper_contract: [0x11; 20],
        underlying_contract: [0x22; 20],
        owner_contract: [0x33; 20],
        host_contract: [0x44; 20],
        dispatcher_contract: [0x55; 20],
        is_weth: false,
        hft_bytecode_hash: sp_core::H256::repeat_byte(0x66),
        controller_bytecode_hash: sp_core::H256::repeat_byte(0x77),
        config_block: 1,
        config_block_hash: sp_core::H256::repeat_byte(0x88),
        nexus_peer_hash: sp_core::H256::repeat_byte(0x99),
        proof_bundle_hash: sp_core::H256::repeat_byte(0xAA),
    }
}

fn setup_polygon_lane() {
    assert_ok!(Usdx::set_global_debt_ceiling(
        RuntimeOrigin::root(),
        1_000_000
    ));
    assert_ok!(Usdx::register_collateral(
        RuntimeOrigin::root(),
        POLYGON_RECEIPT_ID,
        evidence(),
        policy(),
        limits(),
    ));
    assert_ok!(Usdx::set_enabled(
        RuntimeOrigin::root(),
        POLYGON_RECEIPT_ID,
        true
    ));
}

#[test]
fn mint_and_redeem_keep_debt_and_surplus_consistent() {
    new_test_ext().execute_with(|| {
        setup_polygon_lane();

        assert_ok!(Usdx::mint(
            RuntimeOrigin::signed(ALICE),
            POLYGON_RECEIPT_ID,
            10_000,
            9_900,
        ));
        assert_eq!(Assets::balance(USDX_ASSET_ID, ALICE), 9_900);
        assert_eq!(
            Assets::balance(POLYGON_RECEIPT_ID, Usdx::psm_account()),
            10_000
        );
        assert_eq!(CollateralUsdxDebt::<Test>::get(POLYGON_RECEIPT_ID), 9_900);
        assert_eq!(TotalUsdxDebt::<Test>::get(), 9_900);
        assert_eq!(Assets::total_issuance(USDX_ASSET_ID), 9_900);

        assert_ok!(Usdx::redeem(
            RuntimeOrigin::signed(ALICE),
            POLYGON_RECEIPT_ID,
            9_900,
            9_801,
        ));
        assert_eq!(Assets::balance(USDX_ASSET_ID, ALICE), 0);
        assert_eq!(
            Assets::balance(POLYGON_RECEIPT_ID, Usdx::psm_account()),
            199
        );
        assert_eq!(CollateralUsdxDebt::<Test>::get(POLYGON_RECEIPT_ID), 0);
        assert_eq!(TotalUsdxDebt::<Test>::get(), 0);
        assert_eq!(Assets::total_issuance(USDX_ASSET_ID), 0);
    });
}

#[test]
fn admin_credit_keeps_issuance_equal_total_debt() {
    new_test_ext().execute_with(|| {
        assert_ok!(Usdx::set_global_debt_ceiling(
            RuntimeOrigin::root(),
            1_000_000
        ));
        assert_ok!(Usdx::admin_credit_usdx(
            RuntimeOrigin::root(),
            ALICE,
            50_000
        ));
        assert_eq!(Assets::balance(USDX_ASSET_ID, ALICE), 50_000);
        assert_eq!(TotalUsdxDebt::<Test>::get(), 50_000);
        assert_eq!(AdminUsdxDebt::<Test>::get(), 50_000);
        assert_eq!(Assets::total_issuance(USDX_ASSET_ID), 50_000);
        assert_ok!(Usdx::check_accounting_invariants());
    });
}

#[test]
fn registration_is_disabled_until_governance_enables_lane() {
    new_test_ext().execute_with(|| {
        assert_ok!(Usdx::set_global_debt_ceiling(
            RuntimeOrigin::root(),
            1_000_000
        ));
        assert_ok!(Usdx::register_collateral(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            evidence(),
            policy(),
            limits(),
        ));

        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 1_000, 0,),
            Error::<Test>::LaneDisabled
        );
    });
}

#[test]
fn invalid_policy_and_unknown_receipt_are_rejected() {
    new_test_ext().execute_with(|| {
        let invalid = CollateralPolicy {
            mint_factor_bps: 0,
            mint_fee_bps: 0,
            redeem_fee_bps: 0,
        };
        assert_noop!(
            Usdx::register_collateral(
                RuntimeOrigin::root(),
                POLYGON_RECEIPT_ID,
                evidence(),
                invalid,
                limits(),
            ),
            Error::<Test>::InvalidPolicy
        );
        assert_noop!(
            Usdx::register_collateral(RuntimeOrigin::root(), 42, evidence(), policy(), limits(),),
            Error::<Test>::InvalidReceiptAsset
        );
    });
}

#[test]
fn activation_evidence_is_required_and_updates_only_while_disabled() {
    new_test_ext().execute_with(|| {
        let mut invalid = evidence();
        invalid.is_weth = true;
        assert_noop!(
            Usdx::register_collateral(
                RuntimeOrigin::root(),
                POLYGON_RECEIPT_ID,
                invalid,
                policy(),
                limits(),
            ),
            Error::<Test>::InvalidActivationEvidence
        );

        setup_polygon_lane();
        assert_noop!(
            Usdx::update_collateral(RuntimeOrigin::root(), POLYGON_RECEIPT_ID, evidence(),),
            Error::<Test>::LaneMustBeDisabled
        );

        assert_ok!(Usdx::set_enabled(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            false
        ));
        let mut updated = evidence();
        updated.config_block = 2;
        updated.proof_bundle_hash = sp_core::H256::repeat_byte(0xBB);
        assert_ok!(Usdx::update_collateral(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            updated.clone(),
        ));
        assert_eq!(Usdx::lane_evidence(POLYGON_RECEIPT_ID), Some(updated));
    });
}

#[test]
fn protocol_asset_configuration_is_rechecked_before_enable_and_use() {
    new_test_ext().execute_with(|| {
        assert_ok!(Usdx::set_global_debt_ceiling(
            RuntimeOrigin::root(),
            1_000_000
        ));
        assert_ok!(Usdx::register_collateral(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            evidence(),
            policy(),
            limits(),
        ));

        set_protocol_asset_config_valid(false);
        assert_noop!(
            Usdx::set_enabled(RuntimeOrigin::root(), POLYGON_RECEIPT_ID, true),
            Error::<Test>::AssetConfigMismatch
        );

        set_protocol_asset_config_valid(true);
        assert_ok!(Usdx::set_enabled(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            true
        ));
        set_protocol_asset_config_valid(false);
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 1_000, 0),
            Error::<Test>::AssetConfigMismatch
        );
    });
}

#[test]
fn global_and_lane_pause_block_user_operations() {
    new_test_ext().execute_with(|| {
        setup_polygon_lane();
        assert_ok!(Usdx::set_collateral_paused(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            true
        ));
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 1_000, 0,),
            Error::<Test>::LanePaused
        );

        assert_ok!(Usdx::set_collateral_paused(
            RuntimeOrigin::root(),
            POLYGON_RECEIPT_ID,
            false
        ));
        assert_ok!(Usdx::set_global_paused(RuntimeOrigin::root(), true));
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 1_000, 0,),
            Error::<Test>::Paused
        );
    });
}

#[test]
fn debt_ceiling_and_window_limit_are_enforced() {
    new_test_ext().execute_with(|| {
        setup_polygon_lane();
        assert_ok!(Usdx::mint(
            RuntimeOrigin::signed(ALICE),
            POLYGON_RECEIPT_ID,
            100_000,
            0,
        ));
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 100_000, 0,),
            Error::<Test>::WindowLimitExceeded
        );

        System::set_block_number(11);
        GlobalUsdxDebtCeiling::<Test>::put(99_000);
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 100, 0,),
            Error::<Test>::GlobalDebtCeilingExceeded
        );
    });
}

#[test]
fn forced_usdx_issuance_mismatch_stops_psm() {
    new_test_ext().execute_with(|| {
        setup_polygon_lane();
        assert_ok!(<Assets as Mutate<AccountId>>::mint_into(
            USDX_ASSET_ID,
            &BOB,
            1,
        ));
        assert_noop!(
            Usdx::mint(RuntimeOrigin::signed(ALICE), POLYGON_RECEIPT_ID, 1_000, 0,),
            Error::<Test>::AccountingInvariantViolated
        );
    });
}

#[test]
fn diagnostic_invariant_checks_lane_attribution_and_solvency() {
    new_test_ext().execute_with(|| {
        setup_polygon_lane();
        assert_ok!(Usdx::mint(
            RuntimeOrigin::signed(ALICE),
            POLYGON_RECEIPT_ID,
            10_000,
            0,
        ));
        assert_eq!(Usdx::check_accounting_invariants(), Ok(()));

        CollateralUsdxDebt::<Test>::insert(POLYGON_RECEIPT_ID, 9_901);
        assert_eq!(
            Usdx::check_accounting_invariants(),
            Err("sum(collateral debt) != total USDX debt")
        );

        CollateralUsdxDebt::<Test>::insert(POLYGON_RECEIPT_ID, 20_000);
        TotalUsdxDebt::<Test>::put(20_000);
        assert_eq!(
            Usdx::check_accounting_invariants(),
            Err("receipt lane balance is below attributed debt")
        );
    });
}
