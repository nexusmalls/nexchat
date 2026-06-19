// EN: WAL journal ops — byte-compatible with relay-persistence.mjs journal entries.
// Each ndjson line is `{op, ...payload, at}`; the `at` timestamp is appended at write time
// and ignored by the tagged enum on replay. Mirrors `applyJournalEntry`.
// CN: WAL 日志 op——与 relay-persistence.mjs 的 journal 条目逐字节兼容。每行 ndjson 为
// `{op, ...payload, at}`；`at` 写入时追加，重放时被 tagged enum 忽略。对应 applyJournalEntry。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::persistence::{add_spent_token, PersistState};
use crate::types::{InboxRecord, Pointer};
use std::collections::BTreeMap;

/// EN: Durable mutation recorded to the WAL. CN: 记入 WAL 的持久化变更。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum JournalOp {
    #[serde(rename = "index_put")]
    IndexPut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    #[serde(rename = "contacts_put")]
    ContactsPut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    #[serde(rename = "msg_archive_put")]
    MsgArchivePut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    // EN: Track A MLS escrow-vault pointer put (design §4/§13). CN: 路线 A MLS 托管 vault 指针写入（设计 §4/§13）。
    #[serde(rename = "mls_vault_put")]
    MlsVaultPut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    // EN: Track A sending-authority handoff pointer put (design §5.2). CN: 路线 A 发送权交接指针写入（设计 §5.2）。
    #[serde(rename = "handoff_put")]
    HandoffPut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    // EN: Track A PIN-wrapped signing-key backup pointer put (design §5.3 path C). CN: 路线 A PIN 包裹签名钥备份指针写入（设计 §5.3 路径 C）。
    #[serde(rename = "mls_signing_put")]
    MlsSigningPut {
        account: String,
        cid: String,
        updated_at: u64,
    },
    #[serde(rename = "inbox_register")]
    InboxRegister {
        account: String,
        inbox_id: String,
        #[serde(default)]
        epoch: u64,
        #[serde(default)]
        ipk_n: String,
        #[serde(default)]
        ipk_e: String,
        #[serde(default)]
        revoked_tags: Vec<String>,
    },
    #[serde(rename = "spent_add")]
    SpentAdd { inbox_id: String, t: String },
    #[serde(rename = "spent_clear")]
    SpentClear { inbox_id: String },
    /// EN: Offline chat mailbox frame (store-and-forward ciphertext). CN: 离线聊天邮箱帧（密文 store-and-forward）。
    #[serde(rename = "chat_store")]
    ChatStore {
        account: String,
        dedup_key: String,
        row: Value,
    },
}

/// EN: LWW pointer write (`applyPointerPut`): skip empty/zero; keep newer-or-equal.
/// CN: LWW 指针写入：跳过空/零；保留更新或相等者。
pub(crate) fn apply_pointer_put(
    map: &mut BTreeMap<String, Pointer>,
    account: &str,
    cid: &str,
    updated_at: u64,
) {
    if account.is_empty() || cid.is_empty() || updated_at == 0 {
        return;
    }
    match map.get(account) {
        Some(prev) if updated_at < prev.updated_at => {}
        _ => {
            map.insert(
                account.to_string(),
                Pointer {
                    cid: cid.to_string(),
                    updated_at,
                },
            );
        }
    }
}

/// EN: Apply one journal op to in-memory state. CN: 把一条 journal op 应用到内存态。
pub fn apply_journal_op(state: &mut PersistState, op: &JournalOp, spent_cap: usize) {
    match op {
        JournalOp::IndexPut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.index_pointers, account, cid, *updated_at),
        JournalOp::ContactsPut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.contacts_pointers, account, cid, *updated_at),
        JournalOp::MsgArchivePut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.msg_archive_pointers, account, cid, *updated_at),
        JournalOp::MlsVaultPut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.mls_vault_pointers, account, cid, *updated_at),
        JournalOp::HandoffPut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.handoff_pointers, account, cid, *updated_at),
        JournalOp::MlsSigningPut {
            account,
            cid,
            updated_at,
        } => apply_pointer_put(&mut state.mls_signing_pointers, account, cid, *updated_at),
        JournalOp::InboxRegister {
            account,
            inbox_id,
            epoch,
            ipk_n,
            ipk_e,
            revoked_tags,
        } => {
            state.inboxes_by_account.insert(
                account.clone(),
                InboxRecord {
                    inbox_id: inbox_id.clone(),
                    epoch: *epoch,
                    ipk_n: ipk_n.clone(),
                    ipk_e: ipk_e.clone(),
                    revoked_tags: revoked_tags.clone(),
                },
            );
        }
        JournalOp::SpentAdd { inbox_id, t } => {
            add_spent_token(state, inbox_id, t, spent_cap);
        }
        JournalOp::SpentClear { inbox_id } => {
            state.spent_by_inbox.remove(inbox_id);
        }
        JournalOp::ChatStore {
            account,
            dedup_key,
            row,
        } => {
            if account.is_empty() || dedup_key.is_empty() {
                return;
            }
            let b = state.chat_mailbox.entry(account.clone()).or_default();
            if !b.contains_key(dedup_key) {
                b.insert(dedup_key.clone(), row.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistState;

    #[test]
    fn chat_store_replay_inserts_once() {
        let mut state = PersistState::default();
        let row = serde_json::json!({
            "dedupKey": "d:a:b:m1",
            "convId": "d:a:b",
            "ciphertextB64": "AQID",
            "stored_at": 100,
            "bytes": 42
        });
        let op = JournalOp::ChatStore {
            account: "5Bob".into(),
            dedup_key: "d:a:b:m1".into(),
            row: row.clone(),
        };
        apply_journal_op(&mut state, &op, 50_000);
        apply_journal_op(&mut state, &op, 50_000);
        assert_eq!(state.chat_mailbox.get("5Bob").unwrap().len(), 1);
        assert_eq!(
            state.chat_mailbox["5Bob"]["d:a:b:m1"]["ciphertextB64"],
            "AQID"
        );
    }
}
