// EN: Orchestrates conv-index restore (IPFS fetch + decrypt + merge) and push (encrypt +
// IPFS add + pointer publish). CN: 编排 conv-index 恢复（IPFS 拉取+解密+合并）与推送。
//

import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import { fetchIndexPointer, publishIndexPointer } from "@/relay/indexPointer";
import type { LocalStore } from "@/store/localStore";
import {
  applyIndexToLocal,
  buildIndexFromLocal,
  decryptIndexBlob,
  deviceId,
  encryptIndexBlob,
  mergeIndexBlobs,
  type ConvIndexBlob,
  type ConvIndexPointer,
} from "@/store/convIndex";
import {
  applyIndexTombstones,
  loadDeletedConvIds,
  tombstonesForDeletedConversations,
} from "@/store/deletedConversations";

const META_CONV_ID = "__meta__/conv-index-pointer";

export class ConvIndexSync {
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private lastBlob: ConvIndexBlob | null = null;
  private pushing = false;

  constructor(
    private store: LocalStore,
    private hasMeta: boolean,
  ) {}

  /// EN: Pull remote index (if newer), merge into local conv rows. CN: 拉取远端索引并合并到本地。
  async restore(account: string): Promise<boolean> {
    if (!config.convIndexEnabled || !config.ipfsEnabled) return false;
    try {
      const ptr = await this.resolvePointer(account);
      if (!ptr) return false;
      const packed = await ipfsClient.cat(ptr.cid);
      const remote = await decryptIndexBlob(packed);
      const localConvs = await this.store.listLocalConvs();
      const localBlob = buildIndexFromLocal(localConvs, account, deviceId());
      const merged = mergeIndexBlobs(localBlob, remote);
      this.lastBlob = merged;
      await applyIndexTombstones(this.store, merged);
      await this.writeLocalConvs(applyIndexToLocal(localConvs, merged));
      if (this.hasMeta) await this.saveMetaPointer(ptr);
      return true;
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      console.warn("[nexchat] conv-index restore failed:", detail);
      return false;
    }
  }

  schedulePush(account: string): void {
    if (!config.convIndexEnabled || !config.ipfsEnabled) return;
    if (this.pushTimer) clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => {
      this.pushTimer = null;
      void this.push(account);
    }, 1500);
  }

  async push(account: string): Promise<void> {
    if (!config.convIndexEnabled || !config.ipfsEnabled || this.pushing) return;
    this.pushing = true;
    try {
      const localConvs = await this.store.listLocalConvs();
      let blob = buildIndexFromLocal(localConvs, account, deviceId());
      const deletedIds = await loadDeletedConvIds(this.store);
      const tombstones = tombstonesForDeletedConversations(this.lastBlob, localConvs, deletedIds);
      if (tombstones.length) {
        blob = mergeIndexBlobs(blob, {
          v: 1,
          updated_at: Date.now(),
          device_id: blob.device_id,
          conversations: tombstones,
        });
      }

      const remotePtr = await fetchIndexPointer(account);
      const localPtr = this.hasMeta ? await this.loadMetaPointer() : null;
      const bestPtr = remotePtr && localPtr
        ? remotePtr.updated_at >= localPtr.updated_at
          ? remotePtr
          : localPtr
        : remotePtr ?? localPtr;

      if (bestPtr && (!this.lastBlob || bestPtr.updated_at > (this.lastBlob.updated_at ?? 0))) {
        try {
          const packed = await ipfsClient.cat(bestPtr.cid);
          const remote = await decryptIndexBlob(packed);
          blob = mergeIndexBlobs(remote, blob);
        } catch {
          /* stale cid */
        }
      }

      const packed = await encryptIndexBlob(blob);
      const cid = await ipfsClient.add(packed, "conv-index.enc");
      const ptr: ConvIndexPointer = { cid, updated_at: blob.updated_at };
      this.lastBlob = blob;
      if (this.hasMeta) await this.saveMetaPointer(ptr);
      await publishIndexPointer(account, ptr);
    } catch (e) {
      console.warn("[nexchat] conv-index push failed:", e);
    } finally {
      this.pushing = false;
    }
  }

  private async resolvePointer(account: string): Promise<ConvIndexPointer | null> {
    const meta = this.hasMeta ? await this.loadMetaPointer() : null;
    const relay = await fetchIndexPointer(account);
    if (!meta) return relay;
    if (!relay) return meta;
    return relay.updated_at >= meta.updated_at ? relay : meta;
  }

  private async loadMetaPointer(): Promise<ConvIndexPointer | null> {
    const store = this.store as LocalStore & {
      getMeta?: (key: string) => Promise<ConvIndexPointer | null>;
    };
    if (!store.getMeta) return null;
    return store.getMeta(META_CONV_ID);
  }

  private async saveMetaPointer(ptr: ConvIndexPointer): Promise<void> {
    const store = this.store as LocalStore & {
      setMeta?: (key: string, value: ConvIndexPointer) => Promise<void>;
    };
    await store.setMeta?.(META_CONV_ID, ptr);
  }

  private async writeLocalConvs(convs: import("@/merge/spec").LocalConv[]): Promise<void> {
    const store = this.store as LocalStore & {
      replaceLocalConvs?: (rows: import("@/merge/spec").LocalConv[]) => Promise<void>;
    };
    if (store.replaceLocalConvs) {
      await store.replaceLocalConvs(convs);
      return;
    }
    for (const c of convs) {
      const convId =
        c.kind === "direct" ? `d:${c.peer ?? ""}` : `g:${c.groupId ?? 0}`;
      await this.store.ensureConv(convId);
      if (c.title) await this.store.setConvTitle(convId, c.title);
      await this.store.setPref(convId, {
        pinnedPref: c.pinnedPref,
        dndPref: c.dndPref,
        archivedPref: c.archivedPref,
      });
    }
  }
}

let sync: ConvIndexSync | null = null;

export function convIndexSyncFor(store: LocalStore): ConvIndexSync {
  if (!sync) {
    const hasMeta = typeof (store as { setMeta?: unknown }).setMeta === "function";
    sync = new ConvIndexSync(store, hasMeta);
  }
  return sync;
}

export function scheduleConvIndexPush(account: string): void {
  if (sync) sync.schedulePush(account);
}

export async function restoreConvIndex(account: string, store: LocalStore): Promise<boolean> {
  const s = convIndexSyncFor(store);
  return s.restore(account);
}
