import { describe, expect, it } from "vitest";
import {
  filterProductsByCategory,
  filterProductsBySearch,
  filterShopsBySearch,
} from "@/shop/filter";
import type { EntityProduct, EntityShop } from "@/shop/types";

function product(id: number, category: EntityProduct["category"] = "Physical"): EntityProduct {
  return {
    id,
    shopId: 1,
    nameCid: `cid-${id}`,
    imagesCid: "",
    detailCid: "",
    price: "0",
    usdtPrice: 0,
    stock: 1,
    soldCount: 0,
    status: "OnSale",
    visibility: "Public",
    visibilityMinLevel: 0,
    category,
    sortWeight: 0,
    minOrderQuantity: 1,
    maxOrderQuantity: 0,
    createdAt: 0,
  };
}

function shop(id: number, name: string): EntityShop {
  return {
    id,
    entityId: 1,
    name,
    logoCid: null,
    descriptionCid: null,
    status: "Active",
    managers: [],
    productCount: 0,
    totalSales: "0",
    totalOrders: 0,
    createdAt: 0,
  };
}

describe("shop/filter", () => {
  it("filterProductsByCategory", () => {
    const items = [product(1, "Physical"), product(2, "Digital")];
    expect(filterProductsByCategory(items, "Digital").map((p) => p.id)).toEqual([2]);
    expect(filterProductsByCategory(items, "all")).toHaveLength(2);
  });

  it("filterProductsBySearch matches name or id", () => {
    const items = [product(10), product(20)];
    const names = new Map([
      ["cid-10", "Apple Phone"],
      ["cid-20", "Banana Case"],
    ]);
    expect(filterProductsBySearch(items, names, "apple").map((p) => p.id)).toEqual([10]);
    expect(filterProductsBySearch(items, names, "20").map((p) => p.id)).toEqual([20]);
  });

  it("filterShopsBySearch matches shop name or id", () => {
    const items = [shop(1, "Nex Store"), shop(2, "Other")];
    expect(filterShopsBySearch(items, "nex").map((s) => s.id)).toEqual([1]);
    expect(filterShopsBySearch(items, "2").map((s) => s.id)).toEqual([2]);
  });
});
