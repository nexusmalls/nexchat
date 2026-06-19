import { describe, it, expect } from "vitest";
import type { MessageVM } from "@/types/viewModels";
import { canRecallMessage, RECALL_WINDOW_MS } from "@/p3/recall";

function msg(over: Partial<MessageVM>): MessageVM {
  return {
    clientMsgId: "m1",
    convId: "d:bob",
    senderRef: "me",
    isOutgoing: true,
    sentAt: 1_000_000,
    content: { type: "text", text: "hi" },
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
    ...over,
  };
}

describe("canRecallMessage", () => {
  const now = 1_000_000;

  it("allows own delivered text within the window", () => {
    expect(canRecallMessage(msg({}), now)).toBe(true);
    expect(canRecallMessage(msg({ status: "sent" }), now)).toBe(true);
  });

  it("rejects incoming messages", () => {
    expect(canRecallMessage(msg({ isOutgoing: false }), now)).toBe(false);
  });

  it("rejects already-recalled / not-yet-sent messages", () => {
    expect(canRecallMessage(msg({ status: "recalled" }), now)).toBe(false);
    expect(canRecallMessage(msg({ status: "pending" }), now)).toBe(false);
    expect(canRecallMessage(msg({ status: "failed" }), now)).toBe(false);
  });

  it("rejects control / ephemeral messages", () => {
    expect(
      canRecallMessage(msg({ content: { type: "reaction", target: "x", emoji: "👍" } }), now),
    ).toBe(false);
    expect(canRecallMessage(msg({ ephemeralTtlMs: 5000 }), now)).toBe(false);
  });

  it("rejects messages older than the recall window", () => {
    const old = msg({ sentAt: now - RECALL_WINDOW_MS - 1 });
    expect(canRecallMessage(old, now)).toBe(false);
    const edge = msg({ sentAt: now - RECALL_WINDOW_MS });
    expect(canRecallMessage(edge, now)).toBe(true);
  });
});
