//! # 聊天权限系统 Pallet
//!
//! 实现基于场景的聊天权限控制系统，支持同一聊天会话应用于多个业务场景。
//!
//! ## 概述
//!
//! 本模块提供以下功能：
//! - 用户隐私设置管理（权限级别、黑白名单）
//! - 好友关系管理
//! - 场景授权管理（多场景共存）
//! - 聊天权限检查
//!
//! ## 核心概念
//!
//! - **聊天会话**: 两个用户之间的通信通道，唯一
//! - **场景授权**: 为什么这两个用户可以聊天的原因，可以有多个
//! - **权限判定**: 黑名单 → 好友 → 场景授权 → 隐私设置
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

        /// EN: Max pending *incoming* friend requests per account (anti-spam bound).
        /// CN: 单账户最大待处理**收件**好友申请数（防刷上限）。
        #[pallet::constant]
        type MaxFriendRequests: Get<u32>;

        /// EN: Max length (bytes) of the optional greeting attached to a friend
        /// request. CN: 好友申请可选附言（验证消息）的字节上限。
        #[pallet::constant]
        type MaxFriendRequestMsgLen: Get<u32>;

        /// EN: Max length (bytes) of a per-friend remark (alias/note).
        /// CN: 单个好友备注（别名/备忘）的字节上限。
        #[pallet::constant]
        type MaxFriendRemarkLen: Get<u32>;

        /// EN: Max length (bytes) of a per-friend group/category label.
        /// CN: 单个好友分组（标签）的字节上限。
        #[pallet::constant]
        type MaxFriendGroupLen: Get<u32>;

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

    /// 好友关系存储
    ///
    /// 双向存储好友关系，值为建立好友关系的区块号。
    /// 查询时需要检查双向是否都存在。
    #[pallet::storage]
    #[pallet::getter(fn friendships)]
    pub type Friendships<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        T::AccountId,
        BlockNumberFor<T>,
        OptionQuery,
    >;

    /// EN: Pending friend requests, keyed by (target, requester) so a user's
    /// **incoming** requests are an efficient prefix scan. Value = requested block.
    /// CN: 待处理好友申请，按 (接收方, 发起方) 存储，使某用户的**收件**申请可前缀扫描。
    /// 值为申请区块号。好友关系只能经此「申请 → 同意」握手建立（修复旧版
    /// `add_friend` 单方面即建立双向好友、绕过 `FriendsOnly` 隐私闸门的缺口）。
    #[pallet::storage]
    #[pallet::getter(fn friend_requests)]
    pub type FriendRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId, // target / 接收方
        Blake2_128Concat,
        T::AccountId, // requester / 发起方
        BlockNumberFor<T>,
        OptionQuery,
    >;

    /// EN: Count of pending incoming friend requests per account (bounds `FriendRequests`).
    /// CN: 单账户待处理收件好友申请计数（约束 `FriendRequests`，防刷）。
    #[pallet::storage]
    #[pallet::getter(fn incoming_friend_request_count)]
    pub type IncomingFriendRequestCount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// EN: Optional greeting ("verification message") attached to a pending
    /// friend request, keyed identically to `FriendRequests` (target, requester).
    /// Cleared together with the request on accept / reject / cancel / mutual
    /// fast-path. CN: 待处理好友申请附带的可选附言（验证消息），键与 `FriendRequests`
    /// 一致（接收方, 发起方）。在同意 / 拒绝 / 撤回 / 双向快捷路径时随申请一并清除。
    #[pallet::storage]
    pub type FriendRequestMsg<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId, // target / 接收方
        Blake2_128Concat,
        T::AccountId, // requester / 发起方
        BoundedVec<u8, T::MaxFriendRequestMsgLen>,
        OptionQuery,
    >;

    /// EN: Per-owner remark (alias/note) for a friend: `(owner, friend) -> bytes`.
    /// Private to `owner`; cleared when the friendship is removed. CN: 好友备注
    /// （别名/备忘），`(拥有者, 好友) -> 字节`，仅属于 `owner`；解除好友时清除。
    #[pallet::storage]
    pub type FriendRemark<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId, // owner / 拥有者
        Blake2_128Concat,
        T::AccountId, // friend / 好友
        BoundedVec<u8, T::MaxFriendRemarkLen>,
        OptionQuery,
    >;

    /// EN: Per-owner group/category label for a friend: `(owner, friend) -> bytes`.
    /// Lightweight single-tag grouping (a friend belongs to at most one label);
    /// cleared when the friendship is removed. CN: 好友分组（标签），`(拥有者, 好友)
    /// -> 字节`，轻量单标签分组（一个好友至多属于一个分组）；解除好友时清除。
    #[pallet::storage]
    pub type FriendGroupTag<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId, // owner / 拥有者
        Blake2_128Concat,
        T::AccountId, // friend / 好友
        BoundedVec<u8, T::MaxFriendGroupLen>,
        OptionQuery,
    >;

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

        /// 好友关系已建立
        FriendshipCreated {
            user1: T::AccountId,
            user2: T::AccountId,
        },

        /// 好友关系已解除
        FriendshipRemoved {
            user1: T::AccountId,
            user2: T::AccountId,
        },

        /// EN: A friend request was sent (awaiting the target's consent).
        /// CN: 已发送好友申请（等待接收方同意）。
        FriendRequestSent {
            from: T::AccountId,
            to: T::AccountId,
        },

        /// EN: The target accepted; a bidirectional friendship is now established.
        /// CN: 接收方已同意，双向好友关系建立。
        FriendRequestAccepted {
            requester: T::AccountId,
            target: T::AccountId,
        },

        /// EN: The target rejected the request.
        /// CN: 接收方已拒绝该申请。
        FriendRequestRejected {
            requester: T::AccountId,
            target: T::AccountId,
        },

        /// EN: The requester withdrew their own pending request.
        /// CN: 发起方撤回了自己的待处理申请。
        FriendRequestCancelled {
            from: T::AccountId,
            to: T::AccountId,
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

        /// EN: A friend remark/group label was updated (or cleared) by `who`.
        /// CN: `who` 更新（或清除）了对某好友的备注/分组。
        FriendMetaUpdated {
            who: T::AccountId,
            friend: T::AccountId,
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

        /// 好友关系已存在
        FriendshipAlreadyExists,

        /// 好友关系不存在
        FriendshipNotFound,

        /// EN: A pending friend request from you to this target already exists.
        /// CN: 你已有一条发往该接收方的待处理好友申请。
        FriendRequestAlreadyExists,

        /// EN: No matching pending friend request was found.
        /// CN: 未找到匹配的待处理好友申请。
        FriendRequestNotFound,

        /// EN: The target has too many pending incoming requests (anti-spam bound hit).
        /// CN: 接收方待处理收件申请已达上限（防刷）。
        TooManyFriendRequests,

        /// EN: The target has blocked you; the request is refused.
        /// CN: 你已被接收方拉黑，申请被拒。
        BlockedByTarget,

        /// EN: The target is not accepting friend requests (privacy level = Closed).
        /// CN: 接收方未开放好友申请（隐私级别为 Closed）。
        RequestsNotAccepted,

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

        // NOTE / 注意：`call_index(4)` 原为 `add_friend`（单方面即建立双向好友），
        // 因其可被任意账户用来**绕过对方 `FriendsOnly`/隐私闸门**而移除（审计：单向授权缺口）。
        // 好友关系现在只能经 `request_friend` → `accept_friend` 的双方同意握手建立。
        // 索引 4 刻意留空，避免复用造成的语义混淆。
        // `add_friend` (unilateral) was REMOVED: it let anyone bypass the target's
        // `FriendsOnly` privacy gate. Friendships now require mutual consent via
        // `request_friend` → `accept_friend`. Index 4 is left vacant on purpose.

        /// EN: Send a friend request to `target` (awaits the target's consent).
        /// If `target` has already sent you a request, the friendship is
        /// established immediately (mutual request fast-path).
        /// CN: 向 `target` 发送好友申请（等待对方同意）。若对方此前已向你发过申请，
        /// 则立即建立好友关系（双向申请快捷路径）。
        ///
        /// 拒绝条件：自己、已是好友、已有同向待处理申请、被对方拉黑、对方 `Closed`、
        /// 或对方收件申请已达上限。
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::request_friend())]
        pub fn request_friend(
            origin: OriginFor<T>,
            target: T::AccountId,
            message: Option<Vec<u8>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(who != target, Error::<T>::CannotAddSelf);
            ensure!(
                Friendships::<T>::get(&who, &target).is_none(),
                Error::<T>::FriendshipAlreadyExists
            );

            // 提前校验附言长度，避免无效写入 / validate greeting length up-front.
            let greeting: Option<BoundedVec<u8, T::MaxFriendRequestMsgLen>> = match message {
                Some(m) => Some(m.try_into().map_err(|_| Error::<T>::MetadataTooLong)?),
                None => None,
            };

            // 双向申请快捷路径：对方此前已向我发过申请（key = (me=target, requester=other)）。
            // Mutual fast-path: target already requested me earlier.
            if FriendRequests::<T>::take(&who, &target).is_some() {
                Self::decrement_incoming_requests(&who);
                // 申请被立即消费，连带清理其附言 / request consumed now; clear its greeting.
                FriendRequestMsg::<T>::remove(&who, &target);
                Self::establish_friendship(&target, &who);
                Self::deposit_event(Event::FriendRequestAccepted {
                    requester: target,
                    target: who,
                });
                return Ok(());
            }

            // 常规路径：检查我是否已向对方发过、是否被拉黑 / 对方是否关闭、上限。
            ensure!(
                FriendRequests::<T>::get(&target, &who).is_none(),
                Error::<T>::FriendRequestAlreadyExists
            );

            let target_settings = PrivacySettingsOf::<T>::get(&target);
            ensure!(
                !target_settings.block_list.contains(&who),
                Error::<T>::BlockedByTarget
            );
            ensure!(
                target_settings.permission_level != ChatPermissionLevel::Closed,
                Error::<T>::RequestsNotAccepted
            );

            let cnt = IncomingFriendRequestCount::<T>::get(&target);
            ensure!(cnt < T::MaxFriendRequests::get(), Error::<T>::TooManyFriendRequests);

            let now = frame_system::Pallet::<T>::block_number();
            FriendRequests::<T>::insert(&target, &who, now);
            if let Some(greeting) = greeting {
                FriendRequestMsg::<T>::insert(&target, &who, greeting);
            }
            IncomingFriendRequestCount::<T>::insert(&target, cnt.saturating_add(1));

            Self::deposit_event(Event::FriendRequestSent { from: who, to: target });
            Ok(())
        }

        /// EN: Accept a pending friend request from `requester`, establishing a
        /// bidirectional friendship. Caller is the target of that request.
        /// CN: 同意来自 `requester` 的好友申请，建立双向好友关系。调用者为该申请的接收方。
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::accept_friend())]
        pub fn accept_friend(origin: OriginFor<T>, requester: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                FriendRequests::<T>::take(&who, &requester).is_some(),
                Error::<T>::FriendRequestNotFound
            );
            Self::decrement_incoming_requests(&who);
            FriendRequestMsg::<T>::remove(&who, &requester);
            Self::establish_friendship(&requester, &who);

            Self::deposit_event(Event::FriendRequestAccepted {
                requester,
                target: who,
            });
            Ok(())
        }

        /// EN: Reject a pending friend request from `requester` (caller is target).
        /// CN: 拒绝来自 `requester` 的好友申请（调用者为接收方）。
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::reject_friend())]
        pub fn reject_friend(origin: OriginFor<T>, requester: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                FriendRequests::<T>::take(&who, &requester).is_some(),
                Error::<T>::FriendRequestNotFound
            );
            Self::decrement_incoming_requests(&who);
            FriendRequestMsg::<T>::remove(&who, &requester);

            Self::deposit_event(Event::FriendRequestRejected {
                requester,
                target: who,
            });
            Ok(())
        }

        /// EN: Withdraw your own pending request previously sent to `target`.
        /// CN: 撤回自己此前发往 `target` 的待处理申请。
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::cancel_friend_request())]
        pub fn cancel_friend_request(origin: OriginFor<T>, target: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                FriendRequests::<T>::take(&target, &who).is_some(),
                Error::<T>::FriendRequestNotFound
            );
            Self::decrement_incoming_requests(&target);
            FriendRequestMsg::<T>::remove(&target, &who);

            Self::deposit_event(Event::FriendRequestCancelled { from: who, to: target });
            Ok(())
        }

        /// 删除好友
        ///
        /// 解除双向好友关系。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::remove_friend())]
        pub fn remove_friend(origin: OriginFor<T>, friend: T::AccountId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                Friendships::<T>::get(&who, &friend).is_some(),
                Error::<T>::FriendshipNotFound
            );

            // 双向移除好友关系
            Friendships::<T>::remove(&who, &friend);
            Friendships::<T>::remove(&friend, &who);
            // 清理双方对彼此的备注/分组，避免悬挂元数据。
            // Clear both sides' remark/group for each other to avoid dangling meta.
            FriendRemark::<T>::remove(&who, &friend);
            FriendRemark::<T>::remove(&friend, &who);
            FriendGroupTag::<T>::remove(&who, &friend);
            FriendGroupTag::<T>::remove(&friend, &who);

            Self::deposit_event(Event::FriendshipRemoved {
                user1: who,
                user2: friend,
            });
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

        /// EN: Set (or clear with `None`) the caller's private remark and/or group
        /// label for an existing `friend`. Both fields are independent: pass
        /// `Some(bytes)` to set, `None` to leave a field unchanged is NOT the
        /// semantics — `None` CLEARS that field. Caller must already be friends
        /// with `friend`. CN: 设置（或以 `None` 清除）调用者对现有 `friend` 的私有
        /// 备注与/或分组标签。两字段独立：`Some(bytes)` 为设置，`None` 为**清除**该字段
        /// （非"保持不变"）。调用者须已是该好友。
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::set_friend_meta())]
        pub fn set_friend_meta(
            origin: OriginFor<T>,
            friend: T::AccountId,
            remark: Option<Vec<u8>>,
            group: Option<Vec<u8>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                Friendships::<T>::get(&who, &friend).is_some(),
                Error::<T>::FriendshipNotFound
            );

            match remark {
                Some(r) => {
                    let bounded: BoundedVec<u8, T::MaxFriendRemarkLen> =
                        r.try_into().map_err(|_| Error::<T>::MetadataTooLong)?;
                    FriendRemark::<T>::insert(&who, &friend, bounded);
                }
                None => FriendRemark::<T>::remove(&who, &friend),
            }
            match group {
                Some(g) => {
                    let bounded: BoundedVec<u8, T::MaxFriendGroupLen> =
                        g.try_into().map_err(|_| Error::<T>::MetadataTooLong)?;
                    FriendGroupTag::<T>::insert(&who, &friend, bounded);
                }
                None => FriendGroupTag::<T>::remove(&who, &friend),
            }

            Self::deposit_event(Event::FriendMetaUpdated { who, friend });
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

        /// EN: Establish a bidirectional friendship and emit `FriendshipCreated`.
        /// Idempotent on the storage side (re-insert overwrites the block number).
        /// CN: 建立双向好友关系并发出 `FriendshipCreated`（重复写入仅覆盖区块号）。
        fn establish_friendship(user1: &T::AccountId, user2: &T::AccountId) {
            let now = frame_system::Pallet::<T>::block_number();
            Friendships::<T>::insert(user1, user2, now);
            Friendships::<T>::insert(user2, user1, now);
            Self::deposit_event(Event::FriendshipCreated {
                user1: user1.clone(),
                user2: user2.clone(),
            });
        }

        /// EN: Saturating-decrement the target's pending incoming-request counter.
        /// CN: 对接收方的待处理收件申请计数做饱和减一。
        fn decrement_incoming_requests(target: &T::AccountId) {
            IncomingFriendRequestCount::<T>::mutate(target, |c| *c = c.saturating_sub(1));
        }

        /// EN: List all friends of `who` (prefix scan over `Friendships`).
        /// CN: 列出 `who` 的所有好友（对 `Friendships` 做前缀扫描）。
        pub fn list_friends(who: &T::AccountId) -> Vec<T::AccountId> {
            Friendships::<T>::iter_prefix(who).map(|(other, _)| other).collect()
        }

        /// EN: List accounts that have a pending friend request awaiting `who`'s
        /// consent (efficient prefix scan, since `FriendRequests` is keyed by target).
        /// CN: 列出待 `who` 处理（同意/拒绝）的好友申请发起方（前缀扫描，键以接收方在前）。
        pub fn list_incoming_friend_requests(who: &T::AccountId) -> Vec<T::AccountId> {
            FriendRequests::<T>::iter_prefix(who).map(|(requester, _)| requester).collect()
        }

        /// EN: Like `list_incoming_friend_requests`, but each entry also carries the
        /// optional greeting bytes attached to the request (empty when none).
        /// CN: 同 `list_incoming_friend_requests`，但每项附带该申请的可选附言字节（无则为空）。
        pub fn list_incoming_friend_requests_detailed(
            who: &T::AccountId,
        ) -> Vec<(T::AccountId, Vec<u8>)> {
            FriendRequests::<T>::iter_prefix(who)
                .map(|(requester, _)| {
                    let msg = FriendRequestMsg::<T>::get(who, &requester)
                        .map(|m| m.into_inner())
                        .unwrap_or_default();
                    (requester, msg)
                })
                .collect()
        }

        /// EN: Read `owner`'s private `(remark, group)` labels for `friend`
        /// (empty bytes when unset). CN: 读取 `owner` 对 `friend` 的私有
        /// `(备注, 分组)` 标签（未设置则为空字节）。
        pub fn get_friend_meta(
            owner: &T::AccountId,
            friend: &T::AccountId,
        ) -> (Vec<u8>, Vec<u8>) {
            let remark = FriendRemark::<T>::get(owner, friend)
                .map(|m| m.into_inner())
                .unwrap_or_default();
            let group = FriendGroupTag::<T>::get(owner, friend)
                .map(|m| m.into_inner())
                .unwrap_or_default();
            (remark, group)
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

        /// 检查聊天权限
        ///
        /// 按以下优先级检查权限：
        /// 1. 黑名单检查（最高优先级拒绝）
        /// 2. 好友关系检查
        /// 3. 场景授权检查
        /// 4. 隐私设置检查
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

            // 2. 检查好友关系
            if Friendships::<T>::get(sender, receiver).is_some() {
                return PermissionResult::AllowedByFriendship;
            }

            // 3. 检查场景授权
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

            // 4. 根据隐私设置判断
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

    // ==================== 实现 FriendshipChecker Trait ====================

    impl<T: Config> FriendshipChecker<T::AccountId> for Pallet<T> {
        fn is_friend(user1: &T::AccountId, user2: &T::AccountId) -> bool {
            Friendships::<T>::get(user1, user2).is_some()
        }
    }
}
