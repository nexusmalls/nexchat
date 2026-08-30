//! Mainnet retirement of GroupRobot pallets.
//! 主网退役 GroupRobot pallet。
//!
//! Refunds node stake / subscription escrow / pending rewards using on-chain
//! pallet prefixes. A failed refund does **not** write `GroupRobotRetiredVersion`,
//! and `on_runtime_upgrade` panics so later `RemovePallet` cannot run in the same
//! block. Leftover keys are wiped only after refunds succeed.
//! 按链上 pallet 前缀退还节点质押 / 订阅托管 / 待领奖励。退款失败**不**写
//! `GroupRobotRetiredVersion`，且 `on_runtime_upgrade` 会 panic，避免同块执行
//! 后续 `RemovePallet`。剩余键仅在退款成功后清除。
//!
//! Do not drain leftover `RewardPoolAccountId`; it still receives 5% of staking
//! inflation via `InflationSplitter`.
//! 不要抽空 `RewardPoolAccountId` 余额；它仍通过 `InflationSplitter` 收取 5% 质押通胀。
//!
//! Indexes 150–155 stay retired and must not be reused.
//! 索引 150–155 永久退役，禁止复用。

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::ValueQuery,
    parameter_types,
    storage::types::StorageMap,
    traits::{OnRuntimeUpgrade, StorageInstance},
    weights::{constants::RocksDbWeight, Weight},
    Blake2_128Concat,
};
#[cfg(feature = "try-runtime")]
use frame_support::traits::{Currency, ReservableCurrency};
use sp_runtime::traits::Zero;

use super::retire_support;
use crate::{configs::RewardPoolAccountId, AccountId, Balance};
#[cfg(any(test, feature = "try-runtime"))]
use crate::Balances;

/// On-chain construct_runtime names; must match `RemovePallet` prefixes.
/// 链上 construct_runtime 名称，必须与 `RemovePallet` 前缀一致。
#[cfg(any(test, feature = "try-runtime"))]
pub const PALLET_NAMES: [&str; 6] = [
    "GroupRobotRegistry",
    "GroupRobotConsensus",
    "GroupRobotCommunity",
    "GroupRobotCeremony",
    "GroupRobotSubscription",
    "GroupRobotRewards",
];

parameter_types! {
    pub const GroupRobotRegistryName: &'static str = "GroupRobotRegistry";
    pub const GroupRobotConsensusName: &'static str = "GroupRobotConsensus";
    pub const GroupRobotCommunityName: &'static str = "GroupRobotCommunity";
    pub const GroupRobotCeremonyName: &'static str = "GroupRobotCeremony";
    pub const GroupRobotSubscriptionName: &'static str = "GroupRobotSubscription";
    pub const GroupRobotRewardsName: &'static str = "GroupRobotRewards";
}

pub type RemoveGroupRobotRegistry =
    frame_support::migrations::RemovePallet<GroupRobotRegistryName, RocksDbWeight>;
pub type RemoveGroupRobotConsensus =
    frame_support::migrations::RemovePallet<GroupRobotConsensusName, RocksDbWeight>;
pub type RemoveGroupRobotCommunity =
    frame_support::migrations::RemovePallet<GroupRobotCommunityName, RocksDbWeight>;
pub type RemoveGroupRobotCeremony =
    frame_support::migrations::RemovePallet<GroupRobotCeremonyName, RocksDbWeight>;
pub type RemoveGroupRobotSubscription =
    frame_support::migrations::RemovePallet<GroupRobotSubscriptionName, RocksDbWeight>;
pub type RemoveGroupRobotRewards =
    frame_support::migrations::RemovePallet<GroupRobotRewardsName, RocksDbWeight>;

type RemoveGroupRobotInner = (
    RemoveGroupRobotRegistry,
    RemoveGroupRobotConsensus,
    RemoveGroupRobotCommunity,
    RemoveGroupRobotCeremony,
    RemoveGroupRobotSubscription,
    RemoveGroupRobotRewards,
);

const RETIRED_VERSION: u16 = 1;

struct RetiredVersionStorage;
impl StorageInstance for RetiredVersionStorage {
    fn pallet_prefix() -> &'static str {
        "NexusRuntimeMigrations"
    }
    const STORAGE_PREFIX: &'static str = "GroupRobotRetiredVersion";
}
type GroupRobotRetiredVersion =
    frame_support::storage::types::StorageValue<RetiredVersionStorage, u16, ValueQuery>;

/// Names used by prefix wipe / weight estimates.
/// 供前缀清除与重量估算使用的名称。
#[cfg(any(test, feature = "try-runtime"))]
pub fn pallet_names() -> [&'static str; 6] {
    PALLET_NAMES
}

/// True after a successful refund pass.
/// 退款成功后为 true。
pub fn refund_complete() -> bool {
    GroupRobotRetiredVersion::get() >= RETIRED_VERSION
}

struct NodesPrefix;
impl StorageInstance for NodesPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotConsensus"
    }
    const STORAGE_PREFIX: &'static str = "Nodes";
}
type NodesMap = StorageMap<NodesPrefix, Blake2_128Concat, [u8; 32], Vec<u8>>;

struct SubscriptionsPrefix;
impl StorageInstance for SubscriptionsPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotSubscription"
    }
    const STORAGE_PREFIX: &'static str = "Subscriptions";
}
type SubscriptionsMap = StorageMap<SubscriptionsPrefix, Blake2_128Concat, [u8; 32], Vec<u8>>;

struct SubscriptionEscrowPrefix;
impl StorageInstance for SubscriptionEscrowPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotSubscription"
    }
    const STORAGE_PREFIX: &'static str = "SubscriptionEscrow";
}
type SubscriptionEscrowMap =
    StorageMap<SubscriptionEscrowPrefix, Blake2_128Concat, [u8; 32], Balance, ValueQuery>;

struct BotsPrefix;
impl StorageInstance for BotsPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotRegistry"
    }
    const STORAGE_PREFIX: &'static str = "Bots";
}
type BotsMap = StorageMap<BotsPrefix, Blake2_128Concat, [u8; 32], Vec<u8>>;

struct NodePendingRewardsPrefix;
impl StorageInstance for NodePendingRewardsPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotRewards"
    }
    const STORAGE_PREFIX: &'static str = "NodePendingRewards";
}
type NodePendingRewardsMap =
    StorageMap<NodePendingRewardsPrefix, Blake2_128Concat, [u8; 32], Balance, ValueQuery>;

struct OwnerPendingRewardsPrefix;
impl StorageInstance for OwnerPendingRewardsPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotRewards"
    }
    const STORAGE_PREFIX: &'static str = "OwnerPendingRewards";
}
type OwnerPendingRewardsMap =
    StorageMap<OwnerPendingRewardsPrefix, Blake2_128Concat, [u8; 32], Balance, ValueQuery>;

struct RewardRecipientPrefix;
impl StorageInstance for RewardRecipientPrefix {
    fn pallet_prefix() -> &'static str {
        "GroupRobotRewards"
    }
    const STORAGE_PREFIX: &'static str = "RewardRecipient";
}
type RewardRecipientMap = StorageMap<RewardRecipientPrefix, Blake2_128Concat, [u8; 32], AccountId>;

#[derive(Encode, Decode)]
enum NodeStatusLite {
    Active,
    Suspended,
    Exiting,
}

#[derive(Encode, Decode)]
struct ProjectNodeLite {
    operator: AccountId,
    _node_id: [u8; 32],
    _status: NodeStatusLite,
    stake: Balance,
}

fn decode_account(raw: &[u8]) -> Option<AccountId> {
    let mut input = raw;
    AccountId::decode(&mut input).ok()
}

fn decode_node(raw: &[u8]) -> Option<ProjectNodeLite> {
    ProjectNodeLite::decode(&mut &raw[..]).ok()
}

fn add_amount(map: &mut BTreeMap<AccountId, Balance>, who: AccountId, amount: Balance) {
    if amount.is_zero() {
        return;
    }
    map.entry(who)
        .and_modify(|v| *v = v.saturating_add(amount))
        .or_insert(amount);
}

struct GroupRobotPlan {
    unreserve: BTreeMap<AccountId, Balance>,
    payout: BTreeMap<AccountId, Balance>,
    collect_weight: Weight,
}

/// Estimated DB weight of the GroupRobot refund pass.
/// GroupRobot 退款扫描的估算 DB 重量。
#[cfg(any(test, feature = "try-runtime"))]
pub fn estimated_refund_weight() -> Weight {
    if refund_complete() {
        return RocksDbWeight::get().reads(1);
    }
    let rows = NodesMap::iter()
        .count()
        .saturating_add(SubscriptionEscrowMap::iter().count())
        .saturating_add(NodePendingRewardsMap::iter().count())
        .saturating_add(OwnerPendingRewardsMap::iter().count()) as u64;
    RocksDbWeight::get().reads_writes(rows.saturating_mul(4), rows.saturating_mul(2))
}

/// Wipes GroupRobot prefixes only after refunds succeeded.
/// 仅在退款成功后清除 GroupRobot 前缀。
pub struct RemoveGroupRobotAfterRefund;

impl OnRuntimeUpgrade for RemoveGroupRobotAfterRefund {
    fn on_runtime_upgrade() -> Weight {
        if !refund_complete() {
            retire_support::panic_blocked_wipe("grouprobot");
        }
        <RemoveGroupRobotInner as OnRuntimeUpgrade>::on_runtime_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if !refund_complete() {
            return Err("grouprobot refund must complete before RemovePallet".into());
        }
        <RemoveGroupRobotInner as OnRuntimeUpgrade>::pre_upgrade()
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        <RemoveGroupRobotInner as OnRuntimeUpgrade>::post_upgrade(state)
    }
}

/// Refunds GroupRobot user funds, then marks retirement complete.
/// 退还 GroupRobot 用户资金，并标记退役完成。
pub struct RetireGroupRobotFunds;

impl RetireGroupRobotFunds {
    fn collect_plan() -> Result<GroupRobotPlan, &'static str> {
        let mut unreserve = BTreeMap::<AccountId, Balance>::new();
        let mut payout = BTreeMap::<AccountId, Balance>::new();
        let mut collect_weight = RocksDbWeight::get().reads(1);

        for (_node_id, raw) in NodesMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(1));
            let node = decode_node(&raw).ok_or("failed to decode ProjectNode")?;
            add_amount(&mut unreserve, node.operator, node.stake);
        }

        for (bot_id, escrow) in SubscriptionEscrowMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(2));
            if escrow.is_zero() {
                continue;
            }
            let raw = SubscriptionsMap::get(bot_id).ok_or("SubscriptionEscrow has no Subscriptions row")?;
            let owner = decode_account(&raw).ok_or("failed to decode subscription owner")?;
            add_amount(&mut unreserve, owner, escrow);
        }

        for (node_id, pending) in NodePendingRewardsMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(3));
            if pending.is_zero() {
                continue;
            }
            let recipient = RewardRecipientMap::get(node_id).or_else(|| {
                NodesMap::get(node_id).and_then(|raw| decode_node(&raw).map(|n| n.operator))
            });
            let who = recipient.ok_or("NodePendingRewards has no recipient")?;
            add_amount(&mut payout, who, pending);
        }

        for (bot_id, pending) in OwnerPendingRewardsMap::iter() {
            collect_weight = collect_weight.saturating_add(RocksDbWeight::get().reads(2));
            if pending.is_zero() {
                continue;
            }
            let raw = BotsMap::get(bot_id).ok_or("OwnerPendingRewards has no Bots row")?;
            let owner = decode_account(&raw).ok_or("failed to decode bot owner")?;
            add_amount(&mut payout, owner, pending);
        }

        for (who, amount) in unreserve.iter() {
            retire_support::ensure_reserved(who, *amount)?;
        }

        let total_payout: Balance = payout.values().copied().fold(Balance::zero(), |a, b| {
            a.saturating_add(b)
        });
        retire_support::ensure_keep_alive_source(&RewardPoolAccountId::get(), total_payout)?;

        Ok(GroupRobotPlan {
            unreserve,
            payout,
            collect_weight,
        })
    }

    fn apply_plan(plan: &GroupRobotPlan) -> Result<Weight, &'static str> {
        let mut weight = plan.collect_weight;
        for (who, amount) in plan.unreserve.iter() {
            weight = weight.saturating_add(retire_support::unreserve_exact(who, *amount)?);
        }
        let pool = RewardPoolAccountId::get();
        for (who, amount) in plan.payout.iter() {
            weight = weight.saturating_add(retire_support::transfer_keep_alive(&pool, who, *amount)?);
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
        GroupRobotRetiredVersion::put(RETIRED_VERSION);
        Ok(weight.saturating_add(RocksDbWeight::get().writes(1)))
    }

    #[cfg(feature = "try-runtime")]
    fn snapshot(plan: &GroupRobotPlan) -> Vec<u8> {
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
        let pool = RewardPoolAccountId::get();
        free_before.push((
            pool.clone(),
            <Balances as Currency<AccountId>>::free_balance(&pool),
        ));
        (
            1u8,
            GroupRobotRetiredVersion::get(),
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

impl OnRuntimeUpgrade for RetireGroupRobotFunds {
    fn on_runtime_upgrade() -> Weight {
        Self::try_refund().unwrap_or_else(|err| retire_support::panic_refund("grouprobot", err))
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        if refund_complete() {
            return Ok((0u8, GroupRobotRetiredVersion::get()).encode());
        }
        let plan = Self::collect_plan().map_err(sp_runtime::DispatchError::Other)?;
        Ok(Self::snapshot(&plan))
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let mut input = &state[..];
        let tag = u8::decode(&mut input)
            .map_err(|_| "failed to decode grouprobot retirement tag")?;
        if tag == 0 {
            let previous = u16::decode(&mut input)
                .map_err(|_| "failed to decode grouprobot retirement migration state")?;
            if previous < RETIRED_VERSION && !refund_complete() {
                return Err("grouprobot retirement version was not written".into());
            }
            return Ok(());
        }

        let (previous, reserved_before, free_before, unreserve, payout): (
            u16,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
            Vec<(AccountId, Balance)>,
        ) = Decode::decode(&mut input)
            .map_err(|_| "failed to decode grouprobot retirement snapshot")?;

        if previous < RETIRED_VERSION && !refund_complete() {
            return Err("grouprobot retirement version was not written".into());
        }

        for (who, amount) in unreserve {
            let before = reserved_before
                .iter()
                .find(|(a, _)| a == &who)
                .map(|(_, b)| *b)
                .unwrap_or(Balance::zero());
            let after = <Balances as ReservableCurrency<AccountId>>::reserved_balance(&who);
            if after != before.saturating_sub(amount) {
                return Err("grouprobot reserved balance was not fully unreserved".into());
            }
        }

        let pool = RewardPoolAccountId::get();
        let total_payout: Balance = payout.iter().fold(Balance::zero(), |acc, (_, n)| {
            acc.saturating_add(*n)
        });
        for (who, amount) in payout {
            if who == pool {
                continue;
            }
            let before = free_before
                .iter()
                .find(|(a, _)| a == &who)
                .map(|(_, b)| *b)
                .unwrap_or(Balance::zero());
            let after = <Balances as Currency<AccountId>>::free_balance(&who);
            if after != before.saturating_add(amount) {
                return Err("grouprobot pending reward was not paid from the pool".into());
            }
        }
        let pool_before = free_before
            .iter()
            .find(|(a, _)| a == &pool)
            .map(|(_, b)| *b)
            .unwrap_or(Balance::zero());
        let pool_after = <Balances as Currency<AccountId>>::free_balance(&pool);
        if pool_after != pool_before.saturating_sub(total_payout) {
            return Err("grouprobot reward-pool debit does not match pending payouts".into());
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

    fn node(operator: AccountId, node_id: [u8; 32], stake: Balance) -> ProjectNodeLite {
        ProjectNodeLite {
            operator,
            _node_id: node_id,
            _status: NodeStatusLite::Active,
            stake,
        }
    }

    #[test]
    fn pallet_names_match_remove_prefixes() {
        assert_eq!(GroupRobotRegistryName::get(), PALLET_NAMES[0]);
        assert_eq!(GroupRobotConsensusName::get(), PALLET_NAMES[1]);
        assert_eq!(GroupRobotCommunityName::get(), PALLET_NAMES[2]);
        assert_eq!(GroupRobotCeremonyName::get(), PALLET_NAMES[3]);
        assert_eq!(GroupRobotSubscriptionName::get(), PALLET_NAMES[4]);
        assert_eq!(GroupRobotRewardsName::get(), PALLET_NAMES[5]);
    }

    #[test]
    fn refunds_node_stake_and_subscription_escrow() {
        sp_io::TestExternalities::default().execute_with(|| {
            let operator = account(1);
            let owner = account(2);
            let stake = 20 * EXISTENTIAL_DEPOSIT;
            let escrow = 8 * EXISTENTIAL_DEPOSIT;
            fund(&operator, stake * 2);
            fund(&owner, escrow * 2);
            assert_eq!(Balances::reserve(&operator, stake), Ok(()));
            assert_eq!(Balances::reserve(&owner, escrow), Ok(()));

            let node_id = [7u8; 32];
            NodesMap::insert(node_id, node(operator.clone(), node_id, stake).encode());

            let bot_id = [8u8; 32];
            SubscriptionsMap::insert(bot_id, owner.encode());
            SubscriptionEscrowMap::insert(bot_id, escrow);

            RetireGroupRobotFunds::on_runtime_upgrade();
            assert_eq!(Balances::reserved_balance(&operator), 0);
            assert_eq!(Balances::reserved_balance(&owner), 0);
            assert_eq!(GroupRobotRetiredVersion::get(), RETIRED_VERSION);
        });
    }

    #[test]
    fn refunds_pending_rewards_from_pool_and_is_idempotent() {
        sp_io::TestExternalities::default().execute_with(|| {
            let operator = account(3);
            let owner = account(4);
            let pool = RewardPoolAccountId::get();
            let node_reward = 5 * EXISTENTIAL_DEPOSIT;
            let owner_reward = 3 * EXISTENTIAL_DEPOSIT;
            fund(&pool, node_reward + owner_reward + EXISTENTIAL_DEPOSIT * 2);

            let node_id = [9u8; 32];
            RewardRecipientMap::insert(node_id, operator.clone());
            NodePendingRewardsMap::insert(node_id, node_reward);

            let bot_id = [10u8; 32];
            BotsMap::insert(bot_id, owner.encode());
            OwnerPendingRewardsMap::insert(bot_id, owner_reward);

            RetireGroupRobotFunds::on_runtime_upgrade();
            assert!(Balances::free_balance(&operator) >= node_reward);
            assert!(Balances::free_balance(&owner) >= owner_reward);

            let op_after = Balances::free_balance(&operator);
            RetireGroupRobotFunds::on_runtime_upgrade();
            assert_eq!(Balances::free_balance(&operator), op_after);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_on_decode_failure() {
        sp_io::TestExternalities::default().execute_with(|| {
            NodesMap::insert([7u8; 32], vec![1, 2, 3]);
            assert_eq!(
                RetireGroupRobotFunds::try_refund(),
                Err("failed to decode ProjectNode")
            );
            assert_eq!(GroupRobotRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_escrow_row_missing() {
        sp_io::TestExternalities::default().execute_with(|| {
            SubscriptionEscrowMap::insert([8u8; 32], 8 * EXISTENTIAL_DEPOSIT);
            assert_eq!(
                RetireGroupRobotFunds::try_refund(),
                Err("SubscriptionEscrow has no Subscriptions row")
            );
            assert_eq!(GroupRobotRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_pool_cannot_keep_alive() {
        sp_io::TestExternalities::default().execute_with(|| {
            let operator = account(3);
            let node_id = [9u8; 32];
            RewardRecipientMap::insert(node_id, operator);
            NodePendingRewardsMap::insert(node_id, 5 * EXISTENTIAL_DEPOSIT);

            assert_eq!(
                RetireGroupRobotFunds::try_refund(),
                Err("source cannot KeepAlive after paying retirement claimables")
            );
            assert_eq!(GroupRobotRetiredVersion::get(), 0);
        });
    }

    #[test]
    fn try_refund_does_not_write_version_when_pending_has_no_recipient() {
        sp_io::TestExternalities::default().execute_with(|| {
            NodePendingRewardsMap::insert([9u8; 32], 5 * EXISTENTIAL_DEPOSIT);
            assert_eq!(
                RetireGroupRobotFunds::try_refund(),
                Err("NodePendingRewards has no recipient")
            );
            assert_eq!(GroupRobotRetiredVersion::get(), 0);
        });
    }
}
