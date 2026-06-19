import { describe, expect, it } from "vitest";
import { parseUsdtInput } from "@/shop/format";

describe("parseUsdtInput", () => {
  it("parses whole and fractional USDT to micro-USDT", () => {
    expect(parseUsdtInput("1")).toBe(1_000_000);
    expect(parseUsdtInput("9.99")).toBe(9_990_000);
    expect(parseUsdtInput("0.5")).toBe(500_000);
  });

  it("rejects invalid input", () => {
    expect(parseUsdtInput("")).toBeNull();
    expect(parseUsdtInput("abc")).toBeNull();
    expect(parseUsdtInput("1.2.3")).toBeNull();
    expect(parseUsdtInput("0")).toBeNull();
  });
});
