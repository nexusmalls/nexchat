import { describe, expect, it } from "vitest";
import { applySlippageBps, computeOrderQuote, usdtToNexDynamic } from "@/shop/pricing";

describe("shop/pricing", () => {
  it("usdtToNexDynamic converts with 12-dec NEX", () => {
    // 1 USDT at $0.05/NEX → 20 NEX
    expect(usdtToNexDynamic(1_000_000, "50000")).toBe("20000000000000");
  });

  it("applySlippageBps adds buffer", () => {
    const out = applySlippageBps("1000000000000", 300);
    expect(BigInt(out)).toBeGreaterThan(1000000000000n);
  });

  it("computeOrderQuote uses market rate for USDT-priced products", () => {
    const q = computeOrderQuote({
      product: { price: "0", usdtPrice: 1_000_000 },
      quantity: 2,
      marketRate: "50000",
      paymentAsset: "Native",
    });
    expect(q.hasUsdtPrice).toBe(true);
    expect(q.totalUsdt).toBe(2_000_000);
    expect(q.maxNexAmount).not.toBeNull();
  });
});
