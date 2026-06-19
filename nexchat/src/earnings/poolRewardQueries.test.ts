import { describe, expect, it } from "vitest";
import {
  canClaimPoolReward,
  capProgressPercent,
  poolRewardIneligibleReason,
} from "@/earnings/poolRewardQueries";
import type { PoolRewardMemberView } from "@/earnings/types";

const baseView: PoolRewardMemberView = {
  roundDuration: 1000,
  tokenPoolEnabled: false,
  currentRoundId: 2,
  roundStartBlock: 100,
  roundEndBlock: 1100,
  poolSnapshot: "1000000000000",
  tokenPoolSnapshot: null,
  effectiveLevel: 3,
  claimableNex: "500000000000",
  claimableToken: "0",
  alreadyClaimed: false,
  roundExpired: false,
  lastClaimedRound: 1,
  capInfo: {
    cumulativeClaimedUsdt: "250000000",
    currentCapUsdt: "1000000000",
    remainingCapUsdt: "750000000",
    isCapped: false,
    rateSnapshotUsed: 500000,
    unlockPercent: null,
  },
  levelProgress: [{ levelId: 3, ratioBps: 1000, memberCount: 10, claimedCount: 2, perMemberReward: "100" }],
  claimHistory: [],
  isPaused: false,
  hasPendingConfig: false,
};

describe("earnings/poolRewardQueries", () => {
  it("canClaimPoolReward when claimable and active round", () => {
    expect(canClaimPoolReward(baseView)).toBe(true);
  });

  it("capProgressPercent computes fill ratio", () => {
    expect(capProgressPercent(baseView)).toBe(25);
  });

  it("poolRewardIneligibleReason detects paused", () => {
    expect(poolRewardIneligibleReason({ ...baseView, isPaused: true })).toBe("paused");
  });

  it("poolRewardIneligibleReason detects already claimed", () => {
    expect(poolRewardIneligibleReason({ ...baseView, alreadyClaimed: true })).toBe(
      "alreadyClaimed",
    );
  });
});
