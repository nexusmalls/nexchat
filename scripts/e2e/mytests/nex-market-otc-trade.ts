#!/usr/bin/env tsx
/**
 * NEX 场外交易脚本 / NEX Market OTC Trade Script
 *
 * 流程 / Flow:
 *   1. 卖家（fabric...）按市场价挂出卖单：10,000,000 NEX，
 *      接收 USDT 的 TRON 地址为 TLGDrkJrScqcK8zgSZL6kmicJifacs5nCy
 *   2. 买家（observe...）预留该卖单，并提供支付用 TRON 地址：
 *      TDiG6cksP2dJDVpmyoZ7Uhm4Ly71d7XiUV
 *   3. 买家线下转 USDT 后，在链上确认已付款
 *   4. 脚本订阅链上事件，等待 OCW 的 TRC20 校验自动完结交易，
 *      或等待卖家手动确认收款
 *
 * 用法 / Usage:
 *   node --import tsx mytests/nex-market-otc-trade.ts
 *
 * 环境变量 / Environment variables:
 *   WS_URL       — WebSocket 端点（默认: ws://127.0.0.1:9944）
 *   USDT_PRICE   — 覆盖每个 NEX 的 USDT 价格，单位为 micro-USDT（默认: 使用链上市场价）
 *   SKIP_UNTIL   — 从指定步骤继续执行（例如 "2" 表示从步骤 2 开始）
 *   ORDER_ID     — 已存在的订单 ID（当 SKIP_UNTIL > 1 时必填）
 *   TRADE_ID     — 已存在的交易 ID（当 SKIP_UNTIL > 2 时必填）
 */

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { createInterface } from 'node:readline';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import type { ApiPromise } from '@polkadot/api';
import { connectApi, disconnectApi, submitTx, type TxReceipt } from '../framework/api.js';
import { readFreeBalance } from '../framework/accounts.js';
import { assertTxSuccess } from '../framework/assert.js';
import { formatNex, nex } from '../framework/units.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { NEXUS_SS58_FORMAT } from '../../utils/ss58.js';

/* ------------------------------------------------------------------ */
/*  Configuration                                                      */
/* ------------------------------------------------------------------ */

const SELLER_MNEMONIC = 'fabric smile father unique elbow buffalo until emerge novel orient rally basket';
const BUYER_MNEMONIC  = 'observe club local wet fuel bargain mule divorce session leader before below';

// Sell 1,000,000 NEX
const SELL_NEX_AMOUNT = nex(1_000_000);

// USDT price per NEX (precision 10^6, i.e. micro-USDT)
// If USDT_PRICE env is set, use it; otherwise query from chain (LastTradePrice / BestAsk)
const USDT_PRICE_OVERRIDE = process.env.USDT_PRICE ? Number(process.env.USDT_PRICE) : null;

// TRON addresses
const SELLER_TRON_ADDRESS = 'TLGDrkJrScqcK8zgSZL6kmicJifacs5nCy'; // receives USDT
const BUYER_TRON_ADDRESS  = 'TDiG6cksP2dJDVpmyoZ7Uhm4Ly71d7XiUV';  // sends USDT

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
 * 输出阶段标题，突出当前执行步骤。
 */
function logStep(step: number, title: string): void {
  console.log(`\n${'='.repeat(70)}`);
  console.log(`  STEP ${step}: ${title}`);
  console.log(`${'='.repeat(70)}\n`);
}

/**
 * 记录一笔交易回执的关键字段，包括哈希、区块、事件和错误信息。
 */
function logReceipt(receipt: TxReceipt): void {
  log('tx', `  label:    ${receipt.label}`);
  log('tx', `  success:  ${receipt.success}`);
  log('tx', `  txHash:   ${receipt.txHash}`);
  if (receipt.blockHash) log('tx', `  block:    ${receipt.blockHash}`);
  if (receipt.extrinsicIndex != null) log('tx', `  extIndex: ${receipt.extrinsicIndex}`);
  if (receipt.error) log('tx', `  error:    ${receipt.error}`);
  if (receipt.events.length > 0) {
    log('tx', `  events (${receipt.events.length}):`);
    for (const ev of receipt.events) {
      log('tx', `    ${ev.section}.${ev.method}: ${JSON.stringify(ev.data)}`);
    }
  } else {
    log('tx', `  events: (none)`);
  }
}

/* ------------------------------------------------------------------ */
/*  辅助函数 / Helpers                                                 */
/* ------------------------------------------------------------------ */

/**
 * 从事件数据中提取数值字段，兼容具名对象和位置数组两种格式。
 */
function readNumericEventField(data: unknown, ...candidates: string[]): number | undefined {
  const direct = coerceNumber(readObjectField(data, ...candidates));
  if (direct != null) return direct;

  // Fallback: if data is an array, try each element
  if (Array.isArray(data)) {
    for (const item of data) {
      const parsed = coerceNumber(item);
      if (parsed != null) return parsed;
    }
  }
  return undefined;
}

/**
 * 从匹配 section.method 的事件中提取 data 字段。
 */
function findEventData(
  events: { section: string; method: string; data: any }[],
  section: string,
  method: string,
): any | undefined {
  const ev = events.find((e) => e.section === section && e.method === method);
  return ev?.data;
}

/**
 * 按订单 ID 查询链上订单存储。
 */
async function queryOrder(api: ApiPromise, orderId: number): Promise<Record<string, unknown> | null> {
  const raw = await (api.query as any).nexMarket.orders(orderId);
  if ((raw as any).isNone) return null;
  return codecToJson<Record<string, unknown>>((raw as any).unwrap());
}

/**
 * 按 USDT 交易 ID 查询链上交易存储。
 */
async function queryTrade(api: ApiPromise, tradeId: number): Promise<Record<string, unknown> | null> {
  const raw = await (api.query as any).nexMarket.usdtTrades(tradeId);
  if ((raw as any).isNone) return null;
  return codecToJson<Record<string, unknown>>((raw as any).unwrap());
}

/**
 * 判断交易状态是否已进入终态，不会再继续推进。
 */
function isTerminalStatus(status: string): boolean {
  const s = status.toLowerCase();
  return s.includes('completed') || s.includes('refunded') || s.includes('cancelled') || s.includes('disputed');
}

/**
 * 暂停执行，等待用户按下回车继续。
 */
function waitForEnter(prompt: string): Promise<void> {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.question(prompt, () => {
      rl.close();
      resolve();
    });
  });
}

/* ------------------------------------------------------------------ */
/*  主流程 / Main                                                      */
/* ------------------------------------------------------------------ */

/**
 * 主入口：执行 NEX OTC 卖单、预留、付款确认与事件监听流程。
 */
async function main(): Promise<void> {
  // ── Step 0: Initialize ────────────────────────────────────────────
  logStep(0, '初始化 / Initialize');

  log('init', '等待 WASM 加密模块就绪 / Waiting for WASM crypto to be ready...');
  await cryptoWaitReady();
  log('init', 'WASM 加密模块已就绪 / WASM crypto ready');

  const keyring = new Keyring({ type: 'sr25519', ss58Format: NEXUS_SS58_FORMAT });
  log('init', `已创建 Keyring / Keyring created (sr25519, ss58Format=${NEXUS_SS58_FORMAT})`);

  const seller = keyring.addFromMnemonic(SELLER_MNEMONIC);
  const buyer  = keyring.addFromMnemonic(BUYER_MNEMONIC);

  log('init', `卖家助记词前缀 / Seller mnemonic: ${SELLER_MNEMONIC.split(' ').slice(0, 3).join(' ')}...`);
  log('init', `卖家地址 / Seller address:  ${seller.address}`);
  log('init', `买家助记词前缀 / Buyer mnemonic: ${BUYER_MNEMONIC.split(' ').slice(0, 3).join(' ')}...`);
  log('init', `买家地址 / Buyer address:  ${buyer.address}`);
  log('init', `卖出数量 / Sell amount:     ${formatNex(SELL_NEX_AMOUNT)} (raw: ${SELL_NEX_AMOUNT.toString()})`);
  log('init', `卖家 TRON 地址 / Seller TRON:     ${SELLER_TRON_ADDRESS} (receive USDT)`);
  log('init', `买家 TRON 地址 / Buyer TRON:     ${BUYER_TRON_ADDRESS} (send USDT)`);
  log('init', `跳过到步骤 / SKIP_UNTIL:      ${SKIP_UNTIL}`);

  const wsUrl = process.env.WS_URL ?? 'ws://127.0.0.1:9944';
  log('init', `Connecting to chain: ${wsUrl}`);
  const api = await connectApi();
  log('init', `Connected — spec: ${api.runtimeVersion.specName.toString()} v${api.runtimeVersion.specVersion.toString()}`);

  try {
    // ── Balance checks ──
    log('init', 'Querying account balances...');
    const sellerBalance = await readFreeBalance(api, seller.address);
    const buyerBalance  = await readFreeBalance(api, buyer.address);
    log('init', `Seller free balance: ${formatNex(sellerBalance)} (raw: ${sellerBalance.toString()})`);
    log('init', `Buyer  free balance: ${formatNex(buyerBalance)} (raw: ${buyerBalance.toString()})`);

    // ── Resolve USDT price: env override or query from chain ────────
    log('price', 'Resolving USDT price...');
    let usdtPrice: number;
    if (USDT_PRICE_OVERRIDE != null) {
      usdtPrice = USDT_PRICE_OVERRIDE;
      log('price', `Using USDT_PRICE from env override: ${usdtPrice} (${usdtPrice / 1_000_000} USDT/NEX)`);
    } else {
      log('price', 'No USDT_PRICE env set, querying chain for current market price...');
      const lastTradePriceRaw = await (api.query as any).nexMarket.lastTradePrice();
      const bestAskRaw = await (api.query as any).nexMarket.bestAsk();
      const bestBidRaw = await (api.query as any).nexMarket.bestBid();

      const lastTradePrice = lastTradePriceRaw.isSome ? Number(lastTradePriceRaw.unwrap().toString()) : 0;
      const bestAsk = bestAskRaw.isSome ? Number(bestAskRaw.unwrap().toString()) : 0;
      const bestBid = bestBidRaw.isSome ? Number(bestBidRaw.unwrap().toString()) : 0;

      log('price', `  LastTradePrice: ${lastTradePrice === 0 ? '(none)' : `${lastTradePrice} = ${lastTradePrice / 1_000_000} USDT/NEX`}`);
      log('price', `  BestAsk:        ${bestAsk === 0 ? '(none)' : `${bestAsk} = ${bestAsk / 1_000_000} USDT/NEX`}`);
      log('price', `  BestBid:        ${bestBid === 0 ? '(none)' : `${bestBid} = ${bestBid / 1_000_000} USDT/NEX`}`);

      // Priority: LastTradePrice (most recent real price) > BestAsk > BestBid
      if (lastTradePrice > 0) {
        usdtPrice = lastTradePrice;
        log('price', `Selected LastTradePrice: ${usdtPrice}`);
      } else if (bestAsk > 0) {
        usdtPrice = bestAsk;
        log('price', `Selected BestAsk (no LastTradePrice): ${usdtPrice}`);
      } else if (bestBid > 0) {
        usdtPrice = bestBid;
        log('price', `Selected BestBid (no LastTradePrice, no BestAsk): ${usdtPrice}`);
      } else {
        throw new Error(
          'No market price available (LastTradePrice/BestAsk/BestBid all empty). ' +
          'Set USDT_PRICE env manually, e.g. USDT_PRICE=10000 (= 0.01 USDT/NEX)',
        );
      }
      log('price', `Using chain market price: ${usdtPrice} (${usdtPrice / 1_000_000} USDT/NEX)`);
    }

    const totalUsdt = Number(SELL_NEX_AMOUNT / BigInt(1e12)) * usdtPrice / 1_000_000;
    log('init', `── Price summary ──`);
    log('init', `  USDT price:  ${usdtPrice / 1_000_000} USDT/NEX (raw: ${usdtPrice})`);
    log('init', `  NEX amount:  ${formatNex(SELL_NEX_AMOUNT)}`);
    log('init', `  Total USDT:  ${totalUsdt.toLocaleString()} USDT`);

    // ── Pre-flight checks ──
    log('check', 'Running pre-flight checks...');
    if (sellerBalance < SELL_NEX_AMOUNT) {
      log('check', `FAIL: Seller balance ${formatNex(sellerBalance)} < sell amount ${formatNex(SELL_NEX_AMOUNT)}`);
      throw new Error(
        `Seller has insufficient balance: ${formatNex(sellerBalance)} < ${formatNex(SELL_NEX_AMOUNT)}`,
      );
    }
    log('check', `OK: Seller balance ${formatNex(sellerBalance)} >= sell amount ${formatNex(SELL_NEX_AMOUNT)}`);

    if (buyerBalance === 0n) {
      log('check', 'FAIL: Buyer has zero balance — needs NEX for buyer deposit');
      throw new Error('Buyer has zero balance — needs NEX for buyer deposit when reserving sell order');
    }
    log('check', `OK: Buyer has balance ${formatNex(buyerBalance)} (deposit will be deducted when reserving)`);
    log('check', 'All pre-flight checks passed');

    let orderId: number | null = null;
    let tradeId: number | null = null;

    // ── Step 1: Place Sell Order ──────────────────────────────────────
    if (SKIP_UNTIL <= 1) {
      logStep(1, 'Place Sell Order (1,000,000 NEX)');

      log('sell', `Preparing placeSellOrder transaction...`);
      log('sell', `  nex_amount:      ${SELL_NEX_AMOUNT.toString()} (${formatNex(SELL_NEX_AMOUNT)})`);
      log('sell', `  usdt_price:      ${usdtPrice} (${usdtPrice / 1_000_000} USDT/NEX)`);
      log('sell', `  tron_address:    ${SELLER_TRON_ADDRESS}`);
      log('sell', `  min_fill_amount: null (no minimum)`);
      log('sell', `  signer:          ${seller.address}`);

      const sellTx = (api.tx as any).nexMarket.placeSellOrder(
        SELL_NEX_AMOUNT.toString(),       // nex_amount (Balance)
        usdtPrice,                         // usdt_price (u64, micro-USDT per NEX)
        SELLER_TRON_ADDRESS,               // tron_address (Vec<u8>)
        null,                              // min_fill_amount (Option<Balance>)
      );

      log('sell', `Submitting tx...`);
      const sellReceipt = await submitTx(api, sellTx, seller, 'place sell order');
      log('sell', `Transaction result:`);
      logReceipt(sellReceipt);
      assertTxSuccess(sellReceipt, 'place sell order should succeed');

      // Extract order ID from OrderCreated event
      // Event: OrderCreated { order_id, maker, side, nex_amount, usdt_price }
      log('sell', `Extracting order ID from OrderCreated event...`);
      const orderCreatedData = findEventData(sellReceipt.events, 'nexMarket', 'OrderCreated');
      if (orderCreatedData) {
        log('sell', `  OrderCreated event data: ${JSON.stringify(orderCreatedData)}`);
        orderId = readNumericEventField(orderCreatedData, 'order_id', 'orderId') ?? null;
        if (orderId != null) {
          log('sell', `  Extracted order ID: ${orderId}`);
        } else {
          log('sell', `  Could not extract order_id field from event data`);
        }
      } else {
        log('sell', `  OrderCreated event not found in receipt events`);
      }
      if (orderId == null) {
        log('sell', `  Falling back to NextOrderId storage...`);
        const nextOrderId = await (api.query as any).nexMarket.nextOrderId();
        orderId = Number(nextOrderId.toString()) - 1;
        log('sell', `  NextOrderId on chain: ${Number(nextOrderId.toString())}, inferred order ID: ${orderId}`);
      }

      // Verify the order on chain
      log('sell', `Verifying order #${orderId} on chain...`);
      const order = await queryOrder(api, orderId);
      if (order) {
        log('verify', `Order #${orderId} on-chain data:`);
        log('verify', JSON.stringify(order, null, 2));
      } else {
        log('warn', `Order #${orderId} NOT found in storage (unexpected)`);
      }

      // Check seller balance after lock
      const sellerBalanceAfter = await readFreeBalance(api, seller.address);
      log('sell', `Seller balance after NEX lock: ${formatNex(sellerBalanceAfter)} (locked: ${formatNex(sellerBalance - sellerBalanceAfter)})`);
      log('sell', `Order #${orderId} created — ${formatNex(SELL_NEX_AMOUNT)} locked on chain, awaiting buyer`);
    } else {
      log('skip', `Step 1 skipped (SKIP_UNTIL=${SKIP_UNTIL})`);
      orderId = Number(process.env.ORDER_ID ?? '0');
      if (orderId === 0) {
        log('skip', 'ORDER_ID not set, querying UserOrders for seller...');
        const userOrders = await (api.query as any).nexMarket.userOrders(seller.address);
        const orderIds = codecToJson<number[]>(userOrders);
        log('skip', `  UserOrders result: ${JSON.stringify(orderIds)}`);
        if (Array.isArray(orderIds) && orderIds.length > 0) {
          orderId = orderIds[orderIds.length - 1];
          log('skip', `  Using last order ID: ${orderId}`);
        } else {
          throw new Error('No existing orders found and SKIP_UNTIL > 1. Set ORDER_ID env.');
        }
      } else {
        log('skip', `Using ORDER_ID from env: ${orderId}`);
      }
      const order = await queryOrder(api, orderId);
      if (order) {
        log('skip', `Order #${orderId} on-chain data:`);
        log('skip', JSON.stringify(order, null, 2));
      }
    }

    // ── Step 2: Buyer Reserves (Takes) the Sell Order ────────────────
    if (SKIP_UNTIL <= 2) {
      logStep(2, 'Buyer Reserves Sell Order');

      log('buy', `Preparing reserveSellOrder transaction...`);
      log('buy', `  order_id:           ${orderId}`);
      log('buy', `  amount:             null (take all available)`);
      log('buy', `  buyer_tron_address: ${BUYER_TRON_ADDRESS}`);
      log('buy', `  signer:             ${buyer.address}`);
      log('buy', `  buyer balance:      ${formatNex(buyerBalance)}`);

      const reserveTx = (api.tx as any).nexMarket.reserveSellOrder(
        orderId,                           // order_id (u64)
        null,                              // amount (Option<Balance>, null = take all)
        BUYER_TRON_ADDRESS,                // buyer_tron_address (Vec<u8>)
      );

      log('buy', `Submitting tx...`);
      const reserveReceipt = await submitTx(api, reserveTx, buyer, 'reserve sell order');
      log('buy', `Transaction result:`);
      logReceipt(reserveReceipt);
      assertTxSuccess(reserveReceipt, 'reserve sell order should succeed');

      // Extract trade ID from UsdtTradeCreated event
      // Event: UsdtTradeCreated { trade_id, order_id, seller, buyer, nex_amount, usdt_amount }
      log('buy', `Extracting trade ID from UsdtTradeCreated event...`);
      const tradeCreatedData = findEventData(reserveReceipt.events, 'nexMarket', 'UsdtTradeCreated');
      if (tradeCreatedData) {
        log('buy', `  UsdtTradeCreated event data: ${JSON.stringify(tradeCreatedData)}`);
        tradeId = readNumericEventField(tradeCreatedData, 'trade_id', 'tradeId') ?? null;
        if (tradeId != null) {
          log('buy', `  Extracted trade ID: ${tradeId}`);
        } else {
          log('buy', `  Could not extract trade_id field from event data`);
        }
      } else {
        log('buy', `  UsdtTradeCreated event not found in receipt events`);
      }
      if (tradeId == null) {
        log('buy', `  Falling back to NextUsdtTradeId storage...`);
        const nextTradeId = await (api.query as any).nexMarket.nextUsdtTradeId();
        tradeId = Number(nextTradeId.toString()) - 1;
        log('buy', `  NextUsdtTradeId on chain: ${Number(nextTradeId.toString())}, inferred trade ID: ${tradeId}`);
      }

      // Check BuyerDepositLocked event
      const depositData = findEventData(reserveReceipt.events, 'nexMarket', 'BuyerDepositLocked');
      if (depositData) {
        log('buy', `  BuyerDepositLocked: ${JSON.stringify(depositData)}`);
      } else {
        log('buy', `  No BuyerDepositLocked event (deposit may be waived for first trade)`);
      }

      // Verify trade on chain
      log('buy', `Verifying trade #${tradeId} on chain...`);
      const trade = await queryTrade(api, tradeId);
      if (trade) {
        log('verify', `Trade #${tradeId} on-chain data:`);
        log('verify', JSON.stringify(trade, null, 2));
      } else {
        log('warn', `Trade #${tradeId} NOT found in storage (unexpected)`);
      }

      // Check buyer balance after deposit lock
      const buyerBalanceAfter = await readFreeBalance(api, buyer.address);
      log('buy', `Buyer balance after deposit lock: ${formatNex(buyerBalanceAfter)} (deducted: ${formatNex(buyerBalance - buyerBalanceAfter)})`);

      // Verify order status changed
      log('buy', `Verifying order #${orderId} status after reserve...`);
      const orderAfter = await queryOrder(api, orderId!);
      if (orderAfter) {
        const orderStatus = readObjectField(orderAfter, 'status');
        const filledAmount = readObjectField(orderAfter, 'filled_amount', 'filledAmount');
        log('buy', `  Order status: ${JSON.stringify(orderStatus)}, filled: ${filledAmount}`);
      }

      log('buy', `Trade #${tradeId} created — status: AwaitingPayment`);
      log('buy', `Buyer must now send USDT off-chain:`);
      log('buy', `  From: ${BUYER_TRON_ADDRESS}`);
      log('buy', `  To:   ${SELLER_TRON_ADDRESS}`);
      log('buy', `  Amount: ~${totalUsdt.toLocaleString()} USDT`);
    } else {
      log('skip', `Step 2 skipped (SKIP_UNTIL=${SKIP_UNTIL})`);
      tradeId = Number(process.env.TRADE_ID ?? '0');
      if (tradeId === 0) {
        throw new Error('No trade ID available and SKIP_UNTIL > 2. Set TRADE_ID env.');
      }
      log('skip', `Using TRADE_ID from env: ${tradeId}`);
      const trade = await queryTrade(api, tradeId);
      if (trade) {
        log('skip', `Trade #${tradeId} on-chain data:`);
        log('skip', JSON.stringify(trade, null, 2));
      }
    }

    // ── Step 3: Subscribe to events, wait for manual confirmation, then confirm payment
    // Subscribe before confirm_payment to avoid missing events if OCW
    // settles in the same block or very quickly after confirmation.

    logStep(3, 'Confirm Payment (with manual approval)');

    let tradeCompleted = false;
    let tradeTerminal = false;

    // Set up event subscription BEFORE confirming payment
    log('sub', 'Setting up chain event subscription (nexMarket events)...');
    const unsubscribeEvents = await (api.query.system.events as any)((events: any) => {
      for (const record of events) {
        const { event } = record;
        const section = event.section.toString();
        const method = event.method.toString();

        if (section !== 'nexMarket') continue;

        const data = codecToJson(event.data);
        // Event data may be a named object or positional array depending on metadata version
        const eventTradeId = readNumericEventField(data, 'trade_id', 'tradeId');

        if (eventTradeId !== tradeId) continue;

        switch (method) {
          case 'UsdtTradeCompleted':
            log('EVENT', `UsdtTradeCompleted — Trade #${tradeId} settled successfully!`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            tradeCompleted = true;
            tradeTerminal = true;
            break;
          case 'SellerConfirmedReceived':
            log('EVENT', `SellerConfirmedReceived — Seller manually confirmed, trade settled!`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            tradeCompleted = true;
            tradeTerminal = true;
            break;
          case 'UsdtTradeVerificationFailed':
            log('EVENT', `UsdtTradeVerificationFailed — Trade #${tradeId} verification failed`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            tradeTerminal = true;
            break;
          case 'UnderpaidDetected':
            log('EVENT', `UnderpaidDetected — Buyer underpaid, 2h grace period to top up`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            break;
          case 'BuyerDepositReleased':
            log('EVENT', `BuyerDepositReleased — Buyer deposit returned`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            break;
          case 'BuyerDepositForfeited':
            log('EVENT', `BuyerDepositForfeited — Buyer deposit forfeited`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            break;
          case 'UsdtPaymentSubmitted':
            log('EVENT', `UsdtPaymentSubmitted — Payment confirmation recorded on chain`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
            break;
          default:
            log('EVENT', `${section}.${method}`);
            log('EVENT', `  Data: ${JSON.stringify(data)}`);
        }
      }
    });
    log('sub', 'Event subscription active — listening for nexMarket events');

    // Now confirm payment (subscription is already active)
    if (SKIP_UNTIL <= 3) {
      // ── Wait for manual confirmation ──
      console.log(`\n${'─'.repeat(70)}`);
      console.log(`  Trade #${tradeId} is now AwaitingPayment.`);
      console.log(``);
      console.log(`  Please send USDT off-chain now:`);
      console.log(`    From (buyer TRON):  ${BUYER_TRON_ADDRESS}`);
      console.log(`    To (seller TRON):   ${SELLER_TRON_ADDRESS}`);
      console.log(`    Expected amount:    ~${totalUsdt.toLocaleString()} USDT`);
      console.log(``);
      console.log(`  After sending, press ENTER to call confirmPayment on chain.`);
      console.log(`${'─'.repeat(70)}`);
      await waitForEnter('\n  >>> Press ENTER after USDT payment has been sent... ');
      console.log('');

      log('confirm', `Preparing confirmPayment transaction...`);
      log('confirm', `  trade_id: ${tradeId}`);
      log('confirm', `  signer:   ${buyer.address} (buyer)`);
      log('confirm', `  action:   Tells chain that USDT has been sent off-chain`);
      log('confirm', `  result:   Trade status will change AwaitingPayment -> AwaitingVerification`);

      const confirmTx = (api.tx as any).nexMarket.confirmPayment(tradeId);
      log('confirm', `Submitting tx...`);
      const confirmReceipt = await submitTx(api, confirmTx, buyer, 'confirm payment');
      log('confirm', `Transaction result:`);
      logReceipt(confirmReceipt);
      assertTxSuccess(confirmReceipt, 'confirm payment should succeed');

      // Verify trade status changed
      log('confirm', `Verifying trade #${tradeId} status after confirmation...`);
      const tradeAfterConfirm = await queryTrade(api, tradeId!);
      if (tradeAfterConfirm) {
        const status = readObjectField(tradeAfterConfirm, 'status');
        const paymentConfirmed = readObjectField(tradeAfterConfirm, 'payment_confirmed', 'paymentConfirmed');
        log('confirm', `  status:            ${JSON.stringify(status)}`);
        log('confirm', `  payment_confirmed: ${JSON.stringify(paymentConfirmed)}`);
      }

      log('confirm', `Payment confirmed! Trade #${tradeId} -> AwaitingVerification`);
      log('confirm', `OCW will now query TronGrid API to verify the TRC20 USDT transfer`);
    } else {
      log('skip', `Step 3 skipped (SKIP_UNTIL=${SKIP_UNTIL})`);
      log('skip', `Checking current trade status...`);
      const currentTrade = await queryTrade(api, tradeId!);
      if (currentTrade) {
        log('skip', `Trade #${tradeId}: status=${JSON.stringify(readObjectField(currentTrade, 'status'))}`);
      }
    }

    // ── Step 4: Wait for OCW Verification & Trade Completion ─────────
    logStep(4, 'Wait for OCW Verification (auto-detect USDT on TRON)');

    log('ocw', `The chain's Off-Chain Worker (OCW) verification flow:`);
    log('ocw', `  1. OCW reads PendingUsdtTrades queue (every ~60s)`);
    log('ocw', `  2. Calls TronGrid API: query USDT transfers`);
    log('ocw', `     from: ${BUYER_TRON_ADDRESS}`);
    log('ocw', `     to:   ${SELLER_TRON_ADDRESS}`);
    log('ocw', `  3. Validates: amount match, >=19 TRON confirmations, unique tx_hash`);
    log('ocw', `  4. Submits submit_ocw_result(trade_id, actual_amount, tx_hash)`);
    log('ocw', `  5. Chain auto-settles: releases locked NEX to buyer, returns deposit`);
    log('ocw', ``);
    log('ocw', `Monitoring via event subscription + storage polling (every 12s)...`);
    log('ocw', `Max wait: 30 minutes. Events will be logged in real-time.`);
    log('ocw', ``);

    // Poll storage as a fallback alongside event subscription
    const MAX_WAIT_MS = 30 * 60 * 1000; // 30 minutes
    const POLL_INTERVAL_MS = 12_000;     // 12 seconds (~2 blocks)
    const startTime = Date.now();
    let pollCount = 0;

    while (!tradeTerminal && Date.now() - startTime < MAX_WAIT_MS) {
      await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
      pollCount++;

      const trade = await queryTrade(api, tradeId!);
      if (trade) {
        const status = String(readObjectField(trade, 'status') ?? '');
        const elapsed = Math.round((Date.now() - startTime) / 1000);
        const minutes = Math.floor(elapsed / 60);
        const seconds = elapsed % 60;
        log('poll', `#${pollCount} [${minutes}m${String(seconds).padStart(2, '0')}s] Trade #${tradeId} status: ${status}`);

        if (status.toLowerCase().includes('completed')) {
          tradeCompleted = true;
          tradeTerminal = true;
          log('poll', `Trade #${tradeId} COMPLETED!`);
          break;
        }
        if (isTerminalStatus(status)) {
          tradeTerminal = true;
          log('poll', `Trade #${tradeId} reached terminal status: ${status}`);
          break;
        }
      } else {
        log('warn', `poll #${pollCount}: Trade #${tradeId} not found in storage`);
      }
    }

    if (!tradeTerminal) {
      const elapsed = Math.round((Date.now() - startTime) / 1000);
      log('timeout', `Max wait time reached (${elapsed}s / ${MAX_WAIT_MS / 1000}s), ${pollCount} polls executed`);
    }

    // Clean up subscription
    log('sub', 'Cleaning up event subscription...');
    unsubscribeEvents();
    log('sub', 'Event subscription removed');

    // ── Step 5: Final Verification ───────────────────────────────────
    logStep(5, 'Final Status');

    log('final', `Querying final state from chain...`);

    const finalTrade = await queryTrade(api, tradeId!);
    const finalOrder = await queryOrder(api, orderId!);

    const sellerFinalBalance = await readFreeBalance(api, seller.address);
    const buyerFinalBalance  = await readFreeBalance(api, buyer.address);

    log('final', `── Trade #${tradeId} ──`);
    if (finalTrade) {
      log('final', JSON.stringify(finalTrade, null, 2));
    } else {
      log('final', '(not found)');
    }

    log('final', `── Order #${orderId} ──`);
    if (finalOrder) {
      log('final', JSON.stringify(finalOrder, null, 2));
    } else {
      log('final', '(not found)');
    }

    log('final', `── Balance changes ──`);
    log('final', `  Seller: ${formatNex(sellerBalance)} -> ${formatNex(sellerFinalBalance)} (delta: ${formatNex(sellerFinalBalance - sellerBalance)})`);
    log('final', `  Buyer:  ${formatNex(buyerBalance)} -> ${formatNex(buyerFinalBalance)} (delta: ${formatNex(buyerFinalBalance - buyerBalance)})`);

    if (tradeCompleted) {
      console.log(`\n${'='.repeat(70)}`);
      console.log(`  TRADE COMPLETED SUCCESSFULLY`);
      console.log(`${'='.repeat(70)}`);
      console.log(`  Order ID:           ${orderId}`);
      console.log(`  Trade ID:           ${tradeId}`);
      console.log(`  NEX sold:           ${formatNex(SELL_NEX_AMOUNT)}`);
      console.log(`  USDT price:         ${usdtPrice / 1_000_000} USDT/NEX`);
      console.log(`  Total USDT:         ~${totalUsdt.toLocaleString()} USDT`);
      console.log(`  Seller TRON (recv): ${SELLER_TRON_ADDRESS}`);
      console.log(`  Buyer TRON (send):  ${BUYER_TRON_ADDRESS}`);
      console.log(`  Seller balance:     ${formatNex(sellerBalance)} -> ${formatNex(sellerFinalBalance)}`);
      console.log(`  Buyer balance:      ${formatNex(buyerBalance)} -> ${formatNex(buyerFinalBalance)}`);
      console.log(`${'='.repeat(70)}\n`);
    } else {
      const finalStatus = finalTrade ? String(readObjectField(finalTrade, 'status') ?? 'unknown') : 'unknown';
      console.log(`\n${'='.repeat(70)}`);
      console.log(`  TRADE NOT YET COMPLETED`);
      console.log(`${'='.repeat(70)}`);
      console.log(`  Trade ID:  ${tradeId}`);
      console.log(`  Order ID:  ${orderId}`);
      console.log(`  Status:    ${finalStatus}`);
      console.log(`  Elapsed:   ${Math.round((Date.now() - startTime) / 1000)}s (${pollCount} polls)`);
      console.log(``);
      console.log(`  Next steps:`);
      console.log(`  1. Wait longer — OCW retries every ~60 seconds`);
      console.log(`  2. Seller manually confirms via polkadot.js Apps:`);
      console.log(`     nexMarket.sellerConfirmReceived(${tradeId})`);
      console.log(`  3. Re-run this script to continue monitoring:`);
      console.log(`     SKIP_UNTIL=4 TRADE_ID=${tradeId} ORDER_ID=${orderId} node --import tsx mytests/nex-market-otc-trade.ts`);
      console.log(`${'='.repeat(70)}\n`);
    }

  } finally {
    log('cleanup', 'Disconnecting from chain...');
    await disconnectApi(api);
    log('cleanup', 'Disconnected');
  }
}

main().catch((err) => {
  console.error('OTC Trade failed:', err);
  process.exit(1);
});
