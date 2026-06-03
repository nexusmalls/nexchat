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

        /// EN: Current chat-capability revocation epoch of `who` (0 if never
        /// bumped). Off-chain relays/clients compare this against the epoch
        /// embedded in a chat capability token to reject stale capabilities.
        /// CN: `who` 当前的聊天能力撤销纪元（从未递增则为 0）。链下 relay/客户端
        /// 用它与能力令牌内嵌纪元比对，以拒绝过期能力。
        fn capability_epoch(who: AccountId) -> u32;

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
