// EN: Engine round-trip for the read-only member roster (design §8 / spec §3.9): build a real WASM 1:1
// Wire group with multiple leaves (two devices of alice + bob), then assert `memberIdentities` lists all
// leaf identities and that `computeWireDeviceRoster` splits them per side. Proves the WASM primitive,
// the engine wrapper, and the pure helper agree end-to-end. CN: 只读成员名册的引擎往返（设计 §8 / 规范
// §3.9）：用真实 WASM 建多 leaf 的 1:1 Wire 群（alice 两台 + bob），断言 `memberIdentities` 列出全部 leaf
// 身份、`computeWireDeviceRoster` 按方拆分。证明 WASM 原语、引擎包装、纯 helper 端到端一致。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Keyring } from "@polkadot/keyring";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { beforeAll, describe, expect, it } from "vitest";

import init from "../mls-pkg/nexchat_mls.js";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";
import { deviceLeafIdentity, directMlsKey } from "@/mls/directConv";
import { computeWireDeviceRoster, removableSelfDevices } from "@/mls/wireDeviceRoster";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
  await cryptoWaitReady();
});

describe("OpenMlsEngine.memberIdentities → wireDeviceRoster", () => {
  it("lists every leaf of a multi-device 1:1 group and splits it per side", async () => {
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const aliceAddr = canonicalAddress(kr.addFromMnemonic(mnemonicGenerate()).address);
    const bobAddr = canonicalAddress(kr.addFromMnemonic(mnemonicGenerate()).address);
    const mlsKey = directMlsKey(aliceAddr, bobAddr);

    const aliceOld = new OpenMlsEngine();
    const aliceNew = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await aliceOld.init(deviceLeafIdentity(aliceAddr, "old"));
    await aliceNew.init(deviceLeafIdentity(aliceAddr, "new"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));

    // bob owns the group; add alice's old device, then alice's new device → 3 leaves.
    bob.createGroupByConv(mlsKey);
    const w1 = bob.addMembersByConv(mlsKey, [aliceOld.generateKeyPackage()]);
    await aliceOld.processWelcomeByConv(mlsKey, w1.welcome);
    const w2 = bob.addMembersByConv(mlsKey, [aliceNew.generateKeyPackage()]);
    await aliceNew.processWelcomeByConv(mlsKey, w2.welcome);
    aliceOld.processCommitByConv(mlsKey, w2.commit);

    const fromBob = bob.memberIdentities(mlsKey).sort();
    expect(fromBob).toEqual(
      [
        deviceLeafIdentity(bobAddr, "b1"),
        deviceLeafIdentity(aliceAddr, "old"),
        deviceLeafIdentity(aliceAddr, "new"),
      ].sort(),
    );
    // every member sees the same roster
    expect(aliceNew.memberIdentities(mlsKey).sort()).toEqual(fromBob);

    // alice's view: 2 of her devices, 1 peer device; her current device excluded from removable
    const roster = computeWireDeviceRoster(aliceNew.memberIdentities(mlsKey), aliceAddr, bobAddr, "new");
    expect(roster.total).toBe(3);
    expect(roster.self.map((d) => d.deviceId).sort()).toEqual(["new", "old"]);
    expect(roster.peer.map((d) => d.deviceId)).toEqual(["b1"]);
    expect(removableSelfDevices(roster).map((d) => d.deviceId)).toEqual(["old"]);
  });

  it("returns an empty roster for a conversation with no local group", async () => {
    const engine = new OpenMlsEngine();
    await engine.init(deviceLeafIdentity("5fakeAddr", "x"));
    expect(engine.memberIdentities("d:nope:nope")).toEqual([]);
  });
});
