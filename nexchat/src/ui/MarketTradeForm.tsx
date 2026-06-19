import { useEffect, useMemo, useState } from "react";
import { useMarketTx } from "@/hooks/useMarketTx";
import { formatNexPrice, formatUsdt } from "@/market/format";
import {
  acceptBuyOrder,
  placeBuyOrder,
  placeSellOrder,
  reserveSellOrder,
} from "@/market/nexMarketTx";
import { computePriceBand, validateLimitPrice } from "@/market/referencePrice";
import type { MarketSnapshot, NexMarketOrder, OrderActionPrefill } from "@/market/types";
import { estimateTotal, isValidTronAddress, validateAmount } from "@/market/validate";

type FormTab = "limit" | "take";

interface MarketTradeFormProps {
  snapshot: MarketSnapshot | null;
  allOrders: NexMarketOrder[];
  address: string | null;
  prefill: OrderActionPrefill | null;
  onPrefillUsed: () => void;
  onSuccess: () => void;
}

// EN: Limit-order + take-order forms for NEX global market.
// CN: NEX 全局市场限价挂单与吃单表单。
export function MarketTradeForm({
  snapshot,
  allOrders,
  address,
  prefill,
  onPrefillUsed,
  onSuccess,
}: MarketTradeFormProps) {
  const [tab, setTab] = useState<FormTab>("limit");

  useEffect(() => {
    if (prefill) {
      setTab("take");
      requestAnimationFrame(() => {
        document.getElementById("wx-market-trade-form")?.scrollIntoView({
          behavior: "smooth",
          block: "start",
        });
      });
    }
  }, [prefill]);

  return (
    <section id="wx-market-trade-form" className="wx-market-card">
      <div className="wx-market-tabs">
        <button
          type="button"
          className={`wx-market-tab${tab === "limit" ? " active" : ""}`}
          onClick={() => setTab("limit")}
        >
          限价挂单
        </button>
        <button
          type="button"
          className={`wx-market-tab${tab === "take" ? " active" : ""}`}
          onClick={() => setTab("take")}
        >
          吃单
        </button>
      </div>

      {tab === "limit" ? (
        <LimitOrderForm snapshot={snapshot} address={address} onSuccess={onSuccess} />
      ) : (
        <TakeOrderForm
          allOrders={allOrders}
          address={address}
          prefill={prefill}
          onPrefillUsed={onPrefillUsed}
          onSuccess={onSuccess}
        />
      )}

      {!address && (
        <p className="wx-market-form-hint">请先解锁钱包后再交易</p>
      )}
    </section>
  );
}

function LimitOrderForm({
  snapshot,
  address,
  onSuccess,
}: {
  snapshot: MarketSnapshot | null;
  address: string | null;
  onSuccess: () => void;
}) {
  const [side, setSide] = useState<"Buy" | "Sell">("Buy");
  const [price, setPrice] = useState("");
  const [amount, setAmount] = useState("");
  const [tron, setTron] = useState("");
  const [minFill, setMinFill] = useState("");

  const buyTx = useMarketTx(onSuccess);
  const sellTx = useMarketTx(onSuccess);
  const activeTx = side === "Buy" ? buyTx : sellTx;

  const priceValidation = useMemo(() => validateAmount(price, "USDT"), [price]);
  const amountValidation = useMemo(() => validateAmount(amount, "NEX"), [amount]);
  const minFillValidation = useMemo(
    () => (minFill.trim() ? validateAmount(minFill, "NEX") : null),
    [minFill],
  );
  const tronValid = tron.trim() !== "" && isValidTronAddress(tron);

  const band = useMemo(
    () =>
      snapshot
        ? computePriceBand(snapshot.protection, snapshot.stats.referencePrice)
        : { referencePrice: null, minPrice: null, maxPrice: null, maxDeviationBps: 0 },
    [snapshot],
  );

  const priceRangeCheck = useMemo(() => {
    if (!priceValidation.valid || priceValidation.value == null || !snapshot) return null;
    return validateLimitPrice(priceValidation.value, snapshot.protection, band);
  }, [priceValidation, snapshot, band]);

  const priceRangeError = useMemo(() => {
    if (!priceRangeCheck || priceRangeCheck.ok) return null;
    if (priceRangeCheck.reason === "circuit_breaker") return "价格熔断已触发，暂不可挂单";
    if (band.minPrice == null || band.maxPrice == null) return null;
    return `价格超出允许区间 $${formatNexPrice(band.minPrice.toString())} ~ $${formatNexPrice(band.maxPrice.toString())}`;
  }, [priceRangeCheck, band]);

  const minFillWithinAmount =
    !minFillValidation?.valid || !amountValidation.valid
      ? true
      : minFillValidation.value! <= amountValidation.value!;

  const estimatedUsdt = price && amount ? estimateTotal(price, amount, 6, 12, 2) : null;

  const canSubmit =
    !!address &&
    !activeTx.busy &&
    priceValidation.valid &&
    amountValidation.valid &&
    tronValid &&
    (!priceRangeCheck || priceRangeCheck.ok) &&
    (side !== "Sell" || !minFill.trim() || (!!minFillValidation?.valid && minFillWithinAmount));

  const handleSubmit = () => {
    if (!canSubmit || !priceValidation.value || !amountValidation.value) return;
    const priceRaw = priceValidation.value.toString();
    const amountRaw = amountValidation.value.toString();
    const minFillRaw =
      side === "Sell" && minFillValidation?.valid ? minFillValidation.value!.toString() : null;

    if (side === "Buy") {
      void buyTx.run(() => placeBuyOrder(amountRaw, priceRaw, tron));
    } else {
      void sellTx.run(() => placeSellOrder(amountRaw, priceRaw, tron, minFillRaw));
    }
    setPrice("");
    setAmount("");
    setMinFill("");
  };

  return (
    <div className="wx-market-form">
      <div className="wx-market-side-toggle">
        <button
          type="button"
          className={`wx-market-side-btn buy${side === "Buy" ? " active" : ""}`}
          onClick={() => setSide("Buy")}
        >
          买入
        </button>
        <button
          type="button"
          className={`wx-market-side-btn sell${side === "Sell" ? " active" : ""}`}
          onClick={() => setSide("Sell")}
        >
          卖出
        </button>
      </div>

      <label className="wx-market-field">
        <span>USDT 单价</span>
        <input
          inputMode="decimal"
          placeholder="0.00"
          value={price}
          onChange={(e) => setPrice(e.target.value)}
        />
      </label>
      {band.referencePrice != null && band.minPrice != null && band.maxPrice != null && (
        <p className="wx-market-field-hint">
          允许区间 ±{(band.maxDeviationBps / 100).toFixed(2)}%：
          ${formatNexPrice(band.minPrice.toString())} ~ ${formatNexPrice(band.maxPrice.toString())}
        </p>
      )}
      {priceRangeError && <p className="wx-market-field-err">{priceRangeError}</p>}

      <label className="wx-market-field">
        <span>NEX 数量</span>
        <input
          inputMode="decimal"
          placeholder="0"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </label>

      <label className="wx-market-field">
        <span>TRON 地址</span>
        <input
          placeholder="T..."
          value={tron}
          onChange={(e) => setTron(e.target.value)}
        />
      </label>

      {side === "Sell" && (
        <label className="wx-market-field">
          <span>最小成交量（可选）</span>
          <input
            inputMode="decimal"
            placeholder="留空表示无限制"
            value={minFill}
            onChange={(e) => setMinFill(e.target.value)}
          />
        </label>
      )}

      {estimatedUsdt && (
        <div className="wx-market-estimate">
          预计金额：<strong>${estimatedUsdt}</strong>
        </div>
      )}

      <button
        type="button"
        className={`wx-market-submit ${side === "Buy" ? "buy" : "sell"}`}
        disabled={!canSubmit}
        onClick={handleSubmit}
      >
        {activeTx.busy ? "提交中…" : side === "Buy" ? "挂买单" : "挂卖单"}
      </button>
      <TxFeedback tx={activeTx} />
    </div>
  );
}

function TakeOrderForm({
  allOrders,
  address,
  prefill,
  onPrefillUsed,
  onSuccess,
}: {
  allOrders: NexMarketOrder[];
  address: string | null;
  prefill: OrderActionPrefill | null;
  onPrefillUsed: () => void;
  onSuccess: () => void;
}) {
  const [side, setSide] = useState<"takeBuy" | "takeSell">(
    prefill?.target === "takeSell" ? "takeSell" : "takeBuy",
  );

  useEffect(() => {
    if (prefill?.target === "takeBuy") setSide("takeBuy");
    if (prefill?.target === "takeSell") setSide("takeSell");
  }, [prefill]);

  return (
    <div className="wx-market-form">
      <div className="wx-market-side-toggle">
        <button
          type="button"
          className={`wx-market-side-btn buy${side === "takeBuy" ? " active" : ""}`}
          onClick={() => setSide("takeBuy")}
        >
          吃买单
        </button>
        <button
          type="button"
          className={`wx-market-side-btn sell${side === "takeSell" ? " active" : ""}`}
          onClick={() => setSide("takeSell")}
        >
          吃卖单
        </button>
      </div>

      {side === "takeBuy" ? (
        <TakeOrderCard
          kind="takeBuy"
          allOrders={allOrders}
          address={address}
          prefill={prefill?.target === "takeBuy" ? prefill : undefined}
          onPrefillUsed={onPrefillUsed}
          onSuccess={onSuccess}
        />
      ) : (
        <TakeOrderCard
          kind="takeSell"
          allOrders={allOrders}
          address={address}
          prefill={prefill?.target === "takeSell" ? prefill : undefined}
          onPrefillUsed={onPrefillUsed}
          onSuccess={onSuccess}
        />
      )}
    </div>
  );
}

function TakeOrderCard({
  kind,
  allOrders,
  address,
  prefill,
  onPrefillUsed,
  onSuccess,
}: {
  kind: "takeBuy" | "takeSell";
  allOrders: NexMarketOrder[];
  address: string | null;
  prefill?: OrderActionPrefill;
  onPrefillUsed?: () => void;
  onSuccess: () => void;
}) {
  const [orderId, setOrderId] = useState("");
  const [amount, setAmount] = useState("");
  const [tron, setTron] = useState("");

  const tx = useMarketTx(onSuccess);

  useEffect(() => {
    if (prefill) {
      setOrderId(prefill.orderId);
      setAmount(prefill.amount);
      setTron(prefill.tron);
      onPrefillUsed?.();
    }
  }, [prefill, onPrefillUsed]);

  const matchedOrder = useMemo(() => {
    const id = Number(orderId.trim());
    if (!Number.isSafeInteger(id) || id <= 0) return null;
    return allOrders.find((o) => o.id === id) ?? null;
  }, [orderId, allOrders]);

  const orderIdValid =
    /^\d+$/.test(orderId.trim()) &&
    Number(orderId.trim()) > 0 &&
    Number.isSafeInteger(Number(orderId.trim()));
  const amountValidation = useMemo(
    () => (amount.trim() ? validateAmount(amount, "NEX") : null),
    [amount],
  );
  const tronValid = tron.trim() !== "" && isValidTronAddress(tron);

  const estimatedUsdt = useMemo(() => {
    if (!matchedOrder || !amount.trim() || !amountValidation?.valid) return null;
    try {
      return estimateTotal(formatUsdt(matchedOrder.price, 6), amount, 6, 12, 2);
    } catch {
      return null;
    }
  }, [matchedOrder, amount, amountValidation]);

  const canSubmit =
    !!address &&
    !tx.busy &&
    orderIdValid &&
    tronValid &&
    (!amount.trim() || !!amountValidation?.valid);

  const handleSubmit = () => {
    if (!canSubmit) return;
    const id = Number(orderId.trim());
    const amountRaw = amountValidation?.valid ? amountValidation.value!.toString() : null;
    if (kind === "takeBuy") {
      void tx.run(() => acceptBuyOrder(id, amountRaw, tron));
    } else {
      void tx.run(() => reserveSellOrder(id, amountRaw, tron));
    }
    setOrderId("");
    setAmount("");
    setTron("");
  };

  const desc =
    kind === "takeBuy"
      ? "作为卖家接受买单：提供 TRON 地址收 USDT，卖出 NEX"
      : "作为买家吃卖单：提供 TRON 地址付 USDT，买入 NEX";

  return (
    <div className={`wx-market-take-card ${kind}`}>
      <p className="wx-market-take-desc">{desc}</p>

      <label className="wx-market-field">
        <span>订单 ID</span>
        <input
          inputMode="numeric"
          placeholder="例如 0"
          value={orderId}
          onChange={(e) => setOrderId(e.target.value)}
        />
      </label>

      <label className="wx-market-field">
        <span>NEX 数量（可选，留空吃满）</span>
        <input
          inputMode="decimal"
          placeholder="留空表示全部成交"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
        />
      </label>

      <label className="wx-market-field">
        <span>TRON 地址</span>
        <input placeholder="T..." value={tron} onChange={(e) => setTron(e.target.value)} />
      </label>

      {matchedOrder && (
        <div className="wx-market-estimate">
          订单价 <strong>${formatNexPrice(matchedOrder.price)}</strong>
          {estimatedUsdt && (
            <>
              {" "}
              · 预计 <strong>${estimatedUsdt}</strong>
            </>
          )}
        </div>
      )}

      <button
        type="button"
        className={`wx-market-submit ${kind === "takeBuy" ? "buy" : "sell"}`}
        disabled={!canSubmit}
        onClick={handleSubmit}
      >
        {tx.busy ? "提交中…" : kind === "takeBuy" ? "吃买单" : "吃卖单"}
      </button>
      <TxFeedback tx={tx} />
    </div>
  );
}

function TxFeedback({ tx }: { tx: ReturnType<typeof useMarketTx> }) {
  if (tx.status === "idle") return null;
  return (
    <p className={`wx-market-tx-status ${tx.status}`}>
      {tx.status === "pending" && "签名并广播中…"}
      {tx.status === "ok" && "交易已上链"}
      {tx.status === "error" && (tx.error ?? "交易失败")}
    </p>
  );
}
