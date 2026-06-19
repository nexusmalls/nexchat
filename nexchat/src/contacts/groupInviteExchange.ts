// EN: Relay control-plane for group invite notifications (group_invite).
// CN: 群邀请通知的 relay 控制面（group_invite）。

import { config } from "@/config";
import { ensureChainKeyPackagePublished } from "@/mls/chainKeyPackage";
import { ensureGroupMlsReady } from "@/mls/joinGroupMlsFlow";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { ChainClient } from "@/chain/chainClient";
import {
  consumeGroupInviteInbox,
  fetchGroupInviteInbox,
  type GroupInviteMsg,
} from "@/relay/groupInviteInbox";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";
import { canonicalAddress } from "@/wallet/address";

export interface GroupInviteRow {
  inviteId: string;
  groupId: number;
  groupName: string;
  fromAddr: string;
  fromLabel: string;
  sentAt: number;
}

export interface GroupInviteExchangeDeps {
  selfAddress: string;
  selfLabel: string;
  endpointId: string;
  relay: RelayClient;
  engine: OpenMlsEngine;
  chain: ChainClient;
  onChange: (rows: GroupInviteRow[]) => void;
  onSynced?: () => void;
}

export class GroupInviteExchange {
  private invites = new Map<string, GroupInviteRow>();

  constructor(private deps: GroupInviteExchangeDeps) {}

  wire(): void {
    this.deps.relay.onControl((m) => void this.handle(m));
    void this.syncInbox();
  }

  rows(): GroupInviteRow[] {
    return [...this.invites.values()].sort((a, b) => b.sentAt - a.sentAt);
  }

  async syncInbox(): Promise<void> {
    if (!config.relayWs) return;
    try {
      const msgs = await fetchGroupInviteInbox(this.deps.selfAddress);
      const consumed: string[] = [];
      for (const m of msgs) {
        await this.onInbound(m);
        consumed.push(m.inviteId);
      }
      if (consumed.length > 0) {
        await consumeGroupInviteInbox(this.deps.selfAddress, consumed);
      }
    } catch (e) {
      console.warn("[nexchat] group invite inbox sync failed:", e);
    }
  }

  async sendInvites(groupId: number, groupName: string, members: string[]): Promise<void> {
    for (const raw of members) {
      const toAddr = canonicalAddress(raw);
      if (!toAddr || toAddr === this.deps.selfAddress) continue;
      const inviteId = globalThis.crypto?.randomUUID?.() ?? `gi-${Date.now()}-${Math.random()}`;
      await this.deps.relay.sendControl({
        t: "group_invite",
        from: this.deps.endpointId,
        fromAddr: this.deps.selfAddress,
        toAddr,
        inviteId,
        groupId,
        groupName,
        fromLabel: this.deps.selfLabel,
        sentAt: Date.now(),
      });
    }
  }

  dismiss(inviteId: string): void {
    this.invites.delete(inviteId);
    this.deps.onChange(this.rows());
  }

  private async handle(msg: ControlMsg): Promise<void> {
    if (msg.t !== "group_invite") return;
    if (canonicalAddress(msg.toAddr) !== canonicalAddress(this.deps.selfAddress)) return;
    await this.onInbound(msg);
  }

  private async onInbound(msg: GroupInviteMsg): Promise<void> {
    const row: GroupInviteRow = {
      inviteId: msg.inviteId,
      groupId: msg.groupId,
      groupName: msg.groupName,
      fromAddr: canonicalAddress(msg.fromAddr),
      fromLabel: msg.fromLabel,
      sentAt: msg.sentAt,
    };
    this.invites.set(row.inviteId, row);
    this.deps.onChange(this.rows());

    try {
      await ensureChainKeyPackagePublished(
        this.deps.engine,
        this.deps.chain,
        this.deps.selfAddress,
      );
      await ensureGroupMlsReady({
        engine: this.deps.engine,
        chain: this.deps.chain,
        selfAddress: this.deps.selfAddress,
        groupId: row.groupId,
      });
      this.deps.onSynced?.();
    } catch (e) {
      console.warn("[nexchat] group invite MLS sync failed:", row.groupId, e);
    }
  }
}
