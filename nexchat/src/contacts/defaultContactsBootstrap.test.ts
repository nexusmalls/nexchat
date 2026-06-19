import { describe, expect, it } from "vitest";
import { canonicalAddress } from "@/wallet/address";
import {
  collectDefaultContactCandidates,
  resolveDefaultContactLabel,
} from "./defaultContactsBootstrap";

describe("resolveDefaultContactLabel", () => {
  it("prefers chain nickname", () => {
    expect(
      resolveDefaultContactLabel("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY", {
        nickname: "Alice链上",
      }),
    ).toBe("Alice链上");
  });

  it("falls back to seed name then short address", () => {
    const addr = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
    expect(resolveDefaultContactLabel(addr, null, "Alice")).toBe("Alice");
    expect(resolveDefaultContactLabel(addr, { nickname: "  " }, undefined)).toMatch(/^5Grw/);
  });
});

describe("collectDefaultContactCandidates", () => {
  const self = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
  const bob = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
  const charlie = "5CyC6Xpk3r4w3zPgwwGr4qDZXo2gWREXaAXZkTipCN3kj7CE";

  it("dedupes configured and roster and skips self/existing", () => {
    const out = collectDefaultContactCandidates(
      self,
      [bob],
      [bob, self],
      [bob, charlie],
    );
    expect(out).toEqual([canonicalAddress(charlie)]);
  });
});
