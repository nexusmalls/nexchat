//! Types & frozen constants for `pallet-msg-identity`.
//! `pallet-msg-identity` 的类型定义与冻结常量。

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// EN: Per-device identifier. Self-certifying: `DeviceId = blake2_128(ik_x25519_pub)`,
/// so a device id is bound one-to-one to its X25519 identity DH key and needs no
/// separate registration. CN: 设备标识。自证：`DeviceId = blake2_128(ik_x25519_pub)`，
/// 故设备 id 与其 X25519 身份 DH 钥一一绑定，无需额外注册。
pub type DeviceId = [u8; 16];

/// EN: A Curve25519 (X25519) public key used as an X3DH DH target (IK / SPK / OPK leaf).
/// CN: 用作 X3DH DH 目标的 Curve25519（X25519）公钥（IK / SPK / OPK 叶）。
pub type X25519Pub = [u8; 32];

/// EN: Account-key endorsement signature over a published DH public key. The chain
/// stores it **opaque** for off-chain *relay-trustless* verification (peers verify the
/// account's sr25519 signature over the key); on-chain publication is authorized by the
/// signed origin (the account itself). See module docs §"Endorsement boundary".
/// CN: 账户钥对所发布 DH 公钥的背书签名。链上**不透明**存储，供链下 *relay-trustless*
/// 校验（对端验证账户 sr25519 钥对该公钥的签名）；链上发布由签名 origin（账户本身）授权。
/// 见模块文档「背书边界」。
pub type Endorsement = [u8; 64];

/// EN: Merkle root over a device's published one-time-prekey (OPK) public-key set.
/// CN: 设备已发布一次性预密钥（OPK）公钥集合的 Merkle 根。
pub type MerkleRoot = [u8; 32];

/// EN: `ChatStackCaps.flags` bit — client supports the X3DH+Double-Ratchet 1:1 stack.
/// CN: `ChatStackCaps.flags` 位——客户端支持 X3DH+双棘轮 1:1 栈。
pub const STACK_DR: u8 = 0b0000_0001;
/// EN: `ChatStackCaps.flags` bit — client supports the pairwise-MLS-Wire 1:1 stack.
/// CN: `ChatStackCaps.flags` 位——客户端支持 pairwise MLS Wire 1:1 栈。
pub const STACK_MLS_WIRE: u8 = 0b0000_0010;

/// EN: Domain-separation context for the account-key endorsement of an IK (frozen; see
/// design §17.2). Off-chain verifiers MUST sign/verify `CTX_IK_ENDORSE ‖ ik`.
/// CN: IK 账户钥背书的域分隔上下文（冻结；见设计 §17.2）。链下验证方必须对
/// `CTX_IK_ENDORSE ‖ ik` 签名/验签。
pub const CTX_IK_ENDORSE: &[u8] = b"nexchat/x3dh/ik-endorse/v1";
/// EN: Domain-separation context for the account-key endorsement of an SPK (frozen).
/// CN: SPK 账户钥背书的域分隔上下文（冻结）。
pub const CTX_SPK_ENDORSE: &[u8] = b"nexchat/x3dh/spk-endorse/v1";

/// EN: On-chain device identity anchor: the long-term X25519 identity DH key for one
/// device, its account-key endorsement, and a device-level revocation epoch. Holds the
/// reserved anti-spam deposit so it can be refunded on unregister.
/// CN: 链上设备身份锚：单设备的长期 X25519 身份 DH 钥、其账户钥背书，以及设备级撤销纪元。
/// 持有预留的反垃圾押金，注销时退还。
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct DeviceIdentity<Balance, BlockNumber> {
    /// EN: Long-term X25519 identity DH public key. CN: 长期 X25519 身份 DH 公钥。
    pub ik: X25519Pub,
    /// EN: Account-key endorsement over `CTX_IK_ENDORSE ‖ ik` (opaque on-chain).
    /// CN: 账户钥对 `CTX_IK_ENDORSE ‖ ik` 的背书（链上不透明）。
    pub ik_endorsement: Endorsement,
    /// EN: Device-level revocation epoch; bumping it invalidates this device's
    /// previously published prekey bundle. CN: 设备级撤销纪元；递增即作废该设备此前
    /// 发布的预密钥包。
    pub prekey_epoch: u32,
    /// EN: Deposit reserved at registration, returned on unregister. CN: 注册时预留、
    /// 注销时退还的押金。
    pub deposit: Balance,
    /// EN: Block at which the device was registered. CN: 设备注册区块高度。
    pub registered_at: BlockNumber,
}

/// EN: Mid-term signed prekey (SPK) for a device. Acts as the OPK-exhaustion fallback
/// DH target. CN: 设备的中期签名预密钥（SPK）。作为 OPK 耗尽时的回退 DH 目标。
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct SignedPreKey<BlockNumber> {
    /// EN: X25519 SPK public key. CN: X25519 SPK 公钥。
    pub spk: X25519Pub,
    /// EN: Account-key endorsement over `CTX_SPK_ENDORSE ‖ spk` (opaque on-chain).
    /// CN: 账户钥对 `CTX_SPK_ENDORSE ‖ spk` 的背书（链上不透明）。
    pub spk_endorsement: Endorsement,
    /// EN: Advisory expiry block (0 = unset). Clients rotate before it.
    /// CN: 建议过期区块（0 = 未设）。客户端在此之前轮换。
    pub valid_until: BlockNumber,
    /// EN: Last update block (last-writer-wins ordering hint). CN: 最近更新区块（LWW 排序提示）。
    pub updated_at: BlockNumber,
}

/// EN: A device's one-time-prekey (OPK) set anchor: only the Merkle root, remaining
/// count, and a publication epoch are stored — leaves are distributed off-chain (relay).
/// CN: 设备一次性预密钥（OPK）集合锚：仅存 Merkle 根、剩余数量与发布纪元——叶子经链下
/// （relay）分发。
#[derive(Clone, Copy, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct OpkRoot {
    /// EN: Merkle root over the published OPK public-key set. CN: 已发布 OPK 公钥集合的 Merkle 根。
    pub root: MerkleRoot,
    /// EN: Number of OPKs published under this root (advisory replenish trigger).
    /// CN: 此根下发布的 OPK 数量（补货触发参考）。
    pub count: u32,
    /// EN: Monotonic publication epoch (bumped on every root update).
    /// CN: 单调发布纪元（每次更新根递增）。
    pub epoch: u32,
}

/// EN: Per-account 1:1 chat-stack capability advertisement, read by an initiator to pick
/// DR or fall back to MLS-Wire (design §20). CN: 每账户的 1:1 聊天栈能力公告，发起方据此
/// 选择 DR 或回退 MLS-Wire（设计 §20）。
#[derive(Clone, Copy, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
pub struct StackCaps {
    /// EN: Bitflags: `STACK_DR` | `STACK_MLS_WIRE`. CN: 位标志：`STACK_DR` | `STACK_MLS_WIRE`。
    pub flags: u8,
    /// EN: Client protocol version (handshake-redundant tiebreaker). CN: 客户端协议版本。
    pub version: u16,
}
