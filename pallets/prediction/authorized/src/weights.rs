use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

/// Weight interface for authorized dispute resolution.
/// 授权争议解决的权重接口。
pub trait WeightInfoZeitgeist {
    fn authorize_market_outcome_first_report(m: u32) -> Weight;
    fn authorize_market_outcome_existing_report() -> Weight;
    fn on_dispute_weight() -> Weight;
    fn on_resolution_weight() -> Weight;
    fn exchange_weight() -> Weight;
    fn get_auto_resolve_weight() -> Weight;
    fn has_failed_weight() -> Weight;
    fn on_global_dispute_weight() -> Weight;
    fn clear_weight() -> Weight;
}

/// Upstream benchmark weights used for compile validation only.
/// 仅用于编译验证的上游基准权重。
pub struct WeightInfo<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfoZeitgeist for WeightInfo<T> {
    fn authorize_market_outcome_first_report(m: u32) -> Weight {
        Weight::from_parts(39_463_135, 4_507)
            .saturating_add(Weight::from_parts(139_182, 0).saturating_mul(m.into()))
            .saturating_add(T::DbWeight::get().reads(3))
            .saturating_add(T::DbWeight::get().writes(2))
    }
    fn authorize_market_outcome_existing_report() -> Weight {
        Weight::from_parts(33_970_000, 4_173)
            .saturating_add(T::DbWeight::get().reads(2))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn on_dispute_weight() -> Weight {
        Weight::from_parts(300_000, 0)
    }
    fn on_resolution_weight() -> Weight {
        Weight::from_parts(10_950_000, 3_514)
            .saturating_add(T::DbWeight::get().reads(1))
            .saturating_add(T::DbWeight::get().writes(1))
    }
    fn exchange_weight() -> Weight {
        Weight::from_parts(290_000, 0)
    }
    fn get_auto_resolve_weight() -> Weight {
        Weight::from_parts(10_130_000, 3_514).saturating_add(T::DbWeight::get().reads(1))
    }
    fn has_failed_weight() -> Weight {
        Weight::from_parts(300_000, 0)
    }
    fn on_global_dispute_weight() -> Weight {
        Weight::from_parts(310_000, 0)
    }
    fn clear_weight() -> Weight {
        Weight::from_parts(2_180_000, 0).saturating_add(T::DbWeight::get().writes(1))
    }
}
