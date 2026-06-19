// EN: P3 MLS payload envelope (CHAT_P3_ADVANCED_OFFCHAIN_DESIGN.md §3 +
// CHAT_LARGE_FILE_SPEC.md §3). This is the decrypted logical structure carried
// inside an MLS application message. The chain is completely unaware of it.
// All P3 fields are optional → forward/backward compatible.
// CN: P3 MLS payload 信封（见对应设计文档）。这是 MLS 应用消息解密后的逻辑结构，
// 链对其完全无感。所有 P3 字段可选 → 前后向兼容。

export interface FileBody {
  rootCid: string;
  chunked: boolean;
  fileKey: string; // base64 (per-file AES key; only group members can read)
  mime: string;
  size: number;
  fileSha256?: string;
  thumbCid?: string;
  thumbKey?: string;
  durationMs?: number;
  name?: string;
}

export interface EnvelopeV1 {
  v: 1;
  /** client-generated message id (dedup / reference) */
  id: string;
  /** "text" | "image" | "video" | "audio" | "file" | "reaction" | ... */
  type: string;
  /** content or file reference */
  body: unknown;

  // ---- optional P3 interaction fields ----
  replyTo?: string;
  forward?: { fromMsg: string; fromConv: string; preview?: string };
  mentions?: string[];
  reaction?: { target: string; emoji: string; op: "add" | "remove" };
  ephemeral?: { ttlMs: number; burnOn: "read" | "deliver" };
  /** EN: client wall-clock send time (ms); restored on offline mailbox delivery for timeline order.
   *  CN: 客户端墙钟发送时间（毫秒）；离线邮箱投递时恢复，保证时间线顺序。 */
  sentAt?: number;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/// EN: Encode an envelope to bytes (JSON now; CBOR is a drop-in later).
/// CN: 信封编码为字节（暂 JSON；后续可无缝换 CBOR）。
export function encodeEnvelope(env: EnvelopeV1): Uint8Array {
  return encoder.encode(JSON.stringify(env));
}

/// EN: Decode bytes to an envelope. Unknown fields are preserved/ignored.
/// CN: 字节解码为信封；未知字段保留/忽略。
export function decodeEnvelope(bytes: Uint8Array): EnvelopeV1 {
  const obj = JSON.parse(decoder.decode(bytes)) as EnvelopeV1;
  if (obj.v !== 1) throw new Error(`unsupported envelope version: ${obj.v}`);
  return obj;
}

/// EN: Stamp `sentAt` when missing so offline mailbox replay preserves send order.
/// CN: 缺失时写入 `sentAt`，使离线邮箱重放保留发送顺序。
export function stampEnvelopeSentAt(env: EnvelopeV1, sentAt = Date.now()): EnvelopeV1 {
  if (typeof env.sentAt === "number" && env.sentAt > 0) return env;
  return { ...env, sentAt };
}

/// EN: Read envelope send time with safe fallback. CN: 读取信封发送时间（带安全回退）。
export function envelopeSentAt(env: EnvelopeV1, fallback = Date.now()): number {
  return typeof env.sentAt === "number" && env.sentAt > 0 ? env.sentAt : fallback;
}

/// EN: Helper to build a text envelope. CN: 构造文本信封。
export function textEnvelope(
  id: string,
  text: string,
  opts: {
    replyTo?: string;
    forward?: { fromMsg: string; fromConv: string; preview?: string };
    mentions?: string[];
    ephemeralMs?: number;
  } = {},
): EnvelopeV1 {
  return {
    v: 1,
    id,
    type: "text",
    body: { text },
    replyTo: opts.replyTo,
    forward: opts.forward,
    mentions: opts.mentions,
    ephemeral: opts.ephemeralMs ? { ttlMs: opts.ephemeralMs, burnOn: "read" } : undefined,
  };
}

/// EN: Build a media-download ack (`type=media_ack`) — receiver tells the sender the full
/// body was fetched so the sender may release its local pin early (1:1 retention, not a
/// read receipt). Not rendered as a visible message.
/// CN: 构造媒体下载确认（`type=media_ack`）——接收方告知发送方正文已取回，发送方可提前释放
/// 本机 pin（1:1 retention 用，非已读回执）。不渲染为可见消息。
export function mediaAckEnvelope(id: string, target: string): EnvelopeV1 {
  return {
    v: 1,
    id,
    type: "media_ack",
    body: { target },
  };
}

/// EN: Build a recall envelope (`type=recall`) — the SENDER asks both sides to hide a
/// previously sent message (`target` = its client msg id). Like `media_ack` it is a control
/// message and is NOT rendered as its own bubble; the receiver flips the target to the
/// `recalled` placeholder. CN: 构造撤回信封（`type=recall`）——由**发送方**请求收发双方隐藏一条
/// 已发消息（`target` = 其 client msg id）。与 `media_ack` 一样是控制消息，不渲染为独立气泡；
/// 接收方将目标消息翻转为「已撤回」占位。
export function recallEnvelope(id: string, target: string): EnvelopeV1 {
  return {
    v: 1,
    id,
    type: "recall",
    body: { target },
  };
}

/// EN: Build a reaction envelope (`type=reaction`). CN: 构造 reaction 信封。
export function reactionEnvelope(
  id: string,
  target: string,
  emoji: string,
  op: "add" | "remove" = "add",
): EnvelopeV1 {
  return {
    v: 1,
    id,
    type: "reaction",
    body: {},
    reaction: { target, emoji, op },
  };
}

/// EN: Build a file/image/video/audio envelope (body = FileBody). CN: 构造文件类信封。
export function fileEnvelope(
  id: string,
  type: "image" | "video" | "audio" | "file",
  body: FileBody,
  opts: {
    replyTo?: string;
    forward?: { fromMsg: string; fromConv: string; preview?: string };
    ephemeralMs?: number;
  } = {},
): EnvelopeV1 {
  return {
    v: 1,
    id,
    type,
    body,
    replyTo: opts.replyTo,
    forward: opts.forward,
    ephemeral: opts.ephemeralMs ? { ttlMs: opts.ephemeralMs, burnOn: "read" } : undefined,
  };
}
