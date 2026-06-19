import { describe, expect, it } from "vitest";
import {
  chainDirectionToUserSide,
  directionLabel,
  filterSingleLineRecords,
  singleLineTotalEarned,
  sumSingleLineAmount,
  userDownlineTotal,
  userSideLabel,
  userUplineTotal,
} from "@/earnings/singleLineQueries";
import type { SingleLineMemberStats, SingleLinePayoutRecord } from "@/earnings/types";

const stats: SingleLineMemberStats = {
  totalEarnedAsUpline: "2000000000000",
  totalEarnedAsDownline: "1000000000000",
  totalPayoutCount: 3,
  lastPayoutBlock: 100,
};

const records: SingleLinePayoutRecord[] = [
  {
    orderId: 1,
    buyer: "x",
    amount: "1000000000000",
    direction: "downline",
    levelDistance: 13,
    blockNumber: 10,
    shopId: 11,
  },
  {
    orderId: 2,
    buyer: "y",
    amount: "2000000000000",
    direction: "upline",
    levelDistance: 1,
    blockNumber: 20,
    shopId: null,
  },
];

describe("earnings/singleLineQueries", () => {
  it("singleLineTotalEarned sums upline and downline", () => {
    expect(singleLineTotalEarned(stats)).toBe("3000000000000");
  });

  it("maps chain direction to user queue side like dapp", () => {
    expect(chainDirectionToUserSide("downline")).toBe("upline");
    expect(chainDirectionToUserSide("upline")).toBe("downline");
    expect(userUplineTotal(stats)).toBe(stats.totalEarnedAsDownline);
    expect(userDownlineTotal(stats)).toBe(stats.totalEarnedAsUpline);
  });

  it("filterSingleLineRecords filters by user perspective", () => {
    expect(filterSingleLineRecords(records, "upline")).toHaveLength(1);
    expect(filterSingleLineRecords(records, "upline")[0]?.direction).toBe("downline");
    expect(filterSingleLineRecords(records, "downline")).toHaveLength(1);
    expect(filterSingleLineRecords(records, "all")).toHaveLength(2);
  });

  it("userSideLabel shows queue-relative text", () => {
    expect(userSideLabel("downline", 13)).toBe("上层 13 级");
    expect(userSideLabel("upline", 1)).toBe("下层 1 级");
    expect(directionLabel("upline", 2)).toBe("上层 2 级");
  });

  it("sumSingleLineAmount totals filtered rows", () => {
    expect(sumSingleLineAmount(records)).toBe("3000000000000");
  });
});
