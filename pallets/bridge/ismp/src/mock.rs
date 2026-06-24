// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Mock runtime for `pallet-bridge-ismp` unit tests.

use crate as pallet_bridge_ismp;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU128, ConstU64},
};
use frame_system::EnsureRoot;
use ismp::{
	dispatcher::{DispatchRequest, FeeMetadata, IsmpDispatcher},
	host::StateMachine,
	module::IsmpModule,
	router::IsmpRouter,
};
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, AccountId32, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = AccountId32;
pub type Balance = u128;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Timestamp: pallet_timestamp,
		Balances: pallet_balances,
		Ismp: pallet_ismp,
		Bridge: pallet_bridge_ismp,
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

/// Test dispatcher: records nothing and returns a deterministic commitment, so
/// `bridge_out` can be exercised without the full Hyperbridge/consensus stack.
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
		Ok(H256::repeat_byte(0xAB))
	}
}

/// Test router that resolves this bridge's module id to the bridge pallet.
#[derive(Default)]
pub struct MockRouter;

impl IsmpRouter for MockRouter {
	fn module_for_id(&self, id: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
		if id == pallet_bridge_ismp::module_id_bytes() {
			Ok(Box::new(Bridge::default()))
		} else {
			Err(anyhow::anyhow!("no module for id"))
		}
	}
}

parameter_types! {
	pub const NativeDecimals: u8 = 12;
	pub const MinBridgeAmount: Balance = 1_000_000; // 10^6, so 18→12 never truncates to 0
	pub const RequestTimeout: u64 = 3600;
}

impl pallet_bridge_ismp::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Dispatcher = MockDispatcher;
	type NativeCurrency = Balances;
	type EvmToSubstrate = ();
	type NativeDecimals = NativeDecimals;
	type MinBridgeAmount = MinBridgeAmount;
	type DailyLimitWindow = ConstU64<100>;
	type RequestTimeout = RequestTimeout;
	type BridgeOrigin = EnsureRoot<AccountId>;
	type WeightInfo = ();
}

/// Builds a test externality with `balances` pre-funded.
pub fn new_test_ext(balances: Vec<(AccountId, Balance)>) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// A canonical EVM destination chain for tests (BSC = chain id 56).
pub fn bsc() -> StateMachine {
	StateMachine::Evm(56)
}

/// A 32-byte account from a seed byte.
pub fn acc(seed: u8) -> AccountId {
	AccountId32::new([seed; 32])
}
