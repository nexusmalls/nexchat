//! Runtime API for `pallet-msg-identity`.
//! `pallet-msg-identity` 的 Runtime API。
//!
//! EN: Read-only surface consumed by off-chain relays / clients to fetch a peer's
//! X3DH prekey anchors (IK / SPK / OPK root) and 1:1 stack capabilities, so an
//! initiator can run X3DH and pick the right 1:1 stack (design §6 / §20).
//! CN: 供链下 relay / 客户端消费的只读接口，用于取对端 X3DH 预密钥锚（IK / SPK / OPK 根）
//! 与 1:1 栈能力，使发起方能运行 X3DH 并选择正确的 1:1 栈（设计 §6 / §20）。

use crate::types::{DeviceId, MerkleRoot, X25519Pub};
use codec::Codec;

sp_api::decl_runtime_apis! {
    /// EN: Messaging identity prekey-anchor queries. CN: 消息身份预密钥锚查询。
    pub trait MsgIdentityApi<AccountId, BlockNumber>
    where
        AccountId: Codec,
        BlockNumber: Codec,
    {
        /// EN: A device's IK + current prekey epoch, or `None`. CN: 设备 IK + 当前预密钥纪元。
        fn device_ik(account: AccountId, device_id: DeviceId) -> Option<(X25519Pub, u32)>;

        /// EN: A device's SPK + advisory expiry, or `None`. CN: 设备 SPK + 建议过期。
        fn device_spk(account: AccountId, device_id: DeviceId) -> Option<(X25519Pub, BlockNumber)>;

        /// EN: A device's OPK root `(root, count, epoch)`, or `None`. CN: 设备 OPK 根。
        fn device_opk_root(
            account: AccountId,
            device_id: DeviceId,
        ) -> Option<(MerkleRoot, u32, u32)>;

        /// EN: An account's 1:1 stack capabilities `(flags, version)`, or `None`.
        /// CN: 账户 1:1 栈能力 `(flags, version)`。
        fn stack_caps(account: AccountId) -> Option<(u8, u16)>;

        /// EN: Whether `(account, device_id)` is registered. CN: 设备是否已注册。
        fn device_exists(account: AccountId, device_id: DeviceId) -> bool;
    }
}
