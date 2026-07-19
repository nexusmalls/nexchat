//! Weight definitions for `pallet-chat-sync`.
//! `pallet-chat-sync` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`).
//! Generated on a dev chain (steps=50, repeat=20, worst-case `MaxAnchorLen`
//! ciphertext, one ed25519 verification included); re-run on reference hardware
//! before mainnet. CN: 由节点基准框架（见 `benchmarking.rs`）实测得到的权重，
//! 在 dev 链上生成（steps=50, repeat=20，最坏情况 `MaxAnchorLen` 密文，已含一次
//! ed25519 校验）；上主网前应在基准硬件上重跑。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-chat-sync`.
/// CN: `pallet-chat-sync` 所需的权重函数。
pub trait WeightInfo {
    fn publish_sync_anchor() -> Weight;
    fn clear_sync_anchor() -> Weight;
    fn force_clear_sync_anchor() -> Weight;
}

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    /// Storage: System::BlockHash (r:1 w:0), Timestamp::Now (r:1 w:0),
    /// ChatSync::SyncAnchors (r:1 w:1), ChatSync::ClearedAt (r:1 w:0);
    /// + ed25519_verify.
    fn publish_sync_anchor() -> Weight {
        Weight::from_parts(189_844_000, 4088)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: ChatSync::SyncAnchors (r:1 w:1), System::BlockHash (r:1 w:0),
    /// ChatSync::ClearedAt (r:0 w:1); + ed25519_verify.
    fn clear_sync_anchor() -> Weight {
        Weight::from_parts(173_271_000, 4088)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    /// Storage: ChatSync::SyncAnchors (r:1 w:1), ChatSync::ClearedAt (r:0 w:1);
    /// no signature verification (privileged origin).
    fn force_clear_sync_anchor() -> Weight {
        Weight::from_parts(75_230_000, 4088)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(2))
    }
}

impl WeightInfo for () {
    fn publish_sync_anchor() -> Weight {
        Weight::from_parts(193_886_000, 0)
    }
    fn clear_sync_anchor() -> Weight {
        Weight::from_parts(179_879_000, 0)
    }
    fn force_clear_sync_anchor() -> Weight {
        Weight::from_parts(79_396_000, 0)
    }
}
