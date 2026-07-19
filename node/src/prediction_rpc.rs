//! Read-only JSON-RPC facade for Nexus prediction views.
//! Nexus 预测视图的只读 JSON-RPC 门面。

use std::{marker::PhantomData, sync::Arc};

use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{ErrorObject, ErrorObjectOwned},
};
use nexus_runtime::AccountId;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use zeitgeist_primitives::types::SerdeWrapper;
use zrml_prediction_markets_runtime_api::{
    CollateralMirrorStatus, CourtCaseSummary, MarketSummary, PredictionControlStatus,
    PredictionViewApi as PredictionViewRuntimeApi, SpotPriceView,
};

/// Named prediction queries backed exclusively by the runtime API.
/// 完全由 runtime API 支持的具名预测查询。
#[rpc(client, server)]
pub trait PredictionApi<BlockHash> {
    #[method(name = "prediction_marketSummary")]
    fn market_summary(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<BlockHash>,
    ) -> RpcResult<Option<MarketSummary<AccountId>>>;

    #[method(name = "prediction_spotPrices")]
    fn spot_prices(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<BlockHash>,
    ) -> RpcResult<Vec<SpotPriceView>>;

    #[method(name = "prediction_userRedeemable")]
    fn user_redeemable(
        &self,
        market_id: SerdeWrapper<u128>,
        account: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<SerdeWrapper<u128>>;

    #[method(name = "prediction_courtSummary")]
    fn court_summary(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<BlockHash>,
    ) -> RpcResult<Option<CourtCaseSummary>>;

    #[method(name = "prediction_collateralMirrorStatus")]
    fn collateral_mirror_status(
        &self,
        asset_id: u64,
        at: Option<BlockHash>,
    ) -> RpcResult<CollateralMirrorStatus>;

    #[method(name = "prediction_controlStatus")]
    fn control_status(&self, at: Option<BlockHash>) -> RpcResult<PredictionControlStatus>;
}

/// Prediction RPC handler backed by a full client.
/// 由 full client 支持的预测 RPC 处理器。
pub struct Prediction<C, B> {
    client: Arc<C>,
    _marker: PhantomData<B>,
}

impl<C, B> Prediction<C, B> {
    /// Creates a prediction RPC handler.
    /// 创建预测 RPC 处理器。
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: PhantomData,
        }
    }
}

fn runtime_err(error: impl core::fmt::Display) -> ErrorObjectOwned {
    ErrorObject::owned(1, "Prediction runtime API error", Some(error.to_string()))
}

impl<C, Block> PredictionApiServer<<Block as BlockT>::Hash> for Prediction<C, Block>
where
    Block: BlockT,
    C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
    C::Api: PredictionViewRuntimeApi<Block, AccountId>,
{
    fn market_summary(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Option<MarketSummary<AccountId>>> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .market_summary(at, market_id.0)
            .map_err(runtime_err)
    }

    fn spot_prices(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Vec<SpotPriceView>> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .spot_prices(at, market_id.0)
            .map_err(runtime_err)
    }

    fn user_redeemable(
        &self,
        market_id: SerdeWrapper<u128>,
        account: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<SerdeWrapper<u128>> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .user_redeemable(at, market_id.0, account)
            .map_err(runtime_err)
    }

    fn court_summary(
        &self,
        market_id: SerdeWrapper<u128>,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Option<CourtCaseSummary>> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .court_summary(at, market_id.0)
            .map_err(runtime_err)
    }

    fn collateral_mirror_status(
        &self,
        asset_id: u64,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<CollateralMirrorStatus> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .collateral_mirror_status(at, asset_id)
            .map_err(runtime_err)
    }

    fn control_status(
        &self,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<PredictionControlStatus> {
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        self.client
            .runtime_api()
            .control_status(at)
            .map_err(runtime_err)
    }
}
