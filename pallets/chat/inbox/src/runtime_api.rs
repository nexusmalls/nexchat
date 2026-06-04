//! Runtime API for `pallet-chat-inbox`.
//! `pallet-chat-inbox` 的 Runtime API。
//!
//! EN: Read-only surface consumed by off-chain relays to verify blinded delivery
//! tokens against current chain state (epoch freshness + targeted revocation),
//! without contacting the receiver. CN: 供链下 relay 消费的只读接口，用于对照当前
//! 链状态校验盲化投递令牌（纪元新鲜度 + 定向撤销），无需联系接收方。

use crate::types::{ContactTag, InboxId};

sp_api::decl_runtime_apis! {
    /// EN: Off-chain delivery inbox registry queries. CN: 链下投递信箱注册表查询。
    pub trait ChatInboxApi {
        /// EN: Current inbox-keyed revocation epoch, or `None` if the inbox is not
        /// registered. A token is fresh iff its embedded epoch equals this value.
        /// CN: 当前 inbox 维度撤销纪元；信箱未注册则为 `None`。令牌新鲜当且仅当其内嵌
        /// 纪元等于此值。
        fn inbox_epoch(inbox_id: InboxId) -> Option<u32>;

        /// EN: Whether `tag` is currently revoked for `inbox_id` (targeted
        /// revocation). Unregistered inbox returns `false`. CN: `tag` 当前是否在
        /// `inbox_id` 下被撤销（定向撤销）。未注册信箱返回 `false`。
        fn is_tag_revoked(inbox_id: InboxId, tag: ContactTag) -> bool;

        /// EN: Whether `inbox_id` is registered. CN: `inbox_id` 是否已注册。
        fn inbox_exists(inbox_id: InboxId) -> bool;
    }
}
