import type {
  EntityMemberInfo,
  EntityProduct,
  EntityShop,
  ShopCatalog,
} from "@/shop/types";
import { canViewProduct, isOnSaleProduct } from "@/shop/visibility";

// EN: Listable = OnSale + Public + shop Active (legacy helper for tests).
// CN: 可展示 = 在售 + 公开 + 店铺营业中（测试用遗留 helper）。
export function isListableProduct(
  product: EntityProduct,
  shop: EntityShop | undefined,
): boolean {
  return canViewProduct(product, shop, null);
}

export function sortProductsBySales(products: EntityProduct[]): EntityProduct[] {
  return [...products].sort((a, b) => {
    if (b.soldCount !== a.soldCount) return b.soldCount - a.soldCount;
    if (b.sortWeight !== a.sortWeight) return b.sortWeight - a.sortWeight;
    return b.id - a.id;
  });
}

export function sortShopsByOrders(shops: EntityShop[]): EntityShop[] {
  return [...shops]
    .filter((s) => s.status === "Active")
    .sort((a, b) => {
      if (b.totalOrders !== a.totalOrders) return b.totalOrders - a.totalOrders;
      return b.id - a.id;
    });
}

// EN: Restrict catalog to one entity's shops and products.
// CN: 将目录限制为单个 Entity 的商铺与商品。
export function filterCatalogByEntity(
  catalog: ShopCatalog,
  entityId: number,
): ShopCatalog {
  const shops = catalog.shops.filter((s) => s.entityId === entityId);
  const shopIds = new Set(shops.map((s) => s.id));
  const products = catalog.products.filter((p) => shopIds.has(p.shopId));
  const allOnSaleProducts = catalog.allOnSaleProducts.filter((p) => shopIds.has(p.shopId));
  const shopById = new Map(shops.map((s) => [s.id, s]));
  const member = catalog.memberByEntity.get(entityId);
  const memberByEntity = member ? new Map([[entityId, member]]) : new Map<number, EntityMemberInfo>();
  return {
    shops,
    products,
    allOnSaleProducts,
    shopById,
    memberByEntity,
  };
}

export function buildCatalog(
  shops: EntityShop[],
  products: EntityProduct[],
  memberByEntity: Map<number, EntityMemberInfo> = new Map(),
): ShopCatalog {
  const shopById = new Map(shops.map((s) => [s.id, s]));
  const allOnSale = products.filter((p) => isOnSaleProduct(p, shopById.get(p.shopId)));
  const visible = products.filter((p) => {
    const shop = shopById.get(p.shopId);
    const member = shop ? memberByEntity.get(shop.entityId) : undefined;
    return canViewProduct(p, shop, member);
  });
  return {
    shops: sortShopsByOrders(shops),
    products: sortProductsBySales(visible),
    allOnSaleProducts: sortProductsBySales(allOnSale),
    shopById,
    memberByEntity,
  };
}
