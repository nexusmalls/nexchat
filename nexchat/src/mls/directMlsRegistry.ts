// EN: Registry of active 1:1 MLS handshakes; fans in relay control messages.
// CN: 活跃 1:1 MLS 握手注册表；汇聚 relay 控制消息。

import type { ChainClient } from "@/chain/chainClient";
import { directMlsKey, directMlsKeyInvolves, directHandshakeOwner } from "@/mls/directConv";
import {
  DirectMlsCoordinator,
  type DirectMlsStatus,
} from "@/mls/directHandshake";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import type { ControlInbound, RelayClient } from "@/relay/relayClient";
import { canonicalAddress } from "@/wallet/address";

export interface DirectRegistryDeps {
  engine: OpenMlsEngine;
  relay: RelayClient;
  endpointId: string;
  selfAddress: string;
  chain?: Pick<ChainClient, "keyPackagesOf">;
  onPeerStatus: (peer: string, status: DirectMlsStatus) => void;
}

export class DirectMlsRegistry {
  private coords = new Map<string, DirectMlsCoordinator>();
  private statuses = new Map<string, DirectMlsStatus>();
  private wired = false;
  // EN: 1:1 Wire multi-leaf (graft-only ownership): convs whose group is established/owned by the
  // sibling-graft path (`DirectWireSession`), NOT by this registry's pairwise handshake. The registry
  // must NOT initiate or apply control for them — doing so would fork the existing multi-leaf group.
  // Empty unless the Wire engine populates it via `markGraftManaged` → default 1:1 path is unchanged.
  // CN: 1:1 Wire 多 leaf（graft-only 所有权）：其群由兄弟嫁接路径（`DirectWireSession`）建立/拥有、而非
  // 本 registry 1:1 握手的会话。registry 对其**不发起、不应用控制面**——否则会分叉已有的多 leaf 群。
  // 仅当 Wire 引擎经 `markGraftManaged` 填充时非空 → 默认 1:1 路径不变。
  private graftManaged = new Set<string>();
  /// EN: Coordinators that have run `start()` at least once (graft placeholders stay unstarted).
  /// CN: 已至少执行过一次 `start()` 的协调器（嫁接占位保持未启动）。
  private started = new Set<string>();

  constructor(private deps: DirectRegistryDeps) {}

  wire(): void {
    if (this.wired) return;
    this.wired = true;
    const handler: ControlInbound = (m) => {
      if (m.t === "contact_req" || m.t === "contact_ack" || m.t === "group_invite") return;
      if (!m.convId.startsWith("d:") || !m.convId.slice(2).includes(":")) return;
      // EN: graft-owned conv → leave it entirely to the Wire session. CN: 嫁接拥有的会话 → 完全交给
      // Wire 会话。
      if (this.graftManaged.has(m.convId)) return;
      const coord = this.coords.get(m.convId);
      if (coord) {
        coord.handleControl(m);
        return;
      }
      if (directMlsKeyInvolves(m.convId, this.deps.selfAddress)) {
        const parts = m.convId.slice(2).split(":");
        const peer = parts.find((p) => p !== this.deps.selfAddress);
        if (peer) this.ensure(peer).handleControl(m);
      }
    };
    this.deps.relay.onControl(handler);
  }

  /// EN: Mark a pairwise conv as owned by the sibling-graft path (1:1 Wire multi-leaf). Idempotent;
  /// after this the registry will not initiate or apply control for it, and a non-started placeholder
  /// coordinator is kept only so `ensure`/`status` resolve consistently. CN: 把某 pairwise 会话标记为
  /// 兄弟嫁接路径拥有（1:1 Wire 多 leaf）。幂等；此后 registry 不再对其发起或应用控制面，仅保留一个未启动
  /// 的占位协调器以便 `ensure`/`status` 一致解析。
  markGraftManaged(convId: string): void {
    this.graftManaged.add(convId);
    // EN: Drop stale pairwise-coordinator status so `status()` / UI reflect graft readiness
    // (`hasGroup`) instead of a pre-graft `ready:false` cache. CN: 丢弃过期的成对协调器状态，使
    // `status()` / UI 反映嫁接就绪（`hasGroup`）而非嫁接前缓存的 `ready:false`。
    const parts = convId.slice(2).split(":");
    if (parts.length === 2) {
      for (const p of parts) {
        if (p !== this.deps.selfAddress) this.statuses.delete(canonicalAddress(p));
      }
    }
  }

  isGraftManaged(convId: string): boolean {
    return this.graftManaged.has(convId);
  }

  /// EN: Undo graft ownership when a graft never landed — allows pairwise cold-establish.
  /// CN: 嫁接未落地时撤销嫁接标记——允许成对握手冷启动。
  unmarkGraftManaged(convId: string): void {
    this.graftManaged.delete(convId);
  }

  ensure(peerAddress: string): DirectMlsCoordinator {
    // EN: Canonicalize at the boundary so a 42-form peer (e.g. from an echoed relay convId) and
    // a 273-form peer resolve to the same coordinator + status key. CN: 在边界归一，使 42 形态
    // （如 relay 回显的 convId）与 273 形态对端命中同一协调器与状态键。
    peerAddress = canonicalAddress(peerAddress);
    const key = directMlsKey(this.deps.selfAddress, peerAddress);
    let coord = this.coords.get(key);
    if (!coord) {
      coord = new DirectMlsCoordinator({
        engine: this.deps.engine,
        relay: this.deps.relay,
        endpointId: this.deps.endpointId,
        selfAddress: this.deps.selfAddress,
        peerAddress,
        chain: this.deps.chain,
        // EN: For contacts, let the owner bootstrap the group from the peer's on-chain
        // KeyPackage even when a relay WS is present — completes the handshake while the
        // peer is offline (Welcome waits in the relay 1:1 MLS mailbox). Enabled iff a chain
        // client is available (disabled in mock). CN: 对通讯录好友，即便有 relay WS 也允许
        // owner 用对端链上 KeyPackage 直接建群——对端离线也能完成握手（Welcome 暂存于 relay
        // 1:1 MLS 邮箱）。仅在有链客户端时启用（mock 下关闭）。
        chainKpFallback: this.deps.chain != null,
        onStatus: (s) => {
          this.statuses.set(peerAddress, s);
          this.deps.onPeerStatus(peerAddress, s);
        },
      });
      this.coords.set(key, coord);
    }
    if (!this.graftManaged.has(key) && !this.started.has(key)) {
      coord.start();
      this.started.add(key);
    }
    return coord;
  }

  status(peerAddress: string): DirectMlsStatus {
    peerAddress = canonicalAddress(peerAddress);
    const key = directMlsKey(this.deps.selfAddress, peerAddress);
    const role =
      directHandshakeOwner(this.deps.selfAddress, peerAddress) === this.deps.selfAddress
        ? "owner"
        : "member";
    if (this.graftManaged.has(key)) {
      return { ready: this.deps.engine.hasGroup(key), role };
    }
    const coord = this.coords.get(key);
    if (coord) {
      return { ready: coord.isReady(), role };
    }
    const cached = this.statuses.get(peerAddress);
    if (cached) return cached;
    return { ready: this.deps.engine.hasGroup(key), role };
  }

  isReady(peerAddress: string): boolean {
    const key = directMlsKey(this.deps.selfAddress, canonicalAddress(peerAddress));
    // EN: graft-owned conv → readiness is purely "is the grafted group present locally". CN: 嫁接拥有
    // 的会话 → 就绪与否仅取决于本地是否已有该嫁接群。
    if (this.graftManaged.has(key)) return this.deps.engine.hasGroup(key);
    return this.coords.get(key)?.isReady() ?? false;
  }

  /// EN: Recovery after corrupt/stale MLS state (owner resets group; member re-sends KeyPackage).
  /// CN: MLS 状态损坏后恢复（owner 重置群；成员重发 KeyPackage）。
  recoverPeer(peerAddress: string): void {
    peerAddress = canonicalAddress(peerAddress);
    // EN: never re-handshake a graft-owned conv (would fork the multi-leaf group). CN: 绝不对嫁接拥有
    // 的会话重握手（会分叉多 leaf 群）。
    if (this.graftManaged.has(directMlsKey(this.deps.selfAddress, peerAddress))) return;
    const coord = this.ensure(peerAddress);
    const isOwner =
      directHandshakeOwner(this.deps.selfAddress, peerAddress) === this.deps.selfAddress;
    if (isOwner) coord.recoverOwnerSession();
    else coord.recoverMemberSession();
  }

  /// EN: Retry pairwise handshakes after relay WS reconnect (skips graft-owned placeholders). CN: relay
  /// WS 重连后重试 1:1 握手（跳过嫁接占位）。
  onRelayConnected(): void {
    for (const [key, coord] of this.coords) {
      if (this.graftManaged.has(key)) continue;
      coord.onRelayConnected();
    }
  }
}
