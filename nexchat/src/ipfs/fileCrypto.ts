// EN: Per-file AES-256-GCM helpers (CHAT_LARGE_FILE_SPEC.md §3–4).
// Non-chunked layout on IPFS: `iv(12) || ciphertext`.
// Chunked blocks: IV derived from `(file_key, chunk_nonce)`; IPFS stores ciphertext only.
// Manifest blobs use a fixed manifest nonce (`0xffffffff`). The `file_key` travels inside
// the MLS envelope (E2EE); IPFS only ever sees ciphertext.
// CN: 每文件 AES-256-GCM 辅助（大文件规范 §3–4）。非分块 IPFS 布局：`iv(12)||密文`。
// 分块：IV 由 `(file_key, chunk_nonce)` 派生；IPFS 只存密文。manifest 用固定 nonce
//（`0xffffffff`）。`file_key` 随 MLS 信封 E2EE 下发；IPFS 只见密文。

function b64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function b64dec(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/// EN: Encrypt a whole file; returns packed ciphertext + base64 raw AES key for the envelope.
/// CN: 整文件加密；返回打包密文 + 供信封使用的 base64 原始 AES 密钥。
export async function encryptFile(
  plain: Uint8Array,
): Promise<{ ciphertext: Uint8Array; fileKeyB64: string }> {
  const key = await generateFileKey();
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await aesEncrypt(key, iv, plain);
  const packed = new Uint8Array(12 + ct.byteLength);
  packed.set(iv);
  packed.set(ct, 12);
  return { ciphertext: packed, fileKeyB64: await exportFileKeyB64(key) };
}

const MANIFEST_NONCE = 0xffffffff;

/// EN: Import the per-file AES key from envelope `file_key` (base64 raw 32 bytes).
/// CN: 从信封 `file_key`（base64 原始 32 字节）导入 per-file AES 密钥。
export async function importFileKey(fileKeyB64: string): Promise<CryptoKey> {
  return crypto.subtle.importKey(
    "raw",
    b64dec(fileKeyB64) as BufferSource,
    "AES-GCM",
    false,
    ["encrypt", "decrypt"],
  );
}

/// EN: Derive a 12-byte GCM IV from `(file_key, nonce)` (chunk index or manifest sentinel).
/// CN: 从 `(file_key, nonce)` 派生 12 字节 GCM IV（块序号或 manifest 哨兵）。
export async function deriveIv(fileKeyB64: string, nonce: number): Promise<Uint8Array> {
  const keyBytes = b64dec(fileKeyB64);
  const seed = new Uint8Array(keyBytes.length + 4);
  seed.set(keyBytes);
  new DataView(seed.buffer).setUint32(keyBytes.length, nonce, true);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", seed));
  return digest.slice(0, 12);
}

async function aesEncrypt(key: CryptoKey, iv: Uint8Array, plain: Uint8Array): Promise<Uint8Array> {
  const ct = await crypto.subtle.encrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    plain as BufferSource,
  );
  return new Uint8Array(ct);
}

async function aesDecrypt(key: CryptoKey, iv: Uint8Array, ct: Uint8Array): Promise<Uint8Array> {
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: iv as BufferSource },
    key,
    ct as BufferSource,
  );
  return new Uint8Array(pt);
}

/// EN: Encrypt one chunk (ciphertext only — IV is derivable). CN: 加密一块（仅密文，IV 可派生）。
export async function encryptChunk(
  plain: Uint8Array,
  fileKeyB64: string,
  nonce: number,
): Promise<Uint8Array> {
  const key = await importFileKey(fileKeyB64);
  const iv = await deriveIv(fileKeyB64, nonce);
  return aesEncrypt(key, iv, plain);
}

/// EN: Decrypt one chunk fetched from IPFS. CN: 解密从 IPFS 取回的一块。
export async function decryptChunk(
  ciphertext: Uint8Array,
  fileKeyB64: string,
  nonce: number,
): Promise<Uint8Array> {
  const key = await importFileKey(fileKeyB64);
  const iv = await deriveIv(fileKeyB64, nonce);
  return aesDecrypt(key, iv, ciphertext);
}

/// EN: Encrypt the manifest JSON blob (fixed manifest nonce). CN: 加密 manifest JSON（固定 manifest nonce）。
export async function encryptManifest(plain: Uint8Array, fileKeyB64: string): Promise<Uint8Array> {
  return encryptChunk(plain, fileKeyB64, MANIFEST_NONCE);
}

/// EN: Decrypt a manifest blob from IPFS. CN: 解密 IPFS 上的 manifest。
export async function decryptManifest(sealed: Uint8Array, fileKeyB64: string): Promise<Uint8Array> {
  return decryptChunk(sealed, fileKeyB64, MANIFEST_NONCE);
}

/// EN: Decrypt a non-chunked blob (`iv||ct` layout). CN: 解密非分块 blob（`iv||ct` 布局）。
export async function decryptFile(ciphertext: Uint8Array, fileKeyB64: string): Promise<Uint8Array> {
  const key = await importFileKey(fileKeyB64);
  const iv = ciphertext.slice(0, 12);
  const ct = ciphertext.slice(12);
  return aesDecrypt(key, iv, ct);
}

/// EN: Export the raw file key as base64 (for the MLS envelope). CN: 导出原始文件密钥为 base64（供 MLS 信封）。
export async function exportFileKeyB64(key: CryptoKey): Promise<string> {
  return b64(new Uint8Array(await crypto.subtle.exportKey("raw", key)));
}

/// EN: Generate a fresh per-file AES-256-GCM key. CN: 生成新的 per-file AES-256-GCM 密钥。
export async function generateFileKey(): Promise<CryptoKey> {
  return crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, [
    "encrypt",
    "decrypt",
  ]);
}
