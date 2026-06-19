// EN: relay reject envelope parsers. CN: relay 拒绝回执解析。

import { describe, expect, it } from "vitest";
import {
  RelayAuthRejectError,
  RelayInboxStaleEpochError,
  RelayStalePointerError,
  frameRejectHint,
  parseInboxStaleEpochReject,
  parseStalePointerReject,
  rejectTypeForAck,
  relayErrorFromWire,
} from "@/relay/relayErrors";

describe("relayErrors", () => {
  it("maps ack types to reject types", () => {
    expect(rejectTypeForAck("index_ack")).toBe("index_reject");
    expect(rejectTypeForAck("inbox_ack")).toBe("inbox_reject");
    expect(rejectTypeForAck("chat_ack")).toBe("chat_reject");
  });

  it("parses stale pointer reject", () => {
    const err = parseStalePointerReject({
      type: "index_reject",
      reason: "stale_updated_at",
      updated_at: 42,
    });
    expect(err).toBeInstanceOf(RelayStalePointerError);
    expect(err?.remoteUpdatedAt).toBe(42);
  });

  it("parses inbox stale epoch reject", () => {
    const err = parseInboxStaleEpochReject({
      type: "inbox_reject",
      reason: "stale_epoch",
      epoch: 7,
    });
    expect(err).toBeInstanceOf(RelayInboxStaleEpochError);
    expect(err?.remoteEpoch).toBe(7);
  });

  it("relayErrorFromWire filters pointer reject by expected ack", () => {
    const err = relayErrorFromWire(
      { type: "contacts_reject", reason: "stale_updated_at", updated_at: 1 },
      "index_ack",
    );
    expect(err).toBeNull();
    const hit = relayErrorFromWire(
      { type: "index_reject", reason: "stale_updated_at", updated_at: 2 },
      "index_ack",
    );
    expect(hit).toBeInstanceOf(RelayStalePointerError);
  });

  it("parses auth_reject", () => {
    const err = relayErrorFromWire({ type: "auth_reject", op: "chat_consume" });
    expect(err).toBeInstanceOf(RelayAuthRejectError);
    expect((err as RelayAuthRejectError).op).toBe("chat_consume");
  });

  it("frameRejectHint covers delivery_rejected", () => {
    expect(frameRejectHint("delivery_rejected")).toContain("盲签");
    expect(frameRejectHint("rate_limited")).toContain("频繁");
  });
});
