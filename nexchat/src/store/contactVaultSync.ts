// EN: Orchestrates contacts-vault restore (IPFS + decrypt + merge) and push (encrypt + IPFS + pointer).
// CN: 编排通讯录 vault 恢复与推送（IPFS 拉取/加密/指针发布）。

import { config } from "@/config";
import { ipfsClient } from "@/ipfs/ipfsClient";
import {
  fetchContactsPointer,
  publishContactsPointer,
  readLocalContactsPointer,
} from "@/relay/contactsPointer";
import { loadContacts, saveContacts } from "@/store/contactBook";
import {
  buildVaultFromLocal,
  decryptVaultBlob,
  encryptVaultBlob,
  mergeVaultBlobs,
  tombstonesForRemoved,
  vaultToSavedContacts,
  type ContactVaultBlob,
  type ContactVaultPointer,
} from "@/store/contactVault";
import { deviceId } from "@/store/convIndex";

export class ContactVaultSync {
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private lastBlob: ContactVaultBlob | null = null;
  private pushing = false;

  /// EN: Pull remote vault (if any), merge into local contact book. CN: 拉取远端 vault 并合并到本地。
  async restore(account: string): Promise<boolean> {
    if (!config.contactsVaultEnabled || !config.ipfsEnabled) return false;
    try {
      const ptr = await this.resolvePointer(account);
      if (!ptr) return false;
      const packed = await ipfsClient.cat(ptr.cid);
      const remote = await decryptVaultBlob(packed);
      const local = loadContacts(account);
      const localBlob = buildVaultFromLocal(local, deviceId());
      const merged = mergeVaultBlobs(localBlob, remote);
      this.lastBlob = merged;
      saveContacts(account, vaultToSavedContacts(merged));
      return true;
    } catch (e) {
      console.warn("[nexchat] contacts-vault restore failed:", e);
      return false;
    }
  }

  schedulePush(account: string): void {
    if (!config.contactsVaultEnabled || !config.ipfsEnabled) return;
    if (this.pushTimer) clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => {
      this.pushTimer = null;
      void this.push(account);
    }, 1500);
  }

  async push(account: string): Promise<void> {
    if (!config.contactsVaultEnabled || !config.ipfsEnabled || this.pushing) return;
    this.pushing = true;
    try {
      const local = loadContacts(account);
      let blob = buildVaultFromLocal(local, deviceId());

      if (this.lastBlob) {
        const removed = tombstonesForRemoved(this.lastBlob, local);
        if (removed.length) {
          blob = mergeVaultBlobs(blob, {
            v: 1,
            updated_at: Date.now(),
            device_id: deviceId(),
            contacts: removed,
          });
        }
      }

      const remotePtr = await fetchContactsPointer(account);
      const localPtr = readLocalContactsPointer(account);

      const bestPtr =
        remotePtr && localPtr
          ? remotePtr.updated_at >= localPtr.updated_at
            ? remotePtr
            : localPtr
          : remotePtr ?? localPtr;

      if (bestPtr && (!this.lastBlob || bestPtr.updated_at > (this.lastBlob.updated_at ?? 0))) {
        try {
          const packed = await ipfsClient.cat(bestPtr.cid);
          const remote = await decryptVaultBlob(packed);
          blob = mergeVaultBlobs(remote, blob);
        } catch {
          /* stale cid */
        }
      }

      const packed = await encryptVaultBlob(blob);
      const cid = await ipfsClient.add(packed, "contacts-vault.enc");
      const ptr: ContactVaultPointer = { cid, updated_at: blob.updated_at };
      this.lastBlob = blob;
      await publishContactsPointer(account, ptr);
    } catch (e) {
      console.warn("[nexchat] contacts-vault push failed:", e);
    } finally {
      this.pushing = false;
    }
  }

  private async resolvePointer(account: string): Promise<ContactVaultPointer | null> {
    return fetchContactsPointer(account);
  }
}

let sync: ContactVaultSync | null = null;

export function contactVaultSyncFor(): ContactVaultSync {
  if (!sync) sync = new ContactVaultSync();
  return sync;
}

export function scheduleContactsVaultPush(account: string): void {
  contactVaultSyncFor().schedulePush(account);
}

export async function restoreContactsVault(account: string): Promise<boolean> {
  return contactVaultSyncFor().restore(account);
}
