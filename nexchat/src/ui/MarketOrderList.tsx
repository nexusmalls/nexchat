import { useState } from "react";
import { formatBalance, formatNexPrice } from "@/market/format";
import type { NexMarketOrder } from "@/market/types";

interface MarketOrderListProps {
  buyOrders: NexMarketOrder[];
  sellOrders: NexMarketOrder[];
  loading?: boolean;
  onTakeBuy?: (order: NexMarketOrder) => void;
  onTakeSell?: (order: NexMarketOrder) => void;
}

// EN: Global order book list with take-order actions.
// CN: 全局挂单列表，支持一键吃单。
export function MarketOrderList({
  buyOrders,
  sellOrders,
  loading,
  onTakeBuy,
  onTakeSell,
}: MarketOrderListProps) {
  const [tab, setTab] = useState<"sell" | "buy">("sell");

  const activeBuys = buyOrders.filter((o) => BigInt(o.amount) - BigInt(o.filled) > 0n);
  const activeSells = sellOrders.filter((o) => BigInt(o.amount) - BigInt(o.filled) > 0n);

  if (loading && buyOrders.length === 0 && sellOrders.length === 0) {
    return <p className="wx-market-empty">加载订单…</p>;
  }

  return (
    <section className="wx-market-card">
      <div className="wx-market-tabs">
        <button
          type="button"
          className={`wx-market-tab${tab === "sell" ? " active" : ""}`}
          onClick={() => setTab("sell")}
        >
          卖单 {activeSells.length > 0 ? `(${activeSells.length})` : ""}
        </button>
        <button
          type="button"
          className={`wx-market-tab${tab === "buy" ? " active" : ""}`}
          onClick={() => setTab("buy")}
        >
          买单 {activeBuys.length > 0 ? `(${activeBuys.length})` : ""}
        </button>
      </div>

      {tab === "sell" ? (
        <OrderRows
          side="Sell"
          orders={activeSells}
          emptyLabel="暂无卖单"
          actionLabel="吃卖单"
          onAction={onTakeSell}
        />
      ) : (
        <OrderRows
          side="Buy"
          orders={activeBuys}
          emptyLabel="暂无买单"
          actionLabel="吃买单"
          onAction={onTakeBuy}
        />
      )}
    </section>
  );
}

function OrderRows({
  side,
  orders,
  emptyLabel,
  actionLabel,
  onAction,
}: {
  side: "Buy" | "Sell";
  orders: NexMarketOrder[];
  emptyLabel: string;
  actionLabel: string;
  onAction?: (order: NexMarketOrder) => void;
}) {
  if (orders.length === 0) {
    return <p className="wx-market-empty">{emptyLabel}</p>;
  }

  return (
    <>
      <div className="wx-market-order-head">
        <span>ID</span>
        <span>价格</span>
        <span>余量</span>
        <span />
      </div>
      <div className="wx-market-order-list">
        {orders.map((order) => {
          const remaining = BigInt(order.amount) - BigInt(order.filled);
          return (
            <div key={order.id} className="wx-market-order-row">
              <span>#{order.id}</span>
              <span className={side === "Buy" ? "wx-text-buy" : "wx-text-sell"}>
                ${formatNexPrice(order.price)}
              </span>
              <span>{formatBalance(remaining.toString())}</span>
              <button
                type="button"
                className="wx-market-order-action"
                onClick={() => onAction?.(order)}
              >
                {actionLabel}
              </button>
            </div>
          );
        })}
      </div>
    </>
  );
}
