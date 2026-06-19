import { useEffect, useState } from "react";
import { config } from "@/config";
import { useEntityOrder } from "@/hooks/useEntityOrder";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useMarketTx } from "@/hooks/useMarketTx";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useWallet } from "@/hooks/useWallet";
import {
  approveRefund,
  cancelOrder,
  confirmReceipt,
  confirmService,
  rejectRefund,
  requestRefund,
  sellerCancelOrder,
  shipOrder,
  startService,
  completeService,
  withdrawDispute,
} from "@/shop/entityOrderTx";
import { fetchIpfsText } from "@/shop/ipfsMeta";
import {
  formatOrderAmount,
  getOrderPaymentLabel,
  orderStatusLabel,
} from "@/shop/orderFormat";
import { isShopManager as checkShopManager } from "@/shop/seller";
import { formatNexBalance } from "@/shop/pricing";
import { useUiStore } from "@/state/uiStore";
import { canonicalAddress, shortAddress } from "@/wallet/address";

interface ShopOrderDetailPanelProps {
  orderId: number;
}

// EN: Order detail with buyer/seller actions (confirm receipt, ship, etc.).
// CN: 订单详情——买家/卖家操作（确认收货、发货等）。
export function ShopOrderDetailPanel({ orderId }: ShopOrderDetailPanelProps) {
  const backShop = useUiStore((s) => s.backShop);
  const openProductDetail = useUiStore((s) => s.openProductDetail);
  const { address } = useWallet();
  const { ownedEntityIds } = useOwnedEntities(address, !!address);
  const { order, loading, error, refresh } = useEntityOrder(orderId, true);
  const { catalog } = useEntityShop(true, address);

  const [productName, setProductName] = useState("");
  const [trackingCid, setTrackingCid] = useState("");
  const [reasonCid, setReasonCid] = useState("");
  const [sellerReasonCid, setSellerReasonCid] = useState("");
  const [shippingText, setShippingText] = useState("");
  const [trackingText, setTrackingText] = useState("");

  const tx = useMarketTx(() => void refresh());

  const who = address ? canonicalAddress(address) : null;
  const isBuyer = who != null && order != null && canonicalAddress(order.buyer) === who;
  const isSeller = who != null && order != null && canonicalAddress(order.seller) === who;
  const isService = order?.productCategory === "Service";
  const isPhysical = order?.productCategory === "Physical";
  const shop = order ? catalog?.shopById.get(order.shopId) : undefined;
  const managesShop = checkShopManager(shop, address, ownedEntityIds);

  const canActAsSeller = isSeller || managesShop;

  useEffect(() => {
    if (!order) return;
    const product = catalog?.products.find((p) => p.id === order.productId);
    if (product?.nameCid) {
      void fetchIpfsText(product.nameCid).then(setProductName);
    }
  }, [order, catalog]);

  useEffect(() => {
    if (!order?.shippingCid) {
      setShippingText("");
      return;
    }
    void fetchIpfsText(order.shippingCid).then(setShippingText);
  }, [order?.shippingCid]);

  useEffect(() => {
    if (!order?.trackingCid) {
      setTrackingText("");
      return;
    }
    void fetchIpfsText(order.trackingCid).then(setTrackingText);
  }, [order?.trackingCid]);

  const terminal = order != null && ["Completed", "Refunded", "Cancelled"].includes(order.status);

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ 返回
        </button>
        <span>订单 #{orderId}</span>
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
            Mock 模式无法读取链上订单。请设置 <code>VITE_USE_MOCK=false</code>。
          </div>
        )}
        {loading && !order && <p className="wx-market-empty">加载中…</p>}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}
        {!loading && !order && !error && (
          <p className="wx-market-empty">订单不存在</p>
        )}

        {order && (
          <>
            <section className="wx-market-card">
              <div className="wx-shop-order-detail-status">
                <span className={`wx-shop-order-status status-${order.status}`}>
                  {orderStatusLabel(order.status)}
                </span>
                {order.disputeDeadline != null && (
                  <span className="wx-shop-order-sub">
                    争议截止区块 {order.disputeDeadline}
                  </span>
                )}
              </div>
            </section>

            <section className="wx-market-card">
              <h3 className="wx-market-section-title">订单信息</h3>
              <div className="wx-shop-order-row">
                <span>商品</span>
                <button
                  type="button"
                  className="wx-shop-detail-shop-link"
                  onClick={() => openProductDetail(order.productId, order.shopId)}
                >
                  {productName.trim() || `#${order.productId}`}
                </button>
              </div>
              <div className="wx-shop-order-row">
                <span>商铺</span>
                <span>#{order.shopId}</span>
              </div>
              <div className="wx-shop-order-row">
                <span>数量</span>
                <span>{order.quantity}</span>
              </div>
              <div className="wx-shop-order-row">
                <span>单价</span>
                <span>{formatNexBalance(order.unitPrice)} NEX</span>
              </div>
              <div className="wx-shop-order-row">
                <span>合计</span>
                <span>
                  {formatOrderAmount(order)}
                  <span className="wx-shop-order-sub">
                    {" "}
                    ({getOrderPaymentLabel(order)})
                  </span>
                </span>
              </div>
              <div className="wx-shop-order-row">
                <span>买家</span>
                <span>{shortAddress(order.buyer)}</span>
              </div>
              <div className="wx-shop-order-row">
                <span>卖家</span>
                <span>{shortAddress(order.seller)}</span>
              </div>
            </section>

            {isPhysical && shippingText && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">收货地址</h3>
                <p className="wx-shop-detail-text">{shippingText}</p>
              </section>
            )}

            {trackingText && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">物流信息</h3>
                <p className="wx-shop-detail-text">{trackingText}</p>
              </section>
            )}

            {!terminal && address && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">操作</h3>

                {order.status === "Paid" && isBuyer && (
                  <button
                    type="button"
                    className="wx-market-submit sell"
                    disabled={tx.busy}
                    onClick={() => void tx.run(() => cancelOrder(orderId))}
                  >
                    {tx.busy ? "提交中…" : "取消订单"}
                  </button>
                )}

                {order.status === "Paid" && canActAsSeller && isPhysical && (
                  <div className="wx-shop-order-actions">
                    <label className="wx-market-field">
                      <span>物流信息 / IPFS CID</span>
                      <input
                        placeholder="填写运单号或 IPFS CID"
                        value={trackingCid}
                        onChange={(e) => setTrackingCid(e.target.value)}
                      />
                    </label>
                    <button
                      type="button"
                      className="wx-market-submit buy"
                      disabled={!trackingCid.trim() || tx.busy}
                      onClick={() =>
                        void tx.run(() => shipOrder(orderId, trackingCid.trim()))
                      }
                    >
                      {tx.busy ? "提交中…" : "确认发货"}
                    </button>
                    <label className="wx-market-field">
                      <span>取消原因 / IPFS CID</span>
                      <input
                        placeholder="卖家取消订单时填写"
                        value={sellerReasonCid}
                        onChange={(e) => setSellerReasonCid(e.target.value)}
                      />
                    </label>
                    <button
                      type="button"
                      className="wx-market-submit sell"
                      disabled={!sellerReasonCid.trim() || tx.busy}
                      onClick={() =>
                        void tx.run(() =>
                          sellerCancelOrder(orderId, sellerReasonCid.trim()),
                        )
                      }
                    >
                      {tx.busy ? "提交中…" : "卖家取消订单"}
                    </button>
                  </div>
                )}

                {order.status === "Paid" && canActAsSeller && isService && (
                  <button
                    type="button"
                    className="wx-market-submit buy"
                    disabled={tx.busy}
                    onClick={() => void tx.run(() => startService(orderId))}
                  >
                    {tx.busy ? "提交中…" : "开始服务"}
                  </button>
                )}

                {order.status === "Shipped" && isBuyer && !isService && (
                  <div className="wx-shop-order-actions">
                    <button
                      type="button"
                      className="wx-market-submit buy"
                      disabled={tx.busy}
                      onClick={() => void tx.run(() => confirmReceipt(orderId))}
                    >
                      {tx.busy ? "提交中…" : "确认收货"}
                    </button>
                    <label className="wx-market-field">
                      <span>退款原因 / IPFS CID</span>
                      <input
                        placeholder="申请退款时填写"
                        value={reasonCid}
                        onChange={(e) => setReasonCid(e.target.value)}
                      />
                    </label>
                    <button
                      type="button"
                      className="wx-market-submit sell"
                      disabled={!reasonCid.trim() || tx.busy}
                      onClick={() =>
                        void tx.run(() => requestRefund(orderId, reasonCid.trim()))
                      }
                    >
                      {tx.busy ? "提交中…" : "申请退款"}
                    </button>
                  </div>
                )}

                {order.status === "Shipped" && canActAsSeller && isService && (
                  <button
                    type="button"
                    className="wx-market-submit buy"
                    disabled={tx.busy}
                    onClick={() => void tx.run(() => completeService(orderId))}
                  >
                    {tx.busy ? "提交中…" : "完成服务"}
                  </button>
                )}

                {order.status === "Shipped" && isBuyer && isService && (
                  <button
                    type="button"
                    className="wx-market-submit buy"
                    disabled={tx.busy}
                    onClick={() => void tx.run(() => confirmService(orderId))}
                  >
                    {tx.busy ? "提交中…" : "确认服务完成"}
                  </button>
                )}

                {order.status === "Disputed" && canActAsSeller && (
                  <div className="wx-shop-order-actions">
                    <button
                      type="button"
                      className="wx-market-submit buy"
                      disabled={tx.busy}
                      onClick={() => void tx.run(() => approveRefund(orderId))}
                    >
                      {tx.busy ? "提交中…" : "同意退款"}
                    </button>
                    <label className="wx-market-field">
                      <span>拒绝原因 / IPFS CID</span>
                      <input
                        placeholder="拒绝退款时填写"
                        value={reasonCid}
                        onChange={(e) => setReasonCid(e.target.value)}
                      />
                    </label>
                    <button
                      type="button"
                      className="wx-market-submit sell"
                      disabled={!reasonCid.trim() || tx.busy}
                      onClick={() =>
                        void tx.run(() => rejectRefund(orderId, reasonCid.trim()))
                      }
                    >
                      {tx.busy ? "提交中…" : "拒绝退款"}
                    </button>
                  </div>
                )}

                {order.status === "Disputed" && isBuyer && (
                  <button
                    type="button"
                    className="wx-market-submit sell"
                    disabled={tx.busy}
                    onClick={() => void tx.run(() => withdrawDispute(orderId))}
                  >
                    {tx.busy ? "提交中…" : "撤回争议"}
                  </button>
                )}

                {tx.status === "ok" && (
                  <p className="wx-market-tx-status ok">交易已提交</p>
                )}
                {tx.status === "error" && (
                  <p className="wx-market-tx-status error">{tx.error ?? "操作失败"}</p>
                )}
              </section>
            )}

            {terminal && (
              <p className="wx-market-foot">订单已结束</p>
            )}
          </>
        )}
      </div>
    </main>
  );
}
