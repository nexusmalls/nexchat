//! Mainnet retirement of AdsCore, AdsGroupRobot, and AdsEntity.
//! 主网退役 AdsCore、AdsGroupRobot 与 AdsEntity。
//!
//! Refunds reserved campaign escrow / placement deposits / community ad-stake
//! and pays out claimable revenue from treasury using on-chain pallet prefixes.
//! Leftover prefix keys are then wiped by `RemovePallet`.
//! 按链上 pallet 前缀退还 Campaign escrow / 广告位押金 / 社区广告质押，
//! 并从国库发放待领收入；剩余键由 `RemovePallet` 清除。
//!
//! Indexes 160–162 stay retired and must not be reused.
//! 索引 160–162 永久退役，禁止复用。

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::ValueQuery,
    parameter_types,
    storage::types::{StorageDoubleMap, StorageMap},
    traits::{
        Currency, ExistenceRequirement, OnRuntimeUpgrade, ReservableCurrency, StorageInstance,
    },
    weights::{constants::RocksDbWeight, Weight},
    Blake2_128Concat,
};
use sp_runtime::traits::Zero;

use crate::{configs::TreasuryAccountId, AccountId, Balance, Balances, BlockNumber};

parameter_types! {
    pub const AdsCoreName: &'static str = "AdsCore";
    pub const AdsGroupRobotName: &'static str = "AdsGroupRobot";
    pub const AdsEntityName: &'static str = "AdsEntity";
}

pub type RemoveAdsCore = frame_support::migrations::RemovePallet<AdsCoreName, RocksDbWeight>;
pub type RemoveAdsGroupRobot =
    frame_support::migrations::RemovePallet<AdsGroupRobotName, RocksDbWeight>;
pub type RemoveAdsEntity = frame_support::migrations::RemovePallet<AdsEntityName, RocksDbWeight>;

const RETIRED_VERSION: u16 = 1;

struct RetiredVersionStorage;
impl StorageInstance for RetiredVersionStorage {
    fn pallet_prefix() -> &'static str {
        "NexusRuntimeMigrations"
    }
    const STORAGE_PREFIX: &'static str = "AdsRetiredVersion";
}
type AdsRetiredVersion =
    frame_support::storage::types::StorageValue<RetiredVersionStorage, u16, ValueQuery>;

struct CampaignsPrefix;
impl StorageInstance for CampaignsPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsCore"
    }
    const STORAGE_PREFIX: &'static str = "Campaigns";
}
type CampaignsMap = StorageMap<CampaignsPrefix, Blake2_128Concat, u64, Vec<u8>>;

struct CampaignEscrowPrefix;
impl StorageInstance for CampaignEscrowPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsCore"
    }
    const STORAGE_PREFIX: &'static str = "CampaignEscrow";
}
type CampaignEscrowMap =
    StorageMap<CampaignEscrowPrefix, Blake2_128Concat, u64, Balance, ValueQuery>;

struct PlacementClaimablePrefix;
impl StorageInstance for PlacementClaimablePrefix {
    fn pallet_prefix() -> &'static str {
        "AdsCore"
    }
    const STORAGE_PREFIX: &'static str = "PlacementClaimable";
}
type PlacementClaimableMap =
    StorageMap<PlacementClaimablePrefix, Blake2_128Concat, [u8; 32], Balance, ValueQuery>;

struct ReferrerClaimablePrefix;
impl StorageInstance for ReferrerClaimablePrefix {
    fn pallet_prefix() -> &'static str {
        "AdsCore"
    }
    const STORAGE_PREFIX: &'static str = "ReferrerClaimable";
}
type ReferrerClaimableMap =
    StorageMap<ReferrerClaimablePrefix, Blake2_128Concat, AccountId, Balance, ValueQuery>;

struct RegisteredPlacementsPrefix;
impl StorageInstance for RegisteredPlacementsPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsEntity"
    }
    const STORAGE_PREFIX: &'static str = "RegisteredPlacements";
}
type RegisteredPlacementsMap =
    StorageMap<RegisteredPlacementsPrefix, Blake2_128Concat, [u8; 32], Vec<u8>>;

struct PlacementDepositsPrefix;
impl StorageInstance for PlacementDepositsPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsEntity"
    }
    const STORAGE_PREFIX: &'static str = "PlacementDeposits";
}
type PlacementDepositsMap =
    StorageMap<PlacementDepositsPrefix, Blake2_128Concat, [u8; 32], Balance, ValueQuery>;

struct CommunityStakersPrefix;
impl StorageInstance for CommunityStakersPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsGroupRobot"
    }
    const STORAGE_PREFIX: &'static str = "CommunityStakers";
}
type CommunityStakersMap = StorageDoubleMap<
    CommunityStakersPrefix,
    Blake2_128Concat,
    [u8; 32],
    Blake2_128Concat,
    AccountId,
    Balance,
    ValueQuery,
>;

struct UnbondingRequestsPrefix;
impl StorageInstance for UnbondingRequestsPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsGroupRobot"
    }
    const STORAGE_PREFIX: &'static str = "UnbondingRequests";
}
type UnbondingRequestsMap = StorageDoubleMap<
    UnbondingRequestsPrefix,
    Blake2_128Concat,
    [u8; 32],
    Blake2_128Concat,
    AccountId,
    Vec<(Balance, BlockNumber)>,
    ValueQuery,
>;

struct CommunityAdminPrefix;
impl StorageInstance for CommunityAdminPrefix {
    fn pallet_prefix() -> &'static str {
        "AdsGroupRobot"
    }
    const STORAGE_PREFIX: &'static str = "CommunityAdmin";
}
type CommunityAdminMap = StorageMap<CommunityAdminPrefix, Blake2_128Concat, [u8; 32], AccountId>;

struct StakerClaimablePrefix;
impl StorageInstance for StakerClaimablePrefix {
    fn pallet_prefix() -> &'static str {
        "AdsGroupRobot"
    }
    const STORAGE_PREFIX: &'static str = "StakerClaimable";
}
type StakerClaimableMap = StorageDoubleMap<
    StakerClaimablePrefix,
    Blake2_128Concat,
    [u8; 32],
    Blake2_128Concat,
    AccountId,
    Balance,
    ValueQuery,
>;

/// On-chain `PlacementLevel` replica. Must match `pallet-ads-entity` encoding.
/// 链上 `PlacementLevel` 副本，编码必须与 `pallet-ads-entity` 一致。
#[derive(Encode, Decode)]
enum PlacementLevelLite {
    Entity,
    Shop,
}

/// On-chain `AdPlacementInfo` prefix needed to recover `registered_by`.
/// 链上 `AdPlacementInfo` 前缀，用于恢复 `registered_by`。
#[derive(Encode, Decode)]
struct PlacementInfoLite {
    _entity_id: u64,
    _shop_id: u64,
    _level: PlacementLevelLite,
    _daily_impression_cap: u32,
    _daily_click_cap: u32,
    registered_by: AccountId,
}

fn decode_advertiser(raw: &[u8]) -> Option<AccountId> {
    let mut input = raw;
    AccountId::decode(&mut input).ok()
}

fn decode_registered_by(raw: &[u8]) -> Option<AccountId> {
    PlacementInfoLite::decode(&mut &raw[..])
        .ok()
        .map(|info| info.registered_by)
}

fn unreserve(who: &AccountId, amount: Balance) -> Weight {
    if amount.is_zero() {
        return Weight::zero();
    }
    let _ = <Balances as ReservableCurrency<AccountId>>::unreserve(who, amount);
    RocksDbWeight::get().reads_writes(1, 1)
}

fn pay_from_treasury(who: &AccountId, amount: Balance) -> Weight {
    if amount.is_zero() {
        return Weight::zero();
    }
    let treasury = TreasuryAccountId::get();
    let _ = <Balances as Currency<AccountId>>::transfer(
        &treasury,
        who,
        amount,
        ExistenceRequirement::AllowDeath,
    );
    RocksDbWeight::get().reads_writes(2, 2)
}

fn placement_claim_recipient(placement_id: &[u8; 32]) -> Option<AccountId> {
    if let Some(raw) = RegisteredPlacementsMap::get(placement_id) {
        if let Some(who) = decode_registered_by(&raw) {
            return Some(who);
        }
    }
    if let Some(admin) = CommunityAdminMap::get(placement_id) {
        return Some(admin);
    }
    None
}

/// Refunds Ads user funds, then marks retirement complete.
/// 退还 Ads 用户资金，并标记退役完成。
pub struct RetireAdsFunds;

impl RetireAdsFunds {
    fn refund() -> Weight {
        let mut weight = RocksDbWeight::get().reads(1);
        if AdsRetiredVersion::get() >= RETIRED_VERSION {
            return weight;
        }

        for (campaign_id, escrow) in CampaignEscrowMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(2));
            if escrow.is_zero() {
                continue;
            }
            let Some(raw) = CampaignsMap::get(campaign_id) else {
                log::warn!(
                    target: "runtime::retire_ads",
                    "CampaignEscrow {campaign_id} has no Campaigns row"
                );
                continue;
            };
            let Some(advertiser) = decode_advertiser(&raw) else {
                log::warn!(
                    target: "runtime::retire_ads",
                    "failed to decode advertiser for campaign {campaign_id}"
                );
                continue;
            };
            weight = weight.saturating_add(unreserve(&advertiser, escrow));
        }

        for (placement_id, deposit) in PlacementDepositsMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(2));
            if deposit.is_zero() {
                continue;
            }
            let Some(raw) = RegisteredPlacementsMap::get(placement_id) else {
                log::warn!(
                    target: "runtime::retire_ads",
                    "PlacementDeposits has no RegisteredPlacements row"
                );
                continue;
            };
            let Some(who) = decode_registered_by(&raw) else {
                log::warn!(
                    target: "runtime::retire_ads",
                    "failed to decode registered_by for placement deposit"
                );
                continue;
            };
            weight = weight.saturating_add(unreserve(&who, deposit));
        }

        for (placement_id, claimable) in PlacementClaimableMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(3));
            if claimable.is_zero() {
                continue;
            }
            let Some(who) = placement_claim_recipient(&placement_id) else {
                log::warn!(
                    target: "runtime::retire_ads",
                    "PlacementClaimable has no recipient; leaving funds in treasury"
                );
                continue;
            };
            weight = weight.saturating_add(pay_from_treasury(&who, claimable));
        }

        for (referrer, claimable) in ReferrerClaimableMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(1));
            weight = weight.saturating_add(pay_from_treasury(&referrer, claimable));
        }

        for (_community, staker, amount) in CommunityStakersMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(1));
            weight = weight.saturating_add(unreserve(&staker, amount));
        }

        for (_community, staker, queue) in UnbondingRequestsMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(1));
            let locked: Balance = queue.iter().fold(Balance::zero(), |acc, (amount, _)| {
                acc.saturating_add(*amount)
            });
            weight = weight.saturating_add(unreserve(&staker, locked));
        }

        for (_community, staker, claimable) in StakerClaimableMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(1));
            weight = weight.saturating_add(pay_from_treasury(&staker, claimable));
        }

        AdsRetiredVersion::put(RETIRED_VERSION);
        weight.saturating_add(RocksDbWeight::get().writes(1))
    }
}

impl OnRuntimeUpgrade for RetireAdsFunds {
    fn on_runtime_upgrade() -> Weight {
        Self::refund()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        Ok(AdsRetiredVersion::get().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let previous = u16::decode(&mut &state[..])
            .map_err(|_| "failed to decode ads retirement migration state")?;
        if previous < RETIRED_VERSION && AdsRetiredVersion::get() != RETIRED_VERSION {
            return Err("ads retirement version was not written".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXISTENTIAL_DEPOSIT;
    use frame_support::traits::Currency as CurrencyT;

    fn account(b: u8) -> AccountId {
        AccountId::new([b; 32])
    }

    fn fund(who: &AccountId, amount: Balance) {
        let _ = <Balances as CurrencyT<AccountId>>::deposit_creating(who, amount);
    }

    #[test]
    fn refunds_campaign_escrow_and_is_idempotent() {
        sp_io::TestExternalities::default().execute_with(|| {
            let advertiser = account(1);
            let escrow = 50 * EXISTENTIAL_DEPOSIT;
            fund(&advertiser, escrow * 2);
            assert_eq!(Balances::reserve(&advertiser, escrow), Ok(()));
            CampaignsMap::insert(7u64, advertiser.encode());
            CampaignEscrowMap::insert(7u64, escrow);

            RetireAdsFunds::on_runtime_upgrade();
            assert_eq!(Balances::reserved_balance(&advertiser), 0);
            assert_eq!(AdsRetiredVersion::get(), RETIRED_VERSION);

            let reserved_after = Balances::reserved_balance(&advertiser);
            RetireAdsFunds::on_runtime_upgrade();
            assert_eq!(Balances::reserved_balance(&advertiser), reserved_after);
        });
    }

    #[test]
    fn refunds_placement_deposit_and_claimable() {
        sp_io::TestExternalities::default().execute_with(|| {
            let owner = account(2);
            let treasury = TreasuryAccountId::get();
            let deposit = 10 * EXISTENTIAL_DEPOSIT;
            let claimable = 5 * EXISTENTIAL_DEPOSIT;
            fund(&owner, deposit * 2);
            fund(&treasury, claimable * 2);
            assert_eq!(Balances::reserve(&owner, deposit), Ok(()));

            let pid = [9u8; 32];
            let info = PlacementInfoLite {
                _entity_id: 1,
                _shop_id: 0,
                _level: PlacementLevelLite::Entity,
                _daily_impression_cap: 0,
                _daily_click_cap: 0,
                registered_by: owner.clone(),
            };
            RegisteredPlacementsMap::insert(pid, info.encode());
            PlacementDepositsMap::insert(pid, deposit);
            PlacementClaimableMap::insert(pid, claimable);

            RetireAdsFunds::on_runtime_upgrade();
            assert_eq!(Balances::reserved_balance(&owner), 0);
            assert!(Balances::free_balance(&owner) >= deposit + claimable);
        });
    }

    #[test]
    fn refunds_grouprobot_ad_stake_unbonding_and_staker_claimable() {
        sp_io::TestExternalities::default().execute_with(|| {
            let staker = account(3);
            let treasury = TreasuryAccountId::get();
            let stake = 20 * EXISTENTIAL_DEPOSIT;
            let unbonding = 4 * EXISTENTIAL_DEPOSIT;
            let claimable = 3 * EXISTENTIAL_DEPOSIT;
            fund(&staker, (stake + unbonding) * 2);
            fund(&treasury, claimable * 2);
            assert_eq!(Balances::reserve(&staker, stake + unbonding), Ok(()));

            let community = [4u8; 32];
            CommunityStakersMap::insert(community, staker.clone(), stake);
            UnbondingRequestsMap::insert(community, staker.clone(), vec![(unbonding, 1u32)]);
            StakerClaimableMap::insert(community, staker.clone(), claimable);

            RetireAdsFunds::on_runtime_upgrade();
            assert_eq!(Balances::reserved_balance(&staker), 0);
            assert!(Balances::free_balance(&staker) >= stake + unbonding + claimable);
        });
    }
}
