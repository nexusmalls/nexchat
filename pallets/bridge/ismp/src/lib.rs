// Copyright (C) Polytope Labs Ltd. (vendored ABI / precision / send-burn core)
// Copyright (C) Nexus contributors (native burn/mint adaptation + guardrails)
// SPDX-License-Identifier: Apache-2.0

//! # NEX Asset Bridge (`pallet-bridge-ismp`)
//!
//! Self-built ISMP asset bridge for the **native NEX** token, implementing
//! HB-ASSET-01 under decision **D3=(c)**: vendor the audited core of Polytope
//! Labs' `pallet-hyper-fungible-token` (`send` burn branch, `on_accept`,
//! `on_timeout`, the `Message` ABI, and the precision functions) and adapt it to
//! a **burn/mint** model for the native currency, with all guardrails and the
//! in-flight ledger inlined. Unlike the upstream custody model, native NEX is
//! **really burned** (`Currency::withdraw` → `TotalIssuance↓`) on the way out and
//! **really minted** (`Currency::deposit_creating` → `TotalIssuance↑`) on the way
//! back. There is no local liquidity pool, no `NativeFungibleAdapter`, no fork,
//! and no dependency on the deprecated `pallet-token-gateway` or the unpublished
//! HFT crate.
//!
//! # NEX 资产跨链桥（`pallet-bridge-ismp`）
//!
//! 针对**原生 NEX** 的自建 ISMP 资产桥，按决策 **D3=(c)** 实现 HB-ASSET-01：vendor
//! Polytope Labs `pallet-hyper-fungible-token` 已审计的核心（`send` 的 burn 分支、
//! `on_accept`、`on_timeout`、`Message` ABI 与精度函数），并改造为原生币的
//! **burn/mint** 模型，护栏与在途账本全部内联。与上游托管模型不同，原生 NEX 出站时
//! **真销毁**（`Currency::withdraw` → `TotalIssuance↓`），回桥时**真铸造**
//!（`Currency::deposit_creating` → `TotalIssuance↑`）。无本地资金池、无
//! `NativeFungibleAdapter`、无 fork，也不依赖已 deprecated 的 `pallet-token-gateway`
//! 或未发布的 HFT crate。
//!
//! See `docs/HB_ASSET_01_NEX_HFT_DEV_SPEC.md` and `docs/HYPERBRIDGE_INTEGRATION.md`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod error;
pub mod impls;
pub mod module;
pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use types::Message;
pub use weights::WeightInfo;

use frame_support::PalletId;
use pallet_ismp::ModuleId;

/// The well-known ISMP module id for this bridge. EVM `NEX` contracts must set
/// this as the destination (`to`) when sending messages to Nexus, and the
/// runtime's `IsmpRouter` routes this id to this pallet.
/// 本桥的 ISMP 模块 id。EVM `NEX` 合约向 Nexus 发送消息时必须以此为目标（`to`），
/// 运行时 `IsmpRouter` 据此把消息路由到本 pallet。
pub const PALLET_ID: ModuleId = ModuleId::Pallet(PalletId(*b"nexbridg"));

/// Convenience: the byte form of [`PALLET_ID`] used in ISMP request `from`/`to`.
/// 便捷函数：[`PALLET_ID`] 的字节形式，用于 ISMP 请求的 `from`/`to`。
pub fn module_id_bytes() -> alloc::vec::Vec<u8> {
    PALLET_ID.to_bytes()
}

/// The native-currency balance type used throughout this pallet.
/// 本 pallet 通用的原生币余额类型。
pub type BalanceOf<T> = <<T as Config>::NativeCurrency as frame_support::traits::Currency<
    <T as frame_system::Config>::AccountId,
>>::Balance;

/// Maximum supported gap between a chain's ERC decimals and the native decimals.
/// The precision helpers (`convert_to_erc20` / `convert_to_balance`) compute the
/// scale factor `10^(erc_decimals - NativeDecimals)` as a `u128`, so the gap must
/// stay `<= 38` (`10^38 < u128::MAX < 10^39`). A larger gap would overflow that
/// `u128` — silently wrapping to a wrong factor in release builds (no
/// `overflow-checks`), or panicking in debug — corrupting every amount conversion
/// on that chain. Real ERC-20 tokens use `<= 18` decimals, so this is purely a
/// defensive ceiling enforced at [`register_chain`](Pallet::register_chain).
/// 某条链的 ERC 精度与原生精度的最大允许差。精度函数（`convert_to_erc20` /
/// `convert_to_balance`）以 `u128` 计算缩放因子 `10^(erc_decimals - NativeDecimals)`，
/// 故差值须 `<= 38`（`10^38 < u128::MAX < 10^39`）。更大的差值会让该 `u128` 溢出——release
/// 构建（无 `overflow-checks`）下静默 wrapping 成错误因子、debug 下 panic——污染该链上所有
/// 金额换算。真实 ERC-20 token 精度 `<= 18`，故此处纯属在
/// [`register_chain`](Pallet::register_chain) 强制的防御上界。
pub const MAX_ERC_NATIVE_DECIMAL_GAP: u8 = 38;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::types::{BridgeLimits, ChainConfig, EvmToSubstrate};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, WithdrawReasons},
    };
    use frame_system::pallet_prelude::*;
    use ismp::{
        dispatcher::{DispatchPost, DispatchRequest, FeeMetadata, IsmpDispatcher},
        host::StateMachine,
    };
    use sp_core::{H160, H256, U256};
    use sp_runtime::traits::{CheckedAdd, Saturating, Zero};

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    /// Configuration trait for the NEX asset bridge.
    /// NEX 资产桥的配置 trait。
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_ismp::Config {
        /// The aggregated runtime event type.
        /// 聚合的运行时事件类型。
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// ISMP request dispatcher (wired to `pallet-hyperbridge`, which charges the
        /// per-byte protocol fee then commits the request to `pallet-ismp`).
        /// ISMP 请求派发器（接 `pallet-hyperbridge`，先收取按字节协议费再把请求提交给
        /// `pallet-ismp`）。
        type Dispatcher: IsmpDispatcher<
            Account = Self::AccountId,
            Balance = <Self as pallet_ismp::Config>::Balance,
        >;

        /// The native currency (NEX). Bridged out via real burn, bridged back via
        /// real mint — `TotalIssuance` moves with cross-chain supply.
        /// 原生币（NEX）。出站真销毁、回桥真铸造——`TotalIssuance` 随跨链供给变动。
        type NativeCurrency: Currency<Self::AccountId>;

        /// EVM `H160` → Substrate `AccountId` mapping. Unused by the pure asset
        /// bridge; reserved for HB-ENT-01 (Stage 3).
        /// EVM `H160` → Substrate `AccountId` 映射。纯资产桥用不到；为 HB-ENT-01（Stage 3）预留。
        type EvmToSubstrate: EvmToSubstrate<Self>;

        /// Decimals of the native currency (NEX = 12).
        /// 原生币精度（NEX = 12）。
        #[pallet::constant]
        type NativeDecimals: Get<u8>;

        /// Minimum bridge amount. Must be `>= 10^(erc-local)` so inbound conversion
        /// never truncates to zero (see HB-ASSET-01 §3A.5 / G-A1-1).
        /// 最小桥接额。必须 `>= 10^(erc-local)`，以保证入站换算不会被截断为 0
        ///（见 HB-ASSET-01 §3A.5 / G-A1-1）。
        #[pallet::constant]
        type MinBridgeAmount: Get<BalanceOf<Self>>;

        /// Length (in blocks) of the rolling daily-limit window.
        /// 滚动单日限额窗口的长度（区块数）。
        #[pallet::constant]
        type DailyLimitWindow: Get<BlockNumberFor<Self>>;

        /// ISMP request timeout, in seconds (0 = no timeout).
        /// ISMP 请求超时（秒，0 = 不超时）。
        #[pallet::constant]
        type RequestTimeout: Get<u64>;

        /// Privileged origin for governance ops (pause / limits / chain registry).
        /// 治理操作（暂停 / 限额 / 链注册）的特权来源。
        type BridgeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Handler for authenticated cross-chain digital orders (HB-ENT-01). Wired
        /// by the runtime to `pallet-entity-order::do_cross_order`. Defaults to `()`
        /// (every cross-order fails and is credited to the derived buyer).
        /// 经鉴权的跨链数字下单处理器（HB-ENT-01）。由 runtime 接到
        /// `pallet-entity-order::do_cross_order`。默认 `()`（所有跨链下单失败并入账派生买家）。
        type CrossOrderHandler: crate::types::CrossChainOrderHandler<
            Self::AccountId,
            BalanceOf<Self>,
        >;

        /// Timeout callback for tracked outbound payouts (HB-WD-01 mechanism 2).
        /// Wired by the runtime to `pallet-commission-core`. Defaults to `()` (no
        /// business-side compensation; timed-out payouts only get the re-mint).
        /// 已跟踪出站派发的超时回调（HB-WD-01 机制 2）。由 runtime 对接
        /// `pallet-commission-core`。默认 `()`（无业务侧补偿；超时派发仅获得重铸）。
        type PayoutRefundHandler: crate::types::PayoutRefundHandler;

        /// Max byte length of the opaque refund `meta` stored per tracked payout.
        /// 每笔已跟踪派发存储的不透明退款 `meta` 的最大字节长度。
        #[pallet::constant]
        type MaxPayoutMeta: Get<u32>;

        /// Blocks after a tracked payout's dispatch beyond which its
        /// [`PayoutRefunds`] entry is considered definitively resolved — the request
        /// has either timed out (consuming the entry) or been delivered — and may be
        /// pruned by [`prune_payout_refunds`](Pallet::prune_payout_refunds). MUST be
        /// set well above the outbound request timeout (converted to blocks) so an
        /// entry is never pruned before its timeout could still fire.
        /// 已跟踪派发的派发区块之后、超过此区块数即视为该 [`PayoutRefunds`] 条目已确定解决
        ///（请求要么已超时并消费该条目、要么已投递），可由
        /// [`prune_payout_refunds`](Pallet::prune_payout_refunds) 清理。必须设为**远高于**
        /// 出站请求超时（换算成区块），以保证条目永不会在其超时仍可能触发之前被清理。
        #[pallet::constant]
        type PayoutRefundTtl: Get<BlockNumberFor<Self>>;

        /// Veto window (in blocks) for derived-account withdrawals (H2 containment).
        /// A [`WithdrawRequest`](crate::types::WithdrawRequest) is authorised entirely
        /// by the EVM gateway (a derived account has no Substrate key), so to bound a
        /// compromised gateway the inbound callback only **queues** the withdrawal;
        /// [`execute_withdraw`](Pallet::execute_withdraw) may run the burn + outbound
        /// POST no earlier than `WithdrawDelay` blocks later, during which the
        /// [`BridgeOrigin`](Config::BridgeOrigin) guardian can
        /// [`cancel_withdraw`](Pallet::cancel_withdraw) a suspicious entry. `0` makes
        /// withdrawals immediately executable (no veto window).
        /// 派生账户提款的否决窗口（区块数，H2 收敛）。
        /// [`WithdrawRequest`](crate::types::WithdrawRequest) 完全由 EVM 网关鉴权（派生账户无
        /// Substrate 私钥），故为限制网关被攻破，入站回调仅**排队**提款；
        /// [`execute_withdraw`](Pallet::execute_withdraw) 最早须在 `WithdrawDelay` 个区块后才能执行
        /// 销毁 + 出站 POST，窗口内 [`BridgeOrigin`](Config::BridgeOrigin) guardian 可
        /// [`cancel_withdraw`](Pallet::cancel_withdraw) 否决可疑条目。`0` 表示提款可即时执行（无窗口）。
        #[pallet::constant]
        type WithdrawDelay: Get<BlockNumberFor<Self>>;

        /// Weight information.
        /// 权重信息。
        type WeightInfo: WeightInfo;
    }

    /// Cumulative NEX bridged out and still in flight; decremented on bridge-back
    /// and timeout refund. Mints can never exceed this (anti-inflation invariant).
    /// 已桥出且在途的 NEX 累计量；回桥与超时退款时递减。铸造永不超过此值（防增发不变量）。
    #[pallet::storage]
    pub type BridgedOut<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

    /// Per-destination in-flight amount; `Σ == BridgedOut` (checked by `try_state`).
    /// 按目标链分桶的在途量；`Σ == BridgedOut`（由 `try_state` 校验）。
    #[pallet::storage]
    pub type BridgedOutByChain<T: Config> =
        StorageMap<_, Blake2_128Concat, StateMachine, BalanceOf<T>, ValueQuery>;

    /// Global pause switch. 全局暂停开关。
    #[pallet::storage]
    pub type Paused<T: Config> = StorageValue<_, bool, ValueQuery>;

    /// Per-chain pause switch. 分链暂停开关。
    #[pallet::storage]
    pub type PausedChain<T: Config> =
        StorageMap<_, Blake2_128Concat, StateMachine, bool, ValueQuery>;

    /// Rolling daily-out accumulator: `(window_start_block, amount_in_window)`.
    /// 滚动单日桥出累加器：`(窗口起始区块, 窗口内累计量)`。
    #[pallet::storage]
    pub type DailyOut<T: Config> = StorageValue<_, (BlockNumberFor<T>, BalanceOf<T>), ValueQuery>;

    /// Per-tx / per-day outbound limits (governance-adjustable).
    /// 出站单笔 / 单日限额（治理可调）。
    #[pallet::storage]
    pub type Limits<T: Config> = StorageValue<_, BridgeLimits<BalanceOf<T>>, ValueQuery>;

    /// Registered EVM chains: `StateMachine → ChainConfig{ contract, erc_decimals }`.
    /// Serves as the outbound target and the inbound source allow-list.
    /// 已注册 EVM 链：`StateMachine → ChainConfig{ contract, erc_decimals }`。同时作为
    /// 出站目标与入站来源 allow-list。
    #[pallet::storage]
    pub type Chains<T: Config> =
        StorageMap<_, Blake2_128Concat, StateMachine, ChainConfig, OptionQuery>;

    /// Refund context for tracked outbound payouts (HB-WD-01 mechanism 2):
    /// `request commitment → opaque business meta`. Set by
    /// [`do_outbound_tracked`](Pallet::do_outbound_tracked) at dispatch and
    /// `take`n by `on_timeout`, which then invokes
    /// [`PayoutRefundHandler`](crate::types::PayoutRefundHandler). Entries are
    /// only created for tracked payouts (plain `bridge_out` / derived withdraw do
    /// not populate it), and are removed on timeout. The stored block is the
    /// dispatch height; successfully relayed requests never time out, so their
    /// (harmless — commitments are unique and never replayed) entries are reaped by
    /// the permissionless [`prune_payout_refunds`](Pallet::prune_payout_refunds)
    /// once older than [`PayoutRefundTtl`](Config::PayoutRefundTtl).
    /// 已跟踪出站派发的退款上下文（HB-WD-01 机制 2）：`请求 commitment →（派发区块, 不透明业务
    /// meta）`。派发时由 [`do_outbound_tracked`](Pallet::do_outbound_tracked) 写入，`on_timeout`
    /// `take` 后调用 [`PayoutRefundHandler`](crate::types::PayoutRefundHandler)。仅已跟踪派发
    /// 建条目（普通 `bridge_out` / 派生提款不写），超时即删。存储的区块为派发高度；成功转发的请求
    /// 永不超时，其（无害——commitment 唯一且不重放）条目在超过
    /// [`PayoutRefundTtl`](Config::PayoutRefundTtl) 后由无许可的
    /// [`prune_payout_refunds`](Pallet::prune_payout_refunds) 回收。
    #[pallet::storage]
    pub type PayoutRefunds<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        crate::types::Commitment,
        (BlockNumberFor<T>, BoundedVec<u8, T::MaxPayoutMeta>),
        OptionQuery,
    >;

    /// Monotonic id for queued derived-account withdrawals (H2 two-phase flow).
    /// 派生账户排队提款的单调自增 id（H2 两阶段流程）。
    #[pallet::storage]
    pub type NextWithdrawId<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Time-locked derived-account withdrawals awaiting execution or a guardian veto
    /// (H2 containment): `id → PendingWithdraw`. Populated by the inbound withdraw
    /// callback, consumed by [`execute_withdraw`](Pallet::execute_withdraw) (after
    /// [`WithdrawDelay`](Config::WithdrawDelay)) or [`cancel_withdraw`](Pallet::cancel_withdraw).
    /// No balance is touched while an entry sits here — the burn happens only at execution.
    /// 时间锁的派生账户提款，等待执行或 guardian 否决（H2 收敛）：`id → PendingWithdraw`。由入站提款
    /// 回调写入，由 [`execute_withdraw`](Pallet::execute_withdraw)（经
    /// [`WithdrawDelay`](Config::WithdrawDelay) 后）或 [`cancel_withdraw`](Pallet::cancel_withdraw)
    /// 消费。条目存在期间不动用任何余额——销毁只在执行时发生。
    #[pallet::storage]
    pub type PendingWithdraws<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        u64,
        crate::types::PendingWithdraw<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>,
        OptionQuery,
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// NEX was burned and an outbound ISMP POST dispatched.
        /// NEX 已销毁，并派发了出站 ISMP POST。
        BridgedOut {
            sender: T::AccountId,
            recipient: H160,
            dest: StateMachine,
            amount: BalanceOf<T>,
            commitment: H256,
        },
        /// NEX was minted to a beneficiary from a verified inbound message.
        /// 经已验证的入站消息，向受益人铸造了 NEX。
        BridgedIn {
            beneficiary: T::AccountId,
            source: StateMachine,
            amount: BalanceOf<T>,
        },
        /// An outbound transfer timed out and the sender was refunded (re-minted).
        /// 出站转账超时，已向发送方退款（重新铸造）。
        BridgeRefunded {
            beneficiary: T::AccountId,
            dest: StateMachine,
            amount: BalanceOf<T>,
        },
        /// A chain was registered / updated. 链已注册 / 更新。
        ChainRegistered {
            chain: StateMachine,
            contract: H160,
            erc_decimals: u8,
        },
        /// A chain was deregistered. 链已注销。
        ChainDeregistered { chain: StateMachine },
        /// Pause state changed (global if `chain` is `None`). 暂停状态变化（`chain` 为 `None` 即全局）。
        PausedSet {
            chain: Option<StateMachine>,
            paused: bool,
        },
        /// Limits changed. 限额已变更。
        LimitsChanged {
            per_tx: BalanceOf<T>,
            daily: BalanceOf<T>,
        },
        /// A cross-chain digital order was placed (and instantly settled).
        /// 跨链数字商品下单成功（并即时完成）。
        CrossOrderPlaced {
            order_id: u64,
            buyer: T::AccountId,
            source: StateMachine,
            product_id: u64,
        },
        /// A cross-chain order failed; the minted NEX stays credited to the derived
        /// buyer account (DerivedCredit) and can be withdrawn later. 跨链下单失败；
        /// 已铸 NEX 留在派生买家账户（DerivedCredit），可后续提款。
        CrossOrderFailed {
            buyer: T::AccountId,
            source: StateMachine,
            amount: BalanceOf<T>,
            error: sp_runtime::DispatchError,
        },
        /// A derived-account withdrawal was queued (H2 two-phase flow): it becomes
        /// executable at `execute_at` and can be vetoed by the guardian until then.
        /// 派生账户提款已排队（H2 两阶段流程）：将于 `execute_at` 可执行，在此之前 guardian 可否决。
        WithdrawQueued {
            id: u64,
            owner: T::AccountId,
            dest: StateMachine,
            recipient: H160,
            amount: BalanceOf<T>,
            execute_at: BlockNumberFor<T>,
        },
        /// A queued derived-account withdrawal was vetoed by the guardian before
        /// execution (H2 containment); no funds were ever touched.
        /// 一笔排队的派生账户提款在执行前被 guardian 否决（H2 收敛）；从未动用任何资金。
        WithdrawCancelled { id: u64 },
        /// A queued derived-account withdrawal executed (HB-ENT-01 §7): NEX was burned
        /// from the derived owner and an outbound POST dispatched. 排队的派生账户提款已执行
        ///（HB-ENT-01 §7）：从派生持有人销毁 NEX 并派发出站 POST。
        DerivedWithdraw {
            id: u64,
            owner: T::AccountId,
            dest: StateMachine,
            recipient: H160,
            amount: BalanceOf<T>,
            commitment: H256,
        },
        /// A timed-out tracked payout (HB-WD-01 mechanism 2) was handed back to the
        /// business `PayoutRefundHandler`. `handled` is `true` if the callback
        /// returned `Ok` (business-side compensation applied), `false` if it failed
        /// (only the bridge's re-mint to the sender stands). 一笔超时的已跟踪派发
        ///（HB-WD-01 机制 2）已交还业务 `PayoutRefundHandler`。回调返回 `Ok`（业务侧补偿已应用）
        /// 则 `handled = true`，失败则 `false`（仅保留桥向发送方的重铸）。
        PayoutRefundNotified { commitment: H256, handled: bool },
        /// Stale tracked-payout refund contexts were pruned (HB-WD-01 mechanism 2):
        /// `removed` entries past [`PayoutRefundTtl`](Config::PayoutRefundTtl) were
        /// deleted. 已清理过期的已跟踪派发退款上下文（HB-WD-01 机制 2）：删除了
        /// `removed` 个超过 [`PayoutRefundTtl`](Config::PayoutRefundTtl) 的条目。
        PayoutRefundsPruned { removed: u32 },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// The bridge (or this destination lane) is paused. 桥（或该目标通道）已暂停。
        BridgePaused,
        /// The destination chain is not registered. 目标链未注册。
        ChainNotRegistered,
        /// The state machine is not an EVM chain. 状态机不是 EVM 链。
        NotEvmChain,
        /// Configured ERC decimals are below the native decimals. 配置的 ERC 精度低于原生精度。
        ErcDecimalsBelowLocal,
        /// Configured ERC decimals exceed the native decimals by more than
        /// [`MAX_ERC_NATIVE_DECIMAL_GAP`](crate::MAX_ERC_NATIVE_DECIMAL_GAP), which
        /// would overflow the `u128` precision scale factor.
        /// 配置的 ERC 精度高于原生精度超过
        /// [`MAX_ERC_NATIVE_DECIMAL_GAP`](crate::MAX_ERC_NATIVE_DECIMAL_GAP)，会使
        /// `u128` 精度缩放因子溢出。
        ErcDecimalsTooHigh,
        /// Amount is below `MinBridgeAmount`. 金额低于 `MinBridgeAmount`。
        AmountBelowMin,
        /// Amount exceeds the per-tx limit. 金额超过单笔限额。
        PerTxLimitExceeded,
        /// Amount would exceed the daily limit. 金额将超过单日限额。
        DailyLimitExceeded,
        /// Insufficient spendable (free, ED-preserving) balance. 可用（free、保全 ED）余额不足。
        InsufficientFreeBalance,
        /// A ledger invariant would be violated. 将违反账本不变量。
        InvariantViolation,
        /// Arithmetic overflow. 算术溢出。
        Overflow,
        /// The ISMP dispatcher rejected the request. ISMP 派发器拒绝了该请求。
        DispatchFailed,
        /// No queued withdrawal exists for the given id. 给定 id 不存在排队提款。
        WithdrawNotFound,
        /// The queued withdrawal's veto window has not elapsed yet. 排队提款的否决窗口尚未结束。
        WithdrawNotDue,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T>
    where
        T::AccountId: Into<[u8; 32]>,
        BalanceOf<T>: Into<u128>,
        <T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
    {
        /// Bridge native NEX out to a registered EVM chain.
        ///
        /// Guardrails run **before** any burn: not paused, `>= MinBridgeAmount`,
        /// `<= PerTxMax`, daily window not exceeded, and the spend respects locks +
        /// ED (`KeepAlive`). NEX is then really burned (`TotalIssuance↓`), the
        /// in-flight ledger is bumped, and an ISMP POST carrying the vendored
        /// [`Message`] ABI is dispatched. The whole call is transactional: if the
        /// dispatch fails, the burn and ledger writes are rolled back.
        ///
        /// 桥出原生 NEX 到已注册的 EVM 链。
        ///
        /// 护栏在任何销毁**之前**执行：未暂停、`>= MinBridgeAmount`、`<= PerTxMax`、
        /// 未超单日窗口，且消费尊重 locks + ED（`KeepAlive`）。随后真销毁 NEX
        ///（`TotalIssuance↓`）、递增在途账本、派发携带 vendor [`Message`] ABI 的
        /// ISMP POST。整笔调用事务化：派发失败则销毁与账本写入回滚。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::bridge_out())]
        pub fn bridge_out(
            origin: OriginFor<T>,
            dest: StateMachine,
            recipient: H160,
            amount: BalanceOf<T>,
            relayer_fee: BalanceOf<T>,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let commitment = Self::do_outbound(&who, dest, recipient, amount, relayer_fee)?;
            Self::deposit_event(Event::<T>::BridgedOut {
                sender: who,
                recipient,
                dest,
                amount,
                commitment,
            });
            Ok(())
        }

        /// Governance: pause / resume (globally if `chain` is `None`).
        /// 治理：暂停 / 恢复（`chain` 为 `None` 即全局）。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_paused())]
        pub fn set_paused(
            origin: OriginFor<T>,
            chain: Option<StateMachine>,
            paused: bool,
        ) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            match chain {
                Some(c) => PausedChain::<T>::insert(c, paused),
                None => Paused::<T>::put(paused),
            }
            Self::deposit_event(Event::<T>::PausedSet { chain, paused });
            Ok(())
        }

        /// Governance: set per-tx and daily outbound limits.
        /// 治理：设置单笔与单日出站限额。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::set_limits())]
        pub fn set_limits(
            origin: OriginFor<T>,
            per_tx: BalanceOf<T>,
            daily: BalanceOf<T>,
        ) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            Limits::<T>::put(BridgeLimits { per_tx, daily });
            Self::deposit_event(Event::<T>::LimitsChanged { per_tx, daily });
            Ok(())
        }

        /// Governance: register or update an EVM chain (contract + ERC decimals).
        ///
        /// `erc_decimals` must satisfy `NativeDecimals <= erc_decimals` and
        /// `erc_decimals - NativeDecimals <=`
        /// [`MAX_ERC_NATIVE_DECIMAL_GAP`](crate::MAX_ERC_NATIVE_DECIMAL_GAP): the lower
        /// bound keeps the inbound 18→12 down-scaling from widening dust, the upper
        /// bound keeps `10^(erc_decimals - NativeDecimals)` within `u128`.
        ///
        /// 治理：注册或更新一条 EVM 链（合约 + ERC 精度）。
        ///
        /// `erc_decimals` 须满足 `NativeDecimals <= erc_decimals <= NativeDecimals +`
        /// [`MAX_ERC_NATIVE_DECIMAL_GAP`](crate::MAX_ERC_NATIVE_DECIMAL_GAP)：下界保证入站
        /// 18→12 缩小换算不放大 dust，上界保证 `10^(erc_decimals - NativeDecimals)` 不溢出 `u128`。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::register_chain())]
        pub fn register_chain(
            origin: OriginFor<T>,
            chain: StateMachine,
            contract: H160,
            erc_decimals: u8,
        ) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            ensure!(chain.is_evm(), Error::<T>::NotEvmChain);
            let native = T::NativeDecimals::get();
            ensure!(erc_decimals >= native, Error::<T>::ErcDecimalsBelowLocal);
            ensure!(
                erc_decimals.saturating_sub(native) <= crate::MAX_ERC_NATIVE_DECIMAL_GAP,
                Error::<T>::ErcDecimalsTooHigh
            );
            Chains::<T>::insert(
                chain,
                ChainConfig {
                    contract,
                    erc_decimals,
                },
            );
            Self::deposit_event(Event::<T>::ChainRegistered {
                chain,
                contract,
                erc_decimals,
            });
            Ok(())
        }

        /// Governance: deregister an EVM chain. Inbound/outbound for it then fail.
        /// 治理：注销一条 EVM 链。其入站/出站随后失败。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::deregister_chain())]
        pub fn deregister_chain(origin: OriginFor<T>, chain: StateMachine) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            Chains::<T>::remove(chain);
            Self::deposit_event(Event::<T>::ChainDeregistered { chain });
            Ok(())
        }

        /// Permissionless: prune stale [`PayoutRefunds`] entries (HB-WD-01 mechanism 2).
        ///
        /// Examines up to `limit` entries and removes those older than
        /// [`PayoutRefundTtl`](Config::PayoutRefundTtl) — tracked payouts that have
        /// definitively resolved (timed out and consumed, or delivered). This bounds
        /// storage growth with no deposit; because only past-TTL entries are removed,
        /// it can never delete a context a future `on_timeout` would still need, so it
        /// is safe to leave open to anyone (e.g. a keeper). `limit` caps the work and
        /// thus the weight; entries are visited in storage (hash) order, so repeated
        /// calls eventually cover the whole map.
        ///
        /// 无许可：清理过期的 [`PayoutRefunds`] 条目（HB-WD-01 机制 2）。
        ///
        /// 最多检查 `limit` 个条目，删除其中早于
        /// [`PayoutRefundTtl`](Config::PayoutRefundTtl) 的——即已确定解决的已跟踪派发（已超时
        /// 并被消费、或已投递）。以无押金方式限制状态增长；因只删除超过 TTL 的条目，永不会删掉
        /// 未来 `on_timeout` 仍需的上下文，故可安全开放给任何人（如 keeper）。`limit` 限制工作量
        /// 与权重；按存储（哈希）序遍历，反复调用最终覆盖整个映射。
        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::prune_payout_refunds(*limit))]
        pub fn prune_payout_refunds(origin: OriginFor<T>, limit: u32) -> DispatchResult {
            ensure_signed(origin)?;
            let now = frame_system::Pallet::<T>::block_number();
            let ttl = T::PayoutRefundTtl::get();
            let stale: alloc::vec::Vec<crate::types::Commitment> = PayoutRefunds::<T>::iter()
                .take(limit as usize)
                .filter_map(|(commitment, (dispatched_at, _meta))| {
                    (now.saturating_sub(dispatched_at) >= ttl).then_some(commitment)
                })
                .collect();
            let removed = stale.len() as u32;
            for commitment in stale {
                PayoutRefunds::<T>::remove(commitment);
            }
            Self::deposit_event(Event::<T>::PayoutRefundsPruned { removed });
            Ok(())
        }

        /// Permissionless: execute a queued derived-account withdrawal (H2 phase 2)
        /// whose veto window ([`WithdrawDelay`](Config::WithdrawDelay)) has elapsed.
        ///
        /// Burns the queued amount from the derived owner and dispatches the outbound
        /// POST (same core as `bridge_out`). The extrinsic is auto-transactional, so if
        /// the burn or dispatch fails (insufficient balance, paused, deregistered,
        /// limit) nothing is committed and the entry stays queued for a later retry.
        /// Open to anyone (e.g. a keeper) because the entry was already authorised by
        /// the gateway and survived the guardian veto window.
        ///
        /// 无许可：执行一笔否决窗口（[`WithdrawDelay`](Config::WithdrawDelay)）已结束的排队派生
        /// 账户提款（H2 第二阶段）。
        ///
        /// 从派生持有人销毁排队金额并派发出站 POST（与 `bridge_out` 同核心）。本 extrinsic 自动
        /// 事务化，故若销毁或派发失败（余额不足、暂停、已注销、超限）则不提交任何写入，条目保留以便重试。
        /// 因条目已由网关鉴权且度过 guardian 否决窗口，故开放给任何人（如 keeper）。
        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::execute_withdraw())]
        pub fn execute_withdraw(origin: OriginFor<T>, id: u64) -> DispatchResult {
            ensure_signed(origin)?;
            let pending = PendingWithdraws::<T>::get(id).ok_or(Error::<T>::WithdrawNotFound)?;
            let now = frame_system::Pallet::<T>::block_number();
            ensure!(now >= pending.execute_at, Error::<T>::WithdrawNotDue);
            let recipient = H160(pending.recipient);
            let commitment = Self::do_outbound(
                &pending.owner,
                pending.dest,
                recipient,
                pending.amount,
                Zero::zero(),
            )?;
            PendingWithdraws::<T>::remove(id);
            Self::deposit_event(Event::<T>::DerivedWithdraw {
                id,
                owner: pending.owner,
                dest: pending.dest,
                recipient,
                amount: pending.amount,
                commitment,
            });
            Ok(())
        }

        /// Guardian veto: cancel a queued derived-account withdrawal before it executes
        /// (H2 containment). No funds were ever moved, so cancelling simply drops the
        /// entry — use this when a withdrawal is suspected to stem from a compromised or
        /// buggy gateway. Restricted to [`BridgeOrigin`](Config::BridgeOrigin).
        /// guardian 否决：在执行前取消一笔排队的派生账户提款（H2 收敛）。从未移动任何资金，故取消
        /// 仅删除条目——当怀疑提款源自被攻破/有 bug 的网关时使用。限
        /// [`BridgeOrigin`](Config::BridgeOrigin)。
        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::cancel_withdraw())]
        pub fn cancel_withdraw(origin: OriginFor<T>, id: u64) -> DispatchResult {
            T::BridgeOrigin::ensure_origin(origin)?;
            ensure!(
                PendingWithdraws::<T>::take(id).is_some(),
                Error::<T>::WithdrawNotFound
            );
            Self::deposit_event(Event::<T>::WithdrawCancelled { id });
            Ok(())
        }
    }

    impl<T> Default for Pallet<T> {
        fn default() -> Self {
            Self(PhantomData)
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            Self::check_ledger_invariant().map_err(Into::into)
        }
    }

    impl<T: Config> Pallet<T> {
        /// Invariant: `Σ BridgedOutByChain == BridgedOut`.
        /// 不变量：`Σ BridgedOutByChain == BridgedOut`。
        pub fn check_ledger_invariant() -> Result<(), &'static str> {
            let mut sum: BalanceOf<T> = Zero::zero();
            for (_chain, amount) in BridgedOutByChain::<T>::iter() {
                sum = sum
                    .checked_add(&amount)
                    .ok_or("BridgedOutByChain sum overflow")?;
            }
            if sum != BridgedOut::<T>::get() {
                return Err("Σ BridgedOutByChain != BridgedOut");
            }
            Ok(())
        }

        /// Queue a time-locked derived-account withdrawal (H2 two-phase flow) and
        /// return its `(id, execute_at)`. No balance is touched here — the burn +
        /// outbound POST happen later in [`execute_withdraw`](Self::execute_withdraw),
        /// once `execute_at` is reached and the guardian has not vetoed it.
        /// 排队一笔时间锁的派生账户提款（H2 两阶段流程），返回其 `(id, execute_at)`。此处不动用任何
        /// 余额——销毁 + 出站 POST 在到达 `execute_at` 且 guardian 未否决后由
        /// [`execute_withdraw`](Self::execute_withdraw) 执行。
        pub(crate) fn queue_withdraw(
            owner: T::AccountId,
            dest: StateMachine,
            recipient: [u8; 20],
            amount: BalanceOf<T>,
        ) -> (u64, BlockNumberFor<T>) {
            let id = NextWithdrawId::<T>::mutate(|n| {
                let id = *n;
                *n = n.saturating_add(1);
                id
            });
            let execute_at =
                frame_system::Pallet::<T>::block_number().saturating_add(T::WithdrawDelay::get());
            PendingWithdraws::<T>::insert(
                id,
                crate::types::PendingWithdraw {
                    owner,
                    dest,
                    recipient,
                    amount,
                    execute_at,
                },
            );
            (id, execute_at)
        }

        /// Reusable outbound core, shared by the signed `bridge_out` extrinsic and the
        /// derived-account withdraw path ([`InboundOp::Withdraw`](crate::types::InboundOp)).
        /// Runs the outbound guardrails, really burns `amount` NEX from `who` (`KeepAlive`,
        /// `TotalIssuance↓`), books the in-flight ledger, and dispatches the ISMP POST
        /// carrying the vendored [`Message`] ABI; returns the request commitment. Callers
        /// must run this inside a storage layer (extrinsics are auto-wrapped; the inbound
        /// withdraw wraps it explicitly) so a dispatch failure rolls back burn + ledger.
        /// 可复用的出站核心，由签名 `bridge_out` extrinsic 与派生账户提款路径
        /// （[`InboundOp::Withdraw`](crate::types::InboundOp)）共用。执行出站护栏、对 `who`
        /// 真销毁 `amount` NEX（`KeepAlive`、`TotalIssuance↓`）、记在途账本、派发携带 vendor
        /// [`Message`] ABI 的 ISMP POST，并返回请求 commitment。调用方须在存储层内执行
        ///（extrinsic 自动包裹；入站提款显式包裹），以便派发失败时回滚销毁与账本。
        pub fn do_outbound(
            who: &T::AccountId,
            dest: StateMachine,
            recipient: H160,
            amount: BalanceOf<T>,
            relayer_fee: BalanceOf<T>,
        ) -> Result<crate::types::Commitment, sp_runtime::DispatchError>
        where
            T::AccountId: Into<[u8; 32]>,
            BalanceOf<T>: Into<u128>,
            <T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
        {
            // --- Guardrails (before burn) / 护栏（销毁前） ---
            ensure!(!Paused::<T>::get(), Error::<T>::BridgePaused);
            ensure!(!PausedChain::<T>::get(dest), Error::<T>::BridgePaused);
            let cfg = Chains::<T>::get(dest).ok_or(Error::<T>::ChainNotRegistered)?;
            ensure!(
                amount >= T::MinBridgeAmount::get(),
                Error::<T>::AmountBelowMin
            );

            let limits = Limits::<T>::get();
            ensure!(amount <= limits.per_tx, Error::<T>::PerTxLimitExceeded);

            let now = frame_system::Pallet::<T>::block_number();
            let (window_start, used) = DailyOut::<T>::get();
            let (window_start, used) =
                if now.saturating_sub(window_start) >= T::DailyLimitWindow::get() {
                    (now, Zero::zero())
                } else {
                    (window_start, used)
                };
            let new_used = used.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
            ensure!(new_used <= limits.daily, Error::<T>::DailyLimitExceeded);

            // --- Burn (real, respects locks + ED) / 销毁（真实，尊重 locks + ED） ---
            let negative = T::NativeCurrency::withdraw(
                who,
                amount,
                WithdrawReasons::TRANSFER,
                ExistenceRequirement::KeepAlive,
            )
            .map_err(|_| Error::<T>::InsufficientFreeBalance)?;
            drop(negative);

            // --- Ledger / 账本 ---
            BridgedOut::<T>::try_mutate(|b| -> DispatchResult {
                *b = b.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
                Ok(())
            })?;
            BridgedOutByChain::<T>::try_mutate(dest, |b| -> DispatchResult {
                *b = b.checked_add(&amount).ok_or(Error::<T>::Overflow)?;
                Ok(())
            })?;
            DailyOut::<T>::put((window_start, new_used));

            // --- Encode + dispatch ISMP POST / 编码并派发 ISMP POST ---
            let sender: [u8; 32] = who.clone().into();
            let erc20_amount =
                impls::convert_to_erc20(amount.into(), cfg.erc_decimals, T::NativeDecimals::get());
            let message = Message {
                from: sender.to_vec().into(),
                to: recipient.0.to_vec().into(),
                amount: alloy_primitives::U256::from_be_bytes(erc20_amount.to_big_endian()),
                data: Default::default(),
            };
            let post = DispatchPost {
                dest,
                from: module_id_bytes(),
                to: cfg.contract.0.to_vec(),
                timeout: T::RequestTimeout::get(),
                body: {
                    use alloy_sol_types::SolValue;
                    Message::abi_encode(&message)
                },
            };
            let fee =
                <<T as pallet_ismp::Config>::Balance as From<BalanceOf<T>>>::from(relayer_fee);
            let metadata = FeeMetadata {
                payer: who.clone(),
                fee,
            };
            let commitment = <T as Config>::Dispatcher::default()
                .dispatch_request(DispatchRequest::Post(post), metadata)
                .map_err(|_| Error::<T>::DispatchFailed)?;
            Ok(commitment)
        }

        /// Like [`do_outbound`](Self::do_outbound) but additionally records an opaque
        /// `meta` against the request commitment (HB-WD-01 mechanism 2). If the POST
        /// later times out, `on_timeout` hands `meta` to the
        /// [`PayoutRefundHandler`](crate::types::PayoutRefundHandler) so the business
        /// layer can compensate (e.g. restore a promoter's `pending`) on top of the
        /// bridge's re-mint to the sender. The burn, ledger and dispatch are identical
        /// to `do_outbound`; only the extra `PayoutRefunds` write is added. Must run
        /// inside a storage layer (the extrinsic auto-wraps) so a dispatch failure
        /// rolls back the burn, ledger and meta together.
        /// 与 [`do_outbound`](Self::do_outbound) 相同，但额外按请求 commitment 记录不透明 `meta`
        ///（HB-WD-01 机制 2）。若该 POST 之后超时，`on_timeout` 会把 `meta` 交给
        /// [`PayoutRefundHandler`](crate::types::PayoutRefundHandler)，使业务层在桥向发送方重铸
        /// 之上再做补偿（例如恢复推广员 `pending`）。销毁、账本与派发与 `do_outbound` 完全一致，
        /// 仅多一次 `PayoutRefunds` 写入。须在存储层内执行（extrinsic 自动包裹），以便派发失败时
        /// 一并回滚销毁、账本与 meta。
        pub fn do_outbound_tracked(
            who: &T::AccountId,
            dest: StateMachine,
            recipient: H160,
            amount: BalanceOf<T>,
            relayer_fee: BalanceOf<T>,
            meta: BoundedVec<u8, T::MaxPayoutMeta>,
        ) -> Result<crate::types::Commitment, sp_runtime::DispatchError>
        where
            T::AccountId: Into<[u8; 32]>,
            BalanceOf<T>: Into<u128>,
            <T as pallet_ismp::Config>::Balance: From<BalanceOf<T>>,
        {
            let commitment = Self::do_outbound(who, dest, recipient, amount, relayer_fee)?;
            let now = frame_system::Pallet::<T>::block_number();
            PayoutRefunds::<T>::insert(commitment, (now, meta));
            Ok(commitment)
        }

        /// Decodes a 20-byte (EVM) or 32-byte (Substrate) address into an `AccountId`.
        ///
        /// A 32-byte payload is a native Substrate account used verbatim. A 20-byte
        /// payload is an EVM address derived through the **same** [`EvmToSubstrate`]
        /// mapping used by cross-order and withdraw, so a plain inbound transfer to an
        /// EVM address and a later [`InboundOp::Withdraw`](crate::types::InboundOp) of
        /// that balance resolve to one account — otherwise NEX bridged to a bare EVM
        /// address would land in an account the withdraw path could never debit
        /// (stranded funds, HB-ENT-01 §7).
        /// 将 20 字节（EVM）或 32 字节（Substrate）地址解码为 `AccountId`。
        ///
        /// 32 字节载荷为原生 Substrate 账户，原样使用。20 字节载荷为 EVM 地址，用与跨链下单 /
        /// 提款**相同**的 [`EvmToSubstrate`] 映射派生，使「纯转账入站到某 EVM 地址」与之后对该
        /// 余额的 [`InboundOp::Withdraw`](crate::types::InboundOp) 落到同一账户——否则桥到裸 EVM
        /// 地址的 NEX 会进入提款路径永远扣不到的账户（资金冻结，HB-ENT-01 §7）。
        pub(crate) fn account_from_bytes(
            bytes: &[u8],
        ) -> Result<T::AccountId, crate::error::BridgeError>
        where
            T::AccountId: From<[u8; 32]>,
        {
            match bytes.len() {
                32 => {
                    let mut buf = [0u8; 32];
                    buf.copy_from_slice(bytes);
                    Ok(buf.into())
                }
                20 => {
                    let mut addr = [0u8; 20];
                    addr.copy_from_slice(bytes);
                    Ok(<T::EvmToSubstrate as EvmToSubstrate<T>>::convert(H160(
                        addr,
                    )))
                }
                other => Err(crate::error::BridgeError::InvalidRecipientLength(other)),
            }
        }

        /// Inbound credit: mint `amount` NEX to `beneficiary` and decrement the
        /// in-flight ledger for `chain`. Enforces `amount <= BridgedOut` and
        /// `amount <= BridgedOutByChain[chain]` (anti-inflation invariant). All
        /// checks precede the (infallible) mint, so no partial state is possible.
        /// 入站入账：向 `beneficiary` 铸造 `amount` NEX，并对 `chain` 递减在途账本。
        /// 强制 `amount <= BridgedOut` 与 `amount <= BridgedOutByChain[chain]`（防增发
        /// 不变量）。所有校验先于（不可失败的）铸造，故不会留下部分状态。
        pub(crate) fn credit_inbound(
            beneficiary: &T::AccountId,
            chain: StateMachine,
            amount: BalanceOf<T>,
        ) -> Result<(), crate::error::BridgeError> {
            let total = BridgedOut::<T>::get();
            let per_chain = BridgedOutByChain::<T>::get(chain);
            if amount > total || amount > per_chain {
                return Err(crate::error::BridgeError::InvariantViolation);
            }
            BridgedOut::<T>::put(total.saturating_sub(amount));
            BridgedOutByChain::<T>::insert(chain, per_chain.saturating_sub(amount));
            // Dropping the PositiveImbalance increases TotalIssuance (the mint).
            // 丢弃 PositiveImbalance 会增加 TotalIssuance（即铸造）。
            drop(T::NativeCurrency::deposit_creating(beneficiary, amount));
            Ok(())
        }

        /// Converts an ERC-20 `U256` amount (big-endian alloy value) into local NEX,
        /// using the per-chain ERC decimals. Enforces `>= MinBridgeAmount`.
        /// 用逐链 ERC 精度，将 ERC-20 `U256` 金额（big-endian alloy 值）换算为本地 NEX。
        /// 强制 `>= MinBridgeAmount`。
        pub(crate) fn decode_amount(
            erc_amount: alloy_primitives::U256,
            erc_decimals: u8,
        ) -> Result<BalanceOf<T>, crate::error::BridgeError>
        where
            BalanceOf<T>: core::str::FromStr,
        {
            let amount = impls::convert_to_balance::<BalanceOf<T>>(
                U256::from_big_endian(&erc_amount.to_be_bytes::<32>()),
                erc_decimals,
                T::NativeDecimals::get(),
            )
            .map_err(|_| {
                crate::error::BridgeError::InvalidAmountConversion(alloc::format!(
                    "amount conversion failed"
                ))
            })?;
            if amount < T::MinBridgeAmount::get() {
                return Err(crate::error::BridgeError::AmountBelowMin);
            }
            Ok(amount)
        }
    }
}
