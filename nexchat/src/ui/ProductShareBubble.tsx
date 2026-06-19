import { parseProductShare } from "@/shop/shareCard";
import { useEntityShop } from "@/hooks/useEntityShop";
import { useWallet } from "@/hooks/useWallet";
import { useUiStore } from "@/state/uiStore";
import { ProductImage } from "@/ui/ProductImage";

interface ProductShareBubbleProps {
  text: string;
}

// EN: Render product share deep-link as a tappable card in chat.
// CN: 将商品分享深链接渲染为可点击聊天卡片。
export function ProductShareBubble({ text }: ProductShareBubbleProps) {
  const payload = parseProductShare(text);
  const openShop = useUiStore((s) => s.openShop);
  const openProductDetail = useUiStore((s) => s.openProductDetail);
  const { address } = useWallet();
  const { catalog } = useEntityShop(true, address);

  if (!payload) return null;

  const product = catalog?.products.find((p) => p.id === payload.productId);
  const label = payload.label || `商品 #${payload.productId}`;

  return (
    <button
      type="button"
      className="wx-chat-product-card"
      onClick={() => {
        openShop();
        openProductDetail(payload.productId, payload.shopId);
      }}
    >
      <span className="wx-chat-product-card-thumb">
        <ProductImage
          cid={product?.imagesCid}
          alt={label}
          className="wx-chat-product-card-img"
          placeholderClassName="wx-chat-product-card-icon"
        />
      </span>
      <span className="wx-chat-product-card-body">
        <span className="wx-chat-product-card-title">{label}</span>
        <span className="wx-chat-product-card-meta">
          商铺 #{payload.shopId} · 点击查看
        </span>
      </span>
      <span className="wx-chat-product-card-chevron">›</span>
    </button>
  );
}
