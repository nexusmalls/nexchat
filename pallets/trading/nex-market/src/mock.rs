use crate as pallet_nex_market;
use frame_support::{
    derive_impl, parameter_types,
    traits::{ConstU128, ConstU16, ConstU32, ConstU64},
};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        NexMarket: pallet_nex_market,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type AccountStore = System;
    type Balance = u128;
}

// OCW bare unsigned tx support for test runtime
impl frame_system::offchain::CreateTransactionBase<pallet_nex_market::pallet::Call<Test>> for Test {
    type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;
    type RuntimeCall = RuntimeCall;
}

impl frame_system::offchain::CreateBare<pallet_nex_market::pallet::Call<Test>> for Test {
    fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
        sp_runtime::testing::TestXt::new_bare(call)
    }
}

parameter_types! {
    pub const TreasuryAccountId: u64 = 99;
    pub const SeedLiquidityAccountId: u64 = 96;
    pub const RewardSourceId: u64 = 97;
    pub const SeedTronAddr: [u8; 34] = *b"T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWb1";
}

/// Mock DepositCalculator: 模拟固定汇率 1 NEX = 1 USDT
/// nex = usd_amount × 10^12 / 10^6 = usd_amount × 10^6
///
/// 通过 `ORACLE_AVAILABLE` 开关模拟 Oracle 不可用场景。
pub struct MockDepositCalculator;

thread_local! {
    pub static ORACLE_AVAILABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

impl pallet_trading_common::DepositCalculator<u128> for MockDepositCalculator {
    fn calculate_deposit(usd_amount: u64, fallback: u128) -> u128 {
        if !ORACLE_AVAILABLE.with(|c| c.get()) {
            return fallback;
        }
        let nex = (usd_amount as u128)
            .saturating_mul(1_000_000_000_000u128)
            .saturating_div(1_000_000u128);
        if nex == 0 {
            fallback
        } else {
            nex
        }
    }
}

pub struct MarketGovernanceMembers;
impl frame_support::traits::SortedMembers<u64> for MarketGovernanceMembers {
    fn sorted_members() -> Vec<u64> {
        vec![99]
    }
}

impl pallet_nex_market::Config for Test {
    type Currency = Balances;
    type WeightInfo = ();
    type DefaultOrderTTL = ConstU32<14400>;
    type MaxActiveOrdersPerUser = ConstU32<100>;
    type UsdtTimeout = ConstU32<7200>; // 12h
    type BlocksPerHour = ConstU32<600>;
    type BlocksPerDay = ConstU32<14400>;
    type BlocksPerWeek = ConstU32<100800>;
    type CircuitBreakerDuration = ConstU32<600>;
    type VerificationReward = ConstU128<100_000_000_000>; // 0.1 NEX
    type RewardSource = RewardSourceId;
    type BuyerDepositRate = ConstU16<1000>; // 10%
    type MinBuyerDepositUsd = ConstU64<1_000_000>; // 1 USDT
    type DepositCalculator = MockDepositCalculator;
    type DepositForfeitRate = ConstU16<10000>; // 100%
    type TreasuryAccount = TreasuryAccountId;
    type SeedLiquidityAccount = SeedLiquidityAccountId;
    type MarketAdminOrigin = frame_support::traits::EitherOfDiverse<
        frame_system::EnsureRoot<u64>,
        frame_system::EnsureSignedBy<MarketGovernanceMembers, u64>,
    >;
    type FirstOrderTimeout = ConstU32<600>; // 1h (免保证金短超时)
    type MaxFirstOrderAmount = ConstU128<100_000_000_000_000>; // 100 NEX
    type MaxWaivedSeedOrders = ConstU32<10>;
    type SeedPricePremiumBps = ConstU16<2000>; // 20% 溢价
    type SeedOrderUsdtAmount = ConstU64<10_000_000>; // 10 USDT
    type SeedTronAddress = SeedTronAddr;
    type VerificationGracePeriod = ConstU32<600>; // 1h 宽限期
    type UnderpaidGracePeriod = ConstU32<1200>; // 2h 补付窗口
    type DepositPenaltyGracePeriod = ConstU32<1200>; // 2h 罚金宽限期
    type DepositPenaltyRatePerHour = ConstU16<500>; // 5%/h
    type MaxPendingTrades = ConstU32<100>;
    type MaxAwaitingPaymentTrades = ConstU32<100>;
    type MaxUnderpaidTrades = ConstU32<100>;
    type MaxExpiredOrdersPerBlock = ConstU32<10>;
    type TxHashTtlBlocks = ConstU32<100800>; // ~7 days (600 blocks/h × 24h × 7d)
    type MinTxHashTtlBlocks = ConstU32<86400>; // H-1: 最小安全值 ~6 days (72h lookback × 2 安全系数)
    type MinOrderNexAmount = ConstU128<1_000_000_000_000>; // 1 NEX
    type MaxTradesPerUser = ConstU32<200>;
    type MaxOrderTrades = ConstU32<50>;
    type QueueFullThresholdBps = ConstU16<8000>; // 80%
    type DisputeWindowBlocks = ConstU32<100800>; // ~7 days
    type MaxOrderNexAmount = ConstU128<500_000_000_000_000>; // 500 NEX
    type MaxSellOrders = ConstU32<1000>;
    type MaxBuyOrders = ConstU32<1000>;
    type MinIndexerStake = ConstU128<100_000_000_000_000>; // 100 NEX
    type MaxIndexers = ConstU32<10>;
    type IndexerGracePeriod = ConstU32<50>; // 50 blocks
    type IndexerHintReward = ConstU128<10_000_000_000>; // 0.01 NEX
    type MaxIndexerErrors = ConstU32<3>;
    type IndexerSlashRateBps = ConstU16<3000>; // 30% slash on suspend
    type MaxPenaltyTradesPerBlock = ConstU32<10>;
}

pub const ALICE: u64 = 1; // 卖家
pub const BOB: u64 = 2; // 买家
pub const CHARLIE: u64 = 3; // 第三方（领取奖励）
pub const DAVE: u64 = 4; // 新用户（零余额，测试首单免保证金）
pub const INDEXER: u64 = 5; // Indexer 节点

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (ALICE, 1_000_000_000_000_000), // 1000 NEX
            (BOB, 1_000_000_000_000_000),   // 1000 NEX
            (CHARLIE, 100_000_000_000_000), // 100 NEX
            (INDEXER, 500_000_000_000_000), // 500 NEX (Indexer)
            (97, 1_000_000_000_000_000),    // RewardSource: 1000 NEX
            (96, 500_000_000_000_000),      // SeedLiquidity: 500 NEX
            (99, 1_000_000_000_000_000),    // Treasury: 1000 NEX
        ],
        ..Default::default()
    }
    .assimilate_storage(&mut t)
    .unwrap();

    sp_io::TestExternalities::new(t)
}
