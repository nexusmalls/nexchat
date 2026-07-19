// Copyright 2024-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.
//
// Zeitgeist is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at
// your option) any later version.

use crate as zrml_combinatorial_tokens;
use crate::{
    mock::types::MockPayout,
    types::{cryptographic_id_manager::Fuel, CryptographicIdManager},
    weights::WeightInfo,
};
use frame_support::{
    construct_runtime, derive_impl, parameter_types,
    traits::{fungibles::Inspect, AsEnsureOriginWithArg, ConstU32, Everything},
    Blake2_256, PalletId,
};
use frame_system::{mocking::MockBlockU32, EnsureRoot};
use pallet_prediction_collateral::AssetValidator;
use sp_runtime::traits::IdentityLookup;
use zeitgeist_primitives::{
    constants::mock::{
        BlockHashCount, CombinatorialTokensPalletId, ExistentialDeposit, ExistentialDeposits,
        GetNativeCurrencyId, MaxLocks, MaxReserves, MinimumPeriod,
    },
    types::{
        AccountIdTest, Amount, Balance, BasicCurrencyAdapter, CurrencyId, Hash, MarketId, Moment,
    },
};

#[cfg(feature = "runtime-benchmarks")]
use crate::mock::types::BenchmarkHelper;

construct_runtime! {
    pub enum Runtime {
        CombinatorialTokens: zrml_combinatorial_tokens,
        Assets: pallet_assets,
        Balances: pallet_balances,
        Currencies: orml_currencies,
        MarketCommons: zrml_market_commons,
        PredictionCollateral: pallet_prediction_collateral,
        PredictionControl: pallet_prediction_control,
        System: frame_system,
        Timestamp: pallet_timestamp,
        Tokens: orml_tokens,
    }
}

impl zrml_combinatorial_tokens::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = BenchmarkHelper;
    type CombinatorialIdManager = CryptographicIdManager<MarketId, Blake2_256>;
    type Fuel = Fuel;
    type MarketCommons = MarketCommons;
    type MultiCurrency = Currencies;
    type Payout = MockPayout;
    type PalletId = CombinatorialTokensPalletId;
    type WeightInfo = WeightInfo<Runtime>;
}

impl orml_currencies::Config for Runtime {
    type GetNativeCurrencyId = GetNativeCurrencyId;
    type MultiCurrency = Tokens;
    type NativeCurrency = BasicCurrencyAdapter<Runtime, Balances>;
    type WeightInfo = ();
}

parameter_types! {
    pub const AssetDeposit: Balance = 0;
    pub const AssetAccountDeposit: Balance = 0;
    pub const ApprovalDeposit: Balance = 0;
    pub const MetadataDepositBase: Balance = 0;
    pub const MetadataDepositPerByte: Balance = 0;
    pub const StringLimit: u32 = 50;
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type AssetId = u64;
    type AssetIdParameter = u64;
    type Currency = Balances;
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountIdTest>>;
    type ForceOrigin = EnsureRoot<AccountIdTest>;
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

impl pallet_prediction_control::Config for Runtime {
    type UpdateOrigin = EnsureRoot<AccountIdTest>;
    type WeightInfo = ();
}

pub struct LiveAssetValidator;

impl AssetValidator for LiveAssetValidator {
    fn is_valid(asset_id: u64) -> bool {
        <Assets as Inspect<AccountIdTest>>::asset_exists(asset_id)
            && pallet_assets::Asset::<Runtime>::get(asset_id)
                .is_some_and(|details| details.status == pallet_assets::AssetStatus::Live)
    }
}

parameter_types! {
    pub const CollateralPalletId: PalletId = PalletId(*b"ct/collt");
}

impl pallet_prediction_collateral::Config for Runtime {
    type Assets = Assets;
    type PredictionCurrencies = Currencies;
    type Control = PredictionControl;
    type AssetValidator = LiveAssetValidator;
    type WhitelistOrigin = EnsureRoot<AccountIdTest>;
    type PauseOrigin = EnsureRoot<AccountIdTest>;
    type CollateralPalletId = CollateralPalletId;
    type WeightInfo = ();
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ExistentialDeposit;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
}

impl zrml_market_commons::Config for Runtime {
    type Balance = Balance;
    type MarketId = MarketId;
    type Timestamp = Timestamp;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type AccountData = pallet_balances::AccountData<Balance>;
    type AccountId = AccountIdTest;
    type BaseCallFilter = Everything;
    type Block = MockBlockU32<Runtime>;
    type BlockHashCount = BlockHashCount;
    type Hash = Hash;
    type Lookup = IdentityLookup<Self::AccountId>;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {
    type MinimumPeriod = MinimumPeriod;
    type Moment = Moment;
}

impl orml_tokens::Config for Runtime {
    type Amount = Amount;
    type Balance = Balance;
    type CurrencyId = CurrencyId;
    type DustRemovalWhitelist = Everything;
    type ExistentialDeposits = ExistentialDeposits;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type CurrencyHooks = ();
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = ();
}
