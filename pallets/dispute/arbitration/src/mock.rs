use crate as pallet_dispute_arbitration;
use frame_support::{derive_impl, parameter_types, traits::{ConstU32, EitherOfDiverse, SortedMembers}, PalletId};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Escrow: pallet_dispute_escrow,
        Arbitration: pallet_dispute_arbitration,
    }
);

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
    pub const EscrowPalletId: PalletId = PalletId(*b"py/escro");
    pub const TreasuryAccountId: u64 = 99;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ExistentialDeposit;
    type RuntimeHoldReason = RuntimeHoldReason;
}

pub struct TestExpiryPolicy;
impl pallet_dispute_escrow::pallet::ExpiryPolicy<u64, u64> for TestExpiryPolicy {
    fn on_expire(
        id: u64,
    ) -> Result<pallet_dispute_escrow::pallet::ExpiryAction<u64>, sp_runtime::DispatchError> {
        if id % 2 == 0 {
            Ok(pallet_dispute_escrow::pallet::ExpiryAction::ReleaseAll(99))
        } else {
            Ok(pallet_dispute_escrow::pallet::ExpiryAction::RefundAll(99))
        }
    }
}

parameter_types! {
    pub const MaxDisputeDuration: u64 = 14400;
}

impl pallet_dispute_escrow::Config for Test {
    type Currency = Balances;
    type EscrowPalletId = EscrowPalletId;
    type AuthorizedOrigin = frame_system::EnsureSigned<u64>;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type MaxExpiringPerBlock = ConstU32<10>;
    type MaxSplitEntries = ConstU32<10>;
    type ExpiryPolicy = TestExpiryPolicy;
    type WeightInfo = ();
    type MaxReasonLen = ConstU32<128>;
    type Observer = ();
    type MaxCleanupPerCall = ConstU32<10>;
    type MaxDisputeDuration = MaxDisputeDuration;
}

// -- Mock thread-local state --

use core::cell::RefCell;

thread_local! {
    static CAN_DISPUTE: RefCell<bool> = RefCell::new(true);
    static COUNTERPARTY: RefCell<Option<u64>> = RefCell::new(Some(2));
    static ORDER_AMOUNT: RefCell<Option<Balance>> = RefCell::new(Some(10_000));
    static APPLY_DECISION_OK: RefCell<bool> = RefCell::new(true);
    static EVIDENCE_EXISTS: RefCell<bool> = RefCell::new(true);
}

#[allow(dead_code)]
pub fn set_can_dispute(val: bool) {
    CAN_DISPUTE.with(|v| *v.borrow_mut() = val);
}

#[allow(dead_code)]
pub fn set_counterparty(val: Option<u64>) {
    COUNTERPARTY.with(|v| *v.borrow_mut() = val);
}

#[allow(dead_code)]
pub fn set_order_amount(val: Option<Balance>) {
    ORDER_AMOUNT.with(|v| *v.borrow_mut() = val);
}

pub fn set_evidence_exists(val: bool) {
    EVIDENCE_EXISTS.with(|v| *v.borrow_mut() = val);
}

// -- MockRouter (simplified: no ban_account, no get_maker_id) --

pub struct MockRouter;
impl pallet_dispute_arbitration::pallet::ArbitrationRouter<u64, Balance> for MockRouter {
    fn can_dispute(_domain: [u8; 8], _who: &u64, _id: u64) -> bool {
        CAN_DISPUTE.with(|v| *v.borrow())
    }
    fn apply_decision(
        _domain: [u8; 8],
        _id: u64,
        _decision: crate::types::Decision,
    ) -> sp_runtime::DispatchResult {
        if APPLY_DECISION_OK.with(|v| *v.borrow()) {
            Ok(())
        } else {
            Err(sp_runtime::DispatchError::Other("router failed"))
        }
    }
    fn get_counterparty(
        _domain: [u8; 8],
        _initiator: &u64,
        _id: u64,
    ) -> Result<u64, sp_runtime::DispatchError> {
        COUNTERPARTY.with(|v| {
            v.borrow()
                .ok_or(sp_runtime::DispatchError::Other("no counterparty"))
        })
    }
    fn get_order_amount(_domain: [u8; 8], _id: u64) -> Result<Balance, sp_runtime::DispatchError> {
        ORDER_AMOUNT.with(|v| {
            v.borrow()
                .ok_or(sp_runtime::DispatchError::Other("no order amount"))
        })
    }
}

pub struct MockEscrow;
impl pallet_dispute_escrow::pallet::Escrow<u64, Balance> for MockEscrow {
    fn escrow_account() -> u64 {
        50
    }
    fn lock_from(_payer: &u64, _id: u64, _amount: Balance) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn transfer_from_escrow(_id: u64, _to: &u64, _amount: Balance) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn release_all(_id: u64, _to: &u64) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn refund_all(_id: u64, _to: &u64) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn amount_of(_id: u64) -> Balance {
        100_000
    }
    fn split_partial(
        _id: u64,
        _release_to: &u64,
        _refund_to: &u64,
        _bps: u16,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn set_disputed(_id: u64) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn set_resolved(_id: u64) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn refund_partial(_id: u64, _to: &u64, _amount: Balance) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn release_partial(_id: u64, _to: &u64, _amount: Balance) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

pub struct MockCidLockManager;
impl pallet_storage_service::CidLockManager<sp_core::H256, u64> for MockCidLockManager {
    fn lock_cid(
        _cid_hash: sp_core::H256,
        _reason: Vec<u8>,
        _until: Option<u64>,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn unlock_cid(_cid_hash: sp_core::H256, _reason: Vec<u8>) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn is_locked(_cid_hash: &sp_core::H256) -> bool {
        false
    }
}

pub struct MockPricing;
impl pallet_trading_common::PricingProvider<Balance> for MockPricing {
    fn get_nex_to_usd_rate() -> Option<Balance> {
        Some(10_000_000)
    }
    fn report_p2p_trade(
        _timestamp: u64,
        _price_usdt: u64,
        _nex_qty: u128,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

pub struct MockStoragePin;
impl pallet_storage_service::StoragePin<u64> for MockStoragePin {
    fn pin(
        _owner: u64,
        _domain: &[u8],
        _subject_id: u64,
        _entity_id: Option<u64>,
        _cid: Vec<u8>,
        _size_bytes: u64,
        _tier: pallet_storage_service::PinTier,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn unpin(_owner: u64, _cid: Vec<u8>) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

pub struct MockEvidenceChecker;
impl pallet_dispute_arbitration::pallet::EvidenceExistenceChecker for MockEvidenceChecker {
    fn evidence_exists(_id: u64) -> bool {
        EVIDENCE_EXISTS.with(|v| *v.borrow())
    }
}

// -- Arbitration Config --

parameter_types! {
    pub const DepositRatioBps: u16 = 1500;
    pub const ResponseDeadline: u64 = 100;
    pub const RejectedSlashBps: u16 = 3000;
    pub const PartialSlashBps: u16 = 5000;
    pub const ComplaintDeposit: Balance = 100;
    pub const ComplaintDepositUsd: u64 = 1_000_000;
    pub const ComplaintSlashBps: u16 = 5000;
    pub const ArchiveTtlBlocks: u32 = 1000;
    pub const ComplaintArchiveDelayBlocks: u64 = 50;
    pub const ComplaintMaxLifetimeBlocks: u64 = 500;
    pub const AppealWindowBlocks: u64 = 50;
    pub const AutoEscalateBlocks: u64 = 200;
    pub const MaxActivePerUser: u32 = 50;
    pub const ArbitrationGovernanceAccount: u64 = 99;
}

pub struct ArbitrationGovernanceMembers;
impl SortedMembers<u64> for ArbitrationGovernanceMembers {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

thread_local! {
    /// 记录系统通知调用 (to, notice)。/ Records notify calls as (to, notice).
    pub static NOTIFY_LOG: core::cell::RefCell<Vec<(u64, Vec<u8>)>> =
        core::cell::RefCell::new(Vec::new());
}

/// 记录式仲裁通知 mock。/ Recording arbitration-notifier mock.
pub struct RecordingNotifier;
impl crate::ArbitrationNotifier<u64> for RecordingNotifier {
    fn notify(to: &u64, notice: Vec<u8>) {
        NOTIFY_LOG.with(|l| l.borrow_mut().push((*to, notice)));
    }
}

/// 读取通知日志（测试辅助）。/ Read the notify log (test helper).
#[allow(dead_code)]
pub fn notify_log() -> Vec<(u64, Vec<u8>)> {
    NOTIFY_LOG.with(|l| l.borrow().clone())
}

impl pallet_dispute_arbitration::pallet::Config for Test {
    type MaxEvidence = ConstU32<10>;
    type MaxCidLen = ConstU32<64>;
    type Escrow = MockEscrow;
    type WeightInfo = ();
    type Router = MockRouter;
    type DecisionOrigin = EitherOfDiverse<frame_system::EnsureRoot<u64>, frame_system::EnsureSignedBy<ArbitrationGovernanceMembers, u64>>;
    type Fungible = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type DepositRatioBps = DepositRatioBps;
    type ResponseDeadline = ResponseDeadline;
    type RejectedSlashBps = RejectedSlashBps;
    type PartialSlashBps = PartialSlashBps;
    type ComplaintDeposit = ComplaintDeposit;
    type ComplaintDepositUsd = ComplaintDepositUsd;
    type Pricing = MockPricing;
    type ComplaintSlashBps = ComplaintSlashBps;
    type TreasuryAccount = TreasuryAccountId;
    type CidLockManager = MockCidLockManager;
    type StoragePin = MockStoragePin;
    type ArchiveTtlBlocks = ArchiveTtlBlocks;
    type ComplaintArchiveDelayBlocks = ComplaintArchiveDelayBlocks;
    type ComplaintMaxLifetimeBlocks = ComplaintMaxLifetimeBlocks;
    type EvidenceExists = MockEvidenceChecker;
    type AppealWindowBlocks = AppealWindowBlocks;
    type AutoEscalateBlocks = AutoEscalateBlocks;
    type MaxActivePerUser = MaxActivePerUser;
    type Notifier = RecordingNotifier;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    CAN_DISPUTE.with(|v| *v.borrow_mut() = true);
    COUNTERPARTY.with(|v| *v.borrow_mut() = Some(2));
    ORDER_AMOUNT.with(|v| *v.borrow_mut() = Some(10_000));
    APPLY_DECISION_OK.with(|v| *v.borrow_mut() = true);
    EVIDENCE_EXISTS.with(|v| *v.borrow_mut() = true);
    NOTIFY_LOG.with(|l| l.borrow_mut().clear());

    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 10_000_000),
            (2, 10_000_000),
            (3, 10_000_000),
            (50, 10_000_000),
            (99, 10_000_000),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
