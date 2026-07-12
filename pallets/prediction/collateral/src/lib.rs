// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Safe 1:1 collateral mirror between `pallet-assets` and ORML prediction assets.
//! `pallet-assets` 与 ORML 预测资产之间的安全 1:1 抵押镜像。
//!
//! The pallet escrows real foreign assets in an independent sovereign account
//! and issues only the corresponding `Asset::ForeignAsset(u64)` mirror. Every
//! mutation starts and ends with issuance equal to escrow, and no per-user
//! balance is duplicated in this pallet's storage.
//! 本 pallet 将真实外部资产托管在独立主权账户中，仅发行对应的
//! `Asset::ForeignAsset(u64)` 镜像。每次变更前后都要求发行量等于托管余额，
//! 且不在本 pallet storage 中复制用户余额。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;
pub use weights::WeightInfo;

/// Validates live collateral readiness outside this pallet's storage boundary.
/// 在本 pallet storage 边界之外验证抵押资产的实时就绪状态。
///
/// Implementations must check asset existence, transfer/live status, and any
/// protocol-specific readiness such as USDX PSM invariants. The production
/// adapter is wired in Phase 6 and must not read another pallet's private
/// storage directly.
/// 实现必须检查资产存在性、可转账/Live 状态，以及 USDX PSM 不变量等协议特定
/// readiness。生产适配器在 Phase 6 接入，且不得直接读取其他 pallet 的私有 storage。
pub trait AssetValidator {
    /// Returns whether an asset is currently safe for new mirror deposits.
    /// 返回资产当前是否可安全用于新增镜像存入。
    fn is_valid(asset_id: u64) -> bool;
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{
        ensure,
        pallet_prelude::*,
        traits::{
            fungibles::{Inspect, Mutate},
            tokens::Preservation,
            EnsureOrigin, ExistenceRequirement, Get,
        },
        transactional, PalletId,
    };
    use frame_system::pallet_prelude::*;
    use orml_traits::MultiCurrency;
    use pallet_prediction_control::{PredictionControlApi, PredictionMode};
    use sp_runtime::traits::{AccountIdConversion, Zero};
    use zeitgeist_primitives::{traits::PredictionBaseAssetPolicy, types::Asset};

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    /// Runtime configuration for the foreign-collateral mirror boundary.
    /// 外部抵押镜像边界的 runtime 配置。
    #[pallet::config]
    pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
        /// Canonical `pallet-assets` ledger (`AssetId = u64`, `Balance = u128`).
        /// 标准 `pallet-assets` 账本（`AssetId = u64`、`Balance = u128`）。
        type Assets: Inspect<Self::AccountId, AssetId = u64, Balance = u128>
            + Mutate<Self::AccountId>;

        /// ORML prediction ledger with an explicit `Asset<u128>` market-id domain.
        /// 使用显式 `Asset<u128>` 市场 ID 域的 ORML 预测账本。
        type PredictionCurrencies: MultiCurrency<
            Self::AccountId,
            CurrencyId = Asset<u128>,
            Balance = u128,
        >;

        /// Read-only prediction subsystem mode boundary.
        /// 只读预测子系统模式边界。
        type Control: PredictionControlApi;

        /// Live asset and protocol-readiness validator.
        /// 资产实时状态与协议 readiness 验证器。
        type AssetValidator: AssetValidator;

        /// Governance origin allowed to update collateral whitelist admission.
        /// 可更新抵押资产白名单准入的治理来源。
        type WhitelistOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Governance origin allowed to pause new collateral deposits.
        /// 可暂停新增抵押存入的治理来源。
        type PauseOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Independent sovereign account identifier holding real collateral.
        /// 持有真实抵押品的独立主权账户标识。
        #[pallet::constant]
        type CollateralPalletId: Get<PalletId>;

        /// Weight provider; Phase 2 estimates must be regenerated in Phase 7.
        /// 权重提供者；Phase 2 估算值必须在 Phase 7 重新生成。
        type WeightInfo: WeightInfo;
    }

    /// Explicit governance whitelist; absent keys are denied.
    /// 显式治理白名单；不存在的 key 默认拒绝。
    #[pallet::storage]
    #[pallet::getter(fn whitelisted_assets)]
    pub type WhitelistedAssets<T: Config> = StorageMap<_, Blake2_128Concat, u64, bool, OptionQuery>;

    /// Per-asset switch that pauses only new deposits.
    /// 仅暂停新增存入的逐资产开关。
    #[pallet::storage]
    #[pallet::getter(fn asset_deposit_paused)]
    pub type AssetDepositPaused<T: Config> = StorageMap<_, Blake2_128Concat, u64, bool, ValueQuery>;

    /// Global switch that pauses only new deposits.
    /// 仅暂停新增存入的全局开关。
    #[pallet::storage]
    #[pallet::getter(fn global_deposit_paused)]
    pub type GlobalDepositPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

    /// Collateral mirror events.
    /// 抵押镜像事件。
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Real collateral was escrowed and its ORML mirror was issued.
        /// 真实抵押品已托管，并发行对应 ORML 镜像。
        Deposited {
            who: T::AccountId,
            asset_id: u64,
            amount: u128,
        },
        /// An ORML mirror was burned and real collateral was released.
        /// ORML 镜像已销毁，并释放对应真实抵押品。
        Withdrawn {
            who: T::AccountId,
            asset_id: u64,
            amount: u128,
        },
        /// Governance changed one asset's whitelist state.
        /// 治理变更了某资产的白名单状态。
        AssetWhitelistSet { asset_id: u64, enabled: bool },
        /// Governance changed one asset's deposit pause state.
        /// 治理变更了某资产的存入暂停状态。
        AssetDepositPauseSet { asset_id: u64, paused: bool },
        /// Governance changed the global deposit pause state.
        /// 治理变更了全局存入暂停状态。
        GlobalDepositPauseSet { paused: bool },
    }

    /// Collateral mirror failures.
    /// 抵押镜像失败原因。
    #[pallet::error]
    pub enum Error<T> {
        ZeroAmount,
        PredictionModeNotFull,
        AssetNotWhitelisted,
        GlobalDepositIsPaused,
        AssetDepositIsPaused,
        AssetInvalid,
        MirrorInconsistent,
        InsufficientAssetBalance,
        InsufficientMirrorBalance,
        MirrorNotWithdrawable,
        AssetTransferFailed,
        MirrorMintFailed,
        MirrorBurnFailed,
        EscrowReleaseFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Escrows a real asset and mints its 1:1 ORML foreign-asset mirror.
        /// 托管真实资产并按 1:1 铸造其 ORML 外部资产镜像。
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::deposit())]
        #[transactional]
        pub fn deposit(origin: OriginFor<T>, asset_id: u64, amount: u128) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(
                T::Control::prediction_mode() == PredictionMode::Full,
                Error::<T>::PredictionModeNotFull
            );
            ensure!(
                WhitelistedAssets::<T>::get(asset_id) == Some(true),
                Error::<T>::AssetNotWhitelisted
            );
            ensure!(
                !GlobalDepositPaused::<T>::get(),
                Error::<T>::GlobalDepositIsPaused
            );
            ensure!(
                !AssetDepositPaused::<T>::get(asset_id),
                Error::<T>::AssetDepositIsPaused
            );
            ensure!(
                T::AssetValidator::is_valid(asset_id),
                Error::<T>::AssetInvalid
            );
            ensure!(
                Self::is_mirror_consistent(asset_id),
                Error::<T>::MirrorInconsistent
            );
            ensure!(
                T::Assets::balance(asset_id, &who) >= amount,
                Error::<T>::InsufficientAssetBalance
            );

            T::Assets::transfer(
                asset_id,
                &who,
                &Self::sovereign_account(),
                amount,
                Preservation::Preserve,
            )
            .map_err(|_| Error::<T>::AssetTransferFailed)?;
            T::PredictionCurrencies::deposit(Self::mirror_asset(asset_id), &who, amount)
                .map_err(|_| Error::<T>::MirrorMintFailed)?;

            ensure!(
                Self::is_mirror_consistent(asset_id),
                Error::<T>::MirrorInconsistent
            );
            Self::deposit_event(Event::Deposited {
                who,
                asset_id,
                amount,
            });
            Ok(())
        }

        /// Burns a user's ORML mirror before releasing the real escrowed asset.
        /// 先销毁用户 ORML 镜像，再释放真实托管资产。
        ///
        /// This unwind path intentionally ignores mode, whitelist, pause, and
        /// validator state. ORML liquidity restrictions are still enforced.
        /// 本退出路径有意忽略模式、白名单、暂停及验证器状态，但仍执行 ORML 流动性限制。
        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::withdraw())]
        #[transactional]
        pub fn withdraw(origin: OriginFor<T>, asset_id: u64, amount: u128) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(
                Self::is_mirror_consistent(asset_id),
                Error::<T>::MirrorInconsistent
            );
            let mirror = Self::mirror_asset(asset_id);
            ensure!(
                T::PredictionCurrencies::free_balance(mirror, &who) >= amount,
                Error::<T>::InsufficientMirrorBalance
            );
            T::PredictionCurrencies::ensure_can_withdraw(mirror, &who, amount)
                .map_err(|_| Error::<T>::MirrorNotWithdrawable)?;

            T::PredictionCurrencies::withdraw(
                mirror,
                &who,
                amount,
                ExistenceRequirement::AllowDeath,
            )
            .map_err(|_| Error::<T>::MirrorBurnFailed)?;
            T::Assets::transfer(
                asset_id,
                &Self::sovereign_account(),
                &who,
                amount,
                Preservation::Expendable,
            )
            .map_err(|_| Error::<T>::EscrowReleaseFailed)?;

            ensure!(
                Self::is_mirror_consistent(asset_id),
                Error::<T>::MirrorInconsistent
            );
            Self::deposit_event(Event::Withdrawn {
                who,
                asset_id,
                amount,
            });
            Ok(())
        }

        /// Adds or removes one asset from governance admission.
        /// 添加或移除一个资产的治理准入。
        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::set_asset_whitelisted())]
        pub fn set_asset_whitelisted(
            origin: OriginFor<T>,
            asset_id: u64,
            enabled: bool,
        ) -> DispatchResult {
            T::WhitelistOrigin::ensure_origin(origin)?;
            if enabled {
                ensure!(
                    T::AssetValidator::is_valid(asset_id),
                    Error::<T>::AssetInvalid
                );
            }
            WhitelistedAssets::<T>::insert(asset_id, enabled);
            Self::deposit_event(Event::AssetWhitelistSet { asset_id, enabled });
            Ok(())
        }

        /// Pauses or resumes new deposits for one asset.
        /// 暂停或恢复某资产的新增存入。
        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_asset_deposit_paused())]
        pub fn set_asset_deposit_paused(
            origin: OriginFor<T>,
            asset_id: u64,
            paused: bool,
        ) -> DispatchResult {
            T::PauseOrigin::ensure_origin(origin)?;
            AssetDepositPaused::<T>::insert(asset_id, paused);
            Self::deposit_event(Event::AssetDepositPauseSet { asset_id, paused });
            Ok(())
        }

        /// Pauses or resumes all new foreign-collateral deposits.
        /// 暂停或恢复全部新增外部抵押存入。
        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::set_global_deposit_paused())]
        pub fn set_global_deposit_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
            T::PauseOrigin::ensure_origin(origin)?;
            GlobalDepositPaused::<T>::put(paused);
            Self::deposit_event(Event::GlobalDepositPauseSet { paused });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Returns the independent collateral escrow account.
        /// 返回独立的抵押托管账户。
        pub fn sovereign_account() -> T::AccountId {
            T::CollateralPalletId::get().into_account_truncating()
        }

        /// Constructs the ORML mirror without converting or truncating its id.
        /// 直接构造 ORML 镜像，不对 ID 做转换或截断。
        pub const fn mirror_asset(asset_id: u64) -> Asset<u128> {
            Asset::ForeignAsset(asset_id)
        }

        /// Returns the real asset balance held by the collateral escrow.
        /// 返回抵押托管账户持有的真实资产余额。
        pub fn escrow_balance(asset_id: u64) -> u128 {
            T::Assets::balance(asset_id, &Self::sovereign_account())
        }

        /// Returns total issuance of the ORML foreign-asset mirror.
        /// 返回 ORML 外部资产镜像的总发行量。
        pub fn mirror_issuance(asset_id: u64) -> u128 {
            T::PredictionCurrencies::total_issuance(Self::mirror_asset(asset_id))
        }

        /// Returns whether mirror issuance exactly equals real escrow.
        /// 返回镜像发行量是否与真实托管余额完全相等。
        pub fn is_mirror_consistent(asset_id: u64) -> bool {
            Self::mirror_issuance(asset_id) == Self::escrow_balance(asset_id)
        }

        /// Evaluates all live gates for a new foreign-collateral deposit.
        /// 评估新增外部抵押存入的全部实时门禁。
        pub fn is_deposit_allowed(asset_id: u64) -> bool {
            T::Control::prediction_mode() == PredictionMode::Full
                && WhitelistedAssets::<T>::get(asset_id) == Some(true)
                && !GlobalDepositPaused::<T>::get()
                && !AssetDepositPaused::<T>::get(asset_id)
                && T::AssetValidator::is_valid(asset_id)
                && Self::is_mirror_consistent(asset_id)
        }
    }

    impl<T: Config> PredictionBaseAssetPolicy<u64> for Pallet<T> {
        fn is_allowed(asset_id: u64) -> bool {
            Self::is_deposit_allowed(asset_id)
        }
    }
}
