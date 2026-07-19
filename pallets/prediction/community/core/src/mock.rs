// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Mock runtime for prediction community core tests.
//! Prediction community core 单测 mock runtime。

use crate as pallet_prediction_community_core;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU32, Everything},
	PalletId,
};
use orml_traits::parameter_type_with_key;
use sp_runtime::{traits::IdentityLookup, BuildStorage};
use zeitgeist_primitives::types::Asset;

pub type AccountId = u64;
pub type Balance = u128;
pub type MarketId = u32;
pub type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;
pub const DAVE: AccountId = 4;
pub const TREASURY: AccountId = 99;
pub const USDX: Asset<MarketId> = Asset::ForeignAsset(900_000);
pub const NEX: Balance = 1_000_000_000_000;

construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Tokens: orml_tokens,
		CommunityCore: pallet_prediction_community_core,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<0>;
	type DoneSlashHandler = ();
}

parameter_type_with_key! {
	pub ExistentialDeposits: |currency_id: Asset<MarketId>| -> Balance {
		match currency_id {
			Asset::Ztg => 1,
			_ => 0,
		}
	};
}

impl orml_tokens::Config for Test {
	type Balance = Balance;
	type Amount = i128;
	type CurrencyId = Asset<MarketId>;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type CurrencyHooks = ();
	type MaxLocks = ConstU32<50>;
	type MaxReserves = ConstU32<50>;
	type ReserveIdentifier = [u8; 8];
	type DustRemovalWhitelist = Everything;
}

parameter_types! {
	pub const CommunityAssetId: Asset<MarketId> = USDX;
	pub const CommunityBondAmount: Balance = 1_000_000 * NEX;
	pub const CommunityBondUnbondDelay: u64 = 10;
	pub const TreasuryAcc: AccountId = TREASURY;
	pub const CommunityPalletId: PalletId = PalletId(*b"pr/ccomm");
	pub const MaxTickets: u32 = 10_000;
	pub const MaxSettleBatch: u32 = 32;
	pub const MaxSingleLineLength: u32 = 200;
	pub const MaxSegmentCount: u32 = 100;
	pub const PoolRoundDuration: u64 = 1_000;
}

impl pallet_prediction_community_core::Config for Test {
	type Currency = Balances;
	type MultiCurrency = Tokens;
	type MarketId = MarketId;
	type CommunityAsset = CommunityAssetId;
	type CommunityBond = CommunityBondAmount;
	type CommunityBondUnbondDelay = CommunityBondUnbondDelay;
	type TreasuryAccount = TreasuryAcc;
	type PalletId = CommunityPalletId;
	type MaxTickets = MaxTickets;
	type MaxSettleBatch = MaxSettleBatch;
	type MaxSingleLineLength = MaxSingleLineLength;
	type MaxSegmentCount = MaxSegmentCount;
	type PoolRoundDuration = PoolRoundDuration;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("genesis");
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, 10_000_000 * NEX),
			(BOB, 10_000_000 * NEX),
			(CHARLIE, 10_000_000 * NEX),
			(DAVE, 10_000_000 * NEX),
			(TREASURY, 1),
		],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.expect("balances");

	orml_tokens::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, USDX, 1_000_000),
			(BOB, USDX, 1_000_000),
			(CHARLIE, USDX, 1_000_000),
			(DAVE, USDX, 1_000_000),
			(TREASURY, USDX, 0),
		],
	}
	.assimilate_storage(&mut storage)
	.expect("tokens");

	let mut ext = sp_io::TestExternalities::new(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn usdx_free(who: AccountId) -> Balance {
	use orml_traits::MultiCurrency;
	Tokens::free_balance(USDX, &who)
}
