// EN: DrTransport — relay `d:` delivery for the decentralized 1:1 Double Ratchet stack
// (design §21). Ties a `VodozemacEngine` to a `RelayClient`: outbound plaintext →
// `DmEnvelope` → base64 → `RelayFrame{convId:"d:{peer}"}`; inbound DR frames → decode →
// (init | decrypt) → plaintext. Reuses the existing `RelayFrame` with NO new frame
// structure and NO commit-slot CAS (DR frames carry no `commit_epoch`; the relay just
// stores-and-forwards). Multi-device routing uses the envelope's `recvDev` so only the
// addressed device processes a frame (§18.3). STRICTLY decoupled from `@/mls/*`.
// CN: DrTransport —— 去中心化 1:1 双棘轮栈的 relay `d:` 投递（设计 §21）。把
// `VodozemacEngine` 接到 `RelayClient`：出站明文 → `DmEnvelope` → base64 →
// `RelayFrame{convId:"d:{peer}"}`；入站 DR 帧 → 解码 →（建会话 | 解密）→ 明文。复用现有
// `RelayFrame`，**不新增帧结构**、**不触发 commit-slot CAS**（DR 帧无 `commit_epoch`，relay
// 仅存转）。多设备路由用信封 `recvDev`，仅被寻址设备处理该帧（§18.3）。与 `@/mls/*` 严格解耦。

import type { RelayClient, RelayFrame } from "@/relay/relayClient";
import { b64ToBytes, bytesToB64 } from "@/util/b64";
import { canonicalAddress } from "@/wallet/address";
import { DmKind, type DeviceId, type DmEnvelope, type PeerPrekeyBundle } from "@/crypto-dr/types";
import { DM_ENVELOPE_VER } from "@/crypto-dr/dmEnvelope";
import type { DrPersistence } from "@/crypto-dr/sessionStore";
import { VodozemacEngine } from "@/crypto-dr/vodozemacEngine";

const eqBytes = (a: Uint8Array, b: Uint8Array): boolean =>
  a.length === b.length && a.every((x, i) => x === b[i]);

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

const fromHex = (hex: string): Uint8Array => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
};

/// EN: UI / relay routing id for a 1:1 conversation = `d:{canonical_peer}` (§21). Inlined
/// here (instead of importing `@/mls/directConv`) to keep the DR stack import-decoupled from
/// the MLS engine — the convention is shared, but DR must not depend on `@/mls/*` (design
/// §2/§3, enforced by `decoupling.test.ts`). CN: 1:1 会话的 UI / relay 路由 id =
/// `d:{规范对端}`（§21）。在此内联（不 import `@/mls/directConv`），以保 DR 栈与 MLS 引擎
/// import 解耦——约定共享但 DR 不得依赖 `@/mls/*`（设计 §2/§3，由 `decoupling.test.ts` 强制）。
const directConvId = (peer: string): string => `d:${canonicalAddress(peer)}`;

/// EN: A decrypted inbound 1:1 message surfaced to the app layer. CN: 解密后上交应用层的
/// 入站 1:1 消息。
export interface DrIncoming {
  /// EN: Canonical (RPC_SS58) sender account. CN: 规范（RPC_SS58）发送方账户。
  peerAccount: string;
  /// EN: Sender device id (16 bytes). CN: 发送方设备 id（16 字节）。
  peerDevice: DeviceId;
  /// EN: Decrypted plaintext. CN: 解密明文。
  plaintext: Uint8Array;
  /// EN: The relay frame's routing conv-id (`d:{destination_account}`). For a normal inbound
  /// message this is `d:{self}` (use `peerAccount` to key the UI conversation); for a SIBLING ECHO
  /// (sender == our own account, §8) it is `d:{other_party}` — i.e. the conversation the echo
  /// should be deposited into. CN: relay 帧的路由 conv-id（`d:{目标账户}`）。普通入站消息为
  /// `d:{自己}`（UI 会话用 `peerAccount` 键）；兄弟设备回显（发送方为本账户，§8）时为
  /// `d:{对端}`——即该回显应落入的会话。
  convId: string;
}

/// EN: Inbound DR message callback. CN: 入站 DR 消息回调。
export type DrMessageHandler = (msg: DrIncoming) => void;

/// EN: Relay glue for the DR stack. Construct with a ready engine + relay client + self
/// account ref, then `attach()` to start consuming inbound frames. CN: DR 栈的 relay 胶水。
/// 用已就绪的引擎 + relay 客户端 + 本方账户引用构造，再 `attach()` 开始消费入站帧。
export class DrTransport {
  private handlers: DrMessageHandler[] = [];

  constructor(
    private readonly engine: VodozemacEngine,
    private readonly relay: RelayClient,
    private readonly selfRef: string,
    /// EN: Optional at-rest persistence; when present, account + per-session pickles are
    /// saved after every session-mutating op. CN: 可选静态持久化；存在时每次改动会话后保存
    /// 账户 + 会话 pickle。
    private readonly store?: DrPersistence,
  ) {}

  /// EN: Subscribe to decrypted inbound DR messages. CN: 订阅解密后的入站 DR 消息。
  onMessage(cb: DrMessageHandler): void {
    this.handlers.push(cb);
  }

  /// EN: Wire the relay's inbound frame stream into this transport (idempotent per relay
  /// `onMessage` semantics). Use ONLY when the DR stack owns the relay's single `onMessage`
  /// slot; when an app-level dispatcher already owns it, feed frames via `ingestFrame` instead.
  /// CN: 把 relay 入站帧流接入本传输（幂等性依 relay `onMessage` 语义）。仅当 DR 栈独占 relay 的
  /// 单一 `onMessage` 槽时使用；若已有应用级分发器占用该槽，改用 `ingestFrame` 喂帧。
  attach(): void {
    this.relay.onMessage((frame) => {
      void this.handleFrame(frame);
    });
  }

  /// EN: Feed one relay frame to the DR stack from an external dispatcher (e.g. the app's
  /// shared `onMessage` handler that also routes MLS). Returns true iff the frame was a DR
  /// `DmEnvelope` this device consumed (decrypted-for-us, or addressed to a sibling device so
  /// it must NOT be re-processed as MLS); false means "not a DR frame — fall through to MLS".
  /// CN: 由外部分发器（如同时路由 MLS 的应用共享 `onMessage`）向 DR 栈喂入一个 relay 帧。当且仅当
  /// 该帧是本设备消费的 DR `DmEnvelope`（为我解密、或寻址给兄弟设备故不得再按 MLS 处理）时返回
  /// true；false 表示「非 DR 帧——交回 MLS 处理」。
  ingestFrame(frame: RelayFrame): Promise<boolean> {
    return this.handleFrame(frame);
  }

  /// EN: X3DH initiator: establish the outbound session from a verified peer bundle.
  /// CN: X3DH 发起方：用已校验的对端预密钥包建立出站会话。
  async startSession(bundle: PeerPrekeyBundle): Promise<void> {
    await this.engine.initOutbound(bundle);
    await this.persist(bundle.device);
  }

  /// EN: Whether a live ratchet session exists for `peerDevice` (per-device, design §18.3).
  /// CN: `peerDevice` 是否有活跃棘轮会话（按设备，设计 §18.3）。
  hasSession(peerDevice: DeviceId): boolean {
    return this.engine.hasSession(peerDevice);
  }

  /// EN: This device's self-certifying id (`blake2_128(IK)`). CN: 本设备自证 id。
  selfDevice(): DeviceId {
    return this.engine.deviceId();
  }

  /// EN: Encrypt + send `plaintext` to `peerDevice` of `peerAccount`. Requires an existing
  /// session (`startSession` for the first message, or an inbound `Init` already created one).
  /// `opts.convId` overrides the routing conv-id (sibling echo addresses a sibling DEVICE but must
  /// route on the ORIGINAL conversation `d:{other_party}`, §8); `opts.echoSelf` retains the frame
  /// in the sender account's mailbox so offline siblings catch up (network relay). CN: 把
  /// `plaintext` 加密并发送给 `peerAccount` 的 `peerDevice`。需已有会话（首条先 `startSession`，或
  /// 入站 `Init` 已建会话）。`opts.convId` 覆盖路由 conv-id（兄弟回显寻址兄弟**设备**，但须按原
  /// 会话 `d:{对端}` 路由，§8）；`opts.echoSelf` 把帧留存到发送方账户邮箱供离线兄弟补齐（网络 relay）。
  async sendTo(
    peerAccount: string,
    peerDevice: DeviceId,
    plaintext: Uint8Array,
    opts?: { convId?: string; echoSelf?: boolean },
  ): Promise<void> {
    const env = await this.engine.encrypt({ peerDevice }, plaintext);
    const wire = this.engine.encodeForWire(env);
    const frame: RelayFrame = {
      convId: opts?.convId ?? directConvId(peerAccount),
      senderRef: this.selfRef,
      ciphertextB64: bytesToB64(wire),
    };
    await this.relay.send(opts?.echoSelf ? { ...frame, echoSelf: true } : frame);
    await this.persist(peerDevice);
  }

  /// EN: Decode + dispatch one relay frame. Non-DR frames (failed decode, wrong version,
  /// or addressed to another device) are ignored so the same relay can carry MLS too.
  /// CN: 解码并分发一个 relay 帧。非 DR 帧（解码失败 / 版本不符 / 寻址到别的设备）忽略，
  /// 使同一 relay 也能承载 MLS。
  private async handleFrame(frame: RelayFrame): Promise<boolean> {
    if (!frame.convId.startsWith("d:")) return false;
    let env: DmEnvelope;
    try {
      env = this.engine.decodeFromWire(b64ToBytes(frame.ciphertextB64));
    } catch {
      return false; // not a DmEnvelope (e.g. an MLS-Wire frame on the same conv-id space)
    }
    if (env.ver !== DM_ENVELOPE_VER) return false;
    // EN: a genuine DR frame, but only the addressed device processes it (multi-device routing,
    // §18.3). A frame for a sibling device is still consumed (return true) so the dispatcher does
    // NOT re-process it as MLS. CN: 确为 DR 帧，但仅被寻址设备处理（多设备路由，§18.3）。寻址给兄弟
    // 设备的帧仍消费（返回 true），使分发器不再按 MLS 处理。
    if (!eqBytes(env.recvDev, this.engine.deviceId())) return true;

    const peerDevice = env.senderDev;
    let plaintext: Uint8Array;
    if (this.engine.hasSession(peerDevice)) {
      plaintext = await this.engine.decrypt({ peerDevice }, env);
    } else if (env.kind === DmKind.Init) {
      const res = await this.engine.initInbound(env);
      plaintext = res.plaintext;
    } else {
      return true; // a Msg with no session — drop, but it WAS a DR frame (do not MLS-process)
    }
    await this.persist(peerDevice);

    const peerAccount = this.canon(frame.senderRef);
    for (const h of this.handlers) h({ peerAccount, peerDevice, plaintext, convId: frame.convId });
    return true;
  }

  /// EN: Persist the (mutated) session for `peerDevice` plus the account pickle (one-time
  /// keys / fallback state may have advanced). No-op without a store. CN: 持久化
  /// `peerDevice` 的（已变更）会话与账户 pickle（一次性钥/回退态可能已推进）。无 store 时空操作。
  private async persist(peerDevice: DeviceId): Promise<void> {
    if (!this.store) return;
    this.store.saveSession(toHex(peerDevice), this.engine.pickleSession(peerDevice));
    this.store.saveAccount(this.engine.pickle());
  }

  private canon(ref: string): string {
    try {
      return canonicalAddress(ref);
    } catch {
      return ref;
    }
  }
}

/// EN: Open an account's DR engine + transport, restoring the prior Olm account and all
/// per-peer-device sessions from an (encrypted) store — or bootstrapping a fresh device
/// identity and persisting it on first run. The single entry point app wiring should use
/// so ratchet state survives restart. CN: 打开某账户的 DR 引擎 + 传输：从（加密）存储恢复
/// 此前 Olm 账户与所有每对端设备会话——或首次运行时新建设备身份并持久化。应用接线应使用的
/// 唯一入口，使棘轮态跨重启存活。
export async function restoreDrTransport(opts: {
  account: string;
  relay: RelayClient;
  store: DrPersistence;
}): Promise<{ engine: VodozemacEngine; transport: DrTransport }> {
  const { account, relay, store } = opts;
  await store.open(account);
  const accountPickle = await store.loadAccount();
  const engine = new VodozemacEngine();
  await engine.init(accountPickle ?? undefined);
  if (!accountPickle) await store.saveAccount(engine.pickle());
  for (const devHex of await store.listSessions()) {
    const sessionPickle = await store.loadSession(devHex);
    if (sessionPickle) engine.loadSession(fromHex(devHex), sessionPickle);
  }
  const transport = new DrTransport(engine, relay, account, store);
  return { engine, transport };
}
