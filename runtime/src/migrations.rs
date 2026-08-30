//! Nexus runtime migrations.
//!
//! Nexus runtime 迁移。

pub mod retire_ads;
pub mod retire_grouprobot;
pub mod retire_prediction;
pub mod retire_support;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use codec::Compact;
#[cfg(feature = "try-runtime")]
use codec::{Decode, Encode};
use frame_support::{
    storage::types::StorageValue,
    traits::{OnRuntimeUpgrade, StorageInstance},
    weights::Weight,
};
use pallet_assets::WeightInfo as AssetsWeightInfo;
use pallet_usdx::ProtocolAssetInspector;
use sp_runtime::{traits::AccountIdConversion, MultiAddress};

use crate::{
    configs::ismp::{
        protocol_asset_spec, NexusProtocolAssetInspector, ProtocolAssetsAdminPalletId,
    },
    AccountId, Assets, Runtime, RuntimeOrigin,
};

const MIGRATION_VERSION: u16 = 1;
const PROTOCOL_ASSET_IDS: [u64; 3] = [900_000, 900_001, 900_002];

pub struct ProtocolAssetsMigrationStorage;

impl StorageInstance for ProtocolAssetsMigrationStorage {
    fn pallet_prefix() -> &'static str {
        "NexusRuntimeMigrations"
    }

    const STORAGE_PREFIX: &'static str = "UsdxProtocolAssetsVersion";
}

type ProtocolAssetsMigrationVersion =
    StorageValue<ProtocolAssetsMigrationStorage, u16, frame_support::pallet_prelude::ValueQuery>;

/// Creates the reserved USDX and HFT receipt assets without activating a lane.
/// 创建保留的 USDX 与 HFT receipt 资产，但不激活任何通道。
pub struct InitializeUsdxProtocolAssets;

impl InitializeUsdxProtocolAssets {
    fn inspector_accepts(asset_id: u64) -> bool {
        match asset_id {
            900_000 => NexusProtocolAssetInspector::validate_usdx(
                asset_id,
                &pallet_usdx::Pallet::<Runtime>::psm_account(),
            ),
            900_001 | 900_002 => NexusProtocolAssetInspector::validate_receipt(asset_id),
            _ => false,
        }
    }

    fn preflight() -> Result<(), &'static str> {
        for asset_id in PROTOCOL_ASSET_IDS {
            if let Some(details) = pallet_assets::Asset::<Runtime>::get(asset_id) {
                if details.supply != 0 {
                    return Err("reserved USDX protocol asset already has supply");
                }
                if !Self::inspector_accepts(asset_id) {
                    return Err("reserved USDX protocol asset configuration mismatch");
                }
            }
        }
        Ok(())
    }

    fn create(asset_id: u64) {
        let spec = protocol_asset_spec(asset_id).expect("reserved protocol asset has a fixed spec");
        let admin: AccountId = ProtocolAssetsAdminPalletId::get().into_account_truncating();
        let admin_lookup = MultiAddress::Id(admin.clone());
        let issuer_lookup = MultiAddress::Id(spec.issuer.clone());

        Assets::force_create(
            RuntimeOrigin::root(),
            Compact(spec.asset_id),
            admin_lookup.clone(),
            true,
            1,
        )
        .expect("reserved protocol asset creation must succeed");
        Assets::force_asset_status(
            RuntimeOrigin::root(),
            Compact(spec.asset_id),
            admin_lookup.clone(),
            issuer_lookup,
            admin_lookup.clone(),
            admin_lookup,
            1,
            true,
            false,
        )
        .expect("reserved protocol asset roles must be configured");
        Assets::force_set_metadata(
            RuntimeOrigin::root(),
            Compact(spec.asset_id),
            spec.name.to_vec(),
            spec.symbol.to_vec(),
            6,
            true,
        )
        .expect("reserved protocol asset metadata must be configured");
    }
}

impl OnRuntimeUpgrade for InitializeUsdxProtocolAssets {
    fn on_runtime_upgrade() -> Weight {
        if ProtocolAssetsMigrationVersion::get() >= MIGRATION_VERSION {
            return <Runtime as frame_system::Config>::DbWeight::get().reads(1);
        }

        Self::preflight().expect("USDX protocol asset migration preflight failed");

        let mut weight = <Runtime as frame_system::Config>::DbWeight::get().reads(13);
        for asset_id in PROTOCOL_ASSET_IDS {
            if pallet_assets::Asset::<Runtime>::contains_key(asset_id) {
                continue;
            }
            let spec =
                protocol_asset_spec(asset_id).expect("reserved protocol asset has a fixed spec");
            Self::create(asset_id);
            weight = weight
                .saturating_add(
                    <pallet_assets::weights::SubstrateWeight<Runtime> as AssetsWeightInfo>::force_create(),
                )
                .saturating_add(
                    <pallet_assets::weights::SubstrateWeight<Runtime> as AssetsWeightInfo>::force_asset_status(),
                )
                .saturating_add(
                    <pallet_assets::weights::SubstrateWeight<Runtime> as AssetsWeightInfo>::force_set_metadata(
                        spec.name.len() as u32,
                        spec.symbol.len() as u32,
                    ),
                );
        }
        ProtocolAssetsMigrationVersion::put(MIGRATION_VERSION);
        weight.saturating_add(<Runtime as frame_system::Config>::DbWeight::get().writes(1))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        let version = ProtocolAssetsMigrationVersion::get();
        if version < MIGRATION_VERSION {
            Self::preflight().map_err(sp_runtime::DispatchError::Other)?;
        }
        Ok(version.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let previous = u16::decode(&mut &state[..])
            .map_err(|_| "failed to decode USDX protocol asset migration state")?;
        if previous < MIGRATION_VERSION {
            for asset_id in PROTOCOL_ASSET_IDS {
                let details = pallet_assets::Asset::<Runtime>::get(asset_id)
                    .ok_or("USDX protocol asset was not created")?;
                if details.supply != 0 || !Self::inspector_accepts(asset_id) {
                    return Err("USDX protocol asset migration post-check failed".into());
                }
            }
        }
        if ProtocolAssetsMigrationVersion::get() < MIGRATION_VERSION {
            return Err("USDX protocol asset migration version was not written".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_exact_assets_and_is_idempotent() {
        sp_io::TestExternalities::default().execute_with(|| {
            InitializeUsdxProtocolAssets::on_runtime_upgrade();

            for asset_id in PROTOCOL_ASSET_IDS {
                assert!(InitializeUsdxProtocolAssets::inspector_accepts(asset_id));
                assert_eq!(
                    pallet_assets::Asset::<Runtime>::get(asset_id)
                        .expect("asset exists")
                        .supply,
                    0
                );
            }
            assert_eq!(ProtocolAssetsMigrationVersion::get(), MIGRATION_VERSION);

            let before = pallet_assets::Asset::<Runtime>::get(900_000);
            InitializeUsdxProtocolAssets::on_runtime_upgrade();
            assert_eq!(pallet_assets::Asset::<Runtime>::get(900_000), before);
        });
    }

    #[test]
    fn preflight_rejects_a_squatted_reserved_asset() {
        sp_io::TestExternalities::default().execute_with(|| {
            Assets::force_create(
                RuntimeOrigin::root(),
                Compact(900_000),
                MultiAddress::Id(AccountId::new([7; 32])),
                true,
                1,
            )
            .expect("test asset creation succeeds");

            assert_eq!(
                InitializeUsdxProtocolAssets::preflight(),
                Err("reserved USDX protocol asset configuration mismatch")
            );
        });
    }

    #[test]
    fn inspector_rejects_protocol_role_drift() {
        sp_io::TestExternalities::default().execute_with(|| {
            InitializeUsdxProtocolAssets::on_runtime_upgrade();
            pallet_assets::Asset::<Runtime>::mutate(900_001, |details| {
                details.as_mut().expect("receipt exists").issuer = AccountId::new([9; 32]);
            });

            assert!(!NexusProtocolAssetInspector::validate_receipt(900_001));
            assert_eq!(
                InitializeUsdxProtocolAssets::preflight(),
                Err("reserved USDX protocol asset configuration mismatch")
            );
        });
    }
}
