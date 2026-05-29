#!/usr/bin/env tsx
/**
 * NEX 场外批量自动买卖脚本 / Bulk NEX OTC auto trade script
 *
 * 卖家挂卖单 → 买家吃单 → 买家 confirmPayment → 卖家 sellerConfirmReceived 完结。
 * 买卖双方四步链上交易均由脚本自动签名提交，不等待 TRON USDT/TRX 到账或 OCW 校验。
 * Seller/buyer extrinsics are auto-signed; no off-chain USDT/TRX arrival wait.
 *
 * 默认 100 笔 × 1000 NEX（runtime MinOrderNexAmount）。TRON 地址仅写入订单元数据。
 *
 * Usage / 用法:
 *   node --import tsx e2e/mytests/nex-bulk-otc-100-trades.ts
 *   node --import tsx e2e/mytests/nex-bulk-otc-100-trades.ts --dry-run --count 5
 *   node --import tsx e2e/mytests/nex-bulk-otc-100-trades.ts --yes --count 100
 *
 * Environment / 环境变量:
 *   WS_URL              — WebSocket（默认 wss://rpc.nexusmall.net）
 *   SELLER_MNEMONIC     — 卖 NEX 收 USDT 的 Substrate 助记词
 *   BUYER_MNEMONIC      — 买 NEX 付 USDT 的 Substrate 助记词
 *   SELLER_TRON_ADDRESS — 收 USDT 的 TRON 地址
 *   BUYER_TRON_ADDRESS  — 付 USDT 的 TRON 地址
 *   TRADE_COUNT         — 笔数（默认 100）
 *   TRADE_NEX           — 每笔 NEX 数量（默认 1000，不低于链上最低挂单量）
 *   USDT_PRICE          — 覆盖 micro-USDT/NEX 价格（默认读链上 LastTradePrice 等）
 *   TRADE_INTERVAL_MS   — 每笔间隔毫秒（默认 2000）
 */

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline/promises';
import { stdin as input, stdout as output } from 'node:process';
import dotenv from 'dotenv';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
dotenv.config({ path: path.resolve(SCRIPT_DIR, '.env') });
dotenv.config({ path: path.resolve(SCRIPT_DIR, '../../../.env') });

process.env.WS_URL ??= 'wss://rpc.nexusmall.net';

import { Keyring } from '@polkadot/keyring';
import type { KeyringPair } from '@polkadot/keyring/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { assertTxSuccess } from '../framework/assert.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { NEX_PLANCK, formatNex, nex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  Default accounts (override via env)                                */
/* ------------------------------------------------------------------ */

const DEFAULT_SELLER_MNEMONIC = 'project fish scan stock dawn garage quick plate cannon join creek bird';
const DEFAULT_BUYER_MNEMONIC = 'enemy midnight panic critic six oblige pond soup lobster copy choose cousin';
const DEFAULT_SELLER_TRON = 'TFgRWMN4fjA9yq8S6afHDjGVQpfRTxuk8s';
const DEFAULT_BUYER_TRON = 'TREfnTGUMUSzWdodyZ3aucuv8ZEfWeaJWr';

function resolveMnemonic(envName: string, fallback: string): string {
  const raw = process.env[envName]?.trim();
  if (!raw) return fallback;
  const words = raw.split(/\s+/).filter(Boolean);
  if (words.length < 12) return fallback;
  return raw;
}

const SELLER_MNEMONIC = resolveMnemonic('SELLER_MNEMONIC', DEFAULT_SELLER_MNEMONIC);
const BUYER_MNEMONIC = resolveMnemonic('BUYER_MNEMONIC', DEFAULT_BUYER_MNEMONIC);
const SELLER_TRON_ADDRESS = (process.env.SELLER_TRON_ADDRESS ?? DEFAULT_SELLER_TRON).trim();
const BUYER_TRON_ADDRESS = (process.env.BUYER_TRON_ADDRESS ?? DEFAULT_BUYER_TRON).trim();

const USDT_PRICE_OVERRIDE = process.env.USDT_PRICE ? Number(process.env.USDT_PRICE) : null;

const MIN_ORDER_NEX = 1_000n * NEX_PLANCK;
const NATIVE_FEE_BUFFER = 10_000_000_000_000n;

interface CliOptions {
  dryRun: boolean;
  yes: boolean;
  tradeCount: number;
  tradeNex: bigint;
  intervalMs: number;
}

interface TradeResult {
  index: number;
  success: boolean;
  orderId?: number;
  tradeId?: number;
  error?: string;
  elapsedMs: number;
}

function parseCli(argv: string[]): CliOptions {
  let dryRun = false;
  let yes = false;
  let tradeCount = coerceNumber(process.env.TRADE_COUNT) ?? 100;
  let tradeNex = nex(coerceNumber(process.env.TRADE_NEX) ?? 1_000);
  let intervalMs = coerceNumber(process.env.TRADE_INTERVAL_MS) ?? 2_000;

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dry-run') {
      dryRun = true;
    } else if (a === '--yes') {
      yes = true;
    } else if (a === '--count' && argv[i + 1]) {
      tradeCount = Number(argv[++i]);
    } else if (a === '--nex' && argv[i + 1]) {
      tradeNex = nex(Number(argv[++i]));
    } else if (a === '--interval-ms' && argv[i + 1]) {
      intervalMs = Number(argv[++i]);
    }
  }

  if (!Number.isFinite(tradeCount) || tradeCount < 1) {
    throw new Error('--count / TRADE_COUNT 须为正整数');
  }
  if (tradeNex < MIN_ORDER_NEX) {
    throw new Error(`每笔 NEX 不得低于链上最低 ${formatNex(MIN_ORDER_NEX)}，当前 ${formatNex(tradeNex)}`);
  }
  if (!Number.isFinite(intervalMs) || intervalMs < 0) {
    throw new Error('--interval-ms / TRADE_INTERVAL_MS 须为非负整数');
  }

  return { dryRun, yes, tradeCount, tradeNex, intervalMs };
}

function log(tag: string, msg: string): void {
  const ts = new Date().toISOString().slice(11, 23);
  console.log(`[${ts}] [${tag}] ${msg}`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function normalizeEnum(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const keys = Object.keys(value as Record<string, unknown>);
    return keys[0] ?? String(value);
  }
  return String(value ?? '');
}

function readNumericEventField(data: unknown, ...candidates: string[]): number | undefined {
  const direct = coerceNumber(readObjectField(data, ...candidates));
  if (direct != null) return direct;
  if (Array.isArray(data)) {
    for (const item of data) {
      const parsed = coerceNumber(item);
      if (parsed != null) return parsed;
    }
  }
  return undefined;
}

function findEventData(
  events: { section: string; method: string; data: unknown }[],
  section: string,
  method: string,
): unknown {
  return events.find((e) => e.section === section && e.method === method)?.data;
}

async function queryOrder(api: ApiPromise, orderId: number): Promise<Record<string, unknown> | null> {
  const raw = await (api.query as any).nexMarket.orders(orderId);
  if ((raw as any).isNone) return null;
  return codecToJson<Record<string, unknown>>((raw as any).unwrap());
}

async function queryTrade(api: ApiPromise, tradeId: number): Promise<Record<string, unknown> | null> {
  const raw = await (api.query as any).nexMarket.usdtTrades(tradeId);
  if ((raw as any).isNone) return null;
  return codecToJson<Record<string, unknown>>((raw as any).unwrap());
}

async function readChainMinOrderNex(api: ApiPromise): Promise<bigint> {
  try {
    const constants = (api.consts as any).nexMarket;
    const raw = constants?.minOrderNexAmount ?? constants?.MinOrderNexAmount;
    if (raw) {
      const v = BigInt(raw.toString());
      if (v > 0n) return v;
    }
  } catch {
    /* fall through */
  }
  return MIN_ORDER_NEX;
}

async function resolveUsdtPrice(api: ApiPromise): Promise<number> {
  if (USDT_PRICE_OVERRIDE != null) {
    if (!Number.isFinite(USDT_PRICE_OVERRIDE) || USDT_PRICE_OVERRIDE <= 0) {
      throw new Error(`Invalid USDT_PRICE: ${USDT_PRICE_OVERRIDE}`);
    }
    return USDT_PRICE_OVERRIDE;
  }

  const lastTradePriceRaw = await (api.query as any).nexMarket.lastTradePrice();
  const bestAskRaw = await (api.query as any).nexMarket.bestAsk();
  const bestBidRaw = await (api.query as any).nexMarket.bestBid();

  const lastTradePrice = lastTradePriceRaw.isSome ? Number(lastTradePriceRaw.unwrap().toString()) : 0;
  const bestAsk = bestAskRaw.isSome ? Number(bestAskRaw.unwrap().toString()) : 0;
  const bestBid = bestBidRaw.isSome ? Number(bestBidRaw.unwrap().toString()) : 0;

  log('price', `LastTradePrice=${lastTradePrice || '(none)'} BestAsk=${bestAsk || '(none)'} BestBid=${bestBid || '(none)'}`);

  if (lastTradePrice > 0) return lastTradePrice;
  if (bestAsk > 0) return bestAsk;
  if (bestBid > 0) return bestBid;

  throw new Error(
    '链上无可用 NEX/USDT 价格，请设置 USDT_PRICE（micro-USDT per NEX，例如 10000 = 0.01 USDT/NEX）',
  );
}

async function cancelOpenSellOrders(api: ApiPromise, seller: KeyringPair, dryRun: boolean): Promise<void> {
  const raw = await (api.query as any).nexMarket.userOrders(seller.address);
  const orderIds = codecToJson<unknown[]>(raw);
  const ids = Array.isArray(orderIds)
    ? orderIds.map((id) => coerceNumber(id)).filter((id): id is number => id != null)
    : [];

  for (const orderId of ids) {
    const order = await queryOrder(api, orderId);
    if (!order) continue;
    const maker = String(readObjectField(order, 'maker') ?? '');
    const side = normalizeEnum(readObjectField(order, 'side'));
    const status = normalizeEnum(readObjectField(order, 'status'));
    if (maker !== seller.address || side !== 'Sell') continue;
    if (status !== 'Open' && status !== 'PartiallyFilled') continue;

    log('cancel', `cancel sell order #${orderId} status=${status}`);
    if (dryRun) continue;

    const tx = (api.tx as any).nexMarket.cancelOrder(orderId);
    const receipt = await submitTx(api, tx, seller, `cancel order ${orderId}`);
    assertTxSuccess(receipt, `cancel order ${orderId} should succeed`);
  }
}

async function placeSellOrder(
  api: ApiPromise,
  seller: KeyringPair,
  nexAmount: bigint,
  usdtPrice: number,
  dryRun: boolean,
): Promise<number> {
  log('sell', `placeSellOrder amount=${formatNex(nexAmount)} price=${usdtPrice} (${usdtPrice / 1_000_000} USDT/NEX) tron=${SELLER_TRON_ADDRESS}`);
  if (dryRun) return -1;

  const tx = (api.tx as any).nexMarket.placeSellOrder(
    nexAmount.toString(),
    usdtPrice,
    SELLER_TRON_ADDRESS,
    null,
  );
  const receipt = await submitTx(api, tx, seller, 'place sell order');
  assertTxSuccess(receipt, 'place sell order should succeed');

  const created = findEventData(receipt.events, 'nexMarket', 'OrderCreated');
  const eventOrderId = readNumericEventField(created, 'order_id', 'orderId');
  if (eventOrderId != null) return eventOrderId;

  const nextOrderId = await (api.query as any).nexMarket.nextOrderId();
  return Number(nextOrderId.toString()) - 1;
}

async function reserveSellOrder(
  api: ApiPromise,
  buyer: KeyringPair,
  orderId: number,
  dryRun: boolean,
): Promise<number> {
  log('buy', `reserveSellOrder orderId=${orderId} buyerTron=${BUYER_TRON_ADDRESS}`);
  if (dryRun) return -1;

  const tx = (api.tx as any).nexMarket.reserveSellOrder(orderId, null, BUYER_TRON_ADDRESS);
  const receipt = await submitTx(api, tx, buyer, `reserve sell order ${orderId}`);
  assertTxSuccess(receipt, 'reserve sell order should succeed');

  const tradeCreated = findEventData(receipt.events, 'nexMarket', 'UsdtTradeCreated');
  let tradeId = readNumericEventField(tradeCreated, 'trade_id', 'tradeId');
  if (tradeId == null) {
    const nextTradeId = await (api.query as any).nexMarket.nextUsdtTradeId();
    tradeId = Number(nextTradeId.toString()) - 1;
  }
  return tradeId;
}

async function confirmPayment(api: ApiPromise, buyer: KeyringPair, tradeId: number, dryRun: boolean): Promise<void> {
  log('pay', `confirmPayment tradeId=${tradeId}`);
  if (dryRun) return;

  const tx = (api.tx as any).nexMarket.confirmPayment(tradeId);
  const receipt = await submitTx(api, tx, buyer, `confirm payment ${tradeId}`);
  assertTxSuccess(receipt, 'confirm payment should succeed');
}

async function sellerConfirmReceived(
  api: ApiPromise,
  seller: KeyringPair,
  tradeId: number,
  dryRun: boolean,
): Promise<void> {
  log('settle', `sellerConfirmReceived tradeId=${tradeId}`);
  if (dryRun) return;

  const tx = (api.tx as any).nexMarket.sellerConfirmReceived(tradeId);
  const receipt = await submitTx(api, tx, seller, `seller confirm received ${tradeId}`);
  assertTxSuccess(receipt, 'seller confirm received should succeed');
}

async function executeOneTrade(
  api: ApiPromise,
  seller: KeyringPair,
  buyer: KeyringPair,
  index: number,
  total: number,
  nexAmount: bigint,
  usdtPrice: number,
  dryRun: boolean,
): Promise<{ orderId: number; tradeId: number }> {
  const label = `${index}/${total}`;
  log('round', `━━━ Trade ${label} ━━━`);

  await cancelOpenSellOrders(api, seller, dryRun);

  const orderId = await placeSellOrder(api, seller, nexAmount, usdtPrice, dryRun);
  const tradeId = await reserveSellOrder(api, buyer, orderId, dryRun);
  await confirmPayment(api, buyer, tradeId, dryRun);
  await sellerConfirmReceived(api, seller, tradeId, dryRun);

  if (!dryRun) {
    const trade = await queryTrade(api, tradeId);
    const status = normalizeEnum(readObjectField(trade, 'status'));
    if (status !== 'Completed') {
      throw new Error(`Trade ${tradeId} expected Completed, got ${status}`);
    }
  }

  log('round', `Trade ${label} done orderId=${orderId} tradeId=${tradeId}`);
  return { orderId, tradeId };
}

async function confirmRun(options: CliOptions, usdtPrice: number, minOrderNex: bigint): Promise<void> {
  if (options.dryRun || options.yes) return;

  const totalNex = options.tradeNex * BigInt(options.tradeCount);
  const estUsdt = Number(totalNex / NEX_PLANCK) * usdtPrice / 1_000_000;

  console.log('');
  console.log('─'.repeat(70));
  console.log('  即将执行批量 NEX OTC 交易 / Bulk NEX OTC trades');
  console.log(`  笔数 / count:        ${options.tradeCount}`);
  console.log(`  每笔 / per trade:    ${formatNex(options.tradeNex)}`);
  console.log(`  合计 NEX / total:    ${formatNex(totalNex)}`);
  console.log(`  价格 / price:        ${usdtPrice / 1_000_000} USDT/NEX (raw ${usdtPrice})`);
  console.log(`  估算 USDT / est:     ~${estUsdt.toLocaleString()} USDT（链下不校验到账，双方自动签名确认）`);
  console.log(`  最低挂单 / min:      ${formatNex(minOrderNex)}`);
  console.log(`  卖家 TRON 收 USDT:   ${SELLER_TRON_ADDRESS}`);
  console.log(`  买家 TRON 付 USDT:   ${BUYER_TRON_ADDRESS}`);
  console.log(`  确认方式 / confirm:  买家 confirmPayment + 卖家 sellerConfirmReceived（全自动）`);
  console.log(`  节点 / WS:           ${process.env.WS_URL}`);
  console.log('─'.repeat(70));

  const rl = createInterface({ input, output });
  const answer = (await rl.question('\n  确认执行？输入 yes 继续 / Type yes to continue: ')).trim().toLowerCase();
  rl.close();
  if (answer !== 'yes') {
    throw new Error('已取消 / Aborted by user');
  }
}

async function main(): Promise<void> {
  const options = parseCli(process.argv.slice(2));
  const scriptStart = Date.now();

  console.log(`${'='.repeat(70)}`);
  console.log(`  NEX 批量 OTC 自动买卖 / Bulk NEX OTC Auto Trade`);
  console.log(`${'='.repeat(70)}\n`);

  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const seller = keyring.addFromMnemonic(SELLER_MNEMONIC);
  const buyer = keyring.addFromMnemonic(BUYER_MNEMONIC);

  log('init', `ws=${process.env.WS_URL}`);
  log('init', `seller=${seller.address}`);
  log('init', `buyer=${buyer.address}`);
  log('init', `sellerTron=${SELLER_TRON_ADDRESS}`);
  log('init', `buyerTron=${BUYER_TRON_ADDRESS}`);
  log('init', `count=${options.tradeCount} nex=${formatNex(options.tradeNex)} intervalMs=${options.intervalMs} autoSign=true dryRun=${options.dryRun}`);

  const api = await connectApi();
  try {
    const minOrderNex = await readChainMinOrderNex(api);
    if (options.tradeNex < minOrderNex) {
      throw new Error(`TRADE_NEX ${formatNex(options.tradeNex)} < 链上 MinOrderNexAmount ${formatNex(minOrderNex)}`);
    }

    const usdtPrice = await resolveUsdtPrice(api);
    log('price', `using ${usdtPrice} micro-USDT/NEX (${usdtPrice / 1_000_000} USDT/NEX)`);

    const sellerBalance = await readFreeBalance(api, seller.address);
    const buyerBalance = await readFreeBalance(api, buyer.address);
    const requiredSeller = options.tradeNex * BigInt(options.tradeCount) + NATIVE_FEE_BUFFER * BigInt(options.tradeCount);

    log('balance', `seller=${formatNex(sellerBalance)} buyer=${formatNex(buyerBalance)}`);
    log('balance', `seller required≈${formatNex(requiredSeller)} (${options.tradeCount}×${formatNex(options.tradeNex)} + fee buffer)`);

    if (sellerBalance < requiredSeller) {
      throw new Error(`卖家余额不足 / Seller balance too low: ${formatNex(sellerBalance)} < ${formatNex(requiredSeller)}`);
    }
    if (buyerBalance <= 0n) {
      throw new Error('买家余额为 0，无法支付保证金与手续费 / Buyer balance is zero');
    }

    await confirmRun(options, usdtPrice, minOrderNex);

    const results: TradeResult[] = [];
    let succeeded = 0;
    let failed = 0;

    for (let i = 1; i <= options.tradeCount; i++) {
      const roundStart = Date.now();
      try {
        const { orderId, tradeId } = await executeOneTrade(
          api,
          seller,
          buyer,
          i,
          options.tradeCount,
          options.tradeNex,
          usdtPrice,
          options.dryRun,
        );
        succeeded++;
        results.push({
          index: i,
          success: true,
          orderId,
          tradeId,
          elapsedMs: Date.now() - roundStart,
        });
      } catch (err) {
        failed++;
        const message = err instanceof Error ? err.message : String(err);
        log('fail', `Trade ${i}/${options.tradeCount} failed: ${message}`);
        results.push({
          index: i,
          success: false,
          error: message,
          elapsedMs: Date.now() - roundStart,
        });
      }

      if (i < options.tradeCount && options.intervalMs > 0) {
        await sleep(options.intervalMs);
      }
    }

    const sellerFinal = await readFreeBalance(api, seller.address);
    const buyerFinal = await readFreeBalance(api, buyer.address);
    const elapsedSec = ((Date.now() - scriptStart) / 1000).toFixed(1);

    console.log(`\n${'='.repeat(70)}`);
    console.log('  RESULTS / 结果');
    console.log(`${'='.repeat(70)}`);
    console.log(`  Planned:     ${options.tradeCount}`);
    console.log(`  Succeeded:   ${succeeded}`);
    console.log(`  Failed:      ${failed}`);
    console.log(`  Per trade:   ${formatNex(options.tradeNex)}`);
    console.log(`  Auto sign:   buyer confirmPayment + seller sellerConfirmReceived`);
    console.log(`  Dry run:     ${options.dryRun}`);
    console.log(`  Elapsed:     ${elapsedSec}s`);
    console.log(`  Seller bal:  ${formatNex(sellerBalance)} -> ${formatNex(sellerFinal)} (Δ ${formatNex(sellerFinal - sellerBalance)})`);
    console.log(`  Buyer bal:   ${formatNex(buyerBalance)} -> ${formatNex(buyerFinal)} (Δ ${formatNex(buyerFinal - buyerBalance)})`);

    const failures = results.filter((r) => !r.success);
    if (failures.length > 0) {
      console.log('\n  Failures:');
      for (const f of failures) {
        console.log(`    #${f.index}: ${f.error}`);
      }
    }
    console.log(`${'='.repeat(70)}\n`);

    if (failed > 0 && !options.dryRun) {
      process.exitCode = 1;
    }
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Bulk OTC trade failed:', err instanceof Error ? err.stack ?? err.message : err);
  process.exit(1);
});
