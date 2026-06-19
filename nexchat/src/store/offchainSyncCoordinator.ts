// EN: OffchainSyncCoordinator — the unified cold-path pipeline of ADR CHAT_SYNC_ANCHOR
// §14.6: short-debounce relay pushes stay in the per-module sync classes; this layer adds
// the LONG-debounce chain anchor (hash gate + persistent retry queue, §6.1), the
// three-source per-field LWW restore (§6.2), the empty-relay write-back (§6.3) and the
// post-disaster orchestration flags (§6.5). Chain writes are fully async — they must
// never block sending messages.
// CN: OffchainSyncCoordinator——ADR CHAT_SYNC_ANCHOR §14.6 的统一冷路径管线：relay 短
// debounce 仍由各 sync 模块承担；本层补充链锚**长 debounce**（hash gate + 持久化重试
// 队列，§6.1）、三源逐字段 LWW 恢复（§6.2）、空 Relay 写回（§6.3）与灾后重建编排标志
// （§6.5）。链上写完全异步——绝不阻塞发消息。

import { chainClient } from "@/chain/chainClient";
import { config, signingPinBackupActive } from "@/config";
import {
  fetchContactsPointer,
  publishContactsPointer,
  readLocalContactsPointer,
  writeLocalContactsPointer,
} from "@/relay/contactsPointer";
import {
  fetchIndexPointer,
  publishIndexPointer,
  readLocalIndexPointer,
  writeLocalIndexPointer,
} from "@/relay/indexPointer";
import {
  fetchMsgArchivePointer,
  publishMsgArchivePointer,
  readLocalMsgArchivePointer,
  writeLocalMsgArchivePointer,
} from "@/relay/msgArchivePointer";
import {
  fetchMlsVaultPointer,
  publishMlsVaultPointer,
  readLocalMlsVaultPointer,
  writeLocalMlsVaultPointer,
} from "@/relay/mlsVaultPointer";
import {
  fetchMlsSigningPointer,
  publishMlsSigningPointer,
  readLocalMlsSigningPointer,
  writeLocalMlsSigningPointer,
} from "@/relay/mlsSigningPointer";
import type { LocalStore } from "@/store/localStore";
import {
  restoreOffchainData,
  type OffchainSyncStatus,
} from "@/store/offchainSync";
import {
  buildPublishPayload,
  canonicalJsonBytes,
  decryptManifest,
  deriveAnchorKeys,
  encryptManifest,
  isInsufficientBalanceError,
  signAnchorPayload,
  SYNC_ANCHOR_FEE_BUFFER_PLANCK,
  SYNC_ANCHOR_FIRST_DEPOSIT_PLANCK,
  type AnchorKeys,
  type SyncManifest,
  type SyncPointer,
} from "@/store/syncAnchor";
import { contactVaultSyncFor, scheduleContactsVaultPush } from "@/store/contactVaultSync";
import { convIndexSyncFor, scheduleConvIndexPush } from "@/store/convIndexSync";
import { msgArchiveSyncFor, scheduleMsgArchivePush } from "@/store/msgArchiveSync";
import { mlsVaultSyncFor, restoreMlsVault, scheduleMlsVaultPush } from "@/store/mlsVaultSync";
import { deriveSyncPayerPair, payerTopUpAmount } from "@/store/syncAnchorPayer";
import { appendSyncAudit, type SyncFieldAudit } from "@/store/syncAuditLog";
import { getVaultMaster } from "@/wallet/vaultMaster";

export type SyncField = "index" | "contacts" | "archive" | "mls" | "mls_signing";

const FIELDS: SyncField[] = ["index", "contacts", "archive", "mls", "mls_signing"];

function activeSyncFields(): SyncField[] {
  return signingPinBackupActive() ? FIELDS : FIELDS.filter((f) => f !== "mls_signing");
}

function pointerForField(
  ptrs: Partial<Record<SyncField, SyncPointer | null>>,
  field: SyncField,
): SyncPointer | null | undefined {
  return ptrs[field];
}

function manifestField(manifest: SyncManifest, field: SyncField): SyncPointer | undefined {
  return manifest[field];
}

/// EN: Retry queue entry — only the LATEST pending manifest is kept (§6.1: older ones
/// are superseded by LWW, no need to replay). CN: 重试队列条目——仅保留**最新**待发清单
/// （§6.1：旧的被 LWW 取代，无需重放）。
interface PendingPublish {
  manifest: SyncManifest;
  attempts: number;
  nextAttemptAt: number;
}

const RETRY_BASE_MS = 60_000;
const RETRY_MAX_MS = 3_600_000;

export function nextBackoffMs(attempts: number): number {
  return Math.min(RETRY_BASE_MS * 2 ** Math.max(0, attempts), RETRY_MAX_MS);
}

/// EN: Build a SyncManifest from the current per-slot pointers (top-level `updated_at`
/// = max of fields, cache hint only). Returns null when no slot has data. Pure.
/// CN: 由当前各槽位指针构建 SyncManifest（顶层 `updated_at` = 字段最大值，仅缓存提示）。
/// 所有槽位为空时返回 null。纯函数。
export function buildManifestFromPointers(ptrs: {
  index?: SyncPointer | null;
  contacts?: SyncPointer | null;
  archive?: SyncPointer | null;
  mls?: SyncPointer | null;
  mls_signing?: SyncPointer | null;
}): SyncManifest | null {
  const fields: Partial<Record<SyncField, SyncPointer>> = {};
  let maxTs = 0;
  for (const f of activeSyncFields()) {
    const p = pointerForField(ptrs, f);
    if (p?.cid && p.updated_at > 0) {
      fields[f] = { cid: p.cid, updated_at: p.updated_at };
      maxTs = Math.max(maxTs, p.updated_at);
    }
  }
  if (maxTs === 0) return null;
  return { v: 1, updated_at: maxTs, ...fields };
}

/// EN: §6.2 per-field LWW: fields where the chain manifest is STRICTLY newer than the
/// best local/relay pointer (these get injected into the local pointer store before the
/// regular restore runs). Pure. CN: §6.2 逐字段 LWW：链上清单严格新于本地/relay 最优指针
/// 的字段（在常规恢复前注入本地指针存储）。纯函数。
export function chainNewerFields(
  manifest: SyncManifest | null,
  current: Partial<Record<SyncField, SyncPointer | null>>,
): Array<[SyncField, SyncPointer]> {
  if (!manifest) return [];
  const out: Array<[SyncField, SyncPointer]> = [];
  for (const f of activeSyncFields()) {
    const chain = manifestField(manifest, f);
    if (!chain?.cid) continue;
    const cur = current[f];
    if (!cur || chain.updated_at > cur.updated_at) out.push([f, chain]);
  }
  return out;
}

/// EN: §6.3 write-back decision: fields whose effective pointer is missing from or newer
/// than the relay copy. Pure. CN: §6.3 写回判定：有效指针缺失于 relay 或新于 relay 的字段。
/// 纯函数。
export function relayWriteBackFields(
  effective: Partial<Record<SyncField, SyncPointer | null>>,
  relay: Partial<Record<SyncField, SyncPointer | null>>,
): Array<[SyncField, SyncPointer]> {
  const out: Array<[SyncField, SyncPointer]> = [];
  for (const f of activeSyncFields()) {
    const eff = effective[f];
    if (!eff?.cid) continue;
    const r = relay[f];
    if (!r || r.updated_at < eff.updated_at) out.push([f, eff]);
  }
  return out;
}

function toHex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

function fromHex(hex: string): Uint8Array {
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}

async function blake2Hex(bytes: Uint8Array): Promise<string> {
  const { blake2AsHex } = await import("@polkadot/util-crypto");
  return blake2AsHex(bytes, 256);
}

const lastKey = (account: string) => `nexchat:anchor-last:${account}`;
const pendingKey = (account: string) => `nexchat:anchor-pending:${account}`;
const tierKey = (account: string) => `nexchat:anchor-tier:${account}`;

/// EN: Per-account sync tier (ADR §7/§11.3): a user toggle persisted in localStorage,
/// defaulting to the build flag `VITE_SYNC_ANCHOR_DEFAULT`. "relay_only" leaves zero
/// anchor-activity metadata on-chain. CN: 按账户同步档位（ADR §7/§11.3）：用户开关持久化
/// 于 localStorage，缺省取构建 flag `VITE_SYNC_ANCHOR_DEFAULT`。"relay_only" 不在链上留
/// 任何锚活跃度元数据。
export type SyncAnchorTier = "standard" | "relay_only";

export function getSyncAnchorTier(account: string): SyncAnchorTier {
  const v = lsGet<string>(tierKey(account));
  return v === "standard" || v === "relay_only" ? v : config.syncAnchorTier;
}

export function setSyncAnchorTier(account: string, tier: SyncAnchorTier): void {
  lsSet(tierKey(account), tier);
}

interface LastPublished {
  hash: string; // blake2_256 of canonical manifest bytes
  at: number; // wall clock ms of last successful publish
}

function lsGet<T>(key: string): T | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function lsSet(key: string, value: unknown): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(key, JSON.stringify(value));
}

function lsDel(key: string): void {
  if (typeof localStorage === "undefined") return;
  localStorage.removeItem(key);
}

/// EN: Restore result with §6.5 orchestration flags layered on the plain sync status.
/// CN: 恢复结果——在原同步状态上叠加 §6.5 编排标志。
export interface CoordinatedRestoreResult extends OffchainSyncStatus {
  /// EN: chain anchor contributed at least one effective field. CN: 链锚至少贡献一个有效字段。
  usedChainAnchor: boolean;
  /// EN: relay was empty/stale → user should bump inbox epoch to close the spent replay
  /// window (§6.5 step ③). CN: relay 空/陈旧→应提示 bump inbox epoch 关闭重放窗口（§6.5 ③）。
  needsEpochBump: boolean;
  /// EN: Track A MLS escrow vault cold-start import ran this restore (a read-only group client was
  /// installed from the vault). The caller MUST then catch up each imported group's epoch from the
  /// chain (`syncAllGroupMls`) since the vault snapshot may lag the live chain epoch. CN: 本次恢复执行了
  /// 路线 A MLS 托管 vault 冷启动导入（已从 vault 装入只读群客户端）。调用方随后**必须**从链追平各导入群的
  /// epoch（`syncAllGroupMls`），因为 vault 快照可能落后链上当前 epoch。
  mlsRestored: boolean;
}

export class OffchainSyncCoordinator {
  private account = "";
  private store: LocalStore | null = null;
  private keysPromise: Promise<AnchorKeys | null> | null = null;
  private retryTimer: ReturnType<typeof setInterval> | null = null;
  private cadenceTimer: ReturnType<typeof setInterval> | null = null;
  private publishing = false;
  /// EN: in-flight restore run shared by concurrent callers (re-entrancy guard).
  /// CN: 并发调用共享的进行中恢复（重入守卫）。
  private restoring: Promise<CoordinatedRestoreResult> | null = null;

  /// EN: Bind the unlocked account (call at unlock; clears cached keys on re-bind) and
  /// start the long-debounce cadence loop — a cheap periodic flushChain whose hash gate
  /// + debounce decide whether an extrinsic actually goes out (§11.2/§14.6).
  /// CN: 绑定已解锁账户（解锁时调用；重新绑定清掉缓存锚密钥），并启动长 debounce 节拍
  /// 循环——周期性轻量 flushChain，由 hash gate + debounce 决定是否真正发 extrinsic
  /// （§11.2/§14.6）。
  bind(account: string, store: LocalStore): void {
    if (this.account !== account) {
      this.keysPromise = null;
      // EN: drop any in-flight restore from the previous account so the new account starts
      // a fresh run. CN: 丢弃上一账户进行中的恢复，新账户从头开始。
      this.restoring = null;
      // EN: drop the cached K_mls_escrow so the new account re-derives it from its own vault_master.
      // CN: 清掉缓存的 K_mls_escrow，新账户从自身 vault_master 重新派生。
      mlsVaultSyncFor().resetKey();
    }
    this.account = account;
    this.store = store;
    // EN: cadence runs whenever an account is bound — each tick re-checks chainEnabled,
    // so a user toggling the tier at runtime takes effect without rebinding.
    // CN: 只要绑定了账户节拍即运行——每个 tick 重新检查 chainEnabled，用户运行时切换
    // 档位无需重新绑定即生效。
    if (!this.cadenceTimer && !config.useMock) {
      this.cadenceTimer = setInterval(() => {
        void this.flushChain();
        void this.drainPending();
      }, RETRY_BASE_MS);
    }
  }

  unbind(): void {
    this.account = "";
    this.store = null;
    this.keysPromise = null;
    this.restoring = null;
    if (this.retryTimer) clearInterval(this.retryTimer);
    this.retryTimer = null;
    if (this.cadenceTimer) clearInterval(this.cadenceTimer);
    this.cadenceTimer = null;
  }

  private chainEnabled(): boolean {
    return (
      !config.useMock &&
      config.ipfsEnabled &&
      !!this.account &&
      getSyncAnchorTier(this.account) === "standard"
    );
  }

  /// EN: Anchor keys from vault_master; null when no master root (injector wallet /
  /// mock) — then the chain tier is silently disabled (§5.0 hard precondition).
  /// CN: 由 vault_master 派生锚密钥；无 master 根（注入器钱包/mock）时返回 null——此时
  /// 链档位静默停用（§5.0 硬前置）。
  private anchorKeys(): Promise<AnchorKeys | null> {
    if (!this.keysPromise) {
      this.keysPromise = (async () => {
        const master = getVaultMaster();
        if (!master) return null;
        return deriveAnchorKeys(master);
      })();
    }
    return this.keysPromise;
  }

  /// EN: §14.6 markDirty — modules call this instead of their own schedulers; relay
  /// short-debounce stays per-module, chain long-debounce is handled by flushChain.
  /// CN: §14.6 markDirty——模块统一调用；relay 短 debounce 仍在各模块内，链长 debounce
  /// 由 flushChain 承担。
  markDirty(field: SyncField): void {
    if (!this.account) return;
    if (field === "index") scheduleConvIndexPush(this.account);
    else if (field === "contacts") scheduleContactsVaultPush(this.account);
    else if (field === "archive") scheduleMsgArchivePush(this.account);
    else if (field === "mls") scheduleMlsVaultPush(this.account);
    else void this.flushChain();
  }

  /// EN: Immediate relay flush of all slots (blob upload + `*_put`). CN: 立即推送全部
  /// 槽位到 relay（blob 上传 + `*_put`）。
  async flushRelay(): Promise<void> {
    if (!this.account || !this.store || !config.ipfsEnabled) return;
    await Promise.all([
      convIndexSyncFor(this.store).push(this.account),
      contactVaultSyncFor().push(this.account),
      msgArchiveSyncFor(this.store).push(this.account),
    ]);
  }

  /// EN: Long-window chain flush (§6.1 step 4): canonical manifest → hash gate →
  /// debounce → publish_sync_anchor; failures land in the persistent retry queue.
  /// `force` (sign-out / manual backup) skips the debounce but NOT the hash gate.
  /// CN: 长窗口链上 flush（§6.1 第 4 步）：canonical 清单 → hash gate → debounce →
  /// publish_sync_anchor；失败进持久化重试队列。`force`（登出/手动备份）跳过 debounce
  /// 但不跳过 hash gate。
  async flushChain(opts?: { force?: boolean }): Promise<void> {
    if (!this.chainEnabled() || this.publishing) return;
    const manifest = buildManifestFromPointers({
      index: readLocalIndexPointer(this.account),
      contacts: readLocalContactsPointer(this.account),
      archive: readLocalMsgArchivePointer(this.account),
      mls: readLocalMlsVaultPointer(this.account),
      mls_signing: readLocalMlsSigningPointer(this.account),
    });
    if (!manifest) return;

    const bytes = canonicalJsonBytes(manifest);
    const hash = await blake2Hex(bytes);
    const last = lsGet<LastPublished>(lastKey(this.account));
    if (last?.hash === hash) return; // unchanged → no extrinsic (§5.1)
    // EN: within the long debounce → skip; the cadence loop re-runs flushChain with the
    // then-current pointers once the window expires. CN: 仍在长 debounce 窗口内→跳过；
    // 窗口过期后节拍循环会用届时的最新指针重跑 flushChain。
    if (!opts?.force && last && Date.now() - last.at < config.syncAnchorDebounceMs) return;
    if (!(await this.canAffordAnchorPublish())) return;
    await this.publish(manifest, hash);
  }

  /// EN: Skip chain publish when the signer cannot cover deposit (first publish) + fees.
  /// CN: 签名者无力承担押金（首发布）+ 手续费时跳过链上发布。
  private async canAffordAnchorPublish(): Promise<boolean> {
    const keys = await this.anchorKeys();
    if (!keys) return false;
    const who = chainClient.signerAddress;
    if (!who) return false;
    try {
      const free = await chainClient.freeBalance(who);
      const row = await chainClient.syncAnchorOf(toHex(keys.anchorId));
      const min = row
        ? SYNC_ANCHOR_FEE_BUFFER_PLANCK
        : SYNC_ANCHOR_FIRST_DEPOSIT_PLANCK + SYNC_ANCHOR_FEE_BUFFER_PLANCK;
      return free >= min;
    } catch {
      return true;
    }
  }

  private enqueue(manifest: SyncManifest, attempts: number, nextAttemptAt: number): void {
    lsSet(pendingKey(this.account), { manifest, attempts, nextAttemptAt } satisfies PendingPublish);
    this.ensureRetryLoop();
  }

  private async publish(manifest: SyncManifest, precomputedHash?: string): Promise<void> {
    const keys = await this.anchorKeys();
    if (!keys) return;
    this.publishing = true;
    try {
      const bytes = canonicalJsonBytes(manifest);
      const hash = precomputedHash ?? (await blake2Hex(bytes));
      const ciphertext = await encryptManifest(keys, manifest);
      const genesis = await chainClient.genesisHashBytes();
      const payload = await buildPublishPayload(
        genesis,
        keys.anchorId,
        manifest.updated_at,
        ciphertext,
      );
      const sig = await signAnchorPayload(keys, payload);
      const payer = await this.resolvePayer();
      await chainClient.publishSyncAnchor(
        toHex(keys.anchorPk),
        manifest.updated_at,
        toHex(ciphertext),
        toHex(sig),
        payer,
      );
      lsSet(lastKey(this.account), { hash, at: Date.now() } satisfies LastPublished);
      lsDel(pendingKey(this.account));
    } catch (e) {
      if (isInsufficientBalanceError(e)) {
        console.warn(
          "[nexchat] publish_sync_anchor paused — insufficient balance (first publish needs ≥0.5 NEX deposit + fees)",
        );
        const prev = lsGet<PendingPublish>(pendingKey(this.account));
        const attempts = (prev?.attempts ?? 0) + 1;
        this.enqueue(manifest, attempts, Date.now() + Math.max(nextBackoffMs(attempts), 3_600_000));
        return;
      }
      console.warn("[nexchat] publish_sync_anchor failed (queued for retry):", e);
      const prev = lsGet<PendingPublish>(pendingKey(this.account));
      const attempts = (prev?.attempts ?? 0) + 1;
      this.enqueue(manifest, attempts, Date.now() + nextBackoffMs(attempts));
    } finally {
      this.publishing = false;
    }
  }

  /// EN: v2 payer unlinking (ADR §11.1, P3): in "burner" mode the extrinsic is signed by
  /// the dedicated vault_master-derived payer, topped up from the main signer when low
  /// (the one-time funding link is the disclosed residual, §5.7). "main" mode returns
  /// undefined → active signer pays. CN: v2 付费方断链（ADR §11.1，P3）："burner" 模式由
  /// vault_master 派生的专用 payer 签名，余额不足时从主签名者充值（一次性充值关联为已
  /// 披露的残余，§5.7）。"main" 模式返回 undefined→当前签名者付费。
  private async resolvePayer(): Promise<
    import("@polkadot/keyring/types").KeyringPair | undefined
  > {
    if (config.syncAnchorPayer !== "burner") return undefined;
    const master = getVaultMaster();
    if (!master) return undefined;
    const payer = await deriveSyncPayerPair(master);
    try {
      const free = await chainClient.freeBalance(payer.address);
      const topUp = payerTopUpAmount(free);
      if (topUp > 0n) {
        await chainClient.signAndSend("balances", "transferKeepAlive", [payer.address, topUp]);
      }
    } catch (e) {
      console.warn("[nexchat] sync-payer top-up failed (publishing with main signer):", e);
      return undefined;
    }
    return payer;
  }

  /// EN: Background retry loop for the persistent queue (1min cadence; entry-level
  /// exponential backoff up to 1h, §6.1). CN: 持久化队列的后台重试循环（1 分钟节拍；
  /// 条目级指数退避，上限 1h，§6.1）。
  private ensureRetryLoop(): void {
    if (this.retryTimer) return;
    this.retryTimer = setInterval(() => void this.drainPending(), RETRY_BASE_MS);
  }

  async drainPending(): Promise<void> {
    if (!this.chainEnabled() || this.publishing) return;
    const pending = lsGet<PendingPublish>(pendingKey(this.account));
    if (!pending) {
      if (this.retryTimer) clearInterval(this.retryTimer);
      this.retryTimer = null;
      return;
    }
    if (Date.now() < pending.nextAttemptAt) return;
    // EN: always retry with the CURRENT pointers — the queued manifest may be stale.
    // CN: 重试永远用**当前**指针——排队中的清单可能已过期。
    const fresh = buildManifestFromPointers({
      index: readLocalIndexPointer(this.account),
      contacts: readLocalContactsPointer(this.account),
      archive: readLocalMsgArchivePointer(this.account),
      mls: readLocalMlsVaultPointer(this.account),
      mls_signing: readLocalMlsSigningPointer(this.account),
    });
    await this.publish(fresh ?? pending.manifest);
  }

  /// EN: §6.2 + §6.3 + §6.5 self-healing entry: read the chain anchor, inject strictly-newer
  /// chain fields into the local pointer store, run the regular three-slot restore, then write
  /// missing/stale pointers back to the relay. Re-entrancy guard: concurrent callers (e.g. a
  /// second unlock or a retry firing mid-run) coalesce onto the single in-flight run rather
  /// than racing on the local pointer store / relay write-back.
  /// CN: §6.2+§6.3+§6.5 自愈入口：读链锚，把严格更新的链上字段注入本地指针存储，跑常规
  /// 三槽位恢复，最后把缺失/陈旧指针写回 relay。重入守卫：并发调用（如二次解锁或运行中
  /// 触发的重试）合并到同一次进行中的恢复，避免在本地指针存储 / relay 写回上竞争。
  restore(): Promise<CoordinatedRestoreResult> {
    if (this.restoring) return this.restoring;
    this.restoring = (async () => {
      try {
        return await this.restoreInner();
      } finally {
        this.restoring = null;
      }
    })();
    return this.restoring;
  }

  private async restoreInner(): Promise<CoordinatedRestoreResult> {
    if (!this.account || !this.store) {
      return {
        phase: "error",
        contacts: null,
        convIndex: null,
        msgArchive: null,
        message: "coordinator not bound",
        mlsRestored: false,
        usedChainAnchor: false,
        needsEpochBump: false,
      };
    }

    const startedAt = Date.now();
    // EN: snapshot local pointers BEFORE injection so the audit can classify each field's
    // winning source (chain injection overwrites these). CN: 注入前快照本地指针，便于审计
    // 判定每字段胜出来源（链上注入会覆盖它们）。
    const localBefore: Record<SyncField, SyncPointer | null> = {
      index: readLocalIndexPointer(this.account),
      contacts: readLocalContactsPointer(this.account),
      archive: readLocalMsgArchivePointer(this.account),
      mls: readLocalMlsVaultPointer(this.account),
      mls_signing: readLocalMlsSigningPointer(this.account),
    };

    let chainManifest: SyncManifest | null = null;
    if (this.chainEnabled()) {
      try {
        chainManifest = await this.fetchChainManifest();
      } catch (e) {
        console.warn("[nexchat] sync-anchor chain read failed (continuing without):", e);
      }
    }

    // EN: three sources per field: localStorage pointer + relay pointer (fetch* already
    // merges those two) + chain manifest. CN: 每字段三源：本地指针 + relay 指针
    // （fetch* 已合并这两者）+ 链上清单。
    const [relayIndex, relayContacts, relayArchive, relayMls, relayMlsSigning] = await Promise.all([
      fetchIndexPointer(this.account).catch(() => null),
      fetchContactsPointer(this.account).catch(() => null),
      fetchMsgArchivePointer(this.account).catch(() => null),
      fetchMlsVaultPointer(this.account).catch(() => null),
      fetchMlsSigningPointer(this.account).catch(() => null),
    ]);
    const current = {
      index: relayIndex,
      contacts: relayContacts,
      archive: relayArchive,
      mls: relayMls,
      mls_signing: relayMlsSigning,
    };

    const injected = chainNewerFields(chainManifest, current);
    const injectedSet = new Set(injected.map(([f]) => f));
    for (const [field, ptr] of injected) {
      if (field === "index") writeLocalIndexPointer(this.account, ptr);
      else if (field === "contacts") writeLocalContactsPointer(this.account, ptr);
      else if (field === "archive") writeLocalMsgArchivePointer(this.account, ptr);
      else if (field === "mls") writeLocalMlsVaultPointer(this.account, ptr);
      else writeLocalMlsSigningPointer(this.account, ptr);
    }

    const status = await restoreOffchainData(this.account, this.store);

    // EN: Track A MLS escrow vault (design §4): cold-start import of the read-only client from the
    // (possibly chain-injected) pointer. No-op unless `mlsVaultEnabled` and the device has zero
    // local groups. CN: 路线 A MLS 托管 vault（设计 §4）：从（可能链上注入的）指针冷启动导入只读客户端。
    // 仅在 `mlsVaultEnabled` 且设备无本地群时生效，否则空操作。
    const mlsRestored = await restoreMlsVault(this.account).catch(() => false);

    // EN: §6.3 — always attempt the relay write-back after a successful/partial restore
    // (covers the no_backup-on-relay + anchor-on-chain case). CN: §6.3——恢复 ok/partial
    // 后**始终**尝试写回 relay（覆盖 relay 无备份但链上有锚的情况）。
    const writeBackResult = new Map<SyncField, "skip" | "ok" | "fail">(
      activeSyncFields().map((f) => [f, "skip"]),
    );
    if (status.phase === "ok" || status.phase === "partial") {
      const effective = {
        index: readLocalIndexPointer(this.account),
        contacts: readLocalContactsPointer(this.account),
        archive: readLocalMsgArchivePointer(this.account),
        mls: readLocalMlsVaultPointer(this.account),
        mls_signing: readLocalMlsSigningPointer(this.account),
      };
      // EN: §6.3 anti-dangling — only advertise a field to the relay if its slot restore
      // actually succeeded, i.e. the blob behind the (possibly chain-injected) CID was
      // fetched + decrypted this run. Without this gate an unverified chain CID whose blob
      // is unavailable would be propagated to the relay as if it were good.
      // CN: §6.3 防悬空——仅当该槽恢复确实成功（即本次已取回并解密该 CID 背后的 blob）才
      // 向 relay 广播该字段。否则一个 blob 不可达的未校验链上 CID 会被当作有效数据写回 relay。
      const verified: Record<SyncField, boolean> = {
        index: status.convIndex === true,
        contacts: status.contacts === true,
        archive: status.msgArchive === true,
        // EN: only propagate the MLS-vault pointer when this run actually consumed it (cold-start
        // import succeeded); a device that kept its own live state must not push a stale pointer.
        // CN: 仅当本次确实消费了 MLS vault 指针（冷启动导入成功）才传播；保留自身在线状态的设备
        // 不得推送陈旧指针。
        mls: mlsRestored === true,
        // EN: signing backup is user-initiated; never write back on restore without verifying the blob.
        // CN: 签名备份由用户主动创建；恢复时未校验 blob 前不写回。
        mls_signing: false,
      };
      const writeBack = relayWriteBackFields(effective, current);
      for (const [field, ptr] of writeBack) {
        if (!verified[field]) continue;
        try {
          if (field === "index") await publishIndexPointer(this.account, ptr);
          else if (field === "contacts") await publishContactsPointer(this.account, ptr);
          else if (field === "archive") await publishMsgArchivePointer(this.account, ptr);
          else if (field === "mls") await publishMlsVaultPointer(this.account, ptr);
          else await publishMlsSigningPointer(this.account, ptr);
          writeBackResult.set(field, "ok");
        } catch (e) {
          console.warn(`[nexchat] relay write-back failed for ${field}:`, e);
          writeBackResult.set(field, "fail");
        }
      }
    }

    const usedChainAnchor = injected.length > 0;

    // EN: §6.2/§6.3/§6.5 audit trail — one structured record per restore (best-effort).
    // CN: §6.2/§6.3/§6.5 审计轨迹——每次恢复一条结构化记录（尽力而为）。
    this.recordAudit({
      startedAt,
      localBefore,
      relayCurrent: current,
      injectedSet,
      writeBackResult,
      status,
      usedChainAnchor,
    });

    return {
      ...status,
      mlsRestored,
      usedChainAnchor,
      // EN: chain anchor winning a field means the relay lost (or will return with) stale
      // data → the spent replay window is open until the user bumps the inbox epoch
      // (§6.5 ③; §8.3). Deliberately NOT gated on the write-back succeeding: an
      // unreachable relay is exactly the case where the warning matters most.
      // CN: 链锚赢得字段说明 relay 丢过（或将以陈旧状态回归）数据→在用户 bump inbox
      // epoch 前重放窗口开启（§6.5 ③；§8.3）。刻意**不**以写回成功为前提：relay 不可达
      // 恰是最需要该提示的场景。
      needsEpochBump: usedChainAnchor,
      message: usedChainAnchor
        ? `${status.message ?? ""}（已从链上加密锚恢复；建议重置投递信箱纪元以关闭重放窗口）`.trim()
        : status.message,
    };
  }

  /// EN: Build + persist one self-healing audit record. Best-effort; never throws.
  /// CN: 构建并落库一条自愈审计记录。尽力而为，绝不抛错。
  private recordAudit(ctx: {
    startedAt: number;
    localBefore: Record<SyncField, SyncPointer | null>;
    relayCurrent: Record<SyncField, SyncPointer | null>;
    injectedSet: Set<SyncField>;
    writeBackResult: Map<SyncField, "skip" | "ok" | "fail">;
    status: OffchainSyncStatus;
    usedChainAnchor: boolean;
  }): void {
    try {
      const effectiveOf = (f: SyncField): SyncPointer | null => {
        if (f === "index") return readLocalIndexPointer(this.account);
        if (f === "contacts") return readLocalContactsPointer(this.account);
        if (f === "archive") return readLocalMsgArchivePointer(this.account);
        if (f === "mls") return readLocalMlsVaultPointer(this.account);
        return readLocalMlsSigningPointer(this.account);
      };

      const fields: SyncFieldAudit[] = activeSyncFields().map((f) => {
        const localTs = ctx.localBefore[f]?.updated_at ?? 0;
        const relayTs = ctx.relayCurrent[f]?.updated_at ?? 0;
        const chainInjected = ctx.injectedSet.has(f);
        const source: SyncFieldAudit["source"] = chainInjected
          ? "chain"
          : relayTs > localTs
            ? "relay"
            : localTs > 0
              ? "local"
              : relayTs > 0
                ? "relay"
                : "none";
        return {
          field: f,
          source,
          chainInjected,
          writeBack: ctx.writeBackResult.get(f) ?? "skip",
          effectiveUpdatedAt: effectiveOf(f)?.updated_at ?? 0,
        };
      });

      appendSyncAudit({
        at: ctx.startedAt,
        account: this.account,
        tier: getSyncAnchorTier(this.account),
        phase: ctx.status.phase,
        usedChainAnchor: ctx.usedChainAnchor,
        needsEpochBump: ctx.usedChainAnchor,
        durationMs: Date.now() - ctx.startedAt,
        restored: {
          contacts: ctx.status.contacts,
          convIndex: ctx.status.convIndex,
          msgArchive: ctx.status.msgArchive,
        },
        fields,
        message: ctx.status.message,
      });
    } catch (e) {
      console.warn("[nexchat] sync-audit record failed:", e);
    }
  }

  /// EN: Fetch + decrypt this account's chain anchor manifest (null when unpublished or
  /// no vault_master). CN: 拉取并解密本账户链锚清单（未发布或无 vault_master 时为 null）。
  async fetchChainManifest(): Promise<SyncManifest | null> {
    const keys = await this.anchorKeys();
    if (!keys) return null;
    const row = await chainClient.syncAnchorOf(toHex(keys.anchorId));
    if (!row) return null;
    return decryptManifest(keys, fromHex(row.ciphertext));
  }
}

export const offchainSyncCoordinator = new OffchainSyncCoordinator();
