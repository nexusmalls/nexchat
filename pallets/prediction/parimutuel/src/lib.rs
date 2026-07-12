// Copyright 2023-2025 Forecasting Technologies LTD.
//
// This file is part of Zeitgeist.
//
// Zeitgeist is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the
// Free Software Foundation, either version 3 of the License, or (at
// your option) any later version.

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(feature = "mock")]
pub mod mock;
#[cfg(test)]
mod tests;
mod utils;
pub mod weights;

pub use pallet::*;

#[frame_support::pallet]
mod pallet {
    use crate::weights::WeightInfoZeitgeist;
    use core::marker::PhantomData;
    use frame_support::{
        ensure,
        pallet_prelude::{Decode, DecodeWithMemTracking, Encode, TypeInfo},
        require_transactional,
        traits::{ExistenceRequirement, Get, StorageVersion},
        PalletId,
    };
    use frame_system::{
        ensure_signed,
        pallet_prelude::{BlockNumberFor, OriginFor},
    };
    use orml_traits::MultiCurrency;
    use sp_runtime::{
        traits::{AccountIdConversion, CheckedSub, Zero},
        DispatchError, DispatchResult, RuntimeDebug,
    };
    use zeitgeist_primitives::{
        math::fixed::FixedMulDiv,
        traits::DistributeFees,
        types::{Asset, Market, MarketStatus, MarketType, OutcomeReport, ScoringRule},
    };
    use zrml_market_commons::MarketCommonsPalletApi;

    #[pallet::config]
    /// Runtime dependencies for parimutuel betting, payouts, refunds, and market lookup.
    /// 彩池制下注、派彩、退款与市场查询所需的 runtime 依赖。
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Multi-currency backend for collateral and parimutuel shares.
        /// 用于抵押资产与彩池份额的多币种后端。
        type AssetManager: MultiCurrency<Self::AccountId, CurrencyId = AssetOf<Self>>;

        /// Fee distributor that charges the market base asset before minting shares.
        /// 在铸造份额前从市场基础资产收取费用的分配器。
        type ExternalFees: DistributeFees<
            Asset = Asset<MarketIdOf<Self>>,
            AccountId = AccountIdOf<Self>,
            Balance = BalanceOf<Self>,
            MarketId = MarketIdOf<Self>,
        >;

        /// Source of existing market definitions; collateral admission occurs at market creation.
        /// 既有市场定义的数据源；抵押资产准入在市场创建边界执行。
        type MarketCommons: MarketCommonsPalletApi<
            AccountId = Self::AccountId,
            BlockNumber = BlockNumberFor<Self>,
            Balance = BalanceOf<Self>,
        >;

        /// Minimum post-fee amount accepted for one bet.
        /// 单笔下注扣费后可接受的最小金额。
        #[pallet::constant]
        type MinBetSize: Get<BalanceOf<Self>>;

        /// Stable identifier used to derive each market's pot account.
        /// 用于派生每个市场资金池账户的稳定标识符。
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// Weight implementation for public calls.
        /// 公开调用使用的权重实现。
        type WeightInfo: WeightInfoZeitgeist;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);
    const LOG_TARGET: &str = "runtime::zrml-parimutuel";

    pub(crate) type AssetOf<T> = Asset<MarketIdOf<T>>;
    pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
    pub(crate) type BalanceOf<T> =
        <<T as Config>::AssetManager as MultiCurrency<AccountIdOf<T>>>::Balance;
    pub(crate) type MarketIdOf<T> =
        <<T as Config>::MarketCommons as MarketCommonsPalletApi>::MarketId;
    pub(crate) type MomentOf<T> = <<T as Config>::MarketCommons as MarketCommonsPalletApi>::Moment;
    pub(crate) type MarketOf<T> =
        Market<AccountIdOf<T>, BalanceOf<T>, BlockNumberFor<T>, MomentOf<T>, MarketIdOf<T>>;

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T>
    where
        T: Config,
    {
        /// An outcome was bought.
        OutcomeBought {
            market_id: MarketIdOf<T>,
            buyer: AccountIdOf<T>,
            asset: AssetOf<T>,
            amount_minus_fees: BalanceOf<T>,
            fees: BalanceOf<T>,
        },
        /// Rewards of the pot were claimed.
        RewardsClaimed {
            market_id: MarketIdOf<T>,
            asset: AssetOf<T>,
            withdrawn_asset_balance: BalanceOf<T>,
            base_asset_payoff: BalanceOf<T>,
            sender: AccountIdOf<T>,
        },
        /// A market base asset was refunded.
        BalanceRefunded {
            market_id: MarketIdOf<T>,
            asset: AssetOf<T>,
            refunded_balance: BalanceOf<T>,
            sender: AccountIdOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        NoRewardShareOutstanding,
        MarketIsNotActive,
        AmountBelowMinimumBetSize,
        NotParimutuelOutcome,
        InvalidOutcomeAsset,
        InvalidScoringRule,
        InsufficientBalance,
        MarketIsNotResolvedYet,
        Unexpected,
        NoResolvedOutcome,
        RefundNotAllowed,
        RefundableBalanceIsZero,
        NoWinningShares,
        NotCategorical,
        NoRewardToDistribute,
        InconsistentState(InconsistentStateError),
    }

    #[derive(
        Encode,
        Decode,
        DecodeWithMemTracking,
        Eq,
        PartialEq,
        TypeInfo,
        frame_support::PalletError,
        RuntimeDebug,
    )]
    pub enum InconsistentStateError {
        InsufficientFundsInPotAccount,
        OutcomeIssuanceGreaterCollateral,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Buys parimutuel shares with the market base asset.
        /// 使用市场基础资产购买彩池份额。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::buy())]
        #[frame_support::transactional]
        pub fn buy(
            origin: OriginFor<T>,
            asset: Asset<MarketIdOf<T>>,
            #[pallet::compact] amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_buy(who, asset, amount)
        }

        /// Claims a caller's proportional winnings from a resolved market.
        /// 从已结算市场领取调用者按比例计算的奖金。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::claim_rewards())]
        #[frame_support::transactional]
        pub fn claim_rewards(origin: OriginFor<T>, market_id: MarketIdOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_claim_rewards(who, market_id)
        }

        /// Refunds a losing share when nobody bought the winning outcome.
        /// 当无人购买获胜结果时，退还失败结果份额对应的基础资产。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::claim_refunds())]
        #[frame_support::transactional]
        pub fn claim_refunds(origin: OriginFor<T>, refund_asset: AssetOf<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_claim_refunds(who, refund_asset)
        }
    }

    impl<T: Config> Pallet<T> {
        /// Returns the deterministic pot account for a market.
        /// 返回市场对应的确定性资金池账户。
        #[inline]
        pub fn pot_account(market_id: MarketIdOf<T>) -> AccountIdOf<T> {
            T::PalletId::get().into_sub_account_truncating(market_id)
        }

        fn check_values(
            winning_balance: BalanceOf<T>,
            pot_total: BalanceOf<T>,
            outcome_total: BalanceOf<T>,
            payoff: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(
                pot_total >= winning_balance,
                Error::<T>::InconsistentState(
                    InconsistentStateError::InsufficientFundsInPotAccount
                )
            );
            ensure!(
                pot_total >= outcome_total,
                Error::<T>::InconsistentState(
                    InconsistentStateError::OutcomeIssuanceGreaterCollateral
                )
            );
            if payoff < winning_balance {
                log::debug!(
                    target: LOG_TARGET,
                    "The payoff should be greater than or equal to the winning balance."
                );
                debug_assert!(false);
            }
            if pot_total < payoff {
                log::debug!(target: LOG_TARGET, "The payoff should not exceed the pot.");
                debug_assert!(false);
            }
            Ok(())
        }

        pub fn market_assets_contains(market: &MarketOf<T>, asset: &AssetOf<T>) -> DispatchResult {
            if let Asset::ParimutuelShare(_, i) = asset {
                match market.market_type {
                    MarketType::Categorical(categories) => {
                        ensure!(*i < categories, Error::<T>::InvalidOutcomeAsset);
                        return Ok(());
                    }
                    MarketType::Scalar(_) => return Err(Error::<T>::NotCategorical.into()),
                }
            }
            Err(Error::<T>::NotParimutuelOutcome.into())
        }

        #[require_transactional]
        fn do_buy(who: T::AccountId, asset: AssetOf<T>, amount: BalanceOf<T>) -> DispatchResult {
            let market_id = match asset {
                Asset::ParimutuelShare(market_id, _) => market_id,
                _ => return Err(Error::<T>::NotParimutuelOutcome.into()),
            };
            let market = T::MarketCommons::market(&market_id)?;
            let base_asset = market.base_asset;
            ensure!(
                T::AssetManager::ensure_can_withdraw(base_asset, &who, amount).is_ok(),
                Error::<T>::InsufficientBalance
            );
            ensure!(
                market.status == MarketStatus::Active,
                Error::<T>::MarketIsNotActive
            );
            ensure!(
                market.scoring_rule == ScoringRule::Parimutuel,
                Error::<T>::InvalidScoringRule
            );
            ensure!(
                matches!(market.market_type, MarketType::Categorical(_)),
                Error::<T>::NotCategorical
            );
            Self::market_assets_contains(&market, &asset)?;

            let external_fees = T::ExternalFees::distribute(market_id, base_asset, &who, amount);
            let amount_minus_fees = amount
                .checked_sub(&external_fees)
                .ok_or(Error::<T>::Unexpected)?;
            ensure!(
                amount_minus_fees >= T::MinBetSize::get(),
                Error::<T>::AmountBelowMinimumBetSize
            );

            let pot_account = Self::pot_account(market_id);
            T::AssetManager::transfer(
                market.base_asset,
                &who,
                &pot_account,
                amount_minus_fees,
                ExistenceRequirement::AllowDeath,
            )?;
            T::AssetManager::deposit(asset, &who, amount_minus_fees)?;

            Self::deposit_event(Event::OutcomeBought {
                market_id,
                buyer: who,
                asset,
                amount_minus_fees,
                fees: external_fees,
            });
            Ok(())
        }

        fn ensure_parimutuel_market_resolved(market: &MarketOf<T>) -> DispatchResult {
            ensure!(
                market.status == MarketStatus::Resolved,
                Error::<T>::MarketIsNotResolvedYet
            );
            ensure!(
                market.scoring_rule == ScoringRule::Parimutuel,
                Error::<T>::InvalidScoringRule
            );
            ensure!(
                matches!(market.market_type, MarketType::Categorical(_)),
                Error::<T>::NotCategorical
            );
            Ok(())
        }

        fn get_winning_asset(
            market_id: MarketIdOf<T>,
            market: &MarketOf<T>,
        ) -> Result<AssetOf<T>, DispatchError> {
            match market
                .resolved_outcome
                .clone()
                .ok_or(Error::<T>::NoResolvedOutcome)?
            {
                OutcomeReport::Categorical(index) => Ok(Asset::ParimutuelShare(market_id, index)),
                OutcomeReport::Scalar(_) => Err(Error::<T>::NotCategorical.into()),
            }
        }

        #[require_transactional]
        fn do_claim_rewards(who: T::AccountId, market_id: MarketIdOf<T>) -> DispatchResult {
            let market = T::MarketCommons::market(&market_id)?;
            Self::ensure_parimutuel_market_resolved(&market)?;
            let winning_asset = Self::get_winning_asset(market_id, &market)?;
            let outcome_total = T::AssetManager::total_issuance(winning_asset);
            ensure!(
                !outcome_total.is_zero(),
                Error::<T>::NoRewardShareOutstanding
            );
            let winning_balance = T::AssetManager::free_balance(winning_asset, &who);
            ensure!(!winning_balance.is_zero(), Error::<T>::NoWinningShares);
            if outcome_total < winning_balance {
                log::debug!(
                    target: LOG_TARGET,
                    "Outcome issuance should cover the individual winning balance."
                );
                debug_assert!(false);
            }

            let pot_account = Self::pot_account(market_id);
            let pot_total = T::AssetManager::free_balance(market.base_asset, &pot_account);
            let payoff = pot_total.bmul_bdiv(winning_balance, outcome_total)?;
            Self::check_values(winning_balance, pot_total, outcome_total, payoff)?;

            let withdrawn_asset_balance = winning_balance;
            T::AssetManager::withdraw(
                winning_asset,
                &who,
                withdrawn_asset_balance,
                ExistenceRequirement::AllowDeath,
            )?;
            let remaining = T::AssetManager::free_balance(market.base_asset, &pot_account);
            let base_asset_payoff = payoff.min(remaining);
            T::AssetManager::transfer(
                market.base_asset,
                &pot_account,
                &who,
                base_asset_payoff,
                ExistenceRequirement::AllowDeath,
            )?;

            Self::deposit_event(Event::RewardsClaimed {
                market_id,
                asset: winning_asset,
                withdrawn_asset_balance,
                base_asset_payoff,
                sender: who,
            });
            Ok(())
        }

        #[require_transactional]
        fn do_claim_refunds(who: T::AccountId, refund_asset: AssetOf<T>) -> DispatchResult {
            let market_id = match refund_asset {
                Asset::ParimutuelShare(market_id, _) => market_id,
                _ => return Err(Error::<T>::NotParimutuelOutcome.into()),
            };
            let market = T::MarketCommons::market(&market_id)?;
            Self::ensure_parimutuel_market_resolved(&market)?;
            Self::market_assets_contains(&market, &refund_asset)?;
            let winning_asset = Self::get_winning_asset(market_id, &market)?;
            ensure!(
                T::AssetManager::total_issuance(winning_asset).is_zero(),
                Error::<T>::RefundNotAllowed
            );

            let refund_balance = T::AssetManager::free_balance(refund_asset, &who);
            ensure!(
                !refund_balance.is_zero(),
                Error::<T>::RefundableBalanceIsZero
            );
            if refund_asset == winning_asset {
                log::debug!(
                    target: LOG_TARGET,
                    "A zero-issuance winning asset cannot have a refundable balance."
                );
                debug_assert!(false);
            }

            T::AssetManager::withdraw(
                refund_asset,
                &who,
                refund_balance,
                ExistenceRequirement::AllowDeath,
            )?;
            let pot_account = Self::pot_account(market_id);
            let pot_total = T::AssetManager::free_balance(market.base_asset, &pot_account);
            if pot_total < refund_balance {
                log::debug!(target: LOG_TARGET, "Pot is lower than the refund balance.");
                debug_assert!(false);
            }
            let refunded_balance = refund_balance.min(pot_total);
            T::AssetManager::transfer(
                market.base_asset,
                &pot_account,
                &who,
                refunded_balance,
                ExistenceRequirement::AllowDeath,
            )?;

            Self::deposit_event(Event::BalanceRefunded {
                market_id,
                asset: refund_asset,
                refunded_balance,
                sender: who,
            });
            Ok(())
        }
    }
}
