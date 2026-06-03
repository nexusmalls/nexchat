//! # CID验证和媒体验证工具 / CID & media validation utilities
//!
//! 本模块提供IPFS CID验证和媒体内容验证功能。
//!
//! > **重要（审计 C / chat-core × MLS 收敛）**：本模块中与“加密”相关的判定
//! > （`is_cid_encrypted`、`validate_encrypted_cid`、`CidValidationResult::ValidEncrypted`）
//! > 只是基于长度/前缀的**可绕过启发式**，**不能**作为内容已加密的证明。链不应据此
//! > 声称“已验证加密”。`pallet-chat-core` 已移除该启发式，仅做非空 + 长度 sanity，
//! > 加密由客户端 MLS E2EE 单独保证。这些函数当前未被任何在用 pallet 调用，保留仅作
//! > **格式探测**用途；请勿将其重新用作加密闸门。
//! >
//! > IMPORTANT (audit C): the "encryption" helpers below are bypassable
//! > length/prefix heuristics and are NOT proof of encryption. Do not use them
//! > as an encryption gate; encryption is guaranteed solely by client-side MLS E2EE.

/// 函数级详细中文注释：CID验证结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CidValidationResult {
    /// 有效且已加密
    ValidEncrypted,
    /// 有效但未加密
    ValidUnencrypted,
    /// 无效CID（太短）
    TooShort,
    /// 无效CID（太长）
    TooLong,
    /// 无效CID（空）
    Empty,
}

impl CidValidationResult {
    /// 函数级中文注释：检查是否有效
    pub fn is_valid(&self) -> bool {
        matches!(self, CidValidationResult::ValidEncrypted | CidValidationResult::ValidUnencrypted)
    }

    /// 函数级中文注释：检查是否已加密
    pub fn is_encrypted(&self) -> bool {
        matches!(self, CidValidationResult::ValidEncrypted)
    }
}

/// 函数级详细中文注释：基于长度/前缀猜测 CID 是否“像”加密内容（**启发式，非证明**）。
///
/// > **审计 C**：此判定可被随手构造的长字节绕过，未加密 CIDv1 也会被误判为“已加密”，
/// > **不得**用作加密闸门。仅保留为格式探测；加密由客户端 MLS E2EE 保证。
///
/// # 规则
/// - 标准CIDv0（46字节，Qm开头）：返回 false
/// - CIDv1或长度>50字节：返回 true
pub fn is_cid_encrypted(cid: &[u8]) -> bool {
    if cid.len() < 46 {
        return false; // 太短，无效CID
    }

    // 标准CIDv0以"Qm"开头，长度46字节
    if cid.len() == 46 && cid.starts_with(b"Qm") {
        return false; // 未加密的CIDv0
    }

    // 其他情况（CIDv1或加密后的CID）
    true
}

/// 函数级详细中文注释：验证CID格式
///
/// # 参数
/// - `cid`: IPFS CID字节数组
/// - `max_len`: 最大允许长度
///
/// # 返回
/// - `CidValidationResult`: 验证结果
pub fn validate_cid(cid: &[u8], max_len: usize) -> CidValidationResult {
    if cid.is_empty() {
        return CidValidationResult::Empty;
    }

    if cid.len() > max_len {
        return CidValidationResult::TooLong;
    }

    if cid.len() < 46 {
        return CidValidationResult::TooShort;
    }

    if is_cid_encrypted(cid) {
        CidValidationResult::ValidEncrypted
    } else {
        CidValidationResult::ValidUnencrypted
    }
}

/// 函数级详细中文注释：在 `validate_cid` 基础上额外要求“看起来已加密”（**启发式，非证明**）。
///
/// > **审计 C**：不要把它当成加密保证或链上加密闸门（见模块顶部说明）。
/// > `pallet-chat-core` 已不再使用此函数；如需链上校验，请只用非空 + 长度 sanity。
pub fn validate_encrypted_cid(cid: &[u8], max_len: usize) -> Result<(), &'static str> {
    match validate_cid(cid, max_len) {
        CidValidationResult::ValidEncrypted => Ok(()),
        CidValidationResult::ValidUnencrypted => Err("CID must be encrypted"),
        CidValidationResult::TooShort => Err("CID too short"),
        CidValidationResult::TooLong => Err("CID too long"),
        CidValidationResult::Empty => Err("CID cannot be empty"),
    }
}

/// 函数级详细中文注释：检查CID是否为有效的CIDv0
///
/// CIDv0特征：
/// - 长度为46字节
/// - 以"Qm"开头（Base58编码）
///
/// # 参数
/// - `cid`: IPFS CID字节数组
///
/// # 返回
/// - `true`: 是有效的CIDv0
/// - `false`: 不是CIDv0
pub fn is_valid_cid_v0(cid: &[u8]) -> bool {
    cid.len() == 46 && cid.starts_with(b"Qm")
}

/// 函数级详细中文注释：检查CID是否为有效的CIDv1
///
/// CIDv1特征：
/// - 以"b"开头（Base32编码）或其他多基编码前缀
/// - 长度通常>46字节
///
/// # 参数
/// - `cid`: IPFS CID字节数组
///
/// # 返回
/// - `true`: 可能是CIDv1
/// - `false`: 不是CIDv1
pub fn is_valid_cid_v1(cid: &[u8]) -> bool {
    if cid.len() < 50 {
        return false;
    }
    // CIDv1 Base32编码以'b'开头
    // CIDv1 Base58编码以'z'开头
    // CIDv1 Base64编码以'm'开头
    cid.starts_with(b"b") || cid.starts_with(b"z") || cid.starts_with(b"m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_cid_encrypted() {
        // 标准CIDv0（未加密）
        let cid_v0 = b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert!(!is_cid_encrypted(cid_v0));

        // 加密后的CID（长度>50）
        let encrypted_cid = b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi1234";
        assert!(is_cid_encrypted(encrypted_cid));

        // 太短的CID
        let short_cid = b"Qm123";
        assert!(!is_cid_encrypted(short_cid));
    }

    #[test]
    fn test_validate_cid() {
        let cid_v0 = b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert_eq!(validate_cid(cid_v0, 100), CidValidationResult::ValidUnencrypted);

        let encrypted = b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi1234";
        assert_eq!(validate_cid(encrypted, 100), CidValidationResult::ValidEncrypted);

        assert_eq!(validate_cid(b"", 100), CidValidationResult::Empty);
        assert_eq!(validate_cid(b"short", 100), CidValidationResult::TooShort);
    }

    #[test]
    fn test_validate_encrypted_cid() {
        let encrypted = b"bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi1234";
        assert!(validate_encrypted_cid(encrypted, 100).is_ok());

        let cid_v0 = b"QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert!(validate_encrypted_cid(cid_v0, 100).is_err());
    }
}
