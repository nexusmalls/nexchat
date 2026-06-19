import { beforeAll, describe, expect, it } from "vitest";
import { deriveAnchorKeys, deriveMlsEscrowKey } from "@/store/syncAnchor";
import {
  buildSigningBackupPlain,
  decodeSigningBackupPlain,
  deriveMlsSigningPinWrapKey,
  encodeSigningBackupPlain,
  K_MLS_SIGNING_PIN_WRAP_SALT,
  normalizeSigningPin,
  openSigningBackup,
  sealSigningBackup,
  signingBundleFromPlain,
} from "@/mls/signingPinBackup";

const VAULT_MASTER = new Uint8Array(32).fill(0x11);
const IV = new Uint8Array(12).fill(0x09);

let anchorId: Uint8Array;

beforeAll(async () => {
  const keys = await deriveAnchorKeys(VAULT_MASTER);
  anchorId = keys.anchorId;
  expect(anchorId.length).toBe(32);
});

describe("normalizeSigningPin", () => {
  it("accepts 6–8 digit PINs", () => {
    expect(normalizeSigningPin("123456")).toBe("123456");
    expect(normalizeSigningPin(" 12345678 ")).toBe("12345678");
  });

  it("rejects non-digit and wrong length", () => {
    expect(() => normalizeSigningPin("12345")).toThrow(/6.*8/);
    expect(() => normalizeSigningPin("123456789")).toThrow(/6.*8/);
    expect(() => normalizeSigningPin("12ab56")).toThrow(/数字/);
  });
});

describe("signingPinBackup crypto", () => {
  it("uses frozen salt constant", () => {
    expect(K_MLS_SIGNING_PIN_WRAP_SALT).toBe("chat/mls-signing-backup/v1");
  });

  it("deriveMlsSigningPinWrapKey is deterministic and PIN-separated", async () => {
    const k1 = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "123456");
    const k2 = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "123456");
    const kOtherPin = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "654321");
    const plain = encodeSigningBackupPlain(
      buildSigningBackupPlain({
        account: "5Test",
        deviceId: "dev1",
        backupSeq: 1,
        bundle: new Uint8Array([1, 2, 3]),
        exportedAt: 1000,
      }),
    );
    const ct1 = await crypto.subtle.encrypt({ name: "AES-GCM", iv: IV }, k1, plain as BufferSource);
    const ct2 = await crypto.subtle.encrypt({ name: "AES-GCM", iv: IV }, k2, plain as BufferSource);
    expect(new Uint8Array(ct1)).toEqual(new Uint8Array(ct2));
    await expect(
      crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, kOtherPin, ct1 as BufferSource),
    ).rejects.toBeTruthy();
  });

  it("is domain-separated from K_mls_escrow", async () => {
    const pinKey = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "123456");
    const escrowKey = await deriveMlsEscrowKey(VAULT_MASTER, anchorId);
    const pt = new TextEncoder().encode("probe");
    const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv: IV }, pinKey, pt as BufferSource);
    await expect(
      crypto.subtle.decrypt({ name: "AES-GCM", iv: IV }, escrowKey, ct as BufferSource),
    ).rejects.toBeTruthy();
  });

  it("round-trips seal/open", async () => {
    const key = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "123456");
    const plain = buildSigningBackupPlain({
      account: "5Alice",
      deviceId: "abc12345",
      backupSeq: 3,
      bundle: new Uint8Array([9, 8, 7]),
      exportedAt: 2000,
    });
    const opened = await openSigningBackup(key, await sealSigningBackup(key, plain));
    expect(opened).toEqual(plain);
    expect(signingBundleFromPlain(opened)).toEqual(new Uint8Array([9, 8, 7]));
  });

  it("wrong PIN fails open", async () => {
    const key = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "123456");
    const wrong = await deriveMlsSigningPinWrapKey(VAULT_MASTER, anchorId, "111111");
    const packed = await sealSigningBackup(
      key,
      buildSigningBackupPlain({
        account: "5Alice",
        deviceId: "d",
        backupSeq: 1,
        bundle: new Uint8Array([1]),
      }),
    );
    await expect(openSigningBackup(wrong, packed)).rejects.toBeTruthy();
  });

  it("encode/decode plaintext", () => {
    const plain = buildSigningBackupPlain({
      account: "5Bob",
      deviceId: "dev",
      backupSeq: 2,
      bundle: new Uint8Array([4, 5]),
    });
    expect(decodeSigningBackupPlain(encodeSigningBackupPlain(plain))).toEqual(plain);
  });
});
