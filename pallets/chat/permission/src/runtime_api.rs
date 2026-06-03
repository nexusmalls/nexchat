//! Runtime API 定义
//!
//! 本模块定义了聊天权限系统的 Runtime API，供前端和 RPC 调用。

use crate::types::{PermissionResult, PrivacySettingsSummary, SceneAuthorizationInfo};
use codec::Codec;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
    /// 聊天权限系统 Runtime API
    ///
    /// 提供聊天权限检查和场景授权查询功能。
    pub trait ChatPermissionApi<AccountId>
    where
        AccountId: Codec,
    {
        /// 检查聊天权限
        ///
        /// 检查 sender 是否可以向 receiver 发送消息。
        ///
        /// # 参数
        /// - `sender`: 消息发送者
        /// - `receiver`: 消息接收者
        ///
        /// # 返回
        /// 权限检查结果，包含允许或拒绝原因
        fn check_chat_permission(
            sender: AccountId,
            receiver: AccountId,
        ) -> PermissionResult;

        /// 获取两用户间所有有效场景
        ///
        /// 返回两个用户之间所有的场景授权（包括已过期的）。
        /// 前端可以根据 `is_expired` 字段过滤。
        ///
        /// # 参数
        /// - `user1`: 第一个用户
        /// - `user2`: 第二个用户
        ///
        /// # 返回
        /// 场景授权信息列表
        fn get_active_scenes(
            user1: AccountId,
            user2: AccountId,
        ) -> Vec<SceneAuthorizationInfo>;

        /// 检查是否是好友
        ///
        /// # 参数
        /// - `user1`: 第一个用户
        /// - `user2`: 第二个用户
        ///
        /// # 返回
        /// 如果是好友返回 true
        fn is_friend(user1: AccountId, user2: AccountId) -> bool;

        /// 列出某用户的所有好友
        ///
        /// EN: List all friends of `who`. CN: 列出 `who` 的所有好友。
        fn list_friends(who: AccountId) -> Vec<AccountId>;

        /// 列出待该用户处理（同意/拒绝）的收件好友申请发起方
        ///
        /// EN: List requesters whose friend request awaits `who`'s consent.
        /// CN: 列出待 `who` 处理的好友申请发起方。
        fn list_incoming_friend_requests(who: AccountId) -> Vec<AccountId>;

        /// EN: Same as `list_incoming_friend_requests`, but each entry also
        /// carries the optional greeting ("verification message") bytes attached
        /// to the request (empty when none). CN: 同 `list_incoming_friend_requests`，
        /// 但每项附带申请的可选附言（验证消息）字节（无则为空）。
        fn list_incoming_friend_requests_detailed(
            who: AccountId,
        ) -> Vec<(AccountId, Vec<u8>)>;

        /// EN: Read `owner`'s private `(remark, group)` labels for `friend`
        /// (empty bytes when unset). CN: 读取 `owner` 对 `friend` 的私有
        /// `(备注, 分组)` 标签（未设置则为空字节）。
        fn get_friend_meta(owner: AccountId, friend: AccountId) -> (Vec<u8>, Vec<u8>);

        /// EN: Whether `who` is currently platform-muted by governance.
        /// CN: `who` 当前是否被治理平台级禁言。
        fn is_account_muted(who: AccountId) -> bool;

        /// 获取隐私设置摘要
        ///
        /// 返回用户的隐私设置概要信息。
        ///
        /// # 参数
        /// - `user`: 要查询的用户
        ///
        /// # 返回
        /// 隐私设置摘要
        fn get_privacy_settings_summary(user: AccountId) -> PrivacySettingsSummary;
    }
}
