//! Benchmarks for `pallet-chat-core`.
//! `pallet-chat-core` 的基准测试。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Chat;
use frame_benchmarking::v2::*;
use frame_support::{traits::Get, BoundedVec};
use frame_system::RawOrigin;
use sp_std::vec;

/// EN: Seed a 1:1 session between `a` and `b`, returning its id.
/// CN: 在 `a` 与 `b` 之间预置一个 1:1 会话，返回会话 id。
fn seed_session<T: Config>(a: &T::AccountId, b: &T::AccountId) -> T::Hash {
    let sid = Chat::<T>::get_session_id(a, b);
    let now = frame_system::Pallet::<T>::block_number();
    let mut participants: BoundedVec<T::AccountId, ConstU32<2>> = BoundedVec::default();
    participants.try_push(a.clone()).expect("1 <= 2");
    participants.try_push(b.clone()).expect("2 <= 2");
    let session = Session::<T> {
        id: sid,
        participants,
        last_message_id: 0,
        last_active: now,
        created_at: now,
        is_archived: false,
    };
    Sessions::<T>::insert(sid, session);
    sid
}

fn seed_system_message<T: Config>(
    sender: &T::AccountId,
    receiver: &T::AccountId,
    msg_id: u64,
) -> T::Hash {
    let sid = seed_session::<T>(sender, receiver);
    let now = frame_system::Pallet::<T>::block_number();
    let cid: BoundedVec<u8, T::MaxCidLen> = b"bench-cid".to_vec().try_into().expect("cid fits");
    Messages::<T>::insert(
        msg_id,
        MessageMeta::<T> {
            sender: sender.clone(),
            receiver: receiver.clone(),
            sender_chat_id: None,
            receiver_chat_id: None,
            content_cid: cid,
            session_id: sid,
            msg_type: MessageType::System,
            sent_at: now,
            is_read: false,
            is_deleted_by_sender: false,
            is_deleted_by_receiver: false,
            is_recalled: false,
        },
    );
    SessionMessages::<T>::insert(sid, msg_id, ());
    sid
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn send_message() {
        let receiver: T::AccountId = account("receiver", 0, 0);
        let cid: BoundedVec<u8, T::MaxCidLen> = b"bench-cid".to_vec().try_into().expect("cid fits");
        #[extrinsic_call]
        send_message(
            RawOrigin::Root,
            receiver,
            cid.to_vec(),
            4u8,
            None,
        );
    }

    #[benchmark]
    fn mark_as_read() {
        let sender: T::AccountId = account("sender", 0, 0);
        let receiver: T::AccountId = whitelisted_caller();
        seed_system_message::<T>(&sender, &receiver, 0);
        #[extrinsic_call]
        mark_as_read(RawOrigin::Signed(receiver), 0);
    }

    #[benchmark]
    fn delete_message() {
        let sender: T::AccountId = whitelisted_caller();
        let receiver: T::AccountId = account("receiver", 0, 0);
        seed_system_message::<T>(&sender, &receiver, 0);
        #[extrinsic_call]
        delete_message(RawOrigin::Signed(sender), 0);
    }

    #[benchmark]
    fn recall_message() {
        let sender: T::AccountId = whitelisted_caller();
        let receiver: T::AccountId = account("receiver", 0, 0);
        seed_system_message::<T>(&sender, &receiver, 0);
        #[extrinsic_call]
        recall_message(RawOrigin::Signed(sender), 0);
    }

    #[benchmark]
    fn mark_batch_as_read(n: Linear<1, 20>) {
        let sender: T::AccountId = account("sender", 0, 0);
        let receiver: T::AccountId = whitelisted_caller();
        let mut ids = vec![];
        for i in 0..n {
            seed_system_message::<T>(&sender, &receiver, i as u64);
            ids.push(i as u64);
        }
        #[extrinsic_call]
        mark_batch_as_read(RawOrigin::Signed(receiver), ids);
    }

    #[benchmark]
    fn mark_session_as_read(n: Linear<1, 20>) {
        let sender: T::AccountId = account("sender", 0, 0);
        let receiver: T::AccountId = whitelisted_caller();
        let sid = seed_session::<T>(&sender, &receiver);
        for i in 0..n {
            let cid: BoundedVec<u8, T::MaxCidLen> =
                b"bench-cid".to_vec().try_into().expect("cid fits");
            let now = frame_system::Pallet::<T>::block_number();
            Messages::<T>::insert(
                i as u64,
                MessageMeta::<T> {
                    sender: sender.clone(),
                    receiver: receiver.clone(),
                    sender_chat_id: None,
                    receiver_chat_id: None,
                    content_cid: cid,
                    session_id: sid,
                    msg_type: MessageType::System,
                    sent_at: now,
                    is_read: false,
                    is_deleted_by_sender: false,
                    is_deleted_by_receiver: false,
                    is_recalled: false,
                },
            );
            SessionMessages::<T>::insert(sid, i as u64, ());
        }
        #[extrinsic_call]
        mark_session_as_read(RawOrigin::Signed(receiver), sid);
    }

    #[benchmark]
    fn archive_session() {
        let caller: T::AccountId = whitelisted_caller();
        let other: T::AccountId = account("other", 0, 0);
        let sid = seed_session::<T>(&caller, &other);
        #[extrinsic_call]
        archive_session(RawOrigin::Signed(caller), sid);
    }

    #[benchmark]
    fn set_session_muted() {
        let caller: T::AccountId = whitelisted_caller();
        let other: T::AccountId = account("other", 0, 0);
        let sid = seed_session::<T>(&caller, &other);
        #[extrinsic_call]
        set_session_muted(RawOrigin::Signed(caller.clone()), sid, true);
    }

    #[benchmark]
    fn set_session_pinned() {
        let caller: T::AccountId = whitelisted_caller();
        let other: T::AccountId = account("other", 0, 0);
        let sid = seed_session::<T>(&caller, &other);
        #[extrinsic_call]
        set_session_pinned(RawOrigin::Signed(caller.clone()), sid, true);
    }

    #[benchmark]
    fn cleanup_old_messages(n: Linear<1, 20>) {
        let sender: T::AccountId = account("sender", 0, 0);
        let receiver: T::AccountId = account("receiver", 0, 0);
        let sid = seed_session::<T>(&sender, &receiver);
        let now = frame_system::Pallet::<T>::block_number();
        let expiration = T::MessageExpirationTime::get();
        let cid: BoundedVec<u8, T::MaxCidLen> =
            b"bench-cid".to_vec().try_into().expect("cid fits");
        for i in 0..n {
            Messages::<T>::insert(
                i as u64,
                MessageMeta::<T> {
                    sender: sender.clone(),
                    receiver: receiver.clone(),
                    sender_chat_id: None,
                    receiver_chat_id: None,
                    content_cid: cid.clone(),
                    session_id: sid,
                    msg_type: MessageType::System,
                    sent_at: now.saturating_sub(expiration),
                    is_read: false,
                    is_deleted_by_sender: true,
                    is_deleted_by_receiver: true,
                    is_recalled: false,
                },
            );
            SessionMessages::<T>::insert(sid, i as u64, ());
        }
        #[extrinsic_call]
        cleanup_old_messages(RawOrigin::Root, n);
    }

    #[benchmark]
    fn register_chat_user() {
        let caller: T::AccountId = whitelisted_caller();
        #[extrinsic_call]
        register_chat_user(RawOrigin::Signed(caller), None);
    }

    #[benchmark]
    fn update_chat_profile() {
        let caller: T::AccountId = whitelisted_caller();
        Chat::<T>::register_chat_user(RawOrigin::Signed(caller.clone()).into(), None).unwrap();
        #[extrinsic_call]
        update_chat_profile(
            RawOrigin::Signed(caller),
            Some(b"bench".to_vec()),
            None,
            None,
        );
    }

    #[benchmark]
    fn set_user_status() {
        let caller: T::AccountId = whitelisted_caller();
        Chat::<T>::register_chat_user(RawOrigin::Signed(caller.clone()).into(), None).unwrap();
        #[extrinsic_call]
        set_user_status(RawOrigin::Signed(caller), 0u8);
    }

    #[benchmark]
    fn update_privacy_settings() {
        let caller: T::AccountId = whitelisted_caller();
        Chat::<T>::register_chat_user(RawOrigin::Signed(caller.clone()).into(), None).unwrap();
        #[extrinsic_call]
        update_privacy_settings(
            RawOrigin::Signed(caller),
            Some(true),
            Some(false),
        );
    }

    impl_benchmark_test_suite!(Chat, crate::mock::new_test_ext(), crate::mock::Test);
}
