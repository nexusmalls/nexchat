//! Runtime API for `pallet-chat-group` (MLS DS/AS read surface).
//! `pallet-chat-group` 的 Runtime API（MLS DS/AS 只读接口）。
//!
//! EN: Read-only queries that let MLS clients sync without mutating chain state.
//! In particular `pending_welcome` lets a newly added member fetch their Welcome
//! blob BEFORE calling the `claim_welcome` extrinsic (which deletes it), closing
//! the "claim-then-read loses the Welcome" foot-gun. All methods are free (run
//! via `runtime_api`, never as transactions). CN: 供 MLS 客户端在不改链状态下
//! 同步的只读查询。尤其 `pending_welcome` 让新成员在调用 `claim_welcome`
//! extrinsic（会删除 Welcome）**之前**先取回 Welcome 字节，规避「先 claim 后读
//! 丢信」的陷阱。所有方法免费（走 `runtime_api`，非交易）。

use codec::{Codec, Decode, Encode};
use scale_info::TypeInfo;
use sp_std::vec::Vec;

/// EN: Plain (SCALE-encodable) snapshot of a group's on-chain MLS anchor, with
/// the governance `frozen` flag folded in. Carries NO key material — the chain
/// never holds secrets. CN: 群链上 MLS 锚点的扁平（可 SCALE 编码）快照，并合入治理
/// `frozen` 标记。不含任何密钥材料——链绝不持有机密。
#[derive(Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Debug)]
pub struct GroupMlsSnapshot {
    /// EN: monotonic epoch / CN: 单调 epoch
    pub epoch: u64,
    /// EN: current ratchet-tree hash / CN: 当前棘轮树哈希
    pub tree_hash: [u8; 32],
    /// EN: confirmed transcript hash anchor / CN: confirmed transcript 哈希锚
    pub confirmed_transcript_hash: [u8; 32],
    /// EN: external GroupInfo IPFS CID bytes / CN: 外部 GroupInfo 的 IPFS CID 字节
    pub group_info_cid: Vec<u8>,
    /// EN: member count / CN: 成员数
    pub member_count: u32,
    /// EN: ciphersuite tag / CN: 套件标识
    pub cipher_suite: u16,
    /// EN: public vs private group / CN: 公开群 / 私群
    pub is_public: bool,
    /// EN: frozen by governance (or mid-teardown) / CN: 被治理冻结（或拆除中）
    pub frozen: bool,
}

sp_api::decl_runtime_apis! {
    /// EN: Read-only MLS group queries (Welcome mailbox, handshake log, MLS
    /// anchor snapshot, frozen flag). CN: MLS 群只读查询（Welcome 信箱、握手日志、
    /// MLS 锚点快照、冻结标记）。
    pub trait ChatGroupApi<AccountId>
    where
        AccountId: Codec,
    {
        /// EN: Pending Welcome bytes for `who` in `group_id`, or `None` if absent.
        /// Read this BEFORE the `claim_welcome` extrinsic (which deletes it).
        /// CN: `who` 在 `group_id` 的待领 Welcome 字节；无则 `None`。应在
        /// `claim_welcome` extrinsic（会删除它）**之前**读取。
        fn pending_welcome(group_id: u64, who: AccountId) -> Option<Vec<u8>>;

        /// EN: Opaque Commit blob logged at `epoch` (lets offline members catch
        /// up), or `None`. CN: `epoch` 处记录的不透明 Commit 字节（供离线成员补齐）；
        /// 无则 `None`。
        fn handshake_at_epoch(group_id: u64, epoch: u64) -> Option<Vec<u8>>;

        /// EN: Current MLS anchor snapshot for `group_id`, or `None` if the group
        /// does not exist. CN: `group_id` 的当前 MLS 锚点快照；群不存在则 `None`。
        fn group_mls_snapshot(group_id: u64) -> Option<GroupMlsSnapshot>;

        /// EN: Whether `group_id` exists. CN: `group_id` 是否存在。
        fn group_exists(group_id: u64) -> bool;

        /// EN: Whether `group_id` is frozen (governance or mid-teardown).
        /// CN: `group_id` 是否被冻结（治理或拆除中）。
        fn is_group_frozen(group_id: u64) -> bool;
    }
}
