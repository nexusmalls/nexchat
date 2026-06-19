// EN: Track A online-handoff wire messages (design §5.2). These are the two frames exchanged over the
// account self-channel (`s:<account>`, the same account-scoped device transport Track B uses): a
// REQUEST minted by the new device (carrying its directory-key-endorsed peer public key so the old
// primary needs no separate registry lookup), and a GRANT minted by the old primary (the §5 signed
// receipt + the signing-key bundle sealed to the requester's peer key). This module is a PURE codec +
// validator; the actual frame send/receive (relay WebSocket) and the React handoff UI live in the app
// runtime and bind `sealHandoff`/`openHandoff` (handoffCoordinator) to these messages.
// CN: 路线 A 在线交接线消息（设计 §5.2）。这是经账户自通道（`s:<account>`，与路线 B 同一账户级设备传输）交换
// 的两类帧：新设备铸造的 REQUEST（携带其经目录钥背书的对端公钥，使旧主设备无需单独注册表查询），与旧主设备
// 铸造的 GRANT（§5 签名收据 + 封装给请求方对端钥的签名钥 bundle）。本模块为**纯**编解码 + 校验；实际帧收发
// （relay WebSocket）与 React 交接 UI 在应用运行时，绑定 `sealHandoff`/`openHandoff`（handoffCoordinator）。

import type { DevicePeerEndorsement, SealedHandoffPayload } from "@/mls/devicePeerKey";

/// EN: REQUEST: new device → old primary. `from` is the requesting device id; `endorsement` binds it
/// to its peer public key under the account directory key. CN: REQUEST：新设备 → 旧主。`from` 为请求方
/// 设备 id；`endorsement` 用账户目录钥把它绑定到其对端公钥。
export interface HandoffRequestMessage {
  t: "handoff-request";
  v: 1;
  from: string;
  endorsement: DevicePeerEndorsement;
}

/// EN: GRANT: old primary → new device. Carries the §5 signed receipt + the sealed signing-key bundle.
/// CN: GRANT：旧主 → 新设备。携带 §5 签名收据 + 封装签名钥 bundle。
export interface HandoffGrantMessage {
  t: "handoff-grant";
  v: 1;
  payload: SealedHandoffPayload;
}

export type HandoffMessage = HandoffRequestMessage | HandoffGrantMessage;

export function encodeHandoffMessage(m: HandoffMessage): string {
  return JSON.stringify(m);
}

function isEndorsement(x: unknown): x is DevicePeerEndorsement {
  if (!x || typeof x !== "object") return false;
  const e = x as Record<string, unknown>;
  return (
    typeof e.deviceId === "string" &&
    typeof e.peerPublicKey === "string" &&
    typeof e.sig === "string"
  );
}

function isSealedPayload(x: unknown): x is SealedHandoffPayload {
  if (!x || typeof x !== "object") return false;
  const p = x as Record<string, unknown>;
  if (typeof p.sealedBundle !== "string") return false;
  const r = p.receipt as Record<string, unknown> | undefined;
  if (!r || typeof r.sig !== "string") return false;
  const rec = r.receipt as Record<string, unknown> | undefined;
  return (
    !!rec &&
    rec.v === 1 &&
    typeof rec.from === "string" &&
    typeof rec.to === "string" &&
    typeof rec.seq === "number" &&
    typeof rec.ts === "number"
  );
}

/// EN: Decode + validate a self-channel handoff frame payload; null on any shape/version mismatch
/// (never throws). CN: 解码并校验自通道交接帧载荷；形状/版本不符返回 null（绝不抛错）。
export function decodeHandoffMessage(raw: string): HandoffMessage | null {
  let obj: unknown;
  try {
    obj = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!obj || typeof obj !== "object") return null;
  const m = obj as Record<string, unknown>;
  if (m.v !== 1) return null;
  if (m.t === "handoff-request") {
    if (typeof m.from !== "string" || !isEndorsement(m.endorsement)) return null;
    return { t: "handoff-request", v: 1, from: m.from, endorsement: m.endorsement };
  }
  if (m.t === "handoff-grant") {
    if (!isSealedPayload(m.payload)) return null;
    return { t: "handoff-grant", v: 1, payload: m.payload };
  }
  return null;
}
