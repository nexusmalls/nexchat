// EN: Product image from on-chain images_cid — fetch + blob URL with gateway failover.
// CN: 链上 images_cid 商品图——fetch + blob URL，多网关 failover。

import { useEffect, useState } from "react";
import { loadProductImageBlob } from "@/shop/ipfsMeta";

export interface ProductImageProps {
  cid: string | null | undefined;
  alt?: string;
  className?: string;
  placeholderClassName?: string;
  placeholder?: string;
}

export function ProductImage({
  cid,
  alt = "",
  className = "",
  placeholderClassName = "wx-shop-product-img-ph",
  placeholder = "📦",
}: ProductImageProps) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setSrc(null);
    setFailed(false);

    if (!cid?.trim()) {
      setFailed(true);
      return;
    }

    void loadProductImageBlob(cid).then((blobUrl) => {
      if (!alive) return;
      if (blobUrl) {
        setSrc(blobUrl);
        setFailed(false);
      } else {
        setFailed(true);
      }
    });

    return () => {
      alive = false;
    };
  }, [cid]);

  if (!src || failed) {
    return (
      <span className={placeholderClassName} aria-hidden>
        {placeholder}
      </span>
    );
  }

  return <img src={src} alt={alt} className={className} loading="lazy" />;
}
