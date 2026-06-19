// EN: Minimal IPFS client — upload via kubo HTTP API (`/api/v0/add`) and read via the
// gateway (`/ipfs/{cid}`). Reference normalization aligned with nexus-com-dapp.
// CN: 最小 IPFS 客户端——经 kubo API 上传、经网关读取；引用规范化与 nexus-com-dapp 一致。

import { config } from "@/config";
import { fetchWithTimeout } from "@/util/fetchTimeout";
import { hexToBytes } from "@/mls/chainBytes";

const NULLISH_IPFS = new Set(["", "null", "none", "undefined"]);
const IPFS_CID_PREFIXES = ["bafy", "bafk", "Qm"];
const CID_V0_PATTERN = /^Qm[1-9A-HJ-NP-Za-km-z]{44}$/;
const CID_V1_PATTERN = /^(baf[1-9A-HJ-NP-Za-km-z]+|k51[1-9A-HJ-NP-Za-km-z]+)$/i;

function stripLeadingIpfsPath(path: string): string {
  return path
    .replace(/^\/+/, "")
    .replace(/^ipfs\/+?/i, "")
    .trim();
}

function looksLikeIpfsPath(value: string): boolean {
  const base = value.split(/[/?#]/, 1)[0] ?? "";
  return IPFS_CID_PREFIXES.some((prefix) => base.startsWith(prefix));
}

/// EN: Normalize raw chain / UI IPFS references (aligned with nexus-com-dapp).
/// CN: 规范化链上/UI 的 IPFS 引用（与 nexus-com-dapp 一致）。
export function normalizeIpfsReference(value: string | null | undefined): string | null {
  if (!value) return null;
  const trimmed = value.trim();
  if (!trimmed || NULLISH_IPFS.has(trimmed.toLowerCase())) return null;

  if (trimmed.startsWith("data:") || trimmed.startsWith("blob:")) return trimmed;

  if (/^ipfs:\/\//i.test(trimmed)) {
    const normalizedPath = stripLeadingIpfsPath(trimmed.replace(/^ipfs:\/\//i, ""));
    return normalizedPath || null;
  }

  if (/^https?:\/\//i.test(trimmed)) {
    try {
      const url = new URL(trimmed);
      const ipfsIndex = url.pathname.toLowerCase().indexOf("/ipfs/");
      if (ipfsIndex >= 0) {
        const ipfsPath = url.pathname.slice(ipfsIndex + "/ipfs/".length);
        const normalizedPath = stripLeadingIpfsPath(ipfsPath);
        if (!normalizedPath) return null;
        return `${normalizedPath}${url.search}${url.hash}`;
      }
      return trimmed;
    } catch {
      return trimmed;
    }
  }

  let raw = trimmed;
  if (raw.startsWith("0x") && raw.length > 2) {
    try {
      raw = new TextDecoder().decode(hexToBytes(raw)).trim();
    } catch {
      /* keep hex */
    }
  }

  const normalizedPath = stripLeadingIpfsPath(raw);
  if (!normalizedPath) return null;
  if (!looksLikeIpfsPath(normalizedPath)) return null;
  return normalizedPath;
}

/// EN: Legacy alias — prefer normalizeIpfsReference. CN: 兼容别名。
export function normalizeCid(cid: string): string {
  return normalizeIpfsReference(cid) ?? cid.trim();
}

/// EN: Strict CID check (nexus-com-dapp compatible). CN: 严格 CID 校验。
export function isLikelyIpfsCid(raw: string): boolean {
  if (!raw) return false;
  const trimmed = raw.trim().split(/[/?#]/, 1)[0] ?? "";
  if (!trimmed || /\s/.test(trimmed) || /[^\x20-\x7E]/.test(trimmed)) return false;
  if (/JFIF|Exif|Adobe|xmpmeta/i.test(trimmed)) return false;
  return CID_V0_PATTERN.test(trimmed) || CID_V1_PATTERN.test(trimmed);
}

export function sanitizeIpfsCid(raw: string | null | undefined): string | null {
  const ref = normalizeIpfsReference(raw);
  if (!ref) return null;
  if (ref.startsWith("data:") || ref.startsWith("blob:") || /^https?:\/\//i.test(ref)) {
    return ref;
  }
  const base = ref.split(/[/?#]/, 1)[0]?.trim() ?? "";
  return isLikelyIpfsCid(base) ? base : looksLikeIpfsPath(base) ? base : null;
}

const FALLBACK_GATEWAYS = [
  "https://nexusmall.net/nexchat/ipfs-gateway/ipfs",
  "https://ipfs.io/ipfs",
];

/// EN: Ordered gateway base paths (dapp-style: base may already end with `/ipfs`).
/// CN: 有序网关 base（与 dapp 一致：base 可能已含 `/ipfs`）。
export function ipfsGatewayBases(): string[] {
  const configured = config.ipfsGatewayUrl.replace(/\/$/, "");
  const bases = new Set<string>();

  const addBase = (base: string) => {
    const trimmed = base.replace(/\/$/, "");
    if (!trimmed) return;
    bases.add(trimmed);
    if (trimmed.startsWith("/") && typeof window !== "undefined") {
      bases.add(`${window.location.origin}${trimmed}`);
    }
  };

  if (configured.endsWith("/ipfs")) {
    addBase(configured);
  } else if (configured) {
    addBase(`${configured}/ipfs`);
  }

  for (const fb of FALLBACK_GATEWAYS) {
    addBase(fb);
  }

  return [...bases];
}

/// EN: Ordered gateway URLs for embedding (primary proxy + public fallbacks).
/// CN: 嵌入用网关 URL 列表（主代理 + 公共 fallback）。
export function ipfsUrls(cid: string | null | undefined): string[] {
  const normalized = normalizeIpfsReference(cid);
  if (!normalized) return [];

  if (
    normalized.startsWith("data:") ||
    normalized.startsWith("blob:") ||
    /^https?:\/\//i.test(normalized)
  ) {
    return [normalized];
  }

  return Array.from(new Set(ipfsGatewayBases().map((base) => `${base}/${normalized}`)));
}

/// EN: Fetch bytes from gateway list with timeout + failover (nexus-com-dapp text-fetcher style).
/// CN: 带超时与 failover 的网关字节拉取（对齐 dapp text-fetcher）。
export async function fetchIpfsBytes(
  urls: readonly string[],
  timeoutMs = 12_000,
): Promise<{ bytes: Uint8Array; mime: string; url: string } | null> {
  for (const url of urls) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const res = await fetch(url, { signal: controller.signal, cache: "force-cache" });
      if (!res.ok) continue;
      const bytes = new Uint8Array(await res.arrayBuffer());
      if (bytes.length === 0) continue;
      return {
        bytes,
        mime: res.headers.get("content-type") ?? "application/octet-stream",
        url,
      };
    } catch {
      /* try next gateway */
    } finally {
      clearTimeout(timer);
    }
  }
  return null;
}

function gatewayBaseForClient(gatewayBase: string): string {
  const trimmed = gatewayBase.replace(/\/$/, "");
  return trimmed.endsWith("/ipfs") ? trimmed : `${trimmed}/ipfs`;
}

export function ipfsUrl(cid: string | null | undefined): string | null {
  return ipfsUrls(cid)[0] ?? null;
}

export class IpfsClient {
  constructor(
    private apiBase = config.ipfsApiUrl,
    private gatewayBase = config.ipfsGatewayUrl,
  ) {}

  /// EN: Build a gateway URL for embedding (images, download links). CN: 构造可嵌入的网关 URL。
  gatewayUrl(cid: string): string | null {
    const normalized = normalizeIpfsReference(cid);
    if (!normalized) return null;
    if (
      normalized.startsWith("data:") ||
      normalized.startsWith("blob:") ||
      /^https?:\/\//i.test(normalized)
    ) {
      return normalized;
    }
    const base = gatewayBaseForClient(this.gatewayBase);
    return `${base}/${normalized}`;
  }

  /// EN: Quick liveness probe (kubo `/api/v0/version`). CN: 存活探测（kubo `/api/v0/version`）。
  async ping(): Promise<boolean> {
    try {
      const res = await fetch(`${this.apiBase}/version`, { method: "POST" });
      return res.ok;
    } catch {
      return false;
    }
  }

  /// EN: `ipfs add` — returns the root CID (v0/v1 per kubo). `pin` controls local kubo pin
  /// only (sync blobs default true; chat media may pass false for ephemeral / no-retention).
  /// CN: `ipfs add` 返回根 CID。`pin` 仅控制本机 kubo pin（sync blob 默认 true；聊天媒体
  /// ephemeral / 不保留时可传 false）。
  async add(data: Uint8Array, name = "blob", options?: { pin?: boolean }): Promise<string> {
    const pin = options?.pin !== false;
    const form = new FormData();
    form.append("file", new Blob([data as BlobPart]), name);
    const res = await fetch(`${this.apiBase}/add?pin=${pin ? "true" : "false"}&cid-version=1`, {
      method: "POST",
      body: form,
    });
    if (!res.ok) throw new Error(`IPFS add failed: HTTP ${res.status}`);
    const text = await res.text();
    const lines = text.trim().split("\n").filter((l) => l.length > 0);
    if (lines.length === 0) throw new Error("IPFS add: empty response");
    const last = JSON.parse(lines[lines.length - 1]!) as { Hash?: string };
    if (!last.Hash) throw new Error("IPFS add: no Hash in response");
    return last.Hash;
  }

  /// EN: Fetch raw bytes for a CID (gateway first; falls back to kubo `cat`). Each attempt
  /// is bounded so cloud restore cannot hang forever on a dead gateway/API.
  /// CN: 按 CID 取原始字节（先网关后 kubo `cat`）；每次尝试有超时，避免云恢复在死网关/API 上无限挂起。
  async cat(cid: string, timeoutMs = 18_000): Promise<Uint8Array> {
    const normalized = normalizeIpfsReference(cid) ?? normalizeCid(cid);
    const arg = encodeURIComponent(normalized);
    const gatewayUrls = ipfsUrls(normalized);
    for (const url of gatewayUrls) {
      try {
        const res = await fetchWithTimeout(url, undefined, timeoutMs, "IPFS gateway");
        if (res.ok) return new Uint8Array(await res.arrayBuffer());
      } catch {
        /* next */
      }
    }
    let res = await fetchWithTimeout(
      `${gatewayBaseForClient(this.gatewayBase)}/${arg}`,
      undefined,
      timeoutMs,
      "IPFS gateway",
    );
    if (!res.ok) {
      res = await fetchWithTimeout(
        `${this.apiBase}/cat?arg=${arg}`,
        { method: "POST" },
        timeoutMs,
        "IPFS cat",
      );
    }
    if (!res.ok) throw new Error(`IPFS cat failed: HTTP ${res.status}`);
    return new Uint8Array(await res.arrayBuffer());
  }

  /// EN: `ipfs pin rm` on the local kubo node (best-effort sender retention cleanup).
  /// CN: 在本机 kubo 执行 `ipfs pin rm`（发送方 retention 清扫，尽力而为）。
  async unpin(cid: string): Promise<void> {
    const normalized = normalizeIpfsReference(cid) ?? normalizeCid(cid);
    const arg = encodeURIComponent(normalized);
    const res = await fetch(`${this.apiBase}/pin/rm?arg=${arg}`, { method: "POST" });
    if (res.ok) return;
    // EN: kubo returns HTTP 500 "not pinned or pinned indirectly" when the pin is already
    // gone — treat as success (idempotent cleanup). CN: 该 pin 已不存在时 kubo 返回 500
    // "not pinned ..."，视为成功（清扫幂等）。
    const body = await res.text().catch(() => "");
    if (/not pinned/i.test(body)) return;
    throw new Error(`IPFS unpin failed: HTTP ${res.status}`);
  }
}

export const ipfsClient = new IpfsClient();
