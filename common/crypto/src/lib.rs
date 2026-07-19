//! # pallet-crypto-common
//!
//! NEXUS 加密内容公共类型库，供多个 pallet 共享：
//! - `PrivateContent` — 加密内容存储结构
//! - `AccessPolicy` — 访问控制策略
//! - `UserPublicKey` — 用户公钥存储
//! - `KeyRotationRecord` — 密钥轮换记录
//! - `EncryptionMethod` — 加密方法枚举
//! - `KeyType` — 密钥类型枚举
//!
//! ## 设计原则
//!
//! 所有 struct/enum 使用 **原始泛型参数**（AccountId, BlockNumber, MaxCidLen 等），
//! 不依赖任何特定 pallet 的 Config trait。各 pallet 通过 type alias 映射自身 Config：
//!
//! ```rust,ignore
//! // 在 pallet-dispute-evidence 的 private_content.rs 中：
//! pub type PrivateContentOf<T> = pallet_crypto_common::PrivateContent<
//!     <T as frame_system::Config>::AccountId,
//!     BlockNumberFor<T>,
//!     <T as Config>::MaxCidLen,
//!     <T as Config>::MaxAuthorizedUsers,
//!     <T as Config>::MaxKeyLen,
//! >;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::fmt::Debug;
use frame_support::{
    pallet_prelude::*, BoundedVec, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_core::{ConstU32, H256};

// ============================================================
// EncryptionMethod — 替代原始 u8
// ============================================================

/// 加密方法
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum EncryptionMethod {
    /// 未加密（公开内容）
    #[codec(index = 0)]
    None = 0,
    /// AES-256-GCM（推荐，高性能 AEAD）
    #[codec(index = 1)]
    Aes256Gcm = 1,
    /// ChaCha20-Poly1305（移动端友好 AEAD）
    #[codec(index = 2)]
    ChaCha20Poly1305 = 2,
    /// XChaCha20-Poly1305（扩展 nonce，防重放更安全）
    #[codec(index = 3)]
    XChaCha20Poly1305 = 3,
}

impl Default for EncryptionMethod {
    fn default() -> Self {
        Self::None
    }
}

impl EncryptionMethod {
    /// 从原始 u8 转换（兼容旧 pallet-dispute-evidence 存储）
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Aes256Gcm,
            2 => Self::ChaCha20Poly1305,
            3 => Self::XChaCha20Poly1305,
            _ => Self::None,
        }
    }

    /// 转换为 u8（兼容旧存储）
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// 是否为加密方法
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, Self::None)
    }
}

// ============================================================
// KeyType — 替代原始 u8
// ============================================================

/// 密钥类型
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum KeyType {
    /// RSA-2048（DER 格式，270-512 字节）
    #[codec(index = 1)]
    Rsa2048 = 1,
    /// Ed25519（32 字节）
    #[codec(index = 2)]
    Ed25519 = 2,
    /// ECDSA-P256（33 或 65 字节）
    #[codec(index = 3)]
    EcdsaP256 = 3,
}

impl KeyType {
    /// 从原始 u8 转换
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Rsa2048),
            2 => Some(Self::Ed25519),
            3 => Some(Self::EcdsaP256),
            _ => None,
        }
    }

    /// 转换为 u8
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// 验证密钥数据长度是否合法
    pub fn validate_key_len(&self, len: usize) -> bool {
        match self {
            Self::Rsa2048 => (270..=512).contains(&len),
            Self::Ed25519 => len == 32,
            Self::EcdsaP256 => len == 33 || len == 65,
        }
    }
}

// ============================================================
// AccessPolicy — 访问控制策略（原始泛型）
// ============================================================

/// 访问控制策略
///
/// 定义谁可以解密加密内容。所有需要加密的 pallet（evidence、arbitration、kyc、order）
/// 均使用此枚举管理访问权限。
///
/// 泛型参数：
/// - `AccountId` — 账户类型
/// - `BlockNumber` — 区块号类型
/// - `MaxAuthorizedUsers` — 最大授权用户数上限
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    CloneNoBound,
    PartialEqNoBound,
    EqNoBound,
    DebugNoBound,
    TypeInfo,
    MaxEncodedLen,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[codec(mel_bound(
    AccountId: MaxEncodedLen,
    BlockNumber: MaxEncodedLen,
))]
#[scale_info(skip_type_params(MaxAuthorizedUsers))]
pub enum AccessPolicy<
    AccountId: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
    BlockNumber: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
    MaxAuthorizedUsers: Get<u32>,
> {
    /// 仅创建者可访问
    OwnerOnly,
    /// 指定用户列表
    SharedWith(BoundedVec<AccountId, MaxAuthorizedUsers>),
    /// 定时访问（到期后自动撤销）
    TimeboxedAccess {
        users: BoundedVec<AccountId, MaxAuthorizedUsers>,
        expires_at: BlockNumber,
    },
    /// 治理控制（由治理提案决定访问权限）
    GovernanceControlled,
    /// 基于角色的访问（扩展用，role 标识符最长 32 字节）
    RoleBased(BoundedVec<u8, ConstU32<32>>),
}

impl<
        AccountId: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
        BlockNumber: Encode
            + Decode
            + Clone
            + PartialEq
            + Eq
            + PartialOrd
            + core::fmt::Debug
            + TypeInfo
            + MaxEncodedLen,
        MaxAuthorizedUsers: Get<u32>,
    > AccessPolicy<AccountId, BlockNumber, MaxAuthorizedUsers>
{
    /// 检查指定用户是否被当前策略授权访问
    ///
    /// - `user`: 待检查的用户
    /// - `creator`: 内容创建者（OwnerOnly 策略使用）
    /// - `now`: 当前区块号（TimeboxedAccess 过期检查使用）
    ///
    /// 注意：GovernanceControlled 和 RoleBased 需要外部逻辑判断，此方法对其返回 false
    pub fn is_authorized(&self, user: &AccountId, creator: &AccountId, now: &BlockNumber) -> bool {
        match self {
            Self::OwnerOnly => user == creator,
            Self::SharedWith(users) => user == creator || users.contains(user),
            Self::TimeboxedAccess { users, expires_at } => {
                if now > expires_at {
                    // 过期后仅创建者可访问
                    user == creator
                } else {
                    user == creator || users.contains(user)
                }
            }
            // 治理控制和角色访问需要外部逻辑，此处无法判断
            Self::GovernanceControlled => user == creator,
            Self::RoleBased(_) => user == creator,
        }
    }

    /// 检查 TimeboxedAccess 是否已过期
    pub fn is_expired(&self, now: &BlockNumber) -> bool {
        match self {
            Self::TimeboxedAccess { expires_at, .. } => now > expires_at,
            _ => false,
        }
    }
}

// ============================================================
// ContentStatus — 内容生命周期状态
// ============================================================

/// 内容生命周期状态
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    TypeInfo,
    MaxEncodedLen,
)]
pub enum ContentStatus {
    /// 活跃（可正常访问）
    #[codec(index = 0)]
    Active = 0,
    /// 已冻结（争议/仲裁期间禁止修改）
    #[codec(index = 1)]
    Frozen = 1,
    /// 已归档（不可修改，仅可读取）
    #[codec(index = 2)]
    Archived = 2,
    /// 已清除（GDPR 合规删除，元数据保留但内容不可恢复）
    #[codec(index = 3)]
    Purged = 3,
}

impl Default for ContentStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl ContentStatus {
    /// 内容是否可被修改（授权/密钥轮换等）
    pub fn is_mutable(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 内容是否可读取
    pub fn is_readable(&self) -> bool {
        !matches!(self, Self::Purged)
    }
}

// ============================================================
// PrivateContent — 加密内容存储结构（原始泛型）
// ============================================================

/// 加密内容存储结构
///
/// 链上存储加密内容的元数据（CID、哈希、密钥包），实际加密数据存储在 IPFS。
/// 任何需要加密 IPFS 内容的 pallet 均可复用此结构。
///
/// 泛型参数：
/// - `AccountId` — 账户类型
/// - `BlockNumber` — 区块号类型
/// - `MaxCidLen` — CID 最大字节数
/// - `MaxAuthorizedUsers` — 最大授权用户数
/// - `MaxKeyLen` — 加密密钥最大字节数
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    CloneNoBound,
    PartialEqNoBound,
    EqNoBound,
    DebugNoBound,
    TypeInfo,
    MaxEncodedLen,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[codec(mel_bound(
    AccountId: MaxEncodedLen,
    BlockNumber: MaxEncodedLen,
))]
#[scale_info(skip_type_params(MaxCidLen, MaxAuthorizedUsers, MaxKeyLen))]
pub struct PrivateContent<
    AccountId: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
    BlockNumber: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
    MaxCidLen: Get<u32>,
    MaxAuthorizedUsers: Get<u32>,
    MaxKeyLen: Get<u32>,
> {
    /// 内容ID（由使用方 pallet 分配）
    pub id: u64,
    /// 命名空间（8 字节，用于隔离不同业务域）
    pub ns: [u8; 8],
    /// 业务主体ID（如 evidence_id、complaint_id、order_id）
    pub subject_id: u64,
    /// IPFS CID（明文存储，便于索引和去重）
    pub cid: BoundedVec<u8, MaxCidLen>,
    /// 原始内容的哈希（用于验证完整性，SHA-256）
    pub content_hash: H256,
    /// 加密方法
    pub encryption_method: EncryptionMethod,
    /// 创建者
    pub creator: AccountId,
    /// 访问控制策略
    pub access_policy: AccessPolicy<AccountId, BlockNumber, MaxAuthorizedUsers>,
    /// 每个授权用户的加密密钥包
    pub encrypted_keys: BoundedVec<(AccountId, BoundedVec<u8, MaxKeyLen>), MaxAuthorizedUsers>,
    /// 内容状态
    pub status: ContentStatus,
    /// 创建时间
    pub created_at: BlockNumber,
    /// 最后更新时间
    pub updated_at: BlockNumber,
}

// ============================================================
// UserPublicKey — 用户公钥存储（原始泛型）
// ============================================================

/// 用户公钥存储
///
/// 用于非对称加密密钥分发：创建者用接收方公钥加密对称密钥，
/// 接收方用私钥解密对称密钥，再用对称密钥解密内容。
#[derive(
    Encode,
    Decode,
    DecodeWithMemTracking,
    CloneNoBound,
    PartialEqNoBound,
    EqNoBound,
    DebugNoBound,
    TypeInfo,
    MaxEncodedLen,
)]
#[codec(encode_bound())]
#[codec(decode_bound())]
#[codec(mel_bound(BlockNumber: MaxEncodedLen))]
#[scale_info(skip_type_params(MaxKeyLen))]
pub struct UserPublicKey<
    BlockNumber: Encode + Decode + Clone + PartialEq + Eq + core::fmt::Debug + TypeInfo + MaxEncodedLen,
    MaxKeyLen: Get<u32>,
> {
    /// 公钥数据（DER/原始格式）
    pub key_data: BoundedVec<u8, MaxKeyLen>,
    /// 密钥类型
    pub key_type: KeyType,
    /// 注册时间（区块号）
    pub registered_at: BlockNumber,
}

// ============================================================
// KeyRotationRecord — 密钥轮换记录（原始泛型）
// ============================================================

/// 密钥轮换记录
///
/// 当加密内容的对称密钥需要更换时（如用户被撤销访问权限后），
/// 记录每次轮换的元数据，便于审计和追踪。
#[derive(
    Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct KeyRotationRecord<AccountId, BlockNumber> {
    /// 内容ID
    pub content_id: u64,
    /// 轮换批次（递增）
    pub rotation_round: u32,
    /// 轮换时间
    pub rotated_at: BlockNumber,
    /// 轮换者
    pub rotated_by: AccountId,
}

// ============================================================
// PrivateContentManager trait — 加密内容写入管理接口
// ============================================================

/// 加密内容写入管理接口
///
/// 供业务 pallet 通过 trait 间接操作加密内容，
/// 无需直接依赖 pallet-dispute-evidence 的存储。
pub trait PrivateContentManager<AccountId> {
    /// 存储加密内容，返回 content_id
    fn store_private_content(
        creator: AccountId,
        ns: [u8; 8],
        subject_id: u64,
        cid: Vec<u8>,
        content_hash: H256,
        encryption_method: EncryptionMethod,
    ) -> Result<u64, sp_runtime::DispatchError>;

    /// 授权用户访问
    fn grant_access(
        grantor: AccountId,
        content_id: u64,
        user: AccountId,
        encrypted_key: Vec<u8>,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 撤销用户访问
    fn revoke_access(
        revoker: AccountId,
        content_id: u64,
        user: AccountId,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 检查用户是否有访问权限
    fn has_access(content_id: u64, user: &AccountId) -> bool;

    /// 更新内容 CID（重新加密后上传新文件）
    fn update_content(
        updater: AccountId,
        content_id: u64,
        new_cid: Vec<u8>,
        new_content_hash: H256,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 密钥轮换（撤销后重新加密并分发新密钥）
    fn rotate_keys(
        rotator: AccountId,
        content_id: u64,
        new_encrypted_keys: Vec<(AccountId, Vec<u8>)>,
    ) -> Result<u32, sp_runtime::DispatchError>;

    /// 治理/管理员强制授权（GovernanceControlled 策略执行路径）
    fn force_grant_access(
        content_id: u64,
        user: AccountId,
        encrypted_key: Vec<u8>,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 治理/管理员强制撤销
    fn force_revoke_access(
        content_id: u64,
        user: AccountId,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 冻结内容（争议/仲裁期间）
    fn freeze_content(content_id: u64) -> Result<(), sp_runtime::DispatchError>;

    /// 解冻内容
    fn unfreeze_content(content_id: u64) -> Result<(), sp_runtime::DispatchError>;
}

// ============================================================
// PrivateContentProvider trait — 只读查询接口
// ============================================================

/// 加密内容只读查询接口
///
/// 供其他 pallet（如 arbitration）低耦合查询加密内容权限和解密密钥，
/// 不需要写入权限。
pub trait PrivateContentProvider<AccountId> {
    /// 检查用户是否可以访问指定的私密内容
    fn can_access(content_id: u64, user: &AccountId) -> bool;

    /// 获取用户的加密密钥包（用于解密）
    fn get_encrypted_key(content_id: u64, user: &AccountId) -> Option<Vec<u8>>;

    /// 获取完整解密信息：(cid, content_hash, encryption_method, encrypted_key)
    fn get_decryption_info(
        content_id: u64,
        user: &AccountId,
    ) -> Option<(Vec<u8>, H256, EncryptionMethod, Vec<u8>)>;

    /// 获取内容状态
    fn get_content_status(content_id: u64) -> Option<ContentStatus>;

    /// 获取内容创建者
    fn get_content_creator(content_id: u64) -> Option<AccountId>;
}

// ============================================================
// KeyManager trait — 公钥管理接口
// ============================================================

/// 用户公钥管理接口
///
/// 每个需要加密功能的 pallet 都需要查找用户公钥，
/// 此 trait 提供统一的公钥注册、查询和撤销能力。
pub trait KeyManager<AccountId> {
    /// 注册用户公钥
    fn register_public_key(
        user: AccountId,
        key_type: KeyType,
        key_data: Vec<u8>,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 更新用户公钥
    fn update_public_key(
        user: AccountId,
        key_type: KeyType,
        key_data: Vec<u8>,
    ) -> Result<(), sp_runtime::DispatchError>;

    /// 撤销用户公钥
    fn revoke_public_key(user: AccountId) -> Result<(), sp_runtime::DispatchError>;

    /// 查询用户是否已注册公钥
    fn has_public_key(user: &AccountId) -> bool;

    /// 获取用户公钥类型
    fn get_key_type(user: &AccountId) -> Option<KeyType>;

    /// 获取用户公钥数据
    fn get_key_data(user: &AccountId) -> Option<Vec<u8>>;
}

// ============================================================
// CID 验证公共 helper
// ============================================================

/// CID 格式验证（基本检查）
///
/// 验证 CID 非空、长度合理、不含非法字符。
/// 各 pallet 可直接调用此函数，避免各自实现。
pub fn validate_cid(cid: &[u8]) -> bool {
    // CID 不能为空
    if cid.is_empty() {
        return false;
    }
    // CID 长度应在 1-128 字节之间（CIDv0: 46, CIDv1: ~36-64）
    if cid.len() > 128 {
        return false;
    }
    // 基本 ASCII 可打印字符检查（CID 通常是 base32/base58/base64 编码）
    cid.iter().all(|&b| b > 0x20 && b < 0x7f)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use codec::{Decode, Encode};
    use frame_support::BoundedVec;
    use sp_core::ConstU32;

    type MaxUsers = ConstU32<10>;

    // --- EncryptionMethod ---

    #[test]
    fn encryption_method_from_u8_roundtrip() {
        assert_eq!(EncryptionMethod::from_u8(0), EncryptionMethod::None);
        assert_eq!(EncryptionMethod::from_u8(1), EncryptionMethod::Aes256Gcm);
        assert_eq!(
            EncryptionMethod::from_u8(2),
            EncryptionMethod::ChaCha20Poly1305
        );
        assert_eq!(
            EncryptionMethod::from_u8(3),
            EncryptionMethod::XChaCha20Poly1305
        );
        assert_eq!(EncryptionMethod::from_u8(99), EncryptionMethod::None);
    }

    #[test]
    fn encryption_method_as_u8() {
        assert_eq!(EncryptionMethod::None.as_u8(), 0);
        assert_eq!(EncryptionMethod::Aes256Gcm.as_u8(), 1);
        assert_eq!(EncryptionMethod::ChaCha20Poly1305.as_u8(), 2);
        assert_eq!(EncryptionMethod::XChaCha20Poly1305.as_u8(), 3);
    }

    #[test]
    fn encryption_method_is_encrypted() {
        assert!(!EncryptionMethod::None.is_encrypted());
        assert!(EncryptionMethod::Aes256Gcm.is_encrypted());
        assert!(EncryptionMethod::ChaCha20Poly1305.is_encrypted());
        assert!(EncryptionMethod::XChaCha20Poly1305.is_encrypted());
    }

    #[test]
    fn encryption_method_default_is_none() {
        assert_eq!(EncryptionMethod::default(), EncryptionMethod::None);
    }

    #[test]
    fn encryption_method_scale_backward_compat() {
        // SCALE 编码必须与原始 u8 值一致
        assert_eq!(EncryptionMethod::None.encode(), 0u8.encode());
        assert_eq!(EncryptionMethod::Aes256Gcm.encode(), 1u8.encode());
        assert_eq!(EncryptionMethod::ChaCha20Poly1305.encode(), 2u8.encode());
        assert_eq!(EncryptionMethod::XChaCha20Poly1305.encode(), 3u8.encode());
    }

    // --- KeyType ---

    #[test]
    fn key_type_from_u8() {
        assert_eq!(KeyType::from_u8(1), Some(KeyType::Rsa2048));
        assert_eq!(KeyType::from_u8(2), Some(KeyType::Ed25519));
        assert_eq!(KeyType::from_u8(3), Some(KeyType::EcdsaP256));
        assert_eq!(KeyType::from_u8(0), None);
        assert_eq!(KeyType::from_u8(4), None);
    }

    #[test]
    fn key_type_as_u8() {
        assert_eq!(KeyType::Rsa2048.as_u8(), 1);
        assert_eq!(KeyType::Ed25519.as_u8(), 2);
        assert_eq!(KeyType::EcdsaP256.as_u8(), 3);
    }

    #[test]
    fn key_type_scale_backward_compat() {
        // #[codec(index = N)] 确保 SCALE 编码与原始 u8 值一致
        assert_eq!(KeyType::Rsa2048.encode(), 1u8.encode());
        assert_eq!(KeyType::Ed25519.encode(), 2u8.encode());
        assert_eq!(KeyType::EcdsaP256.encode(), 3u8.encode());
    }

    #[test]
    fn key_type_validate_key_len() {
        // RSA-2048: 270-512
        assert!(KeyType::Rsa2048.validate_key_len(270));
        assert!(KeyType::Rsa2048.validate_key_len(512));
        assert!(!KeyType::Rsa2048.validate_key_len(269));
        assert!(!KeyType::Rsa2048.validate_key_len(513));

        // Ed25519: exactly 32
        assert!(KeyType::Ed25519.validate_key_len(32));
        assert!(!KeyType::Ed25519.validate_key_len(31));
        assert!(!KeyType::Ed25519.validate_key_len(33));

        // ECDSA-P256: 33 or 65
        assert!(KeyType::EcdsaP256.validate_key_len(33));
        assert!(KeyType::EcdsaP256.validate_key_len(65));
        assert!(!KeyType::EcdsaP256.validate_key_len(32));
        assert!(!KeyType::EcdsaP256.validate_key_len(64));
    }

    // --- ContentStatus ---

    #[test]
    fn content_status_default_is_active() {
        assert_eq!(ContentStatus::default(), ContentStatus::Active);
    }

    #[test]
    fn content_status_is_mutable() {
        assert!(ContentStatus::Active.is_mutable());
        assert!(!ContentStatus::Frozen.is_mutable());
        assert!(!ContentStatus::Archived.is_mutable());
        assert!(!ContentStatus::Purged.is_mutable());
    }

    #[test]
    fn content_status_is_readable() {
        assert!(ContentStatus::Active.is_readable());
        assert!(ContentStatus::Frozen.is_readable());
        assert!(ContentStatus::Archived.is_readable());
        assert!(!ContentStatus::Purged.is_readable());
    }

    #[test]
    fn content_status_scale_encoding() {
        assert_eq!(ContentStatus::Active.encode(), 0u8.encode());
        assert_eq!(ContentStatus::Frozen.encode(), 1u8.encode());
        assert_eq!(ContentStatus::Archived.encode(), 2u8.encode());
        assert_eq!(ContentStatus::Purged.encode(), 3u8.encode());
    }

    // --- AccessPolicy ---

    #[test]
    fn access_policy_owner_only() {
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::OwnerOnly;
        let creator = 1u64;
        let other = 2u64;
        let now = 100u32;

        assert!(policy.is_authorized(&creator, &creator, &now));
        assert!(!policy.is_authorized(&other, &creator, &now));
    }

    #[test]
    fn access_policy_shared_with() {
        let users: BoundedVec<u64, MaxUsers> = vec![2u64, 3u64].try_into().unwrap();
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::SharedWith(users);
        let creator = 1u64;
        let now = 100u32;

        // 创建者始终有权限
        assert!(policy.is_authorized(&creator, &creator, &now));
        // 授权用户有权限
        assert!(policy.is_authorized(&2u64, &creator, &now));
        assert!(policy.is_authorized(&3u64, &creator, &now));
        // 非授权用户无权限
        assert!(!policy.is_authorized(&4u64, &creator, &now));
    }

    #[test]
    fn access_policy_timeboxed_not_expired() {
        let users: BoundedVec<u64, MaxUsers> = vec![2u64].try_into().unwrap();
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::TimeboxedAccess {
            users,
            expires_at: 200u32,
        };
        let creator = 1u64;
        let now = 100u32; // < 200，未过期

        assert!(policy.is_authorized(&creator, &creator, &now));
        assert!(policy.is_authorized(&2u64, &creator, &now));
        assert!(!policy.is_expired(&now));
    }

    #[test]
    fn access_policy_timeboxed_expired() {
        let users: BoundedVec<u64, MaxUsers> = vec![2u64].try_into().unwrap();
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::TimeboxedAccess {
            users,
            expires_at: 100u32,
        };
        let creator = 1u64;
        let now = 200u32; // > 100，已过期

        // 过期后仅创建者可访问
        assert!(policy.is_authorized(&creator, &creator, &now));
        assert!(!policy.is_authorized(&2u64, &creator, &now));
        assert!(policy.is_expired(&now));
    }

    #[test]
    fn access_policy_timeboxed_at_boundary() {
        let users: BoundedVec<u64, MaxUsers> = vec![2u64].try_into().unwrap();
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::TimeboxedAccess {
            users,
            expires_at: 100u32,
        };
        let creator = 1u64;

        // now == expires_at，未过期（> 才算过期）
        assert!(policy.is_authorized(&2u64, &creator, &100u32));
        assert!(!policy.is_expired(&100u32));
    }

    #[test]
    fn access_policy_governance_controlled() {
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::GovernanceControlled;
        let creator = 1u64;
        let now = 100u32;

        // 仅创建者有内建权限，其他需外部逻辑
        assert!(policy.is_authorized(&creator, &creator, &now));
        assert!(!policy.is_authorized(&2u64, &creator, &now));
        assert!(!policy.is_expired(&now));
    }

    #[test]
    fn access_policy_role_based() {
        let role: BoundedVec<u8, ConstU32<32>> = b"admin".to_vec().try_into().unwrap();
        let policy: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::RoleBased(role);
        let creator = 1u64;
        let now = 100u32;

        // 仅创建者有内建权限
        assert!(policy.is_authorized(&creator, &creator, &now));
        assert!(!policy.is_authorized(&2u64, &creator, &now));
    }

    #[test]
    fn access_policy_non_timeboxed_never_expired() {
        let policy1: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::OwnerOnly;
        let users: BoundedVec<u64, MaxUsers> = vec![2u64].try_into().unwrap();
        let policy2: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::SharedWith(users);
        let policy3: AccessPolicy<u64, u32, MaxUsers> = AccessPolicy::GovernanceControlled;

        assert!(!policy1.is_expired(&u32::MAX));
        assert!(!policy2.is_expired(&u32::MAX));
        assert!(!policy3.is_expired(&u32::MAX));
    }

    // --- AccessPolicy SCALE encode/decode ---

    #[test]
    fn access_policy_scale_roundtrip() {
        let users: BoundedVec<u64, MaxUsers> = vec![2u64, 3u64].try_into().unwrap();
        let policies: Vec<AccessPolicy<u64, u32, MaxUsers>> = vec![
            AccessPolicy::OwnerOnly,
            AccessPolicy::SharedWith(users.clone()),
            AccessPolicy::TimeboxedAccess {
                users,
                expires_at: 999u32,
            },
            AccessPolicy::GovernanceControlled,
            AccessPolicy::RoleBased(b"mod".to_vec().try_into().unwrap()),
        ];

        for policy in policies {
            let encoded = policy.encode();
            let decoded = AccessPolicy::<u64, u32, MaxUsers>::decode(&mut &encoded[..]).unwrap();
            assert_eq!(policy, decoded);
        }
    }

    // --- validate_cid ---

    #[test]
    fn validate_cid_rejects_empty() {
        assert!(!validate_cid(b""));
    }

    #[test]
    fn validate_cid_accepts_valid() {
        assert!(validate_cid(
            b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"
        ));
        assert!(validate_cid(
            b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        ));
    }

    #[test]
    fn validate_cid_rejects_too_long() {
        let long_cid = [b'Q'; 129];
        assert!(!validate_cid(&long_cid));
    }

    #[test]
    fn validate_cid_rejects_control_chars() {
        assert!(!validate_cid(b"Qm\x00abc"));
        assert!(!validate_cid(b"Qm\x1fabc"));
    }

    #[test]
    fn validate_cid_rejects_high_bytes() {
        assert!(!validate_cid(&[0x80, 0x81]));
    }

    // --- EncryptionMethod SCALE decode from raw u8 ---

    #[test]
    fn encryption_method_decode_from_raw_u8_bytes() {
        // 验证从原始 u8 编码的字节可以正确解码为 EncryptionMethod
        let raw_0 = 0u8.encode();
        let raw_1 = 1u8.encode();
        let raw_2 = 2u8.encode();
        let raw_3 = 3u8.encode();

        assert_eq!(
            EncryptionMethod::decode(&mut &raw_0[..]).unwrap(),
            EncryptionMethod::None
        );
        assert_eq!(
            EncryptionMethod::decode(&mut &raw_1[..]).unwrap(),
            EncryptionMethod::Aes256Gcm
        );
        assert_eq!(
            EncryptionMethod::decode(&mut &raw_2[..]).unwrap(),
            EncryptionMethod::ChaCha20Poly1305
        );
        assert_eq!(
            EncryptionMethod::decode(&mut &raw_3[..]).unwrap(),
            EncryptionMethod::XChaCha20Poly1305
        );
    }

    // --- KeyType SCALE decode from raw u8 ---

    #[test]
    fn key_type_decode_from_raw_u8_bytes() {
        // 验证从原始 u8 编码的字节可以正确解码为 KeyType（#[codec(index)] 保证）
        let raw_1 = 1u8.encode();
        let raw_2 = 2u8.encode();
        let raw_3 = 3u8.encode();

        assert_eq!(KeyType::decode(&mut &raw_1[..]).unwrap(), KeyType::Rsa2048);
        assert_eq!(KeyType::decode(&mut &raw_2[..]).unwrap(), KeyType::Ed25519);
        assert_eq!(
            KeyType::decode(&mut &raw_3[..]).unwrap(),
            KeyType::EcdsaP256
        );
    }

    #[test]
    fn key_type_decode_from_zero_fails() {
        // KeyType 无 index=0 的变体，解码 0 应失败
        let raw_0 = 0u8.encode();
        assert!(KeyType::decode(&mut &raw_0[..]).is_err());
    }
}
