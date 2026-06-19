import { describe, expect, it } from "vitest";
import { encodeProductShare, parseProductShare } from "@/shop/shareCard";

describe("shop/shareCard", () => {
  it("round-trips product share payload", () => {
    const text = encodeProductShare(42, 7, "测试商品");
    expect(parseProductShare(text)).toEqual({
      productId: 42,
      shopId: 7,
      label: "测试商品",
    });
  });

  it("returns null for normal text", () => {
    expect(parseProductShare("hello")).toBeNull();
  });
});
