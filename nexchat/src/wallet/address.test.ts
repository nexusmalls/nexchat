import { describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { canonicalAddress, nexDisplayAddress, RPC_SS58 } from "@/wallet/address";
import { NEX_SS58 } from "@/wallet/desktopKeyring";

describe("canonicalAddress", () => {
  it("maps NEX (273) and generic (42) to the canonical SS58 (273)", async () => {
    await cryptoWaitReady();
    const pair42 = new Keyring({ type: "sr25519", ss58Format: 42 }).addFromUri("//Alice");
    const pair273 = new Keyring({ type: "sr25519", ss58Format: NEX_SS58 }).addFromUri("//Alice");
    expect(pair42.address).not.toBe(pair273.address);
    expect(canonicalAddress(pair42.address)).toBe(pair273.address);
    expect(canonicalAddress(pair273.address)).toBe(pair273.address);
    expect(RPC_SS58).toBe(273);
  });

  it("nexDisplayAddress uses SS58 prefix 273 (X…)", async () => {
    await cryptoWaitReady();
    const pair42 = new Keyring({ type: "sr25519", ss58Format: 42 }).addFromUri("//Alice");
    const pair273 = new Keyring({ type: "sr25519", ss58Format: NEX_SS58 }).addFromUri("//Alice");
    expect(nexDisplayAddress(pair42.address)).toBe(pair273.address);
    expect(nexDisplayAddress(pair42.address).startsWith("X")).toBe(true);
  });
});
