import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  IpfsClient,
  ipfsUrls,
  isLikelyIpfsCid,
  normalizeCid,
  normalizeIpfsReference,
  sanitizeIpfsCid,
} from "@/ipfs/ipfsClient";
import { encryptFile, decryptFile } from "@/ipfs/fileCrypto";

describe("normalizeCid", () => {
  it("strips ipfs:// prefix", () => {
    expect(normalizeCid("ipfs://QmTest")).toBe("QmTest");
  });

  it("extracts cid from gateway url", () => {
    expect(normalizeIpfsReference("https://ipfs.io/ipfs/bafybeigdyrzt5sfp7udm7rm27znxt")).toBe(
      "bafybeigdyrzt5sfp7udm7rm27znxt",
    );
  });
});

describe("isLikelyIpfsCid", () => {
  it("accepts CIDv0 and CIDv1", () => {
    expect(isLikelyIpfsCid("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5")).toBe(true);
    expect(isLikelyIpfsCid("bafybeigdyrzt5sfp7udm7rm27znxt")).toBe(true);
  });

  it("rejects JPEG/binary garbage", () => {
    expect(isLikelyIpfsCid("\uFFFD\uFFFDFJFIF")).toBe(false);
    expect(sanitizeIpfsCid("not-a-cid")).toBeNull();
  });
});

describe("ipfsUrls", () => {
  it("builds primary and fallback gateway urls", () => {
    const urls = ipfsUrls("bafybeigdyrzt5sfp7udm7rm27znxt");
    expect(urls[0]).toContain("/ipfs/bafybeigdyrzt5sfp7udm7rm27znxt");
    expect(urls.some((u) => u.includes("nexusmall.net/nexchat/ipfs-gateway/ipfs/"))).toBe(true);
    expect(urls.some((u) => u.startsWith("https://ipfs.io/ipfs/"))).toBe(true);
  });

  it("does not double /ipfs when gateway already ends with /ipfs", () => {
    const urls = ipfsUrls("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5");
    expect(urls.every((u) => !u.includes("/ipfs/ipfs/"))).toBe(true);
  });
});

describe("IpfsClient", () => {
  const fetchMock = vi.fn();
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("add parses kubo NDJSON and returns Hash", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      text: async () => '{"Name":"f","Hash":"bafytest","Size":"12"}\n',
    });
    const c = new IpfsClient("http://api", "http://gw");
    const cid = await c.add(new Uint8Array([1, 2, 3]), "f.bin");
    expect(cid).toBe("bafytest");
    expect(fetchMock.mock.calls[0]![0]).toBe("http://api/add?pin=true&cid-version=1");
  });

  it("add respects pin=false for ephemeral / no-retention uploads", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      text: async () => '{"Name":"f","Hash":"bafytest","Size":"12"}\n',
    });
    const c = new IpfsClient("http://api", "http://gw");
    await c.add(new Uint8Array([1]), "f.bin", { pin: false });
    expect(fetchMock.mock.calls[0]![0]).toBe("http://api/add?pin=false&cid-version=1");
  });

  it("unpin calls kubo pin/rm", async () => {
    fetchMock.mockResolvedValueOnce({ ok: true, text: async () => "" });
    const c = new IpfsClient("http://api", "http://gw");
    await c.unpin("bafytest");
    expect(fetchMock.mock.calls[0]![0]).toBe("http://api/pin/rm?arg=bafytest");
  });

  it("unpin treats kubo 'not pinned' 500 as success", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 500,
      text: async () => '{"Message":"not pinned or pinned indirectly","Code":0,"Type":"error"}',
    });
    const c = new IpfsClient("http://api", "http://gw");
    await expect(c.unpin("bafygone")).resolves.toBeUndefined();
  });

  it("cat reads from gateway", async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      arrayBuffer: async () => new Uint8Array([9, 8, 7]).buffer,
    });
    const c = new IpfsClient("http://api", "http://gw");
    const bytes = await c.cat("QmX");
    expect(bytes).toEqual(new Uint8Array([9, 8, 7]));
    expect(fetchMock.mock.calls[0]![0]).toContain("/ipfs/QmX");
  });

  it("gatewayUrl returns first gateway url", () => {
    const c = new IpfsClient("http://api", "http://gw");
    expect(c.gatewayUrl("bafybeigdyrzt5sfp7udm7rm27znxt")).toBe(
      "http://gw/ipfs/bafybeigdyrzt5sfp7udm7rm27znxt",
    );
    expect(c.gatewayUrl("garbage")).toBeNull();
  });
});

describe("fileCrypto round-trip", () => {
  it("encrypt → decrypt recovers plaintext", async () => {
    const plain = new TextEncoder().encode("hello ipfs e2ee");
    const { ciphertext, fileKeyB64 } = await encryptFile(plain);
    const back = await decryptFile(ciphertext, fileKeyB64);
    expect(new TextDecoder().decode(back)).toBe("hello ipfs e2ee");
  });
});
