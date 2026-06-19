import { describe, expect, it } from "vitest";
import { COMMISSION_MODES } from "@/earnings/commissionModes";
import { buildEarningsPlugins } from "@/earnings/plugins";
import type { CommissionDashboard, CommissionOverview } from "@/earnings/types";

const overview: CommissionOverview = {
  enabledModes: COMMISSION_MODES.DIRECT_REWARD | COMMISSION_MODES.POOL_REWARD,
  commissionRate: 500,
  isEnabled: true,
  multiLevelPaused: false,
  singleLineEnabled: false,
  teamStatus: [false, false],
  poolRewardPaused: false,
  withdrawalPaused: false,
  unallocatedPoolNex: "1000000000000",
};

const dashboard: CommissionDashboard = {
  nexStats: {
    totalEarned: "0",
    pending: "0",
    withdrawn: "0",
    repurchased: "0",
    orderCount: 0,
  },
  multiLevelStats: null,
  teamTier: null,
  singleLine: { position: null, isEnabled: false },
  poolReward: { claimableNex: "500000000000", currentRoundId: 2, isPaused: false },
  referral: { totalEarned: "2000000000000" },
  multiLevelProgress: [],
};

describe("earnings/plugins", () => {
  it("buildEarningsPlugins lists enabled modes", () => {
    const plugins = buildEarningsPlugins(overview, dashboard);
    expect(plugins.map((p) => p.key)).toEqual(["referral", "poolReward"]);
    expect(plugins[0]?.stat).toContain("NEX");
    expect(plugins[1]?.label).toBe("奖池领取");
    expect(plugins[1]?.stat).toContain("可领取:");
    expect(plugins[1]?.stat2).toContain("沉淀池:");
    expect(plugins[1]?.href).toBe("/earnings/pool-reward");
  });

  it("returns empty when commission disabled", () => {
    expect(buildEarningsPlugins({ ...overview, isEnabled: false }, dashboard)).toEqual([]);
  });
});
