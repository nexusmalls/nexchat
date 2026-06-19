// EN: NEX transfer via `balances.transferKeepAlive`.
// CN: 通过 `balances.transferKeepAlive` 转账 NEX。

import { cryptoWaitReady, decodeAddress } from "@polkadot/util-crypto";
import { chainClient } from "@/chain/chainClient";

export async function isValidSs58Address(address: string): Promise<boolean> {
  try {
    await cryptoWaitReady();
    decodeAddress(address.trim());
    return true;
  } catch {
    return false;
  }
}

// EN: Send NEX to recipient (planck amount).
// CN: 向收款地址发送 NEX（planck 金额）。
export async function transferNex(to: string, amountPlanck: bigint): Promise<string> {
  if (amountPlanck <= 0n) throw new Error("金额必须大于 0");
  if (!(await isValidSs58Address(to))) throw new Error("收款地址无效");
  return chainClient.signAndSend("balances", "transferKeepAlive", [to.trim(), amountPlanck]);
}
