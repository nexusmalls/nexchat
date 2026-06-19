// EN: On-chain pool reward reads (`poolRewardDetailApi` + storage fallbacks).
// CN: 链上奖池领取只读查询（`poolRewardDetailApi` + storage 回退）。

import type {
  PoolRewardClaimRecord,
  PoolRewardLevelProgress,
  PoolRewardMemberView,
  PoolRewardRoundFunding,
} from "@/earnings/types";
import { unwrapChainJson } from "@/mls/chainBytes";
import { canonicalAddress } from "@/wallet/address";

type StorageQuery = {
  (...args: unknown[]): Promise<unknown>;
};

export type PoolRewardApi = {
  query: {
    commissionCore?: Record<string, StorageQuery>;
    commissionPoolReward?: Record<string, StorageQuery>;
  };
  call?: {
    poolRewardDetailApi?: {
      getPoolRewardMemberView: (
        entityId: number,
        account: string,
      ) => Promise<unknown>;
    };
  };
  rpc?: {
    chain?: {
      getBlock: () => Promise<{ block: { header: { number: { toNumber: () => number } } } }>;
    };
  };
};

function parseU128(value: unknown): string {
  return String(value ?? "0");
}

function parseMemberView(data: Record<string, unknown>): PoolRewardMemberView {
  const cap = (data.capInfo ?? data.cap_info ?? {}) as Record<string, unknown>;
  const levelProgress = (data.levelProgress ?? data.level_progress ?? []) as Record<
    string,
    unknown
  >[];
  const claimHistory = (data.claimHistory ?? data.claim_history ?? []) as Record<
    string,
    unknown
  >[];

  return {
    roundDuration: Number(data.roundDuration ?? data.round_duration ?? 0),
    tokenPoolEnabled: Boolean(data.tokenPoolEnabled ?? data.token_pool_enabled ?? false),
    currentRoundId: Number(data.currentRoundId ?? data.current_round_id ?? 0),
    roundStartBlock: Number(data.roundStartBlock ?? data.round_start_block ?? 0),
    roundEndBlock: Number(data.roundEndBlock ?? data.round_end_block ?? 0),
    poolSnapshot: parseU128(data.poolSnapshot ?? data.pool_snapshot),
    tokenPoolSnapshot:
      data.tokenPoolSnapshot != null || data.token_pool_snapshot != null
        ? parseU128(data.tokenPoolSnapshot ?? data.token_pool_snapshot)
        : null,
    effectiveLevel: Number(data.effectiveLevel ?? data.effective_level ?? 0),
    claimableNex: parseU128(data.claimableNex ?? data.claimable_nex),
    claimableToken: parseU128(data.claimableToken ?? data.claimable_token),
    alreadyClaimed: Boolean(data.alreadyClaimed ?? data.already_claimed ?? false),
    roundExpired: Boolean(data.roundExpired ?? data.round_expired ?? false),
    lastClaimedRound: Number(data.lastClaimedRound ?? data.last_claimed_round ?? 0),
    capInfo: {
      cumulativeClaimedUsdt: parseU128(
        cap.cumulativeClaimedUsdt ?? cap.cumulative_claimed_usdt,
      ),
      currentCapUsdt: parseU128(cap.currentCapUsdt ?? cap.current_cap_usdt),
      remainingCapUsdt: parseU128(cap.remainingCapUsdt ?? cap.remaining_cap_usdt),
      isCapped: Boolean(cap.isCapped ?? cap.is_capped ?? false),
      rateSnapshotUsed:
        cap.rateSnapshotUsed != null || cap.rate_snapshot_used != null
          ? Number(cap.rateSnapshotUsed ?? cap.rate_snapshot_used)
          : null,
      unlockPercent:
        cap.unlockPercent != null || cap.unlock_percent != null
          ? Number(cap.unlockPercent ?? cap.unlock_percent)
          : null,
    },
    levelProgress: levelProgress.map(
      (p): PoolRewardLevelProgress => ({
        levelId: Number(p.levelId ?? p.level_id ?? 0),
        ratioBps: Number(p.ratioBps ?? p.ratio_bps ?? 0),
        memberCount: Number(p.memberCount ?? p.member_count ?? 0),
        claimedCount: Number(p.claimedCount ?? p.claimed_count ?? 0),
        perMemberReward: parseU128(p.perMemberReward ?? p.per_member_reward),
      }),
    ),
    claimHistory: claimHistory.map(
      (c): PoolRewardClaimRecord => ({
        roundId: Number(c.roundId ?? c.round_id ?? 0),
        amount: parseU128(c.amount),
        tokenAmount: parseU128(c.tokenAmount ?? c.token_amount),
        levelId: Number(c.levelId ?? c.level_id ?? 0),
        claimedAt: Number(c.claimedAt ?? c.claimed_at ?? 0),
      }),
    ),
    isPaused: Boolean(data.isPaused ?? data.is_paused ?? false),
    hasPendingConfig: Boolean(data.hasPendingConfig ?? data.has_pending_config ?? false),
  };
}

// EN: Member pool-reward dashboard via runtime API.
// CN: 会员奖池详情（runtime API）。
export async function fetchPoolRewardMemberView(
  api: PoolRewardApi,
  entityId: number,
  address: string,
): Promise<PoolRewardMemberView | null> {
  const runtime = api.call?.poolRewardDetailApi?.getPoolRewardMemberView;
  if (!runtime) return null;
  const who = canonicalAddress(address);
  const raw = await runtime(entityId, who);
  if (raw && typeof raw === "object" && "isNone" in raw && (raw as { isNone?: boolean }).isNone) {
    return null;
  }
  const view = raw as { toJSON?: () => Record<string, unknown> } | Record<string, unknown>;
  const data = (typeof view.toJSON === "function" ? view.toJSON() : view) as Record<
    string,
    unknown
  > | null;
  if (!data || typeof data !== "object") return null;
  return parseMemberView(data);
}

// EN: Entity sediment pool balance (`commissionCore.unallocatedPool`).
// CN: Entity 沉淀池余额。
export async function fetchUnallocatedPool(
  api: PoolRewardApi,
  entityId: number,
): Promise<string> {
  const q = api.query.commissionCore?.unallocatedPool;
  if (!q) return "0";
  const raw = await q(entityId);
  return String((raw as { toJSON?: () => unknown })?.toJSON?.() ?? raw ?? "0");
}

// EN: Current round funding accumulator.
// CN: 当前轮次入账汇总。
export async function fetchCurrentRoundFunding(
  api: PoolRewardApi,
  entityId: number,
): Promise<PoolRewardRoundFunding> {
  const empty: PoolRewardRoundFunding = {
    nexCommissionRemainder: "0",
    tokenPlatformFeeRetention: "0",
    tokenCommissionRemainder: "0",
    nexCancelReturn: "0",
    totalFundingCount: 0,
  };
  const q = api.query.commissionPoolReward?.currentRoundFunding;
  if (!q) return empty;
  const raw = await q(entityId);
  const data = unwrapChainJson(raw);
  if (!data) return empty;
  return {
    nexCommissionRemainder: parseU128(
      data.nexCommissionRemainder ?? data.nex_commission_remainder,
    ),
    tokenPlatformFeeRetention: parseU128(
      data.tokenPlatformFeeRetention ?? data.token_platform_fee_retention,
    ),
    tokenCommissionRemainder: parseU128(
      data.tokenCommissionRemainder ?? data.token_commission_remainder,
    ),
    nexCancelReturn: parseU128(data.nexCancelReturn ?? data.nex_cancel_return),
    totalFundingCount: Number(data.totalFundingCount ?? data.total_funding_count ?? 0),
  };
}

// EN: Best block number for remaining-blocks countdown.
// CN: 当前最佳区块高度（用于剩余区块倒计时）。
export async function fetchCurrentBlock(api: PoolRewardApi): Promise<number> {
  const block = await api.rpc?.chain?.getBlock?.();
  return block?.block.header.number.toNumber() ?? 0;
}

export function canClaimPoolReward(view: PoolRewardMemberView | null): boolean {
  if (!view) return false;
  return (
    !view.alreadyClaimed &&
    !view.isPaused &&
    view.currentRoundId > 0 &&
    BigInt(view.claimableNex) > 0n
  );
}

export type PoolRewardIneligibleReason =
  | "paused"
  | "alreadyClaimed"
  | "roundExpired"
  | "noActiveRound"
  | "noClaim"
  | "quotaFull"
  | "levelNotInRound"
  | "noRateSnapshot"
  | "capExhausted"
  | "levelNotEligible";

export function poolRewardIneligibleReason(
  view: PoolRewardMemberView | null,
): PoolRewardIneligibleReason | null {
  if (!view || canClaimPoolReward(view)) return null;
  if (view.isPaused) return "paused";
  if (view.alreadyClaimed) return "alreadyClaimed";
  if (view.roundExpired) return "roundExpired";
  if (view.currentRoundId <= 0) return "noActiveRound";
  if (BigInt(view.claimableNex) > 0n) return null;

  const level = view.effectiveLevel;
  const prog = view.levelProgress.find((p) => p.levelId === level);
  if (!prog || prog.memberCount === 0) return "levelNotInRound";
  if (prog.claimedCount >= prog.memberCount) return "quotaFull";
  if (view.capInfo.rateSnapshotUsed == null) return "noRateSnapshot";
  if (BigInt(view.capInfo.remainingCapUsdt) === 0n) return "capExhausted";
  return "levelNotEligible";
}

export const POOL_REWARD_INELIGIBLE_LABELS: Record<PoolRewardIneligibleReason, string> = {
  paused: "已暂停",
  alreadyClaimed: "本轮已领取",
  roundExpired: "轮次已过期，等待新一轮",
  noActiveRound: "当前无活跃轮次",
  noClaim: "暂无可领",
  quotaFull: "本轮您所在等级的配额已全部领完",
  levelNotInRound: "您当前等级不在本轮奖池分配范围内",
  noRateSnapshot: "本轮汇率快照缺失，链上暂时无法计算您的奖励上限",
  capExhausted: "您的累计领取已达上限",
  levelNotEligible: "您当前等级不符合本轮奖池领取条件",
};

export function capProgressPercent(view: PoolRewardMemberView | null): number {
  if (!view) return 0;
  const current = BigInt(view.capInfo.currentCapUsdt);
  const claimed = BigInt(view.capInfo.cumulativeClaimedUsdt);
  if (current <= 0n) return 0;
  return Number((claimed * 10000n) / current) / 100;
}

export function formatBlocksToTime(blocks: number): string {
  const totalSeconds = blocks * 6;
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  if (hours > 0) return `约 ${hours} 小时 ${minutes} 分钟`;
  return `约 ${Math.max(1, minutes)} 分钟`;
}

export function formatRateSnapshot(rate: number | null): string {
  if (rate == null || rate === 0) return "-";
  const value = rate / 1_000_000;
  if (value >= 1) return value.toFixed(2);
  if (value >= 0.01) return value.toFixed(4);
  return value.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}
