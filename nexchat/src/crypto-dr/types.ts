// EN: Types & interfaces for the decentralized 1:1 stack (X3DH + Double Ratchet),
// strictly decoupled from the OpenMLS group stack. This module is the *engine
// boundary*: pure types + codecs live here and in `dmEnvelope.ts`; the actual
// cryptography (vodozemac/Olm) is injected via `DrEngine` (see §17 / §18 of
// `pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`).
// CN: 去中心化 1:1 栈（X3DH + 双棘轮）的类型与接口，与 OpenMLS 群栈严格解耦。本模块是
// *引擎边界*：纯类型 + 编解码在此与 `dmEnvelope.ts`；真正的密码学（vodozemac/Olm）经
// `DrEngine` 注入（见设计 §17 / §18）。

/// EN: Self-certifying device id = blake2_128(ik_x25519_pub), 16 bytes. MUST match the
/// on-chain `pallet-msg-identity` derivation. CN: 自证设备 id = blake2_128(ik_x25519_pub)，
/// 16 字节。必须与链上 `pallet-msg-identity` 派生一致。
export type DeviceId = Uint8Array; // length 16

/// EN: A Curve25519 (X25519) public key, 32 bytes. CN: Curve25519（X25519）公钥，32 字节。
export type X25519Pub = Uint8Array; // length 32

/// EN: `ChatStackCaps.flags` bit — client supports the X3DH+Double-Ratchet 1:1 stack.
/// MUST match `pallet-msg-identity::STACK_DR`. CN: `ChatStackCaps.flags` 位——客户端支持
/// X3DH+双棘轮 1:1 栈。必须与 `pallet-msg-identity::STACK_DR` 一致。
export const STACK_DR = 0b0000_0001;
/// EN: `ChatStackCaps.flags` bit — client supports the pairwise-MLS-Wire 1:1 stack.
/// MUST match `pallet-msg-identity::STACK_MLS_WIRE`. CN: `ChatStackCaps.flags` 位——客户端
/// 支持 pairwise MLS Wire 1:1 栈。必须与 `pallet-msg-identity::STACK_MLS_WIRE` 一致。
export const STACK_MLS_WIRE = 0b0000_0010;

/// EN: DmEnvelope kind discriminant. CN: DmEnvelope 类别判别值。
export enum DmKind {
  /// EN: First message of a session — carries the X3DH / Olm PreKeyMessage.
  /// CN: 会话首条——携带 X3DH / Olm PreKeyMessage。
  Init = 0,
  /// EN: Subsequent message — carries an Olm (Double Ratchet) Message.
  /// CN: 后续消息——携带 Olm（双棘轮）Message。
  Msg = 1,
}

/// EN: Decoded 1:1 direct-message envelope (wire format §18.1). The cleartext header
/// is relay-routable and leaks no content; `body` is the opaque vodozemac ciphertext.
/// CN: 解码后的 1:1 私信信封（wire 格式 §18.1）。明文头供 relay 路由、无内容泄漏；
/// `body` 为不透明 vodozemac 密文。
export interface DmEnvelope {
  /// EN: Format version (currently 1). CN: 格式版本（当前 1）。
  ver: number;
  /// EN: `DmKind`. CN: `DmKind`。
  kind: DmKind;
  /// EN: Sender device id (16 bytes). CN: 发送方设备 id（16 字节）。
  senderDev: DeviceId;
  /// EN: Receiver device id (16 bytes), for multi-device routing. CN: 接收方设备 id（16 字节）。
  recvDev: DeviceId;
  /// EN: Sender's view of the peer's on-chain `prekey_epoch` (stale → re-fetch/reject).
  /// CN: 发送方所见对端链上 `prekey_epoch`（陈旧 → 重取/拒绝）。
  prekeyEpoch: bigint;
  /// EN: Opaque ciphertext: Olm PreKeyMessage (Init) or Message (Msg). CN: 不透明密文。
  body: Uint8Array;
}

/// EN: A peer device's X3DH prekey bundle, fetched from chain (`IK`/`SPK` + endorsement)
/// and relay (one `OPK` leaf + Merkle proof). Verified relay-trustlessly before use.
/// CN: 对端设备的 X3DH 预密钥包：链上取 `IK`/`SPK` + 背书，relay 取一条 `OPK` 叶 + Merkle
/// 证明；使用前做 relay-trustless 校验。
export interface PeerPrekeyBundle {
  account: string;
  device: DeviceId;
  ik: X25519Pub;
  ikEndorsement: Uint8Array; // 64
  spk: X25519Pub;
  spkEndorsement: Uint8Array; // 64
  /// EN: Optional one-time prekey (absent → SPK fallback X3DH). CN: 可选一次性预密钥（缺省 → SPK 回退）。
  opk?: X25519Pub;
  prekeyEpoch: bigint;
}

/// EN: Opaque handle to a live Double Ratchet session, owned by the engine. CN: 引擎持有的
/// 活跃双棘轮会话不透明句柄。
export interface DrSessionHandle {
  /// EN: Routing peer device this session talks to. CN: 此会话对应的对端设备。
  peerDevice: DeviceId;
}

/// EN: The Double Ratchet engine boundary. The real implementation wraps vodozemac
/// (Olm `Account`/`Session`) compiled to wasm (M2). Keeping it an interface enforces the
/// decoupling invariant: nothing here may import the MLS engine. CN: 双棘轮引擎边界。真实
/// 实现包裹编为 wasm 的 vodozemac（Olm `Account`/`Session`，M2）。以接口隔离强制解耦不变量：
/// 此处不得 import MLS 引擎。
export interface DrEngine {
  /// EN: Establish an outbound session to a peer via X3DH; returns the handle.
  /// CN: 经 X3DH 建立到对端的出站会话；返回句柄。
  initOutbound(bundle: PeerPrekeyBundle): Promise<DrSessionHandle>;

  /// EN: Process an inbound `Init` envelope: build the inbound session and return the
  /// decrypted first plaintext. CN: 处理入站 `Init` 信封：建立入站会话并返回首条明文。
  initInbound(env: DmEnvelope): Promise<{ session: DrSessionHandle; plaintext: Uint8Array }>;

  /// EN: Encrypt `plaintext` into a `Msg` envelope on an existing session. CN: 在既有
  /// 会话上把 `plaintext` 加密为 `Msg` 信封。
  encrypt(session: DrSessionHandle, plaintext: Uint8Array): Promise<DmEnvelope>;

  /// EN: Decrypt a `Msg` envelope on an existing session. CN: 在既有会话上解密 `Msg` 信封。
  decrypt(session: DrSessionHandle, env: DmEnvelope): Promise<Uint8Array>;
}

/// EN: Persistent store boundary for serialized (pickled) DR sessions, keyed by peer
/// device. Physically isolated from MLS state (design §9). CN: 序列化（pickle）DR 会话的
/// 持久化边界，按对端设备键控。与 MLS 状态物理隔离（设计 §9）。
export interface DrSessionStore {
  load(peerDevice: DeviceId): Promise<Uint8Array | null>;
  save(peerDevice: DeviceId, pickle: Uint8Array): Promise<void>;
  remove(peerDevice: DeviceId): Promise<void>;
}
