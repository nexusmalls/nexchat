// EN: Locally hidden conversation ids — removes a thread from the chat list while the user
// remains in on-chain groups / crypto sessions can resume on reopen. Synced cross-device via
// conv-index tombstones (`convIndex.ts` / `convIndexSync.ts`). CN: 本地隐藏的会话 id——从聊天列表
// 移除线程，链上群成员关系不变，重开会话时可恢复加密态。经 conv-index 墓碑跨设备同步。

import type { LocalStore } from "@/store/localStore";
import { entryKey, type ConvIndexBlob, type ConvIndexEntry } from "@/store/convIndex";
import { convKey } from "@/merge/spec";
import type { LocalConv } from "@/merge/spec";

export const DELETED_CONVS_META = "__meta__/deleted-conv-ids";

/// EN: Load the set of conversation ids hidden from the chat list. CN: 读取从聊天列表隐藏的会话 id。
export async function loadDeletedConvIds(store: LocalStore): Promise<Set<string>> {
  const raw = await store.getMeta?.<string[]>(DELETED_CONVS_META);
  return new Set(raw ?? []);
}

async function saveDeletedConvIds(store: LocalStore, ids: Set<string>): Promise<void> {
  await store.setMeta?.(DELETED_CONVS_META, [...ids]);
}

/// EN: Hide `convId` from the chat list (local + conv-index tombstone on next push).
/// CN: 从聊天列表隐藏 `convId`（本地 + 下次 push 写 conv-index 墓碑）。
export async function markConversationDeleted(store: LocalStore, convId: string): Promise<void> {
  const ids = await loadDeletedConvIds(store);
  ids.add(convId);
  await saveDeletedConvIds(store, ids);
}

/// EN: Unhide when the user reopens the thread or a new inbound message arrives.
/// CN: 用户重新打开会话或有新入站消息时取消隐藏。
export async function clearConversationDeleted(store: LocalStore, convId: string): Promise<void> {
  const ids = await loadDeletedConvIds(store);
  if (!ids.delete(convId)) return;
  await saveDeletedConvIds(store, ids);
}

/// EN: Minimal conv-index tombstone row for a UI conv id (`d:…` / `g:…`). CN: 为 UI 会话 id 生成
/// 最小 conv-index 墓碑行。
export function tombstoneEntryForConvId(convId: string, now = Date.now()): ConvIndexEntry {
  if (convId.startsWith("d:")) {
    return {
      kind: "direct",
      peer_ref: convId.slice(2),
      pinned: false,
      muted: false,
      last_active: 0,
      updated_at: now,
      tombstone: true,
    };
  }
  const groupId = Number(convId.slice(2));
  return {
    kind: "group",
    group_id: Number.isFinite(groupId) ? groupId : 0,
    pinned: false,
    muted: false,
    last_active: 0,
    updated_at: now,
    tombstone: true,
  };
}

/// EN: Apply conv-index tombstones after restore — purge local rows + update hidden set.
/// CN: 恢复后应用 conv-index 墓碑——清除本地行并更新隐藏集合。
export async function applyIndexTombstones(store: LocalStore, index: ConvIndexBlob): Promise<void> {
  const ids = await loadDeletedConvIds(store);
  let changed = false;
  for (const e of index.conversations) {
    if (!e.tombstone) continue;
    const convId = entryKey(e);
    if (!ids.has(convId)) {
      ids.add(convId);
      changed = true;
    }
    try {
      await store.removeLocalConversation(convId);
    } catch {
      /* already gone */
    }
  }
  if (changed) await saveDeletedConvIds(store, ids);
}

/// EN: Tombstone rows to push: entries dropped locally since `last`, plus explicit hidden ids.
/// CN: 待推送墓碑：相对 `last` 本地已删的条目 + 显式隐藏 id。
export function tombstonesForDeletedConversations(
  last: ConvIndexBlob | null,
  localConvs: LocalConv[],
  deletedIds: ReadonlySet<string>,
  now = Date.now(),
): ConvIndexEntry[] {
  const localKeys = new Set(localConvs.map((c) => convKey(c.kind, c.peer, c.groupId)));
  const out = new Map<string, ConvIndexEntry>();

  if (last) {
    for (const e of last.conversations) {
      if (e.tombstone) continue;
      const k = entryKey(e);
      if (localKeys.has(k)) continue;
      out.set(k, { ...e, tombstone: true, updated_at: Math.max(e.updated_at, now) });
    }
  }

  for (const convId of deletedIds) {
    if (localKeys.has(convId)) continue;
    const prev = last?.conversations.find((e) => entryKey(e) === convId);
    if (prev?.tombstone) continue;
    out.set(convId, prev
      ? { ...prev, tombstone: true, updated_at: Math.max(prev.updated_at, now) }
      : tombstoneEntryForConvId(convId, now));
  }

  return [...out.values()];
}
