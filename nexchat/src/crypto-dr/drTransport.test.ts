import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { canonicalAddress } from "@/wallet/address";
import type { RelayClient, RelayFrame } from "@/relay/relayClient";
import { bytesToB64 } from "@/util/b64";
import { DrTransport, restoreDrTransport, type DrIncoming } from "@/crypto-dr/drTransport";
import { ensureDrWasm, VodozemacEngine } from "@/crypto-dr/vodozemacEngine";
import { MemoryDrSessionStore } from "@/crypto-dr/sessionStore";
import type { PeerPrekeyBundle } from "@/crypto-dr/types";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const dec = (b: Uint8Array): string => new TextDecoder().decode(b);
const flush = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

/// EN: Minimal loopback relay hub — `send` fans a frame out to every OTHER endpoint
/// (mirrors BroadcastChannel multi-tab semantics). CN: 最小环回 relay hub。
class LoopbackHub {
  private subs: Array<{ ref: string; cb: (f: RelayFrame) => void }> = [];
  endpoint(ref: string): RelayClient {
    const hub = this;
    return {
      async connect() {},
      async send(frame: RelayFrame) {
        for (const s of hub.subs) if (s.ref !== ref) s.cb(frame);
      },
      onMessage(cb) {
        hub.subs.push({ ref, cb });
      },
      async sendControl() {},
      onControl() {},
      disconnect() {},
    };
  }
}

let aliceAddr: string;
let bobAddr: string;
let charlieAddr: string;

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../dr-pkg/nexchat_dr_bg.wasm", import.meta.url));
  await ensureDrWasm(readFileSync(wasmPath));
  await cryptoWaitReady();
  const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
  aliceAddr = kr.addFromUri("//Alice").address;
  bobAddr = kr.addFromUri("//Bob").address;
  charlieAddr = kr.addFromUri("//Charlie").address;
});

/// Build a (locally-trusted) bundle straight from the peer engine — verification is
/// exercised in prekeyFetch.test.ts, so endorsements are stubbed here.
function bundleFrom(peer: VodozemacEngine, account: string): PeerPrekeyBundle {
  return {
    account,
    device: peer.deviceId(),
    ik: peer.identityKey(),
    ikEndorsement: new Uint8Array(64),
    spk: peer.rotateSignedPreKey(),
    spkEndorsement: new Uint8Array(64),
    opk: undefined,
    prekeyEpoch: 0n,
  };
}

describe("DrTransport — relay d: delivery (design §21)", () => {
  it("delivers a full X3DH + ratchet round trip over the relay", async () => {
    const hub = new LoopbackHub();
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();

    const aliceTx = new DrTransport(alice, hub.endpoint(aliceAddr), aliceAddr);
    const bobTx = new DrTransport(bob, hub.endpoint(bobAddr), bobAddr);
    const aliceInbox: DrIncoming[] = [];
    const bobInbox: DrIncoming[] = [];
    aliceTx.onMessage((m) => aliceInbox.push(m));
    bobTx.onMessage((m) => bobInbox.push(m));
    aliceTx.attach();
    bobTx.attach();

    await aliceTx.startSession(bundleFrom(bob, bobAddr));
    await aliceTx.sendTo(bobAddr, bob.deviceId(), enc("hello bob"));
    await flush();

    expect(bobInbox).toHaveLength(1);
    expect(dec(bobInbox[0]!.plaintext)).toBe("hello bob");
    expect(bobInbox[0]!.peerDevice).toEqual(alice.deviceId());
    expect(bobInbox[0]!.peerAccount).toBe(canonicalAddress(aliceAddr));

    await bobTx.sendTo(aliceAddr, bobInbox[0]!.peerDevice, enc("hi alice"));
    await flush();
    expect(dec(aliceInbox[0]!.plaintext)).toBe("hi alice");

    // A couple more rounds (ratchet advances both ways).
    await aliceTx.sendTo(bobAddr, bob.deviceId(), enc("msg2"));
    await flush();
    expect(dec(bobInbox[1]!.plaintext)).toBe("msg2");
    await bobTx.sendTo(aliceAddr, alice.deviceId(), enc("msg3"));
    await flush();
    expect(dec(aliceInbox[1]!.plaintext)).toBe("msg3");
  });

  it("only the addressed device processes a frame (multi-device routing §18.3)", async () => {
    const hub = new LoopbackHub();
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    const charlie = new VodozemacEngine();
    await alice.init();
    await bob.init();
    await charlie.init();

    const aliceTx = new DrTransport(alice, hub.endpoint(aliceAddr), aliceAddr);
    const bobTx = new DrTransport(bob, hub.endpoint(bobAddr), bobAddr);
    const charlieTx = new DrTransport(charlie, hub.endpoint(charlieAddr), charlieAddr);
    const bobInbox: DrIncoming[] = [];
    const charlieInbox: DrIncoming[] = [];
    bobTx.onMessage((m) => bobInbox.push(m));
    charlieTx.onMessage((m) => charlieInbox.push(m));
    bobTx.attach();
    charlieTx.attach();

    await aliceTx.startSession(bundleFrom(bob, bobAddr));
    await aliceTx.sendTo(bobAddr, bob.deviceId(), enc("for bob only"));
    await flush();

    expect(bobInbox).toHaveLength(1);
    expect(charlieInbox).toHaveLength(0); // recvDev != charlie → ignored
  });

  it("persists + restores ratchet state across a restart (restoreDrTransport)", async () => {
    const hub = new LoopbackHub();
    const storeA = new MemoryDrSessionStore();
    const storeB = new MemoryDrSessionStore();

    const a1 = await restoreDrTransport({ account: aliceAddr, relay: hub.endpoint(aliceAddr), store: storeA });
    const b1 = await restoreDrTransport({ account: bobAddr, relay: hub.endpoint(bobAddr), store: storeB });
    const bobInbox: DrIncoming[] = [];
    b1.transport.onMessage((m) => bobInbox.push(m));
    b1.transport.attach(); // only Bob receives in phase 1

    await a1.transport.startSession({
      account: bobAddr,
      device: b1.engine.deviceId(),
      ik: b1.engine.identityKey(),
      ikEndorsement: new Uint8Array(64),
      spk: b1.engine.rotateSignedPreKey(),
      spkEndorsement: new Uint8Array(64),
      opk: undefined,
      prekeyEpoch: 0n,
    });
    await a1.transport.sendTo(bobAddr, b1.engine.deviceId(), enc("hello"));
    await flush();
    expect(bobInbox).toHaveLength(1);

    // "Restart" Alice: rebuild engine + transport purely from the persisted store.
    const a2 = await restoreDrTransport({ account: aliceAddr, relay: hub.endpoint(aliceAddr), store: storeA });
    expect(a2.engine.deviceId()).toEqual(a1.engine.deviceId());
    expect(a2.engine.hasSession(b1.engine.deviceId())).toBe(true);
    const aliceInbox: DrIncoming[] = [];
    a2.transport.onMessage((m) => aliceInbox.push(m));
    a2.transport.attach();

    await b1.transport.sendTo(aliceAddr, bobInbox[0]!.peerDevice, enc("welcome back"));
    await flush();
    expect(dec(aliceInbox[0]!.plaintext)).toBe("welcome back");
  });

  it("ingestFrame returns true for a DR frame and false for a non-DR frame (dispatcher discriminator)", async () => {
    const hub = new LoopbackHub();
    const alice = new VodozemacEngine();
    const bob = new VodozemacEngine();
    await alice.init();
    await bob.init();

    // Bob does NOT attach() — an external dispatcher feeds frames via ingestFrame.
    const bobTx = new DrTransport(bob, hub.endpoint(bobAddr), bobAddr);
    const bobInbox: DrIncoming[] = [];
    bobTx.onMessage((m) => bobInbox.push(m));

    // Alice (separate transport) emits a real DR Init frame addressed to Bob.
    let captured: RelayFrame | null = null;
    const sink = hub.endpoint(bobAddr);
    sink.onMessage((f) => {
      captured = f;
    });
    const aliceTx = new DrTransport(alice, hub.endpoint(aliceAddr), aliceAddr);
    await aliceTx.startSession(bundleFrom(bob, bobAddr));
    await aliceTx.sendTo(bobAddr, bob.deviceId(), enc("dr hello"));
    await flush();
    expect(captured).not.toBeNull();

    const drHandled = await bobTx.ingestFrame(captured!);
    expect(drHandled).toBe(true);
    expect(dec(bobInbox[0]!.plaintext)).toBe("dr hello");

    // A non-DR frame on the same conv-id space is rejected (false → MLS fallthrough).
    const mlsLike = await bobTx.ingestFrame({
      convId: `d:${bobAddr}`,
      senderRef: aliceAddr,
      ciphertextB64: bytesToB64(new Uint8Array([2, 2, 2, 2])),
    });
    expect(mlsLike).toBe(false);
  });

  it("ignores non-DR frames on the same conv-id space", async () => {
    const hub = new LoopbackHub();
    const bob = new VodozemacEngine();
    await bob.init();
    const bobTx = new DrTransport(bob, hub.endpoint(bobAddr), bobAddr);
    const bobInbox: DrIncoming[] = [];
    bobTx.onMessage((m) => bobInbox.push(m));
    bobTx.attach();

    const sender = hub.endpoint(aliceAddr);
    await sender.send({
      convId: `d:${bobAddr}`,
      senderRef: aliceAddr,
      ciphertextB64: bytesToB64(new Uint8Array([1, 2, 3])), // not a DmEnvelope
    });
    await flush();
    expect(bobInbox).toHaveLength(0);
  });
});
