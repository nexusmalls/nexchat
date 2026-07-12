// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Prediction subsystem governance modes and dispatch-call classification.
//! 预测子系统治理模式与 dispatch 调用分类。
//!
//! This pallet stores only global and per-module gates. Runtime call filtering
//! remains a Phase 6 integration responsibility; this crate deliberately does
//! not depend on a concrete `RuntimeCall`.
//! 本 pallet 仅存储全局与逐模块门禁。Runtime 调用过滤留待 Phase 6 集成；
//! 本 crate 有意不依赖具体 `RuntimeCall`。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use weights::WeightInfo;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Global operating mode for the prediction subsystem.
/// 预测子系统的全局运行模式。
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Default,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    TypeInfo,
)]
pub enum PredictionMode {
    /// Only unwind and governance recovery calls are admitted.
    /// 仅允许退出风险与治理恢复调用。
    #[default]
    Disabled,
    /// Resolution, unwind, and governance recovery calls are admitted.
    /// 允许争议解决、退出风险与治理恢复调用。
    ResolutionOnly,
    /// All call classes are admitted for enabled modules.
    /// 对已启用模块允许所有调用类别。
    Trading,
    /// All call classes are admitted for enabled modules.
    /// 对已启用模块允许所有调用类别。
    Full,
}

/// Stable identifiers for the twelve prediction business modules.
/// 十二个预测业务模块的稳定标识。
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    Ord,
    PartialEq,
    PartialOrd,
    TypeInfo,
)]
pub enum PredictionModule {
    PredictionMarkets,
    Authorized,
    Court,
    GlobalDisputes,
    LegacySwaps,
    NeoSwaps,
    Orderbook,
    Parimutuel,
    HybridRouter,
    CombinatorialTokens,
    Futarchy,
    Styx,
}

/// Safety class assigned to a prediction dispatchable.
/// 分配给预测 dispatchable 的安全类别。
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    DecodeWithMemTracking,
    Encode,
    Eq,
    MaxEncodedLen,
    PartialEq,
    TypeInfo,
)]
pub enum CallClass {
    RiskIncreasing,
    Resolution,
    Unwind,
    AdminRecovery,
}

/// Static description of one currently ported dispatchable.
/// 当前已移植 dispatchable 的静态描述。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallRegistryEntry {
    pub module: PredictionModule,
    pub call_index: u8,
    pub name: &'static str,
    pub class: CallClass,
}

/// Returns whether a global mode admits a call class.
/// 返回全局模式是否允许指定调用类别。
pub const fn mode_allows(mode: PredictionMode, class: CallClass) -> bool {
    match mode {
        PredictionMode::Disabled => {
            matches!(class, CallClass::Unwind | CallClass::AdminRecovery)
        }
        PredictionMode::ResolutionOnly => {
            matches!(
                class,
                CallClass::Resolution | CallClass::Unwind | CallClass::AdminRecovery
            )
        }
        PredictionMode::Trading | PredictionMode::Full => true,
    }
}

/// Applies both the global mode and business-module gate.
/// 同时应用全局模式与业务模块门禁。
///
/// Prediction-control governance calls are intentionally absent: Phase 6 must
/// exempt them before consulting this function.
/// Prediction-control 治理调用有意不在此处：Phase 6 必须先将其自豁免，再调用本函数。
pub const fn is_call_allowed(mode: PredictionMode, module_enabled: bool, class: CallClass) -> bool {
    module_enabled && mode_allows(mode, class)
}

/// Read-only boundary used by a future runtime call filter.
/// 供未来 runtime call filter 使用的只读边界。
pub trait PredictionControlApi {
    /// Returns the current global mode.
    /// 返回当前全局模式。
    fn prediction_mode() -> PredictionMode;

    /// Returns whether a business module is enabled.
    /// 返回业务模块是否已启用。
    fn module_enabled(module: PredictionModule) -> bool;

    /// Evaluates the combined mode and module gate.
    /// 评估全局模式与模块开关组合门禁。
    fn call_allowed(module: PredictionModule, class: CallClass) -> bool {
        is_call_allowed(Self::prediction_mode(), Self::module_enabled(module), class)
    }
}

macro_rules! call {
    ($module:ident, $index:literal, $name:literal, $class:ident) => {
        CallRegistryEntry {
            module: PredictionModule::$module,
            call_index: $index,
            name: $name,
            class: CallClass::$class,
        }
    };
}

/// Registry of all 68 dispatchables currently present in the twelve ported pallets.
/// 十二个已移植 pallet 当前全部 68 个 dispatchable 的注册表。
///
/// `create_market_and_deploy_pool` is classified under PredictionMarkets and
/// requires an additional NeoSwaps gate in Phase 6. HybridRouter calls likewise
/// require both NeoSwaps and Orderbook gates when runtime filtering is wired.
/// `create_market_and_deploy_pool` 归类到 PredictionMarkets，Phase 6 还必须额外检查
/// NeoSwaps 门禁。HybridRouter 调用在 runtime 接线时同样必须同时检查 NeoSwaps 与
/// Orderbook 门禁。
pub const CALL_REGISTRY: &[CallRegistryEntry] = &[
    call!(
        PredictionMarkets,
        1,
        "admin_move_market_to_closed",
        AdminRecovery
    ),
    call!(
        PredictionMarkets,
        2,
        "admin_move_market_to_resolved",
        AdminRecovery
    ),
    call!(PredictionMarkets, 3, "approve_market", RiskIncreasing),
    call!(PredictionMarkets, 4, "request_edit", AdminRecovery),
    call!(PredictionMarkets, 5, "buy_complete_set", RiskIncreasing),
    call!(PredictionMarkets, 6, "dispute", Resolution),
    call!(PredictionMarkets, 8, "create_market", RiskIncreasing),
    call!(PredictionMarkets, 9, "edit_market", RiskIncreasing),
    call!(PredictionMarkets, 12, "redeem_shares", Unwind),
    call!(PredictionMarkets, 13, "reject_market", AdminRecovery),
    call!(PredictionMarkets, 14, "report", Resolution),
    call!(PredictionMarkets, 15, "sell_complete_set", Unwind),
    call!(PredictionMarkets, 16, "start_global_dispute", Resolution),
    call!(
        PredictionMarkets,
        17,
        "create_market_and_deploy_pool",
        RiskIncreasing
    ),
    call!(
        PredictionMarkets,
        18,
        "schedule_early_close",
        RiskIncreasing
    ),
    call!(PredictionMarkets, 19, "dispute_early_close", Resolution),
    call!(PredictionMarkets, 20, "reject_early_close", AdminRecovery),
    call!(PredictionMarkets, 21, "close_trusted_market", Resolution),
    call!(
        PredictionMarkets,
        22,
        "manually_close_market",
        AdminRecovery
    ),
    call!(Authorized, 0, "authorize_market_outcome", Resolution),
    call!(Court, 0, "join_court", RiskIncreasing),
    call!(Court, 1, "delegate", RiskIncreasing),
    call!(Court, 2, "prepare_exit_court", Unwind),
    call!(Court, 3, "exit_court", Unwind),
    call!(Court, 4, "vote", Resolution),
    call!(Court, 5, "denounce_vote", Resolution),
    call!(Court, 6, "reveal_vote", Resolution),
    call!(Court, 7, "appeal", Resolution),
    call!(Court, 8, "reassign_court_stakes", Unwind),
    call!(Court, 9, "set_inflation", AdminRecovery),
    call!(GlobalDisputes, 0, "add_vote_outcome", Resolution),
    call!(GlobalDisputes, 1, "purge_outcomes", Unwind),
    call!(GlobalDisputes, 2, "reward_outcome_owner", Resolution),
    call!(GlobalDisputes, 3, "vote_on_outcome", Resolution),
    call!(GlobalDisputes, 4, "unlock_vote_balance", Unwind),
    call!(GlobalDisputes, 5, "refund_vote_fees", Unwind),
    call!(LegacySwaps, 1, "pool_exit", Unwind),
    call!(LegacySwaps, 3, "pool_exit_with_exact_asset_amount", Unwind),
    call!(LegacySwaps, 4, "pool_exit_with_exact_pool_amount", Unwind),
    call!(LegacySwaps, 5, "pool_join", RiskIncreasing),
    call!(
        LegacySwaps,
        7,
        "pool_join_with_exact_asset_amount",
        RiskIncreasing
    ),
    call!(
        LegacySwaps,
        8,
        "pool_join_with_exact_pool_amount",
        RiskIncreasing
    ),
    call!(LegacySwaps, 9, "swap_exact_amount_in", RiskIncreasing),
    call!(LegacySwaps, 10, "swap_exact_amount_out", RiskIncreasing),
    call!(LegacySwaps, 11, "force_pool_exit", AdminRecovery),
    call!(NeoSwaps, 0, "buy", RiskIncreasing),
    call!(NeoSwaps, 1, "sell", Unwind),
    call!(NeoSwaps, 2, "join", RiskIncreasing),
    call!(NeoSwaps, 3, "exit", Unwind),
    call!(NeoSwaps, 4, "withdraw_fees", Unwind),
    call!(NeoSwaps, 5, "deploy_pool", RiskIncreasing),
    call!(NeoSwaps, 6, "combo_buy", RiskIncreasing),
    // `combo_sell` includes buy legs that increase pool reserves, so it is not
    // a pure unwind operation.
    // `combo_sell` 包含增加池储备的买入腿，因此不是纯退出操作。
    call!(NeoSwaps, 7, "combo_sell", RiskIncreasing),
    call!(NeoSwaps, 8, "deploy_combinatorial_pool", RiskIncreasing),
    call!(Orderbook, 0, "remove_order", Unwind),
    call!(Orderbook, 1, "fill_order", RiskIncreasing),
    call!(Orderbook, 2, "place_order", RiskIncreasing),
    call!(Parimutuel, 0, "buy", RiskIncreasing),
    call!(Parimutuel, 1, "claim_rewards", Unwind),
    call!(Parimutuel, 2, "claim_refunds", Unwind),
    call!(HybridRouter, 0, "buy", RiskIncreasing),
    call!(HybridRouter, 1, "sell", Unwind),
    call!(CombinatorialTokens, 0, "split_position", RiskIncreasing),
    call!(CombinatorialTokens, 1, "merge_position", Unwind),
    call!(CombinatorialTokens, 2, "redeem_position", Unwind),
    call!(Futarchy, 0, "submit_proposal", RiskIncreasing),
    call!(Styx, 0, "cross", RiskIncreasing),
    call!(Styx, 1, "set_burn_amount", AdminRecovery),
];

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::EnsureOrigin};
    use frame_system::pallet_prelude::*;

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// Runtime configuration for prediction governance controls.
    /// 预测治理控制的 runtime 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Governance origin allowed to change modes and module gates.
        /// 可修改模式与模块门禁的治理来源。
        type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Weight provider; Phase 2 uses non-production estimates and Phase 7
        /// must replace them with generated benchmark weights.
        /// 权重提供者；Phase 2 使用非生产估算值，Phase 7 必须替换为 benchmark 生成权重。
        type WeightInfo: WeightInfo;
    }

    /// Current global prediction mode; defaults to `Disabled`.
    /// 当前预测全局模式；默认值为 `Disabled`。
    #[pallet::storage]
    #[pallet::getter(fn prediction_mode)]
    pub type GlobalMode<T: Config> = StorageValue<_, PredictionMode, ValueQuery>;

    /// Explicit per-business-module gate; every key defaults to `false`.
    /// 显式逐业务模块门禁；每个 key 默认均为 `false`。
    #[pallet::storage]
    #[pallet::getter(fn module_enabled)]
    pub type ModuleEnabled<T: Config> =
        StorageMap<_, Blake2_128Concat, PredictionModule, bool, ValueQuery>;

    /// Governance-control events.
    /// 治理控制事件。
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Global mode changed from `old` to `new`.
        /// 全局模式从 `old` 变更为 `new`。
        PredictionModeSet {
            old: PredictionMode,
            new: PredictionMode,
        },
        /// A business-module gate was updated.
        /// 业务模块门禁已更新。
        ModuleEnabledSet {
            module: PredictionModule,
            enabled: bool,
        },
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Sets the global prediction operating mode.
        /// 设置预测全局运行模式。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::set_prediction_mode())]
        pub fn set_prediction_mode(origin: OriginFor<T>, new: PredictionMode) -> DispatchResult {
            T::UpdateOrigin::ensure_origin(origin)?;
            let old = GlobalMode::<T>::get();
            GlobalMode::<T>::put(new);
            Self::deposit_event(Event::PredictionModeSet { old, new });
            Ok(())
        }

        /// Enables or disables one prediction business module.
        /// 启用或禁用一个预测业务模块。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::set_module_enabled())]
        pub fn set_module_enabled(
            origin: OriginFor<T>,
            module: PredictionModule,
            enabled: bool,
        ) -> DispatchResult {
            T::UpdateOrigin::ensure_origin(origin)?;
            ModuleEnabled::<T>::insert(module, enabled);
            Self::deposit_event(Event::ModuleEnabledSet { module, enabled });
            Ok(())
        }
    }
}

impl<T: Config> PredictionControlApi for Pallet<T> {
    fn prediction_mode() -> PredictionMode {
        GlobalMode::<T>::get()
    }

    fn module_enabled(module: PredictionModule) -> bool {
        ModuleEnabled::<T>::get(module)
    }
}
