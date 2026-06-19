import { describe, expect, it } from "vitest";
import {
  legacyDirectPeersForWireMigration,
  mergeWireJoinThreadPeers,
  type MlsGroupEngine,
} from "@/mls/wireLegacyMigration";
import { directMlsKey } from "@/mls/directConv";

const SELF = "5SelfAddrLong";
const ALICE = "5AliceAddrLong";
const BOB = "5BobAddrLong";

function fakeEngine(groups: Record<string, boolean>): MlsGroupEngine {
  const keys = Object.keys(groups);
  return {
    listGroups: () => keys,
    hasGroup: (id) => groups[id] === true,
  };
}

describe("legacyDirectPeersForWireMigration", () => {
  it("returns peers present on account engine but missing on wire engine", () => {
    const convBob = directMlsKey(SELF, BOB);
    const convAlice = directMlsKey(SELF, ALICE);
    const account = fakeEngine({ [convBob]: true, [convAlice]: true, "g:1": true });
    const wire = fakeEngine({ [convAlice]: true });
    expect(legacyDirectPeersForWireMigration(account, wire, SELF).sort()).toEqual([BOB].sort());
  });

  it("returns empty when wire already holds every direct group", () => {
    const conv = directMlsKey(SELF, BOB);
    const eng = fakeEngine({ [conv]: true });
    expect(legacyDirectPeersForWireMigration(eng, eng, SELF)).toEqual([]);
  });

  it("ignores non-direct and malformed keys", () => {
    const account = fakeEngine({
      "g:99": true,
      "d:onlyone": true,
      [directMlsKey(SELF, BOB)]: true,
    });
    const wire = fakeEngine({});
    expect(legacyDirectPeersForWireMigration(account, wire, SELF)).toEqual([BOB]);
  });
});

describe("mergeWireJoinThreadPeers", () => {
  it("deduplicates and preserves order (threads first, then legacy)", () => {
    expect(
      mergeWireJoinThreadPeers(["5BobAddrLong", "5GhostAddrLong"], ["5BobAddrLong", "5AliceAddrLong"]),
    ).toEqual(["5BobAddrLong", "5GhostAddrLong", "5AliceAddrLong"]);
  });
});
