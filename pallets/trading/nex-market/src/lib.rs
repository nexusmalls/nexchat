//! # NEX/USDT P2P 交易市场模块 (pallet-nex-market)
//!
//! ## 概述
//!
//! 无做市商的 NEX/USDT 订单簿交易市场。任何人可挂单/吃单。
//! - USDT 通道：TRC20 链下支付 + OCW 验证 + 多档判定
//! - 买家保证金：防不付款风险
//! - 三周期 TWAP 预言机：1h / 24h / 7d 防操纵
//! - 熔断机制：价格偏离 7d TWAP 超阈值自动暂停
//!
//! ## 交易对
//!
//! NEX (链上原生代币) ↔ USDT (TRC20, 链下)
//!
//! - 卖 NEX: 卖家锁定 NEX → 买家链下转 USDT → OCW 验证 → 释放 NEX
//! - 买 NEX: 买家挂单 → 卖家接受(锁 NEX) → 买家链下转 USDT → OCW 验证

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod migrations;
pub mod runtime_api;
pub mod weights;

mod credit;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    pub use crate::credit::{BuyerCreditProfile, CreditChangeReason};
    use alloc::vec::Vec;
    use codec::{Decode, Encode};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, EnsureOrigin, ExistenceRequirement, ReservableCurrency},
        transactional, BoundedVec,
    };
    use frame_system::pallet_prelude::*;
    use pallet_trading_common::DepositCalculator;
    use sp_runtime::traits::{CheckedAdd, CheckedSub, Saturating, Zero};
    use sp_runtime::transaction_validity::{
        InvalidTransaction, TransactionSource, TransactionValidity, ValidTransaction,
    };
    use sp_runtime::SaturatedConversion;
    use weights::WeightInfo;

    /// Balance 类型别名
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    // ==================== 数据结构 ====================

    /// 订单方向
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        Copy,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub enum OrderSide {
        /// 买单（用 USDT 买 NEX）
        Buy,
        /// 卖单（卖 NEX 得 USDT）
        Sell,
    }

    /// 订单状态
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        Copy,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub enum OrderStatus {
        /// 挂单中
        Open,
        /// 部分成交
        PartiallyFilled,
        /// 完全成交
        Filled,
        /// 已取消
        Cancelled,
        /// 🆕 H3: 已过期（on_idle GC 自动清理）
        Expired,
    }

    // USDT 交易状态 — 统一使用 pallet-trading-common 定义
    pub use pallet_trading_common::UsdtTradeStatus;

    // 买家保证金状态 — 统一使用 pallet-trading-common 定义
    pub use pallet_trading_common::BuyerDepositStatus;

    // 付款金额验证结果（从 pallet-trading-common 导入）
    pub use pallet_trading_common::PaymentVerificationResult;

    /// 争议状态
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        Copy,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub enum DisputeStatus {
        /// 待处理
        Open,
        /// 已解决：释放 NEX 给买家
        ResolvedForBuyer,
        /// 已解决：退款给卖家
        ResolvedForSeller,
    }

    /// 争议解决方向
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        Copy,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub enum DisputeResolution {
        /// 买家胜：释放 NEX 给买家 + 退还保证金
        ReleaseToBuyer,
        /// 卖家胜：退还 NEX 给卖家 + 没收保证金
        RefundToSeller,
    }

    /// 交易争议记录
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct TradeDispute<T: Config> {
        pub trade_id: u64,
        pub initiator: T::AccountId,
        pub status: DisputeStatus,
        pub created_at: BlockNumberFor<T>,
        /// 证据 CID（IPFS 哈希）
        pub evidence_cid: BoundedVec<u8, ConstU32<128>>,
        /// 对方反驳证据 CID
        pub counter_evidence_cid: Option<BoundedVec<u8, ConstU32<128>>>,
        /// 反驳方
        pub counter_party: Option<T::AccountId>,
    }

    // TRON 地址类型 — 统一使用 pallet-trading-common 定义
    pub use pallet_trading_common::TronAddress;

    // TRON 交易哈希类型 — 统一使用 pallet-trading-common 定义
    pub use pallet_trading_common::TxHash;

    /// B1: 单次提交允许的最大 tx_hash 数量
    pub type MaxTxHashesPerSubmit = ConstU32<20>;
    /// B1: 多 tx_hash 向量类型
    pub type TxHashVec = BoundedVec<TxHash, MaxTxHashesPerSubmit>;

    /// 订单
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct Order<T: Config> {
        pub order_id: u64,
        /// 挂单者
        pub maker: T::AccountId,
        /// 方向
        pub side: OrderSide,
        /// NEX 数量
        pub nex_amount: BalanceOf<T>,
        /// 已成交 NEX 数量
        pub filled_amount: BalanceOf<T>,
        /// USDT 单价（每 NEX 的 USDT 价格，精度 10^6）
        pub usdt_price: u64,
        /// 挂单者的 TRON 地址（卖单=卖家收款地址，买单=买家付款地址）
        pub tron_address: Option<TronAddress>,
        /// 状态
        pub status: OrderStatus,
        /// 创建区块
        pub created_at: BlockNumberFor<T>,
        /// 过期区块
        pub expires_at: BlockNumberFor<T>,
        /// 买家预锁定保证金（仅买单，卖单为 Zero）
        pub buyer_deposit: BalanceOf<T>,
        /// 是否免保证金（仅 seed_liquidity 创建的卖单）
        pub deposit_waived: bool,
        /// 卖家设置的最低吃单量（0 = 不限制）
        pub min_fill_amount: BalanceOf<T>,
    }

    /// USDT 交易记录
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct UsdtTrade<T: Config> {
        pub trade_id: u64,
        pub order_id: u64,
        /// 卖 NEX 方（收 USDT，提供 TRON 地址）
        pub seller: T::AccountId,
        /// 买 NEX 方（付 USDT）
        pub buyer: T::AccountId,
        /// NEX 数量
        pub nex_amount: BalanceOf<T>,
        /// USDT 金额（精度 10^6）
        pub usdt_amount: u64,
        /// 卖家 TRON 地址
        pub seller_tron_address: TronAddress,
        /// 买家 TRON 地址（付款方，用于 OCW 匹配 from/to/amount）
        pub buyer_tron_address: Option<TronAddress>,
        /// 状态
        pub status: UsdtTradeStatus,
        /// 创建区块
        pub created_at: BlockNumberFor<T>,
        /// 超时区块
        pub timeout_at: BlockNumberFor<T>,
        /// 买家保证金
        pub buyer_deposit: BalanceOf<T>,
        /// 保证金状态
        pub deposit_status: BuyerDepositStatus,
        /// 首次 OCW 验证时间（区块号）
        pub first_verified_at: Option<BlockNumberFor<T>>,
        /// 首次检测金额
        pub first_actual_amount: Option<u64>,
        /// 少付补付截止区块
        pub underpaid_deadline: Option<BlockNumberFor<T>>,
        /// W5: 交易终态时间（用于精确争议窗口）
        pub completed_at: Option<BlockNumberFor<T>>,
        /// W6: 买家是否已确认付款（用于争议准入判定）
        pub payment_confirmed: bool,
        /// 逾期罚金：已累计从保证金中扣除的金额
        pub cumulative_penalty: BalanceOf<T>,
    }

    /// 市场统计
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
        Default,
    )]
    pub struct MarketStats {
        pub total_orders: u64,
        pub total_trades: u64,
        pub total_volume_usdt: u128,
    }

    /// 价格快照
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
        Default,
    )]
    pub struct PriceSnapshot {
        pub cumulative_price: u128,
        pub block_number: u32,
    }

    /// TWAP accumulator with three periods (1h / 24h / 7d).
    ///
    /// Each period keeps a dual checkpoint (`prev` / `curr`). `curr` is rotated
    /// into `prev` once it is at least one full period old, so `prev` stays
    /// between 1x and 2x the period old (advanced on trades and in `on_idle`).
    /// TWAP is computed against the checkpoint closest to (but at least) one
    /// period old, which guarantees the measured window matches the period name.
    ///
    /// TWAP 累积器（三周期：1小时 / 24小时 / 7天）。
    ///
    /// 每个周期维护双 checkpoint（`prev` / `curr`）。当 `curr` 距今超过一个完整
    /// 周期时轮换为 `prev`，因此 `prev` 的年龄始终介于 1~2 个周期之间（成交与
    /// `on_idle` 都会推进）。计算 TWAP 时选取"距今至少一个周期、且最接近一个
    /// 周期"的 checkpoint，保证实际测量窗口与周期命名一致。
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
        Default,
    )]
    pub struct TwapAccumulator {
        pub current_cumulative: u128,
        pub current_block: u32,
        /// 最新成交价 (USDT per NEX, 精度 10^6)
        pub last_price: u64,
        pub trade_count: u64,
        /// Block of the first real trade (0 = no real trade yet). Used to
        /// measure how much price history the market has accumulated.
        /// 首笔真实成交的区块号（0 = 尚无真实成交），用于度量市场已积累的
        /// 价格历史长度。
        pub first_trade_block: u32,
        /// 1h checkpoints: previous (older) / current. 1小时双 checkpoint：旧 / 新。
        pub hour_prev: PriceSnapshot,
        /// 1h current checkpoint. 1小时当前 checkpoint。
        pub hour_curr: PriceSnapshot,
        /// 24h previous checkpoint. 24小时旧 checkpoint。
        pub day_prev: PriceSnapshot,
        /// 24h current checkpoint. 24小时当前 checkpoint。
        pub day_curr: PriceSnapshot,
        /// 7d previous checkpoint. 7天旧 checkpoint。
        pub week_prev: PriceSnapshot,
        /// 7d current checkpoint. 7天当前 checkpoint。
        pub week_curr: PriceSnapshot,
    }

    impl TwapAccumulator {
        /// Create a fresh accumulator with all checkpoints anchored at `block`.
        /// 创建全部 checkpoint 锚定在 `block` 的新累积器。
        pub fn new_at(block: u32, last_price: u64, trade_count: u64) -> Self {
            let snapshot = PriceSnapshot {
                cumulative_price: 0,
                block_number: block,
            };
            Self {
                current_cumulative: 0,
                current_block: block,
                last_price,
                trade_count,
                first_trade_block: 0,
                hour_prev: snapshot.clone(),
                hour_curr: snapshot.clone(),
                day_prev: snapshot.clone(),
                day_curr: snapshot.clone(),
                week_prev: snapshot.clone(),
                week_curr: snapshot,
            }
        }
    }

    /// TWAP 周期
    #[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
    pub enum TwapPeriod {
        OneHour,
        OneDay,
        OneWeek,
    }

    /// 价格保护配置
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub struct PriceProtectionConfig {
        pub enabled: bool,
        /// 限价单最大偏离（bps, 2000=20%）
        pub max_price_deviation: u16,
        /// 熔断阈值（bps, 5000=50%）
        pub circuit_breaker_threshold: u16,
        /// 启用 TWAP 的最小成交数
        pub min_trades_for_twap: u64,
        pub circuit_breaker_active: bool,
        pub circuit_breaker_until: u32,
        /// 初始参考价格（USDT per NEX, 精度 10^6）
        pub initial_price: Option<u64>,
    }

    impl Default for PriceProtectionConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                max_price_deviation: 2000,
                circuit_breaker_threshold: 5000,
                min_trades_for_twap: 100,
                circuit_breaker_active: false,
                circuit_breaker_until: 0,
                initial_price: None,
            }
        }
    }

    // ==================== Indexer 数据结构 ====================

    /// Indexer 节点信息
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct IndexerInfo<T: Config> {
        /// 端点 URL
        pub endpoint_url: BoundedVec<u8, ConstU32<256>>,
        /// 质押金额
        pub stake: BalanceOf<T>,
        /// 注册区块
        pub registered_at: BlockNumberFor<T>,
        /// 加速验证成功次数
        pub accelerated_count: u32,
        /// 错误次数
        pub error_count: u32,
        /// 待处理 hint 数量（O(1) 检查是否可退出）
        pub pending_hint_count: u32,
        /// 是否被暂停
        pub suspended: bool,
    }

    /// Indexer Hint 状态
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        Copy,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    pub enum IndexerHintStatus {
        /// 待验证
        Pending,
        /// 已验证通过
        Verified,
        /// 不匹配
        Mismatch,
    }

    /// Indexer 提交的 Hint
    #[derive(
        Encode,
        Decode,
        codec::DecodeWithMemTracking,
        Clone,
        PartialEq,
        Eq,
        TypeInfo,
        MaxEncodedLen,
        Debug,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct IndexerHint<T: Config> {
        /// 提交者
        pub indexer: T::AccountId,
        /// TRON 交易哈希
        pub tx_hash: TxHash,
        /// 报告的金额
        pub reported_amount: u64,
        /// 提交区块
        pub submitted_at: BlockNumberFor<T>,
        /// 状态
        pub status: IndexerHintStatus,
    }

    // ==================== Config ====================

    #[pallet::config]
    pub trait Config:
        frame_system::Config + frame_system::offchain::CreateBare<Call<Self>>
    {
        /// 原生货币（NEX）
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// 权重
        type WeightInfo: WeightInfo;

        /// 默认订单有效期（区块数, 默认 14400 = 24h）
        #[pallet::constant]
        type DefaultOrderTTL: Get<u32>;

        /// 每用户最大活跃订单数
        #[pallet::constant]
        type MaxActiveOrdersPerUser: Get<u32>;

        /// USDT 交易超时（区块数）
        #[pallet::constant]
        type UsdtTimeout: Get<u32>;

        /// 1 小时区块数
        #[pallet::constant]
        type BlocksPerHour: Get<u32>;

        /// 24 小时区块数
        #[pallet::constant]
        type BlocksPerDay: Get<u32>;

        /// 7 天区块数
        #[pallet::constant]
        type BlocksPerWeek: Get<u32>;

        /// 熔断持续时间（区块数）
        #[pallet::constant]
        type CircuitBreakerDuration: Get<u32>;

        /// OCW 验证奖励（NEX）
        #[pallet::constant]
        type VerificationReward: Get<BalanceOf<Self>>;

        /// 奖励来源账户
        type RewardSource: Get<Self::AccountId>;

        /// 买家保证金比例（bps, 1000=10%）
        #[pallet::constant]
        type BuyerDepositRate: Get<u16>;

        /// 最低保证金 USDT 目标值（精度 10^6，如 1_000_000 = 1 USDT）
        #[pallet::constant]
        type MinBuyerDepositUsd: Get<u64>;

        /// 动态 USDT→NEX 换算器
        type DepositCalculator: pallet_trading_common::DepositCalculator<BalanceOf<Self>>;

        /// 保证金没收比例（bps, 10000=100%）
        #[pallet::constant]
        type DepositForfeitRate: Get<u16>;

        /// 国库账户
        type TreasuryAccount: Get<Self::AccountId>;

        /// 种子流动性账户（独立于国库，专用于 seed_liquidity 挂单）
        type SeedLiquidityAccount: Get<Self::AccountId>;

        /// 市场管理 Origin（委员会或 Root）
        /// 用于 seed_liquidity / set_initial_price / configure_price_protection / lift_circuit_breaker
        type MarketAdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// 免保证金交易超时（区块数，远小于 UsdtTimeout）
        #[pallet::constant]
        type FirstOrderTimeout: Get<u32>;

        /// 免保证金单笔最大 NEX 数量
        #[pallet::constant]
        type MaxFirstOrderAmount: Get<BalanceOf<Self>>;

        /// seed_liquidity 单次最多挂单数
        #[pallet::constant]
        type MaxWaivedSeedOrders: Get<u32>;

        /// seed_liquidity 溢价比例（bps, 2000=20%）：seed 价格 ≥ 基准价 × (1 + premium)
        #[pallet::constant]
        type SeedPricePremiumBps: Get<u16>;

        /// seed_liquidity 每笔订单固定 USDT 金额（精度 10^6，10_000_000 = 10 USDT）
        #[pallet::constant]
        type SeedOrderUsdtAmount: Get<u64>;

        /// 种子账户固定 TRON 收款地址（34 字节 Base58）
        type SeedTronAddress: Get<[u8; 34]>;

        /// AwaitingVerification 超时宽限期（区块数）
        /// OCW 验证可能因 API 延迟而慢，给买家额外时间
        #[pallet::constant]
        type VerificationGracePeriod: Get<u32>;

        /// 少付补付窗口（区块数）
        /// 买家少付 50%-99.5% 时，给予补付时间
        #[pallet::constant]
        type UnderpaidGracePeriod: Get<u32>;

        /// 保证金逾期罚金宽限期（区块数）
        /// 交易创建后此时间内不扣罚金（默认 2h = 1200 blocks）
        #[pallet::constant]
        type DepositPenaltyGracePeriod: Get<u32>;

        /// 保证金逾期罚金费率（bps/小时，500 = 5%/h）
        /// 宽限期后每小时从保证金中扣除此比例
        #[pallet::constant]
        type DepositPenaltyRatePerHour: Get<u16>;

        /// 🆕 H2: OCW 待验证队列最大容量
        #[pallet::constant]
        type MaxPendingTrades: Get<u32>;

        /// 🆕 H2: 待付款跟踪队列最大容量
        #[pallet::constant]
        type MaxAwaitingPaymentTrades: Get<u32>;

        /// 🆕 H2: 少付补付跟踪队列最大容量
        #[pallet::constant]
        type MaxUnderpaidTrades: Get<u32>;

        /// 🆕 H3: on_idle 每次过期订单 GC 最大处理数
        #[pallet::constant]
        type MaxExpiredOrdersPerBlock: Get<u32>;

        /// 🆕 M7: UsedTxHashes 条目保留时间（区块数）
        /// 超过此 TTL 的 tx_hash 在 on_idle 中被清理，防止无限增长
        ///
        /// ⚠️ 安全约束 (H-1 审计修复):
        /// 此值**必须**大于 trc20-verifier 的 max_lookback_ms 对应的区块数，
        /// 否则 GC 后的 tx_hash 仍在 TronGrid 回溯窗口内，可被重放。
        /// 最小安全值 = max_lookback_ms / (block_time_ms) × 2 (安全系数)
        /// 默认: max_lookback_ms=72h, block_time=6s → 最小 86400 blocks
        /// Runtime 应配置为 ≥ MinTxHashTtlBlocks
        #[pallet::constant]
        type TxHashTtlBlocks: Get<u32>;

        /// tx_hash TTL 最小安全值（区块数）
        ///
        /// H-1 审计修复: 防止 TxHashTtlBlocks 被配置为过小值导致 GC 后重放。
        /// 推荐值 = max_lookback_ms(72h) / block_time_ms(6000) × 2 = 86400
        /// on_idle GC 会取 max(TxHashTtlBlocks, MinTxHashTtlBlocks) 作为实际 TTL
        #[pallet::constant]
        type MinTxHashTtlBlocks: Get<u32>;

        /// 最低挂单/吃单 NEX 数量（防微量交易浪费 OCW 资源）
        #[pallet::constant]
        type MinOrderNexAmount: Get<BalanceOf<Self>>;

        /// 每用户最大交易记录数（UserTrades 索引上限）
        #[pallet::constant]
        type MaxTradesPerUser: Get<u32>;

        /// 每订单最大关联交易数（OrderTrades 索引上限）
        #[pallet::constant]
        type MaxOrderTrades: Get<u32>;

        /// 队列满自动暂停阈值（bps，8000=80%）
        /// 当待验证队列超过此比例时自动触发市场暂停
        #[pallet::constant]
        type QueueFullThresholdBps: Get<u16>;

        /// 争议发起时间窗口（区块数，超过此时间不允许发起争议）
        #[pallet::constant]
        type DisputeWindowBlocks: Get<u32>;

        /// 单笔挂单/吃单 NEX 最大数量（防大额风险）
        #[pallet::constant]
        type MaxOrderNexAmount: Get<BalanceOf<Self>>;

        /// 卖单订单簿最大容量（可 runtime 治理调整）
        #[pallet::constant]
        type MaxSellOrders: Get<u32>;

        /// 买单订单簿最大容量（可 runtime 治理调整）
        #[pallet::constant]
        type MaxBuyOrders: Get<u32>;

        /// Indexer 最低质押金额
        #[pallet::constant]
        type MinIndexerStake: Get<BalanceOf<Self>>;

        /// 最大 Indexer 节点数
        #[pallet::constant]
        type MaxIndexers: Get<u32>;

        /// Indexer 宽限期（区块数）：无 hint 时等待 Indexer 多久再降级
        #[pallet::constant]
        type IndexerGracePeriod: Get<u32>;

        /// 成功 hint 奖励金额
        #[pallet::constant]
        type IndexerHintReward: Get<BalanceOf<Self>>;

        /// Indexer 自动暂停的错误阈值
        #[pallet::constant]
        type MaxIndexerErrors: Get<u32>;

        /// Indexer 暂停时质押 slash 比例（bps，10000 = 100%）
        ///
        /// 暂停触发时从 reserved stake 中扣除此比例转入国库，
        /// 提高恶意 Indexer 的经济成本。设为 0 则不 slash。
        #[pallet::constant]
        type IndexerSlashRateBps: Get<u16>;

        /// on_idle 每区块最多处理的逾期罚金交易数
        #[pallet::constant]
        type MaxPenaltyTradesPerBlock: Get<u32>;
    }

    /// 当前存储版本
    /// v3: TwapAccumulator 双 checkpoint + first_trade_block
    const STORAGE_VERSION: frame_support::traits::StorageVersion =
        frame_support::traits::StorageVersion::new(3);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ==================== Genesis ====================

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        /// 初始 NEX/USDT 价格（精度 10^6，例 1 = 0.000001 USDT/NEX）
        /// None = 不预设价格（需链上调用 set_initial_price）
        pub initial_price: Option<u64>,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            if let Some(price) = self.initial_price {
                if price > 0 {
                    PriceProtectionStore::<T>::mutate(|maybe| {
                        let config = maybe.get_or_insert_with(Default::default);
                        config.initial_price = Some(price);
                    });

                    // on_idle 需要 trade_count > 0 才刷新 current_block
                    TwapAccumulatorStore::<T>::put(TwapAccumulator::new_at(0, price, 1));

                    LastTradePrice::<T>::put(price);
                }
            }
        }
    }

    // ==================== Hooks ====================

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// 存储迁移 — 检查版本并执行必要的迁移
        ///
        /// 当前版本: v3 (TwapAccumulator 双 checkpoint + first_trade_block)
        fn on_runtime_upgrade() -> Weight {
            let on_chain = Pallet::<T>::on_chain_storage_version();
            let current = Pallet::<T>::in_code_storage_version();

            if on_chain == current {
                return Weight::zero();
            }

            log::info!(
                target: "pallet-nex-market",
                "Running storage migration from {:?} to {:?}",
                on_chain,
                current,
            );

            let mut weight = T::DbWeight::get().reads(1);

            if on_chain < 3 {
                weight = weight.saturating_add(crate::migrations::v3::migrate::<T>());
            }

            current.put::<Pallet<T>>();
            weight.saturating_add(T::DbWeight::get().writes(1))
        }

        fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let db_weight = T::DbWeight::get();
            let base_weight = db_weight.reads_writes(1, 1);
            if remaining_weight.all_lt(base_weight) {
                return Weight::zero();
            }

            // 仅在有 TWAP 数据时刷新
            let did_work = TwapAccumulatorStore::<T>::mutate(|maybe_acc| {
                let acc = match maybe_acc.as_mut() {
                    Some(a) if a.trade_count > 0 => a,
                    _ => return false,
                };

                let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
                let blocks_elapsed = current_block.saturating_sub(acc.current_block);
                if blocks_elapsed == 0 {
                    return false;
                }

                // 用 last_price 填充空白区间的 cumulative
                acc.current_cumulative = acc.current_cumulative.saturating_add(
                    (acc.last_price as u128).saturating_mul(blocks_elapsed as u128),
                );
                acc.current_block = current_block;

                // 推进 snapshots
                Self::advance_snapshots(acc, current_block);
                true
            });

            let mut consumed = if did_work {
                base_weight
            } else {
                db_weight.reads(1)
            };

            // 🆕 H3: 过期订单 GC — 每个 on_idle 清理有限数量的过期订单
            let gc_per_order = db_weight.reads_writes(1, 3); // read order + write order + write order_book + write user_orders
            let max_gc = T::MaxExpiredOrdersPerBlock::get();
            let now = <frame_system::Pallet<T>>::block_number();
            let mut gc_count: u32 = 0;

            // 清理过期卖单
            let sell_ids = SellOrders::<T>::get();
            let mut expired_sell_ids = Vec::new();
            for &id in sell_ids.iter() {
                if gc_count >= max_gc {
                    break;
                }
                if remaining_weight.all_lt(consumed.saturating_add(gc_per_order)) {
                    break;
                }
                if let Some(order) = Orders::<T>::get(id) {
                    if now > order.expires_at
                        && (order.status == OrderStatus::Open
                            || order.status == OrderStatus::PartiallyFilled)
                    {
                        // 退还未成交锁定资产
                        let unfilled = order.nex_amount.saturating_sub(order.filled_amount);
                        if !unfilled.is_zero() {
                            T::Currency::unreserve(&order.maker, unfilled);
                        }
                        let mut closed = order;
                        closed.status = OrderStatus::Expired;
                        Orders::<T>::insert(id, &closed);
                        UserOrders::<T>::mutate(&closed.maker, |orders| {
                            orders.retain(|&oid| oid != id);
                        });
                        expired_sell_ids.push(id);
                        gc_count += 1;
                        consumed = consumed.saturating_add(gc_per_order);
                    }
                }
            }
            if !expired_sell_ids.is_empty() {
                SellOrders::<T>::mutate(|orders| {
                    orders.retain(|id| !expired_sell_ids.contains(id));
                });
            }

            // 清理过期买单
            let buy_ids = BuyOrders::<T>::get();
            let mut expired_buy_ids = Vec::new();
            for &id in buy_ids.iter() {
                if gc_count >= max_gc {
                    break;
                }
                if remaining_weight.all_lt(consumed.saturating_add(gc_per_order)) {
                    break;
                }
                if let Some(order) = Orders::<T>::get(id) {
                    if now > order.expires_at
                        && (order.status == OrderStatus::Open
                            || order.status == OrderStatus::PartiallyFilled)
                    {
                        if !order.buyer_deposit.is_zero() {
                            T::Currency::unreserve(&order.maker, order.buyer_deposit);
                        }
                        let mut closed = order;
                        closed.status = OrderStatus::Expired;
                        Orders::<T>::insert(id, &closed);
                        UserOrders::<T>::mutate(&closed.maker, |orders| {
                            orders.retain(|&oid| oid != id);
                        });
                        expired_buy_ids.push(id);
                        gc_count += 1;
                        consumed = consumed.saturating_add(gc_per_order);
                    }
                }
            }
            if !expired_buy_ids.is_empty() {
                BuyOrders::<T>::mutate(|orders| {
                    orders.retain(|id| !expired_buy_ids.contains(id));
                });
            }

            // GC 后若有清理，刷新 best prices
            if gc_count > 0 {
                Self::refresh_best_prices();
                consumed = consumed.saturating_add(db_weight.reads_writes(2, 2));
            }

            // 🆕 保证金逾期罚金：AwaitingPayment 超宽限期后按小时扣除保证金
            let penalty_per_trade = db_weight.reads_writes(2, 2); // read trade + write trade + repatriate×2
            let grace_blocks: BlockNumberFor<T> = T::DepositPenaltyGracePeriod::get().into();
            let blocks_per_hour: BlockNumberFor<T> = T::BlocksPerHour::get().into();
            let penalty_rate_per_hour: u16 = T::DepositPenaltyRatePerHour::get();
            let max_penalty_trades: u32 = T::MaxPenaltyTradesPerBlock::get();

            if penalty_rate_per_hour > 0 && !blocks_per_hour.is_zero() {
                let awaiting_trades = AwaitingPaymentTrades::<T>::get();
                let mut penalty_count: u32 = 0;
                let mut auto_close_ids: Vec<u64> = Vec::new();

                for &trade_id in awaiting_trades.iter() {
                    if penalty_count >= max_penalty_trades {
                        break;
                    }
                    if remaining_weight.all_lt(consumed.saturating_add(penalty_per_trade)) {
                        break;
                    }

                    if let Some(mut trade) = UsdtTrades::<T>::get(trade_id) {
                        if trade.status != UsdtTradeStatus::AwaitingPayment {
                            continue;
                        }
                        if trade.buyer_deposit.is_zero()
                            || trade.deposit_status != BuyerDepositStatus::Locked
                        {
                            continue;
                        }

                        let elapsed = now.saturating_sub(trade.created_at);
                        if elapsed <= grace_blocks {
                            continue;
                        }

                        // 计算已过的完整小时数（宽限期之后）
                        let penalty_elapsed = elapsed.saturating_sub(grace_blocks);
                        let penalty_elapsed_u128: u128 = penalty_elapsed.saturated_into();
                        let bph_u128: u128 = blocks_per_hour.saturated_into();
                        let hourly_ticks_u128 = penalty_elapsed_u128.saturating_div(bph_u128);
                        if hourly_ticks_u128 == 0 {
                            continue;
                        }

                        // 目标总罚金 = deposit × min(hourly_ticks × rate, 10000) / 10000
                        let total_rate = (hourly_ticks_u128
                            .saturating_mul(penalty_rate_per_hour as u128))
                        .min(10000u128);
                        let deposit_u128: u128 = trade.buyer_deposit.saturated_into();
                        let target_penalty_u128 = deposit_u128
                            .saturating_mul(total_rate)
                            .saturating_div(10000);
                        let target_penalty: BalanceOf<T> = target_penalty_u128.saturated_into();

                        // 本轮需扣 = 目标总罚金 - 已累计扣除
                        let new_penalty = target_penalty.saturating_sub(trade.cumulative_penalty);
                        if new_penalty.is_zero() && total_rate < 10000 {
                            continue;
                        }

                        if !new_penalty.is_zero() {
                            // 三方分配：50% 卖家 + 平台份额拆分（国库 + 奖池）
                            let (
                                actually_penalized,
                                actually_to_treasury,
                                actually_to_seller,
                                actually_to_pool,
                            ) = Self::distribute_deposit_penalty(
                                &trade.buyer,
                                &trade.seller,
                                new_penalty,
                            );

                            trade.cumulative_penalty =
                                trade.cumulative_penalty.saturating_add(actually_penalized);
                            UsdtTrades::<T>::insert(trade_id, &trade);

                            Self::deposit_event(Event::DepositPenaltyCharged {
                                trade_id,
                                buyer: trade.buyer.clone(),
                                penalty_this_round: actually_penalized,
                                cumulative_penalty: trade.cumulative_penalty,
                                to_treasury: actually_to_treasury,
                                to_seller: actually_to_seller,
                                to_pool: actually_to_pool,
                            });
                        }

                        // 保证金全部扣完 → 自动关闭交易
                        if trade.cumulative_penalty >= trade.buyer_deposit {
                            // 退还卖家 NEX
                            T::Currency::unreserve(&trade.seller, trade.nex_amount);
                            Self::rollback_order_filled_amount(trade.order_id, trade.nex_amount);

                            // 退还买家剩余 reserved（如果 repatriate 有残余）
                            let remaining_reserved =
                                trade.buyer_deposit.saturating_sub(trade.cumulative_penalty);
                            if !remaining_reserved.is_zero() {
                                T::Currency::unreserve(&trade.buyer, remaining_reserved);
                            }

                            let buyer_clone = trade.buyer.clone();
                            let seller_clone = trade.seller.clone();
                            let total_penalized = trade.cumulative_penalty;

                            trade.status = UsdtTradeStatus::Refunded;
                            trade.deposit_status = BuyerDepositStatus::Forfeited;
                            trade.completed_at = Some(now);
                            UsdtTrades::<T>::insert(trade_id, &trade);

                            Self::cleanup_waived_trade_counter(&trade.buyer, trade.order_id);
                            Self::credit_on_timeout_violation(&trade.buyer);
                            ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| {
                                *c = c.saturating_sub(1)
                            });
                            // P9: 清理 Indexer hint 存储
                            Self::cleanup_trade_hint(trade_id);

                            auto_close_ids.push(trade_id);

                            Self::deposit_event(Event::TradeAutoClosedByPenalty {
                                trade_id,
                                buyer: buyer_clone,
                                seller: seller_clone,
                                total_penalized,
                            });
                        }

                        penalty_count += 1;
                        consumed = consumed.saturating_add(penalty_per_trade);
                    }
                }

                // 清理已自动关闭的交易
                if !auto_close_ids.is_empty() {
                    AwaitingPaymentTrades::<T>::mutate(|list| {
                        list.retain(|id| !auto_close_ids.contains(id));
                    });
                }
            }

            // 🆕 M7: UsedTxHashes TTL 清理 — cursor-based 有界遍历
            // H-1 审计修复: 取 max(TxHashTtlBlocks, MinTxHashTtlBlocks) 防止过短 TTL 导致重放
            let configured_ttl = T::TxHashTtlBlocks::get();
            let min_ttl = T::MinTxHashTtlBlocks::get();
            let effective_ttl = configured_ttl.max(min_ttl);
            let ttl: BlockNumberFor<T> = effective_ttl.into();
            let gc_per_hash = db_weight.reads_writes(1, 1);
            let max_hash_gc: u32 = 10; // 每区块最多清理 10 条过期 tx_hash
            let mut hash_gc_count: u32 = 0;

            if remaining_weight.all_lt(consumed.saturating_add(gc_per_hash)) {
                return consumed;
            }

            let cursor = TxHashGcCursor::<T>::get();
            let mut iter = if let Some(ref start) = cursor {
                UsedTxHashes::<T>::iter_from(UsedTxHashes::<T>::hashed_key_for(start))
            } else {
                UsedTxHashes::<T>::iter()
            };

            let mut last_key: Option<TxHash> = None;
            let mut exhausted = true;

            while hash_gc_count < max_hash_gc {
                if remaining_weight.all_lt(consumed.saturating_add(gc_per_hash)) {
                    exhausted = false;
                    break;
                }
                match iter.next() {
                    Some((key, (_trade_id, inserted_at))) => {
                        consumed = consumed.saturating_add(gc_per_hash);
                        if now > inserted_at.saturating_add(ttl) {
                            UsedTxHashes::<T>::remove(&key);
                            hash_gc_count += 1;
                        }
                        last_key = Some(key);
                    }
                    None => {
                        exhausted = true;
                        break;
                    }
                }
            }

            if exhausted {
                TxHashGcCursor::<T>::kill();
            } else if let Some(key) = last_key {
                TxHashGcCursor::<T>::put(key);
            }

            consumed
        }

        fn offchain_worker(block_number: BlockNumberFor<T>) {
            // 节点启动后首次 OCW 运行时，从本地预设引导 API Key（幂等）
            pallet_trading_trc20_verifier::bootstrap_api_keys();

            let has_indexers = IndexerCount::<T>::get() > 0;
            let grace_period: BlockNumberFor<T> = T::IndexerGracePeriod::get().into();

            // ── 1. 处理 AwaitingVerification 的交易（四层降级）──
            let pending = PendingUsdtTrades::<T>::get();
            if !pending.is_empty() {
                log::info!(target: "nex-market-ocw",
                    "Processing {} pending USDT trades at block {:?}", pending.len(), block_number);

                for trade_id in pending.iter() {
                    // 防重复：每笔交易每 10 个区块（60s）才调用一次
                    // TRON 确认需要 ≥114s，18s 查一次前 6 次必定失败且浪费 API 配额
                    if Self::ocw_recently_checked(
                        b"nex-verify",
                        *trade_id,
                        block_number,
                        10u32.into(),
                    ) {
                        continue;
                    }
                    if let Some(trade) = UsdtTrades::<T>::get(trade_id) {
                        if trade.status == UsdtTradeStatus::AwaitingVerification {
                            // Layer 1+2: 检查 Indexer hint
                            if let Some(hint) = IndexerHints::<T>::get(trade_id) {
                                if hint.status == IndexerHintStatus::Pending {
                                    Self::cross_verify_hint(*trade_id, &trade, &hint);
                                    Self::ocw_mark_checked(b"nex-verify", *trade_id, block_number);
                                    continue;
                                }
                            }

                            // Layer 3: 无 hint → 检查宽限期
                            if has_indexers {
                                let age = block_number.saturating_sub(trade.created_at);
                                if age < grace_period {
                                    continue; // 还在等 Indexer
                                }
                            }

                            // Layer 3 fallback / Layer 4: 现有完整 TronGrid 查询
                            if let Some(ref buyer_tron) = trade.buyer_tron_address {
                                Self::process_verification(
                                    *trade_id,
                                    &trade,
                                    buyer_tron.as_slice(),
                                    trade.seller_tron_address.as_slice(),
                                );
                                Self::ocw_mark_checked(b"nex-verify", *trade_id, block_number);
                            }
                        }
                    }
                }
            }

            // ── 2. 扫描 UnderpaidPending（补付窗口内持续检查新转账）──
            let underpaid = PendingUnderpaidTrades::<T>::get();
            if !underpaid.is_empty() {
                for trade_id in underpaid.iter() {
                    // 防重复：每笔交易每 10 个区块（60s）才调用一次，与 Phase 1 一致
                    if Self::ocw_recently_checked(
                        b"nex-underpaid",
                        *trade_id,
                        block_number,
                        10u32.into(),
                    ) {
                        continue;
                    }
                    if let Some(trade) = UsdtTrades::<T>::get(trade_id) {
                        if trade.status == UsdtTradeStatus::UnderpaidPending {
                            if let Some(ref buyer_tron) = trade.buyer_tron_address {
                                Self::check_underpaid_topup(
                                    *trade_id,
                                    &trade,
                                    buyer_tron.as_slice(),
                                    trade.seller_tron_address.as_slice(),
                                );
                                Self::ocw_mark_checked(b"nex-underpaid", *trade_id, block_number);
                            }
                        }
                    }
                }
            }

            // ── 3. 预检扫描 AwaitingPayment（四层降级 + 买家兜底）──
            let awaiting = AwaitingPaymentTrades::<T>::get();
            if awaiting.is_empty() {
                return;
            }

            // 冷却 = BlocksPerHour（1 小时），与保证金罚金刻度同步
            let await_cooldown: BlockNumberFor<T> = T::BlocksPerHour::get().into();
            // 起查阈值 = DepositPenaltyGracePeriod（2 小时），与罚金宽限期对齐
            let await_start_threshold: BlockNumberFor<T> =
                T::DepositPenaltyGracePeriod::get().into();

            for trade_id in awaiting.iter() {
                // 防重复：每笔交易每 1 小时才调用一次（与罚金刻度同步）
                if Self::ocw_recently_checked(b"nex-await", *trade_id, block_number, await_cooldown)
                {
                    continue;
                }
                if let Some(trade) = UsdtTrades::<T>::get(trade_id) {
                    if trade.status != UsdtTradeStatus::AwaitingPayment {
                        continue;
                    }

                    // Layer 1+2: 检查 Indexer hint
                    if let Some(hint) = IndexerHints::<T>::get(trade_id) {
                        if hint.status == IndexerHintStatus::Pending {
                            Self::cross_verify_hint(*trade_id, &trade, &hint);
                            Self::ocw_mark_checked(b"nex-await", *trade_id, block_number);
                            continue;
                        }
                    }

                    // Layer 3: 无 hint → 检查宽限期
                    if has_indexers {
                        let age = block_number.saturating_sub(trade.created_at);
                        if age < grace_period {
                            continue;
                        }
                    }

                    // 前 2 小时不查询（等买家操作 / 与保证金罚金宽限期对齐）
                    // 2h 后首次查询，之后每 1h 查一次（冷却控制）
                    let elapsed = block_number.saturating_sub(trade.created_at);
                    if elapsed < await_start_threshold {
                        continue;
                    }
                    if let Some(ref buyer_tron) = trade.buyer_tron_address {
                        Self::auto_check_awaiting_payment(
                            *trade_id,
                            &trade,
                            buyer_tron.as_slice(),
                            trade.seller_tron_address.as_slice(),
                        );
                        Self::ocw_mark_checked(b"nex-await", *trade_id, block_number);
                    }
                }
            }
        }
    }

    // ==================== 存储 ====================

    #[pallet::storage]
    #[pallet::getter(fn next_order_id)]
    pub type NextOrderId<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn orders)]
    pub type Orders<T: Config> = StorageMap<_, Blake2_128Concat, u64, Order<T>>;

    /// 卖单索引（卖 NEX 收 USDT）
    #[pallet::storage]
    #[pallet::getter(fn sell_orders)]
    pub type SellOrders<T: Config> = StorageValue<_, BoundedVec<u64, T::MaxSellOrders>, ValueQuery>;

    /// 买单索引（买 NEX 付 USDT）
    #[pallet::storage]
    #[pallet::getter(fn buy_orders)]
    pub type BuyOrders<T: Config> = StorageValue<_, BoundedVec<u64, T::MaxBuyOrders>, ValueQuery>;

    /// 用户订单索引
    #[pallet::storage]
    #[pallet::getter(fn user_orders)]
    pub type UserOrders<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<u64, ConstU32<100>>, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn next_usdt_trade_id)]
    pub type NextUsdtTradeId<T> = StorageValue<_, u64, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn usdt_trades)]
    pub type UsdtTrades<T: Config> = StorageMap<_, Blake2_128Concat, u64, UsdtTrade<T>>;

    /// OCW 待验证队列（AwaitingVerification 状态）
    /// 🆕 H2: 容量改为 Config::MaxPendingTrades 可配置
    #[pallet::storage]
    #[pallet::getter(fn pending_usdt_trades)]
    pub type PendingUsdtTrades<T: Config> =
        StorageValue<_, BoundedVec<u64, T::MaxPendingTrades>, ValueQuery>;

    /// 待付款跟踪队列（OCW 预检扫描 AwaitingPayment 状态）
    /// 🆕 H2: 容量改为 Config::MaxAwaitingPaymentTrades 可配置
    #[pallet::storage]
    #[pallet::getter(fn awaiting_payment_trades)]
    pub type AwaitingPaymentTrades<T: Config> =
        StorageValue<_, BoundedVec<u64, T::MaxAwaitingPaymentTrades>, ValueQuery>;

    /// 少付补付跟踪队列
    /// 🆕 H2: 容量改为 Config::MaxUnderpaidTrades 可配置
    #[pallet::storage]
    #[pallet::getter(fn pending_underpaid_trades)]
    pub type PendingUnderpaidTrades<T: Config> =
        StorageValue<_, BoundedVec<u64, T::MaxUnderpaidTrades>, ValueQuery>;

    /// OCW 验证结果
    #[pallet::storage]
    #[pallet::getter(fn ocw_verification_results)]
    pub type OcwVerificationResults<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, (PaymentVerificationResult, u64), OptionQuery>;

    /// 最优卖价 (USDT/NEX)
    #[pallet::storage]
    #[pallet::getter(fn best_ask)]
    pub type BestAsk<T> = StorageValue<_, u64>;

    /// 最优买价
    #[pallet::storage]
    #[pallet::getter(fn best_bid)]
    pub type BestBid<T> = StorageValue<_, u64>;

    /// 最新成交价
    #[pallet::storage]
    #[pallet::getter(fn last_trade_price)]
    pub type LastTradePrice<T> = StorageValue<_, u64>;

    /// 市场统计
    #[pallet::storage]
    #[pallet::getter(fn market_stats)]
    pub type MarketStatsStore<T> = StorageValue<_, MarketStats, ValueQuery>;

    /// TWAP 累积器
    #[pallet::storage]
    #[pallet::getter(fn twap_accumulator)]
    pub type TwapAccumulatorStore<T> = StorageValue<_, TwapAccumulator>;

    /// 价格保护配置
    #[pallet::storage]
    #[pallet::getter(fn price_protection)]
    pub type PriceProtectionStore<T> = StorageValue<_, PriceProtectionConfig>;

    /// 已完成首笔交易的买家（L3 Sybil 防御：完成后不再享受免保证金）
    #[pallet::storage]
    pub type CompletedBuyers<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    /// 🆕 C3: 已使用的 TRON 交易哈希（防重放攻击）
    /// 同一 TRON tx 不能用于多笔 nex-market 交易的支付证明
    /// 🆕 M7: 值改为 (trade_id, inserted_at_block) 用于 TTL 清理
    #[pallet::storage]
    #[pallet::getter(fn used_tx_hashes)]
    pub type UsedTxHashes<T: Config> =
        StorageMap<_, Blake2_128Concat, TxHash, (u64, BlockNumberFor<T>)>;

    /// 🆕 M7: UsedTxHashes GC 游标（cursor-based 遍历避免全表扫描）
    #[pallet::storage]
    pub type TxHashGcCursor<T: Config> = StorageValue<_, TxHash>;

    /// 买家当前活跃的免保证金交易数（L2 防 grief：每账户最多 1 笔）
    #[pallet::storage]
    pub type ActiveWaivedTrades<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    /// 用户黑名单（true = 已封禁）
    #[pallet::storage]
    #[pallet::getter(fn is_banned)]
    pub type BannedAccounts<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

    /// 市场紧急暂停标志（#6 紧急暂停）
    #[pallet::storage]
    #[pallet::getter(fn market_paused)]
    pub type MarketPausedStore<T> = StorageValue<_, bool, ValueQuery>;

    /// 用户交易索引（#2/#12 交易历史查询）
    #[pallet::storage]
    #[pallet::getter(fn user_trades)]
    pub type UserTrades<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u64, T::MaxTradesPerUser>,
        ValueQuery,
    >;

    /// 订单关联交易索引（#5 订单-交易关联查询）
    #[pallet::storage]
    #[pallet::getter(fn order_trades)]
    pub type OrderTrades<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BoundedVec<u64, T::MaxOrderTrades>, ValueQuery>;

    /// 交易争议（#1 争议仲裁）
    #[pallet::storage]
    #[pallet::getter(fn trade_disputes)]
    pub type TradeDisputeStore<T: Config> = StorageMap<_, Blake2_128Concat, u64, TradeDispute<T>>;

    /// 交易手续费率（bps，#9 手续费）
    #[pallet::storage]
    #[pallet::getter(fn trading_fee_bps)]
    pub type TradingFeeBps<T> = StorageValue<_, u16, ValueQuery>;

    /// Indexer 奖池分成比例（bps，从 trading_fee 中划拨，最大 10000 = 100%）
    #[pallet::storage]
    #[pallet::getter(fn indexer_pool_share_bps)]
    pub type IndexerPoolShareBps<T> = StorageValue<_, u16, ValueQuery>;

    /// 种子 Tron 收款地址（链上可治理修改，未设置时回退到 Config 默认值）
    #[pallet::storage]
    #[pallet::getter(fn seed_tron_address)]
    pub type SeedTronAddressStore<T: Config> = StorageValue<_, TronAddress>;

    /// 买家信用档案
    #[pallet::storage]
    #[pallet::getter(fn buyer_credit_profiles)]
    pub type BuyerCreditProfiles<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BuyerCreditProfile<BlockNumberFor<T>>,
        OptionQuery,
    >;

    /// 买家活跃交易计数
    #[pallet::storage]
    #[pallet::getter(fn active_buyer_trades)]
    pub type ActiveBuyerTrades<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

    // ==================== Indexer 存储 ====================

    /// 已注册 Indexer 集合
    #[pallet::storage]
    #[pallet::getter(fn indexer_set)]
    pub type IndexerSet<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, IndexerInfo<T>>;

    /// 活跃 Indexer 数量
    #[pallet::storage]
    #[pallet::getter(fn indexer_count)]
    pub type IndexerCount<T> = StorageValue<_, u32, ValueQuery>;

    /// 每笔 trade 的 Indexer hint（先到先得，一个 trade 一个 hint 槽位）
    #[pallet::storage]
    #[pallet::getter(fn indexer_hints)]
    pub type IndexerHints<T: Config> = StorageMap<_, Blake2_128Concat, u64, IndexerHint<T>>;

    // ==================== Events ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 订单已创建
        OrderCreated {
            order_id: u64,
            maker: T::AccountId,
            side: OrderSide,
            nex_amount: BalanceOf<T>,
            usdt_price: u64,
        },
        /// 订单已取消
        OrderCancelled { order_id: u64 },
        /// USDT 交易已创建
        UsdtTradeCreated {
            trade_id: u64,
            order_id: u64,
            seller: T::AccountId,
            buyer: T::AccountId,
            nex_amount: BalanceOf<T>,
            usdt_amount: u64,
        },
        /// USDT 支付已确认（买家声明已付款）
        UsdtPaymentSubmitted { trade_id: u64 },
        /// USDT 交易已完成
        UsdtTradeCompleted { trade_id: u64, order_id: u64 },
        /// 验证失败
        UsdtTradeVerificationFailed { trade_id: u64, reason: Vec<u8> },
        /// 超时退款（AwaitingPayment 阶段超时）
        UsdtTradeRefunded { trade_id: u64 },
        /// 验证超时退款（AwaitingVerification 阶段超时，宽限期后仍无结果）
        VerificationTimeoutRefunded {
            trade_id: u64,
            buyer: T::AccountId,
            seller: T::AccountId,
            usdt_amount: u64,
        },
        /// TWAP 已更新
        TwapUpdated {
            new_price: u64,
            twap_1h: Option<u64>,
            twap_24h: Option<u64>,
            twap_7d: Option<u64>,
        },
        /// 熔断触发
        CircuitBreakerTriggered {
            current_price: u64,
            twap_7d: u64,
            deviation_bps: u16,
            until_block: u32,
        },
        /// 熔断解除
        CircuitBreakerLifted,
        /// 价格保护已配置
        PriceProtectionConfigured { enabled: bool, max_deviation: u16 },
        /// 初始价格已设置
        InitialPriceSet { initial_price: u64 },
        /// OCW 验证结果已提交
        OcwResultSubmitted {
            trade_id: u64,
            verification_result: PaymentVerificationResult,
            actual_amount: u64,
        },
        /// 少付自动处理
        UnderpaidAutoProcessed {
            trade_id: u64,
            expected_amount: u64,
            actual_amount: u64,
            payment_ratio: u32,
            nex_released: BalanceOf<T>,
            deposit_forfeited: BalanceOf<T>,
        },
        /// 保证金已锁定
        BuyerDepositLocked {
            trade_id: u64,
            buyer: T::AccountId,
            deposit: BalanceOf<T>,
        },
        /// 保证金已退还
        BuyerDepositReleased {
            trade_id: u64,
            buyer: T::AccountId,
            deposit: BalanceOf<T>,
        },
        /// 保证金已没收
        BuyerDepositForfeited {
            trade_id: u64,
            buyer: T::AccountId,
            forfeited: BalanceOf<T>,
            to_treasury: BalanceOf<T>,
            to_seller: BalanceOf<T>,
            to_pool: BalanceOf<T>,
        },
        /// 验证奖励已领取
        VerificationRewardClaimed {
            trade_id: u64,
            claimer: T::AccountId,
            reward: BalanceOf<T>,
            reward_paid: bool,
        },
        /// 流动性种子已注入（seed_liquidity）
        LiquiditySeeded {
            order_count: u32,
            total_nex: BalanceOf<T>,
            source: T::AccountId,
        },
        /// 种子账户已注资（国库 → 种子账户）
        SeedAccountFunded {
            amount: BalanceOf<T>,
            treasury: T::AccountId,
            seed_account: T::AccountId,
        },
        /// 免保证金交易已创建
        WaivedDepositTradeCreated {
            trade_id: u64,
            buyer: T::AccountId,
            nex_amount: BalanceOf<T>,
        },
        /// OCW 自动检测到 USDT 到账（买家未手动确认）
        AutoPaymentDetected { trade_id: u64, actual_amount: u64 },
        /// 少付检测到，进入补付窗口
        UnderpaidDetected {
            trade_id: u64,
            expected_amount: u64,
            actual_amount: u64,
            payment_ratio: u32,
            deadline: BlockNumberFor<T>,
        },
        /// 补付窗口内金额已更新
        UnderpaidAmountUpdated {
            trade_id: u64,
            previous_amount: u64,
            new_amount: u64,
        },
        /// 少付终裁完成
        UnderpaidFinalized {
            trade_id: u64,
            final_amount: u64,
            payment_ratio: u32,
            deposit_forfeit_rate: u16,
        },
        /// 市场紧急暂停
        MarketPaused,
        /// 市场恢复交易
        MarketResumed,
        /// 管理员强制结算交易
        TradeForceSettled {
            trade_id: u64,
            actual_amount: u64,
            resolution: DisputeResolution,
        },
        /// 管理员强制取消交易
        TradeForceCancelled { trade_id: u64 },
        /// 交易争议已发起
        TradeDisputed {
            trade_id: u64,
            initiator: T::AccountId,
            evidence_cid: Vec<u8>,
        },
        /// 争议已解决
        DisputeResolved {
            trade_id: u64,
            resolution: DisputeResolution,
        },
        /// 交易手续费已更新
        TradingFeeUpdated { old_fee_bps: u16, new_fee_bps: u16 },
        /// 订单价格已修改
        OrderPriceUpdated {
            order_id: u64,
            old_price: u64,
            new_price: u64,
        },
        /// 队列满自动暂停市场
        QueueOverflowPaused {
            pending_count: u32,
            max_capacity: u32,
        },
        /// 手续费已收取
        TradingFeeCharged {
            trade_id: u64,
            fee_amount: BalanceOf<T>,
            to_treasury: T::AccountId,
        },
        /// 卖家手动确认收款
        SellerConfirmedReceived {
            trade_id: u64,
            seller: T::AccountId,
            buyer: T::AccountId,
        },
        /// 用户已封禁
        UserBanned { account: T::AccountId },
        /// 用户已解封
        UserUnbanned { account: T::AccountId },
        /// 对方反驳证据已提交
        CounterEvidenceSubmitted {
            trade_id: u64,
            party: T::AccountId,
            evidence_cid: Vec<u8>,
        },
        /// 订单数量已修改
        OrderAmountUpdated {
            order_id: u64,
            old_amount: BalanceOf<T>,
            new_amount: BalanceOf<T>,
        },
        /// 批量强制结算完成
        BatchForceSettled {
            settled_count: u32,
            failed_count: u32,
        },
        /// 批量强制取消完成
        BatchForceCancelled {
            cancelled_count: u32,
            failed_count: u32,
        },
        /// 种子 Tron 收款地址已更新
        SeedTronAddressUpdated { new_address: TronAddress },
        /// 买家信用分变更
        CreditScoreUpdated {
            buyer: T::AccountId,
            old_score: u16,
            new_score: u16,
            reason: CreditChangeReason,
        },
        /// 买家因信用自动暂停
        BuyerSuspended {
            buyer: T::AccountId,
            credit_score: u16,
            until: Option<BlockNumberFor<T>>,
        },
        /// 买家暂停解除
        BuyerSuspensionLifted {
            buyer: T::AccountId,
            credit_score: u16,
        },
        /// 买家因信用为零永久封禁
        BuyerPermanentlyBanned { buyer: T::AccountId },
        /// 保证金逾期罚金已扣除（每小时结算一次）
        DepositPenaltyCharged {
            trade_id: u64,
            buyer: T::AccountId,
            penalty_this_round: BalanceOf<T>,
            cumulative_penalty: BalanceOf<T>,
            to_treasury: BalanceOf<T>,
            to_seller: BalanceOf<T>,
            to_pool: BalanceOf<T>,
        },
        /// 保证金因逾期罚金全部扣完，交易自动取消
        TradeAutoClosedByPenalty {
            trade_id: u64,
            buyer: T::AccountId,
            seller: T::AccountId,
            total_penalized: BalanceOf<T>,
        },
        /// Indexer 已注册
        IndexerRegistered {
            indexer: T::AccountId,
            stake: BalanceOf<T>,
        },
        /// Indexer 已退出
        IndexerDeregistered {
            indexer: T::AccountId,
            stake_returned: BalanceOf<T>,
        },
        /// Indexer 提交了 hint
        IndexerHintSubmitted {
            trade_id: u64,
            indexer: T::AccountId,
            tx_hash: TxHash,
            reported_amount: u64,
        },
        /// Indexer hint 已验证通过
        IndexerHintVerified {
            trade_id: u64,
            indexer: T::AccountId,
            reward: BalanceOf<T>,
        },
        /// Indexer hint 不匹配
        IndexerHintMismatch {
            trade_id: u64,
            indexer: T::AccountId,
        },
        /// Indexer 因错误过多被暂停
        IndexerSuspended {
            indexer: T::AccountId,
            error_count: u32,
        },
        /// Indexer 被管理员强制移除
        IndexerForceRemoved { indexer: T::AccountId },
        /// 国库余额不足，Indexer 奖励未发放
        IndexerRewardPoolDepleted {
            trade_id: u64,
            indexer: T::AccountId,
            reward_missed: BalanceOf<T>,
        },
        /// Indexer 奖池分成比例已更新
        IndexerPoolShareUpdated {
            old_share_bps: u16,
            new_share_bps: u16,
        },
        /// 手续费已划拨至 Indexer 奖池
        IndexerPoolFunded {
            trade_id: u64,
            amount: BalanceOf<T>,
            pool_account: T::AccountId,
        },
        /// Indexer 暂停已解除（管理员操作）
        IndexerReinstated { indexer: T::AccountId },
        /// Indexer 质押被 slash（因暂停触发）
        IndexerStakeSlashed {
            indexer: T::AccountId,
            slashed: BalanceOf<T>,
            remaining_stake: BalanceOf<T>,
        },
        /// Indexer 强制移除时清理了孤儿 hints
        IndexerOrphanHintsCleaned {
            indexer: T::AccountId,
            cleaned_count: u32,
        },
        /// M-4 审计修复: 买家信用已被管理员重置
        CreditReset {
            account: T::AccountId,
            old_score: u16,
            new_score: u16,
        },
    }

    // ==================== Errors ====================

    #[pallet::error]
    pub enum Error<T> {
        /// 订单不存在
        OrderNotFound,
        /// 不是订单所有者
        NotOrderOwner,
        /// 订单已关闭
        OrderClosed,
        /// 余额不足
        InsufficientBalance,
        /// 数量过小
        AmountTooSmall,
        /// 价格为零
        ZeroPrice,
        /// 订单簿已满
        OrderBookFull,
        /// 用户订单数已满
        UserOrdersFull,
        /// 不能吃自己的单
        CannotTakeOwnOrder,
        /// 算术溢出
        ArithmeticOverflow,
        /// 价格预言机不可用（无法计算保证金）
        OracleUnavailable,
        /// 订单方向不匹配
        OrderSideMismatch,
        /// TRON 地址无效
        InvalidTronAddress,
        /// USDT 交易不存在
        UsdtTradeNotFound,
        /// 不是交易参与者
        NotTradeParticipant,
        /// 交易状态无效
        InvalidTradeStatus,
        /// 交易已超时
        TradeTimeout,
        /// 待验证队列已满
        PendingQueueFull,
        /// 价格偏离过大
        PriceDeviationTooHigh,
        /// 市场熔断中
        MarketCircuitBreakerActive,
        /// OCW 验证结果不存在
        OcwResultNotFound,
        /// 保证金余额不足
        InsufficientDepositBalance,
        /// 基点参数无效
        InvalidBasisPoints,
        /// 免保证金首单超限（每账户仅 1 笔活跃）
        FirstOrderLimitReached,
        /// 首单金额超过上限
        FirstOrderAmountTooLarge,
        /// 该买家已完成过交易，不再享受免保证金
        BuyerAlreadyCompleted,
        /// seed_liquidity 订单数超限
        TooManySeedOrders,
        /// 无可用的基准价格（需先 set_initial_price）
        NoPriceReference,
        /// 仍在验证宽限期内，不允许超时
        StillInGracePeriod,
        /// 补付窗口尚未到期
        UnderpaidGraceNotExpired,
        /// 交易不在 UnderpaidPending 状态
        NotUnderpaidPending,
        /// 订单已过期
        OrderExpired,
        /// 熔断未激活（无需解除）
        CircuitBreakerNotActive,
        /// 🆕 L2修复: 熔断持续时间未到期
        CircuitBreakerNotExpired,
        /// 待付款跟踪队列已满
        AwaitingPaymentQueueFull,
        /// 少付跟踪队列已满
        UnderpaidQueueFull,
        /// 🆕 C3: TRON 交易哈希已被使用（防重放）
        TxHashAlreadyUsed,
        /// 市场已暂停，禁止交易操作
        MarketIsPaused,
        /// 交易已存在争议
        TradeAlreadyDisputed,
        /// 交易状态不可争议（仅 Refunded 的 VerificationTimeout 可争议）
        TradeNotDisputable,
        /// 争议不存在
        DisputeNotFound,
        /// 争议已解决
        DisputeAlreadyClosed,
        /// 手续费过高（最大 1000 bps = 10%）
        FeeTooHigh,
        /// 订单金额低于最低限额
        OrderAmountBelowMinimum,
        /// 订单有活跃交易，不能修改价格
        OrderHasActiveTrades,
        /// 汇率不能为零
        ZeroExchangeRate,
        /// 用户交易列表已满
        UserTradesFull,
        /// 订单交易列表已满
        OrderTradesFull,
        /// 用户已被封禁
        UserIsBanned,
        /// 争议时间窗口已过期
        DisputeWindowExpired,
        /// 订单金额超过最大限额
        OrderAmountTooLarge,
        /// 新数量低于已成交量
        AmountBelowFilledAmount,
        /// 低于卖家设置的最低吃单量
        BelowMinFillAmount,
        /// 不是交易参与方或管理员
        NotParticipantOrAdmin,
        /// 反驳证据已提交，不可覆盖
        CounterEvidenceAlreadySubmitted,
        /// 买家从未确认付款，不可争议
        PaymentNotConfirmed,
        /// tx_hash 为必填项，不能为空
        TxHashRequired,
        /// 买家因信用分过低被暂停交易
        BuyerSuspended,
        /// 买家并发交易数已达上限
        TradeLimitExceeded,
        /// Indexer 已注册（不可重复注册）
        IndexerAlreadyRegistered,
        /// Indexer 不存在
        IndexerNotFound,
        /// Indexer 已被暂停
        IndexerIsSuspended,
        /// Indexer 仍有待处理的 hint，不可退出
        IndexerHasPendingHints,
        /// Indexer 数量已达上限
        MaxIndexersReached,
        /// 该 trade 已有 hint（先到先得）
        HintAlreadyExists,
        /// hint 金额明显异常（超过预期 10 倍）
        HintAmountInsane,
        /// Indexer 质押不足
        InsufficientIndexerStake,
        /// Indexer 未被暂停（无需恢复）
        IndexerNotSuspended,
        /// Indexer 端点 URL 过长或格式无效
        InvalidEndpointUrl,
    }

    // ==================== Extrinsics ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 挂卖单（卖 NEX 收 USDT）
        ///
        /// 卖家锁定 NEX，提供 TRON 地址接收 USDT
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::place_sell_order())]
        pub fn place_sell_order(
            origin: OriginFor<T>,
            nex_amount: BalanceOf<T>,
            usdt_price: u64,
            tron_address: Vec<u8>,
            min_fill_amount: Option<BalanceOf<T>>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;
            Self::ensure_not_banned(&who)?;

            ensure!(usdt_price > 0, Error::<T>::ZeroPrice);
            ensure!(!nex_amount.is_zero(), Error::<T>::AmountTooSmall);
            ensure!(
                nex_amount >= T::MinOrderNexAmount::get(),
                Error::<T>::OrderAmountBelowMinimum
            );
            ensure!(
                nex_amount <= T::MaxOrderNexAmount::get(),
                Error::<T>::OrderAmountTooLarge
            );

            // 价格偏离检查
            Self::check_price_deviation(usdt_price)?;

            // 验证 TRON 地址（Base58Check 完整校验）
            ensure!(
                pallet_trading_common::is_valid_tron_address(&tron_address),
                Error::<T>::InvalidTronAddress
            );
            let tron_addr: TronAddress = tron_address
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;

            // 锁定 NEX
            T::Currency::reserve(&who, nex_amount).map_err(|_| Error::<T>::InsufficientBalance)?;

            let mfa = min_fill_amount.unwrap_or_else(Zero::zero);
            let order_id = Self::do_create_order(
                who.clone(),
                OrderSide::Sell,
                nex_amount,
                usdt_price,
                Some(tron_addr),
                Zero::zero(),
                false,
                mfa,
            )?;

            Self::update_best_price_on_new_order(usdt_price, OrderSide::Sell);

            Self::deposit_event(Event::OrderCreated {
                order_id,
                maker: who,
                side: OrderSide::Sell,
                nex_amount,
                usdt_price,
            });

            Ok(())
        }

        /// 挂买单（买 NEX 付 USDT）
        ///
        /// 买家声明愿意用 USDT 购买 NEX，无需锁定链上资产
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::place_buy_order())]
        pub fn place_buy_order(
            origin: OriginFor<T>,
            nex_amount: BalanceOf<T>,
            usdt_price: u64,
            buyer_tron_address: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;
            Self::ensure_not_banned(&who)?;
            Self::ensure_not_suspended(&who)?;

            ensure!(usdt_price > 0, Error::<T>::ZeroPrice);
            ensure!(!nex_amount.is_zero(), Error::<T>::AmountTooSmall);
            ensure!(
                nex_amount >= T::MinOrderNexAmount::get(),
                Error::<T>::OrderAmountBelowMinimum
            );
            ensure!(
                nex_amount <= T::MaxOrderNexAmount::get(),
                Error::<T>::OrderAmountTooLarge
            );

            // 验证买家 TRON 地址（Base58Check 完整校验）
            ensure!(
                pallet_trading_common::is_valid_tron_address(&buyer_tron_address),
                Error::<T>::InvalidTronAddress
            );
            let buyer_tron: TronAddress = buyer_tron_address
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;

            Self::check_price_deviation(usdt_price)?;

            // 计算并预锁定买家保证金
            let nex_u128: u128 = nex_amount.saturated_into();
            // 🆕 M2修复: 安全转换防止 u64 截断
            let usdt_total_u128 = nex_u128
                .checked_mul(usdt_price as u128)
                .ok_or(Error::<T>::ArithmeticOverflow)?
                .checked_div(1_000_000_000_000u128)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let usdt_total =
                u64::try_from(usdt_total_u128).map_err(|_| Error::<T>::ArithmeticOverflow)?;
            let base_deposit = Self::calculate_buyer_deposit(usdt_total)?;
            let buyer_deposit = Self::apply_credit_discount(&who, base_deposit);
            if !buyer_deposit.is_zero() {
                T::Currency::reserve(&who, buyer_deposit)
                    .map_err(|_| Error::<T>::InsufficientDepositBalance)?;
            }

            let order_id = Self::do_create_order(
                who.clone(),
                OrderSide::Buy,
                nex_amount,
                usdt_price,
                Some(buyer_tron),
                buyer_deposit,
                false,
                Zero::zero(),
            )?;

            Self::update_best_price_on_new_order(usdt_price, OrderSide::Buy);

            Self::deposit_event(Event::OrderCreated {
                order_id,
                maker: who,
                side: OrderSide::Buy,
                nex_amount,
                usdt_price,
            });

            Ok(())
        }

        /// 取消订单
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::cancel_order())]
        pub fn cancel_order(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let mut order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.maker == who, Error::<T>::NotOrderOwner);
            ensure!(
                order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
                Error::<T>::OrderClosed
            );

            // 退还未成交的锁定资产
            if order.side == OrderSide::Sell {
                let unfilled = order.nex_amount.saturating_sub(order.filled_amount);
                if !unfilled.is_zero() {
                    T::Currency::unreserve(&who, unfilled);
                }
            } else if order.side == OrderSide::Buy {
                // 退还剩余的预锁定保证金
                if !order.buyer_deposit.is_zero() {
                    T::Currency::unreserve(&who, order.buyer_deposit);
                }
            }

            order.status = OrderStatus::Cancelled;
            Orders::<T>::insert(order_id, &order);

            Self::remove_from_order_book(order_id, order.side);
            UserOrders::<T>::mutate(&who, |orders| {
                orders.retain(|&id| id != order_id);
            });
            Self::update_best_price_on_remove(order.usdt_price, order.side);

            Self::deposit_event(Event::OrderCancelled { order_id });

            Ok(())
        }

        /// 预锁定卖单（买家吃卖单）
        ///
        /// 买家看到卖单后调用：锁定保证金 → 创建 USDT 交易
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::reserve_sell_order())]
        pub fn reserve_sell_order(
            origin: OriginFor<T>,
            order_id: u64,
            amount: Option<BalanceOf<T>>,
            buyer_tron_address: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;
            Self::ensure_not_banned(&who)?;
            Self::ensure_not_suspended(&who)?;
            Self::ensure_trade_limit(&who)?;

            // H1修复: 验证买家 TRON 地址（Base58Check 完整校验，与其他 extrinsic 一致）
            ensure!(
                pallet_trading_common::is_valid_tron_address(&buyer_tron_address),
                Error::<T>::InvalidTronAddress
            );
            let buyer_tron: TronAddress = buyer_tron_address
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;

            let mut order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.side == OrderSide::Sell, Error::<T>::OrderSideMismatch);
            ensure!(
                order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
                Error::<T>::OrderClosed
            );
            ensure!(order.maker != who, Error::<T>::CannotTakeOwnOrder);

            // M1: 订单过期检查
            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now <= order.expires_at, Error::<T>::OrderExpired);

            // M4: 吃单时也检查价格偏离 + 熔断
            Self::check_price_deviation(order.usdt_price)?;

            let available = order
                .nex_amount
                .checked_sub(&order.filled_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let fill_amount = amount.unwrap_or(available).min(available);
            ensure!(!fill_amount.is_zero(), Error::<T>::AmountTooSmall);
            // M1修复: 吃单量也需满足最低限额（防微量交易浪费 OCW 资源）
            if available >= T::MinOrderNexAmount::get() {
                ensure!(
                    fill_amount >= T::MinOrderNexAmount::get(),
                    Error::<T>::OrderAmountBelowMinimum
                );
            }
            // S3: 卖家设置的最低吃单量
            if !order.min_fill_amount.is_zero() && available >= order.min_fill_amount {
                ensure!(
                    fill_amount >= order.min_fill_amount,
                    Error::<T>::BelowMinFillAmount
                );
            }
            // B3: 单笔吃单上限
            ensure!(
                fill_amount <= T::MaxOrderNexAmount::get(),
                Error::<T>::OrderAmountTooLarge
            );

            let nex_u128: u128 = fill_amount.saturated_into();
            let usdt_amount = u64::try_from(
                nex_u128
                    .checked_mul(order.usdt_price as u128)
                    .ok_or(Error::<T>::ArithmeticOverflow)?
                    .checked_div(1_000_000_000_000u128)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            )
            .map_err(|_| Error::<T>::ArithmeticOverflow)?;
            ensure!(usdt_amount > 0, Error::<T>::AmountTooSmall);

            let seller_tron_address = order
                .tron_address
                .clone()
                .ok_or(Error::<T>::InvalidTronAddress)?;

            // 保证金逻辑：免保证金卖单 vs 普通卖单
            let (buyer_deposit, is_waived_trade) = if order.deposit_waived {
                // L3: 已完成过交易的买家不再享受免保证金
                ensure!(
                    !CompletedBuyers::<T>::get(&who),
                    Error::<T>::BuyerAlreadyCompleted
                );
                // L2: 每账户最多 1 笔活跃免保证金交易
                ensure!(
                    ActiveWaivedTrades::<T>::get(&who) == 0,
                    Error::<T>::FirstOrderLimitReached
                );
                // L2: 单笔上限
                ensure!(
                    fill_amount <= T::MaxFirstOrderAmount::get(),
                    Error::<T>::FirstOrderAmountTooLarge
                );
                // M-5 审计修复: 免保证金需持有最低余额（MinOrderNexAmount），提高 Sybil 攻击成本
                ensure!(
                    T::Currency::free_balance(&who) >= T::MinOrderNexAmount::get(),
                    Error::<T>::InsufficientBalance
                );
                (Zero::zero(), true)
            } else {
                let base_deposit = Self::calculate_buyer_deposit(usdt_amount)?;
                let deposit = Self::apply_credit_discount(&who, base_deposit);
                if !deposit.is_zero() {
                    T::Currency::reserve(&who, deposit)
                        .map_err(|_| Error::<T>::InsufficientDepositBalance)?;
                }
                (deposit, false)
            };

            let trade_id = Self::do_create_usdt_trade_ex(
                order_id,
                order.maker.clone(),
                who.clone(),
                fill_amount,
                usdt_amount,
                seller_tron_address,
                Some(buyer_tron),
                buyer_deposit,
                is_waived_trade,
            )?;

            // L2: 记录活跃免保证金交易
            if is_waived_trade {
                ActiveWaivedTrades::<T>::mutate(&who, |count| *count = count.saturating_add(1));
            }

            // 信用系统：增加买家活跃交易计数
            ActiveBuyerTrades::<T>::mutate(&who, |c| *c = c.saturating_add(1));

            // 更新订单
            order.filled_amount = order
                .filled_amount
                .checked_add(&fill_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            if order.filled_amount >= order.nex_amount {
                order.status = OrderStatus::Filled;
                Self::remove_from_order_book(order_id, OrderSide::Sell);
                UserOrders::<T>::mutate(&order.maker, |orders| {
                    orders.retain(|&id| id != order_id);
                });
            } else {
                order.status = OrderStatus::PartiallyFilled;
            }
            Orders::<T>::insert(order_id, &order);

            Self::deposit_event(Event::UsdtTradeCreated {
                trade_id,
                order_id,
                seller: order.maker.clone(),
                buyer: who.clone(),
                nex_amount: fill_amount,
                usdt_amount,
            });

            if !buyer_deposit.is_zero() {
                Self::deposit_event(Event::BuyerDepositLocked {
                    trade_id,
                    buyer: who,
                    deposit: buyer_deposit,
                });
            }

            Ok(())
        }

        /// 接受买单（卖家接受买单）
        ///
        /// 卖家看到买单后调用：锁定 NEX + 锁定买家保证金 → 创建 USDT 交易
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::accept_buy_order())]
        pub fn accept_buy_order(
            origin: OriginFor<T>,
            order_id: u64,
            amount: Option<BalanceOf<T>>,
            tron_address: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;
            Self::ensure_not_banned(&who)?;

            let mut order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            let buyer = order.maker.clone();
            ensure!(order.side == OrderSide::Buy, Error::<T>::OrderSideMismatch);
            ensure!(
                order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
                Error::<T>::OrderClosed
            );
            ensure!(buyer != who, Error::<T>::CannotTakeOwnOrder);

            // 信用系统：检查买家（挂单方）暂停状态
            // 注意：不检查 trade_limit，因为买家已通过 place_buy_order 承诺交易
            // 部分成交不应被并发限制阻断
            Self::ensure_not_suspended(&buyer)?;

            // M1: 订单过期检查
            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now <= order.expires_at, Error::<T>::OrderExpired);

            // M4: 吃单时也检查价格偏离 + 熔断
            Self::check_price_deviation(order.usdt_price)?;

            // 验证 TRON 地址（Base58Check 完整校验）
            ensure!(
                pallet_trading_common::is_valid_tron_address(&tron_address),
                Error::<T>::InvalidTronAddress
            );
            let tron_addr: TronAddress = tron_address
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;

            let available = order
                .nex_amount
                .checked_sub(&order.filled_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let fill_amount = amount.unwrap_or(available).min(available);
            ensure!(!fill_amount.is_zero(), Error::<T>::AmountTooSmall);
            if available >= T::MinOrderNexAmount::get() {
                ensure!(
                    fill_amount >= T::MinOrderNexAmount::get(),
                    Error::<T>::OrderAmountBelowMinimum
                );
            }
            ensure!(
                fill_amount <= T::MaxOrderNexAmount::get(),
                Error::<T>::OrderAmountTooLarge
            );

            let nex_u128: u128 = fill_amount.saturated_into();
            // 🆕 M2修复: 安全转换防止 u64 截断
            let usdt_amount = u64::try_from(
                nex_u128
                    .checked_mul(order.usdt_price as u128)
                    .ok_or(Error::<T>::ArithmeticOverflow)?
                    .checked_div(1_000_000_000_000u128)
                    .ok_or(Error::<T>::ArithmeticOverflow)?,
            )
            .map_err(|_| Error::<T>::ArithmeticOverflow)?;
            ensure!(usdt_amount > 0, Error::<T>::AmountTooSmall);

            // 从预锁定保证金中按比例分配给本次交易（已在 place_buy_order 时 reserve）
            // P3 修复：最后一笔填满时直接取走全部剩余保证金，避免整除截断导致 dust 残留
            let trade_deposit = if !order.buyer_deposit.is_zero() && !available.is_zero() {
                if fill_amount == available {
                    order.buyer_deposit
                } else {
                    let deposit_u128: u128 = order.buyer_deposit.saturated_into();
                    let fill_u128: u128 = fill_amount.saturated_into();
                    let avail_u128: u128 = available.saturated_into();
                    let result = deposit_u128
                        .saturating_mul(fill_u128)
                        .saturating_div(avail_u128);
                    result.saturated_into()
                }
            } else {
                Zero::zero()
            };
            order.buyer_deposit = order.buyer_deposit.saturating_sub(trade_deposit);

            // 锁定卖家 NEX
            T::Currency::reserve(&who, fill_amount).map_err(|_| Error::<T>::InsufficientBalance)?;

            // 从买单中取出买家 TRON 地址
            let buyer_tron = order.tron_address.clone();

            let trade_id = Self::do_create_usdt_trade(
                order_id,
                who.clone(),
                buyer.clone(),
                fill_amount,
                usdt_amount,
                tron_addr,
                buyer_tron,
                trade_deposit,
            )?;

            // 信用系统：增加买家活跃交易计数
            ActiveBuyerTrades::<T>::mutate(&buyer, |c| *c = c.saturating_add(1));

            // 更新订单
            order.filled_amount = order
                .filled_amount
                .checked_add(&fill_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            if order.filled_amount >= order.nex_amount {
                order.status = OrderStatus::Filled;
                Self::remove_from_order_book(order_id, OrderSide::Buy);
                UserOrders::<T>::mutate(&buyer, |orders| {
                    orders.retain(|&id| id != order_id);
                });
            } else {
                order.status = OrderStatus::PartiallyFilled;
            }
            Orders::<T>::insert(order_id, &order);

            Self::deposit_event(Event::UsdtTradeCreated {
                trade_id,
                order_id,
                seller: who,
                buyer: buyer.clone(),
                nex_amount: fill_amount,
                usdt_amount,
            });

            if !trade_deposit.is_zero() {
                Self::deposit_event(Event::BuyerDepositLocked {
                    trade_id,
                    buyer,
                    deposit: trade_deposit,
                });
            }

            Ok(())
        }

        /// 买家确认 USDT 支付
        ///
        /// 买家声明已向卖家 TRON 地址转账，OCW 将按 (from, to, amount) 验证。
        /// buyer_tron_address 已在挂单/预锁定阶段提供，此处仅触发验证流程。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::confirm_payment())]
        #[transactional]
        pub fn confirm_payment(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;
            Self::ensure_not_banned(&who)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(trade.buyer == who, Error::<T>::NotTradeParticipant);
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingPayment,
                Error::<T>::InvalidTradeStatus
            );

            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now <= trade.timeout_at, Error::<T>::TradeTimeout);

            // buyer_tron_address 必须已在上游设置（reserve_sell_order 或 place_buy_order）
            ensure!(
                trade.buyer_tron_address.is_some(),
                Error::<T>::InvalidTronAddress
            );

            trade.status = UsdtTradeStatus::AwaitingVerification;
            trade.payment_confirmed = true; // W6
            UsdtTrades::<T>::insert(trade_id, &trade);

            AwaitingPaymentTrades::<T>::mutate(|list| {
                list.retain(|&id| id != trade_id);
            });
            PendingUsdtTrades::<T>::try_mutate(|pending| {
                pending
                    .try_push(trade_id)
                    .map_err(|_| Error::<T>::PendingQueueFull)
            })?;

            // #10: 队列满自动暂停保护
            Self::check_queue_overflow_and_pause();

            Self::deposit_event(Event::UsdtPaymentSubmitted { trade_id });

            Ok(())
        }

        /// 处理超时 USDT 交易
        ///
        /// ## 分阶段超时策略
        /// - **AwaitingPayment**: 买家未付款 → 直接超时，没收保证金
        /// - **AwaitingVerification**: 买家已声明付款 → 需等宽限期结束
        ///   - 若 OCW 已提交验证结果 → 按正常流程结算（不走超时）
        ///   - 若宽限期后仍无结果 → 退款 + 没收保证金 + 发出事件供链下仲裁
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::process_timeout())]
        pub fn process_timeout(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            // C4: 先尝试 Admin origin，再尝试 signed origin（参与方）
            let is_admin = T::MarketAdminOrigin::ensure_origin(origin.clone()).is_ok();
            if !is_admin {
                let who = ensure_signed(origin)?;
                let trade_ref =
                    UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
                ensure!(
                    trade_ref.buyer == who || trade_ref.seller == who,
                    Error::<T>::NotParticipantOrAdmin
                );
            }

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingPayment
                    || trade.status == UsdtTradeStatus::AwaitingVerification
                    || trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::InvalidTradeStatus
            );

            let now = <frame_system::Pallet<T>>::block_number();

            // ── UnderpaidPending: 补付窗口到期后按最终金额终裁 ──
            if trade.status == UsdtTradeStatus::UnderpaidPending {
                let deadline = trade
                    .underpaid_deadline
                    .ok_or(Error::<T>::InvalidTradeStatus)?;
                ensure!(now > deadline, Error::<T>::UnderpaidGraceNotExpired);

                if let Some((_, final_amount)) = OcwVerificationResults::<T>::get(trade_id) {
                    let final_result = Self::calculate_payment_verification_result(
                        trade.usdt_amount,
                        final_amount,
                    );
                    match final_result {
                        PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                            Self::process_full_payment(&mut trade, trade_id)?;
                        }
                        _ => {
                            Self::process_underpaid(&mut trade, trade_id, final_amount)?;
                        }
                    }
                    let payment_ratio = pallet_trading_common::compute_payment_ratio_bps(
                        trade.usdt_amount,
                        final_amount,
                    );
                    let deposit_forfeit_rate = Self::calculate_deposit_forfeit_rate(payment_ratio);

                    PendingUnderpaidTrades::<T>::mutate(|p| {
                        p.retain(|&id| id != trade_id);
                    });
                    OcwVerificationResults::<T>::remove(trade_id);

                    Self::deposit_event(Event::UnderpaidFinalized {
                        trade_id,
                        final_amount,
                        payment_ratio,
                        deposit_forfeit_rate,
                    });
                    return Ok(());
                }
                // 无 OCW 结果 → 走通用超时退款
            }

            ensure!(now > trade.timeout_at, Error::<T>::InvalidTradeStatus);

            // ── AwaitingVerification: 宽限期 + 已有结果检查 ──
            if trade.status == UsdtTradeStatus::AwaitingVerification {
                let grace: BlockNumberFor<T> = T::VerificationGracePeriod::get().into();
                let deadline_with_grace = trade.timeout_at.saturating_add(grace);

                // 宽限期未结束 → 拒绝超时
                ensure!(now > deadline_with_grace, Error::<T>::StillInGracePeriod);

                // 宽限期已过，但 OCW 已提交了验证结果 → 按正常流程结算
                if let Some((verification_result, actual_amount)) =
                    OcwVerificationResults::<T>::get(trade_id)
                {
                    match verification_result {
                        PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                            Self::process_full_payment(&mut trade, trade_id)?;
                        }
                        PaymentVerificationResult::Underpaid
                        | PaymentVerificationResult::SeverelyUnderpaid => {
                            Self::process_underpaid(&mut trade, trade_id, actual_amount)?;
                        }
                        PaymentVerificationResult::Invalid => {
                            Self::process_underpaid(&mut trade, trade_id, 0)?;
                        }
                    }
                    PendingUsdtTrades::<T>::mutate(|pending| {
                        pending.retain(|&id| id != trade_id);
                    });
                    OcwVerificationResults::<T>::remove(trade_id);
                    return Ok(());
                }
            }

            // ── 执行超时退款（AwaitingPayment 或 AwaitingVerification 无结果）──

            // 退还锁定的 NEX 给卖家
            T::Currency::unreserve(&trade.seller, trade.nex_amount);

            // 回滚父订单
            Self::rollback_order_filled_amount(trade.order_id, trade.nex_amount);

            // 没收买家保证金
            Self::forfeit_buyer_deposit(&mut trade, trade_id);

            let is_verification_timeout = trade.status == UsdtTradeStatus::AwaitingVerification;
            let buyer_clone = trade.buyer.clone();
            let seller_clone = trade.seller.clone();
            let usdt_amount = trade.usdt_amount;

            trade.status = UsdtTradeStatus::Refunded;
            trade.completed_at = Some(now); // W5
            UsdtTrades::<T>::insert(trade_id, &trade);

            Self::cleanup_waived_trade_counter(&trade.buyer, trade.order_id);

            // 信用系统：超时违约扣分 + 减少活跃计数
            Self::credit_on_timeout_violation(&trade.buyer);
            ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| *c = c.saturating_sub(1));

            // 清理所有队列
            AwaitingPaymentTrades::<T>::mutate(|list| {
                list.retain(|&id| id != trade_id);
            });
            PendingUsdtTrades::<T>::mutate(|pending| {
                pending.retain(|&id| id != trade_id);
            });
            PendingUnderpaidTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);
            // P9: 清理 Indexer hint 存储（释放 pending_hint_count）
            Self::cleanup_trade_hint(trade_id);

            if is_verification_timeout {
                // AwaitingVerification 超时 — 发出专用事件，便于链下仲裁
                Self::deposit_event(Event::VerificationTimeoutRefunded {
                    trade_id,
                    buyer: buyer_clone,
                    seller: seller_clone,
                    usdt_amount,
                });
            } else {
                Self::deposit_event(Event::UsdtTradeRefunded { trade_id });
            }

            Ok(())
        }

        /// OCW 提交验证结果
        ///
        /// - Exact/Overpaid: 存储结果，等待 claim_verification_reward
        /// - Underpaid (50%-99.5%): 进入 UnderpaidPending 补付窗口
        /// - SeverelyUnderpaid (<50%) / Invalid: 直接存储结果，终裁
        ///
        /// 🆕 C3: tx_hashes 用于防重放。同一 TRON 交易不能验证多笔订单。
        /// B1: 支持多 tx_hash（一笔订单可由多笔链上转账支付）
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::submit_ocw_result())]
        pub fn submit_ocw_result(
            origin: OriginFor<T>,
            trade_id: u64,
            actual_amount: u64,
            tx_hashes: TxHashVec,
        ) -> DispatchResult {
            ensure_none(origin)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingVerification,
                Error::<T>::InvalidTradeStatus
            );

            // B1: tx_hashes 不能为空，且每个 hash 都不能已被使用
            ensure!(!tx_hashes.is_empty(), Error::<T>::TxHashRequired);
            for hash in tx_hashes.iter() {
                ensure!(
                    !UsedTxHashes::<T>::contains_key(hash),
                    Error::<T>::TxHashAlreadyUsed
                );
            }

            let verification_result =
                Self::calculate_payment_verification_result(trade.usdt_amount, actual_amount);

            // W8: 先持久化 OCW 结果，结算失败时可通过 claim_verification_reward 手动恢复
            OcwVerificationResults::<T>::insert(trade_id, (verification_result, actual_amount));

            match verification_result {
                PaymentVerificationResult::Underpaid => {
                    let now = <frame_system::Pallet<T>>::block_number();
                    let grace: BlockNumberFor<T> = T::UnderpaidGracePeriod::get().into();
                    let deadline = now.saturating_add(grace);

                    trade.status = UsdtTradeStatus::UnderpaidPending;
                    trade.first_verified_at = Some(now);
                    trade.first_actual_amount = Some(actual_amount);
                    trade.underpaid_deadline = Some(deadline);
                    UsdtTrades::<T>::insert(trade_id, &trade);

                    PendingUsdtTrades::<T>::mutate(|p| {
                        p.retain(|&id| id != trade_id);
                    });
                    PendingUnderpaidTrades::<T>::try_mutate(|p| {
                        p.try_push(trade_id)
                            .map_err(|_| Error::<T>::UnderpaidQueueFull)
                    })?;

                    let payment_ratio = pallet_trading_common::compute_payment_ratio_bps(
                        trade.usdt_amount,
                        actual_amount,
                    );
                    Self::deposit_event(Event::UnderpaidDetected {
                        trade_id,
                        expected_amount: trade.usdt_amount,
                        actual_amount,
                        payment_ratio,
                        deadline,
                    });
                }
                PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                    Self::process_full_payment(&mut trade, trade_id)?;
                    PendingUsdtTrades::<T>::mutate(|p| {
                        p.retain(|&id| id != trade_id);
                    });
                    OcwVerificationResults::<T>::remove(trade_id); // W8: 结算成功才清理

                    Self::deposit_event(Event::OcwResultSubmitted {
                        trade_id,
                        verification_result,
                        actual_amount,
                    });
                }
                PaymentVerificationResult::SeverelyUnderpaid
                | PaymentVerificationResult::Invalid => {
                    let amt = if verification_result == PaymentVerificationResult::Invalid {
                        0
                    } else {
                        actual_amount
                    };
                    Self::process_underpaid(&mut trade, trade_id, amt)?;
                    PendingUsdtTrades::<T>::mutate(|p| {
                        p.retain(|&id| id != trade_id);
                    });
                    OcwVerificationResults::<T>::remove(trade_id); // W8: 结算成功才清理

                    Self::deposit_event(Event::OcwResultSubmitted {
                        trade_id,
                        verification_result,
                        actual_amount,
                    });
                }
            }

            // B1: 记录所有已使用的 tx_hash（防重放）
            {
                let now = <frame_system::Pallet<T>>::block_number();
                for hash in tx_hashes.iter() {
                    UsedTxHashes::<T>::insert(hash, (trade_id, now));
                }
            }

            Ok(())
        }

        /// 领取验证奖励（任何人）
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::claim_reward())]
        pub fn claim_verification_reward(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            Self::do_claim_verification_reward(&caller, trade_id)
        }

        /// 配置价格保护（Root）
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::configure_price_protection())]
        pub fn configure_price_protection(
            origin: OriginFor<T>,
            enabled: bool,
            max_price_deviation: u16,
            circuit_breaker_threshold: u16,
            min_trades_for_twap: u64,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            ensure!(max_price_deviation <= 10000, Error::<T>::InvalidBasisPoints);
            ensure!(
                circuit_breaker_threshold <= 10000,
                Error::<T>::InvalidBasisPoints
            );

            let mut config = PriceProtectionStore::<T>::get().unwrap_or_default();
            config.enabled = enabled;
            config.max_price_deviation = max_price_deviation;
            config.circuit_breaker_threshold = circuit_breaker_threshold;
            config.min_trades_for_twap = min_trades_for_twap;
            PriceProtectionStore::<T>::put(config);

            Self::deposit_event(Event::PriceProtectionConfigured {
                enabled,
                max_deviation: max_price_deviation,
            });

            Ok(())
        }

        /// 设置初始价格（Root, TWAP 冷启动）
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::set_initial_price())]
        pub fn set_initial_price(origin: OriginFor<T>, initial_price: u64) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            ensure!(initial_price > 0, Error::<T>::ZeroPrice);

            PriceProtectionStore::<T>::mutate(|maybe| {
                let config = maybe.get_or_insert_with(Default::default);
                config.initial_price = Some(initial_price);
            });

            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
            // on_idle 需要 trade_count > 0 才刷新 current_block
            TwapAccumulatorStore::<T>::put(TwapAccumulator::new_at(
                current_block,
                initial_price,
                1,
            ));

            LastTradePrice::<T>::put(initial_price);

            Self::deposit_event(Event::InitialPriceSet { initial_price });

            Ok(())
        }

        /// 解除熔断（Root）
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::lift_circuit_breaker())]
        pub fn lift_circuit_breaker(origin: OriginFor<T>) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
            let config = PriceProtectionStore::<T>::get().unwrap_or_default();
            ensure!(
                config.circuit_breaker_active,
                Error::<T>::CircuitBreakerNotActive
            );
            // 🆕 L2修复: 使用语义正确的错误名（原 InvalidTradeStatus 不匹配）
            ensure!(
                current_block >= config.circuit_breaker_until,
                Error::<T>::CircuitBreakerNotExpired
            );

            PriceProtectionStore::<T>::mutate(|maybe| {
                if let Some(c) = maybe {
                    c.circuit_breaker_active = false;
                    c.circuit_breaker_until = 0;
                }
            });

            Self::deposit_event(Event::CircuitBreakerLifted);

            Ok(())
        }

        /// 委员会批准：国库 → 种子账户注资（委员会审批）
        ///
        /// 用于补充 seed_liquidity 所需的 NEX 资金。
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::fund_seed_account())]
        pub fn fund_seed_account(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            ensure!(!amount.is_zero(), Error::<T>::AmountTooSmall);

            let treasury = T::TreasuryAccount::get();
            let seed = T::SeedLiquidityAccount::get();

            T::Currency::transfer(&treasury, &seed, amount, ExistenceRequirement::KeepAlive)?;

            Self::deposit_event(Event::SeedAccountFunded {
                amount,
                treasury,
                seed_account: seed,
            });

            Ok(())
        }

        /// 种子流动性挂单（委员会/Root）
        ///
        /// 批量挂免保证金卖单，为新用户提供 NEX 购买入口。
        /// 使用 Config 中固定的 SeedTronAddress 作为收款地址。
        /// 每笔订单 USDT 金额默认 SeedOrderUsdtAmount，可通过 usdt_override 临时覆盖。
        /// NEX 数量和价格自动计算：
        ///   seed_price = ref_price × (1 + premium)
        ///   nex_amount = usdt_amount × 10^12 / seed_price
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::seed_liquidity())]
        pub fn seed_liquidity(
            origin: OriginFor<T>,
            order_count: u32,
            usdt_override: Option<u64>,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            ensure!(order_count > 0, Error::<T>::AmountTooSmall);
            ensure!(
                order_count <= T::MaxWaivedSeedOrders::get(),
                Error::<T>::TooManySeedOrders
            );

            let seed_account = T::SeedLiquidityAccount::get();
            let tron_addr: TronAddress = SeedTronAddressStore::<T>::get().unwrap_or_else(|| {
                T::SeedTronAddress::get()
                    .to_vec()
                    .try_into()
                    .expect("Config SeedTronAddress must be valid")
            });

            // 瀑布式基准价 + 溢价
            let ref_price = Self::get_seed_reference_price().ok_or(Error::<T>::NoPriceReference)?;
            let premium = T::SeedPricePremiumBps::get() as u64;
            let seed_price = ref_price.saturating_mul(10000u64.saturating_add(premium)) / 10000u64;
            ensure!(seed_price > 0, Error::<T>::ZeroPrice);

            // 计算每笔 NEX 数量：usdt_amount × 10^12 / seed_price
            let usdt_amount = usdt_override.unwrap_or_else(|| T::SeedOrderUsdtAmount::get());
            ensure!(usdt_amount > 0, Error::<T>::AmountTooSmall);
            let nex_per_order: BalanceOf<T> = ((usdt_amount as u128)
                .saturating_mul(1_000_000_000_000u128) // NEX 精度 10^12
                / (seed_price as u128))
                .try_into()
                .map_err(|_| Error::<T>::AmountTooSmall)?;
            ensure!(!nex_per_order.is_zero(), Error::<T>::AmountTooSmall);

            let mut total_nex: BalanceOf<T> = Zero::zero();

            for _ in 0..order_count {
                T::Currency::reserve(&seed_account, nex_per_order)
                    .map_err(|_| Error::<T>::InsufficientBalance)?;

                let order_id = Self::do_create_order(
                    seed_account.clone(),
                    OrderSide::Sell,
                    nex_per_order,
                    seed_price,
                    Some(tron_addr.clone()),
                    Zero::zero(),
                    true,
                    Zero::zero(),
                )?;

                total_nex = total_nex.saturating_add(nex_per_order);

                Self::deposit_event(Event::OrderCreated {
                    order_id,
                    maker: seed_account.clone(),
                    side: OrderSide::Sell,
                    nex_amount: nex_per_order,
                    usdt_price: seed_price,
                });
            }

            Self::refresh_best_prices();

            Self::deposit_event(Event::LiquiditySeeded {
                order_count,
                total_nex,
                source: seed_account,
            });

            Ok(())
        }

        /// OCW 自动确认付款 + 提交验证结果（unsigned）
        ///
        /// 当 OCW 预检扫描发现 AwaitingPayment 的交易已有 USDT 到账，
        /// 但买家忘记调用 confirm_payment 时，sidecar 可调用此函数
        /// 一步完成：确认付款 + 存储验证结果。
        ///
        /// 🆕 C3: tx_hashes 用于防重放。
        /// B1: 支持多 tx_hash。
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::auto_confirm_payment())]
        pub fn auto_confirm_payment(
            origin: OriginFor<T>,
            trade_id: u64,
            actual_amount: u64,
            tx_hashes: TxHashVec,
        ) -> DispatchResult {
            ensure_none(origin)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingPayment,
                Error::<T>::InvalidTradeStatus
            );
            ensure!(
                trade.buyer_tron_address.is_some(),
                Error::<T>::InvalidTronAddress
            );

            // B1: tx_hashes 不能为空，且每个 hash 都不能已被使用
            ensure!(!tx_hashes.is_empty(), Error::<T>::TxHashRequired);
            for hash in tx_hashes.iter() {
                ensure!(
                    !UsedTxHashes::<T>::contains_key(hash),
                    Error::<T>::TxHashAlreadyUsed
                );
            }

            let verification_result =
                Self::calculate_payment_verification_result(trade.usdt_amount, actual_amount);

            trade.payment_confirmed = true; // W6: OCW 检测到付款
            Self::deposit_event(Event::AutoPaymentDetected {
                trade_id,
                actual_amount,
            });

            AwaitingPaymentTrades::<T>::mutate(|list| {
                list.retain(|&id| id != trade_id);
            });

            match verification_result {
                PaymentVerificationResult::Underpaid => {
                    // 少付 → 进入 UnderpaidPending 补付窗口（与 submit_ocw_result 一致）
                    let now = <frame_system::Pallet<T>>::block_number();
                    let grace: BlockNumberFor<T> = T::UnderpaidGracePeriod::get().into();
                    let deadline = now.saturating_add(grace);

                    trade.status = UsdtTradeStatus::UnderpaidPending;
                    trade.first_verified_at = Some(now);
                    trade.first_actual_amount = Some(actual_amount);
                    trade.underpaid_deadline = Some(deadline);
                    UsdtTrades::<T>::insert(trade_id, &trade);

                    PendingUnderpaidTrades::<T>::try_mutate(|p| {
                        p.try_push(trade_id)
                            .map_err(|_| Error::<T>::UnderpaidQueueFull)
                    })?;
                    OcwVerificationResults::<T>::insert(
                        trade_id,
                        (verification_result, actual_amount),
                    );

                    let payment_ratio = pallet_trading_common::compute_payment_ratio_bps(
                        trade.usdt_amount,
                        actual_amount,
                    );
                    Self::deposit_event(Event::UnderpaidDetected {
                        trade_id,
                        expected_amount: trade.usdt_amount,
                        actual_amount,
                        payment_ratio,
                        deadline,
                    });
                }
                PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                    // R1: 直接结算
                    Self::process_full_payment(&mut trade, trade_id)?;

                    Self::deposit_event(Event::OcwResultSubmitted {
                        trade_id,
                        verification_result,
                        actual_amount,
                    });
                }
                _ => {
                    // SeverelyUnderpaid/Invalid → 直接处理
                    let amt = if verification_result == PaymentVerificationResult::Invalid {
                        0
                    } else {
                        actual_amount
                    };
                    Self::process_underpaid(&mut trade, trade_id, amt)?;

                    Self::deposit_event(Event::OcwResultSubmitted {
                        trade_id,
                        verification_result,
                        actual_amount,
                    });
                }
            }

            // B1: 记录所有已使用的 tx_hash（防重放）
            // M7: 同时记录插入区块号，用于 TTL 清理
            {
                let now = <frame_system::Pallet<T>>::block_number();
                for hash in tx_hashes.iter() {
                    UsedTxHashes::<T>::insert(hash, (trade_id, now));
                }
            }

            Ok(())
        }

        /// OCW 更新少付交易的累计金额（unsigned）
        ///
        /// 补付窗口内，OCW 持续扫描 TronGrid，发现新转账则更新金额。
        /// 若累计金额达到 99.5%，直接升级为 Exact 结算。
        /// B2: 增加 new_tx_hashes 参数（允许为空 — 仅金额增长、无新 hash 的情况）
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::submit_underpaid_update())]
        pub fn submit_underpaid_update(
            origin: OriginFor<T>,
            trade_id: u64,
            new_actual_amount: u64,
            new_tx_hashes: TxHashVec,
        ) -> DispatchResult {
            ensure_none(origin)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::NotUnderpaidPending
            );

            // P1-8: 补付更新必须携带至少一个新 tx_hash 作为证据
            ensure!(!new_tx_hashes.is_empty(), Error::<T>::TxHashRequired);

            // B2: 验证所有新 tx_hash 未被使用
            for hash in new_tx_hashes.iter() {
                ensure!(
                    !UsedTxHashes::<T>::contains_key(hash),
                    Error::<T>::TxHashAlreadyUsed
                );
            }

            let previous_amount = OcwVerificationResults::<T>::get(trade_id)
                .map(|(_, amt)| amt)
                .unwrap_or(0);

            // 只接受递增的金额（防止恶意回退）
            if new_actual_amount <= previous_amount {
                return Ok(());
            }

            let new_result =
                Self::calculate_payment_verification_result(trade.usdt_amount, new_actual_amount);

            // 更新存储的验证结果
            OcwVerificationResults::<T>::insert(trade_id, (new_result, new_actual_amount));

            Self::deposit_event(Event::UnderpaidAmountUpdated {
                trade_id,
                previous_amount,
                new_amount: new_actual_amount,
            });

            // B2: 记录新 tx_hash
            {
                let now = <frame_system::Pallet<T>>::block_number();
                for hash in new_tx_hashes.iter() {
                    UsedTxHashes::<T>::insert(hash, (trade_id, now));
                }
            }

            // R1: 补齐后直接结算（无需二次触发）
            if matches!(
                new_result,
                PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid
            ) {
                Self::process_full_payment(&mut trade, trade_id)?;
                PendingUnderpaidTrades::<T>::mutate(|p| {
                    p.retain(|&id| id != trade_id);
                });
                OcwVerificationResults::<T>::remove(trade_id);
            }

            Ok(())
        }

        /// 少付终裁（任何人，补付窗口到期后）
        ///
        /// 补付窗口到期后，按最终累计金额做终裁：
        /// - 按比例释放 NEX
        /// - 保证金按梯度没收
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::finalize_underpaid())]
        pub fn finalize_underpaid(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            let _who = ensure_signed(origin)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::NotUnderpaidPending
            );

            let deadline = trade
                .underpaid_deadline
                .ok_or(Error::<T>::InvalidTradeStatus)?;
            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now > deadline, Error::<T>::UnderpaidGraceNotExpired);

            // 读取最新的验证结果（可能被 submit_underpaid_update 更新过）
            let (_, final_amount) =
                OcwVerificationResults::<T>::get(trade_id).ok_or(Error::<T>::OcwResultNotFound)?;

            let final_result =
                Self::calculate_payment_verification_result(trade.usdt_amount, final_amount);

            match final_result {
                PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                    // 补付窗口内补齐了
                    Self::process_full_payment(&mut trade, trade_id)?;
                }
                _ => {
                    // 仍然少付 → 按梯度终裁
                    Self::process_underpaid(&mut trade, trade_id, final_amount)?;
                }
            }

            let payment_ratio =
                pallet_trading_common::compute_payment_ratio_bps(trade.usdt_amount, final_amount);
            let deposit_forfeit_rate = Self::calculate_deposit_forfeit_rate(payment_ratio);

            // 清理队列
            PendingUnderpaidTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);

            Self::deposit_event(Event::UnderpaidFinalized {
                trade_id,
                final_amount,
                payment_ratio,
                deposit_forfeit_rate,
            });

            Ok(())
        }

        // ==================== 新增 Extrinsics ====================

        /// 紧急暂停市场（MarketAdmin）
        ///
        /// 暂停所有挂单、吃单、confirm_payment 操作。
        /// 已有的 process_timeout / claim_verification_reward 不受影响。
        #[pallet::call_index(18)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn force_pause_market(origin: OriginFor<T>) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            MarketPausedStore::<T>::put(true);
            Self::deposit_event(Event::MarketPaused);
            Ok(())
        }

        /// 恢复市场交易（MarketAdmin）
        #[pallet::call_index(19)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn force_resume_market(origin: OriginFor<T>) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            MarketPausedStore::<T>::put(false);
            Self::deposit_event(Event::MarketResumed);
            Ok(())
        }

        /// 管理员强制结算交易（MarketAdmin）
        ///
        /// 当 OCW/Sidecar 长期宕机，管理员根据链下证据手动结算。
        /// 仅处理 AwaitingVerification / UnderpaidPending 状态的交易。
        #[pallet::call_index(20)]
        #[pallet::weight(Weight::from_parts(120_000_000, 10_000))]
        pub fn force_settle_trade(
            origin: OriginFor<T>,
            trade_id: u64,
            actual_amount: u64,
            resolution: DisputeResolution,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            Self::do_force_settle(trade_id, actual_amount, resolution)
        }

        /// 管理员强制取消交易（MarketAdmin）
        ///
        /// 退还 NEX 给卖家 + 退还保证金给买家（不没收）。
        /// 用于安全事件或系统故障时保护双方。
        #[pallet::call_index(21)]
        #[pallet::weight(Weight::from_parts(100_000_000, 8_000))]
        pub fn force_cancel_trade(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            Self::do_force_cancel(trade_id)
        }

        /// 发起交易争议（买家或卖家）
        ///
        /// 仅允许对 Refunded（VerificationTimeout）状态的交易发起争议。
        /// 买家声称已付 USDT 但验证超时导致退款，可提交链下证据。
        #[pallet::call_index(22)]
        #[pallet::weight(Weight::from_parts(50_000_000, 5_000))]
        pub fn dispute_trade(
            origin: OriginFor<T>,
            trade_id: u64,
            evidence_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.buyer == who || trade.seller == who,
                Error::<T>::NotTradeParticipant
            );
            ensure!(
                trade.status == UsdtTradeStatus::Refunded
                    || trade.status == UsdtTradeStatus::Completed,
                Error::<T>::TradeNotDisputable
            );
            // W6: Refunded 交易必须有付款确认记录，防止买家从未付款却争议获利
            if trade.status == UsdtTradeStatus::Refunded {
                ensure!(trade.payment_confirmed, Error::<T>::PaymentNotConfirmed);
            }
            ensure!(
                !TradeDisputeStore::<T>::contains_key(trade_id),
                Error::<T>::TradeAlreadyDisputed
            );

            // W5: 争议窗口锚定 completed_at（精确终态时间）
            let now = <frame_system::Pallet<T>>::block_number();
            let window: BlockNumberFor<T> = T::DisputeWindowBlocks::get().into();
            let anchor = trade.completed_at.unwrap_or(trade.timeout_at);
            ensure!(
                now <= anchor.saturating_add(window),
                Error::<T>::DisputeWindowExpired
            );

            let cid: BoundedVec<u8, ConstU32<128>> = evidence_cid
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::ArithmeticOverflow)?;

            let dispute = TradeDispute {
                trade_id,
                initiator: who.clone(),
                status: DisputeStatus::Open,
                created_at: now,
                evidence_cid: cid,
                counter_evidence_cid: None,
                counter_party: None,
            };
            TradeDisputeStore::<T>::insert(trade_id, dispute);

            Self::deposit_event(Event::TradeDisputed {
                trade_id,
                initiator: who,
                evidence_cid,
            });

            Ok(())
        }

        /// 解决争议（MarketAdmin）
        ///
        /// 管理员根据链下证据裁决：
        /// - ReleaseToBuyer: 从国库补偿 NEX 给买家（因原始 NEX 已退还卖家）
        /// - RefundToSeller: 维持原判（无需操作，仅关闭争议）
        #[pallet::call_index(23)]
        #[pallet::weight(Weight::from_parts(80_000_000, 6_000))]
        pub fn resolve_dispute(
            origin: OriginFor<T>,
            trade_id: u64,
            resolution: DisputeResolution,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            let mut dispute =
                TradeDisputeStore::<T>::get(trade_id).ok_or(Error::<T>::DisputeNotFound)?;
            ensure!(
                dispute.status == DisputeStatus::Open,
                Error::<T>::DisputeAlreadyClosed
            );

            let trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;

            match resolution {
                DisputeResolution::ReleaseToBuyer => {
                    // C3: Completed 交易买家已拿到 NEX，仅对 Refunded 交易补偿
                    if trade.status == UsdtTradeStatus::Refunded {
                        let treasury = T::TreasuryAccount::get();
                        let treasury_balance = T::Currency::free_balance(&treasury);
                        let min_balance = T::Currency::minimum_balance();
                        let available = treasury_balance.saturating_sub(min_balance);
                        let compensate_amount = trade.nex_amount.min(available);
                        if !compensate_amount.is_zero() {
                            T::Currency::transfer(
                                &treasury,
                                &trade.buyer,
                                compensate_amount,
                                ExistenceRequirement::KeepAlive,
                            )
                            .map_err(|_| Error::<T>::InsufficientBalance)?;
                        }
                    }
                    // Completed 交易：买家已拿到 NEX，无需额外补偿，仅记录裁决结果
                    dispute.status = DisputeStatus::ResolvedForBuyer;
                }
                DisputeResolution::RefundToSeller => {
                    // 维持原判，无需额外操作
                    dispute.status = DisputeStatus::ResolvedForSeller;
                }
            }

            TradeDisputeStore::<T>::insert(trade_id, dispute);

            Self::deposit_event(Event::DisputeResolved {
                trade_id,
                resolution,
            });

            Ok(())
        }

        /// 设置交易手续费率（MarketAdmin）
        ///
        /// 手续费从结算时的 NEX 释放中扣除，转入国库。
        /// 最大 1000 bps = 10%
        #[pallet::call_index(24)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn set_trading_fee(origin: OriginFor<T>, fee_bps: u16) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            ensure!(fee_bps <= 1000, Error::<T>::FeeTooHigh);

            let old_fee = TradingFeeBps::<T>::get();
            TradingFeeBps::<T>::put(fee_bps);

            Self::deposit_event(Event::TradingFeeUpdated {
                old_fee_bps: old_fee,
                new_fee_bps: fee_bps,
            });

            Ok(())
        }

        /// 修改订单价格（订单所有者）
        ///
        /// 原子修改价格，无需 cancel + re-place。
        /// 仅允许无活跃交易的订单修改。
        #[pallet::call_index(25)]
        #[pallet::weight(Weight::from_parts(60_000_000, 5_000))]
        pub fn update_order_price(
            origin: OriginFor<T>,
            order_id: u64,
            new_price: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;

            ensure!(new_price > 0, Error::<T>::ZeroPrice);
            Self::check_price_deviation(new_price)?;

            let mut order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.maker == who, Error::<T>::NotOrderOwner);
            ensure!(
                order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
                Error::<T>::OrderClosed
            );

            // 检查是否有活跃交易
            let trades = OrderTrades::<T>::get(order_id);
            let has_active = trades.iter().any(|&tid| {
                UsdtTrades::<T>::get(tid).map_or(false, |t| {
                    t.status == UsdtTradeStatus::AwaitingPayment
                        || t.status == UsdtTradeStatus::AwaitingVerification
                        || t.status == UsdtTradeStatus::UnderpaidPending
                })
            });
            ensure!(!has_active, Error::<T>::OrderHasActiveTrades);

            let old_price = order.usdt_price;

            // 🆕 H1修复: 买单价格变更时重新计算保证金
            if order.side == OrderSide::Buy && !order.buyer_deposit.is_zero() {
                let remaining = order.nex_amount.saturating_sub(order.filled_amount);
                if !remaining.is_zero() {
                    let rem_u128: u128 = remaining.saturated_into();
                    let new_usdt = rem_u128
                        .checked_mul(new_price as u128)
                        .ok_or(Error::<T>::ArithmeticOverflow)?
                        .checked_div(1_000_000_000_000u128)
                        .ok_or(Error::<T>::ArithmeticOverflow)?;
                    let new_usdt_u64 =
                        u64::try_from(new_usdt).map_err(|_| Error::<T>::ArithmeticOverflow)?;
                    let base_deposit = Self::calculate_buyer_deposit(new_usdt_u64)?;
                    let new_deposit = Self::apply_credit_discount(&who, base_deposit);
                    let old_deposit = order.buyer_deposit;

                    if new_deposit > old_deposit {
                        let diff = new_deposit.saturating_sub(old_deposit);
                        T::Currency::reserve(&who, diff)
                            .map_err(|_| Error::<T>::InsufficientDepositBalance)?;
                    } else if old_deposit > new_deposit {
                        let diff = old_deposit.saturating_sub(new_deposit);
                        T::Currency::unreserve(&who, diff);
                    }
                    order.buyer_deposit = new_deposit;
                }
            }

            order.usdt_price = new_price;
            Orders::<T>::insert(order_id, &order);

            // 更新最优价格
            Self::update_best_price_on_remove(old_price, order.side);
            Self::update_best_price_on_new_order(new_price, order.side);

            Self::deposit_event(Event::OrderPriceUpdated {
                order_id,
                old_price,
                new_price,
            });

            Ok(())
        }

        /// [deprecated] 保证金汇率已改为实时市场价格，此 extrinsic 不再使用
        #[pallet::call_index(26)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn update_deposit_exchange_rate(
            origin: OriginFor<T>,
            _new_rate: u64,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            Err(DispatchError::Other("deprecated"))
        }

        // ==================== 审计新增 Extrinsics ====================

        /// 卖家手动确认收款（B1/S1: OCW 故障时的备用结算路径）
        ///
        /// 卖家确认已收到 USDT → 直接完成交易释放 NEX。
        /// 卖家自愿承担风险，绕过 OCW 验证。
        #[pallet::call_index(27)]
        #[pallet::weight(Weight::from_parts(100_000_000, 8_000))]
        pub fn seller_confirm_received(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(trade.seller == who, Error::<T>::NotTradeParticipant);
            // W1: 仅允许买家已确认付款后的状态（防社工攻击）
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingVerification
                    || trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::InvalidTradeStatus
            );

            let buyer_clone = trade.buyer.clone();
            Self::process_full_payment(&mut trade, trade_id)?;

            AwaitingPaymentTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            PendingUsdtTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            PendingUnderpaidTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);

            Self::deposit_event(Event::SellerConfirmedReceived {
                trade_id,
                seller: who,
                buyer: buyer_clone,
            });

            Ok(())
        }

        /// 封禁用户（MarketAdmin）
        #[pallet::call_index(28)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn ban_user(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            BannedAccounts::<T>::insert(&account, true);

            // W2: 取消该用户所有无活跃交易的挂单
            let order_ids = UserOrders::<T>::get(&account);
            for &order_id in order_ids.iter() {
                if let Some(mut order) = Orders::<T>::get(order_id) {
                    if order.status != OrderStatus::Open
                        && order.status != OrderStatus::PartiallyFilled
                    {
                        continue;
                    }
                    let trades = OrderTrades::<T>::get(order_id);
                    let has_active = trades.iter().any(|&tid| {
                        UsdtTrades::<T>::get(tid).map_or(false, |t| {
                            t.status == UsdtTradeStatus::AwaitingPayment
                                || t.status == UsdtTradeStatus::AwaitingVerification
                                || t.status == UsdtTradeStatus::UnderpaidPending
                        })
                    });
                    if has_active {
                        continue;
                    }

                    if order.side == OrderSide::Sell {
                        let unfilled = order.nex_amount.saturating_sub(order.filled_amount);
                        if !unfilled.is_zero() {
                            T::Currency::unreserve(&account, unfilled);
                        }
                    } else if !order.buyer_deposit.is_zero() {
                        T::Currency::unreserve(&account, order.buyer_deposit);
                    }
                    order.status = OrderStatus::Cancelled;
                    Orders::<T>::insert(order_id, &order);
                    Self::remove_from_order_book(order_id, order.side);
                    Self::update_best_price_on_remove(order.usdt_price, order.side);
                    Self::deposit_event(Event::OrderCancelled { order_id });
                }
            }

            Self::deposit_event(Event::UserBanned { account });
            Ok(())
        }

        /// 解封用户（MarketAdmin）
        ///
        /// M-4 审计修复: 同时重置信用分到 500，清除暂停状态，
        /// 否则信用分为 0 的用户解封后仍被 ensure_not_suspended 阻止。
        #[pallet::call_index(29)]
        #[pallet::weight(Weight::from_parts(40_000_000, 5_000))]
        pub fn unban_user(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            BannedAccounts::<T>::remove(&account);
            // M-4: 重置信用档案，确保解封后用户可正常交易
            Self::do_reset_credit(&account);
            Self::deposit_event(Event::UserUnbanned { account });
            Ok(())
        }

        /// 提交争议反驳证据（B2: 对方反驳）
        #[pallet::call_index(30)]
        #[pallet::weight(Weight::from_parts(50_000_000, 5_000))]
        pub fn submit_counter_evidence(
            origin: OriginFor<T>,
            trade_id: u64,
            evidence_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.buyer == who || trade.seller == who,
                Error::<T>::NotTradeParticipant
            );

            let mut dispute =
                TradeDisputeStore::<T>::get(trade_id).ok_or(Error::<T>::DisputeNotFound)?;
            ensure!(
                dispute.status == DisputeStatus::Open,
                Error::<T>::DisputeAlreadyClosed
            );
            // 不能自己反驳自己
            ensure!(dispute.initiator != who, Error::<T>::NotTradeParticipant);
            // W3: 反驳证据一经提交不可覆盖
            ensure!(
                dispute.counter_evidence_cid.is_none(),
                Error::<T>::CounterEvidenceAlreadySubmitted
            );

            let cid: BoundedVec<u8, ConstU32<128>> = evidence_cid
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::ArithmeticOverflow)?;

            dispute.counter_evidence_cid = Some(cid);
            dispute.counter_party = Some(who.clone());
            TradeDisputeStore::<T>::insert(trade_id, dispute);

            Self::deposit_event(Event::CounterEvidenceSubmitted {
                trade_id,
                party: who,
                evidence_cid,
            });

            Ok(())
        }

        /// 修改订单数量（S2: 订单所有者）
        ///
        /// 仅允许无活跃交易的订单修改。
        /// 新数量不能低于已成交量。
        #[pallet::call_index(31)]
        #[pallet::weight(Weight::from_parts(60_000_000, 5_000))]
        pub fn update_order_amount(
            origin: OriginFor<T>,
            order_id: u64,
            new_amount: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_market_active()?;

            ensure!(!new_amount.is_zero(), Error::<T>::AmountTooSmall);
            ensure!(
                new_amount >= T::MinOrderNexAmount::get(),
                Error::<T>::OrderAmountBelowMinimum
            );
            ensure!(
                new_amount <= T::MaxOrderNexAmount::get(),
                Error::<T>::OrderAmountTooLarge
            );

            let mut order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.maker == who, Error::<T>::NotOrderOwner);
            ensure!(
                order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
                Error::<T>::OrderClosed
            );
            ensure!(
                new_amount >= order.filled_amount,
                Error::<T>::AmountBelowFilledAmount
            );

            let trades = OrderTrades::<T>::get(order_id);
            let has_active = trades.iter().any(|&tid| {
                UsdtTrades::<T>::get(tid).map_or(false, |t| {
                    t.status == UsdtTradeStatus::AwaitingPayment
                        || t.status == UsdtTradeStatus::AwaitingVerification
                        || t.status == UsdtTradeStatus::UnderpaidPending
                })
            });
            ensure!(!has_active, Error::<T>::OrderHasActiveTrades);

            let old_amount = order.nex_amount;
            let now = <frame_system::Pallet<T>>::block_number();
            ensure!(now <= order.expires_at, Error::<T>::OrderExpired);

            if order.side == OrderSide::Sell {
                let old_unfilled = old_amount.saturating_sub(order.filled_amount);
                let new_unfilled = new_amount.saturating_sub(order.filled_amount);
                if new_unfilled > old_unfilled {
                    let diff = new_unfilled.saturating_sub(old_unfilled);
                    T::Currency::reserve(&who, diff)
                        .map_err(|_| Error::<T>::InsufficientBalance)?;
                } else if old_unfilled > new_unfilled {
                    let diff = old_unfilled.saturating_sub(new_unfilled);
                    T::Currency::unreserve(&who, diff);
                }
            } else {
                // C1: 买单修改数量 → 基于实际锁定保证金做差额调整
                // BUG-1修复: 使用 order.buyer_deposit（实际链上锁定值），而非重算值
                // 重算值受汇率变动影响，与实际锁定金额不一致会导致资金永久锁定
                let old_deposit = order.buyer_deposit;

                let new_unfilled = new_amount.saturating_sub(order.filled_amount);
                let new_unfilled_u128: u128 = new_unfilled.saturated_into();
                let new_usdt = u64::try_from(
                    new_unfilled_u128
                        .saturating_mul(order.usdt_price as u128)
                        .saturating_div(1_000_000_000_000u128),
                )
                .map_err(|_| Error::<T>::ArithmeticOverflow)?;
                let base_deposit = Self::calculate_buyer_deposit(new_usdt)?;
                let new_deposit = Self::apply_credit_discount(&who, base_deposit);

                if new_deposit > old_deposit {
                    let diff = new_deposit.saturating_sub(old_deposit);
                    T::Currency::reserve(&who, diff)
                        .map_err(|_| Error::<T>::InsufficientDepositBalance)?;
                } else if old_deposit > new_deposit {
                    let diff = old_deposit.saturating_sub(new_deposit);
                    T::Currency::unreserve(&who, diff);
                }
                order.buyer_deposit = new_deposit;
            }

            order.nex_amount = new_amount;
            Orders::<T>::insert(order_id, &order);

            Self::deposit_event(Event::OrderAmountUpdated {
                order_id,
                old_amount,
                new_amount,
            });

            Ok(())
        }

        /// 批量强制结算交易（A2: MarketAdmin，上限 20 笔）
        #[pallet::call_index(32)]
        #[pallet::weight(Weight::from_parts(
            120_000_000u64.saturating_mul(trade_ids.len().min(20) as u64).saturating_add(30_000_000),
            10_000u64.saturating_mul(trade_ids.len().min(20) as u64).saturating_add(5_000),
        ))]
        pub fn batch_force_settle(
            origin: OriginFor<T>,
            trade_ids: BoundedVec<u64, ConstU32<20>>,
            actual_amount: u64,
            resolution: DisputeResolution,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            let mut settled: u32 = 0;
            let mut failed: u32 = 0;
            for trade_id in trade_ids.iter() {
                let result = Self::do_force_settle(*trade_id, actual_amount, resolution);
                if result.is_ok() {
                    settled += 1;
                } else {
                    failed += 1;
                }
            }

            Self::deposit_event(Event::BatchForceSettled {
                settled_count: settled,
                failed_count: failed,
            });
            Ok(())
        }

        /// 批量强制取消交易（A2: MarketAdmin，上限 20 笔）
        #[pallet::call_index(33)]
        #[pallet::weight(Weight::from_parts(
            100_000_000u64.saturating_mul(trade_ids.len().min(20) as u64).saturating_add(30_000_000),
            8_000u64.saturating_mul(trade_ids.len().min(20) as u64).saturating_add(5_000),
        ))]
        pub fn batch_force_cancel(
            origin: OriginFor<T>,
            trade_ids: BoundedVec<u64, ConstU32<20>>,
        ) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            let mut cancelled: u32 = 0;
            let mut failed: u32 = 0;
            for trade_id in trade_ids.iter() {
                let result = Self::do_force_cancel(*trade_id);
                if result.is_ok() {
                    cancelled += 1;
                } else {
                    failed += 1;
                }
            }

            Self::deposit_event(Event::BatchForceCancelled {
                cancelled_count: cancelled,
                failed_count: failed,
            });
            Ok(())
        }

        /// 设置种子 Tron 收款地址（MarketAdmin 权限）
        ///
        /// 链上可治理修改 seed_liquidity 使用的 Tron 收款地址。
        /// 未设置时回退到 Config 中编译时默认值。
        #[pallet::call_index(35)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn set_seed_tron_address(origin: OriginFor<T>, new_address: Vec<u8>) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            // M-9 审计修复: 使用完整 Base58Check 校验（与 place_sell_order 等一致）
            ensure!(
                pallet_trading_common::is_valid_tron_address(&new_address),
                Error::<T>::InvalidTronAddress
            );
            let bounded: TronAddress = new_address
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;
            SeedTronAddressStore::<T>::put(bounded.clone());
            Self::deposit_event(Event::SeedTronAddressUpdated {
                new_address: bounded,
            });
            Ok(())
        }

        // ==================== Indexer Extrinsics ====================

        /// 注册 Indexer 节点
        ///
        /// 质押 MinIndexerStake → 写入 IndexerSet → IndexerCount++
        #[pallet::call_index(36)]
        #[pallet::weight(T::WeightInfo::register_indexer())]
        pub fn register_indexer(origin: OriginFor<T>, endpoint_url: Vec<u8>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                !IndexerSet::<T>::contains_key(&who),
                Error::<T>::IndexerAlreadyRegistered
            );
            ensure!(
                IndexerCount::<T>::get() < T::MaxIndexers::get(),
                Error::<T>::MaxIndexersReached
            );

            let stake = T::MinIndexerStake::get();
            T::Currency::reserve(&who, stake).map_err(|_| Error::<T>::InsufficientIndexerStake)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let bounded_url: BoundedVec<u8, ConstU32<256>> = endpoint_url
                .try_into()
                .map_err(|_| Error::<T>::InvalidEndpointUrl)?;

            let info = IndexerInfo {
                endpoint_url: bounded_url,
                stake,
                registered_at: now,
                accelerated_count: 0,
                error_count: 0,
                pending_hint_count: 0,
                suspended: false,
            };

            IndexerSet::<T>::insert(&who, info);
            IndexerCount::<T>::mutate(|c| *c = c.saturating_add(1));

            Self::deposit_event(Event::IndexerRegistered {
                indexer: who,
                stake,
            });

            Ok(())
        }

        /// 退出 Indexer
        ///
        /// 检查 pending_hint_count==0 → 解除质押 → 移除 → IndexerCount--
        #[pallet::call_index(37)]
        #[pallet::weight(T::WeightInfo::deregister_indexer())]
        pub fn deregister_indexer(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let info = IndexerSet::<T>::get(&who).ok_or(Error::<T>::IndexerNotFound)?;
            ensure!(
                info.pending_hint_count == 0,
                Error::<T>::IndexerHasPendingHints
            );

            T::Currency::unreserve(&who, info.stake);

            let stake_returned = info.stake;
            IndexerSet::<T>::remove(&who);
            IndexerCount::<T>::mutate(|c| *c = c.saturating_sub(1));

            Self::deposit_event(Event::IndexerDeregistered {
                indexer: who,
                stake_returned,
            });

            Ok(())
        }

        /// Indexer 提交 hint（tx_hash + 金额）
        ///
        /// 先到先得：每笔 trade 只接受一个 hint
        #[pallet::call_index(38)]
        #[pallet::weight(T::WeightInfo::submit_indexer_hint())]
        pub fn submit_indexer_hint(
            origin: OriginFor<T>,
            trade_id: u64,
            tx_hash: Vec<u8>,
            reported_amount: u64,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            // 检查 Indexer 身份
            let mut indexer_info = IndexerSet::<T>::get(&who).ok_or(Error::<T>::IndexerNotFound)?;
            ensure!(!indexer_info.suspended, Error::<T>::IndexerIsSuspended);

            // 检查 trade 存在且处于待验证/待付款状态
            let trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingVerification
                    || trade.status == UsdtTradeStatus::AwaitingPayment,
                Error::<T>::InvalidTradeStatus
            );

            // 先到先得：检查该 trade 是否已有 hint
            ensure!(
                !IndexerHints::<T>::contains_key(trade_id),
                Error::<T>::HintAlreadyExists
            );

            // tx_hash 防重放
            let bounded_hash: TxHash = tx_hash
                .try_into()
                .map_err(|_| Error::<T>::InvalidTronAddress)?;
            ensure!(
                !UsedTxHashes::<T>::contains_key(&bounded_hash),
                Error::<T>::TxHashAlreadyUsed
            );

            // 金额安全检查（不超过预期 10 倍）
            let amount_cap = trade.usdt_amount.saturating_mul(10);
            ensure!(reported_amount <= amount_cap, Error::<T>::HintAmountInsane);

            let now = <frame_system::Pallet<T>>::block_number();

            let hint = IndexerHint {
                indexer: who.clone(),
                tx_hash: bounded_hash.clone(),
                reported_amount,
                submitted_at: now,
                status: IndexerHintStatus::Pending,
            };

            IndexerHints::<T>::insert(trade_id, hint);
            indexer_info.pending_hint_count = indexer_info.pending_hint_count.saturating_add(1);
            IndexerSet::<T>::insert(&who, indexer_info);

            Self::deposit_event(Event::IndexerHintSubmitted {
                trade_id,
                indexer: who,
                tx_hash: bounded_hash,
                reported_amount,
            });

            Ok(())
        }

        /// OCW 报告 Indexer hint 不匹配
        ///
        /// 移除 hint 条目，释放 trade hint 槽位允许重新提交
        #[pallet::call_index(39)]
        #[pallet::weight(T::WeightInfo::report_indexer_mismatch())]
        pub fn report_indexer_mismatch(origin: OriginFor<T>, trade_id: u64) -> DispatchResult {
            ensure_none(origin)?;

            let hint = IndexerHints::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                hint.status == IndexerHintStatus::Pending,
                Error::<T>::InvalidTradeStatus
            );

            let indexer_account = hint.indexer.clone();

            // 移除 hint → 释放槽位
            IndexerHints::<T>::remove(trade_id);

            // 更新 Indexer 统计
            if let Some(mut info) = IndexerSet::<T>::get(&indexer_account) {
                info.error_count = info.error_count.saturating_add(1);
                info.pending_hint_count = info.pending_hint_count.saturating_sub(1);

                // 超阈值则暂停 + slash 质押
                if info.error_count >= T::MaxIndexerErrors::get() {
                    info.suspended = true;

                    // Slash 质押
                    let slashed = Self::slash_indexer_stake(&indexer_account, &mut info);

                    Self::deposit_event(Event::IndexerSuspended {
                        indexer: indexer_account.clone(),
                        error_count: info.error_count,
                    });

                    if !slashed.is_zero() {
                        Self::deposit_event(Event::IndexerStakeSlashed {
                            indexer: indexer_account.clone(),
                            slashed,
                            remaining_stake: info.stake,
                        });
                    }
                }

                IndexerSet::<T>::insert(&indexer_account, info);
            }

            Self::deposit_event(Event::IndexerHintMismatch {
                trade_id,
                indexer: indexer_account,
            });

            Ok(())
        }

        /// 管理员强制移除 Indexer
        ///
        /// 清理该 Indexer 的所有 pending hints（释放 trade 槽位），
        /// 退还质押（扣除 slash 部分），移除注册记录。
        #[pallet::call_index(40)]
        #[pallet::weight(T::WeightInfo::force_remove_indexer())]
        pub fn force_remove_indexer(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            let info = IndexerSet::<T>::get(&account).ok_or(Error::<T>::IndexerNotFound)?;

            // 清理该 Indexer 的所有 pending hints（避免孤儿 hint）
            let cleaned = Self::cleanup_indexer_hints(&account);
            if cleaned > 0 {
                Self::deposit_event(Event::IndexerOrphanHintsCleaned {
                    indexer: account.clone(),
                    cleaned_count: cleaned,
                });
            }

            T::Currency::unreserve(&account, info.stake);

            IndexerSet::<T>::remove(&account);
            IndexerCount::<T>::mutate(|c| *c = c.saturating_sub(1));

            Self::deposit_event(Event::IndexerForceRemoved { indexer: account });

            Ok(())
        }

        /// 设置 Indexer 奖池从 trading_fee 中的分成比例（MarketAdmin）
        ///
        /// 每笔交易结算时，trading_fee × pool_share_bps / 10000 → Indexer 奖池，
        /// 剩余部分 → 国库。最大 10000 bps = 100%。
        #[pallet::call_index(41)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn set_indexer_pool_share(origin: OriginFor<T>, share_bps: u16) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            ensure!(share_bps <= 10000, Error::<T>::FeeTooHigh);

            let old_share = IndexerPoolShareBps::<T>::get();
            IndexerPoolShareBps::<T>::put(share_bps);

            Self::deposit_event(Event::IndexerPoolShareUpdated {
                old_share_bps: old_share,
                new_share_bps: share_bps,
            });

            Ok(())
        }

        /// 管理员恢复被暂停的 Indexer
        ///
        /// 重置 suspended=false + error_count=0，允许 Indexer 继续提交 hint。
        #[pallet::call_index(42)]
        #[pallet::weight(T::WeightInfo::reinstate_indexer())]
        pub fn reinstate_indexer(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;

            IndexerSet::<T>::try_mutate(&account, |maybe_info| -> DispatchResult {
                let info = maybe_info.as_mut().ok_or(Error::<T>::IndexerNotFound)?;
                ensure!(info.suspended, Error::<T>::IndexerNotSuspended);

                info.suspended = false;
                info.error_count = 0;

                Self::deposit_event(Event::IndexerReinstated {
                    indexer: account.clone(),
                });

                Ok(())
            })
        }

        /// M-4 审计修复: 管理员重置买家信用分
        ///
        /// 将指定用户的信用档案重置为默认状态（score=500），
        /// 清除暂停标志和违约记录，保留累计完成交易数。
        /// 不影响 BannedAccounts 状态（需单独 unban_user）。
        #[pallet::call_index(43)]
        #[pallet::weight(Weight::from_parts(30_000_000, 4_000))]
        pub fn admin_reset_credit(origin: OriginFor<T>, account: T::AccountId) -> DispatchResult {
            T::MarketAdminOrigin::ensure_origin(origin)?;
            Self::do_reset_credit(&account);
            Ok(())
        }
    }

    // ==================== ValidateUnsigned ====================

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            // 安全：拒绝 External 来源（P2P 广播、远程 RPC），只接受本地 OCW 或已入块交易
            if matches!(source, TransactionSource::External) {
                return InvalidTransaction::BadSigner.into();
            }

            match call {
                Call::submit_ocw_result {
                    trade_id,
                    actual_amount,
                    tx_hashes,
                } => {
                    let trade = match UsdtTrades::<T>::get(trade_id) {
                        Some(t) => t,
                        None => return InvalidTransaction::Custom(10).into(),
                    };
                    if trade.status != UsdtTradeStatus::AwaitingVerification {
                        return InvalidTransaction::Custom(11).into();
                    }
                    if OcwVerificationResults::<T>::contains_key(trade_id) {
                        return InvalidTransaction::Custom(12).into();
                    }
                    // B1: tx_hashes 必须非空，且每个 hash 不能已被使用
                    if tx_hashes.is_empty() {
                        return InvalidTransaction::Custom(13).into();
                    }
                    for hash in tx_hashes.iter() {
                        if UsedTxHashes::<T>::contains_key(hash) {
                            return InvalidTransaction::Custom(13).into();
                        }
                    }
                    // M-7 审计修复: 最小交易年龄检查（防止未确认 TRON 交易被提交）
                    // 19 confirmations × 3s = 57s ≈ ~10 blocks (6s/block)，取 10 blocks 作为安全下限
                    {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let trade_age = now.saturating_sub(trade.created_at);
                        let min_blocks: BlockNumberFor<T> = 10u32.into();
                        if trade_age < min_blocks {
                            return InvalidTransaction::Custom(15).into();
                        }
                    }
                    // C4: actual_amount 安全边界检查 (M-8 审计修复: 10x→3x)
                    // 拒绝明显伪造的金额（超过预期 3 倍）防止恶意强制 Overpaid
                    let amount_cap = trade.usdt_amount.saturating_mul(3);
                    if *actual_amount > amount_cap {
                        return InvalidTransaction::Custom(14).into();
                    }
                    let priority = match source {
                        TransactionSource::Local => 100,
                        TransactionSource::InBlock => 80,
                        TransactionSource::External => 0,
                    };
                    // M3: propagate(false) — unsigned 仅来自本地 OCW，不应广播给节点
                    ValidTransaction::with_tag_prefix("NexMarketOcwResult")
                        .priority(priority)
                        .longevity(10)
                        .and_provides([&(b"nex_ocw", trade_id)])
                        .propagate(false)
                        .build()
                }
                Call::auto_confirm_payment {
                    trade_id,
                    actual_amount,
                    tx_hashes,
                } => {
                    let trade = match UsdtTrades::<T>::get(trade_id) {
                        Some(t) => t,
                        None => return InvalidTransaction::Custom(20).into(),
                    };
                    if trade.status != UsdtTradeStatus::AwaitingPayment {
                        return InvalidTransaction::Custom(21).into();
                    }
                    if trade.buyer_tron_address.is_none() {
                        return InvalidTransaction::Custom(22).into();
                    }
                    // B1: tx_hashes 必须非空，且每个 hash 不能已被使用
                    if tx_hashes.is_empty() {
                        return InvalidTransaction::Custom(23).into();
                    }
                    for hash in tx_hashes.iter() {
                        if UsedTxHashes::<T>::contains_key(hash) {
                            return InvalidTransaction::Custom(23).into();
                        }
                    }
                    // CRITICAL-1 审计修复: 最小交易年龄检查（与 submit_ocw_result 对齐）
                    // 19 confirmations × 3s = 57s ≈ ~10 blocks (6s/block)，防止未确认 TRON 交易被提交
                    {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let trade_age = now.saturating_sub(trade.created_at);
                        let min_blocks: BlockNumberFor<T> = 10u32.into();
                        if trade_age < min_blocks {
                            return InvalidTransaction::Custom(25).into();
                        }
                    }
                    // C4: actual_amount 安全边界检查 (M-8 审计修复: 10x→3x)
                    let amount_cap = trade.usdt_amount.saturating_mul(3);
                    if *actual_amount > amount_cap {
                        return InvalidTransaction::Custom(24).into();
                    }
                    let priority = match source {
                        TransactionSource::Local => 90,
                        TransactionSource::InBlock => 70,
                        TransactionSource::External => 0,
                    };
                    // M3: propagate(false) — unsigned 仅来自本地 OCW
                    ValidTransaction::with_tag_prefix("NexMarketAutoConfirm")
                        .priority(priority)
                        .longevity(10)
                        .and_provides([&(b"nex_auto", trade_id)])
                        .propagate(false)
                        .build()
                }
                Call::submit_underpaid_update {
                    trade_id,
                    new_actual_amount,
                    new_tx_hashes,
                } => {
                    let trade = match UsdtTrades::<T>::get(trade_id) {
                        Some(t) => t,
                        None => return InvalidTransaction::Custom(30).into(),
                    };
                    if trade.status != UsdtTradeStatus::UnderpaidPending {
                        return InvalidTransaction::Custom(31).into();
                    }
                    // C4: new_actual_amount 安全边界检查 (M-8 审计修复: 10x→3x)
                    let amount_cap = trade.usdt_amount.saturating_mul(3);
                    if *new_actual_amount > amount_cap {
                        return InvalidTransaction::Custom(32).into();
                    }
                    // C4: 单调递增检查（避免无效更新浪费区块空间）
                    let previous_amount = OcwVerificationResults::<T>::get(trade_id)
                        .map(|(_, amt)| amt)
                        .unwrap_or(0);
                    if *new_actual_amount <= previous_amount {
                        return InvalidTransaction::Custom(33).into();
                    }
                    // P1-8: 补付更新必须携带至少一个新 tx_hash
                    if new_tx_hashes.is_empty() {
                        return InvalidTransaction::Custom(36).into();
                    }
                    // B2: 每个新 tx_hash 重放检查
                    for hash in new_tx_hashes.iter() {
                        if UsedTxHashes::<T>::contains_key(hash) {
                            return InvalidTransaction::Custom(37).into();
                        }
                    }
                    let priority = match source {
                        TransactionSource::Local => 80,
                        TransactionSource::InBlock => 60,
                        TransactionSource::External => 0,
                    };
                    // M3: propagate(false) — unsigned 仅来自本地 OCW
                    ValidTransaction::with_tag_prefix("NexMarketUnderpaidUpdate")
                        .priority(priority)
                        .longevity(5)
                        .and_provides([&(b"nex_upd", trade_id)])
                        .propagate(false)
                        .build()
                }
                Call::report_indexer_mismatch { trade_id } => {
                    // 验证 hint 存在且为 Pending
                    let hint = match IndexerHints::<T>::get(trade_id) {
                        Some(h) => h,
                        None => return InvalidTransaction::Custom(40).into(),
                    };
                    if hint.status != IndexerHintStatus::Pending {
                        return InvalidTransaction::Custom(41).into();
                    }
                    ValidTransaction::with_tag_prefix("NexMarketIndexerMismatch")
                        .priority(70)
                        .longevity(10)
                        .and_provides([&(b"nex_idx", trade_id)])
                        .propagate(false)
                        .build()
                }
                _ => InvalidTransaction::Call.into(),
            }
        }
    }

    // ==================== 内部函数 ====================

    impl<T: Config> Pallet<T> {
        /// 创建订单
        fn do_create_order(
            maker: T::AccountId,
            side: OrderSide,
            nex_amount: BalanceOf<T>,
            usdt_price: u64,
            tron_address: Option<TronAddress>,
            buyer_deposit: BalanceOf<T>,
            deposit_waived: bool,
            min_fill_amount: BalanceOf<T>,
        ) -> Result<u64, DispatchError> {
            frame_support::storage::with_transaction(|| {
                let result = (|| -> Result<u64, DispatchError> {
                    let order_id = NextOrderId::<T>::get();
                    ensure!(order_id < u64::MAX, Error::<T>::ArithmeticOverflow);
                    NextOrderId::<T>::put(order_id.saturating_add(1));

                    let now = <frame_system::Pallet<T>>::block_number();
                    let ttl: u32 = T::DefaultOrderTTL::get();
                    let expires_at = now.saturating_add(ttl.into());

                    let order = Order {
                        order_id,
                        maker: maker.clone(),
                        side,
                        nex_amount,
                        filled_amount: Zero::zero(),
                        usdt_price,
                        tron_address,
                        status: OrderStatus::Open,
                        created_at: now,
                        expires_at,
                        buyer_deposit,
                        deposit_waived,
                        min_fill_amount,
                    };

                    Orders::<T>::insert(order_id, order);

                    match side {
                        OrderSide::Sell => {
                            SellOrders::<T>::try_mutate(|orders| {
                                orders
                                    .try_push(order_id)
                                    .map_err(|_| Error::<T>::OrderBookFull)
                            })?;
                        }
                        OrderSide::Buy => {
                            BuyOrders::<T>::try_mutate(|orders| {
                                orders
                                    .try_push(order_id)
                                    .map_err(|_| Error::<T>::OrderBookFull)
                            })?;
                        }
                    }

                    UserOrders::<T>::try_mutate(&maker, |orders| {
                        orders
                            .try_push(order_id)
                            .map_err(|_| Error::<T>::UserOrdersFull)
                    })?;

                    MarketStatsStore::<T>::mutate(|stats| {
                        stats.total_orders = stats.total_orders.saturating_add(1);
                    });

                    Ok(order_id)
                })();
                match result {
                    Ok(v) => frame_support::storage::TransactionOutcome::Commit(Ok(v)),
                    Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
                }
            })
        }

        /// 创建 USDT 交易（标准超时）
        fn do_create_usdt_trade(
            order_id: u64,
            seller: T::AccountId,
            buyer: T::AccountId,
            nex_amount: BalanceOf<T>,
            usdt_amount: u64,
            seller_tron_address: TronAddress,
            buyer_tron_address: Option<TronAddress>,
            buyer_deposit: BalanceOf<T>,
        ) -> Result<u64, DispatchError> {
            Self::do_create_usdt_trade_ex(
                order_id,
                seller,
                buyer,
                nex_amount,
                usdt_amount,
                seller_tron_address,
                buyer_tron_address,
                buyer_deposit,
                false,
            )
        }

        /// 创建 USDT 交易（扩展版，支持免保证金短超时）
        fn do_create_usdt_trade_ex(
            order_id: u64,
            seller: T::AccountId,
            buyer: T::AccountId,
            nex_amount: BalanceOf<T>,
            usdt_amount: u64,
            seller_tron_address: TronAddress,
            buyer_tron_address: Option<TronAddress>,
            buyer_deposit: BalanceOf<T>,
            waived_deposit: bool,
        ) -> Result<u64, DispatchError> {
            frame_support::storage::with_transaction(|| {
                let result = (|| -> Result<u64, DispatchError> {
                    let trade_id = NextUsdtTradeId::<T>::get();
                    ensure!(trade_id < u64::MAX, Error::<T>::ArithmeticOverflow);
                    NextUsdtTradeId::<T>::put(trade_id.saturating_add(1));

                    let now = <frame_system::Pallet<T>>::block_number();
                    // 免保证金交易使用短超时，普通交易使用标准超时
                    let timeout: u32 = if waived_deposit {
                        T::FirstOrderTimeout::get()
                    } else {
                        T::UsdtTimeout::get()
                    };
                    let timeout_at = now.saturating_add(timeout.into());

                    let deposit_status = if buyer_deposit.is_zero() {
                        BuyerDepositStatus::None
                    } else {
                        BuyerDepositStatus::Locked
                    };

                    let trade = UsdtTrade {
                        trade_id,
                        order_id,
                        seller,
                        buyer,
                        nex_amount,
                        usdt_amount,
                        seller_tron_address,
                        buyer_tron_address,
                        status: UsdtTradeStatus::AwaitingPayment,
                        created_at: now,
                        timeout_at,
                        buyer_deposit,
                        deposit_status,
                        first_verified_at: None,
                        first_actual_amount: None,
                        underpaid_deadline: None,
                        completed_at: None,
                        payment_confirmed: false,
                        cumulative_penalty: Zero::zero(),
                    };

                    UsdtTrades::<T>::insert(trade_id, &trade);

                    // 加入待付款跟踪队列
                    AwaitingPaymentTrades::<T>::try_mutate(|list| {
                        list.try_push(trade_id)
                            .map_err(|_| Error::<T>::AwaitingPaymentQueueFull)
                    })?;

                    // #2/#12: 用户交易索引
                    UserTrades::<T>::try_mutate(&trade.seller, |list| {
                        list.try_push(trade_id)
                            .map_err(|_| Error::<T>::UserTradesFull)
                    })?;
                    if trade.seller != trade.buyer {
                        UserTrades::<T>::try_mutate(&trade.buyer, |list| {
                            list.try_push(trade_id)
                                .map_err(|_| Error::<T>::UserTradesFull)
                        })?;
                    }

                    // #5: 订单-交易索引
                    OrderTrades::<T>::try_mutate(order_id, |list| {
                        list.try_push(trade_id)
                            .map_err(|_| Error::<T>::OrderTradesFull)
                    })?;

                    Ok(trade_id)
                })();
                match result {
                    Ok(v) => frame_support::storage::TransactionOutcome::Commit(Ok(v)),
                    Err(e) => frame_support::storage::TransactionOutcome::Rollback(Err(e)),
                }
            })
        }

        /// 计算买家保证金（使用实时市场价格）
        ///
        /// Oracle 不可用时返回 `OracleUnavailable` 错误，阻止下单。
        fn calculate_buyer_deposit(usdt_amount: u64) -> Result<BalanceOf<T>, Error<T>> {
            let rate = T::BuyerDepositRate::get(); // bps
                                                   // 先计算 USDT 保证金额
            let usdt_deposit = (usdt_amount as u64)
                .saturating_mul(rate as u64)
                .saturating_div(10000);

            // 用 DepositCalculator（实时价格）换算成 NEX
            // fallback = 0：Oracle 挂时返回 0，由调用方阻止交易
            let min_deposit =
                T::DepositCalculator::calculate_deposit(T::MinBuyerDepositUsd::get(), Zero::zero());
            ensure!(!min_deposit.is_zero(), Error::<T>::OracleUnavailable);
            let calculated = T::DepositCalculator::calculate_deposit(
                usdt_deposit,
                min_deposit, // 汇率已确认可用，此 fallback 不会命中
            );

            Ok(if calculated > min_deposit {
                calculated
            } else {
                min_deposit
            })
        }

        /// 多档判定（委托给 pallet-trading-common）
        fn calculate_payment_verification_result(
            expected_amount: u64,
            actual_amount: u64,
        ) -> PaymentVerificationResult {
            pallet_trading_common::calculate_payment_verification_result(
                expected_amount,
                actual_amount,
            )
        }

        /// 手动结算交易（R1 简化：submit_ocw_result 已自动结算，此为兜底）
        ///
        /// 当 submit_ocw_result 因异常未能自动结算时，任何人可手动触发。
        fn do_claim_verification_reward(caller: &T::AccountId, trade_id: u64) -> DispatchResult {
            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingVerification,
                Error::<T>::InvalidTradeStatus
            );

            let (verification_result, actual_amount) =
                OcwVerificationResults::<T>::get(trade_id).ok_or(Error::<T>::OcwResultNotFound)?;

            match verification_result {
                PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                    Self::process_full_payment(&mut trade, trade_id)?;
                }
                PaymentVerificationResult::Underpaid
                | PaymentVerificationResult::SeverelyUnderpaid => {
                    Self::process_underpaid(&mut trade, trade_id, actual_amount)?;
                }
                PaymentVerificationResult::Invalid => {
                    Self::process_underpaid(&mut trade, trade_id, 0)?;
                }
            }

            PendingUsdtTrades::<T>::mutate(|pending| {
                pending.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);

            Self::deposit_event(Event::VerificationRewardClaimed {
                trade_id,
                claimer: caller.clone(),
                reward: Zero::zero(),
                reward_paid: true,
            });

            Ok(())
        }

        /// 统一手续费扣取 + Indexer 奖池分成
        ///
        /// 从卖家 reserved NEX 中扣除 trading_fee：
        /// - pool_share = fee × IndexerPoolShareBps / 10000 → RewardSource（奖池）
        /// - treasury_share = fee - pool_share → TreasuryAccount（国库）
        ///
        /// 返回实际收取的总手续费（用于计算 nex_to_buyer）。
        fn charge_trading_fee_split(
            trade_id: u64,
            seller: &T::AccountId,
            fee_base: BalanceOf<T>,
        ) -> Result<BalanceOf<T>, DispatchError> {
            let fee_bps = TradingFeeBps::<T>::get();
            if fee_bps == 0 || fee_base.is_zero() {
                return Ok(Zero::zero());
            }

            let base_u128: u128 = fee_base.saturated_into();
            let total_fee: u128 = base_u128
                .saturating_mul(fee_bps as u128)
                .saturating_div(10000);
            let total_fee_bal: BalanceOf<T> = total_fee.saturated_into();
            if total_fee_bal.is_zero() {
                return Ok(Zero::zero());
            }

            // 计算 Indexer 奖池分成
            let pool_share_bps = IndexerPoolShareBps::<T>::get();
            let pool_share: u128 = total_fee
                .saturating_mul(pool_share_bps as u128)
                .saturating_div(10000);
            let pool_share_bal: BalanceOf<T> = pool_share.saturated_into();
            let treasury_share_bal = total_fee_bal.saturating_sub(pool_share_bal);

            let treasury = T::TreasuryAccount::get();
            let reward_pool = T::RewardSource::get();
            let mut actually_charged = BalanceOf::<T>::zero();

            // 1. 国库份额
            if !treasury_share_bal.is_zero() {
                let charged = match T::Currency::repatriate_reserved(
                    seller,
                    &treasury,
                    treasury_share_bal,
                    frame_support::traits::BalanceStatus::Free,
                ) {
                    Ok(remaining) => treasury_share_bal.saturating_sub(remaining),
                    Err(_) => Zero::zero(),
                };
                actually_charged = actually_charged.saturating_add(charged);
                if !charged.is_zero() {
                    Self::deposit_event(Event::TradingFeeCharged {
                        trade_id,
                        fee_amount: charged,
                        to_treasury: treasury.clone(),
                    });
                }
            }

            // 2. Indexer 奖池份额
            if !pool_share_bal.is_zero() {
                let charged = match T::Currency::repatriate_reserved(
                    seller,
                    &reward_pool,
                    pool_share_bal,
                    frame_support::traits::BalanceStatus::Free,
                ) {
                    Ok(remaining) => pool_share_bal.saturating_sub(remaining),
                    Err(_) => Zero::zero(),
                };
                actually_charged = actually_charged.saturating_add(charged);
                if !charged.is_zero() {
                    Self::deposit_event(Event::IndexerPoolFunded {
                        trade_id,
                        amount: charged,
                        pool_account: reward_pool,
                    });
                }
            }

            Ok(actually_charged)
        }

        /// 处理全额付款
        fn process_full_payment(trade: &mut UsdtTrade<T>, trade_id: u64) -> DispatchResult {
            // #9: 计算并扣除手续费（含 Indexer 奖池分成）
            let fee_amount =
                Self::charge_trading_fee_split(trade_id, &trade.seller, trade.nex_amount)?;

            // 释放锁定的 NEX 给买家（扣除实际收取的手续费后的余额）
            // BUG-2修复: 使用 actually_charged（而非 fee_bal）确保手续费部分转移失败时
            // 未转出部分不会永久锁死在卖家 reserved 中，而是随 NEX 一起转给买家
            let nex_to_buyer = trade.nex_amount.saturating_sub(fee_amount);
            T::Currency::repatriate_reserved(
                &trade.seller,
                &trade.buyer,
                nex_to_buyer,
                frame_support::traits::BalanceStatus::Free,
            )?;

            // 退还保证金（扣除已罚没部分）
            if !trade.buyer_deposit.is_zero() && trade.deposit_status == BuyerDepositStatus::Locked
            {
                let remaining_deposit =
                    trade.buyer_deposit.saturating_sub(trade.cumulative_penalty);
                if !remaining_deposit.is_zero() {
                    T::Currency::unreserve(&trade.buyer, remaining_deposit);
                }
                trade.deposit_status = if trade.cumulative_penalty.is_zero() {
                    BuyerDepositStatus::Released
                } else {
                    BuyerDepositStatus::PartiallyForfeited
                };

                Self::deposit_event(Event::BuyerDepositReleased {
                    trade_id,
                    buyer: trade.buyer.clone(),
                    deposit: remaining_deposit,
                });
            }

            trade.status = UsdtTradeStatus::Completed;
            trade.completed_at = Some(<frame_system::Pallet<T>>::block_number()); // W5
            let usdt_amount = trade.usdt_amount;
            let order_id = trade.order_id;
            UsdtTrades::<T>::insert(trade_id, &*trade);

            Self::finalize_waived_trade_tracking(&trade.buyer, order_id);

            // 信用系统：完成加分 + 减少活跃计数
            Self::credit_on_completion(&trade.buyer);
            ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| *c = c.saturating_sub(1));

            let maybe_order = Orders::<T>::get(order_id);

            MarketStatsStore::<T>::mutate(|stats| {
                stats.total_trades = stats.total_trades.saturating_add(1);
                stats.total_volume_usdt =
                    stats.total_volume_usdt.saturating_add(usdt_amount as u128);
            });

            if let Some(ref order) = maybe_order {
                Self::on_trade_completed(order.usdt_price);
            }

            Self::deposit_event(Event::UsdtTradeCompleted { trade_id, order_id });

            // Indexer hint 奖励：交易成功完成后，检查并奖励 Indexer
            Self::try_reward_indexer_hint(trade_id);

            Ok(())
        }

        /// 处理少付（保证金按梯度没收）
        fn process_underpaid(
            trade: &mut UsdtTrade<T>,
            trade_id: u64,
            actual_amount: u64,
        ) -> DispatchResult {
            let payment_ratio =
                pallet_trading_common::compute_payment_ratio_bps(trade.usdt_amount, actual_amount);

            // 按比例释放 NEX
            let nex_u128: u128 = trade.nex_amount.saturated_into();
            let nex_to_release_u128 = nex_u128
                .saturating_mul(payment_ratio as u128)
                .saturating_div(10000);
            let nex_to_release: BalanceOf<T> = nex_to_release_u128.saturated_into();
            let nex_to_refund = trade.nex_amount.saturating_sub(nex_to_release);

            // BUG-3修复: 少付场景也扣除手续费，与 process_full_payment 一致
            // 手续费基于实际释放的 NEX 计算（而非全额），确保公平
            let fee_amount =
                Self::charge_trading_fee_split(trade_id, &trade.seller, nex_to_release)?;

            // 释放部分 NEX 给买家（扣除手续费）
            let nex_to_buyer = nex_to_release.saturating_sub(fee_amount);
            if !nex_to_buyer.is_zero() {
                T::Currency::repatriate_reserved(
                    &trade.seller,
                    &trade.buyer,
                    nex_to_buyer,
                    frame_support::traits::BalanceStatus::Free,
                )?;
            }

            // 退还剩余 NEX 给卖家
            if !nex_to_refund.is_zero() {
                T::Currency::unreserve(&trade.seller, nex_to_refund);
                Self::rollback_order_filled_amount(trade.order_id, nex_to_refund);
            }

            // 保证金按梯度没收（扣除已通过逾期罚金转移的部分，三方分配）
            let forfeit_rate = Self::calculate_deposit_forfeit_rate(payment_ratio);
            let mut deposit_forfeited = BalanceOf::<T>::zero();
            if !trade.buyer_deposit.is_zero() && trade.deposit_status == BuyerDepositStatus::Locked
            {
                let deposit_u128: u128 = trade.buyer_deposit.saturated_into();
                let total_forfeit_u128 = deposit_u128
                    .saturating_mul(forfeit_rate as u128)
                    .saturating_div(10000);
                let total_forfeit: BalanceOf<T> = total_forfeit_u128.saturated_into();
                // 本次需额外没收 = 梯度总没收 - 已罚没
                let extra_forfeit = total_forfeit.saturating_sub(trade.cumulative_penalty);

                if !extra_forfeit.is_zero() {
                    let (actually_extra, _act_treasury, _act_seller, _act_pool) =
                        Self::distribute_deposit_penalty(
                            &trade.buyer,
                            &trade.seller,
                            extra_forfeit,
                        );
                    deposit_forfeited = trade.cumulative_penalty.saturating_add(actually_extra);
                } else {
                    deposit_forfeited = trade.cumulative_penalty;
                }

                // 退还未没收部分
                let refund = trade.buyer_deposit.saturating_sub(deposit_forfeited);
                if !refund.is_zero() {
                    T::Currency::unreserve(&trade.buyer, refund);
                }

                trade.deposit_status = if forfeit_rate >= 10000 {
                    BuyerDepositStatus::Forfeited
                } else {
                    BuyerDepositStatus::PartiallyForfeited
                };
            }

            trade.status = if actual_amount > 0 {
                UsdtTradeStatus::Completed
            } else {
                UsdtTradeStatus::Refunded
            };
            trade.completed_at = Some(<frame_system::Pallet<T>>::block_number()); // W5
            let expected_amount = trade.usdt_amount;
            let order_id = trade.order_id;
            UsdtTrades::<T>::insert(trade_id, &*trade);

            Self::cleanup_waived_trade_counter(&trade.buyer, order_id);

            // 信用系统：少付/未付扣分 + 减少活跃计数
            if actual_amount > 0 {
                Self::credit_on_underpaid_violation(&trade.buyer);
            } else {
                Self::credit_on_timeout_violation(&trade.buyer);
            }
            ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| *c = c.saturating_sub(1));

            MarketStatsStore::<T>::mutate(|stats| {
                stats.total_trades = stats.total_trades.saturating_add(1);
                stats.total_volume_usdt = stats
                    .total_volume_usdt
                    .saturating_add(actual_amount as u128);
            });

            Self::deposit_event(Event::UnderpaidAutoProcessed {
                trade_id,
                expected_amount,
                actual_amount,
                payment_ratio,
                nex_released: nex_to_release,
                deposit_forfeited,
            });

            // BUG-1修复: 少付场景也应奖励 Indexer（hint 正确但买家少付不是 Indexer 的错）
            Self::try_reward_indexer_hint(trade_id);

            Ok(())
        }

        /// 保证金没收梯度（委托给 pallet-trading-common）
        fn calculate_deposit_forfeit_rate(payment_ratio: u32) -> u16 {
            pallet_trading_common::calculate_deposit_forfeit_rate(payment_ratio)
        }

        /// 三方分配保证金没收：50% 卖家 + (50% × pool_share) Indexer 奖池 + 余量国库
        ///
        /// 返回 (actually_total, to_treasury, to_seller, to_pool)
        fn distribute_deposit_penalty(
            buyer: &T::AccountId,
            seller: &T::AccountId,
            amount: BalanceOf<T>,
        ) -> (BalanceOf<T>, BalanceOf<T>, BalanceOf<T>, BalanceOf<T>) {
            if amount.is_zero() {
                return (Zero::zero(), Zero::zero(), Zero::zero(), Zero::zero());
            }

            let amount_u128: u128 = amount.saturated_into();
            // 卖家固定 50%
            let seller_share_u128 = amount_u128.saturating_div(2);
            let platform_share_u128 = amount_u128.saturating_sub(seller_share_u128);

            // 平台份额中按 IndexerPoolShareBps 划拨给奖池
            let pool_share_bps = IndexerPoolShareBps::<T>::get();
            let pool_share_u128 = platform_share_u128
                .saturating_mul(pool_share_bps as u128)
                .saturating_div(10000);
            let treasury_share_u128 = platform_share_u128.saturating_sub(pool_share_u128);

            let seller_share: BalanceOf<T> = seller_share_u128.saturated_into();
            let pool_share: BalanceOf<T> = pool_share_u128.saturated_into();
            let treasury_share: BalanceOf<T> = treasury_share_u128.saturated_into();

            let treasury = T::TreasuryAccount::get();
            let reward_pool = T::RewardSource::get();
            let mut actually_total = BalanceOf::<T>::zero();
            let mut act_treasury = BalanceOf::<T>::zero();
            let mut act_seller = BalanceOf::<T>::zero();
            let mut act_pool = BalanceOf::<T>::zero();

            // 1. 国库
            if !treasury_share.is_zero() {
                let charged = match T::Currency::repatriate_reserved(
                    buyer,
                    &treasury,
                    treasury_share,
                    frame_support::traits::BalanceStatus::Free,
                ) {
                    Ok(rem) => treasury_share.saturating_sub(rem),
                    Err(_) => Zero::zero(),
                };
                act_treasury = charged;
                actually_total = actually_total.saturating_add(charged);
            }

            // 2. Indexer 奖池
            if !pool_share.is_zero() {
                let charged = match T::Currency::repatriate_reserved(
                    buyer,
                    &reward_pool,
                    pool_share,
                    frame_support::traits::BalanceStatus::Free,
                ) {
                    Ok(rem) => pool_share.saturating_sub(rem),
                    Err(_) => Zero::zero(),
                };
                act_pool = charged;
                actually_total = actually_total.saturating_add(charged);
            }

            // 3. 卖家
            if !seller_share.is_zero() {
                let charged = match T::Currency::repatriate_reserved(
                    buyer,
                    seller,
                    seller_share,
                    frame_support::traits::BalanceStatus::Free,
                ) {
                    Ok(rem) => seller_share.saturating_sub(rem),
                    Err(_) => Zero::zero(),
                };
                act_seller = charged;
                actually_total = actually_total.saturating_add(charged);
            }

            (actually_total, act_treasury, act_seller, act_pool)
        }

        /// 没收买家保证金（完全违约场景：超时未付款 / 验证超时无结果）
        ///
        /// P1 修复：此函数仅用于买家完全违约，固定 100% 没收。
        /// 少付场景走 `process_underpaid()` 的梯度没收逻辑。
        fn forfeit_buyer_deposit(trade: &mut UsdtTrade<T>, trade_id: u64) {
            if trade.buyer_deposit.is_zero() || trade.deposit_status != BuyerDepositStatus::Locked {
                return;
            }

            // 扣除已通过逾期罚金转移的部分
            let remaining = trade.buyer_deposit.saturating_sub(trade.cumulative_penalty);
            if remaining.is_zero() {
                trade.deposit_status = BuyerDepositStatus::Forfeited;
                Self::deposit_event(Event::BuyerDepositForfeited {
                    trade_id,
                    buyer: trade.buyer.clone(),
                    forfeited: trade.cumulative_penalty,
                    to_treasury: Zero::zero(),
                    to_seller: Zero::zero(),
                    to_pool: Zero::zero(),
                });
                return;
            }

            // 三方分配：50% 卖家 + 平台份额按 IndexerPoolShareBps 拆分（奖池 + 国库）
            let (actually_forfeited, act_treasury, act_seller, act_pool) =
                Self::distribute_deposit_penalty(&trade.buyer, &trade.seller, remaining);

            // 安全起见退还未能转移的部分
            let refund = remaining.saturating_sub(actually_forfeited);
            if !refund.is_zero() {
                T::Currency::unreserve(&trade.buyer, refund);
            }

            trade.deposit_status = BuyerDepositStatus::Forfeited;

            let total_forfeited = trade.cumulative_penalty.saturating_add(actually_forfeited);
            Self::deposit_event(Event::BuyerDepositForfeited {
                trade_id,
                buyer: trade.buyer.clone(),
                forfeited: total_forfeited,
                to_treasury: act_treasury,
                to_seller: act_seller,
                to_pool: act_pool,
            });
        }

        /// 回滚订单 filled_amount
        ///
        /// M2 修复：当订单从 Filled 恢复时，重新加入订单簿和用户索引，
        /// 防止订单成为幽灵记录（状态 Open/PartiallyFilled 但不在索引中）。
        fn rollback_order_filled_amount(order_id: u64, amount: BalanceOf<T>) {
            Orders::<T>::mutate(order_id, |maybe_order| {
                if let Some(order) = maybe_order {
                    let was_filled = order.status == OrderStatus::Filled;
                    order.filled_amount = order.filled_amount.saturating_sub(amount);

                    // 🆕 H1-R2修复: Cancelled/Expired 订单仅回退 filled_amount，不改变状态
                    // 防止已取消订单被回滚为 Open/PartiallyFilled 幽灵记录
                    if order.status == OrderStatus::Cancelled
                        || order.status == OrderStatus::Expired
                    {
                        log::info!(target: "nex-market",
                            "Order {} rollback: status {:?} preserved, only filled_amount reduced",
                            order_id, order.status);
                        return;
                    }

                    if order.filled_amount < order.nex_amount {
                        if order.filled_amount.is_zero() {
                            order.status = OrderStatus::Open;
                        } else {
                            order.status = OrderStatus::PartiallyFilled;
                        }

                        // 如果从 Filled 恢复，重新加入订单簿和用户索引
                        // 🆕 H2修复: 跳过已过期订单，防止过期订单重新进入订单簿污染 BestPrice
                        if was_filled {
                            let now = <frame_system::Pallet<T>>::block_number();
                            if now > order.expires_at {
                                order.status = OrderStatus::Expired;
                                log::info!(target: "nex-market",
                                    "Order {} rollback: order expired, marking as Expired", order_id);
                                return;
                            }

                            let book_ok = match order.side {
                                OrderSide::Sell => {
                                    SellOrders::<T>::try_mutate(|orders| orders.try_push(order_id))
                                        .is_ok()
                                }
                                OrderSide::Buy => {
                                    BuyOrders::<T>::try_mutate(|orders| orders.try_push(order_id))
                                        .is_ok()
                                }
                            };

                            if !book_ok {
                                let unfilled = order.nex_amount.saturating_sub(order.filled_amount);
                                match order.side {
                                    OrderSide::Sell => {
                                        if !unfilled.is_zero() {
                                            T::Currency::unreserve(&order.maker, unfilled);
                                        }
                                    }
                                    OrderSide::Buy => {
                                        if !order.buyer_deposit.is_zero() {
                                            T::Currency::unreserve(
                                                &order.maker,
                                                order.buyer_deposit,
                                            );
                                            order.buyer_deposit = Zero::zero();
                                        }
                                    }
                                }
                                order.status = OrderStatus::Cancelled;
                                log::warn!(target: "nex-market",
                                    "Order {} rollback: order book full, cancelled and refunded maker",
                                    order_id);
                                return;
                            }

                            let user_ok = UserOrders::<T>::try_mutate(&order.maker, |orders| {
                                orders.try_push(order_id)
                            })
                            .is_ok();
                            if !user_ok {
                                Self::remove_from_order_book(order_id, order.side);
                                let unfilled = order.nex_amount.saturating_sub(order.filled_amount);
                                match order.side {
                                    OrderSide::Sell => {
                                        if !unfilled.is_zero() {
                                            T::Currency::unreserve(&order.maker, unfilled);
                                        }
                                    }
                                    OrderSide::Buy => {
                                        if !order.buyer_deposit.is_zero() {
                                            T::Currency::unreserve(
                                                &order.maker,
                                                order.buyer_deposit,
                                            );
                                            order.buyer_deposit = Zero::zero();
                                        }
                                    }
                                }
                                order.status = OrderStatus::Cancelled;
                                log::warn!(target: "nex-market",
                                    "Order {} rollback: user orders full for {:?}, cancelled and refunded maker",
                                    order_id, order.maker);
                                return;
                            }

                            Self::update_best_price_on_new_order(order.usdt_price, order.side);
                        }
                    }
                }
            });
        }

        /// 从订单簿移除
        fn remove_from_order_book(order_id: u64, side: OrderSide) {
            match side {
                OrderSide::Sell => {
                    SellOrders::<T>::mutate(|orders| {
                        orders.retain(|&id| id != order_id);
                    });
                }
                OrderSide::Buy => {
                    BuyOrders::<T>::mutate(|orders| {
                        orders.retain(|&id| id != order_id);
                    });
                }
            }
        }

        /// L3: 成功完成交易后标记买家 + 清理活跃免保证金计数
        fn finalize_waived_trade_tracking(buyer: &T::AccountId, order_id: u64) {
            if let Some(order) = Orders::<T>::get(order_id) {
                if order.deposit_waived {
                    CompletedBuyers::<T>::insert(buyer, true);
                    ActiveWaivedTrades::<T>::mutate(buyer, |count| {
                        *count = count.saturating_sub(1);
                    });
                }
            }
        }

        /// 清理活跃免保证金计数（超时/少付，不标记 CompletedBuyers）
        fn cleanup_waived_trade_counter(buyer: &T::AccountId, order_id: u64) {
            if let Some(order) = Orders::<T>::get(order_id) {
                if order.deposit_waived {
                    ActiveWaivedTrades::<T>::mutate(buyer, |count| {
                        *count = count.saturating_sub(1);
                    });
                }
            }
        }

        /// 保护性瀑布式 seed 基准价格：
        ///   成熟期（≥min_trades）: 7d TWAP（24~48h 窗口，最抗操纵）
        ///   过渡期（≥30 笔）:      max(24h TWAP, InitialPrice)（只涨不跌）
        ///   冷启动（<30 笔）:       InitialPrice（人为设定兜底）
        pub fn get_seed_reference_price() -> Option<u64> {
            let config = PriceProtectionStore::<T>::get().unwrap_or_default();
            let initial = config.initial_price?;

            if let Some(acc) = TwapAccumulatorStore::<T>::get() {
                // 成熟期：交易数充足，信任 7d TWAP
                if acc.trade_count >= config.min_trades_for_twap {
                    if let Some(twap_7d) = Self::calculate_twap(TwapPeriod::OneWeek) {
                        if twap_7d > 0 {
                            return Some(twap_7d);
                        }
                    }
                }
                // 过渡期：取 max(24h TWAP, InitialPrice)，基准只涨不跌
                const TRANSITION_TRADES: u64 = 30;
                if acc.trade_count >= TRANSITION_TRADES {
                    if let Some(twap_24h) = Self::calculate_twap(TwapPeriod::OneDay) {
                        if twap_24h > initial {
                            return Some(twap_24h);
                        }
                    }
                }
            }

            // 冷启动 / 交易不足 / 市场下跌：InitialPrice 兜底
            Some(initial)
        }

        /// 🆕 H4: 全量刷新最优价格（仅在 GC / seed_liquidity 批量操作后调用）
        fn refresh_best_prices() {
            let now = <frame_system::Pallet<T>>::block_number();

            let best_ask = SellOrders::<T>::get()
                .iter()
                .filter_map(|&id| Orders::<T>::get(id))
                .filter(|o| {
                    (o.status == OrderStatus::Open || o.status == OrderStatus::PartiallyFilled)
                        && now <= o.expires_at
                })
                .map(|o| o.usdt_price)
                .min();
            match best_ask {
                Some(p) => BestAsk::<T>::put(p),
                None => BestAsk::<T>::kill(),
            }

            let best_bid = BuyOrders::<T>::get()
                .iter()
                .filter_map(|&id| Orders::<T>::get(id))
                .filter(|o| {
                    (o.status == OrderStatus::Open || o.status == OrderStatus::PartiallyFilled)
                        && now <= o.expires_at
                })
                .map(|o| o.usdt_price)
                .max();
            match best_bid {
                Some(p) => BestBid::<T>::put(p),
                None => BestBid::<T>::kill(),
            }
        }

        /// 🆕 H4: 增量更新 — 新订单创建时 O(1) 比较
        fn update_best_price_on_new_order(price: u64, side: OrderSide) {
            match side {
                OrderSide::Sell => {
                    // 卖单取最低价
                    let should_update = BestAsk::<T>::get().map_or(true, |current| price < current);
                    if should_update {
                        BestAsk::<T>::put(price);
                    }
                }
                OrderSide::Buy => {
                    // 买单取最高价
                    let should_update = BestBid::<T>::get().map_or(true, |current| price > current);
                    if should_update {
                        BestBid::<T>::put(price);
                    }
                }
            }
        }

        /// 🆕 H4: 增量更新 — 订单移除/取消/成交时，仅在影响当前最优价时重扫
        fn update_best_price_on_remove(price: u64, side: OrderSide) {
            match side {
                OrderSide::Sell => {
                    if BestAsk::<T>::get() == Some(price) {
                        // 移除的恰好是最优卖价 → 需要重扫卖单找新最优
                        let now = <frame_system::Pallet<T>>::block_number();
                        let new_best = SellOrders::<T>::get()
                            .iter()
                            .filter_map(|&id| Orders::<T>::get(id))
                            .filter(|o| {
                                (o.status == OrderStatus::Open
                                    || o.status == OrderStatus::PartiallyFilled)
                                    && now <= o.expires_at
                            })
                            .map(|o| o.usdt_price)
                            .min();
                        match new_best {
                            Some(p) => BestAsk::<T>::put(p),
                            None => BestAsk::<T>::kill(),
                        }
                    }
                }
                OrderSide::Buy => {
                    if BestBid::<T>::get() == Some(price) {
                        let now = <frame_system::Pallet<T>>::block_number();
                        let new_best = BuyOrders::<T>::get()
                            .iter()
                            .filter_map(|&id| Orders::<T>::get(id))
                            .filter(|o| {
                                (o.status == OrderStatus::Open
                                    || o.status == OrderStatus::PartiallyFilled)
                                    && now <= o.expires_at
                            })
                            .map(|o| o.usdt_price)
                            .max();
                        match new_best {
                            Some(p) => BestBid::<T>::put(p),
                            None => BestBid::<T>::kill(),
                        }
                    }
                }
            }
        }

        /// 价格偏离检查
        pub fn check_price_deviation(usdt_price: u64) -> Result<(), Error<T>> {
            let config = PriceProtectionStore::<T>::get().unwrap_or_default();
            if !config.enabled {
                return Ok(());
            }

            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
            if config.circuit_breaker_active && current_block < config.circuit_breaker_until {
                return Err(Error::<T>::MarketCircuitBreakerActive);
            }

            let reference_price: Option<u64> = {
                let acc = TwapAccumulatorStore::<T>::get();
                match acc {
                    Some(ref a) if Self::is_twap_data_sufficient(a, current_block, &config) => {
                        Self::calculate_twap(TwapPeriod::OneHour)
                    }
                    _ => config.initial_price,
                }
            };

            let ref_price = match reference_price {
                Some(p) if p > 0 => p,
                _ => return Ok(()),
            };

            // 🆕 M4: 使用 saturating cast 防止 u16 截断（极端偏离 >655% 时不会回绕为小值）
            let deviation_raw = if usdt_price > ref_price {
                (usdt_price as u128 - ref_price as u128) * 10000 / ref_price as u128
            } else {
                (ref_price as u128 - usdt_price as u128) * 10000 / ref_price as u128
            };
            let deviation_bps = deviation_raw.min(u16::MAX as u128) as u16;

            if deviation_bps > config.max_price_deviation {
                return Err(Error::<T>::PriceDeviationTooHigh);
            }

            Ok(())
        }

        // ==================== TWAP ====================

        /// 更新 TWAP 累积器，返回异常过滤后的价格
        fn update_twap_accumulator(trade_price: u64) -> u64 {
            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();

            TwapAccumulatorStore::<T>::mutate(|maybe_acc| {
                let acc = maybe_acc
                    .get_or_insert_with(|| TwapAccumulator::new_at(current_block, trade_price, 0));

                // 异常价格过滤
                let filtered_price = if acc.trade_count > 0 && acc.last_price > 0 {
                    let deviation = if trade_price > acc.last_price {
                        trade_price - acc.last_price
                    } else {
                        acc.last_price - trade_price
                    };
                    if deviation > acc.last_price {
                        if trade_price > acc.last_price {
                            acc.last_price.saturating_mul(3) / 2
                        } else {
                            acc.last_price / 2
                        }
                    } else {
                        trade_price
                    }
                } else {
                    trade_price
                };

                let blocks_elapsed = current_block.saturating_sub(acc.current_block);
                if blocks_elapsed > 0 {
                    acc.current_cumulative = acc.current_cumulative.saturating_add(
                        (acc.last_price as u128).saturating_mul(blocks_elapsed as u128),
                    );
                }

                acc.current_block = current_block;
                acc.last_price = filtered_price;
                acc.trade_count = acc.trade_count.saturating_add(1);

                // Record the block of the first real trade (history coverage
                // anchor). `max(1)` keeps the "0 = no trades yet" sentinel
                // meaningful even for block-0 trades (test environments).
                // 记录首笔真实成交的区块（价格历史覆盖时长的起点）。`max(1)`
                // 保证区块 0 的成交（测试环境）不与"0 = 尚无成交"哨兵值冲突。
                if acc.first_trade_block == 0 {
                    acc.first_trade_block = current_block.max(1);
                }

                Self::advance_snapshots(acc, current_block);

                filtered_price
            })
        }

        /// Rotate per-period checkpoints (shared by trades and `on_idle`): once
        /// `curr` is at least one full period old, it becomes `prev` and a fresh
        /// `curr` is taken. This keeps `prev` between 1x and 2x the period old,
        /// so each TWAP window actually covers its named period.
        ///
        /// 轮换各周期 checkpoint（成交与 `on_idle` 共用）：当 `curr` 距今超过
        /// 一个完整周期时转为 `prev` 并取新的 `curr`。`prev` 的年龄始终保持在
        /// 1~2 个周期之间，确保 TWAP 窗口真实覆盖其命名周期。
        fn advance_snapshots(acc: &mut TwapAccumulator, current_block: u32) {
            let new_snapshot = PriceSnapshot {
                cumulative_price: acc.current_cumulative,
                block_number: current_block,
            };
            let rotations: [(&mut PriceSnapshot, &mut PriceSnapshot, u32); 3] = [
                (
                    &mut acc.hour_prev,
                    &mut acc.hour_curr,
                    T::BlocksPerHour::get(),
                ),
                (&mut acc.day_prev, &mut acc.day_curr, T::BlocksPerDay::get()),
                (
                    &mut acc.week_prev,
                    &mut acc.week_curr,
                    T::BlocksPerWeek::get(),
                ),
            ];
            for (prev, curr, period) in rotations {
                if current_block.saturating_sub(curr.block_number) >= period {
                    *prev = curr.clone();
                    *curr = new_snapshot.clone();
                }
            }
        }

        /// Calculate the TWAP for the given period. Picks the checkpoint closest
        /// to (but at least) one period old when one exists; otherwise falls back
        /// to the oldest available checkpoint (early market life, shorter window).
        ///
        /// 计算指定周期的 TWAP。优先选取"距今至少一个周期、且最接近一个周期"
        /// 的 checkpoint；市场早期两者都不足一个周期时退化为最旧 checkpoint
        /// （窗口较短的尽力估计）。
        pub fn calculate_twap(period: TwapPeriod) -> Option<u64> {
            let acc = TwapAccumulatorStore::<T>::get()?;
            let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();

            let (prev, curr, period_blocks) = match period {
                TwapPeriod::OneHour => (&acc.hour_prev, &acc.hour_curr, T::BlocksPerHour::get()),
                TwapPeriod::OneDay => (&acc.day_prev, &acc.day_curr, T::BlocksPerDay::get()),
                TwapPeriod::OneWeek => (&acc.week_prev, &acc.week_curr, T::BlocksPerWeek::get()),
            };

            // `curr` 始终不早于 `prev`。若 `curr` 已满一个周期，它的窗口最接近
            // 命名周期；否则使用 `prev`（活跃市场中为 1~2 个周期）。
            let snapshot = if current_block.saturating_sub(curr.block_number) >= period_blocks {
                curr
            } else {
                prev
            };

            let blocks_since = current_block.saturating_sub(acc.current_block);
            let current_cumulative = acc
                .current_cumulative
                .saturating_add((acc.last_price as u128).saturating_mul(blocks_since as u128));

            let block_diff = current_block.saturating_sub(snapshot.block_number);
            if block_diff == 0 {
                return Some(acc.last_price);
            }

            let cumulative_diff = current_cumulative.saturating_sub(snapshot.cumulative_price);
            u64::try_from(cumulative_diff / (block_diff as u128)).ok()
        }

        /// Check whether TWAP data is sufficient to serve as the reference price:
        /// enough trades AND at least one hour of real trading history (measured
        /// from `first_trade_block`, NOT from checkpoint age — checkpoints rotate
        /// over time, so their age would never satisfy a "stale enough" check and
        /// the reference price could never switch away from `initial_price`).
        ///
        /// 检查 TWAP 数据是否充足，可作为参考价格使用：成交数达标且已积累至少
        /// 1 小时的真实成交历史（从 `first_trade_block` 起算，而非 checkpoint
        /// 年龄——checkpoint 随时间轮换刷新，按其年龄判断会导致参考价永远无法
        /// 从 `initial_price` 切换到 TWAP）。
        fn is_twap_data_sufficient(
            acc: &TwapAccumulator,
            current_block: u32,
            config: &PriceProtectionConfig,
        ) -> bool {
            if acc.trade_count < config.min_trades_for_twap {
                return false;
            }
            // first_trade_block == 0 表示尚无真实成交
            if acc.first_trade_block == 0 {
                return false;
            }
            current_block.saturating_sub(acc.first_trade_block) >= T::BlocksPerHour::get()
        }

        fn check_circuit_breaker(current_price: u64) {
            let config = match PriceProtectionStore::<T>::get() {
                Some(c) if c.enabled => c,
                _ => return,
            };

            let twap_7d = match Self::calculate_twap(TwapPeriod::OneWeek) {
                Some(t) if t > 0 => t,
                _ => return,
            };

            // 🆕 M4: saturating cast 防止 u16 截断
            let deviation_raw = if current_price > twap_7d {
                (current_price as u128 - twap_7d as u128) * 10000 / twap_7d as u128
            } else {
                (twap_7d as u128 - current_price as u128) * 10000 / twap_7d as u128
            };
            let deviation_bps = deviation_raw.min(u16::MAX as u128) as u16;

            if deviation_bps > config.circuit_breaker_threshold {
                let current_block: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
                let until_block = current_block.saturating_add(T::CircuitBreakerDuration::get());

                PriceProtectionStore::<T>::mutate(|maybe| {
                    if let Some(c) = maybe {
                        c.circuit_breaker_active = true;
                        c.circuit_breaker_until = until_block;
                    }
                });

                Self::deposit_event(Event::CircuitBreakerTriggered {
                    current_price,
                    twap_7d,
                    deviation_bps,
                    until_block,
                });
            }
        }

        fn on_trade_completed(trade_price: u64) {
            let filtered_price = Self::update_twap_accumulator(trade_price);
            LastTradePrice::<T>::put(filtered_price);

            let twap_1h = Self::calculate_twap(TwapPeriod::OneHour);
            let twap_24h = Self::calculate_twap(TwapPeriod::OneDay);
            let twap_7d = Self::calculate_twap(TwapPeriod::OneWeek);
            Self::deposit_event(Event::TwapUpdated {
                new_price: trade_price,
                twap_1h,
                twap_24h,
                twap_7d,
            });

            Self::check_circuit_breaker(trade_price);
        }

        /// 根据交易创建区块估算创建时间戳(ms)，并额外向前留 30 分钟余量
        fn estimate_trade_created_ms(trade_created_at: BlockNumberFor<T>) -> u64 {
            let now_ms = sp_io::offchain::timestamp().unix_millis();
            let current_block: u64 = <frame_system::Pallet<T>>::block_number().saturated_into();
            let created_block: u64 = trade_created_at.saturated_into();
            let blocks_since = current_block.saturating_sub(created_block);
            // P1-7: 从 BlocksPerHour 推导毫秒/块，避免硬编码出块时间
            let bph = T::BlocksPerHour::get() as u64;
            let ms_per_block = if bph > 0 {
                3_600_000u64 / bph
            } else {
                6_000u64
            };
            let elapsed_ms = blocks_since.saturating_mul(ms_per_block);
            let estimated_created_ms = now_ms.saturating_sub(elapsed_ms);
            // 向前多留 30 分钟余量（应对出块时间波动）
            estimated_created_ms.saturating_sub(30 * 60 * 1000)
        }

        /// 清理指定 Indexer 的所有 pending hints（遍历 IndexerHints 存储）
        ///
        /// 返回清理的 hint 数量。force_remove_indexer 使用此函数避免孤儿 hint。
        fn cleanup_indexer_hints(indexer: &T::AccountId) -> u32 {
            let mut cleaned: u32 = 0;
            let mut to_remove: Vec<u64> = Vec::new();

            for (trade_id, hint) in IndexerHints::<T>::iter() {
                if &hint.indexer == indexer {
                    to_remove.push(trade_id);
                }
            }

            for trade_id in to_remove {
                IndexerHints::<T>::remove(trade_id);
                cleaned += 1;
            }

            cleaned
        }

        /// P9: 清理单笔交易的 Indexer hint（交易终态时调用）
        ///
        /// 如果 hint 仍为 Pending 状态，递减对应 Indexer 的 pending_hint_count。
        fn cleanup_trade_hint(trade_id: u64) {
            if let Some(hint) = IndexerHints::<T>::take(trade_id) {
                if hint.status == IndexerHintStatus::Pending {
                    if let Some(mut info) = IndexerSet::<T>::get(&hint.indexer) {
                        info.pending_hint_count = info.pending_hint_count.saturating_sub(1);
                        IndexerSet::<T>::insert(&hint.indexer, info);
                    }
                }
            }
        }

        /// Slash Indexer 质押：从 reserved 中扣除 IndexerSlashRateBps 比例转入国库
        ///
        /// 更新 info.stake 反映 slash 后的余额。返回实际 slash 金额。
        fn slash_indexer_stake(indexer: &T::AccountId, info: &mut IndexerInfo<T>) -> BalanceOf<T> {
            let slash_bps = T::IndexerSlashRateBps::get();
            if slash_bps == 0 || info.stake.is_zero() {
                return Zero::zero();
            }

            let stake_u128: u128 = info.stake.saturated_into();
            let slash_u128 = stake_u128
                .saturating_mul(slash_bps as u128)
                .saturating_div(10000);
            let slash_amount: BalanceOf<T> = slash_u128.saturated_into();

            if slash_amount.is_zero() {
                return Zero::zero();
            }

            // 先 unreserve slash 部分，再转给国库
            T::Currency::unreserve(indexer, slash_amount);
            let treasury = T::TreasuryAccount::get();
            let actually_slashed = match T::Currency::transfer(
                indexer,
                &treasury,
                slash_amount,
                ExistenceRequirement::KeepAlive,
            ) {
                Ok(_) => slash_amount,
                Err(_) => {
                    // 转账失败时回滚 reserve（不应发生，但防御性处理）
                    let _ = T::Currency::reserve(indexer, slash_amount);
                    Zero::zero()
                }
            };

            info.stake = info.stake.saturating_sub(actually_slashed);
            actually_slashed
        }

        /// Indexer hint 奖励：交易成功完成后，标记 hint 为 Verified 并发放奖励，然后清理 hint 存储
        fn try_reward_indexer_hint(trade_id: u64) {
            if let Some(mut hint) = IndexerHints::<T>::get(trade_id) {
                if hint.status == IndexerHintStatus::Pending {
                    hint.status = IndexerHintStatus::Verified;

                    // 更新 Indexer 统计
                    if let Some(mut info) = IndexerSet::<T>::get(&hint.indexer) {
                        info.accelerated_count = info.accelerated_count.saturating_add(1);
                        info.pending_hint_count = info.pending_hint_count.saturating_sub(1);
                        IndexerSet::<T>::insert(&hint.indexer, info);
                    }

                    // 从 Indexer 奖池发放奖励（奖池由 trading_fee 分成自动补充）
                    let reward = T::IndexerHintReward::get();
                    let mut reward_paid = false;
                    if !reward.is_zero() {
                        let reward_pool = T::RewardSource::get();
                        match T::Currency::transfer(
                            &reward_pool,
                            &hint.indexer,
                            reward,
                            ExistenceRequirement::KeepAlive,
                        ) {
                            Ok(_) => {
                                reward_paid = true;
                            }
                            Err(_) => {
                                // 奖池余额不足 — 发告警事件，不阻断交易结算
                                Self::deposit_event(Event::IndexerRewardPoolDepleted {
                                    trade_id,
                                    indexer: hint.indexer.clone(),
                                    reward_missed: reward,
                                });
                            }
                        }
                    }

                    Self::deposit_event(Event::IndexerHintVerified {
                        trade_id,
                        indexer: hint.indexer.clone(),
                        reward: if reward_paid { reward } else { Zero::zero() },
                    });
                }
            }

            // P9: 交易终态清理 hint 存储（无论 Pending/Verified/Mismatch）
            IndexerHints::<T>::remove(trade_id);
        }

        /// OCW: 交叉验证 Indexer 提交的 hint
        ///
        /// 1. 调用 trc20_verifier::verify_trc20_by_txhash 单笔交叉验证
        /// 2. 匹配成功 → 构造 submit_ocw_result / auto_confirm_payment unsigned tx
        /// 3. 不匹配 → 构造 report_indexer_mismatch unsigned tx
        /// 4. 网络失败 → 不惩罚，下个区块重试
        fn cross_verify_hint(trade_id: u64, trade: &UsdtTrade<T>, hint: &IndexerHint<T>) {
            log::info!(target: "nex-market-ocw",
                "Cross-verifying hint for trade {} (tx_hash len={}, reported_amount={})",
                trade_id, hint.tx_hash.len(), hint.reported_amount);

            let result = pallet_trading_trc20_verifier::verify_trc20_by_txhash(
                hint.tx_hash.as_slice(),
                trade.seller_tron_address.as_slice(),
                trade.buyer_tron_address.as_ref().map(|t| t.as_slice()),
            );

            match result {
                Ok(tx_result) => {
                    if tx_result.found
                        && tx_result.to_match
                        && tx_result.contract_match
                        && tx_result.from_match
                    {
                        // 验证成功 — 使用 hint 的 tx_hash 和验证到的金额
                        let actual_amount = tx_result.amount;
                        let bounded_hashes: TxHashVec = alloc::vec![hint.tx_hash.clone()]
                            .try_into()
                            .unwrap_or_default();

                        if bounded_hashes.is_empty() {
                            return;
                        }

                        // 根据 trade 状态选择正确的 extrinsic
                        let call = if trade.status == UsdtTradeStatus::AwaitingVerification {
                            Call::<T>::submit_ocw_result {
                                trade_id,
                                actual_amount,
                                tx_hashes: bounded_hashes.clone(),
                            }
                        } else {
                            // AwaitingPayment → auto_confirm_payment
                            Call::<T>::auto_confirm_payment {
                                trade_id,
                                actual_amount,
                                tx_hashes: bounded_hashes.clone(),
                            }
                        };

                        let xt = T::create_bare(call.into());
                        match frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
                            Ok(_) => {
                                log::info!(target: "nex-market-ocw",
                                    "Hint-verified trade {} submitted: amount={}", trade_id, actual_amount);
                                // M-1 审计修复: 同步注册 tx_hash 到 OCW 本地存储
                                for hash in bounded_hashes.iter() {
                                    pallet_trading_trc20_verifier::register_used_tx_hash(&hash);
                                }
                            }
                            Err(e) => {
                                log::error!(target: "nex-market-ocw",
                                    "Hint-verified trade {} submit failed: {:?}", trade_id, e);
                            }
                        }
                    } else {
                        // 不匹配 → 报告 mismatch（移除 hint，释放槽位）
                        log::warn!(target: "nex-market-ocw",
                            "Hint mismatch for trade {}: found={}, to={}, contract={}",
                            trade_id, tx_result.found, tx_result.to_match, tx_result.contract_match);

                        let call = Call::<T>::report_indexer_mismatch { trade_id };
                        let xt = T::create_bare(call.into());
                        match frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
                            Ok(_) => {
                                log::info!(target: "nex-market-ocw",
                                    "Mismatch reported for trade {}", trade_id);
                            }
                            Err(e) => {
                                log::error!(target: "nex-market-ocw",
                                    "Mismatch report failed for trade {}: {:?}", trade_id, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // 网络失败 → 不惩罚，下个区块重试
                    log::warn!(target: "nex-market-ocw",
                        "Hint cross-verify HTTP error for trade {}: {:?}", trade_id, e);
                }
            }
        }

        /// OCW: 处理验证（按 from/to/amount 匹配 TRON 链上转账）
        ///
        /// 调用 pallet-trading-trc20-verifier 查询 TronGrid API，
        /// 搜索 buyer→seller 的 USDT TRC20 转账记录，
        /// 直接构造 bare unsigned extrinsic 提交到本地交易池。
        fn process_verification(
            trade_id: u64,
            trade: &UsdtTrade<T>,
            buyer_tron: &[u8],
            seller_tron: &[u8],
        ) {
            log::info!(target: "nex-market-ocw",
                "Verifying trade {} by (from={} bytes, to={} bytes, amount={})",
                trade_id, buyer_tron.len(), seller_tron.len(), trade.usdt_amount);

            // 搜索起始时间：锚定交易创建区块，向前留余量
            let min_timestamp = Self::estimate_trade_created_ms(trade.created_at);

            let (actual_amount, all_tx_hashes) =
                match pallet_trading_trc20_verifier::verify_trc20_by_transfer(
                    buyer_tron,
                    seller_tron,
                    trade.usdt_amount,
                    min_timestamp,
                    &trade_id.to_le_bytes(),
                ) {
                    Ok(result) => {
                        if result.found {
                            log::info!(target: "nex-market-ocw",
                            "Trade {} verified: actual_amount={:?}, status={:?}",
                            trade_id, result.actual_amount, result.amount_status);
                            // B1: 收集所有匹配转账的 tx_hash
                            let hashes: Vec<Vec<u8>> = result
                                .matched_transfers
                                .iter()
                                .filter(|t| !t.tx_hash.is_empty())
                                .map(|t| t.tx_hash.clone())
                                .collect();
                            (result.actual_amount.unwrap_or(0), hashes)
                        } else {
                            log::warn!(target: "nex-market-ocw",
                            "Trade {} verification: no matching transfer found (error={:?})",
                            trade_id, result.error);
                            (0, Vec::new())
                        }
                    }
                    Err(e) => {
                        log::error!(target: "nex-market-ocw",
                        "Trade {} verification HTTP error: {}", trade_id, e);
                        // HTTP 失败 — 等待下一个区块重试
                        return;
                    }
                };

            // 跳过无结果的情况（未找到转账或金额为 0 且无 hash）
            if actual_amount == 0 && all_tx_hashes.is_empty() {
                return;
            }

            // B1: 构造 BoundedVec<TxHash>
            let bounded_hashes: TxHashVec = all_tx_hashes
                .into_iter()
                .filter_map(|h| {
                    let bounded_hash: Result<TxHash, _> = h.try_into();
                    bounded_hash.ok()
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap_or_default();

            if bounded_hashes.is_empty() {
                log::warn!(target: "nex-market-ocw",
                    "Trade {} verification: no valid tx_hashes to submit", trade_id);
                return;
            }

            // 直接构造 bare unsigned extrinsic 提交到本地交易池
            let call = Call::<T>::submit_ocw_result {
                trade_id,
                actual_amount,
                tx_hashes: bounded_hashes.clone(),
            };
            let xt = T::create_bare(call.into());
            match frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
                Ok(_) => {
                    log::info!(target: "nex-market-ocw",
                        "Trade {} result submitted: actual_amount={}", trade_id, actual_amount);
                    // M-1 审计修复: 同步注册 tx_hash 到 OCW 本地存储，减少多验证节点重复提交
                    for hash in bounded_hashes.iter() {
                        pallet_trading_trc20_verifier::register_used_tx_hash(&hash);
                    }
                }
                Err(e) => {
                    log::error!(target: "nex-market-ocw",
                        "Trade {} failed to submit tx: {:?}", trade_id, e);
                }
            }
        }

        // ========================= OCW 防重复调用辅助函数 =========================

        /// 检查某笔交易是否在最近 `cooldown` 个区块内已被 OCW 检查过。
        /// 避免每个区块都调用外部 API（TronGrid），节省配额。
        fn ocw_recently_checked(
            prefix: &[u8],
            trade_id: u64,
            current_block: BlockNumberFor<T>,
            cooldown: BlockNumberFor<T>,
        ) -> bool {
            let mut key = prefix.to_vec();
            key.extend_from_slice(b"::");
            key.extend_from_slice(&trade_id.to_le_bytes());
            if let Some(bytes) =
                sp_io::offchain::local_storage_get(sp_core::offchain::StorageKind::PERSISTENT, &key)
            {
                if let Ok(last_block) =
                    <BlockNumberFor<T> as codec::Decode>::decode(&mut &bytes[..])
                {
                    return current_block.saturating_sub(last_block) < cooldown;
                }
            }
            false
        }

        /// 记录某笔交易被 OCW 检查的区块号。
        fn ocw_mark_checked(prefix: &[u8], trade_id: u64, current_block: BlockNumberFor<T>) {
            let mut key = prefix.to_vec();
            key.extend_from_slice(b"::");
            key.extend_from_slice(&trade_id.to_le_bytes());
            sp_io::offchain::local_storage_set(
                sp_core::offchain::StorageKind::PERSISTENT,
                &key,
                &codec::Encode::encode(&current_block),
            );
        }

        /// OCW: 预检 AwaitingPayment 交易是否已有 USDT 到账
        ///
        /// 当买家忘记 confirm_payment 但 USDT 已到时，
        /// 直接构造 bare unsigned extrinsic 提交 auto_confirm_payment。
        fn auto_check_awaiting_payment(
            trade_id: u64,
            trade: &UsdtTrade<T>,
            buyer_tron: &[u8],
            seller_tron: &[u8],
        ) {
            log::info!(target: "nex-market-ocw",
                "Auto-checking AwaitingPayment trade {} (amount={})",
                trade_id, trade.usdt_amount);

            let min_timestamp = Self::estimate_trade_created_ms(trade.created_at);

            match pallet_trading_trc20_verifier::verify_trc20_by_transfer(
                buyer_tron,
                seller_tron,
                trade.usdt_amount,
                min_timestamp,
                &trade_id.to_le_bytes(),
            ) {
                Ok(result) if result.found => {
                    let actual_amount = result.actual_amount.unwrap_or(0);
                    if actual_amount > 0 {
                        log::info!(target: "nex-market-ocw",
                            "Auto-detected payment for trade {}: actual_amount={}",
                            trade_id, actual_amount);

                        // B1: 构造 BoundedVec<TxHash>
                        let bounded_hashes: TxHashVec = result
                            .matched_transfers
                            .iter()
                            .filter(|t| !t.tx_hash.is_empty())
                            .filter_map(|t| {
                                let bounded_hash: Result<TxHash, _> = t.tx_hash.clone().try_into();
                                bounded_hash.ok()
                            })
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap_or_default();

                        if bounded_hashes.is_empty() {
                            return;
                        }

                        let call = Call::<T>::auto_confirm_payment {
                            trade_id,
                            actual_amount,
                            tx_hashes: bounded_hashes.clone(),
                        };
                        let xt = T::create_bare(call.into());
                        match frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
                            Ok(_) => {
                                log::info!(target: "nex-market-ocw",
                                    "Auto-confirm tx submitted for trade {}", trade_id);
                                // M-1 审计修复: 同步注册 tx_hash 到 OCW 本地存储
                                for hash in bounded_hashes.iter() {
                                    pallet_trading_trc20_verifier::register_used_tx_hash(&hash);
                                }
                            }
                            Err(e) => {
                                log::error!(target: "nex-market-ocw",
                                    "Auto-confirm tx failed for trade {}: {:?}", trade_id, e);
                            }
                        }
                    }
                }
                Ok(_) => {
                    // 未找到转账 — 正常情况（买家可能真没付）
                }
                Err(e) => {
                    log::warn!(target: "nex-market-ocw",
                        "Auto-check trade {} HTTP error: {}", trade_id, e);
                }
            }
        }

        /// OCW: 补付窗口内持续检查 UnderpaidPending 交易
        ///
        /// 重新查询 TronGrid 累计金额，若金额增加则直接提交 submit_underpaid_update。
        fn check_underpaid_topup(
            trade_id: u64,
            trade: &UsdtTrade<T>,
            buyer_tron: &[u8],
            seller_tron: &[u8],
        ) {
            log::info!(target: "nex-market-ocw",
                "Checking underpaid topup for trade {} (expected={})",
                trade_id, trade.usdt_amount);

            // 使用 first_verified_at（如果有的话），否则 created_at
            let anchor_block = trade.first_verified_at.unwrap_or(trade.created_at);
            let min_timestamp = Self::estimate_trade_created_ms(anchor_block);

            match pallet_trading_trc20_verifier::verify_trc20_by_transfer(
                buyer_tron,
                seller_tron,
                trade.usdt_amount,
                min_timestamp,
                &trade_id.to_le_bytes(),
            ) {
                Ok(result) if result.found => {
                    let actual_amount = result.actual_amount.unwrap_or(0);
                    if actual_amount > 0 {
                        // B2+P1-8: 构造 BoundedVec<TxHash>，过滤掉已在链上注册的 hash
                        let bounded_hashes: TxHashVec = result
                            .matched_transfers
                            .iter()
                            .filter(|t| !t.tx_hash.is_empty())
                            .filter(|t| {
                                let bounded: Result<TxHash, _> = t.tx_hash.clone().try_into();
                                bounded
                                    .as_ref()
                                    .map_or(false, |h| !UsedTxHashes::<T>::contains_key(h))
                            })
                            .filter_map(|t| {
                                let bounded_hash: Result<TxHash, _> = t.tx_hash.clone().try_into();
                                bounded_hash.ok()
                            })
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap_or_default();

                        // P1-8: 没有新 tx_hash 则跳过（validate_unsigned 会拒绝空 hash）
                        if bounded_hashes.is_empty() {
                            log::debug!(target: "nex-market-ocw",
                                "Underpaid trade {}: no new tx_hashes to submit", trade_id);
                            return;
                        }

                        let call = Call::<T>::submit_underpaid_update {
                            trade_id,
                            new_actual_amount: actual_amount,
                            new_tx_hashes: bounded_hashes.clone(),
                        };
                        let xt = T::create_bare(call.into());
                        match frame_system::offchain::SubmitTransaction::<T, Call<T>>::submit_transaction(xt) {
                            Ok(_) => {
                                log::info!(target: "nex-market-ocw",
                                    "Underpaid trade {} update submitted: amount={}", trade_id, actual_amount);
                                // M-1 审计修复: 同步注册 tx_hash 到 OCW 本地存储
                                for hash in bounded_hashes.iter() {
                                    pallet_trading_trc20_verifier::register_used_tx_hash(&hash);
                                }
                            }
                            Err(e) => {
                                log::error!(target: "nex-market-ocw",
                                    "Underpaid trade {} update tx failed: {:?}", trade_id, e);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log::warn!(target: "nex-market-ocw",
                        "Underpaid check trade {} HTTP error: {}", trade_id, e);
                }
            }
        }

        /// 检查市场是否暂停
        fn ensure_market_active() -> DispatchResult {
            ensure!(!MarketPausedStore::<T>::get(), Error::<T>::MarketIsPaused);
            Ok(())
        }

        /// A1: 检查用户是否被封禁
        fn ensure_not_banned(who: &T::AccountId) -> DispatchResult {
            ensure!(!BannedAccounts::<T>::get(who), Error::<T>::UserIsBanned);
            Ok(())
        }

        /// A2: force_settle 内部逻辑（批量复用）
        fn do_force_settle(
            trade_id: u64,
            actual_amount: u64,
            resolution: DisputeResolution,
        ) -> DispatchResult {
            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingVerification
                    || trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::InvalidTradeStatus
            );

            match resolution {
                DisputeResolution::ReleaseToBuyer => {
                    // M-3 审计修复: 根据 actual_amount 决定全额/部分结算
                    let verification_result = Self::calculate_payment_verification_result(
                        trade.usdt_amount,
                        actual_amount,
                    );
                    match verification_result {
                        PaymentVerificationResult::Exact | PaymentVerificationResult::Overpaid => {
                            Self::process_full_payment(&mut trade, trade_id)?;
                        }
                        _ => {
                            // 部分付款或无效 → 按比例释放 NEX
                            Self::process_underpaid(&mut trade, trade_id, actual_amount)?;
                        }
                    }
                }
                DisputeResolution::RefundToSeller => {
                    T::Currency::unreserve(&trade.seller, trade.nex_amount);
                    Self::rollback_order_filled_amount(trade.order_id, trade.nex_amount);
                    Self::forfeit_buyer_deposit(&mut trade, trade_id);
                    trade.status = UsdtTradeStatus::Refunded;
                    trade.completed_at = Some(<frame_system::Pallet<T>>::block_number()); // W5
                    UsdtTrades::<T>::insert(trade_id, &trade);
                    Self::cleanup_waived_trade_counter(&trade.buyer, trade.order_id);
                    // 信用系统：买家违约扣分 + 减少活跃计数
                    Self::credit_on_timeout_violation(&trade.buyer);
                    ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| *c = c.saturating_sub(1));
                    // P9: 清理 Indexer hint 存储
                    Self::cleanup_trade_hint(trade_id);
                }
            }

            PendingUsdtTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            PendingUnderpaidTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            AwaitingPaymentTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);

            Self::deposit_event(Event::TradeForceSettled {
                trade_id,
                actual_amount,
                resolution,
            });
            Ok(())
        }

        /// A2: force_cancel 内部逻辑（批量复用）
        fn do_force_cancel(trade_id: u64) -> DispatchResult {
            let mut trade = UsdtTrades::<T>::get(trade_id).ok_or(Error::<T>::UsdtTradeNotFound)?;
            ensure!(
                trade.status == UsdtTradeStatus::AwaitingPayment
                    || trade.status == UsdtTradeStatus::AwaitingVerification
                    || trade.status == UsdtTradeStatus::UnderpaidPending,
                Error::<T>::InvalidTradeStatus
            );

            T::Currency::unreserve(&trade.seller, trade.nex_amount);
            Self::rollback_order_filled_amount(trade.order_id, trade.nex_amount);

            if !trade.buyer_deposit.is_zero() && trade.deposit_status == BuyerDepositStatus::Locked
            {
                T::Currency::unreserve(&trade.buyer, trade.buyer_deposit);
                trade.deposit_status = BuyerDepositStatus::Released;
                Self::deposit_event(Event::BuyerDepositReleased {
                    trade_id,
                    buyer: trade.buyer.clone(),
                    deposit: trade.buyer_deposit,
                });
            }

            trade.status = UsdtTradeStatus::Refunded;
            trade.completed_at = Some(<frame_system::Pallet<T>>::block_number()); // W5
            UsdtTrades::<T>::insert(trade_id, &trade);
            Self::cleanup_waived_trade_counter(&trade.buyer, trade.order_id);

            // 信用系统：admin 取消不扣分，只减少活跃计数
            ActiveBuyerTrades::<T>::mutate(&trade.buyer, |c| *c = c.saturating_sub(1));

            AwaitingPaymentTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            PendingUsdtTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            PendingUnderpaidTrades::<T>::mutate(|p| {
                p.retain(|&id| id != trade_id);
            });
            OcwVerificationResults::<T>::remove(trade_id);
            // P9: 清理 Indexer hint 存储
            Self::cleanup_trade_hint(trade_id);

            Self::deposit_event(Event::TradeForceCancelled { trade_id });
            Ok(())
        }

        /// #10: 检查队列容量并在超阈值时自动暂停市场
        fn check_queue_overflow_and_pause() {
            let pending_count = PendingUsdtTrades::<T>::get().len() as u32;
            let max_capacity = T::MaxPendingTrades::get();
            let threshold_bps = T::QueueFullThresholdBps::get();

            // pending_count / max_capacity > threshold_bps / 10000
            if max_capacity > 0
                && pending_count.saturating_mul(10000)
                    > max_capacity.saturating_mul(threshold_bps as u32)
            {
                if !MarketPausedStore::<T>::get() {
                    MarketPausedStore::<T>::put(true);
                    Self::deposit_event(Event::QueueOverflowPaused {
                        pending_count,
                        max_capacity,
                    });
                }
            }
        }
    }
}

// ==================== PriceOracle 实现 ====================

impl<T: pallet::Config> pallet_trading_common::PriceOracle for pallet::Pallet<T> {
    fn get_twap(window: pallet_trading_common::TwapWindow) -> Option<u64> {
        use pallet_trading_common::TwapWindow;
        let period = match window {
            TwapWindow::OneHour => pallet::TwapPeriod::OneHour,
            TwapWindow::OneDay => pallet::TwapPeriod::OneDay,
            TwapWindow::OneWeek => pallet::TwapPeriod::OneWeek,
        };
        Self::calculate_twap(period)
    }

    fn get_last_trade_price() -> Option<u64> {
        pallet::LastTradePrice::<T>::get()
    }

    fn is_price_stale(max_age_blocks: u32) -> bool {
        use sp_runtime::SaturatedConversion;
        match pallet::TwapAccumulatorStore::<T>::get() {
            Some(acc) => {
                let now: u32 = <frame_system::Pallet<T>>::block_number().saturated_into();
                now.saturating_sub(acc.current_block) > max_age_blocks
            }
            // 无 accumulator 但有 LastTradePrice，属于冷启动期，不算 stale
            None => pallet::LastTradePrice::<T>::get().is_none(),
        }
    }

    fn get_trade_count() -> u64 {
        pallet::TwapAccumulatorStore::<T>::get()
            .map(|acc| acc.trade_count)
            .unwrap_or(0)
    }
}

// ==================== 公共查询接口 ====================

use sp_runtime::traits::Saturating;
use sp_runtime::SaturatedConversion as SatConv2;

impl<T: Config> Pallet<T> {
    /// 获取卖单列表（过滤已过期订单，与撮合逻辑一致）
    pub fn get_sell_order_list() -> Vec<Order<T>> {
        let now = <frame_system::Pallet<T>>::block_number();
        SellOrders::<T>::get()
            .iter()
            .filter_map(|&id| Orders::<T>::get(id))
            .filter(|o| {
                (o.status == OrderStatus::Open || o.status == OrderStatus::PartiallyFilled)
                    && now <= o.expires_at
            })
            .collect()
    }

    /// 获取买单列表（过滤已过期订单，与撮合逻辑一致）
    pub fn get_buy_order_list() -> Vec<Order<T>> {
        let now = <frame_system::Pallet<T>>::block_number();
        BuyOrders::<T>::get()
            .iter()
            .filter_map(|&id| Orders::<T>::get(id))
            .filter(|o| {
                (o.status == OrderStatus::Open || o.status == OrderStatus::PartiallyFilled)
                    && now <= o.expires_at
            })
            .collect()
    }

    /// 获取用户订单
    pub fn get_user_order_list(user: &T::AccountId) -> Vec<Order<T>> {
        UserOrders::<T>::get(user)
            .iter()
            .filter_map(|&id| Orders::<T>::get(id))
            .collect()
    }

    /// 获取最优价格
    pub fn get_best_prices() -> (Option<u64>, Option<u64>) {
        (BestAsk::<T>::get(), BestBid::<T>::get())
    }

    /// #2/#12: 获取用户交易列表
    pub fn get_user_trade_list(user: &T::AccountId) -> Vec<UsdtTrade<T>> {
        UserTrades::<T>::get(user)
            .iter()
            .filter_map(|&id| UsdtTrades::<T>::get(id))
            .collect()
    }

    /// #5: 获取订单关联的交易列表
    pub fn get_trades_by_order(order_id: u64) -> Vec<UsdtTrade<T>> {
        OrderTrades::<T>::get(order_id)
            .iter()
            .filter_map(|&id| UsdtTrades::<T>::get(id))
            .collect()
    }

    /// #12: 获取订单深度图数据（按价格聚合）
    pub fn get_order_depth() -> (Vec<(u64, BalanceOf<T>)>, Vec<(u64, BalanceOf<T>)>) {
        // 🆕 L1-R2修复: get_sell/buy_order_list 已过滤过期订单，无需重复检查

        // 卖单深度（按价格升序）
        let mut asks: Vec<(u64, BalanceOf<T>)> = Vec::new();
        for order in Self::get_sell_order_list() {
            let remaining = order.nex_amount.saturating_sub(order.filled_amount);
            if let Some(entry) = asks.iter_mut().find(|(p, _)| *p == order.usdt_price) {
                entry.1 = entry.1.saturating_add(remaining);
            } else {
                asks.push((order.usdt_price, remaining));
            }
        }
        asks.sort_by_key(|(p, _)| *p);

        // 买单深度（按价格降序）
        let mut bids: Vec<(u64, BalanceOf<T>)> = Vec::new();
        for order in Self::get_buy_order_list() {
            let remaining = order.nex_amount.saturating_sub(order.filled_amount);
            if let Some(entry) = bids.iter_mut().find(|(p, _)| *p == order.usdt_price) {
                entry.1 = entry.1.saturating_add(remaining);
            } else {
                bids.push((order.usdt_price, remaining));
            }
        }
        bids.sort_by(|(a, _), (b, _)| b.cmp(a));

        (asks, bids)
    }

    /// #12: 获取用户活跃交易（待付款/待验证/待补付）
    pub fn get_active_trades(user: &T::AccountId) -> Vec<UsdtTrade<T>> {
        UserTrades::<T>::get(user)
            .iter()
            .filter_map(|&id| UsdtTrades::<T>::get(id))
            .filter(|t| {
                t.status == UsdtTradeStatus::AwaitingPayment
                    || t.status == UsdtTradeStatus::AwaitingVerification
                    || t.status == UsdtTradeStatus::UnderpaidPending
            })
            .collect()
    }

    // ==================== Runtime API 转换 ====================

    fn order_to_info(o: &Order<T>) -> runtime_api::OrderInfo<T::AccountId, BalanceOf<T>> {
        use sp_runtime::SaturatedConversion;
        runtime_api::OrderInfo {
            order_id: o.order_id,
            side: match o.side {
                OrderSide::Sell => 0,
                OrderSide::Buy => 1,
            },
            owner: o.maker.clone(),
            nex_amount: o.nex_amount,
            filled_amount: o.filled_amount,
            usdt_price: o.usdt_price,
            status: match o.status {
                OrderStatus::Open => 0,
                OrderStatus::PartiallyFilled => 1,
                OrderStatus::Filled => 2,
                OrderStatus::Cancelled => 3,
                OrderStatus::Expired => 4,
            },
            created_at: o.created_at.saturated_into(),
            expires_at: o.expires_at.saturated_into(),
            min_fill_amount: o.min_fill_amount,
        }
    }

    fn trade_to_info(t: &UsdtTrade<T>) -> runtime_api::TradeInfo<T::AccountId, BalanceOf<T>> {
        use sp_runtime::SaturatedConversion;
        runtime_api::TradeInfo {
            trade_id: t.trade_id,
            order_id: t.order_id,
            seller: t.seller.clone(),
            buyer: t.buyer.clone(),
            nex_amount: t.nex_amount,
            usdt_amount: t.usdt_amount,
            status: match t.status {
                UsdtTradeStatus::AwaitingPayment => 0,
                UsdtTradeStatus::AwaitingVerification => 1,
                UsdtTradeStatus::UnderpaidPending => 2,
                UsdtTradeStatus::Completed => 3,
                UsdtTradeStatus::Refunded => 4,
                UsdtTradeStatus::Disputed => 5,
                UsdtTradeStatus::Cancelled => 6,
            },
            created_at: t.created_at.saturated_into(),
            timeout_at: t.timeout_at.saturated_into(),
            buyer_deposit: t.buyer_deposit,
            deposit_status: match t.deposit_status {
                BuyerDepositStatus::None => 0,
                BuyerDepositStatus::Locked => 1,
                BuyerDepositStatus::Released => 2,
                BuyerDepositStatus::Forfeited => 3,
                BuyerDepositStatus::PartiallyForfeited => 4,
            },
            underpaid_deadline: t.underpaid_deadline.map(|d| d.saturated_into()),
            completed_at: t.completed_at.map(|d| d.saturated_into()),
            payment_confirmed: t.payment_confirmed,
            cumulative_penalty: t.cumulative_penalty,
        }
    }

    /// Runtime API: 获取卖单列表（支持分页）
    pub fn api_get_sell_orders(
        offset: u32,
        limit: u32,
    ) -> Vec<runtime_api::OrderInfo<T::AccountId, BalanceOf<T>>> {
        let all: Vec<_> = Self::get_sell_order_list()
            .iter()
            .map(Self::order_to_info)
            .collect();
        let start = (offset as usize).min(all.len());
        let end = (start + limit as usize).min(all.len());
        all[start..end].to_vec()
    }

    /// Runtime API: 获取买单列表（支持分页）
    pub fn api_get_buy_orders(
        offset: u32,
        limit: u32,
    ) -> Vec<runtime_api::OrderInfo<T::AccountId, BalanceOf<T>>> {
        let all: Vec<_> = Self::get_buy_order_list()
            .iter()
            .map(Self::order_to_info)
            .collect();
        let start = (offset as usize).min(all.len());
        let end = (start + limit as usize).min(all.len());
        all[start..end].to_vec()
    }

    /// Runtime API: 获取用户订单
    pub fn api_get_user_orders(
        user: &T::AccountId,
    ) -> Vec<runtime_api::OrderInfo<T::AccountId, BalanceOf<T>>> {
        Self::get_user_order_list(user)
            .iter()
            .map(Self::order_to_info)
            .collect()
    }

    /// Runtime API: 获取用户交易历史（支持分页）
    pub fn api_get_user_trades(
        user: &T::AccountId,
        offset: u32,
        limit: u32,
    ) -> Vec<runtime_api::TradeInfo<T::AccountId, BalanceOf<T>>> {
        let all: Vec<_> = Self::get_user_trade_list(user)
            .iter()
            .map(Self::trade_to_info)
            .collect();
        let start = (offset as usize).min(all.len());
        let end = (start + limit as usize).min(all.len());
        all[start..end].to_vec()
    }

    /// Runtime API: 获取订单交易
    pub fn api_get_order_trades(
        order_id: u64,
    ) -> Vec<runtime_api::TradeInfo<T::AccountId, BalanceOf<T>>> {
        Self::get_trades_by_order(order_id)
            .iter()
            .map(Self::trade_to_info)
            .collect()
    }

    /// Runtime API: 获取用户活跃交易
    pub fn api_get_active_trades(
        user: &T::AccountId,
    ) -> Vec<runtime_api::TradeInfo<T::AccountId, BalanceOf<T>>> {
        Self::get_active_trades(user)
            .iter()
            .map(Self::trade_to_info)
            .collect()
    }

    /// Runtime API: 获取深度图
    pub fn api_get_order_depth() -> (
        Vec<runtime_api::DepthEntry<BalanceOf<T>>>,
        Vec<runtime_api::DepthEntry<BalanceOf<T>>>,
    ) {
        let (asks, bids) = Self::get_order_depth();
        (
            asks.into_iter()
                .map(|(price, amount)| runtime_api::DepthEntry { price, amount })
                .collect(),
            bids.into_iter()
                .map(|(price, amount)| runtime_api::DepthEntry { price, amount })
                .collect(),
        )
    }

    /// Runtime API: 获取市场摘要
    pub fn api_get_market_summary() -> runtime_api::MarketSummary {
        let (best_ask, best_bid) = Self::get_best_prices();
        runtime_api::MarketSummary {
            best_ask,
            best_bid,
            last_trade_price: LastTradePrice::<T>::get(),
            is_paused: MarketPausedStore::<T>::get(),
            trading_fee_bps: TradingFeeBps::<T>::get(),
            pending_trades_count: PendingUsdtTrades::<T>::get().len() as u32,
        }
    }

    /// Runtime API: 获取单个订单详情
    pub fn api_get_order_by_id(
        order_id: u64,
    ) -> Option<runtime_api::OrderInfo<T::AccountId, BalanceOf<T>>> {
        Orders::<T>::get(order_id).map(|o| Self::order_to_info(&o))
    }

    /// Runtime API: 获取单个交易详情
    pub fn api_get_trade_by_id(
        trade_id: u64,
    ) -> Option<runtime_api::TradeInfo<T::AccountId, BalanceOf<T>>> {
        UsdtTrades::<T>::get(trade_id).map(|t| Self::trade_to_info(&t))
    }

    /// Runtime API: 获取单个 Indexer 详情
    pub fn api_get_indexer_info(
        account: T::AccountId,
    ) -> Option<runtime_api::IndexerInfoView<T::AccountId, BalanceOf<T>>> {
        IndexerSet::<T>::get(&account).map(|info| Self::indexer_to_view(account, &info))
    }

    /// Runtime API: 获取所有 Indexer 列表
    pub fn api_get_all_indexers() -> Vec<runtime_api::IndexerInfoView<T::AccountId, BalanceOf<T>>> {
        IndexerSet::<T>::iter()
            .map(|(account, info)| Self::indexer_to_view(account, &info))
            .collect()
    }

    /// Runtime API: 获取 Indexer 网络汇总
    pub fn api_get_indexer_network_summary() -> runtime_api::IndexerNetworkSummary<BalanceOf<T>> {
        use frame_support::traits::{Currency, Get};
        use sp_runtime::traits::Zero;

        let mut total_count: u32 = 0;
        let mut suspended_count: u32 = 0;
        let mut total_accelerated: u64 = 0;
        let mut total_errors: u64 = 0;
        let mut total_staked: BalanceOf<T> = Zero::zero();

        for (_account, info) in IndexerSet::<T>::iter() {
            total_count += 1;
            if info.suspended {
                suspended_count += 1;
            }
            total_accelerated = total_accelerated.saturating_add(info.accelerated_count as u64);
            total_errors = total_errors.saturating_add(info.error_count as u64);
            total_staked = total_staked.saturating_add(info.stake);
        }

        let reward_pool = T::RewardSource::get();
        let reward_pool_balance = T::Currency::free_balance(&reward_pool);

        runtime_api::IndexerNetworkSummary {
            total_count,
            max_capacity: T::MaxIndexers::get(),
            active_count: total_count.saturating_sub(suspended_count),
            suspended_count,
            total_accelerated,
            total_errors,
            total_staked,
            min_stake: T::MinIndexerStake::get(),
            reward_pool_balance,
            hint_reward: T::IndexerHintReward::get(),
            pool_share_bps: IndexerPoolShareBps::<T>::get(),
            trading_fee_bps: TradingFeeBps::<T>::get(),
        }
    }

    /// 内部: IndexerInfo → IndexerInfoView 转换
    fn indexer_to_view(
        account: T::AccountId,
        info: &IndexerInfo<T>,
    ) -> runtime_api::IndexerInfoView<T::AccountId, BalanceOf<T>> {
        let total = info.accelerated_count.saturating_add(info.error_count);
        let health_score = if info.suspended {
            0u16
        } else if total == 0 {
            500u16 // 新注册，中等分
        } else {
            // 成功率 × 1000，上限 1000
            let score = (info.accelerated_count as u64)
                .saturating_mul(1000)
                .checked_div(total as u64)
                .unwrap_or(500) as u16;
            score.min(1000)
        };

        runtime_api::IndexerInfoView {
            account,
            endpoint_url: info.endpoint_url.to_vec(),
            stake: info.stake,
            registered_at: SatConv2::saturated_into(info.registered_at),
            accelerated_count: info.accelerated_count,
            error_count: info.error_count,
            pending_hint_count: info.pending_hint_count,
            suspended: info.suspended,
            health_score,
        }
    }
}
