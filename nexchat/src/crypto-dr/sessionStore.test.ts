import "fake-indexeddb/auto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { keyVault } from "@/keyvault/keyvault";
import {
  EncryptedDrSessionStore,
  MemoryDrSessionStore,
  type PublishedOpkBundle,
} from "@/crypto-dr/sessionStore";
import { ensureDrWasm, VodozemacEngine } from "@/crypto-dr/vodozemacEngine";
import type { PeerPrekeyBundle } from "@/crypto-dr/types";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const dec = (b: Uint8Array): string => new TextDecoder().decode(b);

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../dr-pkg/nexchat_dr_bg.wasm", import.meta.url));
  await ensureDrWasm(readFileSync(wasmPath));
});

afterEach(() => keyVault.clear());

const opkBundle: PublishedOpkBundle = {
  device: "aabb",
  root: "1234",
  opks: ["00", "01"],
  spent: ["00"],
};

describe("EncryptedDrSessionStore (vault-encrypted at rest, §17.2)", () => {
  it("round-trips account / session / OPK rows under a vault key", async () => {
    keyVault.initForTest("seed-A");
    const store = new EncryptedDrSessionStore();
    await store.open("acct-A");

    expect(await store.loadAccount()).toBeNull();
    await store.saveAccount("ACCOUNT_PICKLE");
    expect(await store.loadAccount()).toBe("ACCOUNT_PICKLE");

    await store.saveSession("deadbeef", "SESSION_1");
    await store.saveSession("cafe", "SESSION_2");
    expect((await store.listSessions()).sort()).toEqual(["cafe", "deadbeef"]);
    expect(await store.loadSession("deadbeef")).toBe("SESSION_1");

    await store.removeSession("deadbeef");
    expect(await store.loadSession("deadbeef")).toBeNull();
    expect(await store.listSessions()).toEqual(["cafe"]);

    await store.saveOpkBundle(opkBundle);
    expect(await store.loadOpkBundle()).toEqual(opkBundle);
  });

  it("a different vault root cannot read the ciphertext (skips, returns null)", async () => {
    keyVault.initForTest("seed-writer");
    const w = new EncryptedDrSessionStore();
    await w.open("acct-B");
    await w.saveAccount("SECRET");
    keyVault.clear();

    keyVault.initForTest("seed-other");
    const r = new EncryptedDrSessionStore();
    await r.open("acct-B");
    expect(await r.loadAccount()).toBeNull(); // GCM auth fails → treated as absent
  });
});

describe("MemoryDrSessionStore", () => {
  it("behaves as a transient store", async () => {
    const s = new MemoryDrSessionStore();
    await s.open("x");
    await s.saveSession("k", "v");
    expect(await s.loadSession("k")).toBe("v");
    await s.clearAll();
    expect(await s.loadSession("k")).toBeNull();
    expect(await s.listSessions()).toEqual([]);
  });
});

describe("ratchet state survives a restart via pickle persistence", () => {
  it("restores the account + session and continues decrypting", async () => {
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();

    const bobBundle: PeerPrekeyBundle = {
      account: "5bob",
      device: bob.deviceId(),
      ik: bob.identityKey(),
      ikEndorsement: new Uint8Array(64),
      spk: bob.rotateSignedPreKey(),
      spkEndorsement: new Uint8Array(64),
      opk: undefined,
      prekeyEpoch: 0n,
    };
    const aSess = await alice.initOutbound(bobBundle);
    const env1 = bob.decodeFromWire(alice.encodeForWire(await alice.encrypt(aSess, enc("hello"))));
    const { session: bSess } = await bob.initInbound(env1);

    // Persist Alice's account + session pickle, then drop the engine.
    const store = new MemoryDrSessionStore();
    await store.open("5alice");
    await store.saveAccount(alice.pickle());
    await store.saveSession(toHex(bob.deviceId()), alice.pickleSession(bob.deviceId()));

    // Restore a fresh engine from the store and keep talking.
    const alice2 = new VodozemacEngine();
    await alice2.init((await store.loadAccount())!);
    expect(alice2.deviceId()).toEqual(alice.deviceId());
    alice2.loadSession(bob.deviceId(), (await store.loadSession(toHex(bob.deviceId())))!);
    expect(alice2.hasSession(bob.deviceId())).toBe(true);

    const reply = await bob.encrypt(bSess, enc("welcome back"));
    expect(dec(await alice2.decrypt(aSess, reply))).toBe("welcome back");
  });
});

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
