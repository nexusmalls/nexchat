// EN: Cross-device encrypted message-history archive blob (CHAT_P2 §2.1 extension).
// CN: 跨设备加密聊天历史归档 blob（CHAT_P2 §2.1 扩展）。

import { keyVault } from "@/keyvault/keyvault";
import { openVersionedBlob, sealVersionedBlob } from "@/keyvault/blobSeal";
import { deviceId } from "@/store/convIndex";
import type { LocalStore } from "@/store/localStore";
import type { MessageContent, MessageVM, MsgSource, MsgStatus } from "@/types/viewModels";

export interface MsgArchiveEntry {
  client_msg_id: string;
  conv_id: string;
  sender_ref: string;
  is_outgoing: boolean;
  sent_at: number;
  content: MessageContent;
  reply_to?: string;
  mentions: string[];
  forward_from?: MessageVM["forwardFrom"];
  starred: boolean;
  status: MsgStatus;
  source: MsgSource;
  updated_at: number;
  tombstone?: boolean;
}

export interface MsgArchiveConv {
  conv_id: string;
  messages: MsgArchiveEntry[];
  updated_at: number;
}

export interface MessageArchiveBlob {
  v: 1;
  updated_at: number;
  device_id: string;
  conversations: MsgArchiveConv[];
}

export interface MsgArchivePointer {
  cid: string;
  updated_at: number;
}

export function msgEntryKey(convId: string, clientMsgId: string): string {
  return `${convId}::${clientMsgId}`;
}

/// EN: Ephemeral / burn-after-read messages must not enter cold archive. Recalled messages
/// ARE archived (with blanked content) so the "recalled" placeholder reaches the user's other
/// devices instead of resurrecting the original text. CN: 阅后即焚消息不得进入冷归档。撤回消息
/// **仍归档**（正文已清空），以便「已撤回」占位同步到用户其他设备，而非把原文复活。
export function isArchivableMessage(msg: MessageVM): boolean {
  if (msg.ephemeralTtlMs != null || msg.ephemeralBurnAt != null) return false;
  return true;
}

export function msgToArchiveEntry(msg: MessageVM, now = Date.now()): MsgArchiveEntry {
  return {
    client_msg_id: msg.clientMsgId,
    conv_id: msg.convId,
    sender_ref: msg.senderRef,
    is_outgoing: msg.isOutgoing,
    sent_at: msg.sentAt,
    content: msg.content,
    reply_to: msg.replyTo,
    mentions: msg.mentions,
    forward_from: msg.forwardFrom,
    starred: msg.starred,
    status: msg.status,
    source: msg.source,
    updated_at: Math.max(msg.sentAt, now),
  };
}

export function archiveEntryToMessage(e: MsgArchiveEntry): MessageVM {
  return {
    clientMsgId: e.client_msg_id,
    convId: e.conv_id,
    senderRef: e.sender_ref,
    isOutgoing: e.is_outgoing,
    sentAt: e.sent_at,
    content: e.content,
    replyTo: e.reply_to,
    mentions: e.mentions,
    forwardFrom: e.forward_from,
    starred: e.starred,
    status: e.status,
    source: e.source,
  };
}

/// EN: Build archive snapshot from local timelines (non-ephemeral only). CN: 从本地时间线构建归档快照。
export async function buildArchiveFromLocal(
  store: LocalStore,
  maxPerConv: number,
  deviceIdValue: string = deviceId(),
  now = Date.now(),
): Promise<MessageArchiveBlob> {
  const convs = await store.listLocalConvs();
  const conversations: MsgArchiveConv[] = [];
  for (const c of convs) {
    const convId = c.kind === "direct" ? `d:${c.peer ?? ""}` : `g:${c.groupId ?? 0}`;
    const msgs = (await store.listMessages(convId))
      .filter(isArchivableMessage)
      .sort((a, b) => b.sentAt - a.sentAt || b.clientMsgId.localeCompare(a.clientMsgId))
      .slice(0, maxPerConv)
      .sort((a, b) => a.sentAt - b.sentAt || a.clientMsgId.localeCompare(b.clientMsgId));
    if (!msgs.length) continue;
    const entries = msgs.map((m) => msgToArchiveEntry(m, now));
    conversations.push({
      conv_id: convId,
      messages: entries,
      updated_at: Math.max(...entries.map((e) => e.updated_at), now),
    });
  }
  return {
    v: 1,
    updated_at: now,
    device_id: deviceIdValue,
    conversations,
  };
}

/// EN: LWW merge of two archive blobs (per message key). CN: 两归档 blob 按消息键 LWW 合并。
export function mergeArchiveBlobs(a: MessageArchiveBlob, b: MessageArchiveBlob): MessageArchiveBlob {
  const convMap = new Map<string, Map<string, MsgArchiveEntry>>();
  const ingest = (blob: MessageArchiveBlob) => {
    for (const conv of blob.conversations) {
      let msgs = convMap.get(conv.conv_id);
      if (!msgs) {
        msgs = new Map();
        convMap.set(conv.conv_id, msgs);
      }
      for (const e of conv.messages) {
        const k = e.client_msg_id;
        const prev = msgs.get(k);
        msgs.set(k, prev ? mergeMsgEntries(prev, e) : e);
      }
    }
  };
  ingest(a);
  ingest(b);

  const conversations: MsgArchiveConv[] = [];
  for (const [conv_id, msgs] of convMap) {
    const messages = [...msgs.values()];
    if (!messages.length) continue;
    conversations.push({
      conv_id,
      messages,
      updated_at: Math.max(...messages.map((e) => e.updated_at)),
    });
  }
  return {
    v: 1,
    updated_at: Math.max(a.updated_at, b.updated_at),
    device_id: a.updated_at >= b.updated_at ? a.device_id : b.device_id,
    conversations,
  };
}

function mergeMsgEntries(a: MsgArchiveEntry, b: MsgArchiveEntry): MsgArchiveEntry {
  // EN: Tombstone (local delete) is terminal — a sibling push that refreshes `updated_at` via
  // `buildArchiveFromLocal(now)` must not resurrect a row the user deleted on another device.
  // CN: 墓碑（本地删除）为终态——兄弟设备 push 时刷新的 `updated_at` 不得复活已在另一设备删除的行。
  const aTomb = !!a.tombstone;
  const bTomb = !!b.tombstone;
  if (aTomb !== bTomb) return aTomb ? a : b;
  // EN: Recall is terminal/irreversible — a recalled entry ALWAYS dominates regardless of
  // updated_at, so an offline device whose later build-time `now` would otherwise win cannot
  // resurrect the original text. CN: 撤回是终态/不可逆——撤回条目**始终**胜出（不看 updated_at），
  // 使离线设备较晚的构建时间 `now` 不会把原文复活。
  const aRecalled = a.status === "recalled";
  const bRecalled = b.status === "recalled";
  if (aRecalled !== bRecalled) return aRecalled ? a : b;
  if (b.updated_at > a.updated_at) return b;
  if (a.updated_at > b.updated_at) return a;
  const statusRank = (s: MsgStatus) =>
    s === "acked" ? 4 : s === "sent" ? 3 : s === "pending" ? 2 : s === "failed" ? 1 : 0;
  return statusRank(b.status) >= statusRank(a.status)
    ? { ...b, updated_at: Math.max(a.updated_at, b.updated_at) }
    : { ...a, updated_at: Math.max(a.updated_at, b.updated_at) };
}

/// EN: Tombstone messages present in `last` but absent from fresh local snapshot. CN: 本地已删消息记墓碑。
export function tombstonesForRemovedMessages(
  last: MessageArchiveBlob,
  local: MessageArchiveBlob,
  now = Date.now(),
): MsgArchiveEntry[] {
  const localKeys = new Set<string>();
  for (const conv of local.conversations) {
    for (const e of conv.messages) {
      localKeys.add(msgEntryKey(conv.conv_id, e.client_msg_id));
    }
  }
  const out: MsgArchiveEntry[] = [];
  for (const conv of last.conversations) {
    for (const e of conv.messages) {
      if (e.tombstone) continue;
      const k = msgEntryKey(conv.conv_id, e.client_msg_id);
      if (!localKeys.has(k)) {
        out.push({ ...e, tombstone: true, updated_at: Math.max(e.updated_at, now) });
      }
    }
  }
  return out;
}

/// EN: Merge blob with tombstone entries (keeps tombstones in stored blob for remote deletes).
/// CN: 将墓碑并入 blob（保留在存储 blob 中供远端应用删除）。
export function mergeArchiveWithTombstones(
  base: MessageArchiveBlob,
  tombstones: MsgArchiveEntry[],
  now = Date.now(),
): MessageArchiveBlob {
  if (!tombstones.length) return base;
  const byConv = new Map<string, MsgArchiveEntry[]>();
  for (const conv of base.conversations) byConv.set(conv.conv_id, [...conv.messages]);
  for (const t of tombstones) {
    const list = byConv.get(t.conv_id) ?? [];
    list.push(t);
    byConv.set(t.conv_id, list);
  }
  const conversations: MsgArchiveConv[] = [];
  for (const [conv_id, messages] of byConv) {
    const map = new Map<string, MsgArchiveEntry>();
    for (const e of messages) {
      const prev = map.get(e.client_msg_id);
      map.set(e.client_msg_id, prev ? mergeMsgEntries(prev, e) : e);
    }
    conversations.push({
      conv_id,
      messages: [...map.values()],
      updated_at: Math.max(...[...map.values()].map((e) => e.updated_at), now),
    });
  }
  return {
    v: 1,
    updated_at: Math.max(base.updated_at, now),
    device_id: base.device_id,
    conversations,
  };
}

/// EN: Flatten merged blob to live messages (no tombstones). CN: 将合并 blob 展平为可写入本地的消息。
export function archiveToMessages(blob: MessageArchiveBlob): MessageVM[] {
  const out: MessageVM[] = [];
  for (const conv of blob.conversations) {
    for (const e of conv.messages) {
      if (e.tombstone) continue;
      out.push(archiveEntryToMessage(e));
    }
  }
  return out;
}

/// EN: AES-GCM seal, versioned wire `0x02||iv(12)||ct` (legacy reads fall back, §5.0).
/// CN: AES-GCM 封装，版本化 wire `0x02||iv(12)||ct`（旧格式读取回退，§5.0）。
export async function encryptArchiveBlob(blob: MessageArchiveBlob): Promise<Uint8Array> {
  const key = await keyVault.deriveMsgArchiveKey();
  return sealVersionedBlob(key, new TextEncoder().encode(JSON.stringify(blob)));
}

export async function decryptArchiveBlob(packed: Uint8Array): Promise<MessageArchiveBlob> {
  const key = await keyVault.deriveMsgArchiveKey();
  const legacy = await keyVault.deriveLegacyMsgArchiveKey();
  const pt = await openVersionedBlob(packed, key, legacy);
  const blob = JSON.parse(new TextDecoder().decode(pt)) as MessageArchiveBlob;
  if (blob.v !== 1) throw new Error(`unsupported msg-archive version ${blob.v}`);
  return blob;
}
