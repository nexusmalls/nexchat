import { describe, expect, it } from "vitest";
import {
  WIRE_GROUP_ACTIVE_RECENCY_MS,
  isWireGroupActive,
  type WireGroupActivityContext,
} from "@/mls/wireGroupActivity";
import type { ConversationVM } from "@/types/viewModels";

function groupConv(
  convId: string,
  recency: number,
  archived = false,
): ConversationVM {
  return {
    convId,
    kind: "group",
    title: convId,
    recency,
    unread: 0,
    pinned: false,
    dnd: false,
    adminMuted: false,
    archived,
    frozen: false,
    memberCount: 3,
    myRole: "member",
    presence: "both",
    groupId: Number(convId.slice(2)),
  };
}

describe("isWireGroupActive (§8.1)", () => {
  const now = 1_700_000_000_000;

  it("treats the currently open group as active regardless of recency", () => {
    const ctx: WireGroupActivityContext = {
      activeConvId: "g:1",
      conversations: [groupConv("g:1", now - WIRE_GROUP_ACTIVE_RECENCY_MS - 1)],
      nowMs: now,
    };
    expect(isWireGroupActive("g:1", ctx)).toBe(true);
  });

  it("treats a recently used non-archived group as active", () => {
    const ctx: WireGroupActivityContext = {
      activeConvId: null,
      conversations: [groupConv("g:2", now - 60_000)],
      nowMs: now,
    };
    expect(isWireGroupActive("g:2", ctx)).toBe(true);
  });

  it("defers dormant (stale recency) and archived groups", () => {
    const ctx: WireGroupActivityContext = {
      activeConvId: null,
      conversations: [
        groupConv("g:3", now - WIRE_GROUP_ACTIVE_RECENCY_MS - 1),
        groupConv("g:4", now - 60_000, true),
      ],
      nowMs: now,
    };
    expect(isWireGroupActive("g:3", ctx)).toBe(false);
    expect(isWireGroupActive("g:4", ctx)).toBe(false);
  });

  it("returns false for unknown or non-group conv ids", () => {
    const ctx: WireGroupActivityContext = { activeConvId: null, conversations: [], nowMs: now };
    expect(isWireGroupActive("d:a:b", ctx)).toBe(false);
    expect(isWireGroupActive("g:99", ctx)).toBe(false);
  });
});
