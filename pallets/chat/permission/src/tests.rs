//! # 聊天权限系统单元测试
//!
//! 测试 pallet-chat-permission 的所有核心功能。

use crate::{
    mock::*, ChatPermissionLevel, Error, Event, PermissionResult, PrivacySettingsOf,
    SceneAuthorizationManager, SceneId, SceneType, ChatPermissionChecker,
};
use frame_support::{assert_noop, assert_ok};

/// 测试辅助：授予 `a`/`b` 间双向 Direct 场景授权，使两者可互发消息。
/// （链上好友图谱已删除，社交联系人改由链下能力令牌承载；测试用场景授权模拟"可聊天"。）
/// Test helper: grant a bidirectional Direct scene authorization so `a` and `b`
/// may message each other. (The on-chain friend graph was removed; social
/// contacts live off-chain as capability tokens, so tests use a scene auth to
/// model "allowed to chat".)
fn allow_pair(a: u64, b: u64) {
    assert_ok!(
        <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_bidirectional_scene_authorization(
            *b"directms",
            &a,
            &b,
            SceneType::Direct,
            SceneId::None,
            None,
            vec![],
        )
    );
}

// ==================== 隐私设置测试 ====================

mod privacy_settings {
    use super::*;

    /// 测试：设置权限级别
    #[test]
    fn set_permission_level_works() {
        new_test_ext().execute_with(|| {
            // 默认是 FriendsOnly
            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert_eq!(settings.permission_level, ChatPermissionLevel::FriendsOnly);

            // 设置为 Open
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Open
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert_eq!(settings.permission_level, ChatPermissionLevel::Open);

            // 验证事件
            System::assert_last_event(Event::PrivacySettingsUpdated { who: ALICE }.into());
        });
    }

    /// 测试：设置拒绝的场景类型
    #[test]
    fn set_rejected_scene_types_works() {
        new_test_ext().execute_with(|| {
            use frame_support::BoundedVec;

            let rejected: BoundedVec<SceneType, frame_support::traits::ConstU32<10>> =
                vec![SceneType::MarketMaker].try_into().unwrap();

            assert_ok!(ChatPermission::set_rejected_scene_types(
                RuntimeOrigin::signed(ALICE),
                rejected.clone()
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert!(settings.rejected_scene_types.contains(&SceneType::MarketMaker));
        });
    }
}

// ==================== 黑名单测试 ====================

mod block_list {
    use super::*;

    /// 测试：添加用户到黑名单
    #[test]
    fn block_user_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::block_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert!(settings.block_list.contains(&BOB));

            System::assert_last_event(
                Event::UserBlocked {
                    blocker: ALICE,
                    blocked: BOB,
                }
                .into(),
            );
        });
    }

    /// 测试：不能屏蔽自己
    #[test]
    fn block_self_fails() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                ChatPermission::block_user(RuntimeOrigin::signed(ALICE), ALICE),
                Error::<Test>::CannotAddSelf
            );
        });
    }

    /// 测试：重复屏蔽失败
    #[test]
    fn block_user_twice_fails() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::block_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));
            assert_noop!(
                ChatPermission::block_user(RuntimeOrigin::signed(ALICE), BOB),
                Error::<Test>::AlreadyBlocked
            );
        });
    }

    /// 测试：从黑名单移除用户
    #[test]
    fn unblock_user_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::block_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));
            assert_ok!(ChatPermission::unblock_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert!(!settings.block_list.contains(&BOB));

            System::assert_last_event(
                Event::UserUnblocked {
                    unblocker: ALICE,
                    unblocked: BOB,
                }
                .into(),
            );
        });
    }

    /// 测试：移除不在黑名单中的用户失败
    #[test]
    fn unblock_not_blocked_user_fails() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                ChatPermission::unblock_user(RuntimeOrigin::signed(ALICE), BOB),
                Error::<Test>::NotInBlockList
            );
        });
    }
}

// ==================== 聊天能力撤销纪元测试 ====================
// Chat-capability revocation epoch tests.

mod capability_epoch {
    use super::*;

    /// 测试：纪元默认 0，每次 bump 递增并发出事件。
    /// Test: epoch defaults to 0, each bump increments and emits an event.
    #[test]
    fn bump_capability_epoch_works() {
        new_test_ext().execute_with(|| {
            assert_eq!(ChatPermission::capability_epoch(ALICE), 0);

            assert_ok!(ChatPermission::bump_capability_epoch(RuntimeOrigin::signed(ALICE)));
            assert_eq!(ChatPermission::capability_epoch(ALICE), 1);
            System::assert_last_event(
                Event::CapabilityEpochBumped { who: ALICE, new_epoch: 1 }.into(),
            );

            assert_ok!(ChatPermission::bump_capability_epoch(RuntimeOrigin::signed(ALICE)));
            assert_eq!(ChatPermission::capability_epoch(ALICE), 2);

            // 各账户独立 / epochs are per-account independent.
            assert_eq!(ChatPermission::capability_epoch(BOB), 0);
        });
    }
}

// ==================== 白名单测试 ====================

mod whitelist {
    use super::*;

    /// 测试：添加到白名单
    #[test]
    fn add_to_whitelist_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::add_to_whitelist(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert!(settings.whitelist.contains(&BOB));

            System::assert_last_event(
                Event::UserAddedToWhitelist {
                    owner: ALICE,
                    user: BOB,
                }
                .into(),
            );
        });
    }

    /// 测试：从白名单移除
    #[test]
    fn remove_from_whitelist_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::add_to_whitelist(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));
            assert_ok!(ChatPermission::remove_from_whitelist(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            let settings = PrivacySettingsOf::<Test>::get(ALICE);
            assert!(!settings.whitelist.contains(&BOB));
        });
    }
}

// ==================== 场景授权测试 ====================

mod scene_authorization {
    use super::*;

    /// 测试：授予场景授权
    #[test]
    fn grant_scene_authorization_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    Some(100), // 100 区块后过期
                    b"Order#12345".to_vec(),
                )
            );

            // 验证授权已存储（使用排序后的 key）
            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            assert_eq!(auths.len(), 1);
            assert_eq!(auths[0].scene_type, SceneType::Order);
            assert_eq!(auths[0].scene_id, SceneId::Numeric(12345));
            assert_eq!(auths[0].source_pallet, OTC_ORDER_SOURCE);
        });
    }

    /// 测试：授予双向场景授权
    #[test]
    fn grant_bidirectional_authorization_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_bidirectional_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    None, // 永不过期
                    b"Order#12345".to_vec(),
                )
            );

            // 无论顺序如何，都能查到
            let auths1 = ChatPermission::scene_authorizations(ALICE, BOB);
            let auths2 = ChatPermission::scene_authorizations(BOB, ALICE);

            // 由于排序存储，两个查询返回相同结果
            assert!(!auths1.is_empty() || !auths2.is_empty());
        });
    }

    /// 测试：撤销场景授权
    #[test]
    fn revoke_scene_authorization_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    None,
                    b"Order#12345".to_vec(),
                )
            );

            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::revoke_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                )
            );

            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            assert!(auths.is_empty());
        });
    }

    /// 测试：延长场景授权有效期
    #[test]
    fn extend_scene_authorization_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    Some(100),
                    b"Order#12345".to_vec(),
                )
            );

            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            let original_expires = auths[0].expires_at;

            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::extend_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    50, // 延长 50 区块
                )
            );

            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            assert!(auths[0].expires_at > original_expires);
        });
    }

    /// 测试：场景授权数量限制
    #[test]
    fn too_many_scenes_fails() {
        new_test_ext().execute_with(|| {
            // 添加到上限（测试配置为 5）
            for i in 0..5u64 {
                assert_ok!(
                    <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                        OTC_ORDER_SOURCE,
                        &ALICE,
                        &BOB,
                        SceneType::Order,
                        SceneId::Numeric(i),
                        None,
                        vec![],
                    )
                );
            }

            // 第 6 个应该失败
            assert_noop!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(100),
                    None,
                    vec![],
                ),
                Error::<Test>::TooManyScenes
            );
        });
    }

    /// 测试：检查是否有任何有效场景授权
    #[test]
    fn has_any_valid_scene_authorization_works() {
        new_test_ext().execute_with(|| {
            assert!(!<ChatPermission as SceneAuthorizationManager<u64, u64>>::has_any_valid_scene_authorization(&ALICE, &BOB));

            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    Some(100),
                    vec![],
                )
            );

            assert!(<ChatPermission as SceneAuthorizationManager<u64, u64>>::has_any_valid_scene_authorization(&ALICE, &BOB));
        });
    }

    /// 测试：场景授权过期
    #[test]
    fn scene_authorization_expires() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    Some(10), // 10 区块后过期
                    vec![],
                )
            );

            // 当前区块 1，授权有效
            assert!(<ChatPermission as SceneAuthorizationManager<u64, u64>>::has_any_valid_scene_authorization(&ALICE, &BOB));

            // 推进到区块 12，授权过期
            run_to_block(12);
            assert!(!<ChatPermission as SceneAuthorizationManager<u64, u64>>::has_any_valid_scene_authorization(&ALICE, &BOB));
        });
    }
}

// ==================== 权限检查测试 ====================

mod permission_check {
    use super::*;

    /// 测试：黑名单最高优先级（先于场景授权）
    #[test]
    fn blocked_user_cannot_send() {
        new_test_ext().execute_with(|| {
            // 先授予可聊天的场景授权
            allow_pair(ALICE, BOB);

            // 再把 BOB 加入黑名单
            assert_ok!(ChatPermission::block_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            // BOB 向 ALICE 发消息应该被拒绝（黑名单先于场景授权）
            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::DeniedBlocked);
        });
    }

    /// 测试：场景授权优先于 Closed 设置
    #[test]
    fn scene_auth_overrides_closed() {
        new_test_ext().execute_with(|| {
            // ALICE 设置为 Closed
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Closed
            ));

            // 授予双向 Direct 场景授权
            allow_pair(ALICE, BOB);

            // BOB 可以发消息（场景授权优先于 Closed 设置）
            match ChatPermission::check_permission(&BOB, &ALICE) {
                PermissionResult::AllowedByScene(scenes) => {
                    assert!(scenes.contains(&SceneType::Direct));
                }
                other => panic!("Expected AllowedByScene, got {:?}", other),
            }
        });
    }

    /// 测试：场景授权允许通过
    #[test]
    fn scene_authorization_allows_sending() {
        new_test_ext().execute_with(|| {
            // ALICE 设置为 FriendsOnly（默认）
            assert_eq!(
                PrivacySettingsOf::<Test>::get(ALICE).permission_level,
                ChatPermissionLevel::FriendsOnly
            );

            // 授予订单场景授权
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    None,
                    vec![],
                )
            );

            // BOB 可以发消息
            let result = ChatPermission::check_permission(&BOB, &ALICE);
            match result {
                PermissionResult::AllowedByScene(scenes) => {
                    assert!(scenes.contains(&SceneType::Order));
                }
                _ => panic!("Expected AllowedByScene"),
            }
        });
    }

    /// 测试：被拒绝的场景类型
    #[test]
    fn rejected_scene_type_blocks_sending() {
        new_test_ext().execute_with(|| {
            use frame_support::BoundedVec;

            // 授予 MarketMaker 场景授权
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    MAKER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::MarketMaker,
                    SceneId::Numeric(1),
                    None,
                    vec![],
                )
            );

            // ALICE 拒绝 MarketMaker 场景
            let rejected: BoundedVec<SceneType, frame_support::traits::ConstU32<10>> =
                vec![SceneType::MarketMaker].try_into().unwrap();
            assert_ok!(ChatPermission::set_rejected_scene_types(
                RuntimeOrigin::signed(ALICE),
                rejected
            ));

            // BOB 无法通过 MarketMaker 场景发消息
            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::DeniedRequiresFriend);
        });
    }

    /// 测试：Open 模式允许所有人
    #[test]
    fn open_mode_allows_all() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Open
            ));

            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::Allowed);
        });
    }

    /// 测试：Whitelist 模式只允许白名单用户
    #[test]
    fn whitelist_mode_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Whitelist
            ));

            // BOB 不在白名单中，被拒绝
            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::DeniedNotInWhitelist);

            // 将 BOB 添加到白名单
            assert_ok!(ChatPermission::add_to_whitelist(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));

            // BOB 现在可以发消息
            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::Allowed);
        });
    }

    /// 测试：Closed 模式拒绝所有人
    #[test]
    fn closed_mode_denies_all() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Closed
            ));

            let result = ChatPermission::check_permission(&BOB, &ALICE);
            assert_eq!(result, PermissionResult::DeniedClosed);
        });
    }

    /// 测试：ChatPermissionChecker trait
    #[test]
    fn chat_permission_checker_trait_works() {
        new_test_ext().execute_with(|| {
            // 默认 FriendsOnly，非好友不能发消息
            assert!(!ChatPermission::can_send_message(&BOB, &ALICE));

            // 设置为 Open
            assert_ok!(ChatPermission::set_permission_level(
                RuntimeOrigin::signed(ALICE),
                ChatPermissionLevel::Open
            ));

            assert!(ChatPermission::can_send_message(&BOB, &ALICE));
        });
    }
}

// ==================== 辅助方法测试 ====================

mod helper_methods {
    use super::*;

    /// 测试：sorted_pair 保证顺序一致性
    #[test]
    fn sorted_pair_works() {
        new_test_ext().execute_with(|| {
            let (u1, u2) = ChatPermission::sorted_pair(&ALICE, &BOB);
            let (u3, u4) = ChatPermission::sorted_pair(&BOB, &ALICE);

            assert_eq!(u1, u3);
            assert_eq!(u2, u4);
            assert!(u1 < u2);
        });
    }

    /// 测试：get_active_scenes
    #[test]
    fn get_active_scenes_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(12345),
                    Some(100),
                    b"Order#12345".to_vec(),
                )
            );

            let scenes = ChatPermission::get_active_scenes(&ALICE, &BOB);
            assert_eq!(scenes.len(), 1);
            assert_eq!(scenes[0].scene_type, SceneType::Order);
            assert!(!scenes[0].is_expired);
        });
    }

    /// 测试：get_privacy_summary
    #[test]
    fn get_privacy_summary_works() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::block_user(
                RuntimeOrigin::signed(ALICE),
                BOB
            ));
            assert_ok!(ChatPermission::add_to_whitelist(
                RuntimeOrigin::signed(ALICE),
                CHARLIE
            ));

            let summary = ChatPermission::get_privacy_summary(&ALICE);
            assert_eq!(summary.permission_level, ChatPermissionLevel::FriendsOnly);
            assert_eq!(summary.block_list_count, 1);
            assert_eq!(summary.whitelist_count, 1);
        });
    }

    /// 测试：cleanup_expired_scenes
    #[test]
    fn cleanup_expired_scenes_works() {
        new_test_ext().execute_with(|| {
            // 添加两个授权，一个即将过期，一个永不过期
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    OTC_ORDER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::Order,
                    SceneId::Numeric(1),
                    Some(5), // 5 区块后过期
                    vec![],
                )
            );
            assert_ok!(
                <ChatPermission as SceneAuthorizationManager<u64, u64>>::grant_scene_authorization(
                    MAKER_SOURCE,
                    &ALICE,
                    &BOB,
                    SceneType::MarketMaker,
                    SceneId::Numeric(2),
                    None, // 永不过期
                    vec![],
                )
            );

            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            assert_eq!(auths.len(), 2);

            // 推进到区块 10，Order 授权过期
            run_to_block(10);

            // 清理过期授权
            ChatPermission::cleanup_expired_scenes(&ALICE, &BOB);

            let auths = ChatPermission::scene_authorizations(ALICE, BOB);
            assert_eq!(auths.len(), 1);
            assert_eq!(auths[0].scene_type, SceneType::MarketMaker);
        });
    }
}

// ==================== 平台合规：禁言账号 / 举报存证 ====================
// Platform compliance: account mute / reports.
mod compliance {
    use super::*;
    use crate::{PermissionResult, ReportTarget};

    /// 治理禁言：被禁言账户作为发送方一律被拒，解除后恢复。
    /// Governance mute: a muted sender is denied; unmute restores.
    #[test]
    fn governance_mute_blocks_sender() {
        new_test_ext().execute_with(|| {
            allow_pair(ALICE, BOB);
            // 有场景授权则可发 / authorized pair can send
            assert!(ChatPermission::can_send_message(&ALICE, &BOB));

            // 非治理来源不可禁言 / non-governance origin rejected
            assert_noop!(
                ChatPermission::force_mute_account(RuntimeOrigin::signed(BOB), ALICE, None),
                sp_runtime::DispatchError::BadOrigin
            );

            // Root 无限期禁言 / Root mutes indefinitely
            assert_ok!(ChatPermission::force_mute_account(RuntimeOrigin::root(), ALICE, None));
            assert!(ChatPermission::is_account_muted(&ALICE));
            assert_eq!(
                ChatPermission::check_permission(&ALICE, &BOB),
                PermissionResult::DeniedSenderMuted
            );
            assert!(!ChatPermission::can_send_message(&ALICE, &BOB));
            // 接收方不受影响 / the receiver side is unaffected
            assert!(ChatPermission::can_send_message(&BOB, &ALICE));

            // 解除后恢复 / unmute restores
            assert_ok!(ChatPermission::force_unmute_account(RuntimeOrigin::root(), ALICE));
            assert!(!ChatPermission::is_account_muted(&ALICE));
            assert!(ChatPermission::can_send_message(&ALICE, &BOB));
        });
    }

    /// 定时禁言到期后自动失效。
    /// A timed mute lapses after its `until` block.
    #[test]
    fn timed_mute_expires() {
        new_test_ext().execute_with(|| {
            assert_ok!(ChatPermission::force_mute_account(RuntimeOrigin::root(), ALICE, Some(10)));
            assert!(ChatPermission::is_account_muted(&ALICE));
            run_to_block(10);
            // until 为不含上界 / `until` is exclusive
            assert!(!ChatPermission::is_account_muted(&ALICE));
        });
    }

    /// 举报：存证、冷却、全局上限、治理处理。
    /// Reports: record, cooldown, global cap, governance resolution.
    #[test]
    fn report_lifecycle_works() {
        new_test_ext().execute_with(|| {
            // 过长 CID 被拒 / over-length CID rejected (mock MaxReportCidLen = 64)
            assert_noop!(
                ChatPermission::report(
                    RuntimeOrigin::signed(ALICE),
                    ReportTarget::Account(BOB),
                    vec![b'x'; 65],
                ),
                Error::<Test>::MetadataTooLong
            );

            // 正常举报 / a normal report
            assert_ok!(ChatPermission::report(
                RuntimeOrigin::signed(ALICE),
                ReportTarget::Account(BOB),
                b"bafyCID".to_vec(),
            ));
            assert_eq!(crate::OpenReportCount::<Test>::get(), 1);
            assert!(crate::Reports::<Test>::contains_key(0));

            // 同一举报人冷却中（mock ReportCooldown = 5）/ same reporter in cooldown
            assert_noop!(
                ChatPermission::report(
                    RuntimeOrigin::signed(ALICE),
                    ReportTarget::Group(7),
                    b"cid2".to_vec(),
                ),
                Error::<Test>::ReportCooldown
            );

            // 治理处理后移除、计数回落 / governance resolves → removed, count drops
            assert_noop!(
                ChatPermission::resolve_report(RuntimeOrigin::signed(BOB), 0, true),
                sp_runtime::DispatchError::BadOrigin
            );
            assert_ok!(ChatPermission::resolve_report(RuntimeOrigin::root(), 0, true));
            assert_eq!(crate::OpenReportCount::<Test>::get(), 0);
            assert!(!crate::Reports::<Test>::contains_key(0));
            assert_noop!(
                ChatPermission::resolve_report(RuntimeOrigin::root(), 0, true),
                Error::<Test>::ReportNotFound
            );
        });
    }

    /// 全局未处理上限：不同举报人填满后第 N+1 个被拒。
    /// Global open-report cap: rejected once filled by distinct reporters.
    #[test]
    fn open_report_cap_enforced() {
        new_test_ext().execute_with(|| {
            // mock MaxOpenReports = 3；不同举报人无冷却干扰
            assert_ok!(ChatPermission::report(
                RuntimeOrigin::signed(10),
                ReportTarget::Account(BOB),
                b"a".to_vec(),
            ));
            assert_ok!(ChatPermission::report(
                RuntimeOrigin::signed(11),
                ReportTarget::Account(BOB),
                b"b".to_vec(),
            ));
            assert_ok!(ChatPermission::report(
                RuntimeOrigin::signed(12),
                ReportTarget::Account(BOB),
                b"c".to_vec(),
            ));
            assert_noop!(
                ChatPermission::report(
                    RuntimeOrigin::signed(13),
                    ReportTarget::Account(BOB),
                    b"d".to_vec(),
                ),
                Error::<Test>::TooManyOpenReports
            );
        });
    }
}
