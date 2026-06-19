// EN: G1 (group Wire-ification) engine-level test (CHAT_GROUP_WIREIFY_DESIGN §11 G1). Proves the
// GROUP engine, when initialised the Wire way — a device-distinct leaf credential (`{account}#{dev}`)
// + an in-MLS E2EI leaf binding, holding its OWN signer (NOT the Track A read-only escrow) — embeds a
// relay-trustless account binding into every GROUP KeyPackage that any member can verify straight from
// the KeyPackage. This is the group analogue of the 1:1 binding proven in directWireSessionJoin.test.
// CN: G1（群 Wire 化）引擎级测试（设计 §11 G1）。证明**群**引擎按 Wire 方式初始化时——设备区分 leaf 凭证
// （`{account}#{dev}`）+ MLS 内 E2EI leaf 绑定、持**自己的** signer（**非**轨 A 只读托管）——把 relay-trustless
// 的账户绑定嵌入**每个群** KeyPackage，任一成员可直接从 KeyPackage 验证。这是 1:1 绑定（见
// directWireSessionJoin.test）的群侧对应。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Keyring } from "@polkadot/keyring";
import type { KeyringPair } from "@polkadot/keyring/types";
import { cryptoWaitReady, mnemonicGenerate } from "@polkadot/util-crypto";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { canonicalAddress } from "@/wallet/address";
import { leafKeyBindingBytes, verifyLeafKeyBinding } from "@/mls/deviceLeafCredential";
import { deviceLeafIdentity } from "@/mls/directConv";
import { OpenMlsEngine } from "@/mls/openMlsEngine";
import { textEnvelope } from "@/mls/envelope";

const GROUP = "g:7";

function toHex(bytes: Uint8Array): string {
  return "0x" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
  await cryptoWaitReady();
});

describe("G1 group Wire engine: device-distinct leaf + E2EI binding in group KeyPackages", () => {
  it("embeds a verifiable account binding in a GROUP device KeyPackage and grafts the device leaf", async () => {
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const bobPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const carolPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);
    const bobAddr = canonicalAddress(bobPair.address);
    const carolAddr = canonicalAddress(carolPair.address);

    // alice has TWO devices; each holds its OWN signer (Wire mode, no read-only escrow).
    const aliceA = new OpenMlsEngine();
    const aliceB = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    const carol = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
    await aliceB.init(deviceLeafIdentity(aliceAddr, "b"));
    await bob.init(deviceLeafIdentity(bobAddr, "b1"));
    await carol.init(deviceLeafIdentity(carolAddr, "c1"));

    // every Wire engine installs its in-MLS E2EI binding at init (account key signs its stable leaf key)
    aliceA.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "a", aliceA.signaturePublicKey())));
    aliceB.setLeafBinding(alicePair.sign(leafKeyBindingBytes(aliceAddr, "b", aliceB.signaturePublicKey())));
    bob.setLeafBinding(bobPair.sign(leafKeyBindingBytes(bobAddr, "b1", bob.signaturePublicKey())));
    carol.setLeafBinding(carolPair.sign(leafKeyBindingBytes(carolAddr, "c1", carol.signaturePublicKey())));

    // a real >=3-account group
    aliceA.createGroupByConv(GROUP);
    const add = aliceA.addMembersByConv(GROUP, [bob.generateKeyPackage(), carol.generateKeyPackage()]);
    await bob.processWelcomeByConv(GROUP, add.welcome);
    await carol.processWelcomeByConv(GROUP, add.welcome);

    // aliceB publishes a GROUP KeyPackage; aliceA inspects its embedded E2EI binding.
    const aliceBKp = aliceB.generateKeyPackage();
    const binding = aliceA.keyPackageBinding(aliceBKp);
    expect(binding.identity).toBe(deviceLeafIdentity(aliceAddr, "b"));
    expect(binding.binding.length).toBeGreaterThan(0); // a binding IS embedded in the group KP

    // the binding verifies relay-trustlessly straight from the KeyPackage (account key signed it)
    const ok = await verifyLeafKeyBinding(aliceAddr, "b", binding.signatureKey, toHex(binding.binding));
    expect(ok).toBe(true);

    // graft aliceB's device leaf into the group; everyone follows; the new device can send
    const grow = aliceA.addMembersByConv(GROUP, [aliceBKp]);
    await aliceB.processWelcomeByConv(GROUP, grow.welcome);
    bob.processCommitByConv(GROUP, grow.commit);
    carol.processCommitByConv(GROUP, grow.commit);
    const epoch = aliceA.epochByConv(GROUP);
    expect(aliceB.epochByConv(GROUP)).toBe(epoch);

    const fromB = await aliceB.encrypt(GROUP, textEnvelope("m", "hi from group device B", {}));
    expect((await bob.decrypt(GROUP, fromB)).body).toMatchObject({ text: "hi from group device B" });
    expect((await carol.decrypt(GROUP, fromB)).body).toMatchObject({ text: "hi from group device B" });
  });

  it("rejects a forged group KeyPackage binding signed by a different account key", async () => {
    const kr = new Keyring({ type: "sr25519", ss58Format: 273 });
    const alicePair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const attackerPair: KeyringPair = kr.addFromMnemonic(mnemonicGenerate());
    const aliceAddr = canonicalAddress(alicePair.address);

    const aliceA = new OpenMlsEngine();
    const evil = new OpenMlsEngine();
    await aliceA.init(deviceLeafIdentity(aliceAddr, "a"));
    await evil.init(deviceLeafIdentity(aliceAddr, "evil")); // claims alice's account...

    // ...but the binding is signed by the ATTACKER's key, not alice's account key.
    const leafKey = evil.signaturePublicKey();
    evil.setLeafBinding(attackerPair.sign(leafKeyBindingBytes(aliceAddr, "evil", leafKey)));

    aliceA.createGroupByConv(GROUP);
    const evilKp = evil.generateKeyPackage();
    const binding = aliceA.keyPackageBinding(evilKp);
    expect(binding.identity).toBe(deviceLeafIdentity(aliceAddr, "evil"));

    // member-side verification fails → the forged device leaf must NOT be accepted into the group.
    const ok = await verifyLeafKeyBinding(aliceAddr, "evil", binding.signatureKey, toHex(binding.binding));
    expect(ok).toBe(false);
  });
});
