// EN: Tests for the Track A device state machine + "credential not ready" classification (§5.6/§7.3).
// CN: 路线 A 设备态机 + 「凭证未就绪」分类（§5.6/§7.3）单测。

import { describe, expect, it } from "vitest";

import { classifyCredentialReadiness, deriveDeviceMode } from "@/mls/deviceState";

describe("deriveDeviceMode (§7.3)", () => {
  it("restoring dominates regardless of send capability", () => {
    expect(deriveDeviceMode({ restoring: true, canSend: true })).toBe("restoring");
    expect(deriveDeviceMode({ restoring: true, canSend: false })).toBe("restoring");
  });

  it("primary iff the device may send, else secondary", () => {
    expect(deriveDeviceMode({ restoring: false, canSend: true })).toBe("primary");
    expect(deriveDeviceMode({ restoring: false, canSend: false })).toBe("secondary");
  });
});

describe("classifyCredentialReadiness (§5.6 three-branch)", () => {
  const base = {
    rpcConnected: true,
    hasLocalSession: false,
    hasPendingWelcome: false,
    joinedOnChain: false,
    hasVault: false,
  };

  it("a usable local session is ready (and short-circuits everything else)", () => {
    expect(
      classifyCredentialReadiness({ ...base, hasLocalSession: true, rpcConnected: false }),
    ).toEqual({ status: "ready" });
  });

  it("① RPC down with no session → retry", () => {
    expect(classifyCredentialReadiness({ ...base, rpcConnected: false })).toEqual({
      status: "blocked",
      branch: "rpc-disconnected",
      action: "retry",
    });
  });

  it("② pending Welcome for a fresh group → claim welcome", () => {
    expect(
      classifyCredentialReadiness({ ...base, hasPendingWelcome: true, joinedOnChain: false }),
    ).toEqual({ status: "blocked", branch: "unclaimed-welcome", action: "claim-welcome" });
  });

  it("③a member without session but with a vault → restore from vault", () => {
    expect(
      classifyCredentialReadiness({ ...base, joinedOnChain: true, hasVault: true }),
    ).toEqual({ status: "blocked", branch: "restore-from-vault", action: "restore-vault" });
  });

  it("③b member without session and without vault → fallback recovery", () => {
    expect(
      classifyCredentialReadiness({ ...base, joinedOnChain: true, hasVault: false }),
    ).toEqual({ status: "blocked", branch: "no-session-no-vault", action: "fallback-recovery" });
  });

  it("not a member and no Welcome → await invite", () => {
    expect(classifyCredentialReadiness(base)).toEqual({
      status: "blocked",
      branch: "not-a-member",
      action: "await-invite",
    });
  });

  it("Welcome takes priority over the member/vault branches", () => {
    expect(
      classifyCredentialReadiness({
        ...base,
        hasPendingWelcome: true,
        joinedOnChain: true,
        hasVault: true,
      }),
    ).toEqual({ status: "blocked", branch: "unclaimed-welcome", action: "claim-welcome" });
  });
});
