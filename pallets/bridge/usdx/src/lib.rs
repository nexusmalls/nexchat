// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0

//! # USDX Peg Stability Module
//!
//! Fully collateralized USDX minting and redemption against authenticated bridge
//! receipt assets. Cross-chain verification remains the responsibility of the HFT
//! layer; this pallet only performs Nexus-local accounting and risk controls.
//!
//! # USDX 锚定稳定模块
//!
//! 使用经过认证的跨链收据资产，执行足额抵押的 USDX 铸造与赎回。跨链验证由 HFT
//! 层负责；本 pallet 只处理 Nexus 链内会计与风险控制。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod types;
pub mod weights;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use types::{
    CollateralPolicy, LaneActivationEvidence, LaneConfig, LaneLimits, WindowDirection, WindowUsage,
    BPS_DENOMINATOR,
};
pub use weights::WeightInfo;

/// Runtime-specific setup required by USDX benchmarks.
/// USDX benchmark 所需的 runtime 专用初始化。
#[cfg(feature = "runtime-benchmarks")]
pub trait BenchmarkHelper<T: Config> {
    /// Creates protocol assets and canonical receipt registry state.
    /// 创建协议资产和规范 receipt registry 状态。
    fn prepare();

    /// Returns activation evidence accepted by the runtime receipt validator.
    /// 返回 runtime receipt validator 可接受的激活证据。
    fn evidence(receipt_asset_id: u64) -> LaneActivationEvidence;
}

use codec::Encode;
use frame_support::PalletId;
use sp_core::H256;

/// Canonical receipt metadata supplied by the HFT registry adapter.
/// HFT registry adapter 提供的规范收据元数据。
pub trait ReceiptValidator {
    /// Returns the canonical descriptor hash for a registered receipt asset.
    /// 返回已注册收据资产的规范描述哈希。
    fn descriptor_hash(asset_id: u64) -> Option<H256>;

    /// Validates governance evidence against the canonical receipt descriptor.
    /// 根据规范收据描述验证治理提交的激活证据。
    fn validate_evidence(
        asset_id: u64,
        descriptor_hash: H256,
        evidence: &LaneActivationEvidence,
    ) -> bool;
}

/// Deny-all adapter used while no authenticated HFT receipt registry is wired.
/// 未接入已认证 HFT 收据注册表时使用的全拒绝 adapter。
impl ReceiptValidator for () {
    fn descriptor_hash(_asset_id: u64) -> Option<H256> {
        None
    }

    fn validate_evidence(
        _asset_id: u64,
        _descriptor_hash: H256,
        _evidence: &LaneActivationEvidence,
    ) -> bool {
        false
    }
}

/// Runtime adapter for validating protocol-level pallet-assets configuration.
/// 用于验证协议级 pallet-assets 配置的 runtime adapter。
pub trait ProtocolAssetInspector<AccountId> {
    /// Validates USDX configuration and its PSM sovereign account.
    /// 验证 USDX 配置及其 PSM sovereign account。
    fn validate_usdx(asset_id: u64, psm_account: &AccountId) -> bool;

    /// Validates an imported receipt asset's fixed runtime configuration.
    /// 验证 imported receipt asset 的固定 runtime 配置。
    fn validate_receipt(asset_id: u64) -> bool;
}

/// Deny-all protocol asset inspector for an inert runtime deployment.
/// 用于 runtime 惰性部署的全拒绝协议资产检查器。
impl<AccountId> ProtocolAssetInspector<AccountId> for () {
    fn validate_usdx(_asset_id: u64, _psm_account: &AccountId) -> bool {
        false
    }

    fn validate_receipt(_asset_id: u64) -> bool {
        false
    }
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use crate::types::{
        CollateralPolicy, LaneActivationEvidence, LaneConfig, LaneLimits, WindowDirection,
        WindowUsage, BPS_DENOMINATOR,
    };
    use frame_support::{
        pallet_prelude::*,
        traits::{
            fungibles::{Inspect, Mutate},
            tokens::{Fortitude, Precision, Preservation},
            EnsureOrigin,
        },
        transactional,
    };
    use frame_system::pallet_prelude::*;
    use sp_core::U256;
    use sp_runtime::traits::{AccountIdConversion, Saturating, Zero};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// USDX PSM runtime configuration.
    /// USDX PSM runtime 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Multi-asset ledger containing USDX and receipt assets.
        /// 承载 USDX 与收据资产的多资产账本。
        type Assets: Inspect<Self::AccountId, AssetId = u64, Balance = u128>
            + Mutate<Self::AccountId>;

        /// Governance origin for lane registration and risk configuration.
        /// 通道注册与风险配置治理来源。
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Emergency origin for global and per-lane pause.
        /// 全局与逐通道紧急暂停来源。
        type PauseOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// HFT-backed canonical receipt registry.
        /// HFT 支持的规范收据注册表。
        type ReceiptValidator: ReceiptValidator;

        /// pallet-assets protocol configuration inspector.
        /// pallet-assets 协议配置检查器。
        type ProtocolAssetInspector: ProtocolAssetInspector<Self::AccountId>;

        /// Fixed USDX AssetId.
        /// 固定 USDX AssetId。
        #[pallet::constant]
        type UsdxAssetId: Get<u64>;

        /// PalletId deriving the PSM collateral account.
        /// 用于派生 PSM 抵押账户的 PalletId。
        #[pallet::constant]
        type PsmPalletId: Get<PalletId>;

        type WeightInfo: WeightInfo;

        /// Runtime-specific benchmark setup.
        /// Runtime 专用 benchmark 初始化。
        #[cfg(feature = "runtime-benchmarks")]
        type BenchmarkHelper: BenchmarkHelper<Self>;
    }

    #[pallet::storage]
    #[pallet::getter(fn collateral_config)]
    pub type CollateralConfigs<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, LaneConfig, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collateral_policy)]
    pub type CollateralPolicies<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, CollateralPolicy, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collateral_limits)]
    pub type CollateralLimits<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, LaneLimits<BlockNumberFor<T>>, OptionQuery>;

    /// Activation evidence accepted when the lane was registered or updated.
    /// 通道注册或更新时接受的激活证据。
    #[pallet::storage]
    #[pallet::getter(fn lane_evidence)]
    pub type LaneEvidence<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, LaneActivationEvidence, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collateral_debt)]
    pub type CollateralUsdxDebt<T: Config> = StorageMap<_, Blake2_128Concat, u64, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_debt)]
    pub type TotalUsdxDebt<T: Config> = StorageValue<_, u128, ValueQuery>;

    /// USDX debt credited by Root admin faucet (not attributed to a receipt lane).
    /// Root 水龙头记入的 USDX 债务（不归属任一 receipt 通道）。
    #[pallet::storage]
    #[pallet::getter(fn admin_debt)]
    pub type AdminUsdxDebt<T: Config> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn global_debt_ceiling)]
    pub type GlobalUsdxDebtCeiling<T: Config> = StorageValue<_, u128, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn global_paused)]
    pub type GlobalPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collateral_paused)]
    pub type CollateralPaused<T: Config> = StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    #[pallet::storage]
    pub type MintWindow<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, WindowUsage<BlockNumberFor<T>>, ValueQuery>;

    #[pallet::storage]
    pub type RedeemWindow<T: Config> =
        StorageMap<_, Blake2_128Concat, u64, WindowUsage<BlockNumberFor<T>>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Receipt deposited and net USDX minted.
        /// 收据已存入并铸造净额 USDX。
        UsdxMinted {
            beneficiary: T::AccountId,
            receipt_asset_id: u64,
            receipt_in: u128,
            gross_usdx: u128,
            mint_fee_usdx: u128,
            net_usdx: u128,
            lane_debt_after: u128,
        },
        /// USDX burned and receipt returned.
        /// USDX 已销毁并返还收据资产。
        UsdxRedeemed {
            account: T::AccountId,
            receipt_asset_id: u64,
            usdx_burned: u128,
            gross_receipt: u128,
            redeem_fee_receipt: u128,
            net_receipt: u128,
            lane_debt_after: u128,
        },
        CollateralRegistered {
            receipt_asset_id: u64,
            descriptor_hash: H256,
            activation_evidence_hash: H256,
        },
        CollateralEvidenceUpdated {
            receipt_asset_id: u64,
            activation_evidence_hash: H256,
        },
        CollateralEnabled {
            receipt_asset_id: u64,
            enabled: bool,
        },
        GlobalPauseChanged {
            paused: bool,
        },
        /// Root credited USDX to an account (test / bootstrap faucet).
        /// Root 向账户记入 USDX（测试 / 引导水龙头）。
        UsdxAdminCredited {
            beneficiary: T::AccountId,
            amount: u128,
            total_debt_after: u128,
        },
        CollateralPauseChanged {
            receipt_asset_id: u64,
            paused: bool,
        },
        LimitsUpdated {
            receipt_asset_id: u64,
        },
        PolicyUpdated {
            receipt_asset_id: u64,
        },
        GlobalDebtCeilingUpdated {
            old_limit: u128,
            new_limit: u128,
        },
        /// A rate-limit window was reset after expiry or a governance limit update.
        /// 限速窗口因到期或治理更新限额而重置。
        WindowReset {
            receipt_asset_id: u64,
            direction: WindowDirection,
            window_start: BlockNumberFor<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        Paused,
        LanePaused,
        UnknownCollateral,
        CollateralAlreadyRegistered,
        LaneDisabled,
        InvalidReceiptAsset,
        ReceiptDescriptorMismatch,
        InvalidActivationEvidence,
        AssetConfigMismatch,
        LaneMustBeDisabled,
        InvalidPolicy,
        InvalidLimits,
        AmountTooSmall,
        PerTxLimitExceeded,
        WindowLimitExceeded,
        LaneDebtCeilingExceeded,
        GlobalDebtCeilingExceeded,
        InsufficientLaneLiquidity,
        InsufficientUsdx,
        ArithmeticOverflow,
        ZeroOutput,
        SlippageExceeded,
        AccountingInvariantViolated,
        AssetOperationFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Deposits an authenticated receipt and mints net USDX.
        /// 存入已认证收据资产并铸造净额 USDX。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::mint())]
        #[transactional]
        pub fn mint(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            receipt_amount: u128,
            min_usdx: u128,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_operational(receipt_asset_id)?;
            Self::ensure_accounting_invariant()?;

            let policy = CollateralPolicies::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::UnknownCollateral)?;
            let limits = CollateralLimits::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::UnknownCollateral)?;
            Self::ensure_amount_limits(receipt_amount, &limits)?;

            let gross = Self::mul_bps_floor(receipt_amount, policy.mint_factor_bps)?;
            let fee = Self::mul_bps_floor(gross, policy.mint_fee_bps)?;
            let net = gross
                .checked_sub(fee)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(net > 0, Error::<T>::ZeroOutput);
            ensure!(net >= min_usdx, Error::<T>::SlippageExceeded);

            let lane_debt = CollateralUsdxDebt::<T>::get(receipt_asset_id);
            let next_lane_debt = lane_debt
                .checked_add(net)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                next_lane_debt <= limits.debt_ceiling,
                Error::<T>::LaneDebtCeilingExceeded
            );

            let total_debt = TotalUsdxDebt::<T>::get();
            let next_total_debt = total_debt
                .checked_add(net)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                next_total_debt <= GlobalUsdxDebtCeiling::<T>::get(),
                Error::<T>::GlobalDebtCeilingExceeded
            );

            Self::consume_mint_window(receipt_asset_id, net, &limits)?;

            let psm = Self::psm_account();
            T::Assets::transfer(
                receipt_asset_id,
                &who,
                &psm,
                receipt_amount,
                Preservation::Expendable,
            )
            .map_err(|_| Error::<T>::AssetOperationFailed)?;
            T::Assets::mint_into(T::UsdxAssetId::get(), &who, net)
                .map_err(|_| Error::<T>::AssetOperationFailed)?;

            CollateralUsdxDebt::<T>::insert(receipt_asset_id, next_lane_debt);
            TotalUsdxDebt::<T>::put(next_total_debt);
            Self::ensure_lane_solvency(receipt_asset_id, next_lane_debt)?;
            Self::ensure_accounting_invariant()?;

            Self::deposit_event(Event::UsdxMinted {
                beneficiary: who,
                receipt_asset_id,
                receipt_in: receipt_amount,
                gross_usdx: gross,
                mint_fee_usdx: fee,
                net_usdx: net,
                lane_debt_after: next_lane_debt,
            });
            Ok(())
        }

        /// Burns USDX and returns a selected lane's receipt asset.
        /// 销毁 USDX 并返还所选通道的收据资产。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::redeem())]
        #[transactional]
        pub fn redeem(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            usdx_amount: u128,
            min_receipt: u128,
        ) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_operational(receipt_asset_id)?;
            Self::ensure_accounting_invariant()?;

            let policy = CollateralPolicies::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::UnknownCollateral)?;
            let limits = CollateralLimits::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::UnknownCollateral)?;
            Self::ensure_amount_limits(usdx_amount, &limits)?;

            let gross_receipt = usdx_amount;
            let fee = Self::mul_bps_ceil(gross_receipt, policy.redeem_fee_bps)?;
            let net_receipt = gross_receipt
                .checked_sub(fee)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(net_receipt > 0, Error::<T>::ZeroOutput);
            ensure!(net_receipt >= min_receipt, Error::<T>::SlippageExceeded);

            let lane_debt = CollateralUsdxDebt::<T>::get(receipt_asset_id);
            ensure!(
                usdx_amount <= lane_debt,
                Error::<T>::InsufficientLaneLiquidity
            );
            let next_lane_debt = lane_debt
                .checked_sub(usdx_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let total_debt = TotalUsdxDebt::<T>::get();
            let next_total_debt = total_debt
                .checked_sub(usdx_amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;

            let psm = Self::psm_account();
            ensure!(
                T::Assets::balance(receipt_asset_id, &psm) >= gross_receipt,
                Error::<T>::InsufficientLaneLiquidity
            );
            Self::consume_redeem_window(receipt_asset_id, usdx_amount, &limits)?;

            T::Assets::burn_from(
                T::UsdxAssetId::get(),
                &who,
                usdx_amount,
                Preservation::Expendable,
                Precision::Exact,
                Fortitude::Polite,
            )
            .map_err(|_| Error::<T>::InsufficientUsdx)?;
            T::Assets::transfer(
                receipt_asset_id,
                &psm,
                &who,
                net_receipt,
                Preservation::Expendable,
            )
            .map_err(|_| Error::<T>::AssetOperationFailed)?;

            CollateralUsdxDebt::<T>::insert(receipt_asset_id, next_lane_debt);
            TotalUsdxDebt::<T>::put(next_total_debt);
            Self::ensure_lane_solvency(receipt_asset_id, next_lane_debt)?;
            Self::ensure_accounting_invariant()?;

            Self::deposit_event(Event::UsdxRedeemed {
                account: who,
                receipt_asset_id,
                usdx_burned: usdx_amount,
                gross_receipt,
                redeem_fee_receipt: fee,
                net_receipt,
                lane_debt_after: next_lane_debt,
            });
            Ok(())
        }

        /// Registers a disabled receipt lane from the canonical HFT registry.
        /// 从规范 HFT registry 注册默认停用的收据通道。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::register_collateral())]
        pub fn register_collateral(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            evidence: LaneActivationEvidence,
            policy: CollateralPolicy,
            limits: LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                !CollateralConfigs::<T>::contains_key(receipt_asset_id),
                Error::<T>::CollateralAlreadyRegistered
            );
            Self::validate_policy(&policy)?;
            Self::validate_limits(&limits)?;
            let descriptor_hash = T::ReceiptValidator::descriptor_hash(receipt_asset_id)
                .ok_or(Error::<T>::InvalidReceiptAsset)?;
            ensure!(
                T::ReceiptValidator::validate_evidence(
                    receipt_asset_id,
                    descriptor_hash,
                    &evidence
                ),
                Error::<T>::InvalidActivationEvidence
            );
            ensure!(
                T::ProtocolAssetInspector::validate_receipt(receipt_asset_id),
                Error::<T>::AssetConfigMismatch
            );
            let psm = Self::psm_account();
            ensure!(
                T::ProtocolAssetInspector::validate_usdx(T::UsdxAssetId::get(), &psm),
                Error::<T>::AssetConfigMismatch
            );

            CollateralConfigs::<T>::insert(
                receipt_asset_id,
                LaneConfig {
                    descriptor_hash,
                    activation_evidence_hash: Self::evidence_hash(&evidence),
                    enabled: false,
                },
            );
            LaneEvidence::<T>::insert(receipt_asset_id, &evidence);
            CollateralPolicies::<T>::insert(receipt_asset_id, policy);
            CollateralLimits::<T>::insert(receipt_asset_id, limits);
            MintWindow::<T>::remove(receipt_asset_id);
            RedeemWindow::<T>::remove(receipt_asset_id);
            Self::deposit_event(Event::CollateralRegistered {
                receipt_asset_id,
                descriptor_hash,
                activation_evidence_hash: Self::evidence_hash(&evidence),
            });
            Ok(())
        }

        /// Enables or disables a registered receipt lane.
        /// 启用或停用已注册收据通道。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_enabled())]
        pub fn set_enabled(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            enabled: bool,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            CollateralConfigs::<T>::try_mutate(receipt_asset_id, |maybe| -> DispatchResult {
                let config = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
                if enabled {
                    Self::ensure_descriptor_matches(receipt_asset_id, config)?;
                    Self::ensure_evidence_valid(receipt_asset_id, config)?;
                    let psm = Self::psm_account();
                    ensure!(
                        T::ProtocolAssetInspector::validate_usdx(T::UsdxAssetId::get(), &psm)
                            && T::ProtocolAssetInspector::validate_receipt(receipt_asset_id),
                        Error::<T>::AssetConfigMismatch
                    );
                    let limits = CollateralLimits::<T>::get(receipt_asset_id)
                        .ok_or(Error::<T>::UnknownCollateral)?;
                    Self::ensure_limits_operational(&limits)?;
                    ensure!(
                        GlobalUsdxDebtCeiling::<T>::get() > 0,
                        Error::<T>::InvalidLimits
                    );
                }
                config.enabled = enabled;
                Ok(())
            })?;
            Self::deposit_event(Event::CollateralEnabled {
                receipt_asset_id,
                enabled,
            });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_paused())]
        pub fn set_global_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            T::PauseOrigin::ensure_origin(origin)?;
            GlobalPaused::<T>::put(paused);
            Self::deposit_event(Event::GlobalPauseChanged { paused });
            Ok(())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::set_paused())]
        pub fn set_collateral_paused(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            paused: bool,
        ) -> DispatchResult {
            T::PauseOrigin::ensure_origin(origin)?;
            ensure!(
                CollateralConfigs::<T>::contains_key(receipt_asset_id),
                Error::<T>::UnknownCollateral
            );
            CollateralPaused::<T>::insert(receipt_asset_id, paused);
            Self::deposit_event(Event::CollateralPauseChanged {
                receipt_asset_id,
                paused,
            });
            Ok(())
        }

        #[pallet::call_index(6)]
        #[pallet::weight(T::WeightInfo::set_limits())]
        pub fn set_limits(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            limits: LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                CollateralConfigs::<T>::contains_key(receipt_asset_id),
                Error::<T>::UnknownCollateral
            );
            Self::validate_limits(&limits)?;
            CollateralLimits::<T>::insert(receipt_asset_id, limits);
            let window_start = frame_system::Pallet::<T>::block_number();
            MintWindow::<T>::insert(
                receipt_asset_id,
                WindowUsage {
                    window_start,
                    used_amount: 0,
                },
            );
            RedeemWindow::<T>::insert(
                receipt_asset_id,
                WindowUsage {
                    window_start,
                    used_amount: 0,
                },
            );
            Self::deposit_event(Event::LimitsUpdated { receipt_asset_id });
            Self::deposit_event(Event::WindowReset {
                receipt_asset_id,
                direction: WindowDirection::Mint,
                window_start,
            });
            Self::deposit_event(Event::WindowReset {
                receipt_asset_id,
                direction: WindowDirection::Redeem,
                window_start,
            });
            Ok(())
        }

        #[pallet::call_index(7)]
        #[pallet::weight(T::WeightInfo::set_policy())]
        pub fn set_policy(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            policy: CollateralPolicy,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(
                CollateralConfigs::<T>::contains_key(receipt_asset_id),
                Error::<T>::UnknownCollateral
            );
            Self::validate_policy(&policy)?;
            CollateralPolicies::<T>::insert(receipt_asset_id, policy);
            Self::deposit_event(Event::PolicyUpdated { receipt_asset_id });
            Ok(())
        }

        #[pallet::call_index(8)]
        #[pallet::weight(T::WeightInfo::set_global_debt_ceiling())]
        pub fn set_global_debt_ceiling(origin: OriginFor<T>, new_limit: u128) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let old_limit = GlobalUsdxDebtCeiling::<T>::get();
            GlobalUsdxDebtCeiling::<T>::put(new_limit);
            Self::deposit_event(Event::GlobalDebtCeilingUpdated {
                old_limit,
                new_limit,
            });
            Ok(())
        }

        /// Replaces activation evidence while the lane is disabled.
        /// 在通道停用时替换激活证据。
        #[pallet::call_index(9)]
        #[pallet::weight(T::WeightInfo::update_collateral())]
        pub fn update_collateral(
            origin: OriginFor<T>,
            receipt_asset_id: u64,
            evidence: LaneActivationEvidence,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            CollateralConfigs::<T>::try_mutate(receipt_asset_id, |maybe| -> DispatchResult {
                let config = maybe.as_mut().ok_or(Error::<T>::UnknownCollateral)?;
                ensure!(!config.enabled, Error::<T>::LaneMustBeDisabled);
                let descriptor_hash = T::ReceiptValidator::descriptor_hash(receipt_asset_id)
                    .ok_or(Error::<T>::InvalidReceiptAsset)?;
                ensure!(
                    descriptor_hash == config.descriptor_hash,
                    Error::<T>::ReceiptDescriptorMismatch
                );
                ensure!(
                    T::ReceiptValidator::validate_evidence(
                        receipt_asset_id,
                        descriptor_hash,
                        &evidence
                    ),
                    Error::<T>::InvalidActivationEvidence
                );
                config.activation_evidence_hash = Self::evidence_hash(&evidence);
                LaneEvidence::<T>::insert(receipt_asset_id, &evidence);
                Ok(())
            })?;
            Self::deposit_event(Event::CollateralEvidenceUpdated {
                receipt_asset_id,
                activation_evidence_hash: Self::evidence_hash(&evidence),
            });
            Ok(())
        }

        /// Root faucet: mint USDX while keeping `total_issuance == TotalUsdxDebt`.
        /// Intended for local/E2E bootstrap; does not create receipt-lane debt.
        /// Root 水龙头：铸造 USDX 并保持 `total_issuance == TotalUsdxDebt`。
        /// 用于本地/E2E 引导；不产生 receipt 通道债务。
        #[pallet::call_index(10)]
        #[pallet::weight(T::WeightInfo::admin_credit_usdx())]
        #[transactional]
        pub fn admin_credit_usdx(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            amount: u128,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(amount > 0, Error::<T>::ZeroOutput);
            ensure!(!GlobalPaused::<T>::get(), Error::<T>::Paused);
            Self::ensure_accounting_invariant()?;

            let next_total = TotalUsdxDebt::<T>::get()
                .checked_add(amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                next_total <= GlobalUsdxDebtCeiling::<T>::get(),
                Error::<T>::GlobalDebtCeilingExceeded
            );
            let next_admin = AdminUsdxDebt::<T>::get()
                .checked_add(amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;

            T::Assets::mint_into(T::UsdxAssetId::get(), &beneficiary, amount)
                .map_err(|_| Error::<T>::AssetOperationFailed)?;
            TotalUsdxDebt::<T>::put(next_total);
            AdminUsdxDebt::<T>::put(next_admin);
            Self::ensure_accounting_invariant()?;

            Self::deposit_event(Event::UsdxAdminCredited {
                beneficiary,
                amount,
                total_debt_after: next_total,
            });
            Ok(())
        }
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        #[cfg(feature = "try-runtime")]
        fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
            Self::check_accounting_invariants().map_err(Into::into)
        }
    }

    impl<T: Config> Pallet<T> {
        /// Returns the deterministic PSM collateral account.
        /// 返回确定性的 PSM 抵押账户。
        pub fn psm_account() -> T::AccountId {
            T::PsmPalletId::get().into_account_truncating()
        }

        fn ensure_operational(receipt_asset_id: u64) -> DispatchResult {
            ensure!(!GlobalPaused::<T>::get(), Error::<T>::Paused);
            ensure!(
                !CollateralPaused::<T>::get(receipt_asset_id),
                Error::<T>::LanePaused
            );
            let config = CollateralConfigs::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::UnknownCollateral)?;
            ensure!(config.enabled, Error::<T>::LaneDisabled);
            Self::ensure_descriptor_matches(receipt_asset_id, &config)?;
            Self::ensure_evidence_valid(receipt_asset_id, &config)?;
            let psm = Self::psm_account();
            ensure!(
                T::ProtocolAssetInspector::validate_usdx(T::UsdxAssetId::get(), &psm)
                    && T::ProtocolAssetInspector::validate_receipt(receipt_asset_id),
                Error::<T>::AssetConfigMismatch
            );
            Ok(())
        }

        fn ensure_descriptor_matches(receipt_asset_id: u64, config: &LaneConfig) -> DispatchResult {
            let current = T::ReceiptValidator::descriptor_hash(receipt_asset_id)
                .ok_or(Error::<T>::InvalidReceiptAsset)?;
            ensure!(
                current == config.descriptor_hash,
                Error::<T>::ReceiptDescriptorMismatch
            );
            Ok(())
        }

        fn ensure_evidence_valid(receipt_asset_id: u64, config: &LaneConfig) -> DispatchResult {
            let evidence = LaneEvidence::<T>::get(receipt_asset_id)
                .ok_or(Error::<T>::InvalidActivationEvidence)?;
            ensure!(
                Self::evidence_hash(&evidence) == config.activation_evidence_hash,
                Error::<T>::InvalidActivationEvidence
            );
            ensure!(
                T::ReceiptValidator::validate_evidence(
                    receipt_asset_id,
                    config.descriptor_hash,
                    &evidence
                ),
                Error::<T>::InvalidActivationEvidence
            );
            Ok(())
        }

        pub(crate) fn evidence_hash(evidence: &LaneActivationEvidence) -> H256 {
            H256::from(sp_core::hashing::blake2_256(&evidence.encode()))
        }

        fn ensure_amount_limits(
            amount: u128,
            limits: &LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            ensure!(amount >= limits.min_amount, Error::<T>::AmountTooSmall);
            ensure!(amount <= limits.max_per_tx, Error::<T>::PerTxLimitExceeded);
            Ok(())
        }

        fn validate_policy(policy: &CollateralPolicy) -> DispatchResult {
            ensure!(
                policy.mint_factor_bps > 0
                    && policy.mint_factor_bps <= BPS_DENOMINATOR
                    && policy.mint_fee_bps <= BPS_DENOMINATOR
                    && policy.redeem_fee_bps <= BPS_DENOMINATOR,
                Error::<T>::InvalidPolicy
            );
            Ok(())
        }

        fn validate_limits(limits: &LaneLimits<BlockNumberFor<T>>) -> DispatchResult {
            ensure!(!limits.window_blocks.is_zero(), Error::<T>::InvalidLimits);
            let disabled =
                limits.max_per_tx == 0 && limits.max_per_window == 0 && limits.debt_ceiling == 0;
            let operational = limits.min_amount > 0
                && limits.max_per_tx >= limits.min_amount
                && limits.max_per_window >= limits.max_per_tx
                && limits.debt_ceiling >= limits.min_amount;
            ensure!(disabled || operational, Error::<T>::InvalidLimits);
            Ok(())
        }

        fn ensure_limits_operational(limits: &LaneLimits<BlockNumberFor<T>>) -> DispatchResult {
            ensure!(
                limits.min_amount > 0
                    && limits.max_per_tx >= limits.min_amount
                    && limits.max_per_window >= limits.max_per_tx
                    && limits.debt_ceiling >= limits.min_amount
                    && !limits.window_blocks.is_zero(),
                Error::<T>::InvalidLimits
            );
            Ok(())
        }

        fn consume_mint_window(
            receipt_asset_id: u64,
            amount: u128,
            limits: &LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            MintWindow::<T>::try_mutate(receipt_asset_id, |usage| {
                Self::consume_window(
                    receipt_asset_id,
                    WindowDirection::Mint,
                    usage,
                    amount,
                    limits,
                )
            })
        }

        fn consume_redeem_window(
            receipt_asset_id: u64,
            amount: u128,
            limits: &LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            RedeemWindow::<T>::try_mutate(receipt_asset_id, |usage| {
                Self::consume_window(
                    receipt_asset_id,
                    WindowDirection::Redeem,
                    usage,
                    amount,
                    limits,
                )
            })
        }

        fn consume_window(
            receipt_asset_id: u64,
            direction: WindowDirection,
            usage: &mut WindowUsage<BlockNumberFor<T>>,
            amount: u128,
            limits: &LaneLimits<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let now = frame_system::Pallet::<T>::block_number();
            if usage.window_start.is_zero()
                || now.saturating_sub(usage.window_start) >= limits.window_blocks
            {
                usage.window_start = now;
                usage.used_amount = 0;
                Self::deposit_event(Event::WindowReset {
                    receipt_asset_id,
                    direction,
                    window_start: now,
                });
            }
            let next = usage
                .used_amount
                .checked_add(amount)
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            ensure!(
                next <= limits.max_per_window,
                Error::<T>::WindowLimitExceeded
            );
            usage.used_amount = next;
            Ok(())
        }

        fn ensure_lane_solvency(receipt_asset_id: u64, debt: u128) -> DispatchResult {
            let balance = T::Assets::balance(receipt_asset_id, &Self::psm_account());
            ensure!(balance >= debt, Error::<T>::AccountingInvariantViolated);
            Ok(())
        }

        fn ensure_accounting_invariant() -> DispatchResult {
            ensure!(
                T::Assets::total_issuance(T::UsdxAssetId::get()) == TotalUsdxDebt::<T>::get(),
                Error::<T>::AccountingInvariantViolated
            );
            Ok(())
        }

        /// Checks global debt attribution and every registered lane's solvency.
        /// 检查全局债务归因与每条已注册通道的偿付能力。
        ///
        /// This performs an unbounded storage iteration and is intended for
        /// try-runtime/off-chain diagnostics, not dispatchable execution.
        /// 本函数会无界遍历 storage，仅用于 try-runtime/链下诊断，不用于交易执行。
        pub fn check_accounting_invariants() -> Result<(), &'static str> {
            let mut attributed_debt = 0u128;
            let psm = Self::psm_account();
            for (receipt_asset_id, debt) in CollateralUsdxDebt::<T>::iter() {
                if !CollateralConfigs::<T>::contains_key(receipt_asset_id) {
                    return Err("debt exists for an unregistered receipt lane");
                }
                attributed_debt = attributed_debt
                    .checked_add(debt)
                    .ok_or("collateral debt sum overflow")?;
                if T::Assets::balance(receipt_asset_id, &psm) < debt {
                    return Err("receipt lane balance is below attributed debt");
                }
            }
            let admin_debt = AdminUsdxDebt::<T>::get();
            let total = TotalUsdxDebt::<T>::get();
            let expected = attributed_debt
                .checked_add(admin_debt)
                .ok_or("attributed + admin debt overflow")?;
            if expected != total {
                return Err("sum(collateral debt) + admin debt != total USDX debt");
            }
            if T::Assets::total_issuance(T::UsdxAssetId::get()) != total {
                return Err("USDX total issuance != total debt");
            }
            Ok(())
        }

        fn mul_bps_floor(value: u128, bps: u16) -> Result<u128, DispatchError> {
            let product = U256::from(value)
                .checked_mul(U256::from(bps))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let result = product / U256::from(BPS_DENOMINATOR);
            ensure!(
                result <= U256::from(u128::MAX),
                Error::<T>::ArithmeticOverflow
            );
            Ok(result.low_u128())
        }

        fn mul_bps_ceil(value: u128, bps: u16) -> Result<u128, DispatchError> {
            if bps == 0 {
                return Ok(0);
            }
            let denominator = U256::from(BPS_DENOMINATOR);
            let product = U256::from(value)
                .checked_mul(U256::from(bps))
                .ok_or(Error::<T>::ArithmeticOverflow)?;
            let rounded = product
                .checked_add(denominator - U256::one())
                .ok_or(Error::<T>::ArithmeticOverflow)?
                / denominator;
            ensure!(
                rounded <= U256::from(u128::MAX),
                Error::<T>::ArithmeticOverflow
            );
            Ok(rounded.low_u128())
        }
    }
}
