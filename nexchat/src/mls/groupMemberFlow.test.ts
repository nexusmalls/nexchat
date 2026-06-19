import { describe, expect, it } from "vitest";
import {
  assertCanDisband,
  removeRequiresSwap,
  validateMemberDelta,
} from "@/mls/groupMemberFlow";
import { queuePendingGroupAdds, clearPendingGroupAdds } from "@/mls/addMembersToGroupFlow";

describe("groupMemberFlow", () => {
  it("validateMemberDelta rejects exactly-2-member outcome", () => {
    expect(() => validateMemberDelta(3, 0, 1)).toThrow(/2 人/);
    expect(() => validateMemberDelta(1, 1, 0)).toThrow(/2 人/);
    expect(() => validateMemberDelta(4, 0, 1)).not.toThrow();
    expect(() => validateMemberDelta(3, 1, 1)).not.toThrow();
  });

  it("removeRequiresSwap detects 3→2", () => {
    expect(removeRequiresSwap(3, 1)).toBe(true);
    expect(removeRequiresSwap(4, 1)).toBe(false);
    expect(removeRequiresSwap(3, 2)).toBe(false);
  });

  it("assertCanDisband is owner-only", () => {
    expect(() =>
      assertCanDisband({ self: "me", groupRole: "owner", memberCount: 3, frozen: false }),
    ).not.toThrow();
    expect(() =>
      assertCanDisband({ self: "me", groupRole: "admin", memberCount: 3, frozen: false }),
    ).toThrow(/群主/);
  });

  it("queuePendingGroupAdds merges solo-group invites", () => {
    clearPendingGroupAdds(9);
    expect(queuePendingGroupAdds(9, ["a"])).toBeNull();
    expect(queuePendingGroupAdds(9, ["b"])).toEqual(["a", "b"]);
    clearPendingGroupAdds(9);
  });
});
