// EN: Canonical SS58 for RPC / MLS / relay — the Nexus chain prefix 273 (matches the wallet
// display prefix). Any input prefix is normalized to 273 so both ends agree on keys.
// CN: RPC / MLS / relay 用的规范 SS58——Nexus 链前缀 273（与钱包展示前缀一致）。任意输入前缀
// 都规范化到 273，使两端键一致。

import { decodeAddress, encodeAddress } from "@polkadot/util-crypto";
import { NEX_SS58 } from "@/wallet/desktopKeyring";

/** EN: Nexus chain SS58 prefix (runtime SS58Prefix = 273); canonical for RPC / MLS / relay keys. CN: Nexus 链 SS58 前缀（runtime SS58Prefix = 273）；RPC / MLS / relay 键的规范前缀。 */
export const RPC_SS58 = 273;

/// EN: Normalize any valid SS58 to RPC_SS58 so both ends agree on MLS keys and queries.
/// CN: 将合法 SS58 规范化为 RPC_SS58，使两端 MLS 键与查询一致。
export function canonicalAddress(addr: string): string {
  try {
    return encodeAddress(decodeAddress(addr), RPC_SS58);
  } catch {
    return addr;
  }
}

/// EN: Return normalized SS58 or null when `addr` is not decodable. CN: 可解码则返回规范 SS58，否则 null。
export function tryCanonicalAddress(addr: string): string | null {
  try {
    return encodeAddress(decodeAddress(addr), RPC_SS58);
  } catch {
    return null;
  }
}

export function shortAddress(addr: string, head = 8, tail = 4): string {
  if (addr.length <= head + tail + 1) return addr;
  return `${addr.slice(0, head)}…${addr.slice(-tail)}`;
}

/// EN: Wallet-facing NEX SS58 (prefix 273, typically starts with `X`). CN: 钱包展示用 NEX SS58（前缀 273，通常以 X 开头）。
export function nexDisplayAddress(addr: string): string {
  try {
    return encodeAddress(decodeAddress(addr), NEX_SS58);
  } catch {
    return addr;
  }
}

/// EN: Truncated NEX display address for list rows. CN: 列表用的 NEX 地址缩写。
export function shortNexAddress(addr: string, head = 6, tail = 4): string {
  return shortAddress(nexDisplayAddress(addr), head, tail);
}
