import { describe, expect, it } from "vitest";
import { buildCatalog, filterCatalogByEntity } from "@/shop/sort";
import type { EntityProduct, EntityShop, ShopCatalog } from "@/shop/types";

function shop(id: number, entityId: number): EntityShop {
  return {
    id,
    entityId,
    name: `Shop ${id}`,
    logoCid: null,
    descriptionCid: null,
    status: "Active",
    managers: [],
    productCount: 1,
    totalSales: "0",
    totalOrders: 0,
    createdAt: 0,
  };
}

function product(
  id: number,
  shopId: number,
  soldCount = 0,
  visibility: EntityProduct["visibility"] = "Public",
): EntityProduct {
  return {
    id,
    shopId,
    nameCid: "QmName",
    imagesCid: "QmImg",
    detailCid: "QmDetail",
    price: "100",
    usdtPrice: 0,
    stock: 10,
    soldCount,
    status: "OnSale",
    visibility,
    visibilityMinLevel: 0,
    category: "Physical",
    sortWeight: 0,
    minOrderQuantity: 1,
    maxOrderQuantity: 0,
    createdAt: 0,
  };
}

describe("shop/sort", () => {
  it("filterCatalogByEntity keeps only matching shops and products", () => {
    const catalog: ShopCatalog = {
      shops: [shop(1, 10), shop(2, 20)],
      products: [product(100, 1), product(200, 2)],
      allOnSaleProducts: [product(100, 1), product(200, 2)],
      shopById: new Map([
        [1, shop(1, 10)],
        [2, shop(2, 20)],
      ]),
      memberByEntity: new Map(),
    };
    const filtered = filterCatalogByEntity(catalog, 10);
    expect(filtered.shops.map((s) => s.id)).toEqual([1]);
    expect(filtered.products.map((p) => p.id)).toEqual([100]);
    expect(filtered.allOnSaleProducts.map((p) => p.id)).toEqual([100]);
  });

  it("buildCatalog lists all on-sale products and filters visible products", () => {
    const shops = [shop(1, 10)];
    const products = [
      product(1, 1, 5, "Public"),
      product(2, 1, 10, "MembersOnly"),
    ];
    const catalog = buildCatalog(shops, products);
    expect(catalog.allOnSaleProducts.map((p) => p.id)).toEqual([2, 1]);
    expect(catalog.products.map((p) => p.id)).toEqual([1]);
  });
});
