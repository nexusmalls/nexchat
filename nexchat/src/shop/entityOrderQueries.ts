// EN: On-chain entity order read queries (entityTransaction).
// CN: 链上 Entity 订单只读查询。

import type { EntityOrder, PaymentAsset, ProductCategory } from "@/shop/types";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
};

export type EntityOrderApi = {
  query: {
    entityTransaction: Record<string, StorageQuery>;
  };
};

function bytesToString(raw: unknown): string {
  if (raw == null) return "";
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) {
    return String.fromCharCode(...(raw as number[]).filter((b) => b > 0));
  }
  if (raw instanceof Uint8Array) {
    return new TextDecoder().decode(raw);
  }
  return String(raw);
}

function parseChainEnum<T extends string>(raw: unknown, fallback: T): T {
  if (typeof raw === "string") return raw as T;
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw)[0];
    if (key) return parseChainEnum(key, fallback);
  }
  return fallback;
}

function parseOrder(data: Record<string, unknown>): EntityOrder {
  const shipping = data.shippingCid ?? data.shipping_cid;
  const tracking = data.trackingCid ?? data.tracking_cid;
  const note = data.noteCid ?? data.note_cid;
  const refund = data.refundReasonCid ?? data.refund_reason_cid;
  return {
    id: Number(data.id ?? 0),
    entityId: Number(data.entityId ?? data.entity_id ?? 0),
    shopId: Number(data.shopId ?? data.shop_id ?? 0),
    productId: Number(data.productId ?? data.product_id ?? 0),
    buyer: String(data.buyer ?? ""),
    seller: String(data.seller ?? ""),
    payer: data.payer != null ? String(data.payer) : null,
    quantity: Number(data.quantity ?? 1),
    unitPrice: String(data.unitPrice ?? data.unit_price ?? "0"),
    totalAmount: String(data.totalAmount ?? data.total_amount ?? "0"),
    platformFee: String(data.platformFee ?? data.platform_fee ?? "0"),
    productCategory: parseChainEnum<ProductCategory>(
      data.productCategory ?? data.product_category,
      "Physical",
    ),
    shippingCid: shipping ? bytesToString(shipping) : null,
    trackingCid: tracking ? bytesToString(tracking) : null,
    status: parseChainEnum(data.status, "Paid"),
    createdAt: Number(data.createdAt ?? data.created_at ?? 0),
    shippedAt:
      data.shippedAt != null || data.shipped_at != null
        ? Number(data.shippedAt ?? data.shipped_at)
        : null,
    completedAt:
      data.completedAt != null || data.completed_at != null
        ? Number(data.completedAt ?? data.completed_at)
        : null,
    paymentAsset: parseChainEnum<PaymentAsset>(
      data.paymentAsset ?? data.payment_asset,
      "Native",
    ),
    tokenPaymentAmount: String(
      data.tokenPaymentAmount ?? data.token_payment_amount ?? "0",
    ),
    shoppingBalanceUsed: String(
      data.shoppingBalanceUsed ?? data.shopping_balance_used ?? "0",
    ),
    confirmExtended: Boolean(data.confirmExtended ?? data.confirm_extended ?? false),
    disputeRejected: Boolean(data.disputeRejected ?? data.dispute_rejected ?? false),
    disputeDeadline:
      data.disputeDeadline != null || data.dispute_deadline != null
        ? Number(data.disputeDeadline ?? data.dispute_deadline)
        : null,
    noteCid: note ? bytesToString(note) : null,
    refundReasonCid: refund ? bytesToString(refund) : null,
  };
}

type RawOption = {
  isNone?: boolean;
  unwrap?: () => { toJSON: () => Record<string, unknown> };
};

// EN: Fetch single order by id.
// CN: 按 id 拉取单个订单。
export async function fetchOrder(
  api: EntityOrderApi,
  orderId: number,
): Promise<EntityOrder | null> {
  const q = api.query.entityTransaction;
  if (!q?.orders) return null;
  const raw = (await q.orders(orderId)) as RawOption;
  if (raw?.isNone) return null;
  return parseOrder(raw.unwrap!().toJSON());
}

// EN: Fetch buyer order list (newest first).
// CN: 拉取买家订单列表（按创建时间倒序）。
export async function fetchBuyerOrders(
  api: EntityOrderApi,
  buyer: string,
): Promise<EntityOrder[]> {
  const q = api.query.entityTransaction;
  if (!q?.buyerOrders || !q?.orders) return [];
  const idsRaw = (await q.buyerOrders(buyer)) as { toJSON?: () => number[] };
  const ids: number[] = idsRaw?.toJSON?.() ?? [];
  const orders: EntityOrder[] = [];
  for (const id of ids) {
    const raw = (await q.orders(id)) as RawOption;
    if (raw?.isNone) continue;
    orders.push(parseOrder(raw.unwrap!().toJSON()));
  }
  return orders.sort((a, b) => b.createdAt - a.createdAt);
}

// EN: Fetch shop order list (newest first).
// CN: 拉取商铺订单列表（按创建时间倒序）。
export async function fetchShopOrders(
  api: EntityOrderApi,
  shopId: number,
): Promise<EntityOrder[]> {
  const q = api.query.entityTransaction;
  if (!q?.shopOrders || !q?.orders) return [];
  const idsRaw = (await q.shopOrders(shopId)) as { toJSON?: () => number[] };
  const ids: number[] = idsRaw?.toJSON?.() ?? [];
  const orders: EntityOrder[] = [];
  for (const id of ids) {
    const raw = (await q.orders(id)) as RawOption;
    if (raw?.isNone) continue;
    orders.push(parseOrder(raw.unwrap!().toJSON()));
  }
  return orders.sort((a, b) => b.createdAt - a.createdAt);
}

// EN: Aggregate orders across managed shops (deduped by order id).
// CN: 聚合多个商铺订单（按 order id 去重）。
export async function fetchSellerOrders(
  api: EntityOrderApi,
  shopIds: number[],
): Promise<EntityOrder[]> {
  const unique = [...new Set(shopIds)].filter((id) => id > 0);
  if (unique.length === 0) return [];
  const byId = new Map<number, EntityOrder>();
  for (const shopId of unique) {
    const rows = await fetchShopOrders(api, shopId);
    for (const order of rows) byId.set(order.id, order);
  }
  return [...byId.values()].sort((a, b) => b.createdAt - a.createdAt);
}
