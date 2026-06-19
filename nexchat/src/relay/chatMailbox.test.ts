// EN: Unit tests for chat mailbox reply parsing.
// CN: 聊天邮箱 reply 解析单测。

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/config", () => ({
  config: { relayWs: "ws://test-relay" },
}));

const relayOneShotSend = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/relay/relayOneShot", () => ({
  relayOneShotFetch: vi.fn(),
  relayOneShotSend,
}));

import { consumeChatMailbox, fetchChatMailbox, parseChatMailboxReply } from "@/relay/chatMailbox";
import { relayOneShotFetch } from "@/relay/relayOneShot";

describe("parseChatMailboxReply", () => {
  it("parses chat_reply frames", () => {
    const requestId = "req-1";
    const frames = parseChatMailboxReply(
      JSON.stringify({
        type: "chat_reply",
        request_id: requestId,
        frames: [
          {
            convId: "d:5Bob",
            senderRef: "5Alice",
            ciphertextB64: "AQID",
            dedupKey: "d:5Bob:m1",
          },
          { bad: true },
        ],
      }),
      requestId,
    );
    expect(frames).toHaveLength(1);
    expect(frames?.[0]?.dedupKey).toBe("d:5Bob:m1");
  });

  it("returns null for wrong request_id", () => {
    expect(
      parseChatMailboxReply(
        JSON.stringify({ type: "chat_reply", request_id: "a", frames: [] }),
        "b",
      ),
    ).toBeNull();
  });
});

describe("consumeChatMailbox", () => {
  beforeEach(() => {
    relayOneShotSend.mockClear();
  });

  it("registers and sends account + dedup_keys for chat_consume", async () => {
    await consumeChatMailbox("5Alice", ["d:5Bob:m1", "d:5Bob:m2"]);
    expect(relayOneShotSend).toHaveBeenCalledWith(
      "5Alice",
      {
        type: "chat_consume",
        account: "5Alice",
        dedup_keys: ["d:5Bob:m1", "d:5Bob:m2"],
      },
      { ackType: "chat_ack" },
    );
  });

  it("skips when dedup_keys is empty", async () => {
    await consumeChatMailbox("5Alice", []);
    expect(relayOneShotSend).not.toHaveBeenCalled();
  });
});

describe("fetchChatMailbox", () => {
  beforeEach(() => {
    vi.mocked(relayOneShotFetch).mockReset();
  });

  it("uses signed register_account via relayOneShotFetch", async () => {
    vi.mocked(relayOneShotFetch).mockImplementation(async (_acct, _msg, parse) => {
      const frames = parse(
        {
          type: "chat_reply",
          request_id: "req-1",
          frames: [
            {
              convId: "d:5Bob",
              senderRef: "5Alice",
              ciphertextB64: "AQID",
              dedupKey: "d:5Bob:m1",
            },
          ],
        },
        "req-1",
      );
      return frames ?? null;
    });
    const frames = await fetchChatMailbox("5Bob");
    expect(relayOneShotFetch).toHaveBeenCalledWith(
      "5Bob",
      { type: "chat_fetch" },
      expect.any(Function),
      8000,
    );
    expect(frames).toHaveLength(1);
    expect(frames[0]?.dedupKey).toBe("d:5Bob:m1");
  });
});
