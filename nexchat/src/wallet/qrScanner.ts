// EN: QR scan helpers — album decode, live camera frame decode, recipient parsing.
// CN: 二维码扫描——相册解码、相机画面解码、收款地址解析。

import jsQR from "jsqr";
import { decodeAddress } from "@polkadot/util-crypto";

export type QrScanErrorCode =
  | "NO_QR_FOUND"
  | "INVALID_IMAGE"
  | "INVALID_RECIPIENT"
  | "CAMERA_DENIED";

export class QrScanError extends Error {
  constructor(
    readonly code: QrScanErrorCode,
    message?: string,
  ) {
    super(message ?? code);
    this.name = "QrScanError";
  }
}

function isValidSs58(address: string): boolean {
  try {
    decodeAddress(address);
    return true;
  } catch {
    return false;
  }
}

// EN: Extract SS58 recipient from raw QR payload (address, URI, JSON).
// CN: 从二维码原始内容解析 SS58 收款地址。
export function parseQrRecipient(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  if (isValidSs58(trimmed)) return trimmed;

  const uriMatch = trimmed.match(/(?:substrate|polkadot):([^:?#\s]+)/i);
  if (uriMatch?.[1] && isValidSs58(uriMatch[1])) return uriMatch[1];

  try {
    const json = JSON.parse(trimmed) as Record<string, unknown>;
    const candidate = json.address ?? json.account ?? json.recipient;
    if (typeof candidate === "string" && isValidSs58(candidate)) return candidate;
  } catch {
    /* plain text / URI */
  }

  return null;
}

async function loadImage(file: File): Promise<ImageBitmap | HTMLImageElement> {
  if ("createImageBitmap" in window) {
    return createImageBitmap(file);
  }

  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const image = new Image();
    image.onload = () => {
      URL.revokeObjectURL(url);
      resolve(image);
    };
    image.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new QrScanError("INVALID_IMAGE"));
    };
    image.src = url;
  });
}

// EN: Decode QR from an image file (album / file picker).
// CN: 从图片文件解码二维码（相册 / 文件选择）。
export async function scanQrCodeFromImage(file: File): Promise<string> {
  if (!file.type.startsWith("image/")) {
    throw new QrScanError("INVALID_IMAGE");
  }

  let source: ImageBitmap | HTMLImageElement;
  try {
    source = await loadImage(file);
  } catch (e) {
    if (e instanceof QrScanError) throw e;
    throw new QrScanError("INVALID_IMAGE");
  }

  const width = source.width;
  const height = source.height;
  if (!width || !height) {
    if ("close" in source) source.close();
    throw new QrScanError("INVALID_IMAGE");
  }

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;

  const context = canvas.getContext("2d");
  if (!context) {
    if ("close" in source) source.close();
    throw new QrScanError("INVALID_IMAGE");
  }

  context.drawImage(source, 0, 0, width, height);
  if ("close" in source) source.close();

  let imageData: ImageData;
  try {
    imageData = context.getImageData(0, 0, width, height);
  } catch {
    throw new QrScanError("INVALID_IMAGE");
  }

  return decodeQrFromImageData(imageData) ?? (() => {
    throw new QrScanError("NO_QR_FOUND");
  })();
}

// EN: Decode QR from a live camera frame.
// CN: 从相机画面解码二维码。
export function decodeQrFromImageData(imageData: ImageData): string | null {
  const result = jsQR(imageData.data, imageData.width, imageData.height);
  return result?.data?.trim() ?? null;
}
