// EN: Entity product extrinsics via ChainClient.signAndSend.
// CN: Entity 商品 extrinsic（ChainClient.signAndSend）。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import type { ProductCategory, ProductVisibility } from "@/shop/types";

function ensureLive(): void {
  if (config.useMock) {
    throw new Error("Mock 模式无法提交链上交易，请设置 VITE_USE_MOCK=false");
  }
}

export interface CreateProductParams {
  shopId: number;
  nameCid: string;
  imagesCid: string;
  detailCid: string;
  usdtPrice: number;
  stock: number;
  category: ProductCategory;
  minOrderQuantity: number;
  maxOrderQuantity: number;
  visibility: ProductVisibility;
  sortWeight?: number;
  tagsCid?: string;
  skuCid?: string;
}

// EN: Create product in draft status (`entityProduct.createProduct`).
// CN: 创建草稿商品（`entityProduct.createProduct`）。
export async function createProduct(params: CreateProductParams): Promise<string> {
  ensureLive();
  const nameCid = params.nameCid.trim();
  const imagesCid = params.imagesCid.trim();
  const detailCid = params.detailCid.trim();
  if (!nameCid || !imagesCid || !detailCid) {
    throw new Error("名称、图片、详情 CID 不能为空");
  }
  if (!Number.isFinite(params.usdtPrice) || params.usdtPrice <= 0) {
    throw new Error("USDT 价格必须大于 0");
  }
  if (!Number.isInteger(params.stock) || params.stock < 0) {
    throw new Error("库存须为非负整数（0 表示无限）");
  }
  if (!Number.isInteger(params.minOrderQuantity) || params.minOrderQuantity < 1) {
    throw new Error("最小购买量须 >= 1");
  }
  if (
    params.maxOrderQuantity > 0 &&
    params.maxOrderQuantity < params.minOrderQuantity
  ) {
    throw new Error("最大购买量须 >= 最小购买量（或为 0 表示不限）");
  }
  if (params.category === "Subscription" || params.category === "Bundle") {
    throw new Error("暂不支持订阅/组合类商品");
  }

  return chainClient.signAndSend("entityProduct", "createProduct", [
    params.shopId,
    nameCid,
    imagesCid,
    detailCid,
    params.usdtPrice,
    params.stock,
    params.category,
    params.sortWeight ?? 0,
    params.tagsCid?.trim() ?? "",
    params.skuCid?.trim() ?? "",
    params.minOrderQuantity,
    params.maxOrderQuantity,
    params.visibility,
  ]);
}

// EN: Publish draft/off-shelf product (`entityProduct.publishProduct`).
// CN: 上架商品（`entityProduct.publishProduct`）。
export async function publishProduct(productId: number): Promise<string> {
  ensureLive();
  if (!Number.isFinite(productId) || productId <= 0) {
    throw new Error("无效的商品 ID");
  }
  return chainClient.signAndSend("entityProduct", "publishProduct", [productId]);
}
