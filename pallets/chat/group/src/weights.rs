//! Weight definitions for `pallet-chat-group`.
//! `pallet-chat-group` 的权重定义。
//!
//! EN: Weights measured via the node benchmark harness (`benchmarking.rs`),
//! generated on a dev chain (steps=50, repeat=20). Exceptions: `disband_group`
//! and `force_disband_group` keep a bounded-budget formula scaled by
//! `MAX_DISBAND_ITEMS_PER_CALL`, because their benchmark only seeds an
//! owner-only group and so does not measure the worst-case teardown. Re-run on
//! reference hardware before mainnet. CN: 由节点基准框架（见 `benchmarking.rs`）
//! 在 dev 链实测（steps=50, repeat=20）。例外：`disband_group` 与
//! `force_disband_group` 仍按 `MAX_DISBAND_ITEMS_PER_CALL` 预算线性计量——其基准
//! 仅种子「仅群主」群，未覆盖最坏情况拆除。上主网前应在基准硬件重跑。

use frame_support::{traits::Get, weights::Weight};

/// Weight functions needed for the pallet. / 模块所需权重函数。
pub trait WeightInfo {
    fn publish_key_package() -> Weight;
    fn revoke_key_package() -> Weight;
    fn create_group() -> Weight;
    /// EN: `a` = added members, `r` = removed members in the commit's delta.
    /// CN: `a` = 本次 commit delta 中新增成员数，`r` = 移除成员数。
    fn commit(a: u32, r: u32) -> Weight;
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

/// Benchmarked weights. / 实测权重。
pub struct SubstrateWeight<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn publish_key_package() -> Weight {
        Weight::from_parts(88_696_000, 3521)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(3))
    }
    fn revoke_key_package() -> Weight {
        Weight::from_parts(77_266_000, 7627)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn create_group() -> Weight {
        Weight::from_parts(103_188_000, 7515)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(6))
    }
    fn claim_welcome() -> Weight {
        Weight::from_parts(43_861_000, 11731)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn anchor_message_digest() -> Weight {
        Weight::from_parts(81_246_000, 3698)
            .saturating_add(T::DbWeight::get().reads(5))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn request_join() -> Weight {
        Weight::from_parts(88_518_000, 3698)
            .saturating_add(T::DbWeight::get().reads(8))
            .saturating_add(T::DbWeight::get().writes(3))
    }
    fn cancel_join_request() -> Weight {
        Weight::from_parts(54_126_000, 3541)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(3))
    }
    fn approve_join() -> Weight {
        Weight::from_parts(78_161_000, 6110)
            .saturating_add(T::DbWeight::get().reads(7))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    /// Storage: GroupMls (r:1 w:1), GroupMembers (r:5 w:2), SceneAuthorizations
    /// (r:5 w:5) — re-keys every member's scene authorization to the new owner,
    /// so reads/writes and proof size scale with group size. Worst case at the
    /// `MaxGroupMembers` bound. / 转让群主会为每个成员重写场景授权，读写与证明
    /// 大小随群规模增长，最坏情况取成员上限。
    fn transfer_ownership() -> Weight {
        Weight::from_parts(209_783_000, 82335)
            .saturating_add(T::DbWeight::get().reads(11))
            .saturating_add(T::DbWeight::get().writes(8))
    }
    fn set_admin() -> Weight {
        Weight::from_parts(53_520_000, 3698)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_group_profile() -> Weight {
        Weight::from_parts(48_881_000, 5767)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_group_nickname() -> Weight {
        Weight::from_parts(41_874_000, 3550)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn ban_member() -> Weight {
        Weight::from_parts(66_609_000, 3698)
            .saturating_add(T::DbWeight::get().reads(4))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn unban_member() -> Weight {
        Weight::from_parts(57_256_000, 3698)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_member_mute() -> Weight {
        Weight::from_parts(63_730_000, 6110)
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_group_mute_all() -> Weight {
        Weight::from_parts(46_274_000, 3698)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn set_group_frozen() -> Weight {
        Weight::from_parts(35_886_000, 3698)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }

    // commit 按成员增减规模线性计量（实测斜率）：基线 + 每个 added/removed 的时间与读写。
    // commit scales with the membership delta (measured slopes): base + per-added /
    // per-removed time and DB reads/writes.
    fn commit(a: u32, r: u32) -> Weight {
        Weight::from_parts(96_507_424, 3698)
            .saturating_add(Weight::from_parts(69_866_354, 16269).saturating_mul(a.into()))
            .saturating_add(Weight::from_parts(66_100_915, 16269).saturating_mul(r.into()))
            .saturating_add(T::DbWeight::get().reads(5))
            .saturating_add(T::DbWeight::get().reads((6u64).saturating_mul(a.into())))
            .saturating_add(T::DbWeight::get().reads((3u64).saturating_mul(r.into())))
            .saturating_add(T::DbWeight::get().writes(3))
            .saturating_add(T::DbWeight::get().writes((5u64).saturating_mul(a.into())))
            .saturating_add(T::DbWeight::get().writes((6u64).saturating_mul(r.into())))
    }

    // 解散为有界拆除（审计 B4）：单次最多处理 MAX_DISBAND_ITEMS_PER_CALL 个成员 + 8 个前缀。
    // 基准仅种子「仅群主」群，未覆盖最坏拆除，故保留按预算线性计量（基线时间取实测值）。
    // Disband is bounded per call (audit B4); the benchmark only seeds an owner-only
    // group, so we keep the per-call budget formula (base time set from the measurement).
    fn disband_group() -> Weight {
        let n = crate::MAX_DISBAND_ITEMS_PER_CALL as u64;
        Weight::from_parts(212_492_000, 7515)
            .saturating_add(T::DbWeight::get().reads(3u64.saturating_add(n.saturating_mul(2))))
            .saturating_add(T::DbWeight::get().writes(8u64.saturating_add(n.saturating_mul(10))))
    }
    fn force_disband_group() -> Weight {
        <Self as WeightInfo>::disband_group()
    }
}

impl WeightInfo for () {
    fn publish_key_package() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn revoke_key_package() -> Weight { Weight::from_parts(20_000_000, 0) }
    fn create_group() -> Weight { Weight::from_parts(30_000_000, 0) }
    fn commit(_a: u32, _r: u32) -> Weight { Weight::from_parts(40_000_000, 0) }
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
