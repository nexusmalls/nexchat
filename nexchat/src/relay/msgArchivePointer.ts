// EN: Cross-device message-archive pointer channel (relay KV + localStorage fallback).
// CN: 跨设备消息归档指针通道（relay KV + localStorage 兜底）。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { MsgArchivePointer } from "@/store/msgArchive";

const lsKey = (account: string) => `nexchat:msg-archive:${account}`;

export function readLocalMsgArchivePointer(account: string): MsgArchivePointer | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return null;
    const p = JSON.parse(raw) as MsgArchivePointer;
    if (!p.cid || !p.updated_at) return null;
    return p;
  } catch {
    return null;
  }
}

export function writeLocalMsgArchivePointer(account: string, ptr: MsgArchivePointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(a: MsgArchivePointer | null, b: MsgArchivePointer | null): MsgArchivePointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest archive CID (relay + local). CN: 发布最新归档 CID（relay + 本地）。
export async function publishMsgArchivePointer(
  account: string,
  ptr: MsgArchivePointer,
): Promise<void> {
  const prev = readLocalMsgArchivePointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalMsgArchivePointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "msg_archive_put",
    "msg_archive_ack",
    ptr,
    writeLocalMsgArchivePointer,
    wsMsgArchiveFetch,
  );
}

/// EN: Fetch newest pointer (max of local + relay). CN: 取最新指针（本地与 relay 较大者）。
export async function fetchMsgArchivePointer(account: string): Promise<MsgArchivePointer | null> {
  const local = readLocalMsgArchivePointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsMsgArchiveFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsMsgArchiveFetch(account: string): Promise<MsgArchivePointer | null> {
  return relayOneShotFetch(account, { type: "msg_archive_fetch" }, (m, _requestId) => {
    if (
      m.type === "msg_archive_reply" &&
      typeof m.cid === "string" &&
      typeof m.updated_at === "number"
    ) {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "msg_archive_reply") return null;
    return undefined;
  });
}
