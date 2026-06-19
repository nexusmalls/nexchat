// EN: vault_master derivation (ADR CHAT_SYNC_ANCHOR §5.0). The KeyVault HKDF root must
// NOT be derivable from public data: it is derived from the unlocked KeyringPair's 64-byte
// sr25519 expanded secret (extracted from the unencrypted PKCS8 encoding), so only a party
// holding the mnemonic / unlocked keystore can compute it — yet it is deterministic across
// devices. This module is the single extraction + derivation point, frozen by test vectors.
// CN: vault_master 派生（ADR CHAT_SYNC_ANCHOR §5.0）。KeyVault 的 HKDF 根**不得**由公开数据
// 派生：它从已解锁 KeyringPair 的 64 字节 sr25519 expanded secret（无密码 PKCS8 编码解包）
// 派生，只有持有助记词/已解锁 keystore 的一方可算，且跨设备确定。本模块是唯一的提取 +
// 派生入口，由固定测试向量冻结。

import type { KeyringPair } from "@polkadot/keyring/types";

// EN: PKCS8 layout constants for an UNENCRYPTED polkadot-js pair encoding:
//   header(16) || secret(64) || divider(5) || publicKey(32)  — total 117 bytes.
// Frozen here (with the embedded-pubkey cross-check below) so a silent polkadot-js
// layout change fails loudly instead of deriving a wrong master.
// CN: polkadot-js **未加密** pair PKCS8 编码的固定布局常量：
//   header(16) || secret(64) || divider(5) || publicKey(32) —— 共 117 字节。
// 在此冻结（配合下方内嵌公钥交叉校验），polkadot-js 布局若变更会显式失败而非静默派生错误主钥。
const PKCS8_HEADER = Uint8Array.from([48, 83, 2, 1, 1, 48, 5, 6, 3, 43, 101, 112, 4, 34, 4, 32]);
const PKCS8_DIVIDER = Uint8Array.from([161, 35, 3, 33, 0]);
const SECRET_LEN = 64;
const PUBLIC_LEN = 32;

const VAULT_MASTER_SALT = "nexchat/vault-master/v1";

function bytesEq(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/// EN: Extract the 64-byte sr25519 expanded secret from an unlocked pair via its
/// unencrypted PKCS8 encoding. Throws if the pair is locked or the layout/embedded
/// public key does not match (ADR §5.0 unified extraction rule — works for mnemonic,
/// keystore-JSON imports and dev `//Uri` pairs alike).
/// CN: 经无密码 PKCS8 编码从已解锁 pair 提取 64 字节 sr25519 expanded secret。pair 未解锁、
/// 布局或内嵌公钥不匹配时抛错（ADR §5.0 统一提取规则——对助记词、keystore JSON 导入、
/// dev `//Uri` pair 一致适用）。
export function extractSr25519Secret(pair: KeyringPair): Uint8Array {
  if (pair.isLocked) throw new Error("vaultMaster: pair is locked, unlock before deriving");
  const pkcs8 = pair.encodePkcs8();
  const expected = PKCS8_HEADER.length + SECRET_LEN + PKCS8_DIVIDER.length + PUBLIC_LEN;
  if (pkcs8.length !== expected) {
    throw new Error(`vaultMaster: unexpected PKCS8 length ${pkcs8.length} (want ${expected})`);
  }
  if (!bytesEq(pkcs8.subarray(0, PKCS8_HEADER.length), PKCS8_HEADER)) {
    throw new Error("vaultMaster: PKCS8 header mismatch (polkadot-js layout changed?)");
  }
  const dividerOff = PKCS8_HEADER.length + SECRET_LEN;
  if (!bytesEq(pkcs8.subarray(dividerOff, dividerOff + PKCS8_DIVIDER.length), PKCS8_DIVIDER)) {
    throw new Error("vaultMaster: PKCS8 divider mismatch (polkadot-js layout changed?)");
  }
  const embeddedPub = pkcs8.subarray(dividerOff + PKCS8_DIVIDER.length, expected);
  if (!bytesEq(embeddedPub, pair.publicKey)) {
    throw new Error("vaultMaster: PKCS8 embedded public key mismatch");
  }
  return pkcs8.slice(PKCS8_HEADER.length, PKCS8_HEADER.length + SECRET_LEN);
}

/// EN: vault_master = HKDF-SHA256(ikm = sr25519_secret(64B), salt = "nexchat/vault-master/v1",
/// info = ss58(prefix 273)). 32 bytes; account-domain-separated via the Nexus prefix-273 address.
/// CN: vault_master = HKDF-SHA256(ikm = sr25519_secret(64B), salt = "nexchat/vault-master/v1",
/// info = ss58(prefix 273))。32 字节；用 Nexus prefix-273 地址做账户域分离。
export async function deriveVaultMasterFromPair(pair: KeyringPair): Promise<Uint8Array> {
  const secret = extractSr25519Secret(pair);
  try {
    const { encodeAddress } = await import("@polkadot/util-crypto");
    const ss58 = encodeAddress(pair.publicKey, 273);
    return await hkdfSha256(secret, VAULT_MASTER_SALT, ss58);
  } finally {
    secret.fill(0);
  }
}

/// EN: Derive vault_master from a dev `//Uri` / mnemonic suri (creates a throwaway pair).
/// CN: 从 dev `//Uri` / 助记词 suri 派生 vault_master（创建临时 pair）。
export async function deriveVaultMasterFromSuri(suri: string): Promise<Uint8Array> {
  const { Keyring } = await import("@polkadot/keyring");
  const { cryptoWaitReady } = await import("@polkadot/util-crypto");
  await cryptoWaitReady();
  const pair = new Keyring({ type: "sr25519", ss58Format: 273 }).addFromUri(suri);
  return deriveVaultMasterFromPair(pair);
}

async function hkdfSha256(ikm: Uint8Array, salt: string, info: string): Promise<Uint8Array> {
  const enc = new TextEncoder();
  const key = await crypto.subtle.importKey("raw", ikm as BufferSource, "HKDF", false, [
    "deriveBits",
  ]);
  const bits = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: enc.encode(salt), info: enc.encode(info) },
    key,
    256,
  );
  return new Uint8Array(bits);
}

// EN: Process-wide holder: set at wallet unlock, consumed by appStore.unlock → keyVault.init,
// zeroized at lock. CN: 进程级持有器：钱包解锁时写入，appStore.unlock → keyVault.init 消费，
// 锁定时清零。
let current: Uint8Array | null = null;

export function setVaultMaster(master: Uint8Array): void {
  clearVaultMaster();
  current = master;
}

export function getVaultMaster(): Uint8Array | null {
  return current;
}

export function clearVaultMaster(): void {
  if (current) current.fill(0);
  current = null;
}
