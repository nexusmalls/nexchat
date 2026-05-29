/**
 * Nexus Runtime API definitions for polkadot.js.
 *
 * polkadot.js v12 uses Metadata V14 by default, which does NOT include runtime API
 * definitions. Without explicit registration here, `api.call.<apiName>` is `undefined`.
 *
 * Each entry in `runtimeDefs` corresponds to one `decl_runtime_apis!` trait block in Rust.
 * The key must be the camelCase version of the Rust trait name (e.g. `MemberTeamApi` → `memberTeamApi`).
 *
 * Type mapping rules (Rust → polkadot.js type string):
 *   u8/u16/u32/u64/u128   → 'u8'/'u16'/'u32'/'u64'/'u128'
 *   bool                  → 'bool'
 *   Vec<u8>               → 'Bytes'
 *   AccountId             → 'AccountId'
 *   Balance               → 'u128'
 *   Option<T>             → 'Option<T>'
 *   Vec<T>                → 'Vec<T>'
 *   (A, B)                → '(A, B)'
 *   struct Foo            → 'Foo'  (must also be registered in customTypes)
 */

import type { DefinitionsCall } from '@polkadot/types/types';
import type { RegistryTypes } from '@polkadot/types/types';

// ---------------------------------------------------------------------------
// Composite type definitions (Rust structs / enums → SCALE JSON codec format)
// These must cover every struct/enum used as a parameter or return type in the
// runtime API methods below.
// ---------------------------------------------------------------------------

export const customTypes: RegistryTypes = {
  // --- PoolRewardDetailApi types ---
  CapBehaviorInfo: {
    _enum: {
      Fixed: 'Null',
      UnlockByTeam: 'CapBehaviorUnlockByTeam',
    },
  },
  CapBehaviorUnlockByTeam: {
    direct_per_unlock: 'u32',
    team_per_unlock: 'u32',
    unlock_percent: 'u16',
    baseline_direct: 'u32',
    baseline_team: 'u32',
  },
  LevelRuleSummaryInfo: {
    level_id: 'u8',
    base_cap_percent: 'u16',
    cap_behavior: 'CapBehaviorInfo',
  },
  AdminLevelRuleInfo: {
    level_id: 'u8',
    base_cap_percent: 'u16',
    cap_behavior: 'CapBehaviorInfo',
    member_count: 'u32',
    capped_member_count: 'u32',
  },
  MemberStatsInfo: {
    direct_count: 'u32',
    team_count: 'u32',
    total_spent: 'u128',
    upgrade_eligible_spent: 'u128',
    cap_basis_spent_usdt: 'u128',
  },
  MemberCapInfo: {
    cumulative_claimed_usdt: 'u128',
    current_cap_usdt: 'u128',
    remaining_cap_usdt: 'u128',
    is_capped: 'bool',
    quota_nex_before_cap: 'u128',
    rate_snapshot_used: 'Option<u64>',
    base_cap_percent: 'u16',
    base_cap_usdt: 'u128',
    unlock_count: 'u32',
    unlock_percent: 'Option<u16>',
    unlock_amount_per_step_usdt: 'Option<u128>',
    next_direct_gap: 'Option<u32>',
    next_team_gap: 'Option<u32>',
    next_unlock_increase_usdt: 'Option<u128>',
  },
  LevelProgressInfo: {
    level_id: 'u8',
    ratio_bps: 'u16',
    member_count: 'u32',
    claimed_count: 'u32',
    per_member_reward: 'u128',
  },
  ClaimRecordInfo: {
    round_id: 'u64',
    amount: 'u128',
    token_amount: 'u128',
    level_id: 'u8',
    claimed_at: 'u64',
  },
  FundingSummaryInfo: {
    nex_commission_remainder: 'u128',
    token_platform_fee_retention: 'u128',
    token_commission_remainder: 'u128',
    nex_cancel_return: 'u128',
    total_funding_count: 'u32',
  },
  RoundDetailInfo: {
    round_id: 'u64',
    start_block: 'u64',
    end_block: 'u64',
    pool_snapshot: 'u128',
    nex_usdt_rate_snapshot: 'Option<u64>',
    eligible_count: 'u32',
    per_member_reward: 'u128',
    claimed_count: 'u32',
    token_pool_snapshot: 'Option<u128>',
    token_per_member_reward: 'Option<u128>',
    token_claimed_count: 'u32',
    level_snapshots: 'Vec<LevelProgressInfo>',
    token_level_snapshots: 'Option<Vec<LevelProgressInfo>>',
  },
  CompletedRoundInfo: {
    round_id: 'u64',
    start_block: 'u64',
    end_block: 'u64',
    pool_snapshot: 'u128',
    nex_usdt_rate_snapshot: 'Option<u64>',
    eligible_count: 'u32',
    per_member_reward: 'u128',
    claimed_count: 'u32',
    token_pool_snapshot: 'Option<u128>',
    token_per_member_reward: 'Option<u128>',
    token_claimed_count: 'u32',
    level_snapshots: 'Vec<LevelProgressInfo>',
    token_level_snapshots: 'Option<Vec<LevelProgressInfo>>',
    funding_summary: 'FundingSummaryInfo',
  },
  PendingConfigInfo: {
    level_rules: 'Vec<(u8, u16)>',
    level_rule_details: 'Vec<LevelRuleSummaryInfo>',
    round_duration: 'u64',
    apply_after: 'u64',
  },
  PoolRewardMemberView: {
    round_duration: 'u64',
    token_pool_enabled: 'bool',
    level_rules: 'Vec<(u8, u16)>',
    level_rule_details: 'Vec<LevelRuleSummaryInfo>',
    current_round_id: 'u64',
    round_start_block: 'u64',
    round_end_block: 'u64',
    pool_snapshot: 'u128',
    token_pool_snapshot: 'Option<u128>',
    effective_level: 'u8',
    claimable_nex: 'u128',
    claimable_token: 'u128',
    already_claimed: 'bool',
    round_expired: 'bool',
    last_claimed_round: 'u64',
    member_stats: 'MemberStatsInfo',
    cap_info: 'MemberCapInfo',
    level_progress: 'Vec<LevelProgressInfo>',
    token_level_progress: 'Option<Vec<LevelProgressInfo>>',
    claim_history: 'Vec<ClaimRecordInfo>',
    is_paused: 'bool',
    has_pending_config: 'bool',
  },
  PoolRewardAdminView: {
    level_rules: 'Vec<(u8, u16)>',
    level_rule_details: 'Vec<AdminLevelRuleInfo>',
    round_duration: 'u64',
    token_pool_enabled: 'bool',
    current_round: 'Option<RoundDetailInfo>',
    total_nex_distributed: 'u128',
    total_token_distributed: 'u128',
    total_rounds_completed: 'u64',
    total_claims: 'u64',
    round_history: 'Vec<CompletedRoundInfo>',
    pending_config: 'Option<PendingConfigInfo>',
    is_paused: 'bool',
    is_global_paused: 'bool',
    current_pool_balance: 'u128',
    current_token_pool_balance: 'u128',
    token_pool_deficit: 'u128',
  },

  // --- MemberTeamApi types ---
  UpgradeRecordInfo: {
    rule_id: 'u32',
    from_level_id: 'u8',
    to_level_id: 'u8',
    upgraded_at: 'u64',
    expires_at: 'Option<u64>',
  },
  MemberDashboardInfo: {
    referrer: 'Option<AccountId>',
    custom_level_id: 'u8',
    effective_level_id: 'u8',
    total_spent: 'u64',
    direct_referrals: 'u32',
    indirect_referrals: 'u32',
    team_size: 'u32',
    order_count: 'u32',
    joined_at: 'u64',
    last_active_at: 'u64',
    activated: 'bool',
    is_banned: 'bool',
    banned_at: 'Option<u64>',
    ban_reason: 'Option<Bytes>',
    level_expires_at: 'Option<u64>',
    upgrade_history: 'Vec<UpgradeRecordInfo>',
  },
  TeamMemberInfo: {
    account: 'AccountId',
    level_id: 'u8',
    total_spent: 'u64',
    direct_referrals: 'u32',
    team_size: 'u32',
    joined_at: 'u64',
    last_active_at: 'u64',
    is_banned: 'bool',
    children: 'Vec<TeamMemberInfo>',
  },
  PaginatedMemberInfo: {
    account: 'AccountId',
    level_id: 'u8',
    total_spent: 'u64',
    direct_referrals: 'u32',
    team_size: 'u32',
    joined_at: 'u64',
    is_banned: 'bool',
    ban_reason: 'Option<Bytes>',
  },
  PaginatedMembersResult: {
    members: 'Vec<PaginatedMemberInfo>',
    total: 'u32',
    has_more: 'bool',
  },
  EntityMemberOverview: {
    total_members: 'u32',
    level_distribution: 'Vec<(u8, u32)>',
    pending_count: 'u32',
    banned_count: 'u32',
  },
  UplineNode: {
    account: 'AccountId',
    level_id: 'u8',
    team_size: 'u32',
    joined_at: 'u64',
  },
  UplineChainResult: {
    chain: 'Vec<UplineNode>',
    truncated: 'bool',
    depth: 'u32',
  },
  ReferralTreeNode: {
    account: 'AccountId',
    level_id: 'u8',
    direct_referrals: 'u32',
    team_size: 'u32',
    total_spent: 'u64',
    joined_at: 'u64',
    is_banned: 'bool',
    children: 'Vec<ReferralTreeNode>',
    has_more_children: 'bool',
  },
  GenerationMemberInfo: {
    account: 'AccountId',
    level_id: 'u8',
    direct_referrals: 'u32',
    team_size: 'u32',
    total_spent: 'u64',
    joined_at: 'u64',
    is_banned: 'bool',
    referrer: 'AccountId',
  },
  PaginatedGenerationResult: {
    generation: 'u32',
    members: 'Vec<GenerationMemberInfo>',
    total_count: 'u32',
    page_size: 'u32',
    page_index: 'u32',
    has_more: 'bool',
  },
};

// ---------------------------------------------------------------------------
// Runtime API call definitions
// Key naming: Rust trait `FooBarApi` → JS key `fooBarApi`
// ---------------------------------------------------------------------------

export const runtimeDefs: DefinitionsCall = {
  // PoolRewardDetailApi (pallet_commission_pool_reward::runtime_api::PoolRewardDetailApi)
  PoolRewardDetailApi: [
    {
      methods: {
        get_pool_reward_member_view: {
          description: 'Get pool reward member view (personal state + round progress + history)',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
          ],
          type: 'Option<PoolRewardMemberView>',
        },
        get_pool_reward_admin_view: {
          description: 'Get pool reward admin view (config + stats + history + pending)',
          params: [
            { name: 'entity_id', type: 'u64' },
          ],
          type: 'Option<PoolRewardAdminView>',
        },
      },
      version: 1,
    },
  ],

  // MemberTeamApi (pallet_entity_member::runtime_api::MemberTeamApi)
  MemberTeamApi: [
    {
      methods: {
        get_member_info: {
          description: 'Get member dashboard info',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
          ],
          type: 'Option<MemberDashboardInfo>',
        },
        get_referral_team: {
          description: 'Get referral team tree',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
            { name: 'depth', type: 'u32' },
          ],
          type: 'Vec<TeamMemberInfo>',
        },
        get_entity_member_overview: {
          description: 'Get entity member overview',
          params: [{ name: 'entity_id', type: 'u64' }],
          type: 'EntityMemberOverview',
        },
        get_members_paginated: {
          description: 'Get paginated member list',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'page_size', type: 'u32' },
            { name: 'page_index', type: 'u32' },
          ],
          type: 'PaginatedMembersResult',
        },
        get_upline_chain: {
          description: 'Get upline referral chain',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
            { name: 'max_depth', type: 'u32' },
          ],
          type: 'UplineChainResult',
        },
        get_referral_tree: {
          description: 'Get deep referral tree',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
            { name: 'depth', type: 'u32' },
          ],
          type: 'ReferralTreeNode',
        },
        get_referrals_by_generation: {
          description: 'Get referrals by generation (paginated)',
          params: [
            { name: 'entity_id', type: 'u64' },
            { name: 'account', type: 'AccountId' },
            { name: 'generation', type: 'u32' },
            { name: 'page_size', type: 'u32' },
            { name: 'page_index', type: 'u32' },
          ],
          type: 'PaginatedGenerationResult',
        },
      },
      version: 1,
    },
  ],
};
