//! EN: nexchat-dr — vodozemac (Olm) client engine compiled to WASM for the
//! decentralized 1:1 stack (X3DH + Double Ratchet). Exposes a single `DrClient` handle
//! to JS that owns one Olm `Account` (long-term identity + prekeys) and a map of live
//! `Session`s keyed by the **peer device id hex** (`blake2_128(peer_IK)`), i.e. one
//! independent Double Ratchet per peer device (design §8 "Scheme A"). The mapping of
//! vodozemac primitives onto the X3DH design is documented in §17.1:
//! - Olm `Account.curve25519_key`  ⇄ device identity key `IK`
//! - Olm one-time keys             ⇄ one-time prekeys `OPK`
//! - Olm fallback key              ⇄ signed prekey `SPK` (OPK-exhaustion fallback)
//! - Olm `PreKeyMessage`           ⇄ X3DH `dm_init` body (`DmKind::Init`)
//! - Olm `Message`                 ⇄ ratchet `dm_msg` body (`DmKind::Msg`)
//!
//! Account-key *endorsements* (sr25519 over `CTX ‖ key`) and the on-chain prekey
//! publication are done in TS / the chain — this crate performs only Olm crypto.
//!
//! CN: nexchat-dr —— 把 vodozemac（Olm）客户端引擎编译为 WASM，供去中心化 1:1 栈
//! （X3DH + 双棘轮）使用。向 JS 暴露单一 `DrClient` 句柄：持有一个 Olm `Account`（长期身份
//! + 预密钥）与一张以**对端设备 id 十六进制**（`blake2_128(对端_IK)`）为键的活跃 `Session`
//! 表，即每对端设备一条独立双棘轮（设计 §8「方案 A」）。vodozemac 原语到 X3DH 设计的映射见
//! §17.1（见上）。账户钥*背书*（sr25519 对 `CTX ‖ key` 签名）与链上预密钥发布在 TS / 链上
//! 完成——本 crate 只做 Olm 密码学。

use std::collections::HashMap;

use vodozemac::olm::{
    Account, AccountPickle, InboundCreationResult, OlmMessage, Session, SessionConfig,
    SessionPickle,
};
use vodozemac::Curve25519PublicKey;
use wasm_bindgen::prelude::*;

/// EN: Map any `Debug` error to a `JsError`. CN: 把任意 `Debug` 错误映射为 `JsError`。
fn js_err<E: core::fmt::Debug>(e: E) -> JsError {
    JsError::new(&format!("{e:?}"))
}

/// EN: Parse a 32-byte Curve25519 public key. CN: 解析 32 字节 Curve25519 公钥。
fn parse_curve(bytes: &[u8]) -> Result<Curve25519PublicKey, JsError> {
    Curve25519PublicKey::from_slice(bytes).map_err(js_err)
}

/// EN: Result of establishing an inbound session from a `dm_init` body: the decrypted
/// first plaintext and the sender's recovered identity key (caller checks
/// `blake2_128(identity_key) == sender_dev`). CN: 由 `dm_init` 体建立入站会话的结果：
/// 解密出的首条明文 + 还原的发送方身份钥（调用方校验 `blake2_128(identity_key)==sender_dev`）。
#[wasm_bindgen(getter_with_clone)]
pub struct DrInbound {
    /// EN: Decrypted first plaintext. CN: 解密出的首条明文。
    pub plaintext: Vec<u8>,
    /// EN: Sender's Curve25519 identity key (32 bytes). CN: 发送方 Curve25519 身份钥（32 字节）。
    pub identity_key: Vec<u8>,
}

/// EN: WASM handle owning one Olm account and its per-peer-device sessions.
/// CN: 持有一个 Olm 账户及其每对端设备会话的 WASM 句柄。
#[wasm_bindgen]
pub struct DrClient {
    account: Account,
    /// Key = peer device id hex (blake2_128(peer IK)). / 键 = 对端设备 id 十六进制。
    sessions: HashMap<String, Session>,
}

#[wasm_bindgen]
impl DrClient {
    /// EN: Create a fresh Olm account (new device identity). CN: 新建 Olm 账户（新设备身份）。
    #[wasm_bindgen(constructor)]
    pub fn new() -> DrClient {
        console_error_panic_hook::set_once();
        DrClient { account: Account::new(), sessions: HashMap::new() }
    }

    /// EN: This device's Curve25519 identity public key (`IK`, 32 bytes). The on-chain
    /// `device_id` is `blake2_128(IK)`. CN: 本设备 Curve25519 身份公钥（`IK`，32 字节）。
    /// 链上 `device_id` 即 `blake2_128(IK)`。
    #[wasm_bindgen(js_name = identityKey)]
    pub fn identity_key(&self) -> Vec<u8> {
        self.account.curve25519_key().to_bytes().to_vec()
    }

    /// EN: This device's Ed25519 public key (Olm signing key; distinct from the account
    /// sr25519 key). CN: 本设备 Ed25519 公钥（Olm 签名钥；区别于账户 sr25519 钥）。
    #[wasm_bindgen(js_name = ed25519Key)]
    pub fn ed25519_key(&self) -> Vec<u8> {
        self.account.ed25519_key().as_bytes().to_vec()
    }

    /// EN: Generate `count` new one-time prekeys (`OPK`). Read them via `oneTimeKeys()`
    /// and publish (Merkle root on-chain + leaves to relay), then `markKeysAsPublished()`.
    /// CN: 生成 `count` 个新一次性预密钥（`OPK`）。经 `oneTimeKeys()` 读取并发布（链上 Merkle
    /// 根 + 叶子给 relay），随后 `markKeysAsPublished()`。
    #[wasm_bindgen(js_name = generateOneTimeKeys)]
    pub fn generate_one_time_keys(&mut self, count: usize) {
        self.account.generate_one_time_keys(count);
    }

    /// EN: Current unpublished one-time prekeys, concatenated as `n × 32` bytes (each a
    /// Curve25519 public key). CN: 当前未发布的一次性预密钥，按 `n × 32` 字节拼接（每个为
    /// Curve25519 公钥）。
    #[wasm_bindgen(js_name = oneTimeKeys)]
    pub fn one_time_keys(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for key in self.account.one_time_keys().values() {
            out.extend_from_slice(&key.to_bytes());
        }
        out
    }

    /// EN: Generate a fresh fallback key (maps to the signed prekey `SPK`). CN: 生成新的
    /// 回退钥（映射到签名预密钥 `SPK`）。
    #[wasm_bindgen(js_name = generateFallbackKey)]
    pub fn generate_fallback_key(&mut self) {
        self.account.generate_fallback_key();
    }

    /// EN: Current fallback key (`SPK`, 32 bytes) if any. CN: 当前回退钥（`SPK`，32 字节，若有）。
    #[wasm_bindgen(js_name = fallbackKey)]
    pub fn fallback_key(&self) -> Option<Vec<u8>> {
        self.account.fallback_key().into_values().next().map(|k| k.to_bytes().to_vec())
    }

    /// EN: Mark all currently unpublished one-time / fallback keys as published. Call
    /// after a successful on-chain `set_opk_root` / `set_signed_prekey`. CN: 把当前所有
    /// 未发布的一次性 / 回退钥标记为已发布。链上 `set_opk_root` / `set_signed_prekey` 成功后调用。
    #[wasm_bindgen(js_name = markKeysAsPublished)]
    pub fn mark_keys_as_published(&mut self) {
        self.account.mark_keys_as_published();
    }

    /// EN: X3DH initiator: create an outbound session to a peer device using its `IK`
    /// and one prekey (`OPK` if available, else its `SPK`). Keyed by `peer_device` hex.
    /// CN: X3DH 发起方：用对端设备的 `IK` 与一条预密钥（有 `OPK` 用之，否则用 `SPK`）建立出站
    /// 会话，以 `peer_device` 十六进制为键。
    #[wasm_bindgen(js_name = createOutboundSession)]
    pub fn create_outbound_session(
        &mut self,
        peer_device: &str,
        their_ik: &[u8],
        their_prekey: &[u8],
    ) -> Result<(), JsError> {
        let ik = parse_curve(their_ik)?;
        let prekey = parse_curve(their_prekey)?;
        let session = self
            .account
            .create_outbound_session(SessionConfig::version_1(), ik, prekey)
            .map_err(js_err)?;
        self.sessions.insert(peer_device.to_string(), session);
        Ok(())
    }

    /// EN: X3DH responder: build an inbound session from a received `dm_init` body (an
    /// Olm PreKeyMessage), consuming the matching `OPK`. Returns the first plaintext and
    /// the sender's recovered `IK`. Keyed by `peer_device` hex (caller MUST check it
    /// equals `blake2_128(identity_key)`). CN: X3DH 应答方：由收到的 `dm_init` 体（Olm
    /// PreKeyMessage）建立入站会话，消费匹配的 `OPK`。返回首条明文与还原的发送方 `IK`。
    /// 以 `peer_device` 十六进制为键（调用方必须校验其等于 `blake2_128(identity_key)`）。
    #[wasm_bindgen(js_name = createInboundSession)]
    pub fn create_inbound_session(
        &mut self,
        peer_device: &str,
        prekey_body: &[u8],
    ) -> Result<DrInbound, JsError> {
        let msg = OlmMessage::from_parts(0, prekey_body).map_err(js_err)?;
        let prekey = match msg {
            OlmMessage::PreKey(m) => m,
            OlmMessage::Normal(_) => {
                return Err(JsError::new("createInboundSession: not a pre-key message"))
            }
        };
        let their_ik = prekey.identity_key();
        let InboundCreationResult { session, plaintext } = self
            .account
            .create_inbound_session(SessionConfig::version_1(), their_ik, &prekey)
            .map_err(js_err)?;
        self.sessions.insert(peer_device.to_string(), session);
        Ok(DrInbound { plaintext, identity_key: their_ik.to_bytes().to_vec() })
    }

    /// EN: Encrypt `plaintext` on the session for `peer_device`. Returns `[msg_type:u8]
    /// ‖ body`, where `msg_type` is `0` (PreKey → `DmKind::Init`) or `1` (Normal →
    /// `DmKind::Msg`). CN: 在 `peer_device` 的会话上加密 `plaintext`。返回 `[msg_type:u8] ‖
    /// body`，`msg_type` 为 `0`（PreKey → `DmKind::Init`）或 `1`（Normal → `DmKind::Msg`）。
    #[wasm_bindgen(js_name = encrypt)]
    pub fn encrypt(&mut self, peer_device: &str, plaintext: &[u8]) -> Result<Vec<u8>, JsError> {
        let session = self
            .sessions
            .get_mut(peer_device)
            .ok_or_else(|| JsError::new("encrypt: no session for peer device"))?;
        let msg = session.encrypt(plaintext).map_err(js_err)?;
        let (mtype, body) = msg.to_parts();
        let mut out = Vec::with_capacity(1 + body.len());
        out.push(mtype as u8);
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// EN: Decrypt a `dm_msg`/`dm_init` body on the session for `peer_device`. `msg_type`
    /// is `0` (PreKey/`Init`) or `1` (Normal/`Msg`). CN: 在 `peer_device` 的会话上解密
    /// `dm_msg`/`dm_init` 体。`msg_type` 为 `0`（PreKey/`Init`）或 `1`（Normal/`Msg`）。
    #[wasm_bindgen(js_name = decrypt)]
    pub fn decrypt(
        &mut self,
        peer_device: &str,
        msg_type: u8,
        body: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        let session = self
            .sessions
            .get_mut(peer_device)
            .ok_or_else(|| JsError::new("decrypt: no session for peer device"))?;
        let msg = OlmMessage::from_parts(msg_type as usize, body).map_err(js_err)?;
        session.decrypt(&msg).map_err(js_err)
    }

    /// EN: Whether a live session exists for `peer_device`. CN: `peer_device` 是否有活跃会话。
    #[wasm_bindgen(js_name = hasSession)]
    pub fn has_session(&self, peer_device: &str) -> bool {
        self.sessions.contains_key(peer_device)
    }

    /// EN: Serialize the account (JSON pickle). At-rest encryption is the TS store's job
    /// (vault-derived key). CN: 序列化账户（JSON pickle）。落盘加密由 TS 存储层负责（vault 派生钥）。
    #[wasm_bindgen(js_name = pickle)]
    pub fn pickle(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.account.pickle()).map_err(js_err)
    }

    /// EN: Restore an account from a JSON pickle (sessions are reloaded separately via
    /// `loadSession`). CN: 由 JSON pickle 恢复账户（会话经 `loadSession` 单独重载）。
    #[wasm_bindgen(js_name = restore)]
    pub fn restore(pickle: &str) -> Result<DrClient, JsError> {
        let p: AccountPickle = serde_json::from_str(pickle).map_err(js_err)?;
        Ok(DrClient { account: Account::from_pickle(p), sessions: HashMap::new() })
    }

    /// EN: Serialize the session for `peer_device` (JSON pickle). CN: 序列化 `peer_device`
    /// 的会话（JSON pickle）。
    #[wasm_bindgen(js_name = pickleSession)]
    pub fn pickle_session(&self, peer_device: &str) -> Result<String, JsError> {
        let s = self
            .sessions
            .get(peer_device)
            .ok_or_else(|| JsError::new("pickleSession: no session for peer device"))?;
        serde_json::to_string(&s.pickle()).map_err(js_err)
    }

    /// EN: Load a session for `peer_device` from a JSON pickle. CN: 由 JSON pickle 为
    /// `peer_device` 载入会话。
    #[wasm_bindgen(js_name = loadSession)]
    pub fn load_session(&mut self, peer_device: &str, pickle: &str) -> Result<(), JsError> {
        let p: SessionPickle = serde_json::from_str(pickle).map_err(js_err)?;
        self.sessions.insert(peer_device.to_string(), Session::from_pickle(p));
        Ok(())
    }
}
