// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! Mock runtime for `pallet-bridge-ismp` unit tests.

use crate as pallet_bridge_ismp;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU128, ConstU32, ConstU64},
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

thread_local! {
    static ORDER_RESULT: core::cell::RefCell<Result<u64, sp_runtime::DispatchError>> =
        core::cell::RefCell::new(Ok(7));
}

/// Test control: set what the mock cross-order handler returns on its next call.
pub fn set_order_result(r: Result<u64, sp_runtime::DispatchError>) {
    ORDER_RESULT.with(|c| *c.borrow_mut() = r);
}

/// Mock cross-order handler: returns the configured result without touching state
/// (the real handler is `pallet-entity-order::do_cross_order`, tested there).
pub struct MockOrderHandler;
impl pallet_bridge_ismp::types::CrossChainOrderHandler<AccountId, Balance> for MockOrderHandler {
    fn do_cross_order(
        _buyer: AccountId,
        _payer: AccountId,
        _product_id: u64,
        _quantity: u32,
        _max_nex_amount: Balance,
        _referrer: Option<AccountId>,
    ) -> Result<u64, sp_runtime::DispatchError> {
        ORDER_RESULT.with(|c| c.borrow().clone())
    }

    fn cross_order_weight() -> frame_support::weights::Weight {
        frame_support::weights::Weight::zero()
    }
}

thread_local! {
    static REFUND_CALLS: core::cell::RefCell<Vec<Vec<u8>>> = const { core::cell::RefCell::new(Vec::new()) };
    static REFUND_FAIL: core::cell::RefCell<bool> = const { core::cell::RefCell::new(false) };
}

/// Test inspector: the metas handed to the mock payout-refund handler so far.
pub fn refund_calls() -> Vec<Vec<u8>> {
    REFUND_CALLS.with(|c| c.borrow().clone())
}

/// Test control: make the mock payout-refund handler fail (to assert `handled=false`).
pub fn set_refund_fail(b: bool) {
    REFUND_FAIL.with(|c| *c.borrow_mut() = b);
}

/// Mock payout-refund handler (HB-WD-01 mechanism 2): records each meta it receives
/// (recording is not rolled back by the nested storage layer) and optionally fails.
pub struct MockPayoutRefundHandler;
impl pallet_bridge_ismp::types::PayoutRefundHandler for MockPayoutRefundHandler {
    fn on_payout_timeout(meta: &[u8]) -> Result<(), sp_runtime::DispatchError> {
        REFUND_CALLS.with(|c| c.borrow_mut().push(meta.to_vec()));
        if REFUND_FAIL.with(|c| *c.borrow()) {
            return Err(sp_runtime::DispatchError::Other("refund handler failed"));
        }
        Ok(())
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
    type CrossOrderHandler = MockOrderHandler;
    type PayoutRefundHandler = MockPayoutRefundHandler;
    type MaxPayoutMeta = ConstU32<128>;
    type PayoutRefundTtl = ConstU64<100>;
    type WithdrawDelay = ConstU64<10>;
    type WeightInfo = ();
}

/// Builds a test externality with `balances` pre-funded.
pub fn new_test_ext(balances: Vec<(AccountId, Balance)>) -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances,
        ..Default::default()
    }
    .assimilate_storage(&mut t)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| {
        System::set_block_number(1);
        set_order_result(Ok(7));
        set_refund_fail(false);
        REFUND_CALLS.with(|c| c.borrow_mut().clear());
    });
    ext
}

/// A canonical EVM destination chain for tests (BSC = chain id 56).
pub fn bsc() -> StateMachine {
    StateMachine::Evm(56)
}

/// A second EVM destination chain for multi-lane tests (Polygon = chain id 137).
/// 第二条 EVM 目的链，用于多 lane 测试（Polygon = chain id 137）。
pub fn polygon() -> StateMachine {
    StateMachine::Evm(137)
}

/// A 32-byte account from a seed byte.
pub fn acc(seed: u8) -> AccountId {
    AccountId32::new([seed; 32])
}
