//! # 聊天系统共享Trait定义
//!
//! 本模块定义了聊天子模块间共享的trait接口

use sp_std::vec::Vec;
use frame_support::dispatch::DispatchResult;
use crate::types::ChatUserId;

/// 函数级详细中文注释：聊天权限检查trait
///
/// 由 pallet-chat-permission 实现，供其他聊天模块调用
/// 用于在发送消息前验证权限
pub trait ChatPermissionCheck<AccountId> {
    /// 函数级中文注释：检查用户是否可以发送消息给目标
    ///
    /// # 参数
    /// - `sender`: 消息发送者
    /// - `receiver`: 消息接收者
    ///
    /// # 返回
    /// - `true`: 允许发送
    /// - `false`: 不允许发送
    fn can_send_message(sender: &AccountId, receiver: &AccountId) -> bool;

    /// 函数级中文注释：检查用户是否被拉黑
    ///
    /// # 参数
    /// - `blocker`: 可能拉黑的用户
    /// - `blocked`: 可能被拉黑的用户
    ///
    /// # 返回
    /// - `true`: 已被拉黑
    /// - `false`: 未被拉黑
    fn is_blocked(blocker: &AccountId, blocked: &AccountId) -> bool;
}

/// 函数级详细中文注释：好友关系检查trait
///
/// 由 pallet-chat-permission 实现
pub trait FriendshipCheck<AccountId> {
    /// 函数级中文注释：检查两个用户是否是好友
    ///
    /// # 参数
    /// - `user1`: 第一个用户
    /// - `user2`: 第二个用户
    ///
    /// # 返回
    /// - `true`: 是好友
    /// - `false`: 不是好友
    fn is_friend(user1: &AccountId, user2: &AccountId) -> bool;
}

/// 函数级详细中文注释：ChatUserId提供者trait
///
/// 由 pallet-chat-core 实现，供其他模块获取用户的ChatUserId
pub trait ChatUserIdProvider<AccountId> {
    /// 函数级中文注释：获取账户对应的ChatUserId
    ///
    /// # 参数
    /// - `account`: 账户地址
    ///
    /// # 返回
    /// - `Some(ChatUserId)`: 用户已注册
    /// - `None`: 用户未注册
    fn get_chat_user_id(account: &AccountId) -> Option<ChatUserId>;

    /// 函数级中文注释：获取ChatUserId对应的账户
    ///
    /// # 参数
    /// - `chat_user_id`: 聊天用户ID
    ///
    /// # 返回
    /// - `Some(AccountId)`: 找到对应账户
    /// - `None`: 无对应账户
    fn get_account(chat_user_id: ChatUserId) -> Option<AccountId>;

    /// 函数级中文注释：获取或创建ChatUserId
    ///
    /// 如果账户没有ChatUserId，则创建一个新的
    ///
    /// # 参数
    /// - `account`: 账户地址
    ///
    /// # 返回
    /// - `Ok(ChatUserId)`: 获取或创建成功
    /// - `Err`: 创建失败
    fn get_or_create_chat_user_id(account: &AccountId) -> Result<ChatUserId, &'static str>;
}

/// 函数级详细中文注释：IPFS内容存储trait
///
/// 用于验证和管理IPFS内容
pub trait IpfsContentValidator {
    /// 函数级中文注释：验证CID格式
    ///
    /// # 参数
    /// - `cid`: IPFS CID字节数组
    ///
    /// # 返回
    /// - `true`: CID格式有效
    /// - `false`: CID格式无效
    fn validate_cid(cid: &[u8]) -> bool;

    /// 函数级中文注释：检查CID是否已加密
    ///
    /// # 参数
    /// - `cid`: IPFS CID字节数组
    ///
    /// # 返回
    /// - `true`: 已加密
    /// - `false`: 未加密
    fn is_encrypted(cid: &[u8]) -> bool;
}

/// 函数级详细中文注释：频率限制检查trait
///
/// 用于防止垃圾消息
pub trait RateLimitCheck<AccountId, BlockNumber> {
    /// 函数级中文注释：检查是否超过频率限制
    ///
    /// # 参数
    /// - `sender`: 发送者账户
    ///
    /// # 返回
    /// - `Ok(())`: 未超过限制
    /// - `Err`: 超过限制
    fn check_rate_limit(sender: &AccountId) -> DispatchResult;

    /// 函数级中文注释：记录发送行为
    ///
    /// # 参数
    /// - `sender`: 发送者账户
    /// - `block`: 当前区块号
    fn record_send(sender: &AccountId, block: BlockNumber);
}

/// 函数级详细中文注释：群组成员检查trait
///
/// 由 pallet-chat-group 实现
pub trait GroupMemberCheck<AccountId> {
    /// 函数级中文注释：检查用户是否是群组成员
    ///
    /// # 参数
    /// - `group_id`: 群组ID
    /// - `user`: 用户账户
    ///
    /// # 返回
    /// - `true`: 是成员
    /// - `false`: 不是成员
    fn is_group_member(group_id: u64, user: &AccountId) -> bool;

    /// 函数级中文注释：检查用户是否是群组管理员
    ///
    /// # 参数
    /// - `group_id`: 群组ID
    /// - `user`: 用户账户
    ///
    /// # 返回
    /// - `true`: 是管理员
    /// - `false`: 不是管理员
    fn is_group_admin(group_id: u64, user: &AccountId) -> bool;

    /// 函数级中文注释：获取群组成员列表
    ///
    /// # 参数
    /// - `group_id`: 群组ID
    ///
    /// # 返回
    /// - 成员账户列表
    fn get_group_members(group_id: u64) -> Vec<AccountId>;
}

/// 函数级详细中文注释：空实现（用于不需要权限检查的场景）
impl<AccountId> ChatPermissionCheck<AccountId> for () {
    fn can_send_message(_sender: &AccountId, _receiver: &AccountId) -> bool {
        true // 默认允许
    }

    fn is_blocked(_blocker: &AccountId, _blocked: &AccountId) -> bool {
        false // 默认不拉黑
    }
}

impl<AccountId> FriendshipCheck<AccountId> for () {
    fn is_friend(_user1: &AccountId, _user2: &AccountId) -> bool {
        false
    }
}
