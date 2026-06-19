import type { MessageVM } from "@/types/viewModels";

/// EN: Short plain-text preview for reply/forward bars. CN: 回复/转发条用的短预览。
export function contentPreviewFromMessage(m: MessageVM): string {
  switch (m.content.type) {
    case "text":
      return m.content.text;
    case "media":
      if (m.content.mime.startsWith("audio/")) return "[语音]";
      if (m.content.mime.startsWith("image/")) return "[图片]";
      if (m.content.mime.startsWith("video/")) return "[视频]";
      return `[${m.content.mime}${m.content.name ? ` ${m.content.name}` : ""}]`;
    case "reaction":
      return `${m.content.emoji} → ${m.content.target}`;
    case "system":
      return m.content.kind;
  }
}

/// EN: Reply-quote line when the target may be deleted or recalled. CN: 回复引用在目标已删/已撤回时的展示。
export function replyQuotePreview(
  replyToId: string | undefined,
  target: MessageVM | undefined,
): string | undefined {
  if (!replyToId) return undefined;
  if (!target) return "原消息已删除";
  if (target.status === "recalled") return "消息已撤回";
  return contentPreviewFromMessage(target);
}

/// EN: Whether the reply quote is a degraded placeholder (deleted/recalled target).
/// CN: 回复引用是否为降级占位（目标已删/已撤回）。
export function isDegradedReplyQuote(text: string): boolean {
  return text === "原消息已删除" || text === "消息已撤回";
}
