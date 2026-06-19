// EN: Attach RFC 9474 blind delivery tokens to 1:1 sends. The `sealedSender` blob hides the sender
// from **other clients** (receiver decrypts with the pairwise MLS key); the relay still learns routing
// metadata (`convId`, authenticated WS account, plaintext `delivery.mlsKey`) — not Signal-style
// metadata privacy. CN: 为 1:1 发送附加 RFC 9474 盲签投递令牌。`sealedSender` 对**其它客户端**隐藏发送方
// （接收方用 pairwise MLS 密钥解密）；relay 仍可见路由元数据（`convId`、已认证 WS 账户、明文 `delivery.mlsKey`）
// ——**非** Signal 式元数据隐私。

import { config } from "@/config";
import { directMlsKey, peerFromMlsKey, resolveMlsConvId } from "@/mls/directConv";
import { sealSender } from "@/delivery/sealedSender";
import { lookupInbox } from "@/delivery/inboxManager";
import type { InboxManager } from "@/delivery/inboxManager";
import type { TokenWallet } from "@/delivery/tokenWallet";
import type { RelayClient, RelayFrame } from "@/relay/relayClient";

const spentByInbox = new Map<string, Set<string>>();
const tokenPrefetchInflight = new Map<string, Promise<void>>();
const tokenPrefetchBackoffUntil = new Map<string, number>();
const tokenPrefetchWarned = new Set<string>();
const TOKEN_PREFETCH_BACKOFF_MS = 120_000;

export function receiverDedupT(inboxId: string, t: string): boolean {
  let set = spentByInbox.get(inboxId);
  if (!set) {
    set = new Set();
    spentByInbox.set(inboxId, set);
  }
  if (set.has(t)) return false;
  set.add(t);
  return true;
}

function peerOfflineOnRelay(msg: string): boolean {
  return (
    msg.includes("对端信箱未在 relay 注册") ||
    msg.includes("对端未在 relay 在线") ||
    msg.includes("投递令牌申领超时")
  );
}

function warnDeliveryTokensSkipped(peer: string, detail: string): void {
  tokenPrefetchBackoffUntil.set(peer, Date.now() + TOKEN_PREFETCH_BACKOFF_MS);
  if (tokenPrefetchWarned.has(peer)) return;
  tokenPrefetchWarned.add(peer);
  console.warn(
    `[nexchat] delivery tokens unavailable for ${peer.slice(0, 8)}… (${detail}); sending without sealed-sender token`,
  );
}

/// EN: Request blind tokens if needed. When peer is offline on relay, skip (MLS send still works).
/// CN: 必要时盲签申领；对端未在 relay 在线时跳过（仍可走 MLS 明文 relay 发送）。
export async function ensureDeliveryTokens(
  peer: string,
  selfAddress: string,
  wallet: TokenWallet,
  relay: RelayClient,
  endpointId: string,
  mlsKey: string,
  timeoutMs = 20_000,
): Promise<void> {
  if (!config.deliveryTokensEnabled) return;
  if (wallet.count(peer) > 0) return;
  const backoffUntil = tokenPrefetchBackoffUntil.get(peer);
  if (backoffUntil != null && Date.now() < backoffUntil) return;
  const inflight = tokenPrefetchInflight.get(peer);
  if (inflight) return inflight;

  const job = (async () => {
    try {
      const wait = wallet.waitForTokens(peer, timeoutMs);
      try {
        await wallet.requestBatch(peer, selfAddress, relay, endpointId, mlsKey);
        await wait;
      } catch (inner) {
        wallet.cancelWait(peer);
        throw inner;
      }
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e);
      if (peerOfflineOnRelay(detail)) {
        warnDeliveryTokensSkipped(peer, detail);
        return;
      }
      throw new Error(`无法向 ${peer.slice(0, 8)}… 申领投递令牌：${detail}`);
    }
    if (wallet.count(peer) === 0) {
      warnDeliveryTokensSkipped(peer, "timeout");
      return;
    }
    tokenPrefetchBackoffUntil.delete(peer);
    tokenPrefetchWarned.delete(peer);
  })();

  tokenPrefetchInflight.set(peer, job);
  try {
    await job;
  } finally {
    tokenPrefetchInflight.delete(peer);
  }
}

export async function attachDelivery(
  frame: RelayFrame,
  peer: string,
  selfAddress: string,
  wallet: TokenWallet,
): Promise<RelayFrame> {
  if (!config.deliveryTokensEnabled) return frame;
  const inbox = await lookupInbox(peer);
  const admission = wallet.consume(peer, inbox?.epoch);
  if (!admission) return frame;
  const mlsKey = directMlsKey(selfAddress, peer);
  const sealedSender = await sealSender(selfAddress, mlsKey);
  return {
    ...frame,
    senderRef: "",
    delivery: { ...admission, sealedSender, mlsKey },
  };
}

export async function resolveInboundSender(
  frame: RelayFrame,
  selfAddress: string,
): Promise<string> {
  if (frame.delivery?.sealedSender && frame.convId.startsWith("d:")) {
    const { unsealSender } = await import("@/delivery/sealedSender");
    const mlsKey =
      frame.delivery.mlsKey ?? resolveMlsConvId(frame.convId, selfAddress);
    try {
      return await unsealSender(frame.delivery.sealedSender, mlsKey);
    } catch {
      const fromKey = frame.delivery.mlsKey
        ? peerFromMlsKey(frame.delivery.mlsKey, selfAddress)
        : null;
      if (fromKey) return fromKey;
      return frame.senderRef || "peer";
    }
  }
  if (frame.delivery && !receiverDedupT(frame.delivery.inboxId, frame.delivery.t)) {
    throw new Error("duplicate delivery token");
  }
  return frame.senderRef || "peer";
}

export async function bootstrapDelivery(
  inbox: InboxManager,
  wallet: TokenWallet,
  selfAddress: string,
): Promise<void> {
  if (!config.deliveryTokensEnabled) return;
  await inbox.ensure(selfAddress);
  await wallet.load();
  await inbox.registerRelay(selfAddress);
}
