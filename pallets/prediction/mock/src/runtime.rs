//! Minimal shared runtime wiring for prediction pallet tests.
//! 预测 pallet 测试的最小共享 runtime 接线。

use crate::base_asset::USDX_ASSET_ID;
use frame_support::{
    assert_ok, construct_runtime, derive_impl, parameter_types,
    traits::{fungibles::Mutate, AsEnsureOriginWithArg, ConstU32, Nothing},
};
use frame_system::EnsureRoot;
use orml_traits::parameter_type_with_key;
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use zeitgeist_primitives::{
    constants::mock::{BlockHashCount, ExistentialDeposit, MaxLocks, MaxReserves, MinimumPeriod},
    types::{Amount, Asset, Balance, MarketId, Moment},
};

pub type Block = frame_system::mocking::MockBlock<Runtime>;
/// FRAME 45 mock account id (`TestDefaultConfig` uses `u64`).
/// FRAME 45 mock 账户 id（`TestDefaultConfig` 使用 `u64`）。
pub type AccountIdOf = u64;

pub const ALICE: AccountIdOf = 0;
pub const SUDO: AccountIdOf = 69;
pub const INITIAL_BALANCE: Balance = 1_000 * BASE;
pub const BASE: Balance = zeitgeist_primitives::constants::BASE;
pub const CENT: Balance = zeitgeist_primitives::constants::CENT;

construct_runtime!(
    pub enum Runtime {
        System: frame_system,
        Balances: pallet_balances,
        Timestamp: pallet_timestamp,
        PredictionTokens: orml_tokens,
        PredictionCurrencies: orml_currencies,
        Assets: pallet_assets,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type AccountId = AccountIdOf;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
    type BlockHashCount = BlockHashCount;
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

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {
    type Moment = Moment;
    type MinimumPeriod = MinimumPeriod;
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
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountIdOf>>;
    type ForceOrigin = EnsureRoot<AccountIdOf>;
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
    pub ExistentialDeposits: |currency_id: Asset<MarketId>| -> Balance {
        match currency_id {
            Asset::Ztg => ExistentialDeposit::get(),
            _ => 10,
        }
    };
}

impl orml_tokens::Config for Runtime {
    type Amount = Amount;
    type Balance = Balance;
    type CurrencyId = Asset<MarketId>;
    type WeightInfo = ();
    type ExistentialDeposits = ExistentialDeposits;
    type CurrencyHooks = ();
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type ReserveIdentifier = [u8; 8];
    type DustRemovalWhitelist = Nothing;
}

parameter_types! {
    pub GetNativeCurrencyId: Asset<MarketId> = Asset::Ztg;
}

type NativeCurrency = zeitgeist_primitives::types::BasicCurrencyAdapter<Runtime, Balances>;

impl orml_currencies::Config for Runtime {
    type MultiCurrency = PredictionTokens;
    type NativeCurrency = NativeCurrency;
    type GetNativeCurrencyId = GetNativeCurrencyId;
    type WeightInfo = ();
}

#[derive(Default)]
pub struct ExtBuilder {
    balances: Vec<(AccountIdOf, Balance)>,
    seed_usdx_for: Vec<(AccountIdOf, Balance)>,
}

impl ExtBuilder {
    pub fn balances(mut self, balances: Vec<(AccountIdOf, Balance)>) -> Self {
        self.balances = balances;
        self
    }

    pub fn seed_usdx(mut self, who: AccountIdOf, amount: Balance) -> Self {
        self.seed_usdx_for.push((who, amount));
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut storage = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .expect("frame-system genesis builds");

        let _ = env_logger::builder().is_test(true).try_init();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balances,
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
                SUDO,
                true,
                1,
            ));
            for (who, amount) in self.seed_usdx_for {
                assert_ok!(<Assets as Mutate<AccountIdOf>>::mint_into(
                    USDX_ASSET_ID,
                    &who,
                    amount,
                ));
            }
        });
        ext
    }
}
