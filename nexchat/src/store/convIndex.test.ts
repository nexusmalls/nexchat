import { describe, it, expect, beforeEach } from "vitest";
import { keyVault } from "@/keyvault/keyvault";
import {
  applyIndexToLocal,
  buildIndexFromLocal,
  decryptIndexBlob,
  encryptIndexBlob,
  mergeIndexBlobs,
  type ConvIndexBlob,
} from "@/store/convIndex";

describe("convIndex", () => {
  beforeEach(() => {
    keyVault.initForTest("5AliceTestAccount");
  });

  it("mergeIndexBlobs keeps newer entry per conversation", () => {
    const a: ConvIndexBlob = {
      v: 1,
      updated_at: 100,
      device_id: "a",
      conversations: [
        {
          kind: "direct",
          peer_ref: "bob",
          pinned: false,
          muted: false,
          last_active: 50,
          updated_at: 100,
        },
      ],
    };
    const b: ConvIndexBlob = {
      v: 1,
      updated_at: 200,
      device_id: "b",
      conversations: [
        {
          kind: "direct",
          peer_ref: "bob",
          pinned: true,
          muted: true,
          last_active: 80,
          updated_at: 200,
        },
      ],
    };
    const m = mergeIndexBlobs(a, b);
    expect(m.conversations).toHaveLength(1);
    expect(m.conversations[0]!.pinned).toBe(true);
    expect(m.conversations[0]!.muted).toBe(true);
    expect(m.updated_at).toBe(200);
  });

  it("applyIndexToLocal adds off-chain-only direct conv", () => {
    const index: ConvIndexBlob = {
      v: 1,
      updated_at: 1,
      device_id: "x",
      conversations: [
        {
          kind: "direct",
          peer_ref: "5Bob",
          title: "Bob",
          pinned: true,
          muted: false,
          last_active: 999,
          updated_at: 999,
        },
      ],
    };
    const out = applyIndexToLocal([], index);
    expect(out).toHaveLength(1);
    expect(out[0]!.peer).toBe("5Bob");
    expect(out[0]!.pinnedPref).toBe(true);
    expect(out[0]!.lastActive).toBe(999);
  });

  it("encrypt/decrypt round-trip", async () => {
    const blob = buildIndexFromLocal(
      [{ kind: "group", groupId: 3, lastActive: 10, unread: 0, pinnedPref: true }],
      "5Alice",
      "dev-1",
      42,
    );
    const packed = await encryptIndexBlob(blob);
    const back = await decryptIndexBlob(packed);
    expect(back.conversations[0]!.group_id).toBe(3);
    expect(back.conversations[0]!.pinned).toBe(true);
  });
});
