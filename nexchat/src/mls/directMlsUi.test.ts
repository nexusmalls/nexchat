import { describe, expect, it } from "vitest";
import { directMlsBadgeText, directMlsDetailHint } from "@/mls/directMlsUi";

describe("directMlsUi", () => {
  it("shows ready badge", () => {
    expect(directMlsBadgeText({ ready: true, role: "owner" })).toBe("E2EE 就绪");
  });

  it("shows role-specific handshake badges", () => {
    expect(directMlsBadgeText({ ready: false, role: "owner" })).toBe("握手中·等对端 KP");
    expect(directMlsBadgeText({ ready: false, role: "member" })).toBe("握手中·等 Welcome");
  });

  it("detail hint mentions handshake not online", () => {
    expect(directMlsDetailHint({ ready: false, role: "owner" })).toContain("KeyPackage");
    expect(directMlsDetailHint({ ready: false, role: "member" })).toContain("Welcome");
  });
});
