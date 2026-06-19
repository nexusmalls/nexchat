// EN: Media bubble — thumb-first for images/videos; tap to open lightbox / auto-play video.
// CN: 媒体气泡——图片/视频先显缩略图；点击全屏看图或自动播放视频。

import { useCallback, useEffect, useRef, useState } from "react";
import { fetchDecryptedFile, fetchDecryptedThumb } from "@/ipfs/media";
import { ImageLightbox } from "@/ui/ImageLightbox";
import { VideoLightbox } from "@/ui/VideoLightbox";
import type { MessageContent } from "@/types/viewModels";

type MediaContentProps = {
  content: Extract<MessageContent, { type: "media" }>;
  /** EN: Outgoing upload in progress. CN: 发送方上传中。 */
  uploading?: boolean;
  /** EN: Fired once after the full body is fetched+decrypted (receiver media_ack hook).
   * CN: 正文成功取回并解密后触发一次（接收方 media_ack 钩子）。 */
  onBodyDownloaded?: () => void;
};

export function MediaContent({ content, uploading = false, onBodyDownloaded }: MediaContentProps) {
  const [thumbUrl, setThumbUrl] = useState<string | null>(null);
  const [fullUrl, setFullUrl] = useState<string | null>(null);
  const [loadingFull, setLoadingFull] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [imageLightboxOpen, setImageLightboxOpen] = useState(false);
  const [videoLightboxOpen, setVideoLightboxOpen] = useState(false);
  const ownedUrls = useRef<string[]>([]);

  const isImage = content.mime.startsWith("image/");
  const isVideo = content.mime.startsWith("video/");
  const isAudio = content.mime.startsWith("audio/");
  const sizeKb = (content.size / 1024).toFixed(0);
  const hasBody = !!content.rootCid && !!content.fileKey;
  const localThumb = content.localPreviewUrl ?? null;

  const trackUrl = useCallback(
    (url: string | null) => {
      if (url && url.startsWith("blob:") && url !== content.localPreviewUrl) {
        ownedUrls.current.push(url);
      }
      return url;
    },
    [content.localPreviewUrl],
  );

  useEffect(() => {
    return () => {
      for (const u of ownedUrls.current) URL.revokeObjectURL(u);
      ownedUrls.current = [];
    };
  }, []);

  // EN: Fetch encrypted thumbnail from IPFS when available. CN: 有 IPFS 缩略图时拉取解密。
  useEffect(() => {
    if (!content.thumbCid || !content.thumbKey) return;
    let alive = true;
    void (async () => {
      try {
        const plain = await fetchDecryptedThumb(content.thumbCid!, content.thumbKey!);
        if (!alive) return;
        const url = trackUrl(
          URL.createObjectURL(new Blob([plain as BlobPart], { type: "image/jpeg" })),
        );
        setThumbUrl(url);
      } catch {
        /* optional */
      }
    })();
    return () => {
      alive = false;
    };
  }, [content.thumbCid, content.thumbKey, trackUrl]);

  useEffect(() => {
    return () => {
      if (fullUrl) URL.revokeObjectURL(fullUrl);
    };
  }, [fullUrl]);

  const loadFull = useCallback(async (): Promise<string | null> => {
    if (!content.rootCid || !content.fileKey) return null;
    if (fullUrl) return fullUrl;
    setLoadingFull(true);
    setLoadError(null);
    try {
      const plain = await fetchDecryptedFile(
        content.rootCid,
        content.fileKey,
        content.chunked ?? false,
      );
      const url = trackUrl(
        URL.createObjectURL(new Blob([plain as BlobPart], { type: content.mime })),
      );
      setFullUrl(url);
      onBodyDownloaded?.();
      return url;
    } catch (e) {
      setLoadError(String(e));
      return null;
    } finally {
      setLoadingFull(false);
    }
  }, [content.rootCid, content.fileKey, content.chunked, content.mime, fullUrl, trackUrl, onBodyDownloaded]);

  // EN: Voice clips auto-load for inline playback. CN: 语音自动解密内联播放。
  useEffect(() => {
    if (!isAudio || !hasBody || fullUrl || loadingFull) return;
    void loadFull();
  }, [isAudio, hasBody, fullUrl, loadingFull, loadFull]);

  const displayThumb = thumbUrl ?? localThumb;

  if (isImage) {
    if (!displayThumb && !hasBody) {
      return (
        <span className="media-pending media-image-pending">
          <span className="media-image-pending-icon" aria-hidden>
            🖼
          </span>
          {uploading ? "上传中…" : "图片处理中…"}
        </span>
      );
    }

    return (
      <>
        <button
          type="button"
          className="media-image-preview"
          onClick={() => setImageLightboxOpen(true)}
          disabled={!displayThumb && !hasBody}
          title="查看原图"
        >
          {displayThumb ? (
            <img className="media-thumb" src={displayThumb} alt={content.name ?? "图片"} />
          ) : (
            <span className="media-image-pending-icon" aria-hidden>
              🖼
            </span>
          )}
          {uploading && <span className="media-upload-overlay">上传中…</span>}
          {!uploading && hasBody && <span className="media-image-hint">点击查看原图</span>}
        </button>
        {loadError && <span className="media-error">{loadError}</span>}
        <ImageLightbox
          open={imageLightboxOpen}
          title={content.name ?? "图片"}
          thumbUrl={displayThumb}
          fullUrl={fullUrl}
          onClose={() => setImageLightboxOpen(false)}
          loadFull={hasBody ? loadFull : undefined}
        />
      </>
    );
  }

  if (isVideo) {
    if (!displayThumb && !hasBody) {
      return (
        <span className="media-pending media-image-pending">
          <span className="media-image-pending-icon" aria-hidden>
            🎬
          </span>
          {uploading ? "上传中…" : "视频处理中…"}
        </span>
      );
    }

    return (
      <>
        <button
          type="button"
          className="media-image-preview media-video-preview"
          onClick={() => setVideoLightboxOpen(true)}
          disabled={!displayThumb && !hasBody}
          title="播放视频"
        >
          {displayThumb ? (
            <>
              <img className="media-thumb" src={displayThumb} alt={content.name ?? "视频"} />
              <span className="media-video-play" aria-hidden>
                ▶
              </span>
            </>
          ) : (
            <span className="media-image-pending-icon" aria-hidden>
              🎬
            </span>
          )}
          {uploading && <span className="media-upload-overlay">上传中…</span>}
          {!uploading && hasBody && <span className="media-image-hint">点击播放</span>}
        </button>
        {loadError && <span className="media-error">{loadError}</span>}
        <VideoLightbox
          open={videoLightboxOpen}
          title={content.name ?? "视频"}
          thumbUrl={displayThumb}
          fullUrl={fullUrl}
          onClose={() => setVideoLightboxOpen(false)}
          loadFull={hasBody ? loadFull : undefined}
        />
      </>
    );
  }

  if (!hasBody) {
    return (
      <span className="media-pending">
        [{content.mime} {sizeKb}KB · {uploading ? "上传中…" : "处理中…"}]
      </span>
    );
  }

  if (loadError) return <span className="media-error">附件加载失败</span>;

  if (fullUrl && isAudio) {
    return (
      <div className="voice-message-wrap">
        <audio className="voice-audio-player" controls preload="metadata" src={fullUrl}>
          语音消息
        </audio>
        <span className="voice-message-meta">{sizeKb} KB</span>
      </div>
    );
  }

  if (isAudio) {
    return (
      <button className="voice-message-loading" onClick={() => void loadFull()} disabled={loadingFull}>
        {loadingFull ? "解密语音…" : `🎙 语音 (${sizeKb} KB)`}
      </button>
    );
  }

  if (displayThumb) {
    return (
      <div className="media-wrap">
        <img className="media-thumb" src={displayThumb} alt="" />
        <button className="media-load-full" onClick={() => void loadFull()} disabled={loadingFull}>
          {loadingFull ? "下载中…" : `下载 ${content.name ?? content.mime} (${sizeKb} KB)`}
        </button>
        {fullUrl && (
          <a className="media-dl" href={fullUrl} download={content.name ?? "file"}>
            保存文件
          </a>
        )}
      </div>
    );
  }

  if (fullUrl) {
    return (
      <a className="media-dl" href={fullUrl} download={content.name ?? "file"}>
        📎 {content.name ?? content.mime} ({sizeKb} KB)
      </a>
    );
  }

  return (
    <button className="media-load-full" onClick={() => void loadFull()} disabled={loadingFull}>
      {loadingFull
        ? "解密中…"
        : `📎 ${content.name ?? content.mime} (${sizeKb} KB${content.chunked ? " · 分块" : ""})`}
    </button>
  );
}
