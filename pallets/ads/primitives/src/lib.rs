#![cfg_attr(not(feature = "std"), no_std)]

//! # Ads Primitives — Shared ad-system types and trait interfaces.
//! # Ads Primitives — 通用广告系统共享类型与 Trait 接口。
//!
//! This crate is used by all ads pallets, including core, grouprobot, and entity.
//! 所有 ads 子 pallet（core、grouprobot、entity 等）都依赖此 crate。
//! It contains no storage or extrinsics and only defines shared types and traits.
//! 无 Storage、无 Extrinsic，纯类型与 Trait 定义。
//!
//! ## Design goals
//! ## 设计目标
//! Separate common advertising concepts (campaign status, review status, preferences)
//! from domain-specific concepts (GroupRobot TEE nodes, Entity shops) so the core
//! ads engine can be reused across adapters.
//! 将广告系统的通用概念（Campaign 状态、审核状态、偏好）与领域特定概念
//! （GroupRobot 的 TEE 节点、Entity 的 Shop）分离，使核心广告引擎可跨适配层复用。

use codec::{Decode, Encode, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;

// ============================================================================
// Type Aliases
// ============================================================================

/// 广告位 ID (32 bytes) — 泛化标识，对应 GroupRobot 的 CommunityIdHash 或 Entity 的 EntityId
pub type PlacementId = [u8; 32];

// ============================================================================
// Enums — 通用广告状态
// ============================================================================

/// 广告活动状态
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
pub enum CampaignStatus {
    Active,
    Paused,
    /// 预算耗尽
    Exhausted,
    Expired,
    Cancelled,
    /// 治理暂停 (可恢复)
    Suspended,
    /// 审核中 (新建或修改后待审核)
    UnderReview,
}

impl Default for CampaignStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// 广告审核状态
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
pub enum AdReviewStatus {
    Pending,
    Approved,
    Rejected,
    /// 举报
    Flagged,
}

impl Default for AdReviewStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// 广告活动类型
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
pub enum CampaignType {
    /// CPM — 按展示量计费
    Cpm,
    /// CPC — 按点击计费
    Cpc,
    /// 固定费用 (包时段/包位)
    Fixed,
    /// 私有广告 (仅指定广告位可见)
    Private,
}

impl Default for CampaignType {
    fn default() -> Self {
        Self::Cpm
    }
}

/// 广告位状态 (由适配层报告)
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
pub enum PlacementStatus {
    /// 正常接收广告
    Active,
    /// 管理员/Owner 主动暂停
    Paused,
    /// 被永久禁止
    Banned,
    /// 未注册或不存在
    Unknown,
}

impl Default for PlacementStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

// ============================================================================
// Click Attestation — C2b Proxy Account 点击证明
// ============================================================================

/// Click attestation signed by a user's proxy account.
/// 点击证明——由用户的 Proxy Account 签名的点击事件。
///
/// In the C2b model, the user's main account delegates limited signing authority
/// to a DApp-managed proxy account through `proxy.addProxy`.
/// C2b 方案中，用户主账户通过 `proxy.addProxy` 委托有限签名权给 DApp 管理的 proxy 账户。
/// When the user clicks an ad, the DApp signs a `ClickAttestation` with the proxy account.
/// 用户点击广告时，DApp 自动使用 proxy 账户签名生成 `ClickAttestation`。
/// Entity can then aggregate attestations and submit them on-chain in batches.
/// Entity 将批量 attestation 聚合后提交上链。
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
pub struct ClickAttestation<AccountId> {
    /// 点击者 (用户主账户)
    pub clicker: AccountId,
    /// 签名者 (proxy 账户, 已被 clicker 授权)
    pub proxy: AccountId,
    /// 点击的 Campaign ID
    pub campaign_id: u64,
    /// 广告位 ID
    pub placement_id: PlacementId,
    /// 点击时间戳 (区块号)
    pub clicked_at: u64,
}

// ============================================================================
// Trait Interfaces — 适配层需实现
// ============================================================================

/// Delivery receipt verification interface implemented by each adapter layer.
/// 投放收据验证——各适配层实现投放真实性验证。
///
/// - GroupRobot: TEE node signature verification plus subscription-tier gating.
/// - GroupRobot：TEE 节点签名验证 + 订阅层级门控。
/// - Entity: entity active-state checks plus audience validation.
/// - Entity：Entity 活跃状态检查 + 展示量验证。
pub trait DeliveryVerifier<AccountId> {
    /// 验证投放收据的合法性
    ///
    /// - `who`: 提交者
    /// - `placement_id`: 广告位
    /// - `audience_size`: 受众规模
    /// - `node_id`: 投放节点 ID (GroupRobot: TEE 节点; Entity: None)
    ///
    /// 返回: 验证通过后的有效受众数 (可能被裁切)
    fn verify_and_cap_audience(
        who: &AccountId,
        placement_id: &PlacementId,
        audience_size: u32,
        node_id: Option<[u8; 32]>,
    ) -> Result<u32, sp_runtime::DispatchError>;
}

/// DeliveryVerifier 空实现 (直通, 用于测试)
/// 返回原始 audience_size 不做裁切。
impl<AccountId> DeliveryVerifier<AccountId> for () {
    fn verify_and_cap_audience(
        _: &AccountId,
        _: &PlacementId,
        audience_size: u32,
        _node_id: Option<[u8; 32]>,
    ) -> Result<u32, sp_runtime::DispatchError> {
        Ok(audience_size)
    }
}

/// Click verification interface implemented by adapter layers, including authenticity
/// checks and daily caps.
/// 点击收据验证——各适配层实现点击真实性验证与每日上限。
///
/// - Entity: entity active-state checks, daily click limits, and permission validation.
/// - Entity：Entity 活跃状态检查 + 每日点击量上限 + 权限验证。
pub trait ClickVerifier<AccountId> {
    /// 验证点击收据的合法性
    ///
    /// - `who`: 提交者 (Entity owner/admin/shop manager)
    /// - `placement_id`: 广告位
    /// - `click_count`: 本次提交的点击数
    /// - `verified_clicks`: 经 proxy 签名验证的点击数 (C2b)
    ///
    /// 返回: 验证通过后的有效点击数 (可能被每日上限裁切)
    fn verify_and_cap_clicks(
        who: &AccountId,
        placement_id: &PlacementId,
        click_count: u32,
        verified_clicks: u32,
    ) -> Result<u32, sp_runtime::DispatchError>;
}

/// 路由层错误 — 适配层不支持的操作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdsRouterError {
    /// 该广告位路径不支持 CPC (点击计费) 模式
    CpcNotSupportedForPath,
}

impl From<AdsRouterError> for sp_runtime::DispatchError {
    fn from(e: AdsRouterError) -> sp_runtime::DispatchError {
        match e {
            AdsRouterError::CpcNotSupportedForPath => {
                sp_runtime::DispatchError::Other("CPC not supported for this placement path")
            }
        }
    }
}

/// ClickVerifier 空实现 (直通, 用于测试)
impl<AccountId> ClickVerifier<AccountId> for () {
    fn verify_and_cap_clicks(
        _: &AccountId,
        _: &PlacementId,
        click_count: u32,
        _verified_clicks: u32,
    ) -> Result<u32, sp_runtime::DispatchError> {
        Ok(click_count)
    }
}

/// Placement-admin resolver implemented by each adapter layer.
/// 广告位管理员解析——各适配层映射广告位到管理员。
///
/// - GroupRobot: `CommunityAdmin` (first staker / bot owner).
/// - GroupRobot：`CommunityAdmin`（首个质押者 / Bot Owner）。
/// - Entity: entity owner / shop admin.
/// - Entity：Entity Owner / Shop Admin。
pub trait PlacementAdminProvider<AccountId> {
    /// 查询广告位管理员
    fn placement_admin(placement_id: &PlacementId) -> Option<AccountId>;
    /// 广告位是否被永久禁止
    fn is_placement_banned(placement_id: &PlacementId) -> bool;
    /// 查询广告位当前状态 (Active/Paused/Banned/Unknown)
    fn placement_status(placement_id: &PlacementId) -> PlacementStatus;
}

impl<AccountId> PlacementAdminProvider<AccountId> for () {
    fn placement_admin(_: &PlacementId) -> Option<AccountId> {
        None
    }
    fn is_placement_banned(_: &PlacementId) -> bool {
        false
    }
    fn placement_status(_: &PlacementId) -> PlacementStatus {
        PlacementStatus::Unknown
    }
}

/// Revenue breakdown.
/// 收入分配明细。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevenueBreakdown<Balance> {
    /// Withdrawable share for the placement side (community / entity owner).
    /// 广告位方可提取份额（社区 / Entity Owner）
    pub placement_share: Balance,
    /// Node share (`GroupRobot`: TEE node; `Entity`: 0).
    /// 节点份额（`GroupRobot`：TEE 节点；`Entity`：0）
    pub node_share: Balance,
    /// Platform or treasury share.
    /// 平台 / 国库份额
    pub platform_share: Balance,
}

/// Post-settlement revenue distribution strategy defined by each adapter layer.
/// 结算后的收入分配策略——各适配层定义分成比例和分配逻辑。
///
/// - GroupRobot: three-way split across community, treasury, and node.
/// - GroupRobot：三方分成（社区 / 国库 / 节点）。
/// - Entity: two-way split across entity owner and platform.
/// - Entity：二方分成（Entity Owner / 平台）。
pub trait RevenueDistributor<AccountId, Balance> {
    /// 分配广告收入
    ///
    /// - `placement_id`: 广告位
    /// - `total_cost`: 本次总费用
    /// - `advertiser`: 广告主账户
    ///
    /// 返回: 收入分配明细 (RevenueBreakdown)
    fn distribute(
        placement_id: &PlacementId,
        total_cost: Balance,
        advertiser: &AccountId,
    ) -> Result<RevenueBreakdown<Balance>, sp_runtime::DispatchError>;
}

/// `()` 空实现: 广告位方份额为零 (全部收入归国库)。
/// 生产环境必须提供领域适配层实现。
impl<AccountId, Balance: Default> RevenueDistributor<AccountId, Balance> for () {
    fn distribute(
        _: &PlacementId,
        total_cost: Balance,
        _: &AccountId,
    ) -> Result<RevenueBreakdown<Balance>, sp_runtime::DispatchError> {
        Ok(RevenueBreakdown {
            placement_share: Balance::default(),
            node_share: Balance::default(),
            platform_share: total_cost,
        })
    }
}

/// 广告排期查询 (其他 pallet 查询广告状态)
pub trait AdScheduleProvider {
    /// 广告位是否启用广告
    fn is_ads_enabled(placement_id: &PlacementId) -> bool;
    /// 广告位累计广告收入 (Balance 用 u128 表示)
    fn placement_ad_revenue(placement_id: &PlacementId) -> u128;
    /// 广告位当前 Era 广告收入 (Balance 用 u128 表示)
    fn placement_era_revenue(placement_id: &PlacementId) -> u128;
}

impl AdScheduleProvider for () {
    fn is_ads_enabled(_: &PlacementId) -> bool {
        false
    }
    fn placement_ad_revenue(_: &PlacementId) -> u128 {
        0
    }
    fn placement_era_revenue(_: &PlacementId) -> u128 {
        0
    }
}

/// 广告投放计数查询 (外部 pallet 查询投放达标情况)
pub trait AdDeliveryCountProvider {
    /// 查询广告位在当前 Era 的广告投放次数
    fn era_delivery_count(placement_id: &PlacementId) -> u32;
    /// 重置广告位的 Era 投放计数 (Era 结算后调用)
    fn reset_era_deliveries(placement_id: &PlacementId);
}

impl AdDeliveryCountProvider for () {
    fn era_delivery_count(_: &PlacementId) -> u32 {
        0
    }
    fn reset_era_deliveries(_: &PlacementId) {}
}

/// 广告策略查询 — 治理层或适配层提供的广告投放策略参数
///
/// 用于限制广告位的投放行为 (每广告位最大活动数、最低预算、审核策略等)。
pub trait AdPolicyProvider {
    /// 广告位允许的最大并发活动数 (0 = 无限制)
    fn max_campaigns_per_placement(placement_id: &PlacementId) -> u32;
    /// 创建活动的最低预算 (u128, 0 = 无门槛)
    fn min_campaign_budget(placement_id: &PlacementId) -> u128;
    /// 新活动是否需要审核
    fn requires_review(placement_id: &PlacementId) -> bool;
}

impl AdPolicyProvider for () {
    fn max_campaigns_per_placement(_: &PlacementId) -> u32 {
        0
    }
    fn min_campaign_budget(_: &PlacementId) -> u128 {
        0
    }
    fn requires_review(_: &PlacementId) -> bool {
        false
    }
}

/// 广告位配置查询 — 适配层提供的广告位级别参数
///
/// GroupRobot: 质押量 → 受众上限映射
/// Entity: 广告位级别 → 每日展示量上限
pub trait PlacementConfigProvider {
    /// 广告位每日展示量上限 (0 = 无限制)
    fn daily_impression_cap(placement_id: &PlacementId) -> u32;
    /// 广告位收入分成比例 (基点, 10000 = 100%)
    fn revenue_share_bps(placement_id: &PlacementId) -> u32;
    /// 广告位是否支持私有广告
    fn supports_private_ads(placement_id: &PlacementId) -> bool;
}

impl PlacementConfigProvider for () {
    fn daily_impression_cap(_: &PlacementId) -> u32 {
        0
    }
    fn revenue_share_bps(_: &PlacementId) -> u32 {
        0
    }
    fn supports_private_ads(_: &PlacementId) -> bool {
        false
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use codec::{Decode, Encode};

    fn roundtrip<T: Encode + Decode + PartialEq + core::fmt::Debug>(val: T) {
        let encoded = val.encode();
        let decoded = T::decode(&mut &encoded[..]).expect("decode failed");
        assert_eq!(val, decoded);
    }

    #[test]
    fn campaign_status_codec_roundtrip() {
        let variants = [
            CampaignStatus::Active,
            CampaignStatus::Paused,
            CampaignStatus::Exhausted,
            CampaignStatus::Expired,
            CampaignStatus::Cancelled,
            CampaignStatus::Suspended,
            CampaignStatus::UnderReview,
        ];
        for v in variants {
            roundtrip(v);
        }
    }

    #[test]
    fn campaign_status_default_is_active() {
        assert_eq!(CampaignStatus::default(), CampaignStatus::Active);
    }

    #[test]
    fn ad_review_status_codec_roundtrip() {
        let variants = [
            AdReviewStatus::Pending,
            AdReviewStatus::Approved,
            AdReviewStatus::Rejected,
            AdReviewStatus::Flagged,
        ];
        for v in variants {
            roundtrip(v);
        }
    }

    #[test]
    fn ad_review_status_default_is_pending() {
        assert_eq!(AdReviewStatus::default(), AdReviewStatus::Pending);
    }

    #[test]
    fn campaign_type_codec_roundtrip() {
        let variants = [
            CampaignType::Cpm,
            CampaignType::Cpc,
            CampaignType::Fixed,
            CampaignType::Private,
        ];
        for v in variants {
            roundtrip(v);
        }
    }

    #[test]
    fn campaign_type_default_is_cpm() {
        assert_eq!(CampaignType::default(), CampaignType::Cpm);
    }

    #[test]
    fn placement_status_codec_roundtrip() {
        let variants = [
            PlacementStatus::Active,
            PlacementStatus::Paused,
            PlacementStatus::Banned,
            PlacementStatus::Unknown,
        ];
        for v in variants {
            roundtrip(v);
        }
    }

    #[test]
    fn placement_status_default_is_unknown() {
        assert_eq!(PlacementStatus::default(), PlacementStatus::Unknown);
    }

    #[test]
    fn click_attestation_codec_roundtrip() {
        let att = ClickAttestation::<u64> {
            clicker: 1u64,
            proxy: 2u64,
            campaign_id: 42,
            placement_id: [0xAB; 32],
            clicked_at: 100,
        };
        roundtrip(att);
    }

    #[test]
    fn enum_discriminant_stability() {
        assert_eq!(CampaignStatus::Active.encode(), [0]);
        assert_eq!(CampaignStatus::Paused.encode(), [1]);
        assert_eq!(CampaignStatus::Exhausted.encode(), [2]);
        assert_eq!(CampaignStatus::Expired.encode(), [3]);
        assert_eq!(CampaignStatus::Cancelled.encode(), [4]);
        assert_eq!(CampaignStatus::Suspended.encode(), [5]);
        assert_eq!(CampaignStatus::UnderReview.encode(), [6]);

        assert_eq!(AdReviewStatus::Pending.encode(), [0]);
        assert_eq!(AdReviewStatus::Approved.encode(), [1]);
        assert_eq!(AdReviewStatus::Rejected.encode(), [2]);
        assert_eq!(AdReviewStatus::Flagged.encode(), [3]);

        assert_eq!(CampaignType::Cpm.encode(), [0]);
        assert_eq!(CampaignType::Cpc.encode(), [1]);
        assert_eq!(CampaignType::Fixed.encode(), [2]);
        assert_eq!(CampaignType::Private.encode(), [3]);

        assert_eq!(PlacementStatus::Active.encode(), [0]);
        assert_eq!(PlacementStatus::Paused.encode(), [1]);
        assert_eq!(PlacementStatus::Banned.encode(), [2]);
        assert_eq!(PlacementStatus::Unknown.encode(), [3]);
    }

    #[test]
    fn invalid_discriminant_decode_fails() {
        assert!(CampaignStatus::decode(&mut &[7u8][..]).is_err());
        assert!(AdReviewStatus::decode(&mut &[4u8][..]).is_err());
        assert!(CampaignType::decode(&mut &[4u8][..]).is_err());
        assert!(PlacementStatus::decode(&mut &[4u8][..]).is_err());
    }

    #[test]
    fn unit_delivery_verifier_passthrough() {
        let result =
            <() as DeliveryVerifier<u64>>::verify_and_cap_audience(&1u64, &[0u8; 32], 500, None);
        assert_eq!(result.unwrap(), 500);
    }

    #[test]
    fn unit_click_verifier_passthrough() {
        let result = <() as ClickVerifier<u64>>::verify_and_cap_clicks(&1u64, &[0u8; 32], 100, 80);
        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn unit_placement_admin_provider_defaults() {
        assert_eq!(
            <() as PlacementAdminProvider<u64>>::placement_admin(&[0u8; 32]),
            None
        );
        assert!(!<() as PlacementAdminProvider<u64>>::is_placement_banned(
            &[0u8; 32]
        ));
        assert_eq!(
            <() as PlacementAdminProvider<u64>>::placement_status(&[0u8; 32]),
            PlacementStatus::Unknown,
        );
    }

    #[test]
    fn unit_revenue_distributor_all_to_platform() {
        let result =
            <() as RevenueDistributor<u64, u128>>::distribute(&[0u8; 32], 1000u128, &1u64).unwrap();
        assert_eq!(result.placement_share, 0);
        assert_eq!(result.node_share, 0);
        assert_eq!(result.platform_share, 1000);
    }

    #[test]
    fn unit_ad_schedule_provider_defaults() {
        assert!(!<() as AdScheduleProvider>::is_ads_enabled(&[0u8; 32]));
        assert_eq!(
            <() as AdScheduleProvider>::placement_ad_revenue(&[0u8; 32]),
            0
        );
        assert_eq!(
            <() as AdScheduleProvider>::placement_era_revenue(&[0u8; 32]),
            0
        );
    }

    #[test]
    fn unit_ad_delivery_count_provider_defaults() {
        assert_eq!(
            <() as AdDeliveryCountProvider>::era_delivery_count(&[0u8; 32]),
            0
        );
        <() as AdDeliveryCountProvider>::reset_era_deliveries(&[0u8; 32]);
    }

    #[test]
    fn unit_ad_policy_provider_defaults() {
        assert_eq!(
            <() as AdPolicyProvider>::max_campaigns_per_placement(&[0u8; 32]),
            0
        );
        assert_eq!(<() as AdPolicyProvider>::min_campaign_budget(&[0u8; 32]), 0);
        assert!(!<() as AdPolicyProvider>::requires_review(&[0u8; 32]));
    }

    #[test]
    fn unit_placement_config_provider_defaults() {
        assert_eq!(
            <() as PlacementConfigProvider>::daily_impression_cap(&[0u8; 32]),
            0
        );
        assert_eq!(
            <() as PlacementConfigProvider>::revenue_share_bps(&[0u8; 32]),
            0
        );
        assert!(!<() as PlacementConfigProvider>::supports_private_ads(
            &[0u8; 32]
        ));
    }

    #[test]
    fn ads_router_error_into_dispatch_error() {
        let err: sp_runtime::DispatchError = AdsRouterError::CpcNotSupportedForPath.into();
        assert_eq!(
            err,
            sp_runtime::DispatchError::Other("CPC not supported for this placement path")
        );
    }
}
