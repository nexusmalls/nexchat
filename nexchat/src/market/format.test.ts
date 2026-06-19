import { describe, expect, it } from "vitest";
import { formatBalance, formatNexPrice, formatUsdt } from "@/market/format";

describe("market/format", () => {
  it("formatNexPrice scales 1e6 USDT precision", () => {
    expect(formatNexPrice("50000")).toBe("0.0500");
    expect(formatNexPrice("1500000")).toBe("1.50");
  });

  it("formatBalance shows NEX decimals", () => {
    expect(formatBalance("1000000000000", 12, 2)).toBe("1.00");
  });

  it("formatUsdt formats micro-USDT", () => {
    expect(formatUsdt("2500000")).toBe("2.50");
  });
});
