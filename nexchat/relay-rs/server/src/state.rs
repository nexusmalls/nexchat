// EN: Relay server runtime state. Durable KV lives in `relay_core::PersistState` (snapshot +
// WAL); this adds the in-memory routing tables (account -> endpoints, inbox-id index, endpoint
// -> sink) and the debounced-snapshot bookkeeping. All mutation happens under one mutex; no
// `.await` is held across it (fs/crypto are synchronous), matching the single-threaded Node loop.
// CN: relay server 运行时状态。持久 KV 在 relay_core::PersistState（快照 + WAL）；此处加内存
// 路由表（账户→端点、inbox-id 索引、端点→发送端）与防抖快照记账。所有变更在单一 mutex 下完成，
// 不跨 await 持锁（fs/密码学均同步），与 Node 单线程事件循环一致。

use relay_core::{
    reseed_slots_from_commits, CommitSlot, InboxRecord, JournalOp, PersistState, Persistence,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Mutex;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;

/// EN: Per-endpoint outbound channel (drained by the connection's writer task). CN: 端点出站通道。
pub type Tx = mpsc::UnboundedSender<Message>;

/// EN: Hard cap on cached unused OPK leaves per `(account, device)` (drops the overflow on upload).
/// CN: 每 `(account, device)` 缓存未用 OPK 叶子数上限（上传时丢弃溢出部分）。
pub const OPK_MAX_LEAVES_PER_DEVICE: usize = 256;
/// EN: Hard cap on distinct `(account, device)` cache entries (a new key past the cap is refused)
/// — bounds memory since any registered account may upload. CN: 不同 `(account, device)` 缓存条目
/// 上限（超限时拒绝新键）——限制内存，因任何已注册账户均可上传。
pub const OPK_MAX_DEVICES: usize = 8192;

/// EN: In-memory OPK leaf cache for ONE device (design §19/§21). Holds the device's advertised
/// one-time-prekey leaves so the relay can single-dispense them to X3DH initiators while the owner
/// is OFFLINE; never persisted (like `commit_slots`), so the on-disk parity red line is untouched
/// and a restart simply waits for the owner to re-upload. `dispensed` records leaves already handed
/// out THIS lifetime so a re-upload of the owner's still-"unspent" set never re-issues a consumed
/// one-time key (strict single-dispense within a relay lifetime; cross-restart double-issue is the
/// documented best-effort of §19). CN: 单设备的内存态 OPK 叶子缓存（设计 §19/§21）。保存设备公告的
/// 一次性预密钥叶子，使持有者**离线**时 relay 可向 X3DH 发起方单发；从不持久化（仿 `commit_slots`），
/// 故磁盘 parity 红线不动、重启后等持有者重新上传即可。`dispensed` 记录本生命周期已派发的叶子，使
/// 持有者重传其仍"未用"集合时不会再次派发已被消费的一次性钥（单生命周期内严格单发；跨重启重复派发
/// 属 §19 记录的 best-effort）。
pub struct OpkCacheEntry {
    /// EN: On-chain OPK Merkle root the leaves prove against (returned to the initiator for the
    /// relay-trustless proof check). CN: 叶子据以证明的链上 OPK Merkle 根（回给发起方做 relay-trustless 校验）。
    pub root: String,
    /// EN: Unused leaves (`{opk_pub, proof}`), dispensed front-to-back. CN: 未用叶子，先进先出派发。
    pub available: VecDeque<Value>,
    /// EN: `opk_pub` hex strings already dispensed this lifetime. CN: 本生命周期已派发的 `opk_pub`。
    pub dispensed: BTreeSet<String>,
}

/// EN: Mutable shared state behind the server mutex. CN: server mutex 后的可变共享状态。
pub struct Inner {
    pub persist: PersistState,
    /// account (SS58-42) -> set of endpoint ids.
    pub endpoints_by_account: BTreeMap<String, BTreeSet<String>>,
    /// inbox_id -> inbox record (derived from `persist.inboxes_by_account`).
    pub inbox_by_id: BTreeMap<String, InboxRecord>,
    /// inbox_id -> owner account (derived).
    pub account_by_inbox_id: BTreeMap<String, String>,
    /// endpoint id -> outbound sink.
    pub clients: HashMap<String, Tx>,
    // EN: 1:1 Wire-multi-leaf Commit serialization slots (Gate 2 of
    // CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3), keyed by `convId` (`d:..`). Not serialized to disk,
    // but REBUILT on startup from the already-persisted MLS commit backlog (`reseed_commit_slots`), so a
    // restart no longer re-seeds from the first (possibly stale) Commit — the post-restart fork window is
    // closed without adding any on-disk format (parity red line untouched, spec §3.6 / §9).
    // CN: 1:1 Wire 多 leaf 的 Commit 串行化槽位（规范 §3「闸二」），按 `convId`（`d:..`）索引。不序列化到
    // 磁盘，但启动时由**已持久化**的 MLS commit backlog 重建（`reseed_commit_slots`），故重启不再从首条
    // （可能陈旧的）Commit 重新播种——在不新增任何磁盘格式（parity 红线不动，规范 §3.6 / §9）的前提下关闭
    // 重启分叉窗口。
    pub commit_slots: BTreeMap<String, CommitSlot>,
    // EN: In-memory OPK leaf cache keyed by `(owner_account, device_hex)` (design §19/§21). Like
    // `commit_slots` it is NEVER serialized — the owner re-uploads on reconnect, so a restart costs
    // nothing on disk (parity red line untouched). CN: 内存态 OPK 叶子缓存，按 `(持有者账户, 设备 hex)`
    // 索引（设计 §19/§21）。与 `commit_slots` 一样**从不**序列化——持有者重连时重新上传，故重启对磁盘
    // 零成本（parity 红线不动）。
    pub opk_cache: BTreeMap<(String, String), OpkCacheEntry>,
    pub dirty: bool,
    pub journal_lines: u64,
}

impl Inner {
    fn from_persist(persist: PersistState) -> Self {
        let mut inner = Inner {
            persist,
            endpoints_by_account: BTreeMap::new(),
            inbox_by_id: BTreeMap::new(),
            account_by_inbox_id: BTreeMap::new(),
            clients: HashMap::new(),
            commit_slots: BTreeMap::new(),
            opk_cache: BTreeMap::new(),
            dirty: false,
            journal_lines: 0,
        };
        inner.rebuild_inbox_index();
        inner.reseed_commit_slots();
        inner
    }

    /// EN: Rebuild the Gate-2 Commit slots from the persisted MLS commit backlog (spec §3.6 / §9).
    /// Restart now closes the post-restart re-seed window instead of accepting the first (possibly
    /// stale) Commit. Parity-safe: reads only existing persisted rows, writes nothing to disk.
    /// CN: 由持久化的 MLS commit backlog 重建闸二 Commit 槽位（规范 §3.6 / §9）。重启后关闭「重新播种」
    /// 窗口，而非采纳首条（可能陈旧的）Commit。Parity 安全：仅读现有持久行，不写任何磁盘。
    pub fn reseed_commit_slots(&mut self) {
        let mut rows: Vec<(String, u64, String)> = Vec::new();
        for boxed in self.persist.mls_mailbox.values() {
            for row in boxed.values() {
                if row.get("t").and_then(Value::as_str) != Some("commit") {
                    continue;
                }
                let Some(conv) = row.get("convId").and_then(Value::as_str) else {
                    continue;
                };
                if !conv.starts_with("d:") {
                    continue;
                }
                let Some(epoch) = row.get("commit_epoch").and_then(Value::as_u64) else {
                    continue;
                };
                let msg_id = row.get("msgId").and_then(Value::as_str).unwrap_or("");
                rows.push((conv.to_string(), epoch, msg_id.to_string()));
            }
        }
        self.commit_slots =
            reseed_slots_from_commits(rows.iter().map(|(c, e, m)| (c.as_str(), *e, m.as_str())));
    }

    /// EN: Rebuild inbox-id indexes from the persisted inbox map. CN: 由持久 inbox 表重建 inbox-id 索引。
    pub fn rebuild_inbox_index(&mut self) {
        self.inbox_by_id.clear();
        self.account_by_inbox_id.clear();
        for (account, ib) in &self.persist.inboxes_by_account {
            if !ib.inbox_id.is_empty() {
                self.inbox_by_id.insert(ib.inbox_id.clone(), ib.clone());
                self.account_by_inbox_id
                    .insert(ib.inbox_id.clone(), account.clone());
            }
        }
    }

    /// EN: Remove an endpoint id from an account's delivery set (empty sets are dropped).
    /// CN: 从账户投递集合移除 endpoint id（空集合一并删除）。
    pub fn remove_endpoint_from_account(&mut self, endpoint_id: &str, account: &str) {
        if let Some(set) = self.endpoints_by_account.get_mut(account) {
            set.remove(endpoint_id);
            if set.is_empty() {
                self.endpoints_by_account.remove(account);
            }
        }
    }

    /// EN: Send a wire string to one endpoint (drops silently if the sink is gone). CN: 向单端点发送。
    fn send_to_endpoint(&self, id: &str, wire: &str) {
        if let Some(tx) = self.clients.get(id) {
            let _ = tx.send(Message::Text(wire.to_string().into()));
        }
    }

    /// EN: Deliver a JSON value to every endpoint of an account. CN: 投递给某账户的所有端点。
    pub fn deliver_to_account(&self, account: &str, msg: &Value) {
        let Some(set) = self.endpoints_by_account.get(account) else {
            return;
        };
        let wire = msg.to_string();
        for id in set {
            self.send_to_endpoint(id, &wire);
        }
    }

    /// EN: Fan a frame to a set of accounts, skipping the sender's own endpoint. CN: 扇出帧（排除发送端点）。
    pub fn deliver_frame_to_accounts(
        &self,
        msg: &Value,
        accounts: &BTreeSet<String>,
        except_endpoint: Option<&str>,
    ) {
        if accounts.is_empty() {
            return;
        }
        let wire = msg.to_string();
        for account in accounts {
            let Some(set) = self.endpoints_by_account.get(account) else {
                continue;
            };
            for id in set {
                if Some(id.as_str()) == except_endpoint {
                    continue;
                }
                self.send_to_endpoint(id, &wire);
            }
        }
    }

    /// EN: Cache an owner device's advertised OPK leaf set so the relay can dispense them while the
    /// owner is offline (design §19/§21). Keyed by `(owner_account, device)` — the uploader is the
    /// AUTHENTICATED connection account, so a foreign account cannot poison another account's slot
    /// (and even a same-key bogus leaf fails the initiator's on-chain Merkle check). Re-uploads
    /// REPLACE the available set, excluding any leaf already dispensed this lifetime, and the set is
    /// capped. CN: 缓存持有者设备公告的 OPK 叶子集合，使 relay 可在持有者离线时派发（设计 §19/§21）。
    /// 按 `(持有者账户, 设备)` 索引——上传者为**认证连接账户**，故外部账户无法污染他人槽位（即便同键
    /// 投毒叶子也会在发起方链上 Merkle 校验时失败）。重传**替换**可用集合，排除本生命周期已派发的叶子，
    /// 并对集合做上限裁剪。
    pub fn opk_cache_publish(&mut self, account: &str, device: &str, root: &str, leaves: &[Value]) {
        let key = (account.to_string(), device.to_string());
        if !self.opk_cache.contains_key(&key) && self.opk_cache.len() >= OPK_MAX_DEVICES {
            return; // capacity guard — refuse new devices past the cap
        }
        let entry = self.opk_cache.entry(key).or_insert_with(|| OpkCacheEntry {
            root: root.to_string(),
            available: VecDeque::new(),
            dispensed: BTreeSet::new(),
        });
        entry.root = root.to_string();
        entry.available.clear();
        for leaf in leaves {
            if entry.available.len() >= OPK_MAX_LEAVES_PER_DEVICE {
                break;
            }
            let Some(pub_hex) = leaf.get("opk_pub").and_then(Value::as_str) else {
                continue;
            };
            if entry.dispensed.contains(pub_hex) {
                continue;
            }
            entry.available.push_back(leaf.clone());
        }
    }

    /// EN: Pop ONE unused leaf for `(owner_account, device)`, marking it dispensed (strict single-
    /// dispense within this relay lifetime). Returns `(root, leaf)` or `None` when no cache / leaves.
    /// CN: 为 `(持有者账户, 设备)` 取出一条未用叶子并标记已派发（单生命周期内严格单发）。返回
    /// `(root, leaf)`，无缓存/叶子时返回 `None`。
    pub fn opk_cache_dispense(&mut self, account: &str, device: &str) -> Option<(String, Value)> {
        let entry = self
            .opk_cache
            .get_mut(&(account.to_string(), device.to_string()))?;
        let leaf = entry.available.pop_front()?;
        if let Some(pub_hex) = leaf.get("opk_pub").and_then(Value::as_str) {
            entry.dispensed.insert(pub_hex.to_string());
        }
        Some((entry.root.clone(), leaf))
    }
}

/// EN: Immutable server context shared across all connections. CN: 跨连接共享的不可变上下文。
pub struct Server {
    pub inner: Mutex<Inner>,
    pub persistence: Persistence,
    pub cfg: Config,
    pub flush_notify: Notify,
}

impl Server {
    /// EN: Load durable state from disk and assemble the server. CN: 从磁盘加载持久态并组装 server。
    pub fn load(cfg: Config) -> std::io::Result<Self> {
        let persistence = Persistence::new(cfg.data_dir.clone(), cfg.spent_cap);
        let mut persist = PersistState::default();
        let loaded = persistence.load_into(&mut persist)?;
        if !loaded {
            println!("[nexchat-relay] persistence empty, dir={}", cfg.data_dir);
        }
        Ok(Server {
            inner: Mutex::new(Inner::from_persist(persist)),
            persistence,
            cfg,
            flush_notify: Notify::new(),
        })
    }

    /// EN: Apply a durable op (journal fsync first, then memory). On failure, memory is unchanged.
    /// CN: 持久 op（先 journal fsync，再内存）；失败时不改内存。
    pub fn record(&self, inner: &mut Inner, op: &JournalOp) -> bool {
        match self.persistence.record(&mut inner.persist, op) {
            Ok(()) => {
                inner.journal_lines += 1;
                self.mark_dirty(inner);
                true
            }
            Err(e) => {
                eprintln!("[nexchat-relay] journal write failed: {e}");
                false
            }
        }
    }

    /// EN: Persist a spent delivery token (journal-before-memory). Returns false on reject or I/O error.
    /// CN: 持久 spent 投递令牌（先 journal 后内存）；拒绝或 IO 失败返回 false。
    pub fn record_spent(&self, inner: &mut Inner, inbox_id: &str, token: &str) -> bool {
        match self
            .persistence
            .record_spent_token(&mut inner.persist, inbox_id, token)
        {
            Ok(true) => {
                inner.journal_lines += 1;
                self.mark_dirty(inner);
                true
            }
            Ok(false) => false,
            Err(e) => {
                eprintln!("[nexchat-relay] spent journal write failed: {e}");
                false
            }
        }
    }

    /// EN: Mark state dirty and wake the debounced flusher. CN: 标脏并唤醒防抖 flusher。
    pub fn mark_dirty(&self, inner: &mut Inner) {
        inner.dirty = true;
        self.flush_notify.notify_one();
    }

    /// EN: Atomic snapshot write + journal truncate (clears `dirty`). CN: 原子快照写入并清空 journal。
    pub fn flush(&self, inner: &mut Inner) {
        if let Err(e) = self.persistence.flush_now(&inner.persist) {
            eprintln!("[nexchat-relay] persistence save failed: {e}");
            return;
        }
        inner.dirty = false;
        inner.journal_lines = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_core::PersistState;

    #[test]
    fn remove_endpoint_drops_empty_account_entry() {
        let mut inner = Inner::from_persist(PersistState::default());
        inner
            .endpoints_by_account
            .entry("5Alice".into())
            .or_default()
            .insert("dev1".into());
        inner.remove_endpoint_from_account("dev1", "5Alice");
        assert!(!inner.endpoints_by_account.contains_key("5Alice"));
    }

    #[test]
    fn rebind_moves_endpoint_between_accounts() {
        let mut inner = Inner::from_persist(PersistState::default());
        inner
            .endpoints_by_account
            .entry("5Alice".into())
            .or_default()
            .insert("dev1".into());

        inner.remove_endpoint_from_account("dev1", "5Alice");
        inner
            .endpoints_by_account
            .entry("5Bob".into())
            .or_default()
            .insert("dev1".into());

        assert!(!inner.endpoints_by_account.contains_key("5Alice"));
        assert!(inner
            .endpoints_by_account
            .get("5Bob")
            .unwrap()
            .contains("dev1"));
    }

    fn leaf(pub_hex: &str) -> Value {
        serde_json::json!({ "opk_pub": pub_hex, "proof": format!("proof-{pub_hex}") })
    }

    // EN: Cache single-dispense (§19) — leaves pop front-to-back, each exactly once, and a re-upload
    // of the owner's set never re-issues an already-dispensed leaf (strict within a relay lifetime).
    // CN: 缓存单发（§19）——叶子先进先出、各派发一次；持有者重传集合不会再次派发已派发的叶子（单生命
    // 周期内严格）。
    #[test]
    fn opk_cache_single_dispenses_and_excludes_redispensed_on_reupload() {
        let mut inner = Inner::from_persist(PersistState::default());
        let acct = "5Alice";
        let dev = "deadbeef";
        inner.opk_cache_publish(acct, dev, "root1", &[leaf("aa"), leaf("bb")]);

        let (root, l0) = inner.opk_cache_dispense(acct, dev).expect("first leaf");
        assert_eq!(root, "root1");
        assert_eq!(l0["opk_pub"], "aa");
        let (_, l1) = inner.opk_cache_dispense(acct, dev).expect("second leaf");
        assert_eq!(l1["opk_pub"], "bb");
        assert!(inner.opk_cache_dispense(acct, dev).is_none(), "exhausted");

        // Owner re-uploads its still-"unspent" set (incl. the relay-dispensed "aa"); the relay must
        // NOT re-offer "aa" (already consumed by the recipient's Olm one-time key), only "cc".
        inner.opk_cache_publish(acct, dev, "root2", &[leaf("aa"), leaf("cc")]);
        let (root, l2) = inner.opk_cache_dispense(acct, dev).expect("fresh leaf");
        assert_eq!(root, "root2");
        assert_eq!(l2["opk_pub"], "cc");
        assert!(
            inner.opk_cache_dispense(acct, dev).is_none(),
            "aa not re-issued"
        );
    }

    // EN: Per-device leaf cap drops the overflow on upload. CN: 每设备叶子上限在上传时丢弃溢出。
    #[test]
    fn opk_cache_caps_leaves_per_device() {
        let mut inner = Inner::from_persist(PersistState::default());
        let leaves: Vec<Value> = (0..OPK_MAX_LEAVES_PER_DEVICE + 10)
            .map(|i| leaf(&format!("{i:04x}")))
            .collect();
        inner.opk_cache_publish("5Alice", "dev", "root", &leaves);
        let entry = inner
            .opk_cache
            .get(&("5Alice".to_string(), "dev".to_string()))
            .unwrap();
        assert_eq!(entry.available.len(), OPK_MAX_LEAVES_PER_DEVICE);
    }
}
