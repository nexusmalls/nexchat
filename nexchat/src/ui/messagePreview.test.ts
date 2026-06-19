import { describe, expect, it } from "vitest";
import type { MessageVM } from "@/types/viewModels";
import {
  contentPreviewFromMessage,
  isDegradedReplyQuote,
  replyQuotePreview,
} from "@/ui/messagePreview";

function textMsg(id: string, text: string, status: MessageVM["status"] = "acked"): MessageVM {
  return {
    clientMsgId: id,
    convId: "d:bob",
    senderRef: "bob",
    isOutgoing: false,
    sentAt: 1,
    content: { type: "text", text },
    mentions: [],
    starred: false,
    status,
    source: "offChainMls",
  };
}

describe("replyQuotePreview", () => {
  it("returns undefined when there is no reply target id", () => {
    expect(replyQuotePreview(undefined, undefined)).toBeUndefined();
  });

  it("shows deleted placeholder when target is missing", () => {
    expect(replyQuotePreview("m1", undefined)).toBe("原消息已删除");
  });

  it("shows recalled placeholder when target is recalled", () => {
    expect(replyQuotePreview("m1", textMsg("m1", "", "recalled"))).toBe("消息已撤回");
  });

  it("shows content preview for a live target", () => {
    expect(replyQuotePreview("m1", textMsg("m1", "hello"))).toBe("hello");
    expect(contentPreviewFromMessage(textMsg("m1", "hello"))).toBe("hello");
  });

  it("flags degraded placeholders", () => {
    expect(isDegradedReplyQuote("原消息已删除")).toBe(true);
    expect(isDegradedReplyQuote("消息已撤回")).toBe(true);
    expect(isDegradedReplyQuote("hello")).toBe(false);
  });
});
