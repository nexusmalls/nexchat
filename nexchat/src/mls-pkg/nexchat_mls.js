/* @ts-self-types="./nexchat_mls.d.ts" */

/**
 * EN: Result of a membership-changing commit, surfaced to JS for the on-chain
 * `commit` extrinsic. `welcome` is the single MLS Welcome covering all addees;
 * the caller duplicates it per addee to satisfy the chain's welcome/delta bijection.
 * CN: 成员变更 commit 的产物，供 JS 提交链上 `commit`。`welcome` 是覆盖所有新成员的单条
 * MLS Welcome；调用方按新成员复制以满足链上 welcome/delta 双射。
 */
export class CommitOut {
    static __wrap(ptr) {
        const obj = Object.create(CommitOut.prototype);
        obj.__wbg_ptr = ptr;
        CommitOutFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        CommitOutFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_commitout_free(ptr, 0);
    }
    /**
     * @returns {Uint8Array}
     */
    get commit() {
        const ret = wasm.__wbg_get_commitout_commit(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {bigint}
     */
    get epoch() {
        const ret = wasm.__wbg_get_commitout_epoch(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {Uint8Array}
     */
    get transcript_hash() {
        const ret = wasm.__wbg_get_commitout_transcript_hash(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    get tree_hash() {
        const ret = wasm.__wbg_get_commitout_tree_hash(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    get welcome() {
        const ret = wasm.__wbg_get_commitout_welcome(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {Uint8Array} arg0
     */
    set commit(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_commitout_commit(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {bigint} arg0
     */
    set epoch(arg0) {
        wasm.__wbg_set_commitout_epoch(this.__wbg_ptr, arg0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set transcript_hash(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_commitout_transcript_hash(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set tree_hash(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_commitout_tree_hash(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set welcome(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_commitout_welcome(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) CommitOut.prototype[Symbol.dispose] = CommitOut.prototype.free;

/**
 * EN: Group fingerprint after a local state change (for create_group args).
 * CN: 本地状态变更后的群指纹（用于 create_group 入参）。
 */
export class GroupFingerprint {
    static __wrap(ptr) {
        const obj = Object.create(GroupFingerprint.prototype);
        obj.__wbg_ptr = ptr;
        GroupFingerprintFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        GroupFingerprintFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_groupfingerprint_free(ptr, 0);
    }
    /**
     * @returns {bigint}
     */
    get epoch() {
        const ret = wasm.__wbg_get_groupfingerprint_epoch(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {Uint8Array}
     */
    get transcript_hash() {
        const ret = wasm.__wbg_get_groupfingerprint_transcript_hash(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    get tree_hash() {
        const ret = wasm.__wbg_get_groupfingerprint_tree_hash(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {bigint} arg0
     */
    set epoch(arg0) {
        wasm.__wbg_set_groupfingerprint_epoch(this.__wbg_ptr, arg0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set transcript_hash(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_groupfingerprint_transcript_hash(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set tree_hash(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_groupfingerprint_tree_hash(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) GroupFingerprint.prototype[Symbol.dispose] = GroupFingerprint.prototype.free;

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
    static __wrap(ptr) {
        const obj = Object.create(KpBinding.prototype);
        obj.__wbg_ptr = ptr;
        KpBindingFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        KpBindingFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_kpbinding_free(ptr, 0);
    }
    /**
     * @returns {Uint8Array}
     */
    get binding() {
        const ret = wasm.__wbg_get_kpbinding_binding(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    get identity() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_kpbinding_identity(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    get signature_key() {
        const ret = wasm.__wbg_get_kpbinding_signature_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {Uint8Array} arg0
     */
    set binding(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_kpbinding_binding(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {string} arg0
     */
    set identity(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_kpbinding_identity(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set signature_key(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_kpbinding_signature_key(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) KpBinding.prototype[Symbol.dispose] = KpBinding.prototype.free;

export class MlsClient {
    static __wrap(ptr) {
        const obj = Object.create(MlsClient.prototype);
        obj.__wbg_ptr = ptr;
        MlsClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MlsClientFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mlsclient_free(ptr, 0);
    }
    /**
     * EN: Add members (>=2 for the first commit) via their published KeyPackages.
     * Returns commit + welcome + new fingerprint. CN: 通过新成员已发布的 KeyPackage 加人
     * （首个 commit 须 ≥2 人），返回 commit + welcome + 新指纹。
     * @param {string} conv_key
     * @param {Uint8Array[]} key_packages
     * @returns {CommitOut}
     */
    addMembers(conv_key, key_packages) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(key_packages, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_addMembers(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return CommitOut.__wrap(ret[0]);
    }
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
     * @param {string} conv_key
     * @param {Uint8Array[]} key_packages
     * @returns {CommitOut}
     */
    addMembersStaged(conv_key, key_packages) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(key_packages, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_addMembersStaged(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return CommitOut.__wrap(ret[0]);
    }
    /**
     * EN: IANA cipher-suite code stored on-chain. CN: 链上存储的 IANA 套件编号。
     * @returns {number}
     */
    cipherSuite() {
        const ret = wasm.mlsclient_cipherSuite(this.__wbg_ptr);
        return ret;
    }
    /**
     * EN: Discard the locally staged pending commit (call on EPOCH_STALE before adopting the
     * winning commit via `processCommit`). No-op if nothing is staged. CN: 丢弃本地暂存的 pending
     * commit（EPOCH_STALE 时、经 `processCommit` 采纳胜出 commit **前**调用）。无暂存时为空操作。
     * @param {string} conv_key
     */
    clearPending(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_clearPending(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Create a new group locally (epoch 0, creator only). When `setLeafBinding` is active the
     * creator leaf carries the same E2EI extension as generated KeyPackages (§3.9). `conv_key` is
     * the frontend conversation id used as the local handle. CN: 本地建群（epoch 0，仅创建者）。若已
     * `setLeafBinding`，创建者 leaf 携带与 KeyPackage 相同的 E2EI 扩展（§3.9）。`conv_key` 为前端会话 id。
     * @param {string} conv_key
     * @returns {GroupFingerprint}
     */
    createGroup(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_createGroup(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return GroupFingerprint.__wrap(ret[0]);
    }
    /**
     * EN: Decrypt an inbound application message; returns the plaintext payload bytes.
     * CN: 解密一条入站应用消息，返回明文载荷字节。
     * @param {string} conv_key
     * @param {Uint8Array} ciphertext
     * @returns {Uint8Array}
     */
    decrypt(conv_key, ciphertext) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(ciphertext, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_decrypt(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
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
     * @param {string} conv_key
     */
    discardIncomingCommit(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.mlsclient_discardIncomingCommit(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * EN: Encrypt an application payload (the P3 envelope bytes) for the group.
     * CN: 为该群加密一条应用载荷（P3 信封字节）。
     * @param {string} conv_key
     * @param {Uint8Array} plaintext
     * @returns {Uint8Array}
     */
    encrypt(conv_key, plaintext) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(plaintext, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_encrypt(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * EN: Current MLS epoch of the bound group (for chain epoch catch-up). CN: 绑定群的
     * 当前 MLS epoch（用于链上 epoch 补齐）。
     * @param {string} conv_key
     * @returns {bigint}
     */
    epoch(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_epoch(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return BigInt.asUintN(64, ret[0]);
    }
    /**
     * EN: Track A escrow export (§3.2/§3.3/§7.1). Same wire layout as `exportState` but the
     * storage KV has the signature key pair REMOVED, so the resulting blob grants READ
     * (decrypt current epoch + same-epoch backlog via the captured secret-tree root, and follow
     * others' commits) WITHOUT the ability to impersonate this identity. The blob is then sealed
     * under `K_mls_escrow` by the caller before it touches the sync anchor.
     * CN: 路线 A 托管导出（§3.2/§3.3/§7.1）。线上布局与 `exportState` 相同，但存储 KV **剔除签名密钥对**，
     * 故 blob 只授「读」（凭捕获的 secret-tree 根解当前 epoch 与同 epoch backlog、跟随他人 commit），
     * **不授冒名**。调用方在写入同步锚点前用 `K_mls_escrow` 封装该 blob。
     * @returns {Uint8Array}
     */
    exportEscrowState() {
        const ret = wasm.mlsclient_exportEscrowState(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
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
     * @returns {Uint8Array}
     */
    exportSigningKeys() {
        const ret = wasm.mlsclient_exportSigningKeys(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Snapshot the entire OpenMLS state (storage KV + identity + the live group
     * handles' GroupIds) into an opaque blob the client persists (IndexedDB). All
     * key material lives in the provider's storage; this captures it verbatim so a
     * later `restore` recreates an identical client (cross-refresh / multi-device).
     * CN: 把整套 OpenMLS 状态（存储 KV + 身份 + 活跃群句柄的 GroupId）快照成一个不透明 blob
     * 供客户端持久化（IndexedDB）。所有密钥材料都在 provider 存储里，这里原样捕获，使后续
     * `restore` 能重建完全一致的客户端（跨刷新 / 多设备）。
     * @returns {Uint8Array}
     */
    exportState() {
        const ret = wasm.mlsclient_exportState(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Drop a local group handle (e.g. after leave/kick). CN: 丢弃本地群句柄（退群/被踢后）。
     * @param {string} conv_key
     */
    forgetGroup(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.mlsclient_forgetGroup(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * EN: Generate + persist a fresh KeyPackage; returns bytes for `publish_key_package`. When a
     * leaf binding is installed (`setLeafBinding`), it is embedded as a custom leaf-node extension
     * (advertised in the leaf capabilities) so the account ownership travels inside MLS (§3.9).
     * CN: 生成并持久化一个 KeyPackage；返回字节用于 `publish_key_package`。装有 leaf 绑定（`setLeafBinding`）
     * 时，作为自定义 leaf-node 扩展嵌入（并在 leaf capabilities 声明），使账户归属随 MLS 内传（§3.9）。
     * @returns {Uint8Array}
     */
    generateKeyPackage() {
        const ret = wasm.mlsclient_generateKeyPackage(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: True if a live group is bound to this conversation id. CN: 该会话是否已有活跃群。
     * @param {string} conv_key
     * @returns {boolean}
     */
    hasGroup(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_hasGroup(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
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
     * @param {string} conv_key
     * @param {Uint8Array} commit
     * @returns {KpBinding[]}
     */
    inspectCommitBindings(conv_key, commit) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(commit, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_inspectCommitBindings(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v3;
    }
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
     * @param {Uint8Array} bundle
     */
    installSigningKeys(bundle) {
        const ptr0 = passArray8ToWasm0(bundle, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_installSigningKeys(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: True when this client was restored from an escrow blob and holds no signing key.
     * CN: 该客户端是否由托管 blob 恢复且不持有签名钥。
     * @returns {boolean}
     */
    isReadOnly() {
        const ret = wasm.mlsclient_isReadOnly(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * EN: Parse a published KeyPackage and extract the fields needed to verify its E2EI device-leaf
     * credential (§3.9): leaf identity, leaf signature key, and the embedded binding blob (empty if
     * none). Validates the KeyPackage on the same path `addMembers` uses, so a malformed KP is
     * rejected here. The SS58 signature check itself runs in TS. CN: 解析已发布 KeyPackage，提取验证其
     * E2EI 设备 leaf 凭证（§3.9）所需字段：leaf identity、leaf 签名钥、嵌入的绑定 blob（无则空）。在与
     * `addMembers` 相同路径上校验 KeyPackage，故畸形 KP 在此被拒。SS58 签名校验本身在 TS 运行。
     * @param {Uint8Array} key_package
     * @returns {KpBinding}
     */
    keyPackageBinding(key_package) {
        const ptr0 = passArray8ToWasm0(key_package, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_keyPackageBinding(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return KpBinding.__wrap(ret[0]);
    }
    /**
     * EN: List conversation keys bound to live groups. CN: 列出已绑定活跃群的会话键。
     * @returns {string[]}
     */
    listGroups() {
        const ret = wasm.mlsclient_listGroups(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
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
     * @param {string} conv_key
     * @returns {string[]}
     */
    memberIdentities(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_memberIdentities(this.__wbg_ptr, ptr0, len0);
        var v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
    /**
     * EN: Merge the locally staged pending commit (call after the relay ACCEPTs the slot).
     * CN: 合并本地暂存的 pending commit（relay ACCEPT 该槽位后调用）。
     * @param {string} conv_key
     */
    mergePending(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_mergePending(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Create a client identity (signature keypair + basic credential).
     * CN: 创建客户端身份（签名密钥对 + basic credential）。
     * @param {string} identity
     */
    constructor(identity) {
        const ptr0 = passStringToWasm0(identity, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        MlsClientFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * EN: Apply a Commit to catch up an epoch (handshake log replay / §6).
     * CN: 应用 Commit 补齐一个 epoch（握手日志回放 / §6）。
     * @param {string} conv_key
     * @param {Uint8Array} commit
     */
    processCommit(conv_key, commit) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(commit, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_processCommit(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Join a group by processing a Welcome (先读后删 step 2). `conv_key` binds the
     * resulting group to the frontend conversation id. CN: 处理 Welcome 入群（先读后删第2步）。
     * @param {string} conv_key
     * @param {Uint8Array} welcome
     */
    processWelcome(conv_key, welcome) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(welcome, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_processWelcome(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Remove members by MLS identity (usually `account#deviceId`). Each hint must resolve to
     * exactly one leaf; bare account hints are allowed only when that account has one leaf.
     * CN: 按 MLS identity（通常为 `account#deviceId`）移除成员。每个 hint 须唯一对应一个 leaf；裸账户 hint
     * 仅在该账户只有一个 leaf 时允许。
     * @param {string} conv_key
     * @param {string[]} member_identities
     * @returns {CommitOut}
     */
    removeMembers(conv_key, member_identities) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(member_identities, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_removeMembers(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return CommitOut.__wrap(ret[0]);
    }
    /**
     * EN: Like `removeMembers`, but DO NOT merge — leaves the removal commit *staged* so the 1:1
     * Wire coordinator can `mergePending` on ACCEPT or `clearPending` on EPOCH_STALE. Each
     * `member_identities` entry must match exactly one leaf: full `account#deviceId`, or a bare
     * account only when that account has a single leaf in the group (ambiguous account hints with
     * multiple device leaves are rejected). CN: 同 `removeMembers`，但**不合并**——把移除 commit 保留为
     * *staged*，使 1:1 Wire 协调设备在 ACCEPT 时 `mergePending`、EPOCH_STALE 时 `clearPending`。
     * 每个 `member_identities` 项须**精确**匹配一个 leaf：完整 `account#deviceId`，或当该账户在群内仅
     * 一个 leaf 时可写裸账户（多设备 leaf 时裸账户 hint 会因歧义被拒）。
     * @param {string} conv_key
     * @param {string[]} member_identities
     * @returns {CommitOut}
     */
    removeMembersStaged(conv_key, member_identities) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(member_identities, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_removeMembersStaged(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return CommitOut.__wrap(ret[0]);
    }
    /**
     * EN: Rebuild a client from an `exportState` blob: restore the storage KV, read
     * the signature keypair back out of it, and `MlsGroup::load` every persisted
     * group handle. CN: 从 `exportState` blob 重建客户端：恢复存储 KV，从中读回签名密钥对，
     * 并 `MlsGroup::load` 每个持久化的群句柄。
     * @param {Uint8Array} blob
     * @returns {MlsClient}
     */
    static restore(blob) {
        const ptr0 = passArray8ToWasm0(blob, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_restore(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MlsClient.__wrap(ret[0]);
    }
    /**
     * EN: Rebuild a READ-ONLY client from an `exportEscrowState` blob (no signature private key).
     * Decrypt and commit catch-up work; every sending method (`generateKeyPackage`/`createGroup`/
     * `addMembers`/`removeMembers`/`swapMembers`/`encrypt`) rejects until the §5 handoff installs a
     * signer. CN: 从 `exportEscrowState` blob 重建**只读**客户端（无签名私钥）。可解密、补齐 commit；
     * 所有发送方法在 §5 交接装入签名钥前一律拒绝。
     * @param {Uint8Array} blob
     * @returns {MlsClient}
     */
    static restoreEscrow(blob) {
        const ptr0 = passArray8ToWasm0(blob, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_restoreEscrow(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MlsClient.__wrap(ret[0]);
    }
    /**
     * EN: Self-update (rekey) the own leaf as a *staged* commit (no merge). Returns the commit
     * bytes; rekey adds no member so there is no Welcome. Pairs with `mergePending`/`clearPending`
     * exactly like `addMembersStaged`. CN: 把本设备 leaf 做一次自更新（rekey）为 *staged* commit
     * （不合并）。返回 commit 字节；rekey 不加人故无 Welcome。与 `mergePending`/`clearPending` 配对，
     * 用法同 `addMembersStaged`。
     * @param {string} conv_key
     * @returns {Uint8Array}
     */
    selfUpdateStaged(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_selfUpdateStaged(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * EN: Install the E2EI device-leaf credential blob to embed in every subsequently generated
     * KeyPackage's leaf node (§3.9). Idempotent; pass an empty slice to clear. Transient (not
     * snapshotted) — re-set after restore. CN: 安装 E2EI 设备 leaf 凭证 blob，置入此后生成的每个
     * KeyPackage 的 leaf 节点（§3.9）。幂等；传空切片清除。瞬态（不入快照）——restore 后重设。
     * @param {Uint8Array} binding
     */
    setLeafBinding(binding) {
        const ptr0 = passArray8ToWasm0(binding, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.mlsclient_setLeafBinding(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * EN: This device's stable MLS leaf signature public key (= the credential's signature key,
     * reused across all its KeyPackages). TS signs `(ctx ‖ account ‖ deviceId ‖ thisKey)` with the
     * account SS58 key to mint the E2EI device-leaf credential (§3.9), then installs it via
     * `setLeafBinding`. CN: 本设备稳定的 MLS leaf 签名公钥（= credential 的签名钥，所有 KeyPackage 复用）。
     * TS 用账户 SS58 钥签 `(ctx ‖ account ‖ deviceId ‖ 本钥)` 铸造 E2EI 设备 leaf 凭证（§3.9），再经
     * `setLeafBinding` 安装。
     * @returns {Uint8Array}
     */
    signaturePublicKey() {
        const ret = wasm.mlsclient_signaturePublicKey(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
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
     * @param {string} conv_key
     * @returns {GroupFingerprint}
     */
    stagedCommitFingerprint(conv_key) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_stagedCommitFingerprint(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return GroupFingerprint.__wrap(ret[0]);
    }
    /**
     * EN: Swap members (remove + add in one commit). CN: 同一 commit 内替换成员。
     * @param {string} conv_key
     * @param {string[]} remove_identities
     * @param {Uint8Array[]} key_packages
     * @returns {CommitOut}
     */
    swapMembers(conv_key, remove_identities, key_packages) {
        const ptr0 = passStringToWasm0(conv_key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArrayJsValueToWasm0(remove_identities, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArrayJsValueToWasm0(key_packages, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.mlsclient_swapMembers(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return CommitOut.__wrap(ret[0]);
    }
}
if (Symbol.dispose) MlsClient.prototype[Symbol.dispose] = MlsClient.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_ef53bc310eb298a0: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_is_function_754e9f305ff6029e: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_56732c2bc353f41d: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_c236cabd84a4d769: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_67b456be8673d3d7: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_string_get_72bdf95d3ae505b1: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_1506f2235d1bdba0: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_9c758de292015997: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_crypto_38df2bab126b63dc: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_error_a6fa202b58aa1cd3: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_getRandomValues_c44a50d8cfdaebeb: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_kpbinding_new: function(arg0) {
            const ret = KpBinding.__wrap(arg0);
            return ret;
        },
        __wbg_length_4a591ecaa01354d9: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_msCrypto_bd5a034af96bcba6: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_new_227d7c05414eb861: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_new_with_length_36a4998e27b014c5: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_84ea875411254db1: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_now_190933fa139cc119: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_process_44c7a14e11e9f69e: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_3249fc62a0fafa30: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_randomFillSync_6c25eac9869eb53c: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_require_b4edbdcf3e2a1ef0: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_stack_3b0d974bbf31e44f: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_9d53f2689e622ca1: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_a1a35cec07001a8a: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_4c59f6c7ea29a144: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_e70ae9f2eb052253: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_4aa221f6a4f5ab22: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_versions_276b2795b1c6a219: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./nexchat_mls_bg.js": import0,
    };
}

const CommitOutFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_commitout_free(ptr, 1));
const GroupFingerprintFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_groupfingerprint_free(ptr, 1));
const KpBindingFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_kpbinding_free(ptr, 1));
const MlsClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mlsclient_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('nexchat_mls_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
