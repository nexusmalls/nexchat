import { useMemo } from "react";
import { config } from "@/config";
import { claimPoolReward } from "@/earnings/commissionTx";
import {
  POOL_REWARD_INELIGIBLE_LABELS,
  canClaimPoolReward,
  capProgressPercent,
  formatBlocksToTime,
  formatRateSnapshot,
  poolRewardIneligibleReason,
} from "@/earnings/poolRewardQueries";
import { usePoolRewardEarnings } from "@/hooks/usePoolRewardEarnings";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useWallet } from "@/hooks/useWallet";
import { formatBalance, formatUsdt } from "@/market/format";
import { useUiStore } from "@/state/uiStore";
import { WeChatNavBar } from "@/ui/WeChatNavBar";

// EN: Me tab — pool reward claim detail (奖池领取).
// CN: 「我」Tab——奖池领取详情页（对齐 nexus-com-dapp /earnings/pool-reward）。
export function EarningsPoolRewardPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const entityId = useUiStore((s) => s.currentEntityId);
  const { address } = useWallet();
  const { memberView, poolBalance, funding, currentBlock, loading, error, refresh } =
    usePoolRewardEarnings(address, entityId, true);
  const tx = useMarketTx(() => void refresh());

  const canClaim = canClaimPoolReward(memberView);
  const ineligible = poolRewardIneligibleReason(memberView);
  const capPct = capProgressPercent(memberView);
  const remainingBlocks = useMemo(() => {
    if (!memberView?.roundEndBlock || !currentBlock) return 0;
    return Math.max(0, memberView.roundEndBlock - currentBlock);
  }, [memberView, currentBlock]);

  const claimHistory = useMemo(
    () => [...(memberView?.claimHistory ?? [])].reverse(),
    [memberView],
  );

  const showLoading = loading && !memberView;

  return (
    <main className="tg-main wx-earnings-main wx-pr-earnings-main">
      <WeChatNavBar
        title="奖池领取"
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

      <div className="wx-earnings-body wx-pr-earnings-body">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上奖池。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        {!address && <p className="wx-market-empty">请先解锁钱包</p>}
        {entityId == null && address && (
          <p className="wx-market-empty">请先在「我 → 实体」中选择实体</p>
        )}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {address && entityId != null && (
          <div className="wx-pr-stack">
            <div className="wx-pr-badges">
              {memberView?.isPaused && <span className="wx-pr-badge warn">已暂停</span>}
              {memberView?.tokenPoolEnabled && (
                <span className="wx-pr-badge outline">代币池已开启</span>
              )}
              {memberView?.hasPendingConfig && (
                <span className="wx-pr-badge muted">配置变更待生效</span>
              )}
            </div>

            <section className="wx-pr-card primary">
              <header className="wx-pr-card-head">
                <span className="wx-pr-card-icon" aria-hidden>
                  💼
                </span>
                <div>
                  <h2 className="wx-pr-card-title">我的参与</h2>
                  <p className="wx-pr-card-desc">当前可领取状态与封顶进度</p>
                </div>
              </header>

              {showLoading ? (
                <div className="wx-earnings-skeleton-block lg" />
              ) : !memberView ? (
                <p className="wx-pr-empty-inline">未配置奖池领取或暂无会员数据</p>
              ) : (
                <>
                  <div className="wx-pr-metrics">
                    <div className="wx-pr-metric">
                      <span>上次领取轮次</span>
                      <strong>{memberView.lastClaimedRound || "-"}</strong>
                    </div>
                    <div className="wx-pr-metric">
                      <span>您可领取的 NEX</span>
                      <strong className="green">
                        {BigInt(memberView.claimableNex) > 0n
                          ? `${formatBalance(memberView.claimableNex, 12, 0)} NEX`
                          : "-"}
                      </strong>
                    </div>
                    {memberView.effectiveLevel > 0 && (
                      <div className="wx-pr-metric">
                        <span>有效等级</span>
                        <strong>Lv.{memberView.effectiveLevel}</strong>
                      </div>
                    )}
                    {memberView.tokenPoolEnabled && (
                      <div className="wx-pr-metric">
                        <span>可领取 USDT</span>
                        <strong className="green">
                          {BigInt(memberView.claimableToken) > 0n
                            ? `${formatBalance(memberView.claimableToken)} USDT`
                            : "-"}
                        </strong>
                      </div>
                    )}
                  </div>

                  <div className="wx-pr-cap">
                    <div className="wx-pr-cap-head">
                      <span>累计封顶进度</span>
                      <span className="wx-pr-cap-values">
                        {formatUsdt(memberView.capInfo.cumulativeClaimedUsdt, 0)} /{" "}
                        {formatUsdt(memberView.capInfo.currentCapUsdt, 0)} USDT
                      </span>
                    </div>
                    <div className="wx-pr-cap-bar">
                      <div
                        className={`wx-pr-cap-fill${memberView.capInfo.isCapped ? " capped" : ""}`}
                        style={{ width: `${Math.min(capPct, 100)}%` }}
                      />
                    </div>
                    <div className="wx-pr-cap-foot">
                      <span>{capPct.toFixed(1)}%</span>
                      <span>
                        剩余：{formatUsdt(memberView.capInfo.remainingCapUsdt, 0)} USDT
                      </span>
                    </div>
                    {memberView.capInfo.isCapped && (
                      <p className="wx-pr-cap-warn">
                        {memberView.capInfo.unlockPercent
                          ? "已达封顶，可通过团队解锁提升上限"
                          : "已达固定累计封顶"}
                      </p>
                    )}
                  </div>

                  <div className="wx-pr-claim-row">
                    <div>
                      <p className="wx-pr-claim-title">领取奖励</p>
                      <p className="wx-pr-claim-hint">
                        {canClaim ? (
                          <span className="green">可以领取奖励!</span>
                        ) : ineligible ? (
                          POOL_REWARD_INELIGIBLE_LABELS[ineligible]
                        ) : (
                          "暂无可领"
                        )}
                      </p>
                    </div>
                    <button
                      type="button"
                      className={`wx-pr-claim-btn${canClaim ? " active" : ""}`}
                      disabled={!canClaim || tx.busy || entityId == null}
                      onClick={() =>
                        entityId != null &&
                        void tx.run(() => claimPoolReward(entityId))
                      }
                    >
                      {tx.busy ? "领取中…" : canClaim ? "领取" : "暂无可领"}
                    </button>
                  </div>

                  {tx.status === "ok" && (
                    <p className="wx-market-tx-status ok">领取交易已提交</p>
                  )}
                  {tx.status === "error" && (
                    <p className="wx-market-tx-status error">{tx.error ?? "领取失败"}</p>
                  )}

                  {claimHistory.length > 0 ? (
                    <div className="wx-pr-history">
                      <p className="wx-pr-history-title">领取记录</p>
                      {claimHistory.map((r, i) => (
                        <div key={`${r.roundId}-${r.claimedAt}-${i}`} className="wx-pr-history-row">
                          <div>
                            <strong>第 {r.roundId} 轮</strong>
                            <span className="wx-pr-history-meta">
                              Lv.{r.levelId} · #{r.claimedAt}
                            </span>
                          </div>
                          <div className="wx-pr-history-amt">
                            <span>{formatBalance(r.amount, 12, 0)} NEX</span>
                            {BigInt(r.tokenAmount) > 0n && (
                              <span className="wx-pr-history-sub">
                                + {formatBalance(r.tokenAmount)} USDT
                              </span>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <p className="wx-pr-empty-inline">暂无领取记录</p>
                  )}
                </>
              )}
            </section>

            <section className="wx-pr-card">
              <header className="wx-pr-card-head">
                <span className="wx-pr-card-icon" aria-hidden>
                  🕐
                </span>
                <div>
                  <h2 className="wx-pr-card-title">当前轮次信息</h2>
                  <p className="wx-pr-card-desc">当前奖池奖励轮次的信息</p>
                </div>
              </header>

              {showLoading ? (
                <div className="wx-earnings-skeleton-block sm" />
              ) : (
                <>
                  <div className="wx-pr-metrics round">
                    <div className="wx-pr-metric">
                      <span>轮次 ID</span>
                      <strong>{memberView?.currentRoundId || "-"}</strong>
                    </div>
                    <div className="wx-pr-metric">
                      <span>开始区块</span>
                      <strong>
                        {memberView?.roundStartBlock ? `#${memberView.roundStartBlock}` : "-"}
                      </strong>
                    </div>
                    <div className="wx-pr-metric">
                      <span>结束区块</span>
                      <strong>
                        {memberView?.roundEndBlock ? `#${memberView.roundEndBlock}` : "-"}
                      </strong>
                    </div>
                    <div className="wx-pr-metric">
                      <span>剩余区块</span>
                      <strong className={remainingBlocks > 0 && remainingBlocks < 100 ? "orange" : ""}>
                        {memberView ? String(remainingBlocks) : "-"}
                      </strong>
                      {remainingBlocks > 0 && (
                        <em>{formatBlocksToTime(remainingBlocks)}</em>
                      )}
                    </div>
                  </div>

                  <div className="wx-pr-highlight">
                    <span>沉淀池余额</span>
                    <strong>{formatBalance(poolBalance, 12, 0)} NEX</strong>
                  </div>

                  {memberView?.tokenPoolSnapshot && BigInt(memberView.tokenPoolSnapshot) > 0n && (
                    <div className="wx-pr-highlight">
                      <span>USDT 池快照</span>
                      <strong>{formatBalance(memberView.tokenPoolSnapshot)} Token</strong>
                    </div>
                  )}

                  {memberView?.capInfo.rateSnapshotUsed != null && (
                    <div className="wx-pr-highlight compact">
                      <span>本轮汇率快照</span>
                      <strong>
                        {formatRateSnapshot(memberView.capInfo.rateSnapshotUsed)} USDT / NEX
                      </strong>
                    </div>
                  )}

                  {funding && funding.totalFundingCount > 0 && (
                    <div className="wx-pr-funding">
                      <p className="wx-pr-funding-title">本轮入账</p>
                      <div className="wx-pr-funding-grid">
                        {BigInt(funding.nexCommissionRemainder) > 0n && (
                          <div>
                            <span>订单佣金</span>
                            <strong>{formatBalance(funding.nexCommissionRemainder, 12, 0)} NEX</strong>
                          </div>
                        )}
                        {BigInt(funding.tokenPlatformFeeRetention) > 0n && (
                          <div>
                            <span>代币平台费</span>
                            <strong>{formatBalance(funding.tokenPlatformFeeRetention)} Token</strong>
                          </div>
                        )}
                        {BigInt(funding.tokenCommissionRemainder) > 0n && (
                          <div>
                            <span>代币佣金</span>
                            <strong>{formatBalance(funding.tokenCommissionRemainder)} Token</strong>
                          </div>
                        )}
                        {BigInt(funding.nexCancelReturn) > 0n && (
                          <div>
                            <span>取消退回</span>
                            <strong>{formatBalance(funding.nexCancelReturn, 12, 0)} NEX</strong>
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </>
              )}
            </section>
          </div>
        )}
      </div>
    </main>
  );
}
