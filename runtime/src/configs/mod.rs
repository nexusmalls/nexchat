// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// ISMP / Hyperbridge protocol-layer config (Stage 1).
// ISMP / Hyperbridge 协议层配置（Stage 1）。
pub mod ismp;

// Substrate and Polkadot dependencies
use crate::{
    CommissionCore, CommissionSingleLine, EntityGovernance, EntityLoyalty, EntityMarket,
    OriginCaller, Preimage, SessionKeys, UncheckedExtrinsic,
};
use frame_support::{
    derive_impl, parameter_types,
    traits::{
        ConstBool, ConstU128, ConstU16, ConstU32, ConstU64, ConstU8, LinearStoragePrice,
        VariantCountOf,
    },
    weights::{
        constants::{RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND},
        IdentityFee, Weight,
    },
};
use frame_system::{
    limits::{BlockLength, BlockWeights},
    EnsureRoot,
};
use frame_election_provider_support::{onchain, SequentialPhragmen};
use pallet_transaction_payment::{FungibleAdapter, Multiplier, TargetedFeeAdjustment};
use sp_runtime::{generic, traits::AccountIdConversion};
use sp_runtime::{transaction_validity::TransactionPriority, FixedPointNumber, Perbill};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
    AccountId,
    ArbitrationCommittee,
    // Entity types (原 ShareMall)
    Assets,
    Babe,
    Balance,
    Balances,
    Block,
    BlockNumber,
    ContentCommittee,
    EntityDisclosure,
    EntityKyc,
    EntityProduct,
    EntityRegistry,
    EntityShop,
    EntityToken,
    EntityTokenSale,
    EntityTransaction,
    Escrow,
    ChatCore,
    ChatPermission,
    Hash,
    Historical,
    Nonce,
    Offences,
    PalletInfo,
    Runtime,
    RuntimeCall,
    RuntimeEvent,
    RuntimeFreezeReason,
    RuntimeHoldReason,
    RuntimeOrigin,
    RuntimeTask,
    Session,
    Staking,
    System,
    TechnicalCommittee,
    Timestamp,
    TreasuryCouncil,
    DAYS,
    EXISTENTIAL_DEPOSIT,
    HOURS,
    MINUTES,
    SLOT_DURATION,
    UNIT,
    VERSION,
};

const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: RuntimeVersion = VERSION;

    /// We allow for 2 seconds of compute with a 6 second average block time.
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::with_sensible_defaults(
        Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
        NORMAL_DISPATCH_RATIO,
    );
    // Hyperbridge (Stage 1b-3): ISMP consensus/state proofs are large, so the block
    // length is raised to 8 MiB with an 85% normal-extrinsic ratio. The length ratio
    // is intentionally decoupled from the weight ratio (`NORMAL_DISPATCH_RATIO`, 75%):
    // proofs are byte-heavy but not compute-heavy, so only the byte budget is widened.
    // Hyperbridge（Stage 1b-3）：ISMP 共识/状态证明体积较大，故将块长度提升至 8 MiB、普通
    // 外部交易占比 85%。长度占比刻意与权重占比（`NORMAL_DISPATCH_RATIO`，75%）解耦：证明
    // 占字节多但算力少，故只放宽字节预算。
    pub RuntimeBlockLength: BlockLength = BlockLength::max_with_normal_ratio(
        8 * 1024 * 1024,
        Perbill::from_percent(85),
    );
    pub const SS58Prefix: u16 = 273;
}

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`SoloChainDefaultConfig`](`struct@frame_system::config_preludes::SolochainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The block type for the runtime.
    type Block = Block;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

// ============================================================================
// BABE consensus / BABE 共识出块
// ============================================================================

parameter_types! {
    /// Number of slots per epoch (1 epoch = 1 session = 4 hours at 6s/slot).
    /// 每个 epoch 的 slot 数量（1 epoch = 1 session = 4 小时，6 秒一个 slot）。
    pub const EpochDuration: u64 = 4 * HOURS as u64;
    /// Report longevity for equivocation proofs: BondingDuration * SessionsPerEra * EpochDuration.
    /// Equivocation 举报有效期 = 解绑周期 × 每 era 的 session 数 × epoch 长度。
    pub const ReportLongevity: u64 = 28 * 6 * 4 * HOURS as u64;
    /// Base unsigned transaction priority for im-online heartbeats.
    /// im-online 心跳 unsigned 交易基础优先级。
    pub const ImOnlineUnsignedPriority: TransactionPriority = TransactionPriority::MAX / 2;
    /// Maximum number of authorities (validators).
    /// 最大验证者数量。
    pub const MaxAuthorities: u32 = 100;
    /// Maximum nominators considered per validator for Grandpa/BABE protocol.
    /// Grandpa/BABE 协议中每个验证者考虑的最大提名者数量。
    pub const MaxNominators: u32 = 256;
    /// Maximum heartbeat keys tracked per session.
    /// 每个 session 跟踪的最大心跳 key 数。
    pub const MaxImOnlineKeys: u32 = 10_000;
    /// Maximum peer entries carried in received heartbeats.
    /// 心跳中允许记录的最大 peer 条目数。
    pub const MaxPeerInHeartbeats: u32 = 10_000;
}

impl pallet_babe::Config for Runtime {
    type EpochDuration = EpochDuration;
    type ExpectedBlockTime = ConstU64<SLOT_DURATION>;
    type EpochChangeTrigger = pallet_babe::ExternalTrigger;
    type DisabledValidators = Session;
    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxNominators = MaxNominators;
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = pallet_babe::EquivocationReportSystem<
        Self,
        Offences,
        Historical,
        ReportLongevity,
    >;
}

parameter_types! {
    pub const MaxSetIdSessionEntries: u64 = 168;
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Babe>;
    type EventHandler = (Staking,);
}

pub struct NexusSessionManager;
impl pallet_session::SessionManager<AccountId> for NexusSessionManager {
    fn new_session(new_index: u32) -> Option<alloc::vec::Vec<AccountId>> {
        <pallet_staking::Pallet<Runtime> as pallet_session::SessionManager<AccountId>>::new_session(
            new_index,
        )
    }
    fn end_session(end_index: u32) {
        <pallet_staking::Pallet<Runtime> as pallet_session::SessionManager<AccountId>>::end_session(
            end_index,
        )
    }
    fn start_session(start_index: u32) {
        <pallet_staking::Pallet<Runtime> as pallet_session::SessionManager<AccountId>>::start_session(
            start_index,
        )
    }
}

impl pallet_session::historical::SessionManager<AccountId, pallet_staking::Exposure<AccountId, Balance>>
    for NexusSessionManager
{
    fn new_session(
        new_index: u32,
    ) -> Option<alloc::vec::Vec<(AccountId, pallet_staking::Exposure<AccountId, Balance>)>> {
        <pallet_staking::Pallet<Runtime> as pallet_session::historical::SessionManager<
            AccountId,
            pallet_staking::Exposure<AccountId, Balance>,
        >>::new_session(new_index)
    }
    fn end_session(end_index: u32) {
        <pallet_staking::Pallet<Runtime> as pallet_session::historical::SessionManager<
            AccountId,
            pallet_staking::Exposure<AccountId, Balance>,
        >>::end_session(end_index)
    }
    fn start_session(start_index: u32) {
        <pallet_staking::Pallet<Runtime> as pallet_session::historical::SessionManager<
            AccountId,
            pallet_staking::Exposure<AccountId, Balance>,
        >>::start_session(start_index)
    }
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = sp_runtime::traits::ConvertInto;
    type ShouldEndSession = Babe;
    type NextSessionRotation = Babe;
    type SessionManager = pallet_session::historical::NoteHistoricalRoot<Self, NexusSessionManager>;
    type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type DisablingStrategy = pallet_session::disabling::UpToLimitDisablingStrategy;
    type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
    type Currency = Balances;
    type KeyDeposit = ();
}

impl pallet_session::historical::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type FullIdentification = pallet_staking::Exposure<AccountId, Balance>;
    type FullIdentificationOf = pallet_staking::DefaultExposureOf<Runtime>;
}

impl pallet_offences::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type IdentificationTuple = pallet_session::historical::IdentificationTuple<Runtime>;
    type OnOffenceHandler = Staking;
}

impl pallet_im_online::Config for Runtime {
    type AuthorityId = pallet_im_online::sr25519::AuthorityId;
    type MaxKeys = MaxImOnlineKeys;
    type MaxPeerInHeartbeats = MaxPeerInHeartbeats;
    type RuntimeEvent = RuntimeEvent;
    type ValidatorSet = Historical;
    type NextSessionRotation = Babe;
    type ReportUnresponsiveness = Offences;
    type UnsignedPriority = ImOnlineUnsignedPriority;
    type WeightInfo = pallet_im_online::weights::SubstrateWeight<Runtime>;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxNominators = MaxNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;

    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = pallet_grandpa::EquivocationReportSystem<
        Self,
        Offences,
        Historical,
        ReportLongevity,
    >;
}

// ============================================================================
// NPoS Staking / 提名权益证明
// ============================================================================

/// Fixed annual inflation of 5% for staking rewards, pro-rated per era.
/// 固定年化 5% 通胀率用于 Staking 奖励，按 Era 时长等比分配。
///
/// Unlike `ConvertCurve<RewardCurve>` which varies between 2.5%–10% based on
/// the staking ratio, this implementation always pays out exactly 5% per year
/// regardless of how much NEX is staked.
/// 不同于 `ConvertCurve<RewardCurve>` 根据质押率在 2.5%~10% 之间浮动，
/// 此实现始终按年化 5% 固定发放，与质押率无关。
///
/// Returns `(validator_payout, remainder)`:
/// - `validator_payout`: 80% of era inflation → distributed to NPoS stakers (validators + nominators)
/// - `remainder`: 20% of era inflation → routed to Treasury + RewardPool via `InflationSplitter`
/// 返回 `(validator_payout, remainder)`：
/// - `validator_payout`: 80% 通胀 → 分配给 NPoS 质押者（验证人 + 提名人）
/// - `remainder`: 20% 通胀 → 经 `InflationSplitter` 路由到国库 + 奖励池
pub struct FixedAnnualInflation;

impl pallet_staking::EraPayout<Balance> for FixedAnnualInflation {
    fn era_payout(
        _total_staked: Balance,
        total_issuance: Balance,
        era_duration_millis: u64,
    ) -> (Balance, Balance) {
        // 5% annual inflation, expressed as parts-per-million: 50_000 / 1_000_000
        // 年化 5% 通胀，以百万分比表示：50_000 / 1_000_000
        const ANNUAL_INFLATION_PPM: u64 = 50_000; // 5%
        const MILLISECONDS_PER_YEAR: u64 = 365_25 * 24 * 3600 * 1000 / 100; // 31_557_600_000

        // era_payout = total_issuance × 5% × (era_duration / year_duration)
        // Use u128 to avoid overflow with large issuance values
        let total_payout = (total_issuance as u128)
            .saturating_mul(ANNUAL_INFLATION_PPM as u128)
            .saturating_mul(era_duration_millis as u128)
            / (1_000_000u128 * MILLISECONDS_PER_YEAR as u128);

        // Split: 80% to stakers, 20% as remainder (→ Treasury + RewardPool)
        // 分成: 80% 给质押者, 20% 作为 remainder (→ 国库 + 奖励池)
        let staker_payout = total_payout * 80 / 100;
        let remainder = total_payout.saturating_sub(staker_payout);

        (staker_payout as Balance, remainder as Balance)
    }
}

/// Routes the staking inflation remainder (20% of total) to Treasury and RewardPool.
/// Split: 75% of remainder → Treasury (15% total), 25% of remainder → RewardPool (5% total).
/// 将质押通胀的 remainder（总通胀的 20%）路由到国库和奖励池。
/// 分配: remainder 的 75% → 国库（总通胀 15%），25% → 奖励池（总通胀 5%）。
pub struct InflationSplitter;

impl frame_support::traits::OnUnbalanced<
    frame_support::traits::fungible::Credit<AccountId, Balances>,
> for InflationSplitter
{
    fn on_nonzero_unbalanced(
        amount: frame_support::traits::fungible::Credit<AccountId, Balances>,
    ) {
        use frame_support::traits::{fungible::Balanced, tokens::imbalance::Imbalance};
        // 75% of remainder → Treasury, 25% → RewardPool
        let treasury_share = amount.peek() * 3 / 4;

        // Split the credit: first resolve treasury portion, then reward pool portion
        let (treasury_part, reward_pool_part) = amount.split(treasury_share);
        let _ = <Balances as Balanced<AccountId>>::resolve(&TreasuryAccountId::get(), treasury_part);
        let _ = <Balances as Balanced<AccountId>>::resolve(
            &RewardPoolAccountId::get(),
            reward_pool_part,
        );
    }
}

parameter_types! {
    /// Sessions per era (6 sessions = 1 era ≈ 24h).
    /// 每个纪元的 session 数 (6 个 session = 1 个纪元 ≈ 24 小时)。
    pub const SessionsPerEra: u32 = 6;
    /// Bonding duration in eras (28 eras ≈ 28 days).
    /// 解绑等待期 (28 个纪元 ≈ 28 天)。
    pub const BondingDuration: u32 = 28;
    /// Slash deferral duration in eras.
    /// 惩罚延迟期。
    pub const SlashDeferDuration: u32 = 27;
    /// Election bounds for on-chain sequential phragmen.
    /// 链上选举边界。
    pub ElectionsBounds: frame_election_provider_support::bounds::ElectionBounds =
        frame_election_provider_support::bounds::ElectionBoundsBuilder::default()
            .voters_count(10_000.into())
            .targets_count(1_500.into())
            .build();
}

/// On-chain sequential Phragmén election provider.
/// 链上 Phragmén 顺序选举提供者。
pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<AccountId, sp_runtime::Perbill>;
    type DataProvider = Staking;
    type WeightInfo = ();
    type MaxWinnersPerPage = ConstU32<100>;
    type MaxBackersPerWinner = ConstU32<100>;
    type Sort = ConstBool<true>;
    type Bounds = ElectionsBounds;
}

/// Staking benchmarking config.
/// Staking 基准测试配置。
pub struct StakingBenchmarkingConfig;
impl pallet_staking::BenchmarkingConfig for StakingBenchmarkingConfig {
    type MaxValidators = ConstU32<100>;
    type MaxNominators = ConstU32<100>;
}

#[derive_impl(pallet_staking::config_preludes::TestDefaultConfig)]
impl pallet_staking::Config for Runtime {
    type OldCurrency = Balances;
    type Currency = Balances;
    type CurrencyBalance = Balance;
    type CurrencyToVote = sp_staking::currency_to_vote::U128CurrencyToVote;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeHoldReason = RuntimeHoldReason;
    type UnixTime = Timestamp;
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = EnsureRoot<AccountId>;
    type SessionInterface = Self;
    type EraPayout = FixedAnnualInflation;
    type RewardRemainder = InflationSplitter;
    type NextNewSession = Session;
    type ElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GenesisElectionProvider = Self::ElectionProvider;
    type VoterList = pallet_staking::UseNominatorsAndValidatorsMap<Self>;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type MaxExposurePageSize = ConstU32<256>;
    type MaxValidatorSet = MaxAuthorities;
    type BenchmarkingConfig = StakingBenchmarkingConfig;
    type WeightInfo = ();
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = Babe;
    type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
    type WeightInfo = ();
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = DustRemovalAdapter;
    type ExistentialDeposit = ConstU128<EXISTENTIAL_DEPOSIT>;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const TargetBlockFullness: sp_runtime::Perquintill = sp_runtime::Perquintill::from_percent(25);
    pub AdjustmentVariable: Multiplier = Multiplier::saturating_from_rational(3, 100_000);
    pub MinimumMultiplier: Multiplier = Multiplier::saturating_from_rational(1, 1_000_000u128);
    pub MaximumMultiplier: Multiplier = Multiplier::saturating_from_integer(10);
}

pub type SlowAdjustingFeeUpdate<R> = TargetedFeeAdjustment<
    R,
    TargetBlockFullness,
    AdjustmentVariable,
    MinimumMultiplier,
    MaximumMultiplier,
>;

/// Dust removal handler — route dust to treasury instead of destroying it
pub struct DustRemovalAdapter;
impl frame_support::traits::OnUnbalanced<pallet_balances::CreditOf<Runtime, ()>>
    for DustRemovalAdapter
{
    fn on_nonzero_unbalanced(amount: pallet_balances::CreditOf<Runtime, ()>) {
        use frame_support::traits::fungible::Balanced;
        let _ = <Balances as Balanced<AccountId>>::resolve(&TreasuryAccountId::get(), amount);
    }
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = FungibleAdapter<Balances, DustRemovalAdapter>; // tips → treasury via DustRemovalAdapter
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Runtime>;
    type WeightInfo = pallet_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = pallet_sudo::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    pub storage PreimageBaseDeposit: Balance = 0;
    pub storage PreimageByteDeposit: Balance = 0;
    pub const PreimageHoldReason: RuntimeHoldReason = RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage);
    pub storage MaximumSchedulerWeight: Weight =
        Perbill::from_percent(80) * RuntimeBlockWeights::get().max_block;
}

impl pallet_preimage::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
    type Currency = Balances;
    type ManagerOrigin = RootOrTechnicalMajority;
    type Consideration = frame_support::traits::fungible::HoldConsideration<
        AccountId,
        Balances,
        PreimageHoldReason,
        LinearStoragePrice<PreimageBaseDeposit, PreimageByteDeposit, Balance>,
    >;
}

impl pallet_scheduler::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeOrigin = RuntimeOrigin;
    type PalletsOrigin = OriginCaller;
    type RuntimeCall = RuntimeCall;
    type MaximumWeight = MaximumSchedulerWeight;
    type ScheduleOrigin = RootOrTechnicalMajority;
    type OriginPrivilegeCmp = frame_support::traits::EqualPrivilegeOnly;
    type MaxScheduledPerBlock = ConstU32<50>;
    type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
    type Preimages = Preimage;
    type BlockNumberProvider = frame_system::Pallet<Self>;
}

/// Governance origin type aliases for replacing EnsureRoot post-Sudo removal
///
/// TechnicalCommittee 2/3 supermajority for system-level operations
pub type TechnicalMajority =
    pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollectiveInstance, 2, 3>;
/// TechnicalCommittee simple majority for lower-risk admin operations
pub type TechnicalSimpleMajority =
    pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollectiveInstance, 1, 2>;
/// Either Root (during Sudo transition) or Technical Committee 2/3
pub type RootOrTechnicalMajority =
    frame_support::traits::EitherOf<EnsureRoot<AccountId>, TechnicalMajority>;

// -------------------- 全局系统账户（简化方案：4 个核心账户）--------------------

parameter_types! {
    // 1. 国库账户 - 核心账户，含平台收入、存储补贴
    pub const TreasuryPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/trsry");
    pub TreasuryAccountId: AccountId = TreasuryPalletId::get().into_account_truncating();

    // 2. 销毁账户 - 专用于代币销毁，必须独立
    pub const BurnPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/burn!");
    pub BurnAccountId: AccountId = BurnPalletId::get().into_account_truncating();

    // 3. 奖励池账户 - 节点奖励资金池 (通胀分成 5% + 广告收入 + 订阅费 fallback)
    pub const RewardPoolPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/rwdpl");
    pub RewardPoolAccountId: AccountId = RewardPoolPalletId::get().into_account_truncating();

}

// ============================================================================
// 随机数生成器
// ============================================================================

/// 安全随机数生成器 - 基于 Collective Coin Flipping 机制
///
/// 原理：
/// - 结合多个历史区块哈希（81个区块，对应九宫格 9x9）
/// - 混合当前区块信息和用户提供的 subject
/// - 使用 blake2_256 进行哈希混合
pub struct CollectiveFlipRandomness;

impl frame_support::traits::Randomness<Hash, BlockNumber> for CollectiveFlipRandomness {
    fn random(subject: &[u8]) -> (Hash, BlockNumber) {
        let block_number = System::block_number();

        // 收集最近 81 个区块的哈希
        let mut combined_entropy = alloc::vec::Vec::with_capacity(81 * 32 + subject.len() + 8);

        // 添加 subject 作为熵源
        combined_entropy.extend_from_slice(subject);

        // 添加当前区块号
        combined_entropy.extend_from_slice(&block_number.to_le_bytes());

        // 收集历史区块哈希
        let blocks_to_collect = core::cmp::min(block_number.saturating_sub(1), 81);
        for i in 1..=blocks_to_collect {
            let hash = System::block_hash(block_number.saturating_sub(i as u32));
            combined_entropy.extend_from_slice(hash.as_ref());
        }

        // 添加父区块哈希作为额外熵源
        let parent_hash = System::parent_hash();
        combined_entropy.extend_from_slice(parent_hash.as_ref());

        // 使用 blake2_256 生成最终随机值
        let final_hash = sp_core::hashing::blake2_256(&combined_entropy);

        (Hash::from_slice(&final_hash), block_number)
    }
}

// ============================================================================
// Common Utilities
// ============================================================================

/// 时间戳提供器 - 使用 pallet_timestamp
pub struct TimestampProvider;

impl frame_support::traits::UnixTime for TimestampProvider {
    fn now() -> core::time::Duration {
        let millis = pallet_timestamp::Pallet::<Runtime>::get();
        core::time::Duration::from_millis(millis)
    }
}

// ============================================================================
// Trading Pallets Configuration
// ============================================================================

/// Pricing Provider 实现 — 基于 pallet-nex-market 的 TWAP/LastTradePrice
///
/// 价格优先级：1h TWAP（抗操纵） → LastTradePrice → initial_price（治理设定）
pub struct TradingPricingProvider;

impl pallet_trading_common::PricingProvider<Balance> for TradingPricingProvider {
    fn get_nex_to_usd_rate() -> Option<Balance> {
        // 优先: 1h TWAP（抗操纵，平滑单笔极端成交）
        // 每级用 .filter(|&p| p > 0) 过滤零值，确保 Some(0) 也触发 fallback
        pallet_nex_market::Pallet::<Runtime>::calculate_twap(
            pallet_nex_market::pallet::TwapPeriod::OneHour,
        )
        .filter(|&p| p > 0)
        // 回退: 最近成交价（TWAP 数据不足时）
        .or_else(|| pallet_nex_market::pallet::LastTradePrice::<Runtime>::get().filter(|&p| p > 0))
        // 兜底: 治理设定的初始价格（冷启动期）
        .or_else(|| {
            pallet_nex_market::pallet::PriceProtectionStore::<Runtime>::get()
                .and_then(|config| config.initial_price)
                .filter(|&p| p > 0)
        })
        .map(|price_u64| price_u64 as Balance)
    }

    fn report_p2p_trade(
        _timestamp: u64,
        _price_usdt: u64,
        _nex_qty: u128,
    ) -> sp_runtime::DispatchResult {
        // nex-market 内部自行更新 TWAP，无需外部上报
        Ok(())
    }
}

/// 统一兑换比率接口实现 — 聚合 TWAP + 陈旧检测 + 置信度评估
pub struct NexExchangeRateProvider;

impl pallet_trading_common::ExchangeRateProvider for NexExchangeRateProvider {
    fn get_nex_usdt_rate() -> Option<u64> {
        // 与 TradingPricingProvider 相同优先级
        <TradingPricingProvider as pallet_trading_common::PricingProvider<Balance>>::get_nex_to_usd_rate()
			.map(|rate| rate as u64)
    }

    fn price_confidence() -> u8 {
        use pallet_trading_common::PriceOracle;
        type Oracle = pallet_nex_market::Pallet<Runtime>;

        // 1. 无任何价格数据
        let rate = <TradingPricingProvider as pallet_trading_common::PricingProvider<Balance>>::get_nex_to_usd_rate();
        if rate.is_none() {
            return 0;
        }

        // 2. 检查数据新鲜度（超过 4h 视为过时）
        let stale = Oracle::is_price_stale(2400);
        let trade_count = Oracle::get_trade_count();

        // 3. 判断数据来源层级
        let has_twap = pallet_nex_market::Pallet::<Runtime>::calculate_twap(
            pallet_nex_market::pallet::TwapPeriod::OneHour,
        )
        .is_some();
        let has_last_trade = pallet_nex_market::pallet::LastTradePrice::<Runtime>::get().is_some();

        if stale {
            // 价格过时：低置信度
            if has_twap {
                25
            } else if has_last_trade {
                15
            } else {
                10
            }
        } else if has_twap && trade_count >= 100 {
            // TWAP 可用 + 高交易量：最高置信度
            95
        } else if has_twap {
            // TWAP 可用但交易量低
            80
        } else if has_last_trade {
            // 仅 LastTradePrice（TWAP 数据不足）
            65
        } else {
            // 仅 initial_price（冷启动期）
            35
        }
    }
}

// -------------------- NEX Market (NEX/USDT 订单簿) --------------------

parameter_types! {
    pub NexMarketTreasuryAccount: AccountId = frame_support::PalletId(*b"nxm/trsy").into_account_truncating();
    pub NexMarketSeedAccount: AccountId = frame_support::PalletId(*b"nxm/seed").into_account_truncating();
    pub NexMarketRewardSource: AccountId = frame_support::PalletId(*b"nxm/rwds").into_account_truncating();
    pub const NexMarketSeedTronAddr: [u8; 34] = *b"T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWb1";
}

impl pallet_nex_market::Config for Runtime {
    type Currency = Balances;
    type WeightInfo = pallet_nex_market::weights::SubstrateWeight<Runtime>;
    type DefaultOrderTTL = ConstU32<{ 24 * HOURS }>;
    type MaxActiveOrdersPerUser = ConstU32<100>;
    type UsdtTimeout = ConstU32<{ 12 * HOURS }>;
    type BlocksPerHour = ConstU32<{ 1 * HOURS }>;
    type BlocksPerDay = ConstU32<{ 24 * HOURS }>;
    type BlocksPerWeek = ConstU32<{ 7 * DAYS }>;
    type CircuitBreakerDuration = ConstU32<{ 1 * HOURS }>;
    type VerificationReward = ConstU128<{ UNIT / 10 }>; // 0.1 NEX
    type RewardSource = NexMarketRewardSource;
    type BuyerDepositRate = ConstU16<1000>; // 10%（折扣后最低 5%）
    type MinBuyerDepositUsd = ConstU64<300_000>; // 0.3 USDT（1 USDT × 30%）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type DepositForfeitRate = ConstU16<10000>; // 100%
    type TreasuryAccount = NexMarketTreasuryAccount;
    type SeedLiquidityAccount = NexMarketSeedAccount;
    type MarketAdminOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, TreasuryCollectiveInstance, 2, 3>;
    type FirstOrderTimeout = ConstU32<{ 1 * HOURS }>; // 免保证金短超时 1h
    type MaxFirstOrderAmount = ConstU128<{ 100 * UNIT }>; // 免保证金单笔上限 100 NEX
    type MaxWaivedSeedOrders = ConstU32<20>; // seed_liquidity 单次最多 20 笔
    type SeedPricePremiumBps = ConstU16<4000>; // seed 溢价 40%
    type SeedOrderUsdtAmount = ConstU64<10_000_000>; // 固定 10 USDT/笔
    type SeedTronAddress = NexMarketSeedTronAddr;
    type VerificationGracePeriod = ConstU32<{ 1 * HOURS }>; // 1h 宽限期
    type UnderpaidGracePeriod = ConstU32<{ 2 * HOURS }>; // 2h 补付窗口
    type DepositPenaltyGracePeriod = ConstU32<{ 2 * HOURS }>; // 2h 罚金宽限期
    type DepositPenaltyRatePerHour = ConstU16<500>; // 5%/h
    type MaxPendingTrades = ConstU32<200>;
    type MaxAwaitingPaymentTrades = ConstU32<200>;
    type MaxUnderpaidTrades = ConstU32<100>;
    type MaxExpiredOrdersPerBlock = ConstU32<20>;
    type TxHashTtlBlocks = ConstU32<{ 7 * DAYS }>; // 🆕 M7: tx_hash 保留 7 天
    type MinTxHashTtlBlocks = ConstU32<{ 6 * DAYS }>; // H-1: 最小安全值 6 天 (72h lookback × 2)
    type MinOrderNexAmount = ConstU128<{ 1_000 * UNIT }>; // 最低 1000 NEX
    type MaxTradesPerUser = ConstU32<500>;
    type MaxOrderTrades = ConstU32<100>;
    type QueueFullThresholdBps = ConstU16<8000>; // 80% 自动暂停
    type DisputeWindowBlocks = ConstU32<{ 7 * DAYS }>; // 争议窗口 ~7 天
    type MaxOrderNexAmount = ConstU128<{ 100_000_000 * UNIT }>; // 单笔最大 100,000,000 NEX
    type MaxSellOrders = ConstU32<1000>; // 卖单簿最大容量
    type MaxBuyOrders = ConstU32<1000>; // 买单簿最大容量
    type MinIndexerStake = ConstU128<{ 1_000 * UNIT }>; // 最低 1000 NEX 质押
    type MaxIndexers = ConstU32<50>; // 最多 50 个 Indexer
    type IndexerGracePeriod = ConstU32<{ 5 * MINUTES }>; // 5 分钟宽限期
    type IndexerHintReward = ConstU128<{ UNIT / 100 }>; // 0.01 NEX 奖励
    type MaxIndexerErrors = ConstU32<10>; // 10 次错误自动暂停
    type IndexerSlashRateBps = ConstU16<3000>; // 暂停时 slash 30% 质押
    type MaxPenaltyTradesPerBlock = ConstU32<20>; // on_idle 每区块最多处理 20 笔罚金
}

// ============================================================================
// Escrow, Referral, IPFS Pallets Configuration
// ============================================================================

// -------------------- Escrow (托管) --------------------

parameter_types! {
    pub const EscrowPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/escro");
}

/// 托管过期策略：默认退款给原始付款人（安全兜底）
pub struct DefaultExpiryPolicy;

impl pallet_dispute_escrow::ExpiryPolicy<AccountId, BlockNumber> for DefaultExpiryPolicy {
    fn on_expire(
        id: u64,
    ) -> Result<pallet_dispute_escrow::ExpiryAction<AccountId>, sp_runtime::DispatchError> {
        match pallet_dispute_escrow::PayerOf::<Runtime>::get(id) {
            Some(payer) => Ok(pallet_dispute_escrow::ExpiryAction::RefundAll(payer)),
            None => Ok(pallet_dispute_escrow::ExpiryAction::Noop),
        }
    }
}

impl pallet_dispute_escrow::Config for Runtime {
    type Currency = Balances;
    type EscrowPalletId = EscrowPalletId;
    type AuthorizedOrigin = RootOrTechnicalMajority;
    type AdminOrigin = RootOrTechnicalMajority;
    type MaxExpiringPerBlock = ConstU32<100>;
    type MaxSplitEntries = ConstU32<20>;
    type ExpiryPolicy = DefaultExpiryPolicy;
    /// 🆕 F5: 争议原因最大长度（256 字节，可容纳 CID 或简短描述）
    type MaxReasonLen = ConstU32<256>;
    /// 🆕 F10: 托管状态变更观察者（暂用空实现，待业务模块集成后替换）
    type Observer = ();
    type MaxCleanupPerCall = ConstU32<100>;
    /// 争议超时 100800 块 (≈7天 @ 6s/block)
    type MaxDisputeDuration = ConstU32<100800>;
    type WeightInfo = pallet_dispute_escrow::weights::SubstrateWeight<Runtime>;
}

// -------------------- Storage Service (存储服务) --------------------

parameter_types! {
    // 3. 存储服务主账户 - 核心账户，含费用收集
    pub const StorageServicePalletId: frame_support::PalletId = frame_support::PalletId(*b"py/storg");
    pub StoragePoolAccountId: AccountId = StorageServicePalletId::get().into_account_truncating();

    // 4. 运营商托管账户 - 必须独立
    pub OperatorEscrowAccountId: AccountId = StorageServicePalletId::get().into_sub_account_truncating(b"escrow");
}

// OCW unsigned transaction support for pallet-storage-service
impl frame_system::offchain::CreateTransactionBase<pallet_storage_service::Call<Runtime>>
    for Runtime
{
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_storage_service::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

impl frame_system::offchain::CreateTransactionBase<pallet_grandpa::Call<Runtime>> for Runtime {
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_grandpa::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

// OCW unsigned transaction support for pallet-babe (equivocation reporting)
impl frame_system::offchain::CreateTransactionBase<pallet_babe::Call<Runtime>> for Runtime {
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_babe::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

// OCW unsigned transaction support for pallet-nex-market
impl frame_system::offchain::CreateTransactionBase<pallet_nex_market::pallet::Call<Runtime>>
    for Runtime
{
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_nex_market::pallet::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

// OCW unsigned transaction support for pallet-im-online (heartbeat)
impl frame_system::offchain::CreateTransactionBase<pallet_im_online::Call<Runtime>> for Runtime {
    type Extrinsic = UncheckedExtrinsic;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_im_online::Call<Runtime>> for Runtime {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        generic::UncheckedExtrinsic::new_bare(call)
    }
}

impl pallet_storage_service::Config for Runtime {
    type Currency = Balances;
    type Balance = Balance;
    type FeeCollector = StoragePoolAccountId;
    // 内容委员会 1/2 多数通过（P0 治理集成）
    type GovernanceOrigin = pallet_collective::EnsureProportionAtLeast<
        AccountId,
        ContentCollectiveInstance,
        1,
        2, // 1/2 多数通过
    >;
    type MaxCidHashLen = ConstU32<64>;
    type MaxPeerIdLen = ConstU32<128>;
    type MinOperatorBond = ConstU128<{ 100 * UNIT }>;
    type MinOperatorBondUsd = ConstU64<100_000_000>; // 100 USDT
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type MinCapacityGiB = ConstU32<10>;
    type WeightInfo = pallet_storage_service::weights::SubstrateWeight<Runtime>;
    type SubjectPalletId = StorageServicePalletId;
    type IpfsPoolAccount = StoragePoolAccountId;
    type OperatorEscrowAccount = OperatorEscrowAccountId;
    type MonthlyPublicFeeQuota = ConstU128<{ 10 * UNIT }>;
    type QuotaResetPeriod = ConstU32<{ 30 * DAYS }>;
    type DefaultBillingPeriod = ConstU32<{ 30 * DAYS }>;
    type OperatorGracePeriod = ConstU32<{ 7 * DAYS }>;
    type EntityFunding = EntityRegistry;
}

// -------------------- Evidence (证据存证) --------------------

parameter_types! {
    pub const EvidenceNsBytes: [u8; 8] = *b"evidence";
}

/// 证据授权适配器 — 争议当事人可提交证据
pub struct DisputePartyAuthorizedEvidence;

impl pallet_dispute_evidence::pallet::EvidenceAuthorizer<AccountId>
    for DisputePartyAuthorizedEvidence
{
    fn is_authorized(_ns: [u8; 8], who: &AccountId) -> bool {
        for (_key, complaint) in pallet_dispute_arbitration::pallet::Complaints::<Runtime>::iter() {
            if &complaint.complainant == who || &complaint.respondent == who {
                return true;
            }
        }
        if pallet_collective::Members::<Runtime, ArbitrationCollectiveInstance>::get().contains(who)
        {
            return true;
        }
        false
    }
}

/// 证据密封授权 — 仅限仲裁委员会成员
pub struct ArbitrationCommitteeSealAuthorizer;

impl pallet_dispute_evidence::pallet::EvidenceSealAuthorizer<AccountId>
    for ArbitrationCommitteeSealAuthorizer
{
    fn can_seal(_ns: [u8; 8], who: &AccountId) -> bool {
        pallet_collective::Members::<Runtime, ArbitrationCollectiveInstance>::get().contains(who)
    }
}

impl pallet_dispute_evidence::Config for Runtime {
    type MaxContentCidLen = ConstU32<64>;
    type MaxSchemeLen = ConstU32<32>;
    type MaxCidLen = ConstU32<64>;
    type MaxImg = ConstU32<20>;
    type MaxVid = ConstU32<10>;
    type MaxDoc = ConstU32<20>;
    type MaxMemoLen = ConstU32<512>;
    type MaxAuthorizedUsers = ConstU32<50>;
    type MaxKeyLen = ConstU32<512>;
    type EvidenceNsBytes = EvidenceNsBytes;
    type Authorizer = DisputePartyAuthorizedEvidence;
    type SealAuthorizer = ArbitrationCommitteeSealAuthorizer;
    type MaxPerSubjectTarget = ConstU32<1000>;
    type MaxPerSubjectNs = ConstU32<1000>;
    type WindowBlocks = ConstU32<{ 10 * MINUTES }>;
    type MaxPerWindow = ConstU32<100>;
    type EnableGlobalCidDedup = ConstBool<true>;
    type MaxListLen = ConstU32<100>;
    type WeightInfo = pallet_dispute_evidence::weights::SubstrateWeight<Runtime>;
    type StoragePin = pallet_storage_service::Pallet<Runtime>;
    type Currency = Balances;
    type EvidenceDeposit = ConstU128<{ UNIT / 100 }>; // 0.01 NEX 兜底（仅防零）
    type EvidenceDepositUsd = ConstU64<150_000>; // 0.15 USDT（0.5 USDT × 30%）
    type CommitRevealDeadline = ConstU32<100_800>; // ~7天 (6s/block)
    type MaxLinksPerEvidence = ConstU32<100>;
    type MaxSupplements = ConstU32<100>;
    type MaxPendingRequestsPerContent = ConstU32<50>;
    type ArchiveTtlBlocks = ConstU32<2_592_000>; // ~180天
    type ArchiveDelayBlocks = ConstU32<1_296_000>; // ~90天
    type PrivateContentDeposit = ConstU128<{ UNIT / 100 }>; // 0.01 NEX 兜底（仅防零）
    type PrivateContentDepositUsd = ConstU64<150_000>; // 0.15 USDT（0.5 USDT × 30%）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type AccessRequestTtlBlocks = ConstU32<201_600>; // ~14天 (6s/block)
    type GovernanceOrigin = RootOrTechnicalMajority;
    type MaxReasonLen = ConstU32<256>;
}

// -------------------- Arbitration (仲裁) --------------------

/// 商城订单域标识（8字节）
const DOMAIN_ENTITY_ORDER: [u8; 8] = *b"entorder";

/// 统一仲裁域路由器
///
/// 将仲裁决议路由到各业务模块执行
/// 当前支持：`entorder`（商城订单）
pub struct UnifiedArbitrationRouter;

impl pallet_dispute_arbitration::pallet::ArbitrationRouter<AccountId, Balance>
    for UnifiedArbitrationRouter
{
    /// 校验是否允许发起争议
    fn can_dispute(domain: [u8; 8], who: &AccountId, id: u64) -> bool {
        use pallet_entity_common::OrderProvider;
        if domain == DOMAIN_ENTITY_ORDER {
            <EntityTransaction as OrderProvider<AccountId, Balance>>::can_dispute(id, who)
        } else {
            false
        }
    }

    /// 应用裁决（放款/退款/部分分账）
    fn apply_decision(
        domain: [u8; 8],
        id: u64,
        decision: pallet_dispute_arbitration::pallet::Decision,
    ) -> sp_runtime::DispatchResult {
        use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
        use pallet_entity_common::OrderProvider;
        if domain == DOMAIN_ENTITY_ORDER {
            let buyer = <EntityTransaction as OrderProvider<AccountId, Balance>>::order_buyer(id)
                .ok_or(sp_runtime::DispatchError::Other("order not found"))?;
            let seller = <EntityTransaction as OrderProvider<AccountId, Balance>>::order_seller(id)
                .ok_or(sp_runtime::DispatchError::Other("order not found"))?;

            // 解除争议锁定
            let _ = <Escrow as EscrowTrait<AccountId, Balance>>::set_resolved(id);

            match decision {
                pallet_dispute_arbitration::pallet::Decision::Release => {
                    // 卖家胜诉：释放给卖家
                    <Escrow as EscrowTrait<AccountId, Balance>>::release_all(id, &seller)
                }
                pallet_dispute_arbitration::pallet::Decision::Refund => {
                    // 买家胜诉：退款给买家
                    <Escrow as EscrowTrait<AccountId, Balance>>::refund_all(id, &buyer)
                }
                pallet_dispute_arbitration::pallet::Decision::Partial(bps) => {
                    // 部分裁决：按比例分账（bps/10000 给卖家，剩余退买家）
                    <Escrow as EscrowTrait<AccountId, Balance>>::split_partial(
                        id, &seller, &buyer, bps,
                    )
                }
            }
        } else {
            Ok(())
        }
    }

    /// 获取纠纷对方账户
    fn get_counterparty(
        domain: [u8; 8],
        initiator: &AccountId,
        id: u64,
    ) -> Result<AccountId, sp_runtime::DispatchError> {
        use pallet_entity_common::OrderProvider;
        if domain == DOMAIN_ENTITY_ORDER {
            let buyer = <EntityTransaction as OrderProvider<AccountId, Balance>>::order_buyer(id)
                .ok_or(sp_runtime::DispatchError::Other("order not found"))?;
            let seller = <EntityTransaction as OrderProvider<AccountId, Balance>>::order_seller(id)
                .ok_or(sp_runtime::DispatchError::Other("order not found"))?;
            // 如果发起人是买家，对方是卖家；反之亦然
            if *initiator == buyer {
                Ok(seller)
            } else if *initiator == seller {
                Ok(buyer)
            } else {
                Err(sp_runtime::DispatchError::Other("not a party"))
            }
        } else {
            Ok(TreasuryAccountId::get())
        }
    }

    /// 获取订单/交易金额（用于计算押金）
    fn get_order_amount(domain: [u8; 8], id: u64) -> Result<Balance, sp_runtime::DispatchError> {
        use pallet_entity_common::OrderProvider;
        if domain == DOMAIN_ENTITY_ORDER {
            <EntityTransaction as OrderProvider<AccountId, Balance>>::order_amount(id)
                .ok_or(sp_runtime::DispatchError::Other("order not found"))
        } else {
            Ok(10 * UNIT)
        }
    }
}

/// 🆕 M-NEW-6修复: 证据存在性检查器（委托给 pallet-dispute-evidence 存储）
pub struct EvidenceExistenceCheckerImpl;

impl pallet_dispute_arbitration::pallet::EvidenceExistenceChecker for EvidenceExistenceCheckerImpl {
    fn evidence_exists(id: u64) -> bool {
        pallet_dispute_evidence::pallet::Evidences::<Runtime>::contains_key(id)
    }
}

/// Bridge arbitration notifications → chat-core System channel (audit 2.3).
/// Best-effort: errors swallowed. / 仲裁系统通知桥接到 chat-core；吞错。
pub struct ArbitrationChatNotifier;
impl pallet_dispute_arbitration::ArbitrationNotifier<AccountId> for ArbitrationChatNotifier {
    fn notify(to: &AccountId, notice: alloc::vec::Vec<u8>) {
        ChatCore::notify_system_best_effort(to, notice);
    }
}

impl pallet_dispute_arbitration::pallet::Config for Runtime {
    type MaxEvidence = ConstU32<20>;
    type MaxCidLen = ConstU32<64>;
    type Escrow = pallet_dispute_escrow::Pallet<Runtime>;
    type WeightInfo = pallet_dispute_arbitration::weights::SubstrateWeight<Runtime>;
    type Router = UnifiedArbitrationRouter;
    type DecisionOrigin =
        pallet_collective::EnsureProportionAtLeast<AccountId, ArbitrationCollectiveInstance, 2, 3>;
    type Fungible = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type DepositRatioBps = ConstU16<1500>; // 15% 押金比例
    type ResponseDeadline = ConstU32<{ 7 * DAYS }>; // 7天应诉期限
    type RejectedSlashBps = ConstU16<3000>; // 驳回时罚没30%
    type PartialSlashBps = ConstU16<5000>; // 部分胜诉罚没50%
    type ComplaintDeposit = ConstU128<{ UNIT / 10 }>; // 投诉押金兜底值 0.1 NEX
    type ComplaintDepositUsd = ConstU64<1_000_000>; // 投诉押金 1 USDT（精度10^6，使用pricing换算）
    type Pricing = TradingPricingProvider; // 定价接口
    type ComplaintSlashBps = ConstU16<5000>; // 投诉败诉罚没50%
    type TreasuryAccount = TreasuryAccountId;
    type CidLockManager = pallet_storage_service::Pallet<Runtime>;
    type StoragePin = pallet_storage_service::Pallet<Runtime>;
    type ArchiveTtlBlocks = ConstU32<2_592_000>;
    type ComplaintArchiveDelayBlocks = ConstU32<432_000>;
    type ComplaintMaxLifetimeBlocks = ConstU32<1_296_000>; // 90 days
    type EvidenceExists = EvidenceExistenceCheckerImpl;
    type AppealWindowBlocks = ConstU32<{ 3 * DAYS }>; // 3 days appeal window
    type AutoEscalateBlocks = ConstU32<{ 14 * DAYS }>; // 14 days auto-escalation
    type MaxActivePerUser = ConstU32<50>;
    type Notifier = ArbitrationChatNotifier;
}

// -------------------- Chat (聊天系统) --------------------

impl pallet_chat_permission::Config for Runtime {
    // 审计 P1：链上黑/白名单已移除（去明文，拉黑/放行改走链下能力令牌），
    // 故不再配置 MaxBlockListSize / MaxWhitelistSize。
    // Audit P1: on-chain block/whitelist removed (off-chain capability tokens).
    // 单对用户最大并存场景授权数（多订单 / 多悬赏 / 群聊等共存）。
    type MaxScenesPerPair = ConstU32<64>;
    // 平台合规：Root 或技术委员会多数可禁言账号、处理举报。
    // Compliance: Root or technical-committee majority can mute / resolve reports.
    type GovernanceOrigin = RootOrTechnicalMajority;
    // 举报证据 CID 字节上限 / report evidence CID byte cap.
    type MaxReportCidLen = ConstU32<128>;
    // 链上未处理举报硬上限 / on-chain open-report hard cap.
    type MaxOpenReports = ConstU32<10_000>;
    // 举报冷却 ~1 分钟 / report cooldown ~1 minute.
    type ReportCooldown = ConstU32<MINUTES>;
    type WeightInfo = pallet_chat_permission::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// EN: Account recorded as the `sender` for governance-issued on-chain chat
    /// `System` messages. Derived from a PalletId so it is deterministic and not
    /// controlled by any user key. CN: 治理签发的链上聊天 `System` 消息所记录的
    /// `sender` 账户；由 PalletId 派生，确定且不受任何用户密钥控制。
    pub ChatSystemMessenger: AccountId =
        frame_support::PalletId(*b"chat/sys").into_account_truncating();
}

impl pallet_chat_core::Config for Runtime {
    type WeightInfo = pallet_chat_core::SubstrateWeight<Runtime>;
    type MaxCidLen = ConstU32<96>; // 加密 IPFS CID
    type MessageExpirationTime = ConstU32<{ 180 * DAYS }>;
    // 撤回时间窗口：2 分钟（对齐常见 IM）。/ recall window: ~2 minutes.
    type MessageRecallWindow = ConstU32<{ 2 * MINUTES }>;
    type Randomness = CollectiveFlipRandomness;
    type UnixTime = TimestampProvider;
    type MaxNicknameLength = ConstU32<64>;
    type MaxSignatureLength = ConstU32<256>;
    // 发送前权限校验：场景授权 / 好友 / 黑白名单 / 隐私级别。
    type ChatPermission = ChatPermission;
    // System 通道仅对治理（Root）开放，sender 记为派生系统账户（审计 B2）：
    // 防止任意用户伪造系统通知。人类聊天走链下 MLS，不经此入口。
    // System channel is governance-only (Root), sender = derived system account
    // (audit B2): prevents forged system notifications. Human chat is off-chain.
    type SystemMessageOrigin =
        frame_system::EnsureRootWithSuccess<AccountId, ChatSystemMessenger>;
    // 程序化系统通知（SystemNotifier::notify）的发信账户，复用同一派生系统账户，
    // 使 extrinsic 路径与 trait 路径落同一 sender（审计 2.3 接线）。
    // Same derived system account for the programmatic notify path (audit 2.3),
    // so trait and extrinsic paths stamp one platform sender.
    type SystemAccount = ChatSystemMessenger;
}

/// EN: Wire platform mute from chat-permission into group MLS write extrinsics.
/// CN: 将 chat-permission 的平台禁言接入群 MLS 写入 extrinsic。
pub struct GroupPlatformMuteCheck;
impl pallet_chat_group::PlatformMuteChecker<AccountId> for GroupPlatformMuteCheck {
    fn is_platform_muted(who: &AccountId) -> bool {
        ChatPermission::is_account_muted(who)
    }
}

/// Adapter mapping `pallet-chat-group` membership hooks to `chat-permission`
/// scene authorizations (`source = *b"chatgrp "`, `SceneType::Group`,
/// `scene_id = Numeric(group_id)`). Each member is related to the group owner so
/// they gain optional 1:1 DM rights (O(1) per event, not the O(N²) full mesh).
/// Errors are swallowed so group lifecycle ops never abort on chat wiring.
///
/// 将群成员钩子映射为 chat-permission 场景授权（成员↔群主），获得可选 1:1 私聊权限；
/// 每事件 O(1)，非 O(N²) 全网格。吞错以免聊天接线中断群生命周期操作。
pub struct GroupChatAuthorizer;
impl pallet_chat_group::GroupChatHook<AccountId> for GroupChatAuthorizer {
    fn on_member_added(group_id: u64, member: &AccountId, counterparty: &AccountId) {
        ChatPermission::grant_scene_from_hook(
            *b"chatgrp ",
            member,
            counterparty,
            pallet_chat_permission::SceneType::Group,
            pallet_chat_permission::SceneId::Numeric(group_id),
            None,
            Default::default(),
        );
    }
    fn on_member_removed(group_id: u64, member: &AccountId, counterparty: &AccountId) {
        ChatPermission::revoke_scene_from_hook(
            *b"chatgrp ",
            member,
            counterparty,
            pallet_chat_permission::SceneType::Group,
            pallet_chat_permission::SceneId::Numeric(group_id),
        );
    }
}

parameter_types! {
    /// 建群押金 / group creation deposit (100_000 NEX)
    pub const GroupDeposit: Balance = 100_000 * UNIT;
    /// KeyPackage 押金 / per-KeyPackage deposit (0.1 NEX)
    pub const KeyPackageDeposit: Balance = UNIT / 10;
}

impl pallet_chat_group::Config for Runtime {
    type Currency = Balances;
    type GroupDeposit = GroupDeposit;
    type KeyPackageDeposit = KeyPackageDeposit;
    type MaxPendingJoins = ConstU32<256>;
    // 成员↔群主场景授权（可选 1:1 私聊）/ member↔owner scene auth
    type ChatHook = GroupChatAuthorizer;
    type PlatformMuteCheck = GroupPlatformMuteCheck;
    type MaxGroupMembers = ConstU32<500>; // 单群最大成员数 / max members per group
    type MaxGroupsPerUser = ConstU32<500>;
    type MaxKeyPackageLen = ConstU32<4096>;
    type MaxHandshakeLen = ConstU32<16384>;
    type MaxWelcomeLen = ConstU32<8192>;
    type MaxCidLen = ConstU32<96>;
    type MaxGroupNameLen = ConstU32<128>;
    type MaxGroupAnnouncementLen = ConstU32<2048>;
    type MaxGroupNicknameLen = ConstU32<64>;
    type MaxKeyPackagesPerUser = ConstU32<16>;
    type GroupCreationCooldown = ConstU32<{ 10 * MINUTES }>;
    // 写入型 MLS 操作（commit/anchor）每账户每分钟最多 30 次，防 HandshakeLog/锚点滥用。
    // Write-heavy MLS actions: ≤30 per account per minute (anti-spam for logs/anchors).
    type MlsActionWindow = ConstU32<MINUTES>;
    type MaxMlsActionsPerWindow = ConstU32<30>;
    // 入群申请：单账户每小时最多 20 次（跨群合计），防刷 PendingJoinCount。
    // Join requests: ≤20 per account per hour (across all groups).
    type JoinRequestWindow = ConstU32<{ 60 * MINUTES }>;
    type MaxJoinRequestsPerWindow = ConstU32<20>;
    // 平台合规：Root 或技术委员会多数可强制解散/冻结群。
    // Compliance: Root or technical-committee majority can force-disband/freeze.
    type GovernanceOrigin = RootOrTechnicalMajority;
    type WeightInfo = pallet_chat_group::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// 投递信箱注册押金 / delivery-inbox registration deposit (0.5 NEX)
    pub const InboxDeposit: Balance = UNIT / 2;
}

// 链下投递信箱注册表（盲化一次性投递令牌的链上锚点）。
// Off-chain delivery inbox registry (on-chain anchor for blinded one-time tokens).
impl pallet_chat_inbox::Config for Runtime {
    type Currency = Balances;
    // 治理回收：Root / 技术委员会多数可在 controller 丢钥时强制注销并退押金。
    // Governance recovery: Root / technical-committee majority may force-deregister
    // (and refund) an inbox if the controller key is lost.
    type ForceOrigin = RootOrTechnicalMajority;
    type InboxDeposit = InboxDeposit;
    // 每信箱定向撤销标签上限（epoch 轮换清空）/ per-inbox revoked-tag cap (cleared on epoch bump).
    type MaxRevokedTags = ConstU32<256>;
    // 单控制账户信箱上限 / max inboxes per controller account.
    type MaxInboxesPerController = ConstU32<16>;
    type WeightInfo = pallet_chat_inbox::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// 设备身份押金 / per-device identity deposit（反垃圾；量级同 inbox 押金）。
    pub const DeviceDeposit: Balance = UNIT / 2;
}

// 消息身份预密钥锚（X3DH IK/SPK/OPK 根 + 1:1 栈能力位）。
// Messaging identity prekey anchor (X3DH IK/SPK/OPK root + 1:1 stack caps).
impl pallet_msg_identity::Config for Runtime {
    type Currency = Balances;
    // 治理回收：controller/设备密钥丢失或滥用时，Root / 技术委员会多数可强制注销并退押金。
    // Governance recovery: Root / technical-committee majority may force-unregister
    // (and refund) a device on key loss or abuse.
    type ForceOrigin = RootOrTechnicalMajority;
    type DeviceDeposit = DeviceDeposit;
    // 单账户设备上限（多设备 X3DH 场景）/ max devices per account (multi-device X3DH).
    type MaxDevicesPerAccount = ConstU32<16>;
    type WeightInfo = pallet_msg_identity::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
    /// 同步锚押金 / sync-anchor deposit（量级同 inbox 押金；终值待 benchmark + 经济评审，ADR §11.5）
    pub const AnchorDeposit: Balance = UNIT / 2;
}

// 账户派生加密同步锚（EISA）：AnchorId 键控密文清单 + 锚密钥 Ed25519 授权。
// Account-derived Encrypted Sync Anchor (EISA): AnchorId-keyed ciphertext manifest
// with anchor-key Ed25519 authorization (payer ≠ authorizer).
impl pallet_chat_sync::Config for Runtime {
    type Currency = Balances;
    // 密文清单上限：当前规范清单约 200B，留增长余量（ADR §5.4）。
    // Ciphertext cap: canonical manifest ≈200B today, headroom per ADR §5.4.
    type MaxAnchorLen = ConstU32<512>;
    // 链上硬顶 ~10min @ 6s；客户端长 debounce 取更严（ADR §11.2）。
    // On-chain hard cap ~10min @ 6s; client debounce is stricter (ADR §11.2).
    type MinBlocksBetweenPublish = ConstU32<100>;
    type AnchorDeposit = AnchorDeposit;
    // updated_at 上界容差 1h（防自锁，ADR §5.5 规则 5）。
    // 1h upper clock-skew tolerance (anti self-lock, ADR §5.5 rule 5).
    type MaxClockSkew = ConstU64<3_600_000>;
    // 治理逃生门（清理遗弃/滥用锚），与 inbox ForceOrigin 同口径。
    // Governance escape hatch (abandoned/abusive anchors), same origin as inbox.
    type ForceOrigin = RootOrTechnicalMajority;
    type WeightInfo = pallet_chat_sync::weights::SubstrateWeight<Runtime>;
}

// ============================================================================
// Governance: Collective (Committees) Configuration
// ============================================================================

// -------------------- 1. 技术委员会 (Technical Committee) --------------------
// 职责：紧急升级、runtime 参数调整、技术提案审核

pub type TechnicalCollectiveInstance = pallet_collective::Instance1;

parameter_types! {
    pub const TechnicalMotionDuration: BlockNumber = 7 * DAYS;
    pub const TechnicalMaxProposals: u32 = 100;
    pub const TechnicalMaxMembers: u32 = 11;
    pub MaxTechnicalProposalWeight: Weight = Perbill::from_percent(50) * RuntimeBlockWeights::get().max_block;
}

impl pallet_collective::Config<TechnicalCollectiveInstance> for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type Proposal = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type MotionDuration = TechnicalMotionDuration;
    type MaxProposals = TechnicalMaxProposals;
    type MaxMembers = TechnicalMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
    type SetMembersOrigin = RootOrTechnicalMajority;
    type MaxProposalWeight = MaxTechnicalProposalWeight;
    type DisapproveOrigin = RootOrTechnicalMajority;
    type KillOrigin = RootOrTechnicalMajority;
    type Consideration = ();
}

// -------------------- 2. 仲裁委员会 (Arbitration Committee) --------------------
// 职责：处理 OTC/Bridge/供奉订单的争议裁决

pub type ArbitrationCollectiveInstance = pallet_collective::Instance2;

parameter_types! {
    pub const ArbitrationMotionDuration: BlockNumber = 3 * DAYS;
    pub const ArbitrationMaxProposals: u32 = 200;
    pub const ArbitrationMaxMembers: u32 = 15;
    pub MaxArbitrationProposalWeight: Weight = Perbill::from_percent(50) * RuntimeBlockWeights::get().max_block;
}

impl pallet_collective::Config<ArbitrationCollectiveInstance> for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type Proposal = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type MotionDuration = ArbitrationMotionDuration;
    type MaxProposals = ArbitrationMaxProposals;
    type MaxMembers = ArbitrationMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
    type SetMembersOrigin = RootOrTechnicalMajority;
    type MaxProposalWeight = MaxArbitrationProposalWeight;
    type DisapproveOrigin = RootOrTechnicalMajority;
    type KillOrigin = RootOrTechnicalMajority;
    type Consideration = ();
}

// -------------------- 3. 财务委员会 (Treasury Council) --------------------
// 职责：审批国库支出、资金分配、生态激励

pub type TreasuryCollectiveInstance = pallet_collective::Instance3;

parameter_types! {
    pub const TreasuryMotionDuration: BlockNumber = 5 * DAYS;
    pub const TreasuryMaxProposals: u32 = 50;
    pub const TreasuryMaxMembers: u32 = 9;
    pub MaxTreasuryProposalWeight: Weight = Perbill::from_percent(50) * RuntimeBlockWeights::get().max_block;
}

impl pallet_collective::Config<TreasuryCollectiveInstance> for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type Proposal = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type MotionDuration = TreasuryMotionDuration;
    type MaxProposals = TreasuryMaxProposals;
    type MaxMembers = TreasuryMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
    type SetMembersOrigin = RootOrTechnicalMajority;
    type MaxProposalWeight = MaxTreasuryProposalWeight;
    type DisapproveOrigin = RootOrTechnicalMajority;
    type KillOrigin = RootOrTechnicalMajority;
    type Consideration = ();
}

// -------------------- 4. 内容委员会 (Content Committee) --------------------
// 职责：审核直播内容合规、证据真实性

pub type ContentCollectiveInstance = pallet_collective::Instance4;

parameter_types! {
    pub const ContentMotionDuration: BlockNumber = 2 * DAYS;
    pub const ContentMaxProposals: u32 = 100;
    pub const ContentMaxMembers: u32 = 7;
    pub MaxContentProposalWeight: Weight = Perbill::from_percent(50) * RuntimeBlockWeights::get().max_block;
}

impl pallet_collective::Config<ContentCollectiveInstance> for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type Proposal = RuntimeCall;
    type RuntimeEvent = RuntimeEvent;
    type MotionDuration = ContentMotionDuration;
    type MaxProposals = ContentMaxProposals;
    type MaxMembers = ContentMaxMembers;
    type DefaultVote = pallet_collective::PrimeDefaultVote;
    type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
    type SetMembersOrigin = RootOrTechnicalMajority;
    type MaxProposalWeight = MaxContentProposalWeight;
    type DisapproveOrigin = RootOrTechnicalMajority;
    type KillOrigin = RootOrTechnicalMajority;
    type Consideration = ();
}

// -------------------- Membership Pallets for Committees --------------------

// 技术委员会成员管理
pub type TechnicalMembershipInstance = pallet_collective_membership::Instance1;

impl pallet_collective_membership::Config<TechnicalMembershipInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddOrigin = RootOrTechnicalMajority;
    type RemoveOrigin = RootOrTechnicalMajority;
    type SwapOrigin = RootOrTechnicalMajority;
    type ResetOrigin = RootOrTechnicalMajority;
    type PrimeOrigin = RootOrTechnicalMajority;
    type MembershipInitialized = TechnicalCommittee;
    type MembershipChanged = TechnicalCommittee;
    type MaxMembers = TechnicalMaxMembers;
    type WeightInfo = pallet_collective_membership::weights::SubstrateWeight<Runtime>;
}

// 仲裁委员会成员管理
pub type ArbitrationMembershipInstance = pallet_collective_membership::Instance2;

impl pallet_collective_membership::Config<ArbitrationMembershipInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddOrigin = RootOrTechnicalMajority;
    type RemoveOrigin = RootOrTechnicalMajority;
    type SwapOrigin = RootOrTechnicalMajority;
    type ResetOrigin = RootOrTechnicalMajority;
    type PrimeOrigin = RootOrTechnicalMajority;
    type MembershipInitialized = ArbitrationCommittee;
    type MembershipChanged = ArbitrationCommittee;
    type MaxMembers = ArbitrationMaxMembers;
    type WeightInfo = pallet_collective_membership::weights::SubstrateWeight<Runtime>;
}

// 财务委员会成员管理
pub type TreasuryMembershipInstance = pallet_collective_membership::Instance3;

impl pallet_collective_membership::Config<TreasuryMembershipInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddOrigin = RootOrTechnicalMajority;
    type RemoveOrigin = RootOrTechnicalMajority;
    type SwapOrigin = RootOrTechnicalMajority;
    type ResetOrigin = RootOrTechnicalMajority;
    type PrimeOrigin = RootOrTechnicalMajority;
    type MembershipInitialized = TreasuryCouncil;
    type MembershipChanged = TreasuryCouncil;
    type MaxMembers = TreasuryMaxMembers;
    type WeightInfo = pallet_collective_membership::weights::SubstrateWeight<Runtime>;
}

// 内容委员会成员管理
pub type ContentMembershipInstance = pallet_collective_membership::Instance4;

impl pallet_collective_membership::Config<ContentMembershipInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AddOrigin = RootOrTechnicalMajority;
    type RemoveOrigin = RootOrTechnicalMajority;
    type SwapOrigin = RootOrTechnicalMajority;
    type ResetOrigin = RootOrTechnicalMajority;
    type PrimeOrigin = RootOrTechnicalMajority;
    type MembershipInitialized = ContentCommittee;
    type MembershipChanged = ContentCommittee;
    type MaxMembers = ContentMaxMembers;
    type WeightInfo = pallet_collective_membership::weights::SubstrateWeight<Runtime>;
}

// ============================================================================
// Storage Lifecycle Pallet Configuration
// ============================================================================

/// StorageService 归档器适配器
///
/// 桥接 pallet-storage-lifecycle 的 StorageArchiver trait 与 pallet-storage-service 的存储。
/// 通过 CidArchiveIndex (u64 → Hash) 映射 lifecycle 的 data_id 到 service 的 cid_hash。
pub struct StorageServiceArchiver;

impl pallet_storage_lifecycle::StorageArchiver for StorageServiceArchiver {
    fn scan_archivable(_delay: u64, _max_count: u32) -> alloc::vec::Vec<u64> {
        // 由 scan_for_level 替代
        alloc::vec::Vec::new()
    }

    fn archive_records(_ids: &[u64]) {
        // 由 archive_to_level 替代
    }

    fn scan_for_level(
        _data_type: &[u8],
        target_level: pallet_storage_lifecycle::ArchiveLevel,
        delay: u64,
        max_count: u32,
    ) -> alloc::vec::Vec<u64> {
        use pallet_storage_lifecycle::ArchiveLevel;

        // 仅处理 pin_storage 数据类型
        let current_block: u64 = <frame_system::Pallet<Runtime>>::block_number()
            .try_into()
            .unwrap_or(0u64);
        let dt =
            frame_support::BoundedVec::<u8, frame_support::traits::ConstU32<32>>::truncate_from(
                b"pin_storage".to_vec(),
            );
        let mut result = alloc::vec::Vec::new();

        // 遍历 CidArchiveIndex 查找符合条件的记录
        let next_id = pallet_storage_service::NextCidArchiveId::<Runtime>::get();
        for archive_id in 0..next_id {
            if result.len() >= max_count as usize {
                break;
            }
            let Some(cid_hash) =
                pallet_storage_service::CidArchiveIndex::<Runtime>::get(archive_id)
            else {
                continue; // 已被清理的索引
            };

            // 检查当前归档级别
            let current_level = pallet_storage_lifecycle::pallet::DataArchiveStatus::<Runtime>::get(
                &dt, archive_id,
            );

            match target_level {
                ArchiveLevel::ArchivedL1 => {
                    // 扫描 Active 数据，检查 last_activity 是否超过 delay
                    if !matches!(current_level, ArchiveLevel::Active) {
                        continue;
                    }
                    if let Some(meta) = pallet_storage_service::PinMeta::<Runtime>::get(cid_hash) {
                        let last: u64 = meta.last_activity.try_into().unwrap_or(0u64);
                        if current_block.saturating_sub(last) >= delay {
                            result.push(archive_id);
                        }
                    }
                }
                ArchiveLevel::ArchivedL2 => {
                    // 扫描 L1 数据，检查归档时间是否超过 delay
                    if !matches!(current_level, ArchiveLevel::ArchivedL1) {
                        continue;
                    }
                    // L1 归档后的时间由 ArchiveStats.last_archive_at 近似
                    // 简化：使用 PinMeta.last_activity 作为 L1 归档时间戳
                    if let Some(meta) = pallet_storage_service::PinMeta::<Runtime>::get(cid_hash) {
                        let last: u64 = meta.last_activity.try_into().unwrap_or(0u64);
                        if current_block.saturating_sub(last) >= delay {
                            result.push(archive_id);
                        }
                    } else {
                        // PinMeta 已被 L1 归档清理，直接符合条件
                        result.push(archive_id);
                    }
                }
                ArchiveLevel::Purged => {
                    // 扫描 L2 数据
                    if !matches!(current_level, ArchiveLevel::ArchivedL2) {
                        continue;
                    }
                    // L2 数据已无 PinMeta，直接按 delay 判断
                    result.push(archive_id);
                }
                _ => {}
            }
        }
        result
    }

    fn archive_to_level(
        _data_type: &[u8],
        ids: &[u64],
        target_level: pallet_storage_lifecycle::ArchiveLevel,
    ) {
        use pallet_storage_lifecycle::ArchiveLevel;

        for &archive_id in ids {
            let Some(cid_hash) =
                pallet_storage_service::CidArchiveIndex::<Runtime>::get(archive_id)
            else {
                continue;
            };
            match target_level {
                ArchiveLevel::ArchivedL1 => {
                    // L1 归档：保留 PinMeta（精简），清理计费和健康检查
                    pallet_storage_service::PinBilling::<Runtime>::remove(cid_hash);
                }
                ArchiveLevel::ArchivedL2 => {
                    // L2 归档：清理 PinMeta，仅保留索引
                    pallet_storage_service::PinMeta::<Runtime>::remove(cid_hash);
                    pallet_storage_service::PinStateOf::<Runtime>::remove(cid_hash);
                }
                ArchiveLevel::Purged => {
                    // 完全清除：调用 do_cleanup_single_cid
                    pallet_storage_service::Pallet::<Runtime>::do_cleanup_single_cid(&cid_hash);
                }
                _ => {}
            }
        }
    }

    fn registered_data_types() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
        alloc::vec![b"pin_storage".to_vec()]
    }

    fn query_archive_level(
        data_type: &[u8],
        data_id: u64,
    ) -> pallet_storage_lifecycle::ArchiveLevel {
        let dt = frame_support::BoundedVec::truncate_from(data_type.to_vec());
        pallet_storage_lifecycle::pallet::DataArchiveStatus::<Runtime>::get(&dt, data_id)
    }

    fn restore_record(
        _data_type: &[u8],
        data_id: u64,
        from_level: pallet_storage_lifecycle::ArchiveLevel,
    ) -> bool {
        use pallet_storage_lifecycle::ArchiveLevel;
        // 仅支持 L1 → Active 恢复（L1 阶段 PinMeta 仍存在）
        if !matches!(from_level, ArchiveLevel::ArchivedL1) {
            return false;
        }
        let Some(cid_hash) = pallet_storage_service::CidArchiveIndex::<Runtime>::get(data_id)
        else {
            return false;
        };
        // 验证 PinMeta 仍存在（L1 阶段保留）
        pallet_storage_service::PinMeta::<Runtime>::contains_key(cid_hash)
    }
}

/// StorageService 数据所有权查询适配器
///
/// 通过 CidArchiveIndex + PinSubjectOf 查询 CID 所有者。
pub struct StorageServiceOwnerProvider;

impl pallet_storage_lifecycle::DataOwnerProvider<AccountId> for StorageServiceOwnerProvider {
    fn is_owner(who: &AccountId, _data_type: &[u8], data_id: u64) -> bool {
        let Some(cid_hash) = pallet_storage_service::CidArchiveIndex::<Runtime>::get(data_id)
        else {
            return false;
        };
        match pallet_storage_service::PinSubjectOf::<Runtime>::get(cid_hash) {
            Some((owner, _)) => &owner == who,
            None => false,
        }
    }
}

impl pallet_storage_lifecycle::Config for Runtime {
    type L1ArchiveDelay = ConstU32<{ 30 * DAYS }>; // 30天后归档到L1
    type L2ArchiveDelay = ConstU32<{ 90 * DAYS }>; // L1后90天归档到L2
    type PurgeDelay = ConstU32<{ 180 * DAYS }>; // L2后180天可清除
    type EnablePurge = ConstBool<false>; // 默认不启用清除
    type MaxBatchSize = ConstU32<100>; // 每次最多处理100条
    type StorageArchiver = StorageServiceArchiver;
    type OnArchive = ();
    type DataOwnerProvider = StorageServiceOwnerProvider;
    type GovernanceOrigin = RootOrTechnicalMajority;
    type WeightInfo = pallet_storage_lifecycle::weights::SubstrateWeight<Runtime>;
}

// ============================================================================
// Contracts Pallet Configuration
// ============================================================================

parameter_types! {
    pub ContractsSchedule: pallet_contracts::Schedule<Runtime> = Default::default();
    pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(30);
}

/// 随机数源（使用确定性随机）
pub struct DummyRandomness;
impl frame_support::traits::Randomness<Hash, BlockNumber> for DummyRandomness {
    fn random(subject: &[u8]) -> (Hash, BlockNumber) {
        use sp_runtime::traits::Hash as HashT;
        let block_number = System::block_number();
        let hash = <Runtime as frame_system::Config>::Hashing::hash(subject);
        (hash, block_number)
    }
}

impl pallet_contracts::Config for Runtime {
    type Time = Timestamp;
    type Randomness = DummyRandomness;
    type Currency = Balances;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;

    /// 合约调用栈深度限制（depth = size + 1 = 6）
    /// 受 integrity_test 约束：(MaxCodeLen*68 + 2MB) * depth + 2MB < runtime_memory/2
    /// 参考 Astar/Shiden 配置；depth 6 足够覆盖绝大多数 ink! 合约调用场景
    type CallStack = [pallet_contracts::Frame<Self>; 5];

    /// 合约存储押金（每字节）
    type DepositPerByte = ConstU128<{ UNIT / 1000 }>;
    /// 合约存储押金（每个存储项）
    type DepositPerItem = ConstU128<{ UNIT / 100 }>;
    /// 默认存款限制
    type DefaultDepositLimit = ConstU128<{ 100 * UNIT }>;

    /// 代码哈希锁定押金百分比
    type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;

    /// 权重信息
    type WeightPrice = pallet_transaction_payment::Pallet<Self>;
    type WeightInfo = pallet_contracts::weights::SubstrateWeight<Self>;

    /// 链扩展（无）
    type ChainExtension = ();

    /// 调度表
    type Schedule = ContractsSchedule;

    /// 地址生成器
    type AddressGenerator = pallet_contracts::DefaultAddressGenerator;

    /// 最大代码长度（受 CallStack depth 约束，depth=6 时上限 ~125KB）
    /// 123KB 参考 Astar 配置，足够部署复杂 ink! 合约
    type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
    /// 最大存储键长度
    type MaxStorageKeyLen = ConstU32<128>;

    /// 不安全的不稳定接口（生产环境应禁用）
    type UnsafeUnstableInterface = ConstBool<false>;

    /// 上传来源
    type UploadOrigin = frame_system::EnsureSigned<AccountId>;
    /// 实例化来源
    type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;

    /// 最大调试缓冲区长度
    type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;

    /// 最大委托依赖数
    type MaxDelegateDependencies = ConstU32<32>;

    /// 运行时 Hold 原因
    type RuntimeHoldReason = RuntimeHoldReason;

    /// 环境类型
    type Environment = ();

    /// API 版本
    type ApiVersion = ();

    /// Xcm 相关（不使用）
    type Xcm = ();

    /// 迁移
    type Migrations = ();

    /// 调试
    type Debug = ();

    /// 调用过滤器（允许所有调用）
    type CallFilter = frame_support::traits::Everything;

    /// 最大瞬态存储大小
    type MaxTransientStorageSize = ConstU32<{ 1024 * 1024 }>;
}

// ============================================================================
// Assets Configuration (for ShareMall Token)
// ============================================================================

parameter_types! {
    /// 创建资产押金: 100 COS
    pub const AssetDeposit: Balance = 100 * UNIT;
    /// 账户持有资产押金: 1 COS
    pub const AssetAccountDeposit: Balance = UNIT;
    /// 元数据押金基础: 10 COS
    pub const MetadataDepositBase: Balance = 10 * UNIT;
    /// 元数据押金每字节: 0.1 NEX
    pub const MetadataDepositPerByte: Balance = UNIT / 10;
    /// 授权押金: 1 COS
    pub const ApprovalDeposit: Balance = UNIT;
    /// 字符串长度限制
    pub const AssetsStringLimit: u32 = 50;
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type AssetId = u64;
    type AssetIdParameter = codec::Compact<u64>;
    type Currency = Balances;
    type CreateOrigin =
        frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
    type ForceOrigin = EnsureRoot<AccountId>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    type RemoveItemsLimit = ConstU32<1000>;
    type ReserveData = ();
    type Holder = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = AssetsBenchHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct AssetsBenchHelper;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_assets::BenchmarkHelper<codec::Compact<u64>, ()> for AssetsBenchHelper {
    fn create_asset_id_parameter(id: u32) -> codec::Compact<u64> {
        codec::Compact(id as u64)
    }
    fn create_reserve_id_parameter(_id: u32) -> () {
        ()
    }
}

// ============================================================================
// Entity Configuration (原 ShareMall，已重构)
// ============================================================================

parameter_types! {
    /// 发货超时: 约 3 天 (假设 6 秒一个块)
    pub const EntityShipTimeout: BlockNumber = 43200;
    /// 确认收货超时: 约 7 天
    pub const EntityConfirmTimeout: BlockNumber = 100800;
    /// 实体代币 ID 偏移量
    pub const EntityTokenOffset: u64 = 1_000_000;
    /// 投票期: 7 天
    pub const GovernanceVotingPeriod: BlockNumber = 100800;
    /// 执行延迟: 2 天
    pub const GovernanceExecutionDelay: BlockNumber = 28800;
    /// 通过阈值: 50%
    pub const GovernancePassThreshold: u8 = 50;
    /// 法定人数: 10%
    pub const GovernanceQuorumThreshold: u8 = 10;
    /// 创建提案所需最低代币持有比例: 1%
    pub const GovernanceMinProposalThreshold: u16 = 100;
    /// C3: 最小投票期: 1 天
    pub const GovernanceMinVotingPeriod: BlockNumber = 14400;
    /// C3: 最小执行延迟: 4 小时
    pub const GovernanceMinExecutionDelay: BlockNumber = 2400;
}

/// 平台账户
pub struct EntityPlatformAccount;
impl frame_support::traits::Get<AccountId> for EntityPlatformAccount {
    fn get() -> AccountId {
        frame_support::PalletId(*b"entity//").into_account_truncating()
    }
}

impl pallet_entity_registry::Config for Runtime {
    type Currency = Balances;
    type MaxEntityNameLength = ConstU32<64>;
    type MaxCidLength = ConstU32<64>;
    type GovernanceOrigin = RootOrTechnicalMajority;
    type PricingProvider = EntityPricingProvider;
    type InitialFundUsdt = ConstU64<50_000_000>; // 50 USDT
    type MinOperatingBalance = ConstU128<{ UNIT / 10 }>;
    type FundWarningThreshold = ConstU128<{ UNIT }>;
    type MaxAdmins = ConstU32<10>;
    type MaxEntitiesPerUser = ConstU32<3>;
    type ShopProvider = EntityShop;
    type MaxShopsPerEntity = ConstU32<16>;
    type PlatformAccount = EntityPlatformAccount;
    type GovernanceProvider = EntityGovernance;
    type CloseRequestTimeout = ConstU32<{ 7 * DAYS }>; // 7 天
    type MaxReferralsPerReferrer = ConstU32<1000>;
    type StoragePin = pallet_storage_service::Pallet<Runtime>;
    type OnEntityStatusChange = EntityDisclosure;
    type OrderProvider = EntityTransaction;
    type TokenSaleProvider = EntityTokenSale;
    type DisputeQueryProvider = pallet_entity_common::NullDisputeQueryProvider;
    type MarketProvider = pallet_entity_common::NullMarketProvider;
    type CommissionCloseGuard = pallet_commission_core::Pallet<Runtime>;
    type CommissionFundGuard = pallet_commission_core::Pallet<Runtime>;
    type CommissionFundDetail = pallet_commission_core::Pallet<Runtime>;
    type FundProtectionQuery = EntityGovernance;
    type WeightInfo = pallet_entity_registry::SubstrateWeight;
}

impl pallet_entity_shop::Config for Runtime {
    type Currency = Balances;
    type EntityProvider = EntityRegistry;
    type MaxShopNameLength = ConstU32<64>;
    type MaxCidLength = ConstU32<64>;
    type MaxManagers = ConstU32<10>;
    type MinOperatingBalance = ConstU128<{ UNIT / 10 }>;
    type WarningThreshold = ConstU128<{ UNIT }>;
    type PricingProvider = EntityPricingProvider;
    type MinInitialFundUsdt = ConstU64<5_000_000>; // 5 USDT
    type CommissionFundGuard = crate::CommissionCore;
    type ShopClosingGracePeriod = ConstU32<100800>; // 7 days @ 6s/block
    type MaxShopsPerEntity = ConstU32<16>;
    type StoragePin = pallet_storage_service::Pallet<Runtime>;
    type ProductProvider = EntityProduct;
    type PointsCleanup = crate::EntityLoyalty;
    type OrderProvider = EntityTransaction;
    type WeightInfo = pallet_entity_shop::weights::SubstrateWeight<Runtime>;
    type FundProtectionQuery = EntityGovernance;
}

impl pallet_entity_product::Config for Runtime {
    type Currency = Balances;
    type EntityProvider = EntityRegistry;
    type ShopProvider = EntityShop;
    type PricingProvider = EntityPricingProvider;
    type MaxProductsPerShop = ConstU32<1000>;
    type MaxCidLength = ConstU32<64>;
    type ProductDepositUsdt = ConstU64<1_000_000>; // 1 USDT
    type StoragePin = pallet_storage_service::Pallet<Runtime>;
    type MaxBatchSize = ConstU32<50>;
    type MaxReasonLength = ConstU32<256>;
    type WeightInfo = pallet_entity_product::weights::SubstrateWeight<Runtime>;
}

/// Bridge order system notifications → chat-core's System channel (audit 2.3).
/// Best-effort: errors are swallowed so chat wiring never aborts an order
/// transition. / 将订单系统通知桥接到 chat-core 的 System 通道（审计 2.3）；吞错,
/// 聊天接线绝不中断订单状态转移。
pub struct OrderChatNotifier;
impl pallet_entity_order::OrderNotifier<AccountId> for OrderChatNotifier {
    fn notify(to: &AccountId, notice: alloc::vec::Vec<u8>) {
        ChatCore::notify_system_best_effort(to, notice);
    }
}

/// Adapter mapping the order pallet's chat hooks to chat-permission scene
/// authorizations (`source = *b"entorder"`, `SceneType::Order`,
/// `scene_id = Numeric(order_id)`). Best-effort: errors are swallowed so chat
/// wiring never aborts an order extrinsic. / 将订单聊天钩子映射为 chat-permission
/// 场景授权（买卖双方双向）；吞错，确保聊天接线不会中断订单交易。
pub struct OrderChatSceneAuthorizer;
impl pallet_entity_order::OrderChatAuthorizer<AccountId> for OrderChatSceneAuthorizer {
    fn grant(order_id: u64, buyer: &AccountId, seller: &AccountId) {
        ChatPermission::grant_scene_from_hook(
            *b"entorder",
            buyer,
            seller,
            pallet_chat_permission::SceneType::Order,
            pallet_chat_permission::SceneId::Numeric(order_id),
            None,
            alloc::vec::Vec::new(),
        );
    }
    fn revoke(order_id: u64, buyer: &AccountId, seller: &AccountId) {
        ChatPermission::revoke_scene_from_hook(
            *b"entorder",
            buyer,
            seller,
            pallet_chat_permission::SceneType::Order,
            pallet_chat_permission::SceneId::Numeric(order_id),
        );
    }
}

impl pallet_entity_order::Config for Runtime {
    type Currency = Balances;
    type Escrow = Escrow;
    type ShopProvider = EntityShop;
    type ProductProvider = EntityProduct;
    type EntityProvider = EntityRegistry;
    type EntityToken = EntityToken;
    type PlatformAccount = EntityPlatformAccount;
    type ShipTimeout = EntityShipTimeout;
    type ConfirmTimeout = EntityConfirmTimeout;
    type ServiceConfirmTimeout = ConstU32<{ 7 * 24 * 600 }>; // 7 天
    type DisputeTimeout = ConstU32<{ 14 * 24 * 600 }>; // 14 天
    type ConfirmExtension = ConstU32<{ 7 * 24 * 600 }>; // 7 天
    type OnOrderCompleted = (
        OrderMemberHook,
        OrderShopStatsHook,
        OrderCommissionHook,
        OrderLoyaltyHook,
    );
    type OnOrderCancelled = OrderCancelHook;
    type TokenFeeConfig = TokenFeeConfigBridge;
    type Loyalty = LoyaltyBridge;
    type MemberProvider = EntityMemberProvider;
    type PricingProvider = EntityPricingProvider;
    type TokenPriceProvider = EntityMarket;
    type MaxCidLength = ConstU32<64>;
    type MaxBuyerOrders = ConstU32<1000>;
    type MaxPayerOrders = ConstU32<1000>;
    type MaxShopOrders = ConstU32<10000>;
    type MaxExpiryQueueSize = ConstU32<500>;
    type Notifier = OrderChatNotifier;
    // 订单存续期内开通买卖双方双向聊天，终态撤销（仿 bounty 的 ChatAuthorizer）。
    // Open buyer↔seller chat for the order's lifetime, revoked at terminal states.
    type Chat = OrderChatSceneAuthorizer;
    type WeightInfo = pallet_entity_order::weights::SubstrateWeight<Runtime>;
}

impl pallet_entity_review::Config for Runtime {
    type EntityProvider = EntityRegistry;
    type OrderProvider = EntityTransaction;
    type ShopProvider = EntityShop;
    type MaxCidLength = ConstU32<64>;
    type MaxReviewsPerUser = ConstU32<500>;
    type ReviewWindowBlocks = ConstU64<100800>; // 7 days × 24h × 60min × 60s / 6s = 100800 blocks
    type EditWindowBlocks = ConstU64<14400>; // 1 day × 24h × 60min × 60s / 6s = 14400 blocks
    type MaxProductReviews = ConstU32<10000>;
    type WeightInfo = pallet_entity_review::weights::SubstrateWeight<Runtime>;
}

/// Token KYC 适配器（桥接 pallet-entity-kyc → KycLevelProvider trait, per-entity）
pub struct TokenKycBridge;
impl pallet_entity_token::pallet::KycLevelProvider<AccountId> for TokenKycBridge {
    fn get_kyc_level(entity_id: u64, account: &AccountId) -> u8 {
        EntityKyc::get_kyc_level(entity_id, account).as_u8()
    }
    fn meets_kyc_requirement(entity_id: u64, account: &AccountId, min_level: u8) -> bool {
        EntityKyc::get_kyc_level(entity_id, account).as_u8() >= min_level
    }
}
pub type TokenMemberProvider = pallet_entity_token::pallet::NullMemberProvider;

impl pallet_entity_token::Config for Runtime {
    type AssetId = u64;
    type AssetBalance = Balance;
    type Assets = Assets;
    type EntityProvider = EntityRegistry;
    type ShopTokenOffset = ConstU64<1_000_000>; // 店铺代币 ID 从 1,000,000 开始
    type MaxTokenNameLength = ConstU32<64>;
    type MaxTokenSymbolLength = ConstU32<8>;
    type MaxTransferListSize = ConstU32<1000>;
    type MaxDividendRecipients = ConstU32<500>;
    type KycProvider = TokenKycBridge;
    type MemberProvider = TokenMemberProvider;
    type DisclosureProvider = EntityDisclosure;
    type WeightInfo = pallet_entity_token::weights::SubstrateWeight<Runtime>;
}

// EntityTokenProvider 使用 EntityToken pallet 直接实现
pub type EntityTokenProvider = EntityToken;

/// Entity PricingProvider 适配器
pub struct EntityPricingProvider;
impl pallet_entity_common::PricingProvider for EntityPricingProvider {
    fn get_nex_usdt_price() -> u64 {
        // 调用 TradingPricingProvider 获取 NEX/USD 汇率（已优先 TWAP）
        <TradingPricingProvider as pallet_trading_common::PricingProvider<Balance>>::get_nex_to_usd_rate()
			.map(|rate| rate as u64)
			.unwrap_or(0)
    }
    fn is_price_stale() -> bool {
        // 超过 2400 区块（~4 小时 @6s/block）无交易则视为过时
        <pallet_nex_market::Pallet<Runtime> as pallet_trading_common::PriceOracle>::is_price_stale(
            2400,
        )
    }
}

/// 桥接实现：将 EntityMember pallet 桥接到 commission 模块的 MemberProvider trait
pub struct MemberProviderBridge;

impl pallet_entity_commission::MemberProvider<AccountId> for MemberProviderBridge {
    fn is_member(entity_id: u64, account: &AccountId) -> bool {
        pallet_entity_member::EntityMembers::<Runtime>::contains_key(entity_id, account)
    }

    fn get_referrer(entity_id: u64, account: &AccountId) -> Option<AccountId> {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .and_then(|m| m.referrer)
    }

    fn get_member_stats(
        entity_id: u64,
        account: &AccountId,
    ) -> pallet_entity_common::MemberStats {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .map(|m| pallet_entity_common::MemberStats {
                direct_referrals: m.direct_referrals,
                team_size: m.team_size,
                spend: pallet_entity_common::MemberSpendStats {
                    total_spent: m.total_spent as u128,
                    upgrade_eligible_spent: m.upgrade_eligible_spent as u128,
                },
            })
            .unwrap_or(pallet_entity_common::MemberStats {
                direct_referrals: 0,
                team_size: 0,
                spend: pallet_entity_common::MemberSpendStats {
                    total_spent: 0,
                    upgrade_eligible_spent: 0,
                },
            })
    }

    fn uses_custom_levels(entity_id: u64) -> bool {
        pallet_entity_member::Pallet::<Runtime>::uses_custom_levels_by_entity(entity_id)
    }

    fn custom_level_id(entity_id: u64, account: &AccountId) -> u8 {
        // H6 审计修复: 使用 get_effective_level 检查等级过期
        pallet_entity_member::Pallet::<Runtime>::get_effective_level_by_entity(entity_id, account)
    }

    fn get_level_commission_bonus(entity_id: u64, level_id: u8) -> u16 {
        pallet_entity_member::Pallet::<Runtime>::get_level_commission_bonus_by_entity(
            entity_id, level_id,
        )
    }

    fn auto_register(
        entity_id: u64,
        account: &AccountId,
        referrer: Option<AccountId>,
    ) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::auto_register_by_entity(
            entity_id, account, referrer,
        )
    }

    fn set_custom_levels_enabled(entity_id: u64, enabled: bool) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::governance_set_custom_levels_enabled(
            entity_id, enabled,
        )
    }

    fn set_upgrade_mode(entity_id: u64, mode: u8) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::governance_set_upgrade_mode(entity_id, mode)
    }

    fn add_custom_level(
        entity_id: u64,
        _level_id: u8,
        name: &[u8],
        threshold: u128,
        discount_rate: u16,
        commission_bonus: u16,
    ) -> sp_runtime::DispatchResult {
        // level_id 由 pallet 自动分配，忽略传入值
        pallet_entity_member::Pallet::<Runtime>::governance_add_custom_level(
            entity_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        )
    }

    fn update_custom_level(
        entity_id: u64,
        level_id: u8,
        name: Option<&[u8]>,
        threshold: Option<u128>,
        discount_rate: Option<u16>,
        commission_bonus: Option<u16>,
    ) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::governance_update_custom_level(
            entity_id,
            level_id,
            name,
            threshold,
            discount_rate,
            commission_bonus,
        )
    }

    fn remove_custom_level(entity_id: u64, level_id: u8) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::governance_remove_custom_level(entity_id, level_id)
    }

    fn custom_level_count(entity_id: u64) -> u8 {
        pallet_entity_member::Pallet::<Runtime>::custom_level_count_by_entity(entity_id)
    }

    fn member_count_by_level(entity_id: u64, level_id: u8) -> u32 {
        pallet_entity_member::LevelMemberCount::<Runtime>::get(entity_id, level_id)
    }

    fn get_member_total_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .map(|m| m.total_spent)
            .unwrap_or(0)
    }

    fn get_member_upgrade_eligible_spent_usdt(entity_id: u64, account: &AccountId) -> u64 {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .map(|m| m.upgrade_eligible_spent)
            .unwrap_or(0)
    }

    fn get_effective_level(entity_id: u64, account: &AccountId) -> u8 {
        pallet_entity_member::Pallet::<Runtime>::get_effective_level_by_entity(entity_id, account)
    }

    fn get_level_discount(entity_id: u64, level_id: u8) -> u16 {
        pallet_entity_member::Pallet::<Runtime>::get_level_discount_by_entity(entity_id, level_id)
    }

    fn update_spent(
        entity_id: u64,
        account: &AccountId,
        amount_usdt: u64,
    ) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::update_spent_by_entity(
            entity_id,
            account,
            amount_usdt,
            false,
        )
    }

    fn check_order_upgrade_rules(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
        amount_usdt: u64,
    ) -> sp_runtime::DispatchResult {
        pallet_entity_member::Pallet::<Runtime>::check_order_upgrade_rules_by_entity(
            entity_id,
            buyer,
            product_id,
            amount_usdt,
        )
    }

    fn is_banned(entity_id: u64, account: &AccountId) -> bool {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .map(|m| m.banned_at.is_some())
            .unwrap_or(false)
    }

    fn is_activated(entity_id: u64, account: &AccountId) -> bool {
        pallet_entity_member::EntityMembers::<Runtime>::get(entity_id, account)
            .map(|m| m.activated)
            .unwrap_or(false)
    }

    fn get_direct_referral_accounts(
        entity_id: u64,
        account: &AccountId,
    ) -> alloc::vec::Vec<AccountId> {
        pallet_entity_member::DirectReferrals::<Runtime>::get(entity_id, account).into_inner()
    }
}

/// Hook: 会员自动注册 + 消费更新 + 升级检查（Phase 5.3）
pub struct OrderMemberHook;
impl pallet_entity_common::OnOrderCompleted<AccountId, Balance> for OrderMemberHook {
    fn on_completed(info: &pallet_entity_common::OrderCompletionInfo<AccountId, Balance>) {
        let _ = pallet_entity_member::Pallet::<Runtime>::auto_register_by_entity(
            info.entity_id,
            &info.buyer,
            info.referrer.clone(),
        );
        let is_shopping = info.payment_asset == pallet_entity_common::PaymentAsset::ShoppingBalance;
        let _ = pallet_entity_member::Pallet::<Runtime>::update_spent_by_entity(
            info.entity_id,
            &info.buyer,
            info.amount_usdt,
            is_shopping,
        );
        let _ = pallet_entity_member::Pallet::<Runtime>::check_order_upgrade_rules_by_entity(
            info.entity_id,
            &info.buyer,
            info.product_id,
            info.amount_usdt,
        );
    }
}

/// Hook: Shop 统计更新（Phase 5.3）
pub struct OrderShopStatsHook;
impl pallet_entity_common::OnOrderCompleted<AccountId, Balance> for OrderShopStatsHook {
    fn on_completed(info: &pallet_entity_common::OrderCompletionInfo<AccountId, Balance>) {
        use sp_runtime::SaturatedConversion;
        let amount: u128 = match info.payment_asset {
            pallet_entity_common::PaymentAsset::Native => info.nex_seller_received.saturated_into(),
            pallet_entity_common::PaymentAsset::ShoppingBalance => {
                info.shopping_balance_used.saturated_into()
            }
            pallet_entity_common::PaymentAsset::EntityToken => info.token_payment_amount,
        };
        let _ = <EntityShop as pallet_entity_common::ShopProvider<AccountId>>::update_shop_stats(
            info.shop_id,
            amount,
            1,
        );
    }
}

/// Hook: NEX/Token 佣金分发（Phase 5.3）
/// P0-1 审计修复: 不再用 let _ = 吞错误，改为记录 log::error 以便链上追踪
/// P0-5 审计修复: Token 平台费转账失败时传 0 避免"账面有承诺、实际没钱"
/// BUG-1 审计修复: 佣金处理后调用 settle_order_commission 结算记录
pub struct OrderCommissionHook;
impl pallet_entity_common::OnOrderCompleted<AccountId, Balance> for OrderCommissionHook {
    fn on_completed(info: &pallet_entity_common::OrderCompletionInfo<AccountId, Balance>) {
        use sp_runtime::SaturatedConversion;
        match info.payment_asset {
            pallet_entity_common::PaymentAsset::Native => {
                let available_pool = info.nex_total_amount.saturating_sub(info.nex_platform_fee);
                let result = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::process_commission(
					info.entity_id, info.shop_id, info.order_id, &info.buyer,
					info.nex_total_amount, available_pool, info.nex_platform_fee,
					info.product_id,
					info.nex_reserved_for_commission,
				);
                if let Err(e) = result {
                    // 分佣失败: fallback 释放锁定，防止卖家资金永久冻结
                    <Balances as frame_support::traits::ReservableCurrency<AccountId>>::unreserve(
                        &info.seller,
                        info.nex_reserved_for_commission,
                    );
                    log::error!(
                        target: "runtime::commission",
                        "process_commission failed for order {}: {:?}",
                        info.order_id, e,
                    );
                }
                // BUG-1 审计修复: 佣金处理后立即结算记录（Pending → Settled）
                if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::settle_order_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"settle_order_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
            }
            pallet_entity_common::PaymentAsset::ShoppingBalance => {
                if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::process_shopping_commission(
					info.entity_id, info.shop_id, info.order_id, &info.buyer,
					info.shopping_balance_used, info.product_id,
				) {
					log::error!(
						target: "runtime::commission",
						"process_shopping_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
                if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::settle_order_shopping_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"settle_order_shopping_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
            }
            pallet_entity_common::PaymentAsset::EntityToken => {
                let token_balance: <Runtime as pallet_commission_core::Config>::TokenBalance =
                    info.token_payment_amount.saturated_into();
                // P0-5: 平台费转账失败时用 0，避免虚记佣金
                let actual_fee: u128 = if info.token_platform_fee_paid {
                    info.token_platform_fee
                } else {
                    0
                };
                let fee_balance: <Runtime as pallet_commission_core::Config>::TokenBalance =
                    actual_fee.saturated_into();
                let available_pool = token_balance.saturating_sub(fee_balance);
                if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::TokenCommissionProvider<AccountId, <Runtime as pallet_commission_core::Config>::TokenBalance>>::process_token_commission(
					info.entity_id, info.shop_id, info.order_id, &info.buyer,
					token_balance, available_pool, fee_balance,
					info.product_id,
				) {
					log::error!(
						target: "runtime::commission",
						"process_token_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
                if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::settle_order_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"settle_order_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
            }
        }
    }
}

/// Hook: 积分奖励（Phase 5.3）
/// Hook: loyalty reward on order completion (Phase 5.3)
pub struct OrderLoyaltyHook;
impl pallet_entity_common::OnOrderCompleted<AccountId, Balance> for OrderLoyaltyHook {
    fn on_completed(info: &pallet_entity_common::OrderCompletionInfo<AccountId, Balance>) {
        use sp_runtime::SaturatedConversion;
        // C-2 fix: 购物余额是国库内部循环资金，不发放 Token 奖励
        // C-2 fix: shopping balance is internal treasury circulation — skip Token reward
        let amount: Balance = match info.payment_asset {
            pallet_entity_common::PaymentAsset::Native => info.nex_total_amount,
            pallet_entity_common::PaymentAsset::ShoppingBalance => return,
            pallet_entity_common::PaymentAsset::EntityToken => {
                info.token_payment_amount.saturated_into()
            }
        };
        let _ = <LoyaltyBridge as pallet_entity_common::LoyaltyWritePort<AccountId, Balance>>::reward_on_purchase(
			info.entity_id, &info.buyer, amount,
		);
    }
}

/// Hook: 取消佣金（Phase 5.3）
/// P0-1 审计修复: 不再用 let _ = 吞错误
pub struct OrderCancelHook;
impl pallet_entity_common::OnOrderCancelled for OrderCancelHook {
    fn on_cancelled(info: &pallet_entity_common::OrderCancellationInfo) {
        match info.payment_asset {
			pallet_entity_common::PaymentAsset::Native => {
				if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::cancel_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"cancel_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
			},
			pallet_entity_common::PaymentAsset::ShoppingBalance => {
				if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::CommissionProvider<AccountId, Balance>>::cancel_shopping_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"cancel_shopping_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
			},
			pallet_entity_common::PaymentAsset::EntityToken => {
				if let Err(e) = <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::TokenCommissionProvider<AccountId, <Runtime as pallet_commission_core::Config>::TokenBalance>>::cancel_token_commission(info.order_id) {
					log::error!(
						target: "runtime::commission",
						"cancel_token_commission failed for order {}: {:?}",
						info.order_id, e,
					);
				}
			},
		}
    }
}

/// 查询 Bridge: NEX 平台费率（commission-core 用于佣金预算上限约束）
pub struct PlatformFeeRateBridge;
impl pallet_entity_common::FeeConfigProvider for PlatformFeeRateBridge {
    fn platform_fee_rate() -> u16 {
        pallet_entity_order::PlatformFeeRate::<Runtime>::get()
    }
}

/// 查询 Bridge: Token 平台费率 + Entity 账户（Phase 5.3）
pub struct TokenFeeConfigBridge;
impl pallet_entity_common::TokenFeeConfigPort<AccountId> for TokenFeeConfigBridge {
    fn token_platform_fee_rate(entity_id: u64) -> u16 {
        <pallet_commission_core::Pallet<Runtime> as pallet_commission_common::TokenCommissionProvider<AccountId, <Runtime as pallet_commission_core::Config>::TokenBalance>>::token_platform_fee_rate(entity_id)
    }
    fn entity_account(entity_id: u64) -> AccountId {
        <EntityRegistry as pallet_entity_common::EntityProvider<AccountId>>::entity_account(
            entity_id,
        )
    }
}

/// 桥接实现：CommissionCore pallet 作为 CommissionProvider
pub type EntityCommissionProvider = pallet_commission_core::Pallet<Runtime>;

/// 桥接：TokenTransferProvider → EntityToken（供 commission-core/pool-reward Token 转账使用）
pub struct TokenTransferProviderBridge;
impl pallet_commission_common::TokenTransferProvider<AccountId, u128>
    for TokenTransferProviderBridge
{
    fn token_balance_of(entity_id: u64, who: &AccountId) -> u128 {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::token_balance(entity_id, who)
    }
    fn token_transfer(
        entity_id: u64,
        from: &AccountId,
        to: &AccountId,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::transfer(
            entity_id, from, to, amount,
        )
    }
}

/// 桥接：购物余额 → Loyalty 模块（供 Transaction 模块下单时抵扣购物余额）
pub struct ShoppingBalanceBridge;
impl pallet_entity_common::ShoppingBalanceProvider<AccountId, Balance> for ShoppingBalanceBridge {
    fn shopping_balance(entity_id: u64, account: &AccountId) -> Balance {
        pallet_entity_loyalty::MemberShoppingBalance::<Runtime>::get(entity_id, account)
    }
    fn consume_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: Balance,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_consume_shopping_balance(
            entity_id, account, amount,
        )
    }
}

/// 桥接：LoyaltyBridge — 将 Loyalty 模块 + EntityToken 实现连接到 LoyaltyWritePort trait
pub struct LoyaltyBridge;
impl pallet_entity_common::LoyaltyReadPort<AccountId, Balance> for LoyaltyBridge {
    fn is_token_enabled(entity_id: u64) -> bool {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::is_token_enabled(entity_id)
    }
    fn token_discount_balance(entity_id: u64, who: &AccountId) -> Balance {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::token_balance(entity_id, who)
    }
    fn shopping_balance(entity_id: u64, account: &AccountId) -> Balance {
        pallet_entity_loyalty::MemberShoppingBalance::<Runtime>::get(entity_id, account)
    }
    fn shopping_total(entity_id: u64) -> Balance {
        pallet_entity_loyalty::ShopShoppingTotal::<Runtime>::get(entity_id)
    }
}
impl pallet_entity_common::LoyaltyWritePort<AccountId, Balance> for LoyaltyBridge {
    fn redeem_for_discount(
        entity_id: u64,
        who: &AccountId,
        tokens: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::redeem_for_discount(entity_id, who, tokens)
    }
    fn consume_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: Balance,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_consume_shopping_balance(
            entity_id, account, amount,
        )
    }
    fn reward_on_purchase(
        entity_id: u64,
        who: &AccountId,
        purchase_amount: Balance,
    ) -> Result<Balance, sp_runtime::DispatchError> {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::reward_on_purchase(entity_id, who, purchase_amount)
    }
    fn credit_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: Balance,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_credit_shopping_balance(entity_id, who, amount)
    }
    fn rollback_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: Balance,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_rollback_shopping_balance(
            entity_id, who, amount,
        )
    }
    fn rollback_token_discount(
        entity_id: u64,
        who: &AccountId,
        tokens: Balance,
    ) -> Result<(), sp_runtime::DispatchError> {
        <EntityToken as pallet_entity_common::EntityTokenProvider<AccountId, Balance>>::refund_discount_tokens(entity_id, who, tokens)
    }
    fn forfeit_all_shopping_balances(entity_id: u64) {
        <pallet_entity_loyalty::Pallet<Runtime> as pallet_entity_common::LoyaltyWritePort<
            AccountId,
            Balance,
        >>::forfeit_all_shopping_balances(entity_id);
    }
    fn forfeit_shopping_balance(
        entity_id: u64,
        who: &AccountId,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_forfeit_shopping_balance(entity_id, who)
    }
}
impl pallet_entity_common::LoyaltyTokenReadPort<AccountId, u128> for LoyaltyBridge {
    fn token_shopping_balance(entity_id: u64, who: &AccountId) -> u128 {
        pallet_entity_loyalty::MemberTokenShoppingBalance::<Runtime>::get(entity_id, who)
    }
    fn token_shopping_total(entity_id: u64) -> u128 {
        pallet_entity_loyalty::TokenShoppingTotal::<Runtime>::get(entity_id)
    }
}
impl pallet_entity_common::LoyaltyTokenWritePort<AccountId, u128> for LoyaltyBridge {
    fn credit_token_shopping_balance(
        entity_id: u64,
        who: &AccountId,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_credit_token_shopping_balance(
            entity_id, who, amount,
        )
    }
    fn consume_token_shopping_balance(
        entity_id: u64,
        account: &AccountId,
        amount: u128,
    ) -> Result<(), sp_runtime::DispatchError> {
        pallet_entity_loyalty::Pallet::<Runtime>::do_consume_token_shopping_balance(
            entity_id, account, amount,
        )
    }
    fn forfeit_all_token_shopping_balances(entity_id: u64) {
        <pallet_entity_loyalty::Pallet<Runtime> as pallet_entity_common::LoyaltyTokenWritePort<
            AccountId,
            u128,
        >>::forfeit_all_token_shopping_balances(entity_id);
    }
}

/// AutoRepurchase Bridge — commission/core 自动下单通过此 bridge 调用 order pallet
///
/// 解耦方向：commission/core 依赖 AutoRepurchasePort trait（定义在 common），
/// order pallet 实现此 trait，两者不直接依赖，无循环依赖。
pub struct AutoRepurchaseBridge;

impl pallet_entity_common::AutoRepurchasePort<AccountId> for AutoRepurchaseBridge {
    fn try_place_repurchase_order(
        entity_id: u64,
        buyer: &AccountId,
        product_id: u64,
    ) -> Result<u64, sp_runtime::DispatchError> {
        // M-4 fix: re-validate product ownership at execution time (defense-in-depth against shop transfer TOCTOU)
        // M-4 修复：执行时重新校验产品归属（纵深防御，防止 Shop 转让后配置悬空）
        Self::validate_repurchase_product(entity_id, product_id)?;
        pallet_entity_order::Pallet::<Runtime>::do_place_order_for_auto_repurchase(
            buyer.clone(),
            product_id,
        )
    }

    fn validate_repurchase_product(
        entity_id: u64,
        product_id: u64,
    ) -> Result<(), sp_runtime::DispatchError> {
        use pallet_entity_common::{ProductProvider, ShopProvider};
        let shop_id = <EntityProduct as ProductProvider<AccountId>>::product_shop_id(product_id)
            .ok_or(sp_runtime::DispatchError::Other("ProductNotFound"))?;
        let product_entity =
            <EntityShop as ShopProvider<AccountId>>::shop_entity_id(shop_id)
                .ok_or(sp_runtime::DispatchError::Other("ShopNotFound"))?;
        frame_support::ensure!(
            product_entity == entity_id,
            sp_runtime::DispatchError::Other("ProductNotInEntity")
        );
        Ok(())
    }
}

pub struct GovernanceShopBridge;
impl pallet_entity_common::ShopGovernancePort for GovernanceShopBridge {
    fn governance_set_points_config(
        entity_id: u64,
        reward_rate: u16,
        exchange_rate: u16,
        transferable: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        EntityLoyalty::governance_set_points_config(
            entity_id,
            reward_rate,
            exchange_rate,
            transferable,
        )
    }
    fn governance_toggle_points(
        entity_id: u64,
        enabled: bool,
    ) -> Result<(), sp_runtime::DispatchError> {
        EntityLoyalty::governance_toggle_points(entity_id, enabled)
    }
    fn governance_set_shop_policies(
        entity_id: u64,
        policies_cid: &[u8],
    ) -> Result<(), sp_runtime::DispatchError> {
        EntityShop::governance_set_shop_policies(entity_id, policies_cid)
    }
}

pub type EntityMemberProvider = MemberProviderBridge;

impl pallet_entity_governance::Config for Runtime {
    type Balance = Balance;
    type EntityProvider = EntityRegistry;
    type ShopProvider = EntityShop;
    type TokenProvider = EntityTokenProvider;
    type CommissionProvider = EntityCommissionProvider;
    type MemberProvider = EntityMemberProvider;
    type VotingPeriod = GovernanceVotingPeriod;
    type ExecutionDelay = GovernanceExecutionDelay;
    type PassThreshold = GovernancePassThreshold;
    type QuorumThreshold = GovernanceQuorumThreshold;
    type MinProposalThreshold = GovernanceMinProposalThreshold;
    type MaxTitleLength = ConstU32<128>;
    type MaxCidLength = ConstU32<64>;
    type MaxActiveProposals = ConstU32<10>;
    type MinVotingPeriod = GovernanceMinVotingPeriod;
    type MinExecutionDelay = GovernanceMinExecutionDelay;
    type TimeWeightFullPeriod = ConstU32<{ 30 * DAYS }>; // 30 天达到最大乘数
    type TimeWeightMaxMultiplier = ConstU32<30000>; // 最大 3x 投票权
    type MaxDelegatorsPerDelegate = ConstU32<100>;
    type MultiLevelWriter = pallet_commission_multi_level::Pallet<Runtime>;
    type TeamWriter = pallet_commission_team::Pallet<Runtime>;
    type PoolRewardWriter = pallet_commission_pool_reward::Pallet<Runtime>;
    type ProductProvider = pallet_entity_product::Pallet<Runtime>;
    type DisclosureProvider = pallet_entity_disclosure::Pallet<Runtime>;
    // Phase 4.2 → Phase 5.2: 领域治理执行 Port（已接线到各 pallet 实现）
    type MarketGovernance = EntityMarket;
    type CommissionGovernance = CommissionCore;
    type SingleLineGovernance = CommissionSingleLine;
    type KycGovernance = EntityKyc;
    type ShopGovernance = GovernanceShopBridge;
    type TokenGovernance = EntityToken;
    // Phase 4.3: 资金保护
    type TreasuryPort = EntityRegistry;
    type ProposalCooldown = ConstU32<{ 1 * DAYS }>; // 1天冷却期
    type EmergencyOrigin = RootOrTechnicalMajority;
    type MaxVotingPeriod = ConstU32<{ 30 * DAYS }>; // 最大投票期30天
    type MaxExecutionDelay = ConstU32<{ 14 * DAYS }>;
    type PricingProvider = EntityPricingProvider;
    type MinTreasuryThresholdUsdt = ConstU64<50_000_000>; // 50 USDT
    type WeightInfo = pallet_entity_governance::weights::SubstrateWeight<Runtime>;
}

/// 桥接：KycChecker → pallet-entity-kyc::can_participate_in_entity
/// Entity 未配置 EntityRequirements 或 mandatory=false 时返回 true
pub struct MemberKycBridge;
impl pallet_entity_member::KycChecker<AccountId> for MemberKycBridge {
    fn is_kyc_passed(entity_id: u64, account: &AccountId) -> bool {
        pallet_entity_kyc::Pallet::<Runtime>::can_participate_in_entity(account, entity_id)
    }
}

impl pallet_entity_member::Config for Runtime {
    type EntityProvider = EntityRegistry;
    type ShopProvider = EntityShop;
    type MaxDirectReferrals = ConstU32<1000>;
    type MaxCustomLevels = ConstU32<10>;
    type MaxUpgradeRules = ConstU32<50>;
    type MaxUpgradeHistory = ConstU32<100>;
    type PendingMemberExpiry = ConstU32<100800>; // 7 days × 24h × 60min × 60s / 6s = 100800 blocks
    type KycChecker = MemberKycBridge;
    type OnMemberRemoved = pallet_commission_pool_reward::Pallet<Runtime>;
    type OnMemberLevelChanged = pallet_commission_pool_reward::Pallet<Runtime>;
    type OnMemberTeamChanged = pallet_commission_pool_reward::Pallet<Runtime>;
    type OnLevelRemoved = pallet_commission_single_line::Pallet<Runtime>;
    type WeightInfo = pallet_entity_member::weights::SubstrateWeight<Runtime>;
}

/// 桥接：EntityReferrerProvider → EntityRegistry::entity_referrer()
pub struct EntityReferrerBridge;

impl pallet_commission_common::EntityReferrerProvider<AccountId> for EntityReferrerBridge {
    fn entity_referrer(entity_id: u64) -> Option<AccountId> {
        pallet_entity_registry::EntityReferrer::<Runtime>::get(entity_id)
    }
    fn referrer_bound_at(entity_id: u64) -> Option<u64> {
        pallet_entity_registry::EntityReferrerBoundAt::<Runtime>::get(entity_id).map(|b| b as u64)
    }
}

/// H3 桥接：ParticipationGuard → pallet-entity-kyc::can_participate_in_entity
/// Entity 未配置 EntityRequirements 或 mandatory=false 时返回 true（允许所有操作）
pub struct KycParticipationGuard;

impl pallet_commission_core::ParticipationGuard<AccountId> for KycParticipationGuard {
    fn can_participate(entity_id: u64, account: &AccountId) -> bool {
        pallet_entity_kyc::Pallet::<Runtime>::can_participate_in_entity(account, entity_id)
    }
}

impl pallet_commission_core::Config for Runtime {
    type Currency = Balances;
    type ShopProvider = EntityShop;
    type EntityProvider = EntityRegistry;
    type MemberProvider = EntityMemberProvider;
    type ReferralPlugin = crate::CommissionReferral;
    type MultiLevelPlugin = crate::CommissionMultiLevel;
    type LevelDiffPlugin = crate::CommissionLevelDiff;
    type SingleLinePlugin = crate::CommissionSingleLine;
    type TeamPlugin = crate::CommissionTeam;
    type ReferralWriter = crate::CommissionReferral;
    type MultiLevelWriter = crate::CommissionMultiLevel;
    type LevelDiffWriter = crate::CommissionLevelDiff;
    type TeamWriter = crate::CommissionTeam;
    type PoolRewardWriter = crate::CommissionPoolReward;
    type EntityReferrerProvider = EntityReferrerBridge;
    type PlatformAccount = EntityPlatformAccount;
    type TreasuryAccount = TreasuryAccountId;
    type ReferrerShareBps = ConstU16<5000>; // 50% of platform fee → referrer

    type MaxCommissionRecordsPerOrder = ConstU32<200>;
    type MaxCustomLevels = ConstU32<10>;
    type ParticipationGuard = KycParticipationGuard;
    type ReferrerProtectionPeriod = ConstU64<10_512_000>; // ~2 years @6s/block
                                                          // Token 多资产扩展
    type TokenBalance = u128;
    type TokenReferralPlugin = crate::CommissionReferral;
    type TokenMultiLevelPlugin = crate::CommissionMultiLevel;
    type TokenLevelDiffPlugin = crate::CommissionLevelDiff;
    type TokenSingleLinePlugin = crate::CommissionSingleLine;
    type TokenTeamPlugin = crate::CommissionTeam;
    type TokenTransferProvider = TokenTransferProviderBridge;
    type MaxWithdrawalRecords = ConstU32<50>;
    type MaxMemberOrderIds = ConstU32<100>;
    // 子模块查询（供 Runtime API 聚合）
    type MultiLevelQuery = crate::CommissionMultiLevel;
    type MultiLevelStatsRollback = crate::CommissionMultiLevel;
    type TeamQuery = crate::CommissionTeam;
    type SingleLineQuery = crate::CommissionSingleLine;
    type LevelDiffQuery = crate::CommissionLevelDiff;
    type PoolRewardQuery = crate::CommissionPoolReward;
    type ReferralQuery = crate::CommissionReferral;
    type WeightInfo = pallet_commission_core::weights::SubstrateWeight<Runtime>;
    type GovernanceProvider = EntityGovernance;
    type Loyalty = LoyaltyBridge;
    type LoyaltyToken = LoyaltyBridge;
    type PlatformFeeRate = PlatformFeeRateBridge;
    type PoolFundingCallback = crate::CommissionPoolReward;
    type FundProtectionQuery = EntityGovernance;
    type PricingProvider = EntityPricingProvider;
    type TokenPriceProvider = EntityMarket;
    type AutoRepurchase = AutoRepurchaseBridge;
    /// 购物余额 TTL 最小值：100_800 ≈ 7 天（6 秒出块）
    type MinShoppingBalanceTtlBlocks = ConstU32<100_800>;
    /// HB-WD-01: 跨链佣金提现端口（withdrawal 部分桥出为原生 NEX）
    type CrossChainPayout = ismp::NexusCommissionPayout;
}

impl pallet_commission_referral::Config for Runtime {
    type Currency = Balances;
    type MemberProvider = EntityMemberProvider;
    type EntityProvider = EntityRegistry;
    type BudgetCapProvider = CommissionCore;
    type MaxTotalReferralRate = ConstU16<10000>;
    type WeightInfo = pallet_commission_referral::weights::SubstrateWeight<Runtime>;
}

impl pallet_commission_multi_level::Config for Runtime {
    type MemberProvider = EntityMemberProvider;
    type EntityProvider = EntityRegistry;
    type BudgetCapProvider = CommissionCore;
    type MaxMultiLevels = ConstU32<1000>;
    type ConfigChangeDelay = ConstU32<100>;
    type WeightInfo = pallet_commission_multi_level::weights::SubstrateWeight<Runtime>;
    type MaxPayoutRecords = ConstU32<50>;
}

impl pallet_commission_pool_reward::Config for Runtime {
    type Currency = Balances;
    type MemberProvider = EntityMemberProvider;
    type EntityProvider = EntityRegistry;
    type PoolBalanceProvider = pallet_commission_core::Pallet<Runtime>;
    type MaxPoolRewardLevels = ConstU32<10>;
    type MaxClaimHistory = ConstU32<20>;
    // Token 多资产扩展
    type TokenBalance = u128;
    type TokenPoolBalanceProvider = pallet_commission_core::Pallet<Runtime>;
    type TokenTransferProvider = TokenTransferProviderBridge;
    type ExchangeRateProvider = EntityPricingProvider;
    type ParticipationGuard = KycParticipationGuard;
    type WeightInfo = pallet_commission_pool_reward::weights::SubstrateWeight<Runtime>;
    // F4: 最小轮次间隔（测试用 300 块 ~30分钟 @6s/block；生产改回 14400 ~1天）
    type MinRoundDuration = ConstU32<7200>;
    // F10: 每个 Entity 保留最近 20 轮历史
    type MaxRoundHistory = ConstU32<20>;
    // F12: 池奖励领取回调（暂用空实现）
    type ClaimCallback = ();
    type ConfigChangeDelay = ConstU32<14400>; // ~1天 @6s/block
                                              // 自动轮转
    type MaxActivePoolRewardEntities = ConstU32<1000>;
    type MaxAutoRotatePerBlock = ConstU32<10>;
    type MaxFundingRecords = ConstU32<50>;
}

impl pallet_commission_level_diff::Config for Runtime {
    type Currency = Balances;
    type MemberProvider = EntityMemberProvider;
    type EntityProvider = EntityRegistry;
    type BudgetCapProvider = CommissionCore;
    type MaxCustomLevels = ConstU32<10>;
    type WeightInfo = pallet_commission_level_diff::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

/// 桥接：CommissionCore 的 MemberCommissionStats 作为 SingleLine 的 StatsProvider
pub struct SingleLineStatsFromCore;

impl pallet_commission_single_line::pallet::SingleLineStatsProvider<AccountId, Balance>
    for SingleLineStatsFromCore
{
    fn get_member_stats(
        entity_id: u64,
        account: &AccountId,
    ) -> pallet_commission_common::MemberCommissionStatsData<Balance> {
        pallet_commission_core::MemberCommissionStats::<Runtime>::get(entity_id, account)
    }
}

/// 桥接：EntityMember 的会员等级 → SingleLine 的 MemberLevelProvider
pub struct SingleLineLevelFromMember;

impl pallet_commission_single_line::pallet::SingleLineMemberLevelProvider<AccountId>
    for SingleLineLevelFromMember
{
    fn custom_level_id(entity_id: u64, account: &AccountId) -> u8 {
        pallet_entity_member::Pallet::<Runtime>::get_effective_level_by_entity(entity_id, account)
    }
    fn custom_level_count(entity_id: u64) -> u8 {
        pallet_entity_member::Pallet::<Runtime>::custom_level_count_by_entity(entity_id)
    }
}

impl pallet_commission_team::Config for Runtime {
    type Currency = Balances;
    type MemberProvider = EntityMemberProvider;
    type EntityProvider = EntityRegistry;
    type BudgetCapProvider = CommissionCore;
    type MaxTeamTiers = ConstU32<10>;
    type WeightInfo = pallet_commission_team::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

impl pallet_commission_single_line::Config for Runtime {
    type Currency = Balances;
    type StatsProvider = SingleLineStatsFromCore;
    type MemberLevelProvider = SingleLineLevelFromMember;
    type EntityProvider = EntityRegistry;
    type MemberProvider = EntityMemberProvider;
    type BudgetCapProvider = CommissionCore;
    type MaxSingleLineLength = ConstU32<200>;
    type WeightInfo = pallet_commission_single_line::weights::SubstrateWeight<Runtime>;
    type ConfigChangeDelay = ConstU32<100>;
    type MaxSegmentCount = ConstU32<500>;
    type MaxTotalRateBps = ConstU32<10000>; // 100%
    type MaxConfigChangeLogs = ConstU32<100>;
    type MaxPayoutRecords = ConstU32<50>;
}

impl pallet_entity_market::Config for Runtime {
    type Currency = Balances;
    type Balance = Balance;
    type TokenBalance = Balance;
    type EntityProvider = EntityRegistry;
    type TokenProvider = EntityToken;
    type DefaultOrderTTL = ConstU32<{ 7 * 24 * 600 }>; // 7 天
    type MaxActiveOrdersPerUser = ConstU32<100>;
    type BlocksPerHour = ConstU32<600>;
    type BlocksPerDay = ConstU32<{ 24 * 600 }>;
    type BlocksPerWeek = ConstU32<{ 7 * 24 * 600 }>;
    type CircuitBreakerDuration = ConstU32<600>; // 1 小时
    type DisclosureProvider = EntityDisclosure;
    type KycProvider = EntityKyc;
    type MaxTradeHistoryPerUser = ConstU32<200>;
    type MaxOrderHistoryPerUser = ConstU32<200>;
    type PricingProvider = EntityPricingProvider;
    type MaxOrderBookSize = ConstU32<1000>;
    type WeightInfo = pallet_entity_market::weights::SubstrateWeight<Runtime>;
}

// ============================================================================
// Entity Pallets Config (Phase 6-8 新模块)
// ============================================================================

parameter_types! {
    // KYC 有效期
    pub const BasicKycValidity: BlockNumber = 525600;      // ~1 年
    pub const StandardKycValidity: BlockNumber = 262800;   // ~6 个月
    pub const EnhancedKycValidity: BlockNumber = 525600;   // ~1 年
    pub const InstitutionalKycValidity: BlockNumber = 1051200; // ~2 年
    // 披露间隔
    pub const BasicDisclosureInterval: BlockNumber = 5256000;    // ~1 年
    pub const StandardDisclosureInterval: BlockNumber = 1314000; // ~3 个月
    pub const EnhancedDisclosureInterval: BlockNumber = 438000;  // ~1 个月
    pub const MaxBlackoutDuration: BlockNumber = 100800;         // ~7 天 @ 6s/block
    pub const InsiderCooldownPeriod: BlockNumber = 14400;        // ~1 天 @ 6s/block
    pub const MajorHolderThreshold: u32 = 500;                   // 5% (basis points)
    pub const ViolationThreshold: u32 = 5;                       // 5 次违规标记高风险
}

impl pallet_entity_disclosure::Config for Runtime {
    type EntityProvider = EntityRegistry;
    type MaxCidLength = ConstU32<64>;
    type MaxInsiders = ConstU32<50>;
    type MaxDisclosureHistory = ConstU32<100>;
    type BasicDisclosureInterval = BasicDisclosureInterval;
    type StandardDisclosureInterval = StandardDisclosureInterval;
    type EnhancedDisclosureInterval = EnhancedDisclosureInterval;
    type MaxBlackoutDuration = MaxBlackoutDuration;
    type MaxAnnouncementHistory = ConstU32<200>;
    type MaxTitleLength = ConstU32<128>;
    type MaxPinnedAnnouncements = ConstU32<5>;
    type MaxInsiderRoleHistory = ConstU32<20>;
    type InsiderCooldownPeriod = InsiderCooldownPeriod;
    type MajorHolderThreshold = MajorHolderThreshold;
    type ViolationThreshold = ViolationThreshold;
    type MaxApprovers = ConstU32<10>;
    type MaxInsiderTransactionHistory = ConstU32<50>;
    type EmergencyBlackoutMultiplier = ConstU32<3>;
    type OnDisclosureViolation = DisclosureViolationHandler;
    type WeightInfo = pallet_entity_disclosure::weights::SubstrateWeight<Runtime>;
}

/// D-2: 披露违规处理器（当前为空实现，未来可触发实体暂停）
pub struct DisclosureViolationHandler;
impl pallet_entity_common::OnDisclosureViolation for DisclosureViolationHandler {
    fn on_violation_threshold_reached(_entity_id: u64, _violation_count: u32, _penalty_level: u8) {
        // 当前为空实现；未来可触发实体自动暂停等措施
    }
}

impl pallet_entity_kyc::Config for Runtime {
    type MaxCidLength = ConstU32<64>;
    type MaxProviderNameLength = ConstU32<64>;
    type MaxProviders = ConstU32<20>;
    type BasicKycValidity = BasicKycValidity;
    type StandardKycValidity = StandardKycValidity;
    type EnhancedKycValidity = EnhancedKycValidity;
    type InstitutionalKycValidity = InstitutionalKycValidity;
    type AdminOrigin = RootOrTechnicalMajority;
    type EntityProvider = EntityRegistry;
    type MaxHistoryEntries = ConstU32<50>;
    type PendingKycTimeout = ConstU32<{ 14400 * 7 }>; // ~7 天 (14400 blocks/天)
    type OnKycStatusChange = ();
    type MaxAuthorizedEntities = ConstU32<100>;
    type WeightInfo = pallet_entity_kyc::weights::SubstrateWeight<Runtime>;
}

/// TokenSale KYC 适配器（桥接 pallet-entity-kyc → KycChecker trait）
pub struct TokenSaleKycBridge;
impl pallet_entity_tokensale::KycChecker<AccountId> for TokenSaleKycBridge {
    fn kyc_level(entity_id: u64, account: &AccountId) -> u8 {
        EntityKyc::get_kyc_level(entity_id, account).as_u8()
    }
}

impl pallet_entity_tokensale::Config for Runtime {
    type Currency = Balances;
    type AssetId = u64;
    type EntityProvider = EntityRegistry;
    type TokenProvider = EntityToken;
    type KycChecker = TokenSaleKycBridge;
    type DisclosureProvider = EntityDisclosure;
    type MaxPaymentOptions = ConstU32<5>;
    type MaxWhitelistSize = ConstU32<1000>;
    type MaxRoundsHistory = ConstU32<50>;
    type MaxSubscriptionsPerRound = ConstU32<10000>;
    type MaxActiveRounds = ConstU32<20>;
    type RefundGracePeriod = ConstU32<{ 7 * DAYS }>;
    type MaxBatchRefund = ConstU32<100>;
    type WeightInfo = pallet_entity_tokensale::weights::SubstrateWeight<Runtime>;
}

impl pallet_entity_loyalty::Config for Runtime {
    type Currency = Balances;
    type ShopProvider = EntityShop;
    type EntityProvider = EntityRegistry;
    type TokenProvider = EntityTokenProvider;
    type ParticipationGuard = KycParticipationGuard;
    type TokenBalance = u128;
    type TokenTransferProvider = TokenTransferProviderBridge;
    type MaxPointsNameLength = ConstU32<32>;
    type MaxPointsSymbolLength = ConstU32<8>;
    type WeightInfo = pallet_entity_loyalty::weights::SubstrateWeight<Runtime>;
}

// ============================================================================
// GroupRobot Pallets Config
// ============================================================================

parameter_types! {
    /// TEE 证明有效期 ~24h @ 6s/block = 43200
    pub const GrAttestationValidityBlocks: BlockNumber = 43_200;
    /// 证明过期扫描间隔: 每 100 区块 (~10 分钟)
    pub const GrAttestationCheckInterval: BlockNumber = 100;
    /// 节点最低质押 NEX 兜底: 0.01 NEX（仅防零）
    pub const GrMinNodeStake: Balance = UNIT / 100;
    /// 节点退出冷却期: 1 天
    pub const GrExitCooldown: BlockNumber = DAYS;
    /// Era 长度: 1 天
    pub const GrEraLength: BlockNumber = DAYS;
    /// Basic 层级每 Era 费用 NEX 兜底: 0.01 NEX（仅防零）
    pub const GrBasicFeePerEra: Balance = UNIT / 100;
    /// Pro 层级每 Era 费用 NEX 兜底: 0.01 NEX（仅防零）
    pub const GrProFeePerEra: Balance = UNIT / 100;
    /// Enterprise 层级每 Era 费用 NEX 兜底: 0.01 NEX（仅防零）
    pub const GrEnterpriseFeePerEra: Balance = UNIT / 100;
    /// 仪式有效期: 180 天
    pub const GrCeremonyValidityBlocks: BlockNumber = 180 * DAYS;
    /// 仪式检查间隔: 每 1000 区块 (~100 分钟)
    pub const GrCeremonyCheckInterval: BlockNumber = 1000;
    /// Peer 心跳过期阈值: 2 小时 (~1200 区块 @ 6s/block)
    pub const GrPeerHeartbeatTimeout: BlockNumber = 1200;
}

impl pallet_grouprobot_registry::Config for Runtime {
    type MaxBotsPerOwner = ConstU32<20>;
    type MaxPlatformsPerCommunity = ConstU32<5>;
    type MaxPlatformBindingsPerUser = ConstU32<5>;
    type AttestationValidityBlocks = GrAttestationValidityBlocks;
    type AttestationCheckInterval = GrAttestationCheckInterval;
    type MaxQuoteLen = ConstU32<8192>;
    type MaxPeersPerBot = ConstU32<16>;
    type MaxEndpointLen = ConstU32<256>;
    type PeerHeartbeatTimeout = GrPeerHeartbeatTimeout;
    type Subscription = pallet_grouprobot_subscription::Pallet<Runtime>;
    type MaxOperatorNameLen = ConstU32<64>;
    type MaxOperatorContactLen = ConstU32<128>;
    type MaxBotsPerOperator = ConstU32<50>;
    type MaxUptimeEraHistory = ConstU32<365>;
    type WeightInfo = pallet_grouprobot_registry::weights::SubstrateWeight<Runtime>;
}

/// GroupRobot BotRegistry Bridge: 将 pallet-grouprobot-registry 桥接到 BotRegistryProvider trait
pub struct GrBotRegistryBridge;

impl pallet_grouprobot_primitives::BotRegistryProvider<AccountId> for GrBotRegistryBridge {
    fn is_bot_active(bot_id_hash: &[u8; 32]) -> bool {
        pallet_grouprobot_registry::Pallet::<Runtime>::is_bot_active(bot_id_hash)
    }
    fn is_tee_node(bot_id_hash: &[u8; 32]) -> bool {
        pallet_grouprobot_registry::Pallet::<Runtime>::is_tee_node(bot_id_hash)
    }
    fn has_dual_attestation(bot_id_hash: &[u8; 32]) -> bool {
        pallet_grouprobot_registry::Pallet::<Runtime>::has_dual_attestation(bot_id_hash)
    }
    fn is_attestation_fresh(bot_id_hash: &[u8; 32]) -> bool {
        pallet_grouprobot_registry::Pallet::<Runtime>::is_attestation_fresh(bot_id_hash)
    }
    fn bot_owner(bot_id_hash: &[u8; 32]) -> Option<AccountId> {
        pallet_grouprobot_registry::Pallet::<Runtime>::bot_owner(bot_id_hash)
    }
    fn bot_public_key(bot_id_hash: &[u8; 32]) -> Option<[u8; 32]> {
        pallet_grouprobot_registry::Pallet::<Runtime>::bot_public_key(bot_id_hash)
    }
    fn peer_count(bot_id_hash: &[u8; 32]) -> u32 {
        pallet_grouprobot_registry::Pallet::<Runtime>::peer_count(bot_id_hash)
    }
    fn bot_operator(bot_id_hash: &[u8; 32]) -> Option<AccountId> {
        pallet_grouprobot_registry::Pallet::<Runtime>::bot_operator_account(bot_id_hash)
    }
    fn bot_status(bot_id_hash: &[u8; 32]) -> Option<pallet_grouprobot_primitives::BotStatus> {
        pallet_grouprobot_registry::Bots::<Runtime>::get(bot_id_hash).map(|b| b.status)
    }
    fn attestation_level(bot_id_hash: &[u8; 32]) -> u8 {
        pallet_grouprobot_registry::Pallet::<Runtime>::attestation_level(bot_id_hash)
    }
    fn tee_type(bot_id_hash: &[u8; 32]) -> Option<pallet_grouprobot_primitives::TeeType> {
        pallet_grouprobot_registry::Pallet::<Runtime>::get_tee_type(bot_id_hash)
    }
}

parameter_types! {
    pub const GrSequenceTtlBlocks: BlockNumber = 14_400; // ~24h (6s/block)
    pub const GrMaxEraHistory: u64 = 365;               // 保留最近 365 个 Era
}

impl pallet_grouprobot_consensus::Config for Runtime {
    type Currency = Balances;
    type WeightInfo = pallet_grouprobot_consensus::weights::SubstrateWeight<Runtime>;
    type MaxActiveNodes = ConstU32<100>;
    type MinStake = GrMinNodeStake;
    type MinStakeUsd = ConstU64<15_000_000>; // 15 USDT（50 USDT × 30%）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type ExitCooldownPeriod = GrExitCooldown;
    type EraLength = GrEraLength;
    type SlashPercentage = ConstU32<10>;
    type BotRegistry = GrBotRegistryBridge;
    type SequenceTtlBlocks = GrSequenceTtlBlocks;
    type MaxSequenceCleanupPerBlock = ConstU32<20>;
    type SubscriptionSettler = pallet_grouprobot_subscription::Pallet<Runtime>;
    type RewardDistributor = pallet_grouprobot_rewards::Pallet<Runtime>;
    type Subscription = pallet_grouprobot_subscription::Pallet<Runtime>;
    type PeerUptimeRecorder = pallet_grouprobot_registry::Pallet<Runtime>;
    type OrphanRewardClaimer = pallet_grouprobot_rewards::Pallet<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = GrConsensusBenchHelper;
}

/// Benchmark helper: 直接写入 registry/subscription 存储, 绕过 extrinsic 前置检查
#[cfg(feature = "runtime-benchmarks")]
pub struct GrConsensusBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_grouprobot_consensus::BenchmarkHelper<Runtime> for GrConsensusBenchHelper {
    fn fund_account(who: &AccountId, amount: Balance) {
        use frame_support::traits::Currency;
        let _ = Balances::make_free_balance_be(who, amount);
    }

    fn setup_tee_bot(bot_id_hash: &pallet_grouprobot_primitives::BotIdHash, owner: &AccountId) {
        use pallet_grouprobot_primitives::{BotStatus, NodeType, TeeType};
        let now = frame_system::Pallet::<Runtime>::block_number();
        // 写入 BotInfo
        pallet_grouprobot_registry::Bots::<Runtime>::insert(
            bot_id_hash,
            pallet_grouprobot_registry::BotInfo::<Runtime> {
                owner: owner.clone(),
                bot_id_hash: *bot_id_hash,
                public_key: [0u8; 32],
                status: BotStatus::Active,
                registered_at: now,
                node_type: NodeType::TeeNode {
                    mrtd: [0u8; 48],
                    mrenclave: None,
                    tdx_attested_at: u64::from(now),
                    sgx_attested_at: None,
                    expires_at: u64::from(now) + 100_000u64,
                },
                community_count: 0,
            },
        );
        // 写入 AttestationRecordV2 (TEE 证明)
        pallet_grouprobot_registry::AttestationsV2::<Runtime>::insert(
            bot_id_hash,
            pallet_grouprobot_registry::AttestationRecordV2::<Runtime> {
                bot_id_hash: *bot_id_hash,
                primary_quote_hash: [0u8; 32],
                secondary_quote_hash: None,
                primary_measurement: [0u8; 48],
                mrenclave: None,
                tee_type: TeeType::Tdx,
                attester: owner.clone(),
                attested_at: now,
                expires_at: now + 100_000u32,
                is_dual_attestation: false,
                quote_verified: true,
                dcap_level: 3,
                api_server_mrtd: None,
                api_server_quote_hash: None,
            },
        );
    }

    fn setup_active_bot(bot_id_hash: &pallet_grouprobot_primitives::BotIdHash, owner: &AccountId) {
        use pallet_grouprobot_primitives::{BotStatus, NodeType};
        let now = frame_system::Pallet::<Runtime>::block_number();
        pallet_grouprobot_registry::Bots::<Runtime>::insert(
            bot_id_hash,
            pallet_grouprobot_registry::BotInfo::<Runtime> {
                owner: owner.clone(),
                bot_id_hash: *bot_id_hash,
                public_key: [0u8; 32],
                status: BotStatus::Active,
                registered_at: now,
                node_type: NodeType::StandardNode,
                community_count: 0,
            },
        );
        // 同时设置 BotOperator 使 bot_operator() 返回 owner
        pallet_grouprobot_registry::BotOperator::<Runtime>::insert(
            bot_id_hash,
            (
                owner.clone(),
                pallet_grouprobot_primitives::Platform::Telegram,
            ),
        );
    }

    fn setup_paid_subscription(bot_id_hash: &pallet_grouprobot_primitives::BotIdHash) {
        use pallet_grouprobot_primitives::{SubscriptionStatus, SubscriptionTier};
        let now = frame_system::Pallet::<Runtime>::block_number();
        pallet_grouprobot_subscription::Subscriptions::<Runtime>::insert(
            bot_id_hash,
            pallet_grouprobot_subscription::SubscriptionRecord::<Runtime> {
                owner: sp_runtime::AccountId32::new([0u8; 32]),
                bot_id_hash: *bot_id_hash,
                tier: SubscriptionTier::Basic,
                fee_per_era: 0u128,
                started_at: now,
                status: SubscriptionStatus::Active,
            },
        );
    }

    fn node_id(seed: u32) -> pallet_grouprobot_primitives::NodeId {
        use sp_core::Pair as _;
        let s = alloc::format!("//Node{}", seed);
        let pair = sp_core::ed25519::Pair::from_string(&s, None).expect("valid seed");
        pair.public().0
    }

    fn bot_id_hash(seed: u32) -> pallet_grouprobot_primitives::BotIdHash {
        let mut h = [0u8; 32];
        h[0] = seed as u8;
        h
    }

    fn sign_message(_node_seed: u32, msg: &[u8]) -> ([u8; 32], [u8; 64]) {
        let msg_hash = sp_core::blake2_256(msg);
        // Benchmark helper: produce deterministic dummy signature
        let mut sig = [0u8; 64];
        sig[..32].copy_from_slice(&msg_hash);
        (msg_hash, sig)
    }
}

/// 订阅 EraStartBlock 提供者: 从 consensus 读取
pub struct GrEraStartBlockProvider;
impl frame_support::traits::Get<BlockNumber> for GrEraStartBlockProvider {
    fn get() -> BlockNumber {
        pallet_grouprobot_consensus::EraStartBlock::<Runtime>::get()
    }
}

/// 当前 Era 提供者: 从 consensus 读取
pub struct GrCurrentEraProvider;
impl frame_support::traits::Get<u64> for GrCurrentEraProvider {
    fn get() -> u64 {
        pallet_grouprobot_consensus::CurrentEra::<Runtime>::get()
    }
}

impl pallet_grouprobot_subscription::Config for Runtime {
    type Currency = Balances;
    type BotRegistry = GrBotRegistryBridge;
    type BasicFeePerEra = GrBasicFeePerEra;
    type BasicFeePerEraUsd = ConstU64<10_000>; // ≈0.01 USDT/天（1 USDT/月 × 30% / 30天）
    type ProFeePerEra = GrProFeePerEra;
    type ProFeePerEraUsd = ConstU64<20_000>; // ≈0.02 USDT/天（2 USDT/月 × 30% / 30天）
    type EnterpriseFeePerEra = GrEnterpriseFeePerEra;
    type EnterpriseFeePerEraUsd = ConstU64<50_000>; // ≈0.05 USDT/天（5 USDT/月 × 30% / 30天）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type TreasuryAccount = TreasuryAccountId;
    type RewardPoolAccount = RewardPoolAccountId;
    type MaxSubscriptionSettlePerEra = ConstU32<200>;
    type EraLength = GrEraLength;
    type EraStartBlockProvider = GrEraStartBlockProvider;
    type CurrentEraProvider = GrCurrentEraProvider;
    type AdDelivery = AdsDeliveryBridge;
    type AdBasicThreshold = ConstU32<3>;
    type AdProThreshold = ConstU32<6>;
    type AdEnterpriseThreshold = ConstU32<11>;
    type MaxUnderdeliveryEras = ConstU8<3>;
    type WeightInfo = pallet_grouprobot_subscription::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

/// NodeConsensus Bridge: 从 consensus pallet 读取节点信息
pub struct GrNodeConsensusBridge;
impl pallet_grouprobot_primitives::NodeConsensusProvider<AccountId> for GrNodeConsensusBridge {
    fn is_node_active(node_id: &[u8; 32]) -> bool {
        pallet_grouprobot_consensus::Pallet::<Runtime>::is_node_active(node_id)
    }
    fn node_operator(node_id: &[u8; 32]) -> Option<AccountId> {
        pallet_grouprobot_consensus::Pallet::<Runtime>::node_operator(node_id)
    }
    fn is_tee_node_by_operator(operator: &AccountId) -> bool {
        pallet_grouprobot_consensus::Pallet::<Runtime>::is_tee_node_by_operator(operator)
    }
}

impl pallet_grouprobot_rewards::Config for Runtime {
    type Currency = Balances;
    type NodeConsensus = GrNodeConsensusBridge;
    type BotRegistry = GrBotRegistryBridge;
    type RewardPoolAccount = RewardPoolAccountId;
    type MaxEraHistory = GrMaxEraHistory;
    type MaxBatchClaim = ConstU32<20>;
    type WeightInfo = pallet_grouprobot_rewards::weights::SubstrateWeight;
}

impl pallet_grouprobot_community::Config for Runtime {
    type MaxLogsPerCommunity = ConstU32<10000>;
    type ReputationCooldown = ConstU32<100>;
    type MaxReputationDelta = ConstU32<1000>;
    type MaxBatchSize = ConstU32<50>;
    type BlocksPerDay = ConstU32<14_400>;
    type WeightInfo = pallet_grouprobot_community::weights::SubstrateWeight<Runtime>;
    type BotRegistry = GrBotRegistryBridge;
    type Subscription = pallet_grouprobot_subscription::Pallet<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

impl pallet_grouprobot_ceremony::Config for Runtime {
    type MaxParticipants = ConstU32<20>;
    type MaxCeremonyHistory = ConstU32<10>;
    type CeremonyValidityBlocks = GrCeremonyValidityBlocks;
    type CeremonyCheckInterval = GrCeremonyCheckInterval;
    type MaxProcessPerBlock = ConstU32<50>;
    type BotRegistry = GrBotRegistryBridge;
    type Subscription = pallet_grouprobot_subscription::Pallet<Runtime>;
    type WeightInfo = pallet_grouprobot_ceremony::weights::SubstrateWeight;
}

// ============================================================================
// Ads Pallets Config (模块化广告引擎)
// ============================================================================

/// 桥接: pallet-ads-core → subscription 的 AdDeliveryProvider
/// ads-primitives::AdDeliveryCountProvider 与 grouprobot-primitives::AdDeliveryProvider 方法签名相同
pub struct AdsDeliveryBridge;

impl pallet_grouprobot_primitives::AdDeliveryProvider for AdsDeliveryBridge {
    fn era_delivery_count(
        community_id_hash: &pallet_grouprobot_primitives::CommunityIdHash,
    ) -> u32 {
        use pallet_ads_primitives::AdDeliveryCountProvider;
        pallet_ads_core::Pallet::<Runtime>::era_delivery_count(community_id_hash)
    }
    fn reset_era_deliveries(community_id_hash: &pallet_grouprobot_primitives::CommunityIdHash) {
        use pallet_ads_primitives::AdDeliveryCountProvider;
        pallet_ads_core::Pallet::<Runtime>::reset_era_deliveries(community_id_hash)
    }
}

parameter_types! {
    /// 最低 CPM 出价: 0.001 NEX
    pub const AdsMinBidPerMille: Balance = UNIT / 1000;
    /// 最低 CPC 出价: 0.0005 NEX
    pub const AdsMinBidPerClick: Balance = UNIT / 2000;
    /// 私有广告注册费 NEX 兜底: 0.01 NEX（仅防零）
    pub const AdsPrivateAdRegistrationFee: Balance = UNIT / 100;
}

impl pallet_ads_core::pallet::Config for Runtime {
    type Currency = Balances;
    type MaxAdTextLength = ConstU32<256>;
    type MaxAdUrlLength = ConstU32<512>;
    type MaxReceiptsPerPlacement = ConstU32<1000>;
    type MaxAdvertiserBlacklist = ConstU32<100>;
    type MaxAdvertiserWhitelist = ConstU32<100>;
    type MaxPlacementBlacklist = ConstU32<100>;
    type MaxPlacementWhitelist = ConstU32<100>;
    type MinBidPerMille = AdsMinBidPerMille;
    type MinAudienceSize = ConstU32<10>;
    type AdSlashPercentage = ConstU32<30>;
    type TreasuryAccount = TreasuryAccountId;
    // 适配层: 路由分发到 Entity 或 GroupRobot
    type DeliveryVerifier = pallet_ads_router::AdsRouter<Runtime>;
    type ClickVerifier = pallet_ads_router::AdsRouter<Runtime>;
    type MinBidPerClick = AdsMinBidPerClick;
    type PlacementAdmin = pallet_ads_router::AdsRouter<Runtime>;
    type RevenueDistributor = pallet_ads_router::AdsRouter<Runtime>;
    type PrivateAdRegistrationFee = AdsPrivateAdRegistrationFee;
    type PrivateAdRegistrationFeeUsd = ConstU64<150_000>; // 0.15 USDT（0.5 USDT × 30%）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type SettlementIncentiveBps = ConstU32<10>; // 0.1%
    type MaxCampaignsPerAdvertiser = ConstU32<100>;
    type MaxTargetsPerCampaign = ConstU32<20>;
    type ReceiptConfirmationWindow = ConstU32<7200>; // ≈ 12h @ 6s/block
    type AdvertiserReferralRate = ConstU32<500>; // 5% of platform share
    type MaxReferredAdvertisers = ConstU32<100>;
    type WeightInfo = pallet_ads_core::weights::SubstrateWeight;
    type MaxActiveApprovedCampaigns = ConstU32<1000>;
    type MaxCampaignsByDeliveryType = ConstU32<500>;
    type MaxCampaignsForPlacement = ConstU32<100>;
}

impl pallet_ads_grouprobot::pallet::Config for Runtime {
    type Currency = Balances;
    type NodeConsensus = GrNodeConsensusBridge;
    type Subscription = pallet_grouprobot_subscription::Pallet<Runtime>;
    type RewardPool = pallet_grouprobot_rewards::Pallet<Runtime>;
    type BotRegistry = GrBotRegistryBridge;
    type TreasuryAccount = TreasuryAccountId;
    type RewardPoolAccount = RewardPoolAccountId;
    type AudienceSurgeThresholdPct = ConstU32<100>; // 允许 100% audience 增长
    type NodeDeviationThresholdPct = ConstU32<20>; // 20% 多节点偏差
    type AdSlashPercentage = ConstU32<30>; // 30% slash
    type UnbondingPeriod = ConstU32<14_400>; // ~24h @ 6s/block
    type StakerRewardPct = ConstU32<10>; // 质押者分成 10%
    type MaxStakersPerCommunity = ConstU32<200>;
    type WeightInfo = pallet_ads_grouprobot::weights::SubstrateWeight;
}

impl pallet_ads_entity::pallet::Config for Runtime {
    type WeightInfo = pallet_ads_entity::weights::SubstrateWeight<Runtime>;
    type Currency = Balances;
    type EntityProvider = pallet_entity_registry::Pallet<Runtime>;
    type ShopProvider = pallet_entity_shop::Pallet<Runtime>;
    type TreasuryAccount = TreasuryAccountId;
    type PlatformAdShareBps = ConstU16<2000>; // 平台 20%
    type AdPlacementDeposit = ConstU128<{ UNIT / 100 }>; // 0.01 NEX 兜底（仅防零）
    type AdPlacementDepositUsd = ConstU64<300_000>; // 0.3 USDT（1 USDT × 30%）
    type DepositCalculator =
        pallet_trading_common::DepositCalculatorImpl<TradingPricingProvider, Balance>;
    type MaxPlacementsPerEntity = ConstU32<20>;
    type DefaultDailyImpressionCap = ConstU32<10_000>;
    type BlocksPerDay = ConstU32<14_400>; // 24h @ 6s/block
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

// ============================================================================
// Inscription Pallet (创世铭文 — 只读，无 extrinsic)
// ============================================================================

impl pallet_inscription::Config for Runtime {}
