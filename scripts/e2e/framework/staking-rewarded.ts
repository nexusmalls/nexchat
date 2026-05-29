/**
 * Parse staking.Rewarded event payload (AccountId, RewardDestination, Balance).
 * `toHuman()` 常导致金额/地址解析为 0 或空；优先使用 Codec.toJSON() 或已解码的 JSON 元组。
 */

import { readObjectField } from './codec.js';
import { asBigInt } from './units.js';

function decodeBalanceField(v: unknown): bigint {
  if (v == null) {
    return 0n;
  }
  if (typeof v === 'bigint') {
    return v;
  }
  if (typeof v === 'number') {
    return Number.isFinite(v) ? BigInt(Math.trunc(v)) : 0n;
  }
  if (typeof v === 'string') {
    const s = v.trim().replace(/,/g, '');
    if (!s) {
      return 0n;
    }
    if (s.startsWith('0x') || s.startsWith('0X')) {
      try {
        return BigInt(s);
      } catch {
        return 0n;
      }
    }
    return asBigInt(s);
  }
  return asBigInt(v);
}

function decodeStashField(v: unknown): string {
  if (v == null) {
    return '';
  }
  if (typeof v === 'string') {
    return v;
  }
  if (typeof v === 'object' && v !== null && 'Id' in (v as object)) {
    return String((v as { Id?: unknown }).Id ?? '');
  }
  return String(v);
}

/**
 * @param raw — `event.data`（Codec）或 `codecToJson(event.data)` 的结果
 */
export function parseStakingRewardedEventData(raw: unknown): { stash: string; amountPlanck: bigint } | null {
  const decoded =
    raw != null && typeof (raw as { toJSON?: () => unknown }).toJSON === 'function'
      ? (raw as { toJSON: () => unknown }).toJSON()
      : raw;

  if (Array.isArray(decoded) && decoded.length >= 3) {
    return {
      stash: decodeStashField(decoded[0]),
      amountPlanck: decodeBalanceField(decoded[2]),
    };
  }

  if (decoded != null && typeof decoded === 'object' && !Array.isArray(decoded)) {
    const o = decoded as Record<string, unknown>;
    const stash = decodeStashField(readObjectField(o, 'stash'));
    const amountPlanck = decodeBalanceField(readObjectField(o, 'amount'));
    if (stash) {
      return { stash, amountPlanck };
    }
  }

  return null;
}
