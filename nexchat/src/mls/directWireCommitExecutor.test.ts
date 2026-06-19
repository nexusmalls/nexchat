// EN: WASM-backed integration test for Wire 1:1 multi-leaf `add_device` via the real executor +
// OpenMlsEngine. Mirrors mls-wasm/tests/hybrid_spike.rs (C2–C5) but at the TS executor level:
// one account holds TWO leaves in a pairwise group, the peer follows, and all leaves cross-decrypt.
// CN: 经真实 executor + OpenMlsEngine 的 Wire 1:1 多 leaf `add_device` WASM 集成测试。对应
// mls-wasm/tests/hybrid_spike.rs（C2–C5），但在 TS executor 层：同一账户在 pairwise 群持两个 leaf，
// 对端跟随，所有 leaf 互相解密。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { deviceLeafIdentity, directMlsKey } from "@/mls/directConv";
import { createAddDeviceExecutor } from "@/mls/directWireCommitExecutor";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import {
  accountSelfConvId,
  type CommitIntentControlMsg,
} from "@/mls/directCommitCoordination";
import { bytesToB64, type ControlMsg } from "@/relay/relayClient";

const ADDR_ALICE = "5AliceAddr";
const ADDR_BOB = "5BobAddrLong";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe("Wire 1:1 add_device executor", () => {
  it("grafts a second same-account leaf; peer follows; all leaves cross-decrypt", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);

    // alice has TWO devices (same identity "alice" → same credential = multi-leaf); bob is single.
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init("alice");
    await aliceB.init("alice");
    await bob.init("bob");

    // ── baseline 2-leaf pairwise: aliceA creates the group and adds bob ──────────────────────
    aliceA.createGroupByConv(mlsKey);
    const bobKp = bob.generateKeyPackage();
    const base = aliceA.addMembersByConv(mlsKey, [bobKp]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    expect(aliceA.epochByConv(mlsKey)).toBe(bob.epochByConv(mlsKey));

    // ── add_device: aliceB publishes a KeyPackage; CD (aliceA) grafts its leaf via the executor ─
    const sent: ControlMsg[] = [];
    const executor = createAddDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async (m) => void sent.push(m) },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });

    const aliceBKp = bytesToB64(aliceB.generateKeyPackage());
    const intent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep-alice-b",
      req_id: "req-1",
      kind: "add_device",
      payload: { dmConvId: mlsKey, kp: aliceBKp },
    };

    const preEpoch = aliceA.epochByConv(mlsKey);
    const out = await executor.runIntent(intent);
    expect(out.preEpoch).toBe(preEpoch); // wire commit_epoch = pre-op epoch
    expect(aliceA.epochByConv(mlsKey)).toBe(preEpoch); // STAGED → local epoch NOT advanced yet

    // relay ACCEPTed the slot → merge the staged commit
    await executor.commitAccepted(intent);
    expect(aliceA.epochByConv(mlsKey)).toBe(preEpoch + 1);

    // joining device consumes Welcome; peer follows the Commit
    await aliceB.processWelcomeByConv(mlsKey, b64(out.welcomeB64));
    bob.processCommitByConv(mlsKey, b64(out.commitB64));

    // all three leaves converged to the same epoch
    expect(aliceB.epochByConv(mlsKey)).toBe(preEpoch + 1);
    expect(bob.epochByConv(mlsKey)).toBe(preEpoch + 1);

    // ── cross-decrypt: aliceB ↔ bob, and aliceA reads aliceB (C4/C5) ─────────────────────────
    const fromB = await aliceB.encrypt(mlsKey, textEnvelope("m-b", "hi from device B", {}));
    const atBob = await bob.decrypt(mlsKey, fromB);
    expect((atBob.body as { text: string }).text).toBe("hi from device B");

    const fromBob = await bob.encrypt(mlsKey, textEnvelope("m-bob", "hi alice (both devices)", {}));
    const atA = await aliceA.decrypt(mlsKey, fromBob);
    expect((atA.body as { text: string }).text).toBe("hi alice (both devices)");

    // deliverWelcome targets MY account so the relay fans it to my devices
    await executor.deliverWelcome(intent, out.welcomeB64);
    const welcome = sent.find((m) => m.t === "welcome");
    expect(welcome && "toAddr" in welcome ? welcome.toAddr : null).toBe(ADDR_ALICE);
  });

  it("staged add can be abandoned without forking the local group (lost CAS race)", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init("alice");
    await aliceB.init("alice");
    await bob.init("bob");

    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    const epochBefore = aliceA.epochByConv(mlsKey);

    const executor = createAddDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const intent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep-alice-b",
      req_id: "req-x",
      kind: "add_device",
      payload: { dmConvId: mlsKey, kp: bytesToB64(aliceB.generateKeyPackage()) },
    };

    await executor.runIntent(intent); // stage (no merge)
    expect(aliceA.epochByConv(mlsKey)).toBe(epochBefore);
    await executor.commitAbandoned(intent); // lost the race → discard
    // group is back to operational at the same epoch (NO fork) and still usable
    expect(aliceA.epochByConv(mlsKey)).toBe(epochBefore);
    const ct = await aliceA.encrypt(mlsKey, textEnvelope("still", "works", {}));
    expect((await bob.decrypt(mlsKey, ct)).body).toMatchObject({ text: "works" });
  });

  it("rekey (self_update) stages, merges on accept, and the peer follows", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const aliceA = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init("alice");
    await bob.init("bob");

    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    const preEpoch = aliceA.epochByConv(mlsKey);

    const executor = createAddDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const intent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep-alice-a",
      req_id: "req-rekey",
      kind: "rekey",
      payload: { dmConvId: mlsKey },
    };

    const out = await executor.runIntent(intent);
    expect(out.preEpoch).toBe(preEpoch);
    expect(out.welcomeB64).toBe(""); // rekey carries no Welcome
    expect(aliceA.epochByConv(mlsKey)).toBe(preEpoch); // staged
    await executor.commitAccepted(intent);
    expect(aliceA.epochByConv(mlsKey)).toBe(preEpoch + 1);

    bob.processCommitByConv(mlsKey, b64(out.commitB64));
    expect(bob.epochByConv(mlsKey)).toBe(preEpoch + 1);
    const ct = await bob.encrypt(mlsKey, textEnvelope("post", "rekey ok", {}));
    expect((await aliceA.decrypt(mlsKey, ct)).body).toMatchObject({ text: "rekey ok" });
  });

  it("loser applies the winning commit while holding a staged commit, catches up, then re-stages", async () => {
    // Cross-account concurrent add: alice's device (CD-A) and bob's device (CD-B) both stage an
    // add at the same epoch; the relay CAS lets alice win. bob (loser) must adopt alice's commit
    // even though bob still holds its own staged pending commit — OpenMLS merge clears the stale one.
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const aliceA = new OpenMlsEngine();
    const aliceA2 = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    const bob2 = new OpenMlsEngine();
    await aliceA.init("alice");
    await aliceA2.init("alice");
    await bob.init("bob");
    await bob2.init("bob");

    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    const epochE = aliceA.epochByConv(mlsKey);
    expect(bob.epochByConv(mlsKey)).toBe(epochE);

    // both stage an add at epoch E (concurrent)
    const aliceWin = aliceA.addMembersStagedByConv(mlsKey, [aliceA2.generateKeyPackage()]);
    const b2kp = bob2.generateKeyPackage();
    bob.addMembersStagedByConv(mlsKey, [b2kp]); // bob's losing staged commit
    expect(aliceA.epochByConv(mlsKey)).toBe(epochE); // staged, not advanced
    expect(bob.epochByConv(mlsKey)).toBe(epochE);

    // relay CAS: alice wins → alice merges its staged commit
    aliceA.mergePendingByConv(mlsKey);
    expect(aliceA.epochByConv(mlsKey)).toBe(epochE + 1);

    // bob LOSES: adopt alice's winning commit despite holding a staged pending commit.
    // OpenMLS merge_staged_commit clears bob's stale pending → bob catches up, no throw.
    bob.processCommitByConv(mlsKey, aliceWin.commit);
    expect(bob.epochByConv(mlsKey)).toBe(epochE + 1);

    // bob can now re-stage its add at the caught-up epoch and merge cleanly
    const bobRetry = bob.addMembersStagedByConv(mlsKey, [b2kp]);
    bob.mergePendingByConv(mlsKey);
    expect(bob.epochByConv(mlsKey)).toBe(epochE + 2);

    // alice follows bob's accepted retry; both converge with all four leaves
    aliceA.processCommitByConv(mlsKey, bobRetry.commit);
    expect(aliceA.epochByConv(mlsKey)).toBe(epochE + 2);
    const ct = await aliceA.encrypt(mlsKey, textEnvelope("conv", "converged", {}));
    expect((await bob.decrypt(mlsKey, ct)).body).toMatchObject({ text: "converged" });
  });

  it("remove_device targets a single device leaf by `{account}#{deviceId}` and gives per-device PCS", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    // device-distinct credential identities → a single device leaf can be targeted
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(ADDR_ALICE, "a"));
    await aliceB.init(deviceLeafIdentity(ADDR_ALICE, "b"));
    await bob.init(deviceLeafIdentity(ADDR_BOB, "b1"));

    aliceA.createGroupByConv(mlsKey);
    const base = aliceA.addMembersByConv(mlsKey, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(mlsKey, base.welcome);
    // graft aliceB as a second alice leaf
    const add = aliceA.addMembersByConv(mlsKey, [aliceB.generateKeyPackage()]);
    await aliceB.processWelcomeByConv(mlsKey, add.welcome);
    bob.processCommitByConv(mlsKey, add.commit);
    const epoch = aliceA.epochByConv(mlsKey);
    expect(aliceB.epochByConv(mlsKey)).toBe(epoch);

    const executor = createAddDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const intent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep-alice-a",
      req_id: "req-rm",
      kind: "remove_device",
      payload: { dmConvId: mlsKey, target: deviceLeafIdentity(ADDR_ALICE, "b") },
    };

    const out = await executor.runIntent(intent);
    expect(out.preEpoch).toBe(epoch); // staged
    expect(aliceA.epochByConv(mlsKey)).toBe(epoch);
    await executor.commitAccepted(intent); // merge removal
    expect(aliceA.epochByConv(mlsKey)).toBe(epoch + 1);
    bob.processCommitByConv(mlsKey, b64(out.commitB64));
    expect(bob.epochByConv(mlsKey)).toBe(epoch + 1);

    // aliceA ↔ bob still converse at the new epoch
    const ct = await aliceA.encrypt(mlsKey, textEnvelope("after", "device b removed", {}));
    expect((await bob.decrypt(mlsKey, ct)).body).toMatchObject({ text: "device b removed" });

    // per-device PCS: removed device B (still at the old epoch) cannot read the new-epoch message
    await expect(aliceB.decrypt(mlsKey, ct)).rejects.toBeTruthy();
  });

  it("deliverWelcome targets payload.welcomeTo (peer-assisted Add) instead of self", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const bob = new OpenMlsEngine();
    const alice2 = new OpenMlsEngine();
    await bob.init(deviceLeafIdentity(ADDR_BOB, "b1"));
    await alice2.init(deviceLeafIdentity(ADDR_ALICE, "a2"));

    // bob owns the 1:1 group (alice's old leaf omitted for brevity — bob just needs a group)
    bob.createGroupByConv(mlsKey);

    const sent: ControlMsg[] = [];
    const executor = createAddDeviceExecutor({
      engine: bob,
      relay: { sendControl: async (m) => void sent.push(m) },
      endpointId: "ep-bob",
      selfAddress: ADDR_BOB, // bob is doing the add on alice's behalf
    });
    const intent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_BOB),
      from: "ep-bob",
      req_id: "req-peer",
      kind: "add_device",
      // joining device belongs to ALICE → Welcome must be delivered to alice, not bob
      payload: { dmConvId: mlsKey, kp: bytesToB64(alice2.generateKeyPackage()), welcomeTo: ADDR_ALICE },
    };

    const out = await executor.runIntent(intent);
    await executor.commitAccepted(intent);
    await executor.deliverWelcome(intent, out.welcomeB64);
    const welcome = sent.find((m) => m.t === "welcome");
    expect(welcome && "toAddr" in welcome ? welcome.toAddr : null).toBe(ADDR_ALICE);

    // the alice device can actually consume that Welcome and join bob's group
    await alice2.processWelcomeByConv(mlsKey, b64(out.welcomeB64));
    expect(alice2.epochByConv(mlsKey)).toBe(bob.epochByConv(mlsKey));
  });

  it("requestCatchUp asks the relay to re-deliver the conv's stored backlog to self", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const alice = new OpenMlsEngine();
    await alice.init(deviceLeafIdentity(ADDR_ALICE, "a1"));
    alice.createGroupByConv(mlsKey);

    const backlogReqs: Array<{ account: string; convId: string }> = [];
    const executor = createAddDeviceExecutor({
      engine: alice,
      relay: {
        sendControl: async () => {},
        requestMlsBacklog: (account, convId) => void backlogReqs.push({ account, convId }),
      },
      endpointId: "ep-alice",
      selfAddress: ADDR_ALICE,
    });

    executor.requestCatchUp?.(mlsKey);
    expect(backlogReqs).toEqual([{ account: ADDR_ALICE, convId: mlsKey }]);
  });

  it("rejects a non-party conv and a missing KeyPackage", async () => {
    const mlsKey = directMlsKey(ADDR_ALICE, ADDR_BOB);
    const aliceA = new OpenMlsEngine();
    await aliceA.init("alice");
    aliceA.createGroupByConv(mlsKey);

    const executor = createAddDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });

    const noKp: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep",
      req_id: "r",
      kind: "add_device",
      payload: { dmConvId: mlsKey },
    };
    await expect(executor.runIntent(noKp)).rejects.toThrow(/KeyPackage/);

    const foreign: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep",
      req_id: "r",
      kind: "add_device",
      payload: { dmConvId: directMlsKey("5XOther", "5YOther"), kp: "AQID" },
    };
    await expect(executor.runIntent(foreign)).rejects.toThrow(/not a party/);
  });
});

function b64(s: string): Uint8Array {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}
