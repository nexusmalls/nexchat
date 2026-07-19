//! Runtime benchmarks for the mixed native and prediction currency adapter.
//! 原生货币与预测资产混合适配器的 runtime benchmark。

use crate::*;
use frame_benchmarking::v2::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;
use orml_traits::{BasicCurrency, MultiCurrency, MultiCurrencyExtended};

/// Supplies a non-native currency and signed amounts for runtime benchmarks.
/// 为 runtime benchmark 提供非原生资产与正负数量。
pub trait BenchmarkHelper<CurrencyId, Balance, Amount> {
    /// Returns `(currency_id, balance, positive_amount, negative_amount)`.
    /// 返回 `(资产 ID, 余额, 正数量, 负数量)`。
    fn get_currency_id_and_amounts() -> Option<(CurrencyId, Balance, Amount, Amount)>;
}

impl<CurrencyId, Balance, Amount> BenchmarkHelper<CurrencyId, Balance, Amount> for () {
    fn get_currency_id_and_amounts() -> Option<(CurrencyId, Balance, Amount, Amount)> {
        None
    }
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn transfer_non_native_currency() {
        let from: T::AccountId = account("from", 0, 0);
        let to: T::AccountId = account("to", 0, 0);
        let to_lookup = T::Lookup::unlookup(to.clone());
        let (currency_id, balance, positive_amount, _) =
            T::BenchmarkHelper::get_currency_id_and_amounts().expect("benchmark values required");
        assert_ok!(T::MultiCurrency::update_balance(
            currency_id,
            &from,
            positive_amount,
        ));

        #[extrinsic_call]
        transfer(RawOrigin::Signed(from), to_lookup, currency_id, balance);

        assert_eq!(T::MultiCurrency::total_balance(currency_id, &to), balance);
    }

    #[benchmark]
    fn transfer_native_currency() {
        let from: T::AccountId = account("from", 0, 0);
        let to: T::AccountId = account("to", 0, 0);
        let to_lookup = T::Lookup::unlookup(to.clone());
        let (_, balance, _, _) =
            T::BenchmarkHelper::get_currency_id_and_amounts().expect("benchmark values required");
        assert_ok!(T::NativeCurrency::deposit(&from, balance));

        #[extrinsic_call]
        transfer_native_currency(RawOrigin::Signed(from), to_lookup, balance);

        assert_eq!(T::NativeCurrency::total_balance(&to), balance);
    }

    #[benchmark]
    fn update_balance_non_native_currency() {
        let who: T::AccountId = account("who", 0, 0);
        let who_lookup = T::Lookup::unlookup(who.clone());
        let (currency_id, balance, positive_amount, _) =
            T::BenchmarkHelper::get_currency_id_and_amounts().expect("benchmark values required");

        #[extrinsic_call]
        update_balance(RawOrigin::Root, who_lookup, currency_id, positive_amount);

        assert_eq!(T::MultiCurrency::total_balance(currency_id, &who), balance);
    }

    #[benchmark]
    fn update_balance_native_currency_creating() {
        let who: T::AccountId = account("who", 0, 0);
        let who_lookup = T::Lookup::unlookup(who.clone());
        let (currency_id, balance, positive_amount, _) =
            T::BenchmarkHelper::get_currency_id_and_amounts().expect("benchmark values required");
        let native_currency_id = T::GetNativeCurrencyId::get();
        assert_ne!(currency_id, native_currency_id);

        #[extrinsic_call]
        update_balance(
            RawOrigin::Root,
            who_lookup,
            native_currency_id,
            positive_amount,
        );

        assert_eq!(T::NativeCurrency::total_balance(&who), balance);
    }

    #[benchmark]
    fn update_balance_native_currency_killing() {
        let who: T::AccountId = account("who", 0, 0);
        let who_lookup = T::Lookup::unlookup(who.clone());
        let (_, balance, _, negative_amount) =
            T::BenchmarkHelper::get_currency_id_and_amounts().expect("benchmark values required");
        let native_currency_id = T::GetNativeCurrencyId::get();
        assert_ok!(T::NativeCurrency::deposit(&who, balance));

        #[extrinsic_call]
        update_balance(
            RawOrigin::Root,
            who_lookup,
            native_currency_id,
            negative_amount,
        );

        assert_eq!(T::NativeCurrency::total_balance(&who), 0u32.into());
    }

    impl_benchmark_test_suite!(
        Pallet,
        crate::mock::ExtBuilder::default().build(),
        crate::mock::Runtime,
    );
}
