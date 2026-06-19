import { formatBalance, formatUsdt } from "@/market/format";
import type { NexMarketTrade } from "@/market/types";
import { canonicalAddress } from "@/wallet/address";

const TERMINAL = new Set(["Completed", "Refunded", "Cancelled", "Disputed"]);

const STATUS_LABEL: Record<string, string> = {
  Completed: "已完成",
  Refunded: "已退款",
  Cancelled: "已取消",
  Disputed: "争议",
};

interface MarketTradeHistoryProps {
  trades: NexMarketTrade[];
  address: string | null;
}

// EN: Completed / terminal NEX market trades for the signed-in user.
// CN: 当前用户已结束的全局市场成交记录。
export function MarketTradeHistory({ trades, address }: MarketTradeHistoryProps) {
  const who = address ? canonicalAddress(address) : null;
  const history = trades.filter((t) => TERMINAL.has(t.status));

  return (
    <section className="wx-market-card">
      <h3 className="wx-market-section-title">
        成交记录{history.length > 0 ? ` (${history.length})` : ""}
      </h3>
      {history.length === 0 ? (
        <p className="wx-market-empty">暂无历史成交</p>
      ) : (
        <div className="wx-market-history-list">
          {history.map((trade) => {
            const isBuyer = who != null && canonicalAddress(trade.buyer) === who;
            return (
              <div key={trade.tradeId} className="wx-market-history-row">
                <span className="wx-market-history-id">#{trade.tradeId}</span>
                <span
                  className={`wx-market-history-nex ${isBuyer ? "buy" : "sell"}`}
                >
                  {isBuyer ? "+" : "-"}
                  {formatBalance(trade.nexAmount)}
                </span>
                <span className="wx-market-history-usdt">
                  ${formatUsdt(trade.usdtAmount)}
                </span>
                <span className={`wx-market-trade-status status-${trade.status}`}>
                  {STATUS_LABEL[trade.status] ?? trade.status}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
