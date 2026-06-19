// EN: On-chain entity shop/product read queries (entityShop + entityProduct).
// CN: 链上 Entity 商铺/商品只读查询。

import { fetchMembershipsForEntities } from "@/shop/entityMemberQueries";
import { buildCatalog } from "@/shop/sort";
import { canonicalAddress } from "@/wallet/address";
import type {
  EntityMemberInfo,
  EntityProduct,
  EntityShop,
  ProductCategory,
  ProductStatus,
  ProductVisibility,
  ShopCatalog,
} from "@/shop/types";
import { decodeChainText } from "@/mls/chainBytes";
import { parseVisibility } from "@/shop/visibility";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
  entries?: () => Promise<unknown>;
};

export type EntityShopApi = {
  query: {
    entityShop: Record<string, StorageQuery>;
    entityProduct: Record<string, StorageQuery>;
  };
};

function parseChainEnum<T extends string>(raw: unknown, fallback: T): T {
  if (typeof raw === "string") {
    if (raw === "onSale") return "OnSale" as T;
    if (raw === "soldOut") return "SoldOut" as T;
    if (raw === "offShelf") return "OffShelf" as T;
    if (raw === "membersOnly") return "MembersOnly" as T;
    if (raw === "levelGated") return "LevelGated" as T;
    return raw as T;
  }
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw)[0];
    if (key) return parseChainEnum(key, fallback);
  }
  return fallback;
}

function parseShop(data: Record<string, unknown>): EntityShop {
  const logo = data.logoCid ?? data.logo_cid;
  const desc = data.descriptionCid ?? data.description_cid;
  return {
    id: Number(data.id ?? 0),
    entityId: Number(data.entityId ?? data.entity_id ?? 0),
    name: decodeChainText(data.name),
    logoCid: logo ? decodeChainText(logo) : null,
    descriptionCid: desc ? decodeChainText(desc) : null,
    status: parseChainEnum(data.status, "Active"),
    managers: Array.isArray(data.managers) ? (data.managers as string[]) : [],
    productCount: Number(data.productCount ?? data.product_count ?? 0),
    totalSales: String(data.totalSales ?? data.total_sales ?? "0"),
    totalOrders: Number(data.totalOrders ?? data.total_orders ?? 0),
    createdAt: Number(data.createdAt ?? data.created_at ?? 0),
  };
}

function parseProduct(data: Record<string, unknown>): EntityProduct {
  const vis = parseVisibility(data.visibility);
  return {
    id: Number(data.id ?? 0),
    shopId: Number(data.shopId ?? data.shop_id ?? 0),
    nameCid: decodeChainText(data.nameCid ?? data.name_cid),
    imagesCid: decodeChainText(data.imagesCid ?? data.images_cid),
    detailCid: decodeChainText(data.detailCid ?? data.detail_cid),
    price: String(data.price ?? "0"),
    usdtPrice: Number(data.usdtPrice ?? data.usdt_price ?? 0),
    stock: Number(data.stock ?? 0),
    soldCount: Number(data.soldCount ?? data.sold_count ?? 0),
    status: parseChainEnum<ProductStatus>(data.status, "Draft"),
    visibility: vis.kind as ProductVisibility,
    visibilityMinLevel: vis.minLevel,
    category: parseChainEnum<ProductCategory>(data.category, "Physical"),
    sortWeight: Number(data.sortWeight ?? data.sort_weight ?? 0),
    minOrderQuantity: Number(data.minOrderQuantity ?? data.min_order_quantity ?? 0),
    maxOrderQuantity: Number(data.maxOrderQuantity ?? data.max_order_quantity ?? 0),
    createdAt: Number(data.createdAt ?? data.created_at ?? 0),
  };
}

async function fetchAllShops(api: EntityShopApi): Promise<EntityShop[]> {
  const q = api.query.entityShop;
  if (!q?.shops?.entries) return [];
  const entries = (await q.shops.entries()) as Array<
    [unknown, { isNone?: boolean; unwrap?: () => { toJSON: () => Record<string, unknown> } }]
  >;
  const shops: EntityShop[] = [];
  for (const [, raw] of entries) {
    if (raw?.isNone) continue;
    shops.push(parseShop(raw.unwrap!().toJSON()));
  }
  return shops;
}

async function fetchAllProducts(api: EntityShopApi): Promise<EntityProduct[]> {
  const q = api.query.entityProduct;
  if (!q?.products?.entries) return [];
  const entries = (await q.products.entries()) as Array<
    [unknown, { isNone?: boolean; unwrap?: () => { toJSON: () => Record<string, unknown> } }]
  >;
  const products: EntityProduct[] = [];
  for (const [, raw] of entries) {
    if (raw?.isNone) continue;
    products.push(parseProduct(raw.unwrap!().toJSON()));
  }
  return products;
}

export type FetchShopCatalogOptions = {
  viewerAddress?: string | null;
};

// EN: Load full shop catalog (shops + viewer-visible products sorted by sales).
// CN: 加载完整购物目录（商铺 + 当前用户可见商品，按销量排序）。
export async function fetchShopCatalog(
  api: EntityShopApi,
  options?: FetchShopCatalogOptions | Map<number, EntityMemberInfo>,
): Promise<ShopCatalog> {
  const [shops, products] = await Promise.all([fetchAllShops(api), fetchAllProducts(api)]);

  let memberByEntity = new Map<number, EntityMemberInfo>();
  if (options instanceof Map) {
    memberByEntity = options;
  } else if (options?.viewerAddress) {
    const memberApi = api as unknown as Parameters<typeof fetchMembershipsForEntities>[0];
    const entityIds = shops.map((s) => s.entityId);
    memberByEntity = await fetchMembershipsForEntities(
      memberApi,
      entityIds,
      canonicalAddress(options.viewerAddress),
    );
  }

  return buildCatalog(shops, products, memberByEntity);
}

// EN: Fetch single product by id.
// CN: 按 id 拉取单个商品。
export async function fetchProduct(
  api: EntityShopApi,
  productId: number,
): Promise<EntityProduct | null> {
  const q = api.query.entityProduct;
  if (!q?.products) return null;
  const raw = (await q.products(productId)) as {
    isNone?: boolean;
    unwrap?: () => { toJSON: () => Record<string, unknown> };
  };
  if (raw?.isNone) return null;
  return parseProduct(raw.unwrap!().toJSON());
}

// EN: Fetch single shop by id.
// CN: 按 id 拉取单个商铺。
export async function fetchShop(
  api: EntityShopApi,
  shopId: number,
): Promise<EntityShop | null> {
  const q = api.query.entityShop;
  if (!q?.shops) return null;
  const raw = (await q.shops(shopId)) as {
    isNone?: boolean;
    unwrap?: () => { toJSON: () => Record<string, unknown> };
  };
  if (raw?.isNone) return null;
  return parseShop(raw.unwrap!().toJSON());
}

// EN: List shops belonging to one entity.
// CN: 列出某 Entity 下的商铺。
export async function fetchShopsByEntity(
  api: EntityShopApi,
  entityId: number,
): Promise<EntityShop[]> {
  const shops = await fetchAllShops(api);
  return shops.filter((s) => s.entityId === entityId).sort((a, b) => a.id - b.id);
}

// EN: Product ids registered under a shop.
// CN: 某店铺下的商品 id 列表。
export async function fetchShopProductIds(
  api: EntityShopApi,
  shopId: number,
): Promise<number[]> {
  const q = api.query.entityProduct;
  if (!q?.shopProducts) return [];
  const raw = (await q.shopProducts(shopId)) as {
    toJSON?: () => unknown;
  };
  const json = raw?.toJSON?.() ?? raw;
  if (!Array.isArray(json)) return [];
  return json.map((id) => Number(id)).filter((id) => Number.isFinite(id) && id > 0);
}

// EN: Load products for one shop by id list.
// CN: 按 id 列表加载某店商品。
export async function fetchProductsByIds(
  api: EntityShopApi,
  productIds: number[],
): Promise<EntityProduct[]> {
  const q = api.query.entityProduct;
  if (!q?.products || productIds.length === 0) return [];
  const rows = await Promise.all(productIds.map((id) => q.products(id)));
  const products: EntityProduct[] = [];
  for (const raw of rows) {
    const row = raw as {
      isNone?: boolean;
      unwrap?: () => { toJSON: () => Record<string, unknown> };
    };
    if (row?.isNone) continue;
    products.push(parseProduct(row.unwrap!().toJSON()));
  }
  return products;
}
