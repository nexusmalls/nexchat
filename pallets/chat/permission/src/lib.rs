//! # 聊天权限系统 Pallet
//!
//! 实现基于场景的聊天权限控制系统，支持同一聊天会话应用于多个业务场景。
//!
//! ## 概述
//!
//! 本模块提供以下功能：
//! - 用户隐私设置管理（权限级别、黑白名单）
//! - 场景授权管理（多场景共存）
//! - 聊天权限检查
//! - 聊天能力撤销纪元（链下能力令牌的链上撤销锚点）
//!
//! ## 隐私：联系人/好友已移出链上 / Privacy: contacts moved off-chain
//!
//! EN: The on-chain bidirectional friend graph (`Friendships` + friend requests
//! + remarks/groups) was removed so that *who is connected to whom* is no longer
//! publicly observable on-chain. Contacts and the "may DM me" right are now
//! receiver-signed **chat capability tokens** stored off-chain (encrypted contact
//! vault). The chain keeps only a per-account [`CapabilityEpoch`] revocation
//! counter (bumped via `bump_capability_epoch`) so relays/clients can invalidate
//! stale capabilities. CN: 链上双向好友图谱（`Friendships` + 好友申请 + 备注/分组）
//! 已删除，使「谁与谁建立联系」不再在链上公开可见。联系人与「允许私聊我」的权利现以
//! 链下、由接收方签名的**聊天能力令牌**承载（加密通讯录保险库）。链上仅保留每账户
//! 的 [`CapabilityEpoch`] 撤销计数器（经 `bump_capability_epoch` 递增），供 relay/
//! 客户端使过期能力失效。
//!
//! ## 核心概念
//!
//! - **聊天会话**: 两个用户之间的通信通道，唯一
//! - **场景授权**: 为什么这两个用户可以聊天的原因，可以有多个
//! - **权限判定**: 平台禁言 → 黑名单 → 场景授权 → 隐私设置
//!
//! ## 使用示例
//!
//! ```ignore
//! // 业务 pallet 授予场景授权
//! T::ChatPermission::grant_bidirectional_scene_authorization(
//!     *b"otc_ordr",
//!     &buyer,
//!     &seller,
//!     SceneType::Order,
//!     SceneId::Numeric(order_id),
//!     Some(30 * 24 * 60 * 10), // 30天
//!     "订单#123".as_bytes().to_vec(),
//! )?;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod traits;
mod types;
pub mod runtime_api;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use traits::*;
pub use types::*;
pub use runtime_api::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Saturating;
    use sp_runtime::SaturatedConversion;
    use sp_std::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Pallet 配置 trait
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        // NOTE / 注意（审计 P1）：`MaxBlockListSize` / `MaxWhitelistSize` 已随链上
        // 黑名单 / 白名单一并移除——拉黑与放行改由链下能力令牌承载（见 `PrivacySettings`
        // 与 `CapabilityEpoch` 说明）。The on-chain block/whitelist (and their size
        // bounds) were removed for privacy; blocking/allowing is off-chain now.

        /// 单对用户最大场景授权数量
        /// 考虑场景：多个订单 + 多个纪念馆 + 群聊等
        #[pallet::constant]
        type MaxScenesPerPair: Get<u32>;

        /// EN: Privileged origin (Root / governance) for platform-compliance
        /// actions: muting accounts and resolving reports. CN: 平台合规动作
        /// （禁言账号、处理举报）的特权来源（Root / 治理）。
        type GovernanceOrigin: frame_support::traits::EnsureOrigin<Self::RuntimeOrigin>;

        /// EN: Max length (bytes) of a report's evidence IPFS CID.
        /// CN: 举报证据 IPFS CID 的字节上限。
        #[pallet::constant]
        type MaxReportCidLen: Get<u32>;

        /// EN: Hard cap on the number of unresolved (open) reports kept on-chain
        /// at once (bounds `Reports` growth; governance prunes via `resolve_report`).
        /// CN: 链上同时保留的未处理（open）举报数量硬上限（约束 `Reports` 增长；
        /// 治理经 `resolve_report` 清理）。
        #[pallet::constant]
        type MaxOpenReports: Get<u32>;

        /// EN: Per-reporter cooldown (in blocks) between two `report` calls
        /// (anti-spam). CN: 单个举报人两次 `report` 之间的冷却（区块数，防刷）。
        #[pallet::constant]
        type ReportCooldown: Get<BlockNumberFor<Self>>;

        /// EN: Weight info / CN: 权重信息
        type WeightInfo: WeightInfo;
    }

    // ==================== 存储 ====================

    /// 用户隐私设置存储
    ///
    /// 存储每个用户的聊天权限配置，包括权限级别、黑白名单等。
    #[pallet::storage]
    #[pallet::getter(fn privacy_settings)]
    pub type PrivacySettingsOf<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        PrivacySettings<T>,
        ValueQuery,
    >;

    /// EN: Per-account chat-capability revocation epoch (a monotonic counter).
    /// The on-chain friend graph has been removed for privacy: contacts and the
    /// "may DM me" right now live entirely off-chain as receiver-signed chat
    /// capability tokens (see `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`). The chain's
    /// only role is to publish this epoch so off-chain relays/clients can reject
    /// capabilities issued before the latest `bump_capability_epoch` (e.g. after
    /// removing a contact or rotating a device). A token is fresh iff its embedded
    /// epoch equals `CapabilityEpoch[issuer]`.
    /// CN: 每账户的聊天能力撤销纪元（单调递增计数器）。为隐私起见已删除链上好友图谱：
    /// 联系人与「允许向我私聊」的权利现完全以链下、由接收方签名的聊天能力令牌承载
    /// （见 `CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md`）。链上唯一职责是公布该纪元，
    /// 供链下 relay/客户端拒绝在最近一次 `bump_capability_epoch`（如删除联系人或
    /// 更换设备）之前签发的能力令牌。令牌新鲜当且仅当其内嵌纪元等于
    /// `CapabilityEpoch[签发者]`。
    #[pallet::storage]
    #[pallet::getter(fn capability_epoch)]
    pub type CapabilityEpoch<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// EN: Accounts platform-muted by governance: `account -> MuteStatus`.
    /// A muted sender is denied by `check_permission` (`DeniedSenderMuted`).
    /// CN: 被治理平台级禁言的账户：`account -> MuteStatus`。被禁言的发送方在
    /// `check_permission` 中被拒（`DeniedSenderMuted`）。
    #[pallet::storage]
    pub type MutedAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, MuteStatus<BlockNumberFor<T>>, OptionQuery>;

    /// EN: Auto-increment id for the next report. CN: 下一条举报的自增 id。
    #[pallet::storage]
    pub type NextReportId<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// EN: Open (unresolved) reports kept for governance review: `id -> record`.
    /// CN: 供治理审阅的未处理举报：`id -> 记录`。
    #[pallet::storage]
    pub type Reports<T: Config> = StorageMap<_, Twox64Concat, u64, ReportRecord<T>, OptionQuery>;

    /// EN: Count of open reports (bounds `Reports` against `MaxOpenReports`).
    /// CN: 未处理举报计数（以 `MaxOpenReports` 约束 `Reports`）。
    #[pallet::storage]
    pub type OpenReportCount<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// EN: Last block at which an account filed a report (per-reporter cooldown).
    /// CN: 账户上次发起举报的区块（按举报人冷却）。
    #[pallet::storage]
    pub type LastReportAt<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

    /// 场景授权存储
    ///
    /// Key: (user1, user2) 按字典序排列，保证双向查询一致性
    /// Value: 场景授权列表
    ///
    /// # 隐私（审计 P2，固有权衡）/ Privacy (audit P2, inherent trade-off)
    /// EN: This map exposes that two accounts share a business context (the
    /// `(user1, user2)` pair + `scene_type` + `scene_id`). This is an **accepted**
    /// trade-off, not a new leak: scene authorizations are granted by business
    /// pallets whose source records already make the relationship public (e.g. a
    /// bounty has `poster`/`solver` on-chain, a group has its members on-chain).
    /// On-chain scene-based permission inherently mirrors that public link. The
    /// truly sensitive layer — message *content* and the social *contact graph* —
    /// is off-chain (MLS E2EE + off-chain capability tokens). To avoid widening
    /// the leak, callers MUST pass opaque/empty `metadata` (see field doc) and the
    /// design does not store amounts/names/notes here.
    /// CN: 本表会暴露两账户存在业务上下文（`(user1, user2)` 对 + `scene_type` +
    /// `scene_id`）。这是**可接受**的固有权衡，而非新增泄漏：场景授权由业务 pallet 授予，
    /// 其来源记录本就已公开该关系（如悬赏的 `poster`/`solver`、群的成员均在链上）。基于
    /// 场景的链上权限天然镜像该公开链接。真正敏感的层面——消息**内容**与社交**联系人图谱**
    /// ——在链下（MLS 端到端加密 + 链下能力令牌）。为不扩大泄漏面，调用方**必须**传入
    /// 不透明/空 `metadata`（见字段说明），本设计不在此存储金额/名称/备注。
    #[pallet::storage]
    #[pallet::getter(fn scene_authorizations)]
    pub type SceneAuthorizations<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<SceneAuthorization<BlockNumberFor<T>>, T::MaxScenesPerPair>,
        ValueQuery,
    >;

    // ==================== 事件 ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 隐私设置已更新
        PrivacySettingsUpdated {
            who: T::AccountId,
        },

        // NOTE / 注意（审计 P1）：`UserBlocked` / `UserUnblocked` /
        // `UserAddedToWhitelist` / `UserRemovedFromWhitelist` 事件已移除——它们会把
        // 拉黑 / 放行的对端账户明文广播到链上日志，泄露通信关系。拉黑 / 放行改由链下
        // 能力令牌承载，撤销以 `CapabilityEpochBumped` 表达。These events were removed
        // because they broadcast blocked/allowed counterparties in plaintext logs,
        // leaking relationships; blocking/allowing is off-chain (see CapabilityEpoch).

        /// EN: An account advanced its chat-capability revocation epoch, making all
        /// previously issued chat capability tokens (with the old epoch) stale.
        /// CN: 账户递增了聊天能力撤销纪元，使其此前签发的（旧纪元）能力令牌全部失效。
        CapabilityEpochBumped {
            who: T::AccountId,
            new_epoch: u32,
        },

        /// 场景授权已授予
        SceneAuthorizationGranted {
            source: [u8; 8],
            user1: T::AccountId,
            user2: T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
        },

        /// 场景授权已撤销
        SceneAuthorizationRevoked {
            source: [u8; 8],
            user1: T::AccountId,
            user2: T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
        },

        /// 场景授权已延期
        SceneAuthorizationExtended {
            user1: T::AccountId,
            user2: T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
            new_expires_at: Option<BlockNumberFor<T>>,
        },

        /// EN: An account was platform-muted by governance. CN: 账户被治理平台级禁言。
        AccountMuted {
            who: T::AccountId,
            /// EN: None = indefinite; Some(b) = until block b. CN: None 无限期；Some(b) 至区块 b。
            until: Option<BlockNumberFor<T>>,
        },

        /// EN: An account's platform mute was lifted by governance. CN: 账户的平台禁言被治理解除。
        AccountUnmuted {
            who: T::AccountId,
        },

        /// EN: A report was filed. CN: 已提交一条举报。
        ReportFiled {
            id: u64,
            reporter: T::AccountId,
        },

        /// EN: A report was resolved (and removed) by governance. CN: 举报被治理处理（并移除）。
        ReportResolved {
            id: u64,
            /// EN: whether the report was upheld. CN: 举报是否成立。
            upheld: bool,
        },
    }

    // ==================== 错误 ====================

    #[pallet::error]
    pub enum Error<T> {
        // NOTE / 注意（审计 P1）：黑名单 / 白名单相关错误（`BlockListFull`/`WhitelistFull`/
        // `AlreadyBlocked`/`NotInBlockList`/`CannotAddSelf`/`AlreadyInWhitelist`/
        // `NotInWhitelist`）已随链上名单移除一并删除。
        // Block/whitelist errors were removed together with the on-chain lists.

        /// 场景授权数量已达上限
        TooManyScenes,

        /// 场景授权不存在
        SceneAuthorizationNotFound,

        /// 场景授权已存在
        SceneAuthorizationAlreadyExists,

        /// 元数据过长
        MetadataTooLong,

        /// EN: Reporter is in cooldown between reports. CN: 举报人处于举报冷却期。
        ReportCooldown,

        /// EN: Too many open reports on-chain (hard cap hit). CN: 链上未处理举报已达上限。
        TooManyOpenReports,

        /// EN: No report with the given id. CN: 指定 id 的举报不存在。
        ReportNotFound,
    }

    // ==================== 用户调用 ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 设置聊天权限级别
        ///
        /// 用户可以设置自己的聊天权限策略：
        /// - Open: 任何人可发起聊天
        /// - FriendsOnly: 仅好友可发起（默认）
        /// - Whitelist: 仅白名单用户可发起
        /// - Closed: 不接受任何消息
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_permission_level())]
        pub fn set_permission_level(
            origin: OriginFor<T>,
            level: ChatPermissionLevel,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            PrivacySettingsOf::<T>::mutate(&who, |settings| {
                settings.permission_level = level;
                settings.updated_at = frame_system::Pallet::<T>::block_number();
            });

            Self::deposit_event(Event::PrivacySettingsUpdated { who });
            Ok(())
        }

        /// 设置拒绝的场景类型
        ///
        /// 用户可以选择拒绝某些类型的场景授权聊天。
        /// 例如，用户可以拒绝所有 MarketMaker 场景的聊天请求。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_rejected_scene_types())]
        pub fn set_rejected_scene_types(
            origin: OriginFor<T>,
            scene_types: BoundedVec<SceneType, ConstU32<10>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            PrivacySettingsOf::<T>::mutate(&who, |settings| {
                settings.rejected_scene_types = scene_types;
                settings.updated_at = frame_system::Pallet::<T>::block_number();
            });

            Self::deposit_event(Event::PrivacySettingsUpdated { who });
            Ok(())
        }

        // NOTE / 注意（审计 P1）：`block_user`（原 call_index 2）/ `unblock_user`
        // （原 call_index 3）已移除。链上黑名单会泄露「谁拉黑了谁」，故拉黑改由链下
        // 能力令牌实现：撤销该联系人的令牌（`bump_capability_epoch` 使旧令牌全失效）
        // 或定向撤销其信箱标签（`pallet-chat-inbox::revoke_tag`）。索引 2/3 刻意留空。
        // `block_user` / `unblock_user` were removed: an on-chain blocklist leaks
        // who-blocked-whom. Blocking is off-chain now (revoke the contact's
        // capability token via `bump_capability_epoch`, or the per-contact inbox
        // tag via `pallet-chat-inbox::revoke_tag`). Indices 2/3 left vacant.

        // NOTE / 注意：旧版好友握手 extrinsics（`add_friend`/`request_friend`/
        // `accept_friend`/`reject_friend`/`cancel_friend_request`/`remove_friend`/
        // `set_friend_meta`，原 call_index 4/8/9/10/11/5/12）已删除：链上好友图谱
        // 整体移出，联系人改由链下、接收方签名的能力令牌承载。索引 4/5/8/9/10/11/12
        // 刻意留空，避免复用造成语义混淆。
        // The old friend-handshake extrinsics were REMOVED: the on-chain friend
        // graph is gone and contacts now live off-chain as receiver-signed
        // capability tokens. Indices 4/5/8/9/10/11/12 are left vacant on purpose.

        /// EN: Advance the caller's chat-capability revocation epoch by one. This
        /// invalidates every chat capability token the caller previously signed
        /// (their embedded epoch no longer matches [`CapabilityEpoch`]), so
        /// off-chain relays/clients will reject DMs authorized by stale tokens.
        /// Use after removing a contact, rotating a device, or a suspected leak.
        /// CN: 将调用者的聊天能力撤销纪元加一。此操作使调用者此前签发的所有聊天能力
        /// 令牌失效（其内嵌纪元不再与 [`CapabilityEpoch`] 相符），链下 relay/客户端
        /// 将拒绝凭过期令牌授权的私聊。删除联系人、更换设备或疑似泄露后使用。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::bump_capability_epoch())]
        pub fn bump_capability_epoch(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let new_epoch = CapabilityEpoch::<T>::mutate(&who, |e| {
                *e = e.saturating_add(1);
                *e
            });
            Self::deposit_event(Event::CapabilityEpochBumped { who, new_epoch });
            Ok(())
        }

        // NOTE / 注意（审计 P1）：`add_to_whitelist`（原 call_index 6）/
        // `remove_from_whitelist`（原 call_index 7）已移除。链上白名单本质是一份
        // 「可向我私聊的联系人清单」，明文上链直接暴露社交关系。放行改由链下、接收方
        // 签名的能力令牌承载（`Whitelist` 级别现等同 `FriendsOnly`）。索引 6/7 刻意留空。
        // `add_to_whitelist` / `remove_from_whitelist` were removed: an on-chain
        // whitelist is a plaintext contact list that leaks relationships. Allowing
        // is off-chain via receiver-signed capability tokens (the `Whitelist`
        // level now behaves like `FriendsOnly`). Indices 6/7 left vacant.

        /// EN: Governance platform-mutes an account so it is denied as a chat
        /// *sender* (private chat via `can_send_message`, and on-chain group
        /// `commit`/`anchor` indirectly through clients). `until = None` mutes
        /// indefinitely; `Some(block)` mutes until that block. CN: 治理对账户施加
        /// 平台级禁言，使其作为聊天**发送方**被拒（私聊经 `can_send_message`；群的
        /// 链上 `commit`/`anchor` 由客户端间接遵循）。`until = None` 为无限期；
        /// `Some(block)` 为禁言至该区块。
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::force_mute_account())]
        pub fn force_mute_account(
            origin: OriginFor<T>,
            who: T::AccountId,
            until: Option<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            let status = match until {
                Some(b) => MuteStatus::Until(b),
                None => MuteStatus::Forever,
            };
            MutedAccounts::<T>::insert(&who, status);
            Self::deposit_event(Event::AccountMuted { who, until });
            Ok(())
        }

        /// EN: Governance lifts an account's platform mute. CN: 治理解除账户的平台禁言。
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::force_unmute_account())]
        pub fn force_unmute_account(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            MutedAccounts::<T>::remove(&who);
            Self::deposit_event(Event::AccountUnmuted { who });
            Ok(())
        }

        /// EN: File a report against an account / group / message for compliance.
        /// `reason_cid` is an IPFS CID pointing to off-chain evidence (no plaintext
        /// on-chain). Rate-limited per reporter and globally bounded by
        /// `MaxOpenReports`. CN: 就账户 / 群 / 消息发起合规举报。`reason_cid` 为指向
        /// 链下证据的 IPFS CID（链上无明文）。按举报人限频，并以 `MaxOpenReports`
        /// 全局上限约束。
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::report())]
        pub fn report(
            origin: OriginFor<T>,
            target: ReportTarget<T>,
            reason_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let now = frame_system::Pallet::<T>::block_number();

            // 举报人冷却 / per-reporter cooldown
            if let Some(last) = LastReportAt::<T>::get(&who) {
                ensure!(
                    now.saturating_sub(last) >= T::ReportCooldown::get(),
                    Error::<T>::ReportCooldown
                );
            }
            // 全局未处理上限 / global open-report cap
            let open = OpenReportCount::<T>::get();
            ensure!(open < T::MaxOpenReports::get(), Error::<T>::TooManyOpenReports);

            let reason_cid: BoundedVec<u8, T::MaxReportCidLen> =
                reason_cid.try_into().map_err(|_| Error::<T>::MetadataTooLong)?;

            let id = NextReportId::<T>::get();
            Reports::<T>::insert(
                id,
                ReportRecord::<T> { reporter: who.clone(), target, reason_cid, filed_at: now },
            );
            NextReportId::<T>::put(id.saturating_add(1));
            OpenReportCount::<T>::put(open.saturating_add(1));
            LastReportAt::<T>::insert(&who, now);

            Self::deposit_event(Event::ReportFiled { id, reporter: who });
            Ok(())
        }

        /// EN: Governance resolves (closes & removes) an open report, recording
        /// whether it was upheld. CN: 治理处理（关闭并移除）一条未处理举报，并记录是否成立。
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::resolve_report())]
        pub fn resolve_report(
            origin: OriginFor<T>,
            report_id: u64,
            upheld: bool,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(Reports::<T>::contains_key(report_id), Error::<T>::ReportNotFound);
            Reports::<T>::remove(report_id);
            OpenReportCount::<T>::mutate(|c| *c = c.saturating_sub(1));
            Self::deposit_event(Event::ReportResolved { id: report_id, upheld });
            Ok(())
        }
    }

    // ==================== 内部方法 ====================

    impl<T: Config> Pallet<T> {
        /// 获取排序后的用户对（保证存储一致性）
        ///
        /// 将两个用户按字典序排列，确保无论传入顺序如何，
        /// 都能查询到同一条存储记录。
        pub fn sorted_pair(
            user1: &T::AccountId,
            user2: &T::AccountId,
        ) -> (T::AccountId, T::AccountId) {
            if user1 < user2 {
                (user1.clone(), user2.clone())
            } else {
                (user2.clone(), user1.clone())
            }
        }

        /// EN: Whether `who` is currently platform-muted by governance (an
        /// indefinite mute, or a timed mute not yet expired). Read-only; does not
        /// lazily clean expired entries. CN: `who` 当前是否被治理平台级禁言（无限期，
        /// 或定时禁言未到期）。只读，不顺带清理过期项。
        pub fn is_account_muted(who: &T::AccountId) -> bool {
            match MutedAccounts::<T>::get(who) {
                Some(MuteStatus::Forever) => true,
                Some(MuteStatus::Until(until)) => {
                    frame_system::Pallet::<T>::block_number() < until
                }
                None => false,
            }
        }

        /// EN: Check whether `sender` may chat with `receiver`. Priority:
        /// 1. platform mute (highest-priority deny), 2. valid scene authorization,
        /// 3. receiver privacy level. The on-chain friend graph was removed: the
        /// social "contact" gate (`FriendsOnly`) is now enforced off-chain via
        /// capability tokens, so on-chain a stranger without a scene authorization
        /// is denied (`DeniedRequiresFriend`).
        ///
        /// AUDIT U2 — scene authorizations INTENTIONALLY override `Closed`: a scene
        /// authorization means the two parties share an active transactional context
        /// (order / dispute / market-making), where the counterparty MUST be able to
        /// reach the receiver about that business regardless of the receiver's general
        /// privacy level. This is not a leak: the receiver retains per-scene control
        /// via `rejected_scene_types` — rejecting a `SceneType` filters its
        /// authorizations out here, after which `Closed` (or `FriendsOnly`) applies
        /// normally. So `Closed` blocks all *non-transactional* contact, while
        /// not-yet-rejected transactional scenes still pass.
        ///
        /// CN: 检查 `sender` 是否可与 `receiver` 聊天。优先级：1. 平台禁言（最高优先级拒绝），
        /// 2. 有效场景授权，3. 接收方隐私级别。链上好友图谱已删除：社交「联系人」闸门
        /// （`FriendsOnly`）改由链下能力令牌强制，故链上对无场景授权的陌生人一律拒绝
        /// （`DeniedRequiresFriend`）。
        ///
        /// 审计 U2——场景授权「有意」覆盖 `Closed`：存在场景授权意味着双方处于活跃交易上下文
        /// （订单 / 争议 / 做市），此时对方「必须」能就该业务联系到接收方，与接收方的总体隐私
        /// 级别无关。这并非泄漏：接收方仍可通过 `rejected_scene_types` 做按场景控制——拒绝某个
        /// `SceneType` 后，该类授权会在此被过滤掉，随后正常套用 `Closed`（或 `FriendsOnly`）。
        /// 即 `Closed` 屏蔽一切「非交易」联系，而尚未被拒绝的交易场景仍可放行。
        pub fn check_permission(
            sender: &T::AccountId,
            receiver: &T::AccountId,
        ) -> PermissionResult {
            let current_block = frame_system::Pallet::<T>::block_number();

            // 0. 平台级禁言闸门：被治理禁言的发送方一律拒绝（最高优先级）。
            // Platform mute gate: a governance-muted sender is denied outright.
            if Self::is_account_muted(sender) {
                return PermissionResult::DeniedSenderMuted;
            }

            // 1. 读取接收方设置（用于场景拒绝过滤 + 权限级别）。
            //    审计 P1：链上黑名单已移除——拉黑改由链下能力令牌 / 信箱标签撤销，
            //    故此处不再做 block_list 判定。
            //    Audit P1: the on-chain blocklist was removed; blocking is enforced
            //    off-chain (capability tokens / inbox tag revocation), so there is
            //    no block_list check here.
            let receiver_settings = PrivacySettingsOf::<T>::get(receiver);

            // 2. 检查场景授权
            let (user1, user2) = Self::sorted_pair(sender, receiver);
            let authorizations = SceneAuthorizations::<T>::get(&user1, &user2);

            let valid_scenes: Vec<SceneType> = authorizations
                .iter()
                .filter(|auth| {
                    // 检查是否过期
                    if let Some(expires_at) = auth.expires_at {
                        if current_block > expires_at {
                            return false;
                        }
                    }
                    // 检查是否被接收方拒绝
                    !receiver_settings
                        .rejected_scene_types
                        .contains(&auth.scene_type)
                })
                .map(|auth| auth.scene_type.clone())
                .collect();

            if !valid_scenes.is_empty() {
                return PermissionResult::AllowedByScene(valid_scenes);
            }

            // 3. 根据隐私设置判断。
            //    审计 P1：`Whitelist` 级别的链上白名单已移除，现等同 `FriendsOnly`
            //    ——「是否放行的联系人」由链下能力令牌判定，链上对无场景授权者一律按
            //    需要联系人处理。Audit P1: the on-chain whitelist was removed, so
            //    `Whitelist` now behaves like `FriendsOnly`; the off-chain capability
            //    token decides, and on-chain a scene-less sender requires a contact.
            match receiver_settings.permission_level {
                ChatPermissionLevel::Open => PermissionResult::Allowed,
                ChatPermissionLevel::FriendsOnly | ChatPermissionLevel::Whitelist => {
                    PermissionResult::DeniedRequiresFriend
                }
                ChatPermissionLevel::Closed => PermissionResult::DeniedClosed,
            }
        }

        /// 获取两用户间所有有效的场景授权
        ///
        /// 返回包含过期状态的场景授权信息列表，用于前端展示。
        pub fn get_active_scenes(
            user1: &T::AccountId,
            user2: &T::AccountId,
        ) -> Vec<SceneAuthorizationInfo> {
            let current_block = frame_system::Pallet::<T>::block_number();
            let (u1, u2) = Self::sorted_pair(user1, user2);
            let authorizations = SceneAuthorizations::<T>::get(&u1, &u2);

            authorizations
                .iter()
                .map(|auth| {
                    let is_expired = auth
                        .expires_at
                        .map(|e| current_block > e)
                        .unwrap_or(false);
                    SceneAuthorizationInfo {
                        scene_type: auth.scene_type.clone(),
                        scene_id: auth.scene_id.clone(),
                        is_expired,
                        expires_at: auth.expires_at.map(|b| b.saturated_into::<u64>()),
                        metadata: auth.metadata.to_vec(),
                    }
                })
                .collect()
        }

        /// 清理过期的场景授权
        ///
        /// 移除两个用户之间所有已过期的场景授权。
        /// 可以在适当时机调用以释放存储空间。
        pub fn cleanup_expired_scenes(user1: &T::AccountId, user2: &T::AccountId) {
            let current_block = frame_system::Pallet::<T>::block_number();
            let (u1, u2) = Self::sorted_pair(user1, user2);

            SceneAuthorizations::<T>::mutate(&u1, &u2, |auths| {
                auths.retain(|auth| auth.expires_at.map(|e| current_block <= e).unwrap_or(true));
            });
        }

        /// 获取用户隐私设置摘要
        ///
        /// 返回简化的隐私设置信息，用于前端展示。
        pub fn get_privacy_summary(user: &T::AccountId) -> PrivacySettingsSummary {
            let settings = PrivacySettingsOf::<T>::get(user);
            PrivacySettingsSummary {
                permission_level: settings.permission_level,
                rejected_scene_types: settings.rejected_scene_types.to_vec(),
            }
        }
    }

    // ==================== 实现 SceneAuthorizationManager Trait ====================

    impl<T: Config> SceneAuthorizationManager<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
        /// 授予场景授权（单向）
        fn grant_scene_authorization(
            source: [u8; 8],
            from: &T::AccountId,
            to: &T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
            duration: Option<BlockNumberFor<T>>,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            let current_block = frame_system::Pallet::<T>::block_number();
            // Saturating add hardens against block-number overflow on long durations (audit P2).
            // 饱和加法，防止超长时长导致区块号溢出（审计 P2）。
            let expires_at = duration.map(|d| current_block.saturating_add(d));
            let (user1, user2) = Self::sorted_pair(from, to);

            let bounded_metadata: BoundedVec<u8, ConstU32<128>> =
                metadata.try_into().map_err(|_| Error::<T>::MetadataTooLong)?;

            let authorization = SceneAuthorization {
                scene_type: scene_type.clone(),
                scene_id: scene_id.clone(),
                source_pallet: source,
                granted_at: current_block,
                expires_at,
                metadata: bounded_metadata,
            };

            SceneAuthorizations::<T>::try_mutate(&user1, &user2, |auths| {
                // 检查是否已存在相同场景
                let existing_pos = auths
                    .iter()
                    .position(|a| a.scene_type == scene_type && a.scene_id == scene_id);

                if let Some(pos) = existing_pos {
                    // 更新现有授权
                    auths[pos] = authorization.clone();
                } else {
                    // 添加新授权
                    auths
                        .try_push(authorization)
                        .map_err(|_| Error::<T>::TooManyScenes)?;
                }
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::SceneAuthorizationGranted {
                source,
                user1,
                user2,
                scene_type,
                scene_id,
            });

            Ok(())
        }

        /// 授予双向场景授权
        fn grant_bidirectional_scene_authorization(
            source: [u8; 8],
            user1: &T::AccountId,
            user2: &T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
            duration: Option<BlockNumberFor<T>>,
            metadata: Vec<u8>,
        ) -> DispatchResult {
            // 由于存储已经是双向的（使用排序后的 key），只需调用一次
            Self::grant_scene_authorization(
                source, user1, user2, scene_type, scene_id, duration, metadata,
            )
        }

        /// 撤销特定场景授权
        fn revoke_scene_authorization(
            source: [u8; 8],
            from: &T::AccountId,
            to: &T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
        ) -> DispatchResult {
            let (user1, user2) = Self::sorted_pair(from, to);

            SceneAuthorizations::<T>::try_mutate(&user1, &user2, |auths| {
                let pos = auths
                    .iter()
                    .position(|a| {
                        a.source_pallet == source
                            && a.scene_type == scene_type
                            && a.scene_id == scene_id
                    })
                    .ok_or(Error::<T>::SceneAuthorizationNotFound)?;

                auths.remove(pos);
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::SceneAuthorizationRevoked {
                source,
                user1,
                user2,
                scene_type,
                scene_id,
            });

            Ok(())
        }

        /// 撤销某来源的所有场景授权
        fn revoke_all_by_source(
            source: [u8; 8],
            user1: &T::AccountId,
            user2: &T::AccountId,
        ) -> DispatchResult {
            let (u1, u2) = Self::sorted_pair(user1, user2);

            SceneAuthorizations::<T>::mutate(&u1, &u2, |auths| {
                auths.retain(|a| a.source_pallet != source);
            });

            Ok(())
        }

        /// 延长场景授权有效期
        fn extend_scene_authorization(
            source: [u8; 8],
            from: &T::AccountId,
            to: &T::AccountId,
            scene_type: SceneType,
            scene_id: SceneId,
            additional_duration: BlockNumberFor<T>,
        ) -> DispatchResult {
            let current_block = frame_system::Pallet::<T>::block_number();
            let (user1, user2) = Self::sorted_pair(from, to);

            let mut new_expires_at = None;

            SceneAuthorizations::<T>::try_mutate(&user1, &user2, |auths| {
                let auth = auths
                    .iter_mut()
                    .find(|a| {
                        a.source_pallet == source
                            && a.scene_type == scene_type
                            && a.scene_id == scene_id
                    })
                    .ok_or(Error::<T>::SceneAuthorizationNotFound)?;

                // 从当前时间或原过期时间延长
                // Saturating add hardens against block-number overflow (audit P2).
                // 饱和加法，防止区块号溢出（审计 P2）。
                let base = auth.expires_at.unwrap_or(current_block);
                let new_time = base.max(current_block).saturating_add(additional_duration);
                auth.expires_at = Some(new_time);
                new_expires_at = Some(new_time);

                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::SceneAuthorizationExtended {
                user1,
                user2,
                scene_type,
                scene_id,
                new_expires_at,
            });

            Ok(())
        }

        /// 检查是否有任何有效的场景授权（仅判过期；**不**套用 `rejected_scene_types` /
        /// 禁言 / 隐私级别——非权限门控事实来源，门控请用 `check_permission`）。
        /// Expiry-only check; does NOT apply `rejected_scene_types` / mute / privacy —
        /// not the permission gate (use `check_permission`). See trait doc.
        fn has_any_valid_scene_authorization(from: &T::AccountId, to: &T::AccountId) -> bool {
            let current_block = frame_system::Pallet::<T>::block_number();
            let (user1, user2) = Self::sorted_pair(from, to);
            let authorizations = SceneAuthorizations::<T>::get(&user1, &user2);

            authorizations.iter().any(|auth| {
                auth.expires_at
                    .map(|e| current_block <= e)
                    .unwrap_or(true)
            })
        }

        /// 获取所有有效的场景授权
        fn get_valid_scene_authorizations(
            user1: &T::AccountId,
            user2: &T::AccountId,
        ) -> Vec<SceneAuthorization<BlockNumberFor<T>>> {
            let current_block = frame_system::Pallet::<T>::block_number();
            let (u1, u2) = Self::sorted_pair(user1, user2);
            let authorizations = SceneAuthorizations::<T>::get(&u1, &u2);

            authorizations
                .into_iter()
                .filter(|auth| auth.expires_at.map(|e| current_block <= e).unwrap_or(true))
                .collect()
        }
    }

    // ==================== 实现 ChatPermissionChecker Trait ====================

    impl<T: Config> ChatPermissionChecker<T::AccountId> for Pallet<T> {
        fn can_send_message(sender: &T::AccountId, receiver: &T::AccountId) -> bool {
            Self::check_permission(sender, receiver).is_allowed()
        }
    }
}
