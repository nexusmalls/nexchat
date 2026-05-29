#!/usr/bin/env tsx
/**
 * 元宇宙之门实体初始化自动化脚本 / Metaverse Door Entity Setup Automation Script
 *
 * 创建一个包含店铺、7 级会员体系（Plan1~Plan7）、7 个套餐商品
 * （5~500 USDT）以及 3 个佣金插件（13 级多级分销、单线 20/30~40/60、
 * 奖池分配）的实体，贴合 Metaverse Door 分配模型。
 *
 * 数据来源 / Data source: 元宇宙模式.docx + Metaverse Door(5).pdf
 *
 * 用法 / Usage:
 *   node --import tsx mytests/setup-entity-full.ts
 *
 * 环境变量 / Environment variables:
 *   WS_URL           — WebSocket 端点（默认: wss://nexuscom.duckdns.org:9948）
 *   ENTITY_NAME      — 实体名称（默认: "MetaverseDoor-{timestamp}"）
 *   SHOP_FUND_NEX    — 店铺经营账户注资的 NEX 数量（默认: 10000）
 *   TARGET_ENTITY_ID — 要检查或复用的实体 ID（默认: 100000）
 *   ACCOUNT_FILE     — 账户 JSON 文件路径；当目标 entity 已存在时，按 owner 地址匹配 signer
 *   PRODUCT_RECOVERY_FUND_NEX — 商品押金恢复补款金额；不设置时自动按 signer 可用余额计算
 *   PRODUCT_RECOVERY_RESERVE_NEX — 自动恢复时为 signer 预留的余额（默认: 1000）
 *   SKIP_UNTIL       — 从指定步骤继续执行（例如 "5" 表示从步骤 5 开始）
 */

process.env.WS_URL ??= 'wss://nexuscom.duckdns.org:9948';

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { stringToU8a } from '@polkadot/util';
import { encodeAddress } from '@polkadot/util-crypto';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import type { KeyringPair } from '@polkadot/keyring/types';
import { connectApi, disconnectApi, submitTx } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { assertTxSuccess } from '../framework/assert.js';
import { formatNex, nex } from '../framework/units.js';
import { codecToJson, codecToHuman, readObjectField, coerceNumber } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';
import {
  readEntityIds,
  readEntity,
  resolvePrimaryShopId,
  waitForNewEntityId,
  readNextProductId,
} from '../suites/helpers.js';

/* ------------------------------------------------------------------ */
/*  Configuration                                                      */
/* ------------------------------------------------------------------ */

const MNEMONIC = 'fabric smile father unique elbow buffalo until emerge novel orient rally basket';
/*  const MNEMONIC = 'please execute flee demise kite elegant tiger must police hunt cat acoustic';*/
const ACCOUNT_FILE = process.env.ACCOUNT_FILE
  ?? '/home/xiaodong/桌面/nexus/scripts/e2e/mytests/test-accounts-2026-04-14T03-13-20-529Z（用于注册entity的账户）.json';
const ENTITY_NAME = process.env.ENTITY_NAME ?? `MetaverseDoor-${Date.now()}`;
const SHOP_FUND = nex(Number(process.env.SHOP_FUND_NEX ?? '100000'));
const PRODUCT_RECOVERY_FUND = process.env.PRODUCT_RECOVERY_FUND_NEX
  ? nex(Number(process.env.PRODUCT_RECOVERY_FUND_NEX))
  : null;
const PRODUCT_RECOVERY_RESERVE = nex(Number(process.env.PRODUCT_RECOVERY_RESERVE_NEX ?? '1000'));
const TARGET_ENTITY_ID = Number(process.env.TARGET_ENTITY_ID ?? '100000');
const SKIP_UNTIL = Number(process.env.SKIP_UNTIL ?? '0');

/* ------------------------------------------------------------------ */
/*  日志输出 / Logging                                                 */
/* ------------------------------------------------------------------ */

/**
 * 输出统一格式的中英双语日志。
 */
function log(tag: string, msg: string): void {
  const ts = new Date().toISOString().slice(11, 23);
  console.log(`[${ts}] [${tag}] ${msg}`);
}

/**
 * 输出步骤标题，便于区分大阶段执行进度。
 */
function logStep(step: number, title: string): void {
  console.log(`\n${'='.repeat(70)}`);
  console.log(`  STEP ${step}: ${title}`);
  console.log(`${'='.repeat(70)}\n`);
}

/**
 * 将字符串按链上 bytes 参数原样透传。
 */
function bytes(value: string): string {
  return value;
}

/**
 * 推导 entity treasury 地址。
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
/*  Member Level Definitions (Metaverse Door 7 套餐)                    */
/* ------------------------------------------------------------------ */

// threshold uses USDT precision (10^6), original amounts ÷100 for testing
// 5 USDT = 5_000_000, 10 USDT = 10_000_000, etc.
// discount_rate / commission_bonus = 0 (Metaverse Door has no discount/bonus concept)
// min_indirect_referrals: Plan1~4=0, Plan5=5, Plan6=8, Plan7=10
const MEMBER_LEVELS = [
  { name: 'Plan1', threshold: 5_000_000,   discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 0  },
  { name: 'Plan2', threshold: 10_000_000,  discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 0  },
  { name: 'Plan3', threshold: 20_000_000,  discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 0  },
  { name: 'Plan4', threshold: 30_000_000,  discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 0  },
  { name: 'Plan5', threshold: 50_000_000,  discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 5  },
  { name: 'Plan6', threshold: 150_000_000, discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 8  },
  { name: 'Plan7', threshold: 500_000_000, discount_rate: 0, commission_bonus: 0, min_indirect_referrals: 10 },
];

/* ------------------------------------------------------------------ */
/*  Plan Products (7 套餐商品 — 与会员等级一一对应)                        */
/* ------------------------------------------------------------------ */

// usdt_price matches level threshold (USDT precision 10^6)
// visibility = MembersOnly (only registered members can purchase)
// stock = 0 (unlimited), min/max_order_quantity = 1 (one per order)
const PLAN_PRODUCTS = [
  { name: 'Plan1-5USDT',   usdt_price: 5_000_000,   sort_weight: 1 },
  { name: 'Plan2-10USDT',  usdt_price: 10_000_000,  sort_weight: 2 },
  { name: 'Plan3-20USDT',  usdt_price: 20_000_000,  sort_weight: 3 },
  { name: 'Plan4-30USDT',  usdt_price: 30_000_000,  sort_weight: 4 },
  { name: 'Plan5-50USDT',  usdt_price: 50_000_000,  sort_weight: 5 },
  { name: 'Plan6-150USDT', usdt_price: 150_000_000, sort_weight: 6 },
  { name: 'Plan7-500USDT', usdt_price: 500_000_000, sort_weight: 7 },
];

/* ------------------------------------------------------------------ */
/*  Multi-Level Tier Definitions (动态奖励 — 太阳线 13 级)              */
/* ------------------------------------------------------------------ */

// 所有 rate 均基于订单金额的 bps (万分比)
// 各级 rate 保持文档原始比例，总和=5000，超出 max_total_rate 的部分自动截断
// L1=15%, L2=5%, L3~L7=2%×5, L8=7%, L9~L12=2%×4, L13=5%
const ML_TIERS = [
  { rate: 1500, required_directs: 0, required_team_size: 0, required_spent: 0 }, // L1  15%
  { rate: 500,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L2  5%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L3  2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L4  2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L5  2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L6  2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L7  2%
  { rate: 700,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L8  7%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L9  2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L10 2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L11 2%
  { rate: 200,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L12 2%
  { rate: 500,  required_directs: 0, required_team_size: 0, required_spent: 0 }, // L13 5%
];
// max_total_rate = ML 的内部截断上限，必须 ≤ multi_level_cap (5000)
// 超出部分自动归沉淀池
const ML_MAX_RATE = 5000;

/* ------------------------------------------------------------------ */
/*  Single-Line Configuration (静态奖励 — 全网一条线公排)                 */
/* ------------------------------------------------------------------ */

// upline_rate / downline_rate = per-level rate in bps (of order_amount)
// SL 预算上限 = 4400 bps (44% of order)
// per-level 44 bps × 40 up + 44 bps × 60 down = 1760 + 2640 = 4400
// 实际分配受 cap (4400) 和 remaining 限制，不会超限
// Chain validation: rate <= 1000, rate×levels + rate×levels <= MaxTotalRateBps
const SL_UPLINE_RATE = 44;          // 0.44% per upline level
const SL_DOWNLINE_RATE = 44;        // 0.44% per downline level
const SL_BASE_UPLINE_LEVELS = 20;   // Plan1: 上20层
const SL_BASE_DOWNLINE_LEVELS = 30; // Plan1: 下30层
const SL_LEVEL_INCREMENT_THRESHOLD = 0;
const SL_MAX_UPLINE_LEVELS = 40;    // Plan5~7: 上40层
const SL_MAX_DOWNLINE_LEVELS = 60;  // Plan5~7: 下60层
const SL_ENABLED = true;

/* ------------------------------------------------------------------ */
/*  Single-Line Level Overrides (按套餐等级配置不同上/下线层数)           */
/* ------------------------------------------------------------------ */

// [level_id, upline_levels, downline_levels]
// level_id 1-based (对应 entityMember 等级 ID)
// 数据来源: 元宇宙模式.docx「提现与复投机制」表
const SL_LEVEL_OVERRIDES: [number, number, number][] = [
  [1, 20, 30],   // Plan1: 上20层 / 下30层（共50层）
  [2, 24, 36],   // Plan2: 上24层 / 下36层（共60层）
  [3, 28, 42],   // Plan3: 上28层 / 下42层（共70层）
  [4, 32, 48],   // Plan4: 上32层 / 下48层（共80层）
  [5, 40, 60],   // Plan5: 上40层 / 下60层（共100层）
  [6, 40, 60],   // Plan6: 上40层 / 下60层（共100层）
  [7, 40, 60],   // Plan7: 上40层 / 下60层（共100层）
];

/* ------------------------------------------------------------------ */
/*  Pool-Reward Configuration (沉淀资金分红)                             */
/* ------------------------------------------------------------------ */

// level_rules: [level_id, LevelClaimRule]
// LevelClaimRule = { base_cap_percent: u16, cap_behavior: CapBehavior }
// CapBehavior = "Fixed" | { UnlockByTeam: { direct_per_unlock, team_per_unlock, unlock_percent } }
// Level index is 0-based: Plan5=level_id 4, Plan6=5, Plan7=6
// Plan1~4 (level 0~3): no pool reward
const PR_LEVEL_RULES: [number, { base_cap_percent: number; cap_behavior: string }][] = [
  [4, { base_cap_percent: 2000, cap_behavior: 'Fixed' }],  // Level 5 (Plan5): 20%
  [5, { base_cap_percent: 3000, cap_behavior: 'Fixed' }],  // Level 6 (Plan6): 30%
  [6, { base_cap_percent: 5000, cap_behavior: 'Fixed' }],  // Level 7 (Plan7): 50%
];
const PR_ROUND_DURATION = 14_400; // ~24h @ 6s/block

/* ------------------------------------------------------------------ */
/*  Commission Modes Bitmask                                           */
/* ------------------------------------------------------------------ */

const COMMISSION_MODES =
  0b0000_0010 +       // MULTI_LEVEL
  0b1000_0000 +       // SINGLE_LINE_UPLINE
  0b1_0000_0000 +     // SINGLE_LINE_DOWNLINE
  0b10_0000_0000 +    // POOL_REWARD
  0b100_0000_0000;    // OWNER_REWARD
// = 1922 = 0x782

/* ------------------------------------------------------------------ */
/*  Creator Reward (创建人收益 — 从佣金池优先扣除)                       */
/* ------------------------------------------------------------------ */

// owner_reward_rate 基于佣金池 (remaining) 的 bps，不是订单金额
// 500 bps = 佣金池的 5% = 订单金额的 9900×500/10000 = 495 bps (4.95%)
const OWNER_REWARD_RATE = 500;

/* ------------------------------------------------------------------ */
/*  Withdrawal Configuration (按等级分层提现/复投)                        */
/* ------------------------------------------------------------------ */

// WithdrawalMode::LevelBased — 按会员等级自动决定提现/复投比例
// default_tier: 兜底 40/60（未升级会员适用）
// level_overrides: [level_id, { withdrawal_rate, repurchase_rate }]
//   level_id 0-based: Plan1=0 ... Plan7=6
const WD_MODE = 'LevelBased';
const WD_DEFAULT_TIER = { withdrawal_rate: 4000, repurchase_rate: 6000 };
const WD_LEVEL_OVERRIDES: [number, { withdrawal_rate: number; repurchase_rate: number }][] = [
  [1, { withdrawal_rate: 4000, repurchase_rate: 6000 }], // Plan1: 40/60
  [2, { withdrawal_rate: 4000, repurchase_rate: 6000 }], // Plan2: 40/60
  [3, { withdrawal_rate: 4000, repurchase_rate: 6000 }], // Plan3: 40/60
  [4, { withdrawal_rate: 4000, repurchase_rate: 6000 }], // Plan4: 40/60
  [5, { withdrawal_rate: 5000, repurchase_rate: 5000 }], // Plan5: 50/50
  [6, { withdrawal_rate: 6000, repurchase_rate: 4000 }], // Plan6: 60/40
  [7, { withdrawal_rate: 7000, repurchase_rate: 3000 }], // Plan7: 70/30
];
const WD_VOLUNTARY_BONUS_RATE = 0;
const WD_ENABLED = true;

/* ------------------------------------------------------------------ */
/*  辅助函数 / Helpers                                                 */
/* ------------------------------------------------------------------ */

type JsonAccountEntry = {
  index?: number;
  name?: string;
  mnemonic?: string;
  address?: string;
};

type JsonAccountFile = {
  accounts?: JsonAccountEntry[];
};

/**
 * 尝试读取实体；如果链上不存在则返回 null。
 */
async function tryReadEntity(
  api: ApiPromise,
  entityId: number,
): Promise<{ json: Record<string, unknown>; human: Record<string, unknown> } | null> {
  const value = await (api.query as any).entityRegistry.entities(entityId);
  if ((value as any).isNone) return null;
  const entity = (value as any).unwrap();
  return { json: codecToJson(entity), human: codecToHuman(entity) };
}

/**
 * 读取账户 JSON 文件并校验基本结构。
 */
async function loadAccountFile(filePath: string): Promise<JsonAccountFile> {
  const raw = await readFile(filePath, 'utf-8');
  const parsed = JSON.parse(raw) as JsonAccountFile;
  if (!Array.isArray(parsed.accounts)) {
    throw new Error(`Account file ${filePath} is missing a valid accounts array`);
  }
  return parsed;
}

/**
 * 按地址从账户文件中选择 signer，并校验派生地址一致。
 */
function selectSignerForAddress(
  parsed: JsonAccountFile,
  ownerAddress: string,
  keyring: Keyring,
): { signer: KeyringPair; name: string; index: number } {
  const position = parsed.accounts?.findIndex((account) => account.address?.trim() === ownerAddress) ?? -1;
  if (position < 0) {
    throw new Error(`Entity owner ${ownerAddress} was not found in account file ${ACCOUNT_FILE}`);
  }

  const account = parsed.accounts![position];
  const name = account.name?.trim() || `account-${position}`;
  const index = account.index ?? position;
  const mnemonic = account.mnemonic?.trim();
  const expectedAddress = account.address?.trim();

  if (!mnemonic) {
    throw new Error(`Account ${name} [index=${index}] is missing mnemonic in ${ACCOUNT_FILE}`);
  }
  if (!expectedAddress) {
    throw new Error(`Account ${name} [index=${index}] is missing address in ${ACCOUNT_FILE}`);
  }

  const signer = keyring.addFromMnemonic(mnemonic);
  if (signer.address !== expectedAddress) {
    throw new Error(`Account ${name} [index=${index}] address mismatch: derived ${signer.address}, JSON ${expectedAddress}`);
  }

  return { signer, name, index };
}

function isInsufficientShopFundError(error: unknown): boolean {
  return String(error ?? '').includes('entityProduct.InsufficientShopFund');
}

async function readShopFundingState(
  api: ApiPromise,
  shopId: number,
): Promise<{ status: string; json: Record<string, unknown> | null }> {
  const shopRaw = await (api.query as any).entityShop.shops(shopId);
  if ((shopRaw as any).isNone) {
    return { status: 'Missing', json: null };
  }

  const shopJson = codecToJson<Record<string, unknown>>((shopRaw as any).unwrap());
  return {
    status: String(readObjectField(shopJson, 'status') ?? 'Unknown'),
    json: shopJson,
  };
}

async function ensureShopOperatingFundForProducts(
  api: ApiPromise,
  account: KeyringPair,
  entityId: number,
  shopId: number,
): Promise<void> {
  const before = await readShopFundingState(api, shopId);
  log('fund', `Shop ${shopId} status before recovery: ${before.status}`);

  const signerBalance = await readFreeBalance(api, account.address);
  const recoveryAmount = PRODUCT_RECOVERY_FUND
    ?? (signerBalance > PRODUCT_RECOVERY_RESERVE ? signerBalance - PRODUCT_RECOVERY_RESERVE : 0n);
  log('fund', `fundOperating signer: ${account.address}`);
  log('fund', `Signer free balance: ${formatNex(signerBalance)}`);
  log('fund', `Requested top-up amount: ${formatNex(recoveryAmount)}${PRODUCT_RECOVERY_FUND ? ' (fixed)' : ' (auto)'}`);

  if (recoveryAmount <= 0n || signerBalance <= recoveryAmount) {
    throw new Error(
      `Signer ${account.address} balance ${formatNex(signerBalance)} is insufficient for fundOperating(${formatNex(recoveryAmount)}) on shop ${shopId}; reserve=${formatNex(PRODUCT_RECOVERY_RESERVE)}`,
    );
  }

  const ss58 = api.registry.chainSS58 ?? NEXUS_SS58_FORMAT;
  const treasuryAddress = deriveTreasuryAddress(entityId, ss58);
  const treasuryBalance = await readFreeBalance(api, treasuryAddress);
  log('fund', `Treasury ${treasuryAddress} free balance (diagnostic): ${formatNex(treasuryBalance)}`);

  const fundOk = await trySubmitTx(
    api,
    (api.tx as any).entityShop.fundOperating(shopId, recoveryAmount.toString()),
    account,
    `recover shop operating fund (${formatNex(recoveryAmount)})`,
  );
  if (!fundOk.ok) {
    throw new Error(`Failed to fund shop ${shopId} from entity treasury after recovery: ${fundOk.error ?? 'unknown error'}`);
  }

  const after = await readShopFundingState(api, shopId);
  log('fund', `Shop ${shopId} status after recovery: ${after.status}`);
  log('fund', `Recovered shop ${shopId} operating fund with ${formatNex(recoveryAmount)}`);
}

/**
 * 提交交易；如果失败则记录警告并返回 false，而不是直接抛错。
 * 适用于可能已提前配置过的幂等操作。
 */
async function trySubmitTx(
  api: ApiPromise,
  tx: any,
  signer: KeyringPair,
  label: string,
): Promise<{ ok: boolean; error?: string }> {
  try {
    const receipt = await submitTx(api, tx, signer, label);
    if (!receipt.success) {
      log('warn', `${label} failed: ${receipt.error} — skipping`);
      return { ok: false, error: receipt.error };
    }
    return { ok: true };
  } catch (err: any) {
    const message = err.message ?? String(err);
    log('warn', `${label} threw: ${message} — skipping`);
    return { ok: false, error: message };
  }
}

/* ------------------------------------------------------------------ */
/*  主流程 / Main                                                      */
/* ------------------------------------------------------------------ */

/**
 * 主入口：按步骤创建或复用实体，并补齐会员、商品与佣金配置。
 */
async function main(): Promise<void> {
  // ── Step 0: Initialize ──────────────────────────────────────────────
  logStep(0, '初始化 / Initialize');

  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });

  const api = await connectApi();

  try {
    let account: KeyringPair;
    let entityId: number;
    let shopId: number;

    // ── Step 1: Detect or Create Entity ───────────────────────────────
    logStep(1, `Detect or Create Entity (target: ${TARGET_ENTITY_ID})`);

    const existingEntity = await tryReadEntity(api, TARGET_ENTITY_ID);
    if (existingEntity) {
      const ownerAddress = String(readObjectField(existingEntity.json, 'owner') ?? '').trim();
      if (!ownerAddress) {
        throw new Error(`Failed to read owner for existing entity ${TARGET_ENTITY_ID}`);
      }

      const accountFile = await loadAccountFile(ACCOUNT_FILE);
      const selected = selectSignerForAddress(accountFile, ownerAddress, keyring);
      account = selected.signer;

      const balance = await readFreeBalance(api, account.address);
      log('init', `Selected owner signer from account file: ${selected.name} [index=${selected.index}]`);
      log('init', `Entity owner address: ${ownerAddress}`);
      log('init', `Signer address: ${account.address}`);
      log('init', `Free balance: ${formatNex(balance)}`);

      // Entity already exists on chain — reuse it
      entityId = TARGET_ENTITY_ID;
      shopId = resolvePrimaryShopId(existingEntity);
      log('entity', `Entity ${entityId} already exists on chain — reusing`);
      log('entity', `Primary shop ID: ${shopId}`);
    } else {
      account = keyring.addFromMnemonic(MNEMONIC);
      const balance = await readFreeBalance(api, account.address);
      log('init', `Fallback signer address: ${account.address}`);
      log('init', `Free balance: ${formatNex(balance)}`);

      // Entity does not exist — create a new one
      log('entity', `Entity ${TARGET_ENTITY_ID} not found on chain, creating new entity...`);

      const beforeEntityIds = await readEntityIds(api, account.address);
      log('entity', `Existing entity IDs for this account: [${beforeEntityIds.join(', ')}]`);

      const createTx = (api.tx as any).entityRegistry.createEntity(
        bytes(ENTITY_NAME), null, null, null,
      );
      const createReceipt = await submitTx(api, createTx, account, 'create entity');
      assertTxSuccess(createReceipt, 'create entity should succeed');
      log('entity', `createEntity tx succeeded: ${createReceipt.txHash}`);

      const detected = await waitForNewEntityId(api, account.address, beforeEntityIds, 10, 2000);
      if (detected.entityId == null) {
        throw new Error('Failed to detect newly created entity ID after polling');
      }
      entityId = detected.entityId;
      log('entity', `Created entity ID: ${entityId}`);

      const entity = await readEntity(api, entityId);
      shopId = resolvePrimaryShopId(entity);
      log('entity', `Primary shop ID: ${shopId}`);
    }

    if (SKIP_UNTIL <= 2) {
      // ── Step 2: Fund Shop Operating Account ───────────────────────────
      logStep(2, 'Fund Shop Operating Account');

      // Idempotency: check shop status — skip if already funded
      let needsFunding = true;
      const shopRaw = await (api.query as any).entityShop.shops(shopId);
      if (!(shopRaw as any).isNone) {
        const shopJson = codecToJson<Record<string, unknown>>((shopRaw as any).unwrap());
        const status = String(readObjectField(shopJson, 'status') ?? '');
        if (status && status !== 'FundDepleted') {
          log('skip', `Step 2: Shop status=${status} — already funded, skipping`);
          needsFunding = false;
        }
      }

      if (needsFunding) {
        const ok = await trySubmitTx(
          api,
          (api.tx as any).entityShop.fundOperating(shopId, SHOP_FUND.toString()),
          account,
          `fund operating (${formatNex(SHOP_FUND)})`,
        );
        if (ok.ok) {
          log('shop', `Funded shop ${shopId} with ${formatNex(SHOP_FUND)}`);
        }
      }
    } else {
      log('skip', 'Step 2 skipped');
    }

    if (SKIP_UNTIL <= 3) {
      // ── Step 3: Create 7-Level Membership System ──────────────────────
      logStep(3, 'Create 7-Level Membership System');

      // Idempotency: check existing level system
      const levelSysRaw = await (api.query as any).entityMember.entityLevelSystems(entityId);
      let existingLevelCount = 0;
      let needsInit = true;

      if (!(levelSysRaw as any).isNone) {
        needsInit = false;
        const levelSysJson = codecToJson<Record<string, unknown>>((levelSysRaw as any).unwrap());
        const levels = readObjectField(levelSysJson, 'levels') as unknown[] | undefined;
        existingLevelCount = Array.isArray(levels) ? levels.length : 0;
        log('member', `Level system exists with ${existingLevelCount} levels`);
      }

      if (existingLevelCount >= MEMBER_LEVELS.length) {
        log('skip', `Step 3: Already has ${existingLevelCount} levels (need ${MEMBER_LEVELS.length}) — skipping`);
      } else {
        if (needsInit) {
          const initOk = await trySubmitTx(
            api,
            (api.tx as any).entityMember.initLevelSystem(shopId, true, 'AutoUpgrade'),
            account,
            'init level system',
          );
          if (initOk.ok) {
            log('member', 'Level system initialized (use_custom=true, AutoUpgrade)');
          } else {
            log('member', 'Level system may already be initialized — continuing');
          }
        } else {
          log('member', `Level system already initialized — skipping init, adding levels from index ${existingLevelCount}`);
        }

        for (let i = existingLevelCount; i < MEMBER_LEVELS.length; i++) {
          const level = MEMBER_LEVELS[i];
          const levelOk = await trySubmitTx(
            api,
            (api.tx as any).entityMember.addCustomLevel(
              shopId,
              bytes(level.name),
              level.threshold,
              level.discount_rate,
              level.commission_bonus,
              0, // min_direct_referrals
              0, // min_team_size
              level.min_indirect_referrals,
            ),
            account,
            `add level ${i + 1} (${level.name})`,
          );
          if (levelOk.ok) {
            log('member', `Added ${level.name}: threshold=${level.threshold} indirect_referrals=${level.min_indirect_referrals}`);
          }
        }
      }
    } else {
      log('skip', 'Step 3 skipped');
    }

    const productIds: number[] = [];

    if (SKIP_UNTIL <= 3) {
      // ── Step 3b: Create 7 Plan Products ─────────────────────────────────
      logStep(3.5, 'Create 7 Plan Products');

      // Check existing products for this shop
      const existingProducts = codecToJson<number[]>(
        await (api.query as any).entityProduct.shopProducts(shopId),
      );
      if (existingProducts && Array.isArray(existingProducts) && existingProducts.length >= PLAN_PRODUCTS.length) {
        log('product', `Shop already has ${existingProducts.length} products — skipping creation`);
        productIds.push(...existingProducts);
      } else {
        // Check if shop needs top-up (only if FundDepleted)
        const shopRawForProducts = await (api.query as any).entityShop.shops(shopId);
        if (!(shopRawForProducts as any).isNone) {
          const shopJsonForProducts = codecToJson<Record<string, unknown>>((shopRawForProducts as any).unwrap());
          const shopStatus = String(readObjectField(shopJsonForProducts, 'status') ?? '');
          if (shopStatus === 'FundDepleted') {
            const fundOk = await trySubmitTx(
              api,
              (api.tx as any).entityShop.fundOperating(shopId, SHOP_FUND.toString()),
              account,
              `top-up shop operating fund (${formatNex(SHOP_FUND)})`,
            );
            if (fundOk.ok) {
              log('shop', `Topped up shop ${shopId} with ${formatNex(SHOP_FUND)} for product deposits`);
            }
          }
        }

        for (let i = 0; i < PLAN_PRODUCTS.length; i++) {
          const prod = PLAN_PRODUCTS[i];
          const nextId = await readNextProductId(api);

          const createProductTx = () => (api.tx as any).entityProduct.createProduct(
            shopId,
            bytes(prod.name),           // name_cid
            bytes(`img-${prod.name}`),  // images_cid
            bytes(`detail-${prod.name}`), // detail_cid
            prod.usdt_price,            // usdt_price (USDT, precision 10^6)
            0,                          // stock (0 = unlimited)
            'Digital',                  // category
            prod.sort_weight,           // sort_weight
            bytes(''),                  // tags_cid
            bytes(''),                  // sku_cid
            1,                          // min_order_quantity
            1,                          // max_order_quantity (1 per order)
            'MembersOnly',              // visibility
          );

          let createOk = await trySubmitTx(
            api,
            createProductTx(),
            account,
            `create product ${prod.name}`,
          );

          if (!createOk.ok && isInsufficientShopFundError(createOk.error)) {
            log('fund', `${prod.name}: shop fund insufficient, recovering operating fund and retrying`);
            await ensureShopOperatingFundForProducts(api, account, entityId, shopId);
            createOk = await trySubmitTx(
              api,
              createProductTx(),
              account,
              `retry create product ${prod.name}`,
            );
          }

          if (createOk.ok) {
            // Publish: Draft → OnSale
            const publishOk = await trySubmitTx(
              api,
              (api.tx as any).entityProduct.publishProduct(nextId),
              account,
              `publish product ${prod.name} (id=${nextId})`,
            );
            if (publishOk.ok) {
              productIds.push(nextId);
              log('product', `Created & published ${prod.name}: productId=${nextId} usdt_price=${prod.usdt_price}`);
            } else {
              productIds.push(nextId);
              log('warn', `${prod.name} created (id=${nextId}) but publish failed`);
            }
          }
        }

        log('product', `Total products created: ${productIds.length} — IDs: [${productIds.join(', ')}]`);
      }
    } else {
      log('skip', 'Step 3b skipped');
    }

    if (SKIP_UNTIL <= 4) {
      // ── Step 4: Configure Commission Core ─────────────────────────────
      logStep(4, 'Configure Commission Core');

      // Idempotency: check existing commission config
      const commConfigRaw = await (api.query as any).commissionCore.commissionConfigs(entityId);
      let commExists = false;
      let commEnabled = false;

      if (!(commConfigRaw as any).isNone) {
        commExists = true;
        const commJson = codecToJson<Record<string, unknown>>((commConfigRaw as any).unwrap());
        commEnabled = readObjectField(commJson, 'enabled') === true;
      }

      if (commExists && commEnabled) {
        log('skip', 'Step 4a-4e: Commission already configured and enabled — skipping');
      } else if (commExists && !commEnabled) {
        log('skip', 'Step 4a-4d: Commission config exists but not enabled — skipping to 4e');

        // 4e. Enable commission
        const enableOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.enableCommission(entityId, true),
          account,
          'enable commission',
        );
        if (enableOk.ok) log('commission', 'Commission enabled');
      } else {
        // 4a. Set commission rate: 99% (9900 bps) = 100% - 1% platform fee
        const rateOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.setCommissionRate(entityId, 9900),
          account,
          'set commission rate (9900 bps = 99%)',
        );
        if (rateOk.ok) log('commission', 'Rate = 99% (9900 bps) — 100% minus 1% platform fee');

        // 4b. Set plugin budget caps: ML=5000, SL=4400, sum=9400 = commission pool after owner_reward
        const capsOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.setPluginBudgetCaps(entityId, {
            referral_cap: 0,
            multi_level_cap: 5000,
            level_diff_cap: 0,
            single_line_cap: 4400,
            team_cap: 0,
          }),
          account,
          'set plugin budget caps',
        );
        if (capsOk.ok) log('commission', 'Plugin caps: multi_level=5000 single_line=4400 (sum=9400 after owner_reward)');

        // 4c. Set commission modes: ML + SL_UP + SL_DOWN + POOL_REWARD
        const modesOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.setCommissionModes(entityId, COMMISSION_MODES),
          account,
          `set commission modes (${COMMISSION_MODES})`,
        );
        if (modesOk.ok) log('commission', `Modes = ${COMMISSION_MODES} (ML + SL_UP + SL_DOWN + POOL_REWARD)`);

        // 4d. Set owner reward rate: 500 bps (5% of commission pool)
        const ownerRewardOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.setOwnerRewardRate(entityId, OWNER_REWARD_RATE),
          account,
          `set owner reward rate (${OWNER_REWARD_RATE} bps)`,
        );
        if (ownerRewardOk.ok) log('commission', `Owner reward rate = ${OWNER_REWARD_RATE} bps (5% of pool = 4.95% of order)`);

        // 4e. Enable commission
        const enableOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.enableCommission(entityId, true),
          account,
          'enable commission',
        );
        if (enableOk.ok) log('commission', 'Commission enabled');
      }

      // 4f. Set withdrawal config: LevelBased — 按等级分层提现/复投
      const wdConfigRaw = await (api.query as any).commissionCore.withdrawalConfigs(entityId);
      if (!(wdConfigRaw as any).isNone) {
        log('skip', 'Step 4f: Withdrawal config already exists — skipping');
      } else {
        const wdOk = await trySubmitTx(
          api,
          (api.tx as any).commissionCore.setWithdrawalConfig(
            entityId,
            WD_MODE,
            WD_DEFAULT_TIER,
            WD_LEVEL_OVERRIDES,
            WD_VOLUNTARY_BONUS_RATE,
            WD_ENABLED,
          ),
          account,
          'set withdrawal config (LevelBased)',
        );
        if (wdOk.ok) log('commission', 'Withdrawal config: LevelBased — Plan1~4=40/60 Plan5=50/50 Plan6=60/40 Plan7=70/30');
      }
    } else {
      log('skip', 'Step 4 skipped');
    }

    if (SKIP_UNTIL <= 5) {
      // ── Step 5: Configure Commission Plugins ──────────────────────────
      logStep(5, 'Configure Commission Plugins');

      // 5a. Multi-Level (动态奖励 — 13 级, 50%)
      const mlConfigRaw = await (api.query as any).commissionMultiLevel.multiLevelConfigs(entityId);
      if (!(mlConfigRaw as any).isNone) {
        log('skip', 'Step 5a: Multi-level config already exists — skipping');
      } else {
        const mlOk = await trySubmitTx(
          api,
          (api.tx as any).commissionMultiLevel.setMultiLevelConfig(
            entityId, ML_TIERS,
          ),
          account,
          'set multi-level config (13 tiers)',
        );
        if (mlOk.ok) {
          log('commission', `MultiLevel: 13 tiers [${ML_TIERS.map(t => t.rate).join(', ')}] max=${ML_MAX_RATE} (cap=5000)`);
        }
      }

      // 5b. Single-Line (静态奖励 — 公排 50%)
      const slConfigRaw = await (api.query as any).commissionSingleLine.singleLineConfigs(entityId);
      if (!(slConfigRaw as any).isNone) {
        log('skip', 'Step 5b: Single-line config already exists — skipping base config');
      } else {
        const slOk = await trySubmitTx(
          api,
          (api.tx as any).commissionSingleLine.setSingleLineConfig(
            entityId,
            SL_UPLINE_RATE,          // 44 bps (0.44%) per upline level
            SL_DOWNLINE_RATE,        // 44 bps (0.44%) per downline level
            SL_BASE_UPLINE_LEVELS,   // 20 (Plan1)
            SL_BASE_DOWNLINE_LEVELS, // 30 (Plan1)
            SL_LEVEL_INCREMENT_THRESHOLD,
            SL_MAX_UPLINE_LEVELS,    // 40 (Plan5~7)
            SL_MAX_DOWNLINE_LEVELS,  // 60 (Plan5~7)
            SL_ENABLED,
          ),
          account,
          'set single-line config',
        );
        if (slOk.ok) {
          log('commission', `SingleLine: up=${SL_UPLINE_RATE}bps×${SL_BASE_UPLINE_LEVELS}-${SL_MAX_UPLINE_LEVELS}lvl down=${SL_DOWNLINE_RATE}bps×${SL_BASE_DOWNLINE_LEVELS}-${SL_MAX_DOWNLINE_LEVELS}lvl`);
        }
      }

      // 5b-2. Single-Line Level Overrides (按套餐等级配置不同上/下线层数)
      for (const [levelId, upLevels, downLevels] of SL_LEVEL_OVERRIDES) {
        const overrideRaw = await (api.query as any).commissionSingleLine.singleLineCustomLevelOverrides(entityId, levelId);
        if (!(overrideRaw as any).isNone) {
          log('skip', `Step 5b-2: SL level override for level=${levelId} already exists — skipping`);
          continue;
        }
        const overrideOk = await trySubmitTx(
          api,
          (api.tx as any).commissionSingleLine.setLevelBasedLevels(
            entityId, levelId, upLevels, downLevels,
          ),
          account,
          `set SL level override (level=${levelId} up=${upLevels} down=${downLevels})`,
        );
        if (overrideOk.ok) {
          log('commission', `SL LevelOverride: level=${levelId} up=${upLevels} down=${downLevels} (total=${upLevels + downLevels})`);
        }
      }

      // 5c. Pool-Reward (沉淀资金分红)
      const prConfigRaw = await (api.query as any).commissionPoolReward.poolRewardConfigs(entityId);
      if (!(prConfigRaw as any).isNone) {
        log('skip', 'Step 5c: Pool-reward config already exists — skipping');
      } else {
        const prOk = await trySubmitTx(
          api,
          (api.tx as any).commissionPoolReward.setPoolRewardConfig(
            entityId,
            PR_LEVEL_RULES,
            PR_ROUND_DURATION,
          ),
          account,
          'set pool reward config',
        );
        if (prOk.ok) {
          log('commission', `PoolReward: Plan5=20% Plan6=30% Plan7=50%, ${PR_ROUND_DURATION} blocks/round (~24h)`);
        }
      }
    } else {
      log('skip', 'Step 5 skipped');
    }

    // ── Step 6: Verify & Output ─────────────────────────────────────────
    logStep(6, 'Verify & Output');

    // Query commission config
    const commConfig = codecToJson<Record<string, unknown>>(
      await (api.query as any).commissionCore.commissionConfigs(entityId),
    );
    log('verify', `Commission config: ${JSON.stringify(commConfig, null, 2)}`);

    // Query multi-level config
    const mlConfig = codecToJson<Record<string, unknown>>(
      await (api.query as any).commissionMultiLevel.multiLevelConfigs(entityId),
    );
    log('verify', `MultiLevel config: ${JSON.stringify(mlConfig, null, 2)}`);

    // Query single-line config
    const slConfig = codecToJson<Record<string, unknown>>(
      await (api.query as any).commissionSingleLine.singleLineConfigs(entityId),
    );
    log('verify', `SingleLine config: ${JSON.stringify(slConfig, null, 2)}`);

    // Query single-line level overrides
    const slOverrides: Record<number, unknown> = {};
    for (const [levelId] of SL_LEVEL_OVERRIDES) {
      const override = codecToJson<Record<string, unknown>>(
        await (api.query as any).commissionSingleLine.singleLineCustomLevelOverrides(entityId, levelId),
      );
      slOverrides[levelId] = override;
    }
    log('verify', `SingleLine level overrides: ${JSON.stringify(slOverrides, null, 2)}`);

    // Query pool-reward config
    const prConfig = codecToJson<Record<string, unknown>>(
      await (api.query as any).commissionPoolReward.poolRewardConfigs(entityId),
    );
    log('verify', `PoolReward config: ${JSON.stringify(prConfig, null, 2)}`);

    // Query level system
    const levelSystem = codecToJson<Record<string, unknown>>(
      await (api.query as any).entityMember.entityLevelSystems(entityId),
    );
    log('verify', `Level system: ${JSON.stringify(levelSystem, null, 2)}`);

    // Query products
    const productsInfo: Record<string, unknown>[] = [];
    for (const pid of productIds) {
      const product = codecToJson<Record<string, unknown>>(
        await (api.query as any).entityProduct.products(pid),
      );
      productsInfo.push({ productId: pid, ...product });
      log('verify', `Product ${pid}: ${JSON.stringify(product)}`);
    }

    // Summary
    const summary = {
      timestamp: new Date().toISOString(),
      address: account.address,
      entityId,
      shopId,
      productIds,
      entityName: ENTITY_NAME,
      shopFund: formatNex(SHOP_FUND),
      memberLevels: MEMBER_LEVELS.length,
      products: PLAN_PRODUCTS.map((p, i) => ({
        name: p.name,
        usdt_price: p.usdt_price,
        productId: productIds[i] ?? null,
      })),
      commissionRate: '99% (9900 bps) — 100% minus 1% platform fee',
      ownerRewardRate: `${OWNER_REWARD_RATE} bps (5% of pool = 4.95% of order)`,
      commissionModes: COMMISSION_MODES,
      plugins: {
        multiLevel: {
          tiers: ML_TIERS.length,
          maxRate: ML_MAX_RATE,
          description: '13-tier dynamic reward (cap=5000 bps of order)',
        },
        singleLine: {
          uplineRate: SL_UPLINE_RATE,
          downlineRate: SL_DOWNLINE_RATE,
          baseLevels: `${SL_BASE_UPLINE_LEVELS}/${SL_BASE_DOWNLINE_LEVELS}`,
          maxLevels: `${SL_MAX_UPLINE_LEVELS}/${SL_MAX_DOWNLINE_LEVELS}`,
          levelOverrides: SL_LEVEL_OVERRIDES.map(([id, up, down]) => ({
            level_id: id, upline: up, downline: down, total: up + down,
          })),
          description: `static reward single-line ${SL_UPLINE_RATE}bps up / ${SL_DOWNLINE_RATE}bps down per level (cap=4400 bps of order)`,
        },
        poolReward: {
          roundDuration: PR_ROUND_DURATION,
          ratios: 'Plan5=20% Plan6=30% Plan7=50%',
        },
      },
      withdrawal: {
        mode: 'LevelBased',
        default: `${WD_DEFAULT_TIER.withdrawal_rate / 100}% withdraw / ${WD_DEFAULT_TIER.repurchase_rate / 100}% repurchase`,
        overrides: WD_LEVEL_OVERRIDES.map(([id, tier]) => ({
          level_id: id,
          withdrawal: `${tier.withdrawal_rate / 100}%`,
          repurchase: `${tier.repurchase_rate / 100}%`,
        })),
      },
      configs: {
        commission: commConfig,
        multiLevel: mlConfig,
        singleLine: slConfig,
        singleLineOverrides: slOverrides,
        poolReward: prConfig,
        levelSystem,
        products: productsInfo,
      },
    };

    console.log(`\n${'='.repeat(70)}`);
    console.log('  SETUP COMPLETE');
    console.log(`${'='.repeat(70)}`);
    console.log(JSON.stringify(summary, null, 2));

    // Save result to file
    const scriptDir = fileURLToPath(new URL('.', import.meta.url));
    await mkdir(join(scriptDir, 'secrets'), { recursive: true });
    const resultFile = join(
      scriptDir,
      'secrets',
      `setup-result-${new Date().toISOString().replace(/[:.]/g, '-')}.json`,
    );
    await writeFile(resultFile, JSON.stringify(summary, null, 2));
    log('output', `Result saved to ${resultFile}`);

    // ── Step 7: Fund Test Accounts from JSON files ────────────────────
    logStep(7, 'Fund Test Accounts (3 JSON files × 20 accounts × 1亿 NEX)');

    const FUND_PER_ACCOUNT = nex(100_000_000); // 1亿 NEX
    const TEST_ACCOUNT_FILES = [
      join(scriptDir, '..', 'framework', 'test-accounts-2026-03-20T01-03-22-751Z.json'),
      join(scriptDir, '..', 'framework', 'test-accounts-2026-03-20T00-38-47-605Z.json'),
      join(scriptDir, '..', 'framework', 'test-accounts-2026-03-20T00-37-56-148Z.json'),
    ];

    // Collect all unique accounts (address + name) from the 3 files
    const accountMap = new Map<string, string>(); // address -> name
    for (const filePath of TEST_ACCOUNT_FILES) {
      const fileName = filePath.split('/').pop()!;
      log('load', `Reading ${fileName} ...`);
      try {
        const raw = await readFile(filePath, 'utf-8');
        const data = JSON.parse(raw) as { accounts: { address: string; name: string }[] };
        let newCount = 0;
        for (const acc of data.accounts) {
          if (!accountMap.has(acc.address)) {
            accountMap.set(acc.address, acc.name);
            newCount++;
          }
        }
        log('load', `  ${fileName}: ${data.accounts.length} accounts, ${newCount} new unique`);
      } catch (err: any) {
        log('warn', `  Failed to read ${fileName}: ${err.message}`);
      }
    }

    const allAccounts = [...accountMap.entries()]; // [address, name][]
    log('fund', `Total unique accounts: ${allAccounts.length}, target balance: ${formatNex(FUND_PER_ACCOUNT)}`);

    // Check funder balance first
    const funderBalance = await readFreeBalance(api, account.address);
    log('fund', `Funder ${account.address.slice(0, 14)}... balance: ${formatNex(funderBalance)}`);

    let funded = 0;
    let skipped = 0;
    let failed = 0;

    console.log(`\n  ${'#'.padEnd(5)} ${'Name'.padEnd(16)} ${'Address'.padEnd(20)} ${'Balance'.padStart(24)}  ${'Action'.padStart(10)}`);
    console.log(`  ${'-'.repeat(80)}`);

    for (let idx = 0; idx < allAccounts.length; idx++) {
      const [addr, name] = allAccounts[idx];
      const seq = `${idx + 1}/${allAccounts.length}`;

      // Query current balance
      log('query', `[${seq}] ${name} (${addr.slice(0, 14)}...) — querying balance...`);
      const currentBalance = await readFreeBalance(api, addr);
      log('query', `[${seq}] ${name} balance = ${formatNex(currentBalance)}`);

      if (currentBalance >= FUND_PER_ACCOUNT) {
        skipped++;
        log('skip', `[${seq}] ${name} — already has ${formatNex(currentBalance)} >= ${formatNex(FUND_PER_ACCOUNT)}, SKIP`);
        console.log(`  ${String(idx + 1).padEnd(5)} ${name.padEnd(16)} ${addr.slice(0, 18).padEnd(20)} ${formatNex(currentBalance).padStart(24)}  ${'SKIP'.padStart(10)}`);
        continue;
      }

      // Need to fund
      const deficit = FUND_PER_ACCOUNT - currentBalance;
      log('tx', `[${seq}] ${name} — need ${formatNex(deficit)}, sending ${formatNex(FUND_PER_ACCOUNT)}...`);

      const ok = await trySubmitTx(
        api,
        (api.tx as any).balances.transferKeepAlive(addr, FUND_PER_ACCOUNT.toString()),
        account,
        `fund ${name}`,
      );

      if (ok) {
        funded++;
        const newBalance = await readFreeBalance(api, addr);
        log('ok', `[${seq}] ${name} — funded! new balance = ${formatNex(newBalance)}`);
        console.log(`  ${String(idx + 1).padEnd(5)} ${name.padEnd(16)} ${addr.slice(0, 18).padEnd(20)} ${formatNex(newBalance).padStart(24)}  ${'FUNDED'.padStart(10)}`);
      } else {
        failed++;
        log('fail', `[${seq}] ${name} — transfer FAILED`);
        console.log(`  ${String(idx + 1).padEnd(5)} ${name.padEnd(16)} ${addr.slice(0, 18).padEnd(20)} ${formatNex(currentBalance).padStart(24)}  ${'FAILED'.padStart(10)}`);
      }
    }

    console.log(`\n  ${'='.repeat(80)}`);
    console.log(`  FUND SUMMARY: total=${allAccounts.length}  funded=${funded}  skipped=${skipped}  failed=${failed}`);
    console.log(`  ${'='.repeat(80)}\n`);

  } finally {
    await disconnectApi(api);
    log('cleanup', 'Disconnected from chain');
  }
}

main().catch((err) => {
  console.error('Setup failed:', err);
  process.exit(1);
});
