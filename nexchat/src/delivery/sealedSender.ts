// EN: Sealed-sender blob — sender SS58 encrypted for the receiver (AES-GCM key = SHA-256 of the
// pairwise MLS conv key). Relay also receives that MLS key in plaintext on the frame for unseal on
// the receiver side; combined with the authenticated WS session this does NOT hide the sender from
// the relay operator. CN: Sealed-sender 封装——发送方 SS58 经 AES-GCM 加密（密钥 = pairwise MLS 会话键的
// SHA-256）。relay 在帧上亦以明文收到该 MLS 键供接收方解封；再叠加已认证 WS 会话，**无法**对 relay 运营方隐藏发送方。

import { bytesToB64, b64ToBytes } from "@/delivery/b64";

const enc = new TextEncoder();
const dec = new TextDecoder();

async function deriveKey(mlsConvKey: string): Promise<CryptoKey> {
  const raw = await crypto.subtle.digest("SHA-256", enc.encode(`sealed-sender/v1:${mlsConvKey}`));
  return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]);
}

export async function sealSender(sender: string, mlsConvKey: string): Promise<string> {
  const key = await deriveKey(mlsConvKey);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    enc.encode(sender),
  );
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  return bytesToB64(out);
}

export async function unsealSender(blobB64: string, mlsConvKey: string): Promise<string> {
  const packed = b64ToBytes(blobB64);
  const iv = packed.slice(0, 12);
  const ct = packed.slice(12);
  const key = await deriveKey(mlsConvKey);
  const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv: iv as BufferSource }, key, ct);
  return dec.decode(pt);
}
