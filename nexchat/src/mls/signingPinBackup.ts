// EN: Track A PIN-wrapped MLS signing-key backup (design §5.3 path C). The signing private key is
// NEVER derivable from the mnemonic alone: `K_pin_wrap = HKDF(vault_master || pin_normalized)`.
// The sealed blob (IPFS) holds metadata + `exportSigningKeys()` bytes; the relay/manifest carry
// only `{cid, updated_at}`. CN: 路线 A PIN 包裹的 MLS 签名钥备份（设计 §5.3 路径 C）。签名私钥**不能**
// 单靠助记词派生：`K_pin_wrap = HKDF(vault_master || pin_normalized)`。密封 blob（IPFS）存元数据 +
// `exportSigningKeys()` 字节；relay/清单仅携带 `{cid, updated_at}`。

import { bytesToB64 } from "@/util/b64";

/// EN: HKDF salt for PIN-wrap key (domain-separated from K_mls_escrow / K_sync). CN: PIN 封装密钥的
/// HKDF salt（与 K_mls_escrow / K_sync 域分离）。
export const K_MLS_SIGNING_PIN_WRAP_SALT = "chat/mls-signing-backup/v1";

const enc = new TextEncoder();
const dec = new TextDecoder();

export const SIGNING_PIN_MIN_LEN = 6;
export const SIGNING_PIN_MAX_LEN = 8;

/// EN: Plaintext stored inside the PIN-sealed IPFS blob (before AES-GCM). CN: PIN 密封 IPFS blob 内的明文。
export interface SigningBackupPlain {
  v: 1;
  account: string;
  device_id: string;
  backup_seq: number;
  exported_at: number;
  /** base64( exportSigningKeys() ) */
  bundle: string;
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

async function hkdfBits(
  ikm: Uint8Array,
  salt: string,
  info: Uint8Array,
  bits: number,
): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", ikm as BufferSource, "HKDF", false, ["deriveBits"]);
  const out = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: enc.encode(salt), info: info as BufferSource },
    key,
    bits,
  );
  return new Uint8Array(out);
}

/// EN: Normalize user PIN: trim + NFKC; digits-only 6–8 chars. CN: 规范化用户 PIN：trim + NFKC；仅数字 6–8 位。
export function normalizeSigningPin(pin: string): string {
  const n = pin.trim().normalize("NFKC");
  if (!/^\d+$/.test(n)) {
    throw new Error(`PIN 须为 ${SIGNING_PIN_MIN_LEN}–${SIGNING_PIN_MAX_LEN} 位数字`);
  }
  if (n.length < SIGNING_PIN_MIN_LEN || n.length > SIGNING_PIN_MAX_LEN) {
    throw new Error(`PIN 须为 ${SIGNING_PIN_MIN_LEN}–${SIGNING_PIN_MAX_LEN} 位数字`);
  }
  return n;
}

/// EN: Derive the AES-256-GCM wrap key: IKM = vault_master || pin_normalized (both required).
/// CN: 派生 AES-256-GCM 封装密钥：IKM = vault_master || pin_normalized（二者缺一不可）。
export async function deriveMlsSigningPinWrapKey(
  vaultMaster: Uint8Array,
  anchorId: Uint8Array,
  pinNormalized: string,
): Promise<CryptoKey> {
  const pinBytes = enc.encode(pinNormalized);
  const ikm = concatBytes(vaultMaster, pinBytes);
  const bits = await hkdfBits(ikm, K_MLS_SIGNING_PIN_WRAP_SALT, anchorId, 256);
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

export function encodeSigningBackupPlain(plain: SigningBackupPlain): Uint8Array {
  return enc.encode(JSON.stringify(plain));
}

/// EN: Decode + validate signing backup plaintext. CN: 解码并校验签名备份明文。
export function decodeSigningBackupPlain(bytes: Uint8Array): SigningBackupPlain {
  const obj = JSON.parse(dec.decode(bytes)) as Partial<SigningBackupPlain>;
  if (
    obj.v !== 1 ||
    typeof obj.account !== "string" ||
    typeof obj.device_id !== "string" ||
    typeof obj.backup_seq !== "number" ||
    typeof obj.exported_at !== "number" ||
    typeof obj.bundle !== "string" ||
    !obj.bundle
  ) {
    throw new Error("invalid signing backup plaintext");
  }
  return {
    v: 1,
    account: obj.account,
    device_id: obj.device_id,
    backup_seq: obj.backup_seq,
    exported_at: obj.exported_at,
    bundle: obj.bundle,
  };
}

/// EN: Seal plaintext: wire = iv(12) || AES-256-GCM ciphertext. CN: 封装明文：wire = iv(12) || AES-256-GCM 密文。
export async function sealSigningBackup(key: CryptoKey, plain: SigningBackupPlain): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    encodeSigningBackupPlain(plain) as BufferSource,
  );
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  return out;
}

/// EN: Open a sealed backup blob. Throws on wrong PIN/key or corrupt blob. CN: 打开密封备份 blob；PIN/密钥
/// 错误或 blob 损坏时抛错。
export async function openSigningBackup(key: CryptoKey, packed: Uint8Array): Promise<SigningBackupPlain> {
  if (packed.length < 13) throw new Error("signing backup blob too short");
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: packed.slice(0, 12) as BufferSource },
    key,
    packed.slice(12) as BufferSource,
  );
  return decodeSigningBackupPlain(new Uint8Array(pt));
}

/// EN: Build plaintext from a raw signing-key export. CN: 由原始签名钥导出构建明文。
export function buildSigningBackupPlain(args: {
  account: string;
  deviceId: string;
  backupSeq: number;
  bundle: Uint8Array;
  exportedAt?: number;
}): SigningBackupPlain {
  return {
    v: 1,
    account: args.account,
    device_id: args.deviceId,
    backup_seq: args.backupSeq,
    exported_at: args.exportedAt ?? Date.now(),
    bundle: bytesToB64(args.bundle),
  };
}

export function signingBundleFromPlain(plain: SigningBackupPlain): Uint8Array {
  const raw = atob(plain.bundle);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}
