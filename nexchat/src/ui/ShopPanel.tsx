import { useUiStore } from "@/state/uiStore";
import { AddProductPanel } from "@/ui/AddProductPanel";
import { ProductDetailPanel } from "@/ui/ProductDetailPanel";
import { ProductOrderPanel } from "@/ui/ProductOrderPanel";
import { ShopDetailPanel } from "@/ui/ShopDetailPanel";
import { ShopHubPanel } from "@/ui/ShopHubPanel";
import { ShopMyOrdersPanel } from "@/ui/ShopMyOrdersPanel";
import { ShopOrderDetailPanel } from "@/ui/ShopOrderDetailPanel";

// EN: Discover shopping router — hub / shop / product / orders.
// CN: 发现购物子路由——首页 / 商铺 / 商品 / 订单。
export function ShopPanel() {
  const shopNav = useUiStore((s) => s.shopNav);

  if (shopNav.screen === "orderDetail" && shopNav.orderId != null) {
    return <ShopOrderDetailPanel orderId={shopNav.orderId} />;
  }
  if (shopNav.screen === "orders") {
    return <ShopMyOrdersPanel />;
  }
  if (shopNav.screen === "order" && shopNav.productId != null) {
    return <ProductOrderPanel productId={shopNav.productId} />;
  }
  if (shopNav.screen === "product" && shopNav.productId != null) {
    return <ProductDetailPanel productId={shopNav.productId} />;
  }
  if (shopNav.screen === "shop" && shopNav.shopId != null) {
    return <ShopDetailPanel shopId={shopNav.shopId} />;
  }
  if (shopNav.screen === "addProduct" && shopNav.shopId != null) {
    return <AddProductPanel shopId={shopNav.shopId} />;
  }
  return <ShopHubPanel />;
}
