#!/usr/bin/env tsx
/**
 * 订单资金流追踪脚本 / Order Fund Flow Tracer
 *
 * Usage:
 *   node --import tsx mytests/trace-order-funds.ts <order_id>
 *   node --import tsx mytests/trace-order-funds.ts 80
 *   ORDER_ID=0 node --import tsx mytests/trace-order-funds.ts
 *
 * Environment:
 *   WS_URL    — WebSocket endpoint (default: ws://127.0.0.1:9944)
 *   ORDER_ID  — Order ID (can also pass as first arg)
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex } from '../framework/units.js';
import { stringToU8a } from '@polkadot/util';
import { encodeAddress } from '@polkadot/util-crypto';

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function asBigInt(value: unknown): bigint {
  if (typeof value === 'bigint') return value;
  if (typeof value === 'number') return BigInt(value);
  if (typeof value === 'string') return BigInt(value.replace(/,/g, '').trim());
  if (value && typeof (value as any).toString === 'function') {
    try { return BigInt((value as any).toString()); } catch { return 0n; }
  }
  return 0n;
}

/**
 * 将 Token 精度数值格式化为可读文本。
 */
function formatToken(raw: bigint | number): string {
  const n = typeof raw === 'bigint' ? raw : BigInt(raw);
  return `${(Number(n) / 1e12).toLocaleString()} Token`;
}

/**
 * 缩短地址展示，保留首尾便于识别。
 */
function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function formatPct(numerator: bigint, denominator: bigint, digits = 2): string {
  if (denominator <= 0n) return '0';
  return ((Number(numerator) / Number(denominator)) * 100).toFixed(digits);
}

function uniqueNonEmpty(values: Iterable<string>): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const v of values) {
    if (!v || seen.has(v)) continue;
    seen.add(v);
    out.push(v);
  }
  return out;
}

function formatDelta(value: bigint, formatter: (raw: bigint) => string): string {
  if (value === 0n) return formatter(0n);
  return `${value > 0n ? '+' : '-'}${formatter(value > 0n ? value : -value)}`;
}

function deriveTreasuryAddress(entityId: number, ss58Format: number): string {
  const raw = new Uint8Array(32);
  raw.set(stringToU8a('modl'), 0);
  raw.set(stringToU8a('et/enty/'), 4);
  const dv = new DataView(raw.buffer);
  dv.setBigUint64(12, BigInt(entityId), true);
  return encodeAddress(raw, ss58Format);
}

type QueryLike = {
  query: Record<string, any>;
};

type ShoppingSnapshot = {
  buyerNexShoppingBalance: bigint;
  buyerTokenShoppingBalance: bigint;
  entityNexShoppingPool: bigint;
  entityTokenShoppingPool: bigint;
};

type TreasurySnapshot = {
  treasuryFree: bigint;
  treasuryReserved: bigint;
  treasuryFrozen: bigint;
  shopPendingTotal: bigint;
  unallocatedPool: bigint;
  pendingRefundTotal: bigint;
  legacyCommittedTotal: bigint;
  solvencyCommittedTotal: bigint;
  legacyUsableFree: bigint;
  solvencyUsableFree: bigint;
};

type BuyerChainDiagnostics = {
  referrer: string | null;
  directReferrals: number;
  indirectReferrals: number;
  teamSize: number;
  singleLineIndex: number | null;
  orderCount: number;
};

async function readBuyerChainDiagnostics(ctx: QueryLike, entityId: number, buyer: string): Promise<BuyerChainDiagnostics> {
  const member = codecToJson<Record<string, unknown>>(
    await (ctx.query as any).entityMember.entityMembers(entityId, buyer),
  );
  const referrerRaw = readObjectField(member, 'referrer');
  const referrer = referrerRaw ? String(referrerRaw) : null;
  const directReferrals = coerceNumber(readObjectField(member, 'directReferrals', 'direct_referrals')) ?? 0;
  const indirectReferrals = coerceNumber(readObjectField(member, 'indirectReferrals', 'indirect_referrals')) ?? 0;
  const teamSize = coerceNumber(readObjectField(member, 'teamSize', 'team_size')) ?? 0;
  const orderCount = coerceNumber(await (ctx.query as any).entityMember.memberOrderCount(entityId, buyer)) ?? 0;

  let singleLineIndex: number | null = null;
  const singleLine = (ctx.query as any).commissionSingleLine;
  if (singleLine?.singleLineIndex) {
    const rawIndex = await singleLine.singleLineIndex(entityId, buyer);
    singleLineIndex = coerceNumber(rawIndex) ?? null;
  }

  return {
    referrer,
    directReferrals,
    indirectReferrals,
    teamSize,
    singleLineIndex,
    orderCount,
  };
}

async function readShoppingSnapshot(ctx: QueryLike, entityId: number, buyer: string): Promise<ShoppingSnapshot> {
  const loyalty = (ctx.query as any).entityLoyalty;
  const buyerNexShoppingBalance = loyalty?.memberShoppingBalance
    ? asBigInt(await loyalty.memberShoppingBalance(entityId, buyer))
    : 0n;
  const buyerTokenShoppingBalance = loyalty?.memberTokenShoppingBalance
    ? asBigInt(await loyalty.memberTokenShoppingBalance(entityId, buyer))
    : 0n;
  const entityNexShoppingPool = loyalty?.shopShoppingTotal
    ? asBigInt(await loyalty.shopShoppingTotal(entityId))
    : 0n;
  const entityTokenShoppingPool = loyalty?.tokenShoppingTotal
    ? asBigInt(await loyalty.tokenShoppingTotal(entityId))
    : 0n;

  return {
    buyerNexShoppingBalance,
    buyerTokenShoppingBalance,
    entityNexShoppingPool,
    entityTokenShoppingPool,
  };
}

async function readTreasurySnapshot(ctx: QueryLike, entityId: number, treasuryAccount: string, shoppingPool: bigint): Promise<TreasurySnapshot> {
  const treasuryAccountInfo = codecToJson<Record<string, unknown>>(
    await (ctx.query as any).system.account(treasuryAccount),
  );
  const treasuryFree = asBigInt(readObjectField(treasuryAccountInfo, 'data', 'free') ?? 0);
  const treasuryReserved = asBigInt(readObjectField(treasuryAccountInfo, 'data', 'reserved') ?? 0);
  const treasuryFrozen = asBigInt(readObjectField(treasuryAccountInfo, 'data', 'frozen') ?? 0);
  const shopPendingTotal = asBigInt(await (ctx.query as any).commissionCore.shopPendingTotal(entityId));
  const unallocatedPool = asBigInt(await (ctx.query as any).commissionCore.unallocatedPool(entityId));
  const pendingRefundTotal = asBigInt(await (ctx.query as any).commissionCore.pendingRefundTotal(entityId));
  const legacyCommittedTotal = shopPendingTotal + unallocatedPool + pendingRefundTotal;
  const solvencyCommittedTotal = legacyCommittedTotal + shoppingPool;

  return {
    treasuryFree,
    treasuryReserved,
    treasuryFrozen,
    shopPendingTotal,
    unallocatedPool,
    pendingRefundTotal,
    legacyCommittedTotal,
    solvencyCommittedTotal,
    legacyUsableFree: treasuryFree - legacyCommittedTotal,
    solvencyUsableFree: treasuryFree - solvencyCommittedTotal,
  };
}

/**
 * 生成指定字符和长度的分隔线。
 */
function ln(char = '─', len = 76): string { return char.repeat(len); }

/** 双语标题 / Bilingual header */
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

/** 双语键值行："中文标签 / English Label: value" */
function kv(zh: string, en: string, value: string): void {
  console.log(`  ${zh} / ${en}:  ${value}`);
}

/* ------------------------------------------------------------------ */
/*  佣金类型中文映射 / Commission type CN labels                        */
/* ------------------------------------------------------------------ */

const COMMISSION_TYPE_CN: Record<string, string> = {
  OwnerReward:       'Owner 奖励',
  MultiLevel:          '多级分销',
  SingleLineUpline:    '单链上线',
  SingleLineDownline:  '单链下线',
  LevelDiff:           '级差奖',
  TeamPerformance:     '团队业绩奖',
  DirectReward:        '直推奖',
  FixedAmount:         '固定金额',
  FirstOrder:          '首单奖',
  RepeatPurchase:      '复购奖',
  EntityReferral:      '实体推荐奖',
  PoolReward:          '奖池分配',
};

const STATUS_CN: Record<string, string> = {
  Pending:     '待结算',
  Distributed: '已分配',
  Settled:     '已结算',
  Cancelled:   '已取消',
};

const ORDER_STATUS_CN: Record<string, string> = {
  Pending:    '待付款',
  Paid:       '已付款',
  Confirmed:  '已确认',
  Shipped:    '已发货',
  Completed:  '已完成',
  Cancelled:  '已取消',
  Refunded:   '已退款',
  Disputed:   '争议中',
};

/* ------------------------------------------------------------------ */
/*  主流程 / Main                                                      */
/* ------------------------------------------------------------------ */

/**
 * 主入口：追踪指定订单的资金、佣金与状态流转明细。
 */
async function main(): Promise<void> {
  const rawOrderId = process.argv[2] ?? process.env.ORDER_ID;
  const orderId = rawOrderId != null ? Number(rawOrderId) : NaN;
  if (isNaN(orderId) || orderId < 0) {
    console.error('用法 / Usage: npx tsx trace-order-funds.ts <order_id>');
    process.exit(1);
  }

  const api = await connectApi();

  try {
    header(`订单 #${orderId} 资金流追踪`, `Order #${orderId} Fund Flow Trace`);

    // ── 1. 订单基本信息 ──
    const orderValue = await (api.query as any).entityTransaction.orders(orderId);
    if (!orderValue || (orderValue as any).isNone) {
      console.error(`\n  订单 #${orderId} 在链上未找到 / Order #${orderId} not found on chain.`);
      await disconnectApi(api);
      process.exit(1);
    }
    const order = codecToJson<Record<string, unknown>>((orderValue as any).unwrap());

    const entityId = coerceNumber(readObjectField(order, 'entityId', 'entity_id'))!;
    const shopId = coerceNumber(readObjectField(order, 'shopId', 'shop_id'))!;
    const productId = coerceNumber(readObjectField(order, 'productId', 'product_id'))!;
    const buyer = String(readObjectField(order, 'buyer') ?? '');
    const seller = String(readObjectField(order, 'seller') ?? '');
    const payer = readObjectField(order, 'payer') as string | null;
    const quantity = coerceNumber(readObjectField(order, 'quantity')) ?? 0;
    const unitPrice = asBigInt(readObjectField(order, 'unitPrice', 'unit_price') ?? 0);
    const totalAmount = asBigInt(readObjectField(order, 'totalAmount', 'total_amount') ?? 0);
    const platformFee = asBigInt(readObjectField(order, 'platformFee', 'platform_fee') ?? 0);
    const status = String(readObjectField(order, 'status') ?? 'Unknown');
    const paymentAsset = String(readObjectField(order, 'paymentAsset', 'payment_asset') ?? 'Native');
    const shoppingBalanceUsed = asBigInt(readObjectField(order, 'shoppingBalanceUsed', 'shopping_balance_used') ?? 0);
    const tokenPaymentAmount = asBigInt(readObjectField(order, 'tokenPaymentAmount', 'token_payment_amount') ?? 0);
    const usdtTotal = coerceNumber(readObjectField(order, 'usdtTotal', 'usdt_total')) ?? 0;
    const nexUsdtRate = coerceNumber(readObjectField(order, 'nexUsdtRate', 'nex_usdt_rate')) ?? 0;
    const tokenNexRate = asBigInt(readObjectField(order, 'tokenNexRate', 'token_nex_rate') ?? 0);
    const createdAt = coerceNumber(readObjectField(order, 'createdAt', 'created_at')) ?? 0;
    const completedAt = readObjectField(order, 'completedAt', 'completed_at');
    const productCategory = String(readObjectField(order, 'productCategory', 'product_category') ?? '');
    const completedAtBlock = coerceNumber(completedAt);
    const anchorBlock = completedAtBlock && completedAtBlock > 0 ? completedAtBlock : createdAt;
    const beforeBlock = anchorBlock > 0 ? anchorBlock - 1 : 0;
    const ss58 = api.registry.chainSS58 ?? 42;
    const treasuryAccount = deriveTreasuryAddress(entityId, ss58);

    let beforeApi: QueryLike | null = null;
    let beforeSnapshotAvailable = false;
    if (beforeBlock > 0) {
      try {
        const beforeHash = await api.rpc.chain.getBlockHash(beforeBlock);
        beforeApi = await api.at(beforeHash);
        beforeSnapshotAvailable = true;
      } catch {
        beforeApi = null;
      }
    }

    subHeader('1. 订单信息', 'Order Info');
    kv('订单号',     'Order ID',       `${orderId}`);
    kv('实体ID',     'Entity ID',      `${entityId}`);
    kv('店铺ID',     'Shop ID',        `${shopId}`);
    kv('商品ID',     'Product ID',     `${productId}  (${productCategory})`);
    kv('数量',       'Quantity',        `${quantity}`);
    kv('状态',       'Status',          `${ORDER_STATUS_CN[status] ?? status} / ${status}`);
    kv('支付方式',   'Payment Asset',   paymentAsset === 'Native' ? 'NEX 原生 / Native' : `${paymentAsset}`);
    kv('买家',       'Buyer',           buyer);
    kv('卖家',       'Seller',          seller);
    if (payer) kv('代付人', 'Payer', payer);
    kv('创建区块',   'Created At',      `Block #${createdAt}`);
    if (completedAt != null) kv('完成区块', 'Completed At', `Block #${coerceNumber(completedAt) ?? completedAt}`);
    kv('审计锚点区块', 'Audit Anchor Block', anchorBlock > 0 ? `Block #${anchorBlock}` : 'N/A');
    kv('快照前一区块', 'Before Snapshot Block', beforeBlock > 0 ? `Block #${beforeBlock}` : 'N/A');
    if (!beforeSnapshotAvailable && beforeBlock > 0) {
      kv('历史快照状态', 'Historical Snapshot', '不可用，已回退为当前状态 / unavailable, fallback to current state');
    }

    // ── 2. 支付明细 ──
    const isTokenPayment = paymentAsset === 'EntityToken';
    const isShoppingPayment = paymentAsset === 'ShoppingBalance';
    // 购物余额支付时 total_amount=0 (by design)，实际金额在 shoppingBalanceUsed
    const paymentAmount = isShoppingPayment ? shoppingBalanceUsed : totalAmount;
    const effectiveAmount = paymentAmount;
    const sellerReceived = isShoppingPayment ? 0n : totalAmount - platformFee;

    subHeader('2. 支付明细', 'Payment Breakdown');

    if (isShoppingPayment) {
      kv('购物余额支付',   'Shopping Balance Used', formatNex(shoppingBalanceUsed));
      kv('单价',           'Unit Price',            formatNex(unitPrice));
      kv('链上 total_amount', 'On-chain total_amount', `${formatNex(totalAmount)}  (购物余额通道为 0 / 0 by design)`);
    } else if (isTokenPayment) {
      kv('Token 支付',      'Token Payment',   formatToken(tokenPaymentAmount));
      kv('Token/NEX 汇率',  'Token/NEX Rate',  `${Number(tokenNexRate) / 1e12}`);
      kv('NEX 等值',        'NEX Equivalent',  formatNex(totalAmount));
    } else {
      kv('单价',           'Unit Price',      formatNex(unitPrice));
      kv('总金额',         'Total Amount',    formatNex(totalAmount));
    }
    kv('平台费',           'Platform Fee',    formatNex(platformFee));
    kv('USDT 等值',        'USDT Total',      `${(usdtTotal / 1e6).toFixed(2)} USDT`);
    kv('NEX/USDT 汇率',   'NEX/USDT Rate',   `${(nexUsdtRate / 1e6).toFixed(6)}`);
    if (!isShoppingPayment) {
      kv('卖家实收',       'Seller Receives', `${formatNex(sellerReceived)}  (总额 - 平台费 / total - platform_fee)`);
    }

    if (isTokenPayment) {
      const tokenPlatformFee = tokenPaymentAmount * platformFee / (totalAmount > 0n ? totalAmount : 1n);
      const tokenSellerReceived = tokenPaymentAmount - tokenPlatformFee;
      kv('Token 平台费',    'Token Platform Fee', `~${formatToken(tokenPlatformFee)}`);
      kv('Token 卖家实收',  'Token Seller Recv',  `~${formatToken(tokenSellerReceived)}`);
    }

    // ── 3. 佣金配置 ──
    subHeader('3. 佣金配置', 'Commission Config');
    const commConfig = codecToJson<Record<string, unknown>>(
      await (api.query as any).commissionCore.commissionConfigs(entityId),
    );
    const maxRate = coerceNumber(readObjectField(commConfig, 'maxCommissionRate', 'max_commission_rate')) ?? 0;
    const enabled = readObjectField(commConfig, 'enabled');
    const commPool = effectiveAmount * BigInt(maxRate) / 10000n;
    const commPoolPct = formatPct(commPool, effectiveAmount, 1);
    const feePct = formatPct(platformFee, effectiveAmount, 1);
    const buyerChain = await readBuyerChainDiagnostics(api as QueryLike, entityId, buyer);

    kv('最大佣金比率',  'Max Commission Rate', `${maxRate} bps (${(maxRate / 100).toFixed(1)}%)`);
    kv('已启用插件',    'Enabled Plugins',     JSON.stringify(enabled));
    kv('佣金池',        'Commission Pool',     `${formatNex(commPool)} (占订单 ${commPoolPct}% / ${commPoolPct}% of order)`);
    if (isShoppingPayment) {
      kv('佣金池基数',  'Pool Base',           `shopping_balance_used = ${formatNex(shoppingBalanceUsed)}`);
      kv('资金来源',    'Fund Source',         'Entity 金库 / Entity Treasury');
    }
    kv('平台费',        'Platform Fee',        `${formatNex(platformFee)} (占订单 ${feePct}% / ${feePct}% of order)`);

    // ── 4. NEX 佣金分配 ──
    subHeader('4. NEX 佣金分配', 'NEX Commission Distribution');

    const nexRecords = codecToJson<any[]>(
      await (api.query as any).commissionCore.orderCommissionRecords(orderId),
    ) ?? [];

    if (nexRecords.length === 0) {
      console.log(isShoppingPayment
        ? '  (购物余额支付不产生 NEX 佣金记录 / ShoppingBalance orders use separate records)'
        : '  (无 NEX 佣金记录 / No NEX commission records)');
    } else {
      const byType = new Map<string, { beneficiaries: { addr: string; amount: bigint; level: number; status: string }[]; total: bigint }>();

      for (const rec of nexRecords) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? 'Unknown');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
        const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
        const recStatus = String(readObjectField(rec, 'status') ?? '');

        if (!byType.has(ctype)) byType.set(ctype, { beneficiaries: [], total: 0n });
        const group = byType.get(ctype)!;
        group.beneficiaries.push({ addr: beneficiary, amount, level, status: recStatus });
        group.total += amount;
      }

      let grandTotal = 0n;
      const typeOrder = [
        'OwnerReward', 'MultiLevel', 'SingleLineUpline', 'SingleLineDownline',
        'LevelDiff', 'TeamPerformance', 'DirectReward', 'FixedAmount',
        'FirstOrder', 'RepeatPurchase', 'EntityReferral', 'PoolReward',
      ];
      const sortedTypes = [...byType.keys()].sort((a, b) => {
        const ia = typeOrder.indexOf(a);
        const ib = typeOrder.indexOf(b);
        return (ia === -1 ? 999 : ia) - (ib === -1 ? 999 : ib);
      });

      for (const ctype of sortedTypes) {
        const group = byType.get(ctype)!;
        grandTotal += group.total;
        const pct = formatPct(group.total, effectiveAmount);
        const cnName = COMMISSION_TYPE_CN[ctype] ?? ctype;
        console.log(`\n  [${cnName} / ${ctype}]  小计 / Subtotal: ${formatNex(group.total)}  (${pct}%)`);

        group.beneficiaries.sort((a, b) => a.level - b.level);
        for (const b of group.beneficiaries) {
          const levelStr = b.level > 0 ? `L${b.level}` : '   ';
          const sCn = STATUS_CN[b.status] ?? b.status;
          console.log(`    ${levelStr}  ${shortAddr(b.addr)}  ${formatNex(b.amount).padStart(20)}  [${sCn} / ${b.status}]`);
        }
      }

      console.log(`\n  ${'─'.repeat(56)}`);
      const grandPct = formatPct(grandTotal, effectiveAmount);
      console.log(`  NEX 佣金合计 / NEX Commission Total:  ${formatNex(grandTotal)}  (${grandPct}%)`);
      console.log(`  记录数 / Records:  ${nexRecords.length}`);
    }

    // ── 4.5. 购物余额佣金分配 ──
    subHeader('4.5. 购物余额佣金分配', 'Shopping Balance Commission Distribution');

    const shoppingRecords = codecToJson<any[]>(
      await (api.query as any).commissionCore.orderShoppingCommissionRecords(orderId),
    ) ?? [];

    if (shoppingRecords.length === 0) {
      console.log(isShoppingPayment
        ? '  (无购物余额佣金记录 / No shopping balance commission records)'
        : '  (非购物余额支付，无记录 / Not a ShoppingBalance order)');
    } else {
      const byType = new Map<string, { beneficiaries: { addr: string; amount: bigint; level: number; status: string }[]; total: bigint }>();
      const distinctBeneficiaries = uniqueNonEmpty(shoppingRecords.map((rec) => String(readObjectField(rec, 'beneficiary') ?? '')));

      for (const rec of shoppingRecords) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? 'Unknown');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
        const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
        const recStatus = String(readObjectField(rec, 'status') ?? '');

        if (!byType.has(ctype)) byType.set(ctype, { beneficiaries: [], total: 0n });
        const group = byType.get(ctype)!;
        group.beneficiaries.push({ addr: beneficiary, amount, level, status: recStatus });
        group.total += amount;
      }

      let grandTotal = 0n;
      const typeOrder = [
        'OwnerReward', 'MultiLevel', 'SingleLineUpline', 'SingleLineDownline',
        'LevelDiff', 'TeamPerformance', 'DirectReward', 'FixedAmount',
        'FirstOrder', 'RepeatPurchase', 'EntityReferral', 'PoolReward',
      ];
      const sortedTypes = [...byType.keys()].sort((a, b) => {
        const ia = typeOrder.indexOf(a);
        const ib = typeOrder.indexOf(b);
        return (ia === -1 ? 999 : ia) - (ib === -1 ? 999 : ib);
      });

      for (const ctype of sortedTypes) {
        const group = byType.get(ctype)!;
        grandTotal += group.total;
        const pct = formatPct(group.total, effectiveAmount);
        const cnName = COMMISSION_TYPE_CN[ctype] ?? ctype;
        console.log(`\n  [${cnName} / ${ctype}]  小计 / Subtotal: ${formatNex(group.total)}  (${pct}%)`);
        console.log(`    去向 / Destination: memberShoppingBalance(entityId, beneficiary)`);

        group.beneficiaries.sort((a, b) => a.level - b.level);
        for (const b of group.beneficiaries) {
          const levelStr = b.level > 0 ? `L${b.level}` : '   ';
          const sCn = STATUS_CN[b.status] ?? b.status;
          console.log(`    ${levelStr}  ${shortAddr(b.addr)}  ${formatNex(b.amount).padStart(20)}  [${sCn} / ${b.status}]`);
        }
      }

      console.log(`\n  ${'─'.repeat(56)}`);
      const grandPct = formatPct(grandTotal, effectiveAmount);
      console.log(`  购物余额佣金合计 / Shopping Commission Total:  ${formatNex(grandTotal)}  (${grandPct}%)`);
      console.log(`  受益人数 / Beneficiaries:  ${distinctBeneficiaries.length}`);
      console.log(`  记录数 / Records:  ${shoppingRecords.length}`);

      const hasShoppingMultiLevel = shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'MultiLevel');
      const hasShoppingSingleLineUpline = shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'SingleLineUpline');
      const hasShoppingSingleLineDownline = shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'SingleLineDownline');
      if (hasShoppingSingleLineDownline && !hasShoppingSingleLineUpline) {
        console.log('  诊断 / Diagnosis: 仅出现 SingleLineDownline，未出现 SingleLineUpline。');
        if (buyerChain.singleLineIndex === 0) {
          console.log('    - singleLineIndex = 0，买家位于链头，没有上级单链节点。');
        }
        if (!buyerChain.referrer) {
          console.log('    - buyer referrer = null，没有上级链路。');
        }
      }
      if (!hasShoppingMultiLevel) {
        console.log('  诊断 / Diagnosis: 未出现 MultiLevel 记录。');
        if (!buyerChain.referrer) {
          console.log('    - buyer referrer = null，上级链为空，MultiLevel 无法向上分配。');
        } else {
          console.log('    - 若预期应有 MultiLevel，请继续检查上级资格/链路有效性。');
        }
      }
    }

    // ── 5. Token 佣金分配 ──
    subHeader('5. Token 佣金分配', 'Token Commission Distribution');

    const tokenRecords = codecToJson<any[]>(
      await (api.query as any).commissionCore.orderTokenCommissionRecords(orderId),
    ) ?? [];

    if (tokenRecords.length === 0) {
      console.log('  (无 Token 佣金记录 / No Token commission records)');
    } else {
      const byType = new Map<string, { beneficiaries: { addr: string; amount: bigint; level: number; status: string }[]; total: bigint }>();

      for (const rec of tokenRecords) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? 'Unknown');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
        const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
        const recStatus = String(readObjectField(rec, 'status') ?? '');

        if (!byType.has(ctype)) byType.set(ctype, { beneficiaries: [], total: 0n });
        const group = byType.get(ctype)!;
        group.beneficiaries.push({ addr: beneficiary, amount, level, status: recStatus });
        group.total += amount;
      }

      let grandTotal = 0n;
      for (const [ctype, group] of byType) {
        grandTotal += group.total;
        const cnName = COMMISSION_TYPE_CN[ctype] ?? ctype;
        console.log(`\n  [${cnName} / ${ctype}]  小计 / Subtotal: ${formatToken(group.total)}`);
        group.beneficiaries.sort((a, b) => a.level - b.level);
        for (const b of group.beneficiaries) {
          const levelStr = b.level > 0 ? `L${b.level}` : '   ';
          const sCn = STATUS_CN[b.status] ?? b.status;
          console.log(`    ${levelStr}  ${shortAddr(b.addr)}  ${formatToken(b.amount).padStart(20)}  [${sCn} / ${b.status}]`);
        }
      }

      console.log(`\n  ${'─'.repeat(56)}`);
      console.log(`  Token 佣金合计 / Token Commission Total: ${formatToken(grandTotal)}`);
      console.log(`  记录数 / Records:  ${tokenRecords.length}`);
    }

    // ── 6. 未分配资金池 ──
    subHeader('6. 未分配资金池（本订单）', 'Unallocated Pool (this order)');

    const orderUnallocated = codecToJson<any>(
      await (api.query as any).commissionCore.orderUnallocated(orderId),
    );

    if (orderUnallocated) {
      const unallocEntityId = coerceNumber(readObjectField(orderUnallocated, '0') ?? readObjectField(orderUnallocated, 'entityId', 'entity_id'));
      const unallocShopId = coerceNumber(readObjectField(orderUnallocated, '1') ?? readObjectField(orderUnallocated, 'shopId', 'shop_id'));
      const unallocAmount = asBigInt(readObjectField(orderUnallocated, '2') ?? readObjectField(orderUnallocated, 'amount') ?? 0);

      if (unallocAmount > 0n) {
        const pct = formatPct(unallocAmount, effectiveAmount);
        kv('未分配金额', 'Unallocated', `${formatNex(unallocAmount)} (${pct}%)`);
        kv('所属实体',   'Entity',      `${unallocEntityId}`);
        kv('所属店铺',   'Shop',        `${unallocShopId}`);
      } else {
        console.log('  (本订单无未分配资金 / No unallocated funds for this order)');
      }
    } else {
      console.log('  (无未分配记录 / No unallocated record)');
    }

    const entityPool = asBigInt(await (api.query as any).commissionCore.unallocatedPool(entityId));
    kv('实体资金池总额', 'Entity Total Pool', formatNex(entityPool));

    // ── 7. 受益人佣金统计 ──
    subHeader('7. 受益人佣金统计', 'Beneficiary Commission Stats');

    const beneficiaries = uniqueNonEmpty([
      seller,
      ...nexRecords.map((rec) => String(readObjectField(rec, 'beneficiary') ?? '')),
      ...shoppingRecords.map((rec) => String(readObjectField(rec, 'beneficiary') ?? '')),
      ...tokenRecords.map((rec) => String(readObjectField(rec, 'beneficiary') ?? '')),
    ]);

    const colH = (zh: string, en: string) => `${zh}/${en}`;
    console.log(`\n  ${'账户/Account'.padEnd(18)} ${colH('累计','Earned').padStart(18)} ${colH('待提','Pending').padStart(18)} ${colH('已提','Withdrawn').padStart(18)} ${colH('复购','Repurchased').padStart(18)}`);
    console.log(`  ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)}`);

    for (const addr of beneficiaries) {
      const stats = codecToJson<Record<string, unknown>>(
        await (api.query as any).commissionCore.memberCommissionStats(entityId, addr),
      );
      const totalEarned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
      const pending = asBigInt(readObjectField(stats, 'pending') ?? 0);
      const withdrawn = asBigInt(readObjectField(stats, 'withdrawn') ?? 0);
      const repurchased = asBigInt(readObjectField(stats, 'repurchased') ?? 0);

      console.log(
        `  ${shortAddr(addr).padEnd(18)} ${formatNex(totalEarned).padStart(18)} ${formatNex(pending).padStart(18)} ${formatNex(withdrawn).padStart(18)} ${formatNex(repurchased).padStart(18)}`,
      );
    }

    if (tokenRecords.length > 0) {
      console.log(`\n  Token 统计 / Token Stats:`);
      console.log(`  ${'账户/Account'.padEnd(18)} ${colH('累计','Earned').padStart(18)} ${colH('待提','Pending').padStart(18)} ${colH('已提','Withdrawn').padStart(18)} ${colH('复购','Repurchased').padStart(18)}`);
      console.log(`  ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)} ${'─'.repeat(18)}`);

      for (const addr of beneficiaries) {
        const stats = codecToJson<Record<string, unknown>>(
          await (api.query as any).commissionCore.memberTokenCommissionStats(entityId, addr),
        );
        const totalEarned = asBigInt(readObjectField(stats, 'totalEarned', 'total_earned') ?? 0);
        const pending = asBigInt(readObjectField(stats, 'pending') ?? 0);
        const withdrawn = asBigInt(readObjectField(stats, 'withdrawn') ?? 0);
        const repurchased = asBigInt(readObjectField(stats, 'repurchased') ?? 0);

        if (totalEarned > 0n || pending > 0n) {
          console.log(
            `  ${shortAddr(addr).padEnd(18)} ${formatToken(totalEarned).padStart(18)} ${formatToken(pending).padStart(18)} ${formatToken(withdrawn).padStart(18)} ${formatToken(repurchased).padStart(18)}`,
          );
        }
      }
    }

    // ── 7.5. 买家链路诊断 ──
    subHeader('7.5. 买家链路诊断', 'Buyer Chain Diagnostics');
    kv('买家 referrer', 'Buyer Referrer', buyerChain.referrer ?? '无 / None');
    kv('买家直推数', 'Buyer Direct Referrals', `${buyerChain.directReferrals}`);
    kv('买家间推数', 'Buyer Indirect Referrals', `${buyerChain.indirectReferrals}`);
    kv('买家团队规模', 'Buyer Team Size', `${buyerChain.teamSize}`);
    kv('买家订单数', 'Buyer Order Count', `${buyerChain.orderCount}`);
    kv('单链索引', 'SingleLine Index', buyerChain.singleLineIndex != null ? `${buyerChain.singleLineIndex}` : 'N/A');

    if (shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'SingleLineDownline')
      && !shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'SingleLineUpline')) {
      console.log('  诊断 / Diagnosis: 本单只有 SingleLineDownline，没有 SingleLineUpline。');
      if (buyerChain.singleLineIndex === 0) {
        console.log('    - 买家 singleLineIndex = 0，处于链头 / buyer is at the head of the line.');
      }
      if (!buyerChain.referrer) {
        console.log('    - 买家 referrer = null，没有上级链路 / buyer has no upline referrer.');
      }
    }

    if (!shoppingRecords.some((rec) => String(readObjectField(rec, 'commissionType', 'commission_type') ?? '') === 'MultiLevel')) {
      console.log('  诊断 / Diagnosis: 本单没有 MultiLevel 记录。');
      if (!buyerChain.referrer) {
        console.log('    - 买家没有 referrer，上级链为空，MultiLevel 无法向上分配。');
      } else {
        console.log('    - buyer 有 referrer；若仍无 MultiLevel，通常需继续检查多级资格/链路有效性。');
      }
    }

    // ── 8. 购物余额 ──
    subHeader('8. 购物余额落点', 'Shopping Balance Landing Points');

    const currentShoppingSnapshot = await readShoppingSnapshot(api as QueryLike, entityId, buyer);
    const beforeShoppingSnapshot = beforeApi
      ? await readShoppingSnapshot(beforeApi, entityId, buyer)
      : currentShoppingSnapshot;

    kv('买家 NEX 购物余额（前）', 'Buyer NEX Shopping Before', formatNex(beforeShoppingSnapshot.buyerNexShoppingBalance));
    kv('买家 NEX 购物余额（后）', 'Buyer NEX Shopping After', formatNex(currentShoppingSnapshot.buyerNexShoppingBalance));
    kv('买家 NEX 购物余额增量', 'Buyer NEX Shopping Delta', formatDelta(
      currentShoppingSnapshot.buyerNexShoppingBalance - beforeShoppingSnapshot.buyerNexShoppingBalance,
      formatNex,
    ));
    kv('买家 Token 购物余额（后）', 'Buyer Token Shopping After', formatToken(currentShoppingSnapshot.buyerTokenShoppingBalance));
    kv('实体 NEX 购物余额池（前）', 'Entity NEX Shopping Pool Before', formatNex(beforeShoppingSnapshot.entityNexShoppingPool));
    kv('实体 NEX 购物余额池（后）', 'Entity NEX Shopping Pool After', formatNex(currentShoppingSnapshot.entityNexShoppingPool));
    kv('实体 NEX 购物余额池变化', 'Entity NEX Shopping Pool Delta', formatDelta(
      currentShoppingSnapshot.entityNexShoppingPool - beforeShoppingSnapshot.entityNexShoppingPool,
      formatNex,
    ));
    kv('实体 Token 购物余额池（后）', 'Entity Token Shopping Pool After', formatToken(currentShoppingSnapshot.entityTokenShoppingPool));

    subHeader('8.5. 实体可用余额', 'Entity Usable Balance');
    const currentTreasurySnapshot = await readTreasurySnapshot(
      api as QueryLike,
      entityId,
      treasuryAccount,
      currentShoppingSnapshot.entityNexShoppingPool,
    );
    const beforeTreasurySnapshot = beforeApi
      ? await readTreasurySnapshot(beforeApi, entityId, treasuryAccount, beforeShoppingSnapshot.entityNexShoppingPool)
      : currentTreasurySnapshot;

    kv('金库账户', 'Treasury Account', treasuryAccount);
    kv('金库可用（前）', 'Treasury Free Before', formatNex(beforeTreasurySnapshot.treasuryFree));
    kv('金库可用（后）', 'Treasury Free After', formatNex(currentTreasurySnapshot.treasuryFree));
    kv('金库可用变化', 'Treasury Free Delta', formatDelta(
      currentTreasurySnapshot.treasuryFree - beforeTreasurySnapshot.treasuryFree,
      formatNex,
    ));
    kv('金库保留', 'Treasury Reserved', formatNex(currentTreasurySnapshot.treasuryReserved));
    kv('金库冻结', 'Treasury Frozen', formatNex(currentTreasurySnapshot.treasuryFrozen));
    kv('待提现佣金', 'Shop Pending Total', formatNex(currentTreasurySnapshot.shopPendingTotal));
    kv('未分配奖池', 'Unallocated Pool', formatNex(currentTreasurySnapshot.unallocatedPool));
    kv('待退款', 'Pending Refund Total', formatNex(currentTreasurySnapshot.pendingRefundTotal));
    kv('已占用合计（旧口径）', 'Committed Total (Legacy)', formatNex(currentTreasurySnapshot.legacyCommittedTotal));
    kv('可自由使用（旧口径）', 'Usable Free (Legacy)', formatNex(currentTreasurySnapshot.legacyUsableFree));
    kv('已占用合计（含购物池）', 'Committed Total (Solvency)', formatNex(currentTreasurySnapshot.solvencyCommittedTotal));
    kv('可自由使用（含购物池）前', 'Usable Free (Solvency) Before', formatNex(beforeTreasurySnapshot.solvencyUsableFree));
    kv('可自由使用（含购物池）后', 'Usable Free (Solvency) After', formatNex(currentTreasurySnapshot.solvencyUsableFree));
    kv('可自由使用（含购物池）变化', 'Usable Free (Solvency) Delta', formatDelta(
      currentTreasurySnapshot.solvencyUsableFree - beforeTreasurySnapshot.solvencyUsableFree,
      formatNex,
    ));

    // ── 9. 资金流向汇总 ──
    header('资金流向汇总', 'Fund Flow Summary');

    let nexCommTotal = 0n;
    for (const rec of nexRecords) nexCommTotal += asBigInt(readObjectField(rec, 'amount') ?? 0);
    let shoppingCommTotal = 0n;
    for (const rec of shoppingRecords) shoppingCommTotal += asBigInt(readObjectField(rec, 'amount') ?? 0);
    let tokenCommTotal = 0n;
    for (const rec of tokenRecords) tokenCommTotal += asBigInt(readObjectField(rec, 'amount') ?? 0);

    let thisOrderUnalloc = 0n;
    if (orderUnallocated) {
      thisOrderUnalloc = asBigInt(readObjectField(orderUnallocated, '2') ?? readObjectField(orderUnallocated, 'amount') ?? 0);
    }

    console.log(`\n  支付金额 / Payment:           ${formatNex(paymentAmount)}${isTokenPayment ? ` (${formatToken(tokenPaymentAmount)})` : ''}`);
    if (isShoppingPayment) {
      console.log(`    支付来源 / Payment Source:   买家购物余额 / Buyer Shopping Balance`);
      console.log(`    卖家实收 / Seller Recv:     ${formatNex(0n)}  (购物余额订单不直接给卖家 / not paid directly to seller)`);
    } else {
      console.log(`    平台费 / Platform Fee:      ${formatNex(platformFee)}`);
      console.log(`    卖家实收 / Seller Recv:     ${formatNex(sellerReceived)}`);
    }
    console.log(`    佣金池 / Commission Pool:   ${formatNex(commPool)}`);
    console.log(`      NEX 已分配 / Distributed:    ${formatNex(nexCommTotal)}`);
    if (shoppingCommTotal > 0n || isShoppingPayment) {
      console.log(`      购物余额已分配 / Distributed: ${formatNex(shoppingCommTotal)}`);
      console.log(`      记录来源 / Source:            orderShoppingCommissionRecords(${orderId})`);
      console.log(`      去向说明 / Landing:           beneficiary -> memberShoppingBalance(entityId, account)`);
    }
    if (tokenRecords.length > 0) {
      console.log(`      Token 已分配 / Distributed:  ${formatToken(tokenCommTotal)}`);
    }
    console.log(`      未分配 / Unallocated:        ${formatNex(thisOrderUnalloc)}`);

    console.log(`\n  对账验证 / Verification:`);
    console.log(`    订单支付额 / Order Payment:                    ${formatNex(paymentAmount)}`);
    if (!isShoppingPayment) {
      console.log(`    = 平台费 + 卖家实收 / Fee + Seller:           ${formatNex(platformFee)} + ${formatNex(sellerReceived)} = ${formatNex(platformFee + sellerReceived)}`);
    }
    console.log(`    佣金池 / Commission Pool (max_rate):          ${formatNex(commPool)}`);
    console.log(`    = 已分配 + 未分配 / Distributed + Unalloc:    ${formatNex(nexCommTotal + shoppingCommTotal)} + ${formatNex(thisOrderUnalloc)} = ${formatNex(nexCommTotal + shoppingCommTotal + thisOrderUnalloc)}`);

    const accountedCommission = nexCommTotal + shoppingCommTotal;
    const gap = commPool - accountedCommission - thisOrderUnalloc;
    if (gap !== 0n) {
      console.log(`    差额 / Gap (池 - 已分配 - 未分配):             ${formatNex(gap)}`);
    } else {
      console.log(`    佣金池已全部分配 / Pool fully accounted for.`);
    }

    if (isShoppingPayment) {
      subHeader('9.5. 购物余额落点汇总', 'Shopping Landing Summary');
      console.log(`  购物余额分佣总额 / Shopping Commission: ${formatNex(shoppingCommTotal)}`);
      console.log(`  买家购物余额增量 / Buyer Balance Delta: ${formatDelta(
        currentShoppingSnapshot.buyerNexShoppingBalance - beforeShoppingSnapshot.buyerNexShoppingBalance,
        formatNex,
      )}`);
      console.log(`  实体购物池变化 / Entity Pool Delta:     ${formatDelta(
        currentShoppingSnapshot.entityNexShoppingPool - beforeShoppingSnapshot.entityNexShoppingPool,
        formatNex,
      )}`);
      console.log(`  金库可用变化 / Treasury Usable Delta:   ${formatDelta(
        currentTreasurySnapshot.solvencyUsableFree - beforeTreasurySnapshot.solvencyUsableFree,
        formatNex,
      )}`);
    }

    console.log(`\n${ln('═')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('错误 / Error:', err);
  process.exit(1);
});
