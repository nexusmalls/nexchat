// EN: v2 payer unlinking for the sync anchor (ADR CHAT_SYNC_ANCHOR §11.1 / P3).
// Authorization is always the anchor key's Ed25519 signature; the extrinsic origin only
// pays fees + deposit. In "burner" mode the origin becomes a DEDICATED payer account
// derived from `vault_master` (deterministic → recoverable from the mnemonic, and the
// clear refund goes back to an account the user still controls). HONEST DISCLOSURE: the
// one-time funding transfer main → payer is visible on-chain; what disappears is the
// main account's per-publish cadence/fee trail (§5.7). Storage & authorization migrate
// zero bytes — only the origin changes.
// CN: 同步锚 v2 付费方断链（ADR CHAT_SYNC_ANCHOR §11.1 / P3）。授权永远是锚密钥的
// Ed25519 签名；extrinsic origin 仅付手续费+押金。"burner" 模式下 origin 换成由
// `vault_master` 派生的**专用付费账户**（确定性→凭助记词可重算，clear 退押金仍回到用户
// 控制的账户）。诚实披露：主账户→payer 的一次性充值转账链上可见；消失的是主账户的逐次
// publish 节奏/手续费痕迹（§5.7）。存储与授权零迁移——仅 origin 变化。

import type { KeyringPair } from "@polkadot/keyring/types";

/// EN: Frozen derivation salt — changing it would orphan existing payer deposits.
/// CN: 冻结的派生 salt——更改会使既有 payer 押金失联。
export const SYNC_PAYER_SALT = "chat/sync-payer/v1";

/// EN: Keep the payer at ~2 NEX; top up when below 1 NEX (deposit 0.5 + fee headroom).
/// CN: payer 维持约 2 NEX；低于 1 NEX 时充值（押金 0.5 + 手续费余量）。
export const PAYER_MIN_FREE = 1_000_000_000_000n; // 1 NEX
export const PAYER_TARGET_FREE = 2_000_000_000_000n; // 2 NEX

/// EN: Pure top-up decision: amount to transfer from the main account (0n when funded).
/// CN: 纯充值判定：需从主账户转入的金额（已充足时为 0n）。
export function payerTopUpAmount(free: bigint): bigint {
  return free >= PAYER_MIN_FREE ? 0n : PAYER_TARGET_FREE - free;
}

/// EN: Derive the dedicated payer pair: sr25519 from a 32-byte HKDF-SHA256 of
/// `vault_master` with the frozen salt (no info — single payer per account domain,
/// already separated by vault_master itself). CN: 派生专用付费 pair：以冻结 salt 对
/// `vault_master` 做 HKDF-SHA256 取 32 字节 sr25519 seed（无 info——每账户域单一 payer，
/// vault_master 本身已做账户分离）。
export async function deriveSyncPayerPair(vaultMaster: Uint8Array): Promise<KeyringPair> {
  const { Keyring } = await import("@polkadot/keyring");
  const { cryptoWaitReady } = await import("@polkadot/util-crypto");
  await cryptoWaitReady();
  const key = await crypto.subtle.importKey("raw", vaultMaster as BufferSource, "HKDF", false, [
    "deriveBits",
  ]);
  const seedBits = await crypto.subtle.deriveBits(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt: new TextEncoder().encode(SYNC_PAYER_SALT),
      info: new Uint8Array(0),
    },
    key,
    256,
  );
  const seed = new Uint8Array(seedBits);
  try {
    return new Keyring({ type: "sr25519", ss58Format: 273 }).addFromSeed(seed);
  } finally {
    seed.fill(0);
  }
}
