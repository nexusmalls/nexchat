//! Types for `pallet-chat-inbox`.
//! `pallet-chat-inbox` 的类型定义。

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::*;
use scale_info::TypeInfo;

/// EN: Off-chain delivery inbox identifier. Opaque 32-byte handle chosen by the
/// client and **bound off-chain** to the inbox's Blind-RSA issuance public key
/// (`inbox_id = H(IPK ‖ salt)`). The chain treats it as opaque bytes and never
/// stores `IPK` itself, so the inbox stays unlinkable to any account beyond the
/// throwaway controller that registered it.
/// CN: 链下投递信箱标识符。由客户端选定的 32 字节不透明句柄，并在**链下**绑定到该
/// 信箱的 Blind-RSA 签发公钥（`inbox_id = H(IPK ‖ salt)`）。链上仅视其为不透明字节、
/// 从不存储 `IPK` 本身，故除注册它的一次性控制账户外，信箱对任何账户不可关联。
pub type InboxId = [u8; 32];

/// EN: Per-contact revocation tag (`ct_c` in the design doc). A 32-byte random
/// value Bob assigns to one contact and binds into every token he blind-signs
/// for them. Adding it to an inbox's revoked set lets a relay reject that single
/// contact without rotating the whole epoch.
/// CN: 每联系人撤销标签（设计文档中的 `ct_c`）。Bob 为某联系人分配的 32 字节随机值，
/// 并绑入为其盲签的每个令牌。把它加入信箱撤销集即可让 relay 拒绝该联系人，而无需轮换
/// 整个 epoch。
pub type ContactTag = [u8; 32];

/// EN: On-chain record of one delivery inbox. Holds only the data a relay needs
/// to verify a blinded delivery token *offline from chain state*: the current
/// revocation `epoch` and the set of `revoked_tags`. The Blind-RSA public key is
/// **not** stored here (carried by the sender, self-authenticated via
/// `inbox_id = H(IPK)`), keeping the registry account-unlinkable.
/// CN: 单个投递信箱的链上记录。仅保存 relay **离线**验证盲化投递令牌所需的数据：
/// 当前撤销 `epoch` 与 `revoked_tags` 集合。Blind-RSA 公钥**不**存于此（由发送方携带、
/// 经 `inbox_id = H(IPK)` 自验证），以保持注册表的账户不可关联性。
#[derive(
    CloneNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo, MaxEncodedLen, DebugNoBound,
)]
#[scale_info(skip_type_params(MaxRevokedTags))]
pub struct InboxRecord<
    AccountId: Clone + PartialEq + Eq + core::fmt::Debug,
    Balance: Clone + PartialEq + Eq + core::fmt::Debug,
    BlockNumber: Clone + PartialEq + Eq + core::fmt::Debug,
    MaxRevokedTags: Get<u32>,
> {
    /// EN: Account authorized to mutate/deregister this inbox (and that paid the
    /// deposit). Should be a throwaway key unrelated to the owner's main account.
    /// CN: 被授权修改/注销此信箱并支付押金的账户。应为与拥有者主账户无关的一次性密钥。
    pub controller: AccountId,
    /// EN: Inbox-keyed revocation epoch. A token is fresh iff its embedded epoch
    /// equals this value; bumping it invalidates every previously issued token.
    /// CN: inbox 维度撤销纪元。令牌新鲜当且仅当其内嵌 epoch 等于此值；递增即作废此前
    /// 签发的所有令牌。
    pub epoch: u32,
    /// EN: Tags of contacts revoked within the current epoch (targeted revocation).
    /// Cleared on every epoch bump. CN: 当前 epoch 内被撤销的联系人标签（定向撤销）。
    /// 每次 epoch 递增时清空。
    pub revoked_tags: BoundedVec<ContactTag, MaxRevokedTags>,
    /// EN: Deposit reserved at registration, returned on deregister. CN: 注册时预留、
    /// 注销时退还的押金。
    pub deposit: Balance,
    /// EN: Block at which the inbox was registered. CN: 信箱注册的区块高度。
    pub created_at: BlockNumber,
}
