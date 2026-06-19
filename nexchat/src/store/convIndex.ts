// EN: Cross-device encrypted conversation-index blob (CHAT_P2 §2.1 / §3 LWW merge).
// CN: 跨设备加密会话索引 blob（CHAT_P2 §2.1 / §3 LWW 合并）。

import type { LocalConv } from "@/merge/spec";
import { convKey } from "@/merge/spec";
import { directMlsKey } from "@/mls/directConv";
import { keyVault } from "@/keyvault/keyvault";
import { openVersionedBlob, sealVersionedBlob } from "@/keyvault/blobSeal";

export interface ConvIndexEntry {
  kind: "direct" | "group";
  peer_ref?: string;
  group_id?: number;
  mls_ref?: string;
  title?: string;
  pinned: boolean;
  muted: boolean;
  last_read?: string;
  last_active: number;
  updated_at: number;
  tombstone?: boolean;
}

export interface ConvIndexBlob {
  v: 1;
  updated_at: number;
  device_id: string;
  conversations: ConvIndexEntry[];
}

export interface ConvIndexPointer {
  cid: string;
  updated_at: number;
}

export function entryKey(e: ConvIndexEntry): string {
  return e.kind === "direct" ? `d:${e.peer_ref ?? ""}` : `g:${e.group_id ?? 0}`;
}

/// EN: Build index snapshot from local off-chain rows. CN: 从链下本地行构建索引快照。
export function buildIndexFromLocal(
  convs: LocalConv[],
  selfAddress: string,
  deviceId: string,
  now = Date.now(),
): ConvIndexBlob {
  const conversations: ConvIndexEntry[] = convs.map((c) => {
    const updated_at = c.lastActive || now;
    if (c.kind === "direct") {
      const peer = c.peer ?? "";
      return {
        kind: "direct" as const,
        peer_ref: peer,
        mls_ref: peer ? directMlsKey(selfAddress, peer) : undefined,
        title: c.title,
        pinned: c.pinnedPref ?? false,
        muted: c.dndPref ?? false,
        last_active: c.lastActive,
        updated_at,
      };
    }
    return {
      kind: "group" as const,
      group_id: c.groupId,
      mls_ref: c.groupId != null ? `g:${c.groupId}` : undefined,
      title: c.title,
      pinned: c.pinnedPref ?? false,
      muted: c.dndPref ?? false,
      last_active: c.lastActive,
      updated_at,
    };
  });
  return { v: 1, updated_at: now, device_id: deviceId, conversations };
}

/// EN: Field-level LWW merge of two index blobs (§3 baseline). CN: 两索引 blob 的字段级 LWW 合并。
export function mergeIndexBlobs(a: ConvIndexBlob, b: ConvIndexBlob): ConvIndexBlob {
  const map = new Map<string, ConvIndexEntry>();
  for (const e of [...a.conversations, ...b.conversations]) {
    const k = entryKey(e);
    const prev = map.get(k);
    if (!prev) {
      map.set(k, e);
      continue;
    }
    map.set(k, mergeEntries(prev, e));
  }
  // EN: Keep tombstones in the blob so remote devices can apply deletions (contact-vault parity).
  // CN: 保留墓碑条目，供远端设备应用删除（与 contact-vault 同构）。
  const conversations = [...map.values()];
  return {
    v: 1,
    updated_at: Math.max(a.updated_at, b.updated_at),
    device_id: a.updated_at >= b.updated_at ? a.device_id : b.device_id,
    conversations,
  };
}

function mergeEntries(a: ConvIndexEntry, b: ConvIndexEntry): ConvIndexEntry {
  // EN: Tombstone (delete conversation) is terminal — must not lose to a stale live row.
  // CN: 墓碑（删除会话）为终态——不得被陈旧的 live 行覆盖。
  const aTomb = !!a.tombstone;
  const bTomb = !!b.tombstone;
  if (aTomb !== bTomb) return aTomb ? a : b;
  if (b.updated_at > a.updated_at) return b;
  if (a.updated_at > b.updated_at) return a;
  return {
    ...a,
    pinned: b.pinned,
    muted: b.muted,
    last_active: Math.max(a.last_active, b.last_active),
    title: b.title ?? a.title,
    last_read: b.last_read ?? a.last_read,
    updated_at: Math.max(a.updated_at, b.updated_at),
    tombstone: aTomb && bTomb,
  };
}

/// EN: Apply merged index entries onto LocalConv rows (for restore). CN: 将合并后的索引应用到本地行。
export function applyIndexToLocal(convs: LocalConv[], index: ConvIndexBlob): LocalConv[] {
  const byKey = new Map(convs.map((c) => [convKey(c.kind, c.peer, c.groupId), c]));
  for (const e of index.conversations) {
    if (e.tombstone) continue;
    const key =
      e.kind === "direct" ? convKey("direct", e.peer_ref) : convKey("group", undefined, e.group_id);
    const prev = byKey.get(key);
    const row: LocalConv = prev ?? {
      kind: e.kind,
      peer: e.peer_ref,
      groupId: e.group_id,
      lastActive: e.last_active,
      unread: 0,
    };
    row.pinnedPref = e.pinned;
    row.dndPref = e.muted;
    row.lastActive = Math.max(row.lastActive, e.last_active);
    if (e.title) row.title = e.title;
    byKey.set(key, row);
  }
  return [...byKey.values()];
}

/// EN: AES-GCM seal, versioned wire `0x02||iv(12)||ct` (legacy reads fall back, §5.0).
/// CN: AES-GCM 封装，版本化 wire `0x02||iv(12)||ct`（旧格式读取回退，§5.0）。
export async function encryptIndexBlob(blob: ConvIndexBlob): Promise<Uint8Array> {
  const key = await keyVault.deriveConvIndexKey();
  return sealVersionedBlob(key, new TextEncoder().encode(JSON.stringify(blob)));
}

export async function decryptIndexBlob(packed: Uint8Array): Promise<ConvIndexBlob> {
  const key = await keyVault.deriveConvIndexKey();
  const legacy = await keyVault.deriveLegacyConvIndexKey();
  const pt = await openVersionedBlob(packed, key, legacy);
  const blob = JSON.parse(new TextDecoder().decode(pt)) as ConvIndexBlob;
  if (blob.v !== 1) throw new Error(`unsupported conv-index version ${blob.v}`);
  return blob;
}

export function deviceId(): string {
  if (typeof localStorage === "undefined") return "node";
  const k = "nexchat:device-id";
  let id = localStorage.getItem(k);
  if (!id) {
    id = globalThis.crypto?.randomUUID?.() ?? `dev-${Date.now()}`;
    localStorage.setItem(k, id);
  }
  return id;
}
