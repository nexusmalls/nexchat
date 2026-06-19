import { describe, it, expect } from "vitest";
import {
  isMentioned,
  parseMentionTokens,
  resolveMentions,
  rosterFromSeeds,
} from "@/p3/mentions";
import { forwardBodyText } from "@/p3/forward";
import { textEnvelope, decodeEnvelope, encodeEnvelope } from "@/mls/envelope";
import type { MessageVM } from "@/types/viewModels";

describe("mentions", () => {
  const roster = rosterFromSeeds(["//Alice", "//Bob"], ["addr-a", "addr-b"]);

  it("parseMentionTokens extracts unique @labels", () => {
    expect(parseMentionTokens("hi @Alice and @bob again @Alice")).toEqual(["Alice", "bob"]);
  });

  it("resolveMentions maps tokens to roster refs", () => {
    expect(resolveMentions(["alice", "Bob"], roster)).toEqual(["Alice", "Bob"]);
  });

  it("isMentioned matches ref or label", () => {
    expect(isMentioned(["Alice"], roster[0]!)).toBe(true);
    expect(isMentioned(["addr-b"], roster[1]!)).toBe(true);
    expect(isMentioned(["Charlie"], roster[0]!)).toBe(false);
  });
});

describe("forward envelope", () => {
  it("textEnvelope carries forward ref round-trip", () => {
    const env = textEnvelope("id1", "fwd body", {
      forward: { fromMsg: "m0", fromConv: "g:1" },
      mentions: ["Bob"],
    });
    const back = decodeEnvelope(encodeEnvelope(env));
    expect(back.forward).toEqual({ fromMsg: "m0", fromConv: "g:1" });
    expect(back.mentions).toEqual(["Bob"]);
  });

  it("forwardBodyText copies text and media summary", () => {
    const text: MessageVM = {
      clientMsgId: "1",
      convId: "g:1",
      senderRef: "a",
      isOutgoing: false,
      sentAt: 0,
      content: { type: "text", text: "hello" },
      mentions: [],
      starred: false,
      status: "acked",
      source: "offChainMls",
    };
    expect(forwardBodyText(text)).toBe("hello");
  });
});
