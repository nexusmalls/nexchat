/* @ts-self-types="./nexchat_dr.d.ts" */

/**
 * EN: WASM handle owning one Olm account and its per-peer-device sessions.
 * CN: 持有一个 Olm 账户及其每对端设备会话的 WASM 句柄。
 */
export class DrClient {
    static __wrap(ptr) {
        const obj = Object.create(DrClient.prototype);
        obj.__wbg_ptr = ptr;
        DrClientFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DrClientFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_drclient_free(ptr, 0);
    }
    /**
     * EN: X3DH responder: build an inbound session from a received `dm_init` body (an
     * Olm PreKeyMessage), consuming the matching `OPK`. Returns the first plaintext and
     * the sender's recovered `IK`. Keyed by `peer_device` hex (caller MUST check it
     * equals `blake2_128(identity_key)`). CN: X3DH 应答方：由收到的 `dm_init` 体（Olm
     * PreKeyMessage）建立入站会话，消费匹配的 `OPK`。返回首条明文与还原的发送方 `IK`。
     * 以 `peer_device` 十六进制为键（调用方必须校验其等于 `blake2_128(identity_key)`）。
     * @param {string} peer_device
     * @param {Uint8Array} prekey_body
     * @returns {DrInbound}
     */
    createInboundSession(peer_device, prekey_body) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(prekey_body, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_createInboundSession(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return DrInbound.__wrap(ret[0]);
    }
    /**
     * EN: X3DH initiator: create an outbound session to a peer device using its `IK`
     * and one prekey (`OPK` if available, else its `SPK`). Keyed by `peer_device` hex.
     * CN: X3DH 发起方：用对端设备的 `IK` 与一条预密钥（有 `OPK` 用之，否则用 `SPK`）建立出站
     * 会话，以 `peer_device` 十六进制为键。
     * @param {string} peer_device
     * @param {Uint8Array} their_ik
     * @param {Uint8Array} their_prekey
     */
    createOutboundSession(peer_device, their_ik, their_prekey) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(their_ik, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(their_prekey, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_createOutboundSession(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Decrypt a `dm_msg`/`dm_init` body on the session for `peer_device`. `msg_type`
     * is `0` (PreKey/`Init`) or `1` (Normal/`Msg`). CN: 在 `peer_device` 的会话上解密
     * `dm_msg`/`dm_init` 体。`msg_type` 为 `0`（PreKey/`Init`）或 `1`（Normal/`Msg`）。
     * @param {string} peer_device
     * @param {number} msg_type
     * @param {Uint8Array} body
     * @returns {Uint8Array}
     */
    decrypt(peer_device, msg_type, body) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(body, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_decrypt(this.__wbg_ptr, ptr0, len0, msg_type, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * EN: This device's Ed25519 public key (Olm signing key; distinct from the account
     * sr25519 key). CN: 本设备 Ed25519 公钥（Olm 签名钥；区别于账户 sr25519 钥）。
     * @returns {Uint8Array}
     */
    ed25519Key() {
        const ret = wasm.drclient_ed25519Key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Encrypt `plaintext` on the session for `peer_device`. Returns `[msg_type:u8]
     * ‖ body`, where `msg_type` is `0` (PreKey → `DmKind::Init`) or `1` (Normal →
     * `DmKind::Msg`). CN: 在 `peer_device` 的会话上加密 `plaintext`。返回 `[msg_type:u8] ‖
     * body`，`msg_type` 为 `0`（PreKey → `DmKind::Init`）或 `1`（Normal → `DmKind::Msg`）。
     * @param {string} peer_device
     * @param {Uint8Array} plaintext
     * @returns {Uint8Array}
     */
    encrypt(peer_device, plaintext) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(plaintext, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_encrypt(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * EN: Current fallback key (`SPK`, 32 bytes) if any. CN: 当前回退钥（`SPK`，32 字节，若有）。
     * @returns {Uint8Array | undefined}
     */
    fallbackKey() {
        const ret = wasm.drclient_fallbackKey(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
    /**
     * EN: Generate a fresh fallback key (maps to the signed prekey `SPK`). CN: 生成新的
     * 回退钥（映射到签名预密钥 `SPK`）。
     */
    generateFallbackKey() {
        wasm.drclient_generateFallbackKey(this.__wbg_ptr);
    }
    /**
     * EN: Generate `count` new one-time prekeys (`OPK`). Read them via `oneTimeKeys()`
     * and publish (Merkle root on-chain + leaves to relay), then `markKeysAsPublished()`.
     * CN: 生成 `count` 个新一次性预密钥（`OPK`）。经 `oneTimeKeys()` 读取并发布（链上 Merkle
     * 根 + 叶子给 relay），随后 `markKeysAsPublished()`。
     * @param {number} count
     */
    generateOneTimeKeys(count) {
        wasm.drclient_generateOneTimeKeys(this.__wbg_ptr, count);
    }
    /**
     * EN: Whether a live session exists for `peer_device`. CN: `peer_device` 是否有活跃会话。
     * @param {string} peer_device
     * @returns {boolean}
     */
    hasSession(peer_device) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_hasSession(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    /**
     * EN: This device's Curve25519 identity public key (`IK`, 32 bytes). The on-chain
     * `device_id` is `blake2_128(IK)`. CN: 本设备 Curve25519 身份公钥（`IK`，32 字节）。
     * 链上 `device_id` 即 `blake2_128(IK)`。
     * @returns {Uint8Array}
     */
    identityKey() {
        const ret = wasm.drclient_identityKey(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Load a session for `peer_device` from a JSON pickle. CN: 由 JSON pickle 为
     * `peer_device` 载入会话。
     * @param {string} peer_device
     * @param {string} pickle
     */
    loadSession(peer_device, pickle) {
        const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(pickle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_loadSession(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * EN: Mark all currently unpublished one-time / fallback keys as published. Call
     * after a successful on-chain `set_opk_root` / `set_signed_prekey`. CN: 把当前所有
     * 未发布的一次性 / 回退钥标记为已发布。链上 `set_opk_root` / `set_signed_prekey` 成功后调用。
     */
    markKeysAsPublished() {
        wasm.drclient_markKeysAsPublished(this.__wbg_ptr);
    }
    /**
     * EN: Create a fresh Olm account (new device identity). CN: 新建 Olm 账户（新设备身份）。
     */
    constructor() {
        const ret = wasm.drclient_new();
        this.__wbg_ptr = ret;
        DrClientFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * EN: Current unpublished one-time prekeys, concatenated as `n × 32` bytes (each a
     * Curve25519 public key). CN: 当前未发布的一次性预密钥，按 `n × 32` 字节拼接（每个为
     * Curve25519 公钥）。
     * @returns {Uint8Array}
     */
    oneTimeKeys() {
        const ret = wasm.drclient_oneTimeKeys(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Serialize the account (JSON pickle). At-rest encryption is the TS store's job
     * (vault-derived key). CN: 序列化账户（JSON pickle）。落盘加密由 TS 存储层负责（vault 派生钥）。
     * @returns {string}
     */
    pickle() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.drclient_pickle(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * EN: Serialize the session for `peer_device` (JSON pickle). CN: 序列化 `peer_device`
     * 的会话（JSON pickle）。
     * @param {string} peer_device
     * @returns {string}
     */
    pickleSession(peer_device) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(peer_device, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.drclient_pickleSession(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * EN: Restore an account from a JSON pickle (sessions are reloaded separately via
     * `loadSession`). CN: 由 JSON pickle 恢复账户（会话经 `loadSession` 单独重载）。
     * @param {string} pickle
     * @returns {DrClient}
     */
    static restore(pickle) {
        const ptr0 = passStringToWasm0(pickle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.drclient_restore(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return DrClient.__wrap(ret[0]);
    }
}
if (Symbol.dispose) DrClient.prototype[Symbol.dispose] = DrClient.prototype.free;

/**
 * EN: Result of establishing an inbound session from a `dm_init` body: the decrypted
 * first plaintext and the sender's recovered identity key (caller checks
 * `blake2_128(identity_key) == sender_dev`). CN: 由 `dm_init` 体建立入站会话的结果：
 * 解密出的首条明文 + 还原的发送方身份钥（调用方校验 `blake2_128(identity_key)==sender_dev`）。
 */
export class DrInbound {
    static __wrap(ptr) {
        const obj = Object.create(DrInbound.prototype);
        obj.__wbg_ptr = ptr;
        DrInboundFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DrInboundFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_drinbound_free(ptr, 0);
    }
    /**
     * EN: Sender's Curve25519 identity key (32 bytes). CN: 发送方 Curve25519 身份钥（32 字节）。
     * @returns {Uint8Array}
     */
    get identity_key() {
        const ret = wasm.__wbg_get_drinbound_identity_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Decrypted first plaintext. CN: 解密出的首条明文。
     * @returns {Uint8Array}
     */
    get plaintext() {
        const ret = wasm.__wbg_get_drinbound_plaintext(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * EN: Sender's Curve25519 identity key (32 bytes). CN: 发送方 Curve25519 身份钥（32 字节）。
     * @param {Uint8Array} arg0
     */
    set identity_key(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_drinbound_identity_key(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * EN: Decrypted first plaintext. CN: 解密出的首条明文。
     * @param {Uint8Array} arg0
     */
    set plaintext(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_drinbound_plaintext(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) DrInbound.prototype[Symbol.dispose] = DrInbound.prototype.free;
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_fdd633d4bb5dd76a: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_is_function_acc5528be2b923f2: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_0beba4a1980d3eea: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_1fca8072260dd261: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_721f8decd50c87a3: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_throw_ea4887a5f8f9a9db: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_5575218572ead796: function() { return handleError(function (arg0, arg1, arg2) {
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
        __wbg_length_589238bdcf171f0e: function(arg0) {
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
        __wbg_new_with_length_9b650f44b5c44a4e: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_84ea875411254db1: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_process_44c7a14e11e9f69e: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_d721637c7ca66eb8: function(arg0, arg1, arg2) {
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
        __wbg_static_accessor_GLOBAL_THIS_2fee5048bcca5938: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_ce44e66a4935da8c: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_44f6e0cb5e67cdad: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_168f178805d978fe: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_b0e8ac4ed313fea8: function(arg0, arg1, arg2) {
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
        "./nexchat_dr_bg.js": import0,
    };
}

const DrClientFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_drclient_free(ptr, 1));
const DrInboundFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_drinbound_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
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
        module_or_path = new URL('nexchat_dr_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
