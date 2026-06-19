// EN: SS58 account normalization (Nexus prefix 273) shared by relay scripts.
// CN: Relay 脚本共用的 SS58 账户规范化（Nexus 前缀 273）。

import { decodeAddress, encodeAddress } from "@polkadot/util-crypto";

const RPC_SS58 = 273;

export function normalizeAccount(addr) {
  if (!addr || typeof addr !== "string") return addr;
  try {
    return encodeAddress(decodeAddress(addr), RPC_SS58);
  } catch {
    return addr;
  }
}
