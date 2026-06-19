// EN: Commission extrinsics via ChainClient.signAndSend.
// CN: 佣金 extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

// EN: Withdraw pending NEX commission (`commissionCore.withdrawCommission`).
// CN: 提取待领取 NEX 佣金。
export async function withdrawCommission(
  entityId: number,
  amount: string | null,
): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("commissionCore", "withdrawCommission", [
    entityId,
    amount,
    null,
  ]);
}

// EN: Claim pool reward for current round (`commissionPoolReward.claimPoolReward`).
// CN: 领取当前轮次奖池奖励。
export async function claimPoolReward(entityId: number): Promise<string> {
  ensureLive();
  return chainClient.signAndSend("commissionPoolReward", "claimPoolReward", [entityId]);
}
