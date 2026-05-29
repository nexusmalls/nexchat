/**
 * Shared helpers for staking reward “unclaimed era / page” detection (pallet-staking ~SDK 45).
 * 与 `validator-staking-audit` / payout 脚本共用的未领取 era 检测逻辑。
 */

import type { ApiPromise } from '@polkadot/api';

import { codecToJson, readObjectField, coerceNumber } from './codec.js';
import { asBigInt } from './units.js';

export interface UnclaimedEraRow {
  era: number;
  path: 'paged' | 'legacy';
  exposureTotalPlanck: string;
  pageCount: number;
  claimedPages: number[];
  missingPages: number[];
}

export interface StakingRewardPreviewRecipient {
  stash: string;
  role: 'validator' | 'nominator';
  stakePlanck: string;
  estimatedRewardPlanck: string | null;
}

export interface StakingEraPagePreview {
  stash: string;
  era: number;
  page: number;
  path: 'paged' | 'legacy';
  withinPayoutWindow: boolean;
  pageCount: number;
  claimedPages: number[];
  isClaimed: boolean;
  exposureTotalPlanck: string;
  pageExposureTotalPlanck: string;
  validatorOwnStakePlanck: string;
  validatorCommissionPerbill: number | null;
  validatorEraRewardPlanck: string | null;
  validatorEraPoints: number | null;
  totalEraPoints: number | null;
  estimatedTotalRewardPlanck: string | null;
  estimatedSharedRewardPlanck: string | null;
  estimatedCommissionRewardPlanck: string | null;
  recipients: StakingRewardPreviewRecipient[];
  computable: boolean;
  reason: string | null;
}

export function bigFromHexOrDec(v: unknown): bigint {
  if (v == null) {
    return 0n;
  }
  if (typeof v === 'string') {
    const s = v.trim();
    if (s.startsWith('0x') || s.startsWith('0X')) {
      try {
        return BigInt(s);
      } catch {
        return 0n;
      }
    }
    return asBigInt(s.replace(/,/g, ''));
  }
  return asBigInt(v);
}

/**
 * Mirrors `EraInfo::get_page_count` when `ErasStakersOverview` exists.
 */
export function pageCountFromOverviewJson(ov: Record<string, unknown>): number {
  const own = bigFromHexOrDec(readObjectField(ov, 'own'));
  const pc = coerceNumber(readObjectField(ov, 'pageCount', 'page_count')) ?? 0;
  if (pc === 0 && own > 0n) {
    return 1;
  }
  return pc;
}

export function readLegacyClaimed(ledgerJson: Record<string, unknown> | null): Set<number> {
  if (!ledgerJson) {
    return new Set();
  }
  const raw = readObjectField(ledgerJson, 'legacyClaimedRewards', 'legacy_claimed_rewards');
  if (!Array.isArray(raw)) {
    return new Set();
  }
  return new Set(raw.map((x) => coerceNumber(x) ?? Number(x)).filter((n) => Number.isFinite(n)));
}

export function buildPayoutEraRange(activeEraIndex: number, historyDepth: number): number[] {
  const eraStart = Math.max(0, activeEraIndex - historyDepth);
  const eraEndInclusive = activeEraIndex - 1;
  const payoutEras: number[] = [];
  for (let e = eraStart; e <= eraEndInclusive; e++) {
    payoutEras.push(e);
  }
  return payoutEras;
}

function decodeAccountIdLike(value: unknown): string {
  if (value == null) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  if (typeof value === 'object' && value !== null && 'Id' in (value as object)) {
    return String((value as { Id?: unknown }).Id ?? '');
  }
  return String(value);
}

function perbillMulFloor(amount: bigint, perbill: bigint): bigint {
  return (amount * perbill) / 1_000_000_000n;
}

function ratioMulFloor(amount: bigint, numerator: bigint, denominator: bigint): bigint {
  if (denominator <= 0n || numerator <= 0n || amount <= 0n) {
    return 0n;
  }
  return (amount * numerator) / denominator;
}

function decodePagedExposureNominators(raw: unknown): Array<{ stash: string; stakePlanck: bigint }> {
  const decoded = codecToJson(raw) as unknown;
  if (decoded == null) {
    return [];
  }

  const directArray = Array.isArray(decoded)
    ? decoded
    : (readObjectField(decoded, 'others', 'nominators', 'individualExposure') as unknown[] | undefined) ?? [];

  if (!Array.isArray(directArray)) {
    return [];
  }

  return directArray
    .map((entry) => {
      if (entry == null || typeof entry !== 'object') {
        return null;
      }
      const stash = decodeAccountIdLike(readObjectField(entry, 'who', 'stash', 'account', 'nominator'));
      const stakePlanck = bigFromHexOrDec(readObjectField(entry, 'value', 'stake', 'amount'));
      return stash ? { stash, stakePlanck } : null;
    })
    .filter((entry): entry is { stash: string; stakePlanck: bigint } => entry != null);
}

function readPagedExposureOwn(raw: unknown): bigint {
  const decoded = codecToJson(raw) as unknown;
  if (decoded == null || typeof decoded !== 'object' || Array.isArray(decoded)) {
    return 0n;
  }
  return bigFromHexOrDec(readObjectField(decoded, 'own', 'validator', 'validatorStake'));
}

function buildPagedRecipients(
  validatorStash: string,
  overviewOwn: bigint,
  raw: unknown,
): { validatorOwn: bigint; nominators: Array<{ stash: string; stakePlanck: bigint }>; pageTotal: bigint } {
  const pageTotal = readPagedExposureTotal(raw);
  const nominators = decodePagedExposureNominators(raw);
  let validatorOwn = readPagedExposureOwn(raw);

  if (validatorOwn === 0n && pageTotal > 0n && overviewOwn > 0n) {
    validatorOwn = overviewOwn;
  }

  if (
    validatorOwn > 0n &&
    !nominators.some((row) => row.stash === validatorStash && row.stakePlanck === validatorOwn)
  ) {
    return { validatorOwn, nominators, pageTotal: pageTotal > 0n ? pageTotal + validatorOwn : validatorOwn + nominators.reduce((sum, row) => sum + row.stakePlanck, 0n) };
  }

  return {
    validatorOwn,
    nominators,
    pageTotal: pageTotal > 0n ? pageTotal + validatorOwn : validatorOwn + nominators.reduce((sum, row) => sum + row.stakePlanck, 0n),
  };
}

function readPagedExposureTotal(raw: unknown): bigint {
  const decoded = codecToJson(raw) as unknown;
  if (decoded == null || typeof decoded !== 'object' || Array.isArray(decoded)) {
    return 0n;
  }
  return bigFromHexOrDec(readObjectField(decoded, 'total', 'pageTotal', 'exposureTotal'));
}

function decodeRewardPoints(raw: unknown, stash: string): { validatorPoints: bigint; totalPoints: bigint } {
  const decoded = codecToJson(raw) as unknown;
  if (decoded == null || typeof decoded !== 'object' || Array.isArray(decoded)) {
    return { validatorPoints: 0n, totalPoints: 0n };
  }

  const totalPoints = bigFromHexOrDec(readObjectField(decoded, 'total'));
  const individual = readObjectField(decoded, 'individual');
  if (individual == null) {
    return { validatorPoints: 0n, totalPoints };
  }

  if (Array.isArray(individual)) {
    for (const row of individual) {
      if (!Array.isArray(row) || row.length < 2) {
        continue;
      }
      if (decodeAccountIdLike(row[0]) === stash) {
        return { validatorPoints: bigFromHexOrDec(row[1]), totalPoints };
      }
    }
    return { validatorPoints: 0n, totalPoints };
  }

  if (typeof individual === 'object') {
    const value = readObjectField(individual, stash);
    return { validatorPoints: bigFromHexOrDec(value), totalPoints };
  }

  return { validatorPoints: 0n, totalPoints };
}

export async function getStakingEraPagePreview(
  api: ApiPromise,
  stash: string,
  ledgerJson: Record<string, unknown> | null,
  era: number,
  page: number,
  payoutEras?: number[],
): Promise<StakingEraPagePreview> {
  const staking = api.query.staking as any;
  const overviewCodec = await staking.erasStakersOverview(era, stash);
  const overviewJson = overviewCodec.isEmpty ? null : (codecToJson(overviewCodec) as Record<string, unknown>);
  const esCodec = await staking.erasStakers(era, stash);
  const esJson = esCodec.isEmpty ? null : (codecToJson(esCodec) as Record<string, unknown>);

  const ovTotal = overviewJson ? bigFromHexOrDec(readObjectField(overviewJson, 'total')) : 0n;
  const esTotal = esJson ? bigFromHexOrDec(readObjectField(esJson, 'total')) : 0n;
  const path: 'paged' | 'legacy' = overviewJson && ovTotal > 0n ? 'paged' : 'legacy';
  const pageCount = path === 'paged' ? pageCountFromOverviewJson(overviewJson ?? {}) : esJson && esTotal > 0n ? 1 : 0;
  const claimedPages =
    path === 'paged'
      ? (((codecToJson(await staking.claimedRewards(era, stash)) as unknown[] | null) ?? [])
          .map((x) => coerceNumber(x) ?? Number(x))
          .filter((n) => Number.isFinite(n))
          .sort((a, b) => a - b))
      : readLegacyClaimed(ledgerJson).has(era)
        ? [0]
        : [];
  const isClaimed = claimedPages.includes(page);
  const withinPayoutWindow = payoutEras ? payoutEras.includes(era) : true;

  if (path === 'legacy' && page > 0) {
    return {
      stash,
      era,
      page,
      path,
      withinPayoutWindow,
      pageCount,
      claimedPages,
      isClaimed,
      exposureTotalPlanck: esTotal.toString(),
      pageExposureTotalPlanck: '0',
      validatorOwnStakePlanck: '0',
      validatorCommissionPerbill: null,
      validatorEraRewardPlanck: null,
      validatorEraPoints: null,
      totalEraPoints: null,
      estimatedTotalRewardPlanck: null,
      estimatedSharedRewardPlanck: null,
      estimatedCommissionRewardPlanck: null,
      recipients: [],
      computable: false,
      reason: 'legacy exposure only supports page 0 preview',
    };
  }

  let validatorOwn = 0n;
  let nominators: Array<{ stash: string; stakePlanck: bigint }> = [];
  let pageExposureTotal = 0n;

  if (path === 'paged') {
    const pagedCodec = await staking.erasStakersPaged(era, stash, page);
    const pagedExposure = buildPagedRecipients(stash, overviewJson ? bigFromHexOrDec(readObjectField(overviewJson, 'own')) : 0n, pagedCodec);
    validatorOwn = pagedExposure.validatorOwn;
    nominators = pagedExposure.nominators;
    pageExposureTotal = pagedExposure.pageTotal;
  } else {
    validatorOwn = esJson ? bigFromHexOrDec(readObjectField(esJson, 'own')) : 0n;
    const others = (readObjectField(esJson, 'others') as unknown[] | undefined) ?? [];
    nominators = Array.isArray(others)
      ? others
          .map((row) => {
            if (row == null || typeof row !== 'object') {
              return null;
            }
            const who = decodeAccountIdLike(readObjectField(row, 'who'));
            const value = bigFromHexOrDec(readObjectField(row, 'value'));
            return who ? { stash: who, stakePlanck: value } : null;
          })
          .filter((row): row is { stash: string; stakePlanck: bigint } => row != null)
      : [];
    pageExposureTotal = esTotal;
  }

  const prefsCodec = await staking.validators(stash);
  const prefsJson = prefsCodec.isEmpty ? null : (codecToJson(prefsCodec) as Record<string, unknown>);
  const commission = prefsJson ? bigFromHexOrDec(readObjectField(prefsJson, 'commission')) : 0n;

  const eraRewardCodec = staking.erasValidatorReward ? await staking.erasValidatorReward(era) : null;
  const eraReward = eraRewardCodec && !eraRewardCodec.isEmpty ? bigFromHexOrDec(codecToJson(eraRewardCodec)) : 0n;
  const rewardPointsCodec = staking.erasRewardPoints ? await staking.erasRewardPoints(era) : null;
  const { validatorPoints, totalPoints } = rewardPointsCodec
    ? decodeRewardPoints(rewardPointsCodec, stash)
    : { validatorPoints: 0n, totalPoints: 0n };

  const computable = pageExposureTotal > 0n && eraReward > 0n && validatorPoints > 0n && totalPoints > 0n;
  const reason =
    pageExposureTotal <= 0n
      ? 'no exposure found for the requested era/page'
      : eraReward <= 0n
        ? 'erasValidatorReward unavailable or zero for this era'
        : validatorPoints <= 0n || totalPoints <= 0n
          ? 'erasRewardPoints unavailable or validator has no recorded points'
          : null;

  let estimatedTotalReward = 0n;
  let estimatedCommissionReward = 0n;
  let estimatedSharedReward = 0n;

  if (computable) {
    estimatedTotalReward = ratioMulFloor(eraReward, validatorPoints, totalPoints);
    estimatedCommissionReward = perbillMulFloor(estimatedTotalReward, commission);
    estimatedSharedReward = estimatedTotalReward - estimatedCommissionReward;
  }

  const recipients: StakingRewardPreviewRecipient[] = [];
  const makeReward = (stakePlanck: bigint, addCommission: boolean): string | null => {
    if (!computable) {
      return null;
    }
    const proportional = ratioMulFloor(estimatedSharedReward, stakePlanck, pageExposureTotal);
    const total = addCommission ? proportional + estimatedCommissionReward : proportional;
    return total.toString();
  };

  recipients.push({
    stash,
    role: 'validator',
    stakePlanck: validatorOwn.toString(),
    estimatedRewardPlanck: makeReward(validatorOwn, true),
  });
  for (const row of nominators) {
    recipients.push({
      stash: row.stash,
      role: 'nominator',
      stakePlanck: row.stakePlanck.toString(),
      estimatedRewardPlanck: makeReward(row.stakePlanck, false),
    });
  }

  return {
    stash,
    era,
    page,
    path,
    withinPayoutWindow,
    pageCount,
    claimedPages,
    isClaimed,
    exposureTotalPlanck: (path === 'paged' ? ovTotal : esTotal).toString(),
    pageExposureTotalPlanck: pageExposureTotal.toString(),
    validatorOwnStakePlanck: validatorOwn.toString(),
    validatorCommissionPerbill: Number(commission),
    validatorEraRewardPlanck: computable ? estimatedTotalReward.toString() : eraReward > 0n ? eraReward.toString() : null,
    validatorEraPoints: validatorPoints > 0n ? Number(validatorPoints) : null,
    totalEraPoints: totalPoints > 0n ? Number(totalPoints) : null,
    estimatedTotalRewardPlanck: computable ? estimatedTotalReward.toString() : null,
    estimatedSharedRewardPlanck: computable ? estimatedSharedReward.toString() : null,
    estimatedCommissionRewardPlanck: computable ? estimatedCommissionReward.toString() : null,
    recipients,
    computable,
    reason,
  };
}

export async function listUnclaimedEraPages(
  api: ApiPromise,
  stash: string,
  ledgerJson: Record<string, unknown> | null,
  eras: number[],
): Promise<UnclaimedEraRow[]> {
  const staking = api.query.staking as any;
  const legacyClaimed = readLegacyClaimed(ledgerJson);
  const rows: UnclaimedEraRow[] = [];

  for (const era of eras) {
    const overviewCodec = await staking.erasStakersOverview(era, stash);
    const overviewJson = overviewCodec.isEmpty ? null : (codecToJson(overviewCodec) as Record<string, unknown>);
    const esCodec = await staking.erasStakers(era, stash);
    const esJson = esCodec.isEmpty ? null : (codecToJson(esCodec) as Record<string, unknown>);

    const ovTotal = overviewJson ? bigFromHexOrDec(readObjectField(overviewJson, 'total')) : 0n;
    const esTotal = esJson ? bigFromHexOrDec(readObjectField(esJson, 'total')) : 0n;

    if (overviewJson && ovTotal > 0n) {
      const pageCount = pageCountFromOverviewJson(overviewJson);
      if (pageCount === 0) {
        continue;
      }
      const claimedCodec = await staking.claimedRewards(era, stash);
      const claimedArr = (codecToJson(claimedCodec) as unknown[] | null) ?? [];
      const claimedPages = claimedArr
        .map((x) => coerceNumber(x) ?? Number(x))
        .filter((n) => Number.isFinite(n))
        .sort((a, b) => a - b);
      const claimedSet = new Set(claimedPages);
      const missingPages: number[] = [];
      for (let p = 0; p < pageCount; p++) {
        if (!claimedSet.has(p)) {
          missingPages.push(p);
        }
      }
      if (missingPages.length > 0) {
        rows.push({
          era,
          path: 'paged',
          exposureTotalPlanck: ovTotal.toString(),
          pageCount,
          claimedPages,
          missingPages,
        });
      }
      continue;
    }

    if (esJson && esTotal > 0n) {
      if (legacyClaimed.has(era)) {
        continue;
      }
      rows.push({
        era,
        path: 'legacy',
        exposureTotalPlanck: esTotal.toString(),
        pageCount: 1,
        claimedPages: [],
        missingPages: [0],
      });
    }
  }

  return rows;
}
