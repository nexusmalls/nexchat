// Copyright (C) Polytope Labs Ltd. (vendored ABI / precision logic)
// Copyright (C) Nexus contributors (burn/mint adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for `pallet-bridge-ismp`.
//! `pallet-bridge-ismp` 的类型定义。
//!
//! The [`Message`] ABI and the [`EvmToSubstrate`] trait are **vendored** from
//! Polytope Labs' `pallet-hyper-fungible-token` (commit `6931d9f6`, crate line
//! `2512.0.0`) so that the on-chain encoding byte-matches the EVM-side ERC-6160
//! `NEX` contract (D3=(c), see `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md` §13.3).
//! [`Message`] ABI 与 [`EvmToSubstrate`] trait 自 Polytope Labs 的
//! `pallet-hyper-fungible-token`（commit `6931d9f6`，crate 线 `2512.0.0`）**vendor**
//! 而来，以保证链上编码与 EVM 侧 ERC-6160 `NEX` 合约逐字节对齐（D3=(c)）。

use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use ismp::host::StateMachine;
use scale_info::TypeInfo;
use sp_core::{H160, H256};

// ABI-compatible message matching the Solidity HyperFungibleToken `Message` struct:
// `struct Message { bytes from; bytes to; uint256 amount; bytes data; }`.
// VENDORED verbatim from `pallet-hyper-fungible-token@6931d9f6` so encoding is
// byte-identical to the EVM contract.
// 与 Solidity HyperFungibleToken `Message` 结构对齐的 ABI 兼容消息。逐字节 vendor 自
// `pallet-hyper-fungible-token@6931d9f6`，确保与 EVM 合约编码一致。
alloy_sol_macro::sol! {
    #![sol(all_derives)]
    struct Message {
        bytes from;
        bytes to;
        uint256 amount;
        bytes data;
    }
}

/// Per-EVM-chain bridge configuration for native NEX.
/// 针对原生 NEX 的逐 EVM 链桥接配置。
///
/// `contract` is both the outbound destination (the `to` module id of the ISMP
/// POST) and the inbound source allow-list entry (`on_accept` requires
/// `from == contract`). `erc_decimals` is the ERC-20 precision on that chain;
/// it must be `>= NativeDecimals` so the 18→12 down-scaling never widens dust.
/// `contract` 同时作为出站目标（ISMP POST 的 `to` 模块 id）与入站来源 allow-list
///（`on_accept` 要求 `from == contract`）。`erc_decimals` 为该链 ERC-20 精度，必须
/// `>= NativeDecimals`，以保证 18→12 缩小换算不会放大 dust。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct ChainConfig {
    /// The ERC-6160 `NEX` token contract address on the EVM chain.
    /// EVM 链上的 ERC-6160 `NEX` 代币合约地址。
    pub contract: H160,
    /// ERC-20 decimals on the EVM chain (must be `>= NativeDecimals`).
    /// EVM 链上的 ERC-20 精度（必须 `>= NativeDecimals`）。
    pub erc_decimals: u8,
}

/// Per-tx / per-day outbound limits (governance-adjustable via `set_limits`).
/// 出站单笔 / 单日限额（经 `set_limits` 治理调整）。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug, MaxEncodedLen)]
pub struct BridgeLimits<Balance> {
    /// Maximum amount per single `bridge_out`. Single-tx cap.
    /// 单次 `bridge_out` 的上限。
    pub per_tx: Balance,
    /// Maximum cumulative amount bridged out within one `DailyLimitWindow`.
    /// 单个 `DailyLimitWindow` 内累计桥出上限。
    pub daily: Balance,
}

impl<Balance: Default> Default for BridgeLimits<Balance> {
    fn default() -> Self {
        Self {
            per_tx: Balance::default(),
            daily: Balance::default(),
        }
    }
}

/// Converts an EVM `H160` address into a Substrate `AccountId`.
/// 将 EVM `H160` 地址转换为 Substrate `AccountId`。
///
/// VENDORED from HFT. Unused by the pure NEX asset bridge (Stage 2); reserved for
/// HB-ENT-01 cross-chain ordering (Stage 3), which derives accounts from EVM keys.
/// 自 HFT vendor。纯 NEX 资产桥（Stage 2）用不到；为 HB-ENT-01 跨链下单（Stage 3）
/// 从 EVM 公钥派生账户预留。
pub trait EvmToSubstrate<T: frame_system::Config> {
    fn convert(addr: H160) -> T::AccountId;
}

/// Default: zero-pad the 20-byte EVM address into a 32-byte `AccountId`.
/// 默认实现：将 20 字节 EVM 地址零填充为 32 字节 `AccountId`。
impl<T: frame_system::Config> EvmToSubstrate<T> for ()
where
    <T as frame_system::Config>::AccountId: From<[u8; 32]>,
{
    fn convert(addr: H160) -> <T as frame_system::Config>::AccountId {
        let mut account = [0u8; 32];
        account[12..].copy_from_slice(&addr.0);
        account.into()
    }
}

/// Re-export for event payloads.
/// 供事件负载复用。
pub type Commitment = H256;

/// The only [`InboundOp`] payload schema version this runtime accepts. Bumped only
/// on a breaking layout change; `on_accept` rejects any other value so a future
/// EVM-side format can never be silently mis-decoded.
/// 本 runtime 接受的唯一 [`InboundOp`] 负载版本号。仅在布局发生破坏性变更时递增；
/// `on_accept` 拒绝其它取值，使未来的 EVM 侧格式永不被静默错解。
pub const PAYLOAD_SCHEMA_VERSION: u8 = 1;

/// Cross-chain order intent (HB-ENT-01), carried SCALE-encoded inside the
/// vendored [`Message`]'s `data` field. The order amount is **not** carried here —
/// it is the bridged [`Message`]`.amount` (single source of truth), which is both
/// the buyer's budget and the slippage cap. `schema_version` is validated against
/// [`PAYLOAD_SCHEMA_VERSION`]; `buyer_evm` / `referrer` are EVM `H160`s derived into
/// local `AccountId`s via [`EvmToSubstrate`]. Replay protection is provided by the
/// ISMP request commitment + receipt, so no application-level nonce is needed.
/// 跨链下单意图（HB-ENT-01），以 SCALE 编码置于 vendor [`Message`] 的 `data` 字段内。
/// 下单金额**不**在此携带——它就是已桥接的 [`Message`]`.amount`（唯一真相来源），同时作为买家
/// 预算与滑点上限。`schema_version` 对 [`PAYLOAD_SCHEMA_VERSION`] 校验；`buyer_evm` / `referrer`
/// 为 EVM `H160`，经 [`EvmToSubstrate`] 派生为本地 `AccountId`。重放保护由 ISMP 请求 commitment
/// + receipt 提供，故无需应用层 nonce。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct OrderIntent {
    /// Payload schema version, validated against [`PAYLOAD_SCHEMA_VERSION`]. 负载版本号，按 [`PAYLOAD_SCHEMA_VERSION`] 校验。
    pub schema_version: u8,
    /// Buyer's EVM address; derived into the local buyer account. 买家 EVM 地址（派生本地账户）。
    pub buyer_evm: [u8; 20],
    /// Target product id. 目标商品 id。
    pub product_id: u64,
    /// Order quantity. 下单数量。
    pub quantity: u32,
    /// Optional referrer EVM address (derived). 可选推荐人 EVM 地址（派生）。
    pub referrer: Option<[u8; 20]>,
}

/// Withdraw a derived account's NEX back to an EVM chain (HB-ENT-01 §7, G-B4).
/// Authorisation is performed on the EVM side (the gateway checks
/// `msg.sender == owner_evm`); on Nexus we trust the registered source contract
/// (same allow-list as every inbound message) and move only the derived owner's funds.
/// `schema_version` is validated against [`PAYLOAD_SCHEMA_VERSION`]; replay protection
/// is provided by the ISMP request commitment + receipt (no application-level nonce).
/// 将派生账户的 NEX 提回某 EVM 链（HB-ENT-01 §7，G-B4）。鉴权在 EVM 侧完成（网关校验
/// `msg.sender == owner_evm`）；Nexus 侧信任已注册来源合约（与所有入站消息同一 allow-list），
/// 且只动用该派生账户本人的资金。`schema_version` 对 [`PAYLOAD_SCHEMA_VERSION`] 校验；重放保护由
/// ISMP 请求 commitment + receipt 提供（无应用层 nonce）。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct WithdrawRequest {
    /// Payload schema version, validated against [`PAYLOAD_SCHEMA_VERSION`]. 负载版本号，按 [`PAYLOAD_SCHEMA_VERSION`] 校验。
    pub schema_version: u8,
    /// Derived owner's EVM address (whose Nexus account is debited). 派生持有人 EVM 地址（其 Nexus 账户被扣款）。
    pub owner_evm: [u8; 20],
    /// Amount to withdraw (EVM precision). 提款金额（EVM 精度）。
    pub amount_nex: u128,
    /// EVM recipient of the bridged-back NEX. 提回 NEX 的 EVM 收款人。
    pub dest_recipient: [u8; 20],
}

/// A derived-account withdrawal queued in the two-phase, time-locked withdraw flow
/// (H2 containment). Because a derived account has no Substrate key, a
/// [`WithdrawRequest`] is authorised entirely by the EVM gateway; to bound the blast
/// radius of a compromised/buggy gateway, `on_accept` does **not** burn immediately —
/// it records this entry and only after [`WithdrawDelay`](crate::Config::WithdrawDelay)
/// blocks can [`execute_withdraw`](crate::Pallet::execute_withdraw) run the burn +
/// outbound POST. During the delay a guardian may
/// [`cancel_withdraw`](crate::Pallet::cancel_withdraw) it. No funds are touched until
/// execution, so a veto simply drops the entry.
/// 两阶段时间锁提款流程中排队的派生账户提款（H2 收敛）。派生账户无 Substrate 私钥，
/// [`WithdrawRequest`] 完全由 EVM 网关鉴权；为限制网关被攻破/有 bug 的爆炸半径，`on_accept`
/// **不**立即销毁——它记录此条目，须经 [`WithdrawDelay`](crate::Config::WithdrawDelay) 个区块后
/// 才能由 [`execute_withdraw`](crate::Pallet::execute_withdraw) 执行销毁 + 出站 POST。延迟窗口内
/// guardian 可 [`cancel_withdraw`](crate::Pallet::cancel_withdraw) 否决。执行前不动用任何资金，
/// 故否决只是删除条目。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub struct PendingWithdraw<AccountId, Balance, BlockNumber> {
    /// Derived owner whose balance will be burned at execution. 执行时被销毁余额的派生持有人。
    pub owner: AccountId,
    /// Destination EVM chain (== the inbound source). 目标 EVM 链（== 入站来源）。
    pub dest: StateMachine,
    /// EVM recipient of the bridged-back NEX. 提回 NEX 的 EVM 收款人。
    pub recipient: [u8; 20],
    /// Native NEX amount to burn (already decoded from EVM precision). 待销毁的原生 NEX 金额（已由 EVM 精度解码）。
    pub amount: Balance,
    /// Earliest block at which `execute_withdraw` may run. 可执行 `execute_withdraw` 的最早区块。
    pub execute_at: BlockNumber,
}

/// Discriminated operation carried in [`Message`]`.data`. An empty `data` means a
/// plain asset transfer (Stage 2); a non-empty `data` SCALE-decodes to this enum.
/// [`Message`]`.data` 携带的判别式操作。空 `data` 表示纯资产转账（Stage 2）；非空 `data`
/// 按本枚举 SCALE 解码。
#[derive(Clone, Encode, Decode, TypeInfo, PartialEq, Eq, RuntimeDebug)]
pub enum InboundOp {
    /// Cross-chain digital order (HB-ENT-01 Stage 3b). 跨链数字商品下单。
    Order(OrderIntent),
    /// Withdraw a derived account's NEX back to an EVM chain (HB-ENT-01 §7, Stage 3c).
    /// 将派生账户的 NEX 提回某 EVM 链（HB-ENT-01 §7，Stage 3c）。
    Withdraw(WithdrawRequest),
}

/// Bridge → business handler for authenticated cross-chain digital orders.
/// Implemented by the runtime against `pallet-entity-order::do_cross_order`,
/// keeping the low-level bridge decoupled from the high-level order pallet.
/// Returns the created order id on success.
/// 桥 → 业务的跨链数字下单处理器（经鉴权）。由 runtime 对接
/// `pallet-entity-order::do_cross_order` 实现，使底层桥与上层订单 pallet 解耦；成功返回订单 id。
pub trait CrossChainOrderHandler<AccountId, Balance> {
    fn do_cross_order(
        buyer: AccountId,
        payer: AccountId,
        product_id: u64,
        quantity: u32,
        max_nex_amount: Balance,
        referrer: Option<AccountId>,
    ) -> Result<u64, sp_runtime::DispatchError>;

    /// Worst-case weight of one [`do_cross_order`](Self::do_cross_order) call. The
    /// bridge adds this to its own base work when reporting `on_accept`'s weight, so
    /// `pallet-ismp` meters the full inbound cost of a cross-order (the order touches
    /// many pallets and can itself dispatch payouts) instead of a flat estimate.
    /// 一次 [`do_cross_order`](Self::do_cross_order) 调用的最坏权重。桥在上报 `on_accept`
    /// 权重时把它加到自身基础开销上，使 `pallet-ismp` 计量跨链下单的完整入站成本（下单触达多个
    /// pallet 且可能自身派发派款），而非一个固定估计值。
    fn cross_order_weight() -> Weight;
}

/// Default: no handler configured — every cross-order fails (and is credited).
/// 默认：未配置处理器——所有跨链下单失败（并入账）。
impl<AccountId, Balance> CrossChainOrderHandler<AccountId, Balance> for () {
    fn do_cross_order(
        _buyer: AccountId,
        _payer: AccountId,
        _product_id: u64,
        _quantity: u32,
        _max_nex_amount: Balance,
        _referrer: Option<AccountId>,
    ) -> Result<u64, sp_runtime::DispatchError> {
        Err(sp_runtime::DispatchError::Other(
            "cross-order handler not configured",
        ))
    }

    /// The no-op handler returns immediately, so its weight is zero.
    /// 空处理器立即返回，权重为零。
    fn cross_order_weight() -> Weight {
        Weight::zero()
    }
}

/// Bridge → business timeout callback for tracked outbound payouts (HB-WD-01
/// mechanism 2). When an outbound POST dispatched via
/// [`do_outbound_tracked`](crate::Pallet::do_outbound_tracked) times out, the
/// bridge — after re-minting the NEX to the original sender — hands the opaque
/// `meta` (attached at dispatch) back to the business layer so it can compensate
/// (e.g. restore a promoter's commission `pending`). Implemented by the runtime
/// against `pallet-commission-core`. This is the timeout dual of the
/// `CrossChainPayout` port (business → bridge → business).
/// 桥 → 业务的「已跟踪出站派发」超时回调（HB-WD-01 机制 2）。当经
/// [`do_outbound_tracked`](crate::Pallet::do_outbound_tracked) 派发的出站 POST 超时时，
/// 桥在把 NEX 铸回原发送方后，将派发时附带的不透明 `meta` 交还业务层用于补偿（例如恢复
/// 推广员佣金 `pending`）。由 runtime 对接 `pallet-commission-core` 实现。这是
/// `CrossChainPayout` 端口的超时对偶（业务 → 桥 → 业务）。
pub trait PayoutRefundHandler {
    /// Called once per timed-out tracked payout, inside a nested storage layer
    /// (so a handler error rolls back only the business side; the bridge's NEX
    /// re-mint to the sender is already committed and kept). `meta` is exactly
    /// the payload supplied at dispatch.
    /// 每个超时的已跟踪派发调用一次，在嵌套存储层内执行（handler 出错仅回滚业务侧；桥向
    /// 发送方的 NEX 重铸已提交并保留）。`meta` 即派发时提供的载荷。
    fn on_payout_timeout(meta: &[u8]) -> Result<(), sp_runtime::DispatchError>;
}

/// Default: no refund handler — timed-out payouts only get the bridge's re-mint
/// to the sender (mechanism 1); no business-side compensation.
/// 默认：无退款处理器——超时派发仅获得桥向发送方的重铸（机制 1）；无业务侧补偿。
impl PayoutRefundHandler for () {
    fn on_payout_timeout(_meta: &[u8]) -> Result<(), sp_runtime::DispatchError> {
        Ok(())
    }
}
