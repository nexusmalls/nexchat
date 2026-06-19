// EN: Bind `t ‖ ct ‖ epoch` into the RSABSSA message (§6 anti-rebind).
// CN: 将 `t ‖ ct ‖ epoch` 绑入 RSABSSA 消息（§6 防重绑）。

const enc = new TextEncoder();

/// EN: Deterministic per-contact tag both parties derive (relay-visible pseudo-id, §4).
/// CN: 双方可确定的每联系人标签（relay 可见伪名，§4）。
export async function deriveContactTag(
  receiver: string,
  sender: string,
): Promise<Uint8Array> {
  const raw = await crypto.subtle.digest(
    "SHA-256",
    enc.encode(`nexchat/ct/v1:${receiver}:${sender}`),
  );
  return new Uint8Array(raw);
}

export function buildTokenMessage(t: Uint8Array, ct: Uint8Array, epoch: number): Uint8Array {
  if (t.length !== 32 || ct.length !== 32) throw new Error("t and ct must be 32 bytes");
  const out = new Uint8Array(68);
  out.set(t, 0);
  out.set(ct, 32);
  new DataView(out.buffer).setUint32(64, epoch >>> 0, false);
  return out;
}
