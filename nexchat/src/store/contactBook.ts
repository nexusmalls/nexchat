// EN: Per-account contact book (localStorage). Merged with env demo roster for UI.
// CN: 按账户的本地通讯录（localStorage），与 env 演示名册合并展示。

import { decodeAddress } from "@polkadot/util-crypto";
import type { MentionMember } from "@/p3/mentions";
import { canonicalAddress, shortAddress } from "@/wallet/address";

const LS_PREFIX = "nexchat-contacts:";

export interface SavedContact {
  address: string;
  label: string;
  addedAt: number;
  /** EN: LWW timestamp for cross-device vault merge. CN: 跨设备 vault 合并用 LWW 时间戳。 */
  updatedAt?: number;
}

function lsKey(account: string): string {
  return `${LS_PREFIX}${account}`;
}

export function loadContacts(account: string): SavedContact[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as SavedContact[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function saveContacts(account: string, contacts: SavedContact[]): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(contacts));
}

/// EN: Validate SS58 (any prefix) and return canonical SS58-273 for protocol use.
/// CN: 校验 SS58（任意前缀）并返回协议用规范 SS58-273。
export function parseContactAddress(input: string): string {
  const trimmed = input.trim();
  if (!trimmed) throw new Error("请输入链上地址");
  try {
    decodeAddress(trimmed);
  } catch {
    throw new Error("无效的 SS58 地址");
  }
  return canonicalAddress(trimmed);
}

export function savedToMentionMember(c: SavedContact): MentionMember {
  const label = c.label;
  const short = shortAddress(c.address);
  return {
    ref: label,
    address: c.address,
    labels: [label, label.toLowerCase(), c.address, short],
  };
}

export function loadUserRoster(account: string): MentionMember[] {
  return loadContacts(account).map(savedToMentionMember);
}

/// EN: Env demo roster ⊕ user contacts, deduped by canonical address.
/// CN: 演示名册与用户通讯录合并，按规范地址去重。
export function mergeRosters(
  env: readonly MentionMember[],
  user: readonly MentionMember[],
  selfAddress: string | undefined,
): MentionMember[] {
  const byAddr = new Map<string, MentionMember>();
  for (const m of env) {
    if (selfAddress && m.address === selfAddress) continue;
    byAddr.set(m.address, m);
  }
  for (const m of user) {
    if (selfAddress && m.address === selfAddress) continue;
    byAddr.set(m.address, m);
  }
  return [...byAddr.values()].sort((a, b) =>
    (a.labels[0] ?? a.ref).localeCompare(b.labels[0] ?? b.ref),
  );
}

export function isUserContact(account: string, peerAddress: string): boolean {
  return loadContacts(account).some((c) => c.address === peerAddress);
}
