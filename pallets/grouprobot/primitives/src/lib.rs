#![cfg_attr(not(feature = "std"), no_std)]

//! # GroupRobot Primitives — Shared types and trait interfaces.
//! # GroupRobot Primitives — 共享类型与 Trait 接口。
//!
//! This crate is used by all grouprobot pallets.
//! 所有 grouprobot 子 pallet 都依赖此 crate。
//! It has no storage or extrinsics and only provides shared types and traits.
//! 无 Storage、无 Extrinsic，纯类型与 Trait 定义。

use codec::{Decode, Encode, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;

// Re-export 通用广告类型 (已迁移到 ads-primitives)
pub use pallet_ads_primitives::{
    AdReviewStatus, CampaignStatus, DeliveryVerifier, PlacementAdminProvider, PlacementId,
    RevenueDistributor,
};

// ============================================================================
// Type Aliases
// ============================================================================

/// 节点 ID (32 bytes)
pub type NodeId = [u8; 32];

/// Bot ID Hash (32 bytes)
pub type BotIdHash = [u8; 32];

/// 社区 ID Hash (32 bytes)
pub type CommunityIdHash = [u8; 32];

// ============================================================================
// Enums
// ============================================================================

/// Social platform enum.
/// 社交平台枚举。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum Platform {
    Telegram,
    Discord,
    Slack,
    Matrix,
    Farcaster,
}

impl Default for Platform {
    fn default() -> Self {
        Self::Telegram
    }
}

/// Bot status.
/// Bot 状态。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum BotStatus {
    Active,
    Suspended,
    Deactivated,
}

impl Default for BotStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// TEE hardware type.
/// TEE 硬件类型。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum TeeType {
    /// TDX-Only: primary_measurement = MRTD (48 bytes)
    Tdx,
    /// SGX-Only: primary_measurement = MRENCLAVE (32 bytes, padded to 48)
    Sgx,
    /// TDX + SGX 双证明: primary_measurement = MRTD, mrenclave = Some(...)
    TdxPlusSgx,
}

impl Default for TeeType {
    fn default() -> Self {
        Self::Tdx
    }
}

/// Node type, distinguishing standard nodes from TEE-backed nodes.
/// 节点类型（标准 vs TEE）。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum NodeType {
    /// 普通节点 (无 TEE 证明)
    StandardNode,
    /// V1: TEE 节点 (TDX + 可选 SGX, 向后兼容)
    TeeNode {
        /// TDX Trust Domain 度量值 (48 bytes)
        mrtd: [u8; 48],
        /// SGX Enclave 度量值 (32 bytes, 可选)
        mrenclave: Option<[u8; 32]>,
        /// TDX Quote 提交区块
        tdx_attested_at: u64,
        /// SGX Quote 提交区块
        sgx_attested_at: Option<u64>,
        /// 证明过期区块
        expires_at: u64,
    },
    /// V2: 三模式统一 TEE 节点 (SGX-Only / TDX-Only / TDX+SGX)
    TeeNodeV2 {
        /// 统一度量值: MRTD(48B) 或 MRENCLAVE(32B + 16B zero-pad)
        primary_measurement: [u8; 48],
        /// TEE 类型
        tee_type: TeeType,
        /// SGX MRENCLAVE (原始 32B; TDX+SGX 双证明时有值, SGX-Only 时同 primary[..32])
        mrenclave: Option<[u8; 32]>,
        /// 主证明提交区块
        attested_at: u64,
        /// 补充 SGX 证明提交区块 (仅 TDX+SGX)
        sgx_attested_at: Option<u64>,
        /// 证明过期区块
        expires_at: u64,
    },
}

impl Default for NodeType {
    fn default() -> Self {
        Self::StandardNode
    }
}

/// Operator status.
/// 运营商状态。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum OperatorStatus {
    Active,
    Suspended,
    Deactivated,
}

impl Default for OperatorStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Node status.
/// 节点状态。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum NodeStatus {
    Active,
    Suspended,
    Exiting,
}

impl Default for NodeStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Subscription tier.
/// 订阅层级。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum SubscriptionTier {
    /// 免费层级 (默认, 无链上订阅记录)
    Free,
    Basic,
    Pro,
    Enterprise,
}

impl Default for SubscriptionTier {
    fn default() -> Self {
        Self::Free
    }
}

impl SubscriptionTier {
    /// Whether this tier requires a paid subscription.
    /// 该层级是否需要付费订阅
    pub fn is_paid(&self) -> bool {
        !matches!(self, Self::Free)
    }

    /// Returns the feature limits for this tier.
    /// 获取该层级的功能限制
    pub fn feature_gate(&self) -> TierFeatureGate {
        match self {
            Self::Free => TierFeatureGate {
                max_rules: 3,
                log_retention_days: 7,
                forced_ads_per_day: 2,
                can_disable_ads: false,
                tee_access: false,
            },
            Self::Basic => TierFeatureGate {
                max_rules: 10,
                log_retention_days: 30,
                forced_ads_per_day: 0,
                can_disable_ads: true,
                tee_access: false,
            },
            Self::Pro => TierFeatureGate {
                max_rules: 50,
                log_retention_days: 90,
                forced_ads_per_day: 0,
                can_disable_ads: true,
                tee_access: true,
            },
            Self::Enterprise => TierFeatureGate {
                max_rules: u16::MAX,
                log_retention_days: 0, // 0 = 永久
                forced_ads_per_day: 0,
                can_disable_ads: true,
                tee_access: true,
            },
        }
    }
}

/// Tier feature limits.
/// 层级功能限制。
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub struct TierFeatureGate {
    /// Maximum number of enabled rules.
    /// 最大可启用规则数
    pub max_rules: u16,
    /// Log-retention period in days. `0` means permanent retention.
    /// 日志保留天数（`0` = 永久）
    pub log_retention_days: u16,
    /// Number of forced ads per day. `0` means none.
    /// 每日强制广告数（`0` = 无强制）
    pub forced_ads_per_day: u8,
    /// Whether ads can be disabled.
    /// 是否可关闭广告
    pub can_disable_ads: bool,
    /// Whether TEE nodes are available for this tier.
    /// 是否可使用 TEE 节点
    pub tee_access: bool,
}

/// 订阅状态
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Suspended,
    Cancelled,
    /// Owner 主动暂停 (不扣费, 不享受层级)
    Paused,
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 广告承诺订阅状态
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum AdCommitmentStatus {
    /// 正常履约中
    Active,
    /// 未达标 (连续 N 个 Era 投放不足)
    Underdelivery,
    /// 已取消
    Cancelled,
}

impl Default for AdCommitmentStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 动作类型
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ActionType {
    Kick,
    Ban,
    Mute,
    Warn,
    Unmute,
    Unban,
    Promote,
    Demote,
    Welcome,
    ConfigUpdate(ConfigUpdateAction),
}

/// 配置更新动作
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ConfigUpdateAction {
    AddBlacklistWord,
    RemoveBlacklistWord,
    LockType,
    UnlockType,
    SetWelcome,
    SetFloodLimit,
    SetWarnLimit,
    SetWarnAction,
}

/// 节点准入策略
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum NodeRequirement {
    /// 任意节点
    Any,
    /// 仅 TEE 节点
    TeeOnly,
    /// TEE 优先 (有 TEE 时优先调度)
    TeePreferred,
    /// 最低 TEE 节点数
    MinTee(u32),
}

impl Default for NodeRequirement {
    fn default() -> Self {
        Self::TeeOnly
    }
}

/// 警告达限后动作
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum WarnAction {
    Kick,
    Ban,
    Mute,
}

impl Default for WarnAction {
    fn default() -> Self {
        Self::Kick
    }
}

// NOTE: CampaignStatus, AdReviewStatus 已迁移至 pallet-ads-primitives
// 通过顶部 `pub use pallet_ads_primitives::{...}` 重导出, 保持向后兼容

// ============================================================================
// Ceremony Types
// ============================================================================

/// 仪式状态
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum CeremonyStatus {
    Active,
    Superseded { replaced_by: [u8; 32] },
    Revoked { revoked_at: u64 },
    Expired,
}

impl Default for CeremonyStatus {
    fn default() -> Self {
        Self::Active
    }
}

// ============================================================================
// Trait Interfaces
// ============================================================================

/// 社区查询 (ads 等模块依赖 community)
pub trait CommunityProvider {
    /// 社区是否已配置
    fn is_community_configured(community_id_hash: &CommunityIdHash) -> bool;
    /// 社区是否被封禁
    fn is_community_banned(community_id_hash: &CommunityIdHash) -> bool;
    /// 社区是否接受广告投放
    fn is_ads_enabled(community_id_hash: &CommunityIdHash) -> bool;
    /// 社区活跃成员数
    fn active_members(community_id_hash: &CommunityIdHash) -> u32;
    /// 社区语言 (ISO 639-1)
    fn language(community_id_hash: &CommunityIdHash) -> [u8; 2];
}

impl CommunityProvider for () {
    fn is_community_configured(_: &CommunityIdHash) -> bool {
        false
    }
    fn is_community_banned(_: &CommunityIdHash) -> bool {
        false
    }
    fn is_ads_enabled(_: &CommunityIdHash) -> bool {
        false
    }
    fn active_members(_: &CommunityIdHash) -> u32 {
        0
    }
    fn language(_: &CommunityIdHash) -> [u8; 2] {
        *b"en"
    }
}

/// Bot 注册查询 (consensus/ceremony 依赖 registry)
pub trait BotRegistryProvider<AccountId> {
    fn is_bot_active(bot_id_hash: &BotIdHash) -> bool;
    fn is_tee_node(bot_id_hash: &BotIdHash) -> bool;
    fn has_dual_attestation(bot_id_hash: &BotIdHash) -> bool;
    fn is_attestation_fresh(bot_id_hash: &BotIdHash) -> bool;
    fn bot_owner(bot_id_hash: &BotIdHash) -> Option<AccountId>;
    fn bot_public_key(bot_id_hash: &BotIdHash) -> Option<[u8; 32]>;
    /// 获取 Bot 的存活 Peer 数量
    fn peer_count(bot_id_hash: &BotIdHash) -> u32;
    /// 获取 Bot 所属运营商
    fn bot_operator(bot_id_hash: &BotIdHash) -> Option<AccountId>;
    /// 获取 Bot 的精确状态 (Active/Suspended/Deactivated)
    fn bot_status(bot_id_hash: &BotIdHash) -> Option<BotStatus>;
    /// 获取 Bot 的 DCAP 证明级别 (0=无证明, 1-4=DCAP Level)
    fn attestation_level(bot_id_hash: &BotIdHash) -> u8;
    /// 获取 Bot 的 TEE 类型 (None=StandardNode)
    fn tee_type(bot_id_hash: &BotIdHash) -> Option<TeeType>;
}

/// BotRegistryProvider 空实现 (用于不依赖 registry 的测试)
impl<AccountId> BotRegistryProvider<AccountId> for () {
    fn is_bot_active(_: &BotIdHash) -> bool {
        false
    }
    fn is_tee_node(_: &BotIdHash) -> bool {
        false
    }
    fn has_dual_attestation(_: &BotIdHash) -> bool {
        false
    }
    fn is_attestation_fresh(_: &BotIdHash) -> bool {
        false
    }
    fn bot_owner(_: &BotIdHash) -> Option<AccountId> {
        None
    }
    fn bot_public_key(_: &BotIdHash) -> Option<[u8; 32]> {
        None
    }
    fn peer_count(_: &BotIdHash) -> u32 {
        0
    }
    fn bot_operator(_: &BotIdHash) -> Option<AccountId> {
        None
    }
    fn bot_status(_: &BotIdHash) -> Option<BotStatus> {
        None
    }
    fn attestation_level(_: &BotIdHash) -> u8 {
        0
    }
    fn tee_type(_: &BotIdHash) -> Option<TeeType> {
        None
    }
}

/// 广告投放计数查询 (subscription 依赖 ads)
pub trait AdDeliveryProvider {
    /// 查询社区在当前 Era 的广告投放次数
    fn era_delivery_count(community_id_hash: &CommunityIdHash) -> u32;
    /// 重置社区的 Era 投放计数 (Era 结算后调用)
    fn reset_era_deliveries(community_id_hash: &CommunityIdHash);
}

impl AdDeliveryProvider for () {
    fn era_delivery_count(_: &CommunityIdHash) -> u32 {
        0
    }
    fn reset_era_deliveries(_: &CommunityIdHash) {}
}

/// 节点共识查询 (community 可选依赖 consensus)
pub trait NodeConsensusProvider<AccountId> {
    fn is_node_active(node_id: &NodeId) -> bool;
    fn node_operator(node_id: &NodeId) -> Option<AccountId>;
    fn is_tee_node_by_operator(operator: &AccountId) -> bool;
}

impl<AccountId> NodeConsensusProvider<AccountId> for () {
    fn is_node_active(_: &NodeId) -> bool {
        false
    }
    fn node_operator(_: &NodeId) -> Option<AccountId> {
        None
    }
    fn is_tee_node_by_operator(_: &AccountId) -> bool {
        false
    }
}

/// 🆕 10.2/10.4: 订阅查询 trait (为未来 subscription pallet 拆分准备)
///
/// 允许 ads 等 pallet 查询 Bot 的有效订阅层级, 无需直接依赖 consensus pallet.
pub trait SubscriptionProvider {
    /// 查询 Bot 的有效订阅层级 (无订阅 = Free)
    fn effective_tier(bot_id_hash: &BotIdHash) -> SubscriptionTier;
    /// 查询 Bot 的功能限制
    fn effective_feature_gate(bot_id_hash: &BotIdHash) -> TierFeatureGate;
    /// 查询 Bot 是否有活跃的付费订阅 (Active/PastDue/Paused 均算)
    fn is_subscription_active(bot_id_hash: &BotIdHash) -> bool;
    /// 查询 Bot 的订阅状态
    fn subscription_status(bot_id_hash: &BotIdHash) -> Option<SubscriptionStatus>;
}

impl SubscriptionProvider for () {
    fn effective_tier(_: &BotIdHash) -> SubscriptionTier {
        SubscriptionTier::Free
    }
    fn effective_feature_gate(_: &BotIdHash) -> TierFeatureGate {
        SubscriptionTier::Free.feature_gate()
    }
    fn is_subscription_active(_: &BotIdHash) -> bool {
        false
    }
    fn subscription_status(_: &BotIdHash) -> Option<SubscriptionStatus> {
        None
    }
}

/// 🆕 10.4: 统一奖励写入 trait (为未来 rewards pallet 拆分准备)
///
/// ads 和 subscription 都通过此 trait 向同一奖励池写入节点奖励,
/// 节点只需调用一次 claim 即可领取全部来源的奖励.
pub trait RewardAccruer {
    /// 向节点累加待领取奖励
    fn accrue_node_reward(node_id: &NodeId, amount: u128);
}

impl RewardAccruer for () {
    fn accrue_node_reward(_: &NodeId, _: u128) {}
}

/// Peer Uptime 记录 trait (consensus on_era_end 调用 registry pallet)
pub trait PeerUptimeRecorder {
    /// Era 结束时快照心跳计数并清理过期历史
    ///
    /// - `era`: 刚结束的 Era 编号
    fn record_era_uptime(era: u64);
}

impl PeerUptimeRecorder for () {
    fn record_era_uptime(_: u64) {}
}

/// Era 订阅结算结果
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Default,
)]
pub struct EraSettlementResult {
    /// 本次结算收取的总收入
    pub total_income: u128,
    /// 运营者分成 (通常 90%)
    pub node_share: u128,
    /// 实际转入国库的金额
    pub treasury_share: u128,
}

/// 订阅结算 trait (consensus on_era_end 调用 subscription pallet)
pub trait SubscriptionSettler {
    /// 结算当前 Era 的订阅费, 返回结算结果
    ///
    /// 90% node_share 已由 subscription pallet 直接转给 Bot 运营者,
    /// 10% treasury_share 已转入国库, 不再进入 RewardPool 参与权重分配。
    fn settle_era() -> EraSettlementResult;
}

impl SubscriptionSettler for () {
    fn settle_era() -> EraSettlementResult {
        EraSettlementResult::default()
    }
}

/// H3-fix: 节点退出时领取残留奖励 (consensus finalize_exit 调用 rewards pallet)
pub trait OrphanRewardClaimer<AccountId> {
    /// 尝试将节点残留奖励转给 operator (best-effort, 失败不阻断退出)
    fn try_claim_orphan_rewards(node_id: &NodeId, operator: &AccountId);
}

impl<AccountId> OrphanRewardClaimer<AccountId> for () {
    fn try_claim_orphan_rewards(_: &NodeId, _: &AccountId) {}
}

/// Era reward distribution trait (called by consensus pallet's on_era_end).
/// Era 奖励分配 trait（由 consensus pallet 的 on_era_end 调用）。
pub trait EraRewardDistributor {
    /// Distribute rewards to nodes and record Era info.
    /// 向节点分配奖励并记录 Era 信息。
    ///
    /// - `era`: current Era number / 当前 Era 编号
    /// - `total_pool`: distributable total (from RewardPool balance) / 可分配总额（来自 RewardPool 余额）
    /// - `subscription_income`: subscription income this era / 本期订阅收入
    /// - `ads_income`: ads income this era / 本期广告收入
    /// - `inflation`: inflation mint this era (0 = no minting) / 本期通胀铸币（0 = 不铸币）
    /// - `treasury_share`: treasury share / 国库分成
    /// - `node_weights`: per-node weights (node_id, weight) / 各节点权重
    /// - `node_count`: active node count / 活跃节点数
    ///
    /// Returns the total amount actually distributed.
    /// 返回实际分配的总额。
    fn distribute_and_record(
        era: u64,
        total_pool: u128,
        subscription_income: u128,
        ads_income: u128,
        inflation: u128,
        treasury_share: u128,
        node_weights: &[(NodeId, u128)],
        node_count: u32,
    ) -> u128;

    /// Prune expired Era reward records.
    /// 清理过期 Era 奖励记录。
    fn prune_old_eras(current_era: u64);

    /// Query the current RewardPool balance available for distribution.
    /// 查询当前 RewardPool 可分配余额。
    fn reward_pool_balance() -> u128;
}

impl EraRewardDistributor for () {
    fn distribute_and_record(
        _: u64,
        _: u128,
        _: u128,
        _: u128,
        _: u128,
        _: u128,
        _: &[(NodeId, u128)],
        _: u32,
    ) -> u128 {
        0
    }
    fn prune_old_eras(_: u64) {}
    fn reward_pool_balance() -> u128 {
        0
    }
}
