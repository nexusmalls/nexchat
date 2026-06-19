import { formatSoldCount, formatUsdtPrice } from "@/shop/format";
import type { EntityProduct } from "@/shop/types";
import { visibilityLabel } from "@/shop/visibility";
import { ProductImage } from "@/ui/ProductImage";

interface ShopProductCardProps {
  product: EntityProduct;
  name?: string;
  onClick: () => void;
}

// EN: WeChat-style product card for shop grids.
// CN: 购物网格微信风格商品卡。
export function ShopProductCard({ product, name, onClick }: ShopProductCardProps) {
  const displayName = name?.trim() || `商品 #${product.id}`;

  return (
    <button type="button" className="wx-shop-product-card" onClick={onClick}>
      <div className="wx-shop-product-img-wrap">
        <ProductImage
          cid={product.imagesCid}
          alt={displayName}
          className="wx-shop-product-img"
        />
      </div>
      <p className="wx-shop-product-name">
        {displayName}
        {visibilityLabel(product) && (
          <span className="wx-shop-visibility-tag"> {visibilityLabel(product)}</span>
        )}
      </p>
      {product.usdtPrice > 0 && (
        <p className="wx-shop-product-price">${formatUsdtPrice(product.usdtPrice)}</p>
      )}
      <p className="wx-shop-product-sold">已售 {formatSoldCount(product.soldCount)}</p>
    </button>
  );
}
