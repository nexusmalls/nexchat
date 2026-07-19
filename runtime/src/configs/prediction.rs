//! Prediction-market runtime wiring.
//! 预测市场 runtime 接线。
//!
//! Phase 6 registers the complete subsystem while `PredictionControl` remains
//! globally `Disabled`. Imported weights are integration-only placeholders and
//! must be regenerated in Phase 7 before any business module is enabled.
//! Phase 6 注册完整子系统，但 `PredictionControl` 全局保持 `Disabled`。导入权重仅
//! 用于集成验证；任何业务模块启用前必须在 Phase 7 重新生成。

use core::marker::PhantomData;

use codec::Encode;
use frame_support::{
    parameter_types,
    traits::{
        fungibles::Inspect, Currency, EitherOf, ExistenceRequirement, OnUnbalanced, Randomness,
    },
    Blake2_256, PalletId,
};
use frame_system::EnsureRoot;
use orml_traits::{currency::MutationHooks, parameter_type_with_key, MultiCurrency};
use pallet_prediction_control::{PredictionControlApi, PredictionModule, CALL_REGISTRY};
use pallet_usdx::ProtocolAssetInspector;
use sp_runtime::{
    traits::{AccountIdConversion, Hash as HashT, Zero},
    Perbill, Percent,
};
use zeitgeist_primitives::{
    constants::BASE as PREDICTION_BASE,
    traits::DistributeFees,
    types::{Amount, Asset, BasicCurrencyAdapter, CombinatorialId, MarketId},
};

use crate::{
    configs::{
        ismp::{NexusProtocolAssetInspector, UsdxAssetId},
        ArbitrationCollectiveInstance, TechnicalCollectiveInstance, TreasuryAccountId,
        TreasuryCollectiveInstance, TreasuryPalletId,
    },
    AccountId, Assets, Babe, Balance, Balances, BlockNumber, Hash, PredictionAuthorized,
    PredictionCollateral, PredictionCombinatorialTokens, PredictionControl, PredictionCourt,
    PredictionCommunityCore, PredictionCurrencies, PredictionGlobalDisputes, PredictionMarkets,
    PredictionNeoSwaps, PredictionOrderbook, PredictionTokens, Runtime, RuntimeCall, RuntimeEvent,
    Scheduler, Timestamp, Usdx, DAYS, HOURS, NEX,
};

pub type PredictionAsset = Asset<MarketId>;

pub type TechnicalMajority =
    pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollectiveInstance, 2, 3>;
pub type ArbitrationMajority =
    pallet_collective::EnsureProportionAtLeast<AccountId, ArbitrationCollectiveInstance, 2, 3>;
pub type TreasuryMajority =
    pallet_collective::EnsureProportionAtLeast<AccountId, TreasuryCollectiveInstance, 2, 3>;

pub type RootOrTechnicalMajority = EitherOf<EnsureRoot<AccountId>, TechnicalMajority>;
pub type RootOrArbitrationMajority = EitherOf<EnsureRoot<AccountId>, ArbitrationMajority>;
pub type RootOrTreasuryMajority = EitherOf<EnsureRoot<AccountId>, TreasuryMajority>;

parameter_types! {
    pub const PredictionCollateralPalletId: PalletId = PalletId(*b"pr/collt");
    pub const PredictionMarketPalletId: PalletId = PalletId(*b"pr/mrkt!");
    pub const PredictionLegacySwapsPalletId: PalletId = PalletId(*b"pr/swaps");
    pub const PredictionNeoSwapsPalletId: PalletId = PalletId(*b"pr/neoss");
    pub const PredictionOrderbookPalletId: PalletId = PalletId(*b"pr/order");
    pub const PredictionParimutuelPalletId: PalletId = PalletId(*b"pr/parim");
    pub const PredictionCourtPalletId: PalletId = PalletId(*b"pr/court");
    pub const PredictionGlobalDisputesPalletId: PalletId = PalletId(*b"pr/globl");
    pub const PredictionCombinatorialPalletId: PalletId = PalletId(*b"pr/combo");
    pub const PredictionHybridRouterPalletId: PalletId = PalletId(*b"pr/hybrd");
    pub const PredictionAuthorizedPalletId: PalletId = PalletId(*b"pr/authz");
    pub const PredictionCommunityPalletId: PalletId = PalletId(*b"pr/ccomm");
    pub const PredictionCommunityBond: Balance = 1_000_000 * NEX;
    pub const PredictionCommunityBondUnbondDelay: BlockNumber = 7 * DAYS;
    pub const PredictionCommunityMaxTickets: u32 = 100_000;
    pub const PredictionCommunityMaxSettleBatch: u32 = 32;
    pub const PredictionCommunityMaxSingleLineLength: u32 = 200;
    pub const PredictionCommunityMaxSegmentCount: u32 = 10_000;
    /// ~1 day at 6s block time.
    pub const PredictionCommunityPoolRoundDuration: BlockNumber = 14_400;
    pub PredictionCommunityUsdx: PredictionAsset = Asset::ForeignAsset(UsdxAssetId::get());

    pub const PredictionAdvisoryBond: Balance = 100 * NEX;
    pub const PredictionValidityBond: Balance = 10 * NEX;
    pub const PredictionOracleBond: Balance = 5 * NEX;
    pub const PredictionOutsiderBond: Balance = 5 * NEX;
    pub const PredictionDisputeBond: Balance = 10 * NEX;
    pub const PredictionCloseEarlyRequestBond: Balance = 10 * NEX;
    pub const PredictionCloseEarlyDisputeBond: Balance = 20 * NEX;
    pub const PredictionAppealBond: Balance = 50 * NEX;
    pub const PredictionMinJurorStake: Balance = 100 * NEX;
    pub const PredictionVotingOutcomeFee: Balance = NEX;
    pub const PredictionMinOutcomeVoteAmount: Balance = NEX;
    pub const PredictionMinBetSize: Balance = NEX;
    pub const PredictionDefaultStyxBurn: Balance = 200 * NEX;

    pub const PredictionAdvisoryBondSlashPercentage: Percent = Percent::from_percent(10);
    pub const PredictionMaxCreatorFee: Perbill = Perbill::from_percent(1);
    pub const PredictionMaxYearlyInflation: Perbill = Perbill::from_percent(1);

    pub const PredictionCorrectionPeriod: BlockNumber = 7 * DAYS;
    pub const PredictionAddOutcomePeriod: BlockNumber = DAYS;
    pub const PredictionGdVotingPeriod: BlockNumber = 7 * DAYS;
    pub const PredictionCourtVotePeriod: BlockNumber = DAYS;
    pub const PredictionCourtAggregationPeriod: BlockNumber = DAYS;
    pub const PredictionCourtAppealPeriod: BlockNumber = DAYS;
    pub const PredictionInflationPeriod: BlockNumber = 30 * DAYS;
    pub const PredictionRequestInterval: BlockNumber = HOURS;
    pub const PredictionBlocksPerYear: BlockNumber = 365 * DAYS;
    pub const PredictionFutarchyMinDuration: BlockNumber = HOURS;

    pub const PredictionCloseEarlyBlockPeriod: BlockNumber = HOURS;
    pub const PredictionCloseEarlyProtectionBlockPeriod: BlockNumber = HOURS;
    pub const PredictionCloseEarlyTimeFramePeriod: u64 = 60 * 60 * 1_000;
    pub const PredictionCloseEarlyProtectionTimeFramePeriod: u64 = 60 * 60 * 1_000;
    pub const PredictionMinDisputeDuration: BlockNumber = HOURS;
    pub const PredictionMinOracleDuration: BlockNumber = HOURS;
    pub const PredictionMaxDisputeDuration: BlockNumber = 30 * DAYS;
    pub const PredictionMaxGracePeriod: BlockNumber = 7 * DAYS;
    pub const PredictionMaxOracleDuration: BlockNumber = 30 * DAYS;
    pub const PredictionMaxMarketLifetime: BlockNumber = 365 * DAYS;

    pub const PredictionMinCategories: u16 = 2;
    pub const PredictionMaxCategories: u16 = 64;
    pub const PredictionMaxDisputes: u32 = 8;
    pub const PredictionMaxEditReasonLen: u32 = 1_024;
    pub const PredictionMaxRejectReasonLen: u32 = 1_024;
    pub const PredictionMaxGlobalDisputeVotes: u32 = 256;
    pub const PredictionMaxOwners: u32 = 64;
    pub const PredictionRemoveKeysLimit: u32 = 64;
    pub const PredictionMaxAppeals: u32 = 4;
    pub const PredictionMaxDelegations: u32 = 64;
    pub const PredictionMaxSelectedDraws: u32 = 510;
    pub const PredictionMaxCourtParticipants: u32 = 1_024;
    pub const PredictionMaxLiquidityTreeDepth: u32 = 8;
    pub const PredictionMaxSplits: u16 = 128;
    pub const PredictionFutarchyMaxProposals: u32 = 16;
    pub const PredictionHybridMaxOrders: u32 = 64;

    pub const PredictionLegacyExitFee: Balance = PREDICTION_BASE / 10_000;
    pub const PredictionLegacyMinAssets: u16 = 2;
    pub const PredictionLegacyMaxAssets: u16 = 64;
    pub const PredictionLegacyMaxSwapFee: Balance = PREDICTION_BASE / 10;
    pub const PredictionLegacyMinWeight: Balance = PREDICTION_BASE;
    pub const PredictionLegacyMaxWeight: Balance = 50 * PREDICTION_BASE;
    pub const PredictionLegacyMaxTotalWeight: Balance = 64 * 50 * PREDICTION_BASE;
    pub const PredictionNeoMaxSwapFee: Balance = PREDICTION_BASE / 10;

    pub const PredictionGlobalDisputeLockId: [u8; 8] = *b"pr/gdlck";
    pub const PredictionCourtLockId: [u8; 8] = *b"pr/crtlk";
}

parameter_type_with_key! {
    pub PredictionExistentialDeposits: |currency_id: PredictionAsset| -> Balance {
        match currency_id {
            Asset::Ztg => 0,
            _ => 1,
        }
    };
}

pub struct PredictionNativeCurrencyId;
impl frame_support::traits::Get<PredictionAsset> for PredictionNativeCurrencyId {
    fn get() -> PredictionAsset {
        Asset::Ztg
    }
}

pub struct PredictionDustWhitelist;
impl frame_support::traits::Contains<AccountId> for PredictionDustWhitelist {
    fn contains(account: &AccountId) -> bool {
        [
            PredictionCollateralPalletId::get(),
            PredictionMarketPalletId::get(),
            PredictionLegacySwapsPalletId::get(),
            PredictionNeoSwapsPalletId::get(),
            PredictionOrderbookPalletId::get(),
            PredictionParimutuelPalletId::get(),
            PredictionCourtPalletId::get(),
            PredictionGlobalDisputesPalletId::get(),
            PredictionCombinatorialPalletId::get(),
            PredictionHybridRouterPalletId::get(),
            PredictionAuthorizedPalletId::get(),
        ]
        .into_iter()
        .map(|id| id.into_account_truncating())
        .any(|id: AccountId| id == *account)
    }
}

pub struct PredictionCurrencyHooks(PhantomData<Runtime>);
impl MutationHooks<AccountId, PredictionAsset, Balance> for PredictionCurrencyHooks {
    type OnDust = orml_tokens::TransferDust<Runtime, TreasuryAccountId>;
    type OnKilledTokenAccount = ();
    type OnNewTokenAccount = ();
    type OnSlash = ();
    type PostDeposit = ();
    type PostTransfer = ();
    type PreDeposit = ();
    type PreTransfer = ();
}

/// Supplies stable non-native prediction assets for ORML runtime benchmarks.
/// 为 ORML runtime benchmark 提供稳定的非原生预测资产。
pub struct PredictionOrmlBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl orml_tokens::BenchmarkHelper<PredictionAsset, Balance> for PredictionOrmlBenchmarkHelper {
    fn get_currency_id_and_amount() -> Option<(PredictionAsset, Balance)> {
        Some((Asset::ForeignAsset(900_001), NEX))
    }
}

#[cfg(feature = "runtime-benchmarks")]
impl orml_currencies::BenchmarkHelper<PredictionAsset, Balance, Amount>
    for PredictionOrmlBenchmarkHelper
{
    fn get_currency_id_and_amounts() -> Option<(PredictionAsset, Balance, Amount, Amount)> {
        let amount = Amount::try_from(NEX).expect("NEX benchmark amount fits in i128");
        Some((Asset::ForeignAsset(900_001), NEX, amount, -amount))
    }
}

impl orml_tokens::Config for Runtime {
    type Amount = Amount;
    type Balance = Balance;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = PredictionOrmlBenchmarkHelper;
    type CurrencyHooks = PredictionCurrencyHooks;
    type CurrencyId = PredictionAsset;
    type DustRemovalWhitelist = PredictionDustWhitelist;
    type ExistentialDeposits = PredictionExistentialDeposits;
    type MaxLocks = frame_support::traits::ConstU32<64>;
    type MaxReserves = frame_support::traits::ConstU32<64>;
    type ReserveIdentifier = [u8; 8];
    type WeightInfo = orml_tokens::SubstrateWeight<Runtime>;
}

impl orml_currencies::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = PredictionOrmlBenchmarkHelper;
    type GetNativeCurrencyId = PredictionNativeCurrencyId;
    type MultiCurrency = PredictionTokens;
    type NativeCurrency = BasicCurrencyAdapter<Runtime, Balances>;
    type WeightInfo = orml_currencies::SubstrateWeight<Runtime>;
}

impl zrml_market_commons::Config for Runtime {
    type Balance = Balance;
    type MarketId = MarketId;
    type Timestamp = Timestamp;
}

impl pallet_prediction_control::Config for Runtime {
    type UpdateOrigin = RootOrTechnicalMajority;
    type WeightInfo = pallet_prediction_control::weights::SubstrateWeight<Runtime>;
}

pub struct NexusPredictionAssetValidator;
impl pallet_prediction_collateral::AssetValidator for NexusPredictionAssetValidator {
    fn is_valid(asset_id: u64) -> bool {
        let asset_is_live = <Assets as Inspect<AccountId>>::asset_exists(asset_id)
            && pallet_assets::Asset::<Runtime>::get(asset_id)
                .is_some_and(|details| details.status == pallet_assets::AssetStatus::Live);
        if !asset_is_live {
            return false;
        }
        if asset_id != UsdxAssetId::get() {
            return true;
        }

        NexusProtocolAssetInspector::validate_usdx(asset_id, &Usdx::psm_account())
            && !Usdx::global_paused()
            && Usdx::global_debt_ceiling() > 0
            && <Assets as Inspect<AccountId>>::total_issuance(asset_id) == Usdx::total_debt()
    }
}

impl pallet_prediction_collateral::Config for Runtime {
    type AssetValidator = NexusPredictionAssetValidator;
    type Assets = Assets;
    type CollateralPalletId = PredictionCollateralPalletId;
    type Control = PredictionControl;
    type PauseOrigin = RootOrTechnicalMajority;
    type PredictionCurrencies = PredictionCurrencies;
    type WeightInfo = pallet_prediction_collateral::weights::SubstrateWeight<Runtime>;
    type WhitelistOrigin = RootOrTechnicalMajority;
}

pub struct PredictionSlashToTreasury;
impl OnUnbalanced<pallet_balances::NegativeImbalance<Runtime>> for PredictionSlashToTreasury {
    fn on_nonzero_unbalanced(amount: pallet_balances::NegativeImbalance<Runtime>) {
        Balances::resolve_creating(&TreasuryAccountId::get(), amount);
    }
}

pub struct BabeEpochRandomness;
impl Randomness<Hash, BlockNumber> for BabeEpochRandomness {
    fn random(subject: &[u8]) -> (Hash, BlockNumber) {
        let mut material = Babe::randomness().to_vec();
        material.extend_from_slice(b"nexus/prediction/court/v1");
        material.extend_from_slice(subject);
        (
            <Runtime as frame_system::Config>::Hashing::hash(&material),
            frame_system::Pallet::<Runtime>::block_number(),
        )
    }
}

impl zrml_authorized::Config for Runtime {
    type AuthorizedDisputeResolutionOrigin = RootOrArbitrationMajority;
    type CorrectionPeriod = PredictionCorrectionPeriod;
    type Currency = Balances;
    type DisputeResolution = PredictionMarkets;
    type MarketCommons = crate::PredictionMarketCommons;
    type PalletId = PredictionAuthorizedPalletId;
    type WeightInfo = zrml_authorized::weights::WeightInfo<Runtime>;
}

impl zrml_court::Config for Runtime {
    type AggregationPeriod = PredictionCourtAggregationPeriod;
    type AppealBond = PredictionAppealBond;
    type AppealPeriod = PredictionCourtAppealPeriod;
    type BlocksPerYear = PredictionBlocksPerYear;
    type Currency = Balances;
    type DisputeResolution = PredictionMarkets;
    type InflationPeriod = PredictionInflationPeriod;
    type LockId = PredictionCourtLockId;
    type MarketCommons = crate::PredictionMarketCommons;
    type MaxAppeals = PredictionMaxAppeals;
    type MaxCourtParticipants = PredictionMaxCourtParticipants;
    type MaxDelegations = PredictionMaxDelegations;
    type MaxSelectedDraws = PredictionMaxSelectedDraws;
    type MaxYearlyInflation = PredictionMaxYearlyInflation;
    type MinJurorStake = PredictionMinJurorStake;
    type MonetaryGovernanceOrigin = RootOrArbitrationMajority;
    type PalletId = PredictionCourtPalletId;
    type Random = BabeEpochRandomness;
    type RequestInterval = PredictionRequestInterval;
    type Slash = PredictionSlashToTreasury;
    type TreasuryPalletId = TreasuryPalletId;
    type VotePeriod = PredictionCourtVotePeriod;
    type WeightInfo = zrml_court::weights::WeightInfo<Runtime>;
}

impl zrml_global_disputes::Config for Runtime {
    type AddOutcomePeriod = PredictionAddOutcomePeriod;
    type Currency = Balances;
    type DisputeResolution = PredictionMarkets;
    type GdVotingPeriod = PredictionGdVotingPeriod;
    type GlobalDisputeLockId = PredictionGlobalDisputeLockId;
    type GlobalDisputesPalletId = PredictionGlobalDisputesPalletId;
    type MarketCommons = crate::PredictionMarketCommons;
    type MaxGlobalDisputeVotes = PredictionMaxGlobalDisputeVotes;
    type MaxOwners = PredictionMaxOwners;
    type MinOutcomeVoteAmount = PredictionMinOutcomeVoteAmount;
    type RemoveKeysLimit = PredictionRemoveKeysLimit;
    type VotingOutcomeFee = PredictionVotingOutcomeFee;
    type WeightInfo = zrml_global_disputes::weights::WeightInfo<Runtime>;
}

impl zrml_prediction_markets::Config for Runtime {
    type AdvisoryBond = PredictionAdvisoryBond;
    type AdvisoryBondSlashPercentage = PredictionAdvisoryBondSlashPercentage;
    type ApproveOrigin = RootOrTechnicalMajority;
    type AssetManager = PredictionCurrencies;
    type Authorized = PredictionAuthorized;
    type BaseAssetPolicy = PredictionCollateral;
    type CloseEarlyBlockPeriod = PredictionCloseEarlyBlockPeriod;
    type CloseEarlyDisputeBond = PredictionCloseEarlyDisputeBond;
    type CloseEarlyProtectionBlockPeriod = PredictionCloseEarlyProtectionBlockPeriod;
    type CloseEarlyProtectionTimeFramePeriod = PredictionCloseEarlyProtectionTimeFramePeriod;
    type CloseEarlyRequestBond = PredictionCloseEarlyRequestBond;
    type CloseEarlyTimeFramePeriod = PredictionCloseEarlyTimeFramePeriod;
    type CloseMarketEarlyOrigin = RootOrTechnicalMajority;
    type CloseOrigin = RootOrTechnicalMajority;
    type Court = PredictionCourt;
    type Currency = Balances;
    type DeployPool = PredictionNeoSwaps;
    type DisputeBond = PredictionDisputeBond;
    type GlobalDisputes = PredictionGlobalDisputes;
    type MaxCategories = PredictionMaxCategories;
    type MaxCreatorFee = PredictionMaxCreatorFee;
    type MaxDisputeDuration = PredictionMaxDisputeDuration;
    type MaxDisputes = PredictionMaxDisputes;
    type MaxEditReasonLen = PredictionMaxEditReasonLen;
    type MaxGracePeriod = PredictionMaxGracePeriod;
    type MaxMarketLifetime = PredictionMaxMarketLifetime;
    type MaxOracleDuration = PredictionMaxOracleDuration;
    type MaxRejectReasonLen = PredictionMaxRejectReasonLen;
    type MinCategories = PredictionMinCategories;
    type MinDisputeDuration = PredictionMinDisputeDuration;
    type MinOracleDuration = PredictionMinOracleDuration;
    type OracleBond = PredictionOracleBond;
    type OutsiderBond = PredictionOutsiderBond;
    type PalletId = PredictionMarketPalletId;
    type RejectOrigin = RootOrTechnicalMajority;
    type RequestEditOrigin = RootOrTechnicalMajority;
    type ResolveOrigin = RootOrTechnicalMajority;
    type Slash = PredictionSlashToTreasury;
    type ValidityBond = PredictionValidityBond;
    type WeightInfo = zrml_prediction_markets::weights::WeightInfo<Runtime>;
}

impl zrml_swaps::Config for Runtime {
    type Asset = PredictionAsset;
    type ExitFee = PredictionLegacyExitFee;
    type MaxAssets = PredictionLegacyMaxAssets;
    type MaxSwapFee = PredictionLegacyMaxSwapFee;
    type MaxTotalWeight = PredictionLegacyMaxTotalWeight;
    type MaxWeight = PredictionLegacyMaxWeight;
    type MinAssets = PredictionLegacyMinAssets;
    type MinWeight = PredictionLegacyMinWeight;
    type MultiCurrency = PredictionCurrencies;
    type PalletId = PredictionLegacySwapsPalletId;
    type WeightInfo = zrml_swaps::weights::WeightInfo<Runtime>;
}

impl pallet_prediction_community_core::Config for Runtime {
    type Currency = Balances;
    type MultiCurrency = PredictionCurrencies;
    type MarketId = MarketId;
    type CommunityAsset = PredictionCommunityUsdx;
    type CommunityBond = PredictionCommunityBond;
    type CommunityBondUnbondDelay = PredictionCommunityBondUnbondDelay;
    type TreasuryAccount = TreasuryAccountId;
    type PalletId = PredictionCommunityPalletId;
    type MaxTickets = PredictionCommunityMaxTickets;
    type MaxSettleBatch = PredictionCommunityMaxSettleBatch;
    type MaxSingleLineLength = PredictionCommunityMaxSingleLineLength;
    type MaxSegmentCount = PredictionCommunityMaxSegmentCount;
    type PoolRoundDuration = PredictionCommunityPoolRoundDuration;
    type WeightInfo = ();
}

/// Protocol trade fee distributor (v0.5): 0.03 = 0.01 top bar + 0.02 community side.
/// 协议交易费分发器（v0.5）：0.03 = 0.01 顶栏 + 0.02 社群侧。
pub struct PredictionCommunityFeeDistributor;
impl DistributeFees for PredictionCommunityFeeDistributor {
    type AccountId = AccountId;
    type Asset = PredictionAsset;
    type Balance = Balance;
    type MarketId = MarketId;

    fn distribute(
        market_id: MarketId,
        asset: PredictionAsset,
        account: &AccountId,
        amount: Balance,
    ) -> Balance {
        if asset == Asset::ForeignAsset(UsdxAssetId::get()) {
            let Ok(market) = <crate::PredictionMarketCommons as zrml_market_commons::MarketCommonsPalletApi>::market(
                &market_id,
            ) else {
                return 0;
            };
            return PredictionCommunityCore::apply_usdx_trade_fee(
                account,
                amount,
                &market.creator,
                market.creator_fee,
            );
        }
        // D13: non-USDX markets — full 0.03 to treasury, no USDX allowance.
        PredictionCommunityCore::apply_native_protocol_fee(account, asset, amount)
    }

    fn fee_percentage(_market_id: MarketId) -> Perbill {
        PredictionCommunityCore::protocol_fee_perbill()
    }
}

/// Legacy creator-only fee path (kept for reference / differential tests).
/// 旧版仅盘创建者收费路径（保留供对照 / 差分测试）。
pub struct PredictionMarketCreatorFee;
impl DistributeFees for PredictionMarketCreatorFee {
    type AccountId = AccountId;
    type Asset = PredictionAsset;
    type Balance = Balance;
    type MarketId = MarketId;

    fn distribute(
        market_id: MarketId,
        asset: PredictionAsset,
        account: &AccountId,
        amount: Balance,
    ) -> Balance {
        let Ok(market) =
            <crate::PredictionMarketCommons as zrml_market_commons::MarketCommonsPalletApi>::market(
                &market_id,
            )
        else {
            return 0;
        };
        let fee = market.creator_fee.mul_floor(amount);
        if fee.is_zero() {
            return 0;
        }
        if <PredictionCurrencies as MultiCurrency<AccountId>>::transfer(
            asset,
            account,
            &market.creator,
            fee,
            ExistenceRequirement::AllowDeath,
        )
        .is_ok()
        {
            fee
        } else {
            0
        }
    }

    fn fee_percentage(market_id: MarketId) -> Perbill {
        <crate::PredictionMarketCommons as zrml_market_commons::MarketCommonsPalletApi>::market(
            &market_id,
        )
        .map(|market| market.creator_fee)
        .unwrap_or_else(|_| Perbill::zero())
    }
}

impl zrml_combinatorial_tokens::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper =
        zrml_prediction_markets::types::PredictionMarketsCombinatorialTokensBenchmarkHelper<
            Runtime,
        >;
    type CombinatorialIdManager =
        zrml_combinatorial_tokens::types::CryptographicIdManager<MarketId, Blake2_256>;
    type Fuel = zrml_combinatorial_tokens::types::Fuel;
    type MarketCommons = crate::PredictionMarketCommons;
    type MultiCurrency = PredictionCurrencies;
    type PalletId = PredictionCombinatorialPalletId;
    type Payout = PredictionMarkets;
    type WeightInfo = zrml_combinatorial_tokens::weights::WeightInfo<Runtime>;
}

impl zrml_neo_swaps::Config for Runtime {
    type CombinatorialId = CombinatorialId;
    type CombinatorialTokens = PredictionCombinatorialTokens;
    type CombinatorialTokensUnsafe = PredictionCombinatorialTokens;
    type CompleteSetOperations = PredictionMarkets;
    type ExternalFees = PredictionCommunityFeeDistributor;
    type MarketCommons = crate::PredictionMarketCommons;
    type MaxLiquidityTreeDepth = PredictionMaxLiquidityTreeDepth;
    type MaxSplits = PredictionMaxSplits;
    type MaxSwapFee = PredictionNeoMaxSwapFee;
    type MultiCurrency = PredictionCurrencies;
    type PalletId = PredictionNeoSwapsPalletId;
    type PoolId = MarketId;
    type WeightInfo = zrml_neo_swaps::weights::WeightInfo<Runtime>;
}

impl zrml_orderbook::Config for Runtime {
    type AssetManager = PredictionCurrencies;
    type ExternalFees = PredictionCommunityFeeDistributor;
    type MarketCommons = crate::PredictionMarketCommons;
    type PalletId = PredictionOrderbookPalletId;
    type WeightInfo = zrml_orderbook::weights::WeightInfo<Runtime>;
}

impl zrml_parimutuel::Config for Runtime {
    type AssetManager = PredictionCurrencies;
    type ExternalFees = PredictionCommunityFeeDistributor;
    type MarketCommons = crate::PredictionMarketCommons;
    type MinBetSize = PredictionMinBetSize;
    type PalletId = PredictionParimutuelPalletId;
    type WeightInfo = zrml_parimutuel::weights::WeightInfo<Runtime>;
}

impl zrml_hybrid_router::Config for Runtime {
    type Amm = PredictionNeoSwaps;
    #[cfg(feature = "runtime-benchmarks")]
    type AmmPoolDeployer = PredictionNeoSwaps;
    type AssetManager = PredictionCurrencies;
    #[cfg(feature = "runtime-benchmarks")]
    type CompleteSetOperations = PredictionMarkets;
    type MarketCommons = crate::PredictionMarketCommons;
    type MaxOrders = PredictionHybridMaxOrders;
    type Orderbook = PredictionOrderbook;
    type PalletId = PredictionHybridRouterPalletId;
    type WeightInfo = zrml_hybrid_router::weights::WeightInfo<Runtime>;
}

impl zrml_futarchy::Config for Runtime {
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = zrml_neo_swaps::types::DecisionMarketBenchmarkHelper<Runtime>;
    type MaxProposals = PredictionFutarchyMaxProposals;
    type MinDuration = PredictionFutarchyMinDuration;
    type Oracle = zrml_neo_swaps::types::DecisionMarketOracle<Runtime>;
    type RuntimeEvent = RuntimeEvent;
    type Scheduler = Scheduler;
    type SubmitOrigin = RootOrTechnicalMajority;
    type WeightInfo = zrml_futarchy::weights::WeightInfo<Runtime>;
}

impl zrml_styx::Config for Runtime {
    type Currency = Balances;
    type DefaultBurnAmount = PredictionDefaultStyxBurn;
    type SetBurnAmountOrigin = RootOrTreasuryMajority;
    type WeightInfo = zrml_styx::weights::WeightInfo<Runtime>;
}

fn dispatchable_index<C: Encode>(call: &C) -> Option<u8> {
    call.using_encoded(|encoded| encoded.first().copied())
}

fn business_call_allowed(module: PredictionModule, call_index: Option<u8>) -> bool {
    let Some(call_index) = call_index else {
        return false;
    };
    let Some(entry) = CALL_REGISTRY
        .iter()
        .find(|entry| entry.module == module && entry.call_index == call_index)
    else {
        return false;
    };
    if !PredictionControl::call_allowed(module, entry.class) {
        return false;
    }
    if module == PredictionModule::HybridRouter
        && (!PredictionControl::module_enabled(PredictionModule::NeoSwaps)
            || !PredictionControl::module_enabled(PredictionModule::Orderbook))
    {
        return false;
    }
    if module == PredictionModule::PredictionMarkets
        && entry.name == "create_market_and_deploy_pool"
        && !PredictionControl::module_enabled(PredictionModule::NeoSwaps)
    {
        return false;
    }
    true
}

/// Whether `base_asset` may be used to create/edit markets under Trading/Full.
/// G-PF-7: `ForeignAsset(USDX)` is admitted; other foreign assets stay blocked in Trading.
/// Trading/Full 下是否允许该计价资产开盘/改盘。
/// G-PF-7：允许 `ForeignAsset(USDX)`；Trading 下其它 foreign 资产仍拒绝。
fn prediction_foreign_base_allowed(base_asset: &PredictionAsset) -> bool {
    match base_asset {
        Asset::Ztg => true,
        // G-PF-7: USDX always passes the call filter; pallet `BaseAssetPolicy` still enforces
        // Full + whitelist + PSM gates before create_market succeeds.
        // G-PF-7：USDX 通过 call filter；开盘仍受 pallet `BaseAssetPolicy`（Full+白名单+PSM）约束。
        Asset::ForeignAsset(id) if *id == UsdxAssetId::get() => true,
        // Other foreign bases: only when live deposit gates already admit them (implies Full).
        // 其它 foreign 计价：仅当实时 deposit 门禁已放行（隐含 Full）。
        Asset::ForeignAsset(id) => PredictionCollateral::is_deposit_allowed(*id),
        _ => true,
    }
}

fn prediction_market_call_allowed(call: &zrml_prediction_markets::Call<Runtime>) -> bool {
    if !business_call_allowed(
        PredictionModule::PredictionMarkets,
        dispatchable_index(call),
    ) {
        return false;
    }
    let mode = PredictionControl::prediction_mode();
    use pallet_prediction_control::PredictionMode;
    // ResolutionOnly / Disabled: business_call_allowed already gated create paths.
    // Trading / Full: admit native + USDX (G-PF-7); reject other foreign bases in Trading.
    if !matches!(mode, PredictionMode::Trading | PredictionMode::Full) {
        return true;
    }
    let base = match call {
        zrml_prediction_markets::Call::create_market { base_asset, .. }
        | zrml_prediction_markets::Call::edit_market { base_asset, .. }
        | zrml_prediction_markets::Call::create_market_and_deploy_pool { base_asset, .. } => {
            Some(base_asset)
        }
        _ => None,
    };
    match base {
        Some(base_asset) => prediction_foreign_base_allowed(base_asset),
        None => true,
    }
}

/// Returns an admission decision for prediction calls, or `None` for unrelated calls.
/// 返回预测调用的准入结论；非预测调用返回 `None`。
pub fn prediction_call_allowed(call: &RuntimeCall) -> Option<bool> {
    let allowed = match call {
        RuntimeCall::PredictionControl(_) => true,
        RuntimeCall::PredictionCollateral(call) => match dispatchable_index(call) {
            // G-PF-7 / Phase 8: admit mirror deposit only in Full mode.
            // G-PF-7 / Phase 8：仅 Full 模式放行镜像 deposit。
            Some(0) => {
                PredictionControl::prediction_mode()
                    == pallet_prediction_control::PredictionMode::Full
            }
            Some(1..=4) => true,
            _ => false,
        },
        RuntimeCall::PredictionCurrencies(_) | RuntimeCall::PredictionTokens(_) => false,
        RuntimeCall::PredictionAuthorized(call) => {
            business_call_allowed(PredictionModule::Authorized, dispatchable_index(call))
        }
        RuntimeCall::PredictionCourt(call) => {
            business_call_allowed(PredictionModule::Court, dispatchable_index(call))
        }
        RuntimeCall::PredictionGlobalDisputes(call) => {
            business_call_allowed(PredictionModule::GlobalDisputes, dispatchable_index(call))
        }
        RuntimeCall::PredictionMarkets(call) => prediction_market_call_allowed(call),
        RuntimeCall::PredictionLegacySwaps(call) => {
            business_call_allowed(PredictionModule::LegacySwaps, dispatchable_index(call))
        }
        RuntimeCall::PredictionNeoSwaps(call) => {
            business_call_allowed(PredictionModule::NeoSwaps, dispatchable_index(call))
        }
        RuntimeCall::PredictionOrderbook(call) => {
            business_call_allowed(PredictionModule::Orderbook, dispatchable_index(call))
        }
        RuntimeCall::PredictionParimutuel(call) => {
            business_call_allowed(PredictionModule::Parimutuel, dispatchable_index(call))
        }
        RuntimeCall::PredictionHybridRouter(call) => {
            business_call_allowed(PredictionModule::HybridRouter, dispatchable_index(call))
        }
        RuntimeCall::PredictionCombinatorialTokens(call) => business_call_allowed(
            PredictionModule::CombinatorialTokens,
            dispatchable_index(call),
        ),
        RuntimeCall::PredictionFutarchy(call) => {
            business_call_allowed(PredictionModule::Futarchy, dispatchable_index(call))
        }
        RuntimeCall::PredictionStyx(call) => {
            business_call_allowed(PredictionModule::Styx, dispatchable_index(call))
        }
        // Community fee/bond path is independent of PredictionMode trading gates.
        // 社群分佣/押金路径独立于 PredictionMode 交易门禁。
        RuntimeCall::PredictionCommunityCore(_) => true,
        RuntimeCall::PredictionMarketCommons(_) => false,
        _ => return None,
    };
    Some(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configs::NexusBaseCallFilter, RuntimeOrigin};
    use frame_support::assert_ok;
    use frame_support::traits::Contains;
    use pallet_prediction_control::{PredictionMode, PredictionModule};
    use zeitgeist_primitives::types::{
        Deadlines, MarketCreation, MarketPeriod, MarketType, MultiHash, ScoringRule,
    };

    fn with_ext(test: impl FnOnce()) {
        sp_io::TestExternalities::default().execute_with(test);
    }

    #[test]
    fn prediction_genesis_defaults_are_inert() {
        with_ext(|| {
            assert_eq!(
                PredictionControl::prediction_mode(),
                PredictionMode::Disabled
            );
            for module in [
                PredictionModule::PredictionMarkets,
                PredictionModule::Authorized,
                PredictionModule::Court,
                PredictionModule::GlobalDisputes,
                PredictionModule::LegacySwaps,
                PredictionModule::NeoSwaps,
                PredictionModule::Orderbook,
                PredictionModule::Parimutuel,
                PredictionModule::HybridRouter,
                PredictionModule::CombinatorialTokens,
                PredictionModule::Futarchy,
                PredictionModule::Styx,
            ] {
                assert!(!PredictionControl::module_enabled(module));
            }
            assert_eq!(
                pallet_prediction_collateral::WhitelistedAssets::<Runtime>::iter().count(),
                0
            );
        });
    }

    #[test]
    fn disabled_mode_rejects_representative_business_calls() {
        with_ext(|| {
            let deposit =
                RuntimeCall::PredictionCollateral(pallet_prediction_collateral::Call::deposit {
                    asset_id: 900_000,
                    amount: NEX,
                });
            let market =
                RuntimeCall::PredictionMarkets(zrml_prediction_markets::Call::buy_complete_set {
                    market_id: 0,
                    amount: NEX,
                });
            let order = RuntimeCall::PredictionOrderbook(zrml_orderbook::Call::place_order {
                market_id: 0,
                maker_asset: Asset::Ztg,
                maker_amount: NEX,
                taker_asset: Asset::CategoricalOutcome(0, 0),
                taker_amount: NEX,
            });
            let styx = RuntimeCall::PredictionStyx(zrml_styx::Call::cross {});

            for call in [deposit, market, order, styx] {
                assert!(!NexusBaseCallFilter::contains(&call));
            }
        });
    }

    #[test]
    fn disabled_mode_keeps_unwind_and_governance_paths_available() {
        with_ext(|| {
            let withdraw =
                RuntimeCall::PredictionCollateral(pallet_prediction_collateral::Call::withdraw {
                    asset_id: 900_000,
                    amount: NEX,
                });
            let control = RuntimeCall::PredictionControl(
                pallet_prediction_control::Call::set_prediction_mode {
                    new: PredictionMode::Disabled,
                },
            );
            assert!(NexusBaseCallFilter::contains(&withdraw));
            assert!(NexusBaseCallFilter::contains(&control));
        });
    }

    #[test]
    fn trading_mode_admits_usdx_but_rejects_other_foreign_market_creation() {
        with_ext(|| {
            assert_ok!(PredictionControl::set_prediction_mode(
                RuntimeOrigin::root(),
                PredictionMode::Trading,
            ));
            assert_ok!(PredictionControl::set_module_enabled(
                RuntimeOrigin::root(),
                PredictionModule::PredictionMarkets,
                true,
            ));
            let create_call = |base_asset| {
                RuntimeCall::PredictionMarkets(zrml_prediction_markets::Call::create_market {
                    base_asset,
                    creator_fee: Perbill::zero(),
                    oracle: AccountId::new([1; 32]),
                    period: MarketPeriod::Block(1..2),
                    deadlines: Deadlines::default(),
                    metadata: MultiHash::Sha3_384([0; 50]),
                    creation: MarketCreation::Permissionless,
                    market_type: MarketType::Categorical(2),
                    dispute_mechanism: None,
                    scoring_rule: ScoringRule::AmmCdaHybrid,
                })
            };

            assert!(NexusBaseCallFilter::contains(&create_call(Asset::Ztg)));
            // G-PF-7: USDX base passes the call filter (pallet BaseAssetPolicy still needs Full+whitelist).
            assert!(NexusBaseCallFilter::contains(&create_call(
                Asset::ForeignAsset(900_000),
            )));
            assert!(!NexusBaseCallFilter::contains(&create_call(
                Asset::ForeignAsset(900_001),
            )));
        });
    }

    #[test]
    fn full_mode_admits_collateral_deposit_call() {
        with_ext(|| {
            let deposit =
                RuntimeCall::PredictionCollateral(pallet_prediction_collateral::Call::deposit {
                    asset_id: 900_000,
                    amount: NEX,
                });
            assert_ok!(PredictionControl::set_prediction_mode(
                RuntimeOrigin::root(),
                PredictionMode::Disabled,
            ));
            assert!(!NexusBaseCallFilter::contains(&deposit));

            assert_ok!(PredictionControl::set_prediction_mode(
                RuntimeOrigin::root(),
                PredictionMode::Full,
            ));
            assert!(NexusBaseCallFilter::contains(&deposit));
        });
    }
}
