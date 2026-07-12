// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::{types::Proposal, weights::WeightInfoZeitgeist, Config, Event, Pallet};
use frame_support::{
    dispatch::RawOrigin,
    pallet_prelude::Weight,
    traits::schedule::{v3::Anon, DispatchTime, HARD_DEADLINE},
};
use zeitgeist_primitives::traits::FutarchyOracle;

impl<T: Config> Pallet<T> {
    pub(crate) fn maybe_schedule_proposal(proposal: Proposal<T>) -> Weight {
        let (evaluate_weight, approved) = proposal.oracle.evaluate();
        if approved {
            let result = T::Scheduler::schedule(
                DispatchTime::At(proposal.when),
                None,
                HARD_DEADLINE,
                RawOrigin::Root.into(),
                proposal.call.clone(),
            );
            if result.is_ok() {
                Self::deposit_event(Event::<T>::Scheduled { proposal });
            } else {
                Self::deposit_event(Event::<T>::UnexpectedSchedulerError);
            }
        } else {
            Self::deposit_event(Event::<T>::Rejected { proposal });
        }
        T::WeightInfo::maybe_schedule_proposal().saturating_add(evaluate_weight)
    }
}
