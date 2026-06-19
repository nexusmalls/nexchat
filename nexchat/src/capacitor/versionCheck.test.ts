import { describe, expect, it } from "vitest";
import { isNewerAppVersion, type AppVersionInfo } from "@/capacitor/versionCheck";

describe("capacitor/versionCheck", () => {
  const running: AppVersionInfo = { version: "0.1.0", builtAt: "2026-06-09T10:00:00.000Z" };

  it("detects version bump", () => {
    expect(isNewerAppVersion(running, { version: "0.1.1", builtAt: running.builtAt })).toBe(true);
  });

  it("detects rebuilt same version", () => {
    expect(
      isNewerAppVersion(running, { version: "0.1.0", builtAt: "2026-06-09T12:00:00.000Z" }),
    ).toBe(true);
  });

  it("ignores when unchanged", () => {
    expect(isNewerAppVersion(running, running)).toBe(false);
  });

  it("treats missing running build stamp as not newer", () => {
    expect(isNewerAppVersion(null, running)).toBe(false);
    expect(isNewerAppVersion({ version: "0.1.0", builtAt: "" }, running)).toBe(false);
  });
});
