// EN: Relay mailbox fetch for offline chat ciphertext frames (chat_fetch / chat_reply).
// CN: 离线聊天密文帧的 relay 邮箱拉取（chat_fetch / chat_reply）。

import { config } from "@/config";
import { relayOneShotFetch, relayOneShotSend } from "@/relay/relayOneShot";
import type { RelayFrame } from "@/relay/relayClient";

function isRelayFrame(raw: unknown): raw is RelayFrame {
  if (!raw || typeof raw !== "object") return false;
  const m = raw as Record<string, unknown>;
  return (
    typeof m.convId === "string" &&
    typeof m.senderRef === "string" &&
    typeof m.ciphertextB64 === "string"
  );
}

/// EN: Parse `chat_reply` wire payload (exported for unit tests). CN: 解析 `chat_reply`（单测导出）。
export function parseChatMailboxReply(
  data: unknown,
  requestId: string,
): RelayFrame[] | null {
  try {
    const m = (typeof data === "string" ? JSON.parse(data) : data) as {
      type?: string;
      request_id?: string;
      frames?: unknown[];
    };
    if (m.type !== "chat_reply" || m.request_id !== requestId) return null;
    return (m.frames ?? []).filter(isRelayFrame);
  } catch {
    return null;
  }
}

/// EN: Pull pending chat frames stored for `account` on the relay. Uses signed
/// `register_account` so `RELAY_STRICT_AUTH=1` deployments can fetch safely.
/// CN: 从 relay 拉取账户待投递聊天帧；带签名 `register_account`，生产 strict_auth 可安全拉取。
export async function fetchChatMailbox(account: string): Promise<RelayFrame[]> {
  const raw = await relayOneShotFetch<RelayFrame[]>(
    account,
    { type: "chat_fetch" },
    (m, requestId) => parseChatMailboxReply(m, requestId) ?? undefined,
    8000,
  );
  return raw ?? [];
}

/// EN: Ops / single-device opt-in — drop processed dedup keys on relay (not default; multi-device unsafe).
/// CN: 运维/单设备可选——在 relay 删除已处理 dedupKey（默认不启用；多端不安全）。
export async function consumeChatMailbox(
  account: string,
  dedupKeys: readonly string[],
): Promise<void> {
  if (!config.relayWs || dedupKeys.length === 0) return;
  await relayOneShotSend(
    account,
    { type: "chat_consume", account, dedup_keys: [...dedupKeys] },
    { ackType: "chat_ack" },
  );
}

/// EN: Stable dedup key for outbound frames (matches relay mailbox + InboundDedup). CN: 出站帧稳定去重键。
export function relayFrameDedupKey(convId: string, clientMsgId: string): string {
  return `${convId}:${clientMsgId}`;
}
