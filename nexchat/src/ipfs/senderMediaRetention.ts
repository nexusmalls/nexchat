// EN: Sender-side local kubo pin registry with TTL — chat media is not globally pinned
// (ADR / CHAT_LARGE_FILE_SPEC §6). After upload, CIDs stay pinned on the sender's local
// node until `expiresAt`; a background sweep calls `ipfs pin rm` (best-effort). Ephemeral
// attachments skip local pin entirely. Chain pin remains opt-in via `VITE_IPFS_PIN_ENABLED`.
// CN: 发送方本机 kubo pin 登记 + TTL——聊天媒体不做全局 pin（ADR / 大文件规范 §6）。上传后
// CID 在本机节点保留至 `expiresAt`；后台清扫尽力 `ipfs pin rm`。阅后即焚附件完全跳过本机 pin。
// 链上 Pin 仍由 `VITE_IPFS_PIN_ENABLED` 可选开启。

import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import type { UploadedEncryptedFile } from "@/ipfs/media";

const STORAGE_KEY = "nexchat/sender-media-retention/v1";

export interface SenderMediaRetentionEntry {
  cid: string;
  expiresAt: number;
  clientMsgId?: string;
  convId?: string;
}

/// EN: All IPFS CIDs produced by one encrypted upload (root, thumb, chunks).
/// CN: 一次加密上传产生的全部 IPFS CID（根、缩略图、分块）。
export function collectCidsFromUpload(uploaded: UploadedEncryptedFile): string[] {
  const cids = [uploaded.rootCid];
  if (uploaded.thumbCid) cids.push(uploaded.thumbCid);
  if (uploaded.chunkCids) {
    for (const ch of uploaded.chunkCids) cids.push(ch.cid);
  }
  return [...new Set(cids)];
}

function loadEntries(): SenderMediaRetentionEntry[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (e): e is SenderMediaRetentionEntry =>
        typeof e === "object" &&
        e != null &&
        typeof (e as SenderMediaRetentionEntry).cid === "string" &&
        typeof (e as SenderMediaRetentionEntry).expiresAt === "number",
    );
  } catch {
    return [];
  }
}

function saveEntries(entries: SenderMediaRetentionEntry[]): void {
  if (typeof localStorage === "undefined") return;
  if (entries.length === 0) localStorage.removeItem(STORAGE_KEY);
  else localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
}

/// EN: Record uploaded media CIDs for TTL-based local unpin (no-op when local pin disabled).
/// CN: 登记已上传媒体 CID，供 TTL 到期后本机 unpin（本机 pin 关闭时为 no-op）。
export function registerUploadedMedia(
  uploaded: UploadedEncryptedFile,
  opts?: { ttlMs?: number; clientMsgId?: string; convId?: string },
): void {
  if (!config.ipfsMediaLocalPinEnabled) return;
  const ttlMs = opts?.ttlMs ?? config.ipfsMediaLocalPinTtlMs;
  if (ttlMs <= 0) return;

  const expiresAt = Date.now() + ttlMs;
  const byCid = new Map(loadEntries().map((e) => [e.cid, e]));
  for (const cid of collectCidsFromUpload(uploaded)) {
    byCid.set(cid, {
      cid,
      expiresAt,
      clientMsgId: opts?.clientMsgId,
      convId: opts?.convId,
    });
  }
  saveEntries([...byCid.values()]);
}

/// EN: Receiver acked full download (1:1) — shorten remaining TTL to a short grace so the
/// next sweep unpins early instead of waiting the full retention window.
/// CN: 接收方已确认下载（1:1）——把剩余 TTL 收短为宽限期，下次清扫即可提前 unpin。
export function shortenRetentionForMessage(
  convId: string,
  clientMsgId: string,
  graceMs = 60 * 60_000,
): void {
  const entries = loadEntries();
  const cap = Date.now() + graceMs;
  let changed = false;
  for (const e of entries) {
    if (e.convId === convId && e.clientMsgId === clientMsgId && e.expiresAt > cap) {
      e.expiresAt = cap;
      changed = true;
    }
  }
  if (changed) saveEntries(entries);
}

/// EN: User kept/starred the attachment — drop registry rows so the TTL sweep never
/// unpins them (local pin becomes permanent until manual cleanup).
/// CN: 用户收藏附件——移除登记行，TTL 清扫不再 unpin（本机 pin 长期保留）。
export function exemptRetentionForMessage(convId: string, clientMsgId: string): void {
  const entries = loadEntries();
  const keep = entries.filter((e) => !(e.convId === convId && e.clientMsgId === clientMsgId));
  if (keep.length !== entries.length) saveEntries(keep);
}

/// EN: Unpin expired local entries (best-effort; safe to call on unlock / idle).
/// CN: 对本机已过期条目尽力 unpin（解锁/空闲时调用即可）。
export async function runSenderMediaRetentionCleanup(now = Date.now()): Promise<number> {
  const entries = loadEntries();
  if (entries.length === 0) return 0;

  const keep: SenderMediaRetentionEntry[] = [];
  let removed = 0;
  for (const entry of entries) {
    if (entry.expiresAt <= now) {
      try {
        await ipfsClient.unpin(entry.cid);
        removed += 1;
      } catch (e) {
        console.warn("[nexchat] local media unpin failed:", entry.cid, e);
        // EN: drop stale registry row even if kubo already GC'd the block.
        // CN: kubo 已 GC 时也丢弃登记行，避免无限重试。
        removed += 1;
      }
    } else {
      keep.push(entry);
    }
  }
  saveEntries(keep);
  return removed;
}

let schedulerTimer: ReturnType<typeof setInterval> | null = null;

/// EN: Periodic retention sweep (immediate run + hourly interval; singleton).
/// CN: 周期性 retention 清扫（立即执行一次 + 每小时；单例）。
export function startSenderMediaRetentionScheduler(intervalMs = 60 * 60_000): () => void {
  if (schedulerTimer) clearInterval(schedulerTimer);
  const run = () =>
    void runSenderMediaRetentionCleanup().catch((e) => {
      console.warn("[nexchat] sender media retention cleanup failed:", e);
    });
  run();
  schedulerTimer = setInterval(run, intervalMs);
  return () => {
    if (schedulerTimer) clearInterval(schedulerTimer);
    schedulerTimer = null;
  };
}

export function listSenderMediaRetentionForTest(): SenderMediaRetentionEntry[] {
  return loadEntries();
}

export function clearSenderMediaRetentionForTest(): void {
  saveEntries([]);
}
