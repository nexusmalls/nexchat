//! Runtime API for `pallet-chat-sync`.
//! `pallet-chat-sync` 的 Runtime API。
//!
//! EN: Read-only surface for the client recovery path: a fresh device recomputes its
//! `anchor_id` from the mnemonic and fetches the encrypted SyncManifest in one query.
//! CN: 客户端恢复路径的只读接口：新设备凭助记词重算 `anchor_id`，一次查询取回加密
//! SyncManifest。

use crate::types::AnchorId;
use sp_std::vec::Vec;

sp_api::decl_runtime_apis! {
    /// EN: Encrypted sync anchor queries. CN: 加密同步锚查询。
    pub trait ChatSyncApi {
        /// EN: `(updated_at, ciphertext)` stored at `anchor_id`, or `None` if absent.
        /// The ciphertext is opaque — only the deriving client can decrypt it.
        /// CN: `anchor_id` 处存储的 `(updated_at, 密文)`；不存在则为 `None`。密文不透明，
        /// 仅派生它的客户端可解密。
        fn sync_anchor(anchor_id: AnchorId) -> Option<(u64, Vec<u8>)>;
    }
}
