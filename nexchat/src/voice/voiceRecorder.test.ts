import { describe, expect, it } from "vitest";
import { MAX_VOICE_MS, MIN_VOICE_MS } from "@/voice/voiceRecorder";

describe("voiceRecorder constants", () => {
  it("uses sensible voice duration bounds", () => {
    expect(MIN_VOICE_MS).toBeGreaterThanOrEqual(500);
    expect(MAX_VOICE_MS).toBeGreaterThan(MIN_VOICE_MS);
    expect(MAX_VOICE_MS).toBeLessThanOrEqual(120_000);
  });
});
