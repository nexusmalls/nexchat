use crate as pallet_dispute_evidence;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU128, ConstU32, ConstU64, EitherOfDiverse, SortedMembers},
    PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        EvidencePallet: pallet_dispute_evidence,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
    type AccountData = pallet_balances::AccountData<u128>;
}

impl pallet_balances::Config for Test {
    type Balance = u128;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ConstU128<1>;
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

// -- Mock EvidenceAuthorizer --

use core::cell::RefCell;

thread_local! {
    static AUTHORIZED: RefCell<bool> = RefCell::new(true);
    static SEAL_AUTHORIZED: RefCell<bool> = RefCell::new(true);
}

pub fn set_authorized(val: bool) {
    AUTHORIZED.with(|v| *v.borrow_mut() = val);
}

pub fn set_seal_authorized(val: bool) {
    SEAL_AUTHORIZED.with(|v| *v.borrow_mut() = val);
}

pub struct MockAuthorizer;
impl pallet_dispute_evidence::pallet::EvidenceAuthorizer<u64> for MockAuthorizer {
    fn is_authorized(_ns: [u8; 8], _who: &u64) -> bool {
        AUTHORIZED.with(|v| *v.borrow())
    }
}

pub struct MockSealAuthorizer;
impl pallet_dispute_evidence::pallet::EvidenceSealAuthorizer<u64> for MockSealAuthorizer {
    fn can_seal(_ns: [u8; 8], _who: &u64) -> bool {
        SEAL_AUTHORIZED.with(|v| *v.borrow())
    }
}

// -- Mock StoragePin --

pub struct MockStoragePin;
impl pallet_storage_service::StoragePin<u64> for MockStoragePin {
    fn pin(
        _owner: u64,
        _domain: &[u8],
        _subject_id: u64,
        _entity_id: Option<u64>,
        _cid: Vec<u8>,
        _size: u64,
        _tier: pallet_storage_service::PinTier,
    ) -> sp_runtime::DispatchResult {
        Ok(())
    }
    fn unpin(_owner: u64, _cid: Vec<u8>) -> sp_runtime::DispatchResult {
        Ok(())
    }
}

// -- Evidence Config --

parameter_types! {
    pub const EvidenceNsBytes: [u8; 8] = *b"evidence";
    pub const EnableGlobalCidDedup: bool = true;
    pub const WindowBlocks: u64 = 100;
    pub const EvidenceDeposit: u128 = 10;
    pub const CommitRevealDeadline: u64 = 500;
    pub const GovernanceAccount: u64 = 99;
}

pub struct GovernanceMembers;
impl SortedMembers<u64> for GovernanceMembers {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

impl pallet_dispute_evidence::pallet::Config for Test {
    type MaxContentCidLen = ConstU32<64>;
    type MaxSchemeLen = ConstU32<32>;
    type MaxCidLen = ConstU32<64>;
    type MaxImg = ConstU32<10>;
    type MaxVid = ConstU32<5>;
    type MaxDoc = ConstU32<5>;
    type MaxMemoLen = ConstU32<256>;
    type MaxAuthorizedUsers = ConstU32<20>;
    type MaxKeyLen = ConstU32<128>;
    type EvidenceNsBytes = EvidenceNsBytes;
    type Authorizer = MockAuthorizer;
    type SealAuthorizer = MockSealAuthorizer;
    type MaxPerSubjectTarget = ConstU32<100>;
    type MaxPerSubjectNs = ConstU32<100>;
    type WindowBlocks = WindowBlocks;
    type MaxPerWindow = ConstU32<50>;
    type EnableGlobalCidDedup = EnableGlobalCidDedup;
    type MaxListLen = ConstU32<50>;
    type WeightInfo = ();
    type StoragePin = MockStoragePin;
    type Currency = Balances;
    type EvidenceDeposit = EvidenceDeposit;
    type EvidenceDepositUsd = ConstU64<500_000>;
    type CommitRevealDeadline = CommitRevealDeadline;
    type MaxLinksPerEvidence = ConstU32<50>;
    type MaxSupplements = ConstU32<100>;
    type MaxPendingRequestsPerContent = ConstU32<20>;
    type ArchiveTtlBlocks = ConstU32<1000>;
    type ArchiveDelayBlocks = ConstU32<50>;
    type PrivateContentDeposit = EvidenceDeposit; // reuse same amount for tests
    type PrivateContentDepositUsd = ConstU64<500_000>;
    type DepositCalculator = ();
    type AccessRequestTtlBlocks = ConstU64<200>;
    type GovernanceOrigin = EitherOfDiverse<
        frame_system::EnsureRoot<u64>,
        frame_system::EnsureSignedBy<GovernanceMembers, u64>,
    >;
    type MaxReasonLen = ConstU32<256>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    AUTHORIZED.with(|v| *v.borrow_mut() = true);
    SEAL_AUTHORIZED.with(|v| *v.borrow_mut() = true);

    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 10_000),
            (2, 10_000),
            (3, 10_000),
            (4, 10_000),
            (5, 10_000),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
