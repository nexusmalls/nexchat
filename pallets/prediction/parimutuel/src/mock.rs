// Copyright 2023-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.

use crate as zrml_parimutuel;
use crate::{AssetOf, BalanceOf, MarketIdOf};
use alloc::{vec, vec::Vec};
use core::marker::PhantomData;
use frame_support::{
    assert_ok, construct_runtime, derive_impl,
    pallet_prelude::Get,
    parameter_types,
    traits::{
        fungibles::{Inspect, Mutate},
        AsEnsureOriginWithArg, ConstU32, Everything, ExistenceRequirement,
    },
    PalletId,
};
use frame_system::{mocking::MockBlockU32, EnsureRoot};
use orml_traits::MultiCurrency;
use pallet_prediction_collateral::AssetValidator;
use pallet_prediction_control::PredictionMode;
use prediction_mock_runtime::USDX_ASSET_ID;
use sp_runtime::{
    traits::{IdentityLookup, Zero},
    BuildStorage, Perbill, SaturatedConversion,
};
use zeitgeist_primitives::{
    constants::mock::{
        BlockHashCount, ExistentialDeposit, ExistentialDeposits, GetNativeCurrencyId, MaxLocks,
        MaxReserves, MinBetSize, MinimumPeriod, ParimutuelPalletId, BASE, CENT,
    },
    traits::DistributeFees,
    types::{
        AccountIdTest, Amount, Balance, BasicCurrencyAdapter, CurrencyId, Hash, MarketId, Moment,
    },
};

pub const ALICE: AccountIdTest = 0;
pub const BOB: AccountIdTest = 1;
pub const CHARLIE: AccountIdTest = 2;
pub const MARKET_CREATOR: AccountIdTest = 42;
pub const INITIAL_BALANCE: Balance = 1_000 * BASE;
pub const INITIAL_FOREIGN_BALANCE: Balance = 1_000 * BASE;
pub const USDX_MIN_BALANCE: Balance = 1;
pub const EXTERNAL_FEES: Balance = CENT;

parameter_types! {
    pub const FeeAccount: AccountIdTest = MARKET_CREATOR;
}

pub fn fee_percentage() -> Perbill {
    Perbill::from_rational(EXTERNAL_FEES, BASE)
}

pub fn calculate_fee<T: crate::Config>(amount: BalanceOf<T>) -> BalanceOf<T> {
    fee_percentage().mul_floor(amount.saturated_into::<BalanceOf<T>>())
}

pub struct ExternalFees<T, F>(PhantomData<T>, PhantomData<F>);

impl<T: crate::Config, F> DistributeFees for ExternalFees<T, F>
where
    F: Get<T::AccountId>,
{
    type Asset = AssetOf<T>;
    type AccountId = T::AccountId;
    type Balance = BalanceOf<T>;
    type MarketId = MarketIdOf<T>;

    fn distribute(
        _market_id: Self::MarketId,
        asset: Self::Asset,
        account: &Self::AccountId,
        amount: Self::Balance,
    ) -> Self::Balance {
        let fees = calculate_fee::<T>(amount);
        match T::AssetManager::transfer(
            asset,
            account,
            &F::get(),
            fees,
            ExistenceRequirement::AllowDeath,
        ) {
            Ok(_) => fees,
            Err(_) => Zero::zero(),
        }
    }

    fn fee_percentage(_market_id: Self::MarketId) -> Perbill {
        fee_percentage()
    }
}

construct_runtime!(
    pub enum Runtime {
        AssetManager: orml_currencies,
        Assets: pallet_assets,
        Balances: pallet_balances,
        MarketCommons: zrml_market_commons,
        Parimutuel: zrml_parimutuel,
        PredictionCollateral: pallet_prediction_collateral,
        PredictionControl: pallet_prediction_control,
        System: frame_system,
        Timestamp: pallet_timestamp,
        Tokens: orml_tokens,
    }
);

impl crate::Config for Runtime {
    type ExternalFees = ExternalFees<Runtime, FeeAccount>;
    type MarketCommons = MarketCommons;
    type AssetManager = AssetManager;
    type MinBetSize = MinBetSize;
    type PalletId = ParimutuelPalletId;
    type WeightInfo = crate::weights::WeightInfo<Runtime>;
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

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {
    type MinimumPeriod = MinimumPeriod;
    type Moment = Moment;
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
    pub const CollateralPalletId: PalletId = PalletId(*b"pa/collt");
}

impl pallet_prediction_collateral::Config for Runtime {
    type Assets = Assets;
    type PredictionCurrencies = AssetManager;
    type Control = PredictionControl;
    type AssetValidator = LiveAssetValidator;
    type WhitelistOrigin = EnsureRoot<AccountIdTest>;
    type PauseOrigin = EnsureRoot<AccountIdTest>;
    type CollateralPalletId = CollateralPalletId;
    type WeightInfo = ();
}

impl orml_tokens::Config for Runtime {
    type Amount = Amount;
    type Balance = Balance;
    type CurrencyId = CurrencyId;
    type DustRemovalWhitelist = Everything;
    type ExistentialDeposits = ExistentialDeposits;
    type MaxLocks = ();
    type MaxReserves = MaxReserves;
    type CurrencyHooks = ();
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = ();
}

pub struct ExtBuilder {
    balances: Vec<(AccountIdTest, Balance)>,
}

impl Default for ExtBuilder {
    fn default() -> Self {
        Self {
            balances: vec![
                (ALICE, INITIAL_BALANCE),
                (BOB, INITIAL_BALANCE),
                (CHARLIE, INITIAL_BALANCE),
            ],
        }
    }
}

impl ExtBuilder {
    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();
        let _ = env_logger::builder().is_test(true).try_init();
        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balances,
            dev_accounts: None,
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: sp_io::TestExternalities = storage.into();
        ext.execute_with(|| {
            System::set_block_number(1);
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                MARKET_CREATOR,
                true,
                USDX_MIN_BALANCE,
            ));
            for account in [ALICE, BOB, CHARLIE, MARKET_CREATOR] {
                assert_ok!(<Assets as Mutate<AccountIdTest>>::mint_into(
                    USDX_ASSET_ID,
                    &account,
                    INITIAL_FOREIGN_BALANCE + USDX_MIN_BALANCE,
                ));
            }
            assert_ok!(PredictionControl::set_prediction_mode(
                RuntimeOrigin::root(),
                PredictionMode::Full,
            ));
            assert_ok!(PredictionCollateral::set_asset_whitelisted(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                true,
            ));
            for account in [ALICE, BOB, CHARLIE, MARKET_CREATOR] {
                assert_ok!(PredictionCollateral::deposit(
                    RuntimeOrigin::signed(account),
                    USDX_ASSET_ID,
                    INITIAL_FOREIGN_BALANCE,
                ));
            }
            System::reset_events();
        });
        ext
    }
}
