// EN: Two-sided recall helpers (CHAT_P3 §4.x). Recall is a control envelope the SENDER emits
// to hide a message it already sent, for BOTH sides, within a short time window. The chain is
// unaware (human messages are off-chain); on-chain `recall_message` only covers System notices.
// CN: 双向撤回辅助（P3 §4.x）。撤回是**发送方**发出的控制信封，在短时间窗口内对**收发双方**隐藏
// 一条已发消息。链对此无感（人类消息在链下）；链上 `recall_message` 仅覆盖 System 通知。

import type { EnvelopeV1 } from "@/mls/envelope";
import type { LocalStore } from "@/store/localStore";
import type { MessageVM } from "@/types/viewModels";

/// EN: How long after sending a message the sender may still recall it. CN: 发送后仍可撤回的时长。
export const RECALL_WINDOW_MS = 2 * 60 * 1000;

type RecallStore = Pick<LocalStore, "getMessage" | "updateMessage">;

/// EN: Whether the UI should offer recall for this message. Only the sender's own delivered
/// (sent/acked) text/media messages within the window — never System/reaction/ephemeral/pending.
/// CN: 是否允许撤回该消息。仅发送方自己**已送达**（sent/acked）的文本/媒体消息且在时间窗口内——
/// System/reaction/阅后即焚/未发送均不可。
export function canRecallMessage(msg: MessageVM, now: number = Date.now()): boolean {
  if (!msg.isOutgoing) return false;
  if (msg.status === "recalled") return false;
  if (msg.status !== "sent" && msg.status !== "acked") return false;
  if (msg.content.type === "system" || msg.content.type === "reaction") return false;
  if (msg.ephemeralTtlMs != null || msg.ephemeralBurnAt != null) return false;
  return now - msg.sentAt <= RECALL_WINDOW_MS;
}

/// EN: Apply a recall to a local message: flip to the `recalled` placeholder and blank the
/// original content so the plaintext is not retained locally or in the cold archive. Returns
/// false when the target is absent (e.g. recall arrived before the message). CN: 对本地消息应用
/// 撤回：翻转为「已撤回」占位并清空原文。目标不存在时返回 false（如撤回早于消息到达）。
export async function markMessageRecalled(
  store: RecallStore,
  convId: string,
  target: string,
): Promise<boolean> {
  const existing = await store.getMessage(convId, target);
  if (!existing) return false;
  await store.updateMessage(convId, target, {
    status: "recalled",
    content: { type: "text", text: "" },
    starred: false,
  });
  return true;
}

/// EN: Handle an inbound `type=recall` control envelope (never rendered as its own bubble).
/// CN: 处理入站 `type=recall` 控制信封（不渲染为独立气泡）。
export async function applyRecallEnvelope(
  store: RecallStore,
  convId: string,
  env: EnvelopeV1,
): Promise<boolean> {
  if (env.type !== "recall") return false;
  const target = (env.body as { target?: string } | null)?.target;
  if (!target) return false;
  return markMessageRecalled(store, convId, target);
}
