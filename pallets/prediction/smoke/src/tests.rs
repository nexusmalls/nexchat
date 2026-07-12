use super::PredictionAsset;
use frame_support::{
    assert_ok, derive_impl, parameter_types,
    storage::{with_transaction, TransactionOutcome},
    traits::{
        fungibles::Mutate, tokens::Preservation, AsEnsureOriginWithArg, ConstU128, ConstU32,
        Everything,
    },
};
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::{parameter_type_with_key, MultiCurrency};
use sp_runtime::{
    traits::{IdentityLookup, Zero},
    BuildStorage, DispatchError, DispatchResult,
};
use zeitgeist_primitives::types::Asset;

type AccountId = u64;
type Balance = u128;
type Amount = i128;
type MarketId = u128;
type AssetId = u64;
type Block = frame_system::mocking::MockBlock<Test>;

const ADMIN: AccountId = 1;
const ALICE: AccountId = 2;
const ESCROW: AccountId = 99;
const USDX_ASSET_ID: AssetId = 900_000;
const INITIAL_USDX: Balance = 1_000_000;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Assets: pallet_assets,
        PredictionTokens: orml_tokens,
        PredictionCurrencies: orml_currencies,
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
    pub ExistentialDeposits: |_asset: PredictionAsset| -> Balance {
        Balance::zero()
    };
}

impl orml_tokens::Config for Test {
    type Amount = Amount;
    type Balance = Balance;
    type CurrencyId = PredictionAsset;
    type WeightInfo = ();
    type ExistentialDeposits = ExistentialDeposits;
    type CurrencyHooks = ();
    type MaxLocks = ConstU32<16>;
    type MaxReserves = ConstU32<16>;
    type ReserveIdentifier = [u8; 8];
    type DustRemovalWhitelist = Everything;
}

parameter_types! {
    pub GetNativeCurrencyId: PredictionAsset = Asset::Ztg;
}

type NativeCurrency = BasicCurrencyAdapter<Test, Balances, Amount, Balance>;

impl orml_currencies::Config for Test {
    type MultiCurrency = PredictionTokens;
    type NativeCurrency = NativeCurrency;
    type GetNativeCurrencyId = GetNativeCurrencyId;
    type WeightInfo = ();
}

fn foreign_usdx() -> Asset<MarketId> {
    Asset::ForeignAsset(USDX_ASSET_ID)
}

fn deposit_foreign(who: &AccountId, amount: Balance) -> DispatchResult {
    with_transaction(|| {
        if let Err(error) = <Assets as Mutate<AccountId>>::transfer(
            USDX_ASSET_ID,
            who,
            &ESCROW,
            amount,
            Preservation::Preserve,
        ) {
            return TransactionOutcome::Rollback(Err(error));
        }
        match <PredictionCurrencies as MultiCurrency<AccountId>>::deposit(
            foreign_usdx(),
            who,
            amount,
        ) {
            Ok(()) => TransactionOutcome::Commit(Ok(())),
            Err(error) => TransactionOutcome::Rollback(Err(error)),
        }
    })
}

fn withdraw_foreign(who: &AccountId, amount: Balance) -> DispatchResult {
    with_transaction(|| {
        if let Err(error) = <PredictionCurrencies as MultiCurrency<AccountId>>::withdraw(
            foreign_usdx(),
            who,
            amount,
            frame_support::traits::ExistenceRequirement::AllowDeath,
        ) {
            return TransactionOutcome::Rollback(Err(error));
        }
        match <Assets as Mutate<AccountId>>::transfer(
            USDX_ASSET_ID,
            &ESCROW,
            who,
            amount,
            Preservation::Expendable,
        ) {
            Ok(_) => TransactionOutcome::Commit(Ok(())),
            Err(error) => TransactionOutcome::Rollback(Err(error)),
        }
    })
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system genesis builds");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ADMIN, 1_000_000), (ALICE, 1_000_000), (ESCROW, 1)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
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
            INITIAL_USDX,
        ));
    });
    ext
}

#[test]
fn native_outcome_and_foreign_ledgers_are_isolated_and_conserved() {
    new_test_ext().execute_with(|| {
        assert_ok!(<PredictionCurrencies as MultiCurrency<AccountId>>::deposit(
            Asset::Ztg,
            &ALICE,
            100,
        ));
        assert_eq!(Balances::free_balance(ALICE), 1_000_100);
        assert_eq!(PredictionTokens::total_issuance(Asset::Ztg), 0);

        let outcome = Asset::CategoricalOutcome(7, 0);
        assert_ok!(<PredictionCurrencies as MultiCurrency<AccountId>>::deposit(
            outcome, &ALICE, 500,
        ));
        assert_eq!(PredictionTokens::free_balance(outcome, &ALICE), 500);

        assert_ok!(deposit_foreign(&ALICE, 10_000));
        assert_eq!(Assets::balance(USDX_ASSET_ID, ESCROW), 10_000);
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::total_issuance(foreign_usdx()),
            10_000
        );

        assert_ok!(withdraw_foreign(&ALICE, 4_000));
        assert_eq!(Assets::balance(USDX_ASSET_ID, ESCROW), 6_000);
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::total_issuance(foreign_usdx()),
            6_000
        );
        assert_eq!(Assets::balance(USDX_ASSET_ID, ALICE), INITIAL_USDX - 6_000);
    });
}

#[test]
fn failed_escrow_release_rolls_back_mirror_burn() {
    new_test_ext().execute_with(|| {
        assert_ok!(deposit_foreign(&ALICE, 10_000));
        assert_ok!(<Assets as Mutate<AccountId>>::transfer(
            USDX_ASSET_ID,
            &ESCROW,
            &ADMIN,
            10_000,
            Preservation::Expendable,
        ));

        let result: Result<(), DispatchError> = withdraw_foreign(&ALICE, 1);
        assert!(result.is_err());
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::free_balance(
                foreign_usdx(),
                &ALICE
            ),
            10_000
        );
        assert_eq!(
            <PredictionCurrencies as MultiCurrency<AccountId>>::total_issuance(foreign_usdx()),
            10_000
        );
    });
}
