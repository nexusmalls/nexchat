//! Mainnet retirement of the prediction-market pallet namespace.
//! 主网退役预测市场 pallet 命名空间。
//!
//! Phase 6 wiring stayed globally Disabled. `RemovePallet` is allowed only after
//! `AssertPredictionNamespaceIdle` proves every prefix is empty or holds only the
//! FRAME `StorageVersion` key. Any other key (including orml-tokens balances)
//! aborts the upgrade so user funds cannot be wiped.
//! Phase 6 接线保持全局 Disabled。仅当 `AssertPredictionNamespaceIdle` 证明每个前缀
//! 为空或只含 FRAME `StorageVersion` 键时，才允许 `RemovePallet`。任何其它键
//! （含 orml-tokens 余额）都会中止升级，避免销毁用户资金。
//!
//! Indexes 176–193 stay retired and must not be reused.
//! 索引 176–193 永久退役，禁止复用。

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use codec::Decode;
#[cfg(any(test, feature = "try-runtime"))]
use codec::Encode;
use frame_support::{
    pallet_prelude::ValueQuery,
    parameter_types,
    traits::{OnRuntimeUpgrade, StorageInstance},
    weights::{constants::RocksDbWeight, Weight},
};

use super::retire_support;

/// On-chain construct_runtime names; must match `RemovePallet` prefixes.
/// 链上 construct_runtime 名称，必须与 `RemovePallet` 前缀一致。
pub const PALLET_NAMES: [&str; 18] = [
    "PredictionControl",
    "PredictionCollateral",
    "PredictionCurrencies",
    "PredictionTokens",
    "PredictionMarketCommons",
    "PredictionAuthorized",
    "PredictionCourt",
    "PredictionGlobalDisputes",
    "PredictionMarkets",
    "PredictionLegacySwaps",
    "PredictionNeoSwaps",
    "PredictionOrderbook",
    "PredictionParimutuel",
    "PredictionHybridRouter",
    "PredictionCombinatorialTokens",
    "PredictionFutarchy",
    "PredictionStyx",
    "PredictionCommunityCore",
];

parameter_types! {
    pub const PredictionControlName: &'static str = "PredictionControl";
    pub const PredictionCollateralName: &'static str = "PredictionCollateral";
    pub const PredictionCurrenciesName: &'static str = "PredictionCurrencies";
    pub const PredictionTokensName: &'static str = "PredictionTokens";
    pub const PredictionMarketCommonsName: &'static str = "PredictionMarketCommons";
    pub const PredictionAuthorizedName: &'static str = "PredictionAuthorized";
    pub const PredictionCourtName: &'static str = "PredictionCourt";
    pub const PredictionGlobalDisputesName: &'static str = "PredictionGlobalDisputes";
    pub const PredictionMarketsName: &'static str = "PredictionMarkets";
    pub const PredictionLegacySwapsName: &'static str = "PredictionLegacySwaps";
    pub const PredictionNeoSwapsName: &'static str = "PredictionNeoSwaps";
    pub const PredictionOrderbookName: &'static str = "PredictionOrderbook";
    pub const PredictionParimutuelName: &'static str = "PredictionParimutuel";
    pub const PredictionHybridRouterName: &'static str = "PredictionHybridRouter";
    pub const PredictionCombinatorialTokensName: &'static str = "PredictionCombinatorialTokens";
    pub const PredictionFutarchyName: &'static str = "PredictionFutarchy";
    pub const PredictionStyxName: &'static str = "PredictionStyx";
    pub const PredictionCommunityCoreName: &'static str = "PredictionCommunityCore";
}

pub type RemovePredictionControl =
    frame_support::migrations::RemovePallet<PredictionControlName, RocksDbWeight>;
pub type RemovePredictionCollateral =
    frame_support::migrations::RemovePallet<PredictionCollateralName, RocksDbWeight>;
pub type RemovePredictionCurrencies =
    frame_support::migrations::RemovePallet<PredictionCurrenciesName, RocksDbWeight>;
pub type RemovePredictionTokens =
    frame_support::migrations::RemovePallet<PredictionTokensName, RocksDbWeight>;
pub type RemovePredictionMarketCommons =
    frame_support::migrations::RemovePallet<PredictionMarketCommonsName, RocksDbWeight>;
pub type RemovePredictionAuthorized =
    frame_support::migrations::RemovePallet<PredictionAuthorizedName, RocksDbWeight>;
pub type RemovePredictionCourt =
    frame_support::migrations::RemovePallet<PredictionCourtName, RocksDbWeight>;
pub type RemovePredictionGlobalDisputes =
    frame_support::migrations::RemovePallet<PredictionGlobalDisputesName, RocksDbWeight>;
pub type RemovePredictionMarkets =
    frame_support::migrations::RemovePallet<PredictionMarketsName, RocksDbWeight>;
pub type RemovePredictionLegacySwaps =
    frame_support::migrations::RemovePallet<PredictionLegacySwapsName, RocksDbWeight>;
pub type RemovePredictionNeoSwaps =
    frame_support::migrations::RemovePallet<PredictionNeoSwapsName, RocksDbWeight>;
pub type RemovePredictionOrderbook =
    frame_support::migrations::RemovePallet<PredictionOrderbookName, RocksDbWeight>;
pub type RemovePredictionParimutuel =
    frame_support::migrations::RemovePallet<PredictionParimutuelName, RocksDbWeight>;
pub type RemovePredictionHybridRouter =
    frame_support::migrations::RemovePallet<PredictionHybridRouterName, RocksDbWeight>;
pub type RemovePredictionCombinatorialTokens =
    frame_support::migrations::RemovePallet<PredictionCombinatorialTokensName, RocksDbWeight>;
pub type RemovePredictionFutarchy =
    frame_support::migrations::RemovePallet<PredictionFutarchyName, RocksDbWeight>;
pub type RemovePredictionStyx =
    frame_support::migrations::RemovePallet<PredictionStyxName, RocksDbWeight>;
pub type RemovePredictionCommunityCore =
    frame_support::migrations::RemovePallet<PredictionCommunityCoreName, RocksDbWeight>;

type RemovePredictionInner = (
    RemovePredictionControl,
    RemovePredictionCollateral,
    RemovePredictionCurrencies,
    RemovePredictionTokens,
    RemovePredictionMarketCommons,
    RemovePredictionAuthorized,
    RemovePredictionCourt,
    RemovePredictionGlobalDisputes,
    RemovePredictionMarkets,
    RemovePredictionLegacySwaps,
    RemovePredictionNeoSwaps,
    RemovePredictionOrderbook,
    RemovePredictionParimutuel,
    RemovePredictionHybridRouter,
    RemovePredictionCombinatorialTokens,
    RemovePredictionFutarchy,
    RemovePredictionStyx,
    RemovePredictionCommunityCore,
);

const RETIRED_VERSION: u16 = 1;

struct RetiredVersionStorage;
impl StorageInstance for RetiredVersionStorage {
    fn pallet_prefix() -> &'static str {
        "NexusRuntimeMigrations"
    }
    const STORAGE_PREFIX: &'static str = "PredictionIdleVersion";
}
type PredictionIdleVersion =
    frame_support::storage::types::StorageValue<RetiredVersionStorage, u16, ValueQuery>;

/// Names used by prefix wipe / weight estimates.
/// 供前缀清除与重量估算使用的名称。
#[cfg(any(test, feature = "try-runtime"))]
pub fn pallet_names() -> [&'static str; 18] {
    PALLET_NAMES
}

/// True after the namespace was proven idle (empty or StorageVersion-only).
/// 命名空间被证明空闲（空或仅 StorageVersion）后为 true。
pub fn idle_complete() -> bool {
    PredictionIdleVersion::get() >= RETIRED_VERSION
}

/// Estimated weight of walking every prediction prefix.
/// 遍历全部 prediction 前缀的估算重量。
#[cfg(any(test, feature = "try-runtime"))]
pub fn estimated_idle_check_weight() -> Weight {
    if idle_complete() {
        return RocksDbWeight::get().reads(1);
    }
    let keys = PALLET_NAMES
        .iter()
        .map(|name| retire_support::count_keys_with_prefix(&retire_support::pallet_prefix(name)))
        .fold(0u64, |acc, n| acc.saturating_add(n));
    RocksDbWeight::get().reads(keys.saturating_add(PALLET_NAMES.len() as u64))
}

fn assert_idle() -> Result<Weight, &'static str> {
    let mut weight = RocksDbWeight::get().reads(1);
    if idle_complete() {
        return Ok(weight);
    }

    for name in PALLET_NAMES {
        let extra = retire_support::count_non_version_keys(name);
        let keys = retire_support::count_keys_with_prefix(&retire_support::pallet_prefix(name));
        weight = weight.saturating_add(RocksDbWeight::get().reads(keys.saturating_add(1)));
        if extra > 0 {
            log::error!(
                target: "runtime::retire_prediction",
                "{name} has {extra} non-StorageVersion keys; refusing RemovePallet"
            );
            return Err(
                "prediction prefix holds user data; refusing RemovePallet so balances cannot be wiped",
            );
        }
    }

    PredictionIdleVersion::put(RETIRED_VERSION);
    Ok(weight.saturating_add(RocksDbWeight::get().writes(1)))
}

/// Aborts the upgrade unless every prediction prefix is empty or StorageVersion-only.
/// 除非每个 prediction 前缀为空或仅含 StorageVersion，否则中止升级。
pub struct AssertPredictionNamespaceIdle;

impl OnRuntimeUpgrade for AssertPredictionNamespaceIdle {
    fn on_runtime_upgrade() -> Weight {
        assert_idle().unwrap_or_else(|err| retire_support::panic_refund("prediction", err))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if idle_complete() {
            return Ok(PredictionIdleVersion::get().encode());
        }
        for name in PALLET_NAMES {
            if retire_support::count_non_version_keys(name) > 0 {
                return Err(
                    "prediction prefix holds user data; refusing RemovePallet so balances cannot be wiped"
                        .into(),
                );
            }
        }
        Ok(PredictionIdleVersion::get().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let previous = u16::decode(&mut &state[..])
            .map_err(|_| "failed to decode prediction idle migration state")?;
        if previous < RETIRED_VERSION && !idle_complete() {
            return Err("prediction idle version was not written".into());
        }
        for name in PALLET_NAMES {
            if retire_support::count_non_version_keys(name) > 0 {
                return Err("prediction user-data keys remain after idle assertion".into());
            }
        }
        Ok(())
    }
}

/// Wipes prediction prefixes only after the idle assertion succeeded.
/// 仅在空闲断言成功后清除 prediction 前缀。
pub struct RemovePredictionAfterIdle;

impl OnRuntimeUpgrade for RemovePredictionAfterIdle {
    fn on_runtime_upgrade() -> Weight {
        if !idle_complete() {
            retire_support::panic_blocked_wipe("prediction");
        }
        <RemovePredictionInner as OnRuntimeUpgrade>::on_runtime_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if !idle_complete() {
            return Err("prediction idle assertion must complete before RemovePallet".into());
        }
        <RemovePredictionInner as OnRuntimeUpgrade>::pre_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        <RemovePredictionInner as OnRuntimeUpgrade>::post_upgrade(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pallet_names_match_remove_prefixes() {
        assert_eq!(PredictionControlName::get(), PALLET_NAMES[0]);
        assert_eq!(PredictionTokensName::get(), PALLET_NAMES[3]);
        assert_eq!(PredictionCommunityCoreName::get(), PALLET_NAMES[17]);
        assert_eq!(PALLET_NAMES.len(), 18);
    }

    #[test]
    fn idle_assertion_allows_empty_or_storage_version_only() {
        sp_io::TestExternalities::default().execute_with(|| {
            assert_idle().expect("empty prefixes are idle");
            assert!(idle_complete());

            PredictionIdleVersion::kill();
            sp_io::storage::set(
                &retire_support::storage_version_key("PredictionTokens"),
                &1u16.encode(),
            );
            assert_idle().expect("StorageVersion-only is idle");
            assert!(idle_complete());
        });
    }

    #[test]
    fn idle_assertion_rejects_user_data_and_does_not_write_version() {
        sp_io::TestExternalities::default().execute_with(|| {
            let mut extra = retire_support::pallet_prefix("PredictionTokens").to_vec();
            extra.extend_from_slice(&[7u8; 16]);
            sp_io::storage::set(&extra, &[1]);

            assert_eq!(
                assert_idle(),
                Err(
                    "prediction prefix holds user data; refusing RemovePallet so balances cannot be wiped"
                )
            );
            assert!(!idle_complete());
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn try_runtime_hooks_reject_user_data() {
        sp_io::TestExternalities::default().execute_with(|| {
            let mut extra = retire_support::pallet_prefix("PredictionTokens").to_vec();
            extra.extend_from_slice(&[7u8; 16]);
            sp_io::storage::set(&extra, &[1]);
            assert!(AssertPredictionNamespaceIdle::pre_upgrade().is_err());
        });
    }
}
