// EN: On-chain commission read queries (commissionCore + CommissionDashboardApi).
// CN: 链上佣金只读查询。

import type {
  CommissionDashboard,
  CommissionMemberStats,
  CommissionOverview,
  EarningEntityOption,
  RepurchaseConfig,
  WithdrawalRecord,
} from "@/earnings/types";
import { canonicalAddress } from "@/wallet/address";
import { decodeChainText, unwrapChainJson } from "@/mls/chainBytes";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
  entries?: () => Promise<unknown>;
};

export type CommissionApi = {
  query: {
    commissionCore: Record<string, StorageQuery>;
    entityRegistry?: Record<string, StorageQuery>;
  };
  call?: {
    commissionDashboardApi?: {
      getMemberCommissionDashboard: (
        entityId: number,
        account: string,
      ) => Promise<{
        isNone?: boolean;
        unwrap?: () => { toJSON: () => Record<string, unknown> };
        toJSON?: () => Record<string, unknown>;
      }>;
      getEntityCommissionOverview: (entityId: number) => Promise<{
        toJSON?: () => Record<string, unknown>;
      }>;
      getMemberWithdrawalRecords: (
        entityId: number,
        account: string,
      ) => Promise<{ toJSON?: () => unknown[] }>;
    };
  };
};

type RawOption = {
  isNone?: boolean;
  unwrap?: () => { toJSON: () => Record<string, unknown> };
  toJSON?: () => unknown;
};

function hasRuntimeApi(api: CommissionApi, name: keyof NonNullable<CommissionApi["call"]>): boolean {
  return !!api.call?.[name];
}

function bytesToString(raw: unknown): string {
  return decodeChainText(raw);
}

function parseMemberStats(data: Record<string, unknown>): CommissionMemberStats {
  return {
    totalEarned: String(data.totalEarned ?? data.total_earned ?? "0"),
    pending: String(data.pending ?? "0"),
    withdrawn: String(data.withdrawn ?? "0"),
    repurchased: String(data.repurchased ?? "0"),
    orderCount: Number(data.orderCount ?? data.order_count ?? 0),
  };
}

function parseDashboard(data: Record<string, unknown>): CommissionDashboard {
  const parseStats = (s: Record<string, unknown> | undefined): CommissionMemberStats =>
    parseMemberStats(s ?? {});

  const ml = data.multiLevelStats ?? data.multi_level_stats;
  const team = data.teamTier ?? data.team_tier;
  const sl = data.singleLine ?? data.single_line;
  const pool = data.poolReward ?? data.pool_reward;
  const ref = data.referral;

  return {
    nexStats: parseStats(
      (data.nexStats ?? data.nex_stats) as Record<string, unknown> | undefined,
    ),
    multiLevelStats: ml
      ? {
          totalEarned: String(
            (ml as Record<string, unknown>).totalEarned ??
              (ml as Record<string, unknown>).total_earned ??
              "0",
          ),
          totalOrders: Number(
            (ml as Record<string, unknown>).totalOrders ??
              (ml as Record<string, unknown>).total_orders ??
              0,
          ),
        }
      : null,
    teamTier: team
      ? {
          tierIndex: Number(
            (team as Record<string, unknown>).tierIndex ??
              (team as Record<string, unknown>).tier_index ??
              0,
          ),
          name: String((team as Record<string, unknown>).name ?? ""),
          rate: Number((team as Record<string, unknown>).rate ?? 0),
          totalEarned: String(
            (team as Record<string, unknown>).totalEarned ??
              (team as Record<string, unknown>).total_earned ??
              "0",
          ),
        }
      : null,
    singleLine: {
      position:
        (sl as Record<string, unknown> | undefined)?.position != null
          ? Number((sl as Record<string, unknown>).position)
          : null,
      isEnabled: Boolean(
        (sl as Record<string, unknown> | undefined)?.isEnabled ??
          (sl as Record<string, unknown> | undefined)?.is_enabled ??
          false,
      ),
    },
    poolReward: {
      claimableNex: String(
        (pool as Record<string, unknown> | undefined)?.claimableNex ??
          (pool as Record<string, unknown> | undefined)?.claimable_nex ??
          "0",
      ),
      currentRoundId: Number(
        (pool as Record<string, unknown> | undefined)?.currentRoundId ??
          (pool as Record<string, unknown> | undefined)?.current_round_id ??
          0,
      ),
      isPaused: Boolean(
        (pool as Record<string, unknown> | undefined)?.isPaused ??
          (pool as Record<string, unknown> | undefined)?.is_paused ??
          false,
      ),
    },
    referral: ref
      ? {
          totalEarned: String(
            (ref as Record<string, unknown>).totalEarned ??
              (ref as Record<string, unknown>).total_earned ??
              "0",
          ),
        }
      : null,
    multiLevelProgress: (
      (data.multiLevelProgress ?? data.multi_level_progress ?? []) as Record<string, unknown>[]
    ).map((p) => ({
      level: Number(p.level ?? 0),
      activated: Boolean(p.activated ?? false),
    })),
  };
}

// EN: Member NEX commission stats from storage.
// CN: 会员 NEX 佣金统计（storage）。
export async function fetchMemberCommissionStats(
  api: CommissionApi,
  entityId: number,
  address: string,
): Promise<CommissionMemberStats | null> {
  const q = api.query.commissionCore?.memberCommissionStats;
  if (!q) return null;
  const who = canonicalAddress(address);
  const raw = (await q(entityId, who)) as RawOption;
  if (raw?.isNone) return null;
  const data = unwrapChainJson(raw);
  if (!data) return null;
  return parseMemberStats(data);
}

// EN: Entity commission overview via runtime API.
// CN: Entity 佣金总览（runtime API）。
export async function fetchCommissionOverview(
  api: CommissionApi,
  entityId: number,
): Promise<CommissionOverview | null> {
  if (!hasRuntimeApi(api, "commissionDashboardApi")) return null;
  const raw = await api.call!.commissionDashboardApi!.getEntityCommissionOverview(entityId);
  const data = (raw?.toJSON?.() ?? raw) as Record<string, unknown> | null;
  if (!data) return null;
  const teamStatus = data.teamStatus ?? data.team_status ?? [false, false];
  return {
    enabledModes: Number(data.enabledModes ?? data.enabled_modes ?? 0),
    commissionRate: Number(data.commissionRate ?? data.commission_rate ?? 0),
    isEnabled: Boolean(data.isEnabled ?? data.is_enabled ?? false),
    multiLevelPaused: Boolean(data.multiLevelPaused ?? data.multi_level_paused ?? false),
    singleLineEnabled: Boolean(data.singleLineEnabled ?? data.single_line_enabled ?? false),
    teamStatus: Array.isArray(teamStatus)
      ? [Boolean(teamStatus[0]), Boolean(teamStatus[1])]
      : [false, false],
    poolRewardPaused: Boolean(data.poolRewardPaused ?? data.pool_reward_paused ?? false),
    withdrawalPaused: Boolean(data.withdrawalPaused ?? data.withdrawal_paused ?? false),
    unallocatedPoolNex: String(
      data.unallocatedPoolNex ?? data.unallocated_pool_nex ?? "0",
    ),
  };
}

// EN: Member commission dashboard via runtime API.
// CN: 会员佣金仪表盘（runtime API）。
export async function fetchCommissionDashboard(
  api: CommissionApi,
  entityId: number,
  address: string,
): Promise<CommissionDashboard | null> {
  if (!hasRuntimeApi(api, "commissionDashboardApi")) return null;
  const who = canonicalAddress(address);
  const raw = await api.call!.commissionDashboardApi!.getMemberCommissionDashboard(
    entityId,
    who,
  );
  if (raw?.isNone) return null;
  const data = unwrapChainJson(raw);
  if (!data) return null;
  return parseDashboard(data);
}

// EN: Withdrawal history for member.
// CN: 会员提现记录。
export async function fetchWithdrawalRecords(
  api: CommissionApi,
  entityId: number,
  address: string,
): Promise<WithdrawalRecord[]> {
  const who = canonicalAddress(address);
  if (hasRuntimeApi(api, "commissionDashboardApi")) {
    const raw = await api.call!.commissionDashboardApi!.getMemberWithdrawalRecords(
      entityId,
      who,
    );
    const rows = (raw?.toJSON?.() ?? raw ?? []) as Record<string, unknown>[];
    return rows.map((r) => ({
      totalAmount: String(r.totalAmount ?? r.total_amount ?? "0"),
      withdrawn: String(r.withdrawn ?? "0"),
      repurchased: String(r.repurchased ?? "0"),
      bonus: String(r.bonus ?? "0"),
      blockNumber: Number(r.blockNumber ?? r.block_number ?? 0),
    }));
  }

  const q = api.query.commissionCore?.memberWithdrawalHistory;
  if (!q) return [];
  const raw = (await q(entityId, who)) as { toJSON?: () => Record<string, unknown>[] };
  const rows = raw?.toJSON?.() ?? [];
  return rows.map((r) => ({
    totalAmount: String(r.totalAmount ?? r.total_amount ?? "0"),
    withdrawn: String(r.withdrawn ?? "0"),
    repurchased: String(r.repurchased ?? "0"),
    bonus: String(r.bonus ?? "0"),
    blockNumber: Number(r.blockNumber ?? r.block_number ?? 0),
  }));
}

// EN: Repurchase / shopping balance threshold config.
// CN: 复购与购物余额阈值配置。
export async function fetchRepurchaseConfig(
  api: CommissionApi,
  entityId: number,
): Promise<RepurchaseConfig | null> {
  const q = api.query.commissionCore?.repurchaseConfigs;
  if (!q) return null;
  const raw = (await q(entityId)) as RawOption;
  if (raw?.isNone) return null;
  const data = unwrapChainJson(raw);
  if (!data) return null;
  return {
    maxShoppingBalanceUsdt: String(
      data.maxShoppingBalanceUsdt ?? data.max_shopping_balance_usdt ?? "0",
    ),
  };
}

// EN: Resolve entity display name from registry.
// CN: 从 registry 解析 Entity 名称。
export async function fetchEntityName(
  api: CommissionApi,
  entityId: number,
): Promise<string> {
  const q = api.query.entityRegistry?.entities;
  if (!q) return `Entity #${entityId}`;
  const raw = (await q(entityId)) as RawOption;
  if (raw?.isNone) return `Entity #${entityId}`;
  const data = unwrapChainJson(raw);
  if (!data) return `Entity #${entityId}`;
  const name = bytesToString(data.name);
  return name.trim() || `Entity #${entityId}`;
}

// EN: Resolve earning entity options (catalog seeds + chain discovery).
// CN: 解析收益页 Entity 选项（目录种子 + 链上发现）。
export async function discoverEarningEntities(
  api: CommissionApi,
  address: string,
  seedEntityIds: number[] = [],
): Promise<EarningEntityOption[]> {
  const { discoverEarningEntityCandidates } = await import("@/earnings/entityDiscovery");
  return discoverEarningEntityCandidates(
    api as Parameters<typeof discoverEarningEntityCandidates>[0],
    address,
    seedEntityIds,
  );
}
