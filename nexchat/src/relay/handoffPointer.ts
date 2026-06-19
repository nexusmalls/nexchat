// EN: Cross-device sending-authority handoff pointer channel (relay KV + localStorage fallback).
// Track A (design CHAT_MULTIDEVICE_MLS_SYNC §5.2). Content-agnostic reuse of the pointer wire shape:
// `cid` carries an opaque base64 HandoffReceipt envelope (`{receipt, sig}`), `updated_at` carries the
// monotone handoff `seq` (the authority counter). The relay never verifies the signature — that is the
// client's job (`handoffCoordinator`) — it only enforces account-writer auth + seq monotonicity. Same
// wire/monotonicity contract as the other pointer slots (`handoff_put`/`_fetch`/`_reply`/`_ack`).
// CN: 跨设备发送权交接指针通道（relay KV + localStorage 兜底）。路线 A（设计 §5.2）。内容无关复用指针
// 线格式：`cid` 承载不透明 base64 HandoffReceipt 信封（`{receipt, sig}`），`updated_at` 承载单调交接
// `seq`（权威计数）。relay 不验签（由客户端 `handoffCoordinator` 验），仅做账户写者鉴权 + seq 单调。
// wire/单调合同与其它指针槽一致。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { SyncPointer } from "@/store/syncAnchor";

const lsKey = (account: string) => `nexchat:handoff:${account}`;

export function readLocalHandoffPointer(account: string): SyncPointer | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return null;
    const p = JSON.parse(raw) as SyncPointer;
    if (!p.cid || !p.updated_at) return null;
    return p;
  } catch {
    return null;
  }
}

export function writeLocalHandoffPointer(account: string, ptr: SyncPointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(a: SyncPointer | null, b: SyncPointer | null): SyncPointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest handoff envelope (relay + local), monotone by seq (`updated_at`). CN: 发布最新
/// 交接信封（relay + 本地），按 seq（`updated_at`）单调。
export async function publishHandoffPointer(account: string, ptr: SyncPointer): Promise<void> {
  const prev = readLocalHandoffPointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalHandoffPointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "handoff_put",
    "handoff_ack",
    ptr,
    writeLocalHandoffPointer,
    wsHandoffFetch,
  );
}

/// EN: Fetch newest handoff envelope (max of local + relay). CN: 取最新交接信封（本地与 relay 较大者）。
export async function fetchHandoffPointer(account: string): Promise<SyncPointer | null> {
  const local = readLocalHandoffPointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsHandoffFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsHandoffFetch(account: string): Promise<SyncPointer | null> {
  return relayOneShotFetch(account, { type: "handoff_fetch" }, (m, _requestId) => {
    if (m.type === "handoff_reply" && typeof m.cid === "string" && typeof m.updated_at === "number") {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "handoff_reply") return null;
    return undefined;
  });
}
