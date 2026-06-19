import { describe, it, expect } from "vitest";
import { InboundDedup, frameDedupKey } from "@/relay/dedup";

describe("InboundDedup", () => {
  it("accepts a key once", () => {
    const d = new InboundDedup();
    expect(d.accept("a")).toBe(true);
    expect(d.accept("a")).toBe(false);
  });

  it("frameDedupKey prefers dedupKey", () => {
    expect(
      frameDedupKey({ dedupKey: "k1", convId: "g:1", ciphertextB64: "x" }),
    ).toBe("k1");
  });
});
