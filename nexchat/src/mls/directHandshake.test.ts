import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { directMlsKey } from "@/mls/directConv";
import { DirectMlsCoordinator } from "@/mls/directHandshake";
import { DirectMlsRegistry } from "@/mls/directMlsRegistry";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import {
  bytesToB64,
  type ControlInbound,
  ControlMsg,
  RelayClient,
  RelayFrame,
  RelayInbound,
} from "@/relay/relayClient";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const ADDR_ALICE = "5AliceAddr";
const ADDR_BOB = "5BobAddrLong";

class TestHub {
  private clients: { id: string; ctrl?: ControlInbound; msg?: RelayInbound }[] = [];
  client(id: string): RelayClient {
    const entry: { id: string; ctrl?: ControlInbound; msg?: RelayInbound } = { id };
    this.clients.push(entry);
    return {
      connect: async () => {},
      disconnect: () => {},
      send: async (frame: RelayFrame) => {
        for (const c of this.clients) if (c.id !== id) c.msg?.(frame);
      },
      sendControl: async (m: ControlMsg) => {
        for (const c of this.clients) if (c.id !== id) c.ctrl?.(m);
      },
      onMessage: (cb: RelayInbound) => {
        entry.msg = cb;
      },
      onControl: (cb: ControlInbound) => {
        entry.ctrl = cb;
      },
    };
  }
}

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe("DirectMlsCoordinator", () => {
  it("pairwise handshake → real MLS message round-trip", async () => {
    const hub = new TestHub();
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    const aliceEngine = new OpenMlsEngine();
    const bobEngine = new OpenMlsEngine();
    await aliceEngine.init("alice");
    await bobEngine.init("bob");

    const aliceRelay = hub.client("ep-alice");
    const bobRelay = hub.client("ep-bob");

    const regA = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      onPeerStatus: () => {},
    });
    const regB = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA.wire();
    regB.wire();
    regA.ensure(ADDR_BOB);
    regB.ensure(ADDR_ALICE);

    await sleep(800);

    expect(aliceEngine.hasGroup(mlsKey)).toBe(true);
    expect(bobEngine.hasGroup(mlsKey)).toBe(true);
    expect(regA.isReady(ADDR_BOB)).toBe(true);
    expect(regB.isReady(ADDR_ALICE)).toBe(true);

    const uiAlice = `d:${ADDR_BOB}`;
    const uiBob = `d:${ADDR_ALICE}`;
    const ct = await aliceEngine.encrypt(mlsKey, textEnvelope("m1", "hi bob", {}));
    const env = await bobEngine.decrypt(mlsKey, ct);
    expect((env.body as { text: string }).text).toBe("hi bob");

    const ct2 = await bobEngine.encrypt(mlsKey, textEnvelope("m2", "hi alice", {}));
    const env2 = await aliceEngine.decrypt(mlsKey, ct2);
    expect((env2.body as { text: string }).text).toBe("hi alice");

    void uiAlice;
    void uiBob;
  });

  it("owner re-handshakes when peer reinstalls and sends a fresh kp", async () => {
    const hub = new TestHub();
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    const aliceEngine = new OpenMlsEngine();
    const bobEngine = new OpenMlsEngine();
    await aliceEngine.init("alice");
    await bobEngine.init("bob");

    const aliceRelay = hub.client("ep-alice");
    const bobRelay = hub.client("ep-bob");

    const regA = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      onPeerStatus: () => {},
    });
    const regB = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA.wire();
    regB.wire();
    regA.ensure(ADDR_BOB);
    regB.ensure(ADDR_ALICE);
    await sleep(800);
    expect(regA.isReady(ADDR_BOB)).toBe(true);

    bobEngine.forgetGroupByConv(mlsKey);
    const bobFresh = new OpenMlsEngine();
    await bobFresh.init("bob-reinstall");
    const regB2 = new DirectMlsRegistry({
      engine: bobFresh,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regB2.wire();
    regB2.ensure(ADDR_ALICE);
    await sleep(1200);

    expect(aliceEngine.hasGroup(mlsKey)).toBe(true);
    expect(bobFresh.hasGroup(mlsKey)).toBe(true);
    expect(regA.isReady(ADDR_BOB)).toBe(true);
    expect(regB2.isReady(ADDR_ALICE)).toBe(true);

    const ct = await aliceEngine.encrypt(mlsKey, textEnvelope("m3", "after reinstall", {}));
    const env = await bobFresh.decrypt(mlsKey, ct);
    expect((env.body as { text: string }).text).toBe("after reinstall");
  });

  it("member re-handshakes on start when a stale group was restored from persistence", async () => {
    const hub = new TestHub();
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    const aliceEngine = new OpenMlsEngine();
    const bobEngine = new OpenMlsEngine();
    await aliceEngine.init("alice");
    await bobEngine.init("bob");

    const aliceRelay = hub.client("ep-alice");
    const bobRelay = hub.client("ep-bob");

    const regA = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      onPeerStatus: () => {},
    });
    const regB = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA.wire();
    regB.wire();
    regA.ensure(ADDR_BOB);
    regB.ensure(ADDR_ALICE);
    await sleep(800);
    expect(regA.isReady(ADDR_BOB)).toBe(true);

    aliceEngine.forgetGroupByConv(mlsKey);
    aliceEngine.createGroupByConv(mlsKey);
    expect(aliceEngine.hasGroup(mlsKey)).toBe(true);
    expect(bobEngine.hasGroup(mlsKey)).toBe(true);

    const regA2 = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      onPeerStatus: () => {},
    });
    const regB2 = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA2.wire();
    regB2.wire();
    regA2.ensure(ADDR_BOB);
    regB2.ensure(ADDR_ALICE);
    await sleep(1200);

    expect(regA2.isReady(ADDR_BOB)).toBe(true);
    expect(regB2.isReady(ADDR_ALICE)).toBe(true);

    const ct = await aliceEngine.encrypt(mlsKey, textEnvelope("m4", "resync", {}));
    const env = await bobEngine.decrypt(mlsKey, ct);
    expect((env.body as { text: string }).text).toBe("resync");
  });

  it("owner restore with epoch≥1 waits for member kp before ready", async () => {
    const hub = new TestHub();
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    const aliceEngine = new OpenMlsEngine();
    const bobEngine = new OpenMlsEngine();
    await aliceEngine.init("alice");
    await bobEngine.init("bob");

    const aliceRelay = hub.client("ep-alice");
    const bobRelay = hub.client("ep-bob");

    const chain = {
      keyPackagesOf: async () => [bobEngine.generateKeyPackage()],
    } satisfies Pick<import("@/chain/chainClient").ChainClient, "keyPackagesOf">;

    const regA = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      chain,
      onPeerStatus: () => {},
    });
    const regB = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA.wire();
    regB.wire();
    regA.ensure(ADDR_BOB);
    regB.ensure(ADDR_ALICE);
    await sleep(800);
    expect(aliceEngine.epochByConv(mlsKey)).toBeGreaterThanOrEqual(1);

    const regA2 = new DirectMlsRegistry({
      engine: aliceEngine,
      relay: aliceRelay,
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      chain,
      onPeerStatus: () => {},
    });
    const regB2 = new DirectMlsRegistry({
      engine: bobEngine,
      relay: bobRelay,
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB,
      onPeerStatus: () => {},
    });
    regA2.wire();
    regB2.wire();
    regA2.ensure(ADDR_BOB);
    expect(regA2.isReady(ADDR_BOB)).toBe(false);

    regB2.ensure(ADDR_ALICE);
    await sleep(1200);
    expect(regA2.isReady(ADDR_BOB)).toBe(true);
    expect(regB2.isReady(ADDR_ALICE)).toBe(true);

    const ct = await aliceEngine.encrypt(mlsKey, textEnvelope("m5", "after restore", {}));
    const env = await bobEngine.decrypt(mlsKey, ct);
    expect((env.body as { text: string }).text).toBe("after restore");
  });

  it("owner does not reset group when peer's second device kp arrives during welcome round", async () => {
    const hub = new TestHub();
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    const aliceEngine = new OpenMlsEngine();
    const bobEngine1 = new OpenMlsEngine();
    const bobEngine2 = new OpenMlsEngine();
    await aliceEngine.init("alice");
    await bobEngine1.init("bob-d1");
    await bobEngine2.init("bob-d2");

    let status: { ready: boolean; role: "owner" | "member" } = { ready: false, role: "member" };
    const coord = new DirectMlsCoordinator({
      engine: aliceEngine,
      relay: hub.client("ep-alice"),
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
      peerAddress: ADDR_BOB,
      onStatus: (s) => {
        status = s;
      },
    });
    coord.start();

    const kp1 = bytesToB64(bobEngine1.generateKeyPackage());
    const kp2 = bytesToB64(bobEngine2.generateKeyPackage());
    expect(kp1).not.toBe(kp2);

    coord.handleControl({
      t: "kp",
      from: "ep-bob1",
      identity: ADDR_BOB,
      convId: mlsKey,
      kp: kp1,
    });
    await sleep(200);
    coord.handleControl({
      t: "kp",
      from: "ep-bob2",
      identity: ADDR_BOB,
      convId: mlsKey,
      kp: kp2,
    });
    await sleep(200);

    expect(aliceEngine.hasGroup(mlsKey)).toBe(true);
    expect(aliceEngine.epochByConv(mlsKey)).toBeGreaterThanOrEqual(1);
    expect(status.ready).toBe(false);
  });

  it("owner is lexicographically smaller address", async () => {
    const hub = new TestHub();
    const engine = new OpenMlsEngine();
    await engine.init("x");
    let status: { ready: boolean; role: "owner" | "member" } = { ready: false, role: "member" };
    const coord = new DirectMlsCoordinator({
      engine,
      relay: hub.client("ep"),
      endpointId: "ep",
      selfAddress: ADDR_BOB,
      peerAddress: ADDR_ALICE,
      onStatus: (s) => {
        status = s;
      },
    });
    hub.client("ep").onControl((m) => coord.handleControl(m));
    coord.start();
    await sleep(100);
    expect(status.role).toBe("member");
  });
});
