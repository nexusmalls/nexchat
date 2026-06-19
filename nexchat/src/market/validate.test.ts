import { describe, expect, it } from "vitest";
import { isValidTronAddress, validateAmount } from "@/market/validate";

describe("market/validate", () => {
  it("validateAmount parses NEX with 12 decimals", () => {
    const r = validateAmount("1.5", "NEX");
    expect(r.valid).toBe(true);
    expect(r.value).toBe(1_500_000_000_000n);
  });

  it("validateAmount parses USDT with 6 decimals", () => {
    const r = validateAmount("0.05", "USDT");
    expect(r.valid).toBe(true);
    expect(r.value).toBe(50_000n);
  });

  it("isValidTronAddress checks T-prefix base58", () => {
    expect(isValidTronAddress("T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb")).toBe(true);
    expect(isValidTronAddress("invalid")).toBe(false);
  });
});
