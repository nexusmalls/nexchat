// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{types::Proposal, Config, ProposalsOf};
use alloc::{collections::BTreeMap, vec::Vec};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::DispatchError;

pub(crate) trait ProposalStorage<T: Config> {
    #[allow(dead_code)]
    fn count() -> u32;
    fn add(block_number: BlockNumberFor<T>, proposal: Proposal<T>) -> Result<(), DispatchError>;
    fn take(block_number: BlockNumberFor<T>) -> Result<ProposalsOf<T>, DispatchError>;
    #[allow(dead_code)]
    fn get(block_number: BlockNumberFor<T>) -> ProposalsOf<T>;
    fn mutate_all<R, F>(mutator: F) -> Result<BTreeMap<BlockNumberFor<T>, Vec<R>>, DispatchError>
    where
        F: FnMut(&mut Proposal<T>) -> R;
}
