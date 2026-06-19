// EN: Ephemeral blob URLs for optimistic image/video preview while uploading.
// CN: 上传过程中用于乐观预览的临时 blob URL。

export function createMediaPreviewUrl(file: File): string | undefined {
  if (file.type.startsWith("image/") || file.type.startsWith("video/")) {
    return URL.createObjectURL(file);
  }
  return undefined;
}

/** @deprecated Use createMediaPreviewUrl — kept for existing imports. */
export function createImagePreviewUrl(file: File): string | undefined {
  return createMediaPreviewUrl(file);
}

export function createThumbPreviewUrl(thumb: Uint8Array): string {
  return URL.createObjectURL(new Blob([thumb as BlobPart], { type: "image/jpeg" }));
}

export function revokePreviewUrl(url?: string): void {
  if (url) URL.revokeObjectURL(url);
}
