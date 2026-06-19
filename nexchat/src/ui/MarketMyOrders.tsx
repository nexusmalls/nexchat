import { useMarketTx } from "@/hooks/useMarketTx";
import { formatBalance, formatNexPrice } from "@/market/format";
import { cancelOrder } from "@/market/nexMarketTx";
import type { NexMarketOrder } from "@/market/types";

interface MarketMyOrdersProps {
  orders: NexMarketOrder[];
  onSuccess: () => void;
}

// EN: User's open orders with cancel action.
// CN: 用户活跃挂单与撤单。
export function MarketMyOrders({ orders, onSuccess }: MarketMyOrdersProps) {
  const tx = useMarketTx(onSuccess);

  if (orders.length === 0) {
    return (
      <section className="wx-market-card">
        <h3 className="wx-market-section-title">我的挂单</h3>
        <p className="wx-market-empty">暂无活跃挂单</p>
      </section>
    );
  }

  return (
    <section className="wx-market-card">
      <h3 className="wx-market-section-title">我的挂单</h3>
      <div className="wx-market-order-head">
        <span>ID</span>
        <span>方向</span>
        <span>价格</span>
        <span />
      </div>
      <div className="wx-market-order-list">
        {orders.map((order) => {
          const remaining = BigInt(order.amount) - BigInt(order.filled);
          return (
            <div key={order.id} className="wx-market-order-row">
              <span>#{order.id}</span>
              <span className={order.side === "Buy" ? "wx-text-buy" : "wx-text-sell"}>
                {order.side === "Buy" ? "买" : "卖"}
              </span>
              <span>${formatNexPrice(order.price)} · {formatBalance(remaining.toString())}</span>
              <button
                type="button"
                className="wx-market-order-action cancel"
                disabled={tx.busy}
                onClick={() => void tx.run(() => cancelOrder(order.id))}
              >
                撤单
              </button>
            </div>
          );
        })}
      </div>
      {tx.status === "error" && (
        <p className="wx-market-tx-status error">{tx.error}</p>
      )}
      {tx.status === "ok" && (
        <p className="wx-market-tx-status ok">撤单成功</p>
      )}
    </section>
  );
}
