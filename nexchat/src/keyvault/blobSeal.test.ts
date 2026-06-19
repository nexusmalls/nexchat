// EN: §5.0 versioned blob wire — v2 seal, legacy fallback parse, migration semantics.
// CN: §5.0 版本化 blob wire——v2 封装、旧格式回退解析、迁移语义。

import { describe, expect, it } from "vitest";
import { BLOB_WIRE_V2, openVersionedBlob, sealVersionedBlob } from "@/keyvault/blobSeal";

const enc = new TextEncoder();
const dec = new TextDecoder();

async function aesKey(seed: string): Promise<CryptoKey> {
  const raw = await crypto.subtle.digest("SHA-256", enc.encode(seed));
  return crypto.subtle.importKey("raw", raw, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

async function legacySeal(key: CryptoKey, pt: Uint8Array): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv: iv as BufferSource }, key, pt as BufferSource);
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  return out;
}

describe("versioned blob seal", () => {
  it("seals as 0x02||iv||ct and opens with the current key", async () => {
    const key = await aesKey("new-root");
    const packed = await sealVersionedBlob(key, enc.encode("hello"));
    expect(packed[0]).toBe(BLOB_WIRE_V2);
    const pt = await openVersionedBlob(packed, key, null);
    expect(dec.decode(pt)).toBe("hello");
  });

  it("falls back to the legacy key for unversioned wire", async () => {
    const newKey = await aesKey("new-root");
    const legacyKey = await aesKey("legacy-root");
    const packed = await legacySeal(legacyKey, enc.encode("old data"));
    const pt = await openVersionedBlob(packed, newKey, legacyKey);
    expect(dec.decode(pt)).toBe("old data");
  });

  it("handles a legacy blob whose iv happens to start with 0x02", async () => {
    const newKey = await aesKey("new-root");
    const legacyKey = await aesKey("legacy-root");
    let packed: Uint8Array;
    do {
      packed = await legacySeal(legacyKey, enc.encode("unlucky iv"));
    } while (packed[0] !== BLOB_WIRE_V2);
    const pt = await openVersionedBlob(packed, newKey, legacyKey);
    expect(dec.decode(pt)).toBe("unlucky iv");
  });

  it("throws when neither key authenticates", async () => {
    const a = await aesKey("a");
    const b = await aesKey("b");
    const c = await aesKey("c");
    const packed = await sealVersionedBlob(a, enc.encode("x"));
    await expect(openVersionedBlob(packed, b, c)).rejects.toThrow(/authenticate/);
  });
});
