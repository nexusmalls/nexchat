//! Mainnet retirement of AdsCore, AdsGroupRobot, and AdsEntity.
//! 主网退役 AdsCore、AdsGroupRobot 与 AdsEntity。
//!
//! Refunds reserved campaign escrow / placement deposits / community ad-stake
//! and pays out claimable revenue from treasury using on-chain pallet prefixes.
//! A failed refund does **not** write `AdsRetiredVersion`, and `on_runtime_upgrade`
//! panics so later `RemovePallet` cannot run in the same block.
//! 按链上 pallet 前缀退还 Campaign escrow / 广告位押金 / 社区广告质押，
//! 并从国库发放待领收入。退款失败**不**写 `AdsRetiredVersion`，且
//! `on_runtime_upgrade` 会 panic，避免同块执行后续 `RemovePallet`。
//!
//! Indexes 160–162 stay retired and must not be reused.
//! 索引 160–162 永久退役，禁止复用。

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::ValueQuery,
    parameter_types,
    storage::types::{StorageDoubleMap, StorageMap},
    traits::{OnRuntimeUpgrade, StorageInstance},
    weights::{constants::RocksDbWeight, Weight},
    Blake2_128Concat,
};
#[cfg(feature = "try-runtime")]
use frame_support::traits::{Currency, ReservableCurrency};
use sp_runtime::traits::Zero;

use super::retire_support;
use crate::{configs::TreasuryAccountId, AccountId, Balance, BlockNumber};
#[cfg(any(test, feature = "try-runtime"))]
use crate::Balances;

/// On-chain construct_runtime names; must match `RemovePallet` prefixes.
/// 链上 construct_runtime 名称，必须与 `RemovePallet` 前缀一致。
#[cfg(any(test, feature = "try-runtime"))]
pub const PALLET_NAMES: [&str; 3] = ["AdsCore", "AdsGroupRobot", "AdsEntity"];

parameter_types! {
    pub const AdsCoreName: &'static str = "AdsCore";
    pub const AdsGroupRobotName: &'static str = "AdsGroupRobot";
    pub const AdsEntityName: &'static str = "AdsEntity";
}

pub type RemoveAdsCore = frame_support::migrations::RemovePallet<AdsCoreName, RocksDbWeight>;
pub type RemoveAdsGroupRobot =
    frame_support::migrations::RemovePallet<AdsGroupRobotName, RocksDbWeight>;
pub type RemoveAdsEntity = frame_support::migrations::RemovePallet<AdsEntityName, RocksDbWeight>;

type RemoveAdsInner = (RemoveAdsCore, RemoveAdsGroupRobot, RemoveAdsEntity);

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

/// Names used by prefix wipe / weight estimates.
/// 供前缀清除与重量估算使用的名称。
#[cfg(any(test, feature = "try-runtime"))]
pub fn pallet_names() -> [&'static str; 3] {
    PALLET_NAMES
}

/// True after a successful refund pass.
/// 退款成功后为 true。
pub fn refund_complete() -> bool {
    AdsRetiredVersion::get() >= RETIRED_VERSION
}

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

fn add_amount(map: &mut BTreeMap<AccountId, Balance>, who: AccountId, amount: Balance) {
    if amount.is_zero() {
        return;
    }
    map.entry(who)
        .and_modify(|v| *v = v.saturating_add(amount))
        .or_insert(amount);
}

fn placement_claim_recipient(placement_id: &[u8; 32]) -> Option<AccountId> {
    if let Some(raw) = RegisteredPlacementsMap::get(placement_id) {
        if let Some(who) = decode_registered_by(&raw) {
            return Some(who);
        }
    }
    CommunityAdminMap::get(placement_id)
}

struct AdsPlan {
    unreserve: BTreeMap<AccountId, Balance>,
    payout: BTreeMap<AccountId, Balance>,
    collect_weight: Weight,
}

/// Estimated DB weight of the ads refund pass (row iteration).
/// Ads 退款扫描的估算 DB 重量。
#[cfg(any(test, feature = "try-runtime"))]
pub fn estimated_refund_weight() -> Weight {
    if refund_complete() {
        return RocksDbWeight::get().reads(1);
    }
    let rows = CampaignEscrowMap::iter()
        .count()
        .saturating_add(PlacementDepositsMap::iter().count())
        .saturating_add(PlacementClaimableMap::iter().count())
        .saturating_add(ReferrerClaimableMap::iter().count())
        .saturating_add(CommunityStakersMap::iter().count())
        .saturating_add(UnbondingRequestsMap::iter().count())
        .saturating_add(StakerClaimableMap::iter().count()) as u64;
    RocksDbWeight::get().reads_writes(rows.saturating_mul(4), rows.saturating_mul(2))
}

/// Wipes Ads prefixes only after refunds succeeded.
/// 仅在退款成功后清除 Ads 前缀。
pub struct RemoveAdsAfterRefund;

impl OnRuntimeUpgrade for RemoveAdsAfterRefund {
    fn on_runtime_upgrade() -> Weight {
        if !refund_complete() {
            retire_support::panic_blocked_wipe("ads");
        }
        <RemoveAdsInner as OnRuntimeUpgrade>::on_runtime_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if !refund_complete() {
            return Err("ads refund must complete before RemovePallet".into());
        }
        <RemoveAdsInner as OnRuntimeUpgrade>::pre_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        <RemoveAdsInner as OnRuntimeUpgrade>::post_upgrade(state)
    }
}

/// Refunds Ads user funds, then marks retirement complete.
/// 退还 Ads 用户资金，并标记退役完成。
pub struct RetireAdsFunds;

impl RetireAdsFunds {
    fn collect_plan() -> Result<AdsPlan, &'static str> {
        let mut unreserve = BTreeMap::<AccountId, Balance>::new();
        let mut payout = BTreeMap::<AccountId, Balance>::new();
        let mut collect_weight = RocksDbWeight::get().reads(1);

        for (campaign_id, escrow) in CampaignEscrowMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(2));
            if escrow.is_zero() {
                continue;
            }
            let raw = CampaignsMap::get(campaign_id).ok_or("CampaignEscrow has no Campaigns row")?;
            let advertiser =
                decode_advertiser(&raw).ok_or("failed to decode advertiser for campaign")?;
            add_amount(&mut unreserve, advertiser, escrow);
        }

        for (placement_id, deposit) in PlacementDepositsMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(2));
            if deposit.is_zero() {
                continue;
            }
            let raw = RegisteredPlacementsMap::get(placement_id)
                .ok_or("PlacementDeposits has no RegisteredPlacements row")?;
            let who = decode_registered_by(&raw)
                .ok_or("failed to decode registered_by for placement deposit")?;
            add_amount(&mut unreserve, who, deposit);
        }

        for (placement_id, claimable) in PlacementClaimableMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(3));
            if claimable.is_zero() {
                continue;
            }
            let who = placement_claim_recipient(&placement_id)
                .ok_or("PlacementClaimable has no recipient")?;
            add_amount(&mut payout, who, claimable);
        }

        for (referrer, claimable) in ReferrerClaimableMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(1));
            add_amount(&mut payout, referrer, claimable);
        }

        for (_community, staker, amount) in CommunityStakersMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(1));
            add_amount(&mut unreserve, staker, amount);
        }

        for (_community, staker, queue) in UnbondingRequestsMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(1));
            let locked: Balance = queue
                .iter()
                .fold(Balance::zero(), |acc, (amount, _)| acc.saturating_add(*amount));
            add_amount(&mut unreserve, staker, locked);
        }

        for (_community, staker, claimable) in StakerClaimableMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(1));
            add_amount(&mut payout, staker, claimable);
        }

        for (who, amount) in unreserve.iter() {
            retire_support::ensure_reserved(who, *amount)?;
        }

        let total_payout: Balance = payout.values().copied().fold(Balance::zero(), |a, b| {
            a.saturating_add(b)
        });
        retire_support::ensure_keep_alive_source(&TreasuryAccountId::get(), total_payout)?;

        Ok(AdsPlan {
            unreserve,
            payout,
            collect_weight,
        })
    }

    fn apply_plan(plan: &AdsPlan) -> Result<Weight, &'static str> {
        let mut weight = plan.collect_weight;
        for (who, amount) in plan.unreserve.iter() {
            weight = weight.saturating_add(retire_support::unreserve_exact(who, *amount)?);
        }
        let treasury = TreasuryAccountId::get();
        for (who, amount) in plan.payout.iter() {
            weight = weight.saturating_add(retire_support::transfer_keep_alive(
                &treasury, who, *amount,
            )?);
        }
        Ok(weight)
    }

    fn try_refund() -> Result<Weight, &'static str> {
        let mut weight = RocksDbWeight::get().reads(1);
        if refund_complete() {
            return Ok(weight);
        }

        let plan = Self::collect_plan()?;
        weight = Self::apply_plan(&plan)?;
        AdsRetiredVersion::put(RETIRED_VERSION);
        Ok(weight.saturating_add(RocksDbWeight::get().writes(1)))
    }

    #[cfg(feature = "try-runtime")]
    fn snapshot(plan: &AdsPlan) -> Vec<u8> {
        let reserved_before: Vec<(AccountId, Balance)> = plan
            .unreserve
            .keys()
            .map(|who| {
                (
                    who.clone(),
                    <Balances as ReservableCurrency<AccountId>>::reserved_balance(who),
                )
            })
            .collect();
        let mut free_before: Vec<(AccountId, Balance)> = plan
            .payout
            .keys()
            .map(|who| {
                (
                    who.clone(),
                    <Balances as Currency<AccountId>>::free_balance(who),
                )
            })
            .collect();
        let treasury = TreasuryAccountId::get();
        free_before.push((
            treasury.clone(),
            <Balances as Currency<AccountId>>::free_balance(&treasury),
        ));
        (
            1u8,
            AdsRetiredVersion::get(),
            reserved_before,
            free_before,
            plan.unreserve
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
            plan.payout
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>(),
        )
            .encode()
    }
}

impl OnRuntimeUpgrade for RetireAdsFunds {
    fn on_runtime_upgrade() -> Weight {
        Self::try_refund().unwrap_or_else(|err| retire_support::panic_refund("ads", err))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        retire_support::ensure_weight_fits(retire_support::estimate_full_retirement_weight())
            .map_err(sp_runtime::DispatchError::Other)?;
        if refund_complete() {
            return Ok((0u8, AdsRetiredVersion::get()).encode());
        }
        let plan = Self::collect_plan().map_err(sp_runtime::DispatchError::Other)?;
        Ok(Self::snapshot(&plan))
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let mut input = &state[..];
        let tag = u8::decode(&mut input).map_err(|_| "failed to decode ads retirement tag")?;
        if tag == 0 {
            let previous = u16::decode(&mut input)
                .map_err(|_| "failed to decode ads retirement migration state")?;
            if previous < RETIRED_VERSION && !refund_complete() {
                return Err("ads retirement version was not written".into());
            }
            return Ok(());
        }

        let (previous, reserved_before, free_before, unreserve, payout): (
            u16,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
        ) = Decode::decode(&mut input).map_err(|_| "failed to decode ads retirement snapshot")?;

        if previous < RETIRED_VERSION && !refund_complete() {
            return Err("ads retirement version was not written".into());
        }

        for (who, amount) in unreserve {
            let before = reserved_before
                .iter()
                .find(|(a, _)| a == &who)
                .map(|(_, b)| *b)
                .unwrap_or(Balance::zero());
            let after = <Balances as ReservableCurrency<AccountId>>::reserved_balance(&who);
            if after != before.saturating_sub(amount) {
                return Err("ads reserved balance was not fully unreserved".into());
            }
        }

        let treasury = TreasuryAccountId::get();
        let total_payout: Balance = payout.iter().fold(Balance::zero(), |acc, (_, n)| {
            acc.saturating_add(*n)
        });
        for (who, amount) in payout {
            if who == treasury {
                continue;
            }
            let before = free_before
                .iter()
                .find(|(a, _)| a == &who)
                .map(|(_, b)| *b)
                .unwrap_or(Balance::zero());
            let after = <Balances as Currency<AccountId>>::free_balance(&who);
            if after != before.saturating_add(amount) {
                return Err("ads claimable was not paid from treasury".into());
            }
        }
        let treasury_before = free_before
            .iter()
            .find(|(a, _)| a == &treasury)
            .map(|(_, b)| *b)
            .unwrap_or(Balance::zero());
        let treasury_after = <Balances as Currency<AccountId>>::free_balance(&treasury);
        if treasury_after != treasury_before.saturating_sub(total_payout) {
            return Err("ads treasury debit does not match claimable payouts".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EXISTENTIAL_DEPOSIT;
    use frame_support::traits::{Currency as CurrencyT, ReservableCurrency};

    fn account(b: u8) -> AccountId {
        AccountId::new([b; 32])
    }

    fn fund(who: &AccountId, amount: Balance) {
        let _ = <Balances as CurrencyT<AccountId>>::deposit_creating(who, amount);
    }

    fn placement_info(owner: AccountId) -> PlacementInfoLite {
        PlacementInfoLite {
            _entity_id: 1,
            _shop_id: 0,
            _level: PlacementLevelLite::Entity,
            _daily_impression_cap: 0,
            _daily_click_cap: 0,
            registered_by: owner,
        }
    }

    #[test]
    fn pallet_names_match_remove_prefixes() {
        assert_eq!(AdsCoreName::get(), PALLET_NAMES[0]);
        assert_eq!(AdsGroupRobotName::get(), PALLET_NAMES[1]);
        assert_eq!(AdsEntityName::get(), PALLET_NAMES[2]);
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
            fund(&treasury, claimable + EXISTENTIAL_DEPOSIT * 2);
            assert_eq!(Balances::reserve(&owner, deposit), Ok(()));

            let pid = [9u8; 32];
            RegisteredPlacementsMap::insert(pid, placement_info(owner.clone()).encode());
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
            fund(&treasury, claimable + EXISTENTIAL_DEPOSIT * 2);
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

    #[test]
    fn try_refund_does_not_write_version_on_decode_failure() {
        sp_io::TestExternalities::default().execute_with(|| {
            let advertiser = account(1);
            let escrow = 10 * EXISTENTIAL_DEPOSIT;
            fund(&advertiser, escrow * 2);
            assert_eq!(Balances::reserve(&advertiser, escrow), Ok(()));
            CampaignsMap::insert(7u64, vec![1, 2, 3]);
            CampaignEscrowMap::insert(7u64, escrow);

            assert_eq!(
                RetireAdsFunds::try_refund(),
                Err("failed to decode advertiser for campaign")
            );
            assert_eq!(AdsRetiredVersion::get(), 0);
            assert_eq!(Balances::reserved_balance(&advertiser), escrow);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_campaign_row_missing() {
        sp_io::TestExternalities::default().execute_with(|| {
            CampaignEscrowMap::insert(7u64, 10 * EXISTENTIAL_DEPOSIT);
            assert_eq!(
                RetireAdsFunds::try_refund(),
                Err("CampaignEscrow has no Campaigns row")
            );
            assert_eq!(AdsRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_treasury_cannot_keep_alive() {
        sp_io::TestExternalities::default().execute_with(|| {
            let owner = account(2);
            let pid = [9u8; 32];
            RegisteredPlacementsMap::insert(pid, placement_info(owner.clone()).encode());
            PlacementClaimableMap::insert(pid, 5 * EXISTENTIAL_DEPOSIT);

            assert_eq!(
                RetireAdsFunds::try_refund(),
                Err("source cannot KeepAlive after paying retirement claimables")
            );
            assert_eq!(AdsRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_claimable_has_no_recipient() {
        sp_io::TestExternalities::default().execute_with(|| {
            PlacementClaimableMap::insert([9u8; 32], 5 * EXISTENTIAL_DEPOSIT);
            assert_eq!(
                RetireAdsFunds::try_refund(),
                Err("PlacementClaimable has no recipient")
            );
            assert_eq!(AdsRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_reserved_is_short() {
        sp_io::TestExternalities::default().execute_with(|| {
            let advertiser = account(1);
            fund(&advertiser, EXISTENTIAL_DEPOSIT * 2);
            CampaignsMap::insert(7u64, advertiser.encode());
            CampaignEscrowMap::insert(7u64, 10 * EXISTENTIAL_DEPOSIT);

            assert_eq!(
                RetireAdsFunds::try_refund(),
                Err("account reserved balance is below the retirement unreserve amount")
            );
            assert_eq!(AdsRetiredVersion::get(), 0);
        });
    }

    #[cfg(feature = "try-runtime")]
    #[test]
    fn try_runtime_hooks_reject_undecodable_campaign_and_accept_clean_refund() {
        sp_io::TestExternalities::default().execute_with(|| {
            let advertiser = account(1);
            let escrow = 50 * EXISTENTIAL_DEPOSIT;
            fund(&advertiser, escrow * 2);
            assert_eq!(Balances::reserve(&advertiser, escrow), Ok(()));
            CampaignsMap::insert(7u64, vec![1, 2, 3]);
            CampaignEscrowMap::insert(7u64, escrow);
            assert!(RetireAdsFunds::pre_upgrade().is_err());

            CampaignsMap::insert(7u64, advertiser.encode());
            let state = RetireAdsFunds::pre_upgrade().expect("pre_upgrade");
            RetireAdsFunds::on_runtime_upgrade();
            RetireAdsFunds::post_upgrade(state).expect("post_upgrade");
            assert_eq!(Balances::reserved_balance(&advertiser), 0);
            assert!(refund_complete());
        });
    }
}
