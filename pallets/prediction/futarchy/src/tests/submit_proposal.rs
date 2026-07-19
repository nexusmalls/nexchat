// Copyright 2024-2025 Forecasting Technologies LTD.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn proposal(value: bool) -> Proposal<Runtime> {
    Proposal {
        when: Default::default(),
        call: Bounded::Inline(vec![7u8; 128].try_into().unwrap()),
        oracle: MockOracle::new(Default::default(), value),
    }
}

#[test]
fn submit_proposal_schedules_proposals() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        let proposal = proposal(true);
        MockScheduler::set_return_value(Ok(()));
        assert_ok!(Futarchy::submit_proposal(
            RawOrigin::Root.into(),
            duration,
            proposal.clone()
        ));
        System::assert_last_event(
            Event::<Runtime>::Submitted {
                duration,
                proposal: proposal.clone(),
            }
            .into(),
        );
        let at = System::block_number() + duration;
        assert_eq!(Proposals::get(at).pop(), Some(proposal.clone()));
        utility::run_to_block(at);
        assert!(Proposals::<Runtime>::get(at).is_empty());
        assert!(MockScheduler::called_once_with(
            DispatchTime::At(proposal.when),
            proposal.call.clone()
        ));
        System::assert_last_event(Event::<Runtime>::Scheduled { proposal }.into());
    });
}

#[test]
fn submit_proposal_rejects_proposals() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        let proposal = proposal(false);
        MockScheduler::set_return_value(Ok(()));
        assert_ok!(Futarchy::submit_proposal(
            RawOrigin::Root.into(),
            duration,
            proposal.clone()
        ));
        let at = System::block_number() + duration;
        utility::run_to_block(at);
        assert!(Proposals::<Runtime>::get(at).is_empty());
        assert!(MockScheduler::not_called());
        System::assert_last_event(Event::<Runtime>::Rejected { proposal }.into());
    });
}

#[test]
fn submit_proposal_reports_scheduler_failure() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        let proposal = proposal(true);
        MockScheduler::set_return_value(Err(DispatchError::Other("scheduler failure")));
        assert_ok!(Futarchy::submit_proposal(
            RawOrigin::Root.into(),
            duration,
            proposal,
        ));
        utility::run_to_block(System::block_number() + duration);
        System::assert_last_event(Event::<Runtime>::UnexpectedSchedulerError.into());
    });
}

#[test]
fn submit_proposal_fails_on_bad_origin() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        assert_noop!(
            Futarchy::submit_proposal(Account::new(0).signed(), duration, proposal(false)),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn technical_committee_origin_submits_proposal() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        let proposal = proposal(false);
        assert_ok!(Futarchy::submit_proposal(
            Account::new(TechnicalCommittee::get()).signed(),
            duration,
            proposal.clone(),
        ));
        System::assert_last_event(Event::<Runtime>::Submitted { duration, proposal }.into());
    });
}

#[test]
fn submit_proposal_fails_if_duration_is_too_short() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get() - 1;
        assert_noop!(
            Futarchy::submit_proposal(RawOrigin::Root.into(), duration, proposal(false)),
            Error::<Runtime>::DurationTooShort
        );
    });
}

#[test]
fn submit_proposal_fails_if_cache_is_full() {
    ExtBuilder::build().execute_with(|| {
        let duration = <Runtime as Config>::MinDuration::get();
        let proposal = proposal(false);
        let at = System::block_number() + duration;
        let max = <Runtime as Config>::MaxProposals::get();
        let proposals: ProposalsOf<Runtime> =
            vec![proposal.clone(); max as usize].try_into().unwrap();
        Proposals::<Runtime>::insert(at, proposals);
        assert_noop!(
            Futarchy::submit_proposal(RawOrigin::Root.into(), duration, proposal),
            Error::<Runtime>::CacheFull
        );
    });
}
