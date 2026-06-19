// EN: Frozen test vectors for §5.0 vault_master derivation. If any of these change,
// every existing ciphertext rooted in vault_master becomes unreadable — treat a failure
// here as a release blocker, never "fix the expected value".
// CN: §5.0 vault_master 派生的冻结测试向量。任何向量变化都意味着所有以 vault_master 为根
// 的存量密文不可读——本文件失败属发布阻断，绝不允许「改期望值」了事。

import { beforeAll, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import {
  clearVaultMaster,
  deriveVaultMasterFromPair,
  deriveVaultMasterFromSuri,
  extractSr25519Secret,
  getVaultMaster,
  setVaultMaster,
} from "@/wallet/vaultMaster";

const ALICE = "//Alice";
const ALICE_SS58 = "X4Y9wZky3HPgyUGy5xH1RrwEVg3rTuzxYQ1GAKWscgAysZvxT";
// Frozen vectors (generated once from @polkadot/keyring + WebCrypto HKDF-SHA256),
// domain-separated via the Nexus prefix-273 address:
const ALICE_SECRET_FIRST8 = "98319d4ff8a9508c";
const ALICE_MASTER_HEX = "086dbd77862df432e1122f8fbfe574de0cd4c8ceb8c7574d41c39002d4c12e2c";
const BOB_MASTER_HEX = "5462275fac9bb70e9f1bb3b8dedd0323174f7a9259255f0b336d7a24a914619a";

const hex = (b: Uint8Array) => [...b].map((x) => x.toString(16).padStart(2, "0")).join("");

let keyring: Keyring;

beforeAll(async () => {
  await cryptoWaitReady();
  keyring = new Keyring({ type: "sr25519", ss58Format: 273 });
});

describe("extractSr25519Secret", () => {
  it("extracts the 64-byte expanded secret from the PKCS8 layout", () => {
    const pair = keyring.addFromUri(ALICE);
    const secret = extractSr25519Secret(pair);
    expect(secret.length).toBe(64);
    expect(hex(secret.slice(0, 8))).toBe(ALICE_SECRET_FIRST8);
  });

  it("rejects a locked pair", () => {
    const pair = keyring.addFromUri(ALICE);
    pair.lock();
    expect(() => extractSr25519Secret(pair)).toThrow(/locked/);
  });
});

describe("deriveVaultMaster", () => {
  it("matches the frozen //Alice vector and is 32 bytes", async () => {
    const pair = keyring.addFromUri(ALICE);
    expect(pair.address).toBe(ALICE_SS58);
    const master = await deriveVaultMasterFromPair(pair);
    expect(master.length).toBe(32);
    expect(hex(master)).toBe(ALICE_MASTER_HEX);
  });

  it("pair path and suri path agree (new-device recompute)", async () => {
    const fromPair = await deriveVaultMasterFromPair(keyring.addFromUri(ALICE));
    const fromSuri = await deriveVaultMasterFromSuri(ALICE);
    expect(hex(fromSuri)).toBe(hex(fromPair));
  });

  it("separates accounts (//Bob frozen vector differs)", async () => {
    const bob = await deriveVaultMasterFromSuri("//Bob");
    expect(hex(bob)).toBe(BOB_MASTER_HEX);
    expect(hex(bob)).not.toBe(ALICE_MASTER_HEX);
  });

  it("is NOT derivable from the public address (differs from SHA-256(ss58))", async () => {
    const legacy = new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(ALICE_SS58)),
    );
    expect(hex(legacy)).not.toBe(ALICE_MASTER_HEX);
  });
});

describe("vault master holder", () => {
  it("set/get/clear round-trips and zeroizes on clear", async () => {
    const master = await deriveVaultMasterFromSuri(ALICE);
    setVaultMaster(master);
    expect(getVaultMaster()).toBe(master);
    clearVaultMaster();
    expect(getVaultMaster()).toBeNull();
    expect(hex(master)).toBe("00".repeat(32));
  });
});
