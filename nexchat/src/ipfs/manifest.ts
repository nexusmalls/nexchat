// EN: Encrypted chunked-file manifest (CHAT_LARGE_FILE_SPEC.md §4). The manifest JSON is
// encrypted with the same per-file key before `ipfs add`; `body.root_cid` points at it when
// `chunked=true`. CN: 分块文件 manifest（大文件规范 §4）。manifest JSON 用同一 per-file 密钥
// 加密后再 `ipfs add`；`chunked=true` 时 `body.root_cid` 指向它。

export interface ChunkEntry {
  cid: string;
  nonce: number;
  sha256: string;
}

export interface FileManifest {
  v: 1;
  size: number;
  chunkSize: number;
  mime: string;
  fileSha256: string;
  chunks: ChunkEntry[];
}

export function encodeManifest(m: FileManifest): Uint8Array {
  return new TextEncoder().encode(JSON.stringify(m));
}

export function decodeManifest(bytes: Uint8Array): FileManifest {
  const m = JSON.parse(new TextDecoder().decode(bytes)) as FileManifest;
  if (m.v !== 1) throw new Error(`unsupported manifest version: ${m.v}`);
  if (!Array.isArray(m.chunks) || m.chunks.length === 0) {
    throw new Error("manifest: empty chunks");
  }
  return m;
}
