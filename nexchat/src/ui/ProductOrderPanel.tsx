import { useEffect, useMemo, useState } from "react";
import { config } from "@/config";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useNexPrice } from "@/hooks/useNexPrice";
import { useShoppingBalance } from "@/hooks/useShoppingBalance";
import { useWallet } from "@/hooks/useWallet";
import { useUiStore } from "@/state/uiStore";
import { placeOrder } from "@/shop/entityOrderTx";
import { formatUsdtPrice } from "@/shop/format";
import { fetchIpfsText } from "@/shop/ipfsMeta";
import { computeOrderQuote, formatNexBalance } from "@/shop/pricing";
import type { PaymentAsset } from "@/shop/types";
import { canonicalAddress } from "@/wallet/address";
import { decodeAddress } from "@polkadot/util-crypto";

interface ProductOrderPanelProps {
  productId: number;
}

async function isValidSs58(address: string): Promise<boolean> {
  try {
    const { cryptoWaitReady } = await import("@polkadot/util-crypto");
    await cryptoWaitReady();
    decodeAddress(address.trim());
    return true;
  } catch {
    return false;
  }
}

// EN: Create order form — mirrors nexus-com-dapp `/order/create`.
// CN: 下单表单——对齐 nexus-com-dapp `/order/create`。
export function ProductOrderPanel({ productId }: ProductOrderPanelProps) {
  const backShop = useUiStore((s) => s.backShop);
  const openShopOrders = useUiStore((s) => s.openShopOrders);
  const { address } = useWallet();
  const { catalog, refresh: refreshCatalog } = useEntityShop(true, address);
  const { marketRate, loading: priceLoading } = useNexPrice(true);

  const product = catalog?.products.find((p) => p.id === productId);
  const shop = product ? catalog?.shopById.get(product.shopId) : undefined;

  const { balance: shoppingBalance } = useShoppingBalance(shop?.entityId, address);

  const [name, setName] = useState("");
  const [quantity, setQuantity] = useState(1);
  const [paymentAsset, setPaymentAsset] = useState<"Native" | "ShoppingBalance">("Native");
  const [shippingAddr, setShippingAddr] = useState("");
  const [referrer, setReferrer] = useState("");
  const [referrerError, setReferrerError] = useState("");

  const tx = useMarketTx(() => {
    void refreshCatalog();
    openShopOrders();
  });

  useEffect(() => {
    if (!product) return;
    void fetchIpfsText(product.nameCid).then(setName);
  }, [product]);

  useEffect(() => {
    if (!referrer.trim()) {
      setReferrerError("");
      return;
    }
    let cancelled = false;
    void isValidSs58(referrer).then((ok) => {
      if (!cancelled) setReferrerError(ok ? "" : "推荐人地址格式无效");
    });
    return () => {
      cancelled = true;
    };
  }, [referrer]);

  const minQty = Math.max(product?.minOrderQuantity || 1, 1);
  const maxQty = product?.maxOrderQuantity || 999;
  const stockLimit =
    product?.stock === 0 ? maxQty : Math.min(product?.stock ?? maxQty, maxQty);

  useEffect(() => {
    setQuantity((q) => Math.min(Math.max(q, minQty), stockLimit));
  }, [minQty, stockLimit]);

  const isPhysical = product?.category === "Physical";
  const who = address ? canonicalAddress(address) : null;
  const isOwnProduct =
    !!who && !!shop?.managers?.some((m) => canonicalAddress(m) === who);

  const member = shop ? catalog?.memberByEntity.get(shop.entityId) : undefined;
  const isMembersOnly = product?.visibility === "MembersOnly";
  const isLevelGated = product?.visibility === "LevelGated";
  const needsAutoRegister = (isMembersOnly || isLevelGated) && !member?.isMember;
  const levelInsufficient =
    isLevelGated &&
    !!member?.isMember &&
    member.level < (product?.visibilityMinLevel ?? 0);

  const quote = useMemo(() => {
    if (!product) return null;
    return computeOrderQuote({
      product,
      quantity,
      marketRate,
      paymentAsset,
      shoppingBalanceRaw: shoppingBalance,
    });
  }, [product, quantity, marketRate, paymentAsset, shoppingBalance]);

  const canUseShoppingBalance = BigInt(shoppingBalance || "0") > 0n;

  const quantityValid = Number.isSafeInteger(quantity) && quantity >= minQty && quantity <= stockLimit;
  const canSubmit =
    !!product &&
    !!address &&
    !config.useMock &&
    quantityValid &&
    !isOwnProduct &&
    !levelInsufficient &&
    !tx.busy &&
    tx.status !== "ok" &&
    !!quote?.priceReady &&
    !referrerError;

  const handleSubmit = () => {
    if (!product || !canSubmit || !quote) return;
    const shippingCid =
      isPhysical && shippingAddr.trim() ? shippingAddr.trim() : null;
    const payAsset: PaymentAsset | null =
      paymentAsset === "ShoppingBalance" && quote.shoppingBalSpend
        ? "ShoppingBalance"
        : paymentAsset === "Native"
          ? null
          : paymentAsset;

    void tx.run(() =>
      placeOrder({
        productId: product.id,
        quantity,
        shippingCid,
        paymentAsset: payAsset,
        referrer: referrer.trim() || null,
        maxNexAmount: quote.maxNexAmount,
      }),
    );
  };

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ 返回
        </button>
        <span>确认订单</span>
        <span className="wx-market-refresh" />
      </header>

      <div className="wx-market-scroll">
        {!product && (
          <p className="wx-market-empty">商品不存在或已下架</p>
        )}

        {product && (
          <>
            <section className="wx-market-card">
              <h3 className="wx-market-section-title">商品信息</h3>
              <div className="wx-shop-order-row">
                <span>商品</span>
                <span>{name.trim() || `#${product.id}`}</span>
              </div>
              <div className="wx-shop-order-row">
                <span>单价</span>
                <span>
                  {quote?.hasUsdtPrice ? (
                    <>
                      ${formatUsdtPrice(product.usdtPrice)}
                      {quote.unitNexDynamic && (
                        <span className="wx-shop-order-sub">
                          {" "}
                          ≈ {formatNexBalance(quote.unitNexDynamic)} NEX
                        </span>
                      )}
                    </>
                  ) : (
                    <>{formatNexBalance(product.price)} NEX</>
                  )}
                </span>
              </div>
              <label className="wx-market-field">
                <span>数量 ({minQty}–{stockLimit})</span>
                <input
                  inputMode="numeric"
                  type="number"
                  min={minQty}
                  max={stockLimit}
                  value={quantity}
                  onChange={(e) => setQuantity(Number(e.target.value) || minQty)}
                />
              </label>
              {quote && (
                <div className="wx-market-estimate">
                  合计：
                  {quote.hasUsdtPrice && quote.totalUsdt != null ? (
                    <strong> ${formatUsdtPrice(quote.totalUsdt)} USDT</strong>
                  ) : null}
                  {quote.priceReady ? (
                    <span className="wx-shop-order-sub">
                      {" "}
                      ≈ {formatNexBalance(quote.totalNex.toString())} NEX
                    </span>
                  ) : (
                    <span className="wx-shop-order-sub"> 行情加载中…</span>
                  )}
                </div>
              )}
            </section>

            <section className="wx-market-card">
              <h3 className="wx-market-section-title">支付方式</h3>
              <div className="wx-market-side-toggle">
                <button
                  type="button"
                  className={`wx-market-side-btn buy${paymentAsset === "Native" ? " active" : ""}`}
                  onClick={() => setPaymentAsset("Native")}
                >
                  NEX 支付
                </button>
                <button
                  type="button"
                  className={`wx-market-side-btn sell${paymentAsset === "ShoppingBalance" ? " active" : ""}`}
                  disabled={!canUseShoppingBalance}
                  onClick={() => setPaymentAsset("ShoppingBalance")}
                >
                  购物余额
                </button>
              </div>
              {paymentAsset === "ShoppingBalance" && (
                <p className="wx-market-field-hint">
                  可用余额 {formatNexBalance(shoppingBalance)} NEX
                  {quote?.shoppingBalSpend
                    ? `，本次抵扣 ${formatNexBalance(quote.shoppingBalSpend)}`
                    : ""}
                </p>
              )}
              {priceLoading && quote?.hasUsdtPrice && !quote.priceReady && (
                <p className="wx-market-field-hint">正在获取 NEX/USDT 汇率…</p>
              )}
            </section>

            {isPhysical && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">收货地址</h3>
                <label className="wx-market-field">
                  <textarea
                    className="wx-shop-order-textarea"
                    placeholder="填写收货地址或 IPFS CID"
                    value={shippingAddr}
                    maxLength={500}
                    rows={3}
                    onChange={(e) => setShippingAddr(e.target.value)}
                  />
                </label>
              </section>
            )}

            <section className="wx-market-card">
              <h3 className="wx-market-section-title">推荐人（可选）</h3>
              <label className="wx-market-field">
                <input
                  placeholder="SS58 地址"
                  value={referrer}
                  onChange={(e) => setReferrer(e.target.value)}
                />
              </label>
              {referrerError && (
                <p className="wx-market-field-err">{referrerError}</p>
              )}
            </section>

            {!address && (
              <p className="wx-market-form-hint">请先解锁钱包后再下单</p>
            )}
            {isOwnProduct && (
              <p className="wx-market-banner wx-market-banner-err">不能购买自己店铺的商品</p>
            )}
            {needsAutoRegister && (
              <p className="wx-market-banner">
                会员专享商品：下单时将尝试自动注册为该 Entity 会员（需填写推荐人或由链上关系推断）。
              </p>
            )}
            {levelInsufficient && (
              <p className="wx-market-banner wx-market-banner-err">
                等级不足：需要 Lv{product?.visibilityMinLevel} 及以上（当前 Lv{member?.level ?? 0}）
              </p>
            )}

            <button
              type="button"
              className="wx-market-submit buy"
              disabled={!canSubmit}
              onClick={handleSubmit}
            >
              {tx.busy ? "提交中…" : !address ? "请先连接钱包" : "确认下单"}
            </button>

            {tx.status === "pending" && (
              <p className="wx-market-tx-status pending">签名并广播中…</p>
            )}
            {tx.status === "ok" && (
              <p className="wx-market-tx-status ok">下单成功，订单已创建</p>
            )}
            {tx.status === "error" && (
              <p className="wx-market-tx-status error">{tx.error ?? "下单失败"}</p>
            )}
          </>
        )}
      </div>
    </main>
  );
}
