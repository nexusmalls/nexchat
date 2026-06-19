// EN: WebSocket relay client — persistent store-and-forward transport for production relay-rs.
// CN: WebSocket relay 客户端——面向生产 relay-rs 的持久化 store-and-forward 传输层。

import type {
  CommitRejectInbound,
  ControlInbound,
  ControlMsg,
  RelayClient,
  RelayFrame,
  RelayInbound,
  RelayRejectInbound,
} from "@/relay/relayClient";
import { RELAY_MAX_FRAME_BYTES } from "@/relay/relayClient";
import { registerAccountWire } from "@/relay/registerAccountAuth";
import { parseCommitReject } from "@/mls/directCommitCoordination";
import { InboundDedup, frameDedupKey } from "@/relay/dedup";

export class WebSocketRelay implements RelayClient {
  private ws: WebSocket | null = null;
  private cb: RelayInbound | null = null;
  private ctrlCbs: ControlInbound[] = [];
  private selfRef = "";
  private account = "";
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;
  private stopped = true;
  private readonly dedup = new InboundDedup();
  private connectCbs: Array<() => void> = [];
  private disconnectCbs: Array<() => void> = [];
  private rejectCbs: RelayRejectInbound[] = [];
  private commitRejectCbs: CommitRejectInbound[] = [];

  constructor(private url: string) {}

  async connect(selfRef: string, account?: string): Promise<void> {
    this.teardown(false);
    this.stopped = false;
    this.selfRef = selfRef;
    this.account = account ?? "";
    await this.openSocket();
  }

  private register(ws: WebSocket): void {
    ws.send(JSON.stringify({ type: "register", id: this.selfRef }));
    if (this.account) {
      ws.send(JSON.stringify(registerAccountWire(this.selfRef, this.account)));
    }
  }

  private scheduleReconnect(): void {
    if (this.stopped || !this.selfRef) return;
    if (this.reconnectTimer) return;
    const delay = Math.min(30_000, 1000 * 2 ** this.reconnectAttempt);
    this.reconnectAttempt++;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.openSocket().catch(() => this.scheduleReconnect());
    }, delay);
  }

  private openSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.url);
      ws.onopen = () => {
        this.register(ws);
        this.ws = ws;
        this.reconnectAttempt = 0;
        for (const cb of this.connectCbs) cb();
        resolve();
      };
      ws.onerror = () => reject(new Error(`WebSocket relay connect failed: ${this.url}`));
      ws.onmessage = (ev) => this.onWire(ev.data);
      ws.onclose = () => {
        if (this.ws === ws) this.ws = null;
        if (!this.stopped) {
          for (const cb of this.disconnectCbs) cb();
          this.scheduleReconnect();
        }
      };
    });
  }

  private onWire(data: unknown): void {
    try {
      const msg = JSON.parse(String(data)) as Record<string, unknown> & { _from?: string };
      if (msg._from === this.selfRef) return;
      if (msg.type === "frame_reject") {
        const reject = {
          reason: typeof msg.reason === "string" ? msg.reason : "unknown",
          dedupKey: typeof msg.dedupKey === "string" ? msg.dedupKey : undefined,
          convId: typeof msg.convId === "string" ? msg.convId : undefined,
        };
        for (const fn of this.rejectCbs) fn(reject);
        return;
      }
      const commitReject = parseCommitReject(msg);
      if (commitReject) {
        for (const fn of this.commitRejectCbs) fn(commitReject);
        return;
      }
      if (msg._ctrl === true) {
        const { _ctrl: _a, _from: _b, ...rest } = msg;
        const key = `ctrl:${JSON.stringify(rest)}`;
        if (!this.dedup.accept(key)) return;
        const ctrl = rest as unknown as ControlMsg;
        for (const fn of this.ctrlCbs) fn(ctrl);
        return;
      }
      const expiresAt = msg.expiresAt as number | undefined;
      if (expiresAt != null && Date.now() > expiresAt) return;
      const frame: RelayFrame = {
        convId: msg.convId as string,
        senderRef: msg.senderRef as string,
        ciphertextB64: msg.ciphertextB64 as string,
        expiresAt,
        dedupKey: msg.dedupKey as string | undefined,
        delivery: msg.delivery as RelayFrame["delivery"],
      };
      if (!this.dedup.accept(frameDedupKey(frame))) return;
      this.cb?.(frame);
    } catch {
      /* ignore malformed wire messages */
    }
  }

  async send(frame: RelayFrame): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("Relay WebSocket 未连接，消息未能送达");
    }
    const dedupKey = frame.dedupKey ?? globalThis.crypto?.randomUUID?.() ?? `dk-${Date.now()}`;
    const wire = JSON.stringify({ ...frame, dedupKey, _from: this.selfRef });
    // EN: fail fast on oversize instead of letting the relay silently drop it (large media
    // must go via IPFS by reference, not inline). CN: 超大帧就地失败，避免被 relay 静默丢弃
    // （大文件应经 IPFS 以 CID 引用，而非内联）。
    if (byteLength(wire) > RELAY_MAX_FRAME_BYTES) {
      throw new Error("消息过大，未发送（请改用文件/图片附件经 IPFS 发送）");
    }
    this.ws.send(wire);
  }

  async sendControl(msg: ControlMsg): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("Relay WebSocket 未连接");
    }
    this.ws.send(JSON.stringify({ ...msg, _ctrl: true, _from: this.selfRef }));
  }

  requestMlsBacklog(account: string, convId: string): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    this.ws.send(JSON.stringify({ type: "mls_backlog_req", account, convId }));
  }

  onMessage(cb: RelayInbound): void {
    this.cb = cb;
  }

  onControl(cb: ControlInbound): void {
    this.ctrlCbs.push(cb);
  }

  onConnect(cb: () => void): () => void {
    this.connectCbs.push(cb);
    return () => {
      this.connectCbs = this.connectCbs.filter((fn) => fn !== cb);
    };
  }

  onDisconnect(cb: () => void): () => void {
    this.disconnectCbs.push(cb);
    return () => {
      this.disconnectCbs = this.disconnectCbs.filter((fn) => fn !== cb);
    };
  }

  /// EN: True when the WebSocket transport is open. CN: WebSocket 传输层已连接时为 true。
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  onReject(cb: RelayRejectInbound): void {
    this.rejectCbs.push(cb);
  }

  onCommitReject(cb: CommitRejectInbound): void {
    this.commitRejectCbs.push(cb);
  }

  disconnect(): void {
    this.teardown(true);
  }

  /** EN: Close socket; full=true also drops listeners and stops reconnect. CN: 关闭 socket；full 为 true 时清监听并停止重连。 */
  private teardown(full: boolean): void {
    if (full) this.stopped = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    if (full) {
      this.cb = null;
      this.ctrlCbs = [];
      this.selfRef = "";
      this.account = "";
      this.reconnectAttempt = 0;
      this.connectCbs = [];
      this.disconnectCbs = [];
      this.rejectCbs = [];
    }
  }
}

/// EN: UTF-8 byte length of a string (matches the relay's byte-based size cap). CN: 字符串
/// 的 UTF-8 字节长度（与 relay 基于字节的大小上限一致）。
function byteLength(s: string): number {
  if (typeof TextEncoder !== "undefined") return new TextEncoder().encode(s).length;
  return unescape(encodeURIComponent(s)).length;
}
