// EN: ChainClient — the ONLY place that talks to the chain.
//   - Reads: raw JSON-RPC over HTTP to the node's `chat_*` methods (JSON-friendly
//     serde DTOs from node/src/chat_rpc.rs). No polkadot type registration needed.
//   - Writes: polkadot.js ApiPromise + active signer (desktop keyring or dev keyring).
//   - Mock mode (VITE_USE_MOCK=true): returns fixtures so the UI runs with no node.
// CN: ChainClient——唯一与链交互处。读走 node 的 `chat_*` JSON-RPC；写走 polkadot.js +
//   当前签名者（桌面 keyring 或 dev keyring）；mock 模式返回夹具。

import { config } from "@/config";
import { fetchWithTimeout, withTimeout } from "@/util/fetchTimeout";
import { isEqualPoolPriorityError, isPoolConflictError } from "@/chain/txErrors";
import type { GroupJoinRequestRow } from "@/group/groupJoinTypes";
import type { OnChainRow } from "@/merge/spec";
import { mockOnChainRows } from "@/mock/mockData";
import { cidBytesToString, type PinGrace, type PinRow } from "@/chain/pinQueries";
import {
  clearSigner,
  getSignerAddress,
  requireSigner,
  setInjectorSigner,
  setSignerPair,
  type SignBackend,
} from "@/chain/signer";
import type { ApiFactory } from "@/wallet/desktopKeyring";

/// EN: RpcConversation as returned by `chat_listConversations` (camelCase serde).
/// CN: `chat_listConversations` 返回的 RpcConversation（camelCase）。
export interface RpcConversation {
  kind: "direct" | "group";
  directId: string | null;
  groupId: number | null;
  peer: string | null;
  name: string;
  avatarCid: string;
  lastActive: number;
  unread: number;
  pinned: boolean;
  muted: boolean;
  archived: boolean;
  memberCount: number;
  groupRole: number;
}

export interface GroupMlsSnapshot {
  epoch: number;
  treeHash: string;
  confirmedTranscriptHash: string;
  groupInfoCid: string;
  memberCount: number;
  cipherSuite: number;
  isPublic: boolean;
  frozen: boolean;
}

/// EN: Raw `pallet-msg-identity` device record (bytes as stored on chain; endorsements
/// are verified by the caller, design §4.4). CN: 链上 `pallet-msg-identity` 设备记录原文
/// （字节照存；背书由调用方校验，设计 §4.4）。
export interface RawDeviceRecord {
  deviceId: Uint8Array;
  ik: Uint8Array;
  ikEndorsement: Uint8Array;
  prekeyEpoch: bigint;
}

/// EN: Raw signed-prekey record. CN: 签名预密钥记录原文。
export interface RawSignedPrekey {
  spk: Uint8Array;
  spkEndorsement: Uint8Array;
  validUntil: bigint;
}

/// EN: Raw OPK Merkle-root record. CN: OPK Merkle 根记录原文。
export interface RawOpkRoot {
  root: Uint8Array;
  count: number;
  epoch: number;
}

/// EN: Raw 1:1 stack capabilities. CN: 1:1 栈能力原文。
export interface RawStackCaps {
  flags: number;
  version: number;
}

let rpcId = 1;

function hexUtf8(str: string): string {
  const bytes = new TextEncoder().encode(str);
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

function storageBytesToString(raw: unknown): string {
  if (raw == null) return "";
  if (typeof raw === "string") {
    if (raw.startsWith("0x")) {
      const hex = raw.slice(2);
      if (hex.length % 2 !== 0) return "";
      const bytes = new Uint8Array(hex.length / 2);
      for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
      return new TextDecoder().decode(bytes);
    }
    return raw;
  }
  if (typeof raw === "object" && raw !== null && "toU8a" in raw) {
    return new TextDecoder().decode(
      (raw as { toU8a: (strip?: boolean) => Uint8Array }).toU8a(true),
    );
  }
  if (Array.isArray(raw)) {
    return new TextDecoder().decode(Uint8Array.from(raw as number[]));
  }
  return String(raw);
}

const JSON_RPC_TIMEOUT_MS = 12_000;

async function jsonRpc<T>(method: string, params: unknown[]): Promise<T> {
  const res = await fetchWithTimeout(
    config.httpEndpoint,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: rpcId++, method, params }),
    },
    JSON_RPC_TIMEOUT_MS,
    `RPC ${method}`,
  );
  if (!res.ok) throw new Error(`RPC ${method} HTTP ${res.status}`);
  const json = (await res.json()) as { result?: T; error?: { message: string } };
  if (json.error) throw new Error(`RPC ${method}: ${json.error.message}`);
  return json.result as T;
}

function rpcConvToRow(c: RpcConversation): OnChainRow {
  return {
    kind: c.kind,
    directId: c.directId ?? undefined,
    groupId: c.groupId ?? undefined,
    peer: c.peer ?? undefined,
    name: c.name,
    avatarCid: c.avatarCid,
    lastActive: c.lastActive,
    unread: c.unread,
    pinned: c.pinned,
    muted: c.muted,
    archived: c.archived,
    memberCount: c.memberCount,
    groupRole: c.groupRole,
  };
}

export class ChainClient {
  async listConversations(who: string): Promise<OnChainRow[]> {
    if (config.useMock) return mockOnChainRows();
    const rows = await jsonRpc<RpcConversation[]>("chat_listConversations", [who]);
    return rows.map(rpcConvToRow);
  }

  async totalDirectUnread(who: string): Promise<number> {
    if (config.useMock) return 0;
    return jsonRpc<number>("chat_totalDirectUnread", [who]);
  }

  async groupSnapshot(groupId: number): Promise<GroupMlsSnapshot | null> {
    if (config.useMock) return null;
    return jsonRpc<GroupMlsSnapshot | null>("chat_groupMlsSnapshot", [groupId]);
  }

  async isGroupFrozen(groupId: number): Promise<boolean> {
    if (config.useMock) return false;
    return jsonRpc<boolean>("chat_isGroupFrozen", [groupId]);
  }

  async pendingWelcome(groupId: number, who: string): Promise<string | null> {
    if (config.useMock) return null;
    return jsonRpc<string | null>("chat_pendingWelcome", [groupId, who]);
  }

  async handshakeAtEpoch(groupId: number, epoch: number): Promise<string | null> {
    if (config.useMock) return null;
    return jsonRpc<string | null>("chat_handshakeAtEpoch", [groupId, epoch]);
  }

  async isAccountMuted(who: string): Promise<boolean> {
    if (config.useMock) return false;
    return jsonRpc<boolean>("chat_isAccountMuted", [who]);
  }

  /// EN: Read the encrypted sync anchor at `anchor_id` (EISA, CHAT_SYNC_ANCHOR_ADR §5.6).
  /// Returns `{ updatedAt, ciphertext(0x hex) }` or null when unpublished.
  /// CN: 读取 `anchor_id` 处的加密同步锚（EISA，CHAT_SYNC_ANCHOR_ADR §5.6）；未发布返回 null。
  async syncAnchorOf(
    anchorIdHex: string,
  ): Promise<{ updatedAt: number; ciphertext: string } | null> {
    if (config.useMock) return null;
    return jsonRpc<{ updatedAt: number; ciphertext: string } | null>("chat_syncAnchor", [
      anchorIdHex,
    ]);
  }

  /// EN: Genesis hash bytes for the anchor signature payload (§5.5; cached per session).
  /// CN: 锚签名 payload 所需的创世哈希字节（§5.5；按会话缓存）。
  private genesisHashCache: Uint8Array | null = null;

  async genesisHashBytes(): Promise<Uint8Array> {
    if (this.genesisHashCache) return this.genesisHashCache;
    const hex = await jsonRpc<string>("chain_getBlockHash", [0]);
    const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
    const bytes = new Uint8Array(clean.length / 2);
    for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
    if (bytes.length !== 32) throw new Error("genesis hash must be 32 bytes");
    this.genesisHashCache = bytes;
    return bytes;
  }

  /// EN: Publish/replace the encrypted sync anchor (pallet-chat-sync; the active signer
  /// only pays fees/deposit — authorization is the Ed25519 `anchor_sig`, §5.5).
  /// CN: 发布/更新加密同步锚（pallet-chat-sync；当前签名者仅付费/押金——授权由 Ed25519
  /// `anchor_sig` 承担，§5.5）。
  async publishSyncAnchor(
    anchorPkHex: string,
    updatedAt: number,
    ciphertextHex: string,
    anchorSigHex: string,
    payerPair?: import("@polkadot/keyring/types").KeyringPair,
  ): Promise<void> {
    if (config.useMock) return;
    await this.signAndSend(
      "chatSync",
      "publishSyncAnchor",
      [anchorPkHex, updatedAt, ciphertextHex, anchorSigHex],
      payerPair,
    );
  }

  /// EN: Clear the anchor and refund the deposit (§5.5). CN: 删除锚并退还押金（§5.5）。
  async clearSyncAnchor(
    anchorPkHex: string,
    anchorSigHex: string,
    payerPair?: import("@polkadot/keyring/types").KeyringPair,
  ): Promise<void> {
    if (config.useMock) return;
    await this.signAndSend("chatSync", "clearSyncAnchor", [anchorPkHex, anchorSigHex], payerPair);
  }

  private apiPromise: Promise<unknown> | null = null;

  private async api() {
    if (!this.apiPromise) {
      this.apiPromise = (async () => {
        const { ApiPromise, WsProvider } = await import("@polkadot/api");
        // EN: Production MUST use `VITE_WS_ENDPOINT` (wss://rpc.nexusmall.net). Archive mainnet
        // metadata (~280KB) can take several minutes on cold connect — keep RPC + connect
        // timeouts high enough (measured ~4min worst case).
        // CN: 生产必须用 `VITE_WS_ENDPOINT`（wss://rpc.nexusmall.net）。归档主网 metadata（约
        // 280KB）冷启动可达数分钟——RPC 与连接超时须足够大（实测最坏约 4 分钟）。
        const WS_RECONNECT_MS = 2_500;
        const WS_RPC_TIMEOUT_MS = 300_000;
        const provider = new WsProvider(
          config.wsEndpoint,
          WS_RECONNECT_MS,
          undefined,
          WS_RPC_TIMEOUT_MS,
        );
        // EN: Bound the initial connect+metadata handshake; a dead WS would otherwise make
        // ApiPromise.create hang forever and strand any awaiting caller (e.g. unlock).
        // CN: 给首次连接+元数据握手设上限；否则 WS 不可达时 ApiPromise.create 永久挂起，拖死
        // 等待方（如 unlock）。
        return ApiPromise.create({ provider, noInitWarn: true });
      })();
      // EN: On connect timeout/failure, drop the cached promise so a later call can retry
      // instead of awaiting a permanently-pending handshake. CN: 连接超时/失败时清掉缓存，
      // 使后续调用可重试，而非永远等待一个挂起的握手。
      const API_CONNECT_TIMEOUT_MS = 300_000;
      this.apiPromise = withTimeout(this.apiPromise, API_CONNECT_TIMEOUT_MS, "chain api connect").catch((e) => {
        this.apiPromise = null;
        throw e;
      });
    }
    return this.apiPromise;
  }

  /** EN: Api handle for desktop-keyring `signPayload`. CN: 供桌面 keyring `signPayload` 用的 Api。 */
  getApiForWallet: ApiFactory = async () => (await this.api()) as Awaited<ReturnType<ApiFactory>>;

  /// EN: Active signer SS58 address. CN: 当前签名者 SS58 地址。
  get signerAddress(): string | null {
    return getSignerAddress();
  }

  disconnectSigner(): void {
    clearSigner();
  }

  /// EN: Dev-only: configure signer from `//Seed` URI (Nexus SS58 prefix 273).
  /// CN: 仅开发：用 `//种子` URI 配置签名者（Nexus SS58 前缀 273）。
  async useDevAccount(seed: string): Promise<string> {
    if (!config.devWallet) {
      throw new Error("dev wallet disabled (set VITE_DEV_WALLET=true or unlock desktop wallet)");
    }
    const { Keyring } = await import("@polkadot/api");
    const { cryptoWaitReady } = await import("@polkadot/util-crypto");
    await cryptoWaitReady();
    const keyring = new Keyring({ type: "sr25519", ss58Format: 273 });
    const pair = keyring.addFromUri(seed);
    setSignerPair(pair);
    // EN: dev pairs also feed the §5.0 vault_master root (same rule as desktop unlock).
    // CN: dev pair 同样作为 §5.0 vault_master 根来源（与桌面解锁同一规则）。
    const { deriveVaultMasterFromPair, setVaultMaster } = await import("@/wallet/vaultMaster");
    setVaultMaster(await deriveVaultMasterFromPair(pair));
    return pair.address;
  }

  async deriveAddress(seed: string): Promise<string> {
    const { Keyring } = await import("@polkadot/api");
    const { cryptoWaitReady } = await import("@polkadot/util-crypto");
    await cryptoWaitReady();
    return new Keyring({ type: "sr25519", ss58Format: 273 }).addFromUri(seed).address;
  }

  async freeBalance(who: string): Promise<bigint> {
    if (config.useMock) return 0n;
    const api = (await this.api()) as any;
    const acc = await api.query.system.account(who);
    return BigInt(acc.data.free.toString());
  }

  async nextGroupId(): Promise<number> {
    if (config.useMock) return config.mlsDemoGroupId;
    const api = (await this.api()) as any;
    const n = await api.query.chatGroup.nextGroupId();
    return Number(n.toString());
  }

  async keyPackagesOf(who: string): Promise<Uint8Array[]> {
    if (config.useMock) return [];
    const api = (await this.api()) as any;
    const entries = await api.query.chatGroup.keyPackages.entries(who);
    const out: { id: number; bytes: Uint8Array }[] = [];
    for (const [key, val] of entries) {
      const id = Number(key.args[1].toString());
      out.push({ id, bytes: val.toU8a(true) as Uint8Array });
    }
    out.sort((a, b) => b.id - a.id);
    return out.map((e) => e.bytes);
  }

  async keyPackageIdsOf(who: string): Promise<number[]> {
    if (config.useMock) return [];
    const api = (await this.api()) as any;
    const entries = await api.query.chatGroup.keyPackages.entries(who);
    return entries
      .map(([key]: [any, unknown]) => Number(key.args[1].toString()))
      .sort((a: number, b: number) => a - b);
  }

  /// EN: On-chain KeyPackage pool size for `who` (`chatGroup.keyPackageCount`). Used by join/add
  /// flows and profile diagnostics. CN: 账户 `who` 的链上 KeyPackage 池大小（`chatGroup.keyPackageCount`），
  /// 供入群/加人与 Profile 诊断使用。
  async keyPackageCountOf(who: string): Promise<number> {
    if (config.useMock) return 0;
    const api = (await this.api()) as any;
    const kpCount = await api.query.chatGroup.keyPackageCount(who);
    return Number(kpCount.toString());
  }

  // ==================== pallet-msg-identity (DR / X3DH 预密钥锚) ====================

  /// EN: All registered devices of `account` (IK + endorsement + prekey epoch). CN: `account`
  /// 已注册的全部设备（IK + 背书 + 预密钥纪元）。
  async msgIdentityDevices(account: string): Promise<RawDeviceRecord[]> {
    if (config.useMock) return [];
    const { hexToU8a } = await import("@polkadot/util");
    const api = (await this.api()) as any;
    const entries = await api.query.msgIdentity.deviceIdentities.entries(account);
    const out: RawDeviceRecord[] = [];
    for (const [key, val] of entries) {
      if (val.isNone) continue;
      const j = val.unwrap().toJSON() as { ik: string; ikEndorsement: string; prekeyEpoch: number };
      out.push({
        deviceId: key.args[1].toU8a() as Uint8Array,
        ik: hexToU8a(j.ik),
        ikEndorsement: hexToU8a(j.ikEndorsement),
        prekeyEpoch: BigInt(j.prekeyEpoch),
      });
    }
    return out;
  }

  /// EN: A device's signed prekey (SPK) + endorsement, or null. CN: 设备签名预密钥 + 背书。
  async msgIdentitySignedPrekey(
    account: string,
    deviceId: Uint8Array,
  ): Promise<RawSignedPrekey | null> {
    if (config.useMock) return null;
    const { hexToU8a, u8aToHex } = await import("@polkadot/util");
    const api = (await this.api()) as any;
    const opt = await api.query.msgIdentity.deviceSignedPreKeys(account, u8aToHex(deviceId));
    if (opt.isNone) return null;
    const j = opt.unwrap().toJSON() as { spk: string; spkEndorsement: string; validUntil: number };
    return {
      spk: hexToU8a(j.spk),
      spkEndorsement: hexToU8a(j.spkEndorsement),
      validUntil: BigInt(j.validUntil),
    };
  }

  /// EN: A device's OPK Merkle root, or null. CN: 设备 OPK Merkle 根。
  async msgIdentityOpkRoot(account: string, deviceId: Uint8Array): Promise<RawOpkRoot | null> {
    if (config.useMock) return null;
    const { hexToU8a, u8aToHex } = await import("@polkadot/util");
    const api = (await this.api()) as any;
    const opt = await api.query.msgIdentity.deviceOpkRoots(account, u8aToHex(deviceId));
    if (opt.isNone) return null;
    const j = opt.unwrap().toJSON() as { root: string; count: number; epoch: number };
    return { root: hexToU8a(j.root), count: Number(j.count), epoch: Number(j.epoch) };
  }

  /// EN: An account's advertised 1:1 stack capabilities (§20), or null. CN: 账户公告的 1:1 栈能力。
  async msgIdentityStackCaps(account: string): Promise<RawStackCaps | null> {
    if (config.useMock) return null;
    const api = (await this.api()) as any;
    const opt = await api.query.msgIdentity.chatStackCaps(account);
    if (opt.isNone) return null;
    const j = opt.unwrap().toJSON() as { flags: number; version: number };
    return { flags: Number(j.flags), version: Number(j.version) };
  }

  async groupProfile(
    groupId: number,
  ): Promise<{ name: string; avatarCid: string; announcement: string } | null> {
    if (config.useMock) return null;
    const api = (await this.api()) as any;
    const opt = await api.query.chatGroup.groupProfiles(groupId);
    if (opt.isNone) return null;
    const json = opt.unwrap().toJSON?.() as
      | { name?: unknown; avatarCid?: unknown; announcement?: unknown }
      | undefined;
    if (!json) return null;
    return {
      name: storageBytesToString(json.name),
      avatarCid: storageBytesToString(json.avatarCid),
      announcement: storageBytesToString(json.announcement),
    };
  }

  async groupJoinFlags(
    groupId: number,
    who: string,
  ): Promise<{
    isMember: boolean;
    hasJoinRequest: boolean;
    hasJoinApproval: boolean;
    isBanned: boolean;
    keyPackageCount: number;
  }> {
    if (config.useMock) {
      return {
        isMember: false,
        hasJoinRequest: false,
        hasJoinApproval: false,
        isBanned: false,
        keyPackageCount: 1,
      };
    }
    const api = (await this.api()) as any;
    const [member, joinReq, joinApproval, banned, kpCount] = await Promise.all([
      api.query.chatGroup.groupMembers(groupId, who),
      api.query.chatGroup.joinRequests(groupId, who),
      api.query.chatGroup.joinApprovals(groupId, who),
      api.query.chatGroup.banned(groupId, who),
      api.query.chatGroup.keyPackageCount(who),
    ]);
    return {
      isMember: member.isSome,
      hasJoinRequest: joinReq.isSome,
      hasJoinApproval: joinApproval.isSome,
      isBanned: banned.isSome,
      keyPackageCount: Number(kpCount.toString()),
    };
  }

  async listPendingJoinRequests(
    who: string,
  ): Promise<{ groupId: number; requestedAt: number }[]> {
    if (config.useMock) return [];
    try {
      const api = (await this.api()) as any;
      const entries = await api.query.chatGroup.joinRequests.entries();
      const out: { groupId: number; requestedAt: number }[] = [];
      for (const [key, val] of entries) {
        const acct = key.args[1]?.toString?.() ?? "";
        if (acct !== who) continue;
        const gid = Number(key.args[0].toString());
        out.push({ groupId: gid, requestedAt: Number(val.toString()) });
      }
      out.sort((a, b) => b.requestedAt - a.requestedAt);
      return out;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes("WebSocket is not connected") || msg.includes("disconnected")) {
        return [];
      }
      throw e;
    }
  }

  async requestJoin(groupId: number): Promise<void> {
    await this.signAndSend("chatGroup", "requestJoin", [groupId]);
  }

  async cancelJoinRequest(groupId: number): Promise<void> {
    await this.signAndSend("chatGroup", "cancelJoinRequest", [groupId]);
  }

  async approveJoin(groupId: number, who: string): Promise<void> {
    await this.signAndSend("chatGroup", "approveJoin", [groupId, who]);
  }

  /// EN: Pending join requests for a group (admin view). CN: 某群待批入群申请（管理员视图）。
  async listGroupJoinRequests(groupId: number): Promise<GroupJoinRequestRow[]> {
    if (config.useMock) return [];
    const api = (await this.api()) as any;
    const [reqEntries, approvalEntries] = await Promise.all([
      api.query.chatGroup.joinRequests.entries(groupId),
      api.query.chatGroup.joinApprovals.entries(groupId),
    ]);
    const byAddr = new Map<string, GroupJoinRequestRow>();
    for (const [key, val] of reqEntries) {
      const addr = key.args[1]?.toString?.() ?? "";
      if (!addr) continue;
      byAddr.set(addr, {
        address: addr,
        requestedAt: Number(val.toString()),
        approved: false,
        hasKeyPackage: false,
      });
    }
    for (const [key] of approvalEntries) {
      const addr = key.args[1]?.toString?.() ?? "";
      if (!addr) continue;
      const prev = byAddr.get(addr);
      if (prev) prev.approved = true;
      else {
        byAddr.set(addr, {
          address: addr,
          requestedAt: 0,
          approved: true,
          hasKeyPackage: false,
        });
      }
    }
    const rows = [...byAddr.values()];
    await Promise.all(
      rows.map(async (row) => {
        const kp = await api.query.chatGroup.keyPackageCount(row.address);
        row.hasKeyPackage = Number(kp.toString()) > 0;
      }),
    );
    rows.sort((a, b) => b.requestedAt - a.requestedAt || a.address.localeCompare(b.address));
    return rows;
  }

  async listGroupMembers(groupId: number): Promise<
    { address: string; role: "owner" | "admin" | "member" }[]
  > {
    if (config.useMock) return [];
    const api = (await this.api()) as any;
    const entries = await api.query.chatGroup.groupMembers.entries(groupId);
    const rows: { address: string; role: "owner" | "admin" | "member" }[] = [];
    for (const [key, val] of entries) {
      const rawAddr = key.args[1]?.toString?.() ?? "";
      if (!rawAddr) continue;
      const json = val.toJSON?.() as { role?: unknown } | null | undefined;
      let role: "owner" | "admin" | "member" = "member";
      if (json?.role && typeof json.role === "object") {
        if ("owner" in (json.role as object)) role = "owner";
        else if ("admin" in (json.role as object)) role = "admin";
      }
      rows.push({ address: rawAddr, role });
    }
    rows.sort((a, b) => {
      const rank = (r: string) => (r === "owner" ? 0 : r === "admin" ? 1 : 2);
      const d = rank(a.role) - rank(b.role);
      return d !== 0 ? d : a.address.localeCompare(b.address);
    });
    return rows;
  }

  /// EN: Per-signer submission queue — serializes extrinsics from the SAME account so one device never
  /// fires two concurrently with a colliding nonce (the dominant tx-pool 1014 cause when several chat
  /// control txs — publish_key_package / commit / inbox / anchor — fire at once). Each task waits for
  /// the prior one to settle (in-block or error) so `accountNextIndex` increments before the next nonce
  /// query. CN: 按签名者的提交队列——串行化同一账户的 extrinsic，使单设备绝不并发提交相同 nonce 的两笔
  /// （多笔聊天控制交易——publish_key_package/commit/inbox/anchor——同时提交时交易池 1014 的主因）。每个
  /// 任务等待前一笔落定（上链或失败），下一笔取 nonce 前 `accountNextIndex` 已递增。
  private submitChains = new Map<string, Promise<unknown>>();

  private enqueueSubmit<T>(address: string, task: () => Promise<T>): Promise<T> {
    const prior = this.submitChains.get(address) ?? Promise.resolve();
    const run = prior.catch(() => undefined).then(task);
    // EN: keep the chain alive even when a task rejects — a failed tx must not wedge the queue.
    // CN: 任务失败也保持队列存活——单笔失败不得卡死整条队列。
    this.submitChains.set(address, run.catch(() => undefined));
    return run;
  }

  /// EN: Submit a signed extrinsic with the active signer, or with an explicit payer
  /// pair (EISA v2 payer unlinking, ADR §11.1 — the burner payer signs instead of the
  /// main account). Serialized per signer; pins a fresh nonce per attempt and, on a tx-pool
  /// priority conflict (1014), re-queries the nonce to advance to the next slot (multi-device:
  /// another device grabbed the slot) rather than only escalating tip to replace.
  /// CN: 用当前签名者提交 extrinsic；或用显式 payer pair（EISA v2 付费方断链，ADR §11.1——burner payer
  /// 代替主账户签名）。按签名者串行；每次尝试取一次新 nonce，遇交易池 priority 冲突（1014）时重取 nonce
  /// 前进到下一个槽位（多设备：另一设备已占该槽），而非只递增 tip 争抢替换。
  async signAndSend(
    section: string,
    method: string,
    args: unknown[],
    payerPair?: import("@polkadot/keyring/types").KeyringPair,
    opts?: { tip?: bigint },
  ): Promise<string> {
    if (config.useMock) return `mock-tx-${section}.${method}`;
    const backend: SignBackend = payerPair
      ? { kind: "pair", address: payerPair.address, pair: payerPair }
      : requireSigner();
    return this.enqueueSubmit(backend.address, () =>
      this.submitTx(section, method, args, backend, opts?.tip ?? 0n),
    );
  }

  /// EN: Nonce-aware single-extrinsic submit core (assumes the caller already serialized per signer).
  /// CN: 含 nonce 处理的单笔提交核心（调用方已按签名者串行化）。
  private async submitTx(
    section: string,
    method: string,
    args: unknown[],
    backend: SignBackend,
    initialTip: bigint,
  ): Promise<string> {
    const api = (await this.api()) as any;
    let tip = initialTip;
    const maxAttempts = 4;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      // EN: Pin a fresh nonce per attempt — `accountNextIndex` includes ready+future pool txs, so a
      // retry after a 1014 collision advances to the next nonce instead of fighting to replace the
      // slot another device/tab already took. CN: 每次尝试取一次新 nonce——`accountNextIndex` 含池中
      // ready+future 交易，故 1014 冲突后重试前进到下一个 nonce，而非争抢替换其它设备/标签页已占的槽位。
      const nonce = await api.rpc.system.accountNextIndex(backend.address);
      const tx = api.tx[section][method](...args);
      try {
        return await new Promise<string>((resolve, reject) => {
          const onResult = (result: any) => {
            if (result.dispatchError) {
              let msg = result.dispatchError.toString();
              if (result.dispatchError.isModule) {
                const meta = api.registry.findMetaError(result.dispatchError.asModule);
                msg = `${meta.section}.${meta.name}`;
              }
              reject(new Error(`${section}.${method} failed: ${msg}`));
              return;
            }
            if (result.status.isInBlock) resolve(result.status.asInBlock.toString());
          };
          const signOpts: Record<string, unknown> = { nonce };
          if (tip > 0n) signOpts.tip = tip;
          if (backend.kind === "pair") {
            tx.signAndSend(backend.pair, signOpts, onResult).catch(reject);
          } else {
            signOpts.signer = backend.signer;
            tx.signAndSend(backend.address, signOpts, onResult).catch(reject);
          }
        });
      } catch (e) {
        if (!isPoolConflictError(e) || attempt >= maxAttempts - 1) throw e;
        if (isEqualPoolPriorityError(e)) {
          // EN: identical priority → an equal tx already occupies this slot; wait for its inclusion
          // so the next nonce frees up, then retry. CN: 同 priority → 该槽已有等价交易；等其上链使
          // 下一 nonce 释放后重试。
          await new Promise((r) => setTimeout(r, 5000));
          continue;
        }
        // EN: different priority → next loop re-queries the nonce (advances slot); bump the tip too so a
        // genuine replace race still resolves. CN: 不同 priority → 下一轮重取 nonce（前进槽位），并同时
        // 提高 tip，使真正的替换竞争仍能收敛。
        tip = tip > 0n ? tip * 2n : 10_000_000_000n;
        await new Promise((r) => setTimeout(r, 1500));
      }
    }
    throw new Error(`${section}.${method}: exhausted pool retries`);
  }

  /** @deprecated Use signAndSend — dev alias kept for e2e/scripts. */
  async signAndSendDev(section: string, method: string, args: unknown[]): Promise<string> {
    return this.signAndSend(section, method, args);
  }

  async createGroup(args: unknown[]): Promise<number> {
    if (config.useMock) return config.mlsDemoGroupId;
    const backend = requireSigner();
    const api = (await this.api()) as any;
    // EN: share the per-signer queue so this never races a concurrent `signAndSend` on the same nonce.
    // CN: 复用按签名者的队列，使其绝不与同账户的并发 `signAndSend` 在同一 nonce 上竞争。
    return this.enqueueSubmit(backend.address, () => new Promise<number>((resolve, reject) => {
      const onResult = (result: any) => {
        if (result.dispatchError) {
          let msg = result.dispatchError.toString();
          if (result.dispatchError.isModule) {
            const meta = api.registry.findMetaError(result.dispatchError.asModule);
            msg = `${meta.section}.${meta.name}`;
          }
          reject(new Error(`chatGroup.createGroup failed: ${msg}`));
          return;
        }
        if (result.status.isInBlock) {
          for (const { event } of result.events) {
            if (api.events.chatGroup.GroupCreated.is(event)) {
              resolve(Number(event.data.groupId.toString()));
              return;
            }
          }
          reject(new Error("createGroup: no GroupCreated event"));
        }
      };
      const tx = api.tx.chatGroup.createGroup(...args);
      if (backend.kind === "pair") {
        tx.signAndSend(backend.pair, onResult).catch(reject);
      } else {
        tx.signAndSend(backend.address, { signer: backend.signer }, onResult).catch(reject);
      }
    }));
  }

  /** @deprecated Use createGroup. */
  async createGroupDev(args: unknown[]): Promise<number> {
    return this.createGroup(args);
  }

  async listMyPins(who: string): Promise<PinRow[]> {
    if (config.useMock) return [];
    const api = (await this.api()) as any;
    const idx = await api.query.storageService.ownerPinIndex(who);
    const hashes: unknown[] = idx.toJSON() as unknown[];
    const out: PinRow[] = [];
    for (const h of hashes) {
      const cidHash = String(h);
      const meta = await api.query.storageService.pinMeta(cidHash);
      if (meta.isNone) continue;
      const m = meta.unwrap();
      const cidOpt = await api.query.storageService.cidRegistry(cidHash);
      const cid = cidOpt.isSome
        ? cidBytesToString(cidOpt.unwrap().toU8a(true) as Uint8Array)
        : cidHash.slice(0, 14);
      const dueBlock = Number(
        (await api.query.storageService.cidBillingDueBlock(cidHash)).toString(),
      );
      let grace: PinGrace = "normal";
      let graceExpiresBlock: number | undefined;
      if (dueBlock > 0) {
        const taskOpt = await api.query.storageService.billingQueue(dueBlock, cidHash);
        if (taskOpt.isSome) {
          const gs = taskOpt.unwrap().graceStatus;
          if (gs.isInGrace) {
            grace = "inGrace";
            graceExpiresBlock = Number(gs.asInGrace.expiresAt.toString());
          } else if (gs.isExpired) grace = "expired";
        }
      }
      out.push({
        cidHash,
        cid,
        sizeBytes: Number(m.size.toString()),
        replicas: Number(m.replicas.toString()),
        dueBlock,
        grace,
        graceExpiresBlock,
      });
    }
    return out;
  }

  async renewPin(cidHash: string, periods: number): Promise<void> {
    if (config.useMock) return;
    await this.signAndSend("storageService", "renewPin", [cidHash, periods]);
  }

  async renewPinDev(cidHash: string, periods: number): Promise<void> {
    return this.renewPin(cidHash, periods);
  }

  async requestPinForSubject(
    subjectId: number,
    cid: string,
    sizeBytes: number,
    tier: "Critical" | "Standard" | "Temporary" = "Temporary",
  ): Promise<void> {
    if (config.useMock) return;
    const tierIdx = tier === "Critical" ? 0 : tier === "Standard" ? 1 : 2;
    await this.signAndSend("storageService", "requestPinForSubject", [
      subjectId,
      hexUtf8(cid),
      sizeBytes,
      tierIdx,
    ]);
  }

  async requestPinForSubjectDev(
    subjectId: number,
    cid: string,
    sizeBytes: number,
    tier: "Critical" | "Standard" | "Temporary" = "Temporary",
  ): Promise<void> {
    return this.requestPinForSubject(subjectId, cid, sizeBytes, tier);
  }

  /// EN: Owner disbands a group (`disband_group`). Large groups may require multiple extrinsics
  /// (bounded teardown); loops until `GroupDisbanded` or the group row is gone. CN: 群主解散群
  /// （`disband_group`）。大群可能需多次 extrinsic（有界拆除）；循环直至 `GroupDisbanded` 或群记录消失。
  async disbandGroup(
    groupId: number,
    onProgress?: (message: string) => void,
  ): Promise<void> {
    if (config.useMock) return;
    const maxRounds = 32;
    for (let round = 0; round < maxRounds; round++) {
      const snap = await this.groupSnapshot(groupId);
      if (!snap) return;
      const done = await this.disbandGroupOnce(groupId);
      if (done) return;
      onProgress?.(`解散进行中（第 ${round + 1} 步）…`);
    }
    if (!(await this.groupSnapshot(groupId))) return;
    throw new Error("解散群未完成，请稍后重试");
  }

  /// EN: Submit one `disband_group` extrinsic; returns true when the pallet emitted `GroupDisbanded`.
  /// CN: 提交一次 `disband_group`；pallet 发出 `GroupDisbanded` 时返回 true。
  private async disbandGroupOnce(groupId: number): Promise<boolean> {
    const backend = requireSigner();
    const api = (await this.api()) as any;
    // EN: share the per-signer queue so this never races a concurrent `signAndSend` on the same nonce.
    // CN: 复用按签名者的队列，使其绝不与同账户的并发 `signAndSend` 在同一 nonce 上竞争。
    return this.enqueueSubmit(backend.address, async () => {
      const nonce = await api.rpc.system.accountNextIndex(backend.address);
      return new Promise<boolean>((resolve, reject) => {
        const onResult = (result: any) => {
          if (result.dispatchError) {
            let msg = result.dispatchError.toString();
            if (result.dispatchError.isModule) {
              const meta = api.registry.findMetaError(result.dispatchError.asModule);
              msg = `${meta.section}.${meta.name}`;
            }
            reject(new Error(`chatGroup.disbandGroup failed: ${msg}`));
            return;
          }
          if (result.status.isInBlock) {
            let disbanded = false;
            for (const { event } of result.events) {
              if (api.events.chatGroup.GroupDisbanded?.is(event)) {
                if (Number(event.data.groupId.toString()) === groupId) disbanded = true;
              }
            }
            resolve(disbanded);
          }
        };
        const tx = api.tx.chatGroup.disbandGroup(groupId);
        if (backend.kind === "pair") {
          tx.signAndSend(backend.pair, { nonce }, onResult).catch(reject);
        } else {
          tx.signAndSend(backend.address, { nonce, signer: backend.signer }, onResult).catch(reject);
        }
      });
    });
  }

  async setGroupProfile(
    groupId: number,
    opts: { name?: string; avatarCid?: string; announcement?: string },
  ): Promise<void> {
    if (config.useMock) return;
    const enc = (v?: string) => (v != null ? hexUtf8(v) : null);
    await this.signAndSend("chatGroup", "setGroupProfile", [
      groupId,
      enc(opts.name),
      enc(opts.avatarCid),
      enc(opts.announcement),
    ]);
  }

  async setGroupProfileDev(
    groupId: number,
    opts: { name?: string; avatarCid?: string; announcement?: string },
  ): Promise<void> {
    return this.setGroupProfile(groupId, opts);
  }

  /// EN: Legacy/debug only — sign via Polkadot.js browser extension. Production uses the built-in
  /// desktop wallet (`WalletGate` → `setSignerPair`); this method is not called from UI.
  /// CN: 遗留/调试用——经 Polkadot.js 浏览器扩展签名。生产走内置桌面钱包，UI 不调用本方法。
  async signAndSendViaExtension(
    address: string,
    section: string,
    method: string,
    args: unknown[],
  ): Promise<string> {
    if (config.useMock) return `mock-tx-${section}.${method}`;
    const { web3FromAddress } = await import("@polkadot/extension-dapp");
    const injector = await web3FromAddress(address);
    setInjectorSigner(address, injector.signer);
    try {
      return await this.signAndSend(section, method, args);
    } finally {
      clearSigner();
    }
  }
}

export const chainClient = new ChainClient();
