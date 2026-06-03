//! # 聊天系统共享类型定义
//!
//! 本模块定义了所有聊天子模块共享的核心类型

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

/// 函数级详细中文注释：消息类型枚举
///
/// 统一了所有聊天模块的消息类型定义：
/// - Text: 文本消息
/// - Image: 图片消息
/// - File: 文件消息
/// - Voice: 语音消息
/// - Video: 视频消息
/// - System: 系统消息（如订单状态变更）
/// - AI: AI生成消息
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
pub enum MessageType {
    /// 文本消息
    Text = 0,
    /// 图片消息
    Image = 1,
    /// 文件消息
    File = 2,
    /// 语音消息
    Voice = 3,
    /// 视频消息
    Video = 4,
    /// 系统消息（如订单状态变更、群组通知等）
    System = 5,
    /// AI生成消息
    AI = 6,
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Text
    }
}

impl MessageType {
    /// 函数级中文注释：从u8代码转换为MessageType枚举
    pub fn from_u8(code: u8) -> Self {
        match code {
            0 => MessageType::Text,
            1 => MessageType::Image,
            2 => MessageType::File,
            3 => MessageType::Voice,
            4 => MessageType::Video,
            5 => MessageType::System,
            6 => MessageType::AI,
            _ => MessageType::Text,
        }
    }

    /// 函数级中文注释：转换为u8代码
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// 函数级中文注释：检查是否为媒体类型消息
    pub fn is_media(&self) -> bool {
        matches!(self, MessageType::Image | MessageType::File | MessageType::Voice | MessageType::Video)
    }

    /// 函数级中文注释：检查是否需要IPFS存储
    pub fn requires_ipfs(&self) -> bool {
        // 所有类型都通过IPFS存储内容
        true
    }
}

/// 函数级详细中文注释：消息状态枚举
///
/// 用于跟踪消息的生命周期状态：
/// - Pending: 待发送（乐观UI使用）
/// - Sent: 已发送到链上
/// - Delivered: 已送达（接收方节点确认）
/// - Read: 已读
/// - Failed: 发送失败
/// - Deleted: 已删除（软删除）
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
pub enum MessageStatus {
    /// 待发送（乐观UI）
    Pending = 0,
    /// 已发送
    Sent = 1,
    /// 已送达
    Delivered = 2,
    /// 已读
    Read = 3,
    /// 发送失败
    Failed = 4,
    /// 已删除
    Deleted = 5,
}

impl Default for MessageStatus {
    fn default() -> Self {
        Self::Sent
    }
}

impl MessageStatus {
    /// 函数级中文注释：从u8代码转换
    pub fn from_u8(code: u8) -> Self {
        match code {
            0 => MessageStatus::Pending,
            1 => MessageStatus::Sent,
            2 => MessageStatus::Delivered,
            3 => MessageStatus::Read,
            4 => MessageStatus::Failed,
            5 => MessageStatus::Deleted,
            _ => MessageStatus::Sent,
        }
    }

    /// 函数级中文注释：转换为u8代码
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// 函数级中文注释：检查消息是否可见
    pub fn is_visible(&self) -> bool {
        !matches!(self, MessageStatus::Deleted | MessageStatus::Failed)
    }
}

/// 函数级详细中文注释：加密模式枚举
///
/// 用于群聊和需要加密的消息：
/// - Military: 军用级（量子抗性加密）
/// - Business: 商用级（标准AES-256）
/// - Selective: 选择性（用户自主选择加密级别）
/// - Transparent: 透明公开（无加密，用于公开群组）
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo, MaxEncodedLen, Debug)]
pub enum EncryptionMode {
    /// 军用级（量子抗性）
    Military = 0,
    /// 商用级（标准加密）
    Business = 1,
    /// 选择性（用户自主选择）
    Selective = 2,
    /// 透明公开（无加密）
    Transparent = 3,
}

impl Default for EncryptionMode {
    fn default() -> Self {
        Self::Business
    }
}

impl EncryptionMode {
    /// 函数级中文注释：从u8代码转换
    pub fn from_u8(code: u8) -> Self {
        match code {
            0 => EncryptionMode::Military,
            1 => EncryptionMode::Business,
            2 => EncryptionMode::Selective,
            3 => EncryptionMode::Transparent,
            _ => EncryptionMode::Business,
        }
    }

    /// 函数级中文注释：转换为u8代码
    pub fn to_u8(&self) -> u8 {
        *self as u8
    }

    /// 函数级中文注释：检查是否需要加密
    pub fn requires_encryption(&self) -> bool {
        !matches!(self, EncryptionMode::Transparent)
    }
}

/// 函数级详细中文注释：聊天用户ID类型
///
/// 11位数字的用户ID，范围：10,000,000,000 - 99,999,999,999
/// 用于隐私保护，避免暴露链上账户地址
pub type ChatUserId = u64;

/// 函数级详细中文注释：ChatUserId常量
pub mod chat_user_id {
    /// 最小ID值（11位数）
    pub const MIN_ID: u64 = 10_000_000_000;
    /// 最大ID值（11位数）
    pub const MAX_ID: u64 = 99_999_999_999;
    /// ID范围
    pub const ID_RANGE: u64 = MAX_ID - MIN_ID + 1;

    /// 函数级中文注释：验证ChatUserId是否有效
    pub fn is_valid(id: u64) -> bool {
        id >= MIN_ID && id <= MAX_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_conversion() {
        assert_eq!(MessageType::from_u8(0), MessageType::Text);
        assert_eq!(MessageType::from_u8(1), MessageType::Image);
        assert_eq!(MessageType::from_u8(6), MessageType::AI);
        assert_eq!(MessageType::from_u8(99), MessageType::Text); // 默认值
    }

    #[test]
    fn test_message_status_visibility() {
        assert!(MessageStatus::Sent.is_visible());
        assert!(MessageStatus::Read.is_visible());
        assert!(!MessageStatus::Deleted.is_visible());
        assert!(!MessageStatus::Failed.is_visible());
    }

    #[test]
    fn test_chat_user_id_validation() {
        assert!(!chat_user_id::is_valid(9_999_999_999));
        assert!(chat_user_id::is_valid(10_000_000_000));
        assert!(chat_user_id::is_valid(99_999_999_999));
        assert!(!chat_user_id::is_valid(100_000_000_000));
    }
}
