// EN: Cross-device conv-index pointer channel (relay KV + localStorage fallback).
// CN: 跨设备 conv-index 指针通道（relay KV + localStorage 兜底）。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { ConvIndexPointer } from "@/store/convIndex";

const lsKey = (account: string) => `nexchat:index:${account}`;

export function readLocalIndexPointer(account: string): ConvIndexPointer | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return null;
    const p = JSON.parse(raw) as ConvIndexPointer;
    if (!p.cid || !p.updated_at) return null;
    return p;
  } catch {
    return null;
  }
}

export function writeLocalIndexPointer(account: string, ptr: ConvIndexPointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(a: ConvIndexPointer | null, b: ConvIndexPointer | null): ConvIndexPointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest index CID (relay + local). CN: 发布最新索引 CID（relay + 本地）。
export async function publishIndexPointer(
  account: string,
  ptr: ConvIndexPointer,
): Promise<void> {
  const prev = readLocalIndexPointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalIndexPointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "index_put",
    "index_ack",
    ptr,
    writeLocalIndexPointer,
    wsIndexFetch,
  );
}

/// EN: Fetch newest pointer (max of local + relay). CN: 取最新指针（本地与 relay 较大者）。
export async function fetchIndexPointer(account: string): Promise<ConvIndexPointer | null> {
  const local = readLocalIndexPointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsIndexFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsIndexFetch(account: string): Promise<ConvIndexPointer | null> {
  return relayOneShotFetch(account, { type: "index_fetch" }, (m, _requestId) => {
    if (
      m.type === "index_reply" &&
      typeof m.cid === "string" &&
      typeof m.updated_at === "number"
    ) {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "index_reply") return null;
    return undefined;
  });
}
