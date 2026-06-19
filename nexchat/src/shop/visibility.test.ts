import { describe, expect, it } from "vitest";
import { canViewProduct, parseVisibility } from "@/shop/visibility";
import type { EntityMemberInfo, EntityProduct, EntityShop } from "@/shop/types";

function product(vis: EntityProduct["visibility"], minLevel = 0): EntityProduct {
  return {
    id: 1,
    shopId: 1,
    nameCid: "n",
    imagesCid: "",
    detailCid: "",
    price: "0",
    usdtPrice: 0,
    stock: 1,
    soldCount: 0,
    status: "OnSale",
    visibility: vis,
    visibilityMinLevel: minLevel,
    category: "Digital",
    sortWeight: 0,
    minOrderQuantity: 1,
    maxOrderQuantity: 0,
    createdAt: 0,
  };
}

const activeShop: EntityShop = {
  id: 1,
  entityId: 10,
  name: "S",
  logoCid: null,
  descriptionCid: null,
  status: "Active",
  managers: [],
  productCount: 1,
  totalSales: "0",
  totalOrders: 0,
  createdAt: 0,
};

const member: EntityMemberInfo = {
  isMember: true,
  level: 2,
  activated: true,
  bannedAt: null,
};

describe("shop/visibility", () => {
  it("parseVisibility handles levelGated object", () => {
    expect(parseVisibility({ levelGated: 3 })).toEqual({
      kind: "LevelGated",
      minLevel: 3,
    });
  });

  it("canViewProduct gates by membership and level", () => {
    expect(canViewProduct(product("Public"), activeShop, null)).toBe(true);
    expect(canViewProduct(product("MembersOnly"), activeShop, null)).toBe(false);
    expect(canViewProduct(product("MembersOnly"), activeShop, member)).toBe(true);
    expect(canViewProduct(product("LevelGated", 3), activeShop, member)).toBe(false);
    expect(canViewProduct(product("LevelGated", 2), activeShop, member)).toBe(true);
  });
});
