use crate as pallet_hyper_fungible_token;
use frame_support::{
    derive_impl, parameter_types,
    traits::{AsEnsureOriginWithArg, ConstU128, ConstU32},
};
use frame_system::EnsureRoot;
use ismp::{
    dispatcher::{DispatchRequest, FeeMetadata, IsmpDispatcher},
    host::StateMachine,
    module::IsmpModule,
    router::IsmpRouter,
};
use polkadot_sdk::*;
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

use alloc::{boxed::Box, vec::Vec};
use core::cell::Cell;

pub type AccountId = AccountId32;
pub type Balance = u128;
pub type AssetId = u64;
pub type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: AccountId = AccountId::new([1; 32]);
pub const ASSET_ID: AssetId = 1;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Timestamp: pallet_timestamp,
        Balances: pallet_balances,
        Assets: pallet_assets,
        Ismp: pallet_ismp,
        Hft: pallet_hyper_fungible_token,
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

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

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
    type ForceOrigin = EnsureRoot<AccountId>;
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
}

parameter_types! {
    pub const HostStateMachine: StateMachine = StateMachine::Substrate(*b"NEXS");
    pub const Coprocessor: Option<StateMachine> = Some(StateMachine::Polkadot(0));
}

impl pallet_ismp::Config for Test {
    type AdminOrigin = EnsureRoot<AccountId>;
    type TimestampProvider = Timestamp;
    type Balance = Balance;
    type Currency = Balances;
    type HostStateMachine = HostStateMachine;
    type Coprocessor = Coprocessor;
    type Router = MockRouter;
    type ConsensusClients = ();
    type FeeHandler = ();
    type OffchainDB = ();
    type MigrationWeightInfo = ();
}

thread_local! {
    static DISPATCH_FAILS: Cell<bool> = const { Cell::new(false) };
}

pub fn set_dispatch_fails(fails: bool) {
    DISPATCH_FAILS.with(|value| value.set(fails));
}

#[derive(Default)]
pub struct MockDispatcher;

impl IsmpDispatcher for MockDispatcher {
    type Account = AccountId;
    type Balance = Balance;

    fn dispatch_request(
        &self,
        _request: DispatchRequest,
        _fee: FeeMetadata<Self::Account, Self::Balance>,
    ) -> Result<H256, anyhow::Error> {
        if DISPATCH_FAILS.with(Cell::get) {
            Err(anyhow::anyhow!("injected dispatcher failure"))
        } else {
            Ok(H256::repeat_byte(0xAB))
        }
    }
}

#[derive(Default)]
pub struct MockRouter;

impl IsmpRouter for MockRouter {
    fn module_for_id(&self, _id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        Err(anyhow::anyhow!("unused test router"))
    }
}

parameter_types! {
    pub const NativeAssetId: AssetId = u64::MAX;
    pub const NativeDecimals: u8 = 12;
}

impl crate::Config for Test {
    type Dispatcher = MockDispatcher;
    type NativeCurrency = Balances;
    type CreateOrigin = EnsureRoot<AccountId>;
    type Assets = Assets;
    type NativeAssetId = NativeAssetId;
    type Decimals = NativeDecimals;
    type EvmToSubstrate = ();
    type WeightInfo = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = HftBenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct HftBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<Test> for HftBenchmarkHelper {
    fn create_asset(asset_id: AssetId, owner: AccountId) {
        let _ = Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1);
        let _ = Assets::force_set_metadata(
            RuntimeOrigin::root(),
            asset_id,
            b"xUSDC".to_vec(),
            b"xUSDC".to_vec(),
            6,
            false,
        );
    }
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .expect("frame-system genesis builds");
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000_000)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut storage)
    .expect("balances genesis assimilates");

    let mut ext = sp_io::TestExternalities::new(storage);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_dispatch_fails(false);
        assert!(Assets::force_create(RuntimeOrigin::root(), ASSET_ID, ALICE, true, 1).is_ok());
        assert!(Assets::force_set_metadata(
            RuntimeOrigin::root(),
            ASSET_ID,
            b"xUSDC".to_vec(),
            b"xUSDC".to_vec(),
            6,
            false,
        )
        .is_ok());
    });
    ext
}
