// Copyright 2024-2025 Forecasting Technologies LTD.
// Copyright 2021-2022 Zeitgeist PM LLC.
//
// This file is part of Zeitgeist.
//
// Zeitgeist is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at
// your option) any later version.
//
// Zeitgeist is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Zeitgeist. If not, see <https://www.gnu.org/licenses/>.

//! Read-only runtime API types for prediction market outcome assets.
//! 预测市场 outcome 资产的只读 runtime API 类型。

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Codec, Decode, Encode, MaxEncodedLen};
use zeitgeist_primitives::types::{Asset, SerdeWrapper};

/// Compact market period returned by the Nexus prediction view API.
/// Nexus 预测视图 API 返回的紧凑市场周期。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub enum MarketPeriodView {
    /// Exclusive block-number range. / 区块号左闭右开范围。
    Block { start: u32, end: u32 },
    /// Exclusive timestamp range in milliseconds. / 毫秒时间戳左闭右开范围。
    Timestamp { start: u64, end: u64 },
}

/// Outcome value returned by the Nexus prediction view API.
/// Nexus 预测视图 API 返回的结果值。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub enum OutcomeView {
    /// Winning category index. / 获胜类别索引。
    Categorical(u16),
    /// Resolved scalar value. / 已决议标量值。
    Scalar(SerdeWrapper<u128>),
}

/// Public report details for one prediction market.
/// 单个预测市场的公开报告详情。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct ReportView<AccountId> {
    /// Block at which the report was submitted. / 报告提交区块。
    pub at: u32,
    /// Reporter account. / 报告账户。
    pub by: AccountId,
    /// Reported outcome. / 报告结果。
    pub outcome: OutcomeView,
}

/// Bounded, client-oriented summary of one prediction market.
/// 面向客户端的单个预测市场有界摘要。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct MarketSummary<AccountId> {
    /// Stable market identifier. / 稳定市场标识。
    pub market_id: SerdeWrapper<u128>,
    /// Collateral asset. / 抵押资产。
    pub base_asset: Asset<SerdeWrapper<u128>>,
    /// Market creator. / 市场创建者。
    pub creator: AccountId,
    /// Designated oracle. / 指定预言机。
    pub oracle: AccountId,
    /// Status tag: proposed=0 through resolved=5. / 状态标签：proposed=0 至 resolved=5。
    pub status: u8,
    /// Scoring tag: hybrid=0, parimutuel=1. / 计分标签：hybrid=0、parimutuel=1。
    pub scoring_rule: u8,
    /// Type tag: categorical=0, scalar=1. / 类型标签：categorical=0、scalar=1。
    pub market_type: u8,
    /// Inclusive scalar lower bound. / 标量闭区间下界。
    pub scalar_low: Option<SerdeWrapper<u128>>,
    /// Inclusive scalar upper bound. / 标量闭区间上界。
    pub scalar_high: Option<SerdeWrapper<u128>>,
    /// Trading period. / 交易周期。
    pub period: MarketPeriodView,
    /// Grace-period duration in blocks. / 宽限期区块数。
    pub grace_period: u32,
    /// Oracle-report duration in blocks. / 预言机报告期区块数。
    pub oracle_duration: u32,
    /// Dispute duration in blocks. / 争议期区块数。
    pub dispute_duration: u32,
    /// Outcome or parimutuel-share assets. / 结果或彩池份额资产。
    pub outcome_assets: Vec<Asset<SerdeWrapper<u128>>>,
    /// Submitted report, if any. / 已提交报告（如有）。
    pub report: Option<ReportView<AccountId>>,
    /// Final outcome, if resolved. / 最终结果（如已决议）。
    pub resolved_outcome: Option<OutcomeView>,
    /// Dispute tag: authorized=0, court=1. / 争议标签：authorized=0、court=1。
    pub dispute_mechanism: Option<u8>,
    /// Active canonical Neo Swaps pool. / 活跃的标准 Neo Swaps 池。
    pub canonical_pool: Option<SerdeWrapper<u128>>,
}

/// One canonical-pool spot price.
/// 一个标准池现货价格。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct SpotPriceView {
    /// Priced outcome asset. / 被定价的结果资产。
    pub asset: Asset<SerdeWrapper<u128>>,
    /// BASE-scaled spot price. / 以 BASE 缩放的现货价格。
    pub price: SerdeWrapper<u128>,
}

/// Public, non-juror-sensitive summary of a Court case.
/// 不包含陪审员敏感信息的 Court 案件公开摘要。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct CourtCaseSummary {
    /// Court case identifier. / Court 案件标识。
    pub court_id: SerdeWrapper<u128>,
    /// Status tag: open=0, closed=1, reassigned=2. / 状态标签：open=0、closed=1、reassigned=2。
    pub status: u8,
    /// Number of submitted appeals. / 已提交申诉数。
    pub appeals: u32,
    /// End block of pre-vote phase. / 预投票阶段结束区块。
    pub pre_vote_end: u32,
    /// End block of vote phase. / 投票阶段结束区块。
    pub vote_end: u32,
    /// End block of aggregation phase. / 聚合阶段结束区块。
    pub aggregation_end: u32,
    /// End block of appeal phase. / 申诉阶段结束区块。
    pub appeal_end: u32,
}

/// Current accounting and admission state of one mirrored collateral asset.
/// 单个镜像抵押资产当前的记账与准入状态。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct CollateralMirrorStatus {
    /// `pallet-assets` asset identifier. / `pallet-assets` 资产标识。
    pub asset_id: u64,
    /// Governance whitelist state. / 治理白名单状态。
    pub whitelisted: bool,
    /// Global new-deposit pause state. / 全局新增存入暂停状态。
    pub global_deposit_paused: bool,
    /// Per-asset new-deposit pause state. / 逐资产新增存入暂停状态。
    pub asset_deposit_paused: bool,
    /// Combined live deposit-admission result. / 实时存入准入综合结果。
    pub deposit_allowed: bool,
    /// Total ORML mirror issuance. / ORML 镜像总发行量。
    pub mirror_issuance: SerdeWrapper<u128>,
    /// Real collateral held in escrow. / 托管的真实抵押余额。
    pub escrow_balance: SerdeWrapper<u128>,
    /// Whether issuance equals escrow. / 发行量是否等于托管余额。
    pub consistent: bool,
}

/// Global prediction mode and the enabled-module bit set.
/// 预测全局模式与已启用模块位集合。
#[derive(Clone, Decode, Encode, Eq, PartialEq, scale_info::TypeInfo)]
#[cfg_attr(feature = "std", derive(Debug, serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(rename_all = "camelCase"))]
pub struct PredictionControlStatus {
    /// Mode tag: disabled=0 through full=3. / 模式标签：disabled=0 至 full=3。
    pub mode: u8,
    /// Bit set ordered by `PredictionModule`. / 按 `PredictionModule` 顺序排列的位集合。
    pub enabled_modules: u16,
}

sp_api::decl_runtime_apis! {
    /// Exposes deterministic prediction-market asset identifiers to clients.
    /// 向客户端公开确定性的预测市场资产标识。
    pub trait PredictionMarketsApi<MarketId, Hash> where
        MarketId: Codec + MaxEncodedLen,
        Hash: Codec,
    {
        /// Returns the asset identifier for one market outcome.
        /// 返回指定市场 outcome 的资产标识。
        fn market_outcome_share_id(market_id: MarketId, outcome: u16) -> Asset<MarketId>;
    }

    /// Exposes bounded, read-only Nexus prediction views without storage iteration.
    /// 公开有界、只读且不遍历 storage 的 Nexus 预测视图。
    pub trait PredictionViewApi<AccountId> where
        AccountId: Codec,
    {
        /// Returns one market's summary, outcomes, deadlines, and resolution state.
        /// 返回单个市场的摘要、结果资产、期限与决议状态。
        fn market_summary(market_id: u128) -> Option<MarketSummary<AccountId>>;

        /// Returns canonical Neo Swaps spot prices for one market.
        /// 返回单个市场标准 Neo Swaps 池的现货价格。
        fn spot_prices(market_id: u128) -> Vec<SpotPriceView>;

        /// Returns the amount currently redeemable by one account.
        /// 返回指定账户当前可赎回的抵押金额。
        fn user_redeemable(market_id: u128, account: AccountId) -> SerdeWrapper<u128>;

        /// Returns the public Court summary associated with one market.
        /// 返回与市场关联的 Court 公开摘要。
        fn court_summary(market_id: u128) -> Option<CourtCaseSummary>;

        /// Returns accounting and admission state for one mirrored collateral asset.
        /// 返回单个镜像抵押资产的记账与准入状态。
        fn collateral_mirror_status(asset_id: u64) -> CollateralMirrorStatus;

        /// Returns the global prediction mode and enabled modules.
        /// 返回预测全局模式与已启用模块。
        fn control_status() -> PredictionControlStatus;
    }
}
