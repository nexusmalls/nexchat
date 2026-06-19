// EN: Unit test for the live-chain `SubmitGroupCommit` factory (CHAT_GROUP_WIREIFY_DESIGN §7/§15.3/G3c).
// Drives `createChainSubmitGroupCommit` against a FAKE chain + FAKE staged-fingerprint engine to pin:
// the extrinsic arg shape (empty member_delta + empty welcomes + G3b post-commit hashes), the ok →
// `newEpoch` mapping, the `EpochStale` → `{ epoch_stale, currentEpoch }` mapping (winner epoch read from
// the snapshot), a non-stale chain error → `{ error }`, and a missing staged commit → `{ error }`.
// CN: 实链 `SubmitGroupCommit` 工厂单元测试（设计 §7/§15.3/G3c）。用**假**链 + **假**暂存指纹引擎驱动
// `createChainSubmitGroupCommit`，钉住：extrinsic 入参形状（空 member_delta + 空 welcomes + G3b 后置哈希）、
// ok → `newEpoch` 映射、`EpochStale` → `{ epoch_stale, currentEpoch }`（胜出 epoch 取自快照）、非 stale 链错误
// → `{ error }`、无暂存 commit → `{ error }`。

import { describe, expect, it, vi } from "vitest";
import {
  createChainSubmitGroupCommit,
  groupIdOfConv,
  isEpochStaleError,
  type GroupCommitChain,
} from "@/mls/chainSubmitGroupCommit";
import { accountSelfConvId, type CommitIntentControlMsg } from "@/mls/directCommitCoordination";

const ADDR = "5AliceAddr";
const GROUP = "g:42";

function addDeviceIntent(): CommitIntentControlMsg {
  return {
    t: "commit_intent",
    convId: accountSelfConvId(ADDR),
    from: "ep-a",
    req_id: "req-1",
    kind: "add_device",
    payload: { dmConvId: GROUP, kp: "AAAA" },
  };
}

const FP = {
  treeHash: new Uint8Array(32).fill(0xaa),
  transcriptHash: new Uint8Array(32).fill(0xbb),
  epoch: 4,
};
const fpEngine = { stagedCommitFingerprintByConv: () => FP };

function fakeChain(over: Partial<GroupCommitChain> = {}): GroupCommitChain {
  return {
    signAndSendDev: vi.fn(async () => "0xblock"),
    groupSnapshot: vi.fn(async () => ({ epoch: 0, memberCount: 3 })),
    ...over,
  };
}

describe("groupIdOfConv / isEpochStaleError", () => {
  it("parses g:<id> and rejects non-group / malformed", () => {
    expect(groupIdOfConv("g:42")).toBe(42);
    expect(() => groupIdOfConv("d:a:b")).toThrow();
    expect(() => groupIdOfConv("g:nope")).toThrow();
  });
  it("detects EpochStale dispatch errors", () => {
    expect(isEpochStaleError(new Error("chatGroup.commit failed: chatGroup.EpochStale"))).toBe(true);
    expect(isEpochStaleError(new Error("chatGroup.commit failed: chatGroup.GroupFrozen"))).toBe(false);
  });
});

describe("createChainSubmitGroupCommit", () => {
  it("submits an empty-delta device commit with G3b post-commit hashes and maps ok → newEpoch", async () => {
    const chain = fakeChain();
    const submit = createChainSubmitGroupCommit({ chain, engine: fpEngine });

    const res = await submit({ intent: addDeviceIntent(), commitB64: "AAEC", expectedEpoch: 3 });

    expect(res).toEqual({ ok: true, newEpoch: 4 });
    expect(chain.signAndSendDev).toHaveBeenCalledTimes(1);
    const [section, method, args] = (chain.signAndSendDev as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(section).toBe("chatGroup");
    expect(method).toBe("commit");
    expect(args[0]).toBe(42); // group id
    expect(args[1]).toBe(3); // expected_epoch
    expect(args[3]).toBe("0x" + "aa".repeat(32)); // new_tree_hash (G3b)
    expect(args[4]).toBe("0x" + "bb".repeat(32)); // new_transcript_hash (G3b)
    expect(args[6]).toEqual([]); // welcomes (off-chain)
    expect(args[7]).toEqual({ added: [], removed: [] }); // empty-delta device commit
  });

  it("maps EpochStale → epoch_stale with the winner's epoch from the snapshot", async () => {
    const chain = fakeChain({
      signAndSendDev: vi.fn(async () => {
        throw new Error("chatGroup.commit failed: chatGroup.EpochStale");
      }),
      groupSnapshot: vi.fn(async () => ({ epoch: 7, memberCount: 3 })),
    });
    const submit = createChainSubmitGroupCommit({ chain, engine: fpEngine });

    const res = await submit({ intent: addDeviceIntent(), commitB64: "AAEC", expectedEpoch: 3 });
    expect(res).toEqual({ ok: false, reason: "epoch_stale", currentEpoch: 7 });
  });

  it("surfaces a non-stale chain error", async () => {
    const err = new Error("chatGroup.commit failed: chatGroup.GroupFrozen");
    const chain = fakeChain({
      signAndSendDev: vi.fn(async () => {
        throw err;
      }),
    });
    const submit = createChainSubmitGroupCommit({ chain, engine: fpEngine });

    const res = await submit({ intent: addDeviceIntent(), commitB64: "AAEC", expectedEpoch: 3 });
    expect(res).toEqual({ ok: false, reason: "error", error: err });
  });

  it("returns an error when nothing is staged (fingerprint throws)", async () => {
    const chain = fakeChain();
    const throwingEngine = {
      stagedCommitFingerprintByConv: () => {
        throw new Error("no staged commit for g:42");
      },
    };
    const submit = createChainSubmitGroupCommit({ chain, engine: throwingEngine });

    const res = await submit({ intent: addDeviceIntent(), commitB64: "AAEC", expectedEpoch: 3 });
    expect(res.ok).toBe(false);
    expect(chain.signAndSendDev).not.toHaveBeenCalled();
  });
});
