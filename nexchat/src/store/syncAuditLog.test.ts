import { afterEach, describe, expect, it } from "vitest";

import {
  appendSyncAudit,
  clearSyncAudit,
  readSyncAudit,
  type SyncAuditRecord,
} from "@/store/syncAuditLog";

// EN: minimal localStorage shim (node test env). CN: 最小 localStorage 垫片（node 测试环境）。
class MemStorage {
  private m = new Map<string, string>();
  getItem(k: string) {
    return this.m.has(k) ? this.m.get(k)! : null;
  }
  setItem(k: string, v: string) {
    this.m.set(k, v);
  }
  removeItem(k: string) {
    this.m.delete(k);
  }
  clear() {
    this.m.clear();
  }
}
(globalThis as Record<string, unknown>).localStorage = new MemStorage();

const ACCOUNT = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

function record(at: number, account = ACCOUNT): SyncAuditRecord {
  return {
    at,
    account,
    tier: "standard",
    phase: "ok",
    usedChainAnchor: false,
    needsEpochBump: false,
    durationMs: 12,
    restored: { contacts: true, convIndex: true, msgArchive: true },
    fields: [
      { field: "index", source: "relay", chainInjected: false, writeBack: "skip", effectiveUpdatedAt: at },
    ],
  };
}

afterEach(() => clearSyncAudit(ACCOUNT));

describe("syncAuditLog", () => {
  it("appends and reads oldest→newest", () => {
    appendSyncAudit(record(1));
    appendSyncAudit(record(2));
    const rows = readSyncAudit(ACCOUNT);
    expect(rows.map((r) => r.at)).toEqual([1, 2]);
  });

  it("rings the buffer at 50 newest records", () => {
    for (let i = 1; i <= 60; i++) appendSyncAudit(record(i));
    const rows = readSyncAudit(ACCOUNT);
    expect(rows).toHaveLength(50);
    expect(rows[0]!.at).toBe(11);
    expect(rows.at(-1)!.at).toBe(60);
  });

  it("isolates records per account and clears", () => {
    const OTHER = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
    appendSyncAudit(record(1));
    appendSyncAudit(record(2, OTHER));
    expect(readSyncAudit(ACCOUNT)).toHaveLength(1);
    expect(readSyncAudit(OTHER)).toHaveLength(1);
    clearSyncAudit(ACCOUNT);
    expect(readSyncAudit(ACCOUNT)).toHaveLength(0);
    expect(readSyncAudit(OTHER)).toHaveLength(1);
    clearSyncAudit(OTHER);
  });

  it("returns [] for empty account or missing data", () => {
    expect(readSyncAudit("")).toEqual([]);
    expect(readSyncAudit("nonexistent")).toEqual([]);
  });
});
