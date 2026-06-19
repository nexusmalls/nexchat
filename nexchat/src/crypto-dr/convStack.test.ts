import { beforeAll, describe, expect, it, vi } from "vitest";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { Keyring } from "@polkadot/keyring";
import { MemoryConvStackRegistry, resolveConvStack } from "@/crypto-dr/convStack";
import type { StackChoice } from "@/crypto-dr/prekeyFetch";

let peer: string;

beforeAll(async () => {
  await cryptoWaitReady();
  peer = new Keyring({ type: "sr25519", ss58Format: 273 }).addFromUri("//Bob").address;
});

const negotiator = (result: StackChoice) => vi.fn(async () => result);

describe("convStack — 二选一收口 (M3, §12/§20)", () => {
  it("returns an existing pin without re-negotiating", async () => {
    const reg = new MemoryConvStackRegistry();
    await reg.set(peer, "dr");
    const neg = negotiator("mls_wire");
    expect(await resolveConvStack(peer, reg, { negotiate: neg })).toBe("dr");
    expect(neg).not.toHaveBeenCalled();
  });

  it("negotiates + pins on first contact, then is sticky", async () => {
    const reg = new MemoryConvStackRegistry();
    const neg = negotiator("dr");
    expect(await resolveConvStack(peer, reg, { negotiate: neg })).toBe("dr");
    expect(neg).toHaveBeenCalledTimes(1);
    // second call uses the pin (二选一收口) — negotiator not called again
    expect(await resolveConvStack(peer, reg, { negotiate: neg })).toBe("dr");
    expect(neg).toHaveBeenCalledTimes(1);
  });

  it("does not pin an incompatible (none) result", async () => {
    const reg = new MemoryConvStackRegistry();
    expect(await resolveConvStack(peer, reg, { negotiate: negotiator("none") })).toBe("none");
    expect(await reg.get(peer)).toBeNull(); // retryable after peer upgrades
  });

  it("renegotiate overrides the existing pin (migration)", async () => {
    const reg = new MemoryConvStackRegistry();
    await reg.set(peer, "mls_wire");
    const neg = negotiator("dr");
    expect(await resolveConvStack(peer, reg, { negotiate: neg, renegotiate: true })).toBe("dr");
    expect(neg).toHaveBeenCalledTimes(1);
    expect(await reg.get(peer)).toBe("dr");
  });

  it("canonicalizes the peer key (42/273 prefix-agnostic)", async () => {
    const reg = new MemoryConvStackRegistry();
    await reg.set(peer, "dr");
    const alt = new Keyring({ type: "sr25519", ss58Format: 42 }).addFromUri("//Bob").address;
    expect(await reg.get(alt)).toBe("dr"); // same account, different prefix
  });
});
