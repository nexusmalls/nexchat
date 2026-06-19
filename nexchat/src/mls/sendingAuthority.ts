// EN: Track A sending-authority primitives (design CHAT_MULTIDEVICE_MLS_SYNC §5). Under the naive
// shared-leaf escrow model, AT MOST ONE device may hold the signing key and send/Commit at any time
// (eliminates the §3.1 nonce reuse + concurrent-commit hazard). The authoritative credential for
// "who may send" is a `HandoffReceipt{ from, to, seq, ts }` signed by the account's device-directory
// key (HKDF-derived from `vault_master`, §5.2): the receipt with the greatest monotone `seq` wins,
// so a stale receipt is naturally superseded. This module is the PURE crypto + authority arithmetic
// (sign/verify + latest-receipt selection + a send guard); the online/offline handoff TRANSPORT
// (relay device-to-device channel) and the three-state UI live elsewhere and are gated by the P0
// security review. Everything here is dormant until `VITE_MLS_VAULT_ENABLED` + the §5 handoff ship.
//
// ⚠️ Route B (subgroup virtual clients) makes this whole notion obsolete — concurrent sends are
// native via the `reuse_guard` PRP (§8.5); this single-active-sender machinery is Route-A-only.
// CN: 路线 A 发送权原语（设计 §5）。朴素共享 leaf 托管下，任一时刻**至多一台设备**持签名钥并发送/Commit
// （消除 §3.1 nonce 重用 + 并发 commit 风险）。「谁可发送」的权威凭据是由账户**设备目录钥**（由 `vault_master`
// HKDF 派生，§5.2）签名的 `HandoffReceipt{ from, to, seq, ts }`：`seq` 单调最大者胜出，旧收据自然失效。
// 本模块为**纯**密码学 + 权威裁决（签/验 + 取最新收据 + 发送守卫）；在线/离线交接**传输**（relay 设备对端
// 通道）与三态 UI 在别处，且受 P0 安全评审门控。在 `VITE_MLS_VAULT_ENABLED` + §5 交接上线前全部休眠。
//
// ⚠️ 路线 B（子群虚拟客户端）使本概念作废——并发发送由 `reuse_guard` PRP 原生支持（§8.5）；本单活跃发送机制
// 仅路线 A 专用。

/// EN: HKDF salt for the account device-directory signing key. Domain-separated from K_sync /
/// K_mls_escrow. CN: 账户设备目录签名钥的 HKDF salt。与 K_sync / K_mls_escrow 域分离。
export const DEVICE_DIRECTORY_SALT = "chat/device-directory/v1";

/// EN: Signature domain-separation context for handoff receipts. CN: 交接收据的签名域分离上下文。
export const HANDOFF_RECEIPT_CONTEXT = "nexus/chat-sync/handoff/v1";

const enc = new TextEncoder();

/// EN: Sending-authority transfer receipt (design §5.2). `from`/`to` are device ids
/// (`convIndex.deviceId()`); `seq` is the account-wide monotone handoff counter (the authority
/// ordering); `ts` is wall-clock (audit + equal-seq tiebreak only). CN: 发送权交接收据（设计 §5.2）。
/// `from`/`to` 为设备 id；`seq` 为账户级单调交接计数（权威排序）；`ts` 为墙钟（仅审计 + 同 seq 平局裁决）。
export interface HandoffReceipt {
  v: 1;
  from: string;
  to: string;
  seq: number;
  ts: number;
}

/// EN: A receipt plus its detached signature (hex). CN: 收据 + 其分离签名（hex）。
export interface SignedHandoffReceipt {
  receipt: HandoffReceipt;
  sig: string;
}

/// EN: Account device-directory key pair (ed25519). The PRIVATE half mints receipts; the PUBLIC
/// half verifies them. Both are recomputable from `vault_master` alone, so every device of the
/// account shares them — the anti-double-primary guarantee comes from the monotone `seq` + the
/// chain's epoch total order (§5.4), not from key secrecy. CN: 账户设备目录密钥对（ed25519）。私钥签发
/// 收据、公钥验证。两者仅凭 `vault_master` 即可重算，故账户全部设备共享——防双 primary 由单调 `seq` +
/// 链 epoch 全序（§5.4）保证，而非密钥保密。
export interface DeviceDirectoryKey {
  publicKey: Uint8Array;
  secretKey: Uint8Array;
}

async function hkdfBits(ikm: Uint8Array, salt: string, bits: number): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", ikm as BufferSource, "HKDF", false, [
    "deriveBits",
  ]);
  const out = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: enc.encode(salt), info: new Uint8Array(0) as BufferSource },
    key,
    bits,
  );
  return new Uint8Array(out);
}

/// EN: Derive the account device-directory ed25519 key pair from `vault_master` (§5.2).
/// CN: 由 `vault_master` 派生账户设备目录 ed25519 密钥对（§5.2）。
export async function deriveDeviceDirectoryKey(vaultMaster: Uint8Array): Promise<DeviceDirectoryKey> {
  const { ed25519PairFromSeed, cryptoWaitReady } = await import("@polkadot/util-crypto");
  await cryptoWaitReady();
  const seed = await hkdfBits(vaultMaster, DEVICE_DIRECTORY_SALT, 256);
  const pair = ed25519PairFromSeed(seed);
  seed.fill(0);
  return { publicKey: pair.publicKey, secretKey: pair.secretKey };
}

function u64Le(value: number): Uint8Array {
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

/// EN: Canonical signing bytes for a receipt: `context || v(1) || len-prefixed from || len-prefixed
/// to || seq(u64 LE) || ts(u64 LE)`. Length prefixes prevent `from`/`to` boundary ambiguity.
/// CN: 收据的规范签名字节：`上下文 || v(1) || 带长度前缀 from || 带长度前缀 to || seq(u64 LE) || ts(u64 LE)`。
/// 长度前缀避免 `from`/`to` 边界歧义。
export function handoffReceiptBytes(r: HandoffReceipt): Uint8Array {
  const from = enc.encode(r.from);
  const to = enc.encode(r.to);
  return concatBytes(
    enc.encode(HANDOFF_RECEIPT_CONTEXT),
    Uint8Array.of(r.v),
    u64Le(from.length),
    from,
    u64Le(to.length),
    to,
    u64Le(r.seq),
    u64Le(r.ts),
  );
}

function toHex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

function fromHex(hex: string): Uint8Array {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/// EN: Sign a receipt with the device-directory private key → hex signature. CN: 用设备目录私钥签名
/// 收据 → hex 签名。
export async function signHandoffReceipt(
  dir: DeviceDirectoryKey,
  receipt: HandoffReceipt,
): Promise<string> {
  const { ed25519Sign } = await import("@polkadot/util-crypto");
  const sig = ed25519Sign(handoffReceiptBytes(receipt), {
    publicKey: dir.publicKey,
    secretKey: dir.secretKey,
  });
  return toHex(sig);
}

/// EN: Verify a signed receipt against the device-directory public key. Never throws. CN: 用设备目录
/// 公钥验证签名收据。绝不抛错。
export async function verifyHandoffReceipt(
  dirPublicKey: Uint8Array,
  signed: SignedHandoffReceipt,
): Promise<boolean> {
  try {
    const { ed25519Verify } = await import("@polkadot/util-crypto");
    return ed25519Verify(handoffReceiptBytes(signed.receipt), fromHex(signed.sig), dirPublicKey);
  } catch {
    return false;
  }
}

/// EN: Total order over receipts: greater `seq` wins; tie broken by greater `ts`. Pure.
/// CN: 收据全序：`seq` 大者胜；平局取 `ts` 大者。纯函数。
export function compareReceipts(a: HandoffReceipt, b: HandoffReceipt): number {
  if (a.seq !== b.seq) return a.seq - b.seq;
  return a.ts - b.ts;
}

/// EN: Pick the authoritative (latest) receipt from a set. Returns null for an empty set. Pure —
/// callers MUST have already cryptographically verified each receipt's signature. CN: 从集合取权威
/// （最新）收据；空集返回 null。纯函数——调用方须**先**验过每条收据签名。
export function pickLatestReceipt(receipts: SignedHandoffReceipt[]): SignedHandoffReceipt | null {
  let best: SignedHandoffReceipt | null = null;
  for (const r of receipts) {
    if (!best || compareReceipts(r.receipt, best.receipt) > 0) best = r;
  }
  return best;
}

/// EN: Resolve which device currently holds sending authority. The `to` of the latest receipt wins;
/// with no receipt yet (no handoff ever happened) authority falls back to `primaryDeviceId` (the
/// §5.1 manifest field — the original/bootstrap sender). Pure. CN: 裁定当前持发送权的设备。最新收据的
/// `to` 胜出；尚无收据（从未交接）时回退到 `primaryDeviceId`（§5.1 清单字段——初始/引导发送设备）。纯函数。
export function resolveAuthoritativeDevice(
  latest: HandoffReceipt | null,
  primaryDeviceId: string | null,
): string | null {
  return latest ? latest.to : primaryDeviceId;
}

/// EN: Client-side send guard (§5.4): a device may send/Commit only if it is the authoritative
/// device AND actually holds a signing key (i.e. not a read-only escrow client). Pure. CN: 客户端发送
/// 守卫（§5.4）：设备仅当**既是**权威设备**且**确实持签名钥（即非只读托管客户端）时方可发送/Commit。纯函数。
export function canSend(args: {
  localDeviceId: string;
  authoritativeDeviceId: string | null;
  hasSigningKey: boolean;
}): boolean {
  return (
    args.hasSigningKey &&
    args.authoritativeDeviceId !== null &&
    args.authoritativeDeviceId === args.localDeviceId
  );
}

/// EN: Build the next receipt for handing authority from `from` to `to`, bumping `seq` past the
/// current latest (1 when there is none). Pure (does not sign). CN: 构造把发送权从 `from` 交给 `to`
/// 的下一张收据，`seq` 自当前最新 +1（无则为 1）。纯函数（不签名）。
export function buildNextReceipt(args: {
  from: string;
  to: string;
  latest: HandoffReceipt | null;
  now: number;
}): HandoffReceipt {
  return {
    v: 1,
    from: args.from,
    to: args.to,
    seq: (args.latest?.seq ?? 0) + 1,
    ts: args.now,
  };
}
