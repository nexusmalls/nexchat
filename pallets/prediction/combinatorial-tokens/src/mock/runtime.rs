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
use frame_support::{construct_runtime, derive_impl, traits::Everything, Blake2_256};
use frame_system::mocking::MockBlockU32;
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
        Balances: pallet_balances,
        Currencies: orml_currencies,
        MarketCommons: zrml_market_commons,
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
