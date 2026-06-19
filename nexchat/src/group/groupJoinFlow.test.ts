import { describe, expect, it } from "vitest";
import { buildGroupLookup, resolveGroupJoinStatus } from "@/group/groupJoinFlow";
import type { GroupMlsSnapshot } from "@/chain/chainClient";
import type { GroupJoinFlags } from "@/group/groupJoinTypes";

const privateSnap: GroupMlsSnapshot = {
  epoch: 1,
  treeHash: "0x00",
  confirmedTranscriptHash: "0x00",
  groupInfoCid: "",
  memberCount: 5,
  cipherSuite: 1,
  isPublic: false,
  frozen: false,
};

const baseFlags: GroupJoinFlags = {
  isMember: false,
  hasJoinRequest: false,
  hasJoinApproval: false,
  isBanned: false,
  keyPackageCount: 2,
};

describe("resolveGroupJoinStatus", () => {
  it("returns not_found when snapshot missing", () => {
    expect(resolveGroupJoinStatus(null, baseFlags)).toBe("not_found");
  });

  it("returns public_group for public snapshots", () => {
    expect(
      resolveGroupJoinStatus({ ...privateSnap, isPublic: true }, baseFlags),
    ).toBe("public_group");
  });

  it("returns pending_request when join request exists", () => {
    expect(
      resolveGroupJoinStatus(privateSnap, { ...baseFlags, hasJoinRequest: true }),
    ).toBe("pending_request");
  });

  it("returns key_package_missing before not_member", () => {
    expect(
      resolveGroupJoinStatus(privateSnap, { ...baseFlags, keyPackageCount: 0 }),
    ).toBe("key_package_missing");
  });
});

describe("buildGroupLookup", () => {
  it("uses profile name when present", () => {
    const vm = buildGroupLookup({
      groupId: 42,
      snap: privateSnap,
      flags: baseFlags,
      profile: { name: "产品组", avatarCid: "bafy", announcement: "hello" },
      adminAddress: "5Alice",
    });
    expect(vm.name).toBe("产品组");
    expect(vm.status).toBe("not_member");
  });
});
