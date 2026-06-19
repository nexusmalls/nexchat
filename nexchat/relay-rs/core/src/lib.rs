// EN: relay-core — shared library for the Rust relay (server + 3 pinners). Houses the
// byte-compatible persistence layer (WAL + snapshot), the dual-track pinner derivation,
// the pin planners, pinner state files, and SS58 normalization. Cryptography (RFC 9474
// verify) and the WS protocol live in the server bin; chain/IPFS IO lives in the pinners.
// CN: relay-core —— Rust relay（server + 三个 pinner）共享库。包含逐字节兼容的持久化层
// （WAL + 快照）、双轨 pinner 推导、pin 规划器、pinner state 文件、SS58 规范化。密码学
// （RFC 9474 验签）与 WS 协议在 server bin；链/IPFS IO 在各 pinner。

pub mod account_auth;
pub mod commit_slot;
pub mod desired;
pub mod journal;
pub mod persistence;
pub mod planner;
pub mod ss58;
pub mod statefile;
pub mod types;

pub use account_auth::{
    register_account_sign_payload, verify_register_account_sig, verify_sr25519_raw,
};
pub use commit_slot::{reseed_slots_from_commits, try_accept_commit, CommitDecision, CommitSlot};
pub use desired::collect_desired_pointers;
pub use journal::{apply_journal_op, JournalOp};
pub use persistence::{
    add_spent_token, prune_orphan_spent, PersistState, Persistence, DEFAULT_SPENT_CAP,
};
pub use planner::{
    plan_chain_pin_requests, plan_pin_ops, ChainPinPlan, OnlyAddState, PinPlan, PinnerState,
    DEFAULT_KEEP_GENERATIONS,
};
pub use ss58::{account_pubkey, encode_account_id, normalize_account};
pub use statefile::{read_state_file, write_state_file};
pub use types::{ContactBox, InboxRecord, Pointer, Snapshot, SCHEMA_V};

use std::time::{SystemTime, UNIX_EPOCH};

/// EN: Current unix time in milliseconds (parity with JS `Date.now()`). CN: 当前毫秒时间戳。
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
