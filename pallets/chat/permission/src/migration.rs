//! Storage migration v0 → v1 for `pallet-chat-permission`.
//! `pallet-chat-permission` 存储迁移 v0 → v1。
//!
//! EN: Rewrites `PrivacySettingsOf`, then clears legacy friend-graph maps in bounded
//! batches across multiple `on_runtime_upgrade` invocations.
//! CN: 重写 `PrivacySettingsOf`，再分多批清除旧好友图谱存储（可跨多次升级调用）。

use crate::{pallet::*, types::PrivacySettings, ChatPermissionLevel, SceneType};
use frame_support::{
    pallet_prelude::*,
    storage_alias,
    traits::GetStorageVersion,
    weights::Weight,
    BoundedVec,
};
use frame_system::pallet_prelude::BlockNumberFor;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// EN: Max legacy rows cleared per map per upgrade call. CN: 每次升级单表最多清理行数。
const LEGACY_CLEAR_BATCH: u32 = 500;

/// v0 privacy settings (included on-chain block/whitelist lists).
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct OldPrivacySettings<T: Config> {
    pub permission_level: ChatPermissionLevel,
    pub block_list: BoundedVec<AccountIdOf<T>, ConstU32<256>>,
    pub whitelist: BoundedVec<AccountIdOf<T>, ConstU32<256>>,
    pub rejected_scene_types: BoundedVec<SceneType, ConstU32<10>>,
    pub updated_at: BlockNumberFor<T>,
}

/// EN: Legacy friend-graph clearing phase. CN: 旧好友图谱清理阶段。
#[derive(
    Encode, Decode, Clone, Copy, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen, Default,
)]
pub enum LegacyMigrationPhase {
    #[default]
    Privacy,
    Friendships,
    FriendRequests,
    IncomingFriendRequestCount,
    FriendRequestMsg,
    FriendRemark,
    FriendGroupTag,
    Done,
}

#[storage_alias]
type LegacyMigrationPhaseStore<T: Config> =
    StorageValue<Pallet<T>, LegacyMigrationPhase, ValueQuery>;

#[storage_alias]
type Friendships<T: Config> = StorageDoubleMap<
    Pallet<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    BlockNumberFor<T>,
    OptionQuery,
>;

#[storage_alias]
type FriendRequests<T: Config> = StorageDoubleMap<
    Pallet<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    BlockNumberFor<T>,
    OptionQuery,
>;

#[storage_alias]
type IncomingFriendRequestCount<T: Config> =
    StorageMap<Pallet<T>, Blake2_128Concat, AccountIdOf<T>, u32, ValueQuery>;

#[storage_alias]
type FriendRequestMsg<T: Config> = StorageDoubleMap<
    Pallet<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    BoundedVec<u8, ConstU32<256>>,
    OptionQuery,
>;

#[storage_alias]
type FriendRemark<T: Config> = StorageDoubleMap<
    Pallet<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    BoundedVec<u8, ConstU32<64>>,
    OptionQuery,
>;

#[storage_alias]
type FriendGroupTag<T: Config> = StorageDoubleMap<
    Pallet<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    Blake2_128Concat,
    AccountIdOf<T>,
    BoundedVec<u8, ConstU32<64>>,
    OptionQuery,
>;

fn migrate_privacy<T: Config>() -> Weight {
    let mut migrated = 0u64;
    let _ = PrivacySettingsOf::<T>::translate::<OldPrivacySettings<T>, _>(|_, old| {
        migrated = migrated.saturating_add(1);
        Some(PrivacySettings {
            permission_level: old.permission_level,
            rejected_scene_types: old.rejected_scene_types,
            updated_at: old.updated_at,
        })
    });
    T::DbWeight::get().reads_writes(migrated, migrated)
}

fn clear_legacy_map<T: Config>(phase: LegacyMigrationPhase) -> (Weight, bool) {
    let result = match phase {
        LegacyMigrationPhase::Friendships => Friendships::<T>::clear(LEGACY_CLEAR_BATCH, None),
        LegacyMigrationPhase::FriendRequests => {
            FriendRequests::<T>::clear(LEGACY_CLEAR_BATCH, None)
        }
        LegacyMigrationPhase::IncomingFriendRequestCount => {
            IncomingFriendRequestCount::<T>::clear(LEGACY_CLEAR_BATCH, None)
        }
        LegacyMigrationPhase::FriendRequestMsg => {
            FriendRequestMsg::<T>::clear(LEGACY_CLEAR_BATCH, None)
        }
        LegacyMigrationPhase::FriendRemark => FriendRemark::<T>::clear(LEGACY_CLEAR_BATCH, None),
        LegacyMigrationPhase::FriendGroupTag => {
            FriendGroupTag::<T>::clear(LEGACY_CLEAR_BATCH, None)
        }
        _ => return (Weight::zero(), true),
    };
    let weight = T::DbWeight::get().writes(result.backend as u64);
    (weight, result.maybe_cursor.is_none())
}

fn next_phase(phase: LegacyMigrationPhase) -> LegacyMigrationPhase {
    match phase {
        LegacyMigrationPhase::Privacy => LegacyMigrationPhase::Friendships,
        LegacyMigrationPhase::Friendships => LegacyMigrationPhase::FriendRequests,
        LegacyMigrationPhase::FriendRequests => LegacyMigrationPhase::IncomingFriendRequestCount,
        LegacyMigrationPhase::IncomingFriendRequestCount => LegacyMigrationPhase::FriendRequestMsg,
        LegacyMigrationPhase::FriendRequestMsg => LegacyMigrationPhase::FriendRemark,
        LegacyMigrationPhase::FriendRemark => LegacyMigrationPhase::FriendGroupTag,
        LegacyMigrationPhase::FriendGroupTag | LegacyMigrationPhase::Done => {
            LegacyMigrationPhase::Done
        }
    }
}

/// EN: Migrate legacy permission storage to v1 (resumable). CN: 将旧版权限存储迁移至 v1（可续跑）。
pub fn migrate_v0_to_v1<T: Config>() -> Weight {
    let on_chain = Pallet::<T>::on_chain_storage_version();
    if on_chain >= StorageVersion::new(1) {
        return Weight::zero();
    }

    let mut weight = T::DbWeight::get().reads(1);
    let mut phase = LegacyMigrationPhaseStore::<T>::get();

    if phase == LegacyMigrationPhase::Privacy {
        weight = weight.saturating_add(migrate_privacy::<T>());
        phase = LegacyMigrationPhase::Friendships;
        LegacyMigrationPhaseStore::<T>::put(phase);
    }

    while phase != LegacyMigrationPhase::Done {
        let (w, done) = clear_legacy_map::<T>(phase);
        weight = weight.saturating_add(w);
        if done {
            phase = next_phase(phase);
            LegacyMigrationPhaseStore::<T>::put(phase);
        } else {
            break;
        }
    }

    if phase == LegacyMigrationPhase::Done {
        LegacyMigrationPhaseStore::<T>::kill();
        StorageVersion::new(1).put::<Pallet<T>>();
        weight = weight.saturating_add(T::DbWeight::get().writes(1));
    }

    weight
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{mock::*, ChatPermissionLevel, PrivacySettingsOf};
    use frame_support::traits::GetStorageVersion;

    fn seed_v0_privacy(who: u64) {
        let old = OldPrivacySettings::<Test> {
            permission_level: ChatPermissionLevel::Open,
            block_list: Default::default(),
            whitelist: Default::default(),
            rejected_scene_types: Default::default(),
            updated_at: 1,
        };
        let key = PrivacySettingsOf::<Test>::hashed_key_for(who);
        sp_io::storage::set(&key, &old.encode());
    }

    #[test]
    fn migrate_v0_privacy_rewrites_settings() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(0).put::<Pallet<Test>>();
            seed_v0_privacy(ALICE);
            assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(0));

            let w = migrate_v0_to_v1::<Test>();
            let _ = w;

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert_eq!(settings.permission_level, ChatPermissionLevel::Open);
            assert_eq!(settings.updated_at, 1);

            assert_eq!(Pallet::<Test>::on_chain_storage_version(), StorageVersion::new(1));
        });
    }

    #[test]
    fn migrate_is_idempotent_after_v1() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<Pallet<Test>>();
            let w = migrate_v0_to_v1::<Test>();
            assert_eq!(w, Weight::zero());
        });
    }
}
