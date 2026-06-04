//! # Pallet Chat Common — 聊天系统共享模块 / chat shared library
//!
//! ## 概述 / Overview
//!
//! EN: Dependency-light building blocks shared across the chat subsystem. After
//! the off-chain convergence (human messages — Text/Image/File/Voice, 1:1 and
//! group — are delivered off-chain via MLS + relay; only `System` notifications
//! are stored on chain), this crate intentionally holds ONLY what is actually
//! shared across pallets / runtime:
//! - `rate_limit`: a windowed anti-spam counter (used by `pallet-chat-group` to
//!   bound write-heavy MLS actions).
//! - `runtime_api`: the unified conversation-view Runtime API (`ChatViewApi`),
//!   whose aggregation is implemented in the runtime where both `ChatCore` and
//!   `ChatGroup` are visible.
//!
//! CN: 聊天子系统的轻量共享构件。链下收敛后（人类消息 Text/Image/File/Voice，无论
//! 私聊还是群聊，均走链下 MLS + relay；链上仅存 `System` 通知），本 crate 仅保留
//! **真正跨 pallet / runtime 共享**的部分：
//! - `rate_limit`：窗口化反垃圾计数器（`pallet-chat-group` 用于约束写入型 MLS 操作）。
//! - `runtime_api`：统一会话视图 Runtime API（`ChatViewApi`），聚合逻辑在 runtime 实现。
//!
//! ## 已移除（审计 P1 类型收敛）/ Removed (audit P1, type convergence)
//!
//! EN: The former on-chain message taxonomy (`MessageType` / `MessageStatus` /
//! `EncryptionMode` / a duplicate `ChatUserId`), the cross-pallet trait ports
//! (`ChatPermissionCheck` / `FriendshipCheck` / `ChatUserIdProvider` / …), and
//! the bypassable CID "encryption" heuristics were deleted: they had ZERO
//! callers and a divergent `MessageType` discriminant was an encoding footgun.
//! Single sources of truth now: the authoritative on-chain `MessageType` lives
//! in `pallet-chat-core`; permission is `pallet-chat-permission::
//! ChatPermissionChecker`; message taxonomy/encryption are client/off-chain
//! concerns guaranteed by MLS E2EE.
//! CN: 旧的链上消息分类（`MessageType` / `MessageStatus` / `EncryptionMode` 与重复的
//! `ChatUserId`）、跨 pallet trait 端口（`ChatPermissionCheck` / `FriendshipCheck` /
//! `ChatUserIdProvider` 等）以及可绕过的 CID「加密」启发式均已删除：它们无任何调用方，
//! 且 `MessageType` 判别值发散是编码地雷。唯一事实来源：链上权威 `MessageType` 在
//! `pallet-chat-core`；权限走 `pallet-chat-permission::ChatPermissionChecker`；消息分类/
//! 加密属客户端/链下职责，由 MLS 端到端加密保证。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod rate_limit;
pub mod runtime_api;

pub use rate_limit::*;
