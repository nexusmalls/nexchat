// EN: Track A MLS state-escrow vault sync (design CHAT_MULTIDEVICE_MLS_SYNC §4). Producer: a full
// client (with signing key) periodically exports the signature-key-stripped MLS state, seals it
// under K_mls_escrow (`iv(12) || AES-256-GCM`), uploads to IPFS, and publishes the `{cid,updated_at}`
// pointer (relay + chain manifest). Cold-start consumer: a device with no local MLS state pulls the
// vault and installs a READ-ONLY client (decrypt + follow commits; sending gated until §5 handoff).
// Bidirectional vault merge (§4.4) and the handoff protocol (§5) are intentionally out of scope here.
// CN: 路线 A MLS 状态托管 vault 同步（设计 §4）。生产者：完整客户端（含签名钥）周期性导出剔除签名钥的
// MLS 状态，用 K_mls_escrow 封装（`iv(12) || AES-256-GCM`）、上传 IPFS、发布 `{cid,updated_at}` 指针
// （relay + 链上清单）。冷启动消费者：无本地 MLS 状态的设备拉取 vault 并装入**只读**客户端（可解密+
// 跟随 commit；发送在 §5 交接前被门控）。vault 双向合并（§4.4）与交接协议（§5）有意不在此范围内。

import { config } from "@/config";
import { b64ToBytes, bytesToB64 } from "@/delivery/b64";
import { ipfsClient } from "@/ipfs/ipfsClient";
import { openMlsEngine } from "@/mls/openMlsEngine";
import { loadMlsState } from "@/mls/mlsStore";
import {
  fetchMlsVaultPointer,
  publishMlsVaultPointer,
  readLocalMlsVaultPointer,
} from "@/relay/mlsVaultPointer";
import {
  decideVaultMerge,
  decodeVaultEnvelope,
  encodeVaultEnvelope,
  type VaultEnvelope,
} from "@/store/mlsVaultMerge";
import { deriveAnchorKeys, deriveMlsEscrowKey, type SyncPointer } from "@/store/syncAnchor";
import { getVaultMaster } from "@/wallet/vaultMaster";

function enabled(): boolean {
  return config.mlsVaultEnabled && config.ipfsEnabled;
}

// EN: Per-account local state for the §4.4 CAS loop. `prevCid` = the remote vault CID our current
// MLS state was last rebased on (imported or published); `deviceSeq` = a per-device monotone write
// counter (equal-epoch tiebreak). CN: §4.4 CAS 循环的按账户本地状态。`prevCid` = 本机 MLS 状态上次
// rebase 的远端 vault CID（导入或发布）；`deviceSeq` = 设备级单调写计数（同 epoch 平局裁决）。
const prevCidKey = (account: string) => `nexchat:mls-vault-prevcid:${account}`;
const deviceSeqKey = (account: string) => `nexchat:mls-vault-seq:${account}`;

function readPrevCid(account: string): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(prevCidKey(account));
}

function writePrevCid(account: string, cid: string | null): void {
  if (typeof localStorage === "undefined") return;
  if (cid) localStorage.setItem(prevCidKey(account), cid);
  else localStorage.removeItem(prevCidKey(account));
}

function nextDeviceSeq(account: string): number {
  if (typeof localStorage === "undefined") return 1;
  const next = (Number(localStorage.getItem(deviceSeqKey(account)) ?? "0") || 0) + 1;
  localStorage.setItem(deviceSeqKey(account), String(next));
  return next;
}

/// EN: Seal raw vault bytes: wire = `iv(12) || AES-256-GCM ciphertext` (no version byte — the blob
/// is self-describing via the MLS codec header). CN: 封装原始 vault 字节：wire = `iv(12) ||
/// AES-256-GCM 密文`（无版本字节——blob 由 MLS 编解码头自描述）。
async function sealVault(key: CryptoKey, blob: Uint8Array): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv: iv as BufferSource }, key, blob as BufferSource);
  const out = new Uint8Array(12 + ct.byteLength);
  out.set(iv);
  out.set(new Uint8Array(ct), 12);
  return out;
}

async function openVault(key: CryptoKey, packed: Uint8Array): Promise<Uint8Array> {
  const pt = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: packed.slice(0, 12) as BufferSource },
    key,
    packed.slice(12) as BufferSource,
  );
  return new Uint8Array(pt);
}

export class MlsVaultSync {
  private pushTimer: ReturnType<typeof setTimeout> | null = null;
  private pushing = false;
  private escrowKey: CryptoKey | null = null;

  /// EN: K_mls_escrow from `vault_master` + anchor_id (design §4.3); cached. Null when no master
  /// root (injector wallet / mock). CN: 由 `vault_master` + anchor_id 派生的 K_mls_escrow（设计
  /// §4.3）；带缓存。无 master 根（注入器钱包/mock）时为 null。
  private async key(): Promise<CryptoKey | null> {
    if (this.escrowKey) return this.escrowKey;
    const master = getVaultMaster();
    if (!master) return null;
    const { anchorId } = await deriveAnchorKeys(master);
    this.escrowKey = await deriveMlsEscrowKey(master, anchorId);
    return this.escrowKey;
  }

  /// EN: Reset the cached key (call on account rebind). CN: 清空缓存密钥（账户重绑时调用）。
  resetKey(): void {
    this.escrowKey = null;
  }

  schedulePush(account: string): void {
    if (!enabled()) return;
    if (this.pushTimer) clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => {
      this.pushTimer = null;
      void this.push(account);
    }, 2000);
  }

  /// EN: §4.4 read-merge-write: export local state → read the remote pointer → if a concurrent
  /// writer advanced it, fetch + decode the remote envelope and run `decideVaultMerge` → only seal +
  /// upload + publish when our per-group state should win (prev_cid CAS). No-op for a read-only /
  /// uninitialised client (only a full client is the escrow authority, §3.2). CN: §4.4 读-合并-写：
  /// 导出本机状态 → 读远端指针 → 若有并发写推进，取回并解码远端信封、跑 `decideVaultMerge` → 仅当本机
  /// 逐群状态应胜出时才封装+上传+发布（prev_cid CAS）。只读/未初始化客户端为空操作（仅完整客户端是托管
  /// 权威，§3.2）。
  async push(account: string): Promise<void> {
    if (!enabled() || this.pushing) return;
    if (!openMlsEngine.canExportEscrow()) return;
    this.pushing = true;
    try {
      const key = await this.key();
      if (!key) return;
      await openMlsEngine.flush();
      const blob = openMlsEngine.exportEscrowState();
      const localGroups = this.snapshotGroups();
      const prevCid = readPrevCid(account);

      // CAS read: has another device advanced the pointer since our base?
      const remotePtr = await fetchMlsVaultPointer(account);
      const remoteCid = remotePtr?.cid ?? null;
      let remote: VaultEnvelope | null = null;
      if (remoteCid && remoteCid !== prevCid) {
        try {
          remote = decodeVaultEnvelope(await openVault(key, await ipfsClient.cat(remoteCid)));
        } catch (e) {
          console.warn("[nexchat] mls-vault remote decode failed (treating as absent):", e);
        }
      }

      const decision = decideVaultMerge({ localGroups, remote, prevCid, remoteCid });
      if (decision.action === "skip") {
        // EN: when the remote is strictly newer, adopt its cid as our base so we don't re-loop;
        // normal commit processing will advance us, then a later push wins. CN: 远端严格更新时，把它
        // 的 cid 作为本机 base，避免空转；正常 commit 处理推进后，后续 push 再胜出。
        if (decision.reason === "remote-newer" && remoteCid) writePrevCid(account, remoteCid);
        return;
      }

      const basedOn = decision.action === "publish-rebased" ? decision.basedOnCid : prevCid;
      const env: VaultEnvelope = {
        v: 1,
        updated_at: Date.now(),
        deviceSeq: nextDeviceSeq(account),
        groups: localGroups,
        prevCid: basedOn,
        blob: bytesToB64(blob),
      };
      const packed = await sealVault(key, encodeVaultEnvelope(env));
      const cid = await ipfsClient.add(packed, "mls-vault.enc");
      const ptr: SyncPointer = { cid, updated_at: env.updated_at };
      await publishMlsVaultPointer(account, ptr);
      writePrevCid(account, cid);
    } catch (e) {
      console.warn("[nexchat] mls-vault push failed:", e);
    } finally {
      this.pushing = false;
    }
  }

  /// EN: Unlock-time cold start (design §4): when IndexedDB has no full MLS snapshot, pull the escrow
  /// vault and install a read-only client **before** `openMlsEngine.init` would mint a fresh signing
  /// key. Returns true when the engine is now read-only from vault. CN: 解锁时冷启动（设计 §4）：IndexedDB
  /// 无完整 MLS 快照时，在 `openMlsEngine.init` 生成新签名钥**之前**拉取托管 vault 并装入只读客户端。
  /// 成功则返回 true。
  async coldStartRestore(account: string): Promise<boolean> {
    if (!enabled()) return false;
    const localBlob = await loadMlsState(account);
    if (localBlob && localBlob.length > 0) return false;
    return this.importVault(account);
  }

  /// EN: Cold-start restore: install a read-only client from the escrow vault when the device has no
  /// local full MLS snapshot, and record its cid as our CAS base. Never clobbers a live full client
  /// (`canExportEscrow()`). Idempotent when already read-only from an earlier cold-start import.
  /// CN: 冷启动恢复：设备无本地完整 MLS 快照时，从托管 vault 装入只读客户端，并把其 cid 记为本机 CAS base。
  /// 绝不覆盖在线完整客户端（`canExportEscrow()`）。若已由冷启动导入为只读则幂等。
  async restore(account: string): Promise<boolean> {
    if (!enabled()) return false;
    if (openMlsEngine.canExportEscrow()) return false;
    if (openMlsEngine.isReadOnlyEscrow()) return true;
    if (openMlsEngine.groupCount() > 0) return false;
    return this.importVault(account);
  }

  private async importVault(account: string): Promise<boolean> {
    try {
      const ptr = await fetchMlsVaultPointer(account);
      if (!ptr) return false;
      const key = await this.key();
      if (!key) return false;
      const packed = await ipfsClient.cat(ptr.cid);
      const env = decodeVaultEnvelope(await openVault(key, packed));
      const blob = b64ToBytes(env.blob);
      if (openMlsEngine.canExportEscrow()) return false;
      if (!openMlsEngine.isReadOnlyEscrow()) {
        await openMlsEngine.initFromEscrowVault(account, account, blob);
      } else {
        openMlsEngine.importEscrowVault(blob);
      }
      writePrevCid(account, ptr.cid);
      return true;
    } catch (e) {
      console.warn("[nexchat] mls-vault restore failed:", e);
      return false;
    }
  }

  /// EN: Per-group snapshot epochs from the live engine (conversation key → epoch). CN: 由在线引擎取
  /// 逐群快照 epoch（会话键→epoch）。
  private snapshotGroups(): Record<string, number> {
    const out: Record<string, number> = {};
    for (const convKey of openMlsEngine.listGroups()) {
      try {
        out[convKey] = openMlsEngine.epochByConv(convKey);
      } catch {
        /* group without a resolvable epoch → skip */
      }
    }
    return out;
  }
}

let sync: MlsVaultSync | null = null;

export function mlsVaultSyncFor(): MlsVaultSync {
  if (!sync) sync = new MlsVaultSync();
  return sync;
}

export function scheduleMlsVaultPush(account: string): void {
  if (sync) sync.schedulePush(account);
}

export async function restoreMlsVault(account: string): Promise<boolean> {
  return mlsVaultSyncFor().restore(account);
}

export async function coldStartMlsVaultRestore(account: string): Promise<boolean> {
  return mlsVaultSyncFor().coldStartRestore(account);
}

export { readLocalMlsVaultPointer };
