import { useCallback, useMemo, useState } from "react";
import { config } from "@/config";
import { useNexMarket } from "@/hooks/useNexMarket";
import { useWallet } from "@/hooks/useWallet";
import { useUiStore } from "@/state/uiStore";
import { formatBalance, formatNexPrice, formatUsdt } from "@/market/format";
import type { NexDepthLevel, NexMarketOrder, OrderActionPrefill } from "@/market/types";
import { MarketActiveTrades } from "@/ui/MarketActiveTrades";
import { MarketTradeHistory } from "@/ui/MarketTradeHistory";
import { MarketMyOrders } from "@/ui/MarketMyOrders";
import { MarketOrderList } from "@/ui/MarketOrderList";
import { MarketTradeForm } from "@/ui/MarketTradeForm";

const MAX_ROWS = 7;

// EN: NEX/USDT global market — discover sub-view with read + trade.
// CN: NEX/USDT 全局市场——发现子视图（行情 + 挂单/吃单）。
export function MarketPanel() {
  const setDiscoverView = useUiStore((s) => s.setDiscoverView);
  const { address } = useWallet();
  const { data, userOrders, userTrades, loading, error, refresh } = useNexMarket(
    true,
    address,
  );
  const [prefill, setPrefill] = useState<OrderActionPrefill | null>(null);

  const stats = data?.stats;
  const hasLastPrice = stats?.lastPrice && stats.lastPrice !== "0";

  const allOrders = useMemo<NexMarketOrder[]>(() => {
    if (!data) return [];
    return [...data.buyOrders, ...data.sellOrders];
  }, [data]);

  const handleTakeBuy = useCallback((order: NexMarketOrder) => {
    const remaining = BigInt(order.amount) - BigInt(order.filled);
    setPrefill({
      target: "takeBuy",
      orderId: String(order.id),
      amount: formatBalance(remaining.toString(), 12, 4),
      tron: "",
    });
  }, []);

  const handleTakeSell = useCallback((order: NexMarketOrder) => {
    const remaining = BigInt(order.amount) - BigInt(order.filled);
    setPrefill({
      target: "takeSell",
      orderId: String(order.id),
      amount: formatBalance(remaining.toString(), 12, 4),
      tron: "",
    });
  }, []);

  return (
    <main className="tg-main wx-market-main">
      <header className="tg-sub-head wx-market-head">
        <button
          type="button"
          className="tg-sub-back wx-nav-back"
          onClick={() => setDiscoverView("list")}
        >
          ‹ 返回
        </button>
        <span>市场</span>
        <button
          type="button"
          className="wx-market-refresh"
          onClick={() => void refresh()}
          disabled={loading}
        >
          {loading ? "…" : "刷新"}
        </button>
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式下无法读取或提交链上市场。请设置 <code>VITE_USE_MOCK=false</code> 并连接节点。
          </div>
        )}

        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        <section className="wx-market-card wx-market-price-card">
          <div className="wx-market-price-row">
            <div>
              <p className="wx-market-label">最新成交价</p>
              <p className="wx-market-last">
                {hasLastPrice ? `$${formatNexPrice(stats!.lastPrice)}` : "--"}
              </p>
            </div>
            <div className="wx-market-price-side">
              <p className="wx-market-label">参考价</p>
              <p className="wx-market-ref">
                {stats?.referencePrice ? `$${formatNexPrice(stats.referencePrice)}` : "--"}
              </p>
            </div>
          </div>
          <div className="wx-market-stats">
            <StatCell label="总订单" value={String(stats?.totalOrders ?? "—")} />
            <StatCell label="总成交" value={String(stats?.totalTrades ?? "—")} />
            <StatCell
              label="成交额"
              value={stats?.totalVolumeUsdt ? `$${formatUsdt(stats.totalVolumeUsdt)}` : "—"}
            />
          </div>
        </section>

        <section className="wx-market-card">
          <h3 className="wx-market-section-title">盘口深度</h3>
          <div className="wx-market-book-head">
            <span>USDT 价</span>
            <span>NEX 量</span>
            <span>累计</span>
          </div>

          {loading && !data ? (
            <p className="wx-market-empty">加载中…</p>
          ) : (
            <>
              <DepthRows
                rows={(data?.asks ?? []).slice(-MAX_ROWS)}
                maxDepth={data?.maxDepth ?? 0n}
                side="ask"
                emptyLabel="暂无卖单"
              />
              <div className="wx-market-mid">
                {hasLastPrice ? `$${formatNexPrice(stats!.lastPrice)}` : "--"}
              </div>
              <DepthRows
                rows={(data?.bids ?? []).slice(0, MAX_ROWS)}
                maxDepth={data?.maxDepth ?? 0n}
                side="bid"
                emptyLabel="暂无买单"
              />
            </>
          )}
        </section>

        <MarketOrderList
          buyOrders={data?.buyOrders ?? []}
          sellOrders={data?.sellOrders ?? []}
          loading={loading}
          onTakeBuy={handleTakeBuy}
          onTakeSell={handleTakeSell}
        />

        <MarketActiveTrades
          trades={userTrades}
          address={address}
          onSuccess={() => void refresh()}
        />

        <MarketTradeHistory trades={userTrades} address={address} />

        <MarketTradeForm
          snapshot={data}
          allOrders={allOrders}
          address={address}
          prefill={prefill}
          onPrefillUsed={() => setPrefill(null)}
          onSuccess={() => void refresh()}
        />

        <MarketMyOrders orders={userOrders} onSuccess={() => void refresh()} />

        <p className="wx-market-foot">NEX 全局市场 · 挂单/吃单 · 确认付款/收款</p>
      </div>
    </main>
  );
}

function StatCell({ label, value }: { label: string; value: string }) {
  return (
    <div className="wx-market-stat">
      <span className="wx-market-stat-label">{label}</span>
      <span className="wx-market-stat-value">{value}</span>
    </div>
  );
}

function DepthRows({
  rows,
  maxDepth,
  side,
  emptyLabel,
}: {
  rows: NexDepthLevel[];
  maxDepth: bigint;
  side: "ask" | "bid";
  emptyLabel: string;
}) {
  if (rows.length === 0) {
    return <p className="wx-market-empty">{emptyLabel}</p>;
  }

  return (
    <div className={`wx-market-depth wx-market-depth-${side}`}>
      {rows.map((level) => {
        const pct = maxDepth > 0n ? Number((level.cumulative * 100n) / maxDepth) : 0;
        return (
          <div key={`${side}-${level.price}`} className="wx-market-depth-row">
            <div
              className="wx-market-depth-bar"
              style={{ width: `${Math.min(100, pct)}%` }}
            />
            <span className="wx-market-depth-price">
              ${formatNexPrice(level.price)}
              {level.hasSeedOrder ? " 🌱" : ""}
            </span>
            <span>{formatBalance(level.totalAmount.toString())}</span>
            <span>{formatBalance(level.cumulative.toString())}</span>
          </div>
        );
      })}
    </div>
  );
}
