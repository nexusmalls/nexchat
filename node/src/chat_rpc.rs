//! Custom JSON-RPC for the chat subsystem.
//! 聊天子系统的自定义 JSON-RPC。
//!
//! EN: Exposes the chat runtime APIs as named JSON-RPC methods so non
//! polkadot-js clients can call them directly with JSON-friendly types. The
//! pallet DTOs are SCALE-only (no serde); to keep the pallets pure we define
//! node-local serde response types here and convert from the runtime-API DTOs.
//! All methods are read-only and free (they run via `runtime_api`, not as
//! transactions).
//!
//! CN: 把聊天 runtime API 暴露为具名 JSON-RPC 方法，便于非 polkadot-js 客户端用
//! JSON 友好类型直接调用。pallet 的 DTO 仅有 SCALE（无 serde）；为保持 pallet 纯净，
//! 这里定义 node 本地的 serde 响应类型并从 runtime-API DTO 转换。所有方法只读且免费
//! （走 `runtime_api`，非交易）。
//!
//! # 链上切片 ≠ 完整消息列表 / On-chain slice ≠ full message list
//! EN: `chat_listConversations` / `chat_totalDirectUnread` return an ON-CHAIN
//! SLICE only. Human chat (1:1 and group) is off-chain (MLS + relay); the only
//! on-chain messages are `System` notifications. Direct `unread`/`last_active`
//! count the System channel ONLY, and a pair that only chatted off-chain has no
//! direct row. Clients MUST merge with local/off-chain MLS state — see the
//! client Merge Spec in `pallets/chat/README.md`.
//! CN: `chat_listConversations` / `chat_totalDirectUnread` 仅返回**链上切片**。人类聊天
//! （私聊与群聊）在链下（MLS + relay）；链上唯一消息是 `System` 通知。私聊 `unread`/
//! `last_active` 只计 System 通道，纯链下聊过的用户对无私聊行。客户端**必须**与本地/链下
//! MLS 状态合并——见 `pallets/chat/README.md` 的客户端 Merge Spec。
//!
//! 提供方法 / Methods:
//! - `chat_listConversations(who, at?)` — 链上会话切片（私聊 + 群聊；非完整列表）
//! - `chat_totalDirectUnread(who, at?)` — 链上 System 通道未读总数（非 App 全局角标）
//! - `chat_checkPermission(sender, receiver, at?)` — 聊天权限检查
//! - `chat_getActiveScenes(user1, user2, at?)` — 场景授权
//! - `chat_capabilityEpoch(who, at?)` — 聊天能力撤销纪元
//! - `chat_isAccountMuted(who, at?)` — 账户是否被治理平台级禁言
//! - `chat_privacySummary(who, at?)` — 隐私设置摘要
//! - `chat_inboxEpoch(inboxId, at?)` — 链下投递信箱的撤销纪元（未注册返回 null）
//! - `chat_isTagRevoked(inboxId, tag, at?)` — 联系人标签是否被定向撤销
//! - `chat_inboxExists(inboxId, at?)` — 信箱是否已注册

use std::marker::PhantomData;
use std::sync::Arc;

use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    types::{ErrorObject, ErrorObjectOwned},
};
use serde::{Deserialize, Serialize};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use nexus_runtime::{AccountId, BlockNumber, Hash};
use pallet_chat_common::runtime_api::{
    ChatViewApi as ChatViewRuntimeApi, ConversationKind, ConversationSummary,
};
use pallet_chat_inbox::runtime_api::ChatInboxApi as ChatInboxRuntimeApi;
use pallet_chat_permission::runtime_api::ChatPermissionApi as ChatPermissionRuntimeApi;
use pallet_chat_permission::{
    ChatPermissionLevel, PermissionResult, PrivacySettingsSummary, SceneAuthorizationInfo, SceneId,
    SceneType,
};

// ==================== node 本地 serde 响应类型 / node-local serde DTOs ====================

/// EN: One unified conversation row. CN: 统一会话列表中的一行。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcConversation {
    /// "direct" | "group"
    pub kind: String,
    pub direct_id: Option<Hash>,
    pub group_id: Option<u64>,
    pub peer: Option<AccountId>,
    /// EN: group name as UTF-8 lossy (empty for direct). CN: 群名（UTF-8 有损）。
    pub name: String,
    /// EN: group avatar IPFS CID (empty for direct). CN: 群头像 CID。
    pub avatar_cid: String,
    /// EN: direct → last-active block of the **System** channel (NOT human chat);
    /// group → 0 (off-chain). Merge with off-chain MLS recency.
    /// CN: 私聊 → **System** 通道最后活跃区块（非人类聊天）；群为 0。需与链下 MLS 合并。
    pub last_active: BlockNumber,
    /// EN: direct → **System**-channel unread only (NOT human chat); group → 0.
    /// App total unread MUST add off-chain MLS unread.
    /// CN: 私聊 → 仅 **System** 通道未读（非人类聊天）；群为 0。App 总未读需叠加链下 MLS。
    pub unread: u32,
    pub pinned: bool,
    /// EN: SEMANTICS DIFFER BY `kind` — branch on `kind`, do not share one icon.
    /// direct → caller's DND (no notifications); group → admin mute (CANNOT send).
    /// CN: 语义按 `kind` 不同——按 `kind` 分支，勿共用图标。私聊 → 免打扰(DND)；
    /// 群 → 管理员禁言（不能发言）。
    pub muted: bool,
    pub archived: bool,
    pub member_count: u32,
    /// EN: 0=Owner,1=Admin,2=Member,255=direct/non-member. CN: 群角色标记。
    pub group_role: u8,
}

impl From<ConversationSummary<AccountId, Hash, BlockNumber>> for RpcConversation {
    fn from(c: ConversationSummary<AccountId, Hash, BlockNumber>) -> Self {
        let kind = match c.kind {
            ConversationKind::Direct => "direct",
            ConversationKind::Group => "group",
        };
        RpcConversation {
            kind: kind.into(),
            direct_id: c.direct_id,
            group_id: c.group_id,
            peer: c.peer,
            name: String::from_utf8_lossy(&c.name).into_owned(),
            avatar_cid: String::from_utf8_lossy(&c.avatar_cid).into_owned(),
            last_active: c.last_active,
            unread: c.unread,
            pinned: c.pinned,
            muted: c.muted,
            archived: c.archived,
            member_count: c.member_count,
            group_role: c.group_role,
        }
    }
}

/// EN: Permission check outcome. CN: 权限检查结果。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPermissionResult {
    pub allowed: bool,
    /// "allowed"|"scene"|"blocked"|"requiresFriend"|"notInWhitelist"|"closed"|"senderMuted"
    pub reason: String,
    /// EN: scene types granting access (only for the "scene" reason).
    /// CN: 授予访问的场景类型（仅 "scene" 时非空）。
    pub scenes: Vec<String>,
}

impl From<PermissionResult> for RpcPermissionResult {
    fn from(r: PermissionResult) -> Self {
        let allowed = r.is_allowed();
        let (reason, scenes) = match r {
            PermissionResult::Allowed => ("allowed", Vec::new()),
            PermissionResult::AllowedByScene(v) => {
                ("scene", v.into_iter().map(scene_type_label).collect())
            }
            PermissionResult::DeniedBlocked => ("blocked", Vec::new()),
            PermissionResult::DeniedRequiresFriend => ("requiresFriend", Vec::new()),
            PermissionResult::DeniedNotInWhitelist => ("notInWhitelist", Vec::new()),
            PermissionResult::DeniedClosed => ("closed", Vec::new()),
            PermissionResult::DeniedSenderMuted => ("senderMuted", Vec::new()),
        };
        RpcPermissionResult { allowed, reason: reason.into(), scenes }
    }
}

/// EN: Scene identifier. CN: 场景标识。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSceneId {
    /// "none" | "numeric" | "hash"
    pub kind: String,
    pub numeric: Option<u64>,
    pub hash: Option<Hash>,
}

impl From<SceneId> for RpcSceneId {
    fn from(id: SceneId) -> Self {
        match id {
            SceneId::None => RpcSceneId { kind: "none".into(), numeric: None, hash: None },
            SceneId::Numeric(n) => {
                RpcSceneId { kind: "numeric".into(), numeric: Some(n), hash: None }
            }
            SceneId::Hash(h) => {
                RpcSceneId { kind: "hash".into(), numeric: None, hash: Some(Hash::from(h)) }
            }
        }
    }
}

/// EN: Active scene authorization. CN: 场景授权详情。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSceneAuth {
    pub scene_type: String,
    pub scene_id: RpcSceneId,
    pub is_expired: bool,
    pub expires_at: Option<u64>,
    /// EN: metadata as UTF-8 lossy. CN: 元数据（UTF-8 有损）。
    pub metadata: String,
}

impl From<SceneAuthorizationInfo> for RpcSceneAuth {
    fn from(s: SceneAuthorizationInfo) -> Self {
        RpcSceneAuth {
            scene_type: scene_type_label(s.scene_type),
            scene_id: s.scene_id.into(),
            is_expired: s.is_expired,
            expires_at: s.expires_at,
            metadata: String::from_utf8_lossy(&s.metadata).into_owned(),
        }
    }
}

/// EN: Privacy settings summary. CN: 隐私设置摘要。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcPrivacySummary {
    /// "open" | "friendsOnly" | "whitelist" | "closed"
    pub permission_level: String,
    pub rejected_scene_types: Vec<String>,
}

impl From<PrivacySettingsSummary> for RpcPrivacySummary {
    fn from(p: PrivacySettingsSummary) -> Self {
        // 审计 P1：链上黑/白名单已移除，摘要不再含 block_list_count / whitelist_count。
        // Audit P1: on-chain block/whitelist removed; summary drops those counts.
        RpcPrivacySummary {
            permission_level: permission_level_label(p.permission_level).into(),
            rejected_scene_types: p.rejected_scene_types.into_iter().map(scene_type_label).collect(),
        }
    }
}

/// EN: Stable label for a scene type. CN: 场景类型的稳定标签。
fn scene_type_label(t: SceneType) -> String {
    match t {
        SceneType::MarketMaker => "marketMaker".into(),
        SceneType::Order => "order".into(),
        SceneType::Memorial => "memorial".into(),
        SceneType::Group => "group".into(),
        SceneType::Direct => "direct".into(),
        SceneType::Custom(bytes) => {
            alloc_format(format_args!("custom:{}", String::from_utf8_lossy(&bytes)))
        }
    }
}

/// EN: Stable label for a permission level. CN: 权限级别的稳定标签。
fn permission_level_label(l: ChatPermissionLevel) -> &'static str {
    match l {
        ChatPermissionLevel::Open => "open",
        ChatPermissionLevel::FriendsOnly => "friendsOnly",
        ChatPermissionLevel::Whitelist => "whitelist",
        ChatPermissionLevel::Closed => "closed",
    }
}

fn alloc_format(args: core::fmt::Arguments<'_>) -> String {
    use core::fmt::Write;
    let mut s = String::new();
    let _ = s.write_fmt(args);
    s
}

// ==================== RPC trait & impl ====================

/// EN: Chat JSON-RPC surface. CN: 聊天 JSON-RPC 接口。
#[rpc(client, server)]
pub trait ChatApi<BlockHash> {
    /// 统一会话列表（私聊 + 群聊）。
    #[method(name = "chat_listConversations")]
    fn list_conversations(
        &self,
        who: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<Vec<RpcConversation>>;

    /// 私聊未读总数。
    #[method(name = "chat_totalDirectUnread")]
    fn total_direct_unread(&self, who: AccountId, at: Option<BlockHash>) -> RpcResult<u32>;

    /// 聊天权限检查。
    #[method(name = "chat_checkPermission")]
    fn check_permission(
        &self,
        sender: AccountId,
        receiver: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<RpcPermissionResult>;

    /// 两用户间有效场景授权。
    #[method(name = "chat_getActiveScenes")]
    fn get_active_scenes(
        &self,
        user1: AccountId,
        user2: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<Vec<RpcSceneAuth>>;

    /// EN: Current chat-capability revocation epoch of `who` (0 if never bumped).
    /// CN: `who` 当前的聊天能力撤销纪元（从未递增则为 0）。
    #[method(name = "chat_capabilityEpoch")]
    fn capability_epoch(&self, who: AccountId, at: Option<BlockHash>) -> RpcResult<u32>;

    /// 账户是否被治理平台级禁言。
    #[method(name = "chat_isAccountMuted")]
    fn is_account_muted(&self, who: AccountId, at: Option<BlockHash>) -> RpcResult<bool>;

    /// 隐私设置摘要。
    #[method(name = "chat_privacySummary")]
    fn privacy_summary(
        &self,
        who: AccountId,
        at: Option<BlockHash>,
    ) -> RpcResult<RpcPrivacySummary>;

    /// EN: Inbox-keyed revocation epoch of an off-chain delivery inbox; `None` if
    /// the inbox is not registered. Relays compare it against the epoch embedded
    /// in a blinded delivery token. CN: 链下投递信箱的 inbox 维度撤销纪元；信箱未注册
    /// 则为 `None`。relay 用它与盲化投递令牌内嵌纪元比对。
    #[method(name = "chat_inboxEpoch")]
    fn inbox_epoch(&self, inbox_id: Hash, at: Option<BlockHash>) -> RpcResult<Option<u32>>;

    /// EN: Whether a contact `tag` is currently revoked for `inbox_id` (targeted
    /// revocation). CN: 联系人 `tag` 当前是否在 `inbox_id` 下被撤销（定向撤销）。
    #[method(name = "chat_isTagRevoked")]
    fn is_tag_revoked(&self, inbox_id: Hash, tag: Hash, at: Option<BlockHash>) -> RpcResult<bool>;

    /// EN: Whether `inbox_id` is registered. CN: `inbox_id` 是否已注册。
    #[method(name = "chat_inboxExists")]
    fn inbox_exists(&self, inbox_id: Hash, at: Option<BlockHash>) -> RpcResult<bool>;
}

/// EN: RPC handler holding a client handle. CN: 持有 client 的 RPC 处理器。
pub struct Chat<C, B> {
    client: Arc<C>,
    _marker: PhantomData<B>,
}

impl<C, B> Chat<C, B> {
    /// Create a new handler. / 新建处理器。
    pub fn new(client: Arc<C>) -> Self {
        Self { client, _marker: PhantomData }
    }
}

/// EN: Map a runtime-API error to a JSON-RPC error. CN: 把 runtime-API 错误映射为 RPC 错误。
fn runtime_err(e: impl core::fmt::Display) -> ErrorObjectOwned {
    ErrorObject::owned(1, "Chat runtime API error", Some(e.to_string()))
}

impl<C, Block> ChatApiServer<<Block as BlockT>::Hash> for Chat<C, Block>
where
    Block: BlockT,
    C: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
    C::Api: ChatViewRuntimeApi<Block, AccountId, Hash, BlockNumber>,
    C::Api: ChatPermissionRuntimeApi<Block, AccountId>,
    C::Api: ChatInboxRuntimeApi<Block>,
{
    fn list_conversations(
        &self,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Vec<RpcConversation>> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        let list = api.list_conversations(at, who).map_err(runtime_err)?;
        Ok(list.into_iter().map(RpcConversation::from).collect())
    }

    fn total_direct_unread(
        &self,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<u32> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.total_direct_unread(at, who).map_err(runtime_err)
    }

    fn check_permission(
        &self,
        sender: AccountId,
        receiver: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<RpcPermissionResult> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        let res = api.check_chat_permission(at, sender, receiver).map_err(runtime_err)?;
        Ok(res.into())
    }

    fn get_active_scenes(
        &self,
        user1: AccountId,
        user2: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Vec<RpcSceneAuth>> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        let scenes = api.get_active_scenes(at, user1, user2).map_err(runtime_err)?;
        Ok(scenes.into_iter().map(RpcSceneAuth::from).collect())
    }

    fn capability_epoch(
        &self,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<u32> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.capability_epoch(at, who).map_err(runtime_err)
    }

    fn is_account_muted(
        &self,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<bool> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.is_account_muted(at, who).map_err(runtime_err)
    }

    fn privacy_summary(
        &self,
        who: AccountId,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<RpcPrivacySummary> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        let summary = api.get_privacy_settings_summary(at, who).map_err(runtime_err)?;
        Ok(summary.into())
    }

    fn inbox_epoch(
        &self,
        inbox_id: Hash,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<Option<u32>> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.inbox_epoch(at, inbox_id.0).map_err(runtime_err)
    }

    fn is_tag_revoked(
        &self,
        inbox_id: Hash,
        tag: Hash,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<bool> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.is_tag_revoked(at, inbox_id.0, tag.0).map_err(runtime_err)
    }

    fn inbox_exists(
        &self,
        inbox_id: Hash,
        at: Option<<Block as BlockT>::Hash>,
    ) -> RpcResult<bool> {
        let api = self.client.runtime_api();
        let at = at.unwrap_or_else(|| self.client.info().best_hash);
        api.inbox_exists(at, inbox_id.0).map_err(runtime_err)
    }
}
