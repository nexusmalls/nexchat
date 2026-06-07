#![cfg_attr(not(feature = "std"), no_std)]
#![allow(deprecated)]

//! # Pallet Chat - 去中心化聊天功能
//! 
//! ## 概述
//! 
//! 本模块提供去中心化的聊天功能，采用混合方案：
//! - **链上存储**：消息元数据（发送方、接收方、IPFS CID、时间戳等）
//! - **IPFS存储**：加密的消息内容
//! - **端到端加密**：前端实现消息内容加密
//! 
//! ## 核心特性
//! 
//! - ✅ 私聊功能（1对1）
//! - ✅ 会话管理
//! - ✅ 已读/未读状态
//! - ✅ 消息软删除
//! - ✅ 未读计数
//! - ✅ 批量标记已读
//! 
//! ## 架构设计
//! 
//! ```text
//! 用户A → 加密消息 → 上传IPFS → 获取CID → 调用send_message → 链上存储元数据
//!                                                    ↓
//!                                               触发事件
//!                                                    ↓
//! 用户B ← 解密显示 ← 下载IPFS ← 获取CID ← 监听事件 ← 链上查询元数据
//! ```

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, BoundedVec, traits::{Randomness, UnixTime, EnsureOrigin}};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::traits::{Hash, Saturating};
use sp_std::convert::TryInto;

/// 聊天用户ID类型定义 - 11位数字
pub type ChatUserId = u64;

/// 受信的程序化系统通知端口（不经 extrinsic）。
/// Trusted programmatic port to emit on-chain `System` notifications WITHOUT an
/// extrinsic.
///
/// # 定位 / Purpose
/// 业务 pallet（订单 / 争议 / 悬赏等）需要在**状态机内部**向用户推送系统通知，
/// 但生产环境的 `send_message` extrinsic 被 `SystemMessageOrigin`
/// (`EnsureRootWithSuccess`) 限定为仅 Root/治理可调，普通业务流程拿不到该 origin。
/// 本 trait 提供一个**仅供 runtime 显式接线的 pallet 调用**的内部入口，把通知能力
/// 解耦给业务 pallet，而无需暴露任何用户可触达的 extrinsic。
/// Business pallets must push system notices from inside their state machines, but
/// the production `send_message` extrinsic is gated to Root/governance.
/// This trait is the internal entry callable only by runtime-wired pallets.
///
/// # 安全（审计 B2）/ Security (audit B2)
/// 这是 `impl` 上的 `pub fn`，**不是** `#[pallet::call]` extrinsic：普通用户无从触达，
/// 唯一调用方是 runtime 在 `Config` 中显式接线的业务 pallet（**编译期门控**）。因此
/// 防伪造系统通知的 B2 边界不仅被保留，反而由「运行时 Root 检查」收紧为「编译期接线
/// 白名单」。This is a `pub fn`, not an extrinsic; only runtime-wired pallets can
/// reach it (compile-time gated), which preserves and strengthens audit-B2.
pub trait SystemNotifier<AccountId> {
	/// 由平台系统账户向 `receiver` 投递一条 System 通知。
	/// Deliver a System notice from the platform system account to `receiver`.
	///
	/// # payload 语义 / Payload semantics
	/// `notice` 是**不透明、客户端本地化的通知描述符**（如模板代码 + 参数），存入
	/// `MessageMeta.content_cid`。对 System 类型而言它**不要求是真 IPFS CID**——链本就
	/// 不校验 CID 内容（审计 C），仅做非空 + 长度 sanity。
	/// `notice` is an opaque, client-localized notice descriptor stored in
	/// `content_cid`; for System it need NOT be a real IPFS CID.
	///
	/// # 会话 / Session
	/// 落入每用户唯一的「平台通知」会话（`system_account ↔ receiver`），不重建
	/// buyer↔seller 等业务关系，隐私友好；客户端用 payload 内的业务 id 深链跳转。
	/// Lands in the per-user platform-notification session (system↔receiver).
	fn notify(
		receiver: &AccountId,
		notice: sp_std::vec::Vec<u8>,
	) -> frame_support::dispatch::DispatchResult;
}

/// 用户状态枚举
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
#[codec(mel_bound())]
pub enum UserStatus {
    /// 在线
    Online,
    /// 离线
    Offline,
    /// 忙碌
    Busy,
    /// 离开
    Away,
    /// 隐身
    Invisible,
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Online
    }
}

/// 资料展示设置（纯 UI 偏好，不参与通信权限判定）。
/// Profile display settings — UI-only preferences, NOT a communication gate.
///
/// 原名 `PrivacySettings`，因与 pallet-chat-permission 的同名结构语义冲突而重命名
/// （审计 2.8：消除「两套 PrivacySettings」混淆）。通信权限的唯一来源是
/// pallet-chat-permission 的 `permission_level`；旧的 `allow_stranger_messages`
/// 门控字段已是死字段，随本次重命名一并删除（审计 2.8）。
/// Renamed from `PrivacySettings` to remove the naming clash with
/// pallet-chat-permission's identically named struct (audit 2.8). Communication
/// gating is decided solely by pallet-chat-permission's `permission_level`; the
/// dead `allow_stranger_messages` gate field is dropped together with the rename.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub struct ProfileDisplaySettings {
    /// 是否显示在线状态 / whether to show online status.
    pub show_online_status: bool,
    /// 是否显示最后活跃时间 / whether to show last-active time.
    pub show_last_active: bool,
}

impl Default for ProfileDisplaySettings {
    fn default() -> Self {
        Self {
            show_online_status: true,
            show_last_active: true,
        }
    }
}

/// 聊天用户资料结构
#[derive(Encode, Decode, Clone, DebugNoBound, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
#[codec(mel_bound())]
pub struct ChatUserProfile<T: Config> {
    /// 用户显示昵称（可选）
    pub nickname: Option<BoundedVec<u8, T::MaxNicknameLength>>,
    /// 头像IPFS CID（可选）
    pub avatar_cid: Option<BoundedVec<u8, T::MaxCidLen>>,
    /// 个性签名（可选）
    pub signature: Option<BoundedVec<u8, T::MaxSignatureLength>>,
    /// 用户状态
    pub status: UserStatus,
    /// 资料展示设置（UI 偏好；非通信权限）/ profile display settings (UI only).
    pub privacy_settings: ProfileDisplaySettings,
    /// 创建时间戳
    pub created_at: u64,
    /// 最后活跃时间戳
    pub last_active: u64,
}

/// EN: Hard upper bound on how many session messages `mark_session_as_read`
/// scans per call. Bounds the extrinsic's worst-case weight (audit B3); a client
/// repeats the call if a session has more unread than this. CN: `mark_session_as_read`
/// 单次扫描会话消息的硬上限，用于约束最坏权重（审计 B3）；会话未读多于此值时
/// 客户端可重复调用。
pub const MAX_SESSION_READ_SCAN: u32 = 512;

/// 函数级详细中文注释：权重信息 trait
/// - 定义所有可调用函数的权重计算
/// - 实际项目中应通过 benchmark 生成精确权重
/// - 这里提供保守的默认估算
pub trait WeightInfo {
	fn send_message() -> Weight;
	fn notify() -> Weight;
	fn mark_as_read() -> Weight;
	fn delete_message() -> Weight;
	fn recall_message() -> Weight;
	fn mark_batch_as_read(n: u32) -> Weight;
	fn mark_session_as_read(n: u32) -> Weight;
	fn archive_session() -> Weight;
	fn set_session_muted() -> Weight;
	fn set_session_pinned() -> Weight;
	fn cleanup_old_messages(n: u32) -> Weight;
	// 新增ChatUserId相关功能权重
	fn register_chat_user() -> Weight;
	fn update_chat_profile() -> Weight;
	fn set_user_status() -> Weight;
	fn update_privacy_settings() -> Weight;
}

/// 函数级详细中文注释：默认权重实现
/// - 基于 Substrate 标准权重单位估算
/// - DbRead = 25_000_000 weight (25微秒)
/// - DbWrite = 100_000_000 weight (100微秒)
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// 发送消息权重（保守估算，System 路径）：约 5 次读 + 4 次写
	/// - 读：Sessions, NextMessageId, SessionMessages, ChatUserId 映射等
	/// - 写：Messages, Sessions, SessionMessages, UnreadCount
	fn send_message() -> Weight {
		Weight::from_parts(
			5 * 25_000_000 + 4 * 100_000_000, // 计算权重
			0 // 存储权重（暂不考虑）
		)
	}

	/// 程序化系统通知权重：等同 `send_message`（同走 `do_send` 的 System 分支）。
	/// 业务 pallet 在其 extrinsic 权重中叠加本项以诚实计量通知成本。
	/// Programmatic notify weight: same as `send_message` (shares `do_send`).
	fn notify() -> Weight {
		Self::send_message()
	}

	/// 标记已读权重：2次读 + 2次写
	/// - 读：Messages, UnreadCount
	/// - 写：Messages, UnreadCount
	fn mark_as_read() -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + 2 * 100_000_000,
			0
		)
	}

	/// 删除消息权重：1次读 + 2次写（Messages + 可能的 UnreadCount 抵消）
	fn delete_message() -> Weight {
		Weight::from_parts(
			1 * 25_000_000 + 2 * 100_000_000,
			0
		)
	}

	/// 撤回消息权重（实测 / benchmarked）：Messages + UnreadCount（r:2 w:2）。
	fn recall_message() -> Weight {
		Weight::from_parts(50_857_000, 3710)
			.saturating_add(T::DbWeight::get().reads(2))
			.saturating_add(T::DbWeight::get().writes(2))
	}

	/// 批量标记已读权重：取决于消息数量
	/// 每条消息：1次读 + 1次写
	fn mark_batch_as_read(n: u32) -> Weight {
		Weight::from_parts(
			(n as u64) * (25_000_000 + 100_000_000),
			0
		)
	}

	/// 会话标记已读权重：取决于消息数量
	/// 基础：2次读(Sessions + SessionMessages迭代)
	/// 每条消息：1次读 + 1次写
	fn mark_session_as_read(n: u32) -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + (n as u64) * (25_000_000 + 100_000_000),
			0
		)
	}

	/// 归档会话权重：1次读 + 1次写
	fn archive_session() -> Weight {
		Weight::from_parts(
			1 * 25_000_000 + 1 * 100_000_000,
			0
		)
	}

	/// 设置会话免打扰权重（实测 / benchmarked）：Sessions(r:1) + SessionMuted(w:1)。
	fn set_session_muted() -> Weight {
		Weight::from_parts(40_832_000, 3627)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	/// 设置会话置顶权重（实测 / benchmarked）：Sessions(r:1) + SessionPinned(w:1)。
	fn set_session_pinned() -> Weight {
		Weight::from_parts(42_137_000, 3627)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(1))
	}

	/// 清理旧消息权重：取决于消息数量
	/// 每条消息：1次读 + 2次写（Messages + SessionMessages）
	fn cleanup_old_messages(n: u32) -> Weight {
		Weight::from_parts(
			(n as u64) * (25_000_000 + 2 * 100_000_000),
			0
		)
	}

	/// 注册聊天用户权重：多次读写操作
	/// - 读：AccountToChatUserId检查 + UsedChatUserIds迭代检查
	/// - 写：UsedChatUserIds + AccountToChatUserId + ChatUserIdToAccount + ChatUserProfiles
	fn register_chat_user() -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + 4 * 100_000_000,
			0
		)
	}

	/// 更新聊天资料权重：2次读 + 1次写
	/// - 读：AccountToChatUserId + ChatUserProfiles
	/// - 写：ChatUserProfiles
	fn update_chat_profile() -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + 1 * 100_000_000,
			0
		)
	}

	/// 设置用户状态权重：2次读 + 1次写
	/// - 读：AccountToChatUserId + ChatUserProfiles
	/// - 写：ChatUserProfiles
	fn set_user_status() -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + 1 * 100_000_000,
			0
		)
	}

	/// 更新隐私设置权重：2次读 + 1次写
	/// - 读：AccountToChatUserId + ChatUserProfiles
	/// - 写：ChatUserProfiles
	fn update_privacy_settings() -> Weight {
		Weight::from_parts(
			2 * 25_000_000 + 1 * 100_000_000,
			0
		)
	}
}

/// 函数级详细中文注释：消息元数据结构
/// - 链上只存储元数据，不存储实际内容
/// - 消息内容加密后存储在IPFS，链上只保存CID
/// - 同时支持AccountId和ChatUserId，提供向后兼容和隐私保护
#[derive(Encode, Decode, Clone, PartialEq, Eq, DebugNoBound, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct MessageMeta<T: Config> {
	/// 发送方账户（用于权限验证）
	pub sender: T::AccountId,
	/// 接收方账户（用于权限验证和通知）
	pub receiver: T::AccountId,
	/// 发送方聊天用户ID（用于显示和隐私）
	pub sender_chat_id: Option<ChatUserId>,
	/// 接收方聊天用户ID（用于显示和隐私）
	pub receiver_chat_id: Option<ChatUserId>,
	/// IPFS CID（加密的消息内容）
	pub content_cid: BoundedVec<u8, <T as Config>::MaxCidLen>,
	/// 会话ID（用于分组消息）
	pub session_id: T::Hash,
	/// 消息类型
	pub msg_type: MessageType,
	/// 发送时间（区块高度）
	pub sent_at: BlockNumberFor<T>,
	/// 是否已读
	pub is_read: bool,
	/// 发送方是否已删除（软删除）
	pub is_deleted_by_sender: bool,
	/// 接收方是否已删除（软删除）
	pub is_deleted_by_receiver: bool,
	/// 是否已被发送方撤回（双方隐藏，区别于单边软删除）。
	/// Whether recalled by the sender (hidden for BOTH sides; distinct from the
	/// per-side soft delete). Clients render a "message recalled" placeholder.
	pub is_recalled: bool,
}

/// 函数级详细中文注释：会话信息结构
#[derive(Encode, Decode, Clone, PartialEq, Eq, DebugNoBound, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Session<T: Config> {
	/// 会话ID
	pub id: T::Hash,
	/// 参与者列表（最多2人，私聊）
	pub participants: BoundedVec<T::AccountId, ConstU32<2>>,
	/// 最后一条消息ID
	pub last_message_id: u64,
	/// 最后活跃时间
	pub last_active: BlockNumberFor<T>,
	/// 创建时间
	pub created_at: BlockNumberFor<T>,
	/// 是否归档
	pub is_archived: bool,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use sp_std::vec::Vec;
	use sp_std::vec;

	/// 消息类型枚举 / Message type.
	///
	/// EN: Only [`MessageType::System`] is ever stored on-chain. The human
	/// variants (`Text`/`Image`/`File`/`Voice`) are **legacy / off-chain only**:
	/// after the MLS convergence human messages move off-chain (MLS + relay) and
	/// `send_message` rejects them with [`Error::HumanMessagesOffChain`]. The
	/// variants are kept solely to preserve SCALE indices and historical decoding.
	/// CN: 链上**只会**存储 [`MessageType::System`]。人类类型
	/// （`Text`/`Image`/`File`/`Voice`）属**遗留 / 仅链下**：MLS 收敛后人类消息走链下
	/// （MLS + relay），`send_message` 会以 [`Error::HumanMessagesOffChain`] 拒绝；
	/// 这些变体仅为保持 SCALE 索引与历史解码而保留。
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
	pub enum MessageType {
		/// 文本消息（遗留 / 仅链下）/ Text (legacy / off-chain only)
		Text,
		/// 图片消息（遗留 / 仅链下）/ Image (legacy / off-chain only)
		Image,
		/// 文件消息（遗留 / 仅链下）/ File (legacy / off-chain only)
		File,
		/// 语音消息（遗留 / 仅链下）/ Voice (legacy / off-chain only)
		Voice,
		/// 系统消息（链上唯一类型，如订单状态变更）/ System — the ONLY on-chain type
		System,
	}

	impl Default for MessageType {
		fn default() -> Self {
			Self::Text
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {

		/// 权重信息
		type WeightInfo: WeightInfo;

		/// IPFS CID最大长度（通常为46-59字节）
		#[pallet::constant]
		type MaxCidLen: Get<u32>;

		// 已移除死配置 MaxSessionsPerUser / MaxMessagesPerSession（C2 职责收窄，审计 L）：
		// 两者从未参与任何逻辑，仅为历史占位。会话/消息上限由业务与权重自然约束。
		// Removed dead config MaxSessionsPerUser / MaxMessagesPerSession (audit L):
		// they were never used by any logic.

		// 已移除死配置 RateLimitWindow / MaxMessagesPerWindow（审计：chat-core 历史层）：
		// 仅服务于已不可达的人类消息限频路径，System 本就跳过限频。
		// Removed dead config RateLimitWindow / MaxMessagesPerWindow (chat-core
		// historical layer): they only fed the now-unreachable human-message rate
		// limit; System always bypassed it.

		/// 消息过期时间（区块数）
		/// 例如：2_592_000个区块 ≈ 180天（假设6秒一个块）
		/// 过期后可被清理
	#[pallet::constant]
	type MessageExpirationTime: Get<BlockNumberFor<Self>>;

		/// 消息撤回时间窗口（区块数）：发送方仅可在发出后该窗口内撤回。
		/// Message recall window (in blocks): a sender may recall a message only
		/// within this many blocks after it was sent (e.g. ~2 minutes).
		#[pallet::constant]
		type MessageRecallWindow: Get<BlockNumberFor<Self>>;

		/// ChatUserId相关配置
		/// 随机数源，用于生成ChatUserId
		type Randomness: Randomness<Self::Hash, BlockNumberFor<Self>>;

		/// 时间提供器，用于时间戳
		type UnixTime: UnixTime;

		/// 用户昵称最大长度
		#[pallet::constant]
		type MaxNicknameLength: Get<u32>;

		/// 用户个性签名最大长度
		#[pallet::constant]
		type MaxSignatureLength: Get<u32>;

		/// 聊天权限检查端口：在 `send_message` 前校验是否允许通信。
		/// Chat permission port: gate `send_message` via `can_send_message`.
		/// 由 `pallet-chat-permission` 提供（场景授权 / 好友 / 黑白名单 / 隐私级别）。
		type ChatPermission: pallet_chat_permission::ChatPermissionChecker<Self::AccountId>;

		/// 允许发送链上 `System` 消息的特权来源，解析为消息 `sender` 账户。
		/// Privileged origin allowed to send on-chain `System` messages, resolving
		/// to the [`AccountId`](frame_system::Config::AccountId) recorded as the
		/// message `sender`.
		///
		/// # 安全（审计 B2）/ Security (audit B2)
		/// 旧版 `send_message` 用 `ensure_signed`，任何账户
		/// 都能发出 `MessageType::System` 消息，从而伪造「看似平台」的系统通知。改用
		/// 本特权来源后，System 通道仅对受信来源（如治理 / 特定系统账户）开放；普通
		/// 人类聊天本就走链下 MLS，不经此入口。The former entries used `ensure_signed`,
		/// letting any account emit `System` messages and spoof platform-looking
		/// notifications. Gating the channel behind this origin restricts it to a
		/// trusted source (e.g. governance / a system account); human chat is
		/// off-chain and never uses this entry.
		type SystemMessageOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Self::AccountId>;

		/// 程序化系统通知（`SystemNotifier::notify`）所记录的 `sender` 账户。
		/// Account recorded as `sender` for programmatic System notifications.
		///
		/// 生产环境应配置为与 `SystemMessageOrigin` 的 success 账户**同一个**派生系统
		/// 账户（如 PalletId 派生的 `ChatSystemMessenger`），使 extrinsic 路径与 trait
		/// 路径落同一平台发信账户，避免「系统消息来源」分裂。Should be the SAME
		/// derived system account as `SystemMessageOrigin`'s success value so both
		/// the extrinsic and trait paths stamp one platform sender.
		type SystemAccount: Get<Self::AccountId>;
	}

	/// 函数级详细中文注释：消息元数据存储
	/// - Key: 消息ID
	/// - Value: 消息元数据
	#[pallet::storage]
	#[pallet::getter(fn messages)]
	pub type Messages<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		u64,
		MessageMeta<T>,
	>;

	/// 函数级详细中文注释：下一个消息ID
	#[pallet::storage]
	#[pallet::getter(fn next_message_id)]
	pub type NextMessageId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// 过期消息清理游标：上次扫描到的消息 ID（用于有界增量 GC）。
	/// Cleanup cursor: last message id scanned, enabling bounded incremental GC.
	///
	/// `cleanup_old_messages` 每次最多扫描 `limit` 条消息（受治理/Root 调用），
	/// 从游标处用 `iter_from` 续扫；扫到表尾则复位为 `None`。这样把单次工作量
	/// 限定为 O(limit)（权重据实计量），避免旧实现“近似全表扫描却按 limit 收费”
	/// 的权重低估与 DoS（审计 G）。
	/// Each call scans at most `limit` entries resuming from the cursor via
	/// `iter_from`, bounding work to O(limit) and fixing the audit-G weight
	/// under-charge / DoS of the former near-full-table scan.
	#[pallet::storage]
	pub type LastCleanupCursor<T: Config> = StorageValue<_, u64, OptionQuery>;

	/// 函数级详细中文注释：会话存储
	/// - Key: 会话ID
	/// - Value: 会话信息
	#[pallet::storage]
	#[pallet::getter(fn sessions)]
	pub type Sessions<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::Hash,
		Session<T>,
	>;

	/// 函数级详细中文注释：用户会话索引
	/// - Key1: 账户地址
	/// - Key2: 会话ID
	/// - Value: () 标记（只用于索引）
	/// - 改用DoubleMap，支持无限会话
	#[pallet::storage]
	pub type UserSessions<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::Hash,
		(),
		OptionQuery,
	>;

	/// 函数级详细中文注释：会话消息索引
	/// - Key1: 会话ID
	/// - Key2: 消息ID
	/// - Value: () 标记（只用于索引）
	/// - 改用DoubleMap，支持无限消息存储
	#[pallet::storage]
	pub type SessionMessages<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::Hash,              // session_id
		Blake2_128Concat,
		u64,                  // message_id
		(),
		OptionQuery,
	>;

	/// 函数级详细中文注释：未读消息计数
	/// - Key: (接收方, 会话ID)
	/// - Value: 未读数量
	#[pallet::storage]
	#[pallet::getter(fn unread_count)]
	pub type UnreadCount<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		(T::AccountId, T::Hash),
		u32,
		ValueQuery,
	>;

	/// 会话级免打扰（每用户、每会话）。存在键即「已静音」。
	/// Per-user, per-session do-not-disturb (mute). Key presence = muted.
	///
	/// 这是**通知偏好**：链不投递推送，故由客户端读取此状态决定是否提示；
	/// 与群禁言不同，免打扰只影响调用者自己的提醒，不限制对方发送。
	/// A notification preference read by clients; unlike group mute it only
	/// silences the caller's own alerts and never blocks the counterparty.
	#[pallet::storage]
	pub type SessionMuted<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::Hash,
		(),
		OptionQuery,
	>;

	/// 会话级置顶（每用户、每会话）。值为置顶时间（区块号），用于置顶区排序。
	/// Per-user, per-session pin. Value = pinned-at block, used to order the
	/// pinned section (most-recently-pinned first).
	#[pallet::storage]
	pub type SessionPinned<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::Hash,
		BlockNumberFor<T>,
		OptionQuery,
	>;

	// 黑名单已收敛到 pallet-chat-permission（单一权限源）。
	// Blacklist has been consolidated into pallet-chat-permission (single source
	// of truth); see audit finding I and the chat-core × MLS convergence design.
	// chat-core 不再自存黑名单，发送闸门统一走 `ChatPermission::can_send_message`。

	// 已移除 `MessageRateLimit` 存储与配套限频（审计：chat-core 历史层）。链下收敛后
	// `send_message` 仅接受 `System`、人类消息全走链下，限频路径对外不可达；System 本就
	// 受信跳过限频，故整套频率限制（storage + `check_rate_limit` + `RateLimitWindow` /
	// `MaxMessagesPerWindow`）已删，零链上行为变更。
	// Removed `MessageRateLimit` storage and its rate-limit machinery (chat-core
	// historical layer): after off-chain convergence `send_message` accepts only
	// `System`, the limited path is unreachable, and System always bypassed it —
	// so the whole rate limit was dead code (no on-chain behavior change).

	/// 函数级详细中文注释：已使用的聊天用户ID
	/// - Key: ChatUserId
	/// - Value: bool（标记是否已使用）
	/// - 用于防止ID重复
	#[pallet::storage]
	pub type UsedChatUserIds<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ChatUserId,
		bool,
		OptionQuery,
	>;

	/// 全局聊天用户 ID 生成计数器（单调递增的种子 nonce）。
	/// Monotonic nonce used as a seed input when generating `ChatUserId`s.
	///
	/// 取代旧版在每次生成时执行的 `UsedChatUserIds::iter().count()` 全表扫描
	/// （O(n) 且未计入权重），改为 O(1) 读写，保证逐次调用的随机种子互异。
	/// Replaces the former O(n) full-storage scan with an O(1) counter so that
	/// each generation uses a distinct seed without iterating all used IDs.
	#[pallet::storage]
	#[pallet::getter(fn next_chat_user_id)]
	pub type NextChatUserId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// 函数级详细中文注释：账户到聊天用户ID的映射
	/// - Key: 账户地址
	/// - Value: ChatUserId
	/// - 每个账户只能有一个ChatUserId
	#[pallet::storage]
	#[pallet::getter(fn account_to_chat_user_id)]
	pub type AccountToChatUserId<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		ChatUserId,
		OptionQuery,
	>;

	/// 函数级详细中文注释：聊天用户ID到账户地址的反向映射
	/// - Key: ChatUserId
	/// - Value: 账户地址
	/// - 用于快速反向查找
	#[pallet::storage]
	#[pallet::getter(fn chat_user_id_to_account)]
	pub type ChatUserIdToAccount<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ChatUserId,
		T::AccountId,
		OptionQuery,
	>;

	/// 函数级详细中文注释：聊天用户资料
	/// - Key: ChatUserId
	/// - Value: 用户资料信息
	/// - 包含昵称、头像、状态等信息
	#[pallet::storage]
	pub type ChatUserProfiles<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		ChatUserId,
		ChatUserProfile<T>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// 消息已发送（链上仅 System）。单一发送事件，已合并旧 `MessageSentWithChatId`。
		/// Message sent (on-chain: System only). Single event; the former
		/// `MessageSentWithChatId` was merged in (audit 2.4: removed double event).
		/// [msg_id, session_id, sender, receiver, sender_chat_id, receiver_chat_id]
		MessageSent {
			msg_id: u64,
			session_id: T::Hash,
			sender: T::AccountId,
			receiver: T::AccountId,
			/// 发送方 ChatUserId（如已注册）/ sender's ChatUserId if registered.
			sender_chat_id: Option<ChatUserId>,
			/// 接收方 ChatUserId（如已注册）/ receiver's ChatUserId if registered.
			receiver_chat_id: Option<ChatUserId>,
		},

		/// 函数级详细中文注释：消息已读
		/// [msg_id, reader]
		MessageRead {
			msg_id: u64,
			reader: T::AccountId,
		},

		/// 函数级详细中文注释：消息已删除
		/// [msg_id, deleter]
		MessageDeleted {
			msg_id: u64,
			deleter: T::AccountId,
		},

		/// 消息已被发送方撤回（双方隐藏）。
		/// Message recalled by its sender (hidden for both parties).
		/// [msg_id, session_id]
		MessageRecalled {
			msg_id: u64,
			session_id: T::Hash,
		},

		/// 函数级详细中文注释：会话已创建
		/// [session_id, participants]
		SessionCreated {
			session_id: T::Hash,
			participants: BoundedVec<T::AccountId, ConstU32<2>>,
		},

		/// 函数级详细中文注释：会话已标记为已读
		/// [session_id, user]
		SessionMarkedAsRead {
			session_id: T::Hash,
			user: T::AccountId,
		},

		/// 函数级详细中文注释：会话已归档
		/// [session_id, operator]
		SessionArchived {
			session_id: T::Hash,
			operator: T::AccountId,
		},

		/// 会话免打扰开关已设置（每用户）。
		/// Per-user session mute (do-not-disturb) toggled.
		/// [session_id, user, muted]
		SessionMuteSet {
			session_id: T::Hash,
			user: T::AccountId,
			muted: bool,
		},

		/// 会话置顶开关已设置（每用户）。
		/// Per-user session pin toggled.
		/// [session_id, user, pinned]
		SessionPinSet {
			session_id: T::Hash,
			user: T::AccountId,
			pinned: bool,
		},

		// 拉黑事件已不在链上（审计 P1）：链上黑名单整体移除，拉黑改由链下能力令牌
		// 撤销（chat-permission `CapabilityEpochBumped`）/ 信箱标签撤销表达。
		// Block events are no longer on-chain (audit P1): the on-chain blocklist
		// was removed; blocking is expressed off-chain via capability-token
		// revocation (chat-permission `CapabilityEpochBumped`) / inbox tag revocation.

		/// 旧消息已清理（治理/Root 触发，无操作者账户）。
		/// Old messages cleaned up (governance/Root-triggered; no operator account).
		OldMessagesCleanedUp {
			count: u32,
		},

		/// 函数级详细中文注释：聊天用户创建成功
		/// [account_id, chat_user_id]
		ChatUserCreated {
			account_id: T::AccountId,
			chat_user_id: ChatUserId,
		},

		/// 函数级详细中文注释：聊天用户资料更新
		/// [chat_user_id]
		ChatUserProfileUpdated {
			chat_user_id: ChatUserId,
		},

		/// 函数级详细中文注释：用户状态变更
		/// [chat_user_id, new_status_code]
		ChatUserStatusChanged {
			chat_user_id: ChatUserId,
			new_status: u8,
		},

		/// 函数级详细中文注释：隐私设置更新
		/// [chat_user_id]
		PrivacySettingsUpdated {
			chat_user_id: ChatUserId,
		},

		// 已移除 `MessageSentWithChatId`（审计 2.4）：与 `MessageSent` 重复发送。
		// 其 ChatUserId 字段已并入 `MessageSent`；`content_cid` 不再入事件（已在
		// `Messages` 存储，避免事件冗余）。
		// Removed `MessageSentWithChatId` (audit 2.4): it duplicated `MessageSent`.
		// Its ChatUserId fields are folded into `MessageSent`; `content_cid` is no
		// longer emitted (it lives in `Messages` storage).
	}

	#[pallet::error]
	pub enum Error<T> {
		/// CID 太长，超过了最大长度限制
		CidTooLong,
		/// 消息未找到，请检查消息ID是否正确
		MessageNotFound,
		/// 会话未找到，请检查会话ID是否正确
		SessionNotFound,
		/// 不是接收方，只有消息接收方才能执行此操作
		NotReceiver,
		/// 未授权，您没有权限执行此操作
		NotAuthorized,
		/// 不是会话参与者，只有会话参与者才能执行此操作
		NotSessionParticipant,
		/// 参与者太多，会话只支持2个参与者
		TooManyParticipants,
		/// CID 非法（当前仅校验：非空）。仅格式 sanity，**不**代表链校验过加密。
		/// Invalid CID (currently: empty). Format sanity only; the chain does NOT
		/// verify encryption — that is guaranteed by client-side MLS E2EE (audit C).
		InvalidCid,
		/// 消息ID列表为空
		EmptyMessageList,
		/// 分页参数无效，offset或limit超出合理范围
		InvalidPagination,
		/// 无聊天权限：被对方拉黑、缺少有效场景授权 / 好友关系，或被隐私级别拒绝。
		/// 统一由 pallet-chat-permission 判定（黑名单 / 陌生人 / 隐私级别单一来源）。
		/// No chat permission: blocked, missing scene authorization / friendship, or
		/// denied by privacy level. Adjudicated solely by pallet-chat-permission
		/// (single source for blacklist / stranger / privacy-level checks).
		ChatNotAuthorized,
		/// 清理数量参数无效（必须大于0且小于等于1000）
		InvalidCleanupLimit,

		/// 聊天用户ID生成失败
		ChatUserIdGenerationFailed,

		/// 聊天用户已存在
		ChatUserAlreadyExists,

		/// 聊天用户不存在
		ChatUserNotFound,

		/// 昵称过长
		NicknameTooLong,

		/// 个性签名过长
		SignatureTooLong,

		/// 无效的用户状态
		InvalidUserStatus,

		/// 只有发送方才能撤回消息
		/// Only the sender may recall a message
		NotSender,

		/// 消息已撤回，无需重复操作
		/// Message already recalled
		AlreadyRecalled,

		/// 已超过撤回时间窗口
		/// The recall time window has elapsed
		RecallWindowExpired,

		/// EN: Human chat messages (Text/Image/File/Voice) are no longer accepted
		/// on-chain. They move off-chain via MLS + relay so that *who talks to whom*
		/// and message content never touch the chain (privacy: hide communication
		/// relationship). Only [`MessageType::System`] is allowed on-chain via
		/// [`Pallet::send_message`]. CN: 人类聊天消息（Text/Image/File/Voice）不再上链，
		/// 改走链下 MLS + relay，使「谁与谁聊、聊什么」不触链（隐私：隐藏通信关系）。
		/// 链上仅允许经 [`Pallet::send_message`] 发送 [`MessageType::System`]。
		HumanMessagesOffChain,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// 发送链上 System 消息（**唯一**的 System 发送入口）。
		/// Send an on-chain System message — the SINGLE canonical System entry.
		///
		/// # 收窄说明 / Narrowing (C-plan finalized)
		/// 按《chat-core × MLS 收敛》路线，**人类聊天消息（Text/Image/File/Voice）
		/// 已迁出链上**，改走链下 MLS + relay，使「谁与谁聊、聊什么」不触链
		/// （隐私：隐藏通信关系）。本入口现仅接受 `System`（`msg_type_code == 4`）；
		/// 传入人类消息类型一律返回 [`Error::HumanMessagesOffChain`]。
		/// Human chat messages now live off-chain (MLS + relay); this entry accepts
		/// `System` only and rejects human types with [`Error::HumanMessagesOffChain`].
		///
		/// # 合并说明（审计 2.1/2.3）/ Consolidation (audit 2.1/2.3)
		/// 旧的重复入口 `send_system_message`（原 call_index 16）已删除——它与本函数
		/// 行为完全一致（同走 `SystemMessageOrigin` + `do_send(System)`）。索引 16 留空
		/// 不复用。程序化通知请用 [`crate::SystemNotifier::notify`]。
		/// The duplicate `send_system_message` (former call_index 16) was removed —
		/// it was behaviorally identical. Index 16 is left vacant; for programmatic
		/// notifications use [`crate::SystemNotifier::notify`].
		///
		/// # 参数 / Params
		/// - `receiver`: 接收方地址
		/// - `content_cid`: IPFS CID（加密的消息内容）
		/// - `msg_type_code`: 消息类型代码（保留参数，仅 `4=System` 被接受）
		/// - `session_id`: 会话ID（可选，如果为None则自动创建新会话）
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::send_message())]
		pub fn send_message(
			origin: OriginFor<T>,
			receiver: T::AccountId,
			content_cid: Vec<u8>,
			msg_type_code: u8,
			session_id: Option<T::Hash>,
		) -> DispatchResult {
			// 仅特权来源可发 System 消息（审计 B2：防伪造系统通知）。
			// Only the privileged origin may emit System messages (audit B2).
			let sender = T::SystemMessageOrigin::ensure_origin(origin)?;

			// 仅 System 可上链；人类消息（Text/Image/File/Voice）改走链下 MLS + relay。
			// Only System may be stored on-chain; human messages move off-chain.
			ensure!(msg_type_code == 4, Error::<T>::HumanMessagesOffChain);

			Self::do_send(sender, receiver, content_cid, MessageType::System, session_id)
		}

		// 已移除重复入口 `send_system_message`（原 call_index 16，审计 2.1/2.3）：与
		// `send_message` 行为一致（`SystemMessageOrigin` + `do_send(System)`）。索引 16
		// 留空不复用；程序化通知走 `SystemNotifier::notify`。
		// Removed duplicate `send_system_message` (former call_index 16, audit 2.1/2.3):
		// behaviorally identical to `send_message`. Index 16 left vacant; programmatic
		// notifications use `SystemNotifier::notify`.

		/// 函数级详细中文注释：标记消息已读
		/// 
		/// # 参数
		/// - `msg_id`: 消息ID
		/// 
		/// # 流程
		/// 1. 验证消息存在
		/// 2. 验证调用者是接收方
		/// 3. 标记已读
		/// 4. 减少未读计数
		/// 5. 触发事件
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::mark_as_read())]
		pub fn mark_as_read(
			origin: OriginFor<T>,
			msg_id: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Messages::<T>::try_mutate(msg_id, |maybe_msg| -> DispatchResult {
				let msg = maybe_msg.as_mut().ok_or(Error::<T>::MessageNotFound)?;

				// 验证是接收方
				ensure!(msg.receiver == who, Error::<T>::NotReceiver);

				// 如果已经是已读，直接返回
				if msg.is_read {
					return Ok(());
				}

				// 标记已读
				msg.is_read = true;

				// 减少未读计数
				UnreadCount::<T>::mutate((who.clone(), msg.session_id), |count| {
					*count = count.saturating_sub(1);
				});

				Ok(())
			})?;

			Self::deposit_event(Event::MessageRead { msg_id, reader: who });

			Ok(())
		}

		/// 函数级详细中文注释：删除消息（软删除）
		/// 
		/// # 参数
		/// - `msg_id`: 消息ID
		/// 
		/// # 流程
		/// 1. 验证消息存在
		/// 2. 验证调用者是发送方或接收方
		/// 3. 分别标记删除（发送方删除不影响接收方，反之亦然）
		/// 4. 触发事件
		/// 
		/// # 说明
		/// - 发送方删除：只对发送方隐藏，接收方仍可见
		/// - 接收方删除：只对接收方隐藏，发送方仍可见
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::delete_message())]
		pub fn delete_message(
			origin: OriginFor<T>,
			msg_id: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Messages::<T>::try_mutate(msg_id, |maybe_msg| -> DispatchResult {
				let msg = maybe_msg.as_mut().ok_or(Error::<T>::MessageNotFound)?;

				// 验证是发送方或接收方
				ensure!(
					msg.sender == who || msg.receiver == who,
					Error::<T>::NotAuthorized
				);

				// 分别标记删除
				if msg.sender == who {
					msg.is_deleted_by_sender = true;
				} else {
					// 接收方删除一条「仍计未读」的消息时，需同步抵消未读计数，避免
					// 角标永久 +1（删除后该消息对接收方隐藏，无法再单独标记已读）。
					// 仅在该消息当前确实计入未读时抵消：未读 + 未撤回 + 接收方此前未删除。
					// 幂等：重复删除不会重复抵消。
					// When the receiver deletes a message that still counts as unread,
					// offset the unread count so the badge cannot get stuck (a deleted
					// message is hidden and can no longer be individually marked read).
					// Offset only once and only while it actually counts as unread:
					// not read, not recalled, not already deleted by the receiver.
					if !msg.is_read && !msg.is_recalled && !msg.is_deleted_by_receiver {
						UnreadCount::<T>::mutate((who.clone(), msg.session_id), |count| {
							*count = count.saturating_sub(1);
						});
					}
					msg.is_deleted_by_receiver = true;
				}

				Ok(())
			})?;

			Self::deposit_event(Event::MessageDeleted { msg_id, deleter: who });

			Ok(())
		}

		/// 撤回消息（真正的双方撤回，带时限）。
		/// Recall a message (true two-sided recall, time-limited).
		///
		/// # 与 `delete_message` 的区别 / vs `delete_message`
		/// `delete_message` 是**单边软删除**（只对操作者隐藏）；`recall_message`
		/// 由**发送方**在 `MessageRecallWindow` 区块窗口内调用，将消息标记为已撤回，
		/// **对收发双方都隐藏**（客户端展示「消息已撤回」占位）。链上仅翻转标记，
		/// 不删除元数据（保留审计可见性）。
		/// `delete_message` hides per-side; `recall_message` lets the SENDER, within
		/// `MessageRecallWindow` blocks, flip a flag that hides the message for BOTH
		/// parties (clients show a "recalled" placeholder). Metadata is kept.
		///
		/// # 参数 / Params
		/// - `msg_id`: 消息ID
		#[pallet::call_index(17)]
		#[pallet::weight(T::WeightInfo::recall_message())]
		pub fn recall_message(
			origin: OriginFor<T>,
			msg_id: u64,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let (session_id, was_unread, receiver) =
				Messages::<T>::try_mutate(msg_id, |maybe_msg| -> Result<(T::Hash, bool, T::AccountId), DispatchError> {
					let msg = maybe_msg.as_mut().ok_or(Error::<T>::MessageNotFound)?;

					// 仅发送方可撤回 / only the sender may recall
					ensure!(msg.sender == who, Error::<T>::NotSender);
					// 幂等保护 / idempotency guard
					ensure!(!msg.is_recalled, Error::<T>::AlreadyRecalled);

					// 时限校验：now - sent_at <= window / within the recall window
					let now = <frame_system::Pallet<T>>::block_number();
					let elapsed = now.saturating_sub(msg.sent_at);
					ensure!(elapsed <= T::MessageRecallWindow::get(), Error::<T>::RecallWindowExpired);

					msg.is_recalled = true;
					Ok((msg.session_id, !msg.is_read, msg.receiver.clone()))
				})?;

			// 撤回未读消息时同步抵消未读计数 / offset unread count if it was unread
			if was_unread {
				UnreadCount::<T>::mutate((receiver, session_id), |count| {
					*count = count.saturating_sub(1);
				});
			}

			Self::deposit_event(Event::MessageRecalled { msg_id, session_id });

			Ok(())
		}

		/// 函数级详细中文注释：批量标记已读（指定消息列表）
		/// 
		/// # 参数
		/// - `message_ids`: 消息ID列表
		/// 
		/// # 流程
		/// 1. 验证消息列表非空
		/// 2. 批量标记已读
		/// 3. 更新未读计数
		/// 4. 触发事件
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::mark_batch_as_read(message_ids.len() as u32))]
		pub fn mark_batch_as_read(
			origin: OriginFor<T>,
			message_ids: Vec<u64>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 验证列表非空
			ensure!(!message_ids.is_empty(), Error::<T>::EmptyMessageList);

			let mut marked_count = 0u32;

			// 批量标记已读
			for msg_id in message_ids.iter() {
				if let Some(mut msg) = Messages::<T>::get(msg_id) {
					// 验证是接收方
					if msg.receiver == who && !msg.is_read {
						msg.is_read = true;
						Messages::<T>::insert(msg_id, msg.clone());
						marked_count = marked_count.saturating_add(1);

						// 减少未读计数
						UnreadCount::<T>::mutate((who.clone(), msg.session_id), |count| {
							*count = count.saturating_sub(1);
						});

						// 触发事件
						Self::deposit_event(Event::MessageRead {
							msg_id: *msg_id,
							reader: who.clone(),
						});
					}
				}
			}

			Ok(())
		}

		/// 函数级详细中文注释：批量标记已读（按会话）
		/// 
		/// # 参数
		/// - `session_id`: 会话ID
		/// 
		/// # 流程
		/// 1. 验证会话存在且用户是参与者
		/// 2. 获取会话的所有消息
		/// 3. 批量标记已读
		/// 4. 清空未读计数
		/// 5. 触发事件
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::mark_session_as_read(crate::MAX_SESSION_READ_SCAN))]
		pub fn mark_session_as_read(
			origin: OriginFor<T>,
			session_id: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 验证会话存在且用户是参与者
			let session = Sessions::<T>::get(session_id)
				.ok_or(Error::<T>::SessionNotFound)?;
			ensure!(
				session.participants.contains(&who),
				Error::<T>::NotSessionParticipant
			);

			// B3（DoS）：会话消息条数无上界，必须有界扫描，否则固定权重会被低估。
			// 单次最多扫描 `MAX_SESSION_READ_SCAN` 条；若仍有剩余，客户端可重复调用直至
			// 未读归零（按本次实际标记数递减，分批安全、不会误清零）。
			// B3 (DoS): a session's message count is unbounded, so the scan must be
			// bounded or the fixed weight would be under-charged. Scan at most
			// `MAX_SESSION_READ_SCAN` per call; the client may repeat until unread
			// reaches zero (we decrement by the count actually marked this call,
			// which is batch-safe and never over-zeros).
			let max_scan = crate::MAX_SESSION_READ_SCAN as usize;
			let message_ids: Vec<u64> = SessionMessages::<T>::iter_prefix(session_id)
				.map(|(msg_id, _)| msg_id)
				.take(max_scan)
				.collect();

			// 批量标记已读，并记录本次实际标记数（仅本人未读的会被标记）。
			let mut marked: u32 = 0;
			for msg_id in message_ids.iter() {
				if let Some(mut msg) = Messages::<T>::get(msg_id) {
					if msg.receiver == who && !msg.is_read {
						msg.is_read = true;
						Messages::<T>::insert(msg_id, msg);
						marked = marked.saturating_add(1);
					}
				}
			}

			// 按本次实际标记数抵消未读计数（分批收敛到 0）。
			UnreadCount::<T>::mutate((who.clone(), session_id), |count| {
				*count = count.saturating_sub(marked);
			});

			Self::deposit_event(Event::SessionMarkedAsRead {
				session_id,
				user: who,
			});

			Ok(())
		}

		/// 函数级详细中文注释：归档会话
		/// 
		/// # 参数
		/// - `session_id`: 会话ID
		/// 
		/// # 流程
		/// 1. 验证会话存在
		/// 2. 验证用户是参与者
		/// 3. 标记会话为归档状态
		/// 4. 触发事件
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::archive_session())]
		pub fn archive_session(
			origin: OriginFor<T>,
			session_id: T::Hash,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 验证会话存在并更新归档状态
			Sessions::<T>::try_mutate(session_id, |maybe_session| -> DispatchResult {
				let session = maybe_session.as_mut().ok_or(Error::<T>::SessionNotFound)?;
				
				// 验证是参与者
				ensure!(
					session.participants.contains(&who),
					Error::<T>::NotSessionParticipant
				);

				// 标记为归档
				session.is_archived = true;

				Ok(())
			})?;

			Self::deposit_event(Event::SessionArchived {
				session_id,
				operator: who,
			});

			Ok(())
		}

		/// 设置会话免打扰（每用户的通知偏好）。
		/// Set per-user session mute (do-not-disturb notification preference).
		///
		/// # 说明 / Notes
		/// 仅影响调用者自己的提醒，由客户端读取此状态执行；不影响消息发送/接收。
		/// 调用者必须是会话参与者。Affects only the caller's own alerts (client-read);
		/// does not gate messaging. Caller must be a session participant.
		#[pallet::call_index(18)]
		#[pallet::weight(T::WeightInfo::set_session_muted())]
		pub fn set_session_muted(
			origin: OriginFor<T>,
			session_id: T::Hash,
			muted: bool,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::ensure_session_participant(&who, session_id)?;

			if muted {
				SessionMuted::<T>::insert(&who, session_id, ());
			} else {
				SessionMuted::<T>::remove(&who, session_id);
			}

			Self::deposit_event(Event::SessionMuteSet { session_id, user: who, muted });
			Ok(())
		}

		/// 设置会话置顶（每用户）。置顶会话在 `list_sessions` 中排在最前。
		/// Set per-user session pin; pinned sessions sort first in `list_sessions`.
		///
		/// 调用者必须是会话参与者。 / Caller must be a session participant.
		#[pallet::call_index(19)]
		#[pallet::weight(T::WeightInfo::set_session_pinned())]
		pub fn set_session_pinned(
			origin: OriginFor<T>,
			session_id: T::Hash,
			pinned: bool,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::ensure_session_participant(&who, session_id)?;

			if pinned {
				let now = <frame_system::Pallet<T>>::block_number();
				SessionPinned::<T>::insert(&who, session_id, now);
			} else {
				SessionPinned::<T>::remove(&who, session_id);
			}

			Self::deposit_event(Event::SessionPinSet { session_id, user: who, pinned });
			Ok(())
		}

		// 拉黑能力已彻底移出链上明文存储（审计 P1）：chat-core 的 call_index 6/7 仍保留
		// 空位以维持其余调用索引稳定；拉黑改由链下能力令牌撤销
		// （`pallet_chat_permission::bump_capability_epoch`）或定向信箱标签撤销
		// （`pallet_chat_inbox::revoke_tag`）实现，链上不再存任何黑名单。
		// Blocking no longer has any on-chain plaintext storage (audit P1):
		// chat-core call indices 6/7 stay vacant to keep other indices stable;
		// blocking is done off-chain via capability-token revocation
		// (`pallet_chat_permission::bump_capability_epoch`) or per-contact inbox
		// tag revocation (`pallet_chat_inbox::revoke_tag`).

		/// 清理过期消息（治理 / Root 限定，有界增量 GC）。
		/// Clean up expired messages — governance/Root only, bounded incremental GC.
		///
		/// # 参数 / Params
		/// - `limit`: 本次最多**扫描**的消息条数（1-1000，即扫描预算，非删除上限）
		///
		/// # 安全 / 权重（审计 G）
		/// - **仅 Root/治理可调**：旧版 `ensure_signed` 允许任何人触发，存在 DoS；
		///   现改为 `ensure_root`，移除该滥用面。
		/// - **有界扫描 + 游标**：每次至多扫描 `limit` 条（从 `LastCleanupCursor`
		///   续扫），把单次工作量限定为 O(limit)，权重据实计量；扫到表尾复位游标。
		///   旧版“扫描近全表却按 limit 收费”的权重低估被修正。
		///
		/// Root/governance-only; scans at most `limit` entries per call resuming
		/// from `LastCleanupCursor`, bounding work to O(limit) so the weight is
		/// honest (fixes audit G). Removes only messages that are expired AND
		/// soft-deleted by both parties.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::cleanup_old_messages(*limit))]
		pub fn cleanup_old_messages(
			origin: OriginFor<T>,
			limit: u32,
		) -> DispatchResult {
			// 治理 / Root 限定（修复审计 G 的“任何人可调”）。
			ensure_root(origin)?;

			// 验证 limit 参数（1-1000，作为扫描预算）。
			ensure!(limit > 0 && limit <= 1000, Error::<T>::InvalidCleanupLimit);

			let now = <frame_system::Pallet<T>>::block_number();
			let expiration_time = T::MessageExpirationTime::get();

			// 从游标处续扫；游标为空则从头开始（有界增量 GC）。
			let mut iter = match LastCleanupCursor::<T>::get() {
				Some(last_id) => Messages::<T>::iter_from(Messages::<T>::hashed_key_for(last_id)),
				None => Messages::<T>::iter(),
			};

			let mut scanned = 0u32;
			let mut cleaned_count = 0u32;
			let mut last_seen: Option<u64> = None;
			let mut messages_to_remove: Vec<(u64, T::Hash)> = Vec::new();

			// 扫描预算限定为 `limit` 条，单次工作量 O(limit)。
			while scanned < limit {
				match iter.next() {
					Some((msg_id, msg)) => {
						scanned = scanned.saturating_add(1);
						last_seen = Some(msg_id);

						let age = now.saturating_sub(msg.sent_at);
						if age >= expiration_time
							&& msg.is_deleted_by_sender
							&& msg.is_deleted_by_receiver
						{
							messages_to_remove.push((msg_id, msg.session_id));
							cleaned_count = cleaned_count.saturating_add(1);
						}
					}
					None => break,
				}
			}

			// 移除命中的过期消息及其会话索引。
			for (msg_id, session_id) in messages_to_remove.iter() {
				Messages::<T>::remove(msg_id);
				SessionMessages::<T>::remove(session_id, msg_id);
			}

			// 推进游标：扫满预算则记录续扫位置；否则已到表尾，复位游标。
			if scanned >= limit {
				LastCleanupCursor::<T>::set(last_seen);
			} else {
				LastCleanupCursor::<T>::kill();
			}

			Self::deposit_event(Event::OldMessagesCleanedUp {
				count: cleaned_count,
			});

			Ok(())
		}

		/// 函数级详细中文注释：注册聊天用户ID
		///
		/// # 参数
		/// - `nickname`: 可选的用户昵称
		///
		/// # 功能
		/// - 为调用者创建聊天用户ID和基础资料
		/// - 如果用户已注册则返回错误
		/// - 可以在注册时设置昵称
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::register_chat_user())]
		pub fn register_chat_user(
			origin: OriginFor<T>,
			nickname: Option<Vec<u8>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 检查是否已注册
			ensure!(
				!AccountToChatUserId::<T>::contains_key(&who),
				Error::<T>::ChatUserAlreadyExists
			);

			// 创建聊天用户ID
			let chat_user_id = Self::get_or_create_chat_user_id(&who)?;

			// 更新昵称（如果提供）
			if let Some(nick_vec) = nickname {
				ensure!(
					nick_vec.len() <= T::MaxNicknameLength::get() as usize,
					Error::<T>::NicknameTooLong
				);

				let nick_bounded: BoundedVec<u8, T::MaxNicknameLength> = nick_vec
					.try_into()
					.map_err(|_| Error::<T>::NicknameTooLong)?;

				ChatUserProfiles::<T>::mutate(chat_user_id, |profile_opt| {
					if let Some(ref mut profile) = profile_opt {
						profile.nickname = Some(nick_bounded);
						profile.last_active = T::UnixTime::now().as_secs();
					}
				});
			}

			Ok(())
		}

		/// 函数级详细中文注释：更新用户资料
		///
		/// # 参数
		/// - `nickname`: 可选的昵称更新
		/// - `avatar_cid`: 可选的头像CID更新
		/// - `signature`: 可选的个性签名更新
		///
		/// # 功能
		/// - 更新调用者的聊天用户资料
		/// - 如果用户未注册则自动创建
		/// - 只更新提供的字段，未提供的保持不变
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::update_chat_profile())]
		pub fn update_chat_profile(
			origin: OriginFor<T>,
			nickname: Option<Vec<u8>>,
			avatar_cid: Option<Vec<u8>>,
			signature: Option<Vec<u8>>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 获取或创建聊天用户ID
			let chat_user_id = Self::get_or_create_chat_user_id(&who)?;

			// 验证和转换数据
			let nickname_bounded = if let Some(nick_vec) = nickname {
				ensure!(
					nick_vec.len() <= T::MaxNicknameLength::get() as usize,
					Error::<T>::NicknameTooLong
				);
				Some(Some(nick_vec.try_into().map_err(|_| Error::<T>::NicknameTooLong)?))
			} else {
				None
			};

			let avatar_cid_bounded = if let Some(cid_vec) = avatar_cid {
				ensure!(
					cid_vec.len() <= T::MaxCidLen::get() as usize,
					Error::<T>::CidTooLong
				);
				Some(Some(cid_vec.try_into().map_err(|_| Error::<T>::CidTooLong)?))
			} else {
				None
			};

			let signature_bounded = if let Some(sig_vec) = signature {
				ensure!(
					sig_vec.len() <= T::MaxSignatureLength::get() as usize,
					Error::<T>::SignatureTooLong
				);
				Some(Some(sig_vec.try_into().map_err(|_| Error::<T>::SignatureTooLong)?))
			} else {
				None
			};

			// 更新用户资料
			ChatUserProfiles::<T>::mutate(chat_user_id, |profile_opt| {
				if let Some(ref mut profile) = profile_opt {
					if let Some(nick) = nickname_bounded {
						profile.nickname = nick;
					}
					if let Some(avatar) = avatar_cid_bounded {
						profile.avatar_cid = avatar;
					}
					if let Some(sig) = signature_bounded {
						profile.signature = sig;
					}
					profile.last_active = T::UnixTime::now().as_secs();
				}
			});

			// 触发事件
			Self::deposit_event(Event::ChatUserProfileUpdated {
				chat_user_id,
			});

			Ok(())
		}

		/// 函数级详细中文注释：设置用户状态
		///
		/// # 参数
		/// - `status_code`: 用户状态代码 (0=Online, 1=Offline, 2=Busy, 3=Away, 4=Invisible)
		///
		/// # 功能
		/// - 更新调用者的在线状态
		/// - 如果用户未注册则自动创建
		/// - 自动更新最后活跃时间
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::set_user_status())]
		pub fn set_user_status(
			origin: OriginFor<T>,
			status_code: u8,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 转换状态代码
			let status = match status_code {
				0 => UserStatus::Online,
				1 => UserStatus::Offline,
				2 => UserStatus::Busy,
				3 => UserStatus::Away,
				4 => UserStatus::Invisible,
				_ => return Err(Error::<T>::InvalidUserStatus.into()),
			};

			// 获取或创建聊天用户ID
			let chat_user_id = Self::get_or_create_chat_user_id(&who)?;

			// 更新用户状态
			ChatUserProfiles::<T>::mutate(chat_user_id, |profile_opt| {
				if let Some(ref mut profile) = profile_opt {
					profile.status = status.clone();
					profile.last_active = T::UnixTime::now().as_secs();
				}
			});

			// 触发事件
			Self::deposit_event(Event::ChatUserStatusChanged {
				chat_user_id,
				new_status: status_code,
			});

			Ok(())
		}

		/// 更新资料展示设置（UI 偏好；非通信权限）。
		/// Update profile display settings (UI preferences; not a communication gate).
		///
		/// # 参数 / Params
		/// - `show_online_status`: 是否显示在线状态 / show online status.
		/// - `show_last_active`: 是否显示最后活跃时间 / show last-active time.
		///
		/// # 说明 / Notes
		/// 已弃用的 `allow_stranger_messages` 参数已删除（审计 2.8）——通信权限只由
		/// pallet-chat-permission 的 `permission_level` 决定。用户未注册则自动创建资料。
		/// The deprecated `allow_stranger_messages` param was removed (audit 2.8);
		/// communication gating is decided solely by pallet-chat-permission.
		#[pallet::call_index(15)]
		#[pallet::weight(T::WeightInfo::update_privacy_settings())]
		pub fn update_privacy_settings(
			origin: OriginFor<T>,
			show_online_status: Option<bool>,
			show_last_active: Option<bool>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 获取或创建聊天用户ID
			let chat_user_id = Self::get_or_create_chat_user_id(&who)?;

			// 更新展示设置
			ChatUserProfiles::<T>::mutate(chat_user_id, |profile_opt| {
				if let Some(ref mut profile) = profile_opt {
					if let Some(show_online) = show_online_status {
						profile.privacy_settings.show_online_status = show_online;
					}
					if let Some(show_active) = show_last_active {
						profile.privacy_settings.show_last_active = show_active;
					}
					profile.last_active = T::UnixTime::now().as_secs();
				}
			});

			// 触发事件
			Self::deposit_event(Event::PrivacySettingsUpdated {
				chat_user_id,
			});

			Ok(())
		}
	}

	/// 受信系统通知端口实现（见顶层 [`crate::SystemNotifier`]）。
	/// Trusted system-notification port impl (see top-level [`crate::SystemNotifier`]).
	impl<T: Config> crate::SystemNotifier<T::AccountId> for Pallet<T> {
		fn notify(receiver: &T::AccountId, notice: Vec<u8>) -> DispatchResult {
			// sender = 平台系统账户；session=None → 自动复用/创建 system↔receiver 会话。
			// 复用 `do_send` 的 System 分支：受信来源跳过权限闸门与限频，仅做 CID sanity。
			// sender = platform system account; reuse `do_send`'s System branch
			// (skips permission gate + rate limit; CID sanity only).
			Self::do_send(
				T::SystemAccount::get(),
				receiver.clone(),
				notice,
				MessageType::System,
				None,
			)
		}
	}

	impl<T: Config> Pallet<T> {
		/// 消息发送的共享内部实现（`send_message` extrinsic 与 `SystemNotifier::notify` 共用）。
		/// Shared internal send path used by the `send_message` extrinsic and `SystemNotifier::notify`.
		///
		/// 校验顺序：统一权限闸门（chat-permission 单一来源；System 跳过）→ CID 长度 →
		/// 会话参与者校验（修复消息注入）→ 落库 + 索引 + 未读 + 事件。
		/// Checks: unified permission gate (chat-permission; System bypasses) → CID
		/// length → session participant validation → store + index + unread + events.
		fn do_send(
			sender: T::AccountId,
			receiver: T::AccountId,
			content_cid: Vec<u8>,
			msg_type: MessageType,
			session_id: Option<T::Hash>,
		) -> DispatchResult {
			// 当前所有公开入口（`send_message` 仅接受 System、`SystemNotifier::notify`）
			// 都只发 `System`，故下面 `!is_system` 分支对外 **不可达**；保留它仅作为共享
			// 私有入口 `do_send` 的纵深防御（若未来有调用方路由可门控类型至此）。System 是
			// 平台通知：来自受信来源（`SystemMessageOrigin::ensure_origin` 已在调用处强制），
			// 必须无视接收方隐私级别直达，因此跳过权限闸门。人类消息已迁出链下（MLS + relay），
			// 链上不再限频。
			// All public entries (`send_message` accepts System only and
			// `SystemNotifier::notify`) emit `System`, so the
			// `!is_system` branch below is UNREACHABLE from outside; it is kept purely
			// as defense-in-depth for the shared private helper `do_send`. System is a
			// platform notification from a trusted origin (already enforced by
			// `SystemMessageOrigin::ensure_origin` at the call site) and must reach the
			// recipient regardless of privacy, so it bypasses the permission gate.
			// Human messages are off-chain (MLS + relay); on-chain rate limiting was
			// removed as dead code (audit: chat-core historical layer).
			let is_system = matches!(msg_type, MessageType::System);
			if !is_system {
				// 统一权限闸门（chat-permission 为唯一事实来源；内部串联场景授权 / 隐私级别）。
				// Single permission gate: chat-permission is the sole source of truth.
				ensure!(
					<T::ChatPermission as pallet_chat_permission::ChatPermissionChecker<T::AccountId>>::can_send_message(&sender, &receiver),
					Error::<T>::ChatNotAuthorized
				);
			}

			// CID 格式 sanity（非空 + 不超长）。
			// CID format sanity (non-empty + within bound).
			//
			// 注意：链**不**校验 CID 内容是否加密——旧版 `is_cid_encrypted` 只是
			// “长度>46 且不以 Qm 开头” 的可绕过启发式，提供的是**虚假安全感**（审计 C）：
			// 攻击者随手构造一个长字节串即可通过，未加密的 CIDv1 也会被误判为已加密。
			// 加密由客户端 MLS E2EE 保证（见 chat-core × MLS 收敛 §8/§14），链只存不透明 CID。
			// The chain does NOT verify CID encryption: the former `is_cid_encrypted`
			// heuristic was trivially bypassable security theater (audit C). Encryption
			// is guaranteed solely by client-side MLS E2EE; the chain stores an opaque CID.
			ensure!(!content_cid.is_empty(), Error::<T>::InvalidCid);
			ensure!(content_cid.len() <= T::MaxCidLen::get() as usize, Error::<T>::CidTooLong);

			let cid_bounded: BoundedVec<u8, T::MaxCidLen> = content_cid
				.try_into()
				.map_err(|_| Error::<T>::CidTooLong)?;

			// 获取或创建ChatUserId（双方）
			let sender_chat_id = Self::get_or_create_chat_user_id(&sender).ok();
			let receiver_chat_id = Self::get_or_create_chat_user_id(&receiver).ok();

			// 获取或创建会话
			let session_id = if let Some(id) = session_id {
				// 【安全检查4】校验传入会话的参与者归属（修复消息注入漏洞）。
				// Security: validate participant membership of a caller-supplied session
				// to prevent injecting messages into an unrelated third-party session.
				let session = Sessions::<T>::get(id).ok_or(Error::<T>::SessionNotFound)?;
				ensure!(
					session.participants.contains(&sender)
						&& session.participants.contains(&receiver),
					Error::<T>::NotSessionParticipant
				);
				id
			} else {
				Self::create_session(&sender, &receiver)?
			};

			// 生成消息ID
			let msg_id = NextMessageId::<T>::get();
			NextMessageId::<T>::put(msg_id.saturating_add(1));

			// 创建消息（包含 ChatUserId）
			let now = <frame_system::Pallet<T>>::block_number();
			let message = MessageMeta {
				sender: sender.clone(),
				receiver: receiver.clone(),
				sender_chat_id,
				receiver_chat_id,
				content_cid: cid_bounded,
				session_id,
				msg_type,
				sent_at: now,
				is_read: false,
				is_deleted_by_sender: false,
				is_deleted_by_receiver: false,
				is_recalled: false,
			};

			// 存储消息
			Messages::<T>::insert(msg_id, message);

			// 更新会话
			Sessions::<T>::try_mutate(session_id, |maybe_session| -> DispatchResult {
				let session = maybe_session.as_mut().ok_or(Error::<T>::SessionNotFound)?;
				session.last_message_id = msg_id;
				session.last_active = now;
				Ok(())
			})?;

			// 添加到会话消息索引
			SessionMessages::<T>::insert(session_id, msg_id, ());

			// 增加未读计数
			UnreadCount::<T>::mutate((receiver.clone(), session_id), |count| {
				*count = count.saturating_add(1);
			});

			// 单一发送事件（含 ChatUserId）。`content_cid` 不入事件——已在 `Messages`
			// 存储，避免事件冗余。Single send event (with ChatUserId); `content_cid`
			// stays in `Messages` storage to avoid event bloat.
			Self::deposit_event(Event::MessageSent {
				msg_id,
				session_id,
				sender,
				receiver,
				sender_chat_id,
				receiver_chat_id,
			});

			Ok(())
		}

		/// 函数级详细中文注释：创建会话
		/// 
		/// # 参数
		/// - `user1`: 第一个用户
		/// - `user2`: 第二个用户
		/// 
		/// # 返回
		/// - 会话ID
		/// 
		/// # 流程
		/// 1. 生成会话ID（基于两个用户地址的哈希）
		/// 2. 检查会话是否已存在
		/// 3. 创建新会话
		/// 4. 添加到用户会话列表
		/// 5. 触发事件
		pub fn create_session(
			user1: &T::AccountId,
			user2: &T::AccountId,
		) -> Result<T::Hash, DispatchError> {
			// 生成会话ID（基于两个用户地址的哈希，需要排序保证一致性）
			let mut participants = alloc::vec![user1.clone(), user2.clone()];
			participants.sort();
			let session_id = T::Hashing::hash_of(&participants);

			// 检查会话是否已存在
			if Sessions::<T>::contains_key(session_id) {
				return Ok(session_id);
			}

			// 创建新会话
			let now = <frame_system::Pallet<T>>::block_number();
			let participants_bounded: BoundedVec<T::AccountId, ConstU32<2>> =
				participants.clone().try_into().map_err(|_| Error::<T>::TooManyParticipants)?;

			let session = Session {
				id: session_id,
				participants: participants_bounded.clone(),
				last_message_id: 0,
				last_active: now,
				created_at: now,
				is_archived: false,
			};

			Sessions::<T>::insert(session_id, session);

			// 添加到用户会话索引
			for user in participants.iter() {
				UserSessions::<T>::insert(user, session_id, ());
			}

			Self::deposit_event(Event::SessionCreated {
				session_id,
				participants: participants_bounded,
			});

			Ok(session_id)
		}

		/// 函数级详细中文注释：查询单条消息
		/// 
		/// # 参数
		/// - `message_id`: 消息ID
		/// 
		/// # 返回
		/// - Some(MessageMeta): 消息元数据
		/// - None: 消息不存在
		pub fn get_message(message_id: u64) -> Option<MessageMeta<T>> {
			Messages::<T>::get(message_id)
		}

		/// 函数级详细中文注释：分页查询会话消息
		/// 
		/// # 参数
		/// - `session_id`: 会话ID
		/// - `offset`: 偏移量（从0开始）
		/// - `limit`: 每页数量（最多100条）
		/// 
		/// # 返回
		/// - Vec<u64>: 消息ID列表（按时间倒序）
		/// 
		/// # 说明
		/// 返回最新的消息优先（倒序），前端需要再次查询消息详情
		pub fn list_messages_by_session(
			session_id: T::Hash,
			offset: u32,
			limit: u32,
		) -> Vec<u64> {
			// Hardening (audit P2): unbounded prefix iteration is acceptable here because this
			// is a read-only helper (runtime API / RPC), not an extrinsic, so it cannot DoS the
			// chain. Heavy users may be slow; the node RPC layer must enforce its own limits.
			// 加固（审计 P2）：此处全前缀迭代无界，但本函数是只读接口（runtime API / RPC），
			// 非 extrinsic，不构成链上 DoS；重度用户可能较慢，需由 node RPC 侧自行限流。
			let mut messages: Vec<u64> = SessionMessages::<T>::iter_prefix(session_id)
				.map(|(msg_id, _)| msg_id)
				.collect();
			
			// 按消息ID排序（消息ID是递增的，所以倒序就是最新的在前）
			messages.sort_by(|a, b| b.cmp(a));
			
			let total = messages.len();
			
			// 限制每页最多100条
			let limit = limit.min(100) as usize;
			let offset = offset as usize;
			
			if offset >= total {
				return Vec::new();
			}
			
			// 分页返回
			messages
				.into_iter()
				.skip(offset)
				.take(limit)
				.collect()
		}

		/// 函数级详细中文注释：查询会话信息
		/// 
		/// # 参数
		/// - `session_id`: 会话ID
		/// 
		/// # 返回
		/// - Some(Session): 会话信息
		/// - None: 会话不存在
		pub fn get_session(session_id: T::Hash) -> Option<Session<T>> {
			Sessions::<T>::get(session_id)
		}

		/// 校验某账户是存在会话的参与者。
		/// Ensure an account is a participant of an existing session.
		fn ensure_session_participant(
			who: &T::AccountId,
			session_id: T::Hash,
		) -> DispatchResult {
			let session = Sessions::<T>::get(session_id).ok_or(Error::<T>::SessionNotFound)?;
			ensure!(session.participants.contains(who), Error::<T>::NotSessionParticipant);
			Ok(())
		}

		/// 查询会话是否被该用户设为免打扰。/ Whether the user muted this session.
		pub fn is_session_muted(user: &T::AccountId, session_id: T::Hash) -> bool {
			SessionMuted::<T>::contains_key(user, session_id)
		}

		/// 查询会话是否被该用户置顶。/ Whether the user pinned this session.
		pub fn is_session_pinned(user: &T::AccountId, session_id: T::Hash) -> bool {
			SessionPinned::<T>::contains_key(user, session_id)
		}

		/// 函数级详细中文注释：查询用户的所有会话
		/// 
		/// # 参数
		/// - `user`: 用户账户
		/// 
		/// # 返回
		/// - Vec<T::Hash>: 会话ID列表。置顶会话排在最前（按置顶时间倒序），
		///   其余按最后活跃时间倒序。
		///   Pinned sessions first (most-recently-pinned first), then the rest by
		///   last-active descending.
		pub fn list_sessions(user: T::AccountId) -> Vec<T::Hash> {
			// Hardening (audit P2): read-only helper; unbounded prefix iteration cannot DoS the
			// chain. The node RPC layer must bound caller cost for heavy users.
			// 加固（审计 P2）：只读接口，全前缀迭代不构成链上 DoS；重度用户成本由 node RPC 侧限流。
			let session_ids: Vec<T::Hash> = UserSessions::<T>::iter_prefix(&user)
				.map(|(sid, _)| sid)
				.collect();

			// 分离置顶与非置顶 / split pinned vs unpinned
			let mut pinned: Vec<(T::Hash, BlockNumberFor<T>)> = Vec::new();
			let mut others: Vec<(T::Hash, BlockNumberFor<T>)> = Vec::new();
			for sid in session_ids.iter() {
				if let Some(session) = Sessions::<T>::get(sid) {
					if let Some(pinned_at) = SessionPinned::<T>::get(&user, sid) {
						pinned.push((*sid, pinned_at));
					} else {
						others.push((*sid, session.last_active));
					}
				}
			}

			// 置顶按置顶时间倒序；其余按最后活跃时间倒序 / sort each section desc
			pinned.sort_by(|a, b| b.1.cmp(&a.1));
			others.sort_by(|a, b| b.1.cmp(&a.1));

			pinned
				.into_iter()
				.chain(others.into_iter())
				.map(|(sid, _)| sid)
				.collect()
		}

		/// 函数级详细中文注释：查询未读消息数
		/// 
		/// # 参数
		/// - `user`: 用户账户
		/// - `session_id`: 会话ID（可选）
		/// 
		/// # 返回
		/// - u32: 未读消息数
		/// 
		/// # 说明
		/// - 如果提供session_id，返回该会话的未读数
		/// - 如果不提供session_id，返回用户所有会话的未读总数
		pub fn get_unread_count(user: T::AccountId, session_id: Option<T::Hash>) -> u32 {
			if let Some(sid) = session_id {
				// 查询指定会话的未读数
				UnreadCount::<T>::get((user, sid))
			} else {
				// 查询所有会话的未读总数
				// Hardening (audit P2): read-only aggregate; unbounded prefix iteration cannot DoS
				// the chain. The node RPC layer must bound caller cost for heavy users.
				// 加固（审计 P2）：只读聚合，全前缀迭代不构成链上 DoS；成本由 node RPC 侧限流。
				let session_ids: Vec<T::Hash> = UserSessions::<T>::iter_prefix(&user)
					.map(|(sid, _)| sid)
					.collect();
				session_ids
					.iter()
					.map(|&sid| UnreadCount::<T>::get((user.clone(), sid)))
					.sum()
			}
		}

		// 黑名单查询（is_blocked / list_blocked_users）已随存储迁移至
		// pallet-chat-permission；前端改用 chat-permission 的隐私设置查询接口。
		// Blacklist queries moved to pallet-chat-permission together with the
		// storage; query the privacy settings there instead.

		// ===== ChatUserId 相关功能 =====

		/// 函数级详细中文注释：生成11位数聊天用户ID
		///
		/// # 返回
		/// - Ok(ChatUserId): 生成的唯一11位数ID
		/// - Err(DispatchError): ID生成失败
		///
		/// # 说明
		/// - ID范围：10,000,000,000 - 99,999,999,999 (11位数)
		/// - 使用多源随机数确保唯一性和随机性
		/// - 最大重试100次防止无限循环
		pub fn generate_chat_user_id() -> Result<ChatUserId, DispatchError> {
			const MIN_ID: u64 = 10_000_000_000;  // 11位数最小值
			const MAX_ID: u64 = 99_999_999_999;  // 11位数最大值
			const MAX_RETRIES: u8 = 100;         // 最大重试次数

			// 取出并自增全局生成计数器，作为本次生成的种子 nonce（O(1)）。
			// Take-and-bump the global counter; used as a per-call seed nonce (O(1)).
			let nonce = NextChatUserId::<T>::mutate(|n| {
				let v = *n;
				*n = n.saturating_add(1);
				v
			});

			for attempt in 0..MAX_RETRIES {
				// 获取多源随机种子
				let random_seed = Self::get_random_seed_for_chat(attempt, nonce);

				// 从种子生成候选ID
				let candidate_id = Self::generate_id_from_seed(random_seed, MIN_ID, MAX_ID);

				// 检查ID是否已被使用
				if !UsedChatUserIds::<T>::contains_key(&candidate_id) {
					// 标记为已使用
					UsedChatUserIds::<T>::insert(&candidate_id, true);
					return Ok(candidate_id);
				}
			}

			// 重试次数用完，返回错误
			Err(Error::<T>::ChatUserIdGenerationFailed.into())
		}

		/// 函数级详细中文注释：获取聊天用户ID专用的随机种子
		///
		/// # 参数
		/// - `attempt`: 当前重试次数，增加随机性
		/// - `nonce`: 全局生成计数器（单调递增），保证逐次调用种子互异
		///
		/// # 返回
		/// - [u8; 32]: 32字节随机种子
		///
		/// # 说明
		/// 结合多个随机源：系统随机数、时间戳、块号、重试次数、全局计数器 nonce。
		/// 注意：不再使用 `UsedChatUserIds::iter().count()`（O(n) 全表扫描），
		/// 改用 O(1) 的 `nonce` 提供逐次差异，避免随用户增长的权重失真与 DoS。
		fn get_random_seed_for_chat(attempt: u8, nonce: u64) -> [u8; 32] {
			let mut seed = [0u8; 32];

			// 1. 系统随机数（主要随机源）
			let random = T::Randomness::random(&b"chat_user_id"[..]).0;
			seed[0..32].copy_from_slice(&random.as_ref()[0..32]);

			// 2. 混合当前时间戳（增加时间随机性）
			let timestamp = T::UnixTime::now().as_secs();
			let timestamp_bytes = timestamp.to_le_bytes();
			for i in 0..8 {
				seed[i] ^= timestamp_bytes[i % 8];
			}

			// 3. 混合块号（增加区块随机性）
			let block_number = <frame_system::Pallet<T>>::block_number();
			if let Ok(block_u64) = TryInto::<u64>::try_into(block_number) {
				let block_bytes = block_u64.to_le_bytes();
				for i in 0..8 {
					seed[8 + i] ^= block_bytes[i];
				}
			}

			// 4. 混合重试次数（防止连续碰撞）
			seed[16] ^= attempt;

			// 5. 混合全局生成计数器 nonce（O(1)，替代旧版 UsedChatUserIds 全表扫描）。
			//    保证同区块内不同账户 / 同次多重试都得到不同种子。
			let nonce_bytes = nonce.to_le_bytes();
			for i in 0..8 {
				seed[17 + i] ^= nonce_bytes[i];
			}

			seed
		}

		/// 函数级详细中文注释：从种子生成指定范围内的ID
		///
		/// # 参数
		/// - `seed`: 32字节随机种子
		/// - `min`: 最小ID值
		/// - `max`: 最大ID值
		///
		/// # 返回
		/// - u64: 范围内的随机ID
		fn generate_id_from_seed(seed: [u8; 32], min: u64, max: u64) -> u64 {
			// 使用前8字节生成基础随机数
			let random_u64 = u64::from_le_bytes([
				seed[0], seed[1], seed[2], seed[3],
				seed[4], seed[5], seed[6], seed[7]
			]);

			// 使用中间8字节增加随机性
			let random_u64_2 = u64::from_le_bytes([
				seed[8], seed[9], seed[10], seed[11],
				seed[12], seed[13], seed[14], seed[15]
			]);

			// 合并两个随机数
			let combined_random = random_u64.wrapping_add(random_u64_2);

			// 映射到指定范围
			min + (combined_random % (max - min + 1))
		}

		/// 函数级详细中文注释：为账户获取或创建聊天用户ID
		///
		/// # 参数
		/// - `account`: 要获取/创建ID的账户
		///
		/// # 返回
		/// - Ok(ChatUserId): 聊天用户ID
		/// - Err(DispatchError): 创建失败
		///
		/// # 说明
		/// - 如果账户已有ChatUserId则直接返回
		/// - 否则生成新ID并建立映射关系
		/// - 同时创建默认用户资料
		pub fn get_or_create_chat_user_id(
			account: &T::AccountId
		) -> Result<ChatUserId, DispatchError> {
			// 检查是否已存在聊天用户ID
			if let Some(existing_id) = AccountToChatUserId::<T>::get(account) {
				return Ok(existing_id);
			}

			// 生成新的聊天用户ID
			let new_chat_user_id = Self::generate_chat_user_id()?;

			// 建立双向映射关系
			AccountToChatUserId::<T>::insert(account, new_chat_user_id);
			ChatUserIdToAccount::<T>::insert(new_chat_user_id, account);

			// 创建默认用户资料
			let profile = ChatUserProfile {
				nickname: None,
				avatar_cid: None,
				signature: None,
				status: UserStatus::Online,
				privacy_settings: ProfileDisplaySettings::default(),
				created_at: T::UnixTime::now().as_secs(),
				last_active: T::UnixTime::now().as_secs(),
			};

			ChatUserProfiles::<T>::insert(new_chat_user_id, profile);

			// 触发事件
			Self::deposit_event(Event::ChatUserCreated {
				account_id: account.clone(),
				chat_user_id: new_chat_user_id,
			});

			Ok(new_chat_user_id)
		}

		/// 函数级详细中文注释：通过聊天用户ID查找账户
		///
		/// # 参数
		/// - `chat_user_id`: 聊天用户ID
		///
		/// # 返回
		/// - Some(T::AccountId): 对应的账户ID
		/// - None: 不存在对应关系
		pub fn get_account_by_chat_user_id(
			chat_user_id: ChatUserId
		) -> Option<T::AccountId> {
			ChatUserIdToAccount::<T>::get(chat_user_id)
		}

		/// 函数级详细中文注释：通过账户查找聊天用户ID
		///
		/// # 参数
		/// - `account`: 账户ID
		///
		/// # 返回
		/// - Some(ChatUserId): 对应的聊天用户ID
		/// - None: 尚未注册聊天用户
		pub fn get_chat_user_id_by_account(
			account: &T::AccountId
		) -> Option<ChatUserId> {
			AccountToChatUserId::<T>::get(account)
		}

		/// 函数级详细中文注释：获取聊天用户资料
		///
		/// # 参数
		/// - `chat_user_id`: 聊天用户ID
		///
		/// # 返回
		/// - Some(ChatUserProfile): 用户资料
		/// - None: 用户不存在
		pub fn get_chat_user_profile(
			chat_user_id: ChatUserId
		) -> Option<ChatUserProfile<T>> {
			ChatUserProfiles::<T>::get(chat_user_id)
		}

		// 陌生人消息校验已收敛到 pallet-chat-permission（单一权限源）：
		// 其 `permission_level`（Open / FriendsOnly / Whitelist / Closed）即覆盖
		// “陌生人能否发起聊天” 的语义，故 chat-core 不再单独维护该闸门（审计 I）。
		// Stranger-message gating is consolidated into pallet-chat-permission,
		// whose `permission_level` already expresses whether strangers may start a
		// chat; chat-core no longer keeps a separate check (audit finding I).

		/// 函数级详细中文注释：计算会话ID
		///
		/// # 参数
		/// - `account1`: 第一个参与者账户
		/// - `account2`: 第二个参与者账户
		///
		/// # 返回
		/// - T::Hash: 会话的唯一标识符
		///
		/// # 说明
		/// 为两个账户生成确定性的会话ID，无论参数顺序如何都返回相同结果
		pub fn get_session_id(
			account1: &T::AccountId,
			account2: &T::AccountId,
		) -> T::Hash {
			// 确保账户顺序一致，生成确定性的会话ID
			let mut participants = vec![account1.clone(), account2.clone()];
			participants.sort();

			T::Hashing::hash_of(&participants)
		}
	}
}

