import { describe, it, expect, beforeEach } from "vitest";
import { InMemoryLocalStore } from "@/store/localStore";
import {
  clearConversationDeleted,
  loadDeletedConvIds,
  markConversationDeleted,
  tombstonesForDeletedConversations,
  tombstoneEntryForConvId,
} from "@/store/deletedConversations";
import { mergeIndexBlobs, type ConvIndexBlob } from "@/store/convIndex";

describe("deletedConversations", () => {
  let store: InMemoryLocalStore;

  beforeEach(() => {
    store = new InMemoryLocalStore();
  });

  it("mark and clear round-trip via meta", async () => {
    await markConversationDeleted(store, "d:5Bob");
    expect(await loadDeletedConvIds(store)).toEqual(new Set(["d:5Bob"]));
    await clearConversationDeleted(store, "d:5Bob");
    expect(await loadDeletedConvIds(store)).toEqual(new Set());
  });

  it("tombstonesForDeletedConversations covers explicit hidden ids", () => {
    const tombs = tombstonesForDeletedConversations(
      null,
      [],
      new Set(["d:5Bob"]),
      100,
    );
    expect(tombs).toHaveLength(1);
    expect(tombs[0]!.tombstone).toBe(true);
    expect(tombs[0]!.peer_ref).toBe("5Bob");
  });

  it("mergeIndexBlobs keeps tombstone terminal over stale live row", () => {
    const live: ConvIndexBlob = {
      v: 1,
      updated_at: 50,
      device_id: "a",
      conversations: [
        {
          kind: "direct",
          peer_ref: "bob",
          pinned: false,
          muted: false,
          last_active: 50,
          updated_at: 50,
        },
      ],
    };
    const deleted: ConvIndexBlob = {
      v: 1,
      updated_at: 200,
      device_id: "b",
      conversations: [tombstoneEntryForConvId("d:bob", 200)],
    };
    const m = mergeIndexBlobs(live, deleted);
    expect(m.conversations).toHaveLength(1);
    expect(m.conversations[0]!.tombstone).toBe(true);
  });
});
