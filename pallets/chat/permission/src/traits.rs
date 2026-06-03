//! 聊天权限系统 Trait 定义
//!
//! 本模块定义了场景授权管理的核心 trait，业务 pallet 通过实现
//! 或调用这些 trait 来管理聊天场景授权。

use crate::types::{SceneAuthorization, SceneId, SceneType};
use frame_support::dispatch::DispatchResult;
use sp_std::vec::Vec;

/// 场景授权管理接口
///
/// 业务 pallet 通过此 trait 管理场景授权。
/// 提供授予、撤销、延期和查询场景授权的功能。
///
/// # 类型参数
/// - `AccountId`: 账户标识类型
/// - `BlockNumber`: 区块号类型
///
/// # 使用示例
/// ```ignore
/// // 在业务 pallet 的 Config 中声明
/// type ChatPermission: SceneAuthorizationManager<Self::AccountId, BlockNumberFor<Self>>;
///
/// // 在业务逻辑中调用
/// T::ChatPermission::grant_bidirectional_scene_authorization(
///     *b"otc_ordr",
///     &buyer,
///     &seller,
///     SceneType::Order,
///     SceneId::Numeric(order_id),
///     Some(duration),
///     metadata,
/// )?;
/// ```
pub trait SceneAuthorizationManager<AccountId, BlockNumber> {
    /// 授予场景授权（单向）
    ///
    /// 允许 `from` 用户向 `to` 用户发起聊天。
    /// 这是单向授权，如需双向聊天请使用 `grant_bidirectional_scene_authorization`。
    ///
    /// # 参数
    /// - `source`: 授权来源 PalletId（8字节标识符）
    /// - `from`: 可以发起聊天的用户
    /// - `to`: 可以被联系的用户
    /// - `scene_type`: 场景类型
    /// - `scene_id`: 场景标识（如订单ID）
    /// - `duration`: 有效期（区块数），None 表示永不过期
    /// - `metadata`: 元数据（用于前端显示，如订单金额）
    ///
    /// # 错误
    /// - 场景授权数量超过上限时返回错误
    fn grant_scene_authorization(
        source: [u8; 8],
        from: &AccountId,
        to: &AccountId,
        scene_type: SceneType,
        scene_id: SceneId,
        duration: Option<BlockNumber>,
        metadata: Vec<u8>,
    ) -> DispatchResult;

    /// 授予双向场景授权
    ///
    /// 允许两个用户相互发起聊天。
    /// 内部实现会将用户按字典序排列存储，保证双向一致性。
    ///
    /// # 参数
    /// - `source`: 授权来源 PalletId
    /// - `user1`: 第一个用户
    /// - `user2`: 第二个用户
    /// - `scene_type`: 场景类型
    /// - `scene_id`: 场景标识
    /// - `duration`: 有效期（区块数）
    /// - `metadata`: 元数据
    fn grant_bidirectional_scene_authorization(
        source: [u8; 8],
        user1: &AccountId,
        user2: &AccountId,
        scene_type: SceneType,
        scene_id: SceneId,
        duration: Option<BlockNumber>,
        metadata: Vec<u8>,
    ) -> DispatchResult;

    /// 撤销特定场景授权
    ///
    /// 移除指定来源、场景类型和场景ID的授权。
    /// 只有授权来源 pallet 才应该撤销自己授予的授权。
    ///
    /// # 参数
    /// - `source`: 授权来源 PalletId
    /// - `from`: 发起聊天的用户
    /// - `to`: 被联系的用户
    /// - `scene_type`: 场景类型
    /// - `scene_id`: 场景标识
    ///
    /// # 错误
    /// - 找不到对应授权时返回错误
    fn revoke_scene_authorization(
        source: [u8; 8],
        from: &AccountId,
        to: &AccountId,
        scene_type: SceneType,
        scene_id: SceneId,
    ) -> DispatchResult;

    /// 撤销某来源的所有场景授权
    ///
    /// 移除指定来源在两个用户之间的所有授权。
    /// 适用于业务场景完全结束时的批量清理。
    ///
    /// # 参数
    /// - `source`: 授权来源 PalletId
    /// - `user1`: 第一个用户
    /// - `user2`: 第二个用户
    fn revoke_all_by_source(
        source: [u8; 8],
        user1: &AccountId,
        user2: &AccountId,
    ) -> DispatchResult;

    /// 延长场景授权有效期
    ///
    /// 为现有授权延长有效期。如果授权已过期，
    /// 则从当前区块开始计算新的有效期。
    ///
    /// # 参数
    /// - `source`: 授权来源 PalletId
    /// - `from`: 发起聊天的用户
    /// - `to`: 被联系的用户
    /// - `scene_type`: 场景类型
    /// - `scene_id`: 场景标识
    /// - `additional_duration`: 额外延长的区块数
    ///
    /// # 错误
    /// - 找不到对应授权时返回错误
    fn extend_scene_authorization(
        source: [u8; 8],
        from: &AccountId,
        to: &AccountId,
        scene_type: SceneType,
        scene_id: SceneId,
        additional_duration: BlockNumber,
    ) -> DispatchResult;

    /// 检查是否有任何有效的场景授权
    ///
    /// 快速检查两个用户之间是否存在至少一个未过期的场景授权。
    /// 用于权限判断的快速路径。
    ///
    /// # 参数
    /// - `from`: 发起聊天的用户
    /// - `to`: 被联系的用户
    ///
    /// # 返回
    /// 如果存在有效授权返回 true，否则返回 false
    fn has_any_valid_scene_authorization(from: &AccountId, to: &AccountId) -> bool;

    /// 获取所有有效的场景授权
    ///
    /// 返回两个用户之间所有未过期的场景授权列表。
    /// 用于前端展示和详细权限查询。
    ///
    /// # 参数
    /// - `user1`: 第一个用户
    /// - `user2`: 第二个用户
    ///
    /// # 返回
    /// 有效场景授权的列表
    fn get_valid_scene_authorizations(
        user1: &AccountId,
        user2: &AccountId,
    ) -> Vec<SceneAuthorization<BlockNumber>>;
}

/// 聊天权限检查接口
///
/// 提供聊天权限检查功能，用于在发送消息前验证权限。
pub trait ChatPermissionChecker<AccountId> {
    /// 检查用户是否可以向另一用户发送消息
    ///
    /// # 参数
    /// - `sender`: 消息发送者
    /// - `receiver`: 消息接收者
    ///
    /// # 返回
    /// 如果允许发送返回 true，否则返回 false
    fn can_send_message(sender: &AccountId, receiver: &AccountId) -> bool;
}

/// 好友关系管理接口
///
/// 提供好友关系的查询功能。
pub trait FriendshipChecker<AccountId> {
    /// 检查两个用户是否是好友
    ///
    /// # 参数
    /// - `user1`: 第一个用户
    /// - `user2`: 第二个用户
    ///
    /// # 返回
    /// 如果是好友返回 true，否则返回 false
    fn is_friend(user1: &AccountId, user2: &AccountId) -> bool;
}
