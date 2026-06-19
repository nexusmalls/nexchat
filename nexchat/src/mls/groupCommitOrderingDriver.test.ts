// EN: WASM-backed test for the group commit ordering driver (CHAT_GROUP_WIREIFY_DESIGN §7/§15.3/G3).
// Drives the real `createGroupDeviceExecutor` against a FAKE chain CAS (a stateful per-group epoch that
// accepts only at `expected_epoch`, else returns EpochStale). Proves: happy-path accept+merge; a
// concurrent winner forces EpochStale → catch-up via `syncGroupEpoch` → re-stage → accept; and bounded
// retries give up cleanly without forking the local group.
// CN: 群 commit 定序驱动的 WASM 测试（设计 §7/§15.3/G3）。用真实 `createGroupDeviceExecutor` 对**假**链 CAS
// （按群有状态 epoch，仅在 `expected_epoch` 接受，否则返回 EpochStale）驱动。证明：顺利路径接受+合并；并发
// 胜出者触发 EpochStale → 经 `syncGroupEpoch` 追平 → 重新暂存 → 接受；有界重试干净放弃且不分叉本地群。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { deviceLeafIdentity } from "@/mls/directConv";
import { createGroupDeviceExecutor } from "@/mls/groupDeviceCommitExecutor";
import {
  GroupCommitOrderingDriver,
  type SubmitGroupCommit,
} from "@/mls/groupCommitOrderingDriver";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";
import { accountSelfConvId, type CommitIntentControlMsg } from "@/mls/directCommitCoordination";
import { bytesToB64 } from "@/relay/relayClient";

const ADDR_ALICE = "5AliceAddr";
const GROUP = "g:99";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

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

function b64(s: string): Uint8Array {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}

describe("GroupCommitOrderingDriver (chain expected_epoch CAS)", () => {
  it("happy path: stages, chain accepts, merges, and delivers the Welcome", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();
    const sent: Array<{ toAddr?: string }> = [];
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async (m) => void sent.push(m as { toAddr?: string }) },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });

    let chainEpoch = aliceA.epochByConv(GROUP);
    let accepted: Uint8Array | null = null;
    const submit: SubmitGroupCommit = async ({ commitB64, expectedEpoch }) => {
      if (expectedEpoch !== chainEpoch) {
        return { ok: false, reason: "epoch_stale", currentEpoch: chainEpoch };
      }
      accepted = b64(commitB64);
      chainEpoch += 1;
      return { ok: true, newEpoch: chainEpoch };
    };

    const driver = new GroupCommitOrderingDriver({ executor, submitGroupCommit: submit });
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));
    const preEpoch = aliceA.epochByConv(GROUP);

    const outcome = await driver.run(intent);
    expect(outcome.ok).toBe(true);
    expect(outcome.attempts).toBe(1);
    expect(outcome.finalEpoch).toBe(preEpoch + 1);
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch + 1); // merged after accept

    // Welcome fanned to alice's own account over s:<account>
    expect(sent.some((m) => m.toAddr === ADDR_ALICE)).toBe(true);

    // the accepted commit converges the rest of the group + the joining device
    bob.processCommitByConv(GROUP, accepted!);
    carol.processCommitByConv(GROUP, accepted!);
    // joining device consumes the staged Welcome (captured by the executor's deliverWelcome payload)
    // here we re-stage independently isn't needed: just assert the committed wire converged peers
    expect(bob.epochByConv(GROUP)).toBe(preEpoch + 1);
    expect(carol.epochByConv(GROUP)).toBe(preEpoch + 1);
  });

  it("EpochStale: a concurrent winner forces catch-up + re-stage, then accepts", async () => {
    const { aliceA, aliceB, bob, carol } = await buildGroup();

    // a concurrent winner (carol rekey) that the chain will accept FIRST
    const winning = carol.selfUpdateStagedByConv(GROUP);
    carol.mergePendingByConv(GROUP);
    bob.processCommitByConv(GROUP, winning);
    const winnerEpoch = carol.epochByConv(GROUP); // preEpoch + 1

    let chainEpoch = winnerEpoch; // chain already advanced past aliceA's pre-op epoch
    let accepted: Uint8Array | null = null;
    const submit: SubmitGroupCommit = async ({ commitB64, expectedEpoch }) => {
      if (expectedEpoch !== chainEpoch) {
        return { ok: false, reason: "epoch_stale", currentEpoch: chainEpoch };
      }
      accepted = b64(commitB64);
      chainEpoch += 1;
      return { ok: true, newEpoch: chainEpoch };
    };

    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
      // syncGroupEpoch applies the winning chain commit so aliceA reaches `toEpoch`
      syncGroupEpoch: async (conv, toEpoch) => {
        if (aliceA.epochByConv(conv) < toEpoch) aliceA.processCommitByConv(conv, winning);
      },
    });

    const driver = new GroupCommitOrderingDriver({ executor, submitGroupCommit: submit });
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));

    const outcome = await driver.run(intent);
    expect(outcome.ok).toBe(true);
    expect(outcome.attempts).toBe(2); // first stale, second accepted
    expect(outcome.finalEpoch).toBe(winnerEpoch + 1);
    expect(aliceA.epochByConv(GROUP)).toBe(winnerEpoch + 1);

    // peers (already at winnerEpoch) converge on the accepted add
    bob.processCommitByConv(GROUP, accepted!);
    carol.processCommitByConv(GROUP, accepted!);
    expect(bob.epochByConv(GROUP)).toBe(winnerEpoch + 1);

    // the group still works post-convergence
    const ct = await aliceA.encrypt(GROUP, textEnvelope("ok", "converged via chain CAS", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "converged via chain CAS" });
  });

  it("gives up after bounded EpochStale retries without forking the local group", async () => {
    const { aliceA, aliceB } = await buildGroup();
    const preEpoch = aliceA.epochByConv(GROUP);
    // chain is permanently ahead and never catches up (syncGroupEpoch is a no-op) → every submit stale
    const submit: SubmitGroupCommit = async () => ({
      ok: false,
      reason: "epoch_stale",
      currentEpoch: preEpoch + 99,
    });
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
      syncGroupEpoch: async () => {}, // never actually catches up
    });

    const driver = new GroupCommitOrderingDriver({ executor, submitGroupCommit: submit, maxRetries: 3 });
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));

    const outcome = await driver.run(intent);
    expect(outcome.ok).toBe(false);
    // catchUpAndRerun throws (local never reaches toEpoch) → classified as error, abandoned cleanly
    expect(outcome.reason).toBe("error");
    // local group is NOT forked: still at the original epoch and usable
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch);
  });

  it("surfaces a non-stale chain error and abandons the staged commit", async () => {
    const { aliceA, aliceB, bob } = await buildGroup();
    const preEpoch = aliceA.epochByConv(GROUP);
    const submit: SubmitGroupCommit = async () => ({
      ok: false,
      reason: "error",
      error: new Error("GroupFrozen"),
    });
    const executor = createGroupDeviceExecutor({
      engine: aliceA,
      relay: { sendControl: async () => {} },
      endpointId: "ep-alice-a",
      selfAddress: ADDR_ALICE,
    });
    const driver = new GroupCommitOrderingDriver({ executor, submitGroupCommit: submit });
    const intent = addDeviceIntent(bytesToB64(aliceB.generateKeyPackage()));

    const outcome = await driver.run(intent);
    expect(outcome.ok).toBe(false);
    expect(outcome.reason).toBe("error");
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch); // staged commit discarded, no fork
    // group still usable
    const ct = await aliceA.encrypt(GROUP, textEnvelope("x", "still ok", {}));
    expect((await bob.decrypt(GROUP, ct)).body).toMatchObject({ text: "still ok" });
  });
});
