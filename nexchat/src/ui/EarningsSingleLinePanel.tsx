import { useMemo, useState } from "react";
import { config } from "@/config";
import {
  chainDirectionToUserSide,
  filterSingleLineRecords,
  singleLineTotalEarned,
  sumSingleLineAmount,
  userDownlineTotal,
  userSideLabel,
  userUplineTotal,
} from "@/earnings/singleLineQueries";
import type { SingleLineDirection } from "@/earnings/types";
import { useSingleLineEarnings } from "@/hooks/useSingleLineEarnings";
import { useWallet } from "@/hooks/useWallet";
import { formatBalance } from "@/market/format";
import { useUiStore } from "@/state/uiStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";
import { shortNexAddress } from "@/wallet/address";

type FilterTab = "all" | SingleLineDirection;

const FILTER_TABS: { key: FilterTab; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "upline", label: "上层" },
  { key: "downline", label: "下层" },
];

// EN: Me tab — single-line win-win earnings detail (共赢收益).
// CN: 「我」Tab——单线共赢收益详情页。
export function EarningsSingleLinePanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const entityId = useUiStore((s) => s.currentEntityId);
  const { address } = useWallet();
  const { stats, records, loading, error, refresh } = useSingleLineEarnings(
    address,
    entityId,
    true,
  );
  const [filter, setFilter] = useState<FilterTab>("all");

  const totalEarned = singleLineTotalEarned(stats);
  const filtered = useMemo(
    () => filterSingleLineRecords(records, filter),
    [records, filter],
  );
  const filteredTotal = useMemo(() => sumSingleLineAmount(filtered), [filtered]);
  const showLoading = loading && !stats && records.length === 0;

  return (
    <main className="tg-main wx-earnings-main wx-sl-earnings-main">
      <WeChatNavBar
        title="共赢收益"
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

      <div className="wx-earnings-body wx-sl-earnings-body">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上共赢收益。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        {!address && <p className="wx-market-empty">请先解锁钱包</p>}
        {entityId == null && address && (
          <p className="wx-market-empty">请先在「我 → 实体」中选择实体</p>
        )}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {address && entityId != null && (
          <>
            <section className="wx-sl-summary-card">
              <p className="wx-sl-summary-label">我的共赢收益</p>
              {showLoading ? (
                <div className="wx-earnings-skeleton-block lg" />
              ) : (
                <>
                  <p className="wx-sl-summary-total">
                    {formatBalance(totalEarned, 12, 0)}
                    <span className="wx-earnings-unit"> NEX</span>
                  </p>
                  <div className="wx-sl-breakdown">
                    <div>
                      <span>上层</span>
                      <strong className="wx-sl-upline">
                        {formatBalance(userUplineTotal(stats), 12, 0)} NEX
                      </strong>
                    </div>
                    <div>
                      <span>下层</span>
                      <strong className="wx-sl-downline">
                        {formatBalance(userDownlineTotal(stats), 12, 0)} NEX
                      </strong>
                    </div>
                  </div>
                  <p className="wx-sl-count">{stats?.totalPayoutCount ?? 0} 笔</p>
                </>
              )}
            </section>

            <div className="wx-sl-filters">
              <span className="wx-sl-filter-icon" aria-hidden>
                ⛃
              </span>
              {FILTER_TABS.map((tab) => (
                <button
                  key={tab.key}
                  type="button"
                  className={`wx-sl-filter-btn${filter === tab.key ? " active" : ""}`}
                  onClick={() => setFilter(tab.key)}
                >
                  {tab.label}
                </button>
              ))}
            </div>

            <section className="wx-sl-record-card">
              <div className="wx-sl-record-head">
                <div>
                  <p className="wx-sl-record-title">
                    <span className="wx-sl-record-icon" aria-hidden>
                      📄
                    </span>
                    共赢收益记录
                  </p>
                  {!showLoading && filtered.length > 0 && (
                    <p className="wx-sl-record-sub">
                      总计: {formatBalance(filteredTotal, 12, 0)} NEX
                    </p>
                  )}
                </div>
                {!showLoading && filtered.length > 0 && (
                  <span className="wx-sl-record-badge">{filtered.length} 条记录</span>
                )}
              </div>

              {showLoading ? (
                <div className="wx-sl-record-list">
                  <div className="wx-earnings-skeleton-block sm" />
                  <div className="wx-earnings-skeleton-block sm" />
                </div>
              ) : filtered.length === 0 ? (
                <p className="wx-sl-empty">暂无共赢收益记录</p>
              ) : (
                <ul className="wx-sl-record-list">
                  {filtered.map((r, i) => {
                    const userSide = chainDirectionToUserSide(r.direction);
                    const fromUpline = userSide === "upline";
                    return (
                    <li key={`${r.blockNumber}-${r.orderId}-${i}`} className="wx-sl-record-row">
                      <span
                        className={`wx-sl-dir-icon ${fromUpline ? "up" : "down"}`}
                        aria-hidden
                      >
                        {fromUpline ? "⌃" : "⌄"}
                      </span>
                      <div className="wx-sl-record-main">
                        <strong className={`wx-sl-record-amt${fromUpline ? " up" : ""}`}>
                          +{formatBalance(r.amount, 12, 0)} NEX
                        </strong>
                        <p className="wx-sl-record-meta">
                          订单 #{r.orderId}
                          {r.shopId != null && r.shopId > 0 ? ` · 店铺 #${r.shopId}` : ""}
                        </p>
                        <p className="wx-sl-record-meta">
                          {shortNexAddress(r.buyer)} · 区块 #{r.blockNumber}
                        </p>
                      </div>
                      <div className="wx-sl-record-badges">
                        <span className="wx-sl-level-pill">
                          {userSideLabel(r.direction, r.levelDistance)}
                        </span>
                        <span className="wx-sl-settled-pill">已结算</span>
                      </div>
                    </li>
                    );
                  })}
                </ul>
              )}
            </section>
          </>
        )}
      </div>
    </main>
  );
}
