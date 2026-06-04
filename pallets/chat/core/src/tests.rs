//! # Chat Pallet 单元测试
//! 
//! 测试所有核心功能

use crate::{mock::*, Error, Event, MessageType};
use frame_support::{assert_noop, assert_ok};

/// 测试账户
const ALICE: u64 = 1;
const BOB: u64 = 2;
const CHARLIE: u64 = 3;
const DAVE: u64 = 4;

// ============================================================================
// 基础功能测试
// ============================================================================

#[test]
fn test_send_message_works() {
	new_test_ext().execute_with(|| {
		// 准备
		let cid = encrypted_cid(1);
		
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			cid.clone(),
			4, // System（人类消息已迁链下，链上仅 System）
			None
		));

		// 验证：消息已创建
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.sender, ALICE);
		assert_eq!(msg.receiver, BOB);
		assert_eq!(msg.content_cid.to_vec(), cid);
		assert_eq!(msg.msg_type, MessageType::System);
		assert_eq!(msg.is_read, false);
		assert_eq!(msg.is_deleted_by_sender, false);
		assert_eq!(msg.is_deleted_by_receiver, false);

		// 验证：会话已创建
		let sessions = Chat::list_sessions(ALICE);
		assert_eq!(sessions.len(), 1);

		// 验证：未读计数增加
		let unread = Chat::get_unread_count(BOB, None);
		assert_eq!(unread, 1);

		// 验证：事件已触发（`MessageSent` 之后还会发 `MessageSentWithChatId`，故用 has_event）。
		System::assert_has_event(
			Event::MessageSent {
				msg_id: 0,
				session_id: msg.session_id,
				sender: ALICE,
				receiver: BOB,
			}.into()
		);
	});
}

#[test]
fn test_send_message_rejects_empty_cid() {
	// 审计 C：链不再用可绕过的 `is_cid_encrypted` 启发式伪装“加密校验”。
	// 现在只做格式 sanity（非空）；加密交由客户端 MLS E2EE 保证。
	new_test_ext().execute_with(|| {
		assert_noop!(
			Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, Vec::new(), 4, None),
			Error::<Test>::InvalidCid
		);
	});
}

#[test]
fn test_send_message_accepts_standard_cidv0() {
	// 旧实现会以 `CidNotEncrypted` 拒绝标准 CIDv0（46 字节、Qm 前缀）。
	// 收敛后链不再判断加密，标准 CIDv0 视为合法 CID 被接受（刻意的姿态变更，审计 C）。
	new_test_ext().execute_with(|| {
		let standard_cidv0 = unencrypted_cid();
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			standard_cidv0,
			4,
			None
		));
	});
}

#[test]
fn test_send_message_rejects_cid_too_long() {
	new_test_ext().execute_with(|| {
		// CID超过100字节
		let too_long_cid = vec![0u8; 101];
		
		assert_noop!(
			Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				too_long_cid,
				4,
				None
			),
			Error::<Test>::CidTooLong
		);
	});
}

#[test]
fn test_multiple_messages_same_session() {
	new_test_ext().execute_with(|| {
		// 发送第一条消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		let session_id = Chat::get_message(0).unwrap().session_id;

		// 发送第二条消息（使用相同会话）
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(2),
			4,
			Some(session_id)
		));

		// BOB回复
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(BOB),
			ALICE,
			encrypted_cid(3),
			4,
			Some(session_id)
		));

		// 验证：会话只有一个
		let alice_sessions = Chat::list_sessions(ALICE);
		assert_eq!(alice_sessions.len(), 1);

		// 验证：会话消息列表有3条
		let messages = Chat::list_messages_by_session(session_id, 0, 100);
		assert_eq!(messages.len(), 3);

		// 验证：未读计数正确（BOB有2条未读，ALICE有1条未读）
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 2);
		assert_eq!(Chat::get_unread_count(ALICE, Some(session_id)), 1);
	});
}

// ============================================================================
// C0 安全急修：会话注入校验（审计 D）
// ============================================================================

#[test]
fn test_send_message_rejects_foreign_session_neither_party() {
	new_test_ext().execute_with(|| {
		// BOB 与 CHARLIE 建立会话 S_bc。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(BOB),
			CHARLIE,
			encrypted_cid(1),
			4,
			None
		));
		let foreign_session = Chat::get_message(0).unwrap().session_id;

		// ALICE 试图借 BOB↔CHARLIE 的会话向 DAVE 发消息：双方都不是参与者 → 拒绝。
		assert_noop!(
			Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				DAVE,
				encrypted_cid(2),
				4,
				Some(foreign_session)
			),
			Error::<Test>::NotSessionParticipant
		);
	});
}

#[test]
fn test_send_message_rejects_session_with_only_sender() {
	new_test_ext().execute_with(|| {
		// ALICE 与 BOB 建立会话 S_ab。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		let session_ab = Chat::get_message(0).unwrap().session_id;

		// ALICE 是 S_ab 参与者，但 CHARLIE 不是；借 S_ab 给 CHARLIE 发消息 → 拒绝。
		// 防止把消息注入到接收方并非参与者的会话索引中。
		assert_noop!(
			Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				CHARLIE,
				encrypted_cid(2),
				4,
				Some(session_ab)
			),
			Error::<Test>::NotSessionParticipant
		);
	});
}

#[test]
fn test_send_message_rejects_nonexistent_session() {
	new_test_ext().execute_with(|| {
		// 传入一个不存在的会话 ID → SessionNotFound。
		let bogus = sp_core::H256::repeat_byte(0xAB);
		assert_noop!(
			Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(1),
				4,
				Some(bogus)
			),
			Error::<Test>::SessionNotFound
		);
	});
}

#[test]
fn test_send_message_accepts_own_session() {
	new_test_ext().execute_with(|| {
		// 合法路径：先建会话，再用同一会话继续发送应成功（回归保护，确保修复不误伤）。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		let session_ab = Chat::get_message(0).unwrap().session_id;

		// ALICE → BOB 复用会话。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(2),
			4,
			Some(session_ab)
		));

		// BOB → ALICE 复用同一会话（接收方/发送方互换仍是参与者）。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(BOB),
			ALICE,
			encrypted_cid(3),
			4,
			Some(session_ab)
		));

		let messages = Chat::list_messages_by_session(session_ab, 0, 100);
		assert_eq!(messages.len(), 3);
	});
}

// ============================================================================
// C0 安全急修：ChatUserId 生成计数器（审计 F，去除全表扫描）
// ============================================================================

#[test]
fn test_chat_user_id_counter_advances_and_ids_unique() {
	new_test_ext().execute_with(|| {
		// 初始计数器为 0。
		assert_eq!(Chat::next_chat_user_id(), 0);

		// 批量注册多个用户，验证 ID 唯一且计数器单调推进（O(1) 路径无全表扫描）。
		let accounts: Vec<u64> = (100u64..150u64).collect();
		let mut ids = alloc::collections::BTreeSet::new();
		for acct in &accounts {
			assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(*acct), None));
			let id = Chat::get_chat_user_id_by_account(acct).unwrap();
			assert!(id >= 10_000_000_000 && id <= 99_999_999_999);
			// 不允许重复 ID。
			assert!(ids.insert(id), "duplicate chat user id generated: {}", id);
		}

		// 计数器至少推进了注册次数（每次成功生成至少自增一次）。
		assert!(Chat::next_chat_user_id() >= accounts.len() as u64);
		assert_eq!(ids.len(), accounts.len());
	});
}

#[test]
fn test_chat_user_id_unique_within_same_block() {
	new_test_ext().execute_with(|| {
		// 同一区块内多个账户注册：nonce 计数器保证种子互异，不应碰撞失败。
		// （旧实现依赖 UsedChatUserIds 全表扫描区分，此处验证新计数器路径。）
		for acct in 200u64..210u64 {
			assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(acct), None));
		}
		let ids: Vec<u64> = (200u64..210u64)
			.map(|a| Chat::get_chat_user_id_by_account(&a).unwrap())
			.collect();
		let unique: alloc::collections::BTreeSet<u64> = ids.iter().copied().collect();
		assert_eq!(unique.len(), ids.len());
	});
}

// ============================================================================
// 已读未读功能测试
// ============================================================================

#[test]
fn test_mark_as_read_works() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// BOB标记已读
		assert_ok!(Chat::mark_as_read(RuntimeOrigin::signed(BOB), 0));

		// 验证：消息已读
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_read, true);

		// 验证：未读计数减少
		let unread = Chat::get_unread_count(BOB, None);
		assert_eq!(unread, 0);

		// 验证：事件已触发
		System::assert_last_event(
			Event::MessageRead {
				msg_id: 0,
				reader: BOB,
			}.into()
		);
	});
}

#[test]
fn test_mark_as_read_rejects_non_receiver() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// CHARLIE尝试标记已读
		assert_noop!(
			Chat::mark_as_read(RuntimeOrigin::signed(CHARLIE), 0),
			Error::<Test>::NotReceiver
		);
	});
}

#[test]
fn test_mark_batch_as_read_works() {
	new_test_ext().execute_with(|| {
		// 发送3条消息
		for i in 1..=3 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		// 验证：BOB有3条未读
		assert_eq!(Chat::get_unread_count(BOB, None), 3);

		// BOB批量标记已读
		assert_ok!(Chat::mark_batch_as_read(
			RuntimeOrigin::signed(BOB),
			vec![0, 1, 2]
		));

		// 验证：所有消息已读
		assert!(Chat::get_message(0).unwrap().is_read);
		assert!(Chat::get_message(1).unwrap().is_read);
		assert!(Chat::get_message(2).unwrap().is_read);

		// 验证：未读计数清零
		assert_eq!(Chat::get_unread_count(BOB, None), 0);
	});
}

#[test]
fn test_mark_batch_as_read_rejects_empty_list() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Chat::mark_batch_as_read(RuntimeOrigin::signed(BOB), vec![]),
			Error::<Test>::EmptyMessageList
		);
	});
}

#[test]
fn test_mark_session_as_read_works() {
	new_test_ext().execute_with(|| {
		// 发送3条消息
		for i in 1..=3 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		let session_id = Chat::get_message(0).unwrap().session_id;

		// BOB标记整个会话已读
		assert_ok!(Chat::mark_session_as_read(
			RuntimeOrigin::signed(BOB),
			session_id
		));

		// 验证：所有消息已读
		assert!(Chat::get_message(0).unwrap().is_read);
		assert!(Chat::get_message(1).unwrap().is_read);
		assert!(Chat::get_message(2).unwrap().is_read);

		// 验证：未读计数清零
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 0);
	});
}

// ============================================================================
// 删除功能测试
// ============================================================================

#[test]
fn test_delete_message_by_sender() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// ALICE删除消息
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 0));

		// 验证：消息已软删除（仅对发送方）
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_deleted_by_sender, true);
		assert_eq!(msg.is_deleted_by_receiver, false);

		// 验证：事件已触发
		System::assert_last_event(
			Event::MessageDeleted {
				msg_id: 0,
				deleter: ALICE,
			}.into()
		);
	});
}

#[test]
fn test_delete_message_by_receiver() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// 删除前：BOB 有 1 条未读 / before delete: BOB has 1 unread
		let session_id = Chat::get_message(0).unwrap().session_id;
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 1);

		// BOB删除消息
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 0));

		// 验证：消息已软删除（仅对接收方）
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_deleted_by_sender, false);
		assert_eq!(msg.is_deleted_by_receiver, true);

		// B1 回归：接收方删除未读消息后未读计数被抵消，且幂等（重复删除不再扣减）。
		// B1 regression: deleting an unread message offsets the unread count, and
		// is idempotent (a repeated delete does not decrement again).
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 0);
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 0));
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 0);
	});
}

#[test]
fn test_delete_message_rejects_unauthorized() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// CHARLIE尝试删除消息
		assert_noop!(
			Chat::delete_message(RuntimeOrigin::signed(CHARLIE), 0),
			Error::<Test>::NotAuthorized
		);
	});
}

// ============================================================================
// 会话管理测试
// ============================================================================

#[test]
fn test_list_sessions_works() {
	new_test_ext().execute_with(|| {
		// ALICE与BOB聊天
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// ALICE与CHARLIE聊天
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			CHARLIE,
			encrypted_cid(2),
			4,
			None
		));

		// 验证：ALICE有2个会话
		let sessions = Chat::list_sessions(ALICE);
		assert_eq!(sessions.len(), 2);

		// 验证：BOB和CHARLIE各有1个会话
		assert_eq!(Chat::list_sessions(BOB).len(), 1);
		assert_eq!(Chat::list_sessions(CHARLIE).len(), 1);
	});
}

#[test]
fn test_archive_session_works() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		let session_id = Chat::get_message(0).unwrap().session_id;

		// ALICE归档会话
		assert_ok!(Chat::archive_session(
			RuntimeOrigin::signed(ALICE),
			session_id
		));

		// 验证：会话已归档
		let session = Chat::get_session(session_id).unwrap();
		assert_eq!(session.is_archived, true);

		// 验证：事件已触发
		System::assert_last_event(
			Event::SessionArchived {
				session_id,
				operator: ALICE,
			}.into()
		);
	});
}

#[test]
fn test_archive_session_rejects_non_participant() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		let session_id = Chat::get_message(0).unwrap().session_id;

		// CHARLIE尝试归档会话
		assert_noop!(
			Chat::archive_session(RuntimeOrigin::signed(CHARLIE), session_id),
			Error::<Test>::NotSessionParticipant
		);
	});
}

// ============================================================================
// 查询功能测试
// ============================================================================

#[test]
fn test_get_message_works() {
	new_test_ext().execute_with(|| {
		// 发送消息
		let cid = encrypted_cid(1);
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			cid.clone(),
			4,
			None
		));

		// 查询消息
		let msg = Chat::get_message(0);
		assert!(msg.is_some());
		assert_eq!(msg.unwrap().content_cid.to_vec(), cid);
	});
}

#[test]
fn test_get_message_returns_none() {
	new_test_ext().execute_with(|| {
		// 查询不存在的消息
		let msg = Chat::get_message(999);
		assert!(msg.is_none());
	});
}

#[test]
fn test_list_messages_by_session_works() {
	new_test_ext().execute_with(|| {
		// 发送5条消息
		for i in 1..=5 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		let session_id = Chat::get_message(0).unwrap().session_id;

		// 查询全部消息
		let messages = Chat::list_messages_by_session(session_id, 0, 100);
		assert_eq!(messages.len(), 5);

		// 验证：倒序返回（最新的在前）
		assert_eq!(messages[0], 4); // 最新消息
		assert_eq!(messages[4], 0); // 最早消息

		// 测试分页：跳过2条，取2条
		let page2 = Chat::list_messages_by_session(session_id, 2, 2);
		assert_eq!(page2.len(), 2);
		assert_eq!(page2[0], 2);
		assert_eq!(page2[1], 1);
	});
}

#[test]
fn test_list_messages_pagination() {
	new_test_ext().execute_with(|| {
		// 发送10条消息
		for i in 1..=10 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		let session_id = Chat::get_message(0).unwrap().session_id;

		// 第一页：0-2
		let page1 = Chat::list_messages_by_session(session_id, 0, 3);
		assert_eq!(page1.len(), 3);
		assert_eq!(page1, vec![9, 8, 7]); // 倒序

		// 第二页：3-5
		let page2 = Chat::list_messages_by_session(session_id, 3, 3);
		assert_eq!(page2.len(), 3);
		assert_eq!(page2, vec![6, 5, 4]);

		// 超出范围
		let page_empty = Chat::list_messages_by_session(session_id, 100, 10);
		assert_eq!(page_empty.len(), 0);
	});
}

#[test]
fn test_get_unread_count_works() {
	new_test_ext().execute_with(|| {
		// 初始未读数为0
		assert_eq!(Chat::get_unread_count(BOB, None), 0);

		// ALICE发送2条消息给BOB
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(2),
			4,
			None
		));

		// CHARLIE发送1条消息给BOB
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(CHARLIE),
			BOB,
			encrypted_cid(3),
			4,
			None
		));

		// 验证：BOB总未读数为3
		assert_eq!(Chat::get_unread_count(BOB, None), 3);

		// 验证：指定会话的未读数
		let session_id = Chat::get_message(0).unwrap().session_id;
		assert_eq!(Chat::get_unread_count(BOB, Some(session_id)), 2);
	});
}

#[test]
fn test_get_session_works() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		let session_id = Chat::get_message(0).unwrap().session_id;

		// 查询会话
		let session = Chat::get_session(session_id);
		assert!(session.is_some());

		let s = session.unwrap();
		assert_eq!(s.participants.len(), 2);
		assert!(s.participants.contains(&ALICE));
		assert!(s.participants.contains(&BOB));
		assert_eq!(s.last_message_id, 0);
		assert_eq!(s.is_archived, false);
	});
}

// ============================================================================
// 消息类型测试
// ============================================================================

#[test]
fn test_send_message_rejects_human_types_onchain() {
	new_test_ext().execute_with(|| {
		// C 方案：人类消息（Text/Image/File/Voice，含未知类型）一律拒绝上链，
		// 改走链下 MLS + relay。仅 System（code 4）可经 send_message 落库。
		// Human types (and unknown) are rejected on-chain; only System is accepted.
		for code in [0u8, 1, 2, 3, 99] {
			assert_noop!(
				Chat::send_message(
					RuntimeOrigin::signed(ALICE),
					BOB,
					encrypted_cid(1),
					code,
					None
				),
				Error::<Test>::HumanMessagesOffChain
			);
		}

		// System（code 4）被接受并落库。/ System (code 4) is accepted and stored.
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(5),
			4, // System
			None
		));
		assert_eq!(Chat::get_message(0).unwrap().msg_type, MessageType::System);
	});
}

// ============================================================================
// 边界条件测试
// ============================================================================

#[test]
fn test_session_id_deterministic() {
	new_test_ext().execute_with(|| {
		// ALICE -> BOB
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		let session_id1 = Chat::get_message(0).unwrap().session_id;

		// BOB -> ALICE (应该是同一个会话)
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(BOB),
			ALICE,
			encrypted_cid(2),
			4,
			None
		));
		let session_id2 = Chat::get_message(1).unwrap().session_id;

		// 验证：会话ID相同
		assert_eq!(session_id1, session_id2);

		// 验证：ALICE和BOB都只有一个会话
		assert_eq!(Chat::list_sessions(ALICE).len(), 1);
		assert_eq!(Chat::list_sessions(BOB).len(), 1);
	});
}

#[test]
fn test_duplicate_mark_as_read() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// 第一次标记已读
		assert_ok!(Chat::mark_as_read(RuntimeOrigin::signed(BOB), 0));
		assert_eq!(Chat::get_unread_count(BOB, None), 0);

		// 第二次标记已读（应该成功但不影响计数）
		assert_ok!(Chat::mark_as_read(RuntimeOrigin::signed(BOB), 0));
		assert_eq!(Chat::get_unread_count(BOB, None), 0);
	});
}

// ============================================================================
// System 通道：受信来源，绕过接收方权限闸门
// System channel: trusted origin, bypasses the recipient permission gate
// ============================================================================
//
// 链上消息当前仅 System 类，且来自受信特权来源（生产为治理 / Root，见
// `SystemMessageOrigin`）。平台通知必须无视接收方隐私级别送达，故 System 绕过
// `ChatPermission::can_send_message` 闸门。人类消息（会被门控的非 System 路径）
// 已迁出链下。以下用可配置的 mock 权限端口验证：即便 chat-permission 拒绝，
// System 消息仍成功送达。
// On-chain messages are currently System-only and come from a trusted privileged
// origin; platform notifications must reach the recipient regardless of privacy,
// so System bypasses the `can_send_message` gate. Gateable human messages are
// off-chain. The tests below assert delivery succeeds even when permission denies.

#[test]
fn test_system_message_bypasses_permission_gate() {
	new_test_ext().execute_with(|| {
		// System 消息来自受信来源（生产为治理 / Root），是平台通知，必须无视接收方隐私
		// 级别送达：即便 chat-permission 拒绝 ALICE → BOB，System 消息仍应成功落库。
		// System messages come from a trusted origin and must reach the recipient
		// regardless of privacy level, so they bypass the permission gate.
		crate::mock::deny_permission(ALICE, BOB);

		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		assert_eq!(Chat::get_message(0).unwrap().msg_type, MessageType::System);
	});
}

#[test]
fn test_send_message_allowed_when_permission_grants() {
	new_test_ext().execute_with(|| {
		// 未注入拒绝项 → chat-permission 放行 → 发送成功（回归保护，确保闸门不误伤）。
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));
		assert!(Chat::get_message(0).is_some());
	});
}

// ============================================================================
// C2 职责收窄：send_system_message（仅 System 类）与统一权限闸门
// ============================================================================

#[test]
fn test_send_system_message_sets_system_type() {
	new_test_ext().execute_with(|| {
		// send_system_message 强制 MessageType::System，并复用同一套校验与落库。
		assert_ok!(Chat::send_system_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			None
		));
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.msg_type, MessageType::System);
		assert_eq!(msg.sender, ALICE);
		assert_eq!(msg.receiver, BOB);
	});
}

#[test]
fn test_send_system_message_bypasses_permission_gate() {
	new_test_ext().execute_with(|| {
		// 系统消息来自受信来源，不受接收方隐私级别约束：即便 chat-permission 拒绝，
		// `send_system_message` 仍成功送达。
		// System messages come from a trusted origin and are not gated by the
		// recipient's privacy level: even when chat-permission denies, delivery succeeds.
		crate::mock::deny_permission(ALICE, BOB);
		assert_ok!(Chat::send_system_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			None
		));
		assert_eq!(Chat::get_message(0).unwrap().msg_type, MessageType::System);
	});
}

// ============================================================================
// System 通道：不受反垃圾限频约束（受信来源）
// System channel: not subject to anti-spam rate limiting (trusted origin)
// ============================================================================

#[test]
fn test_system_messages_not_rate_limited() {
	new_test_ext().execute_with(|| {
		// System 消息来自受信特权来源（生产为治理 / Root），不应被反垃圾限频拦截。
		// 连发远超 `MaxMessagesPerWindow`（=10）的条数，全部应成功。
		// System messages come from a trusted origin and must not be throttled by
		// the anti-spam rate limit; sending well beyond the window cap all succeed.
		for i in 1..=25u8 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}
		// 第 25 条仍成功（不存在 RateLimitExceeded）。/ the 25th still succeeded.
		assert!(Chat::get_message(24).is_some());
	});
}

// ============================================================================
// P1新功能测试：分别软删除
// ============================================================================

#[test]
fn test_delete_message_sender_and_receiver_separate() {
	new_test_ext().execute_with(|| {
		// 发送消息
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			encrypted_cid(1),
			4,
			None
		));

		// ALICE（发送方）删除
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 0));

		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_deleted_by_sender, true);
		assert_eq!(msg.is_deleted_by_receiver, false);

		// BOB（接收方）也删除
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 0));

		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_deleted_by_sender, true);
		assert_eq!(msg.is_deleted_by_receiver, true);
	});
}

// ============================================================================
// P1新功能测试：无限消息和会话
// ============================================================================

#[test]
fn test_unlimited_messages_in_session() {
	new_test_ext().execute_with(|| {
		// 发送超过1000条消息（旧的BoundedVec限制）
		// 使用频率限制窗口，每100个区块发送10条
		let mut total_sent = 0;
		for batch in 0..105 {
			// 推进区块（超过窗口期以重置频率限制）
			System::set_block_number(batch * 101 + 1);
			
			// 发送10条消息
			for _ in 0..10 {
				if total_sent >= 1050 {
					break; // 发送1050条即可证明突破限制
				}
				assert_ok!(Chat::send_message(
					RuntimeOrigin::signed(ALICE),
					BOB,
					encrypted_cid((total_sent % 256) as u8),
					4,
					None
				));
				total_sent += 1;
			}
			if total_sent >= 1050 {
				break;
			}
		}

		// 验证：消息数量超过1000
		let session_id = Chat::get_message(0).unwrap().session_id;
		
		// 验证：能查询到最新的100条消息
		let messages = Chat::list_messages_by_session(session_id, 0, 100);
		assert_eq!(messages.len(), 100); // 查询最新100条（limit被限制为100）

		// 验证：能查询到更多消息（分页）
		let messages_page2 = Chat::list_messages_by_session(session_id, 100, 100);
		assert_eq!(messages_page2.len(), 100);
		
		let messages_page3 = Chat::list_messages_by_session(session_id, 200, 100);
		assert_eq!(messages_page3.len(), 100);
		
		// 验证：总消息数已超过1000（证明突破了旧的BoundedVec限制）
		assert_eq!(total_sent, 1050);
		
		// 分页查询多次，验证至少有1000条消息
		let mut all_msg_count = 0;
		for page in 0..11 {
			let msgs = Chat::list_messages_by_session(session_id, page * 100, 100);
			all_msg_count += msgs.len();
			if msgs.len() < 100 {
				break;
			}
		}
		assert!(all_msg_count >= 1000);
	});
}

// ============================================================================
// P2 新功能测试
// ============================================================================

#[test]
fn test_cleanup_old_messages_works() {
	new_test_ext().execute_with(|| {
		// 发送3条消息
		for i in 0..3 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		// 双方都删除消息
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 0));
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 0));
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 1));
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 1));

		// 推进区块，使消息过期（超过1000个区块）
		System::set_block_number(1002);

		// 验证：消息存在
		assert!(Chat::get_message(0).is_some());
		assert!(Chat::get_message(1).is_some());
		assert!(Chat::get_message(2).is_some());

		// 执行清理（仅 Root/治理可调；只清理双方都删除的过期消息）
		assert_ok!(Chat::cleanup_old_messages(RuntimeOrigin::root(), 100));

		// 验证：双方都删除的消息被清理
		assert!(Chat::get_message(0).is_none());
		assert!(Chat::get_message(1).is_none());
		// 验证：未被双方都删除的消息仍存在
		assert!(Chat::get_message(2).is_some());

		// 验证：事件已触发（治理触发，无操作者账户）
		System::assert_has_event(
			Event::OldMessagesCleanedUp {
				count: 2,
			}
			.into(),
		);
	});
}

#[test]
fn test_cleanup_old_messages_rejects_non_root() {
	new_test_ext().execute_with(|| {
		// C2：清理改为 Root/治理限定，普通签名账户调用应被拒绝（BadOrigin）。
		assert_noop!(
			Chat::cleanup_old_messages(RuntimeOrigin::signed(ALICE), 100),
			sp_runtime::DispatchError::BadOrigin
		);
	});
}

#[test]
fn test_cleanup_old_messages_with_limit() {
	new_test_ext().execute_with(|| {
		// 发送5条消息并双方都删除
		for i in 0..5 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
			assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), i as u64));
			assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), i as u64));
		}

		// 推进区块，使消息过期
		System::set_block_number(1002);

		// 执行清理，扫描预算 3（5 条全部命中且位于扫描预算内 → 清理 3 条）
		assert_ok!(Chat::cleanup_old_messages(RuntimeOrigin::root(), 3));

		// 验证：只清理了3条消息
		let mut cleaned = 0;
		for i in 0..5 {
			if Chat::get_message(i).is_none() {
				cleaned += 1;
			}
		}
		assert_eq!(cleaned, 3);
	});
}

#[test]
fn test_cleanup_old_messages_rejects_invalid_limit() {
	new_test_ext().execute_with(|| {
		// 验证：limit = 0 被拒绝（Root 调用以越过 origin 检查抵达 limit 校验）
		assert_noop!(
			Chat::cleanup_old_messages(RuntimeOrigin::root(), 0),
			Error::<Test>::InvalidCleanupLimit
		);

		// 验证：limit > 1000 被拒绝
		assert_noop!(
			Chat::cleanup_old_messages(RuntimeOrigin::root(), 1001),
			Error::<Test>::InvalidCleanupLimit
		);
	});
}

#[test]
fn test_cleanup_only_removes_fully_deleted_messages() {
	new_test_ext().execute_with(|| {
		// 发送3条消息
		for i in 0..3 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
		}

		// 只有发送方删除消息0
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 0));
		// 只有接收方删除消息1
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 1));
		// 双方都删除消息2
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), 2));
		assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), 2));

		// 推进区块，使消息过期
		System::set_block_number(1002);

		// 执行清理
		assert_ok!(Chat::cleanup_old_messages(RuntimeOrigin::root(), 100));

		// 验证：只有消息2被清理（双方都删除）
		assert!(Chat::get_message(0).is_some()); // 只有发送方删除
		assert!(Chat::get_message(1).is_some()); // 只有接收方删除
		assert!(Chat::get_message(2).is_none()); // 双方都删除
	});
}

#[test]
fn test_cleanup_respects_expiration_time() {
	new_test_ext().execute_with(|| {
		// 发送2条消息并双方都删除
		for i in 0..2 {
			assert_ok!(Chat::send_message(
				RuntimeOrigin::signed(ALICE),
				BOB,
				encrypted_cid(i),
				4,
				None
			));
			assert_ok!(Chat::delete_message(RuntimeOrigin::signed(ALICE), i as u64));
			assert_ok!(Chat::delete_message(RuntimeOrigin::signed(BOB), i as u64));
		}

		// 推进区块，但未超过过期时间（<1000）
		System::set_block_number(500);

		// 执行清理
		assert_ok!(Chat::cleanup_old_messages(RuntimeOrigin::root(), 100));

		// 验证：消息未被清理（因为未过期）
		assert!(Chat::get_message(0).is_some());
		assert!(Chat::get_message(1).is_some());

		// 推进区块，超过过期时间
		System::set_block_number(1002);

		// 再次执行清理
		assert_ok!(Chat::cleanup_old_messages(RuntimeOrigin::root(), 100));

		// 验证：消息被清理
		assert!(Chat::get_message(0).is_none());
		assert!(Chat::get_message(1).is_none());
	});
}

// ============================================================================
// ChatUserId 功能测试
// ============================================================================

#[test]
fn test_register_chat_user_works() {
	new_test_ext().execute_with(|| {
		// 注册聊天用户（不带昵称）
		assert_ok!(Chat::register_chat_user(
			RuntimeOrigin::signed(ALICE),
			None
		));

		// 验证：账户已有ChatUserId
		let chat_user_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();
		assert!(chat_user_id >= 10_000_000_000); // 11位数最小值
		assert!(chat_user_id <= 99_999_999_999); // 11位数最大值

		// 验证：反向映射存在
		let account = Chat::get_account_by_chat_user_id(chat_user_id).unwrap();
		assert_eq!(account, ALICE);

		// 验证：用户资料已创建
		let profile = Chat::get_chat_user_profile(chat_user_id).unwrap();
		assert_eq!(profile.nickname, None);
		assert_eq!(profile.status, crate::UserStatus::Online);
		assert_eq!(profile.privacy_settings.allow_stranger_messages, true);

		// 测试重复注册应该失败
		assert_noop!(
			Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None),
			Error::<Test>::ChatUserAlreadyExists
		);
	});
}

#[test]
fn test_register_chat_user_with_nickname() {
	new_test_ext().execute_with(|| {
		let nickname = b"Alice".to_vec();

		// 注册聊天用户（带昵称）
		assert_ok!(Chat::register_chat_user(
			RuntimeOrigin::signed(ALICE),
			Some(nickname.clone())
		));

		// 验证：昵称已设置
		let chat_user_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();
		let profile = Chat::get_chat_user_profile(chat_user_id).unwrap();
		assert_eq!(profile.nickname.unwrap().to_vec(), nickname);
	});
}

#[test]
fn test_chat_user_id_uniqueness() {
	new_test_ext().execute_with(|| {
		// 注册多个用户
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None));
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(BOB), None));
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(CHARLIE), None));

		// 获取所有ChatUserId
		let alice_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();
		let bob_id = Chat::get_chat_user_id_by_account(&BOB).unwrap();
		let charlie_id = Chat::get_chat_user_id_by_account(&CHARLIE).unwrap();

		// 验证：所有ID都不相同
		assert_ne!(alice_id, bob_id);
		assert_ne!(bob_id, charlie_id);
		assert_ne!(alice_id, charlie_id);

		// 验证：所有ID都在11位数范围内
		for id in [alice_id, bob_id, charlie_id] {
			assert!(id >= 10_000_000_000);
			assert!(id <= 99_999_999_999);
		}
	});
}

#[test]
fn test_update_chat_profile() {
	new_test_ext().execute_with(|| {
		// 注册用户
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None));
		let chat_user_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();

		// 更新资料
		let new_nickname = b"New Alice".to_vec();
		let new_signature = b"Hello World!".to_vec();
		let avatar_cid = b"QmTest123".to_vec();

		assert_ok!(Chat::update_chat_profile(
			RuntimeOrigin::signed(ALICE),
			Some(new_nickname.clone()),
			Some(avatar_cid.clone()),
			Some(new_signature.clone())
		));

		// 验证：资料已更新
		let profile = Chat::get_chat_user_profile(chat_user_id).unwrap();
		assert_eq!(profile.nickname.unwrap().to_vec(), new_nickname);
		assert_eq!(profile.avatar_cid.unwrap().to_vec(), avatar_cid);
		assert_eq!(profile.signature.unwrap().to_vec(), new_signature);
	});
}

#[test]
fn test_set_user_status() {
	new_test_ext().execute_with(|| {
		// 注册用户
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None));
		let chat_user_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();

		// 测试设置不同状态
		for (status_code, expected_status) in [
			(0, crate::UserStatus::Online),
			(1, crate::UserStatus::Offline),
			(2, crate::UserStatus::Busy),
			(3, crate::UserStatus::Away),
			(4, crate::UserStatus::Invisible),
		] {
			assert_ok!(Chat::set_user_status(
				RuntimeOrigin::signed(ALICE),
				status_code
			));

			let profile = Chat::get_chat_user_profile(chat_user_id).unwrap();
			assert_eq!(profile.status, expected_status);
		}

		// 测试无效状态代码
		assert_noop!(
			Chat::set_user_status(RuntimeOrigin::signed(ALICE), 99),
			Error::<Test>::InvalidUserStatus
		);
	});
}

#[test]
fn test_privacy_settings() {
	new_test_ext().execute_with(|| {
		// 注册用户
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None));
		let chat_user_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();

		// 更新隐私设置
		assert_ok!(Chat::update_privacy_settings(
			RuntimeOrigin::signed(ALICE),
			Some(false), // 不允许陌生人消息
			Some(false), // 不显示在线状态
			Some(false), // 不显示最后活跃时间
		));

		// 验证：隐私设置已更新
		let profile = Chat::get_chat_user_profile(chat_user_id).unwrap();
		assert_eq!(profile.privacy_settings.allow_stranger_messages, false);
		assert_eq!(profile.privacy_settings.show_online_status, false);
		assert_eq!(profile.privacy_settings.show_last_active, false);
	});
}

#[test]
fn test_send_message_with_chat_user_id() {
	new_test_ext().execute_with(|| {
		// 注册两个用户
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(ALICE), None));
		assert_ok!(Chat::register_chat_user(RuntimeOrigin::signed(BOB), None));

		let alice_chat_id = Chat::get_chat_user_id_by_account(&ALICE).unwrap();
		let bob_chat_id = Chat::get_chat_user_id_by_account(&BOB).unwrap();

		// 发送消息
		let cid = encrypted_cid(1);
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			cid.clone(),
			4,
			None
		));

		// 验证：消息包含ChatUserId信息
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.sender_chat_id, Some(alice_chat_id));
		assert_eq!(msg.receiver_chat_id, Some(bob_chat_id));
		assert_eq!(msg.sender, ALICE);
		assert_eq!(msg.receiver, BOB);
	});
}

// C1 权限单一化：陌生人消息限制已从 chat-core 移除，统一由 pallet-chat-permission
// 的 permission_level（FriendsOnly / Whitelist / Closed）表达；其拒绝路径由
// `test_send_message_denied_by_chat_permission` 覆盖。chat-core 的
// `allow_stranger_messages` 退化为纯展示标志（见 test_privacy_settings）。

#[test]
fn test_automatic_chat_user_creation() {
	new_test_ext().execute_with(|| {
		// 未注册的用户发送消息时应自动创建ChatUserId
		let cid = encrypted_cid(1);
		assert_ok!(Chat::send_message(
			RuntimeOrigin::signed(ALICE),
			BOB,
			cid,
			4,
			None
		));

		// 验证：ALICE和BOB都自动获得了ChatUserId
		assert!(Chat::get_chat_user_id_by_account(&ALICE).is_some());
		assert!(Chat::get_chat_user_id_by_account(&BOB).is_some());

		// 验证：消息包含ChatUserId
		let msg = Chat::get_message(0).unwrap();
		assert!(msg.sender_chat_id.is_some());
		assert!(msg.receiver_chat_id.is_some());
	});
}

// ============================================================================
// P1 阶段B：消息撤回（带时限，双方隐藏）
// P1 phase B: message recall (time-limited, hidden for both sides)
// ============================================================================

#[test]
fn recall_message_works_within_window() {
	new_test_ext().execute_with(|| {
		assert_ok!(Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, encrypted_cid(1), 4, None));
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_recalled, false);
		// 撤回前未读计数为 1 / unread is 1 before recall
		assert_eq!(Chat::get_unread_count(BOB, None), 1);

		assert_ok!(Chat::recall_message(RuntimeOrigin::signed(ALICE), 0));
		let msg = Chat::get_message(0).unwrap();
		assert_eq!(msg.is_recalled, true);
		// 撤回未读消息抵消未读计数 / unread offset to 0
		assert_eq!(Chat::get_unread_count(BOB, None), 0);
		System::assert_has_event(
			Event::MessageRecalled { msg_id: 0, session_id: msg.session_id }.into()
		);
	});
}

#[test]
fn recall_only_sender_allowed() {
	new_test_ext().execute_with(|| {
		assert_ok!(Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, encrypted_cid(1), 4, None));
		// 接收方不能撤回 / receiver cannot recall
		assert_noop!(
			Chat::recall_message(RuntimeOrigin::signed(BOB), 0),
			Error::<Test>::NotSender
		);
	});
}

#[test]
fn recall_idempotent_guard() {
	new_test_ext().execute_with(|| {
		assert_ok!(Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, encrypted_cid(1), 4, None));
		assert_ok!(Chat::recall_message(RuntimeOrigin::signed(ALICE), 0));
		assert_noop!(
			Chat::recall_message(RuntimeOrigin::signed(ALICE), 0),
			Error::<Test>::AlreadyRecalled
		);
	});
}

#[test]
fn recall_after_window_rejected() {
	new_test_ext().execute_with(|| {
		assert_ok!(Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, encrypted_cid(1), 4, None));
		// MessageRecallWindow = 50（mock）；推进到超过窗口 / advance beyond window
		run_to_block(60);
		assert_noop!(
			Chat::recall_message(RuntimeOrigin::signed(ALICE), 0),
			Error::<Test>::RecallWindowExpired
		);
	});
}

#[test]
fn recall_missing_message_fails() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Chat::recall_message(RuntimeOrigin::signed(ALICE), 999),
			Error::<Test>::MessageNotFound
		);
	});
}

#[test]
fn recall_read_message_keeps_unread_zero() {
	new_test_ext().execute_with(|| {
		assert_ok!(Chat::send_message(RuntimeOrigin::signed(ALICE), BOB, encrypted_cid(1), 4, None));
		// 接收方先读 / receiver reads first
		assert_ok!(Chat::mark_as_read(RuntimeOrigin::signed(BOB), 0));
		assert_eq!(Chat::get_unread_count(BOB, None), 0);
		// 撤回已读消息不应把未读计数减成负数（饱和）/ recalling a read msg keeps unread at 0
		assert_ok!(Chat::recall_message(RuntimeOrigin::signed(ALICE), 0));
		assert_eq!(Chat::get_unread_count(BOB, None), 0);
	});
}

// ============================================================================
// P1 阶段C：会话级免打扰 + 置顶
// P1 phase C: per-session mute (DND) + pin
// ============================================================================

/// 发送一条消息以创建会话，返回该会话的确定性 session_id。
/// Send one message to materialize a session and return its deterministic id.
fn make_session(a: u64, b: u64) -> sp_core::H256 {
	assert_ok!(Chat::send_message(RuntimeOrigin::signed(a), b, encrypted_cid(1), 4, None));
	Chat::get_session_id(&a, &b)
}

#[test]
fn session_mute_set_and_clear() {
	new_test_ext().execute_with(|| {
		let sid = make_session(ALICE, BOB);
		assert!(!Chat::is_session_muted(&ALICE, sid));

		assert_ok!(Chat::set_session_muted(RuntimeOrigin::signed(ALICE), sid, true));
		assert!(Chat::is_session_muted(&ALICE, sid));
		// 免打扰是每用户的：BOB 不受影响 / per-user: BOB unaffected
		assert!(!Chat::is_session_muted(&BOB, sid));
		System::assert_has_event(Event::SessionMuteSet { session_id: sid, user: ALICE, muted: true }.into());

		assert_ok!(Chat::set_session_muted(RuntimeOrigin::signed(ALICE), sid, false));
		assert!(!Chat::is_session_muted(&ALICE, sid));
	});
}

#[test]
fn session_mute_requires_participant() {
	new_test_ext().execute_with(|| {
		let sid = make_session(ALICE, BOB);
		// CHARLIE 非参与者 / non-participant
		assert_noop!(
			Chat::set_session_muted(RuntimeOrigin::signed(CHARLIE), sid, true),
			Error::<Test>::NotSessionParticipant
		);
		// 不存在的会话 / unknown session
		assert_noop!(
			Chat::set_session_muted(RuntimeOrigin::signed(ALICE), sp_core::H256::repeat_byte(9), true),
			Error::<Test>::SessionNotFound
		);
	});
}

#[test]
fn session_pin_set_and_clear() {
	new_test_ext().execute_with(|| {
		let sid = make_session(ALICE, BOB);
		assert!(!Chat::is_session_pinned(&ALICE, sid));
		assert_ok!(Chat::set_session_pinned(RuntimeOrigin::signed(ALICE), sid, true));
		assert!(Chat::is_session_pinned(&ALICE, sid));
		System::assert_has_event(Event::SessionPinSet { session_id: sid, user: ALICE, pinned: true }.into());
		assert_ok!(Chat::set_session_pinned(RuntimeOrigin::signed(ALICE), sid, false));
		assert!(!Chat::is_session_pinned(&ALICE, sid));
	});
}

#[test]
fn pinned_session_sorts_first() {
	new_test_ext().execute_with(|| {
		// ALICE 与 BOB、CHARLIE 各建一个会话；BOB 会话更早活跃。
		let sid_bob = make_session(ALICE, BOB);
		run_to_block(5);
		let sid_charlie = make_session(ALICE, CHARLIE);

		// 默认按最后活跃倒序：CHARLIE 在前。
		let list = Chat::list_sessions(ALICE);
		assert_eq!(list, vec![sid_charlie, sid_bob]);

		// 置顶较旧的 BOB 会话 → 它应排到最前。
		assert_ok!(Chat::set_session_pinned(RuntimeOrigin::signed(ALICE), sid_bob, true));
		let list = Chat::list_sessions(ALICE);
		assert_eq!(list, vec![sid_bob, sid_charlie]);
	});
}

