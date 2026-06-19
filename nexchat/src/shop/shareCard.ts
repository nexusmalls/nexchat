// EN: Product share deep-link encoding for MLS text messages.
// CN: 商品分享深链接编码（MLS 文本消息）。

const SHARE_RE = /^\[nexchat:product:(\d+):shop:(\d+)\](.*)$/s;

// EN: Encode product share payload into a single text line.
// CN: 将商品分享编码为单行文本。
export function encodeProductShare(
  productId: number,
  shopId: number,
  label?: string,
): string {
  const tag = `[nexchat:product:${productId}:shop:${shopId}]`;
  const name = label?.trim();
  return name ? `${tag}${name}` : tag;
}

export interface ProductSharePayload {
  productId: number;
  shopId: number;
  label: string;
}

// EN: Parse product share text; returns null if not a share message.
// CN: 解析商品分享文本；非分享消息返回 null。
export function parseProductShare(text: string): ProductSharePayload | null {
  const m = text.trim().match(SHARE_RE);
  if (!m) return null;
  return {
    productId: Number(m[1]),
    shopId: Number(m[2]),
    label: (m[3] ?? "").trim(),
  };
}
