// EN: Durable relay KV — WAL (append-only journal) + atomic snapshot + .bak fallback.
// Byte-compatible with relay-persistence.mjs: pointer/inbox/spent ops fsync to the journal
// immediately; full state snapshot is written atomically (.tmp -> rename) with a .bak copy,
// then the journal is truncated. Startup loads the snapshot (falling back to .bak) and
// replays journal entries with `at > snapshot.saved_at`.
// CN: 持久化 relay KV——WAL 日志 + 原子快照 + .bak 回退。与 relay-persistence.mjs 逐字节
// 兼容：指针/inbox/spent op 立即 fsync 到 journal；整态快照原子写入（.tmp -> rename）并
// 复制 .bak，然后清空 journal；启动加载快照（损坏回退 .bak）并重放 `at > saved_at` 的条目。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::journal::{apply_journal_op, JournalOp};
use crate::now_ms;
use crate::types::{ContactBox, InboxRecord, Pointer, Snapshot, SCHEMA_V};

pub const DEFAULT_SPENT_CAP: usize = 50_000;
const SNAPSHOT: &str = "relay-state.json";
const SNAPSHOT_BAK: &str = "relay-state.json.bak";
const JOURNAL: &str = "relay-journal.ndjson";

/// EN: In-memory persistable relay state. `spent_by_inbox` uses a set (cap-enforced).
/// CN: 内存中可持久化的 relay 状态；`spent_by_inbox` 为集合（受 cap 限制）。
#[derive(Debug, Default, Clone)]
pub struct PersistState {
    pub index_pointers: BTreeMap<String, Pointer>,
    pub contacts_pointers: BTreeMap<String, Pointer>,
    pub msg_archive_pointers: BTreeMap<String, Pointer>,
    // EN: Track A MLS escrow-vault pointer slot (design §4/§13). CN: 路线 A MLS 托管 vault 指针槽（设计 §4/§13）。
    pub mls_vault_pointers: BTreeMap<String, Pointer>,
    // EN: Track A sending-authority handoff pointer slot (design §5.2). CN: 路线 A 发送权交接指针槽（设计 §5.2）。
    pub handoff_pointers: BTreeMap<String, Pointer>,
    // EN: Track A PIN-wrapped signing-key backup pointer slot (design §5.3 path C). CN: 路线 A PIN 包裹签名钥备份指针槽（设计 §5.3 路径 C）。
    pub mls_signing_pointers: BTreeMap<String, Pointer>,
    pub inboxes_by_account: BTreeMap<String, InboxRecord>,
    pub spent_by_inbox: BTreeMap<String, BTreeSet<String>>,
    pub contact_mailbox: BTreeMap<String, ContactBox>,
    pub group_invite_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
    pub mls_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
    pub chat_mailbox: BTreeMap<String, BTreeMap<String, Value>>,
}

impl PersistState {
    /// EN: Build the on-disk snapshot DTO (set -> array). CN: 构建磁盘快照 DTO（集合转数组）。
    pub fn to_snapshot(&self, saved_at: u64) -> Snapshot {
        Snapshot {
            v: SCHEMA_V,
            saved_at,
            index_pointers: self.index_pointers.clone(),
            contacts_pointers: self.contacts_pointers.clone(),
            msg_archive_pointers: self.msg_archive_pointers.clone(),
            mls_vault_pointers: self.mls_vault_pointers.clone(),
            handoff_pointers: self.handoff_pointers.clone(),
            mls_signing_pointers: self.mls_signing_pointers.clone(),
            inboxes_by_account: self.inboxes_by_account.clone(),
            spent_by_inbox: self
                .spent_by_inbox
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
            contact_mailbox: self.contact_mailbox.clone(),
            group_invite_mailbox: self.group_invite_mailbox.clone(),
            mls_mailbox: self.mls_mailbox.clone(),
            chat_mailbox: self.chat_mailbox.clone(),
        }
    }

    /// EN: Load from snapshot DTO (array -> set). CN: 由快照 DTO 载入（数组转集合）。
    pub fn from_snapshot(s: Snapshot) -> Self {
        PersistState {
            index_pointers: s.index_pointers,
            contacts_pointers: s.contacts_pointers,
            msg_archive_pointers: s.msg_archive_pointers,
            mls_vault_pointers: s.mls_vault_pointers,
            handoff_pointers: s.handoff_pointers,
            mls_signing_pointers: s.mls_signing_pointers,
            inboxes_by_account: s.inboxes_by_account,
            spent_by_inbox: s
                .spent_by_inbox
                .into_iter()
                .map(|(k, v)| (k, v.into_iter().collect()))
                .collect(),
            contact_mailbox: s.contact_mailbox,
            group_invite_mailbox: s.group_invite_mailbox,
            mls_mailbox: s.mls_mailbox,
            chat_mailbox: s.chat_mailbox,
        }
    }
}

/// EN: Add a spent token under the per-inbox cap (`addSpentToken`). Returns false if empty,
/// already present, or at cap. CN: 在每信箱 cap 下加 spent 令牌；空/已存在/超 cap 返回 false。
pub fn add_spent_token(state: &mut PersistState, inbox_id: &str, token: &str, cap: usize) -> bool {
    if inbox_id.is_empty() || token.is_empty() {
        return false;
    }
    let set = state
        .spent_by_inbox
        .entry(inbox_id.to_string())
        .or_default();
    if set.contains(token) {
        return false;
    }
    if set.len() >= cap {
        return false;
    }
    set.insert(token.to_string());
    true
}

/// EN: Drop spent sets whose inbox is no longer registered (`pruneOrphanSpent`).
/// CN: 删除 inbox 不再注册的 spent 集合。
pub fn prune_orphan_spent(state: &mut PersistState) {
    let active: BTreeSet<String> = state
        .inboxes_by_account
        .values()
        .map(|ib| ib.inbox_id.clone())
        .collect();
    state.spent_by_inbox.retain(|k, _| active.contains(k));
}

fn fsync_file(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

fn fsync_dir(dir: &Path) {
    // EN: best-effort directory fsync (Linux). CN: 尽力同步目录元数据。
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

/// EN: File-backed WAL + snapshot engine. CN: 文件 WAL + 快照引擎。
pub struct Persistence {
    pub dir: PathBuf,
    pub snapshot_file: PathBuf,
    pub bak_file: PathBuf,
    pub journal_file: PathBuf,
    pub spent_cap: usize,
}

impl Persistence {
    pub fn new<P: Into<PathBuf>>(dir: P, spent_cap: usize) -> Self {
        let dir = dir.into();
        Persistence {
            snapshot_file: dir.join(SNAPSHOT),
            bak_file: dir.join(SNAPSHOT_BAK),
            journal_file: dir.join(JOURNAL),
            spent_cap,
            dir,
        }
    }

    /// EN: Load snapshot (fallback .bak) then replay journal `at > saved_at`; prune orphans.
    /// Returns whether anything was loaded. CN: 加载快照（回退 .bak）后重放 journal 并清孤儿。
    pub fn load_into(&self, state: &mut PersistState) -> io::Result<bool> {
        fs::create_dir_all(&self.dir)?;
        let mut snapshot_at = 0u64;
        let mut loaded = false;

        for candidate in [&self.snapshot_file, &self.bak_file] {
            if !candidate.exists() {
                continue;
            }
            match fs::read_to_string(candidate)
                .ok()
                .and_then(|t| serde_json::from_str::<Snapshot>(&t).ok())
            {
                Some(snap) if snap.v == SCHEMA_V => {
                    snapshot_at = snap.saved_at;
                    *state = PersistState::from_snapshot(snap);
                    loaded = true;
                    break;
                }
                _ => { /* corrupt or wrong schema -> try next candidate */ }
            }
        }

        let replayed = self.replay_journal(state, snapshot_at)?;
        prune_orphan_spent(state);
        Ok(loaded || replayed > 0)
    }

    fn replay_journal(&self, state: &mut PersistState, after_ms: u64) -> io::Result<usize> {
        if !self.journal_file.exists() {
            return Ok(0);
        }
        let text = fs::read_to_string(&self.journal_file)?;
        let mut replayed = 0usize;
        for line in text.split('\n') {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue; // corrupt line skipped (parity with JS)
            };
            let at = value.get("at").and_then(Value::as_u64).unwrap_or(0);
            if at <= after_ms {
                continue;
            }
            if let Ok(op) = serde_json::from_value::<JournalOp>(value) {
                apply_journal_op(state, &op, self.spent_cap);
                replayed += 1;
            }
        }
        Ok(replayed)
    }

    /// EN: Append+fsync the WAL first, then apply to in-memory state. If the journal write fails,
    /// state is left unchanged (no memory/disk split). CN: 先追加并 fsync WAL，再应用到内存；journal
    /// 写失败时不改内存（避免内存/磁盘不一致）。
    pub fn record(&self, state: &mut PersistState, op: &JournalOp) -> io::Result<()> {
        self.append_journal(op)?;
        apply_journal_op(state, op, self.spent_cap);
        Ok(())
    }

    /// EN: Persist a spent delivery token: journal fsync before marking in memory (RFC 9474 replay
    /// safety). Returns `Ok(false)` when already spent or at cap; `Err` on journal I/O failure.
    /// CN: 持久化 spent 投递令牌：先 journal fsync 再写内存（RFC 9474 防重放）。已 spent 或达 cap 返回
    /// `Ok(false)`；journal IO 失败返回 `Err`。
    pub fn record_spent_token(
        &self,
        state: &mut PersistState,
        inbox_id: &str,
        token: &str,
    ) -> io::Result<bool> {
        if inbox_id.is_empty() || token.is_empty() {
            return Ok(false);
        }
        if state
            .spent_by_inbox
            .get(inbox_id)
            .is_some_and(|set| set.contains(token))
        {
            return Ok(false);
        }
        if state
            .spent_by_inbox
            .get(inbox_id)
            .is_some_and(|set| set.len() >= self.spent_cap)
        {
            return Ok(false);
        }
        let op = JournalOp::SpentAdd {
            inbox_id: inbox_id.to_string(),
            t: token.to_string(),
        };
        self.append_journal(&op)?;
        debug_assert!(add_spent_token(state, inbox_id, token, self.spent_cap));
        Ok(true)
    }

    /// EN: Append `{op, ...payload, at}` and fsync the journal. CN: 追加并 fsync。
    pub fn append_journal(&self, op: &JournalOp) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut value = serde_json::to_value(op).map_err(io::Error::other)?;
        if let Value::Object(ref mut map) = value {
            map.insert("at".to_string(), Value::from(now_ms()));
        }
        let line = format!("{value}\n");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.journal_file)?;
        f.write_all(line.as_bytes())?;
        f.sync_all()?;
        Ok(())
    }

    /// EN: Atomic snapshot write (.tmp -> rename), .bak copy, then truncate journal.
    /// CN: 原子快照写入（.tmp -> rename）+ .bak 复制 + 清空 journal。
    pub fn flush_now(&self, state: &PersistState) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let body = format!(
            "{}\n",
            serde_json::to_string(&state.to_snapshot(now_ms())).map_err(io::Error::other)?
        );
        let tmp = PathBuf::from(format!("{}.tmp", self.snapshot_file.display()));
        fs::write(&tmp, &body)?;
        fsync_file(&tmp)?;
        fs::rename(&tmp, &self.snapshot_file)?;
        fsync_file(&self.snapshot_file)?;
        let _ = fs::copy(&self.snapshot_file, &self.bak_file);
        fsync_dir(&self.dir);
        if self.journal_file.exists() {
            fs::write(&self.journal_file, b"")?;
            fsync_file(&self.journal_file)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalOp;
    use crate::types::InboxRecord;

    fn pers(dir: &Path) -> Persistence {
        Persistence::new(dir, DEFAULT_SPENT_CAP)
    }

    #[test]
    fn record_appends_journal_and_survives_restart_before_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        p1.record(
            &mut s1,
            &JournalOp::ContactsPut {
                account: "5GrwvaEF5zXb26Fz9rcQpDWS57CtEGXpjpH4WTTkPrCEBy".into(),
                cid: "bafybeigdyrzt".into(),
                updated_at: 1000,
            },
        )
        .unwrap();
        assert!(dir.path().join(JOURNAL).exists());

        let mut s2 = PersistState::default();
        let loaded = pers(dir.path()).load_into(&mut s2).unwrap();
        assert!(loaded);
        let ptr = s2
            .contacts_pointers
            .get("5GrwvaEF5zXb26Fz9rcQpDWS57CtEGXpjpH4WTTkPrCEBy")
            .unwrap();
        assert_eq!(ptr.cid, "bafybeigdyrzt");
        assert_eq!(ptr.updated_at, 1000);
    }

    #[test]
    fn flush_writes_snapshot_and_clears_journal() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        p1.record(
            &mut s1,
            &JournalOp::IndexPut {
                account: "5FHneW46xGXgs5mUiveU4sbTyGBzmstSpRY".into(),
                cid: "bafyindex".into(),
                updated_at: 2000,
            },
        )
        .unwrap();
        p1.flush_now(&s1).unwrap();

        assert!(dir.path().join(SNAPSHOT).exists());
        assert_eq!(fs::read_to_string(dir.path().join(JOURNAL)).unwrap(), "");

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert_eq!(
            s2.index_pointers
                .get("5FHneW46xGXgs5mUiveU4sbTyGBzmstSpRY")
                .unwrap()
                .cid,
            "bafyindex"
        );
    }

    #[test]
    fn bak_restores_when_primary_snapshot_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        s1.msg_archive_pointers.insert(
            "5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PT".into(),
            Pointer {
                cid: "bafyarchive".into(),
                updated_at: 3000,
            },
        );
        p1.flush_now(&s1).unwrap();
        fs::write(dir.path().join(SNAPSHOT), "{not json").unwrap();

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert_eq!(
            s2.msg_archive_pointers
                .get("5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PT")
                .unwrap()
                .cid,
            "bafyarchive"
        );
    }

    #[test]
    fn journal_replay_respects_monotonic_updated_at() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        let acct = "5GrwvaEF5zXb26Fz9rcQpDWS57CtEGXpjpH4WTTkPrCEBy".to_string();
        p1.record(
            &mut s1,
            &JournalOp::ContactsPut {
                account: acct.clone(),
                cid: "newer".into(),
                updated_at: 5000,
            },
        )
        .unwrap();
        p1.record(
            &mut s1,
            &JournalOp::ContactsPut {
                account: acct.clone(),
                cid: "stale".into(),
                updated_at: 1000,
            },
        )
        .unwrap();

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert_eq!(s2.contacts_pointers.get(&acct).unwrap().cid, "newer");
    }

    #[test]
    fn spent_add_and_clear_survive_replay() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        let acct = "5GrwvaEF5zXb26Fz9rcQpDWS57CtEGXpjpH4WTTkPrCEBy".to_string();
        p1.record(
            &mut s1,
            &JournalOp::InboxRegister {
                account: acct,
                inbox_id: "0xinbox1".into(),
                epoch: 0,
                ipk_n: "n".into(),
                ipk_e: "AQAB".into(),
                revoked_tags: vec![],
            },
        )
        .unwrap();
        p1.record(
            &mut s1,
            &JournalOp::SpentAdd {
                inbox_id: "0xinbox1".into(),
                t: "tok-a".into(),
            },
        )
        .unwrap();
        p1.record(
            &mut s1,
            &JournalOp::SpentAdd {
                inbox_id: "0xinbox1".into(),
                t: "tok-b".into(),
            },
        )
        .unwrap();

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        let spent = s2.spent_by_inbox.get("0xinbox1").unwrap();
        assert!(spent.contains("tok-a"));
        assert!(spent.contains("tok-b"));
    }

    #[test]
    fn orphan_spent_pruned_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = pers(dir.path());
        let mut s1 = PersistState::default();
        // spent for an inbox that is never registered -> dropped on load
        s1.spent_by_inbox
            .insert("0xorphan".into(), ["t1".to_string()].into_iter().collect());
        p1.flush_now(&s1).unwrap();

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert!(!s2.spent_by_inbox.contains_key("0xorphan"));
    }

    #[test]
    fn spent_cap_enforced() {
        let mut s = PersistState::default();
        assert!(add_spent_token(&mut s, "ib", "a", 2));
        assert!(add_spent_token(&mut s, "ib", "b", 2));
        assert!(!add_spent_token(&mut s, "ib", "c", 2)); // at cap
        assert!(!add_spent_token(&mut s, "ib", "a", 2)); // duplicate
        assert_eq!(s.spent_by_inbox.get("ib").unwrap().len(), 2);
    }

    #[test]
    fn rust_snapshot_is_loadable_round_trip() {
        // Rust writes -> Rust reads; also asserts compact JSON has no spaces.
        let dir = tempfile::tempdir().unwrap();
        let mut s1 = PersistState::default();
        s1.index_pointers.insert(
            "5FHneW46xGXgs5mUiveU4sbTyGBzmstSpRY".into(),
            Pointer {
                cid: "bafyx".into(),
                updated_at: 9,
            },
        );
        pers(dir.path()).flush_now(&s1).unwrap();
        let body = fs::read_to_string(dir.path().join(SNAPSHOT)).unwrap();
        assert!(body.starts_with("{\"v\":1,\"saved_at\":"));
        assert!(!body.contains("\n  "));

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert_eq!(s1.index_pointers, s2.index_pointers);
    }

    #[test]
    fn record_leaves_state_unchanged_when_journal_unwritable() {
        let dir = tempfile::tempdir().unwrap();
        // EN: journal path as directory → append open fails; memory must not change. CN: journal 路径
        // 为目录 → 追加打开失败；内存不得变更。
        fs::create_dir_all(dir.path().join(JOURNAL)).unwrap();
        let p = pers(dir.path());
        let mut s = PersistState::default();
        let op = JournalOp::IndexPut {
            account: "5Alice".into(),
            cid: "bafy".into(),
            updated_at: 1,
        };
        assert!(p.record(&mut s, &op).is_err());
        assert!(s.index_pointers.is_empty());
    }

    #[test]
    fn chat_store_journal_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let p = pers(dir.path());
        let mut s1 = PersistState::default();
        let row = serde_json::json!({
            "dedupKey": "d:a:b:m1",
            "convId": "d:a:b",
            "ciphertextB64": "AQID",
            "stored_at": 100,
            "bytes": 42
        });
        p.record(
            &mut s1,
            &JournalOp::ChatStore {
                account: "5Bob".into(),
                dedup_key: "d:a:b:m1".into(),
                row: row.clone(),
            },
        )
        .unwrap();
        p.flush_now(&s1).unwrap();

        let mut s2 = PersistState::default();
        p.load_into(&mut s2).unwrap();
        assert_eq!(s2.chat_mailbox["5Bob"]["d:a:b:m1"]["ciphertextB64"], "AQID");
    }

    #[test]
    fn record_spent_journal_before_memory_and_survives_replay() {
        let dir = tempfile::tempdir().unwrap();
        let p = pers(dir.path());
        let mut s1 = PersistState::default();
        s1.inboxes_by_account.insert(
            "5Acc".into(),
            InboxRecord {
                inbox_id: "0xinbox".into(),
                epoch: 1,
                ipk_n: String::new(),
                ipk_e: String::new(),
                revoked_tags: vec![],
            },
        );
        p.flush_now(&s1).unwrap();

        assert!(p.record_spent_token(&mut s1, "0xinbox", "tok1").unwrap());
        assert!(s1.spent_by_inbox.get("0xinbox").unwrap().contains("tok1"));
        assert!(!p.record_spent_token(&mut s1, "0xinbox", "tok1").unwrap());

        let mut s2 = PersistState::default();
        pers(dir.path()).load_into(&mut s2).unwrap();
        assert!(s2.spent_by_inbox.get("0xinbox").unwrap().contains("tok1"));
    }

    #[test]
    fn record_spent_leaves_memory_unchanged_when_journal_unwritable() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(JOURNAL)).unwrap();
        let p = pers(dir.path());
        let mut s = PersistState::default();
        assert!(p.record_spent_token(&mut s, "0xinbox", "tok1").is_err());
        assert!(s.spent_by_inbox.is_empty());
    }
}
