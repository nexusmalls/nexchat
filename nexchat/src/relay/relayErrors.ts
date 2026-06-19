// EN: Parse relay server reject envelopes (`*_reject`, `auth_reject`) for client recovery.
// CN: 解析 relay 服务端拒绝回执（`*_reject`、`auth_reject`），供客户端恢复。

export class RelayStalePointerError extends Error {
  readonly rejectType: string;
  readonly remoteUpdatedAt: number;

  constructor(rejectType: string, remoteUpdatedAt: number) {
    super(`relay pointer stale (${rejectType}, remote updated_at=${remoteUpdatedAt})`);
    this.name = "RelayStalePointerError";
    this.rejectType = rejectType;
    this.remoteUpdatedAt = remoteUpdatedAt;
  }
}

export class RelayInboxStaleEpochError extends Error {
  readonly remoteEpoch: number;

  constructor(remoteEpoch: number) {
    super(`relay inbox stale_epoch (remote epoch=${remoteEpoch})`);
    this.name = "RelayInboxStaleEpochError";
    this.remoteEpoch = remoteEpoch;
  }
}

export class RelayAuthRejectError extends Error {
  readonly op: string;

  constructor(op: string) {
    super(`relay auth rejected: ${op}`);
    this.name = "RelayAuthRejectError";
    this.op = op;
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

/// EN: Map `index_ack` → `index_reject`. CN: `index_ack` → `index_reject`。
export function rejectTypeForAck(ackType: string): string | null {
  if (!ackType.endsWith("_ack")) return null;
  return `${ackType.slice(0, -4)}_reject`;
}

/// EN: Parse pointer LWW reject (`stale_updated_at`). CN: 解析指针 LWW 拒绝。
export function parseStalePointerReject(msg: unknown): RelayStalePointerError | null {
  if (!isRecord(msg) || typeof msg.type !== "string" || !msg.type.endsWith("_reject")) return null;
  if (msg.reason !== "stale_updated_at") return null;
  const updatedAt = typeof msg.updated_at === "number" ? msg.updated_at : Number(msg.updated_at);
  if (!Number.isFinite(updatedAt)) return null;
  return new RelayStalePointerError(msg.type, updatedAt);
}

/// EN: Parse `inbox_reject{stale_epoch}`. CN: 解析 `inbox_reject{stale_epoch}`。
export function parseInboxStaleEpochReject(msg: unknown): RelayInboxStaleEpochError | null {
  if (!isRecord(msg) || msg.type !== "inbox_reject" || msg.reason !== "stale_epoch") return null;
  const epoch = typeof msg.epoch === "number" ? msg.epoch : Number(msg.epoch);
  if (!Number.isFinite(epoch)) return null;
  return new RelayInboxStaleEpochError(epoch);
}

/// EN: Turn a wire message into a typed relay error, if applicable. CN: 将 wire 消息转为类型化 relay 错误。
export function relayErrorFromWire(msg: unknown, expectedAck?: string): Error | null {
  if (!isRecord(msg) || typeof msg.type !== "string") return null;
  if (msg.type === "auth_reject") {
    return new RelayAuthRejectError(typeof msg.op === "string" ? msg.op : "write");
  }
  const inbox = parseInboxStaleEpochReject(msg);
  if (inbox) return inbox;
  const pointer = parseStalePointerReject(msg);
  if (pointer) {
    if (expectedAck) {
      const expectedReject = rejectTypeForAck(expectedAck);
      if (expectedReject && pointer.rejectType !== expectedReject) return null;
    }
    return pointer;
  }
  return null;
}

/// EN: User-facing hint for outbound `frame_reject`. CN: 出站 `frame_reject` 的用户提示。
export function frameRejectHint(reason: string): string {
  switch (reason) {
    case "rate_limited":
      return "发送过于频繁，部分消息未送达，请稍后重试";
    case "delivery_rejected":
      return "盲签投递未通过校验，消息未送达，请重试";
    default:
      return "消息未送达，请重试";
  }
}
