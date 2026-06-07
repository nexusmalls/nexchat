use crate::pallet::*;
use crate::types::Decision;
use frame_support::{
    pallet_prelude::DispatchResult,
    traits::{
        fungible::MutateHold as FungibleMutateHold,
        tokens::{Fortitude, Precision, Restriction},
        Get,
    },
};
use pallet_dispute_escrow::pallet::Escrow as EscrowTrait;
use pallet_storage_service::CidLockManager;
use sp_runtime::{traits::Zero, DispatchError, SaturatedConversion, Saturating};

impl<T: Config> Pallet<T> {
    pub(crate) fn get_escrow_account() -> T::AccountId {
        T::Escrow::escrow_account()
    }

    pub(crate) fn handle_deposits_on_arbitration(
        domain: [u8; 8],
        id: u64,
        decision: &Decision,
    ) -> DispatchResult {
        if let Some(deposit_record) = TwoWayDeposits::<T>::take(domain, id) {
            let treasury = T::TreasuryAccount::get();
            let escrow_account = Self::get_escrow_account();

            match decision {
                Decision::Release => {
                    Self::slash_and_release(
                        &escrow_account,
                        deposit_record.initiator_deposit,
                        T::RejectedSlashBps::get(),
                        &HoldReason::DisputeInitiator,
                        &treasury,
                        domain,
                        id,
                    )?;
                    if let Some(respondent_deposit) = deposit_record.respondent_deposit {
                        Self::release_deposit(
                            &escrow_account,
                            respondent_deposit,
                            &HoldReason::DisputeRespondent,
                            domain,
                            id,
                        )?;
                    }
                }
                Decision::Refund => {
                    Self::release_deposit(
                        &escrow_account,
                        deposit_record.initiator_deposit,
                        &HoldReason::DisputeInitiator,
                        domain,
                        id,
                    )?;
                    if let Some(respondent_deposit) = deposit_record.respondent_deposit {
                        Self::slash_and_release(
                            &escrow_account,
                            respondent_deposit,
                            T::RejectedSlashBps::get(),
                            &HoldReason::DisputeRespondent,
                            &treasury,
                            domain,
                            id,
                        )?;
                    }
                }
                Decision::Partial(_) => {
                    Self::slash_and_release(
                        &escrow_account,
                        deposit_record.initiator_deposit,
                        T::PartialSlashBps::get(),
                        &HoldReason::DisputeInitiator,
                        &treasury,
                        domain,
                        id,
                    )?;
                    if let Some(respondent_deposit) = deposit_record.respondent_deposit {
                        Self::slash_and_release(
                            &escrow_account,
                            respondent_deposit,
                            T::PartialSlashBps::get(),
                            &HoldReason::DisputeRespondent,
                            &treasury,
                            domain,
                            id,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn slash_and_release(
        account: &T::AccountId,
        amount: BalanceOf<T>,
        slash_bps: u16,
        hold_reason: &HoldReason,
        treasury: &T::AccountId,
        domain: [u8; 8],
        object_id: u64,
    ) -> DispatchResult {
        let slash_amount =
            sp_runtime::Permill::from_parts((slash_bps as u32) * 100).mul_floor(amount);
        let release_amount = amount.saturating_sub(slash_amount);

        if !slash_amount.is_zero() {
            T::Fungible::transfer_on_hold(
                &T::RuntimeHoldReason::from(hold_reason.clone()),
                account,
                treasury,
                slash_amount,
                Precision::BestEffort,
                Restriction::Free,
                Fortitude::Force,
            )?;
        }

        if !release_amount.is_zero() {
            T::Fungible::release(
                &T::RuntimeHoldReason::from(hold_reason.clone()),
                account,
                release_amount,
                Precision::Exact,
            )?;
        }

        Self::deposit_event(Event::DepositProcessed {
            domain,
            id: object_id,
            account: account.clone(),
            released: release_amount,
            slashed: slash_amount,
        });

        Ok(())
    }

    pub(crate) fn release_deposit(
        account: &T::AccountId,
        amount: BalanceOf<T>,
        hold_reason: &HoldReason,
        domain: [u8; 8],
        object_id: u64,
    ) -> DispatchResult {
        T::Fungible::release(
            &T::RuntimeHoldReason::from(hold_reason.clone()),
            account,
            amount,
            Precision::Exact,
        )?;

        Self::deposit_event(Event::DepositProcessed {
            domain,
            id: object_id,
            account: account.clone(),
            released: amount,
            slashed: BalanceOf::<T>::zero(),
        });

        Ok(())
    }

    pub fn lock_evidence_cid(domain: [u8; 8], id: u64, cid_hash: T::Hash) -> DispatchResult {
        let reason = Self::build_lock_reason(domain, id);
        T::CidLockManager::lock_cid(cid_hash, reason, None)?;
        LockedCidHashes::<T>::try_mutate(domain, id, |hashes| -> Result<(), DispatchError> {
            hashes
                .try_push(cid_hash)
                .map_err(|_| Error::<T>::TooManyComplaints)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn unlock_all_evidence_cids(domain: [u8; 8], id: u64) -> DispatchResult {
        let reason = Self::build_lock_reason(domain, id);
        let locked_hashes = LockedCidHashes::<T>::take(domain, id);
        for cid_hash in locked_hashes.iter() {
            let _ = T::CidLockManager::unlock_cid(*cid_hash, reason.clone());
        }
        Ok(())
    }

    pub(crate) fn build_lock_reason(domain: [u8; 8], id: u64) -> alloc::vec::Vec<u8> {
        let mut reason = b"arb:".to_vec();
        reason.extend_from_slice(&domain);
        reason.push(b':');
        reason.extend_from_slice(&id.to_le_bytes());
        reason
    }

    pub(crate) fn archive_and_cleanup(domain: [u8; 8], id: u64, decision: u8, partial_bps: u16) {
        use crate::types::ArchivedDispute;

        let current_block: u64 = frame_system::Pallet::<T>::block_number().saturated_into();
        let archived = ArchivedDispute {
            domain,
            object_id: id,
            decision,
            partial_bps,
            completed_at: current_block,
            year_month: pallet_storage_lifecycle::block_to_year_month(
                current_block.saturated_into(),
                14400,
            ),
        };

        let archived_id = NextArchivedId::<T>::get();
        ArchivedDisputes::<T>::insert(archived_id, archived);
        NextArchivedId::<T>::put(archived_id.saturating_add(1));

        ArbitrationStats::<T>::mutate(|stats| {
            stats.total_disputes = stats.total_disputes.saturating_add(1);
            match decision {
                0 => stats.release_count = stats.release_count.saturating_add(1),
                1 => stats.refund_count = stats.refund_count.saturating_add(1),
                _ => stats.partial_count = stats.partial_count.saturating_add(1),
            }
        });

        Disputed::<T>::remove(domain, id);
        EvidenceIds::<T>::remove(domain, id);
        TwoWayDeposits::<T>::remove(domain, id);
    }

    /// 8 字节 domain 标识转 ASCII 标签（截断于首个 `0` 字节）。
    /// 8-byte domain id → ASCII tag (trim at first `0` byte).
    pub(crate) fn domain_tag(domain: [u8; 8]) -> alloc::vec::Vec<u8> {
        let n = domain.iter().position(|&b| b == 0).unwrap_or(8);
        domain[..n].to_vec()
    }

    /// `u64` → 十进制 ASCII（no_std 友好）。
    fn u64_ascii(mut n: u64) -> alloc::vec::Vec<u8> {
        if n == 0 {
            return alloc::vec![b'0'];
        }
        let mut buf = alloc::vec::Vec::new();
        while n > 0 {
            buf.push(b'0' + (n % 10) as u8);
            n /= 10;
        }
        buf.reverse();
        buf
    }

    /// 构造争议类通知描述符：`{kind}:{domain}:{id}[:{extra}]`。
    /// Build a dispute notice descriptor: `{kind}:{domain}:{id}[:{extra}]`.
    pub(crate) fn notice_dispute(
        kind: &[u8],
        domain: [u8; 8],
        id: u64,
        extra: Option<u8>,
    ) -> alloc::vec::Vec<u8> {
        let mut v = kind.to_vec();
        v.push(b':');
        v.extend_from_slice(&Self::domain_tag(domain));
        v.push(b':');
        v.extend_from_slice(&Self::u64_ascii(id));
        if let Some(e) = extra {
            v.push(b':');
            v.push(b'0' + e);
        }
        v
    }

    /// 构造投诉类通知描述符：`{kind}:{complaint_id}[:{extra}]`。
    /// Build a complaint notice descriptor: `{kind}:{complaint_id}[:{extra}]`.
    pub(crate) fn notice_complaint(
        kind: &[u8],
        complaint_id: u64,
        extra: Option<u8>,
    ) -> alloc::vec::Vec<u8> {
        let mut v = kind.to_vec();
        v.push(b':');
        v.extend_from_slice(&Self::u64_ascii(complaint_id));
        if let Some(e) = extra {
            v.push(b':');
            v.push(b'0' + e);
        }
        v
    }
}
