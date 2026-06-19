import { describe, it, expect, beforeEach } from "vitest";
import {
  burnAtOnCreate,
  burnAtOnRead,
  ephemeralFromEnvelope,
  relayExpiresAt,
} from "@/ephemeral/ephemeral";
import { InMemoryLocalStore } from "@/store/localStore";
import type { MessageVM } from "@/types/viewModels";

function msg(overrides: Partial<MessageVM> = {}): MessageVM {
  return {
    clientMsgId: "c1",
    convId: "g:1",
    senderRef: "a",
    isOutgoing: false,
    sentAt: 1,
    content: { type: "text", text: "x" },
    mentions: [],
    starred: false,
    status: "acked",
    source: "offChainMls",
    ...overrides,
  };
}

describe("ephemeral helpers", () => {
  it("ephemeralFromEnvelope extracts ttl and burnOn", () => {
    const e = ephemeralFromEnvelope({
      v: 1,
      id: "1",
      type: "text",
      body: {},
      ephemeral: { ttlMs: 5000, burnOn: "read" },
    });
    expect(e.ephemeralTtlMs).toBe(5000);
    expect(e.ephemeralBurnOn).toBe("read");
  });

  it("burnAtOnCreate only arms deliver mode immediately", () => {
    expect(burnAtOnCreate(1000, "deliver", 100)).toBe(1100);
    expect(burnAtOnCreate(1000, "read", 100)).toBeUndefined();
  });

  it("relayExpiresAt gives wider window for read mode", () => {
    expect(relayExpiresAt(1000, "deliver", 0)).toBe(1000);
    expect(relayExpiresAt(1000, "read", 0)).toBe(4000);
  });
});

describe("LocalStore ephemeral purge", () => {
  let store: InMemoryLocalStore;
  beforeEach(() => {
    store = new InMemoryLocalStore();
  });

  it("armEphemeralOnRead sets burnAt for read-mode messages", async () => {
    await store.ensureConv("g:1");
    await store.appendMessage(
      msg({ ephemeralTtlMs: 3000, ephemeralBurnOn: "read" }),
    );
    await store.armEphemeralOnRead("g:1", 1000);
    const m = await store.getMessage("g:1", "c1");
    expect(m?.ephemeralBurnAt).toBe(burnAtOnRead(3000, 1000));
  });

  it("purgeExpiredEphemeral removes expired rows", async () => {
    await store.ensureConv("g:1");
    await store.appendMessage(msg({ ephemeralBurnAt: 50 }));
    await store.appendMessage(msg({ clientMsgId: "c2", ephemeralBurnAt: 200 }));
    const hits = await store.purgeExpiredEphemeral(100);
    expect(hits).toEqual([{ convId: "g:1", removed: ["c1"] }]);
    expect(await store.getMessage("g:1", "c1")).toBeUndefined();
    expect(await store.getMessage("g:1", "c2")).toBeDefined();
  });
});
