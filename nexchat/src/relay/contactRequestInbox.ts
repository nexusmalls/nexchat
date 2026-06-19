// EN: Relay mailbox for offline contact_req / contact_ack (fetch on unlock + consume).
// CN: relay 邮箱持久化离线 contact_req / contact_ack（解锁拉取 + 确认消费）。

import { config } from "@/config";
import { relayOneShotFetch, relayOneShotSend } from "@/relay/relayOneShot";
import type { ControlMsg } from "@/relay/relayClient";

export interface ContactInboxPayload {
  reqs: Extract<ControlMsg, { t: "contact_req" }>[];
  acks: Extract<ControlMsg, { t: "contact_ack" }>[];
}

function isContactReq(m: ControlMsg): m is Extract<ControlMsg, { t: "contact_req" }> {
  return m.t === "contact_req";
}

function isContactAck(m: ControlMsg): m is Extract<ControlMsg, { t: "contact_ack" }> {
  return m.t === "contact_ack";
}

function parseContactReply(m: Record<string, unknown>, requestId: string): ContactInboxPayload | undefined {
  if (m.type !== "contact_reply" || m.request_id !== requestId) return undefined;
  const reqs = ((m.reqs as ControlMsg[] | undefined) ?? []).filter(isContactReq);
  const acks = ((m.acks as ControlMsg[] | undefined) ?? []).filter(isContactAck);
  return { reqs, acks };
}

/// EN: Pull pending contact_req/ack stored for `account` on the relay. CN: 从 relay 拉取账户待投递联系人控制消息。
export async function fetchContactInbox(account: string): Promise<ContactInboxPayload> {
  const empty: ContactInboxPayload = { reqs: [], acks: [] };
  if (!config.relayWs) return empty;
  const raw = await relayOneShotFetch(account, { type: "contact_fetch" }, parseContactReply, 5000);
  return raw ?? empty;
}

/// EN: Acknowledge delivery so relay drops consumed mailbox entries. CN: 确认已处理，relay 删除邮箱条目。
export async function consumeContactInbox(
  account: string,
  reqIds: readonly string[],
  ackIds: readonly string[],
): Promise<void> {
  if (!config.relayWs || (reqIds.length === 0 && ackIds.length === 0)) return;
  await relayOneShotSend(
    account,
    {
      type: "contact_consume",
      account,
      req_ids: [...reqIds],
      ack_ids: [...ackIds],
    },
    { noReply: true },
  );
}
