// EN: Ephemeral (burn-after-read) helpers (CHAT_P3 §4.4, CHAT_DEVICE_RETENTION §5).
// `burn_on: "deliver"` arms the deadline at send/receive; `burn_on: "read"` arms it when
// the conversation is opened (mark-read). CN: 阅后即焚辅助（P3 §4.4、设备保留 §5）。
// `burn_on: "deliver"` 在发送/送达时启动倒计时；`burn_on: "read"` 在打开会话时启动。

import type { MessageVM } from "@/types/viewModels";
import type { EnvelopeV1 } from "@/mls/envelope";

export type EphemeralBurnOn = "read" | "deliver";

export interface EphemeralMeta {
  ephemeralTtlMs?: number;
  ephemeralBurnOn?: EphemeralBurnOn;
  ephemeralBurnAt?: number;
}

/// EN: Extract ephemeral fields from a decrypted envelope. CN: 从解密信封提取 ephemeral 字段。
export function ephemeralFromEnvelope(env: EnvelopeV1): EphemeralMeta {
  if (!env.ephemeral) return {};
  return {
    ephemeralTtlMs: env.ephemeral.ttlMs,
    ephemeralBurnOn: env.ephemeral.burnOn,
  };
}

/// EN: Compute `ephemeralBurnAt` for a newly created message. CN: 为新消息计算 `ephemeralBurnAt`。
export function burnAtOnCreate(
  ttlMs: number,
  burnOn: EphemeralBurnOn,
  now = Date.now(),
): number | undefined {
  if (burnOn === "deliver") return now + ttlMs;
  return undefined;
}

/// EN: Arm read-mode messages when the conversation is opened. CN: 打开会话时为 read 模式消息启动倒计时。
export function burnAtOnRead(ttlMs: number, now = Date.now()): number {
  return now + ttlMs;
}

/// EN: Whether a message should be purged now. CN: 消息是否应在当前时刻被清除。
export function isExpired(msg: MessageVM, now = Date.now()): boolean {
  return msg.ephemeralBurnAt != null && msg.ephemeralBurnAt <= now;
}

/// EN: Relay-side expiry for ephemeral frames (deliver TTL). CN: relay 侧 ephemeral 帧过期时间。
export function relayExpiresAt(ttlMs: number, burnOn: EphemeralBurnOn, now = Date.now()): number | undefined {
  if (burnOn === "deliver") return now + ttlMs;
  // EN: read-mode gets a generous relay window so offline tabs can still receive.
  // CN: read 模式给 relay 更宽窗口，便于离线标签页仍能收到。
  return now + ttlMs * 4;
}
