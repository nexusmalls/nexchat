// EN: The Merge engine — the heart of the client. Implements the README
// "client Merge Spec" as a PURE function so it is fully unit-testable.
// Input: on-chain slice (from chat_listConversations) + local off-chain state.
// Output: the real, render-ready conversation list.
// CN: Merge 引擎——客户端心脏。把 README「客户端 Merge Spec」实现为纯函数，便于全覆盖单测。
// 输入：链上切片（chat_listConversations）+ 链下本地状态；输出：可直接渲染的真实会话列表。
//
// 关键不变量（见 CHAT_FRONTEND_PLAN.md §6 / §3.1.4）：
//  - 会话集合 = 链上 ∪ 链下；纯链下私聊也必须出现。
//  - `muted` 按 kind 拆解：direct→dnd（能发言），group→adminMuted（不能发言）。
//  - App 角标 = Σ 合并后 unread（含链下），≠ total_direct_unread。
//  - 群链上 last_active 恒 0，用链下 last_active 跨类型重排。

import type {
  ConversationVM,
  ConvKind,
  ConvPresence,
  GroupRole,
} from "@/types/viewModels";

/// EN: One row from the on-chain slice (`chat_listConversations` / RpcConversation).
/// CN: 链上切片中的一行（`chat_listConversations` / RpcConversation）。
export interface OnChainRow {
  kind: ConvKind;
  directId?: string;
  groupId?: number;
  peer?: string;
  /** group name (empty for direct) */
  name: string;
  /** group avatar CID (empty for direct) */
  avatarCid: string;
  /** direct → System-channel last-active BLOCK number; group → 0 */
  lastActive: number;
  /** direct → System-channel unread; group → 0 */
  unread: number;
  pinned: boolean;
  /** SEMANTICS DIFFER BY kind: direct=DND, group=admin mute */
  muted: boolean;
  archived: boolean;
  memberCount: number;
  /** 0=Owner,1=Admin,2=Member,255=direct/non-member */
  groupRole: number;
  /** enrichment from group_mls_snapshot (not in list_conversations); default false */
  frozen?: boolean;
}

/// EN: Local off-chain conversation state (MLS lib + encrypted conv-index + prefs).
/// CN: 链下本地会话状态（MLS 库 + 加密会话索引 + 偏好）。
export interface LocalConv {
  kind: ConvKind;
  peer?: string;
  groupId?: number;
  /** real off-chain last message time (ms epoch) */
  lastActive: number;
  /** off-chain MLS unread */
  unread: number;
  /** EN: local @me mention unread (groups). CN: 本地「@我」未读（群）。 */
  mentionUnread?: number;
  /** local pin preference (the ONLY pin for groups) */
  pinnedPref?: boolean;
  /** local DND preference */
  dndPref?: boolean;
  /** local archive preference (groups only; direct archive is on-chain) */
  archivedPref?: boolean;
  lastMessagePreview?: string;
  /** local cached display title (direct nickname) */
  title?: string;
  avatarCid?: string;
}

export interface MergeOptions {
  /** EN: include the on-chain System-channel unread into a direct conv's unread.
   *  CN: 是否把链上 System 通道未读计入私聊未读。默认 true（通知卡片才有未读显示）。 */
  countSystemUnread?: boolean;
}

/// EN: direct → `d:{peer}`, group → `g:{groupId}`. CN: 统一会话主键。
export function convKey(kind: ConvKind, peer?: string, groupId?: number): string {
  return kind === "direct" ? `d:${peer ?? ""}` : `g:${groupId ?? ""}`;
}

function roleFromTag(tag: number): GroupRole {
  switch (tag) {
    case 0:
      return "owner";
    case 1:
      return "admin";
    case 2:
      return "member";
    default:
      return "na";
  }
}

interface Bucket {
  key: string;
  kind: ConvKind;
  onChain?: OnChainRow;
  local?: LocalConv;
}

/// EN: Merge the on-chain slice with local off-chain state into the real list.
/// CN: 把链上切片与链下本地状态合并为真实会话列表。
export function mergeConversations(
  onChain: readonly OnChainRow[],
  local: readonly LocalConv[],
  blockToTime: (block: number) => number,
  opts: MergeOptions = {},
): ConversationVM[] {
  const countSystemUnread = opts.countSystemUnread ?? true;
  const buckets = new Map<string, Bucket>();

  for (const row of onChain) {
    const key = convKey(row.kind, row.peer, row.groupId);
    buckets.set(key, { key, kind: row.kind, onChain: row });
  }
  for (const lc of local) {
    const key = convKey(lc.kind, lc.peer, lc.groupId);
    const existing = buckets.get(key);
    if (existing) existing.local = lc;
    else buckets.set(key, { key, kind: lc.kind, local: lc });
  }

  const out: ConversationVM[] = [];
  for (const b of buckets.values()) {
    const merged = b.kind === "direct" ? mergeDirect(b, blockToTime, countSystemUnread) : mergeGroup(b, blockToTime);
    // EN: Drop empty groups (disbanded / teardown / stale local-only) from chat + contacts lists.
    // CN: 从会话列表与通讯录移除 0 成员群（已解散 / 拆除中 / 仅本地的陈旧行）。
    if (merged.kind === "group" && merged.memberCount === 0) continue;
    out.push(merged);
  }

  // 排序：置顶组在前，组内按 recency 倒序。
  out.sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.recency - a.recency;
  });
  return out;
}

function presenceOf(hasChain: boolean, hasLocal: boolean): ConvPresence {
  if (hasChain && hasLocal) return "both";
  return hasChain ? "onChainOnly" : "offChainOnly";
}

function mergeDirect(
  b: Bucket,
  blockToTime: (block: number) => number,
  countSystemUnread: boolean,
): ConversationVM {
  const oc = b.onChain;
  const lc = b.local;
  const peer = oc?.peer ?? lc?.peer ?? "";

  const systemUnread = oc?.unread ?? 0;
  const localUnread = lc?.unread ?? 0;
  const unread = localUnread + (countSystemUnread ? systemUnread : 0);

  const chainRecency = oc ? blockToTime(oc.lastActive) : 0;
  const localRecency = lc?.lastActive ?? 0;

  return {
    convId: b.key,
    kind: "direct",
    title: lc?.title ?? peer,
    avatarCid: lc?.avatarCid || undefined,
    peer,
    groupId: undefined,
    lastMessagePreview: lc?.lastMessagePreview,
    recency: Math.max(chainRecency, localRecency),
    unread,
    // pinned: 链上私聊置顶 OR 本地置顶偏好
    pinned: (oc?.pinned ?? false) || (lc?.pinnedPref ?? false),
    // dnd: 链上 DND OR 本地免打扰偏好
    dnd: (oc?.muted ?? false) || (lc?.dndPref ?? false),
    // 私聊永远没有"管理员禁言"
    adminMuted: false,
    // archived: 私聊链上权威
    archived: oc?.archived ?? false,
    frozen: false,
    memberCount: 0,
    myRole: "na",
    presence: presenceOf(!!oc, !!lc),
  };
}

function mergeGroup(b: Bucket, blockToTime: (block: number) => number): ConversationVM {
  const oc = b.onChain;
  const lc = b.local;
  const groupId = oc?.groupId ?? lc?.groupId;

  // 群链上 last_active 恒 0；用链下真值排序。
  const chainRecency = oc ? blockToTime(oc.lastActive) : 0;
  const localRecency = lc?.lastActive ?? 0;

  return {
    convId: b.key,
    kind: "group",
    // 群名/头像/成员数/角色：链上权威
    title: oc?.name ?? lc?.title ?? "",
    avatarCid: (oc?.avatarCid || lc?.avatarCid) || undefined,
    peer: undefined,
    groupId,
    lastMessagePreview: lc?.lastMessagePreview,
    recency: Math.max(chainRecency, localRecency),
    // 群链上 unread 恒 0；以链下 MLS 未读为准
    unread: lc?.unread ?? 0,
    mentionUnread: lc?.mentionUnread ?? 0,
    // 群置顶为客户端能力（链上恒 false）
    pinned: lc?.pinnedPref ?? false,
    // 群 DND 是本地偏好（≠ 管理员禁言）
    dnd: lc?.dndPref ?? false,
    // adminMuted: 链上 muted = 被管理员禁言（不能发言）
    adminMuted: oc?.muted ?? false,
    archived: lc?.archivedPref ?? false,
    frozen: oc?.frozen ?? false,
    memberCount: oc?.memberCount ?? 0,
    myRole: roleFromTag(oc?.groupRole ?? 255),
    presence: presenceOf(!!oc, !!lc),
  };
}

/// EN: App global unread badge = sum of merged unread (NOT total_direct_unread).
/// CN: App 全局未读角标 = 合并后 unread 之和（**不是** total_direct_unread）。
export function appUnreadBadge(convs: readonly ConversationVM[]): number {
  return convs.reduce((acc, c) => acc + c.unread, 0);
}
