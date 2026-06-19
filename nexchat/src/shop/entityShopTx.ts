// EN: Entity shop extrinsics via ChainClient.signAndSend.
// CN: Entity 商铺 extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { parseNexAmount } from "@/wallet/amount";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

export type ShopType = "OnlineStore" | "PhysicalStore" | "ServiceCenter" | string;

export interface CreateShopParams {
  entityId: number;
  name: string;
  shopType?: ShopType;
  initialFundNex: string;
}

// EN: Create branch shop under entity (`entityShop.createShop`; entity owner only).
// CN: 在 Entity 下创建分店（`entityShop.createShop`；仅 Entity owner）。
export async function createShop(params: CreateShopParams): Promise<string> {
  ensureLive();
  const name = params.name.trim();
  if (!name) throw new Error("店铺名称不能为空");
  if (!Number.isFinite(params.entityId) || params.entityId <= 0) {
    throw new Error("无效的 Entity ID");
  }
  const fund = parseNexAmount(params.initialFundNex);
  if (fund == null || fund <= 0n) {
    throw new Error("初始运营资金须为有效 NEX 金额");
  }
  return chainClient.signAndSend("entityShop", "createShop", [
    params.entityId,
    name,
    params.shopType ?? "OnlineStore",
    fund.toString(),
  ]);
}
