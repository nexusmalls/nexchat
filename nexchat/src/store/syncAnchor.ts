// EN: Client side of the Encrypted Sync Anchor (EISA) byte contract
// (ADR CHAT_SYNC_ANCHOR §5.2/§5.3/§5.5, frozen):
//   anchor_seed = HKDF-SHA256(ikm = vault_master, salt = "chat/sync-anchor-key/v1")
//   (anchor_sk, anchor_pk) = Ed25519-keygen(anchor_seed)        // RFC 8032 seed
//   anchor_id   = blake2_256(anchor_pk)                          // on-chain storage key
//   K_sync      = HKDF-SHA256(ikm = vault_master, salt = "chat/sync-manifest/v1",
//                             info = anchor_id)
//   manifest wire = iv(12) || AES-256-GCM ciphertext (over CANONICAL JSON bytes)
//   sig payload   = UTF8(context) || genesis_hash(32) || anchor_id(32)
//                   || updated_at(u64 LE, 8) || blake2_256(ciphertext)   // publish
//                   (clear: same minus the ciphertext segment, ts = stored.updated_at)
// Mirrored by `pallet-chat-sync` in Rust; both ends are pinned by shared test vectors
// in `syncAnchor.test.ts` / the pallet's `tests.rs` — never change one side alone.
// CN: 加密同步锚（EISA）字节合同的客户端实现（ADR CHAT_SYNC_ANCHOR §5.2/§5.3/§5.5，已冻结）：
// 派生链、清单 wire、签名 payload 编码如上。Rust 侧由 `pallet-chat-sync` 镜像实现；两端由
// `syncAnchor.test.ts` 与 pallet `tests.rs` 中的共享测试向量钉死——绝不允许单边改动。

export const PUBLISH_CONTEXT = "nexus/chat-sync/publish/v1";
export const CLEAR_CONTEXT = "nexus/chat-sync/clear/v1";
export const ANCHOR_SEED_SALT = "chat/sync-anchor-key/v1";
export const K_SYNC_SALT = "chat/sync-manifest/v1";
/// EN: Track A MLS-state escrow key salt (design CHAT_MULTIDEVICE_MLS_SYNC §4.3). Domain-separated
/// from K_sync so the same `vault_master` yields an independent key for the MLS escrow vault blob.
/// CN: 路线 A MLS 状态托管密钥 salt（设计 §4.3）。与 K_sync 域分离，使同一 `vault_master` 为 MLS
/// 托管 vault blob 派生出独立密钥。
export const K_MLS_ESCROW_SALT = "chat/mls-escrow/v1";
/// EN: Client-side cap for the sealed manifest (chain `MaxAnchorLen` = 512).
/// CN: 客户端密文清单上限（链上 `MaxAnchorLen` = 512）。
export const MAX_ANCHOR_CIPHERTEXT = 512;

const enc = new TextEncoder();

/// EN: One per-slot pointer inside the manifest (same shape as the relay pointers).
/// CN: 清单内的单槽位指针（与 relay 指针同构）。
export interface SyncPointer {
  cid: string;
  updated_at: number;
}

/// EN: SyncManifest plaintext (ADR §5.2): per-slot CID pointers, merged field-by-field
/// LWW; the top-level `updated_at` is only a cache hint. CN: SyncManifest 明文（ADR §5.2）：
/// 各槽位 CID 指针，按字段逐项 LWW 合并；顶层 `updated_at` 仅作缓存提示。
export interface SyncManifest {
  v: 1;
  updated_at: number;
  index?: SyncPointer;
  contacts?: SyncPointer;
  archive?: SyncPointer;
  // EN: Track A — CID of the off-chain MLS escrow vault blob (design §4.1/§4.2). Optional and
  // additive: the manifest stays `v:1` so old clients ignore this unknown field (forward-compat),
  // and it is merged field-by-field LWW like the other slots. The blob it points to is E2E-encrypted
  // under K_mls_escrow and never on-chain. CN: 路线 A —— 链下 MLS 托管 vault blob 的 CID（设计
  // §4.1/§4.2）。可选且加性：清单仍为 `v:1`，旧客户端忽略此未知字段（向前兼容），按字段逐项 LWW
  // 合并。所指 blob 用 K_mls_escrow 端到端加密、绝不上链。
  mls?: SyncPointer;
  // EN: Track A — CID of the PIN-wrapped MLS signing-key backup blob (design §5.3 path C). Optional;
  // merged field-by-field LWW. Decryption requires vault_master + user PIN (mnemonic alone cannot send).
  // CN: 路线 A —— PIN 包裹 MLS 签名钥备份 blob 的 CID（设计 §5.3 路径 C）。可选；按字段 LWW 合并。
  // 解密需 vault_master + 用户 PIN（单靠助记词不能发群）。
  mls_signing?: SyncPointer;
}

/// EN: Derived anchor material — everything a device needs to locate, decrypt and
/// mutate its anchor; recomputable from the mnemonic alone. CN: 派生出的锚材料——设备
/// 定位、解密、变更自己锚所需的一切；仅凭助记词即可重算。
export interface AnchorKeys {
  anchorPk: Uint8Array; // 32B Ed25519 public key
  anchorSk: Uint8Array; // 64B expanded secret (polkadot-js layout)
  anchorId: Uint8Array; // 32B = blake2_256(anchorPk)
  kSync: CryptoKey; // AES-256-GCM, non-extractable
}

async function hkdfBits(
  ikm: Uint8Array,
  salt: string,
  info: Uint8Array,
  bits: number,
): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", ikm as BufferSource, "HKDF", false, [
    "deriveBits",
  ]);
  const out = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: enc.encode(salt), info: info as BufferSource },
    key,
    bits,
  );
  return new Uint8Array(out);
}

/// EN: Derive the full anchor key set from `vault_master` (§5.3). CN: 由 `vault_master`
/// 派生完整锚密钥集（§5.3）。
export async function deriveAnchorKeys(vaultMaster: Uint8Array): Promise<AnchorKeys> {
  const { blake2AsU8a, ed25519PairFromSeed, cryptoWaitReady } = await import(
    "@polkadot/util-crypto"
  );
  await cryptoWaitReady();
  const anchorSeed = await hkdfBits(vaultMaster, ANCHOR_SEED_SALT, new Uint8Array(0), 256);
  const pair = ed25519PairFromSeed(anchorSeed);
  const anchorId = blake2AsU8a(pair.publicKey, 256);
  const kSyncBits = await hkdfBits(vaultMaster, K_SYNC_SALT, anchorId, 256);
  const kSync = await crypto.subtle.importKey(
    "raw",
    kSyncBits as BufferSource,
    { name: "AES-GCM" },
    false,
    ["encrypt", "decrypt"],
  );
  kSyncBits.fill(0);
  anchorSeed.fill(0);
  return { anchorPk: pair.publicKey, anchorSk: pair.secretKey, anchorId, kSync };
}

/// EN: Track A MLS-state escrow key (design §4.3): `K_mls_escrow = HKDF-SHA256(vault_master,
/// salt="chat/mls-escrow/v1", info=anchor_id)`. Non-extractable AES-256-GCM, domain-separated from
/// K_sync. It encrypts the per-account MLS escrow vault blob (design §4.2) that is stored OFF-CHAIN
/// (relay/IPFS) and never on-chain; the manifest only carries its CID. Signature private keys are
/// NEVER inside this vault (design §3.2/§3.3) — escrow grants read, not impersonation.
/// CN: 路线 A 的 MLS 状态托管密钥（设计 §4.3）。不可导出 AES-256-GCM，与 K_sync 域分离。用于加密
/// 链下（relay/IPFS）的 MLS 托管 vault blob（设计 §4.2，绝不上链，清单只存其 CID）。签名私钥**绝不**
/// 进此 vault（设计 §3.2/§3.3）——托管只授予「读」，不授予「冒名发送」。
export async function deriveMlsEscrowKey(
  vaultMaster: Uint8Array,
  anchorId: Uint8Array,
): Promise<CryptoKey> {
  const bits = await hkdfBits(vaultMaster, K_MLS_ESCROW_SALT, anchorId, 256);
  const key = await crypto.subtle.importKey(
    "raw",
    bits as BufferSource,
    { name: "AES-GCM" },
    false,
    ["encrypt", "decrypt"],
  );
  bits.fill(0);
  return key;
}

/// EN: Canonical JSON (frozen contract, §5.2): UTF-8, object keys sorted
/// lexicographically at every depth, no whitespace, numbers via JS default
/// serialization. Hash-skip and encryption both operate on these exact bytes.
/// CN: canonical JSON（冻结合同，§5.2）：UTF-8、对象键各层级按字典序、无空白、数字用
/// JS 默认序列化。hash-skip 与加密都作用于这串字节。
export function canonicalJsonBytes(value: unknown): Uint8Array {
  return enc.encode(canonicalJson(value));
}

export function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const keys = Object.keys(value as Record<string, unknown>)
    .filter((k) => (value as Record<string, unknown>)[k] !== undefined)
    .sort();
  const body = keys
    .map((k) => `${JSON.stringify(k)}:${canonicalJson((value as Record<string, unknown>)[k])}`)
    .join(",");
  return `{${body}}`;
}

/// EN: Seal a manifest under K_sync; wire = `iv(12) || ct` (no version byte — the
/// chain record carries `version` separately). Enforces the 512B chain cap.
/// CN: 用 K_sync 封装清单；wire = `iv(12) || ct`（无版本字节——链上记录单独携带
/// `version`）。强制链上 512B 上限。
export async function encryptManifest(keys: AnchorKeys, manifest: SyncManifest): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    keys.kSync,
    canonicalJsonBytes(manifest) as BufferSource,
  );
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  if (out.length > MAX_ANCHOR_CIPHERTEXT) {
    throw new Error(`syncAnchor: sealed manifest ${out.length}B exceeds chain cap ${MAX_ANCHOR_CIPHERTEXT}B`);
  }
  return out;
}

export async function decryptManifest(keys: AnchorKeys, packed: Uint8Array): Promise<SyncManifest> {
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: packed.slice(0, 12) as BufferSource },
    keys.kSync,
    packed.slice(12) as BufferSource,
  );
  const manifest = JSON.parse(new TextDecoder().decode(pt)) as SyncManifest;
  if (manifest.v !== 1) throw new Error(`unsupported sync-manifest version ${manifest.v}`);
  return manifest;
}

function u64Le(value: number | bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), true);
  return out;
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

/// EN: Publish signature payload (§5.5, bare concatenation, u64 LE).
/// CN: publish 签名 payload（§5.5，裸拼接，u64 小端）。
export async function buildPublishPayload(
  genesisHash: Uint8Array,
  anchorId: Uint8Array,
  updatedAt: number | bigint,
  ciphertext: Uint8Array,
): Promise<Uint8Array> {
  const { blake2AsU8a } = await import("@polkadot/util-crypto");
  return concatBytes(
    enc.encode(PUBLISH_CONTEXT),
    genesisHash,
    anchorId,
    u64Le(updatedAt),
    blake2AsU8a(ciphertext, 256),
  );
}

/// EN: Clear signature payload — binds the CURRENT stored `updated_at`; no ciphertext
/// segment. CN: clear 签名 payload——绑定**当前**已存 `updated_at`；无密文段。
export function buildClearPayload(
  genesisHash: Uint8Array,
  anchorId: Uint8Array,
  storedUpdatedAt: number | bigint,
): Uint8Array {
  return concatBytes(enc.encode(CLEAR_CONTEXT), genesisHash, anchorId, u64Le(storedUpdatedAt));
}

/// EN: Ed25519 anchor signature over a payload. CN: 对 payload 的 Ed25519 锚签名。
export async function signAnchorPayload(keys: AnchorKeys, payload: Uint8Array): Promise<Uint8Array> {
  const { ed25519Sign } = await import("@polkadot/util-crypto");
  return ed25519Sign(payload, { publicKey: keys.anchorPk, secretKey: keys.anchorSk });
}

/// EN: Runtime `pallet_chat_sync::AnchorDeposit` (= UNIT/2 = 0.5 NEX). CN: 链上首发布押金（0.5 NEX）。
export const SYNC_ANCHOR_FIRST_DEPOSIT_PLANCK = 500_000_000_000n;
/// EN: Headroom for extrinsic fees on publish/update. CN: publish/update 交易费余量。
export const SYNC_ANCHOR_FEE_BUFFER_PLANCK = 10_000_000_000n;

/// EN: True when the node rejected the tx for insufficient balance (1010). CN: 节点因余额不足拒绝（1010）。
export function isInsufficientBalanceError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return (
    msg.includes("1010") ||
    msg.includes("Inability to pay") ||
    msg.includes("balance too low") ||
    msg.includes("InsufficientBalance")
  );
}

/// EN: Human hint for UI when anchor publish is blocked by balance. CN: 锚发布因余额不足被阻断时的 UI 提示。
export function syncAnchorBalanceHint(firstPublish: boolean): string {
  return firstPublish
    ? "链上余额不足，无法首次发布同步锚（需约 0.5 NEX 押金 + 手续费）；relay/IPFS 同步仍可用"
    : "链上余额不足，无法更新同步锚（需预留手续费）；relay/IPFS 同步仍可用";
}
