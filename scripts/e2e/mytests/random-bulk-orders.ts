#!/usr/bin/env tsx
/**
 * 随机批量下单脚本 / Random Bulk Orders Script
 *
 * 连接到 wss://rpc.nexusmall.net，针对实体 100010 的主店铺。
 * 从指定的 2 个 JSON 文件加载测试账户。
 * 随机挑选商品与买家，批量下 N 笔订单。
 *
 * placeOrder 内置自动注册逻辑：
 *   - 启动时先确保有一个"种子会员"（seed member）作为首单推荐人
 *   - 若链上已有会员（非卖家），直接使用其中一个作为种子推荐人
 *   - 若无现有会员，则将第一个有余额的测试账户注册为会员（无推荐人）
 *   - 后续订单推荐人从已成功下单的买家中随机选择（排除买家本人）
 *
 * 用法 / Usage:
 *   node --import tsx mytests/random-bulk-orders.ts
 *
 * 环境变量 / Environment variables:
 *   WS_URL           — WebSocket 端点（默认: wss://rpc.nexusmall.net）
 *   ENTITY_ID        — 目标实体 ID（默认: 100008）
 *   ORDER_COUNT      — 下单数量（默认: 100）
 *   ORDER_QTY        — 每笔订单数量（默认: 1）
 */

process.env.WS_URL ??= 'wss://rpc.nexusmall.net';

import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import type { KeyringPair } from '@polkadot/keyring/types';
import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { formatNex, NEX_PLANCK } from '../framework/units.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  Configuration                                                      */
/* ------------------------------------------------------------------ */

const PREFERRED_SEED_MEMBER = 'X4VND53VCaaS8fa2zLH54nQhrNBP5vJQEza1NAy1YRm2L8za6';
const scriptDir = fileURLToPath(new URL('.', import.meta.url));
const ENTITY_ID = Number(process.env.ENTITY_ID ?? 100010);
const ORDER_COUNT = Number(process.env.ORDER_COUNT ?? 100);
const ORDER_QTY = Math.max(1, Number(process.env.ORDER_QTY ?? 1));

const TEST_ACCOUNT_FILES = [
  join(scriptDir, 'test-accounts-2026-04-15T05-59-05-187Z100010.json'),
  join(scriptDir, 'test-accounts-2026-04-15T05-57-40-385Z100010.json'),
  join(scriptDir, 'test-accounts-2026-04-15T05-57-36-684Z100010.json'),
  join(scriptDir, 'test-accounts-2026-04-15T05-57-31-426Z100010.json'),
  join(scriptDir, 'test-accounts-2026-04-15T05-56-58-825Z100010.json'),
];

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
 * 简单休眠一小段时间，避免批量交易连续提交过快。
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * 将十六进制编码名称（如 0x506c616e...）解码为可读文本。
 */
function decodeHexName(raw: string): string {
  if (raw.startsWith('0x')) {
    try {
      return Buffer.from(raw.slice(2), 'hex').toString('utf-8');
    } catch {
      return raw;
    }
  }
  return raw;
}

/**
 * 按 USDT 精度（10^6）格式化金额。
 */
function formatUsdt(raw: bigint): string {
  return `${(Number(raw) / 1e6).toFixed(2)} USDT`;
}

/**
 * 计算从起始时间到当前的耗时秒数文本。
 */
function elapsed(startMs: number): string {
  return `${((Date.now() - startMs) / 1000).toFixed(2)}s`;
}

interface NativeBalanceSnapshot {
  free: bigint;
  reserved: bigint;
  frozen: bigint;
  miscFrozen: bigint;
  feeFrozen: bigint;
  transferableEstimate: bigint;
}

const NATIVE_ORDER_SAFETY_BUFFER = 10_000_000_000_000n;

/** 与 runtime `TradingPricingProvider::get_nex_to_usd_rate` / 订单 `do_place_order` 中 USDT→NEX 换算一致 / Matches on-chain pricing */
function nexAmountFromUsdtMicro(discountedUsdtMicro: bigint, nexUsdtPrice: bigint): bigint {
  if (nexUsdtPrice === 0n) {
    return 0n;
  }
  return (discountedUsdtMicro * NEX_PLANCK) / nexUsdtPrice;
}

/**
 * 与 `runtime` 中 `TradingPricingProvider::get_nex_to_usd_rate` 同优先级：1h TWAP → LastTrade → initial_price。
 */
async function getEffectiveNexUsdtPrice(api: ApiPromise): Promise<{ rate: bigint; source: string }> {
  const nm = (api.query as any).nexMarket;
  if (!nm) {
    return { rate: 0n, source: 'no-nexMarket-pallet' };
  }

  let header;
  try {
    header = await api.rpc.chain.getHeader();
  } catch {
    header = null;
  }
  const currentBlock = header?.number?.toNumber?.() ?? 0;

  const twapStore = nm.twapAccumulatorStore ?? nm.twapAccumulator;
  if (twapStore && currentBlock > 0) {
    try {
      const rawAcc = await twapStore();
      const empty =
        rawAcc == null ||
        (typeof (rawAcc as any).isEmpty === 'boolean' && (rawAcc as any).isEmpty) ||
        (typeof (rawAcc as any).isNone === 'boolean' && (rawAcc as any).isNone);
      if (!empty) {
        const accJson = codecToJson<Record<string, unknown>>(rawAcc);
        const twap = calculateTwapOneHourFromAccumulator(accJson, currentBlock);
        if (twap != null && twap > 0n) {
          return { rate: twap, source: 'twap1h' };
        }
      }
    } catch {
      /* fall through */
    }
  }

  if (nm.lastTradePrice) {
    try {
      const raw = await nm.lastTradePrice();
      const v = BigInt(raw.toString());
      if (v > 0n) {
        return { rate: v, source: 'lastTradePrice' };
      }
    } catch {
      /* fall through */
    }
  }

  const ppQuery = nm.priceProtectionStore ?? nm.priceProtection;
  if (ppQuery) {
    try {
      const ppRaw = await ppQuery();
      const pp = codecToJson<Record<string, unknown>>(ppRaw);
      const initial = readObjectField(pp, 'initialPrice', 'initial_price');
      if (initial != null && initial !== false && initial !== '') {
        const v = BigInt(String(initial).replace(/,/g, ''));
        if (v > 0n) {
          return { rate: v, source: 'initialPrice' };
        }
      }
    } catch {
      /* fall through */
    }
  }

  return { rate: 0n, source: 'none' };
}

/**
 * 复刻 `pallet_nex_market::Pallet::calculate_twap(TwapPeriod::OneHour)`（用于与 Entity 订单同一套 NEX/USDT 参考价）。
 * Mirrors nex-market `calculate_twap(OneHour)` for the effective rate used by entity orders.
 */
function calculateTwapOneHourFromAccumulator(
  acc: Record<string, unknown>,
  currentBlock: number,
): bigint | null {
  const currentCumulative = BigInt(String(readObjectField(acc, 'currentCumulative', 'current_cumulative') ?? '0'));
  const accBlock = coerceNumber(readObjectField(acc, 'currentBlock', 'current_block'));
  const lastPrice = BigInt(String(readObjectField(acc, 'lastPrice', 'last_price') ?? '0'));
  const hourSnap = readObjectField(acc, 'hourSnapshot', 'hour_snapshot') as Record<string, unknown> | undefined;
  if (accBlock == null || !hourSnap) {
    return null;
  }

  const snapBlock = coerceNumber(readObjectField(hourSnap, 'blockNumber', 'block_number'));
  const snapC = BigInt(String(readObjectField(hourSnap, 'cumulativePrice', 'cumulative_price') ?? '0'));
  if (snapBlock == null) {
    return null;
  }

  const blocksSince = Math.max(0, currentBlock - accBlock);
  const currentCumulativeAdj = currentCumulative + lastPrice * BigInt(blocksSince);
  const blockDiff = currentBlock - snapBlock;
  if (blockDiff === 0) {
    return lastPrice > 0n ? lastPrice : null;
  }
  const cumulativeDiff = currentCumulativeAdj >= snapC ? currentCumulativeAdj - snapC : 0n;
  const q = cumulativeDiff / BigInt(blockDiff);
  return q > 0n ? q : null;
}

/**
 * 读取账户的原生余额快照，并给出可转移余额估算值。
 */
async function readNativeBalanceSnapshot(api: ApiPromise, address: string): Promise<NativeBalanceSnapshot> {
  const account = await api.query.system.account(address);
  const data = (account as any).data as {
    free: { toString(): string };
    reserved?: { toString(): string };
    frozen?: { toString(): string };
    miscFrozen?: { toString(): string };
    feeFrozen?: { toString(): string };
  };

  const free = BigInt(data.free.toString());
  const reserved = BigInt(data.reserved?.toString() ?? '0');
  const frozen = BigInt(data.frozen?.toString() ?? '0');
  const miscFrozen = BigInt(data.miscFrozen?.toString() ?? '0');
  const feeFrozen = BigInt(data.feeFrozen?.toString() ?? '0');
  const effectiveFrozen = [frozen, miscFrozen, feeFrozen].reduce((max, value) => value > max ? value : max, 0n);
  const transferableEstimate = free > effectiveFrozen ? free - effectiveFrozen : 0n;

  return {
    free,
    reserved,
    frozen,
    miscFrozen,
    feeFrozen,
    transferableEstimate,
  };
}

/**
 * Native 订单预检：应锁 NEX + 手续费/余量缓冲（与链上 escrow 锁定同阶）。
 * Preflight: NEX to lock + fee buffer (same order of magnitude as escrow lock).
 */
function estimateRequiredNativeForLock(lockNex: bigint): bigint {
  return lockNex + NATIVE_ORDER_SAFETY_BUFFER;
}

/**
 * 统一格式化余额快照，便于日志定位余额/冻结问题。
 */
function formatBalanceSnapshot(snapshot: NativeBalanceSnapshot): string {
  return [
    `free=${formatNex(snapshot.free)}`,
    `reserved=${formatNex(snapshot.reserved)}`,
    `frozen=${formatNex(snapshot.frozen)}`,
    `miscFrozen=${formatNex(snapshot.miscFrozen)}`,
    `feeFrozen=${formatNex(snapshot.feeFrozen)}`,
    `transferable≈${formatNex(snapshot.transferableEstimate)}`,
  ].join(' ');
}

/* ------------------------------------------------------------------ */
/*  Account loading                                                    */
/* ------------------------------------------------------------------ */

interface TestAccount {
  mnemonic: string;
  address: string;
  name: string;
  sourceFile: string;
}

/**
 * 从多个 JSON 文件加载测试账户并去重，同时派生签名对。
 */
async function loadTestAccounts(keyring: Keyring): Promise<{ accounts: TestAccount[]; pairs: Map<string, KeyringPair> }> {
  const accounts: TestAccount[] = [];
  const seen = new Set<string>();

  for (const filePath of TEST_ACCOUNT_FILES) {
    const fileName = filePath.split('/').pop()!;
    log('load', `Reading ${fileName} ...`);
    try {
      const raw = await readFile(filePath, 'utf-8');
      const data = JSON.parse(raw) as { accounts: { mnemonic: string; address: string; name: string }[] };
      let newCount = 0;
      for (const acc of data.accounts) {
        if (!seen.has(acc.address)) {
          seen.add(acc.address);
          accounts.push({ ...acc, sourceFile: fileName });
          newCount++;
        }
      }
      log('load', `  ${fileName}: ${data.accounts.length} accounts, ${newCount} new unique`);
    } catch (err: any) {
      log('warn', `  Failed to read ${fileName}: ${err.message}`);
    }
  }

  const pairs = new Map<string, KeyringPair>();
  for (const acc of accounts) {
    const pair = keyring.addFromMnemonic(acc.mnemonic);
    pairs.set(acc.address, pair);
  }

  log('load', `Total unique accounts: ${accounts.length}`);
  return { accounts, pairs };
}

/* ------------------------------------------------------------------ */
/*  链上查询 / Chain queries                                           */
/* ------------------------------------------------------------------ */

/**
 * 读取实体的主店铺 ID。
 */
async function getEntityPrimaryShopId(api: ApiPromise, entityId: number): Promise<number> {
  log('chain', `Querying entity ${entityId} ...`);
  const value = await (api.query as any).entityRegistry.entities(entityId);
  if ((value as any).isNone) throw new Error(`Entity ${entityId} does not exist`);
  const entity = (value as any).unwrap();
  const json = codecToJson<Record<string, unknown>>(entity);
  const shopId = coerceNumber(readObjectField(json, 'primaryShopId', 'primary_shop_id'));
  if (shopId == null || shopId <= 0) throw new Error(`Entity ${entityId} has no primary shop`);
  return shopId;
}

interface ProductFundingInfo {
  product: ProductInfo;
  usdtMicroTotal: bigint;
  lockNex: bigint;
  requiredNative: bigint;
  affordableBuyers: TestAccount[];
}

interface OrderPlan {
  index: number;
  productId: number;
  productName: string;
  /** USDT 标价（10^6）× 数量；与链上 `usdt_total` 一致 / Matches on-chain usdt_total */
  usdtMicroTotal: bigint;
  buyerAddress: string;
  buyerName: string;
}

/**
 * 读取店铺商品列表，并筛出可下单的 OnSale 商品。
 */
async function getShopProducts(api: ApiPromise, shopId: number): Promise<ProductInfo[]> {
  log('chain', `Querying products for shop ${shopId} ...`);
  const productIds = codecToJson<number[]>(
    await (api.query as any).entityProduct.shopProducts(shopId),
  );
  if (!productIds || !Array.isArray(productIds) || productIds.length === 0) {
    throw new Error(`Shop ${shopId} has no products`);
  }
  log('chain', `Shop ${shopId} has ${productIds.length} product IDs: [${productIds.join(', ')}]`);

  const products: ProductInfo[] = [];
  for (const pid of productIds) {
    log('chain', `  Querying product #${pid} ...`);
    const value = await (api.query as any).entityProduct.products(pid);
    if ((value as any).isNone) {
      log('chain', `  Product #${pid} — not found (None), skipping`);
      continue;
    }
    const product = (value as any).unwrap();
    const json = codecToJson<Record<string, unknown>>(product);
    const status = String(readObjectField(json, 'status') ?? '');
    if (status !== 'OnSale') {
      log('chain', `  Product #${pid} — status="${status}", skipping (need OnSale)`);
      continue;
    }

    const rawName = String(readObjectField(json, 'nameCid', 'name_cid') ?? `product-${pid}`);
    const usdtPrice = BigInt(String(readObjectField(json, 'usdtPrice', 'usdt_price') ?? '0').replace(/,/g, ''));
    const decodedName = decodeHexName(rawName);

    products.push({
      productId: pid,
      name: decodedName,
      usdtPrice,
      status,
      category: String(readObjectField(json, 'category') ?? ''),
      visibility: String(readObjectField(json, 'visibility') ?? ''),
    });
    log('chain', `  Product #${pid} — "${decodedName}" ${formatUsdt(usdtPrice)} [${status}] OK`);
  }

  return products;
}

/**
 * 查询实体的会员列表（取前 N 个），返回会员账户地址列表。
 * Query up to maxCount members for an entity, returning their account addresses.
 */
async function queryEntityMembers(api: ApiPromise, entityId: number, maxCount = 20): Promise<string[]> {
  const entries = await (api.query as any).entityMember.entityMembers.entries(entityId);
  const members: string[] = [];
  for (const [key] of entries) {
    const args = key.args as any[];
    if (args.length >= 2) {
      members.push(args[1].toString());
    }
    if (members.length >= maxCount) break;
  }
  return members;
}

/* ------------------------------------------------------------------ */
/*  随机辅助 / Random helpers                                          */
/* ------------------------------------------------------------------ */

/**
 * 从数组中随机取出一个元素。
 */
function randomElement<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

/* ------------------------------------------------------------------ */
/*  主流程 / Main                                                      */
/* ------------------------------------------------------------------ */

/**
 * 主入口：构建批量订单计划并逐笔执行，同时输出汇总结果。
 */
async function main(): Promise<void> {
  const scriptStart = Date.now();

  console.log(`${'='.repeat(70)}`);
  console.log(`  随机批量下单 / Random Bulk Orders — Entity ${ENTITY_ID} — ${ORDER_COUNT} orders × qty ${ORDER_QTY}`);
  console.log(`${'='.repeat(70)}\n`);

  // ── 1. Initialize ─────────────────────────────────────────────────────
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const api = await connectApi();
  log('init', `已连接节点 / Connected to ${process.env.WS_URL}`);

  try {
    // ── 2. Load test accounts ───────────────────────────────────────────
    const { accounts, pairs } = await loadTestAccounts(keyring);
    if (accounts.length === 0) throw new Error('未找到测试账户 / No test accounts found');

    // ── 2b. Pre-check: verify at least some accounts have balance ───────
    log('check', `Pre-checking balances for ${accounts.length} accounts ...`);
    let fundedCount = 0;
    let zeroCount = 0;
    let transferableCount = 0;
    for (const acc of accounts) {
      const snapshot = await readNativeBalanceSnapshot(api, acc.address);
      if (snapshot.free > 0n) {
        fundedCount++;
      } else {
        zeroCount++;
      }
      if (snapshot.transferableEstimate > NATIVE_ORDER_SAFETY_BUFFER) {
        transferableCount++;
      }
    }
    log('check', `Balance check: ${fundedCount} funded, ${zeroCount} zero-balance, ${transferableCount} transferable-above-buffer`);
    if (fundedCount === 0) {
      throw new Error('All test accounts have zero balance — run "SKIP_UNTIL=7 npm run setup:entity" first');
    }
    if (zeroCount > 0) {
      log('warn', `${zeroCount} accounts have zero balance and will fail if selected as buyer`);
    }

    // ── 3. Get shop & products ──────────────────────────────────────────
    const shopId = await getEntityPrimaryShopId(api, ENTITY_ID);
    log('chain', `Entity ${ENTITY_ID} primary shop: ${shopId}`);

    const products = await getShopProducts(api, shopId);
    if (products.length === 0) throw new Error(`No published products in shop ${shopId}`);
    log('chain', `Available products: ${products.length}`);

    const { rate: sampleNexUsdt, source: samplePriceSource } = await getEffectiveNexUsdtPrice(api);
    if (sampleNexUsdt === 0n) {
      throw new Error(
        'NEX/USDT 价格不可用（TWAP / LastTrade / initial_price 均为 0）。无法估算下单所需 NEX。/ No NEX/USDT rate — cannot estimate order cost.',
      );
    }
    log(
      'price',
      `NEX/USDT 参考价 / Effective rate: ${sampleNexUsdt.toString()} (10^6 USDT per NEX) — source=${samplePriceSource}`,
    );
    console.log('');
    console.log(`  ${'ID'.padEnd(5)} ${'Name'.padEnd(20)} ${'USDT Price'.padStart(14)} ${'Category'.padEnd(10)} ${'Visibility'.padEnd(14)}`);
    console.log(`  ${'-'.repeat(65)}`);
    for (const p of products) {
      console.log(`  ${String(p.productId).padEnd(5)} ${p.name.padEnd(20)} ${formatUsdt(p.usdtPrice).padStart(14)} ${p.category.padEnd(10)} ${String(p.visibility).padEnd(14)}`);
    }
    console.log('');

    // ── 3b. Ensure seed member (referrer bootstrap) ─────────────────────
    // All products are MembersOnly. The order pallet auto-registers a buyer
    // only when a referrer is supplied. We need at least one existing member
    // (who is NOT the seller) to act as the first referrer.
    log('seed', 'Checking for existing entity members ...');
    // The seller = entity owner (shop_owner delegates to entity_owner in the pallet)
    const entityRaw = await (api.query as any).entityRegistry.entities(ENTITY_ID);
    const entityJson = entityRaw.isNone ? {} : codecToJson<Record<string, unknown>>(entityRaw.unwrap());
    const sellerAddress = String(readObjectField(entityJson, 'owner') ?? '');
    const existingMembers = await queryEntityMembers(api, ENTITY_ID);
    const nonSellerMembers = existingMembers.filter(addr => addr !== sellerAddress);
    log('seed', `Seller: ${sellerAddress.slice(0, 18)}...`);
    log('seed', `Found ${existingMembers.length} member(s), ${nonSellerMembers.length} non-seller`);

    // Track addresses that have successfully placed orders (for referrer pool)
    // Pre-seed with existing non-seller members so first order has a valid referrer.
    const succeededBuyers = new Set<string>(nonSellerMembers);
    const unusedReferrers = new Set<string>(nonSellerMembers);
    const initialReferrerCount = unusedReferrers.size;

    if (nonSellerMembers.length === 0) {
      // No existing members — register the preferred funded test account as a seed member.
      const seedAcc = accounts.find(acc => acc.address === PREFERRED_SEED_MEMBER) ?? accounts.find(acc => pairs.has(acc.address));
      if (!seedAcc) throw new Error('No funded test accounts available for seed member registration');
      const seedPair = pairs.get(seedAcc.address)!;

      log('seed', `No existing members — registering seed member: ${seedAcc.name} (${seedAcc.address.slice(0, 18)}...)`);
      const seedTx = (api.tx as any).entityMember.registerMember(shopId, null);
      const seedReceipt: TxReceipt = await submitTx(api, seedTx, seedPair, 'seed-member');
      if (seedReceipt.success) {
        succeededBuyers.add(seedAcc.address);
        unusedReferrers.add(seedAcc.address);
        log('seed', `Seed member registered: ${seedAcc.name}`);
      } else {
        log('warn', `Seed member registration failed: ${seedReceipt.error} — orders may fail for unregistered buyers`);
      }
    }

    // ── 4. Build order plan ─────────────────────────────────────────────
    const accountSnapshots = new Map<string, NativeBalanceSnapshot>();
    for (const acc of accounts) {
      accountSnapshots.set(acc.address, await readNativeBalanceSnapshot(api, acc.address));
    }

    const productFunding: ProductFundingInfo[] = products.map((product) => {
      const usdtMicroTotal = product.usdtPrice * BigInt(ORDER_QTY);
      const lockNex = nexAmountFromUsdtMicro(usdtMicroTotal, sampleNexUsdt);
      const requiredNative = estimateRequiredNativeForLock(lockNex);
      const affordableBuyers = accounts.filter((acc) => {
        const snapshot = accountSnapshots.get(acc.address);
        return snapshot != null && snapshot.transferableEstimate >= requiredNative;
      });
      return {
        product,
        usdtMicroTotal,
        lockNex,
        requiredNative,
        affordableBuyers,
      };
    });

    log('plan', 'Affordability by product:');
    for (const funding of productFunding) {
      log(
        'plan',
        `  product #${funding.product.productId} "${funding.product.name}": ${formatUsdt(funding.usdtMicroTotal)} -> lock≈${formatNex(funding.lockNex)} required≈${formatNex(funding.requiredNative)} affordable=${funding.affordableBuyers.length}/${accounts.length}`,
      );
    }

    const viableProducts = productFunding.filter((funding) => funding.affordableBuyers.length > 0);
    if (viableProducts.length === 0) {
      throw new Error('No products have any buyer with enough transferable NEX for Native escrow');
    }

    const orderPlans: OrderPlan[] = [];
    for (let i = 0; i < ORDER_COUNT; i++) {
      const funding = randomElement(viableProducts);
      const buyer = randomElement(funding.affordableBuyers);

      orderPlans.push({
        index: i + 1,
        productId: funding.product.productId,
        productName: funding.product.name,
        usdtMicroTotal: funding.usdtMicroTotal,
        buyerAddress: buyer.address,
        buyerName: buyer.name,
      });
    }

    // Show distribution
    const productDist = new Map<number, number>();
    const buyerDist = new Map<string, number>();
    for (const plan of orderPlans) {
      productDist.set(plan.productId, (productDist.get(plan.productId) ?? 0) + 1);
      buyerDist.set(plan.buyerAddress, (buyerDist.get(plan.buyerAddress) ?? 0) + 1);
    }
    log('plan', `Planned ${orderPlans.length} orders — ${productDist.size} products, ${buyerDist.size} unique buyers`);
    for (const [pid, count] of productDist.entries()) {
      const p = products.find(x => x.productId === pid);
      log('plan', `  product #${pid} "${p?.name}": ${count} orders`);
    }

    // ── 5. Execute orders ───────────────────────────────────────────────
    let succeeded = 0;
    let failed = 0;
    const errors: { index: number; buyer: string; product: string; error: string }[] = [];

    console.log(`\n${'─'.repeat(70)}`);
    log('exec', `Starting ${orderPlans.length} orders ...`);
    console.log(`${'─'.repeat(70)}\n`);

    for (let i = 0; i < orderPlans.length; i++) {
      const plan = orderPlans[i];
      const seq = `${plan.index}/${ORDER_COUNT}`;
      const orderStart = Date.now();

      log('order', `━━━ Order #${seq} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━`);
      log('order', `  Buyer:   ${plan.buyerName} (${plan.buyerAddress.slice(0, 18)}...)`);
      log('order', `  Product: #${plan.productId} "${plan.productName}" ${formatUsdt(plan.usdtMicroTotal)} (×${ORDER_QTY})`);

      // 5a. Check keypair
      const pair = pairs.get(plan.buyerAddress);
      if (!pair) {
        failed++;
        errors.push({ index: plan.index, buyer: plan.buyerName, product: plan.productName, error: 'missing keypair' });
        log('fail', `  [${seq}] Missing keypair for ${plan.buyerName} — SKIP`);
        continue;
      }

      // 5b. Query buyer balance
      log('query', `  [${seq}] Querying buyer balance ...`);
      const buyerBalance = await readFreeBalance(api, plan.buyerAddress);
      const balanceSnapshot = await readNativeBalanceSnapshot(api, plan.buyerAddress);

      const { rate: nexUsdtPrice, source: priceSource } = await getEffectiveNexUsdtPrice(api);
      if (nexUsdtPrice === 0n) {
        failed++;
        errors.push({
          index: plan.index,
          buyer: plan.buyerName,
          product: plan.productName,
          error: 'NEX/USDT rate unavailable',
        });
        log('fail', `  [${seq}] NEX/USDT rate unavailable — SKIP`);
        continue;
      }

      // 与 `do_place_order` 一致：会员折扣在链上会降低实际锁定额；此处按标价全额估算（偏保守，避免低估）。
      // Matches on-chain path; member discount reduces lock — we use full USDT here (conservative lower bound on funds).
      const lockNex = nexAmountFromUsdtMicro(plan.usdtMicroTotal, nexUsdtPrice);
      const requiredNative = estimateRequiredNativeForLock(lockNex);

      log('query', `  [${seq}] Buyer balance: ${formatNex(buyerBalance)}`);
      log('query', `  [${seq}] Native snapshot: ${formatBalanceSnapshot(balanceSnapshot)}`);
      log(
        'query',
        `  [${seq}] NEX/USDT=${nexUsdtPrice.toString()} (${priceSource}) → lock≈${formatNex(lockNex)} + buffer=${formatNex(NATIVE_ORDER_SAFETY_BUFFER)} → required≈${formatNex(requiredNative)}`,
      );

      if (buyerBalance === 0n) {
        failed++;
        errors.push({ index: plan.index, buyer: plan.buyerName, product: plan.productName, error: 'zero balance' });
        log('fail', `  [${seq}] Buyer has zero balance — SKIP`);
        continue;
      }

      if (balanceSnapshot.transferableEstimate < requiredNative) {
        failed++;
        errors.push({ index: plan.index, buyer: plan.buyerName, product: plan.productName, error: 'preflight insufficient transferable for Native escrow' });
        log('fail', `  [${seq}] Insufficient transferable balance for Native escrow — SKIP`);
        log('fail', `  [${seq}]   ${formatBalanceSnapshot(balanceSnapshot)}`);
        log('fail', `  [${seq}]   required≈${formatNex(requiredNative)} lock≈${formatNex(lockNex)} usdtMicro=${plan.usdtMicroTotal.toString()}`);
        continue;
      }

      // 5c. Determine referrer (avoid self-referral)
      // Each referrer can be used at most once per run.
      // When no unused prior successful buyers exist, pass null.
      // Using the entity owner as referrer would fail with InvalidReferrer
      // because the owner is also the seller.
      let referrer: string | null;
      const pool = [...unusedReferrers].filter(addr => addr !== plan.buyerAddress);
      if (pool.length === 0) {
        referrer = null;
        log('ref', `  [${seq}] Referrer: null — no unused eligible referrer (used=${initialReferrerCount + succeeded - unusedReferrers.size}, remaining=${unusedReferrers.size})`);
      } else {
        referrer = randomElement(pool);
        unusedReferrers.delete(referrer);
        log('ref', `  [${seq}] Referrer: ${referrer.slice(0, 14)}... (used=${initialReferrerCount + succeeded - unusedReferrers.size}, remaining=${unusedReferrers.size}, candidates=${pool.length})`);
      }

      // 5d. Build & submit transaction
      log('tx', `  [${seq}] Building placeOrder tx ...`);
      log('tx', `  [${seq}]   product_id=${plan.productId} quantity=${ORDER_QTY} paymentAsset=Native(null) referrer=${referrer ? referrer.slice(0, 14) + '...' : 'null'}`);
      log('tx', `  [${seq}]   lock≈${formatNex(lockNex)} transferable≈${formatNex(balanceSnapshot.transferableEstimate)} required≈${formatNex(requiredNative)}`);

      try {
        let maxNexOpt: unknown = null;
        try {
          maxNexOpt = api.registry.createType('Option<Balance>', requiredNative.toString());
        } catch {
          maxNexOpt = null;
        }

        const tx = (api.tx as any).entityTransaction.placeOrder(
          plan.productId,   // product_id
          ORDER_QTY,        // quantity
          null,             // shipping_cid
          null,             // use_tokens
          null,             // payment_asset
          null,             // note_cid
          referrer,         // referrer
          maxNexOpt,        // max_nex_amount — slippage cap aligned with preflight
          null,             // max_token_amount
        );

        log('tx', `  [${seq}] Submitting tx (signer=${plan.buyerName}) ...`);
        const receipt: TxReceipt = await submitTx(api, tx, pair, `order-${plan.index}`);

        if (receipt.success) {
          succeeded++;
          succeededBuyers.add(plan.buyerAddress);
          unusedReferrers.add(plan.buyerAddress);

          // Log transaction result
          log('ok', `  [${seq}] TX SUCCESS`);
          log('ok', `  [${seq}]   txHash:    ${receipt.txHash}`);
          log('ok', `  [${seq}]   blockHash: ${receipt.blockHash ?? 'n/a'}`);
          log('ok', `  [${seq}]   extIndex:  ${receipt.extrinsicIndex ?? 'n/a'}`);

          // Log emitted events
          if (receipt.events.length > 0) {
            log('event', `  [${seq}]   Events (${receipt.events.length}):`);
            for (const ev of receipt.events) {
              log('event', `  [${seq}]     ${ev.section}.${ev.method} ${JSON.stringify(ev.data)}`);
            }
          }

          // Query post-order balance
          const postBalance = await readFreeBalance(api, plan.buyerAddress);
          const spent = buyerBalance - postBalance;
          log('ok', `  [${seq}]   Spent: ${formatNex(spent)}, Remaining: ${formatNex(postBalance)}`);
          log('ok', `  [${seq}]   Elapsed: ${elapsed(orderStart)}`);
        } else {
          failed++;
          errors.push({ index: plan.index, buyer: plan.buyerName, product: plan.productName, error: receipt.error ?? 'unknown' });
          log('fail', `  [${seq}] TX FAILED: ${receipt.error}`);
          if ((receipt.error ?? '').includes('escrow.Insufficient')) {
            log('fail', `  [${seq}]   Native escrow lock failed — check frozen/reserved, fee, or rate move vs preflight.`);
            log('fail', `  [${seq}]   ${formatBalanceSnapshot(balanceSnapshot)}`);
            log('fail', `  [${seq}]   required≈${formatNex(requiredNative)} lock≈${formatNex(lockNex)} rate=${nexUsdtPrice.toString()} (${priceSource})`);
          }
          if (receipt.blockHash) {
            log('fail', `  [${seq}]   blockHash: ${receipt.blockHash}`);
          }
          log('fail', `  [${seq}]   Elapsed: ${elapsed(orderStart)}`);
        }
      } catch (err: any) {
        failed++;
        errors.push({ index: plan.index, buyer: plan.buyerName, product: plan.productName, error: err.message });
        log('error', `  [${seq}] EXCEPTION: ${err.message}`);
        log('error', `  [${seq}]   Elapsed: ${elapsed(orderStart)}`);
      }

      // Brief pause every 10 orders to let the node breathe
      if ((i + 1) % 10 === 0 && i + 1 < orderPlans.length) {
        log('exec', `  Processed ${i + 1}/${orderPlans.length}, pausing 300ms ...`);
        await sleep(300);
      }
    }

    // ── 6. Summary ──────────────────────────────────────────────────────
    const totalElapsed = elapsed(scriptStart);

    console.log(`\n${'='.repeat(70)}`);
    console.log('  RESULTS');
    console.log(`${'='.repeat(70)}`);
    console.log(`  Entity:          ${ENTITY_ID}`);
    console.log(`  Shop:            ${shopId}`);
    console.log(`  Total accounts:  ${accounts.length} (${fundedCount} funded, ${zeroCount} zero)`);
    console.log(`  Total planned:   ${orderPlans.length}`);
    console.log(`  Succeeded:       ${succeeded}`);
    console.log(`  Failed:          ${failed}`);
    console.log(`  Success rate:    ${orderPlans.length > 0 ? ((succeeded / orderPlans.length) * 100).toFixed(1) : 0}%`);
    console.log(`  Products used:   ${productDist.size}`);
    console.log(`  Unique buyers:   ${buyerDist.size}`);
    console.log(`  Referrers used:  ${initialReferrerCount + succeeded - unusedReferrers.size}`);
    console.log(`  Referrers left:  ${unusedReferrers.size}`);
    console.log(`  Total elapsed:   ${totalElapsed}`);

    if (errors.length > 0) {
      console.log(`\n  Error details:`);
      console.log(`  ${'#'.padEnd(5)} ${'Buyer'.padEnd(16)} ${'Product'.padEnd(20)} Error`);
      console.log(`  ${'-'.repeat(65)}`);
      for (const e of errors) {
        console.log(`  ${String(e.index).padEnd(5)} ${e.buyer.padEnd(16)} ${e.product.padEnd(20)} ${e.error}`);
      }

      console.log(`\n  Error summary:`);
      const errorGroups = new Map<string, number[]>();
      for (const e of errors) {
        const group = errorGroups.get(e.error) ?? [];
        group.push(e.index);
        errorGroups.set(e.error, group);
      }
      for (const [msg, indices] of errorGroups.entries()) {
        console.log(`    [${indices.length}x] ${msg}`);
        if (indices.length <= 5) {
          console.log(`         orders: ${indices.join(', ')}`);
        } else {
          console.log(`         orders: ${indices.slice(0, 5).join(', ')} ... +${indices.length - 5} more`);
        }
      }
    }
    console.log(`${'='.repeat(70)}\n`);

  } finally {
    await disconnectApi(api);
    log('cleanup', 'Disconnected');
  }
}

main().catch((err) => {
  console.error('脚本执行失败 / Script failed:', err);
  process.exit(1);
});
