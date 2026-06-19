import { describe, expect, it, vi } from "vitest";
import { chainClient } from "@/chain/chainClient";
import { ChainDeviceDirectory } from "@/crypto-dr/multiDevice";
import type { ControlInbound, ControlMsg, RelayClient } from "@/relay/relayClient";

/// Relay whose onControl FANS OUT to every subscriber (matches the real BC/MUX/WS semantics).
class FanoutRelay implements RelayClient {
  private ctrl: ControlInbound[] = [];
  async connect(): Promise<void> {}
  async send(): Promise<void> {}
  onMessage(): void {}
  async sendControl(msg: ControlMsg): Promise<void> {
    for (const c of this.ctrl) c(msg);
  }
  onControl(cb: ControlInbound): void {
    this.ctrl.push(cb);
  }
  disconnect(): void {}
}

const dev = (b: number): Uint8Array => new Uint8Array(16).fill(b);

describe("ChainDeviceDirectory — cache + control-plane refresh subscription", () => {
  it("caches within TTL, then re-reads after an opk_publish invalidates the account", async () => {
    const acc = "device-dir-test-account";
    const spy = vi
      .spyOn(chainClient, "msgIdentityDevices")
      .mockResolvedValueOnce([{ deviceId: dev(1) }] as never)
      .mockResolvedValueOnce([{ deviceId: dev(1) }, { deviceId: dev(2) }] as never);

    const dir = new ChainDeviceDirectory({ ttlMs: 60_000 });

    const first = await dir.listDevices(acc);
    expect(first).toHaveLength(1);

    // Second read within TTL is served from cache — no extra chain hit.
    const cached = await dir.listDevices(acc);
    expect(cached).toHaveLength(1);
    expect(spy).toHaveBeenCalledTimes(1);

    // A peer device advertising prekeys on the control plane invalidates this account.
    const relay = new FanoutRelay();
    dir.subscribeRefresh(relay);
    await relay.sendControl({
      t: "opk_publish",
      convId: `s:${acc}`,
      from: acc,
      device_id: "aa",
      root: "bb",
      leaves: [],
    });

    // Next read re-hits the chain and now includes the newly-joined device.
    const refreshed = await dir.listDevices(acc);
    expect(refreshed).toHaveLength(2);
    expect(spy).toHaveBeenCalledTimes(2);

    spy.mockRestore();
  });

  it("subscribeRefresh is idempotent (a second call does not double-subscribe)", async () => {
    const acc = "device-dir-idem-account";
    const spy = vi
      .spyOn(chainClient, "msgIdentityDevices")
      .mockResolvedValue([{ deviceId: dev(9) }] as never);
    const dir = new ChainDeviceDirectory({ ttlMs: 60_000 });
    const relay = new FanoutRelay();
    let invalidations = 0;
    const origInvalidate = dir.invalidate.bind(dir);
    dir.invalidate = (a: string) => {
      invalidations += 1;
      origInvalidate(a);
    };
    dir.subscribeRefresh(relay);
    dir.subscribeRefresh(relay); // idempotent — must not register a second handler

    await relay.sendControl({
      t: "opk_publish",
      convId: `s:${acc}`,
      from: acc,
      device_id: "aa",
      root: "bb",
      leaves: [],
    });
    expect(invalidations).toBe(1);
    spy.mockRestore();
  });
});
