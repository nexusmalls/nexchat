// EN: Tests for the Track A online-handoff wire-message codec (§5.2): request/grant round-trips +
// rejection of malformed / wrong-version / wrong-shape frames. CN: 路线 A 在线交接线消息编解码（§5.2）
// 单测：request/grant 往返 + 畸形/错版本/错形状帧拒绝。

import { describe, expect, it } from "vitest";

import {
  decodeHandoffMessage,
  encodeHandoffMessage,
  type HandoffGrantMessage,
  type HandoffRequestMessage,
} from "@/mls/handoffMessages";

const request: HandoffRequestMessage = {
  t: "handoff-request",
  v: 1,
  from: "devNew",
  endorsement: { deviceId: "devNew", peerPublicKey: "0xabcd", sig: "0x1234" },
};

const grant: HandoffGrantMessage = {
  t: "handoff-grant",
  v: 1,
  payload: {
    receipt: { receipt: { v: 1, from: "devOld", to: "devNew", seq: 3, ts: 99 }, sig: "0xsig" },
    sealedBundle: "c2VhbGVk",
  },
};

describe("handoff message codec (§5.2)", () => {
  it("round-trips a request", () => {
    expect(decodeHandoffMessage(encodeHandoffMessage(request))).toEqual(request);
  });

  it("round-trips a grant", () => {
    expect(decodeHandoffMessage(encodeHandoffMessage(grant))).toEqual(grant);
  });

  it("rejects malformed JSON / wrong version / unknown type", () => {
    expect(decodeHandoffMessage("not json")).toBeNull();
    expect(decodeHandoffMessage(JSON.stringify({ ...request, v: 2 }))).toBeNull();
    expect(decodeHandoffMessage(JSON.stringify({ t: "nope", v: 1 }))).toBeNull();
  });

  it("rejects a request missing the endorsement", () => {
    expect(decodeHandoffMessage(JSON.stringify({ t: "handoff-request", v: 1, from: "x" }))).toBeNull();
    expect(
      decodeHandoffMessage(
        JSON.stringify({ t: "handoff-request", v: 1, from: "x", endorsement: { deviceId: "x" } }),
      ),
    ).toBeNull();
  });

  it("rejects a grant with a malformed sealed payload", () => {
    expect(
      decodeHandoffMessage(JSON.stringify({ t: "handoff-grant", v: 1, payload: { sealedBundle: "x" } })),
    ).toBeNull();
    expect(
      decodeHandoffMessage(
        JSON.stringify({
          t: "handoff-grant",
          v: 1,
          payload: { sealedBundle: "x", receipt: { sig: "s", receipt: { v: 2 } } },
        }),
      ),
    ).toBeNull();
  });
});
