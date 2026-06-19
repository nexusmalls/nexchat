// EN: Per-account contact requests (localStorage). Inbound/outbound relay notifications.
// CN: 按账户的联系人请求（localStorage），对应 relay 入站/出站通知。

import { canonicalAddress } from "@/wallet/address";

const LS_PREFIX = "nexchat-contact-requests:";

export type ContactRequestStatus = "pending" | "accepted" | "rejected";
export type ContactRequestDirection = "inbound" | "outbound";

export interface ContactRequest {
  reqId: string;
  peerAddress: string;
  fromLabel: string;
  direction: ContactRequestDirection;
  status: ContactRequestStatus;
  sentAt: number;
  updatedAt: number;
}

function lsKey(account: string): string {
  return `${LS_PREFIX}${account}`;
}

export function loadContactRequests(account: string): ContactRequest[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(lsKey(account));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as ContactRequest[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function saveContactRequests(account: string, rows: ContactRequest[]): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(lsKey(account), JSON.stringify(rows));
}

export function pendingInboundCount(rows: readonly ContactRequest[]): number {
  return rows.filter((r) => r.direction === "inbound" && r.status === "pending").length;
}

export function findRequest(rows: readonly ContactRequest[], reqId: string): ContactRequest | undefined {
  return rows.find((r) => r.reqId === reqId);
}

export function upsertRequest(account: string, row: ContactRequest): ContactRequest[] {
  const rows = loadContactRequests(account);
  const i = rows.findIndex((r) => r.reqId === row.reqId);
  if (i >= 0) rows[i] = row;
  else rows.push(row);
  saveContactRequests(account, rows);
  return rows;
}

export function updateRequestStatus(
  account: string,
  reqId: string,
  status: ContactRequestStatus,
  updatedAt = Date.now(),
): ContactRequest[] {
  const rows = loadContactRequests(account);
  const i = rows.findIndex((r) => r.reqId === reqId);
  if (i < 0) return rows;
  rows[i] = { ...rows[i]!, status, updatedAt };
  saveContactRequests(account, rows);
  return rows;
}

/// EN: Drop stale outbound pending rows older than 30 days. CN: 清理 30 天前的出站 pending。
export function pruneStaleRequests(account: string, now = Date.now()): ContactRequest[] {
  const maxAge = 30 * 24 * 60 * 60 * 1000;
  const rows = loadContactRequests(account).filter((r) => {
    if (r.direction === "outbound" && r.status === "pending" && now - r.sentAt > maxAge) {
      return false;
    }
    return true;
  });
  saveContactRequests(account, rows);
  return rows;
}

export function hasSeenReqId(account: string, reqId: string): boolean {
  return loadContactRequests(account).some((r) => r.reqId === reqId);
}

export function peerCanon(peer: string): string {
  return canonicalAddress(peer);
}
