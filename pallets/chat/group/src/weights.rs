//! Weight definitions for `pallet-chat-group`.
//! `pallet-chat-group` 的权重定义。
//!
//! EN: Ships placeholder weights derived from storage reads/writes. Replace with
//! benchmarked values produced by the node benchmark harness (`benchmarking.rs`)
//! before mainnet. CN: 暂用按存储读写估算的占位权重；上主网前用节点基准框架
//! 产出的实测值替换（见 `benchmarking.rs`）。

use frame_support::{traits::Get, weights::Weight};

/// Weight functions needed for the pallet. / 模块所需权重函数。
pub trait WeightInfo {
    fn publish_key_package() -> Weight;
    fn revoke_key_package() -> Weight;
    fn create_group() -> Weight;
    fn commit() -> Weight;
    fn claim_welcome() -> Weight;
    fn disband_group() -> Weight;
    fn anchor_message_digest() -> Weight;
    fn request_join() -> Weight;
    fn cancel_join_request() -> Weight;
    fn approve_join() -> Weight;
    fn transfer_ownership() -> Weight;
    fn set_admin() -> Weight;
    fn set_group_profile() -> Weight;
    fn set_group_nickname() -> Weight;
    fn ban_member() -> Weight;
    fn unban_member() -> Weight;
    fn set_member_mute() -> Weight;
    fn set_group_mute_all() -> Weight;
    fn force_disband_group() -> Weight;
    fn set_group_frozen() -> Weight;
}

/// Placeholder weights (constant base + reads/writes). / 占位权重（常量基数 + 读写计）。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

macro_rules! placeholder {
    ($name:ident, $reads:expr, $writes:expr) => {
        fn $name() -> Weight {
            Weight::from_parts(20_000_000, 0)
                .saturating_add(T::DbWeight::get().reads($reads))
                .saturating_add(T::DbWeight::get().writes($writes))
        }
    };
}

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    placeholder!(publish_key_package, 2, 3);
    placeholder!(revoke_key_package, 2, 3);
    placeholder!(create_group, 3, 6);
    placeholder!(commit, 4, 6);
    placeholder!(claim_welcome, 1, 1);
    placeholder!(disband_group, 3, 8);
    placeholder!(anchor_message_digest, 2, 1);
    placeholder!(request_join, 4, 2);
    placeholder!(cancel_join_request, 1, 2);
    placeholder!(approve_join, 4, 1);
    placeholder!(transfer_ownership, 3, 3);
    placeholder!(set_admin, 2, 1);
    placeholder!(set_group_profile, 2, 1);
    placeholder!(set_group_nickname, 1, 1);
    placeholder!(ban_member, 3, 3);
    placeholder!(unban_member, 2, 1);
    placeholder!(set_member_mute, 3, 1);
    placeholder!(set_group_mute_all, 2, 1);
    placeholder!(force_disband_group, 3, 8);
    placeholder!(set_group_frozen, 1, 1);
}

impl WeightInfo for () {
    fn publish_key_package() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn revoke_key_package() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn create_group() -> Weight { Weight::from_parts(30_000_000, 0) }
    fn commit() -> Weight { Weight::from_parts(40_000_000, 0) }
    fn claim_welcome() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn disband_group() -> Weight { Weight::from_parts(40_000_000, 0) }
    fn anchor_message_digest() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn request_join() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn cancel_join_request() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn approve_join() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn transfer_ownership() -> Weight { Weight::from_parts(25_000_000, 0) }
    fn set_admin() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_group_profile() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_group_nickname() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn ban_member() -> Weight { Weight::from_parts(25_000_000, 0) }
    fn unban_member() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_member_mute() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn set_group_mute_all() -> Weight { Weight::from_parts(15_000_000, 0) }
    fn force_disband_group() -> Weight { Weight::from_parts(40_000_000, 0) }
    fn set_group_frozen() -> Weight { Weight::from_parts(15_000_000, 0) }
}
