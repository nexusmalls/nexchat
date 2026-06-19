// EN: Full-screen image viewer — loads decrypted original on demand.
// CN: 全屏看图——按需解密加载原图。

import { useCallback, useEffect, useState } from "react";

export interface ImageLightboxProps {
  open: boolean;
  title?: string;
  onClose: () => void;
  /** EN: Thumbnail shown while full image loads. CN: 原图加载前显示的缩略图。 */
  thumbUrl?: string | null;
  /** EN: Already-decoded full image URL (skip load). CN: 已有原图 URL。 */
  fullUrl?: string | null;
  /** EN: Lazy loader for the original bytes. CN: 原图懒加载。 */
  loadFull?: () => Promise<string | null>;
}

export function ImageLightbox({
  open,
  title,
  onClose,
  thumbUrl,
  fullUrl: fullUrlProp,
  loadFull,
}: ImageLightboxProps) {
  const [fullUrl, setFullUrl] = useState<string | null>(fullUrlProp ?? null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    if (fullUrlProp) {
      setFullUrl(fullUrlProp);
      return;
    }
    setFullUrl(null);
  }, [open, fullUrlProp]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const ensureFull = useCallback(async () => {
    if (fullUrl || loading) return;
    if (fullUrlProp) {
      setFullUrl(fullUrlProp);
      return;
    }
    if (!loadFull) return;
    setLoading(true);
    setError(null);
    try {
      const url = await loadFull();
      if (url) setFullUrl(url);
      else setError("原图加载失败");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [fullUrl, fullUrlProp, loadFull, loading]);

  useEffect(() => {
    if (open) void ensureFull();
  }, [open, ensureFull]);

  useEffect(() => {
    return () => {
      if (fullUrl && fullUrl !== fullUrlProp && fullUrl.startsWith("blob:")) {
        URL.revokeObjectURL(fullUrl);
      }
    };
  }, [fullUrl, fullUrlProp]);

  if (!open) return null;

  return (
    <div
      className="media-lightbox-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={title ?? "查看原图"}
      onClick={onClose}
    >
      <div className="media-lightbox-panel" onClick={(e) => e.stopPropagation()}>
        <header className="media-lightbox-head">
          <span className="media-lightbox-title">{title ?? "原图"}</span>
          <button type="button" className="media-lightbox-close" onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="media-lightbox-body">
          {loading && <p className="media-lightbox-status">加载原图…</p>}
          {error && <p className="media-lightbox-error">{error}</p>}
          {fullUrl ? (
            <img className="media-lightbox-img" src={fullUrl} alt={title ?? "原图"} />
          ) : thumbUrl ? (
            <img className="media-lightbox-img media-lightbox-img-dim" src={thumbUrl} alt={title ?? "缩略图"} />
          ) : null}
        </div>
      </div>
    </div>
  );
}
