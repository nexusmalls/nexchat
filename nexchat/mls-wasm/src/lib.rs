//! EN: nexchat-mls — OpenMLS (RFC 9420) client engine compiled to WASM.
//! This crate is the ONLY place cryptography happens. It exposes a single
//! `MlsClient` handle to JS that owns the provider (RustCrypto + in-memory storage),
//! the account's signature identity, and a map of live `MlsGroup`s keyed by the
//! frontend conversation id (`g:<gid>` / `d:<peer>`). Mirrors the call ordering in
//! `pallets/chat/CHAT_GROUP_CLIENT_INTEGRATION.md` (publish KP → create_group →
//! add_members → welcome → application encrypt/decrypt).
//! CN: nexchat-mls —— 把 OpenMLS(RFC 9420) 客户端引擎编译为 WASM。本 crate 是密码学唯一所在，
//! 向 JS 暴露单一 `MlsClient` 句柄：持有 provider（RustCrypto + 内存存储）、账户签名身份，以及
//! 以前端会话 id 为键（`g:<gid>` / `d:<peer>`）的活跃 `MlsGroup` 表。调用顺序对齐
//! `CHAT_GROUP_CLIENT_INTEGRATION.md`（发布 KP → 建群 → 加人 → welcome → 应用消息加解密）。

use std::collections::{HashMap, HashSet};

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use sha2::{Digest, Sha256};
use tls_codec::{DeserializeBytes, Serialize as _};
use wasm_bindgen::prelude::*;
use openmls::group::WelcomeCommitMessages;

// EN: Default cipher suite; its IANA code (1) is what we store on-chain as `cipher_suite`.
// CN: 默认密码套件；其 IANA 编号(1)即链上存储的 `cipher_suite`。
const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

// EN: Custom leaf-node extension type carrying the E2EI device-leaf credential
// (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.9 phase 2): the account SS58 key's signature binding
// THIS leaf's stable signature key to its account, so any peer can verify a KeyPackage's account
// ownership from inside MLS (relay-trustless, durable past add-time). Opaque to OpenMLS (Unknown);
// produced + verified in TS against the SS58 address. CN: 承载 E2EI 设备 leaf 凭证的自定义 leaf-node
// 扩展类型（规范 §3.9 二阶段）：账户 SS58 钥把**本 leaf 稳定签名钥**绑定到其账户，使对端可从 MLS 内部
// 验证 KeyPackage 的账户归属（relay-trustless、超越 add 时刻持久）。对 OpenMLS 不透明（Unknown）；
// 在 TS 侧针对 SS58 地址产生与验证。
const E2EI_LEAF_EXT_TYPE: u16 = 0xF7E2;

fn js_err<E: std::fmt::Debug>(e: E) -> JsError {
    JsError::new(&format!("{e:?}"))
}

// EN: Track A read-only escrow guard. A client restored via `restoreEscrow` holds NO signature
// private key, so it can decrypt and follow others' commits but MUST NOT author handshakes or
// application messages until sending authority is activated via the §5 handoff. Every method that
// needs the signer rejects with this error when running read-only.
// CN: 路线 A 只读托管守卫。经 `restoreEscrow` 恢复的客户端**不持有签名私钥**，只能解密、跟随他人 commit，
// 在通过 §5 发送权交接激活前**不得**生成握手或应用消息。所有需要签名钥的方法在只读态下以此错误拒绝。
fn no_signer() -> JsError {
    JsError::new("read-only escrow device: no signing key (activate sending via §5 handoff first)")
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().to_vec()
}

// EN: Storage keys occupied by a signature key pair (deterministic once the pair is readable).
// CN: 签名密钥对在存储中占用的键（密钥对可读后 deterministic）。
fn signer_storage_keys(signer: &SignatureKeyPair) -> Result<HashSet<Vec<u8>>, String> {
    let probe = OpenMlsRustCrypto::default();
    signer.store(probe.storage()).map_err(|e| format!("{e:?}"))?;
    let v = probe
        .storage()
        .values
        .read()
        .map_err(|_| "probe storage poisoned".to_string())?;
    Ok(v.keys().cloned().collect())
}

// EN: E2EI leaf-node capabilities + extension blob shared by KeyPackage and group creation.
// CN: KeyPackage 与建群共用的 E2EI leaf-node capabilities + 扩展 blob。
fn e2ei_leaf_caps() -> Capabilities {
    Capabilities::new(
        None,
        None,
        Some(&[ExtensionType::Unknown(E2EI_LEAF_EXT_TYPE)]),
        None,
        None,
    )
}

fn e2ei_leaf_extensions(binding: &[u8]) -> Result<Extensions<LeafNode>, JsError> {
    Extensions::single(Extension::Unknown(
        E2EI_LEAF_EXT_TYPE,
        UnknownExtension(binding.to_vec()),
    ))
    .map_err(js_err)
}

// ---- compact length-prefixed encoding for state snapshots / 状态快照的紧凑长度前缀编码 ----

// EN: Versioned `exportState` / `exportEscrowState` header (`NCMS` + u32 version). `restore` /
// `restoreEscrow` accept v1 blobs and legacy blobs without a header (IndexedDB snapshots written
// before this format). CN: 带版本的 `exportState` / `exportEscrowState` 头（`NCMS` + u32 版本）。
// `restore` / `restoreEscrow` 接受 v1 blob 与**无头** legacy blob（本格式之前的 IndexedDB 快照）。
const STATE_MAGIC: [u8; 4] = *b"NCMS";
const STATE_VERSION: u32 = 1;
const MAX_STATE_BLOB_BYTES: usize = 32 * 1024 * 1024;
const MAX_STATE_IDENTITY_BYTES: u32 = 512;
const MAX_STATE_SIGNER_PUBLIC_BYTES: u32 = 256;
const MAX_STATE_CONV_KEY_BYTES: u32 = 512;
const MAX_STATE_GROUP_ID_BYTES: u32 = 256;
const MAX_STATE_GROUPS: u32 = 10_000;
const MAX_STATE_KV_ENTRIES: u32 = 100_000;
const MAX_STATE_KV_KEY_BYTES: u32 = 4096;
const MAX_STATE_KV_VALUE_BYTES: u32 = 1024 * 1024;

fn write_state_header(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&STATE_MAGIC);
    put_u32(buf, STATE_VERSION);
}

fn begin_state_parse(blob: &[u8]) -> Result<Cursor<'_>, String> {
    if blob.len() > MAX_STATE_BLOB_BYTES {
        return Err("state blob too large".into());
    }
    let mut cur = Cursor { buf: blob, pos: 0 };
    if blob.len() >= STATE_MAGIC.len() && blob[..STATE_MAGIC.len()] == STATE_MAGIC {
        cur.pos = STATE_MAGIC.len();
        let ver = cur.u32_str()?;
        if ver != STATE_VERSION {
            return Err(format!("unsupported state blob version: {ver}"));
        }
    }
    Ok(cur)
}

fn parse_state_body(cur: &mut Cursor<'_>) -> Result<ParsedStateBody, String> {
    let identity = cur
        .bytes_max_str(MAX_STATE_IDENTITY_BYTES, "identity")?
        .to_vec();
    let signer_public = cur
        .bytes_max_str(MAX_STATE_SIGNER_PUBLIC_BYTES, "signer public key")?
        .to_vec();

    let group_count = cur.count_max_str(MAX_STATE_GROUPS, "groups")?;
    let mut group_ids = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        let conv_key = String::from_utf8(
            cur.bytes_max_str(MAX_STATE_CONV_KEY_BYTES, "conv_key")?
                .to_vec(),
        )
        .map_err(|_| "bad conv_key utf8".to_string())?;
        let gid = cur
            .bytes_max_str(MAX_STATE_GROUP_ID_BYTES, "group id")?
            .to_vec();
        group_ids.push((conv_key, gid));
    }

    let kv_count = cur.count_max_str(MAX_STATE_KV_ENTRIES, "storage entries")?;
    let mut kv_pairs = Vec::with_capacity(kv_count);
    for _ in 0..kv_count {
        let k = cur
            .bytes_max_str(MAX_STATE_KV_KEY_BYTES, "storage key")?
            .to_vec();
        let v = cur
            .bytes_max_str(MAX_STATE_KV_VALUE_BYTES, "storage value")?
            .to_vec();
        kv_pairs.push((k, v));
    }

    if cur.pos != cur.buf.len() {
        return Err("state blob has trailing bytes".into());
    }

    Ok(ParsedStateBody {
        identity,
        signer_public,
        group_ids,
        kv_pairs,
    })
}

fn try_parse_state_blob(blob: &[u8]) -> Result<ParsedStateBody, String> {
    let mut cur = begin_state_parse(blob)?;
    parse_state_body(&mut cur)
}

struct ParsedStateBody {
    identity: Vec<u8>,
    signer_public: Vec<u8>,
    group_ids: Vec<(String, Vec<u8>)>,
    kv_pairs: Vec<(Vec<u8>, Vec<u8>)>,
}

fn append_state_body(
    buf: &mut Vec<u8>,
    identity: &[u8],
    signer_public: &[u8],
    groups: &[(&String, &MlsGroup)],
    kv: &[(&Vec<u8>, &Vec<u8>)],
) {
    put_bytes(buf, identity);
    put_bytes(buf, signer_public);
    put_u32(buf, groups.len() as u32);
    for (conv_key, group) in groups {
        put_bytes(buf, conv_key.as_bytes());
        put_bytes(buf, group.group_id().as_slice());
    }
    put_u32(buf, kv.len() as u32);
    for (k, v) in kv {
        put_bytes(buf, k);
        put_bytes(buf, v);
    }
}

fn put_u32(buf: &mut Vec<u8>, n: u32) {
    buf.extend_from_slice(&n.to_le_bytes());
}

fn put_bytes(buf: &mut Vec<u8>, b: &[u8]) {
    put_u32(buf, b.len() as u32);
    buf.extend_from_slice(b);
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn u32(&mut self) -> Result<u32, JsError> {
        let end = self.pos + 4;
        if end > self.buf.len() {
            return Err(JsError::new("truncated state blob"));
        }
        let n = u32::from_le_bytes(self.buf[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(n)
    }

    fn bytes(&mut self) -> Result<&'a [u8], JsError> {
        let len = self.u32()? as usize;
        let end = self.pos + len;
        if end > self.buf.len() {
            return Err(JsError::new("truncated state blob"));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn bytes_max(&mut self, max_len: u32, label: &str) -> Result<&'a [u8], JsError> {
        let len = self.u32()?;
        if len > max_len {
            return Err(JsError::new(&format!("state blob {label} too large")));
        }
        let end = self.pos + len as usize;
        if end > self.buf.len() {
            return Err(JsError::new("truncated state blob"));
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn count_max(&mut self, max: u32, label: &str) -> Result<usize, JsError> {
        let n = self.u32()?;
        if n > max {
            return Err(JsError::new(&format!("too many {label} in state blob")));
        }
        Ok(n as usize)
    }

    fn u32_str(&mut self) -> Result<u32, String> {
        self.u32().map_err(|_| "truncated state blob".to_string())
    }

    fn bytes_max_str(&mut self, max_len: u32, label: &str) -> Result<&'a [u8], String> {
        self.bytes_max(max_len, label)
            .map_err(|_| format!("state blob {label} invalid"))
    }

    fn count_max_str(&mut self, max: u32, label: &str) -> Result<usize, String> {
        self.count_max(max, label)
            .map_err(|_| format!("too many {label} in state blob"))
    }
}

/// EN: Result of a membership-changing commit, surfaced to JS for the on-chain
/// `commit` extrinsic. `welcome` is the single MLS Welcome covering all addees;
/// the caller duplicates it per addee to satisfy the chain's welcome/delta bijection.
/// CN: 成员变更 commit 的产物，供 JS 提交链上 `commit`。`welcome` 是覆盖所有新成员的单条
/// MLS Welcome；调用方按新成员复制以满足链上 welcome/delta 双射。
#[wasm_bindgen(getter_with_clone)]
pub struct CommitOut {
    pub commit: Vec<u8>,
    pub welcome: Vec<u8>,
    pub tree_hash: Vec<u8>,
    pub transcript_hash: Vec<u8>,
    pub epoch: u64,
}

/// EN: Group fingerprint after a local state change (for create_group args).
/// CN: 本地状态变更后的群指纹（用于 create_group 入参）。
#[wasm_bindgen(getter_with_clone)]
pub struct GroupFingerprint {
    pub tree_hash: Vec<u8>,
    pub transcript_hash: Vec<u8>,
    pub epoch: u64,
}

/// EN: The fields a verifier needs to check a KeyPackage's E2EI device-leaf credential (§3.9): the
/// leaf `identity` (`account#deviceFp`), the leaf `signature_key` the binding must commit to, and the
/// `binding` blob (account-SS58-key signature) extracted from the leaf-node extension. `binding` is
/// empty when the KeyPackage carries none (legacy / unbound). Verification (SS58 signatureVerify) is
/// done in TS. CN: 校验 KeyPackage 的 E2EI 设备 leaf 凭证（§3.9）所需字段：leaf `identity`
/// （`account#deviceFp`）、绑定须承诺的 leaf `signature_key`，以及从 leaf-node 扩展提取的 `binding` blob
/// （账户 SS58 钥签名）。KeyPackage 无绑定（旧版/未绑定）时 `binding` 为空。验证（SS58 signatureVerify）在 TS 完成。
#[wasm_bindgen(getter_with_clone)]
pub struct KpBinding {
    pub identity: String,
    pub signature_key: Vec<u8>,
    pub binding: Vec<u8>,
}

#[wasm_bindgen]
pub struct MlsClient {
    provider: OpenMlsRustCrypto,
    // EN: `None` for a read-only escrow client (Track A §3.2/§3.3): the escrow blob excludes the
    // signature private key, so decrypt/process-commit work but sending is gated until handoff.
    // CN: 只读托管客户端为 `None`（路线 A §3.2/§3.3）：托管 blob 不含签名私钥，可解密/跟随 commit，
    // 但发送被门控直到交接发生。
    signer: Option<SignatureKeyPair>,
    credential: CredentialWithKey,
    identity: Vec<u8>,
    groups: HashMap<String, MlsGroup>,
    // EN: E2EI device-leaf credential (§3.9) to embed in every generated KeyPackage's leaf node when
    // set. Transient session state — NOT part of the persisted snapshot (TS re-sets it after restore
    // by re-signing the stable leaf key with the account key), so the snapshot byte format is
    // unchanged. CN: 置入每个生成的 KeyPackage leaf 节点的 E2EI 设备 leaf 凭证（§3.9，设置后生效）。
    // 会话级瞬态——**不**入持久快照（TS 在 restore 后用账户钥重签稳定 leaf 钥重新设置），故快照字节格式不变。
    leaf_binding: Option<Vec<u8>>,
    // EN: Incoming Commit staged by `inspectCommitBindings` but not yet merged, keyed by conv as
    // `(commit_bytes, staged)`. Lets the TS layer read a Commit's added-leaf E2EI bindings (§3.9),
    // verify them relay-trustlessly, then `processCommit` (same bytes → reuses this so the message is
    // processed EXACTLY ONCE) or `discardIncomingCommit` when an added leaf is unverifiable. Transient,
    // never persisted. CN: 由 `inspectCommitBindings` 暂存、尚未合并的进入 Commit，按 conv 键存
    // `(commit 字节, staged)`。供 TS 读取 Commit 被加 leaf 的 E2EI 绑定（§3.9）、relay-trustless 验证后再
    // `processCommit`（同字节 → 复用之，使消息**只处理一次**）或在被加 leaf 不可验证时 `discardIncomingCommit`。
    // 瞬态，永不持久化。
    staged_incoming: HashMap<String, (Vec<u8>, StagedCommit)>,
}

#[wasm_bindgen]
impl MlsClient {
    /// EN: Create a client identity (signature keypair + basic credential).
    /// CN: 创建客户端身份（签名密钥对 + basic credential）。
    #[wasm_bindgen(constructor)]
    pub fn new(identity: &str) -> Result<MlsClient, JsError> {
        console_error_panic_hook::set_once();
        let provider = OpenMlsRustCrypto::default();
        let signer =
            SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(js_err)?;
        signer.store(provider.storage()).map_err(js_err)?;
        let identity = identity.as_bytes().to_vec();
        let credential = CredentialWithKey {
            credential: BasicCredential::new(identity.clone()).into(),
            signature_key: signer.public().into(),
        };
        Ok(MlsClient {
            provider,
            signer: Some(signer),
            credential,
            identity,
            groups: HashMap::new(),
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        })
    }

    /// EN: Snapshot the entire OpenMLS state (storage KV + identity + the live group
    /// handles' GroupIds) into an opaque blob the client persists (IndexedDB). All
    /// key material lives in the provider's storage; this captures it verbatim so a
    /// later `restore` recreates an identical client (cross-refresh / multi-device).
    /// CN: 把整套 OpenMLS 状态（存储 KV + 身份 + 活跃群句柄的 GroupId）快照成一个不透明 blob
    /// 供客户端持久化（IndexedDB）。所有密钥材料都在 provider 存储里，这里原样捕获，使后续
    /// `restore` 能重建完全一致的客户端（跨刷新 / 多设备）。
    #[wasm_bindgen(js_name = exportState)]
    pub fn export_state(&self) -> Result<Vec<u8>, JsError> {
        let mut buf = Vec::new();
        write_state_header(&mut buf);

        let mut groups: Vec<(&String, &MlsGroup)> = self.groups.iter().collect();
        groups.sort_by(|a, b| a.0.cmp(b.0));

        let values = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| JsError::new("storage poisoned"))?;
        let kv: Vec<(&Vec<u8>, &Vec<u8>)> = values.iter().collect();

        append_state_body(
            &mut buf,
            &self.identity,
            self.signer.as_ref().ok_or_else(no_signer)?.public(),
            &groups,
            &kv,
        );
        Ok(buf)
    }

    /// EN: Track A escrow export (§3.2/§3.3/§7.1). Same wire layout as `exportState` but the
    /// storage KV has the signature key pair REMOVED, so the resulting blob grants READ
    /// (decrypt current epoch + same-epoch backlog via the captured secret-tree root, and follow
    /// others' commits) WITHOUT the ability to impersonate this identity. The blob is then sealed
    /// under `K_mls_escrow` by the caller before it touches the sync anchor.
    /// CN: 路线 A 托管导出（§3.2/§3.3/§7.1）。线上布局与 `exportState` 相同，但存储 KV **剔除签名密钥对**，
    /// 故 blob 只授「读」（凭捕获的 secret-tree 根解当前 epoch 与同 epoch backlog、跟随他人 commit），
    /// **不授冒名**。调用方在写入同步锚点前用 `K_mls_escrow` 封装该 blob。
    #[wasm_bindgen(js_name = exportEscrowState)]
    pub fn export_escrow_state(&self) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        // EN: Discover exactly which storage keys the signature key pair occupies by re-storing it
        // into a throwaway provider and reading back its key set — then exclude those from the dump.
        // CN: 把签名密钥对写入一次性 provider、读回其键集，精确得出它占用的存储键，再从导出中排除。
        let probe = OpenMlsRustCrypto::default();
        signer.store(probe.storage()).map_err(js_err)?;
        let sig_keys: HashSet<Vec<u8>> = {
            let v = probe
                .storage()
                .values
                .read()
                .map_err(|_| JsError::new("probe storage poisoned"))?;
            v.keys().cloned().collect()
        };

        let mut buf = Vec::new();
        write_state_header(&mut buf);

        let mut groups: Vec<(&String, &MlsGroup)> = self.groups.iter().collect();
        groups.sort_by(|a, b| a.0.cmp(b.0));

        let values = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| JsError::new("storage poisoned"))?;
        let escrow: Vec<(&Vec<u8>, &Vec<u8>)> =
            values.iter().filter(|(k, _)| !sig_keys.contains(*k)).collect();

        append_state_body(&mut buf, &self.identity, signer.public(), &groups, &escrow);
        Ok(buf)
    }

    /// EN: Rebuild a client from an `exportState` blob: restore the storage KV, read
    /// the signature keypair back out of it, and `MlsGroup::load` every persisted
    /// group handle. CN: 从 `exportState` blob 重建客户端：恢复存储 KV，从中读回签名密钥对，
    /// 并 `MlsGroup::load` 每个持久化的群句柄。
    #[wasm_bindgen(js_name = restore)]
    pub fn restore(blob: &[u8]) -> Result<MlsClient, JsError> {
        console_error_panic_hook::set_once();
        let body = try_parse_state_blob(blob).map_err(|e| JsError::new(&e))?;

        let provider = OpenMlsRustCrypto::default();
        {
            let mut values = provider
                .storage()
                .values
                .write()
                .map_err(|_| JsError::new("storage poisoned"))?;
            for (k, v) in body.kv_pairs {
                values.insert(k, v);
            }
        }

        let signer = SignatureKeyPair::read(
            provider.storage(),
            &body.signer_public,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or_else(|| JsError::new("signature key pair missing from restored storage"))?;
        let credential = CredentialWithKey {
            credential: BasicCredential::new(body.identity.clone()).into(),
            signature_key: body.signer_public.into(),
        };

        let mut groups = HashMap::new();
        for (conv_key, gid) in body.group_ids {
            let group = MlsGroup::load(provider.storage(), &GroupId::from_slice(&gid))
                .map_err(js_err)?
                .ok_or_else(|| JsError::new(&format!("group {conv_key} not in storage")))?;
            groups.insert(conv_key, group);
        }

        Ok(MlsClient {
            provider,
            signer: Some(signer),
            credential,
            identity: body.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        })
    }

    /// EN: Rebuild a READ-ONLY client from an `exportEscrowState` blob (no signature private key).
    /// Decrypt and commit catch-up work; every sending method (`generateKeyPackage`/`createGroup`/
    /// `addMembers`/`removeMembers`/`swapMembers`/`encrypt`) rejects until the §5 handoff installs a
    /// signer. CN: 从 `exportEscrowState` blob 重建**只读**客户端（无签名私钥）。可解密、补齐 commit；
    /// 所有发送方法在 §5 交接装入签名钥前一律拒绝。
    #[wasm_bindgen(js_name = restoreEscrow)]
    pub fn restore_escrow(blob: &[u8]) -> Result<MlsClient, JsError> {
        console_error_panic_hook::set_once();
        let body = try_parse_state_blob(blob).map_err(|e| JsError::new(&e))?;

        let provider = OpenMlsRustCrypto::default();
        {
            let mut values = provider
                .storage()
                .values
                .write()
                .map_err(|_| JsError::new("storage poisoned"))?;
            for (k, v) in body.kv_pairs {
                values.insert(k, v);
            }
        }

        // EN: No SignatureKeyPair::read here — the escrow blob deliberately omits it. The public
        // half (from the blob header) still anchors the credential so the leaf identity matches.
        // CN: 此处不读 SignatureKeyPair —— 托管 blob 有意省略。仍用公钥（来自 blob 头）锚定 credential，
        // 使叶子身份一致。
        let credential = CredentialWithKey {
            credential: BasicCredential::new(body.identity.clone()).into(),
            signature_key: body.signer_public.into(),
        };

        let mut groups = HashMap::new();
        for (conv_key, gid) in body.group_ids {
            let group = MlsGroup::load(provider.storage(), &GroupId::from_slice(&gid))
                .map_err(js_err)?
                .ok_or_else(|| JsError::new(&format!("group {conv_key} not in storage")))?;
            groups.insert(conv_key, group);
        }

        Ok(MlsClient {
            provider,
            signer: None,
            credential,
            identity: body.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        })
    }

    /// EN: True when this client was restored from an escrow blob and holds no signing key.
    /// CN: 该客户端是否由托管 blob 恢复且不持有签名钥。
    #[wasm_bindgen(js_name = isReadOnly)]
    pub fn is_read_only(&self) -> bool {
        self.signer.is_none()
    }

    /// EN: Track A online-handoff step 2 (design §5.2). Export JUST the signature key pair as a
    /// transferable bundle so the OLD primary can hand sending authority to a NEW device. Layout:
    /// `public || u32 count || [k,v]*` where the KV pairs are exactly the signature-key storage
    /// entries (discovered via the same probe technique as `exportEscrowState`, but KEPT instead of
    /// filtered). The caller MUST encrypt this bundle to the target device's peer key before it
    /// leaves the device (it grants impersonation) and deliver it over the account self-channel.
    /// Requires a live signer (a read-only device has nothing to hand off).
    /// CN: 路线 A 在线交接步骤 2（设计 §5.2）。**仅**把签名密钥对导出为可转移 bundle，使**旧主设备**把发送权
    /// 交给**新设备**。布局：`public || u32 count || [k,v]*`，KV 恰为签名钥存储项（用与 `exportEscrowState`
    /// 相同的探针法定位，但**保留**而非过滤）。调用方在 bundle 离开本设备前**必须**用目标设备对端钥加密
    /// （它授予冒名能力），并经账户自通道投递。需持签名钥（只读设备无可交接）。
    #[wasm_bindgen(js_name = exportSigningKeys)]
    pub fn export_signing_keys(&self) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let probe = OpenMlsRustCrypto::default();
        signer.store(probe.storage()).map_err(js_err)?;
        let sig_keys: HashSet<Vec<u8>> = {
            let v = probe
                .storage()
                .values
                .read()
                .map_err(|_| JsError::new("probe storage poisoned"))?;
            v.keys().cloned().collect()
        };

        let mut buf = Vec::new();
        put_bytes(&mut buf, signer.public());
        let values = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| JsError::new("storage poisoned"))?;
        let bundle: Vec<(&Vec<u8>, &Vec<u8>)> =
            values.iter().filter(|(k, _)| sig_keys.contains(*k)).collect();
        put_u32(&mut buf, bundle.len() as u32);
        for (k, v) in bundle {
            put_bytes(&mut buf, k);
            put_bytes(&mut buf, v);
        }
        Ok(buf)
    }

    /// EN: Track A online-handoff step 4 (design §5.2). Install a signing-key bundle (from
    /// `exportSigningKeys`) into a READ-ONLY escrow client, upgrading it to an active sender. Rejects
    /// if this client already has a signer, or if the bundle's public key does not match the leaf
    /// identity pinned at escrow restore (prevents grafting a FOREIGN signer onto this leaf). After
    /// success `isReadOnly()` is false and the sending methods unlock. The §5 HandoffReceipt (verified
    /// separately by the JS coordinator) is what authorizes calling this.
    /// CN: 路线 A 在线交接步骤 4（设计 §5.2）。把签名钥 bundle（来自 `exportSigningKeys`）装入**只读**托管
    /// 客户端，升级为活跃发送者。若已持签名钥、或 bundle 公钥与托管恢复时锚定的叶子身份不符（防止把**异身份**
    /// 签名钥嫁接到本叶子）则拒绝。成功后 `isReadOnly()` 为 false，发送方法解锁。授权调用此方法的是 §5
    /// HandoffReceipt（由 JS 协调器单独验证）。
    #[wasm_bindgen(js_name = installSigningKeys)]
    pub fn install_signing_keys(&mut self, bundle: &[u8]) -> Result<(), JsError> {
        self.install_signing_keys_impl(bundle).map_err(|e| JsError::new(&e))
    }

    /// EN: IANA cipher-suite code stored on-chain. CN: 链上存储的 IANA 套件编号。
    #[wasm_bindgen(js_name = cipherSuite)]
    pub fn cipher_suite(&self) -> u16 {
        u16::from(CIPHERSUITE)
    }

    /// EN: This device's stable MLS leaf signature public key (= the credential's signature key,
    /// reused across all its KeyPackages). TS signs `(ctx ‖ account ‖ deviceId ‖ thisKey)` with the
    /// account SS58 key to mint the E2EI device-leaf credential (§3.9), then installs it via
    /// `setLeafBinding`. CN: 本设备稳定的 MLS leaf 签名公钥（= credential 的签名钥，所有 KeyPackage 复用）。
    /// TS 用账户 SS58 钥签 `(ctx ‖ account ‖ deviceId ‖ 本钥)` 铸造 E2EI 设备 leaf 凭证（§3.9），再经
    /// `setLeafBinding` 安装。
    #[wasm_bindgen(js_name = signaturePublicKey)]
    pub fn signature_public_key(&self) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        Ok(signer.public().to_vec())
    }

    /// EN: Install the E2EI device-leaf credential blob to embed in every subsequently generated
    /// KeyPackage's leaf node (§3.9). Idempotent; pass an empty slice to clear. Transient (not
    /// snapshotted) — re-set after restore. CN: 安装 E2EI 设备 leaf 凭证 blob，置入此后生成的每个
    /// KeyPackage 的 leaf 节点（§3.9）。幂等；传空切片清除。瞬态（不入快照）——restore 后重设。
    #[wasm_bindgen(js_name = setLeafBinding)]
    pub fn set_leaf_binding(&mut self, binding: &[u8]) {
        self.leaf_binding = if binding.is_empty() { None } else { Some(binding.to_vec()) };
    }

    /// EN: Generate + persist a fresh KeyPackage; returns bytes for `publish_key_package`. When a
    /// leaf binding is installed (`setLeafBinding`), it is embedded as a custom leaf-node extension
    /// (advertised in the leaf capabilities) so the account ownership travels inside MLS (§3.9).
    /// CN: 生成并持久化一个 KeyPackage；返回字节用于 `publish_key_package`。装有 leaf 绑定（`setLeafBinding`）
    /// 时，作为自定义 leaf-node 扩展嵌入（并在 leaf capabilities 声明），使账户归属随 MLS 内传（§3.9）。
    #[wasm_bindgen(js_name = generateKeyPackage)]
    pub fn generate_key_package(&self) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let mut builder = KeyPackage::builder();
        if let Some(binding) = &self.leaf_binding {
            builder = builder
                .leaf_node_capabilities(e2ei_leaf_caps())
                .leaf_node_extensions(e2ei_leaf_extensions(binding)?);
        }
        let bundle = builder
            .build(CIPHERSUITE, &self.provider, signer, self.credential.clone())
            .map_err(js_err)?;
        bundle.key_package().tls_serialize_detached().map_err(js_err)
    }

    /// EN: Parse a published KeyPackage and extract the fields needed to verify its E2EI device-leaf
    /// credential (§3.9): leaf identity, leaf signature key, and the embedded binding blob (empty if
    /// none). Validates the KeyPackage on the same path `addMembers` uses, so a malformed KP is
    /// rejected here. The SS58 signature check itself runs in TS. CN: 解析已发布 KeyPackage，提取验证其
    /// E2EI 设备 leaf 凭证（§3.9）所需字段：leaf identity、leaf 签名钥、嵌入的绑定 blob（无则空）。在与
    /// `addMembers` 相同路径上校验 KeyPackage，故畸形 KP 在此被拒。SS58 签名校验本身在 TS 运行。
    #[wasm_bindgen(js_name = keyPackageBinding)]
    pub fn key_package_binding(&self, key_package: &[u8]) -> Result<KpBinding, JsError> {
        let (kp_in, _) = KeyPackageIn::tls_deserialize_bytes(key_package).map_err(js_err)?;
        let kp = kp_in
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(js_err)?;
        Ok(Self::kp_binding_of(kp.leaf_node()))
    }

    /// EN: Extract a leaf node's E2EI verification fields (§3.9): identity, stable signature key, and
    /// the embedded binding blob (empty if none). Shared by `keyPackageBinding` (published KP) and
    /// `inspectCommitBindings` (a Commit's added leaves). CN: 提取 leaf 节点的 E2EI 验证字段（§3.9）：
    /// identity、稳定签名钥、嵌入绑定 blob（无则空）。`keyPackageBinding`（已发布 KP）与
    /// `inspectCommitBindings`（Commit 新增 leaf）共用。
    fn bindings_from_staged(staged: &StagedCommit) -> Vec<KpBinding> {
        staged
            .add_proposals()
            .map(|qap| Self::kp_binding_of(qap.add_proposal().key_package().leaf_node()))
            .collect()
    }

    /// EN: String-error core of `inspectCommitBindings` — testable on native targets. Rejects staging
    /// a second *different* commit while one is already cached; re-inspecting the *same* bytes is
    /// idempotent (returns the cached bindings without re-processing). CN: `inspectCommitBindings`
    /// 的 String 错误内核——原生可测。已有暂存时再 stage **不同** commit 拒绝；**相同**字节复 inspect
    /// 幂等（返回缓存绑定、不重复 process_message）。
    fn inspect_commit_bindings_impl(
        &mut self,
        conv_key: &str,
        commit: &[u8],
    ) -> Result<Vec<KpBinding>, String> {
        if let Some((cached_bytes, staged)) = self.staged_incoming.get(conv_key) {
            if cached_bytes.as_slice() == commit {
                return Ok(Self::bindings_from_staged(staged));
            }
            return Err(format!(
                "incoming commit already staged for {conv_key}; call processCommit or discardIncomingCommit first"
            ));
        }

        let (msg, _) =
            MlsMessageIn::tls_deserialize_bytes(commit).map_err(|e| format!("{e:?}"))?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(|e| format!("{e:?}"))?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| format!("no group for {conv_key}"))?;
        let processed = group
            .process_message(&self.provider, protocol)
            .map_err(|e| format!("{e:?}"))?;
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                let out = Self::bindings_from_staged(&staged);
                self.staged_incoming
                    .insert(conv_key.to_string(), (commit.to_vec(), *staged));
                Ok(out)
            }
            _ => Err("not a Commit message".into()),
        }
    }

    /// EN: String-error core of `stagedCommitFingerprint` — testable on native targets.
    /// CN: `stagedCommitFingerprint` 的 String 错误内核——原生可测。
    fn staged_commit_fingerprint_impl(&self, conv_key: &str) -> Result<GroupFingerprint, String> {
        let group = self
            .groups
            .get(conv_key)
            .ok_or_else(|| format!("no group for {conv_key}"))?;
        let staged = group
            .pending_commit()
            .ok_or_else(|| format!("no staged commit for {conv_key}"))?;
        Ok(Self::fingerprint_from_context(staged.group_context()))
    }

    fn kp_binding_of(leaf: &LeafNode) -> KpBinding {
        let binding = leaf
            .extensions()
            .iter()
            .find_map(|e| match e {
                Extension::Unknown(t, UnknownExtension(data)) if *t == E2EI_LEAF_EXT_TYPE => {
                    Some(data.clone())
                }
                _ => None,
            })
            .unwrap_or_default();
        KpBinding {
            identity: Self::credential_identity(leaf.credential()),
            signature_key: leaf.signature_key().as_slice().to_vec(),
            binding,
        }
    }

    /// EN: Create a new group locally (epoch 0, creator only). When `setLeafBinding` is active the
    /// creator leaf carries the same E2EI extension as generated KeyPackages (§3.9). `conv_key` is
    /// the frontend conversation id used as the local handle. CN: 本地建群（epoch 0，仅创建者）。若已
    /// `setLeafBinding`，创建者 leaf 携带与 KeyPackage 相同的 E2EI 扩展（§3.9）。`conv_key` 为前端会话 id。
    #[wasm_bindgen(js_name = createGroup)]
    pub fn create_group(&mut self, conv_key: &str) -> Result<GroupFingerprint, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = if let Some(binding) = &self.leaf_binding {
            MlsGroup::builder()
                .ciphersuite(CIPHERSUITE)
                .use_ratchet_tree_extension(true)
                .with_capabilities(e2ei_leaf_caps())
                .with_leaf_node_extensions(e2ei_leaf_extensions(binding)?)
                .map_err(js_err)?
                .build(&self.provider, signer, self.credential.clone())
                .map_err(js_err)?
        } else {
            let cfg = MlsGroupCreateConfig::builder()
                .ciphersuite(CIPHERSUITE)
                .use_ratchet_tree_extension(true)
                .build();
            MlsGroup::new(&self.provider, signer, &cfg, self.credential.clone())
                .map_err(js_err)?
        };
        let fp = Self::fingerprint(&group);
        self.groups.insert(conv_key.to_string(), group);
        Ok(fp)
    }

    /// EN: Add members (>=2 for the first commit) via their published KeyPackages.
    /// Returns commit + welcome + new fingerprint. CN: 通过新成员已发布的 KeyPackage 加人
    /// （首个 commit 须 ≥2 人），返回 commit + welcome + 新指纹。
    #[wasm_bindgen(js_name = addMembers)]
    pub fn add_members(
        &mut self,
        conv_key: &str,
        key_packages: Vec<js_sys::Uint8Array>,
    ) -> Result<CommitOut, JsError> {
        let mut kps: Vec<KeyPackage> = Vec::with_capacity(key_packages.len());
        for arr in &key_packages {
            let bytes = arr.to_vec();
            let (kp_in, _) = KeyPackageIn::tls_deserialize_bytes(&bytes).map_err(js_err)?;
            let kp = kp_in
                .validate(self.provider.crypto(), ProtocolVersion::Mls10)
                .map_err(js_err)?;
            kps.push(kp);
        }
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let (commit, welcome, _group_info) =
            group.add_members(&self.provider, signer, &kps).map_err(js_err)?;
        group.merge_pending_commit(&self.provider).map_err(js_err)?;

        let commit_bytes = commit.tls_serialize_detached().map_err(js_err)?;
        let welcome_bytes = welcome.tls_serialize_detached().map_err(js_err)?;
        let fp = Self::fingerprint(group);
        Ok(CommitOut {
            commit: commit_bytes,
            welcome: welcome_bytes,
            tree_hash: fp.tree_hash,
            transcript_hash: fp.transcript_hash,
            epoch: fp.epoch,
        })
    }

    /// EN: Like `addMembers`, but DO NOT merge — leaves the commit *staged* (pending) locally so a
    /// 1:1 Wire coordinator can decide based on the relay's `(conv, epoch)` CAS verdict: `mergePending`
    /// on ACCEPT, or `clearPending` (then adopt the winning commit) on EPOCH_STALE. Without staging,
    /// a lost race would have already force-merged a forked epoch with no way back (1:1 has no
    /// on-chain commit log). The returned `epoch`/fingerprint reflect the PRE-merge state.
    /// CN: 同 `addMembers`，但**不合并**——把 commit 本地保留为 *staged*（pending），使 1:1 Wire 协调设备
    /// 能依 relay 的 `(conv, epoch)` CAS 裁决决定：ACCEPT 则 `mergePending`，EPOCH_STALE 则 `clearPending`
    /// （再采纳胜出 commit）。若不暂存，落败时已强制合并出分叉 epoch 且无法回退（1:1 无链上 commit 日志）。
    /// 返回的 `epoch`/指纹为**合并前**状态。
    #[wasm_bindgen(js_name = addMembersStaged)]
    pub fn add_members_staged(
        &mut self,
        conv_key: &str,
        key_packages: Vec<js_sys::Uint8Array>,
    ) -> Result<CommitOut, JsError> {
        let mut kps: Vec<KeyPackage> = Vec::with_capacity(key_packages.len());
        for arr in &key_packages {
            let bytes = arr.to_vec();
            let (kp_in, _) = KeyPackageIn::tls_deserialize_bytes(&bytes).map_err(js_err)?;
            let kp = kp_in
                .validate(self.provider.crypto(), ProtocolVersion::Mls10)
                .map_err(js_err)?;
            kps.push(kp);
        }
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let (commit, welcome, _group_info) =
            group.add_members(&self.provider, signer, &kps).map_err(js_err)?;
        // NOTE: intentionally NO merge_pending_commit here — caller merges/clears explicitly.
        let commit_bytes = commit.tls_serialize_detached().map_err(js_err)?;
        let welcome_bytes = welcome.tls_serialize_detached().map_err(js_err)?;
        let fp = Self::fingerprint(group);
        Ok(CommitOut {
            commit: commit_bytes,
            welcome: welcome_bytes,
            tree_hash: fp.tree_hash,
            transcript_hash: fp.transcript_hash,
            epoch: fp.epoch,
        })
    }

    /// EN: Like `removeMembers`, but DO NOT merge — leaves the removal commit *staged* so the 1:1
    /// Wire coordinator can `mergePending` on ACCEPT or `clearPending` on EPOCH_STALE. Each
    /// `member_identities` entry must match exactly one leaf: full `account#deviceId`, or a bare
    /// account only when that account has a single leaf in the group (ambiguous account hints with
    /// multiple device leaves are rejected). CN: 同 `removeMembers`，但**不合并**——把移除 commit 保留为
    /// *staged*，使 1:1 Wire 协调设备在 ACCEPT 时 `mergePending`、EPOCH_STALE 时 `clearPending`。
    /// 每个 `member_identities` 项须**精确**匹配一个 leaf：完整 `account#deviceId`，或当该账户在群内仅
    /// 一个 leaf 时可写裸账户（多设备 leaf 时裸账户 hint 会因歧义被拒）。
    #[wasm_bindgen(js_name = removeMembersStaged)]
    pub fn remove_members_staged(
        &mut self,
        conv_key: &str,
        member_identities: Vec<String>,
    ) -> Result<CommitOut, JsError> {
        let indices = {
            let group = self
                .groups
                .get(conv_key)
                .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
            Self::resolve_leaf_indices(group, &member_identities)?
        };
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let (commit, welcome_opt, _group_info) = group
            .remove_members(&self.provider, signer, &indices)
            .map_err(js_err)?;
        // NOTE: intentionally NO merge_pending_commit here — caller merges/clears explicitly.
        let commit_bytes = commit.tls_serialize_detached().map_err(js_err)?;
        let welcome_bytes = welcome_opt
            .map(|w| w.tls_serialize_detached().map_err(js_err))
            .transpose()?
            .unwrap_or_default();
        let fp = Self::fingerprint(group);
        Ok(CommitOut {
            commit: commit_bytes,
            welcome: welcome_bytes,
            tree_hash: fp.tree_hash,
            transcript_hash: fp.transcript_hash,
            epoch: fp.epoch,
        })
    }

    /// EN: Self-update (rekey) the own leaf as a *staged* commit (no merge). Returns the commit
    /// bytes; rekey adds no member so there is no Welcome. Pairs with `mergePending`/`clearPending`
    /// exactly like `addMembersStaged`. CN: 把本设备 leaf 做一次自更新（rekey）为 *staged* commit
    /// （不合并）。返回 commit 字节；rekey 不加人故无 Welcome。与 `mergePending`/`clearPending` 配对，
    /// 用法同 `addMembersStaged`。
    #[wasm_bindgen(js_name = selfUpdateStaged)]
    pub fn self_update_staged(&mut self, conv_key: &str) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let bundle = group
            .self_update(&self.provider, signer, LeafNodeParameters::default())
            .map_err(js_err)?;
        bundle.into_commit().tls_serialize_detached().map_err(js_err)
    }

    /// EN: Merge the locally staged pending commit (call after the relay ACCEPTs the slot).
    /// CN: 合并本地暂存的 pending commit（relay ACCEPT 该槽位后调用）。
    #[wasm_bindgen(js_name = mergePending)]
    pub fn merge_pending(&mut self, conv_key: &str) -> Result<(), JsError> {
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        group.merge_pending_commit(&self.provider).map_err(js_err)
    }

    /// EN: Discard the locally staged pending commit (call on EPOCH_STALE before adopting the
    /// winning commit via `processCommit`). No-op if nothing is staged. CN: 丢弃本地暂存的 pending
    /// commit（EPOCH_STALE 时、经 `processCommit` 采纳胜出 commit **前**调用）。无暂存时为空操作。
    #[wasm_bindgen(js_name = clearPending)]
    pub fn clear_pending(&mut self, conv_key: &str) -> Result<(), JsError> {
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        group
            .clear_pending_commit(self.provider.storage())
            .map_err(js_err)
    }

    fn resolve_leaf_hint(group: &MlsGroup, hint: &str) -> Result<LeafNodeIndex, String> {
        let hint = hint.trim();
        if hint.is_empty() {
            return Err("empty member identity".into());
        }
        let direct = BasicCredential::new(hint.as_bytes().to_vec());
        if let Some(idx) = group.member_leaf_index(&direct.into()) {
            return Ok(idx);
        }
        let prefix = format!("{hint}#");
        let mut matches = Vec::new();
        for m in group.members() {
            let id = Self::credential_identity(&m.credential);
            if id == hint || id.starts_with(&prefix) {
                matches.push(m.index);
            }
        }
        match matches.len() {
            0 => Err(format!("member not in MLS group: {hint}")),
            1 => Ok(matches[0]),
            n => Err(format!(
                "ambiguous member identity `{hint}`: {n} leaves match; use account#deviceId"
            )),
        }
    }

    fn resolve_leaf_indices(
        group: &MlsGroup,
        identities: &[String],
    ) -> Result<Vec<LeafNodeIndex>, JsError> {
        let mut out = Vec::with_capacity(identities.len());
        for hint in identities {
            out.push(
                Self::resolve_leaf_hint(group, hint).map_err(|e| JsError::new(&e))?,
            );
        }
        Ok(out)
    }

    fn credential_identity(credential: &Credential) -> String {
        if credential.credential_type() == CredentialType::Basic {
            return String::from_utf8_lossy(credential.serialized_content()).into_owned();
        }
        String::new()
    }

    fn commit_out_from_welcome_commit(
        group: &MlsGroup,
        msgs: WelcomeCommitMessages,
    ) -> Result<CommitOut, JsError> {
        let commit_bytes = msgs.commit.tls_serialize_detached().map_err(js_err)?;
        let welcome_bytes = msgs.welcome.tls_serialize_detached().map_err(js_err)?;
        let fp = Self::fingerprint(group);
        Ok(CommitOut {
            commit: commit_bytes,
            welcome: welcome_bytes,
            tree_hash: fp.tree_hash,
            transcript_hash: fp.transcript_hash,
            epoch: fp.epoch,
        })
    }

    /// EN: Remove members by MLS identity (usually `account#deviceId`). Each hint must resolve to
    /// exactly one leaf; bare account hints are allowed only when that account has one leaf.
    /// CN: 按 MLS identity（通常为 `account#deviceId`）移除成员。每个 hint 须唯一对应一个 leaf；裸账户 hint
    /// 仅在该账户只有一个 leaf 时允许。
    #[wasm_bindgen(js_name = removeMembers)]
    pub fn remove_members(
        &mut self,
        conv_key: &str,
        member_identities: Vec<String>,
    ) -> Result<CommitOut, JsError> {
        let indices = {
            let group = self
                .groups
                .get(conv_key)
                .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
            Self::resolve_leaf_indices(group, &member_identities)?
        };
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let (commit, welcome_opt, _group_info) = group
            .remove_members(&self.provider, signer, &indices)
            .map_err(js_err)?;
        group.merge_pending_commit(&self.provider).map_err(js_err)?;
        let commit_bytes = commit.tls_serialize_detached().map_err(js_err)?;
        let welcome_bytes = welcome_opt
            .map(|w| w.tls_serialize_detached().map_err(js_err))
            .transpose()?
            .unwrap_or_default();
        let fp = Self::fingerprint(group);
        Ok(CommitOut {
            commit: commit_bytes,
            welcome: welcome_bytes,
            tree_hash: fp.tree_hash,
            transcript_hash: fp.transcript_hash,
            epoch: fp.epoch,
        })
    }

    /// EN: Swap members (remove + add in one commit). CN: 同一 commit 内替换成员。
    #[wasm_bindgen(js_name = swapMembers)]
    pub fn swap_members(
        &mut self,
        conv_key: &str,
        remove_identities: Vec<String>,
        key_packages: Vec<js_sys::Uint8Array>,
    ) -> Result<CommitOut, JsError> {
        if remove_identities.is_empty() {
            return Err(JsError::new("remove_identities must not be empty"));
        }
        if key_packages.is_empty() {
            return Err(JsError::new("key_packages must not be empty"));
        }
        if remove_identities.len() != key_packages.len() {
            return Err(JsError::new("remove/add count mismatch"));
        }
        let indices = {
            let group = self
                .groups
                .get(conv_key)
                .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
            Self::resolve_leaf_indices(group, &remove_identities)?
        };
        let mut kps: Vec<KeyPackage> = Vec::with_capacity(key_packages.len());
        for arr in &key_packages {
            let bytes = arr.to_vec();
            let (kp_in, _) = KeyPackageIn::tls_deserialize_bytes(&bytes).map_err(js_err)?;
            let kp = kp_in
                .validate(self.provider.crypto(), ProtocolVersion::Mls10)
                .map_err(js_err)?;
            kps.push(kp);
        }
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let msgs = group
            .swap_members(&self.provider, signer, &indices, &kps)
            .map_err(js_err)?;
        group.merge_pending_commit(&self.provider).map_err(js_err)?;
        Self::commit_out_from_welcome_commit(group, msgs)
    }

    /// EN: Join a group by processing a Welcome (先读后删 step 2). `conv_key` binds the
    /// resulting group to the frontend conversation id. CN: 处理 Welcome 入群（先读后删第2步）。
    #[wasm_bindgen(js_name = processWelcome)]
    pub fn process_welcome(&mut self, conv_key: &str, welcome: &[u8]) -> Result<(), JsError> {
        let (msg, _) = MlsMessageIn::tls_deserialize_bytes(welcome).map_err(js_err)?;
        let welcome = match msg.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err(JsError::new("not a Welcome message")),
        };
        let cfg = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        let staged = StagedWelcome::new_from_welcome(&self.provider, &cfg, welcome, None)
            .map_err(js_err)?;
        let group = staged.into_group(&self.provider).map_err(js_err)?;
        self.groups.insert(conv_key.to_string(), group);
        Ok(())
    }

    /// EN: Process an incoming Commit to a *staged* state WITHOUT merging, and return the E2EI
    /// device-leaf bindings (§3.9) of every leaf this Commit ADDS (empty = no Add). The staged commit is
    /// cached under `conv_key`; the caller MUST then either `processCommit` (same bytes → reuses the
    /// cache so the message is processed EXACTLY ONCE) once the bindings verify, or `discardIncomingCommit`
    /// to drop a Commit that admits an unverifiable leaf. This enables MEMBER-side re-verification: a
    /// follower independently confirms every added leaf is account-bound, not only the committer.
    /// CN: 把进入的 Commit 处理为 *staged*（**不合并**），返回该 Commit **新增**的每个 leaf 的 E2EI 设备 leaf
    /// 绑定（§3.9）（空＝无 Add）。staged commit 以 `conv_key` 缓存；调用方随后**必须**：绑定通过则 `processCommit`
    /// （同字节 → 复用缓存，使消息**只处理一次**），或对新增不可验证 leaf 的 Commit `discardIncomingCommit`。
    /// Re-inspecting the **same** commit bytes is idempotent; a **different** commit while one is cached
    /// is rejected (prevents silently dropping the prior staged commit). CN: 由此支持**成员侧复验**。
    /// **相同** commit 字节复 inspect 幂等；已有暂存时再 inspect **不同** commit 拒绝（避免静默丢弃先前暂存）。
    #[wasm_bindgen(js_name = inspectCommitBindings)]
    pub fn inspect_commit_bindings(
        &mut self,
        conv_key: &str,
        commit: &[u8],
    ) -> Result<Vec<KpBinding>, JsError> {
        self.inspect_commit_bindings_impl(conv_key, commit)
            .map_err(|e| JsError::new(&e))
    }

    /// EN: Drop a Commit staged by `inspectCommitBindings` without merging (call when an added leaf's
    /// E2EI binding fails to verify). Clears the cached `(commit, staged)` entry only — the live group
    /// epoch is unchanged because the staged commit was never merged. No-op if nothing is staged.
    /// NOTE: after `inspectCommitBindings` has run, OpenMLS has already processed the commit once;
    /// **do not** call `inspectCommitBindings` again with the same bytes after discard — use
    /// `processCommit` if verification passed, or wait for a fresh delivery. Re-inspecting a
    /// *different* commit is allowed once the slot is clear.
    /// CN: 丢弃 `inspectCommitBindings` 暂存的 Commit 而不合并。仅清除缓存；群 epoch 不变。无暂存为空操作。
    /// 注意：`inspectCommitBindings` 执行后 OpenMLS 已处理过该 commit 一次；discard 后**勿**用相同字节
    /// 再次 inspect——验证通过则用 `processCommit`，否则等待重新投递。槽位清空后可 inspect **不同** commit。
    #[wasm_bindgen(js_name = discardIncomingCommit)]
    pub fn discard_incoming_commit(&mut self, conv_key: &str) {
        self.staged_incoming.remove(conv_key);
    }

    /// EN: Apply a Commit to catch up an epoch (handshake log replay / §6).
    /// CN: 应用 Commit 补齐一个 epoch（握手日志回放 / §6）。
    #[wasm_bindgen(js_name = processCommit)]
    pub fn process_commit(&mut self, conv_key: &str, commit: &[u8]) -> Result<(), JsError> {
        // EN: if this exact Commit was already staged by `inspectCommitBindings` (member-side E2EI
        // re-verification), merge that staged commit so the message is processed EXACTLY ONCE; a stale
        // staged entry for a different Commit is dropped. CN: 若此 Commit 已被 `inspectCommitBindings`
        // 暂存（成员侧 E2EI 复验），合并该 staged commit 使消息**只处理一次**；不同 Commit 的过期暂存则丢弃。
        match self.staged_incoming.get(conv_key) {
            Some((bytes, _)) if bytes.as_slice() == commit => {
                let (_, staged) = self.staged_incoming.remove(conv_key).unwrap();
                let group = self
                    .groups
                    .get_mut(conv_key)
                    .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
                return group.merge_staged_commit(&self.provider, staged).map_err(js_err);
            }
            Some(_) => {
                self.staged_incoming.remove(conv_key);
            }
            None => {}
        }
        let (msg, _) = MlsMessageIn::tls_deserialize_bytes(commit).map_err(js_err)?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(js_err)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let processed = group.process_message(&self.provider, protocol).map_err(js_err)?;
        if let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() {
            group.merge_staged_commit(&self.provider, *staged).map_err(js_err)?;
            Ok(())
        } else {
            Err(JsError::new("not a Commit message"))
        }
    }

    /// EN: Encrypt an application payload (the P3 envelope bytes) for the group.
    /// CN: 为该群加密一条应用载荷（P3 信封字节）。
    pub fn encrypt(&mut self, conv_key: &str, plaintext: &[u8]) -> Result<Vec<u8>, JsError> {
        let signer = self.signer.as_ref().ok_or_else(no_signer)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let out = group
            .create_message(&self.provider, signer, plaintext)
            .map_err(js_err)?;
        out.tls_serialize_detached().map_err(js_err)
    }

    /// EN: Decrypt an inbound application message; returns the plaintext payload bytes.
    /// CN: 解密一条入站应用消息，返回明文载荷字节。
    pub fn decrypt(&mut self, conv_key: &str, ciphertext: &[u8]) -> Result<Vec<u8>, JsError> {
        let (msg, _) = MlsMessageIn::tls_deserialize_bytes(ciphertext).map_err(js_err)?;
        let protocol: ProtocolMessage = msg
            .try_into_protocol_message()
            .map_err(js_err)?;
        let group = self
            .groups
            .get_mut(conv_key)
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))?;
        let processed = group.process_message(&self.provider, protocol).map_err(js_err)?;
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
            _ => Err(JsError::new("not an application message")),
        }
    }

    /// EN: True if a live group is bound to this conversation id. CN: 该会话是否已有活跃群。
    #[wasm_bindgen(js_name = hasGroup)]
    pub fn has_group(&self, conv_key: &str) -> bool {
        self.groups.contains_key(conv_key)
    }

    /// EN: Drop a local group handle (e.g. after leave/kick). CN: 丢弃本地群句柄（退群/被踢后）。
    #[wasm_bindgen(js_name = forgetGroup)]
    pub fn forget_group(&mut self, conv_key: &str) {
        self.staged_incoming.remove(conv_key);
        self.groups.remove(conv_key);
    }

    /// EN: List conversation keys bound to live groups. CN: 列出已绑定活跃群的会话键。
    #[wasm_bindgen(js_name = listGroups)]
    pub fn list_groups(&self) -> Vec<String> {
        self.groups.keys().cloned().collect()
    }

    /// EN: Leaf credential identities (`account#deviceId`) of every member currently in the bound group,
    /// in ratchet-tree order. Read-only roster for the 1:1 Wire multi-leaf device-disclosure UX (§3.9 /
    /// design §8): the caller groups identities by account to show "how many devices each side has" and
    /// to pick a self device to remove (PCS self-heal). Every listed leaf was account-bound-verified at
    /// its add path (§3.9 induction), so membership here == an E2EI-verified device. Empty when no group.
    /// CN: 绑定群当前每个成员的 leaf 凭证身份（`account#deviceId`），按棘轮树序返回。供 1:1 Wire 多 leaf 的
    /// 设备披露 UX（§3.9 / 设计 §8）只读用：调用方按账户分组以展示「各方有几台设备」、并据此挑选要移除的本端
    /// 设备（PCS 自愈）。此处每个 leaf 均在其 add 路径上经账户绑定校验（§3.9 归纳），故在列即为 E2EI 已验证设备。
    /// 无群时返回空。
    #[wasm_bindgen(js_name = memberIdentities)]
    pub fn member_identities(&self, conv_key: &str) -> Vec<String> {
        match self.groups.get(conv_key) {
            Some(group) => group
                .members()
                .map(|m| Self::credential_identity(&m.credential))
                .collect(),
            None => Vec::new(),
        }
    }

    /// EN: Current MLS epoch of the bound group (for chain epoch catch-up). CN: 绑定群的
    /// 当前 MLS epoch（用于链上 epoch 补齐）。
    pub fn epoch(&self, conv_key: &str) -> Result<u64, JsError> {
        self.groups
            .get(conv_key)
            .map(|g| g.epoch().as_u64())
            .ok_or_else(|| JsError::new(&format!("no group for {conv_key}")))
    }

    /// EN: Fingerprint of the currently STAGED (pending, not-yet-merged) commit — the POST-commit
    /// `tree_hash` / `confirmed_transcript_hash` / epoch the chain `commit(new_tree_hash,
    /// new_transcript_hash)` must carry. Read from `StagedCommit::staged_context()` so a group Wire
    /// device op can submit the TRUE new-epoch commitments BEFORE the chain CAS verdict (no speculative
    /// merge). Errors when no commit is staged. Group-Wire G3b (CHAT_GROUP_WIREIFY_DESIGN §7.2).
    /// CN: 当前**已暂存**（pending、未合并）commit 的指纹——链上 `commit(new_tree_hash, new_transcript_hash)`
    /// 须携带的**后置** `tree_hash` / `confirmed_transcript_hash` / epoch。读自 `StagedCommit::staged_context()`，
    /// 使群 Wire 设备操作能在链 CAS 裁决**之前**提交**真实**新 epoch 承诺（无需投机合并）。无暂存 commit 时报错。
    /// 群 Wire G3b（设计 §7.2）。
    #[wasm_bindgen(js_name = stagedCommitFingerprint)]
    pub fn staged_commit_fingerprint(&self, conv_key: &str) -> Result<GroupFingerprint, JsError> {
        self.staged_commit_fingerprint_impl(conv_key)
            .map_err(|e| JsError::new(&e))
    }

    // ---- internals ----

    fn fingerprint(group: &MlsGroup) -> GroupFingerprint {
        // EN: tree_hash = SHA256(serialized ratchet tree); transcript_hash = epoch
        // authenticator (real 32B, stable per epoch). Chain stores these as opaque
        // fingerprints (it does not re-verify the crypto), so any stable values work.
        // CN: tree_hash=SHA256(序列化棘轮树)；transcript_hash=epoch authenticator(真实32字节)。
        // 链只把它们当不透明指纹存储（不复验密码学），故稳定值即可。
        let tree_hash = group
            .export_ratchet_tree()
            .tls_serialize_detached()
            .map(|b| sha256(&b))
            .unwrap_or_else(|_| vec![0u8; 32]);
        let transcript_hash = group.epoch_authenticator().as_slice().to_vec();
        GroupFingerprint { tree_hash, transcript_hash, epoch: group.epoch().as_u64() }
    }

    /// EN: Build a fingerprint from an MLS `GroupContext` — the native post-commit commitments
    /// (`tree_hash` + `confirmed_transcript_hash`, both 32B for SHA256 suites). Read from a STAGED
    /// commit's context (see `stagedCommitFingerprint`); this is exactly the context `merge` will
    /// install, so the value is the TRUE post-commit commitment shared by every member that applies the
    /// commit. CN: 由 MLS `GroupContext` 构造指纹——原生后置承诺（`tree_hash` + `confirmed_transcript_hash`，
    /// SHA256 套件均 32 字节）。读自**已暂存** commit 的 context（见 `stagedCommitFingerprint`）；这正是 `merge`
    /// 将安装的 context，故该值即所有应用此 commit 的成员共享的**真实**后置承诺。
    fn fingerprint_from_context(ctx: &GroupContext) -> GroupFingerprint {
        GroupFingerprint {
            tree_hash: ctx.tree_hash().to_vec(),
            transcript_hash: ctx.confirmed_transcript_hash().to_vec(),
            epoch: ctx.epoch().as_u64(),
        }
    }

    // EN: String-error core of `installSigningKeys` (§5.2 step 4). Split out so error paths are unit-
    // testable on native targets, where constructing a `JsError` panics. CN: `installSigningKeys`
    // 的 String 错误内核（§5.2 步骤 4）。拆出以便错误路径在原生目标可单测（原生构造 `JsError` 会 panic）。
    fn install_signing_keys_impl(&mut self, bundle: &[u8]) -> Result<(), String> {
        if self.signer.is_some() {
            return Err("client already holds a signing key".into());
        }
        let mut cur = Cursor { buf: bundle, pos: 0 };
        let public = cur.bytes().map_err(|_| "truncated signing-key bundle".to_string())?.to_vec();
        if public.as_slice() != self.credential.signature_key.as_slice() {
            return Err(
                "signing-key bundle public does not match this device's leaf identity".into(),
            );
        }
        let kv_count = cur.u32().map_err(|_| "truncated signing-key bundle".to_string())? as usize;
        let mut parsed: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(kv_count);
        for _ in 0..kv_count {
            let k = cur.bytes().map_err(|_| "truncated signing-key bundle".to_string())?.to_vec();
            let v = cur.bytes().map_err(|_| "truncated signing-key bundle".to_string())?.to_vec();
            parsed.push((k, v));
        }
        if cur.pos != cur.buf.len() {
            return Err("signing-key bundle has trailing bytes".into());
        }

        // EN: Probe-parse on a throwaway provider: the KV set must be EXACTLY the signature-key
        // storage entries — reject bundles that smuggle unrelated storage keys. CN: 在一次性 provider
        // 上探针解析：KV 集必须**恰好**是签名钥存储项——拒绝夹带无关 storage 键的 bundle。
        let probe = OpenMlsRustCrypto::default();
        {
            let mut values = probe
                .storage()
                .values
                .write()
                .map_err(|_| "probe storage poisoned".to_string())?;
            for (k, v) in &parsed {
                values.insert(k.clone(), v.clone());
            }
        }
        let signer = SignatureKeyPair::read(
            probe.storage(),
            &public,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or_else(|| "signing-key bundle did not yield a readable key pair".to_string())?;
        let allowed = signer_storage_keys(&signer)?;
        let parsed_keys: HashSet<Vec<u8>> = parsed.iter().map(|(k, _)| k.clone()).collect();
        if parsed_keys != allowed {
            return Err("signing-key bundle contains non-signer storage entries".into());
        }

        {
            let mut values = self
                .provider
                .storage()
                .values
                .write()
                .map_err(|_| "storage poisoned".to_string())?;
            for (k, v) in parsed {
                values.insert(k, v);
            }
        }
        let signer = SignatureKeyPair::read(
            self.provider.storage(),
            &public,
            CIPHERSUITE.signature_algorithm(),
        )
        .ok_or_else(|| "signing-key bundle did not yield a readable key pair".to_string())?;
        self.signer = Some(signer);
        Ok(())
    }
}

// EN: Native unit coverage for the Track A escrow codec (`exportEscrowState`/`restoreEscrow`).
// We build a real two-party group at the OpenMLS layer, wrap the snapshot device into an
// `MlsClient`, export the escrow blob (signature private key stripped), restore a read-only client,
// and prove it decrypts same-epoch backlog WITHOUT any signing key. Only success paths run here, so
// no `JsError` (a JS-interop type) is constructed off-wasm.
// CN: 路线 A 托管编解码（`exportEscrowState`/`restoreEscrow`）的原生单测。在 OpenMLS 层建真实双人群，
// 把快照设备包成 `MlsClient`，导出托管 blob（剔签名私钥）、恢复只读客户端，并证明其在**无签名钥**下解
// 同 epoch backlog。此处仅走成功路径，故不会在非 wasm 下构造 `JsError`（JS 互操作类型）。
#[cfg(test)]
mod escrow_tests {
    use super::*;
    use openmls_basic_credential::SignatureKeyPair;

    struct Party {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        cred: CredentialWithKey,
        identity: Vec<u8>,
    }

    fn party(name: &str) -> Party {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        let identity = name.as_bytes().to_vec();
        let cred = CredentialWithKey {
            credential: BasicCredential::new(identity.clone()).into(),
            signature_key: signer.public().into(),
        };
        Party { provider, signer, cred, identity }
    }

    #[test]
    fn escrow_export_restore_is_read_only_and_decrypts_backlog() {
        let alice = party("alice");
        let bob = party("bob");

        // alice creates the group and adds bob.
        let bob_kp = KeyPackage::builder()
            .build(CIPHERSUITE, &bob.provider, &bob.signer, bob.cred.clone())
            .unwrap();
        let create_cfg = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();
        let mut a_group =
            MlsGroup::new(&alice.provider, &alice.signer, &create_cfg, alice.cred.clone()).unwrap();
        let (_commit, welcome, _gi) = a_group
            .add_members(&alice.provider, &alice.signer, &[bob_kp.key_package().clone()])
            .unwrap();
        a_group.merge_pending_commit(&alice.provider).unwrap();

        // bob joins via the Welcome.
        let join_cfg = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        let welcome_in =
            match MlsMessageIn::tls_deserialize_bytes(&welcome.tls_serialize_detached().unwrap())
                .unwrap()
                .0
                .extract()
            {
                MlsMessageBodyIn::Welcome(w) => w,
                _ => panic!("not a welcome"),
            };
        let b_group = StagedWelcome::new_from_welcome(&bob.provider, &join_cfg, welcome_in, None)
            .unwrap()
            .into_group(&bob.provider)
            .unwrap();

        // alice sends two same-epoch backlog messages the snapshot device never consumes.
        let m0 = a_group
            .create_message(&alice.provider, &alice.signer, b"escrow-backlog-0")
            .unwrap()
            .tls_serialize_detached()
            .unwrap();
        let m1 = a_group
            .create_message(&alice.provider, &alice.signer, b"escrow-backlog-1")
            .unwrap()
            .tls_serialize_detached()
            .unwrap();

        // wrap bob (the snapshot device) into an MlsClient with a live signer.
        let mut groups = HashMap::new();
        groups.insert("g:test".to_string(), b_group);
        let bob_client = MlsClient {
            provider: bob.provider,
            signer: Some(bob.signer),
            credential: bob.cred,
            identity: bob.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        };
        assert!(!bob_client.is_read_only(), "full client retains its signer");

        // export escrow (no signature private key) → restore a read-only client.
        let blob = bob_client.export_escrow_state().unwrap();
        let mut ro = MlsClient::restore_escrow(&blob).unwrap();
        assert!(ro.is_read_only(), "restored escrow client must be read-only");
        assert_eq!(ro.list_groups(), vec!["g:test".to_string()]);

        // read path works WITHOUT any signature key (A1: same-epoch backlog via secret-tree root).
        assert_eq!(ro.decrypt("g:test", &m0).unwrap(), b"escrow-backlog-0");
        assert_eq!(ro.decrypt("g:test", &m1).unwrap(), b"escrow-backlog-1");

        // --- §5.2 online handoff steps 2/4: install the signing-key bundle into the read-only
        //     escrow device and prove it becomes an active sender alice can decrypt. ----------------
        let bundle = bob_client.export_signing_keys().unwrap();
        ro.install_signing_keys(&bundle).unwrap();
        assert!(!ro.is_read_only(), "after handoff the escrow device holds a signer");

        let handed = ro.encrypt("g:test", b"sent-after-handoff").unwrap();
        let processed = a_group
            .process_message(
                &alice.provider,
                MlsMessageIn::tls_deserialize_bytes(&handed)
                    .unwrap()
                    .0
                    .try_into_protocol_message()
                    .unwrap(),
            )
            .unwrap();
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                assert_eq!(app.into_bytes(), b"sent-after-handoff");
            }
            _ => panic!("expected application message from handed-off device"),
        }
    }

    #[test]
    fn install_signing_keys_rejects_foreign_bundle_and_double_install() {
        let alice = party("alice");
        let bob = party("bob");
        let create_cfg = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();
        let a_group =
            MlsGroup::new(&alice.provider, &alice.signer, &create_cfg, alice.cred.clone()).unwrap();

        let mut groups = HashMap::new();
        groups.insert("g:test".to_string(), a_group);
        let alice_client = MlsClient {
            provider: alice.provider,
            signer: Some(alice.signer),
            credential: alice.cred,
            identity: alice.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        };

        // a foreign (bob's) signing-key bundle must NOT graft onto alice's escrow leaf.
        let bob_provider = bob.provider;
        bob.signer.store(bob_provider.storage()).unwrap();
        let bob_client = MlsClient {
            provider: bob_provider,
            signer: Some(bob.signer),
            credential: bob.cred,
            identity: bob.identity,
            groups: HashMap::new(),
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        };
        let foreign = bob_client.export_signing_keys().unwrap();

        let escrow = alice_client.export_escrow_state().unwrap();
        let mut ro = MlsClient::restore_escrow(&escrow).unwrap();
        // use the String-error `_impl` (the wasm wrapper would build a JsError → panics natively).
        assert!(ro.install_signing_keys_impl(&foreign).is_err(), "foreign bundle rejected");
        assert!(ro.is_read_only(), "still read-only after rejected install");

        // the matching bundle installs, and a second install is rejected.
        let own = alice_client.export_signing_keys().unwrap();
        ro.install_signing_keys_impl(&own).unwrap();
        assert!(!ro.is_read_only());
        assert!(ro.install_signing_keys_impl(&own).is_err(), "double install rejected");
    }

    fn signing_bundle_with_extra_kv(bundle: &[u8], foreign_k: &[u8], foreign_v: &[u8]) -> Vec<u8> {
        let mut cur = Cursor { buf: bundle, pos: 0 };
        let public = cur.bytes().unwrap().to_vec();
        let count = cur.u32().unwrap() as usize;
        let mut kvs = Vec::with_capacity(count + 1);
        for _ in 0..count {
            kvs.push((cur.bytes().unwrap().to_vec(), cur.bytes().unwrap().to_vec()));
        }
        assert_eq!(cur.pos, cur.buf.len());
        kvs.push((foreign_k.to_vec(), foreign_v.to_vec()));
        let mut out = Vec::new();
        put_bytes(&mut out, &public);
        put_u32(&mut out, kvs.len() as u32);
        for (k, v) in kvs {
            put_bytes(&mut out, &k);
            put_bytes(&mut out, &v);
        }
        out
    }

    #[test]
    fn install_signing_keys_rejects_trailing_bytes_and_foreign_storage_keys() {
        let alice = party("alice");
        let create_cfg = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();
        let a_group =
            MlsGroup::new(&alice.provider, &alice.signer, &create_cfg, alice.cred.clone()).unwrap();

        let mut groups = HashMap::new();
        groups.insert("g:test".to_string(), a_group);
        let alice_client = MlsClient {
            provider: alice.provider,
            signer: Some(alice.signer),
            credential: alice.cred,
            identity: alice.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        };

        let own = alice_client.export_signing_keys().unwrap();
        let escrow = alice_client.export_escrow_state().unwrap();
        let mut ro = MlsClient::restore_escrow(&escrow).unwrap();

        let mut trailing = own.clone();
        trailing.push(0xff);
        assert!(
            ro.install_signing_keys_impl(&trailing).is_err(),
            "trailing bytes rejected"
        );

        let smuggled = signing_bundle_with_extra_kv(&own, b"evil-storage-key", b"evil-value");
        assert!(
            ro.install_signing_keys_impl(&smuggled).is_err(),
            "foreign storage KV rejected"
        );
        assert!(ro.is_read_only(), "still read-only after rejected smuggled install");

        ro.install_signing_keys_impl(&own).unwrap();
        assert!(!ro.is_read_only(), "valid bundle still installs after rejects");
    }
}

// EN: Native coverage for `createGroup` embedding the E2EI leaf binding on the creator leaf.
// CN: `createGroup` 在创建者 leaf 嵌入 E2EI leaf 绑定的原生覆盖。
#[cfg(test)]
mod create_group_binding_tests {
    use super::*;

    fn creator_binding_of(group: &MlsGroup) -> Option<Vec<u8>> {
        let leaf = group.own_leaf_node()?;
        leaf.extensions().iter().find_map(|e| match e {
            Extension::Unknown(t, UnknownExtension(data)) if *t == E2EI_LEAF_EXT_TYPE => {
                Some(data.clone())
            }
            _ => None,
        })
    }

    #[test]
    fn create_group_embeds_leaf_binding_on_creator_leaf() {
        let mut client = MlsClient::new("alice#dev1").unwrap();
        let blob = b"creator-binding".to_vec();
        client.set_leaf_binding(&blob);
        client.create_group("g:test").unwrap();

        let group = client.groups.get("g:test").expect("group bound");
        assert_eq!(
            creator_binding_of(group).as_deref(),
            Some(blob.as_slice()),
            "creator leaf must carry the installed E2EI binding",
        );
    }

    #[test]
    fn create_group_without_leaf_binding_has_no_creator_extension() {
        let mut client = MlsClient::new("bob#dev1").unwrap();
        client.create_group("g:test").unwrap();
        let group = client.groups.get("g:test").expect("group bound");
        assert!(
            creator_binding_of(group).is_none(),
            "no binding installed → creator leaf has no E2EI extension",
        );
    }
}

// EN: Versioned state snapshot codec + legacy compatibility. CN: 带版本状态快照编解码 + legacy 兼容。
#[cfg(test)]
mod state_snapshot_tests {
    use super::*;

    #[test]
    fn export_writes_ncms_v1_header() {
        let client = MlsClient::new("alice").unwrap();
        let blob = client.export_state().unwrap();
        assert_eq!(&blob[..STATE_MAGIC.len()], &STATE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(blob[STATE_MAGIC.len()..STATE_MAGIC.len() + 4].try_into().unwrap()),
            STATE_VERSION,
        );
    }

    #[test]
    fn v1_round_trip_restore_preserves_groups() {
        let mut client = MlsClient::new("alice#dev").unwrap();
        client.create_group("g:1").unwrap();
        let blob = client.export_state().unwrap();
        let restored = MlsClient::restore(&blob).unwrap();
        assert!(restored.has_group("g:1"));
    }

    #[test]
    fn legacy_blob_without_header_still_restores() {
        let mut client = MlsClient::new("alice").unwrap();
        client.create_group("g:legacy").unwrap();
        let v1 = client.export_state().unwrap();
        let legacy = v1[(STATE_MAGIC.len() + 4)..].to_vec();
        let restored = MlsClient::restore(&legacy).unwrap();
        assert!(restored.has_group("g:legacy"));
    }

    #[test]
    fn restore_rejects_trailing_bytes() {
        let client = MlsClient::new("alice").unwrap();
        let mut blob = client.export_state().unwrap();
        blob.push(0xff);
        assert!(try_parse_state_blob(&blob).is_err());
    }

    #[test]
    fn restore_rejects_unsupported_version() {
        let client = MlsClient::new("alice").unwrap();
        let mut blob = client.export_state().unwrap();
        blob[STATE_MAGIC.len()..STATE_MAGIC.len() + 4]
            .copy_from_slice(&2u32.to_le_bytes());
        assert!(try_parse_state_blob(&blob).is_err());
    }
}

// EN: `resolve_leaf_hint` matching rules (multi-device Wire). CN: `resolve_leaf_hint` 匹配规则（Wire 多设备）。
#[cfg(test)]
mod resolve_leaf_indices_tests {
    use super::*;
    use openmls_basic_credential::SignatureKeyPair;

    struct Party {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        cred: CredentialWithKey,
    }

    fn party(identity: &str) -> Party {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        let cred = CredentialWithKey {
            credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
            signature_key: signer.public().into(),
        };
        Party { provider, signer, cred }
    }

    fn create_cfg() -> MlsGroupCreateConfig {
        MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build()
    }

    fn join_cfg() -> MlsGroupJoinConfig {
        MlsGroupJoinConfig::builder().use_ratchet_tree_extension(true).build()
    }

    fn key_package(p: &Party) -> KeyPackage {
        KeyPackage::builder()
            .build(CIPHERSUITE, &p.provider, &p.signer, p.cred.clone())
            .unwrap()
            .key_package()
            .clone()
    }

    fn add_member(group: &mut MlsGroup, owner: &Party, kp: KeyPackage) {
        group
            .add_members(&owner.provider, &owner.signer, &[kp])
            .unwrap();
        group.merge_pending_commit(&owner.provider).unwrap();
    }

    fn join_from_welcome(joiner: &Party, welcome: &MlsMessageOut) -> MlsGroup {
        let welcome_in = match MlsMessageIn::tls_deserialize_bytes(
            &welcome.tls_serialize_detached().unwrap(),
        )
        .unwrap()
        .0
        .extract()
        {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => panic!("not a welcome"),
        };
        StagedWelcome::new_from_welcome(&joiner.provider, &join_cfg(), welcome_in, None)
            .unwrap()
            .into_group(&joiner.provider)
            .unwrap()
    }

    fn group_with_two_alice_devices() -> MlsGroup {
        let alice_a = party("alice#a");
        let alice_b = party("alice#b");
        let bob = party("bob#b1");
        let mut g = MlsGroup::new(&alice_a.provider, &alice_a.signer, &create_cfg(), alice_a.cred.clone())
            .unwrap();
        let (_c, w_bob) = {
            let (commit, welcome, _gi) =
                g.add_members(&alice_a.provider, &alice_a.signer, &[key_package(&bob)]).unwrap();
            g.merge_pending_commit(&alice_a.provider).unwrap();
            (commit, welcome)
        };
        let _ = join_from_welcome(&bob, &w_bob);
        add_member(&mut g, &alice_a, key_package(&alice_b));
        g
    }

    #[test]
    fn bare_account_hint_rejects_multiple_device_leaves() {
        let group = group_with_two_alice_devices();
        let err = MlsClient::resolve_leaf_hint(&group, "alice").unwrap_err();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(err.contains("account#deviceId"), "{err}");
    }

    #[test]
    fn device_distinct_hint_targets_one_leaf() {
        let group = group_with_two_alice_devices();
        assert!(MlsClient::resolve_leaf_hint(&group, "alice#a").is_ok());
        assert!(MlsClient::resolve_leaf_hint(&group, "alice#b").is_ok());
        assert!(MlsClient::resolve_leaf_hint(&group, "alice").is_err());
    }
}

// EN: Member-side `inspectCommitBindings` staging guard + `stagedCommitFingerprint` negative path.
// CN: 成员侧 `inspectCommitBindings` 暂存守卫 + `stagedCommitFingerprint` 负路径。
#[cfg(test)]
mod inspect_commit_bindings_tests {
    use super::*;
    use openmls_basic_credential::SignatureKeyPair;

    const CONV: &str = "g:inspect";

    struct Party {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        cred: CredentialWithKey,
        identity: Vec<u8>,
    }

    fn party(name: &str) -> Party {
        let provider = OpenMlsRustCrypto::default();
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        signer.store(provider.storage()).unwrap();
        let identity = name.as_bytes().to_vec();
        let cred = CredentialWithKey {
            credential: BasicCredential::new(identity.clone()).into(),
            signature_key: signer.public().into(),
        };
        Party { provider, signer, cred, identity }
    }

    fn create_cfg() -> MlsGroupCreateConfig {
        MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build()
    }

    fn join_cfg() -> MlsGroupJoinConfig {
        MlsGroupJoinConfig::builder().use_ratchet_tree_extension(true).build()
    }

    fn key_package(p: &Party) -> KeyPackage {
        KeyPackage::builder()
            .build(CIPHERSUITE, &p.provider, &p.signer, p.cred.clone())
            .unwrap()
            .key_package()
            .clone()
    }

    fn join_from_welcome(joiner: &Party, welcome: &MlsMessageOut) -> MlsGroup {
        let welcome_in = match MlsMessageIn::tls_deserialize_bytes(
            &welcome.tls_serialize_detached().unwrap(),
        )
        .unwrap()
        .0
        .extract()
        {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => panic!("not a welcome"),
        };
        StagedWelcome::new_from_welcome(&joiner.provider, &join_cfg(), welcome_in, None)
            .unwrap()
            .into_group(&joiner.provider)
            .unwrap()
    }

    /// EN: Bob joined a group; Alice has already merged a self-update commit Bob has NOT applied yet.
    /// CN: Bob 已入群；Alice 已合并 self-update commit，Bob 尚未应用。
    fn bob_client_before_catchup() -> (MlsClient, Party, Vec<u8>) {
        let alice = party("alice");
        let bob = party("bob");
        let mut a_group =
            MlsGroup::new(&alice.provider, &alice.signer, &create_cfg(), alice.cred.clone())
                .unwrap();
        let (_c, welcome, _gi) = a_group
            .add_members(&alice.provider, &alice.signer, &[key_package(&bob)])
            .unwrap();
        a_group.merge_pending_commit(&alice.provider).unwrap();
        let b_group = join_from_welcome(&bob, &welcome);

        let bundle = a_group
            .self_update(
                &alice.provider,
                &alice.signer,
                LeafNodeParameters::default(),
            )
            .unwrap();
        let commit_wire = bundle.commit().tls_serialize_detached().unwrap();
        a_group.merge_pending_commit(&alice.provider).unwrap();

        let mut groups = HashMap::new();
        groups.insert(CONV.to_string(), b_group);
        let bob_client = MlsClient {
            provider: bob.provider,
            signer: Some(bob.signer),
            credential: bob.cred,
            identity: bob.identity,
            groups,
            leaf_binding: None,
            staged_incoming: HashMap::new(),
        };
        (bob_client, alice, commit_wire)
    }

    #[test]
    fn inspect_is_idempotent_for_the_same_commit_bytes() {
        let (mut bob, _alice, commit) = bob_client_before_catchup();
        let first = bob.inspect_commit_bindings_impl(CONV, &commit).unwrap();
        let second = bob.inspect_commit_bindings_impl(CONV, &commit).unwrap();
        assert_eq!(first.len(), second.len());
        assert!(bob.staged_incoming.contains_key(CONV));
    }

    #[test]
    fn inspect_rejects_a_second_different_commit_while_one_is_staged() {
        let (mut bob, _alice, commit) = bob_client_before_catchup();
        bob.inspect_commit_bindings_impl(CONV, &commit).unwrap();
        assert!(
            bob.inspect_commit_bindings_impl(CONV, b"different-commit-bytes").is_err(),
            "second different commit must be rejected while one is staged"
        );
    }

    #[test]
    fn discard_clears_staged_incoming_slot() {
        let (mut bob, _alice, commit) = bob_client_before_catchup();
        bob.inspect_commit_bindings_impl(CONV, &commit).unwrap();
        bob.discard_incoming_commit(CONV);
        assert!(!bob.staged_incoming.contains_key(CONV));
    }

    #[test]
    fn forget_group_clears_staged_incoming() {
        let (mut bob, _alice, commit) = bob_client_before_catchup();
        bob.inspect_commit_bindings_impl(CONV, &commit).unwrap();
        bob.forget_group(CONV);
        assert!(!bob.staged_incoming.contains_key(CONV));
    }

    #[test]
    fn staged_fingerprint_errors_without_local_pending_commit() {
        let (bob, _, _) = bob_client_before_catchup();
        assert!(
            bob.staged_commit_fingerprint_impl(CONV).is_err(),
            "follower with no local pending commit must error"
        );
    }

    #[test]
    fn staged_fingerprint_reads_local_pending_self_update() {
        let mut client = MlsClient::new("alice#dev").unwrap();
        client.create_group(CONV).unwrap();
        client.self_update_staged(CONV).unwrap();
        let fp = client.staged_commit_fingerprint_impl(CONV).unwrap();
        assert_eq!(fp.epoch, 1);
        assert_eq!(fp.tree_hash.len(), 32);
    }
}
