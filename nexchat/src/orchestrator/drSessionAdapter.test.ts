import { describe, expect, it } from "vitest";
import type { MultiDeviceRouter, MultiDeviceSendResult } from "@/crypto-dr/multiDevice";
import { DrSessionAdapter } from "@/orchestrator/drSessionAdapter";

const PEER = "5peer";

/// Minimal fake router: records send calls, no real crypto.
function fakeRouter() {
  const sent: { account: string; plaintext: Uint8Array }[] = [];
  const router = {
    async sendToAccount(account: string, plaintext: Uint8Array): Promise<MultiDeviceSendResult> {
      sent.push({ account, plaintext });
      return { sentTo: [], skipped: [] };
    },
  } as unknown as MultiDeviceRouter;
  return { router, sent };
}

describe("DrSessionAdapter — DrSessionPort lifecycle + send gating", () => {
  it("open marks active; isActive true", async () => {
    const { router } = fakeRouter();
    const a = new DrSessionAdapter(router);
    expect(a.isActive(PEER)).toBe(false);
    await a.open(PEER);
    expect(a.isActive(PEER)).toBe(true);
  });

  it("freeze blocks sends and isActive; resume restores", async () => {
    const { router, sent } = fakeRouter();
    const a = new DrSessionAdapter(router);
    await a.open(PEER);
    await a.freeze(PEER);
    expect(a.isActive(PEER)).toBe(false);
    await expect(a.send(PEER, new Uint8Array([1]))).rejects.toThrow(/not active/);
    expect(sent.length).toBe(0);

    await a.resume(PEER);
    expect(a.isActive(PEER)).toBe(true);
    await a.send(PEER, new Uint8Array([2]));
    expect(sent.length).toBe(1);
  });

  it("retire deactivates; sends rejected", async () => {
    const { router } = fakeRouter();
    const a = new DrSessionAdapter(router);
    await a.open(PEER);
    await a.retire(PEER);
    expect(a.isActive(PEER)).toBe(false);
    await expect(a.send(PEER, new Uint8Array([1]))).rejects.toThrow(/not active/);
  });
});
