// EN: Member-side E2EI inspect guard (P3) — TS/WASM coverage for `inspectCommitBindings`:
// re-inspecting the same commit is idempotent; a different commit while one is staged throws.
// CN: 成员侧 E2EI inspect 守卫（P3）——`inspectCommitBindings` 的 TS/WASM 覆盖：相同 commit 复 inspect
// 幂等；已有暂存时再 inspect 不同 commit 抛错。

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import init from "../mls-pkg/nexchat_mls.js";
import { OpenMlsEngine } from "@/mls/openMlsEngine";

const CONV = "d:alice:bob";

beforeAll(async () => {
  const wasmPath = fileURLToPath(new URL("../mls-pkg/nexchat_mls_bg.wasm", import.meta.url));
  await init({ module_or_path: readFileSync(wasmPath) });
});

describe("inspectCommitBindings staging guard", () => {
  it("is idempotent for the same commit and rejects a different commit while staged", async () => {
    const alice = new OpenMlsEngine();
    const bob = new OpenMlsEngine();
    await alice.init("alice#a");
    await bob.init("bob#b1");

    alice.createGroupByConv(CONV);
    const add = alice.addMembersByConv(CONV, [bob.generateKeyPackage()]);
    await bob.processWelcomeByConv(CONV, add.welcome);

    const rekey = alice.selfUpdateStagedByConv(CONV);
    alice.mergePendingByConv(CONV);

    const first = bob.inspectCommitBindings(CONV, rekey);
    const second = bob.inspectCommitBindings(CONV, rekey);
    expect(second).toEqual(first);

    expect(() => bob.inspectCommitBindings(CONV, new Uint8Array([0xde, 0xad]))).toThrow(
      /already staged/i,
    );

    bob.processCommitByConv(CONV, rekey);
    expect(bob.epochByConv(CONV)).toBe(alice.epochByConv(CONV));

    bob.discardIncomingCommit(CONV);
    // Slot cleared — duplicate-guard lifted. Re-inspecting the *same* commit bytes after discard is
    // unsupported (OpenMLS already processed the message once); callers use processCommit on success.
  });
});
