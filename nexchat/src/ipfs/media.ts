// EN: Encrypt-then-upload for chat attachments — single-blob or chunked+manifest paths,
// optional encrypted thumbnails (thumb-first preview). CN: 聊天附件先加密再上传——单 blob 或
// 分块+manifest，可选加密缩略图（缩略图先行预览）。

import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import {
  decryptChunk,
  decryptFile,
  decryptManifest,
  encryptChunk,
  encryptFile,
  encryptManifest,
  exportFileKeyB64,
  generateFileKey,
} from "@/ipfs/fileCrypto";
import { decodeManifest, encodeManifest, type FileManifest } from "@/ipfs/manifest";
import type { FileBody } from "@/mls/envelope";

export interface UploadedEncryptedFile {
  rootCid: string;
  fileKey: string;
  mime: string;
  size: number;
  name?: string;
  chunked: boolean;
  fileSha256?: string;
  thumbCid?: string;
  thumbKey?: string;
  /** EN: per-chunk IPFS CIDs (chunked path only), for optional chain Pin. CN: 各块 IPFS CID（仅分块），供可选链上 Pin。 */
  chunkCids?: { cid: string; sizeBytes: number }[];
}

/// EN: SHA-256 hex digest of raw bytes. CN: 原始字节的 SHA-256 十六进制摘要。
export async function sha256Hex(data: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", data as BufferSource));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/// EN: Whether this attachment should use the chunked path. CN: 是否走分块路径。
export function shouldChunk(size: number, mime: string): boolean {
  if (size > config.ipfsChunkThreshold) return true;
  return mime.startsWith("video/");
}

export interface UploadEncryptedFileOptions {
  /** EN: skip local kubo pin (ephemeral / burn-after-read). CN: 跳过本机 kubo pin（阅后即焚）。 */
  ephemeral?: boolean;
}

function mediaAddPin(ephemeral?: boolean): boolean {
  if (ephemeral) return false;
  return config.ipfsMediaLocalPinEnabled;
}

/// EN: Encrypt `plain` and upload (auto single vs chunked); optional `thumbPlain` JPEG.
/// CN: 加密 `plain` 并上传（自动单 blob / 分块）；可选 `thumbPlain` JPEG 缩略图。
export async function uploadEncryptedFile(
  plain: Uint8Array,
  name: string,
  mime: string,
  thumbPlain?: Uint8Array | null,
  options?: UploadEncryptedFileOptions,
): Promise<UploadedEncryptedFile> {
  const fileSha256 = await sha256Hex(plain);
  const mimeType = mime || "application/octet-stream";
  const pin = mediaAddPin(options?.ephemeral);

  let uploaded: UploadedEncryptedFile;
  if (shouldChunk(plain.byteLength, mimeType)) {
    uploaded = await uploadChunked(plain, name, mimeType, fileSha256, pin);
  } else {
    const { ciphertext, fileKeyB64 } = await encryptFile(plain);
    const rootCid = await ipfsClient.add(ciphertext, name, { pin });
    uploaded = {
      rootCid,
      fileKey: fileKeyB64,
      mime: mimeType,
      size: plain.byteLength,
      name,
      chunked: false,
      fileSha256,
    };
  }

  if (thumbPlain && thumbPlain.length > 0) {
    const thumb = await encryptFile(thumbPlain);
    uploaded.thumbCid = await ipfsClient.add(thumb.ciphertext, `${name}.thumb.jpg`, { pin });
    uploaded.thumbKey = thumb.fileKeyB64;
  }

  return uploaded;
}

async function uploadChunked(
  plain: Uint8Array,
  name: string,
  mime: string,
  fileSha256: string,
  pin: boolean,
): Promise<UploadedEncryptedFile> {
  const key = await generateFileKey();
  const fileKeyB64 = await exportFileKeyB64(key);
  const chunkSize = config.ipfsChunkSize;
  const chunks: FileManifest["chunks"] = [];
  const chunkCids: { cid: string; sizeBytes: number }[] = [];

  for (let nonce = 0, off = 0; off < plain.byteLength; nonce++) {
    const slice = plain.subarray(off, off + chunkSize);
    off += slice.byteLength;
    const ct = await encryptChunk(slice, fileKeyB64, nonce);
    const cid = await ipfsClient.add(ct, `${name}.part${nonce}`, { pin });
    chunks.push({ cid, nonce, sha256: await sha256Hex(ct) });
    chunkCids.push({ cid, sizeBytes: slice.byteLength });
  }

  const manifest: FileManifest = {
    v: 1,
    size: plain.byteLength,
    chunkSize,
    mime,
    fileSha256,
    chunks,
  };
  const sealedManifest = await encryptManifest(encodeManifest(manifest), fileKeyB64);
  const rootCid = await ipfsClient.add(sealedManifest, `${name}.manifest`, { pin });

  return {
    rootCid,
    fileKey: fileKeyB64,
    mime,
    size: plain.byteLength,
    name,
    chunked: true,
    fileSha256,
    chunkCids,
  };
}

/// EN: Fetch and decrypt a file (single blob or chunked manifest). CN: 取回并解密文件（单 blob 或分块 manifest）。
export async function fetchDecryptedFile(
  rootCid: string,
  fileKey: string,
  chunked = false,
): Promise<Uint8Array> {
  const sealed = await ipfsClient.cat(rootCid);
  if (!chunked) return decryptFile(sealed, fileKey);

  const manifest = decodeManifest(await decryptManifest(sealed, fileKey));
  const parts: Uint8Array[] = [];
  for (const ch of manifest.chunks) {
    const ct = await ipfsClient.cat(ch.cid);
    parts.push(await decryptChunk(ct, fileKey, ch.nonce));
  }
  const out = new Uint8Array(manifest.size);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.byteLength;
  }
  if (manifest.fileSha256) {
    const got = await sha256Hex(out);
    if (got !== manifest.fileSha256) throw new Error("file integrity check failed");
  }
  return out;
}

/// EN: Decrypt a thumbnail blob (non-chunked `iv||ct` layout). CN: 解密缩略图（非分块 `iv||ct` 布局）。
export async function fetchDecryptedThumb(thumbCid: string, thumbKey: string): Promise<Uint8Array> {
  return decryptFile(await ipfsClient.cat(thumbCid), thumbKey);
}

/// EN: Map MIME to P3 envelope `type`. CN: MIME → P3 信封 `type`。
export function envelopeTypeForMime(mime: string): "image" | "video" | "audio" | "file" {
  if (mime.startsWith("image/")) return "image";
  if (mime.startsWith("video/")) return "video";
  if (mime.startsWith("audio/")) return "audio";
  return "file";
}

/// EN: Build a `FileBody` from upload result. CN: 由上传结果构造 `FileBody`。
export function fileBodyFromUpload(u: UploadedEncryptedFile): FileBody {
  return {
    rootCid: u.rootCid,
    chunked: u.chunked,
    fileKey: u.fileKey,
    mime: u.mime,
    size: u.size,
    name: u.name,
    fileSha256: u.fileSha256,
    thumbCid: u.thumbCid,
    thumbKey: u.thumbKey,
  };
}
