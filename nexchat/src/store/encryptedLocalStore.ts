// EN: EncryptedIdbLocalStore — at-rest-encrypted LocalStore backed by IndexedDB. Each row
// (a local conversation or a single message) is stored as { iv, ct } where `ct` is the
// AES-256-GCM ciphertext of the JSON value under a non-extractable per-account key from the
// KeyVault. This is the web stand-in for the client's encrypted SQLite/SQLCipher store: it
// keeps the message timeline + local conversation prefs across refreshes/restarts WITHOUT
// ever writing plaintext to disk. The plaintext model still lives only in memory at runtime.
// CN: EncryptedIdbLocalStore——基于 IndexedDB 的“静态加密”LocalStore。每行（一个本地会话或
// 一条消息）以 { iv, ct } 存储，`ct` 是用 KeyVault 派生的不可导出按账户密钥对 JSON 值做的
// AES-256-GCM 密文。这是客户端加密 SQLite/SQLCipher 在网页端的替身：跨刷新/重开保留消息
// 时间线 + 本地会话偏好，且**绝不**把明文落盘；运行期明文模型仍只在内存。

import type { LocalConv } from "@/merge/spec";
import type { MessageVM } from "@/types/viewModels";
import type { LocalStore } from "@/store/localStore";
import { keyVault } from "@/keyvault/keyvault";
import { burnAtOnRead, isExpired } from "@/ephemeral/ephemeral";

const DB_PREFIX = "nexchat-local";
const STORE_CONVS = "convs";
const STORE_MSGS = "messages";
const STORE_META = "meta";
const META_PREFIX = "__meta__/";

interface Sealed {
  iv: Uint8Array;
  ct: ArrayBuffer;
}

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

function txDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

// EN: Plain (unencrypted) KDF-version marker row in STORE_META; once present, the one-time
// legacy→vault_master re-seal migration (ADR CHAT_SYNC_ANCHOR §5.0) is skipped.
// CN: STORE_META 中的明文 KDF 版本标记行；存在后跳过一次性「旧根→vault_master 重封」迁移
// （ADR CHAT_SYNC_ANCHOR §5.0）。
const KDF_MARKER_KEY = "__kdfv__";
const KDF_VERSION = 2;

export class EncryptedIdbLocalStore implements LocalStore {
  private db: IDBDatabase | null = null;
  private key: CryptoKey | null = null;
  private legacyKey: CryptoKey | null = null;
  private ready: Promise<void> | null = null;

  /// EN: Open (or create) the per-namespace encrypted database and derive the at-rest key.
  /// Must be awaited before any other call. Idempotent for the same namespace.
  /// CN: 打开（或创建）按命名空间隔离的加密库并派生静态密钥；其他调用前必须先 await，
  /// 同一命名空间幂等。
  open(namespace: string): Promise<void> {
    if (this.ready) return this.ready;
    this.ready = (async () => {
      this.key = await keyVault.deriveLocalStoreKey(namespace);
      this.legacyKey = await keyVault.deriveLegacyLocalStoreKey(namespace);
      this.db = await new Promise<IDBDatabase>((resolve, reject) => {
        const open = indexedDB.open(`${DB_PREFIX}-${namespace}`, 2);
        open.onupgradeneeded = () => {
          const db = open.result;
          if (!db.objectStoreNames.contains(STORE_CONVS)) db.createObjectStore(STORE_CONVS);
          if (!db.objectStoreNames.contains(STORE_MSGS)) db.createObjectStore(STORE_MSGS);
          if (!db.objectStoreNames.contains(STORE_META)) db.createObjectStore(STORE_META);
        };
        open.onsuccess = () => resolve(open.result);
        open.onerror = () => reject(open.error);
      });
      await this.migrateLegacyRows();
    })();
    return this.ready;
  }

  private async tryUnsealWith(key: CryptoKey, sealed: Sealed): Promise<Uint8Array | null> {
    try {
      const pt = await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: sealed.iv as BufferSource },
        key,
        sealed.ct,
      );
      return new Uint8Array(pt);
    } catch (e) {
      if (e instanceof DOMException && e.name === "OperationError") return null;
      throw e;
    }
  }

  // EN: One-time migration (ADR §5.0): the pre-vault_master at-rest key was derived from
  // the public address, so every legacy row is re-sealed under the new key here. Rows that
  // authenticate under neither key are left alone (unseal() already skips them on read).
  // CN: 一次性迁移（ADR §5.0）：vault_master 之前的静态钥由公开地址派生，此处把所有旧行用
  // 新钥重封。两把钥都解不开的行原样保留（读取时 unseal() 已会跳过）。
  private async migrateLegacyRows(): Promise<void> {
    // EN: only meaningful when rooted in vault_master; legacy/test roots must not claim
    // (via the marker) that rows are sealed under the new root.
    // CN: 仅在 vault_master 根下有意义；旧根/测试根不得写标记谎称行已重封到新根。
    if (!keyVault.hasMasterRoot()) return;
    const metaTx = this.db!.transaction(STORE_META, "readonly");
    const marker = await req(
      metaTx.objectStore(STORE_META).get(KDF_MARKER_KEY) as IDBRequest<
        { v?: number } | undefined
      >,
    );
    await txDone(metaTx);
    if (marker && typeof marker.v === "number" && marker.v >= KDF_VERSION) return;

    if (this.legacyKey) {
      for (const storeName of [STORE_CONVS, STORE_MSGS, STORE_META]) {
        const tx = this.db!.transaction(storeName, "readonly");
        const store = tx.objectStore(storeName);
        const keys = await req(store.getAllKeys() as IDBRequest<IDBValidKey[]>);
        const rows: { k: IDBValidKey; row: Sealed }[] = [];
        for (const k of keys) {
          if (String(k) === KDF_MARKER_KEY) continue;
          const row = await req(store.get(k) as IDBRequest<Sealed | undefined>);
          if (row && row.iv && row.ct) rows.push({ k, row });
        }
        await txDone(tx);
        const resealed: { k: IDBValidKey; sealed: Sealed }[] = [];
        for (const { k, row } of rows) {
          if ((await this.tryUnsealWith(this.key!, row)) !== null) continue;
          const pt = await this.tryUnsealWith(this.legacyKey, row);
          if (pt === null) continue;
          const iv = crypto.getRandomValues(new Uint8Array(12));
          const ct = await crypto.subtle.encrypt(
            { name: "AES-GCM", iv: iv as BufferSource },
            this.key!,
            pt as BufferSource,
          );
          resealed.push({ k, sealed: { iv, ct } });
        }
        if (resealed.length > 0) {
          const wtx = this.db!.transaction(storeName, "readwrite");
          const ws = wtx.objectStore(storeName);
          for (const { k, sealed } of resealed) ws.put(sealed, k);
          await txDone(wtx);
        }
      }
    }

    const wtx = this.db!.transaction(STORE_META, "readwrite");
    wtx.objectStore(STORE_META).put({ v: KDF_VERSION }, KDF_MARKER_KEY);
    await txDone(wtx);
  }

  private async seal(value: unknown): Promise<Sealed> {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const data = new TextEncoder().encode(JSON.stringify(value));
    const ct = await crypto.subtle.encrypt(
      { name: "AES-GCM", iv: iv as BufferSource },
      this.key!,
      data,
    );
    return { iv, ct };
  }

  private async unseal<T>(sealed: Sealed | undefined): Promise<T | undefined> {
    if (!sealed) return undefined;
    try {
      const pt = await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: sealed.iv as BufferSource },
        this.key!,
        sealed.ct,
      );
      return JSON.parse(new TextDecoder().decode(pt)) as T;
    } catch (e) {
      // EN: stale/corrupt row (account switch, schema change) — skip instead of crashing UI.
      // CN: 过期/损坏行（换账户、schema 变更）——跳过，避免拖垮 UI。
      if (e instanceof DOMException && e.name === "OperationError") {
        return undefined;
      }
      throw e;
    }
  }

  // EN: message rows are keyed `${convId}::${clientMsgId}` so a single conversation is a
  // contiguous key range — no plaintext index needed. CN: 消息行键为 `${convId}::${clientMsgId}`，
  // 同一会话即一段连续键区间，无需明文索引。
  private msgKey(convId: string, clientMsgId: string): string {
    return `${convId}::${clientMsgId}`;
  }

  async listLocalConvs(): Promise<LocalConv[]> {
    await this.ready;
    const tx = this.db!.transaction(STORE_CONVS, "readonly");
    const store = tx.objectStore(STORE_CONVS);
    const keys = await req(store.getAllKeys() as IDBRequest<IDBValidKey[]>);
    const sealedRows: Sealed[] = [];
    for (const k of keys) {
      if (String(k).startsWith(META_PREFIX)) continue;
      const row = await req(store.get(k) as IDBRequest<Sealed | undefined>);
      if (row) sealedRows.push(row);
    }
    await txDone(tx);
    const out: LocalConv[] = [];
    for (const sealed of sealedRows) {
      const c = await this.unseal<LocalConv>(sealed);
      if (c) out.push(c);
    }
    return out;
  }

  async getMeta<T>(key: string): Promise<T | null> {
    await this.ready;
    const tx = this.db!.transaction(STORE_META, "readonly");
    const sealed = await req(
      tx.objectStore(STORE_META).get(key) as IDBRequest<Sealed | undefined>,
    );
    await txDone(tx);
    return (await this.unseal<T>(sealed)) ?? null;
  }

  async setMeta<T>(key: string, value: T): Promise<void> {
    await this.ready;
    const sealed = await this.seal(value);
    const tx = this.db!.transaction(STORE_META, "readwrite");
    tx.objectStore(STORE_META).put(sealed, key);
    await txDone(tx);
  }

  async replaceLocalConvs(convs: LocalConv[]): Promise<void> {
    await this.ready;
    const rows = await Promise.all(
      convs.map(async (c) => ({
        convId: c.kind === "direct" ? `d:${c.peer ?? ""}` : `g:${c.groupId ?? 0}`,
        sealed: await this.seal(c),
      })),
    );
    const tx = this.db!.transaction(STORE_CONVS, "readwrite");
    const os = tx.objectStore(STORE_CONVS);
    for (const { convId, sealed } of rows) os.put(sealed, convId);
    await txDone(tx);
  }

  async listMessages(convId: string): Promise<MessageVM[]> {
    await this.ready;
    const tx = this.db!.transaction(STORE_MSGS, "readonly");
    const range = IDBKeyRange.bound(`${convId}::`, `${convId}::\uffff`);
    const sealed = await req(
      tx.objectStore(STORE_MSGS).getAll(range) as IDBRequest<Sealed[]>,
    );
    await txDone(tx);
    const out: MessageVM[] = [];
    for (const s of sealed) {
      const m = await this.unseal<MessageVM>(s);
      if (m) out.push(m);
    }
    out.sort((a, b) => a.sentAt - b.sentAt || a.clientMsgId.localeCompare(b.clientMsgId));
    return out;
  }

  async appendMessage(msg: MessageVM): Promise<void> {
    await this.ready;
    const sealedMsg = await this.seal(msg);
    // EN: read-modify-write the conv row (only if it exists, mirroring in-memory semantics).
    // CN: 读改写会话行（仅当存在，与内存实现语义一致）。
    const conv = await this.getConv(msg.convId);
    if (conv) {
      conv.lastActive = msg.sentAt;
      conv.lastMessagePreview =
        msg.content.type === "text" ? msg.content.text : `[${msg.content.type}]`;
    }
    const sealedConv = conv ? await this.seal(conv) : null;
    const stores = sealedConv ? [STORE_MSGS, STORE_CONVS] : [STORE_MSGS];
    const tx = this.db!.transaction(stores, "readwrite");
    tx.objectStore(STORE_MSGS).put(sealedMsg, this.msgKey(msg.convId, msg.clientMsgId));
    if (sealedConv) tx.objectStore(STORE_CONVS).put(sealedConv, msg.convId);
    await txDone(tx);
  }

  async upsertMessage(msg: MessageVM): Promise<void> {
    await this.ready;
    const existing = await this.getMessage(msg.convId, msg.clientMsgId);
    const merged = existing ? this.mergeMessageRow(existing, msg) : msg;
    await this.appendMessage(merged);
  }

  private mergeMessageRow(prev: MessageVM, next: MessageVM): MessageVM {
    const rank = (s: MessageVM["status"]) =>
      s === "acked" ? 4 : s === "sent" ? 3 : s === "pending" ? 2 : s === "failed" ? 1 : 0;
    if (next.sentAt > prev.sentAt) return next;
    if (prev.sentAt > next.sentAt) return prev;
    return rank(next.status) >= rank(prev.status) ? next : prev;
  }

  async setPref(
    convId: string,
    pref: Partial<Pick<LocalConv, "pinnedPref" | "dndPref" | "archivedPref">>,
  ): Promise<void> {
    await this.mutateConv(convId, (c) => Object.assign(c, pref));
  }

  async markRead(convId: string): Promise<void> {
    await this.mutateConv(convId, (c) => {
      c.unread = 0;
    });
  }

  async markMentionsRead(convId: string): Promise<void> {
    await this.mutateConv(convId, (c) => {
      c.mentionUnread = 0;
    });
  }

  async bumpUnread(convId: string): Promise<void> {
    await this.mutateConv(convId, (c) => {
      c.unread += 1;
    });
  }

  async bumpMentionUnread(convId: string): Promise<void> {
    await this.mutateConv(convId, (c) => {
      c.mentionUnread = (c.mentionUnread ?? 0) + 1;
    });
  }

  async setConvTitle(convId: string, title: string): Promise<void> {
    await this.mutateConv(convId, (c) => {
      c.title = title;
    });
  }

  async ensureConv(convId: string): Promise<void> {
    await this.ready;
    if (await this.getConv(convId)) return;
    let conv: LocalConv | null = null;
    if (convId.startsWith("d:")) {
      const peer = convId.slice(2);
      conv = { kind: "direct", peer, lastActive: Date.now(), unread: 0, title: peer };
    } else if (convId.startsWith("g:")) {
      conv = { kind: "group", groupId: Number(convId.slice(2)), lastActive: Date.now(), unread: 0 };
    }
    if (conv) await this.putConv(convId, conv);
  }

  async getMessage(convId: string, clientMsgId: string): Promise<MessageVM | undefined> {
    await this.ready;
    const tx = this.db!.transaction(STORE_MSGS, "readonly");
    const sealed = await req(
      tx.objectStore(STORE_MSGS).get(this.msgKey(convId, clientMsgId)) as IDBRequest<Sealed>,
    );
    await txDone(tx);
    return this.unseal<MessageVM>(sealed);
  }

  private async getConv(convId: string): Promise<LocalConv | undefined> {
    const tx = this.db!.transaction(STORE_CONVS, "readonly");
    const sealed = await req(
      tx.objectStore(STORE_CONVS).get(convId) as IDBRequest<Sealed>,
    );
    await txDone(tx);
    return this.unseal<LocalConv>(sealed);
  }

  private async putConv(convId: string, conv: LocalConv): Promise<void> {
    const sealed = await this.seal(conv);
    const tx = this.db!.transaction(STORE_CONVS, "readwrite");
    tx.objectStore(STORE_CONVS).put(sealed, convId);
    await txDone(tx);
  }

  private async mutateConv(convId: string, fn: (c: LocalConv) => void): Promise<void> {
    await this.ready;
    const conv = await this.getConv(convId);
    if (!conv) return;
    fn(conv);
    await this.putConv(convId, conv);
  }

  async updateMessage(
    convId: string,
    clientMsgId: string,
    patch: Partial<MessageVM>,
  ): Promise<void> {
    await this.ready;
    const msg = await this.getMessage(convId, clientMsgId);
    if (!msg) return;
    const sealed = await this.seal({ ...msg, ...patch });
    const tx = this.db!.transaction(STORE_MSGS, "readwrite");
    tx.objectStore(STORE_MSGS).put(sealed, this.msgKey(convId, clientMsgId));
    await txDone(tx);
  }

  async deleteMessage(convId: string, clientMsgId: string): Promise<void> {
    await this.ready;
    const tx = this.db!.transaction(STORE_MSGS, "readwrite");
    tx.objectStore(STORE_MSGS).delete(this.msgKey(convId, clientMsgId));
    await txDone(tx);
  }

  async clearMessages(convId: string): Promise<string[]> {
    await this.ready;
    const removed = (await this.listMessages(convId)).map((m) => m.clientMsgId);
    if (removed.length === 0) return [];
    const tx = this.db!.transaction(STORE_MSGS, "readwrite");
    const store = tx.objectStore(STORE_MSGS);
    for (const id of removed) store.delete(this.msgKey(convId, id));
    await txDone(tx);
    await this.mutateConv(convId, (c) => {
      c.lastMessagePreview = undefined;
    });
    return removed;
  }

  async removeLocalConversation(convId: string): Promise<void> {
    await this.ready;
    const msgTx = this.db!.transaction(STORE_MSGS, "readwrite");
    const msgStore = msgTx.objectStore(STORE_MSGS);
    const range = IDBKeyRange.bound(`${convId}::`, `${convId}::\uffff`);
    const keys = await req(msgStore.getAllKeys(range) as IDBRequest<IDBValidKey[]>);
    for (const k of keys) msgStore.delete(k);
    await txDone(msgTx);
    const convTx = this.db!.transaction(STORE_CONVS, "readwrite");
    convTx.objectStore(STORE_CONVS).delete(convId);
    await txDone(convTx);
  }

  async purgeExpiredEphemeral(now: number): Promise<{ convId: string; removed: string[] }[]> {
    await this.ready;
    const tx = this.db!.transaction(STORE_CONVS, "readonly");
    const convIds = (await req(
      tx.objectStore(STORE_CONVS).getAllKeys() as IDBRequest<IDBValidKey[]>,
    ))
      .map(String)
      .filter((id) => !id.startsWith(META_PREFIX));
    await txDone(tx);
    const hits: { convId: string; removed: string[] }[] = [];
    for (const convId of convIds) {
      const msgs = await this.listMessages(convId);
      const removed = msgs.filter((m) => isExpired(m, now)).map((m) => m.clientMsgId);
      for (const id of removed) await this.deleteMessage(convId, id);
      if (removed.length > 0) hits.push({ convId, removed });
    }
    return hits;
  }

  async armEphemeralOnRead(convId: string, now: number): Promise<void> {
    const msgs = await this.listMessages(convId);
    for (const m of msgs) {
      if (m.ephemeralTtlMs && m.ephemeralBurnOn === "read" && !m.ephemeralBurnAt) {
        await this.updateMessage(convId, m.clientMsgId, {
          ephemeralBurnAt: burnAtOnRead(m.ephemeralTtlMs, now),
        });
      }
    }
  }
}
