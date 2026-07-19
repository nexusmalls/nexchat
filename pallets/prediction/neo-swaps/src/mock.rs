// Copyright 2022-2025 Forecasting Technologies LTD.
// Copyright 2021-2022 Zeitgeist PM LLC.
//
// This file is part of Zeitgeist.
//
// Zeitgeist is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at
// your option) any later version.
//
// Zeitgeist is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Zeitgeist. If not, see <https://www.gnu.org/licenses/>.

#![cfg(feature = "mock")]
#![allow(
    // Mocks are only used for fuzzing and unit tests
    clippy::arithmetic_side_effects,
    clippy::too_many_arguments,
)]

use crate as zrml_neo_swaps;
use crate::{consts::*, AssetOf, MarketIdOf};
use core::marker::PhantomData;
use frame_support::{
    assert_ok, construct_runtime, derive_impl, ord_parameter_types, parameter_types,
    traits::{
        fungibles::{Inspect, Mutate},
        tokens::{PayFromAccount, UnityAssetBalanceConversion},
        AsEnsureOriginWithArg, ConstU32, Contains, EqualPrivilegeOnly, Everything,
        ExistenceRequirement, NeverEnsureOrigin, Randomness,
    },
    weights::Weight,
    Blake2_256, PalletId,
};
use frame_system::{mocking::MockBlockU32, EnsureRoot, EnsureSignedBy};
use orml_traits::MultiCurrency;
use pallet_prediction_collateral::AssetValidator;
use pallet_prediction_control::PredictionMode;
use prediction_mock_runtime::USDX_ASSET_ID;
use sp_runtime::{
    traits::{Get, Hash as HashT, IdentityLookup, Zero},
    BuildStorage, DispatchResult, Perbill, Percent, SaturatedConversion,
};
use zeitgeist_primitives::{
    constants::{
        base_multiples::*,
        mock::{
            AddOutcomePeriod, AggregationPeriod, AppealBond, AppealPeriod, AuthorizedPalletId,
            BlockHashCount, BlocksPerYear, CloseEarlyBlockPeriod, CloseEarlyDisputeBond,
            CloseEarlyProtectionBlockPeriod, CloseEarlyProtectionTimeFramePeriod,
            CloseEarlyRequestBond, CloseEarlyTimeFramePeriod, CombinatorialTokensPalletId,
            CorrectionPeriod, CourtPalletId, ExistentialDeposit, ExistentialDeposits,
            GdVotingPeriod, GetNativeCurrencyId, GlobalDisputeLockId, GlobalDisputesPalletId,
            InflationPeriod, LockId, MaxAppeals, MaxApprovals, MaxCourtParticipants, MaxCreatorFee,
            MaxDelegations, MaxDisputeDuration, MaxDisputes, MaxEditReasonLen,
            MaxGlobalDisputeVotes, MaxGracePeriod, MaxLiquidityTreeDepth, MaxLocks,
            MaxMarketLifetime, MaxOracleDuration, MaxOwners, MaxRejectReasonLen, MaxReserves,
            MaxSelectedDraws, MaxYearlyInflation, MinCategories, MinDisputeDuration, MinJurorStake,
            MinOracleDuration, MinOutcomeVoteAmount, MinimumPeriod, NeoMaxSwapFee,
            NeoSwapsPalletId, OutsiderBond, PmPalletId, RemoveKeysLimit, RequestInterval,
            TreasuryPalletId, VotePeriod, VotingOutcomeFee, BASE, CENT,
        },
    },
    math::fixed::FixedMul,
    traits::{DeployPoolApi, DistributeFees},
    types::{
        AccountIdTest, Amount, Balance, BasicCurrencyAdapter, BlockNumber, CombinatorialId,
        CurrencyId, Hash, MarketId, Moment,
    },
};
use zrml_combinatorial_tokens::types::{CryptographicIdManager, Fuel};
use zrml_neo_swaps::{types::DecisionMarketOracle, BalanceOf};

#[cfg(feature = "runtime-benchmarks")]
use zeitgeist_primitives::types::NoopCombinatorialTokensBenchmarkHelper;

pub const ALICE: AccountIdTest = 0;
#[allow(unused)]
pub const BOB: AccountIdTest = 1;
pub const CHARLIE: AccountIdTest = 2;
pub const DAVE: AccountIdTest = 3;
pub const EVE: AccountIdTest = 4;
pub const FEE_ACCOUNT: AccountIdTest = 5;
pub const SUDO: AccountIdTest = 123456;
pub const EXTERNAL_FEES: Balance = CENT;
pub const INITIAL_FOREIGN_BALANCE: Balance = 1_000 * BASE;
pub const USDX_MIN_BALANCE: Balance = 1;

parameter_types! {
    pub const FeeAccount: AccountIdTest = FEE_ACCOUNT;
}
ord_parameter_types! {
    pub const AuthorizedDisputeResolutionUser: AccountIdTest = ALICE;
}
ord_parameter_types! {
    pub const Sudo: AccountIdTest = SUDO;
}
parameter_types! {
    pub storage NeoMinSwapFee: Balance = 0;
    pub storage MaxSplits: u16 = 128;
}
parameter_types! {
    pub const AdvisoryBond: Balance = 0;
    pub const AdvisoryBondSlashPercentage: Percent = Percent::from_percent(10);
    pub const OracleBond: Balance = 0;
    pub const ValidityBond: Balance = 0;
    pub const DisputeBond: Balance = 0;
    pub const MaxCategories: u16 = MAX_ASSETS + 1;
    pub TreasuryAccount: AccountIdTest = Treasury::account_id();
}

pub struct DeployPoolNoop;

impl DeployPoolApi for DeployPoolNoop {
    type AccountId = AccountIdTest;
    type Balance = Balance;
    type MarketId = MarketId;

    fn deploy_pool(
        _who: Self::AccountId,
        _market_id: Self::MarketId,
        _amount: Self::Balance,
        _swap_prices: Vec<Self::Balance>,
        _swap_fee: Self::Balance,
    ) -> DispatchResult {
        Ok(())
    }
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
        let fees = amount.bmul(EXTERNAL_FEES.saturated_into()).unwrap();
        match T::MultiCurrency::transfer(
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
        Perbill::from_rational(EXTERNAL_FEES, BASE)
    }
}

pub struct DustRemovalWhitelist;

impl Contains<AccountIdTest> for DustRemovalWhitelist {
    fn contains(account_id: &AccountIdTest) -> bool {
        *account_id == FEE_ACCOUNT
    }
}

construct_runtime!(
    pub enum Runtime {
        NeoSwaps: zrml_neo_swaps,
        AssetManager: orml_currencies,
        Assets: pallet_assets,
        Authorized: zrml_authorized,
        Balances: pallet_balances,
        CombinatorialTokens: zrml_combinatorial_tokens,
        Court: zrml_court,
        Futarchy: zrml_futarchy,
        MarketCommons: zrml_market_commons,
        Preimage: pallet_preimage,
        PredictionCollateral: pallet_prediction_collateral,
        PredictionControl: pallet_prediction_control,
        PredictionMarkets: zrml_prediction_markets,
        Scheduler: pallet_scheduler,
        GlobalDisputes: zrml_global_disputes,
        System: frame_system,
        Timestamp: pallet_timestamp,
        Tokens: orml_tokens,
        Treasury: pallet_treasury,
    }
);

parameter_types! {
    pub const FutarchyMaxProposals: u32 = 16;
    pub const FutarchyMinDuration: BlockNumber = 3;
    pub const MaxScheduledPerBlock: u32 = 16;
    pub MaximumSchedulerWeight: Weight = Weight::from_parts(u64::MAX, u64::MAX);
}

impl pallet_preimage::Config for Runtime {
    type Consideration = ();
    type Currency = ();
    type ManagerOrigin = EnsureRoot<AccountIdTest>;
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
}

impl pallet_scheduler::Config for Runtime {
    type BlockNumberProvider = System;
    type MaxScheduledPerBlock = MaxScheduledPerBlock;
    type MaximumWeight = MaximumSchedulerWeight;
    type OriginPrivilegeCmp = EqualPrivilegeOnly;
    type PalletsOrigin = OriginCaller;
    type Preimages = Preimage;
    type RuntimeCall = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type ScheduleOrigin = EnsureRoot<AccountIdTest>;
    type WeightInfo = ();
}

impl zrml_futarchy::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = crate::types::DecisionMarketBenchmarkHelper<Runtime>;
    type MaxProposals = FutarchyMaxProposals;
    type MinDuration = FutarchyMinDuration;
    type Oracle = DecisionMarketOracle<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type Scheduler = Scheduler;
    type SubmitOrigin = EnsureRoot<AccountIdTest>;
    type WeightInfo = zrml_futarchy::weights::WeightInfo<Runtime>;
}

pub struct DeterministicRandomness;

impl Randomness<Hash, BlockNumber> for DeterministicRandomness {
    fn random(subject: &[u8]) -> (Hash, BlockNumber) {
        (
            <Runtime as frame_system::Config>::Hashing::hash(subject),
            System::block_number(),
        )
    }
}

impl crate::Config for Runtime {
    type CombinatorialId = CombinatorialId;
    type CombinatorialTokens = CombinatorialTokens;
    type CombinatorialTokensUnsafe = CombinatorialTokens;
    type CompleteSetOperations = PredictionMarkets;
    type ExternalFees = ExternalFees<Runtime, FeeAccount>;
    type MarketCommons = MarketCommons;
    type MultiCurrency = AssetManager;
    type PoolId = MarketId;
    type MaxLiquidityTreeDepth = MaxLiquidityTreeDepth;
    type MaxSplits = MaxSplits;
    type MaxSwapFee = NeoMaxSwapFee;
    type PalletId = NeoSwapsPalletId;
    type WeightInfo = zrml_neo_swaps::weights::WeightInfo<Runtime>;
}

impl zrml_prediction_markets::Config for Runtime {
    type AdvisoryBond = AdvisoryBond;
    type AdvisoryBondSlashPercentage = AdvisoryBondSlashPercentage;
    type ApproveOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type BaseAssetPolicy = PredictionCollateral;
    type Authorized = Authorized;
    type CloseEarlyBlockPeriod = CloseEarlyBlockPeriod;
    type CloseEarlyDisputeBond = CloseEarlyDisputeBond;
    type CloseEarlyTimeFramePeriod = CloseEarlyTimeFramePeriod;
    type CloseEarlyProtectionBlockPeriod = CloseEarlyProtectionBlockPeriod;
    type CloseEarlyProtectionTimeFramePeriod = CloseEarlyProtectionTimeFramePeriod;
    type CloseEarlyRequestBond = CloseEarlyRequestBond;
    type CloseMarketEarlyOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type CloseOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type Court = Court;
    type Currency = Balances;
    type DeployPool = DeployPoolNoop;
    type DisputeBond = DisputeBond;
    type GlobalDisputes = GlobalDisputes;
    type MaxCategories = MaxCategories;
    type MaxDisputes = MaxDisputes;
    type MinDisputeDuration = MinDisputeDuration;
    type MinOracleDuration = MinOracleDuration;
    type MaxCreatorFee = MaxCreatorFee;
    type MaxDisputeDuration = MaxDisputeDuration;
    type MaxGracePeriod = MaxGracePeriod;
    type MaxOracleDuration = MaxOracleDuration;
    type MaxMarketLifetime = MaxMarketLifetime;
    type MinCategories = MinCategories;
    type MaxEditReasonLen = MaxEditReasonLen;
    type MaxRejectReasonLen = MaxRejectReasonLen;
    type OracleBond = OracleBond;
    type OutsiderBond = OutsiderBond;
    type PalletId = PmPalletId;
    type RejectOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type RequestEditOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type ResolveOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type AssetManager = AssetManager;
    type Slash = Treasury;
    type ValidityBond = ValidityBond;
    type WeightInfo = zrml_prediction_markets::weights::WeightInfo<Runtime>;
}

impl zrml_authorized::Config for Runtime {
    type AuthorizedDisputeResolutionOrigin =
        EnsureSignedBy<AuthorizedDisputeResolutionUser, AccountIdTest>;
    type CorrectionPeriod = CorrectionPeriod;
    type Currency = Balances;
    type DisputeResolution = zrml_prediction_markets::Pallet<Runtime>;
    type MarketCommons = MarketCommons;
    type PalletId = AuthorizedPalletId;
    type WeightInfo = zrml_authorized::weights::WeightInfo<Runtime>;
}

impl zrml_combinatorial_tokens::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = NoopCombinatorialTokensBenchmarkHelper<Balance, MarketId>;
    type CombinatorialIdManager = CryptographicIdManager<MarketId, Blake2_256>;
    type Fuel = Fuel;
    type MarketCommons = MarketCommons;
    type MultiCurrency = AssetManager;
    type Payout = PredictionMarkets;
    type PalletId = CombinatorialTokensPalletId;
    type WeightInfo = zrml_combinatorial_tokens::weights::WeightInfo<Runtime>;
}

impl zrml_court::Config for Runtime {
    type AppealBond = AppealBond;
    type BlocksPerYear = BlocksPerYear;
    type DisputeResolution = zrml_prediction_markets::Pallet<Runtime>;
    type VotePeriod = VotePeriod;
    type AggregationPeriod = AggregationPeriod;
    type AppealPeriod = AppealPeriod;
    type LockId = LockId;
    type Currency = Balances;
    type InflationPeriod = InflationPeriod;
    type MarketCommons = MarketCommons;
    type MaxAppeals = MaxAppeals;
    type MaxDelegations = MaxDelegations;
    type MaxSelectedDraws = MaxSelectedDraws;
    type MaxCourtParticipants = MaxCourtParticipants;
    type MaxYearlyInflation = MaxYearlyInflation;
    type MinJurorStake = MinJurorStake;
    type MonetaryGovernanceOrigin = EnsureRoot<AccountIdTest>;
    type PalletId = CourtPalletId;
    type Random = DeterministicRandomness;
    type RequestInterval = RequestInterval;
    type Slash = Treasury;
    type TreasuryPalletId = TreasuryPalletId;
    type WeightInfo = zrml_court::weights::WeightInfo<Runtime>;
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
    pub const CollateralPalletId: PalletId = PalletId(*b"ns/collt");
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
    type DustRemovalWhitelist = DustRemovalWhitelist;
    type ExistentialDeposits = ExistentialDeposits;
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type CurrencyHooks = ();
    type ReserveIdentifier = [u8; 8];
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

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Runtime {
    type MinimumPeriod = MinimumPeriod;
    type Moment = Moment;
}

impl zrml_global_disputes::Config for Runtime {
    type AddOutcomePeriod = AddOutcomePeriod;
    type DisputeResolution = zrml_prediction_markets::Pallet<Runtime>;
    type MarketCommons = MarketCommons;
    type Currency = Balances;
    type GlobalDisputeLockId = GlobalDisputeLockId;
    type GlobalDisputesPalletId = GlobalDisputesPalletId;
    type MaxGlobalDisputeVotes = MaxGlobalDisputeVotes;
    type MaxOwners = MaxOwners;
    type MinOutcomeVoteAmount = MinOutcomeVoteAmount;
    type RemoveKeysLimit = RemoveKeysLimit;
    type GdVotingPeriod = GdVotingPeriod;
    type VotingOutcomeFee = VotingOutcomeFee;
    type WeightInfo = zrml_global_disputes::weights::WeightInfo<Runtime>;
}

impl pallet_treasury::Config for Runtime {
    type AssetKind = ();
    type BalanceConverter = UnityAssetBalanceConversion;
    type BlockNumberProvider = System;
    type Beneficiary = AccountIdTest;
    type BeneficiaryLookup = IdentityLookup<AccountIdTest>;
    type Burn = ();
    type BurnDestination = ();
    type Currency = Balances;
    type RuntimeEvent = RuntimeEvent;
    type MaxApprovals = MaxApprovals;
    type PalletId = TreasuryPalletId;
    type Paymaster = PayFromAccount<Balances, TreasuryAccount>;
    type PayoutPeriod = ();
    type RejectOrigin = EnsureSignedBy<Sudo, AccountIdTest>;
    type SpendFunds = ();
    type SpendOrigin = NeverEnsureOrigin<Balance>;
    type SpendPeriod = ();
    type WeightInfo = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

#[allow(unused)]
pub struct ExtBuilder {
    balances: Vec<(AccountIdTest, Balance)>,
}

// TODO(#1222): Remove this in favor of adding whatever the account need in the individual tests.
#[allow(unused)]
impl Default for ExtBuilder {
    fn default() -> Self {
        Self {
            balances: vec![
                (ALICE, 100_000_000_001 * _1),
                (CHARLIE, _1),
                (DAVE, _1),
                (EVE, _1),
            ],
        }
    }
}

#[allow(unused)]
impl ExtBuilder {
    pub fn build(self) -> sp_io::TestExternalities {
        let mut t = frame_system::GenesisConfig::<Runtime>::default()
            .build_storage()
            .unwrap();
        // see the logs in tests when using `RUST_LOG=debug cargo test -- --nocapture`
        let _ = env_logger::builder().is_test(true).try_init();
        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balances,
            dev_accounts: None,
        }
        .assimilate_storage(&mut t)
        .unwrap();
        let mut test_ext: sp_io::TestExternalities = t.into();
        test_ext.execute_with(|| {
            System::set_block_number(1);
            assert_ok!(Assets::force_create(
                RuntimeOrigin::root(),
                USDX_ASSET_ID,
                SUDO,
                true,
                USDX_MIN_BALANCE,
            ));
            for account in 0..69 {
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
            for account in 0..69 {
                assert_ok!(PredictionCollateral::deposit(
                    RuntimeOrigin::signed(account),
                    USDX_ASSET_ID,
                    INITIAL_FOREIGN_BALANCE,
                ));
            }
            System::reset_events();
        });
        test_ext
    }
}
