// EN: Pin planners (ADR §5.8) — byte-faithful ports of relay-pinner.mjs::planPinOps (hot
// tier, generation rotation) and relay-chain-pinner.mjs::planChainPinRequests (only-additive,
// shared by chain + crust tiers). Pure functions; the daemons add IO around them.
// CN: Pin 规划器（ADR §5.8）——relay-pinner.mjs::planPinOps（热层，代次轮转）与
// relay-chain-pinner.mjs::planChainPinRequests（only-additive，chain + crust 共用）的忠实
// 移植。纯函数；守护进程在外层加 IO。

use crate::types::Pointer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_KEEP_GENERATIONS: usize = 2;

/// EN: Hot-tier pinner bookkeeping (`relay-pinner-state.json`). CN: 热层 pinner 记账。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnerState {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub slots: BTreeMap<String, Vec<Pointer>>,
    #[serde(default)]
    pub pinned: Vec<String>,
}

/// EN: Result of `plan_pin_ops`. CN: `plan_pin_ops` 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinPlan {
    pub to_pin: Vec<String>,
    pub to_unpin: Vec<String>,
    pub next_state: PinnerState,
}

/// EN: Rotate per-slot generations, compute pin/unpin deltas. Slots that vanished from the
/// relay keep their generations (a relay wipe must not cascade into unpinning user data).
/// CN: 轮转每槽位代次并算 pin/unpin 增量；从 relay 消失的槽位保留代次（清库不级联 unpin）。
pub fn plan_pin_ops(
    desired: &BTreeMap<String, Pointer>,
    prev: Option<&PinnerState>,
    keep_generations: usize,
) -> PinPlan {
    let keep = keep_generations.max(1);
    let empty = BTreeMap::new();
    let prev_slots = prev.map(|p| &p.slots).unwrap_or(&empty);
    let mut next_slots: BTreeMap<String, Vec<Pointer>> = BTreeMap::new();

    for (key, ptr) in desired {
        let mut gens: Vec<Pointer> = prev_slots.get(key).cloned().unwrap_or_default();
        if gens.is_empty() || gens[0].cid != ptr.cid {
            gens.insert(0, ptr.clone());
        } else {
            gens[0] = ptr.clone();
        }
        gens.truncate(keep);
        next_slots.insert(key.clone(), gens);
    }
    for (key, gens) in prev_slots {
        next_slots
            .entry(key.clone())
            .or_insert_with(|| gens.clone());
    }

    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for gens in next_slots.values() {
        for g in gens {
            wanted.insert(g.cid.clone());
        }
    }
    let had: BTreeSet<String> = prev
        .map(|p| p.pinned.iter().cloned().collect())
        .unwrap_or_default();
    let to_pin = wanted.difference(&had).cloned().collect();
    let to_unpin = had.difference(&wanted).cloned().collect();

    PinPlan {
        to_pin,
        to_unpin,
        next_state: PinnerState {
            v: 1,
            slots: next_slots,
            pinned: wanted.into_iter().collect(),
        },
    }
}

/// EN: Only-additive pinner bookkeeping (chain + crust). `requested` values vary by tier
/// (chain: {at,size}; crust: {at,size,requestId}), so kept as opaque `Value`.
/// CN: only-additive 记账（chain + crust）。`requested` 值随层而异，存为不透明 Value。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlyAddState {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub requested: BTreeMap<String, Value>,
}

/// EN: Result of `plan_chain_pin_requests`. CN: 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainPinPlan {
    pub to_request: Vec<String>,
    pub next_state: OnlyAddState,
}

/// EN: CIDs currently referenced and not yet recorded as requested. Only-additive: a CID
/// never leaves `requested` (chain billing owns the pin lifecycle). CN: only-additive 规划。
pub fn plan_chain_pin_requests(
    desired: &BTreeMap<String, Pointer>,
    prev: Option<&OnlyAddState>,
) -> ChainPinPlan {
    let requested: BTreeMap<String, Value> = prev.map(|p| p.requested.clone()).unwrap_or_default();
    let mut to_request = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for ptr in desired.values() {
        if ptr.cid.is_empty() || seen.contains(&ptr.cid) {
            continue;
        }
        seen.insert(ptr.cid.clone());
        if !requested.contains_key(&ptr.cid) {
            to_request.push(ptr.cid.clone());
        }
    }
    ChainPinPlan {
        to_request,
        next_state: OnlyAddState { v: 1, requested },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ptr(cid: &str, ua: u64) -> Pointer {
        Pointer {
            cid: cid.into(),
            updated_at: ua,
        }
    }

    #[test]
    fn first_pin_then_rotation_keeps_two_generations() {
        let mut desired = BTreeMap::new();
        desired.insert("index/5A".to_string(), ptr("c1", 1));
        let p1 = plan_pin_ops(&desired, None, DEFAULT_KEEP_GENERATIONS);
        assert_eq!(p1.to_pin, vec!["c1".to_string()]);
        assert!(p1.to_unpin.is_empty());

        // pointer rotates to c2; both kept (2 generations), nothing unpinned yet
        desired.insert("index/5A".to_string(), ptr("c2", 2));
        let p2 = plan_pin_ops(&desired, Some(&p1.next_state), DEFAULT_KEEP_GENERATIONS);
        assert_eq!(p2.to_pin, vec!["c2".to_string()]);
        assert!(p2.to_unpin.is_empty());
        assert_eq!(p2.next_state.slots["index/5A"].len(), 2);

        // rotate to c3; c1 falls off the 2-gen window -> unpinned
        desired.insert("index/5A".to_string(), ptr("c3", 3));
        let p3 = plan_pin_ops(&desired, Some(&p2.next_state), DEFAULT_KEEP_GENERATIONS);
        assert_eq!(p3.to_pin, vec!["c3".to_string()]);
        assert_eq!(p3.to_unpin, vec!["c1".to_string()]);
    }

    #[test]
    fn vanished_slot_keeps_generations_no_cascade_unpin() {
        let mut desired = BTreeMap::new();
        desired.insert("index/5A".to_string(), ptr("c1", 1));
        let p1 = plan_pin_ops(&desired, None, 2);
        // relay wiped: desired empty, but previous slot generations must be retained
        let p2 = plan_pin_ops(&BTreeMap::new(), Some(&p1.next_state), 2);
        assert!(p2.to_unpin.is_empty());
        assert!(p2.next_state.pinned.contains(&"c1".to_string()));
    }

    #[test]
    fn chain_only_additive() {
        let mut desired = BTreeMap::new();
        desired.insert("index/5A".to_string(), ptr("c1", 1));
        desired.insert("contacts/5A".to_string(), ptr("c2", 1));
        let p1 = plan_chain_pin_requests(&desired, None);
        let mut got = p1.to_request.clone();
        got.sort(); // planner output is set-semantic (order follows slot-key iteration)
        assert_eq!(got, vec!["c1".to_string(), "c2".to_string()]);

        // The planner is only-additive but does NOT mark requested itself — the daemon
        // records each CID after a successful submit. Simulate that, then c3 is the only
        // newly-requested CID and the recorded c1/c2 are never removed.
        let mut state = p1.next_state.clone();
        state
            .requested
            .insert("c1".into(), serde_json::json!({ "at": 1, "size": 9 }));
        state
            .requested
            .insert("c2".into(), serde_json::json!({ "at": 1, "size": 9 }));
        desired.insert("archive/5A".to_string(), ptr("c3", 1));
        let p2 = plan_chain_pin_requests(&desired, Some(&state));
        assert_eq!(p2.to_request, vec!["c3".to_string()]);
        assert!(p2.next_state.requested.contains_key("c1"));
    }
}
