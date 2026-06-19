import { useMemo } from "react";
import { config } from "@/config";
import { POOL_LABELS } from "@/chain/poolDefs";
import type { PoolInfo } from "@/chain/chainQueries";
import { useChainInfo } from "@/hooks/useChainInfo";
import { useGlobalPools } from "@/hooks/useGlobalPools";
import { formatBalance } from "@/market/format";
import { useUiStore } from "@/state/uiStore";
import { shortAddress } from "@/wallet/address";

function truncateEndpoint(url: string, max = 36): string {
  if (url.length <= max) return url;
  const head = Math.floor(max * 0.55);
  const tail = max - head - 1;
  return `${url.slice(0, head)}…${url.slice(-tail)}`;
}

function PoolRow({
  pool,
  decimals,
  symbol,
}: {
  pool: PoolInfo;
  decimals: number;
  symbol: string;
}) {
  const label = POOL_LABELS[pool.key];
  return (
    <div className="wx-chain-pool-row">
      <div className="wx-chain-pool-head">
        <div>
          <p className="wx-chain-pool-name">{label?.name ?? pool.key}</p>
          {label?.desc && <p className="wx-chain-pool-desc">{label.desc}</p>}
        </div>
        <span className="wx-chain-pool-id mono">{pool.palletId}</span>
      </div>
      <div className="wx-staking-kv small">
        <span className="mono muted">{shortAddress(pool.address, 8)}</span>
        <strong>
          {formatBalance(pool.free, decimals, 2)} {symbol}
        </strong>
      </div>
      {pool.reserved > 0n && (
        <div className="wx-staking-kv small">
          <span className="muted">锁定余额</span>
          <span className="mono muted">
            {formatBalance(pool.reserved, decimals, 2)} {symbol}
          </span>
        </div>
      )}
    </div>
  );
}

function PoolGroup({
  title,
  pools,
  decimals,
  symbol,
}: {
  title: string;
  pools: PoolInfo[];
  decimals: number;
  symbol: string;
}) {
  const totalFree = pools.reduce((sum, p) => sum + p.free, 0n);
  return (
    <section className="wx-market-card wx-chain-pool-group">
      <div className="wx-chain-pool-group-head">
        <p className="wx-market-label">{title}</p>
        <span className="wx-chain-pool-total mono">
          总计: {formatBalance(totalFree, decimals, 2)} {symbol}
        </span>
      </div>
      {pools.map((pool) => (
        <PoolRow key={pool.palletId} pool={pool} decimals={decimals} symbol={symbol} />
      ))}
    </section>
  );
}

// EN: Me tab — chain overview, runtime, network, and global fund pools (/chain).
// CN: 「我」Tab——链概览、运行时、网络与全局资金池（对齐 /chain）。
export function ChainPanel() {
  const setSettingsView = useUiStore((s) => s.setSettingsView);
  const enabled = !config.useMock;
  const { info, loading, error, refresh } = useChainInfo(enabled);
  const {
    pools,
    loading: poolsLoading,
    error: poolsError,
    refresh: refreshPools,
  } = useGlobalPools(enabled);

  const activeEndpoint = config.wsEndpoint;
  const decimals = info?.tokenDecimals ?? 12;
  const symbol = info?.tokenSymbol ?? "NEX";

  const refreshAll = () => {
    void refresh();
    void refreshPools();
  };

  const busy = loading || poolsLoading;

  const syncBadge = useMemo(() => {
    if (!info) return null;
    return info.isSyncing ? "同步中" : "已同步";
  }, [info]);

  return (
    <main className="tg-main wx-earnings-main wx-chain-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setSettingsView("list")}>
          ‹ 返回
        </button>
        <span>链上详情</span>
        <button type="button" className="wx-market-refresh" onClick={refreshAll} disabled={busy}>
          {busy ? "…" : "刷新"}
        </button>
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上数据。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}

        {enabled && loading && !info && <p className="wx-market-empty">正在连接链…</p>}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}
        {poolsError && <div className="wx-market-banner wx-market-banner-err">{poolsError}</div>}

        {info && (
          <>
            <section className="wx-market-card wx-chain-overview-card">
              <p className="wx-market-label">链概览</p>
              <div className="wx-chain-grid">
                <div>
                  <span className="wx-market-label">链名称</span>
                  <p className="wx-chain-value">{info.chainName}</p>
                </div>
                <div>
                  <span className="wx-market-label">最新区块</span>
                  <p className="wx-chain-value highlight mono">#{info.bestBlock.toLocaleString()}</p>
                </div>
                <div>
                  <span className="wx-market-label">已确认区块</span>
                  <p className="wx-chain-value mono">#{info.finalizedBlock.toLocaleString()}</p>
                </div>
                <div>
                  <span className="wx-market-label">同步状态</span>
                  <p className={`wx-chain-sync${info.isSyncing ? " warn" : " ok"}`}>{syncBadge}</p>
                </div>
              </div>
            </section>

            <section className="wx-market-card">
              <p className="wx-market-label">代币信息</p>
              <div className="wx-staking-kv">
                <span>代币符号</span>
                <span className="wx-chain-tag mono">{info.tokenSymbol}</span>
              </div>
              <div className="wx-staking-kv">
                <span>精度</span>
                <span className="mono">{info.tokenDecimals}</span>
              </div>
              <div className="wx-staking-kv highlight">
                <span>总发行量</span>
                <strong>
                  {formatBalance(info.totalIssuance, info.tokenDecimals, 2)} {info.tokenSymbol}
                </strong>
              </div>
            </section>

            <section className="wx-market-card">
              <p className="wx-market-label">运行时</p>
              <div className="wx-chain-grid">
                <div>
                  <span className="wx-market-label">规范名称</span>
                  <p className="wx-chain-value">{info.specName}</p>
                </div>
                <div>
                  <span className="wx-market-label">规范版本</span>
                  <p className="wx-chain-value">{info.specVersion}</p>
                </div>
                <div>
                  <span className="wx-market-label">实现版本</span>
                  <p className="wx-chain-value">{info.implVersion}</p>
                </div>
                <div>
                  <span className="wx-market-label">SS58 格式</span>
                  <p className="wx-chain-value">{info.ss58Format}</p>
                </div>
              </div>
            </section>

            <section className="wx-market-card">
              <p className="wx-market-label">网络</p>
              <div className="wx-staking-kv">
                <span>连接节点数</span>
                <strong>{info.peerCount}</strong>
              </div>
              <div className="wx-staking-kv">
                <span>当前连接节点</span>
                <span className="mono wx-chain-endpoint">{truncateEndpoint(activeEndpoint)}</span>
              </div>
              <div className="wx-staking-kv">
                <span>节点名称</span>
                <span>{info.nodeName}</span>
              </div>
              <div className="wx-staking-kv">
                <span>节点版本</span>
                <span className="mono wx-chain-endpoint">{info.nodeVersion}</span>
              </div>
            </section>

            {poolsLoading && !pools && <p className="wx-market-empty">加载资金池…</p>}

            {pools && (
              <>
                <PoolGroup title="核心系统账户" pools={pools.core} decimals={decimals} symbol={symbol} />
                <PoolGroup title="交易所账户" pools={pools.market} decimals={decimals} symbol={symbol} />
                <PoolGroup title="基础设施账户" pools={pools.infra} decimals={decimals} symbol={symbol} />
              </>
            )}
          </>
        )}

        {enabled && !loading && !info && !error && (
          <section className="wx-market-card">
            <p className="wx-market-empty">未连接到链</p>
            <button type="button" className="wx-market-submit buy" onClick={() => void refresh()}>
              重试
            </button>
          </section>
        )}
      </div>
    </main>
  );
}
