// EN: IPFS text resolution for shop product metadata (names/descriptions).
// CN: 商铺商品元数据 IPFS 文本解析（名称/详情）。

import { config } from "@/config";
import {
  fetchIpfsBytes,
  ipfsClient,
  ipfsUrls,
  normalizeCid,
  normalizeIpfsReference,
} from "@/ipfs/ipfsClient";

const textCache = new Map<string, string>();

function decodeText(bytes: Uint8Array): string {
  const raw = new TextDecoder().decode(bytes).trim();
  if (!raw) return "";
  try {
    const json = JSON.parse(raw) as Record<string, unknown>;
    if (typeof json.name === "string") return json.name;
    if (typeof json.title === "string") return json.title;
    if (typeof json.text === "string") return json.text;
    if (Array.isArray(json.images)) {
      return json.images.filter((x): x is string => typeof x === "string").join(",");
    }
  } catch {
    /* plain text */
  }
  return raw;
}

/// EN: Resolve display text from CID (cached).
/// CN: 从 CID 解析展示文本（带缓存）。
export async function fetchIpfsText(cid: string): Promise<string> {
  const key = normalizeCid(cid);
  if (!key) return "";
  const hit = textCache.get(key);
  if (hit != null) return hit;
  if (!config.ipfsEnabled) return "";
  try {
    const bytes = await ipfsClient.cat(key);
    const text = decodeText(bytes);
    textCache.set(key, text);
    return text;
  } catch {
    textCache.set(key, "");
    return "";
  }
}

/// EN: Batch-resolve CIDs to text map.
/// CN: 批量解析 CID 文本。
export async function fetchIpfsTextBatch(cids: string[]): Promise<Map<string, string>> {
  const unique = [...new Set(cids.map(normalizeCid).filter((c) => c.length > 0))];
  const map = new Map<string, string>();
  await Promise.all(
    unique.map(async (cid) => {
      map.set(cid, await fetchIpfsText(cid));
    }),
  );
  return map;
}

/// EN: Extract nested image CID when images_cid points at JSON metadata. CN: 从 JSON 元数据提取图片 CID。
export function extractImageCidFromBytes(bytes: Uint8Array, mime: string): string | null {
  const head = new TextDecoder().decode(bytes.slice(0, Math.min(bytes.length, 256))).trim();
  if (!mime.includes("json") && !head.startsWith("{") && !head.startsWith("[")) return null;
  try {
    const json = JSON.parse(new TextDecoder().decode(bytes)) as Record<string, unknown>;
    if (typeof json.image === "string" && json.image.trim()) return json.image.trim();
    if (typeof json.cid === "string" && json.cid.trim()) return json.cid.trim();
    if (Array.isArray(json.images)) {
      const first = json.images.find((x) => typeof x === "string" && x.trim());
      if (typeof first === "string") return first.trim();
    }
  } catch {
    return null;
  }
  return null;
}

function guessImageMime(bytes: Uint8Array): string {
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 &&
    bytes[1] === 0x50 &&
    bytes[2] === 0x4e &&
    bytes[3] === 0x47
  ) {
    return "image/png";
  }
  if (bytes.length >= 6 && bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) {
    return "image/gif";
  }
  if (bytes.length >= 12 && bytes[4] === 0x66 && bytes[5] === 0x74 && bytes[6] === 0x79 && bytes[7] === 0x70) {
    return "image/webp";
  }
  return "image/jpeg";
}

const productImageBlobCache = new Map<string, string>();

/// EN: Load product image via fetch + blob URL (reliable on mobile / proxied gateways).
/// CN: fetch + blob URL 加载商品图（移动端 / 代理网关更稳）。
export async function loadProductImageBlob(
  cid: string | null | undefined,
): Promise<string | null> {
  const normalized = normalizeIpfsReference(cid);
  if (!normalized) return null;
  if (normalized.startsWith("data:") || normalized.startsWith("blob:") || /^https?:\/\//i.test(normalized)) {
    return normalized;
  }

  const cached = productImageBlobCache.get(normalized);
  if (cached) return cached;

  let urls = ipfsUrls(normalized);
  for (let pass = 0; pass < 2 && urls.length > 0; pass++) {
    const fetched = await fetchIpfsBytes(urls);
    if (!fetched) break;

    const nested = pass === 0 ? extractImageCidFromBytes(fetched.bytes, fetched.mime) : null;
    if (nested) {
      urls = ipfsUrls(nested);
      continue;
    }

    let mime = fetched.mime.split(";")[0]?.trim() ?? "application/octet-stream";
    if (!mime.startsWith("image/")) {
      mime = guessImageMime(fetched.bytes);
    }
    if (!mime.startsWith("image/")) return null;

    const blobUrl = URL.createObjectURL(new Blob([fetched.bytes as BlobPart], { type: mime }));
    productImageBlobCache.set(normalized, blobUrl);
    return blobUrl;
  }

  return null;
}

/// EN: images_cid → gateway URLs (direct image CID). CN: images_cid → 网关 URL。
export function resolveProductImageUrls(imagesCid: string): string[] {
  return ipfsUrls(imagesCid);
}

const thumbnailProbeCache = new Map<string, boolean>();

/// EN: Whether images_cid looks like a resolvable IPFS / URL reference (sync pre-check).
/// CN: images_cid 是否像可解析的 IPFS / URL 引用（同步预检）。
export function hasThumbnailReference(cid: string | null | undefined): boolean {
  return normalizeIpfsReference(cid) != null;
}

/// EN: Probe whether product thumbnail can be loaded (cached per normalized cid).
/// CN: 探测商品缩略图是否可加载（按规范化 cid 缓存）。
export async function hasProductThumbnail(cid: string | null | undefined): Promise<boolean> {
  const normalized = normalizeIpfsReference(cid);
  if (!normalized) return false;
  const hit = thumbnailProbeCache.get(normalized);
  if (hit != null) return hit;
  const blob = await loadProductImageBlob(cid);
  const ok = blob != null;
  thumbnailProbeCache.set(normalized, ok);
  return ok;
}

/// EN: Keep only products whose thumbnail loads successfully; preserves input order.
/// CN: 仅保留缩略图加载成功的商品；保持原排序。
export async function filterProductsWithThumbnail<T extends { imagesCid: string }>(
  products: T[],
): Promise<T[]> {
  if (products.length === 0) return [];
  const flags = await Promise.all(
    products.map(async (p) => hasProductThumbnail(p.imagesCid)),
  );
  return products.filter((_, i) => flags[i]);
}
