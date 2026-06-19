// EN: `DmEnvelope` wire codec for the 1:1 Double Ratchet stack (design §18.1). Concrete
// little-endian binary layout (the SCALE-equivalent reference framing used by the TS
// client and the relay schema §21):
//
//   off  0 : ver         u8
//   off  1 : kind        u8
//   off  2 : senderDev   [16]
//   off 18 : recvDev     [16]
//   off 34 : prekeyEpoch u64 LE
//   off 42 : bodyLen     u32 LE
//   off 46 : body        [bodyLen]
//
// The first 42 bytes are the cleartext routing header (relay sees them; no content
// leak). `body` is the opaque vodozemac ciphertext.
// CN: 1:1 双棘轮栈的 `DmEnvelope` wire 编解码（设计 §18.1）。具体小端二进制布局（TS 客户端
// 与 relay schema §21 使用的 SCALE 等价参考框架）。前 42 字节为明文路由头（relay 可见、无
// 内容泄漏）；`body` 为不透明 vodozemac 密文。

import { DmKind, type DeviceId, type DmEnvelope } from "@/crypto-dr/types";
import { blake2AsU8a } from "@polkadot/util-crypto";

/// EN: Current envelope format version. CN: 当前信封格式版本。
export const DM_ENVELOPE_VER = 1;

const DEVICE_ID_LEN = 16;
const HEADER_LEN = 46; // 1 + 1 + 16 + 16 + 8 + 4

/// EN: Derive the self-certifying device id from an X25519 identity public key:
/// `DeviceId = blake2_128(ik)`. MUST match `pallet-msg-identity` on-chain derivation
/// (cross-language frozen vector). CN: 由 X25519 身份公钥派生自证设备 id：
/// `DeviceId = blake2_128(ik)`。必须与 `pallet-msg-identity` 链上派生一致（跨语言冻结向量）。
export function deviceIdFromIk(ik: Uint8Array): DeviceId {
  if (ik.length !== 32) {
    throw new Error(`deviceIdFromIk: expected 32-byte IK, got ${ik.length}`);
  }
  return blake2AsU8a(ik, 128);
}

/// EN: Encode a `DmEnvelope` to bytes (relay payload, before base64). Validates the
/// fixed-length fields. CN: 把 `DmEnvelope` 编码为字节（relay 载荷，base64 之前）。校验
/// 定长字段。
export function encodeDmEnvelope(env: DmEnvelope): Uint8Array {
  if (env.senderDev.length !== DEVICE_ID_LEN || env.recvDev.length !== DEVICE_ID_LEN) {
    throw new Error("encodeDmEnvelope: device ids must be 16 bytes");
  }
  if (env.prekeyEpoch < 0n || env.prekeyEpoch > 0xffff_ffff_ffff_ffffn) {
    throw new Error("encodeDmEnvelope: prekeyEpoch out of u64 range");
  }
  const out = new Uint8Array(HEADER_LEN + env.body.length);
  const view = new DataView(out.buffer);
  out[0] = env.ver & 0xff;
  out[1] = env.kind & 0xff;
  out.set(env.senderDev, 2);
  out.set(env.recvDev, 18);
  view.setBigUint64(34, env.prekeyEpoch, true);
  view.setUint32(42, env.body.length, true);
  out.set(env.body, HEADER_LEN);
  return out;
}

/// EN: Decode bytes into a `DmEnvelope`. Throws on truncation / length mismatch /
/// unknown kind. CN: 把字节解码为 `DmEnvelope`。截断 / 长度不符 / 未知类别时抛错。
export function decodeDmEnvelope(bytes: Uint8Array): DmEnvelope {
  if (bytes.length < HEADER_LEN) {
    throw new Error(`decodeDmEnvelope: too short (${bytes.length} < ${HEADER_LEN})`);
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const ver = bytes[0];
  const kindRaw = bytes[1];
  if (kindRaw !== DmKind.Init && kindRaw !== DmKind.Msg) {
    throw new Error(`decodeDmEnvelope: unknown kind ${kindRaw}`);
  }
  const senderDev = bytes.slice(2, 18);
  const recvDev = bytes.slice(18, 34);
  const prekeyEpoch = view.getBigUint64(34, true);
  const bodyLen = view.getUint32(42, true);
  if (bytes.length !== HEADER_LEN + bodyLen) {
    throw new Error(
      `decodeDmEnvelope: body length mismatch (have ${bytes.length - HEADER_LEN}, want ${bodyLen})`,
    );
  }
  const body = bytes.slice(HEADER_LEN, HEADER_LEN + bodyLen);
  return { ver, kind: kindRaw as DmKind, senderDev, recvDev, prekeyEpoch, body };
}

/// EN: Read only the cleartext routing header without copying the body — used by the
/// relay/transport layer to route without decrypting. CN: 仅读明文路由头、不复制 body——
/// 供 relay/传输层路由用，无需解密。
export function peekDmHeader(
  bytes: Uint8Array,
): { ver: number; kind: DmKind; senderDev: DeviceId; recvDev: DeviceId; prekeyEpoch: bigint } {
  if (bytes.length < HEADER_LEN) {
    throw new Error(`peekDmHeader: too short (${bytes.length} < ${HEADER_LEN})`);
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    ver: bytes[0],
    kind: bytes[1] as DmKind,
    senderDev: bytes.slice(2, 18),
    recvDev: bytes.slice(18, 34),
    prekeyEpoch: view.getBigUint64(34, true),
  };
}
