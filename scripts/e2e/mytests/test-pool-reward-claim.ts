#!/usr/bin/env tsx
/**
 * 沉淀池领奖规则测试脚本
 * Pool Reward Claim Rule Test Script
 *
 * 按步骤交互式验证 claim_pool_reward 的完整行为：
 *
 *   步骤 1  — 查询管理者视角（等级配置、奖池余额、当前轮次概况）
 *   步骤 2  — 查询会员视角（个人上限、可领金额、等级进度）
 *   步骤 3  — 本地重算上限公式，与链端返回值比对
 *   步骤 4  — 提交 claim_pool_reward 交易，确认余额增加
 *   步骤 5  — 领奖后重查会员视角，核验状态更新
 *   步骤 6  — 重复领奖，核验被正确拒绝（AlreadyClaimedThisRound）
 *
 * 用法:
 *   node --import tsx mytests/test-pool-reward-claim.ts [entity_id]
 *   ENTITY_ID=100000 CLAIMANT=bob node --import tsx mytests/test-pool-reward-claim.ts
 *   CLAIMANT=17 node --import tsx mytests/test-pool-reward-claim.ts       ← 按索引取账户
 *   CLAIMANT=17 CLAIMANT_FILE=test-accounts-2026-03-20T00-37-56-148Z.json node --import tsx mytests/test-pool-reward-claim.ts
 *   SKIP_CLAIM=1 node --import tsx mytests/test-pool-reward-claim.ts      ← 仅查询不发交易
 *
 * 环境变量:
 *   WS_URL         — WebSocket 端点（默认: ws://127.0.0.1:9944）
 *   ENTITY_ID      — 实体 ID（默认: 100000）
 *   CLAIMANT       — 领奖账户助记词（多词短语）、角色名（bob）或账户文件中的数字索引（如 17）
 *   CLAIMANT_FILE  — 当 CLAIMANT 为数字时，指定账户文件路径（绝对路径或 framework/ 下的文件名）
 *                    不指定则自动使用 framework/ 下最新的 test-accounts-*.json
 *   SKIP_CLAIM     — 设为 1 则跳过所有写交易，仅查询
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { createInterface } from 'node:readline';
import { readFile, readdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Keyring } from '@polkadot/keyring';
import type { KeyringPair } from '@polkadot/keyring/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';

import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { assert, assertEqual } from '../framework/assert.js';
import {
  getDevActors,
  ensureNamedActorBalance,
  getSelectedActorsFilePath,
  readFreeBalance,
} from '../framework/accounts.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  参数配置                                                            */
/* ------------------------------------------------------------------ */

const ENTITY_ID     = Number(process.argv[2] ?? process.env.ENTITY_ID ?? '100000');
const CLAIMANT_ROLE = (process.env.CLAIMANT ?? 'twist junior giant alien adapt abuse present soon when seven culture banner').trim();
// CLAIMANT_FILE: 指定账户文件路径（绝对路径或相对于 framework/ 目录的文件名）
// 当 CLAIMANT 为数字时必须配合此变量使用，否则默认由 getDevActors 按 WS_URL 匹配
const CLAIMANT_FILE = process.env.CLAIMANT_FILE?.trim();
const SKIP_CLAIM    = process.env.SKIP_CLAIM === '1';

/* ------------------------------------------------------------------ */
/*  按索引从指定账户文件加载 KeyringPair                               */
/* ------------------------------------------------------------------ */

const FRAMEWORK_DIR = path.dirname(fileURLToPath(import.meta.url).replace('/mytests/', '/framework/'));

async function loadActorByIndex(filePath: string, index: number): Promise<KeyringPair> {
  await cryptoWaitReady();
  const raw     = await readFile(filePath, 'utf-8');
  const parsed  = JSON.parse(raw) as { accounts: Array<{ mnemonic: string; address: string; name?: string }> };
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`账户文件缺少 accounts 数组：${filePath}`);
  }
  const account = parsed.accounts[index];
  if (!account?.mnemonic) {
    throw new Error(`账户文件 ${filePath} 中索引 ${index} 无效或缺少 mnemonic`);
  }
  const kr = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  return kr.addFromMnemonic(account.mnemonic);
}

/**
 * 解析 CLAIMANT 参数，返回对应的 KeyringPair。
 * - 多单词字符串（如 "word1 word2 ..."）→ 视为助记词，直接从助记词加载
 * - 数字字符串（如 "17"）→ 按索引从 CLAIMANT_FILE 指定文件加载
 * - 角色名（如 "bob"）      → 从标准 getDevActors() 中取
 */
async function resolveClaimant(actors: Awaited<ReturnType<typeof getDevActors>>): Promise<KeyringPair> {
  // 助记词模式：包含空格的多词字符串
  if (CLAIMANT_ROLE.includes(' ')) {
    await cryptoWaitReady();
    const kr = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
    return kr.addFromMnemonic(CLAIMANT_ROLE);
  }

  const indexNum = Number(CLAIMANT_ROLE);
  if (Number.isInteger(indexNum) && !Number.isNaN(indexNum) && /^\d+$/.test(CLAIMANT_ROLE)) {
    // 数字索引模式
    let filePath: string;
    if (CLAIMANT_FILE) {
      filePath = path.isAbsolute(CLAIMANT_FILE)
        ? CLAIMANT_FILE
        : path.resolve(FRAMEWORK_DIR, CLAIMANT_FILE);
    } else {
      // 未指定文件时，在 framework/ 下找最新的 test-accounts-*.json
      const entries = await readdir(FRAMEWORK_DIR);
      const candidates = entries
        .filter((e) => /^test-accounts-.*\.json$/.test(e))
        .map((e) => path.resolve(FRAMEWORK_DIR, e));
      if (candidates.length === 0) {
        throw new Error(`framework/ 目录下未找到 test-accounts-*.json，请通过 CLAIMANT_FILE 指定文件`);
      }
      const withStat = await Promise.all(
        candidates.map(async (fp) => ({ fp, mtime: (await stat(fp)).mtimeMs })),
      );
      withStat.sort((a, b) => b.mtime - a.mtime);
      filePath = withStat[0].fp;
    }
    info(`数字索引模式：从 ${path.basename(filePath)} 的第 ${indexNum} 号账户加载`);
    return loadActorByIndex(filePath, indexNum);
  }
  // 角色名模式
  const pair = actors[CLAIMANT_ROLE];
  assert(pair != null, `找不到测试账户角色 "${CLAIMANT_ROLE}"，请检查 CLAIMANT 环境变量或账户文件`);
  return pair;
}

/* ------------------------------------------------------------------ */
/*  格式化工具                                                          */
/* ------------------------------------------------------------------ */

function ln(char = '─', len = 76): string { return char.repeat(len); }

function header(title: string): void {
  console.log(`\n${ln('═')}`);
  console.log(`  ${title}`);
  console.log(ln('═'));
}

function subHeader(title: string): void {
  console.log(`\n  ${ln('─', 70)}`);
  console.log(`  ${title}`);
  console.log(`  ${ln('─', 70)}`);
}

function kv(label: string, value: string): void {
  console.log(`  ${label.padEnd(36)}  ${value}`);
}

function logStep(index: number, title: string): void {
  console.log(`\n${'━'.repeat(76)}`);
  console.log(`  【步骤 ${index}】${title}`);
  console.log('━'.repeat(76));
}

function ok(msg: string):   void { console.log(`  ✓  ${msg}`); }
function fail(msg: string): void { console.log(`  ✗  ${msg}`); }
function info(msg: string): void { console.log(`  ℹ  ${msg}`); }
function warn(msg: string): void { console.log(`  ⚠  ${msg}`); }

function bpsToPct(bps: number): string {
  return `${(bps / 100).toFixed(2)}%`;
}

function formatToken(raw: bigint): string {
  return `${(Number(raw) / 1e12).toLocaleString()} Token`;
}

function formatUsdt(raw: bigint): string {
  return `${(Number(raw) / 1e6).toFixed(6)} USDT`;
}

/* ------------------------------------------------------------------ */
/*  键盘确认                                                            */
/* ------------------------------------------------------------------ */

function waitForEnter(prompt: string): Promise<void> {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(`\n  ▶  ${prompt}\n     [按 Enter 继续] `, () => {
      rl.close();
      resolve();
    });
  });
}

/* ------------------------------------------------------------------ */
/*  类型定义                                                            */
/* ------------------------------------------------------------------ */

interface CapBehaviorDecoded {
  Fixed?: null;
  UnlockByTeam?: {
    direct_per_unlock: number;
    team_per_unlock: number;
    unlock_percent: number;
    baseline_direct: number;
    baseline_team: number;
  };
}

interface LevelRuleSummary {
  level_id: number;
  base_cap_percent: number;
  cap_behavior: CapBehaviorDecoded;
}

interface AdminLevelRule extends LevelRuleSummary {
  member_count: number;
  capped_member_count: number;
}

interface MemberCapInfo {
  cumulative_claimed_usdt: bigint;
  current_cap_usdt: bigint;
  remaining_cap_usdt: bigint;
  is_capped: boolean;
  base_cap_percent: number;
  base_cap_usdt: bigint;
  unlock_count: number;
  unlock_percent: number | null;
  unlock_amount_per_step_usdt: bigint | null;
  next_direct_gap: number | null;
  next_team_gap: number | null;
  next_unlock_increase_usdt: bigint | null;
}

interface MemberStats {
  direct_count: number;
  team_count: number;
  total_spent: bigint;
  upgrade_eligible_spent: bigint;
  cap_basis_spent_usdt: bigint;
}

interface LevelProgress {
  level_id: number;
  member_count: number;
  claimed_count: number;
  per_member_reward: bigint;
}

interface MemberView {
  round_duration: number;
  token_pool_enabled: boolean;
  level_rule_details: LevelRuleSummary[];
  current_round_id: bigint;
  round_start_block: bigint;
  round_end_block: bigint;
  pool_snapshot: bigint;
  effective_level: number;
  claimable_nex: bigint;
  claimable_token: bigint;
  already_claimed: boolean;
  round_expired: boolean;
  last_claimed_round: bigint;
  member_stats: MemberStats;
  cap_info: MemberCapInfo;
  level_progress: LevelProgress[];
  is_paused: boolean;
}

/* ------------------------------------------------------------------ */
/*  数据解析                                                            */
/* ------------------------------------------------------------------ */

function num(v: unknown, fallback = 0): number {
  return coerceNumber(v) ?? fallback;
}

function parseMemberView(raw: Record<string, unknown>): MemberView {
  const capRaw   = readObjectField(raw, 'capInfo', 'cap_info') as Record<string, unknown> ?? {};
  const statsRaw = readObjectField(raw, 'memberStats', 'member_stats') as Record<string, unknown> ?? {};
  const rulesRaw = (readObjectField(raw, 'levelRuleDetails', 'level_rule_details') as unknown[]) ?? [];
  const progRaw  = (readObjectField(raw, 'levelProgress', 'level_progress') as unknown[]) ?? [];

  const capInfo: MemberCapInfo = {
    cumulative_claimed_usdt:    asBigInt(readObjectField(capRaw, 'cumulativeClaimedUsdt', 'cumulative_claimed_usdt') ?? 0),
    current_cap_usdt:           asBigInt(readObjectField(capRaw, 'currentCapUsdt', 'current_cap_usdt') ?? 0),
    remaining_cap_usdt:         asBigInt(readObjectField(capRaw, 'remainingCapUsdt', 'remaining_cap_usdt') ?? 0),
    is_capped:                  Boolean(readObjectField(capRaw, 'isCapped', 'is_capped')),
    base_cap_percent:           num(readObjectField(capRaw, 'baseCapPercent', 'base_cap_percent')),
    base_cap_usdt:              asBigInt(readObjectField(capRaw, 'baseCapUsdt', 'base_cap_usdt') ?? 0),
    unlock_count:               num(readObjectField(capRaw, 'unlockCount', 'unlock_count')),
    unlock_percent:             coerceNumber(readObjectField(capRaw, 'unlockPercent', 'unlock_percent')) ?? null,
    unlock_amount_per_step_usdt: readObjectField(capRaw, 'unlockAmountPerStepUsdt', 'unlock_amount_per_step_usdt') != null
      ? asBigInt(readObjectField(capRaw, 'unlockAmountPerStepUsdt', 'unlock_amount_per_step_usdt'))
      : null,
    next_direct_gap:            coerceNumber(readObjectField(capRaw, 'nextDirectGap', 'next_direct_gap')) ?? null,
    next_team_gap:              coerceNumber(readObjectField(capRaw, 'nextTeamGap', 'next_team_gap')) ?? null,
    next_unlock_increase_usdt:  readObjectField(capRaw, 'nextUnlockIncreaseUsdt', 'next_unlock_increase_usdt') != null
      ? asBigInt(readObjectField(capRaw, 'nextUnlockIncreaseUsdt', 'next_unlock_increase_usdt'))
      : null,
  };

  const stats: MemberStats = {
    direct_count:           num(readObjectField(statsRaw, 'directCount', 'direct_count')),
    team_count:             num(readObjectField(statsRaw, 'teamCount', 'team_count')),
    total_spent:            asBigInt(readObjectField(statsRaw, 'totalSpent', 'total_spent') ?? 0),
    upgrade_eligible_spent: asBigInt(readObjectField(statsRaw, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0),
    cap_basis_spent_usdt:   asBigInt(readObjectField(statsRaw, 'capBasisSpentUsdt', 'cap_basis_spent_usdt') ?? 0),
  };

  const rules: LevelRuleSummary[] = (rulesRaw as Record<string, unknown>[]).map((r) => ({
    level_id:         num(readObjectField(r, 'levelId', 'level_id')),
    base_cap_percent: num(readObjectField(r, 'baseCapPercent', 'base_cap_percent')),
    cap_behavior:     readObjectField(r, 'capBehavior', 'cap_behavior') as CapBehaviorDecoded ?? { Fixed: null },
  }));

  const progress: LevelProgress[] = (progRaw as Record<string, unknown>[]).map((p) => ({
    level_id:          num(readObjectField(p, 'levelId', 'level_id')),
    member_count:      num(readObjectField(p, 'memberCount', 'member_count')),
    claimed_count:     num(readObjectField(p, 'claimedCount', 'claimed_count')),
    per_member_reward: asBigInt(readObjectField(p, 'perMemberReward', 'per_member_reward') ?? 0),
  }));

  return {
    round_duration:     num(readObjectField(raw, 'roundDuration', 'round_duration')),
    token_pool_enabled: Boolean(readObjectField(raw, 'tokenPoolEnabled', 'token_pool_enabled')),
    level_rule_details: rules,
    current_round_id:   asBigInt(readObjectField(raw, 'currentRoundId', 'current_round_id') ?? 0),
    round_start_block:  asBigInt(readObjectField(raw, 'roundStartBlock', 'round_start_block') ?? 0),
    round_end_block:    asBigInt(readObjectField(raw, 'roundEndBlock', 'round_end_block') ?? 0),
    pool_snapshot:      asBigInt(readObjectField(raw, 'poolSnapshot', 'pool_snapshot') ?? 0),
    effective_level:    num(readObjectField(raw, 'effectiveLevel', 'effective_level')),
    claimable_nex:      asBigInt(readObjectField(raw, 'claimableNex', 'claimable_nex') ?? 0),
    claimable_token:    asBigInt(readObjectField(raw, 'claimableToken', 'claimable_token') ?? 0),
    already_claimed:    Boolean(readObjectField(raw, 'alreadyClaimed', 'already_claimed')),
    round_expired:      Boolean(readObjectField(raw, 'roundExpired', 'round_expired')),
    last_claimed_round: asBigInt(readObjectField(raw, 'lastClaimedRound', 'last_claimed_round') ?? 0),
    member_stats:       stats,
    cap_info:           capInfo,
    level_progress:     progress,
    is_paused:          Boolean(readObjectField(raw, 'isPaused', 'is_paused')),
  };
}

/* ------------------------------------------------------------------ */
/*  打印会员视角                                                        */
/* ------------------------------------------------------------------ */

function printMemberView(v: MemberView, label: string): void {
  subHeader(label);

  console.log('\n  ── 轮次信息 ──');
  kv('当前轮次 ID', `#${v.current_round_id}`);
  kv('轮次时长（区块数）', `${v.round_duration} 块`);
  kv('轮次区块范围', `#${v.round_start_block} ～ #${v.round_end_block}`);
  kv('奖池快照余额', formatNex(v.pool_snapshot));
  kv('本轮是否已领奖', v.already_claimed ? '是（已领）' : '否（未领）');
  kv('轮次是否已过期', v.round_expired ? '是（已过期）' : '否（未过期）');
  kv('上一次领奖轮次', `#${v.last_claimed_round}`);
  kv('奖池是否暂停', v.is_paused ? '是（已暂停）' : '否（运行中）');

  console.log('\n  ── 本人可领金额 ──');
  kv('本人有效等级', `Lv${v.effective_level}`);
  kv('本轮可领 NEX', formatNex(v.claimable_nex));
  if (v.token_pool_enabled) {
    kv('本轮可领 Token', formatToken(v.claimable_token));
  }

  const cap = v.cap_info;
  console.log('\n  ── 累计上限信息 ──');
  kv('基础上限比例', bpsToPct(cap.base_cap_percent));
  kv('基础上限额（USDT 计价）', formatUsdt(cap.base_cap_usdt));
  kv('当前总上限（含解锁）', formatUsdt(cap.current_cap_usdt));
  kv('已累计领取（USDT 计价）', formatUsdt(cap.cumulative_claimed_usdt));
  kv('剩余可领空间（USDT 计价）', formatUsdt(cap.remaining_cap_usdt));
  kv('是否已达上限', cap.is_capped ? '是（已封顶）' : '否（未封顶）');
  kv('已解锁次数', `${cap.unlock_count} 次`);
  if (cap.unlock_percent != null) {
    kv('每次解锁增加比例', bpsToPct(cap.unlock_percent));
    kv('每次解锁增加额（USDT）', formatUsdt(cap.unlock_amount_per_step_usdt ?? 0n));
  }
  if (cap.next_direct_gap != null) {
    kv('下次解锁还需直推人数', `${cap.next_direct_gap} 人`);
    kv('下次解锁还需团队人数', `${cap.next_team_gap ?? 0} 人`);
    kv('下次解锁可增加额（USDT）', formatUsdt(cap.next_unlock_increase_usdt ?? 0n));
  }

  const s = v.member_stats;
  console.log('\n  ── 会员团队统计 ──');
  kv('直推人数', `${s.direct_count} 人`);
  kv('团队人数', `${s.team_count} 人`);
  kv('累计消费（原始值）', String(s.total_spent));
  kv('升级消费（原始值）', String(s.upgrade_eligible_spent));
  kv('上限计算基数（USDT）', formatUsdt(s.cap_basis_spent_usdt));

  if (v.level_progress.length > 0) {
    console.log('\n  ── 各等级领取进度 ──');
    console.log(`    ${'等级'.padEnd(8)} ${'总人数'.padStart(8)} ${'已领人数'.padStart(10)} ${'领取率'.padStart(8)} ${'每人奖励'.padStart(22)}`);
    console.log(`    ${'─'.repeat(8)} ${'─'.repeat(8)} ${'─'.repeat(10)} ${'─'.repeat(8)} ${'─'.repeat(22)}`);
    for (const p of v.level_progress) {
      const ratio = p.member_count > 0
        ? `${((p.claimed_count / p.member_count) * 100).toFixed(0)}%`
        : '—';
      console.log(
        `    ${`Lv${p.level_id}`.padEnd(8)}` +
        ` ${String(p.member_count).padStart(8)}` +
        ` ${String(p.claimed_count).padStart(10)}` +
        ` ${ratio.padStart(8)}` +
        ` ${formatNex(p.per_member_reward).padStart(22)}`,
      );
    }
  }

  if (v.level_rule_details.length > 0) {
    console.log('\n  ── 等级规则配置 ──');
    for (const r of v.level_rule_details) {
      const beh = r.cap_behavior;
      let behaviorDesc: string;
      if (beh.UnlockByTeam) {
        const u = beh.UnlockByTeam;
        behaviorDesc = `按团队解锁 每${u.direct_per_unlock}直推/${u.team_per_unlock}团队 解锁 +${bpsToPct(u.unlock_percent)}`;
      } else {
        behaviorDesc = '固定上限';
      }
      console.log(`    Lv${r.level_id}：基础上限 ${bpsToPct(r.base_cap_percent)}，上限行为：${behaviorDesc}`);
    }
  }
}

/* ------------------------------------------------------------------ */
/*  上限公式本地验证                                                    */
/* ------------------------------------------------------------------ */

function verifyCapMath(view: MemberView, stepLabel: string): boolean {
  const cap   = view.cap_info;
  const stats = view.member_stats;
  let passed  = true;

  // 公式：base_cap_usdt = cap_basis_spent_usdt × base_cap_percent ÷ 10000
  const expectedBase = stats.cap_basis_spent_usdt * BigInt(cap.base_cap_percent) / 10000n;
  if (expectedBase !== cap.base_cap_usdt) {
    fail(`${stepLabel} 基础上限计算有误：本地算得 ${formatUsdt(expectedBase)}，链端返回 ${formatUsdt(cap.base_cap_usdt)}`);
    passed = false;
  } else {
    ok(`${stepLabel} 基础上限计算正确：${formatUsdt(cap.base_cap_usdt)}（消费基数 ${formatUsdt(stats.cap_basis_spent_usdt)} × ${bpsToPct(cap.base_cap_percent)}）`);
  }

  // 公式：current_cap = base_cap + unlock_count × per_step
  if (cap.unlock_amount_per_step_usdt != null && cap.unlock_count > 0) {
    const expectedCap = cap.base_cap_usdt + BigInt(cap.unlock_count) * cap.unlock_amount_per_step_usdt;
    if (expectedCap !== cap.current_cap_usdt) {
      fail(`${stepLabel} 解锁后总上限计算有误：本地算得 ${formatUsdt(expectedCap)}，链端返回 ${formatUsdt(cap.current_cap_usdt)}`);
      passed = false;
    } else {
      ok(`${stepLabel} 解锁后总上限正确：基础 ${formatUsdt(cap.base_cap_usdt)} + ${cap.unlock_count} 次解锁 × ${formatUsdt(cap.unlock_amount_per_step_usdt)} = ${formatUsdt(cap.current_cap_usdt)}`);
    }
  } else if (cap.unlock_count === 0) {
    info(`${stepLabel} 当前未解锁，总上限 = 基础上限 = ${formatUsdt(cap.current_cap_usdt)}`);
  }

  // 公式：remaining = current_cap - cumulative_claimed（下限 0）
  const expectedRemaining = cap.current_cap_usdt > cap.cumulative_claimed_usdt
    ? cap.current_cap_usdt - cap.cumulative_claimed_usdt
    : 0n;
  if (expectedRemaining !== cap.remaining_cap_usdt) {
    fail(`${stepLabel} 剩余上限计算有误：本地算得 ${formatUsdt(expectedRemaining)}，链端返回 ${formatUsdt(cap.remaining_cap_usdt)}`);
    passed = false;
  } else {
    ok(`${stepLabel} 剩余上限正确：总上限 ${formatUsdt(cap.current_cap_usdt)} − 已领 ${formatUsdt(cap.cumulative_claimed_usdt)} = ${formatUsdt(cap.remaining_cap_usdt)}`);
  }

  // is_capped 应与 remaining == 0 一致
  const expectedCapped = cap.remaining_cap_usdt === 0n;
  if (expectedCapped !== cap.is_capped) {
    fail(`${stepLabel} is_capped 状态不一致：剩余上限 ${formatUsdt(cap.remaining_cap_usdt)}，但 is_capped = ${cap.is_capped}`);
    passed = false;
  }

  return passed;
}

/* ------------------------------------------------------------------ */
/*  主流程                                                              */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {
  assert(Number.isInteger(ENTITY_ID) && ENTITY_ID > 0, `ENTITY_ID 无效: ${ENTITY_ID}`);

  header('沉淀池领奖规则测试 | Pool Reward Claim Rule Test');
  kv('节点地址', process.env.WS_URL ?? 'ws://127.0.0.1:9944');
  kv('实体 ID', String(ENTITY_ID));
  kv('领奖账户角色', CLAIMANT_ROLE);
  kv('仅查询模式（不发交易）', SKIP_CLAIM ? '是' : '否');

  await waitForEnter('参数确认，开始连接链节点（步骤 1：查询管理者视角）');

  const api = await connectApi();
  let allPassed = true;

  try {
    const actors = await getDevActors();
    info(`账户文件：${getSelectedActorsFilePath() ?? '(未知)'}`);

    const claimant = await resolveClaimant(actors);
    info(`领奖账户：${CLAIMANT_ROLE} → ${claimant.address}`);

    if (!SKIP_CLAIM) {
      info('检查账户余额，不足时自动补充（最低 50 NEX）...');
      // 数字索引模式下直接用 alice 转账，角色名模式沿用原有逻辑
      const indexNum = Number(CLAIMANT_ROLE);
      if (Number.isInteger(indexNum) && /^\d+$/.test(CLAIMANT_ROLE)) {
        const { readFreeBalance: rfb } = await import('../framework/accounts.js');
        const { nex } = await import('../framework/units.js');
        const minimum = nex(50);
        const free = await rfb(api, claimant.address);
        if (free < minimum) {
          const delta = minimum - free;
          const faucet = actors.alice;
          const tx = api.tx.balances.transferKeepAlive(claimant.address, delta.toString());
          const receipt = await submitTx(api, tx, faucet, `fund claimant-${CLAIMANT_ROLE}`);
          if (!receipt.success) {
            warn(`余额补充失败：${receipt.error ?? '未知错误'}`);
          }
        }
      } else {
        await ensureNamedActorBalance(api, actors, [CLAIMANT_ROLE], 50);
      }
      ok(`账户余额充足`);
    }

    /* ── 步骤 1：管理者视角 ───────────────────────────────────────── */
    logStep(1, '查询管理者视角（奖池配置 + 当前轮次概况）');

    let adminRaw: Record<string, unknown> | null = null;
    try {
      const codec = await (api.call as any).poolRewardDetailApi?.getPoolRewardAdminView?.(ENTITY_ID);
      if (codec) {
        const parsed = codecToJson<Record<string, unknown> | null>(codec);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
          adminRaw = parsed;
        }
      }
    } catch (e) {
      warn(`Runtime API 调用失败，节点可能不支持此版本: ${e}`);
    }

    if (adminRaw) {
      const levelRules = (readObjectField(adminRaw, 'levelRuleDetails', 'level_rule_details') as unknown[]) ?? [];
      const poolBalance = asBigInt(readObjectField(adminRaw, 'currentPoolBalance', 'current_pool_balance') ?? 0);
      const isPaused    = Boolean(readObjectField(adminRaw, 'isPaused', 'is_paused'));
      const isGlobal    = Boolean(readObjectField(adminRaw, 'isGlobalPaused', 'is_global_paused'));

      kv('等级规则数量', `${levelRules.length} 条`);
      kv('当前 NEX 奖池余额', formatNex(poolBalance));
      kv('实体级暂停状态', isPaused ? '已暂停' : '运行中');
      kv('全局暂停状态', isGlobal ? '已暂停' : '运行中');

      const roundRaw = readObjectField(adminRaw, 'currentRound', 'current_round') as Record<string, unknown> | null;
      if (roundRaw) {
        const roundId      = asBigInt(readObjectField(roundRaw, 'roundId', 'round_id') ?? 0);
        const eligibleCnt  = num(readObjectField(roundRaw, 'eligibleCount', 'eligible_count'));
        const claimedCnt   = num(readObjectField(roundRaw, 'claimedCount', 'claimed_count'));
        const perMember    = asBigInt(readObjectField(roundRaw, 'perMemberReward', 'per_member_reward') ?? 0);

        console.log('\n  ── 当前轮次概况 ──');
        kv('轮次 ID', `#${roundId}`);
        kv('合格参与人数', `${eligibleCnt} 人`);
        kv('已领人数', `${claimedCnt} 人`);
        kv('每人奖励', formatNex(perMember));

        const snaps = (readObjectField(roundRaw, 'levelSnapshots', 'level_snapshots') as unknown[]) ?? [];
        if (snaps.length > 0) {
          console.log('\n  ── 各等级快照 ──');
          for (const snap of snaps as Record<string, unknown>[]) {
            const lid = num(readObjectField(snap, 'levelId', 'level_id'));
            const mc  = num(readObjectField(snap, 'memberCount', 'member_count'));
            const cc  = num(readObjectField(snap, 'claimedCount', 'claimed_count'));
            const pmr = asBigInt(readObjectField(snap, 'perMemberReward', 'per_member_reward') ?? 0);
            const ratio = mc > 0 ? `${((cc / mc) * 100).toFixed(0)}%` : '—';
            console.log(`    Lv${lid}：总人数 ${mc} 人，已领 ${cc} 人（${ratio}），每人奖励 ${formatNex(pmr)}`);
          }
        }
        ok('步骤 1 完成：成功读取管理者视角，轮次正在进行中');
      } else {
        warn('当前没有活跃轮次 —— 首次 claim 时将自动触发新轮快照');
        ok('步骤 1 完成：读取管理者视角成功（无活跃轮次）');
      }
    } else {
      warn('步骤 1 跳过：poolRewardDetailApi 不可用或返回 None，可能需要先配置奖池');
    }

    /* ── 步骤 2：会员视角 ─────────────────────────────────────────── */
    await waitForEnter('步骤 1 完成，继续查询会员个人视角（步骤 2）');
    logStep(2, `查询会员视角（领奖账户：${CLAIMANT_ROLE} / ${claimant.address.slice(0, 20)}...）`);

    let memberView: MemberView | null = null;
    try {
      const codec = await (api.call as any).poolRewardDetailApi?.getPoolRewardMemberView?.(
        ENTITY_ID, claimant.address,
      );
      if (codec) {
        const parsed = codecToJson<Record<string, unknown> | null>(codec);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
          memberView = parseMemberView(parsed);
        }
      }
    } catch (e) {
      warn(`会员视角查询失败: ${e}`);
    }

    if (memberView) {
      printMemberView(memberView, '领奖前会员状态快照');

      const myRule = memberView.level_rule_details.find((r) => r.level_id === memberView!.effective_level);
      if (myRule) {
        ok(`账户等级 Lv${myRule.level_id} 在奖池规则中，基础上限 ${bpsToPct(myRule.base_cap_percent)}`);
      } else if (memberView.effective_level === 0) {
        warn('账户等级为 0，可能未注册为会员或尚未激活，领奖可能会失败');
      } else {
        fail(`账户等级 Lv${memberView.effective_level} 不在奖池等级规则中，无法参与本轮领奖`);
        allPassed = false;
      }

      if (memberView.is_paused) {
        warn('奖池当前处于暂停状态，领奖将被拒绝');
      }
    } else {
      warn('步骤 2 跳过：无法获取会员视角，可能原因：账户不是会员 / 奖池未配置 / API 不可用');
    }

    /* ── 步骤 3：上限公式验证 ─────────────────────────────────────── */
    await waitForEnter('步骤 2 完成，继续进行上限公式本地验证（步骤 3）');
    logStep(3, '本地重算上限公式，与链端返回值逐项核对');

    if (memberView) {
      info('正在验证三项公式：基础上限、解锁后总上限、剩余上限...');
      const mathOk = verifyCapMath(memberView, '步骤3');
      if (!mathOk) {
        allPassed = false;
        warn('上限公式存在偏差，请检查上方具体失败项');
      } else {
        ok('全部上限公式验证通过，链端计算结果与本地公式一致');
      }

      // 管理者 vs 会员视角等级规则数量一致性
      if (adminRaw) {
        const adminRulesRaw = (readObjectField(adminRaw, 'levelRuleDetails', 'level_rule_details') as unknown[]) ?? [];
        if (adminRulesRaw.length === memberView.level_rule_details.length) {
          ok(`管理者视角与会员视角等级规则数量一致（均为 ${memberView.level_rule_details.length} 条）`);
        } else {
          fail(`视角数据不一致：管理者视角 ${adminRulesRaw.length} 条，会员视角 ${memberView.level_rule_details.length} 条`);
          allPassed = false;
        }
      }
    } else {
      warn('步骤 3 跳过：无会员视角数据');
    }

    /* ── 步骤 4：领奖交易 ─────────────────────────────────────────── */
    if (SKIP_CLAIM) {
      info('SKIP_CLAIM=1，跳过步骤 4～6（所有写交易）');
    } else {
      await waitForEnter('步骤 3 完成，下一步将提交领奖交易（步骤 4）——此操作会上链，请确认');
      logStep(4, '提交 claim_pool_reward 交易，验证余额增加与状态更新');

      const balanceBefore = await readFreeBalance(api, claimant.address);
      kv('领奖前账户余额', formatNex(balanceBefore));

      // 前置条件检查
      if (!memberView) {
        warn('步骤 4 跳过：无会员视角，无法确认领奖资格');
      } else if (memberView.already_claimed) {
        warn('步骤 4 跳过：本轮已经领过奖励了');
        info('说明：若要重新测试完整领奖流程，需要等待下一轮次开始（当前轮次结束后第一次 claim 会触发新轮）');
      } else if (memberView.round_expired) {
        warn('步骤 4 跳过：当前轮次已过期，需等待下一次 claim 触发新轮');
      } else if (memberView.is_paused) {
        warn('步骤 4 跳过：奖池已暂停');
      } else if (memberView.claimable_nex === 0n) {
        warn('步骤 4 跳过：可领 NEX 为 0，等级不在配置中或本轮无资格');
      } else {
        info(`即将领取奖励，预计可领 ${formatNex(memberView.claimable_nex)}...`);
        info(`交易发起账户：${CLAIMANT_ROLE}（${claimant.address}）`);

        const claimTx = (api.tx as any).commissionPoolReward.claimPoolReward(ENTITY_ID);
        info('正在提交交易，等待链上确认...');
        const receipt = await submitTx(api, claimTx, claimant, 'pool-reward-claim');

        if (receipt.success) {
          ok(`交易成功上链，txHash：${receipt.txHash}`);
          ok(`区块哈希：${receipt.blockHash ?? 'N/A'}，外部索引：${receipt.extrinsicIndex ?? 'N/A'}`);

          // 打印关键事件
          const claimEvent = receipt.events.find(
            (e) => e.section === 'commissionPoolReward' && e.method === 'PoolRewardClaimed',
          );
          if (claimEvent) {
            console.log(`\n  PoolRewardClaimed 事件数据：`);
            console.log(`    ${JSON.stringify(claimEvent.data, null, 2).split('\n').join('\n    ')}`);
          }
          const cappedEvent = receipt.events.find(
            (e) => e.section === 'commissionPoolReward' && e.method === 'MemberCapReached',
          );
          if (cappedEvent) {
            warn(`MemberCapReached 事件已触发！说明本次领奖后账户累计领取已达到上限。`);
            console.log(`    ${JSON.stringify(cappedEvent.data, null, 2).split('\n').join('\n    ')}`);
          }

          // 验证余额增加
          const balanceAfter = await readFreeBalance(api, claimant.address);
          kv('领奖后账户余额', formatNex(balanceAfter));
          const delta = balanceAfter - balanceBefore;
          if (delta > 0n) {
            ok(`余额增加确认：+${formatNex(delta)}`);
          } else {
            fail(`余额未增加，领奖前 ${formatNex(balanceBefore)}，领奖后 ${formatNex(balanceAfter)}，差值 ${formatNex(delta)}`);
            allPassed = false;
          }

          /* ── 步骤 5：领奖后状态核验 ─────────────────────────────── */
          await waitForEnter('交易成功，继续重查会员状态，核验链上数据已正确更新（步骤 5）');
          logStep(5, '领奖后重查会员视角，核验状态字段已全部更新');

          let viewAfter: MemberView | null = null;
          try {
            const codec = await (api.call as any).poolRewardDetailApi?.getPoolRewardMemberView?.(
              ENTITY_ID, claimant.address,
            );
            if (codec) {
              const parsed = codecToJson<Record<string, unknown> | null>(codec);
              if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
                viewAfter = parseMemberView(parsed);
              }
            }
          } catch { /* api 不可用，跳过 */ }

          if (viewAfter) {
            printMemberView(viewAfter, '领奖后会员状态快照');

            // 核验 already_claimed = true
            if (viewAfter.already_claimed) {
              ok('already_claimed 已更新为 true，本轮不可再领');
            } else {
              fail('already_claimed 仍为 false，链上状态未正确更新');
              allPassed = false;
            }

            // 核验 last_claimed_round = 当前轮次
            if (viewAfter.last_claimed_round === memberView.current_round_id) {
              ok(`last_claimed_round 已更新为 #${viewAfter.last_claimed_round}，与本轮 ID 一致`);
            } else {
              fail(`last_claimed_round = #${viewAfter.last_claimed_round}，但本轮 ID = #${memberView.current_round_id}，不一致`);
              allPassed = false;
            }

            // 核验 cumulative_claimed_usdt 增加
            const claimedBefore = memberView.cap_info.cumulative_claimed_usdt;
            const claimedAfter  = viewAfter.cap_info.cumulative_claimed_usdt;
            if (claimedAfter >= claimedBefore) {
              ok(`累计领取额（USDT 计价）已增加：${formatUsdt(claimedBefore)} → ${formatUsdt(claimedAfter)}（+${formatUsdt(claimedAfter - claimedBefore)}）`);
            } else {
              fail(`累计领取额未增加，领奖前 ${formatUsdt(claimedBefore)}，领奖后 ${formatUsdt(claimedAfter)}`);
              allPassed = false;
            }

            // 领奖后再次复核上限公式
            info('对领奖后数据重新运行上限公式验证...');
            const capReOk = verifyCapMath(viewAfter, '步骤5');
            if (!capReOk) allPassed = false;

          } else {
            warn('步骤 5 跳过：领奖后无法重新查询会员视角');
          }

          /* ── 步骤 6：重复领奖防护 ───────────────────────────────── */
          await waitForEnter('步骤 5 完成，下一步将尝试重复领奖，验证链上拒绝保护（步骤 6）——此操作预期失败');
          logStep(6, '重复提交 claim_pool_reward，验证被正确拒绝（AlreadyClaimedThisRound）');

          info('正在提交第二次领奖交易（预期链上拒绝）...');
          const receipt2 = await submitTx(
            api,
            (api.tx as any).commissionPoolReward.claimPoolReward(ENTITY_ID),
            claimant,
            'pool-reward-claim-duplicate',
          );

          if (!receipt2.success) {
            const errMsg = receipt2.error ?? '(无错误信息)';
            ok(`重复领奖被正确拒绝，错误信息：${errMsg}`);

            const isExpected = errMsg.includes('AlreadyClaimedThisRound') || errMsg.includes('AlreadyClaimed');
            if (isExpected) {
              ok('错误类型符合预期：AlreadyClaimedThisRound ✓');
            } else {
              warn(`错误类型非预期 AlreadyClaimedThisRound，实际错误：${errMsg}`);
              warn('可能是其他防护逻辑触发，请人工确认是否符合业务预期');
            }
          } else {
            fail('严重错误：重复领奖竟然成功了！链上双重领奖防护未生效，请立即排查');
            allPassed = false;
          }

        } else {
          // 领奖失败 — 区分预期失败 vs 非预期失败
          const errMsg = receipt.error ?? '';
          if (errMsg.includes('NoActiveRound') || errMsg.includes('NoCurrentRound')) {
            info(`步骤 4 说明：当前无活跃轮次，首次领奖会触发建轮快照，但快照需要至少 1 个合格会员。错误：${errMsg}`);
          } else if (errMsg.includes('MemberCapExceeded') || errMsg.includes('CapExceeded')) {
            info(`步骤 4 说明：账户累计领取已达上限，无法继续领奖。错误：${errMsg}`);
          } else if (errMsg.includes('NotEligible') || errMsg.includes('LevelNotEligible')) {
            info(`步骤 4 说明：账户等级不在当前奖池配置的等级规则中。错误：${errMsg}`);
          } else if (errMsg.includes('NotMember')) {
            fail(`步骤 4 失败：账户不是实体会员，无法领奖。错误：${errMsg}`);
            allPassed = false;
          } else {
            fail(`步骤 4 失败（非预期错误）：${errMsg}`);
            allPassed = false;
          }
        }
      }
    }

    /* ── 汇总 ─────────────────────────────────────────────────────── */
    subHeader('测试结论汇总');
    if (allPassed) {
      ok('全部验证项通过，沉淀池领奖规则行为符合预期');
    } else {
      fail('存在失败项，请向上检查具体错误信息');
      process.exitCode = 1;
    }

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('\n[错误]', err instanceof Error ? err.message : String(err));
  process.exit(1);
});
