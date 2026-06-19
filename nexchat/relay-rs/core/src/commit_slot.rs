// EN: 1:1 Wire-multi-leaf Commit serialization — "Gate 2" of
// CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md §3. 1:1 DMs are off-chain pairwise MLS with NO
// on-chain epoch total order, so any leaf (any device of either account) may emit a Commit and
// concurrent Commits onto the same epoch would fork the group. The relay arbitrates per
// `(conv, epoch)` with CAS semantics: at a given epoch it ACCEPTS only the FIRST Commit and
// returns EpochStale to the rest, who re-fetch and retry. This module is the pure decision core;
// it never parses MLS ciphertext — the `commit_epoch` is carried as a plaintext integer in the
// frame header (an acceptable disclosure: a monotone counter, no content).
//
// CN: 1:1 Wire 多 leaf 的 Commit 串行化——CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md §3 的「闸二」。
// 1:1 是链下 pairwise MLS，**无链上 epoch 全序**，故任意 leaf（任一账户的任一设备）都能发 Commit，
// 同一 epoch 的并发 Commit 会使群分叉。relay 按 `(conv, epoch)` 用 CAS 仲裁：同一 epoch 只接受**第一条**
// Commit，其余返回 EpochStale，落败方重取状态后重试。本模块是纯决策内核，**不解析 MLS 密文**——
// `commit_epoch` 以明文整数随帧头携带（可接受泄漏：单调计数、无内容）。

use std::collections::BTreeMap;

/// EN: Per-conversation Commit slot: the last accepted epoch and the `msg_id` that advanced it
/// (the latter makes idempotent re-delivery cheap). Ephemeral routing state — NOT persisted.
/// CN: 每会话 Commit 槽位：最近被采纳到的 epoch，以及推进它的 `msg_id`（后者让幂等重投廉价）。
/// 临时路由态——**不持久化**。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitSlot {
    /// EN: Epoch the conversation has been accepted up to (= epoch AFTER the last accepted Commit).
    /// CN: 会话已被采纳到的 epoch（= 最近被采纳 Commit 之后的 epoch）。
    pub epoch: u64,
    /// EN: `msg_id` of the last accepted Commit (idempotency key). CN: 最近被采纳 Commit 的 `msg_id`（幂等键）。
    pub last_msg_id: String,
}

/// EN: Outcome of submitting a Commit to the relay's serialization gate. CN: 向 relay 串行化闸提交 Commit 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitDecision {
    /// EN: First/winning Commit at this epoch — fan it out; relay advanced to `next_epoch`.
    /// CN: 该 epoch 的第一条/胜出 Commit——扇出之；relay 已推进到 `next_epoch`。
    Accepted { next_epoch: u64 },
    /// EN: Same `msg_id` as the last accepted Commit — already applied; safe to re-fan, no advance.
    /// CN: 与最近被采纳 Commit 同 `msg_id`——已应用；可安全重扇，不再推进。
    Idempotent,
    /// EN: Lost the race (its `commit_epoch` < the relay's current epoch) — sender must catch up to
    /// `current_epoch` and retry. CN: 竞争落败（其 `commit_epoch` < relay 当前 epoch）——发送方需追平到
    /// `current_epoch` 后重试。
    EpochStale { current_epoch: u64 },
}

/// EN: CAS arbitration for one Commit (spec §3.2). `commit_epoch` is the Commit's PRE-epoch
/// (== `expected_epoch`). Rules:
///   - slot absent (first-seen / post-restart re-seed) → ACCEPT, seed `epoch = commit_epoch + 1`;
///   - `msg_id` equals the slot's last accepted id → IDEMPOTENT (no advance);
///   - `commit_epoch < slot.epoch` → EPOCH_STALE{current = slot.epoch} (a newer Commit already won);
///   - otherwise (`commit_epoch >= slot.epoch`) → ACCEPT, advance `epoch = commit_epoch + 1`.
/// The `>=` (rather than strict `==`) branch is forgiving for the post-restart case where the slot
/// was lost: it re-seeds forward instead of wedging a legitimately-ahead client. Within one relay
/// lifetime the winner always sees `commit_epoch == slot.epoch`, and the SECOND of two concurrent
/// same-epoch Commits sees `slot.epoch` already incremented → EPOCH_STALE (the fork is prevented).
///
/// CN: 单条 Commit 的 CAS 仲裁（规范 §3.2）。`commit_epoch` 为该 Commit 的前置 epoch（== `expected_epoch`）。
/// 规则：①槽位缺失（首见 / 重启后重新播种）→ 采纳，播种 `epoch = commit_epoch + 1`；②`msg_id` 等于槽位
/// 最近被采纳 id → 幂等（不推进）；③`commit_epoch < slot.epoch` → EpochStale{current = slot.epoch}（已有更新
/// Commit 胜出）；④否则（`commit_epoch >= slot.epoch`）→ 采纳并推进 `epoch = commit_epoch + 1`。用 `>=`
/// 而非严格 `==` 是为兼容重启后槽位丢失：向前重播种而非卡死一个合法领先的客户端。同一 relay 生命周期内
/// 胜者总满足 `commit_epoch == slot.epoch`，两条并发同 epoch Commit 中的第二条会看到 `slot.epoch` 已自增
/// → EpochStale（分叉被阻止）。
pub fn try_accept_commit(
    slots: &mut BTreeMap<String, CommitSlot>,
    conv: &str,
    commit_epoch: u64,
    msg_id: &str,
) -> CommitDecision {
    match slots.get_mut(conv) {
        None => {
            let next_epoch = commit_epoch.saturating_add(1);
            slots.insert(
                conv.to_string(),
                CommitSlot {
                    epoch: next_epoch,
                    last_msg_id: msg_id.to_string(),
                },
            );
            CommitDecision::Accepted { next_epoch }
        }
        Some(slot) => {
            if !msg_id.is_empty() && slot.last_msg_id == msg_id {
                return CommitDecision::Idempotent;
            }
            if commit_epoch < slot.epoch {
                return CommitDecision::EpochStale {
                    current_epoch: slot.epoch,
                };
            }
            let next_epoch = commit_epoch.saturating_add(1);
            slot.epoch = next_epoch;
            slot.last_msg_id = msg_id.to_string();
            CommitDecision::Accepted { next_epoch }
        }
    }
}

/// EN: Rebuild the Gate-2 slots on startup from already-persisted winning Commit rows, so a relay
/// restart does NOT reopen the post-restart re-seed window (spec §2.5 degradation / §9 "slot
/// persistence"). Without this, after a restart the empty slot would re-seed from the FIRST Commit it
/// sees — which could be a STALE one (a loser that never caught up), briefly double-accepting an already
/// consumed epoch until clients heal. PARITY-SAFE: derives PURELY from the existing persisted MLS commit
/// backlog (only winners are ever stored — losers are neither stored nor fanned), adding NO new on-disk
/// format, so the disk-parity red line (`core/tests/js_compat.rs`) is untouched. For each conversation it
/// takes the MAX observed `commit_epoch` and seeds `epoch = max + 1` with that Commit's `msg_id` (so the
/// exact winning Commit re-delivered after restart is still recognized as Idempotent, not EpochStale).
/// `rows` yields `(conv, commit_epoch, msg_id)`; duplicates (the same winner stored under each
/// recipient's mailbox) are harmless under the max reduction.
///
/// CN: 启动时由**已持久化**的胜出 Commit 行重建闸二槽位，使 relay 重启**不再**重开「重启后重新播种」
/// 窗口（规范 §2.5 降级 / §9「槽位持久化」）。否则重启后空槽位会从**首条**所见 Commit 重新播种——而那可能
/// 是一条**陈旧** Commit（从未追平的落败方），在客户端自愈前短暂二次采纳一个已消费的 epoch。**保持 parity
/// 安全**：纯粹由现有持久化的 MLS commit backlog 推导（只有胜者会被存储——落败者既不存储也不扇出），**不新增
/// 任何磁盘格式**，故磁盘 parity 红线（`core/tests/js_compat.rs`）不受影响。每会话取观测到的**最大** `commit_epoch`
/// 并以 `epoch = max + 1` 播种、记录该 Commit 的 `msg_id`（使重启后重投的那条胜出 Commit 仍被判为幂等而非
/// EpochStale）。`rows` 产出 `(conv, commit_epoch, msg_id)`；重复项（同一胜者存于各接收方邮箱）在取最大下无害。
pub fn reseed_slots_from_commits<'a>(
    rows: impl IntoIterator<Item = (&'a str, u64, &'a str)>,
) -> BTreeMap<String, CommitSlot> {
    let mut slots: BTreeMap<String, CommitSlot> = BTreeMap::new();
    for (conv, commit_epoch, msg_id) in rows {
        let next_epoch = commit_epoch.saturating_add(1);
        slots
            .entry(conv.to_string())
            .and_modify(|s| {
                if next_epoch > s.epoch {
                    s.epoch = next_epoch;
                    s.last_msg_id = msg_id.to_string();
                }
            })
            .or_insert_with(|| CommitSlot {
                epoch: next_epoch,
                last_msg_id: msg_id.to_string(),
            });
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> BTreeMap<String, CommitSlot> {
        BTreeMap::new()
    }

    #[test]
    fn first_commit_is_accepted_and_seeds_epoch() {
        let mut s = slots();
        let d = try_accept_commit(&mut s, "d:a:b", 0, "m0");
        assert_eq!(d, CommitDecision::Accepted { next_epoch: 1 });
        assert_eq!(s["d:a:b"].epoch, 1);
        assert_eq!(s["d:a:b"].last_msg_id, "m0");
    }

    #[test]
    fn second_concurrent_commit_at_same_epoch_loses() {
        let mut s = slots();
        // device A wins epoch 0 -> slot.epoch = 1
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 0, "mA"),
            CommitDecision::Accepted { next_epoch: 1 }
        );
        // device B also built its Commit on epoch 0 -> stale, told to catch up to 1
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 0, "mB"),
            CommitDecision::EpochStale { current_epoch: 1 }
        );
        // slot did not regress
        assert_eq!(s["d:a:b"].epoch, 1);
        assert_eq!(s["d:a:b"].last_msg_id, "mA");
    }

    #[test]
    fn retry_at_new_epoch_after_catch_up_is_accepted() {
        let mut s = slots();
        try_accept_commit(&mut s, "d:a:b", 0, "mA"); // epoch -> 1
                                                     // loser caught up to epoch 1 and retried
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 1, "mB"),
            CommitDecision::Accepted { next_epoch: 2 }
        );
        assert_eq!(s["d:a:b"].epoch, 2);
    }

    #[test]
    fn idempotent_replay_does_not_advance() {
        let mut s = slots();
        try_accept_commit(&mut s, "d:a:b", 0, "mA"); // epoch -> 1
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 0, "mA"),
            CommitDecision::Idempotent
        );
        assert_eq!(s["d:a:b"].epoch, 1, "idempotent replay must not bump epoch");
    }

    #[test]
    fn empty_msg_id_never_matches_as_idempotent() {
        let mut s = slots();
        try_accept_commit(&mut s, "d:a:b", 0, ""); // epoch -> 1, last_msg_id = ""
                                                   // a second empty-id commit at the same epoch is a real (losing) commit, not idempotent
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 0, ""),
            CommitDecision::EpochStale { current_epoch: 1 }
        );
    }

    #[test]
    fn distinct_conversations_have_independent_slots() {
        let mut s = slots();
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 0, "m1"),
            CommitDecision::Accepted { next_epoch: 1 }
        );
        // a different conversation is unaffected by the first one's epoch
        assert_eq!(
            try_accept_commit(&mut s, "d:a:c", 0, "m2"),
            CommitDecision::Accepted { next_epoch: 1 }
        );
    }

    #[test]
    fn post_restart_reseed_accepts_a_higher_epoch() {
        // simulate a relay restart: slot lost; the client is genuinely at epoch 5.
        let mut s = slots();
        let d = try_accept_commit(&mut s, "d:a:b", 5, "m5");
        assert_eq!(d, CommitDecision::Accepted { next_epoch: 6 });
        assert_eq!(s["d:a:b"].epoch, 6);
    }

    #[test]
    fn reseed_takes_max_epoch_per_conversation() {
        // stored winning Commits (incl. duplicates under each recipient's mailbox), out of order.
        let rows = vec![
            ("d:a:b", 0u64, "m0"),
            ("d:a:b", 2, "m2"),
            ("d:a:b", 1, "m1"),
            ("d:a:b", 2, "m2"), // duplicate of the max — harmless
            ("d:a:c", 7, "m7"),
        ];
        let s = reseed_slots_from_commits(rows);
        assert_eq!(
            s["d:a:b"],
            CommitSlot {
                epoch: 3,
                last_msg_id: "m2".into()
            }
        );
        assert_eq!(
            s["d:a:c"],
            CommitSlot {
                epoch: 8,
                last_msg_id: "m7".into()
            }
        );
    }

    #[test]
    fn reseed_closes_the_post_restart_stale_window() {
        // device A had won epoch 4 before the restart (its Commit is in the persisted mailbox).
        let mut s = reseed_slots_from_commits(vec![("d:a:b", 4u64, "mA")]);
        assert_eq!(s["d:a:b"].epoch, 5);
        // a stale Commit at the already-consumed epoch 4 is now REJECTED (pre-reseed it would have been
        // accepted by an empty slot, double-consuming epoch 4 → a transient fork).
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 4, "mStale"),
            CommitDecision::EpochStale { current_epoch: 5 }
        );
        // a legitimately-ahead client at epoch 5 still advances normally.
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 5, "mNext"),
            CommitDecision::Accepted { next_epoch: 6 }
        );
    }

    #[test]
    fn reseed_recognizes_winner_redelivery_as_idempotent() {
        // after restart, re-delivering the EXACT winning Commit must be Idempotent, not EpochStale.
        let mut s = reseed_slots_from_commits(vec![("d:a:b", 4u64, "mA")]);
        assert_eq!(
            try_accept_commit(&mut s, "d:a:b", 4, "mA"),
            CommitDecision::Idempotent
        );
        assert_eq!(
            s["d:a:b"].epoch, 5,
            "idempotent re-delivery must not bump epoch"
        );
    }

    #[test]
    fn reseed_empty_yields_no_slots() {
        let s = reseed_slots_from_commits(Vec::<(&str, u64, &str)>::new());
        assert!(s.is_empty());
    }
}
