#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]

#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

pub mod apis;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;
pub mod configs;
mod migrations;

extern crate alloc;
use alloc::vec::Vec;
use sp_runtime::{
    generic, impl_opaque_keys,
    traits::{BlakeTwo256, IdentifyAccount, Verify},
    MultiAddress, MultiSignature,
};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

pub use frame_system::Call as SystemCall;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_timestamp::Call as TimestampCall;
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

pub mod genesis_config_presets;

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
    use super::*;
    use sp_runtime::{
        generic,
        traits::{BlakeTwo256, Hash as HashT},
    };

    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;

    /// Opaque block header type.
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    /// Opaque block type.
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    /// Opaque block identifier type.
    pub type BlockId = generic::BlockId<Block>;
    /// Opaque block hash type.
    pub type Hash = <BlakeTwo256 as HashT>::Output;
}

impl_opaque_keys! {
    pub struct SessionKeys {
        pub babe: Babe,
        pub grandpa: Grandpa,
        pub im_online: ImOnline,
    }
}

// To learn more about runtime versioning, see:
// https://docs.substrate.io/main-docs/build/upgrade#runtime-versioning
#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("nexus"),
    impl_name: alloc::borrow::Cow::Borrowed("nexus-node"),
    authoring_version: 1,
    // The version of the runtime specification. A full node will not attempt to use its native
    //   runtime in substitute for the on-chain Wasm runtime unless all of `spec_name`,
    //   `spec_version`, and `authoring_version` are the same between Wasm and native.
    // This value is set to 100 to notify Polkadot-JS App (https://polkadot.js.org/apps) to use
    //   the compatible custom types.
    spec_version: 107,
    impl_version: 2,
    apis: apis::RUNTIME_API_VERSIONS,
    transaction_version: 2,
    system_version: 1,
};

mod block_times {
    /// This determines the average expected block time that we are targeting. Blocks will be
    /// produced at a minimum duration defined by `SLOT_DURATION`. `SLOT_DURATION` is picked up by
    /// `pallet_timestamp` which is in turn picked up by `pallet_babe` to implement `fn
    /// slot_duration()`.
    ///
    /// Change this to adjust the block time.
    pub const MILLI_SECS_PER_BLOCK: u64 = 6000;

    // NOTE: Currently it is not possible to change the slot duration after the chain has started.
    // Attempting to do so will brick block production.
    pub const SLOT_DURATION: u64 = MILLI_SECS_PER_BLOCK;
}
pub use block_times::*;

// Time is measured by number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLI_SECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

pub const BLOCK_HASH_COUNT: BlockNumber = 2400;

// NEX token units (1 NEX = 10^12 indivisible units)
pub const NEX: Balance = 1_000_000_000_000;
pub const MILLI_NEX: Balance = 1_000_000_000;
pub const MICRO_NEX: Balance = 1_000_000;

// Backward-compatible aliases
pub const UNIT: Balance = NEX;
pub const MILLI_UNIT: Balance = MILLI_NEX;
pub const MICRO_UNIT: Balance = MICRO_NEX;

/// Existential deposit.
pub const EXISTENTIAL_DEPOSIT: Balance = MILLI_NEX;

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
    NativeVersion {
        runtime_version: VERSION,
        can_author_with: Default::default(),
    }
}

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Balance of an account.
pub type Balance = u128;

/// Index of a transaction in the chain.
pub type Nonce = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;

/// An index to a block.
pub type BlockNumber = u32;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The `TransactionExtension` to the basic transaction logic.
pub type TxExtension = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
    frame_metadata_hash_extension::CheckMetadataHash<Runtime>,
    frame_system::WeightReclaim<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, TxExtension>;

/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<RuntimeCall, TxExtension>;

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
pub type Migrations = (
    pallet_dispute_escrow::migrations::V2RemoveLockNonces<Runtime>,
    pallet_entity_order::migration::MigrateV1ToV2<Runtime>,
    migrations::InitializeUsdxProtocolAssets,
    migrations::retire_ads::RetireAdsFunds,
    migrations::retire_grouprobot::RetireGroupRobotFunds,
    migrations::retire_ads::RemoveAdsCore,
    migrations::retire_ads::RemoveAdsGroupRobot,
    migrations::retire_ads::RemoveAdsEntity,
    migrations::retire_grouprobot::RemoveGroupRobotRegistry,
    migrations::retire_grouprobot::RemoveGroupRobotConsensus,
    migrations::retire_grouprobot::RemoveGroupRobotCommunity,
    migrations::retire_grouprobot::RemoveGroupRobotCeremony,
    migrations::retire_grouprobot::RemoveGroupRobotSubscription,
    migrations::retire_grouprobot::RemoveGroupRobotRewards,
    migrations::retire_prediction::RemovePredictionControl,
    migrations::retire_prediction::RemovePredictionCollateral,
    migrations::retire_prediction::RemovePredictionCurrencies,
    migrations::retire_prediction::RemovePredictionTokens,
    migrations::retire_prediction::RemovePredictionMarketCommons,
    migrations::retire_prediction::RemovePredictionAuthorized,
    migrations::retire_prediction::RemovePredictionCourt,
    migrations::retire_prediction::RemovePredictionGlobalDisputes,
    migrations::retire_prediction::RemovePredictionMarkets,
    migrations::retire_prediction::RemovePredictionLegacySwaps,
    migrations::retire_prediction::RemovePredictionNeoSwaps,
    migrations::retire_prediction::RemovePredictionOrderbook,
    migrations::retire_prediction::RemovePredictionParimutuel,
    migrations::retire_prediction::RemovePredictionHybridRouter,
    migrations::retire_prediction::RemovePredictionCombinatorialTokens,
    migrations::retire_prediction::RemovePredictionFutarchy,
    migrations::retire_prediction::RemovePredictionStyx,
    migrations::retire_prediction::RemovePredictionCommunityCore,
);

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
    Migrations,
>;

// Create the runtime by composing the FRAME pallets that were previously configured.
#[frame_support::runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;

    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;

    #[runtime::pallet_index(2)]
    pub type Babe = pallet_babe;

    #[runtime::pallet_index(3)]
    pub type Grandpa = pallet_grandpa;

    #[runtime::pallet_index(4)]
    pub type Balances = pallet_balances;

    #[runtime::pallet_index(5)]
    pub type TransactionPayment = pallet_transaction_payment;

    #[runtime::pallet_index(6)]
    pub type Sudo = pallet_sudo;

    #[runtime::pallet_index(7)]
    pub type Preimage = pallet_preimage;

    #[runtime::pallet_index(8)]
    pub type Scheduler = pallet_scheduler;

    #[runtime::pallet_index(9)]
    pub type Authorship = pallet_authorship;

    #[runtime::pallet_index(10)]
    pub type Session = pallet_session;

    #[runtime::pallet_index(11)]
    pub type Historical = pallet_session::historical;

    #[runtime::pallet_index(12)]
    pub type Offences = pallet_offences;

    #[runtime::pallet_index(13)]
    pub type ImOnline = pallet_im_online;

    #[runtime::pallet_index(14)]
    pub type Staking = pallet_staking;

    #[runtime::pallet_index(15)]
    pub type Inscription = pallet_inscription;

    // ============================================================================
    // Governance: Committees (Collective + Membership)
    // ============================================================================

    // 1. 技术委员会 (Technical Committee)
    #[runtime::pallet_index(70)]
    pub type TechnicalCommittee = pallet_collective<Instance1>;

    #[runtime::pallet_index(71)]
    pub type TechnicalMembership = pallet_collective_membership<Instance1>;

    // 2. 仲裁委员会 (Arbitration Committee)
    #[runtime::pallet_index(72)]
    pub type ArbitrationCommittee = pallet_collective<Instance2>;

    #[runtime::pallet_index(73)]
    pub type ArbitrationMembership = pallet_collective_membership<Instance2>;

    // 3. 财务委员会 (Treasury Council)
    #[runtime::pallet_index(74)]
    pub type TreasuryCouncil = pallet_collective<Instance3>;

    #[runtime::pallet_index(75)]
    pub type TreasuryMembership = pallet_collective_membership<Instance3>;

    // 4. 内容委员会 (Content Committee)
    #[runtime::pallet_index(76)]
    pub type ContentCommittee = pallet_collective<Instance4>;

    #[runtime::pallet_index(77)]
    pub type ContentMembership = pallet_collective_membership<Instance4>;

    // ============================================================================
    // Trading Pallets
    // ============================================================================

    #[runtime::pallet_index(56)]
    pub type NexMarket = pallet_nex_market;

    // ============================================================================
    // Escrow, Referral, IPFS Pallets
    // ============================================================================

    #[runtime::pallet_index(60)]
    pub type Escrow = pallet_dispute_escrow;

    #[runtime::pallet_index(62)]
    pub type StorageService = pallet_storage_service;

    #[runtime::pallet_index(63)]
    pub type Evidence = pallet_dispute_evidence;

    #[runtime::pallet_index(64)]
    pub type Arbitration = pallet_dispute_arbitration;

    #[runtime::pallet_index(65)]
    pub type StorageLifecycle = pallet_storage_lifecycle;

    // ============================================================================
    // Chat (聊天系统：场景授权 + 加密私聊)
    // ============================================================================

    #[runtime::pallet_index(67)]
    pub type ChatPermission = pallet_chat_permission;

    #[runtime::pallet_index(68)]
    pub type ChatCore = pallet_chat_core;

    #[runtime::pallet_index(69)]
    pub type ChatGroup = pallet_chat_group;

    // 链下投递信箱注册表（索引接在治理委员会 70-77 之后的首个空位）。
    // Off-chain delivery inbox registry (first free index after the 70-77 committees).
    #[runtime::pallet_index(78)]
    pub type ChatInbox = pallet_chat_inbox;

    // 账户派生加密同步锚（EISA，CHAT_SYNC_ANCHOR_ADR §5）。
    // Account-derived Encrypted Sync Anchor (EISA, CHAT_SYNC_ANCHOR_ADR §5).
    #[runtime::pallet_index(79)]
    pub type ChatSync = pallet_chat_sync;

    // 消息身份预密钥锚（X3DH IK/SPK/OPK 根 + 1:1 栈能力，CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN）。
    // Messaging identity prekey anchor (X3DH IK/SPK/OPK root + 1:1 stack caps).
    #[runtime::pallet_index(80)]
    pub type MsgIdentity = pallet_msg_identity;

    // ============================================================================
    // Smart Contracts
    // ============================================================================

    #[runtime::pallet_index(90)]
    pub type Contracts = pallet_contracts;

    // ============================================================================
    // Assets Pallet (for ShareMall Token)
    // ============================================================================

    #[runtime::pallet_index(110)]
    pub type Assets = pallet_assets;

    // ============================================================================
    // Entity Pallets (原 ShareMall，已重构)
    // ============================================================================

    #[runtime::pallet_index(120)]
    pub type EntityRegistry = pallet_entity_registry;

    #[runtime::pallet_index(129)]
    pub type EntityShop = pallet_entity_shop;

    #[runtime::pallet_index(121)]
    pub type EntityProduct = pallet_entity_product;

    #[runtime::pallet_index(122)]
    pub type EntityTransaction = pallet_entity_order;

    #[runtime::pallet_index(123)]
    pub type EntityReview = pallet_entity_review;

    #[runtime::pallet_index(124)]
    pub type EntityToken = pallet_entity_token;

    #[runtime::pallet_index(125)]
    pub type EntityGovernance = pallet_entity_governance;

    #[runtime::pallet_index(126)]
    pub type EntityMember = pallet_entity_member;

    #[runtime::pallet_index(127)]
    pub type CommissionCore = pallet_commission_core;

    #[runtime::pallet_index(133)]
    pub type CommissionReferral = pallet_commission_referral;

    #[runtime::pallet_index(138)]
    pub type CommissionMultiLevel = pallet_commission_multi_level;

    #[runtime::pallet_index(134)]
    pub type CommissionLevelDiff = pallet_commission_level_diff;

    #[runtime::pallet_index(135)]
    pub type CommissionSingleLine = pallet_commission_single_line;

    #[runtime::pallet_index(136)]
    pub type CommissionTeam = pallet_commission_team;

    #[runtime::pallet_index(137)]
    pub type CommissionPoolReward = pallet_commission_pool_reward;

    #[runtime::pallet_index(128)]
    pub type EntityMarket = pallet_entity_market;

    #[runtime::pallet_index(130)]
    pub type EntityDisclosure = pallet_entity_disclosure;

    #[runtime::pallet_index(131)]
    pub type EntityKyc = pallet_entity_kyc;

    #[runtime::pallet_index(132)]
    pub type EntityTokenSale = pallet_entity_tokensale;

    #[runtime::pallet_index(139)]
    pub type EntityLoyalty = pallet_entity_loyalty;

    // Indexes 150–155 (GroupRobot) and 160–162 (Ads) are retired and must not
    // be reused. Storage prefixes were cleared by spec 107.
    // 索引 150–155（GroupRobot）与 160–162（Ads）已退役，禁止复用。
    // 存储前缀已由 spec 107 清除。

    // ============================================================================
    // Hyperbridge / ISMP protocol layer (Stage 1: cross-chain messaging base)
    // Hyperbridge / ISMP 协议层（Stage 1：跨链消息基座）
    // ============================================================================

    #[runtime::pallet_index(170)]
    pub type Ismp = pallet_ismp;

    // Stage 1b-1: vendored `pallet-hyperbridge` (D3=(c)) — host-param / fee module.
    // Stage 1b-1：vendor 的 `pallet-hyperbridge`（D3=(c)）——host-param / 费用模块。
    #[runtime::pallet_index(171)]
    pub type Hyperbridge = pallet_hyperbridge;

    // Stage 1b-2: `ismp-grandpa` GRANDPA consensus client (published 2512.1.0).
    // Stage 1b-2：`ismp-grandpa` GRANDPA 共识客户端（已发布 2512.1.0）。
    #[runtime::pallet_index(172)]
    pub type IsmpGrandpa = ismp_grandpa;

    // Stage 2 / HB-ASSET-01: self-built native-NEX asset bridge (burn/mint, D3=(c)).
    // Stage 2 / HB-ASSET-01：自建原生 NEX 资产桥（burn/mint，D3=(c)）。
    #[runtime::pallet_index(173)]
    pub type BridgeIsmp = pallet_bridge_ismp;

    // Phase 0 USDX PSM. It is intentionally inert: runtime adapters reject every
    // receipt until a pinned, authenticated HFT receipt registry is integrated.
    // Phase 0 USDX PSM。当前刻意保持惰性：在接入锁定版本、可认证的 HFT 收据注册表前，
    // runtime adapter 拒绝所有收据。
    #[runtime::pallet_index(174)]
    pub type Usdx = pallet_usdx;

    // Official Hyperbridge HFT 2512.0.0. No tokens are registered by genesis;
    // production registration remains blocked on upstream callback review and lane evidence.
    // 官方 Hyperbridge HFT 2512.0.0。genesis 不注册任何 token；生产注册仍受上游 callback
    // 审查与通道证据阻塞。
    #[runtime::pallet_index(175)]
    pub type HyperFungibleToken = pallet_hyper_fungible_token;
}

#[cfg(test)]
mod pallet_index_compatibility_tests {
    use super::*;
    use frame_support::traits::PalletInfoAccess;

    #[test]
    fn existing_pallet_indices_zero_through_175_are_stable() {
        let expected = [
            (System::name(), System::index(), 0),
            (Timestamp::name(), Timestamp::index(), 1),
            (Babe::name(), Babe::index(), 2),
            (Grandpa::name(), Grandpa::index(), 3),
            (Balances::name(), Balances::index(), 4),
            (TransactionPayment::name(), TransactionPayment::index(), 5),
            (Sudo::name(), Sudo::index(), 6),
            (Preimage::name(), Preimage::index(), 7),
            (Scheduler::name(), Scheduler::index(), 8),
            (Authorship::name(), Authorship::index(), 9),
            (Session::name(), Session::index(), 10),
            (Historical::name(), Historical::index(), 11),
            (Offences::name(), Offences::index(), 12),
            (ImOnline::name(), ImOnline::index(), 13),
            (Staking::name(), Staking::index(), 14),
            (Inscription::name(), Inscription::index(), 15),
            (NexMarket::name(), NexMarket::index(), 56),
            (Escrow::name(), Escrow::index(), 60),
            (StorageService::name(), StorageService::index(), 62),
            (Evidence::name(), Evidence::index(), 63),
            (Arbitration::name(), Arbitration::index(), 64),
            (StorageLifecycle::name(), StorageLifecycle::index(), 65),
            (ChatPermission::name(), ChatPermission::index(), 67),
            (ChatCore::name(), ChatCore::index(), 68),
            (ChatGroup::name(), ChatGroup::index(), 69),
            (TechnicalCommittee::name(), TechnicalCommittee::index(), 70),
            (
                TechnicalMembership::name(),
                TechnicalMembership::index(),
                71,
            ),
            (
                ArbitrationCommittee::name(),
                ArbitrationCommittee::index(),
                72,
            ),
            (
                ArbitrationMembership::name(),
                ArbitrationMembership::index(),
                73,
            ),
            (TreasuryCouncil::name(), TreasuryCouncil::index(), 74),
            (TreasuryMembership::name(), TreasuryMembership::index(), 75),
            (ContentCommittee::name(), ContentCommittee::index(), 76),
            (ContentMembership::name(), ContentMembership::index(), 77),
            (ChatInbox::name(), ChatInbox::index(), 78),
            (ChatSync::name(), ChatSync::index(), 79),
            (MsgIdentity::name(), MsgIdentity::index(), 80),
            (Contracts::name(), Contracts::index(), 90),
            (Assets::name(), Assets::index(), 110),
            (EntityRegistry::name(), EntityRegistry::index(), 120),
            (EntityProduct::name(), EntityProduct::index(), 121),
            (EntityTransaction::name(), EntityTransaction::index(), 122),
            (EntityReview::name(), EntityReview::index(), 123),
            (EntityToken::name(), EntityToken::index(), 124),
            (EntityGovernance::name(), EntityGovernance::index(), 125),
            (EntityMember::name(), EntityMember::index(), 126),
            (CommissionCore::name(), CommissionCore::index(), 127),
            (EntityMarket::name(), EntityMarket::index(), 128),
            (EntityShop::name(), EntityShop::index(), 129),
            (EntityDisclosure::name(), EntityDisclosure::index(), 130),
            (EntityKyc::name(), EntityKyc::index(), 131),
            (EntityTokenSale::name(), EntityTokenSale::index(), 132),
            (CommissionReferral::name(), CommissionReferral::index(), 133),
            (
                CommissionLevelDiff::name(),
                CommissionLevelDiff::index(),
                134,
            ),
            (
                CommissionSingleLine::name(),
                CommissionSingleLine::index(),
                135,
            ),
            (CommissionTeam::name(), CommissionTeam::index(), 136),
            (
                CommissionPoolReward::name(),
                CommissionPoolReward::index(),
                137,
            ),
            (
                CommissionMultiLevel::name(),
                CommissionMultiLevel::index(),
                138,
            ),
            (EntityLoyalty::name(), EntityLoyalty::index(), 139),
            (Ismp::name(), Ismp::index(), 170),
            (Hyperbridge::name(), Hyperbridge::index(), 171),
            (IsmpGrandpa::name(), IsmpGrandpa::index(), 172),
            (BridgeIsmp::name(), BridgeIsmp::index(), 173),
            (Usdx::name(), Usdx::index(), 174),
            (HyperFungibleToken::name(), HyperFungibleToken::index(), 175),
        ];

        for (name, actual, stable) in expected {
            assert_eq!(actual, stable, "{name} index changed");
        }
    }
}
