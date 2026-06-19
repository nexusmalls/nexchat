//! # 账户派生加密同步锚 Pallet / Account-derived Encrypted Sync Anchor (EISA) Pallet
//!
//! EN: Layer C of the chat sync/recovery design (the account-derived encrypted sync
//! anchor, EISA). Stores, per opaque `anchor_id = blake2_256(anchor_pk)`, one client-encrypted
//! `SyncManifest` (CIDs of the user's conv-index / contacts-vault / msg-archive blobs).
//! Properties:
//! - **Key is mnemonic-recomputable**: `anchor_pk` derives deterministically from the
//!   client's `vault_master`, so a brand-new device can locate its anchor with zero
//!   external dependencies (the v1 `InboxId` keying could not).
//! - **Authorization ≠ payment**: every mutation must carry an Ed25519 `anchor_sig`
//!   by the anchor key over a domain-separated payload (context ‖ genesis_hash ‖
//!   anchor_id ‖ updated_at LE ‖ blake2_256(ciphertext)); the signed origin only pays
//!   fees and the anti-spam deposit. Swapping the payer (proxy / throwaway / sponsored)
//!   later requires zero storage migration.
//! - **The chain stores ciphertext only** — no plaintext CID, no decryption, no inbox
//!   coupling; LWW by `updated_at` with an upper-bound clock-skew guard so a stolen or
//!   buggy client cannot self-lock the anchor at `u64::MAX`.
//!
//! CN: 聊天同步/恢复设计（账户派生加密同步锚 EISA）的 C 层。以不透明
//! `anchor_id = blake2_256(anchor_pk)` 为键，存储一份客户端加密的 `SyncManifest`
//! （用户 conv-index / contacts-vault / msg-archive blob 的 CID）。特性：
//! - **键可凭助记词重算**：`anchor_pk` 由客户端 `vault_master` 确定性派生，新设备零外部
//!   依赖即可定位自己的锚（v1 草案 `InboxId` 键控做不到）。
//! - **授权与付费分离**：每次变更必须携带锚密钥对域分离 payload（context ‖ genesis_hash ‖
//!   anchor_id ‖ updated_at LE ‖ blake2_256(ciphertext)）的 Ed25519 `anchor_sig`；签名
//!   origin 仅支付手续费与反垃圾押金。后续更换付费方（proxy / 一次性账户 / 赞助）无需任何
//!   存储迁移。
//! - **链上只存密文**——无明文 CID、不解密、不与投递 inbox 耦合；按 `updated_at` LWW，
//!   并以时钟偏移上界兜底，防止被盗/异常客户端把锚自锁在 `u64::MAX`。

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
use sp_runtime::traits::SaturatedConversion;
use sp_std::vec::Vec;

/// EN: Balance type of the configured reservable currency.
/// CN: 所配置可预留货币的余额类型。
pub type BalanceOf<T> =
    <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// EN: Domain-separation context for publish signatures (frozen byte contract, ADR §5.5).
/// CN: publish 签名的域分离 context（冻结的字节合同，ADR §5.5）。
pub const PUBLISH_CONTEXT: &[u8] = b"nexus/chat-sync/publish/v1";

/// EN: Domain-separation context for clear signatures (frozen byte contract, ADR §5.5).
/// CN: clear 签名的域分离 context（冻结的字节合同，ADR §5.5）。
pub const CLEAR_CONTEXT: &[u8] = b"nexus/chat-sync/clear/v1";

/// EN: Minimum plausible ciphertext length (AES-GCM wire can never be shorter).
/// CN: 密文最小合理长度（AES-GCM wire 不可能更短）。
pub const MIN_CIPHERTEXT_LEN: u32 = 16;

/// EN: Current anchor record wire version. CN: 当前锚记录 wire 版本。
pub const ANCHOR_VERSION: u8 = 1;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::types::{AnchorId, SyncAnchorRecord};
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use pallet_chat_common::{min_blocks_elapsed, reserve_deposit, unreserve_deposit};
    use sp_core::ed25519;
    use sp_io::hashing::blake2_256;

    /// EN: In-code storage version (no migrations yet). CN: 代码内存储版本（暂无迁移）。
    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// EN: Pallet configuration. CN: Pallet 配置。
    #[pallet::config]
    pub trait Config:
        frame_system::Config<RuntimeEvent: From<Event<Self>>> + pallet_timestamp::Config
    {
        /// EN: Reservable currency for the anti-spam anchor deposit.
        /// CN: 用于反垃圾锚押金的可预留货币。
        type Currency: ReservableCurrency<Self::AccountId>;

        /// EN: Max encrypted SyncManifest length (suggest 512: headroom over the
        /// current ~200B canonical manifest). CN: 加密 SyncManifest 最大长度
        /// （建议 512：在当前约 200B 规范清单之上留增长余量）。
        #[pallet::constant]
        type MaxAnchorLen: Get<u32>;

        /// EN: Minimum blocks between two accepted publishes of the same anchor
        /// (on-chain hard cap, NOT the target write rate — clients debounce much
        /// longer, ADR §11.2). CN: 同一锚两次成功 publish 之间的最小块数（链上硬顶，
        /// 非目标写入频率——客户端 debounce 远更长，ADR §11.2）。
        #[pallet::constant]
        type MinBlocksBetweenPublish: Get<BlockNumberFor<Self>>;

        /// EN: Deposit reserved from the origin on first publish, refunded on clear.
        /// CN: 首次发布时从 origin 预留、clear 时退还的押金。
        #[pallet::constant]
        type AnchorDeposit: Get<BalanceOf<Self>>;

        /// EN: Upper tolerance (ms) for `updated_at` over on-chain time. Prevents a
        /// stolen/buggy client from self-locking the anchor at `u64::MAX` — the anchor
        /// key derives from the mnemonic and cannot be rotated, so the chain must
        /// backstop. CN: `updated_at` 相对链上时间的上界容差（毫秒）。防止被盗/异常客户
        /// 端把锚自锁在 `u64::MAX`——锚密钥派生自助记词、不可轮换，必须由链兜底。
        #[pallet::constant]
        type MaxClockSkew: Get<u64>;

        /// EN: Privileged origin for `force_clear_sync_anchor` — the governance escape
        /// hatch for abandoned/abusive anchors (the anchor key derives from a mnemonic
        /// and may be lost forever; without this the record + deposit are permanent).
        /// CN: `force_clear_sync_anchor` 的特权 origin——清理被遗弃/滥用锚的治理逃生门
        /// （锚密钥派生自助记词、可能永久丢失；没有它记录与押金将永久滞留）。
        type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// EN: Weight info (must include the ed25519_verify cost).
        /// CN: 权重信息（须包含 ed25519_verify 开销）。
        type WeightInfo: WeightInfo;
    }

    // ==================== 存储 / Storage ====================

    /// EN: Encrypted sync anchors: `anchor_id -> record`. CN: 加密同步锚存储。
    #[pallet::storage]
    #[pallet::getter(fn sync_anchors)]
    pub type SyncAnchors<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        AnchorId,
        SyncAnchorRecord<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, T::MaxAnchorLen>,
        OptionQuery,
    >;

    /// EN: Clear tombstone watermark: `anchor_id -> updated_at at clear time`. A publish
    /// onto a cleared (absent) anchor must carry `updated_at` STRICTLY greater — without
    /// this, anyone could "resurrect" a deliberately cleared anchor by replaying a
    /// historical publish payload + signature from chain history (publish signatures do
    /// not bind the origin). Kept forever: 40B per cleared anchor is the cost of
    /// revocation memory.
    /// CN: clear 墓碑水位：`anchor_id -> clear 时的 updated_at`。向已 clear（不存在）的锚
    /// publish 必须携带**严格更大**的 `updated_at`——否则任何人都能用链历史中的 publish
    /// 载荷 + 签名「复活」被主动 clear 的锚（publish 签名不绑定 origin）。永久保留：
    /// 每个被 clear 的锚 40B，是撤销记忆的代价。
    #[pallet::storage]
    pub type ClearedAt<T: Config> = StorageMap<_, Blake2_128Concat, AnchorId, u64, OptionQuery>;

    // ==================== 事件 / Events ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// EN: An anchor was published or replaced (client's on-chain confirmation
        /// basis). CN: 锚已发布或更新（前端确认上链的依据）。
        AnchorPublished { anchor_id: AnchorId, updated_at: u64 },

        /// EN: An anchor was cleared and its deposit refunded.
        /// CN: 锚已删除并退还押金。
        AnchorCleared { anchor_id: AnchorId },

        /// EN: An anchor was force-cleared by [`Config::ForceOrigin`] (deposit refunded
        /// to the depositor; distinct event for governance transparency).
        /// CN: 锚被 [`Config::ForceOrigin`] 强制清除（押金退还 depositor；独立事件以保证
        /// 治理透明）。
        AnchorForceCleared { anchor_id: AnchorId },
    }

    // ==================== 错误 / Errors ====================

    #[pallet::error]
    pub enum Error<T> {
        /// EN: `anchor_sig` does not verify under `anchor_pk` for the canonical
        /// payload. CN: `anchor_sig` 对规范 payload 在 `anchor_pk` 下校验失败。
        BadAnchorSignature,
        /// EN: No anchor stored at this `anchor_id`. CN: 该 `anchor_id` 无锚。
        AnchorNotFound,
        /// EN: `updated_at` is older than the stored value (LWW; equality is
        /// allowed). CN: `updated_at` 旧于已存值（LWW；相等允许）。
        StaleUpdatedAt,
        /// EN: `updated_at` exceeds on-chain time + `MaxClockSkew`.
        /// CN: `updated_at` 超过链上时间 + `MaxClockSkew`。
        UpdatedAtTooFarInFuture,
        /// EN: Ciphertext shorter than the minimum AES-GCM wire size.
        /// CN: 密文短于 AES-GCM wire 最小长度。
        CiphertextTooShort,
        /// EN: Re-publish within `MinBlocksBetweenPublish` of the previous one.
        /// CN: 距上次 publish 不足 `MinBlocksBetweenPublish` 块。
        PublishTooFrequent,
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
        /// EN: Publish or replace the encrypted sync anchor at `blake2_256(anchor_pk)`.
        /// Authorization = Ed25519 `anchor_sig` by the anchor key over
        /// `"nexus/chat-sync/publish/v1" ‖ genesis_hash ‖ anchor_id ‖ updated_at(LE u64)
        /// ‖ blake2_256(ciphertext)`; the signed origin only pays fees and (on first
        /// publish) the deposit. Rejects stale (`<` stored, LWW with `==` allowed) and
        /// far-future `updated_at`, and enforces the per-anchor block-height rate limit.
        /// The chain never inspects `ciphertext`.
        /// CN: 发布/更新 `blake2_256(anchor_pk)` 处的加密同步锚。授权 = 锚密钥对
        /// `"nexus/chat-sync/publish/v1" ‖ genesis_hash ‖ anchor_id ‖ updated_at(LE u64)
        /// ‖ blake2_256(密文)` 的 Ed25519 签名；签名 origin 仅付手续费与（首次发布的）
        /// 押金。拒绝过期（`<` 已存值，LWW 且 `==` 允许）与超前时间戳，并执行每锚块高
        /// 频率限制。链从不检查 `ciphertext` 内容。
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config>::WeightInfo::publish_sync_anchor())]
        pub fn publish_sync_anchor(
            origin: OriginFor<T>,
            anchor_pk: [u8; 32],
            updated_at: u64,
            ciphertext: BoundedVec<u8, T::MaxAnchorLen>,
            anchor_sig: [u8; 64],
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 1. 存储键由链计算，调用方不可指定 / storage key computed by the chain.
            let anchor_id: AnchorId = blake2_256(&anchor_pk);

            // 3. 密文长度（先于昂贵的签名校验）/ length check before the expensive verify.
            ensure!(
                ciphertext.len() >= MIN_CIPHERTEXT_LEN as usize,
                Error::<T>::CiphertextTooShort
            );

            // 2. 锚签名校验 / anchor signature verification.
            let payload = Self::publish_payload(&anchor_id, updated_at, &ciphertext);
            Self::verify_anchor_sig(&anchor_pk, &anchor_sig, &payload)?;

            // 5. 上界容差（防自锁）/ upper clock-skew bound (anti self-lock).
            let now_ms: u64 = pallet_timestamp::Pallet::<T>::get().saturated_into::<u64>();
            ensure!(
                updated_at <= now_ms.saturating_add(T::MaxClockSkew::get()),
                Error::<T>::UpdatedAtTooFarInFuture
            );

            let current_block = frame_system::Pallet::<T>::block_number();

            match SyncAnchors::<T>::get(anchor_id) {
                Some(mut record) => {
                    // 4. LWW（`==` 允许：幂等重发是有意语义）/ LWW (`==` allowed: idempotent
                    // resend is intentional).
                    ensure!(updated_at >= record.updated_at, Error::<T>::StaleUpdatedAt);

                    // 4b. 内容未变的等值重发 = 幂等 no-op：不写状态、不重置限频时钟。
                    // 否则任何观察者都能用公开的 (payload, sig) 反复重发，把
                    // `last_publish_block` 顶到最新，对合法用户的下一次更新做费用竞价式
                    // 活性骚扰。/ Unchanged equal resend = idempotent no-op: no state
                    // write, and crucially NO rate-limit clock reset — otherwise any
                    // observer could re-submit the public (payload, sig) to keep bumping
                    // `last_publish_block`, fee-bidding the owner's next update out of
                    // every window.
                    if updated_at == record.updated_at && ciphertext == record.ciphertext {
                        Self::deposit_event(Event::AnchorPublished { anchor_id, updated_at });
                        return Ok(());
                    }

                    // 6. 每锚块高频率限制 / per-anchor block-height rate limit.
                    ensure!(
                        min_blocks_elapsed(
                            record.last_publish_block,
                            current_block,
                            T::MinBlocksBetweenPublish::get(),
                        ),
                        Error::<T>::PublishTooFrequent
                    );
                    // 7. depositor 与押金保持不变（不论本次 origin 是谁）。
                    // depositor & deposit stay unchanged regardless of this origin.
                    record.version = ANCHOR_VERSION;
                    record.updated_at = updated_at;
                    record.ciphertext = ciphertext;
                    record.last_publish_block = current_block;
                    SyncAnchors::<T>::insert(anchor_id, record);
                }
                None => {
                    // 4c. clear 墓碑水位：必须严格新于 clear 时的 updated_at，否则链历史
                    // 中的旧 publish 载荷可被任何人重放「复活」已主动 clear 的锚。
                    // / clear tombstone watermark: must be strictly newer than the
                    // updated_at at clear time, else anyone could resurrect a
                    // deliberately cleared anchor by replaying an old publish payload
                    // from chain history.
                    if let Some(watermark) = ClearedAt::<T>::get(anchor_id) {
                        ensure!(updated_at > watermark, Error::<T>::StaleUpdatedAt);
                    }
                    // 7. 首次发布：从 origin 预留押金 / first publish: reserve from origin.
                    let deposit = T::AnchorDeposit::get();
                    reserve_deposit::<T::Currency, _, _>(&who, deposit)?;
                    SyncAnchors::<T>::insert(
                        anchor_id,
                        SyncAnchorRecord {
                            version: ANCHOR_VERSION,
                            updated_at,
                            ciphertext,
                            depositor: who,
                            deposit,
                            last_publish_block: current_block,
                        },
                    );
                }
            }

            Self::deposit_event(Event::AnchorPublished { anchor_id, updated_at });
            Ok(())
        }

        /// EN: Remove the anchor and refund the deposit to the recorded `depositor`
        /// (not necessarily this origin). The signature binds `stored.updated_at`
        /// under the clear context, so a captured clear signature cannot be replayed
        /// against a later anchor state. CN: 删除锚并向记录的 `depositor`（不必是本次
        /// origin）退还押金。签名在 clear context 下绑定 `stored.updated_at`，截获的
        /// clear 签名无法对之后的锚状态重放。
        #[pallet::call_index(1)]
        #[pallet::weight(<T as Config>::WeightInfo::clear_sync_anchor())]
        pub fn clear_sync_anchor(
            origin: OriginFor<T>,
            anchor_pk: [u8; 32],
            anchor_sig: [u8; 64],
        ) -> DispatchResult {
            ensure_signed(origin)?;

            let anchor_id: AnchorId = blake2_256(&anchor_pk);
            let record = SyncAnchors::<T>::get(anchor_id).ok_or(Error::<T>::AnchorNotFound)?;

            let payload = Self::clear_payload(&anchor_id, record.updated_at);
            Self::verify_anchor_sig(&anchor_pk, &anchor_sig, &payload)?;

            unreserve_deposit::<T::Currency, _, _>(&record.depositor, record.deposit);
            SyncAnchors::<T>::remove(anchor_id);
            // 墓碑水位：使所有 ≤ 此值的历史 publish 签名永久失效（防复活）。
            // Tombstone watermark: permanently invalidates all historical publish
            // signatures at or below this value (anti-resurrection).
            ClearedAt::<T>::insert(anchor_id, record.updated_at);

            Self::deposit_event(Event::AnchorCleared { anchor_id });
            Ok(())
        }

        /// EN: Governance escape hatch: remove an anchor via [`Config::ForceOrigin`]
        /// without an anchor signature (the anchor key derives from a mnemonic and may
        /// be lost forever). The deposit is refunded to the recorded depositor and the
        /// clear tombstone is set, exactly as in a regular clear. The owner can always
        /// re-publish with a newer manifest — force-clear cannot censor future state.
        /// CN: 治理逃生门：经 [`Config::ForceOrigin`] 在无锚签名的情况下移除锚（锚密钥
        /// 派生自助记词，可能永久丢失）。押金退还记录的 depositor，并与常规 clear 一样
        /// 写入墓碑水位。持有者随时可用更新清单重新发布——force-clear 无法审查未来状态。
        #[pallet::call_index(2)]
        #[pallet::weight(<T as Config>::WeightInfo::force_clear_sync_anchor())]
        pub fn force_clear_sync_anchor(origin: OriginFor<T>, anchor_id: AnchorId) -> DispatchResult {
            T::ForceOrigin::ensure_origin(origin)?;

            let record = SyncAnchors::<T>::get(anchor_id).ok_or(Error::<T>::AnchorNotFound)?;
            unreserve_deposit::<T::Currency, _, _>(&record.depositor, record.deposit);
            SyncAnchors::<T>::remove(anchor_id);
            ClearedAt::<T>::insert(anchor_id, record.updated_at);

            Self::deposit_event(Event::AnchorForceCleared { anchor_id });
            Ok(())
        }
    }

    // ==================== 内部辅助 / Internal helpers ====================

    impl<T: Config> Pallet<T> {
        /// EN: Canonical publish payload (frozen byte contract, ADR §5.5): bare
        /// concatenation, no length prefixes, `updated_at` little-endian.
        /// CN: 规范 publish payload（冻结字节合同，ADR §5.5）：裸拼接、无长度前缀、
        /// `updated_at` 小端。
        pub fn publish_payload(
            anchor_id: &AnchorId,
            updated_at: u64,
            ciphertext: &[u8],
        ) -> Vec<u8> {
            let genesis = frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero());
            let mut payload =
                Vec::with_capacity(PUBLISH_CONTEXT.len() + 32 + 32 + 8 + 32);
            payload.extend_from_slice(PUBLISH_CONTEXT);
            payload.extend_from_slice(genesis.as_ref());
            payload.extend_from_slice(anchor_id);
            payload.extend_from_slice(&updated_at.to_le_bytes());
            payload.extend_from_slice(&blake2_256(ciphertext));
            payload
        }

        /// EN: Canonical clear payload — binds the CURRENT stored `updated_at`
        /// (anti-replay across states); the ciphertext segment is omitted.
        /// CN: 规范 clear payload——绑定**当前**已存 `updated_at`（跨状态防重放）；
        /// 省略密文段。
        pub fn clear_payload(anchor_id: &AnchorId, stored_updated_at: u64) -> Vec<u8> {
            let genesis = frame_system::Pallet::<T>::block_hash(BlockNumberFor::<T>::zero());
            let mut payload = Vec::with_capacity(CLEAR_CONTEXT.len() + 32 + 32 + 8);
            payload.extend_from_slice(CLEAR_CONTEXT);
            payload.extend_from_slice(genesis.as_ref());
            payload.extend_from_slice(anchor_id);
            payload.extend_from_slice(&stored_updated_at.to_le_bytes());
            payload
        }

        fn verify_anchor_sig(
            anchor_pk: &[u8; 32],
            anchor_sig: &[u8; 64],
            payload: &[u8],
        ) -> DispatchResult {
            let pk = ed25519::Public::from_raw(*anchor_pk);
            let sig = ed25519::Signature::from_raw(*anchor_sig);
            ensure!(
                sp_io::crypto::ed25519_verify(&sig, payload, &pk),
                Error::<T>::BadAnchorSignature
            );
            Ok(())
        }
    }

    // ==================== 只读辅助 / Read-only helpers ====================

    impl<T: Config> Pallet<T> {
        /// EN: Stored anchor at `anchor_id` as `(updated_at, ciphertext)`, or `None`.
        /// CN: `anchor_id` 处的锚（`(updated_at, ciphertext)`）；不存在则为 `None`。
        pub fn sync_anchor(anchor_id: AnchorId) -> Option<(u64, Vec<u8>)> {
            SyncAnchors::<T>::get(anchor_id).map(|r| (r.updated_at, r.ciphertext.into_inner()))
        }
    }
}
