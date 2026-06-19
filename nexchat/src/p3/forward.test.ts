import { describe, it, expect } from "vitest";
import {
  canForwardMessage,
  fileBodyFromMessage,
  forwardBodyText,
  forwardPreview,
  isMediaForwardReady,
} from "@/p3/forward";
import { fileEnvelope, decodeEnvelope, encodeEnvelope } from "@/mls/envelope";
import type { MessageVM } from "@/types/viewModels";

function baseMsg(content: MessageVM["content"]): MessageVM {
  return {
    clientMsgId: "m1",
    convId: "g:1",
    senderRef: "Alice",
    isOutgoing: false,
    sentAt: 0,
    content,
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
  };
}

describe("canForwardMessage", () => {
  it("allows text and media", () => {
    expect(canForwardMessage(baseMsg({ type: "text", text: "hi" }))).toBe(true);
    expect(
      canForwardMessage(
        baseMsg({
          type: "media",
          mime: "image/png",
          size: 1,
          thumbReady: true,
          bodyReady: true,
          rootCid: "bafytest",
          fileKey: "key",
        }),
      ),
    ).toBe(true);
  });

  it("blocks system and reaction", () => {
    expect(canForwardMessage(baseMsg({ type: "system", kind: "join" }))).toBe(false);
    expect(
      canForwardMessage(baseMsg({ type: "reaction", target: "m0", emoji: "👍" })),
    ).toBe(false);
  });
});

describe("media forward helpers", () => {
  const media = baseMsg({
    type: "media",
    mime: "image/jpeg",
    name: "photo.jpg",
    size: 1024,
    thumbReady: true,
    bodyReady: true,
    rootCid: "bafybeigdyrzt5sfp7udm7rm27znxt",
    fileKey: "abc123",
    thumbCid: "bafkthumb",
    thumbKey: "thumbkey",
  });

  it("detects ready media", () => {
    expect(isMediaForwardReady(media)).toBe(true);
    const body = fileBodyFromMessage(media);
    expect(body?.rootCid).toBe("bafybeigdyrzt5sfp7udm7rm27znxt");
    expect(body?.fileKey).toBe("abc123");
  });

  it("fileEnvelope carries forward ref with preview", () => {
    const body = fileBodyFromMessage(media)!;
    const env = fileEnvelope("id1", "image", body, {
      forward: { fromMsg: "m0", fromConv: "g:2", preview: "[图片]" },
    });
    const back = decodeEnvelope(encodeEnvelope(env));
    expect(back.forward).toEqual({ fromMsg: "m0", fromConv: "g:2", preview: "[图片]" });
  });
});

describe("forwardBodyText", () => {
  it("copies text and media summary", () => {
    expect(forwardBodyText(baseMsg({ type: "text", text: "hello" }))).toBe("hello");
    expect(
      forwardPreview(
        baseMsg({ type: "media", mime: "audio/webm", size: 100, thumbReady: false, bodyReady: false }),
      ),
    ).toContain("audio");
  });
});
