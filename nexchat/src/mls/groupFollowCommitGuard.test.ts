// EN: Member-side E2EI re-verification for GROUP Wire commits (CHAT_GROUP_WIREIFY_DESIGN §6.4 / G2).
// A follower in a group independently re-verifies every device leaf an incoming Commit ADDS: the
// in-MLS binding must be signed by the claimed account's SS58 key AND that account must be a CURRENT
// group member. Mirrors the 1:1 guard test but with the group membership policy.
// CN: 群 Wire commit 的成员侧 E2EI 复验（设计 §6.4 / G2）。群内跟随者独立复验进入 Commit **新增**的每个
// 设备 leaf：MLS 内绑定须由所声称账户的 SS58 钥签名，且该账户须是**当前**群成员。对应 1:1 守卫测试，但用
// 群成员策略。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { canonicalAddress } from "@/wallet/address";
import { leafKeyBindingBytes } from "@/mls/deviceLeafCredential";
import { deviceLeafIdentity } from "@/mls/directConv";
import { verifyIncomingGroupCommit } from "@/mls/followCommitGuard";
import { OpenMlsEngine } from "@/mls/openMlsEngine";

const GROUP = "g:11";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
  await cryptoWaitReady();
});

interface World {
  aliceAddr: string;
  bobAddr: string;
  carolAddr: string;
  aliceA: OpenMlsEngine;
  aliceB: OpenMlsEngine;
  bob: OpenMlsEngine;
  carol: OpenMlsEngine;
  members: Set<string>;
}

// EN: 3-account group; `aliceB` is a second alice device whose binding is signed by `signer`.
// CN: 3 账户群；`aliceB` 为 alice 第二台设备，其绑定由 `signer` 签名。
async function setup(bindAliceBWith: "alice" | "attacker"): Promise<World> {
  const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
  const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
  const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
  const carolPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
  const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
  const aliceAddr = canonicalAddress(alicePair.address);
  const bobAddr = canonicalAddress(bobPair.address);
  const carolAddr = canonicalAddress(carolPair.address);

  const aliceA = new OpenMlsEngine();
  const aliceB = new OpenMlsEngine();
  const bob = new OpenMlsEngine();
  const carol = new OpenMlsEngine();
  await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
  await aliceB.init(deviceLeafIdentity(aliceAddr, "b"));
  await bob.init(deviceLeafIdentity(bobAddr, "b1"));
  await carol.init(deviceLeafIdentity(carolAddr, "c1"));

  aliceA.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "a", aliceA.signaturePublicKey())));
  const aliceBSigner = bindAliceBWith === "alice" ? alicePair : attackerPair;
  aliceB.setLeafBinding(
    aliceBSigner.sign(leafKeyBindingBytes(aliceAddr, "b", aliceB.signaturePublicKey())),
  );
  bob.setLeafBinding(bobPair.sign(leafKeyBindingBytes(bobAddr, "b1", bob.signaturePublicKey())));
  carol.setLeafBinding(carolPair.sign(leafKeyBindingBytes(carolAddr, "c1", carol.signaturePublicKey())));

  aliceA.createGroupByConv(GROUP);
  const add = aliceA.addMembersByConv(GROUP, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
  await bob.processWelcomeByConv(GROUP, add.welcome);
  await carol.processWelcomeByConv(GROUP, add.welcome);

  return {
    aliceAddr,
    bobAddr,
    carolAddr,
    aliceA,
    aliceB,
    bob,
    carol,
    members: new Set([aliceAddr, bobAddr, carolAddr]),
  };
}

describe("verifyIncomingGroupCommit (group member-side E2EI re-verification)", () => {
  it("accepts an add of a current member's device leaf with a valid binding", async () => {
    const w = await setup("alice");
    const grow = w.aliceA.addMembersByConv(GROUP, [w.aliceB.generateKeyPackage()]);

    const ok = await verifyIncomingGroupCommit(w.bob, GROUP, grow.commit, (a) => w.members.has(a));
    expect(ok).toBe(true);

    // verified → bob can process the same commit bytes and converge
    w.bob.processCommitByConv(GROUP, grow.commit);
    expect(w.bob.epochByConv(GROUP)).toBe(w.aliceA.epochByConv(GROUP));
  });

  it("rejects an added leaf whose binding is signed by a different account key", async () => {
    const w = await setup("attacker");
    const epochBefore = w.bob.epochByConv(GROUP);
    const grow = w.aliceA.addMembersByConv(GROUP, [w.aliceB.generateKeyPackage()]);

    const ok = await verifyIncomingGroupCommit(w.bob, GROUP, grow.commit, (a) => w.members.has(a));
    expect(ok).toBe(false);
    // discarded → bob stays at the old epoch (did NOT merge the forged add)
    expect(w.bob.epochByConv(GROUP)).toBe(epochBefore);
  });

  it("rejects an added leaf bound to an account that is NOT a current group member", async () => {
    const w = await setup("alice");
    const grow = w.aliceA.addMembersByConv(GROUP, [w.aliceB.generateKeyPackage()]);
    const epochBefore = w.bob.epochByConv(GROUP);

    // membership policy excludes alice → even a cryptographically valid binding is rejected
    const nonMember = new Set([w.bobAddr, w.carolAddr]);
    const ok = await verifyIncomingGroupCommit(w.bob, GROUP, grow.commit, (a) => nonMember.has(a));
    expect(ok).toBe(false);
    expect(w.bob.epochByConv(GROUP)).toBe(epochBefore);
  });
});
