//! # Pallet Chat Common - 聊天系统共享模块
//!
//! ## 概述
//!
//! 本模块提供聊天系统各子模块共享的类型和工具：
//! - 消息类型枚举 (MessageType)
//! - 消息状态枚举 (MessageStatus)
//! - CID验证工具
//! - 频率限制工具
//! - 共享trait定义
//!
//! ## 模块结构
//!
//! - `types`: 共享类型定义
//! - `traits`: 共享trait定义
//! - `validation`: CID验证和媒体验证工具
//! - `rate_limit`: 频率限制工具

#![cfg_attr(not(feature = "std"), no_std)]

pub mod types;
pub mod traits;
pub mod validation;
pub mod rate_limit;
pub mod runtime_api;

// 重新导出常用类型
pub use types::*;
pub use traits::*;
pub use validation::*;
pub use rate_limit::*;
