import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@/config", () => ({
  config: {
    ipfsMediaLocalPinEnabled: true,
    ipfsMediaLocalPinTtlMs: 86_400_000,
  },
}));

const unpinMock = vi.fn(async (_cid: string) => {});

vi.mock("@/ipfs/ipfsClient", () => ({
  ipfsClient: {
    unpin: (cid: string) => unpinMock(cid),
  },
}));

import {
  clearSenderMediaRetentionForTest,
  collectCidsFromUpload,
  exemptRetentionForMessage,
  listSenderMediaRetentionForTest,
  registerUploadedMedia,
  runSenderMediaRetentionCleanup,
  shortenRetentionForMessage,
} from "@/ipfs/senderMediaRetention";
import type { UploadedEncryptedFile } from "@/ipfs/media";

const sampleUpload: UploadedEncryptedFile = {
  rootCid: "bafyroot",
  fileKey: "key",
  mime: "image/png",
  size: 100,
  name: "a.png",
  chunked: true,
  thumbCid: "bafythumb",
  chunkCids: [{ cid: "bafypart0", sizeBytes: 50 }],
};

describe("collectCidsFromUpload", () => {
  it("collects root, thumb, and chunk cids uniquely", () => {
    expect(collectCidsFromUpload(sampleUpload).sort()).toEqual(
      ["bafypart0", "bafyroot", "bafythumb"].sort(),
    );
  });
});

describe("sender media retention registry", () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => {
        storage.set(k, v);
      },
      removeItem: (k: string) => {
        storage.delete(k);
      },
    });
    clearSenderMediaRetentionForTest();
    unpinMock.mockClear();
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-12T00:00:00Z"));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("registerUploadedMedia stores cids with ttl", () => {
    registerUploadedMedia(sampleUpload, { clientMsgId: "m1", convId: "g:1" });
    const rows = listSenderMediaRetentionForTest();
    expect(rows).toHaveLength(3);
    expect(rows.every((r) => r.expiresAt === Date.now() + 86_400_000)).toBe(true);
  });

  it("runSenderMediaRetentionCleanup unpins expired rows", async () => {
    registerUploadedMedia(sampleUpload, { ttlMs: 1000 });
    vi.setSystemTime(new Date("2026-06-12T00:00:02Z"));
    const removed = await runSenderMediaRetentionCleanup();
    expect(removed).toBe(3);
    expect(unpinMock).toHaveBeenCalledTimes(3);
    expect(listSenderMediaRetentionForTest()).toHaveLength(0);
  });

  it("runSenderMediaRetentionCleanup keeps future rows", async () => {
    registerUploadedMedia(sampleUpload);
    const removed = await runSenderMediaRetentionCleanup();
    expect(removed).toBe(0);
    expect(unpinMock).not.toHaveBeenCalled();
    expect(listSenderMediaRetentionForTest()).toHaveLength(3);
  });

  it("shortenRetentionForMessage caps ttl to the grace window", () => {
    registerUploadedMedia(sampleUpload, { clientMsgId: "m1", convId: "d:peer" });
    shortenRetentionForMessage("d:peer", "m1", 3_600_000);
    const rows = listSenderMediaRetentionForTest();
    expect(rows.every((r) => r.expiresAt === Date.now() + 3_600_000)).toBe(true);
    // unrelated message untouched / 不影响其他消息
    registerUploadedMedia(
      { ...sampleUpload, rootCid: "bafyother", thumbCid: undefined, chunkCids: undefined },
      { clientMsgId: "m2", convId: "d:peer" },
    );
    shortenRetentionForMessage("d:peer", "m1", 1_000);
    const other = listSenderMediaRetentionForTest().find((r) => r.cid === "bafyother");
    expect(other!.expiresAt).toBe(Date.now() + 86_400_000);
  });

  it("exemptRetentionForMessage removes rows so the sweep never unpins", async () => {
    registerUploadedMedia(sampleUpload, { clientMsgId: "m1", convId: "g:1", ttlMs: 1000 });
    exemptRetentionForMessage("g:1", "m1");
    expect(listSenderMediaRetentionForTest()).toHaveLength(0);
    vi.setSystemTime(new Date("2026-06-12T01:00:00Z"));
    expect(await runSenderMediaRetentionCleanup()).toBe(0);
    expect(unpinMock).not.toHaveBeenCalled();
  });
});
