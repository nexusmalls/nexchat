use crate as pallet_task_bounty;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU8, ConstU16, ConstU32, ConstU64},
    PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

pub type AccountId = u64;
pub type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Escrow: pallet_dispute_escrow,
        TaskBounty: pallet_task_bounty,
    }
);

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
    pub const EscrowPalletId: PalletId = PalletId(*b"py/escro");
    pub const TaskBountyPalletId: PalletId = PalletId(*b"py/tbnty");
    pub const FeeCollector: AccountId = 999;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = Balance;
    type ExistentialDeposit = ExistentialDeposit;
}

pub struct TestExpiryPolicy;
impl pallet_dispute_escrow::pallet::ExpiryPolicy<AccountId, u64> for TestExpiryPolicy {
    fn on_expire(
        _id: u64,
    ) -> Result<pallet_dispute_escrow::pallet::ExpiryAction<AccountId>, sp_runtime::DispatchError> {
        Ok(pallet_dispute_escrow::pallet::ExpiryAction::RefundAll(0))
    }
}

impl pallet_dispute_escrow::Config for Test {
    type Currency = Balances;
    type EscrowPalletId = EscrowPalletId;
    type AuthorizedOrigin = frame_system::EnsureSigned<AccountId>;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxExpiringPerBlock = ConstU32<10>;
    type MaxSplitEntries = ConstU32<10>;
    type ExpiryPolicy = TestExpiryPolicy;
    type MaxReasonLen = ConstU32<256>;
    type Observer = ();
    type MaxCleanupPerCall = ConstU32<50>;
    type MaxDisputeDuration = ConstU64<100800>;
    type WeightInfo = ();
}

parameter_types! {
    pub const EscrowIdOffset: u64 = 1u64 << 60;
    pub const MinReward: Balance = 100;
    pub const MaxQuotaUnitReward: Balance = 1_000_000;
    /// Poster reputation gate triggers only for total reward >= 5000. / 总额 ≥5000 才校验发布方声誉。
    pub const PosterRepThreshold: Balance = 5_000;
    /// Category 1 = ground-promotion project; requires a region. / 类目 1=地推项目，需地区。
    pub const GroundPromoCategory: u16 = 1;
}

/// Mock evidence ownership: any non-zero id is considered poster-owned.
/// Mock 证据归属：任意非 0 id 视为发布方自有。
pub struct MockEvidence;
impl pallet_task_bounty::EvidenceOwnership<AccountId> for MockEvidence {
    fn is_owner(evidence_id: u64, _who: &AccountId) -> bool {
        evidence_id != 0
    }
}

thread_local! {
    /// Records chat authorization calls as (is_grant, bounty_id, poster, solver). / 记录聊天授权调用。
    pub static CHAT_LOG: core::cell::RefCell<Vec<(bool, u64, AccountId, AccountId)>> =
        core::cell::RefCell::new(Vec::new());
}

/// Mock chat authorizer: records grant/revoke into `CHAT_LOG`. / 记录式聊天授权 mock。
pub struct RecordingChat;
impl pallet_task_bounty::ChatAuthorizer<AccountId> for RecordingChat {
    fn grant(bounty_id: u64, poster: &AccountId, solver: &AccountId) {
        CHAT_LOG.with(|l| l.borrow_mut().push((true, bounty_id, *poster, *solver)));
    }
    fn revoke(bounty_id: u64, poster: &AccountId, solver: &AccountId) {
        CHAT_LOG.with(|l| l.borrow_mut().push((false, bounty_id, *poster, *solver)));
    }
}

/// Test helper: drain and return the recorded chat actions. / 取出并清空已记录的聊天动作。
pub fn chat_log_take() -> Vec<(bool, u64, AccountId, AccountId)> {
    CHAT_LOG.with(|l| core::mem::take(&mut *l.borrow_mut()))
}

impl pallet_task_bounty::Config for Test {
    type Currency = Balances;
    type Escrow = Escrow;
    type EscrowIdOffset = EscrowIdOffset;
    type PalletId = TaskBountyPalletId;
    type StakeBps = ConstU16<1000>; // 10%
    type FeeBps = ConstU16<500>; // 5%
    type FeeCollector = FeeCollector;
    type MaxSubmissions = ConstU32<10>;
    type MaxSlots = ConstU32<100>;
    type MinReward = MinReward;
    type MaxQuotaUnitReward = MaxQuotaUnitReward;
    type DefaultDuration = ConstU64<100>;
    type MinOpenWindow = ConstU64<3>;
    type MinKycLevelForPayout = ConstU8<0>; // platform KYC off in MVP mock
    type Kyc = ();
    type Evidence = MockEvidence;
    type GroundPromoCategory = GroundPromoCategory;
    type Chat = RecordingChat;
    type BountyReputation = TaskBounty;
    type MinSolverReputation = ConstU32<1000>;
    type MinPosterReputation = ConstU32<1000>;
    type PosterReputationRewardThreshold = PosterRepThreshold;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, 10_000_000),
            (2, 10_000_000),
            (3, 10_000_000),
            (4, 10_000_000),
            (5, 10_000_000),
        ],
        dev_accounts: None,
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// Advance the block number to `n`. / 推进区块到 `n`。
pub fn run_to(n: u64) {
    System::set_block_number(n);
}
