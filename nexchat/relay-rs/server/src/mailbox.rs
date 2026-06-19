// EN: Mailbox business logic ported byte-for-byte from relay-chat-mailbox.mjs and
// relay-contact-mailbox.mjs: TTL/expiry prune, LRU caps, dedup keys, and wire shaping.
// Rows are opaque JSON objects (same as the Node Maps); persistence stores them verbatim.
// CN: 邮箱业务逻辑，逐字段复刻 relay-chat-mailbox.mjs / relay-contact-mailbox.mjs：
// TTL/过期清理、LRU 上限、去重键、上线整形。行为不透明 JSON 对象，持久化原样存取。

use relay_core::ContactBox;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

type Box_ = BTreeMap<String, Value>;

fn stored_at(row: &Value) -> u64 {
    row.get("stored_at").and_then(Value::as_u64).unwrap_or(0)
}

/// EN: `expiresAt != null && now > expiresAt`. CN: 帧是否过期。
pub fn frame_expired(row: &Value, now: u64) -> bool {
    match row.get("expiresAt") {
        None | Some(Value::Null) => false,
        Some(v) => v.as_u64().is_some_and(|e| now > e),
    }
}

/// EN: Dedup key — prefer client `dedupKey`, else `convId:ciphertextB64`. CN: 去重键。
pub fn chat_frame_dedup_key(msg: &Value) -> String {
    if let Some(d) = msg.get("dedupKey") {
        if !d.is_null() {
            return d
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| d.to_string());
        }
    }
    let conv = msg.get("convId").and_then(Value::as_str).unwrap_or("");
    let ct = msg
        .get("ciphertextB64")
        .and_then(Value::as_str)
        .unwrap_or("");
    format!("{conv}:{ct}")
}

/// EN: TTL + expiresAt prune; returns whether anything was removed. CN: TTL/过期清理。
pub fn prune_chat_box(b: &mut Box_, ttl: u64, now: u64) -> bool {
    let before = b.len();
    b.retain(|_, row| !(frame_expired(row, now) || now.saturating_sub(stored_at(row)) > ttl));
    before != b.len()
}

/// EN: Generic TTL prune by `stored_at` (mls/group-invite mailboxes). CN: 按 stored_at 的通用 TTL 清理。
pub fn prune_ttl(b: &mut Box_, ttl: u64, now: u64) -> bool {
    let before = b.len();
    b.retain(|_, row| now.saturating_sub(stored_at(row)) <= ttl);
    before != b.len()
}

fn box_total_bytes(b: &Box_) -> u64 {
    b.values()
        .map(|r| r.get("bytes").and_then(Value::as_u64).unwrap_or(0))
        .sum()
}

/// EN: LRU trim by `stored_at` when over frame/byte caps (chat + MLS mailboxes). CN: 超上限 LRU 裁剪。
pub fn enforce_json_box_cap(b: &mut Box_, max_frames: usize, max_bytes: u64) -> bool {
    if b.is_empty() {
        return false;
    }
    let mut changed = false;
    while b.len() > max_frames || box_total_bytes(b) > max_bytes {
        let oldest = b
            .iter()
            .min_by_key(|(_, r)| stored_at(r))
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                b.remove(&k);
                changed = true;
            }
            None => break,
        }
    }
    changed
}

fn put_if_present(out: &mut Map<String, Value>, src: &Value, key: &str, skip_null: bool) {
    if let Some(v) = src.get(key) {
        if skip_null && v.is_null() {
            return;
        }
        out.insert(key.to_string(), v.clone());
    }
}

/// EN: Build a stored chat row (`dedupKey` + frame fields + `stored_at`/`bytes`). CN: 构建存储行。
pub fn build_chat_row(msg: &Value, now: u64, wire_bytes: u64) -> Value {
    let mut o = Map::new();
    o.insert("dedupKey".into(), Value::String(chat_frame_dedup_key(msg)));
    put_if_present(&mut o, msg, "convId", false);
    put_if_present(&mut o, msg, "senderRef", true);
    put_if_present(&mut o, msg, "ciphertextB64", false);
    put_if_present(&mut o, msg, "expiresAt", true);
    put_if_present(&mut o, msg, "delivery", true);
    put_if_present(&mut o, msg, "routeTo", true);
    o.insert("stored_at".into(), Value::from(now));
    o.insert("bytes".into(), Value::from(wire_bytes));
    Value::Object(o)
}

/// EN: Strip relay metadata before wire send (`chatRowToWire`). CN: 发送前去掉 relay 元数据。
pub fn chat_row_to_wire(row: &Value) -> Value {
    let mut o = Map::new();
    put_if_present(&mut o, row, "convId", false);
    put_if_present(&mut o, row, "senderRef", true);
    put_if_present(&mut o, row, "ciphertextB64", false);
    put_if_present(&mut o, row, "dedupKey", false);
    put_if_present(&mut o, row, "expiresAt", true);
    put_if_present(&mut o, row, "delivery", true);
    put_if_present(&mut o, row, "routeTo", true);
    Value::Object(o)
}

/// EN: One newly inserted chat-mailbox row (for WAL `chat_store`). CN: 新插入的聊天邮箱行（供 WAL `chat_store`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatStoreInsert {
    pub account: String,
    pub dedup_key: String,
    pub row: Value,
}

/// EN: Plan offline-mailbox inserts without mutating state (journal `record` applies them).
/// CN: 规划离线邮箱插入（不修改状态；由 journal `record` 应用）。
pub fn plan_chat_frame_stores(
    map: &BTreeMap<String, Box_>,
    msg: &Value,
    accounts: &BTreeSet<String>,
    except: Option<&str>,
    now: u64,
    wire_bytes: u64,
) -> Vec<ChatStoreInsert> {
    let conv_ok = msg
        .get("convId")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let ct_ok = msg
        .get("ciphertextB64")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    if !conv_ok || !ct_ok || frame_expired(msg, now) {
        return vec![];
    }
    let key = chat_frame_dedup_key(msg);
    let mut out = Vec::new();
    for account in accounts {
        if except == Some(account.as_str()) {
            continue;
        }
        let empty = BTreeMap::new();
        let b = map.get(account).unwrap_or(&empty);
        if b.contains_key(&key) {
            continue;
        }
        out.push(ChatStoreInsert {
            account: account.clone(),
            dedup_key: key.clone(),
            row: build_chat_row(msg, now, wire_bytes),
        });
    }
    out
}

/// EN: Persist a frame for each recipient (except sender). Prefer `plan_chat_frame_stores` +
/// journal `ChatStore` in production; this helper applies directly (tests / legacy).
/// CN: 为各收件人持久化帧（排除发送方）。生产路径请用 `plan_chat_frame_stores` + journal `ChatStore`；
/// 本 helper 直接写入（测试/遗留）。
#[allow(clippy::too_many_arguments)]
pub fn store_chat_frame_for_accounts(
    map: &mut BTreeMap<String, Box_>,
    msg: &Value,
    accounts: &BTreeSet<String>,
    except: Option<&str>,
    now: u64,
    ttl: u64,
    max_frames: usize,
    max_bytes: u64,
    wire_bytes: u64,
) -> Vec<ChatStoreInsert> {
    let planned = plan_chat_frame_stores(map, msg, accounts, except, now, wire_bytes);
    for ins in &planned {
        let b = map.entry(ins.account.clone()).or_default();
        prune_chat_box(b, ttl, now);
        b.insert(ins.dedup_key.clone(), ins.row.clone());
        enforce_json_box_cap(b, max_frames, max_bytes);
    }
    planned
}

/// EN: Non-destructive list for `chat_fetch` (TTL-only eviction). CN: chat_fetch 非破坏性列举。
pub fn list_chat_mailbox(
    map: &mut BTreeMap<String, Box_>,
    account: &str,
    now: u64,
    ttl: u64,
) -> Vec<Value> {
    let Some(b) = map.get_mut(account) else {
        return vec![];
    };
    prune_chat_box(b, ttl, now);
    if b.is_empty() {
        map.remove(account);
        return vec![];
    }
    b.values()
        .filter(|row| !frame_expired(row, now))
        .map(chat_row_to_wire)
        .collect()
}

/// EN: Remove consumed dedup keys (ops/single-device opt-in). CN: 删除已消费 dedupKey。
pub fn consume_chat_mailbox(
    map: &mut BTreeMap<String, Box_>,
    account: &str,
    keys: &[String],
) -> bool {
    let Some(b) = map.get_mut(account) else {
        return false;
    };
    if keys.is_empty() {
        return false;
    }
    let mut changed = false;
    for k in keys {
        if b.remove(k).is_some() {
            changed = true;
        }
    }
    if b.is_empty() {
        map.remove(account);
    }
    changed
}

/// EN: Aggregate chat-mailbox stats (`chatMailboxStats`). CN: 聚合聊天邮箱统计。
pub struct ChatStats {
    pub accounts: usize,
    pub frames: usize,
    pub bytes: u64,
    pub max_frames_per_account: usize,
}

pub fn chat_mailbox_stats(map: &BTreeMap<String, Box_>) -> ChatStats {
    let mut frames = 0;
    let mut bytes = 0u64;
    let mut max_frames = 0;
    for b in map.values() {
        frames += b.len();
        if b.len() > max_frames {
            max_frames = b.len();
        }
        bytes += box_total_bytes(b);
    }
    ChatStats {
        accounts: map.len(),
        frames,
        bytes,
        max_frames_per_account: max_frames,
    }
}

/// EN: TTL prune of a contact box (reqs + acks). CN: 联系人邮箱 TTL 清理。
pub fn prune_contact_box(b: &mut ContactBox, ttl: u64, now: u64) -> bool {
    let before = b.reqs.len() + b.acks.len();
    b.reqs
        .retain(|_, row| now.saturating_sub(stored_at(row)) <= ttl);
    b.acks
        .retain(|_, row| now.saturating_sub(stored_at(row)) <= ttl);
    before != b.reqs.len() + b.acks.len()
}

/// EN: LRU trim combined reqs+acks when over `max_entries`. CN: 合并 reqs+acks 超上限时 LRU 裁剪。
pub fn enforce_contact_cap(b: &mut ContactBox, max_entries: usize) -> bool {
    let mut changed = false;
    while b.reqs.len() + b.acks.len() > max_entries {
        let oldest_req = b
            .reqs
            .iter()
            .min_by_key(|(_, r)| stored_at(r))
            .map(|(k, _)| (true, k.clone()));
        let oldest_ack = b
            .acks
            .iter()
            .min_by_key(|(_, r)| stored_at(r))
            .map(|(k, _)| (false, k.clone()));
        let pick = match (oldest_req, oldest_ack) {
            (Some((_, rk)), Some((_, ak))) => {
                let rt = b.reqs.get(&rk).map(stored_at).unwrap_or(u64::MAX);
                let at = b.acks.get(&ak).map(stored_at).unwrap_or(u64::MAX);
                if rt <= at {
                    Some((true, rk))
                } else {
                    Some((false, ak))
                }
            }
            (Some(r), None) => Some(r),
            (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        match pick {
            Some((true, k)) => {
                b.reqs.remove(&k);
                changed = true;
            }
            Some((false, k)) => {
                b.acks.remove(&k);
                changed = true;
            }
            None => break,
        }
    }
    changed
}

/// EN: Remove consumed contact req/ack ids; drops empty box. CN: 删除已消费 req/ack；空则删账户。
pub fn consume_contact_box(
    map: &mut BTreeMap<String, ContactBox>,
    account: &str,
    req_ids: &[String],
    ack_ids: &[String],
) -> bool {
    let Some(b) = map.get_mut(account) else {
        return false;
    };
    for id in req_ids {
        b.reqs.remove(id);
    }
    for id in ack_ids {
        b.acks.remove(id);
    }
    if b.reqs.is_empty() && b.acks.is_empty() {
        map.remove(account);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(conv: &str, ct: &str) -> Value {
        serde_json::json!({ "convId": conv, "ciphertextB64": ct, "senderRef": "a" })
    }

    fn accounts(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // EN: Track B echo — with `except = None` the frame is retained in the SENDER account's mailbox
    // (so its offline sibling devices catch up), whereas `except = Some(sender)` preserves the
    // single-device default of NOT storing it for the sender. CN: 路线 B 回显——`except = None` 时
    // 帧留存到「发送方账户」邮箱（其离线兄弟设备得以补齐）；`except = Some(sender)` 则保持单设备
    // 默认：不为发送方留存。
    #[test]
    fn echo_self_retains_frame_for_sender_account() {
        let recipients = accounts(&["a", "b"]); // 1:1 d:a:b, sender = a
        let ttl = 1_000_000;
        let now = 1;

        // echoSelf path: except = None → sender account "a" also retains.
        let mut map: BTreeMap<String, Box_> = BTreeMap::new();
        let inserted = store_chat_frame_for_accounts(
            &mut map,
            &frame("d:a:b", "ct1"),
            &recipients,
            None,
            now,
            ttl,
            100,
            1 << 20,
            42,
        );
        assert_eq!(inserted.len(), 2);
        assert!(
            map.contains_key("a"),
            "echoSelf must retain for sender account"
        );
        assert!(map.contains_key("b"));

        // default path: except = Some("a") → sender account is NOT retained (parity).
        let mut map2: BTreeMap<String, Box_> = BTreeMap::new();
        store_chat_frame_for_accounts(
            &mut map2,
            &frame("d:a:b", "ct2"),
            &recipients,
            Some("a"),
            now,
            ttl,
            100,
            1 << 20,
            42,
        );
        assert!(
            !map2.contains_key("a"),
            "default must NOT retain for sender account"
        );
        assert!(map2.contains_key("b"));
    }

    #[test]
    fn mls_box_cap_trims_oldest() {
        let mut b: Box_ = BTreeMap::new();
        for i in 0..5u64 {
            b.insert(
                format!("k{i}"),
                serde_json::json!({ "stored_at": i, "bytes": 10 }),
            );
        }
        enforce_json_box_cap(&mut b, 3, 100);
        assert_eq!(b.len(), 3);
        assert!(!b.contains_key("k0"));
        assert!(!b.contains_key("k1"));
        assert!(b.contains_key("k4"));
    }

    #[test]
    fn contact_cap_trims_oldest_across_buckets() {
        let mut b = ContactBox::default();
        b.reqs
            .insert("r1".into(), serde_json::json!({ "stored_at": 1 }));
        b.reqs
            .insert("r2".into(), serde_json::json!({ "stored_at": 5 }));
        b.acks
            .insert("a1".into(), serde_json::json!({ "stored_at": 2 }));
        enforce_contact_cap(&mut b, 2);
        assert_eq!(b.reqs.len() + b.acks.len(), 2);
        assert!(!b.reqs.contains_key("r1"));
        assert!(b.reqs.contains_key("r2"));
        assert!(b.acks.contains_key("a1"));
    }
}
