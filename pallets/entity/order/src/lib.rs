//! # Entity order pallet (pallet-entity-order)
//!
//! ## Overview
//!
//! This pallet manages the full order lifecycle, including:
//! 本模块负责订单的完整生命周期管理，包括：
//! - placing and paying for orders (escrow-backed funding)
//! - 下单并支付（资金托管）
//! - order cancellation
//! - 取消订单
//! - shipment
//! - 发货
//! - delivery confirmation
//! - 确认收货
//! - refund flow
//! - 退款流程
//! - automatic timeout handling
//! - 超时自动处理
//!
//! ## Version history
//!
//! - v0.1.0 (2026-01-31): split out from pallet-mall
//! - v0.1.0 (2026-01-31): 从 pallet-mall 拆分

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use pallet_entity_common::OrderStatus;

/// 订单系统通知端口（runtime 适配器桥接到 chat-core 的 System 通道）。
/// Order system-notification port; the runtime adapter bridges these calls to
/// chat-core's System channel.
///
/// # 解耦与尽力而为 / Decoupling & best-effort
/// 与 `pallet-chat-core` 解耦：order 仅依赖
/// 本 trait，由 runtime 适配器调用 `ChatCore::notify`。调用为**尽力而为**：通知失败
/// **绝不**回滚订单状态转移（适配器吞错）。`()` 提供 no-op 默认实现,便于 mock/可选接线。
/// Decoupled from `pallet-chat-core`; the runtime adapter calls `ChatCore::notify`.
/// Best-effort: a notification failure must NEVER abort an order state transition
/// (the adapter swallows errors). `()` is a no-op default for mocks/optional wiring.
pub trait OrderNotifier<AccountId> {
    /// 向 `to` 推送一条订单系统通知（payload 为客户端本地化的模板描述符）。
    /// Push an order system notice to `to` (payload = client-localized template descriptor).
    fn notify(to: &AccountId, notice: alloc::vec::Vec<u8>);
}

impl<AccountId> OrderNotifier<AccountId> for () {
    fn notify(_to: &AccountId, _notice: alloc::vec::Vec<u8>) {}
}

/// 订单聊天授权端口（runtime 适配器映射为 chat-permission 场景授权）。
/// Chat-authorization port for the order scene; the runtime adapter maps these
/// calls to chat-permission scene authorizations.
///
/// # 解耦与尽力而为 / Decoupling & best-effort
/// EN: Decoupled from `pallet-chat-permission`: the order pallet depends only on this
/// trait, and the runtime adapter grants/revokes a bidirectional buyer↔seller
/// scene authorization (`source = *b"entorder"`, `SceneType::Order`,
/// `scene_id = Numeric(order_id)`). Calls are **best-effort**: a failure (e.g.
/// the pair's scene table is full) MUST NEVER abort an order state transition —
/// the adapter swallows errors. `()` is a no-op default for mocks/optional wiring.
/// CN: 与 `pallet-chat-permission` 解耦：
/// 订单模块仅依赖本 trait，由 runtime 适配器授予/撤销买卖双方的双向场景授权
/// （`source = *b"entorder"`，`SceneType::Order`，`scene_id = Numeric(order_id)`）。
/// 调用为**尽力而为**：失败（如该用户对的场景表已满）**绝不**回滚订单状态转移——
/// 适配器吞错。`()` 提供 no-op 默认实现，便于 mock/可选接线。
pub trait OrderChatAuthorizer<AccountId> {
    /// Grant bidirectional buyer↔seller chat for this order. / 为该订单授予买卖双方双向聊天。
    fn grant(order_id: u64, buyer: &AccountId, seller: &AccountId);
    /// Revoke this order's buyer↔seller chat authorization. / 撤销该订单的买卖双方聊天授权。
    fn revoke(order_id: u64, buyer: &AccountId, seller: &AccountId);
}

impl<AccountId> OrderChatAuthorizer<AccountId> for () {
    fn grant(_order_id: u64, _buyer: &AccountId, _seller: &AccountId) {}
    fn revoke(_order_id: u64, _buyer: &AccountId, _seller: &AccountId) {}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::WeightInfo;

mod dispute;
pub mod migration;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use alloc::vec::Vec;
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, Get, ReservableCurrency},
        BoundedVec,
    };
    use frame_system::ensure_root;
    use frame_system::pallet_prelude::*;
    use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
    use pallet_entity_common::{
        AssetLedgerPort, EntityProvider, EntityTokenPriceProvider, LoyaltyReadPort,
        LoyaltyWritePort, MemberProvider, OnOrderCancelled, OnOrderCompleted,
        OrderCancellationInfo, OrderCompletionInfo, OrderProvider, OrderStatus, PaymentAsset,
        PricingProvider, ProductCategory, ProductProvider, ProductStatus, ProductVisibility,
        ShopProvider, TokenFeeConfigPort,
    };
    use sp_runtime::{
        traits::{Saturating, Zero},
        SaturatedConversion,
    };

    /// 货币余额类型别名
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    /// Order data snapshot.
    /// 订单信息。
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
    #[scale_info(skip_type_params(MaxCidLen))]
    pub struct Order<AccountId, Balance, BlockNumber, MaxCidLen: Get<u32>> {
        pub id: u64,
        /// Entity ID captured at order creation to avoid indirect shop lookups later.
        /// 订单所属 Entity ID（创建时快照，避免后续通过 shop 间接查询）
        pub entity_id: u64,
        pub shop_id: u64,
        pub product_id: u64,
        pub buyer: AccountId,
        pub seller: AccountId,
        /// Optional payer account. `Some` means a third-party payer; `None` means self-paid.
        /// 代付人（第三方付款时为 Some，自付时为 None）
        pub payer: Option<AccountId>,
        pub quantity: u32,
        /// NEX unit-price snapshot after converting from USDT.
        /// NEX 单价快照（USDT → NEX 换算后）
        pub unit_price: Balance,
        /// Final paid amount in NEX after points, shopping balance, and member discounts.
        /// 实际支付金额（积分/购物余额/会员折扣后，NEX 计价）
        pub total_amount: Balance,
        pub platform_fee: Balance,
        /// USDT total-price snapshot after discounts, using 10^6 precision.
        /// USDT 总价快照（折扣后，精度 10^6）
        pub usdt_total: u64,
        /// NEX/USDT exchange-rate snapshot at order time, using 10^6 precision.
        /// 下单时 NEX/USDT 汇率快照（精度 10^6）
        pub nex_usdt_rate: u64,
        /// Token/NEX exchange-rate snapshot at order time, using 10^12 precision.
        /// Native payment uses 0.
        /// 下单时 Token/NEX 汇率快照（精度 10^12，Native 支付为 0）
        pub token_nex_rate: u128,
        /// Product category determines the order flow and whether shipping is required.
        /// 商品类别（决定订单流程，是否需要物流由此推导）
        pub product_category: ProductCategory,
        pub shipping_cid: Option<BoundedVec<u8, MaxCidLen>>,
        pub tracking_cid: Option<BoundedVec<u8, MaxCidLen>>,
        pub status: OrderStatus,
        pub created_at: BlockNumber,
        pub shipped_at: Option<BlockNumber>,
        pub completed_at: Option<BlockNumber>,
        pub service_started_at: Option<BlockNumber>,
        /// Service completion timestamp set by the seller. Can only be set once.
        /// 服务完成时间（卖家标记，限设置一次）
        pub service_completed_at: Option<BlockNumber>,
        pub payment_asset: PaymentAsset,
        /// Token payment amount, only used for `EntityToken` payment.
        /// `u128` avoids generic expansion.
        /// Token 支付金额（仅 EntityToken 时有效，u128 避免泛型膨胀）
        pub token_payment_amount: u128,
        /// Whether the buyer has already extended the confirmation deadline.
        /// Limited to one extension.
        /// 买家是否已延长确认收货期限（限延一次）
        pub confirm_extended: bool,
        /// Whether the seller has already rejected a refund request.
        /// Limited to one rejection to avoid infinite delay.
        /// 卖家是否已拒绝退款（限一次，防无限延期）
        pub dispute_rejected: bool,
        pub dispute_deadline: Option<BlockNumber>,
        pub note_cid: Option<BoundedVec<u8, MaxCidLen>>,
        /// Refund or dispute reason CID, stored during `request_refund`.
        /// 退款/争议理由 CID（`request_refund` 时存储）
        pub refund_reason_cid: Option<BoundedVec<u8, MaxCidLen>>,
        /// Shopping balance consumed at order time, denominated in NEX and rolled back on refund.
        /// 下单时消费的购物余额（NEX 计价，退款时回滚）
        pub shopping_balance_used: Balance,
        /// Tokens burned for discount at order time, rolled back on refund.
        /// Uses the same `u128` pattern as `token_payment_amount`.
        /// 下单时燃烧的 Token 数量（退款时回滚，`u128` 与 `token_payment_amount` 同模式）
        pub token_discount_tokens_burned: u128,
    }

    /// 订单类型别名
    pub type OrderOf<T> = Order<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        BlockNumberFor<T>,
        <T as Config>::MaxCidLength,
    >;

    /// Order statistics.
    /// 订单统计。
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
    pub struct OrderStatistics<Balance: Default> {
        /// Total order count.
        /// 总订单数
        pub total_orders: u64,
        /// Completed order count.
        /// 已完成订单数
        pub completed_orders: u64,
        /// Total trading volume in NEX.
        /// 总交易额（NEX）
        pub total_volume: Balance,
        /// Total platform-fee revenue in NEX.
        /// 总平台费收入（NEX）
        pub total_platform_fees: Balance,
        /// Total token trading volume, using `u128` to avoid generic expansion.
        /// 总 Token 交易额（`u128` 避免泛型膨胀）
        pub total_token_volume: u128,
        /// Total token platform fees, stored as `u128`.
        /// 总 Token 平台费（`u128`）
        pub total_token_platform_fees: u128,
    }

    /// Order side-effect operation type used for failure tracking.
    /// 订单附属操作类型（用于失败事件追踪）
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
    pub enum OrderOperation {
        /// Escrow refund.
        /// Escrow 退款
        EscrowRefund,
        /// 库存恢复
        StockRestore,
        /// Commission cancellation.
        /// 佣金取消
        CommissionCancel,
        /// Commission settlement.
        /// 佣金结算
        CommissionComplete,
        /// 店铺统计更新
        ShopStatsUpdate,
        /// 积分奖励
        TokenReward,
        /// 会员注册/消费更新
        MemberUpdate,
        /// 订单自动完成
        AutoComplete,
        /// 升级规则检查
        UpgradeRuleCheck,
        /// Token 平台费分配失败
        TokenPlatformFee,
        /// 会员自动注册失败
        MemberAutoRegister,
        /// Loyalty rollback failure (shopping balance / token discount).
        /// 忠诚度回滚失败（购物余额 / Token 折扣）
        LoyaltyRollback,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Currency implementation.
        /// 货币类型
        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Escrow integration.
        /// 托管接口
        type Escrow: EscrowTrait<Self::AccountId, BalanceOf<Self>>;

        /// Shop lookup interface for the separated entity-shop architecture.
        /// Shop 查询接口（Entity-Shop 分离架构）
        type ShopProvider: ShopProvider<Self::AccountId>;

        /// Product lookup interface.
        /// 商品查询接口
        type ProductProvider: ProductProvider<Self::AccountId>;

        /// Entity lookup interface used to resolve payment-channel configuration.
        /// 实体查询接口（用于查询支付通道配置）
        type EntityProvider: pallet_entity_common::EntityProvider<Self::AccountId>;

        /// Entity token asset-ledger interface for reserve / unreserve / repatriate.
        /// 实体代币资产账本接口（reserve / unreserve / repatriate）
        type EntityToken: AssetLedgerPort<Self::AccountId, BalanceOf<Self>>;

        /// Platform account.
        /// 平台账户
        #[pallet::constant]
        type PlatformAccount: Get<Self::AccountId>;

        /// Shipping timeout in blocks.
        /// 发货超时（区块数）
        #[pallet::constant]
        type ShipTimeout: Get<BlockNumberFor<Self>>;

        /// Delivery-confirmation timeout in blocks.
        /// 确认收货超时（区块数）
        #[pallet::constant]
        type ConfirmTimeout: Get<BlockNumberFor<Self>>;

        /// Service-confirmation timeout in blocks.
        /// 服务确认超时（区块数）
        #[pallet::constant]
        type ServiceConfirmTimeout: Get<BlockNumberFor<Self>>;

        /// Dispute timeout in blocks. The seller must approve or reject within this window,
        /// otherwise the order is refunded automatically.
        /// 争议超时（区块数）——卖家在此期限内必须响应（approve / reject），否则自动退款
        #[pallet::constant]
        type DisputeTimeout: Get<BlockNumberFor<Self>>;

        /// Confirmation extension in blocks. The buyer may extend once.
        /// 确认收货延长时间（区块数）——买家可延长一次
        #[pallet::constant]
        type ConfirmExtension: Get<BlockNumberFor<Self>>;

        /// Commission-processing hook triggered when an order completes.
        /// Phase 5.3 moved this responsibility into `OnOrderCompleted`.
        /// 佣金处理接口（订单完成时触发返佣）→ Phase 5.3：迁入 `OnOrderCompleted` Hook
        type OnOrderCompleted: OnOrderCompleted<Self::AccountId, BalanceOf<Self>>;

        /// Order-cancellation hook used to clean up commissions and related side effects.
        /// 订单取消 Hook（取消 / 退款时清理佣金等）
        type OnOrderCancelled: OnOrderCancelled;

        /// Incentive-system interface covering token discounts, shopping balance, and rewards.
        /// 激励系统接口（Token 折扣 + 购物余额 + 奖励）
        type Loyalty: LoyaltyWritePort<Self::AccountId, BalanceOf<Self>>;

        /// Token platform-fee and entity-account lookup interface split out from
        /// `TokenCommissionHandler`.
        /// Token 平台费率 + Entity 账户查询接口（从 `TokenCommissionHandler` 剥离）
        type TokenFeeConfig: TokenFeeConfigPort<Self::AccountId>;

        /// NEX/USDT pricing interface used to convert NEX amounts into USDT for
        /// member-consumption statistics.
        /// NEX / USDT 定价接口（用于将 NEX 金额转换为 USDT 以更新会员消费统计）
        type PricingProvider: PricingProvider;

        /// Token price lookup interface used to convert Entity Token prices into NEX
        /// and then indirectly into USDT.
        /// Token 价格查询接口（Entity Token → NEX 价格，用于间接换算 USDT）
        type TokenPriceProvider: EntityTokenPriceProvider<Balance = BalanceOf<Self>>;

        /// Member lookup interface used for `MembersOnly` / `LevelGated` visibility checks.
        /// 会员查询接口（用于商品可见性校验：`MembersOnly` / `LevelGated`）
        type MemberProvider: MemberProvider<Self::AccountId>;

        /// CID 最大长度
        #[pallet::constant]
        type MaxCidLength: Get<u32>;

        /// 每买家最大订单索引数
        #[pallet::constant]
        type MaxBuyerOrders: Get<u32>;

        /// 每代付人最大订单索引数
        #[pallet::constant]
        type MaxPayerOrders: Get<u32>;

        /// 每店铺最大订单索引数
        #[pallet::constant]
        type MaxShopOrders: Get<u32>;

        /// 每区块过期队列最大订单数
        #[pallet::constant]
        type MaxExpiryQueueSize: Get<u32>;

        /// 订单系统通知端口（桥接到聊天 System 通道；尽力而为，失败不回滚订单）。
        /// Order system-notification port (bridged to chat System channel;
        /// best-effort, never aborts an order transition).
        type Notifier: OrderNotifier<Self::AccountId>;

        /// 订单聊天授权端口：订单存续期间授予买卖双方双向聊天，终态撤销（尽力而为）。
        /// Order chat-authorization port: grant buyer↔seller chat for the order's
        /// lifetime and revoke at terminal states (best-effort, never aborts).
        type Chat: OrderChatAuthorizer<Self::AccountId>;

        /// 权重信息
        type WeightInfo: WeightInfo;
    }

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(2);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // ==================== 存储项 ====================

    /// NEX 平台费率（基点，100 = 1%）
    /// 可通过 set_platform_fee_rate 治理调整，0 = 关闭平台费
    #[pallet::storage]
    pub type PlatformFeeRate<T> = StorageValue<_, u16, ValueQuery, DefaultPlatformFeeRate>;

    /// NEX 平台费率默认值（100 bps = 1%）
    #[pallet::type_value]
    pub fn DefaultPlatformFeeRate() -> u16 {
        100
    }

    /// 下一个订单 ID
    #[pallet::storage]
    #[pallet::getter(fn next_order_id)]
    pub type NextOrderId<T> = StorageValue<_, u64, ValueQuery>;

    /// 订单存储
    #[pallet::storage]
    #[pallet::getter(fn orders)]
    pub type Orders<T: Config> = StorageMap<_, Blake2_128Concat, u64, OrderOf<T>>;

    /// 买家订单索引
    #[pallet::storage]
    #[pallet::getter(fn buyer_orders)]
    pub type BuyerOrders<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u64, T::MaxBuyerOrders>,
        ValueQuery,
    >;

    /// 代付人订单索引
    #[pallet::storage]
    #[pallet::getter(fn payer_orders)]
    pub type PayerOrders<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        BoundedVec<u64, T::MaxPayerOrders>,
        ValueQuery,
    >;

    /// 店铺订单索引
    #[pallet::storage]
    #[pallet::getter(fn shop_orders)]
    pub type ShopOrders<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, BoundedVec<u64, T::MaxShopOrders>, ValueQuery>;

    /// 订单统计
    #[pallet::storage]
    #[pallet::getter(fn order_stats)]
    pub type OrderStats<T: Config> = StorageValue<_, OrderStatistics<BalanceOf<T>>, ValueQuery>;

    /// 过期检查队列：到期区块号 → 待检查订单 ID 列表
    #[pallet::storage]
    pub type ExpiryQueue<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedVec<u64, T::MaxExpiryQueueSize>,
        ValueQuery,
    >;

    /// 订单推荐人（place_order 时记录，完成时传递给 MemberHandler::auto_register）
    #[pallet::storage]
    pub type OrderReferrer<T: Config> = StorageMap<_, Blake2_128Concat, u64, T::AccountId>;

    // ==================== 事件 ====================

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// 订单已创建并支付
        OrderCreated {
            order_id: u64,
            entity_id: u64,
            buyer: T::AccountId,
            seller: T::AccountId,
            payer: Option<T::AccountId>,
            amount: BalanceOf<T>,
            payment_asset: PaymentAsset,
            token_amount: u128,
            usdt_total: u64,
            nex_usdt_rate: u64,
        },
        OrderShipped {
            order_id: u64,
        },
        OrderCompleted {
            order_id: u64,
            seller_received: BalanceOf<T>,
            token_seller_received: u128,
        },
        OrderCancelled {
            order_id: u64,
            amount: BalanceOf<T>,
            token_amount: u128,
        },
        OrderRefunded {
            order_id: u64,
            amount: BalanceOf<T>,
            token_amount: u128,
        },
        OrderDisputed {
            order_id: u64,
        },
        OrderOperationFailed {
            order_id: u64,
            operation: OrderOperation,
        },
        ServiceStarted {
            order_id: u64,
        },
        ServiceCompleted {
            order_id: u64,
        },
        PlatformFeeRateUpdated {
            old_rate: u16,
            new_rate: u16,
        },
        BuyerOrdersCleaned {
            buyer: T::AccountId,
            removed: u32,
        },
        PayerOrdersCleaned {
            payer: T::AccountId,
            removed: u32,
        },
        RefundRejected {
            order_id: u64,
            reason_cid: Vec<u8>,
        },
        OrderSellerCancelled {
            order_id: u64,
            amount: BalanceOf<T>,
            token_amount: u128,
            reason_cid: Vec<u8>,
        },
        OrderForceRefunded {
            order_id: u64,
            reason_cid: Option<Vec<u8>>,
        },
        OrderForceCompleted {
            order_id: u64,
            reason_cid: Option<Vec<u8>>,
        },
        ShippingAddressUpdated {
            order_id: u64,
        },
        ConfirmTimeoutExtended {
            order_id: u64,
            new_deadline: BlockNumberFor<T>,
        },
        ShopOrdersCleaned {
            shop_id: u64,
            removed: u32,
        },
        TrackingInfoUpdated {
            order_id: u64,
        },
        /// 卖家主动退款（Shipped 状态）
        OrderSellerRefunded {
            order_id: u64,
            amount: BalanceOf<T>,
            token_amount: u128,
            reason_cid: Vec<u8>,
        },
        /// 管理员部分退款
        OrderPartialRefunded {
            order_id: u64,
            refund_bps: u16,
            reason_cid: Option<Vec<u8>>,
        },
        /// 买家撤回争议
        DisputeWithdrawn {
            order_id: u64,
        },
        /// 管理员手动处理指定区块的过期订单
        StaleExpirationsProcessed {
            target_block: BlockNumberFor<T>,
            processed: u32,
        },
    }

    // ==================== 错误 ====================

    #[pallet::error]
    pub enum Error<T> {
        OrderNotFound,
        ProductNotFound,
        ShopNotFound,
        NotOrderBuyer,
        NotOrderSeller,
        InvalidOrderStatus,
        CannotCancelOrder,
        CannotBuyOwnProduct,
        ProductNotOnSale,
        InsufficientStock,
        CidTooLong,
        Overflow,
        DigitalProductCannotCancel,
        InvalidQuantity,
        DigitalProductCannotRefund,
        /// 非服务类/订阅类订单
        NotServiceLikeOrder,
        InvalidAmount,
        ExpiryQueueFull,
        ShippingCidRequired,
        /// 服务/订阅类订单不可使用发货/收货流程
        ServiceLikeOrderCannotShip,
        EmptyReasonCid,
        EntityTokenNotEnabled,
        InsufficientTokenBalance,
        EmptyTrackingCid,
        PlatformFeeRateTooHigh,
        NothingToClean,
        NotShopOwner,
        AlreadyExtended,
        CannotForceOrder,
        QuantityBelowMinimum,
        QuantityAboveMaximum,
        ProductMembersOnly,
        MemberLevelInsufficient,
        DisputeAlreadyRejected,
        /// 买家已被该 Entity 封禁
        BuyerBanned,
        /// 部分退款比例无效（需 1-9999 bps）
        InvalidRefundBps,
        /// Token 订单不支持部分退款
        PartialRefundNotSupported,
        /// 推荐人不能是买家或卖家自己
        InvalidReferrer,
        /// Subscription 类商品暂不支持下单（与 Service 流程等价，请使用 Service 类别）
        SubscriptionNotSupported,
        /// 店铺未激活（存在但处于暂停/关闭状态）
        ShopInactive,
        /// 代付人不能是卖家
        PayerCannotBeSeller,
        /// Cross-chain order targets a non-Digital product (HB-ENT-01 first phase
        /// only supports Digital). 跨链下单指向了非 Digital 商品（HB-ENT-01 首期仅支持 Digital）。
        CrossOrderDigitalOnly,
        /// Cross-chain order targets a non-Public product (HB-ENT-01 first phase
        /// only supports Public). 跨链下单指向了非 Public 商品（HB-ENT-01 首期仅支持 Public）。
        CrossOrderPublicOnly,
        /// 非订单参与者（buyer/payer）
        NotOrderParticipant,
        /// 代付人订单索引已满
        PayerOrdersFull,
        /// 会员未绑定推荐人（REFERRAL_REQUIRED 策略下不允许下单）
        ReferrerRequired,
        /// USDT 价格未设置
        UsdtPriceNotSet,
        /// NEX/USDT 价格不可用
        NexPriceUnavailable,
        /// Token 价格不可用
        TokenPriceUnavailable,
        /// NEX 滑点超限
        NexSlippageExceeded,
        /// Token 滑点超限
        TokenSlippageExceeded,
        /// 支付通道未启用
        PaymentChannelDisabled,
        /// 代付人不能消费买家的积分/购物余额
        PayerCannotUseBuyerLoyalty,
        /// 购物余额不足以全额支付
        InsufficientShoppingBalance,
    }

    // ==================== Hooks ====================

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// 空闲时处理超时订单（基于 ExpiryQueue 精确索引）
        fn on_idle(now: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
            let per_order_weight = Weight::from_parts(200_000_000, 8_000);
            if remaining_weight.ref_time() < per_order_weight.ref_time().saturating_add(50_000_000)
            {
                return Weight::zero();
            }

            let max_count =
                (remaining_weight.ref_time() / per_order_weight.ref_time()).min(20) as u32;
            Self::process_expired_orders(now, now, max_count)
        }
    }

    // ==================== Extrinsics ====================

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// 下单并支付
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::place_order())]
        pub fn place_order(
            origin: OriginFor<T>,
            product_id: u64,
            quantity: u32,
            shipping_cid: Option<Vec<u8>>,
            use_tokens: Option<BalanceOf<T>>,
            payment_asset: Option<PaymentAsset>,
            note_cid: Option<Vec<u8>>,
            referrer: Option<T::AccountId>,
            max_nex_amount: Option<BalanceOf<T>>,
            max_token_amount: Option<u128>,
        ) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            Self::do_place_order(
                buyer,
                None,
                product_id,
                quantity,
                shipping_cid,
                use_tokens,
                payment_asset,
                note_cid,
                referrer,
                max_nex_amount,
                max_token_amount,
            )
        }

        /// 取消订单（数字商品不可取消）
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::cancel_order())]
        pub fn cancel_order(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(
                Self::is_order_participant(&order, &who),
                Error::<T>::NotOrderParticipant
            );
            ensure!(
                order.product_category != ProductCategory::Digital,
                Error::<T>::DigitalProductCannotCancel
            );
            ensure!(
                order.status == OrderStatus::Paid,
                Error::<T>::CannotCancelOrder
            );

            Self::do_cancel_or_refund(&order, order_id, OrderStatus::Cancelled)?;

            Self::deposit_event(Event::OrderCancelled {
                order_id,
                amount: order.total_amount,
                token_amount: order.token_payment_amount,
            });
            Ok(())
        }

        /// 发货（服务/订阅类不可用，须走 start_service）
        #[pallet::call_index(2)]
        // 叠加系统通知的存储成本（chat-core `notify` ≈ do_send 的 System 分支）。
        // Add the system-notification storage cost on top of base ship weight.
        #[pallet::weight(T::WeightInfo::ship_order().saturating_add(T::DbWeight::get().reads_writes(8, 8)))]
        pub fn ship_order(
            origin: OriginFor<T>,
            order_id: u64,
            tracking_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let (created_at, buyer) = Orders::<T>::try_mutate(
                order_id,
                |maybe_order| -> Result<(BlockNumberFor<T>, T::AccountId), DispatchError> {
                    let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                    ensure!(order.seller == who, Error::<T>::NotOrderSeller);
                    ensure!(
                        !Self::is_service_like(&order.product_category),
                        Error::<T>::ServiceLikeOrderCannotShip
                    );
                    ensure!(
                        order.status == OrderStatus::Paid,
                        Error::<T>::InvalidOrderStatus
                    );

                    ensure!(!tracking_cid.is_empty(), Error::<T>::EmptyTrackingCid);
                    order.tracking_cid = Some(
                        tracking_cid
                            .try_into()
                            .map_err(|_| Error::<T>::CidTooLong)?,
                    );
                    let ca = order.created_at;
                    order.status = OrderStatus::Shipped;
                    order.shipped_at = Some(<frame_system::Pallet<T>>::block_number());
                    Ok((ca, order.buyer.clone()))
                },
            )?;

            // H4: 清理 place_order 创建的旧 ShipTimeout 条目
            let old_expiry = created_at.saturating_add(T::ShipTimeout::get());
            ExpiryQueue::<T>::mutate(old_expiry, |ids| {
                ids.retain(|&id| id != order_id);
            });

            let now = <frame_system::Pallet<T>>::block_number();
            let expiry_block = now.saturating_add(T::ConfirmTimeout::get());
            ExpiryQueue::<T>::try_mutate(expiry_block, |ids| {
                ids.try_push(order_id)
                    .map_err(|_| Error::<T>::ExpiryQueueFull)
            })?;

            Self::deposit_event(Event::OrderShipped { order_id });

            // 系统通知买家「订单已发货」（模板描述符 + order_id 深链）。尽力而为：
            // 适配器内部吞错，通知失败不影响发货成功。
            // Notify the buyer of shipment (best-effort; adapter swallows errors).
            T::Notifier::notify(&buyer, Self::notice(b"order:shipped:", order_id));
            Ok(())
        }

        /// 确认收货
        #[pallet::call_index(3)]
        // 叠加系统通知的存储成本（同 ship_order）。/ Add notify storage cost.
        #[pallet::weight(T::WeightInfo::confirm_receipt().saturating_add(T::DbWeight::get().reads_writes(8, 8)))]
        pub fn confirm_receipt(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.buyer == who, Error::<T>::NotOrderBuyer);
            ensure!(
                !Self::is_service_like(&order.product_category),
                Error::<T>::ServiceLikeOrderCannotShip
            );
            ensure!(
                order.status == OrderStatus::Shipped,
                Error::<T>::InvalidOrderStatus
            );

            Self::do_complete_order(order_id, &order)?;

            // 系统通知卖家「买家已确认收货 / 订单完成」。尽力而为,不影响完成流程。
            // Notify the seller that the buyer confirmed receipt (best-effort).
            T::Notifier::notify(&order.seller, Self::notice(b"order:confirmed:", order_id));
            Ok(())
        }

        /// 申请退款（数字商品不可退款）
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::request_refund())]
        pub fn request_refund(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_request_refund(who, order_id, reason_cid)
        }

        /// 同意退款（卖家）
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::approve_refund())]
        pub fn approve_refund(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_approve_refund(who, order_id)
        }

        /// 开始服务（卖家，服务类订单）
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::start_service())]
        pub fn start_service(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let created_at = Orders::<T>::try_mutate(
                order_id,
                |maybe_order| -> Result<BlockNumberFor<T>, DispatchError> {
                    let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                    ensure!(order.seller == who, Error::<T>::NotOrderSeller);
                    ensure!(
                        Self::is_service_like(&order.product_category),
                        Error::<T>::NotServiceLikeOrder
                    );
                    ensure!(
                        order.status == OrderStatus::Paid,
                        Error::<T>::InvalidOrderStatus
                    );

                    let ca = order.created_at;
                    order.status = OrderStatus::Shipped;
                    order.service_started_at = Some(<frame_system::Pallet<T>>::block_number());
                    Ok(ca)
                },
            )?;

            // H4: 清理 place_order 创建的旧 ShipTimeout 条目
            let old_expiry = created_at.saturating_add(T::ShipTimeout::get());
            ExpiryQueue::<T>::mutate(old_expiry, |ids| {
                ids.retain(|&id| id != order_id);
            });

            let now = <frame_system::Pallet<T>>::block_number();
            let expiry_block = now.saturating_add(T::ServiceConfirmTimeout::get());
            ExpiryQueue::<T>::try_mutate(expiry_block, |ids| {
                ids.try_push(order_id)
                    .map_err(|_| Error::<T>::ExpiryQueueFull)
            })?;

            Self::deposit_event(Event::ServiceStarted { order_id });
            Ok(())
        }

        /// 标记服务完成（卖家，服务类订单）
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::complete_service())]
        pub fn complete_service(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let service_started_at = Orders::<T>::try_mutate(
                order_id,
                |maybe_order| -> Result<Option<BlockNumberFor<T>>, DispatchError> {
                    let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                    ensure!(order.seller == who, Error::<T>::NotOrderSeller);
                    ensure!(
                        Self::is_service_like(&order.product_category),
                        Error::<T>::NotServiceLikeOrder
                    );
                    ensure!(
                        order.status == OrderStatus::Shipped,
                        Error::<T>::InvalidOrderStatus
                    );
                    ensure!(
                        order.service_completed_at.is_none(),
                        Error::<T>::InvalidOrderStatus
                    );

                    let sa = order.service_started_at;
                    order.service_completed_at = Some(<frame_system::Pallet<T>>::block_number());
                    Ok(sa)
                },
            )?;

            // H4: 清理 start_service 创建的旧 ServiceConfirmTimeout 条目
            if let Some(sa) = service_started_at {
                let old_expiry = sa.saturating_add(T::ServiceConfirmTimeout::get());
                ExpiryQueue::<T>::mutate(old_expiry, |ids| {
                    ids.retain(|&id| id != order_id);
                });
            }

            let now = <frame_system::Pallet<T>>::block_number();
            let expiry_block = now.saturating_add(T::ServiceConfirmTimeout::get());
            ExpiryQueue::<T>::try_mutate(expiry_block, |ids| {
                ids.try_push(order_id)
                    .map_err(|_| Error::<T>::ExpiryQueueFull)
            })?;

            Self::deposit_event(Event::ServiceCompleted { order_id });
            Ok(())
        }

        /// 确认服务完成（买家，服务类订单）
        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::confirm_service())]
        pub fn confirm_service(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.buyer == who, Error::<T>::NotOrderBuyer);
            ensure!(
                Self::is_service_like(&order.product_category),
                Error::<T>::NotServiceLikeOrder
            );
            ensure!(
                order.status == OrderStatus::Shipped,
                Error::<T>::InvalidOrderStatus
            );
            ensure!(
                order.service_completed_at.is_some(),
                Error::<T>::InvalidOrderStatus
            );

            Self::do_complete_order(order_id, &order)
        }

        /// 设置 NEX 平台费率（Root / 治理）
        ///
        /// rate 为基点，0 = 关闭平台费，上限 1000 bps（10%）
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::set_platform_fee_rate())]
        pub fn set_platform_fee_rate(origin: OriginFor<T>, new_rate: u16) -> DispatchResult {
            ensure_root(origin)?;
            ensure!(new_rate <= 1000, Error::<T>::PlatformFeeRateTooHigh);
            let old_rate = PlatformFeeRate::<T>::get();
            PlatformFeeRate::<T>::put(new_rate);
            Self::deposit_event(Event::PlatformFeeRateUpdated { old_rate, new_rate });
            Ok(())
        }

        /// 清理买家订单索引（移除已终态的订单 ID，释放 BoundedVec 容量）
        ///
        /// 终态 = Completed / Cancelled / Refunded
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::cleanup_buyer_orders())]
        pub fn cleanup_buyer_orders(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let orders = BuyerOrders::<T>::get(&who);
            let before = orders.len() as u32;

            // 保留非终态订单
            let retained: Vec<u64> = orders
                .iter()
                .copied()
                .filter(|&oid| {
                    Orders::<T>::get(oid)
                        .map(|o| {
                            !matches!(
                                o.status,
                                OrderStatus::Completed
                                    | OrderStatus::Cancelled
                                    | OrderStatus::Refunded
                            )
                        })
                        .unwrap_or(false) // 订单不存在也移除
                })
                .collect();

            let after = retained.len() as u32;
            let removed = before.saturating_sub(after);
            ensure!(removed > 0, Error::<T>::NothingToClean);

            let bounded: BoundedVec<u64, T::MaxBuyerOrders> = retained
                .try_into()
                .expect("retained is subset of original bounded vec");
            BuyerOrders::<T>::insert(&who, bounded);

            Self::deposit_event(Event::BuyerOrdersCleaned {
                buyer: who,
                removed,
            });
            Ok(())
        }

        /// 拒绝退款（卖家）— 订单保持 Disputed，写入争议超时队列
        ///
        /// 卖家拒绝后，争议进入 DisputeTimeout 倒计时。
        /// 超时未仲裁则自动退款给买家。
        #[pallet::call_index(11)]
        #[pallet::weight(T::WeightInfo::reject_refund())]
        pub fn reject_refund(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_reject_refund(who, order_id, reason_cid)
        }

        /// 卖家主动取消订单（仅 Paid 状态，非数字商品）
        #[pallet::call_index(12)]
        #[pallet::weight(T::WeightInfo::seller_cancel_order())]
        pub fn seller_cancel_order(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let bounded_reason = Self::validate_reason_cid(reason_cid)?;

            let order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.seller == who, Error::<T>::NotOrderSeller);
            ensure!(
                order.product_category != ProductCategory::Digital,
                Error::<T>::DigitalProductCannotCancel
            );
            ensure!(
                order.status == OrderStatus::Paid,
                Error::<T>::CannotCancelOrder
            );

            Self::do_cancel_or_refund(&order, order_id, OrderStatus::Cancelled)?;

            Self::deposit_event(Event::OrderSellerCancelled {
                order_id,
                amount: order.total_amount,
                token_amount: order.token_payment_amount,
                reason_cid: bounded_reason.into_inner(),
            });
            Ok(())
        }

        /// 管理员强制退款（Root / 治理）
        ///
        /// 可对 Paid / Shipped / Disputed 状态的订单强制退款
        #[pallet::call_index(13)]
        #[pallet::weight(T::WeightInfo::force_refund())]
        pub fn force_refund(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Option<Vec<u8>>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_force_refund(order_id, reason_cid)
        }

        /// 管理员强制完成订单（Root / 治理）
        #[pallet::call_index(14)]
        #[pallet::weight(T::WeightInfo::force_complete())]
        pub fn force_complete(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Option<Vec<u8>>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_force_complete(order_id, reason_cid)
        }

        /// 买家修改收货地址（仅 Paid 状态，发货前）
        #[pallet::call_index(15)]
        #[pallet::weight(T::WeightInfo::update_shipping_address())]
        pub fn update_shipping_address(
            origin: OriginFor<T>,
            order_id: u64,
            new_shipping_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(
                !new_shipping_cid.is_empty(),
                Error::<T>::ShippingCidRequired
            );
            let bounded_cid: BoundedVec<u8, T::MaxCidLength> = new_shipping_cid
                .try_into()
                .map_err(|_| Error::<T>::CidTooLong)?;

            Orders::<T>::try_mutate(order_id, |maybe_order| -> DispatchResult {
                let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                ensure!(order.buyer == who, Error::<T>::NotOrderBuyer);
                ensure!(
                    order.status == OrderStatus::Paid,
                    Error::<T>::InvalidOrderStatus
                );
                ensure!(
                    Self::category_requires_shipping(&order.product_category),
                    Error::<T>::ServiceLikeOrderCannotShip
                );

                order.shipping_cid = Some(bounded_cid);
                Ok(())
            })?;

            Self::deposit_event(Event::ShippingAddressUpdated { order_id });
            Ok(())
        }

        /// 买家延长确认收货期限（仅 Shipped 状态，限延一次）
        ///
        /// 在 ExpiryQueue 中追加一条新的超时条目
        #[pallet::call_index(16)]
        #[pallet::weight(T::WeightInfo::extend_confirm_timeout())]
        pub fn extend_confirm_timeout(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let new_deadline = now.saturating_add(T::ConfirmExtension::get());

            let shipped_at = Orders::<T>::try_mutate(
                order_id,
                |maybe_order| -> Result<Option<BlockNumberFor<T>>, DispatchError> {
                    let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                    ensure!(order.buyer == who, Error::<T>::NotOrderBuyer);
                    ensure!(
                        !Self::is_service_like(&order.product_category),
                        Error::<T>::ServiceLikeOrderCannotShip
                    );
                    ensure!(
                        order.status == OrderStatus::Shipped,
                        Error::<T>::InvalidOrderStatus
                    );
                    ensure!(!order.confirm_extended, Error::<T>::AlreadyExtended);

                    let sa = order.shipped_at;
                    order.confirm_extended = true;
                    Ok(sa)
                },
            )?;

            // H4: 清理 ship_order 创建的旧 ConfirmTimeout 条目
            if let Some(sa) = shipped_at {
                let old_expiry = sa.saturating_add(T::ConfirmTimeout::get());
                ExpiryQueue::<T>::mutate(old_expiry, |ids| {
                    ids.retain(|&id| id != order_id);
                });
            }

            ExpiryQueue::<T>::try_mutate(new_deadline, |ids| {
                ids.try_push(order_id)
                    .map_err(|_| Error::<T>::ExpiryQueueFull)
            })?;

            Self::deposit_event(Event::ConfirmTimeoutExtended {
                order_id,
                new_deadline,
            });
            Ok(())
        }

        /// 清理店铺订单索引（移除已终态的订单 ID，释放 BoundedVec 容量）
        ///
        /// 仅店铺 owner 可调用
        #[pallet::call_index(17)]
        #[pallet::weight(T::WeightInfo::cleanup_shop_orders())]
        pub fn cleanup_shop_orders(origin: OriginFor<T>, shop_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let owner = T::ShopProvider::shop_owner(shop_id).ok_or(Error::<T>::ShopNotFound)?;
            ensure!(owner == who, Error::<T>::NotShopOwner);

            let orders = ShopOrders::<T>::get(shop_id);
            let before = orders.len() as u32;

            let retained: Vec<u64> = orders
                .iter()
                .copied()
                .filter(|&oid| {
                    Orders::<T>::get(oid)
                        .map(|o| {
                            !matches!(
                                o.status,
                                OrderStatus::Completed
                                    | OrderStatus::Cancelled
                                    | OrderStatus::Refunded
                            )
                        })
                        .unwrap_or(false)
                })
                .collect();

            let after = retained.len() as u32;
            let removed = before.saturating_sub(after);
            ensure!(removed > 0, Error::<T>::NothingToClean);

            let bounded: BoundedVec<u64, T::MaxShopOrders> = retained
                .try_into()
                .expect("retained is subset of original bounded vec");
            ShopOrders::<T>::insert(shop_id, bounded);

            Self::deposit_event(Event::ShopOrdersCleaned { shop_id, removed });
            Ok(())
        }

        /// 卖家更新物流信息（仅 Shipped 状态）
        ///
        /// 允许卖家在发货后修改/更新物流追踪 CID（如更换快递单号）
        #[pallet::call_index(18)]
        #[pallet::weight(T::WeightInfo::update_tracking())]
        pub fn update_tracking(
            origin: OriginFor<T>,
            order_id: u64,
            new_tracking_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            ensure!(!new_tracking_cid.is_empty(), Error::<T>::EmptyTrackingCid);
            let bounded_cid: BoundedVec<u8, T::MaxCidLength> = new_tracking_cid
                .try_into()
                .map_err(|_| Error::<T>::CidTooLong)?;

            Orders::<T>::try_mutate(order_id, |maybe_order| -> DispatchResult {
                let order = maybe_order.as_mut().ok_or(Error::<T>::OrderNotFound)?;
                ensure!(order.seller == who, Error::<T>::NotOrderSeller);
                ensure!(
                    order.status == OrderStatus::Shipped,
                    Error::<T>::InvalidOrderStatus
                );

                order.tracking_cid = Some(bounded_cid);
                Ok(())
            })?;

            Self::deposit_event(Event::TrackingInfoUpdated { order_id });
            Ok(())
        }

        /// 卖家主动退款（Shipped 状态，含发货后发现问题等场景）
        #[pallet::call_index(19)]
        #[pallet::weight(T::WeightInfo::seller_refund_order())]
        pub fn seller_refund_order(
            origin: OriginFor<T>,
            order_id: u64,
            reason_cid: Vec<u8>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let bounded_reason = Self::validate_reason_cid(reason_cid)?;

            let order = Orders::<T>::get(order_id).ok_or(Error::<T>::OrderNotFound)?;
            ensure!(order.seller == who, Error::<T>::NotOrderSeller);
            ensure!(
                order.product_category != ProductCategory::Digital,
                Error::<T>::DigitalProductCannotCancel
            );
            ensure!(
                order.status == OrderStatus::Shipped,
                Error::<T>::InvalidOrderStatus
            );

            Self::do_cancel_or_refund(&order, order_id, OrderStatus::Refunded)?;

            Self::deposit_event(Event::OrderSellerRefunded {
                order_id,
                amount: order.total_amount,
                token_amount: order.token_payment_amount,
                reason_cid: bounded_reason.into_inner(),
            });
            Ok(())
        }

        /// 管理员部分退款（Root，仅 NEX 订单）
        ///
        /// refund_bps: 退给买家的比例（基点，1-9999），剩余归卖家
        #[pallet::call_index(20)]
        #[pallet::weight(T::WeightInfo::force_partial_refund())]
        pub fn force_partial_refund(
            origin: OriginFor<T>,
            order_id: u64,
            refund_bps: u16,
            reason_cid: Option<Vec<u8>>,
        ) -> DispatchResult {
            ensure_root(origin)?;
            Self::do_force_partial_refund(order_id, refund_bps, reason_cid)
        }

        /// 买家撤回争议（仅卖家尚未拒绝时可用）
        ///
        /// 恢复订单到争议前状态（Paid / Shipped），重建相应超时队列
        #[pallet::call_index(21)]
        #[pallet::weight(T::WeightInfo::withdraw_dispute())]
        pub fn withdraw_dispute(origin: OriginFor<T>, order_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::do_withdraw_dispute(who, order_id)
        }

        /// 管理员手动处理指定区块的过期订单（解决 C1 孤立条目问题）
        ///
        /// 当 on_idle weight 不足导致某区块的超时订单未被完全处理时，
        /// Root 可调用此接口指定区块号进行补偿处理。
        #[pallet::call_index(22)]
        #[pallet::weight(T::WeightInfo::force_process_expirations())]
        pub fn force_process_expirations(
            origin: OriginFor<T>,
            target_block: BlockNumberFor<T>,
        ) -> DispatchResult {
            ensure_root(origin)?;

            let now = <frame_system::Pallet<T>>::block_number();
            let weight = Self::process_expired_orders(now, target_block, 500);
            let _ = weight;

            let remaining = ExpiryQueue::<T>::get(target_block).len() as u32;

            Self::deposit_event(Event::StaleExpirationsProcessed {
                target_block,
                processed: 500u32.saturating_sub(remaining),
            });
            Ok(())
        }

        /// 代付下单：payer 签名为 buyer 付款
        #[pallet::call_index(23)]
        #[pallet::weight(T::WeightInfo::place_order_for())]
        pub fn place_order_for(
            origin: OriginFor<T>,
            buyer: T::AccountId,
            product_id: u64,
            quantity: u32,
            shipping_cid: Option<Vec<u8>>,
            use_tokens: Option<BalanceOf<T>>,
            payment_asset: Option<PaymentAsset>,
            note_cid: Option<Vec<u8>>,
            referrer: Option<T::AccountId>,
            max_nex_amount: Option<BalanceOf<T>>,
            max_token_amount: Option<u128>,
        ) -> DispatchResult {
            let payer = ensure_signed(origin)?;
            Self::do_place_order(
                buyer,
                Some(payer),
                product_id,
                quantity,
                shipping_cid,
                use_tokens,
                payment_asset,
                note_cid,
                referrer,
                max_nex_amount,
                max_token_amount,
            )
        }

        /// 清理代付人订单索引（移除已终态的订单 ID，释放 BoundedVec 容量）
        #[pallet::call_index(24)]
        #[pallet::weight(T::WeightInfo::cleanup_payer_orders())]
        pub fn cleanup_payer_orders(origin: OriginFor<T>) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let orders = PayerOrders::<T>::get(&who);
            let before = orders.len() as u32;

            let retained: Vec<u64> = orders
                .iter()
                .copied()
                .filter(|&oid| {
                    Orders::<T>::get(oid)
                        .map(|o| {
                            !matches!(
                                o.status,
                                OrderStatus::Completed
                                    | OrderStatus::Cancelled
                                    | OrderStatus::Refunded
                            )
                        })
                        .unwrap_or(false)
                })
                .collect();

            let after = retained.len() as u32;
            let removed = before.saturating_sub(after);
            ensure!(removed > 0, Error::<T>::NothingToClean);

            let bounded: BoundedVec<u64, T::MaxPayerOrders> = retained
                .try_into()
                .expect("retained is subset of original bounded vec");
            PayerOrders::<T>::insert(&who, bounded);

            Self::deposit_event(Event::PayerOrdersCleaned {
                payer: who,
                removed,
            });
            Ok(())
        }
    }

    // ==================== 内部函数 ====================

    impl<T: Config> Pallet<T> {
        /// 获取实际资金账户：有代付人用代付人，否则用买家
        pub(crate) fn fund_account(order: &OrderOf<T>) -> &T::AccountId {
            order.payer.as_ref().unwrap_or(&order.buyer)
        }

        /// 是否为订单参与者（buyer 或 payer）
        pub(crate) fn is_order_participant(order: &OrderOf<T>, who: &T::AccountId) -> bool {
            order.buyer == *who || order.payer.as_ref() == Some(who)
        }

        /// 供 AutoRepurchasePort bridge 使用的内部入口
        ///
        /// 固定参数：ShoppingBalance 通道，quantity=1，无 shipping/referrer/note/slippage。
        /// 失败时返回 Err，调用方（commission/core）降级为发 RepurchaseReady 事件。
        pub fn do_place_order_for_auto_repurchase(
            buyer: T::AccountId,
            product_id: u64,
        ) -> Result<u64, sp_runtime::DispatchError> {
            let order_id = NextOrderId::<T>::get();
            Self::do_place_order(
                buyer,
                None, // payer
                product_id,
                1,                                   // quantity
                None,                                // shipping_cid
                None,                                // use_tokens
                Some(PaymentAsset::ShoppingBalance), // payment_asset
                None,                                // note_cid
                None,                                // referrer
                None,                                // max_nex_amount
                None,                                // max_token_amount
            )?;
            Ok(order_id)
        }

        /// Cross-chain Digital order entry (HB-ENT-01), callable only by the
        /// bridge layer. Wraps [`do_place_order`] with a fixed Native payment
        /// channel and a bridge-funded `payer`, and restricts the target to
        /// `ProductCategory::Digital` + `ProductVisibility::Public` so the order
        /// settles instantly with no buyer-confirmation / dispute lifecycle.
        ///
        /// 跨链 Digital 下单入口（HB-ENT-01），仅供桥层调用。以固定的 Native 支付通道与
        /// 由桥出资的 `payer` 包装 [`do_place_order`]，并限定目标为
        /// `ProductCategory::Digital` + `ProductVisibility::Public`，使订单即时完成、
        /// 无需买家确认 / 争议生命周期。
        pub fn do_cross_order(
            buyer: T::AccountId,
            payer: T::AccountId,
            product_id: u64,
            quantity: u32,
            max_nex_amount: BalanceOf<T>,
            referrer: Option<T::AccountId>,
        ) -> Result<u64, sp_runtime::DispatchError> {
            let product_info = T::ProductProvider::get_product_info(product_id)
                .ok_or(Error::<T>::ProductNotFound)?;
            ensure!(
                product_info.category == ProductCategory::Digital,
                Error::<T>::CrossOrderDigitalOnly
            );
            ensure!(
                product_info.visibility == ProductVisibility::Public,
                Error::<T>::CrossOrderPublicOnly
            );
            let order_id = NextOrderId::<T>::get();
            Self::do_place_order(
                buyer,
                Some(payer),
                product_id,
                quantity,
                None,                       // shipping_cid
                None,                       // use_tokens
                Some(PaymentAsset::Native), // payment_asset
                None,                       // note_cid
                referrer,
                Some(max_nex_amount), // max_nex_amount (slippage cap)
                None,                 // max_token_amount
            )?;
            Ok(order_id)
        }

        /// 下单核心逻辑（place_order 和 place_order_for 共用）
        fn do_place_order(
            buyer: T::AccountId,
            payer: Option<T::AccountId>,
            product_id: u64,
            quantity: u32,
            shipping_cid: Option<Vec<u8>>,
            use_tokens: Option<BalanceOf<T>>,
            payment_asset: Option<PaymentAsset>,
            note_cid: Option<Vec<u8>>,
            referrer: Option<T::AccountId>,
            max_nex_amount: Option<BalanceOf<T>>,
            max_token_amount: Option<u128>,
        ) -> DispatchResult {
            ensure!(quantity > 0, Error::<T>::InvalidQuantity);

            // actual_payer: 实际出资账户
            let actual_payer = payer.as_ref().unwrap_or(&buyer);

            let product_info = T::ProductProvider::get_product_info(product_id)
                .ok_or(Error::<T>::ProductNotFound)?;
            ensure!(
                product_info.status == ProductStatus::OnSale,
                Error::<T>::ProductNotOnSale
            );
            let shop_id = product_info.shop_id;

            // ① USDT 价格必须已设置
            let usdt_price = product_info.usdt_price;
            ensure!(usdt_price > 0, Error::<T>::UsdtPriceNotSet);

            if product_info.min_order_quantity > 0 {
                ensure!(
                    quantity >= product_info.min_order_quantity,
                    Error::<T>::QuantityBelowMinimum
                );
            }
            if product_info.max_order_quantity > 0 {
                ensure!(
                    quantity <= product_info.max_order_quantity,
                    Error::<T>::QuantityAboveMaximum
                );
            }

            ensure!(
                T::ShopProvider::shop_exists(shop_id),
                Error::<T>::ShopNotFound
            );
            ensure!(
                T::ShopProvider::is_shop_active(shop_id),
                Error::<T>::ShopInactive
            );
            let seller = T::ShopProvider::shop_owner(shop_id).ok_or(Error::<T>::ShopNotFound)?;
            ensure!(seller != buyer, Error::<T>::CannotBuyOwnProduct);
            // 代付人不能是卖家
            ensure!(*actual_payer != seller, Error::<T>::PayerCannotBeSeller);

            let entity_id =
                T::ShopProvider::shop_entity_id(shop_id).ok_or(Error::<T>::ShopNotFound)?;

            if let Some(ref r) = referrer {
                ensure!(*r != buyer, Error::<T>::InvalidReferrer);
                ensure!(*r != seller, Error::<T>::InvalidReferrer);
                ensure!(
                    T::MemberProvider::is_member(entity_id, r),
                    Error::<T>::InvalidReferrer
                );
            }

            ensure!(
                !T::MemberProvider::is_banned(entity_id, &buyer),
                Error::<T>::BuyerBanned
            );

            // REFERRAL_REQUIRED 策略：无论是否已注册会员，只要没有推荐人且未传入 referrer 就拒绝下单
            if T::MemberProvider::requires_referral(entity_id)
                && T::MemberProvider::get_referrer(entity_id, &buyer).is_none()
                && referrer.is_none()
            {
                return Err(Error::<T>::ReferrerRequired.into());
            }

            // 预注册：非会员买家购买会员专属商品时，尝试自动注册
            // - 仅限 MembersOnly / LevelGated 商品（Public 无需）
            // - 需要 referrer（传入参数或已有记录）
            // - auto_register 幂等，已是会员则 no-op
            // - 失败时忽略结果（让后续可见性检查自然拒绝）
            // - Substrate with_storage_layer 保证 place_order 整体失败时自动回滚
            if !T::MemberProvider::is_member(entity_id, &buyer)
                && matches!(
                    product_info.visibility,
                    ProductVisibility::MembersOnly | ProductVisibility::LevelGated(_)
                )
            {
                let effective_referrer = referrer
                    .clone()
                    .or_else(|| T::MemberProvider::get_referrer(entity_id, &buyer));
                if effective_referrer.is_some() {
                    let _ = T::MemberProvider::auto_register(entity_id, &buyer, effective_referrer);
                }
            }

            let buyer_level = T::MemberProvider::get_effective_level(entity_id, &buyer);

            match product_info.visibility {
                ProductVisibility::Public => {}
                ProductVisibility::MembersOnly => {
                    ensure!(
                        T::MemberProvider::is_member(entity_id, &buyer),
                        Error::<T>::ProductMembersOnly
                    );
                }
                ProductVisibility::LevelGated(required_level) => {
                    ensure!(
                        T::MemberProvider::is_member(entity_id, &buyer),
                        Error::<T>::ProductMembersOnly
                    );
                    ensure!(
                        buyer_level >= required_level,
                        Error::<T>::MemberLevelInsufficient
                    );
                }
            }

            if product_info.stock > 0 {
                ensure!(
                    product_info.stock >= quantity,
                    Error::<T>::InsufficientStock
                );
            }

            // ② USDT 总价 = usdt_price * quantity
            let usdt_total = usdt_price
                .checked_mul(quantity as u64)
                .ok_or(Error::<T>::Overflow)?;

            // ③ 获取 NEX/USDT 汇率（精度 10^6）
            let nex_usdt_price = T::PricingProvider::get_nex_usdt_price();
            ensure!(nex_usdt_price > 0, Error::<T>::NexPriceUnavailable);
            ensure!(
                !T::PricingProvider::is_price_stale(),
                Error::<T>::NexPriceUnavailable
            );

            // ④ 会员折扣在 USDT 层
            let mut discounted_usdt = usdt_total;
            if buyer_level > 0 {
                let discount_bps: u32 =
                    T::MemberProvider::get_level_discount(entity_id, buyer_level).into();
                if discount_bps > 0 && discount_bps < 10000 {
                    let discount =
                        (discounted_usdt as u128).saturating_mul(discount_bps as u128) / 10000u128;
                    discounted_usdt = discounted_usdt.saturating_sub(discount as u64);
                }
            }

            // ⑤ USDT → NEX: nex_amount = discounted_usdt * 10^12 / nex_usdt_price
            // (discounted_usdt 精度 10^6, nex_usdt_price 精度 10^6, NEX 精度 10^12)
            let nex_amount_u128 = (discounted_usdt as u128)
                .checked_mul(1_000_000_000_000u128) // 10^12
                .ok_or(Error::<T>::Overflow)?
                .checked_div(nex_usdt_price as u128)
                .ok_or(Error::<T>::Overflow)?;
            let nex_amount: BalanceOf<T> = nex_amount_u128
                .try_into()
                .map_err(|_| Error::<T>::Overflow)?;

            // ⑥ 检查 PaymentConfig 通道是否开启
            let resolved_payment_asset = payment_asset.unwrap_or(PaymentAsset::Native);
            let payment_config = T::EntityProvider::payment_config(entity_id);
            match resolved_payment_asset {
                PaymentAsset::Native => {
                    ensure!(
                        payment_config.native_enabled,
                        Error::<T>::PaymentChannelDisabled
                    );
                }
                PaymentAsset::EntityToken => {
                    ensure!(
                        payment_config.token_enabled,
                        Error::<T>::PaymentChannelDisabled
                    );
                }
                PaymentAsset::ShoppingBalance => {
                    // 购物余额通道：需要 Native 通道开启（购物余额基于 NEX 计价）
                    ensure!(
                        payment_config.native_enabled,
                        Error::<T>::PaymentChannelDisabled
                    );
                    // 购物余额支付不支持代付
                    ensure!(
                        payer.is_none() || payer.as_ref() == Some(&buyer),
                        Error::<T>::PayerCannotUseBuyerLoyalty
                    );
                }
            }

            // NEX 单价快照
            let unit_price_nex: BalanceOf<T> = if quantity > 0 {
                nex_amount / quantity.into()
            } else {
                Zero::zero()
            };

            let mut final_amount = nex_amount;

            // BUG-2 guard: 代付人不能消费买家的积分
            if payer.is_some() && payer.as_ref() != Some(&buyer) {
                if let Some(ref t) = use_tokens {
                    ensure!(t.is_zero(), Error::<T>::PayerCannotUseBuyerLoyalty);
                }
            }

            // 忠诚度消费追踪（退款时回滚）
            let mut shopping_bal_used = BalanceOf::<T>::zero();
            let mut token_discount_burned: u128 = 0u128;

            // ⑦ 积分抵扣（仅 Native，基于 buyer 的权益）
            if resolved_payment_asset == PaymentAsset::Native {
                if let Some(tokens) = use_tokens {
                    if !tokens.is_zero() && T::Loyalty::is_token_enabled(entity_id) {
                        let discount = T::Loyalty::redeem_for_discount(entity_id, &buyer, tokens)?;
                        final_amount = final_amount.saturating_sub(discount);
                        token_discount_burned = tokens.saturated_into::<u128>();
                    }
                }
            }

            // ⑦b 购物余额全额支付（ShoppingBalance 通道）
            if resolved_payment_asset == PaymentAsset::ShoppingBalance {
                let available = T::Loyalty::shopping_balance(entity_id, &buyer);
                ensure!(
                    available >= nex_amount,
                    Error::<T>::InsufficientShoppingBalance
                );
                T::Loyalty::consume_shopping_balance(entity_id, &buyer, nex_amount)?;
                shopping_bal_used = nex_amount;
                final_amount = Zero::zero();
            }

            // final_amount 校验仅 Native 时要求非零
            if resolved_payment_asset == PaymentAsset::Native {
                ensure!(!final_amount.is_zero(), Error::<T>::InvalidAmount);
            }

            let platform_fee = match resolved_payment_asset {
                PaymentAsset::Native => {
                    final_amount.saturating_mul(PlatformFeeRate::<T>::get().into())
                        / 10000u32.into()
                }
                PaymentAsset::EntityToken | PaymentAsset::ShoppingBalance => Zero::zero(),
            };

            let shipping_cid: Option<BoundedVec<u8, T::MaxCidLength>> = shipping_cid
                .map(|c| c.try_into().map_err(|_| Error::<T>::CidTooLong))
                .transpose()?;

            let bounded_note_cid: Option<BoundedVec<u8, T::MaxCidLength>> = note_cid
                .map(|c| c.try_into().map_err(|_| Error::<T>::CidTooLong))
                .transpose()?;

            let product_category = product_info.category;

            ensure!(
                product_category != ProductCategory::Subscription,
                Error::<T>::SubscriptionNotSupported
            );

            let requires_shipping = Self::category_requires_shipping(&product_category);

            if requires_shipping {
                ensure!(shipping_cid.is_some(), Error::<T>::ShippingCidRequired);
            }

            let order_id = NextOrderId::<T>::get();
            let now = <frame_system::Pallet<T>>::block_number();

            // ⑧ 资金锁定 + 滑点检查
            let token_payment_amount: u128;
            let token_nex_rate: u128;

            match resolved_payment_asset {
                PaymentAsset::Native => {
                    // 滑点检查
                    if let Some(max_nex) = max_nex_amount {
                        ensure!(final_amount <= max_nex, Error::<T>::NexSlippageExceeded);
                    }
                    T::Escrow::lock_from(actual_payer, order_id, final_amount)?;
                    token_payment_amount = 0u128;
                    token_nex_rate = 0u128;
                }
                PaymentAsset::ShoppingBalance => {
                    // 购物余额全额支付：无 Escrow 锁定，资金在 Entity 内部
                    token_payment_amount = 0u128;
                    token_nex_rate = 0u128;
                }
                PaymentAsset::EntityToken => {
                    ensure!(
                        T::EntityToken::is_token_enabled(entity_id),
                        Error::<T>::EntityTokenNotEnabled
                    );

                    // Token/NEX 汇率
                    let token_nex_price = T::TokenPriceProvider::get_token_price(entity_id)
                        .ok_or(Error::<T>::TokenPriceUnavailable)?;
                    let token_nex_price_u128: u128 = token_nex_price.saturated_into();
                    ensure!(token_nex_price_u128 > 0, Error::<T>::TokenPriceUnavailable);
                    ensure!(
                        T::TokenPriceProvider::is_token_price_reliable(entity_id),
                        Error::<T>::TokenPriceUnavailable
                    );

                    // token_amount = nex_amount * 10^12 / token_nex_price
                    let token_amount_u128 = (final_amount.saturated_into::<u128>())
                        .checked_mul(1_000_000_000_000u128)
                        .ok_or(Error::<T>::Overflow)?
                        .checked_div(token_nex_price_u128)
                        .ok_or(Error::<T>::Overflow)?;
                    let token_amount: BalanceOf<T> = token_amount_u128
                        .try_into()
                        .map_err(|_| Error::<T>::Overflow)?;

                    // 滑点检查
                    if let Some(max_token) = max_token_amount {
                        ensure!(
                            token_amount_u128 <= max_token,
                            Error::<T>::TokenSlippageExceeded
                        );
                    }

                    let payer_token_balance =
                        T::EntityToken::token_balance(entity_id, actual_payer);
                    ensure!(
                        payer_token_balance >= token_amount,
                        Error::<T>::InsufficientTokenBalance
                    );
                    T::EntityToken::reserve(entity_id, actual_payer, token_amount)?;
                    token_payment_amount = token_amount_u128;
                    token_nex_rate = token_nex_price_u128;
                }
            }

            T::ProductProvider::deduct_stock(product_id, quantity)?;
            T::ProductProvider::add_sold_count(product_id, quantity)?;

            let initial_status = if product_category == ProductCategory::Digital {
                OrderStatus::Completed
            } else {
                OrderStatus::Paid
            };

            // payer==buyer 退化为 None
            let stored_payer = payer.filter(|p| *p != buyer);

            let order = Order {
                id: order_id,
                entity_id,
                shop_id,
                product_id,
                buyer: buyer.clone(),
                seller: seller.clone(),
                payer: stored_payer.clone(),
                quantity,
                unit_price: unit_price_nex,
                total_amount: final_amount,
                platform_fee,
                usdt_total: discounted_usdt,
                nex_usdt_rate: nex_usdt_price,
                token_nex_rate,
                product_category,
                shipping_cid,
                tracking_cid: None,
                status: initial_status,
                created_at: now,
                shipped_at: None,
                completed_at: if product_category == ProductCategory::Digital {
                    Some(now)
                } else {
                    None
                },
                service_started_at: None,
                service_completed_at: None,
                payment_asset: resolved_payment_asset,
                token_payment_amount,
                confirm_extended: false,
                dispute_rejected: false,
                dispute_deadline: None,
                note_cid: bounded_note_cid,
                refund_reason_cid: None,
                shopping_balance_used: shopping_bal_used,
                token_discount_tokens_burned: token_discount_burned,
            };

            Orders::<T>::insert(order_id, &order);
            BuyerOrders::<T>::try_mutate(&buyer, |ids| ids.try_push(order_id))
                .map_err(|_| Error::<T>::Overflow)?;
            ShopOrders::<T>::try_mutate(shop_id, |ids| ids.try_push(order_id))
                .map_err(|_| Error::<T>::Overflow)?;

            // 代付人订单索引
            if let Some(ref p) = stored_payer {
                PayerOrders::<T>::try_mutate(p, |ids| ids.try_push(order_id))
                    .map_err(|_| Error::<T>::PayerOrdersFull)?;
            }

            let next_id = order_id.checked_add(1).ok_or(Error::<T>::Overflow)?;
            NextOrderId::<T>::put(next_id);

            if product_category != ProductCategory::Digital {
                let expiry_block = now.saturating_add(T::ShipTimeout::get());
                ExpiryQueue::<T>::try_mutate(expiry_block, |ids| {
                    ids.try_push(order_id)
                        .map_err(|_| Error::<T>::ExpiryQueueFull)
                })?;
            }

            if let Some(ref r) = referrer {
                OrderReferrer::<T>::insert(order_id, r);
            }

            OrderStats::<T>::mutate(|stats| {
                stats.total_orders = stats.total_orders.saturating_add(1);
            });

            Self::deposit_event(Event::OrderCreated {
                order_id,
                entity_id,
                buyer: buyer.clone(),
                seller: seller.clone(),
                payer: stored_payer.clone(),
                amount: final_amount,
                payment_asset: resolved_payment_asset,
                token_amount: token_payment_amount,
                usdt_total: discounted_usdt,
                nex_usdt_rate: nex_usdt_price,
            });

            if product_category == ProductCategory::Digital {
                // 数字商品即时完成，无存续期，不开聊天（避免每笔 grant+revoke 无谓写入）。
                // Digital orders complete instantly (no lifetime), so no chat is opened.
                Self::do_complete_order(order_id, &order)?;
            } else {
                // 非即时订单：为买卖双方开通订单存续期内的双向聊天（尽力而为，失败不影响下单）。
                // Non-instant order: open buyer↔seller chat for the order's lifetime
                // (best-effort; a failure must not affect order placement).
                T::Chat::grant(order_id, &buyer, &seller);
            }

            Ok(())
        }

        /// 商品类别是否需要物流
        fn category_requires_shipping(cat: &ProductCategory) -> bool {
            matches!(
                cat,
                ProductCategory::Physical | ProductCategory::Bundle | ProductCategory::Other
            )
        }

        /// 构造系统通知描述符：`prefix + 十进制 order_id`（如 `order:shipped:1234`）。
        /// 这是客户端本地化用的不透明模板描述符，非 IPFS CID（见 chat-core `SystemNotifier`）。
        /// Build a notice descriptor `prefix + decimal order_id`; an opaque,
        /// client-localized template token (NOT an IPFS CID).
        pub(crate) fn notice(prefix: &[u8], order_id: u64) -> Vec<u8> {
            let mut v = prefix.to_vec();
            v.extend_from_slice(Self::u64_ascii(order_id).as_slice());
            v
        }

        /// `u64` 转十进制 ASCII 字节（no_std 友好，避免依赖 `to_string`）。
        /// `u64` → decimal ASCII bytes (no_std friendly).
        fn u64_ascii(mut n: u64) -> Vec<u8> {
            if n == 0 {
                return alloc::vec![b'0'];
            }
            let mut buf = Vec::new();
            while n > 0 {
                buf.push(b'0' + (n % 10) as u8);
                n /= 10;
            }
            buf.reverse();
            buf
        }

        /// 是否为服务类/订阅类（共享 start_service/complete_service/confirm_service 流程）
        pub(crate) fn is_service_like(cat: &ProductCategory) -> bool {
            matches!(
                cat,
                ProductCategory::Service | ProductCategory::Subscription
            )
        }

        pub(crate) fn validate_reason_cid(
            cid: Vec<u8>,
        ) -> Result<BoundedVec<u8, T::MaxCidLength>, DispatchError> {
            ensure!(!cid.is_empty(), Error::<T>::EmptyReasonCid);
            let bounded: BoundedVec<u8, T::MaxCidLength> =
                cid.try_into().map_err(|_| Error::<T>::CidTooLong)?;
            Ok(bounded)
        }

        pub(crate) fn validate_optional_reason_cid(
            cid: &Option<Vec<u8>>,
        ) -> Result<Option<Vec<u8>>, DispatchError> {
            if let Some(c) = cid {
                ensure!(!c.is_empty(), Error::<T>::EmptyReasonCid);
                let _: BoundedVec<u8, T::MaxCidLength> =
                    c.clone().try_into().map_err(|_| Error::<T>::CidTooLong)?;
            }
            Ok(cid.clone())
        }

        /// 回滚下单时消费的忠诚度资产（购物余额 + Token 折扣）
        /// 最佳努力模式：失败时发事件，不阻塞退款流程
        pub(crate) fn rollback_loyalty(order: &OrderOf<T>, order_id: u64) {
            if !order.shopping_balance_used.is_zero() {
                if T::Loyalty::rollback_shopping_balance(
                    order.entity_id,
                    &order.buyer,
                    order.shopping_balance_used,
                )
                .is_err()
                {
                    Self::deposit_event(Event::OrderOperationFailed {
                        order_id,
                        operation: OrderOperation::LoyaltyRollback,
                    });
                }
            }
            if order.token_discount_tokens_burned > 0 {
                let tokens: BalanceOf<T> = order.token_discount_tokens_burned.saturated_into();
                if T::Loyalty::rollback_token_discount(order.entity_id, &order.buyer, tokens)
                    .is_err()
                {
                    Self::deposit_event(Event::OrderOperationFailed {
                        order_id,
                        operation: OrderOperation::LoyaltyRollback,
                    });
                }
            }
        }

        pub(crate) fn do_cancel_or_refund(
            order: &OrderOf<T>,
            order_id: u64,
            final_status: OrderStatus,
        ) -> DispatchResult {
            Self::rollback_loyalty(order, order_id);
            Self::refund_by_asset(order, order_id)?;
            if T::ProductProvider::restore_stock(order.product_id, order.quantity).is_err() {
                Self::deposit_event(Event::OrderOperationFailed {
                    order_id,
                    operation: OrderOperation::StockRestore,
                });
            }
            Self::notify_order_cancelled(order, order_id);
            // 终态撤销订单场景聊天授权（尽力而为；未授予过则为 no-op）。
            // Revoke the order's chat authorization at terminal state (best-effort;
            // a no-op if it was never granted, e.g. digital/instant orders).
            T::Chat::revoke(order_id, &order.buyer, &order.seller);
            Orders::<T>::mutate(order_id, |maybe_order| {
                if let Some(o) = maybe_order {
                    o.status = final_status;
                }
            });
            OrderReferrer::<T>::remove(order_id);
            Ok(())
        }

        pub(crate) fn do_complete_order(order_id: u64, order: &OrderOf<T>) -> DispatchResult {
            let seller_amount = order.total_amount.saturating_sub(order.platform_fee);
            let entity_id = order.entity_id;
            let fund_acct = Self::fund_account(order);

            let token_platform_fee: u128 = match order.payment_asset {
                PaymentAsset::Native | PaymentAsset::ShoppingBalance => 0u128,
                PaymentAsset::EntityToken => {
                    let ta: u128 = order.token_payment_amount;
                    let tfr = T::TokenFeeConfig::token_platform_fee_rate(entity_id) as u128;
                    // M3-R8: 防御性上限 — 费率不超过 10000 bps (100%)，防止外部错误配置导致卖家收入为 0
                    let safe_rate = tfr.min(10000u128);
                    ta.saturating_mul(safe_rate) / 10000u128
                }
            };

            // P0-5 审计修复: 跟踪 Token 平台费实际到账状态
            let mut token_fee_paid = true;
            // Reserve 模式: 记录 seller 锁定的佣金资金
            let mut nex_reserved = BalanceOf::<T>::zero();

            match order.payment_asset {
                PaymentAsset::Native => {
                    // NEX 支付：从托管释放资金给卖家
                    T::Escrow::transfer_from_escrow(order_id, &order.seller, seller_amount)?;
                    // 平台费转给平台账户
                    if !order.platform_fee.is_zero() {
                        T::Escrow::transfer_from_escrow(
                            order_id,
                            &T::PlatformAccount::get(),
                            order.platform_fee,
                        )?;
                    }
                    // ── Reserve 模式: 锁定佣金资金 ──
                    // seller_amount 是 commission engine 最大可能从 seller 扣除的金额。
                    // reserve 需保留 ED，避免账户被回收。
                    let seller_free = T::Currency::free_balance(&order.seller);
                    let min_balance = T::Currency::minimum_balance();
                    let max_reservable = seller_free.saturating_sub(min_balance);
                    let reserve_target = seller_amount.min(max_reservable);
                    if !reserve_target.is_zero() {
                        T::Currency::reserve(&order.seller, reserve_target)?;
                    }
                    nex_reserved = reserve_target;
                }
                PaymentAsset::ShoppingBalance => {
                    // 购物余额支付：无 Escrow 结算，无卖家收款
                    // 资金始终在 Entity 内部，分佣由 commission engine 纯记账处理
                }
                PaymentAsset::EntityToken => {
                    // Token 支付：使用预先计算的平台费拆分转账
                    let token_amount: u128 = order.token_payment_amount;
                    let token_seller_amount = token_amount.saturating_sub(token_platform_fee);

                    // 卖家获得扣除平台费后的金额
                    let seller_token: BalanceOf<T> = token_seller_amount.saturated_into();
                    T::EntityToken::repatriate_reserved(
                        entity_id,
                        fund_acct,
                        &order.seller,
                        seller_token,
                    )?;

                    // M1-fix: 平台费转入 entity_account，失败时发事件而非静默吞错
                    // P0-5 审计修复: 跟踪实际到账状态，传递给 Hook 链
                    if token_platform_fee > 0 {
                        let entity_account = T::TokenFeeConfig::entity_account(entity_id);
                        let fee_token: BalanceOf<T> = token_platform_fee.saturated_into();
                        if T::EntityToken::repatriate_reserved(
                            entity_id,
                            fund_acct,
                            &entity_account,
                            fee_token,
                        )
                        .is_err()
                        {
                            token_fee_paid = false;
                            Self::deposit_event(Event::OrderOperationFailed {
                                order_id,
                                operation: OrderOperation::TokenPlatformFee,
                            });
                        }
                    }
                }
            }

            let now = <frame_system::Pallet<T>>::block_number();

            Orders::<T>::mutate(order_id, |maybe_order| {
                if let Some(o) = maybe_order {
                    o.status = OrderStatus::Completed;
                    o.completed_at = Some(now);
                }
            });

            // Phase 5.3: 预计算 Hook 所需数据，然后委托给 Hook 链
            let amount_usdt = Self::calculate_amount_usdt(order);
            let referrer = OrderReferrer::<T>::take(order_id);

            let token_seller_received: u128 = match order.payment_asset {
                PaymentAsset::Native | PaymentAsset::ShoppingBalance => 0u128,
                PaymentAsset::EntityToken => order
                    .token_payment_amount
                    .saturating_sub(token_platform_fee),
            };
            let nex_seller_received: BalanceOf<T> = match order.payment_asset {
                PaymentAsset::Native => seller_amount,
                PaymentAsset::EntityToken | PaymentAsset::ShoppingBalance => Zero::zero(),
            };

            let info = OrderCompletionInfo {
                order_id,
                entity_id,
                shop_id: order.shop_id,
                product_id: order.product_id,
                buyer: order.buyer.clone(),
                seller: order.seller.clone(),
                payer: order.payer.clone(),
                quantity: order.quantity,
                payment_asset: order.payment_asset,
                nex_total_amount: order.total_amount,
                nex_platform_fee: order.platform_fee,
                nex_seller_received,
                nex_reserved_for_commission: nex_reserved,
                token_payment_amount: order.token_payment_amount,
                token_platform_fee,
                token_seller_received,
                token_platform_fee_paid: token_fee_paid,
                referrer,
                amount_usdt,
                product_category: order.product_category,
                shopping_balance_used: order.shopping_balance_used,
            };
            T::OnOrderCompleted::on_completed(&info);

            // 订单完成：撤销买卖双方场景聊天授权（尽力而为；数字/即时订单未授予则 no-op）。
            // Order completed: revoke the buyer↔seller chat authorization (best-effort;
            // a no-op for digital/instant orders that never opened chat).
            T::Chat::revoke(order_id, &order.buyer, &order.seller);

            // 内部统计更新（不是外部副作用，保留在 order 内）
            OrderStats::<T>::mutate(|stats| {
                stats.completed_orders = stats.completed_orders.saturating_add(1);
                match order.payment_asset {
                    PaymentAsset::Native => {
                        stats.total_volume = stats.total_volume.saturating_add(order.total_amount);
                        stats.total_platform_fees =
                            stats.total_platform_fees.saturating_add(order.platform_fee);
                    }
                    PaymentAsset::ShoppingBalance => {
                        // 购物余额支付：计入 volume（基于 shopping_balance_used），无平台费
                        stats.total_volume = stats
                            .total_volume
                            .saturating_add(order.shopping_balance_used);
                    }
                    PaymentAsset::EntityToken => {
                        stats.total_token_volume = stats
                            .total_token_volume
                            .saturating_add(order.token_payment_amount);
                        stats.total_token_platform_fees = stats
                            .total_token_platform_fees
                            .saturating_add(token_platform_fee);
                    }
                }
            });

            Self::deposit_event(Event::OrderCompleted {
                order_id,
                seller_received: nex_seller_received,
                token_seller_received,
            });

            Ok(())
        }

        /// 预计算 USDT 等值金额（供 Hook 链使用）
        fn calculate_amount_usdt(order: &OrderOf<T>) -> u64 {
            // 新版订单已在下单时快照 usdt_total，直接返回
            if order.usdt_total > 0 {
                return order.usdt_total;
            }
            // 兼容旧订单（usdt_total == 0 的存量数据）
            match order.payment_asset {
                PaymentAsset::Native => {
                    let amount_nex: u128 = order.total_amount.saturated_into();
                    let nex_price: u128 = T::PricingProvider::get_nex_usdt_price() as u128;
                    amount_nex
                        .saturating_mul(nex_price)
                        .checked_div(1_000_000_000_000u128)
                        .unwrap_or(0) as u64
                }
                PaymentAsset::ShoppingBalance => {
                    // 购物余额基于 NEX 计价，使用 shopping_balance_used
                    let amount_nex: u128 = order.shopping_balance_used.saturated_into();
                    let nex_price: u128 = T::PricingProvider::get_nex_usdt_price() as u128;
                    amount_nex
                        .saturating_mul(nex_price)
                        .checked_div(1_000_000_000_000u128)
                        .unwrap_or(0) as u64
                }
                PaymentAsset::EntityToken => {
                    let entity_id = order.entity_id;
                    if T::TokenPriceProvider::is_token_price_reliable(entity_id) {
                        if let Some(token_nex_price) =
                            T::TokenPriceProvider::get_token_price(entity_id)
                        {
                            let nex_usdt: u128 = T::PricingProvider::get_nex_usdt_price() as u128;
                            if nex_usdt > 0 {
                                let token_nex_u128: u128 = token_nex_price.saturated_into();
                                order
                                    .token_payment_amount
                                    .checked_mul(token_nex_u128)
                                    .and_then(|v| v.checked_div(1_000_000_000_000u128))
                                    .and_then(|v| v.checked_mul(nex_usdt))
                                    .and_then(|v| v.checked_div(1_000_000_000_000u128))
                                    .unwrap_or(0) as u64
                            } else {
                                0u64
                            }
                        } else {
                            0u64
                        }
                    } else {
                        0u64
                    }
                }
            }
        }

        /// Phase 5.3: 通知订单取消 Hook（替代 cancel_commission_by_asset）
        pub(crate) fn notify_order_cancelled(order: &OrderOf<T>, order_id: u64) {
            let info = OrderCancellationInfo {
                order_id,
                entity_id: order.entity_id,
                shop_id: order.shop_id,
                payment_asset: order.payment_asset,
            };
            T::OnOrderCancelled::on_cancelled(&info);
        }

        /// 根据支付资产类型退款（Token 用 unreserve，NEX 用 Escrow refund）
        /// 返回 Ok(()) 表示成功，Err 表示 NEX escrow 退款失败
        fn refund_by_asset(order: &OrderOf<T>, order_id: u64) -> DispatchResult {
            let fund_acct = Self::fund_account(order);
            match order.payment_asset {
                PaymentAsset::Native => {
                    T::Escrow::refund_all(order_id, fund_acct)?;
                }
                PaymentAsset::ShoppingBalance => {
                    // 购物余额支付：无 Escrow，rollback_loyalty 已处理余额恢复
                }
                PaymentAsset::EntityToken => {
                    let token_refund: BalanceOf<T> = order.token_payment_amount.saturated_into();
                    T::EntityToken::unreserve(order.entity_id, fund_acct, token_refund);
                }
            }
            Ok(())
        }

        fn do_auto_refund(order: &OrderOf<T>, order_id: u64) -> bool {
            if Self::do_cancel_or_refund(order, order_id, OrderStatus::Refunded).is_ok() {
                Self::deposit_event(Event::OrderRefunded {
                    order_id,
                    amount: order.total_amount,
                    token_amount: order.token_payment_amount,
                });
                true
            } else {
                Self::deposit_event(Event::OrderOperationFailed {
                    order_id,
                    operation: OrderOperation::EscrowRefund,
                });
                false
            }
        }

        /// 处理过期订单（基于 ExpiryQueue 精确索引）
        ///
        /// now: 当前区块号（用于判断 deadline 是否到达）
        /// target_block: 要处理的 ExpiryQueue key（通常 = now，force 时可指定过去区块）
        /// 二次确认订单状态：可能已被手动确认/取消/退款
        fn process_expired_orders(
            now: BlockNumberFor<T>,
            target_block: BlockNumberFor<T>,
            max_count: u32,
        ) -> Weight {
            let order_ids = ExpiryQueue::<T>::get(target_block);
            if order_ids.is_empty() {
                // M2-R9-fix: 仅消耗 1 次 storage read，补充 proof_size
                return Weight::from_parts(5_000, 64);
            }

            let mut processed = 0u32;
            let mut iterated = 0usize;

            for &order_id in order_ids.iter() {
                if processed >= max_count {
                    // 未遍历的全部保留
                    break;
                }
                iterated = iterated.saturating_add(1);

                if let Some(order) = Orders::<T>::get(order_id) {
                    match order.status {
                        // 发货超时：自动退款（L1-R9-fix: 统一使用 do_auto_refund 消除重复代码）
                        OrderStatus::Paid => {
                            if Self::do_auto_refund(&order, order_id) {
                                processed = processed.saturating_add(1);
                            }
                        }
                        // 确认超时：自动确认收货/服务
                        OrderStatus::Shipped => {
                            if Self::is_service_like(&order.product_category)
                                && order.service_completed_at.is_none()
                            {
                                // H4+H5: 服务已开始但未完成 — 检查是否超过 ServiceConfirmTimeout
                                if let Some(started_at) = order.service_started_at {
                                    let deadline =
                                        started_at.saturating_add(T::ServiceConfirmTimeout::get());
                                    if now >= deadline {
                                        // 卖家超时未完成服务 → 自动退款
                                        if Self::do_auto_refund(&order, order_id) {
                                            processed = processed.saturating_add(1);
                                        }
                                    }
                                    // else: 服务期限内，跳过（start_service 已在正确的 deadline 区块创建了独立条目）
                                }
                                // else: service_started_at 为 None（理论上不应出现），跳过
                            } else if Self::do_complete_order(order_id, &order).is_ok() {
                                processed = processed.saturating_add(1);
                            } else {
                                Self::deposit_event(Event::OrderOperationFailed {
                                    order_id,
                                    operation: OrderOperation::AutoComplete,
                                });
                            }
                        }
                        // 争议超时：仅在 dispute_deadline 到期后自动退款
                        OrderStatus::Disputed => {
                            let deadline_reached =
                                order.dispute_deadline.map(|d| now >= d).unwrap_or(false);
                            if deadline_reached {
                                // 解除争议锁定（仅 Native 需要）
                                if order.payment_asset == PaymentAsset::Native {
                                    let _ = T::Escrow::set_resolved(order_id);
                                }
                                if Self::do_auto_refund(&order, order_id) {
                                    processed = processed.saturating_add(1);
                                }
                            }
                            // else: 非争议超时条目（如 ShipTimeout），跳过
                        }
                        // 已被手动处理（取消/退款/确认等），跳过（从队列移除）
                        _ => {}
                    }
                }
            }

            if iterated >= order_ids.len() {
                ExpiryQueue::<T>::remove(target_block);
            } else {
                let remaining: Vec<u64> = order_ids.iter().skip(iterated).copied().collect();
                let bounded: BoundedVec<u64, T::MaxExpiryQueueSize> = remaining
                    .try_into()
                    .expect("remaining is subset of original bounded vec");
                ExpiryQueue::<T>::insert(target_block, bounded);
            }

            // M1-R8: 精确报告 weight：读队列 + 每个处理订单读写 + 每个跳过订单读开销
            let skipped = (iterated as u64).saturating_sub(processed as u64);
            Weight::from_parts(
                50_000_000u64
                    .saturating_add(200_000_000u64.saturating_mul(processed as u64))
                    .saturating_add(25_000_000u64.saturating_mul(skipped)),
                4_000u64
                    .saturating_add(8_000u64.saturating_mul(processed as u64))
                    .saturating_add(2_000u64.saturating_mul(skipped)),
            )
        }
    }

    // ==================== OrderProvider 实现 ====================

    impl<T: Config> OrderProvider<T::AccountId, BalanceOf<T>> for Pallet<T> {
        fn order_exists(order_id: u64) -> bool {
            Orders::<T>::contains_key(order_id)
        }

        fn order_buyer(order_id: u64) -> Option<T::AccountId> {
            Orders::<T>::get(order_id).map(|o| o.buyer)
        }

        fn order_seller(order_id: u64) -> Option<T::AccountId> {
            Orders::<T>::get(order_id).map(|o| o.seller)
        }

        fn order_amount(order_id: u64) -> Option<BalanceOf<T>> {
            Orders::<T>::get(order_id).map(|o| o.total_amount)
        }

        fn order_shop_id(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id).map(|o| o.shop_id)
        }

        fn is_order_completed(order_id: u64) -> bool {
            Orders::<T>::get(order_id)
                .map(|o| o.status == OrderStatus::Completed)
                .unwrap_or(false)
        }

        fn is_order_disputed(order_id: u64) -> bool {
            Orders::<T>::get(order_id)
                .map(|o| o.status == OrderStatus::Disputed)
                .unwrap_or(false)
        }

        fn can_dispute(order_id: u64, who: &T::AccountId) -> bool {
            Orders::<T>::get(order_id)
                .map(|o| {
                    let is_participant = Self::is_order_participant(&o, who);
                    let status_ok = matches!(o.status, OrderStatus::Paid | OrderStatus::Shipped);
                    is_participant && status_ok
                })
                .unwrap_or(false)
        }

        fn order_token_amount(order_id: u64) -> Option<u128> {
            Orders::<T>::get(order_id).map(|o| o.token_payment_amount)
        }

        fn order_payment_asset(order_id: u64) -> Option<PaymentAsset> {
            Orders::<T>::get(order_id).map(|o| o.payment_asset)
        }

        fn order_completed_at(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id)
                .and_then(|o| o.completed_at)
                .map(|b| b.try_into().unwrap_or(u64::MAX))
        }

        fn order_status(order_id: u64) -> Option<OrderStatus> {
            Orders::<T>::get(order_id).map(|o| o.status)
        }

        fn order_entity_id(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id).map(|o| o.entity_id)
        }

        fn order_product_id(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id).map(|o| o.product_id)
        }

        fn order_quantity(order_id: u64) -> Option<u32> {
            Orders::<T>::get(order_id).map(|o| o.quantity)
        }

        fn order_created_at(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id).map(|o| o.created_at.try_into().unwrap_or(u64::MAX))
        }

        fn order_paid_at(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id).map(|o| o.created_at.try_into().unwrap_or(u64::MAX))
        }

        fn order_shipped_at(order_id: u64) -> Option<u64> {
            Orders::<T>::get(order_id)
                .and_then(|o| o.shipped_at)
                .map(|b| b.try_into().unwrap_or(u64::MAX))
        }

        fn has_active_orders_for_shop(shop_id: u64) -> bool {
            ShopOrders::<T>::get(shop_id).iter().any(|&order_id| {
                Orders::<T>::get(order_id)
                    .map(|o| {
                        !matches!(
                            o.status,
                            OrderStatus::Completed
                                | OrderStatus::Cancelled
                                | OrderStatus::Refunded
                                | OrderStatus::Expired
                                | OrderStatus::PartiallyRefunded
                        )
                    })
                    .unwrap_or(false)
            })
        }

        fn order_payer(order_id: u64) -> Option<T::AccountId> {
            Orders::<T>::get(order_id).and_then(|o| o.payer)
        }

        fn order_fund_account(order_id: u64) -> Option<T::AccountId> {
            Orders::<T>::get(order_id).map(|o| o.payer.unwrap_or(o.buyer))
        }
    }
}
