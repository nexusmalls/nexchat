// EN: On-chain multi-level commission reads (`commissionMultiLevel` pallet).
// CN: 链上多级佣金只读查询（`commissionMultiLevel` pallet）。

import type { MultiLevelMemberStats, MultiLevelPayoutRecord } from "@/earnings/types";
import { unwrapChainJson } from "@/mls/chainBytes";
import { canonicalAddress } from "@/wallet/address";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
};

export type MultiLevelApi = {
  query: {
    commissionMultiLevel?: Record<string, StorageQuery>;
  };
};

function parsePayoutRow(
  row: Record<string, unknown>,
  levelRates: number[],
): MultiLevelPayoutRecord {
  const level = Number(row.level ?? 0);
  const rateBps = level > 0 && levelRates[level - 1] != null ? levelRates[level - 1]! : null;
  return {
    buyer: String(row.buyer ?? ""),
    orderId: Number(row.orderId ?? row.order_id ?? 0),
    amount: String(row.amount ?? "0"),
    level,
    blockNumber: Number(row.blockNumber ?? row.block_number ?? 0),
    rateBps,
  };
}

function parseLevelRates(config: Record<string, unknown> | null): number[] {
  const levels = (config?.levels ?? []) as Record<string, unknown>[];
  return levels.map((tier) => Number(tier.rate ?? 0));
}

// EN: Multi-level tier rates for an entity (bps per level index).
// CN: Entity 多级各层费率（按层级索引，单位基点）。
export async function fetchMultiLevelRates(
  api: MultiLevelApi,
  entityId: number,
): Promise<number[]> {
  const q = api.query.commissionMultiLevel?.multiLevelConfig;
  if (!q) return [];
  const raw = await q(entityId);
  return parseLevelRates(unwrapChainJson(raw));
}

// EN: Member multi-level stats from storage.
// CN: 会员多级佣金统计（storage）。
export async function fetchMultiLevelMemberStats(
  api: MultiLevelApi,
  entityId: number,
  address: string,
): Promise<MultiLevelMemberStats | null> {
  const q = api.query.commissionMultiLevel?.memberMultiLevelStats;
  if (!q) return null;
  const who = canonicalAddress(address);
  const raw = await q(entityId, who);
  const data = unwrapChainJson(raw as Record<string, unknown>);
  if (!data) return null;
  return {
    totalEarned: String(data.totalEarned ?? data.total_earned ?? "0"),
    totalOrders: Number(data.totalOrders ?? data.total_orders ?? 0),
    lastCommissionBlock: Number(
      data.lastCommissionBlock ?? data.last_commission_block ?? 0,
    ),
  };
}

// EN: Member multi-level payout records (newest first).
// CN: 会员多级分佣记录（新到旧）。
export async function fetchMultiLevelPayouts(
  api: MultiLevelApi,
  entityId: number,
  address: string,
): Promise<MultiLevelPayoutRecord[]> {
  const q = api.query.commissionMultiLevel?.memberMultiLevelPayouts;
  if (!q) return [];
  const who = canonicalAddress(address);
  const [raw, levelRates] = await Promise.all([
    q(entityId, who),
    fetchMultiLevelRates(api, entityId),
  ]);
  const rows = ((raw as { toJSON?: () => unknown[] })?.toJSON?.() ?? []) as Record<
    string,
    unknown
  >[];
  return rows
    .map((row) => parsePayoutRow(row, levelRates))
    .reverse();
}

export function bpsToPercentLabel(bps: number | null | undefined): string | null {
  if (bps == null || !Number.isFinite(bps)) return null;
  return `${(bps / 100).toFixed(2)}%`;
}
