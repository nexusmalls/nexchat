import { describe, expect, it } from "vitest";
import { planWireGroupJoinSettle } from "@/mls/wireGroupJoinSettlePlan";

describe("planWireGroupJoinSettle (§8.4 no-sibling join)", () => {
  const G1 = "g:1";
  const G2 = "g:2";
  const G3 = "g:3";

  it("splits active vs dormant member groups, skips held", () => {
    const plan = planWireGroupJoinSettle({
      memberGroups: [G1, G2, G3, "d:x"],
      isHeld: (c) => c === G2,
      isActive: (c) => c === G1 || c === G2,
    });
    expect(plan.peerAssist).toEqual([G1]);
    expect(plan.defer).toEqual([G3]);
  });

  it("dedupes and ignores non-group convs", () => {
    const plan = planWireGroupJoinSettle({
      memberGroups: [G1, G1, G2],
      isHeld: () => false,
      isActive: () => true,
    });
    expect(plan.peerAssist).toEqual([G1, G2]);
    expect(plan.defer).toEqual([]);
  });

  it("all held → empty plan", () => {
    const plan = planWireGroupJoinSettle({
      memberGroups: [G1, G2],
      isHeld: () => true,
      isActive: () => true,
    });
    expect(plan.peerAssist).toEqual([]);
    expect(plan.defer).toEqual([]);
  });
});
