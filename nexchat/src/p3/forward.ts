// EN: Forward helpers — copy body text + attach `forward` ref (CHAT_P3 §4.1).
// CN: 转发辅助——复制正文并附带 `forward` 引用（P3 §4.1）。

import type { FileBody } from "@/mls/envelope";
import type { MessageVM } from "@/types/viewModels";

/// EN: Whether the UI should offer forward for this message. CN: 是否允许转发该消息。
export function canForwardMessage(msg: MessageVM): boolean {
  if (msg.content.type === "system" || msg.content.type === "reaction") return false;
  if (msg.status === "recalled") return false;
  return true;
}

/// EN: Media message has IPFS refs ready to re-send without re-upload. CN: 媒体已就绪，可复用 CID 转发。
export function isMediaForwardReady(msg: MessageVM): boolean {
  if (msg.content.type !== "media") return false;
  const c = msg.content;
  return !!c.bodyReady && !!c.rootCid && !!c.fileKey;
}

/// EN: Build FileBody from a decrypted media message (reuse CIDs on forward).
/// CN: 由已解密媒体消息构造 FileBody（转发时复用 CID）。
export function fileBodyFromMessage(msg: MessageVM): FileBody | null {
  if (!isMediaForwardReady(msg) || msg.content.type !== "media") return null;
  const c = msg.content;
  return {
    rootCid: c.rootCid!,
    chunked: c.chunked ?? false,
    fileKey: c.fileKey!,
    mime: c.mime,
    size: c.size,
    name: c.name,
    thumbCid: c.thumbCid,
    thumbKey: c.thumbKey,
    durationMs: c.durationMs,
  };
}

/// EN: Plain-text copy of a message for forwarding (media → bracket summary).
/// CN: 用于转发的纯文本副本（媒体 → 括号摘要）。
export function forwardBodyText(msg: MessageVM): string {
  switch (msg.content.type) {
    case "text":
      return msg.content.text;
    case "media":
      return `[${msg.content.mime}${msg.content.name ? ` ${msg.content.name}` : ""}]`;
    case "reaction":
      return `${msg.content.emoji} → ${msg.content.target}`;
    case "system":
      return msg.content.kind;
  }
}

/// EN: Short preview for forward card UI. CN: 转发卡片 UI 用的短预览。
export function forwardPreview(msg: MessageVM): string {
  const body = forwardBodyText(msg);
  return body.length > 80 ? `${body.slice(0, 80)}…` : body;
}
