//! Weight definitions for `pallet-chat-permission`.
//! `pallet-chat-permission` 的权重定义。
//!
//! EN: Ships placeholder weights derived from storage reads/writes. Replace with
//! benchmarked values produced by the node benchmark harness (`benchmarking.rs`)
//! before mainnet. CN: 暂用按存储读写估算的占位权重；上主网前用节点基准框架
//! 产出的实测值替换（见 `benchmarking.rs`）。

use frame_support::{traits::Get, weights::Weight};

/// EN: Weight functions needed by `pallet-chat-permission`.
/// CN: `pallet-chat-permission` 所需的权重函数。
pub trait WeightInfo {
    fn set_permission_level() -> Weight;
    fn set_rejected_scene_types() -> Weight;
    fn block_user() -> Weight;
    fn unblock_user() -> Weight;
    fn remove_friend() -> Weight;
    fn add_to_whitelist() -> Weight;
    fn remove_from_whitelist() -> Weight;
    fn request_friend() -> Weight;
    fn accept_friend() -> Weight;
    fn reject_friend() -> Weight;
    fn cancel_friend_request() -> Weight;
    fn set_friend_meta() -> Weight;
    fn force_mute_account() -> Weight;
    fn force_unmute_account() -> Weight;
    fn report() -> Weight;
    fn resolve_report() -> Weight;
}

/// Placeholder weights (constant base + reads/writes). / 占位权重（常量基数 + 读写计）。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

macro_rules! placeholder {
    ($name:ident, $reads:expr, $writes:expr) => {
        fn $name() -> Weight {
            Weight::from_parts(15_000_000, 0)
                .saturating_add(T::DbWeight::get().reads($reads))
                .saturating_add(T::DbWeight::get().writes($writes))
        }
    };
}

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    placeholder!(set_permission_level, 1, 1);
    placeholder!(set_rejected_scene_types, 1, 1);
    placeholder!(block_user, 1, 1);
    placeholder!(unblock_user, 1, 1);
    placeholder!(remove_friend, 1, 6);
    placeholder!(add_to_whitelist, 1, 1);
    placeholder!(remove_from_whitelist, 1, 1);
    placeholder!(request_friend, 3, 3);
    placeholder!(accept_friend, 2, 4);
    placeholder!(reject_friend, 2, 3);
    placeholder!(cancel_friend_request, 2, 3);
    placeholder!(set_friend_meta, 1, 2);
    placeholder!(force_mute_account, 0, 1);
    placeholder!(force_unmute_account, 0, 1);
    placeholder!(report, 3, 4);
    placeholder!(resolve_report, 1, 2);
}

impl WeightInfo for () {
    fn set_permission_level() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn set_rejected_scene_types() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn block_user() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn unblock_user() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn remove_friend() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn add_to_whitelist() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn remove_from_whitelist() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn request_friend() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn accept_friend() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn reject_friend() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn cancel_friend_request() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn set_friend_meta() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn force_mute_account() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn force_unmute_account() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn report() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn resolve_report() -> Weight { Weight::from_parts(15_000_000, 0) }
}
