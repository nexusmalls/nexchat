// EN: Unit tests for the Track A MLS vault envelope codec + §4.4 concurrent-write merge decision.
// CN: 路线 A MLS vault 信封编解码 + §4.4 并发写合并决策的单测。

import { describe, expect, it } from "vitest";

import {
  decideVaultMerge,
  decodeVaultEnvelope,
  encodeVaultEnvelope,
  type VaultEnvelope,
} from "@/store/mlsVaultMerge";

const env = (over: Partial<VaultEnvelope> = {}): VaultEnvelope => ({
  v: 1,
  updated_at: 1000,
  deviceSeq: 1,
  groups: {},
  prevCid: null,
  blob: "AAAA",
  ...over,
});

describe("vault envelope codec", () => {
  it("round-trips through encode/decode", () => {
    const e = env({ groups: { "g:1": 5, "g:2": 0 }, prevCid: "bafyPrev", deviceSeq: 7 });
    expect(decodeVaultEnvelope(encodeVaultEnvelope(e))).toEqual(e);
  });

  it("rejects an unsupported version", () => {
    const bad = new TextEncoder().encode(JSON.stringify({ ...env(), v: 2 }));
    expect(() => decodeVaultEnvelope(bad)).toThrow(/version/);
  });

  it("rejects a missing blob / groups", () => {
    const noBlob = new TextEncoder().encode(JSON.stringify({ ...env(), blob: "" }));
    expect(() => decodeVaultEnvelope(noBlob)).toThrow(/blob/);
    const noGroups = new TextEncoder().encode(
      JSON.stringify({ v: 1, updated_at: 1, deviceSeq: 1, prevCid: null, blob: "AAAA" }),
    );
    expect(() => decodeVaultEnvelope(noGroups)).toThrow(/groups/);
  });

  it("tolerates missing optional fields with defaults", () => {
    const minimal = new TextEncoder().encode(JSON.stringify({ v: 1, groups: { "g:1": 3 }, blob: "Qg==" }));
    expect(decodeVaultEnvelope(minimal)).toEqual({
      v: 1,
      updated_at: 0,
      deviceSeq: 0,
      groups: { "g:1": 3 },
      prevCid: null,
      blob: "Qg==",
    });
  });
});

describe("decideVaultMerge (§4.4 per-group max-epoch + prev_cid CAS)", () => {
  it("publishes when no concurrent write since our base (cid unchanged)", () => {
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 5 },
        remote: env({ groups: { "g:1": 5 } }),
        prevCid: "bafyBase",
        remoteCid: "bafyBase",
      }),
    ).toEqual({ action: "publish" });
  });

  it("publishes when the remote slot is empty", () => {
    expect(
      decideVaultMerge({ localGroups: { "g:1": 5 }, remote: null, prevCid: null, remoteCid: null }),
    ).toEqual({ action: "publish" });
  });

  it("skips (remote-newer) when a concurrent device is strictly ahead on a group", () => {
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 5, "g:2": 3 },
        remote: env({ groups: { "g:1": 5, "g:2": 9 } }),
        prevCid: "bafyOld",
        remoteCid: "bafyNew",
      }),
    ).toEqual({ action: "skip", reason: "remote-newer" });
  });

  it("publishes-rebased when we strictly dominate a concurrent remote", () => {
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 6, "g:2": 3 },
        remote: env({ groups: { "g:1": 5, "g:2": 3 } }),
        prevCid: "bafyOld",
        remoteCid: "bafyNew",
      }),
    ).toEqual({ action: "publish-rebased", basedOnCid: "bafyNew" });
  });

  it("skips (divergent) when each side is ahead on a different group", () => {
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 6, "g:2": 3 },
        remote: env({ groups: { "g:1": 5, "g:2": 4 } }),
        prevCid: "bafyOld",
        remoteCid: "bafyNew",
      }),
    ).toEqual({ action: "skip", reason: "divergent" });
  });

  it("skips (no-op) when a concurrent remote has identical per-group epochs", () => {
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 5 },
        remote: env({ groups: { "g:1": 5 } }),
        prevCid: "bafyOld",
        remoteCid: "bafyNew",
      }),
    ).toEqual({ action: "skip", reason: "no-op" });
  });

  it("treats a group missing on one side as epoch 0", () => {
    // local has g:2 (the remote never saw it) → local strictly ahead → rebase-publish.
    expect(
      decideVaultMerge({
        localGroups: { "g:1": 5, "g:2": 1 },
        remote: env({ groups: { "g:1": 5 } }),
        prevCid: "bafyOld",
        remoteCid: "bafyNew",
      }),
    ).toEqual({ action: "publish-rebased", basedOnCid: "bafyNew" });
  });
});
