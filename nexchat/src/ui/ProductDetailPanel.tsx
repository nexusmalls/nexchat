import { useEffect, useState } from "react";
import { config } from "@/config";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useWallet } from "@/hooks/useWallet";
import { useUiStore } from "@/state/uiStore";
import { formatSoldCount, formatUsdtPrice } from "@/shop/format";
import { visibilityLabel } from "@/shop/visibility";
import { fetchIpfsText } from "@/shop/ipfsMeta";
import { ProductImage } from "@/ui/ProductImage";
interface ProductDetailPanelProps {
  productId: number;
}

// EN: Product detail — images, IPFS name/detail, price, sold count (Phase 1 browse).
// CN: 商品详情——图片、IPFS 名称/详情、价格、销量（Phase 1 浏览）。
export function ProductDetailPanel({ productId }: ProductDetailPanelProps) {
  const backShop = useUiStore((s) => s.backShop);
  const openShopDetail = useUiStore((s) => s.openShopDetail);
  const openProductOrder = useUiStore((s) => s.openProductOrder);
  const openShareProductPicker = useUiStore((s) => s.openShareProductPicker);
  const { address } = useWallet();
  const { catalog, loading } = useEntityShop(true, address);

  const product = catalog?.products.find((p) => p.id === productId);
  const shop = product ? catalog?.shopById.get(product.shopId) : undefined;

  const [name, setName] = useState("");
  const [detail, setDetail] = useState("");

  useEffect(() => {
    if (!product) return;
    void fetchIpfsText(product.nameCid).then(setName);
    if (product.detailCid) {
      void fetchIpfsText(product.detailCid).then(setDetail);
    } else {
      setDetail("");
    }
  }, [product]);

  const dappBase = config.shopDappUrl?.replace(/\/$/, "");

  return (
    <main className="tg-main wx-shop-main">
      <header className="tg-sub-head wx-market-head">
        <button type="button" className="tg-sub-back wx-nav-back" onClick={backShop}>
          ‹ 返回
        </button>
        <span>商品详情</span>
        <span className="wx-market-refresh" />
      </header>

      <div className="wx-market-scroll">
        {loading && !product && (
          <p className="wx-market-empty">加载中…</p>
        )}
        {!loading && !product && (
          <p className="wx-market-empty">商品不存在或已下架</p>
        )}

        {product && (
          <>
            <div className="wx-shop-detail-gallery">
              <ProductImage
                cid={product.imagesCid}
                alt={name.trim() || `商品 #${product.id}`}
                className="wx-shop-detail-gallery-img"
                placeholderClassName="wx-shop-detail-gallery-ph"
              />
            </div>

            <section className="wx-market-card">
              <h2 className="wx-shop-detail-title">{name.trim() || `商品 #${product.id}`}</h2>
              {product.usdtPrice > 0 && (
                <p className="wx-shop-detail-price">${formatUsdtPrice(product.usdtPrice)} USDT</p>
              )}
              <p className="wx-shop-detail-sold">
                已售 {formatSoldCount(product.soldCount)} 件
                {visibilityLabel(product) && (
                  <span className="wx-shop-visibility-tag"> · {visibilityLabel(product)}</span>
                )}
              </p>
              {shop && (
                <button
                  type="button"
                  className="wx-shop-detail-shop-link"
                  onClick={() => openShopDetail(shop.id)}
                >
                  所属店铺：{shop.name || `#${shop.id}`} ›
                </button>
              )}
            </section>

            {detail.trim() && (
              <section className="wx-market-card">
                <h3 className="wx-market-section-title">商品详情</h3>
                <p className="wx-shop-detail-text">{detail}</p>
              </section>
            )}

            <button
              type="button"
              className="wx-market-submit buy wx-shop-buy-btn"
              disabled={config.useMock || !address}
              onClick={() => openProductOrder(product.id, product.shopId)}
            >
              {!address ? "请先解锁钱包" : "立即购买"}
            </button>

            <button
              type="button"
              className="wx-shop-share-btn"
              disabled={config.useMock}
              onClick={() =>
                openShareProductPicker(
                  product.id,
                  product.shopId,
                  name.trim() || `商品 #${product.id}`,
                )
              }
            >
              分享到聊天
            </button>

            {dappBase && (
              <a
                className="wx-market-dapp-link"
                href={`${dappBase}/order/create?product=${product.id}&quantity=1`}
                target="_blank"
                rel="noreferrer"
              >
                在 NEXCOM 下单 →
              </a>
            )}
          </>
        )}
      </div>
    </main>
  );
}
