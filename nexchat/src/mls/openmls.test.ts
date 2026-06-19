// EN: Validates the real OpenMLS (RFC 9420) WASM engine end-to-end: three independent
// clients run the actual handshake (create_group → add_members commit+welcome →
// process_welcome) and exchange AEAD-encrypted application messages. This is the proof
// that swapping the WebCrypto placeholder for OpenMLS works.
// CN: 端到端验证真实 OpenMLS(RFC 9420) WASM 引擎：三个独立客户端跑真实握手（建群→加人
// commit+welcome→处理 welcome）并互发 AEAD 加密应用消息。证明用 OpenMLS 替换 WebCrypto 占位可行。

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

describe("OpenMLS WASM engine", () => {
  it("cipher suite is MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519 (IANA 1)", () => {
    const alice = new MlsClient("alice");
    expect(alice.cipherSuite()).toBe(1);
  });

  it("group lifecycle: create → add 2 → welcome → application round-trip", () => {
    const alice = new MlsClient("alice");
    const bob = new MlsClient("bob");
    const charlie = new MlsClient("charlie");

    const bobKp = bob.generateKeyPackage();
    const charlieKp = charlie.generateKeyPackage();
    expect(bobKp.length).toBeGreaterThan(0);

    const fp0 = alice.createGroup("g:0");
    expect(fp0.epoch).toBe(0n);
    expect(fp0.tree_hash.length).toBe(32);

    // first commit adds >=2 (1 -> 3), mirroring the on-chain TwoMemberGroupForbidden rule
    const out = alice.addMembers("g:0", [bobKp, charlieKp]);
    expect(out.epoch).toBe(1n);
    expect(out.commit.length).toBeGreaterThan(0);
    expect(out.welcome.length).toBeGreaterThan(0);

    // new members join by processing the SAME welcome (each finds its own secrets)
    bob.processWelcome("g:0", out.welcome);
    charlie.processWelcome("g:0", out.welcome);
    expect(bob.hasGroup("g:0")).toBe(true);
    expect(charlie.hasGroup("g:0")).toBe(true);

    // Alice → group: both others decrypt the same ciphertext
    const ct = alice.encrypt("g:0", enc.encode("hello group"));
    expect(dec.decode(bob.decrypt("g:0", ct))).toBe("hello group");
    expect(dec.decode(charlie.decrypt("g:0", ct))).toBe("hello group");

    // Bob → group: Alice and Charlie decrypt
    const ct2 = bob.encrypt("g:0", enc.encode("bob here"));
    expect(dec.decode(alice.decrypt("g:0", ct2))).toBe("bob here");
    expect(dec.decode(charlie.decrypt("g:0", ct2))).toBe("bob here");
  });

  it("a non-member cannot decrypt the group ciphertext", () => {
    const alice = new MlsClient("alice");
    const bob = new MlsClient("bob");
    const charlie = new MlsClient("charlie");
    const eve = new MlsClient("eve");

    alice.createGroup("g:1");
    alice.addMembers("g:1", [bob.generateKeyPackage(), charlie.generateKeyPackage()]);
    const ct = alice.encrypt("g:1", enc.encode("secret"));

    // Eve has no group bound to g:1 → decrypt throws.
    expect(() => eve.decrypt("g:1", ct)).toThrow();
  });
});
