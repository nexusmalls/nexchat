use crate::{
    mock::*,
    pallet::{Error, WhitelistedAssets},
};
use frame_support::{
    assert_noop, assert_ok,
    traits::{
        fungibles::{Inspect, Mutate},
        tokens::Preservation,
    },
};
use orml_traits::{MultiCurrency, MultiReservableCurrency};
use pallet_prediction_control::{PredictionMode, PredictionModule};
use proptest::prelude::*;
use sp_runtime::DispatchError;
use zeitgeist_primitives::{traits::PredictionBaseAssetPolicy, types::Asset};

fn set_full_and_whitelist(asset_id: AssetId) {
    assert_ok!(PredictionControl::set_prediction_mode(
        RuntimeOrigin::root(),
        PredictionMode::Full,
    ));
    assert_ok!(PredictionCollateral::set_asset_whitelisted(
        RuntimeOrigin::root(),
        asset_id,
        true,
    ));
}

fn mirror(asset_id: AssetId) -> Asset<MarketId> {
    Asset::ForeignAsset(asset_id)
}

fn assert_consistent(asset_id: AssetId) {
    assert!(PredictionCollateral::is_mirror_consistent(asset_id));
    assert_eq!(
        PredictionCollateral::mirror_issuance(asset_id),
        PredictionCollateral::escrow_balance(asset_id)
    );
}

#[test]
fn defaults_are_empty_whitelist_disabled_and_denied() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            PredictionControl::prediction_mode(),
            PredictionMode::Disabled
        );
        assert_eq!(WhitelistedAssets::<Test>::get(USDX_ASSET_ID), None);
        assert!(!PredictionCollateral::is_deposit_allowed(USDX_ASSET_ID));
        assert!(
            !<PredictionCollateral as PredictionBaseAssetPolicy<u64>>::is_allowed(USDX_ASSET_ID)
        );
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1),
            Error::<Test>::PredictionModeNotFull
        );
    });
}

#[test]
fn governance_calls_require_root() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            PredictionCollateral::set_asset_whitelisted(
                RuntimeOrigin::signed(ALICE),
                USDX_ASSET_ID,
                true,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            PredictionCollateral::set_asset_deposit_paused(
                RuntimeOrigin::signed(ALICE),
                USDX_ASSET_ID,
                true,
            ),
            DispatchError::BadOrigin
        );
        assert_noop!(
            PredictionCollateral::set_global_deposit_paused(RuntimeOrigin::signed(ALICE), true,),
            DispatchError::BadOrigin
        );

        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        assert_ok!(PredictionCollateral::set_asset_deposit_paused(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        assert_ok!(PredictionCollateral::set_global_deposit_paused(
            RuntimeOrigin::root(),
            true,
        ));
    });
}

#[test]
fn full_whitelisted_deposit_and_partial_withdraw_roundtrip() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            10_000,
        ));
        assert_consistent(USDX_ASSET_ID);
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::free_balance(
                mirror(USDX_ASSET_ID),
                &ALICE,
            ),
            10_000
        );

        assert_ok!(PredictionCollateral::withdraw(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            4_000,
        ));
        assert_consistent(USDX_ASSET_ID);
        assert_eq!(
            Assets::balance(USDX_ASSET_ID, ALICE),
            INITIAL_ASSET_BALANCE - 6_000
        );
        assert_eq!(PredictionCollateral::mirror_issuance(USDX_ASSET_ID), 6_000);
    });
}

#[test]
fn nex_uses_balances_and_never_creates_orml_native_issuance() {
    new_test_ext().execute_with(|| {
        let before = Balances::free_balance(ALICE);
        assert_ok!(<PredictionCurrencies as MultiCurrency<AccountId>>::deposit(
            Asset::Ztg,
            &ALICE,
            100,
        ));
        assert_eq!(Balances::free_balance(ALICE), before + 100);
        assert_eq!(PredictionTokens::total_issuance(Asset::Ztg), 0);
    });
}

#[test]
fn unwhitelisted_and_trading_mode_deposits_are_rejected() {
    new_test_ext().execute_with(|| {
        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::Full,
        ));
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 100,),
            Error::<Test>::AssetNotWhitelisted
        );

        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::Trading,
        ));
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 100,),
            Error::<Test>::PredictionModeNotFull
        );
    });
}

#[test]
fn global_and_asset_pauses_block_deposit_but_not_withdraw() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            300,
        ));

        assert_ok!(PredictionCollateral::set_global_deposit_paused(
            RuntimeOrigin::root(),
            true,
        ));
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::GlobalDepositIsPaused
        );
        assert_ok!(PredictionCollateral::withdraw(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));

        assert_ok!(PredictionCollateral::set_global_deposit_paused(
            RuntimeOrigin::root(),
            false,
        ));
        assert_ok!(PredictionCollateral::set_asset_deposit_paused(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::AssetDepositIsPaused
        );
        assert_ok!(PredictionCollateral::withdraw(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_consistent(USDX_ASSET_ID);
    });
}

#[test]
fn whitelist_admission_and_deposit_reject_invalid_assets() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            PredictionCollateral::set_asset_whitelisted(
                RuntimeOrigin::root(),
                MISSING_ASSET_ID,
                true,
            ),
            Error::<Test>::AssetInvalid
        );
        assert_eq!(WhitelistedAssets::<Test>::get(MISSING_ASSET_ID), None);

        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::Full,
        ));
        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        set_asset_frozen(true);
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::AssetInvalid
        );
        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            false,
        ));
        assert_noop!(
            PredictionCollateral::set_asset_whitelisted(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                true,
            ),
            Error::<Test>::AssetInvalid
        );

        set_asset_frozen(false);
        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        set_protocol_ready(false);
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::AssetInvalid
        );
    });
}

#[test]
fn insufficient_user_asset_and_mirror_are_rejected() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_noop!(
            PredictionCollateral::deposit(
                RuntimeOrigin::signed(ALICE),
                USDX_ASSET_ID,
                INITIAL_ASSET_BALANCE + 1,
            ),
            Error::<Test>::InsufficientAssetBalance
        );
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_noop!(
            PredictionCollateral::withdraw(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 101,),
            Error::<Test>::InsufficientMirrorBalance
        );
        assert_consistent(USDX_ASSET_ID);
    });
}

#[test]
fn locked_mirror_cannot_be_withdrawn() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_ok!(<PredictionCurrencies as MultiReservableCurrency<
            AccountId,
        >>::reserve(mirror(USDX_ASSET_ID), &ALICE, 100,));
        assert_noop!(
            PredictionCollateral::withdraw(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::InsufficientMirrorBalance
        );
    });
}

#[test]
fn failed_escrow_release_rolls_back_mirror_burn() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_ok!(Assets::freeze_asset(
            RuntimeOrigin::signed(ADMIN),
            USDX_ASSET_ID,
        ));

        assert_noop!(
            PredictionCollateral::withdraw(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 40,),
            Error::<Test>::EscrowReleaseFailed
        );
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::free_balance(
                mirror(USDX_ASSET_ID),
                &ALICE,
            ),
            100
        );
        assert_consistent(USDX_ASSET_ID);
    });
}

#[test]
fn drained_escrow_is_rejected_by_pre_mutation_invariant() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_ok!(<Assets as Mutate<AccountId>>::transfer(
            USDX_ASSET_ID,
            &PredictionCollateral::sovereign_account(),
            &ADMIN,
            100,
            Preservation::Expendable,
        ));

        assert_noop!(
            PredictionCollateral::withdraw(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 1,),
            Error::<Test>::MirrorInconsistent
        );
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::free_balance(
                mirror(USDX_ASSET_ID),
                &ALICE,
            ),
            100
        );
    });
}

#[test]
fn preexisting_desync_rejects_deposit() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(<PredictionCurrencies as MultiCurrency<AccountId>>::deposit(
            mirror(USDX_ASSET_ID),
            &ALICE,
            1,
        ));
        assert_noop!(
            PredictionCollateral::deposit(RuntimeOrigin::signed(ALICE), USDX_ASSET_ID, 100,),
            Error::<Test>::MirrorInconsistent
        );
        assert_eq!(Assets::balance(USDX_ASSET_ID, ALICE), INITIAL_ASSET_BALANCE);
    });
}

#[test]
fn whitelist_removal_and_control_downgrade_do_not_block_withdrawal() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        assert_ok!(PredictionCollateral::deposit(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_ok!(PredictionCollateral::set_asset_whitelisted(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            false,
        ));
        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::Disabled,
        ));
        set_asset_frozen(true);
        set_protocol_ready(false);

        assert_ok!(PredictionCollateral::withdraw(
            RuntimeOrigin::signed(ALICE),
            USDX_ASSET_ID,
            100,
        ));
        assert_consistent(USDX_ASSET_ID);
    });
}

#[test]
fn multi_user_repeated_sequence_preserves_invariant_every_step() {
    new_test_ext().execute_with(|| {
        set_full_and_whitelist(USDX_ASSET_ID);
        for round in 1..=20_u128 {
            let alice_amount = round * 3;
            let bob_amount = round * 5;
            assert_ok!(PredictionCollateral::deposit(
                RuntimeOrigin::signed(ALICE),
                USDX_ASSET_ID,
                alice_amount,
            ));
            assert_consistent(USDX_ASSET_ID);
            assert_ok!(PredictionCollateral::deposit(
                RuntimeOrigin::signed(BOB),
                USDX_ASSET_ID,
                bob_amount,
            ));
            assert_consistent(USDX_ASSET_ID);
            assert_ok!(PredictionCollateral::withdraw(
                RuntimeOrigin::signed(ALICE),
                USDX_ASSET_ID,
                alice_amount,
            ));
            assert_consistent(USDX_ASSET_ID);
            assert_ok!(PredictionCollateral::withdraw(
                RuntimeOrigin::signed(BOB),
                USDX_ASSET_ID,
                bob_amount,
            ));
            assert_consistent(USDX_ASSET_ID);
        }
        assert_eq!(PredictionCollateral::mirror_issuance(USDX_ASSET_ID), 0);
    });
}

proptest! {
    #[test]
    fn arbitrary_multi_user_deposit_withdraw_sequence_preserves_escrow_equality(
        operations in prop::collection::vec((any::<bool>(), any::<bool>(), 1_u16..=250), 1..=128),
    ) {
        new_test_ext().execute_with(|| {
            set_full_and_whitelist(USDX_ASSET_ID);

            for (use_bob, is_deposit, raw_amount) in operations {
                let who = if use_bob { BOB } else { ALICE };
                let requested = u128::from(raw_amount);

                if is_deposit {
                    let available = Assets::balance(USDX_ASSET_ID, &who);
                    let amount = requested.min(available);
                    if amount > 0 {
                        assert_ok!(PredictionCollateral::deposit(
                            RuntimeOrigin::signed(who),
                            USDX_ASSET_ID,
                            amount,
                        ));
                    }
                } else {
                    let available = PredictionCurrencies::free_balance(mirror(USDX_ASSET_ID), &who);
                    let amount = requested.min(available);
                    if amount > 0 {
                        assert_ok!(PredictionCollateral::withdraw(
                            RuntimeOrigin::signed(who),
                            USDX_ASSET_ID,
                            amount,
                        ));
                    }
                }

                assert_consistent(USDX_ASSET_ID);
            }
        });
    }
}

#[test]
fn policy_tracks_all_live_gates_and_accounts_are_isolated() {
    new_test_ext().execute_with(|| {
        assert_ne!(PredictionCollateral::sovereign_account(), USDX_PSM_ACCOUNT);
        set_full_and_whitelist(USDX_ASSET_ID);
        assert!(PredictionCollateral::is_deposit_allowed(USDX_ASSET_ID));
        assert!(
            <PredictionCollateral as PredictionBaseAssetPolicy<u64>>::is_allowed(USDX_ASSET_ID)
        );

        assert_ok!(PredictionControl::set_module_enabled(
            RuntimeOrigin::root(),
            PredictionModule::PredictionMarkets,
            true,
        ));
        assert_ok!(PredictionCollateral::set_asset_deposit_paused(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            true,
        ));
        assert!(!PredictionCollateral::is_deposit_allowed(USDX_ASSET_ID));
    });
}
