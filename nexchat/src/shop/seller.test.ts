import { describe, expect, it } from "vitest";
import { getManagedShopIds, isShopManager } from "@/shop/seller";
import type { EntityShop, ShopCatalog } from "@/shop/types";

function shop(id: number, managers: string[]): EntityShop {
  return {
    id,
    entityId: id,
    name: `Shop ${id}`,
    logoCid: null,
    descriptionCid: null,
    status: "Active",
    managers,
    productCount: 0,
    totalSales: "0",
    totalOrders: 0,
    createdAt: 0,
  };
}

describe("shop/seller", () => {
  it("getManagedShopIds matches manager address", () => {
    const catalog: ShopCatalog = {
      shops: [shop(1, ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]), shop(2, [])],
      products: [],
      allOnSaleProducts: [],
      shopById: new Map(),
      memberByEntity: new Map(),
    };
    const ids = getManagedShopIds(catalog, "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY");
    expect(ids).toEqual([1]);
  });

  it("isShopManager checks managers list", () => {
    const s = shop(3, ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"]);
    expect(isShopManager(s, "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")).toBe(true);
    expect(isShopManager(s, null)).toBe(false);
  });

  it("getManagedShopIds includes shops under owned entities", () => {
    const catalog: ShopCatalog = {
      shops: [shop(2, []), shop(3, ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"])],
      products: [],
      allOnSaleProducts: [],
      shopById: new Map(),
      memberByEntity: new Map(),
    };
    catalog.shops[0]!.entityId = 100;
    catalog.shops[1]!.entityId = 200;
    const ids = getManagedShopIds(catalog, "5Fxxx", [100]);
    expect(ids).toEqual([2]);
  });

  it("isShopManager treats entity owner as manager", () => {
    const s = shop(4, []);
    s.entityId = 88;
    expect(isShopManager(s, "5Alice", [88])).toBe(true);
    expect(isShopManager(s, "5Bob", [99])).toBe(false);
  });
});
