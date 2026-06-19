import { describe, expect, it } from "vitest";
import { computePriceBand, validateLimitPrice } from "@/market/referencePrice";

describe("market/referencePrice", () => {
  it("computes ±20% band from reference", () => {
    const band = computePriceBand(
      { enabled: true, maxPriceDeviation: 2000, circuitBreakerActive: false, initialPrice: "1000000" },
      "1000000",
    );
    expect(band.minPrice).toBe(800000n);
    expect(band.maxPrice).toBe(1200000n);
  });

  it("rejects price outside band", () => {
    const protection = {
      enabled: true,
      maxPriceDeviation: 2000,
      circuitBreakerActive: false,
      initialPrice: "1000000",
    };
    const band = computePriceBand(protection, "1000000");
    expect(validateLimitPrice(700000n, protection, band)).toEqual({ ok: false, reason: "too_low" });
    expect(validateLimitPrice(900000n, protection, band)).toEqual({ ok: true });
  });
});
