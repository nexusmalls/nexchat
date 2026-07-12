#![cfg(test)]

extern crate alloc;

use crate::{self as zrml_authorized, mock_storage::pallet as mock_storage};
use alloc::{vec, vec::Vec};
use frame_support::{construct_runtime, derive_impl, ord_parameter_types, weights::Weight};
use frame_system::EnsureSignedBy;
use sp_runtime::{BuildStorage, DispatchError};
use zeitgeist_primitives::{
    constants::mock::{
        AuthorizedPalletId, BlockHashCount, CorrectionPeriod, ExistentialDeposit, MaxLocks,
        MaxReserves, MinimumPeriod, BASE,
    },
    traits::{DisputeResolutionApi, MarketOfDisputeResolutionApi},
    types::{Balance, BlockNumber, MarketId, Moment},
};

pub type AccountIdTest = u64;
pub type Block = frame_system::mocking::MockBlockU32<Runtime>;
pub const ALICE: AccountIdTest = 0;
pub const BOB: AccountIdTest = 1;
pub const CHARLIE: AccountIdTest = 2;

construct_runtime!(
    pub enum Runtime {
        System: frame_system,
        Balances: pallet_balances,
        Timestamp: pallet_timestamp,
        MarketCommons: zrml_market_commons,
        Authorized: zrml_authorized,
        MockStorage: mock_storage,
    }
);

ord_parameter_types! {
    pub const AuthorizedDisputeResolutionUser: AccountIdTest = ALICE;
}

pub struct MockResolution;
impl DisputeResolutionApi for MockResolution {
    type AccountId = AccountIdTest;
    type Balance = Balance;
    type BlockNumber = BlockNumber;
    type MarketId = MarketId;
    type Moment = Moment;

    fn resolve(
        _: &Self::MarketId,
        _: &MarketOfDisputeResolutionApi<Self>,
    ) -> Result<Weight, DispatchError> {
        Ok(Weight::zero())
    }
    fn add_auto_resolve(
        market_id: &Self::MarketId,
        resolve_at: Self::BlockNumber,
    ) -> Result<u32, DispatchError> {
        mock_storage::MarketIdsPerDisputeBlock::<Runtime>::try_mutate(resolve_at, |ids| {
            ids.try_push(*market_id)
                .map_err(|_| DispatchError::Other("Storage Overflow"))?;
            Ok(ids.len() as u32)
        })
    }
    fn auto_resolve_exists(market_id: &Self::MarketId, resolve_at: Self::BlockNumber) -> bool {
        mock_storage::MarketIdsPerDisputeBlock::<Runtime>::get(resolve_at).contains(market_id)
    }
    fn remove_auto_resolve(market_id: &Self::MarketId, resolve_at: Self::BlockNumber) -> u32 {
        mock_storage::MarketIdsPerDisputeBlock::<Runtime>::mutate(resolve_at, |ids| {
            ids.retain(|id| id != market_id);
            ids.len() as u32
        })
    }
}

impl crate::Config for Runtime {
    type Currency = Balances;
    type CorrectionPeriod = CorrectionPeriod;
    type DisputeResolution = MockResolution;
    type MarketCommons = MarketCommons;
    type PalletId = AuthorizedPalletId;
    type AuthorizedDisputeResolutionOrigin =
        EnsureSignedBy<AuthorizedDisputeResolutionUser, AccountIdTest>;
    type WeightInfo = crate::weights::WeightInfo<Runtime>;
}
impl mock_storage::Config for Runtime {
    type MarketCommons = MarketCommons;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type AccountId = AccountIdTest;
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

pub struct ExtBuilder {
    balances: Vec<(AccountIdTest, Balance)>,
}
impl Default for ExtBuilder {
    fn default() -> Self {
        Self {
            balances: vec![
                (ALICE, 1_000 * BASE),
                (BOB, 1_000 * BASE),
                (CHARLIE, 1_000 * BASE),
            ],
        }
    }
}
impl ExtBuilder {
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
        storage.into()
    }
}
