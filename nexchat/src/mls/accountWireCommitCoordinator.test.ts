import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createUnifiedWireAccountCoordinator } from "@/mls/accountWireCommitCoordinator";
import type { WireSessionJoinBridge } from "@/mls/accountWireCommitCoordinator";
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
const G1 = "g:42";

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
  offers(): Extract<ControlMsg, { t: "device_join_offer" }>[] {
    return this.sent.filter((m) => m.t === "device_join_offer") as never;
  }
}

function joinRequest(fromDevice: string): ControlMsg {
  return {
    t: "device_join_request",
    convId: accountSelfConvId(ACCOUNT),
    device_id: fromDevice,
  } as ControlMsg;
}

function intentFrame(reqId: string, convId: string): ControlMsg {
  return {
    t: "commit_intent",
    convId: accountSelfConvId(ACCOUNT),
    from: "ep-other",
    req_id: reqId,
    kind: "add_device",
    payload: { dmConvId: convId, kp: "kpB64" },
  } as ControlMsg;
}

function bridge(convs: string[]): WireSessionJoinBridge {
  return {
    listJoinableConvs: () => convs,
    handleDeviceJoinOffer: vi.fn(async () => {}),
    handleDeviceJoinKp: vi.fn(async () => {}),
  };
}

let relay: MockRelay;
let runIntent: ReturnType<typeof vi.fn>;
let onGroupExecuteIntent: ReturnType<typeof vi.fn>;
let directBridge: WireSessionJoinBridge;
let groupBridge: WireSessionJoinBridge;

beforeEach(() => {
  vi.useFakeTimers();
  relay = new MockRelay();
  runIntent = vi.fn(async () => ({ commitB64: "cm0", welcomeB64: "wc0", preEpoch: 1 }));
  onGroupExecuteIntent = vi.fn(async () => {});
  directBridge = bridge([DM]);
  groupBridge = bridge([G1]);
});

afterEach(() => {
  vi.useRealTimers();
});

function stubExecutor() {
  return {
    runIntent: runIntent as never,
    commitAccepted: vi.fn(async () => {}),
    commitAbandoned: vi.fn(async () => {}),
    catchUpAndRerun: vi.fn(async () => ({ commitB64: "cm1", welcomeB64: "wc1", newEpoch: 2 })),
    deliverWelcome: vi.fn(async () => {}),
  };
}

function makeUnified() {
  const c = createUnifiedWireAccountCoordinator({
    relay,
    account: ACCOUNT,
    deviceId: "dev-cd",
    endpointId: "ep-cd",
    directExecutor: stubExecutor(),
    getDirectBridge: () => directBridge,
    getGroupBridge: () => groupBridge,
    onGroupExecuteIntent: onGroupExecuteIntent as never,
  });
  c.wire();
  return c;
}

describe("createUnifiedWireAccountCoordinator", () => {
  it("merges d: + g: convs into one device_join_offer", async () => {
    makeUnified();
    relay.ctrl!(joinRequest("dev-new"));
    await vi.advanceTimersByTimeAsync(0);

    const offers = relay.offers();
    expect(offers).toHaveLength(1);
    expect(offers[0]?.device_id).toBe("dev-new");
    expect(offers[0]?.conv_ids).toEqual([DM, G1]);
  });

  it("fans device_join_offer to both session bridges", async () => {
    makeUnified();
    const msg = {
      t: "device_join_offer" as const,
      convId: accountSelfConvId(ACCOUNT),
      device_id: "dev-cd",
      conv_ids: [DM, G1],
    };
    relay.ctrl!(msg as ControlMsg);
    await vi.advanceTimersByTimeAsync(0);

    expect(directBridge.handleDeviceJoinOffer).toHaveBeenCalledWith(msg);
    expect(groupBridge.handleDeviceJoinOffer).toHaveBeenCalledWith(msg);
  });

  it("fans device_join_kp to both session bridges", async () => {
    makeUnified();
    const msg = {
      t: "device_join_kp" as const,
      convId: accountSelfConvId(ACCOUNT),
      device_id: "dev-new",
      kps: [{ conv_id: DM, kp: "kp1" }],
    };
    relay.ctrl!(msg as ControlMsg);
    await vi.advanceTimersByTimeAsync(0);

    expect(directBridge.handleDeviceJoinKp).toHaveBeenCalledTimes(1);
    expect(groupBridge.handleDeviceJoinKp).toHaveBeenCalledTimes(1);
    expect(directBridge.handleDeviceJoinKp).toHaveBeenCalledWith(
      expect.objectContaining({ t: "device_join_kp", device_id: "dev-new" }),
    );
    expect(groupBridge.handleDeviceJoinKp).toHaveBeenCalledWith(
      expect.objectContaining({ t: "device_join_kp", device_id: "dev-new" }),
    );
  });

  it("routes commit_intent: d: → relay executor, g: → onGroupExecuteIntent", async () => {
    makeUnified();
    relay.ctrl!(intentFrame("req-d", DM));
    relay.ctrl!(intentFrame("req-g", G1));
    await vi.advanceTimersByTimeAsync(0);

    expect(runIntent).toHaveBeenCalledTimes(1);
    expect(runIntent.mock.calls[0]?.[0]?.payload?.dmConvId).toBe(DM);
    expect(onGroupExecuteIntent).toHaveBeenCalledTimes(1);
    expect(onGroupExecuteIntent.mock.calls[0]?.[0]?.payload?.dmConvId).toBe(G1);
  });
});
