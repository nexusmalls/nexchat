use crate::{
    mock::*,
    types::{ChainConfig, Message, SendParams, TokenRegistration, TokenUpdate},
    ContractToAsset, Error, NativeAssets, Precisions, TokenContracts,
};
use alloc::{collections::BTreeMap, vec};
use alloy_sol_types::SolValue;
use ismp::{
    host::StateMachine,
    module::IsmpModule,
    router::{PostRequest, Request},
};
use polkadot_sdk::frame_support::{
    assert_noop, assert_ok,
    traits::fungibles::{Inspect, Mutate},
    BoundedVec,
};
use polkadot_sdk::sp_core::H160;

fn chain_config(byte: u8, decimals: u8) -> ChainConfig {
    ChainConfig {
        token_contract: H160::repeat_byte(byte),
        decimals,
    }
}

fn register(asset_id: u64, chain: StateMachine, byte: u8) {
    let mut chains = BTreeMap::new();
    chains.insert(chain, chain_config(byte, 6));
    assert_ok!(Hft::register_token(
        RuntimeOrigin::root(),
        TokenRegistration {
            local_id: asset_id,
            native: false,
            chains: chains.try_into().expect("test registration is bounded")
        },
    ));
}

#[test]
fn dispatcher_failure_rolls_back_burn_and_supply() {
    new_test_ext().execute_with(|| {
        let destination = StateMachine::Evm(137);
        register(ASSET_ID, destination, 0x11);
        assert_ok!(<Assets as Mutate<AccountId>>::mint_into(
            ASSET_ID, &ALICE, 100
        ));
        let balance_before = Assets::balance(ASSET_ID, &ALICE);
        let supply_before = Assets::total_issuance(ASSET_ID);
        set_dispatch_fails(true);

        assert_noop!(
            Hft::send(
                RuntimeOrigin::signed(ALICE),
                SendParams {
                    asset_id: ASSET_ID,
                    destination,
                    recipient: BoundedVec::truncate_from(vec![0x22; 20]),
                    amount: 40,
                    timeout: 60,
                    relayer_fee: 0,
                    call_data: None,
                },
            ),
            Error::<Test>::DispatchError
        );
        assert_eq!(Assets::balance(ASSET_ID, &ALICE), balance_before);
        assert_eq!(Assets::total_issuance(ASSET_ID), supply_before);
    });
}

#[test]
fn authenticated_inbound_request_mints_imported_receipt() {
    new_test_ext().execute_with(|| {
        let source = StateMachine::Evm(80_002);
        register(ASSET_ID, source, 0x11);
        let beneficiary: [u8; 32] = ALICE.clone().into();
        let message = Message {
            from: vec![0x22; 20].into(),
            to: beneficiary.to_vec().into(),
            amount: alloy_primitives::U256::from(25u128),
            data: Vec::new().into(),
        };
        let request = PostRequest {
            source,
            dest: HostStateMachine::get(),
            nonce: 1,
            from: H160::repeat_byte(0x11).0.to_vec(),
            to: crate::PALLET_ID.to_bytes(),
            timeout_timestamp: u64::MAX,
            body: message.abi_encode(),
        };

        assert!(Hft::default().on_accept(request).is_ok());
        assert_eq!(Assets::balance(ASSET_ID, &ALICE), 25);
        assert_eq!(Assets::total_issuance(ASSET_ID), 25);
    });
}

#[test]
fn outbound_timeout_restores_imported_receipt() {
    new_test_ext().execute_with(|| {
        let destination = StateMachine::Evm(80_002);
        register(ASSET_ID, destination, 0x11);
        let sender: [u8; 32] = ALICE.clone().into();
        let message = Message {
            from: sender.to_vec().into(),
            to: vec![0x22; 20].into(),
            amount: alloy_primitives::U256::from(40u128),
            data: Vec::new().into(),
        };
        let request = PostRequest {
            source: HostStateMachine::get(),
            dest: destination,
            nonce: 2,
            from: crate::PALLET_ID.to_bytes(),
            to: H160::repeat_byte(0x11).0.to_vec(),
            timeout_timestamp: 60,
            body: message.abi_encode(),
        };

        assert!(Hft::default().on_timeout(Request::Post(request)).is_ok());
        assert_eq!(Assets::balance(ASSET_ID, &ALICE), 40);
        assert_eq!(Assets::total_issuance(ASSET_ID), 40);
    });
}

#[test]
fn invalid_registration_has_no_partial_registry_writes() {
    new_test_ext().execute_with(|| {
        let valid_chain = StateMachine::Evm(137);
        let mut chains = BTreeMap::new();
        chains.insert(valid_chain, chain_config(0x11, 6));
        chains.insert(StateMachine::Polkadot(2000), chain_config(0x22, 6));

        assert_noop!(
            Hft::register_token(
                RuntimeOrigin::root(),
                TokenRegistration {
                    local_id: ASSET_ID,
                    native: false,
                    chains: chains.try_into().expect("test registration is bounded")
                },
            ),
            Error::<Test>::NonEvmPeerChain
        );
        assert!(!NativeAssets::<Test>::contains_key(ASSET_ID));
        assert_eq!(TokenContracts::<Test>::get(valid_chain, ASSET_ID), None);
        assert_eq!(
            ContractToAsset::<Test>::get(valid_chain, H160::repeat_byte(0x11).0.to_vec()),
            None
        );
    });
}

#[test]
fn contract_cannot_map_to_two_assets() {
    new_test_ext().execute_with(|| {
        let chain = StateMachine::Evm(137);
        register(ASSET_ID, chain, 0x11);
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            2,
            ALICE,
            true,
            1
        ));
        assert_ok!(Assets::force_set_metadata(
            RuntimeOrigin::root(),
            2,
            b"xUSDC2".to_vec(),
            b"xUSDC2".to_vec(),
            6,
            false,
        ));
        let mut chains = BTreeMap::new();
        chains.insert(chain, chain_config(0x11, 6));

        assert_noop!(
            Hft::register_token(
                RuntimeOrigin::root(),
                TokenRegistration {
                    local_id: 2,
                    native: false,
                    chains: chains.try_into().expect("test registration is bounded")
                },
            ),
            Error::<Test>::ContractAlreadyInUse
        );
        assert!(!NativeAssets::<Test>::contains_key(2));
        assert_eq!(
            ContractToAsset::<Test>::get(chain, H160::repeat_byte(0x11).0.to_vec()),
            Some(ASSET_ID)
        );
    });
}

#[test]
fn invalid_update_has_no_partial_registry_writes() {
    new_test_ext().execute_with(|| {
        register(ASSET_ID, StateMachine::Evm(10), 0x10);
        let valid_new_chain = StateMachine::Evm(1);
        let invalid_new_chain = StateMachine::Evm(137);
        let mut add_chains = BTreeMap::new();
        add_chains.insert(valid_new_chain, chain_config(0x21, 6));
        add_chains.insert(invalid_new_chain, chain_config(0x22, 5));

        assert_noop!(
            Hft::update_token(
                RuntimeOrigin::root(),
                TokenUpdate {
                    asset_id: ASSET_ID,
                    add_chains: add_chains.try_into().expect("test update is bounded"),
                    remove_chains: Vec::new().try_into().expect("empty update is bounded")
                },
            ),
            Error::<Test>::ErcDecimalsBelowLocal
        );
        assert_eq!(TokenContracts::<Test>::get(valid_new_chain, ASSET_ID), None);
        assert_eq!(Precisions::<Test>::get(ASSET_ID, valid_new_chain), None);
    });
}

#[test]
fn update_rejects_inconsistent_reverse_registry() {
    new_test_ext().execute_with(|| {
        let chain = StateMachine::Evm(137);
        register(ASSET_ID, chain, 0x11);
        assert_ok!(Hft::check_registry_invariants());
        ContractToAsset::<Test>::remove(chain, H160::repeat_byte(0x11).0.to_vec());
        assert_eq!(
            Hft::check_registry_invariants(),
            Err("HFT forward registry has no matching reverse entry")
        );
        let mut add_chains = BTreeMap::new();
        add_chains.insert(chain, chain_config(0x12, 6));

        assert_noop!(
            Hft::update_token(
                RuntimeOrigin::root(),
                TokenUpdate {
                    asset_id: ASSET_ID,
                    add_chains: add_chains.try_into().expect("test update is bounded"),
                    remove_chains: Vec::new().try_into().expect("empty update is bounded")
                },
            ),
            Error::<Test>::RegistryInconsistent
        );
        assert_eq!(
            TokenContracts::<Test>::get(chain, ASSET_ID),
            Some(H160::repeat_byte(0x11).0.to_vec())
        );
    });
}
