import { describe, expect, it } from "vitest";
import { parseNexAmount } from "@/wallet/amount";

describe("wallet/amount", () => {
  it("parseNexAmount converts decimals to planck", () => {
    expect(parseNexAmount("1")).toBe(1_000_000_000_000n);
    expect(parseNexAmount("1.5")).toBe(1_500_000_000_000n);
    expect(parseNexAmount("0.01")).toBe(10_000_000_000n);
  });

  it("parseNexAmount rejects invalid input", () => {
    expect(parseNexAmount("")).toBeNull();
    expect(parseNexAmount("abc")).toBeNull();
    expect(parseNexAmount("1.2.3")).toBeNull();
  });
});
