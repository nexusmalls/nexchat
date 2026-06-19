// EN: Unit test for the lazy/on-demand group join planner (CHAT_GROUP_WIREIFY_DESIGN §8.1). Pins the
// active→joinNow / dormant→defer split, exclusion of already-held groups + non-`g:` entries, dedup +
// order preservation, and the eager fallback (`isActive: () => true`).
// CN: 延迟/按需群 join 规划器单元测试（设计 §8.1）。钉住 活跃→joinNow / 休眠→defer 切分、排除已持有群 +
// 非 `g:` 项、去重 + 保序，以及急加载回退（`isActive: () => true`）。

import { describe, expect, it } from "vitest";
import { planWireGroupJoin } from "@/mls/wireGroupJoinPlan";

describe("planWireGroupJoin (§8.1 lazy/on-demand Add)", () => {
  it("splits active → joinNow and dormant → defer, excluding held groups", () => {
    const active = new Set(["g:1", "g:3"]);
    const held = new Set(["g:2"]);
    const plan = planWireGroupJoin({
      offeredGroups: ["g:1", "g:2", "g:3", "g:4"],
      isHeld: (c) => held.has(c),
      isActive: (c) => active.has(c),
    });
    expect(plan.joinNow).toEqual(["g:1", "g:3"]); // active, not held
    expect(plan.defer).toEqual(["g:4"]); // dormant, not held (g:2 excluded as held)
  });

  it("eager fallback: with isActive always-true, every not-held group joins now", () => {
    const plan = planWireGroupJoin({
      offeredGroups: ["g:1", "g:2"],
      isHeld: () => false,
      isActive: () => true,
    });
    expect(plan.joinNow).toEqual(["g:1", "g:2"]);
    expect(plan.defer).toEqual([]);
  });

  it("dedups, preserves order, and ignores non-group entries", () => {
    const plan = planWireGroupJoin({
      offeredGroups: ["g:5", "d:a:b", "g:5", "", "g:9"],
      isHeld: () => false,
      isActive: (c) => c === "g:5",
    });
    expect(plan.joinNow).toEqual(["g:5"]);
    expect(plan.defer).toEqual(["g:9"]);
  });

  it("all held → both buckets empty (idempotent re-offer)", () => {
    const plan = planWireGroupJoin({
      offeredGroups: ["g:1", "g:2"],
      isHeld: () => true,
      isActive: () => true,
    });
    expect(plan.joinNow).toEqual([]);
    expect(plan.defer).toEqual([]);
  });
});
