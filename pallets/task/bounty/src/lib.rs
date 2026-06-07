//! # Task Bounty Pallet / 任务悬赏模块
//!
//! Lifecycle orchestration for on-chain task bounties. The pallet reuses the
//! existing `pallet-dispute-escrow` for fund custody and `pallet-dispute-arbitration`
//! for dispute resolution; it introduces **no new fund primitives**.
//! 任务悬赏的链上生命周期编排。资金托管复用 `pallet-dispute-escrow`，争议仲裁复用
//! `pallet-dispute-arbitration`，本模块**不新增资金原语**。
//!
//! Design reference / 设计依据: `docs/TASK_BOUNTY_PALLET_DESIGN.md` (Part III, Phase 1 MVP).
//!
//! ## Fund ledgers / 资金账本
//! - Reward + platform fee live in escrow under `bounty_id` (= escrow key = arbitration id).
//!   赏金本金 + 平台费锁在 escrow 的 `bounty_id` 下（bounty_id = escrow key = arbitration id）。
//! - Solver stake is reserved per submission via `ReservableCurrency`, never entering escrow.
//!   求解方质押按 submission 各自 `reserve`，绝不进入 escrow。

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use frame_support::traits::{Currency, ReservableCurrency};
use scale_info::TypeInfo;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};

/// Balance type bound to the configured currency. / 绑定到所配置货币的余额类型。
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Settlement model. Single = one prize many compete; Quota = fixed unit reward × slots.
/// 结算模型。Single=单中标竞赛；Quota=定额众包多名额。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum BountyKind {
    /// Single-winner contest. / 单中标竞赛。
    Single,
    /// Quota crowdsourcing. / 定额众包。
    Quota,
}

/// Review mode deciding who triggers payout. MVP only supports `Manual`.
/// 验收方式，决定谁触发放款。MVP 仅支持 `Manual`。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum ReviewMode {
    /// Poster accepts manually. / 出资方人工验收。
    Manual,
    /// On-chain self-proof (e.g. referral). Reserved for later phase. / 链上自证，后续阶段。
    AutoOnReveal,
    /// Trusted oracle callback. Reserved for later phase. / 可信预言机回调，后续阶段。
    Oracle,
}

/// Bounty lifecycle state. / 悬赏生命周期状态。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum BountyState {
    Open,
    Disputed,
    Completed,
    Refunded,
    Cancelled,
}

/// Submission state. / 提交状态。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum SubmissionState {
    /// Stake reserved, deliverable not yet backfilled. / 已质押占坑，未回填交付物。
    Submitted,
    /// Deliverable backfilled, acceptable. / 已回填交付物，可被验收。
    Delivered,
    Accepted,
    Rejected,
    Withdrawn,
}

/// Role used when reading bounty-domain reputation. / 读取悬赏域声誉时的角色。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum BountyReputationRole {
    Poster,
    Solver,
}

/// Outcome handed back from the arbitration router after escrow settlement.
/// 仲裁路由在 escrow 结算后回传给本模块的裁决结果。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug)]
pub enum ArbitrationOutcome {
    /// Funds released to contested solver. / 放款给被争议 solver。
    Release,
    /// Funds refunded to poster. / 退款给 poster。
    Refund,
    /// Split bps to solver, rest to poster. / 按 bps 给 solver，余退 poster。
    Partial(u16),
}

/// Bounty record. / 悬赏主记录。
#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
#[scale_info(skip_type_params(T))]
pub struct Bounty<AccountId, Balance, BlockNumber> {
    /// Sponsor (= refund target, arbitration "buyer"). / 出资方（退款目标，仲裁 buyer）。
    pub poster: AccountId,
    pub kind: BountyKind,
    pub review_mode: ReviewMode,
    /// Precise-publishing category (off-chain template routing). / 精准发布类目。
    pub category: u16,
    /// Per-slot reward. / 单份赏金。
    pub reward: Balance,
    /// Number of payable slots (Single = 1). / 名额（Single=1）。
    pub slots: u32,
    /// Slots already paid out. / 已放款名额。
    pub filled: u32,
    /// Total platform fee locked alongside reward. / 与赏金一并锁定的平台费总额。
    pub fee: Balance,
    pub state: BountyState,
    pub submission_count: u32,
    /// Accepted solver for Single. / Single 的中标者。
    pub winner: Option<AccountId>,
    /// Contested solver (arbitration "seller"/release candidate). / 被争议方（仲裁 seller）。
    pub contested: Option<AccountId>,
    /// Optional ads campaign for paid ranking. / 可选竞价曝光 campaign。
    pub promotion: Option<u64>,
    pub created: BlockNumber,
    pub deadline: BlockNumber,
}

/// Submission record. / 提交记录。
#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub struct Submission<AccountId, Balance, Hash> {
    pub solver: AccountId,
    /// Reserved stake. / 已 reserve 的质押。
    pub stake: Balance,
    /// Optional commit-reveal commitment. / commit-reveal 承诺（可选）。
    pub commit: Option<Hash>,
    /// Backfilled deliverable pointer: evidence_id. / 回填交付物指针：evidence_id。
    pub evidence: Option<u64>,
    pub state: SubmissionState,
}

/// Auditable per-account counters for bounty reputation. / 悬赏声誉可审计计数器。
#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug, Default)]
pub struct BountyUserStats {
    pub poster_published: u32,
    pub poster_completed: u32,
    pub poster_refunded: u32,
    pub poster_dispute_lost: u32,
    pub solver_submitted: u32,
    pub solver_accepted: u32,
    pub solver_withdrawn: u32,
    pub solver_dispute_lost: u32,
    pub solver_slashed: u32,
}

/// Visibility policy for the poster's off-chain contact pointer. / 发布方链下联系方式的可见策略。
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub enum ContactVisibility {
    /// Revealed only after a submission is accepted. / 仅验收后可见。
    AfterAccept,
    /// Revealed once a solver submits. / solver 提交后可见。
    OnSubmit,
    /// Never auto-revealed; chat only. / 不自动公开，仅走聊天。
    Hidden,
}

/// Matchmaking metadata attached to a bounty. Never touches escrow.
/// 附加在悬赏上的撮合元数据。完全不参与 escrow 资金状态机。
///
/// The competitor "合作类型" (10-dimension multi-select) lives off-chain as JSON;
/// only its evidence pointer (`coop_profile_ref`) and an optional digest are stored
/// on-chain, plus the chain-relevant `region` for ground/offline tasks.
/// 竞品「合作类型」（十维多选）以 JSON 存于链下；链上仅存其证据指针
/// （`coop_profile_ref`）与可选摘要，以及地推/线下任务相关的 `region`。
#[derive(Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Debug)]
pub struct Meta<Hash> {
    /// Evidence id of the off-chain coop_profile JSON, owned by poster. / 链下 coop_profile JSON 的证据 id（发布方自有）。
    pub coop_profile_ref: u64,
    /// Optional Blake2 digest of canonical JSON, pinned at publish time. / 发布时上链的 canonical JSON 摘要（可选，防事后篡改）。
    pub coop_profile_digest: Option<Hash>,
    /// GB/T 2260 region code; mandatory for ground/offline categories. / GB/T 2260 地区码；地推/线下类目必填。
    pub region: Option<u32>,
    /// Encrypted PII pointer; no plaintext on-chain. / 加密 PII 指针，链上无明文。
    pub contact_ref: Option<u64>,
    pub contact_visibility: ContactVisibility,
}

/// Platform-level KYC inspection (bounty is not entity-scoped). / 平台级 KYC 查询（悬赏非 entity 域）。
pub trait KycInspect<AccountId> {
    /// KYC level of an account. / 账户的 KYC 等级。
    fn kyc_level(who: &AccountId) -> u8;
}

impl<AccountId> KycInspect<AccountId> for () {
    fn kyc_level(_who: &AccountId) -> u8 {
        0
    }
}

/// Ownership check for a `coop_profile_ref` evidence id. / 对 `coop_profile_ref` 证据 id 的归属校验。
///
/// Decoupled from `pallet-dispute-evidence`; the runtime supplies an adapter.
/// 与 `pallet-dispute-evidence` 解耦；由 runtime 提供适配器。
pub trait EvidenceOwnership<AccountId> {
    /// Whether `who` owns the evidence identified by `evidence_id`. / `who` 是否为该证据的归属者。
    fn is_owner(evidence_id: u64, who: &AccountId) -> bool;
}

impl<AccountId> EvidenceOwnership<AccountId> for () {
    fn is_owner(_evidence_id: u64, _who: &AccountId) -> bool {
        true
    }
}

/// Chat authorization port for the bounty scene. / 悬赏场景的聊天授权端口。
///
/// Decoupled from `pallet-chat-permission`; the runtime adapter maps these calls to
/// scene authorizations (`source = *b"taskbnty"`, `scene_id = bounty_id`). Calls are
/// best-effort: failures must never abort the bounty extrinsic.
/// 与 `pallet-chat-permission` 解耦；runtime 适配器将其映射为场景授权
/// （`source = *b"taskbnty"`，`scene_id = bounty_id`）。调用为尽力而为，失败不得中断悬赏交易。
pub trait ChatAuthorizer<AccountId> {
    /// Grant bidirectional chat between poster and solver for this bounty. / 为该悬赏授予发布方↔求解方双向聊天。
    fn grant(bounty_id: u64, poster: &AccountId, solver: &AccountId);
    /// Revoke the bounty-scene chat authorization for this pair. / 撤销该悬赏场景下这对用户的聊天授权。
    fn revoke(bounty_id: u64, poster: &AccountId, solver: &AccountId);
}

impl<AccountId> ChatAuthorizer<AccountId> for () {
    fn grant(_bounty_id: u64, _poster: &AccountId, _solver: &AccountId) {}
    fn revoke(_bounty_id: u64, _poster: &AccountId, _solver: &AccountId) {}
}

/// 悬赏系统通知端口（runtime 适配器桥接到 chat-core 的 System 通道）。
/// Bounty system-notification port; runtime adapter bridges to chat-core.
///
/// 与 `pallet-chat-core` 解耦；尽力而为，失败不回滚悬赏状态转移。`()` 为 no-op 默认。
/// Decoupled from chat-core; best-effort; `()` is the no-op default.
pub trait BountyNotifier<AccountId> {
    /// 向 `to` 推送悬赏系统通知（客户端本地化模板描述符）。
    /// Push a bounty system notice (client-localized template descriptor).
    fn notify(to: &AccountId, notice: alloc::vec::Vec<u8>);
}

impl<AccountId> BountyNotifier<AccountId> for () {
    fn notify(_to: &AccountId, _notice: alloc::vec::Vec<u8>) {}
}

/// Bounty-domain reputation inspection (derived 0..10000). / 悬赏域声誉查询（派生 0..10000）。
pub trait BountyReputationInspect<AccountId> {
    /// Derived reputation; neutral default when no record. / 派生声誉，无记录返回中性默认值。
    fn reputation_of(who: &AccountId, role: BountyReputationRole) -> u32;
}

/// Read-only info provider consumed by the arbitration router. / 供仲裁路由读取的只读信息端口。
pub trait BountyInfoProvider<AccountId, Balance> {
    /// Poster → arbitration "buyer" (refund target). / poster → 仲裁 buyer。
    fn poster(id: u64) -> Option<AccountId>;
    /// Contested solver → arbitration "seller" (release target). / 被争议方 → 仲裁 seller。
    fn contested_solver(id: u64) -> Option<AccountId>;
    /// Escrow-locked amount used as deposit base. / 用于押金基数的 escrow 锁定额。
    fn amount(id: u64) -> Option<Balance>;
    /// Whether `who` may dispute this bounty. / `who` 是否可对该悬赏发起争议。
    fn can_dispute(id: u64, who: &AccountId) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, PalletId};
    use frame_system::pallet_prelude::*;
    use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
    use sp_runtime::{
        traits::{AccountIdConversion, Hash, Saturating, Zero},
        Perbill,
    };

    type SubmissionOf<T> =
        Submission<<T as frame_system::Config>::AccountId, BalanceOf<T>, <T as frame_system::Config>::Hash>;
    type BountyOf<T> =
        Bounty<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;
    type MetaOf<T> = Meta<<T as frame_system::Config>::Hash>;

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Currency for solver stake reservation. / 用于求解方质押的货币。
        type Currency: ReservableCurrency<Self::AccountId>;

        /// Escrow port (reuses dispute-escrow). / 托管端口（复用 dispute-escrow）。
        type Escrow: EscrowTrait<Self::AccountId, BalanceOf<Self>>;

        /// Reserved id-space base; bounty_id doubles as escrow key and arbitration id.
        /// 预留 id 区间基址；bounty_id 同时作为 escrow key 与 arbitration id。
        #[pallet::constant]
        type EscrowIdOffset: Get<u64>;

        /// Pallet account, used when a unique sink account is needed. / 模块账户。
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Solver stake = reward × StakeBps / 10000. / 求解方质押比例（基点）。
        #[pallet::constant]
        type StakeBps: Get<u16>;

        /// Platform fee = reward × FeeBps / 10000. / 平台费比例（基点）。
        #[pallet::constant]
        type FeeBps: Get<u16>;

        /// Fee destination (treasury). / 平台费收款账户（国库）。
        type FeeCollector: Get<Self::AccountId>;

        /// Max submissions per bounty (weight bound). / 单悬赏最大提交数（权重上界）。
        #[pallet::constant]
        type MaxSubmissions: Get<u32>;

        /// Max slots for a Quota bounty. / Quota 最大名额。
        #[pallet::constant]
        type MaxSlots: Get<u32>;

        /// Minimum per-slot reward. / 最小单份赏金。
        #[pallet::constant]
        type MinReward: Get<BalanceOf<Self>>;

        /// Per-slot reward cap for Quota. / Quota 单份赏金上限。
        #[pallet::constant]
        type MaxQuotaUnitReward: Get<BalanceOf<Self>>;

        /// Default bounty duration in blocks. / 默认任务时长（块）。
        #[pallet::constant]
        type DefaultDuration: Get<BlockNumberFor<Self>>;

        /// Mandatory open window before accept. / accept 前的强制开放期。
        #[pallet::constant]
        type MinOpenWindow: Get<BlockNumberFor<Self>>;

        /// Min KYC level required for payout. / 放款所需最低 KYC 等级。
        #[pallet::constant]
        type MinKycLevelForPayout: Get<u8>;

        /// KYC port. / KYC 查询端口。
        type Kyc: KycInspect<Self::AccountId>;

        /// Evidence ownership port validating `coop_profile_ref`. / 校验 `coop_profile_ref` 的证据归属端口。
        type Evidence: EvidenceOwnership<Self::AccountId>;

        /// Category requiring a `region` (ground/offline promotion). / 强制填写 `region` 的类目（地推/线下）。
        #[pallet::constant]
        type GroundPromoCategory: Get<u16>;

        /// Chat authorization port (scene-based grant/revoke). / 聊天授权端口（基于场景的授予/撤销）。
        type Chat: ChatAuthorizer<Self::AccountId>;

        /// 悬赏系统通知端口（桥接到聊天 System 通道；尽力而为）。
        /// Bounty system-notification port (chat System channel; best-effort).
        type Notifier: BountyNotifier<Self::AccountId>;

        /// Bounty-domain reputation (this pallet implements it). / 悬赏域声誉（本模块自实现）。
        type BountyReputation: BountyReputationInspect<Self::AccountId>;

        /// Min solver reputation to submit. / 求解方提交所需最低声誉。
        #[pallet::constant]
        type MinSolverReputation: Get<u32>;

        /// Min poster reputation to create high-value bounty. / 发布高额赏金所需最低声誉。
        #[pallet::constant]
        type MinPosterReputation: Get<u32>;

        /// Reward threshold above which poster reputation is gated. / 触发 poster 声誉校验的阈值。
        #[pallet::constant]
        type PosterReputationRewardThreshold: Get<BalanceOf<Self>>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Global counter; values start above `EscrowIdOffset`. / 全局自增 id，从 `EscrowIdOffset` 之上取值。
    #[pallet::storage]
    pub type NextBountyId<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Bounty records. / 悬赏主记录。
    #[pallet::storage]
    pub type Bounties<T: Config> = StorageMap<_, Blake2_128Concat, u64, BountyOf<T>>;

    /// Submissions per bounty indexed by sequence. / 每悬赏下按序号的提交。
    #[pallet::storage]
    pub type Submissions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        u64,
        Twox64Concat,
        u32,
        SubmissionOf<T>,
    >;

    /// Quota dedup: paid solvers per bounty (one slot each). / Quota 去重：每悬赏已放款 solver。
    #[pallet::storage]
    pub type RewardedSolvers<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, u64, Blake2_128Concat, T::AccountId, ()>;

    /// Per-account bounty-domain stats. / 按账户的悬赏域统计。
    #[pallet::storage]
    pub type UserStats<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, BountyUserStats, ValueQuery>;

    /// Optional matchmaking metadata per bounty. / 每悬赏的可选撮合元数据。
    #[pallet::storage]
    pub type BountyMeta<T: Config> = StorageMap<_, Blake2_128Concat, u64, MetaOf<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A bounty was created. / 悬赏已创建。
        BountyCreated { id: u64, poster: T::AccountId, kind: BountyKind, reward: BalanceOf<T>, slots: u32 },
        /// A submission was made. / 已提交。
        Submitted { id: u64, index: u32, solver: T::AccountId },
        /// A deliverable was backfilled. / 已回填交付物。
        Delivered { id: u64, index: u32 },
        /// A submission was accepted and paid. / 提交被验收并放款。
        Accepted { id: u64, index: u32, solver: T::AccountId, reward: BalanceOf<T> },
        /// A bounty reached terminal Completed. / 悬赏整单完成。
        BountyCompleted { id: u64 },
        /// A submission was withdrawn. / 提交被撤回。
        SubmissionWithdrawn { id: u64, index: u32 },
        /// A bounty was cancelled by poster. / 悬赏被发布方取消。
        BountyCancelled { id: u64 },
        /// A bounty entered dispute. / 悬赏进入争议。
        BountyDisputed { id: u64, index: u32, contested: T::AccountId },
        /// A bounty expired and refunded. / 悬赏到期退款。
        BountyExpired { id: u64 },
        /// A dispute was settled by arbitration. / 争议经仲裁结算。
        DisputeSettled { id: u64, outcome: ArbitrationOutcome },
        /// Matchmaking metadata was set or updated. / 撮合元数据被设置或更新。
        MetaUpdated { id: u64 },
    }

    #[pallet::error]
    pub enum Error<T> {
        RewardTooLow,
        NotPoster,
        NotOpen,
        SubmissionFull,
        SelfSubmit,
        SubmissionNotFound,
        NotSolver,
        HasActiveSubmissions,
        NotYetExpired,
        BadState,
        OpenWindowNotElapsed,
        KycTooLow,
        NoDeliverable,
        CommitMismatch,
        MissingDeliverable,
        MissingSalt,
        BadSlots,
        QuotaUnitTooHigh,
        SlotsFull,
        AlreadyRewarded,
        SolverReputationTooLow,
        PosterReputationTooLow,
        ReviewModeUnsupported,
        /// `coop_profile_ref` is not an evidence owned by the poster. / `coop_profile_ref` 非发布方自有证据。
        BadCoopProfileRef,
        /// Ground/offline category requires a `region`. / 地推/线下类目必须填写 `region`。
        RegionRequired,
        Overflow,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Create a bounty and lock `reward × slots + fee` into escrow.
        /// 创建悬赏并将 `reward×slots + fee` 锁入托管。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::create_bounty())]
        pub fn create_bounty(
            origin: OriginFor<T>,
            kind: BountyKind,
            reward: BalanceOf<T>,
            slots: u32,
            category: u16,
            review_mode: Option<ReviewMode>,
            duration: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let poster = ensure_signed(origin)?;
            let review_mode = review_mode.unwrap_or(ReviewMode::Manual);
            ensure!(review_mode == ReviewMode::Manual, Error::<T>::ReviewModeUnsupported);
            ensure!(reward >= T::MinReward::get(), Error::<T>::RewardTooLow);

            match kind {
                BountyKind::Single => ensure!(slots == 1, Error::<T>::BadSlots),
                BountyKind::Quota => {
                    ensure!(slots > 1 && slots <= T::MaxSlots::get(), Error::<T>::BadSlots);
                    ensure!(reward <= T::MaxQuotaUnitReward::get(), Error::<T>::QuotaUnitTooHigh);
                }
            }

            // Total reward and fee. / 赏金总额与平台费。
            let total_reward = Self::mul_u32(reward, slots);
            let fee = Self::bps_of(total_reward, T::FeeBps::get());

            // Poster reputation gate for high-value bounties. / 大额悬赏的发布方声誉门槛。
            if total_reward >= T::PosterReputationRewardThreshold::get() {
                ensure!(
                    T::BountyReputation::reputation_of(&poster, BountyReputationRole::Poster)
                        >= T::MinPosterReputation::get(),
                    Error::<T>::PosterReputationTooLow
                );
            }

            let locked = total_reward.saturating_add(fee);
            let id = Self::next_id()?;
            T::Escrow::lock_from(&poster, id, locked)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let deadline = now.saturating_add(duration.unwrap_or_else(T::DefaultDuration::get));

            Bounties::<T>::insert(
                id,
                Bounty {
                    poster: poster.clone(),
                    kind,
                    review_mode,
                    category,
                    reward,
                    slots,
                    filled: 0,
                    fee,
                    state: BountyState::Open,
                    submission_count: 0,
                    winner: None,
                    contested: None,
                    promotion: None,
                    created: now,
                    deadline,
                },
            );

            UserStats::<T>::mutate(&poster, |s| s.poster_published = s.poster_published.saturating_add(1));
            Self::deposit_event(Event::BountyCreated { id, poster, kind, reward, slots });
            Ok(())
        }

        /// Submit to a bounty; reserve stake and optionally backfill the deliverable.
        /// 提交悬赏；reserve 质押并可选回填交付物。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::submit())]
        pub fn submit(
            origin: OriginFor<T>,
            bounty_id: u64,
            evidence: Option<u64>,
            commit: Option<T::Hash>,
        ) -> DispatchResult {
            let solver = ensure_signed(origin)?;
            let mut bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);
            ensure!(solver != bounty.poster, Error::<T>::SelfSubmit);
            ensure!(bounty.submission_count < T::MaxSubmissions::get(), Error::<T>::SubmissionFull);
            ensure!(evidence.is_some() || commit.is_some(), Error::<T>::MissingDeliverable);
            ensure!(
                T::BountyReputation::reputation_of(&solver, BountyReputationRole::Solver)
                    >= T::MinSolverReputation::get(),
                Error::<T>::SolverReputationTooLow
            );

            let stake = Self::bps_of(bounty.reward, T::StakeBps::get());
            T::Currency::reserve(&solver, stake)?;

            let index = bounty.submission_count;
            let state = if evidence.is_some() {
                SubmissionState::Delivered
            } else {
                SubmissionState::Submitted
            };
            Submissions::<T>::insert(
                bounty_id,
                index,
                Submission { solver: solver.clone(), stake, commit, evidence, state },
            );
            bounty.submission_count = index.saturating_add(1);
            Bounties::<T>::insert(bounty_id, &bounty);

            UserStats::<T>::mutate(&solver, |s| s.solver_submitted = s.solver_submitted.saturating_add(1));
            // Grant chat early when the poster opted into `OnSubmit` contact. / OnSubmit 策略下提交即开通聊天。
            if Self::chat_visibility(bounty_id) == ContactVisibility::OnSubmit {
                T::Chat::grant(bounty_id, &bounty.poster, &solver);
            }

            // 通知发布方：有新提交。
            T::Notifier::notify(
                &bounty.poster,
                Self::notice_bounty(b"bounty:submitted", bounty_id, &[index as u64]),
            );

            Self::deposit_event(Event::Submitted { id: bounty_id, index, solver });
            if state == SubmissionState::Delivered {
                Self::deposit_event(Event::Delivered { id: bounty_id, index });
            }
            Ok(())
        }

        /// Backfill the deliverable (commit-reveal or late delivery). / 回填交付物（揭示或迟交）。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::deliver())]
        pub fn deliver(
            origin: OriginFor<T>,
            bounty_id: u64,
            index: u32,
            evidence: u64,
            salt: Option<[u8; 32]>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(
                matches!(bounty.state, BountyState::Open | BountyState::Disputed),
                Error::<T>::BadState
            );
            Submissions::<T>::try_mutate(bounty_id, index, |maybe| -> DispatchResult {
                let sub = maybe.as_mut().ok_or(Error::<T>::SubmissionNotFound)?;
                ensure!(sub.solver == who, Error::<T>::NotSolver);
                ensure!(
                    matches!(sub.state, SubmissionState::Submitted | SubmissionState::Delivered),
                    Error::<T>::BadState
                );
                if let Some(commit) = sub.commit {
                    let salt = salt.ok_or(Error::<T>::MissingSalt)?;
                    let computed = T::Hashing::hash_of(&(evidence, salt, &who));
                    ensure!(computed == commit, Error::<T>::CommitMismatch);
                }
                sub.evidence = Some(evidence);
                sub.state = SubmissionState::Delivered;
                Ok(())
            })?;
            Self::deposit_event(Event::Delivered { id: bounty_id, index });
            Ok(())
        }

        /// Accept a delivered submission and release funds. / 验收已交付的提交并放款。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::accept())]
        pub fn accept(origin: OriginFor<T>, bounty_id: u64, index: u32) -> DispatchResult {
            let poster = ensure_signed(origin)?;
            let mut bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.poster == poster, Error::<T>::NotPoster);
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);

            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(
                now >= bounty.created.saturating_add(T::MinOpenWindow::get()),
                Error::<T>::OpenWindowNotElapsed
            );

            let sub = Submissions::<T>::get(bounty_id, index).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(sub.state == SubmissionState::Delivered, Error::<T>::NoDeliverable);
            ensure!(
                T::Kyc::kyc_level(&sub.solver) >= T::MinKycLevelForPayout::get(),
                Error::<T>::KycTooLow
            );

            match bounty.kind {
                BountyKind::Single => Self::accept_single(bounty_id, &mut bounty, index, &sub)?,
                BountyKind::Quota => Self::accept_quota(bounty_id, &mut bounty, index, &sub)?,
            }
            Ok(())
        }

        /// Withdraw an own submission and unreserve stake. / 撤回本人提交并解押。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::withdraw_submission())]
        pub fn withdraw_submission(origin: OriginFor<T>, bounty_id: u64, index: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Submissions::<T>::try_mutate(bounty_id, index, |maybe| -> DispatchResult {
                let sub = maybe.as_mut().ok_or(Error::<T>::SubmissionNotFound)?;
                ensure!(sub.solver == who, Error::<T>::NotSolver);
                ensure!(
                    matches!(sub.state, SubmissionState::Submitted | SubmissionState::Delivered),
                    Error::<T>::BadState
                );
                T::Currency::unreserve(&who, sub.stake);
                sub.state = SubmissionState::Withdrawn;
                Ok(())
            })?;
            // Revoke this solver's bounty-scene chat. / 撤销该求解方的悬赏聊天授权。
            if let Some(bounty) = Bounties::<T>::get(bounty_id) {
                T::Chat::revoke(bounty_id, &bounty.poster, &who);
            }
            UserStats::<T>::mutate(&who, |s| s.solver_withdrawn = s.solver_withdrawn.saturating_add(1));
            Self::deposit_event(Event::SubmissionWithdrawn { id: bounty_id, index });
            Ok(())
        }

        /// Cancel an open bounty with no active submissions; refund poster. / 取消无活跃提交的悬赏，退款。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::cancel_bounty())]
        pub fn cancel_bounty(origin: OriginFor<T>, bounty_id: u64) -> DispatchResult {
            let poster = ensure_signed(origin)?;
            let mut bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.poster == poster, Error::<T>::NotPoster);
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);
            ensure!(!Self::has_active_submissions(bounty_id, &bounty), Error::<T>::HasActiveSubmissions);

            T::Escrow::refund_all(bounty_id, &poster)?;
            bounty.state = BountyState::Cancelled;
            Bounties::<T>::insert(bounty_id, &bounty);
            Self::revoke_non_accepted_chat(bounty_id, &bounty);
            UserStats::<T>::mutate(&poster, |s| s.poster_refunded = s.poster_refunded.saturating_add(1));
            Self::deposit_event(Event::BountyCancelled { id: bounty_id });
            Ok(())
        }

        /// Open a dispute on a delivered submission; freeze escrow. / 对已交付提交发起争议并冻结托管。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::open_dispute())]
        pub fn open_dispute(origin: OriginFor<T>, bounty_id: u64, index: u32) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let mut bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);
            let sub = Submissions::<T>::get(bounty_id, index).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(sub.state == SubmissionState::Delivered, Error::<T>::NoDeliverable);
            ensure!(who == bounty.poster || who == sub.solver, Error::<T>::NotPoster);

            T::Escrow::set_disputed(bounty_id)?;
            bounty.contested = Some(sub.solver.clone());
            bounty.state = BountyState::Disputed;
            Bounties::<T>::insert(bounty_id, &bounty);

            // 通知对方：争议已开启（poster↔solver）。
            let notice = Self::notice_bounty(b"bounty:disputed", bounty_id, &[index as u64]);
            if who == bounty.poster {
                T::Notifier::notify(&sub.solver, notice);
            } else {
                T::Notifier::notify(&bounty.poster, notice);
            }

            Self::deposit_event(Event::BountyDisputed { id: bounty_id, index, contested: sub.solver });
            Ok(())
        }

        /// Permissionless refund after deadline. / 到期后无许可退款。
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::expire_bounty())]
        pub fn expire_bounty(origin: OriginFor<T>, bounty_id: u64) -> DispatchResult {
            ensure_signed(origin)?;
            let mut bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);
            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now >= bounty.deadline, Error::<T>::NotYetExpired);

            Self::unreserve_active(bounty_id, &bounty, None);
            T::Escrow::refund_all(bounty_id, &bounty.poster)?;
            bounty.state = BountyState::Refunded;
            let poster = bounty.poster.clone();
            Bounties::<T>::insert(bounty_id, &bounty);
            Self::revoke_non_accepted_chat(bounty_id, &bounty);
            UserStats::<T>::mutate(&poster, |s| s.poster_refunded = s.poster_refunded.saturating_add(1));

            T::Notifier::notify(&poster, Self::notice_bounty(b"bounty:expired", bounty_id, &[]));

            Self::deposit_event(Event::BountyExpired { id: bounty_id });
            Ok(())
        }

        /// Set or update off-chain matchmaking metadata (coop_profile / region / contact).
        /// 设置或更新链下撮合元数据（coop_profile / region / contact）。
        ///
        /// Poster-only, while the bounty is still `Open`. Validates evidence ownership of
        /// `coop_profile_ref` and enforces `region` for ground/offline categories. Never
        /// touches escrow. / 仅发布方、且悬赏处于 `Open` 时可调用；校验 `coop_profile_ref`
        /// 的证据归属，并对地推/线下类目强制 `region`；不触碰 escrow。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::set_meta())]
        pub fn set_meta(
            origin: OriginFor<T>,
            bounty_id: u64,
            coop_profile_ref: u64,
            coop_profile_digest: Option<T::Hash>,
            region: Option<u32>,
            contact_ref: Option<u64>,
            contact_visibility: ContactVisibility,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let bounty = Bounties::<T>::get(bounty_id).ok_or(Error::<T>::SubmissionNotFound)?;
            ensure!(bounty.poster == who, Error::<T>::NotPoster);
            ensure!(bounty.state == BountyState::Open, Error::<T>::NotOpen);
            ensure!(
                T::Evidence::is_owner(coop_profile_ref, &who),
                Error::<T>::BadCoopProfileRef
            );
            if bounty.category == T::GroundPromoCategory::get() {
                ensure!(region.is_some(), Error::<T>::RegionRequired);
            }
            BountyMeta::<T>::insert(
                bounty_id,
                Meta { coop_profile_ref, coop_profile_digest, region, contact_ref, contact_visibility },
            );
            Self::deposit_event(Event::MetaUpdated { id: bounty_id });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Derive the pallet sink account. / 派生模块账户。
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        /// Allocate the next bounty id within the reserved high range. / 在预留高位区间分配下一个 id。
        fn next_id() -> Result<u64, DispatchError> {
            NextBountyId::<T>::try_mutate(|n| -> Result<u64, DispatchError> {
                if *n < T::EscrowIdOffset::get() {
                    *n = T::EscrowIdOffset::get();
                }
                let id = *n;
                *n = n.checked_add(1).ok_or(Error::<T>::Overflow)?;
                Ok(id)
            })
        }

        /// `amount × bps / 10000`. / 基点计算。
        fn bps_of(amount: BalanceOf<T>, bps: u16) -> BalanceOf<T> {
            Perbill::from_rational(bps as u32, 10_000u32) * amount
        }

        /// `amount × n` via bounded saturating addition (n ≤ MaxSlots), avoiding
        /// `From<u32>`/`CheckedMul` bounds on the balance type.
        /// 通过有界饱和加法实现 `amount × n`（n ≤ MaxSlots），避免对余额类型的额外 trait 约束。
        fn mul_u32(amount: BalanceOf<T>, n: u32) -> BalanceOf<T> {
            let mut acc = BalanceOf::<T>::zero();
            let mut i = 0u32;
            while i < n {
                acc = acc.saturating_add(amount);
                i = i.saturating_add(1);
            }
            acc
        }

        /// Effective contact visibility (defaults to `AfterAccept` when no meta).
        /// 生效的联系可见策略（无 meta 时默认 `AfterAccept`）。
        fn chat_visibility(bounty_id: u64) -> ContactVisibility {
            BountyMeta::<T>::get(bounty_id)
                .map(|m| m.contact_visibility)
                .unwrap_or(ContactVisibility::AfterAccept)
        }

        /// `u64` → 十进制 ASCII（no_std 友好）。
        fn u64_ascii(mut n: u64) -> alloc::vec::Vec<u8> {
            if n == 0 {
                return alloc::vec![b'0'];
            }
            let mut buf = alloc::vec::Vec::new();
            while n > 0 {
                buf.push(b'0' + (n % 10) as u8);
                n /= 10;
            }
            buf.reverse();
            buf
        }

        /// 构造悬赏通知描述符：`{kind}:{bounty_id}[:part…]`。
        /// Build a bounty notice descriptor: `{kind}:{bounty_id}[:part…]`.
        pub(crate) fn notice_bounty(kind: &[u8], bounty_id: u64, parts: &[u64]) -> alloc::vec::Vec<u8> {
            let mut v = kind.to_vec();
            v.push(b':');
            v.extend_from_slice(&Self::u64_ascii(bounty_id));
            for p in parts {
                v.push(b':');
                v.extend_from_slice(&Self::u64_ascii(*p));
            }
            v
        }

        /// 仲裁结果编码（供通知 payload 使用）。/ Arbitration outcome code for notices.
        fn outcome_code(outcome: ArbitrationOutcome) -> u64 {
            match outcome {
                ArbitrationOutcome::Release => 0,
                ArbitrationOutcome::Refund => 1,
                ArbitrationOutcome::Partial(_) => 2,
            }
        }

        /// Revoke bounty-scene chat for every non-accepted submitter (losers / withdrawn).
        /// Accepted solvers keep their chat. / 撤销所有未被验收提交者的聊天；被验收方保留。
        fn revoke_non_accepted_chat(bounty_id: u64, bounty: &BountyOf<T>) {
            for i in 0..bounty.submission_count {
                if let Some(sub) = Submissions::<T>::get(bounty_id, i) {
                    if sub.state != SubmissionState::Accepted {
                        T::Chat::revoke(bounty_id, &bounty.poster, &sub.solver);
                    }
                }
            }
        }

        fn has_active_submissions(bounty_id: u64, bounty: &BountyOf<T>) -> bool {
            for i in 0..bounty.submission_count {
                if let Some(sub) = Submissions::<T>::get(bounty_id, i) {
                    if matches!(sub.state, SubmissionState::Submitted | SubmissionState::Delivered) {
                        return true;
                    }
                }
            }
            false
        }

        /// Unreserve every still-active submission, optionally skipping one index. / 解押所有活跃提交。
        fn unreserve_active(bounty_id: u64, bounty: &BountyOf<T>, skip: Option<u32>) {
            for i in 0..bounty.submission_count {
                if Some(i) == skip {
                    continue;
                }
                Submissions::<T>::mutate(bounty_id, i, |maybe| {
                    if let Some(sub) = maybe.as_mut() {
                        if matches!(sub.state, SubmissionState::Submitted | SubmissionState::Delivered) {
                            T::Currency::unreserve(&sub.solver, sub.stake);
                            sub.state = SubmissionState::Rejected;
                        }
                    }
                });
            }
        }

        fn accept_single(
            bounty_id: u64,
            bounty: &mut BountyOf<T>,
            index: u32,
            sub: &SubmissionOf<T>,
        ) -> DispatchResult {
            if !bounty.fee.is_zero() {
                T::Escrow::transfer_from_escrow(bounty_id, &T::FeeCollector::get(), bounty.fee)?;
            }
            T::Escrow::release_all(bounty_id, &sub.solver)?;

            // Unreserve all stakes (winner + losers). / 解押全部质押。
            Self::unreserve_active(bounty_id, bounty, Some(index));
            T::Currency::unreserve(&sub.solver, sub.stake);
            Submissions::<T>::mutate(bounty_id, index, |m| {
                if let Some(s) = m.as_mut() {
                    s.state = SubmissionState::Accepted;
                }
            });

            bounty.filled = 1;
            bounty.winner = Some(sub.solver.clone());
            bounty.state = BountyState::Completed;
            Bounties::<T>::insert(bounty_id, &*bounty);

            // Winner keeps chat (unless Hidden); revoke losers' scene authorizations. / 中标者保留聊天，落选撤销。
            if Self::chat_visibility(bounty_id) != ContactVisibility::Hidden {
                T::Chat::grant(bounty_id, &bounty.poster, &sub.solver);
            }
            Self::revoke_non_accepted_chat(bounty_id, bounty);

            UserStats::<T>::mutate(&sub.solver, |s| s.solver_accepted = s.solver_accepted.saturating_add(1));
            UserStats::<T>::mutate(&bounty.poster, |s| s.poster_completed = s.poster_completed.saturating_add(1));
            T::Notifier::notify(
                &sub.solver,
                Self::notice_bounty(b"bounty:accepted", bounty_id, &[index as u64]),
            );
            T::Notifier::notify(
                &bounty.poster,
                Self::notice_bounty(b"bounty:completed", bounty_id, &[]),
            );

            Self::deposit_event(Event::Accepted {
                id: bounty_id,
                index,
                solver: sub.solver.clone(),
                reward: bounty.reward,
            });
            Self::deposit_event(Event::BountyCompleted { id: bounty_id });
            Ok(())
        }

        fn accept_quota(
            bounty_id: u64,
            bounty: &mut BountyOf<T>,
            index: u32,
            sub: &SubmissionOf<T>,
        ) -> DispatchResult {
            ensure!(bounty.filled < bounty.slots, Error::<T>::SlotsFull);
            ensure!(
                !RewardedSolvers::<T>::contains_key(bounty_id, &sub.solver),
                Error::<T>::AlreadyRewarded
            );

            let per_slot_fee = Self::bps_of(bounty.reward, T::FeeBps::get());
            if !per_slot_fee.is_zero() {
                T::Escrow::transfer_from_escrow(bounty_id, &T::FeeCollector::get(), per_slot_fee)?;
            }
            T::Escrow::release_partial(bounty_id, &sub.solver, bounty.reward)?;
            T::Currency::unreserve(&sub.solver, sub.stake);

            RewardedSolvers::<T>::insert(bounty_id, &sub.solver, ());
            Submissions::<T>::mutate(bounty_id, index, |m| {
                if let Some(s) = m.as_mut() {
                    s.state = SubmissionState::Accepted;
                }
            });
            bounty.filled = bounty.filled.saturating_add(1);

            // Accepted quota solver keeps chat (unless Hidden). / 被验收的众包名额保留聊天。
            if Self::chat_visibility(bounty_id) != ContactVisibility::Hidden {
                T::Chat::grant(bounty_id, &bounty.poster, &sub.solver);
            }

            UserStats::<T>::mutate(&sub.solver, |s| s.solver_accepted = s.solver_accepted.saturating_add(1));

            T::Notifier::notify(
                &sub.solver,
                Self::notice_bounty(b"bounty:accepted", bounty_id, &[index as u64]),
            );

            Self::deposit_event(Event::Accepted {
                id: bounty_id,
                index,
                solver: sub.solver.clone(),
                reward: bounty.reward,
            });

            if bounty.filled == bounty.slots {
                // Drain rounding dust + unreserve leftovers, then complete. / 清残余、解押、完成。
                Self::unreserve_active(bounty_id, bounty, None);
                Self::revoke_non_accepted_chat(bounty_id, bounty);
                let rem = T::Escrow::amount_of(bounty_id);
                if !rem.is_zero() {
                    T::Escrow::transfer_from_escrow(bounty_id, &T::FeeCollector::get(), rem)?;
                }
                bounty.state = BountyState::Completed;
                UserStats::<T>::mutate(&bounty.poster, |s| {
                    s.poster_completed = s.poster_completed.saturating_add(1)
                });
                T::Notifier::notify(
                    &bounty.poster,
                    Self::notice_bounty(b"bounty:completed", bounty_id, &[]),
                );
                Bounties::<T>::insert(bounty_id, &*bounty);
                Self::deposit_event(Event::BountyCompleted { id: bounty_id });
            } else {
                Bounties::<T>::insert(bounty_id, &*bounty);
            }
            Ok(())
        }

        /// Update bounty state + stats after the arbitration router settled escrow.
        /// 仲裁路由完成 escrow 结算后，更新悬赏状态与统计。
        ///
        /// Intended to be called from the runtime `ArbitrationRouter::apply_decision`
        /// branch for `DOMAIN_TASK_BOUNTY`, in the same transaction as escrow settlement.
        /// 由 runtime 的 `DOMAIN_TASK_BOUNTY` 路由分支在 escrow 结算同事务中调用。
        pub fn settle_from_arbitration(bounty_id: u64, outcome: ArbitrationOutcome) -> DispatchResult {
            Bounties::<T>::try_mutate(bounty_id, |maybe| -> DispatchResult {
                let bounty = maybe.as_mut().ok_or(Error::<T>::SubmissionNotFound)?;
                ensure!(bounty.state == BountyState::Disputed, Error::<T>::BadState);
                // Unreserve any still-active stake (incl. contested). / 解押残留质押。
                for i in 0..bounty.submission_count {
                    Submissions::<T>::mutate(bounty_id, i, |m| {
                        if let Some(s) = m.as_mut() {
                            if matches!(s.state, SubmissionState::Submitted | SubmissionState::Delivered) {
                                T::Currency::unreserve(&s.solver, s.stake);
                                s.state = SubmissionState::Rejected;
                            }
                        }
                    });
                }
                match outcome {
                    ArbitrationOutcome::Release | ArbitrationOutcome::Partial(_) => {
                        bounty.state = BountyState::Completed;
                        UserStats::<T>::mutate(&bounty.poster, |s| {
                            s.poster_dispute_lost = s.poster_dispute_lost.saturating_add(1)
                        });
                    }
                    ArbitrationOutcome::Refund => {
                        bounty.state = BountyState::Refunded;
                        if let Some(solver) = &bounty.contested {
                            UserStats::<T>::mutate(solver, |s| {
                                s.solver_dispute_lost = s.solver_dispute_lost.saturating_add(1)
                            });
                        }
                    }
                }
                Ok(())
            })?;
            // Dispute is terminal: revoke remaining bounty-scene chat. / 争议终结，撤销剩余聊天授权。
            if let Some(bounty) = Bounties::<T>::get(bounty_id) {
                Self::revoke_non_accepted_chat(bounty_id, &bounty);
                let notice = Self::notice_bounty(
                    b"bounty:dispute_settled",
                    bounty_id,
                    &[Self::outcome_code(outcome)],
                );
                T::Notifier::notify(&bounty.poster, notice.clone());
                if let Some(solver) = &bounty.contested {
                    T::Notifier::notify(solver, notice);
                }
            }
            Self::deposit_event(Event::DisputeSettled { id: bounty_id, outcome });
            Ok(())
        }
    }

    // ----- Provider / reputation implementations consumed by the runtime. -----
    // ----- 供 runtime 消费的 provider / 声誉实现。 -----

    impl<T: Config> BountyInfoProvider<T::AccountId, BalanceOf<T>> for Pallet<T> {
        fn poster(id: u64) -> Option<T::AccountId> {
            Bounties::<T>::get(id).map(|b| b.poster)
        }
        fn contested_solver(id: u64) -> Option<T::AccountId> {
            Bounties::<T>::get(id).and_then(|b| b.contested)
        }
        fn amount(id: u64) -> Option<BalanceOf<T>> {
            Bounties::<T>::get(id).map(|_| T::Escrow::amount_of(id))
        }
        fn can_dispute(id: u64, who: &T::AccountId) -> bool {
            match Bounties::<T>::get(id) {
                Some(b) => {
                    matches!(b.state, BountyState::Open | BountyState::Disputed)
                        && (*who == b.poster || b.contested.as_ref() == Some(who))
                }
                None => false,
            }
        }
    }

    impl<T: Config> BountyReputationInspect<T::AccountId> for Pallet<T> {
        /// Additive, newcomer-neutral reputation. / 加性、对新人中性的声誉。
        ///
        /// Every account starts at `NEUTRAL` (5000). Positive outcomes add points,
        /// genuine negative signals subtract; merely "submitted but not yet accepted"
        /// is **not** penalised, so honest newcomers are never gated out.
        /// 每个账户从中性值 5000 起；正向结果加分，真实负面信号扣分；仅「已提交未验收」
        /// **不**扣分，避免误伤诚实新人。
        fn reputation_of(who: &T::AccountId, role: BountyReputationRole) -> u32 {
            const NEUTRAL: i64 = 5_000;
            const MAX: i64 = 10_000;
            let s = UserStats::<T>::get(who);
            let score: i64 = match role {
                BountyReputationRole::Poster => {
                    NEUTRAL
                        + (s.poster_completed as i64) * 1_500
                        - (s.poster_refunded as i64) * 300
                        - (s.poster_dispute_lost as i64) * 4_000
                }
                BountyReputationRole::Solver => {
                    NEUTRAL
                        + (s.solver_accepted as i64) * 1_500
                        - (s.solver_withdrawn as i64) * 800
                        - (s.solver_dispute_lost as i64) * 4_000
                        - (s.solver_slashed as i64) * 4_000
                }
            };
            score.clamp(0, MAX) as u32
        }
    }
}
