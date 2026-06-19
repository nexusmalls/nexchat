import { formatSoldCount } from "@/shop/format";
import type { EntityShop } from "@/shop/types";
import { ProductImage } from "@/ui/ProductImage";

interface ShopCardProps {
  shop: EntityShop;
  onClick: () => void;
}

// EN: Shop list row/card for discover shopping.
// CN: 发现购物商铺列表卡片。
export function ShopCard({ shop, onClick }: ShopCardProps) {
  return (
    <button type="button" className="wx-shop-card" onClick={onClick}>
      <div className="wx-shop-card-logo">
        <ProductImage cid={shop.logoCid} className="wx-shop-card-logo-img" placeholder="🏪" />
      </div>
      <div className="wx-shop-card-body">
        <span className="wx-shop-card-name">{shop.name || `店铺 #${shop.id}`}</span>
        <span className="wx-shop-card-meta">
          {shop.productCount} 件商品 · 订单 {formatSoldCount(shop.totalOrders)}
        </span>
      </div>
      <span className="wx-cell-chevron">›</span>
    </button>
  );
}
