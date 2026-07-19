// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0

//! Prediction community core: fee ledger, MultiLevel, SingleLine, pool + withdraw/reinvest.
//! Prediction 社群核心：交易费账本、助力、公排、沉淀领取与提现复投。
//!
//! - P0.5–P2: community bond, deposit allowance, protocol trade fee (paths A/B).
//! - P3: `settle_multi_level` · P4: `settle_single_line`.
//! - P5: `claim_pool_reward` (P5–P7 tier pots) + `withdraw_commission` (cash/reinvest).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;
pub use weights::WeightInfo;

mod multi_level;
mod pool;
mod single_line;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::{
	Currency, ExistenceRequirement, Get, LockIdentifier, NamedReservableCurrency,
};
use orml_traits::MultiCurrency;
use pallet_prediction_community_common::{
	is_activated, lookup_tier_id, split_bps, split_commission_budget, split_top_bar,
	withdraw_split_by_tier, CommissionTicket, CommunityStatus, PROTOCOL_COMMISSION_BPS,
	PROTOCOL_TRADE_FEE_BPS,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, Saturating, Zero},
	DispatchError, Perbill, RuntimeDebug,
};
use zeitgeist_primitives::types::Asset;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
pub type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::NegativeImbalance;
pub type AssetOf<T> = Asset<<T as Config>::MarketId>;
pub type CurrencyBalanceOf<T> =
	<<T as Config>::MultiCurrency as MultiCurrency<<T as frame_system::Config>::AccountId>>::Balance;

const COMMUNITY_BOND_ID: LockIdentifier = *b"pr/cbond";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Native currency for community NEX bond.
		/// 社区 NEX 押金使用的原生货币。
		type Currency: NamedReservableCurrency<
			Self::AccountId,
			ReserveIdentifier = LockIdentifier,
		>;

		/// ORML multi-currency for USDX community ledger (D20).
		/// USDX 社群账本用的 ORML 多币种（D20）。
		type MultiCurrency: MultiCurrency<Self::AccountId, CurrencyId = AssetOf<Self>>;

		/// Market id type used by prediction `Asset`.
		/// 预测 `Asset` 使用的市场 id 类型。
		type MarketId: Parameter + Member + Copy + MaxEncodedLen + Default;

		/// Community USDX asset id (`Asset::ForeignAsset(...)`).
		/// 社群 USDX 资产（`Asset::ForeignAsset(...)`）。
		#[pallet::constant]
		type CommunityAsset: Get<AssetOf<Self>>;

		/// NEX bond required to register a community operator (1_000_000 NEX).
		/// 登记社区运营账户所需的 NEX 押金（1_000_000 NEX）。
		#[pallet::constant]
		type CommunityBond: Get<BalanceOf<Self>>;

		/// Delay before unbonding completes.
		/// 解押完成前的延迟。
		#[pallet::constant]
		type CommunityBondUnbondDelay: Get<BlockNumberFor<Self>>;

		/// Treasury account receiving top-bar remainder and unbound operator 3%.
		/// 接收顶栏余款与未绑定运营 3% 的国库账户。
		#[pallet::constant]
		type TreasuryAccount: Get<Self::AccountId>;

		/// Community vault pallet id (holds USDX for pending / unsettled tickets / pool seed).
		/// 社群金库 PalletId（持有 pending / 未结算票 / 沉淀种子 USDX）。
		#[pallet::constant]
		type PalletId: Get<frame_support::PalletId>;

		/// Max pending deposit/trade tickets.
		/// 待结算充值/成交票上限。
		#[pallet::constant]
		type MaxTickets: Get<u32>;

		/// Max tickets settled per settle call (ML / SL).
		/// 单次结算调用可处理票数上限（助力 / 公排）。
		#[pallet::constant]
		type MaxSettleBatch: Get<u32>;

		/// Max accounts per SingleLine segment.
		/// 公排链每段账户上限。
		#[pallet::constant]
		type MaxSingleLineLength: Get<u32>;

		/// Max SingleLine segments (global chain capacity).
		/// 公排链最大段数（全局容量）。
		#[pallet::constant]
		type MaxSegmentCount: Get<u32>;

		/// Pool reward round duration in blocks (default ~1 day @6s).
		/// 沉淀领取轮次时长（块；默认约 1 天 @6s）。
		#[pallet::constant]
		type PoolRoundDuration: Get<BlockNumberFor<Self>>;

		type WeightInfo: WeightInfo;
	}

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(4);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Registered community operators and bond state.
	/// 已登记社区运营账户与押金状态。
	#[pallet::storage]
	pub type RegisteredCommunities<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		CommunityRecord<BlockNumberFor<T>, BalanceOf<T>>,
		OptionQuery,
	>;

	/// Community members (referrer graph for MultiLevel).
	/// 社群会员（动态助力推荐关系图）。
	#[pallet::storage]
	pub type Members<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		MemberRecord<T::AccountId>,
		OptionQuery,
	>;

	/// User → home community operator binding.
	/// 用户 → 归属社区运营账户绑定。
	#[pallet::storage]
	pub type HomeCommunityOperator<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::AccountId, OptionQuery>;

	/// Remaining fee allowance (offsets community-side 0.02 only).
	/// 剩余交易费额度（仅抵社群侧 0.02）。
	#[pallet::storage]
	pub type FeeAllowance<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Lifetime USDX deposited (stats only; not used for tiering).
	/// 累计 USDX 充值（仅统计；不定档）。
	#[pallet::storage]
	pub type LifetimeFeeDeposited<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Lifetime community-side trading fee (tiering metric).
	/// 累计社群侧交易费（定档指标）。
	#[pallet::storage]
	pub type LifetimeTradingFee<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Direct referral count (increments when downline first reaches P1).
	/// 直推计数（下线首次达 P1 时 +1）。
	#[pallet::storage]
	pub type DirectCount<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	/// Operator pending USDX commission.
	/// 运营账户待领 USDX 佣金。
	#[pallet::storage]
	pub type OperatorPending<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Member pending USDX commission (SL/ML credits).
	/// 会员待领 USDX 佣金（公排/助力入账）。
	#[pallet::storage]
	pub type MemberPending<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Next commission ticket id.
	/// 下一张分佣票 id。
	#[pallet::storage]
	pub type NextTicketId<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Unsettled commission tickets (P3 settles ML; P4 settles SL).
	/// 未结算分佣票（P3 结算助力；P4 结算公排）。
	#[pallet::storage]
	pub type CommissionTickets<T: Config> = StorageMap<
		_,
		Twox64Concat,
		u64,
		CommissionTicket<T::AccountId, CurrencyBalanceOf<T>>,
		OptionQuery,
	>;

	/// Pool seed / unsettled remainder holding (USDX accounting counter).
	/// 沉淀种子 / 未分配余量计数（USDX）。
	#[pallet::storage]
	pub type UnallocatedPool<T: Config> = StorageValue<_, CurrencyBalanceOf<T>, ValueQuery>;

	/// D19: last protocol fee charge marker within an extrinsic.
	/// Keyed by `(block, extrinsic_index, payer, notional)` so indices cannot collide across blocks.
	/// D19：同一 extrinsic 内协议费已收取标记。
	/// 使用 `(块高, extrinsic_index, payer, notional)`，避免跨块 index 碰撞。
	#[pallet::storage]
	pub type ChargedInExtrinsic<T: Config> = StorageValue<
		_,
		(
			BlockNumberFor<T>,
			u32,
			T::AccountId,
			CurrencyBalanceOf<T>,
		),
		OptionQuery,
	>;

	/// Global SingleLine segments (D6=A, no entity namespace).
	/// 全局公排链分段（D6=A，无 entity 命名空间）。
	#[pallet::storage]
	pub type SingleLineSegments<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		u32,
		BoundedVec<T::AccountId, T::MaxSingleLineLength>,
		ValueQuery,
	>;

	/// Number of SingleLine segments.
	/// 公排链段数。
	#[pallet::storage]
	pub type SingleLineSegmentCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Account → global SingleLine index.
	/// 账户 → 全局公排下标。
	#[pallet::storage]
	pub type SingleLineIndex<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u32, OptionQuery>;

	/// Logically removed members (skipped on SL walk).
	/// 逻辑移除会员（公排遍历跳过）。
	#[pallet::storage]
	pub type RemovedMembers<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, bool, ValueQuery>;

	/// Count of pool-eligible members per tier id (5/6/7).
	/// 可领沉淀的各档会员人数（5/6/7）。
	#[pallet::storage]
	pub type TierMemberCount<T: Config> = StorageMap<_, Twox64Concat, u8, u32, ValueQuery>;

	/// Current pool reward round snapshot.
	/// 当前沉淀领取轮次快照。
	#[pallet::storage]
	pub type CurrentPoolRound<T: Config> = StorageValue<
		_,
		PoolRoundInfo<CurrencyBalanceOf<T>, BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Last pool round claimed by account.
	/// 账户上次领取的沉淀轮次。
	#[pallet::storage]
	pub type LastClaimedPoolRound<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery>;

	/// Root emergency pause for pool claims.
	/// Root 紧急暂停沉淀领取。
	#[pallet::storage]
	pub type PoolRewardPaused<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// Lifetime USDX withdrawn as cash (stats).
	/// 累计现金提现 USDX（统计）。
	#[pallet::storage]
	pub type MemberWithdrawn<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	/// Lifetime USDX earned to pending (stats).
	/// 累计入账 pending 的 USDX（统计）。
	#[pallet::storage]
	pub type MemberTotalEarned<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, CurrencyBalanceOf<T>, ValueQuery>;

	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct CommunityRecord<BlockNumber, Balance> {
		pub bond: Balance,
		pub status: CommunityStatus,
		pub registered_at: BlockNumber,
		pub unlock_at: Option<BlockNumber>,
	}

	/// Snapshot of one pool reward round (P5).
	/// 一期沉淀领取快照（P5）。
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
		Default,
	)]
	pub struct PoolRoundInfo<Balance, BlockNumber> {
		pub round_id: u64,
		pub start_block: BlockNumber,
		pub pool_snapshot: Balance,
		pub p5_count: u32,
		pub p5_per_member: Balance,
		pub p5_claimed: u32,
		pub p6_count: u32,
		pub p6_per_member: Balance,
		pub p6_claimed: u32,
		pub p7_count: u32,
		pub p7_per_member: Balance,
		pub p7_claimed: u32,
	}

	/// Community member record (referrer is immutable after register).
	/// 社群会员记录（推荐人注册后不可变）。
	#[derive(
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct MemberRecord<AccountId> {
		pub referrer: Option<AccountId>,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CommunityRegistered { operator: T::AccountId, bond: BalanceOf<T> },
		MemberRegistered {
			who: T::AccountId,
			referrer: Option<T::AccountId>,
		},
		HomeCommunityBound { who: T::AccountId, operator: T::AccountId },
		FeeAllowanceDeposited {
			who: T::AccountId,
			amount: CurrencyBalanceOf<T>,
			allowance_after: CurrencyBalanceOf<T>,
			operator_share: CurrencyBalanceOf<T>,
			ticket_id: u64,
		},
		CommunityOperatorShareToTreasury {
			who: T::AccountId,
			amount: CurrencyBalanceOf<T>,
		},
		OperatorPendingWithdrawn { operator: T::AccountId, amount: CurrencyBalanceOf<T> },
		/// Member commission withdrawn with tier cash/reinvest split.
		/// 会员佣金按档位现金/复投拆分提现。
		CommissionWithdrawn {
			who: T::AccountId,
			amount: CurrencyBalanceOf<T>,
			cash: CurrencyBalanceOf<T>,
			reinvest: CurrencyBalanceOf<T>,
			tier_id: u8,
		},
		TradeFeeWithAllowance {
			payer: T::AccountId,
			creator_cut: CurrencyBalanceOf<T>,
			treasury_cut: CurrencyBalanceOf<T>,
			commission_fee: CurrencyBalanceOf<T>,
			allowance_after: CurrencyBalanceOf<T>,
			lifetime_trading_fee: CurrencyBalanceOf<T>,
		},
		TradeFeeCashCommission {
			payer: T::AccountId,
			creator_cut: CurrencyBalanceOf<T>,
			treasury_cut: CurrencyBalanceOf<T>,
			commission_fee: CurrencyBalanceOf<T>,
			ticket_id: Option<u64>,
			lifetime_trading_fee: CurrencyBalanceOf<T>,
		},
		CommunityUnbonding {
			operator: T::AccountId,
			unlock_at: BlockNumberFor<T>,
		},
		/// MultiLevel portion of a ticket settled.
		/// 票的动态助力部分已结算。
		MultiLevelSettled {
			ticket_id: u64,
			payer: T::AccountId,
			paid: CurrencyBalanceOf<T>,
			remaining_to_pool: CurrencyBalanceOf<T>,
		},
		/// SingleLine portion of a ticket settled.
		/// 票的公排部分已结算。
		SingleLineSettled {
			ticket_id: u64,
			payer: T::AccountId,
			paid: CurrencyBalanceOf<T>,
			remaining_to_pool: CurrencyBalanceOf<T>,
		},
		MemberActivated {
			who: T::AccountId,
			referrer: Option<T::AccountId>,
		},
		AddedToSingleLine {
			who: T::AccountId,
			index: u32,
		},
		NewPoolRoundStarted {
			round_id: u64,
			pool_snapshot: CurrencyBalanceOf<T>,
			p5_count: u32,
			p6_count: u32,
			p7_count: u32,
		},
		PoolRewardClaimed {
			who: T::AccountId,
			round_id: u64,
			tier_id: u8,
			amount: CurrencyBalanceOf<T>,
		},
		PoolRewardPauseSet { paused: bool },
	}

	#[pallet::error]
	pub enum Error<T> {
		AlreadyRegistered,
		CommunityNotFound,
		CommunityNotActive,
		InsufficientBond,
		BondTransferFailed,
		UsdxTransferFailed,
		TicketQueueFull,
		NothingToWithdraw,
		NotOperator,
		StillBonded,
		ZeroAmount,
		/// Caller is already a community member.
		/// 调用者已是社群会员。
		AlreadyMember,
		/// Referrer is invalid (self or not a member).
		/// 推荐人无效（自己或非会员）。
		InvalidReferrer,
		/// Ticket missing or MultiLevel already settled.
		/// 票不存在或助力已结算。
		TicketNotReady,
		/// Empty settle batch.
		/// 结算批次为空。
		EmptyBatch,
		/// Global SingleLine segment cap reached.
		/// 全局公排段数已达上限。
		MaxSegmentCountReached,
		NotMember,
		MemberRemoved,
		TierNotEligible,
		AlreadyClaimedPool,
		NothingToClaim,
		PoolTierExhausted,
		InsufficientPool,
		PoolRewardPaused,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register as a community operator and lock `CommunityBond` NEX.
		/// 登记为社区运营账户并锁定 `CommunityBond` NEX。
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::register_community())]
		pub fn register_community(origin: OriginFor<T>) -> DispatchResult {
			let operator = ensure_signed(origin)?;
			ensure!(
				!RegisteredCommunities::<T>::contains_key(&operator),
				Error::<T>::AlreadyRegistered
			);
			let bond = T::CommunityBond::get();
			T::Currency::reserve_named(&COMMUNITY_BOND_ID, &operator, bond)
				.map_err(|_| Error::<T>::InsufficientBond)?;
			let now = <frame_system::Pallet<T>>::block_number();
			RegisteredCommunities::<T>::insert(
				&operator,
				CommunityRecord {
					bond,
					status: CommunityStatus::Active,
					registered_at: now,
					unlock_at: None,
				},
			);
			Self::deposit_event(Event::CommunityRegistered { operator, bond });
			Ok(())
		}

		/// Bind caller's home community to an active registered operator.
		/// 将调用者归属社区绑定到已登记且有效的运营账户。
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::bind_home_community())]
		pub fn bind_home_community(
			origin: OriginFor<T>,
			operator: T::AccountId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::ensure_active_community(&operator)?;
			HomeCommunityOperator::<T>::insert(&who, &operator);
			Self::deposit_event(Event::HomeCommunityBound { who, operator });
			Ok(())
		}

		/// Deposit USDX: immediate 50/47/3, increase fee allowance, enqueue DepositTicket.
		/// 充值 USDX：立即 50/47/3 分佣，增加额度，入队 DepositTicket。
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::deposit_fee_allowance())]
		pub fn deposit_fee_allowance(
			origin: OriginFor<T>,
			amount: CurrencyBalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_credit_deposit(&who, amount, true)
		}

		/// Withdraw operator pending USDX commission from the community vault.
		/// 从社群金库提取运营账户待领 USDX 佣金。
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::withdraw_operator_pending())]
		pub fn withdraw_operator_pending(
			origin: OriginFor<T>,
			amount: CurrencyBalanceOf<T>,
		) -> DispatchResult {
			let operator = ensure_signed(origin)?;
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			let pending = OperatorPending::<T>::get(&operator);
			ensure!(pending >= amount, Error::<T>::NothingToWithdraw);
			let vault = Self::vault_account();
			let asset = T::CommunityAsset::get();
			T::MultiCurrency::transfer(asset, &vault, &operator, amount, ExistenceRequirement::AllowDeath)
				.map_err(|_| Error::<T>::UsdxTransferFailed)?;
			OperatorPending::<T>::insert(&operator, pending.saturating_sub(amount));
			Self::deposit_event(Event::OperatorPendingWithdrawn { operator, amount });
			Ok(())
		}

		/// Begin unbonding; stops receiving new deposit 3% immediately.
		/// 开始解押；立即停止接收新的充值 3%。
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::unbond_community())]
		pub fn unbond_community(origin: OriginFor<T>) -> DispatchResult {
			let operator = ensure_signed(origin)?;
			RegisteredCommunities::<T>::try_mutate(&operator, |maybe| -> DispatchResult {
				let rec = maybe.as_mut().ok_or(Error::<T>::CommunityNotFound)?;
				ensure!(
					matches!(rec.status, CommunityStatus::Active),
					Error::<T>::CommunityNotActive
				);
				let unlock_at = <frame_system::Pallet<T>>::block_number()
					.saturating_add(T::CommunityBondUnbondDelay::get());
				rec.status = CommunityStatus::Unbonding;
				rec.unlock_at = Some(unlock_at);
				Self::deposit_event(Event::CommunityUnbonding {
					operator: operator.clone(),
					unlock_at,
				});
				Ok(())
			})
		}

		/// Register as a community member and optionally bind an immutable referrer.
		/// 登记为社群会员，可选绑定不可变推荐人。
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::register())]
		pub fn register(
			origin: OriginFor<T>,
			referrer: Option<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(!Members::<T>::contains_key(&who), Error::<T>::AlreadyMember);
			if let Some(ref r) = referrer {
				ensure!(r != &who, Error::<T>::InvalidReferrer);
				ensure!(Members::<T>::contains_key(r), Error::<T>::InvalidReferrer);
			}
			Members::<T>::insert(&who, MemberRecord { referrer: referrer.clone() });
			Self::deposit_event(Event::MemberRegistered { who, referrer });
			Ok(())
		}

		/// Settle MultiLevel budget on the given commission tickets (P3).
		/// 结算给定分佣票的动态助力预算（P3）。
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::settle_multi_level(ticket_ids.len() as u32))]
		pub fn settle_multi_level(
			origin: OriginFor<T>,
			ticket_ids: BoundedVec<u64, T::MaxSettleBatch>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!ticket_ids.is_empty(), Error::<T>::EmptyBatch);
			for id in ticket_ids.into_iter() {
				Self::do_settle_multi_level(id)?;
			}
			Ok(())
		}

		/// Settle SingleLine budget on the given commission tickets (P4).
		/// 结算给定分佣票的公排预算（P4）。
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::settle_single_line(ticket_ids.len() as u32))]
		pub fn settle_single_line(
			origin: OriginFor<T>,
			ticket_ids: BoundedVec<u64, T::MaxSettleBatch>,
		) -> DispatchResult {
			ensure_signed(origin)?;
			ensure!(!ticket_ids.is_empty(), Error::<T>::EmptyBatch);
			for id in ticket_ids.into_iter() {
				Self::do_settle_single_line(id)?;
			}
			Ok(())
		}

		/// Withdraw member pending with tier cash/reinvest split (§10.2).
		/// Reinvest goes through the deposit commission path (funds already in vault).
		/// 按档位拆现金/复投提取会员 pending（§10.2）；复投走充值分佣路径（资金已在金库）。
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::withdraw_commission())]
		pub fn withdraw_commission(
			origin: OriginFor<T>,
			amount: CurrencyBalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			let pending = MemberPending::<T>::get(&who);
			ensure!(pending >= amount, Error::<T>::NothingToWithdraw);

			let tier_id = lookup_tier_id(LifetimeTradingFee::<T>::get(&who));
			let (cash, reinvest) = withdraw_split_by_tier(tier_id, amount);
			MemberPending::<T>::insert(&who, pending.saturating_sub(amount));

			if !cash.is_zero() {
				let vault = Self::vault_account();
				let asset = T::CommunityAsset::get();
				T::MultiCurrency::transfer(
					asset,
					&vault,
					&who,
					cash,
					ExistenceRequirement::AllowDeath,
				)
				.map_err(|_| Error::<T>::UsdxTransferFailed)?;
				MemberWithdrawn::<T>::mutate(&who, |v| *v = v.saturating_add(cash));
			}
			if !reinvest.is_zero() {
				// USDX already in vault; only run allowance + 50/47/3 accounting.
				Self::do_credit_deposit(&who, reinvest, false)?;
			}
			Self::deposit_event(Event::CommissionWithdrawn {
				who,
				amount,
				cash,
				reinvest,
				tier_id,
			});
			Ok(())
		}

		/// Claim current pool reward round (P5–P7 only).
		/// 领取当期沉淀（仅 P5–P7）。
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::claim_pool_reward())]
		pub fn claim_pool_reward(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_claim_pool_reward(&who)
		}

		/// Root: pause or resume pool reward claims.
		/// Root：暂停或恢复沉淀领取。
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::set_pool_reward_paused())]
		pub fn set_pool_reward_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
			ensure_root(origin)?;
			PoolRewardPaused::<T>::put(paused);
			Self::deposit_event(Event::PoolRewardPauseSet { paused });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn vault_account() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		pub fn ensure_active_community(operator: &T::AccountId) -> DispatchResult {
			let rec = RegisteredCommunities::<T>::get(operator)
				.ok_or(Error::<T>::CommunityNotFound)?;
			ensure!(
				matches!(rec.status, CommunityStatus::Active),
				Error::<T>::CommunityNotActive
			);
			Ok(())
		}

		fn credit_operator_or_treasury(
			who: &T::AccountId,
			operator_share: CurrencyBalanceOf<T>,
		) -> DispatchResult {
			if operator_share.is_zero() {
				return Ok(());
			}
			let asset = T::CommunityAsset::get();
			let vault = Self::vault_account();
			if let Some(operator) = HomeCommunityOperator::<T>::get(who) {
				if Self::ensure_active_community(&operator).is_ok() {
					OperatorPending::<T>::mutate(&operator, |p| {
						*p = p.saturating_add(operator_share)
					});
					return Ok(());
				}
			}
			T::MultiCurrency::transfer(
				asset,
				&vault,
				&T::TreasuryAccount::get(),
				operator_share,
				ExistenceRequirement::AllowDeath,
			)
			.map_err(|_| Error::<T>::UsdxTransferFailed)?;
			Self::deposit_event(Event::CommunityOperatorShareToTreasury {
				who: who.clone(),
				amount: operator_share,
			});
			Ok(())
		}

		/// Credit deposit / reinvest: optional user→vault transfer, then 50/47/3 + allowance.
		/// 充值/复投入账：可选用户→金库转账，然后 50/47/3 分佣并加额度。
		fn do_credit_deposit(
			who: &T::AccountId,
			amount: CurrencyBalanceOf<T>,
			transfer_from_user: bool,
		) -> DispatchResult {
			ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
			let vault = Self::vault_account();
			let asset = T::CommunityAsset::get();
			if transfer_from_user {
				T::MultiCurrency::transfer(
					asset,
					who,
					&vault,
					amount,
					ExistenceRequirement::AllowDeath,
				)
				.map_err(|_| Error::<T>::UsdxTransferFailed)?;
			}

			LifetimeFeeDeposited::<T>::mutate(who, |v| *v = v.saturating_add(amount));
			FeeAllowance::<T>::mutate(who, |v| *v = v.saturating_add(amount));

			let (sl, ml, operator_share) = split_commission_budget(amount);
			Self::credit_operator_or_treasury(who, operator_share)?;

			let ticket_id = Self::enqueue_ticket(who.clone(), sl, ml)?;
			let allowance_after = FeeAllowance::<T>::get(who);
			Self::deposit_event(Event::FeeAllowanceDeposited {
				who: who.clone(),
				amount,
				allowance_after,
				operator_share,
				ticket_id,
			});
			Ok(())
		}

		fn enqueue_ticket(
			who: T::AccountId,
			sl: CurrencyBalanceOf<T>,
			ml: CurrencyBalanceOf<T>,
		) -> Result<u64, DispatchError> {
			let id = NextTicketId::<T>::get();
			ensure!(
				(id as u32) < T::MaxTickets::get().saturating_mul(1024),
				Error::<T>::TicketQueueFull
			);
			CommissionTickets::<T>::insert(
				id,
				CommissionTicket {
					who,
					single_line: sl,
					multi_level: ml,
					ml_settled: false,
					sl_settled: false,
				},
			);
			NextTicketId::<T>::put(id.saturating_add(1));
			// Park SL+ML as unallocated until settlers credit pending / leave pool dust.
			UnallocatedPool::<T>::mutate(|p| *p = p.saturating_add(sl.saturating_add(ml)));
			Ok(id)
		}

		/// Credit lifetime trading fee and bump referrer `direct_count` on first P1 crossing.
		/// 累加社群侧交易费；首次跨过 P1 时给推荐人 `direct_count +1`。
		fn credit_lifetime_trading_fee(
			who: &T::AccountId,
			amount: CurrencyBalanceOf<T>,
		) -> CurrencyBalanceOf<T> {
			let before = LifetimeTradingFee::<T>::get(who);
			let after = before.saturating_add(amount);
			LifetimeTradingFee::<T>::insert(who, after);
			Self::note_tier_change(who, before, after);
			if !is_activated(before) && is_activated(after) {
				let referrer = Members::<T>::get(who).and_then(|m| m.referrer);
				if let Some(ref r) = referrer {
					DirectCount::<T>::mutate(r, |c| *c = c.saturating_add(1));
				}
				// D3': join global SingleLine on first P1 crossing.
				if Self::add_to_single_line(who).is_ok() {
					if let Some(index) = SingleLineIndex::<T>::get(who) {
						Self::deposit_event(Event::AddedToSingleLine {
							who: who.clone(),
							index,
						});
					}
				}
				Self::deposit_event(Event::MemberActivated {
					who: who.clone(),
					referrer,
				});
			}
			after
		}

		/// Settle one ticket's MultiLevel slice: credit pending, leave unpaid in pool.
		/// 结算单票助力：入账 pending，未分配部分留在沉淀计数。
		pub(crate) fn do_settle_multi_level(ticket_id: u64) -> DispatchResult {
			CommissionTickets::<T>::try_mutate(ticket_id, |maybe| -> DispatchResult {
				let ticket = maybe.as_mut().ok_or(Error::<T>::TicketNotReady)?;
				ensure!(!ticket.ml_settled, Error::<T>::TicketNotReady);
				let ml_budget = ticket.multi_level;
				let payer = ticket.who.clone();

				let (credits, _remaining) = Self::process_multi_level(&payer, ml_budget);
				let mut paid = CurrencyBalanceOf::<T>::zero();
				for c in credits {
					MemberPending::<T>::mutate(&c.beneficiary, |p| {
						*p = p.saturating_add(c.amount)
					});
					MemberTotalEarned::<T>::mutate(&c.beneficiary, |p| {
						*p = p.saturating_add(c.amount)
					});
					paid = paid.saturating_add(c.amount);
				}
				// Paid moves from unallocated accounting into MemberPending; remainder stays pooled.
				UnallocatedPool::<T>::mutate(|p| *p = p.saturating_sub(paid));
				ticket.ml_settled = true;
				let remaining_to_pool = ml_budget.saturating_sub(paid);
				Self::deposit_event(Event::MultiLevelSettled {
					ticket_id,
					payer,
					paid,
					remaining_to_pool,
				});
				Ok(())
			})
		}

		/// Settle one ticket's SingleLine slice: credit pending, leave unpaid in pool.
		/// 结算单票公排：入账 pending，未分配部分留在沉淀计数。
		pub(crate) fn do_settle_single_line(ticket_id: u64) -> DispatchResult {
			CommissionTickets::<T>::try_mutate(ticket_id, |maybe| -> DispatchResult {
				let ticket = maybe.as_mut().ok_or(Error::<T>::TicketNotReady)?;
				ensure!(!ticket.sl_settled, Error::<T>::TicketNotReady);
				let sl_budget = ticket.single_line;
				let payer = ticket.who.clone();

				let (credits, remaining_to_pool) = Self::process_single_line(&payer, sl_budget);
				let mut paid = CurrencyBalanceOf::<T>::zero();
				for c in credits {
					MemberPending::<T>::mutate(&c.beneficiary, |p| {
						*p = p.saturating_add(c.amount)
					});
					MemberTotalEarned::<T>::mutate(&c.beneficiary, |p| {
						*p = p.saturating_add(c.amount)
					});
					paid = paid.saturating_add(c.amount);
				}
				UnallocatedPool::<T>::mutate(|p| *p = p.saturating_sub(paid));
				ticket.sl_settled = true;
				debug_assert_eq!(paid.saturating_add(remaining_to_pool), sl_budget);
				Self::deposit_event(Event::SingleLineSettled {
					ticket_id,
					payer,
					paid,
					remaining_to_pool,
				});
				Ok(())
			})
		}

		/// Protocol fee percentage for USDX markets (0.03).
		/// USDX 市场协议费率（0.03）。
		pub fn protocol_fee_perbill() -> Perbill {
			// Perbill parts are out of 1_000_000_000; 3% = 30_000_000.
			Perbill::from_parts(30_000_000)
		}

		fn already_charged_this_extrinsic(
			payer: &T::AccountId,
			notional: CurrencyBalanceOf<T>,
		) -> bool {
			let Some(idx) = frame_system::Pallet::<T>::extrinsic_index() else {
				return false;
			};
			let now = <frame_system::Pallet<T>>::block_number();
			matches!(
				ChargedInExtrinsic::<T>::get(),
				Some((b, i, ref a, n)) if b == now && i == idx && a == payer && n == notional
			)
		}

		fn mark_charged_this_extrinsic(payer: &T::AccountId, notional: CurrencyBalanceOf<T>) {
			if let Some(idx) = frame_system::Pallet::<T>::extrinsic_index() {
				let now = <frame_system::Pallet<T>>::block_number();
				ChargedInExtrinsic::<T>::put((now, idx, payer.clone(), notional));
			}
		}

		/// Apply USDX protocol trade fee (paths A/B). Returns cash taken from `payer`.
		/// 收取 USDX 协议交易费（路径 A/B）。返回从 `payer` 实扣的现金。
		///
		/// Infallible: returns fees already successfully transferred.
		pub fn apply_usdx_trade_fee(
			payer: &T::AccountId,
			notional: CurrencyBalanceOf<T>,
			creator: &T::AccountId,
			creator_fee: Perbill,
		) -> CurrencyBalanceOf<T> {
			if notional.is_zero() {
				return Zero::zero();
			}
			// D19: once per extrinsic for same payer+notional.
			if Self::already_charged_this_extrinsic(payer, notional) {
				return Zero::zero();
			}

			let asset = T::CommunityAsset::get();
			let (creator_cut, treasury_cut) = split_top_bar(notional, creator_fee);
			let commission_fee = split_bps(notional, PROTOCOL_COMMISSION_BPS);
			let allowance = FeeAllowance::<T>::get(payer);

			if allowance >= commission_fee {
				// Path A
				let mut taken = CurrencyBalanceOf::<T>::zero();
				if !creator_cut.is_zero()
					&& T::MultiCurrency::transfer(
						asset,
						payer,
						creator,
						creator_cut,
						ExistenceRequirement::AllowDeath,
					)
					.is_ok()
				{
					taken = taken.saturating_add(creator_cut);
				}
				if !treasury_cut.is_zero()
					&& T::MultiCurrency::transfer(
						asset,
						payer,
						&T::TreasuryAccount::get(),
						treasury_cut,
						ExistenceRequirement::AllowDeath,
					)
					.is_ok()
				{
					taken = taken.saturating_add(treasury_cut);
				}
				if taken.is_zero() {
					return Zero::zero();
				}
				FeeAllowance::<T>::insert(payer, allowance.saturating_sub(commission_fee));
				let life = Self::credit_lifetime_trading_fee(payer, commission_fee);
				Self::mark_charged_this_extrinsic(payer, notional);
				Self::deposit_event(Event::TradeFeeWithAllowance {
					payer: payer.clone(),
					creator_cut,
					treasury_cut,
					commission_fee,
					allowance_after: FeeAllowance::<T>::get(payer),
					lifetime_trading_fee: life,
				});
				return taken;
			}

			// Path B
			let mut taken = CurrencyBalanceOf::<T>::zero();
			if !creator_cut.is_zero()
				&& T::MultiCurrency::transfer(
					asset,
					payer,
					creator,
					creator_cut,
					ExistenceRequirement::AllowDeath,
				)
				.is_ok()
			{
				taken = taken.saturating_add(creator_cut);
			}
			if !treasury_cut.is_zero()
				&& T::MultiCurrency::transfer(
					asset,
					payer,
					&T::TreasuryAccount::get(),
					treasury_cut,
					ExistenceRequirement::AllowDeath,
				)
				.is_ok()
			{
				taken = taken.saturating_add(treasury_cut);
			}
			let mut ticket_id = None;
			let vault = Self::vault_account();
			if !commission_fee.is_zero()
				&& T::MultiCurrency::transfer(
					asset,
					payer,
					&vault,
					commission_fee,
					ExistenceRequirement::AllowDeath,
				)
				.is_ok()
			{
				taken = taken.saturating_add(commission_fee);
				let (sl, ml, op) = split_commission_budget(commission_fee);
				let _ = Self::credit_operator_or_treasury(payer, op);
				if let Ok(id) = Self::enqueue_ticket(payer.clone(), sl, ml) {
					ticket_id = Some(id);
				}
				let life = Self::credit_lifetime_trading_fee(payer, commission_fee);
				Self::mark_charged_this_extrinsic(payer, notional);
				Self::deposit_event(Event::TradeFeeCashCommission {
					payer: payer.clone(),
					creator_cut,
					treasury_cut,
					commission_fee,
					ticket_id,
					lifetime_trading_fee: life,
				});
				return taken;
			}

			if !taken.is_zero() {
				Self::mark_charged_this_extrinsic(payer, notional);
			}
			taken
		}

		/// NEX / non-USDX market: take full 0.03 to treasury in `asset` (no allowance).
		/// NEX / 非 USDX 市场：按 0.03 全额进国库（该资产），不耗额度。
		pub fn apply_native_protocol_fee(
			payer: &T::AccountId,
			asset: AssetOf<T>,
			notional: CurrencyBalanceOf<T>,
		) -> CurrencyBalanceOf<T> {
			if notional.is_zero() {
				return Zero::zero();
			}
			if Self::already_charged_this_extrinsic(payer, notional) {
				return Zero::zero();
			}
			let fee = split_bps(notional, PROTOCOL_TRADE_FEE_BPS);
			if fee.is_zero() {
				return Zero::zero();
			}
			if T::MultiCurrency::transfer(
				asset,
				payer,
				&T::TreasuryAccount::get(),
				fee,
				ExistenceRequirement::AllowDeath,
			)
			.is_ok()
			{
				Self::mark_charged_this_extrinsic(payer, notional);
				fee
			} else {
				Zero::zero()
			}
		}
	}
}
