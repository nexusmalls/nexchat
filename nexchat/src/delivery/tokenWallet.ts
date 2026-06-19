// EN: Sender-side delivery token wallet (blind issuance + consume on send).
// CN: 发送方投递令牌钱包（盲签领取 + 发送时消费）。

import { config } from "@/config";
import type { DeliveryAdmission, TokenWalletState } from "@/delivery/types";
import { bytesToB64, b64ToBytes } from "@/delivery/b64";
import {
  blindTokenRequest,
  finalizeToken,
  importPublicKey,
} from "@/delivery/rsabssa";
import { deriveContactTag } from "@/delivery/tokenMessage";
import { lookupInbox } from "@/delivery/inboxManager";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";

const META_WALLET = "__meta__/token-wallet";

type MetaStore = {
  getMeta?: <T>(key: string) => Promise<T | null>;
  setMeta?: <T>(key: string, value: T) => Promise<void>;
};

interface PendingBatch {
  peer: string;
  invs: Uint8Array[];
  prepared: Uint8Array[];
  ts: Uint8Array[];
  ct: string;
  epoch: number;
  inboxId: string;
  ipkN: string;
  ipkE: string;
}

export class TokenWallet {
  private state: TokenWalletState = { byPeer: {} };
  private pending = new Map<string, PendingBatch>();
  private waiters = new Map<string, Array<() => void>>();
  private waitTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private batchInflight = new Map<string, Promise<void>>();

  constructor(private meta: MetaStore) {}

  async load(): Promise<void> {
    const s = await this.meta.getMeta?.<TokenWalletState>(META_WALLET);
    if (s) this.state = s;
  }

  private async save(): Promise<void> {
    await this.meta.setMeta?.(META_WALLET, this.state);
  }

  count(peer: string): number {
    return this.state.byPeer[peer]?.length ?? 0;
  }

  /// EN: Resolve when `absorbTokenSig` adds tokens for `peer` (no polling). CN: 收到 token_sig 后唤醒（无轮询）。
  waitForTokens(peer: string, timeoutMs = 20_000): Promise<void> {
    if (this.count(peer) > 0) return Promise.resolve();
    this.cancelWait(peer);
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => {
        this.clearWaitState(peer);
        reject(new Error("投递令牌申领超时"));
      }, timeoutMs);
      this.waitTimers.set(peer, t);
      const wake = () => {
        this.clearWaitState(peer);
        resolve();
      };
      const list = this.waiters.get(peer) ?? [];
      list.push(wake);
      this.waiters.set(peer, list);
    });
  }

  private clearWaitState(peer: string): void {
    const t = this.waitTimers.get(peer);
    if (t) clearTimeout(t);
    this.waitTimers.delete(peer);
    this.waiters.delete(peer);
  }

  private notifyPeer(peer: string): void {
    const t = this.waitTimers.get(peer);
    if (t) clearTimeout(t);
    this.waitTimers.delete(peer);
    const list = this.waiters.get(peer) ?? [];
    this.waiters.delete(peer);
    for (const fn of list) fn();
  }

  cancelWait(peer: string): void {
    this.clearWaitState(peer);
  }

  async requestBatch(
    peer: string,
    selfAddress: string,
    relay: RelayClient,
    endpointId: string,
    mlsKey: string,
  ): Promise<void> {
    const inflight = this.batchInflight.get(peer);
    if (inflight) return inflight;
    const job = this.requestBatchOnce(peer, selfAddress, relay, endpointId, mlsKey);
    this.batchInflight.set(peer, job);
    try {
      await job;
    } finally {
      this.batchInflight.delete(peer);
    }
  }

  private async requestBatchOnce(
    peer: string,
    selfAddress: string,
    relay: RelayClient,
    endpointId: string,
    mlsKey: string,
  ): Promise<void> {
    const info = await lookupInbox(peer, { fresh: true });
    if (!info) throw new Error("对端信箱未在 relay 注册（需对方在线）");
    if (info.online === false) {
      throw new Error("对端未在 relay 在线（需对方打开 NexChat）");
    }
    const pub = await importPublicKey({ kty: "RSA", n: info.ipkN, e: info.ipkE, alg: "PS384" });
    const ct = await deriveContactTag(peer, selfAddress);
    const ctB64 = bytesToB64(ct);
    const invs: Uint8Array[] = [];
    const prepared: Uint8Array[] = [];
    const ts: Uint8Array[] = [];
    const batch = Math.max(1, config.deliveryTokenBatch);
    const blinds: string[] = [];
    const preparedRows = await Promise.all(
      Array.from({ length: batch }, async () => {
        const t = crypto.getRandomValues(new Uint8Array(32));
        const { blindedMsg, inv, preparedMsg } = await blindTokenRequest(pub, t, ct, info.epoch);
        return { t, inv, preparedMsg, blindedMsg };
      }),
    );
    for (const row of preparedRows) {
      ts.push(row.t);
      invs.push(row.inv);
      prepared.push(row.preparedMsg);
      blinds.push(bytesToB64(row.blindedMsg));
    }
    this.pending.set(peer, {
      peer,
      invs,
      prepared,
      ts,
      ct: ctB64,
      epoch: info.epoch,
      inboxId: info.inboxId,
      ipkN: info.ipkN,
      ipkE: info.ipkE,
    });
    await relay.sendControl({
      t: "token_req",
      from: endpointId,
      fromAddr: selfAddress,
      toAddr: peer,
      convId: mlsKey,
      blinds,
    });
  }

  async absorbTokenSig(msg: Extract<ControlMsg, { t: "token_sig" }>): Promise<void> {
    const peer = msg.issuer;
    const batch = this.pending.get(peer);
    if (!batch || batch.inboxId !== msg.inboxId) return;
    if (batch.epoch !== msg.epoch || batch.ct !== msg.ct) {
      console.warn("[nexchat] token_sig ignored (stale epoch/ct)");
      return;
    }
    if (msg.sigs.length !== batch.prepared.length) {
      console.warn("[nexchat] token_sig ignored (batch size mismatch)");
      return;
    }
    try {
      const pub = await importPublicKey({ kty: "RSA", n: msg.ipkN, e: msg.ipkE, alg: "PS384" });
      const tokens = this.state.byPeer[peer] ?? [];
      for (let i = 0; i < msg.sigs.length; i++) {
        const sig = await finalizeToken(
          pub,
          batch.prepared[i]!,
          b64ToBytes(msg.sigs[i]!),
          batch.invs[i]!,
        );
        tokens.push({
          inboxId: msg.inboxId,
          ipkN: msg.ipkN,
          ipkE: msg.ipkE,
          epoch: msg.epoch,
          ct: msg.ct,
          t: bytesToB64(batch.ts[i]!),
          s: bytesToB64(sig),
          p: bytesToB64(batch.prepared[i]!),
          peer,
        });
      }
      this.state.byPeer[peer] = tokens;
      this.pending.delete(peer);
      await this.save();
      this.notifyPeer(peer);
    } catch (e) {
      console.warn("[nexchat] token_sig finalize failed (stale batch?):", e);
      this.pending.delete(peer);
      this.cancelWait(peer);
    }
  }

  consume(peer: string, expectedEpoch?: number): DeliveryAdmission | null {
    const list = this.state.byPeer[peer];
    if (!list || list.length === 0) return null;
    while (list.length > 0) {
      const tok = list[0]!;
      if (expectedEpoch != null && tok.epoch !== expectedEpoch) {
        list.shift();
        continue;
      }
      list.shift();
      void this.save();
      return {
        inboxId: tok.inboxId,
        ipkN: tok.ipkN,
        ipkE: tok.ipkE,
        epoch: tok.epoch,
        ct: tok.ct,
        t: tok.t,
        s: tok.s,
        p: tok.p,
      };
    }
    if (expectedEpoch != null && (this.state.byPeer[peer]?.length ?? 0) === 0) {
      delete this.state.byPeer[peer];
      void this.save();
    }
    return null;
  }
}
