//! Benchmarks for `pallet-chat-core` P1 extrinsics.
//! `pallet-chat-core` P1 新增 extrinsic 的基准测试。
//!
//! EN: Scoped to the P1 additions (`recall_message`, `set_session_muted`,
//! `set_session_pinned`). The other extrinsics still use the conservative
//! hand-tuned `SubstrateWeight`; only the new calls are benchmarked here.
//! Sessions/messages are seeded directly into storage so the setup does not
//! depend on the permission gate / rate limit / CID validation of `send_message`.
//!
//! CN: 仅覆盖 P1 新增项（`recall_message` / `set_session_muted` /
//! `set_session_pinned`）。其余 extrinsic 仍用保守的手工 `SubstrateWeight`，
//! 此处只基准新调用。会话/消息直接写入 storage，避免基准设置依赖 `send_message`
//! 的权限闸门 / 限频 / CID 校验。

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Chat;
use frame_benchmarking::v2::*;
use frame_support::BoundedVec;
use frame_system::RawOrigin;

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

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn recall_message() {
        let sender: T::AccountId = whitelisted_caller();
        let receiver: T::AccountId = account("receiver", 0, 0);
        let sid = seed_session::<T>(&sender, &receiver);
        let now = frame_system::Pallet::<T>::block_number();
        // 未读 + 未读计数：走「撤回未读消息抵消未读」的较重分支。
        // Unread message + unread counter: exercises the heavier "offset unread" path.
        let meta = MessageMeta::<T> {
            sender: sender.clone(),
            receiver: receiver.clone(),
            sender_chat_id: None,
            receiver_chat_id: None,
            content_cid: BoundedVec::default(),
            session_id: sid,
            msg_type: MessageType::Text,
            sent_at: now,
            is_read: false,
            is_deleted_by_sender: false,
            is_deleted_by_receiver: false,
            is_recalled: false,
        };
        Messages::<T>::insert(0u64, meta);
        UnreadCount::<T>::insert((receiver, sid), 1u32);

        #[extrinsic_call]
        recall_message(RawOrigin::Signed(sender), 0u64);

        assert!(Messages::<T>::get(0u64).expect("message kept").is_recalled);
    }

    #[benchmark]
    fn set_session_muted() {
        let caller: T::AccountId = whitelisted_caller();
        let other: T::AccountId = account("other", 0, 0);
        let sid = seed_session::<T>(&caller, &other);

        #[extrinsic_call]
        set_session_muted(RawOrigin::Signed(caller.clone()), sid, true);

        assert!(SessionMuted::<T>::contains_key(&caller, sid));
    }

    #[benchmark]
    fn set_session_pinned() {
        let caller: T::AccountId = whitelisted_caller();
        let other: T::AccountId = account("other", 0, 0);
        let sid = seed_session::<T>(&caller, &other);

        #[extrinsic_call]
        set_session_pinned(RawOrigin::Signed(caller.clone()), sid, true);

        assert!(SessionPinned::<T>::contains_key(&caller, sid));
    }

    impl_benchmark_test_suite!(Chat, crate::mock::new_test_ext(), crate::mock::Test);
}
