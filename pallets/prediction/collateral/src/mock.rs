// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Mock runtime for prediction-collateral tests.
//! Prediction-collateral 测试使用的 mock runtime。

use crate as pallet_prediction_collateral;
use crate::AssetValidator;
use frame_support::{
    assert_ok, construct_runtime, derive_impl, parameter_types,
    traits::{
        fungibles::{Inspect, Mutate},
        AsEnsureOriginWithArg, ConstU128, ConstU32, Nothing,
    },
    PalletId,
};
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::parameter_type_with_key;
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use std::cell::Cell;
use zeitgeist_primitives::types::Asset;

pub type AccountId = u64;
pub type Balance = u128;
pub type Amount = i128;
pub type MarketId = u128;
pub type AssetId = u64;
pub type Block = frame_system::mocking::MockBlockU32<Test>;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const ADMIN: AccountId = 69;
pub const USDX_ASSET_ID: AssetId = 900_000;
pub const MISSING_ASSET_ID: AssetId = 42;
pub const INITIAL_ASSET_BALANCE: Balance = 1_000_000;
pub const USDX_PSM_ACCOUNT: AccountId = 88;

construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets,
        PredictionTokens: orml_tokens,
        PredictionCurrencies: orml_currencies,
        PredictionControl: pallet_prediction_control,
        PredictionCollateral: pallet_prediction_collateral,
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
    type MaxReserves = ConstU32<16>;
    type ReserveIdentifier = [u8; 8];
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
    type AssetId = AssetId;
    type AssetIdParameter = AssetId;
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
    type RemoveItemsLimit = ConstU32<1_000>;
    type Holder = ();
    type ReserveData = ();
}

parameter_type_with_key! {
    pub ExistentialDeposits: |_asset: Asset<MarketId>| -> Balance { 0 };
}

impl orml_tokens::Config for Test {
    type Amount = Amount;
    type Balance = Balance;
    type CurrencyId = Asset<MarketId>;
    type WeightInfo = ();
    type ExistentialDeposits = ExistentialDeposits;
    type CurrencyHooks = ();
    type MaxLocks = ConstU32<16>;
    type MaxReserves = ConstU32<16>;
    type ReserveIdentifier = [u8; 8];
    type DustRemovalWhitelist = Nothing;
}

parameter_types! {
    pub GetNativeCurrencyId: Asset<MarketId> = Asset::Ztg;
}

type NativeCurrency = BasicCurrencyAdapter<Test, Balances, Amount, Balance>;

impl orml_currencies::Config for Test {
    type MultiCurrency = PredictionTokens;
    type NativeCurrency = NativeCurrency;
    type GetNativeCurrencyId = GetNativeCurrencyId;
    type WeightInfo = ();
}

impl pallet_prediction_control::Config for Test {
    type UpdateOrigin = frame_system::EnsureRoot<AccountId>;
    type WeightInfo = ();
}

thread_local! {
    static ASSET_FROZEN: Cell<bool> = const { Cell::new(false) };
    static PROTOCOL_READY: Cell<bool> = const { Cell::new(true) };
}

pub struct MockAssetValidator;

impl AssetValidator for MockAssetValidator {
    fn is_valid(asset_id: u64) -> bool {
        <Assets as Inspect<AccountId>>::asset_exists(asset_id)
            && !ASSET_FROZEN.with(Cell::get)
            && PROTOCOL_READY.with(Cell::get)
    }
}

parameter_types! {
    pub const CollateralPalletId: PalletId = PalletId(*b"py/collt");
}

impl pallet_prediction_collateral::Config for Test {
    type Assets = Assets;
    type PredictionCurrencies = PredictionCurrencies;
    type Control = PredictionControl;
    type AssetValidator = MockAssetValidator;
    type WhitelistOrigin = frame_system::EnsureRoot<AccountId>;
    type PauseOrigin = frame_system::EnsureRoot<AccountId>;
    type CollateralPalletId = CollateralPalletId;
    type WeightInfo = ();
}

pub fn set_asset_frozen(frozen: bool) {
    ASSET_FROZEN.with(|value| value.set(frozen));
}

pub fn set_protocol_ready(ready: bool) {
    PROTOCOL_READY.with(|value| value.set(ready));
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system genesis builds");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (ALICE, 1_000_000),
            (BOB, 1_000_000),
            (ADMIN, 1_000_000),
            (USDX_PSM_ACCOUNT, 1),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_asset_frozen(false);
        set_protocol_ready(true);
        assert_ok!(Assets::force_create(
            RuntimeOrigin::root(),
            USDX_ASSET_ID,
            ADMIN,
            true,
            1,
        ));
        assert_ok!(<Assets as Mutate<AccountId>>::mint_into(
            USDX_ASSET_ID,
            &ALICE,
            INITIAL_ASSET_BALANCE,
        ));
        assert_ok!(<Assets as Mutate<AccountId>>::mint_into(
            USDX_ASSET_ID,
            &BOB,
            INITIAL_ASSET_BALANCE,
        ));
    });
    ext
}
