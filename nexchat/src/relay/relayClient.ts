// EN: RelayClient — off-chain store-and-forward delivery. The relay only ever sees
// CIPHERTEXT frames (encryption happens in MlsEngine before send). Two impls:
//   - BroadcastChannelRelay: REAL same-origin multi-tab delivery (open two tabs =
//     two users). Frames carry base64 ciphertext only.
//   - StubRelayClient: offline no-op (SSR / tests).
// Phase 4 swaps in a networked relay with RFC 9474 blinded delivery tokens,
// sealed-sender, dedup by `t`, and ephemeral TTL.
// CN: RelayClient——链下投递。relay 只见密文帧（加密在 MlsEngine 内、发送前完成）。
// BroadcastChannelRelay 为同源多标签页真实投递（开两个标签页=两个用户）；StubRelayClient
// 为离线空操作。Phase 4 换网络化 relay（RFC 9474 盲签令牌 + sealed-sender + 按 t 去重 + TTL）。

import { config } from "@/config";
import type { DevicePeerEndorsement, SealedHandoffPayload } from "@/mls/devicePeerKey";
import { WebSocketRelay } from "@/relay/wsRelay";
import { MultiplexRelay } from "@/relay/multiplexRelay";
import { networkRelayRequired } from "@/relay/relayNetwork";

export interface RelayFrame {
  convId: string;
  /** EN: Sender display ref omitted when RFC 9474 delivery admission is attached; the relay still
   *  learns the sender from the authenticated WebSocket session (`register_account`) and routing
   *  metadata (`convId`, optional `delivery.mlsKey`) — this is NOT Signal-style metadata privacy.
   *  CN: 附加 RFC 9474 投递准入时省略发送方展示引用；relay 仍可从已认证 WS 会话（`register_account`）
   *  与路由元数据（`convId`、可选 `delivery.mlsKey`）获知发送方——**非** Signal 式元数据隐私。 */
  senderRef: string;
  /** base64 ciphertext (relay never decodes it) */
  ciphertextB64: string;
  /** EN: optional relay-side expiry (ms epoch); expired frames are dropped undelivered.
   *  CN: 可选 relay 侧过期时间（毫秒时间戳）；过期帧丢弃不投递。 */
  expiresAt?: number;
  /** EN: client-generated dedup key (multiplex / WS relay). CN: 客户端生成的去重键。 */
  dedupKey?: string;
  /** EN: RFC 9474 blind delivery admission (1:1 direct). CN: RFC 9474 盲签投递准入（1:1）。 */
  delivery?: import("@/delivery/types").DeliveryAdmission;
  /** EN: Group chat routing — relay delivers only to these SS58 accounts. CN: 群聊路由——relay 仅投递给这些 SS58。 */
  routeTo?: string[];
  /** EN: Track B — also retain in sender account mailbox for offline sibling devices.
   *  CN: 路线 B——同时留存到发送方账户邮箱，供离线兄弟设备补齐。 */
  echoSelf?: boolean;
}

/// EN: Opt in to relay sender-mailbox echo when Wire multi-leaf is enabled for this conv.
/// CN: Wire 多 leaf 开启时，向 relay 声明发送方邮箱回显（多设备离线补齐）。
export function withMultiDeviceEcho(frame: RelayFrame, convId: string): RelayFrame {
  const isGroup = convId.startsWith("g:");
  const echo =
    (isGroup && config.wireGroupMultileafEnabled) ||
    (!isGroup && config.wireMultileafEnabled);
  return echo ? { ...frame, echoSelf: true } : frame;
}

export type RelayInbound = (frame: RelayFrame) => void;

// EN: MLS handshake control-plane messages carried on the same relay (a minimal
// off-chain DS/AS stand-in for the multi-tab demo): peers announce themselves (hello),
// joiners publish a KeyPackage (kp), the owner sends a directed Welcome and broadcasts
// each Commit so existing members advance their epoch. All payloads are base64 of the
// real OpenMLS bytes — the relay still never sees plaintext.
// CN: 同一 relay 上承载的 MLS 握手控制面消息（多标签页 demo 的最小链下 DS/AS 替身）：peer 宣告
// (hello)、加入者发布 KeyPackage(kp)、owner 定向投递 Welcome 并广播每个 Commit 让旧成员推进
// epoch。载荷均为真实 OpenMLS 字节的 base64 —— relay 仍不见明文。
export type ControlMsg =
  | { t: "hello"; from: string; identity: string; convId: string; owner: boolean }
  | { t: "kp"; from: string; identity: string; convId: string; kp: string }
  | {
      t: "welcome";
      from: string;
      to: string;
      /** EN: SS58 peer for cross-endpoint 1:1 routing. CN: 跨端点 1:1 路由用对端 SS58。 */
      toAddr?: string;
      convId: string;
      welcome: string;
    }
  | {
      t: "commit";
      from: string;
      convId: string;
      commit: string;
      /** EN: Gate 2 — MLS pre-epoch for relay CAS (Wire multi-leaf only). CN: 闸二——relay CAS 用的 MLS 前置 epoch（仅 Wire 多 leaf）。 */
      commit_epoch?: number;
      /** EN: Idempotency key for relay CAS + MLS mailbox dedup. CN: relay CAS 与 MLS 邮箱去重的幂等键。 */
      msgId?: string;
    }
  /** EN: Track B device-subgroup state snapshot on `s:<account>` (opaque MLS export; fans out to all
   *  account devices). Reserved for virtual-client endgame — current Wire multi-leaf does not emit this.
   *  CN: 路线 B 设备子群状态快照（`s:<account>`，不透明 MLS 导出；扇出到账户全部设备）。预留给虚拟客户端
   *  终态——现行 Wire 多 leaf 不发此帧。 */
  | {
      t: "new_device_state";
      from: string;
      convId: string;
      state: string;
      /** EN: Dedup key for concurrent subgroup snapshots (relay `msgId` path). CN: 并发子群快照去重键（relay `msgId` 路径）。 */
      msgId?: string;
    }
  | {
      t: "mls_ready";
      from: string;
      identity: string;
      convId: string;
    }
  | {
      t: "token_req";
      from: string;
      fromAddr: string;
      toAddr: string;
      convId: string;
      blinds: string[];
    }
  | {
      t: "token_sig";
      from: string;
      issuer: string;
      toAddr: string;
      convId: string;
      inboxId: string;
      epoch: number;
      ipkN: string;
      ipkE: string;
      ct: string;
      sigs: string[];
    }
  | {
      t: "contact_req";
      from: string;
      fromAddr: string;
      toAddr: string;
      reqId: string;
      /** EN: sender's display name shared with recipient. CN: 发送方展示名。 */
      fromLabel: string;
      sentAt: number;
    }
  | {
      t: "contact_ack";
      from: string;
      fromAddr: string;
      toAddr: string;
      reqId: string;
      action: "accept" | "reject";
      /** EN: recipient's label for sender (accept only). CN: 接收方为发送方起的显示名。 */
      label?: string;
    }
  | {
      t: "group_invite";
      from: string;
      fromAddr: string;
      toAddr: string;
      inviteId: string;
      groupId: number;
      groupName: string;
      fromLabel: string;
      sentAt: number;
    }
  /** EN: Account self-channel (`s:<account>`) — Gate 1 presence (Wire multi-leaf). CN: 账户自通道 presence（Wire 多 leaf，闸一）。 */
  | { t: "presence"; convId: string; device_id: string; online: boolean }
  /** EN: Non-CD → CD Commit intent (Gate 1). CN: 非 CD → CD 的 Commit 意图（闸一）。 */
  | {
      t: "commit_intent";
      convId: string;
      from: string;
      req_id: string;
      kind: "add_device" | "remove_device" | "rekey";
      payload: { dmConvId: string; kp?: string; target?: string; welcomeTo?: string };
    }
  /** EN: CD → requester Commit result (Gate 1). CN: CD → 请求方的 Commit 结果（闸一）。 */
  | {
      t: "commit_result";
      convId: string;
      req_id: string;
      ok: boolean;
      reason?: "epoch_stale";
      current_epoch?: number;
    }
  /** EN: New device → account: request graft into existing 1:1s (Wire join trigger). CN: 新设备 →
   *  账户：请求嫁接进已有 1:1（Wire 加入触发）。 */
  | { t: "device_join_request"; convId: string; device_id: string }
  /** EN: CD → new device: pairwise convs to join. CN: CD → 新设备：待加入的 pairwise 会话集。 */
  | { t: "device_join_offer"; convId: string; device_id: string; conv_ids: string[] }
  /** EN: New device → CD: one fresh KeyPackage per conv to graft. CN: 新设备 → CD：每会话一个一次性
   *  KeyPackage。 */
  | { t: "device_join_kp"; convId: string; device_id: string; kps: Array<{ conv_id: string; kp: string }> }
  /** EN: Peer-assisted Add (§3.8): a device of `requester_account` asks the OTHER party of `convId`
   *  to graft its leaf into the existing 1:1 (used when no sibling of the requester is online). `kp`
   *  is the joining device's KeyPackage. `_senderAccount` is stamped by the relay (authenticated
   *  sender) — the receiver MUST verify it equals `requester_account` before adding. CN: 对端代 Add
   *  （§3.8）：`requester_account` 的某设备请求 `convId` 的**对方**把其 leaf 接进已有 1:1（请求方无在线
   *  兄弟时使用）。`kp` 为加入设备的 KeyPackage。`_senderAccount` 由 relay（认证发送者）盖章——接收方在
   *  加人前**必须**校验其等于 `requester_account`。 */
  | {
      t: "peer_add_req";
      convId: string;
      from: string;
      requester_account: string;
      device_id: string;
      kp: string;
      /** EN: E2EI account ownership rides INSIDE the KeyPackage's leaf node (§3.9, in-MLS binding); the
       *  peer verifies it relay-trustlessly straight from `kp`. The legacy request-level `cred` field was
       *  retired once every engine embeds the in-MLS binding. CN: E2EI 账户归属驻留在 KeyPackage 的 leaf
       *  节点内（§3.9，MLS 内绑定）；对端直接从 `kp` 做 relay-trustless 验证。全引擎嵌入 MLS 内绑定后，旧的
       *  请求级 `cred` 字段已退役。 */
      /** EN: Relay-stamped authenticated sender account (anti-impersonation). CN: relay 盖章的认证发送
       *  者账户（防冒充）。 */
      _senderAccount?: string;
    }
  /** EN: Track A sending-authority online handoff (design §5.2), carried on the account self-channel
   *  `s:<account>`. REQUEST: a read-only (escrow-restored) device asks its account siblings for sending
   *  authority, carrying its device-directory-key-endorsed peer public key. CN: 路线 A 发送权在线交接
   *  （设计 §5.2），走账户自通道 `s:<account>`。REQUEST：只读（托管恢复）设备向账户兄弟设备申请发送权，
   *  携带其经设备目录钥背书的对端公钥。 */
  | { t: "handoff-request"; convId: string; from: string; endorsement: DevicePeerEndorsement }
  /** EN: GRANT: the old primary returns the §5 signed receipt + the signing-key bundle SEALED to the
   *  requester's device peer key (relay never sees plaintext). CN: GRANT：旧主设备返回 §5 签名收据 +
   *  封装给请求方设备对端钥的签名钥 bundle（relay 不见明文）。 */
  | { t: "handoff-grant"; convId: string; to: string; payload: SealedHandoffPayload }
  /** EN: DR stack (X3DH) one-time-prekey publication on the account self-channel `s:<account>` — a
   *  device advertises its OPK leaves (each with a Merkle proof against the on-chain `DeviceOpkRoot`)
   *  so an initiator can run X3DH with a one-time key. The relay caches the uploaded set and
   *  single-dispenses leaves even while the owner is offline (a `toAddr`-stamped variant is a live
   *  single-leaf reply routed back to a specific initiator on a cache miss). Decoupled from MLS
   *  control (design §21). CN: DR 栈（X3DH）一次性预密钥发布，走账户自通道 `s:<account>`——设备公告其
   *  OPK 叶子（各带对链上 `DeviceOpkRoot` 的 Merkle 证明），使发起方可用一次性钥跑 X3DH。relay 缓存上传
   *  集合并在持有者离线时单发（带 `toAddr` 的变体为缓存未命中时路由回指定发起方的实时单叶回复）。与 MLS
   *  控制解耦（设计 §21）。 */
  | {
      t: "opk_publish";
      /** EN: Account self-channel `s:<account>` this is published on. CN: 发布所在账户自通道。 */
      convId: string;
      from: string;
      device_id: string;
      root: string;
      leaves: Array<{ opk_pub: string; proof: string }>;
      /** EN: Set ONLY on a live single-leaf reply to an `opk_fetch` whose cache missed on the relay:
       *  the initiator account to route this reply to (absent on an owner's bulk advertisement, which
       *  the relay caches instead). CN: 仅在对缓存未命中的 `opk_fetch` 的实时单叶回复上设置：要路由到
       *  的发起方账户（持有者批量公告时缺省，relay 改为缓存）。 */
      toAddr?: string;
    }
  /** EN: DR stack: an initiator requests one unused OPK leaf for `target_device` (relay/peer replies
   *  with a single-leaf `opk_publish`). CN: DR 栈：发起方为 `target_device` 请求一条未用 OPK 叶子
   *  （relay/对端以单叶 `opk_publish` 回复）。 */
  | { t: "opk_fetch"; convId: string; from: string; target_device: string };

export type ControlInbound = (msg: ControlMsg) => void;

/** EN: Relay-side NACK for a dropped outbound frame (e.g. rate limited), correlated by the
 *  frame's `dedupKey` so the sender can mark it failed / retry. CN: relay 对被丢弃的出站帧
 *  （如限流）的 NACK，按帧 `dedupKey` 关联，供发送方标记失败/重试。 */
export interface RelayReject {
  reason: string;
  dedupKey?: string;
  convId?: string;
}

export type RelayRejectInbound = (reject: RelayReject) => void;

/** EN: Gate 2 loser — relay `commit_reject{epoch_stale}` (Wire 1:1 Commit CAS). CN: 闸二落败——relay `commit_reject{epoch_stale}`。 */
export interface CommitReject {
  reason: "epoch_stale";
  convId: string;
  current_epoch: number;
  msgId?: string;
}

export type CommitRejectInbound = (reject: CommitReject) => void;

/** EN: Max wire size of a single relay frame (bytes); must match the server's
 *  `RELAY_MAX_MSG_BYTES` default so oversize frames fail fast client-side instead of being
 *  silently dropped by the relay. CN: 单个 relay 帧 wire 上限（字节），须与服务端
 *  `RELAY_MAX_MSG_BYTES` 默认值一致，使超大帧在客户端快速失败而非被 relay 静默丢弃。 */
export const RELAY_MAX_FRAME_BYTES = 256 * 1024;

export interface RelayClient {
  /** EN: `account` registers SS58 for targeted control-plane delivery on WS relay. CN: `account` 在 WS relay 上注册 SS58 以接收定向控制面消息。 */
  connect(selfRef: string, account?: string): Promise<void>;
  send(frame: RelayFrame): Promise<void>;
  onMessage(cb: RelayInbound): void;
  sendControl(msg: ControlMsg): Promise<void>;
  onControl(cb: ControlInbound): void;
  /** EN: Fired when a transport connects/reconnects (WS open). CN: 传输层连接/重连（WS open）时触发。 */
  onConnect?(cb: () => void): () => void;
  /** EN: Fired when the WS transport drops (reconnect may follow). CN: WS 传输断开时触发（可能随后重连）。 */
  onDisconnect?(cb: () => void): () => void;
  /** EN: Subscribe to relay frame NACKs (rate limit etc.); only the WS transport emits these.
   *  CN: 订阅 relay 帧 NACK（限流等）；仅 WS 传输会触发。 */
  onReject?(cb: RelayRejectInbound): void;
  /** EN: Subscribe to Gate 2 `commit_reject` (epoch_stale); WS transport only. CN: 订阅闸二 `commit_reject`（epoch_stale）；仅 WS。 */
  onCommitReject?(cb: CommitRejectInbound): void;
  /** EN: Ask the relay to RE-DELIVER the stored MLS control (Commits/Welcomes) for one conv to this
   *  account NOW. Lets a device that lost a Commit `(conv,epoch)` CAS race deterministically catch up
   *  to the winning epoch instead of waiting for incidental fan-out or a reconnect mailbox flush
   *  (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.3). WS transport only; a no-op on mock transports
   *  (BroadcastChannel already fans every frame to all tabs). CN: 请求 relay **立即**把某会话已存的
   *  MLS 控制（Commit/Welcome）重投到本账户。让 Commit `(conv,epoch)` CAS 落败设备确定性追平到胜出
   *  epoch，而非等待偶发扇出或重连邮箱 flush（串行化规范 §3.3）。仅 WS；mock 传输空操作（BroadcastChannel
   *  已把每帧扇给所有标签页）。 */
  requestMlsBacklog?(account: string, convId: string): void;
  disconnect(): void;
}

const CHANNEL = "nexchat-relay-v1";

let wsRelayInstance: WebSocketRelay | null = null;

/// EN: Live WS relay connection state (always true when network relay is not required). CN: WS relay
/// 连接态（未要求网络 relay 时恒为 true）。
export function isNetworkRelayConnected(): boolean {
  if (!networkRelayRequired()) return true;
  return wsRelayInstance?.isConnected() ?? false;
}

/// EN: Real loopback relay over BroadcastChannel (same-origin tabs). CN: 真实环回 relay。
export class BroadcastChannelRelay implements RelayClient {
  private ch: BroadcastChannel | null = null;
  private cb: RelayInbound | null = null;
  private ctrlCbs: ControlInbound[] = [];
  private selfRef = "";

  async connect(selfRef: string, _account?: string): Promise<void> {
    // EN: idempotent — close any prior channel (guards React StrictMode double-mount).
    // CN: 幂等——先关旧通道（防 React StrictMode 双挂载重复监听）。
    this.ch?.close();
    this.selfRef = selfRef;
    this.ch = new BroadcastChannel(CHANNEL);
    this.ch.onmessage = (ev: MessageEvent<Record<string, unknown> & { _from: string }>) => {
      const data = ev.data;
      // EN: ignore our own echo. CN: 忽略自己的回声。
      if (data._from === this.selfRef) return;
      if (data._ctrl === true) {
        const { _ctrl: _drop, _from: _drop2, ...msg } = data;
        const ctrl = msg as unknown as ControlMsg;
        for (const fn of this.ctrlCbs) fn(ctrl);
        return;
      }
      const expiresAt = data.expiresAt as number | undefined;
      if (expiresAt != null && Date.now() > expiresAt) return;
      this.cb?.({
        convId: data.convId as string,
        senderRef: data.senderRef as string,
        ciphertextB64: data.ciphertextB64 as string,
        expiresAt,
        delivery: data.delivery as RelayFrame["delivery"],
      });
    };
  }

  async send(frame: RelayFrame): Promise<void> {
    const dedupKey = frame.dedupKey ?? globalThis.crypto?.randomUUID?.() ?? `dk-${Date.now()}`;
    this.ch?.postMessage({ ...frame, dedupKey, _from: this.selfRef });
  }

  async sendControl(msg: ControlMsg): Promise<void> {
    this.ch?.postMessage({ ...msg, _ctrl: true, _from: this.selfRef });
  }

  onMessage(cb: RelayInbound): void {
    this.cb = cb;
  }

  onControl(cb: ControlInbound): void {
    this.ctrlCbs.push(cb);
  }

  disconnect(): void {
    this.ch?.close();
    this.ch = null;
    this.cb = null;
    this.ctrlCbs = [];
  }
}

/// EN: Offline no-op. CN: 离线空操作。
export class StubRelayClient implements RelayClient {
  async connect(_selfRef?: string, _account?: string): Promise<void> {}
  async send(): Promise<void> {}
  async sendControl(): Promise<void> {}
  onMessage(): void {}
  onControl(): void {}
  disconnect(): void {}
}

function makeRelayClient(): RelayClient {
  const parts: RelayClient[] = [];
  if (config.relayWs) {
    wsRelayInstance = new WebSocketRelay(config.relayWs);
    parts.push(wsRelayInstance);
  } else {
    wsRelayInstance = null;
  }
  if (typeof BroadcastChannel !== "undefined") parts.push(new BroadcastChannelRelay());
  if (parts.length === 0) return new StubRelayClient();
  if (parts.length === 1) return parts[0]!;
  return new MultiplexRelay(parts);
}

// EN: BC (+ optional WS when VITE_RELAY_WS is set). CN: BC（配置 VITE_RELAY_WS 时叠加 WS）。
export const relayClient: RelayClient = makeRelayClient();

export { bytesToB64, b64ToBytes } from "@/util/b64";
