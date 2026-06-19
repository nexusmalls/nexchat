// EN: Cross-device MLS escrow-vault pointer channel (relay KV + localStorage fallback). Track A
// (design CHAT_MULTIDEVICE_MLS_SYNC §4/§13): the pointer carries only `{cid, updated_at}`; the blob
// behind the CID is the per-account MLS state vault, E2E-encrypted under K_mls_escrow and stored
// off-chain (IPFS) — never on the relay or chain in cleartext. Same wire/monotonicity contract as
// the other pointer slots (`mls_vault_put`/`mls_vault_fetch`/`mls_vault_reply`/`mls_vault_ack`).
// CN: 跨设备 MLS 托管 vault 指针通道（relay KV + localStorage 兜底）。路线 A（设计 §4/§13）：指针只
// 携带 `{cid, updated_at}`；CID 背后是按账户的 MLS 状态 vault，由 K_mls_escrow 端到端加密、存于链下
// （IPFS）——绝不以明文存在于 relay 或链上。wire/单调合同与其它指针槽一致。

import { config } from "@/config";
import { publishCloudPointer } from "@/relay/pointerPut";
import { relayOneShotFetch } from "@/relay/relayOneShot";
import type { SyncPointer } from "@/store/syncAnchor";

const lsKey = (account: string) => `nexchat:mls-vault:${account}`;

export function readLocalMlsVaultPointer(account: string): SyncPointer | null {
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

export function writeLocalMlsVaultPointer(account: string, ptr: SyncPointer): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(ptr));
}

function pickNewer(a: SyncPointer | null, b: SyncPointer | null): SyncPointer | null {
  if (!a) return b;
  if (!b) return a;
  return a.updated_at >= b.updated_at ? a : b;
}

/// EN: Publish latest MLS-vault CID (relay + local), monotone by updated_at. CN: 发布最新 MLS vault
/// CID（relay + 本地），按 updated_at 单调。
export async function publishMlsVaultPointer(account: string, ptr: SyncPointer): Promise<void> {
  const prev = readLocalMlsVaultPointer(account);
  if (prev && prev.updated_at > ptr.updated_at) return;
  writeLocalMlsVaultPointer(account, ptr);
  if (!config.relayWs) return;
  await publishCloudPointer(
    account,
    "mls_vault_put",
    "mls_vault_ack",
    ptr,
    writeLocalMlsVaultPointer,
    wsMlsVaultFetch,
  );
}

/// EN: Fetch newest pointer (max of local + relay). CN: 取最新指针（本地与 relay 较大者）。
export async function fetchMlsVaultPointer(account: string): Promise<SyncPointer | null> {
  const local = readLocalMlsVaultPointer(account);
  if (!config.relayWs) return local;
  try {
    const remote = await wsMlsVaultFetch(account);
    return pickNewer(local, remote);
  } catch {
    return local;
  }
}

function wsMlsVaultFetch(account: string): Promise<SyncPointer | null> {
  return relayOneShotFetch(account, { type: "mls_vault_fetch" }, (m, _requestId) => {
    if (
      m.type === "mls_vault_reply" &&
      typeof m.cid === "string" &&
      typeof m.updated_at === "number"
    ) {
      return { cid: m.cid, updated_at: m.updated_at };
    }
    if (m.type === "mls_vault_reply") return null;
    return undefined;
  });
}
