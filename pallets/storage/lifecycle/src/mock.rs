use crate as pallet_storage_lifecycle;
use crate::ArchiveLevel;
use frame_support::{
    derive_impl, parameter_types,
    traits::{EitherOfDiverse, SortedMembers},
};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        StorageLifecycle: pallet_storage_lifecycle,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
}

parameter_types! {
    pub const L1ArchiveDelay: u32 = 100;
    pub const L2ArchiveDelay: u32 = 200;
    pub const PurgeDelay: u32 = 300;
    pub const EnablePurge: bool = true;
    pub const MaxBatchSize: u32 = 10;
    pub const GovernanceAccount: u64 = 99;
}

use std::cell::RefCell;

thread_local! {
    // 按级别分离的可归档ID列表
    static ACTIVE_IDS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
    static L1_IDS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
    static L2_IDS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
    // 兼容旧 API
    static ARCHIVABLE_IDS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
    static ARCHIVED_IDS: RefCell<Vec<u64>> = RefCell::new(Vec::new());
    // 回调跟踪
    static ARCHIVE_CALLBACKS: RefCell<Vec<(Vec<u8>, u64, u8, u8)>> = RefCell::new(Vec::new());
    // 恢复支持
    static RESTORE_ALLOWED: RefCell<bool> = RefCell::new(true);
}

pub fn set_archivable_ids(ids: Vec<u64>) {
    ARCHIVABLE_IDS.with(|a| *a.borrow_mut() = ids.clone());
    ACTIVE_IDS.with(|a| *a.borrow_mut() = ids);
}

pub fn set_l1_ids(ids: Vec<u64>) {
    L1_IDS.with(|a| *a.borrow_mut() = ids);
}

pub fn set_l2_ids(ids: Vec<u64>) {
    L2_IDS.with(|a| *a.borrow_mut() = ids);
}

pub fn get_archived_ids() -> Vec<u64> {
    ARCHIVED_IDS.with(|a| a.borrow().clone())
}

pub fn get_archive_callbacks() -> Vec<(Vec<u8>, u64, u8, u8)> {
    ARCHIVE_CALLBACKS.with(|a| a.borrow().clone())
}

pub fn clear_archive_callbacks() {
    ARCHIVE_CALLBACKS.with(|a| a.borrow_mut().clear());
}

pub fn set_restore_allowed(allowed: bool) {
    RESTORE_ALLOWED.with(|a| *a.borrow_mut() = allowed);
}

pub struct MockStorageArchiver;

impl pallet_storage_lifecycle::StorageArchiver for MockStorageArchiver {
    fn scan_archivable(_delay: u64, max_count: u32) -> Vec<u64> {
        ARCHIVABLE_IDS.with(|a| {
            let ids = a.borrow();
            ids.iter().take(max_count as usize).copied().collect()
        })
    }

    fn archive_records(ids: &[u64]) {
        ARCHIVED_IDS.with(|a| a.borrow_mut().extend_from_slice(ids));
        ARCHIVABLE_IDS.with(|a| {
            let mut v = a.borrow_mut();
            v.retain(|id| !ids.contains(id));
        });
    }

    fn scan_for_level(
        _data_type: &[u8],
        target_level: ArchiveLevel,
        _delay: u64,
        max_count: u32,
    ) -> Vec<u64> {
        match target_level {
            ArchiveLevel::ArchivedL1 => ACTIVE_IDS.with(|a| {
                a.borrow()
                    .iter()
                    .take(max_count as usize)
                    .copied()
                    .collect()
            }),
            ArchiveLevel::ArchivedL2 => L1_IDS.with(|a| {
                a.borrow()
                    .iter()
                    .take(max_count as usize)
                    .copied()
                    .collect()
            }),
            ArchiveLevel::Purged => L2_IDS.with(|a| {
                a.borrow()
                    .iter()
                    .take(max_count as usize)
                    .copied()
                    .collect()
            }),
            _ => Vec::new(),
        }
    }

    fn archive_to_level(_data_type: &[u8], ids: &[u64], target_level: ArchiveLevel) {
        match target_level {
            ArchiveLevel::ArchivedL1 => {
                ACTIVE_IDS.with(|a| {
                    let mut v = a.borrow_mut();
                    v.retain(|id| !ids.contains(id));
                });
                L1_IDS.with(|a| a.borrow_mut().extend_from_slice(ids));
            }
            ArchiveLevel::ArchivedL2 => {
                L1_IDS.with(|a| {
                    let mut v = a.borrow_mut();
                    v.retain(|id| !ids.contains(id));
                });
                L2_IDS.with(|a| a.borrow_mut().extend_from_slice(ids));
            }
            ArchiveLevel::Purged => {
                L2_IDS.with(|a| {
                    let mut v = a.borrow_mut();
                    v.retain(|id| !ids.contains(id));
                });
                ARCHIVED_IDS.with(|a| a.borrow_mut().extend_from_slice(ids));
            }
            _ => {}
        }
    }

    fn registered_data_types() -> Vec<Vec<u8>> {
        vec![b"pin_storage".to_vec()]
    }

    fn query_archive_level(_data_type: &[u8], data_id: u64) -> ArchiveLevel {
        if ACTIVE_IDS.with(|a| a.borrow().contains(&data_id)) {
            ArchiveLevel::Active
        } else if L1_IDS.with(|a| a.borrow().contains(&data_id)) {
            ArchiveLevel::ArchivedL1
        } else if L2_IDS.with(|a| a.borrow().contains(&data_id)) {
            ArchiveLevel::ArchivedL2
        } else {
            ArchiveLevel::Purged
        }
    }

    fn restore_record(_data_type: &[u8], data_id: u64, from_level: ArchiveLevel) -> bool {
        if !RESTORE_ALLOWED.with(|a| *a.borrow()) {
            return false;
        }
        match from_level {
            ArchiveLevel::ArchivedL1 => {
                L1_IDS.with(|a| {
                    let mut v = a.borrow_mut();
                    v.retain(|id| *id != data_id);
                });
                ACTIVE_IDS.with(|a| a.borrow_mut().push(data_id));
                true
            }
            _ => false,
        }
    }
}

pub struct MockOnArchiveHandler;

impl pallet_storage_lifecycle::OnArchiveHandler for MockOnArchiveHandler {
    fn on_archived(
        data_type: &[u8],
        data_id: u64,
        from_level: ArchiveLevel,
        to_level: ArchiveLevel,
    ) {
        ARCHIVE_CALLBACKS.with(|a| {
            a.borrow_mut().push((
                data_type.to_vec(),
                data_id,
                from_level.to_u8(),
                to_level.to_u8(),
            ));
        });
    }
}

pub struct GovernanceMembers;
impl SortedMembers<u64> for GovernanceMembers {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

impl pallet_storage_lifecycle::pallet::Config for Test {
    type L1ArchiveDelay = L1ArchiveDelay;
    type L2ArchiveDelay = L2ArchiveDelay;
    type PurgeDelay = PurgeDelay;
    type EnablePurge = EnablePurge;
    type MaxBatchSize = MaxBatchSize;
    type StorageArchiver = MockStorageArchiver;
    type OnArchive = MockOnArchiveHandler;
    type DataOwnerProvider = ();
    type GovernanceOrigin = EitherOfDiverse<
        frame_system::EnsureRoot<u64>,
        frame_system::EnsureSignedBy<GovernanceMembers, u64>,
    >;
    type WeightInfo = pallet_storage_lifecycle::SubstrateWeight<Test>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    ARCHIVABLE_IDS.with(|a| a.borrow_mut().clear());
    ARCHIVED_IDS.with(|a| a.borrow_mut().clear());
    ACTIVE_IDS.with(|a| a.borrow_mut().clear());
    L1_IDS.with(|a| a.borrow_mut().clear());
    L2_IDS.with(|a| a.borrow_mut().clear());
    ARCHIVE_CALLBACKS.with(|a| a.borrow_mut().clear());
    RESTORE_ALLOWED.with(|a| *a.borrow_mut() = true);
    let t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
