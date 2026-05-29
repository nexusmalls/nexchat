#!/usr/bin/env tsx
/**
 * NEX 四档卖单 + 四档买单自动挂撤机器人 / Quad-tier sell + buy order bot
 *
 * 卖家：四笔卖单，末档为链上允许最高价，其余为参考价 × 110%/115%/118%（可配置）。
 * 买家：四笔买单，首档为链上允许最低价，其余为参考价 × 90%/95%/100%（可配置）。
 * 始终维持 4 卖 + 4 买；约束：最高买价 < 最低卖价。
 * Always maintain 4 sell + 4 buy. One sell at max allowed price, one buy at min allowed price.
 *
 * - 每 30 分钟触发一轮按档撤挂；档与档之间间隔 2 分钟（撤单 → 挂单 → 确认后再等 2 分钟处理下一档）
 * - 任何错误只记录日志，不中断主循环 / Errors are logged and never stop the bot
 * - 无限循环 / Runs forever until SIGINT/SIGTERM
 *
 * Usage / 用法:
 *   node --import tsx e2e/mytests/nex-dual-tier-sell-bot.ts
 *   node --import tsx e2e/mytests/nex-dual-tier-sell-bot.ts --dry-run --once
 *
 * Environment / 环境变量:
 *   WS_URL               — 默认 wss://rpc.nexusmall.net
 *   SELLER_MNEMONIC      — 卖家 Substrate 助记词
 *   SELLER_TRON_ADDRESS  — 收 USDT 的 TRON 地址
 *   BUYER_MNEMONIC       — 买家 Substrate 助记词
 *   BUYER_TRON_ADDRESS   — 付 USDT 的 TRON 地址
 *   RANDOM_ORDER_NEX_MIN  — 随机挂单下限 NEX（默认 10000000）
 *   RANDOM_ORDER_NEX_MAX  — 随机挂单上限 NEX（默认 100000000）
 *   REFRESH_INTERVAL_MS  — 定时撤挂周期（默认 30 分钟）
 *   TIER_RECONCILE_INTERVAL_MS — 每档撤挂完成后等待下一档（默认 2 分钟）
 *   WS_RECONNECT_BACKOFF_MS — WebSocket 断线后重连等待（默认 5 秒）
 *   POLL_INTERVAL_MS     — 轮询间隔（默认 60 秒）
 *   PRICE_TIER_1_BPS     — 卖单第 1 档倍数 bps（默认 11000 = 110%）
 *   PRICE_TIER_2_BPS     — 卖单第 2 档倍数 bps（默认 11500 = 115%）
 *   PRICE_TIER_3_BPS     — 卖单第 3 档倍数 bps（默认 11800 = 118%）
 *   BUY_PRICE_TIER_1_BPS — 买单第 2 档倍数 bps（默认 9000 = 90%，第 1 档为允许最低价）
 *   BUY_PRICE_TIER_2_BPS — 买单第 3 档倍数 bps（默认 9500 = 95%）
 *   BUY_PRICE_TIER_3_BPS — 买单第 4 档倍数 bps（默认 10000 = 100%）
 *   PRICE_MATCH_BPS      — 判定“同一档”价格容差 bps（默认 50）
 *   DRY_RUN=1            — 只打印计划不发交易
 *   RUN_ONCE=1           — 跑一轮后退出（调试用）
 */

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import dotenv from 'dotenv';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_WS_URL = 'wss://rpc.nexusmall.net';
const wsUrlFromShell = process.env.WS_URL?.trim();
dotenv.config({ path: path.resolve(SCRIPT_DIR, '.env') });
dotenv.config({ path: path.resolve(SCRIPT_DIR, '../../../.env') });
process.env.WS_URL = wsUrlFromShell || DEFAULT_WS_URL;

import { Keyring } from '@polkadot/keyring';
import type { KeyringPair } from '@polkadot/keyring/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { NEX_PLANCK, asBigInt, formatNex, nex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

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

const REFRESH_INTERVAL_MS = Number(process.env.REFRESH_INTERVAL_MS ?? 30 * 60 * 1000);
const TIER_RECONCILE_INTERVAL_MS = Number(process.env.TIER_RECONCILE_INTERVAL_MS ?? 2 * 60 * 1000);
const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS ?? 60_000);
const ITERATION_ERROR_BACKOFF_MS = Number(process.env.ITERATION_ERROR_BACKOFF_MS ?? POLL_INTERVAL_MS);
const WS_RECONNECT_BACKOFF_MS = Number(process.env.WS_RECONNECT_BACKOFF_MS ?? 5_000);
const PRICE_TIER_1_BPS = BigInt(Number(process.env.PRICE_TIER_1_BPS ?? 11_000));
const PRICE_TIER_2_BPS = BigInt(Number(process.env.PRICE_TIER_2_BPS ?? 11_500));
const PRICE_TIER_3_BPS = BigInt(Number(process.env.PRICE_TIER_3_BPS ?? 11_800));
const BUY_PRICE_TIER_1_BPS = BigInt(Number(process.env.BUY_PRICE_TIER_1_BPS ?? 9_000));
const BUY_PRICE_TIER_2_BPS = BigInt(Number(process.env.BUY_PRICE_TIER_2_BPS ?? 9_500));
const BUY_PRICE_TIER_3_BPS = BigInt(Number(process.env.BUY_PRICE_TIER_3_BPS ?? 10_000));
const PRICE_MATCH_BPS = BigInt(Number(process.env.PRICE_MATCH_BPS ?? 50));
const RANDOM_ORDER_NEX_MIN = nex(Number(process.env.RANDOM_ORDER_NEX_MIN ?? 10_000_000));
const RANDOM_ORDER_NEX_MAX = nex(Number(process.env.RANDOM_ORDER_NEX_MAX ?? 100_000_000));
const BID_ASK_SPREAD = BigInt(Number(process.env.BID_ASK_SPREAD ?? 1));
const DRY_RUN = process.env.DRY_RUN === '1' || process.argv.includes('--dry-run');
const RUN_ONCE = process.env.RUN_ONCE === '1' || process.argv.includes('--once');

const MIN_ORDER_NEX = 1_000n * NEX_PLANCK;
const MAX_ORDER_NEX = 100_000_000n * NEX_PLANCK;
const NATIVE_FEE_BUFFER = 10_000_000_000_000n;

type PriceSource = 'twap1h' | 'initialPrice' | 'lastTradePrice';

interface ReconcileResult {
  lastRefreshAt: number;
  needsReconnect: boolean;
}

interface PriceProtectionContext {
  /** 链上 check_price_deviation 使用的参考价 / Reference used by on-chain deviation check */
  effectiveReference: bigint;
  effectiveSource: PriceSource;
  /** 市场提示价（TWAP 或 LastTrade，仅日志） / Market hint for logging */
  marketHint: bigint | null;
  maxDeviationBps: bigint;
  priceProtectionEnabled: boolean;
}

interface ActiveOrder {
  orderId: number;
  price: bigint;
  nexAmount: bigint;
  filledAmount: bigint;
  status: string;
  side: 'Sell' | 'Buy';
}

type TierPriceMode = 'max' | 'min' | 'multiplier';

interface OrderTier {
  label: string;
  side: 'Sell' | 'Buy';
  priceMode: TierPriceMode;
  multiplierBps?: bigint;
}

interface TierTarget {
  tier: OrderTier;
  price: bigint;
}

const SELL_TIERS: OrderTier[] = [
  { label: 'sell-tier1+10%', priceMode: 'multiplier', multiplierBps: PRICE_TIER_1_BPS, side: 'Sell' },
  { label: 'sell-tier2+15%', priceMode: 'multiplier', multiplierBps: PRICE_TIER_2_BPS, side: 'Sell' },
  { label: 'sell-tier3+18%', priceMode: 'multiplier', multiplierBps: PRICE_TIER_3_BPS, side: 'Sell' },
  { label: 'sell-max', priceMode: 'max', side: 'Sell' },
];

const BUY_TIERS: OrderTier[] = [
  { label: 'buy-min', priceMode: 'min', side: 'Buy' },
  { label: 'buy-tier2-10%', priceMode: 'multiplier', multiplierBps: BUY_PRICE_TIER_1_BPS, side: 'Buy' },
  { label: 'buy-tier3-5%', priceMode: 'multiplier', multiplierBps: BUY_PRICE_TIER_2_BPS, side: 'Buy' },
  { label: 'buy-tier4@ref', priceMode: 'multiplier', multiplierBps: BUY_PRICE_TIER_3_BPS, side: 'Buy' },
];

const REQUIRED_SELL_ORDERS = SELL_TIERS.length;
const REQUIRED_BUY_ORDERS = BUY_TIERS.length;

function formatTierSpec(tier: OrderTier): string {
  if (tier.priceMode === 'max') return `${tier.label}=maxAllowed`;
  if (tier.priceMode === 'min') return `${tier.label}=minAllowed`;
  return `${tier.label}=${tier.multiplierBps}bps`;
}

function randomOrderNex(): bigint {
  const min = Number(RANDOM_ORDER_NEX_MIN / NEX_PLANCK);
  const max = Number(RANDOM_ORDER_NEX_MAX / NEX_PLANCK);
  if (!Number.isFinite(min) || !Number.isFinite(max) || min < 1 || max < min) {
    log('warn', `invalid random order range ${min}..${max}, fallback to min=${RANDOM_ORDER_NEX_MIN}`);
    return RANDOM_ORDER_NEX_MIN;
  }
  const amountNex = min + Math.floor(Math.random() * (max - min + 1));
  return nex(amountNex);
}

function log(tag: string, msg: string): void {
  const ts = new Date().toISOString().slice(11, 23);
  console.log(`[${ts}] [${tag}] ${msg}`);
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.stack ?? error.message : String(error);
}

function logError(tag: string, error: unknown): void {
  log('error', `${tag}: ${formatError(error)}`);
}

function isWsDisconnectedError(error: unknown): boolean {
  const message = formatError(error).toLowerCase();
  return message.includes('websocket is not connected')
    || message.includes('connection closed')
    || message.includes('disconnected')
    || message.includes('socket has been closed')
    || message.includes('after sending request');
}

async function verifyApiConnection(api: ApiPromise): Promise<boolean> {
  if (!api.isConnected) {
    return false;
  }
  try {
    await api.rpc.system.health();
    return true;
  } catch (error) {
    if (isWsDisconnectedError(error)) {
      return false;
    }
    return api.isConnected;
  }
}

function logReceipt(receipt: TxReceipt): void {
  log('tx', `label=${receipt.label} success=${receipt.success} txHash=${receipt.txHash}`);
  if (receipt.error) log('tx', `error=${receipt.error}`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitBeforeNextTier(step: number, total: number, reason: string): Promise<void> {
  if (DRY_RUN || step >= total || TIER_RECONCILE_INTERVAL_MS <= 0) {
    return;
  }
  log('wait', `[${step}/${total}] ${reason}, next tier in ${TIER_RECONCILE_INTERVAL_MS}ms (${TIER_RECONCILE_INTERVAL_MS / 60_000} min)`);
  await sleep(TIER_RECONCILE_INTERVAL_MS);
}

function absBigInt(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function normalizeEnum(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const keys = Object.keys(value as Record<string, unknown>);
    return keys[0] ?? String(value);
  }
  return String(value ?? '');
}

function withinBps(actual: bigint, expected: bigint, toleranceBps: bigint): boolean {
  if (expected <= 0n) return actual === expected;
  return absBigInt(actual - expected) * 10_000n <= expected * toleranceBps;
}

function remainingOrderAmount(order: ActiveOrder): bigint {
  return order.nexAmount > order.filledAmount ? order.nexAmount - order.filledAmount : 0n;
}

function formatMicroUsdtPrice(raw: bigint): string {
  return `${raw.toString()} (${Number(raw) / 1_000_000} USDT/NEX)`;
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

async function readStorageMaybe(query: any, ...names: string[]): Promise<any | null> {
  for (const name of names) {
    const fn = query?.[name];
    if (typeof fn === 'function') {
      return await fn();
    }
  }
  return null;
}

async function readCurrentBlock(api: ApiPromise): Promise<bigint> {
  const block = await (api.query as any).system.number();
  return asBigInt(block.toString());
}

async function readTwap1h(api: ApiPromise): Promise<bigint | null> {
  const raw = await readStorageMaybe(
    (api.query as any).nexMarket,
    'twapAccumulator',
    'twapAccumulatorStore',
  );
  if (!raw || (raw as any).isNone) return null;

  const acc = codecToJson<Record<string, unknown>>((raw as any).unwrap());
  const hourSnapshot = readObjectField(acc, 'hourSnapshot', 'hour_snapshot');
  if (!hourSnapshot || typeof hourSnapshot !== 'object') return null;

  const currentBlock = await readCurrentBlock(api);
  const accCurrentBlock = asBigInt(readObjectField(acc, 'currentBlock', 'current_block'));
  const lastPrice = asBigInt(readObjectField(acc, 'lastPrice', 'last_price'));
  const currentCumulative = asBigInt(readObjectField(acc, 'currentCumulative', 'current_cumulative'));
  const snapshotBlock = asBigInt(readObjectField(hourSnapshot, 'blockNumber', 'block_number'));
  const snapshotCumulative = asBigInt(readObjectField(hourSnapshot, 'cumulativePrice', 'cumulative_price'));

  const blocksSince = currentBlock >= accCurrentBlock ? currentBlock - accCurrentBlock : 0n;
  const effectiveCumulative = currentCumulative + lastPrice * blocksSince;
  const blockDiff = currentBlock >= snapshotBlock ? currentBlock - snapshotBlock : 0n;

  if (blockDiff === 0n) {
    return lastPrice > 0n ? lastPrice : null;
  }

  const cumulativeDiff = effectiveCumulative >= snapshotCumulative
    ? effectiveCumulative - snapshotCumulative
    : 0n;
  const twap = cumulativeDiff / blockDiff;
  return twap > 0n ? twap : null;
}

async function readPriceProtectionConfig(api: ApiPromise): Promise<Record<string, unknown>> {
  const raw = await readStorageMaybe(
    (api.query as any).nexMarket,
    'priceProtectionStore',
    'priceProtection',
  );
  return raw ? codecToJson<Record<string, unknown>>(raw) : {};
}

async function isTwapDataSufficient(api: ApiPromise, minTradesForTwap: bigint): Promise<boolean> {
  const raw = await readStorageMaybe(
    (api.query as any).nexMarket,
    'twapAccumulatorStore',
    'twapAccumulator',
  );
  if (!raw || (raw as any).isNone) return false;

  const acc = codecToJson<Record<string, unknown>>((raw as any).unwrap());
  const tradeCount = asBigInt(readObjectField(acc, 'tradeCount', 'trade_count'));
  if (tradeCount < minTradesForTwap) return false;

  const currentBlock = Number(await readCurrentBlock(api));
  const constants = (api.consts as any).nexMarket ?? {};
  const blocksPerHour = Number(constants.blocksPerHour?.toString?.() ?? constants.BlocksPerHour?.toString?.() ?? 600);
  const blocksPerDay = Number(constants.blocksPerDay?.toString?.() ?? constants.BlocksPerDay?.toString?.() ?? 14400);
  const blocksPerWeek = Number(constants.blocksPerWeek?.toString?.() ?? constants.BlocksPerWeek?.toString?.() ?? 100800);

  const hourSnap = readObjectField(acc, 'hourSnapshot', 'hour_snapshot') as Record<string, unknown> | undefined;
  const daySnap = readObjectField(acc, 'daySnapshot', 'day_snapshot') as Record<string, unknown> | undefined;
  const weekSnap = readObjectField(acc, 'weekSnapshot', 'week_snapshot') as Record<string, unknown> | undefined;
  if (!hourSnap || !daySnap || !weekSnap) return false;

  const hourBlock = coerceNumber(readObjectField(hourSnap, 'blockNumber', 'block_number')) ?? 0;
  const dayBlock = coerceNumber(readObjectField(daySnap, 'blockNumber', 'block_number')) ?? 0;
  const weekBlock = coerceNumber(readObjectField(weekSnap, 'blockNumber', 'block_number')) ?? 0;

  return currentBlock - hourBlock >= blocksPerHour
    && currentBlock - dayBlock >= blocksPerDay
    && currentBlock - weekBlock >= blocksPerWeek;
}

async function readPriceProtectionContext(api: ApiPromise): Promise<PriceProtectionContext> {
  const config = await readPriceProtectionConfig(api);
  const enabled = readObjectField(config, 'enabled') !== false;
  const maxDeviationBps = BigInt(
    coerceNumber(readObjectField(config, 'maxPriceDeviation', 'max_price_deviation')) ?? 2000,
  );
  const minTradesForTwap = BigInt(
    coerceNumber(readObjectField(config, 'minTradesForTwap', 'min_trades_for_twap')) ?? 100,
  );
  const initialPrice = asBigInt(readObjectField(config, 'initialPrice', 'initial_price'));
  const twap1h = await readTwap1h(api);
  const twapSufficient = await isTwapDataSufficient(api, minTradesForTwap);

  let effectiveReference: bigint;
  let effectiveSource: PriceSource;
  if (twapSufficient && twap1h != null && twap1h > 0n) {
    effectiveReference = twap1h;
    effectiveSource = 'twap1h';
  } else if (initialPrice > 0n) {
    effectiveReference = initialPrice;
    effectiveSource = 'initialPrice';
  } else if (twap1h != null && twap1h > 0n) {
    effectiveReference = twap1h;
    effectiveSource = 'twap1h';
  } else {
    const lastTradePriceRaw = await readStorageMaybe((api.query as any).nexMarket, 'lastTradePrice');
    if (lastTradePriceRaw && (lastTradePriceRaw as any).isSome) {
      const last = asBigInt((lastTradePriceRaw as any).unwrap().toString());
      if (last > 0n) {
        effectiveReference = last;
        effectiveSource = 'lastTradePrice';
      } else {
        log('warn', 'No effective reference price available, fallback initialPrice=10');
        effectiveReference = 10n;
        effectiveSource = 'initialPrice';
      }
    } else {
      log('warn', 'No TWAP / initialPrice / lastTradePrice available, fallback initialPrice=10');
      effectiveReference = 10n;
      effectiveSource = 'initialPrice';
    }
  }

  return {
    effectiveReference,
    effectiveSource,
    marketHint: twap1h,
    maxDeviationBps,
    priceProtectionEnabled: enabled,
  };
}

async function queryOrder(api: ApiPromise, orderId: number): Promise<Record<string, unknown> | null> {
  const raw = await (api.query as any).nexMarket.orders(orderId);
  if ((raw as any).isNone) return null;
  return codecToJson<Record<string, unknown>>((raw as any).unwrap());
}

async function readOwnActiveOrders(
  api: ApiPromise,
  ownerAddress: string,
  side: 'Sell' | 'Buy',
): Promise<ActiveOrder[]> {
  const raw = await (api.query as any).nexMarket.userOrders(ownerAddress);
  const orderIds = codecToJson<unknown[]>(raw);
  const ids = Array.isArray(orderIds)
    ? orderIds.map((id) => coerceNumber(id)).filter((id): id is number => id != null)
    : [];

  const result: ActiveOrder[] = [];
  for (const orderId of ids) {
    const order = await queryOrder(api, orderId);
    if (!order) continue;
    const maker = String(readObjectField(order, 'maker') ?? '');
    const orderSide = normalizeEnum(readObjectField(order, 'side'));
    const status = normalizeEnum(readObjectField(order, 'status'));
    if (maker !== ownerAddress) continue;
    if (orderSide !== side) continue;
    if (status !== 'Open' && status !== 'PartiallyFilled') continue;

    result.push({
      orderId,
      price: asBigInt(readObjectField(order, 'usdtPrice', 'usdt_price')),
      nexAmount: asBigInt(readObjectField(order, 'nexAmount', 'nex_amount')),
      filledAmount: asBigInt(readObjectField(order, 'filledAmount', 'filled_amount')),
      status,
      side,
    });
  }
  return result;
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

function maxAllowedPrice(ctx: PriceProtectionContext): bigint {
  if (!ctx.priceProtectionEnabled) {
    return (ctx.effectiveReference * 12_000n) / 10_000n;
  }
  return ctx.effectiveReference + (ctx.effectiveReference * ctx.maxDeviationBps) / 10_000n;
}

function minAllowedPrice(ctx: PriceProtectionContext): bigint {
  if (!ctx.priceProtectionEnabled) {
    return (ctx.effectiveReference * 9_000n) / 10_000n;
  }
  return ctx.effectiveReference > (ctx.effectiveReference * ctx.maxDeviationBps) / 10_000n
    ? ctx.effectiveReference - (ctx.effectiveReference * ctx.maxDeviationBps) / 10_000n
    : 1n;
}

function clampPriceToProtection(price: bigint, ctx: PriceProtectionContext): bigint {
  if (!ctx.priceProtectionEnabled) {
    return price;
  }
  const maxPrice = maxAllowedPrice(ctx);
  const minPrice = minAllowedPrice(ctx);
  if (price > maxPrice) {
    return maxPrice;
  }
  if (price < minPrice) {
    return minPrice;
  }
  return price;
}

function computeTierPrice(
  referencePrice: bigint,
  multiplierBps: bigint,
  ctx: PriceProtectionContext,
): bigint {
  const requested = (referencePrice * multiplierBps) / 10_000n;
  if (requested <= 0n) {
    log('warn', `invalid tier price from ref=${referencePrice.toString()} bps=${multiplierBps.toString()}, fallback to ref`);
    return clampPriceToProtection(referencePrice, ctx);
  }
  const clamped = clampPriceToProtection(requested, ctx);
  if (clamped !== requested) {
    log('clamp', `requested=${requested.toString()} clamped to ${clamped.toString()} (maxDev=${ctx.maxDeviationBps.toString()}bps)`);
  }
  return clamped;
}

function resolveTierPrice(tier: OrderTier, ctx: PriceProtectionContext): bigint {
  if (tier.priceMode === 'max') {
    return maxAllowedPrice(ctx);
  }
  if (tier.priceMode === 'min') {
    return minAllowedPrice(ctx);
  }
  const multiplierBps = tier.multiplierBps ?? 10_000n;
  return computeTierPrice(ctx.effectiveReference, multiplierBps, ctx);
}

function buildTierTargets(tiers: OrderTier[], ctx: PriceProtectionContext): TierTarget[] {
  return tiers.map((tier) => ({
    tier,
    price: resolveTierPrice(tier, ctx),
  }));
}

function enforceBidAskSpread(
  sellTargets: TierTarget[],
  buyTargets: TierTarget[],
  ctx: PriceProtectionContext,
): void {
  if (sellTargets.length === 0 || buyTargets.length === 0) {
    return;
  }

  sellTargets.sort((a, b) => (a.price < b.price ? -1 : a.price > b.price ? 1 : 0));
  buyTargets.sort((a, b) => (a.price < b.price ? -1 : a.price > b.price ? 1 : 0));

  for (let i = 1; i < sellTargets.length; i++) {
    if (sellTargets[i].price <= sellTargets[i - 1].price) {
      sellTargets[i].price = clampPriceToProtection(sellTargets[i - 1].price + 1n, ctx);
    }
  }
  for (let i = buyTargets.length - 2; i >= 0; i--) {
    if (buyTargets[i].price >= buyTargets[i + 1].price) {
      buyTargets[i].price = buyTargets[i + 1].price > 1n
        ? clampPriceToProtection(buyTargets[i + 1].price - 1n, ctx)
        : buyTargets[i].price;
    }
  }

  let maxBuy = buyTargets.reduce((max, target) => (target.price > max ? target.price : max), buyTargets[0].price);
  let minSell = sellTargets.reduce((min, target) => (target.price < min ? target.price : min), sellTargets[0].price);

  if (maxBuy >= minSell) {
    log('spread', `overlap maxBuy=${maxBuy.toString()} minSell=${minSell.toString()}, adjusting`);

    const cappedMaxBuy = minSell > BID_ASK_SPREAD ? minSell - BID_ASK_SPREAD : 1n;
    for (const target of buyTargets) {
      if (target.price >= minSell) {
        const adjusted = clampPriceToProtection(cappedMaxBuy, ctx);
        log('spread', `${target.tier.label} buy ${target.price.toString()} -> ${adjusted.toString()}`);
        target.price = adjusted;
      }
    }

    maxBuy = buyTargets.reduce((max, target) => (target.price > max ? target.price : max), buyTargets[0].price);
    minSell = sellTargets.reduce((min, target) => (target.price < min ? target.price : min), sellTargets[0].price);

    if (maxBuy >= minSell) {
      const raisedMinSell = clampPriceToProtection(maxBuy + BID_ASK_SPREAD, ctx);
      log('spread', `${sellTargets[0].tier.label} sell ${minSell.toString()} -> ${raisedMinSell.toString()}`);
      sellTargets[0].price = raisedMinSell;

      for (let i = 1; i < sellTargets.length; i++) {
        if (sellTargets[i].price <= sellTargets[i - 1].price) {
          sellTargets[i].price = clampPriceToProtection(sellTargets[i - 1].price + 1n, ctx);
        }
      }
    }
  }

  maxBuy = buyTargets.reduce((max, target) => (target.price > max ? target.price : max), buyTargets[0].price);
  minSell = sellTargets.reduce((min, target) => (target.price < min ? target.price : min), sellTargets[0].price);

  if (maxBuy >= minSell) {
    log(
      'warn',
      `bid-ask spread enforce skipped: maxBuy=${maxBuy.toString()} minSell=${minSell.toString()}`,
    );
    return;
  }

  log(
    'spread',
    `ok sells=${sellTargets.map((t) => t.price.toString()).join('/')} `
    + `buys=${buyTargets.map((t) => t.price.toString()).join('/')} gap=${(minSell - maxBuy).toString()}`,
  );
}

function estimateBuyOrderDeposit(nexAmount: bigint): bigint {
  // Mirrors on-chain: deposit ≈ nex_amount × BuyerDepositRate (10% USDT value ≡ 10% NEX when priced in NEX)
  const buyerDepositRateBps = 1000n;
  return (nexAmount * buyerDepositRateBps) / 10_000n;
}

async function cancelOrphanOrders(
  api: ApiPromise,
  owner: KeyringPair,
  side: 'Sell' | 'Buy',
  processedOrderIds: Set<number>,
): Promise<void> {
  try {
    const activeOrders = await readOwnActiveOrders(api, owner.address, side);
    for (const order of activeOrders) {
      if (processedOrderIds.has(order.orderId)) continue;
      try {
        log('orphan', `cancel extra ${side.toLowerCase()} order #${order.orderId} price=${order.price.toString()}`);
        const cancelled = await cancelOrder(api, owner, order.orderId, side);
        if (!cancelled) {
          log('warn', `skip orphan ${side.toLowerCase()} order #${order.orderId}, cannot cancel`);
        }
      } catch (error) {
        logError(`orphan cancel #${order.orderId}`, error);
      }
    }
  } catch (error) {
    logError(`orphan scan ${side.toLowerCase()}`, error);
  }
}

function findTierOrder(orders: ActiveOrder[], targetPrice: bigint): ActiveOrder | undefined {
  return orders.find(
    (order) => remainingOrderAmount(order) > 0n && withinBps(order.price, targetPrice, PRICE_MATCH_BPS),
  );
}

async function cancelOrder(
  api: ApiPromise,
  owner: KeyringPair,
  orderId: number,
  side: 'Sell' | 'Buy',
): Promise<boolean> {
  log('cancel', `cancel ${side.toLowerCase()} order #${orderId}`);
  if (DRY_RUN) return true;
  try {
    const tx = (api.tx as any).nexMarket.cancelOrder(orderId);
    const receipt = await submitTx(api, tx, owner, `cancel ${side.toLowerCase()} order ${orderId}`);
    logReceipt(receipt);
    if (!receipt.success) {
      log('warn', `skip cancel ${side.toLowerCase()} order #${orderId}: ${receipt.error ?? 'tx failed'}`);
      return false;
    }
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    log('warn', `skip cancel ${side.toLowerCase()} order #${orderId}: ${message}`);
    return false;
  }
}

async function placeSellOrder(
  api: ApiPromise,
  seller: KeyringPair,
  nexAmount: bigint,
  usdtPrice: bigint,
  tierLabel: string,
): Promise<number> {
  log(
    'sell',
    `[${tierLabel}] placeSellOrder amount=${formatNex(nexAmount)} price=${formatMicroUsdtPrice(usdtPrice)} tron=${SELLER_TRON_ADDRESS}`,
  );
  if (DRY_RUN) return -1;

  if (usdtPrice > BigInt(Number.MAX_SAFE_INTEGER)) {
    log('warn', `[${tierLabel}] skip place sell: price exceeds JS safe integer ${usdtPrice.toString()}`);
    return -1;
  }

  try {
    const tx = (api.tx as any).nexMarket.placeSellOrder(
      nexAmount.toString(),
      Number(usdtPrice),
      SELLER_TRON_ADDRESS,
      null,
    );
    const receipt = await submitTx(api, tx, seller, `place sell order ${tierLabel}`);
    logReceipt(receipt);
    if (!receipt.success) {
      log('warn', `[${tierLabel}] skip place sell: ${receipt.error ?? 'tx failed'}`);
      return -1;
    }

    const created = findEventData(receipt.events, 'nexMarket', 'OrderCreated');
    const eventOrderId = readNumericEventField(created, 'order_id', 'orderId');
    if (eventOrderId != null) return eventOrderId;

    const nextOrderId = await (api.query as any).nexMarket.nextOrderId();
    return Number(nextOrderId.toString()) - 1;
  } catch (error) {
    logError(`place sell ${tierLabel}`, error);
    return -1;
  }
}

async function placeBuyOrder(
  api: ApiPromise,
  buyer: KeyringPair,
  nexAmount: bigint,
  usdtPrice: bigint,
  tierLabel: string,
): Promise<number> {
  log(
    'buy',
    `[${tierLabel}] placeBuyOrder amount=${formatNex(nexAmount)} price=${formatMicroUsdtPrice(usdtPrice)} tron=${BUYER_TRON_ADDRESS}`,
  );
  if (DRY_RUN) return -1;

  if (usdtPrice > BigInt(Number.MAX_SAFE_INTEGER)) {
    log('warn', `[${tierLabel}] skip place buy: price exceeds JS safe integer ${usdtPrice.toString()}`);
    return -1;
  }

  try {
    const tx = (api.tx as any).nexMarket.placeBuyOrder(
      nexAmount.toString(),
      Number(usdtPrice),
      BUYER_TRON_ADDRESS,
    );
    const receipt = await submitTx(api, tx, buyer, `place buy order ${tierLabel}`);
    logReceipt(receipt);
    if (!receipt.success) {
      log('warn', `[${tierLabel}] skip place buy: ${receipt.error ?? 'tx failed'}`);
      return -1;
    }

    const created = findEventData(receipt.events, 'nexMarket', 'OrderCreated');
    const eventOrderId = readNumericEventField(created, 'order_id', 'orderId');
    if (eventOrderId != null) return eventOrderId;

    const nextOrderId = await (api.query as any).nexMarket.nextOrderId();
    return Number(nextOrderId.toString()) - 1;
  } catch (error) {
    logError(`place buy ${tierLabel}`, error);
    return -1;
  }
}

async function placeTierOrder(
  api: ApiPromise,
  seller: KeyringPair,
  buyer: KeyringPair,
  tier: OrderTier,
  usdtPrice: bigint,
  nexAmount: bigint,
): Promise<number> {
  if (tier.side === 'Sell') {
    return placeSellOrder(api, seller, nexAmount, usdtPrice, tier.label);
  }
  return placeBuyOrder(api, buyer, nexAmount, usdtPrice, tier.label);
}

async function ensureAccountBalances(
  api: ApiPromise,
  seller: KeyringPair,
  buyer: KeyringPair,
  minOrderNex: bigint,
): Promise<boolean> {
  try {
    const sellerBalance = await readFreeBalance(api, seller.address);
    const buyerBalance = await readFreeBalance(api, buyer.address);
    const sellerRequired = RANDOM_ORDER_NEX_MAX * BigInt(REQUIRED_SELL_ORDERS) + NATIVE_FEE_BUFFER;
    const buyerRequired = estimateBuyOrderDeposit(RANDOM_ORDER_NEX_MAX) * BigInt(REQUIRED_BUY_ORDERS) + NATIVE_FEE_BUFFER;

    log('balance', `seller free=${formatNex(sellerBalance)} required≈${formatNex(sellerRequired)} randomMax=${formatNex(RANDOM_ORDER_NEX_MAX)}×${REQUIRED_SELL_ORDERS}`);
    log('balance', `buyer free=${formatNex(buyerBalance)} required≈${formatNex(buyerRequired)} depositMax=${formatNex(estimateBuyOrderDeposit(RANDOM_ORDER_NEX_MAX))}×${REQUIRED_BUY_ORDERS}`);

    if (RANDOM_ORDER_NEX_MIN < minOrderNex) {
      log('warn', `RANDOM_ORDER_NEX_MIN ${formatNex(RANDOM_ORDER_NEX_MIN)} < chain min ${formatNex(minOrderNex)}`);
      return false;
    }
    if (RANDOM_ORDER_NEX_MAX > MAX_ORDER_NEX) {
      log('warn', `RANDOM_ORDER_NEX_MAX ${formatNex(RANDOM_ORDER_NEX_MAX)} > chain max ${formatNex(MAX_ORDER_NEX)}`);
      return false;
    }
    if (RANDOM_ORDER_NEX_MIN > RANDOM_ORDER_NEX_MAX) {
      log('warn', 'RANDOM_ORDER_NEX_MIN > RANDOM_ORDER_NEX_MAX');
      return false;
    }
    if (sellerBalance < sellerRequired) {
      log('warn', `Seller balance too low: ${formatNex(sellerBalance)} < ${formatNex(sellerRequired)}`);
      return false;
    }
    if (buyerBalance < buyerRequired) {
      log('warn', `Buyer balance too low: ${formatNex(buyerBalance)} < ${formatNex(buyerRequired)}`);
      return false;
    }
    return true;
  } catch (error) {
    logError('balance check', error);
    return false;
  }
}

async function reconcileOneTier(
  api: ApiPromise,
  seller: KeyringPair,
  buyer: KeyringPair,
  ctx: PriceProtectionContext,
  target: TierTarget,
  dueRefresh: boolean,
  processedOrderIds: Set<number>,
  step: number,
  total: number,
): Promise<boolean> {
  try {
    const owner = target.tier.side === 'Sell' ? seller : buyer;
    const activeOrders = await readOwnActiveOrders(api, owner.address, target.tier.side);
    const available = activeOrders.filter((o) => !processedOrderIds.has(o.orderId));

    const pct = target.tier.priceMode === 'multiplier'
      ? Number(target.tier.multiplierBps ?? 10_000n) / 100 - 100
      : 0;
    const pctLabel = target.tier.priceMode === 'max'
      ? 'max allowed'
      : target.tier.priceMode === 'min'
        ? 'min allowed'
        : pct === 0
          ? 'at ref'
          : `${pct >= 0 ? '+' : ''}${pct.toFixed(0)}%`;
    log(
      'tier',
      `[${step}/${total}] ${target.tier.label} target=${formatMicroUsdtPrice(target.price)} (ref ${ctx.effectiveSource} ${pctLabel})`,
    );

    const matched = findTierOrder(available, target.price);

    if (matched && !dueRefresh) {
      processedOrderIds.add(matched.orderId);
      log(
        'tier',
        `[${step}/${total}] ${target.tier.label} keep order #${matched.orderId} price=${matched.price.toString()} remaining=${formatNex(remainingOrderAmount(matched))}`,
      );
      return false;
    }

    if (dueRefresh) {
      const toCancel = matched ?? available[0];
      if (toCancel) {
        log('tier', `[${step}/${total}] ${target.tier.label} refresh cancel order #${toCancel.orderId}`);
        const cancelled = await cancelOrder(api, owner, toCancel.orderId, target.tier.side);
        processedOrderIds.add(toCancel.orderId);
        if (!cancelled) {
          log('warn', `[${step}/${total}] ${target.tier.label} skip refresh: cannot cancel order #${toCancel.orderId}`);
          return false;
        }
      }
    }

    let nexAmount: bigint;
    try {
      nexAmount = randomOrderNex();
    } catch (error) {
      logError(`${target.tier.label} random amount`, error);
      return false;
    }

    log('tier', `[${step}/${total}] ${target.tier.label} place ${target.tier.side.toLowerCase()} amount=${formatNex(nexAmount)}`);
    const orderId = await placeTierOrder(api, seller, buyer, target.tier, target.price, nexAmount);
    if (orderId >= 0) {
      processedOrderIds.add(orderId);
      log('tier', `[${step}/${total}] ${target.tier.label} done orderId=${orderId}`);
    }
    return orderId >= 0;
  } catch (error) {
    logError(`${target.tier.label} reconcile`, error);
    return false;
  }
}

async function reconcileMarketOrders(
  api: ApiPromise,
  seller: KeyringPair,
  buyer: KeyringPair,
  lastRefreshAt: number,
  forceRefresh: boolean,
): Promise<ReconcileResult> {
  if (!await verifyApiConnection(api)) {
    log('warn', 'rpc disconnected before reconcile, will reconnect');
    return { lastRefreshAt, needsReconnect: true };
  }

  try {
    const ctx = await readPriceProtectionContext(api);
    log(
      'price',
      `effectiveRef=${formatMicroUsdtPrice(ctx.effectiveReference)} source=${ctx.effectiveSource} `
      + `allowed=[${formatMicroUsdtPrice(minAllowedPrice(ctx))}, ${formatMicroUsdtPrice(maxAllowedPrice(ctx))}] `
      + `marketHint=${ctx.marketHint == null ? 'null' : formatMicroUsdtPrice(ctx.marketHint)} `
      + `maxDev=${ctx.maxDeviationBps.toString()}bps`,
    );

    const activeSellOrders = await readOwnActiveOrders(api, seller.address, 'Sell');
    const activeBuyOrders = await readOwnActiveOrders(api, buyer.address, 'Buy');
    log(
      'order',
      `sellActive=${activeSellOrders.map((o) => `#${o.orderId}:${o.price.toString()}:${o.status}:rem=${formatNex(remainingOrderAmount(o))}`).join(', ') || '(none)'}`,
    );
    log(
      'order',
      `buyActive=${activeBuyOrders.map((o) => `#${o.orderId}:${o.price.toString()}:${o.status}:rem=${formatNex(remainingOrderAmount(o))}`).join(', ') || '(none)'}`,
    );

    const now = Date.now();
    const dueRefresh = forceRefresh || now - lastRefreshAt >= REFRESH_INTERVAL_MS;
    if (dueRefresh) {
      const nextIn = forceRefresh ? 0 : REFRESH_INTERVAL_MS;
      log('refresh', `scheduled refresh due (interval=${REFRESH_INTERVAL_MS}ms, tierGap=${TIER_RECONCILE_INTERVAL_MS}ms, next cycle in ${nextIn}ms)`);
      lastRefreshAt = now;
    }

    const sellTargets = buildTierTargets(SELL_TIERS, ctx);
    const buyTargets = buildTierTargets(BUY_TIERS, ctx);
    enforceBidAskSpread(sellTargets, buyTargets, ctx);

    const allTargets = [...sellTargets, ...buyTargets];
    const processedOrderIds = new Set<number>();
    for (let i = 0; i < allTargets.length; i++) {
      try {
        const step = i + 1;
        const total = allTargets.length;
        const mutated = await reconcileOneTier(
          api,
          seller,
          buyer,
          ctx,
          allTargets[i],
          dueRefresh,
          processedOrderIds,
          step,
          total,
        );
        if (mutated) {
          await waitBeforeNextTier(step, total, `${allTargets[i].tier.label} reconcile done`);
        }
      } catch (error) {
        logError(`${allTargets[i].tier.label} tier loop`, error);
      }
    }

    await cancelOrphanOrders(api, seller, 'Sell', processedOrderIds);
    await cancelOrphanOrders(api, buyer, 'Buy', processedOrderIds);

    for (let i = 0; i < allTargets.length; i++) {
      try {
        const target = allTargets[i];
        const owner = target.tier.side === 'Sell' ? seller : buyer;
        const activeOrders = await readOwnActiveOrders(api, owner.address, target.tier.side);
        if (findTierOrder(activeOrders, target.price)) continue;
        log('fill', `missing ${target.tier.label}, placing now`);
        const mutated = await reconcileOneTier(
          api,
          seller,
          buyer,
          ctx,
          target,
          false,
          processedOrderIds,
          i + 1,
          allTargets.length,
        );
        if (mutated) {
          await waitBeforeNextTier(i + 1, allTargets.length, `${target.tier.label} fill done`);
        }
      } catch (error) {
        logError(`${allTargets[i].tier.label} fill loop`, error);
      }
    }

    const finalSells = await readOwnActiveOrders(api, seller.address, 'Sell');
    const finalBuys = await readOwnActiveOrders(api, buyer.address, 'Buy');
    log('book', `target ${REQUIRED_SELL_ORDERS} sell + ${REQUIRED_BUY_ORDERS} buy | actual sell=${finalSells.length} buy=${finalBuys.length}`);
    if (!DRY_RUN && (finalSells.length < REQUIRED_SELL_ORDERS || finalBuys.length < REQUIRED_BUY_ORDERS)) {
      log('warn', `order book incomplete: need ${REQUIRED_SELL_ORDERS} sell and ${REQUIRED_BUY_ORDERS} buy, will retry next poll`);
    } else if (!DRY_RUN && (finalSells.length > REQUIRED_SELL_ORDERS || finalBuys.length > REQUIRED_BUY_ORDERS)) {
      log('warn', `order book has extras: sell=${finalSells.length} buy=${finalBuys.length}, will clean next poll`);
    }
  } catch (error) {
    logError('reconcileMarketOrders', error);
    return {
      lastRefreshAt,
      needsReconnect: isWsDisconnectedError(error) || !api.isConnected,
    };
  }

  return { lastRefreshAt, needsReconnect: false };
}

async function main(): Promise<void> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const seller = keyring.addFromMnemonic(SELLER_MNEMONIC);
  const buyer = keyring.addFromMnemonic(BUYER_MNEMONIC);

  log('init', `ws=${process.env.WS_URL}`);
  log('init', `seller=${seller.address}`);
  log('init', `buyer=${buyer.address}`);
  log('init', `sellerTron=${SELLER_TRON_ADDRESS}`);
  log('init', `buyerTron=${BUYER_TRON_ADDRESS}`);
  log('init', `sellTiers=${SELL_TIERS.map(formatTierSpec).join(', ')}`);
  log('init', `buyTiers=${BUY_TIERS.map(formatTierSpec).join(', ')}`);
  log('init', `maintain ${REQUIRED_SELL_ORDERS} sell + ${REQUIRED_BUY_ORDERS} buy orders`);
  log('init', `randomOrderNex=${formatNex(RANDOM_ORDER_NEX_MIN)}..${formatNex(RANDOM_ORDER_NEX_MAX)}`);
  log('init', `refreshMs=${REFRESH_INTERVAL_MS} tierIntervalMs=${TIER_RECONCILE_INTERVAL_MS} pollMs=${POLL_INTERVAL_MS} dryRun=${DRY_RUN} runOnce=${RUN_ONCE}`);

  let stopping = false;
  const stop = () => {
    stopping = true;
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);

  while (!stopping) {
    let api: ApiPromise | undefined;
    let lastRefreshAt = 0;

    try {
      log('connect', `connecting ${process.env.WS_URL}`);
      api = await connectApi(process.env.WS_URL);
      log('connect', `connected spec=${api.runtimeVersion.specName.toString()} v${api.runtimeVersion.specVersion.toString()}`);
    } catch (error) {
      logError('connectApi', error);
      if (RUN_ONCE) break;
      await sleep(WS_RECONNECT_BACKOFF_MS);
      continue;
    }

    try {
      try {
        const minOrderNex = await readChainMinOrderNex(api);
        const balanceOk = await ensureAccountBalances(api, seller, buyer, minOrderNex);
        if (!balanceOk) {
          log('warn', 'balance check failed, continue and retry later');
        }
      } catch (error) {
        logError('startup checks', error);
        if (isWsDisconnectedError(error) || !api.isConnected) {
          throw error;
        }
      }

      try {
        const startup = await reconcileMarketOrders(api, seller, buyer, lastRefreshAt, true);
        lastRefreshAt = startup.lastRefreshAt;
        if (startup.needsReconnect) {
          throw new Error('websocket disconnected during startup reconcile');
        }
      } catch (error) {
        logError('startup reconcile', error);
        if (isWsDisconnectedError(error) || !api.isConnected) {
          throw error;
        }
      }

      if (RUN_ONCE) {
        break;
      }

      while (!stopping) {
        try {
          await sleep(POLL_INTERVAL_MS);
          if (!await verifyApiConnection(api)) {
            log('warn', 'rpc disconnected during poll, reconnecting session');
            break;
          }
          const result = await reconcileMarketOrders(api, seller, buyer, lastRefreshAt, false);
          lastRefreshAt = result.lastRefreshAt;
          if (result.needsReconnect) {
            log('warn', 'rpc error during reconcile, reconnecting session');
            break;
          }
        } catch (error) {
          logError('poll iteration', error);
          if (isWsDisconnectedError(error) || !api.isConnected) {
            break;
          }
          await sleep(ITERATION_ERROR_BACKOFF_MS);
        }
      }
      if (stopping) {
        break;
      }
    } catch (error) {
      logError('session', error);
      if (RUN_ONCE) break;
      await sleep(WS_RECONNECT_BACKOFF_MS);
    } finally {
      if (api) {
        try {
          await disconnectApi(api);
        } catch (error) {
          logError('disconnectApi', error);
        }
      }
    }
  }

  process.off('SIGINT', stop);
  process.off('SIGTERM', stop);
  log('cleanup', 'stopped');
}

main().catch((error) => {
  logError('main unhandled', error);
});
