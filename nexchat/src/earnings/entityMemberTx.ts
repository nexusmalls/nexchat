// EN: Entity member extrinsics via ChainClient.signAndSend.
// CN: Entity 会员 extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

// EN: Register as entity member via primary shop (`entityMember.registerMember`).
// CN: 通过主商铺注册为 Entity 会员（`entityMember.registerMember`）。
export async function registerEntityMember(
  shopId: number,
  referrer: string | null = null,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("entityMember", "registerMember", [shopId, referrer]);
}
