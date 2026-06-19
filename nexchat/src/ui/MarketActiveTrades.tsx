import { useCallback, useState } from "react";
import { useMarketTx } from "@/hooks/useMarketTx";
import { formatBalance, formatUsdt } from "@/market/format";
import {
  confirmPayment,
  processTimeout,
  sellerConfirmReceived,
} from "@/market/nexMarketTx";
import type { NexMarketTrade } from "@/market/types";
import { canonicalAddress, shortAddress } from "@/wallet/address";

const ACTIVE_STATUSES = new Set([
  "AwaitingPayment",
  "AwaitingVerification",
  "UnderpaidPending",
]);

const STATUS_LABEL: Record<string, string> = {
  AwaitingPayment: "待付款",
  AwaitingVerification: "待验证",
  UnderpaidPending: "少付待处理",
  Completed: "已完成",
  Refunded: "已退款",
  Cancelled: "已取消",
  Disputed: "争议中",
};

interface MarketActiveTradesProps {
  trades: NexMarketTrade[];
  address: string | null;
  onSuccess: () => void;
}

// EN: Active USDT settlement trades — confirm payment / confirm received.
// CN: 进行中 USDT 结算交易——确认付款 / 确认收款。
export function MarketActiveTrades({ trades, address, onSuccess }: MarketActiveTradesProps) {
  const active = trades.filter((t) => ACTIVE_STATUSES.has(t.status));

  return (
    <section className="wx-market-card">
      <h3 className="wx-market-section-title">
        进行中交易{active.length > 0 ? ` (${active.length})` : ""}
      </h3>
      {active.length === 0 ? (
        <p className="wx-market-empty">暂无待处理交易</p>
      ) : (
        <div className="wx-market-trade-list">
          {active.map((trade) => (
            <ActiveTradeCard
              key={trade.tradeId}
              trade={trade}
              address={address}
              onSuccess={onSuccess}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function ActiveTradeCard({
  trade,
  address,
  onSuccess,
}: {
  trade: NexMarketTrade;
  address: string | null;
  onSuccess: () => void;
}) {
  const who = address ? canonicalAddress(address) : null;
  const isBuyer = who != null && canonicalAddress(trade.buyer) === who;
  const isSeller = who != null && canonicalAddress(trade.seller) === who;

  const payTx = useMarketTx(onSuccess);
  const recvTx = useMarketTx(onSuccess);
  const timeoutTx = useMarketTx(onSuccess);

  const [copied, setCopied] = useState(false);
  const tronToPay = trade.sellerTronAddress;

  const handleCopy = useCallback(async () => {
    if (!tronToPay) return;
    try {
      await navigator.clipboard.writeText(tronToPay);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }, [tronToPay]);

  const roleLabel = isBuyer ? "买家" : isSeller ? "卖家" : "旁观";

  return (
    <div className="wx-market-trade-card">
      <div className="wx-market-trade-head">
        <span className="wx-market-trade-id">#{trade.tradeId}</span>
        <span className="wx-market-trade-role">{roleLabel}</span>
        <span className={`wx-market-trade-status status-${trade.status}`}>
          {STATUS_LABEL[trade.status] ?? trade.status}
        </span>
      </div>

      <div className="wx-market-trade-meta">
        <span>NEX {formatBalance(trade.nexAmount)}</span>
        <span>USDT ${formatUsdt(trade.usdtAmount)}</span>
        {BigInt(trade.buyerDeposit) > 0n && (
          <span>保证金 {formatBalance(trade.buyerDeposit)}</span>
        )}
      </div>

      {isBuyer && tronToPay && trade.status === "AwaitingPayment" && (
        <div className="wx-market-tron-row">
          <span className="wx-market-tron-label">付款至</span>
          <span className="wx-market-tron-addr" title={tronToPay}>
            {shortAddress(tronToPay, 10, 6)}
          </span>
          <button type="button" className="wx-market-tron-copy" onClick={() => void handleCopy()}>
            {copied ? "已复制" : "复制"}
          </button>
        </div>
      )}

      <div className="wx-market-trade-actions">
        {isBuyer && trade.status === "AwaitingPayment" && !trade.paymentConfirmed && (
          <button
            type="button"
            className="wx-market-trade-btn confirm-pay"
            disabled={payTx.busy}
            onClick={() => void payTx.run(() => confirmPayment(trade.tradeId))}
          >
            {payTx.busy ? "提交中…" : "确认付款"}
          </button>
        )}

        {isSeller &&
          (trade.status === "AwaitingVerification" || trade.status === "UnderpaidPending") && (
            <button
              type="button"
              className="wx-market-trade-btn confirm-recv"
              disabled={recvTx.busy}
              onClick={() => void recvTx.run(() => sellerConfirmReceived(trade.tradeId))}
            >
              {recvTx.busy ? "提交中…" : "确认收款"}
            </button>
          )}

        {(isBuyer || isSeller) && (
          <button
            type="button"
            className="wx-market-trade-btn timeout"
            disabled={timeoutTx.busy}
            onClick={() => void timeoutTx.run(() => processTimeout(trade.tradeId))}
          >
            {timeoutTx.busy ? "…" : "处理超时"}
          </button>
        )}
      </div>

      <TxLine tx={payTx} okText="已确认付款" />
      <TxLine tx={recvTx} okText="已确认收款" />
      <TxLine tx={timeoutTx} okText="超时已处理" />
    </div>
  );
}

function TxLine({
  tx,
  okText,
}: {
  tx: ReturnType<typeof useMarketTx>;
  okText: string;
}) {
  if (tx.status === "idle") return null;
  return (
    <p className={`wx-market-tx-status ${tx.status}`}>
      {tx.status === "pending" && "签名并广播中…"}
      {tx.status === "ok" && okText}
      {tx.status === "error" && (tx.error ?? "操作失败")}
    </p>
  );
}
