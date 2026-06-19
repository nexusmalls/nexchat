import { describe, expect, it } from "vitest";
import {
  accountFromSelfConv,
  accountSelfConvId,
  applyPresenceUpdate,
  buildCommitIntent,
  buildCommitResult,
  buildDeviceJoinKp,
  buildDeviceJoinOffer,
  buildDeviceJoinRequest,
  buildPresence,
  buildWireDmCommit,
  buildWireNewDeviceState,
  currentCoordinator,
  initCoordinatorElection,
  isCoordinatorDevice,
  isSelfCoordinator,
  parseAccountSelfControl,
  parseCommitReject,
  pickCoordinatorDevice,
  routeCommitIntent,
  shouldRetryCommitReject,
  tickCoordinatorElection,
  CD_SETTLE_MS,
  MAX_COMMIT_RETRY,
} from "@/mls/directCommitCoordination";
import type { ControlMsg } from "@/relay/relayClient";

const ACCOUNT = "5AliceAddr";

describe("pickCoordinatorDevice", () => {
  it("returns lexicographically smallest online device", () => {
    expect(pickCoordinatorDevice(["dev-b", "dev-a", "dev-c"])).toBe("dev-a");
  });

  it("returns null for empty set", () => {
    expect(pickCoordinatorDevice([])).toBeNull();
  });

  it("single device is always CD", () => {
    expect(isCoordinatorDevice(["only-me"], "only-me")).toBe(true);
    expect(isCoordinatorDevice(["only-me"], "other")).toBe(false);
  });
});

describe("presence settle + CD election", () => {
  it("does not flip CD until settle window elapses", () => {
    let state = initCoordinatorElection("dev-a", 0);
    state = applyPresenceUpdate(state, "dev-b", true, 100);
    expect(currentCoordinator(state)).toBe("dev-a");
    state = tickCoordinatorElection(state, 100 + CD_SETTLE_MS - 1);
    expect(currentCoordinator(state)).toBe("dev-a");
    state = tickCoordinatorElection(state, 100 + CD_SETTLE_MS);
    expect(currentCoordinator(state)).toBe("dev-a");
    expect(isSelfCoordinator(state, "dev-a")).toBe(true);
  });

  it("offline removes device from settled set after settle", () => {
    let state = initCoordinatorElection("dev-a", 0);
    state = applyPresenceUpdate(state, "dev-b", true, 0);
    state = tickCoordinatorElection(state, CD_SETTLE_MS);
    state = applyPresenceUpdate(state, "dev-a", false, CD_SETTLE_MS + 1);
    state = tickCoordinatorElection(state, CD_SETTLE_MS * 2 + 2);
    expect(currentCoordinator(state)).toBe("dev-b");
    expect(isSelfCoordinator(state, "dev-b")).toBe(true);
  });
});

describe("account self-channel codecs", () => {
  it("round-trips presence / intent / result", () => {
    const presence = buildPresence({ account: ACCOUNT, deviceId: "dev-a", online: true });
    expect(parseAccountSelfControl(presence as ControlMsg)).toEqual(presence);

    const intent = buildCommitIntent({
      account: ACCOUNT,
      from: "ep-a",
      reqId: "req-1",
      kind: "add_device",
      payload: { dmConvId: "d:a:b", kp: "kpB64" },
    });
    expect(parseAccountSelfControl(intent as ControlMsg)).toEqual(intent);

    const result = buildCommitResult({
      account: ACCOUNT,
      reqId: "req-1",
      ok: false,
      reason: "epoch_stale",
      currentEpoch: 3,
    });
    expect(parseAccountSelfControl(result as ControlMsg)).toEqual(result);
  });

  it("rejects malformed self-channel frames", () => {
    expect(parseAccountSelfControl({ t: "presence", convId: "d:x:y", device_id: "a", online: true } as ControlMsg)).toBeNull();
    expect(parseAccountSelfControl({ t: "commit_intent", convId: accountSelfConvId(ACCOUNT) } as ControlMsg)).toBeNull();
  });

  it("accountFromSelfConv parses s: prefix", () => {
    expect(accountFromSelfConv(accountSelfConvId(ACCOUNT))).toBe(ACCOUNT);
    expect(accountFromSelfConv("d:a:b")).toBeNull();
    expect(accountFromSelfConv("s:")).toBeNull();
  });

  it("round-trips device-join request / offer / kp", () => {
    const req = buildDeviceJoinRequest({ account: ACCOUNT, deviceId: "dev-b" });
    expect(parseAccountSelfControl(req as ControlMsg)).toEqual(req);

    const offer = buildDeviceJoinOffer({ account: ACCOUNT, deviceId: "dev-b", convIds: ["d:a:b", "d:a:c"] });
    expect(parseAccountSelfControl(offer as ControlMsg)).toEqual(offer);

    const kp = buildDeviceJoinKp({
      account: ACCOUNT,
      deviceId: "dev-b",
      kps: [{ conv_id: "d:a:b", kp: "kp1" }],
    });
    expect(parseAccountSelfControl(kp as ControlMsg)).toEqual(kp);
  });

  it("drops malformed device-join frames + filters bad kp/conv entries", () => {
    expect(
      parseAccountSelfControl({ t: "device_join_request", convId: accountSelfConvId(ACCOUNT) } as ControlMsg),
    ).toBeNull();
    expect(
      parseAccountSelfControl({
        t: "device_join_offer",
        convId: accountSelfConvId(ACCOUNT),
        device_id: "x",
      } as ControlMsg),
    ).toBeNull();
    const dirty = parseAccountSelfControl({
      t: "device_join_kp",
      convId: accountSelfConvId(ACCOUNT),
      device_id: "x",
      kps: [{ conv_id: "d:a:b", kp: "ok" }, { conv_id: 1, kp: "bad" }, { kp: "nokconv" }],
    } as unknown as ControlMsg);
    expect(dirty).toEqual({
      t: "device_join_kp",
      convId: accountSelfConvId(ACCOUNT),
      device_id: "x",
      // scope is back-filled from the conv prefix (d: → dm)
      kps: [{ conv_id: "d:a:b", kp: "ok", scope: "dm" }],
    });
  });

  it("carries an explicit group scope and derives scope from the g: prefix", () => {
    const kp = buildDeviceJoinKp({
      account: ACCOUNT,
      deviceId: "dev-b",
      kps: [{ conv_id: "g:42", kp: "kpG" }, { conv_id: "d:a:b", kp: "kpD", scope: "dm" }],
    });
    expect(kp.kps).toEqual([
      { conv_id: "g:42", kp: "kpG", scope: "group" },
      { conv_id: "d:a:b", kp: "kpD", scope: "dm" },
    ]);
    expect(parseAccountSelfControl(kp as ControlMsg)).toEqual(kp);
  });
});

describe("Wire Commit + commit_reject", () => {
  it("buildWireDmCommit carries commit_epoch and msgId", () => {
    const frame = buildWireDmCommit({
      from: "ep-a",
      convId: "d:a:b",
      commitB64: "cm0=",
      commitEpoch: 2,
      msgId: "m-1",
    });
    expect(frame.commit_epoch).toBe(2);
    expect(frame.msgId).toBe("m-1");
  });

  it("buildWireNewDeviceState targets s:<account> with opaque state blob", () => {
    const frame = buildWireNewDeviceState({
      from: "ep-a",
      account: ACCOUNT,
      stateB64: "c3RhdGU=",
      msgId: "nds-1",
    });
    expect(frame.t).toBe("new_device_state");
    expect(frame.convId).toBe(accountSelfConvId(ACCOUNT));
    expect(frame.state).toBe("c3RhdGU=");
    expect(frame.msgId).toBe("nds-1");
  });

  it("parseCommitReject reads epoch_stale", () => {
    expect(
      parseCommitReject({
        type: "commit_reject",
        reason: "epoch_stale",
        convId: "d:a:b",
        current_epoch: 4,
        msgId: "m-2",
      }),
    ).toEqual({
      reason: "epoch_stale",
      convId: "d:a:b",
      current_epoch: 4,
      msgId: "m-2",
    });
    expect(parseCommitReject({ type: "commit_reject", reason: "other" })).toBeNull();
  });

  it("shouldRetryCommitReject respects MAX_COMMIT_RETRY", () => {
    expect(shouldRetryCommitReject(0)).toBe(true);
    expect(shouldRetryCommitReject(MAX_COMMIT_RETRY - 1)).toBe(true);
    expect(shouldRetryCommitReject(MAX_COMMIT_RETRY)).toBe(false);
  });
});

describe("routeCommitIntent", () => {
  it("CD executes locally", () => {
    const election = initCoordinatorElection("dev-a");
    expect(
      routeCommitIntent({
        election,
        selfDeviceId: "dev-a",
        account: ACCOUNT,
        endpointId: "ep-a",
        reqId: "r1",
        kind: "rekey",
        payload: { dmConvId: "d:a:b" },
      }).action,
    ).toBe("execute");
  });

  it("non-CD delegates via commit_intent", () => {
    let election = initCoordinatorElection("dev-a");
    election = applyPresenceUpdate(election, "dev-b", true, 0);
    election = tickCoordinatorElection(election, CD_SETTLE_MS);
    const routed = routeCommitIntent({
      election,
      selfDeviceId: "dev-b",
      account: ACCOUNT,
      endpointId: "ep-b",
      reqId: "r2",
      kind: "add_device",
      payload: { dmConvId: "d:a:b", kp: "kp" },
    });
    expect(routed.action).toBe("delegate");
    if (routed.action === "delegate") {
      expect(routed.intent.t).toBe("commit_intent");
      expect(routed.intent.req_id).toBe("r2");
      expect(routed.intent.payload.kp).toBe("kp");
    }
  });
});
