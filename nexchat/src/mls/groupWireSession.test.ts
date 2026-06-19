// EN: WASM-backed end-to-end test for the group Wire-ification session orchestrator
// (CHAT_GROUP_WIREIFY_DESIGN §6/§7/§15.3, G3c). Two `GroupWireSession`s (one CD device that holds a
// >=3-account group, one fresh same-account device) run over a shared in-memory control bus + REAL
// `OpenMlsEngine`s + the REAL `createChainSubmitGroupCommit` wiring, against a FAKE chain `expected_epoch`
// CAS. Proves the full device-join cascade for a GROUP: announce → CD offers its groups (scope=group) →
// new device returns a KeyPackage → CD runs `add_device` through the chain-ordering driver → chain CAS
// accepts → merge → Welcome fanned over `s:<account>` → the new device joins and every leaf converges.
// Also covers the CD `rekey`/`removeDevice` public ops and the EpochStale catch-up retry over the live
// chain submit path.
//
// CN: 群 Wire 化会话编排器的 WASM 端到端测试（设计 §6/§7/§15.3，G3c）。两个 `GroupWireSession`（一个持
// ≥3 账户群的 CD 设备、一个同账户新设备）跑在共享内存控制总线 + 真实 `OpenMlsEngine` + 真实
// `createChainSubmitGroupCommit` 接线上，对**假**链 `expected_epoch` CAS。证明**群**的完整设备 join 级联：
// 广播 → CD 提供其群（scope=group）→ 新设备返回 KeyPackage → CD 经链定序驱动跑 `add_device` → 链 CAS 接受 →
// 合并 → Welcome 经 `s:<account>` 扇出 → 新设备加入且全 leaf 收敛。并覆盖 CD `rekey`/`removeDevice` 公开操作
// 与实链提交路径上的 EpochStale 追平重试。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { hexToBytes } from "@/mls/chainBytes";
import type { GroupCommitChain } from "@/mls/chainSubmitGroupCommit";
import { leafKeyBindingBytes } from "@/mls/deviceLeafCredential";
import { deviceLeafIdentity } from "@/mls/directConv";
import { GroupWireSession } from "@/mls/groupWireSession";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import { canonicalAddress } from "@/wallet/address";
import {
  type CommitRejectInbound,
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
  // EN: authenticated account this connection writes as (relay stamps it on peer_add_req, §8.4). CN:
  // 本连接认证写入的账户（relay 在 peer_add_req 上盖章，§8.4）。
  constructor(
    private bus: Bus,
    private account?: string,
  ) {
    bus.relays.push(this);
  }
  async connect(): Promise<void> {}
  disconnect(): void {}
  async send(_f: RelayFrame): Promise<void> {}
  onMessage(_cb: RelayInbound): void {}
  async sendControl(m: ControlMsg): Promise<void> {
    // EN: emulate the relay stamping the AUTHENTICATED sender account on peer-add. CN: 模拟 relay 在
    // peer-add 上盖章**认证**发送者账户。
    const out =
      m.t === "peer_add_req" && this.account ? { ...m, _senderAccount: this.account } : m;
    this.sent.push(out);
    this.bus.publish(this, out);
  }
  onControl(cb: ControlInbound): void {
    this.handlers.push(cb);
  }
  deliver(m: ControlMsg): void {
    for (const h of this.handlers) h(m);
  }
  onCommitReject(_cb: CommitRejectInbound): void {}
  count(t: ControlMsg["t"]): number {
    return this.sent.filter((m) => m.t === t).length;
  }
}

// EN: A fake chain whose `commit` accepts ONLY at `expected_epoch` (block total order) and captures the
// accepted commit bytes so the test can converge the other members (they would normally pull it from the
// chain handshake log). CN: 假链：`commit` 仅在 `expected_epoch` 接受（区块全序），并捕获已接受 commit 字节，
// 供测试收敛其他成员（生产中他们从链握手日志拉取）。
function fakeChain(startEpoch: number) {
  let epoch = startEpoch;
  let accepted: Uint8Array | null = null;
  const chain: GroupCommitChain = {
    async signAndSendDev(section, method, args) {
      if (section !== "chatGroup" || method !== "commit") throw new Error(`unexpected ${section}.${method}`);
      const expectedEpoch = args[1] as number;
      const commitHex = args[2] as string;
      if (expectedEpoch !== epoch) {
        throw new Error("chatGroup.commit failed: chatGroup.EpochStale");
      }
      accepted = hexToBytes(commitHex);
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
    set epoch(v: number) {
      epoch = v;
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

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("GroupWireSession (chain-ordered group Wire device ops)", () => {
  it("device-join cascade: announce → offer → kp → chain add_device → merge → Welcome → join", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);
    const fc = fakeChain(epoch0);

    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);

    const sessionA = new GroupWireSession({
      engine: aliceA,
      relay: relayA,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "a", // smallest id → CD
      endpointId: "ep-a",
      listJoinableGroups: () => aliceA.listGroups().filter((k) => k.startsWith("g:")),
      syncGroupEpoch: async (conv, toEpoch) => {
        if (fc.accepted && aliceA.epochByConv(conv) < toEpoch) aliceA.processCommitByConv(conv, fc.accepted);
      },
    });
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
    expect(sessionA.isCoordinator()).toBe(true);

    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(50); // flush request → offer → kp → add → chain commit → welcome

    // CD offered the group, the new device returned exactly one KeyPackage
    expect(relayA.count("device_join_offer")).toBe(1);
    expect(relayB.count("device_join_kp")).toBe(1);

    // chain accepted exactly one commit; the new device joined; the committer merged one epoch up
    expect(fc.epoch).toBe(epoch0 + 1);
    expect(aliceB.hasGroup(GROUP)).toBe(true);
    expect(aliceA.epochByConv(GROUP)).toBe(epoch0 + 1);
    expect(aliceB.epochByConv(GROUP)).toBe(epoch0 + 1);

    // the rest of the group converges on the accepted commit (chain catch-up)
    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    expect(bob.epochByConv(GROUP)).toBe(epoch0 + 1);
    expect(carol.epochByConv(GROUP)).toBe(epoch0 + 1);

    // cross-decrypt: the new device B ↔ bob at the new epoch
    const fromB = await aliceB.encrypt(GROUP, textEnvelope("m-b", "joined from device B", {}));
    expect((await bob.decrypt(GROUP, fromB)).body).toMatchObject({ text: "joined from device B" });

    sessionA.stop();
    sessionB.stop();
  });

  it("§8.1 lazy Add: a DORMANT offered group is deferred (no commit) until activateGroup grafts it", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);
    const fc = fakeChain(epoch0);

    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);

    const sessionA = new GroupWireSession({
      engine: aliceA,
      relay: relayA,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "a",
      endpointId: "ep-a",
      listJoinableGroups: () => aliceA.listGroups().filter((k) => k.startsWith("g:")),
      syncGroupEpoch: async (conv, toEpoch) => {
        if (fc.accepted && aliceA.epochByConv(conv) < toEpoch) aliceA.processCommitByConv(conv, fc.accepted);
      },
    });
    // the new device treats the group as DORMANT → it should be deferred, not joined now
    let active = false;
    const planned: Array<{ joinNow: string[]; defer: string[] }> = [];
    const sessionB = new GroupWireSession({
      engine: aliceB,
      relay: relayB,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
      isGroupActive: () => active,
      onJoinPlanned: (p) => planned.push({ joinNow: [...p.joinNow], defer: [...p.defer] }),
    });
    sessionA.start();
    sessionB.start();

    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(50);

    // the offer was deferred: CD offered, but the new device sent NO KeyPackage and the chain never moved
    expect(relayA.count("device_join_offer")).toBe(1);
    expect(planned).toEqual([{ joinNow: [], defer: [GROUP] }]);
    expect(relayB.count("device_join_kp")).toBe(0);
    expect(fc.epoch).toBe(epoch0);
    expect(aliceB.hasGroup(GROUP)).toBe(false);

    // group becomes active (opened) → on-demand graft fires for just this group
    active = true;
    await sessionB.activateGroup(GROUP);
    await vi.advanceTimersByTimeAsync(50);

    expect(relayB.count("device_join_kp")).toBe(1);
    expect(fc.epoch).toBe(epoch0 + 1);
    expect(aliceB.hasGroup(GROUP)).toBe(true);
    expect(aliceB.epochByConv(GROUP)).toBe(epoch0 + 1);

    // a second activateGroup is a no-op (already held, no longer deferred)
    await sessionB.activateGroup(GROUP);
    await vi.advanceTimersByTimeAsync(50);
    expect(relayB.count("device_join_kp")).toBe(1);

    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    const fromB = await aliceB.encrypt(GROUP, textEnvelope("lz", "joined lazily on activate", {}));
    expect((await bob.decrypt(GROUP, fromB)).body).toMatchObject({ text: "joined lazily on activate" });

    sessionA.stop();
    sessionB.stop();
  });

  it("CD rekey: public op runs through the chain CAS and converges followers", async () => {
    const { aliceA, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);
    const fc = fakeChain(epoch0);

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
    });
    sessionA.start();
    expect(sessionA.isCoordinator()).toBe(true);

    const route = await sessionA.rekey(GROUP);
    expect(route).toBe("execute");
    await vi.advanceTimersByTimeAsync(50);

    expect(fc.epoch).toBe(epoch0 + 1);
    expect(aliceA.epochByConv(GROUP)).toBe(epoch0 + 1);

    bob.processCommitByConv(GROUP, fc.accepted!);
    carol.processCommitByConv(GROUP, fc.accepted!);
    const ct = await aliceA.encrypt(GROUP, textEnvelope("rk", "rekeyed via chain CAS", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "rekeyed via chain CAS" });

    sessionA.stop();
  });

  it("EpochStale: a concurrent winner forces catch-up + re-stage over the live chain submit, then accepts", async () => {
    const { aliceA, bob, carol } = await buildGroup();
    const epoch0 = aliceA.epochByConv(GROUP);

    // a concurrent winner (carol rekey) the chain accepts FIRST → chain is one epoch ahead of aliceA
    const winning = carol.selfUpdateStagedByConv(GROUP);
    carol.mergePendingByConv(GROUP);
    bob.processCommitByConv(GROUP, winning);
    const winnerEpoch = carol.epochByConv(GROUP); // epoch0 + 1

    const fc = fakeChain(winnerEpoch); // chain already advanced past aliceA's pre-op epoch

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
      // catch-up applies the winning chain commit so aliceA reaches the winner epoch, then re-stages
      syncGroupEpoch: async (conv, toEpoch) => {
        if (aliceA.epochByConv(conv) < toEpoch) aliceA.processCommitByConv(conv, winning);
      },
    });
    sessionA.start();

    const route = await sessionA.rekey(GROUP);
    expect(route).toBe("execute");
    await vi.advanceTimersByTimeAsync(50);

    // aliceA caught up to the winner, re-staged, and the chain accepted at winnerEpoch+1
    expect(fc.epoch).toBe(winnerEpoch + 1);
    expect(aliceA.epochByConv(GROUP)).toBe(winnerEpoch + 1);

    bob.processCommitByConv(GROUP, fc.accepted!);
    expect(bob.epochByConv(GROUP)).toBe(winnerEpoch + 1);
    expect(epoch0).toBeLessThan(aliceA.epochByConv(GROUP));

    sessionA.stop();
  });
});

// EN: Group peer-add-device fallback (CHAT_GROUP_WIREIFY_DESIGN §8.4, G5). When a fresh device of an
// EXISTING member has NO sibling/CD online to graft it, another member grafts it over `peer_add_req`,
// authorizing relay-/chain-trustlessly: relay-stamped sender == requester, requester is already a member,
// and the KeyPackage carries a VALID in-MLS E2EI binding signed by the requester's account key. Uses REAL
// SS58 keys + REAL timers (binding verification is async crypto). CN: 群 peer-add 设备兜底（设计 §8.4，
// G5）。既有成员的新设备无在线兄弟/CD 嫁接时，另一成员经 `peer_add_req` 代为嫁接，relay-/链-trustless 鉴权：
// relay 盖章发送者 == 请求方、请求方已是成员、KeyPackage 携带由请求方账户钥签名的**有效** MLS 内 E2EI 绑定。
// 用**真实** SS58 钥 + **真实**定时器（绑定校验是异步密码学）。
describe("GroupWireSession peer-add fallback (§8.4)", () => {
  const G = "g:42";

  async function setup() {
    await cryptoWaitReady();
    const keyring = new Keyring({ type: "sr25519" });
    const alicePair = keyring.addFromUri(mnemonicGenerate());
    const bobPair = keyring.addFromUri(mnemonicGenerate());
    const carolPair = keyring.addFromUri(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const carolAddr = canonicalAddress(carolPair.address);

    const bind = (engine: OpenMlsEngine, pair: KeyringPair, account: string, device: string) => {
      engine.setLeafBinding(
        pair.sign(leafKeyBindingBytes(account, device, engine.signaturePublicKey())),
      );
    };

    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    const carol = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
    await aliceB.init(deviceLeafIdentity(aliceAddr, "b"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));
    await carol.init(deviceLeafIdentity(carolAddr, "c1"));
    bind(aliceA, alicePair, aliceAddr, "a");
    bind(aliceB, alicePair, aliceAddr, "b");
    bind(bob, bobPair, bobAddr, "b1");
    bind(carol, carolPair, carolAddr, "c1");

    aliceA.createGroupByConv(G);
    const add = aliceA.addMembersByConv(G, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
    await bob.processWelcomeByConv(G, add.welcome);
    await carol.processWelcomeByConv(G, add.welcome);

    const members = new Set([aliceAddr, bobAddr, carolAddr]);
    const fc = fakeChain(aliceA.epochByConv(G));

    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    const relayAliceB = new BusRelay(bus, aliceAddr);

    // bob is the only device of its account → CD; it grafts the requester via the chain ordering driver.
    const sessionBob = new GroupWireSession({
      engine: bob,
      relay: relayBob,
      chain: fc.chain,
      selfAddress: bobAddr,
      deviceId: "b1",
      endpointId: "ep-bob",
      listJoinableGroups: () => bob.listGroups().filter((k) => k.startsWith("g:")),
      isGroupMember: (_conv, acct) => members.has(canonicalAddress(acct)),
      syncGroupEpoch: async (conv, toEpoch) => {
        if (fc.accepted && bob.epochByConv(conv) < toEpoch) bob.processCommitByConv(conv, fc.accepted);
      },
    });
    // alice's NEW device: no sibling/CD online; it only requests a peer-add and consumes the Welcome.
    const sessionAliceB = new GroupWireSession({
      engine: aliceB,
      relay: relayAliceB,
      chain: fc.chain,
      selfAddress: aliceAddr,
      deviceId: "b",
      endpointId: "ep-alice-b",
    });
    return {
      aliceA, aliceB, bob, carol, alicePair, bobPair, carolPair,
      aliceAddr, bobAddr, carolAddr, fc, bus, relayBob, relayAliceB, sessionBob, sessionAliceB, bind,
    };
  }

  it("a member grafts a new device of an EXISTING member: verify → chain add_device → Welcome → join", async () => {
    vi.useRealTimers();
    const s = await setup();
    const epoch0 = s.aliceA.epochByConv(G);
    s.sessionBob.start();
    s.sessionAliceB.start();

    await s.sessionAliceB.requestGroupPeerAdd(G);
    await new Promise((r) => setTimeout(r, 300)); // flush verify → submit → chain → merge → welcome

    expect(s.relayAliceB.count("peer_add_req")).toBe(1);
    expect(s.fc.epoch).toBe(epoch0 + 1); // bob committed exactly once
    expect(s.aliceB.hasGroup(G)).toBe(true); // requester consumed the Welcome
    expect(s.aliceB.epochByConv(G)).toBe(epoch0 + 1);

    // the rest of the group converges on the accepted commit
    s.aliceA.processCommitByConv(G, s.fc.accepted!);
    s.carol.processCommitByConv(G, s.fc.accepted!);
    const ct = await s.aliceB.encrypt(G, textEnvelope("pa", "joined via peer-add", {}));
    expect((await s.carol.decrypt(G, ct)).body).toMatchObject({ text: "joined via peer-add" });

    s.sessionBob.stop();
    s.sessionAliceB.stop();
  });

  it("rejects peer-add from a NON-member account (group authz, gate d)", async () => {
    vi.useRealTimers();
    const s = await setup();
    const epoch0 = s.aliceA.epochByConv(G);
    s.sessionBob.start();

    // mallory is NOT a member; her device carries a perfectly valid in-MLS binding for her own account.
    const keyring = new Keyring({ type: "sr25519" });
    const malloryPair = keyring.addFromUri(mnemonicGenerate());
    const malloryAddr = canonicalAddress(malloryPair.address);
    const mallory = new OpenMlsEngine();
    await mallory.init(deviceLeafIdentity(malloryAddr, "m"));
    s.bind(mallory, malloryPair, malloryAddr, "m");
    const relayMallory = new BusRelay(s.bus, malloryAddr);
    const sessionMallory = new GroupWireSession({
      engine: mallory,
      relay: relayMallory,
      chain: s.fc.chain,
      selfAddress: malloryAddr,
      deviceId: "m",
      endpointId: "ep-mallory",
    });
    sessionMallory.start();

    await sessionMallory.requestGroupPeerAdd(G);
    await new Promise((r) => setTimeout(r, 300));

    expect(s.fc.epoch).toBe(epoch0); // bob refused → no commit
    expect(mallory.hasGroup(G)).toBe(false);

    s.sessionBob.stop();
    sessionMallory.stop();
  });

  it("rejects peer-add when the KeyPackage's E2EI binding is forged (gate e)", async () => {
    vi.useRealTimers();
    const s = await setup();
    const epoch0 = s.aliceA.epochByConv(G);
    s.sessionBob.start();

    // a fresh alice device whose binding is signed by an ATTACKER key, not alice's → bob must reject.
    const keyring = new Keyring({ type: "sr25519" });
    const attackerPair = keyring.addFromUri(mnemonicGenerate());
    const aliceEvil = new OpenMlsEngine();
    await aliceEvil.init(deviceLeafIdentity(s.aliceAddr, "evil"));
    aliceEvil.setLeafBinding(
      attackerPair.sign(leafKeyBindingBytes(s.aliceAddr, "evil", aliceEvil.signaturePublicKey())),
    );
    const relayAliceEvil = new BusRelay(s.bus, s.aliceAddr);
    const sessionAliceEvil = new GroupWireSession({
      engine: aliceEvil,
      relay: relayAliceEvil,
      chain: s.fc.chain,
      selfAddress: s.aliceAddr,
      deviceId: "evil",
      endpointId: "ep-alice-evil",
    });
    sessionAliceEvil.start();

    await sessionAliceEvil.requestGroupPeerAdd(G);
    await new Promise((r) => setTimeout(r, 300));

    expect(s.fc.epoch).toBe(epoch0); // bob refused: binding not signed by alice's account key
    expect(aliceEvil.hasGroup(G)).toBe(false);

    s.sessionBob.stop();
    sessionAliceEvil.stop();
  });

  it("onPeerAddTimeout fires when no member grafts within the fallback window", async () => {
    vi.useFakeTimers();
    const s = await setup();
    const onPeerAddTimeout = vi.fn();
    const sessionAliceB = new GroupWireSession({
      engine: s.aliceB,
      relay: s.relayAliceB,
      chain: s.fc.chain,
      selfAddress: s.aliceAddr,
      deviceId: "b",
      endpointId: "ep-alice-b",
      peerAddFallbackMs: 100,
      onPeerAddTimeout,
    });
    sessionAliceB.start();
    await sessionAliceB.requestGroupPeerAdd(G);
    expect(onPeerAddTimeout).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(100);
    expect(onPeerAddTimeout).toHaveBeenCalledWith(G);
    expect(s.aliceB.hasGroup(G)).toBe(false);
    sessionAliceB.stop();
  });

  it("ensureGraftOrPeerAdd falls back to peer_add_req when the group was not deferred from a CD offer", async () => {
    vi.useFakeTimers();
    const { aliceB } = await buildGroup();
    const bus = new Bus();
    const relayB = new BusRelay(bus, ADDR_ALICE);
    const fc = fakeChain(1);
    const sessionB = new GroupWireSession({
      engine: aliceB,
      relay: relayB,
      chain: fc.chain,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
    });
    sessionB.start();
    await sessionB.ensureGraftOrPeerAdd(GROUP);
    expect(relayB.count("peer_add_req")).toBe(1);
    await sessionB.ensureGraftOrPeerAdd(GROUP);
    expect(relayB.count("peer_add_req")).toBe(1);
    sessionB.stop();
  });
});
