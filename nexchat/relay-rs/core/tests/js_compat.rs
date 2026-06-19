// EN: Cross-language disk-format conformance (the drop-in命门): a snapshot + journal in the
// exact shape emitted by relay-persistence.mjs must load into Rust with identical semantics,
// and a Rust-written snapshot must serialize back in JS-readable shape (camelCase map fields,
// `spentByInbox` as arrays, nested contactMailbox reqs/acks).
// CN: 跨语言磁盘格式一致性（drop-in 命门）：JS relay-persistence.mjs 输出形态的快照 + journal
// 必须被 Rust 语义一致地载入；Rust 落盘的快照必须仍是 JS 可读形态。

use relay_core::{PersistState, Persistence, DEFAULT_SPENT_CAP};
use serde_json::Value;
use std::fs;

// A snapshot exactly as relay-persistence.mjs::snapshot() would emit (saved_at = 1000).
const JS_SNAPSHOT: &str = r#"{"v":1,"saved_at":1000,"indexPointers":{"5Acc":{"cid":"bafyi","updated_at":50}},"contactsPointers":{},"msgArchivePointers":{},"inboxesByAccount":{"5Acc":{"inbox_id":"0xib","epoch":2,"ipk_n":"nnn","ipk_e":"AQAB","revoked_tags":["t1"]}},"spentByInbox":{"0xib":["tokA","tokB"]},"contactMailbox":{"5Acc":{"reqs":{"r1":{"t":"contact_req","toAddr":"5Acc","reqId":"r1","_ctrl":true,"stored_at":900}},"acks":{}}},"groupInviteMailbox":{},"mlsMailbox":{},"chatMailbox":{"5Acc":{"f1":{"dedupKey":"f1","convId":"d:5Acc","senderRef":"5X","ciphertextB64":"AQID","stored_at":900,"bytes":64}}}}"#;

// Journal: one entry newer than the snapshot (applied) + one stale (skipped at <= saved_at).
const JS_JOURNAL: &str = concat!(
    r#"{"op":"index_put","account":"5Acc","cid":"bafyNEW","updated_at":99,"at":2000}"#,
    "\n",
    r#"{"op":"index_put","account":"5Acc","cid":"bafySTALE","updated_at":10,"at":500}"#,
    "\n",
);

#[test]
fn js_snapshot_and_journal_load_into_rust() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("relay-state.json"), JS_SNAPSHOT).unwrap();
    fs::write(dir.path().join("relay-journal.ndjson"), JS_JOURNAL).unwrap();

    let mut state = PersistState::default();
    let loaded = Persistence::new(dir.path(), DEFAULT_SPENT_CAP)
        .load_into(&mut state)
        .unwrap();
    assert!(loaded);

    // journal entry with at=2000 > saved_at=1000 wins; the at=500 stale one is skipped.
    assert_eq!(state.index_pointers.get("5Acc").unwrap().cid, "bafyNEW");

    let ib = state.inboxes_by_account.get("5Acc").unwrap();
    assert_eq!(ib.inbox_id, "0xib");
    assert_eq!(ib.epoch, 2);
    assert_eq!(ib.ipk_e, "AQAB");
    assert_eq!(ib.revoked_tags, vec!["t1".to_string()]);

    let spent = state.spent_by_inbox.get("0xib").unwrap();
    assert!(spent.contains("tokA") && spent.contains("tokB"));

    // opaque mailbox rows preserved verbatim
    assert!(state
        .contact_mailbox
        .get("5Acc")
        .unwrap()
        .reqs
        .contains_key("r1"));
    let frame = &state.chat_mailbox.get("5Acc").unwrap()["f1"];
    assert_eq!(frame["ciphertextB64"], Value::from("AQID"));
}

#[test]
fn rust_snapshot_is_js_readable_shape() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("relay-state.json"), JS_SNAPSHOT).unwrap();
    let mut state = PersistState::default();
    let p = Persistence::new(dir.path(), DEFAULT_SPENT_CAP);
    p.load_into(&mut state).unwrap();

    // Re-snapshot from Rust, then re-parse as raw JSON to assert JS-compatible shape.
    p.flush_now(&state).unwrap();
    let body = fs::read_to_string(dir.path().join("relay-state.json")).unwrap();
    let v: Value = serde_json::from_str(&body).unwrap();

    assert_eq!(v["v"], Value::from(1));
    assert!(v.get("saved_at").is_some(), "saved_at stays snake_case");
    // camelCase map fields
    assert!(v["indexPointers"]["5Acc"]["cid"] == Value::from("bafyi"));
    assert!(v["inboxesByAccount"]["5Acc"]["inbox_id"] == Value::from("0xib"));
    // spentByInbox is an ARRAY per inbox (Set -> array), not an object
    assert!(v["spentByInbox"]["0xib"].is_array());
    // nested contact mailbox reqs/acks
    assert!(v["contactMailbox"]["5Acc"]["reqs"]["r1"]["reqId"] == Value::from("r1"));
    assert!(v["contactMailbox"]["5Acc"]["acks"].is_object());
}
