// EN: WASM-backed integration test for the group Wire-ification device executor. Mirrors
// mls-wasm/tests/group_wire_spike.rs (S1–S3) at the TS executor level: in a >=3-account GROUP, one
// account grafts a SECOND device leaf via the executor, both devices send concurrently, removal gives
// per-device PCS, and a lost ordering race is discarded without forking the local group.
// CN: 群 Wire 化设备执行器的 WASM 集成测试。对应 mls-wasm/tests/group_wire_spike.rs（S1–S3），但在 TS
// executor 层：在 ≥3 账户**群**里，一个账户经执行器嫁接**第二台**设备 leaf，两设备并发发送、移除得到按
// 设备 PCS，落败定序竞争被丢弃且不分叉本地群。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { deviceLeafIdentity } from "@/mls/directConv";
import { createGroupDeviceExecutor } from "@/mls/groupDeviceCommitExecutor";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import {
  accountSelfConvId,
  type CommitIntentControlMsg,
} from "@/mls/directCommitCoordination";
import { bytesToB64, type ControlMsg } from "@/relay/relayClient";

const ADDR_ALICE = "5AliceAddr";
const GROUP = "g:42";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

// EN: Build a 3-account group (aliceA creator + bob + carol) and return the engines/views.
// CN: 建 3 账户群（aliceA 建者 + bob + carol）并返回引擎/视图。
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
  const addBoth = aliceA.addMembersByConv(GROUP, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
  await bob.processWelcomeByConv(GROUP, addBoth.welcome);
  await carol.processWelcomeByConv(GROUP, addBoth.welcome);
  expect(aliceA.epochByConv(GROUP)).toBe(bob.epochByConv(GROUP));
  expect(aliceA.epochByConv(GROUP)).toBe(carol.epochByConv(GROUP));
  return { aliceA, aliceB, bob, carol };
}

function addDeviceIntent(kpB64: string): CommitIntentControlMsg {
  return {
    t: "commit_intent",
    convId: accountSelfConvId(ADDR_ALICE),
    from: "ep-alice-b",
    req_id: "req-1",
    kind: "add_device",
    payload: { dmConvId: GROUP, kp: kpB64 },
  };
}

describe("group Wire add_device executor", () => {
  it("grafts a second same-account device leaf; all members follow and cross-decrypt", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();

    const sent: ControlMsg[] = [];
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async (m) => void sent.push(m) },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });

    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));
    const preEpoch = aliceA.epochByConv(GROUP);

    const out = await executor.runIntent(intent);
    expect(out.preEpoch).toBe(preEpoch); // chain expected_epoch = pre-op epoch
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch); // STAGED → not advanced

    // chain `commit` returned Ok → merge
    await executor.commitAccepted(intent);
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch + 1);

    // joining device consumes Welcome; the other members follow the Commit
    await aliceB.processWelcomeByConv(GROUP, b64(out.welcomeB64));
    bob.processCommitByConv(GROUP, b64(out.commitB64));
    carol.processCommitByConv(GROUP, b64(out.commitB64));
    expect(aliceB.epochByConv(GROUP)).toBe(preEpoch + 1);
    expect(bob.epochByConv(GROUP)).toBe(preEpoch + 1);
    expect(carol.epochByConv(GROUP)).toBe(preEpoch + 1);

    // the new device can send; every member (incl. sibling) reads it
    const fromB = await aliceB.encrypt(GROUP, textEnvelope("m-b", "hi from device B", {}));
    expect((await bob.decrypt(GROUP, fromB)).body).toMatchObject({ text: "hi from device B" });
    expect((await carol.decrypt(GROUP, fromB)).body).toMatchObject({ text: "hi from device B" });
    expect((await aliceA.decrypt(GROUP, fromB)).body).toMatchObject({ text: "hi from device B" });

    // deliverWelcome targets MY account so the relay fans it to my devices over s:<account>
    await executor.deliverWelcome(intent, out.welcomeB64);
    const welcome = sent.find((m) => m.t === "welcome");
    expect(welcome && "toAddr" in welcome ? welcome.toAddr : null).toBe(ADDR_ALICE);
  });

  it("two devices send concurrently at the same epoch with no ciphertext reuse", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));
    const out = await executor.runIntent(intent);
    await executor.commitAccepted(intent);
    await aliceB.processWelcomeByConv(GROUP, b64(out.welcomeB64));
    bob.processCommitByConv(GROUP, b64(out.commitB64));
    carol.processCommitByConv(GROUP, b64(out.commitB64));

    const epoch = aliceA.epochByConv(GROUP);
    const wires: Uint8Array[] = [];
    for (let i = 0; i < 3; i++) {
      wires.push(await aliceA.encrypt(GROUP, textEnvelope(`a-${i}`, `A-${i}`, {})));
      wires.push(await aliceB.encrypt(GROUP, textEnvelope(`b-${i}`, `B-${i}`, {})));
    }
    // application messages do not advance the epoch (truly same-epoch concurrent sends)
    expect(aliceA.epochByConv(GROUP)).toBe(epoch);
    expect(aliceB.epochByConv(GROUP)).toBe(epoch);
    // all ciphertexts distinct (proxy for no (key,nonce) reuse across the two leaves)
    const uniq = new Set(wires.map((w) => bytesToB64(w)));
    expect(uniq.size).toBe(wires.length);
  });

  it("remove_device targets a single device leaf and gives per-device PCS", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });

    // graft aliceB first
    const addIntent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));
    const add = await executor.runIntent(addIntent);
    await executor.commitAccepted(addIntent);
    await aliceB.processWelcomeByConv(GROUP, b64(add.welcomeB64));
    bob.processCommitByConv(GROUP, b64(add.commitB64));
    carol.processCommitByConv(GROUP, b64(add.commitB64));
    const epoch = aliceA.epochByConv(GROUP);

    // now remove aliceB's device leaf by its device-distinct identity
    const rmIntent: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep-alice-a",
      req_id: "req-rm",
      kind: "remove_device",
      payload: { dmConvId: GROUP, target: deviceLeafIdentity(ADDR_ALICE, "b") },
    };
    const out = await executor.runIntent(rmIntent);
    expect(out.preEpoch).toBe(epoch); // staged
    expect(aliceA.epochByConv(GROUP)).toBe(epoch);
    await executor.commitAccepted(rmIntent);
    expect(aliceA.epochByConv(GROUP)).toBe(epoch + 1);
    bob.processCommitByConv(GROUP, b64(out.commitB64));
    carol.processCommitByConv(GROUP, b64(out.commitB64));

    // remaining members still converse
    const ct = await aliceA.encrypt(GROUP, textEnvelope("after", "device b removed", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "device b removed" });
    expect((await carol.decrypt(GROUP, ct)).body).toMatchObject({ text: "device b removed" });
    // per-device PCS: removed device cannot read the new-epoch message
    await expect(aliceB.decrypt(GROUP, ct)).rejects.toBeTruthy();
  });

  it("staged add can be abandoned without forking the local group (lost chain race)", async () => {
    const { aliceA, aliceB, bob } = await buildGroup();
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const epochBefore = aliceA.epochByConv(GROUP);
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));

    await executor.runIntent(intent); // stage (no merge)
    expect(aliceA.epochByConv(GROUP)).toBe(epochBefore);
    await executor.commitAbandoned(intent); // EpochStale / gave up → discard
    expect(aliceA.epochByConv(GROUP)).toBe(epochBefore); // no fork
    // group still usable at the same epoch
    const ct = await aliceA.encrypt(GROUP, textEnvelope("still", "works", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "works" });
  });

  it("catchUpAndRerun syncs the chain epoch then re-stages at the caught-up epoch", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();

    // Simulate a concurrent winning chain commit: carol adds a self-update (rekey) that advances the
    // epoch; the winning commit bytes are captured so `syncGroupEpoch` can apply them to aliceA.
    const winning = carol.selfUpdateStagedByConv(GROUP);
    carol.mergePendingByConv(GROUP);
    const winningEpoch = carol.epochByConv(GROUP);
    bob.processCommitByConv(GROUP, winning);

    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
      syncGroupEpoch: async (convId, toEpoch) => {
        // pull + apply the winning chain commit so the local group reaches `toEpoch`
        if (aliceA.epochByConv(convId) < toEpoch) aliceA.processCommitByConv(convId, winning);
      },
    });

    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));
    await executor.runIntent(intent); // staged at the now-stale epoch

    // chain rejected with EpochStale(current = winningEpoch) → catch up and re-stage
    const rerun = await executor.catchUpAndRerun(intent, winningEpoch);
    expect(rerun.newEpoch).toBe(winningEpoch);
    expect(aliceA.epochByConv(GROUP)).toBe(winningEpoch); // caught up, still staged (not merged)

    await executor.commitAccepted(intent);
    expect(aliceA.epochByConv(GROUP)).toBe(winningEpoch + 1);
    await aliceB.processWelcomeByConv(GROUP, b64(rerun.welcomeB64));
    bob.processCommitByConv(GROUP, b64(rerun.commitB64));
    carol.processCommitByConv(GROUP, b64(rerun.commitB64));
    expect(bob.epochByConv(GROUP)).toBe(winningEpoch + 1);
    expect(carol.epochByConv(GROUP)).toBe(winningEpoch + 1);
    expect(aliceB.epochByConv(GROUP)).toBe(winningEpoch + 1);
  });

  it("rejects a non-group conv and a missing KeyPackage", async () => {
    const { aliceA } = await buildGroup();
    const executor = createGroupDeviceExecutor({
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
      payload: { dmConvId: GROUP },
    };
    await expect(executor.runIntent(noKp)).rejects.toThrow(/KeyPackage/);

    const dmConv: CommitIntentControlMsg = {
      t: "commit_intent",
      convId: accountSelfConvId(ADDR_ALICE),
      from: "ep",
      req_id: "r",
      kind: "add_device",
      payload: { dmConvId: "d:5X:5Y", kp: "AQID" },
    };
    await expect(executor.runIntent(dmConv)).rejects.toThrow(/group conv/);
  });
});

function b64(s: string): Uint8Array {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}
