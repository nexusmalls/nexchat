import { useEffect, useMemo, useState } from "react";
import { config } from "@/config";
import { chainClient } from "@/chain/chainClient";
import { useBuyerOrders } from "@/hooks/useBuyerOrders";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useSellerOrders } from "@/hooks/useSellerOrders";
import { useWallet } from "@/hooks/useWallet";
import { fetchProduct } from "@/shop/entityQueries";
import { fetchIpfsTextBatch } from "@/shop/ipfsMeta";
import {
  formatOrderAmount,
  matchOrderStatusFilter,
  orderStatusLabel,
  type OrderStatusFilter,
} from "@/shop/orderFormat";
import { getManagedShopIds } from "@/shop/seller";
import type { EntityOrder } from "@/shop/types";
import { useUiStore, type ShopOrdersTab } from "@/state/uiStore";

const FILTERS: { id: OrderStatusFilter; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "active", label: "待发货" },
  { id: "shipped", label: "待收货" },
  { id: "completed", label: "已完成" },
  { id: "disputed", label: "争议" },
  { id: "closed", label: "已关闭" },
];

// EN: Buyer + seller order lists with status filters.
// CN: 买家 / 卖家订单列表（含状态筛选）。
export function ShopMyOrdersPanel() {
  const backShop = useUiStore((s) => s.backShop);
  const shopNav = useUiStore((s) => s.shopNav);
  const openShopOrders = useUiStore((s) => s.openShopOrders);
  const openShopOrderDetail = useUiStore((s) => s.openShopOrderDetail);
  const { address } = useWallet();
  const { catalog } = useEntityShop(true, address);
  const { ownedEntityIds } = useOwnedEntities(address, !!address);

  const tab = shopNav.ordersTab;
  const shopFilter = shopNav.ordersShopFilter;

  const managedShopIds = useMemo(
    () => getManagedShopIds(catalog, address, ownedEntityIds),
    [catalog, address, ownedEntityIds],
  );

  const buyer = useBuyerOrders(address, tab === "buyer");
  const seller = useSellerOrders(address, catalog, tab === "seller", ownedEntityIds);

  const active = tab === "buyer" ? buyer : seller;
  const { orders, loading, error, refresh } = active;

  const [filter, setFilter] = useState<OrderStatusFilter>("all");
  const [productNames, setProductNames] = useState<Map<number, string>>(new Map());

  const scopedOrders = useMemo(() => {
    if (tab !== "seller" || shopFilter == null) return orders;
    return orders.filter((o) => o.shopId === shopFilter);
  }, [orders, tab, shopFilter]);

  const filtered = useMemo(
    () => scopedOrders.filter((o) => matchOrderStatusFilter(o.status, filter)),
    [scopedOrders, filter],
  );

  const productIdsKey = useMemo(
    () => [...new Set(filtered.map((o) => o.productId))].sort((a, b) => a - b).join(","),
    [filtered],
  );

  useEffect(() => {
    if (config.useMock || filtered.length === 0) return;
    let cancelled = false;
    void (async () => {
      try {
        const api = (await chainClient.getApiForWallet()) as unknown as Parameters<
          typeof fetchProduct
        >[0];
        const uniqueIds = productIdsKey.split(",").map(Number).filter((n) => n > 0);
        const cidByProduct = new Map<number, string>();
        await Promise.all(
          uniqueIds.map(async (id) => {
            const p = await fetchProduct(api, id);
            if (p?.nameCid) cidByProduct.set(id, p.nameCid);
          }),
        );
        const cids = [...cidByProduct.values()];
        const nameByCid = cids.length > 0 ? await fetchIpfsTextBatch(cids) : new Map();
        const names = new Map<number, string>();
        for (const [id, cid] of cidByProduct) {
          const n = nameByCid.get(cid);
          if (n) names.set(id, n);
        }
        if (!cancelled) setProductNames(names);
      } catch {
        /* optional */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [productIdsKey, filtered.length]);

  const setTab = (next: ShopOrdersTab) => {
    openShopOrders(next, next === "seller" ? shopFilter : null);
  };

  const title =
    tab === "seller"
      ? shopFilter != null
        ? `店铺订单 #${shopFilter}`
        : "卖家订单"
      : "我的订单";

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ 返回
        </button>
        <span>{title}</span>
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
        {!address && <p className="wx-market-empty">请先解锁钱包查看订单</p>}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {address && (
          <>
            <div className="wx-market-tabs">
              <button
                type="button"
                className={`wx-market-tab${tab === "buyer" ? " active" : ""}`}
                onClick={() => setTab("buyer")}
              >
                我买到的
              </button>
              <button
                type="button"
                className={`wx-market-tab${tab === "seller" ? " active" : ""}`}
                onClick={() => setTab("seller")}
              >
                我卖出的
                {managedShopIds.length > 0 ? ` (${managedShopIds.length}店)` : ""}
              </button>
            </div>

            {tab === "seller" && managedShopIds.length === 0 && (
              <p className="wx-market-empty">你暂未管理任何商铺</p>
            )}

            {tab === "seller" && managedShopIds.length > 0 && (
              <div className="wx-shop-order-filters">
                <button
                  type="button"
                  className={`wx-shop-order-filter${shopFilter == null ? " active" : ""}`}
                  onClick={() => openShopOrders("seller", null)}
                >
                  全部店铺
                </button>
                {managedShopIds.map((id) => (
                  <button
                    key={id}
                    type="button"
                    className={`wx-shop-order-filter${shopFilter === id ? " active" : ""}`}
                    onClick={() => openShopOrders("seller", id)}
                  >
                    店 #{id}
                  </button>
                ))}
              </div>
            )}

            <div className="wx-shop-order-filters">
              {FILTERS.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  className={`wx-shop-order-filter${filter === f.id ? " active" : ""}`}
                  onClick={() => setFilter(f.id)}
                >
                  {f.label}
                </button>
              ))}
            </div>

            {loading && scopedOrders.length === 0 ? (
              <p className="wx-market-empty">加载中…</p>
            ) : filtered.length === 0 ? (
              <p className="wx-market-empty">暂无订单</p>
            ) : (
              <div className="wx-shop-order-list">
                {filtered.map((order) => (
                  <OrderCard
                    key={order.id}
                    order={order}
                    productName={productNames.get(order.productId)}
                    role={tab}
                    onClick={() => openShopOrderDetail(order.id)}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </main>
  );
}

function OrderCard({
  order,
  productName,
  role,
  onClick,
}: {
  order: EntityOrder;
  productName?: string;
  role: ShopOrdersTab;
  onClick: () => void;
}) {
  return (
    <button type="button" className="wx-shop-order-card" onClick={onClick}>
      <div className="wx-shop-order-card-head">
        <span className="wx-shop-order-card-shop">
          商铺 #{order.shopId}
          {role === "seller" && (
            <span className="wx-shop-order-sub"> · 买家 {order.buyer.slice(0, 8)}…</span>
          )}
        </span>
        <span className={`wx-shop-order-status status-${order.status}`}>
          {orderStatusLabel(order.status)}
        </span>
      </div>
      <div className="wx-shop-order-card-body">
        <span className="wx-shop-order-card-product">
          {productName?.trim() || `商品 #${order.productId}`}
        </span>
        <span className="wx-shop-order-card-qty">×{order.quantity}</span>
      </div>
      <div className="wx-shop-order-card-foot">
        <span className="wx-shop-order-card-amount">{formatOrderAmount(order)}</span>
        <span className="wx-shop-order-card-chevron">›</span>
      </div>
    </button>
  );
}
