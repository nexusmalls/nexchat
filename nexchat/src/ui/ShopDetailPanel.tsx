import { useEffect, useMemo, useState } from "react";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useWallet } from "@/hooks/useWallet";
import { useTranslations } from "@/i18n";
import { useUiStore } from "@/state/uiStore";
import { formatSoldCount } from "@/shop/format";
import { fetchIpfsTextBatch } from "@/shop/ipfsMeta";
import { isShopManager } from "@/shop/seller";
import { sortProductsBySales } from "@/shop/sort";
import { ShopProductCard } from "@/ui/ShopProductCard";
import { ProductImage } from "@/ui/ProductImage";

interface ShopDetailPanelProps {
  shopId: number;
}

// EN: Single shop page — header + in-shop products by sold_count.
// CN: 商铺详情页——店头 + 店内商品（按销量）。
export function ShopDetailPanel({ shopId }: ShopDetailPanelProps) {
  const t = useTranslations("shopDetail");
  const backShop = useUiStore((s) => s.backShop);
  const openProductDetail = useUiStore((s) => s.openProductDetail);
  const openShopOrders = useUiStore((s) => s.openShopOrders);
  const openAddProduct = useUiStore((s) => s.openAddProduct);
  const { address } = useWallet();
  const { ownedEntityIds } = useOwnedEntities(address, !!address);
  const { catalog, loading, refresh } = useEntityShop(true, address);

  const shop = catalog?.shopById.get(shopId);
  const products = useMemo(() => {
    if (!catalog) return [];
    return sortProductsBySales(catalog.products.filter((p) => p.shopId === shopId));
  }, [catalog, shopId]);
  const isManager = isShopManager(shop, address, ownedEntityIds);

  const [nameMap, setNameMap] = useState<Map<string, string>>(new Map());

  useEffect(() => {
    if (products.length === 0) return;
    void fetchIpfsTextBatch(products.map((p) => p.nameCid)).then(setNameMap);
  }, [products]);

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ 返回
        </button>
        <span>{shop?.name || `店铺 #${shopId}`}</span>
        <button type="button" className="wx-market-refresh" onClick={() => void refresh()} disabled={loading}>
          {loading ? "…" : "刷新"}
        </button>
      </header>

      <div className="wx-market-scroll">
        {shop && (
          <section className="wx-shop-detail-head">
            <div className="wx-shop-detail-logo">
              <ProductImage
                cid={shop.logoCid}
                className="wx-shop-detail-logo-img"
                placeholderClassName="wx-shop-detail-logo-ph"
                placeholder="🏪"
              />
            </div>
            <div>
              <h2 className="wx-shop-detail-name">{shop.name || `店铺 #${shop.id}`}</h2>
              <p className="wx-shop-detail-meta">
                {shop.productCount} 件商品 · 订单 {formatSoldCount(shop.totalOrders)}
              </p>
              {isManager && (
                <div className="wx-shop-detail-manage-actions">
                  <button
                    type="button"
                    className="wx-market-submit buy wx-shop-add-product-btn"
                    onClick={() => openAddProduct(shop.id)}
                  >
                    {t("addProduct")}
                  </button>
                  <button
                    type="button"
                    className="wx-shop-orders-btn wx-shop-detail-orders-btn"
                    onClick={() => openShopOrders("seller", shop.id)}
                  >
                    {t("shopOrders")} ›
                  </button>
                </div>
              )}
            </div>
          </section>
        )}

        {!shop && !loading && (
          <p className="wx-market-empty">店铺不存在或未营业</p>
        )}

        <h3 className="wx-market-section-title">店内商品</h3>
        {loading && products.length === 0 ? (
          <p className="wx-market-empty">加载中…</p>
        ) : products.length === 0 ? (
          <div className="wx-shop-empty-products">
            <p className="wx-market-empty">{t("noProducts")}</p>
            {isManager && (
              <button
                type="button"
                className="wx-market-submit buy wx-open-shop-submit"
                onClick={() => openAddProduct(shopId)}
              >
                {t("addProduct")}
              </button>
            )}
          </div>
        ) : (
          <div className="wx-shop-product-grid">
            {products.map((p) => (
              <ShopProductCard
                key={p.id}
                product={p}
                name={nameMap.get(p.nameCid)}
                onClick={() => openProductDetail(p.id, shopId)}
              />
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
