// EN: Probe a live NexChat relay for Wire-multi-leaf prerequisites (mls_backlog_req, peer_add_req
// routing, commit_epoch CAS, device_join on s:<account>). Used by client startup warnings and by
// CI/live e2e scripts before Wire QA.
// CN: 探测在线 NexChat relay 是否具备 Wire 多 leaf 前置能力（mls_backlog_req、peer_add_req 路由、
// commit_epoch CAS、s:<account> 上的 device_join）。供客户端启动告警与 CI/live e2e 脚本在 Wire QA 前使用。

import { normalizeRelayAccount, registerAccountWire } from "@/relay/registerAccountAuth";

export const RELAY_WIRE_PROBE_PEER = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM794ty";

/// EN: Wire relay features required for production 1:1 multi-device. CN: 生产 1:1 多设备所需的 relay 特性。
export const RELAY_WIRE_FEATURES = [
  "mls_backlog_req",
  "peer_add_req",
  "commit_epoch_cas",
  "device_join",
] as const;

export type RelayWireFeature = (typeof RELAY_WIRE_FEATURES)[number];

export interface RelayWireCapabilityReport {
  ok: boolean;
  missing: RelayWireFeature[];
  details: Record<RelayWireFeature, boolean>;
}

type RelayJson = Record<string, unknown>;

export interface RelayWireProbeTransport {
  connect(): Promise<void>;
  register(endpointId: string, account: string): void;
  send(payload: RelayJson): void;
  sendControl(ctrl: RelayJson): void;
  waitFor(match: (msg: RelayJson) => boolean, timeoutMs?: number): Promise<RelayJson>;
  close(): void;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/// EN: Run the full Wire capability matrix against one relay URL (two logical accounts).
/// CN: 对单个 relay URL 运行完整 Wire 能力矩阵（两个逻辑账户）。
export async function probeRelayWireCapabilities(
  transportFactory: (label: string, account: string) => RelayWireProbeTransport,
  accountA: string,
  accountB: string,
  pairwiseConv: string,
): Promise<RelayWireCapabilityReport> {
  const details = Object.fromEntries(RELAY_WIRE_FEATURES.map((f) => [f, false])) as Record<
    RelayWireFeature,
    boolean
  >;

  const alice = transportFactory("alice", accountA);
  const bob = transportFactory("bob", accountB);
  await alice.connect();
  await bob.connect();
  alice.register("probe-a", accountA);
  bob.register("probe-b", accountB);
  await sleep(200);

  // 1) mls_backlog_req — authenticated owner must NOT get auth_reject.
  try {
    alice.send({ type: "mls_backlog_req", account: normalizeRelayAccount(accountA), convId: pairwiseConv });
    await alice.waitFor((m) => m.type === "auth_reject" && m.op === "mls_backlog_req", 800);
    details.mls_backlog_req = false;
  } catch {
    details.mls_backlog_req = true;
  }

  // 2) peer_add_req — routes to the other conv party with relay-stamped sender.
  try {
    const pending = bob.waitFor(
      (m) => m._ctrl === true && m.t === "peer_add_req" && m.convId === pairwiseConv,
      2_000,
    );
    await alice.sendControl({
      t: "peer_add_req",
      from: "probe-a",
      convId: pairwiseConv,
      requester_account: accountA,
      device_id: "probe-dev",
      kp: "dGVzdA==",
    });
    const msg = await pending;
    details.peer_add_req = Boolean(msg._senderAccount) && msg.requester_account === accountA;
  } catch {
    details.peer_add_req = false;
  }

  // 3) commit_epoch CAS — second concurrent epoch-0 Commit gets commit_reject epoch_stale.
  try {
    await alice.sendControl({
      t: "commit",
      from: "probe-a",
      convId: pairwiseConv,
      commit: "Y29tbWl0LUE=",
      commit_epoch: 0,
      msgId: "probe-mA",
    });
    await sleep(60);
    const pendingReject = alice.waitFor(
      (m) => m.type === "commit_reject" && m.reason === "epoch_stale" && m.convId === pairwiseConv,
      2_000,
    );
    await alice.sendControl({
      t: "commit",
      from: "probe-a",
      convId: pairwiseConv,
      commit: "Y29tbWl0LUI=",
      commit_epoch: 0,
      msgId: "probe-mB",
    });
    const reject = await pendingReject;
    details.commit_epoch_cas =
      reject.current_epoch === 1 || reject.current_epoch === "1" || Number(reject.current_epoch) >= 1;
  } catch {
    details.commit_epoch_cas = false;
  }

  // 4) device_join on s:<account> — routes to account owner devices.
  try {
    const selfNorm = normalizeRelayAccount(accountA);
    const pendingJoin = alice.waitFor((m) => {
      if (!(m._ctrl === true && m.t === "device_join_request")) return false;
      const conv = String(m.convId ?? "");
      if (!conv.startsWith("s:")) return false;
      return normalizeRelayAccount(conv.slice(2)) === selfNorm;
    }, 4_000);
    await bob.sendControl({
      t: "device_join_request",
      from: "probe-b",
      convId: `s:${accountA}`,
      device_id: "foreign-dev",
    });
    await pendingJoin;
    details.device_join = true;
  } catch {
    details.device_join = false;
  }

  alice.close();
  bob.close();

  const missing = RELAY_WIRE_FEATURES.filter((f) => !details[f]);
  return { ok: missing.length === 0, missing, details };
}

/// EN: Browser/Node WebSocket transport for `probeRelayWireCapabilities`. CN: 供探测用的 WebSocket 传输。
export function webSocketProbeTransport(
  url: string,
  label: string,
): (sideLabel: string, account: string) => RelayWireProbeTransport {
  return (side) => {
    let ws: WebSocket | null = null;
    const inbox: RelayJson[] = [];
    const waiters: Array<{
      match: (msg: RelayJson) => boolean;
      resolve: (msg: RelayJson) => void;
      reject: (err: Error) => void;
    }> = [];
    const endpointId = side === "alice" ? `${label}-probe-a` : `${label}-probe-b`;

    const deliver = (msg: RelayJson) => {
      if (msg._from === endpointId) return;
      inbox.push(msg);
      for (let i = 0; i < waiters.length; i++) {
        const w = waiters[i]!;
        if (w.match(msg)) {
          waiters.splice(i, 1);
          w.resolve(msg);
          return;
        }
      }
    };

    return {
      async connect() {
        await new Promise<void>((resolve, reject) => {
          ws = new WebSocket(url);
          const timer = setTimeout(() => {
            ws?.close();
            reject(new Error(`relay probe connect timeout (${url})`));
          }, 8_000);
          ws.onopen = () => {
            clearTimeout(timer);
            resolve();
          };
          ws.onerror = () => {
            clearTimeout(timer);
            reject(new Error(`relay probe connect failed (${url})`));
          };
          ws.onmessage = (ev) => {
            try {
              deliver(JSON.parse(String(ev.data)) as RelayJson);
            } catch {
              /* ignore */
            }
          };
          ws.onclose = () => {
            for (const w of waiters.splice(0)) w.reject(new Error("relay probe socket closed"));
          };
        });
      },
      register(id, acct) {
        ws?.send(JSON.stringify({ type: "register", id }));
        ws?.send(JSON.stringify(registerAccountWire(id, acct)));
      },
      send(payload) {
        ws?.send(JSON.stringify({ ...payload, _from: endpointId }));
      },
      sendControl(ctrl) {
        ws?.send(JSON.stringify({ ...ctrl, _ctrl: true, _from: endpointId }));
      },
      waitFor(match, timeoutMs = 5_000) {
        const hit = inbox.find(match);
        if (hit) return Promise.resolve(hit);
        return new Promise<RelayJson>((resolve, reject) => {
          const timer = setTimeout(() => {
            const idx = waiters.findIndex((w) => w.resolve === resolve);
            if (idx >= 0) waiters.splice(idx, 1);
            reject(new Error(`relay probe wait timeout (${label})`));
          }, timeoutMs);
          waiters.push({
            match,
            resolve: (msg) => {
              clearTimeout(timer);
              resolve(msg);
            },
            reject: (err) => {
              clearTimeout(timer);
              reject(err);
            },
          });
        });
      },
      close() {
        ws?.close();
        ws = null;
      },
    };
  };
}
