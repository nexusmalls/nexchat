//! Mainnet retirement of the prediction-market pallet namespace.
//! 主网退役预测市场 pallet 命名空间。
//!
//! The Phase 6 wiring stayed globally Disabled with no modules or collateral
//! enabled, so leftover prefix keys are wiped by `RemovePallet` without a
//! fund-refund pass.
//! Phase 6 接线保持全局 Disabled，且未启用任何模块或抵押，故无需退款，
//! 剩余键由 `RemovePallet` 清除。
//!
//! Indexes 176–193 stay retired and must not be reused.
//! 索引 176–193 永久退役，禁止复用。

use frame_support::{parameter_types, weights::constants::RocksDbWeight};

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
