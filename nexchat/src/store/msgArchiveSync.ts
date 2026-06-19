// EN: Orchestrates message-archive restore (IPFS + decrypt + merge) and push.
// CN: 编排消息历史归档的恢复与推送。

import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import {
  fetchMsgArchivePointer,
  publishMsgArchivePointer,
  readLocalMsgArchivePointer,
} from "@/relay/msgArchivePointer";
import type { LocalStore } from "@/store/localStore";
import {
  archiveEntryToMessage,
  buildArchiveFromLocal,
  decryptArchiveBlob,
  encryptArchiveBlob,
  mergeArchiveBlobs,
  mergeArchiveWithTombstones,
  tombstonesForRemovedMessages,
  type MessageArchiveBlob,
  type MsgArchivePointer,
} from "@/store/msgArchive";
import { deviceId } from "@/store/convIndex";

const META_MSG_ARCHIVE_ID = "__meta__/msg-archive-pointer";

// EN: Bounded delayed re-pull schedule for the §4.5 "middle window" gap refill. An online sibling that
// received the gap messages archives them on a ~2s push debounce, so a short first delay catches the
// common case while the later ones cover a slow/late sibling push. CN: §4.5「中间空窗」补齐的有界延迟
// 重拉时刻表。收到空窗消息的在线兄弟约 2s push debounce 后归档，故首个短延迟覆盖常见情况，后续延迟兜底
// 兄弟推送较慢/较晚的情况。
const GAP_REFILL_DELAYS_MS = [3_000, 12_000, 40_000];

export class MsgArchiveSync {
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private lastBlob: MessageArchiveBlob | null = null;
  private pushing = false;
  private refillTimers: Array<ReturnType<typeof setTimeout>> = [];

  constructor(
    private store: LocalStore,
    private hasMeta: boolean,
  ) {}

  /// EN: Pull remote archive, merge into local timelines. CN: 拉取远端归档并合并到本地时间线。
  async restore(account: string): Promise<boolean> {
    if (!config.msgArchiveEnabled || !config.ipfsEnabled) return false;
    try {
      const ptr = await this.resolvePointer(account);
      if (!ptr) return false;
      const packed = await ipfsClient.cat(ptr.cid);
      const remote = await decryptArchiveBlob(packed);
      const localBlob = await buildArchiveFromLocal(
        this.store,
        config.msgArchiveMaxPerConv,
        deviceId(),
      );
      const merged = mergeArchiveBlobs(localBlob, remote);
      this.lastBlob = merged;
      await this.applyArchive(merged);
      if (this.hasMeta) await this.saveMetaPointer(ptr);
      return true;
    } catch (e) {
      console.warn("[nexchat] msg-archive restore failed:", e);
      return false;
    }
  }

  /// EN: Eventual-consistency gap refill (hybrid design §4.5). After a new device is grafted into a 1:1,
  /// messages the peer sent BEFORE the graft (the "middle window") are NOT MLS-decryptable here and may
  /// post-date the archive snapshot pulled at unlock. An online sibling that received them archives the
  /// plaintext shortly after; this re-pulls the archive on a bounded delayed schedule so those messages
  /// eventually appear. Each re-pull is an idempotent merge; `onApplied` fires after a re-pull that
  /// actually restored, so the caller can refresh the UI. Coalesces: re-scheduling resets the pending
  /// sequence. CN: 最终一致性空窗补齐（混合设计 §4.5）。新设备被嫁接进 1:1 后，对端在嫁接**前**所发消息
  /// （「中间空窗」）在此不可 MLS 解密、且可能晚于解锁时拉取的 archive 快照。收到它们的在线兄弟稍后归档其
  /// 明文；本方法按有界延迟重拉 archive，使这些消息最终出现。每次重拉为幂等合并；重拉确实恢复后触发
  /// `onApplied`，便于调用方刷新 UI。合并调度：重新调度会重置待执行序列。
  scheduleGapRefill(
    account: string,
    onApplied?: () => void,
    delaysMs: number[] = GAP_REFILL_DELAYS_MS,
  ): void {
    if (!config.msgArchiveEnabled || !config.ipfsEnabled) return;
    this.clearRefillTimers();
    for (const delay of delaysMs) {
      this.refillTimers.push(
        setTimeout(() => {
          void this.restore(account).then((ok) => {
            if (ok) onApplied?.();
          });
        }, delay),
      );
    }
  }

  private clearRefillTimers(): void {
    for (const t of this.refillTimers) clearTimeout(t);
    this.refillTimers = [];
  }

  schedulePush(account: string): void {
    if (!config.msgArchiveEnabled || !config.ipfsEnabled) return;
    if (this.pushTimer) clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => {
      this.pushTimer = null;
      void this.push(account);
    }, 2000);
  }

  async push(account: string): Promise<void> {
    if (!config.msgArchiveEnabled || !config.ipfsEnabled || this.pushing) return;
    this.pushing = true;
    try {
      let blob = await buildArchiveFromLocal(
        this.store,
        config.msgArchiveMaxPerConv,
        deviceId(),
      );

      if (this.lastBlob) {
        const tombstones = tombstonesForRemovedMessages(this.lastBlob, blob);
        if (tombstones.length) blob = mergeArchiveWithTombstones(blob, tombstones);
      }

      const remotePtr = await fetchMsgArchivePointer(account);
      const localPtr = this.hasMeta
        ? await this.loadMetaPointer()
        : readLocalMsgArchivePointer(account);

      const bestPtr =
        remotePtr && localPtr
          ? remotePtr.updated_at >= localPtr.updated_at
            ? remotePtr
            : localPtr
          : remotePtr ?? localPtr;

      if (bestPtr && (!this.lastBlob || bestPtr.updated_at > (this.lastBlob.updated_at ?? 0))) {
        try {
          const packed = await ipfsClient.cat(bestPtr.cid);
          const remote = await decryptArchiveBlob(packed);
          blob = mergeArchiveBlobs(remote, blob);
        } catch {
          /* stale cid */
        }
      }

      const packed = await encryptArchiveBlob(blob);
      const cid = await ipfsClient.add(packed, "msg-archive.enc");
      const ptr: MsgArchivePointer = { cid, updated_at: blob.updated_at };
      this.lastBlob = blob;
      if (this.hasMeta) await this.saveMetaPointer(ptr);
      await publishMsgArchivePointer(account, ptr);
    } catch (e) {
      console.warn("[nexchat] msg-archive push failed:", e);
    } finally {
      this.pushing = false;
    }
  }

  private async applyArchive(blob: MessageArchiveBlob): Promise<void> {
    for (const conv of blob.conversations) {
      for (const e of conv.messages) {
        if (e.tombstone) {
          await this.store.deleteMessage(e.conv_id, e.client_msg_id);
          continue;
        }
        await this.store.ensureConv(e.conv_id);
        const msg = archiveEntryToMessage(e);
        if (this.store.upsertMessage) {
          await this.store.upsertMessage(msg);
        } else {
          const exists = await this.store.getMessage(msg.convId, msg.clientMsgId);
          if (!exists) await this.store.appendMessage(msg);
        }
      }
    }
  }

  private async resolvePointer(account: string): Promise<MsgArchivePointer | null> {
    const meta = this.hasMeta ? await this.loadMetaPointer() : null;
    const relay = await fetchMsgArchivePointer(account);
    if (!meta) return relay;
    if (!relay) return meta;
    return relay.updated_at >= meta.updated_at ? relay : meta;
  }

  private async loadMetaPointer(): Promise<MsgArchivePointer | null> {
    return this.store.getMeta?.(META_MSG_ARCHIVE_ID) ?? null;
  }

  private async saveMetaPointer(ptr: MsgArchivePointer): Promise<void> {
    await this.store.setMeta?.(META_MSG_ARCHIVE_ID, ptr);
  }
}

let sync: MsgArchiveSync | null = null;

export function msgArchiveSyncFor(store: LocalStore): MsgArchiveSync {
  if (!sync) {
    const hasMeta = typeof (store as { setMeta?: unknown }).setMeta === "function";
    sync = new MsgArchiveSync(store, hasMeta);
  }
  return sync;
}

export function scheduleMsgArchivePush(account: string): void {
  if (sync) sync.schedulePush(account);
}

/// EN: Schedule the §4.5 eventual-consistency gap refill (no-op until the archive sync singleton exists,
/// which it does after the unlock restore). CN: 调度 §4.5 最终一致性空窗补齐（archive sync 单例存在前为
/// 空操作；解锁恢复后即存在）。
export function scheduleMsgArchiveGapRefill(account: string, onApplied?: () => void): void {
  if (sync) sync.scheduleGapRefill(account, onApplied);
}

export async function restoreMsgArchive(account: string, store: LocalStore): Promise<boolean> {
  return msgArchiveSyncFor(store).restore(account);
}
