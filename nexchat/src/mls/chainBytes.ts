// EN: Hex helpers for polkadot.js chatGroup extrinsics. CN: chatGroup extrinsic 用 hex 编解码。

export function hex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

export function hexUtf8(str: string): string {
  return hex(new TextEncoder().encode(str));
}

export function hexToBytes(hexStr: string): Uint8Array {
  const s = hexStr.startsWith("0x") ? hexStr.slice(2) : hexStr;
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/// EN: Decode on-chain text stored as UTF-8 or legacy hex-encoded UTF-8.
/// CN: 解码链上文本（UTF-8 或历史 hex 编码 UTF-8）。
export function decodeChainText(raw: unknown): string {
  if (raw == null) return "";
  const o = raw as { isNone?: boolean; isSome?: boolean; Some?: unknown; unwrap?: () => unknown };
  if (o.isNone) return "";
  if (o.isSome && typeof o.unwrap === "function") return decodeChainText(o.unwrap());
  if (o.Some !== undefined) return decodeChainText(o.Some);
  const v = raw as { toUtf8?: () => string; toString?: () => string };
  if (typeof v.toUtf8 === "function") {
    const s = v.toUtf8();
    if (s.length > 0) return s;
  }
  const str = typeof v.toString === "function" ? v.toString() : String(raw);
  return decodeHexUtf8String(str);
}

type ChainOption = {
  isNone?: boolean;
  isSome?: boolean;
  Some?: unknown;
  unwrap?: () => unknown;
  toJSON?: () => unknown;
};

/// EN: Unwrap polkadot `Option` / runtime JSON into plain data (safe when `unwrap` is not a fn).
/// CN: 安全解包 polkadot `Option`（兼容 runtime 返回的 JSON 形态）。
export function unwrapChainJson<T extends Record<string, unknown> = Record<string, unknown>>(
  raw: unknown,
): T | null {
  if (raw == null) return null;
  const o = raw as ChainOption;
  if (o.isNone) return null;
  if (typeof o.unwrap === "function") {
    const inner = o.unwrap();
    if (inner != null && typeof (inner as ChainOption).toJSON === "function") {
      return (inner as ChainOption).toJSON!() as T;
    }
    return inner as T;
  }
  if (o.Some !== undefined) {
    const some = o.Some;
    if (some != null && typeof (some as ChainOption).toJSON === "function") {
      return (some as ChainOption).toJSON!() as T;
    }
    return some as T;
  }
  if (typeof o.toJSON === "function") return o.toJSON() as T;
  return typeof raw === "object" ? (raw as T) : null;
}

/// EN: Decode display labels that may be plain or `0x` hex UTF-8. CN: 解码可能是 hex 的展示文本。
export function decodeHexUtf8String(str: string): string {
  if (/^0x[0-9a-fA-F]+$/.test(str) && str.length > 4 && str.length % 2 === 0) {
    try {
      return new TextDecoder().decode(hexToBytes(str));
    } catch {
      /* fall through */
    }
  }
  return str;
}
