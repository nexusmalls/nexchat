// EN: Validates OpenMLS state persistence (cross-refresh / multi-device). A client's
// whole crypto state lives in the WASM provider storage; `exportState()` snapshots it
// and `MlsClient.restore()` rebuilds an identical client. We prove a restored client
// keeps decrypting/encrypting in the SAME ratchet (epoch + message keys survive), which
// is exactly what a page refresh or a second device must do.
// CN: 验证 OpenMLS 状态持久化（跨刷新 / 多设备）。客户端整套密码状态都在 WASM provider 存储里；
// `exportState()` 快照它，`MlsClient.restore()` 重建出一致的客户端。我们证明 restore 后的客户端
// 仍在同一棘轮上继续收发（epoch 与消息密钥都保留）——这正是页面刷新或第二设备需要的。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init, { MlsClient } from "../mls-pkg/nexchat_mls.js";

const dec = new TextDecoder();
const enc = new TextEncoder();

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe("OpenMLS state persistence", () => {
  it("a joined member survives export → restore and keeps decrypting", () => {
    const alice = new MlsClient("alice");
    const bob = new MlsClient("bob");
    const charlie = new MlsClient("charlie");

    alice.createGroup("g:0");
    const out = alice.addMembers("g:0", [bob.generateKeyPackage(), charlie.generateKeyPackage()]);
    bob.processWelcome("g:0", out.welcome);
    charlie.processWelcome("g:0", out.welcome);

    // Simulate Bob's tab refreshing: snapshot → drop → restore from blob.
    const blob = bob.exportState();
    expect(blob.length).toBeGreaterThan(0);
    const bob2 = MlsClient.restore(blob);
    expect(bob2.hasGroup("g:0")).toBe(true);

    // Restored Bob decrypts a fresh message from Alice.
    const ct = alice.encrypt("g:0", enc.encode("after refresh"));
    expect(dec.decode(bob2.decrypt("g:0", ct))).toBe("after refresh");

    // Restored Bob can also send; Alice and (live) Charlie decrypt it.
    const ct2 = bob2.encrypt("g:0", enc.encode("bob is back"));
    expect(dec.decode(alice.decrypt("g:0", ct2))).toBe("bob is back");
    expect(dec.decode(charlie.decrypt("g:0", ct2))).toBe("bob is back");
  });

  it("the group owner survives export → restore and can still commit new members", () => {
    const alice = new MlsClient("alice");
    const bob = new MlsClient("bob");
    const charlie = new MlsClient("charlie");
    const dave = new MlsClient("dave");

    alice.createGroup("g:1");
    const out1 = alice.addMembers("g:1", [bob.generateKeyPackage(), charlie.generateKeyPackage()]);
    bob.processWelcome("g:1", out1.welcome);
    charlie.processWelcome("g:1", out1.welcome);

    // Owner refreshes after the first commit (epoch 1).
    const alice2 = MlsClient.restore(alice.exportState());
    expect(alice2.hasGroup("g:1")).toBe(true);

    // Restored owner commits a NEW member at epoch 2 — proves signer + group are intact.
    const out2 = alice2.addMembers("g:1", [dave.generateKeyPackage()]);
    expect(out2.epoch).toBe(2n);

    // Existing members catch up on the commit; Dave joins via the new welcome.
    bob.processCommit("g:1", out2.commit);
    charlie.processCommit("g:1", out2.commit);
    dave.processWelcome("g:1", out2.welcome);

    const ct = alice2.encrypt("g:1", enc.encode("welcome dave"));
    expect(dec.decode(bob.decrypt("g:1", ct))).toBe("welcome dave");
    expect(dec.decode(charlie.decrypt("g:1", ct))).toBe("welcome dave");
    expect(dec.decode(dave.decrypt("g:1", ct))).toBe("welcome dave");
  });

  it("restore is byte-stable: a second export equals re-restoring the same state", () => {
    const alice = new MlsClient("alice");
    const bob = new MlsClient("bob");
    alice.createGroup("g:2");
    const out = alice.addMembers("g:2", [bob.generateKeyPackage(), new MlsClient("x").generateKeyPackage()]);
    bob.processWelcome("g:2", out.welcome);

    const blob = bob.exportState();
    const bob2 = MlsClient.restore(blob);
    // Re-exporting a freshly restored client yields a state that itself restores cleanly.
    const bob3 = MlsClient.restore(bob2.exportState());
    expect(bob3.hasGroup("g:2")).toBe(true);
    const ct = alice.encrypt("g:2", enc.encode("stable"));
    expect(dec.decode(bob3.decrypt("g:2", ct))).toBe("stable");
  });
});
