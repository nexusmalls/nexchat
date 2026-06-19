// EN: Cross-device PIN-wrapped MLS signing-key backup pointer (relay KV + localStorage fallback).
// Track A design §5.3 path C: pointer carries `{cid, updated_at}`; the blob is E2E-encrypted under
// K_pin_wrap (vault_master + PIN) on IPFS. Wire: `mls_signing_put/fetch/reply/ack`.
// CN: 跨设备 PIN 包裹 MLS 签名钥备份指针（relay KV + localStorage 兜底）。路线 A 设计 §5.3 路径 C：指针
// 携带 `{cid, updated_at}`；blob 在 IPFS 上由 K_pin_wrap（vault_master + PIN）端到端加密。Wire：
// `mls_signing_put/fetch/reply/ack`。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { SyncPointer } from "@/store/syncAnchor";

const lsKey = (account: string) => `nexchat:mls-signing:${account}`;

export function readLocalMlsSigningPointer(account: string): SyncPointer | null {
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

export function writeLocalMlsSigningPointer(account: string, ptr: SyncPointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(a: SyncPointer | null, b: SyncPointer | null): SyncPointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest signing-backup CID (relay + local), monotone by updated_at. CN: 发布最新签名备份
/// CID（relay + 本地），按 updated_at 单调。
export async function publishMlsSigningPointer(account: string, ptr: SyncPointer): Promise<void> {
  const prev = readLocalMlsSigningPointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalMlsSigningPointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "mls_signing_put",
    "mls_signing_ack",
    ptr,
    writeLocalMlsSigningPointer,
    wsMlsSigningFetch,
  );
}

/// EN: Fetch newest pointer (max of local + relay). CN: 取最新指针（本地与 relay 较大者）。
export async function fetchMlsSigningPointer(account: string): Promise<SyncPointer | null> {
  const local = readLocalMlsSigningPointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsMlsSigningFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsMlsSigningFetch(account: string): Promise<SyncPointer | null> {
  return relayOneShotFetch(account, { type: "mls_signing_fetch" }, (m, _requestId) => {
    if (
      m.type === "mls_signing_reply" &&
      typeof m.cid === "string" &&
      typeof m.updated_at === "number"
    ) {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "mls_signing_reply") return null;
    return undefined;
  });
}
