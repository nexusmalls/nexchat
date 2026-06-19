// EN: VodozemacEngine — the real Double Ratchet engine backed by the `nexchat-dr` WASM
// module (vodozemac/Olm). Implements `DrEngine` over `DmEnvelope`, plus the prekey /
// identity surface the X3DH publication flow needs (`pallet-msg-identity`). All 1:1
// cryptography stays inside the WASM client; this layer only maps DmEnvelope ⇄ Olm
// parts and routes by peer device id. STRICTLY decoupled from `@/mls/*` (design §2/§3).
// CN: VodozemacEngine —— 由 `nexchat-dr` WASM 模块（vodozemac/Olm）支撑的真实双棘轮引擎。
// 在 `DmEnvelope` 上实现 `DrEngine`，并补齐 X3DH 发布流程所需的预密钥 / 身份接口
// （`pallet-msg-identity`）。1:1 密码学全在 WASM 客户端内，本层仅做 DmEnvelope ⇄ Olm parts
// 映射并按对端设备 id 路由。与 `@/mls/*` 严格解耦（设计 §2/§3）。

import initWasm, { DrClient, type InitInput } from "@/dr-pkg/nexchat_dr.js";
import wasmUrl from "@/dr-pkg/nexchat_dr_bg.wasm?url";
import { decodeDmEnvelope, deviceIdFromIk, encodeDmEnvelope } from "@/crypto-dr/dmEnvelope";
import {
  DmKind,
  type DeviceId,
  type DmEnvelope,
  type DrEngine,
  type DrSessionHandle,
  type PeerPrekeyBundle,
  type X25519Pub,
} from "@/crypto-dr/types";

let wasmReady: Promise<void> | null = null;

/// EN: Initialise the WASM module once (idempotent). In the browser build the bundled
/// `?url` asset is used; pass `input` (e.g. raw bytes) to override under node/tests.
/// CN: 仅初始化一次 WASM 模块（幂等）。浏览器构建用打包的 `?url` 资源；node/测试下可传
/// `input`（如原始字节）覆盖。
export function ensureDrWasm(input?: InitInput): Promise<void> {
  if (!wasmReady) {
    wasmReady = initWasm({ module_or_path: input ?? wasmUrl }).then(() => undefined);
  }
  return wasmReady;
}

const toHex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

const eqBytes = (a: Uint8Array, b: Uint8Array): boolean =>
  a.length === b.length && a.every((x, i) => x === b[i]);

/// EN: Double Ratchet engine over the vodozemac WASM client. Construct, then `init()`
/// (fresh or restored), then use as a `DrEngine`. CN: 基于 vodozemac WASM 客户端的双棘轮
/// 引擎。先构造、再 `init()`（新建或恢复），随后作为 `DrEngine` 使用。
export class VodozemacEngine implements DrEngine {
  private client: DrClient | null = null;
  private myDevice: DeviceId | null = null;
  /// EN: Sender's last-known on-chain `prekey_epoch` of each peer device (stamped into
  /// outbound envelopes). CN: 发送方所知各对端设备的链上 `prekey_epoch`（盖入出站信封）。
  private peerEpoch = new Map<string, bigint>();

  /// EN: Bring up the WASM client. With `pickle`, restore the prior Olm account; else
  /// create a fresh device identity. CN: 启动 WASM 客户端。给 `pickle` 则恢复此前 Olm 账户；
  /// 否则新建设备身份。
  async init(pickle?: string): Promise<void> {
    await ensureDrWasm();
    this.client = pickle ? DrClient.restore(pickle) : new DrClient();
    this.myDevice = deviceIdFromIk(this.client.identityKey());
  }

  private get c(): DrClient {
    if (!this.client) throw new Error("VodozemacEngine not initialised");
    return this.client;
  }

  // ---- identity / prekey publication surface (pallet-msg-identity) ----

  /// EN: This device's X25519 identity key (`IK`). CN: 本设备 X25519 身份钥（`IK`）。
  identityKey(): X25519Pub {
    return this.c.identityKey();
  }

  /// EN: This device's self-certifying device id `= blake2_128(IK)`. CN: 本设备自证
  /// 设备 id `= blake2_128(IK)`。
  deviceId(): DeviceId {
    if (!this.myDevice) throw new Error("VodozemacEngine not initialised");
    return this.myDevice;
  }

  /// EN: Generate `n` one-time prekeys; returns their public keys (`OPK`, 32 bytes each)
  /// for Merkle-root publication. CN: 生成 `n` 个一次性预密钥；返回其公钥（`OPK`，每个 32
  /// 字节）用于 Merkle 根发布。
  generateOneTimePreKeys(n: number): X25519Pub[] {
    this.c.generateOneTimeKeys(n);
    return splitKeys(this.c.oneTimeKeys());
  }

  /// EN: Rotate + return the signed prekey (`SPK`, the Olm fallback key). CN: 轮换并返回
  /// 签名预密钥（`SPK`，即 Olm 回退钥）。
  rotateSignedPreKey(): X25519Pub {
    this.c.generateFallbackKey();
    const spk = this.c.fallbackKey();
    if (!spk) throw new Error("rotateSignedPreKey: no fallback key produced");
    return spk;
  }

  /// EN: Mark all currently unpublished keys as published (after on-chain commit).
  /// CN: 把当前所有未发布钥标记为已发布（链上提交成功后）。
  markKeysAsPublished(): void {
    this.c.markKeysAsPublished();
  }

  /// EN: Serialize account state (TS store encrypts at rest). CN: 序列化账户状态（TS 存储
  /// 层负责落盘加密）。
  pickle(): string {
    return this.c.pickle();
  }

  // ---- DrEngine ----

  async initOutbound(bundle: PeerPrekeyBundle): Promise<DrSessionHandle> {
    if (deviceIdFromIk(bundle.ik).join() !== bundle.device.join()) {
      throw new Error("initOutbound: bundle.device does not match blake2_128(ik)");
    }
    const prekey = bundle.opk ?? bundle.spk; // OPK preferred, SPK fallback (design §6)
    this.c.createOutboundSession(toHex(bundle.device), bundle.ik, prekey);
    this.peerEpoch.set(toHex(bundle.device), bundle.prekeyEpoch);
    return { peerDevice: bundle.device };
  }

  async initInbound(
    env: DmEnvelope,
  ): Promise<{ session: DrSessionHandle; plaintext: Uint8Array }> {
    if (env.kind !== DmKind.Init) {
      throw new Error("initInbound: envelope is not a dm_init (DmKind.Init)");
    }
    const peerHex = toHex(env.senderDev);
    const res = this.c.createInboundSession(peerHex, env.body);
    try {
      const senderIk = res.identity_key;
      const plaintext = res.plaintext;
      // 校验信封头声明的 sender_dev 确实由其 IK 派生（防错误路由 / 头伪造）。
      // Verify the header's sender_dev truly derives from the recovered IK.
      if (!eqBytes(deviceIdFromIk(senderIk), env.senderDev)) {
        throw new Error("initInbound: sender_dev != blake2_128(recovered IK)");
      }
      return { session: { peerDevice: env.senderDev }, plaintext };
    } finally {
      res.free();
    }
  }

  async encrypt(session: DrSessionHandle, plaintext: Uint8Array): Promise<DmEnvelope> {
    const peerHex = toHex(session.peerDevice);
    const out = this.c.encrypt(peerHex, plaintext);
    const msgType = out[0];
    const body = out.slice(1);
    return {
      ver: 1,
      kind: msgType === 0 ? DmKind.Init : DmKind.Msg,
      senderDev: this.deviceId(),
      recvDev: session.peerDevice,
      prekeyEpoch: this.peerEpoch.get(peerHex) ?? 0n,
      body,
    };
  }

  async decrypt(session: DrSessionHandle, env: DmEnvelope): Promise<Uint8Array> {
    const peerHex = toHex(session.peerDevice);
    const msgType = env.kind === DmKind.Init ? 0 : 1;
    return this.c.decrypt(peerHex, msgType, env.body);
  }

  /// EN: Whether a live ratchet session exists for `peerDevice`. CN: `peerDevice` 是否有活跃棘轮会话。
  hasSession(peerDevice: DeviceId): boolean {
    return this.c.hasSession(toHex(peerDevice));
  }

  /// EN: Serialize the ratchet session for `peerDevice` (JSON pickle; the store encrypts it
  /// at rest, §17.2). Throws if there is no session. CN: 序列化 `peerDevice` 的棘轮会话
  /// （JSON pickle；由存储层静态加密，§17.2）。无会话时抛错。
  pickleSession(peerDevice: DeviceId): string {
    return this.c.pickleSession(toHex(peerDevice));
  }

  /// EN: Load a ratchet session for `peerDevice` from a JSON pickle (restore path). CN: 由
  /// JSON pickle 为 `peerDevice` 载入棘轮会话（恢复路径）。
  loadSession(peerDevice: DeviceId, pickle: string): void {
    this.c.loadSession(toHex(peerDevice), pickle);
  }

  // ---- transport glue ----

  /// EN: Encode an envelope to the relay payload bytes (`RelayFrame.ciphertextB64`
  /// pre-base64, §18.1). CN: 把信封编码为 relay 载荷字节（`RelayFrame.ciphertextB64`
  /// base64 之前，§18.1）。
  encodeForWire(env: DmEnvelope): Uint8Array {
    return encodeDmEnvelope(env);
  }

  /// EN: Decode relay payload bytes into an envelope. CN: 把 relay 载荷字节解码为信封。
  decodeFromWire(bytes: Uint8Array): DmEnvelope {
    return decodeDmEnvelope(bytes);
  }
}

/// EN: Split an `n × 32`-byte buffer into individual Curve25519 public keys. CN: 把
/// `n × 32` 字节缓冲拆为单个 Curve25519 公钥。
function splitKeys(buf: Uint8Array): X25519Pub[] {
  if (buf.length % 32 !== 0) throw new Error("splitKeys: not a multiple of 32 bytes");
  const out: X25519Pub[] = [];
  for (let i = 0; i < buf.length; i += 32) out.push(buf.slice(i, i + 32));
  return out;
}
