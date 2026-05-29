import type { ApiPromise } from '@polkadot/api';
import { assert } from '../framework/assert.js';
import { codecToHuman, codecToJson, coerceNumber, describeValue, readObjectField } from '../framework/codec.js';

export const VALID_TRON_ADDRESSES = {
  seller: 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',
  buyer: 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE',
};

export interface MarketRecord {
  json: Record<string, unknown>;
  human: Record<string, unknown>;
}

/**
 * 读取某个地址在实体市场中的订单索引列表。
 */
export async function readUserOrders(api: ApiPromise, address: string): Promise<number[]> {
  const value = await (api.query as any).entityMarket.userOrders(address);
  const json = codecToJson<unknown[]>(value);
  return Array.isArray(json) ? json.map((item) => Number(item)) : [];
}

/**
 * 读取单笔实体市场订单，并兼容不同运行时下的查询入口。
 */
export async function readMarketOrder(api: ApiPromise, orderId: number): Promise<MarketRecord> {
  const value = await (api.query as any).entityMarket.orderDetails(orderId)
    ?? await (api.query as any).entityMarket.orders(orderId);
  const actual = value && (value as any).isSome !== undefined ? value : await (api.query as any).entityMarket.orders(orderId);
  assert((actual as any).isSome, `entity market order ${orderId} should exist`);
  const order = (actual as any).unwrap();
  return {
    json: codecToJson<Record<string, unknown>>(order),
    human: codecToHuman<Record<string, unknown>>(order),
  };
}

/**
 * 尝试读取实体市场订单，不存在时返回 undefined。
 */
export async function tryReadMarketOrder(api: ApiPromise, orderId: number): Promise<MarketRecord | undefined> {
  const query = (api.query as any).entityMarket.orderDetails ?? (api.query as any).entityMarket.orders;
  const value = await query(orderId);
  if (!(value as any).isSome) {
    return undefined;
  }
  const order = (value as any).unwrap();
  return {
    json: codecToJson<Record<string, unknown>>(order),
    human: codecToHuman<Record<string, unknown>>(order),
  };
}

/**
 * 读取订单中的指定字段并转成适合断言输出的字符串。
 */
export function describeMarketField(record: MarketRecord, field: string): string {
  const value = readObjectField(record.human, field) ?? readObjectField(record.json, field);
  return describeValue(value);
}

/**
 * 判断订单字段描述中是否包含指定关键字，忽略大小写。
 */
export function marketFieldContains(record: MarketRecord, field: string, keyword: string): boolean {
  return describeMarketField(record, field).toLowerCase().includes(keyword.toLowerCase());
}

/**
 * 判断订单挂单人是否与目标地址一致。
 */
function makerMatches(record: MarketRecord, address: string): boolean {
  const maker = readObjectField(record.human, 'maker', 'seller') ?? readObjectField(record.json, 'maker', 'seller');
  return String(maker) === address;
}

/**
 * 判断订单方向是否匹配买单或卖单关键字。
 */
function sideMatches(record: MarketRecord, sideKeyword: 'buy' | 'sell'): boolean {
  return describeMarketField(record, 'side').toLowerCase().includes(sideKeyword);
}

/**
 * 在最近生成的订单区间内，反向查找某个挂单人创建的指定方向订单。
 */
export async function findRecentMakerOrder(
  api: ApiPromise,
  maker: string,
  sideKeyword: 'buy' | 'sell',
  fromOrderId: number,
  toOrderIdExclusive: number,
): Promise<number | undefined> {
  for (let orderId = toOrderIdExclusive - 1; orderId >= fromOrderId; orderId -= 1) {
    const order = await tryReadMarketOrder(api, orderId);
    if (!order) {
      continue;
    }
    if (makerMatches(order, maker) && sideMatches(order, sideKeyword)) {
      return orderId;
    }
  }

  return undefined;
}

/**
 * 基于市场保护参数推导安全的买卖价格，避免测试价格越界。
 */
export async function readSafeMarketPrices(api: ApiPromise, marketPrice: number): Promise<{ sellPrice: number; buyPrice: number }> {
  const protectionQuery = (api.query as any).nexMarket?.priceProtection
    ?? (api.query as any).nexMarket?.priceProtectionStore;

  if (protectionQuery) {
    const protection = codecToJson(await protectionQuery());
    const maxDeviationBps = coerceNumber(readObjectField(protection, 'maxPriceDeviation', 'max_price_deviation'));
    if (maxDeviationBps && maxDeviationBps > 0) {
      const offset = Math.max(1, Math.floor((marketPrice * maxDeviationBps) / 10_000));
      return {
        sellPrice: marketPrice + offset,
        buyPrice: Math.max(1, marketPrice - offset),
      };
    }
  }

  const fallbackOffset = Math.max(1, Math.floor(marketPrice / 5));
  return {
    sellPrice: marketPrice + fallbackOffset,
    buyPrice: Math.max(1, marketPrice - fallbackOffset),
  };
}
