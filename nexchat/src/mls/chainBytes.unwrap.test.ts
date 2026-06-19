import { describe, expect, it } from "vitest";
import { decodeChainText, unwrapChainJson } from "@/mls/chainBytes";

describe("unwrapChainJson", () => {
  it("unwraps polkadot Option via unwrap()", () => {
    const data = unwrapChainJson({
      isNone: false,
      unwrap: () => ({ toJSON: () => ({ pending: "1" }) }),
    });
    expect(data?.pending).toBe("1");
  });

  it("unwraps JSON Some shape without calling non-function unwrap", () => {
    const data = unwrapChainJson({
      isSome: true,
      unwrap: { pending: "2" },
      Some: { pending: "3" },
    });
    expect(data?.pending).toBe("3");
  });

  it("returns null for None", () => {
    expect(unwrapChainJson({ isNone: true })).toBeNull();
  });

  it("falls back to toJSON()", () => {
    const data = unwrapChainJson({ toJSON: () => ({ pending: "4" }) });
    expect(data?.pending).toBe("4");
  });
});

describe("decodeChainText", () => {
  it("does not throw when unwrap is not a function", () => {
    expect(() =>
      decodeChainText({
        isSome: true,
        unwrap: "0x4e6578757320436f6d6d756e697479",
      }),
    ).not.toThrow();
  });
});
