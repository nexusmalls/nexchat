import { config } from "@/config";
import { bpsToPercentLabel } from "@/earnings/multiLevelQueries";
import { useMultiLevelEarnings } from "@/hooks/useMultiLevelEarnings";
import { useWallet } from "@/hooks/useWallet";
import { formatBalance } from "@/market/format";
import { useUiStore } from "@/state/uiStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { shortNexAddress } from "@/wallet/address";

// EN: Me tab — multi-level boost earnings detail (助力收益).
// CN: 「我」Tab——多级助力收益详情页。
export function EarningsMultiLevelPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const entityId = useUiStore((s) => s.currentEntityId);
  const { address } = useWallet();
  const { stats, records, loading, error, refresh } = useMultiLevelEarnings(
    address,
    entityId,
    true,
  );

  const totalEarned = stats?.totalEarned ?? "0";
  const showLoading = loading && !stats && records.length === 0;

  return (
    <main className="tg-main wx-earnings-main wx-ml-earnings-main">
      <WeChatNavBar
        title="助力收益"
        onBack={() => setSettingsView("earnings")}
        actions={
          <button
            type="button"
            className="wx-nav-text-btn"
            onClick={() => void refresh()}
            disabled={loading}
          >
            {loading ? "…" : "刷新"}
          </button>
        }
      />

      <div className="wx-earnings-body wx-ml-earnings-body">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上助力收益。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        {!address && <p className="wx-market-empty">请先解锁钱包</p>}
        {entityId == null && address && (
          <p className="wx-market-empty">请先在「我 → 实体」中选择实体</p>
        )}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {address && entityId != null && (
          <>
            <section className="wx-ml-summary-card">
              <p className="wx-ml-summary-label">我的助力收益</p>
              {showLoading ? (
                <div className="wx-earnings-skeleton-block lg" />
              ) : (
                <p className="wx-ml-summary-total">
                  {formatBalance(totalEarned, 12, 0)}
                  <span className="wx-earnings-unit"> NEX</span>
                </p>
              )}
            </section>

            <div className="wx-ml-section-head">
              <span className="wx-ml-section-title">
                <span className="wx-ml-section-icon" aria-hidden>
                  ↻
                </span>
                奖励记录
              </span>
              {!showLoading && records.length > 0 && (
                <span className="wx-ml-section-badge">{records.length}</span>
              )}
            </div>

            {showLoading ? (
              <div className="wx-ml-record-list">
                <div className="wx-earnings-skeleton-block sm" />
                <div className="wx-earnings-skeleton-block sm" />
                <div className="wx-earnings-skeleton-block sm" />
              </div>
            ) : records.length === 0 ? (
              <p className="wx-ml-empty">暂无助力奖励记录</p>
            ) : (
              <ul className="wx-ml-record-list">
                {records.map((r, i) => {
                  const rateLabel = bpsToPercentLabel(r.rateBps);
                  return (
                    <li key={`${r.blockNumber}-${r.orderId}-${i}`} className="wx-ml-record-row">
                      <span className="wx-ml-level-badge">L{r.level}</span>
                      <div className="wx-ml-record-main">
                        <strong className="wx-ml-record-amt">
                          +{formatBalance(r.amount, 12, 0)} NEX
                        </strong>
                        <p className="wx-ml-record-meta">
                          订单 #{r.orderId} · 买家 {shortNexAddress(r.buyer)}
                        </p>
                        <p className="wx-ml-record-block">区块 #{r.blockNumber}</p>
                      </div>
                      {rateLabel && (
                        <span className="wx-ml-rate-pill">
                          L{r.level} · {rateLabel}
                        </span>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        )}
      </div>
    </main>
  );
}
