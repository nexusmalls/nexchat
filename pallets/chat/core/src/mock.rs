//! # Mock环境配置
//! 
//! 用于Chat Pallet的单元测试

use crate as pallet_chat;
use frame_support::{
	parameter_types,
    traits::{ConstU32, ConstU64, EnsureOrigin, OriginTrait, Randomness, UnixTime},
};
use frame_system::RawOrigin;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

// 配置测试运行时
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Chat: pallet_chat,
	}
);

// System配置
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
	type AccountData = ();
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

/// EN: Test origin gate — Root maps to [`SystemChatAccount`], Signed maps to the signer
/// (matches production Root→system account while keeping unit tests unchanged).
/// CN: 测试 origin 门控——Root 映射为 [`SystemChatAccount`]，Signed 映射为签名者
/// （对齐生产 Root→系统账户，同时保持单测写法不变）。
pub struct MockSystemMessageOrigin;
impl EnsureOrigin<RuntimeOrigin> for MockSystemMessageOrigin {
	type Success = u64;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		match o.as_system_ref() {
			Some(RawOrigin::Root) => Ok(SystemChatAccount::get()),
			Some(RawOrigin::Signed(who)) => Ok(*who),
			_ => Err(o),
		}
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(<RuntimeOrigin as OriginTrait>::root())
	}
}

// Chat Pallet配置
parameter_types! {
	/// IPFS CID最大长度：100字节（足够容纳加密后的CID）
	pub const MaxCidLen: u32 = 100;
	/// 消息过期时间：1000个区块（测试用）
	pub const MessageExpirationTime: u64 = 1000;
	/// 撤回时间窗口：50个区块（测试用）
	pub const MessageRecallWindow: u64 = 50;
	/// 程序化系统通知的发信账户（测试用，区别于普通账户 1..=4）。
	/// Platform system sender account for programmatic notifications (test).
	pub const SystemChatAccount: u64 = 9_999;
}

thread_local! {
	/// 每次 `random` 调用自增的 nonce。 / Per-call nonce.
	///
	/// 真实 runtime 使用变化的随机源；本 mock 的 `subject` 恒为 `b"chat_user_id"`，
	/// 若返回固定值，则同块内多个用户 / 多次重试会算出相同候选 ID 而无限碰撞。
	/// 这里混入自增 nonce，保证每次调用产生不同种子。
	static RAND_NONCE: core::cell::Cell<u64> = core::cell::Cell::new(0);
}

/// 简单的测试用随机数生成器（逐次调用产生不同值，避免 ID 生成碰撞）。
pub struct TestRandomness;
impl Randomness<sp_core::H256, u64> for TestRandomness {
	fn random(subject: &[u8]) -> (sp_core::H256, u64) {
		// 取出并自增调用 nonce。 / Take and bump the per-call nonce.
		let nonce = RAND_NONCE.with(|n| {
			let v = n.get();
			n.set(v.wrapping_add(1));
			v
		});

		// 简单的伪随机实现用于测试
		let mut seed = [0u8; 32];
		for (i, byte) in subject.iter().enumerate() {
			if i < 32 {
				seed[i] = *byte;
			}
		}
		// 混入 nonce（落在 ID 生成实际使用的前 8 字节内），保证逐次调用不同。
		let nonce_bytes = nonce.to_le_bytes();
		for i in 0..8 {
			seed[i] = seed[i].wrapping_add(nonce_bytes[i]);
		}
		// 使用简单的变换生成不同的随机值
		for i in 0..32 {
			seed[i] = seed[i].wrapping_add(i as u8).wrapping_add(1);
		}
		(sp_core::H256::from(seed), frame_system::Pallet::<Test>::block_number())
	}
}

/// 简单的测试用时间戳
pub struct TestTime;
impl UnixTime for TestTime {
	fn now() -> core::time::Duration {
		// 返回基于区块号的简单时间戳
		let block_number = frame_system::Pallet::<Test>::block_number();
		core::time::Duration::from_secs(block_number * 6) // 6秒/块
	}
}

thread_local! {
	/// 被拒绝的 (sender, receiver) 有向对集合。 / Denied directed (sender, receiver) pairs.
	///
	/// 默认放行所有聊天权限（chat-core 单测聚焦消息逻辑）；测试可通过
	/// `deny_permission` 注入拒绝项，用于验证 chat-core 的发送闸门确实
	/// 以 `ChatPermission::can_send_message` 为单一事实来源（C1 权限单一化）。
	/// Defaults to allow-all; tests may inject denials via `deny_permission` to
	/// assert chat-core's send gate defers solely to chat-permission (C1).
	static DENIED_PAIRS: core::cell::RefCell<Vec<(u64, u64)>> =
		core::cell::RefCell::new(Vec::new());
}

/// 测试辅助：拒绝 `sender → receiver` 的聊天权限。
/// Test helper: deny chat permission for `sender → receiver`.
#[allow(dead_code)]
pub fn deny_permission(sender: u64, receiver: u64) {
	DENIED_PAIRS.with(|d| d.borrow_mut().push((sender, receiver)));
}

/// 测试用聊天权限端口：默认放行，命中 `DENIED_PAIRS` 则拒绝。
/// Test stub: allow all chat permissions unless the pair is in `DENIED_PAIRS`.
pub struct MockPermission;
impl pallet_chat_permission::ChatPermissionChecker<u64> for MockPermission {
	fn can_send_message(sender: &u64, receiver: &u64) -> bool {
		DENIED_PAIRS.with(|d| !d.borrow().contains(&(*sender, *receiver)))
	}
}

impl pallet_chat::Config for Test {
	type WeightInfo = pallet_chat::SubstrateWeight<Test>;
	type MaxCidLen = MaxCidLen;
	type MessageExpirationTime = MessageExpirationTime;
	type MessageRecallWindow = MessageRecallWindow;
	// ChatUserId相关配置
	type Randomness = TestRandomness;
	type UnixTime = TestTime;
	type MaxNicknameLength = frame_support::traits::ConstU32<64>;
	type MaxSignatureLength = frame_support::traits::ConstU32<256>;
	type ChatPermission = MockPermission;
	// 测试：Signed 或 Root（benchmark 用 Root，单测仍用 Signed）。
	// Tests: Signed or Root (benchmarks use Root; unit tests keep Signed).
	type SystemMessageOrigin = MockSystemMessageOrigin;
	type SystemAccount = SystemChatAccount;
}

/// 函数级详细中文注释：构建测试存储
/// 用于初始化测试环境
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// 函数级详细中文注释：运行到指定区块
/// 用于测试中推进区块高度
#[allow(dead_code)]
pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		System::set_block_number(System::block_number() + 1);
	}
}

/// 生成一个具有代表性的合法 CID（CIDv1 风格，唯一）。
/// 注意：审计 C 后链不再区分“加密/未加密”，此名称仅为历史保留，语义是“一个有效 CID”。
pub fn encrypted_cid(id: u8) -> Vec<u8> {
	let mut cid = b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_vec();
	cid.push(id); // 添加一个字节使其唯一
	cid
}

/// 标准 CIDv0（46 字节、Qm 前缀）。收敛前会被旧启发式判为“未加密”，
/// 收敛后链不再做该判断，此 CID 视为合法。
pub fn unencrypted_cid() -> Vec<u8> {
	b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG".to_vec()
}

