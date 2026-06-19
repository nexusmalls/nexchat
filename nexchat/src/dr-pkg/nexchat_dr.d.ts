/* tslint:disable */
/* eslint-disable */

/**
 * EN: WASM handle owning one Olm account and its per-peer-device sessions.
 * CN: 持有一个 Olm 账户及其每对端设备会话的 WASM 句柄。
 */
export class DrClient {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * EN: X3DH responder: build an inbound session from a received `dm_init` body (an
     * Olm PreKeyMessage), consuming the matching `OPK`. Returns the first plaintext and
     * the sender's recovered `IK`. Keyed by `peer_device` hex (caller MUST check it
     * equals `blake2_128(identity_key)`). CN: X3DH 应答方：由收到的 `dm_init` 体（Olm
     * PreKeyMessage）建立入站会话，消费匹配的 `OPK`。返回首条明文与还原的发送方 `IK`。
     * 以 `peer_device` 十六进制为键（调用方必须校验其等于 `blake2_128(identity_key)`）。
     */
    createInboundSession(peer_device: string, prekey_body: Uint8Array): DrInbound;
    /**
     * EN: X3DH initiator: create an outbound session to a peer device using its `IK`
     * and one prekey (`OPK` if available, else its `SPK`). Keyed by `peer_device` hex.
     * CN: X3DH 发起方：用对端设备的 `IK` 与一条预密钥（有 `OPK` 用之，否则用 `SPK`）建立出站
     * 会话，以 `peer_device` 十六进制为键。
     */
    createOutboundSession(peer_device: string, their_ik: Uint8Array, their_prekey: Uint8Array): void;
    /**
     * EN: Decrypt a `dm_msg`/`dm_init` body on the session for `peer_device`. `msg_type`
     * is `0` (PreKey/`Init`) or `1` (Normal/`Msg`). CN: 在 `peer_device` 的会话上解密
     * `dm_msg`/`dm_init` 体。`msg_type` 为 `0`（PreKey/`Init`）或 `1`（Normal/`Msg`）。
     */
    decrypt(peer_device: string, msg_type: number, body: Uint8Array): Uint8Array;
    /**
     * EN: This device's Ed25519 public key (Olm signing key; distinct from the account
     * sr25519 key). CN: 本设备 Ed25519 公钥（Olm 签名钥；区别于账户 sr25519 钥）。
     */
    ed25519Key(): Uint8Array;
    /**
     * EN: Encrypt `plaintext` on the session for `peer_device`. Returns `[msg_type:u8]
     * ‖ body`, where `msg_type` is `0` (PreKey → `DmKind::Init`) or `1` (Normal →
     * `DmKind::Msg`). CN: 在 `peer_device` 的会话上加密 `plaintext`。返回 `[msg_type:u8] ‖
     * body`，`msg_type` 为 `0`（PreKey → `DmKind::Init`）或 `1`（Normal → `DmKind::Msg`）。
     */
    encrypt(peer_device: string, plaintext: Uint8Array): Uint8Array;
    /**
     * EN: Current fallback key (`SPK`, 32 bytes) if any. CN: 当前回退钥（`SPK`，32 字节，若有）。
     */
    fallbackKey(): Uint8Array | undefined;
    /**
     * EN: Generate a fresh fallback key (maps to the signed prekey `SPK`). CN: 生成新的
     * 回退钥（映射到签名预密钥 `SPK`）。
     */
    generateFallbackKey(): void;
    /**
     * EN: Generate `count` new one-time prekeys (`OPK`). Read them via `oneTimeKeys()`
     * and publish (Merkle root on-chain + leaves to relay), then `markKeysAsPublished()`.
     * CN: 生成 `count` 个新一次性预密钥（`OPK`）。经 `oneTimeKeys()` 读取并发布（链上 Merkle
     * 根 + 叶子给 relay），随后 `markKeysAsPublished()`。
     */
    generateOneTimeKeys(count: number): void;
    /**
     * EN: Whether a live session exists for `peer_device`. CN: `peer_device` 是否有活跃会话。
     */
    hasSession(peer_device: string): boolean;
    /**
     * EN: This device's Curve25519 identity public key (`IK`, 32 bytes). The on-chain
     * `device_id` is `blake2_128(IK)`. CN: 本设备 Curve25519 身份公钥（`IK`，32 字节）。
     * 链上 `device_id` 即 `blake2_128(IK)`。
     */
    identityKey(): Uint8Array;
    /**
     * EN: Load a session for `peer_device` from a JSON pickle. CN: 由 JSON pickle 为
     * `peer_device` 载入会话。
     */
    loadSession(peer_device: string, pickle: string): void;
    /**
     * EN: Mark all currently unpublished one-time / fallback keys as published. Call
     * after a successful on-chain `set_opk_root` / `set_signed_prekey`. CN: 把当前所有
     * 未发布的一次性 / 回退钥标记为已发布。链上 `set_opk_root` / `set_signed_prekey` 成功后调用。
     */
    markKeysAsPublished(): void;
    /**
     * EN: Create a fresh Olm account (new device identity). CN: 新建 Olm 账户（新设备身份）。
     */
    constructor();
    /**
     * EN: Current unpublished one-time prekeys, concatenated as `n × 32` bytes (each a
     * Curve25519 public key). CN: 当前未发布的一次性预密钥，按 `n × 32` 字节拼接（每个为
     * Curve25519 公钥）。
     */
    oneTimeKeys(): Uint8Array;
    /**
     * EN: Serialize the account (JSON pickle). At-rest encryption is the TS store's job
     * (vault-derived key). CN: 序列化账户（JSON pickle）。落盘加密由 TS 存储层负责（vault 派生钥）。
     */
    pickle(): string;
    /**
     * EN: Serialize the session for `peer_device` (JSON pickle). CN: 序列化 `peer_device`
     * 的会话（JSON pickle）。
     */
    pickleSession(peer_device: string): string;
    /**
     * EN: Restore an account from a JSON pickle (sessions are reloaded separately via
     * `loadSession`). CN: 由 JSON pickle 恢复账户（会话经 `loadSession` 单独重载）。
     */
    static restore(pickle: string): DrClient;
}

/**
 * EN: Result of establishing an inbound session from a `dm_init` body: the decrypted
 * first plaintext and the sender's recovered identity key (caller checks
 * `blake2_128(identity_key) == sender_dev`). CN: 由 `dm_init` 体建立入站会话的结果：
 * 解密出的首条明文 + 还原的发送方身份钥（调用方校验 `blake2_128(identity_key)==sender_dev`）。
 */
export class DrInbound {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * EN: Sender's Curve25519 identity key (32 bytes). CN: 发送方 Curve25519 身份钥（32 字节）。
     */
    identity_key: Uint8Array;
    /**
     * EN: Decrypted first plaintext. CN: 解密出的首条明文。
     */
    plaintext: Uint8Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_drclient_free: (a: number, b: number) => void;
    readonly __wbg_drinbound_free: (a: number, b: number) => void;
    readonly __wbg_get_drinbound_identity_key: (a: number) => [number, number];
    readonly __wbg_get_drinbound_plaintext: (a: number) => [number, number];
    readonly __wbg_set_drinbound_identity_key: (a: number, b: number, c: number) => void;
    readonly __wbg_set_drinbound_plaintext: (a: number, b: number, c: number) => void;
    readonly drclient_createInboundSession: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly drclient_createOutboundSession: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly drclient_decrypt: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly drclient_ed25519Key: (a: number) => [number, number];
    readonly drclient_encrypt: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly drclient_fallbackKey: (a: number) => [number, number];
    readonly drclient_generateFallbackKey: (a: number) => void;
    readonly drclient_generateOneTimeKeys: (a: number, b: number) => void;
    readonly drclient_hasSession: (a: number, b: number, c: number) => number;
    readonly drclient_identityKey: (a: number) => [number, number];
    readonly drclient_loadSession: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly drclient_markKeysAsPublished: (a: number) => void;
    readonly drclient_new: () => number;
    readonly drclient_oneTimeKeys: (a: number) => [number, number];
    readonly drclient_pickle: (a: number) => [number, number, number, number];
    readonly drclient_pickleSession: (a: number, b: number, c: number) => [number, number, number, number];
    readonly drclient_restore: (a: number, b: number) => [number, number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
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
