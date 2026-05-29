#!/usr/bin/env tsx
/**
 * 通过下级推荐链连续升级主账户至 7 星，并追加高余轮次验证超额稳定性。
 * Upgrade Main Account to 7-Star via Referral Chain, then add surplus rounds to verify over-threshold stability.
 *
 * 链上等级升级条件（来自节点查询）/ On-chain level upgrade conditions (queried from node):
 *   Level 5 (5星): threshold≥50,000,000  | direct≥0 | indirect≥5
 *   Level 6 (6星): threshold≥150,000,000 | direct≥3 | indirect≥8
 *   Level 7 (7星): threshold≥500,000,000 | direct≥5 | indirect≥10
 *   注意：calculate_custom_level_full 按等级顺序逐级检查，遇到不满足即停止。
 *   Note: levels are checked sequentially; any level failing breaks the chain.
 *
 * 场景描述 / Scenario:
 *   阶段一（达标）:
 *   Phase 1 (reach 7-star):
 *     1. 主账户购买 Plan7（500 USDT）→ 消费阈值一步满足所有等级（L1~L7 均≥threshold）
 *     2. 招募 5 个直推 L1（满足 direct≥5，同时满足 L6 的 direct≥3）
 *     3. 每个 L1 各带 2 个间接下级 L2（共 10 个，满足 indirect≥10，同时满足 L5/L6 的 indirect≥5/8）
 *     所有条件满足后，AutoUpgrade 策略在下次读取有效等级时自动写穿补升至 7 星。
 *
 *   阶段二（高余轮次）:
 *   Phase 2 (surplus rounds — verify over-threshold stability):
 *     每轮追加 3 个直推 L1 + 4 个间接 L2，分配规则：
 *     Each surplus round adds 3 direct L1s + 4 indirect L2s, distributed as:
 *       L1[round_1]: 2 个 L2 / 2 L2s
 *       L1[round_2]: 1 个 L2 / 1 L2
 *       L1[round_3]: 1 个 L2 / 1 L2
 *     共 SURPLUS_ROUNDS 轮（默认 10 轮），每轮结束后断言等级仍为 7 星。
 *     Total SURPLUS_ROUNDS rounds (default 10), asserting level remains 7-star after each.
 *     Total SURPLUS_ROUNDS rounds (default 2), asserting level remains 7-star after each.
 *
 * 账户结构 / Account structure:
 *   L0  — 主账户（MAIN_ACCOUNTS_FILE index MAIN_INDEX，默认 17）
 *   达标阶段:
 *     L1[0..4]   — 5 个直推（运行时动态从账户池中找到第 1~5 个未注册账户）
 *     L2[i][0,1] — 每个 L1 带 2 个间接下级（运行时动态从账户池中找到第 6~15 个未注册账户）
 *   高余阶段（每轮）:
 *     L1[r][0..2] — 3 个直推（从剩余未注册池中依次分配）
 *     L2[r][0..3] — 4 个间接下级（前1 L1带2个，后2 L1各带1个）
 *
 * 下单账户来源 / Order Account Sources:
 *   四个账户文件合并为统一账户池（共 160 个账户）：
 *     FILE_A: test-accounts-2026-03-20T00-37-56-148Z.json  (pool index   0- 19)
 *     FILE_B: test-accounts-2026-03-20T00-38-47-605Z.json  (pool index  20- 39)
 *     FILE_C: test-accounts-2026-03-20T01-03-22-751Z.json  (pool index  40- 59)
 *     FILE_D: test-accounts-2026-04-03T11-54-52-606Z.json  (pool index  60-159)
 *
 *   脚本在连接链后自动扫描账户池，按 pool index 从小到大找出未注册账户，
 *   达标阶段需要 NUM_L1 + NUM_L1*L2_PER_L1 = 15 个；
 *   高余阶段每轮额外需要 SURPLUS_L1_PER_ROUND + SURPLUS_L2_PER_ROUND = 7 个；
 *   10 轮共需 15 + 70 = 85 个，160 个账户池可覆盖。
 *   Script auto-scans the pool after connecting; base phase needs 15, each surplus round needs 7 more;
 *   10 rounds total = 85 accounts needed, covered by the 160-account pool.
 *
 * 用法 / Usage:
 *   node --import tsx mytests/upgrade-via-referral-chain.ts [entityId] [plan7ProductId] [plan1ProductId]
 *
 * 环境变量 / Environment variables:
 *   WS_URL              — WebSocket 端点（默认: ws://127.0.0.1:9944）
 *   ENTITY_ID           — 实体 ID（默认: 100000）
 *   PLAN7_PRODUCT_ID    — 7 星套餐商品 ID（默认: 1）
 *   PLAN1_PRODUCT_ID    — 1 星套餐商品 ID（默认: 1）
 *   MAIN_ACCOUNTS_FILE  — 主账户所在 JSON 文件路径
 *   MAIN_INDEX          — 主账户在该文件中的 index（默认: 17）
 *   SURPLUS_ROUNDS      — 高余轮次数（默认: 10）
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { createInterface } from 'node:readline';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { KeyringPair } from '@polkadot/keyring/types';

import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { assert, assertTxSuccess } from '../framework/assert.js';
import { readFreeBalance } from '../framework/accounts.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, nex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  参数配置 / Parameter Configuration                                 */
/* ------------------------------------------------------------------ */

const ENTITY_ID         = Number(process.argv[2] ?? process.env.ENTITY_ID         ?? '100000');
const PLAN7_PRODUCT_ID  = Number(process.argv[3] ?? process.env.PLAN7_PRODUCT_ID  ?? '1');
const PLAN1_PRODUCT_ID  = Number(process.argv[4] ?? process.env.PLAN1_PRODUCT_ID  ?? '1');
const MAIN_INDEX        = Number(process.env.MAIN_INDEX ?? '17');

// 达标阶段配置 / Base phase config (reach 7-star)
// 7 星需要 direct≥5；L6 需要 direct≥3 — 统一用 5 个 L1 同时满足两级要求
// 7-star requires direct≥5; L6 requires direct≥3 — use 5 L1s to satisfy both levels
const NUM_L1    = 5;
// 5 个 L1 × 2 个 L2 = 10 间接下级，满足 indirect≥10（同时满足 L5/L6 的 indirect≥5/8）
// 5 L1s × 2 L2s = 10 indirect, satisfying indirect≥10 (also covers L5/L6 requirements)
const L2_PER_L1 = 2;

// 高余阶段配置 / Surplus phase config (over-threshold stability verification)
// 每轮 +3 直推 L1，+4 间接 L2：L1[round_0]带2个L2，L1[round_1]带1个L2，L1[round_2]带1个L2
// Each round: +3 direct L1s, +4 indirect L2s: L1[0] gets 2 L2s, L1[1] gets 1, L1[2] gets 1
const SURPLUS_ROUNDS         = Number(process.env.SURPLUS_ROUNDS ?? '10');
const SURPLUS_L1_PER_ROUND   = 3;
const SURPLUS_L2_PER_ROUND   = 4;
// L2 distribution within each surplus round: index → number of L2s this L1 gets
// 轮内 L2 分配：第 0 个 L1 带 2 个 L2，第 1、2 个 L1 各带 1 个
const SURPLUS_L2_DIST: readonly number[] = [2, 1, 1] as const;

const FRAMEWORK_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../framework');

const MAIN_ACCOUNTS_FILE = process.env.MAIN_ACCOUNTS_FILE
  ?? path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T00-37-56-148Z.json');

// 四个下单账户文件，合并为统一账户池（共 160 个账户）
// Four order-account files merged into a unified pool (160 accounts total)
//   FILE_A (pool index   0- 19): test-accounts-2026-03-20T00-37-56-148Z.json
//   FILE_B (pool index  20- 39): test-accounts-2026-03-20T00-38-47-605Z.json
//   FILE_C (pool index  40- 59): test-accounts-2026-03-20T01-03-22-751Z.json
//   FILE_D (pool index  60-159): test-accounts-2026-04-03T11-54-52-606Z.json
const SUB_ACCOUNT_FILES: string[] = [
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T00-37-56-148Z.json'),
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T00-38-47-605Z.json'),
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-03-20T01-03-22-751Z.json'),
  path.join(FRAMEWORK_DIR, 'test-accounts-2026-04-03T11-54-52-606Z.json'),
];

// Sub-account pool indices are determined dynamically at runtime by scanning the chain.
// 下级账户 pool index 在运行时动态查询链上状态确定，无需手动维护。

/* ------------------------------------------------------------------ */
/*  格式化工具 / Formatting Utilities                                  */
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

function logStep(index: number | string, title: string): void {
  console.log(`\n${'━'.repeat(76)}`);
  console.log(`  【步骤 ${index}】${title}`);
  console.log('━'.repeat(76));
}

function info(msg: string): void { console.log(`  ℹ  ${msg}`); }
function ok(msg: string):   void { console.log(`  ✓  ${msg}`); }
function warn(msg: string): void { console.log(`  ⚠  ${msg}`); }

/* ------------------------------------------------------------------ */
/*  键盘确认 / Keyboard Confirmation                                   */
/* ------------------------------------------------------------------ */

/**
 * 暂停执行，等待用户按下 Enter 键后继续。
 * 每个关键操作前调用，确保操作者可以人工审查当前状态再决定是否继续。
 *
 * Pauses execution until the user presses Enter.
 * Called before each critical operation so the operator can review and decide.
 */
function waitForEnter(prompt: string): Promise<void> {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(`\n  ▶  ${prompt}\n     [按 Enter 继续 / Press Enter to continue] `, () => {
      rl.close();
      resolve();
    });
  });
}

/**
 * 暂停执行，等待用户按下 Enter 继续或输入 s 终止高余轮次。
 * 返回 'continue' 或 'stop'。
 *
 * Pauses execution; returns 'continue' if Enter pressed, 'stop' if user types 's' then Enter.
 * Used in surplus rounds to allow early exit without aborting the whole script.
 */
function waitForEnterOrStop(prompt: string): Promise<'continue' | 'stop'> {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(
      `\n  ▶  ${prompt}\n     [按 Enter 继续 / Enter=continue | 输入 s 终止高余轮次 / s=stop surplus] `,
      (answer) => {
        rl.close();
        resolve(answer.trim().toLowerCase() === 's' ? 'stop' : 'continue');
      },
    );
  });
}

/* ------------------------------------------------------------------ */
/*  账户加载 / Account Loading                                          */
/* ------------------------------------------------------------------ */

interface JsonAccountEntry {
  index?: number;
  name?: string;
  mnemonic: string;
  address: string;
}

interface JsonAccountFile {
  accounts: JsonAccountEntry[];
}

async function loadAccountsFromFile(filePath: string): Promise<JsonAccountEntry[]> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as JsonAccountFile;
  assert(Array.isArray(parsed.accounts) && parsed.accounts.length > 0,
    `账户文件格式无效 / Invalid accounts file format: ${filePath}`);
  return parsed.accounts;
}

function buildKeypair(entry: JsonAccountEntry, keyring: Keyring): KeyringPair {
  return keyring.addFromMnemonic(entry.mnemonic);
}

/* ------------------------------------------------------------------ */
/*  会员状态快照 / Member Snapshot                                      */
/* ------------------------------------------------------------------ */

type MemberSnapshot = {
  exists: boolean;
  directReferrals: number;
  indirectReferrals: number;
  teamSize: number;
  totalSpent: bigint;
  upgradeEligibleSpent: bigint;
  activated: boolean;
  customLevelId: number;
  effectiveLevelId: number;
  orderCount: number;
};

function asBigInt(value: unknown): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') {
    const cleaned = value.replace(/,/g, '').trim();
    return cleaned ? BigInt(cleaned) : 0n;
  }
  if (value != null && typeof (value as any).toString === 'function') {
    try { return BigInt((value as any).toString()); } catch { return 0n; }
  }
  return 0n;
}

async function readMemberSnapshot(api: any, entityId: number, address: string): Promise<MemberSnapshot> {
  const raw = await (api.query as any).entityMember.entityMembers(entityId, address);
  const exists = !(raw as any).isNone;
  const member = exists ? codecToJson<Record<string, unknown>>((raw as any).unwrap()) : null;
  const orderCount = coerceNumber(
    ((await (api.query as any).entityMember.memberOrderCount(entityId, address)) as any).toJSON()
  ) ?? 0;

  let effectiveLevelId = 0;
  try {
    const maybeInfo = await (api.call as any).memberTeamApi?.getMemberInfo?.(entityId, address);
    const parsed = codecToJson<Record<string, unknown> | null>(maybeInfo);
    if (parsed) {
      effectiveLevelId = coerceNumber(readObjectField(parsed, 'effectiveLevelId', 'effective_level_id')) ?? 0;
    }
  } catch {
    // runtime api unavailable — fall back to storage value
    effectiveLevelId = member ? (coerceNumber(readObjectField(member, 'customLevelId', 'custom_level_id')) ?? 0) : 0;
  }

  return {
    exists,
    directReferrals:      member ? (coerceNumber(readObjectField(member, 'directReferrals',      'direct_referrals'))      ?? 0)  : 0,
    indirectReferrals:    member ? (coerceNumber(readObjectField(member, 'indirectReferrals',    'indirect_referrals'))    ?? 0)  : 0,
    teamSize:             member ? (coerceNumber(readObjectField(member, 'teamSize',             'team_size'))             ?? 0)  : 0,
    totalSpent:           member ? asBigInt(readObjectField(member, 'totalSpent',           'total_spent')           ?? 0) : 0n,
    upgradeEligibleSpent: member ? asBigInt(readObjectField(member, 'upgradeEligibleSpent', 'upgrade_eligible_spent') ?? 0) : 0n,
    activated:            member ? Boolean(readObjectField(member, 'activated')) : false,
    customLevelId:        member ? (coerceNumber(readObjectField(member, 'customLevelId',        'custom_level_id'))        ?? 0)  : 0,
    effectiveLevelId,
    orderCount,
  };
}

function printMemberSnapshot(label: string, snap: MemberSnapshot): void {
  console.log(`\n  ── ${label} ──`);
  kv('已是会员 / Is Member',           String(snap.exists));
  kv('激活状态 / Activated',            String(snap.activated));
  kv('自定义等级 ID / Custom Level ID', String(snap.customLevelId));
  kv('有效等级 ID / Effective Level',   String(snap.effectiveLevelId));
  kv('直推数 / Direct Referrals',       String(snap.directReferrals));
  kv('间接下级数 / Indirect Referrals', String(snap.indirectReferrals));
  kv('团队人数 / Team Size',            String(snap.teamSize));
  kv('累计消费 / Total Spent',          String(snap.totalSpent));
  kv('升级消费 / Eligible Spent',       String(snap.upgradeEligibleSpent));
  kv('已完成订单数 / Completed Orders',   String(snap.orderCount));
}

/* ------------------------------------------------------------------ */
/*  交易辅助 / Transaction Helpers                                      */
/* ------------------------------------------------------------------ */

function extractOrderId(receipt: TxReceipt): number | null {
  const event = receipt.events.find(
    (e) => e.section === 'entityTransaction' && e.method === 'OrderCreated'
  );
  if (!event) return null;
  return coerceNumber(readObjectField(event.data as any, 'orderId', 'order_id')) ?? null;
}

/**
 * 为目标账户从水龙头补充余额至 minNex NEX（不足时才转账）。
 * Tops up target account from faucet to minNex NEX if current balance is insufficient.
 */
async function ensureBalance(
  api: any,
  faucet: KeyringPair,
  target: KeyringPair,
  label: string,
  minNex: number,
): Promise<void> {
  const minimum = nex(minNex);
  const free = await readFreeBalance(api, target.address);
  if (free >= minimum) {
    info(`${label} 余额充足（${formatNex(free)}），跳过补充。`);
    return;
  }
  const delta = minimum - free;
  info(`${label} 余额不足（${formatNex(free)}），从水龙头补充 ${formatNex(delta)}...`);
  const tx = api.tx.balances.transferKeepAlive(target.address, delta.toString());
  const receipt = await submitTx(api, tx, faucet, `fund-${label}`);
  assertTxSuccess(receipt, `补充 ${label} 余额失败 / Failed to fund ${label}`);
  ok(`${label} 余额补充完成：${formatNex(await readFreeBalance(api, target.address))}`);
}

/**
 * 购买套餐（幂等：已有订单则跳过）。
 * Place order idempotently (skip if already has orders).
 */
async function buyPlan(
  api: any,
  buyer: KeyringPair,
  productId: number,
  referrer: KeyringPair | null,
  label: string,
  entityId: number,
): Promise<void> {
  const snap = await readMemberSnapshot(api, entityId, buyer.address);
  if (snap.exists) {
    // 检查推荐人是否符合预期，避免跳过后主账户直推计数不增加
    // Check referrer matches expectation, so we don't silently skip while referrer is wrong
    if (referrer) {
      const memberRaw = await (api.query as any).entityMember.entityMembers(entityId, buyer.address);
      if ((memberRaw as any).isSome) {
        const memberData = codecToJson<Record<string, unknown>>((memberRaw as any).unwrap());
        const refField = readObjectField(memberData, 'referrer');
        let actualReferrer: string | null = null;
        if (refField != null && refField !== '' && typeof refField === 'object') {
          const inner = (refField as any).some ?? (refField as any).Some ?? (refField as any).value ?? refField;
          actualReferrer = typeof inner === 'string' ? inner : null;
        } else if (typeof refField === 'string' && refField.length > 0) {
          actualReferrer = refField;
        }
        if (actualReferrer !== referrer.address) {
          warn(`${label} 已是会员但推荐人不匹配！实际推荐人: ${actualReferrer ?? '(无)'}，期望: ${referrer.address}`);
          warn(`${label} is already a member but referrer mismatch! Cannot fix — will NOT count toward expected referrer's direct_referrals.`);
          return;
        }
      }
    }
    info(`${label} 已是会员（Level ${snap.effectiveLevelId}，已完成订单 ${snap.orderCount} 单），推荐人正确，跳过购买。`);
    info(`${label} already a member with correct referrer (Level ${snap.effectiveLevelId}), skipping.`);
    return;
  }
  info(`${label} 正在购买 productId=${productId}，推荐人=${referrer?.address ?? '无'}...`);
  const tx = (api.tx as any).entityTransaction.placeOrder(
    productId, 1,
    null, null, null, null,
    referrer?.address ?? null,
    null, null,
  );
  const receipt = await submitTx(api, tx, buyer, label);
  assertTxSuccess(receipt, `${label} 购买失败 / purchase failed`);
  const orderId = extractOrderId(receipt);
  ok(`${label} 购买成功 — 订单 ID: ${orderId ?? 'N/A'}，txHash: ${receipt.txHash}`);
}

/* ------------------------------------------------------------------ */
/*  主流程 / Main Flow                                                  */
/* ------------------------------------------------------------------ */

async function main(): Promise<void> {

  /* ── 0. 参数校验 ── */
  assert(Number.isInteger(ENTITY_ID) && ENTITY_ID > 0,
    `ENTITY_ID 无效 / Invalid ENTITY_ID: ${ENTITY_ID}`);
  assert(Number.isInteger(PLAN7_PRODUCT_ID) && PLAN7_PRODUCT_ID > 0,
    `PLAN7_PRODUCT_ID 无效 / Invalid: ${PLAN7_PRODUCT_ID}`);
  assert(Number.isInteger(PLAN1_PRODUCT_ID) && PLAN1_PRODUCT_ID > 0,
    `PLAN1_PRODUCT_ID 无效 / Invalid: ${PLAN1_PRODUCT_ID}`);
  assert(Number.isInteger(SURPLUS_ROUNDS) && SURPLUS_ROUNDS >= 0,
    `SURPLUS_ROUNDS 无效 / Invalid SURPLUS_ROUNDS: ${SURPLUS_ROUNDS}`);

  header('通过下级推荐链升级主账户至 7 星并验证高余稳定性 | Upgrade to 7-Star + Surplus Rounds');

  /* ── 1. 加载账户文件（不含密钥）/ Load account files (entries only) ── */
  logStep(1, '加载账户文件并初始化密钥 | Load account files and init keypairs');

  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });

  // 主账户文件
  // Main account file
  const mainAccounts = await loadAccountsFromFile(MAIN_ACCOUNTS_FILE);
  assert(mainAccounts.length > MAIN_INDEX,
    `主账户文件账户数不足 / Not enough in main file: need index ${MAIN_INDEX}, got ${mainAccounts.length}`);

  // 合并三个下单账户文件
  // Merge three order-account files into a unified pool
  info('正在合并三个下单账户文件...');
  const subPool: JsonAccountEntry[] = [];
  for (const filePath of SUB_ACCOUNT_FILES) {
    const entries = await loadAccountsFromFile(filePath);
    subPool.push(...entries);
    info(`  已加载 ${entries.length} 个账户 from ${path.basename(filePath)}`);
  }
  ok(`合并账户池共 ${subPool.length} 个账户 / Merged pool: ${subPool.length} accounts`);

  const mainAccount = buildKeypair(mainAccounts[MAIN_INDEX], keyring);
  // 水龙头账户
  // Faucet account (operator from setup-entity-full.ts)
  const faucet = keyring.addFromMnemonic('fabric smile father unique elbow buffalo until emerge novel orient rally basket');

  // 打印已知基础信息
  kv('主账户文件 / Main File',            path.basename(MAIN_ACCOUNTS_FILE));
  kv('下单账户文件池 / Sub Files',         SUB_ACCOUNT_FILES.map(f => path.basename(f)).join(', '));
  kv('节点地址 / WS URL',                  process.env.WS_URL ?? 'ws://127.0.0.1:9944');
  kv('实体 ID / Entity ID',               String(ENTITY_ID));
  kv('7 星商品 ID / Plan7 Product ID',    String(PLAN7_PRODUCT_ID));
  kv('1 星商品 ID / Plan1 Product ID',    String(PLAN1_PRODUCT_ID));
  kv('直推 L1 数量 / NUM_L1',             String(NUM_L1));
  kv('每 L1 间接下级数 / L2_PER_L1',      String(L2_PER_L1));
  kv('总间接下级数 / Total Indirect',      String(NUM_L1 * L2_PER_L1));
  kv('高余轮次 / SURPLUS_ROUNDS',          String(SURPLUS_ROUNDS));
  kv('每轮直推数 / SURPLUS_L1_PER_ROUND',  String(SURPLUS_L1_PER_ROUND));
  kv('每轮间推数 / SURPLUS_L2_PER_ROUND',  String(SURPLUS_L2_PER_ROUND));
  console.log();
  kv('水龙头 / Faucet',                   faucet.address);
  kv(`主账户 L0（main file index ${MAIN_INDEX}）`, mainAccount.address);

  // 验证主账户地址
  const EXPECTED_MAIN_ADDRESS = 'X4VDNGNo92nUvFxY1dguwEnAeM2deFdLVjLA1b1wLjR7isVCp';
  if (mainAccount.address === EXPECTED_MAIN_ADDRESS) {
    ok(`主账户地址验证通过：${mainAccount.address}`);
  } else {
    warn(`主账户地址（${mainAccount.address}）与预期（${EXPECTED_MAIN_ADDRESS}）不符，请检查配置。`);
    warn(`Main account address mismatch! Expected: ${EXPECTED_MAIN_ADDRESS}`);
  }

  await waitForEnter('账户信息确认完毕，继续执行（步骤 2：连接链、扫描未注册账户并补充余额）');

  /* ── 2. 连接链 + 动态扫描未注册账户 + 补充余额 ── */
  logStep(2, '连接节点、扫描未注册账户并补充余额 | Connect, scan unregistered accounts, fund all');

  const api = await connectApi();
  ok('链节点连接成功 / Connected to chain node');

  try {
    // 读取主店铺 ID
    // Read primary shop ID
    const entityRaw = await (api.query as any).entityRegistry.entities(ENTITY_ID);
    assert(!(entityRaw as any).isNone, `实体 ${ENTITY_ID} 不存在 / Entity ${ENTITY_ID} not found`);
    const entityJson = codecToJson<Record<string, unknown>>((entityRaw as any).unwrap());
    const primaryShopId = coerceNumber(readObjectField(entityJson, 'primaryShopId', 'primary_shop_id'));
    assert(primaryShopId != null && primaryShopId > 0,
      `无法读取实体 ${ENTITY_ID} 的主店铺 ID / Cannot read primaryShopId`);
    ok(`实体 ${ENTITY_ID} 主店铺 ID / Primary Shop ID: ${primaryShopId}`);

    // 动态扫描账户池，找出未注册账户
    // Dynamically scan the pool for unregistered accounts
    const BASE_NEEDED    = NUM_L1 + NUM_L1 * L2_PER_L1;
    const SURPLUS_NEEDED = SURPLUS_ROUNDS * (SURPLUS_L1_PER_ROUND + SURPLUS_L2_PER_ROUND);
    const needed = BASE_NEEDED + SURPLUS_NEEDED;
    info(`正在扫描账户池（共 ${subPool.length} 个），寻找 ${needed} 个未注册账户（达标 ${BASE_NEEDED} + 高余 ${SURPLUS_NEEDED}）...`);
    info(`Scanning pool (${subPool.length} accounts) for ${needed} unregistered accounts (base ${BASE_NEEDED} + surplus ${SURPLUS_NEEDED})...`);

    const unregisteredPoolIndices: number[] = [];
    for (let pi = 0; pi < subPool.length && unregisteredPoolIndices.length < needed; pi++) {
      const addr = buildKeypair(subPool[pi], keyring).address;
      const memberRaw = await (api.query as any).entityMember.entityMembers(ENTITY_ID, addr);
      if ((memberRaw as any).isNone) {
        unregisteredPoolIndices.push(pi);
      }
    }

    assert(
      unregisteredPoolIndices.length >= needed,
      `未注册账户数量不足（需要 ${needed} 个，可用 ${unregisteredPoolIndices.length} 个）/ ` +
      `Not enough unregistered accounts (need ${needed}, found ${unregisteredPoolIndices.length})`
    );

    ok(`找到 ${unregisteredPoolIndices.length} 个未注册账户，取前 ${needed} 个使用。`);
    ok(`Found ${unregisteredPoolIndices.length} unregistered accounts; using first ${needed}.`);

    // 达标阶段账户分配
    // Base phase account assignment
    const TOTAL_INDIRECT = NUM_L1 * L2_PER_L1;
    // L1[i] = unregisteredPoolIndices[i]，i in [0, NUM_L1)
    const l1Accounts = unregisteredPoolIndices.slice(0, NUM_L1).map(pi =>
      buildKeypair(subPool[pi], keyring)
    );
    // L2[i] = unregisteredPoolIndices[NUM_L1 + i]，i in [0, TOTAL_INDIRECT)
    // l2Accounts[i] belongs to l1Accounts[Math.floor(i / L2_PER_L1)]
    const l2Accounts = unregisteredPoolIndices.slice(NUM_L1, BASE_NEEDED).map(pi =>
      buildKeypair(subPool[pi], keyring)
    );

    // 高余阶段账户分配（预分配，运行时顺序消耗）
    // Surplus phase account pool (pre-allocated, consumed in order at runtime)
    // surplusPool[r][0..SURPLUS_L1_PER_ROUND-1] = L1s for round r
    // surplusPool[r][SURPLUS_L1_PER_ROUND..SURPLUS_L1_PER_ROUND+SURPLUS_L2_PER_ROUND-1] = L2s for round r
    const surplusL1Accounts: KeyringPair[][] = [];
    const surplusL2Accounts: KeyringPair[][] = [];
    const surplusL1PoolIndices: number[][] = [];
    const surplusL2PoolIndices: number[][] = [];

    for (let r = 0; r < SURPLUS_ROUNDS; r++) {
      const baseOffset = BASE_NEEDED + r * (SURPLUS_L1_PER_ROUND + SURPLUS_L2_PER_ROUND);
      const roundL1Indices = unregisteredPoolIndices.slice(baseOffset, baseOffset + SURPLUS_L1_PER_ROUND);
      const roundL2Indices = unregisteredPoolIndices.slice(baseOffset + SURPLUS_L1_PER_ROUND, baseOffset + SURPLUS_L1_PER_ROUND + SURPLUS_L2_PER_ROUND);
      surplusL1Accounts.push(roundL1Indices.map(pi => buildKeypair(subPool[pi], keyring)));
      surplusL2Accounts.push(roundL2Indices.map(pi => buildKeypair(subPool[pi], keyring)));
      surplusL1PoolIndices.push(roundL1Indices);
      surplusL2PoolIndices.push(roundL2Indices);
    }

    // 打印 L1/L2 账户分配（达标阶段）
    // Print base phase account assignments
    for (let i = 0; i < NUM_L1; i++) {
      kv(`[达标] 直推 L1[${i + 1}]（pool index ${unregisteredPoolIndices[i]}）`, l1Accounts[i].address);
    }
    for (let i = 0; i < TOTAL_INDIRECT; i++) {
      const l1Owner = Math.floor(i / L2_PER_L1) + 1;
      kv(`[达标] 间接下级 L2[${i + 1}]（属于 L1[${l1Owner}]，pool index ${unregisteredPoolIndices[NUM_L1 + i]}）`,
        l2Accounts[i].address);
    }

    // 打印高余阶段账户分配
    // Print surplus phase account assignments
    for (let r = 0; r < SURPLUS_ROUNDS; r++) {
      let l2GlobalIdx = 0;
      for (let li = 0; li < SURPLUS_L1_PER_ROUND; li++) {
        kv(`[高余轮${r + 1}] 直推 L1[${li + 1}]（pool index ${surplusL1PoolIndices[r][li]}）`,
          surplusL1Accounts[r][li].address);
      }
      for (let li = 0; li < SURPLUS_L1_PER_ROUND; li++) {
        const l2Count = SURPLUS_L2_DIST[li] ?? 0;
        for (let lj = 0; lj < l2Count; lj++) {
          kv(`[高余轮${r + 1}] 间接 L2[${l2GlobalIdx + 1}]（属于本轮 L1[${li + 1}]，pool index ${surplusL2PoolIndices[r][l2GlobalIdx]}）`,
            surplusL2Accounts[r][l2GlobalIdx].address);
          l2GlobalIdx++;
        }
      }
    }

    // 下级账户补充 1,000,000 NEX，主账户补充 600 NEX（Plan7 费用）
    // Sub-accounts funded to 1,000,000 NEX; main account to 600 NEX (Plan7 cost)
    info('正在补充各账户余额（达标阶段）...');
    await ensureBalance(api, faucet, mainAccount, `主账户L0[main:${MAIN_INDEX}]`, 600);
    for (let i = 0; i < NUM_L1; i++) {
      await ensureBalance(api, faucet, l1Accounts[i], `L1[${i + 1}][pool:${unregisteredPoolIndices[i]}]`, 1_000_000);
    }
    for (let i = 0; i < TOTAL_INDIRECT; i++) {
      await ensureBalance(api, faucet, l2Accounts[i],
        `L2[${i + 1}][pool:${unregisteredPoolIndices[NUM_L1 + i]}]`, 1_000_000);
    }

    info('正在补充各账户余额（高余阶段）...');
    for (let r = 0; r < SURPLUS_ROUNDS; r++) {
      for (let li = 0; li < SURPLUS_L1_PER_ROUND; li++) {
        await ensureBalance(api, faucet, surplusL1Accounts[r][li],
          `[高余轮${r + 1}]L1[${li + 1}][pool:${surplusL1PoolIndices[r][li]}]`, 1_000_000);
      }
      for (let li = 0; li < SURPLUS_L2_PER_ROUND; li++) {
        await ensureBalance(api, faucet, surplusL2Accounts[r][li],
          `[高余轮${r + 1}]L2[${li + 1}][pool:${surplusL2PoolIndices[r][li]}]`, 1_000_000);
      }
    }
    ok('所有账户余额已就绪 / All accounts funded');

    await waitForEnter('余额充足，继续（步骤 3：读取主账户初始状态）');

    /* ── 3. 读取主账户初始状态 ── */
    logStep(3, '读取主账户当前会员状态 | Read main account current member state');

    const mainBefore = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
    printMemberSnapshot('主账户初始状态 / Main Initial State', mainBefore);
    info(`升级目标 / Target: direct≥5, indirect≥${TOTAL_INDIRECT}, threshold≥500,000,000`);

    if (!mainBefore.exists) {
      warn('主账户尚未注册（PURCHASE_REQUIRED 策略），购买 Plan7 时将自动注册。');
      warn('Main account not yet registered; will auto-register on Plan7 purchase.');
    }

    await waitForEnter('初始状态确认，继续（步骤 4：主账户购买 7 星套餐）');

    /* ── 4. 主账户购买套餐直到消费阈值满足 ── */
    logStep(4, '主账户累积消费至 500 USDT 阈值（循环购买）| Main: Accumulate spend to 500 USDT threshold');

    // 使用 upgradeEligibleSpent 判断，与 calculate_custom_level_full 逻辑一致
    // Use upgradeEligibleSpent for idempotency check, matching calculate_custom_level_full
    const SPEND_THRESHOLD = 500_000_000n;
    const thresholdAlreadyMet = mainBefore.upgradeEligibleSpent >= SPEND_THRESHOLD;
    let purchaseCount = 0;

    if (thresholdAlreadyMet) {
      warn(`主账户升级消费已达 ${mainBefore.upgradeEligibleSpent}（≥500,000,000），跳过购买。`);
      warn('Main account upgradeEligibleSpent already ≥500 USDT, skipping purchases.');
    } else {
      info(`当前升级消费 / Current upgradeEligibleSpent: ${mainBefore.upgradeEligibleSpent}，目标 / target: ${SPEND_THRESHOLD}`);
      info(`将循环购买 productId=${PLAN7_PRODUCT_ID} 直到消费满足阈值 / Buying productId=${PLAN7_PRODUCT_ID} until threshold is met`);

      let currentEligible = mainBefore.upgradeEligibleSpent;
      while (currentEligible < SPEND_THRESHOLD) {
        // 每次购买前确保主账户有足够余额
        await ensureBalance(api, faucet, mainAccount, `主账户L0[循环购买 #${purchaseCount + 1}]`, 600);

        info(`第 ${purchaseCount + 1} 次购买（productId=${PLAN7_PRODUCT_ID}），当前升级消费 ${currentEligible}...`);
        const planTx = (api.tx as any).entityTransaction.placeOrder(
          PLAN7_PRODUCT_ID, 1,
          null, null, null, null,
          null,   // 主账户购买，无推荐人 / no referrer for main account
          null, null,
        );
        const planReceipt = await submitTx(api, planTx, mainAccount, `main-buy-plan-${purchaseCount + 1}`);
        assertTxSuccess(planReceipt, `主账户第 ${purchaseCount + 1} 次购买失败 / Main purchase #${purchaseCount + 1} failed`);
        purchaseCount++;
        ok(`第 ${purchaseCount} 次购买成功 — 订单 ID: ${extractOrderId(planReceipt) ?? 'N/A'}，txHash: ${planReceipt.txHash}`);

        const snap = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
        currentEligible = snap.upgradeEligibleSpent;
        info(`购买后升级消费 / upgradeEligibleSpent after purchase: ${currentEligible}`);
      }
      ok(`✅ 消费阈值已满足：upgradeEligibleSpent=${currentEligible}（共购买 ${purchaseCount} 次）`);
    }

    const mainAfterPurchase = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
    printMemberSnapshot('主账户购买后状态 / Main After Purchases', mainAfterPurchase);
    info(`当前等级 Level ${mainAfterPurchase.effectiveLevelId}（目标 7），直推 ${mainAfterPurchase.directReferrals}/5，间接 ${mainAfterPurchase.indirectReferrals}/${TOTAL_INDIRECT}`);

    if (mainAfterPurchase.effectiveLevelId >= 7) {
      ok('主账户消费后已达到 7 星（稀少情况：已有足够直推和间接），将直接进入高余轮次。');
      ok('Main already at level 7 after purchases — skipping base referral phase, proceeding to surplus rounds.');
    }

    await waitForEnter(
      `主账户消费阈值已满足，当前等级 Level ${mainAfterPurchase.effectiveLevelId}。` +
      `继续（步骤 5：逐一激活 ${NUM_L1} 个直推 L1，每个 L1 再带 ${L2_PER_L1} 个 L2）`
    );

    /* ── 5. 逐一激活 L1[1..N]，每个 L1 再激活其对应的 L2 ── */
    logStep(5, `逐一激活 ${NUM_L1} 个直推 L1，每个 L1 带 ${L2_PER_L1} 个 L2 | Activate L1s and their L2s`);

    info(`7 星升级条件（来自节点）/ 7-star conditions (from node):`);
    info(`  消费阈值 threshold ≥ 500,000,000（已满足）`);
    info(`  直推 direct ≥ 5（激活 ${NUM_L1} 个 L1 后满足）`);
    info(`  间接 indirect ≥ 10（每个 L1 带 ${L2_PER_L1} 个 L2，共 ${TOTAL_INDIRECT} 个后满足）`);
    info(`  注意：等级 L5/L6/L7 是连续检查，L5 需 indirect≥5，L6 需 direct≥3+indirect≥8，均会被同步满足。`);

    let currentMain = mainAfterPurchase;

    for (let l1Idx = 0; l1Idx < NUM_L1; l1Idx++) {
      const l1Account = l1Accounts[l1Idx];
      const l1PoolIdx = unregisteredPoolIndices[l1Idx];

      subHeader(`L1[${l1Idx + 1}/${NUM_L1}] — pool index ${l1PoolIdx} — ${l1Account.address}`);

      info(`主账户当前：Level ${currentMain.effectiveLevelId}，直推 ${currentMain.directReferrals}/${NUM_L1}，间接 ${currentMain.indirectReferrals}/${TOTAL_INDIRECT}`);

      await waitForEnter(
        `准备激活 L1[${l1Idx + 1}]（pool index ${l1PoolIdx}），推荐人 = 主账户。按 Enter 执行。`
      );

      // L1 购买 Plan1，referrer = 主账户，auto_register 自动注册 L1
      // L1 buys Plan1 with main as referrer; auto_register handles registration
      await buyPlan(api, l1Account, PLAN1_PRODUCT_ID, mainAccount,
        `L1[${l1Idx + 1}][pool:${l1PoolIdx}]`, ENTITY_ID);

      currentMain = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
      info(`L1[${l1Idx + 1}] 激活后 — 主账户直推 ${currentMain.directReferrals}/${NUM_L1}，间接 ${currentMain.indirectReferrals}/${TOTAL_INDIRECT}`);

      // 每个 L1 激活后立即激活其对应的 L2[0..L2_PER_L1-1]
      // After each L1, activate its corresponding L2s
      for (let j = 0; j < L2_PER_L1; j++) {
        const l2GlobalIdx = l1Idx * L2_PER_L1 + j;
        const l2Account   = l2Accounts[l2GlobalIdx];
        const l2PoolIdx   = unregisteredPoolIndices[NUM_L1 + l2GlobalIdx];

        info(`  激活 L2[${l2GlobalIdx + 1}]（属于 L1[${l1Idx + 1}]，pool index ${l2PoolIdx}）...`);

        await waitForEnter(
          `准备激活 L2[${l2GlobalIdx + 1}]（pool index ${l2PoolIdx}），推荐人 = L1[${l1Idx + 1}]。按 Enter 执行。`
        );

        // L2 购买 Plan1，referrer = 其对应的 L1
        // L2 buys Plan1 with its L1 as referrer
        await buyPlan(api, l2Account, PLAN1_PRODUCT_ID, l1Account,
          `L2[${l2GlobalIdx + 1}][pool:${l2PoolIdx}]`, ENTITY_ID);

        currentMain = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
        info(`  L2[${l2GlobalIdx + 1}] 激活后 — 主账户：Level ${currentMain.effectiveLevelId}，直推 ${currentMain.directReferrals}/${NUM_L1}，间接 ${currentMain.indirectReferrals}/${TOTAL_INDIRECT}`);
      }

      printMemberSnapshot(
        `主账户状态（L1[${l1Idx + 1}] 及其 L2 全部激活后）/ Main after L1[${l1Idx + 1}] group`,
        currentMain,
      );

      if (currentMain.effectiveLevelId >= 7) {
        ok(`🎉 主账户已成功升级至 7 星（Level ${currentMain.effectiveLevelId}）！`);
        ok(`🎉 Main account upgraded to 7-star!`);
        info('条件全部满足：threshold≥500,000,000 + direct≥5 + indirect≥10。');
        info('All conditions met: threshold≥500 USDT + direct≥5 + indirect≥10.');
        break;
      } else {
        info(`距 7 星还差：直推 ${Math.max(0, NUM_L1 - currentMain.directReferrals)} 个，间接 ${Math.max(0, TOTAL_INDIRECT - currentMain.indirectReferrals)} 个。`);
        if (l1Idx + 1 < NUM_L1) {
          await waitForEnter(
            `L1[${l1Idx + 1}] 组完成。按 Enter 继续激活 L1[${l1Idx + 2}] 组。`
          );
        }
      }
    }

    /* ── 6. 高余轮次：每轮 +3 直推 +4 间接，验证 7 星保持 ── */
    /* ── Step 6: Surplus rounds — +3 direct +4 indirect per round, assert level stays 7-star ── */
    if (SURPLUS_ROUNDS > 0) {
      logStep(6, `高余轮次（共 ${SURPLUS_ROUNDS} 轮，每轮 +${SURPLUS_L1_PER_ROUND} 直推 +${SURPLUS_L2_PER_ROUND} 间接）| Surplus Rounds`);
      info(`高余设计：7 星达标后继续增量，验证等级不退化。`);
      info(`Surplus design: keep adding beyond 7-star threshold to verify stability.`);
      info(`每轮 L2 分配 / L2 dist per round: L1[1]×${SURPLUS_L2_DIST[0]}, L1[2]×${SURPLUS_L2_DIST[1]}, L1[3]×${SURPLUS_L2_DIST[2]}`);

      for (let r = 0; r < SURPLUS_ROUNDS; r++) {
        subHeader(`高余轮 ${r + 1}/${SURPLUS_ROUNDS} | Surplus Round ${r + 1}`);

        const snapBeforeRound = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
        info(`轮前状态：Level ${snapBeforeRound.effectiveLevelId}，直推 ${snapBeforeRound.directReferrals}，间接 ${snapBeforeRound.indirectReferrals}`);

        const startAction = await waitForEnterOrStop(
          `准备执行高余轮 ${r + 1}（+${SURPLUS_L1_PER_ROUND} 直推 +${SURPLUS_L2_PER_ROUND} 间接）。`
        );
        if (startAction === 'stop') {
          warn(`用户选择终止高余轮次，已完成 ${r}/${SURPLUS_ROUNDS} 轮。`);
          warn(`Surplus rounds stopped by user after ${r}/${SURPLUS_ROUNDS} rounds.`);
          break;
        }

        // 逐一激活本轮 L1，每个 L1 带对应数量的 L2
        // Activate each L1 in this round, then activate its L2s
        let l2GlobalIdx = 0;
        for (let li = 0; li < SURPLUS_L1_PER_ROUND; li++) {
          const sL1 = surplusL1Accounts[r][li];
          const sL1PoolIdx = surplusL1PoolIndices[r][li];
          const l2Count = SURPLUS_L2_DIST[li] ?? 0;

          info(`  [轮${r + 1}] 激活 L1[${li + 1}]（pool index ${sL1PoolIdx}）...`);
          await buyPlan(api, sL1, PLAN1_PRODUCT_ID, mainAccount,
            `[高余轮${r + 1}]L1[${li + 1}][pool:${sL1PoolIdx}]`, ENTITY_ID);

          currentMain = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
          info(`  L1[${li + 1}] 激活后 — 主账户直推 ${currentMain.directReferrals}，间接 ${currentMain.indirectReferrals}`);

          for (let lj = 0; lj < l2Count; lj++) {
            const sL2 = surplusL2Accounts[r][l2GlobalIdx];
            const sL2PoolIdx = surplusL2PoolIndices[r][l2GlobalIdx];

            info(`    [轮${r + 1}] 激活 L2[${l2GlobalIdx + 1}]（属于本轮 L1[${li + 1}]，pool index ${sL2PoolIdx}）...`);
            await buyPlan(api, sL2, PLAN1_PRODUCT_ID, sL1,
              `[高余轮${r + 1}]L2[${l2GlobalIdx + 1}][pool:${sL2PoolIdx}]`, ENTITY_ID);

            currentMain = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
            info(`    L2[${l2GlobalIdx + 1}] 激活后 — 主账户：Level ${currentMain.effectiveLevelId}，直推 ${currentMain.directReferrals}，间接 ${currentMain.indirectReferrals}`);
            l2GlobalIdx++;
          }
        }

        printMemberSnapshot(`主账户状态（高余轮 ${r + 1} 完成后）/ Main after surplus round ${r + 1}`, currentMain);

        // 高余轮次断言：等级仍应为 7 星
        // Surplus round assertion: level must remain 7-star
        if (currentMain.effectiveLevelId >= 7) {
          ok(`✅ [高余轮 ${r + 1}] 等级稳定在 7 星（Level ${currentMain.effectiveLevelId}），直推 +${SURPLUS_L1_PER_ROUND}，间接 +${SURPLUS_L2_PER_ROUND}`);
          ok(`✅ [Surplus round ${r + 1}] Level still 7-star after +${SURPLUS_L1_PER_ROUND} direct +${SURPLUS_L2_PER_ROUND} indirect.`);
        } else {
          warn(`❌ [高余轮 ${r + 1}] 等级退化至 ${currentMain.effectiveLevelId}，期望仍为 7 星！`);
          warn(`❌ [Surplus round ${r + 1}] Level degraded to ${currentMain.effectiveLevelId}, expected 7-star!`);
        }

        if (r + 1 < SURPLUS_ROUNDS) {
          const nextAction = await waitForEnterOrStop(`高余轮 ${r + 1} 完成。继续高余轮 ${r + 2}？`);
          if (nextAction === 'stop') {
            warn(`用户选择终止高余轮次，已完成 ${r + 1}/${SURPLUS_ROUNDS} 轮。`);
            warn(`Surplus rounds stopped by user after ${r + 1}/${SURPLUS_ROUNDS} rounds.`);
            break;
          }
        }
      }
    } else {
      info('SURPLUS_ROUNDS=0，跳过高余轮次。/ SURPLUS_ROUNDS=0, skipping surplus rounds.');
    }

    /* ── 7. 最终状态汇总 ── */
    logStep(7, '最终状态汇总与结论 | Final State Summary & Conclusion');

    await waitForEnter('所有操作完毕，按 Enter 查看最终汇总。');

    const mainFinal = await readMemberSnapshot(api, ENTITY_ID, mainAccount.address);
    printMemberSnapshot('主账户最终状态 / Main Final State', mainFinal);
    kv('主账户当前余额 / Main Balance', formatNex(await readFreeBalance(api, mainAccount.address)));

    console.log();
    subHeader('最终验证结论 / Final Assertions');

    if (mainFinal.effectiveLevelId >= 7) {
      ok(`✅ 主账户成功升级至 7 星（effectiveLevelId = ${mainFinal.effectiveLevelId}）`);
      ok(`✅ Main account upgraded to 7-star (effectiveLevelId = ${mainFinal.effectiveLevelId}).`);
    } else {
      warn(`❌ 主账户等级仍为 ${mainFinal.effectiveLevelId}，未达到 7 星。`);
      warn(`❌ Main account level is ${mainFinal.effectiveLevelId}, not yet level 7.`);
      warn(`   直推数：${mainFinal.directReferrals}（需要 ≥${NUM_L1}）`);
      warn(`   间接下级：${mainFinal.indirectReferrals}（需要 ≥${TOTAL_INDIRECT}）`);
      warn(`   升级消费：${mainFinal.upgradeEligibleSpent}（需要 ≥500,000,000）`);
    }

    const expectedOrders = mainBefore.orderCount + (thresholdAlreadyMet ? 0 : purchaseCount);
    if (mainFinal.orderCount === expectedOrders) {
      ok(`✅ 主账户套餐购买次数符合预期（共 ${mainFinal.orderCount} 单，本次新增 ${purchaseCount} 单）`);
    } else {
      warn(`⚠  主账户总订单数 ${mainFinal.orderCount}（预期 ${expectedOrders}，本次应新增 ${purchaseCount} 单），请确认。`);
    }

    ok(`✅ 主账户最终直推数：${mainFinal.directReferrals}（目标 ≥${NUM_L1}）`);
    ok(`✅ 主账户最终间接下级数：${mainFinal.indirectReferrals}（目标 ≥${TOTAL_INDIRECT}）`);

    header('脚本执行完成 | Script Completed');
    info(`主账户（${mainAccount.address}）通过推荐链完成 7 星升级，自身只购买了一次套餐。`);
    info(`Main account upgraded to 7-star via referral chain with only one self-purchase.`);
    if (SURPLUS_ROUNDS > 0) {
      info(`高余验证：共执行 ${SURPLUS_ROUNDS} 轮，每轮 +${SURPLUS_L1_PER_ROUND} 直推 +${SURPLUS_L2_PER_ROUND} 间接，最终直推 ${mainFinal.directReferrals}，间接 ${mainFinal.indirectReferrals}。`);
      info(`Surplus verification: ${SURPLUS_ROUNDS} rounds × (+${SURPLUS_L1_PER_ROUND} direct, +${SURPLUS_L2_PER_ROUND} indirect). Final: direct=${mainFinal.directReferrals}, indirect=${mainFinal.indirectReferrals}.`);
    }

  } finally {
    await disconnectApi(api);
  }
}

main().catch((error: unknown) => {
  console.error('\n[ERROR]', error instanceof Error ? error.message : String(error));
  process.exit(1);
});
