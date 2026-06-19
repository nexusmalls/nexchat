//! Weight definitions for `pallet-chat-inbox`.
//! `pallet-chat-inbox` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`).
//! Generated on a dev chain (steps=50, repeat=20); re-run on reference hardware
//! before mainnet. CN: 由节点基准框架（见 `benchmarking.rs`）实测得到的权重，
//! 在 dev 链上生成（steps=50, repeat=20）；上主网前应在基准硬件上重跑。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-chat-inbox`.
/// CN: `pallet-chat-inbox` 所需的权重函数。
pub trait WeightInfo {
    fn register_inbox() -> Weight;
    fn bump_epoch() -> Weight;
    fn revoke_tag() -> Weight;
    fn unrevoke_tag() -> Weight;
    fn transfer_controller() -> Weight;
    fn deregister_inbox() -> Weight;
    fn force_deregister_inbox() -> Weight;
}

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Storage: Inboxes (r:1 w:1), InboxCountByController (r:1 w:1).
    fn register_inbox() -> Weight {
        Weight::from_parts(75_577_000, 11763)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    /// Storage: Inboxes (r:1 w:1).
    fn bump_epoch() -> Weight {
        Weight::from_parts(38_524_000, 11763)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: Inboxes (r:1 w:1).
    fn revoke_tag() -> Weight {
        Weight::from_parts(39_500_000, 11763)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: Inboxes (r:1 w:1).
    fn unrevoke_tag() -> Weight {
        Weight::from_parts(44_980_000, 11763)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: Inboxes (r:1 w:1), InboxCountByController (r:2 w:2), System::Account (r:1 w:1).
    fn transfer_controller() -> Weight {
        Weight::from_parts(134_807_000, 11763)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(4))
    }
    /// Storage: Inboxes (r:1 w:1), InboxCountByController (r:1 w:1).
    fn deregister_inbox() -> Weight {
        Weight::from_parts(79_445_000, 11763)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    /// Storage: Inboxes (r:1 w:1), InboxCountByController (r:1 w:1).
    fn force_deregister_inbox() -> Weight {
        Weight::from_parts(78_562_000, 11763)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
}

impl WeightInfo for () {
    fn register_inbox() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn bump_epoch() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn revoke_tag() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn unrevoke_tag() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn transfer_controller() -> Weight { Weight::from_parts(25_000_000, 0) }
    fn deregister_inbox() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn force_deregister_inbox() -> Weight { Weight::from_parts(20_000_000, 0) }
}
