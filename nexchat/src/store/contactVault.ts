// EN: Cross-device encrypted contact-book vault blob (conv-index pattern, CHAT_P2 §2.1).
// CN: 跨设备加密通讯录 vault blob（与 conv-index 同构，CHAT_P2 §2.1）。

import { keyVault } from "@/keyvault/keyvault";
import { openVersionedBlob, sealVersionedBlob } from "@/keyvault/blobSeal";
import type { SavedContact } from "@/store/contactBook";
import { deviceId } from "@/store/convIndex";

export interface ContactVaultEntry {
  address: string;
  label: string;
  addedAt: number;
  updated_at: number;
  tombstone?: boolean;
}

export interface ContactVaultBlob {
  v: 1;
  updated_at: number;
  device_id: string;
  contacts: ContactVaultEntry[];
}

export interface ContactVaultPointer {
  cid: string;
  updated_at: number;
}

/// EN: Build vault snapshot from local contact rows. CN: 从本地通讯录行构建 vault 快照。
export function buildVaultFromLocal(
  contacts: SavedContact[],
  deviceIdValue: string = deviceId(),
  now = Date.now(),
): ContactVaultBlob {
  const rows: ContactVaultEntry[] = contacts.map((c) => ({
    address: c.address,
    label: c.label,
    addedAt: c.addedAt,
    updated_at: c.updatedAt ?? c.addedAt ?? now,
  }));
  return { v: 1, updated_at: now, device_id: deviceIdValue, contacts: rows };
}

/// EN: Field-level LWW merge of two vault blobs. CN: 两 vault blob 的字段级 LWW 合并。
export function mergeVaultBlobs(a: ContactVaultBlob, b: ContactVaultBlob): ContactVaultBlob {
  const map = new Map<string, ContactVaultEntry>();
  for (const e of [...a.contacts, ...b.contacts]) {
    const prev = map.get(e.address);
    if (!prev) {
      map.set(e.address, e);
      continue;
    }
    map.set(e.address, mergeEntries(prev, e));
  }
  // EN: Keep tombstones in the blob so remote devices can apply deletions.
  // CN: 保留墓碑条目，供远端设备应用删除。
  const contacts = [...map.values()];
  return {
    v: 1,
    updated_at: Math.max(a.updated_at, b.updated_at),
    device_id: a.updated_at >= b.updated_at ? a.device_id : b.device_id,
    contacts,
  };
}

function mergeEntries(a: ContactVaultEntry, b: ContactVaultEntry): ContactVaultEntry {
  if (b.updated_at > a.updated_at) return b;
  if (a.updated_at > b.updated_at) return a;
  return {
    ...a,
    label: b.label,
    addedAt: Math.min(a.addedAt, b.addedAt),
    updated_at: Math.max(a.updated_at, b.updated_at),
    tombstone: a.tombstone && b.tombstone,
  };
}

/// EN: Convert merged vault to SavedContact rows for localStorage. CN: 将合并后的 vault 写回本地行。
export function vaultToSavedContacts(vault: ContactVaultBlob): SavedContact[] {
  return vault.contacts
    .filter((e) => !e.tombstone)
    .map((e) => ({
      address: e.address,
      label: e.label,
      addedAt: e.addedAt,
      updatedAt: e.updated_at,
    }));
}

/// EN: Tombstone entries for contacts removed locally since last push. CN: 本地删除的联系人记为墓碑。
export function tombstonesForRemoved(
  last: ContactVaultBlob,
  local: SavedContact[],
  now = Date.now(),
): ContactVaultEntry[] {
  const localAddrs = new Set(local.map((c) => c.address));
  const out: ContactVaultEntry[] = [];
  for (const e of last.contacts) {
    if (e.tombstone || localAddrs.has(e.address)) continue;
    out.push({ ...e, tombstone: true, updated_at: Math.max(e.updated_at, now) });
  }
  return out;
}

/// EN: AES-GCM seal, versioned wire `0x02||iv(12)||ct` (legacy reads fall back, §5.0).
/// CN: AES-GCM 封装，版本化 wire `0x02||iv(12)||ct`（旧格式读取回退，§5.0）。
export async function encryptVaultBlob(blob: ContactVaultBlob): Promise<Uint8Array> {
  const key = await keyVault.deriveContactsVaultKey();
  return sealVersionedBlob(key, new TextEncoder().encode(JSON.stringify(blob)));
}

export async function decryptVaultBlob(packed: Uint8Array): Promise<ContactVaultBlob> {
  const key = await keyVault.deriveContactsVaultKey();
  const legacy = await keyVault.deriveLegacyContactsVaultKey();
  const pt = await openVersionedBlob(packed, key, legacy);
  const blob = JSON.parse(new TextDecoder().decode(pt)) as ContactVaultBlob;
  if (blob.v !== 1) throw new Error(`unsupported contact-vault version ${blob.v}`);
  return blob;
}
