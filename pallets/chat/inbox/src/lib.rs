//! # 链下投递信箱注册表 Pallet / Off-chain Delivery Inbox Registry Pallet
//!
//! EN: Minimal on-chain anchor for the **Blinded One-Time Delivery Token**
//! protocol (see `pallets/chat/CHAT_OFFCHAIN_DELIVERY_DESIGN.md`). It registers
//! opaque delivery inboxes and publishes, per inbox, the data a relay needs to
//! verify a token *offline from chain state*:
//! - an **inbox-keyed revocation `epoch`** (rotate to invalidate every token), and
//! - a set of **`revoked_tags`** (per-contact targeted revocation).
//!
//! The chain deliberately stores **no Blind-RSA key, no message, and no human
//! relationship**. The issuance key is carried by the sender and self-authenticated
//! via `inbox_id = H(IPK ‖ salt)` (verified by the relay, not the chain), so the
//! chain never performs RSA and the inbox stays unlinkable to any account beyond
//! the throwaway *controller* that registered it.
//!
//! CN: **盲化一次性投递令牌**协议（见 `pallets/chat/CHAT_OFFCHAIN_DELIVERY_DESIGN.md`）
//! 的最小链上锚点。它注册不透明投递信箱，并按信箱公布 relay **离线**验证令牌所需的数据：
//! - **inbox 维度撤销 `epoch`**（轮换即作废所有令牌），与
//! - **`revoked_tags`** 集合（每联系人定向撤销）。
//!
//! 链上刻意**不存储任何 Blind-RSA 密钥、任何消息、任何人际关系**。签发公钥由发送方携带、
//! 经 `inbox_id = H(IPK ‖ salt)` 自验证（由 relay 而非链校验），故链从不做 RSA，且除注册
//! 它的一次性*控制账户*外，信箱对任何账户不可关联。
//!
//! ## 与 `pallet-chat-permission::CapabilityEpoch` 的关系 / Relation to CapabilityEpoch
//!
//! EN: `CapabilityEpoch` is keyed by `AccountId` and serves account-level /
//! compliance revocation of legacy capability tokens. This pallet's epoch is keyed
//! by `inbox_id` precisely so relay verification never needs the receiver's account
//! — the two epochs are orthogonal and intentionally not shared.
//! CN: `CapabilityEpoch` 以 `AccountId` 为键，服务账户级 / 合规层面的旧能力令牌撤销。
//! 本 pallet 的 epoch 以 `inbox_id` 为键，正是为了让 relay 验证永不需要接收方账户——两者
//! 正交、刻意不共用。
//!
//! ## v1 落地取舍 / v1 trade-offs
//!
//! EN: Registration / mutation use a **signed** origin (the controller pays a
//! reserved deposit for anti-spam). This links `inbox_id → controller_account`
//! on-chain. For unlinkability the controller MUST be a throwaway key distinct
//! from the owner's main chat account; relays only ever read inbox-keyed state and
//! never the controller. Account-free (unsigned + inbox-key-signed) registration is
//! a future hardening (see design doc §3 / §10).
//! CN: 注册 / 修改使用**签名**来源（控制账户支付预留押金以反垃圾），这会在链上关联
//! `inbox_id → 控制账户`。为不可关联，控制账户**必须**是与拥有者主聊天账户无关的一次性
//! 密钥；relay 只读 inbox 维度状态、从不读控制账户。账户无关（unsigned + inbox 钥签名）
//! 注册为后续加固项（见设计文档 §3 / §10）。

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod types;
pub mod runtime_api;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub use runtime_api::*;
pub use types::*;
pub use weights::WeightInfo;

use frame_support::traits::{Currency, ReservableCurrency};

/// EN: Balance type of the configured reservable currency.
/// CN: 所配置可预留货币的余额类型。
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::types::{ContactTag, InboxId, InboxRecord};
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// EN: Pallet configuration. CN: Pallet 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// EN: Reservable currency used for the anti-spam registration deposit.
        /// CN: 用于反垃圾注册押金的可预留货币。
        type Currency: ReservableCurrency<Self::AccountId>;

        /// EN: Deposit reserved on `register_inbox`, returned on `deregister_inbox`.
        /// CN: `register_inbox` 预留、`deregister_inbox` 退还的押金。
        #[pallet::constant]
        type InboxDeposit: Get<BalanceOf<Self>>;

        /// EN: Max targeted-revocation tags kept per inbox (cleared on epoch bump).
        /// CN: 每信箱保留的定向撤销标签上限（epoch 递增时清空）。
        #[pallet::constant]
        type MaxRevokedTags: Get<u32>;

        /// EN: Max inboxes a single controller account may register (anti-hoarding,
        /// on top of the per-inbox deposit). CN: 单个控制账户可注册的信箱上限
        /// （在每信箱押金之外的反囤积约束）。
        #[pallet::constant]
        type MaxInboxesPerController: Get<u32>;

        /// EN: Weight info. CN: 权重信息。
        type WeightInfo: WeightInfo;
    }

    // ==================== 存储 / Storage ====================

    /// EN: Registry of delivery inboxes: `inbox_id -> record`. CN: 投递信箱注册表。
    #[pallet::storage]
    #[pallet::getter(fn inboxes)]
    pub type Inboxes<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        InboxId,
        InboxRecord<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, T::MaxRevokedTags>,
        OptionQuery,
    >;

    /// EN: Number of inboxes registered by each controller (bounds against
    /// `MaxInboxesPerController`). CN: 每个控制账户已注册的信箱数（以
    /// `MaxInboxesPerController` 约束）。
    #[pallet::storage]
    pub type InboxCountByController<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    // ==================== 事件 / Events ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// EN: A delivery inbox was registered (epoch starts at 0).
        /// CN: 注册了一个投递信箱（epoch 从 0 开始）。
        InboxRegistered { inbox_id: InboxId, controller: T::AccountId },

        /// EN: An inbox advanced its revocation epoch; all prior tokens are now
        /// stale and `revoked_tags` was cleared. CN: 信箱递增撤销纪元；此前所有令牌
        /// 失效，`revoked_tags` 已清空。
        InboxEpochBumped { inbox_id: InboxId, new_epoch: u32 },

        /// EN: A contact tag was revoked for an inbox (targeted revocation).
        /// CN: 为某信箱撤销了一个联系人标签（定向撤销）。
        ContactTagRevoked { inbox_id: InboxId, tag: ContactTag },

        /// EN: An inbox was deregistered and its deposit returned.
        /// CN: 注销了一个信箱并退还押金。
        InboxDeregistered { inbox_id: InboxId },
    }

    // ==================== 错误 / Errors ====================

    #[pallet::error]
    pub enum Error<T> {
        /// EN: `inbox_id` already registered. CN: `inbox_id` 已注册。
        InboxAlreadyExists,
        /// EN: `inbox_id` not registered. CN: `inbox_id` 未注册。
        InboxNotFound,
        /// EN: Caller is not the inbox controller. CN: 调用者不是信箱控制账户。
        NotController,
        /// EN: Revoked-tag set is full for this epoch (bump epoch to reset).
        /// CN: 本纪元撤销标签集已满（递增 epoch 以重置）。
        TooManyRevokedTags,
        /// EN: Tag is already revoked. CN: 标签已被撤销。
        TagAlreadyRevoked,
        /// EN: Controller reached its inbox cap. CN: 控制账户已达信箱上限。
        TooManyInboxes,
    }

    // ==================== 调用 / Calls ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// EN: Register a new delivery inbox under a client-chosen `inbox_id`
        /// (off-chain bound to `H(IPK)`). Reserves [`Config::InboxDeposit`] from the
        /// caller, who becomes the inbox *controller*. Use a throwaway key for
        /// unlinkability. CN: 以客户端选定的 `inbox_id`（链下绑定 `H(IPK)`）注册新投递
        /// 信箱。从调用者预留 [`Config::InboxDeposit`]，调用者成为信箱*控制账户*。为不可
        /// 关联请使用一次性密钥。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::register_inbox())]
        pub fn register_inbox(origin: OriginFor<T>, inbox_id: InboxId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!Inboxes::<T>::contains_key(inbox_id), Error::<T>::InboxAlreadyExists);

            let count = InboxCountByController::<T>::get(&who);
            ensure!(count < T::MaxInboxesPerController::get(), Error::<T>::TooManyInboxes);

            let deposit = T::InboxDeposit::get();
            T::Currency::reserve(&who, deposit)?;

            let record = InboxRecord {
                controller: who.clone(),
                epoch: 0,
                revoked_tags: BoundedVec::default(),
                deposit,
                created_at: frame_system::Pallet::<T>::block_number(),
            };
            Inboxes::<T>::insert(inbox_id, record);
            InboxCountByController::<T>::insert(&who, count.saturating_add(1));

            Self::deposit_event(Event::InboxRegistered { inbox_id, controller: who });
            Ok(())
        }

        /// EN: Advance the inbox's revocation epoch by one and clear `revoked_tags`.
        /// Invalidates every token previously issued for this inbox (their embedded
        /// epoch no longer matches). CN: 将信箱撤销纪元加一并清空 `revoked_tags`。
        /// 作废此前为此信箱签发的所有令牌（内嵌纪元不再相符）。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::bump_epoch())]
        pub fn bump_epoch(origin: OriginFor<T>, inbox_id: InboxId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let new_epoch = Inboxes::<T>::try_mutate(inbox_id, |maybe| {
                let record = maybe.as_mut().ok_or(Error::<T>::InboxNotFound)?;
                ensure!(record.controller == who, Error::<T>::NotController);
                record.epoch = record.epoch.saturating_add(1);
                record.revoked_tags = BoundedVec::default();
                Ok::<u32, DispatchError>(record.epoch)
            })?;
            Self::deposit_event(Event::InboxEpochBumped { inbox_id, new_epoch });
            Ok(())
        }

        /// EN: Revoke a single contact `tag` for an inbox (targeted revocation): a
        /// relay then rejects tokens carrying this tag, without disturbing other
        /// contacts. CN: 为信箱撤销单个联系人 `tag`（定向撤销）：relay 随后拒绝携带此
        /// 标签的令牌，且不影响其他联系人。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::revoke_tag())]
        pub fn revoke_tag(origin: OriginFor<T>, inbox_id: InboxId, tag: ContactTag) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Inboxes::<T>::try_mutate(inbox_id, |maybe| {
                let record = maybe.as_mut().ok_or(Error::<T>::InboxNotFound)?;
                ensure!(record.controller == who, Error::<T>::NotController);
                ensure!(!record.revoked_tags.contains(&tag), Error::<T>::TagAlreadyRevoked);
                record
                    .revoked_tags
                    .try_push(tag)
                    .map_err(|_| Error::<T>::TooManyRevokedTags)?;
                Ok::<(), DispatchError>(())
            })?;
            Self::deposit_event(Event::ContactTagRevoked { inbox_id, tag });
            Ok(())
        }

        /// EN: Deregister an inbox and return its deposit. Existing tokens become
        /// undeliverable (relays drop unknown inboxes). CN: 注销信箱并退还押金。现有
        /// 令牌随之不可投递（relay 丢弃未知信箱）。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::deregister_inbox())]
        pub fn deregister_inbox(origin: OriginFor<T>, inbox_id: InboxId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let record = Inboxes::<T>::get(inbox_id).ok_or(Error::<T>::InboxNotFound)?;
            ensure!(record.controller == who, Error::<T>::NotController);

            T::Currency::unreserve(&who, record.deposit);
            Inboxes::<T>::remove(inbox_id);
            InboxCountByController::<T>::mutate(&who, |c| *c = c.saturating_sub(1));

            Self::deposit_event(Event::InboxDeregistered { inbox_id });
            Ok(())
        }
    }

    // ==================== 只读辅助 / Read-only helpers ====================

    impl<T: Config> Pallet<T> {
        /// EN: Current inbox-keyed revocation epoch, or `None` if not registered.
        /// CN: 当前 inbox 维度撤销纪元；未注册则为 `None`。
        pub fn inbox_epoch(inbox_id: InboxId) -> Option<u32> {
            Inboxes::<T>::get(inbox_id).map(|r| r.epoch)
        }

        /// EN: Whether `tag` is currently revoked for `inbox_id` (false if the inbox
        /// is unregistered). CN: `tag` 当前是否在 `inbox_id` 下被撤销（信箱未注册则为
        /// false）。
        pub fn is_tag_revoked(inbox_id: InboxId, tag: ContactTag) -> bool {
            Inboxes::<T>::get(inbox_id)
                .map(|r| r.revoked_tags.contains(&tag))
                .unwrap_or(false)
        }

        /// EN: Whether `inbox_id` is registered. CN: `inbox_id` 是否已注册。
        pub fn inbox_exists(inbox_id: InboxId) -> bool {
            Inboxes::<T>::contains_key(inbox_id)
        }
    }
}
