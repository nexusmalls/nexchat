//! Unit tests for `pallet-chat-inbox`.
//! `pallet-chat-inbox` 单元测试。

use crate::{mock::*, ContactTag, Error, Event, InboxId, Inboxes};
use frame_support::{assert_noop, assert_ok};

const ID_A: InboxId = [1u8; 32];
const ID_B: InboxId = [2u8; 32];
const TAG1: ContactTag = [10u8; 32];
const TAG2: ContactTag = [20u8; 32];

fn last_event() -> Event<Test> {
    System::events()
        .into_iter()
        .rev()
        .find_map(|r| {
            if let RuntimeEvent::ChatInbox(e) = r.event {
                Some(e)
            } else {
                None
            }
        })
        .expect("expected a ChatInbox event")
}

#[test]
fn register_reserves_deposit_and_starts_at_epoch_zero() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_eq!(Balances::reserved_balance(1), 100);
        assert_eq!(ChatInbox::inbox_epoch(ID_A), Some(0));
        assert!(ChatInbox::inbox_exists(ID_A));
        assert_eq!(last_event(), Event::InboxRegistered { inbox_id: ID_A, controller: 1 });
    });
}

#[test]
fn cannot_register_same_inbox_twice() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        // even a different controller cannot claim the same id
        assert_noop!(
            ChatInbox::register_inbox(RuntimeOrigin::signed(2), ID_A),
            Error::<Test>::InboxAlreadyExists
        );
    });
}

#[test]
fn controller_inbox_cap_enforced() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), [1u8; 32]));
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), [2u8; 32]));
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), [3u8; 32]));
        assert_noop!(
            ChatInbox::register_inbox(RuntimeOrigin::signed(1), [4u8; 32]),
            Error::<Test>::TooManyInboxes
        );
    });
}

#[test]
fn bump_epoch_increments_and_clears_tags() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_ok!(ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_A, TAG1));
        assert!(ChatInbox::is_tag_revoked(ID_A, TAG1));

        assert_ok!(ChatInbox::bump_epoch(RuntimeOrigin::signed(1), ID_A));
        assert_eq!(ChatInbox::inbox_epoch(ID_A), Some(1));
        // epoch rotation clears the targeted-revocation set
        assert!(!ChatInbox::is_tag_revoked(ID_A, TAG1));
        assert_eq!(last_event(), Event::InboxEpochBumped { inbox_id: ID_A, new_epoch: 1 });
    });
}

#[test]
fn only_controller_can_mutate() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_noop!(
            ChatInbox::bump_epoch(RuntimeOrigin::signed(2), ID_A),
            Error::<Test>::NotController
        );
        assert_noop!(
            ChatInbox::revoke_tag(RuntimeOrigin::signed(2), ID_A, TAG1),
            Error::<Test>::NotController
        );
        assert_noop!(
            ChatInbox::deregister_inbox(RuntimeOrigin::signed(2), ID_A),
            Error::<Test>::NotController
        );
    });
}

#[test]
fn mutating_unknown_inbox_fails() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ChatInbox::bump_epoch(RuntimeOrigin::signed(1), ID_B),
            Error::<Test>::InboxNotFound
        );
        assert_noop!(
            ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_B, TAG1),
            Error::<Test>::InboxNotFound
        );
    });
}

#[test]
fn revoke_tag_targeted_and_deduplicated() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_ok!(ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_A, TAG1));
        assert!(ChatInbox::is_tag_revoked(ID_A, TAG1));
        // a different contact tag is unaffected
        assert!(!ChatInbox::is_tag_revoked(ID_A, TAG2));
        // revoking the same tag twice is rejected
        assert_noop!(
            ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_A, TAG1),
            Error::<Test>::TagAlreadyRevoked
        );
    });
}

#[test]
fn revoked_tag_set_is_bounded() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        // MaxRevokedTags = 4 in mock
        for i in 0..4u8 {
            assert_ok!(ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_A, [i; 32]));
        }
        assert_noop!(
            ChatInbox::revoke_tag(RuntimeOrigin::signed(1), ID_A, [99u8; 32]),
            Error::<Test>::TooManyRevokedTags
        );
    });
}

#[test]
fn deregister_returns_deposit_and_frees_slot() {
    new_test_ext().execute_with(|| {
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_eq!(Balances::reserved_balance(1), 100);

        assert_ok!(ChatInbox::deregister_inbox(RuntimeOrigin::signed(1), ID_A));
        assert_eq!(Balances::reserved_balance(1), 0);
        assert!(!ChatInbox::inbox_exists(ID_A));
        assert!(!Inboxes::<Test>::contains_key(ID_A));

        // slot is freed, so the same controller can register again
        assert_ok!(ChatInbox::register_inbox(RuntimeOrigin::signed(1), [2u8; 32]));
        assert_eq!(last_event(), Event::InboxRegistered { inbox_id: [2u8; 32], controller: 1 });
    });
}

#[test]
fn read_helpers_on_unregistered_inbox() {
    new_test_ext().execute_with(|| {
        assert_eq!(ChatInbox::inbox_epoch(ID_A), None);
        assert!(!ChatInbox::is_tag_revoked(ID_A, TAG1));
        assert!(!ChatInbox::inbox_exists(ID_A));
    });
}
