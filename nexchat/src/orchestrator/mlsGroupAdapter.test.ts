import { beforeEach, describe, expect, it, vi } from "vitest";

const createGroupWithMembers = vi.fn();
const disbandGroup = vi.fn();

vi.mock("@/mls/createGroupFlow", () => ({
  createGroupWithMembers: (...args: unknown[]) => createGroupWithMembers(...args),
}));
vi.mock("@/mls/changeGroupMembersFlow", () => ({
  disbandGroup: (...args: unknown[]) => disbandGroup(...args),
}));

import type { ChainClient } from "@/chain/chainClient";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { MlsGroupAdapter } from "@/orchestrator/mlsGroupAdapter";

function fakeEngine(groups: Set<string>): OpenMlsEngine {
  return { hasGroup: (convId: string) => groups.has(convId) } as unknown as OpenMlsEngine;
}

describe("MlsGroupAdapter — MlsGroupPort over real flows", () => {
  beforeEach(() => {
    createGroupWithMembers.mockReset();
    disbandGroup.mockReset();
  });

  it("createGroup delegates to createGroupWithMembers with mapped deps", async () => {
    createGroupWithMembers.mockResolvedValue(42);
    const engine = fakeEngine(new Set());
    const chain = {} as ChainClient;
    const a = new MlsGroupAdapter({ engine, chain, selfAddress: "5alice", groupName: "G", isPublic: false });

    const gid = await a.createGroup(["5bob", "5carol"]);
    expect(gid).toBe(42);
    expect(createGroupWithMembers).toHaveBeenCalledWith({
      engine,
      chain,
      selfAddress: "5alice",
      name: "G",
      memberAddresses: ["5bob", "5carol"],
      isPublic: false,
    });
  });

  it("createGroup uses a default group name when none provided", async () => {
    createGroupWithMembers.mockResolvedValue(1);
    const a = new MlsGroupAdapter({ engine: fakeEngine(new Set()), chain: {} as ChainClient, selfAddress: "5alice" });
    await a.createGroup(["5bob", "5carol"]);
    expect(createGroupWithMembers).toHaveBeenCalledWith(expect.objectContaining({ name: "群聊" }));
  });

  it("dissolve delegates to disbandGroup", async () => {
    disbandGroup.mockResolvedValue(undefined);
    const engine = fakeEngine(new Set());
    const chain = {} as ChainClient;
    const a = new MlsGroupAdapter({ engine, chain, selfAddress: "5alice" });
    await a.dissolve(7);
    expect(disbandGroup).toHaveBeenCalledWith(
      expect.objectContaining({ engine, chain, selfAddress: "5alice", groupId: 7 }),
    );
  });

  it("isActive reflects engine.hasGroup('g:{id}')", () => {
    const a = new MlsGroupAdapter({ engine: fakeEngine(new Set(["g:9"])), chain: {} as ChainClient, selfAddress: "5alice" });
    expect(a.isActive(9)).toBe(true);
    expect(a.isActive(10)).toBe(false);
  });
});
