// EN: On-chain NEX market read queries via polkadot.js (pallet-nex-market).
// CN: 经 polkadot.js 读取链上 NEX 市场（pallet-nex-market）。

import type {
  MarketSnapshot,
  NexDepthLevel,
  NexMarketOrder,
  NexMarketStats,
  NexMarketTrade,
  NexPriceProtection,
  NexTradeStatus,
} from "@/market/types";

function bytesToString(raw: unknown): string {
  if (raw == null) return "";
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) {
    return String.fromCharCode(...(raw as number[]).filter((b) => b > 0));
  }
  if (raw instanceof Uint8Array) {
    return new TextDecoder().decode(raw);
  }
  return String(raw);
}

function parseTradeStatus(status: unknown): NexTradeStatus {
  if (typeof status === "string") {
    const lower = status.toLowerCase();
    if (lower === "awaitingpayment") return "AwaitingPayment";
    if (lower === "awaitingverification") return "AwaitingVerification";
    if (lower === "underpaidpending") return "UnderpaidPending";
    if (lower === "completed") return "Completed";
    if (lower === "refunded") return "Refunded";
    if (lower === "cancelled") return "Cancelled";
    if (lower === "disputed") return "Disputed";
    return status as NexTradeStatus;
  }
  if (status && typeof status === "object") {
    const s = status as Record<string, boolean>;
    if (s.isAwaitingPayment) return "AwaitingPayment";
    if (s.isAwaitingVerification) return "AwaitingVerification";
    if (s.isUnderpaidPending) return "UnderpaidPending";
    if (s.isCompleted) return "Completed";
    if (s.isRefunded) return "Refunded";
    if (s.isCancelled) return "Cancelled";
    if (s.isDisputed) return "Disputed";
  }
  return "AwaitingPayment";
}

function parseNexTrade(data: Record<string, unknown>): NexMarketTrade {
  return {
    tradeId: Number(data.tradeId ?? data.trade_id ?? 0),
    orderId: Number(data.orderId ?? data.order_id ?? 0),
    buyer: String(data.buyer ?? ""),
    seller: String(data.seller ?? ""),
    nexAmount: String(data.nexAmount ?? data.nex_amount ?? "0"),
    usdtAmount: String(data.usdtAmount ?? data.usdt_amount ?? "0"),
    sellerTronAddress: bytesToString(data.sellerTronAddress ?? data.seller_tron_address),
    buyerTronAddress: bytesToString(data.buyerTronAddress ?? data.buyer_tron_address),
    status: parseTradeStatus(data.status),
    paymentConfirmed: Boolean(data.paymentConfirmed ?? data.payment_confirmed ?? false),
    createdAt: Number(data.createdAt ?? data.created_at ?? 0),
    buyerDeposit: String(data.buyerDeposit ?? data.buyer_deposit ?? "0"),
  };
}

type NexMarketApi = {
  query: {
    nexMarket: Record<string, (...args: unknown[]) => Promise<unknown>>;
  };
};

function parseNexOrder(id: number, data: Record<string, unknown>): NexMarketOrder {
  const sideRaw = data.side ?? data.orderType;
  let side: "Buy" | "Sell" = "Sell";
  if (typeof sideRaw === "string") {
    side = sideRaw.includes("Buy") || sideRaw === "Buy" ? "Buy" : "Sell";
  } else if (sideRaw && typeof sideRaw === "object" && "isBuy" in sideRaw) {
    side = (sideRaw as { isBuy?: boolean }).isBuy ? "Buy" : "Sell";
  }
  return {
    id,
    side,
    price: String(data.price ?? data.usdtPrice ?? data.usdt_price ?? "0"),
    amount: String(data.amount ?? data.nexAmount ?? data.nex_amount ?? data.totalAmount ?? "0"),
    filled: String(data.filled ?? data.filledAmount ?? data.filled_amount ?? "0"),
    deposit: String(data.buyerDeposit ?? data.buyer_deposit ?? "0"),
    depositWaived: Boolean(data.depositWaived ?? data.deposit_waived ?? false),
    createdAt: Number(data.createdAt ?? data.created_at ?? 0),
  };
}

function buildDepth(
  orders: NexMarketOrder[],
  side: "Buy" | "Sell",
): NexDepthLevel[] {
  const levels = orders.reduce<NexDepthLevel[]>((acc, o) => {
    if (o.side !== side) return acc;
    const remaining = BigInt(o.amount) - BigInt(o.filled);
    if (remaining <= 0n) return acc;
    const existing = acc.find((l) => l.price === o.price);
    if (existing) {
      existing.totalAmount += remaining;
      existing.orderCount++;
      if (o.depositWaived) existing.hasSeedOrder = true;
    } else {
      acc.push({
        price: o.price,
        totalAmount: remaining,
        orderCount: 1,
        cumulative: 0n,
        hasSeedOrder: o.depositWaived,
      });
    }
    return acc;
  }, []);

  if (side === "Sell") {
    levels.sort((a, b) => (BigInt(a.price) < BigInt(b.price) ? -1 : 1));
    let cum = 0n;
    const withCum = levels.map((l) => {
      cum += l.totalAmount;
      return { ...l, cumulative: cum };
    });
    return withCum.reverse();
  }

  levels.sort((a, b) => (BigInt(b.price) < BigInt(a.price) ? -1 : 1));
  let cum = 0n;
  return levels.map((l) => {
    cum += l.totalAmount;
    return { ...l, cumulative: cum };
  });
}

// EN: Fetch global market snapshot (stats + order book depth).
// CN: 拉取全局市场快照（统计 + 盘口深度）。
export async function fetchMarketSnapshot(api: NexMarketApi): Promise<MarketSnapshot> {
  const q = api.query.nexMarket;
  if (!q) {
    throw new Error("nexMarket pallet not found on runtime");
  }

  const [buyIdsRaw, sellIdsRaw, statsRaw, lastPriceRaw, protectionRaw] = await Promise.all([
    q.buyOrders(),
    q.sellOrders(),
    q.marketStatsStore(),
    q.lastTradePrice().catch(() => null),
    q.priceProtectionStore().catch(() => null),
  ]);

  const buyIds = ((buyIdsRaw as { toJSON?: () => number[] })?.toJSON?.() ?? []) as number[];
  const sellIds = ((sellIdsRaw as { toJSON?: () => number[] })?.toJSON?.() ?? []) as number[];

  const resolveOrders = async (ids: number[]): Promise<NexMarketOrder[]> => {
    const orders: NexMarketOrder[] = [];
    for (const id of ids) {
      const raw = (await q.orders(id)) as { isNone?: boolean; unwrap?: () => { toJSON: () => Record<string, unknown> } };
      if (raw?.isNone) continue;
      orders.push(parseNexOrder(id, raw.unwrap!().toJSON()));
    }
    return orders;
  };

  const [buyOrders, sellOrders] = await Promise.all([
    resolveOrders(buyIds),
    resolveOrders(sellIds),
  ]);

  buyOrders.sort((a, b) => {
    const diff = BigInt(b.price) - BigInt(a.price);
    return diff < 0n ? -1 : diff > 0n ? 1 : 0;
  });
  sellOrders.sort((a, b) => {
    const diff = BigInt(a.price) - BigInt(b.price);
    return diff < 0n ? -1 : diff > 0n ? 1 : 0;
  });

  const statsJson = ((statsRaw as { toJSON?: () => Record<string, unknown> })?.toJSON?.() ??
    {}) as Record<string, unknown>;
  const protection = ((protectionRaw as { toJSON?: () => Record<string, unknown> } | null)?.toJSON?.() ??
    {}) as Record<string, unknown>;

  let lastPrice = "0";
  const lp = lastPriceRaw as { isNone?: boolean; unwrap?: () => { toJSON: () => unknown } } | null;
  if (lp && !lp.isNone) {
    lastPrice = String(lp.unwrap!().toJSON() ?? "0");
  }

  const initialPriceRaw = protection.initialPrice ?? protection.initial_price;
  const referencePrice =
    initialPriceRaw != null && String(initialPriceRaw) !== "0" ? String(initialPriceRaw) : null;

  const priceProtection: NexPriceProtection = {
    enabled: Boolean(protection.enabled ?? true),
    maxPriceDeviation: Number(protection.maxPriceDeviation ?? protection.max_price_deviation ?? 0),
    circuitBreakerActive: Boolean(
      protection.circuitBreakerActive ?? protection.circuit_breaker_active ?? false,
    ),
    initialPrice: referencePrice,
  };

  const stats: NexMarketStats = {
    lastPrice,
    totalOrders: Number(statsJson.totalOrders ?? statsJson.total_orders ?? 0),
    totalTrades: Number(statsJson.totalTrades ?? statsJson.total_trades ?? 0),
    totalVolumeUsdt: String(statsJson.totalVolumeUsdt ?? statsJson.total_volume_usdt ?? "0"),
    referencePrice,
    referenceSource: referencePrice ? "initial" : null,
  };

  const asks = buildDepth(sellOrders, "Sell");
  const bids = buildDepth(buyOrders, "Buy");
  const askMax = asks.length ? asks[asks.length - 1]!.cumulative : 0n;
  const bidMax = bids.length ? bids[bids.length - 1]!.cumulative : 0n;
  const maxDepth = askMax > bidMax ? askMax : bidMax;

  return { stats, protection: priceProtection, buyOrders, sellOrders, asks, bids, maxDepth };
}

// EN: Fetch active orders for a user address.
// CN: 拉取用户活跃挂单。
export async function fetchUserOrders(
  api: NexMarketApi,
  address: string,
): Promise<NexMarketOrder[]> {
  const q = api.query.nexMarket;
  if (!q) return [];

  const idsRaw = await q.userOrders(address);
  const ids = ((idsRaw as { toJSON?: () => number[] })?.toJSON?.() ?? []) as number[];
  const orders: NexMarketOrder[] = [];

  for (const id of ids) {
    const raw = (await q.orders(id)) as {
      isNone?: boolean;
      unwrap?: () => { toJSON: () => Record<string, unknown> };
    };
    if (raw?.isNone) continue;
    const order = parseNexOrder(id, raw.unwrap!().toJSON());
    const remaining = BigInt(order.amount) - BigInt(order.filled);
    if (remaining > 0n) orders.push(order);
  }

  return orders.sort((a, b) => b.createdAt - a.createdAt);
}

// EN: Fetch user's NEX market trades (active + recent).
// CN: 拉取用户 NEX 市场交易（进行中 + 近期）。
export async function fetchUserTrades(
  api: NexMarketApi,
  address: string,
): Promise<NexMarketTrade[]> {
  const q = api.query.nexMarket;
  if (!q) return [];

  const idsRaw = await q.userTrades(address);
  const ids = ((idsRaw as { toJSON?: () => number[] })?.toJSON?.() ?? []) as number[];
  const trades: NexMarketTrade[] = [];

  for (const id of ids) {
    const raw = (await q.usdtTrades(id)) as {
      isNone?: boolean;
      unwrap?: () => { toJSON: () => Record<string, unknown> };
    };
    if (raw?.isNone) continue;
    trades.push(parseNexTrade(raw.unwrap!().toJSON()));
  }

  return trades.sort((a, b) => b.createdAt - a.createdAt);
}
