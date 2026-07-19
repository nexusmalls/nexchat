//! # Storage Lifecycle Pallet
//!
//! Tiered storage-lifecycle management for archival workflows.
//! 存储生命周期管理模块，提供分级归档框架。
//!
//! ## Features
//! ## 功能
//! - three storage stages plus purge: Active → L1 archive → L2 archive → purge
//! - 支持三级存储：活跃 → L1 归档 → L2 归档 → 清除
//! - automatic archive processing inside `on_idle`
//! - 在 `on_idle` 中自动处理归档任务
//! - bridges concrete storage modules through the `StorageArchiver` trait
//! - 通过 `StorageArchiver` trait 桥接具体存储模块
//!
//! ## Archive policy
//! ## 归档策略
//! - L1 archive: keep core fields and compress storage (~50–80% savings)
//! - L1 归档：保留核心字段，压缩存储（~50–80% 节省）
//! - L2 archive: keep only statistical summaries (~90%+ savings)
//! - L2 归档：仅保留统计摘要（~90%+ 节省）
//! - purge: fully remove data while keeping permanent statistics
//! - 清除：完全删除，仅保留永久统计

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
pub mod weights;

pub mod runtime_api;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::pallet_prelude::*;
use frame_system::ensure_root;
use frame_system::pallet_prelude::OriginFor;
use sp_std::marker::PhantomData;

/// Archive state.
/// 归档状态。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ArchiveLevel {
    /// Active data with full storage retained.
    /// 活跃数据（完整存储）
    Active,
    /// Level-1 archive with reduced storage.
    /// 一级归档（精简存储）
    ArchivedL1,
    /// Level-2 archive with minimal storage.
    /// 二级归档（最小存储）
    ArchivedL2,
    /// Purged data with only statistics retained.
    /// 已清除（仅统计）
    Purged,
}

impl Default for ArchiveLevel {
    fn default() -> Self {
        Self::Active
    }
}

impl ArchiveLevel {
    /// Converts the archive level to `u8`.
    /// 转换为 `u8`
    pub fn to_u8(&self) -> u8 {
        match self {
            Self::Active => 0,
            Self::ArchivedL1 => 1,
            Self::ArchivedL2 => 2,
            Self::Purged => 3,
        }
    }

    /// 从 u8 转换
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Active,
            1 => Self::ArchivedL1,
            2 => Self::ArchivedL2,
            3 => Self::Purged,
            _ => Self::Active,
        }
    }
}

/// Storage archiver trait with multi-level archive support.
/// 存储归档器 Trait（D2：多级操作支持）。
///
/// Implemented by `pallet-storage-service` and called by the lifecycle pallet.
/// 由 `pallet-storage-service` 实现，供 lifecycle pallet 调用。
/// It decouples archive policy from the concrete storage implementation.
/// 解耦归档逻辑与具体存储实现。
pub trait StorageArchiver {
    /// 扫描可归档的记录ID（已过期超过 delay 区块的 CID）
    fn scan_archivable(delay: u64, max_count: u32) -> sp_std::vec::Vec<u64>;

    /// 执行归档清理（删除链上存储记录）
    fn archive_records(ids: &[u64]);

    /// 按目标归档级别扫描可处理的记录 (D2)
    fn scan_for_level(
        _data_type: &[u8],
        target_level: ArchiveLevel,
        delay: u64,
        max_count: u32,
    ) -> sp_std::vec::Vec<u64> {
        if matches!(target_level, ArchiveLevel::Purged) {
            Self::scan_archivable(delay, max_count)
        } else {
            sp_std::vec::Vec::new()
        }
    }

    /// 执行指定级别的归档操作 (D2)
    fn archive_to_level(_data_type: &[u8], ids: &[u64], target_level: ArchiveLevel) {
        if matches!(target_level, ArchiveLevel::Purged) {
            Self::archive_records(ids);
        }
    }

    /// 返回已注册的数据类型列表 (D4)
    fn registered_data_types() -> sp_std::vec::Vec<sp_std::vec::Vec<u8>> {
        sp_std::vec![b"pin_storage".to_vec()]
    }

    /// 查询数据当前归档级别 (U1)
    fn query_archive_level(_data_type: &[u8], _data_id: u64) -> ArchiveLevel {
        ArchiveLevel::Active
    }

    /// 请求恢复已归档数据到 Active (U4)
    fn restore_record(_data_type: &[u8], _data_id: u64, _from_level: ArchiveLevel) -> bool {
        false
    }
}

/// 空实现（runtime 未接入 service pallet 时使用）
impl StorageArchiver for () {
    fn scan_archivable(_delay: u64, _max_count: u32) -> sp_std::vec::Vec<u64> {
        sp_std::vec::Vec::new()
    }
    fn archive_records(_ids: &[u64]) {}
}

/// Archive callback trait.
/// 归档回调 Trait（D3）。
///
/// Notifies downstream pallets after archive completion.
/// 归档完成后通知下游 pallet。
pub trait OnArchiveHandler {
    fn on_archived(
        data_type: &[u8],
        data_id: u64,
        from_level: ArchiveLevel,
        to_level: ArchiveLevel,
    );
}

impl OnArchiveHandler for () {
    fn on_archived(_: &[u8], _: u64, _: ArchiveLevel, _: ArchiveLevel) {}
}

/// Data-ownership lookup trait for user-scoped permissions.
/// 数据所有权查询 Trait（用户权限分层）。
///
/// Used by `extend_active_period` and `restore_from_archive` to authorize signed users.
/// 用于 `extend_active_period` 和 `restore_from_archive` 的签名用户鉴权。
/// For non-root callers, the runtime checks whether they own the target data.
/// 当用户（非 Root）调用时，需验证其是否为数据所有者。
pub trait DataOwnerProvider<AccountId> {
    /// 检查 `who` 是否为 `data_type` + `data_id` 的所有者
    fn is_owner(who: &AccountId, data_type: &[u8], data_id: u64) -> bool;
}

/// 空实现：拒绝所有非 Root 用户（向后兼容）
impl<AccountId> DataOwnerProvider<AccountId> for () {
    fn is_owner(_who: &AccountId, _data_type: &[u8], _data_id: u64) -> bool {
        false
    }
}

/// Global archive configuration (G1: runtime-adjustable).
/// 归档全局配置（G1：运行时可调）。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ArchiveConfig {
    /// Delay in blocks before data can move from Active to L1 archive.
    /// 从 Active 进入 L1 归档前的延迟区块数
    pub l1_delay: u32,
    /// Delay in blocks before L1 data can move to L2 archive.
    /// 从 L1 进入 L2 归档前的延迟区块数
    pub l2_delay: u32,
    /// Delay in blocks before archived data can be purged.
    /// 数据可被清除前的延迟区块数
    pub purge_delay: u32,
    /// Whether purge is enabled.
    /// 是否启用清除功能
    pub purge_enabled: bool,
    /// Maximum number of records processed in one batch.
    /// 单批次最大处理数量
    pub max_batch_size: u32,
}

/// Per-data-type archive policy (G3: differentiated archival).
/// 按数据类型的归档策略（G3：差异化归档）。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ArchivePolicy {
    /// Per-type delay in blocks before data can move from Active to L1 archive.
    /// 按数据类型生效的 Active → L1 延迟区块数
    pub l1_delay: u32,
    /// Per-type delay in blocks before L1 data can move to L2 archive.
    /// 按数据类型生效的 L1 → L2 延迟区块数
    pub l2_delay: u32,
    /// Per-type delay in blocks before data can be purged.
    /// 按数据类型生效的清除延迟区块数
    pub purge_delay: u32,
    /// Whether purge is enabled for this data type.
    /// 当前数据类型是否启用清除
    pub purge_enabled: bool,
}

/// WeightInfo trait for the O4 benchmark-weight framework.
/// WeightInfo trait（O4：Benchmark 权重框架）。
pub trait WeightInfo {
    fn set_archive_config() -> Weight;
    fn pause_archival() -> Weight;
    fn resume_archival() -> Weight;
    fn set_archive_policy() -> Weight;
    fn force_archive() -> Weight;
    fn protect_from_purge() -> Weight;
    fn remove_purge_protection() -> Weight;
    fn extend_active_period() -> Weight;
    fn restore_from_archive() -> Weight;
}

/// 默认权重实现（re-export from weights module）
pub use weights::SubstrateWeight;

/// Archive batch metadata.
/// 归档批次信息。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct ArchiveBatch {
    /// 批次ID
    pub batch_id: u64,
    /// 数据ID范围起始
    pub id_start: u64,
    /// 数据ID范围结束
    pub id_end: u64,
    /// 归档数量
    pub count: u32,
    /// 归档时间
    pub archived_at: u64,
    /// 归档级别 (0=Active, 1=L1, 2=L2, 3=Purged)
    pub level: u8,
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// L1 archive delay in blocks. Controls when completed data becomes eligible for L1.
        /// L1 归档延迟（区块数），控制数据多久后可进入 L1
        #[pallet::constant]
        type L1ArchiveDelay: Get<u32>;

        /// L2 archive delay in blocks. Controls when L1 data can move into L2.
        /// L2 归档延迟（区块数），控制 L1 数据多久后可进入 L2
        #[pallet::constant]
        type L2ArchiveDelay: Get<u32>;

        /// Purge delay in blocks. Controls when L2 data becomes purgeable.
        /// 清除延迟（区块数），控制 L2 数据多久后可被清除
        #[pallet::constant]
        type PurgeDelay: Get<u32>;

        /// Whether purge is enabled.
        /// 是否启用清除功能
        #[pallet::constant]
        type EnablePurge: Get<bool>;

        /// Maximum number of items processed during one `on_idle` run.
        /// 每次 `on_idle` 最大处理数量
        #[pallet::constant]
        type MaxBatchSize: Get<u32>;

        /// Storage archiver that provides scan and cleanup interfaces for archiveable records.
        /// 存储归档器：提供可归档记录的扫描与清理接口
        type StorageArchiver: StorageArchiver;

        /// Archive callback handler.
        /// 归档回调处理器（D3）
        type OnArchive: OnArchiveHandler;

        /// Data-owner lookup used for user-scoped permission checks on
        /// `extend_active_period` and `restore_from_archive`.
        /// 数据所有权查询（用户权限分层），用于 `extend_active_period` /
        /// `restore_from_archive` 的签名用户鉴权
        type DataOwnerProvider: DataOwnerProvider<Self::AccountId>;

        /// Governance origin for archive policy management.
        /// 归档策略治理 Origin。
        type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Weight provider.
        /// 权重信息（O4）
        type WeightInfo: WeightInfo;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<frame_system::pallet_prelude::BlockNumberFor<T>> for Pallet<T> {
        fn on_idle(
            n: frame_system::pallet_prelude::BlockNumberFor<T>,
            remaining_weight: Weight,
        ) -> Weight {
            // G2: 检查暂停状态
            if ArchivalPaused::<T>::get() {
                return Weight::zero();
            }

            let min_weight = Weight::from_parts(50_000_000, 5_000);
            if remaining_weight.any_lt(min_weight) {
                return Weight::zero();
            }

            let now: u64 = n.try_into().unwrap_or(0u64);
            let config = Pallet::<T>::effective_config();
            // D4: 多数据类型支持
            let data_types = T::StorageArchiver::registered_data_types();
            let mut total_processed = 0u32;
            let types_count = (data_types.len() as u32).max(1);
            let batch_per_type = config.max_batch_size / types_count;
            // M1-R3: 预算跟踪，避免无限制消耗资源
            let per_scan_weight = Weight::from_parts(5_000_000, 1_000);
            let per_item_weight = Weight::from_parts(10_000_000, 2_000);
            let mut budget_used = min_weight; // base overhead

            for dt_bytes in data_types.iter() {
                let data_type: BoundedVec<u8, ConstU32<32>> =
                    BoundedVec::truncate_from(dt_bytes.clone());
                let policy = Pallet::<T>::effective_policy(&data_type, &config);
                let phase_batch = (batch_per_type / 3).max(1);

                // D1 阶段 1: Active → L1
                let l1_ids = T::StorageArchiver::scan_for_level(
                    dt_bytes,
                    ArchiveLevel::ArchivedL1,
                    policy.l1_delay as u64,
                    phase_batch,
                );
                // U3: 过滤已延期的数据
                let l1_ids: sp_std::vec::Vec<u64> = l1_ids
                    .into_iter()
                    .filter(|id| {
                        let ext = ActiveExtensions::<T>::get(&data_type, *id);
                        ext == 0 || now >= ext
                    })
                    .collect();
                // M1-R3: 扫描计入预算
                budget_used = budget_used.saturating_add(per_scan_weight);
                if !l1_ids.is_empty() {
                    let count = l1_ids.len() as u32;
                    T::StorageArchiver::archive_to_level(
                        dt_bytes,
                        &l1_ids,
                        ArchiveLevel::ArchivedL1,
                    );
                    for id in &l1_ids {
                        // M1-R2: 读取实际 from_level，避免硬编码
                        let from_level = DataArchiveStatus::<T>::get(&data_type, *id);
                        DataArchiveStatus::<T>::insert(&data_type, *id, ArchiveLevel::ArchivedL1);
                        T::OnArchive::on_archived(
                            dt_bytes,
                            *id,
                            from_level,
                            ArchiveLevel::ArchivedL1,
                        );
                        ActiveExtensions::<T>::remove(&data_type, *id);
                    }
                    let _ = StorageLifecycleManager::<T>::record_batch(
                        data_type.clone(),
                        *l1_ids.first().unwrap_or(&0),
                        *l1_ids.last().unwrap_or(&0),
                        count,
                        ArchiveLevel::ArchivedL1,
                        now,
                    );
                    Self::deposit_event(Event::ArchivedToL1 {
                        data_type: data_type.clone(),
                        count,
                        saved_bytes: 0,
                    });
                    total_processed = total_processed.saturating_add(count);
                    budget_used =
                        budget_used.saturating_add(per_item_weight.saturating_mul(count as u64));
                }
                // M1-R3: 超出预算则中止后续阶段
                if budget_used.any_gt(remaining_weight) {
                    break;
                }

                // D1 阶段 2: L1 → L2
                let l2_ids = T::StorageArchiver::scan_for_level(
                    dt_bytes,
                    ArchiveLevel::ArchivedL2,
                    policy.l2_delay as u64,
                    phase_batch,
                );
                budget_used = budget_used.saturating_add(per_scan_weight);
                if !l2_ids.is_empty() {
                    let count = l2_ids.len() as u32;
                    T::StorageArchiver::archive_to_level(
                        dt_bytes,
                        &l2_ids,
                        ArchiveLevel::ArchivedL2,
                    );
                    for id in &l2_ids {
                        // M1-R2: 读取实际 from_level
                        let from_level = DataArchiveStatus::<T>::get(&data_type, *id);
                        DataArchiveStatus::<T>::insert(&data_type, *id, ArchiveLevel::ArchivedL2);
                        T::OnArchive::on_archived(
                            dt_bytes,
                            *id,
                            from_level,
                            ArchiveLevel::ArchivedL2,
                        );
                    }
                    let _ = StorageLifecycleManager::<T>::record_batch(
                        data_type.clone(),
                        *l2_ids.first().unwrap_or(&0),
                        *l2_ids.last().unwrap_or(&0),
                        count,
                        ArchiveLevel::ArchivedL2,
                        now,
                    );
                    Self::deposit_event(Event::ArchivedToL2 {
                        data_type: data_type.clone(),
                        count,
                        saved_bytes: 0,
                    });
                    total_processed = total_processed.saturating_add(count);
                    budget_used =
                        budget_used.saturating_add(per_item_weight.saturating_mul(count as u64));
                }
                if budget_used.any_gt(remaining_weight) {
                    break;
                }

                // D1 阶段 3: L2 → Purge
                if policy.purge_enabled {
                    let purge_ids = T::StorageArchiver::scan_for_level(
                        dt_bytes,
                        ArchiveLevel::Purged,
                        policy.purge_delay as u64,
                        phase_batch,
                    );
                    // G4: 过滤受保护的数据
                    let purge_ids: sp_std::vec::Vec<u64> = purge_ids
                        .into_iter()
                        .filter(|id| !PurgeProtected::<T>::get(&data_type, *id))
                        .collect();
                    budget_used = budget_used.saturating_add(per_scan_weight);
                    if !purge_ids.is_empty() {
                        let count = purge_ids.len() as u32;
                        T::StorageArchiver::archive_to_level(
                            dt_bytes,
                            &purge_ids,
                            ArchiveLevel::Purged,
                        );
                        for id in &purge_ids {
                            // M1-R2: 读取实际 from_level
                            let from_level = DataArchiveStatus::<T>::get(&data_type, *id);
                            DataArchiveStatus::<T>::insert(&data_type, *id, ArchiveLevel::Purged);
                            T::OnArchive::on_archived(
                                dt_bytes,
                                *id,
                                from_level,
                                ArchiveLevel::Purged,
                            );
                        }
                        let _ = StorageLifecycleManager::<T>::record_batch(
                            data_type.clone(),
                            *purge_ids.first().unwrap_or(&0),
                            *purge_ids.last().unwrap_or(&0),
                            count,
                            ArchiveLevel::Purged,
                            now,
                        );
                        Self::deposit_event(Event::DataPurged {
                            data_type: data_type.clone(),
                            count,
                        });
                        total_processed = total_processed.saturating_add(count);
                        budget_used = budget_used
                            .saturating_add(per_item_weight.saturating_mul(count as u64));
                    }
                }
                if budget_used.any_gt(remaining_weight) {
                    break;
                }

                // U2: 归档前预警 — 扫描即将达到 L1 归档条件的数据
                let warning_delay = (policy.l1_delay as u64) * 4 / 5; // 80% of L1 delay
                let approaching = T::StorageArchiver::scan_for_level(
                    dt_bytes,
                    ArchiveLevel::ArchivedL1,
                    warning_delay,
                    phase_batch,
                );
                // 排除已在本轮归档的 ID（已处理过）
                let approaching_count =
                    approaching.iter().filter(|id| !l1_ids.contains(id)).count() as u32;
                if approaching_count > 0 {
                    Self::deposit_event(Event::ArchivalWarning {
                        data_type: data_type.clone(),
                        approaching_count,
                    });
                }

                budget_used = budget_used.saturating_add(per_scan_weight);

                // O3: 积压告警 — 检查是否有大量待处理记录
                // M2-R2: 扫描上限必须 > 阈值，否则条件永远为 false
                let backlog_threshold = config.max_batch_size.saturating_mul(3);
                let pending = T::StorageArchiver::scan_for_level(
                    dt_bytes,
                    ArchiveLevel::ArchivedL1,
                    0,
                    backlog_threshold.saturating_add(1),
                )
                .len() as u32;
                if pending > backlog_threshold {
                    Self::deposit_event(Event::ArchivalBacklog {
                        data_type: data_type.clone(),
                        pending_count: pending,
                    });
                }
                budget_used = budget_used.saturating_add(per_scan_weight);
            }

            // H1-R2: 确保不超过 remaining_weight（Substrate on_idle 契约）
            // M1-R3: budget_used 已在循环中精确跟踪
            budget_used.min(remaining_weight)
        }

        fn integrity_test() {
            assert!(T::L1ArchiveDelay::get() > 0, "L1ArchiveDelay must be > 0");
            assert!(T::L2ArchiveDelay::get() > 0, "L2ArchiveDelay must be > 0");
            assert!(T::MaxBatchSize::get() > 0, "MaxBatchSize must be > 0");
            assert!(
                T::L1ArchiveDelay::get() <= T::L2ArchiveDelay::get(),
                "L1ArchiveDelay must be <= L2ArchiveDelay"
            );
        }
    }

    /// 归档游标（按数据类型）
    #[pallet::storage]
    #[pallet::getter(fn archive_cursor)]
    pub type ArchiveCursor<T: Config> =
        StorageMap<_, Blake2_128Concat, BoundedVec<u8, ConstU32<32>>, u64, ValueQuery>;

    /// 归档批次记录
    #[pallet::storage]
    #[pallet::getter(fn archive_batches)]
    pub type ArchiveBatches<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        BoundedVec<ArchiveBatch, ConstU32<100>>,
        ValueQuery,
    >;

    /// 归档统计
    #[pallet::storage]
    #[pallet::getter(fn archive_stats)]
    pub type ArchiveStats<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        ArchiveStatistics,
        ValueQuery,
    >;

    /// G2: 归档暂停标志
    #[pallet::storage]
    pub type ArchivalPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    /// G1: 运行时可调归档配置覆盖
    #[pallet::storage]
    pub type ArchiveConfigOverride<T: Config> = StorageValue<_, ArchiveConfig>;

    /// G3: 按数据类型的归档策略
    #[pallet::storage]
    pub type ArchivePolicies<T: Config> =
        StorageMap<_, Blake2_128Concat, BoundedVec<u8, ConstU32<32>>, ArchivePolicy>;

    /// U1: 数据归档状态跟踪
    #[pallet::storage]
    pub type DataArchiveStatus<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        Blake2_128Concat,
        u64,
        ArchiveLevel,
        ValueQuery,
    >;

    /// G4: 清除保护标志
    #[pallet::storage]
    pub type PurgeProtected<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        Blake2_128Concat,
        u64,
        bool,
        ValueQuery,
    >;

    /// U3: Active 延期（数据 ID → 延期截止区块号）
    #[pallet::storage]
    pub type ActiveExtensions<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        BoundedVec<u8, ConstU32<32>>,
        Blake2_128Concat,
        u64,
        u64,
        ValueQuery,
    >;

    /// O1: 累计批次计数
    #[pallet::storage]
    pub type TotalBatchCount<T: Config> =
        StorageMap<_, Blake2_128Concat, BoundedVec<u8, ConstU32<32>>, u64, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 数据已归档到L1
        ArchivedToL1 {
            data_type: BoundedVec<u8, ConstU32<32>>,
            count: u32,
            saved_bytes: u64,
        },
        /// 数据已归档到L2
        ArchivedToL2 {
            data_type: BoundedVec<u8, ConstU32<32>>,
            count: u32,
            saved_bytes: u64,
        },
        /// 数据已清除
        DataPurged {
            data_type: BoundedVec<u8, ConstU32<32>>,
            count: u32,
        },
        /// G1: 归档配置已更新
        ArchiveConfigUpdated { config: ArchiveConfig },
        /// G2: 归档已暂停
        ArchivalPausedEvent,
        /// G2: 归档已恢复
        ArchivalResumedEvent,
        /// G3: 归档策略已设置
        ArchivePolicySet {
            data_type: BoundedVec<u8, ConstU32<32>>,
            policy: ArchivePolicy,
        },
        /// G4: 清除保护状态变更
        PurgeProtectionChanged {
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
            protected: bool,
        },
        /// G4: 数据强制归档
        DataForceArchived {
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_ids: BoundedVec<u64, ConstU32<100>>,
            target_level: u8,
        },
        /// U3: Active 期延长
        ActivePeriodExtended {
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
            extended_until: u64,
        },
        /// U4: 数据已恢复
        DataRestored {
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
            from_level: u8,
        },
        /// U2: 归档前预警（数据即将被归档到 L1）
        ArchivalWarning {
            data_type: BoundedVec<u8, ConstU32<32>>,
            approaching_count: u32,
        },
        /// O3: 归档积压告警
        ArchivalBacklog {
            data_type: BoundedVec<u8, ConstU32<32>>,
            pending_count: u32,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// 归档批次已满
        BatchQueueFull,
        /// 数据状态不允许归档
        InvalidArchiveState,
        /// 归档已暂停
        ArchivalAlreadyPaused,
        /// 归档未暂停
        ArchivalNotPaused,
        /// 无法从该级别恢复
        CannotRestoreFromLevel,
        /// 配置参数无效
        InvalidConfig,
        /// 延期太短
        ExtensionTooShort,
        /// 数据已受保护
        AlreadyProtected,
        /// 数据未受保护
        NotProtected,
        /// 恢复失败
        RestoreFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// G1: 设置归档全局配置（运行时可调）
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_archive_config())]
        pub fn set_archive_config(origin: OriginFor<T>, config: ArchiveConfig) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                config.l1_delay > 0 && config.l2_delay > 0 && config.max_batch_size > 0,
                Error::<T>::InvalidConfig
            );
            // M1-R1: purge_enabled 时 purge_delay 必须 > 0
            ensure!(
                !config.purge_enabled || config.purge_delay > 0,
                Error::<T>::InvalidConfig
            );
            // M2-R3: 延迟必须递增 l1 <= l2，且 purge_enabled 时 l2 <= purge
            ensure!(
                config.l1_delay <= config.l2_delay,
                Error::<T>::InvalidConfig
            );
            if config.purge_enabled {
                ensure!(
                    config.l2_delay <= config.purge_delay,
                    Error::<T>::InvalidConfig
                );
            }
            ArchiveConfigOverride::<T>::put(config.clone());
            Self::deposit_event(Event::ArchiveConfigUpdated { config });
            Ok(())
        }

        /// G2: 暂停归档
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::pause_archival())]
        pub fn pause_archival(origin: OriginFor<T>) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                !ArchivalPaused::<T>::get(),
                Error::<T>::ArchivalAlreadyPaused
            );
            ArchivalPaused::<T>::put(true);
            Self::deposit_event(Event::ArchivalPausedEvent);
            Ok(())
        }

        /// G2: 恢复归档
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::resume_archival())]
        pub fn resume_archival(origin: OriginFor<T>) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(ArchivalPaused::<T>::get(), Error::<T>::ArchivalNotPaused);
            ArchivalPaused::<T>::put(false);
            Self::deposit_event(Event::ArchivalResumedEvent);
            Ok(())
        }

        /// G3: 设置按数据类型的归档策略
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_archive_policy())]
        pub fn set_archive_policy(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            policy: ArchivePolicy,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                policy.l1_delay > 0 && policy.l2_delay > 0,
                Error::<T>::InvalidConfig
            );
            // M1-R1: purge_enabled 时 purge_delay 必须 > 0
            ensure!(
                !policy.purge_enabled || policy.purge_delay > 0,
                Error::<T>::InvalidConfig
            );
            // M2-R3: 延迟必须递增 l1 <= l2，且 purge_enabled 时 l2 <= purge
            ensure!(
                policy.l1_delay <= policy.l2_delay,
                Error::<T>::InvalidConfig
            );
            if policy.purge_enabled {
                ensure!(
                    policy.l2_delay <= policy.purge_delay,
                    Error::<T>::InvalidConfig
                );
            }
            ArchivePolicies::<T>::insert(&data_type, policy.clone());
            Self::deposit_event(Event::ArchivePolicySet { data_type, policy });
            Ok(())
        }

        /// G4: 强制归档指定数据
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::force_archive())]
        pub fn force_archive(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_ids: BoundedVec<u64, ConstU32<100>>,
            target_level: u8,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            let level = ArchiveLevel::from_u8(target_level);
            ensure!(
                !matches!(level, ArchiveLevel::Active),
                Error::<T>::InvalidArchiveState
            );
            let ids: sp_std::vec::Vec<u64> = data_ids.to_vec();
            // H1-R3: 过滤掉后退/同级转换（Purged→L1 等无意义操作）
            // M3-R4: 缓存 from_level 避免二次读取
            let forward_pairs: sp_std::vec::Vec<(u64, ArchiveLevel)> = ids
                .iter()
                .filter_map(|id| {
                    let current = DataArchiveStatus::<T>::get(&data_type, *id);
                    if current.to_u8() < level.to_u8() {
                        Some((*id, current))
                    } else {
                        None
                    }
                })
                .collect();
            let forward_ids: sp_std::vec::Vec<u64> =
                forward_pairs.iter().map(|(id, _)| *id).collect();
            T::StorageArchiver::archive_to_level(&data_type, &forward_ids, level);
            for (id, from_level) in &forward_pairs {
                DataArchiveStatus::<T>::insert(&data_type, *id, level);
                // H2-R1: 通知下游 pallet
                T::OnArchive::on_archived(&data_type, *id, *from_level, level);
                // M3-R2: 清理延期和保护标志
                ActiveExtensions::<T>::remove(&data_type, *id);
                if matches!(level, ArchiveLevel::Purged) {
                    PurgeProtected::<T>::remove(&data_type, *id);
                }
            }
            // M2-R4: 事件仅报告实际归档的 forward_ids
            let archived_ids = BoundedVec::truncate_from(forward_ids);
            Self::deposit_event(Event::DataForceArchived {
                data_type,
                data_ids: archived_ids,
                target_level,
            });
            Ok(())
        }

        /// G4: 设置清除保护
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::protect_from_purge())]
        pub fn protect_from_purge(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                !PurgeProtected::<T>::get(&data_type, data_id),
                Error::<T>::AlreadyProtected
            );
            PurgeProtected::<T>::insert(&data_type, data_id, true);
            Self::deposit_event(Event::PurgeProtectionChanged {
                data_type,
                data_id,
                protected: true,
            });
            Ok(())
        }

        /// G4: 移除清除保护
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::remove_purge_protection())]
        pub fn remove_purge_protection(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
        ) -> DispatchResult {
            T::GovernanceOrigin::ensure_origin(origin)?;
            ensure!(
                PurgeProtected::<T>::get(&data_type, data_id),
                Error::<T>::NotProtected
            );
            PurgeProtected::<T>::remove(&data_type, data_id);
            Self::deposit_event(Event::PurgeProtectionChanged {
                data_type,
                data_id,
                protected: false,
            });
            Ok(())
        }

        /// U3: 延长数据 Active 期（Root 或数据所有者）
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::extend_active_period())]
        pub fn extend_active_period(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
            extend_blocks: u64,
        ) -> DispatchResult {
            // 支持 Root 和签名用户（所有者）
            let maybe_who = match ensure_root(origin.clone()) {
                Ok(()) => None,
                Err(_) => {
                    let who = frame_system::ensure_signed(origin)?;
                    ensure!(
                        T::DataOwnerProvider::is_owner(&who, &data_type, data_id),
                        Error::<T>::InvalidArchiveState
                    );
                    Some(who)
                }
            };
            let _ = maybe_who; // 仅用于鉴权
            ensure!(extend_blocks >= 100, Error::<T>::ExtensionTooShort);
            let current_level = DataArchiveStatus::<T>::get(&data_type, data_id);
            ensure!(
                matches!(current_level, ArchiveLevel::Active),
                Error::<T>::InvalidArchiveState
            );
            let n: u64 = <frame_system::Pallet<T>>::block_number()
                .try_into()
                .unwrap_or(0u64);
            let current_ext = ActiveExtensions::<T>::get(&data_type, data_id);
            let base = if current_ext > n { current_ext } else { n };
            let new_until = base.saturating_add(extend_blocks);
            ActiveExtensions::<T>::insert(&data_type, data_id, new_until);
            Self::deposit_event(Event::ActivePeriodExtended {
                data_type,
                data_id,
                extended_until: new_until,
            });
            Ok(())
        }

        /// U4: 从归档恢复数据（仅支持 L1 → Active）（Root 或数据所有者）
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::restore_from_archive())]
        pub fn restore_from_archive(
            origin: OriginFor<T>,
            data_type: BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
        ) -> DispatchResult {
            // 支持 Root 和签名用户（所有者）
            let maybe_who = match ensure_root(origin.clone()) {
                Ok(()) => None,
                Err(_) => {
                    let who = frame_system::ensure_signed(origin)?;
                    ensure!(
                        T::DataOwnerProvider::is_owner(&who, &data_type, data_id),
                        Error::<T>::InvalidArchiveState
                    );
                    Some(who)
                }
            };
            let _ = maybe_who; // 仅用于鉴权
            let current_level = DataArchiveStatus::<T>::get(&data_type, data_id);
            ensure!(
                matches!(current_level, ArchiveLevel::ArchivedL1),
                Error::<T>::CannotRestoreFromLevel
            );
            let success = T::StorageArchiver::restore_record(&data_type, data_id, current_level);
            ensure!(success, Error::<T>::RestoreFailed);
            DataArchiveStatus::<T>::insert(&data_type, data_id, ArchiveLevel::Active);
            // M1-R4: 通知下游 pallet 数据已恢复
            T::OnArchive::on_archived(&data_type, data_id, current_level, ArchiveLevel::Active);
            Self::deposit_event(Event::DataRestored {
                data_type,
                data_id,
                from_level: current_level.to_u8(),
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// G1: 获取有效归档配置（运行时覆盖 > genesis 常量）
        pub fn effective_config() -> ArchiveConfig {
            ArchiveConfigOverride::<T>::get().unwrap_or_else(|| ArchiveConfig {
                l1_delay: T::L1ArchiveDelay::get(),
                l2_delay: T::L2ArchiveDelay::get(),
                purge_delay: T::PurgeDelay::get(),
                purge_enabled: T::EnablePurge::get(),
                max_batch_size: T::MaxBatchSize::get(),
            })
        }

        /// G3: 获取有效归档策略（per-type > global config）
        pub fn effective_policy(
            data_type: &BoundedVec<u8, ConstU32<32>>,
            config: &ArchiveConfig,
        ) -> ArchivePolicy {
            ArchivePolicies::<T>::get(data_type).unwrap_or_else(|| ArchivePolicy {
                l1_delay: config.l1_delay,
                l2_delay: config.l2_delay,
                purge_delay: config.purge_delay,
                purge_enabled: config.purge_enabled,
            })
        }

        /// U1: 查询数据归档状态
        pub fn query_data_status(
            data_type: &BoundedVec<u8, ConstU32<32>>,
            data_id: u64,
        ) -> ArchiveLevel {
            DataArchiveStatus::<T>::get(data_type, data_id)
        }

        /// U2: 查询即将被归档的数据数量（在 L1 延迟 80% 窗口内）
        pub fn query_approaching_archival(data_type: &BoundedVec<u8, ConstU32<32>>) -> u32 {
            let config = Self::effective_config();
            let policy = Self::effective_policy(data_type, &config);
            let warning_delay = (policy.l1_delay as u64) * 4 / 5;
            T::StorageArchiver::scan_for_level(
                data_type,
                ArchiveLevel::ArchivedL1,
                warning_delay,
                config.max_batch_size,
            )
            .len() as u32
        }

        /// O2: 获取归档仪表盘摘要
        pub fn get_dashboard(
            data_type: &BoundedVec<u8, ConstU32<32>>,
        ) -> (ArchiveStatistics, bool, ArchiveConfig) {
            let stats = ArchiveStats::<T>::get(data_type);
            let paused = ArchivalPaused::<T>::get();
            let config = Self::effective_config();
            (stats, paused, config)
        }
    }
}

/// 归档统计信息
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
pub struct ArchiveStatistics {
    /// 总归档到L1数量
    pub total_l1_archived: u64,
    /// 总归档到L2数量
    pub total_l2_archived: u64,
    /// 总清除数量
    pub total_purged: u64,
    /// 节省的存储字节数
    pub total_bytes_saved: u64,
    /// 最后归档时间
    pub last_archive_at: u64,
}

/// 存储生命周期管理器
///
/// 提供分级归档的核心逻辑
pub struct StorageLifecycleManager<T: Config> {
    _marker: PhantomData<T>,
}

impl<T: Config> StorageLifecycleManager<T> {
    /// 记录归档批次
    pub fn record_batch(
        data_type: BoundedVec<u8, ConstU32<32>>,
        id_start: u64,
        id_end: u64,
        count: u32,
        level: ArchiveLevel,
        now: u64,
    ) -> Result<(), Error<T>> {
        let batch = ArchiveBatch {
            batch_id: Self::next_batch_id(&data_type),
            id_start,
            id_end,
            count,
            archived_at: now,
            level: level.to_u8(),
        };

        ArchiveBatches::<T>::try_mutate(&data_type, |batches| {
            // LC3修复：队列满时淘汰最旧批次，避免永久失败
            if batches.try_push(batch.clone()).is_err() {
                batches.remove(0);
                batches
                    .try_push(batch)
                    .map_err(|_| Error::<T>::BatchQueueFull)?;
            }
            Ok::<(), Error<T>>(())
        })?;

        // M2-R1: 递增批次计数
        TotalBatchCount::<T>::mutate(&data_type, |count| {
            *count = count.saturating_add(1);
        });

        // 更新统计
        ArchiveStats::<T>::mutate(&data_type, |stats| {
            match level {
                ArchiveLevel::ArchivedL1 => {
                    stats.total_l1_archived = stats.total_l1_archived.saturating_add(count as u64);
                }
                ArchiveLevel::ArchivedL2 => {
                    stats.total_l2_archived = stats.total_l2_archived.saturating_add(count as u64);
                }
                ArchiveLevel::Purged => {
                    stats.total_purged = stats.total_purged.saturating_add(count as u64);
                }
                _ => {}
            }
            stats.last_archive_at = now;
        });

        Ok(())
    }

    /// 获取下一个批次ID
    fn next_batch_id(data_type: &BoundedVec<u8, ConstU32<32>>) -> u64 {
        ArchiveBatches::<T>::get(data_type)
            .last()
            .map(|b| b.batch_id.saturating_add(1))
            .unwrap_or(1)
    }

    /// 更新归档游标
    pub fn update_cursor(data_type: BoundedVec<u8, ConstU32<32>>, cursor: u64) {
        ArchiveCursor::<T>::insert(data_type, cursor);
    }

    /// 获取归档游标
    pub fn get_cursor(data_type: &BoundedVec<u8, ConstU32<32>>) -> u64 {
        ArchiveCursor::<T>::get(data_type)
    }

    /// 更新节省的存储统计
    pub fn record_bytes_saved(data_type: &BoundedVec<u8, ConstU32<32>>, bytes: u64) {
        ArchiveStats::<T>::mutate(data_type, |stats| {
            stats.total_bytes_saved = stats.total_bytes_saved.saturating_add(bytes);
        });
    }
}

/// 辅助函数：将区块号转换为年月格式
pub fn block_to_year_month(block_number: u32, blocks_per_day: u32) -> u16 {
    // LC2修复：防止除零
    if blocks_per_day == 0 {
        return 2401; // 默认返回 2024年1月
    }
    // 假设创世区块是2024年1月
    let days = block_number / blocks_per_day;
    let months = days / 30;
    let year = 24 + (months / 12) as u16;
    let month = (months % 12 + 1) as u16;
    year * 100 + month // YYMM 格式
}

/// 辅助函数：将金额转换为档位
pub fn amount_to_tier(amount: u64) -> u8 {
    match amount {
        0..=99 => 0,          // < 100
        100..=999 => 1,       // 100-999
        1000..=9999 => 2,     // 1K-10K
        10000..=99999 => 3,   // 10K-100K
        100000..=999999 => 4, // 100K-1M
        _ => 5,               // > 1M
    }
}
