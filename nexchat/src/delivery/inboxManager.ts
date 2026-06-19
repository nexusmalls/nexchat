// EN: Receiver-side inbox (IPK + epoch + per-contact tags) for blind token issuance.
// CN: 接收方信箱（IPK + epoch + 每联系人标签）用于盲签令牌签发。

import { config } from "@/config";
import { RelayInboxStaleEpochError } from "@/relay/relayErrors";
import { relayOneShotSend } from "@/relay/relayOneShot";
import { withOneShotSlot } from "@/relay/oneShotLimit";
import type { InboxRecord } from "@/delivery/types";
import { bytesToB64 } from "@/delivery/b64";
import { deriveContactTag } from "@/delivery/tokenMessage";
import {
  blindSignToken,
  exportKeyJwk,
  generateInboxKeyPair,
  importPrivateKey,
  importPublicKey,
} from "@/delivery/rsabssa";

const META_INBOX = "__meta__/inbox-record";
const INBOX_LOOKUP_TTL_MS = 60_000;
const inboxLookupCache = new Map<
  string,
  { at: number; info: { inboxId: string; epoch: number; ipkN: string; ipkE: string } }
>();

type MetaStore = {
  getMeta?: <T>(key: string) => Promise<T | null>;
  setMeta?: <T>(key: string, value: T) => Promise<void>;
};

export async function deriveInboxId(publicKey: CryptoKey, salt: Uint8Array): Promise<string> {
  const spki = new Uint8Array(await crypto.subtle.exportKey("spki", publicKey));
  const buf = new Uint8Array(spki.length + salt.length);
  buf.set(spki, 0);
  buf.set(salt, spki.length);
  const hash = await crypto.subtle.digest("SHA-256", buf);
  return (
    "0x" +
    [...new Uint8Array(hash)].map((b) => b.toString(16).padStart(2, "0")).join("")
  );
}

function randomSalt(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(16));
}

export class InboxManager {
  private record: InboxRecord | null = null;
  private account = "";

  constructor(private meta: MetaStore) {}

  async ensure(account: string): Promise<InboxRecord> {
    this.account = account;
    if (this.record) return this.record;
    const cached = await this.meta.getMeta?.<InboxRecord>(META_INBOX);
    if (cached) {
      this.record = cached;
      this.account = account;
      return cached;
    }
    const { publicKey, privateKey } = await generateInboxKeyPair(
      config.deliveryModulusBits,
    );
    const salt = randomSalt();
    const inboxId = await deriveInboxId(publicKey, salt);
    const rec: InboxRecord = {
      inboxId,
      epoch: 0,
      saltB64: bytesToB64(salt),
      privateKeyJwk: await exportKeyJwk(privateKey),
      publicKeyJwk: await exportKeyJwk(publicKey),
      contactTags: {},
      revokedTags: [],
    };
    await this.meta.setMeta?.(META_INBOX, rec);
    this.record = rec;
    return rec;
  }

  get(): InboxRecord | null {
    return this.record;
  }

  async contactTagFor(sender: string): Promise<Uint8Array> {
    return deriveContactTag(this.account, sender);
  }

  async signBlinds(peer: string, blinds: Uint8Array[]): Promise<Uint8Array[]> {
    const rec = this.record!;
    const priv = await importPrivateKey(rec.privateKeyJwk);
    void this.contactTagFor(peer);
    return Promise.all(blinds.map((b) => blindSignToken(priv, b)));
  }

  async publicKey(): Promise<CryptoKey> {
    return importPublicKey(this.record!.publicKeyJwk);
  }

  ipkPayload(): { ipkN: string; ipkE: string; inboxId: string; epoch: number } {
    const rec = this.record!;
    return {
      inboxId: rec.inboxId,
      epoch: rec.epoch,
      ipkN: rec.publicKeyJwk.n!,
      ipkE: rec.publicKeyJwk.e!,
    };
  }

  async registerRelay(account: string): Promise<void> {
    if (!config.relayWs || !this.record) return;
    await this.sendInboxRegister(account);
  }

  /// EN: §6.5 ③ — bump the inbox epoch and re-register on the relay. Tokens bind
  /// `t ‖ ct ‖ epoch` (tokenMessage), so advancing the epoch invalidates every
  /// previously issued token: this is THE action that closes the spent-replay
  /// window after a relay wipe / chain-anchor recovery. Persists before
  /// re-registering so a failed relay round-trip cannot roll the epoch back.
  /// CN: §6.5 ③——递增信箱 epoch 并向 relay 重注册。令牌绑定 `t ‖ ct ‖ epoch`
  /// （tokenMessage），epoch 前进即作废所有已签发旧令牌：这就是 relay 失库 /
  /// 链锚恢复后**关闭 spent 重放窗口**的动作。先持久化再重注册，relay 往返失败
  /// 也不会让 epoch 回退。
  async bumpEpoch(account: string): Promise<number> {
    const rec = this.record ?? (await this.ensure(account));
    rec.epoch += 1;
    await this.meta.setMeta?.(META_INBOX, rec);
    this.record = rec;
    await this.sendInboxRegister(account);
    return rec.epoch;
  }

  private inboxRegisterPayload(account: string): Record<string, unknown> {
    const { inboxId, epoch, ipkN, ipkE } = this.ipkPayload();
    return {
      type: "inbox_register",
      account,
      inbox_id: inboxId,
      epoch,
      ipk_n: ipkN,
      ipk_e: ipkE,
      revoked_tags: this.record!.revokedTags,
    };
  }

  /// EN: Register inbox on relay; on `inbox_reject{stale_epoch}` adopt server epoch and retry once.
  /// CN: 向 relay 注册 inbox；遇 `inbox_reject{stale_epoch}` 采纳服务端 epoch 并重试一次。
  private async sendInboxRegister(account: string): Promise<void> {
    const { inboxId, epoch, ipkN, ipkE } = this.ipkPayload();
    cacheInboxLookup(account, { inboxId, epoch, ipkN, ipkE });
    const payload = this.inboxRegisterPayload(account);
    try {
      await relayOneShotSend(account, payload, { ackType: "inbox_ack" });
    } catch (e) {
      if (!(e instanceof RelayInboxStaleEpochError) || !this.record) throw e;
      this.record.epoch = Math.max(this.record.epoch, e.remoteEpoch);
      await this.meta.setMeta?.(META_INBOX, this.record);
      await relayOneShotSend(account, this.inboxRegisterPayload(account), { ackType: "inbox_ack" });
    }
  }
}

export interface RelayInboxInfo {
  inboxId: string;
  epoch: number;
  ipkN: string;
  ipkE: string;
  /** EN: Whether the account has a live relay WebSocket (for token_req). CN: 是否有在线 relay WS（token_req 用）。 */
  online?: boolean;
}

export async function lookupInbox(
  account: string,
  opts?: { fresh?: boolean },
): Promise<RelayInboxInfo | null> {
  if (!config.relayWs) return null;
  if (!opts?.fresh) {
    const hit = inboxLookupCache.get(account);
    if (hit && Date.now() - hit.at < INBOX_LOOKUP_TTL_MS) return hit.info;
  }
  const info = await wsInboxLookup(account);
  if (info) inboxLookupCache.set(account, { at: Date.now(), info });
  return info;
}

/// EN: Cache peer inbox after relay `inbox_register` (avoids extra WS round-trip). CN: 注册后写入缓存。
export function cacheInboxLookup(account: string, info: RelayInboxInfo): void {
  inboxLookupCache.set(account, { at: Date.now(), info });
}

async function wsInboxLookup(account: string): Promise<RelayInboxInfo | null> {
  if (!config.relayWs) return null;
  return withOneShotSlot(() => new Promise<RelayInboxInfo | null>((resolve) => {
    const ws = new WebSocket(config.relayWs);
    const request_id = globalThis.crypto?.randomUUID?.() ?? `r-${Date.now()}`;
    const t = setTimeout(() => {
      ws.close();
      resolve(null);
    }, 4000);
    ws.onopen = () =>
      ws.send(JSON.stringify({ type: "inbox_lookup", account, request_id }));
    ws.onmessage = (ev) => {
      try {
        const m = JSON.parse(String(ev.data)) as {
          type?: string;
          request_id?: string;
          inbox_id?: string;
          epoch?: number;
          ipk_n?: string;
          ipk_e?: string;
          online?: boolean;
        };
        if (m.type === "inbox_reply" && m.request_id === request_id && m.inbox_id) {
          clearTimeout(t);
          ws.close();
          resolve({
            inboxId: m.inbox_id,
            epoch: m.epoch ?? 0,
            ipkN: m.ipk_n!,
            ipkE: m.ipk_e!,
            online: m.online === true,
          });
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
