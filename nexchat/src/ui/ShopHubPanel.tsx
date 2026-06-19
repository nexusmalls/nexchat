import { useEffect, useMemo, useState } from "react";
import { config } from "@/config";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useOwnedEntities } from "@/hooks/useOwnedEntities";
import { useWallet } from "@/hooks/useWallet";
import { useTranslations } from "@/i18n";
import {
  filterProductsByCategory,
  filterProductsBySearch,
  filterShopsBySearch,
} from "@/shop/filter";
import { fetchIpfsTextBatch, filterProductsWithThumbnail } from "@/shop/ipfsMeta";
import { filterCatalogByEntity } from "@/shop/sort";
import type { ProductCategory, EntityProduct } from "@/shop/types";
import { useUiStore } from "@/state/uiStore";
import { ShopCard } from "@/ui/ShopCard";
import { ShopProductCard } from "@/ui/ShopProductCard";

const PAGE = 20;

const CATEGORIES: { id: ProductCategory | "all"; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "Physical", label: "实物" },
  { id: "Digital", label: "数字" },
  { id: "Service", label: "服务" },
];

// EN: Shopping hub — shops tab respects entity selection; hot tab always lists all on-chain products by sales.
// CN: 购物首页——商铺 Tab 随实体筛选；热销 Tab 始终展示链上全部在售商品（按销量）。
export function ShopHubPanel() {
  const t = useTranslations("shopHub");
  const setDiscoverView = useUiStore((s) => s.setDiscoverView);
  const currentEntityId = useUiStore((s) => s.currentEntityId);
  const openShopDetail = useUiStore((s) => s.openShopDetail);
  const openProductDetail = useUiStore((s) => s.openProductDetail);
  const openShopOrders = useUiStore((s) => s.openShopOrders);
  const openRegisterShop = useUiStore((s) => s.openRegisterShop);
  const { address } = useWallet();
  const { entities: ownedEntities } = useOwnedEntities(address, !!address && !config.useMock);
  const { catalog, loading, error, refresh } = useEntityShop(true, address);

  const [tab, setTab] = useState<"hot" | "shops">(() =>
    currentEntityId == null ? "hot" : "shops",
  );
  const [hotLimit, setHotLimit] = useState(PAGE);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState<ProductCategory | "all">("all");
  const [nameMap, setNameMap] = useState<Map<string, string>>(new Map());
  const [hotProducts, setHotProducts] = useState<EntityProduct[]>([]);
  const [thumbFilterReady, setThumbFilterReady] = useState(false);

  const scoped = currentEntityId != null;

  useEffect(() => {
    if (currentEntityId == null) {
      setTab("hot");
      setHotLimit(PAGE);
    }
  }, [currentEntityId]);

  const displayCatalog = useMemo(() => {
    if (!catalog) return null;
    if (currentEntityId == null) return catalog;
    return filterCatalogByEntity(catalog, currentEntityId);
  }, [catalog, currentEntityId]);

  const hotCandidates = useMemo(() => {
    if (!catalog) return [];
    const byCat = filterProductsByCategory(catalog.allOnSaleProducts, category);
    return filterProductsBySearch(byCat, nameMap, search);
  }, [catalog, category, search, nameMap]);

  useEffect(() => {
    let cancelled = false;
    setThumbFilterReady(false);
    if (hotCandidates.length === 0) {
      setHotProducts([]);
      setThumbFilterReady(true);
      return;
    }
    void filterProductsWithThumbnail(hotCandidates).then((filtered) => {
      if (cancelled) return;
      setHotProducts(filtered);
      setThumbFilterReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, [hotCandidates]);

  const shops = useMemo(() => {
    const base = displayCatalog?.shops ?? [];
    return filterShopsBySearch(base, search);
  }, [displayCatalog, search]);

  const visibleHot = hotProducts.slice(0, hotLimit);
  const primaryOwned = ownedEntities[0];
  const showRegisterBanner = !!address && !config.useMock && ownedEntities.length === 0;

  useEffect(() => {
    if (!catalog?.allOnSaleProducts.length) return;
    const cids = catalog.allOnSaleProducts.map((p) => p.nameCid).filter((c) => c.length > 0);
    void fetchIpfsTextBatch(cids).then(setNameMap);
  }, [catalog]);

  const emptyShopMsg = search.trim()
    ? "无匹配商铺"
    : scoped
      ? "该实体暂无营业商铺"
      : "暂无营业商铺";
  const emptyProductMsg = search.trim()
    ? "无匹配商品"
    : thumbFilterReady && hotCandidates.length > 0 && hotProducts.length === 0
      ? "暂无有缩略图的在售商品"
      : "暂无在售商品";

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={() => setDiscoverView("list")}>
          ‹ 返回
        </button>
        <span>购物</span>
        <button type="button" className="wx-market-refresh" onClick={() => void refresh()} disabled={loading}>
          {loading ? "…" : "刷新"}
        </button>
      </header>

      <div className="wx-market-scroll">
        {config.useMock && (
          <div className="wx-market-banner">
            Mock 模式无法读取链上商铺。请设置 <code>VITE_USE_MOCK=false</code> 并连接节点。
          </div>
        )}
        {error && <div className="wx-market-banner wx-market-banner-err">{error}</div>}

        {showRegisterBanner && (
          <div className="wx-open-shop-banner">
            <p>{t("registerBanner")}</p>
            <button type="button" className="wx-earnings-entity-switch-btn" onClick={() => openRegisterShop()}>
              {t("registerCta")}
            </button>
          </div>
        )}

        {primaryOwned && primaryOwned.primaryShopId > 0 && (
          <div className="wx-open-shop-banner wx-open-shop-banner-owned">
            <p>{t("myShopBanner", { name: primaryOwned.name })}</p>
            <button
              type="button"
              className="wx-earnings-entity-switch-btn"
              onClick={() => openShopDetail(primaryOwned.primaryShopId)}
            >
              {t("myShopCta")}
            </button>
          </div>
        )}

        <div className="wx-shop-hub-toolbar">
          <input
            className="wx-shop-search"
            type="search"
            placeholder="搜索商品或商铺"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {address && (
            <button type="button" className="wx-shop-orders-btn" onClick={() => openShopOrders()}>
              我的订单
            </button>
          )}
        </div>

        <div className="wx-market-tabs">
          <button
            type="button"
            className={`wx-market-tab${tab === "shops" ? " active" : ""}`}
            onClick={() => setTab("shops")}
          >
            商铺 {shops.length > 0 ? `(${shops.length})` : ""}
          </button>
          <button
            type="button"
            className={`wx-market-tab${tab === "hot" ? " active" : ""}`}
            onClick={() => setTab("hot")}
          >
            热销商品
          </button>
        </div>

        {tab === "shops" && (
          <div className="wx-shop-list">
            {loading && shops.length === 0 ? (
              <p className="wx-market-empty">加载中…</p>
            ) : shops.length === 0 ? (
              <div className="wx-market-empty wx-open-shop-empty">
                <p>{emptyShopMsg}</p>
                {showRegisterBanner && (
                  <button type="button" className="wx-market-submit buy" onClick={() => openRegisterShop()}>
                    {t("registerCta")}
                  </button>
                )}
              </div>
            ) : (
              shops.map((s) => (
                <ShopCard key={s.id} shop={s} onClick={() => openShopDetail(s.id)} />
              ))
            )}
          </div>
        )}

        {tab === "hot" && (
          <>
            <div className="wx-shop-category-row">
              {CATEGORIES.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className={`wx-shop-category-chip${category === c.id ? " active" : ""}`}
                  onClick={() => setCategory(c.id)}
                >
                  {c.label}
                </button>
              ))}
            </div>

            {(loading || !thumbFilterReady) && hotProducts.length === 0 ? (
              <p className="wx-market-empty">加载中…</p>
            ) : hotProducts.length === 0 ? (
              <p className="wx-market-empty">{emptyProductMsg}</p>
            ) : (
              <div className="wx-shop-product-grid">
                {visibleHot.map((p) => (
                  <ShopProductCard
                    key={p.id}
                    product={p}
                    name={nameMap.get(p.nameCid)}
                    onClick={() => openProductDetail(p.id, p.shopId)}
                  />
                ))}
              </div>
            )}
            {hotLimit < hotProducts.length && (
              <button
                type="button"
                className="wx-shop-load-more"
                onClick={() => setHotLimit((n) => n + PAGE)}
              >
                加载更多
              </button>
            )}
          </>
        )}

        <p className="wx-market-foot">链上 Entity 商城 · 下单 · 订单履约</p>
      </div>
    </main>
  );
}
