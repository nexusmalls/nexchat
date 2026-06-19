// EN: Unit test for the pure Wire device-roster derivation (CHAT_GROUP_WIREIFY_DESIGN §9, G6 disclosure
// UX). Pins the 1:1 self/peer/other split + this-device marking + removable filter, and the GROUP roster
// self/members split + distinct-account count, all with SS58-prefix canonicalization.
// CN: Wire 设备名册纯推导单测（设计 §9，G6 披露 UX）。钉住 1:1 self/peer/other 切分 + 本机标记 + 可移除过滤，
// 以及群名册 self/members 切分 + 去重账户计数，均经 SS58 前缀规范化。

import { describe, expect, it } from "vitest";
import {
  computeWireDeviceRoster,
  computeWireGroupRoster,
  removableSelfDevices,
} from "@/mls/wireDeviceRoster";

const ALICE = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"; // canonical sr25519 well-known
const BOB = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";

describe("computeWireDeviceRoster (1:1, §9)", () => {
  it("splits self/peer, marks this-device, and lists only my OTHER devices as removable", () => {
    const ids = [`${ALICE}#a`, `${ALICE}#b`, `${BOB}#x`];
    const roster = computeWireDeviceRoster(ids, ALICE, BOB, "a");
    expect(roster.total).toBe(3);
    expect(roster.self.map((d) => d.deviceId)).toEqual(["a", "b"]);
    expect(roster.peer.map((d) => d.deviceId)).toEqual(["x"]);
    expect(roster.other).toEqual([]);
    expect(roster.self.find((d) => d.deviceId === "a")?.isThisDevice).toBe(true);
    expect(removableSelfDevices(roster).map((d) => d.deviceId)).toEqual(["b"]); // never the local device
  });

  it("surfaces leaves bound to neither party under `other`", () => {
    const roster = computeWireDeviceRoster([`${ALICE}#a`, "5StrangerAddr#z"], ALICE, BOB);
    expect(roster.self.map((d) => d.deviceId)).toEqual(["a"]);
    expect(roster.peer).toEqual([]);
    expect(roster.other.map((d) => d.deviceId)).toEqual(["z"]);
  });
});

describe("computeWireGroupRoster (group, §9)", () => {
  it("splits my devices from other members and counts distinct other accounts", () => {
    const CAROL = "5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy";
    const ids = [
      `${ALICE}#a`,
      `${ALICE}#b`,
      `${BOB}#x`,
      `${BOB}#y`,
      `${CAROL}#c`,
    ];
    const roster = computeWireGroupRoster(ids, ALICE, "a");
    expect(roster.total).toBe(5);
    expect(roster.self.map((d) => d.deviceId)).toEqual(["a", "b"]);
    expect(roster.members.map((d) => d.deviceId)).toEqual(["x", "y", "c"]);
    expect(roster.memberAccounts).toBe(2); // bob + carol (distinct accounts, not devices)
    expect(roster.self.find((d) => d.deviceId === "a")?.isThisDevice).toBe(true);
    expect(roster.self.find((d) => d.deviceId === "b")?.isThisDevice).toBe(false);
  });

  it("a solo group (only my one device) discloses no other members", () => {
    const roster = computeWireGroupRoster([`${ALICE}#a`], ALICE, "a");
    expect(roster.total).toBe(1);
    expect(roster.members).toEqual([]);
    expect(roster.memberAccounts).toBe(0);
  });
});
