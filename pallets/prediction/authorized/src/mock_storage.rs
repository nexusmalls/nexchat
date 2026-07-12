#![cfg(test)]

#[frame_support::pallet]
pub(crate) mod pallet {
    use core::marker::PhantomData;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::BlockNumberFor;
    use zrml_market_commons::MarketCommonsPalletApi;

    type MarketIdOf<T> = <<T as Config>::MarketCommons as MarketCommonsPalletApi>::MarketId;
    pub type CacheSize = ConstU32<64>;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type MarketCommons: MarketCommonsPalletApi<
            AccountId = Self::AccountId,
            BlockNumber = BlockNumberFor<Self>,
        >;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::storage]
    pub(crate) type MarketIdsPerDisputeBlock<T: Config> = StorageMap<
        _,
        Twox64Concat,
        BlockNumberFor<T>,
        BoundedVec<MarketIdOf<T>, CacheSize>,
        ValueQuery,
    >;
}
