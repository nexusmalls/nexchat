// EN: sr25519 account binding for relay `register_account` (parity with relay-core
// `register_account_sign_payload`). CN: relay `register_account` 的 sr25519 账户绑定（与 relay-core
// `register_account_sign_payload` 一致）。

import { decodeAddress, encodeAddress, base64Encode } from "@polkadot/util-crypto";
import { signRawWithAccountKey } from "@/chain/signer";
import { config } from "@/config";

const REGISTER_PREFIX = new TextEncoder().encode("nexchat-relay-register-v1\0");

/// EN: Normalize to relay storage prefix 42 (matches relay-core SS58 normalize). CN: 规范化为 relay 存储前缀 42。
export function normalizeRelayAccount(account: string): string {
  try {
    return encodeAddress(decodeAddress(account), 42);
  } catch {
    return account;
  }
}

/// EN: Canonical v1 signing payload bytes. CN: 规范 v1 签名载荷字节。
export function buildRegisterAccountSignPayload(endpointId: string, account: string): Uint8Array {
  const norm = normalizeRelayAccount(account);
  const enc = new TextEncoder();
  const idBytes = enc.encode(endpointId);
  const acctBytes = enc.encode(norm);
  const out = new Uint8Array(REGISTER_PREFIX.length + idBytes.length + 1 + acctBytes.length);
  let off = 0;
  out.set(REGISTER_PREFIX, off);
  off += REGISTER_PREFIX.length;
  out.set(idBytes, off);
  off += idBytes.length;
  out[off] = 0;
  off += 1;
  out.set(acctBytes, off);
  return out;
}

/// EN: Base64 sr25519 signature for `register_account`, or null when no keyring pair is active.
/// CN: `register_account` 的 base64 sr25519 签名；无 keyring pair 时返回 null。
export function signRegisterAccount(endpointId: string, account: string): string | null {
  const sig = signRawWithAccountKey(buildRegisterAccountSignPayload(endpointId, account));
  if (!sig) return null;
  return base64Encode(sig);
}

/// EN: Wire fields for `register_account` (includes `account_sig` only when `config.relayStrictAuth`).
/// CN: `register_account` wire 字段（仅 `config.relayStrictAuth` 为 true 时附带 `account_sig`）。
export function registerAccountWire(
  endpointId: string,
  account: string,
): { type: "register_account"; id: string; account: string; account_sig?: string } {
  const wire: { type: "register_account"; id: string; account: string; account_sig?: string } = {
    type: "register_account",
    id: endpointId,
    account,
  };
  if (config.relayStrictAuth) {
    const sig = signRegisterAccount(endpointId, account);
    if (sig) wire.account_sig = sig;
  }
  return wire;
}
