#![cfg_attr(not(feature = "std"), no_std)]
#![allow(deprecated)]

//! # Dispute arbitration pallet.
//! # 争议仲裁模块。
//!
//! This pallet coordinates complaint filing, response deadlines, evidence linkage,
//! appeal windows, and arbitration decisions across supported dispute domains.
//! 本模块负责在支持的争议域中协调投诉提交、响应时限、证据关联、上诉窗口与仲裁裁决流程。

extern crate alloc;

/// 仲裁系统通知端口（runtime 适配器桥接到 chat-core 的 System 通道）。
/// Arbitration system-notification port; runtime adapter bridges to chat-core.
///
/// 与 `pallet-chat-core` 解耦；调用为尽力而为，失败不回滚仲裁状态转移。
/// Decoupled from chat-core; best-effort — notification failure must NEVER
/// abort an arbitration transition. `()` is the no-op default.
pub trait ArbitrationNotifier<AccountId> {
    /// 向 `to` 推送一条仲裁/投诉系统通知（不透明模板描述符）。
    /// Push an arbitration/complaint system notice to `to`.
    fn notify(to: &AccountId, notice: alloc::vec::Vec<u8>);
}

impl<AccountId> ArbitrationNotifier<AccountId> for () {
    fn notify(_to: &AccountId, _notice: alloc::vec::Vec<u8>) {}
}

pub use pallet::*;
pub mod runtime_api;
pub mod state_machine;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

mod cleanup;
mod complaint;
mod dispute;

#[cfg(test)]
pub mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    pub use crate::types::*;
    use crate::weights::WeightInfo;
    use frame_support::traits::{
        fungible::{
            Inspect as FungibleInspect, Mutate as FungibleMutate, MutateHold as FungibleMutateHold,
        },
        EnsureOrigin,
    };
    use frame_support::{pallet_prelude::*, BoundedVec};
    use frame_system::pallet_prelude::*;
    use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
    use pallet_storage_service::StoragePin;
    use pallet_trading_common::PricingProvider;
    use sp_runtime::{SaturatedConversion, Saturating};

    // ==================== Complaint struct (depends on T) ====================

    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    /// Complaint record.
    /// 投诉记录。
    pub struct Complaint<T: Config> {
        pub domain: [u8; 8],
        pub object_id: u64,
        pub complaint_type: ComplaintType,
        pub complainant: T::AccountId,
        pub respondent: T::AccountId,
        pub details_cid: BoundedVec<u8, T::MaxCidLen>,
        pub response_cid: Option<BoundedVec<u8, T::MaxCidLen>>,
        pub amount: Option<BalanceOf<T>>,
        pub status: ComplaintStatus,
        pub created_at: BlockNumberFor<T>,
        pub response_deadline: BlockNumberFor<T>,
        pub settlement_cid: Option<BoundedVec<u8, T::MaxCidLen>>,
        pub resolution_cid: Option<BoundedVec<u8, T::MaxCidLen>>,
        pub appeal_cid: Option<BoundedVec<u8, T::MaxCidLen>>,
        /// Who filed the appeal (None until appeal is filed)
        pub appellant: Option<T::AccountId>,
        pub updated_at: BlockNumberFor<T>,
    }

    // ==================== Traits ====================

    /// Arbitration routing interface.
    /// 仲裁路由接口。
    pub trait ArbitrationRouter<AccountId, Balance> {
        fn can_dispute(domain: [u8; 8], who: &AccountId, id: u64) -> bool;
        fn apply_decision(domain: [u8; 8], id: u64, decision: Decision) -> DispatchResult;
        fn get_counterparty(
            domain: [u8; 8],
            initiator: &AccountId,
            id: u64,
        ) -> Result<AccountId, DispatchError>;
        fn get_order_amount(domain: [u8; 8], id: u64) -> Result<Balance, DispatchError>;
    }

    /// Evidence-existence checker.
    /// 证据存在性检查接口。
    pub trait EvidenceExistenceChecker {
        fn evidence_exists(id: u64) -> bool;
    }

    // ==================== Config ====================

    pub type BalanceOf<T> =
        <<T as pallet_dispute_escrow::pallet::Config>::Currency as frame_support::traits::Currency<
            <T as frame_system::Config>::AccountId,
        >>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>>
        + pallet_dispute_escrow::pallet::Config
    {
        /// Maximum number of evidence items.
        /// 最大证据数量
        #[pallet::constant]
        type MaxEvidence: Get<u32>;

        /// Maximum CID length.
        /// CID 最大长度
        #[pallet::constant]
        type MaxCidLen: Get<u32>;

        /// Escrow integration.
        /// 托管接口
        type Escrow: EscrowTrait<Self::AccountId, BalanceOf<Self>>;

        /// Weight provider.
        /// 权重信息
        type WeightInfo: WeightInfo;

        /// Arbitration router.
        /// 仲裁路由接口
        type Router: ArbitrationRouter<Self::AccountId, BalanceOf<Self>>;

        /// Decision origin.
        /// 裁决 Origin
        type DecisionOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        type Fungible: FungibleInspect<Self::AccountId, Balance = BalanceOf<Self>>
            + FungibleMutate<Self::AccountId>
            + FungibleMutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;
        type RuntimeHoldReason: From<HoldReason>;

        /// Deposit ratio in basis points.
        /// 押金比例（基点）
        #[pallet::constant]
        type DepositRatioBps: Get<u16>;

        /// Complaint-response deadline.
        /// 响应时限
        #[pallet::constant]
        type ResponseDeadline: Get<BlockNumberFor<Self>>;

        #[pallet::constant]
        type RejectedSlashBps: Get<u16>;
        #[pallet::constant]
        type PartialSlashBps: Get<u16>;
        #[pallet::constant]
        type ComplaintDeposit: Get<BalanceOf<Self>>;
        #[pallet::constant]
        type ComplaintDepositUsd: Get<u64>;

        /// Pricing provider.
        /// 定价接口
        type Pricing: PricingProvider<BalanceOf<Self>>;

        #[pallet::constant]
        type ComplaintSlashBps: Get<u16>;

        /// Treasury account.
        /// 国库账户
        type TreasuryAccount: Get<Self::AccountId>;

        type CidLockManager: pallet_storage_service::CidLockManager<
            Self::Hash,
            BlockNumberFor<Self>,
        >;

        /// IPFS pin management interface for arbitration document CIDs, including
        /// `details_cid`, `response_cid`, `settlement_cid`, and `resolution_cid`.
        /// IPFS Pin 管理接口（用于仲裁文书 CID 持久化：`details_cid`、`response_cid`、
        /// `settlement_cid`、`resolution_cid`）
        type StoragePin: pallet_storage_service::StoragePin<Self::AccountId>;

        #[pallet::constant]
        type ArchiveTtlBlocks: Get<u32>;
        #[pallet::constant]
        type ComplaintArchiveDelayBlocks: Get<BlockNumberFor<Self>>;
        #[pallet::constant]
        type ComplaintMaxLifetimeBlocks: Get<BlockNumberFor<Self>>;

        type EvidenceExists: EvidenceExistenceChecker;

        /// Appeal window after resolution during which the losing party can appeal.
        /// 裁决后允许败诉方提起上诉的窗口期
        #[pallet::constant]
        type AppealWindowBlocks: Get<BlockNumberFor<Self>>;

        /// Auto-escalation timeout: Responded complaints auto-escalate to Arbitrating
        #[pallet::constant]
        type AutoEscalateBlocks: Get<BlockNumberFor<Self>>;

        /// Maximum active complaints per user. Replaces the old `BoundedVec<50>` limit.
        /// Max active complaints per user（替代旧的 `BoundedVec<50>` 限制）
        #[pallet::constant]
        type MaxActivePerUser: Get<u32>;

        /// 仲裁系统通知端口（桥接到聊天 System 通道；尽力而为）。
        /// Arbitration system-notification port (chat System channel; best-effort).
        type Notifier: crate::ArbitrationNotifier<Self::AccountId>;
    }

    // ==================== HoldReason ====================

    #[pallet::composite_enum]
    pub enum HoldReason {
        DisputeInitiator,
        DisputeRespondent,
        ComplaintDeposit,
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ==================== Storage: Dispute subsystem ====================

    #[pallet::storage]
    pub type Disputed<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, [u8; 8], Blake2_128Concat, u64, (), OptionQuery>;

    #[pallet::storage]
    pub type EvidenceIds<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        [u8; 8],
        Blake2_128Concat,
        u64,
        BoundedVec<u64, T::MaxEvidence>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type LockedCidHashes<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        [u8; 8],
        Blake2_128Concat,
        u64,
        BoundedVec<T::Hash, T::MaxEvidence>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type TwoWayDeposits<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        [u8; 8],
        Blake2_128Concat,
        u64,
        TwoWayDepositRecord<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::storage]
    pub type NextArchivedId<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn archived_disputes)]
    pub type ArchivedDisputes<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ArchivedDispute, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn arbitration_stats)]
    pub type ArbitrationStats<T: Config> = StorageValue<_, ArbitrationPermanentStats, ValueQuery>;

    #[pallet::storage]
    pub type PendingArbitrationDisputes<T: Config> =
        StorageDoubleMap<_, Blake2_128Concat, [u8; 8], Blake2_128Concat, u64, (), OptionQuery>;

    // ==================== Storage: Complaint subsystem ====================

    #[pallet::storage]
    pub type NextComplaintId<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn complaints)]
    pub type Complaints<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, Complaint<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn archived_complaints)]
    pub type ArchivedComplaints<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, ArchivedComplaint, OptionQuery>;

    #[pallet::storage]
    pub type ComplaintDeposits<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BalanceOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn domain_stats)]
    pub type DomainStats<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 8], DomainStatistics, ValueQuery>;

    #[pallet::storage]
    pub type PendingArbitrationComplaints<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (), OptionQuery>;

    /// Per-user active complaint counter (replaces BoundedVec index)
    #[pallet::storage]
    pub type ActiveComplaintCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// Complaint evidence CIDs (on-chain audit trail)
    #[pallet::storage]
    pub type ComplaintEvidenceCids<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        BoundedVec<BoundedVec<u8, T::MaxCidLen>, T::MaxEvidence>,
        ValueQuery,
    >;

    /// Cooldown: (domain, object_id) => block when new complaint is allowed
    #[pallet::storage]
    pub type ComplaintCooldown<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        [u8; 8],
        Blake2_128Concat,
        u64,
        BlockNumberFor<T>,
        OptionQuery,
    >;

    // ==================== Storage: Cursors ====================

    #[pallet::storage]
    pub type ArchiveDisputeCleanupCursor<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type ArchiveComplaintCleanupCursor<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type ComplaintArchiveCursor<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type ComplaintExpiryCursor<T: Config> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    pub type AutoEscalateCursor<T: Config> = StorageValue<_, u64, ValueQuery>;

    // ==================== Storage: Global controls ====================

    #[pallet::storage]
    pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    pub type DomainPenaltyRates<T: Config> =
        StorageMap<_, Blake2_128Concat, [u8; 8], u16, OptionQuery>;

    // ==================== Events ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Disputed {
            domain: [u8; 8],
            id: u64,
        },
        Arbitrated {
            domain: [u8; 8],
            id: u64,
            decision: u8,
            bps: Option<u16>,
        },
        DisputeWithDepositInitiated {
            domain: [u8; 8],
            id: u64,
            initiator: T::AccountId,
            respondent: T::AccountId,
            deposit: BalanceOf<T>,
            deadline: BlockNumberFor<T>,
        },
        RespondentDepositLocked {
            domain: [u8; 8],
            id: u64,
            respondent: T::AccountId,
            deposit: BalanceOf<T>,
        },
        DepositProcessed {
            domain: [u8; 8],
            id: u64,
            account: T::AccountId,
            released: BalanceOf<T>,
            slashed: BalanceOf<T>,
        },

        ComplaintFiled {
            complaint_id: u64,
            domain: [u8; 8],
            object_id: u64,
            complainant: T::AccountId,
            respondent: T::AccountId,
            complaint_type: ComplaintType,
        },
        ComplaintResponded {
            complaint_id: u64,
            respondent: T::AccountId,
        },
        ComplaintWithdrawn {
            complaint_id: u64,
        },
        ComplaintSettled {
            complaint_id: u64,
        },
        ComplaintEscalated {
            complaint_id: u64,
        },
        ComplaintResolved {
            complaint_id: u64,
            decision: u8,
        },
        ComplaintExpired {
            complaint_id: u64,
        },
        ComplaintArchived {
            complaint_id: u64,
        },
        ComplaintAutoEscalated {
            complaint_id: u64,
        },

        DefaultJudgment {
            domain: [u8; 8],
            id: u64,
            initiator: T::AccountId,
        },
        ComplaintEvidenceSupplemented {
            complaint_id: u64,
            who: T::AccountId,
            evidence_cid: BoundedVec<u8, T::MaxCidLen>,
        },
        DisputeSettled {
            domain: [u8; 8],
            id: u64,
        },
        ComplaintMediationStarted {
            complaint_id: u64,
        },
        DisputeDismissed {
            domain: [u8; 8],
            id: u64,
        },
        ComplaintDismissed {
            complaint_id: u64,
        },
        PausedStateChanged {
            paused: bool,
        },
        DisputeForceClosed {
            domain: [u8; 8],
            id: u64,
        },
        ComplaintForceClosed {
            complaint_id: u64,
        },
        DomainPenaltyRateUpdated {
            domain: [u8; 8],
            rate_bps: Option<u16>,
        },

        AppealFiled {
            complaint_id: u64,
            appellant: T::AccountId,
        },
        AppealResolved {
            complaint_id: u64,
            decision: u8,
        },
        AccountBanRequested {
            domain: [u8; 8],
            object_id: u64,
            account: T::AccountId,
        },
        /// Deposit operation failed — funds may remain held
        DepositOperationFailed {
            complaint_id: u64,
            operation: u8,
            amount: BalanceOf<T>,
        },
    }

    // ==================== Errors ====================

    #[pallet::error]
    pub enum Error<T> {
        AlreadyDisputed,
        NotDisputed,
        InsufficientDeposit,
        AlreadyResponded,
        ResponseDeadlinePassed,
        CounterpartyNotFound,
        ComplaintNotFound,
        NotAuthorized,
        InvalidComplaintType,
        InvalidState,
        TooManyComplaints,
        TooManyActiveComplaints,
        EvidenceNotFound,
        ResponseDeadlineNotReached,
        SettlementNotConfirmed,
        ModulePaused,
        InvalidPenaltyRate,
        Deprecated,
        /// Complaint cooldown period has not elapsed for this object
        CooldownActive,
        /// Appeal window has closed
        AppealWindowClosed,
        /// Cannot appeal this complaint (wrong status or party)
        CannotAppeal,
        /// Invalid decision code (must be 0=Release, 1=Refund, 2=Partial)
        InvalidDecisionCode,
    }

    // ==================== Extrinsics ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Deprecated: use dispute_with_two_way_deposit (call_index 4)
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::dispute(_evidence.len() as u32))]
        pub fn dispute(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
            _evidence: alloc::vec::Vec<BoundedVec<u8, T::MaxCidLen>>,
        ) -> DispatchResult {
            let _ = (domain, id);
            ensure_signed(origin)?;
            Err(Error::<T>::Deprecated.into())
        }

        /// Governance arbitration decision for disputes
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::arbitrate())]
        pub fn arbitrate(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
            decision_code: u8,
            bps: Option<u16>,
        ) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;
            ensure!(
                Disputed::<T>::get(domain, id).is_some(),
                Error::<T>::NotDisputed
            );
            let parties = TwoWayDeposits::<T>::get(domain, id);
            let decision = match (decision_code, bps) {
                (0, _) => Decision::Release,
                (1, _) => Decision::Refund,
                (2, Some(p)) => Decision::Partial(p.min(10_000)),
                (2, None) => Decision::Partial(T::PartialSlashBps::get().min(10_000)),
                _ => return Err(Error::<T>::InvalidDecisionCode.into()),
            };
            T::Router::apply_decision(domain, id, decision.clone())?;
            Self::handle_deposits_on_arbitration(domain, id, &decision)?;
            Self::unlock_all_evidence_cids(domain, id)?;

            let out = match decision {
                Decision::Release => (0, None),
                Decision::Refund => (1, None),
                Decision::Partial(p) => (2, Some(p)),
            };
            PendingArbitrationDisputes::<T>::remove(domain, id);
            Self::archive_and_cleanup(domain, id, out.0, out.1.unwrap_or(0));

            Self::deposit_event(Event::Arbitrated {
                domain,
                id,
                decision: out.0,
                bps: out.1,
            });

            // 通知争议双方：治理裁决已出（best-effort）。
            if let Some(record) = parties {
                let notice =
                    Self::notice_dispute(b"dispute:arbitrated", domain, id, Some(decision_code));
                T::Notifier::notify(&record.initiator, notice.clone());
                T::Notifier::notify(&record.respondent, notice);
            }
            Ok(())
        }

        /// Deprecated: use dispute_with_two_way_deposit (call_index 4)
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::dispute(1))]
        pub fn dispute_with_evidence_id(
            origin: OriginFor<T>,
            _domain: [u8; 8],
            _id: u64,
            _evidence_id: u64,
        ) -> DispatchResult {
            ensure_signed(origin)?;
            Err(Error::<T>::Deprecated.into())
        }

        /// Append evidence to an existing dispute
        #[pallet::call_index(3)]
        #[pallet::weight(<T as Config>::WeightInfo::dispute(1))]
        pub fn append_evidence_id(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
            evidence_id: u64,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let _who = ensure_signed(origin)?;
            ensure!(
                Disputed::<T>::get(domain, id).is_some(),
                Error::<T>::NotDisputed
            );
            #[cfg(not(feature = "runtime-benchmarks"))]
            {
                ensure!(
                    T::Router::can_dispute(domain, &_who, id),
                    Error::<T>::NotAuthorized
                );
            }
            ensure!(
                T::EvidenceExists::evidence_exists(evidence_id),
                Error::<T>::EvidenceNotFound
            );
            EvidenceIds::<T>::try_mutate(domain, id, |v| -> Result<(), Error<T>> {
                v.try_push(evidence_id)
                    .map_err(|_| Error::<T>::AlreadyDisputed)?;
                Ok(())
            })?;
            Ok(())
        }

        /// Initiate dispute with two-way deposit (primary dispute entry point)
        #[pallet::call_index(4)]
        #[pallet::weight(<T as Config>::WeightInfo::dispute(1))]
        pub fn dispute_with_two_way_deposit(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
            evidence_id: u64,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let initiator = ensure_signed(origin)?;

            #[cfg(not(feature = "runtime-benchmarks"))]
            {
                ensure!(
                    T::Router::can_dispute(domain, &initiator, id),
                    Error::<T>::NotDisputed
                );
            }
            ensure!(
                Disputed::<T>::get(domain, id).is_none(),
                Error::<T>::AlreadyDisputed
            );
            ensure!(
                T::EvidenceExists::evidence_exists(evidence_id),
                Error::<T>::EvidenceNotFound
            );

            let order_amount = T::Router::get_order_amount(domain, id)
                .map_err(|_| Error::<T>::CounterpartyNotFound)?;
            let deposit_ratio_bps = T::DepositRatioBps::get();
            let deposit_amount = sp_runtime::Permill::from_parts((deposit_ratio_bps as u32) * 100)
                .mul_floor(order_amount);

            let escrow_balance = T::Escrow::amount_of(id);
            ensure!(
                escrow_balance >= deposit_amount,
                Error::<T>::InsufficientDeposit
            );

            let escrow_account = Self::get_escrow_account();
            T::Fungible::hold(
                &T::RuntimeHoldReason::from(HoldReason::DisputeInitiator),
                &escrow_account,
                deposit_amount,
            )
            .map_err(|_| Error::<T>::InsufficientDeposit)?;

            let respondent = T::Router::get_counterparty(domain, &initiator, id)
                .map_err(|_| Error::<T>::CounterpartyNotFound)?;
            ensure!(initiator != respondent, Error::<T>::NotAuthorized);

            let current_block = frame_system::Pallet::<T>::block_number();
            let deadline = current_block + T::ResponseDeadline::get();

            Disputed::<T>::insert(domain, id, ());
            TwoWayDeposits::<T>::insert(
                domain,
                id,
                TwoWayDepositRecord {
                    initiator: initiator.clone(),
                    initiator_deposit: deposit_amount,
                    respondent: respondent.clone(),
                    respondent_deposit: None,
                    response_deadline: deadline,
                    has_responded: false,
                },
            );

            EvidenceIds::<T>::try_mutate(domain, id, |v| -> Result<(), Error<T>> {
                v.try_push(evidence_id)
                    .map_err(|_| Error::<T>::AlreadyDisputed)?;
                Ok(())
            })?;

            PendingArbitrationDisputes::<T>::insert(domain, id, ());

            // 通知被诉方：争议已开启（best-effort）。
            T::Notifier::notify(
                &respondent,
                Self::notice_dispute(b"dispute:opened", domain, id, None),
            );

            Self::deposit_event(Event::DisputeWithDepositInitiated {
                domain,
                id,
                initiator,
                respondent,
                deposit: deposit_amount,
                deadline,
            });
            Ok(())
        }

        /// Respondent locks deposit and submits counter-evidence
        #[pallet::call_index(5)]
        #[pallet::weight(<T as Config>::WeightInfo::dispute(1))]
        pub fn respond_to_dispute(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
            counter_evidence_id: u64,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let respondent = ensure_signed(origin)?;

            let mut deposit_record =
                TwoWayDeposits::<T>::get(domain, id).ok_or(Error::<T>::NotDisputed)?;
            let initiator = deposit_record.initiator.clone();
            ensure!(
                deposit_record.respondent == respondent,
                Error::<T>::NotDisputed
            );
            ensure!(!deposit_record.has_responded, Error::<T>::AlreadyResponded);
            let current_block = frame_system::Pallet::<T>::block_number();
            ensure!(
                current_block <= deposit_record.response_deadline,
                Error::<T>::ResponseDeadlinePassed
            );
            ensure!(
                T::EvidenceExists::evidence_exists(counter_evidence_id),
                Error::<T>::EvidenceNotFound
            );

            let deposit_amount = deposit_record.initiator_deposit;
            let escrow_balance = T::Escrow::amount_of(id);
            ensure!(
                escrow_balance >= deposit_amount,
                Error::<T>::InsufficientDeposit
            );

            let escrow_account = Self::get_escrow_account();
            T::Fungible::hold(
                &T::RuntimeHoldReason::from(HoldReason::DisputeRespondent),
                &escrow_account,
                deposit_amount,
            )
            .map_err(|_| Error::<T>::InsufficientDeposit)?;

            deposit_record.respondent_deposit = Some(deposit_amount);
            deposit_record.has_responded = true;
            TwoWayDeposits::<T>::insert(domain, id, deposit_record);

            EvidenceIds::<T>::try_mutate(domain, id, |v| -> Result<(), Error<T>> {
                v.try_push(counter_evidence_id)
                    .map_err(|_| Error::<T>::AlreadyDisputed)?;
                Ok(())
            })?;

            Self::deposit_event(Event::RespondentDepositLocked {
                domain,
                id,
                respondent,
                deposit: deposit_amount,
            });

            // 通知发起方：被诉方已应诉并锁定押金。
            T::Notifier::notify(
                &initiator,
                Self::notice_dispute(b"dispute:responded", domain, id, None),
            );
            Ok(())
        }

        // ==================== Complaint extrinsics ====================

        /// File a complaint (with deposit)
        #[pallet::call_index(10)]
        #[pallet::weight(<T as Config>::WeightInfo::file_complaint())]
        pub fn file_complaint(
            origin: OriginFor<T>,
            domain: [u8; 8],
            object_id: u64,
            complaint_type: ComplaintType,
            details_cid: BoundedVec<u8, T::MaxCidLen>,
            amount: Option<BalanceOf<T>>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let complainant = ensure_signed(origin)?;

            #[cfg(not(feature = "runtime-benchmarks"))]
            ensure!(
                T::Router::can_dispute(domain, &complainant, object_id),
                Error::<T>::NotAuthorized
            );

            let respondent = T::Router::get_counterparty(domain, &complainant, object_id)
                .map_err(|_| Error::<T>::CounterpartyNotFound)?;
            ensure!(complainant != respondent, Error::<T>::NotAuthorized);

            ensure!(
                complaint_type.domain() == domain || matches!(complaint_type, ComplaintType::Other),
                Error::<T>::InvalidComplaintType
            );

            // Cooldown check
            let now = frame_system::Pallet::<T>::block_number();
            if let Some(cooldown_until) = ComplaintCooldown::<T>::get(domain, object_id) {
                ensure!(now >= cooldown_until, Error::<T>::CooldownActive);
            }

            // Rate limit
            let active_count = ActiveComplaintCount::<T>::get(&complainant);
            ensure!(
                active_count < T::MaxActivePerUser::get(),
                Error::<T>::TooManyActiveComplaints
            );

            // Calculate and lock deposit
            let min_deposit = T::ComplaintDeposit::get();
            let deposit_usd = T::ComplaintDepositUsd::get();
            let deposit_amount = if let Some(price) = T::Pricing::get_nex_to_usd_rate() {
                let price_u128: u128 = price.saturated_into();
                if price_u128 > 0u128 {
                    let required_u128 =
                        (deposit_usd as u128).saturating_mul(1_000_000u128) / price_u128;
                    let required: BalanceOf<T> = required_u128.saturated_into();
                    if required > min_deposit {
                        required
                    } else {
                        min_deposit
                    }
                } else {
                    min_deposit
                }
            } else {
                min_deposit
            };

            T::Fungible::hold(
                &T::RuntimeHoldReason::from(HoldReason::ComplaintDeposit),
                &complainant,
                deposit_amount,
            )
            .map_err(|_| Error::<T>::InsufficientDeposit)?;

            let complaint_id = NextComplaintId::<T>::mutate(|id| {
                let current = *id;
                *id = id.saturating_add(1);
                current
            });

            ComplaintDeposits::<T>::insert(complaint_id, deposit_amount);
            let deadline = now + T::ResponseDeadline::get();

            let complaint = Complaint {
                domain,
                object_id,
                complaint_type: complaint_type.clone(),
                complainant: complainant.clone(),
                respondent: respondent.clone(),
                details_cid,
                response_cid: None,
                amount,
                status: ComplaintStatus::Submitted,
                created_at: now,
                response_deadline: deadline,
                settlement_cid: None,
                resolution_cid: None,
                appeal_cid: None,
                appellant: None,
                updated_at: now,
            };

            Complaints::<T>::insert(complaint_id, &complaint);
            ActiveComplaintCount::<T>::mutate(&complainant, |c| *c = c.saturating_add(1));

            // Pin details_cid to IPFS (best-effort: failure logged, not blocking)
            let cid_vec: alloc::vec::Vec<u8> = complaint.details_cid.clone().into_inner();
            let cid_size = cid_vec.len() as u64;
            if let Err(e) = T::StoragePin::pin(
                complainant.clone(),
                b"arbitration",
                complaint_id,
                None,
                cid_vec,
                cid_size,
                pallet_storage_service::PinTier::Critical,
            ) {
                log::warn!(target: "pallet-dispute-arbitration", "pin details_cid failed for complaint {}: {:?}", complaint_id, e);
            }

            DomainStats::<T>::mutate(domain, |stats| {
                stats.total_complaints = stats.total_complaints.saturating_add(1);
            });

            // 通知被诉方：投诉已提交。
            T::Notifier::notify(
                &respondent,
                Self::notice_complaint(b"complaint:filed", complaint_id, None),
            );

            Self::deposit_event(Event::ComplaintFiled {
                complaint_id,
                domain,
                object_id,
                complainant,
                respondent,
                complaint_type,
            });
            Ok(())
        }

        /// Respond to a complaint
        #[pallet::call_index(11)]
        #[pallet::weight(<T as Config>::WeightInfo::respond_to_complaint())]
        pub fn respond_to_complaint(
            origin: OriginFor<T>,
            complaint_id: u64,
            response_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let respondent = ensure_signed(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(
                    complaint.respondent == respondent,
                    Error::<T>::NotAuthorized
                );
                ensure!(
                    state_machine::can_transition(&complaint.status, &ComplaintStatus::Responded),
                    Error::<T>::InvalidState
                );
                let now = frame_system::Pallet::<T>::block_number();
                ensure!(
                    now <= complaint.response_deadline,
                    Error::<T>::ResponseDeadlinePassed
                );

                complaint.response_cid = Some(response_cid.clone());
                complaint.status = ComplaintStatus::Responded;
                complaint.updated_at = now;

                // Pin response_cid to IPFS (best-effort)
                let cid_vec: alloc::vec::Vec<u8> = response_cid.into_inner();
                let cid_size = cid_vec.len() as u64;
                if let Err(e) = T::StoragePin::pin(
                    respondent.clone(),
                    b"arbitration",
                    complaint_id,
                    None,
                    cid_vec,
                    cid_size,
                    pallet_storage_service::PinTier::Critical,
                ) {
                    log::warn!(target: "pallet-dispute-arbitration", "pin response_cid failed for complaint {}: {:?}", complaint_id, e);
                }

                Self::deposit_event(Event::ComplaintResponded {
                    complaint_id,
                    respondent,
                });
                Ok(())
            })
        }

        /// Withdraw a complaint (complainant only)
        #[pallet::call_index(12)]
        #[pallet::weight(<T as Config>::WeightInfo::withdraw_complaint())]
        pub fn withdraw_complaint(origin: OriginFor<T>, complaint_id: u64) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(complaint.complainant == who, Error::<T>::NotAuthorized);
                ensure!(
                    state_machine::can_transition(&complaint.status, &ComplaintStatus::Withdrawn),
                    Error::<T>::InvalidState
                );

                let now = frame_system::Pallet::<T>::block_number();
                complaint.status = ComplaintStatus::Withdrawn;
                complaint.updated_at = now;

                Self::refund_complaint_deposit(complaint_id, &complaint.complainant);
                Self::decrement_active_count(&complaint.complainant);

                Self::deposit_event(Event::ComplaintWithdrawn { complaint_id });
                Ok(())
            })
        }

        /// Settle a complaint (complainant only)
        #[pallet::call_index(13)]
        #[pallet::weight(<T as Config>::WeightInfo::settle_complaint())]
        pub fn settle_complaint(
            origin: OriginFor<T>,
            complaint_id: u64,
            settlement_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(complaint.complainant == who, Error::<T>::NotAuthorized);
                ensure!(
                    state_machine::can_transition(
                        &complaint.status,
                        &ComplaintStatus::ResolvedSettlement
                    ),
                    Error::<T>::InvalidState
                );

                let now = frame_system::Pallet::<T>::block_number();
                complaint.settlement_cid = Some(settlement_cid.clone());
                complaint.status = ComplaintStatus::ResolvedSettlement;
                complaint.updated_at = now;

                // Pin settlement_cid to IPFS (best-effort)
                let cid_vec: alloc::vec::Vec<u8> = settlement_cid.into_inner();
                let cid_size = cid_vec.len() as u64;
                if let Err(e) = T::StoragePin::pin(
                    who.clone(),
                    b"arbitration",
                    complaint_id,
                    None,
                    cid_vec,
                    cid_size,
                    pallet_storage_service::PinTier::Critical,
                ) {
                    log::warn!(target: "pallet-dispute-arbitration", "pin settlement_cid failed for complaint {}: {:?}", complaint_id, e);
                }

                Self::refund_complaint_deposit(complaint_id, &complaint.complainant);
                Self::decrement_active_count(&complaint.complainant);

                DomainStats::<T>::mutate(complaint.domain, |stats| {
                    stats.resolved_count = stats.resolved_count.saturating_add(1);
                    stats.settlements = stats.settlements.saturating_add(1);
                });

                // Set cooldown on this object
                ComplaintCooldown::<T>::insert(
                    complaint.domain,
                    complaint.object_id,
                    now + T::ResponseDeadline::get(),
                );

                Self::deposit_event(Event::ComplaintSettled { complaint_id });
                Ok(())
            })
        }

        /// Escalate complaint to arbitration
        #[pallet::call_index(14)]
        #[pallet::weight(<T as Config>::WeightInfo::escalate_to_arbitration())]
        pub fn escalate_to_arbitration(origin: OriginFor<T>, complaint_id: u64) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(
                    complaint.complainant == who || complaint.respondent == who,
                    Error::<T>::NotAuthorized
                );
                ensure!(
                    state_machine::can_transition(&complaint.status, &ComplaintStatus::Arbitrating),
                    Error::<T>::InvalidState
                );

                // Submitted -> Arbitrating requires response deadline to have passed
                let now = frame_system::Pallet::<T>::block_number();
                if complaint.status == ComplaintStatus::Submitted {
                    ensure!(
                        now > complaint.response_deadline,
                        Error::<T>::ResponseDeadlineNotReached
                    );
                }

                complaint.status = ComplaintStatus::Arbitrating;
                complaint.updated_at = now;
                PendingArbitrationComplaints::<T>::insert(complaint_id, ());

                Self::deposit_event(Event::ComplaintEscalated { complaint_id });
                Ok(())
            })
        }

        /// Resolve a complaint (governance only)
        #[pallet::call_index(15)]
        #[pallet::weight(<T as Config>::WeightInfo::arbitrate())]
        pub fn resolve_complaint(
            origin: OriginFor<T>,
            complaint_id: u64,
            decision: u8,
            reason_cid: BoundedVec<u8, T::MaxCidLen>,
            partial_bps: Option<u16>,
        ) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(
                    complaint.status == ComplaintStatus::Arbitrating,
                    Error::<T>::InvalidState
                );

                let router_decision = match decision {
                    0 => Decision::Refund,
                    1 => Decision::Release,
                    _ => {
                        let bps = partial_bps.unwrap_or(T::PartialSlashBps::get());
                        Decision::Partial(bps.min(10_000))
                    }
                };
                T::Router::apply_decision(complaint.domain, complaint.object_id, router_decision)?;

                let now = frame_system::Pallet::<T>::block_number();
                complaint.resolution_cid = Some(reason_cid.clone());

                // Pin resolution_cid to IPFS (best-effort, legal-grade data)
                let cid_vec: alloc::vec::Vec<u8> = reason_cid.into_inner();
                let cid_size = cid_vec.len() as u64;
                if let Err(e) = T::StoragePin::pin(
                    complaint.complainant.clone(),
                    b"arbitration",
                    complaint_id,
                    None,
                    cid_vec,
                    cid_size,
                    pallet_storage_service::PinTier::Critical,
                ) {
                    log::warn!(target: "pallet-dispute-arbitration", "pin resolution_cid failed for complaint {}: {:?}", complaint_id, e);
                }

                // decision: 0=ComplainantWin, 1=RespondentWin, >=2=Partial (mapped to ComplainantWin
                // because partial rulings favor the complainant for deposit/appeal purposes)
                complaint.status = match decision {
                    0 | 2.. => ComplaintStatus::ResolvedComplainantWin,
                    1 => ComplaintStatus::ResolvedRespondentWin,
                };
                complaint.updated_at = now;

                match decision {
                    1 => {
                        Self::slash_complaint_deposit(
                            complaint_id,
                            &complaint.complainant,
                            &complaint.respondent,
                            complaint.domain,
                            &complaint.complaint_type,
                        );
                    }
                    _ => {
                        Self::refund_complaint_deposit(complaint_id, &complaint.complainant);
                    }
                }

                // Ban request via event (instead of dead trait method)
                if decision == 0 && complaint.complaint_type.triggers_permanent_ban() {
                    Self::deposit_event(Event::AccountBanRequested {
                        domain: complaint.domain,
                        object_id: complaint.object_id,
                        account: complaint.respondent.clone(),
                    });
                }

                PendingArbitrationComplaints::<T>::remove(complaint_id);
                Self::decrement_active_count(&complaint.complainant);

                DomainStats::<T>::mutate(complaint.domain, |stats| {
                    stats.resolved_count = stats.resolved_count.saturating_add(1);
                    match decision {
                        0 | 2.. => {
                            stats.complainant_wins = stats.complainant_wins.saturating_add(1)
                        }
                        1 => stats.respondent_wins = stats.respondent_wins.saturating_add(1),
                    }
                });

                ComplaintCooldown::<T>::insert(
                    complaint.domain,
                    complaint.object_id,
                    now + T::ResponseDeadline::get(),
                );

                let complainant = complaint.complainant.clone();
                let respondent = complaint.respondent.clone();
                Self::deposit_event(Event::ComplaintResolved {
                    complaint_id,
                    decision,
                });

                let notice =
                    Self::notice_complaint(b"complaint:resolved", complaint_id, Some(decision));
                T::Notifier::notify(&complainant, notice.clone());
                T::Notifier::notify(&respondent, notice);
                Ok(())
            })
        }

        // ==================== Dispute utility extrinsics ====================

        /// Default judgment when respondent misses deadline
        #[pallet::call_index(20)]
        #[pallet::weight(<T as Config>::WeightInfo::request_default_judgment())]
        pub fn request_default_judgment(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            let deposit_record =
                TwoWayDeposits::<T>::get(domain, id).ok_or(Error::<T>::NotDisputed)?;
            ensure!(deposit_record.initiator == who, Error::<T>::NotAuthorized);
            ensure!(!deposit_record.has_responded, Error::<T>::AlreadyResponded);
            let current_block = frame_system::Pallet::<T>::block_number();
            ensure!(
                current_block > deposit_record.response_deadline,
                Error::<T>::ResponseDeadlineNotReached
            );

            let decision = Decision::Refund;
            T::Router::apply_decision(domain, id, decision.clone())?;
            Self::handle_deposits_on_arbitration(domain, id, &decision)?;
            Self::unlock_all_evidence_cids(domain, id)?;

            PendingArbitrationDisputes::<T>::remove(domain, id);
            Self::archive_and_cleanup(domain, id, 1, 0);

            Self::deposit_event(Event::DefaultJudgment {
                domain,
                id,
                initiator: who,
            });

            // 通知被诉方：应诉超时，默认裁决（退款给发起方）。
            T::Notifier::notify(
                &deposit_record.respondent,
                Self::notice_dispute(b"dispute:default", domain, id, None),
            );
            Ok(())
        }

        /// Complainant supplements evidence (stored on-chain)
        #[pallet::call_index(21)]
        #[pallet::weight(<T as Config>::WeightInfo::supplement_evidence())]
        pub fn supplement_complaint_evidence(
            origin: OriginFor<T>,
            complaint_id: u64,
            evidence_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            let complaint =
                Complaints::<T>::get(complaint_id).ok_or(Error::<T>::ComplaintNotFound)?;
            ensure!(complaint.complainant == who, Error::<T>::NotAuthorized);
            ensure!(complaint.status.is_active(), Error::<T>::InvalidState);

            ComplaintEvidenceCids::<T>::try_mutate(complaint_id, |cids| {
                cids.try_push(evidence_cid.clone())
                    .map_err(|_| Error::<T>::TooManyComplaints)
            })?;

            Self::deposit_event(Event::ComplaintEvidenceSupplemented {
                complaint_id,
                who,
                evidence_cid,
            });
            Ok(())
        }

        /// Respondent supplements evidence (stored on-chain)
        #[pallet::call_index(22)]
        #[pallet::weight(<T as Config>::WeightInfo::supplement_evidence())]
        pub fn supplement_response_evidence(
            origin: OriginFor<T>,
            complaint_id: u64,
            evidence_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            let complaint =
                Complaints::<T>::get(complaint_id).ok_or(Error::<T>::ComplaintNotFound)?;
            ensure!(complaint.respondent == who, Error::<T>::NotAuthorized);
            ensure!(
                matches!(
                    complaint.status,
                    ComplaintStatus::Responded
                        | ComplaintStatus::Mediating
                        | ComplaintStatus::Arbitrating
                        | ComplaintStatus::Appealed
                ),
                Error::<T>::InvalidState
            );

            ComplaintEvidenceCids::<T>::try_mutate(complaint_id, |cids| {
                cids.try_push(evidence_cid.clone())
                    .map_err(|_| Error::<T>::TooManyComplaints)
            })?;

            Self::deposit_event(Event::ComplaintEvidenceSupplemented {
                complaint_id,
                who,
                evidence_cid,
            });
            Ok(())
        }

        /// Settle a dispute (release both deposits, no slash)
        #[pallet::call_index(23)]
        #[pallet::weight(<T as Config>::WeightInfo::settle_dispute())]
        pub fn settle_dispute(origin: OriginFor<T>, domain: [u8; 8], id: u64) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            ensure!(
                Disputed::<T>::get(domain, id).is_some(),
                Error::<T>::NotDisputed
            );
            let deposit_record =
                TwoWayDeposits::<T>::get(domain, id).ok_or(Error::<T>::NotDisputed)?;
            ensure!(
                deposit_record.initiator == who || deposit_record.respondent == who,
                Error::<T>::NotAuthorized
            );
            ensure!(
                deposit_record.has_responded,
                Error::<T>::SettlementNotConfirmed
            );

            let escrow_account = Self::get_escrow_account();
            Self::release_deposit(
                &escrow_account,
                deposit_record.initiator_deposit,
                &HoldReason::DisputeInitiator,
                domain,
                id,
            )?;
            if let Some(respondent_deposit) = deposit_record.respondent_deposit {
                Self::release_deposit(
                    &escrow_account,
                    respondent_deposit,
                    &HoldReason::DisputeRespondent,
                    domain,
                    id,
                )?;
            }

            Self::unlock_all_evidence_cids(domain, id)?;
            PendingArbitrationDisputes::<T>::remove(domain, id);
            Disputed::<T>::remove(domain, id);
            EvidenceIds::<T>::remove(domain, id);
            TwoWayDeposits::<T>::remove(domain, id);

            if let Err(e) = T::Router::apply_decision(domain, id, Decision::Release) {
                log::warn!(
                    target: "pallet-dispute-arbitration",
                    "settle_dispute: apply_decision(Release) failed for ({:?}, {}): {:?}",
                    domain, id, e,
                );
            }

            Self::deposit_event(Event::DisputeSettled { domain, id });
            Ok(())
        }

        /// Start mediation (governance only)
        #[pallet::call_index(24)]
        #[pallet::weight(<T as Config>::WeightInfo::start_mediation())]
        pub fn start_mediation(origin: OriginFor<T>, complaint_id: u64) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(
                    state_machine::can_transition(&complaint.status, &ComplaintStatus::Mediating),
                    Error::<T>::InvalidState
                );

                let now = frame_system::Pallet::<T>::block_number();
                complaint.status = ComplaintStatus::Mediating;
                complaint.updated_at = now;

                Self::deposit_event(Event::ComplaintMediationStarted { complaint_id });
                Ok(())
            })
        }

        /// Dismiss a dispute (governance only, slashes initiator)
        #[pallet::call_index(25)]
        #[pallet::weight(<T as Config>::WeightInfo::dismiss_dispute())]
        pub fn dismiss_dispute(origin: OriginFor<T>, domain: [u8; 8], id: u64) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            ensure!(
                Disputed::<T>::get(domain, id).is_some(),
                Error::<T>::NotDisputed
            );

            let decision = Decision::Release;
            T::Router::apply_decision(domain, id, decision.clone())?;
            Self::handle_deposits_on_arbitration(domain, id, &decision)?;
            Self::unlock_all_evidence_cids(domain, id)?;

            PendingArbitrationDisputes::<T>::remove(domain, id);
            Self::archive_and_cleanup(domain, id, 0, 0);

            Self::deposit_event(Event::DisputeDismissed { domain, id });
            Ok(())
        }

        /// Dismiss a complaint (governance only, slashes complainant)
        #[pallet::call_index(26)]
        #[pallet::weight(<T as Config>::WeightInfo::dismiss_complaint())]
        pub fn dismiss_complaint(origin: OriginFor<T>, complaint_id: u64) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(!complaint.status.is_resolved(), Error::<T>::InvalidState);

                let now = frame_system::Pallet::<T>::block_number();
                complaint.status = ComplaintStatus::ResolvedRespondentWin;
                complaint.updated_at = now;

                Self::slash_complaint_deposit(
                    complaint_id,
                    &complaint.complainant,
                    &complaint.respondent,
                    complaint.domain,
                    &complaint.complaint_type,
                );
                PendingArbitrationComplaints::<T>::remove(complaint_id);
                Self::decrement_active_count(&complaint.complainant);

                DomainStats::<T>::mutate(complaint.domain, |stats| {
                    stats.resolved_count = stats.resolved_count.saturating_add(1);
                    stats.respondent_wins = stats.respondent_wins.saturating_add(1);
                });

                Self::deposit_event(Event::ComplaintDismissed { complaint_id });
                Ok(())
            })
        }

        /// Pause/unpause the module (governance only)
        #[pallet::call_index(27)]
        #[pallet::weight(Weight::from_parts(10_000_000, 1_000))]
        pub fn set_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;
            Paused::<T>::put(paused);
            Self::deposit_event(Event::PausedStateChanged { paused });
            Ok(())
        }

        /// Force close a stuck dispute (governance only)
        #[pallet::call_index(28)]
        #[pallet::weight(<T as Config>::WeightInfo::force_close_dispute())]
        pub fn force_close_dispute(
            origin: OriginFor<T>,
            domain: [u8; 8],
            id: u64,
        ) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;
            ensure!(
                Disputed::<T>::get(domain, id).is_some(),
                Error::<T>::NotDisputed
            );

            if let Some(deposit_record) = TwoWayDeposits::<T>::take(domain, id) {
                let escrow_account = Self::get_escrow_account();
                if Self::release_deposit(
                    &escrow_account,
                    deposit_record.initiator_deposit,
                    &HoldReason::DisputeInitiator,
                    domain,
                    id,
                )
                .is_err()
                {
                    Self::deposit_event(Event::DepositOperationFailed {
                        complaint_id: id,
                        operation: 0,
                        amount: deposit_record.initiator_deposit,
                    });
                }
                if let Some(respondent_deposit) = deposit_record.respondent_deposit {
                    if Self::release_deposit(
                        &escrow_account,
                        respondent_deposit,
                        &HoldReason::DisputeRespondent,
                        domain,
                        id,
                    )
                    .is_err()
                    {
                        Self::deposit_event(Event::DepositOperationFailed {
                            complaint_id: id,
                            operation: 1,
                            amount: respondent_deposit,
                        });
                    }
                }
            }

            let _ = Self::unlock_all_evidence_cids(domain, id);
            PendingArbitrationDisputes::<T>::remove(domain, id);
            Disputed::<T>::remove(domain, id);
            EvidenceIds::<T>::remove(domain, id);

            Self::deposit_event(Event::DisputeForceClosed { domain, id });
            Ok(())
        }

        /// Force close a stuck complaint (governance only)
        #[pallet::call_index(29)]
        #[pallet::weight(<T as Config>::WeightInfo::force_close_complaint())]
        pub fn force_close_complaint(origin: OriginFor<T>, complaint_id: u64) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(!complaint.status.is_resolved(), Error::<T>::InvalidState);

                let now = frame_system::Pallet::<T>::block_number();
                complaint.status = ComplaintStatus::Withdrawn;
                complaint.updated_at = now;

                Self::refund_complaint_deposit(complaint_id, &complaint.complainant);
                PendingArbitrationComplaints::<T>::remove(complaint_id);
                Self::decrement_active_count(&complaint.complainant);

                Self::deposit_event(Event::ComplaintForceClosed { complaint_id });
                Ok(())
            })
        }

        /// Set domain penalty rate (governance only)
        #[pallet::call_index(30)]
        #[pallet::weight(Weight::from_parts(10_000_000, 1_000))]
        pub fn set_domain_penalty_rate(
            origin: OriginFor<T>,
            domain: [u8; 8],
            rate_bps: Option<u16>,
        ) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;
            if let Some(rate) = rate_bps {
                ensure!(rate <= 10_000, Error::<T>::InvalidPenaltyRate);
                DomainPenaltyRates::<T>::insert(domain, rate);
            } else {
                DomainPenaltyRates::<T>::remove(domain);
            }
            Self::deposit_event(Event::DomainPenaltyRateUpdated { domain, rate_bps });
            Ok(())
        }

        // ==================== Appeal extrinsics (Phase 4) ====================

        /// Appeal a resolved complaint (losing party only, within window)
        #[pallet::call_index(31)]
        #[pallet::weight(<T as Config>::WeightInfo::appeal())]
        pub fn appeal(
            origin: OriginFor<T>,
            complaint_id: u64,
            appeal_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            ensure!(!Paused::<T>::get(), Error::<T>::ModulePaused);
            let who = ensure_signed(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;

                // Only the losing party can appeal
                let is_loser = match complaint.status {
                    ComplaintStatus::ResolvedComplainantWin => complaint.respondent == who,
                    ComplaintStatus::ResolvedRespondentWin => complaint.complainant == who,
                    _ => false,
                };
                ensure!(is_loser, Error::<T>::CannotAppeal);

                ensure!(
                    state_machine::can_transition(&complaint.status, &ComplaintStatus::Appealed),
                    Error::<T>::InvalidState
                );

                // Check appeal window
                let now = frame_system::Pallet::<T>::block_number();
                ensure!(
                    now.saturating_sub(complaint.updated_at) <= T::AppealWindowBlocks::get(),
                    Error::<T>::AppealWindowClosed
                );

                // Appeal deposit: 2x the current market-rate deposit (same pricing as file_complaint)
                // Original deposit was already refunded/slashed during resolve_complaint.
                // Guard against stale record (should be None after resolve).
                ensure!(
                    ComplaintDeposits::<T>::get(complaint_id).is_none(),
                    Error::<T>::InvalidState
                );

                let min_deposit = T::ComplaintDeposit::get();
                let base_deposit = if let Some(price) = T::Pricing::get_nex_to_usd_rate() {
                    let price_u128: u128 = price.saturated_into();
                    if price_u128 > 0u128 {
                        let deposit_usd = T::ComplaintDepositUsd::get();
                        let required_u128 =
                            (deposit_usd as u128).saturating_mul(1_000_000u128) / price_u128;
                        let required: BalanceOf<T> = required_u128.saturated_into();
                        if required > min_deposit {
                            required
                        } else {
                            min_deposit
                        }
                    } else {
                        min_deposit
                    }
                } else {
                    min_deposit
                };
                let appeal_deposit = base_deposit.saturating_mul(2u32.into());

                T::Fungible::hold(
                    &T::RuntimeHoldReason::from(HoldReason::ComplaintDeposit),
                    &who,
                    appeal_deposit,
                )
                .map_err(|_| Error::<T>::InsufficientDeposit)?;

                ComplaintDeposits::<T>::insert(complaint_id, appeal_deposit);

                complaint.status = ComplaintStatus::Appealed;
                complaint.appeal_cid = Some(appeal_cid.clone());
                complaint.appellant = Some(who.clone());
                complaint.updated_at = now;

                // Pin appeal_cid to IPFS (best-effort)
                let cid_vec: alloc::vec::Vec<u8> = appeal_cid.into_inner();
                let cid_size = cid_vec.len() as u64;
                if let Err(e) = T::StoragePin::pin(
                    who.clone(),
                    b"arbitration",
                    complaint_id,
                    None,
                    cid_vec,
                    cid_size,
                    pallet_storage_service::PinTier::Critical,
                ) {
                    log::warn!(target: "pallet-dispute-arbitration", "pin appeal_cid failed for complaint {}: {:?}", complaint_id, e);
                }

                // Re-activate: appeal brings complaint back to active state
                ActiveComplaintCount::<T>::mutate(&complaint.complainant, |c| {
                    *c = c.saturating_add(1)
                });

                PendingArbitrationComplaints::<T>::insert(complaint_id, ());

                Self::deposit_event(Event::AppealFiled {
                    complaint_id,
                    appellant: who,
                });
                Ok(())
            })
        }

        /// Resolve an appeal (governance only, final decision)
        #[pallet::call_index(32)]
        #[pallet::weight(<T as Config>::WeightInfo::resolve_appeal())]
        pub fn resolve_appeal(
            origin: OriginFor<T>,
            complaint_id: u64,
            decision: u8,
            reason_cid: BoundedVec<u8, T::MaxCidLen>,
        ) -> DispatchResult {
            T::DecisionOrigin::ensure_origin(origin)?;

            Complaints::<T>::try_mutate(complaint_id, |maybe_complaint| -> DispatchResult {
                let complaint = maybe_complaint
                    .as_mut()
                    .ok_or(Error::<T>::ComplaintNotFound)?;
                ensure!(
                    complaint.status == ComplaintStatus::Appealed,
                    Error::<T>::InvalidState
                );

                let appellant = complaint
                    .appellant
                    .clone()
                    .ok_or(Error::<T>::InvalidState)?;

                let now = frame_system::Pallet::<T>::block_number();
                complaint.resolution_cid = Some(reason_cid.clone());

                // Pin appeal resolution_cid to IPFS (best-effort, legal-grade data)
                let cid_vec: alloc::vec::Vec<u8> = reason_cid.into_inner();
                let cid_size = cid_vec.len() as u64;
                if let Err(e) = T::StoragePin::pin(
                    complaint.complainant.clone(),
                    b"arbitration",
                    complaint_id,
                    None,
                    cid_vec,
                    cid_size,
                    pallet_storage_service::PinTier::Critical,
                ) {
                    log::warn!(target: "pallet-dispute-arbitration", "pin appeal resolution_cid failed for complaint {}: {:?}", complaint_id, e);
                }

                let appellant_is_respondent = appellant == complaint.respondent;

                complaint.status = match decision {
                    0 => ComplaintStatus::ResolvedComplainantWin,
                    _ => ComplaintStatus::ResolvedRespondentWin,
                };
                complaint.updated_at = now;

                let appeal_succeeded = match (decision, appellant_is_respondent) {
                    (0, false) => true, // Complainant appealed, complainant wins
                    (1, true) => true,  // Respondent appealed, respondent wins
                    _ => false,
                };

                if appeal_succeeded {
                    Self::refund_complaint_deposit(complaint_id, &appellant);
                } else {
                    let beneficiary = if appellant_is_respondent {
                        &complaint.complainant
                    } else {
                        &complaint.respondent
                    };
                    Self::slash_complaint_deposit(
                        complaint_id,
                        &appellant,
                        beneficiary,
                        complaint.domain,
                        &complaint.complaint_type,
                    );
                }

                PendingArbitrationComplaints::<T>::remove(complaint_id);
                Self::decrement_active_count(&complaint.complainant);

                DomainStats::<T>::mutate(complaint.domain, |stats| {
                    stats.resolved_count = stats.resolved_count.saturating_add(1);
                    match decision {
                        0 | 2.. => {
                            stats.complainant_wins = stats.complainant_wins.saturating_add(1)
                        }
                        1 => stats.respondent_wins = stats.respondent_wins.saturating_add(1),
                    }
                });

                let complainant = complaint.complainant.clone();
                let respondent = complaint.respondent.clone();
                Self::deposit_event(Event::AppealResolved {
                    complaint_id,
                    decision,
                });

                let notice = Self::notice_complaint(
                    b"complaint:appeal_resolved",
                    complaint_id,
                    Some(decision),
                );
                T::Notifier::notify(&complainant, notice.clone());
                T::Notifier::notify(&respondent, notice);
                Ok(())
            })
        }
    }

    // ==================== Hooks ====================

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = Pallet::<T>::on_chain_storage_version();
            if on_chain < STORAGE_VERSION {
                log::info!(
                    target: "pallet-dispute-arbitration",
                    "Migrating storage to v{:?}", STORAGE_VERSION,
                );
                STORAGE_VERSION.put::<Pallet<T>>();
                T::DbWeight::get().reads_writes(1, 1)
            } else {
                T::DbWeight::get().reads(1)
            }
        }

        fn on_idle(now: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let mut weight_used = Weight::zero();
            let per_item_write = Weight::from_parts(25_000_000, 2_000);
            let per_item_read = Weight::from_parts(10_000_000, 1_000);
            let cursor_overhead = Weight::from_parts(5_000_000, 500);

            // Each stage: cursor read/write + N reads (scanning) + M writes (processing)
            // batch_weight(scan, process) = cursor_overhead + scan * per_item_read + process * per_item_write

            // Stage 1: Expire old complaints (scan up to 5, process up to 5)
            let stage_budget = cursor_overhead
                .saturating_add(per_item_read.saturating_mul(5))
                .saturating_add(per_item_write.saturating_mul(5));
            if remaining_weight.ref_time() > stage_budget.ref_time() {
                let expired = Self::expire_old_complaints(5);
                weight_used = weight_used.saturating_add(
                    cursor_overhead
                        .saturating_add(per_item_read.saturating_mul(5))
                        .saturating_add(per_item_write.saturating_mul(expired as u64)),
                );
            }

            // Stage 2: Auto-escalate stale Responded complaints
            let remaining = remaining_weight.saturating_sub(weight_used);
            let stage_budget = cursor_overhead
                .saturating_add(per_item_read.saturating_mul(3))
                .saturating_add(per_item_write.saturating_mul(3));
            if remaining.ref_time() > stage_budget.ref_time() {
                let escalated = Self::auto_escalate_stale_complaints(3);
                weight_used = weight_used.saturating_add(
                    cursor_overhead
                        .saturating_add(per_item_read.saturating_mul(3))
                        .saturating_add(per_item_write.saturating_mul(escalated as u64)),
                );
            }

            // Stage 3: Archive resolved complaints
            let remaining = remaining_weight.saturating_sub(weight_used);
            let stage_budget = cursor_overhead
                .saturating_add(per_item_read.saturating_mul(10))
                .saturating_add(per_item_write.saturating_mul(10));
            if remaining.ref_time() > stage_budget.ref_time() {
                let archived = Self::archive_old_complaints(10);
                weight_used = weight_used.saturating_add(
                    cursor_overhead
                        .saturating_add(per_item_read.saturating_mul(10))
                        .saturating_add(per_item_write.saturating_mul(archived as u64)),
                );
            }

            // Stage 4: Cleanup old archived disputes
            let current_block: u64 = now.saturated_into();
            let remaining = remaining_weight.saturating_sub(weight_used);
            let stage_budget = cursor_overhead
                .saturating_add(per_item_read.saturating_mul(5))
                .saturating_add(per_item_write.saturating_mul(5));
            if remaining.ref_time() > stage_budget.ref_time() {
                let cleaned = Self::cleanup_old_archived_disputes(current_block, 5);
                weight_used = weight_used.saturating_add(
                    cursor_overhead
                        .saturating_add(per_item_read.saturating_mul(5))
                        .saturating_add(per_item_write.saturating_mul(cleaned as u64)),
                );
            }

            // Stage 5: Cleanup old archived complaints
            let remaining = remaining_weight.saturating_sub(weight_used);
            let stage_budget = cursor_overhead
                .saturating_add(per_item_read.saturating_mul(5))
                .saturating_add(per_item_write.saturating_mul(5));
            if remaining.ref_time() > stage_budget.ref_time() {
                let cleaned = Self::cleanup_old_archived_complaints(current_block, 5);
                weight_used = weight_used.saturating_add(
                    cursor_overhead
                        .saturating_add(per_item_read.saturating_mul(5))
                        .saturating_add(per_item_write.saturating_mul(cleaned as u64)),
                );
            }

            weight_used
        }
    }

    // ==================== Runtime API helpers ====================

    impl<T: Config> Pallet<T> {
        const MAX_API_SCAN: u64 = 10_000;

        pub fn api_get_complaints_by_status(
            status: ComplaintStatus,
            offset: u32,
            limit: u32,
        ) -> alloc::vec::Vec<crate::runtime_api::ComplaintSummary<T::AccountId, BalanceOf<T>>>
        {
            let limit = limit.min(100) as usize;
            let mut skipped = 0u32;
            let mut results = alloc::vec::Vec::new();
            let max_id = NextComplaintId::<T>::get();
            let scan_end = max_id.min(Self::MAX_API_SCAN);

            for id in 0..scan_end {
                if results.len() >= limit {
                    break;
                }
                if let Some(c) = Complaints::<T>::get(id) {
                    if c.status == status {
                        if skipped < offset {
                            skipped += 1;
                            continue;
                        }
                        results.push(crate::runtime_api::ComplaintSummary {
                            id,
                            domain: c.domain,
                            object_id: c.object_id,
                            complaint_type: c.complaint_type,
                            complainant: c.complainant,
                            respondent: c.respondent,
                            amount: c.amount,
                            status: c.status,
                            created_at: c.created_at.saturated_into(),
                            updated_at: c.updated_at.saturated_into(),
                        });
                    }
                }
            }
            results
        }

        pub fn api_get_user_complaints(account: &T::AccountId) -> alloc::vec::Vec<u64> {
            let max_id = NextComplaintId::<T>::get();
            let scan_end = max_id.min(Self::MAX_API_SCAN);
            let mut ids = alloc::vec::Vec::new();
            for id in 0..scan_end {
                if let Some(c) = Complaints::<T>::get(id) {
                    if c.status.is_active()
                        && (c.complainant == *account || c.respondent == *account)
                    {
                        ids.push(id);
                    }
                }
            }
            ids
        }

        pub fn api_get_complaint_detail(
            complaint_id: u64,
        ) -> Option<crate::runtime_api::ComplaintDetail<T::AccountId, BalanceOf<T>>> {
            let c = Complaints::<T>::get(complaint_id)?;
            let deposit = ComplaintDeposits::<T>::get(complaint_id);
            let evidence_cids: alloc::vec::Vec<alloc::vec::Vec<u8>> =
                ComplaintEvidenceCids::<T>::get(complaint_id)
                    .into_iter()
                    .map(|bv| bv.into_inner())
                    .collect();

            Some(crate::runtime_api::ComplaintDetail {
                id: complaint_id,
                domain: c.domain,
                object_id: c.object_id,
                complaint_type: c.complaint_type,
                complainant: c.complainant,
                respondent: c.respondent,
                details_cid: c.details_cid.into_inner(),
                response_cid: c.response_cid.map(|v| v.into_inner()),
                amount: c.amount,
                status: c.status,
                created_at: c.created_at.saturated_into(),
                response_deadline: c.response_deadline.saturated_into(),
                settlement_cid: c.settlement_cid.map(|v| v.into_inner()),
                resolution_cid: c.resolution_cid.map(|v| v.into_inner()),
                appeal_cid: c.appeal_cid.map(|v| v.into_inner()),
                appellant: c.appellant,
                updated_at: c.updated_at.saturated_into(),
                deposit,
                evidence_cids,
            })
        }
    }
}
