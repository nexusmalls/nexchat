import { describe, expect, it } from "vitest";
import {
  minApplicantsForSoloGroup,
  validateApproveJoinBatch,
} from "@/mls/approveJoinRequestsFlow";

describe("validateApproveJoinBatch", () => {
  it("requires at least 2 for solo owner group", () => {
    expect(() => validateApproveJoinBatch(1, 1)).toThrow(/至少 2/);
    expect(() => validateApproveJoinBatch(1, 2)).not.toThrow();
  });

  it("allows single add when group has 3+ members", () => {
    expect(() => validateApproveJoinBatch(5, 1)).not.toThrow();
  });

  it("rejects empty selection", () => {
    expect(() => validateApproveJoinBatch(5, 0)).toThrow(/至少选择 1/);
  });
});

describe("minApplicantsForSoloGroup", () => {
  it("returns 2 for solo group", () => {
    expect(minApplicantsForSoloGroup(1, 1)).toBe(2);
    expect(minApplicantsForSoloGroup(1, 3)).toBe(3);
  });

  it("returns 1 for larger groups", () => {
    expect(minApplicantsForSoloGroup(5, 1)).toBe(1);
  });
});
