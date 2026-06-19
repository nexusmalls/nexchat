# Delete & recall (off-chain vs on-chain)

EN: How NexChat hides or removes chat content. **Human messages** (MLS + relay + IPFS) use the
**off-chain** paths below. **`pallet-chat-core`** also exposes `delete_message` / `recall_message`
extrinsics — those apply only to **on-chain System notices**, not to encrypted human payloads.

CN: NexChat 如何隐藏或删除聊天内容。**人类消息**（MLS + relay + IPFS）走下方**链下**路径。
**`pallet-chat-core`** 另有 `delete_message` / `recall_message` extrinsic——仅作用于**链上 System
通知**，不作用于加密的人类消息正文。

---

## Quick matrix

| Operation | Scope | Peer affected? | Cross-device (your account) | Chain extrinsic |
|-----------|--------|--------------|----------------------------|-----------------|
| **Delete one** | Local hide | No | Yes — msg-archive **tombstone** (when archive + IPFS on) | — |
| **Clear conversation** | Local hide all | No | Yes — tombstones | — |
| **Recall** | Both sides placeholder | Yes — `type=recall` MLS control envelope | Yes — `recalled` row in archive | — |
| **Ephemeral burn** | Local purge | No | No — never archived | — |
| **`Chat::delete_message`** | Per-party soft delete | N/A (System msgs) | Via chain indexers | System notices only |
| **`Chat::recall_message`** | Sender, time window | Both parties on-chain flag | Via chain indexers | System notices only |

Do **not** wire human-message UI to `Chat::delete_message` / `Chat::recall_message` unless the
payload is an on-chain System row with a chain `msg_id`.

---

## Off-chain delete (local + tombstone)

**UI:** bubble 🗑 or header 🗑 (clear thread).

**Client path:** `appStore.deleteMessage` / `clearConversationHistory` → `localStore` →
`scheduleOffchainSync` → `msgArchiveSync.push` → `tombstonesForRemovedMessages` → encrypted IPFS
blob + relay pointer.

**Semantics:**

- Hides the message on **your** devices only.
- Does **not** remove ciphertext from the peer's device or the relay mailbox.
- Tombstones are **terminal** in archive merge (see `mergeMsgEntries` in `store/msgArchive.ts`).

**Code:** `state/appStore.ts`, `store/msgArchive.ts`, `store/msgArchiveSync.ts`, `ui/ChatWindow.tsx`.

---

## Off-chain recall (two-sided placeholder)

**UI:** bubble ↺ within **2 minutes** (`RECALL_WINDOW_MS`), sender only (`canRecallMessage`).

**Client path:**

1. Sender: `recallEnvelope` → MLS encrypt → `relayClient.send` → `markMessageRecalled` locally.
2. Receiver: inbound decrypt → `applyRecallEnvelope` (`p3/recall.ts`) → blank content, `status: recalled`.
3. Both: `scheduleOffchainSync` — archive keeps a **blank** `recalled` row (not a tombstone).

**Semantics:**

- Both sides see 「消息已撤回」; original plaintext is cleared locally and in archive.
- Does **not** revoke IPFS media the peer already downloaded.
- Does **not** delete relay mailbox entries.

**Tests:** `p3/recall.test.ts`, `p3/recallFlow.test.ts`.

---

## On-chain pallet (System notices only)

`pallets/chat/core`:

| Extrinsic | Who | Effect |
|-----------|-----|--------|
| `delete_message(msg_id)` | Sender **or** receiver | **Unilateral** soft delete for that party |
| `recall_message(msg_id)` | Sender only, within `MessageRecallWindow` | **Both sides** hidden; metadata kept on-chain |

NexChat **does not** call these for MLS human messages today (`chain/chainClient.ts` has no
`delete_message` / `recall_message` for chat payloads). Indexers or future System-notice UI may
use them.

**Docs:** `pallets/chat/core/README.md`, `p3/recall.ts` module header.

---

## Related

- Privacy copy in app: **Settings → Privacy** (`privacyDeleteRecallNote`).
- Reply quotes when target deleted/recalled: `ui/messagePreview.ts` → `replyQuotePreview`.
- Intentional dual paths (Track A vs Wire): [`INTENTIONAL_DUAL_PATHS.md`](INTENTIONAL_DUAL_PATHS.md).
