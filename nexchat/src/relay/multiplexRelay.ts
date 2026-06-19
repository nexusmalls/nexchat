// EN: Fan-out / fan-in relay across multiple transports (e.g. BC + WebSocket).
// CN: 多传输层 relay 扇出/扇入（如 BC + WebSocket）。

import type {
  ControlInbound,
  ControlMsg,
  RelayClient,
  RelayFrame,
  RelayInbound,
  RelayRejectInbound,
} from "@/relay/relayClient";
import { InboundDedup, frameDedupKey } from "@/relay/dedup";
import { networkRelayRequired } from "@/relay/relayNetwork";
import { WebSocketRelay } from "@/relay/wsRelay";

export class MultiplexRelay implements RelayClient {
  private cb: RelayInbound | null = null;
  private ctrlCbs: ControlInbound[] = [];
  private readonly dedup = new InboundDedup();

  constructor(private readonly inner: RelayClient[]) {}

  async connect(selfRef: string, account?: string): Promise<void> {
    const results = await Promise.allSettled(
      this.inner.map((c) => c.connect(selfRef, account)),
    );
    const ws = this.inner.find((c): c is WebSocketRelay => c instanceof WebSocketRelay);
    if (ws && networkRelayRequired()) {
      if (!ws.isConnected()) {
        const reason = results.find((r) => r.status === "rejected") as
          | PromiseRejectedResult
          | undefined;
        throw reason?.reason ?? new Error("WebSocket relay connect failed");
      }
      return;
    }
    const ok = results.some((r) => r.status === "fulfilled");
    if (!ok) {
      const reason = results.find((r) => r.status === "rejected") as PromiseRejectedResult | undefined;
      throw reason?.reason ?? new Error("relay connect failed");
    }
  }

  async send(frame: RelayFrame): Promise<void> {
    const dedupKey = frame.dedupKey ?? globalThis.crypto?.randomUUID?.() ?? `dk-${Date.now()}`;
    const out = { ...frame, dedupKey };
    const results = await Promise.allSettled(this.inner.map((c) => c.send(out)));
    if (results.some((r) => r.status === "fulfilled")) return;
    const reason = results.find((r) => r.status === "rejected") as
      | PromiseRejectedResult
      | undefined;
    throw reason?.reason ?? new Error("Relay 发送失败");
  }

  async sendControl(msg: ControlMsg): Promise<void> {
    const results = await Promise.allSettled(this.inner.map((c) => c.sendControl(msg)));
    if (results.some((r) => r.status === "fulfilled")) return;
    const reason = results.find((r) => r.status === "rejected") as
      | PromiseRejectedResult
      | undefined;
    throw reason?.reason ?? new Error("Relay 控制面发送失败");
  }

  requestMlsBacklog(account: string, convId: string): void {
    for (const c of this.inner) c.requestMlsBacklog?.(account, convId);
  }

  onConnect(cb: () => void): () => void {
    const unsubs = this.inner.map((c) => c.onConnect?.(cb) ?? (() => {}));
    return () => unsubs.forEach((u) => u());
  }

  onDisconnect(cb: () => void): () => void {
    const unsubs = this.inner.map((c) => c.onDisconnect?.(cb) ?? (() => {}));
    return () => unsubs.forEach((u) => u());
  }

  onReject(cb: RelayRejectInbound): void {
    for (const c of this.inner) c.onReject?.(cb);
  }

  onMessage(cb: RelayInbound): void {
    this.cb = cb;
    for (const c of this.inner) {
      c.onMessage((frame) => {
        if (!this.dedup.accept(frameDedupKey(frame))) return;
        this.cb?.(frame);
      });
    }
  }

  onControl(cb: ControlInbound): void {
    this.ctrlCbs.push(cb);
    for (const c of this.inner) {
      c.onControl((msg) => {
        const key = `ctrl:${JSON.stringify(msg)}`;
        if (!this.dedup.accept(key)) return;
        for (const fn of this.ctrlCbs) fn(msg);
      });
    }
  }

  disconnect(): void {
    for (const c of this.inner) c.disconnect();
    this.cb = null;
    this.ctrlCbs = [];
  }
}
