// EN: Entity registry extrinsics via ChainClient.signAndSend.
// CN: Entity registry extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

export interface CreateEntityParams {
  name: string;
  logoCid?: string | null;
  descriptionCid?: string | null;
  referrer?: string | null;
}

// EN: Register entity on chain; auto-creates primary shop (`entityRegistry.createEntity`).
// CN: 链上注册 Entity；自动创建主店铺（`entityRegistry.createEntity`）。
export async function createEntity(params: CreateEntityParams): Promise<string> {
  ensureLive();
  const name = params.name.trim();
  if (!name) throw new Error("店铺名称不能为空");
  const logo = params.logoCid?.trim() || null;
  const desc = params.descriptionCid?.trim() || null;
  const referrer = params.referrer?.trim() || null;
  return chainClient.signAndSend("entityRegistry", "createEntity", [name, logo, desc, referrer]);
}
