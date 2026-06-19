import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@/config", () => ({
  config: {
    ipfsChunkThreshold: 64,
    ipfsChunkSize: 32,
    ipfsThumbMaxPx: 320,
    ipfsMediaLocalPinEnabled: true,
  },
}));

const store = new Map<string, Uint8Array>();
const addOpts = new Map<string, { pin?: boolean }>();
let cidSeq = 0;

vi.mock("@/ipfs/ipfsClient", () => ({
  ipfsClient: {
    add: vi.fn(async (data: Uint8Array, name: string, options?: { pin?: boolean }) => {
      const cid = `bafytest${cidSeq++}-${name}`;
      store.set(cid, new Uint8Array(data));
      addOpts.set(cid, options ?? {});
      return cid;
    }),
    cat: vi.fn(async (cid: string) => {
      const v = store.get(cid);
      if (!v) throw new Error(`missing cid ${cid}`);
      return new Uint8Array(v);
    }),
  },
}));

import { shouldChunk, uploadEncryptedFile, fetchDecryptedFile, sha256Hex } from "@/ipfs/media";
import { decryptChunk, encryptChunk, decryptManifest, encryptManifest } from "@/ipfs/fileCrypto";
import { decodeManifest, encodeManifest } from "@/ipfs/manifest";
import { exportFileKeyB64, generateFileKey } from "@/ipfs/fileCrypto";

describe("shouldChunk", () => {
  it("chunks when over threshold or video mime", () => {
    expect(shouldChunk(100, "image/png")).toBe(true);
    expect(shouldChunk(10, "video/mp4")).toBe(true);
    expect(shouldChunk(10, "image/png")).toBe(false);
  });
});

describe("chunk crypto", () => {
  it("encryptChunk → decryptChunk round-trips", async () => {
    const key = await generateFileKey();
    const fileKeyB64 = await exportFileKeyB64(key);
    const plain = new TextEncoder().encode("chunk-payload-xyz");
    const ct = await encryptChunk(plain, fileKeyB64, 3);
    const back = await decryptChunk(ct, fileKeyB64, 3);
    expect(new TextDecoder().decode(back)).toBe("chunk-payload-xyz");
  });

  it("manifest encrypt → decrypt round-trips", async () => {
    const key = await generateFileKey();
    const fileKeyB64 = await exportFileKeyB64(key);
    const manifest = encodeManifest({
      v: 1,
      size: 100,
      chunkSize: 32,
      mime: "application/octet-stream",
      fileSha256: "abc",
      chunks: [{ cid: "Qm1", nonce: 0, sha256: "def" }],
    });
    const sealed = await encryptManifest(manifest, fileKeyB64);
    const back = decodeManifest(await decryptManifest(sealed, fileKeyB64));
    expect(back.chunks).toHaveLength(1);
    expect(back.size).toBe(100);
  });
});

describe("chunked upload round-trip", () => {
  beforeEach(() => {
    store.clear();
    cidSeq = 0;
  });

  it("uploadEncryptedFile chunks, fetchDecryptedFile reassembles with integrity check", async () => {
    const plain = new Uint8Array(100);
    for (let i = 0; i < plain.length; i++) plain[i] = i;
    const uploaded = await uploadEncryptedFile(plain, "big.bin", "application/octet-stream");
    expect(uploaded.chunked).toBe(true);
    expect(uploaded.fileSha256).toBe(await sha256Hex(plain));

    const back = await fetchDecryptedFile(uploaded.rootCid, uploaded.fileKey, true);
    expect(back).toEqual(plain);
  });

  it("single-blob path still works under threshold", async () => {
    const plain = new TextEncoder().encode("small");
    const uploaded = await uploadEncryptedFile(plain, "s.txt", "text/plain");
    expect(uploaded.chunked).toBe(false);
    const back = await fetchDecryptedFile(uploaded.rootCid, uploaded.fileKey, false);
    expect(new TextDecoder().decode(back)).toBe("small");
  });

  it("ephemeral upload skips local kubo pin", async () => {
    addOpts.clear();
    const plain = new TextEncoder().encode("ephemeral");
    await uploadEncryptedFile(plain, "e.txt", "text/plain", null, { ephemeral: true });
    for (const opts of addOpts.values()) {
      expect(opts.pin).toBe(false);
    }
  });
});
