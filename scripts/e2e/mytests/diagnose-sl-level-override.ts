#!/usr/bin/env tsx
/**
 * diagnose-sl-level-override.ts
 *
 * 诊断 CS1 账号排线奖（SingleLine）等级层数覆盖问题:
 *   - Lv6 配置下线 6 层，但实际只拿到下线 3 层
 *
 * 逐步排查：
 *   1. SingleLine 基础配置 & 等级层数覆盖
 *   2. CS1 会员信息 & 排线位置
 *   3. CS1 下游成员链 (referral chain + SL queue)
 *   4. 所有订单佣金记录分析
 *   5. 模拟 effective_base_levels 逻辑，定位 bug 根因
 *
 * 关键节点需要键盘确认后继续。
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { createInterface } from 'node:readline';
import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

/* ─── 目标账户 ─── */
const CS1_ADDRESS = 'X4XLb5Z7rDcjhQ6EajaLXXdPwGBDkBTES7FpALx1WHKrX7pHr';
const ENTITY_ID = 100000;

/* ─── 工具函数 ─── */
function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}
function ln(ch = '═', len = 86): string { return ch.repeat(len); }
function header(text: string): void {
  console.log(`\n${ln()}`);
  console.log(`  ${text}`);
  console.log(ln());
}
function sub(text: string): void {
  console.log(`\n  ${ln('─', 78)}`);
  console.log(`  ${text}`);
  console.log(`  ${ln('─', 78)}`);
}

const rl = createInterface({ input: process.stdin, output: process.stdout });
const isInteractive = process.stdin.isTTY === true;

function waitForEnter(prompt: string): Promise<void> {
  if (!isInteractive) {
    console.log(`\n  -- ${prompt} (非交互模式，自动继续) --`);
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    rl.question(`\n  >>  ${prompt}，按 Enter 继续 ...`, () => resolve());
  });
}

/* ─── Main ─── */
async function main(): Promise<void> {
  const api = await connectApi();

  try {
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    const sl = (api.query as any).commissionSingleLine;
    const cc = (api.query as any).commissionCore;
    const mb = (api.query as any).entityMember;
    const tx = (api.query as any).entityTransaction;

    header(`排线奖等级层数覆盖诊断  |  Block #${currentBlock}`);
    console.log(`  目标账户: ${CS1_ADDRESS}`);
    console.log(`  实体 ID:  ${ENTITY_ID}`);

    // ================================================================
    // 1. SingleLine 基础配置
    // ================================================================
    sub('1. SingleLine 基础配置');

    let slConfig: Record<string, unknown> | null = null;
    try {
      const raw = await sl.singleLineConfigs(ENTITY_ID);
      if (raw && !(raw as any).isNone) {
        slConfig = codecToJson<Record<string, unknown>>(
          (raw as any).unwrap ? (raw as any).unwrap() : raw,
        );
      }
    } catch {}

    if (!slConfig) {
      console.log('  [错误] SingleLine 未配置，无法继续');
      return;
    }

    const uplineRate = coerceNumber(readObjectField(slConfig, 'uplineRate', 'upline_rate')) ?? 0;
    const downlineRate = coerceNumber(readObjectField(slConfig, 'downlineRate', 'downline_rate')) ?? 0;
    const baseUp = coerceNumber(readObjectField(slConfig, 'baseUplineLevels', 'base_upline_levels')) ?? 0;
    const baseDown = coerceNumber(readObjectField(slConfig, 'baseDownlineLevels', 'base_downline_levels')) ?? 0;
    const maxUp = coerceNumber(readObjectField(slConfig, 'maxUplineLevels', 'max_upline_levels')) ?? 0;
    const maxDown = coerceNumber(readObjectField(slConfig, 'maxDownlineLevels', 'max_downline_levels')) ?? 0;
    const threshold = asBigInt(readObjectField(slConfig, 'levelIncrementThreshold', 'level_increment_threshold') ?? 0);

    console.log(`  上线费率 (uplineRate):          ${uplineRate} bps (${(uplineRate / 100).toFixed(1)}%)`);
    console.log(`  下线费率 (downlineRate):        ${downlineRate} bps (${(downlineRate / 100).toFixed(1)}%)`);
    console.log(`  基础上线层数 (baseUplineLevels):  ${baseUp}`);
    console.log(`  基础下线层数 (baseDownlineLevels): ${baseDown}`);
    console.log(`  最大上线层数 (maxUplineLevels):   ${maxUp}`);
    console.log(`  最大下线层数 (maxDownlineLevels):  ${maxDown}`);
    console.log(`  升级阈值 (levelIncrementThreshold): ${threshold > 0n ? formatNex(threshold) : '无'}`);

    let slEnabled = true;
    try {
      slEnabled = codecToJson<boolean>(await sl.singleLineEnabled(ENTITY_ID)) ?? true;
    } catch {}
    console.log(`  排线启用状态: ${slEnabled ? '已启用' : '已暂停 ⚠️'}`);

    // ================================================================
    // 2. 等级层数覆盖配置 (逐个等级查询)
    // ================================================================
    sub('2. 等级层数覆盖配置 (SingleLineCustomLevelOverrides)');

    // 逐个等级查询覆盖配置 (Lv0 ~ Lv7)
    const overrides: { levelId: number; upline: number; downline: number }[] = [];
    for (let lvl = 0; lvl <= 7; lvl++) {
      try {
        const raw = await sl.singleLineCustomLevelOverrides(ENTITY_ID, lvl);
        if (raw && !(raw as any).isNone) {
          const data = codecToJson<Record<string, unknown>>(
            (raw as any).unwrap ? (raw as any).unwrap() : raw,
          );
          const up = coerceNumber(readObjectField(data, 'uplineLevels', 'upline_levels')) ?? 0;
          const down = coerceNumber(readObjectField(data, 'downlineLevels', 'downline_levels')) ?? 0;
          if (up > 0 || down > 0) {
            overrides.push({ levelId: lvl, upline: up, downline: down });
          }
        }
      } catch {}
    }

    if (overrides.length === 0) {
      console.log('  [!] 没有找到任何等级层数覆盖配置');
      console.log(`  所有等级均使用基础层数: 上线=${baseUp}, 下线=${baseDown}`);
    } else {
      console.log(`  ${'等级ID'.padEnd(10)} ${'上线层数'.padEnd(12)} ${'下线层数'.padEnd(12)}`);
      console.log(`  ${'──────'.padEnd(10)} ${'────────'.padEnd(12)} ${'────────'.padEnd(12)}`);
      for (const o of overrides) {
        const mark = o.levelId === 6 ? '  <-- Lv6 (CS1 等级)' : '';
        console.log(`  ${`Lv${o.levelId}`.padEnd(10)} ${String(o.upline).padEnd(12)} ${String(o.downline).padEnd(12)}${mark}`);
      }
      console.log(`\n  基础配置 (无覆盖时): 上线=${baseUp}, 下线=${baseDown}`);
      console.log(`  最大限制: 上线=${maxUp}, 下线=${maxDown}`);

      const lv6 = overrides.find(o => o.levelId === 6);
      if (lv6) {
        console.log(`\n  [OK] Lv6 覆盖存在: 上线=${lv6.upline}, 下线=${lv6.downline}`);
        if (lv6.downline <= baseDown) {
          console.log(`  [!] Lv6 覆盖的下线层数 (${lv6.downline}) <= 基础下线层数 (${baseDown})，覆盖无意义!`);
        }
      } else {
        console.log(`\n  [!] Lv6 没有层数覆盖! CS1 使用基础层数: 上线=${baseUp}, 下线=${baseDown}`);
      }
    }

    await waitForEnter('已检查配置');

    // ================================================================
    // 3. CS1 会员信息
    // ================================================================
    sub('3. CS1 会员信息');

    let cs1Member: Record<string, unknown> | null = null;
    try {
      const raw = await mb.entityMembers(ENTITY_ID, CS1_ADDRESS);
      if (raw && !(raw as any).isNone) {
        cs1Member = codecToJson<Record<string, unknown>>((raw as any).unwrap());
      }
    } catch {}

    if (!cs1Member) {
      console.log('  [错误] CS1 不是该实体的会员');
      return;
    }

    const cs1Level = coerceNumber(readObjectField(cs1Member, 'customLevelId', 'custom_level_id')) ?? 0;
    const cs1Referrer = String(readObjectField(cs1Member, 'referrer') ?? '');
    const cs1DirectRefs = coerceNumber(readObjectField(cs1Member, 'directReferrals', 'direct_referrals')) ?? 0;
    const cs1IndirectRefs = coerceNumber(readObjectField(cs1Member, 'indirectReferrals', 'indirect_referrals')) ?? 0;
    const cs1TeamSize = coerceNumber(readObjectField(cs1Member, 'teamSize', 'team_size')) ?? 0;
    const cs1Activated = readObjectField(cs1Member, 'activated') ?? readObjectField(cs1Member, 'isActivated', 'is_activated');
    const cs1TotalSpent = coerceNumber(readObjectField(cs1Member, 'totalSpent', 'total_spent')) ?? 0;
    const cs1UpgradeSpent = coerceNumber(readObjectField(cs1Member, 'upgradeEligibleSpent', 'upgrade_eligible_spent')) ?? 0;

    console.log(`  地址:     ${CS1_ADDRESS}`);
    console.log(`  等级ID (custom_level_id): ${cs1Level}`);
    console.log(`  推荐人:   ${cs1Referrer ? shortAddr(cs1Referrer) : '(无)'}`);
    console.log(`  直推人数: ${cs1DirectRefs}`);
    console.log(`  间推人数: ${cs1IndirectRefs}`);
    console.log(`  团队人数: ${cs1TeamSize}`);
    console.log(`  激活状态: ${JSON.stringify(cs1Activated)}`);
    console.log(`  累计消费: ${cs1TotalSpent} (USDT精度)`);
    console.log(`  可升级消费: ${cs1UpgradeSpent}`);
    console.log(`  原始数据: ${JSON.stringify(cs1Member)}`);

    // 查询 CS1 的 SL 位置
    const cs1IndexRaw = codecToJson(await sl.singleLineIndex(ENTITY_ID, CS1_ADDRESS));
    const cs1Index = coerceNumber(cs1IndexRaw);
    console.log(`  排线位置: ${cs1Index !== undefined ? `#${cs1Index}` : '不在排线中 ⚠️'}`);

    // CS1 佣金统计
    const cs1Stats = codecToJson<Record<string, unknown>>(
      await cc.memberCommissionStats(ENTITY_ID, CS1_ADDRESS),
    );
    const cs1Earned = asBigInt(readObjectField(cs1Stats, 'totalEarned', 'total_earned') ?? 0);
    console.log(`  佣金总收入: ${formatNex(cs1Earned)}`);

    // SL 佣金统计
    let slStats: Record<string, unknown> | null = null;
    try {
      slStats = codecToJson<Record<string, unknown>>(
        await sl.memberSingleLineStats(ENTITY_ID, CS1_ADDRESS),
      );
    } catch {}
    if (slStats) {
      const slUpEarned = asBigInt(readObjectField(slStats, 'totalEarnedAsUpline', 'total_earned_as_upline') ?? 0);
      const slDownEarned = asBigInt(readObjectField(slStats, 'totalEarnedAsDownline', 'total_earned_as_downline') ?? 0);
      console.log(`  排线上线收入: ${formatNex(slUpEarned)}`);
      console.log(`  排线下线收入: ${formatNex(slDownEarned)}`);
    }

    // ================================================================
    // 3b. 模拟 effective_base_levels 逻辑
    // ================================================================
    sub('3b. 模拟 effective_base_levels (CS1 作为买家时的有效层数)');

    console.log(`  CS1 等级ID (custom_level_id): ${cs1Level}`);

    // 查询 CS1 等级对应的覆盖
    let cs1OverrideUp = baseUp;
    let cs1OverrideDown = baseDown;
    let cs1HasOverride = false;

    try {
      const overrideRaw = await sl.singleLineCustomLevelOverrides(ENTITY_ID, cs1Level);
      const overrideData = codecToJson<Record<string, unknown>>(
        overrideRaw && (overrideRaw as any).unwrap ? (overrideRaw as any).unwrap() : overrideRaw,
      );
      if (overrideData) {
        const oUp = coerceNumber(readObjectField(overrideData, 'uplineLevels', 'upline_levels'));
        const oDown = coerceNumber(readObjectField(overrideData, 'downlineLevels', 'downline_levels'));
        if (oUp !== undefined && oUp > 0) { cs1OverrideUp = oUp; cs1HasOverride = true; }
        if (oDown !== undefined && oDown > 0) { cs1OverrideDown = oDown; cs1HasOverride = true; }
      }
    } catch {}

    if (cs1HasOverride) {
      console.log(`  ✅ 等级 ${cs1Level} 有覆盖: 上线=${cs1OverrideUp}, 下线=${cs1OverrideDown}`);
    } else {
      console.log(`  ❌ 等级 ${cs1Level} 无覆盖，使用基础值: 上线=${baseUp}, 下线=${baseDown}`);
    }

    // calc_extra_levels
    let cs1ExtraLevels = 0;
    if (threshold > 0n && cs1Earned > 0n) {
      cs1ExtraLevels = Number(cs1Earned / threshold);
    }
    console.log(`  extra_levels (基于佣金收入): ${cs1ExtraLevels}`);

    // 最终 max_levels (同 process_downline 逻辑)
    const cs1EffectiveDown = Math.min(cs1OverrideDown + cs1ExtraLevels, maxDown);
    const cs1EffectiveUp = Math.min(cs1OverrideUp + cs1ExtraLevels, maxUp);
    console.log(`\n  === CS1 作为买家时的有效层数 ===`);
    console.log(`  上线: min(${cs1OverrideUp} + ${cs1ExtraLevels}, ${maxUp}) = ${cs1EffectiveUp}`);
    console.log(`  下线: min(${cs1OverrideDown} + ${cs1ExtraLevels}, ${maxDown}) = ${cs1EffectiveDown}`);

    console.log(`\n  ⚠️ 重要: 排线佣金是以 **买家** 的有效层数来分发的`);
    console.log(`     当 CS1 的下线成员 A 购买时，佣金分发使用的是 A 的有效层数，不是 CS1 的`);
    console.log(`     CS1 能否收到 A 的排线佣金，取决于 A 的上线层数是否覆盖到 CS1 的位置`);

    await waitForEnter('已检查 CS1 会员信息和有效层数');

    // ================================================================
    // 4. 排线队列全览 & 推荐链
    // ================================================================
    sub('4. 排线队列全览');

    const segCount = coerceNumber(codecToJson(await sl.singleLineSegmentCount(ENTITY_ID))) ?? 0;
    console.log(`  段数: ${segCount}`);

    type QueueMember = {
      index: number;
      addr: string;
      removed: boolean;
      memberLevel: number;
      referrer: string;
    };
    const queue: QueueMember[] = [];

    for (let seg = 0; seg < segCount; seg++) {
      const members = codecToJson<string[]>(await sl.singleLineSegments(ENTITY_ID, seg)) ?? [];
      for (let pos = 0; pos < members.length; pos++) {
        const addr = String(members[pos]);
        const globalIdx = seg * 1000 + pos;
        let removed = false;
        try {
          removed = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, addr)) ?? false;
        } catch {}

        // 查询会员等级和推荐人
        let memberLevel = 0;
        let referrer = '';
        try {
          const memRaw = await mb.entityMembers(ENTITY_ID, addr);
          if (memRaw && !(memRaw as any).isNone) {
            const memData = codecToJson<Record<string, unknown>>((memRaw as any).unwrap());
            memberLevel = coerceNumber(readObjectField(memData, 'customLevelId', 'custom_level_id')) ?? 0;
            referrer = String(readObjectField(memData, 'referrer') ?? '');
          }
        } catch {}

        queue.push({ index: globalIdx, addr, removed, memberLevel, referrer });
      }
    }

    console.log(`  队列总长: ${queue.length}\n`);
    console.log(`  ${'位置'.padStart(6)} ${'地址'.padEnd(52)} ${'等级'.padEnd(6)} ${'推荐人'.padEnd(18)} ${'已移除'.padEnd(6)} 备注`);
    console.log(`  ${'────'.padStart(6)} ${'────'.padEnd(52)} ${'────'.padEnd(6)} ${'──────'.padEnd(18)} ${'────'.padEnd(6)} ────`);

    for (const m of queue) {
      let notes = '';
      if (m.addr === CS1_ADDRESS) notes += ' ← CS1 目标';
      if (m.removed) notes += ' [已移除]';
      if (m.referrer === CS1_ADDRESS) notes += ' [CS1直推]';

      console.log(
        `  ${`#${m.index}`.padStart(6)} ${m.addr.padEnd(52)} ${`Lv${m.memberLevel}`.padEnd(6)} ${shortAddr(m.referrer).padEnd(18)} ${(m.removed ? '是' : '否').padEnd(6)}${notes}`,
      );
    }

    // 找出 CS1 在队列中的位置
    const cs1InQueue = queue.find(m => m.addr === CS1_ADDRESS);
    if (!cs1InQueue) {
      console.log('\n  ⚠️ CS1 不在排线队列中，无法获得排线佣金');
      return;
    }

    // 找出 CS1 下游 (队列位置 > CS1) 的成员
    const downlineMembers = queue.filter(m => m.index > cs1InQueue.index && !m.removed);
    const uplineMembers = queue.filter(m => m.index < cs1InQueue.index && !m.removed);

    console.log(`\n  CS1 位置: #${cs1InQueue.index}`);
    console.log(`  CS1 上方有效成员 (上线方向): ${uplineMembers.length} 人`);
    console.log(`  CS1 下方有效成员 (下线方向): ${downlineMembers.length} 人`);

    await waitForEnter('已检查排线队列');

    // ================================================================
    // 5. 分析每个买家的有效层数 → CS1 能否被覆盖
    // ================================================================
    sub('5. 分析每笔订单的买家有效层数');

    const nextOrderId = coerceNumber(codecToJson(await tx.nextOrderId())) ?? 0;
    console.log(`  扫描订单 0 ~ ${nextOrderId - 1} ...\n`);

    type OrderInfo = {
      orderId: number;
      buyer: string;
      buyerLevel: number;
      buyerIndex: number | null;
      buyerEffectiveUp: number;
      buyerEffectiveDown: number;
      totalAmount: bigint;
      status: string;
      slRecords: { beneficiary: string; level: number; amount: bigint; type: string }[];
    };

    const relevantOrders: OrderInfo[] = [];

    for (let oid = 0; oid < nextOrderId; oid++) {
      let order: Record<string, unknown> | null = null;
      try {
        const raw = await tx.orders(oid);
        if (raw && !(raw as any).isNone) {
          order = codecToJson<Record<string, unknown>>((raw as any).unwrap());
        }
      } catch {}
      if (!order) continue;

      const entityId = coerceNumber(readObjectField(order, 'entityId', 'entity_id'));
      if (entityId !== ENTITY_ID) continue;

      const buyer = String(readObjectField(order, 'buyer') ?? '');
      const status = String(readObjectField(order, 'status') ?? '');
      const totalAmount = asBigInt(readObjectField(order, 'totalAmount', 'total_amount') ?? 0);

      // 买家的排线位置
      const buyerIndexRaw = codecToJson(await sl.singleLineIndex(ENTITY_ID, buyer));
      const buyerIndex = coerceNumber(buyerIndexRaw) ?? null;

      // 买家等级
      let buyerLevel = 0;
      try {
        const memRaw = await mb.entityMembers(ENTITY_ID, buyer);
        if (memRaw && !(memRaw as any).isNone) {
          const memData = codecToJson<Record<string, unknown>>((memRaw as any).unwrap());
          buyerLevel = coerceNumber(readObjectField(memData, 'customLevelId', 'custom_level_id')) ?? 0;
        }
      } catch {}

      // 模拟 effective_base_levels(buyer)
      let bOverrideUp = baseUp;
      let bOverrideDown = baseDown;
      try {
        const oRaw = await sl.singleLineCustomLevelOverrides(ENTITY_ID, buyerLevel);
        if (oRaw && !(oRaw as any).isNone) {
          const oData = codecToJson<Record<string, unknown>>(
            (oRaw as any).unwrap ? (oRaw as any).unwrap() : oRaw,
          );
          if (oData) {
            const u = coerceNumber(readObjectField(oData, 'uplineLevels', 'upline_levels'));
            const d = coerceNumber(readObjectField(oData, 'downlineLevels', 'downline_levels'));
            if (u !== undefined && u > 0) bOverrideUp = u;
            if (d !== undefined && d > 0) bOverrideDown = d;
          }
        }
      } catch {}

      // 买家佣金收入 (用于 calc_extra_levels)
      let buyerEarned = 0n;
      try {
        const bStats = codecToJson<Record<string, unknown>>(
          await cc.memberCommissionStats(ENTITY_ID, buyer),
        );
        buyerEarned = asBigInt(readObjectField(bStats, 'totalEarned', 'total_earned') ?? 0);
      } catch {}

      let buyerExtra = 0;
      if (threshold > 0n && buyerEarned > 0n) {
        buyerExtra = Number(buyerEarned / threshold);
      }

      const buyerEffectiveUp = Math.min(bOverrideUp + buyerExtra, maxUp);
      const buyerEffectiveDown = Math.min(bOverrideDown + buyerExtra, maxDown);

      // 查询该订单的 SL 佣金记录
      const recs = codecToJson<any[]>(await cc.orderCommissionRecords(oid)) ?? [];
      const slRecs: OrderInfo['slRecords'] = [];
      for (const rec of recs) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
        if (!ctype.includes('SingleLine')) continue;
        slRecs.push({
          beneficiary: String(readObjectField(rec, 'beneficiary') ?? ''),
          level: coerceNumber(readObjectField(rec, 'level')) ?? 0,
          amount: asBigInt(readObjectField(rec, 'amount') ?? 0),
          type: ctype,
        });
      }

      relevantOrders.push({
        orderId: oid,
        buyer,
        buyerLevel,
        buyerIndex,
        buyerEffectiveUp,
        buyerEffectiveDown,
        totalAmount,
        status,
        slRecords: slRecs,
      });
    }

    console.log(`  找到 ${relevantOrders.length} 笔订单\n`);

    for (const o of relevantOrders) {
      const distToCS1 = (cs1Index !== undefined && o.buyerIndex !== null)
        ? cs1Index - o.buyerIndex
        : null;

      const direction = distToCS1 !== null
        ? (distToCS1 > 0 ? '下线方向 (buyer在CS1上方)' : distToCS1 < 0 ? '上线方向 (buyer在CS1下方)' : '买家=CS1')
        : '未知';

      const canReachCS1 = distToCS1 !== null && distToCS1 > 0
        ? distToCS1 <= o.buyerEffectiveDown
        : distToCS1 !== null && distToCS1 < 0
          ? Math.abs(distToCS1) <= o.buyerEffectiveUp
          : false;

      // CS1 在该订单中收到的 SL 记录
      const cs1Recs = o.slRecords.filter(r => r.beneficiary === CS1_ADDRESS);

      console.log(`  ── 订单 #${o.orderId} ──`);
      console.log(`    买家: ${shortAddr(o.buyer)} (Lv${o.buyerLevel})`);
      console.log(`    买家排线位置: ${o.buyerIndex !== null ? `#${o.buyerIndex}` : '不在队列'}`);
      console.log(`    买家有效层数: 上线=${o.buyerEffectiveUp}, 下线=${o.buyerEffectiveDown}`);
      console.log(`    订单金额: ${formatNex(o.totalAmount)}, 状态: ${o.status}`);
      console.log(`    CS1 距离买家: ${distToCS1 !== null ? distToCS1 : 'N/A'} (${direction})`);
      console.log(`    买家能覆盖到 CS1: ${canReachCS1 ? '是 ✅' : '否 ❌'}`);

      if (cs1Recs.length > 0) {
        for (const r of cs1Recs) {
          console.log(`    → CS1 收到: ${r.type} L${r.level} ${formatNex(r.amount)}`);
        }
      } else {
        if (canReachCS1) {
          console.log(`    ⚠️ 买家理论上能覆盖 CS1 但 CS1 未收到佣金!`);
        } else {
          console.log(`    (CS1 不在覆盖范围内，未收到正常)`);
        }
      }

      // 该订单的全部 SL 记录
      if (o.slRecords.length > 0) {
        console.log(`    该订单全部排线佣金 (${o.slRecords.length} 条):`);
        // 按 type + level 排序
        const sorted = [...o.slRecords].sort((a, b) => {
          if (a.type !== b.type) return a.type < b.type ? -1 : 1;
          return a.level - b.level;
        });
        for (const r of sorted) {
          const isCS1 = r.beneficiary === CS1_ADDRESS ? ' ← CS1' : '';
          const memberInQueue = queue.find(m => m.addr === r.beneficiary);
          const pos = memberInQueue ? `#${memberInQueue.index}` : '?';
          console.log(`      ${r.type.replace('SingleLine', 'SL')} L${r.level} => ${shortAddr(r.beneficiary)} (${pos}) ${formatNex(r.amount)}${isCS1}`);
        }

        // 检查最大下线层数
        const downlineRecs = o.slRecords.filter(r => r.type === 'SingleLineDownline');
        const maxDownlineLevel = downlineRecs.reduce((max, r) => Math.max(max, r.level), 0);
        const uplineRecs = o.slRecords.filter(r => r.type === 'SingleLineUpline');
        const maxUplineLevel = uplineRecs.reduce((max, r) => Math.max(max, r.level), 0);

        console.log(`    实际分发: 上线最远 L${maxUplineLevel}/${o.buyerEffectiveUp}, 下线最远 L${maxDownlineLevel}/${o.buyerEffectiveDown}`);

        if (maxDownlineLevel < o.buyerEffectiveDown && o.buyerIndex !== null) {
          const queueLen = queue.length;
          const maxPossible = queueLen - 1 - o.buyerIndex;
          if (maxPossible < o.buyerEffectiveDown) {
            console.log(`    (队列长度限制: 买家下方仅有 ${maxPossible} 人)`);
          } else {
            console.log(`    ⚠️ 下线分发未达到有效层数上限! 可能有 bug`);
          }
        }
      } else {
        console.log(`    (该订单无排线佣金记录)`);
      }
      console.log();
    }

    await waitForEnter('已分析所有订单的买家有效层数');

    // ================================================================
    // 6. 核心问题验证: CS1 购买订单时的分发范围
    // ================================================================
    sub('6. CS1 作为买家时的佣金分发分析');

    const cs1Orders = relevantOrders.filter(o => o.buyer === CS1_ADDRESS);
    if (cs1Orders.length === 0) {
      console.log('  CS1 没有作为买家的订单');
    } else {
      for (const o of cs1Orders) {
        console.log(`  订单 #${o.orderId}: 金额=${formatNex(o.totalAmount)}`);
        console.log(`  CS1 有效层数: 上线=${o.buyerEffectiveUp}, 下线=${o.buyerEffectiveDown}`);

        const downlineRecs = o.slRecords.filter(r => r.type === 'SingleLineDownline');
        const uplineRecs = o.slRecords.filter(r => r.type === 'SingleLineUpline');

        console.log(`  实际分发上线: ${uplineRecs.length} 条 (最远 L${uplineRecs.reduce((m, r) => Math.max(m, r.level), 0)})`);
        console.log(`  实际分发下线: ${downlineRecs.length} 条 (最远 L${downlineRecs.reduce((m, r) => Math.max(m, r.level), 0)})`);
      }
    }

    // ================================================================
    // 7. 从 CS1 下线成员购买的角度反查
    // ================================================================
    sub('7. CS1 推荐链下游成员购买时的排线覆盖分析');

    console.log('  查找 CS1 直接或间接推荐的成员及其购买情况...\n');

    // 通过 referrer 链找出 CS1 的下游成员
    const cs1Referrals = queue.filter(m => m.referrer === CS1_ADDRESS);
    console.log(`  CS1 直推且在排线中的成员: ${cs1Referrals.length} 人`);

    // BFS 找出完整推荐树
    const referralTree = new Map<string, string[]>(); // parent => children
    for (const m of queue) {
      if (!m.referrer) continue;
      const children = referralTree.get(m.referrer) ?? [];
      children.push(m.addr);
      referralTree.set(m.referrer, children);
    }

    // BFS from CS1
    const cs1Team: QueueMember[] = [];
    const visited = new Set<string>();
    const bfsQueue = [CS1_ADDRESS];
    visited.add(CS1_ADDRESS);

    while (bfsQueue.length > 0) {
      const current = bfsQueue.shift()!;
      const children = referralTree.get(current) ?? [];
      for (const child of children) {
        if (visited.has(child)) continue;
        visited.add(child);
        bfsQueue.push(child);
        const member = queue.find(m => m.addr === child);
        if (member) cs1Team.push(member);
      }
    }

    console.log(`  CS1 推荐树中在排线的成员: ${cs1Team.length} 人\n`);

    // 分析每个下游成员的购买情况和排线关系
    for (const member of cs1Team) {
      const dist = (cs1Index !== undefined) ? member.index - cs1Index : null;
      const memberOrders = relevantOrders.filter(o => o.buyer === member.addr);

      // 查询该成员的有效层数
      let mOverrideUp = baseUp;
      try {
        const oRaw = await sl.singleLineCustomLevelOverrides(ENTITY_ID, member.memberLevel);
        if (oRaw && !(oRaw as any).isNone) {
          const oData = codecToJson<Record<string, unknown>>(
            (oRaw as any).unwrap ? (oRaw as any).unwrap() : oRaw,
          );
          if (oData) {
            const u = coerceNumber(readObjectField(oData, 'uplineLevels', 'upline_levels'));
            if (u !== undefined && u > 0) mOverrideUp = u;
          }
        }
      } catch {}

      let mEarned = 0n;
      try {
        const s = codecToJson<Record<string, unknown>>(
          await cc.memberCommissionStats(ENTITY_ID, member.addr),
        );
        mEarned = asBigInt(readObjectField(s, 'totalEarned', 'total_earned') ?? 0);
      } catch {}
      let mExtra = 0;
      if (threshold > 0n && mEarned > 0n) mExtra = Number(mEarned / threshold);
      const mEffectiveUp = Math.min(mOverrideUp + mExtra, maxUp);

      const reachesCS1 = dist !== null && dist > 0 && dist <= mEffectiveUp;

      console.log(`  ${shortAddr(member.addr)} (Lv${member.memberLevel}) 排线#${member.index}`);
      console.log(`    距CS1: ${dist ?? 'N/A'} 位 (${dist !== null && dist > 0 ? '在CS1下方' : 'CS1上方或相同'})`);
      console.log(`    有效上线层数: ${mEffectiveUp} (base=${mOverrideUp} + extra=${mExtra}, max=${maxUp})`);
      console.log(`    能覆盖到CS1: ${reachesCS1 ? '是 ✅' : '否 ❌'} (需要上线层数 ≥ ${dist})`);
      console.log(`    购买订单数: ${memberOrders.length}`);

      for (const o of memberOrders) {
        const cs1Recs = o.slRecords.filter(r => r.beneficiary === CS1_ADDRESS);
        console.log(`      订单 #${o.orderId}: ${formatNex(o.totalAmount)} | CS1收到: ${cs1Recs.length > 0 ? cs1Recs.map(r => `${r.type.replace('SingleLine', 'SL')} L${r.level} ${formatNex(r.amount)}`).join(', ') : '无'}`);
      }
      console.log();
    }

    await waitForEnter('已分析下游成员覆盖情况');

    // ================================================================
    // 8. 总结
    // ================================================================
    header('诊断总结');

    console.log(`\n  目标: CS1 (${shortAddr(CS1_ADDRESS)})`);
    console.log(`  等级: Lv${cs1Level}`);
    console.log(`  排线位置: #${cs1Index ?? 'N/A'}`);
    console.log(`  配置 Lv6 覆盖: 上线=${cs1OverrideUp}, 下线=${cs1OverrideDown}`);
    console.log(`  CS1 有效层数: 上线=${cs1EffectiveUp}, 下线=${cs1EffectiveDown}`);
    console.log(`  基础配置: base_up=${baseUp}, base_down=${baseDown}, max_up=${maxUp}, max_down=${maxDown}`);

    console.log(`\n  ── 关键机制说明 ──`);
    console.log(`  排线佣金的有效层数是以 **买家** 为中心计算的:`);
    console.log(`    - 买家购买 → process_upline: 向上(位置更小) 分发 buyer.effective_up 层`);
    console.log(`    - 买家购买 → process_downline: 向下(位置更大) 分发 buyer.effective_down 层`);
    console.log();
    console.log(`  CS1 Lv6 配置 "上4下6" 表示:`);
    console.log(`    ✅ CS1 自己购买时，能覆盖上方 ${cs1EffectiveUp} 人 + 下方 ${cs1EffectiveDown} 人`);
    console.log(`    ❓ CS1 被别人的订单覆盖，取决于 **那个买家** 的有效层数，不是 CS1 的`);

    // 统计 CS1 实际收到的 SL 佣金
    let totalSlUpRecs = 0;
    let totalSlDownRecs = 0;
    for (const o of relevantOrders) {
      for (const r of o.slRecords) {
        if (r.beneficiary !== CS1_ADDRESS) continue;
        if (r.type === 'SingleLineUpline') totalSlUpRecs++;
        else if (r.type === 'SingleLineDownline') totalSlDownRecs++;
      }
    }
    console.log(`\n  CS1 实际收到排线佣金: 上线方向 ${totalSlUpRecs} 条, 下线方向 ${totalSlDownRecs} 条`);

    // 找出具体哪些订单给了 CS1 下线佣金
    console.log(`\n  CS1 收到排线下线佣金的订单:`);
    for (const o of relevantOrders) {
      const cs1DownRecs = o.slRecords.filter(r => r.beneficiary === CS1_ADDRESS && r.type === 'SingleLineDownline');
      for (const r of cs1DownRecs) {
        console.log(`    订单 #${o.orderId} 买家 ${shortAddr(o.buyer)} (Lv${o.buyerLevel}) L${r.level} ${formatNex(r.amount)}`);
      }
    }

    console.log(`\n  CS1 收到排线上线佣金的订单:`);
    for (const o of relevantOrders) {
      const cs1UpRecs = o.slRecords.filter(r => r.beneficiary === CS1_ADDRESS && r.type === 'SingleLineUpline');
      for (const r of cs1UpRecs) {
        console.log(`    订单 #${o.orderId} 买家 ${shortAddr(o.buyer)} (Lv${o.buyerLevel}) L${r.level} ${formatNex(r.amount)}`);
      }
    }

    console.log(`\n${ln()}\n`);

  } finally {
    rl.close();
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误:', err);
  process.exit(1);
});
