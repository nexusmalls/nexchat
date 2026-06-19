import { useMemo, useState } from "react";
import { config } from "@/config";
import { useAllEntities } from "@/hooks/useEntities";
import { useEarningEntities, useEarnings } from "@/hooks/useEarnings";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useNexPrice } from "@/hooks/useNexPrice";
import { useShoppingBalance } from "@/hooks/useShoppingBalance";
import { useWallet } from "@/hooks/useWallet";
import { withdrawCommission } from "@/earnings/commissionTx";
import type { EarningsPluginCard } from "@/earnings/types";
import { formatBalance, formatUsdt } from "@/market/format";
import { decodeHexUtf8String } from "@/mls/chainBytes";
import { useUiStore } from "@/state/uiStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";

// EN: Me tab — commission earnings (aligned with nexus-com-dapp /earnings).
// CN: 「我」Tab——佣金收益（对齐 nexus-com-dapp /earnings）。
export function EarningsPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const currentEntityId = useUiStore((s) => s.currentEntityId);
  const currentEntityName = useUiStore((s) => s.currentEntityName);
  const { address } = useWallet();
  const { entities: earningEntities, loading: entitiesLoading } = useEarningEntities(address, true);
  const { entities: registryEntities } = useAllEntities(!!address);

  const entityId = currentEntityId;

  const {
    memberStats,
    overview,
    dashboard,
    withdrawals,
    repurchaseConfig,
    plugins,
    loading,
    error,
    refresh,
  } = useEarnings(address, entityId, true);

  const { balance: shoppingBal } = useShoppingBalance(entityId, address);
  const { toUsdt } = useNexPrice(true);

  const tx = useMarketTx(() => void refresh());

  const selectedEntity = earningEntities.find((e) => e.entityId === entityId);
  const selectedRegistry = registryEntities.find((e) => e.id === entityId);
  const entityLabel = useMemo(() => {
    const raw =
      currentEntityName ??
      selectedEntity?.name ??
      selectedRegistry?.name ??
      (entityId != null ? `Entity #${entityId}` : null);
    return raw ? decodeHexUtf8String(raw) : null;
  }, [currentEntityName, selectedEntity?.name, selectedRegistry?.name, entityId]);

  const dappBase = useMemo(() => config.shopDappUrl?.replace(/\/$/, "") ?? "", []);

  const totalEarned =
    memberStats?.totalEarned ?? dashboard?.nexStats.totalEarned ?? "0";
  const pending = memberStats?.pending ?? dashboard?.nexStats.pending ?? "0";
  const withdrawn = memberStats?.withdrawn ?? dashboard?.nexStats.withdrawn ?? "0";
  const repurchased =
    memberStats?.repurchased ?? dashboard?.nexStats.repurchased ?? "0";

  const shoppingBalUsdt = shoppingBal && toUsdt ? toUsdt(shoppingBal) : null;
  const thresholdUsdt = repurchaseConfig?.maxShoppingBalanceUsdt ?? "0";
  const hasThreshold = BigInt(thresholdUsdt) > 0n;
  const hasShoppingBal = BigInt(shoppingBal || "0") > 0n;
  const shoppingExceedsThreshold =
    hasThreshold &&
    hasShoppingBal &&
    (shoppingBalUsdt == null || BigInt(shoppingBalUsdt) > BigInt(thresholdUsdt));

  const canWithdraw =
    !!address &&
    !config.useMock &&
    !tx.busy &&
    !overview?.withdrawalPaused &&
    BigInt(pending) > 0n &&
    !shoppingExceedsThreshold;

  const commissionActive = overview?.isEnabled === true;
  const [historyOpen, setHistoryOpen] = useState(false);
  const showLoading = loading && !memberStats && !overview;

  const pluginUrl = (p: EarningsPluginCard) =>
    p.href && dappBase ? `${dappBase}${p.href}` : null;

  return (
    <main className="tg-main wx-earnings-main">
      <WeChatNavBar
        title="收益"
        onBack={() => setSettingsView("list")}
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

      <div className="wx-earnings-body">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上收益。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        {!address && <p className="wx-market-empty">请先解锁钱包</p>}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {address && entityId == null && (
          <div className="wx-earnings-empty-card">
            <p className="wx-earnings-empty-title">未选择实体</p>
            <p className="wx-earnings-empty-desc">请先在「我 → 实体」中选择或加入实体</p>
            <button
              type="button"
              className="wx-earnings-entity-switch-btn"
              onClick={() => setSettingsView("entity")}
            >
              去选择实体
            </button>
          </div>
        )}

        {address && entityId != null && (
          <>
            {entityLabel && (
              <p className="wx-earnings-entity-inline">
                当前实体：{entityLabel}
                <button type="button" onClick={() => setSettingsView("entity")}>
                  切换
                </button>
              </p>
            )}

            {entitiesLoading && earningEntities.length === 0 && !selectedRegistry ? (
              <EarningsSkeleton />
            ) : (
              <>
                <section className="wx-earnings-hero-card">
                  <p className="wx-earnings-hero-label">累计收益</p>

                  {showLoading ? (
                    <div className="wx-earnings-skeleton-block lg" />
                  ) : !commissionActive ? (
                    <p className="wx-earnings-empty-desc">该 Entity 未启用佣金系统</p>
                  ) : (
                    <>
                      <p className="wx-earnings-total">
                        {formatBalance(totalEarned)}
                        <span className="wx-earnings-unit"> NEX</span>
                      </p>

                      <div className="wx-earnings-breakdown">
                        <div>
                          <span className="wx-earnings-breakdown-label">待提现</span>
                          <strong>{formatBalance(pending)}</strong>
                        </div>
                        <div>
                          <span className="wx-earnings-breakdown-label">已提现</span>
                          <strong>{formatBalance(withdrawn)}</strong>
                        </div>
                        <div>
                          <span className="wx-earnings-breakdown-label">回购</span>
                          <strong>{formatBalance(repurchased)}</strong>
                        </div>
                      </div>

                      <div className="wx-earnings-shopping-row">
                        <span>剩余购物余额</span>
                        <strong>{formatBalance(shoppingBal ?? "0")} NEX</strong>
                      </div>

                      {shoppingExceedsThreshold && (
                        <div className="wx-earnings-alert">
                          <span>购物余额超过复购阈值，暂不可提现</span>
                          {shoppingBalUsdt != null && (
                            <span className="wx-earnings-alert-sub">
                              当前 ≈ ${formatUsdt(shoppingBalUsdt, 0)} USDT，阈值 $
                              {formatUsdt(thresholdUsdt, 0)} USDT
                            </span>
                          )}
                        </div>
                      )}

                      {overview?.withdrawalPaused && (
                        <div className="wx-earnings-alert">该 Entity 已暂停提现</div>
                      )}

                      <button
                        type="button"
                        className={`wx-earnings-withdraw-btn${canWithdraw ? " active" : ""}`}
                        disabled={!canWithdraw}
                        onClick={() =>
                          void tx.run(() => withdrawCommission(entityId, pending))
                        }
                      >
                        {tx.busy ? "提交中…" : `↓ 提现 (${formatBalance(pending)} NEX)`}
                      </button>

                      {tx.status === "ok" && (
                        <p className="wx-market-tx-status ok">提现交易已提交</p>
                      )}
                      {tx.status === "error" && (
                        <p className="wx-market-tx-status error">{tx.error ?? "提现失败"}</p>
                      )}
                    </>
                  )}
                </section>

                {commissionActive && !showLoading && plugins.length > 0 && (
                  <div className="wx-earnings-plugin-list">
                    {plugins.map((p) => (
                      <PluginRow
                        key={p.key}
                        plugin={p}
                        href={pluginUrl(p)}
                        onOpen={(key) => {
                          if (key === "multiLevel") setSettingsView("earningsMultiLevel");
                          if (key === "singleLine") setSettingsView("earningsSingleLine");
                          if (key === "poolReward") setSettingsView("earningsPoolReward");
                        }}
                      />
                    ))}
                  </div>
                )}

                {commissionActive && !showLoading && (
                  <section className="wx-earnings-history-card">
                    <button
                      type="button"
                      className="wx-earnings-history-toggle"
                      onClick={() => setHistoryOpen((v) => !v)}
                    >
                      <span className="wx-earnings-history-left">
                        <span className="wx-earnings-history-icon" aria-hidden>
                          🕐
                        </span>
                        <span>提现记录</span>
                        {withdrawals.length > 0 && (
                          <span className="wx-earnings-history-badge">{withdrawals.length}</span>
                        )}
                      </span>
                      <span className={`wx-earnings-history-chevron${historyOpen ? " open" : ""}`}>
                        ›
                      </span>
                    </button>
                    {historyOpen && (
                      <div className="wx-earnings-history-list">
                        {withdrawals.length === 0 ? (
                          <p className="wx-earnings-history-empty">暂无提现明细记录</p>
                        ) : (
                          [...withdrawals].reverse().slice(0, 20).map((r, i) => (
                            <div key={`${r.blockNumber}-${i}`} className="wx-earnings-history-row">
                              <div>
                                <strong className="wx-earnings-history-amt">
                                  +{formatBalance(r.withdrawn)} NEX
                                </strong>
                                {BigInt(r.repurchased) > 0n && (
                                  <p className="wx-earnings-history-sub">
                                    回购 {formatBalance(r.repurchased)} NEX
                                  </p>
                                )}
                              </div>
                              <span className="wx-earnings-history-block">#{r.blockNumber}</span>
                            </div>
                          ))
                        )}
                      </div>
                    )}
                  </section>
                )}
              </>
            )}
          </>
        )}
      </div>
    </main>
  );
}

function PluginRow({
  plugin,
  href,
  onOpen,
}: {
  plugin: EarningsPluginCard;
  href: string | null;
  onOpen?: (key: string) => void;
}) {
  const inner = (
    <>
      <div className="wx-earnings-plugin-icon-wrap">{plugin.icon}</div>
      <div className="wx-earnings-plugin-body">
        <div className="wx-earnings-plugin-head">
          <span className="wx-earnings-plugin-title">{plugin.label}</span>
          <span
            className={`wx-earnings-plugin-badge ${plugin.status === "enabled" ? "on" : "off"}`}
          >
            {plugin.status === "enabled" ? "✓ 已启用" : "已暂停"}
          </span>
        </div>
        {plugin.stat && <p className="wx-earnings-plugin-stat">{plugin.stat}</p>}
        {plugin.stat2 && <p className="wx-earnings-plugin-stat2">{plugin.stat2}</p>}
        {!plugin.stat && !plugin.stat2 && (
          <p className="wx-earnings-plugin-desc">{plugin.description}</p>
        )}
      </div>
      {(href || onOpen) && <span className="wx-earnings-plugin-chevron">›</span>}
    </>
  );

  if ((plugin.key === "multiLevel" || plugin.key === "singleLine" || plugin.key === "poolReward") && onOpen) {
    return (
      <button
        type="button"
        className="wx-earnings-plugin-row link"
        onClick={() => onOpen(plugin.key)}
      >
        {inner}
      </button>
    );
  }

  if (href) {
    return (
      <a className="wx-earnings-plugin-row link" href={href} target="_blank" rel="noreferrer">
        {inner}
      </a>
    );
  }

  return <div className="wx-earnings-plugin-row">{inner}</div>;
}

function EarningsSkeleton() {
  return (
    <section className="wx-earnings-hero-card">
      <div className="wx-earnings-skeleton-block sm" />
      <div className="wx-earnings-skeleton-block lg" />
      <div className="wx-earnings-skeleton-grid">
        <div className="wx-earnings-skeleton-block sm" />
        <div className="wx-earnings-skeleton-block sm" />
        <div className="wx-earnings-skeleton-block sm" />
      </div>
    </section>
  );
}
