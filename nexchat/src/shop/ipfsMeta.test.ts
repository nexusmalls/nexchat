import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { extractImageCidFromBytes, loadProductImageBlob } from "@/shop/ipfsMeta";
import { ipfsUrls } from "@/ipfs/ipfsClient";

describe("extractImageCidFromBytes", () => {
  it("reads first image from JSON manifest", () => {
    const bytes = new TextEncoder().encode('{"images":["QmImg1234567890123456789012345678901234567890"]}');
    expect(extractImageCidFromBytes(bytes, "application/json")).toBe(
      "QmImg1234567890123456789012345678901234567890",
    );
  });
});

describe("loadProductImageBlob", () => {
  const fetchMock = vi.fn();
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:product-image"),
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("creates blob URL from gateway JPEG bytes", async () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x01]);
    fetchMock.mockResolvedValueOnce({
      ok: true,
      headers: { get: () => "image/jpeg" },
      arrayBuffer: async () => jpeg.buffer,
    });
    const url = await loadProductImageBlob("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5");
    expect(url).toBe("blob:product-image");
    expect(fetchMock.mock.calls[0]?.[0]).toContain("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5");
  });

  it("follows nested image CID in JSON manifest", async () => {
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        headers: { get: () => "application/json" },
        arrayBuffer: async () =>
          new TextEncoder().encode('{"images":["bafybeigdyrzt5sfp7udm7rm27znxt"]}').buffer,
      })
      .mockResolvedValueOnce({
        ok: true,
        headers: { get: () => "image/png" },
        arrayBuffer: async () => new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).buffer,
      });

    const url = await loadProductImageBlob("bafybeigdyrzt5sfp7udm7rm27znxt");
    expect(url).toBe("blob:product-image");
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});

describe("hasProductThumbnail", () => {
  const fetchMock = vi.fn();
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:product-image"),
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns false for placeholder non-IPFS ids", async () => {
    const { hasProductThumbnail } = await import("@/shop/ipfsMeta");
    expect(await hasProductThumbnail("img-Plan5-50USDT")).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("returns true when gateway returns JPEG", async () => {
    const { hasProductThumbnail } = await import("@/shop/ipfsMeta");
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x01]);
    fetchMock.mockResolvedValueOnce({
      ok: true,
      headers: { get: () => "image/jpeg" },
      arrayBuffer: async () => jpeg.buffer,
    });
    expect(await hasProductThumbnail("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5")).toBe(true);
  });
});

describe("filterProductsWithThumbnail", () => {
  const fetchMock = vi.fn();
  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn(() => "blob:product-image"),
    });
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("drops products without loadable thumbnails", async () => {
    const { filterProductsWithThumbnail } = await import("@/shop/ipfsMeta");
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x01]);
    fetchMock.mockResolvedValueOnce({
      ok: true,
      headers: { get: () => "image/jpeg" },
      arrayBuffer: async () => jpeg.buffer,
    });
    const products = [
      { id: 1, imagesCid: "img-bad" },
      { id: 2, imagesCid: "QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5" },
    ];
    const filtered = await filterProductsWithThumbnail(products);
    expect(filtered.map((p) => p.id)).toEqual([2]);
  });
});

describe("resolveProductImageUrls", () => {
  it("builds gateway urls for hex-encoded chain cid", async () => {
    const { resolveProductImageUrls } = await import("@/shop/ipfsMeta");
    const hex =
      "0x516d55567353574d36737275663273637742546a356f383636514546686f506647467364767a536d6633666e5735";
    const urls = resolveProductImageUrls(hex);
    expect(urls[0]).toContain("QmUVsSWM6sruf2scwBTj5o866QEFhoPfGFsdvzSmf3fnW5");
    expect(ipfsUrls(hex)[0]).toBe(urls[0]);
  });
});
