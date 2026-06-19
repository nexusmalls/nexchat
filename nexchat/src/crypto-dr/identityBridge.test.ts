import { beforeAll, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import type { KeyringPair } from "@polkadot/keyring/types";
import { clearSigner, setSignerPair } from "@/chain/signer";
import {
  CTX_IK_ENDORSE,
  CTX_SPK_ENDORSE,
  endorseKey,
  verifyEndorsement,
} from "@/crypto-dr/identityBridge";

let alice: KeyringPair;
let bob: KeyringPair;

beforeAll(async () => {
  await cryptoWaitReady();
  const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
  alice = kr.addFromUri("//Alice");
  bob = kr.addFromUri("//Bob");
});

const dec = (b: Uint8Array): string => new TextDecoder().decode(b);

describe("identity bridge — endorsement contexts (frozen, match pallet-msg-identity)", () => {
  it("CTX strings equal the on-chain constants byte-for-byte", () => {
    expect(dec(CTX_IK_ENDORSE)).toBe("nexchat/x3dh/ik-endorse/v1");
    expect(dec(CTX_SPK_ENDORSE)).toBe("nexchat/x3dh/spk-endorse/v1");
  });
});

describe("identity bridge — account-key endorsement (sr25519)", () => {
  it("endorses an IK and verifies relay-trustlessly against the account address", () => {
    setSignerPair(alice);
    try {
      const ik = new Uint8Array(32).fill(0x11);
      const sig = endorseKey(CTX_IK_ENDORSE, ik);
      expect(sig.length).toBe(64);
      expect(verifyEndorsement(CTX_IK_ENDORSE, ik, sig, alice.address)).toBe(true);
    } finally {
      clearSigner();
    }
  });

  it("rejects verification under the wrong account", () => {
    setSignerPair(alice);
    try {
      const spk = new Uint8Array(32).fill(0x22);
      const sig = endorseKey(CTX_SPK_ENDORSE, spk);
      expect(verifyEndorsement(CTX_SPK_ENDORSE, spk, sig, bob.address)).toBe(false);
    } finally {
      clearSigner();
    }
  });

  it("rejects verification under the wrong context (domain separation)", () => {
    setSignerPair(alice);
    try {
      const key = new Uint8Array(32).fill(0x33);
      const sig = endorseKey(CTX_IK_ENDORSE, key);
      // Same key + sig but verified under the SPK context must fail.
      expect(verifyEndorsement(CTX_SPK_ENDORSE, key, sig, alice.address)).toBe(false);
    } finally {
      clearSigner();
    }
  });

  it("rejects a tampered key", () => {
    setSignerPair(alice);
    try {
      const key = new Uint8Array(32).fill(0x44);
      const sig = endorseKey(CTX_IK_ENDORSE, key);
      const tampered = new Uint8Array(32).fill(0x45);
      expect(verifyEndorsement(CTX_IK_ENDORSE, tampered, sig, alice.address)).toBe(false);
    } finally {
      clearSigner();
    }
  });

  it("throws when no raw-signing pair is active", () => {
    clearSigner();
    expect(() => endorseKey(CTX_IK_ENDORSE, new Uint8Array(32))).toThrow();
  });
});
