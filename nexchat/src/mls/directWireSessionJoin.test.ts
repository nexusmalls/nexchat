// EN: WASM-backed end-to-end test for the Wire 1:1 multi-device JOIN trigger
// (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.7): a fresh same-account device announces over the
// account self-channel, the elected CD offers its existing 1:1 convs, the new device mints a
// KeyPackage per conv, and the CD grafts it via the serialized `add_device` path. The new device
// consumes the resulting Welcome and converges with the existing leaves. Two `DirectWireSession`s
// run over a shared in-memory control bus + real `OpenMlsEngine`s.
// CN: Wire 1:1 多设备**加入触发**的 WASM 端到端测试（规范 §3.7）：同账户新设备经账户自通道广播，当选
// CD 提供其已有 1:1 会话集，新设备每会话造一个 KeyPackage，CD 经串行化 `add_device` 嫁接之。新设备消费
// 随之而来的 Welcome 并与既有 leaf 收敛。两个 `DirectWireSession` 跑在共享内存控制总线 + 真实
// `OpenMlsEngine` 上。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { canonicalAddress } from "@/wallet/address";
import { leafKeyBindingBytes } from "@/mls/deviceLeafCredential";
import { deviceLeafIdentity, directMlsKey } from "@/mls/directConv";
import { verifyIncomingCommit } from "@/mls/followCommitGuard";
import { DirectWireSession } from "@/mls/directWireSession";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import {
  b64ToBytes,
  bytesToB64,
  type CommitRejectInbound,
  type ControlInbound,
  type ControlMsg,
  type RelayClient,
  type RelayFrame,
  type RelayInbound,
} from "@/relay/relayClient";

const ADDR_ALICE = "5AliceAddr";
const ADDR_BOB = "5BobAddrLong";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

/// EN: A control bus that fans every `sendControl` to all OTHER attached relays (no self-echo, like
/// a multi-tab BroadcastChannel). CN: 把每次 `sendControl` 扇出给所有**其他**已接入 relay 的控制总线
/// （不回显自身，类似多标签 BroadcastChannel）。
class Bus {
  relays: BusRelay[] = [];
  publish(from: BusRelay, m: ControlMsg): void {
    for (const r of this.relays) if (r !== from) r.deliver(m);
  }
}

class BusRelay implements RelayClient {
  sent: ControlMsg[] = [];
  private handlers: ControlInbound[] = [];
  // EN: authenticated account this connection writes as (relay stamps it on peer_add_req). CN: 本连接
  // 认证写入的账户（relay 在 peer_add_req 上盖章）。
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
    // EN: emulate the relay stamping the AUTHENTICATED sender account on peer-assisted Add. CN: 模拟
    // relay 在对端代 Add 上盖章**认证**发送者账户。
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

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("Wire 1:1 multi-device join trigger", () => {
  it("grafts a fresh same-account device into an existing 1:1 via announce → offer → kp → add", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    // ── engines: alice device A (existing), alice device B (new), bob (peer) ─────────────────
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(ADDR_ALICE, "a"));
    await aliceB.init(deviceLeafIdentity(ADDR_ALICE, "b"));
    await bob.init(deviceLeafIdentity(ADDR_BOB, "b1"));

    // baseline 2-leaf pairwise: aliceA + bob (device B does NOT have the group yet)
    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    const epoch0 = aliceA.epochByConv(mlsKey);
    expect(aliceB.hasGroup(mlsKey)).toBe(false);

    // ── shared control bus: relayA (CD), relayB (new device), relayBob (peer follower) ───────
    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);
    const relayBob = new BusRelay(bus);
    // bob follows raw Commit frames on the conv to stay converged
    relayBob.onControl((m) => {
      if (m.t === "commit" && m.convId === mlsKey) bob.processCommitByConv(mlsKey, b64ToBytes(m.commit));
    });

    const sessionA = new DirectWireSession({
      engine: aliceA,
      relay: relayA,
      selfAddress: ADDR_ALICE,
      deviceId: "a", // smallest id → CD
      endpointId: "ep-a",
      listJoinableConvs: () => aliceA.listGroups().filter((k) => k.startsWith("d:")),
      settleMs: 50,
    });
    const sessionB = new DirectWireSession({
      engine: aliceB,
      relay: relayB,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
      settleMs: 50,
    });
    sessionA.start();
    sessionB.start();
    expect(sessionA.isCoordinator()).toBe(true);

    // ── the new device announces → cascade runs to completion ────────────────────────────────
    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(60); // flush request→offer→kp→add stage + 50ms settle merge

    // CD offered the existing conv, the new device returned exactly one KeyPackage
    expect(relayA.count("device_join_offer")).toBe(1);
    expect(relayB.count("device_join_kp")).toBe(1);

    // new device joined and all three leaves converged one epoch up
    expect(aliceB.hasGroup(mlsKey)).toBe(true);
    expect(aliceA.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(aliceB.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(bob.epochByConv(mlsKey)).toBe(epoch0 + 1);

    // ── cross-decrypt: new device B ↔ bob ────────────────────────────────────────────────────
    const fromB = await aliceB.encrypt(mlsKey, textEnvelope("m-b", "joined from device B", {}));
    expect((await bob.decrypt(mlsKey, fromB)).body).toMatchObject({ text: "joined from device B" });
    const fromBob = await bob.encrypt(mlsKey, textEnvelope("m-bob", "welcome aboard", {}));
    expect((await aliceB.decrypt(mlsKey, fromBob)).body).toMatchObject({ text: "welcome aboard" });

    // ── idempotent: re-announcing grafts nothing (device already holds the group) ────────────
    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(60);
    expect(relayA.count("device_join_offer")).toBe(2); // CD re-offers
    expect(relayB.count("device_join_kp")).toBe(1); // but the new device sends NO new KeyPackage
    expect(aliceA.epochByConv(mlsKey)).toBe(epoch0 + 1); // no new commit
  });

  it("offered convs become graft-owned + join settles once; the new device then follows live commits", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(ADDR_ALICE, "a"));
    await aliceB.init(deviceLeafIdentity(ADDR_ALICE, "b"));
    await bob.init(deviceLeafIdentity(ADDR_BOB, "b1"));

    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    const epoch0 = aliceA.epochByConv(mlsKey);

    const bus = new Bus();
    const relayA = new BusRelay(bus);
    const relayB = new BusRelay(bus);
    const relayBob = new BusRelay(bus);
    relayBob.onControl((m) => {
      if (m.t === "commit" && m.convId === mlsKey) bob.processCommitByConv(mlsKey, b64ToBytes(m.commit));
    });

    const graftConvsSeen: string[][] = [];
    const settledWith: string[][] = [];
    const sessionA = new DirectWireSession({
      engine: aliceA,
      relay: relayA,
      selfAddress: ADDR_ALICE,
      deviceId: "a",
      endpointId: "ep-a",
      listJoinableConvs: () => aliceA.listGroups().filter((k) => k.startsWith("d:")),
      settleMs: 50,
    });
    const sessionB = new DirectWireSession({
      engine: aliceB,
      relay: relayB,
      selfAddress: ADDR_ALICE,
      deviceId: "b",
      endpointId: "ep-b",
      settleMs: 50,
      joinSettleMs: 1000,
      onGraftConvs: (c) => graftConvsSeen.push(c),
      onJoinSettled: (c) => settledWith.push(c),
    });
    sessionA.start();
    sessionB.start();

    await sessionB.announceJoin();
    await vi.advanceTimersByTimeAsync(60);

    // the offered conv was reported as graft-owned (on offer + again on graft-completion; downstream
    // markGraftManaged is idempotent), and the join settled EXACTLY once with it
    expect(graftConvsSeen.flat()).toContain(mlsKey);
    expect(settledWith).toEqual([[mlsKey]]);
    expect(aliceB.hasGroup(mlsKey)).toBe(true);

    // ── a later live Commit (aliceA rekeys) must be followed by the grafted device automatically ──
    const beforeRekey = aliceA.epochByConv(mlsKey);
    await sessionA.rekey(mlsKey);
    await vi.advanceTimersByTimeAsync(60); // stage → settle(50) → merge + broadcast commit
    expect(aliceA.epochByConv(mlsKey)).toBe(beforeRekey + 1);
    // grafted device B followed the rekey Commit off the relay (registry is NOT managing this conv)
    expect(aliceB.epochByConv(mlsKey)).toBe(beforeRekey + 1);
    expect(bob.epochByConv(mlsKey)).toBe(beforeRekey + 1);

    const ct = await aliceB.encrypt(mlsKey, textEnvelope("post-rekey", "still in sync", {}));
    expect((await bob.decrypt(mlsKey, ct)).body).toMatchObject({ text: "still in sync" });
    expect(epoch0).toBeLessThan(aliceA.epochByConv(mlsKey));
  });

  it("peer-assisted Add: the peer grafts a requester's new device when no sibling is online", async () => {
    // EN: real timers + real SS58 keys — the new device's KeyPackage carries the in-MLS E2EI binding
    // (§3.9) that bob now REQUIRES, and verifying it is async (dynamic import + crypto init) which fake
    // timers don't flush deterministically. CN: 用真实定时器 + 真实 SS58 钥——新设备 KeyPackage 携带 bob
    // 现在**要求**的 MLS 内 E2EI 绑定（§3.9），其校验是异步（动态 import + 加密初始化），假定时器无法确定性 flush。
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);
    // alice's OLD device established the 1:1 with bob, then went offline; alice's NEW device (a2) has
    // no group and no online sibling → it asks bob to graft it.
    const aliceOld = new OpenMlsEngine();
    const aliceNew = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceOld.init(deviceLeafIdentity(aliceAddr, "old"));
    await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));

    // alice's new device installs the in-MLS E2EI binding (account key signs its stable leaf key), as
    // every production wire engine does at init → its KeyPackage proves account ownership to the peer.
    const leafKey = aliceNew.signaturePublicKey();
    aliceNew.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "new", leafKey)));

    // bob owns the group with alice's old leaf
    bob.createGroupByConv(mlsKey);
    const base = bob.addMembersByConv(mlsKey, [aliceOld.generateKeyPackage()]);
    await aliceOld.processWelcomeByConv(mlsKey, base.welcome);
    const epoch0 = bob.epochByConv(mlsKey);
    expect(aliceNew.hasGroup(mlsKey)).toBe(false);

    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    const relayAliceNew = new BusRelay(bus, aliceAddr);
    // aliceOld follows commits off the relay (so it stays converged too)
    const relayAliceOld = new BusRelay(bus, aliceAddr);
    relayAliceOld.onControl((m) => {
      if (m.t === "commit" && m.convId === mlsKey) {
        try {
          aliceOld.processCommitByConv(mlsKey, b64ToBytes(m.commit));
        } catch {
          /* may already be applied */
        }
      }
    });

    const bobSession = new DirectWireSession({
      engine: bob,
      relay: relayBob,
      selfAddress: bobAddr,
      deviceId: "b1",
      endpointId: "ep-bob",
      listJoinableConvs: () => bob.listGroups().filter((k) => k.startsWith("d:")),
      settleMs: 50,
    });
    const graftMarked: string[] = [];
    const aliceSession = new DirectWireSession({
      engine: aliceNew,
      relay: relayAliceNew,
      selfAddress: aliceAddr,
      deviceId: "new",
      endpointId: "ep-alice-new",
      settleMs: 50,
      onGraftConvs: (c) => graftMarked.push(...c),
    });
    bobSession.start();
    aliceSession.start();

    // alice's new device asks bob (the peer) to graft it
    await aliceSession.requestPeerAdd(bobAddr);
    await new Promise((r) => setTimeout(r, 300)); // peer_add_req → verify → add_device → settle → welcome

    // alice's new device joined the EXISTING group (not a fork): converged with bob + old leaf
    expect(aliceNew.hasGroup(mlsKey)).toBe(true);
    expect(bob.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(aliceNew.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(aliceOld.epochByConv(mlsKey)).toBe(epoch0 + 1);
    // the new device reported the conv as graft-owned (registry must back off)
    expect(graftMarked).toContain(mlsKey);

    // cross-decrypt: alice's NEW device ↔ bob
    const ct = await aliceNew.encrypt(mlsKey, textEnvelope("pa", "added by the peer", {}));
    expect((await bob.decrypt(mlsKey, ct)).body).toMatchObject({ text: "added by the peer" });
  });

  it("peer-assisted Add is rejected when the relay-stamped sender does not match the requester", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const bob = new OpenMlsEngine();
    await bob.init(deviceLeafIdentity(ADDR_BOB, "b1"));
    bob.createGroupByConv(mlsKey);
    const epoch0 = bob.epochByConv(mlsKey);

    const bus = new Bus();
    const relayBob = new BusRelay(bus, ADDR_BOB);
    // attacker authenticates as someone else but claims to be ALICE
    const relayAttacker = new BusRelay(bus, "5MalloryImposter");

    const bobSession = new DirectWireSession({
      engine: bob,
      relay: relayBob,
      selfAddress: ADDR_BOB,
      deviceId: "b1",
      endpointId: "ep-bob",
      settleMs: 50,
    });
    bobSession.start();

    // forged peer_add_req: requester_account claims ALICE but the authenticated sender is Mallory
    await relayAttacker.sendControl({
      t: "peer_add_req",
      from: "ep-mallory",
      convId: mlsKey,
      requester_account: ADDR_ALICE,
      device_id: "mallory-dev",
      kp: "AQID",
    } as ControlMsg);
    await vi.advanceTimersByTimeAsync(100);

    // bob must NOT have added anyone (no commit, epoch unchanged)
    expect(bob.epochByConv(mlsKey)).toBe(epoch0);
    expect(relayBob.sent.some((m) => m.t === "commit")).toBe(false);
  });

  it("peer-assisted Add is rejected when a compromised relay forges the auth stamp but the KeyPackage's in-MLS binding is not signed by the claimed account (relay-trustless)", async () => {
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    const bob = new OpenMlsEngine();
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));
    bob.createGroupByConv(mlsKey);
    const epoch0 = bob.epochByConv(mlsKey);

    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    // EN: simulate a MALICIOUS relay that forges the auth stamp as alice (so gate (a) passes); the
    // in-MLS E2EI binding is what actually stops the attack. CN: 模拟**恶意** relay 伪造 alice 的盖章
    // （使闸 (a) 通过）；真正拦下攻击的是 MLS 内 E2EI 绑定。
    const relayAttacker = new BusRelay(bus, aliceAddr);

    const bobSession = new DirectWireSession({
      engine: bob,
      relay: relayBob,
      selfAddress: bobAddr,
      deviceId: "b1",
      endpointId: "ep-bob",
      settleMs: 50,
    });
    bobSession.start();

    // attacker mints its OWN KeyPackage with an in-MLS binding signed by ITS key, claiming to be alice
    const attackerEngine = new OpenMlsEngine();
    await attackerEngine.init(deviceLeafIdentity(aliceAddr, "evil"));
    const leafKey = attackerEngine.signaturePublicKey();
    attackerEngine.setLeafBinding(attackerPair.sign(leafKeyBindingBytes(aliceAddr, "evil", leafKey)));
    const kp = bytesToB64(attackerEngine.generateKeyPackage());

    await relayAttacker.sendControl({
      t: "peer_add_req",
      from: "ep-evil",
      convId: mlsKey,
      requester_account: aliceAddr,
      device_id: "evil",
      kp,
    } as ControlMsg);
    await new Promise((r) => setTimeout(r, 300));

    // bob refuses to graft: the in-MLS binding is not signed by alice's account key
    expect(bob.epochByConv(mlsKey)).toBe(epoch0);
    expect(relayBob.sent.some((m) => m.t === "commit")).toBe(false);
  });

  it("peer-assisted Add: an in-MLS leaf binding (§3.9) carried by the KeyPackage is verified and grafts", async () => {
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    const aliceOld = new OpenMlsEngine();
    const aliceNew = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceOld.init(deviceLeafIdentity(aliceAddr, "old"));
    await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));

    // alice's new device installs the in-MLS E2EI binding: account key signs its stable leaf key →
    // every KeyPackage it mints carries the account ownership inside MLS.
    const leafKey = aliceNew.signaturePublicKey();
    aliceNew.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "new", leafKey)));

    bob.createGroupByConv(mlsKey);
    const base = bob.addMembersByConv(mlsKey, [aliceOld.generateKeyPackage()]);
    await aliceOld.processWelcomeByConv(mlsKey, base.welcome);
    const epoch0 = bob.epochByConv(mlsKey);

    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    const relayAliceNew = new BusRelay(bus, aliceAddr);

    const bobSession = new DirectWireSession({
      engine: bob,
      relay: relayBob,
      selfAddress: bobAddr,
      deviceId: "b1",
      endpointId: "ep-bob",
      settleMs: 50,
    });
    const aliceSession = new DirectWireSession({
      engine: aliceNew,
      relay: relayAliceNew,
      selfAddress: aliceAddr,
      deviceId: "new",
      endpointId: "ep-alice-new",
      settleMs: 50,
      // NOTE: bob verifies the in-MLS binding straight from the KeyPackage (no request-level cred).
    });
    bobSession.start();
    aliceSession.start();

    await aliceSession.requestPeerAdd(bobAddr);
    await new Promise((r) => setTimeout(r, 300));

    // bob accepted via the in-MLS binding carried by the KeyPackage and grafted the device
    expect(relayAliceNew.sent.some((m) => m.t === "peer_add_req")).toBe(true);
    expect(aliceNew.hasGroup(mlsKey)).toBe(true);
    expect(bob.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(aliceNew.epochByConv(mlsKey)).toBe(epoch0 + 1);
  });

  it("peer-assisted Add is rejected when the KeyPackage's in-MLS binding is forged (different account key)", async () => {
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    const aliceNew = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));

    // the binding is signed by the ATTACKER's key, not alice's → bob must reject the graft
    const leafKey = aliceNew.signaturePublicKey();
    aliceNew.setLeafBinding(attackerPair.sign(leafKeyBindingBytes(aliceAddr, "new", leafKey)));

    bob.createGroupByConv(mlsKey);
    const epoch0 = bob.epochByConv(mlsKey);

    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    const relayAliceNew = new BusRelay(bus, aliceAddr);

    const bobSession = new DirectWireSession({
      engine: bob,
      relay: relayBob,
      selfAddress: bobAddr,
      deviceId: "b1",
      endpointId: "ep-bob",
      settleMs: 50,
    });
    const aliceSession = new DirectWireSession({
      engine: aliceNew,
      relay: relayAliceNew,
      selfAddress: aliceAddr,
      deviceId: "new",
      endpointId: "ep-alice-new",
      settleMs: 50,
    });
    bobSession.start();
    aliceSession.start();

    await aliceSession.requestPeerAdd(bobAddr);
    await new Promise((r) => setTimeout(r, 300));

    expect(bob.epochByConv(mlsKey)).toBe(epoch0);
    expect(relayBob.sent.some((m) => m.t === "commit")).toBe(false);
    expect(aliceNew.hasGroup(mlsKey)).toBe(false);
  });

  it("join-trigger (§3.7) is rejected when a foreign device_join_kp carries a binding NOT signed by our own account key", async () => {
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    // alice's CD (device "a") holds the 1:1 group; it is the only sibling → it is the coordinator.
    const aliceA = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
    aliceA.createGroupByConv(mlsKey);
    const epoch0 = aliceA.epochByConv(mlsKey);

    const bus = new Bus();
    const relayA = new BusRelay(bus, aliceAddr);
    // a MALICIOUS relay injects a device_join_kp on alice's sibling channel claiming a new device.
    const relayAttacker = new BusRelay(bus, aliceAddr);

    const sessionA = new DirectWireSession({
      engine: aliceA,
      relay: relayA,
      selfAddress: aliceAddr,
      deviceId: "a",
      endpointId: "ep-a",
      listJoinableConvs: () => aliceA.listGroups().filter((k) => k.startsWith("d:")),
      settleMs: 50,
    });
    sessionA.start();
    expect(sessionA.isCoordinator()).toBe(true);

    // attacker mints its OWN KeyPackage and embeds a binding signed by ITS key, posing as alice#evil.
    const attackerEngine = new OpenMlsEngine();
    await attackerEngine.init(deviceLeafIdentity(aliceAddr, "evil"));
    const leafKey = attackerEngine.signaturePublicKey();
    attackerEngine.setLeafBinding(attackerPair.sign(leafKeyBindingBytes(aliceAddr, "evil", leafKey)));
    const kp = bytesToB64(attackerEngine.generateKeyPackage());

    await relayAttacker.sendControl({
      t: "device_join_kp",
      convId: `s:${aliceAddr}`,
      device_id: "evil",
      kps: [{ conv_id: mlsKey, kp }],
    } as ControlMsg);
    await new Promise((r) => setTimeout(r, 300));

    // the CD refuses to graft: the in-MLS binding is not signed by alice's own account key.
    expect(aliceA.epochByConv(mlsKey)).toBe(epoch0);
    expect(relayA.sent.some((m) => m.t === "commit")).toBe(false);
  });

  it("member-side re-verification (§3.9): a follower accepts an Add commit whose added leaf is genuinely account-bound, and REJECTS one with a forged binding", async () => {
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const conv = directMlsKey(aliceAddr, bobAddr);

    // committer = alice device "a"; follower = bob device "b1" (the CROSS-account peer that follows).
    const aliceA = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));
    aliceA.createGroupByConv(conv);
    const base = aliceA.addMembersByConv(conv, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(conv, base.welcome);
    const epoch0 = bob.epochByConv(conv);

    // ── (1) GENUINE: alice grafts a new device "a2" whose binding is signed by alice's account key ──
    const aliceA2 = new OpenMlsEngine();
    await aliceA2.init(deviceLeafIdentity(aliceAddr, "a2"));
    const goodKey = aliceA2.signaturePublicKey();
    aliceA2.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "a2", goodKey)));
    const goodAdd = aliceA.addMembersByConv(conv, [aliceA2.generateKeyPackage()]);

    expect(await verifyIncomingCommit(bob, conv, goodAdd.commit)).toBe(true);
    bob.processCommitByConv(conv, goodAdd.commit); // reuses the staged commit cached by inspect
    expect(bob.epochByConv(conv)).toBe(epoch0 + 1);

    // ── (2) FORGED: alice (or a malicious committer) tries to graft "a3" bound by the ATTACKER key ──
    const aliceA3 = new OpenMlsEngine();
    await aliceA3.init(deviceLeafIdentity(aliceAddr, "a3"));
    const badKey = aliceA3.signaturePublicKey();
    aliceA3.setLeafBinding(attackerPair.sign(leafKeyBindingBytes(aliceAddr, "a3", badKey)));
    const badAdd = aliceA.addMembersByConv(conv, [aliceA3.generateKeyPackage()]);
    const epoch1 = bob.epochByConv(conv);

    // the follower refuses: the added leaf's in-MLS binding is not signed by alice's account key.
    expect(await verifyIncomingCommit(bob, conv, badAdd.commit)).toBe(false);
    expect(bob.epochByConv(conv)).toBe(epoch1); // discarded → no epoch advance
  });

  it("device-removal PCS self-heal through DirectWireSession.removeDevice (H5/H6): the removed device can no longer read future messages", async () => {
    // EN: drives the exact path the H5 `removeWireDevice` UI action calls — session coordinator elects
    // the sole device as CD, executes a serialized remove, settles, and merges — then asserts per-device
    // PCS end-to-end. CN: 驱动 H5 `removeWireDevice` UI 动作所调用的同一路径——会话协调器把唯一设备选为 CD、
    // 执行串行化移除、静默、合并——再端到端断言按设备 PCS。
    vi.useRealTimers();
    await cryptoWaitReady();
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    const aliceOld = new OpenMlsEngine();
    const aliceNew = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceOld.init(deviceLeafIdentity(aliceAddr, "old"));
    await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));
    // both alice devices carry the in-MLS E2EI binding, as production wire engines do at init.
    aliceOld.setLeafBinding(
      alicePair.sign(leafKeyBindingBytes(aliceAddr, "old", aliceOld.signaturePublicKey())),
    );
    aliceNew.setLeafBinding(
      alicePair.sign(leafKeyBindingBytes(aliceAddr, "new", aliceNew.signaturePublicKey())),
    );

    // baseline 3-leaf group: bob owns it, both alice devices joined and converged.
    bob.createGroupByConv(mlsKey);
    const w1 = bob.addMembersByConv(mlsKey, [aliceOld.generateKeyPackage()]);
    await aliceOld.processWelcomeByConv(mlsKey, w1.welcome);
    const w2 = bob.addMembersByConv(mlsKey, [aliceNew.generateKeyPackage()]);
    await aliceNew.processWelcomeByConv(mlsKey, w2.welcome);
    aliceOld.processCommitByConv(mlsKey, w2.commit);
    const epoch0 = bob.epochByConv(mlsKey);
    expect(aliceOld.epochByConv(mlsKey)).toBe(epoch0);
    expect(aliceNew.epochByConv(mlsKey)).toBe(epoch0);

    // bob follows commits off the relay; aliceOld does NOT (it is the device being removed / offline).
    const bus = new Bus();
    const relayBob = new BusRelay(bus, bobAddr);
    const relayAliceNew = new BusRelay(bus, aliceAddr);
    relayBob.onControl((m) => {
      if (m.t === "commit" && m.convId === mlsKey) {
        try {
          bob.processCommitByConv(mlsKey, b64ToBytes(m.commit));
        } catch {
          /* idempotent */
        }
      }
    });

    // aliceNew is alice's sole online device → CD → it executes the remove of aliceOld via the session.
    const aliceSession = new DirectWireSession({
      engine: aliceNew,
      relay: relayAliceNew,
      selfAddress: aliceAddr,
      deviceId: "new",
      endpointId: "ep-alice-new",
      settleMs: 50,
    });
    aliceSession.start();

    const route = await aliceSession.removeDevice(mlsKey, deviceLeafIdentity(aliceAddr, "old"));
    expect(route).toBe("execute");
    await new Promise((r) => setTimeout(r, 300)); // broadcast → settle(50) → accept → merge

    // the serialized remove commit advanced both the committer and the following peer.
    expect(relayAliceNew.sent.some((m) => m.t === "commit")).toBe(true);
    expect(aliceNew.epochByConv(mlsKey)).toBe(epoch0 + 1);
    expect(bob.epochByConv(mlsKey)).toBe(epoch0 + 1);

    // alice's surviving device ↔ bob still converse at the new epoch.
    const ct = await bob.encrypt(mlsKey, textEnvelope("pcs", "old device gone", {}));
    expect((await aliceNew.decrypt(mlsKey, ct)).body).toMatchObject({ text: "old device gone" });

    // per-device PCS: the removed device (stuck at the old epoch) cannot read the new-epoch message.
    await expect(aliceOld.decrypt(mlsKey, ct)).rejects.toBeTruthy();

    aliceSession.stop();
  });

  it("first device (no CD answers) settles via the fallback timer with no grafts", async () => {
    const bus = new Bus();
    const relay = new BusRelay(bus);
    const engine = new OpenMlsEngine();
    await engine.init(deviceLeafIdentity(ADDR_ALICE, "solo"));
    const settledWith: string[][] = [];
    const session = new DirectWireSession({
      engine,
      relay,
      selfAddress: ADDR_ALICE,
      deviceId: "solo",
      endpointId: "ep-solo",
      joinSettleMs: 1000,
      onJoinSettled: (c) => settledWith.push(c),
    });
    session.start();
    await session.announceJoin();

    expect(settledWith).toEqual([]); // not settled yet
    await vi.advanceTimersByTimeAsync(1000); // fallback window elapses, no CD offer
    expect(settledWith).toEqual([[]]); // settled once, with zero grafts → host handshakes everyone
  });
});
