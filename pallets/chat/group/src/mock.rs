//! Mock runtime for `pallet-chat-group` unit tests.
//! `pallet-chat-group` 单元测试用 mock runtime。

use crate as pallet_chat_group;
use crate::{GroupChatHook, GroupId};
use frame_support::traits::{ConstU128, ConstU32, ConstU64};
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};
use sp_std::vec::Vec;

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        ChatGroup: pallet_chat_group,
    }
);

impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type RuntimeTask = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
    type ExtensionsWeightInfo = ();
}

impl pallet_balances::Config for Test {
    type MaxLocks = ();
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type DoneSlashHandler = ();
}

thread_local! {
    /// 记录 ChatHook 触发的成员授权镜像事件：(added?, group_id, member, counterparty)。
    /// Records ChatHook membership-mirroring events: (added?, group_id, member, counterparty).
    ///
    /// 用于验证「1:1 = 2 人群」时成员关系正确镜像到外部授权层（runtime 中即
    /// chat-permission 的场景授权），从而让这对用户获得 1:1 私聊权限。
    /// Used to assert that a 2-member (1:1) group mirrors membership to the external
    /// authorization layer so the pair gains direct-message rights.
    pub static HOOK_EVENTS: core::cell::RefCell<Vec<(bool, GroupId, u64, u64)>> =
        core::cell::RefCell::new(Vec::new());
}

/// 测试用记录型 ChatHook：把成员增减事件推入 `HOOK_EVENTS`。
/// Test recording ChatHook: pushes membership add/remove events into `HOOK_EVENTS`.
pub struct RecordingHook;
impl GroupChatHook<u64> for RecordingHook {
    fn on_member_added(group_id: GroupId, member: &u64, counterparty: &u64) {
        HOOK_EVENTS.with(|e| {
            e.borrow_mut()
                .push((true, group_id, *member, *counterparty))
        });
    }
    fn on_member_removed(group_id: GroupId, member: &u64, counterparty: &u64) {
        HOOK_EVENTS.with(|e| {
            e.borrow_mut()
                .push((false, group_id, *member, *counterparty))
        });
    }
}

/// 取出并清空已记录的 ChatHook 事件。 / Drain recorded ChatHook events.
pub fn drain_hook_events() -> Vec<(bool, GroupId, u64, u64)> {
    HOOK_EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()))
}

pub struct MockPlatformMuteCheck;
impl crate::PlatformMuteChecker<u64> for MockPlatformMuteCheck {
    fn is_platform_muted(who: &u64) -> bool {
        PLATFORM_MUTED.with(|m| m.borrow().contains(who))
    }
}

thread_local! {
    static PLATFORM_MUTED: core::cell::RefCell<Vec<u64>> = core::cell::RefCell::new(Vec::new());
}

/// EN: Mark `who` platform-muted for tests. CN: 测试用：标记平台禁言。
pub fn mute_platform(who: u64) {
    PLATFORM_MUTED.with(|m| m.borrow_mut().push(who));
}

/// EN: Clear platform-mute test state. CN: 清空平台禁言测试状态。
pub fn clear_platform_mutes() {
    PLATFORM_MUTED.with(|m| m.borrow_mut().clear());
}

impl pallet_chat_group::Config for Test {
    type Currency = Balances;
    type GroupDeposit = ConstU128<100>;
    type KeyPackageDeposit = ConstU128<10>;
    type MaxPendingJoins = ConstU32<16>;
    type ChatHook = RecordingHook;
    type PlatformMuteCheck = MockPlatformMuteCheck;
    type MaxGroupMembers = ConstU32<8>;
    type MaxGroupsPerUser = ConstU32<4>;
    type MaxKeyPackageLen = ConstU32<256>;
    type MaxHandshakeLen = ConstU32<512>;
    type MaxWelcomeLen = ConstU32<512>;
    type MaxCidLen = ConstU32<96>;
    type MaxGroupNameLen = ConstU32<64>;
    type MaxGroupAnnouncementLen = ConstU32<512>;
    type MaxGroupNicknameLen = ConstU32<48>;
    type MaxKeyPackagesPerUser = ConstU32<4>;
    type GroupCreationCooldown = ConstU64<10>;
    type MlsActionWindow = ConstU64<50>;
    type MaxMlsActionsPerWindow = ConstU32<50>;
    type JoinRequestWindow = ConstU64<50>;
    type MaxJoinRequestsPerWindow = ConstU32<5>;
    type GovernanceOrigin = frame_system::EnsureRoot<u64>;
    type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    // 给测试账户充足余额以预留押金 / fund test accounts for deposits
    pallet_balances::GenesisConfig::<Test> {
        balances: (1..=20u64).map(|a| (a, 1_000_000u128)).collect(),
        ..Default::default()
    }
    .assimilate_storage(&mut t)
    .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        System::set_block_number(System::block_number() + 1);
    }
}
