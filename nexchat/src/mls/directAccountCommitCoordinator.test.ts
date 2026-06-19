import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  DirectAccountCommitCoordinator,
  type WireCommitExecutor,
} from "@/mls/directAccountCommitCoordinator";
import { accountSelfConvId } from "@/mls/directCommitCoordination";
import type {
  CommitRejectInbound,
  ControlInbound,
  ControlMsg,
  RelayClient,
  RelayFrame,
  RelayInbound,
} from "@/relay/relayClient";

const ACCOUNT = "5AliceAddr";
const DM = "d:5AliceAddr:5BobAddr";

class MockRelay implements RelayClient {
  sent: ControlMsg[] = [];
  ctrl: ControlInbound | null = null;
  commitReject: CommitRejectInbound | null = null;
  async connect(): Promise<void> {}
  disconnect(): void {}
  async send(_frame: RelayFrame): Promise<void> {}
  onMessage(_cb: RelayInbound): void {}
  async sendControl(msg: ControlMsg): Promise<void> {
    this.sent.push(msg);
  }
  onControl(cb: ControlInbound): void {
    this.ctrl = cb;
  }
  onCommitReject(cb: CommitRejectInbound): void {
    this.commitReject = cb;
  }
  lastCommit(): Extract<ControlMsg, { t: "commit" }> | undefined {
    return [...this.sent].reverse().find((m) => m.t === "commit") as never;
  }
  commits(): Extract<ControlMsg, { t: "commit" }>[] {
    return this.sent.filter((m) => m.t === "commit") as never;
  }
  results(): Extract<ControlMsg, { t: "commit_result" }>[] {
    return this.sent.filter((m) => m.t === "commit_result") as never;
  }
}

function intentFrame(reqId: string): ControlMsg {
  return {
    t: "commit_intent",
    convId: accountSelfConvId(ACCOUNT),
    from: "ep-other",
    req_id: reqId,
    kind: "add_device",
    payload: { dmConvId: DM, kp: "kpB64" },
  } as ControlMsg;
}

const SETTLE_MS = 100;

let relay: MockRelay;
let executor: WireCommitExecutor;
let runIntent: ReturnType<typeof vi.fn>;
let catchUp: ReturnType<typeof vi.fn>;
let deliverWelcome: ReturnType<typeof vi.fn>;
let commitAccepted: ReturnType<typeof vi.fn>;
let commitAbandoned: ReturnType<typeof vi.fn>;
let requestCatchUp: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers();
  relay = new MockRelay();
  runIntent = vi.fn(async () => ({ commitB64: "cm0", welcomeB64: "wc0", preEpoch: 3 }));
  catchUp = vi.fn(async () => ({ commitB64: "cm1", welcomeB64: "wc1", newEpoch: 5 }));
  deliverWelcome = vi.fn(async () => {});
  commitAccepted = vi.fn(async () => {});
  commitAbandoned = vi.fn(async () => {});
  requestCatchUp = vi.fn();
  executor = {
    runIntent: runIntent as never,
    catchUpAndRerun: catchUp as never,
    deliverWelcome: deliverWelcome as never,
    commitAccepted: commitAccepted as never,
    commitAbandoned: commitAbandoned as never,
    requestCatchUp: requestCatchUp as never,
  };
});

afterEach(() => {
  vi.useRealTimers();
});

function makeCoordinator(): DirectAccountCommitCoordinator {
  const c = new DirectAccountCommitCoordinator({
    relay,
    account: ACCOUNT,
    deviceId: "dev-a", // single device → always CD
    endpointId: "ep-a",
    executor,
    settleMs: SETTLE_MS,
  });
  c.wire();
  return c;
}

describe("DirectAccountCommitCoordinator lifecycle driver", () => {
  it("CD: intent → Wire commit with pre-epoch, settle → welcome + ok", async () => {
    makeCoordinator();
    relay.ctrl!(intentFrame("req-1"));
    await vi.advanceTimersByTimeAsync(0); // flush runIntent

    const commit = relay.lastCommit();
    expect(commit?.commit_epoch).toBe(3);
    expect(commit?.commit).toBe("cm0");
    expect(typeof commit?.msgId).toBe("string");

    await vi.advanceTimersByTimeAsync(SETTLE_MS); // implicit accept
    expect(commitAccepted).toHaveBeenCalledTimes(1); // staged commit merged on accept
    expect(deliverWelcome).toHaveBeenCalledWith(expect.anything(), "wc0");
    const ok = relay.results();
    expect(ok).toHaveLength(1);
    expect(ok[0]?.ok).toBe(true);
  });

  it("CD: epoch_stale → catch up + resend at new epoch, then settle → ok", async () => {
    makeCoordinator();
    relay.ctrl!(intentFrame("req-2"));
    await vi.advanceTimersByTimeAsync(0);

    const first = relay.lastCommit()!;
    // relay rejects the first commit
    relay.commitReject!({
      reason: "epoch_stale",
      convId: DM,
      current_epoch: 5,
      msgId: first.msgId,
    });
    await vi.advanceTimersByTimeAsync(0); // flush catchUpAndRerun

    expect(catchUp).toHaveBeenCalledWith(expect.anything(), 5);
    const commits = relay.commits();
    expect(commits).toHaveLength(2);
    const resent = commits[1]!;
    expect(resent.commit_epoch).toBe(5);
    expect(resent.commit).toBe("cm1");
    expect(resent.msgId).not.toBe(first.msgId);

    // no result yet (awaiting accept of the resend)
    expect(relay.results()).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(SETTLE_MS);
    expect(relay.results()[0]?.ok).toBe(true);
    expect(deliverWelcome).toHaveBeenCalledWith(expect.anything(), "wc1");
  });

  it("CD: epoch_stale → polls until the engine catches up, then resends", async () => {
    const c = new DirectAccountCommitCoordinator({
      relay,
      account: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: SETTLE_MS,
      catchupPollMs: 10,
      maxCatchupPolls: 5,
    });
    c.wire();
    // not caught up on the first try, then caught up
    catchUp
      .mockRejectedValueOnce(new Error("epoch 4 < 5; awaiting winner"))
      .mockResolvedValueOnce({ commitB64: "cm1", welcomeB64: "wc1", newEpoch: 5 });

    relay.ctrl!(intentFrame("req-poll"));
    await vi.advanceTimersByTimeAsync(0);
    const first = relay.lastCommit()!;
    relay.commitReject!({ reason: "epoch_stale", convId: DM, current_epoch: 5, msgId: first.msgId });
    await vi.advanceTimersByTimeAsync(0); // first catchUp attempt → throws → schedules poll
    expect(relay.commits()).toHaveLength(1); // no resend yet
    // on the FIRST miss the coordinator actively pulls the conv's backlog from the relay (once)
    expect(requestCatchUp).toHaveBeenCalledTimes(1);
    expect(requestCatchUp).toHaveBeenCalledWith(DM);
    await vi.advanceTimersByTimeAsync(10); // poll fires → catchUp succeeds → resend
    expect(catchUp).toHaveBeenCalledTimes(2);
    const commits = relay.commits();
    expect(commits).toHaveLength(2);
    expect(commits[1]?.commit_epoch).toBe(5);
    // not fired again on subsequent attempts
    expect(requestCatchUp).toHaveBeenCalledTimes(1);
  });

  it("CD: epoch_stale → catch-up never lands → give up (ok:false) + abandon", async () => {
    const c = new DirectAccountCommitCoordinator({
      relay,
      account: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: SETTLE_MS,
      catchupPollMs: 10,
      maxCatchupPolls: 2,
    });
    c.wire();
    catchUp.mockRejectedValue(new Error("never caught up"));

    relay.ctrl!(intentFrame("req-fail"));
    await vi.advanceTimersByTimeAsync(0);
    const first = relay.lastCommit()!;
    relay.commitReject!({ reason: "epoch_stale", convId: DM, current_epoch: 7, msgId: first.msgId });
    await vi.advanceTimersByTimeAsync(0); // attempt 0 throws → poll
    await vi.advanceTimersByTimeAsync(10); // attempt 1 throws → poll
    await vi.advanceTimersByTimeAsync(10); // attempt 2 throws → exhausted → give up
    expect(relay.commits()).toHaveLength(1); // never resent
    const result = relay.results();
    expect(result).toHaveLength(1);
    expect(result[0]?.ok).toBe(false);
    expect(commitAbandoned).toHaveBeenCalled();
  });

  it("non-CD intent is ignored when not coordinator", async () => {
    const c = new DirectAccountCommitCoordinator({
      relay,
      account: ACCOUNT,
      deviceId: "dev-z",
      endpointId: "ep-z",
      executor,
      settleMs: SETTLE_MS,
    });
    c.wire();
    // a lexicographically smaller device is online → dev-z is NOT CD after settle
    relay.ctrl!({ t: "presence", convId: accountSelfConvId(ACCOUNT), device_id: "dev-a", online: true } as ControlMsg);
    await vi.advanceTimersByTimeAsync(2500); // CD_SETTLE_MS settle + election tick
    relay.ctrl!(intentFrame("req-3"));
    await vi.advanceTimersByTimeAsync(0);
    expect(runIntent).not.toHaveBeenCalled();
  });
});
