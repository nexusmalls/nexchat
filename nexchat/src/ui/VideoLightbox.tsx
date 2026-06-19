// EN: Full-screen video viewer — decrypts on demand and auto-plays.
// CN: 全屏看视频——按需解密并自动播放。

import { useCallback, useEffect, useRef, useState } from "react";

export interface VideoLightboxProps {
  open: boolean;
  title?: string;
  onClose: () => void;
  /** EN: Poster/thumbnail while full video loads. CN: 视频加载前显示的缩略图。 */
  thumbUrl?: string | null;
  /** EN: Already-decoded full video URL (skip load). CN: 已有视频 URL。 */
  fullUrl?: string | null;
  /** EN: Lazy loader for the original bytes. CN: 原视频懒加载。 */
  loadFull?: () => Promise<string | null>;
}

export function VideoLightbox({
  open,
  title,
  onClose,
  thumbUrl,
  fullUrl: fullUrlProp,
  loadFull,
}: VideoLightboxProps) {
  const [fullUrl, setFullUrl] = useState<string | null>(fullUrlProp ?? null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

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
      else setError("视频加载失败");
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
    if (!open || !fullUrl) return;
    const el = videoRef.current;
    if (!el) return;
    void el.play().catch(() => {
      /* autoplay may be blocked until user gesture — click already counts */
    });
  }, [open, fullUrl]);

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
      aria-label={title ?? "播放视频"}
      onClick={onClose}
    >
      <div className="media-lightbox-panel media-lightbox-panel-video" onClick={(e) => e.stopPropagation()}>
        <header className="media-lightbox-head">
          <span className="media-lightbox-title">{title ?? "视频"}</span>
          <button type="button" className="media-lightbox-close" onClick={onClose}>
            ✕
          </button>
        </header>
        <div className="media-lightbox-body">
          {loading && <p className="media-lightbox-status">加载视频…</p>}
          {error && <p className="media-lightbox-error">{error}</p>}
          {fullUrl ? (
            <video
              ref={videoRef}
              className="media-lightbox-video"
              src={fullUrl}
              controls
              autoPlay
              playsInline
              poster={thumbUrl ?? undefined}
            />
          ) : thumbUrl ? (
            <img className="media-lightbox-img media-lightbox-img-dim" src={thumbUrl} alt={title ?? "缩略图"} />
          ) : null}
        </div>
      </div>
    </div>
  );
}
