import { describe, expect, it } from "vitest";
import { planWireJoinTargets } from "@/mls/wireJoinPlan";

const SELF = "5Self";
const none = () => false;

describe("planWireJoinTargets", () => {
  it("routes existing-1:1 peers to peer-assist and the rest to registry", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: ["5Alice", "5Bob", "5Carol"],
      threadPeers: ["5Bob"], // only Bob has an existing 1:1
      isGraftManaged: none,
    });
    expect(plan.peerAssist).toEqual(["5Bob"]);
    expect(plan.registry.sort()).toEqual(["5Alice", "5Carol"]);
  });

  it("excludes self from both lists", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: [SELF, "5Alice"],
      threadPeers: [SELF],
      isGraftManaged: none,
    });
    expect(plan.peerAssist).toEqual([]);
    expect(plan.registry).toEqual(["5Alice"]);
  });

  it("excludes graft-managed convs (already joined via a sibling) from both lists", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: ["5Alice", "5Bob"],
      threadPeers: ["5Alice", "5Bob"],
      isGraftManaged: (p) => p === "5Alice",
    });
    expect(plan.peerAssist).toEqual(["5Bob"]);
    expect(plan.registry).toEqual([]);
  });

  it("includes existing-1:1 peers even when not in the contact roster", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: ["5Alice"],
      threadPeers: ["5Ghost"], // 1:1 thread with a non-contact
      isGraftManaged: none,
    });
    expect(plan.peerAssist).toEqual(["5Ghost"]);
    expect(plan.registry).toEqual(["5Alice"]);
  });

  it("empty thread list (no known 1:1s yet) → everything is a new-chat handshake", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: ["5Alice", "5Bob"],
      threadPeers: [],
      isGraftManaged: none,
    });
    expect(plan.peerAssist).toEqual([]);
    expect(plan.registry.sort()).toEqual(["5Alice", "5Bob"]);
  });

  it("deduplicates peers appearing in both contacts and threads", () => {
    const plan = planWireJoinTargets({
      self: SELF,
      contacts: ["5Bob", "5Bob"],
      threadPeers: ["5Bob"],
      isGraftManaged: none,
    });
    expect(plan.peerAssist).toEqual(["5Bob"]);
    expect(plan.registry).toEqual([]);
  });
});
