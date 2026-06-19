// EN: DrSessionAdapter — binds the orchestrator's `DrSessionPort` to the real DR stack
// (`MultiDeviceRouter` over a `DrTransport`). It owns ONLY lifecycle bookkeeping (which peers
// are active / frozen) and gates outbound sends; it never reads ratchet/key state, honouring
// the §2.3 invariant. Lives in `orchestrator/` (not `crypto-dr/`) so the DR engine stays
// import-clean while this adapter may depend on it. CN: DrSessionAdapter —— 把编排器的
// `DrSessionPort` 绑定到真实 DR 栈（`DrTransport` 之上的 `MultiDeviceRouter`）。它**只**持有
// 生命周期账本（哪些对端活跃/冻结）并门控出站发送；绝不读棘轮/密钥态，遵守 §2.3 不变量。
// 置于 `orchestrator/`（非 `crypto-dr/`），使 DR 引擎保持 import 纯净而本适配器可依赖它。

import type { MultiDeviceRouter, MultiDeviceSendResult } from "@/crypto-dr/multiDevice";
import type { DrSessionPort } from "@/orchestrator/ports";
import { canonicalAddress } from "@/wallet/address";

export class DrSessionAdapter implements DrSessionPort {
  private readonly active = new Set<string>();
  private readonly frozen = new Set<string>();

  constructor(private readonly router: MultiDeviceRouter) {}

  private key(peer: string): string {
    try {
      return canonicalAddress(peer);
    } catch {
      return peer;
    }
  }

  async open(peer: string): Promise<void> {
    const k = this.key(peer);
    this.active.add(k);
    this.frozen.delete(k);
  }

  async freeze(peer: string): Promise<void> {
    this.frozen.add(this.key(peer));
  }

  async resume(peer: string): Promise<void> {
    this.frozen.delete(this.key(peer));
  }

  async retire(peer: string): Promise<void> {
    const k = this.key(peer);
    this.active.delete(k);
    this.frozen.delete(k);
  }

  isActive(peer: string): boolean {
    const k = this.key(peer);
    return this.active.has(k) && !this.frozen.has(k);
  }

  /// EN: Fan out `plaintext` to all of `peer`'s devices over the DR sessions, but only while
  /// the conversation is DR-active (not frozen/retired). Throws otherwise so a mid-switch
  /// send cannot leak onto a stack the orchestrator has frozen. CN: 仅在会话 DR 活跃（未冻结/
  /// 退役）时把 `plaintext` 经 DR 会话扇出给 `peer` 的所有设备；否则抛错，使切换中途的发送不会
  /// 落到编排器已冻结的栈上。
  async send(peer: string, plaintext: Uint8Array): Promise<MultiDeviceSendResult> {
    if (!this.isActive(peer)) {
      throw new Error("DrSessionAdapter: DR not active for peer (frozen/retired)");
    }
    return this.router.sendToAccount(peer, plaintext);
  }
}
