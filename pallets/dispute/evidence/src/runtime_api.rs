//! Runtime API 定义：用于前端查询证据与私密内容

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
use Debug;

// ============================================================================
// 证据 DTO
// ============================================================================

/// 证据摘要（列表页用）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct EvidenceSummary<AccountId> {
    pub id: u64,
    pub domain: u8,
    pub target_id: u64,
    pub owner: AccountId,
    pub content_cid: Vec<u8>,
    /// 0=Image, 1=Video, 2=Document, 3=Mixed, 4=Text
    pub content_type: u8,
    /// 0=Active, 1=Sealed, 2=Withdrawn, 3=Removed
    pub status: u8,
    pub is_encrypted: bool,
    pub created_at: u64,
    pub link_count: u32,
    pub child_count: u32,
    pub has_commit: bool,
}

/// 证据详情（详情页用，含完整关系）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct EvidenceDetail<AccountId, Balance> {
    pub summary: EvidenceSummary<AccountId>,
    pub ns: Option<[u8; 8]>,
    pub parent_id: Option<u64>,
    pub children_ids: Vec<u64>,
    pub deposit: Balance,
}

/// 证据分页结果
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct EvidencePage<AccountId> {
    pub items: Vec<EvidenceSummary<AccountId>>,
    pub total: u32,
}

// ============================================================================
// 私密内容 DTO
// ============================================================================

/// 私密内容元数据（公开查询，不含密钥）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct PrivateContentMeta<AccountId> {
    pub content_id: u64,
    pub ns: [u8; 8],
    pub subject_id: u64,
    pub cid: Vec<u8>,
    /// 0=None, 1=Aes256Gcm, 2=ChaCha20Poly1305, 3=XChaCha20Poly1305
    pub encryption_method: u8,
    pub creator: AccountId,
    /// 0=OwnerOnly, 1=SharedWith, 2=Timeboxed, 3=Governance, 4=RoleBased
    pub access_policy: u8,
    pub authorized_count: u32,
    pub pending_requests: u32,
    pub created_at: u64,
}

/// 解密包（仅授权用户可获取）
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct DecryptionPackage {
    pub cid: Vec<u8>,
    pub content_hash: [u8; 32],
    pub encryption_method: u8,
    pub encrypted_key: Vec<u8>,
}

/// 访问请求条目
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct AccessRequestEntry<AccountId> {
    pub requester: AccountId,
    pub requested_at: u64,
}

/// 用户公钥信息
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, Debug)]
pub struct PublicKeyInfo {
    pub key_data: Vec<u8>,
    /// 0=Rsa2048, 1=Ed25519, 2=EcdsaP256
    pub key_type: u8,
}

// ============================================================================
// Runtime API 声明
// ============================================================================

sp_api::decl_runtime_apis! {
    /// 证据模块 Runtime API
    ///
    /// 前端通过 `state_call` 调用，替代逐条读取 Storage 的 N+1 查询模式。
    pub trait EvidenceApi<AccountId, Balance>
    where
        AccountId: Codec,
        Balance: Codec,
    {
        // ==================== 证据查询 ====================

        /// 证据详情（含状态、子证据链、押金等，1 次调用替代前端的多次 RPC）
        fn get_evidence_detail(id: u64) -> Option<EvidenceDetail<AccountId, Balance>>;

        /// 按目标分页查询证据列表（争议详情页 → 关联证据标签）
        fn get_evidences_by_target(domain: u8, target_id: u64, offset: u32, limit: u32) -> EvidencePage<AccountId>;

        /// 按命名空间分页查询证据列表
        fn get_evidences_by_ns(ns: [u8; 8], subject_id: u64, offset: u32, limit: u32) -> EvidencePage<AccountId>;

        /// 我的证据（按 owner 分页查询）
        fn get_user_evidences(account: AccountId, offset: u32, limit: u32) -> EvidencePage<AccountId>;

        // ==================== 私密内容查询 ====================

        /// 私密内容元数据（公开查询，不含密钥包）
        fn get_private_content_meta(content_id: u64) -> Option<PrivateContentMeta<AccountId>>;

        /// 解密包（含权限校验，未授权返回 None）
        fn get_decryption_package(content_id: u64, viewer: AccountId) -> Option<DecryptionPackage>;

        /// 主体下的私密内容 ID 列表（分页）
        fn get_private_contents_by_subject(ns: [u8; 8], subject_id: u64, offset: u32, limit: u32) -> Vec<u64>;

        /// 待处理访问请求（分页）
        fn get_access_requests(content_id: u64, offset: u32, limit: u32) -> Vec<AccessRequestEntry<AccountId>>;

        // ==================== 密钥管理 ====================

        /// 查询用户公钥
        fn get_user_public_key(account: AccountId) -> Option<PublicKeyInfo>;
    }
}
