// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Mock runtime for `pallet-usdx`.
//! `pallet-usdx` 单元测试 mock runtime。

use crate as pallet_usdx;
use frame_support::{
    assert_ok, derive_impl, parameter_types,
    traits::{
        fungibles::{Inspect, Mutate},
        AsEnsureOriginWithArg, ConstU128, ConstU32,
    },
    PalletId,
};
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use std::cell::Cell;

pub type AccountId = u64;
pub type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

pub const ADMIN: AccountId = 1;
pub const ALICE: AccountId = 2;
pub const BOB: AccountId = 3;
pub const USDX_ASSET_ID: u64 = 900_000;
pub const POLYGON_RECEIPT_ID: u64 = 900_001;
pub const ETHEREUM_RECEIPT_ID: u64 = 900_002;

std::thread_local! {
    static PROTOCOL_ASSET_CONFIG_VALID: Cell<bool> = const { Cell::new(true) };
}

pub fn set_protocol_asset_config_valid(valid: bool) {
    PROTOCOL_ASSET_CONFIG_VALID.with(|value| value.set(valid));
}

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets,
        Usdx: pallet_usdx,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ConstU128<1>;
}

parameter_types! {
    pub const AssetDeposit: Balance = 0;
    pub const AssetAccountDeposit: Balance = 0;
    pub const ApprovalDeposit: Balance = 0;
    pub const MetadataDepositBase: Balance = 0;
    pub const MetadataDepositPerByte: Balance = 0;
    pub const StringLimit: u32 = 50;
}

impl pallet_assets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type AssetId = u64;
    type AssetIdParameter = u64;
    type Currency = Balances;
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = StringLimit;
    type Freezer = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = ();
    type RemoveItemsLimit = ConstU32<1000>;
    type Holder = ();
    type ReserveData = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

pub struct MockReceiptValidator;
impl crate::ReceiptValidator for MockReceiptValidator {
    fn descriptor_hash(asset_id: u64) -> Option<H256> {
        match asset_id {
            POLYGON_RECEIPT_ID => Some(H256::repeat_byte(0xA1)),
            ETHEREUM_RECEIPT_ID => Some(H256::repeat_byte(0xA2)),
            _ => None,
        }
    }

    fn validate_evidence(
        asset_id: u64,
        descriptor_hash: H256,
        evidence: &crate::LaneActivationEvidence,
    ) -> bool {
        Self::descriptor_hash(asset_id) == Some(descriptor_hash)
            && !evidence.is_weth
            && evidence.wrapper_contract != [0; 20]
            && evidence.underlying_contract != [0; 20]
            && evidence.owner_contract != [0; 20]
            && evidence.host_contract != [0; 20]
            && evidence.dispatcher_contract != [0; 20]
            && evidence.config_block > 0
            && evidence.proof_bundle_hash != H256::zero()
    }
}

pub struct MockProtocolAssetInspector;
impl crate::ProtocolAssetInspector<AccountId> for MockProtocolAssetInspector {
    fn validate_usdx(asset_id: u64, _psm_account: &AccountId) -> bool {
        PROTOCOL_ASSET_CONFIG_VALID.with(Cell::get)
            && asset_id == USDX_ASSET_ID
            && Assets::asset_exists(asset_id)
    }

    fn validate_receipt(asset_id: u64) -> bool {
        PROTOCOL_ASSET_CONFIG_VALID.with(Cell::get)
            && matches!(asset_id, POLYGON_RECEIPT_ID | ETHEREUM_RECEIPT_ID)
            && Assets::asset_exists(asset_id)
    }
}

parameter_types! {
    pub const UsdxAssetId: u64 = USDX_ASSET_ID;
    pub const PsmPalletId: PalletId = PalletId(*b"nex/usdx");
}

impl pallet_usdx::Config for Test {
    type Assets = Assets;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type PauseOrigin = frame_system::EnsureRoot<AccountId>;
    type ReceiptValidator = MockReceiptValidator;
    type ProtocolAssetInspector = MockProtocolAssetInspector;
    type UsdxAssetId = UsdxAssetId;
    type PsmPalletId = PsmPalletId;
    type WeightInfo = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = MockBenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct MockBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<Test> for MockBenchmarkHelper {
    fn prepare() {}

    fn evidence(_receipt_asset_id: u64) -> crate::LaneActivationEvidence {
        crate::LaneActivationEvidence {
            wrapper_contract: [0x11; 20],
            underlying_contract: [0x22; 20],
            owner_contract: [0x33; 20],
            host_contract: [0x44; 20],
            dispatcher_contract: [0x55; 20],
            is_weth: false,
            hft_bytecode_hash: H256::repeat_byte(0x66),
            controller_bytecode_hash: H256::repeat_byte(0x77),
            config_block: 1,
            config_block_hash: H256::repeat_byte(0x88),
            nexus_peer_hash: H256::repeat_byte(0x99),
            proof_bundle_hash: H256::repeat_byte(0xAA),
        }
    }
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system genesis builds");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ADMIN, 1_000_000), (ALICE, 1_000_000), (BOB, 1_000_000)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        set_protocol_asset_config_valid(true);
        System::set_block_number(1);
        for asset_id in [USDX_ASSET_ID, POLYGON_RECEIPT_ID, ETHEREUM_RECEIPT_ID] {
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                asset_id,
                ADMIN,
                true,
                1,
            ));
        }
        assert_ok!(<Assets as Mutate<AccountId>>::mint_into(
            POLYGON_RECEIPT_ID,
            &ALICE,
            1_000_000,
        ));
    });
    ext
}
