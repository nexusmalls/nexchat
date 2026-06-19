//! # 消息身份预密钥锚 Pallet / Messaging Identity Prekey Anchor Pallet
//!
//! EN: Shared identity base for the chat crypto stacks. It custodies, **per
//! `(AccountId, DeviceId)`**, the X3DH prekey anchors used by the decentralized
//! 1:1 stack (X3DH + Double Ratchet, see `pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`):
//! - `DeviceIdentities` — long-term X25519 identity DH key (`IK`) + account-key endorsement;
//! - `DeviceSignedPreKeys` — mid-term signed prekey (`SPK`) + endorsement;
//! - `DeviceOpkRoots` — Merkle root of the one-time-prekey (`OPK`) set (leaves stay off-chain);
//! - a device-level `prekey_epoch` revocation counter;
//! - `ChatStackCaps` — per-account capability advert for 1:1 stack negotiation (§20).
//!
//! The chain stores **no shared secret, no ratchet state, and no plaintext**; it
//! performs no DH/AEAD. It is the *Authentication Service* anchor only.
//!
//! CN: 聊天密码学栈的共享身份底座。按 **`(AccountId, DeviceId)`** 托管去中心化 1:1 栈
//! （X3DH + 双棘轮，见 `pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`）所需的
//! X3DH 预密钥锚：
//! - `DeviceIdentities`——长期 X25519 身份 DH 钥（`IK`）+ 账户钥背书；
//! - `DeviceSignedPreKeys`——中期签名预密钥（`SPK`）+ 背书；
//! - `DeviceOpkRoots`——一次性预密钥（`OPK`）集合的 Merkle 根（叶子链下）；
//! - 设备级 `prekey_epoch` 撤销计数器；
//! - `ChatStackCaps`——1:1 栈协商用的每账户能力公告（§20）。
//!
//! 链上**不存共享秘密、不存棘轮态、不存明文**，也不做任何 DH/AEAD，仅作*认证服务*锚。
//!
//! ## 背书边界 / Endorsement boundary (v1)
//!
//! EN: On-chain publication is authorized by the **signed origin** — only the account
//! itself can write its `(account, device)` subtree. The stored `*_endorsement`
//! signatures are kept **opaque** so off-chain consumers (peers, relays) can verify
//! *relay-trustlessly* that a DH key belongs to the claimed account (account sr25519
//! signature over `CTX ‖ key`), without trusting any chain-query path. The chain does
//! not re-verify the endorsement against the AccountId (kept generic over `AccountId`).
//! CN: 链上发布由**签名 origin** 授权——只有账户本人能写自己的 `(account, device)` 子树。
//! 所存 `*_endorsement` 签名**不透明**保留，供链下消费方（对端、relay）做 *relay-trustless*
//! 校验（账户 sr25519 钥对 `CTX ‖ key` 的签名），无需信任任何链查询路径。链上不就背书对
//! AccountId 复验（保持对 `AccountId` 泛型）。

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

use frame_support::traits::{Currency, EnsureOrigin, ReservableCurrency};

/// EN: Balance type of the configured reservable currency.
/// CN: 所配置可预留货币的余额类型。
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::types::{
        DeviceId, DeviceIdentity, Endorsement, MerkleRoot, OpkRoot, SignedPreKey, StackCaps,
        X25519Pub,
    };
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use pallet_chat_common::{bump_u32_epoch, next_u32_epoch, reserve_deposit, unreserve_deposit};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// EN: Pallet configuration. CN: Pallet 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// EN: Reservable currency used for the anti-spam per-device deposit.
        /// CN: 用于反垃圾每设备押金的可预留货币。
        type Currency: ReservableCurrency<Self::AccountId>;

        /// EN: Privileged origin allowed to force-unregister a device (governance
        /// recovery / abuse). Refunds the deposit to the owning account.
        /// CN: 可强制注销设备的特权来源（治理回收 / 反滥用）。押金退还给所属账户。
        type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// EN: Deposit reserved per `register_device`, returned on `unregister_device`.
        /// CN: 每次 `register_device` 预留、`unregister_device` 退还的押金。
        #[pallet::constant]
        type DeviceDeposit: Get<BalanceOf<Self>>;

        /// EN: Max devices a single account may register (anti-hoarding).
        /// CN: 单账户可注册的设备上限（反囤积）。
        #[pallet::constant]
        type MaxDevicesPerAccount: Get<u32>;

        /// EN: Weight info. CN: 权重信息。
        type WeightInfo: WeightInfo;
    }

    // ==================== 存储 / Storage ====================

    /// EN: Per-device identity anchor: `(account, device_id) -> DeviceIdentity`.
    /// CN: 每设备身份锚。
    #[pallet::storage]
    pub type DeviceIdentities<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        DeviceId,
        DeviceIdentity<BalanceOf<T>, BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// EN: Per-device signed prekey: `(account, device_id) -> SignedPreKey`.
    /// CN: 每设备签名预密钥。
    #[pallet::storage]
    pub type DeviceSignedPreKeys<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        DeviceId,
        SignedPreKey<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// EN: Per-device OPK Merkle root: `(account, device_id) -> OpkRoot`.
    /// CN: 每设备 OPK Merkle 根。
    #[pallet::storage]
    pub type DeviceOpkRoots<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        Blake2_128Concat,
        DeviceId,
        OpkRoot,
        OptionQuery,
    >;

    /// EN: Number of devices registered by each account (bounds `MaxDevicesPerAccount`).
    /// CN: 每账户已注册设备数（约束 `MaxDevicesPerAccount`）。
    #[pallet::storage]
    pub type DeviceCountByAccount<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// EN: Per-account 1:1 chat-stack capability advert (negotiation, §20).
    /// CN: 每账户 1:1 聊天栈能力公告（协商，§20）。
    #[pallet::storage]
    pub type ChatStackCaps<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, StackCaps, OptionQuery>;

    // ==================== 事件 / Events ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// EN: A device identity (`IK`) was registered. CN: 注册了设备身份（`IK`）。
        DeviceRegistered { account: T::AccountId, device_id: DeviceId },
        /// EN: A device's signed prekey was set/rotated. CN: 设置/轮换了设备签名预密钥。
        SignedPreKeySet { account: T::AccountId, device_id: DeviceId },
        /// EN: A device's OPK Merkle root was published/updated (with new epoch).
        /// CN: 发布/更新了设备 OPK Merkle 根（携新纪元）。
        OpkRootSet { account: T::AccountId, device_id: DeviceId, epoch: u32, count: u32 },
        /// EN: A device's prekey epoch was bumped (revocation). CN: 设备预密钥纪元递增（撤销）。
        PrekeyEpochBumped { account: T::AccountId, device_id: DeviceId, new_epoch: u32 },
        /// EN: A device identity was unregistered and its deposit returned.
        /// CN: 注销了设备身份并退还押金。
        DeviceUnregistered { account: T::AccountId, device_id: DeviceId },
        /// EN: A device was force-unregistered by the privileged origin (deposit refunded).
        /// CN: 设备被特权来源强制注销（押金已退）。
        DeviceForceUnregistered { account: T::AccountId, device_id: DeviceId },
        /// EN: An account's 1:1 stack capabilities were set. CN: 设置了账户 1:1 栈能力。
        StackCapsSet { account: T::AccountId, flags: u8, version: u16 },
    }

    // ==================== 错误 / Errors ====================

    #[pallet::error]
    pub enum Error<T> {
        /// EN: Device already registered for this account. CN: 该账户的设备已注册。
        DeviceAlreadyExists,
        /// EN: Device not registered for this account. CN: 该账户的设备未注册。
        DeviceNotFound,
        /// EN: `device_id != blake2_128(ik)` (device id must self-certify the IK).
        /// CN: `device_id != blake2_128(ik)`（设备 id 须自证 IK）。
        DeviceIdMismatch,
        /// EN: Account reached its device cap. CN: 账户已达设备上限。
        TooManyDevices,
        /// EN: OPK `count` must be non-zero on publication. CN: 发布时 OPK `count` 不可为 0。
        EmptyOpkSet,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_runtime_upgrade() -> Weight {
            let on_chain = <Pallet<T> as frame_support::traits::GetStorageVersion>::on_chain_storage_version();
            if on_chain < STORAGE_VERSION {
                STORAGE_VERSION.put::<Pallet<T>>();
                T::DbWeight::get().writes(1)
            } else {
                Weight::zero()
            }
        }
    }

    // ==================== 调用 / Calls ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// EN: Register a device's long-term X25519 identity key (`IK`). `device_id`
        /// MUST equal `blake2_128(ik)` (self-certifying). Reserves
        /// [`Config::DeviceDeposit`]. The caller (signed origin) is the owning account.
        /// CN: 注册设备的长期 X25519 身份钥（`IK`）。`device_id` 必须等于 `blake2_128(ik)`
        /// （自证）。预留 [`Config::DeviceDeposit`]。调用者（签名 origin）即所属账户。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::register_device())]
        pub fn register_device(
            origin: OriginFor<T>,
            device_id: DeviceId,
            ik: X25519Pub,
            ik_endorsement: Endorsement,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                !DeviceIdentities::<T>::contains_key(&who, device_id),
                Error::<T>::DeviceAlreadyExists
            );
            // 自证：device_id 必须由 IK 决定，杜绝伪造设备路由。
            // Self-certify: device_id must be derived from IK, preventing spoofed routing.
            ensure!(
                device_id == sp_io::hashing::blake2_128(&ik),
                Error::<T>::DeviceIdMismatch
            );

            let count = DeviceCountByAccount::<T>::get(&who);
            ensure!(count < T::MaxDevicesPerAccount::get(), Error::<T>::TooManyDevices);

            let deposit = T::DeviceDeposit::get();
            reserve_deposit::<T::Currency, _, _>(&who, deposit)?;

            DeviceIdentities::<T>::insert(
                &who,
                device_id,
                DeviceIdentity {
                    ik,
                    ik_endorsement,
                    prekey_epoch: 0,
                    deposit,
                    registered_at: frame_system::Pallet::<T>::block_number(),
                },
            );
            DeviceCountByAccount::<T>::insert(&who, count.saturating_add(1));

            Self::deposit_event(Event::DeviceRegistered { account: who, device_id });
            Ok(())
        }

        /// EN: Set or rotate a device's signed prekey (`SPK`). Device must exist.
        /// CN: 设置或轮换设备签名预密钥（`SPK`）。设备须已存在。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_signed_prekey())]
        pub fn set_signed_prekey(
            origin: OriginFor<T>,
            device_id: DeviceId,
            spk: X25519Pub,
            spk_endorsement: Endorsement,
            valid_until: BlockNumberFor<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                DeviceIdentities::<T>::contains_key(&who, device_id),
                Error::<T>::DeviceNotFound
            );
            DeviceSignedPreKeys::<T>::insert(
                &who,
                device_id,
                SignedPreKey {
                    spk,
                    spk_endorsement,
                    valid_until,
                    updated_at: frame_system::Pallet::<T>::block_number(),
                },
            );
            Self::deposit_event(Event::SignedPreKeySet { account: who, device_id });
            Ok(())
        }

        /// EN: Publish/update a device's OPK Merkle root (leaves distributed off-chain).
        /// Each call bumps the publication epoch. CN: 发布/更新设备 OPK Merkle 根（叶子
        /// 链下分发）。每次调用递增发布纪元。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::set_opk_root())]
        pub fn set_opk_root(
            origin: OriginFor<T>,
            device_id: DeviceId,
            root: MerkleRoot,
            count: u32,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(
                DeviceIdentities::<T>::contains_key(&who, device_id),
                Error::<T>::DeviceNotFound
            );
            ensure!(count > 0, Error::<T>::EmptyOpkSet);
            let epoch = DeviceOpkRoots::<T>::get(&who, device_id)
                .map(|r| next_u32_epoch(r.epoch))
                .unwrap_or(0);
            DeviceOpkRoots::<T>::insert(&who, device_id, OpkRoot { root, count, epoch });
            Self::deposit_event(Event::OpkRootSet { account: who, device_id, epoch, count });
            Ok(())
        }

        /// EN: Bump a device's prekey revocation epoch (invalidates its prior bundle).
        /// CN: 递增设备预密钥撤销纪元（作废其此前的预密钥包）。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::bump_prekey_epoch())]
        pub fn bump_prekey_epoch(origin: OriginFor<T>, device_id: DeviceId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let new_epoch = DeviceIdentities::<T>::try_mutate(&who, device_id, |maybe| {
                let rec = maybe.as_mut().ok_or(Error::<T>::DeviceNotFound)?;
                Ok::<u32, DispatchError>(bump_u32_epoch(&mut rec.prekey_epoch))
            })?;
            Self::deposit_event(Event::PrekeyEpochBumped {
                account: who,
                device_id,
                new_epoch,
            });
            Ok(())
        }

        /// EN: Unregister a device, clear its prekey state, and return the deposit.
        /// CN: 注销设备、清空其预密钥状态并退还押金。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::unregister_device())]
        pub fn unregister_device(origin: OriginFor<T>, device_id: DeviceId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let rec = DeviceIdentities::<T>::get(&who, device_id)
                .ok_or(Error::<T>::DeviceNotFound)?;
            unreserve_deposit::<T::Currency, _, _>(&who, rec.deposit);
            Self::purge_device(&who, device_id);
            Self::deposit_event(Event::DeviceUnregistered { account: who, device_id });
            Ok(())
        }

        /// EN: Set this account's 1:1 chat-stack capabilities (negotiation advert, §20).
        /// CN: 设置本账户 1:1 聊天栈能力（协商公告，§20）。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::set_stack_caps())]
        pub fn set_stack_caps(origin: OriginFor<T>, flags: u8, version: u16) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ChatStackCaps::<T>::insert(&who, StackCaps { flags, version });
            Self::deposit_event(Event::StackCapsSet { account: who, flags, version });
            Ok(())
        }

        /// EN: Force-unregister a device via the privileged [`Config::ForceOrigin`]
        /// (governance recovery / abuse); refunds the deposit to the owning account.
        /// CN: 经特权 [`Config::ForceOrigin`] 强制注销设备（治理回收 / 反滥用）；押金退还
        /// 给所属账户。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::force_unregister_device())]
        pub fn force_unregister_device(
            origin: OriginFor<T>,
            account: T::AccountId,
            device_id: DeviceId,
        ) -> DispatchResult {
            T::ForceOrigin::ensure_origin(origin)?;
            let rec = DeviceIdentities::<T>::get(&account, device_id)
                .ok_or(Error::<T>::DeviceNotFound)?;
            unreserve_deposit::<T::Currency, _, _>(&account, rec.deposit);
            Self::purge_device(&account, device_id);
            Self::deposit_event(Event::DeviceForceUnregistered { account, device_id });
            Ok(())
        }
    }

    // ==================== 内部辅助 / Internal helpers ====================

    impl<T: Config> Pallet<T> {
        /// EN: Remove all per-device state and decrement the account's device count.
        /// CN: 移除设备全部状态并递减账户设备计数。
        fn purge_device(account: &T::AccountId, device_id: DeviceId) {
            DeviceIdentities::<T>::remove(account, device_id);
            DeviceSignedPreKeys::<T>::remove(account, device_id);
            DeviceOpkRoots::<T>::remove(account, device_id);
            DeviceCountByAccount::<T>::mutate(account, |c| *c = c.saturating_sub(1));
        }
    }

    // ==================== 只读辅助 / Read-only helpers ====================

    impl<T: Config> Pallet<T> {
        /// EN: A device's IK + current prekey epoch, or `None` if not registered.
        /// CN: 设备的 IK + 当前预密钥纪元；未注册则为 `None`。
        pub fn device_ik(account: &T::AccountId, device_id: DeviceId) -> Option<(X25519Pub, u32)> {
            DeviceIdentities::<T>::get(account, device_id).map(|r| (r.ik, r.prekey_epoch))
        }

        /// EN: A device's signed prekey + advisory expiry, or `None`.
        /// CN: 设备的签名预密钥 + 建议过期；否则 `None`。
        pub fn device_spk(
            account: &T::AccountId,
            device_id: DeviceId,
        ) -> Option<(X25519Pub, BlockNumberFor<T>)> {
            DeviceSignedPreKeys::<T>::get(account, device_id).map(|r| (r.spk, r.valid_until))
        }

        /// EN: A device's OPK root `(root, count, epoch)`, or `None`. CN: 设备 OPK 根。
        pub fn device_opk_root(
            account: &T::AccountId,
            device_id: DeviceId,
        ) -> Option<(MerkleRoot, u32, u32)> {
            DeviceOpkRoots::<T>::get(account, device_id).map(|r| (r.root, r.count, r.epoch))
        }

        /// EN: An account's 1:1 stack capabilities `(flags, version)`, or `None`.
        /// CN: 账户 1:1 栈能力 `(flags, version)`；否则 `None`。
        pub fn stack_caps(account: &T::AccountId) -> Option<(u8, u16)> {
            ChatStackCaps::<T>::get(account).map(|c| (c.flags, c.version))
        }

        /// EN: Whether `(account, device_id)` is registered. CN: 设备是否已注册。
        pub fn device_exists(account: &T::AccountId, device_id: DeviceId) -> bool {
            DeviceIdentities::<T>::contains_key(account, device_id)
        }
    }
}
