// EN: Unit tests for the 1:1 Wire multi-leaf "graft-only ownership" behavior of DirectMlsRegistry:
// a graft-managed conv must NOT trigger a pairwise handshake (no fork of the multi-leaf group), the
// registry must ignore its inbound control, and readiness reflects the locally grafted group. The
// default (non-graft) path is asserted to be unchanged.
// CN: DirectMlsRegistry 在 1:1 Wire 多 leaf「graft-only 所有权」下的单测：嫁接拥有的会话**不得**触发
// 1:1 握手（不分叉多 leaf 群），registry 须忽略其入站控制，就绪与否反映本地嫁接群。并断言默认（非嫁接）
// 路径不变。

import { describe, expect, it, vi } from "vitest";
import { directMlsKey } from "@/mls/directConv";
import { DirectMlsRegistry } from "@/mls/directMlsRegistry";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { ControlInbound, ControlMsg, RelayClient } from "@/relay/relayClient";

// member side: self address sorts AFTER peer so the peer is the handshake owner → our start() sends a
// KeyPackage, which is the observable "initiation" signal.
const SELF = "5zzzzSelf";
const PEER = "5aaaaPeer";

class MockRelay implements RelayClient {
  sent: ControlMsg[] = [];
  ctrl: ControlInbound | null = null;
  async connect(): Promise<void> {}
  disconnect(): void {}
  async send(): Promise<void> {}
  onMessage(): void {}
  async sendControl(m: ControlMsg): Promise<void> {
    this.sent.push(m);
  }
  onControl(cb: ControlInbound): void {
    this.ctrl = cb;
  }
  onCommitReject(): void {}
}

function fakeEngine(groups: Set<string>): OpenMlsEngine {
  return {
    hasGroup: (k: string) => groups.has(k),
    generateKeyPackage: vi.fn(() => new Uint8Array([1, 2, 3])),
    forgetGroupByConv: vi.fn(),
    epochByConv: () => 0,
  } as unknown as OpenMlsEngine;
}

function makeRegistry(groups = new Set<string>()) {
  const relay = new MockRelay();
  const engine = fakeEngine(groups);
  const reg = new DirectMlsRegistry({
    engine,
    relay,
    endpointId: "ep-self",
    selfAddress: SELF,
    onPeerStatus: () => {},
  });
  reg.wire();
  return { reg, relay, engine };
}

describe("DirectMlsRegistry graft-only ownership (1:1 Wire)", () => {
  it("default path: ensure() starts the member handshake (sends a KeyPackage)", () => {
    const { reg, relay, engine } = makeRegistry();
    reg.ensure(PEER);
    expect(engine.generateKeyPackage).toHaveBeenCalled();
    expect(relay.sent.some((m) => m.t === "kp")).toBe(true);
  });

  it("graft-managed conv: ensure() does NOT initiate a handshake (no fork)", () => {
    const { reg, relay, engine } = makeRegistry();
    reg.markGraftManaged(directMlsKey(SELF, PEER));
    expect(reg.isGraftManaged(directMlsKey(SELF, PEER))).toBe(true);
    reg.ensure(PEER);
    expect(engine.generateKeyPackage).not.toHaveBeenCalled();
    expect(relay.sent.some((m) => m.t === "kp")).toBe(false);
  });

  it("graft-managed conv: registry ignores inbound control (no lazy coordinator/handshake)", () => {
    const { reg, relay, engine } = makeRegistry();
    const key = directMlsKey(SELF, PEER);
    reg.markGraftManaged(key);
    // a Commit frame for the graft-owned conv must be ignored entirely
    relay.ctrl?.({ t: "commit", from: "ep-peer", convId: key, commit: "Y20=" } as ControlMsg);
    expect(engine.generateKeyPackage).not.toHaveBeenCalled();
    expect(relay.sent.length).toBe(0);
  });

  it("readiness of a graft-managed conv reflects the locally grafted group", () => {
    const key = directMlsKey(SELF, PEER);
    const { reg } = makeRegistry(new Set([key]));
    reg.markGraftManaged(key);
    expect(reg.isReady(PEER)).toBe(true);

    const { reg: reg2 } = makeRegistry(new Set()); // group not yet grafted
    reg2.markGraftManaged(key);
    expect(reg2.isReady(PEER)).toBe(false);
  });

  it("unmarkGraftManaged + ensure() starts a deferred handshake", () => {
    const { reg, relay, engine } = makeRegistry();
    const key = directMlsKey(SELF, PEER);
    reg.markGraftManaged(key);
    reg.ensure(PEER);
    expect(engine.generateKeyPackage).not.toHaveBeenCalled();
    reg.unmarkGraftManaged(key);
    reg.ensure(PEER);
    expect(engine.generateKeyPackage).toHaveBeenCalled();
    expect(relay.sent.some((m) => m.t === "kp")).toBe(true);
  });

  it("recoverPeer is a no-op for a graft-managed conv (never re-handshakes)", () => {
    const { reg, relay, engine } = makeRegistry();
    reg.markGraftManaged(directMlsKey(SELF, PEER));
    reg.recoverPeer(PEER);
    expect(engine.generateKeyPackage).not.toHaveBeenCalled();
    expect(relay.sent.length).toBe(0);
  });
});
