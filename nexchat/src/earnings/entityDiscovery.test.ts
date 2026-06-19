import { describe, expect, it } from "vitest";
import {
  discoverCommissionEntityIds,
  hasCommissionActivity,
  mergeEntityIds,
} from "@/earnings/entityDiscovery";
import type { EntityDiscoveryApi } from "@/earnings/entityDiscovery";

describe("earnings/entityDiscovery", () => {
  it("hasCommissionActivity detects non-zero stats", () => {
    expect(hasCommissionActivity(null)).toBe(false);
    expect(
      hasCommissionActivity({
        totalEarned: "0",
        pending: "0",
        withdrawn: "0",
        repurchased: "0",
        orderCount: 0,
      }),
    ).toBe(false);
    expect(
      hasCommissionActivity({
        totalEarned: "0",
        pending: "1000",
        withdrawn: "0",
        repurchased: "0",
        orderCount: 0,
      }),
    ).toBe(true);
    expect(
      hasCommissionActivity({
        totalEarned: "0",
        pending: "0",
        withdrawn: "0",
        repurchased: "0",
        orderCount: 2,
      }),
    ).toBe(true);
  });

  it("mergeEntityIds dedupes and sorts", () => {
    expect(mergeEntityIds([3, 1], [2, 3, 0, -1])).toEqual([1, 2, 3]);
  });

  it("discoverCommissionEntityIds scans stats entries for address", async () => {
    const api = {
      query: {
        commissionCore: {
          memberCommissionStats: {
            entries: async () => [
              [[1, "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCP5UnYjPaPwcX"], { toJSON: () => ({
                totalEarned: "0",
                pending: "5000",
                withdrawn: "0",
                repurchased: "0",
                orderCount: 0,
              }) }],
              [[2, "other-account"], { toJSON: () => ({
                totalEarned: "9000",
                pending: "0",
                withdrawn: "0",
                repurchased: "0",
                orderCount: 0,
              }) }],
            ],
          },
        },
      },
    } as unknown as EntityDiscoveryApi;

    const ids = await discoverCommissionEntityIds(
      api,
      "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCP5UnYjPaPwcX",
    );
    expect(ids).toEqual([1]);
  });
});
