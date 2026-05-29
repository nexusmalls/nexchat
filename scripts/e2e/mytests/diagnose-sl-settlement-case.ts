#!/usr/bin/env tsx

process.env.WS_URL ??= 'ws://149.88.70.10:9944';

import { createInterface } from 'node:readline';
import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

type Args = {
  wsUrl: string;
  entityId: number;
  target: string;
  caseMember: string;
  orderId?: number;
  nonInteractive: boolean;
  maxDepth: number;
};

type QueueMember = {
  address: string;
  storedIndex: number | null;
  removed: boolean;
  level: number;
  referrer: string;
  activated: boolean | null;
  isMemberActive: boolean | null;
  isBanned: boolean | null;
};

type OrderSummary = {
  orderId: number;
  entityId: number;
  buyer: string;
  seller: string;
  payer: string;
  status: string;
  totalAmount: bigint;
  platformFee: bigint;
  createdAt: number | null;
  completedAt: number | null;
  paymentAsset: string;
  raw: Record<string, unknown>;
};

type CommissionRecord = {
  beneficiary: string;
  amount: bigint;
  level: number;
  commissionType: string;
  status: string;
  direction: string;
};

type SingleLineConfig = {
  uplineRate: number;
  downlineRate: number;
  baseUplineLevels: number;
  baseDownlineLevels: number;
  maxUplineLevels: number;
  maxDownlineLevels: number;
  levelIncrementThreshold: bigint;
};

type MemberState = {
  address: string;
  level: number;
  referrer: string;
  activated: boolean | null;
  isMemberActive: boolean | null;
  isBanned: boolean | null;
  directReferrals: number;
  teamSize: number;
  totalSpent: bigint;
  upgradeEligibleSpent: bigint;
  raw: Record<string, unknown> | null;
};

type EffectiveLevels = {
  baseUp: number;
  baseDown: number;
  extraLevels: number;
  effectiveUp: number;
  effectiveDown: number;
  hasOverride: boolean;
};

const DEFAULT_ENTITY_ID = 100000;
const DEFAULT_TARGET = 'X4XLb5Z7rDcjhQ6EajaLXXdPwGBDkBTES7FpALx1WHKrX7pHr';
const DEFAULT_CASE_MEMBER = 'X4TW7kpWztMoWgUNN5NvoGsur9Nn8BgazkznG7WmpxxDeoCJZ';

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function line(char = '═', len = 96): string {
  return char.repeat(len);
}

function header(title: string): void {
  console.log(`\n${line('═')}`);
  console.log(`  ${title}`);
  console.log(line('═'));
}

function subHeader(title: string): void {
  console.log(`\n  ${line('─', 84)}`);
  console.log(`  ${title}`);
  console.log(`  ${line('─', 84)}`);
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(28)} ${value}`);
}

function boolLabel(value: boolean | null): string {
  if (value === true) return '是';
  if (value === false) return '否';
  return '未知';
}

function parseBool(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value;
  if (typeof value === 'string') {
    const normalized = value.trim().toLowerCase();
    if (['true', 'yes', '1'].includes(normalized)) return true;
    if (['false', 'no', '0'].includes(normalized)) return false;
  }
  return null;
}

function normalizeCommissionType(type: string): string {
  if (type.includes('SingleLineDownline')) return 'SingleLineDownline';
  if (type.includes('SingleLineUpline')) return 'SingleLineUpline';
  if (type.includes('DirectReward')) return 'DirectReward';
  return type;
}

function directionOf(type: string): string {
  if (type === 'SingleLineDownline') return '下线';
  if (type === 'SingleLineUpline') return '上线';
  return '-';
}

function formatReachMode(value: unknown): string {
  if (value == null) return '未知';
  if (typeof value === 'string') return value;
  if (typeof value === 'number') {
    if (value === 0) return 'Bidirectional';
    if (value === 1) return 'BuyerOnly';
    if (value === 2) return 'BeneficiaryOnly';
    return String(value);
  }
  if (typeof value === 'object') {
    const json = JSON.stringify(value);
    if (json.includes('Bidirectional')) return 'Bidirectional';
    if (json.includes('BuyerOnly')) return 'BuyerOnly';
    if (json.includes('BeneficiaryOnly')) return 'BeneficiaryOnly';
    return json;
  }
  return String(value);
}

function getReachModeName(rawConfig: Record<string, unknown> | null): string {
  return formatReachMode(readObjectField(rawConfig, 'reachMode', 'reach_mode'));
}

function parseArgs(): Args {
  const argv = process.argv.slice(2);
  const parsed: Record<string, string | boolean> = {};

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith('--')) continue;
    const key = arg.slice(2);
    const next = argv[i + 1];
    if (!next || next.startsWith('--')) {
      parsed[key] = true;
      continue;
    }
    parsed[key] = next;
    i++;
  }

  const wsUrl = String(parsed.ws ?? process.env.WS_URL ?? 'ws://149.88.70.10:9944');
  process.env.WS_URL = wsUrl;

  return {
    wsUrl,
    entityId: Number(parsed.entity ?? DEFAULT_ENTITY_ID),
    target: String(parsed.target ?? DEFAULT_TARGET),
    caseMember: String(parsed['case-member'] ?? DEFAULT_CASE_MEMBER),
    orderId: parsed.order != null ? Number(parsed.order) : undefined,
    nonInteractive: parsed['non-interactive'] === true || process.stdin.isTTY !== true,
    maxDepth: Number(parsed['max-depth'] ?? 8),
  };
}

function createWaiter(nonInteractive: boolean) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  async function waitForEnter(prompt: string): Promise<void> {
    if (nonInteractive) {
      console.log(`\n  [自动继续] ${prompt}`);
      return;
    }
    await new Promise<void>((resolve) => {
      rl.question(`\n  >> ${prompt}，按 Enter 继续 ... `, () => resolve());
    });
  }
  function close(): void {
    rl.close();
  }
  return { waitForEnter, close };
}

async function getMemberState(api: any, entityId: number, address: string): Promise<MemberState> {
  const mb = (api.query as any).entityMember;
  let raw: Record<string, unknown> | null = null;

  try {
    const value = await mb.entityMembers(entityId, address);
    if (value && !(value as any).isNone) {
      raw = codecToJson<Record<string, unknown>>((value as any).unwrap());
    }
  } catch {}

  if (!raw) {
    return {
      address,
      level: 0,
      referrer: '',
      activated: null,
      isMemberActive: null,
      isBanned: null,
      directReferrals: 0,
      teamSize: 0,
      totalSpent: 0n,
      upgradeEligibleSpent: 0n,
      raw: null,
    };
  }

  return {
    address,
    level: coerceNumber(readObjectField(raw, 'customLevelId', 'custom_level_id', 'levelId', 'level_id')) ?? 0,
    referrer: String(readObjectField(raw, 'referrer') ?? ''),
    activated: parseBool(readObjectField(raw, 'activated', 'isActivated', 'is_activated')),
    isMemberActive: parseBool(readObjectField(raw, 'memberActive', 'isMemberActive', 'is_member_active')),
    isBanned: parseBool(readObjectField(raw, 'banned', 'isBanned', 'is_banned')),
    directReferrals: coerceNumber(readObjectField(raw, 'directReferrals', 'direct_referrals')) ?? 0,
    teamSize: coerceNumber(readObjectField(raw, 'teamSize', 'team_size')) ?? 0,
    totalSpent: asBigInt(readObjectField(raw, 'totalSpent', 'total_spent') ?? 0),
    upgradeEligibleSpent: asBigInt(readObjectField(raw, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0),
    raw,
  };
}

async function getSingleLineConfig(api: any, entityId: number): Promise<{ config: SingleLineConfig; raw: Record<string, unknown> | null; enabled: boolean; pending: Record<string, unknown> | null; }> {
  const sl = (api.query as any).commissionSingleLine;
  let raw: Record<string, unknown> | null = null;
  let pending: Record<string, unknown> | null = null;

  const value = await sl.singleLineConfigs(entityId);
  if (value && !(value as any).isNone) {
    raw = codecToJson<Record<string, unknown>>((value as any).unwrap ? (value as any).unwrap() : value);
  }
  if (!raw) {
    throw new Error(`entity ${entityId} 未配置 SingleLine`);
  }

  try {
    const pendingRaw = await sl.pendingConfigChanges(entityId);
    if (pendingRaw && !(pendingRaw as any).isNone) {
      pending = codecToJson<Record<string, unknown>>((pendingRaw as any).unwrap ? (pendingRaw as any).unwrap() : pendingRaw);
    }
  } catch {}

  let enabled = true;
  try {
    enabled = codecToJson<boolean>(await sl.singleLineEnabled(entityId)) ?? true;
  } catch {}

  return {
    raw,
    pending,
    enabled,
    config: {
      uplineRate: coerceNumber(readObjectField(raw, 'uplineRate', 'upline_rate')) ?? 0,
      downlineRate: coerceNumber(readObjectField(raw, 'downlineRate', 'downline_rate')) ?? 0,
      baseUplineLevels: coerceNumber(readObjectField(raw, 'baseUplineLevels', 'base_upline_levels')) ?? 0,
      baseDownlineLevels: coerceNumber(readObjectField(raw, 'baseDownlineLevels', 'base_downline_levels')) ?? 0,
      maxUplineLevels: coerceNumber(readObjectField(raw, 'maxUplineLevels', 'max_upline_levels')) ?? 0,
      maxDownlineLevels: coerceNumber(readObjectField(raw, 'maxDownlineLevels', 'max_downline_levels')) ?? 0,
      levelIncrementThreshold: asBigInt(readObjectField(raw, 'levelIncrementThreshold', 'level_increment_threshold') ?? 0),
    },
  };
}

async function getLevelOverride(api: any, entityId: number, levelId: number): Promise<{ upline: number; downline: number; raw: Record<string, unknown> | null; }> {
  const sl = (api.query as any).commissionSingleLine;
  try {
    const value = await sl.singleLineCustomLevelOverrides(entityId, levelId);
    if (value && !(value as any).isNone) {
      const raw = codecToJson<Record<string, unknown>>((value as any).unwrap ? (value as any).unwrap() : value);
      return {
        raw,
        upline: coerceNumber(readObjectField(raw, 'uplineLevels', 'upline_levels')) ?? 0,
        downline: coerceNumber(readObjectField(raw, 'downlineLevels', 'downline_levels')) ?? 0,
      };
    }
  } catch {}
  return { upline: 0, downline: 0, raw: null };
}

async function getEffectiveLevels(api: any, entityId: number, address: string, config: SingleLineConfig, level?: number): Promise<EffectiveLevels> {
  const cc = (api.query as any).commissionCore;
  const member = level != null ? { level } : await getMemberState(api, entityId, address);
  const override = await getLevelOverride(api, entityId, member.level);
  const stats = codecToJson<Record<string, unknown>>(await cc.memberCommissionStats(entityId, address));
  const earned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
  const extraLevels = config.levelIncrementThreshold > 0n ? Number(earned / config.levelIncrementThreshold) : 0;
  const baseUp = override.raw ? override.upline : config.baseUplineLevels;
  const baseDown = override.raw ? override.downline : config.baseDownlineLevels;

  return {
    baseUp,
    baseDown,
    extraLevels,
    effectiveUp: Math.min(baseUp + extraLevels, config.maxUplineLevels),
    effectiveDown: Math.min(baseDown + extraLevels, config.maxDownlineLevels),
    hasOverride: override.raw != null,
  };
}

async function getQueueMember(api: any, entityId: number, address: string): Promise<QueueMember> {
  const sl = (api.query as any).commissionSingleLine;
  const member = await getMemberState(api, entityId, address);
  let storedIndex: number | null = null;
  let removed = false;

  try {
    storedIndex = coerceNumber(codecToJson(await sl.singleLineIndex(entityId, address))) ?? null;
  } catch {}
  try {
    removed = codecToJson<boolean>(await sl.removedMembers(entityId, address)) ?? false;
  } catch {}

  return {
    address,
    storedIndex,
    removed,
    level: member.level,
    referrer: member.referrer,
    activated: member.activated,
    isMemberActive: member.isMemberActive,
    isBanned: member.isBanned,
  };
}

async function getMemberStateAt(apiAt: any, entityId: number, address: string): Promise<MemberState> {
  const mb = (apiAt.query as any).entityMember;
  let raw: Record<string, unknown> | null = null;

  try {
    const value = await mb.entityMembers(entityId, address);
    if (value && !(value as any).isNone) {
      raw = codecToJson<Record<string, unknown>>((value as any).unwrap ? (value as any).unwrap() : value);
    }
  } catch {}

  if (!raw) {
    return {
      address,
      level: 0,
      referrer: '',
      activated: null,
      isMemberActive: null,
      isBanned: null,
      directReferrals: 0,
      teamSize: 0,
      totalSpent: 0n,
      upgradeEligibleSpent: 0n,
      raw: null,
    };
  }

  return {
    address,
    level: coerceNumber(readObjectField(raw, 'customLevelId', 'custom_level_id', 'levelId', 'level_id')) ?? 0,
    referrer: String(readObjectField(raw, 'referrer') ?? ''),
    activated: parseBool(readObjectField(raw, 'activated', 'isActivated', 'is_activated')),
    isMemberActive: parseBool(readObjectField(raw, 'memberActive', 'isMemberActive', 'is_member_active')),
    isBanned: parseBool(readObjectField(raw, 'banned', 'isBanned', 'is_banned')),
    directReferrals: coerceNumber(readObjectField(raw, 'directReferrals', 'direct_referrals')) ?? 0,
    teamSize: coerceNumber(readObjectField(raw, 'teamSize', 'team_size')) ?? 0,
    totalSpent: asBigInt(readObjectField(raw, 'totalSpent', 'total_spent') ?? 0),
    upgradeEligibleSpent: asBigInt(readObjectField(raw, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0),
    raw,
  };
}

async function getQueueMemberAt(apiAt: any, entityId: number, address: string): Promise<QueueMember> {
  const sl = (apiAt.query as any).commissionSingleLine;
  const member = await getMemberStateAt(apiAt, entityId, address);
  let storedIndex: number | null = null;
  let removed = false;

  try {
    storedIndex = coerceNumber(codecToJson(await sl.singleLineIndex(entityId, address))) ?? null;
  } catch {}
  try {
    removed = codecToJson<boolean>(await sl.removedMembers(entityId, address)) ?? false;
  } catch {}

  return {
    address,
    storedIndex,
    removed,
    level: member.level,
    referrer: member.referrer,
    activated: member.activated,
    isMemberActive: member.isMemberActive,
    isBanned: member.isBanned,
  };
}

async function getSingleLineConfigAt(apiAt: any, entityId: number): Promise<{ raw: Record<string, unknown> | null; enabled: boolean; }> {
  const sl = (apiAt.query as any).commissionSingleLine;
  let raw: Record<string, unknown> | null = null;
  let enabled = true;

  try {
    const value = await sl.singleLineConfigs(entityId);
    if (value && !(value as any).isNone) {
      raw = codecToJson<Record<string, unknown>>((value as any).unwrap ? (value as any).unwrap() : value);
    }
  } catch {}

  try {
    enabled = codecToJson<boolean>(await sl.singleLineEnabled(entityId)) ?? true;
  } catch {}

  return { raw, enabled };
}

async function getEffectiveLevelsAt(apiAt: any, entityId: number, address: string, configRaw: Record<string, unknown> | null, level?: number): Promise<EffectiveLevels | null> {
  if (!configRaw) return null;
  const config: SingleLineConfig = {
    uplineRate: coerceNumber(readObjectField(configRaw, 'uplineRate', 'upline_rate')) ?? 0,
    downlineRate: coerceNumber(readObjectField(configRaw, 'downlineRate', 'downline_rate')) ?? 0,
    baseUplineLevels: coerceNumber(readObjectField(configRaw, 'baseUplineLevels', 'base_upline_levels')) ?? 0,
    baseDownlineLevels: coerceNumber(readObjectField(configRaw, 'baseDownlineLevels', 'base_downline_levels')) ?? 0,
    maxUplineLevels: coerceNumber(readObjectField(configRaw, 'maxUplineLevels', 'max_upline_levels')) ?? 0,
    maxDownlineLevels: coerceNumber(readObjectField(configRaw, 'maxDownlineLevels', 'max_downline_levels')) ?? 0,
    levelIncrementThreshold: asBigInt(readObjectField(configRaw, 'levelIncrementThreshold', 'level_increment_threshold') ?? 0),
  };

  const cc = (apiAt.query as any).commissionCore;
  const member = level != null ? { level } : await getMemberStateAt(apiAt, entityId, address);
  const override = await (async () => {
    const sl = (apiAt.query as any).commissionSingleLine;
    try {
      const value = await sl.singleLineCustomLevelOverrides(entityId, member.level);
      if (value && !(value as any).isNone) {
        const raw = codecToJson<Record<string, unknown>>((value as any).unwrap ? (value as any).unwrap() : value);
        return {
          raw,
          upline: coerceNumber(readObjectField(raw, 'uplineLevels', 'upline_levels')) ?? 0,
          downline: coerceNumber(readObjectField(raw, 'downlineLevels', 'downline_levels')) ?? 0,
        };
      }
    } catch {}
    return { raw: null as Record<string, unknown> | null, upline: 0, downline: 0 };
  })();

  const stats = codecToJson<Record<string, unknown>>(await cc.memberCommissionStats(entityId, address));
  const earned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
  const extraLevels = config.levelIncrementThreshold > 0n ? Number(earned / config.levelIncrementThreshold) : 0;
  const baseUp = override.raw ? override.upline : config.baseUplineLevels;
  const baseDown = override.raw ? override.downline : config.baseDownlineLevels;

  return {
    baseUp,
    baseDown,
    extraLevels,
    effectiveUp: Math.min(baseUp + extraLevels, config.maxUplineLevels),
    effectiveDown: Math.min(baseDown + extraLevels, config.maxDownlineLevels),
    hasOverride: override.raw != null,
  };
}

async function getBlockHashByNumber(api: any, blockNumber: number): Promise<string | null> {
  try {
    const hash = await api.rpc.chain.getBlockHash(blockNumber);
    return hash?.toString() ?? null;
  } catch {
    return null;
  }
}

async function scanOrders(api: any, entityId: number): Promise<OrderSummary[]> {
  const tx = (api.query as any).entityTransaction;
  const nextOrderId = coerceNumber(codecToJson(await tx.nextOrderId())) ?? 0;
  const orders: OrderSummary[] = [];

  for (let orderId = 0; orderId < nextOrderId; orderId++) {
    let raw: Record<string, unknown> | null = null;
    try {
      const value = await tx.orders(orderId);
      if (value && !(value as any).isNone) {
        raw = codecToJson<Record<string, unknown>>((value as any).unwrap());
      }
    } catch {}
    if (!raw) continue;

    const currentEntityId = coerceNumber(readObjectField(raw, 'entityId', 'entity_id')) ?? -1;
    if (currentEntityId !== entityId) continue;

    orders.push({
      orderId,
      entityId: currentEntityId,
      buyer: String(readObjectField(raw, 'buyer') ?? ''),
      seller: String(readObjectField(raw, 'seller') ?? ''),
      payer: String(readObjectField(raw, 'payer') ?? ''),
      status: String(readObjectField(raw, 'status') ?? ''),
      totalAmount: asBigInt(readObjectField(raw, 'totalAmount', 'total_amount') ?? 0),
      platformFee: asBigInt(readObjectField(raw, 'platformFee', 'platform_fee') ?? 0),
      createdAt: coerceNumber(readObjectField(raw, 'createdAt', 'created_at')) ?? null,
      completedAt: coerceNumber(readObjectField(raw, 'completedAt', 'completed_at')) ?? null,
      paymentAsset: String(readObjectField(raw, 'paymentAsset', 'payment_asset') ?? 'Native'),
      raw,
    });
  }

  return orders;
}

async function loadCommissionRecords(api: any, orderId: number): Promise<CommissionRecord[]> {
  const cc = (api.query as any).commissionCore;
  const raw = codecToJson<any[]>(await cc.orderCommissionRecords(orderId)) ?? [];
  return raw.map((record) => {
    const commissionType = normalizeCommissionType(String(readObjectField(record, 'commissionType', 'commission_type') ?? ''));
    return {
      beneficiary: String(readObjectField(record, 'beneficiary') ?? ''),
      amount: asBigInt(readObjectField(record, 'amount') ?? 0),
      level: coerceNumber(readObjectField(record, 'level')) ?? 0,
      commissionType,
      status: String(readObjectField(record, 'status') ?? ''),
      direction: directionOf(commissionType),
    };
  });
}

async function loadUnallocated(api: any, orderId: number): Promise<bigint> {
  const cc = (api.query as any).commissionCore;
  try {
    const raw = codecToJson<any>(await cc.orderUnallocated(orderId));
    return asBigInt(readObjectField(raw, 'amount') ?? readObjectField(raw, '2') ?? 0);
  } catch {
    return 0n;
  }
}

async function loadFullSingleLineChain(api: any, entityId: number): Promise<QueueMember[]> {
  const sl = (api.query as any).commissionSingleLine;
  const segmentCount = coerceNumber(codecToJson(await sl.singleLineSegmentCount(entityId))) ?? 0;
  const members: QueueMember[] = [];

  for (let seg = 0; seg < segmentCount; seg++) {
    const addresses = codecToJson<string[]>(await sl.singleLineSegments(entityId, seg)) ?? [];
    for (const address of addresses) {
      members.push(await getQueueMember(api, entityId, String(address)));
    }
  }

  members.sort((a, b) => {
    const ai = a.storedIndex ?? Number.MAX_SAFE_INTEGER;
    const bi = b.storedIndex ?? Number.MAX_SAFE_INTEGER;
    return ai - bi;
  });

  return members;
}

async function loadAllReferralMembers(api: any, entityId: number): Promise<MemberState[]> {
  const mb = (api.query as any).entityMember;
  const entries = await mb.entityMembers.entries(entityId);
  const members: MemberState[] = [];

  for (const [storageKey, storageValue] of entries) {
    const args = (storageKey as any).args ?? [];
    const address = String(args[1] ?? '');
    if (!address) continue;

    let raw: Record<string, unknown> | null = null;
    if (storageValue && !(storageValue as any).isNone) {
      raw = codecToJson<Record<string, unknown>>((storageValue as any).unwrap ? (storageValue as any).unwrap() : storageValue);
    }

    if (!raw) {
      members.push({
        address,
        level: 0,
        referrer: '',
        activated: null,
        isMemberActive: null,
        isBanned: null,
        directReferrals: 0,
        teamSize: 0,
        totalSpent: 0n,
        upgradeEligibleSpent: 0n,
        raw: null,
      });
      continue;
    }

    members.push({
      address,
      level: coerceNumber(readObjectField(raw, 'customLevelId', 'custom_level_id', 'levelId', 'level_id')) ?? 0,
      referrer: String(readObjectField(raw, 'referrer') ?? ''),
      activated: parseBool(readObjectField(raw, 'activated', 'isActivated', 'is_activated')),
      isMemberActive: parseBool(readObjectField(raw, 'memberActive', 'isMemberActive', 'is_member_active')),
      isBanned: parseBool(readObjectField(raw, 'banned', 'isBanned', 'is_banned')),
      directReferrals: coerceNumber(readObjectField(raw, 'directReferrals', 'direct_referrals')) ?? 0,
      teamSize: coerceNumber(readObjectField(raw, 'teamSize', 'team_size')) ?? 0,
      totalSpent: asBigInt(readObjectField(raw, 'totalSpent', 'total_spent') ?? 0),
      upgradeEligibleSpent: asBigInt(readObjectField(raw, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0),
      raw,
    });
  }

  members.sort((a, b) => {
    const ar = a.referrer || 'zzzz';
    const br = b.referrer || 'zzzz';
    if (ar !== br) return ar.localeCompare(br);
    return a.address.localeCompare(b.address);
  });

  return members;
}

function describeSkipReason(member: QueueMember): string {
  const reasons: string[] = [];
  if (member.removed) reasons.push('removed');
  if (member.activated === false) reasons.push('未激活');
  if (member.isMemberActive === false) reasons.push('不活跃');
  if (member.isBanned === true) reasons.push('已封禁');
  return reasons.length > 0 ? reasons.join('、') : '可参与';
}

function buildSingleLineArrowView(chain: QueueMember[], target: string, caseMember: string): string {
  return chain.map((member) => {
    const marks: string[] = [];
    if (member.address === target) marks.push('CS1');
    if (member.address === caseMember) marks.push('CS6');
    return `[${member.storedIndex}]${shortAddr(member.address)}${marks.length ? `(${marks.join('/')})` : ''}`;
  }).join(' -> ');
}

function printReferralTree(
  root: string,
  childrenMap: Map<string, string[]>,
  memberMap: Map<string, MemberState>,
  target: string,
  caseMember: string,
  prefix = '',
  isLast = true,
): void {
  const member = memberMap.get(root);
  const marks: string[] = [];
  if (root === target) marks.push('CS1');
  if (root === caseMember) marks.push('CS6');
  const branch = prefix ? `${isLast ? '└─ ' : '├─ '}` : '';
  console.log(`  ${prefix}${branch}${root} | Lv${member?.level ?? 0}${marks.length ? ` | ${marks.join(', ')}` : ''}`);

  const children = childrenMap.get(root) ?? [];
  children.forEach((child, index) => {
    const nextPrefix = prefix + (prefix ? (isLast ? '   ' : '│  ') : '');
    printReferralTree(child, childrenMap, memberMap, target, caseMember, nextPrefix, index === children.length - 1);
  });
}

function printReferralForest(
  roots: string[],
  childrenMap: Map<string, string[]>,
  memberMap: Map<string, MemberState>,
  target: string,
  caseMember: string,
): void {
  const walk = (node: string, prefix: string, isLast: boolean) => {
    const member = memberMap.get(node);
    const marks: string[] = [];
    if (node === target) marks.push('CS1');
    if (node === caseMember) marks.push('CS6');
    const connector = prefix ? (isLast ? '└─ ' : '├─ ') : '';
    console.log(`  ${prefix}${connector}${node} | Lv${member?.level ?? 0}${marks.length ? ` | ${marks.join(', ')}` : ''}`);

    const children = [...(childrenMap.get(node) ?? [])].sort();
    children.forEach((child, index) => {
      const childPrefix = prefix + (prefix ? (isLast ? '   ' : '│  ') : '');
      walk(child, childPrefix, index === children.length - 1);
    });
  };

  roots.forEach((root, index) => {
    walk(root, '', index === roots.length - 1);
  });
}

function referralDepthFrom(start: string, target: string, memberMap: Map<string, MemberState>, maxDepth: number): number | null {
  const path = findReferralPath(memberMap, start, target, maxDepth);
  return path.length > 0 ? path.length - 1 : null;
}

function singleLineDistance(targetIndex: number | null, otherIndex: number | null): number | null {
  if (targetIndex == null || otherIndex == null) return null;
  return otherIndex - targetIndex;
}

function describeSingleLineRelation(distance: number | null): string {
  if (distance == null) return '不在 single-line 链';
  if (distance === 0) return '同一位置';
  if (distance > 0) return `在 CS1 下方第 ${distance} 位`;
  return `在 CS1 上方第 ${Math.abs(distance)} 位`;
}

function describeReferralRelation(depth: number | null): string {
  if (depth == null) return '不是 CS1 推荐树后代';
  if (depth === 0) return 'CS1 本人';
  return `CS1 推荐树第 ${depth} 层`;
}

function findReferralRoots(memberMap: Map<string, MemberState>): string[] {
  const all = [...memberMap.keys()];
  return all.filter((address) => {
    const referrer = memberMap.get(address)?.referrer ?? '';
    return !referrer || !memberMap.has(referrer);
  }).sort();
}

function collectComparisonRows(
  allReferralMembers: MemberState[],
  queueMap: Map<string, QueueMember>,
  target: string,
  memberMap: Map<string, MemberState>,
  maxDepth: number,
): Array<{ address: string; level: number; referralDepth: number | null; queueDistance: number | null; }> {
  const targetIndex = queueMap.get(target)?.storedIndex ?? null;
  return allReferralMembers.map((member) => ({
    address: member.address,
    level: member.level,
    referralDepth: referralDepthFrom(target, member.address, memberMap, maxDepth),
    queueDistance: singleLineDistance(targetIndex, queueMap.get(member.address)?.storedIndex ?? null),
  }));
}

function isSkippedMember(member: QueueMember): boolean {
  return member.removed || member.activated === false || member.isMemberActive === false || member.isBanned === true;
}

function findReferralPath(memberMap: Map<string, MemberState>, start: string, target: string, maxDepth: number): string[] {
  const path: string[] = [];
  const visited = new Set<string>();
  let current = target;
  let depth = 0;

  while (current && depth <= maxDepth) {
    path.unshift(current);
    if (current === start) return path;
    if (visited.has(current)) break;
    visited.add(current);
    current = memberMap.get(current)?.referrer ?? '';
    depth += 1;
  }

  return [];
}

async function main(): Promise<void> {
  const args = parseArgs();
  const { waitForEnter, close } = createWaiter(args.nonInteractive);
  const api = await connectApi(args.wsUrl);

  try {
    const chainHeader = await api.rpc.chain.getHeader();
    const block = chainHeader.number.toNumber();
    const spec = `${api.runtimeVersion.specName.toString()} v${api.runtimeVersion.specVersion.toString()}`;
    const cc = (api.query as any).commissionCore;
    const sl = (api.query as any).commissionSingleLine;

    header(`单排线奖励异常案例诊断 | Block #${block}`);
    kv('节点', args.wsUrl);
    kv('链版本', spec);
    kv('实体 ID', String(args.entityId));
    kv('目标账户 CS1', args.target);
    kv('案例账户 CS6', args.caseMember);
    kv('交互模式', args.nonInteractive ? '关闭（自动继续）' : '开启');

    await waitForEnter('请确认当前节点和案例参数正确');

    subHeader('阶段 1：先列出完整 single-line 链与全部推荐关系');
    const fullSingleLineChain = await loadFullSingleLineChain(api, args.entityId);
    const allReferralMembers = await loadAllReferralMembers(api, args.entityId);

    kv('single-line 链总人数', String(fullSingleLineChain.length));
    console.log(`  single-line 箭头图: ${buildSingleLineArrowView(fullSingleLineChain, args.target, args.caseMember)}`);
    for (const member of fullSingleLineChain) {
      const marks: string[] = [];
      if (member.address === args.target) marks.push('CS1');
      if (member.address === args.caseMember) marks.push('CS6');
      console.log(`  [SL #${member.storedIndex ?? '?'}] ${member.address} | Lv${member.level} | referrer=${member.referrer ? shortAddr(member.referrer) : '无'} | removed=${member.removed ? '是' : '否'} | activated=${boolLabel(member.activated)} | active=${boolLabel(member.isMemberActive)}${marks.length ? ` | ${marks.join(', ')}` : ''}`);
    }

    console.log('');
    kv('实体会员总数', String(allReferralMembers.length));
    for (const member of allReferralMembers) {
      const marks: string[] = [];
      if (member.address === args.target) marks.push('CS1');
      if (member.address === args.caseMember) marks.push('CS6');
      console.log(`  ${member.address} | Lv${member.level} | referrer=${member.referrer || '无'} | 直推=${member.directReferrals} | 团队=${member.teamSize} | activated=${boolLabel(member.activated)}${marks.length ? ` | ${marks.join(', ')}` : ''}`);
    }

    const memberMapStage1 = new Map<string, MemberState>(allReferralMembers.map((member) => [member.address, member]));
    const queueMapStage1 = new Map<string, QueueMember>(fullSingleLineChain.map((member) => [member.address, member]));
    const childrenMap = new Map<string, string[]>();
    for (const member of allReferralMembers) {
      if (!member.referrer) continue;
      const siblings = childrenMap.get(member.referrer) ?? [];
      siblings.push(member.address);
      siblings.sort();
      childrenMap.set(member.referrer, siblings);
    }

    console.log('\n  推荐关系树：');
    const roots = findReferralRoots(memberMapStage1);
    printReferralForest(roots, childrenMap, memberMapStage1, args.target, args.caseMember);

    console.log('\n  推荐层级 vs single-line 距离对照：');
    const comparisonRows = collectComparisonRows(allReferralMembers, queueMapStage1, args.target, memberMapStage1, args.maxDepth);
    for (const row of comparisonRows) {
      const marks: string[] = [];
      if (row.address === args.target) marks.push('CS1');
      if (row.address === args.caseMember) marks.push('CS6');
      console.log(`  ${row.address} | Lv${row.level} | ${describeReferralRelation(row.referralDepth)} | ${describeSingleLineRelation(row.queueDistance)}${marks.length ? ` | ${marks.join(', ')}` : ''}`);
    }

    await waitForEnter('已列出完整 single-line 链、推荐关系树和层级对照');

    subHeader('阶段 2：定位与案例账户相关的订单');
    const orders = await scanOrders(api, args.entityId);
    const relatedOrders = orders.filter((order) =>
      order.buyer === args.caseMember || order.seller === args.caseMember || order.payer === args.caseMember,
    );

    kv('实体订单总数', String(orders.length));
    kv('与 CS6 相关订单数', String(relatedOrders.length));
    if (relatedOrders.length > 0) {
      for (const order of relatedOrders) {
        console.log(`  订单 #${order.orderId}: 买家=${shortAddr(order.buyer)} 卖家=${shortAddr(order.seller)} payer=${shortAddr(order.payer)} 状态=${order.status} 金额=${formatNex(order.totalAmount)} 平台费=${formatNex(order.platformFee)} 创建=#${order.createdAt ?? 'N/A'} 完成=${order.completedAt != null ? `#${order.completedAt}` : '未完成'}`);
      }
    }

    const chosenOrder = args.orderId != null
      ? orders.find((order) => order.orderId === args.orderId)
      : relatedOrders[relatedOrders.length - 1];

    if (!chosenOrder) {
      throw new Error('没有找到可分析的目标订单，请用 --order 指定订单号');
    }

    kv('本次分析订单', `#${chosenOrder.orderId}`);
    kv('订单买家', chosenOrder.buyer);
    kv('订单状态', chosenOrder.status);
    await waitForEnter('请确认分析订单无误');

    subHeader('阶段 3：查看目标订单最终佣金结果');
    const commissionRecords = await loadCommissionRecords(api, chosenOrder.orderId);
    const unallocated = await loadUnallocated(api, chosenOrder.orderId);
    const grouped = new Map<string, CommissionRecord[]>();
    for (const record of commissionRecords) {
      const bucket = grouped.get(record.commissionType) ?? [];
      bucket.push(record);
      grouped.set(record.commissionType, bucket);
    }

    for (const [type, items] of grouped.entries()) {
      console.log(`  ${type} (${items.length} 条)`);
      for (const item of items) {
        const marker = item.beneficiary === args.target ? '  <-- CS1' : '';
        console.log(`    beneficiary=${shortAddr(item.beneficiary)} amount=${formatNex(item.amount)} level=L${item.level} status=${item.status}${marker}`);
      }
    }
    kv('订单未分配金额', formatNex(unallocated));

    const targetDirect = commissionRecords.filter((item) => item.beneficiary === args.target && item.commissionType === 'DirectReward');
    const targetDownline = commissionRecords.filter((item) => item.beneficiary === args.target && item.commissionType === 'SingleLineDownline');
    kv('CS1 直推奖条数', String(targetDirect.length));
    kv('CS1 下线排线奖条数', String(targetDownline.length));
    await waitForEnter('已确认该订单的最终佣金记录');

    subHeader('阶段 4：检查 SingleLine 配置与生效状态');
    const singleLine = await getSingleLineConfig(api, args.entityId);
    const commissionConfig = codecToJson<Record<string, unknown>>(await cc.commissionConfigs(args.entityId));
    const globalPaused = codecToJson<boolean>(await cc.globalCommissionPaused()) ?? false;
    const withdrawalPaused = codecToJson<boolean>(await cc.withdrawalPaused(args.entityId)) ?? false;

    kv('SingleLine 启用', singleLine.enabled ? '是' : '否');
    kv('基础上线层数', String(singleLine.config.baseUplineLevels));
    kv('基础下线层数', String(singleLine.config.baseDownlineLevels));
    kv('最大上线层数', String(singleLine.config.maxUplineLevels));
    kv('最大下线层数', String(singleLine.config.maxDownlineLevels));
    kv('ReachMode 模式', formatReachMode(readObjectField(singleLine.raw, 'reachMode', 'reach_mode')));
    kv('上线费率', `${singleLine.config.uplineRate} bps`);
    kv('下线费率', `${singleLine.config.downlineRate} bps`);
    kv('动态增层阈值', singleLine.config.levelIncrementThreshold > 0n ? formatNex(singleLine.config.levelIncrementThreshold) : '未开启');
    kv('CommissionCore 全局暂停', globalPaused ? '是' : '否');
    kv('提现暂停', withdrawalPaused ? '是' : '否');
    console.log(`  CommissionConfigs: ${JSON.stringify(commissionConfig)}`);
    if (singleLine.pending) {
      console.log(`  [提示] 发现 PendingConfigChanges，当前配置可能不是订单发生时生效的配置: ${JSON.stringify(singleLine.pending)}`);
    }
    await waitForEnter('已检查配置和暂停状态');

    subHeader('阶段 5：链上查询 CS1 的四种 skip 状态');
    const targetMember = await getMemberState(api, args.entityId, args.target);
    const targetSkipQueue = await getQueueMember(api, args.entityId, args.target);
    const isBanned = targetSkipQueue.isBanned === true;
    const isActivated = targetSkipQueue.activated === true;
    const isMemberActive = targetSkipQueue.isMemberActive === true;
    const removedFromSingleLine = targetSkipQueue.removed;
    kv('is_banned', isBanned ? 'true' : 'false');
    kv('!is_activated', (!isActivated) ? 'true' : 'false');
    kv('!is_member_active', (!isMemberActive) ? 'true' : 'false');
    kv('RemovedMembers', removedFromSingleLine ? 'true' : 'false');
    kv('最终 is_member_skipped', (isBanned || !isActivated || !isMemberActive || removedFromSingleLine) ? 'true' : 'false');

    const bannedAtRaw = readObjectField(targetMember.raw, 'bannedAt', 'banned_at');
    const activatedRaw = readObjectField(targetMember.raw, 'activated', 'isActivated', 'is_activated');
    const derivedBanned = bannedAtRaw != null;
    const derivedActivated = parseBool(activatedRaw) === true;
    const derivedMemberActive = !derivedBanned && derivedActivated;
    const derivedSkipped = derivedBanned || !derivedActivated || removedFromSingleLine;

    console.log('  原始字段展开：');
    console.log(`    banned_at 原始值 = ${JSON.stringify(bannedAtRaw)}`);
    console.log(`    activated 原始值 = ${JSON.stringify(activatedRaw)}`);
    console.log(`    removed 原始值 = ${removedFromSingleLine}`);

    console.log('  按链端定义推导：');
    console.log(`    推导 is_banned = ${derivedBanned}`);
    console.log(`    推导 is_activated = ${derivedActivated}`);
    console.log(`    推导 is_member_active = ${derivedMemberActive}`);
    console.log(`    推导 is_member_skipped = ${derivedSkipped}`);

    const consistencyOk = isBanned === derivedBanned
      && isActivated === derivedActivated
      && isMemberActive === derivedMemberActive;
    console.log(`  一致性检查: ${consistencyOk ? '一致' : '不一致 ⚠️'}`);
    if (!consistencyOk) {
      console.log('  [提示] 当前脚本展示的 memberActive/banned/activated 与链端定义推导不一致，说明前端读取字段与 runtime 真正 Provider 口径存在偏差。');
    }
    console.log('  对应 single-line/src/lib.rs:1270 的四个条件已按链上当前状态展开。');
    await waitForEnter('已查询 CS1 的四种 skip 状态');

    subHeader('阶段 6：检查 CS1 当前会员等级与有效层数依据');
    const targetEffective = await getEffectiveLevels(api, args.entityId, args.target, singleLine.config, targetMember.level);
    kv('CS1 当前等级', `Lv${targetMember.level}`);
    kv('CS1 推荐人', targetMember.referrer ? shortAddr(targetMember.referrer) : '无');
    kv('CS1 激活状态', boolLabel(targetMember.activated));
    kv('CS1 活跃状态', boolLabel(targetMember.isMemberActive));
    kv('CS1 封禁状态', boolLabel(targetMember.isBanned));
    kv('CS1 直推人数', String(targetMember.directReferrals));
    kv('CS1 团队人数', String(targetMember.teamSize));
    kv('CS1 累计消费', formatNex(targetMember.totalSpent));
    kv('CS1 可升级消费', formatNex(targetMember.upgradeEligibleSpent));
    kv('CS1 生效基础层数', `上=${targetEffective.baseUp} 下=${targetEffective.baseDown}`);
    kv('CS1 extra_levels', String(targetEffective.extraLevels));
    kv('CS1 有效层数', `上=${targetEffective.effectiveUp} 下=${targetEffective.effectiveDown}`);
    console.log('  [关键说明] SingleLine 的覆盖范围按“买家”的有效层数计算，不按收益接收人的层数计算。');
    await waitForEnter('已检查 CS1 当前等级与有效层数');

    subHeader('阶段 7：检查等级覆盖配置');
    const seenOverrides: Array<{ levelId: number; up: number; down: number; mark: string }> = [];
    for (let levelId = 0; levelId <= 10; levelId++) {
      const override = await getLevelOverride(api, args.entityId, levelId);
      if (!override.raw) continue;
      seenOverrides.push({
        levelId,
        up: override.upline,
        down: override.downline,
        mark: levelId === targetMember.level ? '<-- CS1 当前等级' : '',
      });
    }
    if (seenOverrides.length === 0) {
      console.log('  没有找到任何等级覆盖，当前都走基础层数。');
    } else {
      for (const item of seenOverrides) {
        console.log(`  Lv${item.levelId}: 上线=${item.up} 下线=${item.down} ${item.mark}`);
      }
    }
    await waitForEnter('已检查等级覆盖');

    subHeader('阶段 8：检查消费单链队列与索引');
    const addresses = new Set<string>([args.target, args.caseMember, chosenOrder.buyer, chosenOrder.seller, chosenOrder.payer].filter(Boolean));
    const queueSnapshot = new Map<string, QueueMember>();
    for (const address of addresses) {
      queueSnapshot.set(address, await getQueueMember(api, args.entityId, address));
    }

    for (const member of queueSnapshot.values()) {
      console.log(`  ${shortAddr(member.address)} index=${member.storedIndex ?? '不在队列'} removed=${member.removed ? '是' : '否'} level=Lv${member.level} activated=${boolLabel(member.activated)} active=${boolLabel(member.isMemberActive)} banned=${boolLabel(member.isBanned)}`);
    }

    const segmentCount = coerceNumber(codecToJson(await sl.singleLineSegmentCount(args.entityId))) ?? 0;
    kv('Segment 数量', String(segmentCount));
    console.log('  [说明] 最终索引判断统一以 singleLineIndex 为准，不使用 seg * 常量 + pos 作为结论。');
    await waitForEnter('已检查关键账户在 single-line 队列中的位置');

    subHeader('阶段 9：对照推荐关系链');
    const memberMap = new Map<string, MemberState>();
    for (const address of addresses) {
      memberMap.set(address, await getMemberState(api, args.entityId, address));
    }
    const referralPath = findReferralPath(memberMap, args.target, args.caseMember, args.maxDepth);
    if (referralPath.length > 0) {
      console.log(`  CS1 -> CS6 推荐路径: ${referralPath.map(shortAddr).join(' -> ')}`);
      console.log(`  推荐层级距离: ${referralPath.length - 1}`);
    } else {
      console.log('  未能仅凭当前已查询成员拼出完整 CS1 -> CS6 推荐路径，可能需要扩展查询更多成员。');
    }

    const targetIndex = queueSnapshot.get(args.target)?.storedIndex ?? null;
    const caseIndex = queueSnapshot.get(args.caseMember)?.storedIndex ?? null;
    if (targetIndex != null && caseIndex != null) {
      kv('消费单链距离(CS6 - CS1)', String(caseIndex - targetIndex));
      console.log(caseIndex > targetIndex
        ? '  说明：CS6 在 CS1 的消费单链下方。'
        : caseIndex < targetIndex
          ? '  说明：CS6 在 CS1 的消费单链上方。'
          : '  说明：CS6 与 CS1 消费单链位置相同（理论上不应出现）。');
    }
    console.log('  [关键说明] 推荐树第 N 层 ≠ 消费单链第 N 距离，这两个概念必须分开看。');
    await waitForEnter('已对照推荐关系和消费单链距离');

    subHeader('阶段 10：检查订单买家的覆盖范围、候选窗口与 skip 条件');
    const buyerMember = await getMemberState(api, args.entityId, chosenOrder.buyer);
    const buyerEffective = await getEffectiveLevels(api, args.entityId, chosenOrder.buyer, singleLine.config, buyerMember.level);
    const buyerQueue = await getQueueMember(api, args.entityId, chosenOrder.buyer);

    kv('订单买家', chosenOrder.buyer);
    kv('买家等级', `Lv${buyerMember.level}`);
    kv('买家 single-line index', buyerQueue.storedIndex != null ? String(buyerQueue.storedIndex) : '不在队列');
    kv('买家有效上线层数', String(buyerEffective.effectiveUp));
    kv('买家有效下线层数', String(buyerEffective.effectiveDown));

    const targetQueue = queueSnapshot.get(args.target)!;
    const reachMode = getReachModeName(singleLine.raw);
    let simulatedReach = false;
    let relativeDistance: number | null = null;
    let buyerCovers = false;
    let beneficiaryCovers = false;
    if (buyerQueue.storedIndex != null && targetQueue.storedIndex != null) {
      relativeDistance = targetQueue.storedIndex - buyerQueue.storedIndex;
      kv('CS1 相对买家距离', String(relativeDistance));
      if (relativeDistance > 0) {
        buyerCovers = relativeDistance <= buyerEffective.effectiveDown;
        const targetEffectiveForBeneficiary = await getEffectiveLevels(api, args.entityId, args.target, singleLine.config, targetMember.level);
        beneficiaryCovers = relativeDistance <= targetEffectiveForBeneficiary.effectiveUp;
        console.log(`  方向=下线，买家覆盖要求: 下线层数 >= ${relativeDistance}，结果=${buyerCovers ? '满足' : '不满足'}`);
        console.log(`  方向=下线，受益人反向覆盖要求: 上线层数 >= ${relativeDistance}，结果=${beneficiaryCovers ? '满足' : '不满足'}`);
      } else if (relativeDistance < 0) {
        const needed = Math.abs(relativeDistance);
        buyerCovers = needed <= buyerEffective.effectiveUp;
        const targetEffectiveForBeneficiary = await getEffectiveLevels(api, args.entityId, args.target, singleLine.config, targetMember.level);
        beneficiaryCovers = needed <= targetEffectiveForBeneficiary.effectiveDown;
        console.log(`  方向=上线，买家覆盖要求: 上线层数 >= ${needed}，结果=${buyerCovers ? '满足' : '不满足'}`);
        console.log(`  方向=上线，受益人反向覆盖要求: 下线层数 >= ${needed}，结果=${beneficiaryCovers ? '满足' : '不满足'}`);
      } else {
        console.log('  CS1 与买家是同一个账户，本案不适用。');
      }
    }

    if (reachMode === 'BuyerOnly') {
      simulatedReach = buyerCovers;
      console.log('  ReachMode 判定: BuyerOnly -> 只看买家覆盖。');
    } else if (reachMode === 'BeneficiaryOnly') {
      simulatedReach = beneficiaryCovers;
      console.log('  ReachMode 判定: BeneficiaryOnly -> 只看受益人反向覆盖。');
    } else if (reachMode === 'Bidirectional') {
      simulatedReach = buyerCovers || beneficiaryCovers;
      console.log('  ReachMode 判定: Bidirectional -> 买家覆盖或受益人反向覆盖任一满足即可。');
    } else {
      simulatedReach = buyerCovers;
      console.log(`  ReachMode 判定: ${reachMode}（未知枚举，暂按 BuyerOnly 辅助判断）`);
    }

    console.log('  [提醒] README 规定被跳过成员（banned / 未激活 / 不活跃 / removed）也会消耗 depth。');
    console.log(`  CS1 当前 skip 状态：removed=${targetQueue.removed ? '是' : '否'} activated=${boolLabel(targetQueue.activated)} active=${boolLabel(targetQueue.isMemberActive)} banned=${boolLabel(targetQueue.isBanned)}`);

    const chainByIndex = new Map<number, QueueMember>();
    for (const member of fullSingleLineChain) {
      if (member.storedIndex != null) {
        chainByIndex.set(member.storedIndex, member);
      }
    }

    if (buyerQueue.storedIndex != null) {
      console.log('\n  买家上线候选窗口：');
      for (let step = 1; step <= buyerEffective.effectiveUp; step++) {
        const idx = buyerQueue.storedIndex - step;
        const candidate = chainByIndex.get(idx);
        if (!candidate) {
          console.log(`    L${step}: index=${idx} 无成员`);
          continue;
        }
        const marks: string[] = [];
        if (candidate.address === args.target) marks.push('CS1');
        if (candidate.address === args.caseMember) marks.push('CS6');
        const skipReason = describeSkipReason(candidate);
        const recorded = commissionRecords.find((item) => item.commissionType === 'SingleLineUpline' && item.level === step && item.beneficiary === candidate.address);
        console.log(`    L${step}: index=${idx} ${candidate.address} | Lv${candidate.level} | ${skipReason} | ${recorded ? `链上已发 ${formatNex(recorded.amount)}` : '链上未发'}${marks.length ? ` | ${marks.join(', ')}` : ''}`);
      }

      console.log('\n  买家下线候选窗口：');
      for (let step = 1; step <= buyerEffective.effectiveDown; step++) {
        const idx = buyerQueue.storedIndex + step;
        const candidate = chainByIndex.get(idx);
        if (!candidate) {
          console.log(`    L${step}: index=${idx} 无成员`);
          continue;
        }
        const marks: string[] = [];
        if (candidate.address === args.target) marks.push('CS1');
        if (candidate.address === args.caseMember) marks.push('CS6');
        const skipReason = describeSkipReason(candidate);
        const recorded = commissionRecords.find((item) => item.commissionType === 'SingleLineDownline' && item.level === step && item.beneficiary === candidate.address);
        console.log(`    L${step}: index=${idx} ${candidate.address} | Lv${candidate.level} | ${skipReason} | ${recorded ? `链上已发 ${formatNex(recorded.amount)}` : '链上未发'}${marks.length ? ` | ${marks.join(', ')}` : ''}`);
      }

      if (relativeDistance != null && relativeDistance < 0) {
        const needed = Math.abs(relativeDistance);
        console.log(`\n  为什么到不了 CS1：`);
        console.log(`    - ReachMode = ${reachMode}`);
        console.log(`    - 买家 index = ${buyerQueue.storedIndex}`);
        console.log(`    - CS1 index = ${targetQueue.storedIndex}`);
        console.log(`    - CS1 位于买家上线第 ${needed} 层`);
        console.log(`    - 买家覆盖判定: ${buyerCovers ? '满足' : '不满足'}（买家有效上线=${buyerEffective.effectiveUp}）`);
        console.log(`    - 受益人反向覆盖判定: ${beneficiaryCovers ? '满足' : '不满足'}（CS1 有效下线=${targetEffective.effectiveDown}）`);
        console.log(`    - 当前模式最终结论: ${simulatedReach ? '理论可覆盖' : '理论不可覆盖'}`);
      } else if (relativeDistance != null && relativeDistance > 0) {
        const needed = relativeDistance;
        console.log(`\n  为什么到不了 CS1：`);
        console.log(`    - ReachMode = ${reachMode}`);
        console.log(`    - CS1 位于买家下线第 ${needed} 层`);
        console.log(`    - 买家覆盖判定: ${buyerCovers ? '满足' : '不满足'}（买家有效下线=${buyerEffective.effectiveDown}）`);
        console.log(`    - 受益人反向覆盖判定: ${beneficiaryCovers ? '满足' : '不满足'}（CS1 有效上线=${targetEffective.effectiveUp}）`);
        console.log(`    - 当前模式最终结论: ${simulatedReach ? '理论可覆盖' : '理论不可覆盖'}`);
      }
    }

    await waitForEnter('已检查买家覆盖范围、候选窗口与关键 skip 条件');

    subHeader('阶段 11：订单完成块历史态对照');
    const completionBlock = chosenOrder.completedAt;
    let historicalSummary = '未执行';
    if (completionBlock != null) {
      const completionHash = await getBlockHashByNumber(api, completionBlock);
      kv('订单完成区块', `#${completionBlock}`);
      kv('订单完成块哈希', completionHash ?? '获取失败');

      if (completionHash) {
        try {
          const apiAt = await api.at(completionHash);
          const historicalConfig = await getSingleLineConfigAt(apiAt, args.entityId);
          const historicalReachMode = getReachModeName(historicalConfig.raw);
          const historicalTargetMember = await getMemberStateAt(apiAt, args.entityId, args.target);
          const historicalBuyerMember = await getMemberStateAt(apiAt, args.entityId, chosenOrder.buyer);
          const historicalTargetQueue = await getQueueMemberAt(apiAt, args.entityId, args.target);
          const historicalBuyerQueue = await getQueueMemberAt(apiAt, args.entityId, chosenOrder.buyer);
          const historicalTargetEffective = await getEffectiveLevelsAt(apiAt, args.entityId, args.target, historicalConfig.raw, historicalTargetMember.level);
          const historicalBuyerEffective = await getEffectiveLevelsAt(apiAt, args.entityId, chosenOrder.buyer, historicalConfig.raw, historicalBuyerMember.level);

          console.log('  当前态 vs 完成块历史态：');
          console.log(`    ReachMode: 当前=${reachMode} | 历史=${historicalReachMode}`);
          console.log(`    CS1 等级: 当前=Lv${targetMember.level} | 历史=Lv${historicalTargetMember.level}`);
          console.log(`    买家等级: 当前=Lv${buyerMember.level} | 历史=Lv${historicalBuyerMember.level}`);
          console.log(`    CS1 index: 当前=${targetQueue.storedIndex ?? '无'} | 历史=${historicalTargetQueue.storedIndex ?? '无'}`);
          console.log(`    买家 index: 当前=${buyerQueue.storedIndex ?? '无'} | 历史=${historicalBuyerQueue.storedIndex ?? '无'}`);
          console.log(`    CS1 activated: 当前=${boolLabel(targetMember.activated)} | 历史=${boolLabel(historicalTargetMember.activated)}`);
          console.log(`    CS1 removed: 当前=${targetQueue.removed ? '是' : '否'} | 历史=${historicalTargetQueue.removed ? '是' : '否'}`);
          console.log(`    买家 activated: 当前=${boolLabel(buyerMember.activated)} | 历史=${boolLabel(historicalBuyerMember.activated)}`);
          console.log(`    买家 removed: 当前=${buyerQueue.removed ? '是' : '否'} | 历史=${historicalBuyerQueue.removed ? '是' : '否'}`);
          if (historicalTargetEffective && historicalBuyerEffective) {
            console.log(`    CS1 有效层数: 当前=上${targetEffective.effectiveUp}/下${targetEffective.effectiveDown} | 历史=上${historicalTargetEffective.effectiveUp}/下${historicalTargetEffective.effectiveDown}`);
            console.log(`    买家有效层数: 当前=上${buyerEffective.effectiveUp}/下${buyerEffective.effectiveDown} | 历史=上${historicalBuyerEffective.effectiveUp}/下${historicalBuyerEffective.effectiveDown}`);
          }

          historicalSummary = `ReachMode历史=${historicalReachMode}, CS1历史等级=Lv${historicalTargetMember.level}, CS1历史index=${historicalTargetQueue.storedIndex ?? '无'}, 买家历史等级=Lv${historicalBuyerMember.level}, 买家历史index=${historicalBuyerQueue.storedIndex ?? '无'}`;
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          console.log(`  [提示] 无法读取完成块历史状态：${message}`);
          console.log('  这通常表示远端节点已经裁剪旧状态，只保留了块哈希但没有对应 state。');
          historicalSummary = `历史状态读取失败: ${message}`;
        }
      } else {
        historicalSummary = '无法获取完成块哈希';
      }
    } else {
      console.log('  订单没有 completedAt，无法做历史块对照。');
      historicalSummary = '订单没有完成区块';
    }
    await waitForEnter('已检查订单完成块历史态');

    subHeader('阶段 12：结合链上记录给出模拟结论');
    const facts: string[] = [];
    const inferences: string[] = [];
    const rootCauses: string[] = [];
    const nextSteps: string[] = [];

    facts.push(`目标订单 #${chosenOrder.orderId} 中，CS1 直推奖记录数 = ${targetDirect.length}`);
    facts.push(`目标订单 #${chosenOrder.orderId} 中，CS1 下线排线奖记录数 = ${targetDownline.length}`);
    facts.push(`订单买家有效层数：上线=${buyerEffective.effectiveUp}，下线=${buyerEffective.effectiveDown}`);
    facts.push(`当前 ReachMode = ${reachMode}`);
    facts.push(`CS1 当前消费单链位置 = ${targetQueue.storedIndex ?? '不在队列'}；买家位置 = ${buyerQueue.storedIndex ?? '不在队列'}`);
    facts.push(`历史态摘要 = ${historicalSummary}`);

    if (targetDownline.length === 0 && simulatedReach) {
      inferences.push(`按当前 ReachMode=${reachMode} 模拟，CS1 理论上应在覆盖窗口内，但链上没有对应 SingleLineDownline 记录。`);
      rootCauses.push('疑似链端 SingleLine 结算逻辑异常，或订单发生时历史状态与当前状态不同。');
      nextSteps.push('优先按订单完成区块做 api.at(blockHash) 历史查询，核实当时的等级、index、激活状态和配置。');
    } else if (targetDownline.length === 0 && !simulatedReach) {
      inferences.push(`按当前 ReachMode=${reachMode} 模拟，CS1 不在最终覆盖窗口内。`);
      rootCauses.push('更可能是 reach mode 配置 + 推荐树层级/消费单链距离差异共同导致未命中，而不一定是结算 bug。');
      nextSteps.push('继续扩展脚本，列出买家窗口内的完整上下线候选成员，逐个标注 skip 原因。');
    } else {
      inferences.push('链上已经存在 CS1 的 SingleLineDownline 记录，本案需要进一步核对用户关注的是哪一笔订单或哪一次结算。');
      rootCauses.push('可能分析订单选错，或问题发生在另一笔相关订单上。');
      nextSteps.push('用 --order 指定用户截图对应的精确订单，再重跑脚本。');
    }

    if (singleLine.pending) {
      rootCauses.push('存在 PendingConfigChanges，当前配置可能并非订单发生时实际生效的配置。');
    }

    header('最终摘要');
    console.log('  事实：');
    for (const fact of facts) console.log(`    - ${fact}`);
    console.log('  推断：');
    for (const item of inferences) console.log(`    - ${item}`);
    console.log('  最可能根因：');
    for (const item of rootCauses) console.log(`    - ${item}`);
    console.log('  下一步建议：');
    for (const item of nextSteps) console.log(`    - ${item}`);

  } finally {
    close();
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error('执行失败:', error);
  process.exit(1);
});
