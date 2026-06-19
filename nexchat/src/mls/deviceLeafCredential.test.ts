import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { u8aToHex } from "@polkadot/util";
import { beforeAll, describe, expect, it } from "vitest";

import { leafKeyBindingBytes, verifyLeafKeyBinding } from "@/mls/deviceLeafCredential";

const NEX_SS58 = 273;

function makePair(): KeyringPair {
  const kr = new Keyring({ type: "sr25519", ss58Format: NEX_SS58 });
  return kr.addFromMnemonic(mnemonicGenerate());
}

describe("deviceLeafCredential (in-MLS leaf-key binding)", () => {
  beforeAll(async () => {
    await cryptoWaitReady();
  });

  it("verifies a genuine account-key binding over the stable leaf signature key", async () => {
    const pair = makePair();
    const leafKey = new Uint8Array(32).fill(7);
    const deviceId = "devFp1";
    const sig = u8aToHex(pair.sign(leafKeyBindingBytes(pair.address, deviceId, leafKey)));
    expect(await verifyLeafKeyBinding(pair.address, deviceId, leafKey, sig)).toBe(true);
  });

  it("rejects a leaf-key binding signed by a different account (impersonation)", async () => {
    const victim = makePair();
    const attacker = makePair();
    const leafKey = new Uint8Array(32).fill(3);
    const forged = u8aToHex(attacker.sign(leafKeyBindingBytes(victim.address, "d", leafKey)));
    expect(await verifyLeafKeyBinding(victim.address, "d", leafKey, forged)).toBe(false);
  });

  it("rejects when the leaf key is swapped after signing (binding is key-scoped)", async () => {
    const pair = makePair();
    const leafKey = new Uint8Array(32).fill(1);
    const sig = u8aToHex(pair.sign(leafKeyBindingBytes(pair.address, "d", leafKey)));
    const otherKey = new Uint8Array(32).fill(2);
    expect(await verifyLeafKeyBinding(pair.address, "d", otherKey, sig)).toBe(false);
  });

  it("rejects when the deviceId is swapped (binding is device-scoped)", async () => {
    const pair = makePair();
    const leafKey = new Uint8Array(32).fill(5);
    const sig = u8aToHex(pair.sign(leafKeyBindingBytes(pair.address, "devA", leafKey)));
    expect(await verifyLeafKeyBinding(pair.address, "devB", leafKey, sig)).toBe(false);
  });

  it("rejects a malformed signature without throwing", async () => {
    const pair = makePair();
    const leafKey = new Uint8Array(32).fill(0);
    expect(await verifyLeafKeyBinding(pair.address, "d", leafKey, "0xdeadbeef")).toBe(false);
    expect(await verifyLeafKeyBinding(pair.address, "d", leafKey, "not-hex")).toBe(false);
  });
});
