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
        /// 黑名单最大数量
        #[pallet::constant]
        type MaxBlockListSize: Get<u32>;

        /// 白名单最大数量
        #[pallet::constant]
        type MaxWhitelistSize: Get<u32>;

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

        /// 用户已被屏蔽
        UserBlocked {
            blocker: T::AccountId,
            blocked: T::AccountId,
        },

        /// 用户已被解除屏蔽
        UserUnblocked {
            unblocker: T::AccountId,
            unblocked: T::AccountId,
        },

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

        /// 用户添加到白名单
        UserAddedToWhitelist {
            owner: T::AccountId,
            user: T::AccountId,
        },

        /// 用户从白名单移除
        UserRemovedFromWhitelist {
            owner: T::AccountId,
            user: T::AccountId,
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
        /// 黑名单已满
        BlockListFull,

        /// 白名单已满
        WhitelistFull,

        /// 用户已在黑名单中
        AlreadyBlocked,

        /// 用户不在黑名单中
        NotInBlockList,

        /// 不能添加自己
        CannotAddSelf,

        /// 场景授权数量已达上限
        TooManyScenes,

        /// 场景授权不存在
        SceneAuthorizationNotFound,

        /// 场景授权已存在
        SceneAuthorizationAlreadyExists,

        /// 用户已在白名单中
        AlreadyInWhitelist,

        /// 用户不在白名单中
        NotInWhitelist,

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

        /// 添加用户到黑名单
        ///
        /// 被屏蔽的用户将无法向屏蔽者发送消息，
        /// 即使存在有效的场景授权或好友关系。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::block_user())]
        pub fn block_user(origin: OriginFor<T>, user: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(who != user, Error::<T>::CannotAddSelf);

            PrivacySettingsOf::<T>::try_mutate(&who, |settings| {
                ensure!(
                    !settings.block_list.contains(&user),
                    Error::<T>::AlreadyBlocked
                );
                settings
                    .block_list
                    .try_push(user.clone())
                    .map_err(|_| Error::<T>::BlockListFull)?;
                settings.updated_at = frame_system::Pallet::<T>::block_number();
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::UserBlocked {
                blocker: who,
                blocked: user,
            });
            Ok(())
        }

        /// 从黑名单移除用户
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::unblock_user())]
        pub fn unblock_user(origin: OriginFor<T>, user: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;

            PrivacySettingsOf::<T>::try_mutate(&who, |settings| {
                let pos = settings
                    .block_list
                    .iter()
                    .position(|x| x == &user)
                    .ok_or(Error::<T>::NotInBlockList)?;
                settings.block_list.remove(pos);
                settings.updated_at = frame_system::Pallet::<T>::block_number();
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::UserUnblocked {
                unblocker: who,
                unblocked: user,
            });
            Ok(())
        }

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

        /// 添加用户到白名单
        ///
        /// 在 Whitelist 模式下，只有白名单中的用户才能发起聊天。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::add_to_whitelist())]
        pub fn add_to_whitelist(origin: OriginFor<T>, user: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(who != user, Error::<T>::CannotAddSelf);

            PrivacySettingsOf::<T>::try_mutate(&who, |settings| {
                ensure!(
                    !settings.whitelist.contains(&user),
                    Error::<T>::AlreadyInWhitelist
                );
                settings
                    .whitelist
                    .try_push(user.clone())
                    .map_err(|_| Error::<T>::WhitelistFull)?;
                settings.updated_at = frame_system::Pallet::<T>::block_number();
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::UserAddedToWhitelist { owner: who, user });
            Ok(())
        }

        /// 从白名单移除用户
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::remove_from_whitelist())]
        pub fn remove_from_whitelist(origin: OriginFor<T>, user: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;

            PrivacySettingsOf::<T>::try_mutate(&who, |settings| {
                let pos = settings
                    .whitelist
                    .iter()
                    .position(|x| x == &user)
                    .ok_or(Error::<T>::NotInWhitelist)?;
                settings.whitelist.remove(pos);
                settings.updated_at = frame_system::Pallet::<T>::block_number();
                Ok::<_, DispatchError>(())
            })?;

            Self::deposit_event(Event::UserRemovedFromWhitelist { owner: who, user });
            Ok(())
        }

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
        /// 1. platform mute (highest-priority deny), 2. receiver block list,
        /// 3. valid scene authorization, 4. receiver privacy level. The on-chain
        /// friend graph was removed: the social "contact" gate (`FriendsOnly`) is
        /// now enforced off-chain via capability tokens, so on-chain a stranger
        /// without a scene authorization or whitelist entry is denied
        /// (`DeniedRequiresFriend`). CN: 检查 `sender` 是否可与 `receiver` 聊天。
        /// 优先级：1. 平台禁言（最高优先级拒绝），2. 接收方黑名单，3. 有效场景授权，
        /// 4. 接收方隐私级别。链上好友图谱已删除：社交「联系人」闸门（`FriendsOnly`）
        /// 改由链下能力令牌强制，故链上对无场景授权且不在白名单的陌生人一律拒绝
        /// （`DeniedRequiresFriend`）。
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

            // 1. 检查是否被屏蔽
            let receiver_settings = PrivacySettingsOf::<T>::get(receiver);
            if receiver_settings.block_list.contains(sender) {
                return PermissionResult::DeniedBlocked;
            }

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

            // 3. 根据隐私设置判断
            match receiver_settings.permission_level {
                ChatPermissionLevel::Open => PermissionResult::Allowed,
                ChatPermissionLevel::FriendsOnly => PermissionResult::DeniedRequiresFriend,
                ChatPermissionLevel::Whitelist => {
                    if receiver_settings.whitelist.contains(sender) {
                        PermissionResult::Allowed
                    } else {
                        PermissionResult::DeniedNotInWhitelist
                    }
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
                block_list_count: settings.block_list.len() as u32,
                whitelist_count: settings.whitelist.len() as u32,
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
            let expires_at = duration.map(|d| current_block + d);
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
                let base = auth.expires_at.unwrap_or(current_block);
                let new_time = base.max(current_block) + additional_duration;
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

        /// 检查是否有任何有效的场景授权
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
