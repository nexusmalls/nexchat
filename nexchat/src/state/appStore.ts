// EN: ConversationStore facade — wires KeyVault + ChainClient + MlsEngine + RelayClient
// + LocalStore + Merge engine, and exposes the actions the UI calls (mirrors the Tauri
// command set in plan §3.1.2). The UI never touches chain/MLS/relay directly.
// Message pipeline: send = build P3 envelope → MlsEngine.encrypt → RelayClient.send;
// inbound = RelayClient → MlsEngine.decrypt → MessageVM → LocalStore → re-merge.
// CN: ConversationStore 门面——串起 KeyVault/ChainClient/MlsEngine/RelayClient/LocalStore/
// Merge 引擎，向 UI 暴露动作（对应 §3.1.2 command 集）。发送=构造 P3 信封→加密→relay；
// 入站=relay→解密→MessageVM→本地→重 Merge。

import { create } from "zustand";
import { ensureDefaultContacts } from "@/contacts/defaultContactsBootstrap";
import { chainClient } from "@/chain/chainClient";
import { signRawWithAccountKey } from "@/chain/signer";
import { leafKeyBindingBytes } from "@/mls/deviceLeafCredential";
import { localStore } from "@/store/localStore";
import { mlsEngine, type MlsEngine } from "@/mls/mlsEngine";
import { openMlsEngine } from "@/mls/openMlsEngine";
import { MlsCoordinator, type MlsStatus } from "@/mls/handshake";
import { ChainMlsCoordinator } from "@/mls/chainHandshake";
import { createGroupWithMembers } from "@/mls/createGroupFlow";
import { addMembersToGroup } from "@/mls/addMembersToGroupFlow";
import { ensureChainKeyPackagePublished } from "@/mls/chainKeyPackage";
import {
  disbandGroup,
  leaveGroup,
  removeGroupMembers,
  swapGroupMembers,
} from "@/mls/changeGroupMembersFlow";
import {
  ensureGroupMlsErrorMessage,
  ensureGroupMlsReady,
  syncAllGroupMls,
} from "@/mls/joinGroupMlsFlow";
import { GroupHandoffRuntime } from "@/mls/groupHandoffRuntime";
import type { DeviceMode } from "@/mls/deviceState";
import { deviceId as persistentDeviceId } from "@/store/convIndex";
import { approveAndCommitJoinRequests } from "@/mls/approveJoinRequestsFlow";
import {
  chainJoinErrorMessage,
  lookupGroupForJoin as fetchGroupLookup,
} from "@/group/groupJoinFlow";
import type { GroupLookupVM, PendingJoinVM } from "@/group/groupJoinTypes";
import {
  textEnvelope,
  fileEnvelope,
  mediaAckEnvelope,
  recallEnvelope,
  stampEnvelopeSentAt,
  envelopeSentAt,
  encodeEnvelope,
  decodeEnvelope,
  type EnvelopeV1,
  type FileBody,
} from "@/mls/envelope";
import { applyRecallEnvelope, canRecallMessage, markMessageRecalled } from "@/p3/recall";
import {
  uploadEncryptedFile,
  envelopeTypeForMime,
  fileBodyFromUpload,
} from "@/ipfs/media";
import { mediaThumbnail } from "@/ipfs/thumbnail";
import {
  createMediaPreviewUrl,
  createThumbPreviewUrl,
  revokePreviewUrl,
} from "@/chat/localMediaPreview";
import {
  burnAtOnCreate,
  ephemeralFromEnvelope,
  relayExpiresAt,
} from "@/ephemeral/ephemeral";
import { startEphemeralScheduler } from "@/ephemeral/scheduler";
import { maybePinUploadedFile, pinAttachmentOnChain } from "@/ipfs/pin";
import {
  exemptRetentionForMessage,
  registerUploadedMedia,
  shortenRetentionForMessage,
  startSenderMediaRetentionScheduler,
} from "@/ipfs/senderMediaRetention";
import { ipfsClient } from "@/ipfs/ipfsClient";
import { keyVault } from "@/keyvault/keyvault";
import {
  deriveVaultMasterFromSuri,
  getVaultMaster,
  setVaultMaster,
} from "@/wallet/vaultMaster";
import {
  relayClient,
  bytesToB64,
  b64ToBytes,
  isNetworkRelayConnected,
  withMultiDeviceEcho,
  type RelayFrame,
} from "@/relay/relayClient";
import { networkRelayRequired } from "@/relay/relayNetwork";
import { groupRouteTo } from "@/relay/groupRoute";
import { mergeConversations, appUnreadBadge } from "@/merge/spec";
import { fetchPeerAvatarMap } from "@/chat/peerAvatars";
import type {
  ConversationVM,
  MessageVM,
  AccountVM,
  MessageContent,
  GroupRole,
} from "@/types/viewModels";
import { config, signingPinBackupActive } from "@/config";
import { forwardBodyText, forwardPreview, fileBodyFromMessage, isMediaForwardReady } from "@/p3/forward";
import {
  isMentioned,
  parseMentionTokens,
  resolveMentions,
  rosterFromSeeds,
  type MentionMember,
} from "@/p3/mentions";
import type { PinRow } from "@/chain/pinQueries";
import {
  accountFromLeafIdentity,
  deviceLeafIdentity,
  directConvId,
  directMlsKey,
  peerFromDirectConvId,
  peerFromMlsKey,
  resolveDirectInboundConv,
  resolveMlsConvId,
} from "@/mls/directConv";
import { DirectMlsRegistry } from "@/mls/directMlsRegistry";
import {
  computeWireDeviceRoster,
  computeWireGroupRoster,
  type WireDeviceRoster,
  type WireGroupRoster,
} from "@/mls/wireDeviceRoster";
import { DirectWireSession } from "@/mls/directWireSession";
import { createUnifiedWireAccountCoordinator } from "@/mls/accountWireCommitCoordinator";
import { DirectAccountCommitCoordinator } from "@/mls/directAccountCommitCoordinator";
import { createAddDeviceExecutor } from "@/mls/directWireCommitExecutor";
import { GroupWireSession } from "@/mls/groupWireSession";
import { syncGroupEpoch as syncGroupChainEpoch } from "@/mls/groupMemberFlow";
import { isWireGroupActive } from "@/mls/wireGroupActivity";
import { planWireJoinTargets } from "@/mls/wireJoinPlan";
import {
  legacyDirectPeersForWireMigration,
  mergeWireJoinThreadPeers,
} from "@/mls/wireLegacyMigration";
import {
  probeRelayWireCapabilities,
  RELAY_WIRE_PROBE_PEER,
  webSocketProbeTransport,
} from "@/relay/relayWireCapabilities";
import { planWireGroupJoinSettle } from "@/mls/wireGroupJoinSettlePlan";
import { OpenMlsEngine, isReadOnlyEscrowError } from "@/mls/openMlsEngine";
import type { DirectMlsStatus } from "@/mls/directHandshake";
import { InboxManager } from "@/delivery/inboxManager";
import { TokenWallet } from "@/delivery/tokenWallet";
import { TokenExchange } from "@/delivery/tokenExchange";
import {
  attachDelivery,
  ensureDeliveryTokens,
  bootstrapDelivery,
  resolveInboundSender,
} from "@/delivery/deliveryGate";
import {
  offchainSyncEnabled,
  pushOffchainData,
  type OffchainSyncStatus,
} from "@/store/offchainSync";
import {
  offchainSyncCoordinator,
  type CoordinatedRestoreResult,
  getSyncAnchorTier,
} from "@/store/offchainSyncCoordinator";
import {
  SYNC_ANCHOR_FEE_BUFFER_PLANCK,
  SYNC_ANCHOR_FIRST_DEPOSIT_PLANCK,
  syncAnchorBalanceHint,
} from "@/store/syncAnchor";
import { readSyncAudit, type SyncAuditRecord } from "@/store/syncAuditLog";
import { scheduleConvIndexPush } from "@/store/convIndexSync";
import {
  clearConversationDeleted,
  loadDeletedConvIds,
  markConversationDeleted,
} from "@/store/deletedConversations";
import { scheduleContactsVaultPush } from "@/store/contactVaultSync";
import {
  msgArchiveSyncFor,
  scheduleMsgArchiveGapRefill,
  scheduleMsgArchivePush,
} from "@/store/msgArchiveSync";
import { assembleChatStack, type ChatStack } from "@/orchestrator/assembleChatStack";
import { resolveConvStack } from "@/crypto-dr/convStack";
import type { DrIncoming } from "@/crypto-dr/drTransport";
import { publishPrekeyBundle } from "@/crypto-dr/identityBridge";
import {
  nextSigningBackupSeq,
  pushSigningPinBackup,
  readLocalMlsSigningPointer,
  restoreSigningPinBackup,
} from "@/store/mlsSigningBackupSync";
import { coldStartMlsVaultRestore } from "@/store/mlsVaultSync";
import {
  loadContacts,
  loadUserRoster,
  mergeRosters,
  parseContactAddress,
  saveContacts,
  savedToMentionMember,
} from "@/store/contactBook";
import { canonicalAddress, shortAddress } from "@/wallet/address";
import { ContactRequestExchange } from "@/contacts/contactRequestExchange";
import { ChatMailboxSync } from "@/relay/chatMailboxSync";
import { relayFrameDedupKey, consumeChatMailbox } from "@/relay/chatMailbox";
import { frameRejectHint } from "@/relay/relayErrors";
import {
  GroupInviteExchange,
  type GroupInviteRow,
} from "@/contacts/groupInviteExchange";
import {
  findRequest,
  loadContactRequests,
  pendingInboundCount,
  pruneStaleRequests,
  type ContactRequest,
  updateRequestStatus,
} from "@/store/contactRequests";

export type UnlockMode = "dev" | "desktop";

const blockToTime = (block: number): number => (block === 0 ? 0 : block * 1000);

let directRegistry: DirectMlsRegistry | null = null;
// EN: Track A group sending-authority + online-handoff runtime (config.mlsVaultEnabled). Account-global;
// null when escrow is off (default) → groups behave as single-device (always `primary`). CN: 路线 A 群发送
// 权 + 在线交接运行时（config.mlsVaultEnabled）。账户级；托管关闭（默认）时为 null → 群按单设备处理（恒
// `primary`）。
let groupHandoff: GroupHandoffRuntime | null = null;
// EN: 1:1 Wire multi-leaf (config.wireMultileafEnabled). Separate device-distinct OpenMLS engine for
// direct chats + its Gate-1 coordinator session; null when the feature is off (default). CN: 1:1 Wire
// 多 leaf（config.wireMultileafEnabled）。私聊专用的设备区分 OpenMLS 引擎 + 其闸一协调会话；功能关闭
// （默认）时为 null。
let wireEngine: OpenMlsEngine | null = null;
let wireSession: DirectWireSession | null = null;
// EN: Decentralized 1:1 DR stack + 2↔3 orchestrator (config.drEnabled). Assembled at unlock by
// `assembleChatStack`; null when DR is off → 1:1 stays MLS-Wire-only. Default (with relay): DR-first
// (§20). Use `getChatStack()` for the conversation-stack decision + transition hooks. CN: 去中心化 1:1 DR 栈
// + 2↔3 编排器（config.drEnabled）。解锁时由 `assembleChatStack` 装配；DR 关闭时为 null →
// 1:1 仅走 MLS-Wire。有 relay 时默认 DR 优先（§20）。会话栈决策 + 切换钩子用 `getChatStack()`。
let chatStack: ChatStack | null = null;

/// EN: The assembled DR + 2↔3 orchestration stack (null until a DR-enabled unlock). CN: 装配好的
/// DR + 2↔3 编排栈（DR 启用解锁前为 null）。
export function getChatStack(): ChatStack | null {
  return chatStack;
}

// EN: Sync fast-path cache of peers whose 1:1 conversation is pinned to the DR stack (canonical
// address). Populated by `pinConvStack` (negotiation at open, §20) and auto-pinned on the first
// inbound DR frame. The send/recv hot path reads this synchronously to branch DR vs MLS-Wire
// without an async chain read. Empty (and inert) when DR is off. CN: 1:1 会话已钉定到 DR 栈的对端
// 同步快表（规范地址）。由 `pinConvStack`（开会话时 §20 协商）填充，首个入站 DR 帧自动钉定。收发
// 热路径同步读取以分流 DR / MLS-Wire，无需异步链读。DR 关闭时为空（且无副作用）。
const drPinnedPeers = new Set<string>();

// EN: One-shot guard so the DR X3DH prekey bundle is published at most once per unlock session.
// CN: 一次性守卫，使 DR X3DH 预密钥包每次解锁会话至多发布一次。
let drPrekeysPublished = false;

/// EN: Pin `peer`'s 1:1 to the DR stack in BOTH the sync send/recv fast-path set and the reactive
/// `drPeers` UI map (so the contacts badge / composer reflect "DR E2EE ready"). Idempotent. CN: 把
/// `peer` 的 1:1 钉定到 DR 栈——同时写入收发热路径同步集合与响应式 `drPeers` UI 映射（使通讯录标记 /
/// 输入框反映「DR E2EE 就绪」）。幂等。
function markDrPeer(peer: string): void {
  const key = canonicalAddress(peer);
  drPinnedPeers.add(key);
  if (!useAppStore.getState().drPeers[key]) {
    useAppStore.setState((s) => ({ drPeers: { ...s.drPeers, [key]: true } }));
  }
}

/// EN: Unpin `peer` from the DR stack in both the sync set and the UI map. CN: 从 DR 栈解钉
/// `peer`——同步集合与 UI 映射均移除。
function unmarkDrPeer(peer: string): void {
  const key = canonicalAddress(peer);
  drPinnedPeers.delete(key);
  if (useAppStore.getState().drPeers[key]) {
    useAppStore.setState((s) => {
      const next = { ...s.drPeers };
      delete next[key];
      return { drPeers: next };
    });
  }
}

/// EN: Publish this device's DR X3DH prekey bundle (register IK + SPK + OPK Merkle root) and
/// advertise DR stack capability (§20) so peers can negotiate DR and run SPK-fallback X3DH against
/// us. Best-effort + non-blocking: skipped under mock or when the active signer cannot raw-sign
/// (`endorseKey` needs it); a failure just leaves 1:1 on MLS-Wire until caps reach the chain.
/// CN: 发布本设备 DR X3DH 预密钥包（注册 IK + SPK + OPK Merkle 根）并公告 DR 栈能力（§20），使对端
/// 可协商 DR 并对我们跑 SPK 回退 X3DH。尽力而为 + 非阻塞：mock 或当前签名者不可裸签（`endorseKey`
/// 需要）时跳过；失败仅使 1:1 维持 MLS-Wire，直到能力上链。
async function publishDrPrekeysInBackground(): Promise<void> {
  const stack = chatStack;
  if (!stack || config.useMock || drPrekeysPublished) return;
  try {
    await publishPrekeyBundle(stack.engine, { store: stack.drStore });
    drPrekeysPublished = true;
    // EN: Upload the unspent OPK leaf set to the relay so it single-dispenses one-time prekeys to
    // X3DH initiators even while THIS device is offline (design §19/§21 OPK-over-relay). Best-effort:
    // a failure only means initiators fall back to the SPK until we serve live. CN: 把未用 OPK 叶子
    // 集合上传给 relay，使其在**本设备离线**时也向 X3DH 发起方单发一次性预密钥（设计 §19/§21
    // OPK-over-relay）。尽力而为：失败仅使发起方在我们实时服务前回退 SPK。
    await stack.opkResponder.upload();
  } catch (e) {
    console.warn("[nexchat] DR prekey publish failed (1:1 stays MLS-Wire until caps on-chain):", e);
  }
}

/// EN: Whether `peer`'s 1:1 conversation is currently routed over the DR stack. CN: `peer` 的 1:1
/// 会话当前是否走 DR 栈。
function isDrPeer(peer: string | null | undefined): boolean {
  if (!peer || !chatStack) return false;
  try {
    return drPinnedPeers.has(canonicalAddress(peer));
  } catch {
    return drPinnedPeers.has(peer);
  }
}

/// EN: Negotiate + pin the crypto stack for a 1:1 with `peer` (§20 二选一收口), mirroring the DR
/// choice into the sync cache. Best-effort: a failed negotiation leaves the conversation on
/// MLS-Wire. CN: 为与 `peer` 的 1:1 协商并钉定密码栈（§20 二选一收口），把 DR 选择镜像进同步快表。
/// 尽力而为：协商失败则会话保持 MLS-Wire。
async function pinConvStack(peer: string): Promise<void> {
  if (!chatStack) return;
  try {
    const choice = await resolveConvStack(peer, chatStack.stackRegistry);
    if (choice === "dr") markDrPeer(peer);
    else unmarkDrPeer(peer);
  } catch (e) {
    console.warn("[nexchat] conv stack negotiation failed; staying MLS-Wire:", e);
  }
}

/// EN: Outbound branch: if `convId` is a DR-pinned 1:1, fan the envelope out to every peer device
/// over the DR stack (§18.3) and report whether at least one copy was delivered. Returns
/// `{ dr: false }` for non-DR conversations so the caller uses the MLS-Wire path. CN: 出站分流：若
/// `convId` 为 DR 钉定的 1:1，则把信封经 DR 栈扇出给对端每个设备（§18.3）并报告是否至少送达一份。
/// 非 DR 会话返回 `{ dr: false }`，调用方走 MLS-Wire 路径。
async function drSend(
  convId: string,
  env: EnvelopeV1,
): Promise<{ dr: false } | { dr: true; delivered: boolean }> {
  const peer = peerFromDirectConvId(convId);
  if (!peer || !chatStack || !isDrPeer(peer)) return { dr: false };
  const plaintext = encodeEnvelope(env);
  const res = await chatStack.router.sendToAccount(peer, plaintext);
  // EN: Sibling echo (§8): fan a copy to our OWN other devices on the SAME conversation id so they
  // render the sent message. Best-effort + non-blocking — never gates peer delivery. CN: 兄弟设备
  // 回显（§8）：在同一会话 id 上向本账户其他设备各发一份，使其渲染已发消息。尽力而为 + 非阻塞——
  // 绝不阻塞对端投递。
  const self = useAppStore.getState().account?.account;
  if (self) {
    void chatStack.router
      .sendToAccount(self, plaintext, { convId, echoSelf: true })
      .catch((e) => console.warn("[nexchat] DR sibling echo failed:", e));
  }
  return { dr: true, delivered: res.sentTo.length > 0 };
}

/// EN: Inbound DR callback: decode the decrypted plaintext back into an `EnvelopeV1` and deposit it
/// into the right 1:1 timeline via the shared inbound path. A SIBLING ECHO (sender == our own
/// account, §8) is deposited as a self-sent message into `m.convId` (= `d:{other_party}`); a normal
/// inbound is keyed by `peerAccount`. CN: 入站 DR 回调：把解密明文解码回 `EnvelopeV1` 并经共享入站
/// 路径存入正确的 1:1 时间线。兄弟设备回显（发送方为本账户，§8）以自发消息落入 `m.convId`
/// （= `d:{对端}`）；普通入站按 `peerAccount` 键。
async function depositDrMessage(m: DrIncoming): Promise<void> {
  const self = useAppStore.getState().account?.account;
  if (!self) return;
  let env: EnvelopeV1;
  try {
    env = decodeEnvelope(m.plaintext);
  } catch {
    return;
  }
  if (canonicalAddress(m.peerAccount) === canonicalAddress(self)) {
    // EN: sibling echo — m.convId is the destination conversation (`d:{other_party}`). CN: 兄弟回显
    // ——m.convId 即目标会话（`d:{对端}`）。
    const echoPeer = peerFromDirectConvId(m.convId);
    if (!echoPeer) return;
    markDrPeer(echoPeer);
    await depositInboundEnvelope(m.convId, env, self, true);
    return;
  }
  markDrPeer(m.peerAccount);
  await depositInboundEnvelope(directConvId(m.peerAccount), env, m.peerAccount, false);
}
// EN: One-shot Wire relay capability probe per page load (warn when production relay lags).
// CN: 每页一次 Wire relay 能力探测（生产 relay 落后时告警）。
let relayWireProbeDone = false;
// EN: Auto-clear timer for the transient `notice` toast (setNotice). CN: 瞬态 `notice` 提示的自动清除计时器。
let noticeTimer: ReturnType<typeof setTimeout> | null = null;
// EN: This device's wire leaf id (= `endpointId.slice(0,8)`), captured at unlock so the device-roster
// UX (design §8) can mark which leaf is the local device (never offered for self-removal). CN: 本设备的
// wire leaf id（= `endpointId.slice(0,8)`），解锁时捕获，使设备名册 UX（设计 §8）能标记哪个 leaf 是本机
// （绝不作为自移除对象）。
let wireDeviceId: string | null = null;
// EN: This device's GROUP wire leaf id (= `endpointId.slice(0,8)`, config.wireGroupMultileafEnabled),
// captured at unlock so the group device-disclosure UX (design §9) can mark the local leaf. Null when
// group wire mode is off (default). CN: 本设备的**群** wire leaf id（= `endpointId.slice(0,8)`，
// config.wireGroupMultileafEnabled），解锁时捕获，供群设备披露 UX（设计 §9）标记本机 leaf。群 wire 模式
// 关闭（默认）时为 null。
let groupWireDeviceId: string | null = null;
// EN: Group Wire-ification live session (config.wireGroupMultileafEnabled, design §17 / G7): CD election +
// chain-ordered per-device add/remove/rekey for the GROUP engine. Null when group wire mode is off
// (default). CN: 群 Wire 化实时会话（config.wireGroupMultileafEnabled，设计 §17 / G7）：对**群**引擎做 CD
// 选举 + 链定序的每设备 增/删/rekey。群 wire 模式关闭（默认）时为 null。
let groupWireSession: GroupWireSession | null = null;
// EN: Shared account CD when BOTH 1:1 + group Wire are on (single election / presence / join offer).
// CN: 1:1 + 群 Wire 同开时的共享账户 CD（单次选举 / presence / join offer）。
let unifiedWireCoordinator: DirectAccountCommitCoordinator | null = null;
// EN: Max wait for cloud restore to settle before planning a no-sibling Wire join. The existing-1:1
// thread list (which keeps `peer_add_req` from leaking "new device online" to non-1:1 contacts, and
// keeps a fresh pairwise handshake from forking an existing multi-leaf group) is only authoritative
// AFTER restore; we bound the wait so a slow/absent restore still proceeds. CN: 规划无兄弟 Wire join
// 前等待云恢复安定的上限。已有 1:1 线索表（避免 `peer_add_req` 向非 1:1 联系人泄漏「新设备上线」、并避免
// 新 1:1 握手分叉已有多 leaf 群）仅在恢复**后**权威；设上限以便慢/无恢复仍继续。
const WIRE_JOIN_RESTORE_WAIT_MS = 8000;

let relayStatusUnsub: (() => void) | null = null;

/// EN: Subscribe to WS relay connect/disconnect for the production banner + send guard. CN: 订阅 WS relay
/// 连接/断开，驱动生产环境提示条与发送拦截。
function wireRelayConnectivityWatch(
  set: (partial: Partial<AppState> | ((s: AppState) => Partial<AppState>)) => void,
): void {
  relayStatusUnsub?.();
  if (!networkRelayRequired()) return;
  const syncConnected = () => set({ relayConnected: isNetworkRelayConnected() });
  const unsubs = [
    relayClient.onConnect?.(syncConnected) ?? (() => {}),
    relayClient.onDisconnect?.(() => set({ relayConnected: false })) ?? (() => {}),
  ];
  relayStatusUnsub = () => unsubs.forEach((u) => u());
}

/// EN: Block outbound chat when the networked relay is required but down. CN: 需要网络 relay 但未连接时
/// 拦截出站聊天。
function blockSendWithoutRelay(
  set: (partial: Partial<AppState> | ((s: AppState) => Partial<AppState>)) => void,
): boolean {
  if (!networkRelayRequired() || isNetworkRelayConnected()) return false;
  set({
    error: "消息服务未连接，无法发送。请检查网络后点击顶部横幅重试。",
  });
  return true;
}

const sleepMs = (ms: number) => new Promise((r) => setTimeout(r, ms));

/// EN: Resolve once cloud restore has reached a terminal phase (or the wait is exhausted / sync is
/// disabled — in which case the existing-1:1 list is already local-only and synchronously known).
/// CN: 当云恢复到达终态（或等待耗尽 / 同步关闭——此时已有 1:1 列表已是纯本地、同步可知）时解析。
async function awaitRestoreSettled(get: AppGet, timeoutMs: number): Promise<void> {
  if (!offchainSyncEnabled()) return;
  const isSettled = (phase?: string) =>
    phase === "ok" || phase === "partial" || phase === "no_backup" || phase === "error";
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (isSettled(get().offchainSync?.phase)) return;
    await sleepMs(150);
  }
}
let inboxManager: InboxManager | null = null;
let tokenWallet: TokenWallet | null = null;
let contactExchange: ContactRequestExchange | null = null;
let chatMailboxSync: ChatMailboxSync | null = null;
const inboundRecoverOnce = new Set<string>();
/// EN: Per-peer — only block once on MLS handshake wait per session; mailbox bursts
/// otherwise stall ~12s × N frames. CN: 每 peer 本会话仅阻塞等待一次 MLS 握手；否则信箱
/// 爆发会 ~12s×N 帧卡死。
const directMlsWaitOnce = new Set<string>();
const mailboxDeadKeys = new Set<string>();
let relayRejectWired = false;
const mailboxDropWarned = new Set<string>();

// EN: Coalesce relay chat-mailbox `consume` deletes. A mailbox burst can surface dozens of
// undecryptable (stale) frames; firing one short-lived WS per frame exhausted the browser's
// per-host WebSocket cap ("Insufficient resources"). Batch keys over a short window into a
// single `consumeChatMailbox` call (one WS). CN: 合并 relay chat 信箱的 consume 删除。一次信箱
// 爆发可能涌出几十条无法解密的陈旧帧；每帧开一个短命 WS 会打满浏览器单 host 的 WebSocket
// 上限（"Insufficient resources"）。在短窗口内把这些 key 攒成一次 consumeChatMailbox（一个 WS）。
const pendingChatConsume = new Set<string>();
let chatConsumeAccount: string | null = null;
let chatConsumeTimer: ReturnType<typeof setTimeout> | null = null;
const CHAT_CONSUME_BATCH_MS = 400;

function queueChatConsume(account: string, dedupKey: string): void {
  chatConsumeAccount = account;
  pendingChatConsume.add(dedupKey);
  if (chatConsumeTimer) return;
  chatConsumeTimer = setTimeout(() => {
    chatConsumeTimer = null;
    const acct = chatConsumeAccount;
    const keys = [...pendingChatConsume];
    pendingChatConsume.clear();
    if (!acct || keys.length === 0) return;
    void consumeChatMailbox(acct, keys).catch(() => {});
  }, CHAT_CONSUME_BATCH_MS);
}
let groupInviteExchange: GroupInviteExchange | null = null;
let relayEndpointId = "";
// EN: session-scoped dedup for outbound media_ack (resend is harmless but wasteful).
// CN: 本会话内 media_ack 去重（重发无害但浪费）。
const mediaAcksSent = new Set<string>();
let refreshConvTimer: ReturnType<typeof setTimeout> | null = null;
let pendingJoinPollTimer: ReturnType<typeof setInterval> | null = null;

function stopPendingJoinPoll(): void {
  if (pendingJoinPollTimer) {
    clearInterval(pendingJoinPollTimer);
    pendingJoinPollTimer = null;
  }
}

function startPendingJoinPoll(): void {
  stopPendingJoinPoll();
  pendingJoinPollTimer = setInterval(() => {
    void useAppStore.getState().refreshPendingJoins(true);
  }, 15_000);
}

async function refreshAdminJoinRequestCounts(
  conversations: ConversationVM[],
): Promise<Record<number, number>> {
  const counts: Record<number, number> = {};
  const adminGroups = conversations.filter(
    (c) =>
      c.kind === "group" &&
      c.groupId != null &&
      (c.myRole === "owner" || c.myRole === "admin"),
  );
  await Promise.all(
    adminGroups.map(async (c) => {
      const gid = c.groupId!;
      try {
        const rows = await chainClient.listGroupJoinRequests(gid);
        if (rows.length > 0) counts[gid] = rows.length;
      } catch (e) {
        console.warn("[nexchat] listGroupJoinRequests failed:", gid, e);
      }
    }),
  );
  return counts;
}

function scheduleOffchainSync(account: string): void {
  scheduleConvIndexPush(account);
  scheduleContactsVaultPush(account);
  scheduleMsgArchivePush(account);
}

type AppSet = (
  partial: Partial<AppState> | ((state: AppState) => Partial<AppState>),
) => void;
type AppGet = () => AppState;

const OFFCHAIN_RESTORE_TIMEOUT_MS = 90_000;

/// EN: Bound coordinated restore so a stuck IPFS/RPC call cannot leave the banner on
/// "restoring" forever. CN: 为协调恢复设上限，避免 IPFS/RPC 挂死导致恢复提示条永不消失。
async function restoreWithTimeout(): Promise<CoordinatedRestoreResult> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      offchainSyncCoordinator.restore(),
      new Promise<CoordinatedRestoreResult>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("云端恢复超时，请检查 IPFS 与网络连接")),
          OFFCHAIN_RESTORE_TIMEOUT_MS,
        );
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

/// EN: After cloud restore merges contacts, start 1:1 MLS handshakes for newly restored
/// friends (unlock only had the local roster, often empty on a fresh device).
/// CN: 云恢复合并通讯录后，为新恢复的好友启动 1:1 MLS 握手（解锁时仅有本地名册，新设备常为空）。
function ensureDirectMlsForContacts(
  account: string,
  contacts: MentionMember[],
  set: AppSet,
  get: AppGet,
): void {
  if (config.mlsBackend !== "openmls" || !directRegistry) return;
  for (const m of contacts) {
    if (m.address === account) continue;
    kickDirectMlsHandshake(m.address, set, get);
  }
}

/// EN: (Re)start pairwise MLS for one contact and refresh UI badge. Safe no-op before registry exists.
/// CN: 对单个联系人（重）启成对 MLS 并刷新 UI 标记；registry 未就绪时安全空操作。
function kickDirectMlsHandshake(peer: string, set: AppSet, get: AppGet): void {
  if (config.mlsBackend !== "openmls" || !directRegistry) return;
  const self = get().account?.account;
  if (!self) return;
  const canon = canonicalAddress(peer);
  if (canon === canonicalAddress(self)) return;
  directRegistry.ensure(canon);
  publishDirectMlsStatus(canon, set, get);
}

/// EN: Retry handshakes for all saved contacts after relay reconnect (kp/welcome flush).
/// CN: relay 重连后对所有已存联系人重试握手（flush kp/welcome）。
function retryDirectMlsForAllContacts(set: AppSet, get: AppGet): void {
  const self = get().account?.account;
  if (!self) return;
  for (const m of get().userContacts) {
    if (m.address === self) continue;
    kickDirectMlsHandshake(m.address, set, get);
  }
}

/// EN: One-shot probe of the live relay for Wire-multi-leaf features; surfaces a notice when the
/// deployed relay is behind the client (common pre-launch gap). CN: 对在线 relay 做一次 Wire 多 leaf
/// 能力探测；部署 relay 落后于客户端时弹出提示（上线前常见缺口）。
function scheduleRelayWireProbe(selfAddress: string, set: AppSet): void {
  if (relayWireProbeDone) return;
  if (!config.wireMultileafEnabled || !config.relayWs || !networkRelayRequired()) return;
  relayWireProbeDone = true;
  void (async () => {
    try {
      const conv = directMlsKey(selfAddress, RELAY_WIRE_PROBE_PEER);
      const report = await probeRelayWireCapabilities(
        webSocketProbeTransport(config.relayWs!, "client"),
        selfAddress,
        RELAY_WIRE_PROBE_PEER,
        conv,
      );
      if (!report.ok) {
        set({
          notice: `Relay 缺少 Wire 特性（${report.missing.join(", ")}），1:1 多设备可能不可用 / Relay missing Wire features (${report.missing.join(", ")})`,
        });
      }
    } catch (e) {
      console.warn("[nexchat][wire] relay capability probe failed:", e);
    }
  })();
}

/// EN: Push registry-derived 1:1 MLS status into UI state (contacts E2EE badge, chat send gate).
/// Used after coordinator emits AND after Wire graft lands (graft bypasses the coordinator).
/// CN: 把 registry 推导的 1:1 MLS 状态写入 UI（通讯录 E2EE 标记、聊天发送门）。协调器 emit 与 Wire 嫁接
/// 完成后（嫁接不经协调器）均需调用。
function publishDirectMlsStatus(peer: string, set: AppSet, get: AppGet): void {
  if (!directRegistry || config.mlsBackend !== "openmls") return;
  peer = canonicalAddress(peer);
  const account = get().account?.account;
  if (!account) return;
  const status = directRegistry.status(peer);
  const ready = directRegistry.isReady(peer);
  const merged = { ...status, ready };
  const prev = get().directMls;
  const becameReady = merged.ready && !prev[peer]?.ready;
  if (config.wireMultileafEnabled) {
    console.info("[nexchat][wire] 1:1 status", {
      peer,
      role: merged.role,
      ready: merged.ready,
    });
  }
  set({ directMls: { ...prev, [peer]: merged } });
  if (
    merged.ready &&
    tokenWallet &&
    tokenWallet.count(peer) < 8 &&
    config.deliveryTokensEnabled
  ) {
    prefetchDeliveryTokens(peer, account);
  }
  if (becameReady) {
    directMlsWaitOnce.delete(peer);
    void chatMailboxSync?.syncInbox();
    set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
  }
}

/// EN: Coordinated cloud restore + push + async chain anchor (shared by retry + post-unlock).
/// CN: 协调式云恢复 + 推送 + 异步链锚（重试与解锁后后台任务共用）。
async function performCoordinatedOffchainSync(
  account: string,
  set: AppSet,
  get: AppGet,
): Promise<CoordinatedRestoreResult | OffchainSyncStatus | null> {
  if (!offchainSyncEnabled()) return null;
  set({
    offchainSync: { phase: "restoring", contacts: null, convIndex: null, msgArchive: null },
  });
  offchainSyncCoordinator.bind(account, localStore);
  let status: CoordinatedRestoreResult;
  try {
    status = await restoreWithTimeout();
  } catch (e) {
    status = {
      phase: "error",
      contacts: null,
      convIndex: null,
      msgArchive: null,
      message: e instanceof Error ? e.message : "云端恢复失败",
      mlsRestored: false,
      usedChainAnchor: false,
      needsEpochBump: false,
    };
  }
  const userContacts = loadUserRoster(account);
  ensureDirectMlsForContacts(account, userContacts, set, get);
  set({ offchainSync: status, userContacts });
  await get().refreshConversations();
  // EN: Track A escrow restore-order fix (design §4) — when the cold-start vault import installed a
  // READ-ONLY group client, its snapshot epoch may lag the live chain epoch. Catch each imported group
  // up from the chain AFTER the import (the unlock-time `syncAllGroupMls` ran before the vault was
  // available), then refresh the inbox so restored groups become readable instead of `welcome_pending`.
  // CN: 路线 A 托管恢复时序修正（设计 §4）——冷启动 vault 导入装入**只读**群客户端后，其快照 epoch 可能落后
  // 链上当前 epoch。导入后再从链追平各导入群（解锁时的 `syncAllGroupMls` 早于 vault 可用而先跑），随后刷新
  // 信箱，使恢复的群可读，而非 `welcome_pending`。
  if ("mlsRestored" in status && status.mlsRestored) {
    void syncAllGroupMls(openMlsEngine, chainClient, account)
      .then(() => {
        set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
        void chatMailboxSync?.syncInbox();
      })
      .catch((e) => console.warn("[nexchat][group] post-vault catch-up failed:", e));
  }
  if (status.phase === "ok" || status.phase === "partial") {
    set({ offchainSync: { ...status, phase: "pushing" } });
    try {
      await pushOffchainData(account, localStore);
      set({ offchainSync: { ...status, phase: "ok" } });
    } catch (e) {
      console.warn("[nexchat] offchain push after restore failed:", e);
      set({ offchainSync: status });
    }
    // EN: chain anchor follows the long debounce via coordinator cadence — not on every unlock (§14.6).
    // CN: 链锚走 coordinator 长 debounce 节拍——不在每次解锁时立即发 extrinsic（§14.6）。
  }
  return status;
}

async function attachAnchorBalanceNote(
  account: string,
  status: CoordinatedRestoreResult | OffchainSyncStatus | null,
): Promise<CoordinatedRestoreResult | OffchainSyncStatus | null> {
  if (!status || getSyncAnchorTier(account) !== "standard") return status;
  try {
    const free = await chainClient.freeBalance(account);
    const min = SYNC_ANCHOR_FIRST_DEPOSIT_PLANCK + SYNC_ANCHOR_FEE_BUFFER_PLANCK;
    if (free >= min) return status;
    return {
      ...status,
      anchorNote: syncAnchorBalanceHint(true),
    };
  } catch {
    return status;
  }
}

/// EN: Heavy cold-path work after the UI is shown — KeyPackage, delivery inbox, cloud
/// restore/push, pending-join MLS catch-up. Must not block WalletGate unlock.
/// CN: UI 展示后的重冷路径——KeyPackage、投递信箱、云恢复/推送、待入群 MLS 补齐；不得阻塞
/// WalletGate 解锁。
async function runPostUnlockBackgroundWork(
  selfAddress: string,
  useChainCp: boolean,
  set: AppSet,
  get: AppGet,
): Promise<void> {
  try {
    if (config.deliveryTokensEnabled && inboxManager && tokenWallet) {
      try {
        await bootstrapDelivery(inboxManager, tokenWallet, selfAddress);
        set((s) =>
          s.account ? { account: { ...s.account, inboxRegistered: true } } : {},
        );
      } catch (e) {
        console.warn("[nexchat] delivery bootstrap failed:", e);
      }
    }

    if (useChainCp) {
      // EN: Do not block cloud restore — chain txs may stall when RPC/WS is flaky.
      // CN: 不阻塞云恢复——RPC/WS 不稳定时链上交易可能卡住。
      void ensureChainKeyPackagePublished(openMlsEngine, chainClient, selfAddress, 4).catch((e) => {
        console.warn("[nexchat] publishKeyPackage failed:", e);
      });
    }

    // EN: Publish the DR X3DH prekey bundle + stack caps (§20) so 1:1 can negotiate DR. Non-blocking;
    // no-op unless `config.drEnabled` assembled `chatStack`. CN: 发布 DR X3DH 预密钥包 + 栈能力（§20），
    // 使 1:1 可协商 DR。非阻塞；未经 `config.drEnabled` 装配 `chatStack` 时为空操作。
    void publishDrPrekeysInBackground();

    const status = await performCoordinatedOffchainSync(selfAddress, set, get);
    const withNote = await attachAnchorBalanceNote(selfAddress, status);
    if (withNote && withNote !== status) set({ offchainSync: withNote });
    try {
      const added = await ensureDefaultContacts(selfAddress, (peer, label, opts) =>
        get().addContact(peer, label, opts),
      );
      if (added > 0) {
        console.info(`[nexchat] added ${added} default contact(s) with on-chain nicknames`);
      }
    } catch (e) {
      console.warn("[nexchat] default contacts bootstrap failed:", e);
    }
    await get().refreshPendingJoins(true);
    const peers = get().userContacts.map((m) => m.address);
    await syncChatMailboxWhenMlsReady(selfAddress, peers);
    // EN: Defer heavy joinRequests.entries() polling until after restore/sync settles.
    // CN: 等恢复/同步稳定后再启动 joinRequests 全表扫描轮询，减轻 WS 压力。
    setTimeout(() => startPendingJoinPoll(), 45_000);
  } catch (e) {
    console.warn("[nexchat] post-unlock background sync failed:", e);
    // EN: Failsafe — never leave the banner stuck on "restoring" if the cold path threw before
    // the restore status was applied. Surface an actionable error (retry button) instead.
    // CN: 兜底——冷路径在写回恢复状态前抛错时，绝不让横幅停在"恢复中"，改为可重试的错误提示。
    const cur = get().offchainSync;
    if (cur && cur.phase === "restoring") {
      set({
        offchainSync: {
          phase: "error",
          contacts: null,
          convIndex: null,
          msgArchive: null,
          message: e instanceof Error ? e.message : "云端恢复失败",
        },
      });
    }
  }
}

function scheduleRefreshConversations(delayMs = 400): void {
  if (refreshConvTimer) clearTimeout(refreshConvTimer);
  refreshConvTimer = setTimeout(() => {
    refreshConvTimer = null;
    void useAppStore.getState().refreshConversations();
  }, delayMs);
}

function syncContactRequestState(account: string): {
  contactRequests: ContactRequest[];
  contactRequestBadge: number;
} {
  const contactRequests = pruneStaleRequests(account);
  return { contactRequests, contactRequestBadge: pendingInboundCount(contactRequests) };
}

function prefetchDeliveryTokens(peer: string, selfAddress: string): void {
  if (!config.deliveryTokensEnabled || !tokenWallet || !relayEndpointId) return;
  if (tokenWallet.count(peer) >= 8) return;
  void ensureDeliveryTokens(
    peer,
    selfAddress,
    tokenWallet,
    relayClient,
    relayEndpointId,
    directMlsKey(selfAddress, peer),
  ).catch(() => {});
}

// EN: OpenMLS storage key (group `g:{id}` or canonical direct `d:a:b`). CN: OpenMLS 存储键。
function mlsStorageKey(convId: string, selfAddress?: string): string {
  if (convId.startsWith("d:") && selfAddress) return resolveMlsConvId(convId, selfAddress);
  return convId;
}

// EN: pick the crypto engine for a conversation — the real OpenMLS engine once its group
// exists (after the relay handshake), otherwise the WebCrypto placeholder.
// CN: 按会话选引擎——群建立后（relay 握手完成）用真 OpenMLS，否则用 WebCrypto 占位。
// EN: Which real OpenMLS engine owns a conversation. Direct (`d:`) chats live on the separate
// device-distinct wire engine when 1:1 Wire is enabled; everything else (groups, `g:`) stays on the
// shared account engine. Falls back to the account engine when the wire engine is absent (feature
// off), so the default path is unchanged. CN: 某会话归属哪个真实 OpenMLS 引擎。启用 1:1 Wire 时私聊
// （`d:`）跑在独立的设备区分 wire 引擎上；其余（群、`g:`）仍在共享账户引擎。wire 引擎缺席（功能关闭）
// 时回退账户引擎，默认路径不变。
function realEngineFor(convId: string): OpenMlsEngine {
  if (wireEngine && convId.startsWith("d:")) return wireEngine;
  return openMlsEngine;
}

// EN: True for any real OpenMLS engine (account or wire) — they share the `mlsStorageKey` convention.
// CN: 对任意真实 OpenMLS 引擎（账户或 wire）为真——二者共用 `mlsStorageKey` 约定。
function isOpenMlsEngine(eng: MlsEngine): boolean {
  return eng === openMlsEngine || (wireEngine != null && eng === wireEngine);
}

function engineFor(convId: string, selfAddress?: string): MlsEngine {
  if (config.mlsBackend === "openmls") {
    try {
      const key = mlsStorageKey(convId, selfAddress);
      const real = realEngineFor(convId);
      if (real.hasGroup(key)) return real;
    } catch {
      /* OpenMLS engine not initialised yet */
    }
  }
  return mlsEngine;
}

/// EN: Track A group escrow — after a group (g:) MLS state change on the FULL account engine, schedule
/// an escrow-vault push (debounced) so a future cold/swapped device can restore read-only group state
/// from IPFS instead of hitting `welcome_pending` (design §4). No-op unless `mlsVaultEnabled`; the push
/// itself is a no-op for a read-only restored client (only a full client is the escrow authority, §3.2)
/// and for the mock backend. CN: 路线 A 群托管——完整账户引擎上群（g:）MLS 状态变更后，安排（防抖）托管
/// vault 推送，使未来冷/换机设备可从 IPFS 恢复只读群态，而非陷入 `welcome_pending`（设计 §4）。未开
/// `mlsVaultEnabled` 为空操作；推送本身对只读恢复客户端（仅完整客户端为托管权威，§3.2）与 mock 后端亦为空操作。
function scheduleGroupVaultBackup(): void {
  if (!config.mlsVaultEnabled || config.useMock || config.mlsBackend !== "openmls") return;
  try {
    offchainSyncCoordinator.markDirty("mls");
  } catch {
    /* coordinator not bound yet → ignore */
  }
}

async function mlsEncrypt(convId: string, env: EnvelopeV1, selfAddress: string): Promise<Uint8Array> {
  const eng = engineFor(convId, selfAddress);
  const key = isOpenMlsEngine(eng) ? mlsStorageKey(convId, selfAddress) : convId;
  const out = eng.encrypt(key, env);
  if (convId.startsWith("g:") && eng === openMlsEngine) scheduleGroupVaultBackup();
  return out;
}

async function mlsDecrypt(
  convId: string,
  ciphertext: Uint8Array,
  selfAddress: string,
): Promise<EnvelopeV1> {
  const eng = engineFor(convId, selfAddress);
  const key = isOpenMlsEngine(eng) ? mlsStorageKey(convId, selfAddress) : convId;
  const env = eng.decrypt(key, ciphertext);
  if (convId.startsWith("g:") && eng === openMlsEngine) scheduleGroupVaultBackup();
  return env;
}

function isMlsReady(convId: string, st: {
  mls: MlsStatus | null;
  mlsGroupId: number | null;
  directMls: Record<string, DirectMlsStatus>;
}): boolean {
  if (config.mlsBackend !== "openmls") return true;
  const peer = peerFromDirectConvId(convId);
  // EN: DR-pinned 1:1 has no blocking handshake — the router establishes per-device sessions
  // lazily on first send (§18.3). CN: DR 钉定的 1:1 无阻塞握手——首次发送时由路由按设备懒建会话。
  if (peer && isDrPeer(peer)) return true;
  if (peer) return st.directMls[peer]?.ready ?? directRegistry?.isReady(peer) ?? false;
  if (convId.startsWith("g:")) {
    try {
      return openMlsEngine.hasGroup(convId);
    } catch {
      return false;
    }
  }
  return engineFor(convId) === mlsEngine;
}

async function waitForDirectMls(peer: string, timeoutMs = 12_000): Promise<boolean> {
  if (config.mlsBackend !== "openmls") return true;
  if (!directRegistry) return false;
  directRegistry.ensure(peer);
  if (directRegistry.isReady(peer)) return true;
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    await new Promise((r) => setTimeout(r, 200));
    if (directRegistry.isReady(peer)) return true;
  }
  return directRegistry.isReady(peer);
}

/// EN: §8.1 on-demand graft — trigger `activateGroup` for a deferred group and wait until the local engine
/// holds it (Welcome landed). No-op when group wire mode is off or the group is already held. CN: §8.1 按需
/// 嫁接——对延迟群触发 `activateGroup` 并等待本地引擎持群（Welcome 落地）。群 wire 模式关闭或已持群时空操作。
async function ensureGroupWireGraftReady(convId: string, timeoutMs = 15_000): Promise<boolean> {
  if (!config.wireGroupMultileafEnabled || !groupWireSession || !convId.startsWith("g:")) {
    return true;
  }
  try {
    if (openMlsEngine.hasGroup(convId)) return true;
  } catch {
    return false;
  }
  await groupWireSession.ensureGraftOrPeerAdd(convId);
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      if (openMlsEngine.hasGroup(convId)) return true;
    } catch {
      /* keep waiting */
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  try {
    return openMlsEngine.hasGroup(convId);
  } catch {
    return false;
  }
}

/// EN: Read group activity for the §8.1 lazy join planner from live UI state. CN: 从实时 UI 状态读取群活跃度，
/// 供 §8.1 延迟 join 规划器使用。
function wireGroupActivityContext(): {
  activeConvId: string | null;
  conversations: ConversationVM[];
} {
  const st = useAppStore.getState();
  return { activeConvId: st.activeConvId, conversations: st.conversations };
}

/// EN: Wait until direct MLS handshakes settle for listed peers (or timeout).
/// CN: 等待列表中对端的 1:1 MLS 握手完成（或超时）。
async function waitMlsReadyForPeers(
  selfAddress: string,
  peers: string[],
  timeoutMs = 30_000,
): Promise<void> {
  if (!directRegistry || config.mlsBackend !== "openmls") return;
  const uniquePeers = [...new Set(peers.filter((p) => p && p !== selfAddress))];
  if (uniquePeers.length === 0) return;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const pending = uniquePeers.filter((p) => !directRegistry!.isReady(p));
    if (pending.length === 0) break;
    await new Promise((r) => setTimeout(r, 500));
  }
}

/// EN: Pull chat mailbox only after direct MLS handshakes settle (avoids WrongGroupId burst).
/// CN: 等 1:1 MLS 握手完成后再拉 chat 信箱（避免 WrongGroupId 刷屏）。
async function syncChatMailboxWhenMlsReady(
  selfAddress: string,
  peers: string[],
): Promise<void> {
  if (!chatMailboxSync) return;
  await waitMlsReadyForPeers(selfAddress, peers);
  await chatMailboxSync.syncInbox();
}

function newClientMsgId(prefix = "c"): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
}

function forwardMetaFrom(source: MessageVM) {
  return {
    msgId: source.clientMsgId,
    convId: source.convId,
    preview: forwardPreview(source),
  };
}

function forwardEnvelopeRef(source: MessageVM) {
  return {
    fromMsg: source.clientMsgId,
    fromConv: source.convId,
    preview: forwardPreview(source),
  };
}

/// EN: Persist message fields to encrypted IDB (append alone is not enough after in-memory edits).
/// CN: 将消息字段写入加密 IDB（仅 append 无法反映后续内存修改）。
async function syncMessageToStore(convId: string, msg: MessageVM): Promise<void> {
  await localStore.updateMessage(convId, msg.clientMsgId, {
    content: msg.content,
    status: msg.status,
    ephemeralTtlMs: msg.ephemeralTtlMs,
    ephemeralBurnOn: msg.ephemeralBurnOn,
    ephemeralBurnAt: msg.ephemeralBurnAt,
  });
  if (useAppStore.getState().activeConvId === convId) {
    useAppStore.setState({ messages: await localStore.listMessages(convId) });
  }
}

/// EN: Attach group member routing hint for relay targeted delivery. CN: 附加群成员路由供 relay 定向投递。
async function withRelayRoute(frame: RelayFrame, convId: string): Promise<RelayFrame> {
  if (!convId.startsWith("g:")) return frame;
  const routeTo = await groupRouteTo(convId);
  return routeTo?.length ? { ...frame, routeTo } : frame;
}

/// EN: Route + optional Wire multi-device echo, then send. CN: 路由 + 可选 Wire 多设备回显后发送。
async function relaySendFrame(
  frame: RelayFrame,
  convId: string,
  opts: { echoSelf?: boolean } = {},
): Promise<void> {
  let out = await withRelayRoute(frame, convId);
  if (opts.echoSelf !== false) {
    out = withMultiDeviceEcho(out, convId);
  }
  await relayClient.send(out);
}

/// EN: Append optimistic message, encrypt envelope, relay send. CN: 乐观写入 → 加密 → relay 发送。
async function persistAndRelay(
  convId: string,
  env: EnvelopeV1,
  optimistic: MessageVM,
  account: AccountVM,
): Promise<void> {
  if (blockSendWithoutRelay(useAppStore.setState)) {
    optimistic.status = "failed";
    await localStore.appendMessage(optimistic);
    const active = useAppStore.getState().activeConvId;
    if (active === convId) {
      useAppStore.setState({ messages: await localStore.listMessages(convId) });
    }
    return;
  }
  await localStore.appendMessage(optimistic);
  const activeConvId = useAppStore.getState().activeConvId;
  if (activeConvId === convId) {
    useAppStore.setState({ messages: await localStore.listMessages(convId) });
  }
  try {
    const peer = peerFromDirectConvId(convId);
    if (peer && tokenWallet && relayEndpointId) {
      prefetchDeliveryTokens(peer, account.account);
    }
    const eph = ephemeralFromEnvelope(env);
    const stamped = stampEnvelopeSentAt(env, optimistic.sentAt);
    const drOut = await drSend(convId, stamped);
    if (drOut.dr) {
      optimistic.status = drOut.delivered ? "sent" : "failed";
    } else {
      const ciphertext = await mlsEncrypt(convId, stamped, account.account);
      let frame: RelayFrame = {
        convId,
        senderRef: account.account,
        ciphertextB64: bytesToB64(ciphertext),
        dedupKey: relayFrameDedupKey(convId, optimistic.clientMsgId),
        expiresAt:
          eph.ephemeralTtlMs && eph.ephemeralBurnOn
            ? relayExpiresAt(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
            : undefined,
      };
      if (peer && tokenWallet) {
        frame = await attachDelivery(frame, peer, account.account, tokenWallet);
      }
      await relaySendFrame(frame, convId);
      optimistic.status = "sent";
    }
  } catch (e) {
    optimistic.status = "failed";
    useAppStore.setState({ error: String(e) });
  }
  await syncMessageToStore(convId, optimistic);
  scheduleMsgArchivePush(account.account);
}

function contentFromEnvelope(env: EnvelopeV1): MessageContent {
  if (env.type === "text") {
    return { type: "text", text: (env.body as { text: string }).text };
  }
  if (env.type === "reaction" && env.reaction) {
    return {
      type: "reaction",
      target: env.reaction.target,
      emoji: env.reaction.emoji,
      op: env.reaction.op,
    };
  }
  if (["image", "video", "audio", "file"].includes(env.type)) {
    const body = env.body as FileBody;
    return {
      type: "media",
      mime: body.mime,
      name: body.name,
      size: body.size,
      thumbReady: !!body.thumbCid,
      bodyReady: !!body.rootCid,
      durationMs: body.durationMs,
      rootCid: body.rootCid,
      fileKey: body.fileKey,
      thumbCid: body.thumbCid,
      thumbKey: body.thumbKey,
      chunked: body.chunked,
    };
  }
  return { type: "text", text: `[${env.type}]` };
}

interface AppState {
  account: AccountVM | null;
  conversations: ConversationVM[];
  activeConvId: string | null;
  messages: MessageVM[];
  badge: number;
  loading: boolean;
  error: string | null;
  /** EN: transient success/info toast (auto-dismissed), distinct from the error toast. CN: 瞬态
   * 成功/信息提示（自动消失），与错误提示区分。 */
  notice: string | null;
  /** EN: set (or clear with null) the transient notice toast; non-null messages auto-clear after a
   * few seconds. CN: 设置（传 null 清除）瞬态提示；非空消息数秒后自动清除。 */
  setNotice: (msg: string | null) => void;
  replyingTo: MessageVM | null;
  forwardingFrom: MessageVM | null;
  /** EN: message pending forward (picker open). CN: 待转发的消息（选择会话弹层）。 */
  forwardSource: MessageVM | null;
  ephemeralMs: number | null;
  /** EN: demo roster from env seeds. CN: env 种子演示名册。 */
  mentionRoster: MentionMember[];
  /** EN: user-added contacts (localStorage per account). CN: 用户添加的联系人。 */
  userContacts: MentionMember[];
  selfMention: MentionMember | null;
  mls: MlsStatus | null;
  // EN: chain control-plane: the on-chain group id the OpenMLS demo bound to (null until
  // owner mints / member discovers it). CN: 链上控制面：OpenMLS 演示绑定的链上群 id。
  mlsGroupId: number | null;
  /** EN: IPFS pins owned by the signed-in account (storage-service). CN: 当前账户名下 IPFS Pin。 */
  pins: PinRow[];
  pinsOpen: boolean;
  pinsLoading: boolean;
  /** EN: per-peer 1:1 MLS handshake status. CN: 各对端 1:1 MLS 握手状态。 */
  directMls: Record<string, DirectMlsStatus>;
  /** EN: per-peer 1:1 DR-stack pin (§20/§21): true once a 1:1 is negotiated to the decentralized
   * Double Ratchet stack — E2EE is ready immediately (no blocking handshake). Keyed by canonical
   * address. CN: 各对端 1:1 DR 栈钉定（§20/§21）：1:1 协商到去中心化双棘轮栈后为 true——E2EE 立即
   * 就绪（无阻塞握手）。按规范地址键。 */
  drPeers: Record<string, boolean>;
  /** EN: bump to re-check OpenMLS hasGroup after background sync. CN: 后台 MLS 同步后触发重渲染。 */
  mlsSyncRev: number;
  /** EN: Track A group send-authority state for THIS device (design §7.3). `primary` = may send;
   * `secondary` = read-only (escrow-restored / handed-off-away) → must request authority; `restoring`
   * = resolving. Always `primary` unless the escrow vault is enabled. CN: 本设备路线 A 群发送权态
   * （设计 §7.3）。`primary` = 可发送；`secondary` = 只读（托管恢复/已交出）→ 需申请发送权；`restoring`
   * = 解析中。未启用托管 vault 时恒为 `primary`。 */
  groupSendMode: DeviceMode;
  /** EN: New-device action — request group sending authority from the account's primary device via the
   * §5.2 online handoff. CN: 新设备动作——经 §5.2 在线交接向账户主设备申请群发送权。 */
  requestGroupSendAuthority: () => Promise<void>;
  /** EN: Track A — seal + upload signing-key backup under PIN (design §5.3 path C, P1). CN: 路线 A ——
   * 用 PIN 密封并上传签名钥备份（设计 §5.3 路径 C，P1）。 */
  createSigningPinBackup: (pin: string) => Promise<{ cid: string; updated_at: number }>;
  /** EN: Track A — offline restore signing keys from PIN backup (design §5.3 path C, P2). CN: 路线 A ——
   * 用 PIN 备份离线恢复签名钥（设计 §5.3 路径 C，P2）。 */
  restoreSigningPinBackup: (pin: string) => Promise<void>;
  newDmOpen: boolean;
  newGroupOpen: boolean;
  joinGroupOpen: boolean;
  /** EN: pending private-group join requests (not yet in conv list). CN: 待批私群入群申请。 */
  pendingJoins: PendingJoinVM[];
  /** EN: per-group inbound join-request counts for admin groups. CN: 管理员群的待批入群申请数。 */
  groupJoinRequestCounts: Record<number, number>;
  /** EN: deep-link / banner target group id for join preview. CN: 深链/横幅打开预览的目标群 id。 */
  joinPreviewGroupId: number | null;
  /** EN: group member manage modal target. CN: 群成员管理弹窗目标。 */
  groupManageTarget: {
    groupId: number;
    title: string;
    memberCount: number;
    myRole: GroupRole;
    initialTab?: "members" | "joinRequests";
  } | null;
  /** EN: invite-members modal target. CN: 邀请成员弹窗目标群。 */
  inviteGroupTarget: {
    groupId: number;
    title: string;
    memberCount: number;
  } | null;
  /** EN: relay group_invite rows. CN: relay 群邀请记录。 */
  groupInvites: GroupInviteRow[];
  groupInviteBadge: number;
  /** EN: relay contact_req / contact_ack rows. CN: relay 联系人请求记录。 */
  contactRequests: ContactRequest[];
  /** EN: pending inbound contact requests (contacts tab badge). CN: 待处理入站请求数。 */
  contactRequestBadge: number;
  /** EN: Cloud backup restore status after unlock — carries the §6.5 orchestration
   * flags (`usedChainAnchor` / `needsEpochBump`) when the coordinated restore ran.
   * CN: 解锁后云端备份恢复状态——经 coordinator 恢复时附带 §6.5 编排标志
   * （`usedChainAnchor` / `needsEpochBump`）。 */
  offchainSync: OffchainSyncStatus | CoordinatedRestoreResult | null;
  /** EN: WS relay connectivity (always true when network relay is not required). CN: WS relay 连接态（未
   *  要求网络 relay 时恒为 true）。 */
  relayConnected: boolean;
  /** EN: Retry the WebSocket relay after a failed connect / drop. CN: connect 失败或断线后重试 WS relay。 */
  retryRelayConnect: () => Promise<void>;
  retryOffchainSync: () => Promise<void>;
  /** EN: User acknowledged the epoch-bump notice (§6.5 ③). CN: 用户已知晓 epoch bump 提示（§6.5 ③）。 */
  dismissEpochBump: () => void;
  /** EN: §6.5 ②③ — bump the delivery-inbox epoch and re-register on the relay,
   * closing the spent-replay window; resolves false when delivery is disabled or
   * the bump failed. CN: §6.5 ②③——递增投递信箱 epoch 并向 relay 重注册，关闭
   * spent 重放窗口；delivery 未启用或失败时返回 false。 */
  bumpInboxEpoch: () => Promise<boolean>;
  /** EN: Self-healing audit trail (oldest→newest) for the active account; [] when none.
   * Replay entry point for ops/support (§6.2/§6.3/§6.5). CN: 当前账户的数据层自愈审计
   * 轨迹（旧→新），无则 []。运维/客服复盘入口。 */
  getSyncAuditLog: () => SyncAuditRecord[];

  unlock: (
    address: string,
    nickname?: string,
    opts?: { mode?: UnlockMode },
  ) => Promise<void>;
  refreshConversations: () => Promise<void>;
  openConversation: (convId: string) => Promise<void>;
  closeConversation: () => void;
  sendMessage: (text: string) => Promise<void>;
  sendFile: (file: File) => Promise<void>;
  /** EN: Receiver downloaded the full media body — notify the sender (1:1 only) so it can
   * release its local pin early. CN: 接收方已下载完整媒体——通知发送方（仅 1:1）提前释放本机 pin。 */
  ackMediaDownloaded: (convId: string, clientMsgId: string) => Promise<void>;
  /** EN: User "keep attachment" — star the message, exempt sender local-pin TTL, and (LIVE)
   * request an on-chain Temporary Pin. CN: 用户「保留附件」——标星、豁免发送方本机 TTL，
   * LIVE 模式下发起链上 Temporary Pin。 */
  keepAttachment: (msg: MessageVM) => Promise<void>;
  /** EN: Delete one message locally and propagate a tombstone to other devices via the
   * encrypted message archive. CN: 本地删除一条消息，并经加密历史归档把墓碑同步到其他设备。 */
  deleteMessage: (msg: MessageVM) => Promise<void>;
  /** EN: Clear the whole local timeline of a conversation; tombstones propagate to other
   * devices. CN: 清空一个会话的本地聊天记录；墓碑同步到其他设备。 */
  clearConversationHistory: (convId: string) => Promise<void>;
  /** EN: Remove a conversation from the chat list and delete local messages; conv-index +
   * msg-archive tombstones propagate to your other devices. Does not leave groups or delete
   * peer-side history. CN: 从聊天列表移除会话并删除本地消息；conv-index 与 msg-archive 墓碑同步到
   * 本账户其他设备。不会退群，也不会删除对端历史。 */
  deleteConversation: (convId: string) => Promise<void>;
  /** EN: Two-sided recall — the SENDER hides a sent message for both parties (within the recall
   * window) by emitting a `recall` control envelope; the target flips to a "recalled" placeholder
   * on every device. CN: 双向撤回——**发送方**在撤回窗口内经 `recall` 控制信封对收发双方隐藏一条
   * 已发消息；目标在所有设备翻转为「已撤回」占位。 */
  recallMessage: (msg: MessageVM) => Promise<void>;
  setGroupAvatar: (groupId: number, file: File) => Promise<void>;
  setReplyingTo: (msg: MessageVM | null) => void;
  setForwardingFrom: (msg: MessageVM | null) => void;
  openForwardPicker: (msg: MessageVM) => void;
  closeForwardPicker: () => void;
  forwardToConversations: (convIds: string[], comment?: string) => Promise<void>;
  setEphemeral: (ms: number | null) => void;
  setPref: (
    convId: string,
    pref: { pinnedPref?: boolean; dndPref?: boolean; archivedPref?: boolean },
  ) => Promise<void>;
  setPinsOpen: (open: boolean) => void;
  refreshPins: () => Promise<void>;
  renewPin: (cidHash: string, periods: number) => Promise<void>;
  setNewDmOpen: (open: boolean) => void;
  setNewGroupOpen: (open: boolean) => void;
  setJoinGroupOpen: (open: boolean) => void;
  openJoinPreview: (groupId: number) => Promise<void>;
  lookupGroupForJoin: (groupId: number) => Promise<GroupLookupVM>;
  requestJoinGroup: (groupId: number) => Promise<void>;
  cancelJoinRequestGroup: (groupId: number) => Promise<void>;
  publishKeyPackageForJoin: () => Promise<void>;
  refreshPendingJoins: (silent?: boolean) => Promise<void>;
  approveJoinRequests: (
    groupId: number,
    applicantAddresses: string[],
    opts?: { onProgress?: (message: string) => void },
  ) => Promise<void>;
  createGroupChat: (name: string, memberAddresses: string[]) => Promise<void>;
  openGroupManage: (target: {
    groupId: number;
    title: string;
    memberCount: number;
    myRole: GroupRole;
    initialTab?: "members" | "joinRequests";
  }) => void;
  closeGroupManage: () => void;
  openInviteGroupMembers: (target: {
    groupId: number;
    title: string;
    memberCount: number;
  }) => void;
  closeInviteGroupMembers: () => void;
  inviteGroupMembers: (
    memberAddresses: string[],
    opts?: { onProgress?: (message: string) => void },
  ) => Promise<"committed" | "queued">;
  dismissGroupInvite: (inviteId: string) => void;
  acceptGroupInvite: (inviteId: string) => Promise<void>;
  syncGroupInvite: (inviteId: string) => Promise<void>;
  removeGroupMember: (
    memberAddress: string,
    opts?: { onProgress?: (message: string) => void },
  ) => Promise<void>;
  swapGroupMember: (
    removeAddress: string,
    addAddress: string,
    opts?: { onProgress?: (message: string) => void },
  ) => Promise<void>;
  leaveGroupChat: (opts?: { onProgress?: (message: string) => void }) => Promise<void>;
  disbandGroupChat: (opts?: { onProgress?: (message: string) => void }) => Promise<void>;
  startDirectChat: (peerAddress: string, title?: string) => Promise<void>;
  addContact: (
    address: string,
    label: string,
    opts?: { notify?: boolean },
  ) => Promise<void>;
  removeContact: (peerAddress: string) => Promise<void>;
  acceptContactRequest: (reqId: string, label: string) => Promise<void>;
  rejectContactRequest: (reqId: string) => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  account: null,
  conversations: [],
  activeConvId: null,
  messages: [],
  badge: 0,
  loading: false,
  error: null,
  notice: null,
  replyingTo: null,
  forwardingFrom: null,
  forwardSource: null,
  ephemeralMs: null,
  mentionRoster: [],
  userContacts: [],
  selfMention: null,
  mls: null,
  mlsGroupId: null,
  pins: [],
  pinsOpen: false,
  pinsLoading: false,
  directMls: {},
  drPeers: {},
  mlsSyncRev: 0,
  groupSendMode: "primary",
  newDmOpen: false,
  newGroupOpen: false,
  joinGroupOpen: false,
  pendingJoins: [],
  groupJoinRequestCounts: {},
  joinPreviewGroupId: null,
  groupManageTarget: null,
  inviteGroupTarget: null,
  groupInvites: [],
  groupInviteBadge: 0,
  contactRequests: [],
  contactRequestBadge: 0,
  offchainSync: null,
  relayConnected: true,

  getSyncAuditLog() {
    const account = get().account?.account;
    return account ? readSyncAudit(account) : [];
  },

  setNotice(msg) {
    if (noticeTimer) {
      clearTimeout(noticeTimer);
      noticeTimer = null;
    }
    set({ notice: msg });
    if (msg) {
      noticeTimer = setTimeout(() => {
        noticeTimer = null;
        set({ notice: null });
      }, 4000);
    }
  },

  dismissEpochBump() {
    const status = get().offchainSync;
    if (!status || !("needsEpochBump" in status)) return;
    set({ offchainSync: { ...status, needsEpochBump: false } });
  },

  async bumpInboxEpoch() {
    const account = get().account?.account;
    if (!account || !config.deliveryTokensEnabled || !inboxManager) return false;
    try {
      const epoch = await inboxManager.bumpEpoch(account);
      console.info(`[nexchat] delivery inbox epoch bumped to ${epoch} (§6.5 ③)`);
      get().dismissEpochBump();
      return true;
    } catch (e) {
      console.warn("[nexchat] inbox epoch bump failed:", e);
      return false;
    }
  },

  async retryOffchainSync() {
    const account = get().account?.account;
    if (!account || !offchainSyncEnabled()) return;
    await performCoordinatedOffchainSync(account, set, get);
  },

  async retryRelayConnect() {
    const account = get().account?.account;
    if (!account || !networkRelayRequired() || !relayEndpointId) return;
    try {
      await relayClient.connect(relayEndpointId, account);
      set({ relayConnected: isNetworkRelayConnected(), error: null });
    } catch (e) {
      console.warn("[nexchat] relay reconnect failed:", e);
      set({ relayConnected: false });
    }
  },

  async unlock(address, nickname, opts) {
    // EN: guard against React StrictMode double-invoke. CN: 防 React StrictMode 双触发。
    if (get().account || get().loading) return;
    inboundRecoverOnce.clear();
    directMlsWaitOnce.clear();
    set({ loading: true, error: null });
    try {
      const kp = await mlsEngine.ensureKeyPackages(5);

      const useChainCp =
        config.mlsBackend === "openmls" && config.mlsControlPlane === "chain" && !config.useMock;
      const mode = opts?.mode ?? (config.devWallet && !config.useMock ? "dev" : "desktop");
      let selfAddress = canonicalAddress(address);

      if (mode === "dev" && config.devWallet) {
        if (useChainCp) {
          selfAddress = canonicalAddress(await chainClient.useDevAccount(config.devSeed));
        } else if (!config.useMock) {
          selfAddress = canonicalAddress(await chainClient.deriveAddress(config.devSeed));
        }
      } else {
        const signer = chainClient.signerAddress;
        if (useChainCp && !signer) {
          throw new Error("链上控制面需要已解锁的签名者（请先解锁桌面钱包）");
        }
        if (signer) selfAddress = canonicalAddress(signer);
      }

      // EN: account-scoped KDF root for local store + cross-device blobs. §5.0: production
      // root is vault_master (derived from the unlocked pair's secret — set by desktop
      // unlock / useDevAccount, or derived here from the dev seed); the public address is
      // passed only as the LEGACY seed so existing ciphertexts auto-migrate. Mock keeps the legacy
      // address root; extension-injector path is unused in production (see docs/WALLET.md).
      // CN: 本地库与跨设备 blob 的按账户 KDF 根。§5.0：生产根为 vault_master（派生自已解锁
      // pair 的 secret——由 WalletGate 桌面解锁 / dev useDevAccount 写入）；公开地址仅作**旧根**
      // 种子传入。mock 沿用旧地址根；生产主路径不使用扩展注入器（见 docs/WALLET.md）。
      if (config.useMock) {
        keyVault.initForTest(selfAddress);
      } else {
        let master = getVaultMaster();
        if (!master && mode === "dev" && config.devWallet) {
          master = await deriveVaultMasterFromSuri(config.devSeed);
          setVaultMaster(master);
        }
        if (master) {
          keyVault.init(master, { legacySeed: selfAddress });
        } else {
          console.warn(
            "[nexchat] no vault_master (wallet not unlocked?) — falling back to the " +
              "legacy address-derived KeyVault root; cloud sync blobs stay address-keyed (§5.0)",
          );
          keyVault.initForTest(selfAddress);
        }
      }

      let platformMuted = false;
      try {
        platformMuted = await chainClient.isAccountMuted(selfAddress);
      } catch (e) {
        console.warn("[nexchat] chat_isAccountMuted failed (node down?):", e);
      }

      const endpointId = globalThis.crypto?.randomUUID?.() ?? `ep-${Math.random()}`;
      relayEndpointId = endpointId;
      let relayConnected = !networkRelayRequired();
      try {
        await relayClient.connect(endpointId, selfAddress);
        relayConnected = isNetworkRelayConnected();
      } catch (e) {
        console.warn("[nexchat] relay connect failed (start relay:server?):", e);
        relayConnected = false;
      }
      if (networkRelayRequired() && !relayConnected) {
        console.warn("[nexchat] network relay unavailable — read-only until reconnect");
      }
      wireRelayConnectivityWatch(set);
      relayClient.onMessage((frame) => void handleInbound(frame));

      contactExchange = new ContactRequestExchange({
        selfAddress,
        selfLabel: nickname?.trim() || shortAddress(selfAddress),
        endpointId,
        relay: relayClient,
        onChange: (rows) =>
          set({
            contactRequests: rows,
            contactRequestBadge: pendingInboundCount(rows),
          }),
        onAutoAccept: async (peer, label) => {
          await get().addContact(peer, label, { notify: false });
        },
        onEnsureHandshake: (peer) => kickDirectMlsHandshake(peer, set, get),
      });
      contactExchange.wire();

      chatMailboxSync = new ChatMailboxSync({
        selfAddress,
        onFrame: handleInbound,
        beforeSync: async () => {
          const peers = useAppStore.getState().userContacts.map((m) => m.address);
          await waitMlsReadyForPeers(selfAddress, peers, 20_000);
        },
      });
      chatMailboxSync.wire();

      // EN: U1 — surface relay frame NACKs (e.g. rate limited) so a dropped outbound message is
      // marked failed instead of silently appearing "sent". Wired once (transport is a singleton).
      // CN: U1——把 relay 帧 NACK（如限流）暴露出来，让被丢弃的出站消息标记为失败而非静默显示
      // 「已发送」。仅注册一次（传输层为单例）。
      if (!relayRejectWired) {
        relayRejectWired = true;
        relayClient.onReject?.((reject) => {
          if (!reject.dedupKey) return;
          // EN: dedupKey = `${convId}:${clientMsgId}`; clientMsgId carries no ':' so split last.
          // CN: dedupKey = `${convId}:${clientMsgId}`；clientMsgId 不含 ':'，按最后一个 ':' 切分。
          const cut = reject.dedupKey.lastIndexOf(":");
          if (cut < 0) return;
          const convId = reject.convId ?? reject.dedupKey.slice(0, cut);
          const clientMsgId = reject.dedupKey.slice(cut + 1);
          void (async () => {
            await localStore.updateMessage(convId, clientMsgId, { status: "failed" });
            if (get().activeConvId === convId) {
              set({ messages: await localStore.listMessages(convId) });
            }
            const hint = frameRejectHint(reject.reason);
            set({ error: hint });
          })();
        });
      }

      if (useChainCp) {
        // EN: real on-chain DS/AS handshake + OpenMLS persistence (keyed by the account).
        // CN: 真实链上 DS/AS 握手 + OpenMLS 持久化（以账户为键）。
        // EN: Group Wire-ification (§17): when on, the group engine is a per-device Wire engine and
        // the Track A escrow / handoff / vault-backup paths are OFF (no read-only state to recover).
        // CN: 群 Wire 化（§17）：开启后群引擎为每设备 Wire 引擎，轨 A 托管 / 交接 / vault 备份路径关闭
        // （无只读态需恢复）。
        const groupWireMode = config.wireGroupMultileafEnabled;
        const groupVaultActive = config.mlsVaultEnabled && !groupWireMode;
        // EN: Track A cold start — when IndexedDB has no MLS snapshot, pull the escrow vault
        // BEFORE init mints a fresh signing client (otherwise restoreMlsVault later no-ops).
        // CN: 路线 A 冷启动——IndexedDB 无 MLS 快照时，在 init 生成新签名客户端**之前**拉取托管
        // vault（否则后续 restoreMlsVault 会因 canExportEscrow 直接跳过）。
        if (groupVaultActive) {
          const vaultOk = await coldStartMlsVaultRestore(selfAddress).catch((e) => {
            console.warn("[nexchat] cold-start MLS vault restore failed:", e);
            return false;
          });
          if (vaultOk) console.info("[nexchat] MLS vault cold-start restore ok (read-only)");
        }
        if (groupWireMode) {
          // EN: device-distinct group engine (own signer + E2EI binding); separate `gwire:` snapshot so
          // it never mixes with the Track A account/group snapshot. CN: 设备区分群引擎（自有 signer +
          // E2EI 绑定）；独立 `gwire:` 快照，绝不与轨 A 账户/群快照混淆。
          const groupDeviceId = endpointId.slice(0, 8);
          groupWireDeviceId = groupDeviceId;
          await openMlsEngine.init(deviceLeafIdentity(selfAddress, groupDeviceId), `gwire:${selfAddress}`);
          try {
            const leafKey = openMlsEngine.signaturePublicKey();
            const sig = signRawWithAccountKey(leafKeyBindingBytes(selfAddress, groupDeviceId, leafKey));
            if (sig) openMlsEngine.setLeafBinding(sig);
          } catch (e) {
            console.warn("[nexchat] group wire E2EI leaf binding setup skipped:", e);
          }
        } else {
          await openMlsEngine.init(selfAddress, selfAddress);
        }
        // EN: 1:1 Wire multi-leaf — bring up a SEPARATE device-distinct engine for direct chats
        // (persistKey `wire:{account}` so it never clobbers the account/group snapshot). Its leaf
        // credential is `{account}#{device}`, enabling per-device 1:1 add/remove. CN: 1:1 Wire 多
        // leaf——为私聊启动**独立**的设备区分引擎（persistKey `wire:{account}`，绝不覆盖账户/群快照）。
        // 其 leaf 凭证为 `{account}#{device}`，支持按设备 1:1 增删。
        if (config.wireMultileafEnabled) {
          const deviceId = endpointId.slice(0, 8);
          wireDeviceId = deviceId;
          wireEngine = new OpenMlsEngine();
          await wireEngine.init(deviceLeafIdentity(selfAddress, deviceId), `wire:${selfAddress}`);
          // EN: install the E2EI device-leaf credential (§3.9 phase 2) so EVERY wire KeyPackage carries
          // an in-MLS, relay-trustless account binding: the account SS58 key signs this device's stable
          // leaf signature key. Any add path then verifies ownership straight from the KeyPackage.
          // Skipped (relay account-auth stamp still gates) when no account signer can sign raw bytes.
          // CN: 安装 E2EI 设备 leaf 凭证（§3.9 二阶段），使**每个** wire KeyPackage 携带 MLS 内、
          // relay-trustless 的账户绑定：账户 SS58 钥签名本设备稳定 leaf 签名钥。任一 add 路径即可直接从
          // KeyPackage 验证归属。无法裸签的账户签名者下跳过（仍由 relay 账户盖章把关）。
          try {
            const leafKey = wireEngine.signaturePublicKey();
            const sig = signRawWithAccountKey(leafKeyBindingBytes(selfAddress, deviceId, leafKey));
            if (sig) wireEngine.setLeafBinding(sig);
          } catch (e) {
            console.warn("[nexchat] wire E2EI leaf binding setup skipped:", e);
          }
        }
        groupInviteExchange = new GroupInviteExchange({
          selfAddress,
          selfLabel: nickname?.trim() || shortAddress(selfAddress),
          endpointId,
          relay: relayClient,
          engine: openMlsEngine,
          chain: chainClient,
          onChange: (rows) =>
            set({ groupInvites: rows, groupInviteBadge: rows.length }),
          onSynced: () => {
            void get().refreshConversations();
            set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
          },
        });
        groupInviteExchange.wire();
        const roster = await Promise.all(
          config.mlsRosterSeeds.map((s) => chainClient.deriveAddress(s)),
        );
        const coordinator = new ChainMlsCoordinator({
          engine: openMlsEngine,
          chain: chainClient,
          selfAddress,
          roster,
          onStatus: (s) => set({ mls: s }),
          onGroupId: (gid) => {
            set({ mlsGroupId: gid });
            if (get().activeConvId === `g:${gid}`) void get().openConversation(`g:${gid}`);
          },
          // EN: a read-only (escrow-restored) device is EXPECTED to lack a signing key — surface its
          // recovery affordance (banner / composer), never the raw `no_signer` error as a global toast
          // (§5.4/§7.3). CN: 只读（托管恢复）设备**预期**无签名钥——通过恢复入口（横幅/输入区）引导，
          // 绝不把裸 `no_signer` 当全局错误弹出（§5.4/§7.3）。
          onError: (e) => {
            if (!isReadOnlyEscrowError(e)) set({ error: e });
          },
        });
        coordinator.start();
        void syncAllGroupMls(openMlsEngine, chainClient, selfAddress).then(() => {
          set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
          // EN: back up the (now caught-up) group state to the escrow vault for future cold devices.
          // Skipped in group Wire mode (no read-only escrow). CN: 把（已追平的）群态备份到托管 vault，
          // 供未来冷设备恢复。群 Wire 模式跳过（无只读托管）。
          if (groupVaultActive) scheduleGroupVaultBackup();
        });
        // EN: Track A group send-authority runtime (design §5.2/§7.3) — resolves whether THIS device may
        // send to groups and drives the online signing-key handoff. Only active with the escrow vault on;
        // otherwise `groupSendMode` stays `primary` (single-device behaviour). CN: 路线 A 群发送权运行时
        // （设计 §5.2/§7.3）——解析本设备是否可向群发送并驱动在线签名钥交接。仅托管 vault 开启时生效；否则
        // `groupSendMode` 恒为 `primary`（单设备行为）。
        if (groupVaultActive) {
          groupHandoff?.stop();
          groupHandoff = new GroupHandoffRuntime();
          void groupHandoff
            .start({
              account: selfAddress,
              selfDeviceId: persistentDeviceId(),
              relay: relayClient,
              engine: openMlsEngine,
              vaultMaster: getVaultMaster(),
              onChange: () => set({ groupSendMode: groupHandoff?.mode() ?? "primary" }),
              // EN: stage 2 of the online-handoff UX — the grant arrived and this device is now a sender.
              // CN: 在线交接 UX 第二段——授权到达、本设备已可发送。
              onSendAuthorityGranted: () => get().setNotice("已获得本设备发送权"),
            })
            .then(() => set({ groupSendMode: groupHandoff?.mode() ?? "primary" }))
            .catch((e) => console.warn("[nexchat][handoff] runtime start failed:", e));
        }
      } else if (config.mlsBackend === "openmls") {
        // EN: relay-simulated DS/AS for mock/offline multi-tab demos.
        // CN: mock/离线多标签页演示用 relay 模拟 DS/AS。
        await openMlsEngine.init(`${selfAddress}#${endpointId.slice(0, 8)}`);
        const coordinator = new MlsCoordinator({
          engine: openMlsEngine,
          relay: relayClient,
          endpointId,
          identity: `${nickname ?? selfAddress}#${endpointId.slice(0, 4)}`,
          groupId: config.mlsDemoGroupId,
          onStatus: (s) => {
            set({ mls: s, mlsGroupId: config.mlsDemoGroupId });
            if (s.ready && get().activeConvId === `g:${config.mlsDemoGroupId}`) {
              void get().openConversation(`g:${config.mlsDemoGroupId}`);
            }
          },
        });
        coordinator.start();
      }

      // EN: open the encrypted local DB (per account) so the message timeline + local conv
      // prefs persist across refreshes; no-op for the in-memory/mock store.
      // CN: 打开按账户的加密本地库，使消息时间线 + 本地会话偏好跨刷新保留；内存/mock 实现为空操作。
      await localStore.open?.(selfAddress);

      // EN: show restoring banner immediately; full restore runs in background (§14.6).
      // CN: 立即展示恢复中提示条；完整恢复在后台跑（§14.6）。
      let offchainSync: OffchainSyncStatus | CoordinatedRestoreResult | null = null;
      if (offchainSyncEnabled()) {
        offchainSync = {
          phase: "restoring",
          contacts: null,
          convIndex: null,
          msgArchive: null,
        };
        offchainSyncCoordinator.bind(selfAddress, localStore);
      }

      // EN: Assemble the decentralized 1:1 DR stack + 2↔3 orchestrator (design §11/§13). Best-effort:
      // a DR init failure (e.g. WASM load) must NEVER abort unlock — 1:1 falls back to MLS-Wire.
      // CN: 装配去中心化 1:1 DR 栈 + 2↔3 编排器（设计 §11/§13）。尽力而为：DR 初始化失败（如 WASM
      // 加载）绝不中断 unlock —— 1:1 回退到 MLS-Wire。
      chatStack = null;
      drPinnedPeers.clear();
      set({ drPeers: {} });
      drPrekeysPublished = false;
      if (config.drEnabled) {
        try {
          chatStack = await assembleChatStack({
            account: selfAddress,
            relay: relayClient,
            chain: chainClient,
            mlsEngine: openMlsEngine,
            endpointId,
            archivePusher: msgArchiveSyncFor(localStore),
          });
          // EN: Do NOT call `transport.attach()` — `handleInbound` owns the relay's single
          // `onMessage` slot and dispatches DR frames via `ingestFrame`. Register only the decoded
          // -message sink so DR messages land in the shared timeline. CN: 不调用 `transport.attach()`
          // ——`handleInbound` 独占 relay 单一 `onMessage` 槽并经 `ingestFrame` 分发 DR 帧。仅注册解码
          // 后消息回调，使 DR 消息进入共享时间线。
          chatStack.transport.onMessage((m) => void depositDrMessage(m));
        } catch (e) {
          console.warn("[nexchat] DR stack assembly failed; 1:1 stays MLS-Wire:", e);
          chatStack = null;
        }
      }

      if (config.deliveryTokensEnabled) {
        inboxManager = new InboxManager(localStore);
        tokenWallet = new TokenWallet(localStore);
        const exchange = new TokenExchange({
          selfAddress,
          endpointId,
          inbox: inboxManager,
          wallet: tokenWallet,
          relay: relayClient,
          onNeedTokens: async () => {},
        });
        exchange.wire();
      }

      const rosterAddrs = (
        await Promise.all(config.mlsRosterSeeds.map((s) => chainClient.deriveAddress(s)))
      ).map(canonicalAddress);
      const mentionRoster = rosterFromSeeds(config.mlsRosterSeeds, rosterAddrs);
      const selfMention =
        mentionRoster.find((m) => m.address === selfAddress) ??
        ({ ref: "me", address: selfAddress, labels: [selfAddress] } satisfies MentionMember);

      if (config.mlsBackend === "openmls") {
        directRegistry = new DirectMlsRegistry({
          // EN: 1:1 Wire routes the handshake to the device-distinct wire engine; chain is omitted so
          // KeyPackages are exchanged over the relay only (both ends' KPs come from their wire engines
          // → no cross-engine KP mismatch vs chain-published account KPs). CN: 启用 1:1 Wire 时把握手
          // 路由到设备区分 wire 引擎；省略 chain 使 KeyPackage 仅经 relay 交换（两端 KP 均出自各自 wire
          // 引擎 → 不与链上账户 KP 跨引擎失配）。
          engine: wireEngine ?? openMlsEngine,
          relay: relayClient,
          endpointId,
          selfAddress,
          chain: wireEngine ? undefined : config.useMock ? undefined : chainClient,
          onPeerStatus: (peer) => publishDirectMlsStatus(peer, set, get),
        });
        directRegistry.wire();
        const reg = directRegistry;
        // EN: Start the normal pairwise handshake for every roster/contact peer whose conv is NOT
        // graft-owned (`isGraftManaged` skips those — the Wire session owns them). On the default
        // (non-Wire) path this runs immediately; on the Wire path it is deferred until the join phase
        // settles so a fresh handshake never races/forks a graft. CN: 对每个会话**非**嫁接拥有的
        // roster/联系人对端发起常规 1:1 握手（`isGraftManaged` 跳过——Wire 会话拥有它们）。默认（非 Wire）
        // 路径立即执行；Wire 路径推迟到 join 阶段安定，使新握手不与嫁接竞争/分叉。
        const candidatePeers = () => {
          const peers = new Set<string>([
            ...rosterAddrs,
            ...loadUserRoster(selfAddress).map((m) => m.address),
          ]);
          peers.delete(selfAddress);
          return [...peers];
        };
        const ensureRosterHandshakes = () => {
          for (const addr of candidatePeers()) {
            if (reg.isGraftManaged(directMlsKey(selfAddress, addr))) continue;
            reg.ensure(addr);
          }
        };

        const dualWireMode =
          !!wireEngine &&
          useChainCp &&
          config.wireGroupMultileafEnabled;
        const unifiedWireBridge: {
          direct: DirectWireSession | null;
          group: GroupWireSession | null;
        } = { direct: null, group: null };
        if (dualWireMode && wireEngine) {
          unifiedWireCoordinator = createUnifiedWireAccountCoordinator({
            relay: relayClient,
            account: selfAddress,
            deviceId: endpointId.slice(0, 8),
            endpointId,
            directExecutor: createAddDeviceExecutor({
              engine: wireEngine,
              relay: relayClient,
              endpointId,
              selfAddress,
            }),
            getDirectBridge: () => unifiedWireBridge.direct?.joinBridge() ?? null,
            getGroupBridge: () => unifiedWireBridge.group?.joinBridge() ?? null,
            onGroupExecuteIntent: (intent) =>
              unifiedWireBridge.group?.handleExecuteIntent(intent) ?? Promise.resolve(),
          });
        }

        // EN: Gate-1 coordinator session on the wire engine — CD election + presence + add/remove/rekey
        // intent routing + multi-device join trigger. Shares the SAME wire engine the registry uses.
        // CN: wire 引擎上的闸一协调会话——CD 选举 + presence + 增删/rekey 意图路由 + 多设备加入触发。与
        // registry 用的是**同一** wire 引擎。
        if (wireEngine) {
          const wireEngineRef = wireEngine;
          wireSession = new DirectWireSession({
            engine: wireEngineRef,
            relay: relayClient,
            selfAddress,
            deviceId: endpointId.slice(0, 8),
            endpointId,
            coordinator: unifiedWireCoordinator ?? undefined,
            ownsCoordinator: !dualWireMode,
            broadcastDeviceJoinRequest: !dualWireMode,
            // EN: CD enumerates the 1:1 (`d:`) groups this engine holds for the join offer. CN: CD 为
            // join offer 枚举本引擎持有的 1:1（`d:`）群。
            listJoinableConvs: () => wireEngineRef.listGroups().filter((k) => k.startsWith("d:")),
            // EN: offered convs are graft-owned → take them off the registry so its handshake can't
            // fork the multi-leaf group. CN: 被提供的会话归嫁接拥有 → 从 registry 摘除，使其握手不能分叉
            // 多 leaf 群。
            onGraftConvs: (convIds) => {
              for (const c of convIds) reg.markGraftManaged(c);
              // EN: Only refresh UI when graft actually landed (hasGroup). Early offers mark
              // graft-managed before Welcome — publishing there overwrote ready:true with false.
              // CN: 仅在嫁接落地（hasGroup）时刷新 UI。过早 offer 在 Welcome 前就标记嫁接——那时发布会把
              // ready:true 覆盖成 false。
              for (const c of convIds) {
                if (!wireEngineRef.hasGroup(c)) continue;
                const peer = peerFromMlsKey(c, selfAddress);
                if (peer) publishDirectMlsStatus(peer, set, get);
              }
              // EN: §4.5 eventual-consistency gap refill — being grafted into a 1:1 means the peer may
              // have sent messages BEFORE our leaf was added (the "middle window"); those are not
              // MLS-decryptable here, so re-pull the archive a few times to merge the plaintext once an
              // online sibling has archived it. CN: §4.5 最终一致性空窗补齐——被嫁接进 1:1 意味着对端可能在
              // 我方 leaf 被加入**前**发过消息（「中间空窗」），此处不可 MLS 解密，故重拉 archive 数次，使在线
              // 兄弟归档后其明文被合并。
              scheduleMsgArchiveGapRefill(selfAddress, () => {
                const active = get().activeConvId;
                if (active) void localStore.listMessages(active).then((messages) => set({ messages }));
                void get().refreshConversations();
              });
            },
            // EN: join phase settled. If a sibling answered (graftConvs non-empty), those convs are
            // graft-owned and the rest are genuinely new → normal handshakes now. If NO sibling
            // answered (empty), WAIT for cloud restore so the existing-1:1 thread list is authoritative,
            // then split (§3.8): existing 1:1s → targeted peer-assisted Add (a peer holding the 1:1
            // grafts us, marking it graft-managed so the registry never forks the multi-leaf group);
            // contacts with NO existing 1:1 → normal pairwise handshake (new chat). Targeting only real
            // 1:1 peers avoids broadcasting "new device online" to every contact, and gating on restore
            // avoids a fresh handshake forking a group whose thread hadn't loaded yet. CN: join 阶段安定。
            // 若兄弟应答（graftConvs 非空），这些会话归嫁接、其余为全新 → 立即常规握手。若**无**兄弟应答
            // （空），**等待**云恢复使已有 1:1 线索表权威，再切分（§3.8）：已有 1:1 → 定向对端代 Add（持有
            // 该 1:1 的对端嫁接我们并标记 graft-managed，registry 绝不分叉多 leaf 群）；无 1:1 的联系人 →
            // 常规 1:1 握手（新会话）。仅触达真实 1:1 对端，避免向每个联系人广播「新设备上线」；等待恢复避免
            // 新握手分叉一个线索尚未加载的群。
            onJoinSettled: (graftConvs) => {
              void (async () => {
                await awaitRestoreSettled(get, WIRE_JOIN_RESTORE_WAIT_MS);
                const session = wireSession;
                const legacyPeers = legacyDirectPeersForWireMigration(
                  openMlsEngine,
                  wireEngineRef,
                  selfAddress,
                );
                const threadPeers = mergeWireJoinThreadPeers(
                  get()
                    .conversations.filter((c) => c.kind === "direct" && c.peer)
                    .map((c) => c.peer!),
                  legacyPeers,
                );
                const graftSet = new Set(graftConvs);
                const plan = planWireJoinTargets({
                  self: selfAddress,
                  contacts: candidatePeers(),
                  threadPeers,
                  isGraftManaged: (peer) => {
                    const conv = directMlsKey(selfAddress, peer);
                    return reg.isGraftManaged(conv) || graftSet.has(conv);
                  },
                });
                if (legacyPeers.length > 0) {
                  console.info("[nexchat][wire] legacy account-engine 1:1 → wire re-handshake", {
                    legacyPeers,
                    peerAssist: plan.peerAssist,
                    registry: plan.registry,
                  });
                } else {
                  console.info("[nexchat][wire] join settled", {
                    grafted: graftConvs.length,
                    peerAssist: plan.peerAssist,
                    registry: plan.registry,
                  });
                }
                for (const peer of plan.peerAssist) {
                  const conv = directMlsKey(selfAddress, peer);
                  if (session?.adoptRestoredGroup(conv)) {
                    console.info("[nexchat][wire] adopt restored 1:1 group (skip re-graft)", { conv });
                    continue;
                  }
                  void session?.requestPeerAdd(peer);
                }
                for (const peer of plan.registry) reg.ensure(peer);
                if (graftConvs.length > 0) ensureRosterHandshakes();
              })();
            },
            // EN: peer-assist cold-start fallback (§3.8) — the targeted peer holds no wire group yet
            // (e.g. BOTH devices enabling Wire for a pre-existing 1:1, or the peer offline), so it
            // cannot graft us. Cold-establish the wire 1:1 via the deterministic pairwise handshake on
            // the SAME wire engine (owner = smaller SS58 → no fork even if both sides fall back).
            // CRITICAL: if we ALREADY hold a local group for this conv, do NOT touch it — it may be a
            // healthy group restored from persistence (in-memory `graftedConvs` is empty after a
            // restart, so the timer can fire for a conv we are genuinely a member of). Forgetting +
            // re-establishing it here would FORK: we'd send on a fresh group the peer is not in
            // ("can send but peer never receives"). Only cold-establish when there is truly no group;
            // a stale/broken group is healed reactively on a decrypt failure (inbound recover path),
            // which has positive evidence the group is broken. CN: 对端代 Add 冷启动回退（§3.8）——目标
            // 对端尚无 wire 群（如双方都为既有 1:1 首次启用 Wire，或对端离线），无法嫁接我们。用同一 wire
            // 引擎上的确定性 1:1 握手冷启动建群（owner = 较小 SS58 → 即便双方都回退也不分叉）。**关键**：若
            // 本地**已持有**该会话的群，绝不触碰——它可能是从持久化恢复的健康群（重启后内存态 `graftedConvs`
            // 为空，故定时器会对我们确实在内的会话触发）。此处删群重建会**分叉**：我们在对端不在的新群里发送
            // （「能发但对方永远收不到」）。仅在确实无群时冷启动；残留/损坏的群由解密失败时的入站恢复路径
            // 反应式修复（那时有该群确已损坏的实证）。
            onPeerAddTimeout: (peer, conv) => {
              if (wireEngineRef.hasGroup(conv)) {
                console.info("[nexchat][wire] cold-establish skipped (group already held)", { conv });
                return;
              }
              if (reg.isGraftManaged(conv)) {
                console.info("[nexchat][wire] graft never landed — unmark and cold-establish", {
                  conv,
                  peer,
                });
                reg.unmarkGraftManaged(conv);
              }
              console.info("[nexchat][wire] cold-establish via reg.ensure", { conv, peer });
              reg.ensure(peer);
            },
          });
          if (dualWireMode) unifiedWireBridge.direct = wireSession;
          wireSession.start(!dualWireMode);
          void wireSession.announceJoin();
        } else {
          ensureRosterHandshakes();
        }
        relayClient.onConnect?.(() => {
          directRegistry?.onRelayConnected();
          retryDirectMlsForAllContacts(set, get);
          if (config.wireMultileafEnabled) scheduleRelayWireProbe(selfAddress, set);
          if (dualWireMode && unifiedWireCoordinator) {
            unifiedWireCoordinator.onRelayConnected();
          } else {
            wireSession?.onRelayConnected();
            groupWireSession?.onRelayConnected();
          }
        });

        // EN: Group Wire-ification live session (CHAT_GROUP_WIREIFY_DESIGN §17 / G7) — when dual Wire mode
        // is on, shares `unifiedWireCoordinator` with the 1:1 session (ONE CD election + ONE join offer).
        // CN: 群 Wire 化实时会话（设计 §17 / G7）——双 Wire 模式时与 1:1 会话共享 `unifiedWireCoordinator`
        // （**一次** CD 选举 + **一次** join offer）。
        if (useChainCp && config.wireGroupMultileafEnabled) {
          groupWireSession = new GroupWireSession({
            engine: openMlsEngine,
            relay: relayClient,
            chain: chainClient,
            selfAddress,
            deviceId: groupWireDeviceId ?? endpointId.slice(0, 8),
            endpointId,
            coordinator: unifiedWireCoordinator ?? undefined,
            ownsCoordinator: !dualWireMode,
            broadcastDeviceJoinRequest: !dualWireMode,
            listJoinableGroups: () =>
              openMlsEngine.listGroups().filter((k) => k.startsWith("g:")),
            syncGroupEpoch: async (convId) => {
              const gid = Number(convId.slice(2));
              if (!Number.isInteger(gid)) return;
              await syncGroupChainEpoch(openMlsEngine, chainClient, gid);
            },
            // EN: member predicate (§6.4 / §8.4 authz) — is `account` already a leaf-holder of this group
            // locally? Group device changes graft devices of EXISTING member accounts only. CN: 成员判定
            // （§6.4 / §8.4 鉴权）——`account` 是否已在本地群持有 leaf？群设备变更只嫁接**既有成员账户**的设备。
            isGroupMember: (convId, account) => {
              try {
                const acct = canonicalAddress(account);
                return openMlsEngine
                  .memberIdentities(convId)
                  .some((id) => accountFromLeafIdentity(id) === acct);
              } catch {
                return false;
              }
            },
            // EN: §8.1 lazy Add — only graft ACTIVE groups at join-offer time; dormant ones wait for
            // `activateGroup` when the user opens them. CN: §8.1 延迟 Add——join offer 时只嫁接**活跃**群；休眠群
            // 待用户打开时经 `activateGroup` 再嫁接。
            isGroupActive: (convId) => isWireGroupActive(convId, wireGroupActivityContext()),
            onGroupGrafted: (convId) => {
              useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1, error: null }));
              if (useAppStore.getState().activeConvId === convId) {
                void useAppStore.getState().openConversation(convId);
              }
              // EN: §4.5 eventual-consistency gap refill — grafted into a group means other members may
              // have sent messages BEFORE our leaf was added; those are not MLS-decryptable here, so re-pull
              // the archive on a bounded schedule (same as 1:1 Wire graft). CN: §4.5 最终一致性空窗补齐——被
              // 嫁接进群意味着其它成员可能在我方 leaf 加入**前**已发消息；此处不可 MLS 解密，故按有界时刻表重拉
              // archive（与 1:1 Wire 嫁接相同）。
              scheduleMsgArchiveGapRefill(selfAddress, () => {
                const active = get().activeConvId;
                if (active) void localStore.listMessages(active).then((messages) => set({ messages }));
                void get().refreshConversations();
              });
            },
            // EN: No-sibling join settle (§8.4) — CD did not graft us; after restore, ask online members
            // to peer-add our leaf into member groups we do not hold yet (active now; dormant on open).
            // CN: 无兄弟 join 安定（§8.4）——CD 未嫁接我们；恢复安定后，请在线成员 peer-add 我们尚未持有的成员群
            // （活跃群立即；休眠群待打开时 `ensureGraftOrPeerAdd`）。
            onJoinSettled: (graftConvs) => {
              if (graftConvs.length > 0) return;
              void (async () => {
                await awaitRestoreSettled(get, WIRE_JOIN_RESTORE_WAIT_MS);
                const session = groupWireSession;
                if (!session) return;
                const memberGroups = get()
                  .conversations.filter((c) => c.kind === "group")
                  .map((c) => `g:${c.groupId}`);
                const ctx = wireGroupActivityContext();
                const plan = planWireGroupJoinSettle({
                  memberGroups,
                  isHeld: (c) => {
                    try {
                      return openMlsEngine.hasGroup(c);
                    } catch {
                      return false;
                    }
                  },
                  isActive: (c) => isWireGroupActive(c, ctx),
                });
                console.info("[nexchat][group-wire] join settled (no sibling)", plan);
                for (const conv of plan.peerAssist) void session.requestGroupPeerAdd(conv);
              })();
            },
            // EN: Peer-add cold-start fallback (§8.4) — no online member grafted us in time. Re-broadcast
            // `peer_add_req` so the next member to come online can graft us (history remains readable via
            // archive meanwhile). Skip if we already hold the group. External Commit remains optional.
            // CN: peer-add 冷启动回退（§8.4）——窗口内无在线成员嫁接。重广播 `peer_add_req`，供下次上线的成员
            // 嫁接（其间历史仍可读 archive）。已持群则跳过。External Commit 仍可选。
            onPeerAddTimeout: (groupConvId) => {
              try {
                if (openMlsEngine.hasGroup(groupConvId)) return;
              } catch {
                return;
              }
              console.info("[nexchat][group-wire] peer-add timeout → retry broadcast", {
                conv: groupConvId,
              });
              void groupWireSession?.requestGroupPeerAdd(groupConvId);
            },
          });
          if (dualWireMode) unifiedWireBridge.group = groupWireSession;
          groupWireSession.start(!dualWireMode);
          void groupWireSession.announceJoin();
          if (dualWireMode && unifiedWireCoordinator) {
            unifiedWireCoordinator.wire();
            unifiedWireCoordinator.onRelayConnected();
            void unifiedWireCoordinator.sendDeviceJoinRequest();
          }
        }
      }

      startEphemeralScheduler((hits) => {
        const st = get();
        const active = st.activeConvId;
        if (active && hits.some((h) => h.convId === active)) {
          void localStore.listMessages(active).then((messages) => set({ messages }));
        }
        void get().refreshConversations();
      });

      startSenderMediaRetentionScheduler();

      const userContacts = loadUserRoster(selfAddress);
      set({
        mentionRoster,
        userContacts,
        selfMention,
        ...syncContactRequestState(selfAddress),
        offchainSync,
        relayConnected,
        account: {
          account: selfAddress,
          nickname,
          keyPackagesAvailable: kp,
          inboxRegistered: config.useMock,
          platformMuted,
        },
      });
      // EN: Initial conversation paint is fire-and-forget — it must NOT block (or abort via a
      // chain error) the post-unlock background work, which owns the cloud-restore lifecycle and
      // the "restoring" banner. refreshConversations runs again inside that work after restore.
      // CN: 初次会话列表刷新改为不阻塞——绝不能阻塞/因链错误中断解锁后台任务（它负责云恢复生命周期
      // 与"恢复中"横幅）。该任务内部在恢复后会再次刷新会话。
      void get().refreshConversations();
      scheduleOffchainSync(selfAddress);
      void runPostUnlockBackgroundWork(selfAddress, useChainCp, set, get);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      const hint =
        msg.includes("relay") || msg.includes("WebSocket")
          ? "（提示：可先运行 npm run relay:server，或暂时注释 .env 中 VITE_RELAY_WS）"
          : msg.includes("RPC") || msg.includes("fetch") || msg.includes("HTTP")
            ? "（提示：请先启动 ./target/release/nexus-node --dev）"
            : "";
      set({ error: msg + hint });
    } finally {
      set({ loading: false });
    }
  },

  async refreshConversations() {
    const account = get().account;
    if (!account) return;
    let local: Awaited<ReturnType<typeof localStore.listLocalConvs>> = [];
    try {
      local = await localStore.listLocalConvs();
    } catch (e) {
      console.warn("[nexchat] local conv list failed:", e);
    }
    // EN: Chain list is best-effort — when RPC is slow/unreachable, degrade to local-only
    // conversations instead of throwing (a throw here used to abort unlock() before the cloud
    // restore task ran, stranding the "restoring" banner). CN: 链上列表尽力而为——RPC 慢/不可达
    // 时退化为仅本地会话，而非抛错（此处抛错曾中断 unlock() 在云恢复任务前，导致"恢复中"横幅卡死）。
    let onChain: Awaited<ReturnType<typeof chainClient.listConversations>> = [];
    try {
      onChain = await chainClient.listConversations(account.account);
    } catch (e) {
      console.warn("[nexchat] chain conv list failed (using local only):", e);
    }
    for (const row of onChain) {
      if (row.kind === "group" && row.memberCount === 0 && row.groupId != null) {
        try {
          await localStore.removeLocalConversation(`g:${row.groupId}`);
        } catch (e) {
          console.warn("[nexchat] purge empty group local state failed:", row.groupId, e);
        }
      }
    }
    if (onChain.some((r) => r.kind === "group" && r.memberCount === 0)) {
      try {
        local = await localStore.listLocalConvs();
      } catch (e) {
        console.warn("[nexchat] local conv list failed after empty-group purge:", e);
      }
    }
    let conversations = mergeConversations(onChain, local, blockToTime);
    const hidden = await loadDeletedConvIds(localStore);
    if (hidden.size > 0) {
      conversations = conversations.filter((c) => !hidden.has(c.convId));
    }
    const peerAvatars = await fetchPeerAvatarMap(
      conversations.filter((c) => c.kind === "direct" && c.peer).map((c) => c.peer!),
    );
    if (peerAvatars.size > 0) {
      conversations = conversations.map((c) => {
        if (c.kind !== "direct" || !c.peer || c.avatarCid) return c;
        const cid = peerAvatars.get(canonicalAddress(c.peer));
        return cid ? { ...c, avatarCid: cid } : c;
      });
    }
    set({ conversations, badge: appUnreadBadge(conversations) });
    if (!config.useMock && config.mlsControlPlane === "chain") {
      void refreshAdminJoinRequestCounts(conversations).then((counts) =>
        set({ groupJoinRequestCounts: counts }),
      );
    }
    scheduleOffchainSync(account.account);
  },

  closeConversation() {
    set({ activeConvId: null, replyingTo: null, forwardingFrom: null });
  },

  async requestGroupSendAuthority() {
    if (!groupHandoff) return;
    try {
      await groupHandoff.requestSendAuthority();
      set({
        error: null,
        groupSendMode: groupHandoff.mode(),
      });
      // EN: stage 1 of the online-handoff UX — request broadcast; authority arrives async via the grant
      // (stage 2 fires from the runtime's onSendAuthorityGranted). CN: 在线交接 UX 第一段——申请已广播；
      // 发送权经 grant 异步到达（第二段由运行时 onSendAuthorityGranted 触发）。
      get().setNotice("申请已发送，等待主设备授权…");
    } catch (e) {
      console.warn("[nexchat][handoff] request failed:", e);
      set({ error: "申请发送权失败，请稍后重试" });
    }
  },

  async createSigningPinBackup(pin: string) {
    if (!signingPinBackupActive()) throw new Error("PIN 签名备份未启用");
    const account = get().account?.account;
    if (!account) throw new Error("请先解锁钱包");
    if (!openMlsEngine.canExportEscrow()) {
      throw new Error("当前设备为只读，无法备份签名钥");
    }
    const bundle = openMlsEngine.exportSigningKeys();
    const prev = readLocalMlsSigningPointer(account);
    const backupSeq = nextSigningBackupSeq(prev);
    const ptr = await pushSigningPinBackup({
      account,
      deviceId: persistentDeviceId(),
      backupSeq,
      pin,
      signingBundle: bundle,
    });
    offchainSyncCoordinator.markDirty("mls_signing");
    return ptr;
  },

  async restoreSigningPinBackup(pin: string) {
    if (!signingPinBackupActive()) throw new Error("PIN 签名备份未启用");
    const account = get().account?.account;
    if (!account) throw new Error("请先解锁钱包");
    if (openMlsEngine.canExportEscrow()) throw new Error("本设备已有签名钥");
    await restoreSigningPinBackup({
      account,
      selfDeviceId: persistentDeviceId(),
      pin,
      engine: openMlsEngine,
    });
    await ensureChainKeyPackagePublished(openMlsEngine, chainClient, account);
    await groupHandoff?.refreshAfterSigningRestored();
    set({ groupSendMode: groupHandoff?.mode() ?? "primary", error: null });
    scheduleGroupVaultBackup();
  },

  async openConversation(convId) {
    await clearConversationDeleted(localStore, convId);
    set({ activeConvId: convId, replyingTo: null });
    const peer = peerFromDirectConvId(convId);
    // EN: Stack decision (§20): a DR-pinned 1:1 skips the MLS handshake entirely; otherwise ensure
    // the MLS-Wire session and (best-effort) negotiate the stack for next time. CN: 栈决策（§20）：
    // DR 钉定的 1:1 完全跳过 MLS 握手；否则建 MLS-Wire 会话并（尽力）协商栈以备下次。
    if (peer && !isDrPeer(peer)) {
      if (directRegistry) directRegistry.ensure(peer);
      if (chatStack) void pinConvStack(peer);
    }
    const account = get().account;
    if (peer && account) prefetchDeliveryTokens(peer, account.account);
    void chatMailboxSync?.syncInbox();

    if (
      convId.startsWith("g:") &&
      account &&
      config.mlsBackend === "openmls" &&
      config.mlsControlPlane === "chain" &&
      !config.useMock
    ) {
      const gid = Number(convId.slice(2));
      if (Number.isFinite(gid)) {
        try {
          if (config.wireGroupMultileafEnabled) {
            await ensureGroupWireGraftReady(convId);
          }
          const result = await ensureGroupMlsReady({
            engine: openMlsEngine,
            chain: chainClient,
            selfAddress: account.account,
            groupId: gid,
          });
          if (result.ok) {
            set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1, error: null }));
            scheduleGroupVaultBackup();
          } else if (!openMlsEngine.hasGroup(convId)) {
            set({ error: ensureGroupMlsErrorMessage(result) });
          }
        } catch (e) {
          console.warn("[nexchat] ensureGroupMlsReady failed:", e);
        }
      }
    }

    await localStore.armEphemeralOnRead(convId, Date.now());
    let messages: MessageVM[] = [];
    try {
      messages = await localStore.listMessages(convId);
    } catch (e) {
      console.warn("[nexchat] message list failed:", e);
      set({ error: "本地消息解密失败，可尝试清除站点数据后重试" });
    }
    set({ messages });
    await localStore.markRead(convId);
    await localStore.markMentionsRead(convId);
    await get().refreshConversations();
  },

  async sendMessage(text) {
    const {
      activeConvId,
      replyingTo,
      forwardingFrom,
      ephemeralMs,
      mentionRoster,
      userContacts,
      account,
    } = get();
    const mentionRosterAll = mergeRosters(
      mentionRoster,
      userContacts,
      account?.account,
    );
    if (!activeConvId || !account) return;
    if (blockSendWithoutRelay(set)) return;
    if (config.mlsBackend === "openmls" && !isMlsReady(activeConvId, get())) {
      set({
        error: activeConvId.startsWith("d:")
          ? "1:1 MLS 握手尚未完成，请稍候…"
          : "MLS 群握手尚未完成，请稍候…",
      });
      return;
    }
    // EN: Track A send-authority guard (design §5.4) — a read-only (escrow-restored / handed-off-away)
    // device CANNOT encrypt to a group (the engine has no signing key); attempting it would throw. Block
    // with an actionable hint instead and let the UI offer "send on this device" (online handoff). Only
    // engaged when the escrow vault is on; otherwise `groupSendMode` is always `primary`. CN: 路线 A 发送
    // 权守卫（设计 §5.4）——只读（托管恢复/已交出）设备无法向群加密（引擎无签名钥），强发会抛错。改为给出可
    // 操作提示并由 UI 提供「在此设备发送」（在线交接）。仅托管 vault 开启时生效；否则 `groupSendMode` 恒
    // `primary`。
    if (
      activeConvId.startsWith("g:") &&
      !config.wireGroupMultileafEnabled &&
      get().groupSendMode !== "primary"
    ) {
      set({
        error:
          get().groupSendMode === "restoring"
            ? "正在恢复本设备的群发送权，请稍候…"
            : "此设备为只读（已从云端恢复群聊）。请在原设备发送，或点「在此设备发送」申请发送权。",
      });
      return;
    }
    const peer = peerFromDirectConvId(activeConvId);
    const clientMsgId = `c-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const isGroup = activeConvId.startsWith("g:");
    const mentionRefs = isGroup
      ? resolveMentions(parseMentionTokens(text), mentionRosterAll)
      : [];
    const bodyText = forwardingFrom
      ? text.length > 0
        ? text
        : forwardBodyText(forwardingFrom)
      : text;
    const sentAt = Date.now();
    const env = stampEnvelopeSentAt(
      textEnvelope(clientMsgId, bodyText, {
        replyTo: replyingTo?.clientMsgId,
        forward: forwardingFrom
          ? {
              fromMsg: forwardingFrom.clientMsgId,
              fromConv: forwardingFrom.convId,
              preview: forwardPreview(forwardingFrom),
            }
          : undefined,
        mentions: mentionRefs.length > 0 ? mentionRefs : undefined,
        ephemeralMs: ephemeralMs ?? undefined,
      }),
      sentAt,
    );
    const eph = ephemeralFromEnvelope(env);

    const optimistic: MessageVM = {
      clientMsgId,
      convId: activeConvId,
      senderRef: "me",
      isOutgoing: true,
      sentAt,
      content: { type: "text", text: bodyText },
      replyTo: replyingTo?.clientMsgId,
      forwardFrom: forwardingFrom
        ? {
            msgId: forwardingFrom.clientMsgId,
            convId: forwardingFrom.convId,
            preview: forwardPreview(forwardingFrom),
          }
        : undefined,
      mentions: mentionRefs,
      ephemeralTtlMs: eph.ephemeralTtlMs,
      ephemeralBurnOn: eph.ephemeralBurnOn,
      ephemeralBurnAt:
        eph.ephemeralTtlMs && eph.ephemeralBurnOn
          ? burnAtOnCreate(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
          : undefined,
      starred: false,
      status: "pending",
      source: "offChainMls",
    };
    await localStore.appendMessage(optimistic);
    set({
      messages: await localStore.listMessages(activeConvId),
      replyingTo: null,
      forwardingFrom: null,
    });

    try {
      if (peer && tokenWallet && relayEndpointId) {
        prefetchDeliveryTokens(peer, account.account);
      }
      // EN: DR outbound branch (design §21): a DR-pinned 1:1 fans the envelope out to every peer
      // device over the Double Ratchet (no `mlsEncrypt`, no delivery-token frame). CN: DR 出站分流
      // （设计 §21）：DR 钉定的 1:1 把信封经双棘轮扇出给对端每个设备（不走 `mlsEncrypt`、不附投递令牌帧）。
      const drOut = await drSend(activeConvId, env);
      if (drOut.dr) {
        optimistic.status = drOut.delivered ? "sent" : "failed";
        if (!drOut.delivered) set({ error: "对方暂无可送达的设备（DR）" });
      } else {
        const ciphertext = await mlsEncrypt(activeConvId, env, account.account);
        let frame: RelayFrame = {
          convId: activeConvId,
          senderRef: account.account,
          ciphertextB64: bytesToB64(ciphertext),
          dedupKey: relayFrameDedupKey(activeConvId, clientMsgId),
          expiresAt:
            eph.ephemeralTtlMs && eph.ephemeralBurnOn
              ? relayExpiresAt(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
              : undefined,
        };
        if (peer && tokenWallet) {
          frame = await attachDelivery(frame, peer, account.account, tokenWallet);
        }
        await relaySendFrame(frame, activeConvId);
        optimistic.status = "sent";
      }
    } catch (e) {
      optimistic.status = "failed";
      set({
        error: isReadOnlyEscrowError(e)
          ? "此设备为只读（已从云端恢复）。请在原设备发送，或点「在此设备发送」申请发送权。"
          : String(e),
      });
    }
    await syncMessageToStore(activeConvId, optimistic);
    scheduleRefreshConversations();
    if (account) scheduleMsgArchivePush(account.account);
  },

  async sendFile(file) {
    const { activeConvId, replyingTo, ephemeralMs, account } = get();
    if (!activeConvId || !account) return;
    if (blockSendWithoutRelay(set)) return;
    if (!config.ipfsEnabled) {
      set({ error: "IPFS 未启用" });
      return;
    }
    if (config.mlsBackend === "openmls" && !isMlsReady(activeConvId, get())) {
      set({ error: "MLS 握手尚未完成，请稍候…" });
      return;
    }
    const peer = peerFromDirectConvId(activeConvId);
    const clientMsgId = `c-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const plain = new Uint8Array(await file.arrayBuffer());
    const mime = file.type || "application/octet-stream";
    const previewUrls: string[] = [];
    const trackPreview = (url?: string) => {
      if (url) previewUrls.push(url);
      return url;
    };
    let localPreviewUrl = trackPreview(createMediaPreviewUrl(file));

    const optimistic: MessageVM = {
      clientMsgId,
      convId: activeConvId,
      senderRef: "me",
      isOutgoing: true,
      sentAt: Date.now(),
      content: {
        type: "media",
        mime,
        name: file.name,
        size: file.size,
        thumbReady: !!localPreviewUrl,
        bodyReady: false,
        localPreviewUrl,
      },
      replyTo: replyingTo?.clientMsgId,
      mentions: [],
      ephemeralTtlMs: ephemeralMs ?? undefined,
      ephemeralBurnOn: ephemeralMs ? "read" : undefined,
      ephemeralBurnAt: ephemeralMs ? burnAtOnCreate(ephemeralMs, "read") : undefined,
      starred: false,
      status: "pending",
      source: "offChainMls",
    };
    await localStore.appendMessage(optimistic);
    set({
      messages: await localStore.listMessages(activeConvId),
      replyingTo: null,
    });

    try {
      const thumbPlain = await mediaThumbnail(file, config.ipfsThumbMaxPx);
      if (mime.startsWith("video/") && thumbPlain && optimistic.content.type === "media") {
        revokePreviewUrl(localPreviewUrl);
        const idx = previewUrls.indexOf(localPreviewUrl!);
        if (idx >= 0) previewUrls.splice(idx, 1);
        localPreviewUrl = trackPreview(createThumbPreviewUrl(thumbPlain));
        optimistic.content = {
          ...optimistic.content,
          localPreviewUrl,
          thumbReady: true,
        };
        await syncMessageToStore(activeConvId, optimistic);
      }
      const uploaded = await uploadEncryptedFile(plain, file.name, mime, thumbPlain, {
        ephemeral: !!ephemeralMs,
      });
      const body = fileBodyFromUpload(uploaded);
      const envType = envelopeTypeForMime(mime);
      const env = stampEnvelopeSentAt(
        fileEnvelope(clientMsgId, envType, body, {
          replyTo: replyingTo?.clientMsgId,
          ephemeralMs: ephemeralMs ?? undefined,
        }),
        optimistic.sentAt,
      );
      const eph = ephemeralFromEnvelope(env);
      optimistic.ephemeralTtlMs = eph.ephemeralTtlMs;
      optimistic.ephemeralBurnOn = eph.ephemeralBurnOn;
      optimistic.ephemeralBurnAt =
        eph.ephemeralTtlMs && eph.ephemeralBurnOn
          ? burnAtOnCreate(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
          : undefined;
      optimistic.content = {
        type: "media",
        mime: body.mime,
        name: body.name,
        size: body.size,
        thumbReady: !!body.thumbCid,
        bodyReady: true,
        rootCid: body.rootCid,
        fileKey: body.fileKey,
        thumbCid: body.thumbCid,
        thumbKey: body.thumbKey,
        chunked: body.chunked,
      };
      if (peer && tokenWallet && relayEndpointId) {
        prefetchDeliveryTokens(peer, account.account);
      }
      // EN: DR outbound branch (design §21): fan the media envelope out over the Double Ratchet for
      // a DR-pinned 1:1; otherwise MLS-Wire encrypt + relay. CN: DR 出站分流（设计 §21）：DR 钉定的
      // 1:1 把媒体信封经双棘轮扇出；否则 MLS-Wire 加密 + relay。
      const drOut = await drSend(activeConvId, env);
      if (drOut.dr) {
        if (!drOut.delivered) set({ error: "对方暂无可送达的设备（DR）" });
      } else {
        const ciphertext = await mlsEncrypt(activeConvId, env, account.account);
        let frame: RelayFrame = {
          convId: activeConvId,
          senderRef: account.account,
          ciphertextB64: bytesToB64(ciphertext),
          dedupKey: relayFrameDedupKey(activeConvId, clientMsgId),
          expiresAt:
            eph.ephemeralTtlMs && eph.ephemeralBurnOn
              ? relayExpiresAt(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
              : undefined,
        };
        if (peer && tokenWallet) {
          frame = await attachDelivery(frame, peer, account.account, tokenWallet);
        }
        await relaySendFrame(frame, activeConvId);
      }
      const gid = activeConvId.startsWith("g:")
        ? Number(activeConvId.slice(2))
        : undefined;
      if (gid != null && !eph.ephemeralTtlMs) {
        await maybePinUploadedFile(uploaded, gid);
      }
      if (!eph.ephemeralTtlMs) {
        registerUploadedMedia(uploaded, {
          clientMsgId,
          convId: activeConvId,
        });
      }
      optimistic.status = drOut.dr && !drOut.delivered ? "failed" : "sent";
      await syncMessageToStore(activeConvId, optimistic);
    } catch (e) {
      optimistic.status = "failed";
      set({
        error: isReadOnlyEscrowError(e)
          ? "此设备为只读（已从云端恢复）。请在原设备发送，或点「在此设备发送」申请发送权。"
          : String(e),
      });
      await syncMessageToStore(activeConvId, optimistic);
    } finally {
      for (const url of previewUrls) revokePreviewUrl(url);
    }
    await get().refreshConversations();
    scheduleMsgArchivePush(account.account);
  },

  async ackMediaDownloaded(convId, clientMsgId) {
    const account = get().account;
    if (!account) return;
    const peer = peerFromDirectConvId(convId);
    // EN: 1:1 only — group early-unpin would need all-member ack tracking (out of scope).
    // CN: 仅 1:1——群聊提前 unpin 需全员 ack 跟踪，不在范围内。
    if (!peer) return;
    const dedupKey = `${convId}:${clientMsgId}`;
    if (mediaAcksSent.has(dedupKey)) return;
    mediaAcksSent.add(dedupKey);
    try {
      const env = mediaAckEnvelope(newClientMsgId("ack"), clientMsgId);
      const drOut = await drSend(convId, env);
      if (!drOut.dr) {
        let frame: RelayFrame = {
          convId,
          senderRef: account.account,
          ciphertextB64: bytesToB64(await mlsEncrypt(convId, env, account.account)),
          dedupKey: relayFrameDedupKey(convId, env.id),
        };
        if (tokenWallet) {
          frame = await attachDelivery(frame, peer, account.account, tokenWallet);
        }
        await relaySendFrame(frame, convId, { echoSelf: false });
      }
    } catch (e) {
      // EN: best-effort — sender just keeps the full TTL. CN: 尽力而为，失败则发送方走完整 TTL。
      mediaAcksSent.delete(dedupKey);
      console.warn("[nexchat] media_ack send failed:", e);
    }
  },

  async keepAttachment(msg) {
    if (msg.content.type !== "media" || msg.ephemeralTtlMs) return;
    const { convId, clientMsgId } = msg;
    await localStore.updateMessage(convId, clientMsgId, { starred: true });
    if (get().activeConvId === convId) {
      set({ messages: await localStore.listMessages(convId) });
    }
    // EN: sender side — exempt from the local-pin TTL sweep (starred = keep).
    // CN: 发送方——豁免本机 pin TTL 清扫（收藏即保留）。
    if (msg.isOutgoing) exemptRetentionForMessage(convId, clientMsgId);
    if (config.useMock || !msg.content.rootCid) return;
    try {
      const gid = convId.startsWith("g:") ? Number(convId.slice(2)) : 0;
      await pinAttachmentOnChain(
        {
          rootCid: msg.content.rootCid,
          size: msg.content.size,
          thumbCid: msg.content.thumbCid,
        },
        gid,
      );
    } catch (e) {
      set({ error: `链上保留附件失败: ${String(e)}` });
    }
  },

  async deleteMessage(msg) {
    const { convId, clientMsgId } = msg;
    await localStore.deleteMessage(convId, clientMsgId);
    if (get().activeConvId === convId) {
      set({ messages: await localStore.listMessages(convId) });
    }
    scheduleRefreshConversations();
    // EN: the next archive push diffs against the last blob and tombstones the removed
    // message, so other devices delete it too. CN: 下次归档推送会与上次 blob 比对并对已删消息
    // 记墓碑，从而让其他设备一并删除。
    const account = get().account;
    if (account) scheduleOffchainSync(account.account);
  },

  async clearConversationHistory(convId) {
    await localStore.clearMessages(convId);
    if (get().activeConvId === convId) {
      set({ messages: await localStore.listMessages(convId) });
    }
    scheduleRefreshConversations();
    const account = get().account;
    if (account) scheduleOffchainSync(account.account);
  },

  async deleteConversation(convId) {
    try {
      await localStore.removeLocalConversation(convId);
    } catch (e) {
      console.warn("[nexchat] removeLocalConversation failed:", convId, e);
    }
    await markConversationDeleted(localStore, convId);
    if (get().activeConvId === convId) {
      get().closeConversation();
    }
    scheduleRefreshConversations();
    const account = get().account;
    if (account) scheduleOffchainSync(account.account);
  },

  async recallMessage(msg) {
    const account = get().account;
    if (!account) return;
    if (!canRecallMessage(msg)) {
      set({ error: "该消息无法撤回（仅限自己 2 分钟内发送的消息）" });
      return;
    }
    const { convId, clientMsgId } = msg;
    const peer = peerFromDirectConvId(convId);
    try {
      const env = stampEnvelopeSentAt(recallEnvelope(newClientMsgId("rcl"), clientMsgId), Date.now());
      const drOut = await drSend(convId, env);
      if (!drOut.dr) {
        let frame: RelayFrame = {
          convId,
          senderRef: account.account,
          ciphertextB64: bytesToB64(await mlsEncrypt(convId, env, account.account)),
          dedupKey: relayFrameDedupKey(convId, env.id),
        };
        if (peer && tokenWallet) {
          frame = await attachDelivery(frame, peer, account.account, tokenWallet);
        }
        await relaySendFrame(frame, convId);
      }
    } catch (e) {
      set({ error: `撤回失败：${String(e)}` });
      return;
    }
    await markMessageRecalled(localStore, convId, clientMsgId);
    if (get().activeConvId === convId) {
      set({ messages: await localStore.listMessages(convId) });
    }
    scheduleRefreshConversations();
    scheduleOffchainSync(account.account);
  },

  async setGroupAvatar(groupId, file) {
    if (!config.ipfsEnabled) {
      set({ error: "IPFS 未启用" });
      return;
    }
    try {
      const plain = new Uint8Array(await file.arrayBuffer());
      const cid = await ipfsClient.add(plain, file.name);
      await chainClient.setGroupProfileDev(groupId, { avatarCid: cid });
      await get().refreshConversations();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setReplyingTo(msg) {
    set({ replyingTo: msg, forwardingFrom: null });
  },

  setForwardingFrom(msg) {
    set({ forwardingFrom: msg, replyingTo: null, forwardSource: msg });
  },

  openForwardPicker(msg) {
    set({ forwardSource: msg, replyingTo: null, forwardingFrom: null });
  },

  closeForwardPicker() {
    set({ forwardSource: null });
  },

  async forwardToConversations(convIds, comment = "") {
    const { forwardSource: source, account, conversations } = get();
    if (!source || !account || convIds.length === 0) return;

    const forwardMeta = forwardEnvelopeRef(source);
    const forwardFrom = forwardMetaFrom(source);
    const commentTrim = comment.trim();
    const mediaBody = fileBodyFromMessage(source);
    const sendMedia = isMediaForwardReady(source) && mediaBody != null;

    set({ forwardSource: null });

    let sentAny = false;
    for (const convId of convIds) {
      const conv = conversations.find((c) => c.convId === convId);
      if (!conv || conv.frozen || conv.adminMuted) continue;
      if (config.mlsBackend === "openmls" && !isMlsReady(convId, get())) continue;

      if (commentTrim) {
        const clientMsgId = newClientMsgId();
        const env = textEnvelope(clientMsgId, commentTrim, { forward: forwardMeta });
        const optimistic: MessageVM = {
          clientMsgId,
          convId,
          senderRef: "me",
          isOutgoing: true,
          sentAt: Date.now(),
          content: { type: "text", text: commentTrim },
          mentions: [],
          forwardFrom,
          starred: false,
          status: "pending",
          source: "offChainMls",
        };
        await persistAndRelay(convId, env, optimistic, account);
        sentAny = true;
      }

      if (sendMedia) {
        const clientMsgId = newClientMsgId();
        const envType = envelopeTypeForMime(mediaBody.mime);
        const env = fileEnvelope(clientMsgId, envType, mediaBody, { forward: forwardMeta });
        const optimistic: MessageVM = {
          clientMsgId,
          convId,
          senderRef: "me",
          isOutgoing: true,
          sentAt: Date.now(),
          content: {
            type: "media",
            mime: mediaBody.mime,
            name: mediaBody.name,
            size: mediaBody.size,
            thumbReady: !!mediaBody.thumbCid,
            bodyReady: true,
            rootCid: mediaBody.rootCid,
            fileKey: mediaBody.fileKey,
            thumbCid: mediaBody.thumbCid,
            thumbKey: mediaBody.thumbKey,
            chunked: mediaBody.chunked,
            durationMs: mediaBody.durationMs,
          },
          forwardFrom,
          mentions: [],
          starred: false,
          status: "pending",
          source: "offChainMls",
        };
        await persistAndRelay(convId, env, optimistic, account);
        sentAny = true;
      } else if (!commentTrim) {
        const bodyText = forwardBodyText(source);
        const clientMsgId = newClientMsgId();
        const env = textEnvelope(clientMsgId, bodyText, { forward: forwardMeta });
        const optimistic: MessageVM = {
          clientMsgId,
          convId,
          senderRef: "me",
          isOutgoing: true,
          sentAt: Date.now(),
          content: { type: "text", text: bodyText },
          mentions: [],
          forwardFrom,
          starred: false,
          status: "pending",
          source: "offChainMls",
        };
        await persistAndRelay(convId, env, optimistic, account);
        sentAny = true;
      }
    }

    if (!sentAny) {
      set({ error: "无法转发：目标会话不可用或加密未就绪" });
    }
    scheduleRefreshConversations();
    await get().refreshConversations();
  },

  setEphemeral(ms) {
    set({ ephemeralMs: ms });
  },

  async setPref(convId, pref) {
    await localStore.setPref(convId, pref);
    await get().refreshConversations();
    const account = get().account;
    if (account) scheduleOffchainSync(account.account);
  },

  setPinsOpen(open) {
    set({ pinsOpen: open });
    if (open) void get().refreshPins();
  },

  async refreshPins() {
    const account = get().account;
    if (!account || config.useMock) {
      set({ pins: [] });
      return;
    }
    set({ pinsLoading: true });
    try {
      const pins = await chainClient.listMyPins(account.account);
      set({ pins });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ pinsLoading: false });
    }
  },

  async renewPin(cidHash, periods) {
    try {
      await chainClient.renewPinDev(cidHash, periods);
      await get().refreshPins();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setNewDmOpen(open) {
    set({ newDmOpen: open });
  },

  setNewGroupOpen(open) {
    set({ newGroupOpen: open });
  },

  setJoinGroupOpen(open) {
    set({ joinGroupOpen: open, ...(open ? {} : { joinPreviewGroupId: null }) });
  },

  async openJoinPreview(groupId) {
    set({ joinPreviewGroupId: groupId, joinGroupOpen: true });
  },

  async lookupGroupForJoin(groupId) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    return fetchGroupLookup(chainClient, groupId, account.account);
  },

  async publishKeyPackageForJoin() {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("发布 KeyPackage 需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await ensureChainKeyPackagePublished(openMlsEngine, chainClient, account.account);
      set((s) => ({
        account: s.account
          ? { ...s.account, keyPackagesAvailable: Math.max(1, s.account.keyPackagesAvailable) }
          : s.account,
      }));
    } catch (e) {
      const msg = chainJoinErrorMessage(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async requestJoinGroup(groupId) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("申请入群需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await ensureChainKeyPackagePublished(openMlsEngine, chainClient, account.account);
      await chainClient.requestJoin(groupId);
      await get().refreshPendingJoins(true);
    } catch (e) {
      const msg = chainJoinErrorMessage(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async cancelJoinRequestGroup(groupId) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain") {
      throw new Error("撤回申请需要链上控制面");
    }
    set({ loading: true, error: null });
    try {
      await chainClient.cancelJoinRequest(groupId);
      await get().refreshPendingJoins(true);
    } catch (e) {
      const msg = chainJoinErrorMessage(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async refreshPendingJoins(silent = false) {
    const account = get().account;
    if (!account || config.useMock || config.mlsControlPlane !== "chain") {
      set({ pendingJoins: [] });
      return;
    }
    try {
      const rows = await chainClient.listPendingJoinRequests(account.account);
      const pending: PendingJoinVM[] = [];
      for (const row of rows) {
        const flags = await chainClient.groupJoinFlags(row.groupId, account.account);
        if (flags.isMember) {
          try {
            await ensureGroupMlsReady({
              engine: openMlsEngine,
              chain: chainClient,
              selfAddress: account.account,
              groupId: row.groupId,
            });
            await localStore.ensureConv(`g:${row.groupId}`);
            scheduleGroupVaultBackup();
          } catch (e) {
            console.warn("[nexchat] pending join MLS sync:", row.groupId, e);
          }
          continue;
        }
        const profile = await chainClient.groupProfile(row.groupId);
        const title = profile?.name?.trim() || `群 #${row.groupId}`;
        pending.push({
          groupId: row.groupId,
          title,
          avatarCid: profile?.avatarCid ?? "",
          status: flags.hasJoinApproval ? "approved" : "pending",
        });
      }
      set({ pendingJoins: pending });
      if (pending.length === 0 && rows.length > 0) {
        await get().refreshConversations();
        set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      }
    } catch (e) {
      if (!silent) set({ error: e instanceof Error ? e.message : String(e) });
    }
  },

  async approveJoinRequests(groupId, applicantAddresses, opts) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("批准入群需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await approveAndCommitJoinRequests({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId,
        applicantAddresses,
        onProgress: opts?.onProgress,
      });
      await get().refreshConversations();
      await get().refreshPendingJoins(true);
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
    } catch (e) {
      const msg = chainJoinErrorMessage(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async createGroupChat(name, memberAddresses) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock) {
      throw new Error("发起群聊需要连接链上节点（VITE_USE_MOCK=false）");
    }
    if (config.mlsControlPlane !== "chain") {
      throw new Error("发起群聊需要链上 MLS 控制面（VITE_MLS_CONTROL_PLANE=chain）");
    }
    if (config.mlsBackend !== "openmls") {
      throw new Error("发起群聊需要 OpenMLS 后端（VITE_MLS_BACKEND=openmls）");
    }
    // EN: Track A — creating a group needs a FULL client (signing key + MLS create/add/commit). A
    // read-only escrow-restore device must hand off first (§5). CN: 路线 A——建群需**完整**客户端（签名钥
    // + MLS create/add/commit）。只读托管恢复设备须先交接（§5）。
    if (config.mlsVaultEnabled && !openMlsEngine.canExportEscrow()) {
      throw new Error(
        "此设备为只读（已从云端恢复），无法创建群聊。请点「在此设备发送」向原主设备申请发送权，或在原主设备上建群。",
      );
    }
    set({ loading: true, error: null });
    try {
      const gid = await createGroupWithMembers({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        name,
        memberAddresses,
      });
      await localStore.ensureConv(`g:${gid}`);
      await localStore.setConvTitle(`g:${gid}`, name);
      await get().refreshConversations();
      await get().openConversation(`g:${gid}`);
      scheduleOffchainSync(account.account);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  openGroupManage(target) {
    set({ groupManageTarget: target });
  },

  closeGroupManage() {
    set({ groupManageTarget: null });
  },

  openInviteGroupMembers(target) {
    set({ inviteGroupTarget: target });
  },

  closeInviteGroupMembers() {
    set({ inviteGroupTarget: null });
  },

  async inviteGroupMembers(memberAddresses, opts) {
    const target = get().inviteGroupTarget;
    const account = get().account;
    if (!target) throw new Error("请先选择群聊");
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("邀请成员需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      const result = await addMembersToGroup({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: target.groupId,
        groupName: target.title,
        memberAddresses,
        onProgress: opts?.onProgress,
        notifyMembers: async (gid, groupName, members) => {
          if (groupInviteExchange) {
            await groupInviteExchange.sendInvites(gid, groupName, members);
          }
        },
      });
      await get().refreshConversations();
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
      return result;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  dismissGroupInvite(inviteId) {
    groupInviteExchange?.dismiss(inviteId);
  },

  async acceptGroupInvite(inviteId) {
    const invite = get().groupInvites.find((r) => r.inviteId === inviteId);
    if (!invite) throw new Error("邀请不存在或已忽略");
    await get().refreshConversations();
    await get().openConversation(`g:${invite.groupId}`);
    get().dismissGroupInvite(inviteId);
  },

  async syncGroupInvite(inviteId) {
    const account = get().account;
    const invite = get().groupInvites.find((r) => r.inviteId === inviteId);
    if (!account) throw new Error("请先解锁账户");
    if (!invite) throw new Error("邀请不存在或已忽略");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("群邀请同步需要链上 OpenMLS 控制面");
    }
    const result = await ensureGroupMlsReady({
      engine: openMlsEngine,
      chain: chainClient,
      selfAddress: account.account,
      groupId: invite.groupId,
    });
    if (!result.ok && !openMlsEngine.hasGroup(`g:${invite.groupId}`)) {
      throw new Error(ensureGroupMlsErrorMessage(result));
    }
    set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1, error: null }));
    scheduleGroupVaultBackup();
    await get().refreshConversations();
  },

  async removeGroupMember(memberAddress, opts) {
    const target = get().groupManageTarget;
    const account = get().account;
    if (!target) throw new Error("请先选择群聊");
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("成员管理需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await removeGroupMembers({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: target.groupId,
        removeAddresses: [memberAddress],
        onProgress: opts?.onProgress,
      });
      await get().refreshConversations();
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async swapGroupMember(removeAddress, addAddress, opts) {
    const target = get().groupManageTarget;
    const account = get().account;
    if (!target) throw new Error("请先选择群聊");
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("成员管理需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await swapGroupMembers({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: target.groupId,
        removeAddresses: [removeAddress],
        addAddresses: [addAddress],
        onProgress: opts?.onProgress,
      });
      await get().refreshConversations();
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async leaveGroupChat(opts) {
    const target = get().groupManageTarget;
    const account = get().account;
    if (!target) throw new Error("请先选择群聊");
    if (!account) throw new Error("请先解锁账户");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("退群需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await leaveGroup({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: target.groupId,
        onProgress: opts?.onProgress,
      });
      await get().refreshConversations();
      if (get().activeConvId === `g:${target.groupId}`) {
        get().closeConversation();
      }
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async disbandGroupChat(opts) {
    const target = get().groupManageTarget;
    const account = get().account;
    if (!target) throw new Error("请先选择群聊");
    if (!account) throw new Error("请先解锁账户");
    if (target.myRole !== "owner") throw new Error("仅群主可以解散群");
    if (config.useMock || config.mlsControlPlane !== "chain" || config.mlsBackend !== "openmls") {
      throw new Error("解散群需要链上 OpenMLS 控制面");
    }
    set({ loading: true, error: null });
    try {
      await disbandGroup({
        engine: openMlsEngine,
        chain: chainClient,
        selfAddress: account.account,
        groupId: target.groupId,
        onProgress: opts?.onProgress,
      });
      const convId = `g:${target.groupId}`;
      try {
        await localStore.removeLocalConversation(convId);
      } catch (e) {
        console.warn("[nexchat] removeLocalConversation after disband failed:", e);
      }
      scheduleConvIndexPush(account.account);
      await get().refreshConversations();
      if (get().activeConvId === `g:${target.groupId}`) {
        get().closeConversation();
      }
      set((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
      scheduleOffchainSync(account.account);
      scheduleGroupVaultBackup();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ error: msg });
      throw e;
    } finally {
      set({ loading: false });
    }
  },

  async addContact(addressRaw, label, opts) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    const canon = parseContactAddress(addressRaw);
    if (canon === account.account) throw new Error("不能添加自己为联系人");
    const trimmedLabel = label.trim();
    if (!trimmedLabel) throw new Error("请输入显示名称");

    const saved = loadContacts(account.account);
    const existing = saved.findIndex((c) => c.address === canon);
    const isNew = existing < 0;
    if (isNew && opts?.notify !== false && contactExchange) {
      await contactExchange.sendRequest(canon);
    }

    const now = Date.now();
    const row: (typeof saved)[number] = {
      address: canon,
      label: trimmedLabel,
      addedAt: existing >= 0 ? saved[existing]!.addedAt : now,
      updatedAt: now,
    };
    if (existing >= 0) saved[existing] = row;
    else saved.push(row);
    saveContacts(account.account, saved);
    scheduleContactsVaultPush(account.account);

    const userContacts = saved.map(savedToMentionMember);
    if (directRegistry) directRegistry.ensure(canon);
    prefetchDeliveryTokens(canon, account.account);
    set({ userContacts });
  },

  async acceptContactRequest(reqId, label) {
    const account = get().account;
    if (!account) throw new Error("请先解锁账户");
    const trimmedLabel = label.trim();
    if (!trimmedLabel) throw new Error("请输入显示名称");
    const req = findRequest(loadContactRequests(account.account), reqId);
    if (!req || req.direction !== "inbound" || req.status !== "pending") {
      throw new Error("请求不存在或已处理");
    }
    await get().addContact(req.peerAddress, trimmedLabel, { notify: false });
    await contactExchange?.sendAck(req.peerAddress, reqId, "accept", trimmedLabel);
    const rows = updateRequestStatus(account.account, reqId, "accepted");
    set({
      contactRequests: rows,
      contactRequestBadge: pendingInboundCount(rows),
    });
  },

  async rejectContactRequest(reqId) {
    const account = get().account;
    if (!account) return;
    const req = findRequest(loadContactRequests(account.account), reqId);
    if (!req || req.direction !== "inbound" || req.status !== "pending") return;
    await contactExchange?.sendAck(req.peerAddress, reqId, "reject");
    const rows = updateRequestStatus(account.account, reqId, "rejected");
    set({
      contactRequests: rows,
      contactRequestBadge: pendingInboundCount(rows),
    });
  },

  async removeContact(peerAddress) {
    const account = get().account;
    if (!account) return;
    const canon = canonicalAddress(peerAddress);
    const saved = loadContacts(account.account).filter((c) => c.address !== canon);
    saveContacts(account.account, saved);
    scheduleContactsVaultPush(account.account);
    set({ userContacts: saved.map(savedToMentionMember) });
  },

  async startDirectChat(peerAddress, title) {
    const account = get().account;
    if (!account || peerAddress === account.account) return;
    try {
      const convId = directConvId(peerAddress);
      // EN: Negotiate the stack up front (§20); only ensure MLS-Wire when not DR-pinned so a DR
      // conversation never kicks off an MLS handshake. CN: 先行协商栈（§20）；仅非 DR 钉定时建
      // MLS-Wire，使 DR 会话绝不触发 MLS 握手。
      if (chatStack) await pinConvStack(peerAddress);
      if (!isDrPeer(peerAddress) && directRegistry) directRegistry.ensure(peerAddress);
      prefetchDeliveryTokens(peerAddress, account.account);
      await clearConversationDeleted(localStore, convId);
      await localStore.ensureConv(convId);
      if (title) await localStore.setConvTitle(convId, title);
      await get().refreshConversations();
      await get().openConversation(convId);
      scheduleOffchainSync(account.account);
      set({ newDmOpen: false });
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

// EN: dev-only debug hook to inspect store state from the console / CDP.
// CN: 仅开发用调试钩子，便于控制台 / CDP 检查 store 状态。
if (typeof window !== "undefined") {
  (window as unknown as { __nexchat: typeof useAppStore }).__nexchat = useAppStore;
}

// EN: Read the live per-side device roster of a 1:1 Wire conversation for the security-disclosure UX
// (design §8). Returns null when the feature is off, the conv is not a 1:1, or no wire group is held —
// callers should treat null as "nothing to disclose". Recompute on `mlsSyncRev` changes for reactivity.
// CN: 读取 1:1 Wire 会话按方分组的实时设备名册，供安全披露 UX（设计 §8）。功能关闭、非 1:1、或无 wire 群时
// 返回 null——调用方按「无可披露」处理。随 `mlsSyncRev` 变化重算以获得响应式。
export function wireDeviceRosterFor(uiConvId: string): WireDeviceRoster | null {
  if (!wireEngine) return null;
  const peer = peerFromDirectConvId(uiConvId);
  if (!peer) return null;
  const self = useAppStore.getState().account?.account;
  if (!self) return null;
  const mlsKey = directMlsKey(self, peer);
  if (!wireEngine.hasGroup(mlsKey)) return null;
  return computeWireDeviceRoster(
    wireEngine.memberIdentities(mlsKey),
    self,
    peer,
    wireDeviceId ?? undefined,
  );
}

// EN: §8.1 on-demand graft entry for UI — trigger lazy graft for a deferred group and wait for Welcome.
// CN: §8.1 按需嫁接 UI 入口——对延迟群触发懒嫁接并等待 Welcome。
export async function prepareGroupWireConversation(convId: string): Promise<boolean> {
  return ensureGroupWireGraftReady(convId);
}

// EN: Read the live device roster of a GROUP Wire conversation for the security-disclosure UX (design §9).
// The group MLS key IS the UI conv id (`g:<id>`). Returns null when group wire mode is off, the conv is
// not a group, or no group is held — callers treat null as "nothing to disclose". Recompute on
// `mlsSyncRev`. CN: 读取**群** Wire 会话的实时设备名册，供安全披露 UX（设计 §9）。群 MLS key 即 UI 会话 id
// （`g:<id>`）。群 wire 模式关闭、非群、或未持群时返回 null——调用方按「无可披露」处理。随 `mlsSyncRev` 重算。
export function wireGroupRosterFor(uiConvId: string): WireGroupRoster | null {
  if (!config.wireGroupMultileafEnabled) return null;
  if (!uiConvId.startsWith("g:")) return null;
  const self = useAppStore.getState().account?.account;
  if (!self) return null;
  try {
    if (!openMlsEngine.hasGroup(uiConvId)) return null;
    return computeWireGroupRoster(
      openMlsEngine.memberIdentities(uiConvId),
      self,
      groupWireDeviceId ?? undefined,
    );
  } catch {
    return null;
  }
}

// EN: Remove one of MY OTHER devices from a 1:1 Wire conversation (per-device PCS self-heal, design §8):
// the removed leaf can no longer decrypt FUTURE messages, without rotating the mnemonic. Refuses to
// remove the local device (would lock the user out) or a peer device (not ours to remove). Returns true
// when the remove intent was submitted (executed locally or delegated to the coordinator). CN: 把我**其他**
// 设备从 1:1 Wire 会话移除（按设备 PCS 自愈，设计 §8）：被移除 leaf 无法再解**未来**消息，且无需轮换助记词。
// 拒绝移除本机设备（会把用户锁出）或对端设备（无权移除）。提交移除意图（本地执行或委派协调器）即返回 true。
export async function removeWireDevice(uiConvId: string, deviceIdentity: string): Promise<boolean> {
  if (!wireSession || !wireEngine) return false;
  const peer = peerFromDirectConvId(uiConvId);
  if (!peer) return false;
  const self = useAppStore.getState().account?.account;
  if (!self) return false;
  const mlsKey = directMlsKey(self, peer);
  const roster = computeWireDeviceRoster(
    wireEngine.memberIdentities(mlsKey),
    self,
    peer,
    wireDeviceId ?? undefined,
  );
  // EN: only my own non-local leaves are eligible. CN: 仅我自己的非本机 leaf 可移除。
  const target = roster.self.find((d) => d.identity === deviceIdentity && !d.isThisDevice);
  if (!target) {
    useAppStore.setState({ error: "无法移除该设备（仅能移除你自己的其他设备）" });
    return false;
  }
  try {
    await wireSession.removeDevice(mlsKey, deviceIdentity);
    useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1, error: null }));
    void useAppStore.getState().refreshConversations();
    return true;
  } catch (e) {
    useAppStore.setState({ error: e instanceof Error ? e.message : "移除设备失败" });
    return false;
  }
}

// EN: Remove one of MY OTHER devices from a GROUP Wire conversation (per-device PCS self-heal, design §8 /
// §17): the removed leaf can no longer decrypt FUTURE group messages, without changing group membership
// (empty-`member_delta` chain commit) or rotating the mnemonic. The CD commits it on-chain (or a non-CD
// device delegates to the CD); other members + my other devices follow via chain sync. Refuses to remove
// the local device or another member's device. Returns true when the remove intent was submitted. CN: 把我
// **其他**设备从**群** Wire 会话移除（按设备 PCS 自愈，设计 §8 / §17）：被移除 leaf 无法再解**未来**群消息，
// 且不改群成员（空-`member_delta` 链 commit）、无需轮换助记词。由 CD 上链提交（非 CD 设备委派给 CD）；其它
// 成员 + 我其它设备经链同步跟随。拒绝移除本机设备或其它成员设备。提交移除意图即返回 true。
export async function removeGroupWireDevice(
  uiConvId: string,
  deviceIdentity: string,
): Promise<boolean> {
  if (!groupWireSession) return false;
  if (!uiConvId.startsWith("g:")) return false;
  const self = useAppStore.getState().account?.account;
  if (!self) return false;
  try {
    if (!openMlsEngine.hasGroup(uiConvId)) return false;
  } catch {
    return false;
  }
  const roster = computeWireGroupRoster(
    openMlsEngine.memberIdentities(uiConvId),
    self,
    groupWireDeviceId ?? undefined,
  );
  // EN: only my own non-local leaves are eligible. CN: 仅我自己的非本机 leaf 可移除。
  const target = roster.self.find((d) => d.identity === deviceIdentity && !d.isThisDevice);
  if (!target) {
    useAppStore.setState({ error: "无法移除该设备（仅能移除你自己的其他设备）" });
    return false;
  }
  try {
    await groupWireSession.removeDevice(uiConvId, deviceIdentity);
    useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1, error: null }));
    void useAppStore.getState().refreshConversations();
    return true;
  } catch (e) {
    useAppStore.setState({ error: e instanceof Error ? e.message : "移除设备失败" });
    return false;
  }
}

// EN: Inbound relay handler (module-level; not part of the UI state surface).
// Decrypt → MessageVM → LocalStore → live update / unread bump → re-merge.
// CN: 入站 relay 处理（模块级，不属 UI 状态面）。解密→MessageVM→本地→实时更新/未读+1→重 Merge。
/// EN: Shared inbound deposit: turn a decrypted `EnvelopeV1` into stored state + UI, regardless of
/// the crypto stack it arrived on (MLS-Wire via `handleInbound`, or DR via `depositDrMessage`).
/// Handles `media_ack` / `recall` control envelopes, dedup, unread/mention bumps, and refresh.
/// CN: 共享入站存储：把解密后的 `EnvelopeV1` 落为存储 + UI，与其到达的密码栈无关（经 `handleInbound`
/// 的 MLS-Wire，或经 `depositDrMessage` 的 DR）。处理 `media_ack` / `recall` 控制信封、去重、未读/
/// 提及计数与刷新。
async function depositInboundEnvelope(
  convId: string,
  env: EnvelopeV1,
  inboundSender: string,
  isSelfSent: boolean,
): Promise<void> {
  // EN: media_ack is a retention control message, never rendered — shorten the local-pin
  // TTL for the acked upload (1:1) and stop. CN: media_ack 为 retention 控制消息，不渲染——
  // 收短对应上传（1:1）的本机 pin TTL 后直接返回。
  if (env.type === "media_ack") {
    const target = (env.body as { target?: string } | null)?.target;
    if (target && peerFromDirectConvId(convId)) {
      shortenRetentionForMessage(convId, target);
    }
    return;
  }
  // EN: recall is a control message — hide the sender's targeted message on this device and
  // propagate the recalled placeholder to our other devices via the archive. Never rendered as
  // its own bubble. CN: recall 为控制消息——在本设备隐藏发送方指定的消息，并经归档把「已撤回」
  // 占位同步到本账户其他设备；不渲染为独立气泡。
  if (env.type === "recall") {
    const applied = await applyRecallEnvelope(localStore, convId, env);
    if (applied) {
      const st = useAppStore.getState();
      if (st.activeConvId === convId) {
        useAppStore.setState({ messages: await localStore.listMessages(convId) });
      }
      scheduleRefreshConversations();
      if (st.account) scheduleOffchainSync(st.account.account);
    }
    return;
  }
  await clearConversationDeleted(localStore, convId);
  await localStore.ensureConv(convId);
  const eph = ephemeralFromEnvelope(env);
  const incoming: MessageVM = {
    clientMsgId: env.id,
    convId,
    senderRef: isSelfSent ? "me" : inboundSender,
    isOutgoing: isSelfSent,
    sentAt: envelopeSentAt(env),
    content: contentFromEnvelope(env),
    replyTo: env.replyTo,
    forwardFrom: env.forward
      ? {
          msgId: env.forward.fromMsg,
          convId: env.forward.fromConv,
          preview: env.forward.preview,
        }
      : undefined,
    mentions: env.mentions ?? [],
    ephemeralTtlMs: eph.ephemeralTtlMs,
    ephemeralBurnOn: eph.ephemeralBurnOn,
    ephemeralBurnAt:
      eph.ephemeralTtlMs && eph.ephemeralBurnOn
        ? burnAtOnCreate(eph.ephemeralTtlMs, eph.ephemeralBurnOn)
        : undefined,
    starred: false,
    status: "acked",
    source: "offChainMls",
  };
  // EN: dedup by client msg id. CN: 按客户端 id 去重。
  const exists = await localStore.getMessage(convId, env.id);
  if (!exists) {
    await localStore.appendMessage(incoming);
  } else if (isSelfSent && exists.status === "pending") {
    await localStore.updateMessage(convId, env.id, { status: "sent" });
  }

  const st = useAppStore.getState();
  const isReaction = env.type === "reaction";
  const mentionedMe =
    !isReaction &&
    convId.startsWith("g:") &&
    st.selfMention != null &&
    isMentioned(incoming.mentions, st.selfMention);

  if (st.activeConvId === convId) {
    await localStore.armEphemeralOnRead(convId, Date.now());
    useAppStore.setState({ messages: await localStore.listMessages(convId) });
    await localStore.markRead(convId);
    await localStore.markMentionsRead(convId);
  } else if (!isReaction && !isSelfSent) {
    await localStore.bumpUnread(convId);
    if (mentionedMe) await localStore.bumpMentionUnread(convId);
  }
  scheduleRefreshConversations();
  if (st.account) scheduleOffchainSync(st.account.account);
}

async function handleInbound(frame: RelayFrame): Promise<void> {
  let selfAddress: string | undefined;
  const dedupKey =
    frame.dedupKey ??
    `${frame.convId}:${frame.ciphertextB64.slice(0, 24)}`;
  if (mailboxDeadKeys.has(dedupKey)) return;
  try {
    selfAddress = useAppStore.getState().account?.account;
    if (!selfAddress) return;
    const senderRef = await resolveInboundSender(frame, selfAddress);
    const resolved = resolveDirectInboundConv(frame, selfAddress, senderRef);
    let convId = resolved.convId;
    const inboundSender = resolved.senderRef;
    const isSelfSent =
      inboundSender !== "peer" &&
      canonicalAddress(inboundSender) === canonicalAddress(selfAddress);
    const peer = peerFromDirectConvId(convId);
    // EN: DR dispatch (design §21): when the DR stack exists, offer the frame to it FIRST. A
    // genuine DR `DmEnvelope` is decoded/decrypted + deposited via the transport's onMessage
    // callback (`depositDrMessage`); we then stop so a DR frame is never MLS-processed. An
    // already DR-pinned peer that can't decrypt the frame here also stops (do not fall to MLS).
    // Non-DR frames return false and fall through to the unchanged MLS-Wire path. CN: DR 分流
    // （设计 §21）：DR 栈存在时先把帧交给它。真正的 DR `DmEnvelope` 由传输 onMessage 回调
    // （`depositDrMessage`）解码/解密 + 存储；随后停止，使 DR 帧绝不走 MLS。已钉定 DR 的对端若此处
    // 无法解密也停止（不回退 MLS）。非 DR 帧返回 false，落到不变的 MLS-Wire 路径。
    if (peer && chatStack) {
      const pinned = isDrPeer(peer);
      const handled = await chatStack.transport.ingestFrame(frame);
      if (handled) {
        if (!pinned) markDrPeer(peer);
        return;
      }
      if (pinned) return;
    }
    if (peer && directRegistry) {
      directRegistry.ensure(peer);
      if (config.mlsBackend === "openmls" && !directRegistry.isReady(peer)) {
        if (!directMlsWaitOnce.has(peer)) {
          directMlsWaitOnce.add(peer);
          if (!(await waitForDirectMls(peer))) {
            throw new Error(`MLS 握手未完成，无法解密 (${peer.slice(0, 8)}…)`);
          }
        } else {
          throw new Error(`MLS 握手未完成，无法解密 (${peer.slice(0, 8)}…)`);
        }
      }
    }
    const env = await mlsDecrypt(convId, b64ToBytes(frame.ciphertextB64), selfAddress);
    await depositInboundEnvelope(convId, env, inboundSender, isSelfSent);
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e);
    const staleCipher =
      detail.includes("WrongGroupId") ||
      detail.includes("ValidationError") ||
      detail.includes("UnknownValue") ||
      detail.includes("NoMatchingKeyPackage");
    const handshakePending = detail.includes("MLS 握手未完成");
    if (!mailboxDropWarned.has(dedupKey)) {
      mailboxDropWarned.add(dedupKey);
      console.warn("[nexchat] inbound frame dropped:", dedupKey, detail);
    }
    const peer = peerFromDirectConvId(
      selfAddress ? resolveDirectInboundConv(frame, selfAddress, "peer").convId : frame.convId,
    );
    if (staleCipher && selfAddress) {
      mailboxDeadKeys.add(dedupKey);
      if (config.chatMailboxConsumeStaleEnabled) {
        queueChatConsume(selfAddress, dedupKey);
      }
    }
    if (
      peer &&
      directRegistry &&
      config.mlsBackend === "openmls" &&
      staleCipher &&
      !inboundRecoverOnce.has(peer) &&
      !detail.includes("duplicate delivery token")
    ) {
      inboundRecoverOnce.add(peer);
      directRegistry.recoverPeer(peer);
    }
    if (handshakePending && peer && directRegistry) {
      directRegistry.ensure(peer);
    }
    // EN: Group (g:) inbound recovery — groups have no per-peer handshake to recover, so a stale-cipher
    // decrypt (epoch-behind: our local group epoch lags the on-chain handshake log, or a pending
    // re-add Welcome) would otherwise drop forever. Actively catch up from chain via
    // `ensureGroupMlsReady` (claims any pending Welcome + replays missed Commits in order), then bump
    // `mlsSyncRev` and re-pull the inbox so subsequent frames decrypt. Guarded per group to avoid a
    // hot loop; the guard is cleared on success so a later epoch advance can re-trigger, while a
    // permanent chain gap stays guarded. This is the group analog of the 1:1 `recoverPeer` above.
    // CN: 群（g:）入站恢复——群无按对端握手可恢复，stale-cipher 解密（epoch 落后：本地群 epoch 落后链上
    // handshake 日志，或存在待领取的重加入 Welcome）否则会永久丢弃。经 `ensureGroupMlsReady` 主动从链追平
    // （领取待处理 Welcome + 按序重放漏掉的 Commit），随后递增 `mlsSyncRev` 并重拉信箱使后续帧可解密。
    // 按群加守卫避免热循环；成功后清除守卫以便后续 epoch 推进再触发，链上永久缺口则保持守卫。此为上方 1:1
    // `recoverPeer` 的群对应。
    const groupConv = frame.convId;
    if (
      staleCipher &&
      selfAddress &&
      groupConv.startsWith("g:") &&
      config.mlsBackend === "openmls" &&
      config.mlsControlPlane === "chain" &&
      !config.useMock &&
      !inboundRecoverOnce.has(groupConv)
    ) {
      const gid = Number(groupConv.slice(2));
      if (Number.isFinite(gid)) {
        inboundRecoverOnce.add(groupConv);
        console.info("[nexchat][group] stale-cipher → chain catch-up", { conv: groupConv, detail });
        void (async () => {
          if (config.wireGroupMultileafEnabled && groupWireSession) {
            await ensureGroupWireGraftReady(groupConv);
          }
          return ensureGroupMlsReady({
            engine: openMlsEngine,
            chain: chainClient,
            selfAddress,
            groupId: gid,
          });
        })()
          .then((result) => {
            console.info("[nexchat][group] catch-up result", {
              conv: groupConv,
              ok: result.ok,
              reason: result.ok ? undefined : result.reason,
              epoch: openMlsEngine.hasGroup(groupConv) ? openMlsEngine.epochByConv(groupConv) : null,
            });
            if (result.ok) {
              inboundRecoverOnce.delete(groupConv);
              useAppStore.setState((s) => ({ mlsSyncRev: s.mlsSyncRev + 1 }));
              void chatMailboxSync?.syncInbox();
              scheduleGroupVaultBackup();
            }
          })
          .catch((err) =>
            console.warn("[nexchat][group] catch-up failed:", groupConv, err),
          );
      }
    }
  }
}
