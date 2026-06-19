import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import { DmKind, type PeerPrekeyBundle } from "@/crypto-dr/types";
import { VodozemacEngine, ensureDrWasm } from "@/crypto-dr/vodozemacEngine";

beforeAll(async () => {
  // Node/vitest: load the wasm bytes directly (the browser `?url` asset can't be fetched).
  const wasmPath = fileURLToPath(new URL("../dr-pkg/nexchat_dr_bg.wasm", import.meta.url));
  await ensureDrWasm(readFileSync(wasmPath));
});

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const dec = (b: Uint8Array): string => new TextDecoder().decode(b);

/// Build a prekey bundle for `peer`, optionally consuming a one-time prekey.
async function bundleFor(peer: VodozemacEngine, useOpk: boolean): Promise<PeerPrekeyBundle> {
  const spk = peer.rotateSignedPreKey();
  const opks = useOpk ? peer.generateOneTimePreKeys(3) : [];
  return {
    account: "5peer",
    device: peer.deviceId(),
    ik: peer.identityKey(),
    ikEndorsement: new Uint8Array(64),
    spk,
    spkEndorsement: new Uint8Array(64),
    opk: useOpk ? opks[0] : undefined,
    prekeyEpoch: 3n,
  };
}

describe("VodozemacEngine (vodozemac wasm) — end to end", () => {
  it("X3DH (OPK) + ratchet: Alice ↔ Bob over the wire", async () => {
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();

    // Distinct self-certifying device ids.
    expect(alice.deviceId()).not.toEqual(bob.deviceId());

    // Alice runs X3DH against Bob's published prekey bundle (uses an OPK).
    const bobBundle = await bundleFor(bob, true);
    const aSess = await alice.initOutbound(bobBundle);

    // First message is a dm_init (PreKey) carrying X3DH; stamped with Bob's epoch.
    const env1 = await alice.encrypt(aSess, enc("hello bob"));
    expect(env1.kind).toBe(DmKind.Init);
    expect(env1.recvDev).toEqual(bob.deviceId());
    expect(env1.senderDev).toEqual(alice.deviceId());
    expect(env1.prekeyEpoch).toBe(3n);

    // Through the relay wire (encode → decode) before Bob consumes it.
    const wire = alice.encodeForWire(env1);
    const env1b = bob.decodeFromWire(wire);

    const { session: bSess, plaintext } = await bob.initInbound(env1b);
    expect(dec(plaintext)).toBe("hello bob");
    expect(bSess.peerDevice).toEqual(alice.deviceId());

    // Bob replies; Alice decrypts (ratchet advances).
    const reply = await bob.encrypt(bSess, enc("hi alice"));
    expect(dec(await alice.decrypt(aSess, reply))).toBe("hi alice");

    // A few more rounds in both directions.
    expect(dec(await bob.decrypt(bSess, await alice.encrypt(aSess, enc("msg2"))))).toBe("msg2");
    expect(dec(await alice.decrypt(aSess, await bob.encrypt(bSess, enc("msg3"))))).toBe("msg3");
    expect(dec(await bob.decrypt(bSess, await alice.encrypt(aSess, enc("msg4"))))).toBe("msg4");
  });

  it("falls back to SPK when no OPK is available", async () => {
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();

    const bobBundle = await bundleFor(bob, false); // opk undefined → SPK fallback
    expect(bobBundle.opk).toBeUndefined();
    const aSess = await alice.initOutbound(bobBundle);

    const env1 = bob.decodeFromWire(
      alice.encodeForWire(await alice.encrypt(aSess, enc("via spk"))),
    );
    const { session: bSess, plaintext } = await bob.initInbound(env1);
    expect(dec(plaintext)).toBe("via spk");
    expect(dec(await alice.decrypt(aSess, await bob.encrypt(bSess, enc("ack"))))).toBe("ack");
  });

  it("restores account state from a pickle", async () => {
    const e = new VodozemacEngine();
    await e.init();
    const id = e.identityKey();
    const pickle = e.pickle();

    const e2 = new VodozemacEngine();
    await e2.init(pickle);
    expect(e2.identityKey()).toEqual(id);
    expect(e2.deviceId()).toEqual(e.deviceId());
  });

  it("rejects an Init envelope whose sender_dev does not match its IK", async () => {
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();
    const aSess = await alice.initOutbound(await bundleFor(bob, true));
    const env = await alice.encrypt(aSess, enc("x"));
    env.senderDev = new Uint8Array(16).fill(0xee); // tamper the routing header
    await expect(bob.initInbound(env)).rejects.toThrow();
  });
});
