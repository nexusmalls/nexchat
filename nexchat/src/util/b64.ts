// EN: Browser-safe base64 helpers (shared by relay frames, MLS wire payloads, delivery tokens).
// CN: 浏览器安全的 base64 工具（relay 帧、MLS wire 载荷、投递令牌共用）。

export function bytesToB64(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

export function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
