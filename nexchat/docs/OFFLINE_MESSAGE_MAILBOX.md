# NexChat Offline Message Mailbox / 离线消息邮箱

> **Note / 注:** Production relay is Rust **`relay-rs`** (`relay-rs/server` + `relay-rs/core`).
> Historical `.mjs` module names below refer to the pre-Rust layout; behavior lives in relay-rs now.
> / 生产 relay 为 Rust **`relay-rs`**。下文历史 `.mjs` 模块名指 Rust 重写前布局；行为现已在 relay-rs 中实现。
>
> **Status:** Design approved for implementation (2026-06-13)  
> **Priority:** P0 — mandatory for production IM  
> **Scope:** `nexchat/` relay + client (Phase 1–2); `node/` gossipsub mailbox deferred to Phase 4

---

## 1. Problem / 问题

Today relay **only forwards chat frames to live WebSocket connections**. If the recipient is offline, the frame is dropped. The sender sees “sent” locally; the recipient never receives the message after coming online.

当前 relay **只向在线 WebSocket 连接转发聊天帧**。接收方离线时帧被丢弃；发送方本地显示已发送，对方上线后**无法补收**。

**Already works offline (for reference):**

| Channel | Offline store | Replay trigger |
|---------|---------------|----------------|
| `contact_req` / `contact_ack` | ✅ 30d mailbox | `contact_fetch` on unlock |
| MLS control `kp` / `welcome` / `commit` | ✅ 7d `mlsMailbox` | `flushMlsMailbox` on `register_account` |
| **Chat ciphertext frames** | ❌ none | — |

---

## 2. Goal / 目标

| Requirement | Detail |
|-------------|--------|
| **Store-and-forward** | Relay persists **ciphertext only** (E2EE unchanged); recipient gets frames after reconnect |
| **1:1 + group** | Same mechanism for `d:` direct and `g:` group convs |
| **Multi-device** | Same account on phone + desktop: both can receive; client dedup prevents double UI |
| **Retention** | Default **180 days** per account mailbox (align `CHAT_GROUP_MLS_DESIGN.md` §11); configurable cap |
| **No chain hot path** | Messages stay off-chain; chain still handles MLS epoch / KeyPackage only |
| **VPS relay first** | Ship on `relay-rs` before `node/` §13 gossipsub |

---

## 3. Architecture / 架构

Reuse the **contact mailbox pattern** (`contact_fetch` / `contact_consume` via `relayOneShot*` on the client; `relay-rs/server` on the relay).

```text
Sender client                Relay                         Recipient client
     │ encrypt (MLS)            │                                │
     │── RelayFrame ──────────►│ storeChatFrame(recipients[])   │
     │                           │ deliverToAccount (if online)   │── realtime
     │                           │                                │
     │                           │         [offline …]            │
     │                           │                                │
     │                           │◄── register_account ───────────│ reconnect
     │                           │ flushChatMailbox(account)      │── burst frames
     │                           │                                │ decrypt → localStore
```

**Trust model:** relay sees `convId`, `dedupKey`, ciphertext, optional delivery metadata — **never plaintext**. For 1:1 RFC 9474 delivery admission, the frame may also carry plaintext `delivery.mlsKey` (pairwise routing key) and the sender is bound to the authenticated WebSocket session — **this is anti-abuse admission, not Signal-style metadata privacy**. Malicious relay can delay/drop/censor; cannot forge MLS content.

---

## 4. Relay design / Relay 设计

### 4.1 New module: `scripts/relay-chat-mailbox.mjs`

Shared helpers (mirror `relay-contact-mailbox.mjs`):

```javascript
// Constants (env-overridable)
CHAT_MAILBOX_TTL_MS      = 180 * 24 * 60 * 60 * 1000  // 180d
CHAT_MAILBOX_MAX_FRAMES  = 5000                        // per account
CHAT_MAILBOX_MAX_BYTES   = 256 * 1024 * 1024             // 256 MiB per account

chatBox(chatMailbox, account)           // get-or-create Map<dedupKey, row>
pruneChatBox(box, ttlMs, now)           // drop expired (expiresAt + stored_at TTL)
enforceChatCap(box)                     // LRU trim by stored_at when over max frames/bytes
storeChatFrame(chatMailbox, account, frameRow)
flushChatMailbox(chatMailbox, account, deliverFn)
```

**Row shape:**

```javascript
{
  dedupKey, convId, senderRef, ciphertextB64,
  expiresAt?, delivery?, routeTo?,
  stored_at: Date.now(),
  bytes: wire.length,
}
```

**Dedup key:** `frame.dedupKey` (required stable id from sender). Fallback: `sha256(convId + ciphertextB64)` for legacy senders.

### 4.2 Hook points in `relay-rs/server`

| Hook | Module (historical `.mjs` name) | Rust location |
|------|----------------------------------|---------------|
| Chat store + flush | `relay-chat-mailbox.mjs` | `relay-rs/server/src/mailbox.rs`, `protocol.rs` |
| Contact / invite mailbox | `relay-contact-mailbox.mjs` | `protocol.rs` |
| Persistence | `relay-persistence.mjs` | `relay-rs/core/src/persistence.rs` |

**On chat frame ingress** (after `verifyDeliveryFrame`, before/after realtime fan-out):

```javascript
const recipients = resolveFrameRecipients(msg, ws);
storeChatFrameForAccounts(chatMailbox, msg, recipients);  // always persist
deliverFrameToAccounts(msg, recipients, msg._from ?? ws._nexId);  // best-effort realtime
touch(); // snapshot debounce
```

**On `register_account`:**

```javascript
flushMlsMailbox(account);
flushChatMailbox(account);  // NEW — same as MLS control replay
```

**Optional explicit pull (Phase 2):**

| Wire op | Direction | Purpose |
|---------|-----------|---------|
| `chat_fetch` | client → relay | `{ type, account, request_id, since? }` |
| `chat_reply` | relay → client | `{ type, request_id, frames: [...] }` |
| `chat_consume` | client → relay | `{ type, account, dedup_keys: [...] }` — optional early delete |

**MVP (Phase 1):** only `flushChatMailbox` on reconnect — no new client wire ops.

### 4.3 Ephemeral frames

If `expiresAt` is set and `Date.now() > expiresAt`:

- **Do not store** (preferred), or store but skip on flush/fetch.
- Matches existing client drop in `wsRelay.onWire`.

### 4.4 Group vs direct routing

Reuse existing `resolveFrameRecipients`:

- `d:{a}:{b}` → both accounts
- `g:{id}` + `routeTo[]` → listed members
- `delivery.inboxId` → inbox owner only (sealed-sender path)

Store **one copy per recipient account** (same frame row referenced or duplicated — duplicate simpler for persistence JSON).

### 4.5 Persistence tier

Per `docs/RELAY_PERSISTENCE.md`:

| Data | Strategy |
|------|----------|
| `chatMailbox` | Debounced snapshot (300ms) + **WAL `chat_store` per frame** + SIGTERM flush |
| Future hardening | Journal op `chat_store` fsync if ops requires stronger durability |

Extend `relay-persistence.mjs`:

- `state.chatMailbox: Map<account, Map<dedupKey, row>>`
- Serialize/deserialize in snapshot (like `mlsMailbox`)

### 4.6 Limits & abuse

| Control | Default |
|---------|---------|
| `RELAY_MAX_MSG_BYTES` | 256 KiB / frame (existing) |
| `RELAY_RATE_LIMIT` | 120 msg/min/conn (existing) |
| `RELAY_CHAT_MAILBOX_MAX_FRAMES` | 5000 / account |
| `RELAY_CHAT_MAILBOX_MAX_BYTES` | 256 MiB / account |
| `RELAY_CHAT_MAILBOX_TTL_MS` | 180 days |
| Overflow | Drop **oldest** `stored_at` (LRU), log metric |

Sender must have called `register_account` before sending frames (existing implicit path).

---

## 5. Client design / 客户端设计

### 5.1 Phase 1 — zero new wire ops

Existing flow already sufficient if relay flushes on reconnect:

1. `unlock()` → `relayClient.connect(endpointId, account)` → `register_account`
2. Relay `flushChatMailbox` pushes pending frames
3. `wsRelay.onWire` → existing `onMessage` → `handleInboundFrame` → decrypt → `localStore.appendMessage`
4. `InboundDedup` prevents duplicate UI if realtime + flush overlap

**Verify:** `WebSocketRelay.connect` always sends `register_account` — ✅ already.

### 5.2 Phase 2 — stable dedup + explicit fetch

**Stable `dedupKey` on send** (idempotent retry + mailbox dedup):

```typescript
// persistAndRelay — use client message id, not random UUID
dedupKey: `${convId}:${optimistic.clientMsgId}`,
```

**Background fetch on unlock** (mobile / flaky WS):

```typescript
// after relay connect
await fetchChatMailbox(account);  // chat_fetch one-shot, like contactRequestInbox
```

**Optional consume:** after successful decrypt + local persist, batch `chat_consume` dedup keys (reduces relay storage). **Multi-device note:** only consume after all devices synced OR rely on TTL-only eviction + client dedup (recommended for MVP).

### 5.3 UI / UX

| Signal | Behavior |
|--------|----------|
| Offline delivery burst | No special UI — messages appear in timeline (may trigger unread bump) |
| Decrypt failure | Existing silent drop; log `[nexchat] undecryptable offline frame` |
| Mailbox full (sender side) | Relay drops oldest — sender cannot know; acceptable for v1 |
| Ephemeral | Never stored; recipient offline past TTL = message lost (by design) |

### 5.4 `msg_archive` relationship

`msg_archive` = **same-account cross-device backup** (IPFS). Offline mailbox = **cross-user delivery**. Both can coexist:

- Mailbox delivers ciphertext to recipient
- Recipient decrypts → localStore → `scheduleMsgArchivePush` backs up to IPFS

---

## 6. Multi-device semantics / 多端语义

**Problem:** `chat_consume` deleting globally breaks second device.

**Decision (MVP):**

- **No consume on flush** — entries removed only by **TTL + LRU cap**
- All devices use **client `InboundDedup`** + localStore `env.id` dedup
- Phase 3 optional: per-device cursor in `chat_fetch { device_id, since }`

---

## 7. Implementation plan / 分阶段开发

### Phase 1 — Relay store-and-forward (P0, ~2–3 days)

| # | Task | Files |
|---|------|-------|
| 1.1 | Chat mailbox module + unit tests | `relay-rs/server/src/mailbox.rs`, `relay-rs` tests |
| 1.2 | Wire store + flush in relay-server | `relay-rs/server/src/protocol.rs` |
| 1.3 | Snapshot persistence for `chatMailbox` | `relay-rs/core/src/persistence.rs` |
| 1.4 | Integration test: send while offline → reconnect → receive | extend `scripts/relay-server-auth.test.mjs` or new e2e |
| 1.5 | Deploy VPS + restart `nexchat-relay` | ops |

**Acceptance:**

- Alice sends to Bob while Bob WS disconnected
- Bob `register_account` → receives frame → decrypts in existing handler
- Relay restart does not lose pending frames (snapshot)

### Phase 2 — Client hardening (P0, ~1–2 days) ✅

| # | Task | Files |
|---|------|-------|
| 2.1 | Stable `dedupKey = convId:clientMsgId` on send | `appStore.ts`, `relay/chatMailbox.ts` |
| 2.2 | `chat_fetch` relay + client + unlock/visibility sync | `relay-rs`, `relay/chatMailbox.ts`, `relay/chatMailboxSync.ts`, `appStore.ts` |
| 2.3 | Log metric when offline burst > N frames | `chatMailboxSync.ts` |
| 2.4 | Manual E2E: two browsers, offline send, reconnect | QA checklist |

### Phase 3 — Ops & hardening (P1, ~2 days) ✅

| # | Task |
|---|------|
| 3.1 | Env docs: `RELAY_CHAT_MAILBOX_*` in `.env.example`, `RELAY_PERSISTENCE.md` |
| 3.2 | `admin_stats` include `chatMailbox` counts + bytes |
| 3.3 | `chat_consume` on relay + client helper (ops; per-device cursor deferred) |
| 3.4 | Load test: 5k frames/account, restart, flush latency (`relay-chat-mailbox-load.test.mjs`) |

### Phase 4 — Node gossipsub mailbox (P2, future)

Align with `docs/CHAT_GROUP_MLS_DESIGN.md` §13:

- `node/` notifications topic + per-member offline DB
- Client subscribes via chain node WS instead of standalone relay
- Relay mailbox remains for deployments without full node WS bridge

---

## 8. Test plan / 测试

### Unit (relay)

- `storeChatFrame` dedup by `dedupKey`
- TTL prune + LRU cap
- `expiresAt` frame not stored
- `flushChatMailbox` only sends to connected WS
- Snapshot round-trip preserves mailbox

### Integration

```text
1. Bob connects register_account, disconnects
2. Alice sends MLS frame to d:Bob
3. Assert chatMailbox has entry for Bob
4. Bob reconnects register_account
5. Bob WS receives frame (dedupKey match)
6. Alice sends duplicate dedupKey → single mailbox entry
```

### Client

- `InboundDedup` + stable dedupKey: reconnect does not duplicate messages in UI
- Group `routeTo`: all offline members get separate mailbox entries

### Regression

- Delivery token verify path unchanged
- Contact / MLS mailboxes unaffected
- Rate limit + max bytes still enforced

---

## 9. Rollout / 上线

1. Merge Phase 1 → deploy `relay-rs` release binary to VPS (`nexchat-relay.service`)
2. `systemctl restart nexchat-relay`
3. Merge Phase 2 client → rebuild NexChat frontend
4. Monitor relay data dir growth (`chatMailbox` size in admin_stats)

**Backward compatibility:** old clients without stable dedupKey still work; mailbox uses ciphertext hash fallback. Old relay + new client: no offline delivery (graceful degradation).

---

## 10. Open questions / 待决项

| ID | Question | Recommendation |
|----|----------|----------------|
| Q1 | Consume vs TTL-only eviction? | **TTL + LRU only** for MVP (multi-device safe) |
| Q2 | 180d vs 30d retention? | **180d** default; env override |
| Q3 | Group offline: store for all `routeTo` or all chain members? | **`resolveFrameRecipients` only** (what sender routed) |
| Q4 | Chain anchor for mailbox audit? | Defer — not required for v1 |

---

## 11. References

- `nexchat/scripts/relay-contact-mailbox.mjs` — mailbox pattern
- `nexchat/docs/RELAY_PERSISTENCE.md` — durability tiers
- `docs/CHAT_GROUP_MLS_DESIGN.md` §13 — long-term node mailbox
- `docs/CHAT_CORE_MLS_CONVERGENCE_DESIGN.md` §12–13 — 1:1 + group shared delivery
