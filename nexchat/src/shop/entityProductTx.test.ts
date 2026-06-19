import { describe, expect, it, vi } from "vitest";

vi.mock("@/config", () => ({ config: { useMock: false } }));
vi.mock("@/chain/chainClient", () => ({
  chainClient: { signAndSend: vi.fn(async () => "0xabc") },
}));

import { chainClient } from "@/chain/chainClient";
import { createProduct, publishProduct } from "@/shop/entityProductTx";

describe("entityProductTx", () => {
  it("createProduct sends expected pallet call", async () => {
    await createProduct({
      shopId: 42,
      nameCid: "QmName",
      imagesCid: "QmImg",
      detailCid: "QmDetail",
      usdtPrice: 1_000_000,
      stock: 10,
      category: "Physical",
      minOrderQuantity: 1,
      maxOrderQuantity: 0,
      visibility: "Public",
    });
    expect(chainClient.signAndSend).toHaveBeenCalledWith("entityProduct", "createProduct", [
      42,
      "QmName",
      "QmImg",
      "QmDetail",
      1_000_000,
      10,
      "Physical",
      0,
      "",
      "",
      1,
      0,
      "Public",
    ]);
  });

  it("publishProduct sends product id", async () => {
    await publishProduct(99);
    expect(chainClient.signAndSend).toHaveBeenCalledWith("entityProduct", "publishProduct", [99]);
  });

  it("createProduct rejects empty CIDs", async () => {
    await expect(
      createProduct({
        shopId: 1,
        nameCid: "",
        imagesCid: "QmImg",
        detailCid: "QmDetail",
        usdtPrice: 1,
        stock: 1,
        category: "Physical",
        minOrderQuantity: 1,
        maxOrderQuantity: 0,
        visibility: "Public",
      }),
    ).rejects.toThrow(/CID/);
  });
});
