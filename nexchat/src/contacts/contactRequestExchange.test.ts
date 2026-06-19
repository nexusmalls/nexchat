import { beforeEach, describe, expect, it, vi } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import type { ControlInbound, ControlMsg, RelayClient } from "@/relay/relayClient";
import { ContactRequestExchange } from "@/contacts/contactRequestExchange";
import { loadContacts, saveContacts } from "@/store/contactBook";
import { loadContactRequests } from "@/store/contactRequests";
import { NEX_SS58 } from "@/wallet/desktopKeyring";
import { canonicalAddress } from "@/wallet/address";

vi.mock("@/config", async (importOriginal) => {
  const orig = await importOriginal<typeof import("@/config")>();
  return {
    config: { ...orig.config, relayWs: "ws://127.0.0.1:8765" },
  };
});

vi.mock("@/relay/contactRequestInbox", () => ({
  fetchContactInbox: vi.fn(async () => ({ reqs: [], acks: [] })),
  consumeContactInbox: vi.fn(async () => {}),
}));

import { fetchContactInbox, consumeContactInbox } from "@/relay/contactRequestInbox";

// EN: canonical (prefix-273) fixtures — the exchange canonicalizes peer addresses, so raw
// prefix-42 fixtures would mismatch stored/emitted addresses. CN: 规范（前缀 273）夹具——交换
// 会对对端地址归一化，原始前缀 42 夹具会与存储/发出的地址不一致。
const ALICE = canonicalAddress("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY");
const BOB = canonicalAddress("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty");

function mockRelay(): RelayClient & { sent: ControlMsg[]; emit: (m: ControlMsg) => void } {
  const sent: ControlMsg[] = [];
  const handlers: ControlInbound[] = [];
  return {
    sent,
    emit(m) {
      for (const h of handlers) h(m);
    },
    async connect() {},
    async send() {},
    async sendControl(msg) {
      sent.push(msg);
    },
    onMessage() {},
    onControl(cb) {
      handlers.push(cb);
    },
    disconnect() {},
  };
}

beforeEach(() => {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => store.set(k, v),
      removeItem: (k: string) => store.delete(k),
      key: () => null,
      get length() {
        return store.size;
      },
      clear: () => store.clear(),
    },
    configurable: true,
  });
  store.clear();
});

describe("ContactRequestExchange", () => {
  it("sendRequest emits contact_req and stores outbound row", async () => {
    const relay = mockRelay();
    const changes: unknown[] = [];
    const ex = new ContactRequestExchange({
      selfAddress: ALICE,
      selfLabel: "Alice",
      endpointId: "ep-a",
      relay,
      onChange: (rows) => changes.push(rows),
    });
    ex.wire();
    await ex.sendRequest(BOB);
    expect(relay.sent[0]).toMatchObject({ t: "contact_req", toAddr: BOB, fromLabel: "Alice" });
    expect(loadContactRequests(ALICE)[0]?.direction).toBe("outbound");
  });

  it("inbound contact_req creates pending row for recipient", async () => {
    const relay = mockRelay();
    const changes: unknown[] = [];
    const ex = new ContactRequestExchange({
      selfAddress: BOB,
      selfLabel: "Bob",
      endpointId: "ep-b",
      relay,
      onChange: (rows) => changes.push(rows),
    });
    ex.wire();
    relay.emit({
      t: "contact_req",
      from: "ep-a",
      fromAddr: ALICE,
      toAddr: BOB,
      reqId: "req-1",
      fromLabel: "Alice",
      sentAt: 1000,
    });
    await Promise.resolve();
    const rows = loadContactRequests(BOB);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ direction: "inbound", status: "pending", fromLabel: "Alice" });
    expect(changes).toHaveLength(1);
  });

  it("accepts contact_req when toAddr uses NEX SS58 prefix", async () => {
    await cryptoWaitReady();
    const bobNex = new Keyring({ type: "sr25519", ss58Format: NEX_SS58 }).addFromUri("//Bob");
    const relay = mockRelay();
    const changes: unknown[] = [];
    const ex = new ContactRequestExchange({
      selfAddress: BOB,
      selfLabel: "Bob",
      endpointId: "ep-b",
      relay,
      onChange: (rows) => changes.push(rows),
    });
    ex.wire();
    relay.emit({
      t: "contact_req",
      from: "ep-a",
      fromAddr: ALICE,
      toAddr: bobNex.address,
      reqId: "req-nex",
      fromLabel: "Alice",
      sentAt: 1000,
    });
    await Promise.resolve();
    expect(loadContactRequests(BOB)[0]?.reqId).toBe("req-nex");
    expect(changes).toHaveLength(1);
  });

  it("accept ack triggers onAutoAccept for original sender", async () => {
    const relayA = mockRelay();
    const autoAccept = vi.fn(async () => {});

    const exA = new ContactRequestExchange({
      selfAddress: ALICE,
      selfLabel: "Alice",
      endpointId: "ep-a",
      relay: relayA,
      onChange: () => {},
      onAutoAccept: autoAccept,
    });
    exA.wire();

    relayA.emit({
      t: "contact_ack",
      from: "ep-b",
      fromAddr: BOB,
      toAddr: ALICE,
      reqId: "req-2",
      action: "accept",
      label: "Bob",
    });
    await Promise.resolve();

    expect(autoAccept).toHaveBeenCalledWith(BOB, "Bob");
  });

  it("syncInbox processes relay mailbox and consumes", async () => {
    vi.mocked(fetchContactInbox).mockResolvedValueOnce({
      reqs: [
        {
          t: "contact_req",
          from: "ep-a",
          fromAddr: ALICE,
          toAddr: BOB,
          reqId: "req-sync",
          fromLabel: "Alice",
          sentAt: 2000,
        },
      ],
      acks: [],
    });
    const relay = mockRelay();
    const ex = new ContactRequestExchange({
      selfAddress: BOB,
      selfLabel: "Bob",
      endpointId: "ep-b",
      relay,
      onChange: () => {},
    });
    await ex.syncInbox();
    expect(loadContactRequests(BOB)[0]?.reqId).toBe("req-sync");
    expect(consumeContactInbox).toHaveBeenCalledWith(BOB, ["req-sync"], []);
  });

  it("inbound req auto-acks when peer already in contacts", async () => {
    saveContacts(BOB, [{ address: ALICE, label: "Alice", addedAt: 1 }]);
    const relay = mockRelay();
    const ensureHandshake = vi.fn();
    const ex = new ContactRequestExchange({
      selfAddress: BOB,
      selfLabel: "Bob",
      endpointId: "ep-b",
      relay,
      onChange: () => {},
      onEnsureHandshake: ensureHandshake,
    });
    ex.wire();
    relay.emit({
      t: "contact_req",
      from: "ep-a",
      fromAddr: ALICE,
      toAddr: BOB,
      reqId: "req-3",
      fromLabel: "Alice",
      sentAt: 1000,
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(relay.sent[0]).toMatchObject({ t: "contact_ack", action: "accept", label: "Alice" });
    expect(loadContacts(BOB)).toHaveLength(1);
    expect(ensureHandshake).toHaveBeenCalledWith(ALICE);
  });

  it("accept ack for existing contact retriggers handshake", async () => {
    saveContacts(ALICE, [{ address: BOB, label: "Bob", addedAt: 1 }]);
    const relay = mockRelay();
    const autoAccept = vi.fn(async () => {});
    const ensureHandshake = vi.fn();
    const ex = new ContactRequestExchange({
      selfAddress: ALICE,
      selfLabel: "Alice",
      endpointId: "ep-a",
      relay,
      onChange: () => {},
      onAutoAccept: autoAccept,
      onEnsureHandshake: ensureHandshake,
    });
    ex.wire();
    relay.emit({
      t: "contact_ack",
      from: "ep-b",
      fromAddr: BOB,
      toAddr: ALICE,
      reqId: "req-4",
      action: "accept",
      label: "Bob",
    });
    await Promise.resolve();
    expect(autoAccept).not.toHaveBeenCalled();
    expect(ensureHandshake).toHaveBeenCalledWith(BOB);
  });
});
