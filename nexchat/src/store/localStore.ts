// EN: LocalStore — local conversation state + message timeline. In the real client
// this is encrypted SQLite (web: IndexedDB/SQLCipher-wasm). Phase 0 uses an in-memory
// impl seeded from mock fixtures so the merge + UI run end-to-end.
// CN: LocalStore——本地会话状态 + 消息时间线。真实客户端为加密 SQLite（网页：
// IndexedDB / SQLCipher-wasm）。Phase 0 用内存实现 + mock 夹具，跑通 merge + UI。

import type { LocalConv } from "@/merge/spec";
import type { MessageVM } from "@/types/viewModels";
import { mockLocalConvs, mockMessages } from "@/mock/mockData";
import { config } from "@/config";
import { EncryptedIdbLocalStore } from "@/store/encryptedLocalStore";
import { burnAtOnRead, isExpired } from "@/ephemeral/ephemeral";

export interface LocalStore {
  /** EN: open the per-account backing store (encrypted impl only); no-op in memory. CN: 打开按账户的底层存储（仅加密实现有效），内存实现为空操作。 */
  open?(namespace: string): Promise<void>;
  listLocalConvs(): Promise<LocalConv[]>;
  listMessages(convId: string): Promise<MessageVM[]>;
  appendMessage(msg: MessageVM): Promise<void>;
  /** EN: insert or LWW-update by `clientMsgId` (archive restore). CN: 按 clientMsgId 插入或 LWW 更新（归档恢复）。 */
  upsertMessage?(msg: MessageVM): Promise<void>;
  setPref(
    convId: string,
    pref: Partial<Pick<LocalConv, "pinnedPref" | "dndPref" | "archivedPref">>,
  ): Promise<void>;
  markRead(convId: string): Promise<void>;
  markMentionsRead(convId: string): Promise<void>;
  bumpUnread(convId: string): Promise<void>;
  bumpMentionUnread(convId: string): Promise<void>;
  /** EN: ensure a local conv row exists (e.g. inbound from a new peer). CN: 确保本地会话行存在。 */
  ensureConv(convId: string): Promise<void>;
  setConvTitle(convId: string, title: string): Promise<void>;
  getMessage(convId: string, clientMsgId: string): Promise<MessageVM | undefined>;
  /** EN: patch fields on an existing message row. CN: 更新已有消息行的字段。 */
  updateMessage(
    convId: string,
    clientMsgId: string,
    patch: Partial<MessageVM>,
  ): Promise<void>;
  /** EN: delete one message. CN: 删除一条消息。 */
  deleteMessage(convId: string, clientMsgId: string): Promise<void>;
  /** EN: delete every message in a conversation; returns the removed clientMsgIds.
   * CN: 删除会话内全部消息，返回被删除的 clientMsgId。 */
  clearMessages(convId: string): Promise<string[]>;
  /** EN: remove local conv row + all messages (e.g. after group disband). CN: 删除本地会话行及全部消息（如群解散后）。 */
  removeLocalConversation(convId: string): Promise<void>;
  /** EN: remove messages whose `ephemeralBurnAt <= now`. CN: 删除 `ephemeralBurnAt <= now` 的消息。 */
  purgeExpiredEphemeral(now: number): Promise<{ convId: string; removed: string[] }[]>;
  /** EN: arm `burn_on: read` messages in a conversation. CN: 为会话内 read 模式消息启动倒计时。 */
  armEphemeralOnRead(convId: string, now: number): Promise<void>;
  /** EN: encrypted meta blob store (inbox, token wallet, conv-index pointer). CN: 加密元数据存储。 */
  getMeta?<T>(key: string): Promise<T | null>;
  setMeta?<T>(key: string, value: T): Promise<void>;
}

export class InMemoryLocalStore implements LocalStore {
  private convs = new Map<string, LocalConv>();
  private timelines = new Map<string, MessageVM[]>();
  private meta = new Map<string, unknown>();

  async open(): Promise<void> {
    /* no-op: in-memory store needs no per-account backing db */
  }

  constructor() {
    if (config.useMock) {
      for (const c of mockLocalConvs()) {
        this.convs.set(this.keyOf(c), c);
      }
    }
  }

  private keyOf(c: LocalConv): string {
    return c.kind === "direct" ? `d:${c.peer ?? ""}` : `g:${c.groupId ?? ""}`;
  }

  async listLocalConvs(): Promise<LocalConv[]> {
    return [...this.convs.values()];
  }

  async listMessages(convId: string): Promise<MessageVM[]> {
    if (!this.timelines.has(convId) && config.useMock) {
      this.timelines.set(convId, mockMessages(convId));
    }
    return this.timelines.get(convId) ?? [];
  }

  async appendMessage(msg: MessageVM): Promise<void> {
    const list = this.timelines.get(msg.convId) ?? [];
    list.push(msg);
    this.timelines.set(msg.convId, list);
    const conv = this.convs.get(msg.convId);
    if (conv) {
      conv.lastActive = msg.sentAt;
      conv.lastMessagePreview =
        msg.content.type === "text" ? msg.content.text : `[${msg.content.type}]`;
    }
  }

  async upsertMessage(msg: MessageVM): Promise<void> {
    const list = this.timelines.get(msg.convId) ?? [];
    const i = list.findIndex((m) => m.clientMsgId === msg.clientMsgId);
    if (i < 0) {
      list.push(msg);
    } else {
      list[i] = mergeMessageRow(list[i]!, msg);
    }
    this.timelines.set(msg.convId, list);
    const conv = this.convs.get(msg.convId);
    if (conv && msg.sentAt >= conv.lastActive) {
      conv.lastActive = msg.sentAt;
      conv.lastMessagePreview =
        msg.content.type === "text" ? msg.content.text : `[${msg.content.type}]`;
    }
  }

  async setPref(
    convId: string,
    pref: Partial<Pick<LocalConv, "pinnedPref" | "dndPref" | "archivedPref">>,
  ): Promise<void> {
    const conv = this.convs.get(convId);
    if (conv) Object.assign(conv, pref);
  }

  async markRead(convId: string): Promise<void> {
    const conv = this.convs.get(convId);
    if (conv) conv.unread = 0;
  }

  async markMentionsRead(convId: string): Promise<void> {
    const conv = this.convs.get(convId);
    if (conv) conv.mentionUnread = 0;
  }

  async bumpUnread(convId: string): Promise<void> {
    const conv = this.convs.get(convId);
    if (conv) conv.unread += 1;
  }

  async bumpMentionUnread(convId: string): Promise<void> {
    const conv = this.convs.get(convId);
    if (conv) conv.mentionUnread = (conv.mentionUnread ?? 0) + 1;
  }

  async setConvTitle(convId: string, title: string): Promise<void> {
    const c = this.convs.get(convId);
    if (c) c.title = title;
  }

  async ensureConv(convId: string): Promise<void> {
    if (this.convs.has(convId)) return;
    // convId form: "d:{peer}" | "g:{groupId}"
    if (convId.startsWith("d:")) {
      const peer = convId.slice(2);
      this.convs.set(convId, {
        kind: "direct",
        peer,
        lastActive: Date.now(),
        unread: 0,
        title: peer,
      });
    } else if (convId.startsWith("g:")) {
      this.convs.set(convId, {
        kind: "group",
        groupId: Number(convId.slice(2)),
        lastActive: Date.now(),
        unread: 0,
      });
    }
  }

  async getMessage(convId: string, clientMsgId: string): Promise<MessageVM | undefined> {
    return (this.timelines.get(convId) ?? []).find((m) => m.clientMsgId === clientMsgId);
  }

  async updateMessage(
    convId: string,
    clientMsgId: string,
    patch: Partial<MessageVM>,
  ): Promise<void> {
    const list = this.timelines.get(convId) ?? [];
    const i = list.findIndex((m) => m.clientMsgId === clientMsgId);
    if (i < 0) return;
    list[i] = { ...list[i]!, ...patch };
    this.timelines.set(convId, list);
  }

  async deleteMessage(convId: string, clientMsgId: string): Promise<void> {
    const list = this.timelines.get(convId) ?? [];
    this.timelines.set(
      convId,
      list.filter((m) => m.clientMsgId !== clientMsgId),
    );
  }

  async clearMessages(convId: string): Promise<string[]> {
    const removed = (this.timelines.get(convId) ?? []).map((m) => m.clientMsgId);
    this.timelines.set(convId, []);
    const conv = this.convs.get(convId);
    if (conv) conv.lastMessagePreview = undefined;
    return removed;
  }

  async removeLocalConversation(convId: string): Promise<void> {
    this.convs.delete(convId);
    this.timelines.delete(convId);
  }

  async purgeExpiredEphemeral(now: number): Promise<{ convId: string; removed: string[] }[]> {
    const hits: { convId: string; removed: string[] }[] = [];
    for (const [convId, list] of this.timelines) {
      const removed: string[] = [];
      const kept = list.filter((m) => {
        if (isExpired(m, now)) {
          removed.push(m.clientMsgId);
          return false;
        }
        return true;
      });
      if (removed.length > 0) {
        this.timelines.set(convId, kept);
        hits.push({ convId, removed });
      }
    }
    return hits;
  }

  async armEphemeralOnRead(convId: string, now: number): Promise<void> {
    const list = this.timelines.get(convId) ?? [];
    for (const m of list) {
      if (m.ephemeralTtlMs && m.ephemeralBurnOn === "read" && !m.ephemeralBurnAt) {
        await this.updateMessage(convId, m.clientMsgId, {
          ephemeralBurnAt: burnAtOnRead(m.ephemeralTtlMs, now),
        });
      }
    }
  }

  async getMeta<T>(key: string): Promise<T | null> {
    return (this.meta.get(key) as T | undefined) ?? null;
  }

  async setMeta<T>(key: string, value: T): Promise<void> {
    this.meta.set(key, value);
  }
}

// EN: Live browser sessions persist the encrypted timeline to IndexedDB so messages survive
// refreshes/restarts; mock and non-browser (test/node) runs keep the in-memory store seeded
// from fixtures. CN: 真实浏览器会话把加密时间线落到 IndexedDB，跨刷新/重开保留消息；mock 与
// 非浏览器（测试/node）环境沿用内存实现（从夹具填充）。
function makeLocalStore(): LocalStore {
  if (!config.useMock && typeof indexedDB !== "undefined") {
    return new EncryptedIdbLocalStore();
  }
  return new InMemoryLocalStore();
}

function statusRank(s: MessageVM["status"]): number {
  if (s === "acked") return 4;
  if (s === "sent") return 3;
  if (s === "pending") return 2;
  if (s === "failed") return 1;
  return 0;
}

function mergeMessageRow(prev: MessageVM, next: MessageVM): MessageVM {
  if (next.sentAt > prev.sentAt) return next;
  if (prev.sentAt > next.sentAt) return prev;
  return statusRank(next.status) >= statusRank(prev.status) ? next : prev;
}

export const localStore: LocalStore = makeLocalStore();
