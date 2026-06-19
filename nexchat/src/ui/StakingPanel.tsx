import { useEffect, useMemo, useState } from "react";
import { config } from "@/config";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useNexBalance } from "@/hooks/useNexBalance";
import { useStaking } from "@/hooks/useStaking";
import { useWallet } from "@/hooks/useWallet";
import { formatBalance } from "@/market/format";
import { isUnlockChunkWithdrawable } from "@/staking/stakingQueries";
import {
  bondExtraStaking,
  bondStaking,
  nominateStaking,
  unbondStaking,
  withdrawUnbondedStaking,
} from "@/staking/stakingTx";
import { useUiStore } from "@/state/uiStore";
import { parseNexAmount } from "@/wallet/amount";
import { shortAddress } from "@/wallet/address";

// EN: Me tab — nominator staking (bond / nominate / unbond), mirrors nexus-com-dapp /me/staking nominator tab.
// CN: 「我」Tab——提名人质押（绑定 / 提名 / 解押），对齐 nexus-com-dapp /me/staking 提名人页。
export function StakingPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const { address, isConnected } = useWallet();
  const { overview, loading, error, refresh } = useStaking(address, !config.useMock);
  const { balance: nexBal } = useNexBalance(address, !config.useMock);
  const { balance: stashBal, loading: stashBalLoading } = useNexBalance(
    overview?.ledger ? overview.stash : null,
    !config.useMock,
  );

  const tx = useMarketTx(() => void refresh());
  const txUnbond = useMarketTx(() => void refresh());

  const [amountStr, setAmountStr] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [stashCopied, setStashCopied] = useState(false);

  const busy = tx.busy || txUnbond.busy;

  useEffect(() => {
    if (overview?.nominations?.length) {
      setSelected(new Set(overview.nominations));
    }
  }, [overview?.stash, overview?.nominations?.join(",")]);

  const amountBn = useMemo(() => {
    if (!amountStr.trim()) return null;
    return parseNexAmount(amountStr);
  }, [amountStr]);

  const signingIsController = !!(address && overview && address === overview.controller);
  const signingIsStash = !!(address && overview && address === overview.stash);

  const canBondNew = !!overview && !overview.ledger && signingIsStash && overview.supported;
  const canControllerOps = !!overview && !!overview.ledger && signingIsController;
  const wrongAccountForOps =
    !!overview?.supported && !!overview.ledger && !signingIsController && overview.role === "stash";

  const withdrawable = useMemo(() => {
    if (!overview?.ledger) return 0n;
    let s = 0n;
    for (const u of overview.ledger.unlocking) {
      if (isUnlockChunkWithdrawable(u.era, overview.activeEraIndex)) s += u.value;
    }
    return s;
  }, [overview]);

  const pendingUnlock = useMemo(() => {
    if (!overview?.ledger) return 0n;
    let s = 0n;
    for (const u of overview.ledger.unlocking) {
      if (!isUnlockChunkWithdrawable(u.era, overview.activeEraIndex)) s += u.value;
    }
    return s;
  }, [overview]);

  function toggleValidator(v: string) {
    if (!overview) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(v)) next.delete(v);
      else if (next.size < overview.maxNominations) next.add(v);
      return next;
    });
  }

  async function handleBond() {
    if (!address || !amountBn || busy || !overview?.supported) return;
    await tx.run(() => bondStaking(address, amountBn));
    setAmountStr("");
  }

  async function handleBondExtra() {
    if (!amountBn || busy || !canControllerOps) return;
    await tx.run(() => bondExtraStaking(amountBn));
    setAmountStr("");
  }

  async function handleUnbond() {
    if (!amountBn || busy || !canControllerOps) return;
    if (
      !window.confirm("确认解押？资金将进入解锁队列，在若干 Era 结束后可领取。")
    ) {
      return;
    }
    await txUnbond.run(() => unbondStaking(amountBn));
    setAmountStr("");
  }

  async function handleNominate() {
    if (busy || !canControllerOps || selected.size === 0) return;
    await tx.run(() => nominateStaking(Array.from(selected)));
  }

  async function handleWithdrawUnbonded() {
    if (busy || !canControllerOps || withdrawable <= 0n) return;
    await tx.run(() => withdrawUnbondedStaking());
  }

  function handleCopyStash() {
    if (!overview?.stash) return;
    void navigator.clipboard.writeText(overview.stash);
    setStashCopied(true);
    setTimeout(() => setStashCopied(false), 2000);
  }

  const txError = tx.error ?? txUnbond.error;

  return (
    <main className="tg-main wx-earnings-main wx-staking-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setSettingsView("list")}>
          ‹ 返回
        </button>
        <span>节点提名</span>
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
            Mock 模式无法使用质押功能。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}

        <p className="wx-staking-subtitle">
          以提名人身份质押 NEX、选择验证人并查看解锁进度。首次绑定需使用 Stash 账户签名；加质押、解押、提名与领取需使用控制器账户。
        </p>

        <section className="wx-market-card wx-staking-links-card">
          <p className="wx-market-label">源码与部署</p>
          <p className="wx-staking-hint">查看 Nexus 源代码仓库与部署脚本仓库。</p>
          <div className="wx-staking-link-row">
            <a
              href="https://github.com/nexusmalls/nexus.git"
              target="_blank"
              rel="noopener noreferrer"
              className="wx-staking-link-btn"
            >
              源代码 ↗
            </a>
            <a
              href="https://github.com/nexusmalls/nodedeploy.git"
              target="_blank"
              rel="noopener noreferrer"
              className="wx-staking-link-btn"
            >
              部署脚本 ↗
            </a>
          </div>
        </section>

        {!isConnected || !address ? (
          <section className="wx-market-card">
            <p className="wx-market-empty">请先解锁钱包以使用质押功能。</p>
          </section>
        ) : (
          <>
            {loading && !overview && <p className="wx-market-empty">加载质押数据…</p>}
            {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}
            {txError && <div className="wx-market-banner wx-market-banner-err">{txError}</div>}

            {overview && !overview.supported && !loading && (
              <section className="wx-market-card">
                <p className="wx-market-empty">当前链元数据中未暴露 Staking 模块。</p>
              </section>
            )}

            {overview?.supported && (
              <>
                {wrongAccountForOps && (
                  <div className="wx-market-banner">
                    当前为 Stash 账户，请切换到控制器 {shortAddress(overview.controller)} 以进行加质押、解押、提名或领取。
                  </div>
                )}

                <section className="wx-market-card">
                  <p className="wx-market-label">总览</p>
                  <div className="wx-staking-kv">
                    <span>Stash</span>
                    <span className="mono">{shortAddress(overview.stash)}</span>
                  </div>
                  <div className="wx-staking-kv">
                    <span>控制器</span>
                    <span className="mono">{shortAddress(overview.controller)}</span>
                  </div>
                  <div className="wx-staking-kv">
                    <span>当前 Era</span>
                    <span>{overview.activeEraIndex}</span>
                  </div>
                  <div className="wx-staking-kv">
                    <span>最低提名质押</span>
                    <span>{formatBalance(overview.minNominatorBond.toString())} NEX</span>
                  </div>
                  {nexBal && (
                    <div className="wx-staking-kv">
                      <span>可用余额</span>
                      <span>{formatBalance(nexBal.free.toString())} NEX</span>
                    </div>
                  )}
                  {overview.ledger && (
                    <>
                      <div className="wx-staking-kv highlight">
                        <span>活跃质押</span>
                        <strong>{formatBalance(overview.ledger.active.toString())} NEX</strong>
                      </div>
                      <div className="wx-staking-kv">
                        <span>锁定合计</span>
                        <span>{formatBalance(overview.ledger.total.toString())} NEX</span>
                      </div>
                      {pendingUnlock > 0n && (
                        <div className="wx-staking-kv">
                          <span>解锁中</span>
                          <span>{formatBalance(pendingUnlock.toString())} NEX</span>
                        </div>
                      )}
                      {withdrawable > 0n && (
                        <div className="wx-staking-kv">
                          <span>可领取</span>
                          <span>{formatBalance(withdrawable.toString())} NEX</span>
                        </div>
                      )}
                    </>
                  )}
                </section>

                {overview.ledger && (
                  <section className="wx-market-card">
                    <p className="wx-market-label">
                      我质押的验证人
                      {overview.nominations && overview.nominations.length > 0 && (
                        <span className="wx-staking-badge">已选 {overview.nominations.length} 个</span>
                      )}
                    </p>
                    <p className="wx-staking-hint">
                      以下为链上记录的提名对象，收益与验证人表现及佣金相关。
                    </p>
                    {overview.nominations && overview.nominations.length > 0 ? (
                      <ul className="wx-staking-nom-list">
                        {overview.nominations.map((v) => (
                          <li key={v} className="mono">
                            {shortAddress(v)}
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="wx-staking-warn">
                        当前尚未提名验证人。在下方列表中选择后点击「更新提名」。
                      </p>
                    )}
                  </section>
                )}

                <section className="wx-market-card">
                  <p className="wx-market-label">数量 (NEX)</p>
                  <input
                    className="wx-staking-input"
                    inputMode="decimal"
                    placeholder="例如 100"
                    value={amountStr}
                    onChange={(e) => setAmountStr(e.target.value)}
                    disabled={busy}
                  />
                  {amountStr && amountBn == null && (
                    <p className="wx-staking-err">请输入有效的 NEX 数量。</p>
                  )}
                  <div className="wx-staking-btn-row">
                    {canBondNew && (
                      <button
                        type="button"
                        className="wx-market-submit buy"
                        disabled={!amountBn || busy}
                        onClick={() => void handleBond()}
                      >
                        {busy ? "提交中…" : "首次绑定"}
                      </button>
                    )}
                    {canControllerOps && (
                      <>
                        <button
                          type="button"
                          className="wx-market-submit"
                          disabled={!amountBn || busy}
                          onClick={() => void handleBondExtra()}
                        >
                          追加质押
                        </button>
                        <button
                          type="button"
                          className="wx-market-submit danger"
                          disabled={!amountBn || busy}
                          onClick={() => void handleUnbond()}
                        >
                          解押
                        </button>
                      </>
                    )}
                  </div>
                  {canBondNew && (
                    <p className="wx-staking-hint">
                      首次绑定会创建质押账本，随后在验证人列表中选择并点击「更新提名」。
                    </p>
                  )}
                </section>

                {overview.ledger && canControllerOps && withdrawable > 0n && (
                  <button
                    type="button"
                    className="wx-market-submit wx-staking-withdraw-btn"
                    disabled={busy}
                    onClick={() => void handleWithdrawUnbonded()}
                  >
                    领取已解锁
                  </button>
                )}

                {overview.validators.length === 0 ? (
                  <section className="wx-market-card">
                    <p className="wx-market-empty">链上未返回活跃验证人（session）。</p>
                  </section>
                ) : (
                  <section className="wx-market-card">
                    <p className="wx-market-label">
                      活跃验证人
                      <span className="wx-staking-badge muted">
                        {selected.size}/{overview.maxNominations}
                      </span>
                    </p>
                    <div className="wx-staking-validator-list">
                      {overview.validators.map((v) => {
                        const on = selected.has(v);
                        return (
                          <button
                            key={v}
                            type="button"
                            disabled={busy || (!on && selected.size >= overview.maxNominations)}
                            className={`wx-staking-validator-row${on ? " on" : ""}`}
                            onClick={() => toggleValidator(v)}
                          >
                            <span className="mono">{shortAddress(v)}</span>
                            {on && <span className="wx-staking-validator-tag">已选</span>}
                          </button>
                        );
                      })}
                    </div>
                    {canControllerOps ? (
                      <button
                        type="button"
                        className="wx-market-submit buy wx-staking-nominate-btn"
                        disabled={busy || selected.size === 0}
                        onClick={() => void handleNominate()}
                      >
                        更新提名
                      </button>
                    ) : overview.ledger ? (
                      <p className="wx-staking-hint">请使用控制器账户连接以修改提名。</p>
                    ) : null}
                  </section>
                )}

                {overview.ledger?.unlocking?.length ? (
                  <section className="wx-market-card">
                    <p className="wx-market-label">解锁进度</p>
                    {overview.ledger.unlocking.map((u, i) => (
                      <div key={i} className="wx-staking-kv small">
                        <span>{formatBalance(u.value.toString())} NEX</span>
                        <span className="muted">
                          Era {u.era}
                          {isUnlockChunkWithdrawable(u.era, overview.activeEraIndex) ? " · 可领" : ""}
                        </span>
                      </div>
                    ))}
                  </section>
                ) : null}

                <section className="wx-market-card">
                  <p className="wx-market-label">收益说明</p>
                  <p className="wx-staking-hint">
                    收益与验证人出块及佣金相关，按链上规则计入 Stash；精确记录请使用区块浏览器查询。
                  </p>
                  {overview.ledger && (
                    <>
                      <p className="wx-staking-hint">当前提名验证人见上方「我质押的验证人」。</p>
                      {(stashBalLoading || stashBal) && (
                        <div className="wx-staking-stash-bal">
                          <span className="wx-market-label">Stash 可用余额（奖励常见入账位置之一）</span>
                          <strong>
                            {stashBalLoading ? "…" : `${formatBalance(stashBal!.free.toString())} NEX`}
                          </strong>
                        </div>
                      )}
                      <p className="wx-staking-hint pre">
                        1）首次绑定使用「并入质押（Staked）」作为奖励去向，收益多数会体现在活跃质押随 Era 缓慢增加。
                        {"\n"}
                        2）本页不提供「每笔收益流水」；需结合解锁、手续费与链上规则综合理解数字变化。
                        {"\n"}
                        3）若要按 Era、转账、payout 等查历史，请在区块浏览器中搜索你的 Stash 地址。
                      </p>
                      <button type="button" className="wx-staking-link-btn inline" onClick={handleCopyStash}>
                        {stashCopied ? "已复制" : "复制 Stash 地址"}
                      </button>
                    </>
                  )}
                </section>
              </>
            )}
          </>
        )}
      </div>
    </main>
  );
}
