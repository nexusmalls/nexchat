// EN: Unit tests for the coordinator's pure decision logic (manifest build, §6.2
// per-field chain LWW injection, §6.3 relay write-back, §6.1 backoff).
// CN: coordinator 纯决策逻辑单测（清单构建、§6.2 逐字段链上 LWW 注入、§6.3 relay 写回、
// §6.1 退避）。

import { describe, expect, it, vi } from "vitest";

vi.mock("@/config", async (importActual) => {
  const actual = await importActual<typeof import("@/config")>();
  return {
    ...actual,
    signingPinBackupActive: () => false,
  };
});

import {
  buildManifestFromPointers,
  chainNewerFields,
  nextBackoffMs,
  relayWriteBackFields,
} from "@/store/offchainSyncCoordinator";
import type { SyncManifest } from "@/store/syncAnchor";

describe("buildManifestFromPointers", () => {
  it("builds a manifest with top-level updated_at = max of fields", () => {
    const m = buildManifestFromPointers({
      index: { cid: "bafyIndex", updated_at: 100 },
      contacts: { cid: "bafyContacts", updated_at: 300 },
      archive: null,
    });
    expect(m).toEqual({
      v: 1,
      updated_at: 300,
      index: { cid: "bafyIndex", updated_at: 100 },
      contacts: { cid: "bafyContacts", updated_at: 300 },
    });
    expect(m && "archive" in m && m.archive).toBeFalsy();
  });

  it("returns null when no slot has data", () => {
    expect(buildManifestFromPointers({})).toBeNull();
    expect(buildManifestFromPointers({ index: null, contacts: null, archive: null })).toBeNull();
    expect(buildManifestFromPointers({ index: { cid: "", updated_at: 5 } })).toBeNull();
    expect(buildManifestFromPointers({ index: { cid: "bafy", updated_at: 0 } })).toBeNull();
  });

  it("includes the Track A mls slot (design §4/§13) and counts it toward updated_at", () => {
    const m = buildManifestFromPointers({
      index: { cid: "bafyIndex", updated_at: 100 },
      mls: { cid: "bafyMlsVault", updated_at: 700 },
    });
    expect(m).toEqual({
      v: 1,
      updated_at: 700,
      index: { cid: "bafyIndex", updated_at: 100 },
      mls: { cid: "bafyMlsVault", updated_at: 700 },
    });
  });

  it("builds a manifest from the mls slot alone", () => {
    expect(buildManifestFromPointers({ mls: { cid: "bafyMls", updated_at: 42 } })).toEqual({
      v: 1,
      updated_at: 42,
      mls: { cid: "bafyMls", updated_at: 42 },
    });
  });

  it("omits mls_signing when PIN backup is disabled (default)", () => {
    const m = buildManifestFromPointers({
      index: { cid: "bafyIndex", updated_at: 100 },
      mls_signing: { cid: "bafySigning", updated_at: 900 },
    });
    expect(m).toEqual({
      v: 1,
      updated_at: 100,
      index: { cid: "bafyIndex", updated_at: 100 },
    });
    expect(m && "mls_signing" in m).toBe(false);
  });
});

describe("chainNewerFields (§6.2 per-field LWW)", () => {
  const manifest: SyncManifest = {
    v: 1,
    updated_at: 900,
    index: { cid: "chainIndex", updated_at: 500 },
    contacts: { cid: "chainContacts", updated_at: 900 },
    archive: { cid: "chainArchive", updated_at: 100 },
    mls: { cid: "chainMls", updated_at: 800 },
  };

  it("picks only fields where chain is strictly newer", () => {
    const injected = chainNewerFields(manifest, {
      index: { cid: "localIndex", updated_at: 500 }, // equal → keep local
      contacts: { cid: "localContacts", updated_at: 100 }, // chain newer
      archive: { cid: "localArchive", updated_at: 200 }, // local newer
      mls: { cid: "localMls", updated_at: 400 }, // chain newer
    });
    expect(injected).toEqual([
      ["contacts", { cid: "chainContacts", updated_at: 900 }],
      ["mls", { cid: "chainMls", updated_at: 800 }],
    ]);
  });

  it("injects all chain fields when relay/local are empty (empty-relay self-heal)", () => {
    const injected = chainNewerFields(manifest, {
      index: null,
      contacts: null,
      archive: null,
      mls: null,
    });
    expect(injected.map(([f]) => f)).toEqual(["index", "contacts", "archive", "mls"]);
  });

  it("handles null manifest and manifests with missing fields", () => {
    expect(chainNewerFields(null, {})).toEqual([]);
    const partial: SyncManifest = { v: 1, updated_at: 5, index: { cid: "x", updated_at: 5 } };
    expect(chainNewerFields(partial, {})).toEqual([["index", { cid: "x", updated_at: 5 }]]);
  });
});

describe("relayWriteBackFields (§6.3)", () => {
  it("writes back fields missing from or stale on the relay", () => {
    const out = relayWriteBackFields(
      {
        index: { cid: "effIndex", updated_at: 500 },
        contacts: { cid: "effContacts", updated_at: 300 },
        archive: { cid: "effArchive", updated_at: 700 },
        mls: { cid: "effMls", updated_at: 900 }, // stale on relay → write back
      },
      {
        index: null, // missing → write back
        contacts: { cid: "relayContacts", updated_at: 300 }, // equal → skip
        archive: { cid: "relayArchive", updated_at: 100 }, // stale → write back
        mls: { cid: "relayMls", updated_at: 800 }, // stale → write back
      },
    );
    expect(out).toEqual([
      ["index", { cid: "effIndex", updated_at: 500 }],
      ["archive", { cid: "effArchive", updated_at: 700 }],
      ["mls", { cid: "effMls", updated_at: 900 }],
    ]);
  });

  it("never writes back empty effective fields", () => {
    expect(relayWriteBackFields({ index: null }, { index: null })).toEqual([]);
  });
});

describe("nextBackoffMs (§6.1: 1min base, exponential, 1h cap)", () => {
  it("doubles from 1min and caps at 1h", () => {
    expect(nextBackoffMs(0)).toBe(60_000);
    expect(nextBackoffMs(1)).toBe(120_000);
    expect(nextBackoffMs(5)).toBe(1_920_000);
    expect(nextBackoffMs(6)).toBe(3_600_000);
    expect(nextBackoffMs(20)).toBe(3_600_000);
  });
});
