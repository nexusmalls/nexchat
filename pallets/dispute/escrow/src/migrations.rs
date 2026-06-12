//! Storage migrations for pallet-dispute-escrow
//!
//! v2: 移除 LockNonces StorageMap（lock_with_nonce extrinsic 已删除）

use frame_support::{
    pallet_prelude::*, storage::storage_prefix, traits::OnRuntimeUpgrade, weights::Weight,
};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// v1 → v2: 清理 LockNonces 存储
pub struct V2RemoveLockNonces<T>(core::marker::PhantomData<T>);

impl<T: crate::Config> OnRuntimeUpgrade for V2RemoveLockNonces<T> {
    fn on_runtime_upgrade() -> Weight {
        let on_chain = StorageVersion::get::<crate::Pallet<T>>();
        if on_chain < 2 {
            // 清除 LockNonces 前缀下的所有 key
            // pallet name = "Escrow" (由 construct_runtime 决定), storage name = "LockNonces"
            let pallet_prefix = <crate::Pallet<T> as PalletInfoAccess>::name().as_bytes();
            let prefix = storage_prefix(pallet_prefix, b"LockNonces");
            let removed = frame_support::storage::unhashed::clear_prefix(&prefix, None, None);
            log::info!(
                target: "escrow::migration",
                "v2: removed {} LockNonces entries (loops={})",
                removed.unique, removed.loops,
            );
            StorageVersion::new(2).put::<crate::Pallet<T>>();
            // 每个 key 清理约 1 read + 1 write，加上基础开销
            T::DbWeight::get().reads_writes(removed.unique as u64 + 1, removed.unique as u64 + 1)
        } else {
            log::info!(target: "escrow::migration", "v2: skipped (on_chain={:?})", on_chain);
            T::DbWeight::get().reads(1)
        }
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let on_chain = StorageVersion::get::<crate::Pallet<T>>();
        log::info!(target: "escrow::migration", "v2 pre_upgrade: on_chain_version={:?}", on_chain);
        Ok(Vec::new())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let on_chain = StorageVersion::get::<crate::Pallet<T>>();
        assert_eq!(
            on_chain,
            StorageVersion::new(2),
            "v2 migration did not complete"
        );
        // 验证 LockNonces 前缀已清空
        let pallet_prefix = <crate::Pallet<T> as PalletInfoAccess>::name().as_bytes();
        let prefix = storage_prefix(pallet_prefix, b"LockNonces");
        let remaining = frame_support::storage::unhashed::clear_prefix(&prefix, Some(1), None);
        assert_eq!(
            remaining.unique, 0,
            "LockNonces still has entries after migration"
        );
        Ok(())
    }
}
