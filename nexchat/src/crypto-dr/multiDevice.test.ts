import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import type { RelayClient, RelayFrame } from "@/relay/relayClient";
import { b64ToBytes } from "@/util/b64";
import { DrTransport } from "@/crypto-dr/drTransport";
import { decodeDmEnvelope } from "@/crypto-dr/dmEnvelope";
import {
  MultiDeviceRouter,
  type DeviceDirectory,
  type PeerBundleProvider,
} from "@/crypto-dr/multiDevice";
import { ensureDrWasm, VodozemacEngine } from "@/crypto-dr/vodozemacEngine";
import { DmKind, type DeviceId, type PeerPrekeyBundle } from "@/crypto-dr/types";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
const eq = (a: Uint8Array, b: Uint8Array) => a.length === b.length && a.every((x, i) => x === b[i]);

/// Capturing relay: records every frame the router sends (no fan-back needed for these tests).
class CaptureRelay implements RelayClient {
  frames: RelayFrame[] = [];
  async connect() {}
  async send(frame: RelayFrame) {
    this.frames.push(frame);
  }
  onMessage() {}
  async sendControl() {}
  onControl() {}
  disconnect() {}
}

/// In-memory directory + provider built from live peer engines.
class FakeDirectory implements DeviceDirectory {
  constructor(private readonly devices: DeviceId[]) {}
  async listDevices(): Promise<DeviceId[]> {
    return this.devices;
  }
}

class EngineBundleProvider implements PeerBundleProvider {
  constructor(private readonly engines: Map<string, VodozemacEngine>) {}
  async get(_account: string, device: DeviceId): Promise<PeerPrekeyBundle | null> {
    for (const eng of this.engines.values()) {
      if (eq(eng.deviceId(), device)) {
        return {
          account: "5peer",
          device: eng.deviceId(),
          ik: eng.identityKey(),
          ikEndorsement: new Uint8Array(64),
          spk: eng.rotateSignedPreKey(),
          spkEndorsement: new Uint8Array(64),
          opk: undefined,
          prekeyEpoch: 0n,
        };
      }
    }
    return null;
  }
}

let aliceAddr: string;
let bobAddr: string;

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../dr-pkg/nexchat_dr_bg.wasm", import.meta.url));
  await ensureDrWasm(readFileSync(wasmPath));
  await cryptoWaitReady();
  const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
  aliceAddr = kr.addFromUri("//Alice").address;
  bobAddr = kr.addFromUri("//Bob").address;
});

describe("MultiDeviceRouter — Scheme A fan-out (§8 / §18.3)", () => {
  it("encrypts an independent copy per peer device and routes by recv_dev", async () => {
    const alice = new VodozemacEngine();
    const bob1 = new VodozemacEngine();
    const bob2 = new VodozemacEngine();
    await alice.init();
    await bob1.init();
    await bob2.init();

    const relay = new CaptureRelay();
    const aliceTx = new DrTransport(alice, relay, aliceAddr);
    const dir = new FakeDirectory([bob1.deviceId(), bob2.deviceId()]);
    const provider = new EngineBundleProvider(
      new Map([
        ["b1", bob1],
        ["b2", bob2],
      ]),
    );
    const router = new MultiDeviceRouter(aliceTx, dir, provider);

    const res = await router.sendToAccount(bobAddr, enc("hi all my devices"));
    expect(res.sentTo).toHaveLength(2);
    expect(res.skipped).toHaveLength(0);
    expect(relay.frames).toHaveLength(2);

    const env1 = decodeDmEnvelope(b64ToBytes(relay.frames[0]!.ciphertextB64));
    const env2 = decodeDmEnvelope(b64ToBytes(relay.frames[1]!.ciphertextB64));

    // Distinct recv_dev (one per peer device), independent ciphertext bodies.
    expect(env1.recvDev).toEqual(bob1.deviceId());
    expect(env2.recvDev).toEqual(bob2.deviceId());
    expect(env1.kind).toBe(DmKind.Init);
    expect(env2.kind).toBe(DmKind.Init);
    expect(env1.body).not.toEqual(env2.body); // NONCE RED LINE: independent sessions

    // Each peer device decrypts only its OWN copy.
    expect(new TextDecoder().decode((await bob1.initInbound(env1)).plaintext)).toBe(
      "hi all my devices",
    );
    expect(new TextDecoder().decode((await bob2.initInbound(env2)).plaintext)).toBe(
      "hi all my devices",
    );
  });

  it("a device cannot decrypt a sibling device's copy (sessions are isolated)", async () => {
    const alice = new VodozemacEngine();
    const bob1 = new VodozemacEngine();
    const bob2 = new VodozemacEngine();
    await alice.init();
    await bob1.init();
    await bob2.init();

    const relay = new CaptureRelay();
    const aliceTx = new DrTransport(alice, relay, aliceAddr);
    const router = new MultiDeviceRouter(
      aliceTx,
      new FakeDirectory([bob1.deviceId(), bob2.deviceId()]),
      new EngineBundleProvider(new Map([["b1", bob1], ["b2", bob2]])),
    );
    await router.sendToAccount(bobAddr, enc("secret"));

    const forBob2 = decodeDmEnvelope(b64ToBytes(relay.frames[1]!.ciphertextB64));
    expect(forBob2.recvDev).toEqual(bob2.deviceId());
    // bob1 must NOT be able to consume bob2's prekey message.
    await expect(bob1.initInbound(forBob2)).rejects.toThrow();
  });

  it("re-sending advances each device's own chain (distinct, non-colliding nonces)", async () => {
    const alice = new VodozemacEngine();
    const bob1 = new VodozemacEngine();
    const bob2 = new VodozemacEngine();
    await alice.init();
    await bob1.init();
    await bob2.init();

    const relay = new CaptureRelay();
    const aliceTx = new DrTransport(alice, relay, aliceAddr);
    const router = new MultiDeviceRouter(
      aliceTx,
      new FakeDirectory([bob1.deviceId(), bob2.deviceId()]),
      new EngineBundleProvider(new Map([["b1", bob1], ["b2", bob2]])),
    );

    await router.sendToAccount(bobAddr, enc("m1")); // frames 0,1
    await router.sendToAccount(bobAddr, enc("m2")); // frames 2,3 (sessions already exist)

    const bodies = relay.frames.map((f) => b64ToBytes(f.ciphertextB64));
    // All four ciphertext payloads are distinct — no nonce/ratchet reuse across devices or messages.
    for (let i = 0; i < bodies.length; i++) {
      for (let j = i + 1; j < bodies.length; j++) {
        expect(eq(bodies[i]!, bodies[j]!)).toBe(false);
      }
    }
    // Second round still targets bob1's own session (distinct ciphertext, counter advanced).
    const env3 = decodeDmEnvelope(bodies[2]!);
    expect(env3.recvDev).toEqual(bob1.deviceId());
  });

  it("excludes the local device from fan-out (no self-send loop)", async () => {
    const alice = new VodozemacEngine();
    const bob1 = new VodozemacEngine();
    await alice.init();
    await bob1.init();

    const relay = new CaptureRelay();
    const aliceTx = new DrTransport(alice, relay, aliceAddr);
    // Directory lists Alice's own device alongside a peer device.
    const router = new MultiDeviceRouter(
      aliceTx,
      new FakeDirectory([alice.deviceId(), bob1.deviceId()]),
      new EngineBundleProvider(new Map([["b1", bob1]])),
    );
    const res = await router.sendToAccount(bobAddr, enc("x"));
    expect(res.sentTo).toHaveLength(1);
    expect(res.sentTo[0]).toEqual(bob1.deviceId());
    expect(relay.frames).toHaveLength(1);
  });

  it("skips devices with no available bundle (offline / unregistered)", async () => {
    const alice = new VodozemacEngine();
    const bob1 = new VodozemacEngine();
    await alice.init();
    await bob1.init();

    const relay = new CaptureRelay();
    const aliceTx = new DrTransport(alice, relay, aliceAddr);
    const ghost = new Uint8Array(16).fill(0x77);
    const router = new MultiDeviceRouter(
      aliceTx,
      new FakeDirectory([bob1.deviceId(), ghost]),
      new EngineBundleProvider(new Map([["b1", bob1]])), // ghost has no engine → null bundle
    );
    const res = await router.sendToAccount(bobAddr, enc("y"));
    await flush();
    expect(res.sentTo).toHaveLength(1);
    expect(res.skipped).toEqual([ghost]);
  });

  it("sibling echo: fans a copy to our OWN devices on the ORIGINAL conv-id with echoSelf", async () => {
    const a1 = new VodozemacEngine();
    const a2 = new VodozemacEngine(); // sibling device of the SAME account
    await a1.init();
    await a2.init();

    const relay = new CaptureRelay();
    const a1Tx = new DrTransport(a1, relay, aliceAddr);
    // The "account" being fanned to is OUR OWN (siblings); a1 itself is excluded.
    const router = new MultiDeviceRouter(
      a1Tx,
      new FakeDirectory([a1.deviceId(), a2.deviceId()]),
      new EngineBundleProvider(new Map([["a2", a2]])),
    );

    const convId = `d:${bobAddr}`; // the conversation is with Bob, not with our own account
    const res = await router.sendToAccount(aliceAddr, enc("echo to my other device"), {
      convId,
      echoSelf: true,
    });

    expect(res.sentTo).toEqual([a2.deviceId()]); // only the sibling, never a1 itself
    expect(relay.frames).toHaveLength(1);
    const frame = relay.frames[0]!;
    expect(frame.convId).toBe(convId); // routes on the ORIGINAL conversation (d:Bob), not d:Alice
    expect(frame.echoSelf).toBe(true);

    const env = decodeDmEnvelope(b64ToBytes(frame.ciphertextB64));
    expect(env.recvDev).toEqual(a2.deviceId());
    // The sibling decrypts the echo on its own a1↔a2 session.
    expect(new TextDecoder().decode((await a2.initInbound(env)).plaintext)).toBe(
      "echo to my other device",
    );
  });
});
