import { beforeEach, describe, expect, it } from "vitest";
import {
  loadContactRequests,
  pendingInboundCount,
  pruneStaleRequests,
  saveContactRequests,
  updateRequestStatus,
  upsertRequest,
} from "@/store/contactRequests";

const ACCOUNT = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

beforeEach(() => {
  const store = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    value: {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
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

describe("contactRequests", () => {
  it("upsert and update status", () => {
    upsertRequest(ACCOUNT, {
      reqId: "r1",
      peerAddress: "peer-a",
      fromLabel: "Alice",
      direction: "inbound",
      status: "pending",
      sentAt: 100,
      updatedAt: 100,
    });
    expect(loadContactRequests(ACCOUNT)).toHaveLength(1);
    updateRequestStatus(ACCOUNT, "r1", "accepted", 200);
    expect(loadContactRequests(ACCOUNT)[0]!.status).toBe("accepted");
    expect(pendingInboundCount(loadContactRequests(ACCOUNT))).toBe(0);
  });

  it("pruneStaleRequests drops old outbound pending", () => {
    saveContactRequests(ACCOUNT, [
      {
        reqId: "old",
        peerAddress: "p",
        fromLabel: "x",
        direction: "outbound",
        status: "pending",
        sentAt: 0,
        updatedAt: 0,
      },
      {
        reqId: "new",
        peerAddress: "p2",
        fromLabel: "y",
        direction: "inbound",
        status: "pending",
        sentAt: Date.now(),
        updatedAt: Date.now(),
      },
    ]);
    const rows = pruneStaleRequests(ACCOUNT, 31 * 24 * 60 * 60 * 1000);
    expect(rows.map((r) => r.reqId)).toEqual(["new"]);
  });
});
