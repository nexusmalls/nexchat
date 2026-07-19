//! Types for `pallet-chat-sync`.
//! `pallet-chat-sync` 的类型定义。

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

/// EN: Account-derived opaque anchor key: `anchor_id = blake2_256(anchor_pk)`, where
/// `anchor_pk` is an Ed25519 public key deterministically derived from the client's
/// `vault_master` (ADR CHAT_SYNC_ANCHOR §5.3). The chain never learns which account
/// derives it; authorization is by an Ed25519 signature of the anchor key, not by the
/// extrinsic origin. Any device holding the mnemonic can recompute it.
/// CN: 账户派生的不透明锚键：`anchor_id = blake2_256(anchor_pk)`，其中 `anchor_pk` 是
/// 客户端由 `vault_master` 确定性派生的 Ed25519 公钥（ADR CHAT_SYNC_ANCHOR §5.3）。链不
/// 知道它由哪个账户派生；授权依据锚密钥的 Ed25519 签名，而非 extrinsic origin。任何持有
/// 助记词的设备都可重算。
pub type AnchorId = [u8; 32];

/// EN: On-chain record of one encrypted sync anchor. `ciphertext` is the client-side
/// AES-256-GCM sealed SyncManifest (the chain never decrypts or inspects it);
/// `updated_at` is the manifest's wall-clock timestamp used for LWW; `depositor` /
/// `deposit` track the reserved anti-spam deposit (refunded on clear, unchanged by
/// later publishes); `last_publish_block` backs the per-anchor block-height rate limit.
/// CN: 单个加密同步锚的链上记录。`ciphertext` 是客户端 AES-256-GCM 封装的 SyncManifest
/// （链从不解密或检查内容）；`updated_at` 为清单的墙钟时间戳，用于 LWW；`depositor` /
/// `deposit` 记录预留的反垃圾押金（clear 时退还，后续 publish 不变）；`last_publish_block`
/// 支撑每锚块高频率限制。
#[derive(
    CloneNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, DebugNoBound,
)]
#[scale_info(skip_type_params(MaxAnchorLen))]
pub struct SyncAnchorRecord<
    AccountId: Clone + PartialEq + Eq + core::fmt::Debug,
    Balance: Clone + PartialEq + Eq + core::fmt::Debug,
    BlockNumber: Clone + PartialEq + Eq + core::fmt::Debug,
    MaxAnchorLen: Get<u32>,
> {
    /// EN: Wire version of the sealed manifest (currently 1). CN: 密文清单 wire 版本（当前 1）。
    pub version: u8,
    /// EN: Manifest `updated_at` in ms; LWW ordering key (`>=` replaces).
    /// CN: 清单 `updated_at`（毫秒）；LWW 排序键（`>=` 即覆盖）。
    pub updated_at: u64,
    /// EN: Client-encrypted SyncManifest bytes (opaque to the chain).
    /// CN: 客户端加密的 SyncManifest 字节（链上不透明）。
    pub ciphertext: BoundedVec<u8, MaxAnchorLen>,
    /// EN: Account whose deposit is reserved (set on first publish, refunded on clear;
    /// later publishes never change it regardless of origin).
    /// CN: 押金被预留的账户（首次发布时记录、clear 时退还；后续 publish 不论 origin 是谁
    /// 都不变更）。
    pub depositor: AccountId,
    /// EN: Reserved deposit amount. CN: 预留押金数额。
    pub deposit: Balance,
    /// EN: Block of the last accepted publish (rate-limit basis).
    /// CN: 最近一次成功 publish 的区块（频率限制依据）。
    pub last_publish_block: BlockNumber,
}
