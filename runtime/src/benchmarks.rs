// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

frame_benchmarking::define_benchmarks!(
    [frame_benchmarking, BaselineBench::<Runtime>]
    [frame_system, SystemBench::<Runtime>]
    [frame_system_extensions, SystemExtensionsBench::<Runtime>]
    [pallet_balances, Balances]
    [pallet_babe, Babe]
    [pallet_staking, Staking]
    [pallet_timestamp, Timestamp]
    [pallet_sudo, Sudo]

    // Trading
    [pallet_nex_market, NexMarket]

    // Dispute
    [pallet_dispute_escrow, Escrow]
    [pallet_dispute_evidence, Evidence]
    [pallet_dispute_arbitration, Arbitration]

    // Storage
    [pallet_storage_service, StorageService]
    [pallet_storage_lifecycle, StorageLifecycle]

    // Entity
    [pallet_entity_registry, EntityRegistry]
    [pallet_entity_shop, EntityShop]
    [pallet_entity_order, EntityTransaction]
    [pallet_entity_review, EntityReview]
    [pallet_entity_governance, EntityGovernance]
    [pallet_entity_market, EntityMarket]
    [pallet_entity_product, EntityProduct]
    [pallet_entity_token, EntityToken]
    [pallet_entity_disclosure, EntityDisclosure]
    [pallet_entity_kyc, EntityKyc]
    [pallet_entity_tokensale, EntityTokenSale]

    // Commission
    [pallet_commission_core, CommissionCore]
    [pallet_commission_pool_reward, CommissionPoolReward]

    // GroupRobot
    [pallet_grouprobot_registry, GroupRobotRegistry]
    [pallet_grouprobot_consensus, GroupRobotConsensus]
    [pallet_grouprobot_community, GroupRobotCommunity]
    [pallet_grouprobot_ceremony, GroupRobotCeremony]
    [pallet_grouprobot_rewards, GroupRobotRewards]
    [pallet_grouprobot_subscription, GroupRobotSubscription]

    // Ads
    [pallet_ads_core, AdsCore]
    [pallet_ads_entity, AdsEntity]
    [pallet_ads_grouprobot, AdsGroupRobot]

    // Chat
    [pallet_chat_group, ChatGroup]
    [pallet_chat_core, ChatCore]
    [pallet_chat_permission, ChatPermission]
    [pallet_chat_inbox, ChatInbox]
    [pallet_chat_sync, ChatSync]
);
