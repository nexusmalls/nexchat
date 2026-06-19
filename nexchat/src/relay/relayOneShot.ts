// EN: One-shot relay WebSocket — register + signed register_account before authenticated KV writes.
// All mailbox consume paths (`chat_consume`, `contact_consume`, `group_invite_consume`) use
// `relayOneShotSend`; fetches use `relayOneShotFetch`. `inbox_lookup` stays unauthenticated (no
// register_account) — see `inboxManager.ts`.
// CN: 一次性 relay WebSocket——authenticated KV 写入前先 register + 带签名的 register_account。
// 邮箱 consume（`chat_consume`、`contact_consume`、`group_invite_consume`）均走 `relayOneShotSend`；
// fetch 走 `relayOneShotFetch`。`inbox_lookup` 保持未认证（不 register_account）——见 `inboxManager.ts`。

import { config } from "@/config";
import { relayErrorFromWire } from "@/relay/relayErrors";
import { registerAccountWire } from "@/relay/registerAccountAuth";
import { withOneShotSlot } from "@/relay/oneShotLimit";

function openRegistered(ws: WebSocket, id: string, account: string, afterRegister: () => void): void {
  ws.send(JSON.stringify({ type: "register", id }));
  ws.send(JSON.stringify(registerAccountWire(id, account)));
  afterRegister();
}

/// EN: Send one message on a fresh WS after account registration (for pointer/inbox puts).
/// CN: 在新 WS 上注册账户后发送单条消息（指针/inbox 写入）。
export function relayOneShotSend(
  account: string,
  msg: Record<string, unknown>,
  opts?: { ackType?: string; timeoutMs?: number; noReply?: boolean },
): Promise<void> {
  if (!config.relayWs) return Promise.resolve();
  const timeoutMs = opts?.timeoutMs ?? 4000;
  const ackType = opts?.ackType;
  const noReply = opts?.noReply === true;
  return withOneShotSlot(() => new Promise<void>((resolve, reject) => {
    const ws = new WebSocket(config.relayWs);
    const id = `kv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(t);
      fn();
    };
    const t = setTimeout(() => {
      finish(() => {
        ws.close();
        reject(new Error("relay ws timeout"));
      });
    }, timeoutMs);
    ws.onopen = () => openRegistered(ws, id, account, () => {
      ws.send(JSON.stringify(msg));
      if (noReply) {
        setTimeout(() => finish(() => {
          ws.close();
          resolve();
        }), 100);
      }
    });
    ws.onerror = () => {
      finish(() => {
        ws.close();
        reject(new Error("relay ws error"));
      });
    };
    ws.onmessage = (ev) => {
      try {
        const m = JSON.parse(String(ev.data)) as Record<string, unknown>;
        const relayErr = relayErrorFromWire(m, ackType);
        if (relayErr) {
          finish(() => {
            ws.close();
            reject(relayErr);
          });
          return;
        }
        if (noReply) return;
        if (!ackType || m.type === ackType) {
          finish(() => {
            ws.close();
            resolve();
          });
        }
      } catch {
        /* ignore */
      }
    };
    ws.onclose = () => {
      finish(() => resolve());
    };
  }));
}

/// EN: Fetch with request/reply matching; registers the account on the session first.
/// CN: 带 request/reply 匹配的 fetch；先在会话上 register_account。
export function relayOneShotFetch<T>(
  account: string,
  send: Record<string, unknown>,
  match: (msg: Record<string, unknown>, requestId: string) => T | undefined,
  timeoutMs = 4000,
): Promise<T | null> {
  if (!config.relayWs) return Promise.resolve(null);
  return withOneShotSlot(() => new Promise<T | null>((resolve) => {
    const ws = new WebSocket(config.relayWs);
    const id = `fetch-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const request_id = globalThis.crypto?.randomUUID?.() ?? `r-${Date.now()}`;
    const t = setTimeout(() => {
      ws.close();
      resolve(null);
    }, timeoutMs);
    ws.onopen = () => {
      openRegistered(ws, id, account, () => {
        ws.send(JSON.stringify({ ...send, account, request_id }));
      });
    };
    ws.onmessage = (ev) => {
      try {
        const m = JSON.parse(String(ev.data)) as Record<string, unknown>;
        if (relayErrorFromWire(m)) {
          clearTimeout(t);
          ws.close();
          resolve(null);
          return;
        }
        const hit = match(m, request_id);
        if (hit !== undefined) {
          clearTimeout(t);
          ws.close();
          resolve(hit);
        }
      } catch {
        /* ignore */
      }
    };
    ws.onerror = () => {
      clearTimeout(t);
      ws.close();
      resolve(null);
    };
  }));
}
