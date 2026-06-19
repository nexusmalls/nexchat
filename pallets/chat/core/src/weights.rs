//! Weight definitions for `pallet-chat-core`.
//! `pallet-chat-core` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`),
//! generated on a dev chain (steps=50, repeat=20). Re-run on reference hardware
//! before mainnet.
//! CN: 由节点基准框架（见 `benchmarking.rs`）在 dev 链实测（steps=50, repeat=20）。
//! 上主网前应在基准硬件重跑。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-chat-core`.
/// CN: `pallet-chat-core` 所需的权重函数。
pub trait WeightInfo {
    fn send_message() -> Weight;
    fn mark_as_read() -> Weight;
    fn delete_message() -> Weight;
    fn recall_message() -> Weight;
    fn mark_batch_as_read(n: u32) -> Weight;
    fn mark_session_as_read(n: u32) -> Weight;
    fn archive_session() -> Weight;
    fn set_session_muted() -> Weight;
    fn set_session_pinned() -> Weight;
    fn cleanup_old_messages(n: u32) -> Weight;
    fn register_chat_user() -> Weight;
    fn update_chat_profile() -> Weight;
    fn set_user_status() -> Weight;
    fn update_privacy_settings() -> Weight;
}

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn send_message() -> Weight {
        Weight::from_parts(837_095_000, 6052)
            .saturating_add(T::DbWeight::get().reads(9))
            .saturating_add(T::DbWeight::get().writes(12))
    }
    fn mark_as_read() -> Weight {
        Weight::from_parts(42_927_000, 3710)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn delete_message() -> Weight {
        Weight::from_parts(36_050_000, 3710)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn recall_message() -> Weight {
        Weight::from_parts(45_232_000, 3710)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn mark_batch_as_read(n: u32) -> Weight {
        Weight::from_parts(29_007_387, 3549)
            .saturating_add(Weight::from_parts(21_005_521, 0).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes(1))
            .saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
            .saturating_add(Weight::from_parts(0, 2720).saturating_mul(n.into()))
    }
    fn mark_session_as_read(n: u32) -> Weight {
        Weight::from_parts(83_062_358, 3627)
            .saturating_add(Weight::from_parts(18_716_460, 0).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().reads((2_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes(2))
            .saturating_add(T::DbWeight::get().writes((1_u64).saturating_mul(n.into())))
            .saturating_add(Weight::from_parts(0, 2720).saturating_mul(n.into()))
    }
    fn archive_session() -> Weight {
        Weight::from_parts(37_323_000, 3627)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_session_muted() -> Weight {
        Weight::from_parts(40_320_000, 3627)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_session_pinned() -> Weight {
        Weight::from_parts(41_529_000, 3627)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn cleanup_old_messages(n: u32) -> Weight {
        Weight::from_parts(39_258_291, 3710)
            .saturating_add(Weight::from_parts(8_089_116, 0).saturating_mul(n.into()))
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().reads((1_u64).saturating_mul(n.into())))
            .saturating_add(T::DbWeight::get().writes(1))
            .saturating_add(Weight::from_parts(0, 2720).saturating_mul(n.into()))
    }
    fn register_chat_user() -> Weight {
        Weight::from_parts(68_141_000, 3521)
            .saturating_add(T::DbWeight::get().reads(5))
            .saturating_add(T::DbWeight::get().writes(5))
    }
    fn update_chat_profile() -> Weight {
        Weight::from_parts(50_144_000, 3933)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_user_status() -> Weight {
        Weight::from_parts(48_319_000, 3933)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn update_privacy_settings() -> Weight {
        Weight::from_parts(55_228_000, 3933)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
}
