import { beforeAll, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import type { KeyringPair } from "@polkadot/keyring/types";
import { clearSigner, setSignerPair } from "@/chain/signer";
import type { RawDeviceRecord, RawSignedPrekey } from "@/chain/chainClient";
import { deviceIdFromIk } from "@/crypto-dr/dmEnvelope";
import { CTX_IK_ENDORSE, CTX_SPK_ENDORSE, endorseKey } from "@/crypto-dr/identityBridge";
import { assemblePeerBundle, chooseStack } from "@/crypto-dr/prekeyFetch";
import { STACK_DR, STACK_MLS_WIRE } from "@/crypto-dr/types";

let bob: KeyringPair;

beforeAll(async () => {
  await cryptoWaitReady();
  bob = new Keyring({ type: "sr25519", ss58Format: 273 }).addFromUri("//Bob");
});

/// Build a chain-shaped (verified-able) device record + SPK signed by `bob`.
function bobRecords(): { dev: RawDeviceRecord; spk: RawSignedPrekey } {
  const ik = new Uint8Array(32).fill(0x11);
  const spk = new Uint8Array(32).fill(0x22);
  setSignerPair(bob);
  try {
    return {
      dev: {
        deviceId: deviceIdFromIk(ik),
        ik,
        ikEndorsement: endorseKey(CTX_IK_ENDORSE, ik),
        prekeyEpoch: 7n,
      },
      spk: { spk, spkEndorsement: endorseKey(CTX_SPK_ENDORSE, spk), validUntil: 0n },
    };
  } finally {
    clearSigner();
  }
}

describe("prekeyFetch — assemblePeerBundle (relay-trustless verification)", () => {
  it("assembles a verified SPK-fallback bundle (no OPK in v1)", () => {
    const { dev, spk } = bobRecords();
    const bundle = assemblePeerBundle(bob.address, dev, spk);
    expect(bundle.account).toBe(bob.address);
    expect(bundle.device).toEqual(dev.deviceId);
    expect(bundle.ik).toEqual(dev.ik);
    expect(bundle.spk).toEqual(spk.spk);
    expect(bundle.prekeyEpoch).toBe(7n);
    expect(bundle.opk).toBeUndefined();
  });

  it("rejects a device id that is not blake2_128(ik)", () => {
    const { dev, spk } = bobRecords();
    const bad = { ...dev, deviceId: new Uint8Array(16).fill(0xaa) };
    expect(() => assemblePeerBundle(bob.address, bad, spk)).toThrow(/device id/);
  });

  it("rejects a forged IK endorsement (wrong account)", () => {
    const { dev, spk } = bobRecords();
    const eve = new Keyring({ type: "sr25519", ss58Format: 273 }).addFromUri("//Eve");
    expect(() => assemblePeerBundle(eve.address, dev, spk)).toThrow(/IK endorsement/);
  });

  it("rejects a tampered SPK", () => {
    const { dev, spk } = bobRecords();
    const bad = { ...spk, spk: new Uint8Array(32).fill(0x23) };
    expect(() => assemblePeerBundle(bob.address, dev, bad)).toThrow(/SPK endorsement/);
  });
});

describe("prekeyFetch — chooseStack (§20 negotiation)", () => {
  const self = STACK_DR | STACK_MLS_WIRE;
  it("picks DR when both support it", () => {
    expect(chooseStack(STACK_DR | STACK_MLS_WIRE, self)).toBe("dr");
    expect(chooseStack(STACK_DR, self)).toBe("dr");
  });
  it("falls back to MLS-Wire when peer lacks DR", () => {
    expect(chooseStack(STACK_MLS_WIRE, self)).toBe("mls_wire");
  });
  it("is incompatible when no stack is shared", () => {
    expect(chooseStack(STACK_DR, STACK_MLS_WIRE)).toBe("none");
    expect(chooseStack(0, self)).toBe("none");
  });
});
