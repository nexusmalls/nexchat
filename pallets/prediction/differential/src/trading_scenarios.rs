//! Representative Phase 4 scenarios normalized across the fixed upstream port.
//! 固定上游移植的 Phase 4 代表性归一化场景。

use frame_support::assert_ok;
use orml_traits::{MultiCurrency, MultiReservableCurrency};
use sp_arithmetic::Perbill;
use zeitgeist_primitives::{
    constants::BASE,
    types::{
        AccountIdTest, Asset, Balance, Deadlines, Market, MarketBonds, MarketCreation,
        MarketDisputeMechanism, MarketId, MarketPeriod, MarketStatus, MarketType, Moment,
        OutcomeReport, ScoringRule,
    },
};

use crate::snapshot::TradingSnapshot;

pub const ORDERBOOK_PARTIAL_FILL: &str = "orderbook_partial_fill_native";
pub const PARIMUTUEL_NO_WINNER: &str = "parimutuel_no_winner_native";
pub const COMBINATORIAL_ROUNDTRIP: &str = "combinatorial_roundtrip_native";

pub fn trading_snapshots() -> Vec<TradingSnapshot> {
    vec![
        orderbook_partial_fill_native(),
        parimutuel_no_winner_native(),
        combinatorial_roundtrip_native(),
    ]
}

fn market(
    creator: AccountIdTest,
    base_asset: Asset<MarketId>,
    market_type: MarketType,
    scoring_rule: ScoringRule,
) -> Market<AccountIdTest, Balance, u32, Moment, MarketId> {
    Market {
        market_id: 0,
        base_asset,
        creation: MarketCreation::Permissionless,
        creator_fee: Perbill::zero(),
        creator,
        market_type,
        dispute_mechanism: Some(MarketDisputeMechanism::Authorized),
        metadata: Default::default(),
        oracle: creator,
        period: MarketPeriod::Block(0..2),
        deadlines: Deadlines {
            grace_period: 1,
            oracle_duration: 1,
            dispute_duration: 1,
        },
        report: None,
        resolved_outcome: None,
        scoring_rule,
        status: MarketStatus::Active,
        bonds: MarketBonds::default(),
        early_close: None,
    }
}

fn orderbook_partial_fill_native() -> TradingSnapshot {
    use zrml_orderbook::{
        mock::{
            AssetManager, ExtBuilder, Orderbook, Runtime, RuntimeOrigin, ALICE, BOB, MARKET_CREATOR,
        },
        Orders,
    };

    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let base_asset = Asset::Ztg;
        let outcome_asset = Asset::CategoricalOutcome(market_id, 1);
        zrml_market_commons::Markets::<Runtime>::insert(
            market_id,
            market(
                MARKET_CREATOR,
                base_asset,
                MarketType::Categorical(2),
                ScoringRule::AmmCdaHybrid,
            ),
        );
        let maker_amount = 50 * BASE;
        let taker_amount = 10 * BASE;
        let partial_fill = 5 * BASE;
        assert_ok!(AssetManager::deposit(outcome_asset, &ALICE, taker_amount));
        assert_ok!(Orderbook::place_order(
            RuntimeOrigin::signed(BOB),
            market_id,
            base_asset,
            maker_amount,
            outcome_asset,
            taker_amount,
        ));
        assert_ok!(Orderbook::fill_order(
            RuntimeOrigin::signed(ALICE),
            0,
            Some(partial_fill),
        ));

        let order = Orders::<Runtime>::get(0).unwrap();
        let mut snapshot = TradingSnapshot::new(ORDERBOOK_PARTIAL_FILL);
        snapshot.value("order.maker_amount", order.maker_amount);
        snapshot.value("order.taker_amount", order.taker_amount);
        snapshot.value(
            "maker.reserved_base",
            AssetManager::reserved_balance(base_asset, &BOB),
        );
        snapshot.value(
            "taker.free_base",
            AssetManager::free_balance(base_asset, &ALICE),
        );
        snapshot.value(
            "fee_recipient.free_base",
            AssetManager::free_balance(base_asset, &MARKET_CREATOR),
        );
        snapshot.checkpoint("order_remains", Orders::<Runtime>::contains_key(0));
        snapshot
    })
}

fn parimutuel_no_winner_native() -> TradingSnapshot {
    use zrml_parimutuel::{
        mock::{
            AssetManager, ExtBuilder, Parimutuel, Runtime, RuntimeOrigin, ALICE, BOB,
            MARKET_CREATOR,
        },
        Config,
    };

    ExtBuilder::default().build().execute_with(|| {
        let market_id = 0;
        let base_asset = Asset::Ztg;
        zrml_market_commons::Markets::<Runtime>::insert(
            market_id,
            market(
                MARKET_CREATOR,
                base_asset,
                MarketType::Categorical(3),
                ScoringRule::Parimutuel,
            ),
        );
        let alice_asset = Asset::ParimutuelShare(market_id, 0);
        let bob_asset = Asset::ParimutuelShare(market_id, 1);
        let alice_amount = 20 * <Runtime as Config>::MinBetSize::get();
        let bob_amount = 10 * <Runtime as Config>::MinBetSize::get();
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(ALICE),
            alice_asset,
            alice_amount,
        ));
        assert_ok!(Parimutuel::buy(
            RuntimeOrigin::signed(BOB),
            bob_asset,
            bob_amount,
        ));
        zrml_market_commons::Markets::<Runtime>::mutate(market_id, |market| {
            let market = market.as_mut().unwrap();
            market.status = MarketStatus::Resolved;
            market.resolved_outcome = Some(OutcomeReport::Categorical(2));
        });
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(ALICE),
            alice_asset,
        ));
        assert_ok!(Parimutuel::claim_refunds(
            RuntimeOrigin::signed(BOB),
            bob_asset,
        ));

        let mut snapshot = TradingSnapshot::new(PARIMUTUEL_NO_WINNER);
        snapshot.value(
            "alice.free_base",
            AssetManager::free_balance(base_asset, &ALICE),
        );
        snapshot.value(
            "bob.free_base",
            AssetManager::free_balance(base_asset, &BOB),
        );
        snapshot.value(
            "fee_recipient.free_base",
            AssetManager::free_balance(base_asset, &MARKET_CREATOR),
        );
        snapshot.value(
            "pot.free_base",
            AssetManager::free_balance(base_asset, &Parimutuel::pot_account(market_id)),
        );
        snapshot.checkpoint(
            "shares_burned",
            AssetManager::free_balance(alice_asset, &ALICE) == 0
                && AssetManager::free_balance(bob_asset, &BOB) == 0,
        );
        snapshot
    })
}

fn combinatorial_roundtrip_native() -> TradingSnapshot {
    use zrml_combinatorial_tokens::{
        mock::{
            ext_builder::ExtBuilder,
            runtime::{CombinatorialTokens, Currencies, MarketCommons, Runtime, RuntimeOrigin},
        },
        types::Fuel,
        Pallet,
    };
    use zrml_market_commons::MarketCommonsPalletApi;

    ExtBuilder::build().execute_with(|| {
        let base_asset = Asset::Ztg;
        assert_ok!(MarketCommons::push_market(market(
            0,
            base_asset,
            MarketType::Categorical(2),
            ScoringRule::AmmCdaHybrid,
        )));
        let market_id = MarketCommons::latest_market_id().unwrap();
        let partition = vec![vec![true, false], vec![false, true]];
        let amount = 10 * BASE;
        assert_ok!(Currencies::deposit(base_asset, &0, amount));
        let first_position = Pallet::<Runtime>::position_from_parent_collection(
            None,
            market_id,
            partition[0].clone(),
            Fuel::new(16, false),
        )
        .unwrap();
        assert_ok!(CombinatorialTokens::split_position(
            RuntimeOrigin::signed(0),
            None,
            market_id,
            partition.clone(),
            amount,
            Fuel::new(16, false),
        ));
        assert_ok!(CombinatorialTokens::merge_position(
            RuntimeOrigin::signed(0),
            None,
            market_id,
            partition,
            amount,
            Fuel::new(16, false),
        ));

        let mut snapshot = TradingSnapshot::new(COMBINATORIAL_ROUNDTRIP);
        snapshot.value(
            "account.free_base",
            Currencies::free_balance(base_asset, &0),
        );
        snapshot.value(
            "pallet.free_base",
            Currencies::free_balance(base_asset, &Pallet::<Runtime>::account_id()),
        );
        snapshot.value(
            "first_position.issuance",
            Currencies::total_issuance(first_position),
        );
        snapshot.checkpoint(
            "roundtrip_restored",
            Currencies::free_balance(base_asset, &0) == amount,
        );
        snapshot
    })
}
