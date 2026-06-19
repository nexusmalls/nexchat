// EN: Track A device peer key + sealed-box for the online handoff bundle (design §5.1/§5.2). Each
// device generates its OWN encryption keypair on first login (ECDH P-256; NOT derivable from
// vault_master — it must be device-specific so a mnemonic holder cannot read handoff bundles). The
// public half is endorsed by the account device-directory key (`sendingAuthority`) so any device can
// confirm "this peer pubkey belongs to MY account's device X", then exchanged over relay. The signing-
// key bundle (`mls-wasm.exportSigningKeys`) is sealed to the recipient's peer pubkey with an ephemeral
// ECDH key (anonymous-sender ECIES: ephemeral_pub || iv || AES-256-GCM), so it never crosses the relay
// in cleartext (it grants impersonation). Authorization to act on a received bundle is the §5
// HandoffReceipt, verified separately by `handoffCoordinator`.
// CN: 路线 A 设备对端钥 + 在线交接 bundle 的封装盒（设计 §5.1/§5.2）。每台设备首登生成**自己**的加密密钥对
// （ECDH P-256；**不可**由 vault_master 派生——必须设备专属，使持助记词者读不到交接 bundle）。公钥由账户设备
// 目录钥（`sendingAuthority`）背书，任意设备可确认「此对端公钥属于**本账户**的设备 X」，再经 relay 交换。签名钥
// bundle（`mls-wasm.exportSigningKeys`）用临时 ECDH 钥封装给接收方对端公钥（匿名发送方 ECIES：
// ephemeral_pub || iv || AES-256-GCM），故绝不以明文过 relay（它授予冒名能力）。处理收到 bundle 的授权是 §5
// HandoffReceipt，由 `handoffCoordinator` 单独验证。

import {
  type DeviceDirectoryKey,
  type SignedHandoffReceipt,
} from "@/mls/sendingAuthority";

/// EN: HKDF salt for the sealed-box AES key derived from the ECDH shared secret. CN: 由 ECDH 共享秘密
/// 派生封装盒 AES 钥的 HKDF salt。
const SEAL_HKDF_SALT = "chat/device-peer-seal/v1";

/// EN: ed25519 signature domain-separation context for device-peer-key endorsements. CN: 设备对端钥
/// 背书的 ed25519 签名域分离上下文。
export const DEVICE_PEER_ENDORSE_CONTEXT = "nexus/chat-sync/device-peer/v1";

const ECDH = { name: "ECDH", namedCurve: "P-256" } as const;
const lsKey = (account: string) => `nexchat:device-peer:${account}`;
const enc = new TextEncoder();

/// EN: A device's encryption keypair: raw (65-byte uncompressed) public half + the live private key
/// handle. CN: 设备加密密钥对：raw（65 字节非压缩）公钥 + 活跃私钥句柄。
export interface DevicePeerKey {
  publicKeyRaw: Uint8Array;
  privateKey: CryptoKey;
}

/// EN: Directory-key-endorsed binding of a device id ↔ its peer public key (exchanged over relay).
/// CN: 设备 id ↔ 其对端公钥 的目录钥背书绑定（经 relay 交换）。
export interface DevicePeerEndorsement {
  deviceId: string;
  peerPublicKey: string;
  sig: string;
}

function toHex(b: Uint8Array): string {
  let s = "0x";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
}

function fromHex(hex: string): Uint8Array {
  const c = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(c.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(c.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

async function importPublic(raw: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", raw as BufferSource, ECDH, false, []);
}

/// EN: Generate a fresh device peer keypair. CN: 生成全新设备对端密钥对。
export async function generateDevicePeerKey(): Promise<DevicePeerKey> {
  const kp = await crypto.subtle.generateKey(ECDH, true, ["deriveBits"]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", kp.publicKey));
  return { publicKeyRaw: raw, privateKey: kp.privateKey };
}

/// EN: Load (or first-time create + persist) this device's peer keypair for `account`. The private
/// key is stored as an extractable JWK in localStorage — same device-local trust boundary as the
/// rest of the key vault. Returns null when there is no DOM storage (e.g. node). CN: 载入（或首次创建
/// 并持久化）本设备对 `account` 的对端密钥对。私钥以可导出 JWK 存于 localStorage——与密钥库其余部分同为
/// 设备本地信任边界。无 DOM 存储（如 node）时返回 null。
export async function getOrCreateDevicePeerKey(account: string): Promise<DevicePeerKey | null> {
  if (typeof localStorage === "undefined") return null;
  const raw = localStorage.getItem(lsKey(account));
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as { pub: string; jwk: JsonWebKey };
      const privateKey = await crypto.subtle.importKey("jwk", parsed.jwk, ECDH, true, ["deriveBits"]);
      return { publicKeyRaw: fromHex(parsed.pub), privateKey };
    } catch {
      /* fall through to regenerate */
    }
  }
  const kp = await crypto.subtle.generateKey(ECDH, true, ["deriveBits"]);
  const pub = new Uint8Array(await crypto.subtle.exportKey("raw", kp.publicKey));
  const jwk = await crypto.subtle.exportKey("jwk", kp.privateKey);
  localStorage.setItem(lsKey(account), JSON.stringify({ pub: toHex(pub), jwk }));
  return { publicKeyRaw: pub, privateKey: kp.privateKey };
}

function endorsementBytes(deviceId: string, peerPublicKeyRaw: Uint8Array): Uint8Array {
  const id = enc.encode(deviceId);
  const len = (n: number) => {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(n), true);
    return b;
  };
  return concat(enc.encode(DEVICE_PEER_ENDORSE_CONTEXT), len(id.length), id, len(peerPublicKeyRaw.length), peerPublicKeyRaw);
}

/// EN: Endorse a device's peer public key with the account device-directory key (§5.1). CN: 用账户
/// 设备目录钥背书某设备的对端公钥（§5.1）。
export async function endorseDevicePeerKey(
  dir: DeviceDirectoryKey,
  deviceId: string,
  peerPublicKeyRaw: Uint8Array,
): Promise<DevicePeerEndorsement> {
  const { ed25519Sign } = await import("@polkadot/util-crypto");
  const sig = ed25519Sign(endorsementBytes(deviceId, peerPublicKeyRaw), {
    publicKey: dir.publicKey,
    secretKey: dir.secretKey,
  });
  return { deviceId, peerPublicKey: toHex(peerPublicKeyRaw), sig: toHex(sig) };
}

/// EN: Verify a peer-key endorsement against the account device-directory public key. Never throws.
/// CN: 用账户设备目录公钥验证对端钥背书。绝不抛错。
export async function verifyDevicePeerEndorsement(
  dirPublicKey: Uint8Array,
  e: DevicePeerEndorsement,
): Promise<boolean> {
  try {
    const { ed25519Verify } = await import("@polkadot/util-crypto");
    return ed25519Verify(endorsementBytes(e.deviceId, fromHex(e.peerPublicKey)), fromHex(e.sig), dirPublicKey);
  } catch {
    return false;
  }
}

async function deriveSealKey(privateKey: CryptoKey, peerPublic: CryptoKey): Promise<CryptoKey> {
  const shared = new Uint8Array(
    await crypto.subtle.deriveBits({ name: "ECDH", public: peerPublic }, privateKey, 256),
  );
  const hk = await crypto.subtle.importKey("raw", shared as BufferSource, "HKDF", false, ["deriveKey"]);
  return crypto.subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt: enc.encode(SEAL_HKDF_SALT), info: new Uint8Array(0) as BufferSource },
    hk,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

/// EN: Seal `plaintext` to a recipient peer public key (anonymous-sender ECIES). Layout:
/// `ephemeral_pub(65) || iv(12) || AES-256-GCM(plaintext)`. CN: 把 `plaintext` 封装给接收方对端公钥
/// （匿名发送方 ECIES）。布局：`ephemeral_pub(65) || iv(12) || AES-256-GCM(明文)`。
export async function sealToPeer(peerPublicKeyRaw: Uint8Array, plaintext: Uint8Array): Promise<Uint8Array> {
  const eph = await crypto.subtle.generateKey(ECDH, true, ["deriveBits"]);
  const ephPub = new Uint8Array(await crypto.subtle.exportKey("raw", eph.publicKey));
  const key = await deriveSealKey(eph.privateKey, await importPublic(peerPublicKeyRaw));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-GCM", iv: iv as BufferSource }, key, plaintext as BufferSource),
  );
  return concat(ephPub, iv, ct);
}

/// EN: Open a sealed bundle with this device's peer private key. Returns null on any failure (wrong
/// recipient / tampered / truncated). CN: 用本设备对端私钥打开封装 bundle。任何失败（收件人不符/被篡改/
/// 截断）返回 null。
export async function openSealed(myPrivate: CryptoKey, sealed: Uint8Array): Promise<Uint8Array | null> {
  try {
    if (sealed.length < 65 + 12 + 16) return null;
    const ephPub = sealed.slice(0, 65);
    const iv = sealed.slice(65, 77);
    const ct = sealed.slice(77);
    const key = await deriveSealKey(myPrivate, await importPublic(ephPub));
    const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv: iv as BufferSource }, key, ct as BufferSource);
    return new Uint8Array(pt);
  } catch {
    return null;
  }
}

/// EN: Convenience: a complete sealed handoff payload = the §5 signed receipt (authority proof, plain)
/// + the sealed signing-key bundle (confidential). The receipt is NOT secret (it only proves who may
/// send); only the bundle is sealed. CN: 便捷：完整封装交接载荷 = §5 签名收据（权威证明，明文）+ 封装签名钥
/// bundle（机密）。收据**非**秘密（仅证明谁可发送）；仅 bundle 被封装。
export interface SealedHandoffPayload {
  receipt: SignedHandoffReceipt;
  sealedBundle: string;
}
