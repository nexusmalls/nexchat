// EN: On-disk types for the relay snapshot — byte-compatible with relay-persistence.mjs.
// The snapshot JSON field names mix camelCase (maps) and snake_case (`saved_at`), so each
// field is renamed explicitly. Mailbox rows are opaque `Value`s (their shape varies by
// message type); persistence stores/loads them verbatim.
// CN: relay 快照的磁盘类型——与 relay-persistence.mjs 逐字节兼容。快照 JSON 字段名混用
// camelCase（各 map）与 snake_case（`saved_at`），故逐字段显式 rename。邮箱行是不透明
// `Value`（形态随消息类型而变），持久化原样存取。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// EN/CN: snapshot schema version (`relay-persistence.mjs` SCHEMA_V).
pub const SCHEMA_V: u32 = 1;

/// EN: Cloud-sync pointer (`account -> {cid, updated_at}`). CN: 云同步指针。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pointer {
    pub cid: String,
    pub updated_at: u64,
}

/// EN: RFC 9474 inbox registration record. CN: RFC 9474 信箱注册记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxRecord {
    pub inbox_id: String,
    #[serde(default)]
    pub epoch: u64,
    #[serde(default)]
    pub ipk_n: String,
    #[serde(default)]
    pub ipk_e: String,
    #[serde(default)]
    pub revoked_tags: Vec<String>,
}

/// EN: Contact mailbox (`reqs`/`acks` keyed by reqId). CN: 联系人邮箱。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactBox {
    #[serde(default)]
    pub reqs: BTreeMap<String, Value>,
    #[serde(default)]
    pub acks: BTreeMap<String, Value>,
}

/// EN: Full relay snapshot DTO (`relay-state.json`). Field order matches
/// `relay-persistence.mjs::snapshot()`; `spentByInbox` is `{inboxId: [token...]}`.
/// CN: relay 快照 DTO（`relay-state.json`）。字段顺序对齐 `snapshot()`；spent 为数组。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub v: u32,
    pub saved_at: u64,
    #[serde(rename = "indexPointers", default)]
    pub index_pointers: BTreeMap<String, Pointer>,
    #[serde(rename = "contactsPointers", default)]
    pub contacts_pointers: BTreeMap<String, Pointer>,
    #[serde(rename = "msgArchivePointers", default)]
    pub msg_archive_pointers: BTreeMap<String, Pointer>,
    // EN: Track A MLS escrow-vault pointers; `default` keeps old snapshots loadable and matches the
    // Node relay's JSON key for cross-impl snapshot parity. CN: 路线 A MLS 托管 vault 指针；`default`
    // 兼容旧快照，并与 Node relay 的 JSON 键一致以保证跨实现快照互通。
    #[serde(rename = "mlsVaultPointers", default)]
    pub mls_vault_pointers: BTreeMap<String, Pointer>,
    // EN: Track A sending-authority handoff pointer; `default` for old-snapshot compat + Node parity.
    // CN: 路线 A 发送权交接指针；`default` 兼容旧快照并与 Node relay 互通。
    #[serde(rename = "handoffPointers", default)]
    pub handoff_pointers: BTreeMap<String, Pointer>,
    // EN: Track A PIN-wrapped signing-key backup pointers; `default` for old-snapshot compat. CN: 路线 A PIN 包裹签名钥备份指针；`default` 兼容旧快照。
    #[serde(rename = "mlsSigningPointers", default)]
    pub mls_signing_pointers: BTreeMap<String, Pointer>,
    #[serde(rename = "inboxesByAccount", default)]
    pub inboxes_by_account: BTreeMap<String, InboxRecord>,
    #[serde(rename = "spentByInbox", default)]
    pub spent_by_inbox: BTreeMap<String, Vec<String>>,
    #[serde(rename = "contactMailbox", default)]
    pub contact_mailbox: BTreeMap<String, ContactBox>,
    #[serde(rename = "groupInviteMailbox", default)]
    pub group_invite_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(rename = "mlsMailbox", default)]
    pub mls_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(rename = "chatMailbox", default)]
    pub chat_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
}
