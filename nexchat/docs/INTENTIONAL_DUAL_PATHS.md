# Intentional dual paths (do not merge)

EN: NexChat `src/` keeps several **parallel code paths on purpose**. They look redundant but serve
different product modes, control planes, or ordering backends. Refactors that "dedupe" them without
understanding these boundaries tend to break gray rollout or cross-mode behavior.

CN: NexChat `src/` 里有多条**刻意并存**的代码路径。它们看起来像重复，但对应不同产品模式、控制面或定序
后端。不理解这些边界就做「合并重构」，容易破坏灰度或跨模式行为。

---

## Track A (group escrow) vs Wire (per-device leaf)

| | **Track A** | **Wire (1:1 + group)** |
|---|-------------|-------------------------|
| **Flags** | `VITE_MLS_VAULT_ENABLED` (default ON) | `VITE_WIRE_MULTILEAF_ENABLED`, `VITE_WIRE_GROUP_MULTILEAF_ENABLED` (default OFF) |
| **Wallet** | Built-in desktop keyring (`WalletGate`) | Same — E2EI uses unlocked pair via `signRawWithAccountKey` (see `docs/WALLET.md`) |
| **Group leaf model** | One shared read-only escrow leaf per account | Each device holds its own signer + leaf (`{account}#{device}`) |
| **Send authority** | `GroupHandoffRuntime`, online handoff, optional PIN backup | No primary/handoff — every logged-in device can send |
| **Key modules** | `groupHandoffRuntime.ts`, `handoffCoordinator.ts`, `mlsVaultSync.ts`, `signingPinBackup.ts` | `directWireSession.ts`, `groupWireSession.ts`, `*CommitExecutor.ts`, `accountWireCommitCoordinator.ts` |
| **Do not** | Delete Track A when Wire flag exists — flag OFF must stay byte-identical | Fold Track A vault/handoff into Wire sessions |

Wire group mode uses a **separate engine snapshot** (`gwire:{account}`); turning the flag on is
forward-only (see `CHAT_GROUP_WIREIFY_DESIGN.md` §17.2).

---

## 1:1 Wire vs group Wire (symmetric twins)

| | **1:1** (`d:`) | **Group** (`g:`) |
|---|----------------|------------------|
| **Session** | `DirectWireSession` | `GroupWireSession` |
| **Executor** | `directWireCommitExecutor.ts` | `groupDeviceCommitExecutor.ts` |
| **Ordering** | Relay `commit_slot` / epoch CAS on `d:` conv | Chain `expected_epoch` CAS |
| **Join planners** | `wireJoinPlan.ts` | `wireGroupJoinPlan.ts`, `wireGroupJoinSettlePlan.ts` |

Shared join-phase skeleton (CD election, graft, peer-add timers) is **documented symmetry**, not
accidental copy-paste to delete blindly. Merge only when a shared abstraction is tested against
`groupWireAcceptance.test.ts` and 1:1 Wire join tests together.

When **both** Wire flags are on, `accountWireCommitCoordinator.ts` intentionally **unifies** one
account CD — do not split back into two coordinators without reading the dual-CD merge design.

---

## DirectMlsRegistry vs DirectWireSession graft

| | **`DirectMlsRegistry` + `directHandshake`** | **`DirectWireSession` graft path** |
|---|-----------------------------------------------|-------------------------------------|
| **When** | Pairwise 1:1 handshake (new chat, cold-start after peer-add timeout) | Multi-leaf 1:1 after sibling CD offer or peer-assisted Add |
| **Owns MLS group** | Yes — drives hello/kp/welcome/commit until `ready` | Grafted convs marked `graft-managed`; registry **must not** fork the group |
| **Do not** | Call `registry.ensure(peer)` for convs already graft-managed | Re-handshake a healthy restored group on peer-add timeout (appStore guards this) |

---

## Handshake control planes (three coordinators)

| Plane | Module | When |
|-------|--------|------|
| Relay demo | `handshake.ts` | Mock / multi-tab demo group on BroadcastChannel |
| Chain DS/AS | `chainHandshake.ts` | Production groups (`VITE_MLS_CONTROL_PLANE=chain`) |
| 1:1 pairwise | `directHandshake.ts` + `directMlsRegistry.ts` | Direct chats (`d:{peer}`) |

Selected by `VITE_MLS_CONTROL_PLANE` and `VITE_USE_MOCK`. Not mergeable into one class without losing
mock-vs-chain test coverage.

---

## Crypto engines

| | **`MlsEngine` (webcrypto)** | **`OpenMlsEngine` (WASM)** |
|---|------------------------------|----------------------------|
| **Flag** | `VITE_MLS_BACKEND=webcrypto` | `VITE_MLS_BACKEND=openmls` (default) |
| **Role** | AES-GCM transport placeholder | Real RFC 9420 |

---

## Relay pointer / sync modules (template duplication)

Files like `relay/*Pointer.ts` and `store/*Sync.ts` share mechanical patterns (localStorage +
relay KV + IPFS). Consolidating into generics is **optional cleanup**, not a correctness fix. Safe
to leave duplicated until a fourth/fifth channel makes abstraction worthwhile.

---

## Related docs

- Group Wire: [`../pallets/chat/CHAT_GROUP_WIREIFY_DESIGN.md`](../pallets/chat/CHAT_GROUP_WIREIFY_DESIGN.md)
- Multi-device MLS (Track A): [`../pallets/chat/CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md`](../pallets/chat/CHAT_MULTIDEVICE_MLS_SYNC_DESIGN.md)
- 1:1 Wire: [`../pallets/chat/CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md`](../pallets/chat/CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC.md)
