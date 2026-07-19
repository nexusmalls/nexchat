use crate as pallet_dispute_escrow;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU32, ConstU64},
    PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Escrow: pallet_dispute_escrow,
    }
);

parameter_types! {
    pub const ExistentialDeposit: u128 = 1;
    pub const EscrowPalletId: PalletId = PalletId(*b"py/escro");
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<u64>;
    type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = u128;
    type ExistentialDeposit = ExistentialDeposit;
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

pub struct EscrowAdminMembers;
impl frame_support::traits::SortedMembers<u64> for EscrowAdminMembers {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

impl pallet_dispute_escrow::Config for Test {
    type Currency = Balances;
    type EscrowPalletId = EscrowPalletId;
    type AuthorizedOrigin = frame_system::EnsureSigned<u64>;
    type AdminOrigin = frame_support::traits::EitherOfDiverse<
        frame_system::EnsureRoot<u64>,
        frame_system::EnsureSignedBy<EscrowAdminMembers, u64>,
    >;
    type MaxExpiringPerBlock = ConstU32<10>;
    type MaxSplitEntries = ConstU32<10>;
    type ExpiryPolicy = TestExpiryPolicy;
    /// 🆕 F5: 争议原因最大长度
    type MaxReasonLen = ConstU32<256>;
    /// 🆕 F10: 观察者（测试用空实现）
    type Observer = ();
    type MaxCleanupPerCall = ConstU32<50>;
    /// 争议超时 100800 块 (≈7天 @ 6s/block)，测试中用小值
    type MaxDisputeDuration = ConstU64<100800>;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 10_000), (2, 10_000), (3, 10_000), (99, 10_000)],
        dev_accounts: None,
    }
    .assimilate_storage(&mut t)
    .unwrap();

    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
