// EN: Wire protocol handler for relay-rs. Message types, acks/rejects, routing, mailbox semantics,
// and RFC 9474 admission. JSON undefined fields are reproduced by omitting absent keys. Account
// keys are normalized to SS58 prefix-42 internally; replies echo the caller's `account`/`request_id`.
// CN: relay-rs wire 协议处理。消息类型、ack/reject、路由、邮箱语义、RFC 9474 准入。JSON undefined
// 以缺键复现。账户键内部规范化为 SS58 prefix-42；回执原样回显调用方 account/request_id。

use relay_core::{
    normalize_account, now_ms, try_accept_commit, verify_register_account_sig, CommitDecision,
    InboxRecord, JournalOp, Pointer,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

use crate::config::{CONTACT_TTL_MS, GROUP_INVITE_TTL_MS, MLS_CTRL_TTL_MS};
use crate::mailbox::{
    chat_mailbox_stats, chat_row_to_wire, consume_chat_mailbox, consume_contact_box,
    enforce_contact_cap, enforce_json_box_cap, frame_expired, list_chat_mailbox,
    plan_chat_frame_stores, prune_chat_box, prune_contact_box, prune_ttl,
};
use crate::state::{Inner, Server, Tx};
use crate::token::{verify_delivery_token, DeliveryToken};
use tokio_tungstenite::tungstenite::Message;

/// EN: Per-connection state carried across messages. CN: 跨消息的连接级状态。
pub struct Conn {
    pub id: Option<String>,
    pub account: Option<String>,
    pub is_loopback: bool,
}

fn reply(tx: &Tx, v: Value) {
    let _ = tx.send(Message::Text(v.to_string().into()));
}

/// EN: Best-effort NACK for a frame the connection layer dropped (rate limit, etc.) so the
/// sender can surface "failed / retry" instead of the message silently vanishing. Only user
/// frames carrying a `dedupKey` are NACK'd; control-plane / fetch / register messages are
/// ignored. `raw_text` is the already-size-bounded wire text (never the oversize path).
/// CN: 对连接层丢弃的帧（限流等）尽力回一条 NACK，让发送方显示「失败/重试」而非静默丢失。
/// 仅对带 `dedupKey` 的用户帧回执；控制面/拉取/注册消息忽略。`raw_text` 为已限长的 wire 文本
/// （不走超大消息路径）。
pub fn reject_frame(tx: &Tx, raw_text: &str, reason: &str) {
    let Ok(msg) = serde_json::from_str::<Value>(raw_text) else {
        return;
    };
    if !msg.is_object() || msg.get("_ctrl") == Some(&Value::Bool(true)) {
        return;
    }
    let Some(dedup) = truthy_str(&msg, "dedupKey") else {
        return;
    };
    let mut o = Map::new();
    o.insert("type".into(), "frame_reject".into());
    o.insert("reason".into(), Value::String(reason.to_string()));
    o.insert("dedupKey".into(), Value::String(dedup));
    if let Some(conv) = truthy_str(&msg, "convId") {
        o.insert("convId".into(), Value::String(conv));
    }
    reply(tx, Value::Object(o));
}

/// EN: Truthy string (present + non-empty), mirroring JS `if (x)`. CN: 非空字符串（对应 JS 真值）。
fn truthy_str(msg: &Value, key: &str) -> Option<String> {
    msg.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn get_string(msg: &Value, key: &str) -> String {
    msg.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn pos_u64(msg: &Value, key: &str) -> Option<u64> {
    msg.get(key).and_then(Value::as_u64).filter(|n| *n > 0)
}

fn str_array(msg: &Value, key: &str) -> Vec<String> {
    msg.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// EN: Copy a present, non-null field verbatim (reproduces JS pass-through). CN: 原样透传非空字段。
fn passthrough(out: &mut Map<String, Value>, msg: &Value, key: &str) {
    if let Some(v) = msg.get(key) {
        if !v.is_null() {
            out.insert(key.to_string(), v.clone());
        }
    }
}

fn assert_writer(conn: &Conn, normalized_account: &str) -> bool {
    conn.account.as_deref() == Some(normalized_account)
}

/// EN: Reader gate — open in dev; when `strict_auth`, same as writer. CN: 读路径门禁——开发模式开放；
/// `strict_auth` 时与 writer 相同。
fn assert_reader(server: &Server, conn: &Conn, normalized_account: &str) -> bool {
    if server.cfg.strict_auth {
        assert_writer(conn, normalized_account)
    } else {
        true
    }
}

fn decode_account_sig_b64(raw: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .ok()
        .filter(|v| !v.is_empty())
}

/// EN: Validate optional/required `account_sig` on `register_account`. CN: 校验 `register_account` 的 `account_sig`。
fn check_register_account_sig(
    server: &Server,
    msg: &Value,
    endpoint_id: &str,
    account: &str,
) -> Result<(), &'static str> {
    let sig_raw = msg.get("account_sig").and_then(Value::as_str);
    match sig_raw {
        None if server.cfg.strict_auth => Err("missing_sig"),
        None => Ok(()),
        Some(raw) => {
            let Some(sig) = decode_account_sig_b64(raw) else {
                return Err("invalid_sig");
            };
            if verify_register_account_sig(endpoint_id, account, &sig) {
                Ok(())
            } else {
                Err("invalid_sig")
            }
        }
    }
}

fn send_auth_reject_reason(tx: &Tx, msg: &Value, op: &str, reason: &str) {
    let mut o = Map::new();
    o.insert("type".into(), "auth_reject".into());
    o.insert("op".into(), Value::String(op.to_string()));
    o.insert("reason".into(), Value::String(reason.to_string()));
    passthrough(&mut o, msg, "account");
    reply(tx, Value::Object(o));
}

fn send_auth_reject(tx: &Tx, msg: &Value, op: &str) {
    let mut o = Map::new();
    o.insert("type".into(), "auth_reject".into());
    o.insert("op".into(), Value::String(op.to_string()));
    passthrough(&mut o, msg, "account");
    reply(tx, Value::Object(o));
}

/// EN: Tell a Commit sender it lost the `(conv, epoch)` race so it can catch up to `current_epoch`
/// and retry (spec §3.3). Echoes `msgId` when present for client-side correlation.
/// CN: 通知 Commit 发送方在 `(conv, epoch)` 竞争中落败，使其追平到 `current_epoch` 后重试（规范 §3.3）。
/// 存在 `msgId` 时回显以便客户端关联。
fn send_commit_reject(tx: &Tx, msg: &Value, conv: &str, current_epoch: u64) {
    let mut o = Map::new();
    o.insert("type".into(), "commit_reject".into());
    o.insert("reason".into(), "epoch_stale".into());
    o.insert("convId".into(), Value::String(conv.to_string()));
    o.insert("current_epoch".into(), Value::from(current_epoch));
    if let Some(id) = truthy_str(msg, "msgId") {
        o.insert("msgId".into(), Value::String(id));
    }
    reply(tx, Value::Object(o));
}

fn assert_admin(conn: &Conn, msg: &Value, secret: &str) -> bool {
    if !secret.is_empty() && msg.get("admin_secret").and_then(Value::as_str) == Some(secret) {
        return true;
    }
    conn.is_loopback
}

#[derive(Clone, Copy)]
enum PtrKind {
    Index,
    Contacts,
    MsgArchive,
    // EN: Track A MLS escrow-vault pointer (design CHAT_MULTIDEVICE_MLS_SYNC §4/§13). Same contract
    // as the other pointer slots — relay stores only `{cid, updated_at}`. CN: 路线 A MLS 托管 vault
    // 指针（设计 §4/§13）。与其它指针槽合同一致——relay 仅存 `{cid, updated_at}`。
    MlsVault,
    // EN: Track A sending-authority handoff pointer (design §5.2). Content-agnostic reuse: `cid` =
    // opaque base64 HandoffReceipt envelope, `updated_at` = monotone handoff seq. CN: 路线 A 发送权
    // 交接指针（设计 §5.2）。内容无关复用：`cid` = 不透明 base64 HandoffReceipt 信封，`updated_at` = 单调 seq。
    Handoff,
    // EN: Track A PIN-wrapped signing-key backup pointer (design §5.3 path C). CN: 路线 A PIN 包裹签名钥备份指针（设计 §5.3 路径 C）。
    MlsSigning,
}

impl PtrKind {
    fn op_name(self) -> &'static str {
        match self {
            PtrKind::Index => "index_put",
            PtrKind::Contacts => "contacts_put",
            PtrKind::MsgArchive => "msg_archive_put",
            PtrKind::MlsVault => "mls_vault_put",
            PtrKind::Handoff => "handoff_put",
            PtrKind::MlsSigning => "mls_signing_put",
        }
    }
    fn ack_name(self) -> &'static str {
        match self {
            PtrKind::Index => "index_ack",
            PtrKind::Contacts => "contacts_ack",
            PtrKind::MsgArchive => "msg_archive_ack",
            PtrKind::MlsVault => "mls_vault_ack",
            PtrKind::Handoff => "handoff_ack",
            PtrKind::MlsSigning => "mls_signing_ack",
        }
    }
    fn reply_name(self) -> &'static str {
        match self {
            PtrKind::Index => "index_reply",
            PtrKind::Contacts => "contacts_reply",
            PtrKind::MsgArchive => "msg_archive_reply",
            PtrKind::MlsVault => "mls_vault_reply",
            PtrKind::Handoff => "handoff_reply",
            PtrKind::MlsSigning => "mls_signing_reply",
        }
    }
    fn reject_name(self) -> &'static str {
        match self {
            PtrKind::Index => "index_reject",
            PtrKind::Contacts => "contacts_reject",
            PtrKind::MsgArchive => "msg_archive_reject",
            PtrKind::MlsVault => "mls_vault_reject",
            PtrKind::Handoff => "handoff_reject",
            PtrKind::MlsSigning => "mls_signing_reject",
        }
    }
    fn op(self, account: String, cid: String, updated_at: u64) -> JournalOp {
        match self {
            PtrKind::Index => JournalOp::IndexPut {
                account,
                cid,
                updated_at,
            },
            PtrKind::Contacts => JournalOp::ContactsPut {
                account,
                cid,
                updated_at,
            },
            PtrKind::MsgArchive => JournalOp::MsgArchivePut {
                account,
                cid,
                updated_at,
            },
            PtrKind::MlsVault => JournalOp::MlsVaultPut {
                account,
                cid,
                updated_at,
            },
            PtrKind::Handoff => JournalOp::HandoffPut {
                account,
                cid,
                updated_at,
            },
            PtrKind::MlsSigning => JournalOp::MlsSigningPut {
                account,
                cid,
                updated_at,
            },
        }
    }
}

fn pointer_map_mut<'a>(inner: &'a mut Inner, kind: PtrKind) -> &'a mut BTreeMap<String, Pointer> {
    match kind {
        PtrKind::Index => &mut inner.persist.index_pointers,
        PtrKind::Contacts => &mut inner.persist.contacts_pointers,
        PtrKind::MsgArchive => &mut inner.persist.msg_archive_pointers,
        PtrKind::MlsVault => &mut inner.persist.mls_vault_pointers,
        PtrKind::Handoff => &mut inner.persist.handoff_pointers,
        PtrKind::MlsSigning => &mut inner.persist.mls_signing_pointers,
    }
}

fn pointer_map_ref<'a>(inner: &'a Inner, kind: PtrKind) -> &'a BTreeMap<String, Pointer> {
    match kind {
        PtrKind::Index => &inner.persist.index_pointers,
        PtrKind::Contacts => &inner.persist.contacts_pointers,
        PtrKind::MsgArchive => &inner.persist.msg_archive_pointers,
        PtrKind::MlsVault => &inner.persist.mls_vault_pointers,
        PtrKind::Handoff => &inner.persist.handoff_pointers,
        PtrKind::MlsSigning => &inner.persist.mls_signing_pointers,
    }
}

/// EN: Entry point — parse, lock, dispatch one wire message. CN: 入口——解析、加锁、分发一条消息。
pub fn process(server: &Server, conn: &mut Conn, self_tx: &Tx, raw_text: &str) {
    let Ok(mut msg) = serde_json::from_str::<Value>(raw_text) else {
        return;
    };
    if !msg.is_object() {
        return;
    }
    let mut guard = server.inner.lock().unwrap_or_else(|e| e.into_inner());
    let inner = &mut *guard;
    let now = now_ms();
    let mtype = msg
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // --- session registration ---------------------------------------------------------
    if mtype == "register" {
        if let Some(id) = truthy_str(&msg, "id") {
            inner.clients.insert(id.clone(), self_tx.clone());
            conn.id = Some(id);
            return;
        }
    }
    if mtype == "register_account" {
        if let (Some(id), Some(acct_raw)) = (truthy_str(&msg, "id"), truthy_str(&msg, "account")) {
            if conn.id.as_deref() != Some(id.as_str()) {
                send_auth_reject(self_tx, &msg, "register_account");
                return;
            }
            let account = normalize_account(&acct_raw);
            if let Err(reason) = check_register_account_sig(server, &msg, &id, &account) {
                send_auth_reject_reason(self_tx, &msg, "register_account", reason);
                return;
            }
            if let Some(prev) = conn.account.as_ref() {
                if prev != &account {
                    inner.remove_endpoint_from_account(&id, prev);
                }
            }
            conn.account = Some(account.clone());
            inner
                .endpoints_by_account
                .entry(account.clone())
                .or_default()
                .insert(id);
            flush_mls_mailbox(inner, &account, now);
            flush_chat_mailbox(server, inner, &account, now);
            return;
        }
    }

    // --- admin ------------------------------------------------------------------------
    if mtype == "admin_stats" {
        if !assert_admin(conn, &msg, &server.cfg.admin_secret) {
            let mut o = Map::new();
            o.insert("type".into(), "admin_reject".into());
            passthrough(&mut o, &msg, "request_id");
            o.insert("reason".into(), "forbidden".into());
            reply(self_tx, Value::Object(o));
            return;
        }
        let mut o = Map::new();
        o.insert("type".into(), "admin_stats_reply".into());
        passthrough(&mut o, &msg, "request_id");
        o.insert("stats".into(), build_stats(inner));
        o.insert(
            "file".into(),
            Value::String(server.persistence.snapshot_file.display().to_string()),
        );
        o.insert(
            "journal".into(),
            Value::String(server.persistence.journal_file.display().to_string()),
        );
        reply(self_tx, Value::Object(o));
        return;
    }

    // --- cloud-sync pointers (index / contacts / msg_archive / mls_vault / handoff) ---
    for kind in [
        PtrKind::Index,
        PtrKind::Contacts,
        PtrKind::MsgArchive,
        PtrKind::MlsVault,
        PtrKind::Handoff,
        PtrKind::MlsSigning,
    ] {
        if mtype == kind.op_name() && handle_pointer_put(server, inner, conn, self_tx, &msg, kind) {
            return;
        }
        if mtype == format!("{}_fetch", &kind.op_name()[..kind.op_name().len() - 4]) {
            if handle_pointer_fetch(server, inner, conn, self_tx, &msg, kind) {
                return;
            }
        }
    }

    // --- inbox (RFC 9474 registration / lookup) ---------------------------------------
    if mtype == "inbox_register" {
        if handle_inbox_register(server, inner, conn, self_tx, &msg) {
            return;
        }
    }
    if mtype == "inbox_lookup" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            let ib = inner.persist.inboxes_by_account.get(&account).cloned();
            let online = inner
                .endpoints_by_account
                .get(&account)
                .is_some_and(|s| !s.is_empty());
            let mut o = Map::new();
            o.insert("type".into(), "inbox_reply".into());
            passthrough(&mut o, &msg, "request_id");
            if let Some(ib) = &ib {
                o.insert("inbox_id".into(), Value::String(ib.inbox_id.clone()));
                o.insert("epoch".into(), Value::from(ib.epoch));
                o.insert("ipk_n".into(), Value::String(ib.ipk_n.clone()));
                o.insert("ipk_e".into(), Value::String(ib.ipk_e.clone()));
            }
            let revoked = ib
                .as_ref()
                .map(|i| i.revoked_tags.iter().cloned().map(Value::String).collect())
                .unwrap_or_default();
            o.insert("revoked_tags".into(), Value::Array(revoked));
            o.insert("online".into(), Value::Bool(online));
            reply(self_tx, Value::Object(o));
            return;
        }
    }

    // --- contact mailbox --------------------------------------------------------------
    if mtype == "contact_fetch" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_reader(server, conn, &account) {
                send_auth_reject(self_tx, &msg, "contact_fetch");
                return;
            }
            let mut pruned = false;
            let mut empty_after = false;
            if let Some(b) = inner.persist.contact_mailbox.get_mut(&account) {
                pruned = prune_contact_box(b, CONTACT_TTL_MS, now);
                empty_after = b.reqs.is_empty() && b.acks.is_empty();
            }
            if pruned {
                if empty_after {
                    inner.persist.contact_mailbox.remove(&account);
                }
                server.mark_dirty(inner);
            }
            let (reqs, acks) = match inner.persist.contact_mailbox.get(&account) {
                Some(b) => (
                    b.reqs.values().cloned().collect::<Vec<_>>(),
                    b.acks.values().cloned().collect::<Vec<_>>(),
                ),
                None => (vec![], vec![]),
            };
            let mut o = Map::new();
            o.insert("type".into(), "contact_reply".into());
            passthrough(&mut o, &msg, "request_id");
            passthrough(&mut o, &msg, "account");
            o.insert("reqs".into(), Value::Array(reqs));
            o.insert("acks".into(), Value::Array(acks));
            reply(self_tx, Value::Object(o));
            return;
        }
    }
    if mtype == "contact_consume" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_writer(conn, &account) {
                send_auth_reject(self_tx, &msg, "contact_consume");
                return;
            }
            let req_ids = str_array(&msg, "req_ids");
            let ack_ids = str_array(&msg, "ack_ids");
            if consume_contact_box(
                &mut inner.persist.contact_mailbox,
                &account,
                &req_ids,
                &ack_ids,
            ) {
                server.mark_dirty(inner);
            }
            return;
        }
    }

    // --- group invite mailbox ---------------------------------------------------------
    if mtype == "group_invite_fetch" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_reader(server, conn, &account) {
                send_auth_reject(self_tx, &msg, "group_invite_fetch");
                return;
            }
            let mut pruned = false;
            let mut empty_after = false;
            if let Some(b) = inner.persist.group_invite_mailbox.get_mut(&account) {
                pruned = prune_ttl(b, GROUP_INVITE_TTL_MS, now);
                empty_after = b.is_empty();
            }
            if pruned {
                if empty_after {
                    inner.persist.group_invite_mailbox.remove(&account);
                }
                server.mark_dirty(inner);
            }
            let invites = inner
                .persist
                .group_invite_mailbox
                .get(&account)
                .map(|b| b.values().cloned().collect())
                .unwrap_or_default();
            let mut o = Map::new();
            o.insert("type".into(), "group_invite_reply".into());
            passthrough(&mut o, &msg, "request_id");
            passthrough(&mut o, &msg, "account");
            o.insert("invites".into(), Value::Array(invites));
            reply(self_tx, Value::Object(o));
            return;
        }
    }
    if mtype == "group_invite_consume" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_writer(conn, &account) {
                send_auth_reject(self_tx, &msg, "group_invite_consume");
                return;
            }
            let ids = str_array(&msg, "invite_ids");
            if inner.persist.group_invite_mailbox.contains_key(&account) {
                if let Some(b) = inner.persist.group_invite_mailbox.get_mut(&account) {
                    for id in &ids {
                        b.remove(id);
                    }
                }
                if inner
                    .persist
                    .group_invite_mailbox
                    .get(&account)
                    .is_some_and(|b| b.is_empty())
                {
                    inner.persist.group_invite_mailbox.remove(&account);
                }
                server.mark_dirty(inner);
            }
            return;
        }
    }

    // --- chat mailbox -----------------------------------------------------------------
    if mtype == "chat_fetch" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_reader(server, conn, &account) {
                send_auth_reject(self_tx, &msg, "chat_fetch");
                return;
            }
            let frames = list_chat_mailbox(
                &mut inner.persist.chat_mailbox,
                &account,
                now,
                server.cfg.chat_ttl_ms,
            );
            if !frames.is_empty() {
                server.mark_dirty(inner);
            }
            let mut o = Map::new();
            o.insert("type".into(), "chat_reply".into());
            passthrough(&mut o, &msg, "request_id");
            passthrough(&mut o, &msg, "account");
            o.insert("frames".into(), Value::Array(frames));
            reply(self_tx, Value::Object(o));
            return;
        }
    }
    if mtype == "chat_consume" {
        if let Some(acct_raw) = truthy_str(&msg, "account") {
            let account = normalize_account(&acct_raw);
            if !assert_writer(conn, &account) {
                send_auth_reject(self_tx, &msg, "chat_consume");
                return;
            }
            let keys = str_array(&msg, "dedup_keys");
            if consume_chat_mailbox(&mut inner.persist.chat_mailbox, &account, &keys) {
                server.mark_dirty(inner);
            }
            let mut o = Map::new();
            o.insert("type".into(), "chat_ack".into());
            passthrough(&mut o, &msg, "account");
            reply(self_tx, Value::Object(o));
            return;
        }
    }

    // EN: On-demand re-delivery of a conv's stored MLS control (Commits/Welcomes) to the requesting
    // account so a Gate-2 CAS loser deterministically catches up to the winning epoch without waiting
    // for a reconnect mailbox flush (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.3). Only the
    // authenticated account owner may pull ITS OWN mailbox. CN: 按需把某会话已存 MLS 控制（Commit/
    // Welcome）重投到请求账户，使闸二 CAS 落败方确定性追平胜出 epoch，无需等重连邮箱 flush（规范 §3.3）。
    // 仅认证的账户本人可拉取**自己**的邮箱。
    if mtype == "mls_backlog_req" {
        if let (Some(acct_raw), Some(conv)) =
            (truthy_str(&msg, "account"), truthy_str(&msg, "convId"))
        {
            let account = normalize_account(&acct_raw);
            if !assert_writer(conn, &account) {
                send_auth_reject(self_tx, &msg, "mls_backlog_req");
                return;
            }
            prune_mls(inner, &account, now);
            if let Some(b) = inner.persist.mls_mailbox.get(&account) {
                let rows: Vec<Value> = b
                    .values()
                    .filter(|r| r.get("convId").and_then(Value::as_str) == Some(conv.as_str()))
                    .cloned()
                    .collect();
                for row in rows {
                    reply(self_tx, row);
                }
            }
            return;
        }
    }

    // --- control-plane messages (`_ctrl: true`) ---------------------------------------
    if msg.get("_ctrl") == Some(&Value::Bool(true)) {
        let t = get_string(&msg, "t");
        let conv = get_string(&msg, "convId");
        if t == "contact_req"
            && truthy_str(&msg, "toAddr").is_some()
            && truthy_str(&msg, "reqId").is_some()
        {
            store_contact_box(server, inner, &msg, now, true);
        }
        if t == "contact_ack"
            && truthy_str(&msg, "toAddr").is_some()
            && truthy_str(&msg, "reqId").is_some()
        {
            store_contact_box(server, inner, &msg, now, false);
        }
        if t == "group_invite"
            && truthy_str(&msg, "toAddr").is_some()
            && truthy_str(&msg, "inviteId").is_some()
        {
            store_group_invite(server, inner, &msg, now);
        }
        if matches!(t.as_str(), "kp" | "welcome" | "commit" | "mls_ready") && conv.starts_with("d:")
        {
            // EN: Gate 2 — 1:1 Wire-multi-leaf Commit serialization (CHAT_1TO1_WIRE_COMMIT_
            // SERIALIZATION_SPEC §3.2). Only Commits that opt in by carrying a plaintext
            // `commit_epoch` are CAS-arbitrated per `(conv, epoch)`; the legacy 2-leaf handshake
            // Commit (no `commit_epoch`) passes through unchanged. Strictly additive — existing 1:1
            // behavior is byte-identical when the field is absent. A losing Commit is NOT stored or
            // fanned out; the sender is told to catch up via `commit_reject{epoch_stale}`.
            // CN: 闸二——1:1 Wire 多 leaf 的 Commit 串行化（规范 §3.2）。仅对携带明文 `commit_epoch` 的
            // Commit 按 `(conv, epoch)` 做 CAS 仲裁；旧 2-leaf 握手 Commit（无 `commit_epoch`）原样透传。
            // 严格加性——字段缺失时与既有 1:1 行为逐字节一致。落败 Commit 不存储、不扇出；经
            // `commit_reject{epoch_stale}` 通知发送方追平。
            if t == "commit" {
                if let Some(commit_epoch) = msg.get("commit_epoch").and_then(Value::as_u64) {
                    let msg_id = get_string(&msg, "msgId");
                    if let CommitDecision::EpochStale { current_epoch } =
                        try_accept_commit(&mut inner.commit_slots, &conv, commit_epoch, &msg_id)
                    {
                        send_commit_reject(self_tx, &msg, &conv, current_epoch);
                        return;
                    }
                }
            }
            store_mls_control(server, inner, &msg, now);
        }
        // EN: Peer-assisted Add (§3.8) — stamp the AUTHENTICATED sender account so the receiving peer
        // can verify the request truly came from `requester_account`. This relay account-auth stamp is
        // the FIRST gate against cross-account leaf-injection; the receiver ALSO verifies the E2EI
        // in-MLS device-leaf binding inside the KeyPackage (§3.9, account-SS58-key signature over the
        // stable leaf key) which is relay-trustless and which the relay never inspects. Routed to the
        // other conv party and stored for offline catch-up. Drop unauthenticated requests: without a
        // stamped sender the receiver cannot tell a genuine party-device from an impostor, so a forged
        // add could inject an eavesdropping leaf. CN: 对端代 Add（§3.8）——盖章**认证发送者**账户，使接收
        // 对端可校验请求确实来自 `requester_account`。relay 账户盖章是防跨账户注入 leaf 的**第一道**门；
        // 接收方**还**校验 KeyPackage leaf 节点内的 E2EI MLS 内设备 leaf 绑定（§3.9，账户 SS58 钥对稳定
        // leaf key 的签名），该校验 relay-trustless 且 relay 从不检查。路由到会话另一方并存储以便离线补齐。
        // 丢弃未认证请求：无盖章发送者则接收方无法分辨真正的会话方设备与冒充者，伪造 add 可能注入窃听 leaf。
        if t == "peer_add_req" && conv.starts_with("d:") {
            let Some(sender) = conn.account.as_ref().map(|a| normalize_account(a)) else {
                return;
            };
            if let Value::Object(m) = &mut msg {
                m.insert("_senderAccount".into(), Value::String(sender));
            }
            store_mls_control(server, inner, &msg, now);
        }
        // EN: Device-subgroup control transport (Track B, off-chain, account-scoped). `s:<account>`
        // carries the device subgroup's Welcome/Commit/new_device_state, fanned to ALL of that
        // account's own devices (incl. offline catch-up via the MLS mailbox flush on reconnect).
        // It ALSO carries Gate-1 commit intent routing (`commit_intent`/`commit_result`, driving CD
        // election) and `presence`. Never touches the chain; payload is opaque to the relay. CN: 设备
        // 子群控制传输（路线 B，链下、账户内）。`s:<account>` 承载设备子群 Welcome/Commit/new_device_state，
        // 扇出到该账户所有设备（离线设备经重连时 MLS 邮箱 flush 补齐）；**也**承载闸一意图路由
        // （`commit_intent`/`commit_result`，驱动 CD 选举）与 `presence`。不触链；payload 对 relay 不透明。
        // EN: `handoff-request` / `handoff-grant` (Track A sending-authority online handoff, design
        // §5.2) also ride `s:<account>`: the new device asks its account siblings for sending authority
        // and the old primary returns the §5 receipt + the signing-key bundle SEALED to the requester's
        // device peer key (the relay never sees plaintext). Stored for offline catch-up like the other
        // account-scoped control. CN: `handoff-request` / `handoff-grant`（路线 A 发送权在线交接，设计
        // §5.2）同样走 `s:<account>`：新设备向账户兄弟设备申请发送权，旧主设备返回 §5 收据 + 封装给请求方
        // 设备对端钥的签名钥 bundle（relay 不见明文）。与其它账户内控制一样存储以便离线补齐。
        // EN: `device_join_request`/`device_join_offer`/`device_join_kp` (1:1 + group Wire device-join
        // cascade, CHAT_GROUP_WIREIFY_DESIGN §6/§8) ALSO ride `s:<account>`: a new device announces, the
        // elected CD offers the convs it can graft, the new device returns its KeyPackage(s). Account-
        // scoped + stored for offline catch-up like the rest of the subgroup control. CN:
        // `device_join_request`/`device_join_offer`/`device_join_kp`（1:1 + 群 Wire 设备加入级联，
        // 设计 §6/§8）同样走 `s:<account>`：新设备宣告、当选 CD 提供可嫁接会话、新设备回 KeyPackage。
        // 账户内 + 与其余子群控制一样存储以便离线补齐。
        if matches!(
            t.as_str(),
            "kp" | "welcome"
                | "commit"
                | "new_device_state"
                | "presence"
                | "commit_intent"
                | "commit_result"
                | "device_join_request"
                | "device_join_offer"
                | "device_join_kp"
                | "handoff-request"
                | "handoff-grant"
        ) && conv.starts_with("s:")
        {
            store_mls_control(server, inner, &msg, now);
        }
        if t == "token_req" {
            if let Some(to) = truthy_str(&msg, "toAddr") {
                inner.deliver_to_account(&normalize_account(&to), &ctrl_clone(&msg));
            }
        }
        if t == "token_sig" {
            if let Some(to) = truthy_str(&msg, "toAddr") {
                inner.deliver_to_account(&normalize_account(&to), &ctrl_clone(&msg));
            }
        }
        // EN: DR X3DH one-time-prekey distribution over the control plane (design §19/§21). The relay
        // caches an OWNER's advertised leaf set (`opk_publish` WITHOUT `toAddr`, keyed by the
        // AUTHENTICATED uploader account + device) and single-dispenses one leaf per `opk_fetch` to the
        // initiator EVEN WHILE THE OWNER IS OFFLINE. On a cache miss it forwards the fetch to an online
        // owner, whose live single-leaf reply (`opk_publish` WITH `toAddr`) is routed back to the
        // initiator. Crypto is opaque to the relay — the initiator verifies every leaf's Merkle proof
        // against the on-chain `DeviceOpkRoot` (relay-trustless), so a bogus cache only causes an SPK
        // fallback, never a security break. The cache is in-memory only (parity red line, §21). CN: DR
        // X3DH 一次性预密钥的控制面分发（设计 §19/§21）。relay 缓存**持有者**公告的叶子集合（无 `toAddr`
        // 的 `opk_publish`，按**认证上传者**账户 + 设备索引），并按每个 `opk_fetch` 向发起方单发一条，
        // **即使持有者离线**。缓存未命中时把 fetch 转发给在线持有者，其实时单叶回复（带 `toAddr` 的
        // `opk_publish`）经上方路由回发起方。密码学对 relay 不透明——发起方对链上 `DeviceOpkRoot` 校验每条
        // 叶子的 Merkle 证明（relay-trustless），故伪造缓存只会导致 SPK 回退、绝不破坏安全。缓存仅内存态
        // （parity 红线，§21）。
        if t == "opk_publish" {
            if let Some(account) = conn.account.as_ref().map(|a| normalize_account(a)) {
                if let Some(to) = truthy_str(&msg, "toAddr") {
                    // Live single-leaf reply from an online owner → route to the initiator.
                    inner.deliver_to_account(&normalize_account(&to), &ctrl_clone(&msg));
                } else if let Some(leaves) = msg.get("leaves").and_then(Value::as_array) {
                    // Owner advertisement → cache the unused leaf set for offline serving.
                    let device = get_string(&msg, "device_id");
                    let root = get_string(&msg, "root");
                    if !device.is_empty() && !root.is_empty() {
                        inner.opk_cache_publish(&account, &device, &root, leaves);
                    }
                }
            }
        }
        if t == "opk_fetch" {
            if let Some(initiator) = conn.account.as_ref().map(|a| normalize_account(a)) {
                let device = get_string(&msg, "target_device");
                let owner = conv.strip_prefix("s:").map(normalize_account);
                if let (Some(owner), false) = (owner, device.is_empty()) {
                    if let Some((root, leaf)) = inner.opk_cache_dispense(&owner, &device) {
                        let mut resp = Map::new();
                        resp.insert("_ctrl".into(), Value::Bool(true));
                        resp.insert("t".into(), Value::String("opk_publish".into()));
                        resp.insert("convId".into(), Value::String(conv.clone()));
                        resp.insert("from".into(), Value::String(owner));
                        resp.insert("device_id".into(), Value::String(device));
                        resp.insert("root".into(), Value::String(root));
                        resp.insert("leaves".into(), Value::Array(vec![leaf]));
                        inner.deliver_to_account(&initiator, &Value::Object(resp));
                    } else if owner != initiator {
                        // Cache miss → let an online owner serve live (it replies with
                        // `opk_publish{toAddr}` routed above). An offline owner can't beat the
                        // initiator's fetch timeout, so the fetch is NOT stored — the initiator
                        // simply falls back to the SPK (design §6).
                        inner.deliver_to_account(&owner, &ctrl_clone(&msg));
                    }
                }
            }
        }
        // EN: Group Wire-ification (CHAT_GROUP_WIREIFY_DESIGN §15) — a group device Welcome is delivered
        // to the JOINING device's ACCOUNT (`toAddr`) and stored for offline catch-up, mirroring the 1:1
        // `d:` welcome route. CN: 群 Wire 化（设计 §15）——群设备 Welcome 投递到**加入设备账户**（`toAddr`）
        // 并存储以便离线补齐，与 1:1 `d:` welcome 路由对齐。
        if t == "welcome" && conv.starts_with("g:") {
            store_mls_control(server, inner, &msg, now);
        }
        // EN: legacy group handshake broadcast (`hello`/`kp`/`commit`) + Group Wire-ification peer-assisted
        // Add request (§8.4). `peer_add_req` is stamped with the AUTHENTICATED sender account (the FIRST
        // gate against cross-account leaf injection; the receiving member ALSO verifies the relay-trustless
        // E2EI in-MLS device-leaf binding inside the KeyPackage), then broadcast to connected accounts —
        // the receiver-side authz (is-current-member, not-self) drops non-members. Unauthenticated
        // requests are dropped: without a stamped sender a forged add could inject an eavesdropping leaf.
        // CN: 旧群握手广播（`hello`/`kp`/`commit`）+ 群 Wire 化对端代 Add 请求（§8.4）。`peer_add_req` 盖
        // **认证发送者**账户（防跨账户注入 leaf 的**第一道**门；接收成员**还**校验 KeyPackage 内 relay-trustless
        // 的 E2EI MLS 内设备 leaf 绑定）后广播给已连接账户——接收侧鉴权（是当前成员、非我）丢弃非成员。
        // 未认证请求丢弃：无盖章发送者则伪造 add 可能注入窃听 leaf。
        if matches!(t.as_str(), "hello" | "kp" | "commit" | "peer_add_req")
            && conv.starts_with("g:")
        {
            if t == "peer_add_req" {
                let Some(sender) = conn.account.as_ref().map(|a| normalize_account(a)) else {
                    return;
                };
                if let Value::Object(m) = &mut msg {
                    m.insert("_senderAccount".into(), Value::String(sender));
                }
            }
            let row = ctrl_clone(&msg);
            let accounts: Vec<String> = inner.endpoints_by_account.keys().cloned().collect();
            for a in &accounts {
                inner.deliver_to_account(a, &row);
            }
        }
        return;
    }

    // --- plain MLS / sealed-sender frame ----------------------------------------------
    if msg.get("delivery").is_some_and(|d| !d.is_null()) {
        let ok = verify_delivery_frame(server, inner, &msg, now);
        if !ok {
            if server.cfg.strict_auth {
                reject_frame(self_tx, raw_text, "delivery_rejected");
                return;
            }
            eprintln!(
                "[nexchat-relay] delivery verify failed — fallback to plain MLS frame {}",
                msg.get("convId").and_then(Value::as_str).unwrap_or("")
            );
            if let Value::Object(m) = &mut msg {
                m.remove("delivery");
            }
            if truthy_str(&msg, "senderRef").is_none() {
                if let Some(acc) = &conn.account {
                    let norm = normalize_account(acc);
                    if let Value::Object(m) = &mut msg {
                        m.insert("senderRef".into(), Value::String(norm));
                    }
                }
            }
        }
    }

    let recipients = resolve_frame_recipients(inner, &msg, conn);
    if recipients.is_empty() {
        return;
    }
    let sender_account = conn.account.as_ref().map(|a| normalize_account(a));
    // EN: Track B multi-device echo. When the client opts in via `echoSelf: true`, the frame is
    // ALSO retained in the SENDER account's mailbox so the sender's OTHER devices (offline at send
    // time) catch up on reconnect via `chat_fetch`/flush; online siblings already get it live since
    // live delivery only excludes the sending connection. The sending device dedups its own frame
    // locally by `dedupKey`. Default (no flag) excludes the sender account → single-device parity
    // with the Node relay is preserved. CN: 路线 B 多设备回显。客户端经 `echoSelf: true` 开启时，
    // 帧也留存到「发送方账户」邮箱，使其离线的其它设备重连后经 chat_fetch/flush 补齐；在线兄弟设备
    // 本就通过实时投递收到（仅排除发送连接）。发送设备按 `dedupKey` 本地去重自身回显。默认（无标志）
    // 排除发送方账户 → 保持与 Node relay 的单设备 parity。
    let echo_self = msg
        .get("echoSelf")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store_except = if echo_self {
        None
    } else {
        sender_account.as_deref()
    };
    let wire_bytes = raw_text.len() as u64;
    let planned = plan_chat_frame_stores(
        &inner.persist.chat_mailbox,
        &msg,
        &recipients,
        store_except,
        now,
        wire_bytes,
    );
    let mut stored_any = false;
    for ins in &planned {
        if !server.record(
            inner,
            &JournalOp::ChatStore {
                account: ins.account.clone(),
                dedup_key: ins.dedup_key.clone(),
                row: ins.row.clone(),
            },
        ) {
            eprintln!(
                "[nexchat-relay] chat_store journal failed account={} key={}",
                ins.account, ins.dedup_key
            );
            continue;
        }
        stored_any = true;
        if let Some(b) = inner.persist.chat_mailbox.get_mut(&ins.account) {
            prune_chat_box(b, server.cfg.chat_ttl_ms, now);
            enforce_json_box_cap(b, server.cfg.chat_max_frames, server.cfg.chat_max_bytes);
        }
    }
    if stored_any {
        server.mark_dirty(inner);
    }
    let except = msg
        .get("_from")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| conn.id.clone());
    inner.deliver_frame_to_accounts(&msg, &recipients, except.as_deref());
}

fn build_stats(inner: &Inner) -> Value {
    let chat = chat_mailbox_stats(&inner.persist.chat_mailbox);
    let spent_tokens: usize = inner.persist.spent_by_inbox.values().map(|s| s.len()).sum();
    let mut o = Map::new();
    o.insert(
        "indexPointers".into(),
        Value::from(inner.persist.index_pointers.len()),
    );
    o.insert(
        "contactsPointers".into(),
        Value::from(inner.persist.contacts_pointers.len()),
    );
    o.insert(
        "msgArchivePointers".into(),
        Value::from(inner.persist.msg_archive_pointers.len()),
    );
    o.insert(
        "mlsVaultPointers".into(),
        Value::from(inner.persist.mls_vault_pointers.len()),
    );
    o.insert(
        "handoffPointers".into(),
        Value::from(inner.persist.handoff_pointers.len()),
    );
    o.insert(
        "mlsSigningPointers".into(),
        Value::from(inner.persist.mls_signing_pointers.len()),
    );
    o.insert(
        "inboxes".into(),
        Value::from(inner.persist.inboxes_by_account.len()),
    );
    o.insert(
        "contactMailboxes".into(),
        Value::from(inner.persist.contact_mailbox.len()),
    );
    o.insert(
        "groupInviteMailboxes".into(),
        Value::from(inner.persist.group_invite_mailbox.len()),
    );
    o.insert(
        "mlsMailboxes".into(),
        Value::from(inner.persist.mls_mailbox.len()),
    );
    o.insert("chatMailboxes".into(), Value::from(chat.accounts));
    o.insert("chatFrames".into(), Value::from(chat.frames));
    o.insert("chatMailboxBytes".into(), Value::from(chat.bytes));
    o.insert(
        "chatMaxFramesPerAccount".into(),
        Value::from(chat.max_frames_per_account),
    );
    o.insert(
        "spentInboxes".into(),
        Value::from(inner.persist.spent_by_inbox.len()),
    );
    o.insert("spentTokens".into(), Value::from(spent_tokens));
    o.insert("journalLines".into(), Value::from(inner.journal_lines));
    Value::Object(o)
}

fn handle_pointer_put(
    server: &Server,
    inner: &mut Inner,
    conn: &Conn,
    self_tx: &Tx,
    msg: &Value,
    kind: PtrKind,
) -> bool {
    let (Some(account_raw), Some(cid), Some(updated_at)) = (
        truthy_str(msg, "account"),
        truthy_str(msg, "cid"),
        pos_u64(msg, "updated_at"),
    ) else {
        return false;
    };
    let account = normalize_account(&account_raw);
    if !assert_writer(conn, &account) {
        send_auth_reject(self_tx, msg, kind.op_name());
        return true;
    }
    let prev_at = pointer_map_mut(inner, kind)
        .get(&account)
        .map(|p| p.updated_at);
    if prev_at.is_some_and(|pa| updated_at < pa) {
        let mut o = Map::new();
        o.insert("type".into(), Value::String(kind.reject_name().to_string()));
        passthrough(&mut o, msg, "account");
        o.insert("reason".into(), "stale_updated_at".into());
        if let Some(pa) = prev_at {
            o.insert("updated_at".into(), Value::from(pa));
        }
        reply(self_tx, Value::Object(o));
        return true;
    }
    server.record(inner, &kind.op(account.clone(), cid.clone(), updated_at));
    let mut o = Map::new();
    o.insert("type".into(), Value::String(kind.ack_name().to_string()));
    passthrough(&mut o, msg, "account");
    reply(self_tx, Value::Object(o));
    true
}

fn handle_pointer_fetch(
    server: &Server,
    inner: &Inner,
    conn: &Conn,
    self_tx: &Tx,
    msg: &Value,
    kind: PtrKind,
) -> bool {
    let Some(account_raw) = truthy_str(msg, "account") else {
        return false;
    };
    let account = normalize_account(&account_raw);
    if !assert_reader(server, conn, &account) {
        send_auth_reject(
            self_tx,
            msg,
            kind.reply_name().strip_suffix("_reply").unwrap_or("fetch"),
        );
        return true;
    }
    let ptr = pointer_map_ref(inner, kind).get(&account).cloned();
    let mut o = Map::new();
    o.insert("type".into(), Value::String(kind.reply_name().to_string()));
    passthrough(&mut o, msg, "request_id");
    passthrough(&mut o, msg, "account");
    if let Some(p) = ptr {
        o.insert("cid".into(), Value::String(p.cid));
        o.insert("updated_at".into(), Value::from(p.updated_at));
    }
    reply(self_tx, Value::Object(o));
    true
}

fn handle_inbox_register(
    server: &Server,
    inner: &mut Inner,
    conn: &Conn,
    self_tx: &Tx,
    msg: &Value,
) -> bool {
    let (Some(account_raw), Some(inbox_id)) =
        (truthy_str(msg, "account"), truthy_str(msg, "inbox_id"))
    else {
        return false;
    };
    let account = normalize_account(&account_raw);
    if !assert_writer(conn, &account) {
        send_auth_reject(self_tx, msg, "inbox_register");
        return true;
    }
    let next_epoch = msg.get("epoch").and_then(Value::as_u64).unwrap_or(0);
    let prev = inner.persist.inboxes_by_account.get(&account).cloned();

    if let Some(p) = &prev {
        if next_epoch < p.epoch {
            let mut o = Map::new();
            o.insert("type".into(), "inbox_reject".into());
            passthrough(&mut o, msg, "account");
            o.insert("reason".into(), "stale_epoch".into());
            o.insert("epoch".into(), Value::from(p.epoch));
            reply(self_tx, Value::Object(o));
            return true;
        }
    }
    if let Some(p) = &prev {
        if next_epoch > p.epoch || p.inbox_id != inbox_id {
            server.record(
                inner,
                &JournalOp::SpentClear {
                    inbox_id: p.inbox_id.clone(),
                },
            );
        }
    }

    let rec = InboxRecord {
        inbox_id: inbox_id.clone(),
        epoch: next_epoch,
        ipk_n: get_string(msg, "ipk_n"),
        ipk_e: get_string(msg, "ipk_e"),
        revoked_tags: str_array(msg, "revoked_tags"),
    };
    server.record(
        inner,
        &JournalOp::InboxRegister {
            account: account.clone(),
            inbox_id: rec.inbox_id.clone(),
            epoch: rec.epoch,
            ipk_n: rec.ipk_n.clone(),
            ipk_e: rec.ipk_e.clone(),
            revoked_tags: rec.revoked_tags.clone(),
        },
    );

    if let Some(p) = &prev {
        if !p.inbox_id.is_empty() && p.inbox_id != inbox_id {
            inner.inbox_by_id.remove(&p.inbox_id);
            inner.account_by_inbox_id.remove(&p.inbox_id);
        }
    }
    inner.inbox_by_id.insert(inbox_id.clone(), rec);
    inner.account_by_inbox_id.insert(inbox_id, account);

    let mut o = Map::new();
    o.insert("type".into(), "inbox_ack".into());
    passthrough(&mut o, msg, "account");
    reply(self_tx, Value::Object(o));
    true
}

/// EN: Clone a control row with `_ctrl: true` ensured. CN: 克隆控制行并确保 _ctrl: true。
fn ctrl_clone(msg: &Value) -> Value {
    let mut row = msg.clone();
    if let Value::Object(m) = &mut row {
        m.insert("_ctrl".into(), Value::Bool(true));
    }
    row
}

/// EN: Control row with normalized `toAddr` + `stored_at` + `bytes`. CN: 带 toAddr、stored_at、bytes 的控制行。
fn ctrl_row(msg: &Value, to_addr: &str, now: u64, wire_bytes: u64) -> Value {
    let mut row = msg.clone();
    if let Value::Object(m) = &mut row {
        m.insert("toAddr".into(), Value::String(to_addr.to_string()));
        m.insert("_ctrl".into(), Value::Bool(true));
        m.insert("stored_at".into(), Value::from(now));
        m.insert("bytes".into(), Value::from(wire_bytes));
    }
    row
}

fn store_contact_box(server: &Server, inner: &mut Inner, msg: &Value, now: u64, is_req: bool) {
    let Some(to_raw) = truthy_str(msg, "toAddr") else {
        return;
    };
    let Some(req_id) = truthy_str(msg, "reqId") else {
        return;
    };
    let to = normalize_account(&to_raw);
    let wire_bytes = msg.to_string().len() as u64;
    let row = ctrl_row(msg, &to, now, wire_bytes);
    {
        let b = inner.persist.contact_mailbox.entry(to.clone()).or_default();
        let bucket = if is_req { &mut b.reqs } else { &mut b.acks };
        if bucket.contains_key(&req_id) {
            return;
        }
        bucket.insert(req_id, row.clone());
        enforce_contact_cap(b, server.cfg.contact_max_entries);
    }
    inner.deliver_to_account(&to, &row);
    server.mark_dirty(inner);
}

fn store_group_invite(server: &Server, inner: &mut Inner, msg: &Value, now: u64) {
    let Some(to_raw) = truthy_str(msg, "toAddr") else {
        return;
    };
    let Some(invite_id) = truthy_str(msg, "inviteId") else {
        return;
    };
    let to = normalize_account(&to_raw);
    let wire_bytes = msg.to_string().len() as u64;
    let row = ctrl_row(msg, &to, now, wire_bytes);
    {
        let b = inner
            .persist
            .group_invite_mailbox
            .entry(to.clone())
            .or_default();
        if b.contains_key(&invite_id) {
            return;
        }
        b.insert(invite_id, row.clone());
    }
    inner.deliver_to_account(&to, &row);
    server.mark_dirty(inner);
}

/// EN: Direct-MLS control recipient resolution (`mlsControlRecipient`). CN: 1:1 MLS 控制收件人解析。
fn mls_control_recipient(msg: &Value) -> Option<String> {
    let conv = msg.get("convId").and_then(Value::as_str)?;
    // EN: Device-subgroup control `s:<account>` → recipient is that account (all its devices).
    // CN: 设备子群控制 `s:<account>` → 收件人为该账户（其全部设备）。
    if let Some(acct) = conv.strip_prefix("s:") {
        let t = msg.get("t").and_then(Value::as_str).unwrap_or("");
        // EN: account self channel also carries Gate-1 intent routing (`commit_intent`/`commit_result`,
        // driving CD election) and `presence`. CN: 账户自通道还承载闸一意图路由（`commit_intent`/
        // `commit_result`，驱动 CD 选举）与 `presence`。
        if !acct.is_empty()
            && matches!(
                t,
                "kp" | "welcome"
                    | "commit"
                    | "new_device_state"
                    | "presence"
                    | "commit_intent"
                    | "commit_result"
                    | "device_join_request"
                    | "device_join_offer"
                    | "device_join_kp"
                    | "handoff-request"
                    | "handoff-grant"
            )
        {
            return Some(normalize_account(acct));
        }
        return None;
    }
    // EN: Group Wire-ification — a group device Welcome is delivered (and mailbox-stored) to the joining
    // device's account (`toAddr`); other group control (hello/kp/commit/peer_add_req) is broadcast in the
    // handler, not mailbox-routed. CN: 群 Wire 化——群设备 Welcome 投递（并入信箱）到加入设备账户（`toAddr`）；
    // 其它群控制（hello/kp/commit/peer_add_req）在 handler 内广播，不走信箱路由。
    if let Some(group) = conv.strip_prefix("g:") {
        let t = msg.get("t").and_then(Value::as_str).unwrap_or("");
        if !group.is_empty() && t == "welcome" {
            return truthy_str(msg, "toAddr").map(|a| normalize_account(&a));
        }
        return None;
    }
    if !conv.starts_with("d:") {
        return None;
    }
    let parts: Vec<&str> = conv[2..].split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let (owner, member) = (parts[0], parts[1]);
    let t = msg.get("t").and_then(Value::as_str).unwrap_or("");
    match t {
        "kp" if truthy_str(msg, "identity").is_some() => Some(normalize_account(owner)),
        "welcome" => truthy_str(msg, "toAddr").map(|a| normalize_account(&a)),
        "commit" => Some(normalize_account(member)),
        "mls_ready" => Some(normalize_account(owner)),
        // EN: peer-assisted Add (§3.8) → recipient is the OTHER conv party (the one asked to add).
        // CN: 对端代 Add（§3.8）→ 收件人是会话**另一方**（被请求执行 add 的一方）。
        "peer_add_req" => {
            let req = truthy_str(msg, "requester_account").map(|a| normalize_account(&a))?;
            let o = normalize_account(owner);
            let m = normalize_account(member);
            if req == o {
                Some(m)
            } else if req == m {
                Some(o)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn mls_dedup_key(msg: &Value) -> String {
    let t = msg.get("t").and_then(Value::as_str).unwrap_or("");
    let conv = msg.get("convId").and_then(Value::as_str).unwrap_or("");
    // EN: Frames carrying an explicit unique `msgId` (device-subgroup Commits, where many share
    // the same (t, conv)) dedup on it to avoid collapsing distinct messages into one slot.
    // CN: 带显式唯一 `msgId` 的帧（设备子群 Commit，多条共享同一 (t, conv)）按它去重，避免不同
    // 消息折叠到同一槽位。
    if let Some(id) = msg
        .get("msgId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return format!("{t}:{conv}:{id}");
    }
    // EN: per-(conv) dedup slots for the multi-leaf control types so distinct rounds don't collapse:
    // presence by device, intent/result by request id, peer_add_req by joining device. CN: 多 leaf 控制
    // 类型的按 (conv) 去重槽，避免不同轮次折叠：presence 按设备、intent/result 按请求 id、peer_add_req
    // 按加入设备。
    if t == "presence" {
        if let Some(dev) = truthy_str(msg, "device_id") {
            return format!("presence:{conv}:{dev}");
        }
    }
    if t == "commit_intent" {
        if let Some(req) = truthy_str(msg, "req_id") {
            return format!("commit_intent:{conv}:{req}");
        }
    }
    if t == "commit_result" {
        if let Some(req) = truthy_str(msg, "req_id") {
            return format!("commit_result:{conv}:{req}");
        }
    }
    if t == "peer_add_req" {
        if let Some(dev) = truthy_str(msg, "device_id") {
            return format!("peer_add_req:{conv}:{dev}");
        }
    }
    // EN: device-join cascade dedups per (conv, device) so concurrent siblings' requests/offers/kps each
    // keep their own mailbox slot. CN: 设备加入级联按 (conv, 设备) 去重，使并发兄弟设备的 请求/offer/kp 各
    // 自占信箱槽。
    if matches!(
        t,
        "device_join_request" | "device_join_offer" | "device_join_kp"
    ) {
        if let Some(dev) = truthy_str(msg, "device_id") {
            return format!("{t}:{conv}:{dev}");
        }
    }
    // EN: handoff frames dedup per requesting/target device so concurrent multi-device handoffs don't
    // collapse into one mailbox slot. CN: 交接帧按请求/目标设备去重，避免多设备并发交接折叠到同一信箱槽。
    if t == "handoff-request" {
        if let Some(dev) = truthy_str(msg, "from") {
            return format!("handoff-request:{conv}:{dev}");
        }
    }
    if t == "handoff-grant" {
        if let Some(dev) = truthy_str(msg, "to") {
            return format!("handoff-grant:{conv}:{dev}");
        }
    }
    let identity = msg.get("identity").and_then(Value::as_str).unwrap_or("");
    let to_addr = msg.get("toAddr").and_then(Value::as_str).unwrap_or("");
    format!("{t}:{conv}:{identity}:{to_addr}")
}

fn prune_mls(inner: &mut Inner, acc: &str, now: u64) {
    if let Some(b) = inner.persist.mls_mailbox.get_mut(acc) {
        prune_ttl(b, MLS_CTRL_TTL_MS, now);
        if b.is_empty() {
            inner.persist.mls_mailbox.remove(acc);
        }
    }
}

fn store_mls_control(server: &Server, inner: &mut Inner, msg: &Value, now: u64) {
    let Some(account) = mls_control_recipient(msg) else {
        return;
    };
    let acc = normalize_account(&account);
    prune_mls(inner, &acc, now);
    let key = mls_dedup_key(msg);
    let wire_bytes = msg.to_string().len() as u64;
    let mut row = msg.clone();
    if let Value::Object(m) = &mut row {
        m.insert("_ctrl".into(), Value::Bool(true));
        m.insert("stored_at".into(), Value::from(now));
        m.insert("bytes".into(), Value::from(wire_bytes));
    }
    {
        let b = inner.persist.mls_mailbox.entry(acc.clone()).or_default();
        b.insert(key, row.clone());
        enforce_json_box_cap(b, server.cfg.mls_max_frames, server.cfg.mls_max_bytes);
    }
    inner.deliver_to_account(&acc, &row);
    server.mark_dirty(inner);
}

fn flush_mls_mailbox(inner: &mut Inner, account: &str, now: u64) {
    let acc = normalize_account(account);
    prune_mls(inner, &acc, now);
    let rows: Vec<Value> = match inner.persist.mls_mailbox.get(&acc) {
        Some(b) => b.values().cloned().collect(),
        None => return,
    };
    for row in &rows {
        inner.deliver_to_account(&acc, row);
    }
}

fn flush_chat_mailbox(server: &Server, inner: &mut Inner, account: &str, now: u64) {
    let acc = normalize_account(account);
    let mut wires: Vec<Value> = vec![];
    let present = inner.persist.chat_mailbox.contains_key(&acc);
    if present {
        let empty = if let Some(b) = inner.persist.chat_mailbox.get_mut(&acc) {
            prune_chat_box(b, server.cfg.chat_ttl_ms, now);
            b.is_empty()
        } else {
            false
        };
        if empty {
            inner.persist.chat_mailbox.remove(&acc);
        } else if let Some(b) = inner.persist.chat_mailbox.get(&acc) {
            wires = b
                .values()
                .filter(|r| !frame_expired(r, now))
                .map(chat_row_to_wire)
                .collect();
        }
    }
    for w in &wires {
        inner.deliver_to_account(&acc, w);
    }
    server.mark_dirty(inner);
}

fn resolve_frame_recipients(inner: &Inner, msg: &Value, conn: &Conn) -> BTreeSet<String> {
    let mut accounts = BTreeSet::new();
    if let Some(d) = msg.get("delivery") {
        if let Some(inbox_id) = truthy_str(d, "inboxId") {
            if let Some(owner) = inner.account_by_inbox_id.get(&inbox_id) {
                accounts.insert(owner.clone());
            }
        }
    }
    if let Some(arr) = msg.get("routeTo").and_then(Value::as_array) {
        for v in arr {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    accounts.insert(normalize_account(s));
                }
            }
        }
    }
    if let Some(conv) = msg.get("convId").and_then(Value::as_str) {
        if let Some(rest) = conv.strip_prefix("d:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                accounts.insert(normalize_account(parts[0]));
                accounts.insert(normalize_account(parts[1]));
            } else if parts.len() == 1 && !parts[0].is_empty() {
                accounts.insert(normalize_account(parts[0]));
                if let Some(a) = &conn.account {
                    accounts.insert(a.clone());
                }
            }
        }
    }
    accounts
}

fn verify_delivery_frame(server: &Server, inner: &mut Inner, msg: &Value, now: u64) -> bool {
    let _ = now;
    let Some(d) = msg.get("delivery") else {
        return false;
    };
    let dbg = server.cfg.debug;
    let (Some(inbox_id), Some(t), Some(s), Some(ct)) = (
        truthy_str(d, "inboxId"),
        truthy_str(d, "t"),
        truthy_str(d, "s"),
        truthy_str(d, "ct"),
    ) else {
        if dbg {
            eprintln!("[relay] delivery reject: missing fields");
        }
        return false;
    };

    let (ib_epoch, ib_revoked, ib_ipk_n, ib_ipk_e) = match inner.inbox_by_id.get(&inbox_id) {
        Some(ib) => (
            ib.epoch,
            ib.revoked_tags.clone(),
            ib.ipk_n.clone(),
            ib.ipk_e.clone(),
        ),
        None => {
            if dbg {
                eprintln!("[relay] delivery reject: unknown inbox {inbox_id}");
            }
            return false;
        }
    };
    if d.get("epoch").and_then(Value::as_u64) != Some(ib_epoch) {
        if dbg {
            eprintln!("[relay] delivery reject: epoch");
        }
        return false;
    }
    if ib_revoked.iter().any(|r| r == &ct) {
        if dbg {
            eprintln!("[relay] delivery reject: revoked ct");
        }
        return false;
    }
    if inner
        .persist
        .spent_by_inbox
        .get(&inbox_id)
        .is_some_and(|set| set.contains(&t))
    {
        if dbg {
            eprintln!("[relay] delivery reject: spent t");
        }
        return false;
    }
    let Some(p) = truthy_str(d, "p") else {
        if dbg {
            eprintln!("[relay] delivery reject: missing p");
        }
        return false;
    };
    let ipk_n = truthy_str(d, "ipkN").unwrap_or(ib_ipk_n);
    let ipk_e = truthy_str(d, "ipkE").unwrap_or(ib_ipk_e);
    if !verify_delivery_token(&DeliveryToken {
        ipk_n: &ipk_n,
        ipk_e: &ipk_e,
        s: &s,
        p: &p,
    }) {
        if dbg {
            eprintln!("[relay] delivery reject: bad signature");
        }
        return false;
    }
    if !server.record_spent(inner, &inbox_id, &t) {
        if dbg {
            eprintln!("[relay] delivery reject: spent cap or journal failure");
        }
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn recv_text(rx: &mut mpsc::UnboundedReceiver<Message>) -> Option<Value> {
        match rx.try_recv() {
            Ok(Message::Text(t)) => serde_json::from_str(t.as_str()).ok(),
            _ => None,
        }
    }

    #[test]
    fn reject_frame_emits_nack_for_user_frame() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        reject_frame(
            &tx,
            r#"{"convId":"d:a:b","ciphertextB64":"x","dedupKey":"d:a:b:c-1"}"#,
            "rate_limited",
        );
        let v = recv_text(&mut rx).expect("frame_reject emitted");
        assert_eq!(v["type"], "frame_reject");
        assert_eq!(v["reason"], "rate_limited");
        assert_eq!(v["dedupKey"], "d:a:b:c-1");
        assert_eq!(v["convId"], "d:a:b");
    }

    // EN: Gate 2 loser reply (spec §3.3) — `commit_reject{epoch_stale}` carries the conv, the
    // relay's current epoch (to catch up to), and echoes `msgId`. CN: 闸二落败回执——
    // `commit_reject{epoch_stale}` 带 conv、relay 当前 epoch（用于追平）并回显 `msgId`。
    #[test]
    fn send_commit_reject_emits_epoch_stale() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let msg: Value = serde_json::from_str(
            r#"{"_ctrl":true,"t":"commit","convId":"d:a:b","commit_epoch":3,"msgId":"m-7"}"#,
        )
        .unwrap();
        send_commit_reject(&tx, &msg, "d:a:b", 5);
        let v = recv_text(&mut rx).expect("commit_reject emitted");
        assert_eq!(v["type"], "commit_reject");
        assert_eq!(v["reason"], "epoch_stale");
        assert_eq!(v["convId"], "d:a:b");
        assert_eq!(v["current_epoch"], Value::from(5u64));
        assert_eq!(v["msgId"], "m-7");
    }

    #[test]
    fn reject_frame_skips_control_and_keyless_and_invalid() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        // control-plane row
        reject_frame(
            &tx,
            r#"{"_ctrl":true,"dedupKey":"k","t":"hello"}"#,
            "rate_limited",
        );
        // no dedupKey (e.g. register / fetch)
        reject_frame(
            &tx,
            r#"{"type":"register_account","account":"a"}"#,
            "rate_limited",
        );
        // not an object / invalid JSON
        reject_frame(&tx, "not json", "rate_limited");
        reject_frame(&tx, "123", "rate_limited");
        assert!(rx.try_recv().is_err(), "no NACK should be emitted");
    }

    // EN: Track B device-subgroup control `s:<account>` routes to that account (all its devices)
    // for the subgroup message types; unknown conv / type yields no recipient. CN: 路线 B 设备子群
    // 控制 `s:<account>` 对子群消息类型路由到该账户（全部设备）；未知 conv/类型 不产生收件人。
    #[test]
    fn subgroup_control_routes_to_owning_account() {
        for t in ["kp", "welcome", "commit", "new_device_state"] {
            let msg: Value =
                serde_json::from_str(&format!(r#"{{"convId":"s:5GrwvaEF","t":"{t}"}}"#)).unwrap();
            assert_eq!(
                mls_control_recipient(&msg).as_deref(),
                Some(normalize_account("5GrwvaEF")).as_deref()
            );
        }
        // unrelated control type on a subgroup conv → not routed
        let other: Value =
            serde_json::from_str(r#"{"convId":"s:5GrwvaEF","t":"mls_ready"}"#).unwrap();
        assert_eq!(mls_control_recipient(&other), None);
        // empty account → not routed
        let empty: Value = serde_json::from_str(r#"{"convId":"s:","t":"commit"}"#).unwrap();
        assert_eq!(mls_control_recipient(&empty), None);
    }

    // EN: Subgroup Commits sharing (t, conv) must NOT collapse: `msgId` makes dedup keys distinct;
    // legacy frames without `msgId` keep the old identity:toAddr key. CN: 共享 (t,conv) 的子群
    // Commit 不得折叠：`msgId` 使去重键互异；无 `msgId` 的旧帧仍用 identity:toAddr 键。
    #[test]
    fn mls_dedup_key_uses_msg_id_when_present() {
        let a: Value =
            serde_json::from_str(r#"{"t":"commit","convId":"s:x","msgId":"m-1"}"#).unwrap();
        let b: Value =
            serde_json::from_str(r#"{"t":"commit","convId":"s:x","msgId":"m-2"}"#).unwrap();
        assert_ne!(mls_dedup_key(&a), mls_dedup_key(&b));
        assert_eq!(mls_dedup_key(&a), "commit:s:x:m-1");
        // legacy path (no msgId) unchanged
        let legacy: Value =
            serde_json::from_str(r#"{"t":"welcome","convId":"d:o:m","toAddr":"m"}"#).unwrap();
        assert_eq!(mls_dedup_key(&legacy), "welcome:d:o:m::m");
    }

    // EN: Gate-1 — the account self channel `s:<account>` ALSO routes commit intent (CD election) and
    // presence to that account's devices (parity with the Node relay). CN: 闸一——账户自通道
    // `s:<account>` **也**把 commit 意图（CD 选举）与 presence 路由到该账户设备（与 Node relay 对齐）。
    #[test]
    fn subgroup_control_routes_gate1_intent_and_presence() {
        for t in ["presence", "commit_intent", "commit_result"] {
            let msg: Value =
                serde_json::from_str(&format!(r#"{{"convId":"s:5GrwvaEF","t":"{t}"}}"#)).unwrap();
            assert_eq!(
                mls_control_recipient(&msg).as_deref(),
                Some(normalize_account("5GrwvaEF")).as_deref(),
                "type {t} must route on the account self channel"
            );
        }
    }

    // EN: Track A sending-authority handoff (§5.2) — `handoff-request`/`handoff-grant` on `s:<account>`
    // route to (and are stored for) that account's devices, with per-device dedup so concurrent handoffs
    // don't collapse. CN: 路线 A 发送权交接（§5.2）——`s:<account>` 上的 `handoff-request`/`handoff-grant`
    // 路由到（并存储给）该账户设备，按设备去重避免并发交接折叠。
    #[test]
    fn handoff_control_routes_and_dedups_per_device() {
        for t in ["handoff-request", "handoff-grant"] {
            let msg: Value =
                serde_json::from_str(&format!(r#"{{"convId":"s:5GrwvaEF","t":"{t}"}}"#)).unwrap();
            assert_eq!(
                mls_control_recipient(&msg).as_deref(),
                Some(normalize_account("5GrwvaEF")).as_deref(),
                "type {t} must route on the account self channel"
            );
        }
        let req_a: Value =
            serde_json::from_str(r#"{"convId":"s:5A","t":"handoff-request","from":"devA"}"#)
                .unwrap();
        let req_b: Value =
            serde_json::from_str(r#"{"convId":"s:5A","t":"handoff-request","from":"devB"}"#)
                .unwrap();
        assert_ne!(mls_dedup_key(&req_a), mls_dedup_key(&req_b));
        let grant_a: Value =
            serde_json::from_str(r#"{"convId":"s:5A","t":"handoff-grant","to":"devA"}"#).unwrap();
        let grant_b: Value =
            serde_json::from_str(r#"{"convId":"s:5A","t":"handoff-grant","to":"devB"}"#).unwrap();
        assert_ne!(mls_dedup_key(&grant_a), mls_dedup_key(&grant_b));
    }

    // EN: Peer-assisted Add (§3.8) — `peer_add_req` on `d:owner:member` routes to the OTHER party
    // (the one asked to perform the add): requester==owner → member, requester==member → owner; a
    // third-party requester or a missing one yields no recipient. CN: 对端代 Add（§3.8）——`d:owner:member`
    // 上的 `peer_add_req` 路由到**另一方**（被请求执行 add 者）：请求方==owner → member，请求方==member →
    // owner；第三方或缺失请求方 → 无收件人。
    #[test]
    fn peer_add_req_routes_to_the_other_conv_party() {
        let to_member: Value = serde_json::from_str(
            r#"{"convId":"d:owner:member","t":"peer_add_req","requester_account":"owner"}"#,
        )
        .unwrap();
        assert_eq!(
            mls_control_recipient(&to_member).as_deref(),
            Some(normalize_account("member")).as_deref()
        );

        let to_owner: Value = serde_json::from_str(
            r#"{"convId":"d:owner:member","t":"peer_add_req","requester_account":"member"}"#,
        )
        .unwrap();
        assert_eq!(
            mls_control_recipient(&to_owner).as_deref(),
            Some(normalize_account("owner")).as_deref()
        );

        // requester is neither conv party → not routed (no cross-conv injection).
        let third: Value = serde_json::from_str(
            r#"{"convId":"d:owner:member","t":"peer_add_req","requester_account":"stranger"}"#,
        )
        .unwrap();
        assert_eq!(mls_control_recipient(&third), None);

        // missing requester_account → not routed.
        let noreq: Value =
            serde_json::from_str(r#"{"convId":"d:owner:member","t":"peer_add_req"}"#).unwrap();
        assert_eq!(mls_control_recipient(&noreq), None);
    }

    // EN: Dedup slots for the multi-leaf control types so distinct rounds don't collapse into one
    // mailbox entry; each falls back to the legacy identity:toAddr key when its id field is absent.
    // CN: 多 leaf 控制类型的去重槽，避免不同轮次折叠到同一邮箱项；缺对应 id 字段时回退旧 identity:toAddr 键。
    #[test]
    fn mls_dedup_key_for_multileaf_control_types() {
        let presence: Value =
            serde_json::from_str(r#"{"t":"presence","convId":"s:x","device_id":"devA"}"#).unwrap();
        assert_eq!(mls_dedup_key(&presence), "presence:s:x:devA");

        let intent: Value =
            serde_json::from_str(r#"{"t":"commit_intent","convId":"s:x","req_id":"r1"}"#).unwrap();
        assert_eq!(mls_dedup_key(&intent), "commit_intent:s:x:r1");

        let result: Value =
            serde_json::from_str(r#"{"t":"commit_result","convId":"s:x","req_id":"r1"}"#).unwrap();
        assert_eq!(mls_dedup_key(&result), "commit_result:s:x:r1");
        // distinct intent and result for the SAME req_id never collide.
        assert_ne!(mls_dedup_key(&intent), mls_dedup_key(&result));

        let peer: Value =
            serde_json::from_str(r#"{"t":"peer_add_req","convId":"d:o:m","device_id":"devNew"}"#)
                .unwrap();
        assert_eq!(mls_dedup_key(&peer), "peer_add_req:d:o:m:devNew");

        // fallback when the id field is missing (e.g. malformed presence) → legacy identity:toAddr key.
        let nodev: Value = serde_json::from_str(r#"{"t":"presence","convId":"s:x"}"#).unwrap();
        assert_eq!(mls_dedup_key(&nodev), "presence:s:x::");
    }

    // EN: Device-join cascade (CHAT_GROUP_WIREIFY_DESIGN §6/§8, shared by 1:1 + group Wire) rides
    // `s:<account>`: announce → offer → kp must route to (and be stored for) that account's devices, with
    // per-(conv, device) dedup so concurrent siblings don't collapse. CN: 设备加入级联（设计 §6/§8，1:1 与
    // 群 Wire 共用）走 `s:<account>`：宣告 → offer → kp 须路由到（并存储给）该账户设备，按 (conv, 设备) 去重
    // 避免并发兄弟设备折叠。
    #[test]
    fn device_join_cascade_routes_and_dedups_per_device() {
        for t in ["device_join_request", "device_join_offer", "device_join_kp"] {
            let msg: Value =
                serde_json::from_str(&format!(r#"{{"convId":"s:5GrwvaEF","t":"{t}"}}"#)).unwrap();
            assert_eq!(
                mls_control_recipient(&msg).as_deref(),
                Some(normalize_account("5GrwvaEF")).as_deref(),
                "type {t} must route on the account self channel"
            );
        }
        let req_a: Value = serde_json::from_str(
            r#"{"convId":"s:5A","t":"device_join_request","device_id":"devA"}"#,
        )
        .unwrap();
        let req_b: Value = serde_json::from_str(
            r#"{"convId":"s:5A","t":"device_join_request","device_id":"devB"}"#,
        )
        .unwrap();
        assert_ne!(mls_dedup_key(&req_a), mls_dedup_key(&req_b));
        assert_eq!(mls_dedup_key(&req_a), "device_join_request:s:5A:devA");
    }

    // EN: Group Wire-ification (§15) — a group device Welcome is mailbox-routed to the JOINING device's
    // account (`toAddr`); other group control (hello/kp/commit/peer_add_req) is broadcast in the handler,
    // so it yields no single mailbox recipient here. CN: 群 Wire 化（§15）——群设备 Welcome 按信箱路由到
    // **加入设备账户**（`toAddr`）；其它群控制（hello/kp/commit/peer_add_req）在 handler 内广播，故此处无
    // 单一信箱收件人。
    #[test]
    fn group_welcome_routes_to_joining_account() {
        let welcome: Value = serde_json::from_str(
            r#"{"convId":"g:7","t":"welcome","toAddr":"5GrwvaEF","welcome":"x"}"#,
        )
        .unwrap();
        assert_eq!(
            mls_control_recipient(&welcome).as_deref(),
            Some(normalize_account("5GrwvaEF")).as_deref()
        );
        // a group Welcome without `toAddr` cannot be mailbox-routed.
        let noaddr: Value = serde_json::from_str(r#"{"convId":"g:7","t":"welcome"}"#).unwrap();
        assert_eq!(mls_control_recipient(&noaddr), None);
        // group hello/kp/commit/peer_add_req are broadcast in the handler, not mailbox-routed.
        for t in ["hello", "kp", "commit", "peer_add_req"] {
            let msg: Value =
                serde_json::from_str(&format!(r#"{{"convId":"g:7","t":"{t}"}}"#)).unwrap();
            assert_eq!(
                mls_control_recipient(&msg),
                None,
                "group {t} must not mailbox-route"
            );
        }
    }

    // EN: OPK-over-relay (design §19/§21) — an owner uploads its leaf set, the relay single-dispenses
    // one leaf per `opk_fetch` to the INITIATOR's account (not the owner's) even while the owner is
    // idle, and on cache exhaustion forwards the fetch to the (online) owner for a live reply. CN:
    // OPK-over-relay（设计 §19/§21）——持有者上传叶子集合，relay 每次 `opk_fetch` 向**发起方**账户单发
    // 一条（而非持有者），即使持有者空闲；缓存耗尽时把 fetch 转发给（在线）持有者以便实时回复。
    #[test]
    fn opk_over_relay_caches_and_dispenses_to_initiator() {
        use crate::config::Config;
        use crate::state::Server;
        use std::fs;
        use std::sync::Arc;

        const ALICE: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
        const BOB: &str = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";

        let data_dir = std::env::temp_dir().join(format!("relay-opk-proc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).unwrap();
        let mut cfg = Config::from_env();
        cfg.data_dir = data_dir.to_string_lossy().into_owned();
        let server = Arc::new(Server::load(cfg).unwrap());

        // Owner (Alice) and initiator (Bob), both registered over loopback (sig check skipped).
        let (owner_tx, mut owner_rx) = mpsc::unbounded_channel();
        let mut owner = Conn {
            id: None,
            account: None,
            is_loopback: true,
        };
        process(
            &server,
            &mut owner,
            &owner_tx,
            r#"{"type":"register","id":"ownerEp"}"#,
        );
        process(
            &server,
            &mut owner,
            &owner_tx,
            &format!(r#"{{"type":"register_account","id":"ownerEp","account":"{ALICE}"}}"#),
        );
        while owner_rx.try_recv().is_ok() {} // drain mailbox-flush noise

        let (init_tx, mut init_rx) = mpsc::unbounded_channel();
        let mut init = Conn {
            id: None,
            account: None,
            is_loopback: true,
        };
        process(
            &server,
            &mut init,
            &init_tx,
            r#"{"type":"register","id":"initEp"}"#,
        );
        process(
            &server,
            &mut init,
            &init_tx,
            &format!(r#"{{"type":"register_account","id":"initEp","account":"{BOB}"}}"#),
        );
        while init_rx.try_recv().is_ok() {}

        // Owner advertises two leaves on its self channel.
        let dev = "deadbeefdeadbeefdeadbeefdeadbeef";
        process(
            &server,
            &mut owner,
            &owner_tx,
            &format!(
                r#"{{"_ctrl":true,"t":"opk_publish","convId":"s:{ALICE}","from":"{ALICE}","device_id":"{dev}","root":"abcd","leaves":[{{"opk_pub":"aa","proof":"p0"}},{{"opk_pub":"bb","proof":"p1"}}]}}"#
            ),
        );
        assert!(
            owner_rx.try_recv().is_err(),
            "upload yields no reply to owner"
        );

        // Initiator fetches → gets the FIRST leaf, delivered to Bob (the initiator), not Alice.
        let fetch = format!(
            r#"{{"_ctrl":true,"t":"opk_fetch","convId":"s:{ALICE}","from":"{BOB}","target_device":"{dev}"}}"#
        );
        process(&server, &mut init, &init_tx, &fetch);
        let v = recv_text(&mut init_rx).expect("initiator receives opk_publish");
        assert_eq!(v["t"], "opk_publish");
        assert_eq!(v["device_id"], dev);
        assert_eq!(v["root"], "abcd");
        assert_eq!(v["from"], normalize_account(ALICE));
        assert_eq!(v["leaves"][0]["opk_pub"], "aa");
        assert_eq!(v["leaves"].as_array().unwrap().len(), 1, "single-dispense");
        assert!(
            owner_rx.try_recv().is_err(),
            "cache hit does not disturb owner"
        );

        // Second fetch → second leaf.
        process(&server, &mut init, &init_tx, &fetch);
        let v = recv_text(&mut init_rx).expect("second leaf");
        assert_eq!(v["leaves"][0]["opk_pub"], "bb");

        // Third fetch → cache exhausted → relay forwards the fetch to the (online) owner.
        process(&server, &mut init, &init_tx, &fetch);
        assert!(init_rx.try_recv().is_err(), "no leaf left for initiator");
        let fwd = recv_text(&mut owner_rx).expect("fetch forwarded to owner on cache miss");
        assert_eq!(fwd["t"], "opk_fetch");
        assert_eq!(fwd["target_device"], dev);

        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn register_account_rejects_when_endpoint_id_not_registered_on_conn() {
        use crate::config::Config;
        use crate::state::Server;
        use std::fs;
        use std::sync::Arc;

        let data_dir = std::env::temp_dir().join(format!("relay-reg-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).unwrap();
        let mut cfg = Config::from_env();
        cfg.data_dir = data_dir.to_string_lossy().into_owned();
        let server = Arc::new(Server::load(cfg).unwrap());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut conn = Conn {
            id: None,
            account: None,
            is_loopback: false,
        };

        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"register_account","id":"dev1","account":"5Alice"}"#,
        );
        let v = recv_text(&mut rx).expect("auth_reject");
        assert_eq!(v["type"], "auth_reject");
        assert_eq!(v["op"], "register_account");
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn register_account_rebinds_endpoint_when_account_changes() {
        use crate::config::Config;
        use crate::state::Server;
        use std::fs;
        use std::sync::Arc;

        let data_dir =
            std::env::temp_dir().join(format!("relay-rebind-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).unwrap();
        let mut cfg = Config::from_env();
        cfg.data_dir = data_dir.to_string_lossy().into_owned();
        let server = Arc::new(Server::load(cfg).unwrap());
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut conn = Conn {
            id: None,
            account: None,
            is_loopback: true,
        };

        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"register","id":"dev1"}"#,
        );
        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"register_account","id":"dev1","account":"5Alice"}"#,
        );
        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"register_account","id":"dev1","account":"5Bob"}"#,
        );

        let inner = server.inner.lock().unwrap();
        assert!(!inner.endpoints_by_account.contains_key("5Alice"));
        assert!(inner
            .endpoints_by_account
            .get("5Bob")
            .unwrap()
            .contains("dev1"));
        assert_eq!(conn.account.as_deref(), Some("5Bob"));
        let _ = fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn pointer_put_rejects_stale_updated_at() {
        use crate::config::Config;
        use crate::state::Server;
        use std::fs;
        use std::sync::Arc;

        let data_dir = std::env::temp_dir().join(format!("relay-ptr-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).unwrap();
        let mut cfg = Config::from_env();
        cfg.data_dir = data_dir.to_string_lossy().into_owned();
        let server = Arc::new(Server::load(cfg).unwrap());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut conn = Conn {
            id: Some("dev1".into()),
            account: Some("5Alice".into()),
            is_loopback: true,
        };

        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"index_put","account":"5Alice","cid":"bafyA","updated_at":10}"#,
        );
        let _ = rx.try_recv();

        process(
            &server,
            &mut conn,
            &tx,
            r#"{"type":"index_put","account":"5Alice","cid":"bafyB","updated_at":5}"#,
        );
        let v = recv_text(&mut rx).expect("index_reject");
        assert_eq!(v["type"], "index_reject");
        assert_eq!(v["reason"], "stale_updated_at");
        assert_eq!(v["updated_at"], Value::from(10u64));
        let _ = fs::remove_dir_all(&data_dir);
    }

    /// EN: Offline chat mailbox — Alice sends while Bob offline; Bob fetches with strict auth + sig;
    /// journal `chat_store` survives reload. CN: 离线邮箱——Alice 发、Bob 离线；Bob 带 strict auth +
    /// 签名 chat_fetch；journal `chat_store` 经 reload 仍在。
    #[test]
    fn offline_chat_mailbox_store_fetch_and_journal_replay() {
        use crate::config::Config;
        use crate::state::Server;
        use base64::Engine;
        use relay_core::{encode_account_id, register_account_sign_payload};
        use schnorrkel::Keypair;
        use std::fs;
        use std::sync::Arc;

        let data_dir =
            std::env::temp_dir().join(format!("relay-chat-offline-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_dir);
        fs::create_dir_all(&data_dir).unwrap();
        let mut cfg = Config::from_env();
        cfg.data_dir = data_dir.to_string_lossy().into_owned();
        cfg.strict_auth = true;
        let server = Arc::new(Server::load(cfg).unwrap());

        let bob_kp = Keypair::generate();
        let mut bob_pk = [0u8; 32];
        bob_pk.copy_from_slice(&bob_kp.public.to_bytes());
        let bob = normalize_account(&encode_account_id(&bob_pk));
        let alice = normalize_account("5GrwvaEF5zXj16J3HTbUXiCqCa5xj3UmNpYRFhY1M4B");

        let (alice_tx, mut _alice_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut alice_conn = Conn {
            id: Some("alice-ep".into()),
            account: Some(alice.clone()),
            is_loopback: true,
        };
        let conv = format!("d:{alice}:{bob}");
        let dedup = format!("{conv}:m-offline-1");
        let frame = format!(
            r#"{{"convId":"{conv}","senderRef":"{alice}","ciphertextB64":"AQID","dedupKey":"{dedup}","_from":"alice-ep"}}"#
        );
        process(&server, &mut alice_conn, &alice_tx, &frame);

        {
            let inner = server.inner.lock().unwrap();
            let box_ = inner.persist.chat_mailbox.get(&bob).expect("bob mailbox");
            assert!(box_.contains_key(&dedup));
        }

        let (anon_tx, mut anon_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut anon_conn = Conn {
            id: None,
            account: None,
            is_loopback: false,
        };
        process(
            &server,
            &mut anon_conn,
            &anon_tx,
            &format!(r#"{{"type":"chat_fetch","account":"{bob}","request_id":"req-anon"}}"#),
        );
        let reject = recv_text(&mut anon_rx).expect("auth_reject");
        assert_eq!(reject["type"], "auth_reject");
        assert_eq!(reject["op"], "chat_fetch");

        let (bob_tx, mut bob_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut bob_conn = Conn {
            id: None,
            account: None,
            is_loopback: false,
        };
        process(
            &server,
            &mut bob_conn,
            &bob_tx,
            r#"{"type":"register","id":"bob-ep"}"#,
        );
        let payload = register_account_sign_payload("bob-ep", &bob);
        let sig = bob_kp
            .sign_simple(b"substrate", payload.as_slice())
            .to_bytes();
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig);
        process(
            &server,
            &mut bob_conn,
            &bob_tx,
            &format!(
                r#"{{"type":"register_account","id":"bob-ep","account":"{bob}","account_sig":"{sig_b64}"}}"#
            ),
        );
        while bob_rx.try_recv().is_ok() {}

        process(
            &server,
            &mut bob_conn,
            &bob_tx,
            &format!(r#"{{"type":"chat_fetch","account":"{bob}","request_id":"req-bob"}}"#),
        );
        let reply = recv_text(&mut bob_rx).expect("chat_reply");
        assert_eq!(reply["type"], "chat_reply");
        let frames = reply["frames"].as_array().expect("frames array");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["dedupKey"], dedup);

        server.flush(&mut server.inner.lock().unwrap());
        let server2 = Arc::new(
            Server::load({
                let mut c = Config::from_env();
                c.data_dir = data_dir.to_string_lossy().into_owned();
                c.strict_auth = true;
                c
            })
            .unwrap(),
        );
        let inner2 = server2.inner.lock().unwrap();
        assert!(inner2
            .persist
            .chat_mailbox
            .get(&bob)
            .is_some_and(|b| b.contains_key(&dedup)));
        drop(inner2);

        let _ = fs::remove_dir_all(&data_dir);
    }
}
