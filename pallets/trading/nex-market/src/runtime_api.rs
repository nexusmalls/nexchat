//! Runtime API 定义：用于前端查询 NEX/USDT 市场数据

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use Debug;

/// 订单摘要（Runtime API 返回用，不含泛型 BlockNumber）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct OrderInfo<AccountId, Balance> {
    pub order_id: u64,
    /// 0 = Sell, 1 = Buy
    pub side: u8,
    pub owner: AccountId,
    pub nex_amount: Balance,
    pub filled_amount: Balance,
    pub usdt_price: u64,
    /// 0=Open, 1=PartiallyFilled, 2=Filled, 3=Cancelled, 4=Expired
    pub status: u8,
    pub created_at: u64,
    pub expires_at: u64,
    pub min_fill_amount: Balance,
}

/// 交易摘要（Runtime API 返回用）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct TradeInfo<AccountId, Balance> {
    pub trade_id: u64,
    pub order_id: u64,
    pub seller: AccountId,
    pub buyer: AccountId,
    pub nex_amount: Balance,
    pub usdt_amount: u64,
    /// 0=AwaitingPayment, 1=AwaitingVerification, 2=Completed, 3=Refunded,
    /// 4=UnderpaidPending
    pub status: u8,
    pub created_at: u64,
    pub timeout_at: u64,
    pub buyer_deposit: Balance,
    /// 0=None, 1=Locked, 2=Released, 3=Forfeited
    pub deposit_status: u8,
    pub underpaid_deadline: Option<u64>,
    /// W5: 交易终态时间（区块号），用于精确争议窗口
    pub completed_at: Option<u64>,
    /// W6: 买家是否已确认/检测到付款
    pub payment_confirmed: bool,
    /// 逾期罚金：已累计扣除的保证金金额
    pub cumulative_penalty: Balance,
}

/// 市场统计摘要
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct MarketSummary {
    pub best_ask: Option<u64>,
    pub best_bid: Option<u64>,
    pub last_trade_price: Option<u64>,
    pub is_paused: bool,
    pub trading_fee_bps: u16,
    pub pending_trades_count: u32,
}

/// 深度图条目
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct DepthEntry<Balance> {
    pub price: u64,
    pub amount: Balance,
}

/// Indexer 节点详情（Runtime API 返回用）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct IndexerInfoView<AccountId, Balance> {
    pub account: AccountId,
    /// 端点 URL（UTF-8）
    pub endpoint_url: Vec<u8>,
    /// 质押金额
    pub stake: Balance,
    /// 注册区块
    pub registered_at: u64,
    /// 成功验证次数
    pub accelerated_count: u32,
    /// 错误次数
    pub error_count: u32,
    /// 待处理 hint 数
    pub pending_hint_count: u32,
    /// 是否被暂停
    pub suspended: bool,
    /// 健康评分 (0-1000)：基于成功率和错误率
    pub health_score: u16,
}

/// Indexer 网络汇总信息
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct IndexerNetworkSummary<Balance> {
    /// 注册 Indexer 总数（含暂停）
    pub total_count: u32,
    /// 最大 Indexer 容量
    pub max_capacity: u32,
    /// 活跃 Indexer 数量（未暂停）
    pub active_count: u32,
    /// 被暂停的 Indexer 数量
    pub suspended_count: u32,
    /// 全网总加速验证次数
    pub total_accelerated: u64,
    /// 全网总错误次数
    pub total_errors: u64,
    /// 全网总质押
    pub total_staked: Balance,
    /// 最低质押要求
    pub min_stake: Balance,
    /// 奖池账户余额
    pub reward_pool_balance: Balance,
    /// 单次 hint 奖励金额
    pub hint_reward: Balance,
    /// 奖池分成比例 (bps)
    pub pool_share_bps: u16,
    /// 交易手续费率 (bps)
    pub trading_fee_bps: u16,
}

sp_api::decl_runtime_apis! {
    /// NEX Market Runtime API
    pub trait NexMarketApi<AccountId, Balance>
    where
        AccountId: Codec,
        Balance: Codec,
    {
        /// 获取活跃卖单列表（按价格升序，支持分页）
        fn get_sell_orders(offset: u32, limit: u32) -> Vec<OrderInfo<AccountId, Balance>>;

        /// 获取活跃买单列表（按价格降序，支持分页）
        fn get_buy_orders(offset: u32, limit: u32) -> Vec<OrderInfo<AccountId, Balance>>;

        /// 获取用户的所有订单
        fn get_user_orders(user: AccountId) -> Vec<OrderInfo<AccountId, Balance>>;

        /// 获取用户交易历史（支持分页）
        fn get_user_trades(user: AccountId, offset: u32, limit: u32) -> Vec<TradeInfo<AccountId, Balance>>;

        /// 获取订单关联的交易列表
        fn get_order_trades(order_id: u64) -> Vec<TradeInfo<AccountId, Balance>>;

        /// 获取用户活跃交易（待付款/待验证/待补付）
        fn get_active_trades(user: AccountId) -> Vec<TradeInfo<AccountId, Balance>>;

        /// 获取订单深度图（asks 升序, bids 降序）
        fn get_order_depth() -> (Vec<DepthEntry<Balance>>, Vec<DepthEntry<Balance>>);

        /// 获取最优买卖价格
        fn get_best_prices() -> (Option<u64>, Option<u64>);

        /// 获取市场统计摘要
        fn get_market_summary() -> MarketSummary;

        /// 获取单个订单详情
        fn get_order_by_id(order_id: u64) -> Option<OrderInfo<AccountId, Balance>>;

        /// 获取单个交易详情
        fn get_trade_by_id(trade_id: u64) -> Option<TradeInfo<AccountId, Balance>>;

        /// 获取单个 Indexer 详情
        fn get_indexer_info(account: AccountId) -> Option<IndexerInfoView<AccountId, Balance>>;

        /// 获取所有 Indexer 列表
        fn get_all_indexers() -> Vec<IndexerInfoView<AccountId, Balance>>;

        /// 获取 Indexer 网络汇总
        fn get_indexer_network_summary() -> IndexerNetworkSummary<Balance>;
    }
}
