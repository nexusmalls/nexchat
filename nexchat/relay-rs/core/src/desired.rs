// EN: Dual-track derivation of the pinner's desired pointer set (ADR §5.8) — byte-faithful
// port of relay-pinner.mjs::collectDesiredPointers. Re-derives the full desired set every
// tick from the snapshot JSON + journal text (per-slot LWW), immune to journal truncation.
// CN: pinner 期望指针集的双轨推导（ADR §5.8）——relay-pinner.mjs::collectDesiredPointers 的
// 忠实移植。每 tick 由快照 JSON + journal 文本重新全量推导（按 slot LWW），免疫 journal 截断。

use crate::types::Pointer;
use serde_json::Value;
use std::collections::BTreeMap;

/// (kind, snapshot field name)
const POINTER_SLOTS: [(&str, &str); 5] = [
    ("index", "indexPointers"),
    ("contacts", "contactsPointers"),
    ("archive", "msgArchivePointers"),
    // EN: Track A MLS escrow-vault pointer slot (design §4/§13). CN: 路线 A MLS 托管 vault 指针槽（设计 §4/§13）。
    ("mls", "mlsVaultPointers"),
    // EN: Track A PIN-wrapped signing-key backup (design §5.3 path C) — real IPFS CID. CN: 路线 A PIN 包裹签名钥备份（设计 §5.3 路径 C）——真实 IPFS CID。
    ("mls_signing", "mlsSigningPointers"),
];

fn put(
    slots: &mut BTreeMap<String, Pointer>,
    kind: &str,
    account: &str,
    cid: &str,
    updated_at: u64,
) {
    if account.is_empty() || cid.is_empty() || updated_at == 0 {
        return;
    }
    let key = format!("{kind}/{account}");
    match slots.get(&key) {
        Some(prev) if updated_at < prev.updated_at => {}
        _ => {
            slots.insert(
                key,
                Pointer {
                    cid: cid.to_string(),
                    updated_at,
                },
            );
        }
    }
}

fn op_field(op: &str) -> Option<&'static str> {
    match op {
        "index_put" => Some("indexPointers"),
        "contacts_put" => Some("contactsPointers"),
        "msg_archive_put" => Some("msgArchivePointers"),
        "mls_vault_put" => Some("mlsVaultPointers"),
        "mls_signing_put" => Some("mlsSigningPointers"),
        _ => None,
    }
}

/// EN: Map slotKey (`{kind}/{account}`) -> {cid, updated_at}. `snapshot` is the parsed
/// relay-state.json (any Value); `journal_text` is the raw ndjson. CN: 推导 槽位→指针。
pub fn collect_desired_pointers(
    snapshot: Option<&Value>,
    journal_text: &str,
) -> BTreeMap<String, Pointer> {
    let mut slots: BTreeMap<String, Pointer> = BTreeMap::new();

    if let Some(Value::Object(root)) = snapshot {
        for (kind, field) in POINTER_SLOTS {
            if let Some(Value::Object(ptrs)) = root.get(field) {
                for (account, ptr) in ptrs {
                    let cid = ptr.get("cid").and_then(Value::as_str).unwrap_or("");
                    let ua = ptr.get("updated_at").and_then(Value::as_u64).unwrap_or(0);
                    put(&mut slots, kind, account, cid, ua);
                }
            }
        }
    }

    for line in journal_text.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(op) = entry.get("op").and_then(Value::as_str) else {
            continue;
        };
        let Some(field) = op_field(op) else {
            continue;
        };
        let kind = POINTER_SLOTS.iter().find(|(_, f)| *f == field).unwrap().0;
        let account = entry.get("account").and_then(Value::as_str).unwrap_or("");
        let cid = entry.get("cid").and_then(Value::as_str).unwrap_or("");
        let ua = entry.get("updated_at").and_then(Value::as_u64).unwrap_or(0);
        put(&mut slots, kind, account, cid, ua);
    }

    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn snapshot_plus_journal_lww() {
        let snap = json!({
            "indexPointers": { "5Alice": { "cid": "old", "updated_at": 1 } },
            "contactsPointers": {},
            "msgArchivePointers": {}
        });
        let journal = [
            r#"{"op":"index_put","account":"5Alice","cid":"new","updated_at":5,"at":10}"#,
            r#"{"op":"index_put","account":"5Alice","cid":"stale","updated_at":2,"at":11}"#,
            "garbage line",
            r#"{"op":"contacts_put","account":"5Bob","cid":"cbob","updated_at":3,"at":12}"#,
        ]
        .join("\n");

        let got = collect_desired_pointers(Some(&snap), &journal);
        assert_eq!(got.get("index/5Alice").unwrap().cid, "new");
        assert_eq!(got.get("contacts/5Bob").unwrap().cid, "cbob");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn mls_vault_slot_from_snapshot_and_journal() {
        let snap = json!({
            "mlsVaultPointers": { "5Alice": { "cid": "vold", "updated_at": 1 } }
        });
        let journal = [
            r#"{"op":"mls_vault_put","account":"5Alice","cid":"vnew","updated_at":7,"at":10}"#,
            r#"{"op":"mls_vault_put","account":"5Bob","cid":"vbob","updated_at":4,"at":11}"#,
        ]
        .join("\n");

        let got = collect_desired_pointers(Some(&snap), &journal);
        assert_eq!(got.get("mls/5Alice").unwrap().cid, "vnew");
        assert_eq!(got.get("mls/5Bob").unwrap().cid, "vbob");
    }

    #[test]
    fn mls_signing_slot_from_snapshot_and_journal() {
        let snap = json!({
            "mlsSigningPointers": { "5Alice": { "cid": "sold", "updated_at": 1 } }
        });
        let journal = [
            r#"{"op":"mls_signing_put","account":"5Alice","cid":"snew","updated_at":9,"at":10}"#,
            r#"{"op":"mls_signing_put","account":"5Bob","cid":"sbob","updated_at":4,"at":11}"#,
        ]
        .join("\n");

        let got = collect_desired_pointers(Some(&snap), &journal);
        assert_eq!(got.get("mls_signing/5Alice").unwrap().cid, "snew");
        assert_eq!(got.get("mls_signing/5Bob").unwrap().cid, "sbob");
    }

    #[test]
    fn empty_inputs() {
        assert!(collect_desired_pointers(None, "").is_empty());
    }
}
