// EN: Self-healing audit trail for the data-layer login restore (ADR CHAT_SYNC_ANCHOR
// §6.2/§6.3/§6.5). Each `OffchainSyncCoordinator.restore()` appends ONE structured record so
// ops/support can replay "what did self-healing do for this account on this login": per-field
// winning source (local/relay/chain), chain injections, relay write-back results, the epoch-
// bump signal and timing. Persisted as a per-account ring buffer in localStorage; best-effort
// and never throws (auditing must not break restore).
// CN: 数据层登录自愈的审计轨迹（ADR CHAT_SYNC_ANCHOR §6.2/§6.3/§6.5）。每次
// `OffchainSyncCoordinator.restore()` 追加一条结构化记录，便于运维/客服复盘“某账户某次登录
// 自愈做了什么”：每字段胜出来源（local/relay/chain）、链上注入、relay 写回结果、重放窗口
// 提示与耗时。以按账户环形缓冲持久化于 localStorage；尽力而为、绝不抛错（审计不得破坏恢复）。

import type { OffchainSyncPhase } from "@/store/offchainSync";
import type { SyncAnchorTier, SyncField } from "@/store/offchainSyncCoordinator";

/// EN: Per-field self-healing outcome. CN: 单字段自愈结果。
export interface SyncFieldAudit {
  field: SyncField;
  /// EN: winning source for this field's effective pointer. CN: 该字段有效指针的胜出来源。
  source: "chain" | "relay" | "local" | "none";
  /// EN: chain anchor was strictly newer and got injected (§6.2). CN: 链锚严格更新并被注入。
  chainInjected: boolean;
  /// EN: relay write-back outcome (§6.3). CN: relay 写回结果。
  writeBack: "skip" | "ok" | "fail";
  /// EN: effective pointer `updated_at` after restore (0 = none). CN: 恢复后有效指针时间戳。
  effectiveUpdatedAt: number;
}

/// EN: One self-healing audit record (one restore run). CN: 一条自愈审计记录（一次恢复）。
export interface SyncAuditRecord {
  at: number;
  account: string;
  tier: SyncAnchorTier;
  phase: OffchainSyncPhase;
  usedChainAnchor: boolean;
  needsEpochBump: boolean;
  durationMs: number;
  restored: {
    contacts: boolean | null;
    convIndex: boolean | null;
    msgArchive: boolean | null;
  };
  fields: SyncFieldAudit[];
  message?: string;
}

const MAX_RECORDS = 50;

const auditKey = (account: string) => `nexchat:sync-audit:${account}`;

/// EN: Append one record to the account's ring buffer (oldest evicted past MAX_RECORDS).
/// Never throws. CN: 向账户环形缓冲追加一条（超 MAX_RECORDS 淘汰最旧）。绝不抛错。
export function appendSyncAudit(record: SyncAuditRecord): void {
  if (typeof localStorage === "undefined" || !record.account) return;
  try {
    const prev = readSyncAudit(record.account);
    prev.push(record);
    const trimmed = prev.slice(-MAX_RECORDS);
    localStorage.setItem(auditKey(record.account), JSON.stringify(trimmed));
  } catch (e) {
    console.warn("[nexchat] sync-audit append failed:", e);
  }
}

/// EN: Read an account's audit records (oldest→newest); [] on missing/corrupt.
/// CN: 读取账户审计记录（旧→新）；缺失/损坏返回 []。
export function readSyncAudit(account: string): SyncAuditRecord[] {
  if (typeof localStorage === "undefined" || !account) return [];
  try {
    const raw = localStorage.getItem(auditKey(account));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? (parsed as SyncAuditRecord[]) : [];
  } catch {
    return [];
  }
}

/// EN: Drop an account's audit trail. CN: 清空账户审计轨迹。
export function clearSyncAudit(account: string): void {
  if (typeof localStorage === "undefined" || !account) return;
  try {
    localStorage.removeItem(auditKey(account));
  } catch {
    /* ignore */
  }
}
