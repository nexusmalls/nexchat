#!/usr/bin/env tsx
/**
 * Pool Reward Admin Stats Script
 * 沉淀池奖励管理总览脚本
 *
 * 按等级和人数统计领取 pool reward 奖金情况：
 *   - 等级配置：level_rules（等级 ID、基础上限比例、上限行为）
 *   - 当前轮次：进度、奖池余额、各等级人数与已领人数
 *   - 分发统计：累计 NEX/Token、已完成轮次、总领取次数
 *   - 历史轮次：按轮汇总等级人数与领取情况
 *   - 待生效变更（如有）
 *
 * Usage / 用法:
 *   node --import tsx mytests/pool-reward-stats.ts [entity_id]
 *   ENTITY_ID=100000 node --import tsx mytests/pool-reward-stats.ts
 *
 * Environment / 环境变量:
 *   WS_URL     — WebSocket endpoint (default: ws://127.0.0.1:9944)
 *   ENTITY_ID  — Entity ID (default: 100000)
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

/* ------------------------------------------------------------------ */
/*  Helpers                                                             */
/* ------------------------------------------------------------------ */

function formatToken(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} Token`;
}

function ln(char = '─', len = 80): string {
  return char.repeat(len);
}

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

function pct(count: number, total: number): string {
  if (total === 0) return '—';
  return `${((count / total) * 100).toFixed(1)}%`;
}

function bpsToPct(bps: number): string {
  return `${(bps / 100).toFixed(2)}%`;
}

/* ------------------------------------------------------------------ */
/*  Cap behavior formatting                                             */
/* ------------------------------------------------------------------ */

/**
 * Parse a possibly comma-formatted number string (e.g. "1,000") as a plain number.
 * 解析链上 toHuman() 返回的带逗号数字字符串。
 */
function parseHumanNumber(v: unknown): number {
  if (v == null) return 0;
  if (typeof v === 'number') return v;
  const s = String(v).replace(/,/g, '');
  return Number(s) || 0;
}

/**
 * Format cap behavior from a level rule object (rule is the full level rule, behavior is rule.capBehavior).
 * 格式化等级上限行为，rule 是整个等级规则对象，behavior 是 capBehavior 字段。
 */
function formatCapBehavior(rule: any, behavior: any): string {
  if (behavior == null) return '固定 (Fixed)';

  // behavior is either a string "Fixed" or an object { UnlockByTeam: {...} }
  const key = typeof behavior === 'string'
    ? behavior
    : typeof behavior === 'object' ? Object.keys(behavior)[0] : 'Fixed';

  if (key === 'Fixed' || key === 'fixed') return '固定 (Fixed)';

  if (key === 'UnlockByTeam' || key === 'unlockByTeam') {
    const inner = typeof behavior === 'object' ? behavior[key] : {};
    const directPerUnlock = parseHumanNumber(readObjectField(inner, 'directPerUnlock', 'direct_per_unlock'));
    const teamPerUnlock   = parseHumanNumber(readObjectField(inner, 'teamPerUnlock',   'team_per_unlock'));
    const unlockPercent   = parseHumanNumber(readObjectField(inner, 'unlockPercent',   'unlock_percent'));
    // baselineDirect / baselineTeam live on the rule itself (not inside capBehavior)
    const baselineDirect  = parseHumanNumber(readObjectField(rule, 'baselineDirect',  'baseline_direct'));
    const baselineTeam    = parseHumanNumber(readObjectField(rule, 'baselineTeam',    'baseline_team'));
    return `按团队解锁 +${bpsToPct(unlockPercent)} (基线: ${baselineDirect}/${baselineTeam})`;
  }

  return key;
}

/* ------------------------------------------------------------------ */
/*  Level table                                                         */
/* ------------------------------------------------------------------ */

function printLevelTable(
  title_zh: string,
  title_en: string,
  rows: Array<{
    levelId: number;
    baseCap: number;
    ruleRaw: any;
    behavior: any;
    memberCount: number;
    cappedCount: number;
  }>,
): void {
  subHeader(title_zh, title_en);

  const COL = ['等级', '基础上限', '上限行为', '该等级人数', '已达上限', '达上限占比'];
  const W   = [6, 10, 42, 10, 10, 10];

  console.log();
  const header_row = COL.map((c, i) => c.padEnd(W[i])).join('  ');
  console.log(`  ${header_row}`);
  console.log(`  ${ln('─', 92)}`);

  for (const r of rows) {
    const capPct = pct(r.cappedCount, r.memberCount);
    const cols = [
      `Lv${r.levelId}`,
      bpsToPct(r.baseCap),
      formatCapBehavior(r.ruleRaw, r.behavior),
      String(r.memberCount),
      String(r.cappedCount),
      capPct,
    ];
    console.log(`  ${cols.map((c, i) => c.padEnd(W[i])).join('  ')}`);
  }
}

/* ------------------------------------------------------------------ */
/*  Round level progress table                                          */
/* ------------------------------------------------------------------ */

function printLevelProgress(
  snapshots: any[],
  label: string,
  perMemberFmt: (v: bigint) => string,
): void {
  if (!snapshots || snapshots.length === 0) return;
  console.log(`\n  ${label}:`);
  console.log(`    ${'等级'.padEnd(6)} ${'人数'.padStart(8)} ${'已领'.padStart(8)} ${'占比'.padStart(8)} ${'每人奖励'.padStart(22)}`);
  console.log(`    ${'─'.repeat(6)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(22)}`);
  for (const snap of snapshots) {
    const levelId     = coerceNumber(readObjectField(snap, 'levelId', 'level_id')) ?? 0;
    const memberCount = coerceNumber(readObjectField(snap, 'memberCount', 'member_count')) ?? 0;
    const claimedCount = coerceNumber(readObjectField(snap, 'claimedCount', 'claimed_count')) ?? 0;
    const perMember   = asBigInt(readObjectField(snap, 'perMemberReward', 'per_member_reward') ?? 0);
    console.log(
      `    ${`Lv${levelId}`.padEnd(6)} ${String(memberCount).padStart(8)} ${String(claimedCount).padStart(8)} ${pct(claimedCount, memberCount).padStart(8)} ${perMemberFmt(perMember).padStart(22)}`,
    );
  }
}

/* ------------------------------------------------------------------ */
/*  Main                                                                */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {
  const entityIdArg = process.argv[2] ?? process.env.ENTITY_ID ?? '100000';
  const entityId = Number(entityIdArg);

  if (!Number.isInteger(entityId) || entityId <= 0) {
    console.error('用法 / Usage: npx tsx mytests/pool-reward-stats.ts [entity_id]');
    process.exit(1);
  }

  const api = await connectApi();

  try {
    const spec = `${api.runtimeVersion.specName} v${api.runtimeVersion.specVersion}`;
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();

    header('沉淀池奖励管理总览', 'Pool Reward Admin Stats');
    console.log(`  链 / Chain: ${spec}  |  当前区块 / Block: #${currentBlock}`);
    console.log(`  实体 / Entity: #${entityId}`);

    const pr = (api.query as any).commissionPoolReward;

    // ── 1. 配置 ──
    subHeader('1. 奖池配置', 'Pool Reward Config');

    const configCodec = await pr.poolRewardConfigs(entityId);
    if ((configCodec as any).isNone || configCodec == null) {
      console.log('  (未配置 / No config)');
      await disconnectApi(api);
      return;
    }

    const config = codecToJson<any>(
      (configCodec as any).isSome ? (configCodec as any).unwrap() : configCodec,
    );

    const levelRules: any[] = config?.levelRules ?? config?.level_rules ?? [];
    const roundDuration = parseHumanNumber(readObjectField(config, 'roundDuration', 'round_duration'));
    const tokenEnabled  = !!(config?.tokenPoolEnabled ?? config?.token_pool_enabled);

    kv('轮次时长',    'Round Duration',     `${roundDuration} blocks`);
    kv('Token 池启用', 'Token Pool Enabled', tokenEnabled ? '是 (Yes)' : '否 (No)');
    kv('等级规则数',   'Level Rule Count',   `${levelRules.length}`);

    // ── 2. 各等级人数（从 CappedMemberCount 和 MemberProvider） ──
    // 读取每个等级的会员数和达上限人数
    const levelRows: Array<{
      levelId: number;
      baseCap: number;
      ruleRaw: any;
      behavior: any;
      memberCount: number;
      cappedCount: number;
    }> = [];

    for (const [levelIdRaw, rule] of levelRules) {
      const levelId = parseHumanNumber(levelIdRaw);
      // rule is LevelClaimRule = { base_cap_percent, cap_behavior }
      const baseCap  = parseHumanNumber(readObjectField(rule, 'baseCapPercent', 'base_cap_percent'));
      const behavior = readObjectField(rule, 'capBehavior', 'cap_behavior');

      // member_count: iterate member profiles for this level
      let memberCount = 0;
      try {
        const entries = await (api.query as any).entityMember.memberProfiles.entries(entityId);
        for (const [, value] of entries) {
          const profile = codecToJson<any>(value);
          const lvl = coerceNumber(readObjectField(profile, 'levelId', 'level_id')) ?? 0;
          if (lvl === levelId) memberCount++;
        }
      } catch { /* pallet not available */ }

      // capped count from CappedMemberCount storage
      const cappedCount =
        coerceNumber(codecToJson(await pr.cappedMemberCount(entityId, levelId))) ?? 0;

      levelRows.push({ levelId, baseCap, ruleRaw: rule, behavior, memberCount, cappedCount });
    }

    printLevelTable('等级规则 & 人数统计', 'Level Rules & Member Counts', levelRows);

    // ── 3. 当前轮次 ──
    subHeader('3. 当前轮次', 'Current Round');

    const currentRoundCodec = await pr.currentRound(entityId);
    if ((currentRoundCodec as any).isSome) {
      const round = codecToJson<any>((currentRoundCodec as any).unwrap());

      const roundId        = coerceNumber(readObjectField(round, 'roundId', 'round_id')) ?? 0;
      const startBlock     = coerceNumber(readObjectField(round, 'startBlock', 'start_block')) ?? 0;
      const poolSnapshot   = asBigInt(readObjectField(round, 'poolSnapshot', 'pool_snapshot') ?? 0);
      const eligibleCount  = coerceNumber(readObjectField(round, 'eligibleCount', 'eligible_count')) ?? 0;
      const claimedCount   = coerceNumber(readObjectField(round, 'claimedCount', 'claimed_count')) ?? 0;
      const perMember      = asBigInt(readObjectField(round, 'perMemberReward', 'per_member_reward') ?? 0);
      const tokenSnapshot  = readObjectField(round, 'tokenPoolSnapshot', 'token_pool_snapshot');
      const tokenPerMember = readObjectField(round, 'tokenPerMemberReward', 'token_per_member_reward');
      const tokenClaimed   = coerceNumber(readObjectField(round, 'tokenClaimedCount', 'token_claimed_count')) ?? 0;
      const endBlock       = startBlock + roundDuration;
      const remaining      = Math.max(0, endBlock - currentBlock);

      kv('轮次 ID',   'Round ID',       `#${roundId}`);
      kv('起始区块',  'Start Block',    `#${startBlock}`);
      kv('结束区块',  'End Block',      `#${endBlock} (剩余 ${remaining} blocks)`);
      kv('奖池快照',  'Pool Snapshot',  formatNex(poolSnapshot));
      kv('合格人数',  'Eligible Count', `${eligibleCount}`);
      kv('已领人数',  'Claimed Count',  `${claimedCount} / ${eligibleCount} (${pct(claimedCount, eligibleCount)})`);
      kv('每人奖励',  'Per Member',     formatNex(perMember));

      if (tokenSnapshot != null) {
        const tkSnap = asBigInt(tokenSnapshot);
        const tkPer  = tokenPerMember != null ? asBigInt(tokenPerMember) : 0n;
        kv('Token 快照', 'Token Snapshot',   formatToken(tkSnap));
        kv('Token 每人', 'Token Per Member', formatToken(tkPer));
        kv('Token 已领', 'Token Claimed',    `${tokenClaimed}`);
      }

      // level_quotas (per-level breakdown within current round)
      const levelQuotas: any[] = readObjectField(round, 'levelQuotas', 'level_quotas') as any[] ?? [];
      if (levelQuotas.length > 0) {
        printLevelProgress(levelQuotas, '各等级进度 (NEX)', formatNex);
      }

      const tokenLevelQuotas: any[] | null = readObjectField(round, 'tokenLevelQuotas', 'token_level_quotas') as any[] | null;
      if (tokenLevelQuotas && tokenLevelQuotas.length > 0) {
        printLevelProgress(tokenLevelQuotas, '各等级进度 (Token)', formatToken);
      }
    } else {
      console.log('  (无当前轮次 / No active round)');
    }

    // ── 4. 实时奖池余额 ──
    subHeader('4. 实时奖池余额', 'Live Pool Balance');

    const nexBalance   = asBigInt(codecToJson(await (api.query as any).commissionCore.unallocatedPool(entityId)));
    kv('NEX 池余额', 'NEX Pool Balance', formatNex(nexBalance));

    if (tokenEnabled) {
      const tokenBalance = asBigInt(codecToJson(await (api.query as any).commissionCore.unallocatedTokenPool(entityId)));
      kv('Token 池余额', 'Token Pool Balance', formatToken(tokenBalance));
    }

    // ── 5. 分发统计 ──
    subHeader('5. 累计分发统计', 'Cumulative Distribution Stats');

    const distStats = codecToJson<any>(await pr.distributionStatistics(entityId));
    if (distStats) {
      const totalNex     = asBigInt(readObjectField(distStats, 'totalNexDistributed', 'total_nex_distributed') ?? 0);
      const totalToken   = asBigInt(readObjectField(distStats, 'totalTokenDistributed', 'total_token_distributed') ?? 0);
      const totalRounds  = coerceNumber(readObjectField(distStats, 'totalRoundsCompleted', 'total_rounds_completed')) ?? 0;
      const totalClaims  = coerceNumber(readObjectField(distStats, 'totalClaims', 'total_claims')) ?? 0;

      kv('累计 NEX 分发', 'Total NEX Distributed',    formatNex(totalNex));
      kv('累计 Token 分发', 'Total Token Distributed', formatToken(totalToken));
      kv('已完成轮次',    'Rounds Completed',           `${totalRounds}`);
      kv('累计领取次数',  'Total Claims',               `${totalClaims}`);
    } else {
      console.log('  (无统计数据 / No stats)');
    }

    // ── 6. 历史轮次汇总 ──
    subHeader('6. 历史轮次 (按等级人数汇总)', 'Round History (Level Breakdown)');

    const roundHistory: any[] = codecToJson<any[]>(await pr.roundHistory(entityId)) ?? [];

    if (roundHistory.length === 0) {
      console.log('  (无历史轮次 / No completed rounds)');
    } else {
      console.log(`  共 ${roundHistory.length} 个已完成轮次 / ${roundHistory.length} completed round(s)\n`);

      // Summary table: one row per round
      console.log(`  ${'轮次'.padEnd(6)} ${'区块范围'.padEnd(22)} ${'奖池快照'.padStart(18)} ${'合格人数'.padStart(8)} ${'已领人数'.padStart(8)} ${'领取率'.padStart(8)}`);
      console.log(`  ${'─'.repeat(6)} ${'─'.repeat(22)} ${'─'.repeat(18)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(8)}`);

      for (const r of roundHistory) {
        const rid     = coerceNumber(readObjectField(r, 'roundId', 'round_id')) ?? 0;
        const sBlock  = coerceNumber(readObjectField(r, 'startBlock', 'start_block')) ?? 0;
        const eBlock  = coerceNumber(readObjectField(r, 'endBlock', 'end_block')) ?? 0;
        const snap    = asBigInt(readObjectField(r, 'poolSnapshot', 'pool_snapshot') ?? 0);
        const eligible = coerceNumber(readObjectField(r, 'eligibleCount', 'eligible_count')) ?? 0;
        const claimed  = coerceNumber(readObjectField(r, 'claimedCount', 'claimed_count')) ?? 0;

        console.log(
          `  ${`#${rid}`.padEnd(6)} ${`#${sBlock}~#${eBlock}`.padEnd(22)} ${formatNex(snap).padStart(18)} ${String(eligible).padStart(8)} ${String(claimed).padStart(8)} ${pct(claimed, eligible).padStart(8)}`,
        );
      }

      // Per-round level breakdown (last 5 rounds to avoid excessive output)
      const displayRounds = roundHistory.slice(-5);
      if (roundHistory.length > 5) {
        console.log(`\n  (仅展示最近 5 轮的等级明细 / Showing last 5 rounds level detail)\n`);
      }

      for (const r of displayRounds) {
        const rid     = coerceNumber(readObjectField(r, 'roundId', 'round_id')) ?? 0;
        const levelQuotas: any[] = readObjectField(r, 'levelQuotas', 'level_quotas') as any[] ?? [];

        if (levelQuotas.length > 0) {
          console.log(`\n  轮次 #${rid} 等级明细 / Round #${rid} Level Detail:`);
          console.log(`    ${'等级'.padEnd(6)} ${'人数'.padStart(8)} ${'已领'.padStart(8)} ${'领取率'.padStart(8)} ${'每人奖励'.padStart(22)}`);
          console.log(`    ${'─'.repeat(6)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(22)}`);

          for (const snap of levelQuotas) {
            const lvl  = coerceNumber(readObjectField(snap, 'levelId', 'level_id')) ?? 0;
            const cnt  = coerceNumber(readObjectField(snap, 'memberCount', 'member_count')) ?? 0;
            const clmd = coerceNumber(readObjectField(snap, 'claimedCount', 'claimed_count')) ?? 0;
            const per  = asBigInt(readObjectField(snap, 'perMemberReward', 'per_member_reward') ?? 0);

            console.log(
              `    ${`Lv${lvl}`.padEnd(6)} ${String(cnt).padStart(8)} ${String(clmd).padStart(8)} ${pct(clmd, cnt).padStart(8)} ${formatNex(per).padStart(22)}`,
            );
          }
        }
      }
    }

    // ── 7. 待生效配置变更 ──
    const pendingCodec = await pr.pendingPoolRewardConfig(entityId);
    if (pendingCodec != null && !(pendingCodec as any).isNone) {
      subHeader('7. 待生效配置变更', 'Pending Config Change');

      const pending = codecToJson<any>(
        (pendingCodec as any).isSome ? (pendingCodec as any).unwrap() : pendingCodec,
      );
      const applyAfter   = coerceNumber(readObjectField(pending, 'applyAfter', 'apply_after')) ?? 0;
      const newDuration  = coerceNumber(readObjectField(pending, 'roundDuration', 'round_duration')) ?? 0;
      const newRules: any[] = readObjectField(pending, 'levelRules', 'level_rules') as any[] ?? [];

      kv('生效区块',     'Apply After Block', `#${applyAfter}`);
      kv('新轮次时长',   'New Round Duration', `${newDuration} blocks`);
      kv('新等级规则数', 'New Level Count',    `${newRules.length}`);

      if (newRules.length > 0) {
        console.log('\n  新等级规则 / New Level Rules:');
        for (const [levelIdRaw, rule] of newRules) {
          const lid  = coerceNumber(levelIdRaw) ?? 0;
          const cap  = coerceNumber(readObjectField(rule, 'baseCapPercent', 'base_cap_percent')) ?? 0;
          const beh  = readObjectField(rule, 'capBehavior', 'cap_behavior');
          console.log(`    Lv${lid}: 基础上限 ${bpsToPct(cap)} — ${formatCapBehavior(rule, beh)}`);
        }
      }
    }

    // ── 8. 暂停状态 ──
    subHeader('8. 暂停状态', 'Pause Status');

    const isPaused       = !!(codecToJson(await pr.poolRewardPaused(entityId)));
    const isGlobalPaused = !!(codecToJson(await pr.globalPoolRewardPaused()));

    kv('实体暂停',   'Entity Paused',  isPaused       ? '[暂停]' : '运行中 (Running)');
    kv('全局暂停',   'Global Paused',  isGlobalPaused ? '[暂停]' : '运行中 (Running)');

    console.log(`\n${ln('═')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误 / Error:', err);
  process.exit(1);
});
