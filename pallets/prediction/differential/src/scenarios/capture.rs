//! Snapshot capture helpers for the prediction-markets mock runtime.
//! prediction-markets mock runtime 的快照捕获 helper。

use orml_traits::{MultiCurrency, MultiReservableCurrency};
use zeitgeist_primitives::types::{Asset, MarketId};
use zrml_market_commons::MarketCommonsPalletApi;
use zrml_prediction_markets::mock::{AssetManager, Runtime, ALICE, BOB, CHARLIE, INITIAL_BALANCE};

use crate::{
    normalize::{normalize_asset, normalize_outcome, NormalizedAsset},
    snapshot::ScenarioSnapshot,
};

pub fn initial_native_balance() -> u128 {
    INITIAL_BALANCE
}

pub fn track_core_accounts(snapshot: &mut ScenarioSnapshot, base_asset: Asset<MarketId>) {
    for account in [ALICE, BOB, CHARLIE] {
        track_account(snapshot, account, Asset::Ztg);
        if base_asset != Asset::Ztg {
            track_account(snapshot, account, base_asset);
        }
    }
}

pub fn track_account(snapshot: &mut ScenarioSnapshot, account: u128, asset: Asset<MarketId>) {
    let Some(normalized) = normalize_asset(asset) else {
        return;
    };
    let free = AssetManager::free_balance(asset, &account);
    let reserved = AssetManager::reserved_balance(asset, &account);
    snapshot.track_balance(account, normalized, free, reserved);
}

pub fn finalize_market_snapshot(
    mut snapshot: ScenarioSnapshot,
    market_id: MarketId,
    base_asset: Asset<MarketId>,
) -> ScenarioSnapshot {
    let market = zrml_market_commons::Pallet::<Runtime>::market(&market_id).expect("market exists");
    snapshot.market_status = market.status;
    snapshot.latest_market_id =
        zrml_market_commons::Pallet::<Runtime>::latest_market_id().unwrap_or(market_id);
    snapshot.resolved_outcome = market.resolved_outcome.as_ref().map(normalize_outcome);
    snapshot.creation_bond_settled = market
        .bonds
        .creation
        .as_ref()
        .map(|bond| bond.is_settled)
        .unwrap_or(false);
    snapshot.oracle_bond_settled = market
        .bonds
        .oracle
        .as_ref()
        .map(|bond| bond.is_settled)
        .unwrap_or(false);
    snapshot.outsider_bond_settled = market
        .bonds
        .outsider
        .as_ref()
        .map(|bond| bond.is_settled)
        .unwrap_or(false);
    snapshot.dispute_bond_settled = market
        .bonds
        .dispute
        .as_ref()
        .map(|bond| bond.is_settled)
        .unwrap_or(false);
    track_core_accounts(&mut snapshot, base_asset);
    snapshot
}

pub fn native_asset() -> NormalizedAsset {
    NormalizedAsset::Native
}
