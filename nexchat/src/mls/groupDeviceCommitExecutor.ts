// EN: Group Wire-ification device executor (CHAT_GROUP_WIREIFY_DESIGN §15.3). It is the group-side
// twin of `createAddDeviceExecutor` (1:1 Wire, `directWireCommitExecutor.ts`): it runs the OpenMLS
// `add_device` / `remove_device` / `rekey` op for a GROUP conv (`g:<id>`) as a STAGED commit (no
// merge), so a lost ordering race can be discarded cleanly. The ONLY structural difference from 1:1
// is the ordering backend: 1:1 uses the relay's off-chain `commit_slot` CAS, whereas a group uses the
// chain's `expected_epoch` total order (`pallet-chat-group::commit`). The merge therefore happens via
// `commitAccepted` only AFTER the chain `commit` succeeds; on `EpochStale` the driver calls
// `catchUpAndRerun`, which pulls the winning chain commits (`syncGroupEpoch`) and re-stages.
//
// CN: 群 Wire 化设备执行器（设计 §15.3）。它是 `createAddDeviceExecutor`（1:1 Wire，
// `directWireCommitExecutor.ts`）的群侧孪生：对**群** conv（`g:<id>`）把 OpenMLS 的 `add_device` /
// `remove_device` / `rekey` 跑成 STAGED commit（不合并），使落败的定序竞争可被干净丢弃。与 1:1 的**唯一**
// 结构差异是定序后端：1:1 用 relay 链下 `commit_slot` CAS，群用链上 `expected_epoch` 全序
// （`pallet-chat-group::commit`）。故合并仅在链上 `commit` 成功**之后**经 `commitAccepted` 发生；
// `EpochStale` 时驱动调用 `catchUpAndRerun`，由其拉取胜出链上 commit（`syncGroupEpoch`）并重新暂存。

import type { CommitIntentControlMsg } from "@/mls/directCommitCoordination";
import type { WireCommitExecutor } from "@/mls/directAccountCommitCoordinator";
import type { WireExecutorEngine } from "@/mls/directWireCommitExecutor";
import { b64ToBytes, bytesToB64, type RelayClient } from "@/relay/relayClient";

/// EN: Dependencies for the group device executor. CN: 群设备执行器依赖。
export interface GroupDeviceExecutorDeps {
  /** EN: Staged-commit OpenMLS surface (a subset of `OpenMlsEngine`; same as 1:1). CN: staged-commit
   *  OpenMLS 接口（`OpenMlsEngine` 子集；同 1:1）。 */
  engine: WireExecutorEngine;
  /** EN: Relay used ONLY to deliver the device Welcome over `s:<account>` (NOT for ordering). CN:
   *  relay 仅用于经 `s:<account>` 投递设备 Welcome（**不**用于定序）。 */
  relay: Pick<RelayClient, "sendControl">;
  /** EN: This device's relay endpoint id. CN: 本设备 relay endpoint id。 */
  endpointId: string;
  /** EN: This device's account (canonical). CN: 本设备账户（规范形态）。 */
  selfAddress: string;
  /** EN: Pull + apply the winning chain commits so the local group reaches at least `toEpoch`, used by
   *  `catchUpAndRerun` after an `EpochStale` rejection. Replaces 1:1's relay-backlog `requestCatchUp`
   *  (the chain IS the commit log). CN: 拉取并应用胜出的链上 commit，使本地群至少到 `toEpoch`，供
   *  `EpochStale` 落败后的 `catchUpAndRerun` 使用。取代 1:1 的 relay backlog `requestCatchUp`（链即 commit 日志）。 */
  syncGroupEpoch?: (convId: string, toEpoch: number) => Promise<void>;
}

function assertGroupConv(deps: GroupDeviceExecutorDeps, intent: CommitIntentControlMsg): string {
  const convId = intent.payload.dmConvId;
  if (!convId || !convId.startsWith("g:")) {
    throw new Error(`GroupDeviceExecutor: expected a group conv (g:…), got ${convId}`);
  }
  if (!deps.engine.hasGroup(convId)) {
    throw new Error(`GroupDeviceExecutor: no local group for ${convId}`);
  }
  return convId;
}

function addDeviceKp(intent: CommitIntentControlMsg): Uint8Array {
  if (!intent.payload.kp) {
    throw new Error("GroupDeviceExecutor: add_device intent missing KeyPackage");
  }
  return b64ToBytes(intent.payload.kp);
}

/// EN: Build a group `WireCommitExecutor` for `add_device` / `remove_device` / `rekey` intents. Same
/// verbs as 1:1 so a chain-ordering driver can reuse the lifecycle; ordering is the chain epoch.
/// CN: 为 `add_device` / `remove_device` / `rekey` 意图构造群 `WireCommitExecutor`。动词与 1:1 一致，
/// 使链定序驱动可复用生命周期；定序用链上 epoch。
export function createGroupDeviceExecutor(deps: GroupDeviceExecutorDeps): WireCommitExecutor {
  // EN: Stage the op at the current local epoch (no merge). CN: 在当前本地 epoch 暂存（不合并）。
  function stage(
    intent: CommitIntentControlMsg,
    convId: string,
  ): { commitB64: string; welcomeB64: string } {
    if (intent.kind === "add_device") {
      const out = deps.engine.addMembersStagedByConv(convId, [addDeviceKp(intent)]);
      return { commitB64: bytesToB64(out.commit), welcomeB64: bytesToB64(out.welcome) };
    }
    if (intent.kind === "rekey") {
      const commit = deps.engine.selfUpdateStagedByConv(convId);
      return { commitB64: bytesToB64(commit), welcomeB64: "" };
    }
    if (intent.kind === "remove_device") {
      const target = intent.payload.target?.trim();
      if (!target) {
        throw new Error("GroupDeviceExecutor: remove_device intent missing target device identity");
      }
      const out = deps.engine.removeMembersStagedByConv(convId, [target]);
      return { commitB64: bytesToB64(out.commit), welcomeB64: bytesToB64(out.welcome) };
    }
    throw new Error(`GroupDeviceExecutor: unsupported intent kind ${intent.kind}`);
  }

  return {
    async runIntent(intent) {
      const convId = assertGroupConv(deps, intent);
      // EN: pre-op epoch = the chain `expected_epoch` for the upcoming `commit`. Staging does not
      // advance it. CN: 操作前 epoch = 即将提交 `commit` 的链上 `expected_epoch`。暂存不推进它。
      const preEpoch = deps.engine.epochByConv(convId);
      const out = stage(intent, convId);
      return { ...out, preEpoch };
    },

    async commitAccepted(intent) {
      // EN: called only AFTER the chain `commit` returns Ok. CN: 仅在链上 `commit` 返回 Ok 后调用。
      deps.engine.mergePendingByConv(intent.payload.dmConvId);
    },

    async commitAbandoned(intent) {
      // EN: discard the staged commit; clearing a non-pending group is a safe no-op. CN: 丢弃暂存
      // commit；对无 pending 的群清除是安全空操作。
      deps.engine.clearPendingByConv(intent.payload.dmConvId);
    },

    async catchUpAndRerun(intent, toEpoch) {
      const convId = assertGroupConv(deps, intent);
      // EN: discard our forked staged commit FIRST (always safe), then pull the winning chain commits
      // and re-stage at the caught-up epoch. CN: **先**丢弃我方分叉暂存 commit（永远安全），再拉取胜出链上
      // commit 并在追平后的 epoch 重新暂存。
      deps.engine.clearPendingByConv(convId);
      await deps.syncGroupEpoch?.(convId, toEpoch);
      const epoch = deps.engine.epochByConv(convId);
      if (epoch < toEpoch) {
        // EN: chain commits not yet applied locally → fail fast so the driver re-polls. CN: 链上 commit
        // 尚未本地应用 → 快速失败让驱动重轮询。
        throw new Error(
          `GroupDeviceExecutor: local group at epoch ${epoch} < ${toEpoch}; awaiting winning chain commit`,
        );
      }
      const out = stage(intent, convId);
      return { ...out, newEpoch: epoch };
    },

    async deliverWelcome(intent, welcomeB64) {
      // EN: rekey / member-less ops carry no Welcome. CN: rekey / 无新成员操作无 Welcome。
      if (!welcomeB64) return;
      // EN: the Welcome rides the group conv (`g:<id>`, so the receiver knows which group to join) and the
      // relay routes it to the joining device's ACCOUNT by `toAddr` (mailbox-stored for offline catch-up,
      // mirroring the 1:1 `d:` welcome route — see relay-rs protocol.rs `mls_control_recipient` g: branch).
      // Default `toAddr` = my own account (sibling add). CN: Welcome 走群 conv（`g:<id>`，使接收方知道入哪个
      // 群），relay 按 `toAddr` 路由到**加入设备账户**（入信箱以便离线补齐，与 1:1 `d:` welcome 路由对齐——见
      // relay-rs protocol.rs `mls_control_recipient` g: 分支）。默认 `toAddr` = 我自己的账户（兄弟 add）。
      await deps.relay.sendControl({
        t: "welcome",
        from: deps.endpointId,
        to: "",
        toAddr: intent.payload.welcomeTo ?? deps.selfAddress,
        convId: intent.payload.dmConvId,
        welcome: welcomeB64,
      });
    },
  };
}
