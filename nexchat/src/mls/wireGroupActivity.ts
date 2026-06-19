// EN: Activity signal for group Wire-ification lazy/on-demand Add (CHAT_GROUP_WIREIFY_DESIGN §8.1). A group
// is ACTIVE when it is currently open OR recently used (non-archived, recency within the window). Dormant
// / archived groups are deferred at device-join offer time and grafted lazily via `activateGroup` when the
// user next opens them. Pure + deterministic → unit-testable.
//
// CN: 群 Wire 化延迟/按需 Add 的活跃度信号（设计 §8.1）。群在**当前打开**或**近期使用**（未归档、recency 在
// 窗口内）时为 ACTIVE。休眠/归档群在设备 join offer 时延迟，待用户下次打开时经 `activateGroup` 懒嫁接。纯函数、
// 确定性 → 可单测。

import type { ConversationVM } from "@/types/viewModels";

/** EN: Groups with `recency` newer than this are treated as active at unlock/join-offer time. CN: 解锁 /
 *  join-offer 时 `recency` 新于此阈值的群视为活跃。 */
export const WIRE_GROUP_ACTIVE_RECENCY_MS = 7 * 24 * 60 * 60 * 1000;

export interface WireGroupActivityContext {
  activeConvId: string | null;
  conversations: readonly ConversationVM[];
  /** EN: override for tests. CN: 测试用时间覆盖。 */
  nowMs?: number;
}

/// EN: True when `convId` should be grafted immediately (join-now) rather than deferred. CN: `convId` 应
/// 立即嫁接（join-now）而非延迟时为真。
export function isWireGroupActive(convId: string, ctx: WireGroupActivityContext): boolean {
  if (!convId.startsWith("g:")) return false;
  if (ctx.activeConvId === convId) return true;
  const row = ctx.conversations.find((c) => c.convId === convId);
  if (!row || row.kind !== "group") return false;
  if (row.archived) return false;
  const now = ctx.nowMs ?? Date.now();
  return now - row.recency <= WIRE_GROUP_ACTIVE_RECENCY_MS;
}
