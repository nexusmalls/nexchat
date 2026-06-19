import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DirectWireSession } from "@/mls/directWireSession";
import type { WireCommitExecutor } from "@/mls/directAccountCommitCoordinator";
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
  async connect(): Promise<void> {}
  disconnect(): void {}
  async send(_f: RelayFrame): Promise<void> {}
  onMessage(_cb: RelayInbound): void {}
  async sendControl(m: ControlMsg): Promise<void> {
    this.sent.push(m);
  }
  onControl(cb: ControlInbound): void {
    this.ctrl = cb;
  }
  onCommitReject(_cb: CommitRejectInbound): void {}
  presence(): Extract<ControlMsg, { t: "presence" }>[] {
    return this.sent.filter((m) => m.t === "presence") as never;
  }
}

let relay: MockRelay;
let executor: WireCommitExecutor;
let runIntent: ReturnType<typeof vi.fn>;
let commitAccepted: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers();
  relay = new MockRelay();
  runIntent = vi.fn(async () => ({ commitB64: "cm0", welcomeB64: "wc0", preEpoch: 2 }));
  commitAccepted = vi.fn(async () => {});
  executor = {
    runIntent: runIntent as never,
    commitAccepted: commitAccepted as never,
    commitAbandoned: vi.fn(async () => {}) as never,
    catchUpAndRerun: vi.fn(async () => ({ commitB64: "x", welcomeB64: "y", newEpoch: 9 })) as never,
    deliverWelcome: vi.fn(async () => {}) as never,
  };
});

afterEach(() => {
  vi.useRealTimers();
});

function makeSession(): DirectWireSession {
  return new DirectWireSession({
    engine: {} as never, // unused: a custom executor is injected
    relay,
    selfAddress: ACCOUNT,
    deviceId: "dev-a", // single device → always CD
    endpointId: "ep-a",
    executor,
    settleMs: 50,
  });
}

describe("DirectWireSession orchestrator", () => {
  it("broadcasts presence online on start and offline on stop", () => {
    const s = makeSession();
    s.start();
    expect(relay.presence().at(-1)).toMatchObject({
      convId: accountSelfConvId(ACCOUNT),
      device_id: "dev-a",
      online: true,
    });
    s.stop();
    expect(relay.presence().at(-1)).toMatchObject({ device_id: "dev-a", online: false });
  });

  it("as sole device (CD) executes a local add_device intent end-to-end", async () => {
    const s = makeSession();
    s.start();

    const route = await s.addDevice(DM, "newDeviceKpB64");
    expect(route).toBe("execute");
    await vi.advanceTimersByTimeAsync(0); // flush runIntent

    expect(runIntent).toHaveBeenCalledTimes(1);
    const commit = relay.sent.find((m) => m.t === "commit") as Extract<ControlMsg, { t: "commit" }>;
    expect(commit?.commit_epoch).toBe(2);
    expect(commit?.commit).toBe("cm0");

    await vi.advanceTimersByTimeAsync(50); // settle → implicit accept → merge
    expect(commitAccepted).toHaveBeenCalledTimes(1);
    const result = relay.sent.find((m) => m.t === "commit_result") as Extract<
      ControlMsg,
      { t: "commit_result" }
    >;
    expect(result?.ok).toBe(true);
  });

  it("rekey routes through the same local-execute path", async () => {
    const s = makeSession();
    s.start();
    runIntent.mockResolvedValueOnce({ commitB64: "rk", welcomeB64: "", preEpoch: 2 });
    await s.rekey(DM);
    await vi.advanceTimersByTimeAsync(0);
    expect(runIntent).toHaveBeenCalledWith(expect.objectContaining({ kind: "rekey" }));
    const commit = relay.sent.find((m) => m.t === "commit") as Extract<ControlMsg, { t: "commit" }>;
    expect(commit?.commit).toBe("rk");
  });

  it("requestPeerAdd cold-start fallback fires onPeerAddTimeout when no graft lands (deadlock fix)", async () => {
    const onPeerAddTimeout = vi.fn();
    let groupHeld = false;
    const s = new DirectWireSession({
      engine: {
        generateKeyPackage: () => new Uint8Array([1, 2, 3]),
        hasGroup: () => groupHeld,
      } as never,
      relay,
      selfAddress: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: 50,
      peerAddFallbackMs: 100,
      onPeerAddTimeout,
    });
    s.start();

    const conv = await s.requestPeerAdd("5BobAddr");
    expect(relay.sent.some((m) => m.t === "peer_add_req")).toBe(true);
    expect(onPeerAddTimeout).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(100);
    expect(onPeerAddTimeout).toHaveBeenCalledWith("5BobAddr", conv);
  });

  it("requestPeerAdd fallback is suppressed once the conv is actually grafted (no fork)", async () => {
    const onPeerAddTimeout = vi.fn();
    let groupHeld = false;
    const s = new DirectWireSession({
      engine: {
        generateKeyPackage: () => new Uint8Array([1, 2, 3]),
        hasGroup: () => groupHeld,
      } as never,
      relay,
      selfAddress: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: 50,
      peerAddFallbackMs: 100,
      onPeerAddTimeout,
    });
    s.start();

    const conv = await s.requestPeerAdd("5BobAddr");
    // EN: a graft Welcome lands → markGrafted records the conv in graftedConvs (genuine convergence).
    // CN: 嫁接 Welcome 到达 → markGrafted 把会话记入 graftedConvs（真正收敛）。
    groupHeld = true;
    relay.ctrl?.({ t: "welcome", from: "ep-bob", to: "", toAddr: ACCOUNT, convId: conv, welcome: "wcX" });

    await vi.advanceTimersByTimeAsync(100);
    expect(onPeerAddTimeout).not.toHaveBeenCalled();
  });

  it("requestPeerAdd still fires fallback when a stale group is held but never grafted", async () => {
    const onPeerAddTimeout = vi.fn();
    const s = new DirectWireSession({
      engine: {
        generateKeyPackage: () => new Uint8Array([1, 2, 3]),
        // EN: a stale/half-established group persists locally, but no graft Welcome ever converged.
        // CN: 本地残留过期/半建立群，但从未有嫁接 Welcome 收敛。
        hasGroup: () => true,
      } as never,
      relay,
      selfAddress: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: 50,
      peerAddFallbackMs: 100,
      onPeerAddTimeout,
    });
    s.start();

    const conv = await s.requestPeerAdd("5BobAddr");
    await vi.advanceTimersByTimeAsync(100);
    expect(onPeerAddTimeout).toHaveBeenCalledWith("5BobAddr", conv);
  });

  it("adoptRestoredGroup marks a held conv graft-owned without sending peer_add_req (no fork)", async () => {
    const onGraftConvs = vi.fn();
    const s = new DirectWireSession({
      engine: {
        generateKeyPackage: () => new Uint8Array([1, 2, 3]),
        // EN: group restored from persistence — we are already a member. CN: 群从持久化恢复——我们已是成员。
        hasGroup: () => true,
      } as never,
      relay,
      selfAddress: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: 50,
      peerAddFallbackMs: 100,
      onGraftConvs,
    });
    s.start();

    expect(s.adoptRestoredGroup(DM)).toBe(true);
    // EN: adopting must NOT request a re-graft (would bump the epoch under our joined leaf → fork).
    // CN: 采纳绝不能请求重新嫁接（会在我们已加入的 leaf 之上抬升 epoch → 分叉）。
    expect(relay.sent.some((m) => m.t === "peer_add_req")).toBe(false);
    // EN: it tells the host the conv is graft-owned → registry reports ready via hasGroup.
    // CN: 它告知宿主该会话归嫁接拥有 → registry 按 hasGroup 报告就绪。
    expect(onGraftConvs).toHaveBeenCalledWith([DM]);
  });

  it("adoptRestoredGroup is a no-op (false) when no local group is held", () => {
    const s = new DirectWireSession({
      engine: {
        generateKeyPackage: () => new Uint8Array([1, 2, 3]),
        hasGroup: () => false,
      } as never,
      relay,
      selfAddress: ACCOUNT,
      deviceId: "dev-a",
      endpointId: "ep-a",
      executor,
      settleMs: 50,
    });
    s.start();
    expect(s.adoptRestoredGroup(DM)).toBe(false);
  });
});
