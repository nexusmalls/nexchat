// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

// External crates imports
use alloc::vec::Vec;
use frame_support::{
    genesis_builder_helper::{build_state, get_preset},
    traits::KeyOwnerProofSystem,
    weights::Weight,
};
use pallet_grandpa::AuthorityId as GrandpaId;
use sp_api::impl_runtime_apis;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};
use sp_runtime::{
    traits::{Block as BlockT, NumberFor},
    transaction_validity::{TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, ExtrinsicInclusionMode,
};
use sp_version::RuntimeVersion;

// Local module imports
use super::{
    AccountId, Arbitration, Babe, Balance, Block, BlockNumber, ChatCore, ChatGroup, ChatPermission,
    CommissionPoolReward, EntityMarket, EntityRegistry, Evidence, Executive, Grandpa, Hash,
    Historical, InherentDataExt, NexMarket, Nonce, Runtime, RuntimeCall, RuntimeGenesisConfig,
    SessionKeys, StorageService, System, TransactionPayment, VERSION,
};
impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> RuntimeVersion {
            VERSION
        }

        fn execute_block(block: <Block as BlockT>::LazyBlock) {
            Executive::execute_block(block.into());
        }

        fn initialize_block(header: &<Block as BlockT>::Header) -> ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata {
            OpaqueMetadata::new(Runtime::metadata().into())
        }

        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }

        fn metadata_versions() -> Vec<u32> {
            Runtime::metadata_versions()
        }
    }

    impl frame_support::view_functions::runtime_api::RuntimeViewFunction<Block> for Runtime {
        fn execute_view_function(id: frame_support::view_functions::ViewFunctionId, input: Vec<u8>) -> Result<Vec<u8>, frame_support::view_functions::ViewFunctionDispatchError> {
            Runtime::execute_view_function(id, input)
        }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }

        fn finalize_block() -> <Block as BlockT>::Header {
            Executive::finalize_block()
        }

        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }

        fn check_inherents(
            block: <Block as BlockT>::LazyBlock,
            data: sp_inherents::InherentData,
        ) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block.into())
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_consensus_babe::BabeApi<Block> for Runtime {
        fn configuration() -> sp_consensus_babe::BabeConfiguration {
            let epoch_config = Babe::epoch_config().unwrap_or(sp_consensus_babe::BabeEpochConfiguration {
                c: (1, 4),
                allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryVRFSlots,
            });
            sp_consensus_babe::BabeConfiguration {
                slot_duration: Babe::slot_duration(),
                epoch_length: crate::configs::EpochDuration::get(),
                c: epoch_config.c,
                authorities: Babe::authorities().to_vec(),
                randomness: Babe::randomness(),
                allowed_slots: epoch_config.allowed_slots,
            }
        }

        fn current_epoch_start() -> sp_consensus_babe::Slot {
            Babe::current_epoch_start()
        }

        fn current_epoch() -> sp_consensus_babe::Epoch {
            Babe::current_epoch()
        }

        fn next_epoch() -> sp_consensus_babe::Epoch {
            Babe::next_epoch()
        }

        fn generate_key_ownership_proof(
            _slot: sp_consensus_babe::Slot,
            authority_id: BabeId,
        ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
            use codec::Encode;
            Historical::prove((sp_consensus_babe::KEY_TYPE, authority_id))
                .map(|p| p.encode())
                .map(sp_consensus_babe::OpaqueKeyOwnershipProof::new)
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_babe::EquivocationProof<
                <Block as BlockT>::Header,
            >,
            key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Babe::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(_owner: Vec<u8>, seed: Option<Vec<u8>>) -> sp_session::OpaqueGeneratedSessionKeys {
            SessionKeys::generate(&[], seed).into()
        }

        fn decode_session_keys(
            encoded: Vec<u8>,
        ) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
            Grandpa::grandpa_authorities()
        }

        fn current_set_id() -> sp_consensus_grandpa::SetId {
            Grandpa::current_set_id()
        }

        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_grandpa::EquivocationProof<
                <Block as BlockT>::Hash,
                NumberFor<Block>,
            >,
            key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof = key_owner_proof.decode()?;
            Grandpa::submit_unsigned_equivocation_report(
                equivocation_proof,
                key_owner_proof,
            )
        }

        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            authority_id: GrandpaId,
        ) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
            use codec::Encode;
            Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id))
                .map(|p| p.encode())
                .map(sp_consensus_grandpa::OpaqueKeyOwnershipProof::new)
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce {
            System::account_nonce(account)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(
            uxt: <Block as BlockT>::Extrinsic,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<Block, Balance, RuntimeCall>
        for Runtime
    {
        fn query_call_info(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_call_info(call, len)
        }
        fn query_call_fee_details(
            call: RuntimeCall,
            len: u32,
        ) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_call_fee_details(call, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance {
            TransactionPayment::weight_to_fee(weight)
        }
        fn query_length_to_fee(length: u32) -> Balance {
            TransactionPayment::length_to_fee(length)
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    impl frame_benchmarking::Benchmark<Block> for Runtime {
        fn benchmark_metadata(extra: bool) -> (
            Vec<frame_benchmarking::BenchmarkList>,
            Vec<frame_support::traits::StorageInfo>,
        ) {
            use frame_benchmarking::{baseline, BenchmarkList};
            use frame_support::traits::StorageInfoTrait;
            use frame_system_benchmarking::Pallet as SystemBench;
            use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
            use baseline::Pallet as BaselineBench;
            use super::*;

            let mut list = Vec::<BenchmarkList>::new();
            list_benchmarks!(list, extra);

            let storage_info = AllPalletsWithSystem::storage_info();

            (list, storage_info)
        }

        #[allow(non_local_definitions)]
        fn dispatch_benchmark(
            config: frame_benchmarking::BenchmarkConfig
        ) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, alloc::string::String> {
            use frame_benchmarking::{baseline, BenchmarkBatch};
            use sp_storage::TrackedStorageKey;
            use frame_system_benchmarking::Pallet as SystemBench;
            use frame_system_benchmarking::extensions::Pallet as SystemExtensionsBench;
            use baseline::Pallet as BaselineBench;
            use super::*;

            impl frame_system_benchmarking::Config for Runtime {}
            impl baseline::Config for Runtime {}

            use frame_support::traits::WhitelistedStorageKeys;
            let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

            let mut batches = Vec::<BenchmarkBatch>::new();
            let params = (&config, &whitelist);
            add_benchmarks!(params, batches);

            Ok(batches)
        }
    }

    #[cfg(feature = "try-runtime")]
    impl frame_try_runtime::TryRuntime<Block> for Runtime {
        fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here. If any of the pre/post migration checks fail, we shall stop
            // right here and right now.
            let weight = Executive::try_runtime_upgrade(checks).unwrap();
            (weight, super::configs::RuntimeBlockWeights::get().max_block)
        }

        fn execute_block(
            block: <Block as BlockT>::LazyBlock,
            state_root_check: bool,
            signature_check: bool,
            select: frame_try_runtime::TryStateSelect
        ) -> Weight {
            // NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
            // have a backtrace here.
            Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
        }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }

        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, crate::genesis_config_presets::get_preset)
        }

        fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
            crate::genesis_config_presets::preset_names()
        }
    }

    impl pallet_entity_member::runtime_api::MemberTeamApi<Block, AccountId> for Runtime {
        fn get_member_info(entity_id: u64, account: AccountId) -> Option<pallet_entity_member::runtime_api::MemberDashboardInfo<AccountId>> {
            pallet_entity_member::Pallet::<Runtime>::get_member_info(entity_id, &account)
        }

        fn get_referral_team(entity_id: u64, account: AccountId, depth: u32) -> Vec<pallet_entity_member::runtime_api::TeamMemberInfo<AccountId>> {
            pallet_entity_member::Pallet::<Runtime>::get_referral_team(entity_id, &account, depth)
        }

        fn get_entity_member_overview(entity_id: u64) -> pallet_entity_member::runtime_api::EntityMemberOverview {
            pallet_entity_member::Pallet::<Runtime>::get_entity_member_overview(entity_id)
        }

        fn get_members_paginated(entity_id: u64, page_size: u32, page_index: u32) -> pallet_entity_member::runtime_api::PaginatedMembersResult<AccountId> {
            pallet_entity_member::Pallet::<Runtime>::get_members_paginated(entity_id, page_size, page_index)
        }

        fn get_upline_chain(entity_id: u64, account: AccountId, max_depth: u32) -> pallet_entity_member::runtime_api::UplineChainResult<AccountId> {
            pallet_entity_member::Pallet::<Runtime>::get_upline_chain(entity_id, &account, max_depth)
        }

        fn get_referral_tree(entity_id: u64, account: AccountId, depth: u32) -> pallet_entity_member::runtime_api::ReferralTreeNode<AccountId> {
            pallet_entity_member::Pallet::<Runtime>::get_referral_tree(entity_id, &account, depth)
        }

        fn get_referrals_by_generation(entity_id: u64, account: AccountId, generation: u32, page_size: u32, page_index: u32) -> pallet_entity_member::runtime_api::PaginatedGenerationResult<AccountId> {
            pallet_entity_member::Pallet::<Runtime>::get_referrals_by_generation(entity_id, &account, generation, page_size, page_index)
        }
    }

    impl pallet_commission_single_line::runtime_api::SingleLineQueryApi<Block, AccountId, Balance> for Runtime {
        fn single_line_member_position(
            entity_id: u64,
            account: AccountId,
        ) -> Option<pallet_commission_single_line::runtime_api::SingleLineMemberPositionInfo<AccountId>> {
            pallet_commission_single_line::Pallet::<Runtime>::single_line_member_position_info(entity_id, &account)
        }

        fn single_line_member_view(
            entity_id: u64,
            account: AccountId,
        ) -> Option<pallet_commission_single_line::runtime_api::SingleLineMemberView<AccountId, Balance>> {
            pallet_commission_single_line::Pallet::<Runtime>::single_line_member_view(entity_id, &account)
        }

        fn single_line_overview(
            entity_id: u64,
        ) -> pallet_commission_single_line::runtime_api::SingleLineOverview {
            pallet_commission_single_line::Pallet::<Runtime>::single_line_overview(entity_id)
        }

        fn single_line_member_payouts(
            entity_id: u64,
            account: AccountId,
        ) -> Vec<pallet_commission_single_line::runtime_api::SingleLinePayoutRecordView<AccountId, Balance>> {
            pallet_commission_single_line::Pallet::<Runtime>::single_line_member_payouts(entity_id, &account)
        }

        fn single_line_preview_commission(
            entity_id: u64,
            buyer: AccountId,
            order_amount: Balance,
        ) -> Vec<pallet_commission_single_line::runtime_api::SingleLinePreviewOutput<AccountId, Balance>> {
            pallet_commission_single_line::Pallet::<Runtime>::preview_single_line_commission(entity_id, &buyer, order_amount)
                .into_iter()
                .map(|o| pallet_commission_single_line::runtime_api::SingleLinePreviewOutput {
                    beneficiary: o.beneficiary,
                    amount: o.amount,
                    commission_type: o.commission_type,
                    level: o.level,
                })
                .collect()
        }
    }

    impl pallet_commission_core::runtime_api::CommissionDashboardApi<Block, AccountId, Balance, u128> for Runtime {
        fn get_member_commission_dashboard(
            entity_id: u64,
            account: AccountId,
        ) -> Option<pallet_commission_core::runtime_api::MemberCommissionDashboard<Balance, u128>> {
            pallet_commission_core::Pallet::<Runtime>::get_member_commission_dashboard(entity_id, &account)
        }

        fn get_direct_referral_info(
            entity_id: u64,
            account: AccountId,
        ) -> pallet_commission_core::runtime_api::DirectReferralInfo<Balance> {
            pallet_commission_core::Pallet::<Runtime>::get_direct_referral_info(entity_id, &account)
        }

        fn get_team_performance_info(
            entity_id: u64,
            account: AccountId,
        ) -> pallet_commission_core::runtime_api::TeamPerformanceInfo<Balance> {
            pallet_commission_core::Pallet::<Runtime>::get_team_performance_info(entity_id, &account)
        }

        fn get_entity_commission_overview(
            entity_id: u64,
        ) -> pallet_commission_core::runtime_api::EntityCommissionOverview<Balance, u128> {
            pallet_commission_core::Pallet::<Runtime>::get_entity_commission_overview(entity_id)
        }

        fn get_direct_referral_details(
            entity_id: u64,
            account: AccountId,
        ) -> pallet_commission_core::runtime_api::DirectReferralDetails<AccountId, Balance> {
            pallet_commission_core::Pallet::<Runtime>::get_direct_referral_details(entity_id, &account)
        }

        fn get_member_withdrawal_records(
            entity_id: u64,
            account: AccountId,
        ) -> Vec<pallet_commission_core::runtime_api::WithdrawalRecordView<Balance>> {
            pallet_commission_core::Pallet::<Runtime>::get_member_withdrawal_records(entity_id, &account)
        }
    }

    impl pallet_ads_core::runtime_api::AdsDiscoveryApi<Block, AccountId, Balance> for Runtime {
        fn available_campaigns_for_placement(
            placement_id: pallet_ads_primitives::PlacementId,
            max_results: u32,
        ) -> Vec<pallet_ads_core::runtime_api::CampaignSummary<AccountId, Balance>> {
            pallet_ads_core::Pallet::<Runtime>::available_campaigns_for_placement(&placement_id, max_results)
        }

        fn campaign_details(campaign_id: u64) -> Option<pallet_ads_core::runtime_api::CampaignDetail<AccountId, Balance>> {
            pallet_ads_core::Pallet::<Runtime>::campaign_details(campaign_id)
        }
    }

    impl pallet_nex_market::runtime_api::NexMarketApi<Block, AccountId, Balance> for Runtime {
        fn get_sell_orders(offset: u32, limit: u32) -> Vec<pallet_nex_market::runtime_api::OrderInfo<AccountId, Balance>> {
            NexMarket::api_get_sell_orders(offset, limit)
        }

        fn get_buy_orders(offset: u32, limit: u32) -> Vec<pallet_nex_market::runtime_api::OrderInfo<AccountId, Balance>> {
            NexMarket::api_get_buy_orders(offset, limit)
        }

        fn get_user_orders(user: AccountId) -> Vec<pallet_nex_market::runtime_api::OrderInfo<AccountId, Balance>> {
            NexMarket::api_get_user_orders(&user)
        }

        fn get_user_trades(user: AccountId, offset: u32, limit: u32) -> Vec<pallet_nex_market::runtime_api::TradeInfo<AccountId, Balance>> {
            NexMarket::api_get_user_trades(&user, offset, limit)
        }

        fn get_order_trades(order_id: u64) -> Vec<pallet_nex_market::runtime_api::TradeInfo<AccountId, Balance>> {
            NexMarket::api_get_order_trades(order_id)
        }

        fn get_active_trades(user: AccountId) -> Vec<pallet_nex_market::runtime_api::TradeInfo<AccountId, Balance>> {
            NexMarket::api_get_active_trades(&user)
        }

        fn get_order_depth() -> (
            Vec<pallet_nex_market::runtime_api::DepthEntry<Balance>>,
            Vec<pallet_nex_market::runtime_api::DepthEntry<Balance>>,
        ) {
            NexMarket::api_get_order_depth()
        }

        fn get_best_prices() -> (Option<u64>, Option<u64>) {
            NexMarket::get_best_prices()
        }

        fn get_market_summary() -> pallet_nex_market::runtime_api::MarketSummary {
            NexMarket::api_get_market_summary()
        }

        fn get_order_by_id(order_id: u64) -> Option<pallet_nex_market::runtime_api::OrderInfo<AccountId, Balance>> {
            NexMarket::api_get_order_by_id(order_id)
        }

        fn get_trade_by_id(trade_id: u64) -> Option<pallet_nex_market::runtime_api::TradeInfo<AccountId, Balance>> {
            NexMarket::api_get_trade_by_id(trade_id)
        }

        fn get_indexer_info(account: AccountId) -> Option<pallet_nex_market::runtime_api::IndexerInfoView<AccountId, Balance>> {
            NexMarket::api_get_indexer_info(account)
        }

        fn get_all_indexers() -> Vec<pallet_nex_market::runtime_api::IndexerInfoView<AccountId, Balance>> {
            NexMarket::api_get_all_indexers()
        }

        fn get_indexer_network_summary() -> pallet_nex_market::runtime_api::IndexerNetworkSummary<Balance> {
            NexMarket::api_get_indexer_network_summary()
        }
    }

    impl pallet_commission_pool_reward::runtime_api::PoolRewardDetailApi<Block, AccountId, Balance, u128> for Runtime {
        fn get_pool_reward_member_view(
            entity_id: u64,
            account: AccountId,
        ) -> Option<pallet_commission_pool_reward::runtime_api::PoolRewardMemberView<Balance, u128>> {
            CommissionPoolReward::get_pool_reward_member_view(entity_id, &account)
        }

        fn get_pool_reward_admin_view(
            entity_id: u64,
        ) -> Option<pallet_commission_pool_reward::runtime_api::PoolRewardAdminView<Balance, u128>> {
            CommissionPoolReward::get_pool_reward_admin_view(entity_id)
        }
    }

    impl pallet_storage_service::runtime_api::StorageServiceApi<Block, AccountId, Balance> for Runtime {
        fn get_user_funding_account(user: AccountId) -> AccountId {
            StorageService::derive_user_funding_account(&user)
        }

        fn get_user_funding_balance(user: AccountId) -> Balance {
            let funding_account = StorageService::derive_user_funding_account(&user);
            pallet_balances::Pallet::<Runtime>::free_balance(&funding_account)
        }

        fn get_subject_usage(user: AccountId, domain: u8, subject_id: u64) -> Balance {
            pallet_storage_service::SubjectUsage::<Runtime>::get((user, domain, subject_id))
        }

        fn get_user_all_usage(user: AccountId) -> Vec<(u8, u64, Balance)> {
            pallet_storage_service::SubjectUsage::<Runtime>::iter()
                .filter_map(|((u, domain, subject_id), amount)| {
                    if u == user {
                        Some((domain, subject_id, amount))
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    impl pallet_entity_registry::runtime_api::EntityRegistryApi<Block, AccountId, Balance, BlockNumber> for Runtime {
        fn get_entity(entity_id: u64) -> Option<pallet_entity_registry::runtime_api::EntityInfo<AccountId, Balance>> {
            EntityRegistry::api_get_entity(entity_id)
        }

        fn get_entities_by_owner(account: AccountId) -> Vec<u64> {
            EntityRegistry::api_get_entities_by_owner(&account)
        }

        fn get_entity_fund_info(entity_id: u64) -> Option<pallet_entity_registry::runtime_api::EntityFundInfo<Balance>> {
            EntityRegistry::api_get_entity_fund_info(entity_id)
        }

        fn get_verified_entities(offset: u32, limit: u32) -> Vec<u64> {
            EntityRegistry::api_get_verified_entities(offset, limit)
        }

        fn get_entity_by_name(name: Vec<u8>) -> Option<u64> {
            EntityRegistry::api_get_entity_by_name(name)
        }

        fn get_entity_admins(entity_id: u64) -> Vec<(AccountId, u32)> {
            EntityRegistry::api_get_entity_admins(entity_id)
        }

        fn get_entity_suspension_reason(entity_id: u64) -> Option<Vec<u8>> {
            EntityRegistry::api_get_entity_suspension_reason(entity_id)
        }

        fn get_entity_sales(entity_id: u64) -> Option<(Balance, u64)> {
            EntityRegistry::api_get_entity_sales(entity_id)
        }

        fn get_entity_referrer(entity_id: u64) -> Option<AccountId> {
            EntityRegistry::api_get_entity_referrer(entity_id)
        }

        fn get_referrer_entities(account: AccountId) -> Vec<u64> {
            EntityRegistry::api_get_referrer_entities(&account)
        }

        fn get_entities_by_status(status: pallet_entity_common::EntityStatus, offset: u32, limit: u32) -> Vec<u64> {
            EntityRegistry::api_get_entities_by_status(status, offset, limit)
        }

        fn check_admin_permission(entity_id: u64, account: AccountId) -> Option<u32> {
            EntityRegistry::api_check_admin_permission(entity_id, &account)
        }

        fn get_close_request_info(entity_id: u64) -> Option<pallet_entity_registry::runtime_api::CloseRequestInfo<BlockNumber>> {
            EntityRegistry::api_get_close_request_info(entity_id)
        }

        fn get_entity_funds(entity_id: u64) -> Option<pallet_entity_registry::runtime_api::EntityFundsView<Balance>> {
            EntityRegistry::api_get_entity_funds(entity_id)
        }
    }

    impl pallet_dispute_arbitration::runtime_api::ArbitrationApi<Block, AccountId, Balance> for Runtime {
        fn get_complaints_by_status(
            status: pallet_dispute_arbitration::types::ComplaintStatus,
            offset: u32,
            limit: u32,
        ) -> Vec<pallet_dispute_arbitration::runtime_api::ComplaintSummary<AccountId, Balance>> {
            Arbitration::api_get_complaints_by_status(status, offset, limit)
        }

        fn get_user_complaints(account: AccountId) -> Vec<u64> {
            Arbitration::api_get_user_complaints(&account)
        }

        fn get_complaint_detail(
            complaint_id: u64,
        ) -> Option<pallet_dispute_arbitration::runtime_api::ComplaintDetail<AccountId, Balance>> {
            Arbitration::api_get_complaint_detail(complaint_id)
        }
    }

    impl pallet_dispute_evidence::runtime_api::EvidenceApi<Block, AccountId, Balance> for Runtime {
        fn get_evidence_detail(id: u64) -> Option<pallet_dispute_evidence::runtime_api::EvidenceDetail<AccountId, Balance>> {
            Evidence::api_get_evidence_detail(id)
        }

        fn get_evidences_by_target(domain: u8, target_id: u64, offset: u32, limit: u32) -> pallet_dispute_evidence::runtime_api::EvidencePage<AccountId> {
            Evidence::api_get_evidences_by_target(domain, target_id, offset, limit)
        }

        fn get_evidences_by_ns(ns: [u8; 8], subject_id: u64, offset: u32, limit: u32) -> pallet_dispute_evidence::runtime_api::EvidencePage<AccountId> {
            Evidence::api_get_evidences_by_ns(ns, subject_id, offset, limit)
        }

        fn get_user_evidences(account: AccountId, offset: u32, limit: u32) -> pallet_dispute_evidence::runtime_api::EvidencePage<AccountId> {
            Evidence::api_get_user_evidences(&account, offset, limit)
        }

        fn get_private_content_meta(content_id: u64) -> Option<pallet_dispute_evidence::runtime_api::PrivateContentMeta<AccountId>> {
            Evidence::api_get_private_content_meta(content_id)
        }

        fn get_decryption_package(content_id: u64, viewer: AccountId) -> Option<pallet_dispute_evidence::runtime_api::DecryptionPackage> {
            Evidence::api_get_decryption_package(content_id, &viewer)
        }

        fn get_private_contents_by_subject(ns: [u8; 8], subject_id: u64, offset: u32, limit: u32) -> Vec<u64> {
            Evidence::api_get_private_contents_by_subject(ns, subject_id, offset, limit)
        }

        fn get_access_requests(content_id: u64, offset: u32, limit: u32) -> Vec<pallet_dispute_evidence::runtime_api::AccessRequestEntry<AccountId>> {
            Evidence::api_get_access_requests(content_id, offset, limit)
        }

        fn get_user_public_key(account: AccountId) -> Option<pallet_dispute_evidence::runtime_api::PublicKeyInfo> {
            Evidence::api_get_user_public_key(&account)
        }
    }

    impl pallet_entity_market::runtime_api::EntityMarketApi<Block, AccountId, Balance, Balance> for Runtime {
        fn get_sell_orders(entity_id: u64) -> Vec<pallet_entity_market::runtime_api::OrderInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_sell_orders(entity_id)
        }

        fn get_buy_orders(entity_id: u64) -> Vec<pallet_entity_market::runtime_api::OrderInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_buy_orders(entity_id)
        }

        fn get_user_orders(user: AccountId) -> Vec<pallet_entity_market::runtime_api::OrderInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_user_orders(&user)
        }

        fn get_order(order_id: u64) -> Option<pallet_entity_market::runtime_api::OrderInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_order(order_id)
        }

        fn get_order_book_depth(entity_id: u64, max_depth: u32) -> pallet_entity_market::runtime_api::OrderBookDepthInfo<Balance, Balance> {
            EntityMarket::api_get_order_book_depth(entity_id, max_depth)
        }

        fn get_best_prices(entity_id: u64) -> (Option<Balance>, Option<Balance>) {
            EntityMarket::api_get_best_prices(entity_id)
        }

        fn get_spread(entity_id: u64) -> Option<Balance> {
            EntityMarket::api_get_spread(entity_id)
        }

        fn get_market_summary(entity_id: u64) -> pallet_entity_market::runtime_api::MarketSummaryInfo<Balance, Balance> {
            EntityMarket::api_get_market_summary(entity_id)
        }

        fn get_order_book_snapshot(entity_id: u64) -> (Vec<(Balance, Balance)>, Vec<(Balance, Balance)>) {
            EntityMarket::api_get_order_book_snapshot(entity_id)
        }

        fn get_user_trade_history(user: AccountId, page: u32, page_size: u32) -> Vec<pallet_entity_market::runtime_api::TradeInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_user_trade_history(&user, page, page_size)
        }

        fn get_entity_trade_history(entity_id: u64, page: u32, page_size: u32) -> Vec<pallet_entity_market::runtime_api::TradeInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_entity_trade_history(entity_id, page, page_size)
        }

        fn get_user_order_history(user: AccountId, page: u32, page_size: u32) -> Vec<pallet_entity_market::runtime_api::OrderInfo<AccountId, Balance, Balance>> {
            EntityMarket::api_get_user_order_history(&user, page, page_size)
        }

        fn get_daily_stats(entity_id: u64) -> pallet_entity_market::runtime_api::DailyStatsInfo<Balance> {
            EntityMarket::api_get_daily_stats(entity_id)
        }

        fn get_global_stats() -> pallet_entity_market::runtime_api::MarketStatsInfo {
            EntityMarket::api_get_global_stats()
        }

        fn get_market_status(entity_id: u64) -> u8 {
            EntityMarket::api_get_market_status(entity_id)
        }

        fn get_market_config(entity_id: u64) -> Option<pallet_entity_market::runtime_api::MarketConfigInfo> {
            EntityMarket::api_get_market_config(entity_id)
        }

        fn get_kyc_requirement(entity_id: u64) -> u8 {
            EntityMarket::api_get_kyc_requirement(entity_id)
        }

        fn get_twap_info(entity_id: u64) -> pallet_entity_market::runtime_api::TwapInfo<Balance> {
            EntityMarket::api_get_twap_info(entity_id)
        }
    }

    // 统一会话视图：聚合私聊（ChatCore）与群聊（ChatGroup）为单一列表。
    // Unified conversation view: aggregate private (ChatCore) + group (ChatGroup)
    // into one list. Group unread / last_active are off-chain (see API docs).
    impl pallet_chat_common::runtime_api::ChatViewApi<Block, AccountId, Hash, BlockNumber> for Runtime {
        fn list_conversations(
            who: AccountId,
        ) -> Vec<pallet_chat_common::runtime_api::ConversationSummary<AccountId, Hash, BlockNumber>> {
            use pallet_chat_common::runtime_api::{ConversationKind, ConversationSummary, role};

            let mut out = Vec::new();

            // 私聊会话：core 已按"置顶优先 + 最后活跃倒序"排序。
            // Direct sessions: already sorted (pinned first, last-active desc) by core.
            for sid in ChatCore::list_sessions(who.clone()) {
                if let Some(session) = ChatCore::get_session(sid) {
                    let peer = session.participants.iter().find(|p| **p != who).cloned();
                    out.push(ConversationSummary {
                        kind: ConversationKind::Direct,
                        direct_id: Some(sid),
                        group_id: None,
                        peer,
                        name: Vec::new(),
                        avatar_cid: Vec::new(),
                        last_active: session.last_active,
                        unread: ChatCore::get_unread_count(who.clone(), Some(sid)),
                        pinned: ChatCore::is_session_pinned(&who, sid),
                        muted: ChatCore::is_session_muted(&who, sid),
                        archived: session.is_archived,
                        member_count: 0,
                        group_role: role::NONE,
                    });
                }
            }

            // 群聊会话：链上仅有元数据；活跃度/未读在链下，由客户端合并。
            // Group conversations: chain holds metadata only; recency/unread off-chain.
            for gid in ChatGroup::user_group_ids(&who) {
                let (name, avatar_cid) = ChatGroup::group_profile(gid)
                    .map(|p| (p.name.into_inner(), p.avatar_cid.into_inner()))
                    .unwrap_or_default();
                out.push(ConversationSummary {
                    kind: ConversationKind::Group,
                    direct_id: None,
                    group_id: Some(gid),
                    peer: None,
                    name,
                    avatar_cid,
                    last_active: BlockNumber::default(),
                    unread: 0,
                    pinned: false,
                    muted: ChatGroup::is_member_muted(gid, &who),
                    archived: false,
                    member_count: ChatGroup::group_member_count(gid),
                    group_role: ChatGroup::member_role_tag(gid, &who),
                });
            }

            out
        }

        fn total_direct_unread(who: AccountId) -> u32 {
            ChatCore::get_unread_count(who, None)
        }
    }

    // 聊天权限系统查询：检查权限 / 场景授权 / 能力撤销纪元 / 隐私设置摘要。
    // Chat permission queries: permission check / scene auth / capability epoch / privacy.
    impl pallet_chat_permission::runtime_api::ChatPermissionApi<Block, AccountId> for Runtime {
        fn check_chat_permission(
            sender: AccountId,
            receiver: AccountId,
        ) -> pallet_chat_permission::PermissionResult {
            ChatPermission::check_permission(&sender, &receiver)
        }

        fn get_active_scenes(
            user1: AccountId,
            user2: AccountId,
        ) -> Vec<pallet_chat_permission::SceneAuthorizationInfo> {
            ChatPermission::get_active_scenes(&user1, &user2)
        }

        fn capability_epoch(who: AccountId) -> u32 {
            ChatPermission::capability_epoch(&who)
        }

        fn is_account_muted(who: AccountId) -> bool {
            ChatPermission::is_account_muted(&who)
        }

        fn get_privacy_settings_summary(
            user: AccountId,
        ) -> pallet_chat_permission::PrivacySettingsSummary {
            ChatPermission::get_privacy_summary(&user)
        }
    }
}
