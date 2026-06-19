/* tslint:disable */
/* eslint-disable */

/**
 * EN: Result of a membership-changing commit, surfaced to JS for the on-chain
 * `commit` extrinsic. `welcome` is the single MLS Welcome covering all addees;
 * the caller duplicates it per addee to satisfy the chain's welcome/delta bijection.
 * CN: 成员变更 commit 的产物，供 JS 提交链上 `commit`。`welcome` 是覆盖所有新成员的单条
 * MLS Welcome；调用方按新成员复制以满足链上 welcome/delta 双射。
 */
export class CommitOut {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    commit: Uint8Array;
    epoch: bigint;
    transcript_hash: Uint8Array;
    tree_hash: Uint8Array;
    welcome: Uint8Array;
}

/**
 * EN: Group fingerprint after a local state change (for create_group args).
 * CN: 本地状态变更后的群指纹（用于 create_group 入参）。
 */
export class GroupFingerprint {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    epoch: bigint;
    transcript_hash: Uint8Array;
    tree_hash: Uint8Array;
}

/**
 * EN: The fields a verifier needs to check a KeyPackage's E2EI device-leaf credential (§3.9): the
 * leaf `identity` (`account#deviceFp`), the leaf `signature_key` the binding must commit to, and the
 * `binding` blob (account-SS58-key signature) extracted from the leaf-node extension. `binding` is
 * empty when the KeyPackage carries none (legacy / unbound). Verification (SS58 signatureVerify) is
 * done in TS. CN: 校验 KeyPackage 的 E2EI 设备 leaf 凭证（§3.9）所需字段：leaf `identity`
 * （`account#deviceFp`）、绑定须承诺的 leaf `signature_key`，以及从 leaf-node 扩展提取的 `binding` blob
 * （账户 SS58 钥签名）。KeyPackage 无绑定（旧版/未绑定）时 `binding` 为空。验证（SS58 signatureVerify）在 TS 完成。
 */
export class KpBinding {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    binding: Uint8Array;
    identity: string;
    signature_key: Uint8Array;
}

export class MlsClient {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * EN: Add members (>=2 for the first commit) via their published KeyPackages.
     * Returns commit + welcome + new fingerprint. CN: 通过新成员已发布的 KeyPackage 加人
     * （首个 commit 须 ≥2 人），返回 commit + welcome + 新指纹。
     */
    addMembers(conv_key: string, key_packages: Uint8Array[]): CommitOut;
    /**
     * EN: Like `addMembers`, but DO NOT merge — leaves the commit *staged* (pending) locally so a
     * 1:1 Wire coordinator can decide based on the relay's `(conv, epoch)` CAS verdict: `mergePending`
     * on ACCEPT, or `clearPending` (then adopt the winning commit) on EPOCH_STALE. Without staging,
     * a lost race would have already force-merged a forked epoch with no way back (1:1 has no
     * on-chain commit log). The returned `epoch`/fingerprint reflect the PRE-merge state.
     * CN: 同 `addMembers`，但**不合并**——把 commit 本地保留为 *staged*（pending），使 1:1 Wire 协调设备
     * 能依 relay 的 `(conv, epoch)` CAS 裁决决定：ACCEPT 则 `mergePending`，EPOCH_STALE 则 `clearPending`
     * （再采纳胜出 commit）。若不暂存，落败时已强制合并出分叉 epoch 且无法回退（1:1 无链上 commit 日志）。
     * 返回的 `epoch`/指纹为**合并前**状态。
     */
    addMembersStaged(conv_key: string, key_packages: Uint8Array[]): CommitOut;
    /**
     * EN: IANA cipher-suite code stored on-chain. CN: 链上存储的 IANA 套件编号。
     */
    cipherSuite(): number;
    /**
     * EN: Discard the locally staged pending commit (call on EPOCH_STALE before adopting the
     * winning commit via `processCommit`). No-op if nothing is staged. CN: 丢弃本地暂存的 pending
     * commit（EPOCH_STALE 时、经 `processCommit` 采纳胜出 commit **前**调用）。无暂存时为空操作。
     */
    clearPending(conv_key: string): void;
    /**
     * EN: Create a new group locally (epoch 0, creator only). When `setLeafBinding` is active the
     * creator leaf carries the same E2EI extension as generated KeyPackages (§3.9). `conv_key` is
     * the frontend conversation id used as the local handle. CN: 本地建群（epoch 0，仅创建者）。若已
     * `setLeafBinding`，创建者 leaf 携带与 KeyPackage 相同的 E2EI 扩展（§3.9）。`conv_key` 为前端会话 id。
     */
    createGroup(conv_key: string): GroupFingerprint;
    /**
     * EN: Decrypt an inbound application message; returns the plaintext payload bytes.
     * CN: 解密一条入站应用消息，返回明文载荷字节。
     */
    decrypt(conv_key: string, ciphertext: Uint8Array): Uint8Array;
    /**
     * EN: Drop a Commit staged by `inspectCommitBindings` without merging (call when an added leaf's
     * E2EI binding fails to verify). Clears the cached `(commit, staged)` entry only — the live group
     * epoch is unchanged because the staged commit was never merged. No-op if nothing is staged.
     * NOTE: after `inspectCommitBindings` has run, OpenMLS has already processed the commit once;
     * **do not** call `inspectCommitBindings` again with the same bytes after discard — use
     * `processCommit` if verification passed, or wait for a fresh delivery. Re-inspecting a
     * *different* commit is allowed once the slot is clear.
     * CN: 丢弃 `inspectCommitBindings` 暂存的 Commit 而不合并。仅清除缓存；群 epoch 不变。无暂存为空操作。
     * 注意：`inspectCommitBindings` 执行后 OpenMLS 已处理过该 commit 一次；discard 后**勿**用相同字节
     * 再次 inspect——验证通过则用 `processCommit`，否则等待重新投递。槽位清空后可 inspect **不同** commit。
     */
    discardIncomingCommit(conv_key: string): void;
    /**
     * EN: Encrypt an application payload (the P3 envelope bytes) for the group.
     * CN: 为该群加密一条应用载荷（P3 信封字节）。
     */
    encrypt(conv_key: string, plaintext: Uint8Array): Uint8Array;
    /**
     * EN: Current MLS epoch of the bound group (for chain epoch catch-up). CN: 绑定群的
     * 当前 MLS epoch（用于链上 epoch 补齐）。
     */
    epoch(conv_key: string): bigint;
    /**
     * EN: Track A escrow export (§3.2/§3.3/§7.1). Same wire layout as `exportState` but the
     * storage KV has the signature key pair REMOVED, so the resulting blob grants READ
     * (decrypt current epoch + same-epoch backlog via the captured secret-tree root, and follow
     * others' commits) WITHOUT the ability to impersonate this identity. The blob is then sealed
     * under `K_mls_escrow` by the caller before it touches the sync anchor.
     * CN: 路线 A 托管导出（§3.2/§3.3/§7.1）。线上布局与 `exportState` 相同，但存储 KV **剔除签名密钥对**，
     * 故 blob 只授「读」（凭捕获的 secret-tree 根解当前 epoch 与同 epoch backlog、跟随他人 commit），
     * **不授冒名**。调用方在写入同步锚点前用 `K_mls_escrow` 封装该 blob。
     */
    exportEscrowState(): Uint8Array;
    /**
     * EN: Track A online-handoff step 2 (design §5.2). Export JUST the signature key pair as a
     * transferable bundle so the OLD primary can hand sending authority to a NEW device. Layout:
     * `public || u32 count || [k,v]*` where the KV pairs are exactly the signature-key storage
     * entries (discovered via the same probe technique as `exportEscrowState`, but KEPT instead of
     * filtered). The caller MUST encrypt this bundle to the target device's peer key before it
     * leaves the device (it grants impersonation) and deliver it over the account self-channel.
     * Requires a live signer (a read-only device has nothing to hand off).
     * CN: 路线 A 在线交接步骤 2（设计 §5.2）。**仅**把签名密钥对导出为可转移 bundle，使**旧主设备**把发送权
     * 交给**新设备**。布局：`public || u32 count || [k,v]*`，KV 恰为签名钥存储项（用与 `exportEscrowState`
     * 相同的探针法定位，但**保留**而非过滤）。调用方在 bundle 离开本设备前**必须**用目标设备对端钥加密
     * （它授予冒名能力），并经账户自通道投递。需持签名钥（只读设备无可交接）。
     */
    exportSigningKeys(): Uint8Array;
    /**
     * EN: Snapshot the entire OpenMLS state (storage KV + identity + the live group
     * handles' GroupIds) into an opaque blob the client persists (IndexedDB). All
     * key material lives in the provider's storage; this captures it verbatim so a
     * later `restore` recreates an identical client (cross-refresh / multi-device).
     * CN: 把整套 OpenMLS 状态（存储 KV + 身份 + 活跃群句柄的 GroupId）快照成一个不透明 blob
     * 供客户端持久化（IndexedDB）。所有密钥材料都在 provider 存储里，这里原样捕获，使后续
     * `restore` 能重建完全一致的客户端（跨刷新 / 多设备）。
     */
    exportState(): Uint8Array;
    /**
     * EN: Drop a local group handle (e.g. after leave/kick). CN: 丢弃本地群句柄（退群/被踢后）。
     */
    forgetGroup(conv_key: string): void;
    /**
     * EN: Generate + persist a fresh KeyPackage; returns bytes for `publish_key_package`. When a
     * leaf binding is installed (`setLeafBinding`), it is embedded as a custom leaf-node extension
     * (advertised in the leaf capabilities) so the account ownership travels inside MLS (§3.9).
     * CN: 生成并持久化一个 KeyPackage；返回字节用于 `publish_key_package`。装有 leaf 绑定（`setLeafBinding`）
     * 时，作为自定义 leaf-node 扩展嵌入（并在 leaf capabilities 声明），使账户归属随 MLS 内传（§3.9）。
     */
    generateKeyPackage(): Uint8Array;
    /**
     * EN: True if a live group is bound to this conversation id. CN: 该会话是否已有活跃群。
     */
    hasGroup(conv_key: string): boolean;
    /**
     * EN: Process an incoming Commit to a *staged* state WITHOUT merging, and return the E2EI
     * device-leaf bindings (§3.9) of every leaf this Commit ADDS (empty = no Add). The staged commit is
     * cached under `conv_key`; the caller MUST then either `processCommit` (same bytes → reuses the
     * cache so the message is processed EXACTLY ONCE) once the bindings verify, or `discardIncomingCommit`
     * to drop a Commit that admits an unverifiable leaf. This enables MEMBER-side re-verification: a
     * follower independently confirms every added leaf is account-bound, not only the committer.
     * CN: 把进入的 Commit 处理为 *staged*（**不合并**），返回该 Commit **新增**的每个 leaf 的 E2EI 设备 leaf
     * 绑定（§3.9）（空＝无 Add）。staged commit 以 `conv_key` 缓存；调用方随后**必须**：绑定通过则 `processCommit`
     * （同字节 → 复用缓存，使消息**只处理一次**），或对新增不可验证 leaf 的 Commit `discardIncomingCommit`。
     * Re-inspecting the **same** commit bytes is idempotent; a **different** commit while one is cached
     * is rejected (prevents silently dropping the prior staged commit). CN: 由此支持**成员侧复验**。
     * **相同** commit 字节复 inspect 幂等；已有暂存时再 inspect **不同** commit 拒绝（避免静默丢弃先前暂存）。
     */
    inspectCommitBindings(conv_key: string, commit: Uint8Array): KpBinding[];
    /**
     * EN: Track A online-handoff step 4 (design §5.2). Install a signing-key bundle (from
     * `exportSigningKeys`) into a READ-ONLY escrow client, upgrading it to an active sender. Rejects
     * if this client already has a signer, or if the bundle's public key does not match the leaf
     * identity pinned at escrow restore (prevents grafting a FOREIGN signer onto this leaf). After
     * success `isReadOnly()` is false and the sending methods unlock. The §5 HandoffReceipt (verified
     * separately by the JS coordinator) is what authorizes calling this.
     * CN: 路线 A 在线交接步骤 4（设计 §5.2）。把签名钥 bundle（来自 `exportSigningKeys`）装入**只读**托管
     * 客户端，升级为活跃发送者。若已持签名钥、或 bundle 公钥与托管恢复时锚定的叶子身份不符（防止把**异身份**
     * 签名钥嫁接到本叶子）则拒绝。成功后 `isReadOnly()` 为 false，发送方法解锁。授权调用此方法的是 §5
     * HandoffReceipt（由 JS 协调器单独验证）。
     */
    installSigningKeys(bundle: Uint8Array): void;
    /**
     * EN: True when this client was restored from an escrow blob and holds no signing key.
     * CN: 该客户端是否由托管 blob 恢复且不持有签名钥。
     */
    isReadOnly(): boolean;
    /**
     * EN: Parse a published KeyPackage and extract the fields needed to verify its E2EI device-leaf
     * credential (§3.9): leaf identity, leaf signature key, and the embedded binding blob (empty if
     * none). Validates the KeyPackage on the same path `addMembers` uses, so a malformed KP is
     * rejected here. The SS58 signature check itself runs in TS. CN: 解析已发布 KeyPackage，提取验证其
     * E2EI 设备 leaf 凭证（§3.9）所需字段：leaf identity、leaf 签名钥、嵌入的绑定 blob（无则空）。在与
     * `addMembers` 相同路径上校验 KeyPackage，故畸形 KP 在此被拒。SS58 签名校验本身在 TS 运行。
     */
    keyPackageBinding(key_package: Uint8Array): KpBinding;
    /**
     * EN: List conversation keys bound to live groups. CN: 列出已绑定活跃群的会话键。
     */
    listGroups(): string[];
    /**
     * EN: Leaf credential identities (`account#deviceId`) of every member currently in the bound group,
     * in ratchet-tree order. Read-only roster for the 1:1 Wire multi-leaf device-disclosure UX (§3.9 /
     * design §8): the caller groups identities by account to show "how many devices each side has" and
     * to pick a self device to remove (PCS self-heal). Every listed leaf was account-bound-verified at
     * its add path (§3.9 induction), so membership here == an E2EI-verified device. Empty when no group.
     * CN: 绑定群当前每个成员的 leaf 凭证身份（`account#deviceId`），按棘轮树序返回。供 1:1 Wire 多 leaf 的
     * 设备披露 UX（§3.9 / 设计 §8）只读用：调用方按账户分组以展示「各方有几台设备」、并据此挑选要移除的本端
     * 设备（PCS 自愈）。此处每个 leaf 均在其 add 路径上经账户绑定校验（§3.9 归纳），故在列即为 E2EI 已验证设备。
     * 无群时返回空。
     */
    memberIdentities(conv_key: string): string[];
    /**
     * EN: Merge the locally staged pending commit (call after the relay ACCEPTs the slot).
     * CN: 合并本地暂存的 pending commit（relay ACCEPT 该槽位后调用）。
     */
    mergePending(conv_key: string): void;
    /**
     * EN: Create a client identity (signature keypair + basic credential).
     * CN: 创建客户端身份（签名密钥对 + basic credential）。
     */
    constructor(identity: string);
    /**
     * EN: Apply a Commit to catch up an epoch (handshake log replay / §6).
     * CN: 应用 Commit 补齐一个 epoch（握手日志回放 / §6）。
     */
    processCommit(conv_key: string, commit: Uint8Array): void;
    /**
     * EN: Join a group by processing a Welcome (先读后删 step 2). `conv_key` binds the
     * resulting group to the frontend conversation id. CN: 处理 Welcome 入群（先读后删第2步）。
     */
    processWelcome(conv_key: string, welcome: Uint8Array): void;
    /**
     * EN: Remove members by MLS identity (usually `account#deviceId`). Each hint must resolve to
     * exactly one leaf; bare account hints are allowed only when that account has one leaf.
     * CN: 按 MLS identity（通常为 `account#deviceId`）移除成员。每个 hint 须唯一对应一个 leaf；裸账户 hint
     * 仅在该账户只有一个 leaf 时允许。
     */
    removeMembers(conv_key: string, member_identities: string[]): CommitOut;
    /**
     * EN: Like `removeMembers`, but DO NOT merge — leaves the removal commit *staged* so the 1:1
     * Wire coordinator can `mergePending` on ACCEPT or `clearPending` on EPOCH_STALE. Each
     * `member_identities` entry must match exactly one leaf: full `account#deviceId`, or a bare
     * account only when that account has a single leaf in the group (ambiguous account hints with
     * multiple device leaves are rejected). CN: 同 `removeMembers`，但**不合并**——把移除 commit 保留为
     * *staged*，使 1:1 Wire 协调设备在 ACCEPT 时 `mergePending`、EPOCH_STALE 时 `clearPending`。
     * 每个 `member_identities` 项须**精确**匹配一个 leaf：完整 `account#deviceId`，或当该账户在群内仅
     * 一个 leaf 时可写裸账户（多设备 leaf 时裸账户 hint 会因歧义被拒）。
     */
    removeMembersStaged(conv_key: string, member_identities: string[]): CommitOut;
    /**
     * EN: Rebuild a client from an `exportState` blob: restore the storage KV, read
     * the signature keypair back out of it, and `MlsGroup::load` every persisted
     * group handle. CN: 从 `exportState` blob 重建客户端：恢复存储 KV，从中读回签名密钥对，
     * 并 `MlsGroup::load` 每个持久化的群句柄。
     */
    static restore(blob: Uint8Array): MlsClient;
    /**
     * EN: Rebuild a READ-ONLY client from an `exportEscrowState` blob (no signature private key).
     * Decrypt and commit catch-up work; every sending method (`generateKeyPackage`/`createGroup`/
     * `addMembers`/`removeMembers`/`swapMembers`/`encrypt`) rejects until the §5 handoff installs a
     * signer. CN: 从 `exportEscrowState` blob 重建**只读**客户端（无签名私钥）。可解密、补齐 commit；
     * 所有发送方法在 §5 交接装入签名钥前一律拒绝。
     */
    static restoreEscrow(blob: Uint8Array): MlsClient;
    /**
     * EN: Self-update (rekey) the own leaf as a *staged* commit (no merge). Returns the commit
     * bytes; rekey adds no member so there is no Welcome. Pairs with `mergePending`/`clearPending`
     * exactly like `addMembersStaged`. CN: 把本设备 leaf 做一次自更新（rekey）为 *staged* commit
     * （不合并）。返回 commit 字节；rekey 不加人故无 Welcome。与 `mergePending`/`clearPending` 配对，
     * 用法同 `addMembersStaged`。
     */
    selfUpdateStaged(conv_key: string): Uint8Array;
    /**
     * EN: Install the E2EI device-leaf credential blob to embed in every subsequently generated
     * KeyPackage's leaf node (§3.9). Idempotent; pass an empty slice to clear. Transient (not
     * snapshotted) — re-set after restore. CN: 安装 E2EI 设备 leaf 凭证 blob，置入此后生成的每个
     * KeyPackage 的 leaf 节点（§3.9）。幂等；传空切片清除。瞬态（不入快照）——restore 后重设。
     */
    setLeafBinding(binding: Uint8Array): void;
    /**
     * EN: This device's stable MLS leaf signature public key (= the credential's signature key,
     * reused across all its KeyPackages). TS signs `(ctx ‖ account ‖ deviceId ‖ thisKey)` with the
     * account SS58 key to mint the E2EI device-leaf credential (§3.9), then installs it via
     * `setLeafBinding`. CN: 本设备稳定的 MLS leaf 签名公钥（= credential 的签名钥，所有 KeyPackage 复用）。
     * TS 用账户 SS58 钥签 `(ctx ‖ account ‖ deviceId ‖ 本钥)` 铸造 E2EI 设备 leaf 凭证（§3.9），再经
     * `setLeafBinding` 安装。
     */
    signaturePublicKey(): Uint8Array;
    /**
     * EN: Fingerprint of the currently STAGED (pending, not-yet-merged) commit — the POST-commit
     * `tree_hash` / `confirmed_transcript_hash` / epoch the chain `commit(new_tree_hash,
     * new_transcript_hash)` must carry. Read from `StagedCommit::staged_context()` so a group Wire
     * device op can submit the TRUE new-epoch commitments BEFORE the chain CAS verdict (no speculative
     * merge). Errors when no commit is staged. Group-Wire G3b (CHAT_GROUP_WIREIFY_DESIGN §7.2).
     * CN: 当前**已暂存**（pending、未合并）commit 的指纹——链上 `commit(new_tree_hash, new_transcript_hash)`
     * 须携带的**后置** `tree_hash` / `confirmed_transcript_hash` / epoch。读自 `StagedCommit::staged_context()`，
     * 使群 Wire 设备操作能在链 CAS 裁决**之前**提交**真实**新 epoch 承诺（无需投机合并）。无暂存 commit 时报错。
     * 群 Wire G3b（设计 §7.2）。
     */
    stagedCommitFingerprint(conv_key: string): GroupFingerprint;
    /**
     * EN: Swap members (remove + add in one commit). CN: 同一 commit 内替换成员。
     */
    swapMembers(conv_key: string, remove_identities: string[], key_packages: Uint8Array[]): CommitOut;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_commitout_free: (a: number, b: number) => void;
    readonly __wbg_get_commitout_commit: (a: number) => [number, number];
    readonly __wbg_get_commitout_epoch: (a: number) => bigint;
    readonly __wbg_get_commitout_transcript_hash: (a: number) => [number, number];
    readonly __wbg_get_commitout_tree_hash: (a: number) => [number, number];
    readonly __wbg_get_commitout_welcome: (a: number) => [number, number];
    readonly __wbg_get_groupfingerprint_transcript_hash: (a: number) => [number, number];
    readonly __wbg_get_groupfingerprint_tree_hash: (a: number) => [number, number];
    readonly __wbg_get_kpbinding_binding: (a: number) => [number, number];
    readonly __wbg_get_kpbinding_identity: (a: number) => [number, number];
    readonly __wbg_get_kpbinding_signature_key: (a: number) => [number, number];
    readonly __wbg_groupfingerprint_free: (a: number, b: number) => void;
    readonly __wbg_kpbinding_free: (a: number, b: number) => void;
    readonly __wbg_mlsclient_free: (a: number, b: number) => void;
    readonly __wbg_set_commitout_commit: (a: number, b: number, c: number) => void;
    readonly __wbg_set_commitout_epoch: (a: number, b: bigint) => void;
    readonly __wbg_set_commitout_transcript_hash: (a: number, b: number, c: number) => void;
    readonly __wbg_set_commitout_tree_hash: (a: number, b: number, c: number) => void;
    readonly __wbg_set_commitout_welcome: (a: number, b: number, c: number) => void;
    readonly __wbg_set_kpbinding_identity: (a: number, b: number, c: number) => void;
    readonly __wbg_set_kpbinding_signature_key: (a: number, b: number, c: number) => void;
    readonly mlsclient_addMembers: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mlsclient_addMembersStaged: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mlsclient_cipherSuite: (a: number) => number;
    readonly mlsclient_clearPending: (a: number, b: number, c: number) => [number, number];
    readonly mlsclient_createGroup: (a: number, b: number, c: number) => [number, number, number];
    readonly mlsclient_decrypt: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly mlsclient_discardIncomingCommit: (a: number, b: number, c: number) => void;
    readonly mlsclient_encrypt: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly mlsclient_epoch: (a: number, b: number, c: number) => [bigint, number, number];
    readonly mlsclient_exportEscrowState: (a: number) => [number, number, number, number];
    readonly mlsclient_exportSigningKeys: (a: number) => [number, number, number, number];
    readonly mlsclient_exportState: (a: number) => [number, number, number, number];
    readonly mlsclient_forgetGroup: (a: number, b: number, c: number) => void;
    readonly mlsclient_generateKeyPackage: (a: number) => [number, number, number, number];
    readonly mlsclient_hasGroup: (a: number, b: number, c: number) => number;
    readonly mlsclient_inspectCommitBindings: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly mlsclient_installSigningKeys: (a: number, b: number, c: number) => [number, number];
    readonly mlsclient_isReadOnly: (a: number) => number;
    readonly mlsclient_keyPackageBinding: (a: number, b: number, c: number) => [number, number, number];
    readonly mlsclient_listGroups: (a: number) => [number, number];
    readonly mlsclient_memberIdentities: (a: number, b: number, c: number) => [number, number];
    readonly mlsclient_mergePending: (a: number, b: number, c: number) => [number, number];
    readonly mlsclient_new: (a: number, b: number) => [number, number, number];
    readonly mlsclient_processCommit: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly mlsclient_processWelcome: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly mlsclient_removeMembers: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mlsclient_removeMembersStaged: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mlsclient_restore: (a: number, b: number) => [number, number, number];
    readonly mlsclient_restoreEscrow: (a: number, b: number) => [number, number, number];
    readonly mlsclient_selfUpdateStaged: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mlsclient_setLeafBinding: (a: number, b: number, c: number) => void;
    readonly mlsclient_signaturePublicKey: (a: number) => [number, number, number, number];
    readonly mlsclient_stagedCommitFingerprint: (a: number, b: number, c: number) => [number, number, number];
    readonly mlsclient_swapMembers: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number];
    readonly __wbg_set_groupfingerprint_epoch: (a: number, b: bigint) => void;
    readonly __wbg_set_groupfingerprint_transcript_hash: (a: number, b: number, c: number) => void;
    readonly __wbg_set_groupfingerprint_tree_hash: (a: number, b: number, c: number) => void;
    readonly __wbg_set_kpbinding_binding: (a: number, b: number, c: number) => void;
    readonly __wbg_get_groupfingerprint_epoch: (a: number) => bigint;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
