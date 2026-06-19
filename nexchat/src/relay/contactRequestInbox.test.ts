// EN: contact mailbox fetch/consume relay wiring. CN: contact 邮箱 fetch/consume 接线。

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/config", () => ({ config: { relayWs: "ws://test" } }));

const relayOneShotFetch = vi.hoisted(() => vi.fn(async () => ({ reqs: [], acks: [] })));
const relayOneShotSend = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/relay/relayOneShot", () => ({
  relayOneShotFetch,
  relayOneShotSend,
}));

import { consumeContactInbox, fetchContactInbox } from "@/relay/contactRequestInbox";

describe("fetchContactInbox", () => {
  it("uses relayOneShotFetch with contact_fetch", async () => {
    await fetchContactInbox("5Bob");
    expect(relayOneShotFetch).toHaveBeenCalledWith(
      "5Bob",
      { type: "contact_fetch" },
      expect.any(Function),
      5000,
    );
  });
});

describe("consumeContactInbox", () => {
  beforeEach(() => {
    relayOneShotSend.mockClear();
  });

  it("sends authenticated contact_consume with account and ids", async () => {
    await consumeContactInbox("5Bob", ["req-1"], ["ack-2"]);
    expect(relayOneShotSend).toHaveBeenCalledWith(
      "5Bob",
      {
        type: "contact_consume",
        account: "5Bob",
        req_ids: ["req-1"],
        ack_ids: ["ack-2"],
      },
      { noReply: true },
    );
  });

  it("skips when both id lists are empty", async () => {
    await consumeContactInbox("5Bob", [], []);
    expect(relayOneShotSend).not.toHaveBeenCalled();
  });
});
