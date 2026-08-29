//! Mainnet retirement of GroupRobot pallets.
//! 主网退役 GroupRobot pallet。
//!
//! Refunds node stake / subscription escrow / pending rewards using on-chain
//! pallet prefixes, then leftover keys are wiped by `RemovePallet`.
//! 按链上 pallet 前缀退还节点质押 / 订阅托管 / 待领奖励，剩余键由 `RemovePallet` 清除。
//!
//! Do not drain leftover `RewardPoolAccountId`; it still receives 5% of staking
//! inflation via `InflationSplitter`.
//! 不要抽空 `RewardPoolAccountId` 余额；它仍通过 `InflationSplitter` 收取 5% 质押通胀。
//!
//! Indexes 150–155 stay retired and must not be reused.
//! 索引 150–155 永久退役，禁止复用。

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::ValueQuery,
    parameter_types,
    storage::types::StorageMap,
    traits::{
        Currency, ExistenceRequirement, OnRuntimeUpgrade, ReservableCurrency, StorageInstance,
    },
    weights::{constants::RocksDbWeight, Weight},
    Blake2_128Concat,
};
use sp_runtime::traits::Zero;

use crate::{configs::RewardPoolAccountId, AccountId, Balance, Balances};

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

fn unreserve(who: &AccountId, amount: Balance) -> Weight {
    if amount.is_zero() {
        return Weight::zero();
    }
    let _ = <Balances as ReservableCurrency<AccountId>>::unreserve(who, amount);
    RocksDbWeight::get().reads_writes(1, 1)
}

fn pay_from_pool(who: &AccountId, amount: Balance) -> Weight {
    if amount.is_zero() {
        return Weight::zero();
    }
    let pool = RewardPoolAccountId::get();
    let _ = <Balances as Currency<AccountId>>::transfer(
        &pool,
        who,
        amount,
        ExistenceRequirement::AllowDeath,
    );
    RocksDbWeight::get().reads_writes(2, 2)
}

/// Refunds GroupRobot user funds, then marks retirement complete.
/// 退还 GroupRobot 用户资金，并标记退役完成。
pub struct RetireGroupRobotFunds;

impl RetireGroupRobotFunds {
    fn refund() -> Weight {
        let mut weight = RocksDbWeight::get().reads(1);
        if GroupRobotRetiredVersion::get() >= RETIRED_VERSION {
            return weight;
        }

        for (_node_id, raw) in NodesMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(1));
            let Some(node) = decode_node(&raw) else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "failed to decode ProjectNode"
                );
                continue;
            };
            weight = weight.saturating_add(unreserve(&node.operator, node.stake));
        }

        for (bot_id, escrow) in SubscriptionEscrowMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(2));
            if escrow.is_zero() {
                continue;
            }
            let Some(raw) = SubscriptionsMap::get(bot_id) else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "SubscriptionEscrow has no Subscriptions row"
                );
                continue;
            };
            let Some(owner) = decode_account(&raw) else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "failed to decode subscription owner"
                );
                continue;
            };
            weight = weight.saturating_add(unreserve(&owner, escrow));
        }

        for (node_id, pending) in NodePendingRewardsMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(3));
            if pending.is_zero() {
                continue;
            }
            let recipient = RewardRecipientMap::get(node_id).or_else(|| {
                NodesMap::get(node_id).and_then(|raw| decode_node(&raw).map(|n| n.operator))
            });
            let Some(who) = recipient else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "NodePendingRewards has no recipient"
                );
                continue;
            };
            weight = weight.saturating_add(pay_from_pool(&who, pending));
        }

        for (bot_id, pending) in OwnerPendingRewardsMap::iter() {
            weight = weight.saturating_add(RocksDbWeight::get().reads(2));
            if pending.is_zero() {
                continue;
            }
            let Some(raw) = BotsMap::get(bot_id) else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "OwnerPendingRewards has no Bots row"
                );
                continue;
            };
            let Some(owner) = decode_account(&raw) else {
                log::warn!(
                    target: "runtime::retire_grouprobot",
                    "failed to decode bot owner"
                );
                continue;
            };
            weight = weight.saturating_add(pay_from_pool(&owner, pending));
        }

        GroupRobotRetiredVersion::put(RETIRED_VERSION);
        weight.saturating_add(RocksDbWeight::get().writes(1))
    }
}

impl OnRuntimeUpgrade for RetireGroupRobotFunds {
    fn on_runtime_upgrade() -> Weight {
        Self::refund()
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
        Ok(GroupRobotRetiredVersion::get().encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        let previous = u16::decode(&mut &state[..])
            .map_err(|_| "failed to decode grouprobot retirement migration state")?;
        if previous < RETIRED_VERSION && GroupRobotRetiredVersion::get() != RETIRED_VERSION {
            return Err("grouprobot retirement version was not written".into());
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
            let node = ProjectNodeLite {
                operator: operator.clone(),
                _node_id: node_id,
                _status: NodeStatusLite::Active,
                stake,
            };
            NodesMap::insert(node_id, node.encode());

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
            fund(&pool, (node_reward + owner_reward) * 2);

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
}
