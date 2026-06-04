//! # 聊天权限系统测试模拟环境
//!
//! 为 pallet-chat-permission 单元测试提供模拟 Runtime 配置。

use crate as pallet_chat_permission;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU32, ConstU64},
};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup},
    BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

// 构建测试 Runtime
frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        ChatPermission: pallet_chat_permission,
    }
);

/// 函数级中文注释：系统 Pallet 默认配置
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
}

parameter_types! {
    /// 函数级中文注释：每对用户最大场景授权数量（测试用：5）
    pub const MaxScenesPerPair: u32 = 5;
    pub const MaxReportCidLen: u32 = 64;
    pub const MaxOpenReports: u32 = 3;
    pub const ReportCooldown: u64 = 5;
}

// 函数级中文注释：聊天权限 Pallet 测试配置
impl pallet_chat_permission::Config for Test {
    type MaxScenesPerPair = MaxScenesPerPair;
    type GovernanceOrigin = frame_system::EnsureRoot<u64>;
    type MaxReportCidLen = MaxReportCidLen;
    type MaxOpenReports = MaxOpenReports;
    type ReportCooldown = ReportCooldown;
    type WeightInfo = ();
}

/// 函数级中文注释：测试账户常量
pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const CHARLIE: u64 = 3;
#[allow(dead_code)]
pub const DAVE: u64 = 4;

/// 函数级中文注释：测试源 pallet 标识
pub const OTC_ORDER_SOURCE: [u8; 8] = *b"otc_ordr";
pub const MAKER_SOURCE: [u8; 8] = *b"maker___";
#[allow(dead_code)]
pub const MEMORIAL_SOURCE: [u8; 8] = *b"memorial";

/// 函数级中文注释：构建测试外部环境
///
/// 创建新的测试环境，包含干净的存储状态。
pub fn new_test_ext() -> sp_io::TestExternalities {
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

/// 函数级中文注释：推进区块号
///
/// 用于测试时间相关功能（如过期检查）。
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        System::set_block_number(System::block_number() + 1);
    }
}
