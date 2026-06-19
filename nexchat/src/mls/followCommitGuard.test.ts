// EN: Unit tests for member-side E2EI follow guard policy — especially the tightened rule that
// `inspectCommitBindings` failures REJECT (no blind fall-through to processCommit).
// CN: 成员侧 E2EI 跟随守卫策略单测——重点覆盖收紧规则：`inspectCommitBindings` 失败即**拒绝**（不再 blind
// 放行到 processCommit）。

import { describe, expect, it, vi } from "vitest";
import {
  verifyIncomingCommitWithPolicy,
  type CommitInspectEngine,
} from "@/mls/followCommitGuard";

const CONV = "d:alice:bob";
const COMMIT = new Uint8Array([1, 2, 3]);

describe("verifyIncomingCommitWithPolicy inspect failures", () => {
  it("returns true when the engine has no inspectCommitBindings (legacy engine)", async () => {
    const engine: CommitInspectEngine = {};
    expect(await verifyIncomingCommitWithPolicy(engine, CONV, COMMIT, () => true)).toBe(true);
  });

  it("returns false and discards when inspectCommitBindings throws", async () => {
    const discard = vi.fn();
    const engine: CommitInspectEngine = {
      inspectCommitBindings: () => {
        throw new Error("epoch stale");
      },
      discardIncomingCommit: discard,
    };
    expect(await verifyIncomingCommitWithPolicy(engine, CONV, COMMIT, () => true)).toBe(false);
    expect(discard).toHaveBeenCalledWith(CONV);
  });

  it("returns true when inspect succeeds with no bound added leaves", async () => {
    const engine: CommitInspectEngine = {
      inspectCommitBindings: () => [],
    };
    expect(await verifyIncomingCommitWithPolicy(engine, CONV, COMMIT, () => true)).toBe(true);
  });
});
