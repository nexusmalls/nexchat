// EN: Unified account-scoped Commit coordinator for when BOTH 1:1 Wire (`wireMultileafEnabled`) and
// group Wire (`wireGroupMultileafEnabled`) are on. A single `DirectAccountCommitCoordinator` owns ONE CD
// election + ONE presence stream on `s:<account>`, merges device-join offers (`d:` + `g:` convs in one
// offer), routes `commit_intent` by conv prefix (`d:` → relay CAS executor, `g:` → chain driver), and
// fans device-join KP handling to both session bridges.
//
// CN: 当 1:1 Wire（`wireMultileafEnabled`）与群 Wire（`wireGroupMultileafEnabled`）**同时**开启时的统一账户级
// Commit 协调器。单个 `DirectAccountCommitCoordinator` 拥有**一次** CD 选举 + **一条** `s:<account>` presence，
// 合并 device-join offer（一次 offer 含 `d:` + `g:` 会话），按 conv 前缀路由 `commit_intent`（`d:` → relay CAS
// 执行器，`g:` → 链驱动），并把 device-join KP 处理扇出到两个会话桥。

import {
  DirectAccountCommitCoordinator,
  type WireCommitExecutor,
} from "@/mls/directAccountCommitCoordinator";
import type {
  CommitIntentControlMsg,
  DeviceJoinKpControlMsg,
  DeviceJoinOfferControlMsg,
  DeviceJoinRequestControlMsg,
} from "@/mls/directCommitCoordination";
import type { RelayClient } from "@/relay/relayClient";

/// EN: Join-phase hooks exposed by a Wire session for unified CD fan-out. CN: Wire 会话暴露给统一 CD 扇出
/// 的 join 阶段钩子。
export interface WireSessionJoinBridge {
  listJoinableConvs(): string[];
  handleDeviceJoinOffer(msg: DeviceJoinOfferControlMsg): Promise<void>;
  handleDeviceJoinKp(msg: DeviceJoinKpControlMsg): Promise<void>;
}

export interface UnifiedWireAccountCoordinatorDeps {
  relay: RelayClient;
  account: string;
  deviceId: string;
  endpointId: string;
  /** EN: 1:1 relay-CAS executor (installed on the shared coordinator). CN: 1:1 relay-CAS 执行器（装在共享协调器上）。 */
  directExecutor: WireCommitExecutor;
  /** EN: Lazy bridge to the live 1:1 session (appStore wires after construction). CN: 延迟取 1:1 会话桥
   *  （appStore 构造后接线）。 */
  getDirectBridge: () => WireSessionJoinBridge | null;
  /** EN: Lazy bridge to the live group session. CN: 延迟取群会话桥。 */
  getGroupBridge: () => WireSessionJoinBridge | null;
  /** EN: Group-side intent execution (chain ordering driver). CN: 群侧意图执行（链定序驱动）。 */
  onGroupExecuteIntent: (intent: CommitIntentControlMsg) => Promise<void>;
}

/// EN: Build ONE shared coordinator for dual Wire mode. CN: 为双 Wire 模式构造**一个**共享协调器。
export function createUnifiedWireAccountCoordinator(
  deps: UnifiedWireAccountCoordinatorDeps,
): DirectAccountCommitCoordinator {
  let coordinator!: DirectAccountCommitCoordinator;
  coordinator = new DirectAccountCommitCoordinator({
    relay: deps.relay,
    account: deps.account,
    deviceId: deps.deviceId,
    endpointId: deps.endpointId,
    executor: deps.directExecutor,
    shouldUseRelayExecutor: (intent) => intent.payload.dmConvId.startsWith("d:"),
    onExecuteIntent: (intent) => {
      if (intent.payload.dmConvId.startsWith("g:")) {
        void deps.onGroupExecuteIntent(intent);
      }
    },
    onDeviceJoinRequest: async (msg: DeviceJoinRequestControlMsg) => {
      const direct = deps.getDirectBridge()?.listJoinableConvs() ?? [];
      const group = deps.getGroupBridge()?.listJoinableConvs() ?? [];
      const convIds = [...direct, ...group];
      if (convIds.length === 0) return;
      await coordinator.sendDeviceJoinOffer(msg.device_id, convIds);
    },
    onDeviceJoinOffer: async (msg: DeviceJoinOfferControlMsg) => {
      await Promise.all([
        deps.getDirectBridge()?.handleDeviceJoinOffer(msg) ?? Promise.resolve(),
        deps.getGroupBridge()?.handleDeviceJoinOffer(msg) ?? Promise.resolve(),
      ]);
    },
    onDeviceJoinKp: async (msg: DeviceJoinKpControlMsg) => {
      await Promise.all([
        deps.getDirectBridge()?.handleDeviceJoinKp(msg) ?? Promise.resolve(),
        deps.getGroupBridge()?.handleDeviceJoinKp(msg) ?? Promise.resolve(),
      ]);
    },
  });
  return coordinator;
}
