// EN: Cross-device contacts-vault pointer channel (relay KV + localStorage fallback).
// CN: 跨设备通讯录 vault 指针通道（relay KV + localStorage 兜底）。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { ContactVaultPointer } from "@/store/contactVault";

const lsKey = (account: string) => `nexchat:contacts-vault:${account}`;

export function readLocalContactsPointer(account: string): ContactVaultPointer | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return null;
    const p = JSON.parse(raw) as ContactVaultPointer;
    if (!p.cid || !p.updated_at) return null;
    return p;
  } catch {
    return null;
  }
}

export function writeLocalContactsPointer(account: string, ptr: ContactVaultPointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(
  a: ContactVaultPointer | null,
  b: ContactVaultPointer | null,
): ContactVaultPointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest vault CID (relay + local). CN: 发布最新 vault CID（relay + 本地）。
export async function publishContactsPointer(
  account: string,
  ptr: ContactVaultPointer,
): Promise<void> {
  const prev = readLocalContactsPointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalContactsPointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "contacts_put",
    "contacts_ack",
    ptr,
    writeLocalContactsPointer,
    wsContactsFetch,
  );
}

/// EN: Fetch newest pointer (max of local + relay). CN: 取最新指针（本地与 relay 较大者）。
export async function fetchContactsPointer(account: string): Promise<ContactVaultPointer | null> {
  const local = readLocalContactsPointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsContactsFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsContactsFetch(account: string): Promise<ContactVaultPointer | null> {
  return relayOneShotFetch(account, { type: "contacts_fetch" }, (m, _requestId) => {
    if (
      m.type === "contacts_reply" &&
      typeof m.cid === "string" &&
      typeof m.updated_at === "number"
    ) {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "contacts_reply") return null;
    return undefined;
  });
}
