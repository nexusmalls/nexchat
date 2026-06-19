// EN: group invite mailbox consume must authenticate via relayOneShotSend (writer gate).
// CN: 群邀请邮箱 consume 须经 relayOneShotSend 鉴权（writer 门禁）。

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/config", () => ({
  config: { relayWs: "ws://test-relay" },
}));

const relayOneShotSend = vi.hoisted(() => vi.fn(async () => {}));

vi.mock("@/relay/relayOneShot", () => ({
  relayOneShotSend,
}));

import { consumeGroupInviteInbox } from "@/relay/groupInviteInbox";

describe("consumeGroupInviteInbox", () => {
  beforeEach(() => {
    relayOneShotSend.mockClear();
  });

  it("sends authenticated group_invite_consume with account and invite_ids", async () => {
    await consumeGroupInviteInbox("5Alice", ["inv-1", "inv-2"]);
    expect(relayOneShotSend).toHaveBeenCalledWith(
      "5Alice",
      {
        type: "group_invite_consume",
        account: "5Alice",
        invite_ids: ["inv-1", "inv-2"],
      },
      { noReply: true },
    );
  });

  it("skips when invite_ids is empty", async () => {
    await consumeGroupInviteInbox("5Alice", []);
    expect(relayOneShotSend).not.toHaveBeenCalled();
  });
});
