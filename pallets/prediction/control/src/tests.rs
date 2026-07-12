use crate::{
    is_call_allowed,
    mock::{new_test_ext, PredictionControl, RuntimeEvent, RuntimeOrigin, System},
    mode_allows, CallClass, CallRegistryEntry, Event, PredictionMode, PredictionModule,
    CALL_REGISTRY,
};
use frame_support::{assert_noop, assert_ok};
use sp_runtime::DispatchError;
use std::collections::BTreeSet;

const MODULES: [PredictionModule; 12] = [
    PredictionModule::PredictionMarkets,
    PredictionModule::Authorized,
    PredictionModule::Court,
    PredictionModule::GlobalDisputes,
    PredictionModule::LegacySwaps,
    PredictionModule::NeoSwaps,
    PredictionModule::Orderbook,
    PredictionModule::Parimutuel,
    PredictionModule::HybridRouter,
    PredictionModule::CombinatorialTokens,
    PredictionModule::Futarchy,
    PredictionModule::Styx,
];

#[test]
fn defaults_are_disabled_and_all_modules_are_off() {
    new_test_ext().execute_with(|| {
        assert_eq!(
            PredictionControl::prediction_mode(),
            PredictionMode::Disabled
        );
        for module in MODULES {
            assert!(!PredictionControl::module_enabled(module));
        }
    });
}

#[test]
fn update_origin_is_enforced() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            PredictionControl::set_prediction_mode(RuntimeOrigin::signed(1), PredictionMode::Full),
            DispatchError::BadOrigin
        );
        assert_noop!(
            PredictionControl::set_module_enabled(
                RuntimeOrigin::signed(1),
                PredictionModule::Court,
                true
            ),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn mode_transitions_store_and_emit_old_and_new_values() {
    new_test_ext().execute_with(|| {
        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::ResolutionOnly
        ));
        assert_eq!(
            PredictionControl::prediction_mode(),
            PredictionMode::ResolutionOnly
        );
        System::assert_last_event(RuntimeEvent::PredictionControl(Event::PredictionModeSet {
            old: PredictionMode::Disabled,
            new: PredictionMode::ResolutionOnly,
        }));

        assert_ok!(PredictionControl::set_prediction_mode(
            RuntimeOrigin::root(),
            PredictionMode::Trading
        ));
        System::assert_last_event(RuntimeEvent::PredictionControl(Event::PredictionModeSet {
            old: PredictionMode::ResolutionOnly,
            new: PredictionMode::Trading,
        }));
    });
}

#[test]
fn module_switch_stores_and_emits_state() {
    new_test_ext().execute_with(|| {
        let module = PredictionModule::NeoSwaps;
        assert_ok!(PredictionControl::set_module_enabled(
            RuntimeOrigin::root(),
            module,
            true
        ));
        assert!(PredictionControl::module_enabled(module));
        System::assert_last_event(RuntimeEvent::PredictionControl(Event::ModuleEnabledSet {
            module,
            enabled: true,
        }));

        assert_ok!(PredictionControl::set_module_enabled(
            RuntimeOrigin::root(),
            module,
            false
        ));
        assert!(!PredictionControl::module_enabled(module));
    });
}

#[test]
fn mode_class_matrix_is_exact() {
    use CallClass::*;
    use PredictionMode::*;

    let cases = [
        (Disabled, RiskIncreasing, false),
        (Disabled, Resolution, false),
        (Disabled, Unwind, true),
        (Disabled, AdminRecovery, true),
        (ResolutionOnly, RiskIncreasing, false),
        (ResolutionOnly, Resolution, true),
        (ResolutionOnly, Unwind, true),
        (ResolutionOnly, AdminRecovery, true),
        (Trading, RiskIncreasing, true),
        (Trading, Resolution, true),
        (Trading, Unwind, true),
        (Trading, AdminRecovery, true),
        (Full, RiskIncreasing, true),
        (Full, Resolution, true),
        (Full, Unwind, true),
        (Full, AdminRecovery, true),
    ];

    for (mode, class, expected) in cases {
        assert_eq!(mode_allows(mode, class), expected, "{mode:?}/{class:?}");
    }
}

#[test]
fn module_gate_is_always_required_for_business_calls() {
    for mode in [
        PredictionMode::Disabled,
        PredictionMode::ResolutionOnly,
        PredictionMode::Trading,
        PredictionMode::Full,
    ] {
        for class in [
            CallClass::RiskIncreasing,
            CallClass::Resolution,
            CallClass::Unwind,
            CallClass::AdminRecovery,
        ] {
            assert!(!is_call_allowed(mode, false, class));
            assert_eq!(is_call_allowed(mode, true, class), mode_allows(mode, class));
        }
    }
}

#[test]
fn registry_contains_all_68_current_dispatchables_with_unique_keys() {
    assert_eq!(CALL_REGISTRY.len(), 68);
    let mut keys = BTreeSet::new();
    for CallRegistryEntry {
        module,
        call_index,
        name,
        ..
    } in CALL_REGISTRY
    {
        assert!(!name.is_empty());
        assert!(
            keys.insert((*module, *call_index)),
            "duplicate registry key: {module:?}/{call_index}"
        );
    }
    assert_eq!(keys.len(), 68);
}

#[test]
fn every_registered_dispatchable_obeys_the_full_filter_matrix() {
    let modes = [
        PredictionMode::Disabled,
        PredictionMode::ResolutionOnly,
        PredictionMode::Trading,
        PredictionMode::Full,
    ];

    for entry in CALL_REGISTRY {
        for mode in modes {
            assert_eq!(
                is_call_allowed(mode, true, entry.class),
                mode_allows(mode, entry.class),
                "enabled {mode:?}/{:?}/{}",
                entry.module,
                entry.name,
            );
            assert!(
                !is_call_allowed(mode, false, entry.class),
                "disabled module admitted {mode:?}/{:?}/{}",
                entry.module,
                entry.name,
            );
        }
    }
}

#[test]
fn mixed_leg_and_governance_classifications_are_pinned() {
    let class_of = |module, name| {
        CALL_REGISTRY
            .iter()
            .find(|entry| entry.module == module && entry.name == name)
            .map(|entry| entry.class)
            .expect("registered call")
    };

    assert_eq!(
        class_of(PredictionModule::NeoSwaps, "combo_sell"),
        CallClass::RiskIncreasing
    );
    assert_eq!(
        class_of(PredictionModule::PredictionMarkets, "approve_market"),
        CallClass::RiskIncreasing
    );
    assert_eq!(
        class_of(PredictionModule::PredictionMarkets, "request_edit"),
        CallClass::AdminRecovery
    );
    assert_eq!(
        class_of(PredictionModule::PredictionMarkets, "schedule_early_close"),
        CallClass::RiskIncreasing
    );
    assert_eq!(
        class_of(PredictionModule::Court, "reassign_court_stakes"),
        CallClass::Unwind
    );
    assert_eq!(
        class_of(PredictionModule::GlobalDisputes, "purge_outcomes"),
        CallClass::Unwind
    );
    assert_eq!(
        class_of(PredictionModule::GlobalDisputes, "reward_outcome_owner"),
        CallClass::Resolution
    );
    assert_eq!(
        class_of(PredictionModule::NeoSwaps, "sell"),
        CallClass::Unwind
    );
    assert_eq!(
        class_of(PredictionModule::HybridRouter, "sell"),
        CallClass::Unwind
    );
}
