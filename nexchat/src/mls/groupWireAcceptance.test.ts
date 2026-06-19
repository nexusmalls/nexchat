// EN: G7 client acceptance matrix for group Wire-ification (CHAT_GROUP_WIREIFY_DESIGN §13). WASM-backed,
// end-to-end at the SESSION layer (`GroupWireSession` + real `OpenMlsEngine`s + real chain-ordering driver
// + real `createChainSubmitGroupCommit`) against a FAKE chain `expected_epoch` CAS and an in-memory relay
// bus. Proves the §13 client scenarios that retire the Track A "read-only device" model:
//   A. 多端并发群发 — two devices of one account each send at the same epoch; every member + the sibling
//      decrypt both, with independent per-device ratchets (no cross-device key/nonce reuse).
//   B. 换机/被 Add 续发 — a fresh device is grafted via the device-join cascade and immediately sends.
//   C. 移除设备自愈 — the CD removes a sibling device; the removed leaf can no longer read FUTURE messages
//      (per-device PCS) while the rest of the group stays converged.
//   D. 并发 commit 链上仲裁 — a concurrent winner forces EpochStale; the loser catches up, re-stages, and
//      the chain accepts the retry → eventual consistency.
//   E. 无主死锁不复现 — any device can originate a change: a non-CD device delegates to the elected CD, and
//      after the CD goes offline a sibling RE-ELECTS as CD and commits itself (no single point of send).
//
// CN: 群 Wire 化的 G7 客户端验收矩阵（设计 §13）。WASM 支撑，会话层端到端（`GroupWireSession` + 真实
// `OpenMlsEngine` + 真实链定序驱动 + 真实 `createChainSubmitGroupCommit`），对**假**链 `expected_epoch` CAS
// 与内存 relay 总线。证明 §13 中令轨 A「只读设备」模型退役的客户端场景（A 多端并发群发 / B 换机续发 /
// C 移除设备自愈 / D 并发 commit 链上仲裁 / E 无主死锁不复现）。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { hexToBytes } from "@/mls/chainBytes";
import type { GroupCommitChain } from "@/mls/chainSubmitGroupCommit";
import { deviceLeafIdentity } from "@/mls/directConv";
import { GroupWireSession } from "@/mls/groupWireSession";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import {
  type ControlInbound,
  type ControlMsg,
  type RelayClient,
  type RelayFrame,
  type RelayInbound,
} from "@/relay/relayClient";

const ADDR_ALICE = "5AliceAddr";
const GROUP = "g:7";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

class Bus {
  relays: BusRelay[] = [];
  publish(from: BusRelay, m: ControlMsg): void {
    for (const r of this.relays) if (r !== from) r.deliver(m);
  }
}

class BusRelay implements RelayClient {
  sent: ControlMsg[] = [];
  private handlers: ControlInbound[] = [];
  constructor(private bus: Bus) {
    bus.relays.push(this);
  }
  async connect(): Promise<void> {}
  disconnect(): void {}
  async send(_f: RelayFrame): Promise<void> {}
  onMessage(_cb: RelayInbound): void {}
  async sendControl(m: ControlMsg): Promise<void> {
    this.sent.push(m);
    this.bus.publish(this, m);
  }
  onControl(cb: ControlInbound): void {
    this.handlers.push(cb);
  }
  deliver(m: ControlMsg): void {
    for (const h of this.handlers) h(m);
  }
  onCommitReject(): void {}
  count(t: ControlMsg["t"]): number {
    return this.sent.filter((m) => m.t === t).length;
  }
}

// EN: a fake chain whose `commit` accepts ONLY at `expected_epoch` (block total order) and captures the
// accepted commit bytes so the test can converge the other members. CN: 假链：`commit` 仅在 `expected_epoch`
// 接受（区块全序），并捕获已接受 commit 字节供测试收敛其他成员。
function fakeChain(startEpoch: number) {
  let epoch = startEpoch;
  let accepted: Uint8Array | null = null;
  const chain: GroupCommitChain = {
    async signAndSendDev(section, method, args) {
      if (section !== "chatGroup" || method !== "commit") throw new Error(`unexpected ${section}.${method}`);
      const expectedEpoch = args[1] as number;
      if (expectedEpoch !== epoch) throw new Error("chatGroup.commit failed: chatGroup.EpochStale");
      accepted = hexToBytes(args[2] as string);
      epoch += 1;
      return "0xblock";
    },
    async groupSnapshot() {
      return { epoch, memberCount: 3 };
    },
  };
  return {
    chain,
    get epoch() {
      return epoch;
    },
    get accepted() {
      return accepted;
    },
  };
}

async function buildGroup() {
  const aliceA = new OpenMlsEngine();
  const aliceB = new OpenMlsEngine();
  const bob = new OpenMlsEngine();
  const carol = new OpenMlsEngine();
  await aliceA.init(deviceLeafIdentity(ADDR_ALICE, "a"));
  await aliceB.init(deviceLeafIdentity(ADDR_ALICE, "b"));
  await bob.init(deviceLeafIdentity("5BobAddrLong", "b1"));
  await carol.init(deviceLeafIdentity("5CarolAddrLong", "c1"));
  aliceA.createGroupByConv(GROUP);
  const add = aliceA.addMembersByConv(GROUP, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
  await bob.processWelcomeByConv(GROUP, add.welcome);
  await carol.processWelcomeByConv(GROUP, add.welcome);
  return { aliceA, aliceB, bob, carol };
}

function cdSession(deps: {
  engine: OpenMlsEngine;
  relay: RelayClient;
  chain: ReturnType<typeof fakeChain>;
  deviceId: string;
  endpointId: string;
}) {
  return new GroupWireSession({
    engine: deps.engine,
    relay: deps.relay,
    chain: deps.chain.chain,
    selfAddress: ADDR_ALICE,
    deviceId: deps.deviceId,
    endpointId: deps.endpointId,
    listJoinableGroups: () => deps.engine.listGroups().filter((k) => k.startsWith("g:")),
    syncGroupEpoch: async (conv, toEpoch) => {
      if (deps.chain.accepted && deps.engine.epochByConv(conv) < toEpoch) {
        deps.engine.processCommitByConv(conv, deps.chain.accepted);
      }
    },
  });
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("G7 group Wire acceptance (§13)", () => {
  it("B+A: 换机被 Add 续发 then 多端并发群发 — both devices send at one epoch, all decrypt both", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);
    const fc = fakeChain(epoch0);
    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);
    const sessionA = cdSession({ engine: aliceA, relay: relayA, chain: fc, deviceId: "a", endpointId: "ep-a" });
    const sessionB = new GroupWireSession({
      engine: aliceB,
      relay: relayB,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
    });
    sessionA.start();
    sessionB.start();

    // (B) 换机/被 Add：fresh device joins via the cascade, all leaves converge
    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(50);
    expect(aliceB.hasGroup(GROUP)).toBe(true);
    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    const epoch1 = aliceA.epochByConv(GROUP);
    expect([aliceB.epochByConv(GROUP), bob.epochByConv(GROUP), carol.epochByConv(GROUP)]).toEqual([
      epoch1, epoch1, epoch1,
    ]);

    // (A) 多端并发群发: A and B both send at the SAME epoch (application msgs don't bump the epoch)
    const ctA = await aliceA.encrypt(GROUP, textEnvelope("ma", "from device A", {}));
    const ctB = await aliceB.encrypt(GROUP, textEnvelope("mb", "from device B", {}));
    expect(new Uint8Array(ctA)).not.toEqual(new Uint8Array(ctB)); // independent per-device ratchets

    // every member + the sibling decrypt BOTH (no single-sender bottleneck, no key/nonce reuse)
    for (const peer of [bob, carol, aliceB]) {
      expect((await peer.decrypt(GROUP, ctA)).body).toMatchObject({ text: "from device A" });
    }
    for (const peer of [bob, carol, aliceA]) {
      expect((await peer.decrypt(GROUP, ctB)).body).toMatchObject({ text: "from device B" });
    }

    sessionA.stop();
    sessionB.stop();
  });

  it("C: 移除设备自愈 — CD removes a sibling; the removed device cannot read future messages (per-device PCS)", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const fc = fakeChain(aliceA.epochByConv(GROUP));
    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);
    const sessionA = cdSession({ engine: aliceA, relay: relayA, chain: fc, deviceId: "a", endpointId: "ep-a" });
    const sessionB = new GroupWireSession({
      engine: aliceB,
      relay: relayB,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
    });
    sessionA.start();
    sessionB.start();

    // sibling B joins
    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(50);
    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    expect(aliceB.hasGroup(GROUP)).toBe(true);

    // CD removes device B's leaf
    const targetB = deviceLeafIdentity(ADDR_ALICE, "b");
    const route = await sessionA.removeDevice(GROUP, targetB);
    expect(route).toBe("execute");
    await vi.advanceTimersByTimeAsync(50);
    const removeCommit = fc.accepted!;
    bob.processCommitByConv(GROUP, removeCommit);
    carol.processCommitByConv(GROUP, removeCommit);
    const epochAfterRemove = aliceA.epochByConv(GROUP);
    expect(bob.epochByConv(GROUP)).toBe(epochAfterRemove);

    // a message sent AFTER the removal: remaining members read it, the removed device CANNOT (PCS self-heal)
    const post = await aliceA.encrypt(GROUP, textEnvelope("post", "after removal", {}));
    expect((await bob.decrypt(GROUP, post)).body).toMatchObject({ text: "after removal" });
    expect((await carol.decrypt(GROUP, post)).body).toMatchObject({ text: "after removal" });
    await expect(aliceB.decrypt(GROUP, post)).rejects.toBeDefined();

    sessionA.stop();
    sessionB.stop();
  });

  it("D: 并发 commit 链上仲裁 — concurrent winner forces EpochStale; loser catches up, retries, converges", async () => {
    const { aliceA, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);

    // a concurrent winner (carol rekey) the chain accepts FIRST → chain is one epoch ahead of aliceA
    const winning = carol.selfUpdateStagedByConv(GROUP);
    carol.mergePendingByConv(GROUP);
    bob.processCommitByConv(GROUP, winning);
    const winnerEpoch = carol.epochByConv(GROUP); // epoch0 + 1
    const fc = fakeChain(winnerEpoch);

    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const sessionA = new GroupWireSession({
      engine: aliceA,
      relay: relayA,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "a",
      endpointId: "ep-a",
      listJoinableGroups: () => aliceA.listGroups().filter((k) => k.startsWith("g:")),
      syncGroupEpoch: async (conv, toEpoch) => {
        if (aliceA.epochByConv(conv) < toEpoch) aliceA.processCommitByConv(conv, winning);
      },
    });
    sessionA.start();

    const route = await sessionA.rekey(GROUP);
    expect(route).toBe("execute");
    await vi.advanceTimersByTimeAsync(50);

    // aliceA caught up to the winner, re-staged, chain accepted at winnerEpoch+1; everyone converges
    expect(fc.epoch).toBe(winnerEpoch + 1);
    expect(aliceA.epochByConv(GROUP)).toBe(winnerEpoch + 1);
    bob.processCommitByConv(GROUP, fc.accepted!);
    expect(bob.epochByConv(GROUP)).toBe(winnerEpoch + 1);
    expect(epoch0).toBeLessThan(aliceA.epochByConv(GROUP));

    sessionA.stop();
  });

  it("E: 无主死锁不复现 — a non-CD device delegates to the CD, then RE-ELECTS as CD after it goes offline", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const fc = fakeChain(aliceA.epochByConv(GROUP));
    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);
    const sessionA = cdSession({ engine: aliceA, relay: relayA, chain: fc, deviceId: "a", endpointId: "ep-a" });
    const sessionB = cdSession({ engine: aliceB, relay: relayB, chain: fc, deviceId: "b", endpointId: "ep-b" });
    sessionA.start();
    sessionB.start();
    // EN: both subscribed now → re-publish presence so EACH learns the other (a connect-order race
    // otherwise hides the device that published before its peer subscribed), then let the CD settle window
    // (CD_SETTLE_MS=2000) + ticker elapse so B settles to [a,b] and steps down (CD = smallest id "a").
    // CN: 双方均已订阅 → 重发 presence 使彼此都获知对方（否则先发后订阅会漏掉对端），再等 CD 静默窗
    // （CD_SETTLE_MS=2000）+ ticker 走完，使 B 安定到 [a,b] 并让位（CD = 最小 id "a"）。
    sessionA.onRelayConnected();
    sessionB.onRelayConnected();
    await vi.advanceTimersByTimeAsync(2600);
    expect(sessionA.isCoordinator()).toBe(true); // smallest deviceId → CD
    expect(sessionB.isCoordinator()).toBe(false);

    // sibling B joins so it holds the group too
    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(50);
    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    const e1 = aliceA.epochByConv(GROUP);

    // the NON-CD device originates a rekey → it is DELEGATED to the elected CD, which commits via the chain
    const route = await sessionB.rekey(GROUP);
    expect(route).toBe("delegated");
    await vi.advanceTimersByTimeAsync(50);
    expect(fc.epoch).toBe(e1 + 1); // the CD committed on B's behalf — no primary bottleneck
    aliceB.processCommitByConv(GROUP, fc.accepted!);
    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    const e2 = aliceA.epochByConv(GROUP);

    // the CD goes offline → B RE-ELECTS as CD and can commit ITSELF (single point of send eliminated)
    sessionA.stop();
    await vi.advanceTimersByTimeAsync(2600); // re-settle to the [b]-only online set
    expect(sessionB.isCoordinator()).toBe(true);
    const route2 = await sessionB.rekey(GROUP);
    expect(route2).toBe("execute");
    await vi.advanceTimersByTimeAsync(50);
    expect(fc.epoch).toBe(e2 + 1);
    expect(aliceB.epochByConv(GROUP)).toBe(e2 + 1);
    bob.processCommitByConv(GROUP, fc.accepted!);
    const ct = await aliceB.encrypt(GROUP, textEnvelope("alive", "B still sends with no primary", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "B still sends with no primary" });

    sessionB.stop();
  });
});
