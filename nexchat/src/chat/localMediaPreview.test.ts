import { describe, expect, it } from "vitest";
import {
  createImagePreviewUrl,
  createMediaPreviewUrl,
  createThumbPreviewUrl,
  revokePreviewUrl,
} from "@/chat/localMediaPreview";

describe("localMediaPreview", () => {
  it("creates blob URL for images", () => {
    const file = new File([new Uint8Array([1, 2, 3])], "a.png", { type: "image/png" });
    const url = createMediaPreviewUrl(file);
    expect(url).toMatch(/^blob:/);
    revokePreviewUrl(url);
  });

  it("creates blob URL for videos", () => {
    const file = new File([new Uint8Array([1, 2, 3])], "a.mp4", { type: "video/mp4" });
    const url = createMediaPreviewUrl(file);
    expect(url).toMatch(/^blob:/);
    revokePreviewUrl(url);
  });

  it("createImagePreviewUrl alias works for images", () => {
    const file = new File([new Uint8Array([1])], "a.png", { type: "image/png" });
    expect(createImagePreviewUrl(file)).toMatch(/^blob:/);
    revokePreviewUrl(createImagePreviewUrl(file));
  });

  it("returns undefined for non-media", () => {
    const file = new File(["x"], "a.txt", { type: "text/plain" });
    expect(createMediaPreviewUrl(file)).toBeUndefined();
  });

  it("creates jpeg thumb preview URL", () => {
    const url = createThumbPreviewUrl(new Uint8Array([0xff, 0xd8, 0xff]));
    expect(url).toMatch(/^blob:/);
    revokePreviewUrl(url);
  });
});
