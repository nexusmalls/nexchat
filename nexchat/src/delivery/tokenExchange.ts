// EN: Relay control-plane exchange for blind token issuance (token_req / token_sig).
// CN: 盲签令牌签发的 relay 控制面交换。

import { bytesToB64, b64ToBytes } from "@/delivery/b64";
import { deriveContactTag } from "@/delivery/tokenMessage";
import type { InboxManager } from "@/delivery/inboxManager";
import type { TokenWallet } from "@/delivery/tokenWallet";
import type { ControlInbound, ControlMsg, RelayClient } from "@/relay/relayClient";

export interface TokenExchangeDeps {
  selfAddress: string;
  endpointId: string;
  inbox: InboxManager;
  wallet: TokenWallet;
  relay: RelayClient;
  onNeedTokens: (peer: string, mlsKey: string) => Promise<void>;
}

export class TokenExchange {
  constructor(private deps: TokenExchangeDeps) {}

  wire(): void {
    this.deps.relay.onControl((m) => void this.handle(m));
  }

  private async handle(m: ControlMsg): Promise<void> {
    try {
      await this.handleInner(m);
    } catch (e) {
      console.warn("[nexchat] token exchange failed:", e);
    }
  }

  private async handleInner(m: ControlMsg): Promise<void> {
    if (m.t === "token_req" && m.toAddr === this.deps.selfAddress) {
      await this.deps.inbox.ensure(this.deps.selfAddress);
      const rec = this.deps.inbox.get();
      if (!rec) return;
      const ct = await deriveContactTag(this.deps.selfAddress, m.fromAddr);
      const blinds = m.blinds.map((b) => b64ToBytes(b));
      const sigs = await this.deps.inbox.signBlinds(m.fromAddr, blinds);
      const ipk = this.deps.inbox.ipkPayload();
      await this.deps.relay.sendControl({
        t: "token_sig",
        from: this.deps.endpointId,
        issuer: this.deps.selfAddress,
        toAddr: m.fromAddr,
        convId: m.convId,
        inboxId: ipk.inboxId,
        epoch: ipk.epoch,
        ipkN: ipk.ipkN,
        ipkE: ipk.ipkE,
        ct: bytesToB64(ct),
        sigs: sigs.map(bytesToB64),
      });
      return;
    }
    if (m.t === "token_sig" && m.toAddr === this.deps.selfAddress) {
      await this.deps.wallet.absorbTokenSig(m);
    }
  }
}

export type TokenControlHandler = ControlInbound;
