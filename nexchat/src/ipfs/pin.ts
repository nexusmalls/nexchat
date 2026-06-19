// EN: Optional on-chain IPFS Pin via `storageService.requestPinForSubject` (CHAT_LARGE_FILE_SPEC
// §6). **Opt-in only** (`VITE_IPFS_PIN_ENABLED=true`); default is no chain/global pin for chat
// media. Skipped for ephemeral messages. Local kubo retention is handled separately by
// `senderMediaRetention.ts`. Uses group id as `subject_id`; thumb → Standard, body/chunks → Temporary.
// CN: 可选链上 IPFS Pin（`storageService.requestPinForSubject`，大文件规范 §6）。**仅 opt-in**
//（`VITE_IPFS_PIN_ENABLED=true`）；聊天媒体默认不做链上/全局 pin。阅后即焚跳过。本机 kubo
// 保留由 `senderMediaRetention.ts` 单独处理。以群 id 为 `subject_id`；缩略图 Standard，正文/块 Temporary。

import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import type { UploadedEncryptedFile } from "@/ipfs/media";

export type PinTierName = "Critical" | "Standard" | "Temporary";

/// EN: Pin uploaded CIDs on-chain (best-effort; logs errors to console). CN: 链上 Pin 已上传 CID（尽力而为）。
export async function maybePinUploadedFile(
  uploaded: UploadedEncryptedFile,
  subjectId: number,
): Promise<void> {
  if (!config.ipfsPinEnabled || config.useMock) return;
  const targets: { cid: string; sizeBytes: number; tier: PinTierName }[] = [];
  if (uploaded.thumbCid) {
    targets.push({ cid: uploaded.thumbCid, sizeBytes: 32_768, tier: "Standard" });
  }
  targets.push({
    cid: uploaded.rootCid,
    sizeBytes: uploaded.chunked ? 4096 : uploaded.size,
    tier: "Temporary",
  });
  if (uploaded.chunkCids) {
    for (const ch of uploaded.chunkCids) {
      targets.push({ cid: ch.cid, sizeBytes: ch.sizeBytes, tier: "Temporary" });
    }
  }
  for (const t of targets) {
    try {
      await chainClient.requestPinForSubjectDev(subjectId, t.cid, t.sizeBytes, t.tier);
    } catch (e) {
      console.warn("[nexchat] requestPinForSubject failed:", t.cid, e);
    }
  }
}

/// EN: Explicit user "keep attachment" — chain-pin a received/sent media reference at
/// Temporary tier (thumb Standard). Forced action: ignores `VITE_IPFS_PIN_ENABLED`, still
/// requires LIVE mode. Chunk CIDs live inside the encrypted manifest, so only root + thumb
/// are pinned here; kubo pins the root DAG recursively on the operator side anyway.
/// CN: 用户显式「保留附件」——对媒体引用做链上 Temporary Pin（缩略图 Standard）。强制动作：
/// 不受 `VITE_IPFS_PIN_ENABLED` 限制，仅要求 LIVE 模式。分块 CID 在加密 manifest 内，此处
/// 只 pin 根 + 缩略图；运营侧 kubo 对根 DAG 递归 pin 即可覆盖。
export async function pinAttachmentOnChain(media: {
  rootCid: string;
  size: number;
  thumbCid?: string;
}, subjectId: number): Promise<void> {
  if (config.useMock) throw new Error("链上 Pin 需要 LIVE 模式");
  if (media.thumbCid) {
    await chainClient.requestPinForSubjectDev(subjectId, media.thumbCid, 32_768, "Standard");
  }
  await chainClient.requestPinForSubjectDev(
    subjectId,
    media.rootCid,
    Math.max(media.size, 1),
    "Temporary",
  );
}
