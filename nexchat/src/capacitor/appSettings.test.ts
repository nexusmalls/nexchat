import { describe, expect, it } from "vitest";
import { isMicrophonePermissionError } from "@/capacitor/appSettings";

describe("capacitor/appSettings", () => {
  it("detects permission-related errors", () => {
    expect(isMicrophonePermissionError(new Error("Permission denied"))).toBe(true);
    expect(isMicrophonePermissionError(new Error("NotAllowedError"))).toBe(true);
    expect(isMicrophonePermissionError(new Error("network fail"))).toBe(false);
  });
});
