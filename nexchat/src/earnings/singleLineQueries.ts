// EN: On-chain single-line (win-win) commission reads.
// CN: 链上单线排网（共赢）佣金只读查询。

import type {
  SingleLineDirection,
  SingleLineMemberStats,
  SingleLinePayoutRecord,
} from "@/earnings/types";
import { unwrapChainJson } from "@/mls/chainBytes";
import { canonicalAddress } from "@/wallet/address";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
};

export type SingleLineApi = {
  query: {
    commissionSingleLine?: Record<string, StorageQuery>;
    entityTransaction?: Record<string, StorageQuery>;
  };
  call?: {
    singleLineQueryApi?: {
      singleLineMemberView: (
        entityId: number,
        account: string,
      ) => Promise<{ isNone?: boolean; toJSON?: () => Record<string, unknown> } | Record<string, unknown>>;
    };
  };
};

function parseDirection(raw: unknown): SingleLineDirection {
  if (raw === 1 || raw === "1" || raw === "Downline") return "downline";
  if (typeof raw === "string") {
    return raw.toLowerCase().includes("down") ? "downline" : "upline";
  }
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw as object)[0] ?? "";
    return key.toLowerCase().includes("down") ? "downline" : "upline";
  }
  return "upline";
}

function parseSummary(data: Record<string, unknown>): SingleLineMemberStats {
  return {
    totalEarnedAsUpline: String(
      data.totalEarnedAsUpline ?? data.total_earned_as_upline ?? "0",
    ),
    totalEarnedAsDownline: String(
      data.totalEarnedAsDownline ?? data.total_earned_as_downline ?? "0",
    ),
    totalPayoutCount: Number(data.totalPayoutCount ?? data.total_payout_count ?? 0),
    lastPayoutBlock: Number(data.lastPayoutBlock ?? data.last_payout_block ?? 0),
  };
}

function parsePayoutRow(row: Record<string, unknown>): SingleLinePayoutRecord {
  return {
    orderId: Number(row.orderId ?? row.order_id ?? 0),
    buyer: String(row.buyer ?? ""),
    amount: String(row.amount ?? "0"),
    direction: parseDirection(row.direction),
    levelDistance: Number(row.levelDistance ?? row.level_distance ?? 0),
    blockNumber: Number(row.blockNumber ?? row.block_number ?? 0),
    shopId: null,
  };
}

async function enrichShopIds(
  api: SingleLineApi,
  records: SingleLinePayoutRecord[],
): Promise<SingleLinePayoutRecord[]> {
  const q = api.query.entityTransaction?.orders;
  if (!q || records.length === 0) return records;
  const uniqueIds = [...new Set(records.map((r) => r.orderId).filter((id) => id > 0))];
  const shopByOrder = new Map<number, number>();
  await Promise.all(
    uniqueIds.map(async (orderId) => {
      try {
        const raw = await q(orderId);
        const data = unwrapChainJson(raw);
        if (data) {
          const shopId = Number(data.shopId ?? data.shop_id ?? 0);
          if (shopId > 0) shopByOrder.set(orderId, shopId);
        }
      } catch {
        /* optional enrichment */
      }
    }),
  );
  return records.map((r) => ({
    ...r,
    shopId: shopByOrder.get(r.orderId) ?? null,
  }));
}

function parseMemberViewPayload(raw: Record<string, unknown>): {
  stats: SingleLineMemberStats;
  records: SingleLinePayoutRecord[];
} {
  const summary = parseSummary(
    (raw.summary ?? {}) as Record<string, unknown>,
  );
  const payouts = (raw.recentPayouts ?? raw.recent_payouts ?? []) as Record<string, unknown>[];
  const records = payouts.map(parsePayoutRow).reverse();
  return { stats: summary, records };
}

async function fetchFromStorage(
  api: SingleLineApi,
  entityId: number,
  address: string,
): Promise<{ stats: SingleLineMemberStats | null; records: SingleLinePayoutRecord[] }> {
  const who = canonicalAddress(address);
  const statsQ = api.query.commissionSingleLine?.memberSingleLineStats;
  const payoutsQ = api.query.commissionSingleLine?.memberSingleLinePayouts;
  if (!statsQ && !payoutsQ) return { stats: null, records: [] };

  const [statsRaw, payoutsRaw] = await Promise.all([
    statsQ ? statsQ(entityId, who) : Promise.resolve(null),
    payoutsQ ? payoutsQ(entityId, who) : Promise.resolve(null),
  ]);

  const statsData = statsRaw ? unwrapChainJson(statsRaw) : null;
  const stats = statsData ? parseSummary(statsData) : null;
  const rows = payoutsRaw
    ? (((payoutsRaw as { toJSON?: () => unknown[] }).toJSON?.() ?? []) as Record<string, unknown>[])
    : [];
  const records = rows.map(parsePayoutRow).reverse();
  return { stats, records };
}

export function singleLineTotalEarned(stats: SingleLineMemberStats | null): string {
  if (!stats) return "0";
  return (
    BigInt(stats.totalEarnedAsUpline) + BigInt(stats.totalEarnedAsDownline)
  ).toString();
}

export function directionLabel(side: "upline" | "downline", levelDistance: number): string {
  const label = side === "downline" ? "下层" : "上层";
  return levelDistance > 0 ? `${label} ${levelDistance} 级` : label;
}

// EN: Chain payout direction → user queue perspective (matches nexus-com-dapp).
// CN: 链上分佣方向 → 用户公排队列视角（对齐 nexus-com-dapp）。
// User「上层」= 买家在我前面（我作为下线收佣）= chain Downline.
export function chainDirectionToUserSide(
  direction: SingleLineDirection,
): "upline" | "downline" {
  return direction === "downline" ? "upline" : "downline";
}

export function userSideLabel(direction: SingleLineDirection, levelDistance: number): string {
  return directionLabel(chainDirectionToUserSide(direction), levelDistance);
}

export function userUplineTotal(stats: SingleLineMemberStats | null): string {
  return stats?.totalEarnedAsDownline ?? "0";
}

export function userDownlineTotal(stats: SingleLineMemberStats | null): string {
  return stats?.totalEarnedAsUpline ?? "0";
}

export function filterSingleLineRecords(
  records: SingleLinePayoutRecord[],
  filter: "all" | "upline" | "downline",
): SingleLinePayoutRecord[] {
  if (filter === "all") return records;
  return records.filter((r) => chainDirectionToUserSide(r.direction) === filter);
}

export function sumSingleLineAmount(records: SingleLinePayoutRecord[]): string {
  return records.reduce((acc, r) => acc + BigInt(r.amount), 0n).toString();
}

// EN: Member single-line stats + payout records (newest first).
// CN: 会员单线排网统计与分佣记录（新到旧）。
export async function fetchSingleLineEarnings(
  api: SingleLineApi,
  entityId: number,
  address: string,
): Promise<{ stats: SingleLineMemberStats | null; records: SingleLinePayoutRecord[] }> {
  const who = canonicalAddress(address);
  const runtime = api.call?.singleLineQueryApi?.singleLineMemberView;

  if (runtime) {
    try {
      const raw = await runtime(entityId, who);
      if (raw && "isNone" in raw && raw.isNone) {
        return { stats: null, records: [] };
      }
      const view = raw as { toJSON?: () => Record<string, unknown> } | Record<string, unknown>;
      const data = (typeof view.toJSON === "function" ? view.toJSON() : view) as Record<
        string,
        unknown
      > | null;
      if (data && typeof data === "object") {
        const parsed = parseMemberViewPayload(data);
        const records = await enrichShopIds(api, parsed.records);
        return { stats: parsed.stats, records };
      }
    } catch {
      /* fall through to storage */
    }
  }

  const fromStorage = await fetchFromStorage(api, entityId, address);
  const records = await enrichShopIds(api, fromStorage.records);
  return { stats: fromStorage.stats, records };
}
