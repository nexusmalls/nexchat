// EN: Group Wire-ification G3b (CHAT_GROUP_WIREIFY_DESIGN §7.2) — TS engine coverage for
// `stagedCommitFingerprintByConv`. A group Wire device op stages an add/remove/rekey, then reads the
// POST-commit `(treeHash, transcriptHash, epoch)` to put in the chain `commit(new_tree_hash,
// new_transcript_hash)` BEFORE the `expected_epoch` CAS verdict — without a speculative merge. This
// complements the native test (mls-wasm/tests/staged_fingerprint.rs) by exercising a real >=3-account
// group (Uint8Array KeyPackages) and the field-mapping + error path the wrapper adds.
// CN: 群 Wire 化 G3b（设计 §7.2）——`stagedCommitFingerprintByConv` 的 TS 引擎覆盖。群 Wire 设备操作暂存
// add/remove/rekey 后，读**后置** `(treeHash, transcriptHash, epoch)` 填入链上 `commit(new_tree_hash,
// new_transcript_hash)`，在 `expected_epoch` CAS 裁决**之前**，且不投机合并。本测试在真实 ≥3 账户群
// （Uint8Array KeyPackage）上覆盖原生测试覆盖不到的字段映射 + 错误路径。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { deviceLeafIdentity } from "@/mls/directConv";
import { OpenMlsEngine } from "@/mls/openMlsEngine";

const ADDR_ALICE = "5AliceAddr";
const GROUP = "g:99";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

async function buildGroup() {
  const aliceA = new OpenMlsEngine();
  const bob = new OpenMlsEngine();
  const carol = new OpenMlsEngine();
  await aliceA.init(deviceLeafIdentity(ADDR_ALICE, "a"));
  await bob.init(deviceLeafIdentity("5BobAddrLong", "b1"));
  await carol.init(deviceLeafIdentity("5CarolAddrLong", "c1"));

  aliceA.createGroupByConv(GROUP);
  const add = aliceA.addMembersByConv(GROUP, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
  await bob.processWelcomeByConv(GROUP, add.welcome);
  await carol.processWelcomeByConv(GROUP, add.welcome);
  return { aliceA, bob, carol };
}

describe("stagedCommitFingerprintByConv (group Wire G3b §7.2)", () => {
  it("returns the POST-commit fingerprint of a staged add WITHOUT merging, then merge lands on it", async () => {
    const { aliceA, bob, carol } = await buildGroup();
    const preEpoch = aliceA.epochByConv(GROUP);

    const dave = new OpenMlsEngine();
    await dave.init(deviceLeafIdentity(ADDR_ALICE, "d"));

    // STAGE the add (no merge) and read the staged post-commit fingerprint.
    const staged = aliceA.addMembersStagedByConv(GROUP, [dave.generateKeyPackage()]);
    const fp = aliceA.stagedCommitFingerprintByConv(GROUP);

    // field mapping (snake_case wasm → camelCase wrapper) + post-commit shape
    expect(fp.treeHash).toBeInstanceOf(Uint8Array);
    expect(fp.transcriptHash).toBeInstanceOf(Uint8Array);
    expect(fp.treeHash.length).toBe(32);
    expect(fp.transcriptHash.length).toBe(32);
    expect(fp.epoch).toBe(preEpoch + 1);

    // staging does NOT advance the live epoch
    expect(aliceA.epochByConv(GROUP)).toBe(preEpoch);

    // peers follow the same commit and converge to the staged epoch
    bob.processCommitByConv(GROUP, staged.commit);
    carol.processCommitByConv(GROUP, staged.commit);
    expect(bob.epochByConv(GROUP)).toBe(fp.epoch);
    expect(carol.epochByConv(GROUP)).toBe(fp.epoch);

    // committer merges and lands on exactly the staged epoch
    aliceA.mergePendingByConv(GROUP);
    expect(aliceA.epochByConv(GROUP)).toBe(fp.epoch);
  });

  it("throws when no commit is staged", async () => {
    const { aliceA } = await buildGroup();
    expect(() => aliceA.stagedCommitFingerprintByConv(GROUP)).toThrow();
  });
});
