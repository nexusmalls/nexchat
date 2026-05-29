#!/usr/bin/env tsx

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';

const ACCOUNT = process.argv[2] ?? 'X4TjpDzhtqvxzzt49Fop2653xuV8AS8SXcebzK7JVpKGYa8y1';
const ENTITY_ID = Number(process.argv[3] ?? '100000');

function asString(v: unknown): string {
  return v == null ? '' : String(v);
}

function asNumber(v: unknown, fallback = 0): number {
  const n = coerceNumber(v);
  return n == null ? fallback : n;
}

async function main(): Promise<void> {
  const api = await connectApi();
  try {
    const orderQuery = (api.query as any).entityTransaction;
    const memberQuery = (api.query as any).entityMember;

    const buyerOrderIdsCodec = await orderQuery.buyerOrders(ACCOUNT);
    const buyerOrderIds = codecToJson<number[]>(buyerOrderIdsCodec) ?? [];

    const memberCodec = await memberQuery.entityMembers(ENTITY_ID, ACCOUNT);
    const member = codecToJson<Record<string, unknown>>(memberCodec as any);

    console.log(JSON.stringify({
      account: ACCOUNT,
      entityId: ENTITY_ID,
      member,
      buyerOrderIds,
    }, null, 2));

    let totalUsdt = 0;
    let eligibleUsdt = 0;
    let shoppingUsdt = 0;

    const rows: Array<Record<string, unknown>> = [];

    for (const orderId of buyerOrderIds) {
      const orderCodec = await orderQuery.orders(orderId);
      if (!orderCodec || (orderCodec as any).isNone) continue;
      const order = codecToJson<Record<string, unknown>>((orderCodec as any).unwrap());

      const entityId = asNumber(readObjectField(order, 'entityId', 'entity_id'));
      const buyer = asString(readObjectField(order, 'buyer'));
      if (entityId !== ENTITY_ID || buyer !== ACCOUNT) continue;

      const status = asString(readObjectField(order, 'status'));
      const paymentAsset = asString(readObjectField(order, 'paymentAsset', 'payment_asset'));
      const usdtTotal = asNumber(readObjectField(order, 'usdtTotal', 'usdt_total'));
      const productId = asNumber(readObjectField(order, 'productId', 'product_id'));
      const shopId = asNumber(readObjectField(order, 'shopId', 'shop_id'));
      const shoppingBalanceUsed = asString(readObjectField(order, 'shoppingBalanceUsed', 'shopping_balance_used') ?? '0');
      const totalAmount = asString(readObjectField(order, 'totalAmount', 'total_amount') ?? '0');
      const createdAt = asNumber(readObjectField(order, 'createdAt', 'created_at'));
      const completedAt = readObjectField(order, 'completedAt', 'completed_at');

      const completed = status === 'Completed';
      const isShopping = paymentAsset === 'ShoppingBalance';
      if (completed) {
        totalUsdt += usdtTotal;
        if (isShopping) {
          shoppingUsdt += usdtTotal;
        } else {
          eligibleUsdt += usdtTotal;
        }
      }

      rows.push({
        orderId,
        shopId,
        productId,
        status,
        paymentAsset,
        usdtTotal,
        usdtTotalDisplay: (usdtTotal / 1e6).toFixed(2),
        eligibleForUpgrade: completed && !isShopping,
        shoppingBalanceUsed,
        totalAmount,
        createdAt,
        completedAt,
      });
    }

    rows.sort((a, b) => Number(a.orderId) - Number(b.orderId));

    console.log('\n=== order rows ===');
    console.log(JSON.stringify(rows, null, 2));

    console.log('\n=== summary ===');
    console.log(JSON.stringify({
      completedTotalUsdt: totalUsdt,
      completedTotalUsdtDisplay: (totalUsdt / 1e6).toFixed(2),
      completedEligibleUsdt: eligibleUsdt,
      completedEligibleUsdtDisplay: (eligibleUsdt / 1e6).toFixed(2),
      completedShoppingUsdt: shoppingUsdt,
      completedShoppingUsdtDisplay: (shoppingUsdt / 1e6).toFixed(2),
    }, null, 2));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
