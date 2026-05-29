#!/usr/bin/env tsx
/**
 * MultiLevel & SingleLine 佣金资金流追踪脚本
 *
 * 追踪指定账户在所有实体中收到的 MultiLevel / SingleLine 佣金流水:
 *   - 佣金统计 (NEX + Token)
 *   - MultiLevel 逐笔入账记录 (含层级、买家、订单号)
 *   - SingleLine 逐笔入账记录 (含方向、距离、买家、订单号)
 *   - 按订单维度汇总
 *   - 对账验证
 *
 * Usage:
 *   node --import tsx mytests/trace-commission-flow.ts <account_address> [entity_id]
 *   node --import tsx mytests/trace-commission-flow.ts X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94
 *   node --import tsx mytests/trace-commission-flow.ts X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94 100000
 *
 * Environment:
 *   WS_URL      — WebSocket endpoint (default: ws://202.140.140.202:9944)
 *   ACCOUNT     — Account address (can also pass as first arg)
 *   ENTITY_ID   — Entity ID (optional, can also pass as second arg; omit to auto-detect)
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function formatToken(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} Token`;
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function ln(char = '─', len = 80): string { return char.repeat(len); }

function header(zh: string, en: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(ln('═'));
}

function subHeader(zh: string, en: string): void {
  console.log(`\n  ${ln('─', 68)}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(`  ${ln('─', 68)}`);
}

function kv(zh: string, en: string, value: string): void {
  console.log(`  ${zh} / ${en}:  ${value}`);
}

function formatBps(bps: number): string {
  return `${bps} bps (${(bps / 100).toFixed(1)}%)`;
}

/* ------------------------------------------------------------------ */
/*  佣金类型 / Commission Type Labels                                   */
/* ------------------------------------------------------------------ */

const COMMISSION_TYPE_CN: Record<string, string> = {
  OwnerReward:        'Owner 奖励',
  MultiLevel:         '多级分销',
  SingleLineUpline:   '单链上线',
  SingleLineDownline: '单链下线',
  LevelDiff:          '级差奖',
  TeamPerformance:    '团队业绩奖',
  DirectReward:       '直推奖',
  FixedAmount:        '固定金额',
  FirstOrder:         '首单奖',
  RepeatPurchase:     '复购奖',
  EntityReferral:     '实体推荐奖',
  PoolReward:         '奖池分配',
};

const STATUS_CN: Record<string, string> = {
  Pending:     '待结算',
  Distributed: '已分配',
  Settled:     '已结算',
  Cancelled:   '已取消',
};

const DIRECTION_CN: Record<string, string> = {
  Upline:   '上线方向',
  Downline: '下线方向',
};

/* ------------------------------------------------------------------ */
/*  Main                                                               */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {
  const accountArg = process.argv[2] ?? process.env.ACCOUNT;
  const entityIdArg = process.argv[3] ?? process.env.ENTITY_ID;

  if (!accountArg) {
    console.error('用法 / Usage: npx tsx trace-commission-flow.ts <account_address> [entity_id]');
    console.error('示例 / Example: npx tsx trace-commission-flow.ts X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94');
    process.exit(1);
  }

  const account = accountArg;
  const entityId = entityIdArg != null ? Number(entityIdArg) : NaN;

  const api = await connectApi();

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();

    header(
      `MultiLevel & SingleLine 佣金资金流追踪`,
      `MultiLevel & SingleLine Commission Flow Trace`,
    );
    console.log(`  链 / Chain: ${spec}  |  当前区块 / Block: #${currentBlock}`);
    console.log(`  账户 / Account: ${account}`);

    // ── 自动检测实体 ID ──
    const entityIds: number[] = [];
    if (!isNaN(entityId) && entityId > 0) {
      entityIds.push(entityId);
    } else {
      // 尝试从 member 模块查询该账户的 membership
      try {
        const memberEntries = await (api.query as any).entityMember.memberProfiles.entries();
        for (const [key, value] of memberEntries) {
          const keyArgs = (key as any).args ?? [];
          const eid = coerceNumber(keyArgs[0]) ?? 0;
          const addr = String(keyArgs[1] ?? '');
          if (addr === account && eid > 0) {
            entityIds.push(eid);
          }
        }
      } catch {
        // fallback: try common entity IDs
      }
      if (entityIds.length === 0) {
        // 尝试常见实体 ID
        for (const candidateId of [100000, 100001, 100002]) {
          try {
            const stats = codecToJson<Record<string, unknown>>(
              await (api.query as any).commissionCore.memberCommissionStats(candidateId, account),
            );
            const earned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
            if (earned > 0n) {
              entityIds.push(candidateId);
            }
          } catch { /* skip */ }
        }
      }
      if (entityIds.length === 0) {
        console.error('\n  未找到该账户的实体会员记录 / No entity membership found for this account.');
        console.error('  请指定 entity_id / Please specify entity_id');
        await disconnectApi(api);
        process.exit(1);
      }
      console.log(`  检测到实体 / Detected entities: [${entityIds.join(', ')}]`);
    }

    const cc = (api.query as any).commissionCore;
    const ml = (api.query as any).commissionMultiLevel;
    const sl = (api.query as any).commissionSingleLine;

    for (const eid of entityIds) {
      header(`实体 #${eid} 佣金流`, `Entity #${eid} Commission Flow`);

      // ══════════════════════════════════════════════════════════════
      // 1. 佣金总统计
      // ══════════════════════════════════════════════════════════════
      subHeader('1. NEX 佣金统计', 'NEX Commission Stats');

      const nexStats = codecToJson<Record<string, unknown>>(
        await cc.memberCommissionStats(eid, account),
      );
      const nexEarned     = asBigInt(readObjectField(nexStats, 'totalEarned', 'total_earned') ?? 0);
      const nexPending    = asBigInt(readObjectField(nexStats, 'pending') ?? 0);
      const nexWithdrawn  = asBigInt(readObjectField(nexStats, 'withdrawn') ?? 0);
      const nexRepurchased = asBigInt(readObjectField(nexStats, 'repurchased') ?? 0);
      const nexOrderCount = coerceNumber(readObjectField(nexStats, 'orderCount', 'order_count')) ?? 0;

      kv('累计收入',   'Total Earned',  formatNex(nexEarned));
      kv('待提现',     'Pending',       formatNex(nexPending));
      kv('已提现',     'Withdrawn',     formatNex(nexWithdrawn));
      kv('已复购',     'Repurchased',   formatNex(nexRepurchased));
      kv('佣金订单数', 'Order Count',   `${nexOrderCount}`);

      // Token
      const tokenStats = codecToJson<Record<string, unknown>>(
        await cc.memberTokenCommissionStats(eid, account),
      );
      const tokenEarned = asBigInt(readObjectField(tokenStats, 'totalEarned', 'total_earned') ?? 0);
      if (tokenEarned > 0n) {
        const tokenPending = asBigInt(readObjectField(tokenStats, 'pending') ?? 0);
        const tokenWithdrawn = asBigInt(readObjectField(tokenStats, 'withdrawn') ?? 0);
        const tokenRepurchased = asBigInt(readObjectField(tokenStats, 'repurchased') ?? 0);
        console.log();
        kv('Token 累计', 'Token Earned',      formatToken(tokenEarned));
        kv('Token 待提', 'Token Pending',     formatToken(tokenPending));
        kv('Token 已提', 'Token Withdrawn',   formatToken(tokenWithdrawn));
        kv('Token 复购', 'Token Repurchased', formatToken(tokenRepurchased));
      }

      // ══════════════════════════════════════════════════════════════
      // 2. MultiLevel 专属统计 & 入账记录
      // ══════════════════════════════════════════════════════════════
      subHeader('2. MultiLevel 佣金明细', 'MultiLevel Commission Details');

      // 2a. MultiLevel Stats
      let mlStats: Record<string, unknown> | null = null;
      try {
        mlStats = codecToJson<Record<string, unknown>>(
          await ml.memberMultiLevelStats(eid, account),
        );
      } catch { /* pallet not available */ }

      if (mlStats) {
        const mlEarned = asBigInt(readObjectField(mlStats, 'totalEarned', 'total_earned') ?? 0);
        const mlCount  = coerceNumber(readObjectField(mlStats, 'commissionReceiptCount', 'commission_receipt_count')) ?? 0;
        const mlLastBlock = coerceNumber(readObjectField(mlStats, 'lastCommissionBlock', 'last_commission_block')) ?? 0;

        kv('ML 累计收入',   'ML Total Earned',   formatNex(mlEarned));
        kv('ML 入账次数',   'ML Receipt Count',  `${mlCount}`);
        kv('ML 最后入账区块', 'ML Last Block',    mlLastBlock > 0 ? `#${mlLastBlock}` : '(无)');
      } else {
        console.log('  (MultiLevel Stats 不可用 / ML Stats unavailable)');
      }

      // 2b. MultiLevel Summary
      let mlSummary: Record<string, unknown> | null = null;
      try {
        mlSummary = codecToJson<Record<string, unknown>>(
          await ml.memberMultiLevelSummaryStats(eid, account),
        );
      } catch { /* not available */ }

      if (mlSummary) {
        const summaryEarned = asBigInt(readObjectField(mlSummary, 'totalEarned', 'total_earned') ?? 0);
        const summaryCount  = coerceNumber(readObjectField(mlSummary, 'totalPayoutCount', 'total_payout_count')) ?? 0;
        if (summaryEarned > 0n) {
          kv('ML 汇总累计',  'ML Summary Earned', formatNex(summaryEarned));
          kv('ML 汇总笔数',  'ML Summary Count',  `${summaryCount}`);
        }
      }

      // 2c. MultiLevel Payout Records (逐笔)
      let mlPayouts: any[] = [];
      try {
        mlPayouts = codecToJson<any[]>(
          await ml.memberMultiLevelPayouts(eid, account),
        ) ?? [];
      } catch { /* not available */ }

      if (mlPayouts.length > 0) {
        console.log(`\n  MultiLevel 入账流水 (${mlPayouts.length} 笔):`);
        console.log(`  ${'#'.padStart(4)} ${'区块'.padEnd(10)} ${'订单ID'.padEnd(10)} ${'层级'.padEnd(6)} ${'金额'.padStart(22)} ${'买家'.padEnd(18)}`);
        console.log(`  ${'─'.repeat(4)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(18)}`);

        let mlTotal = 0n;
        const mlByLevel = new Map<number, { count: number; total: bigint }>();
        const mlByOrder = new Map<number, { total: bigint; level: number; buyer: string }>();

        mlPayouts.forEach((rec: any, idx: number) => {
          const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const level  = coerceNumber(readObjectField(rec, 'level')) ?? 0;
          const orderId = coerceNumber(readObjectField(rec, 'orderId', 'order_id')) ?? 0;
          const buyer   = String(readObjectField(rec, 'buyer') ?? '');
          const block   = coerceNumber(readObjectField(rec, 'blockNumber', 'block_number')) ?? 0;

          mlTotal += amount;

          // 按层级汇总
          const levelEntry = mlByLevel.get(level) ?? { count: 0, total: 0n };
          levelEntry.count++;
          levelEntry.total += amount;
          mlByLevel.set(level, levelEntry);

          // 按订单汇总
          const orderEntry = mlByOrder.get(orderId) ?? { total: 0n, level: 0, buyer: '' };
          orderEntry.total += amount;
          orderEntry.level = level;
          orderEntry.buyer = buyer;
          mlByOrder.set(orderId, orderEntry);

          console.log(
            `  ${String(idx + 1).padStart(4)} ${`#${block}`.padEnd(10)} ${`#${orderId}`.padEnd(10)} ${`L${level}`.padEnd(6)} ${formatNex(amount).padStart(22)} ${shortAddr(buyer).padEnd(18)}`,
          );
        });

        console.log(`  ${'─'.repeat(4)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(18)}`);
        console.log(`  合计 / Total:${' '.repeat(26)} ${formatNex(mlTotal).padStart(22)}`);

        // 按层级汇总表
        console.log(`\n  MultiLevel 按层级汇总 / By Level:`);
        console.log(`  ${'层级'.padEnd(8)} ${'笔数'.padStart(6)} ${'金额'.padStart(22)} ${'占比'.padStart(8)}`);
        console.log(`  ${'─'.repeat(8)} ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(8)}`);

        const sortedLevels = [...mlByLevel.entries()].sort((a, b) => a[0] - b[0]);
        for (const [level, data] of sortedLevels) {
          const pct = mlTotal > 0n ? ((Number(data.total) / Number(mlTotal)) * 100).toFixed(1) : '0';
          console.log(
            `  ${`L${level}`.padEnd(8)} ${String(data.count).padStart(6)} ${formatNex(data.total).padStart(22)} ${`${pct}%`.padStart(8)}`,
          );
        }
      } else {
        console.log('  (无 MultiLevel 入账记录 / No MultiLevel payout records)');
      }

      // ══════════════════════════════════════════════════════════════
      // 3. SingleLine 专属统计 & 入账记录
      // ══════════════════════════════════════════════════════════════
      subHeader('3. SingleLine 佣金明细', 'SingleLine Commission Details');

      // 3a. SingleLine Stats
      let slStats: Record<string, unknown> | null = null;
      try {
        slStats = codecToJson<Record<string, unknown>>(
          await sl.memberSingleLineStats(eid, account),
        );
      } catch { /* pallet not available */ }

      if (slStats) {
        const slUplineEarned   = asBigInt(readObjectField(slStats, 'totalEarnedAsUpline', 'total_earned_as_upline') ?? 0);
        const slDownlineEarned = asBigInt(readObjectField(slStats, 'totalEarnedAsDownline', 'total_earned_as_downline') ?? 0);
        const slPayoutCount    = coerceNumber(readObjectField(slStats, 'totalPayoutCount', 'total_payout_count')) ?? 0;
        const slLastBlock      = coerceNumber(readObjectField(slStats, 'lastPayoutBlock', 'last_payout_block')) ?? 0;

        kv('SL 上线方向收入', 'SL Upline Earned',     formatNex(slUplineEarned));
        kv('SL 下线方向收入', 'SL Downline Earned',   formatNex(slDownlineEarned));
        kv('SL 合计',         'SL Total',             formatNex(slUplineEarned + slDownlineEarned));
        kv('SL 入账次数',     'SL Payout Count',      `${slPayoutCount}`);
        kv('SL 最后入账区块', 'SL Last Payout Block', slLastBlock > 0 ? `#${slLastBlock}` : '(无)');
      } else {
        console.log('  (SingleLine Stats 不可用 / SL Stats unavailable)');
      }

      // 3b. SingleLine Position
      let slIndex: number | null = null;
      try {
        const indexVal = codecToJson(await sl.singleLineIndex(eid, account));
        slIndex = coerceNumber(indexVal) ?? null;
      } catch { /* not available */ }

      if (slIndex != null) {
        kv('SL 队列位置', 'SL Queue Position', `#${slIndex}`);
      }

      // 3c. SingleLine Payout Records (逐笔)
      let slPayouts: any[] = [];
      try {
        slPayouts = codecToJson<any[]>(
          await sl.memberSingleLinePayouts(eid, account),
        ) ?? [];
      } catch { /* not available */ }

      if (slPayouts.length > 0) {
        console.log(`\n  SingleLine 入账流水 (${slPayouts.length} 笔):`);
        console.log(`  ${'#'.padStart(4)} ${'区块'.padEnd(10)} ${'订单ID'.padEnd(10)} ${'方向'.padEnd(10)} ${'距离'.padEnd(6)} ${'金额'.padStart(22)} ${'买家'.padEnd(18)}`);
        console.log(`  ${'─'.repeat(4)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(18)}`);

        let slTotal = 0n;
        let slUplineTotal = 0n;
        let slDownlineTotal = 0n;
        const slByOrder = new Map<number, { total: bigint; direction: string; buyer: string }>();

        slPayouts.forEach((rec: any, idx: number) => {
          const amount    = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const orderId   = coerceNumber(readObjectField(rec, 'orderId', 'order_id')) ?? 0;
          const buyer     = String(readObjectField(rec, 'buyer') ?? '');
          const block     = coerceNumber(readObjectField(rec, 'blockNumber', 'block_number')) ?? 0;
          const distance  = coerceNumber(readObjectField(rec, 'levelDistance', 'level_distance')) ?? 0;
          const direction = readObjectField(rec, 'direction');
          const dirKey    = typeof direction === 'string' ? direction
                          : (typeof direction === 'object' && direction ? Object.keys(direction)[0] : 'Unknown');
          const dirCn     = DIRECTION_CN[dirKey] ?? dirKey;

          slTotal += amount;
          if (dirKey === 'Upline') slUplineTotal += amount;
          else slDownlineTotal += amount;

          const orderEntry = slByOrder.get(orderId) ?? { total: 0n, direction: '', buyer: '' };
          orderEntry.total += amount;
          orderEntry.direction = dirKey;
          orderEntry.buyer = buyer;
          slByOrder.set(orderId, orderEntry);

          console.log(
            `  ${String(idx + 1).padStart(4)} ${`#${block}`.padEnd(10)} ${`#${orderId}`.padEnd(10)} ${dirCn.padEnd(10)} ${`D${distance}`.padEnd(6)} ${formatNex(amount).padStart(22)} ${shortAddr(buyer).padEnd(18)}`,
          );
        });

        console.log(`  ${'─'.repeat(4)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(18)}`);
        console.log(`  合计 / Total:${' '.repeat(30)} ${formatNex(slTotal).padStart(22)}`);
        console.log(`    上线方向 / Upline:   ${formatNex(slUplineTotal)}`);
        console.log(`    下线方向 / Downline: ${formatNex(slDownlineTotal)}`);
      } else {
        console.log('  (无 SingleLine 入账记录 / No SingleLine payout records)');
      }

      // ══════════════════════════════════════════════════════════════
      // 4. 按订单维度的 ML + SL 佣金汇总 (从 commissionCore 记录)
      // ══════════════════════════════════════════════════════════════
      subHeader('4. 按订单维度汇总 (ML+SL)', 'Order-level ML+SL Summary');

      const nexOrderIds = codecToJson<number[]>(
        await cc.memberCommissionOrderIds(eid, account),
      ) ?? [];

      if (nexOrderIds.length === 0) {
        console.log('  (无关联订单 / No associated orders)');
      } else {
        console.log(`  关联订单 ${nexOrderIds.length} 笔 / ${nexOrderIds.length} order(s)\n`);

        const ML_SL_TYPES = new Set([
          'MultiLevel', 'SingleLineUpline', 'SingleLineDownline',
        ]);

        let grandMlNex = 0n;
        let grandSlUpNex = 0n;
        let grandSlDownNex = 0n;
        let grandMlToken = 0n;
        let grandSlUpToken = 0n;
        let grandSlDownToken = 0n;
        let orderCount = 0;

        // 限制显示最近 50 笔
        const displayOrders = nexOrderIds.slice(-50);
        if (nexOrderIds.length > 50) {
          console.log(`  (仅显示最近 50 笔 / Showing last 50 orders)\n`);
        }

        console.log(`  ${'订单ID'.padEnd(10)} ${'ML NEX'.padStart(20)} ${'SL上线 NEX'.padStart(20)} ${'SL下线 NEX'.padStart(20)} ${'状态'.padEnd(8)}`);
        console.log(`  ${'─'.repeat(10)} ${'─'.repeat(20)} ${'─'.repeat(20)} ${'─'.repeat(20)} ${'─'.repeat(8)}`);

        for (const oid of displayOrders) {
          const nexRecs = codecToJson<any[]>(
            await cc.orderCommissionRecords(oid),
          ) ?? [];
          const tokenRecs = codecToJson<any[]>(
            await cc.orderTokenCommissionRecords(oid),
          ) ?? [];

          // 过滤出当前用户的 ML/SL 记录
          let orderMl = 0n;
          let orderSlUp = 0n;
          let orderSlDown = 0n;
          let orderMlToken = 0n;
          let orderSlUpToken = 0n;
          let orderSlDownToken = 0n;
          let anyStatus = '';

          for (const rec of nexRecs) {
            const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
            if (beneficiary !== account) continue;
            const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
            if (!ML_SL_TYPES.has(ctype)) continue;
            const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
            const status = String(readObjectField(rec, 'status') ?? '');
            anyStatus = status;

            if (ctype === 'MultiLevel') orderMl += amount;
            else if (ctype === 'SingleLineUpline') orderSlUp += amount;
            else if (ctype === 'SingleLineDownline') orderSlDown += amount;
          }

          for (const rec of tokenRecs) {
            const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
            if (beneficiary !== account) continue;
            const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
            if (!ML_SL_TYPES.has(ctype)) continue;
            const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);

            if (ctype === 'MultiLevel') orderMlToken += amount;
            else if (ctype === 'SingleLineUpline') orderSlUpToken += amount;
            else if (ctype === 'SingleLineDownline') orderSlDownToken += amount;
          }

          if (orderMl === 0n && orderSlUp === 0n && orderSlDown === 0n &&
              orderMlToken === 0n && orderSlUpToken === 0n && orderSlDownToken === 0n) {
            continue; // 该订单没有 ML/SL 佣金给当前账户
          }

          orderCount++;
          grandMlNex += orderMl;
          grandSlUpNex += orderSlUp;
          grandSlDownNex += orderSlDown;
          grandMlToken += orderMlToken;
          grandSlUpToken += orderSlUpToken;
          grandSlDownToken += orderSlDownToken;

          const sCn = STATUS_CN[anyStatus] ?? anyStatus;
          console.log(
            `  ${`#${oid}`.padEnd(10)} ${formatNex(orderMl).padStart(20)} ${formatNex(orderSlUp).padStart(20)} ${formatNex(orderSlDown).padStart(20)} ${sCn.padEnd(8)}`,
          );
        }

        console.log(`  ${'─'.repeat(10)} ${'─'.repeat(20)} ${'─'.repeat(20)} ${'─'.repeat(20)} ${'─'.repeat(8)}`);
        console.log(
          `  ${'合计'.padEnd(10)} ${formatNex(grandMlNex).padStart(20)} ${formatNex(grandSlUpNex).padStart(20)} ${formatNex(grandSlDownNex).padStart(20)}`,
        );

        const grandTotal = grandMlNex + grandSlUpNex + grandSlDownNex;
        console.log(`\n  ML+SL NEX 合计 / ML+SL NEX Total: ${formatNex(grandTotal)}`);
        console.log(`  涉及订单 / Orders: ${orderCount} 笔`);

        if (grandMlToken > 0n || grandSlUpToken > 0n || grandSlDownToken > 0n) {
          const grandTokenTotal = grandMlToken + grandSlUpToken + grandSlDownToken;
          console.log(`\n  Token 维度:`);
          console.log(`    ML Token: ${formatToken(grandMlToken)}`);
          console.log(`    SL 上线 Token: ${formatToken(grandSlUpToken)}`);
          console.log(`    SL 下线 Token: ${formatToken(grandSlDownToken)}`);
          console.log(`    ML+SL Token 合计: ${formatToken(grandTokenTotal)}`);
        }

        // ── 占比分析 ──
        if (grandTotal > 0n) {
          const mlPct = ((Number(grandMlNex) / Number(grandTotal)) * 100).toFixed(1);
          const slUpPct = ((Number(grandSlUpNex) / Number(grandTotal)) * 100).toFixed(1);
          const slDownPct = ((Number(grandSlDownNex) / Number(grandTotal)) * 100).toFixed(1);
          console.log(`\n  占比分析 / Proportion:`);
          console.log(`    MultiLevel:          ${mlPct}%`);
          console.log(`    SingleLine Upline:   ${slUpPct}%`);
          console.log(`    SingleLine Downline: ${slDownPct}%`);
        }
      }

      // ══════════════════════════════════════════════════════════════
      // 5. 配置信息
      // ══════════════════════════════════════════════════════════════
      subHeader('5. 佣金配置', 'Commission Configuration');

      // MultiLevel Config
      let mlConfig: Record<string, unknown> | null = null;
      try {
        const mlConfigRaw = await ml.multiLevelConfigs(eid);
        if (mlConfigRaw && !(mlConfigRaw as any).isNone) {
          mlConfig = codecToJson<Record<string, unknown>>(
            (mlConfigRaw as any).unwrap ? (mlConfigRaw as any).unwrap() : mlConfigRaw,
          );
        }
      } catch { /* not available */ }

      if (mlConfig) {
        const tiers = (readObjectField(mlConfig, 'tiers') as any[]) ?? [];
        console.log(`  MultiLevel 配置:`);
        console.log(`    层级数 / Tiers: ${tiers.length}`);

        if (tiers.length > 0) {
          console.log(`    ${'层级'.padEnd(6)} ${'费率'.padStart(10)} ${'直推要求'.padStart(10)} ${'团队要求'.padStart(10)} ${'消费要求'.padStart(14)} ${'等级要求'.padStart(8)}`);
          console.log(`    ${'─'.repeat(6)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(10)} ${'─'.repeat(14)} ${'─'.repeat(8)}`);

          tiers.forEach((tier: any, idx: number) => {
            const rate = coerceNumber(readObjectField(tier, 'rate')) ?? 0;
            const directs = coerceNumber(readObjectField(tier, 'requiredDirects', 'required_directs')) ?? 0;
            const teamSize = coerceNumber(readObjectField(tier, 'requiredTeamSize', 'required_team_size')) ?? 0;
            const spent = asBigInt(readObjectField(tier, 'requiredSpent', 'required_spent') ?? 0);
            const levelId = coerceNumber(readObjectField(tier, 'requiredLevelId', 'required_level_id')) ?? 0;

            const spentStr = spent > 0n ? `${(Number(spent) / 1e6).toFixed(0)} USDT` : '-';
            console.log(
              `    ${`L${idx + 1}`.padEnd(6)} ${formatBps(rate).padStart(10)} ${(directs > 0 ? `${directs}` : '-').padStart(10)} ${(teamSize > 0 ? `${teamSize}` : '-').padStart(10)} ${spentStr.padStart(14)} ${(levelId > 0 ? `Lv${levelId}` : '-').padStart(8)}`,
            );
          });
        }

        // ML 全局暂停
        let mlPaused = false;
        try {
          mlPaused = codecToJson<boolean>(await ml.globalPaused(eid)) ?? false;
        } catch { /* not available */ }
        if (mlPaused) console.log(`    [!!] MultiLevel 已暂停 / ML PAUSED`);
      } else {
        console.log('  MultiLevel 配置: 未配置');
      }

      // SingleLine Config
      let slConfig: Record<string, unknown> | null = null;
      try {
        const slConfigRaw = await sl.singleLineConfigs(eid);
        if (slConfigRaw && !(slConfigRaw as any).isNone) {
          slConfig = codecToJson<Record<string, unknown>>(
            (slConfigRaw as any).unwrap ? (slConfigRaw as any).unwrap() : slConfigRaw,
          );
        }
      } catch { /* not available */ }

      if (slConfig) {
        const uplineRate    = coerceNumber(readObjectField(slConfig, 'uplineRate', 'upline_rate')) ?? 0;
        const downlineRate  = coerceNumber(readObjectField(slConfig, 'downlineRate', 'downline_rate')) ?? 0;
        const baseUp        = coerceNumber(readObjectField(slConfig, 'baseUplineLevels', 'base_upline_levels')) ?? 0;
        const baseDown      = coerceNumber(readObjectField(slConfig, 'baseDownlineLevels', 'base_downline_levels')) ?? 0;
        const maxUp         = coerceNumber(readObjectField(slConfig, 'maxUplineLevels', 'max_upline_levels')) ?? 0;
        const maxDown       = coerceNumber(readObjectField(slConfig, 'maxDownlineLevels', 'max_downline_levels')) ?? 0;
        const threshold     = asBigInt(readObjectField(slConfig, 'levelIncrementThreshold', 'level_increment_threshold') ?? 0);

        console.log(`\n  SingleLine 配置:`);
        kv('  上线费率',       'Upline Rate',       formatBps(uplineRate));
        kv('  下线费率',       'Downline Rate',     formatBps(downlineRate));
        kv('  基础上线层数',   'Base Up Levels',    `${baseUp}`);
        kv('  基础下线层数',   'Base Down Levels',  `${baseDown}`);
        kv('  最大上线层数',   'Max Up Levels',     `${maxUp}`);
        kv('  最大下线层数',   'Max Down Levels',   `${maxDown}`);
        kv('  升级阈值',       'Increment Threshold', threshold > 0n ? formatNex(threshold) : '-');

        // SL 启用状态
        let slEnabled = true;
        try {
          slEnabled = codecToJson<boolean>(await sl.singleLineEnabled(eid)) ?? true;
        } catch { /* not available */ }
        if (!slEnabled) console.log(`    [!!] SingleLine 已暂停 / SL PAUSED`);
      } else {
        console.log('  SingleLine 配置: 未配置');
      }

      // ══════════════════════════════════════════════════════════════
      // 6. 资金流总览
      // ══════════════════════════════════════════════════════════════
      header('资金流总览', 'Fund Flow Overview');

      // 从 payout 记录汇总
      let payoutMlTotal = 0n;
      for (const rec of mlPayouts) {
        payoutMlTotal += asBigInt(readObjectField(rec, 'amount') ?? 0);
      }
      let payoutSlTotal = 0n;
      for (const rec of slPayouts) {
        payoutSlTotal += asBigInt(readObjectField(rec, 'amount') ?? 0);
      }

      console.log(`\n  会员 / Member: ${shortAddr(account)}`);
      console.log(`  实体 / Entity: #${eid}\n`);

      console.log(`  ┌────────────────────────────────────────────────────────────┐`);
      console.log(`  │  佣金来源分布                                                │`);
      console.log(`  │  Commission Source Breakdown                               │`);
      console.log(`  ├────────────────────────────────────────────────────────────┤`);
      console.log(`  │  MultiLevel (多级分销):                                     │`);
      console.log(`  │    Payout 记录合计: ${formatNex(payoutMlTotal).padEnd(38)}│`);
      console.log(`  │    (含 ${mlPayouts.length} 笔入账)${' '.repeat(43 - String(mlPayouts.length).length)}│`);
      console.log(`  ├────────────────────────────────────────────────────────────┤`);
      console.log(`  │  SingleLine (单链):                                         │`);
      console.log(`  │    Payout 记录合计: ${formatNex(payoutSlTotal).padEnd(38)}│`);
      console.log(`  │    (含 ${slPayouts.length} 笔入账)${' '.repeat(43 - String(slPayouts.length).length)}│`);
      console.log(`  ├────────────────────────────────────────────────────────────┤`);
      console.log(`  │  ML + SL 合计: ${formatNex(payoutMlTotal + payoutSlTotal).padEnd(42)}│`);
      console.log(`  ├────────────────────────────────────────────────────────────┤`);
      console.log(`  │  Core 总统计:                                               │`);
      console.log(`  │    NEX 总收入:  ${formatNex(nexEarned).padEnd(41)}│`);
      console.log(`  │    待提现:      ${formatNex(nexPending).padEnd(41)}│`);
      console.log(`  │    已提现:      ${formatNex(nexWithdrawn).padEnd(41)}│`);
      console.log(`  │    已复购:      ${formatNex(nexRepurchased).padEnd(41)}│`);
      console.log(`  └────────────────────────────────────────────────────────────┘`);

      // 对账提示
      if (nexEarned > 0n) {
        const accountedFor = nexPending + nexWithdrawn + nexRepurchased;
        const diff = nexEarned - accountedFor;
        if (diff === 0n) {
          console.log(`\n  [OK] NEX 对账验证通过: earned = pending + withdrawn + repurchased`);
        } else {
          console.log(`\n  [!!] NEX 对账差额: ${formatNex(diff)}`);
        }
      }

      // ML+SL 与总 earned 的关系说明
      const mlSlTotal = payoutMlTotal + payoutSlTotal;
      if (mlSlTotal > 0n && nexEarned > 0n) {
        const mlSlPct = ((Number(mlSlTotal) / Number(nexEarned)) * 100).toFixed(1);
        console.log(`  ML+SL 占总收入比: ${mlSlPct}% (其余来自 Owner/Direct/LevelDiff/Team 等)`);
      }
    }

    console.log(`\n${ln('═')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误 / Error:', err);
  process.exit(1);
});
