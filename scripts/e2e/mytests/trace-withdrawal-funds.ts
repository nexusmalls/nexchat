#!/usr/bin/env tsx
/**
 * 用户提现资金流追踪脚本 / User Withdrawal Fund Flow Tracer
 *
 * 查询指定成员在指定实体中的提现相关数据:
 *   - 佣金统计 (NEX + Token)
 *   - 提现历史记录
 *   - 提现配置 (模式、冷却期、复购比例)
 *   - 购物余额
 *   - 实体资金池约束 (已承诺 vs 可用)
 *   - 对账验证
 *
 * Usage:
 *   node --import tsx mytests/trace-withdrawal-funds.ts <entity_id> <account_address>
 *   node --import tsx mytests/trace-withdrawal-funds.ts 100000 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
 *
 * Environment:
 *   WS_URL      — WebSocket endpoint (default: ws://127.0.0.1:9944)
 *   ENTITY_ID   — Entity ID (can also pass as first arg)
 *   ACCOUNT     — Account address (can also pass as second arg)
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { readFreeBalance } from '../framework/accounts.js';
import { formatNex, asBigInt } from '../framework/units.js';
import { stringToU8a } from '@polkadot/util';
import { encodeAddress } from '@polkadot/util-crypto';

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function formatToken(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} Token`;
}

/**
 * 将 bps 格式化成 bps 与百分比双表示。
 */
function formatBps(bps: number): string {
  return `${bps} bps (${(bps / 100).toFixed(1)}%)`;
}

/**
 * 缩短地址展示，保留首尾便于识别。
 */
function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

/**
 * 生成指定字符和长度的分隔线。
 */
function ln(char = '─', len = 76): string { return char.repeat(len); }

function header(zh: string, en: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(ln('═'));
}

/**
 * 输出双语子标题。
 */
function subHeader(zh: string, en: string): void {
  console.log(`\n  ${ln('─', 64)}`);
  console.log(`  ${zh}  |  ${en}`);
  console.log(`  ${ln('─', 64)}`);
}

/**
 * 输出双语键值行。
 */
function kv(zh: string, en: string, value: string): void {
  console.log(`  ${zh} / ${en}:  ${value}`);
}

/**
 * 根据 PalletId 和实体 ID 推导实体金库地址。
 */
function deriveTreasuryAddress(entityId: number, ss58Format: number): string {
  const raw = new Uint8Array(32);
  raw.set(stringToU8a('modl'), 0);
  raw.set(stringToU8a('et/enty/'), 4);
  const dv = new DataView(raw.buffer);
  dv.setBigUint64(12, BigInt(entityId), true);
  return encodeAddress(raw, ss58Format);
}

/* ------------------------------------------------------------------ */
/*  提现模式中文映射                                                     */
/* ------------------------------------------------------------------ */

const WITHDRAWAL_MODE_CN: Record<string, string> = {
  FullWithdrawal: '全额提现',
  FixedRate:      '固定复购比例',
  LevelBased:     '等级差异化',
  MemberChoice:   '会员自选',
};

/* ------------------------------------------------------------------ */
/*  主流程 / Main                                                      */
/* ------------------------------------------------------------------ */

/**
 * 主入口：追踪指定成员在实体中的提现资金流与对账状态。
 */
async function main(): Promise<void> {
  const entityIdArg = process.argv[2] ?? process.env.ENTITY_ID;
  const accountArg = process.argv[3] ?? process.env.ACCOUNT;

  const entityId = entityIdArg != null ? Number(entityIdArg) : NaN;
  if (isNaN(entityId) || entityId <= 0 || !accountArg) {
    console.error('用法 / Usage: node --import tsx mytests/trace-withdrawal-funds.ts <entity_id> <account_address>');
    console.error('示例 / Example: node --import tsx mytests/trace-withdrawal-funds.ts 100000 5GrwvaEF...');
    process.exit(1);
  }

  const account = accountArg;
  const api = await connectApi();

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();

    header(
      `实体 #${entityId} 会员提现资金流`,
      `Entity #${entityId} Member Withdrawal Fund Flow`,
    );
    console.log(`  链 / Chain: ${spec}  |  当前区块 / Block: #${currentBlock}`);
    console.log(`  账户 / Account: ${account}`);

    const cc = (api.query as any).commissionCore;
    const ly = (api.query as any).entityLoyalty;

    // ═══════════════════════════════════════════════════════════════════
    // 1. 会员佣金统计
    // ═══════════════════════════════════════════════════════════════════
    subHeader('1. NEX 佣金统计', 'NEX Commission Stats');

    const nexStats = codecToJson<Record<string, unknown>>(
      await cc.memberCommissionStats(entityId, account),
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

    if (nexEarned > 0n) {
      const withdrawnPct = ((Number(nexWithdrawn) / Number(nexEarned)) * 100).toFixed(1);
      const repurchasedPct = ((Number(nexRepurchased) / Number(nexEarned)) * 100).toFixed(1);
      const pendingPct = ((Number(nexPending) / Number(nexEarned)) * 100).toFixed(1);
      kv('提现率',   'Withdrawal %',   `${withdrawnPct}%`);
      kv('复购率',   'Repurchase %',   `${repurchasedPct}%`);
      kv('待提率',   'Pending %',      `${pendingPct}%`);

      // 对账验证: totalEarned = pending + withdrawn + repurchased
      const accountedFor = nexPending + nexWithdrawn + nexRepurchased;
      const diff = nexEarned - accountedFor;
      if (diff === 0n) {
        console.log(`  [通过 / OK] 对账验证: earned = pending + withdrawn + repurchased`);
      } else {
        console.log(`  [!!] 对账差额 / Discrepancy: ${formatNex(diff)}`);
        console.log(`       earned(${formatNex(nexEarned)}) != pending(${formatNex(nexPending)}) + withdrawn(${formatNex(nexWithdrawn)}) + repurchased(${formatNex(nexRepurchased)})`);
      }
    }

    // Token 佣金统计
    subHeader('1b. Token 佣金统计', 'Token Commission Stats');

    const tokenStats = codecToJson<Record<string, unknown>>(
      await cc.memberTokenCommissionStats(entityId, account),
    );
    const tokenEarned     = asBigInt(readObjectField(tokenStats, 'totalEarned', 'total_earned') ?? 0);
    const tokenPending    = asBigInt(readObjectField(tokenStats, 'pending') ?? 0);
    const tokenWithdrawn  = asBigInt(readObjectField(tokenStats, 'withdrawn') ?? 0);
    const tokenRepurchased = asBigInt(readObjectField(tokenStats, 'repurchased') ?? 0);
    const tokenOrderCount = coerceNumber(readObjectField(tokenStats, 'orderCount', 'order_count')) ?? 0;

    if (tokenEarned > 0n || tokenPending > 0n) {
      kv('累计收入',   'Total Earned',  formatToken(tokenEarned));
      kv('待提现',     'Pending',       formatToken(tokenPending));
      kv('已提现',     'Withdrawn',     formatToken(tokenWithdrawn));
      kv('已复购',     'Repurchased',   formatToken(tokenRepurchased));
      kv('佣金订单数', 'Order Count',   `${tokenOrderCount}`);

      if (tokenEarned > 0n) {
        const accountedFor = tokenPending + tokenWithdrawn + tokenRepurchased;
        const diff = tokenEarned - accountedFor;
        if (diff === 0n) {
          console.log(`  [OK] Token 对账验证通过`);
        } else {
          console.log(`  [!!] Token 对账差额: ${formatToken(diff)}`);
        }
      }
    } else {
      console.log('  (无 Token 佣金记录 / No Token commission records)');
    }

    // ═══════════════════════════════════════════════════════════════════
    // 2. 提现配置
    // ═══════════════════════════════════════════════════════════════════
    subHeader('2. 提现配置', 'Withdrawal Configuration');

    // 全局暂停
    const globalPaused = codecToJson<boolean>(await cc.globalCommissionPaused()) ?? false;
    const entityPaused = codecToJson<boolean>(await cc.withdrawalPaused(entityId)) ?? false;
    kv('全局佣金暂停', 'Global Paused',       globalPaused ? 'YES' : 'No');
    kv('实体提现暂停', 'Entity W/D Paused',   entityPaused ? 'YES' : 'No');

    // NEX 提现配置
    const nexWdConfig = codecToJson<Record<string, unknown>>(
      await cc.withdrawalConfigs(entityId),
    );
    if (nexWdConfig) {
      const mode = readObjectField(nexWdConfig, 'mode');
      const modeKey = typeof mode === 'string' ? mode : (typeof mode === 'object' && mode ? Object.keys(mode)[0] : 'Unknown');
      const enabled = readObjectField(nexWdConfig, 'enabled');
      const bonusRate = coerceNumber(readObjectField(nexWdConfig, 'voluntaryBonusRate', 'voluntary_bonus_rate')) ?? 0;

      kv('NEX 提现模式',    'NEX W/D Mode',     `${WITHDRAWAL_MODE_CN[modeKey] ?? modeKey} / ${modeKey}`);
      kv('NEX 配置启用',    'NEX W/D Enabled',  `${enabled}`);
      kv('自愿复购加成率',  'Voluntary Bonus',   formatBps(bonusRate));

      // 提取 FixedRate / LevelBased 参数
      if (typeof mode === 'object' && mode) {
        const inner = (mode as Record<string, unknown>)[modeKey];
        if (inner && typeof inner === 'object') {
          console.log(`  NEX 模式参数 / Mode Params: ${JSON.stringify(inner)}`);
        }
      }

      // default_tier
      const defaultTier = codecToJson(readObjectField(nexWdConfig, 'defaultTier', 'default_tier'));
      if (defaultTier) {
        const wdRate = coerceNumber(readObjectField(defaultTier as any, 'withdrawalRate', 'withdrawal_rate')) ?? 0;
        const rpRate = coerceNumber(readObjectField(defaultTier as any, 'repurchaseRate', 'repurchase_rate')) ?? 0;
        kv('默认提现率',   'Default W/D Rate',  formatBps(wdRate));
        kv('默认复购率',   'Default RP Rate',   formatBps(rpRate));
      }
    } else {
      console.log('  NEX 提现配置: 未配置 (默认全额提现)');
      console.log('  NEX W/D Config: Not set (default: full withdrawal)');
    }

    // Token 提现配置
    const tokenWdConfig = codecToJson<Record<string, unknown>>(
      await cc.tokenWithdrawalConfigs(entityId),
    );
    if (tokenWdConfig) {
      const mode = readObjectField(tokenWdConfig, 'mode');
      const modeKey = typeof mode === 'string' ? mode : (typeof mode === 'object' && mode ? Object.keys(mode)[0] : 'Unknown');
      const enabled = readObjectField(tokenWdConfig, 'enabled');
      kv('Token 提现模式',  'Token W/D Mode',    `${WITHDRAWAL_MODE_CN[modeKey] ?? modeKey} / ${modeKey}`);
      kv('Token 配置启用',  'Token W/D Enabled', `${enabled}`);
    } else {
      console.log('  Token 提现配置: 未配置');
    }

    // Governance 最低复购率
    const govMinNex = coerceNumber(codecToJson(await cc.globalMinRepurchaseRate(entityId))) ?? 0;
    const govMinToken = coerceNumber(codecToJson(await cc.globalMinTokenRepurchaseRate(entityId))) ?? 0;
    kv('治理最低复购率 (NEX)',   'Gov Min RP Rate (NEX)',   formatBps(govMinNex));
    kv('治理最低复购率 (Token)', 'Gov Min RP Rate (Token)', formatBps(govMinToken));

    // 冷却期 & 间隔
    const commConfig = codecToJson<Record<string, unknown>>(
      await cc.commissionConfigs(entityId),
    );
    const nexCooldown = coerceNumber(readObjectField(commConfig, 'withdrawalCooldown', 'withdrawal_cooldown')) ?? 0;
    const tokenCooldown = coerceNumber(readObjectField(commConfig, 'tokenWithdrawalCooldown', 'token_withdrawal_cooldown')) ?? 0;
    const minInterval = coerceNumber(codecToJson(await cc.minWithdrawalInterval(entityId))) ?? 0;

    kv('NEX 提现冷却期',    'NEX Cooldown',      `${nexCooldown} blocks`);
    kv('Token 提现冷却期',  'Token Cooldown',     `${tokenCooldown} blocks`);
    kv('最小提现间隔',      'Min W/D Interval',   `${minInterval} blocks`);

    // ═══════════════════════════════════════════════════════════════════
    // 3. 冷却期 & 间隔状态
    // ═══════════════════════════════════════════════════════════════════
    subHeader('3. 冷却期 & 频率限制状态', 'Cooldown & Frequency Status');

    const nexLastCredited = coerceNumber(codecToJson(
      await cc.memberLastCredited(entityId, account),
    )) ?? 0;
    const tokenLastCredited = coerceNumber(codecToJson(
      await cc.memberTokenLastCredited(entityId, account),
    )) ?? 0;
    const nexLastWithdrawn = coerceNumber(codecToJson(
      await cc.memberLastWithdrawn(entityId, account),
    )) ?? 0;
    const tokenLastWithdrawn = coerceNumber(codecToJson(
      await cc.memberTokenLastWithdrawn(entityId, account),
    )) ?? 0;

    kv('NEX 最后入账区块',     'NEX Last Credited',   nexLastCredited > 0 ? `#${nexLastCredited}` : '(无)');
    kv('Token 最后入账区块',   'Token Last Credited', tokenLastCredited > 0 ? `#${tokenLastCredited}` : '(无)');
    kv('NEX 最后提现区块',     'NEX Last Withdrawn',  nexLastWithdrawn > 0 ? `#${nexLastWithdrawn}` : '(无)');
    kv('Token 最后提现区块',   'Token Last Withdrawn', tokenLastWithdrawn > 0 ? `#${tokenLastWithdrawn}` : '(无)');

    // NEX 冷却期判定
    if (nexCooldown > 0 && nexLastCredited > 0) {
      const nexCooldownEnd = nexLastCredited + nexCooldown;
      const nexCooldownReady = currentBlock >= nexCooldownEnd;
      kv('NEX 冷却期结束',   'NEX Cooldown End',   `#${nexCooldownEnd}`);
      kv('NEX 冷却期状态',   'NEX Cooldown Status', nexCooldownReady ? 'OK 可提现' : `等待 ${nexCooldownEnd - currentBlock} blocks`);
    }

    // Token 冷却期判定
    if (tokenCooldown > 0 && tokenLastCredited > 0) {
      const tokenCooldownEnd = tokenLastCredited + tokenCooldown;
      const tokenCooldownReady = currentBlock >= tokenCooldownEnd;
      kv('Token 冷却期结束', 'Token Cooldown End', `#${tokenCooldownEnd}`);
      kv('Token 冷却期状态', 'Token CD Status',    tokenCooldownReady ? 'OK 可提现' : `等待 ${tokenCooldownEnd - currentBlock} blocks`);
    }

    // 提现间隔判定
    if (minInterval > 0) {
      if (nexLastWithdrawn > 0) {
        const nexNextAllowed = nexLastWithdrawn + minInterval;
        const nexIntervalOk = currentBlock >= nexNextAllowed;
        kv('NEX 下次可提现',   'NEX Next Allowed',  `#${nexNextAllowed} ${nexIntervalOk ? '(OK)' : `(等待 ${nexNextAllowed - currentBlock} blocks)`}`);
      }
      if (tokenLastWithdrawn > 0) {
        const tokenNextAllowed = tokenLastWithdrawn + minInterval;
        const tokenIntervalOk = currentBlock >= tokenNextAllowed;
        kv('Token 下次可提现', 'Token Next Allowed', `#${tokenNextAllowed} ${tokenIntervalOk ? '(OK)' : `(等待 ${tokenNextAllowed - currentBlock} blocks)`}`);
      }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 4. NEX 提现历史
    // ═══════════════════════════════════════════════════════════════════
    subHeader('4. NEX 提现历史', 'NEX Withdrawal History');

    const nexHistory = codecToJson<any[]>(
      await cc.memberWithdrawalHistory(entityId, account),
    ) ?? [];

    if (nexHistory.length === 0) {
      console.log('  (无 NEX 提现记录 / No NEX withdrawal history)');
    } else {
      console.log(`  共 ${nexHistory.length} 条记录 / ${nexHistory.length} record(s)\n`);
      console.log(`  ${'#'.padStart(4)} ${'区块/Block'.padEnd(14)} ${'提现总额/Total'.padStart(18)} ${'到账/Withdrawn'.padStart(18)} ${'复购/Repurchased'.padStart(18)} ${'加成/Bonus'.padStart(16)}`);
      console.log(`  ${'─'.repeat(4)} ${'─'.repeat(14)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(16)}`);

      let histTotal = 0n;
      let histWithdrawn = 0n;
      let histRepurchased = 0n;
      let histBonus = 0n;

      nexHistory.forEach((rec: any, idx: number) => {
        const total = asBigInt(readObjectField(rec, 'totalAmount', 'total_amount') ?? 0);
        const wd = asBigInt(readObjectField(rec, 'withdrawn') ?? 0);
        const rp = asBigInt(readObjectField(rec, 'repurchased') ?? 0);
        const bonus = asBigInt(readObjectField(rec, 'bonus') ?? 0);
        const block = coerceNumber(readObjectField(rec, 'blockNumber', 'block_number')) ?? 0;

        histTotal += total;
        histWithdrawn += wd;
        histRepurchased += rp;
        histBonus += bonus;

        console.log(
          `  ${String(idx + 1).padStart(4)} ${`#${block}`.padEnd(14)} ${formatNex(total).padStart(18)} ${formatNex(wd).padStart(18)} ${formatNex(rp).padStart(18)} ${formatNex(bonus).padStart(16)}`,
        );
      });

      console.log(`  ${'─'.repeat(4)} ${'─'.repeat(14)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(16)}`);
      console.log(
        `  ${'合计'.padStart(4)} ${''.padEnd(14)} ${formatNex(histTotal).padStart(18)} ${formatNex(histWithdrawn).padStart(18)} ${formatNex(histRepurchased).padStart(18)} ${formatNex(histBonus).padStart(16)}`,
      );

      // 历史 vs 统计 交叉验证
      if (histWithdrawn !== nexWithdrawn) {
        console.log(`\n  [!!] 历史提现合计 ${formatNex(histWithdrawn)} != Stats.withdrawn ${formatNex(nexWithdrawn)}`);
        console.log(`       (可能因历史截断 / May be due to history truncation)`);
      }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 5. Token 提现历史
    // ═══════════════════════════════════════════════════════════════════
    subHeader('5. Token 提现历史', 'Token Withdrawal History');

    const tokenHistory = codecToJson<any[]>(
      await cc.memberTokenWithdrawalHistory(entityId, account),
    ) ?? [];

    if (tokenHistory.length === 0) {
      console.log('  (无 Token 提现记录 / No Token withdrawal history)');
    } else {
      console.log(`  共 ${tokenHistory.length} 条记录 / ${tokenHistory.length} record(s)\n`);
      console.log(`  ${'#'.padStart(4)} ${'区块/Block'.padEnd(14)} ${'提现总额/Total'.padStart(18)} ${'到账/Withdrawn'.padStart(18)} ${'复购/Repurchased'.padStart(18)} ${'加成/Bonus'.padStart(16)}`);
      console.log(`  ${'─'.repeat(4)} ${'─'.repeat(14)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(16)}`);

      let histTotal = 0n;
      let histWithdrawn = 0n;
      let histRepurchased = 0n;
      let histBonus = 0n;

      tokenHistory.forEach((rec: any, idx: number) => {
        const total = asBigInt(readObjectField(rec, 'totalAmount', 'total_amount') ?? 0);
        const wd = asBigInt(readObjectField(rec, 'withdrawn') ?? 0);
        const rp = asBigInt(readObjectField(rec, 'repurchased') ?? 0);
        const bonus = asBigInt(readObjectField(rec, 'bonus') ?? 0);
        const block = coerceNumber(readObjectField(rec, 'blockNumber', 'block_number')) ?? 0;

        histTotal += total;
        histWithdrawn += wd;
        histRepurchased += rp;
        histBonus += bonus;

        console.log(
          `  ${String(idx + 1).padStart(4)} ${`#${block}`.padEnd(14)} ${formatToken(total).padStart(18)} ${formatToken(wd).padStart(18)} ${formatToken(rp).padStart(18)} ${formatToken(bonus).padStart(16)}`,
        );
      });

      console.log(`  ${'─'.repeat(4)} ${'─'.repeat(14)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(16)}`);
      console.log(
        `  ${'合计'.padStart(4)} ${''.padEnd(14)} ${formatToken(histTotal).padStart(18)} ${formatToken(histWithdrawn).padStart(18)} ${formatToken(histRepurchased).padStart(18)} ${formatToken(histBonus).padStart(16)}`,
      );
    }

    // ═══════════════════════════════════════════════════════════════════
    // 6. 购物余额
    // ═══════════════════════════════════════════════════════════════════
    subHeader('6. 购物余额 (复购所得)', 'Shopping Balance (from Repurchase)');

    const nexShopping = asBigInt(await ly.memberShoppingBalance(entityId, account));
    const tokenShopping = asBigInt(await ly.memberTokenShoppingBalance(entityId, account));

    kv('NEX 购物余额',   'NEX Shopping Bal',   formatNex(nexShopping));
    kv('Token 购物余额', 'Token Shopping Bal',  formatToken(tokenShopping));

    // ═══════════════════════════════════════════════════════════════════
    // 7. 关联订单佣金明细
    // ═══════════════════════════════════════════════════════════════════
    subHeader('7. 关联订单佣金明细', 'Commission Order Details');

    // 查询此会员的佣金订单 ID 列表
    const nexOrderIds = codecToJson<number[]>(
      await cc.memberCommissionOrderIds(entityId, account),
    ) ?? [];
    const tokenOrderIds = codecToJson<number[]>(
      await cc.memberTokenCommissionOrderIds(entityId, account),
    ) ?? [];

    const allOrderIds = [...new Set([...nexOrderIds, ...tokenOrderIds])].sort((a, b) => a - b);

    if (allOrderIds.length === 0) {
      console.log('  (无关联订单 / No associated orders)');
    } else {
      console.log(`  关联订单 ${allOrderIds.length} 笔 / ${allOrderIds.length} order(s): [${allOrderIds.join(', ')}]\n`);

      const COMMISSION_TYPE_CN: Record<string, string> = {
        OwnerReward: 'Owner 奖励', MultiLevel: '多级分销',
        SingleLineUpline: '单链上线', SingleLineDownline: '单链下线',
        LevelDiff: '级差奖', TeamPerformance: '团队业绩奖',
        DirectReward: '直推奖', FixedAmount: '固定金额',
        FirstOrder: '首单奖', RepeatPurchase: '复购奖',
        EntityReferral: '实体推荐奖', PoolReward: '奖池分配',
      };

      const STATUS_CN: Record<string, string> = {
        Pending: '待结算', Distributed: '已分配', Settled: '已结算', Cancelled: '已取消',
      };

      // 限制显示最近 20 笔订单的详情
      const displayOrders = allOrderIds.slice(-20);
      if (allOrderIds.length > 20) {
        console.log(`  (仅显示最近 20 笔 / Showing last 20 orders)\n`);
      }

      let grandNex = 0n;
      let grandToken = 0n;

      for (const oid of displayOrders) {
        const nexRecs = codecToJson<any[]>(
          await cc.orderCommissionRecords(oid),
        ) ?? [];
        const tokenRecs = codecToJson<any[]>(
          await cc.orderTokenCommissionRecords(oid),
        ) ?? [];

        // 过滤出当前用户的记录
        const myNex = nexRecs.filter((r: any) => {
          const beneficiary = String(readObjectField(r, 'beneficiary') ?? '');
          return beneficiary === account;
        });
        const myToken = tokenRecs.filter((r: any) => {
          const beneficiary = String(readObjectField(r, 'beneficiary') ?? '');
          return beneficiary === account;
        });

        if (myNex.length === 0 && myToken.length === 0) continue;

        let orderNex = 0n;
        let orderToken = 0n;

        console.log(`  订单 / Order #${oid}:`);

        for (const rec of myNex) {
          const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
          const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
          const status = String(readObjectField(rec, 'status') ?? '');
          const cnType = COMMISSION_TYPE_CN[ctype] ?? ctype;
          const cnStatus = STATUS_CN[status] ?? status;
          orderNex += amount;
          const levelStr = level > 0 ? `L${level}` : '';
          console.log(`    NEX  ${cnType}/${ctype} ${levelStr}  ${formatNex(amount).padStart(18)}  [${cnStatus}]`);
        }

        for (const rec of myToken) {
          const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
          const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
          const status = String(readObjectField(rec, 'status') ?? '');
          const cnType = COMMISSION_TYPE_CN[ctype] ?? ctype;
          const cnStatus = STATUS_CN[status] ?? status;
          orderToken += amount;
          const levelStr = level > 0 ? `L${level}` : '';
          console.log(`    TKN  ${cnType}/${ctype} ${levelStr}  ${formatToken(amount).padStart(18)}  [${cnStatus}]`);
        }

        grandNex += orderNex;
        grandToken += orderToken;
      }

      console.log(`\n  ${ln('─', 56)}`);
      console.log(`  订单佣金合计 / Order Commission Total:`);
      console.log(`    NEX:   ${formatNex(grandNex)}`);
      if (grandToken > 0n) {
        console.log(`    Token: ${formatToken(grandToken)}`);
      }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 8. 实体资金池约束
    // ═══════════════════════════════════════════════════════════════════
    subHeader('8. 实体资金池约束 (提现可用性)', 'Entity Fund Pool Constraints');

    const ss58 = api.registry.chainSS58 ?? 273;
    const treasuryAddr = deriveTreasuryAddress(entityId, ss58);
    const treasuryBalance = await readFreeBalance(api, treasuryAddr);

    const shopPending   = asBigInt(codecToJson(await cc.shopPendingTotal(entityId)));
    const tokenPendingTotal = asBigInt(codecToJson(await cc.tokenPendingTotal(entityId)));
    const unallocPool   = asBigInt(codecToJson(await cc.unallocatedPool(entityId)));
    const unallocToken  = asBigInt(codecToJson(await cc.unallocatedTokenPool(entityId)));
    const pendingRefund = asBigInt(codecToJson(await cc.pendingRefundTotal(entityId)));
    const shopShoppingTotal = asBigInt(codecToJson(await ly.shopShoppingTotal(entityId)));

    const committed = shopPending + unallocPool + pendingRefund;
    const unencumbered = treasuryBalance - committed;

    kv('Treasury 地址',    'Treasury Addr',       shortAddr(treasuryAddr));
    kv('Treasury 余额',    'Treasury Balance',     formatNex(treasuryBalance));
    console.log(`  ${'─'.repeat(56)}`);
    kv('已承诺: 待提佣金', 'Committed: Pending',   formatNex(shopPending));
    kv('已承诺: 未分配池', 'Committed: Unalloc',   formatNex(unallocPool));
    kv('已承诺: 待退款',   'Committed: Refund',    formatNex(pendingRefund));
    kv('已承诺: 购物余额', 'Committed: Shopping',  formatNex(shopShoppingTotal));
    kv('已承诺总计',       'Total Committed',      formatNex(committed));
    console.log(`  ${'─'.repeat(56)}`);
    kv('可自由使用',       'Unencumbered',         formatNex(unencumbered));

    if (tokenPendingTotal > 0n || unallocToken > 0n) {
      console.log(`\n  Token 约束:`);
      kv('Token 待提佣金',   'Token Pending',     formatToken(tokenPendingTotal));
      kv('Token 未分配池',   'Token Unalloc',     formatToken(unallocToken));
    }

    // 验证: 该用户的 pending 不超过实体 shopPending
    if (nexPending > shopPending) {
      console.log(`\n  [!!] 异常: 用户 pending (${formatNex(nexPending)}) > 实体 ShopPendingTotal (${formatNex(shopPending)})`);
    }

    // 验证: Treasury 余额足够覆盖 committed
    if (treasuryBalance < committed) {
      console.log(`\n  [!!] 异常: Treasury (${formatNex(treasuryBalance)}) < Committed (${formatNex(committed)})`);
      console.log(`       实体资金不足以覆盖所有承诺! / Entity funds insufficient for all commitments!`);
    }

    // ═══════════════════════════════════════════════════════════════════
    // 9. 提现可行性判定
    // ═══════════════════════════════════════════════════════════════════
    header('提现可行性判定', 'Withdrawal Feasibility');

    const issues: string[] = [];

    if (globalPaused) issues.push('全局佣金已暂停 / Global commission paused');
    if (entityPaused) issues.push('实体提现已暂停 / Entity withdrawal paused');
    if (nexPending === 0n && tokenPending === 0n) issues.push('无待提现佣金 / No pending commission');

    // 冷却期检查
    if (nexCooldown > 0 && nexLastCredited > 0) {
      const nexCooldownEnd = nexLastCredited + nexCooldown;
      if (currentBlock < nexCooldownEnd) {
        issues.push(`NEX 冷却期未满 (还需 ${nexCooldownEnd - currentBlock} blocks) / NEX cooldown active`);
      }
    }
    if (tokenCooldown > 0 && tokenLastCredited > 0) {
      const tokenCooldownEnd = tokenLastCredited + tokenCooldown;
      if (currentBlock < tokenCooldownEnd) {
        issues.push(`Token 冷却期未满 (还需 ${tokenCooldownEnd - currentBlock} blocks) / Token cooldown active`);
      }
    }

    // 频率检查
    if (minInterval > 0 && nexLastWithdrawn > 0) {
      const nexNext = nexLastWithdrawn + minInterval;
      if (currentBlock < nexNext) {
        issues.push(`NEX 提现间隔未满 (还需 ${nexNext - currentBlock} blocks) / NEX interval not met`);
      }
    }
    if (minInterval > 0 && tokenLastWithdrawn > 0) {
      const tokenNext = tokenLastWithdrawn + minInterval;
      if (currentBlock < tokenNext) {
        issues.push(`Token 提现间隔未满 (还需 ${tokenNext - currentBlock} blocks) / Token interval not met`);
      }
    }

    // Treasury 充足性
    if (treasuryBalance < committed) {
      issues.push('Treasury 资金不足 / Treasury insufficient');
    }

    if (issues.length === 0) {
      console.log('\n  [OK] 所有检查通过，可以提现');
      console.log('  [OK] All checks passed, withdrawal is feasible');

      if (nexPending > 0n) {
        // 模拟提现拆分
        let effectiveRepurchaseRate = govMinNex; // 治理底线
        if (nexWdConfig) {
          const mode = readObjectField(nexWdConfig, 'mode');
          const modeKey = typeof mode === 'string' ? mode : (typeof mode === 'object' && mode ? Object.keys(mode)[0] : 'Unknown');
          if (modeKey === 'FixedRate' && typeof mode === 'object' && mode) {
            const inner = (mode as Record<string, unknown>)[modeKey] as Record<string, unknown> | undefined;
            const fixedRate = coerceNumber(readObjectField(inner, 'repurchaseRate', 'repurchase_rate')) ?? 0;
            effectiveRepurchaseRate = Math.max(effectiveRepurchaseRate, fixedRate);
          }
        }

        const repurchaseAmount = nexPending * BigInt(effectiveRepurchaseRate) / 10000n;
        const walletAmount = nexPending - repurchaseAmount;
        console.log(`\n  NEX 预估提现拆分 / Estimated NEX Split:`);
        console.log(`    待提现 / Pending:     ${formatNex(nexPending)}`);
        console.log(`    到账 / To Wallet:     ${formatNex(walletAmount)}  (${((10000 - effectiveRepurchaseRate) / 100).toFixed(1)}%)`);
        console.log(`    复购 / Repurchase:    ${formatNex(repurchaseAmount)}  (${(effectiveRepurchaseRate / 100).toFixed(1)}%)`);
        console.log(`    有效复购率 / Eff RP:  ${formatBps(effectiveRepurchaseRate)}  (治理底线 ${formatBps(govMinNex)})`);
      }
    } else {
      console.log(`\n  [!!] 存在 ${issues.length} 个阻塞问题 / ${issues.length} blocking issue(s):\n`);
      for (const issue of issues) {
        console.log(`    - ${issue}`);
      }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 10. 资金流总览
    // ═══════════════════════════════════════════════════════════════════
    header('资金流总览', 'Fund Flow Overview');

    console.log(`\n  会员 / Member: ${shortAddr(account)}`);
    console.log(`  实体 / Entity: #${entityId}\n`);

    console.log(`  ┌──────────────────────────────────────────────────┐`);
    console.log(`  │  订单佣金入账                                     │`);
    console.log(`  │  Commission Income                               │`);
    console.log(`  │    NEX 累计: ${formatNex(nexEarned).padEnd(35)}│`);
    if (tokenEarned > 0n) {
      console.log(`  │    Token 累计: ${formatToken(tokenEarned).padEnd(33)}│`);
    }
    console.log(`  ├──────────────────────────────────────────────────┤`);
    console.log(`  │  ┌─ 已提现 (到钱包)                               │`);
    console.log(`  │  │  NEX: ${formatNex(nexWithdrawn).padEnd(41)} │`);
    if (tokenWithdrawn > 0n) {
      console.log(`  │  │  Token: ${formatToken(tokenWithdrawn).padEnd(39)} │`);
    }
    console.log(`  │  ├─ 已复购 (到购物余额)                           │`);
    console.log(`  │  │  NEX: ${formatNex(nexRepurchased).padEnd(41)} │`);
    if (tokenRepurchased > 0n) {
      console.log(`  │  │  Token: ${formatToken(tokenRepurchased).padEnd(39)} │`);
    }
    console.log(`  │  ├─ 待提现 (可提)                                 │`);
    console.log(`  │  │  NEX: ${formatNex(nexPending).padEnd(41)} │`);
    if (tokenPending > 0n) {
      console.log(`  │  │  Token: ${formatToken(tokenPending).padEnd(39)} │`);
    }
    console.log(`  │  └─ 当前购物余额                                  │`);
    console.log(`  │     NEX: ${formatNex(nexShopping).padEnd(41)} │`);
    if (tokenShopping > 0n) {
      console.log(`  │     Token: ${formatToken(tokenShopping).padEnd(39)} │`);
    }
    console.log(`  └──────────────────────────────────────────────────┘`);

    console.log(`\n${ln('═')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误 / Error:', err);
  process.exit(1);
});
