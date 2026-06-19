// EN: Client-side thumbnail generation (CHAT_LARGE_FILE_SPEC.md §5). Images are scaled to
// ≤maxPx; videos capture the first frame. Returns JPEG bytes or null when unsupported.
// CN: 客户端缩略图生成（大文件规范 §5）。图片缩放到 ≤maxPx；视频抓首帧。不支持则返回 null。

/// EN: Scale an image `File` to a JPEG thumbnail. CN: 把图片 `File` 缩成 JPEG 缩略图。
export async function imageThumbnail(file: File, maxPx: number): Promise<Uint8Array | null> {
  if (!file.type.startsWith("image/")) return null;
  const bmp = await createImageBitmap(file);
  try {
    const scale = Math.min(1, maxPx / Math.max(bmp.width, bmp.height));
    const w = Math.max(1, Math.round(bmp.width * scale));
    const h = Math.max(1, Math.round(bmp.height * scale));
    const canvas = Object.assign(document.createElement("canvas"), { width: w, height: h });
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(bmp, 0, 0, w, h);
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob((b) => resolve(b), "image/jpeg", 0.82),
    );
    if (!blob) return null;
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    bmp.close();
  }
}

/// EN: Capture the first frame of a video as a JPEG thumbnail. CN: 抓取视频首帧为 JPEG 缩略图。
export async function videoThumbnail(file: File, maxPx: number): Promise<Uint8Array | null> {
  if (!file.type.startsWith("video/")) return null;
  const url = URL.createObjectURL(file);
  try {
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.preload = "metadata";
    video.src = url;
    await new Promise<void>((resolve, reject) => {
      video.onloadeddata = () => resolve();
      video.onerror = () => reject(new Error("video load failed"));
    });
    video.currentTime = Math.min(0.1, video.duration || 0);
    await new Promise<void>((resolve) => {
      video.onseeked = () => resolve();
    });
    const scale = Math.min(1, maxPx / Math.max(video.videoWidth, video.videoHeight));
    const w = Math.max(1, Math.round(video.videoWidth * scale));
    const h = Math.max(1, Math.round(video.videoHeight * scale));
    const canvas = Object.assign(document.createElement("canvas"), { width: w, height: h });
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.drawImage(video, 0, 0, w, h);
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob((b) => resolve(b), "image/jpeg", 0.82),
    );
    if (!blob) return null;
    return new Uint8Array(await blob.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
}

/// EN: Pick image or video thumbnail generator. CN: 按类型选择图片或视频缩略图生成器。
export async function mediaThumbnail(file: File, maxPx: number): Promise<Uint8Array | null> {
  if (file.type.startsWith("image/")) return imageThumbnail(file, maxPx);
  if (file.type.startsWith("video/")) return videoThumbnail(file, maxPx);
  return null;
}
