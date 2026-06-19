// EN: Discover entities relevant to commission earnings (catalog seeds + chain scan).
// CN: 发现与佣金收益相关的 Entity（目录种子 + 链上扫描）。

import { unwrapChainJson } from "@/mls/chainBytes";

import {
  fetchMemberCommissionStats,
  fetchEntityName,
  type CommissionApi,
} from "@/earnings/commissionQueries";
import type { CommissionMemberStats, EarningEntityOption } from "@/earnings/types";
import { fetchEntityMember } from "@/shop/entityMemberQueries";
import type { EntityMemberApi } from "@/shop/entityMemberQueries";
import { canonicalAddress } from "@/wallet/address";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
  entries?: () => Promise<unknown>;
};

export type EntityDiscoveryApi = CommissionApi &
  EntityMemberApi & {
    query: CommissionApi["query"] & {
      entityRegistry?: Record<string, StorageQuery>;
    };
  };

const MEMBERSHIP_BATCH = 12;

function parseChainEnum(raw: unknown, fallback: string): string {
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object") {
    const key = Object.keys(raw)[0];
    if (key) return parseChainEnum(key, fallback);
  }
  return fallback;
}

type RawOption = {
  isNone?: boolean;
  unwrap?: () => { toJSON: () => Record<string, unknown> };
};

function parseDoubleMapKey(key: unknown): [number, string] | null {
  const json =
    (key as { toJSON?: () => unknown })?.toJSON?.() ??
    (Array.isArray(key) ? key : null);
  if (!Array.isArray(json) || json.length < 2) return null;
  const entityId = Number(json[0]);
  const account = String(json[1] ?? "");
  if (!Number.isFinite(entityId) || entityId <= 0 || !account) return null;
  return [entityId, account];
}

function parseStatsFromEntry(raw: unknown): CommissionMemberStats | null {
  const opt = raw as RawOption;
  if (opt?.isNone) return null;
  const data = unwrapChainJson(raw);
  if (!data) return null;
  const row = data as Record<string, unknown>;
  return {
    totalEarned: String(row.totalEarned ?? row.total_earned ?? "0"),
    pending: String(row.pending ?? "0"),
    withdrawn: String(row.withdrawn ?? "0"),
    repurchased: String(row.repurchased ?? "0"),
    orderCount: Number(row.orderCount ?? row.order_count ?? 0),
  };
}

// EN: True when member has any recorded commission activity.
// CN: 会员存在任意佣金记录时返回 true。
export function hasCommissionActivity(stats: CommissionMemberStats | null | undefined): boolean {
  if (!stats) return false;
  return (
    BigInt(stats.totalEarned) > 0n ||
    BigInt(stats.pending) > 0n ||
    BigInt(stats.withdrawn) > 0n ||
    BigInt(stats.repurchased) > 0n ||
    stats.orderCount > 0
  );
}

// EN: Merge and dedupe positive entity ids.
// CN: 合并并去重有效的 entity id。
export function mergeEntityIds(...lists: number[][]): number[] {
  const set = new Set<number>();
  for (const list of lists) {
    for (const id of list) {
      if (id > 0) set.add(id);
    }
  }
  return [...set].sort((a, b) => a - b);
}

async function runBatched<T, R>(
  items: T[],
  batchSize: number,
  fn: (item: T) => Promise<R>,
): Promise<R[]> {
  const out: R[] = [];
  for (let i = 0; i < items.length; i += batchSize) {
    const chunk = items.slice(i, i + batchSize);
    const chunkResults = await Promise.all(chunk.map(fn));
    out.push(...chunkResults);
  }
  return out;
}

// EN: Active entity ids from `entityRegistry.entities`.
// CN: 从 registry 读取活跃 Entity id。
export async function fetchActiveRegistryEntityIds(api: EntityDiscoveryApi): Promise<number[]> {
  const q = api.query.entityRegistry?.entities;
  if (!q?.entries) return [];
  const entries = (await q.entries()) as Array<[unknown, RawOption]>;
  const ids: number[] = [];
  for (const [, raw] of entries) {
    if (raw?.isNone) continue;
    const data = unwrapChainJson(raw);
    if (!data) continue;
    const status = parseChainEnum(data.status, "Active");
    if (status !== "Active") continue;
    const id = Number(data.id ?? 0);
    if (id > 0) ids.push(id);
  }
  return ids;
}

// EN: Entity ids with commission stats for `address` (storage entries scan).
// CN: 扫描佣金 storage，找出当前地址有记录的 Entity。
export async function discoverCommissionEntityIds(
  api: EntityDiscoveryApi,
  address: string,
): Promise<number[]> {
  const who = canonicalAddress(address);
  const ids = new Set<number>();
  const queries = [
    api.query.commissionCore?.memberCommissionStats,
    api.query.commissionCore?.memberShoppingCommissionStats,
  ];

  for (const q of queries) {
    if (!q?.entries) continue;
    const entries = (await q.entries()) as Array<[unknown, unknown]>;
    for (const [key, raw] of entries) {
      const parsed = parseDoubleMapKey(key);
      if (!parsed) continue;
      const [entityId, account] = parsed;
      if (canonicalAddress(account) !== who) continue;
      const stats = parseStatsFromEntry(raw);
      if (hasCommissionActivity(stats)) ids.add(entityId);
    }
  }

  return [...ids].sort((a, b) => a - b);
}

// EN: Among `entityIds`, return those where `address` is a registry member.
// CN: 在给定 entity 列表中筛选当前地址为会员的项。
export async function discoverMemberEntityIds(
  api: EntityDiscoveryApi,
  address: string,
  entityIds: number[],
): Promise<number[]> {
  const unique = [...new Set(entityIds)].filter((id) => id > 0);
  if (unique.length === 0) return [];

  const hits = await runBatched(unique, MEMBERSHIP_BATCH, async (entityId) => {
    const member = await fetchEntityMember(api, entityId, address);
    return member ? entityId : null;
  });

  return hits.filter((id): id is number => id != null).sort((a, b) => a - b);
}

// EN: Full earning-entity discovery: seeds + commission scan + active registry membership.
// CN: 完整收益 Entity 发现：种子 + 佣金扫描 + registry 活跃会员。
export async function discoverEarningEntityCandidates(
  api: EntityDiscoveryApi,
  address: string,
  seedEntityIds: number[] = [],
): Promise<EarningEntityOption[]> {
  const who = canonicalAddress(address);
  const [commissionIds, activeIds] = await Promise.all([
    discoverCommissionEntityIds(api, who),
    fetchActiveRegistryEntityIds(api),
  ]);

  const seeds = mergeEntityIds(seedEntityIds, commissionIds);
  const seedSet = new Set(seeds);
  const membershipProbeIds = activeIds.filter((id) => !seedSet.has(id));
  const memberIds = await discoverMemberEntityIds(api, who, membershipProbeIds);
  const allIds = mergeEntityIds(seeds, memberIds);

  const options = await runBatched(allIds, MEMBERSHIP_BATCH, async (entityId) => {
    const [name, stats] = await Promise.all([
      fetchEntityName(api, entityId),
      fetchMemberCommissionStats(api, entityId, who),
    ]);
    return {
      entityId,
      name,
      pending: stats?.pending ?? "0",
    } satisfies EarningEntityOption;
  });

  return options.sort((a, b) => {
    const pa = BigInt(a.pending ?? "0");
    const pb = BigInt(b.pending ?? "0");
    if (pb !== pa) return pb > pa ? 1 : -1;
    return a.entityId - b.entityId;
  });
}
