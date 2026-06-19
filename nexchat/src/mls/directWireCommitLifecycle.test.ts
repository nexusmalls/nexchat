import { describe, expect, it } from "vitest";
import {
  reduceCommit,
  startCommit,
  type CommitAttempt,
  type CommitLifecycleState,
} from "@/mls/directWireCommitLifecycle";
import { MAX_COMMIT_RETRY } from "@/mls/directCommitCoordination";

const attempt: CommitAttempt = {
  convId: "d:a:b",
  reqId: "req-1",
  kind: "add_device",
  payload: { dmConvId: "d:a:b", kp: "kpB64" },
};

function fresh(): CommitLifecycleState {
  return startCommit({ attempt, commitEpoch: 3, msgId: "m-0" });
}

describe("reduceCommit", () => {
  it("settle_timeout → deliver welcome + ok (implicit accept)", () => {
    const step = reduceCommit(fresh(), { t: "settle_timeout", msgId: "m-0" });
    expect(step.state.phase).toBe("delivered");
    expect(step.actions).toEqual([{ t: "deliver_welcome_and_ok" }]);
  });

  it("ignores settle_timeout for a stale msgId", () => {
    const step = reduceCommit(fresh(), { t: "settle_timeout", msgId: "OTHER" });
    expect(step.state.phase).toBe("awaiting");
    expect(step.actions).toEqual([]);
  });

  it("epoch_stale → catch_up_and_retry while budget remains", () => {
    const step = reduceCommit(fresh(), { t: "epoch_stale", msgId: "m-0", currentEpoch: 5 });
    expect(step.state.phase).toBe("awaiting");
    expect(step.actions).toEqual([{ t: "catch_up_and_retry", currentEpoch: 5 }]);
  });

  it("caught_up → resend with new epoch + msgId, increments retries", () => {
    let s = fresh();
    s = reduceCommit(s, { t: "epoch_stale", msgId: "m-0", currentEpoch: 5 }).state;
    const step = reduceCommit(s, {
      t: "caught_up",
      newEpoch: 5,
      commitB64: "cm1",
      welcomeB64: "wc1",
      newMsgId: "m-1",
    });
    expect(step.state.commitEpoch).toBe(5);
    expect(step.state.msgId).toBe("m-1");
    expect(step.state.retries).toBe(1);
    expect(step.actions).toEqual([
      { t: "resend_commit", commitEpoch: 5, commitB64: "cm1", welcomeB64: "wc1", msgId: "m-1" },
    ]);
  });

  it("gives up after MAX_COMMIT_RETRY stale rounds", () => {
    let s = fresh();
    // drive retries up to the budget
    for (let i = 0; i < MAX_COMMIT_RETRY; i++) {
      const stale = reduceCommit(s, { t: "epoch_stale", msgId: s.msgId, currentEpoch: 5 + i });
      expect(stale.actions[0]?.t).toBe("catch_up_and_retry");
      s = reduceCommit(s, {
        t: "caught_up",
        newEpoch: 5 + i,
        commitB64: "cm",
        welcomeB64: "wc",
        newMsgId: `m-${i + 1}`,
      }).state;
    }
    expect(s.retries).toBe(MAX_COMMIT_RETRY);
    const giveUp = reduceCommit(s, { t: "epoch_stale", msgId: s.msgId, currentEpoch: 99 });
    expect(giveUp.state.phase).toBe("failed");
    expect(giveUp.actions).toEqual([{ t: "reply_give_up", currentEpoch: 99 }]);
  });

  it("is inert once delivered or failed", () => {
    const delivered = reduceCommit(fresh(), { t: "settle_timeout", msgId: "m-0" }).state;
    expect(reduceCommit(delivered, { t: "epoch_stale", msgId: "m-0", currentEpoch: 9 }).actions).toEqual([]);
  });
});
