// EN: Relay control-plane exchange for contact add notifications (contact_req / contact_ack).
// CN: 联系人添加通知的 relay 控制面交换（contact_req / contact_ack）。

import { config } from "@/config";
import { consumeContactInbox, fetchContactInbox } from "@/relay/contactRequestInbox";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";
import { loadContacts } from "@/store/contactBook";
import {
  hasSeenReqId,
  type ContactRequest,
  upsertRequest,
  updateRequestStatus,
  peerCanon,
} from "@/store/contactRequests";

const INBOX_POLL_MS = 60_000;

export interface ContactRequestExchangeDeps {
  selfAddress: string;
  selfLabel: string;
  endpointId: string;
  relay: RelayClient;
  onChange: (rows: ContactRequest[]) => void;
  onAutoAccept?: (peerAddress: string, label: string) => Promise<void>;
  /** EN: (Re)start 1:1 MLS handshake when a contact relationship is confirmed. CN: 联系人关系确认后（重）启 1:1 MLS 握手。 */
  onEnsureHandshake?: (peerAddress: string) => void;
}

export class ContactRequestExchange {
  private onVisible: (() => void) | null = null;

  constructor(private deps: ContactRequestExchangeDeps) {}

  wire(): void {
    this.deps.relay.onControl((m) => void this.handle(m));
    void this.syncInbox();

    if (!config.relayWs || typeof document === "undefined") return;

    window.setInterval(() => void this.syncInbox(), INBOX_POLL_MS);
    this.onVisible = () => {
      if (document.visibilityState === "visible") void this.syncInbox();
    };
    document.addEventListener("visibilitychange", this.onVisible);
  }

  /// EN: Fetch relay mailbox (offline contact_req/ack) and process. CN: 拉取 relay 邮箱中的离线请求并处理。
  async syncInbox(): Promise<void> {
    if (!config.relayWs) return;
    try {
      const { reqs, acks } = await fetchContactInbox(this.deps.selfAddress);
      const consumedReqs: string[] = [];
      const consumedAcks: string[] = [];
      for (const m of reqs) {
        await this.onInboundReq(m);
        consumedReqs.push(m.reqId);
      }
      for (const m of acks) {
        await this.onInboundAck(m);
        consumedAcks.push(m.reqId);
      }
      if (consumedReqs.length > 0 || consumedAcks.length > 0) {
        await consumeContactInbox(this.deps.selfAddress, consumedReqs, consumedAcks);
      }
    } catch (e) {
      console.warn("[nexchat] contact inbox sync failed:", e);
    }
  }

  /// EN: Notify peer after local add. CN: 本地添加后通知对端。
  async sendRequest(peerAddress: string): Promise<string | null> {
    const peer = peerCanon(peerAddress);
    if (peer === peerCanon(this.deps.selfAddress)) return null;
    if (loadContacts(this.deps.selfAddress).some((c) => c.address === peer)) {
      // EN: re-label update — no new notification. CN: 仅改备注，不重复通知。
      return null;
    }

    const reqId = globalThis.crypto?.randomUUID?.() ?? `cr-${Date.now()}-${Math.random()}`;
    const sentAt = Date.now();
    const rows = upsertRequest(this.deps.selfAddress, {
      reqId,
      peerAddress: peer,
      fromLabel: this.deps.selfLabel,
      direction: "outbound",
      status: "pending",
      sentAt,
      updatedAt: sentAt,
    });
    this.deps.onChange(rows);

    await this.deps.relay.sendControl({
      t: "contact_req",
      from: this.deps.endpointId,
      fromAddr: this.deps.selfAddress,
      toAddr: peer,
      reqId,
      fromLabel: this.deps.selfLabel,
      sentAt,
    });
    return reqId;
  }

  async sendAck(
    peerAddress: string,
    reqId: string,
    action: "accept" | "reject",
    label?: string,
  ): Promise<void> {
    const peer = peerCanon(peerAddress);
    await this.deps.relay.sendControl({
      t: "contact_ack",
      from: this.deps.endpointId,
      fromAddr: this.deps.selfAddress,
      toAddr: peer,
      reqId,
      action,
      label: action === "accept" ? label : undefined,
    });
  }

  private isForSelf(toAddr: string): boolean {
    return peerCanon(toAddr) === peerCanon(this.deps.selfAddress);
  }

  private async handle(m: ControlMsg): Promise<void> {
    if (m.t === "contact_req" && this.isForSelf(m.toAddr)) {
      await this.onInboundReq(m);
      return;
    }
    if (m.t === "contact_ack" && this.isForSelf(m.toAddr)) {
      await this.onInboundAck(m);
    }
  }

  private async onInboundReq(m: Extract<ControlMsg, { t: "contact_req" }>): Promise<void> {
    const peer = peerCanon(m.fromAddr);
    if (peer === peerCanon(this.deps.selfAddress)) return;
    if (hasSeenReqId(this.deps.selfAddress, m.reqId)) return;

    const already = loadContacts(this.deps.selfAddress).some((c) => c.address === peer);
    if (already) {
      const label = loadContacts(this.deps.selfAddress).find((c) => c.address === peer)?.label ?? m.fromLabel;
      upsertRequest(this.deps.selfAddress, {
        reqId: m.reqId,
        peerAddress: peer,
        fromLabel: m.fromLabel,
        direction: "inbound",
        status: "accepted",
        sentAt: m.sentAt,
        updatedAt: Date.now(),
      });
      await this.sendAck(peer, m.reqId, "accept", label);
      this.deps.onEnsureHandshake?.(peer);
      return;
    }

    const rows = upsertRequest(this.deps.selfAddress, {
      reqId: m.reqId,
      peerAddress: peer,
      fromLabel: m.fromLabel,
      direction: "inbound",
      status: "pending",
      sentAt: m.sentAt,
      updatedAt: m.sentAt,
    });
    this.deps.onChange(rows);
  }

  private async onInboundAck(m: Extract<ControlMsg, { t: "contact_ack" }>): Promise<void> {
    const peer = peerCanon(m.fromAddr);
    const rows = updateRequestStatus(this.deps.selfAddress, m.reqId, m.action === "accept" ? "accepted" : "rejected");
    this.deps.onChange(rows);

    if (m.action === "accept" && m.label && this.deps.onAutoAccept) {
      const existing = loadContacts(this.deps.selfAddress).some((c) => c.address === peer);
      if (!existing) {
        await this.deps.onAutoAccept(peer, m.label);
      } else {
        this.deps.onEnsureHandshake?.(peer);
      }
    }
  }
}
