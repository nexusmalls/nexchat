// Copyright 2022-2025 Forecasting Technologies LTD.
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

#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod benchmarks;
mod mock;
mod tests;
pub mod weights;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{pallet_prelude::*, traits::Currency};
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::Zero;

    use crate::weights::WeightInfoZeitgeist;

    /// Balance type of the configured native NEX currency.
    /// 配置的原生 NEX 货币余额类型。
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// Runtime configuration for the Styx native-token burn gate.
    /// Styx 原生代币销毁门槛的 runtime 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Origin allowed to update the amount burned when crossing Styx.
        /// 允许更新跨越 Styx 时销毁数量的 origin。
        type SetBurnAmountOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Native currency burned by a successful crossing.
        /// 成功跨越时销毁的原生货币。
        type Currency: Currency<Self::AccountId>;

        /// Initial NEX amount burned by a crossing.
        /// 跨越时初始销毁的 NEX 数量。
        type DefaultBurnAmount: Get<BalanceOf<Self>>;

        /// Benchmark-generated weights.
        /// 基准测试生成的权重。
        type WeightInfo: WeightInfoZeitgeist;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    /// Keep track of crossings. Accounts are only able to cross once.
    /// 记录跨越；每个账户只能跨越一次。
    #[pallet::storage]
    pub type Crossings<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, ()>;

    /// Return the default amount burned for a crossing.
    /// 返回跨越时默认销毁的数量。
    #[pallet::type_value]
    pub fn DefaultBurnAmount<T: Config>() -> BalanceOf<T> {
        T::DefaultBurnAmount::get()
    }

    /// Configured amount burned for a crossing.
    /// 跨越时配置的销毁数量。
    #[pallet::storage]
    pub type BurnAmount<T: Config> =
        StorageValue<_, BalanceOf<T>, ValueQuery, DefaultBurnAmount<T>>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// An account crossed and claimed its right to create an avatar.
        /// 账户完成跨越并获得创建头像的资格。
        AccountCrossed(T::AccountId, BalanceOf<T>),
        /// The crossing fee was changed.
        /// 跨越费用已变更。
        CrossingFeeChanged(BalanceOf<T>),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Account does not have enough balance to cross.
        /// 账户没有足够余额完成跨越。
        FundDoesNotHaveEnoughFreeBalance,
        /// Account has already crossed.
        /// 账户已经完成过跨越。
        HasAlreadyCrossed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Burns NEX (`styx.burnAmount()`) to enter the off-chain registry.
        /// 销毁 NEX（`styx.burnAmount()`）以进入链下注册表。
        ///
        /// The signer can only cross once.
        /// 每个签名者只能跨越一次。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::cross())]
        pub fn cross(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            if Crossings::<T>::contains_key(&who) {
                Err(Error::<T>::HasAlreadyCrossed)?;
            }

            let amount = BurnAmount::<T>::get();

            if !T::Currency::can_slash(&who, amount) {
                Err(Error::<T>::FundDoesNotHaveEnoughFreeBalance)?;
            }

            let (imbalance, missing) = T::Currency::slash(&who, amount);
            debug_assert!(
                missing.is_zero(),
                "Could not slash all of the amount. who: {:?}, amount: {:?}.",
                &who,
                amount,
            );
            drop(imbalance);
            Crossings::<T>::insert(&who, ());

            Self::deposit_event(Event::AccountCrossed(who, amount));

            Ok(())
        }

        /// Set the burn amount after validating `SetBurnAmountOrigin`.
        /// 验证 `SetBurnAmountOrigin` 后设置销毁数量。
        ///
        /// Intended to be called by a governing body such as the council.
        /// 预期由委员会等治理机构调用。
        ///
        /// # Arguments / 参数
        ///
        /// * `amount`: The new burn price. / 新的销毁价格。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_burn_amount())]
        pub fn set_burn_amount(
            origin: OriginFor<T>,
            #[pallet::compact] amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::SetBurnAmountOrigin::ensure_origin(origin)?;
            BurnAmount::<T>::put(amount);

            Self::deposit_event(Event::CrossingFeeChanged(amount));

            Ok(())
        }
    }
}
