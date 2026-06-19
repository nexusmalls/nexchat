// EN: OpenMlsEngine — real OpenMLS (RFC 9420) engine backed by the nexchat-mls WASM
// module. Implements MlsEngine (encrypt/decrypt over the P3 envelope) and adds the
// group-lifecycle surface (KeyPackage / create_group / add_members / process_welcome)
// the handshake needs. All cryptography stays inside the WASM client; the TS layer only
// encodes/decodes the P3 envelope and routes bytes by conversation id.
// CN: OpenMlsEngine —— 由 nexchat-mls WASM 模块支撑的真实 OpenMLS(RFC 9420) 引擎。实现
// MlsEngine（对 P3 信封加解密）并补齐握手所需的群生命周期接口（KeyPackage / 建群 / 加人 /
// 处理 welcome）。密码学全在 WASM 客户端内，TS 仅做 P3 信封编解码与按会话 id 路由字节。

import initWasm, { MlsClient } from "@/mls-pkg/nexchat_mls.js";
import wasmUrl from "@/mls-pkg/nexchat_mls_bg.wasm?url";
import { decodeEnvelope, encodeEnvelope, type EnvelopeV1 } from "@/mls/envelope";
import type { MlsEngine } from "@/mls/mlsEngine";
import { loadMlsState, saveMlsState } from "@/mls/mlsStore";

let wasmReady: Promise<void> | null = null;

/// EN: Initialise the WASM module once (idempotent). CN: 仅初始化一次 WASM 模块（幂等）。
function ensureWasm(): Promise<void> {
  if (!wasmReady) {
    wasmReady = initWasm({ module_or_path: wasmUrl }).then(() => undefined);
  }
  return wasmReady;
}

const convKey = (groupId: number): string => `g:${groupId}`;

/// EN: True when `e` is the Track A read-only escrow guard (expected on secondary cold devices;
/// should NOT surface as a global login error). CN: 判断是否为路线 A 只读托管守卫错误（副设备冷启动
/// 预期行为；不应作为全局登录错误弹出）。
export function isReadOnlyEscrowError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.includes("read-only escrow device");
}

export class OpenMlsEngine implements MlsEngine {
  private client: MlsClient | null = null;
  private keyPackages = 0;
  // EN: stable per-device persistence key (account address); null = in-memory only.
  // CN: 稳定的设备级持久化键（账户地址）；null 表示仅内存、不落盘。
  private persistKey: string | null = null;
  private persistTimer: ReturnType<typeof setTimeout> | null = null;

  /// EN: Bring up the WASM client. If `persistKey` is given and a snapshot exists,
  /// restore the prior OpenMLS state (cross-refresh / multi-device); otherwise create a
  /// fresh identity and persist it. CN: 启动 WASM 客户端。若给了 `persistKey` 且存在快照，
  /// 则恢复此前 OpenMLS 状态（跨刷新 / 多设备）；否则新建身份并落盘。
  async init(identity: string, persistKey?: string): Promise<void> {
    await ensureWasm();
    this.persistKey = persistKey ?? null;
    // EN: Cold-start vault import may install a read-only client before init — never overwrite
    // it with a fresh signing identity (would discard the restored group state). CN: 冷启动 vault
    // 导入可能在 init 前已装入只读客户端——绝不用新签名身份覆盖（否则会丢弃已恢复的群状态）。
    if (this.client?.isReadOnly()) {
      this.installFlushHooks();
      return;
    }
    if (this.persistKey) {
      const blob = await loadMlsState(this.persistKey);
      if (blob && blob.length > 0) {
        try {
          this.client = MlsClient.restore(blob);
          this.installFlushHooks();
          return;
        } catch {
          // Corrupt/incompatible snapshot → fall back to a fresh identity.
          this.client = null;
        }
      }
    }
    this.client = new MlsClient(identity);
    this.installFlushHooks();
    this.schedulePersist();
  }

  private get c(): MlsClient {
    if (!this.client) throw new Error("OpenMlsEngine not initialised");
    return this.client;
  }

  /// EN: IANA cipher suite for the on-chain `cipher_suite` arg. CN: 链上 `cipher_suite` 入参。
  cipherSuite(): number {
    return this.c.cipherSuite();
  }

  /// EN: Generate one KeyPackage (bytes for `publish_key_package`). CN: 生成一个 KeyPackage。
  generateKeyPackage(): Uint8Array {
    this.keyPackages += 1;
    const kp = this.c.generateKeyPackage();
    this.schedulePersist();
    return kp;
  }

  async ensureKeyPackages(min: number): Promise<number> {
    while (this.keyPackages < min) this.generateKeyPackage();
    return this.keyPackages;
  }

  /// EN: This device's stable MLS leaf signature public key — the key the E2EI device-leaf credential
  /// (§3.9) binds to. CN: 本设备稳定的 MLS leaf 签名公钥——E2EI 设备 leaf 凭证（§3.9）绑定的目标。
  signaturePublicKey(): Uint8Array {
    return this.c.signaturePublicKey();
  }

  /// EN: Install the E2EI device-leaf credential blob embedded into every subsequently generated
  /// KeyPackage's leaf node (§3.9). Empty clears it. CN: 安装 E2EI 设备 leaf 凭证 blob，置入此后生成的
  /// 每个 KeyPackage 的 leaf 节点（§3.9）。空清除。
  setLeafBinding(binding: Uint8Array): void {
    this.c.setLeafBinding(binding);
  }

  /// EN: Parse + validate a KeyPackage and return the fields needed to verify its E2EI device-leaf
  /// credential: leaf identity (`account#deviceFp`), leaf signature key, and the embedded binding
  /// (empty if none). CN: 解析并校验 KeyPackage，返回验证其 E2EI 设备 leaf 凭证所需字段：leaf identity
  /// （`account#deviceFp`）、leaf 签名钥、嵌入绑定（无则空）。
  keyPackageBinding(keyPackage: Uint8Array): {
    identity: string;
    signatureKey: Uint8Array;
    binding: Uint8Array;
  } {
    const b = this.c.keyPackageBinding(keyPackage);
    return { identity: b.identity, signatureKey: b.signature_key, binding: b.binding };
  }

  /// EN: Create a group locally; returns the on-chain fingerprint (tree/transcript/epoch).
  /// CN: 本地建群；返回链上指纹（tree/transcript/epoch）。
  createGroup(groupId: number) {
    return this.createGroupByConv(convKey(groupId));
  }

  /// EN: Create a group bound to an arbitrary conversation key (1:1 direct MLS).
  /// CN: 绑定任意会话键建群（1:1 私聊 MLS）。
  createGroupByConv(convKey: string) {
    const fp = this.c.createGroup(convKey);
    this.schedulePersist();
    return fp;
  }

  /// EN: Add members via their KeyPackages; returns commit + welcome + new fingerprint.
  /// CN: 用新成员 KeyPackage 加人；返回 commit + welcome + 新指纹。
  addMembers(groupId: number, keyPackages: Uint8Array[]) {
    return this.addMembersByConv(convKey(groupId), keyPackages);
  }

  /// EN: Add members on a string conversation key (direct or group). CN: 在字符串会话键上加人。
  addMembersByConv(convKey: string, keyPackages: Uint8Array[]) {
    const out = this.c.addMembers(convKey, keyPackages);
    this.schedulePersist();
    return out;
  }

  // ---- 1:1 Wire staged commits (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §4) ----
  // EN: Stage-then-decide so a lost `(conv, epoch)` race never force-merges a forked epoch.
  // CN: 先暂存再决定，使落败的 `(conv, epoch)` 竞争永不强制合并出分叉 epoch。

  /// EN: Add members WITHOUT merging — returns commit + welcome + pre-merge fingerprint; merge via
  /// `mergePending` on relay ACCEPT or discard via `clearPending` on EPOCH_STALE. CN: 加人但**不合并**
  /// ——返回 commit + welcome + 合并前指纹；relay ACCEPT 用 `mergePending` 合并，EPOCH_STALE 用
  /// `clearPending` 丢弃。
  addMembersStagedByConv(convKey: string, keyPackages: Uint8Array[]) {
    const out = this.c.addMembersStaged(convKey, keyPackages);
    this.schedulePersist();
    return out;
  }

  /// EN: Remove members WITHOUT merging; `memberIdentities` match exactly or by the `account#device`
  /// convention so a single device leaf can be targeted (1:1 Wire `remove_device`). CN: 移除成员但
  /// **不合并**；`memberIdentities` 按精确或 `account#device` 约定匹配，可定位单个设备 leaf（1:1 Wire
  /// `remove_device`）。
  removeMembersStagedByConv(convKey: string, memberIdentities: string[]) {
    const out = this.c.removeMembersStaged(convKey, memberIdentities);
    this.schedulePersist();
    return out;
  }

  /// EN: Self-update (rekey) own leaf WITHOUT merging; returns the staged commit bytes (no welcome).
  /// CN: 自更新（rekey）本设备 leaf 但**不合并**；返回 staged commit 字节（无 welcome）。
  selfUpdateStagedByConv(convKey: string): Uint8Array {
    const commit = this.c.selfUpdateStaged(convKey);
    this.schedulePersist();
    return commit;
  }

  /// EN: Merge the locally staged pending commit (relay ACCEPTed the slot). CN: 合并本地暂存的
  /// pending commit（relay 已 ACCEPT 该槽位）。
  mergePendingByConv(convKey: string): void {
    this.c.mergePending(convKey);
    this.schedulePersist();
  }

  /// EN: Discard the locally staged pending commit (EPOCH_STALE; adopt the winner next). CN: 丢弃
  /// 本地暂存的 pending commit（EPOCH_STALE；随后采纳胜出者）。
  clearPendingByConv(convKey: string): void {
    this.c.clearPending(convKey);
    this.schedulePersist();
  }

  /// EN: Remove members by SS58 identity; returns commit + optional welcome + fingerprint.
  /// CN: 按 SS58 identity 移除成员；返回 commit + 可选 welcome + 指纹。
  removeMembers(groupId: number, memberIdentities: string[]) {
    const out = this.c.removeMembers(convKey(groupId), memberIdentities);
    this.schedulePersist();
    return out;
  }

  /// EN: Swap members (remove + add in one commit). CN: 同一 commit 内替换成员。
  swapMembers(groupId: number, removeIdentities: string[], keyPackages: Uint8Array[]) {
    const out = this.c.swapMembers(convKey(groupId), removeIdentities, keyPackages);
    this.schedulePersist();
    return out;
  }

  async processWelcome(groupId: number, welcome: Uint8Array): Promise<void> {
    await this.processWelcomeByConv(convKey(groupId), welcome);
  }

  async processWelcomeByConv(convKey: string, welcome: Uint8Array): Promise<void> {
    this.c.processWelcome(convKey, welcome);
    this.schedulePersist();
  }

  /// EN: Apply a Commit to catch up an epoch. CN: 应用 Commit 补齐 epoch。
  processCommit(groupId: number, commit: Uint8Array): void {
    this.processCommitByConv(convKey(groupId), commit);
  }

  processCommitByConv(convKey: string, commit: Uint8Array): void {
    this.c.processCommit(convKey, commit);
    this.schedulePersist();
  }

  /// EN: Stage an incoming Commit WITHOUT merging and return the E2EI device-leaf bindings (§3.9) of
  /// every leaf it ADDS (empty = no Add). The caller verifies them, then `processCommitByConv` (same
  /// bytes → the staged commit is reused so the message is processed exactly once) or
  /// `discardIncomingCommit` on a bad binding. Enables member-side re-verification. CN: 把进入的 Commit
  /// 暂存（**不合并**），返回其**新增**每个 leaf 的 E2EI 设备 leaf 绑定（§3.9，空＝无 Add）。调用方验证后再
  /// `processCommitByConv`（同字节 → 复用暂存，使消息只处理一次）或在绑定非法时 `discardIncomingCommit`。
  /// 支持成员侧复验。
  inspectCommitBindings(
    convKey: string,
    commit: Uint8Array,
  ): Array<{ identity: string; signatureKey: Uint8Array; binding: Uint8Array }> {
    return this.c
      .inspectCommitBindings(convKey, commit)
      .map((b) => ({ identity: b.identity, signatureKey: b.signature_key, binding: b.binding }));
  }

  /// EN: Drop a Commit staged by `inspectCommitBindings` without merging. CN: 丢弃
  /// `inspectCommitBindings` 暂存的 Commit 而不合并。
  discardIncomingCommit(convKey: string): void {
    this.c.discardIncomingCommit(convKey);
  }

  forgetGroup(groupId: number): void {
    this.forgetGroupByConv(`g:${groupId}`);
  }

  forgetGroupByConv(convKey: string): void {
    this.c.forgetGroup(convKey);
    this.schedulePersist();
  }

  listGroups(): string[] {
    return this.c.listGroups();
  }

  /// EN: Leaf credential identities (`account#deviceId`) of every member in the bound group, in
  /// ratchet-tree order; empty when no group. Read-only roster for the 1:1 Wire device-disclosure UX
  /// (design §8): the caller groups by account to show per-side device counts and to pick a self device
  /// to remove. Every listed leaf was E2EI-verified at its add path (§3.9 induction). CN: 绑定群每个成员
  /// 的 leaf 凭证身份（`account#deviceId`），按棘轮树序返回，无群时为空。供 1:1 Wire 设备披露 UX（设计 §8）
  /// 只读用：调用方按账户分组以展示各方设备数、并据此挑选要移除的本端设备。每个 leaf 均在其 add 路径上经
  /// E2EI 校验（§3.9 归纳）。
  memberIdentities(convKey: string): string[] {
    return this.c.memberIdentities(convKey);
  }

  // ---- Track A escrow vault (design CHAT_MULTIDEVICE_MLS_SYNC §4/§7.1) ----

  /// EN: Number of live group handles — used to detect a cold-start device (0 = no local MLS
  /// state, safe to import an escrow vault). CN: 活跃群句柄数——用于识别冷启动设备（0 = 无本地
  /// MLS 状态，可安全导入托管 vault）。
  groupCount(): number {
    return this.client ? this.client.listGroups().length : 0;
  }

  /// EN: True when this engine holds a full client (with signing key) that can produce an escrow
  /// vault. A read-only client (restored from a vault) returns false — it must NOT re-publish.
  /// CN: 当本引擎持有可生成托管 vault 的完整客户端（含签名钥）时为真。只读客户端（由 vault 恢复）
  /// 返回 false——不得再次发布。
  canExportEscrow(): boolean {
    return !!this.client && !this.client.isReadOnly();
  }

  /// EN: Export the signature-key-stripped MLS state vault (design §3.2/§7.1) for off-chain escrow.
  /// Only valid on a full client; callers gate via `canExportEscrow`. CN: 导出剔除签名钥的 MLS 状态
  /// vault（设计 §3.2/§7.1）用于链下托管。仅对完整客户端有效；调用方用 `canExportEscrow` 把关。
  exportEscrowState(): Uint8Array {
    return this.c.exportEscrowState();
  }

  /// EN: True when this engine is a read-only escrow-restored client (no signing key). CN: 本引擎是否为
  /// 只读托管恢复客户端（无签名钥）。
  isReadOnlyEscrow(): boolean {
    return !!this.client?.isReadOnly();
  }

  /// EN: Cold-start unlock path: install a READ-ONLY client from a decrypted escrow vault **instead
  /// of** `new MlsClient(identity)`. Call ONLY when IndexedDB has no local full snapshot (design §4).
  /// CN: 冷启动解锁路径：由已解密的托管 vault 装入**只读**客户端，**替代** `new MlsClient(identity)`。
  /// 仅在 IndexedDB 无本地完整快照时调用（设计 §4）。
  async initFromEscrowVault(_identity: string, persistKey: string, blob: Uint8Array): Promise<void> {
    await ensureWasm();
    this.persistKey = persistKey;
    this.client = MlsClient.restoreEscrow(blob);
    this.installFlushHooks();
  }

  /// EN: Cold-start import of a decrypted escrow vault → installs a READ-ONLY client (no signer):
  /// it can decrypt + follow commits but not send until §5 handoff. Caller MUST ensure this is a
  /// cold start (no local full snapshot and not `canExportEscrow()`); we never clobber a live full
  /// client here (bidirectional merge is design §4.4, handled elsewhere). The read-only state is NOT
  /// locally persisted (the codec cannot re-export without a signer), so it is rebuilt from the vault
  /// on each unlock.
  /// CN: 冷启动导入已解密的托管 vault → 装入**只读**客户端（无签名钥）：可解密+跟随 commit，但在
  /// §5 交接前不能发送。调用方须确保冷启动（无本地完整快照且非 `canExportEscrow()`）；此处绝不覆盖
  /// 在线完整客户端（双向合并见设计 §4.4，另行处理）。只读状态不本地持久化（无签名钥无法再导出），故
  /// 每次解锁都从 vault 重建。
  importEscrowVault(blob: Uint8Array): void {
    this.client = MlsClient.restoreEscrow(blob);
    this.installFlushHooks();
  }

  /// EN: Online-handoff step 2 (design §5.2): export THIS device's signing-key bundle so the old
  /// primary can hand sending authority to a new device. Only valid on a full client (gate via
  /// `canExportEscrow`). ⚠️ The bundle grants impersonation — the caller MUST encrypt it to the
  /// target device peer key before it leaves this device. CN: 在线交接步骤 2（设计 §5.2）：导出**本设备**
  /// 的签名钥 bundle，使旧主设备把发送权交给新设备。仅对完整客户端有效（用 `canExportEscrow` 把关）。
  /// ⚠️ bundle 授予冒名能力——调用方在其离开本设备前**必须**用目标设备对端钥加密。
  exportSigningKeys(): Uint8Array {
    return this.c.exportSigningKeys();
  }

  /// EN: Online-handoff step 4 (design §5.2): install a decrypted signing-key bundle into this
  /// read-only escrow client, upgrading it to an active sender. Rejects a foreign bundle / a client
  /// that already has a signer (enforced in wasm). After success the flush gate reopens (the client
  /// can now `exportState`), so we persist immediately. Authorization is the §5 HandoffReceipt,
  /// verified by `handoffCoordinator` before calling this. CN: 在线交接步骤 4（设计 §5.2）：把已解密的
  /// 签名钥 bundle 装入本只读托管客户端，升级为活跃发送者。异身份 bundle / 已持签名钥者被拒（wasm 内
  /// 强制）。成功后 flush 门重开（客户端可 `exportState`），故立即持久化。授权为 §5 HandoffReceipt，
  /// 由 `handoffCoordinator` 在调用前验证。
  installSigningKeys(bundle: Uint8Array): void {
    this.c.installSigningKeys(bundle);
    void this.flush();
  }

  hasGroup(convId: string): boolean {
    return this.c.hasGroup(convId);
  }

  /// EN: Current MLS epoch for a string conversation key (0 = creator-only group).
  /// CN: 字符串会话键上的当前 MLS epoch（0 = 仅创建者的群）。
  epochByConv(convKey: string): number {
    return Number(this.c.epoch(convKey));
  }

  /// EN: Post-commit fingerprint of the currently STAGED (pending, not-yet-merged) commit — the TRUE
  /// `(treeHash, transcriptHash, epoch)` the chain `commit(new_tree_hash, new_transcript_hash)` must
  /// carry, read WITHOUT merging (group Wire G3b, CHAT_GROUP_WIREIFY_DESIGN §7.2). Throws when nothing
  /// is staged for `convKey`. Use after an executor stages an add/remove/rekey and before submitting
  /// the chain `expected_epoch` CAS. CN: 当前**已暂存**（pending、未合并）commit 的后置指纹——链上
  /// `commit(new_tree_hash, new_transcript_hash)` 须携带的**真实** `(treeHash, transcriptHash, epoch)`，
  /// **不合并**即可读取（群 Wire G3b，设计 §7.2）。`convKey` 无暂存时抛错。在 executor 暂存 add/remove/rekey
  /// 之后、提交链上 `expected_epoch` CAS 之前调用。
  stagedCommitFingerprintByConv(convKey: string): {
    treeHash: Uint8Array;
    transcriptHash: Uint8Array;
    epoch: number;
  } {
    const fp = this.c.stagedCommitFingerprint(convKey);
    return {
      treeHash: fp.tree_hash,
      transcriptHash: fp.transcript_hash,
      epoch: Number(fp.epoch),
    };
  }

  /// EN: Current local MLS epoch for the group (for chain catch-up). CN: 群当前本地 epoch。
  epochOf(groupId: number): number {
    return Number(this.c.epoch(convKey(groupId)));
  }

  async encrypt(convId: string, plaintext: EnvelopeV1): Promise<Uint8Array> {
    const ct = this.c.encrypt(convId, encodeEnvelope(plaintext));
    this.schedulePersist();
    return ct;
  }

  async decrypt(convId: string, ciphertext: Uint8Array): Promise<EnvelopeV1> {
    const env = decodeEnvelope(this.c.decrypt(convId, ciphertext));
    this.schedulePersist();
    return env;
  }

  // ---- persistence plumbing / 持久化管线 ----

  // EN: Debounce snapshots — many ops fire in bursts (ratchet steps). CN: 防抖快照——
  // 很多操作会成批发生（棘轮推进）。
  private schedulePersist(): void {
    if (!this.persistKey) return;
    if (this.persistTimer) clearTimeout(this.persistTimer);
    this.persistTimer = setTimeout(() => {
      this.persistTimer = null;
      void this.flush();
    }, 200);
  }

  /// EN: Write the current state out immediately. CN: 立即写出当前状态。
  async flush(): Promise<void> {
    if (!this.persistKey || !this.client) return;
    // EN: a read-only escrow client has no signer → `exportState` would throw and there is nothing
    // signer-bound to persist locally (it is rebuilt from the vault each unlock). CN: 只读托管客户端
    // 无签名钥 → `exportState` 会抛错，且无签名绑定状态需本地持久化（每次解锁从 vault 重建）。
    if (this.client.isReadOnly()) return;
    try {
      await saveMlsState(this.persistKey, this.client.exportState());
    } catch {
      /* best-effort */
    }
  }

  // EN: Flush on tab hide/unload so an in-flight ratchet step is never lost.
  // CN: 在标签页隐藏/卸载时落盘，避免丢失正在进行的棘轮步进。
  private flushHooksInstalled = false;
  private installFlushHooks(): void {
    if (this.flushHooksInstalled || typeof window === "undefined") return;
    this.flushHooksInstalled = true;
    const flushNow = () => {
      if (this.persistTimer) {
        clearTimeout(this.persistTimer);
        this.persistTimer = null;
      }
      void this.flush();
    };
    window.addEventListener("pagehide", flushNow);
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden") flushNow();
    });
  }
}

export const openMlsEngine = new OpenMlsEngine();
