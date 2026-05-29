#!/usr/bin/env tsx

process.env.WS_URL ??= 'ws://127.0.0.1:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';

const ENTITY_ID = Number(process.argv[2] ?? process.env.ENTITY_ID ?? '100000');
const LIMIT = Number(process.argv[3] ?? process.env.LIMIT ?? '80');

function asString(v: unknown): string {
  return v == null ? '' : String(v);
}

function normalizeStatus(v: unknown): string {
  const s = asString(v);
  return s.replace(/[{}"\s]/g, '');
}

async function main(): Promise<void> {
  const api = await connectApi();
  try {
    const rows: Array<Record<string, unknown>> = [];
    const entries = await (api.query as any).entityProduct.products.entries();

    for (const [key, value] of entries) {
      if ((value as any).isNone) continue;
      const productId = coerceNumber(key.args?.[0]) ?? 0;
      const product = codecToJson<Record<string, unknown>>((value as any).unwrap());
      const entityId = coerceNumber(readObjectField(product, 'entityId', 'entity_id')) ?? 0;
      if (entityId !== ENTITY_ID) continue;

      const status = asString(readObjectField(product, 'status'));
      const visibility = asString(readObjectField(product, 'visibility'));
      const category = asString(readObjectField(product, 'category'));
      const seller = asString(readObjectField(product, 'seller'));
      const shopId = coerceNumber(readObjectField(product, 'shopId', 'shop_id')) ?? 0;
      const minOrderQuantity = coerceNumber(readObjectField(product, 'minOrderQuantity', 'min_order_quantity')) ?? 1;
      const maxOrderQuantity = coerceNumber(readObjectField(product, 'maxOrderQuantity', 'max_order_quantity'));
      const usdtPrice = readObjectField(product, 'usdtPrice', 'usdt_price');
      const name = readObjectField(product, 'name');
      const displayName = typeof name === 'string' ? name : JSON.stringify(name);

      rows.push({
        productId,
        shopId,
        seller,
        entityId,
        status,
        visibility,
        category,
        minOrderQuantity,
        maxOrderQuantity,
        usdtPrice,
        name: displayName,
        likelyRunnable:
          normalizeStatus(status).includes('OnSale') && visibility.includes('MembersOnly'),
      });
    }

    rows.sort((a, b) => Number(a.productId) - Number(b.productId));

    console.log(JSON.stringify({
      entityId: ENTITY_ID,
      total: rows.length,
      runnable: rows.filter((row) => Boolean(row.likelyRunnable)).length,
      rows: rows.slice(0, LIMIT),
    }, null, 2));
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
