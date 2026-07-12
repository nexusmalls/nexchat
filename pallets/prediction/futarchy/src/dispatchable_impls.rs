// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{traits::ProposalStorage, types::Proposal, Config, Error, Event, Pallet};
use frame_support::{ensure, require_transactional, traits::Get};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{DispatchResult, Saturating};

impl<T: Config> Pallet<T> {
    #[require_transactional]
    pub(crate) fn do_submit_proposal(
        duration: BlockNumberFor<T>,
        proposal: Proposal<T>,
    ) -> DispatchResult {
        ensure!(
            duration >= T::MinDuration::get(),
            Error::<T>::DurationTooShort
        );
        let now = frame_system::Pallet::<T>::block_number();
        let to_be_scheduled_at = now.saturating_add(duration);
        <Pallet<T> as ProposalStorage<T>>::add(to_be_scheduled_at, proposal.clone())?;
        Self::deposit_event(Event::<T>::Submitted { duration, proposal });
        Ok(())
    }
}
