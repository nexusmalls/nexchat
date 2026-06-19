// EN: Unified off-chain backup restore/push (contacts vault + conv index + msg archive).
// CN: 统一的链下备份恢复/推送（通讯录 vault + 会话索引 + 消息归档）。

import { config } from "@/config";
import { fetchContactsPointer } from "@/relay/contactsPointer";
import { fetchIndexPointer } from "@/relay/indexPointer";
import { fetchMsgArchivePointer } from "@/relay/msgArchivePointer";
import { restoreConvIndex, convIndexSyncFor } from "@/store/convIndexSync";
import {
  contactVaultSyncFor,
  restoreContactsVault,
} from "@/store/contactVaultSync";
import type { LocalStore } from "@/store/localStore";
import {
  msgArchiveSyncFor,
  restoreMsgArchive,
} from "@/store/msgArchiveSync";

export type OffchainSyncPhase =
  | "idle"
  | "restoring"
  | "pushing"
  | "ok"
  | "partial"
  | "no_backup"
  | "error";

export interface OffchainSyncStatus {
  phase: OffchainSyncPhase;
  contacts: boolean | null;
  convIndex: boolean | null;
  msgArchive: boolean | null;
  message?: string;
  /// EN: Optional note when chain anchor publish is paused (e.g. low balance). CN: 链锚暂停原因（如余额不足）。
  anchorNote?: string;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function remoteHasBackup(account: string): Promise<boolean> {
  if (!config.relayWs) return false;
  const [c, i, m] = await Promise.all([
    fetchContactsPointer(account),
    fetchIndexPointer(account),
    fetchMsgArchivePointer(account),
  ]);
  return !!(c?.cid || i?.cid || m?.cid);
}

async function restoreOnce(account: string, store: LocalStore) {
  const [contacts, convIndex, msgArchive] = await Promise.all([
    restoreContactsVault(account),
    restoreConvIndex(account, store),
    restoreMsgArchive(account, store),
  ]);
  return { contacts, convIndex, msgArchive };
}

/// EN: Pull encrypted blobs from IPFS via relay pointers (retries transient failures).
/// CN: 经 relay 指针从 IPFS 拉取加密 blob（ transient 失败会重试）。
export async function restoreOffchainData(
  account: string,
  store: LocalStore,
): Promise<OffchainSyncStatus> {
  if (!config.ipfsEnabled) {
    return {
      phase: "error",
      contacts: null,
      convIndex: null,
      msgArchive: null,
      message: "IPFS 未启用，无法云同步",
    };
  }

  const hasRemote = await remoteHasBackup(account);
  if (!hasRemote) {
    return {
      phase: "no_backup",
      contacts: false,
      convIndex: false,
      msgArchive: false,
      message: "云端暂无备份（新设备需先用同一钱包在线聊天一段时间）",
    };
  }

  let lastErr: unknown;
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      const result = await restoreOnce(account, store);
      const okCount = [result.contacts, result.convIndex, result.msgArchive].filter(Boolean).length;
      const phase = okCount === 3 ? "ok" : okCount > 0 ? "partial" : "error";
      return {
        phase,
        ...result,
        message:
          phase === "ok"
            ? "云端数据已恢复"
            : phase === "partial"
              ? "部分云端数据已恢复"
              : "云端备份拉取失败，请检查网络",
      };
    } catch (e) {
      lastErr = e;
      await sleep(800 * (attempt + 1));
    }
  }

  return {
    phase: "error",
    contacts: null,
    convIndex: null,
    msgArchive: null,
    message: lastErr instanceof Error ? lastErr.message : "云端恢复失败",
  };
}

/// EN: Push local state to IPFS + relay immediately (not debounced). CN: 立即推送本地状态到 IPFS + relay。
export async function pushOffchainData(account: string, store: LocalStore): Promise<void> {
  if (!config.ipfsEnabled) return;
  await Promise.all([
    convIndexSyncFor(store).push(account),
    contactVaultSyncFor().push(account),
    msgArchiveSyncFor(store).push(account),
  ]);
}

export function offchainSyncEnabled(): boolean {
  return (
    config.ipfsEnabled &&
    !!config.relayWs &&
    (config.contactsVaultEnabled || config.convIndexEnabled || config.msgArchiveEnabled)
  );
}
