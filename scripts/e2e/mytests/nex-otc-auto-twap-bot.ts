#!/usr/bin/env tsx

import path from 'node:path';
import { fileURLToPath } from 'node:url';
import dotenv from 'dotenv';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
dotenv.config({ path: path.resolve(SCRIPT_DIR, '.env') });
dotenv.config({ path: path.resolve(SCRIPT_DIR, '../../../.env') });

process.env.WS_URL ??= 'wss://nexuscom.duckdns.org:9948';

import { Keyring } from '@polkadot/keyring';
import type { KeyringPair } from '@polkadot/keyring/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { assertTxSuccess } from '../framework/assert.js';
import { codecToJson, coerceNumber, readObjectField } from '../framework/codec.js';
import { NEX_PLANCK, asBigInt, formatNex } from '../framework/units.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

const SELLER_MNEMONIC = requireEnv('SELLER_MNEMONIC');
const BUYER_MNEMONIC = requireEnv('BUYER_MNEMONIC');

const SELLER_TRON_ADDRESS = process.env.SELLER_TRON_ADDRESS ?? 'TLGDrkJrScqcK8zgSZL6kmicJifacs5nCy';
const BUYER_TRON_ADDRESS = process.env.BUYER_TRON_ADDRESS ?? 'TDiG6cksP2dJDVpmyoZ7Uhm4Ly71d7XiUV';

const TARGET_USDT_MICRO = 1_000n * 1_000_000n;
const PRICE_MULTIPLIER_BPS = 12_000n;
const BUY_INTERVAL_MS = Number(process.env.BUY_INTERVAL_MS ?? 4 * 60 * 60 * 1000);
const POLL_INTERVAL_MS = Number(process.env.POLL_INTERVAL_MS ?? 30_000);
const ITERATION_ERROR_BACKOFF_MS = Number(process.env.ITERATION_ERROR_BACKOFF_MS ?? POLL_INTERVAL_MS);
const TWAP_REPRICE_BPS = BigInt(Number(process.env.TWAP_REPRICE_BPS ?? 25));
const DRY_RUN = process.env.DRY_RUN === '1';
const RUN_ONCE = process.env.RUN_ONCE === '1';
const MIN_ORDER_NEX = 1_000n * NEX_PLANCK;
const MAX_ORDER_NEX = 100_000_000n * NEX_PLANCK;

type PriceSource = 'twap1h' | 'lastTradePrice' | 'initialPrice';

interface PriceSnapshot {
  source: PriceSource;
  referencePrice: bigint;
  twapRaw: bigint | null;
}

interface TargetOrder {
  twapRaw: bigint | null;
  referencePrice: bigint;
  targetPrice: bigint;
  targetNexRaw: bigint;
}

interface ActiveSellOrder {
  orderId: number;
  price: bigint;
  nexAmount: bigint;
  filledAmount: bigint;
  status: string;
}

function log(tag: string, msg: string): void {
  const ts = new Date().toISOString().slice(11, 23);
  console.log(`[${ts}] [${tag}] ${msg}`);
}

function logReceipt(receipt: TxReceipt): void {
  log('tx', `label=${receipt.label} success=${receipt.success} txHash=${receipt.txHash}`);
  if (receipt.blockHash) log('tx', `block=${receipt.blockHash}`);
  if (receipt.extrinsicIndex != null) log('tx', `extIndex=${receipt.extrinsicIndex}`);
  if (receipt.error) log('tx', `error=${receipt.error}`);
  for (const ev of receipt.events) {
    log('tx', `${ev.section}.${ev.method}: ${JSON.stringify(ev.data)}`);
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function requireEnv(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function absBigInt(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function remainingOrderAmount(order: ActiveSellOrder): bigint {
  return order.nexAmount > order.filledAmount ? order.nexAmount - order.filledAmount : 0n;
}

function withinBps(actual: bigint, expected: bigint, toleranceBps: bigint): boolean {
  if (expected <= 0n) return actual === expected;
  return absBigInt(actual - expected) * 10_000n <= expected * toleranceBps;
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

async function readPriceSnapshot(api: ApiPromise): Promise<PriceSnapshot> {
  const twapRaw = await readTwap1h(api);
  if (twapRaw != null && twapRaw > 0n) {
    return { source: 'twap1h', referencePrice: twapRaw, twapRaw };
  }

  const lastTradePriceRaw = await readStorageMaybe(
    (api.query as any).nexMarket,
    'lastTradePrice',
  );
  if (lastTradePriceRaw && (lastTradePriceRaw as any).isSome) {
    const price = asBigInt((lastTradePriceRaw as any).unwrap().toString());
    if (price > 0n) {
      return { source: 'lastTradePrice', referencePrice: price, twapRaw: null };
    }
  }

  const priceProtectionRaw = await readStorageMaybe(
    (api.query as any).nexMarket,
    'priceProtectionStore',
    'priceProtection',
  );
  const priceProtection = priceProtectionRaw
    ? codecToJson<Record<string, unknown> | null>(priceProtectionRaw)
    : null;
  if (priceProtectionRaw) {
    log('price', 'TWAP unavailable, fallback storage priceProtection available');
  }
  const initialPrice = asBigInt(readObjectField(priceProtection, 'initialPrice', 'initial_price'));
  if (initialPrice > 0n) {
    return { source: 'initialPrice', referencePrice: initialPrice, twapRaw: null };
  }

  throw new Error('No TWAP / lastTradePrice / initialPrice available');
}

function computeTarget(snapshot: PriceSnapshot): TargetOrder {
  const targetPrice = (snapshot.referencePrice * PRICE_MULTIPLIER_BPS) / 10_000n;
  if (targetPrice <= 0n) {
    throw new Error(`Invalid target price: ${targetPrice.toString()}`);
  }
  const targetNexRaw = (TARGET_USDT_MICRO * NEX_PLANCK) / targetPrice;
  return {
    twapRaw: snapshot.twapRaw,
    referencePrice: snapshot.referencePrice,
    targetPrice,
    targetNexRaw,
  };
}

async function readOwnActiveSellOrders(api: ApiPromise, sellerAddress: string): Promise<ActiveSellOrder[]> {
  const raw = await (api.query as any).nexMarket.userOrders(sellerAddress);
  const orderIds = codecToJson<unknown[]>(raw);
  const ids = Array.isArray(orderIds) ? orderIds.map((id) => coerceNumber(id)).filter((id): id is number => id != null) : [];
  const result: ActiveSellOrder[] = [];

  for (const orderId of ids) {
    const order = await queryOrder(api, orderId);
    if (!order) continue;
    const maker = String(readObjectField(order, 'maker') ?? '');
    const side = normalizeEnum(readObjectField(order, 'side'));
    const status = normalizeEnum(readObjectField(order, 'status'));
    if (maker !== sellerAddress) continue;
    if (side !== 'Sell') continue;
    if (status !== 'Open' && status !== 'PartiallyFilled') continue;

    result.push({
      orderId,
      price: asBigInt(readObjectField(order, 'usdtPrice', 'usdt_price')),
      nexAmount: asBigInt(readObjectField(order, 'nexAmount', 'nex_amount')),
      filledAmount: asBigInt(readObjectField(order, 'filledAmount', 'filled_amount')),
      status,
    });
  }

  return result;
}

function formatMicroUsdtPrice(raw: bigint): string {
  return `${raw.toString()} (${Number(raw) / 1_000_000} USDT/NEX)`;
}

function isOrderCompatibleWithTarget(order: ActiveSellOrder, target: TargetOrder): boolean {
  if (order.status !== 'Open' && order.status !== 'PartiallyFilled') return false;
  return remainingOrderAmount(order) > 0n && withinBps(order.price, target.targetPrice, TWAP_REPRICE_BPS);
}

function logTargetDetails(snapshot: PriceSnapshot, target: TargetOrder): void {
  const estimatedUsdtMicro = target.targetNexRaw * target.targetPrice / NEX_PLANCK;
  log('price', `source=${snapshot.source}`);
  log('price', `referencePrice=${formatMicroUsdtPrice(snapshot.referencePrice)}`);
  log('price', `twapRaw=${snapshot.twapRaw == null ? 'null' : formatMicroUsdtPrice(snapshot.twapRaw)}`);
  log('price', `targetPrice=reference*1.2=${formatMicroUsdtPrice(target.targetPrice)}`);
  log('price', `targetAmount=${formatNex(target.targetNexRaw)} raw=${target.targetNexRaw.toString()}`);
  log('price', `estimatedNotional=${Number(estimatedUsdtMicro) / 1_000_000} USDT raw=${estimatedUsdtMicro.toString()}`);
  log('limit', `minOrder=${formatNex(MIN_ORDER_NEX)} maxOrder=${formatNex(MAX_ORDER_NEX)}`);
}

async function ensureBalances(api: ApiPromise, seller: KeyringPair, buyer: KeyringPair, target: TargetOrder): Promise<void> {
  const sellerBalance = await readFreeBalance(api, seller.address);
  const buyerBalance = await readFreeBalance(api, buyer.address);
  log('balance', `seller=${formatNex(sellerBalance)} buyer=${formatNex(buyerBalance)}`);

  if (target.targetNexRaw < MIN_ORDER_NEX) {
    log('limit', 'target amount is below chain minimum');
    throw new Error(`Target order amount below minimum: ${formatNex(target.targetNexRaw)}`);
  }
  if (target.targetNexRaw > MAX_ORDER_NEX) {
    log('limit', 'target amount is above chain maximum');
    throw new Error(`Target order amount above maximum: ${formatNex(target.targetNexRaw)}`);
  }
  if (sellerBalance < target.targetNexRaw) {
    throw new Error(`Seller balance too low: ${formatNex(sellerBalance)} < ${formatNex(target.targetNexRaw)}`);
  }
  if (buyerBalance <= 0n) {
    throw new Error('Buyer free balance is zero');
  }
}

async function cancelOrder(api: ApiPromise, seller: KeyringPair, orderId: number): Promise<void> {
  log('cancel', `cancel order #${orderId}`);
  if (DRY_RUN) return;
  const tx = (api.tx as any).nexMarket.cancelOrder(orderId);
  const receipt = await submitTx(api, tx, seller, `cancel order ${orderId}`);
  logReceipt(receipt);
  assertTxSuccess(receipt, `cancel order ${orderId} should succeed`);
}

async function cancelAllOwnSellOrders(api: ApiPromise, seller: KeyringPair): Promise<void> {
  const orders = await readOwnActiveSellOrders(api, seller.address);
  for (const order of orders) {
    await cancelOrder(api, seller, order.orderId);
  }
}

async function placeTargetSellOrder(api: ApiPromise, seller: KeyringPair, target: TargetOrder): Promise<number> {
  log('sell', `place order priceRaw=${target.targetPrice.toString()} amount=${formatNex(target.targetNexRaw)}`);
  if (DRY_RUN) return -1;

  if (target.targetPrice > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`Target price exceeds JS safe integer: ${target.targetPrice.toString()}`);
  }

  const tx = (api.tx as any).nexMarket.placeSellOrder(
    target.targetNexRaw.toString(),
    Number(target.targetPrice),
    SELLER_TRON_ADDRESS,
    null,
  );
  const receipt = await submitTx(api, tx, seller, 'place target sell order');
  logReceipt(receipt);
  assertTxSuccess(receipt, 'place target sell order should succeed');

  const created = findEventData(receipt.events, 'nexMarket', 'OrderCreated');
  const eventOrderId = readNumericEventField(created, 'order_id', 'orderId');
  if (eventOrderId != null) return eventOrderId;

  const nextOrderId = await (api.query as any).nexMarket.nextOrderId();
  return Number(nextOrderId.toString()) - 1;
}

async function resolveBuyableOrderId(api: ApiPromise, sellerAddress: string, preferredOrderId: number | null): Promise<number | null> {
  const isBuyable = (order: Record<string, unknown> | null): boolean => {
    if (!order) return false;
    const maker = String(readObjectField(order, 'maker') ?? '');
    const side = normalizeEnum(readObjectField(order, 'side'));
    const status = normalizeEnum(readObjectField(order, 'status'));
    const nexAmount = asBigInt(readObjectField(order, 'nexAmount', 'nex_amount'));
    const filledAmount = asBigInt(readObjectField(order, 'filledAmount', 'filled_amount'));
    return maker === sellerAddress && side === 'Sell' && (status === 'Open' || status === 'PartiallyFilled') && nexAmount > filledAmount;
  };

  if (preferredOrderId != null) {
    const preferredOrder = await queryOrder(api, preferredOrderId);
    if (isBuyable(preferredOrder)) {
      const status = normalizeEnum(readObjectField(preferredOrder, 'status'));
      const nexAmount = asBigInt(readObjectField(preferredOrder, 'nexAmount', 'nex_amount'));
      const filledAmount = asBigInt(readObjectField(preferredOrder, 'filledAmount', 'filled_amount'));
      log('buy', `verified order #${preferredOrderId} status=${status} remaining=${formatNex(nexAmount - filledAmount)}`);
      return preferredOrderId;
    }
    log('buy', `preferred order #${preferredOrderId} is no longer buyable`);
  }

  const activeOrders = await readOwnActiveSellOrders(api, sellerAddress);
  if (activeOrders.length === 1 && remainingOrderAmount(activeOrders[0]) > 0n) {
    const fallback = activeOrders[0];
    log('buy', `fallback to active order #${fallback.orderId} status=${fallback.status} remaining=${formatNex(remainingOrderAmount(fallback))}`);
    return fallback.orderId;
  }

  return null;
}

async function reconcileSellerOrder(api: ApiPromise, seller: KeyringPair, buyer: KeyringPair): Promise<{ orderId: number | null; target: TargetOrder; snapshot: PriceSnapshot }> {
  const snapshot = await readPriceSnapshot(api);
  const target = computeTarget(snapshot);
  logTargetDetails(snapshot, target);
  await ensureBalances(api, seller, buyer, target);

  const activeOrders = await readOwnActiveSellOrders(api, seller.address);
  log('order', `active sell orders=${activeOrders.map((o) => `${o.orderId}:${o.price.toString()}:${o.status}`).join(', ') || '(none)'}`);

  if (activeOrders.length === 1 && isOrderCompatibleWithTarget(activeOrders[0], target)) {
    const current = activeOrders[0];
    const priceDiffBps = target.targetPrice > 0n
      ? Number((absBigInt(current.price - target.targetPrice) * 10_000n) / target.targetPrice)
      : 0;
    log('order', `reuse existing order #${current.orderId} priceDiffBps=${priceDiffBps} remaining=${formatNex(remainingOrderAmount(current))}`);
    return { orderId: current.orderId, target, snapshot };
  }

  if (activeOrders.length === 1) {
    const current = activeOrders[0];
    const priceDiffBps = target.targetPrice > 0n
      ? Number((absBigInt(current.price - target.targetPrice) * 10_000n) / target.targetPrice)
      : 0;
    log('order', `recreate order #${current.orderId} currentPrice=${current.price.toString()} targetPrice=${target.targetPrice.toString()} priceDiffBps=${priceDiffBps} threshold=${TWAP_REPRICE_BPS.toString()}`);
  } else if (activeOrders.length > 1) {
    log('order', `recreate because multiple active sell orders exist (${activeOrders.length})`);
  }

  await cancelAllOwnSellOrders(api, seller);
  const orderId = await placeTargetSellOrder(api, seller, target);
  return { orderId: DRY_RUN ? null : orderId, target, snapshot };
}

async function buyAndCompleteCurrentOrder(api: ApiPromise, seller: KeyringPair, buyer: KeyringPair, orderId: number): Promise<void> {
  const order = await queryOrder(api, orderId);
  const status = normalizeEnum(readObjectField(order, 'status'));
  const nexAmount = asBigInt(readObjectField(order, 'nexAmount', 'nex_amount'));
  const filledAmount = asBigInt(readObjectField(order, 'filledAmount', 'filled_amount'));
  const remaining = nexAmount > filledAmount ? nexAmount - filledAmount : 0n;
  log('buy', `verified order #${orderId} status=${status} remaining=${formatNex(remaining)}`);
  if (status !== 'Open' && status !== 'PartiallyFilled') {
    throw new Error(`Order ${orderId} is not buyable, current status=${status}`);
  }
  if (remaining <= 0n) {
    throw new Error(`Order ${orderId} has no remaining amount`);
  }

  log('buy', `reserve order #${orderId}`);
  if (DRY_RUN) return;

  const reserveTx = (api.tx as any).nexMarket.reserveSellOrder(orderId, null, BUYER_TRON_ADDRESS);
  const reserveReceipt = await submitTx(api, reserveTx, buyer, `reserve sell order ${orderId}`);
  logReceipt(reserveReceipt);
  assertTxSuccess(reserveReceipt, 'reserve sell order should succeed');

  const tradeCreated = findEventData(reserveReceipt.events, 'nexMarket', 'UsdtTradeCreated');
  let tradeId = readNumericEventField(tradeCreated, 'trade_id', 'tradeId');
  if (tradeId == null) {
    const nextTradeId = await (api.query as any).nexMarket.nextUsdtTradeId();
    tradeId = Number(nextTradeId.toString()) - 1;
  }

  log('buy', `confirm payment trade #${tradeId}`);
  const confirmTx = (api.tx as any).nexMarket.confirmPayment(tradeId);
  const confirmReceipt = await submitTx(api, confirmTx, buyer, `confirm payment ${tradeId}`);
  logReceipt(confirmReceipt);
  assertTxSuccess(confirmReceipt, 'confirm payment should succeed');

  log('sell', `seller confirm received trade #${tradeId}`);
  const sellerConfirmTx = (api.tx as any).nexMarket.sellerConfirmReceived(tradeId);
  const sellerConfirmReceipt = await submitTx(api, sellerConfirmTx, seller, `seller confirm received ${tradeId}`);
  logReceipt(sellerConfirmReceipt);
  assertTxSuccess(sellerConfirmReceipt, 'seller confirm received should succeed');

  const trade = await queryTrade(api, tradeId);
  const tradeStatus = normalizeEnum(readObjectField(trade, 'status'));
  log('trade', `trade #${tradeId} status=${tradeStatus}`);
  if (tradeStatus !== 'Completed') {
    throw new Error(`Trade ${tradeId} not completed, current status=${tradeStatus}`);
  }
}

async function main(): Promise<void> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  const seller = keyring.addFromMnemonic(SELLER_MNEMONIC);
  const buyer = keyring.addFromMnemonic(BUYER_MNEMONIC);

  log('init', `ws=${process.env.WS_URL}`);
  log('init', `seller=${seller.address}`);
  log('init', `buyer=${buyer.address}`);
  log('init', `dryRun=${DRY_RUN} runOnce=${RUN_ONCE} buyIntervalMs=${BUY_INTERVAL_MS} pollIntervalMs=${POLL_INTERVAL_MS} repriceBps=${TWAP_REPRICE_BPS.toString()}`);

  const api = await connectApi();
  let stopping = false;
  let lastSeenTwapRaw: string | null = null;
  let currentOrderId: number | null = null;
  let lastBuyAt = 0;

  const stop = () => {
    stopping = true;
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);

  try {
    const initial = await reconcileSellerOrder(api, seller, buyer);
    currentOrderId = initial.orderId;
    lastSeenTwapRaw = initial.snapshot.twapRaw?.toString() ?? null;

    if (RUN_ONCE) {
      return;
    }

    while (!stopping) {
      try {
        const snapshot = await readPriceSnapshot(api);
        const currentTwapRaw = snapshot.twapRaw?.toString() ?? null;

        if (currentTwapRaw !== lastSeenTwapRaw) {
          log('watch', `twap changed ${lastSeenTwapRaw ?? 'null'} -> ${currentTwapRaw ?? 'null'}`);
          const reconciled = await reconcileSellerOrder(api, seller, buyer);
          currentOrderId = reconciled.orderId;
          lastSeenTwapRaw = reconciled.snapshot.twapRaw?.toString() ?? null;
        }

        const now = Date.now();
        if (now - lastBuyAt >= BUY_INTERVAL_MS) {
          const buyableOrderId = await resolveBuyableOrderId(api, seller.address, currentOrderId);
          if (buyableOrderId == null) {
            log('buy', 'no buyable seller order found, reconcile before next attempt');
            const reconciled = await reconcileSellerOrder(api, seller, buyer);
            currentOrderId = reconciled.orderId;
            lastSeenTwapRaw = reconciled.snapshot.twapRaw?.toString() ?? null;
          } else {
            currentOrderId = buyableOrderId;
            await buyAndCompleteCurrentOrder(api, seller, buyer, buyableOrderId);
            lastBuyAt = now;
            const reconciled = await reconcileSellerOrder(api, seller, buyer);
            currentOrderId = reconciled.orderId;
            lastSeenTwapRaw = reconciled.snapshot.twapRaw?.toString() ?? null;
          }
        }

        await sleep(POLL_INTERVAL_MS);
      } catch (error) {
        const message = error instanceof Error ? error.stack ?? error.message : String(error);
        log('error', `iteration failed, retry after ${ITERATION_ERROR_BACKOFF_MS} ms: ${message}`);
        await sleep(ITERATION_ERROR_BACKOFF_MS);
      }
    }
  } finally {
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : error);
  process.exitCode = 1;
});
