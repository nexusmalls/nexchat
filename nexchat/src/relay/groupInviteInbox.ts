// EN: Relay mailbox for offline group_invite (fetch on unlock + consume).
// CN: relay 邮箱持久化离线 group_invite（解锁拉取 + 确认消费）。

import { config } from "@/config";
import { relayOneShotFetch, relayOneShotSend } from "@/relay/relayOneShot";
import type { ControlMsg } from "@/relay/relayClient";

export type GroupInviteMsg = Extract<ControlMsg, { t: "group_invite" }>;

function isGroupInvite(m: ControlMsg): m is GroupInviteMsg {
  return m.t === "group_invite";
}

function parseGroupInviteReply(m: Record<string, unknown>, requestId: string): GroupInviteMsg[] | undefined {
  if (m.type !== "group_invite_reply" || m.request_id !== requestId) return undefined;
  return ((m.invites as ControlMsg[] | undefined) ?? []).filter(isGroupInvite);
}

export async function fetchGroupInviteInbox(account: string): Promise<GroupInviteMsg[]> {
  if (!config.relayWs) return [];
  const raw = await relayOneShotFetch(account, { type: "group_invite_fetch" }, parseGroupInviteReply, 5000);
  return raw ?? [];
}

export async function consumeGroupInviteInbox(
  account: string,
  inviteIds: readonly string[],
): Promise<void> {
  if (!config.relayWs || inviteIds.length === 0) return;
  await relayOneShotSend(
    account,
    {
      type: "group_invite_consume",
      account,
      invite_ids: [...inviteIds],
    },
    { noReply: true },
  );
}
